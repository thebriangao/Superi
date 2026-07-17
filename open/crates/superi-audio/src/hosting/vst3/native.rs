//! Audited native VST3 ABI, module lifecycle, COM ownership, and process bridge.

#![allow(unsafe_code)]

use std::cell::UnsafeCell;
use std::collections::BTreeMap;
use std::ffi::{c_char, c_void, CString};
use std::mem::{self, MaybeUninit};
use std::path::Path;
#[cfg(any(target_os = "windows", target_os = "linux"))]
use std::path::PathBuf;
use std::ptr;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use superi_concurrency::threads::{current_execution_domain, ExecutionDomain};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use vst3::Steinberg::Vst::*;
use vst3::Steinberg::*;
use vst3::{Class, ComPtr, ComWrapper, Interface};

use super::{Vst3EffectConfig, Vst3PluginState, Vst3ProcessMode};
use crate::plugins::MAX_AUDIO_PLUGIN_STATE_BYTES;

const COMPONENT: &str = "superi-audio.hosting.vst3.native";
const MAXIMUM_PARAMETERS: usize = 4_096;
const MAXIMUM_PARAMETER_POINT_CELLS: usize = 1_048_576;
const AUDIO_EFFECT_CATEGORY: &str = "Audio Module Class";
const MAXIMUM_HOST_ATTRIBUTES: usize = 64;
const MAXIMUM_HOST_ATTRIBUTE_ID_BYTES: usize = 127;
const MAXIMUM_HOST_STRING_CODE_UNITS: usize = 4_096;
const MAXIMUM_HOST_BINARY_BYTES: usize = 1_048_576;
const MAXIMUM_HOST_BINARY_VERSIONS: usize = 256;
const MAXIMUM_HOST_MESSAGE_ID_BYTES: usize = 127;
const MAXIMUM_HOST_MESSAGE_ID_VERSIONS: usize = 64;
const MAXIMUM_HOST_MESSAGE_ID_TOTAL_BYTES: usize = 8_192;

#[cfg(target_os = "windows")]
const fn sample_32_code() -> i32 {
    SymbolicSampleSizes_::kSample32
}

#[cfg(not(target_os = "windows"))]
const fn sample_32_code() -> i32 {
    SymbolicSampleSizes_::kSample32 as i32
}

#[derive(Clone, Debug)]
pub(super) struct NativeParameterInfo {
    pub(super) id: u32,
    pub(super) title: String,
    pub(super) default_normalized_value: f64,
    pub(super) automatable: bool,
    pub(super) read_only: bool,
}

#[derive(Clone, Debug)]
pub(super) struct NativeMetadata {
    pub(super) factory_vendor: String,
    pub(super) component_name: String,
    pub(super) latency_samples: u32,
    pub(super) tail_samples: u32,
    pub(super) parameters: Vec<NativeParameterInfo>,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct AutomationPoint {
    pub(super) parameter_id: u32,
    pub(super) sample_offset: i32,
    pub(super) normalized_value: f64,
}

impl AutomationPoint {
    pub(super) const EMPTY: Self = Self {
        parameter_id: 0,
        sample_offset: 0,
        normalized_value: 0.0,
    };
}

#[derive(Clone, Copy, Debug)]
pub(super) struct OutputPoint {
    pub(super) parameter_id: u32,
    pub(super) sample_offset: i32,
    pub(super) normalized_value: f64,
}

pub(super) struct ProcessBlock<'a> {
    pub(super) sample_rate: u32,
    pub(super) start_sample: i64,
    pub(super) frame_count: usize,
    pub(super) channel_count: usize,
    pub(super) channel_stride: usize,
    pub(super) process_mode: i32,
    pub(super) input_silence_flags: u64,
    pub(super) input_planar: &'a mut [f32],
    pub(super) output_planar: &'a mut [f32],
    pub(super) automation: &'a [AutomationPoint],
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ProcessOutcome {
    pub(super) output_silence_flags: u64,
    pub(super) restart_flags: u64,
}

fn zeroed<T>() -> T {
    // SAFETY: This helper is used only for VST3 C ABI plain-old-data records whose all-zero bit
    // pattern is valid and whose required fields are assigned before a plugin reads them.
    unsafe { MaybeUninit::<T>::zeroed().assume_init() }
}

fn copy_utf16(value: &str, destination: &mut [TChar]) {
    destination.fill(0);
    let capacity = destination.len().saturating_sub(1);
    for (source, destination) in value
        .encode_utf16()
        .zip(destination.iter_mut().take(capacity))
    {
        *destination = source as TChar;
    }
}

fn utf16_string(value: &[TChar]) -> String {
    let length = value
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(value.len());
    String::from_utf16_lossy(&value[..length])
}

fn c_string(value: &[c_char]) -> String {
    let length = value
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(value.len());
    String::from_utf8_lossy(
        &value[..length]
            .iter()
            .map(|byte| *byte as u8)
            .collect::<Vec<_>>(),
    )
    .into_owned()
}

fn status(
    result: tresult,
    operation: &'static str,
    message: &'static str,
    category: ErrorCategory,
) -> Result<()> {
    if result == kResultOk {
        Ok(())
    } else {
        Err(
            Error::new(category, Recoverability::UserCorrectable, message).with_context(
                ErrorContext::new(COMPONENT, operation).with_field("tresult", result.to_string()),
            ),
        )
    }
}

fn optional_state_status(
    result: tresult,
    operation: &'static str,
    message: &'static str,
) -> Result<bool> {
    if result == kResultOk {
        Ok(true)
    } else if result == kNotImplemented || result == kResultFalse {
        Ok(false)
    } else {
        Err(Error::new(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            message,
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation).with_field("tresult", result.to_string()),
        ))
    }
}

fn unavailable(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::Unavailable,
        Recoverability::UserCorrectable,
        message.into(),
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn host_control_call_allowed() -> bool {
    current_execution_domain() != Some(ExecutionDomain::Audio)
}

struct MemoryStreamBuffer {
    bytes: Vec<u8>,
    cursor: usize,
}

struct MemoryStream {
    buffer: Mutex<MemoryStreamBuffer>,
}

impl MemoryStream {
    fn empty() -> Self {
        Self::from_bytes(Vec::new())
    }

    fn from_bytes(bytes: Vec<u8>) -> Self {
        Self {
            buffer: Mutex::new(MemoryStreamBuffer { bytes, cursor: 0 }),
        }
    }

    fn bytes(&self) -> Result<Vec<u8>> {
        self.buffer
            .lock()
            .map(|buffer| buffer.bytes.clone())
            .map_err(|_| unavailable("read_state_stream", "VST3 state stream lock was poisoned"))
    }
}

impl Class for MemoryStream {
    type Interfaces = (IBStream,);
}

impl IBStreamTrait for MemoryStream {
    unsafe fn read(
        &self,
        buffer: *mut c_void,
        num_bytes: i32,
        num_bytes_read: *mut i32,
    ) -> tresult {
        if num_bytes < 0 || (num_bytes != 0 && buffer.is_null()) {
            return kInvalidArgument;
        }
        let Ok(mut state) = self.buffer.lock() else {
            return kInternalError;
        };
        let requested = usize::try_from(num_bytes).unwrap_or(0);
        let start = state.cursor.min(state.bytes.len());
        let end = start.saturating_add(requested).min(state.bytes.len());
        let read = end - start;
        if read != 0 {
            // SAFETY: The plugin supplied at least num_bytes writable bytes and this copy uses no
            // more than that validated extent from retained stream storage.
            unsafe {
                ptr::copy_nonoverlapping(state.bytes[start..end].as_ptr(), buffer.cast(), read)
            };
        }
        state.cursor = end;
        if !num_bytes_read.is_null() {
            // SAFETY: VST3 supplied optional writable storage for one signed byte count.
            unsafe { num_bytes_read.write(i32::try_from(read).unwrap_or(i32::MAX)) };
        }
        if read == requested {
            kResultOk
        } else {
            kResultFalse
        }
    }

    unsafe fn write(
        &self,
        buffer: *mut c_void,
        num_bytes: i32,
        num_bytes_written: *mut i32,
    ) -> tresult {
        if num_bytes < 0 || (num_bytes != 0 && buffer.is_null()) {
            return kInvalidArgument;
        }
        let requested = usize::try_from(num_bytes).unwrap_or(0);
        let Ok(mut state) = self.buffer.lock() else {
            return kInternalError;
        };
        let Some(end) = state.cursor.checked_add(requested) else {
            return kOutOfMemory;
        };
        if end > MAX_AUDIO_PLUGIN_STATE_BYTES {
            return kOutOfMemory;
        }
        if end > state.bytes.len() {
            let additional = end - state.bytes.len();
            if state.bytes.try_reserve_exact(additional).is_err() {
                return kOutOfMemory;
            }
            state.bytes.resize(end, 0);
        }
        if requested != 0 {
            // SAFETY: The plugin supplied num_bytes readable bytes and the destination range is
            // fully allocated and uniquely locked for the synchronous copy.
            let source = unsafe { std::slice::from_raw_parts(buffer.cast::<u8>(), requested) };
            let start = state.cursor;
            state.bytes[start..end].copy_from_slice(source);
        }
        state.cursor = end;
        if !num_bytes_written.is_null() {
            // SAFETY: VST3 supplied optional writable storage for one signed byte count.
            unsafe { num_bytes_written.write(num_bytes) };
        }
        kResultOk
    }

    unsafe fn seek(&self, pos: i64, mode: i32, result: *mut i64) -> tresult {
        let Ok(mut state) = self.buffer.lock() else {
            return kInternalError;
        };
        let base = match mode {
            0 => 0_i64,
            1 => i64::try_from(state.cursor).unwrap_or(i64::MAX),
            2 => i64::try_from(state.bytes.len()).unwrap_or(i64::MAX),
            _ => return kInvalidArgument,
        };
        let Some(next) = base.checked_add(pos) else {
            return kInvalidArgument;
        };
        let Ok(next) = usize::try_from(next) else {
            return kInvalidArgument;
        };
        if next > MAX_AUDIO_PLUGIN_STATE_BYTES {
            return kOutOfMemory;
        }
        state.cursor = next;
        if !result.is_null() {
            // SAFETY: VST3 supplied optional writable storage for the resulting cursor.
            unsafe { result.write(i64::try_from(next).unwrap_or(i64::MAX)) };
        }
        kResultOk
    }

    unsafe fn tell(&self, pos: *mut i64) -> tresult {
        if pos.is_null() {
            return kInvalidArgument;
        }
        let Ok(state) = self.buffer.lock() else {
            return kInternalError;
        };
        // SAFETY: The nonnull pointer is writable storage supplied by the synchronous caller.
        unsafe { pos.write(i64::try_from(state.cursor).unwrap_or(i64::MAX)) };
        kResultOk
    }
}

unsafe fn tuid_matches(value: *const TUID, expected: &[u8; 16]) -> bool {
    if value.is_null() {
        return false;
    }
    for (index, expected) in expected.iter().copied().enumerate() {
        // SAFETY: VST3 supplies one readable 16-byte TUID for the synchronous interface call.
        if unsafe { *(value.cast::<u8>().add(index)) } != expected {
            return false;
        }
    }
    true
}

unsafe fn bounded_c_bytes(value: *const c_char, maximum: usize) -> Option<Vec<u8>> {
    if value.is_null() {
        return None;
    }
    let mut result = Vec::new();
    for index in 0..=maximum {
        // SAFETY: The VST3 ABI requires a readable null-terminated string. Reading is capped so a
        // malformed caller cannot drive an unbounded scan or allocation.
        let byte = unsafe { *value.add(index) } as u8;
        if byte == 0 {
            return Some(result);
        }
        result.push(byte);
    }
    None
}

unsafe fn bounded_utf16(value: *const TChar) -> Option<Vec<TChar>> {
    if value.is_null() {
        return None;
    }
    let mut result = Vec::new();
    for index in 0..=MAXIMUM_HOST_STRING_CODE_UNITS {
        // SAFETY: The VST3 ABI requires a readable null-terminated TChar string. The explicit cap
        // prevents an unbounded scan or allocation for malformed plugin input.
        let unit = unsafe { *value.add(index) };
        if unit == 0 {
            return Some(result);
        }
        result.push(unit);
    }
    None
}

#[derive(Clone)]
enum HostAttributeValue {
    Integer(i64),
    Float(f64),
    String(Vec<TChar>),
    Binary(Arc<[u8]>),
}

#[derive(Default)]
struct HostAttributeState {
    values: BTreeMap<Vec<u8>, HostAttributeValue>,
    retained_binary: Vec<Arc<[u8]>>,
    retained_binary_bytes: usize,
}

#[derive(Default)]
struct HostAttributeList {
    state: Mutex<HostAttributeState>,
}

impl HostAttributeList {
    fn set_value(&self, key: Vec<u8>, value: HostAttributeValue) -> tresult {
        let Ok(mut state) = self.state.lock() else {
            return kResultFalse;
        };
        if !state.values.contains_key(key.as_slice())
            && state.values.len() >= MAXIMUM_HOST_ATTRIBUTES
        {
            return kOutOfMemory;
        }
        state.values.insert(key, value);
        kResultOk
    }

    fn value(&self, key: &[u8]) -> Option<HostAttributeValue> {
        self.state
            .lock()
            .ok()
            .and_then(|state| state.values.get(key).cloned())
    }
}

impl Class for HostAttributeList {
    type Interfaces = (IAttributeList,);
}

impl IAttributeListTrait for HostAttributeList {
    unsafe fn setInt(&self, id: *const c_char, value: i64) -> tresult {
        if !host_control_call_allowed() {
            return kResultFalse;
        }
        // SAFETY: The plugin supplies one null-terminated AttrID for this synchronous control call.
        let Some(key) = (unsafe { bounded_c_bytes(id, MAXIMUM_HOST_ATTRIBUTE_ID_BYTES) }) else {
            return kInvalidArgument;
        };
        self.set_value(key, HostAttributeValue::Integer(value))
    }

    unsafe fn getInt(&self, id: *const c_char, value: *mut i64) -> tresult {
        if !host_control_call_allowed() || value.is_null() {
            return kResultFalse;
        }
        // SAFETY: The plugin supplies one null-terminated AttrID for this synchronous control call.
        let Some(key) = (unsafe { bounded_c_bytes(id, MAXIMUM_HOST_ATTRIBUTE_ID_BYTES) }) else {
            return kInvalidArgument;
        };
        let Some(HostAttributeValue::Integer(stored)) = self.value(&key) else {
            return kResultFalse;
        };
        // SAFETY: The plugin supplied writable storage for one i64 value.
        unsafe { value.write(stored) };
        kResultOk
    }

    unsafe fn setFloat(&self, id: *const c_char, value: f64) -> tresult {
        if !host_control_call_allowed() || !value.is_finite() {
            return kResultFalse;
        }
        // SAFETY: The plugin supplies one null-terminated AttrID for this synchronous control call.
        let Some(key) = (unsafe { bounded_c_bytes(id, MAXIMUM_HOST_ATTRIBUTE_ID_BYTES) }) else {
            return kInvalidArgument;
        };
        self.set_value(key, HostAttributeValue::Float(value))
    }

    unsafe fn getFloat(&self, id: *const c_char, value: *mut f64) -> tresult {
        if !host_control_call_allowed() || value.is_null() {
            return kResultFalse;
        }
        // SAFETY: The plugin supplies one null-terminated AttrID for this synchronous control call.
        let Some(key) = (unsafe { bounded_c_bytes(id, MAXIMUM_HOST_ATTRIBUTE_ID_BYTES) }) else {
            return kInvalidArgument;
        };
        let Some(HostAttributeValue::Float(stored)) = self.value(&key) else {
            return kResultFalse;
        };
        // SAFETY: The plugin supplied writable storage for one f64 value.
        unsafe { value.write(stored) };
        kResultOk
    }

    unsafe fn setString(&self, id: *const c_char, string: *const TChar) -> tresult {
        if !host_control_call_allowed() {
            return kResultFalse;
        }
        // SAFETY: The plugin supplies null-terminated key and value strings for this control call.
        let Some(key) = (unsafe { bounded_c_bytes(id, MAXIMUM_HOST_ATTRIBUTE_ID_BYTES) }) else {
            return kInvalidArgument;
        };
        // SAFETY: The same ABI contract supplies one readable TChar string.
        let Some(value) = (unsafe { bounded_utf16(string) }) else {
            return kInvalidArgument;
        };
        self.set_value(key, HostAttributeValue::String(value))
    }

    unsafe fn getString(
        &self,
        id: *const c_char,
        string: *mut TChar,
        size_in_bytes: u32,
    ) -> tresult {
        if !host_control_call_allowed() || string.is_null() {
            return kResultFalse;
        }
        let capacity = usize::try_from(size_in_bytes)
            .unwrap_or(usize::MAX)
            .checked_div(mem::size_of::<TChar>())
            .unwrap_or(0);
        if capacity == 0 {
            return kInvalidArgument;
        }
        // SAFETY: The plugin supplies one null-terminated AttrID for this synchronous control call.
        let Some(key) = (unsafe { bounded_c_bytes(id, MAXIMUM_HOST_ATTRIBUTE_ID_BYTES) }) else {
            return kInvalidArgument;
        };
        let Some(HostAttributeValue::String(stored)) = self.value(&key) else {
            return kResultFalse;
        };
        let copied = stored.len().min(capacity.saturating_sub(1));
        // SAFETY: The plugin declares size_in_bytes of writable TChar storage. copied and its null
        // terminator are bounded by that capacity.
        unsafe {
            ptr::copy_nonoverlapping(stored.as_ptr(), string, copied);
            string.add(copied).write(0);
        }
        kResultOk
    }

    unsafe fn setBinary(
        &self,
        id: *const c_char,
        data: *const c_void,
        size_in_bytes: u32,
    ) -> tresult {
        if !host_control_call_allowed() || (data.is_null() && size_in_bytes != 0) {
            return kResultFalse;
        }
        // SAFETY: The plugin supplies one null-terminated AttrID for this synchronous control call.
        let Some(key) = (unsafe { bounded_c_bytes(id, MAXIMUM_HOST_ATTRIBUTE_ID_BYTES) }) else {
            return kInvalidArgument;
        };
        let size = usize::try_from(size_in_bytes).unwrap_or(usize::MAX);
        if size > MAXIMUM_HOST_BINARY_BYTES {
            return kOutOfMemory;
        }
        let bytes: Arc<[u8]> = if size == 0 {
            Arc::from([])
        } else {
            // SAFETY: The plugin declares size readable bytes for this synchronous call.
            Arc::from(unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size) })
        };
        let Ok(mut state) = self.state.lock() else {
            return kResultFalse;
        };
        if (!state.values.contains_key(key.as_slice())
            && state.values.len() >= MAXIMUM_HOST_ATTRIBUTES)
            || state.retained_binary.len() >= MAXIMUM_HOST_BINARY_VERSIONS
            || state
                .retained_binary_bytes
                .checked_add(size)
                .map_or(true, |total| total > MAXIMUM_HOST_BINARY_BYTES)
        {
            return kOutOfMemory;
        }
        state.retained_binary_bytes += size;
        state.retained_binary.push(Arc::clone(&bytes));
        state.values.insert(key, HostAttributeValue::Binary(bytes));
        kResultOk
    }

    unsafe fn getBinary(
        &self,
        id: *const c_char,
        data: *mut *const c_void,
        size_in_bytes: *mut u32,
    ) -> tresult {
        if !host_control_call_allowed() || data.is_null() || size_in_bytes.is_null() {
            return kResultFalse;
        }
        // SAFETY: The plugin supplies one null-terminated AttrID for this synchronous control call.
        let Some(key) = (unsafe { bounded_c_bytes(id, MAXIMUM_HOST_ATTRIBUTE_ID_BYTES) }) else {
            return kInvalidArgument;
        };
        let Ok(state) = self.state.lock() else {
            return kResultFalse;
        };
        let Some(HostAttributeValue::Binary(stored)) = state.values.get(key.as_slice()) else {
            return kResultFalse;
        };
        // SAFETY: Both output pointers are writable. Every binary allocation is retained for the
        // attribute-list lifetime, so the returned data pointer survives later replacements.
        unsafe {
            data.write(stored.as_ptr().cast::<c_void>());
            size_in_bytes.write(u32::try_from(stored.len()).expect("host binary bound fits u32"));
        }
        kResultOk
    }
}

#[derive(Default)]
struct HostMessageState {
    identifiers: Vec<CString>,
    current: Option<usize>,
    retained_bytes: usize,
}

struct HostMessage {
    state: Mutex<HostMessageState>,
    attributes: ComWrapper<HostAttributeList>,
}

impl HostMessage {
    fn new() -> Self {
        Self {
            state: Mutex::new(HostMessageState::default()),
            attributes: ComWrapper::new(HostAttributeList::default()),
        }
    }
}

impl Class for HostMessage {
    type Interfaces = (IMessage,);
}

impl IMessageTrait for HostMessage {
    unsafe fn getMessageID(&self) -> FIDString {
        if !host_control_call_allowed() {
            return ptr::null();
        }
        let Ok(state) = self.state.lock() else {
            return ptr::null();
        };
        state
            .current
            .and_then(|index| state.identifiers.get(index))
            .map_or(ptr::null(), |identifier| identifier.as_ptr())
    }

    unsafe fn setMessageID(&self, id: FIDString) {
        if !host_control_call_allowed() {
            return;
        }
        // SAFETY: The plugin supplies one null-terminated message identifier for this control call.
        let Some(bytes) = (unsafe { bounded_c_bytes(id, MAXIMUM_HOST_MESSAGE_ID_BYTES) }) else {
            return;
        };
        let length = bytes.len();
        let Ok(identifier) = CString::new(bytes) else {
            return;
        };
        let Ok(mut state) = self.state.lock() else {
            return;
        };
        if state.identifiers.len() >= MAXIMUM_HOST_MESSAGE_ID_VERSIONS
            || state
                .retained_bytes
                .checked_add(length)
                .map_or(true, |total| total > MAXIMUM_HOST_MESSAGE_ID_TOTAL_BYTES)
        {
            return;
        }
        state.retained_bytes += length;
        state.identifiers.push(identifier);
        state.current = Some(state.identifiers.len() - 1);
    }

    unsafe fn getAttributes(&self) -> *mut IAttributeList {
        if !host_control_call_allowed() {
            return ptr::null_mut();
        }
        self.attributes
            .as_com_ref::<IAttributeList>()
            .map_or(ptr::null_mut(), |attributes| attributes.as_ptr())
    }
}

struct HostApplication;

impl Class for HostApplication {
    type Interfaces = (IHostApplication, IPlugInterfaceSupport);
}

impl IPlugInterfaceSupportTrait for HostApplication {
    unsafe fn isPlugInterfaceSupported(&self, iid: *const TUID) -> tresult {
        // SAFETY: The plugin supplies one readable TUID for this synchronous query.
        if unsafe { tuid_matches(iid, &IComponentHandler::IID) } {
            kResultTrue
        } else {
            kResultFalse
        }
    }
}

impl IHostApplicationTrait for HostApplication {
    unsafe fn getName(&self, name: *mut String128) -> tresult {
        if name.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: The VST3 caller supplied writable storage for one String128 for the duration of
        // this synchronous callback, as required by IHostApplication::getName.
        let name = unsafe { &mut *name };
        copy_utf16("Superi VST3 Worker", name);
        kResultOk
    }

    unsafe fn createInstance(
        &self,
        cid: *mut TUID,
        iid: *mut TUID,
        object: *mut *mut c_void,
    ) -> tresult {
        if object.is_null() || cid.is_null() || iid.is_null() {
            return kInvalidArgument;
        }
        // SAFETY: The VST3 caller supplied writable out-pointer storage. It remains null unless a
        // newly owned matching interface is transferred below.
        unsafe { object.write(ptr::null_mut()) };
        if !host_control_call_allowed() {
            return kResultFalse;
        }

        // SAFETY: cid and iid each point to one readable TUID for this synchronous call.
        if unsafe { tuid_matches(cid, &IMessage::IID) && tuid_matches(iid, &IMessage::IID) } {
            let message = ComWrapper::new(HostMessage::new())
                .to_com_ptr::<IMessage>()
                .expect("host message implements IMessage");
            // SAFETY: into_raw transfers the new owned COM reference to the plugin out-pointer.
            unsafe { object.write(message.into_raw().cast::<c_void>()) };
            return kResultOk;
        }
        // SAFETY: cid and iid each point to one readable TUID for this synchronous call.
        if unsafe {
            tuid_matches(cid, &IAttributeList::IID) && tuid_matches(iid, &IAttributeList::IID)
        } {
            let attributes = ComWrapper::new(HostAttributeList::default())
                .to_com_ptr::<IAttributeList>()
                .expect("host attribute list implements IAttributeList");
            // SAFETY: into_raw transfers the new owned COM reference to the plugin out-pointer.
            unsafe { object.write(attributes.into_raw().cast::<c_void>()) };
            return kResultOk;
        }
        kNoInterface
    }
}

struct ComponentHandler {
    restart_flags: AtomicU64,
}

impl ComponentHandler {
    fn new() -> Self {
        Self {
            restart_flags: AtomicU64::new(0),
        }
    }
}

impl Class for ComponentHandler {
    type Interfaces = (IComponentHandler,);
}

impl IComponentHandlerTrait for ComponentHandler {
    unsafe fn beginEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }

    unsafe fn performEdit(&self, _id: ParamID, value_normalized: ParamValue) -> tresult {
        if value_normalized.is_finite() && (0.0..=1.0).contains(&value_normalized) {
            kResultOk
        } else {
            kInvalidArgument
        }
    }

    unsafe fn endEdit(&self, _id: ParamID) -> tresult {
        kResultOk
    }

    unsafe fn restartComponent(&self, flags: i32) -> tresult {
        self.restart_flags
            .fetch_or(flags as u32 as u64, Ordering::Relaxed);
        kResultOk
    }
}

#[derive(Clone, Copy)]
struct ParameterPoint {
    sample_offset: i32,
    normalized_value: f64,
}

struct ParameterQueue {
    parameter_id: u32,
    points: Box<[UnsafeCell<ParameterPoint>]>,
    point_count: AtomicI32,
    active: AtomicBool,
}

// SAFETY: One prepared VST3 instance owns each fixed queue. Only its unique audio process thread
// reads or writes point cells, while the atomic length and active flag publish callback-visible
// bounds. No two plugin calls access a queue concurrently.
unsafe impl Send for ParameterQueue {}
// SAFETY: The same single-process-thread invariant protects the UnsafeCell point storage. Shared
// COM references may exist only during the synchronous process call owned by that thread.
unsafe impl Sync for ParameterQueue {}

impl ParameterQueue {
    fn new(parameter_id: u32, capacity: usize) -> Self {
        let points = (0..capacity)
            .map(|_| {
                UnsafeCell::new(ParameterPoint {
                    sample_offset: 0,
                    normalized_value: 0.0,
                })
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self {
            parameter_id,
            points,
            point_count: AtomicI32::new(0),
            active: AtomicBool::new(false),
        }
    }

    fn reset(&self) {
        self.point_count.store(0, Ordering::Relaxed);
        self.active.store(false, Ordering::Relaxed);
    }

    fn activate(&self) {
        self.active.store(true, Ordering::Relaxed);
    }

    fn is_active(&self) -> bool {
        self.active.load(Ordering::Relaxed)
    }

    fn append(&self, sample_offset: i32, normalized_value: f64) -> Option<i32> {
        if sample_offset < 0 || !normalized_value.is_finite() {
            return None;
        }
        let index = self.point_count.load(Ordering::Relaxed);
        let index_usize = usize::try_from(index).ok()?;
        let destination = self.points.get(index_usize)?;
        // SAFETY: The unique process thread writes one point below the fixed capacity before it
        // publishes the incremented point count. No other callback accesses this cell concurrently.
        unsafe {
            destination.get().write(ParameterPoint {
                sample_offset,
                normalized_value,
            });
        }
        self.active.store(true, Ordering::Relaxed);
        self.point_count.store(index + 1, Ordering::Relaxed);
        Some(index)
    }

    fn point(&self, index: usize) -> Option<ParameterPoint> {
        if index >= usize::try_from(self.point_count.load(Ordering::Relaxed)).ok()? {
            return None;
        }
        let point = self.points.get(index)?;
        // SAFETY: The point count was published only after this initialized cell was written, and
        // the same unique process thread reads it before any reset or later process call.
        Some(unsafe { *point.get() })
    }
}

impl Class for ParameterQueue {
    type Interfaces = (IParamValueQueue,);
}

impl IParamValueQueueTrait for ParameterQueue {
    unsafe fn getParameterId(&self) -> ParamID {
        self.parameter_id
    }

    unsafe fn getPointCount(&self) -> i32 {
        self.point_count.load(Ordering::Relaxed)
    }

    unsafe fn getPoint(
        &self,
        index: i32,
        sample_offset: *mut i32,
        value: *mut ParamValue,
    ) -> tresult {
        if sample_offset.is_null() || value.is_null() || index < 0 {
            return kInvalidArgument;
        }
        let Some(point) = usize::try_from(index)
            .ok()
            .and_then(|index| self.point(index))
        else {
            return kInvalidArgument;
        };
        // SAFETY: The plugin supplied writable output pointers for this synchronous COM call, and
        // both values come from initialized fixed queue storage.
        unsafe {
            sample_offset.write(point.sample_offset);
            value.write(point.normalized_value);
        }
        kResultTrue
    }

    unsafe fn addPoint(&self, sample_offset: i32, value: ParamValue, index: *mut i32) -> tresult {
        let Some(point_index) = self.append(sample_offset, value) else {
            return kOutOfMemory;
        };
        if !index.is_null() {
            // SAFETY: The plugin supplied optional writable index storage for this synchronous call.
            unsafe { index.write(point_index) };
        }
        kResultTrue
    }
}

struct ParameterChanges {
    queues: Vec<ComWrapper<ParameterQueue>>,
}

// SAFETY: The queue vector and COM wrappers are immutable after preparation. Each contained queue
// enforces the same unique synchronous process-thread rule for its interior point storage.
unsafe impl Send for ParameterChanges {}
// SAFETY: Concurrent parameter-change calls are excluded by the unique VST3 process owner. Shared
// references exist only to support the COM ABI during that synchronous call.
unsafe impl Sync for ParameterChanges {}

impl ParameterChanges {
    fn new(parameter_ids: &[u32], capacity: usize) -> Self {
        Self {
            queues: parameter_ids
                .iter()
                .copied()
                .map(|id| ComWrapper::new(ParameterQueue::new(id, capacity)))
                .collect(),
        }
    }

    fn reset(&self) {
        for queue in &self.queues {
            queue.reset();
        }
    }

    fn queue(&self, parameter_id: u32) -> Option<&ComWrapper<ParameterQueue>> {
        self.queues
            .binary_search_by_key(&parameter_id, |queue| queue.parameter_id)
            .ok()
            .and_then(|index| self.queues.get(index))
    }

    fn active_count(&self) -> usize {
        self.queues.iter().filter(|queue| queue.is_active()).count()
    }

    fn active_queue(&self, active_index: usize) -> Option<&ComWrapper<ParameterQueue>> {
        self.queues
            .iter()
            .filter(|queue| queue.is_active())
            .nth(active_index)
    }

    fn visit_points(&self, visitor: &mut impl FnMut(OutputPoint)) {
        for queue in self.queues.iter().filter(|queue| queue.is_active()) {
            let count = usize::try_from(queue.point_count.load(Ordering::Relaxed)).unwrap_or(0);
            for index in 0..count {
                if let Some(point) = queue.point(index) {
                    visitor(OutputPoint {
                        parameter_id: queue.parameter_id,
                        sample_offset: point.sample_offset,
                        normalized_value: point.normalized_value,
                    });
                }
            }
        }
    }
}

impl Class for ParameterChanges {
    type Interfaces = (IParameterChanges,);
}

impl IParameterChangesTrait for ParameterChanges {
    unsafe fn getParameterCount(&self) -> i32 {
        i32::try_from(self.active_count()).unwrap_or(i32::MAX)
    }

    unsafe fn getParameterData(&self, index: i32) -> *mut IParamValueQueue {
        if index < 0 {
            return ptr::null_mut();
        }
        self.active_queue(usize::try_from(index).unwrap_or(usize::MAX))
            .and_then(|queue| queue.as_com_ref::<IParamValueQueue>())
            .map_or(ptr::null_mut(), |queue| queue.as_ptr())
    }

    unsafe fn addParameterData(
        &self,
        parameter_id: *const ParamID,
        index: *mut i32,
    ) -> *mut IParamValueQueue {
        if parameter_id.is_null() {
            return ptr::null_mut();
        }
        // SAFETY: The plugin supplies one readable ParamID for the complete synchronous call.
        let parameter_id = unsafe { *parameter_id };
        let Some(queue) = self.queue(parameter_id) else {
            return ptr::null_mut();
        };
        queue.activate();
        if !index.is_null() {
            let active_index = self
                .queues
                .iter()
                .filter(|candidate| candidate.is_active())
                .position(|candidate| candidate.parameter_id == parameter_id)
                .and_then(|index| i32::try_from(index).ok())
                .unwrap_or(-1);
            // SAFETY: The plugin supplied optional writable index storage for this synchronous call.
            unsafe { index.write(active_index) };
        }
        queue
            .as_com_ref::<IParamValueQueue>()
            .map_or(ptr::null_mut(), |queue| queue.as_ptr())
    }
}

type GetPluginFactory = unsafe extern "system" fn() -> *mut IPluginFactory;

#[cfg(target_os = "macos")]
mod platform {
    use super::*;
    use objc2_core_foundation::{CFBundle, CFRetained, CFString, CFURL};

    type BundleEntry = unsafe extern "system" fn(*mut c_void) -> bool;
    type BundleExit = unsafe extern "system" fn() -> bool;

    pub(super) struct NativeModule {
        bundle: CFRetained<CFBundle>,
        exit: BundleExit,
        factory: GetPluginFactory,
        entered: bool,
    }

    impl NativeModule {
        pub(super) fn load(path: &Path) -> Result<Self> {
            if !path.is_dir() || path.extension().and_then(|value| value.to_str()) != Some("vst3") {
                return Err(unavailable(
                    "load_module",
                    "macOS VST3 path must be an existing .vst3 bundle directory",
                ));
            }
            let url = CFURL::from_directory_path(path).ok_or_else(|| {
                unavailable(
                    "load_module",
                    "macOS VST3 bundle path is not a valid file URL",
                )
            })?;
            let bundle = CFBundle::new(None, Some(&url)).ok_or_else(|| {
                unavailable("load_module", "Core Foundation rejected the VST3 bundle")
            })?;
            // SAFETY: The retained CFBundle owns its executable URL and remains live through the
            // matching explicit unload in NativeModule::drop.
            if !unsafe { bundle.load_executable() } {
                return Err(unavailable(
                    "load_module",
                    "Core Foundation could not load the VST3 bundle executable",
                ));
            }
            let entry_pointer = symbol(&bundle, &["bundleEntry", "BundleEntry"]);
            let exit_pointer = symbol(&bundle, &["bundleExit", "BundleExit"]);
            let factory_pointer = symbol(&bundle, &["GetPluginFactory"]);
            if entry_pointer.is_null() || exit_pointer.is_null() || factory_pointer.is_null() {
                // SAFETY: The executable was loaded successfully above and no plugin entry call has
                // occurred, so it may be unloaded immediately on this control thread.
                unsafe { bundle.unload_executable() };
                return Err(unavailable(
                    "load_module",
                    "macOS VST3 bundle is missing an entry, exit, or factory symbol",
                ));
            }
            // SAFETY: VST3 specifies these exported symbols with the exact system signatures below,
            // and the retained CFBundle keeps their code mapped for this module lifetime.
            let entry: BundleEntry = unsafe { mem::transmute(entry_pointer) };
            // SAFETY: The symbol name and retained executable establish the VST3 bundle-exit ABI.
            let exit: BundleExit = unsafe { mem::transmute(exit_pointer) };
            // SAFETY: The required factory export has the VST3 GetPluginFactory signature.
            let factory: GetPluginFactory = unsafe { mem::transmute(factory_pointer) };
            let bundle_reference = (&*bundle as *const CFBundle).cast_mut().cast::<c_void>();
            // SAFETY: The retained CFBundle reference remains live until after bundleExit and the
            // plugin entry function is called once on the worker control thread.
            if !unsafe { entry(bundle_reference) } {
                // SAFETY: Entry reported failure before any factory object was acquired.
                unsafe { bundle.unload_executable() };
                return Err(unavailable(
                    "load_module",
                    "macOS VST3 bundle entry rejected initialization",
                ));
            }
            Ok(Self {
                bundle,
                exit,
                factory,
                entered: true,
            })
        }

        pub(super) fn factory(&self) -> GetPluginFactory {
            self.factory
        }
    }

    impl Drop for NativeModule {
        fn drop(&mut self) {
            if self.entered {
                // SAFETY: All plugin COM owners are dropped before NativeModule. The matching entry
                // succeeded, so this control-thread call closes that lifecycle exactly once.
                let _ = unsafe { (self.exit)() };
                self.entered = false;
            }
            // SAFETY: No plugin object or copied symbol remains reachable when this module drops.
            unsafe { self.bundle.unload_executable() };
        }
    }

    fn symbol(bundle: &CFBundle, names: &[&str]) -> *mut c_void {
        names
            .iter()
            .map(|name| CFString::from_str(name))
            .map(|name| bundle.function_pointer_for_name(Some(&name)))
            .find(|pointer| !pointer.is_null())
            .unwrap_or(ptr::null_mut())
    }
}

#[cfg(target_os = "windows")]
mod platform {
    use super::*;
    use libloading::os::windows::Library;

    type ModuleEntry = unsafe extern "system" fn() -> bool;
    type ModuleExit = unsafe extern "system" fn() -> bool;

    pub(super) struct NativeModule {
        _library: Library,
        exit: Option<ModuleExit>,
        factory: GetPluginFactory,
        entered: bool,
    }

    impl NativeModule {
        pub(super) fn load(path: &Path) -> Result<Self> {
            let module_path = module_binary_path(path)?;
            // SAFETY: The explicit worker-owned module path is retained by Library and all symbols
            // are used only while the Library remains alive.
            let library = unsafe { Library::new(&module_path) }
                .map_err(|error| unavailable("load_module", error.to_string()))?;
            // SAFETY: Optional InitDll has the VST3 Windows module-entry signature.
            let entry = unsafe { library.get::<ModuleEntry>(b"InitDll\0") }
                .ok()
                .map(|symbol| *symbol);
            // SAFETY: Optional ExitDll has the matching VST3 Windows module-exit signature.
            let exit = unsafe { library.get::<ModuleExit>(b"ExitDll\0") }
                .ok()
                .map(|symbol| *symbol);
            // SAFETY: GetPluginFactory is required by VST3 and copied while the Library is retained.
            let factory = *unsafe { library.get::<GetPluginFactory>(b"GetPluginFactory\0") }
                .map_err(|error| unavailable("load_module", error.to_string()))?;
            if entry.is_some_and(|entry| {
                // SAFETY: The copied optional entry function belongs to the retained library and is
                // invoked once on the worker control thread.
                !unsafe { entry() }
            }) {
                return Err(unavailable(
                    "load_module",
                    "Windows VST3 InitDll rejected initialization",
                ));
            }
            Ok(Self {
                _library: library,
                exit,
                factory,
                entered: entry.is_some(),
            })
        }

        pub(super) fn factory(&self) -> GetPluginFactory {
            self.factory
        }
    }

    impl Drop for NativeModule {
        fn drop(&mut self) {
            if self.entered {
                if let Some(exit) = self.exit {
                    // SAFETY: All COM owners have been released and the matching InitDll succeeded.
                    let _ = unsafe { exit() };
                }
                self.entered = false;
            }
        }
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    use libloading::os::unix::{Library, RTLD_LOCAL, RTLD_NOW};

    type ModuleEntry = unsafe extern "system" fn(*mut c_void) -> bool;
    type ModuleExit = unsafe extern "system" fn() -> bool;

    pub(super) struct NativeModule {
        _library: Library,
        exit: ModuleExit,
        factory: GetPluginFactory,
        entered: bool,
    }

    impl NativeModule {
        pub(super) fn load(path: &Path) -> Result<Self> {
            let module_path = module_binary_path(path)?;
            // SAFETY: The explicit worker-owned module path is opened locally and retained for all
            // copied symbol and COM object lifetimes.
            let library = unsafe { Library::open(Some(&module_path), RTLD_NOW | RTLD_LOCAL) }
                .map_err(|error| unavailable("load_module", error.to_string()))?;
            // SAFETY: ModuleEntry is required on Linux and has the VST3 raw-handle signature.
            let entry = *unsafe { library.get::<ModuleEntry>(b"ModuleEntry\0") }
                .map_err(|error| unavailable("load_module", error.to_string()))?;
            // SAFETY: ModuleExit is required and copied while the library remains retained.
            let exit = *unsafe { library.get::<ModuleExit>(b"ModuleExit\0") }
                .map_err(|error| unavailable("load_module", error.to_string()))?;
            // SAFETY: GetPluginFactory is required and copied while the library remains retained.
            let factory = *unsafe { library.get::<GetPluginFactory>(b"GetPluginFactory\0") }
                .map_err(|error| unavailable("load_module", error.to_string()))?;
            let raw_handle = library.into_raw();
            // SAFETY: The handle came directly from this Library::into_raw call and is passed to the
            // required VST3 ModuleEntry before being reconstructed exactly once below.
            let entered = unsafe { entry(raw_handle) };
            // SAFETY: The raw handle is valid, uniquely transferred above, and reconstructed once so
            // normal Library ownership resumes regardless of the plugin entry result.
            let library = unsafe { Library::from_raw(raw_handle) };
            if !entered {
                return Err(unavailable(
                    "load_module",
                    "Linux VST3 ModuleEntry rejected initialization",
                ));
            }
            Ok(Self {
                _library: library,
                exit,
                factory,
                entered: true,
            })
        }

        pub(super) fn factory(&self) -> GetPluginFactory {
            self.factory
        }
    }

    impl Drop for NativeModule {
        fn drop(&mut self) {
            if self.entered {
                // SAFETY: Every COM owner has been released and the matching ModuleEntry succeeded.
                let _ = unsafe { (self.exit)() };
                self.entered = false;
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
mod platform {
    use super::*;

    pub(super) struct NativeModule;

    impl NativeModule {
        pub(super) fn load(_path: &Path) -> Result<Self> {
            Err(unsupported(
                "load_module",
                "VST3 hosting is available only on macOS, Windows, and Linux",
            ))
        }

        pub(super) fn factory(&self) -> GetPluginFactory {
            unreachable!("unsupported targets never construct a VST3 module")
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn module_binary_path(path: &Path) -> Result<PathBuf> {
    if path.is_file() {
        return Ok(path.to_owned());
    }
    if !path.is_dir() || path.extension().and_then(|value| value.to_str()) != Some("vst3") {
        return Err(unavailable(
            "resolve_module_path",
            "VST3 path must be an existing module file or .vst3 bundle directory",
        ));
    }
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| unavailable("resolve_module_path", "VST3 bundle name must be UTF-8"))?;
    #[cfg(target_os = "windows")]
    let (architecture, extension) = match std::env::consts::ARCH {
        "x86_64" => ("x86_64-win", "vst3"),
        "aarch64" => ("arm64-win", "vst3"),
        _ => {
            return Err(unsupported(
                "resolve_module_path",
                "Windows VST3 hosting supports x86_64 and arm64 workers",
            ))
        }
    };
    #[cfg(target_os = "linux")]
    let (architecture, extension) = match std::env::consts::ARCH {
        "x86_64" => ("x86_64-linux", "so"),
        "aarch64" => ("aarch64-linux", "so"),
        _ => {
            return Err(unsupported(
                "resolve_module_path",
                "Linux VST3 hosting supports x86_64 and arm64 workers",
            ))
        }
    };
    let module = path
        .join("Contents")
        .join(architecture)
        .join(format!("{stem}.{extension}"));
    if module.is_file() {
        Ok(module)
    } else {
        Err(unavailable(
            "resolve_module_path",
            format!("VST3 bundle is missing {}", module.display()),
        ))
    }
}

struct NativePlugin {
    factory: Option<ComPtr<IPluginFactory>>,
    component: Option<ComPtr<IComponent>>,
    processor: Option<ComPtr<IAudioProcessor>>,
    controller: Option<ComPtr<IEditController>>,
    component_connection: Option<ComPtr<IConnectionPoint>>,
    controller_connection: Option<ComPtr<IConnectionPoint>>,
    host: ComWrapper<HostApplication>,
    handler: ComWrapper<ComponentHandler>,
    input_changes: Option<ComWrapper<ParameterChanges>>,
    output_changes: Option<ComWrapper<ParameterChanges>>,
    input_channel_pointers: Vec<*mut f32>,
    output_channel_pointers: Vec<*mut f32>,
    controller_separate: bool,
    component_initialized: bool,
    controller_initialized: bool,
    handler_installed: bool,
    component_connected: bool,
    controller_connected: bool,
    input_bus_active: bool,
    output_bus_active: bool,
    component_active: bool,
    processing_active: bool,
    module: platform::NativeModule,
}

impl NativePlugin {
    fn load(config: &Vst3EffectConfig) -> Result<(Self, NativeMetadata)> {
        let module = platform::NativeModule::load(config.bundle_path())?;
        let host = ComWrapper::new(HostApplication);
        let handler = ComWrapper::new(ComponentHandler::new());
        let mut plugin = Self {
            factory: None,
            component: None,
            processor: None,
            controller: None,
            component_connection: None,
            controller_connection: None,
            host,
            handler,
            input_changes: None,
            output_changes: None,
            input_channel_pointers: vec![ptr::null_mut(); config.layout().len()],
            output_channel_pointers: vec![ptr::null_mut(); config.layout().len()],
            controller_separate: false,
            component_initialized: false,
            controller_initialized: false,
            handler_installed: false,
            component_connected: false,
            controller_connected: false,
            input_bus_active: false,
            output_bus_active: false,
            component_active: false,
            processing_active: false,
            module,
        };
        match plugin.initialize(config) {
            Ok(metadata) => Ok((plugin, metadata)),
            Err(error) => {
                if plugin.shutdown().is_err() {
                    // A failed partial-lifecycle unwind must keep executable code and every
                    // remaining COM owner mapped until the dedicated worker process exits.
                    mem::forget(plugin);
                }
                Err(error)
            }
        }
    }

    fn initialize(&mut self, config: &Vst3EffectConfig) -> Result<NativeMetadata> {
        let get_factory = self.module.factory();
        // SAFETY: NativeModule retains the loaded executable and validated GetPluginFactory symbol
        // for the complete COM factory and plugin object lifetime.
        let factory_pointer = unsafe { get_factory() };
        // SAFETY: VST3 GetPluginFactory transfers one owned IPluginFactory reference on success.
        let factory = unsafe { ComPtr::from_raw(factory_pointer) }.ok_or_else(|| {
            unavailable(
                "get_factory",
                "VST3 GetPluginFactory returned a null interface",
            )
        })?;
        self.factory = Some(factory);

        let factory = self
            .factory
            .as_ref()
            .expect("factory assigned above")
            .clone();
        let mut factory_info: PFactoryInfo = zeroed();
        status(
            // SAFETY: factory_info is writable for one exact PFactoryInfo and factory is retained.
            unsafe { factory.getFactoryInfo(&mut factory_info) },
            "get_factory_info",
            "VST3 factory did not provide valid metadata",
            ErrorCategory::CorruptData,
        )?;
        let factory_vendor = c_string(&factory_info.vendor);

        let requested_class = config.class_id().tuid();
        // SAFETY: The retained factory exposes a synchronous class-count query with no pointers.
        let class_count = unsafe { factory.countClasses() };
        if class_count < 0
            || usize::try_from(class_count).unwrap_or(usize::MAX) > MAXIMUM_PARAMETERS
        {
            return Err(Error::new(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                "VST3 factory reported an invalid class count",
            )
            .with_context(ErrorContext::new(COMPONENT, "enumerate_classes")));
        }
        let mut component_name = None;
        for index in 0..class_count {
            let mut class_info: PClassInfo = zeroed();
            // SAFETY: class_info is writable for one PClassInfo and the index is within the factory
            // count returned by the same retained interface.
            if unsafe { factory.getClassInfo(index, &mut class_info) } != kResultOk {
                continue;
            }
            if class_info.cid != requested_class {
                continue;
            }
            if c_string(&class_info.category) != AUDIO_EFFECT_CATEGORY {
                return Err(unsupported(
                    "validate_class",
                    "requested VST3 class is not an audio-effect component",
                ));
            }
            component_name = Some(c_string(&class_info.name));
            break;
        }
        let component_name = component_name.ok_or_else(|| {
            Error::new(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "requested VST3 class is not exposed by the explicit module",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "validate_class")
                    .with_field("class_id", config.class_id().to_string()),
            )
        })?;

        let component = create_instance::<IComponent>(&factory, requested_class, IComponent_iid)?;
        let processor = component.cast::<IAudioProcessor>().ok_or_else(|| {
            unsupported(
                "create_component",
                "requested VST3 component does not expose IAudioProcessor",
            )
        })?;
        let host_pointer = self
            .host
            .as_com_ref::<IHostApplication>()
            .expect("host implements IHostApplication")
            .as_ptr() as *mut FUnknown;
        status(
            // SAFETY: The host COM object is retained by self for the entire initialized component
            // lifetime and the component is a newly owned interface not used concurrently.
            unsafe { component.initialize(host_pointer) },
            "initialize_component",
            "VST3 component initialization failed",
            ErrorCategory::Unavailable,
        )?;
        self.component_initialized = true;
        self.component = Some(component);
        self.processor = Some(processor);

        self.initialize_controller(&factory, host_pointer)?;
        self.connect_component_controller()?;
        if let Some(state) = config.initial_state() {
            self.restore_state(state)?;
        }
        let parameters = self.read_parameters()?;
        let mut parameter_ids = parameters
            .iter()
            .map(|parameter| parameter.id)
            .collect::<Vec<_>>();
        parameter_ids.sort_unstable();
        if parameter_ids.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "VST3 controller exposes duplicate parameter identities",
            )
            .with_context(ErrorContext::new(COMPONENT, "read_parameters")));
        }
        let mut automation_parameter_ids = parameters
            .iter()
            .filter(|parameter| parameter.automatable && !parameter.read_only)
            .map(|parameter| parameter.id)
            .collect::<Vec<_>>();
        automation_parameter_ids.sort_unstable();
        let parameter_queue_count = automation_parameter_ids
            .len()
            .checked_add(parameter_ids.len())
            .ok_or_else(|| {
                unavailable("prepare_parameter_queues", "VST3 queue count overflowed")
            })?;
        let parameter_point_cells = parameter_queue_count
            .checked_mul(config.maximum_automation_points_per_block())
            .ok_or_else(|| {
                unavailable("prepare_parameter_queues", "VST3 point storage overflowed")
            })?;
        if parameter_point_cells > MAXIMUM_PARAMETER_POINT_CELLS {
            return Err(Error::new(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "VST3 parameter queues exceed the explicit preparation bound",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "prepare_parameter_queues")
                    .with_field("parameter_count", parameter_ids.len().to_string())
                    .with_field("point_cells", parameter_point_cells.to_string()),
            ));
        }
        self.input_changes = Some(ComWrapper::new(ParameterChanges::new(
            &automation_parameter_ids,
            config.maximum_automation_points_per_block(),
        )));
        self.output_changes = Some(ComWrapper::new(ParameterChanges::new(
            &parameter_ids,
            config.maximum_automation_points_per_block(),
        )));

        self.prepare_audio(config)?;
        let processor = self.processor.as_ref().expect("processor retained");
        // SAFETY: The initialized unique processor is active and queried synchronously on the
        // worker control thread before any process callback can begin.
        let latency_samples = unsafe { processor.getLatencySamples() };
        // SAFETY: The same retained unique processor is queried before callback publication.
        let tail_samples = unsafe { processor.getTailSamples() };

        Ok(NativeMetadata {
            factory_vendor,
            component_name,
            latency_samples,
            tail_samples,
            parameters,
        })
    }

    fn initialize_controller(
        &mut self,
        factory: &ComPtr<IPluginFactory>,
        host_pointer: *mut FUnknown,
    ) -> Result<()> {
        let component = self.component.as_ref().expect("component initialized");
        if let Some(controller) = component.cast::<IEditController>() {
            self.controller = Some(controller);
            return self.install_component_handler();
        }

        let mut controller_class = [0_i8; 16];
        // SAFETY: controller_class is writable TUID storage and component is retained and uniquely
        // controlled during preparation.
        let result = unsafe { component.getControllerClassId(&mut controller_class) };
        if result != kResultOk {
            return Ok(());
        }
        let controller =
            create_instance::<IEditController>(factory, controller_class, IEditController_iid)?;
        status(
            // SAFETY: The retained host and new controller are not used concurrently.
            unsafe { controller.initialize(host_pointer) },
            "initialize_controller",
            "VST3 controller initialization failed",
            ErrorCategory::Unavailable,
        )?;
        self.controller_initialized = true;
        self.controller_separate = true;
        self.controller = Some(controller);
        self.install_component_handler()
    }

    fn install_component_handler(&mut self) -> Result<()> {
        let Some(controller) = self.controller.as_ref() else {
            return Ok(());
        };
        let handler = self
            .handler
            .as_com_ref::<IComponentHandler>()
            .expect("handler implements IComponentHandler");
        status(
            // SAFETY: The handler COM object is retained by self until after it is removed during
            // shutdown, and the controller call is synchronous on the worker control thread.
            unsafe { controller.setComponentHandler(handler.as_ptr()) },
            "set_component_handler",
            "VST3 controller rejected the component handler",
            ErrorCategory::Unavailable,
        )?;
        self.handler_installed = true;
        Ok(())
    }

    fn connect_component_controller(&mut self) -> Result<()> {
        let Some(controller) = self.controller.as_ref() else {
            return Ok(());
        };
        let Some(component_connection) = self
            .component
            .as_ref()
            .and_then(|component| component.cast::<IConnectionPoint>())
        else {
            return Ok(());
        };
        let Some(controller_connection) = controller.cast::<IConnectionPoint>() else {
            return Ok(());
        };
        self.component_connection = Some(component_connection);
        self.controller_connection = Some(controller_connection);
        let component_connection = self
            .component_connection
            .as_ref()
            .expect("component connection retained");
        let controller_connection = self
            .controller_connection
            .as_ref()
            .expect("controller connection retained");
        status(
            // SAFETY: Both retained connection interfaces remain live together until reverse
            // disconnection, and this call is not concurrent with processing.
            unsafe { component_connection.connect(controller_connection.as_ptr()) },
            "connect_component",
            "VST3 component connection failed",
            ErrorCategory::Unavailable,
        )?;
        self.component_connected = true;
        status(
            // SAFETY: The reciprocal retained connection pointers have the same joint lifetime.
            unsafe { controller_connection.connect(component_connection.as_ptr()) },
            "connect_controller",
            "VST3 controller connection failed",
            ErrorCategory::Unavailable,
        )?;
        self.controller_connected = true;
        Ok(())
    }

    fn restore_state(&self, state: &Vst3PluginState) -> Result<()> {
        if !state.component_state().is_empty() {
            let component_stream =
                ComWrapper::new(MemoryStream::from_bytes(state.component_state().to_vec()));
            let component_stream = component_stream
                .as_com_ref::<IBStream>()
                .expect("memory stream implements IBStream");
            let component = self.component.as_ref().expect("component initialized");
            status(
                // SAFETY: The inactive initialized component and retained stream are uniquely
                // controlled during single-threaded worker preparation.
                unsafe { component.setState(component_stream.as_ptr()) },
                "restore_component_state",
                "VST3 component rejected its durable state",
                ErrorCategory::CorruptData,
            )?;

            if let Some(controller) = self.controller.as_ref() {
                let controller_stream =
                    ComWrapper::new(MemoryStream::from_bytes(state.component_state().to_vec()));
                let controller_stream = controller_stream
                    .as_com_ref::<IBStream>()
                    .expect("memory stream implements IBStream");
                status(
                    // SAFETY: The initialized inactive controller consumes an independent stream
                    // positioned at the beginning of the exact component state.
                    unsafe { controller.setComponentState(controller_stream.as_ptr()) },
                    "restore_controller_component_state",
                    "VST3 controller rejected the restored component state",
                    ErrorCategory::CorruptData,
                )?;
            }
        }

        if !state.controller_state().is_empty() {
            let Some(controller) = self.controller.as_ref() else {
                return Err(Error::new(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "VST3 state contains controller bytes but the component has no controller",
                )
                .with_context(ErrorContext::new(COMPONENT, "restore_controller_state")));
            };
            let controller_stream =
                ComWrapper::new(MemoryStream::from_bytes(state.controller_state().to_vec()));
            let controller_stream = controller_stream
                .as_com_ref::<IBStream>()
                .expect("memory stream implements IBStream");
            status(
                // SAFETY: The initialized inactive controller and exact retained stream are
                // uniquely controlled during preparation.
                unsafe { controller.setState(controller_stream.as_ptr()) },
                "restore_controller_state",
                "VST3 controller rejected its durable state",
                ErrorCategory::CorruptData,
            )?;
        }
        Ok(())
    }

    fn capture_state(&self) -> Result<Vst3PluginState> {
        if !host_control_call_allowed() {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::Retryable,
                "VST3 state cannot be captured on the audio callback domain",
            )
            .with_context(ErrorContext::new(COMPONENT, "capture_state")));
        }
        let component_stream = ComWrapper::new(MemoryStream::empty());
        let component_stream_reference = component_stream
            .as_com_ref::<IBStream>()
            .expect("memory stream implements IBStream");
        let component = self.component.as_ref().expect("component initialized");
        let component_supported = optional_state_status(
            // SAFETY: The retained component and bounded writable stream are accessed only by the
            // exclusive prepared worker owner outside the audio callback.
            unsafe { component.getState(component_stream_reference.as_ptr()) },
            "capture_component_state",
            "VST3 component state capture failed",
        )?;
        let component_state = if component_supported {
            component_stream.bytes()?
        } else {
            Vec::new()
        };

        let controller_state = if let Some(controller) = self.controller.as_ref() {
            let controller_stream = ComWrapper::new(MemoryStream::empty());
            let controller_stream_reference = controller_stream
                .as_com_ref::<IBStream>()
                .expect("memory stream implements IBStream");
            let supported = optional_state_status(
                // SAFETY: The retained controller and independent bounded stream have the same
                // exclusive control-side access as the component capture above.
                unsafe { controller.getState(controller_stream_reference.as_ptr()) },
                "capture_controller_state",
                "VST3 controller state capture failed",
            )?;
            if supported {
                controller_stream.bytes()?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        Vst3PluginState::new(component_state, controller_state)
    }

    fn read_parameters(&self) -> Result<Vec<NativeParameterInfo>> {
        let Some(controller) = self.controller.as_ref() else {
            return Ok(Vec::new());
        };
        // SAFETY: The retained controller is queried synchronously before processing begins.
        let count = unsafe { controller.getParameterCount() };
        if count < 0 || usize::try_from(count).unwrap_or(usize::MAX) > MAXIMUM_PARAMETERS {
            return Err(Error::new(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "VST3 controller parameter count exceeds the host bound",
            )
            .with_context(ErrorContext::new(COMPONENT, "read_parameters")));
        }
        let mut parameters = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
        for index in 0..count {
            let mut info: ParameterInfo = zeroed();
            status(
                // SAFETY: info is writable exact storage and index is within the retained
                // controller's reported parameter count.
                unsafe { controller.getParameterInfo(index, &mut info) },
                "read_parameter",
                "VST3 controller returned invalid parameter metadata",
                ErrorCategory::CorruptData,
            )?;
            if !info.defaultNormalizedValue.is_finite()
                || !(0.0..=1.0).contains(&info.defaultNormalizedValue)
            {
                return Err(Error::new(
                    ErrorCategory::CorruptData,
                    Recoverability::UserCorrectable,
                    "VST3 controller reported an invalid normalized parameter default",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "read_parameter")
                        .with_field("parameter_id", info.id.to_string()),
                ));
            }
            parameters.push(NativeParameterInfo {
                id: info.id,
                title: utf16_string(&info.title),
                default_normalized_value: info.defaultNormalizedValue,
                automatable: info.flags & ParameterInfo_::ParameterFlags_::kCanAutomate != 0,
                read_only: info.flags & ParameterInfo_::ParameterFlags_::kIsReadOnly != 0,
            });
        }
        Ok(parameters)
    }

    fn prepare_audio(&mut self, config: &Vst3EffectConfig) -> Result<()> {
        let component = self.component.as_ref().expect("component initialized");
        let processor = self.processor.as_ref().expect("processor retained");
        for direction in [
            BusDirections_::kInput as BusDirection,
            BusDirections_::kOutput as BusDirection,
        ] {
            // SAFETY: Retained component bus counts are queried during single-threaded preparation.
            let audio_count = unsafe { component.getBusCount(MediaTypes_::kAudio as _, direction) };
            if audio_count != 1 {
                return Err(unsupported(
                    "validate_buses",
                    "VST3 hosting requires exactly one main audio input and one main audio output",
                ));
            }
            // SAFETY: Event bus count is a pointer-free query on the retained component.
            let event_count = unsafe { component.getBusCount(MediaTypes_::kEvent as _, direction) };
            if event_count != 0 {
                return Err(unsupported(
                    "validate_buses",
                    "VST3 hosting does not support event buses or instruments",
                ));
            }
            let mut bus_info: BusInfo = zeroed();
            status(
                // SAFETY: bus_info is exact writable storage and index zero exists by the count.
                unsafe {
                    component.getBusInfo(MediaTypes_::kAudio as _, direction, 0, &mut bus_info)
                },
                "read_bus",
                "VST3 component did not provide valid main-bus metadata",
                ErrorCategory::CorruptData,
            )?;
            if bus_info.busType != BusTypes_::kMain as BusType
                || usize::try_from(bus_info.channelCount).ok() != Some(config.layout().len())
            {
                return Err(unsupported(
                    "validate_buses",
                    "VST3 main-bus channel count or role does not match the prepared layout",
                ));
            }
        }

        let io_mode = match config.process_mode() {
            Vst3ProcessMode::Realtime => IoModes_::kAdvanced as IoMode,
            Vst3ProcessMode::Offline => IoModes_::kOfflineProcessing as IoMode,
        };
        status(
            // SAFETY: The retained component is inactive and exclusively configured here.
            unsafe { component.setIoMode(io_mode) },
            "set_io_mode",
            "VST3 component rejected the selected I/O mode",
            ErrorCategory::Unsupported,
        )?;
        let mut input_arrangement = config.speaker_arrangement();
        let mut output_arrangement = config.speaker_arrangement();
        status(
            // SAFETY: Both pointers cover one writable arrangement and the processor is inactive.
            unsafe {
                processor.setBusArrangements(&mut input_arrangement, 1, &mut output_arrangement, 1)
            },
            "set_bus_arrangements",
            "VST3 component rejected the exact semantic speaker arrangement",
            ErrorCategory::Unsupported,
        )?;
        for direction in [
            BusDirections_::kInput as BusDirection,
            BusDirections_::kOutput as BusDirection,
        ] {
            let mut arrangement = 0_u64;
            status(
                // SAFETY: arrangement is one writable value and bus zero exists.
                unsafe { processor.getBusArrangement(direction, 0, &mut arrangement) },
                "confirm_bus_arrangement",
                "VST3 component did not confirm its speaker arrangement",
                ErrorCategory::CorruptData,
            )?;
            if arrangement != config.speaker_arrangement() {
                return Err(unsupported(
                    "confirm_bus_arrangement",
                    "VST3 component changed the requested semantic speaker arrangement",
                ));
            }
        }
        status(
            // SAFETY: This pointer-free query occurs before setup on the retained processor.
            unsafe { processor.canProcessSampleSize(sample_32_code()) },
            "validate_sample_size",
            "VST3 component does not support 32-bit floating-point processing",
            ErrorCategory::Unsupported,
        )?;
        if let Some(requirements) = processor.cast::<IProcessContextRequirements>() {
            // SAFETY: The optional retained requirements interface is queried once during setup.
            if unsafe { requirements.getProcessContextRequirements() } != 0 {
                return Err(unsupported(
                    "validate_process_context_requirements",
                    "VST3 component requires optional process context fields that Superi does not guess",
                ));
            }
        }
        let mut setup = ProcessSetup {
            processMode: config.process_mode().native_code(),
            symbolicSampleSize: sample_32_code(),
            maxSamplesPerBlock: i32::try_from(config.maximum_frames())
                .expect("configuration bounds maximum frames to i32"),
            sampleRate: f64::from(config.sample_rate()),
        };
        status(
            // SAFETY: setup is exact initialized storage and the processor is inactive.
            unsafe { processor.setupProcessing(&mut setup) },
            "setup_processing",
            "VST3 component rejected its bounded process setup",
            ErrorCategory::Unsupported,
        )?;
        status(
            // SAFETY: Main input bus zero was validated and activation precedes component use.
            unsafe {
                component.activateBus(
                    MediaTypes_::kAudio as _,
                    BusDirections_::kInput as BusDirection,
                    0,
                    1,
                )
            },
            "activate_input_bus",
            "VST3 component rejected main-input activation",
            ErrorCategory::Unavailable,
        )?;
        self.input_bus_active = true;
        status(
            // SAFETY: Main output bus zero was validated and activation precedes component use.
            unsafe {
                component.activateBus(
                    MediaTypes_::kAudio as _,
                    BusDirections_::kOutput as BusDirection,
                    0,
                    1,
                )
            },
            "activate_output_bus",
            "VST3 component rejected main-output activation",
            ErrorCategory::Unavailable,
        )?;
        self.output_bus_active = true;
        status(
            // SAFETY: Preparation completed and the component is exclusively owned.
            unsafe { component.setActive(1) },
            "activate_component",
            "VST3 component activation failed",
            ErrorCategory::Unavailable,
        )?;
        self.component_active = true;
        status(
            // SAFETY: The component is active and the processor remains uniquely owned.
            unsafe { processor.setProcessing(1) },
            "start_processing",
            "VST3 processor could not enter processing state",
            ErrorCategory::Unavailable,
        )?;
        self.processing_active = true;
        Ok(())
    }

    fn process(&mut self, block: ProcessBlock<'_>) -> Result<ProcessOutcome> {
        let expected_planar = block
            .channel_count
            .checked_mul(block.channel_stride)
            .ok_or_else(|| unavailable("process", "VST3 planar geometry overflowed"))?;
        if block.input_planar.len() != expected_planar
            || block.output_planar.len() != expected_planar
            || block.frame_count > block.channel_stride
            || block.channel_count != self.input_channel_pointers.len()
        {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "VST3 native process bridge received inconsistent planar geometry",
            )
            .with_context(ErrorContext::new(COMPONENT, "process")));
        }
        let input_changes = self.input_changes.as_ref().expect("changes prepared");
        let output_changes = self.output_changes.as_ref().expect("changes prepared");
        input_changes.reset();
        output_changes.reset();
        for point in block.automation {
            let queue = input_changes.queue(point.parameter_id).ok_or_else(|| {
                Error::new(
                    ErrorCategory::NotFound,
                    Recoverability::UserCorrectable,
                    "VST3 process automation references an unknown parameter",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "process")
                        .with_field("parameter_id", point.parameter_id.to_string()),
                )
            })?;
            if queue
                .append(point.sample_offset, point.normalized_value)
                .is_none()
            {
                return Err(Error::new(
                    ErrorCategory::ResourceExhausted,
                    Recoverability::Retryable,
                    "VST3 input parameter queue exceeded its prepared capacity",
                )
                .with_context(ErrorContext::new(COMPONENT, "process")));
            }
        }

        for channel in 0..block.channel_count {
            let offset = channel
                .checked_mul(block.channel_stride)
                .expect("validated planar geometry");
            // SAFETY: Each offset is within the exact mutable input planar slice and the pointer is
            // borrowed only for the synchronous VST3 process call below.
            self.input_channel_pointers[channel] =
                unsafe { block.input_planar.as_mut_ptr().add(offset) };
            // SAFETY: The same checked offset is within the exact mutable output planar slice.
            self.output_channel_pointers[channel] =
                unsafe { block.output_planar.as_mut_ptr().add(offset) };
        }
        let mut input_bus = AudioBusBuffers {
            numChannels: i32::try_from(block.channel_count)
                .expect("supported layouts fit signed channel count"),
            silenceFlags: block.input_silence_flags,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: self.input_channel_pointers.as_mut_ptr(),
            },
        };
        let mut output_bus = AudioBusBuffers {
            numChannels: i32::try_from(block.channel_count)
                .expect("supported layouts fit signed channel count"),
            silenceFlags: 0,
            __field0: AudioBusBuffers__type0 {
                channelBuffers32: self.output_channel_pointers.as_mut_ptr(),
            },
        };
        let mut context: ProcessContext = zeroed();
        context.state = 0;
        context.sampleRate = f64::from(block.sample_rate);
        context.projectTimeSamples = block.start_sample;
        let input_changes_pointer = input_changes
            .as_com_ref::<IParameterChanges>()
            .expect("input changes implement IParameterChanges")
            .as_ptr();
        let output_changes_pointer = output_changes
            .as_com_ref::<IParameterChanges>()
            .expect("output changes implement IParameterChanges")
            .as_ptr();
        let mut process_data = ProcessData {
            processMode: block.process_mode,
            symbolicSampleSize: sample_32_code(),
            numSamples: i32::try_from(block.frame_count)
                .expect("configuration bounds frame count to i32"),
            numInputs: 1,
            numOutputs: 1,
            inputs: &mut input_bus,
            outputs: &mut output_bus,
            inputParameterChanges: input_changes_pointer,
            outputParameterChanges: output_changes_pointer,
            inputEvents: ptr::null_mut(),
            outputEvents: ptr::null_mut(),
            processContext: &mut context,
        };
        let processor = self.processor.as_ref().expect("processor retained");
        status(
            // SAFETY: Every pointer covers the exact callback extent, the processor has one owner,
            // and no controller or lifecycle call runs concurrently.
            unsafe { processor.process(&mut process_data) },
            "process",
            "VST3 processor returned a process failure",
            ErrorCategory::Unavailable,
        )?;
        Ok(ProcessOutcome {
            output_silence_flags: output_bus.silenceFlags,
            restart_flags: self.handler.restart_flags.swap(0, Ordering::Relaxed),
        })
    }

    fn visit_output_points(&self, mut visitor: impl FnMut(OutputPoint)) {
        if let Some(changes) = self.output_changes.as_ref() {
            changes.visit_points(&mut visitor);
        }
    }

    fn shutdown(&mut self) -> Result<()> {
        if self.processing_active {
            if let Some(processor) = self.processor.as_ref() {
                status(
                    // SAFETY: Processing has stopped at the graph boundary and this unique processor
                    // is shut down once on the worker control thread.
                    unsafe { processor.setProcessing(0) },
                    "stop_processing",
                    "VST3 processor could not leave processing state",
                    ErrorCategory::Unavailable,
                )?;
            }
            self.processing_active = false;
        }
        if self.component_active {
            if let Some(component) = self.component.as_ref() {
                status(
                    // SAFETY: Processing is stopped and this component is deactivated exactly once.
                    unsafe { component.setActive(0) },
                    "deactivate_component",
                    "VST3 component deactivation failed",
                    ErrorCategory::Unavailable,
                )?;
            }
            self.component_active = false;
        }
        if self.output_bus_active {
            if let Some(component) = self.component.as_ref() {
                status(
                    // SAFETY: The component is inactive and the validated output bus is released once.
                    unsafe {
                        component.activateBus(
                            MediaTypes_::kAudio as _,
                            BusDirections_::kOutput as BusDirection,
                            0,
                            0,
                        )
                    },
                    "deactivate_output_bus",
                    "VST3 main-output deactivation failed",
                    ErrorCategory::Unavailable,
                )?;
            }
            self.output_bus_active = false;
        }
        if self.input_bus_active {
            if let Some(component) = self.component.as_ref() {
                status(
                    // SAFETY: The component is inactive and the validated input bus is released once.
                    unsafe {
                        component.activateBus(
                            MediaTypes_::kAudio as _,
                            BusDirections_::kInput as BusDirection,
                            0,
                            0,
                        )
                    },
                    "deactivate_input_bus",
                    "VST3 main-input deactivation failed",
                    ErrorCategory::Unavailable,
                )?;
            }
            self.input_bus_active = false;
        }
        if self.controller_connected {
            if let (Some(component), Some(controller)) = (
                self.component_connection.as_ref(),
                self.controller_connection.as_ref(),
            ) {
                status(
                    // SAFETY: Both retained endpoints live and controller connection was made.
                    unsafe { controller.disconnect(component.as_ptr()) },
                    "disconnect_controller",
                    "VST3 controller connection could not be removed",
                    ErrorCategory::Unavailable,
                )?;
            }
            self.controller_connected = false;
        }
        if self.component_connected {
            if let (Some(component), Some(controller)) = (
                self.component_connection.as_ref(),
                self.controller_connection.as_ref(),
            ) {
                status(
                    // SAFETY: Both retained endpoints live and component connection was made.
                    unsafe { component.disconnect(controller.as_ptr()) },
                    "disconnect_component",
                    "VST3 component connection could not be removed",
                    ErrorCategory::Unavailable,
                )?;
            }
            self.component_connected = false;
        }
        self.controller_connection = None;
        self.component_connection = None;
        if self.handler_installed {
            if let Some(controller) = self.controller.as_ref() {
                status(
                    // SAFETY: The handler is retained and processing and connections have stopped.
                    unsafe { controller.setComponentHandler(ptr::null_mut()) },
                    "remove_component_handler",
                    "VST3 controller could not release the component handler",
                    ErrorCategory::Unavailable,
                )?;
            }
            self.handler_installed = false;
        }
        if self.controller_initialized && self.controller_separate {
            if let Some(controller) = self.controller.as_ref() {
                status(
                    // SAFETY: Separate initialization succeeded and handler and connections are gone.
                    unsafe { controller.terminate() },
                    "terminate_controller",
                    "VST3 controller termination failed",
                    ErrorCategory::Unavailable,
                )?;
            }
            self.controller_initialized = false;
        }
        self.controller = None;
        self.output_changes = None;
        self.input_changes = None;
        self.processor = None;
        if self.component_initialized {
            if let Some(component) = self.component.as_ref() {
                status(
                    // SAFETY: Derived process owners are gone and initialization is matched once.
                    unsafe { component.terminate() },
                    "terminate_component",
                    "VST3 component termination failed",
                    ErrorCategory::Unavailable,
                )?;
            }
            self.component_initialized = false;
        }
        self.component = None;
        self.factory = None;
        Ok(())
    }
}

impl Drop for NativePlugin {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn create_instance<I: Interface>(
    factory: &ComPtr<IPluginFactory>,
    class_id: TUID,
    interface_id: TUID,
) -> Result<ComPtr<I>> {
    let mut object = ptr::null_mut();
    status(
        // SAFETY: Both IDs cover 16 readable bytes, object is writable out-pointer storage, and
        // the retained factory owns any returned reference until transfer.
        unsafe {
            factory.createInstance(
                class_id.as_ptr() as FIDString,
                interface_id.as_ptr() as FIDString,
                &mut object,
            )
        },
        "create_instance",
        "VST3 factory could not create the requested interface",
        ErrorCategory::Unsupported,
    )?;
    // SAFETY: A successful VST3 createInstance transfers one owned reference of the requested
    // interface type through the nonnull out pointer.
    unsafe { ComPtr::from_raw(object.cast::<I>()) }.ok_or_else(|| {
        unavailable(
            "create_instance",
            "VST3 factory returned a null interface after successful creation",
        )
    })
}

/// Retained native plugin lease shared only by one control owner and one prepared audio owner.
pub(super) struct NativeLease {
    plugin: UnsafeCell<Option<NativePlugin>>,
}

// SAFETY: NativeLease is created with exactly one session owner and one prepared processor owner.
// The session performs lifecycle calls only after Arc strong-count proof that the processor lease
// has returned. The processor alone performs synchronous audio calls, so NativePlugin is never
// accessed concurrently despite third-party interfaces advertising broader Send behavior.
unsafe impl Send for NativeLease {}
// SAFETY: Shared references expose only methods governed by the same exclusive lease protocol.
// Interior mutation is confined to the one audio owner or the later control-side shutdown, never
// both, and Drop leaks an unretired plugin instead of running lifecycle code on an unknown thread.
unsafe impl Sync for NativeLease {}

impl NativeLease {
    pub(super) fn load(config: &Vst3EffectConfig) -> Result<(Arc<Self>, NativeMetadata)> {
        let (plugin, metadata) = NativePlugin::load(config)?;
        Ok((
            Arc::new(Self {
                plugin: UnsafeCell::new(Some(plugin)),
            }),
            metadata,
        ))
    }

    pub(super) fn process(&self, block: ProcessBlock<'_>) -> Result<ProcessOutcome> {
        // SAFETY: The PreparedVst3WorkerEffect is the sole process owner and session lifecycle is
        // excluded while its Arc lease exists. This yields unique mutable access for one callback.
        let plugin = unsafe { &mut *self.plugin.get() }
            .as_mut()
            .ok_or_else(|| unavailable("process", "VST3 native session is already shut down"))?;
        plugin.process(block)
    }

    pub(super) fn visit_output_points(&self, visitor: impl FnMut(OutputPoint)) {
        // SAFETY: Called by the sole audio owner immediately after its synchronous process call;
        // control-side shutdown remains excluded by the outstanding Arc lease.
        if let Some(plugin) = unsafe { &*self.plugin.get() }.as_ref() {
            plugin.visit_output_points(visitor);
        }
    }

    pub(super) fn capture_state(&self) -> Result<Vst3PluginState> {
        // SAFETY: The public prepared effect requires exclusive mutable ownership and a non-audio
        // execution domain before entering this control-side operation. Session shutdown remains
        // excluded by the outstanding prepared Arc lease.
        let plugin = unsafe { &*self.plugin.get() }
            .as_ref()
            .ok_or_else(|| unavailable("capture_state", "VST3 native session is shut down"))?;
        plugin.capture_state()
    }

    pub(super) fn shutdown(&self) -> Result<()> {
        // SAFETY: The public session verifies it holds the only Arc before this call, proving no
        // prepared audio owner can access the plugin while lifecycle calls run here.
        let slot = unsafe { &mut *self.plugin.get() };
        if let Some(plugin) = slot.as_mut() {
            plugin.shutdown()?;
        }
        if let Some(plugin) = slot.take() {
            drop(plugin);
        }
        Ok(())
    }
}

impl Drop for NativeLease {
    fn drop(&mut self) {
        // SAFETY: Drop has exclusive access to the lease cell. An unretired plugin is intentionally
        // removed and forgotten so no COM release, module exit, or unload can occur on an audio or
        // otherwise unknown thread. The dedicated worker process reclaims it at exit.
        if let Some(plugin) = unsafe { &mut *self.plugin.get() }.take() {
            mem::forget(plugin);
        }
    }
}
