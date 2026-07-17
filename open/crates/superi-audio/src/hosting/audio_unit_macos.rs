//! Private AudioToolbox ownership and callback boundary for macOS Audio Unit effects.

#![allow(unsafe_code)]

use std::array;
use std::ffi::c_void;
use std::fmt;
use std::mem::{offset_of, size_of, MaybeUninit};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicI64, AtomicPtr, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use block2::RcBlock;
use objc2_audio_toolbox as at;
use objc2_core_audio_types as ca;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{ChannelLayout, ChannelPosition};

use crate::graph::AudioProcessBlock;

use super::{AudioUnitExecutionPolicy, AudioUnitHostConfig};

const COMPONENT: &str = "superi-audio.hosting.audio-unit.macos";
const ELEMENT_ZERO: u32 = 0;
const MAX_CHANNELS: usize = 64;
const MAX_EXACT_F64_INTEGER: i64 = 1_i64 << 53;
const INSTANTIATION_TIMEOUT: Duration = Duration::from_secs(30);

const CALLBACK_OK: i32 = 0;
const CALLBACK_INACTIVE: i32 = 1;
const CALLBACK_WRONG_BUS: i32 = 2;
const CALLBACK_INVALID_TIMESTAMP: i32 = 3;
const CALLBACK_OUT_OF_RANGE: i32 = 4;
const CALLBACK_INVALID_BUFFER_LIST: i32 = 5;
const CALLBACK_INVALID_BUFFER: i32 = 6;
const CALLBACK_PANICKED: i32 = 7;

#[repr(C)]
struct FixedAudioBufferList {
    number_buffers: u32,
    buffers: [ca::AudioBuffer; MAX_CHANNELS],
}

impl FixedAudioBufferList {
    fn new(channel_count: usize) -> Self {
        Self {
            number_buffers: u32::try_from(channel_count).expect("validated channel count"),
            buffers: [ca::AudioBuffer {
                mNumberChannels: 1,
                mDataByteSize: 0,
                mData: ptr::null_mut(),
            }; MAX_CHANNELS],
        }
    }

    fn as_audio_buffer_list(&mut self) -> NonNull<ca::AudioBufferList> {
        NonNull::from(self).cast()
    }
}

#[repr(C)]
struct FixedChannelLayout {
    tag: ca::AudioChannelLayoutTag,
    bitmap: ca::AudioChannelBitmap,
    description_count: u32,
    descriptions: [ca::AudioChannelDescription; MAX_CHANNELS],
}

impl FixedChannelLayout {
    fn from_layout(layout: &ChannelLayout) -> Result<Self> {
        let mut descriptions = [ca::AudioChannelDescription {
            mChannelLabel: ca::kAudioChannelLabel_Unknown,
            mChannelFlags: ca::AudioChannelFlags(0),
            mCoordinates: [0.0; 3],
        }; MAX_CHANNELS];
        for (description, position) in descriptions.iter_mut().zip(layout.positions()) {
            description.mChannelLabel = channel_label(*position).ok_or_else(|| {
                host_error(
                    ErrorCategory::Unsupported,
                    Recoverability::UserCorrectable,
                    "map_channel_layout",
                    "Audio Unit host cannot represent a channel position from this core version",
                )
            })?;
        }
        Ok(Self {
            tag: ca::kAudioChannelLayoutTag_UseChannelDescriptions,
            bitmap: ca::AudioChannelBitmap(0),
            description_count: u32::try_from(layout.len()).expect("validated channel count"),
            descriptions,
        })
    }

    fn byte_size(channel_count: usize) -> u32 {
        let bytes = offset_of!(Self, descriptions)
            + channel_count * size_of::<ca::AudioChannelDescription>();
        u32::try_from(bytes).expect("fixed channel layout fits u32")
    }

    fn matches(&self, layout: &ChannelLayout, returned_size: u32) -> bool {
        let required_size = Self::byte_size(layout.len());
        self.tag == ca::kAudioChannelLayoutTag_UseChannelDescriptions
            && self.bitmap == ca::AudioChannelBitmap(0)
            && self.description_count == u32::try_from(layout.len()).expect("validated layout")
            && returned_size >= required_size
            && self
                .descriptions
                .iter()
                .zip(layout.positions())
                .take(layout.len())
                .all(|(description, position)| {
                    channel_label(*position).is_some_and(|label| {
                        description.mChannelLabel == label
                            && description.mChannelFlags == ca::AudioChannelFlags(0)
                    })
                })
    }
}

struct CallbackContext {
    active: AtomicBool,
    start_sample: AtomicI64,
    frame_count: AtomicUsize,
    channel_count: usize,
    maximum_frames: usize,
    input_planes: [AtomicPtr<f32>; MAX_CHANNELS],
    first_error: AtomicI32,
}

#[derive(Default)]
struct InstantiationState {
    result: Option<(SendAudioUnit, i32)>,
    abandoned: bool,
}

struct SendAudioUnit(at::AudioComponentInstance);

// SAFETY: AudioComponentInstantiate explicitly transfers the completed instance to its escaping
// completion handler. This wrapper moves that uniquely owned value through the completion mutex;
// neither thread accesses the instance until the receiver removes it from the state.
unsafe impl Send for SendAudioUnit {}

impl CallbackContext {
    fn new(input: &[f32], channel_count: usize, maximum_frames: usize) -> Self {
        let input_planes = array::from_fn(|channel| {
            let plane = if channel < channel_count {
                input
                    .as_ptr()
                    .wrapping_add(channel * maximum_frames)
                    .cast_mut()
            } else {
                ptr::null_mut()
            };
            AtomicPtr::new(plane)
        });
        Self {
            active: AtomicBool::new(false),
            start_sample: AtomicI64::new(0),
            frame_count: AtomicUsize::new(0),
            channel_count,
            maximum_frames,
            input_planes,
            first_error: AtomicI32::new(CALLBACK_OK),
        }
    }

    fn begin(&self, start_sample: i64, frame_count: usize) {
        self.start_sample.store(start_sample, Ordering::Relaxed);
        self.frame_count.store(frame_count, Ordering::Relaxed);
        self.first_error.store(CALLBACK_OK, Ordering::Relaxed);
        self.active.store(true, Ordering::Release);
    }

    fn finish(&self) -> i32 {
        self.active.store(false, Ordering::Release);
        self.first_error.load(Ordering::Acquire)
    }

    fn record_error(&self, code: i32) {
        let _ = self.first_error.compare_exchange(
            CALLBACK_OK,
            code,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
    }
}

/// One uniquely owned and initialized native Audio Unit instance.
pub(super) struct PreparedAudioUnit {
    unit: at::AudioUnit,
    component_version: u32,
    loaded_out_of_process: bool,
    input_planes: Vec<f32>,
    output_planes: Vec<f32>,
    output_buffers: FixedAudioBufferList,
    callback: Box<CallbackContext>,
    initialized: bool,
    poisoned: bool,
}

impl fmt::Debug for PreparedAudioUnit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedAudioUnit")
            .field("unit", &self.unit)
            .field("component_version", &self.component_version)
            .field("loaded_out_of_process", &self.loaded_out_of_process)
            .field("initialized", &self.initialized)
            .field("poisoned", &self.poisoned)
            .finish_non_exhaustive()
    }
}

// SAFETY: The native handle and all callback storage have one Rust owner. Public processing needs
// exclusive mutable access, the callback is live only inside a synchronous render call, and Drop
// tears down the native instance before releasing the registered context or plane allocations.
unsafe impl Send for PreparedAudioUnit {}

impl PreparedAudioUnit {
    pub(super) fn prepare(config: &AudioUnitHostConfig) -> Result<Self> {
        let mut description = at::AudioComponentDescription {
            componentType: config.component().raw_component_type(),
            componentSubType: config.component().raw_subtype(),
            componentManufacturer: config.component().raw_manufacturer(),
            componentFlags: 0,
            componentFlagsMask: 0,
        };
        // SAFETY: `description` remains live for the synchronous lookup and the null first
        // component pointer requests the first exact match.
        let component =
            unsafe { at::AudioComponentFindNext(ptr::null_mut(), NonNull::from(&mut description)) };
        if component.is_null() {
            return Err(host_error(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "find_component",
                "the exact Audio Unit effect is not installed",
            ));
        }

        let unit = instantiate(component, config.execution_policy())?;
        let sample_count = config
            .maximum_frames()
            .checked_mul(config.input_layout().len())
            .expect("configuration validated scratch length");
        let input_planes = vec![0.0; sample_count];
        let output_planes = vec![0.0; sample_count];
        let callback = Box::new(CallbackContext::new(
            &input_planes,
            config.input_layout().len(),
            config.maximum_frames(),
        ));
        let mut prepared = Self {
            unit,
            component_version: 0,
            loaded_out_of_process: false,
            input_planes,
            output_planes,
            output_buffers: FixedAudioBufferList::new(config.output_layout().len()),
            callback,
            initialized: false,
            poisoned: false,
        };
        prepared.verify_component(config)?;
        prepared.configure(config)?;
        Ok(prepared)
    }

    pub(super) const fn component_version(&self) -> u32 {
        self.component_version
    }

    pub(super) const fn loaded_out_of_process(&self) -> bool {
        self.loaded_out_of_process
    }

    pub(super) const fn is_poisoned(&self) -> bool {
        self.poisoned
    }

    fn verify_component(&mut self, config: &AudioUnitHostConfig) -> Result<()> {
        // SAFETY: `self.unit` is the live instance returned by successful instantiation.
        let component = unsafe { at::AudioComponentInstanceGetComponent(self.unit) };
        if component.is_null() {
            return Err(host_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "verify_component",
                "the native instance did not retain its component identity",
            ));
        }
        let mut actual = MaybeUninit::<at::AudioComponentDescription>::uninit();
        // SAFETY: `component` is live and `actual` provides writable storage for the complete call.
        let status = unsafe {
            at::AudioComponentGetDescription(component, NonNull::new_unchecked(actual.as_mut_ptr()))
        };
        check_status(status, "read_component_description")?;
        // SAFETY: A successful call initialized the complete description value.
        let actual = unsafe { actual.assume_init() };
        if actual.componentType != config.component().raw_component_type()
            || actual.componentSubType != config.component().raw_subtype()
            || actual.componentManufacturer != config.component().raw_manufacturer()
        {
            return Err(host_error(
                ErrorCategory::Conflict,
                Recoverability::Terminal,
                "verify_component",
                "the instantiated Audio Unit identity does not match the requested effect",
            ));
        }

        let mut version = 0_u32;
        // SAFETY: `component` remains live and `version` is writable for the complete call.
        let status =
            unsafe { at::AudioComponentGetVersion(component, NonNull::from(&mut version)) };
        check_status(status, "read_component_version")?;
        self.component_version = version;

        let loaded: u32 = get_property(
            self.unit,
            at::kAudioUnitProperty_LoadedOutOfProcess,
            at::kAudioUnitScope_Global,
            ELEMENT_ZERO,
            "read_process_location",
        )?;
        if loaded > 1 {
            return Err(host_error(
                ErrorCategory::CorruptData,
                Recoverability::Terminal,
                "read_process_location",
                "Audio Unit returned an invalid process-location value",
            ));
        }
        self.loaded_out_of_process = loaded == 1;
        if config.execution_policy() == AudioUnitExecutionPolicy::RequireOutOfProcess
            && !self.loaded_out_of_process
        {
            return Err(host_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "verify_process_location",
                "Audio Unit fell back to the host process when isolation was required",
            ));
        }
        Ok(())
    }

    fn configure(&mut self, config: &AudioUnitHostConfig) -> Result<()> {
        let maximum_frames = u32::try_from(config.maximum_frames()).expect("validated frame count");
        set_property(
            self.unit,
            at::kAudioUnitProperty_MaximumFramesPerSlice,
            at::kAudioUnitScope_Global,
            ELEMENT_ZERO,
            &maximum_frames,
            "set_maximum_frames",
        )?;
        let actual_maximum: u32 = get_property(
            self.unit,
            at::kAudioUnitProperty_MaximumFramesPerSlice,
            at::kAudioUnitScope_Global,
            ELEMENT_ZERO,
            "read_maximum_frames",
        )?;
        if actual_maximum != maximum_frames {
            return Err(host_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "verify_maximum_frames",
                "Audio Unit did not accept the exact maximum process slice",
            ));
        }

        let format = linear_pcm_format(config.sample_rate(), config.input_layout().len());
        set_and_verify_format(
            self.unit,
            at::kAudioUnitScope_Input,
            config.input_layout(),
            &format,
            "input",
        )?;
        set_and_verify_format(
            self.unit,
            at::kAudioUnitScope_Output,
            config.output_layout(),
            &format,
            "output",
        )?;
        negotiate_layout(
            self.unit,
            at::kAudioUnitScope_Input,
            config.input_layout(),
            "input",
        )?;
        negotiate_layout(
            self.unit,
            at::kAudioUnitScope_Output,
            config.output_layout(),
            "output",
        )?;

        let callback = at::AURenderCallbackStruct {
            inputProc: Some(input_callback),
            inputProcRefCon: NonNull::from(self.callback.as_mut()).as_ptr().cast(),
        };
        set_property(
            self.unit,
            at::kAudioUnitProperty_SetRenderCallback,
            at::kAudioUnitScope_Input,
            ELEMENT_ZERO,
            &callback,
            "set_input_callback",
        )?;

        // SAFETY: The live unit has complete stream, layout, slice, and callback configuration.
        let status = unsafe { at::AudioUnitInitialize(self.unit) };
        check_status(status, "initialize_audio_unit")?;
        self.initialized = true;
        Ok(())
    }

    pub(super) fn process(
        &mut self,
        config: &AudioUnitHostConfig,
        block: AudioProcessBlock<'_>,
    ) -> Result<()> {
        validate_process(config, self.poisoned, &block)?;
        let input = block.input.expect("validated connected input");
        let channel_count = config.input_layout().len();
        let frame_count = block.frame_count;
        let byte_count = u32::try_from(frame_count * size_of::<f32>())
            .expect("configuration validated byte count");

        for channel in 0..channel_count {
            let plane_start = channel * config.maximum_frames();
            let plane = &mut self.input_planes[plane_start..plane_start + frame_count];
            for (frame, sample) in plane
                .iter_mut()
                .zip(input[channel..].iter().step_by(channel_count))
            {
                *frame = *sample;
            }

            let output_plane = &mut self.output_planes[plane_start..plane_start + frame_count];
            output_plane.fill(f32::NAN);
            self.output_buffers.buffers[channel] = ca::AudioBuffer {
                mNumberChannels: 1,
                mDataByteSize: byte_count,
                mData: output_plane.as_mut_ptr().cast(),
            };
        }
        self.output_buffers.number_buffers =
            u32::try_from(channel_count).expect("validated channel count");

        self.callback
            .begin(block.start_time.sample(), block.frame_count);
        let mut action_flags = at::AudioUnitRenderActionFlags(0);
        let mut timestamp = sample_timestamp(block.start_time.sample());
        // SAFETY: The unit is initialized, the timestamp and action flags are live for the call,
        // and the fixed buffer list advertises exactly the initialized planar output entries.
        let status = unsafe {
            at::AudioUnitRender(
                self.unit,
                &mut action_flags,
                NonNull::from(&mut timestamp),
                ELEMENT_ZERO,
                u32::try_from(frame_count).expect("validated frame count"),
                self.output_buffers.as_audio_buffer_list(),
            )
        };
        let callback_error = self.callback.finish();
        if status != 0 {
            self.poisoned = true;
            return Err(native_status_error(
                status,
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "render_audio_unit",
                "Audio Unit render failed",
            ));
        }
        if callback_error != CALLBACK_OK {
            self.poisoned = true;
            return Err(callback_error_value(callback_error));
        }

        let output_is_silence =
            action_flags.contains(at::AudioUnitRenderActionFlags::UnitRenderAction_OutputIsSilence);
        if self.output_buffers.number_buffers
            != u32::try_from(channel_count).expect("validated channel count")
        {
            self.poisoned = true;
            return Err(host_error(
                ErrorCategory::CorruptData,
                Recoverability::Terminal,
                "validate_render_output",
                "Audio Unit changed the prepared output buffer count",
            ));
        }
        if output_is_silence {
            for channel in 0..channel_count {
                let plane_start = channel * config.maximum_frames();
                self.output_planes[plane_start..plane_start + frame_count].fill(0.0);
            }
        }
        for channel in 0..channel_count {
            let plane_start = channel * config.maximum_frames();
            let output_plane = &self.output_planes[plane_start..plane_start + frame_count];
            let native_buffer = &self.output_buffers.buffers[channel];
            if !output_is_silence
                && (native_buffer.mNumberChannels != 1
                    || native_buffer.mDataByteSize != byte_count
                    || native_buffer.mData != output_plane.as_ptr().cast_mut().cast())
            {
                self.poisoned = true;
                return Err(host_error(
                    ErrorCategory::CorruptData,
                    Recoverability::Terminal,
                    "validate_render_output",
                    "Audio Unit changed the prepared output buffer contract",
                ));
            }
            if output_plane.iter().any(|sample| !sample.is_finite()) {
                self.poisoned = true;
                return Err(host_error(
                    ErrorCategory::Internal,
                    Recoverability::Terminal,
                    "validate_render_output",
                    "Audio Unit did not produce a complete finite output block",
                ));
            }
        }

        for (frame_index, output_frame) in block.output.chunks_exact_mut(channel_count).enumerate()
        {
            for (channel, output) in output_frame.iter_mut().enumerate() {
                *output = self.output_planes[channel * config.maximum_frames() + frame_index];
            }
        }
        Ok(())
    }
}

impl Drop for PreparedAudioUnit {
    fn drop(&mut self) {
        self.callback.active.store(false, Ordering::Release);
        if self.initialized {
            // SAFETY: This owner initialized the still-live unit and uninitializes it exactly once
            // before disposing it or releasing callback storage.
            let _ = unsafe { at::AudioUnitUninitialize(self.unit) };
            self.initialized = false;
        }
        // SAFETY: This owner received the instance with a +1 native lifetime and disposes it once,
        // while the registered callback context and plane storage are still live.
        let _ = unsafe { at::AudioComponentInstanceDispose(self.unit) };
    }
}

fn instantiate(
    component: at::AudioComponent,
    policy: AudioUnitExecutionPolicy,
) -> Result<at::AudioComponentInstance> {
    let options = match policy {
        AudioUnitExecutionPolicy::RequireOutOfProcess => {
            at::AudioComponentInstantiationOptions::LoadOutOfProcess
        }
        AudioUnitExecutionPolicy::AllowAuditedInProcess => {
            at::AudioComponentInstantiationOptions::LoadInProcess
        }
    };
    let shared = Arc::new((Mutex::new(InstantiationState::default()), Condvar::new()));
    let callback_shared = Arc::clone(&shared);
    let completion: RcBlock<dyn Fn(at::AudioComponentInstance, i32)> =
        RcBlock::new(move |instance: at::AudioComponentInstance, status: i32| {
            let (state, ready) = &*callback_shared;
            let mut state = match state.lock() {
                Ok(state) => state,
                Err(_) => {
                    if !instance.is_null() {
                        // SAFETY: A completion whose ownership state is unavailable cannot be
                        // transferred, so this callback disposes its native instance exactly once.
                        let _ = unsafe { at::AudioComponentInstanceDispose(instance) };
                    }
                    return;
                }
            };
            if state.abandoned || state.result.is_some() {
                drop(state);
                if !instance.is_null() {
                    // SAFETY: A late or duplicate completion cannot transfer ownership to the
                    // waiter, so this callback disposes its native instance exactly once.
                    let _ = unsafe { at::AudioComponentInstanceDispose(instance) };
                }
                return;
            }
            state.result = Some((SendAudioUnit(instance), status));
            ready.notify_one();
        });
    // SAFETY: `component` is the live exact match returned by discovery. The escaping block owns
    // its shared completion state, and the local copy stays live while this background thread waits.
    unsafe { at::AudioComponentInstantiate(component, options, &completion) };
    let (state, ready) = &*shared;
    let state = state.lock().map_err(|_| {
        host_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "instantiate_audio_unit",
            "Audio Unit completion state was poisoned",
        )
    })?;
    let (mut state, timeout) = ready
        .wait_timeout_while(state, INSTANTIATION_TIMEOUT, |state| state.result.is_none())
        .map_err(|_| {
            host_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "instantiate_audio_unit",
                "Audio Unit completion wait was poisoned",
            )
        })?;
    let Some((instance, status)) = state.result.take() else {
        state.abandoned = true;
        return Err(host_error(
            ErrorCategory::Timeout,
            Recoverability::Retryable,
            "instantiate_audio_unit",
            if timeout.timed_out() {
                "Audio Unit instantiation exceeded the bounded wait"
            } else {
                "Audio Unit instantiation ended without a completion result"
            },
        ));
    };
    let instance = instance.0;
    if status != 0 {
        if !instance.is_null() {
            // SAFETY: A failed completion did not transfer the instance into an owner, so this
            // path disposes the unexpected native value exactly once before returning the error.
            let _ = unsafe { at::AudioComponentInstanceDispose(instance) };
        }
        return Err(native_status_error(
            status,
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "instantiate_audio_unit",
            "AudioToolbox operation failed",
        ));
    }
    if instance.is_null() {
        return Err(host_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "instantiate_audio_unit",
            "Audio Unit instantiation succeeded without an instance",
        ));
    }
    Ok(instance)
}

fn validate_process(
    config: &AudioUnitHostConfig,
    poisoned: bool,
    block: &AudioProcessBlock<'_>,
) -> Result<()> {
    if poisoned {
        return Err(host_error(
            ErrorCategory::Conflict,
            Recoverability::Terminal,
            "process_audio_unit",
            "Audio Unit instance is poisoned after a native failure",
        ));
    }
    if block.start_time.sample_rate() != config.sample_rate() {
        return Err(invalid_process(
            "block sample rate does not match the prepared Audio Unit",
        ));
    }
    if block.frame_count == 0 || block.frame_count > config.maximum_frames() {
        return Err(invalid_process(
            "block frame count exceeds the prepared Audio Unit slice",
        ));
    }
    if block.input_layout != Some(config.input_layout())
        || block.output_layout != config.output_layout()
    {
        return Err(invalid_process(
            "block layouts do not match the prepared Audio Unit",
        ));
    }
    let expected_samples = block
        .frame_count
        .checked_mul(config.input_layout().len())
        .ok_or_else(|| invalid_process("Audio Unit block sample count overflowed"))?;
    let input = block
        .input
        .ok_or_else(|| invalid_process("Audio Unit effect requires one connected input"))?;
    if input.len() != expected_samples || block.output.len() != expected_samples {
        return Err(invalid_process(
            "Audio Unit buffers do not match the block frame count",
        ));
    }
    if input.iter().any(|sample| !sample.is_finite()) {
        return Err(invalid_process("Audio Unit input samples must be finite"));
    }
    let start = block.start_time.sample();
    let end = start
        .checked_add(i64::try_from(block.frame_count).expect("frame count fits i64"))
        .ok_or_else(|| invalid_process("Audio Unit sample range overflowed"))?;
    if start.unsigned_abs() > MAX_EXACT_F64_INTEGER as u64
        || end.unsigned_abs() > MAX_EXACT_F64_INTEGER as u64
    {
        return Err(invalid_process(
            "Audio Unit sample range is not exactly representable by AudioTimeStamp",
        ));
    }
    Ok(())
}

fn linear_pcm_format(sample_rate: u32, channel_count: usize) -> ca::AudioStreamBasicDescription {
    ca::AudioStreamBasicDescription {
        mSampleRate: f64::from(sample_rate),
        mFormatID: ca::kAudioFormatLinearPCM,
        mFormatFlags: ca::kAudioFormatFlagsNativeFloatPacked | ca::kAudioFormatFlagIsNonInterleaved,
        mBytesPerPacket: u32::try_from(size_of::<f32>()).expect("f32 size fits u32"),
        mFramesPerPacket: 1,
        mBytesPerFrame: u32::try_from(size_of::<f32>()).expect("f32 size fits u32"),
        mChannelsPerFrame: u32::try_from(channel_count).expect("validated channel count"),
        mBitsPerChannel: 32,
        mReserved: 0,
    }
}

fn set_and_verify_format(
    unit: at::AudioUnit,
    scope: at::AudioUnitScope,
    layout: &ChannelLayout,
    format: &ca::AudioStreamBasicDescription,
    direction: &'static str,
) -> Result<()> {
    set_property(
        unit,
        at::kAudioUnitProperty_StreamFormat,
        scope,
        ELEMENT_ZERO,
        format,
        "set_stream_format",
    )?;
    let actual: ca::AudioStreamBasicDescription = get_property(
        unit,
        at::kAudioUnitProperty_StreamFormat,
        scope,
        ELEMENT_ZERO,
        "read_stream_format",
    )?;
    if actual != *format || actual.mChannelsPerFrame as usize != layout.len() {
        return Err(host_error(
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            "verify_stream_format",
            match direction {
                "input" => "Audio Unit did not accept the exact input stream format",
                _ => "Audio Unit did not accept the exact output stream format",
            },
        ));
    }
    Ok(())
}

fn negotiate_layout(
    unit: at::AudioUnit,
    scope: at::AudioUnitScope,
    layout: &ChannelLayout,
    direction: &'static str,
) -> Result<()> {
    let mut property_size = 0_u32;
    let mut writable = 0_u8;
    // SAFETY: `unit` is live and both output values remain writable for the synchronous query.
    let status = unsafe {
        at::AudioUnitGetPropertyInfo(
            unit,
            at::kAudioUnitProperty_AudioChannelLayout,
            scope,
            ELEMENT_ZERO,
            &mut property_size,
            &mut writable,
        )
    };
    if status != 0 {
        if layout.len() <= 2 {
            return Ok(());
        }
        return Err(native_status_error(
            status,
            ErrorCategory::Unsupported,
            Recoverability::UserCorrectable,
            "query_channel_layout",
            "Audio Unit cannot prove semantic meaning for the requested multichannel layout",
        ));
    }
    let capacity = u32::try_from(size_of::<FixedChannelLayout>())
        .expect("fixed channel layout capacity fits u32");
    if property_size > capacity {
        return Err(host_error(
            ErrorCategory::ResourceExhausted,
            Recoverability::UserCorrectable,
            "query_channel_layout",
            "Audio Unit channel-layout property exceeds the audited capacity",
        ));
    }
    let requested = FixedChannelLayout::from_layout(layout)?;
    if writable != 0 {
        // SAFETY: The fixed layout is C-compatible with AudioChannelLayout, remains live for the
        // synchronous call, and the supplied size covers exactly its initialized descriptions.
        let status = unsafe {
            at::AudioUnitSetProperty(
                unit,
                at::kAudioUnitProperty_AudioChannelLayout,
                scope,
                ELEMENT_ZERO,
                ptr::addr_of!(requested).cast(),
                FixedChannelLayout::byte_size(layout.len()),
            )
        };
        check_status(status, "set_channel_layout")?;
    }
    let mut actual = FixedChannelLayout::from_layout(layout)?;
    let mut actual_size = capacity;
    // SAFETY: `actual` is aligned writable storage for the full audited capacity and
    // `actual_size` remains live for the synchronous readback.
    let status = unsafe {
        at::AudioUnitGetProperty(
            unit,
            at::kAudioUnitProperty_AudioChannelLayout,
            scope,
            ELEMENT_ZERO,
            NonNull::from(&mut actual).cast(),
            NonNull::from(&mut actual_size),
        )
    };
    check_status(status, "read_channel_layout")?;
    if !actual.matches(layout, actual_size) {
        return Err(host_error(
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            "verify_channel_layout",
            match direction {
                "input" => "Audio Unit did not preserve the exact input channel meaning",
                _ => "Audio Unit did not preserve the exact output channel meaning",
            },
        ));
    }
    Ok(())
}

fn set_property<T>(
    unit: at::AudioUnit,
    property: at::AudioUnitPropertyID,
    scope: at::AudioUnitScope,
    element: at::AudioUnitElement,
    value: &T,
    operation: &'static str,
) -> Result<()> {
    let size = u32::try_from(size_of::<T>()).expect("native property type size fits u32");
    // SAFETY: `unit` is live and `value` remains readable for the complete synchronous call with
    // its exact Rust/C binding size.
    let status = unsafe {
        at::AudioUnitSetProperty(
            unit,
            property,
            scope,
            element,
            ptr::from_ref(value).cast(),
            size,
        )
    };
    check_status(status, operation)
}

fn get_property<T: Copy>(
    unit: at::AudioUnit,
    property: at::AudioUnitPropertyID,
    scope: at::AudioUnitScope,
    element: at::AudioUnitElement,
    operation: &'static str,
) -> Result<T> {
    let mut value = MaybeUninit::<T>::uninit();
    let expected_size = u32::try_from(size_of::<T>()).expect("native property type size fits u32");
    let mut actual_size = expected_size;
    // SAFETY: `unit` is live, `value` is aligned writable storage for `T`, and `actual_size`
    // advertises that exact capacity for the synchronous property read.
    let status = unsafe {
        at::AudioUnitGetProperty(
            unit,
            property,
            scope,
            element,
            NonNull::new_unchecked(value.as_mut_ptr().cast()),
            NonNull::from(&mut actual_size),
        )
    };
    check_status(status, operation)?;
    if actual_size != expected_size {
        return Err(host_error(
            ErrorCategory::CorruptData,
            Recoverability::Terminal,
            operation,
            "Audio Unit property returned an unexpected value size",
        ));
    }
    // SAFETY: A successful exact-size property read initialized the complete `T` value.
    Ok(unsafe { value.assume_init() })
}

fn sample_timestamp(sample: i64) -> ca::AudioTimeStamp {
    ca::AudioTimeStamp {
        mSampleTime: sample as f64,
        mHostTime: 0,
        mRateScalar: 0.0,
        mWordClockTime: 0,
        mSMPTETime: ca::SMPTETime {
            mSubframes: 0,
            mSubframeDivisor: 0,
            mCounter: 0,
            mType: ca::SMPTETimeType(0),
            mFlags: ca::SMPTETimeFlags(0),
            mHours: 0,
            mMinutes: 0,
            mSeconds: 0,
            mFrames: 0,
        },
        mFlags: ca::AudioTimeStampFlags::SampleTimeValid,
        mReserved: 0,
    }
}

unsafe extern "C-unwind" fn input_callback(
    reference: NonNull<c_void>,
    action_flags: NonNull<at::AudioUnitRenderActionFlags>,
    timestamp: NonNull<ca::AudioTimeStamp>,
    bus: u32,
    frame_count: u32,
    buffers: *mut ca::AudioBufferList,
) -> i32 {
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: AudioToolbox calls this function only with the stable registered context and
        // live callback arguments during the synchronous render owned by `process`.
        unsafe {
            input_callback_inner(
                reference,
                action_flags,
                timestamp,
                bus,
                frame_count,
                buffers,
            )
        }
    }));
    match result {
        Ok(status) => status,
        Err(_) => {
            // SAFETY: The same registration invariant guarantees the context remains live until
            // the native unit is uninitialized and disposed.
            let context = unsafe { reference.cast::<CallbackContext>().as_ref() };
            context.record_error(CALLBACK_PANICKED);
            at::kAudioUnitErr_CannotDoInCurrentContext
        }
    }
}

unsafe fn input_callback_inner(
    reference: NonNull<c_void>,
    mut action_flags: NonNull<at::AudioUnitRenderActionFlags>,
    timestamp: NonNull<ca::AudioTimeStamp>,
    bus: u32,
    frame_count: u32,
    buffers: *mut ca::AudioBufferList,
) -> i32 {
    // SAFETY: The callback registration stores a stable boxed `CallbackContext` at this address.
    let context = unsafe { reference.cast::<CallbackContext>().as_ref() };
    if !context.active.load(Ordering::Acquire) {
        return callback_failure(context, CALLBACK_INACTIVE);
    }
    if bus != ELEMENT_ZERO {
        return callback_failure(context, CALLBACK_WRONG_BUS);
    }
    // SAFETY: AudioToolbox provides a live timestamp for the complete callback invocation.
    let timestamp = unsafe { timestamp.as_ref() };
    if !timestamp
        .mFlags
        .contains(ca::AudioTimeStampFlags::SampleTimeValid)
        || !timestamp.mSampleTime.is_finite()
        || timestamp.mSampleTime.fract() != 0.0
        || timestamp.mSampleTime.abs() > MAX_EXACT_F64_INTEGER as f64
    {
        return callback_failure(context, CALLBACK_INVALID_TIMESTAMP);
    }
    let requested_start = timestamp.mSampleTime as i64;
    let requested_frames = frame_count as usize;
    let active_start = context.start_sample.load(Ordering::Relaxed);
    let active_frames = context.frame_count.load(Ordering::Relaxed);
    let Some(active_end) = active_start.checked_add(active_frames as i64) else {
        return callback_failure(context, CALLBACK_OUT_OF_RANGE);
    };
    let Some(requested_end) = requested_start.checked_add(i64::from(frame_count)) else {
        return callback_failure(context, CALLBACK_OUT_OF_RANGE);
    };
    if requested_frames > context.maximum_frames
        || requested_start < active_start
        || requested_end > active_end
    {
        return callback_failure(context, CALLBACK_OUT_OF_RANGE);
    }
    let Some(buffer_list) = NonNull::new(buffers) else {
        return callback_failure(context, CALLBACK_INVALID_BUFFER_LIST);
    };
    // SAFETY: The nonnull list is live for the callback and its count field is readable.
    let buffer_list_ref = unsafe { buffer_list.as_ref() };
    if buffer_list_ref.mNumberBuffers as usize != context.channel_count {
        return callback_failure(context, CALLBACK_INVALID_BUFFER_LIST);
    }
    let byte_count = match requested_frames
        .checked_mul(size_of::<f32>())
        .and_then(|bytes| u32::try_from(bytes).ok())
    {
        Some(bytes) => bytes,
        None => return callback_failure(context, CALLBACK_INVALID_BUFFER),
    };
    let frame_offset = (requested_start - active_start) as usize;
    // SAFETY: A list advertising the validated channel count was allocated by AudioToolbox for
    // that many flexible-array entries. The first entry address is the start of that array.
    let first_buffer = unsafe {
        buffer_list
            .as_ptr()
            .cast::<u8>()
            .add(offset_of!(ca::AudioBufferList, mBuffers))
            .cast::<ca::AudioBuffer>()
    };
    for channel in 0..context.channel_count {
        // SAFETY: `channel` is bounded by the native list's validated buffer count.
        let buffer = unsafe { &mut *first_buffer.add(channel) };
        if buffer.mNumberChannels != 1 {
            return callback_failure(context, CALLBACK_INVALID_BUFFER);
        }
        let source_base = context.input_planes[channel].load(Ordering::Relaxed);
        if source_base.is_null() {
            return callback_failure(context, CALLBACK_INVALID_BUFFER);
        }
        let source = source_base.cast_const().wrapping_add(frame_offset);
        if buffer.mData.is_null() {
            buffer.mData = source.cast_mut().cast();
        } else {
            if buffer.mDataByteSize < byte_count {
                return callback_failure(context, CALLBACK_INVALID_BUFFER);
            }
            // SAFETY: The source subrange is bounded by the published active window. The native
            // destination advertises at least `byte_count`, and `copy` permits possible overlap.
            unsafe { ptr::copy(source, buffer.mData.cast::<f32>(), requested_frames) };
        }
        buffer.mDataByteSize = byte_count;
    }
    // SAFETY: The action-flags pointer is live and uniquely writable for this callback invocation.
    unsafe {
        action_flags.as_mut().0 &=
            !at::AudioUnitRenderActionFlags::UnitRenderAction_OutputIsSilence.0
    };
    0
}

fn callback_failure(context: &CallbackContext, code: i32) -> i32 {
    context.record_error(code);
    at::kAudioUnitErr_CannotDoInCurrentContext
}

fn callback_error_value(code: i32) -> Error {
    let message = match code {
        CALLBACK_INACTIVE => "Audio Unit requested input outside the active render window",
        CALLBACK_WRONG_BUS => "Audio Unit requested an unsupported input bus",
        CALLBACK_INVALID_TIMESTAMP => "Audio Unit requested input with an invalid sample timestamp",
        CALLBACK_OUT_OF_RANGE => "Audio Unit requested input outside the active sample range",
        CALLBACK_INVALID_BUFFER_LIST => "Audio Unit requested input with an invalid buffer list",
        CALLBACK_INVALID_BUFFER => "Audio Unit requested input with an invalid channel buffer",
        CALLBACK_PANICKED => "Audio Unit input callback panicked and was contained",
        _ => "Audio Unit input callback reported an unknown failure",
    };
    host_error(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "provide_render_input",
        message,
    )
    .with_context(
        ErrorContext::new(COMPONENT, "callback_failure").with_field("code", code.to_string()),
    )
}

fn channel_label(position: ChannelPosition) -> Option<ca::AudioChannelLabel> {
    Some(match position {
        ChannelPosition::FrontLeft => ca::kAudioChannelLabel_Left,
        ChannelPosition::FrontRight => ca::kAudioChannelLabel_Right,
        ChannelPosition::FrontCenter => ca::kAudioChannelLabel_Center,
        ChannelPosition::LowFrequency => ca::kAudioChannelLabel_LFEScreen,
        ChannelPosition::BackLeft => ca::kAudioChannelLabel_RearSurroundLeft,
        ChannelPosition::BackRight => ca::kAudioChannelLabel_RearSurroundRight,
        ChannelPosition::FrontLeftOfCenter => ca::kAudioChannelLabel_LeftCenter,
        ChannelPosition::FrontRightOfCenter => ca::kAudioChannelLabel_RightCenter,
        ChannelPosition::BackCenter => ca::kAudioChannelLabel_CenterSurround,
        ChannelPosition::SideLeft => ca::kAudioChannelLabel_LeftSurround,
        ChannelPosition::SideRight => ca::kAudioChannelLabel_RightSurround,
        ChannelPosition::TopCenter => ca::kAudioChannelLabel_TopCenterSurround,
        ChannelPosition::TopFrontLeft => ca::kAudioChannelLabel_VerticalHeightLeft,
        ChannelPosition::TopFrontCenter => ca::kAudioChannelLabel_VerticalHeightCenter,
        ChannelPosition::TopFrontRight => ca::kAudioChannelLabel_VerticalHeightRight,
        ChannelPosition::TopBackLeft => ca::kAudioChannelLabel_TopBackLeft,
        ChannelPosition::TopBackCenter => ca::kAudioChannelLabel_TopBackCenter,
        ChannelPosition::TopBackRight => ca::kAudioChannelLabel_TopBackRight,
        ChannelPosition::Discrete(index) => (1 << 16) | u32::from(index),
        _ => return None,
    })
}

fn check_status(status: i32, operation: &'static str) -> Result<()> {
    if status == 0 {
        return Ok(());
    }
    Err(native_status_error(
        status,
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        operation,
        "AudioToolbox operation failed",
    ))
}

fn native_status_error(
    status: i32,
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
) -> Error {
    host_error(category, recoverability, operation, message).with_context(
        ErrorContext::new(COMPONENT, "native_status").with_field("os_status", status.to_string()),
    )
}

fn invalid_process(message: &'static str) -> Error {
    host_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "process_audio_unit",
        message,
    )
}

fn host_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn planar_input() -> Vec<f32> {
        (0..8)
            .map(|sample| sample as f32)
            .chain((100..108).map(|sample| sample as f32))
            .collect()
    }

    fn timestamp(sample: i64) -> ca::AudioTimeStamp {
        sample_timestamp(sample)
    }

    #[test]
    fn callback_serves_repeated_bounded_subranges_into_native_storage() {
        let input = planar_input();
        let context = CallbackContext::new(&input, 2, 8);
        context.begin(100, 8);
        let mut left = vec![-1.0_f32; 3];
        let mut right = vec![-1.0_f32; 3];
        let mut buffers = FixedAudioBufferList::new(2);
        buffers.buffers[0] = ca::AudioBuffer {
            mNumberChannels: 1,
            mDataByteSize: 12,
            mData: left.as_mut_ptr().cast(),
        };
        buffers.buffers[1] = ca::AudioBuffer {
            mNumberChannels: 1,
            mDataByteSize: 12,
            mData: right.as_mut_ptr().cast(),
        };
        assert_eq!(
            offset_of!(FixedAudioBufferList, buffers),
            offset_of!(ca::AudioBufferList, mBuffers)
        );
        assert_eq!(
            // SAFETY: The context stores the base of the still-live first input plane.
            unsafe {
                std::slice::from_raw_parts(
                    context.input_planes[0].load(Ordering::Relaxed).cast_const(),
                    8,
                )
            },
            &[0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0]
        );
        let mut flags = at::AudioUnitRenderActionFlags(
            at::AudioUnitRenderActionFlags::UnitRenderAction_OutputIsSilence.0,
        );
        let mut time = timestamp(102);
        for _ in 0..2 {
            // SAFETY: The test publishes a live active context, timestamp, flags value, and fixed
            // two-entry buffer list for the complete callback invocation.
            let status = unsafe {
                input_callback_inner(
                    NonNull::from(&context).cast(),
                    NonNull::from(&mut flags),
                    NonNull::from(&mut time),
                    0,
                    3,
                    buffers.as_audio_buffer_list().as_ptr(),
                )
            };
            assert_eq!(status, 0);
            assert_eq!(left, [2.0, 3.0, 4.0]);
            assert_eq!(right, [102.0, 103.0, 104.0]);
        }
        assert!(!flags.contains(at::AudioUnitRenderActionFlags::UnitRenderAction_OutputIsSilence));
        assert_eq!(context.finish(), CALLBACK_OK);
    }

    #[test]
    fn callback_can_publish_host_planes_and_rejects_invalid_requests() {
        let input = planar_input();
        let context = CallbackContext::new(&input, 2, 8);
        context.begin(200, 8);
        let mut buffers = FixedAudioBufferList::new(2);
        let mut flags = at::AudioUnitRenderActionFlags(0);
        let mut time = timestamp(204);
        // SAFETY: The test publishes a live active context and a fixed two-entry list whose null
        // data pointers explicitly request host-owned plane addresses.
        let status = unsafe {
            input_callback_inner(
                NonNull::from(&context).cast(),
                NonNull::from(&mut flags),
                NonNull::from(&mut time),
                0,
                2,
                buffers.as_audio_buffer_list().as_ptr(),
            )
        };
        assert_eq!(status, 0);
        for (channel, expected) in [[4.0, 5.0], [104.0, 105.0]].iter().enumerate() {
            // SAFETY: A successful callback assigned a pointer into the still-live input plane for
            // exactly the two frames advertised in the corresponding buffer entry.
            let actual = unsafe {
                std::slice::from_raw_parts(buffers.buffers[channel].mData.cast::<f32>(), 2)
            };
            assert_eq!(actual, expected);
            assert_eq!(buffers.buffers[channel].mDataByteSize, 8);
        }
        assert_eq!(context.finish(), CALLBACK_OK);

        context.begin(200, 8);
        time = timestamp(207);
        // SAFETY: All callback pointers remain live, but the requested three-frame range is
        // intentionally outside the published window and must be rejected before buffer access.
        let status = unsafe {
            input_callback_inner(
                NonNull::from(&context).cast(),
                NonNull::from(&mut flags),
                NonNull::from(&mut time),
                0,
                3,
                buffers.as_audio_buffer_list().as_ptr(),
            )
        };
        assert_eq!(status, at::kAudioUnitErr_CannotDoInCurrentContext);
        assert_eq!(context.finish(), CALLBACK_OUT_OF_RANGE);

        context.begin(200, 8);
        time = timestamp(200);
        // SAFETY: All callback pointers remain live, while bus one is intentionally outside the
        // prepared single-input effect contract.
        let status = unsafe {
            input_callback_inner(
                NonNull::from(&context).cast(),
                NonNull::from(&mut flags),
                NonNull::from(&mut time),
                1,
                2,
                buffers.as_audio_buffer_list().as_ptr(),
            )
        };
        assert_eq!(status, at::kAudioUnitErr_CannotDoInCurrentContext);
        assert_eq!(context.finish(), CALLBACK_WRONG_BUS);
    }
}
