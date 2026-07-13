//! Narrow private runtime boundary for the official libvpx 1.16 C ABI.
//!
//! Safe codec code enters through `Runtime`, `DecoderHandle`, and `EncoderHandle`. This module
//! owns the loaded library, opaque contexts, checked C shim, and every conversion from native
//! pointers into Rust values.

#![allow(unsafe_code)]

use std::env;
use std::ffi::{c_char, c_int, c_void, CStr, OsString};
use std::fmt;
use std::ptr::{self, NonNull};
use std::sync::Arc;

use libloading::Library;

const REQUIRED_VERSION_PREFIX: &str = "v1.16.";
const FRAME_IS_KEY: u32 = 0x1;
const FRAME_IS_INVISIBLE: u32 = 0x4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub(crate) enum Codec {
    Vp8 = 8,
    Vp9 = 9,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(i32)]
pub(crate) enum Format {
    I420_8 = 1,
    I422_8 = 2,
    I444_8 = 3,
    I420_10 = 11,
    I422_10 = 12,
    I444_10 = 13,
}

impl Format {
    fn from_native(value: i32) -> Option<Self> {
        match value {
            1 => Some(Self::I420_8),
            2 => Some(Self::I422_8),
            3 => Some(Self::I444_8),
            11 => Some(Self::I420_10),
            12 => Some(Self::I422_10),
            13 => Some(Self::I444_10),
            _ => None,
        }
    }

    pub(crate) const fn bits_per_sample(self) -> usize {
        match self {
            Self::I420_8 | Self::I422_8 | Self::I444_8 => 1,
            Self::I420_10 | Self::I422_10 | Self::I444_10 => 2,
        }
    }

    fn chroma_shifts(self) -> (u32, u32) {
        match self {
            Self::I420_8 | Self::I420_10 => (1, 1),
            Self::I422_8 | Self::I422_10 => (1, 0),
            Self::I444_8 | Self::I444_10 => (0, 0),
        }
    }

    fn plane_layout(self, width: u32, height: u32) -> Result<Vec<PlaneLayout>, FfiError> {
        let bytes = self.bits_per_sample();
        let width =
            usize::try_from(width).map_err(|_| FfiError::internal("frame width overflow"))?;
        let luma_row_bytes = width
            .checked_mul(bytes)
            .ok_or_else(|| FfiError::internal("frame row size overflow"))?;
        let (horizontal, vertical) = self.chroma_shifts();
        let chroma_width = ceil_shift(width, horizontal);
        let chroma_height = ceil_shift(
            usize::try_from(height).map_err(|_| FfiError::internal("frame height overflow"))?,
            vertical,
        );
        let chroma_row_bytes = chroma_width
            .checked_mul(bytes)
            .ok_or_else(|| FfiError::internal("chroma row size overflow"))?;
        Ok(vec![
            PlaneLayout {
                row_bytes: luma_row_bytes,
                rows: height,
            },
            PlaneLayout {
                row_bytes: chroma_row_bytes,
                rows: u32::try_from(chroma_height)
                    .map_err(|_| FfiError::internal("chroma height overflow"))?,
            },
            PlaneLayout {
                row_bytes: chroma_row_bytes,
                rows: u32::try_from(chroma_height)
                    .map_err(|_| FfiError::internal("chroma height overflow"))?,
            },
        ])
    }
}

fn ceil_shift(value: usize, shift: u32) -> usize {
    if shift == 0 {
        value
    } else {
        value.div_ceil(1_usize << shift)
    }
}

#[derive(Clone, Copy, Debug)]
struct PlaneLayout {
    row_bytes: usize,
    rows: u32,
}

#[derive(Debug)]
pub(crate) struct RawFrame {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) format: Format,
    pub(crate) bit_depth: u32,
    pub(crate) color_space: i32,
    pub(crate) color_range: i32,
    pub(crate) planes: Vec<Vec<u8>>,
}

#[derive(Debug)]
pub(crate) struct RawPacket {
    pub(crate) data: Vec<u8>,
    pub(crate) pts: i64,
    pub(crate) duration: u64,
    pub(crate) keyframe: bool,
    pub(crate) invisible: bool,
}

#[derive(Debug)]
pub(crate) struct FfiError {
    message: String,
}

impl FfiError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    fn internal(message: &'static str) -> Self {
        Self::new(message)
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for FfiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for FfiError {}

#[derive(Clone, Copy)]
#[repr(C)]
struct VpxApi {
    codec_err_to_string: *mut c_void,
    codec_error: *mut c_void,
    codec_control: *mut c_void,
    codec_vp8_dx: *mut c_void,
    codec_vp9_dx: *mut c_void,
    codec_dec_init_ver: *mut c_void,
    codec_decode: *mut c_void,
    codec_get_frame: *mut c_void,
    codec_vp8_cx: *mut c_void,
    codec_vp9_cx: *mut c_void,
    codec_enc_config_default: *mut c_void,
    codec_enc_init_ver: *mut c_void,
    codec_encode: *mut c_void,
    codec_get_cx_data: *mut c_void,
    codec_destroy: *mut c_void,
    img_alloc: *mut c_void,
    img_free: *mut c_void,
}

// SAFETY: The table contains immutable function addresses owned by the retained Library.
unsafe impl Send for VpxApi {}
// SAFETY: Every libvpx context is separately owned, and the immutable addresses are safe to share.
unsafe impl Sync for VpxApi {}

pub(crate) struct Runtime {
    api: VpxApi,
    version: String,
    _library: Library,
}

impl Runtime {
    pub(crate) fn load() -> Result<Arc<Self>, FfiError> {
        let mut incompatible = Vec::new();
        for candidate in library_candidates() {
            // SAFETY: The loaded library remains owned by Runtime for at least as long as every
            // copied symbol address, and `from_library` rejects an incompatible libvpx ABI.
            let loaded = unsafe { Library::new(&candidate) };
            let Ok(library) = loaded else {
                continue;
            };
            match Self::from_library(library) {
                Ok(runtime) => return Ok(Arc::new(runtime)),
                Err(error) => {
                    incompatible.push(format!("{}: {error}", candidate.to_string_lossy()))
                }
            }
        }
        let detail = if incompatible.is_empty() {
            "no libvpx runtime was found".to_owned()
        } else {
            format!(
                "no compatible libvpx runtime was found ({})",
                incompatible.join("; ")
            )
        };
        Err(FfiError::new(detail))
    }

    fn from_library(library: Library) -> Result<Self, FfiError> {
        type VersionFn = unsafe extern "C" fn() -> *const c_char;
        // SAFETY: Official libvpx exports this exact no-argument C signature. The library remains
        // live in this function and is retained in Runtime after validation.
        let version_fn = unsafe { library.get::<VersionFn>(b"vpx_codec_version_str\0") }
            .map_err(|error| FfiError::new(format!("missing version symbol: {error}")))?;
        // SAFETY: The symbol type above matches the libvpx public ABI and the library is live.
        let version_pointer = unsafe { version_fn() };
        if version_pointer.is_null() {
            return Err(FfiError::new("libvpx returned no version string"));
        }
        // SAFETY: libvpx returned a nonnull pointer to its static NUL-terminated version string.
        let version = unsafe { CStr::from_ptr(version_pointer) }
            .to_str()
            .map_err(|_| FfiError::new("libvpx returned a non-UTF-8 version string"))?
            .to_owned();
        if !version.starts_with(REQUIRED_VERSION_PREFIX) {
            return Err(FfiError::new(format!(
                "runtime {version} does not match required libvpx 1.16 ABI"
            )));
        }
        let api = VpxApi {
            codec_err_to_string: function_pointer(&library, b"vpx_codec_err_to_string\0")?,
            codec_error: function_pointer(&library, b"vpx_codec_error\0")?,
            codec_control: function_pointer(&library, b"vpx_codec_control_\0")?,
            codec_vp8_dx: function_pointer(&library, b"vpx_codec_vp8_dx\0")?,
            codec_vp9_dx: function_pointer(&library, b"vpx_codec_vp9_dx\0")?,
            codec_dec_init_ver: function_pointer(&library, b"vpx_codec_dec_init_ver\0")?,
            codec_decode: function_pointer(&library, b"vpx_codec_decode\0")?,
            codec_get_frame: function_pointer(&library, b"vpx_codec_get_frame\0")?,
            codec_vp8_cx: function_pointer(&library, b"vpx_codec_vp8_cx\0")?,
            codec_vp9_cx: function_pointer(&library, b"vpx_codec_vp9_cx\0")?,
            codec_enc_config_default: function_pointer(
                &library,
                b"vpx_codec_enc_config_default\0",
            )?,
            codec_enc_init_ver: function_pointer(&library, b"vpx_codec_enc_init_ver\0")?,
            codec_encode: function_pointer(&library, b"vpx_codec_encode\0")?,
            codec_get_cx_data: function_pointer(&library, b"vpx_codec_get_cx_data\0")?,
            codec_destroy: function_pointer(&library, b"vpx_codec_destroy\0")?,
            img_alloc: function_pointer(&library, b"vpx_img_alloc\0")?,
            img_free: function_pointer(&library, b"vpx_img_free\0")?,
        };
        Ok(Self {
            api,
            version,
            _library: library,
        })
    }

    pub(crate) fn version(&self) -> &str {
        &self.version
    }

    pub(crate) fn decoder(self: &Arc<Self>, codec: Codec) -> Result<DecoderHandle, FfiError> {
        let mut native = ptr::null_mut();
        // SAFETY: The retained API table is ABI-validated, `native` is writable output storage,
        // and the shim owns all concrete libvpx state created by this call.
        let status =
            unsafe { superi_vpx_decoder_create(&self.api, codec as c_int, 1, &mut native) };
        if status != 0 {
            return Err(self.status_error(status, None));
        }
        let native = NonNull::new(native)
            .ok_or_else(|| FfiError::internal("libvpx decoder creation returned no context"))?;
        Ok(DecoderHandle {
            native,
            runtime: Arc::clone(self),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn encoder(
        self: &Arc<Self>,
        codec: Codec,
        format: Format,
        width: u32,
        height: u32,
        timebase_numerator: u32,
        timebase_denominator: u32,
        bitrate_kbps: u32,
    ) -> Result<EncoderHandle, FfiError> {
        let mut native = ptr::null_mut();
        // SAFETY: All dimensions and timebase values use the shim's exact integer ABI, the API
        // table is retained, and `native` is writable output storage for one owned context.
        let status = unsafe {
            superi_vpx_encoder_create(
                &self.api,
                codec as c_int,
                format as c_int,
                width,
                height,
                timebase_numerator,
                timebase_denominator,
                bitrate_kbps,
                1,
                &mut native,
            )
        };
        if status != 0 {
            return Err(self.status_error(status, None));
        }
        let native = NonNull::new(native)
            .ok_or_else(|| FfiError::internal("libvpx encoder creation returned no context"))?;
        Ok(EncoderHandle {
            native,
            runtime: Arc::clone(self),
        })
    }

    fn status_error(&self, status: c_int, detail: Option<&CStr>) -> FfiError {
        // SAFETY: The API table belongs to the retained runtime and the shim returns either a
        // static message or a libvpx string whose library remains loaded.
        let summary = unsafe { superi_vpx_status_string(&self.api, status) };
        let summary = c_string(summary).unwrap_or_else(|| "unknown libvpx error".to_owned());
        match detail.and_then(|value| value.to_str().ok()) {
            Some(detail) if detail != summary => FfiError::new(format!("{summary}: {detail}")),
            _ => FfiError::new(summary),
        }
    }
}

fn function_pointer(library: &Library, name: &'static [u8]) -> Result<*mut c_void, FfiError> {
    type UntypedFn = unsafe extern "C" fn();
    // SAFETY: Every requested name is an official libvpx 1.16 function symbol. Rust never calls
    // this erased signature; the address is passed to the C shim, which restores the exact type.
    let function = *unsafe { library.get::<UntypedFn>(name) }.map_err(|error| {
        FfiError::new(format!(
            "missing {}: {error}",
            String::from_utf8_lossy(&name[..name.len().saturating_sub(1)])
        ))
    })?;
    Ok(function as *const () as *mut c_void)
}

fn c_string(value: *const c_char) -> Option<String> {
    if value.is_null() {
        None
    } else {
        // SAFETY: Callers pass only libvpx or shim error strings, which remain NUL-terminated and
        // valid while the retained runtime and native context are live.
        unsafe { CStr::from_ptr(value) }
            .to_str()
            .ok()
            .map(str::to_owned)
    }
}

fn library_candidates() -> Vec<OsString> {
    if let Some(explicit) = env::var_os("SUPERI_LIBVPX_PATH") {
        return vec![explicit];
    }
    let mut candidates = Vec::new();
    if let Ok(executable) = env::current_exe() {
        if let Some(directory) = executable.parent() {
            candidates.extend(
                platform_names()
                    .iter()
                    .map(|name| directory.join(name).into()),
            );
        }
    }
    candidates.extend(platform_names().iter().map(OsString::from));
    if cfg!(target_os = "macos") {
        candidates.extend([
            OsString::from("/opt/homebrew/opt/libvpx/lib/libvpx.12.dylib"),
            OsString::from("/usr/local/opt/libvpx/lib/libvpx.12.dylib"),
        ]);
    }
    let mut unique = Vec::new();
    for candidate in candidates {
        if !unique.contains(&candidate) {
            unique.push(candidate);
        }
    }
    unique
}

fn platform_names() -> &'static [&'static str] {
    if cfg!(target_os = "windows") {
        &["vpx.dll", "libvpx.dll"]
    } else if cfg!(target_os = "macos") {
        &["libvpx.12.dylib", "libvpx.dylib"]
    } else {
        &["libvpx.so.12", "libvpx.so"]
    }
}

pub(crate) struct DecoderHandle {
    native: NonNull<NativeDecoder>,
    runtime: Arc<Runtime>,
}

// SAFETY: The context has one owner and is never used concurrently.
unsafe impl Send for DecoderHandle {}

impl DecoderHandle {
    pub(crate) fn decode(&mut self, data: &[u8]) -> Result<Vec<RawFrame>, FfiError> {
        let data_pointer = if data.is_empty() {
            ptr::null()
        } else {
            data.as_ptr()
        };
        // SAFETY: The handle uniquely owns a live shim decoder, and data_pointer is null only for
        // an empty slice or otherwise covers exactly data.len() readable bytes.
        let status =
            unsafe { superi_vpx_decoder_decode(self.native.as_ptr(), data_pointer, data.len()) };
        if status != 0 {
            return Err(self.runtime.status_error(status, self.error_detail()));
        }
        self.drain()
    }

    fn drain(&mut self) -> Result<Vec<RawFrame>, FfiError> {
        let mut frames = Vec::new();
        loop {
            let mut info = NativeFrameInfo::default();
            // SAFETY: The decoder is live and uniquely borrowed, and info is writable output
            // storage with the layout declared in vpx_shim.h.
            let status = unsafe { superi_vpx_decoder_next(self.native.as_ptr(), &mut info) };
            if status == 0 {
                return Ok(frames);
            }
            if status != 1 {
                return Err(self.runtime.status_error(status, self.error_detail()));
            }
            let format = Format::from_native(info.format)
                .ok_or_else(|| FfiError::internal("libvpx returned an unknown image format"))?;
            let mut planes = Vec::with_capacity(3);
            for (plane, layout) in format
                .plane_layout(info.width, info.height)?
                .into_iter()
                .enumerate()
            {
                let size = layout
                    .row_bytes
                    .checked_mul(
                        usize::try_from(layout.rows)
                            .map_err(|_| FfiError::internal("frame plane row count overflow"))?,
                    )
                    .ok_or_else(|| FfiError::internal("frame plane size overflow"))?;
                let mut bytes = vec![0_u8; size];
                // SAFETY: bytes owns layout.row_bytes times layout.rows writable bytes, and the
                // checked shim copies only the requested visible plane into that exact geometry.
                let status = unsafe {
                    superi_vpx_decoder_copy_plane(
                        self.native.as_ptr(),
                        u32::try_from(plane)
                            .map_err(|_| FfiError::internal("frame plane index overflow"))?,
                        bytes.as_mut_ptr(),
                        layout.row_bytes,
                        layout.rows,
                        layout.row_bytes,
                    )
                };
                if status != 0 {
                    return Err(self.runtime.status_error(status, self.error_detail()));
                }
                planes.push(bytes);
            }
            frames.push(RawFrame {
                width: info.width,
                height: info.height,
                format,
                bit_depth: info.bit_depth,
                color_space: info.color_space,
                color_range: info.color_range,
                planes,
            });
        }
    }

    fn error_detail(&self) -> Option<&CStr> {
        // SAFETY: The handle owns a live decoder and the returned message is borrowed only while
        // that context and its retained runtime remain live.
        let value = unsafe { superi_vpx_decoder_error(self.native.as_ptr()) };
        if value.is_null() {
            None
        } else {
            // SAFETY: The checked shim returns a NUL-terminated libvpx or static error string.
            Some(unsafe { CStr::from_ptr(value) })
        }
    }
}

impl Drop for DecoderHandle {
    fn drop(&mut self) {
        // SAFETY: This handle uniquely owns the live decoder and destroys it exactly once.
        unsafe { superi_vpx_decoder_destroy(self.native.as_ptr()) };
    }
}

pub(crate) struct EncoderHandle {
    native: NonNull<NativeEncoder>,
    runtime: Arc<Runtime>,
}

// SAFETY: The context has one owner and is never used concurrently.
unsafe impl Send for EncoderHandle {}

impl EncoderHandle {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn encode(
        &mut self,
        data: &[u8],
        format: Format,
        width: u32,
        height: u32,
        pts: i64,
        duration: u64,
        force_keyframe: bool,
        color_space: i32,
        color_range: i32,
    ) -> Result<Vec<RawPacket>, FfiError> {
        // SAFETY: The encoder is live and uniquely borrowed. The slice covers data.len() bytes,
        // and all scalar values use the exact checked shim ABI.
        let status = unsafe {
            superi_vpx_encoder_encode(
                self.native.as_ptr(),
                data.as_ptr(),
                data.len(),
                format as c_int,
                width,
                height,
                pts,
                duration,
                c_int::from(force_keyframe),
                color_space,
                color_range,
            )
        };
        if status != 0 {
            return Err(self.runtime.status_error(status, self.error_detail()));
        }
        self.drain()
    }

    pub(crate) fn flush(&mut self) -> Result<Vec<RawPacket>, FfiError> {
        // SAFETY: The handle uniquely owns a live encoder accepted by the checked shim.
        let status = unsafe { superi_vpx_encoder_flush(self.native.as_ptr()) };
        if status != 0 {
            return Err(self.runtime.status_error(status, self.error_detail()));
        }
        self.drain()
    }

    fn drain(&mut self) -> Result<Vec<RawPacket>, FfiError> {
        let mut packets = Vec::new();
        loop {
            let mut info = NativePacketInfo::default();
            // SAFETY: The encoder is live and info is writable output storage matching the C
            // declaration. Packet bytes remain owned by libvpx until the next shim call.
            let status = unsafe { superi_vpx_encoder_next(self.native.as_ptr(), &mut info) };
            if status == 0 {
                return Ok(packets);
            }
            if status != 1 {
                return Err(self.runtime.status_error(status, self.error_detail()));
            }
            if info.data.is_null() {
                return Err(FfiError::internal("libvpx returned a packet without data"));
            }
            if info.size > isize::MAX as usize {
                return Err(FfiError::internal("libvpx returned an oversized packet"));
            }
            // SAFETY: The shim guarantees a nonnull pointer to info.size readable bytes for the
            // current packet, and the bytes are copied before any subsequent shim call.
            let data = unsafe { std::slice::from_raw_parts(info.data, info.size) }.to_vec();
            packets.push(RawPacket {
                data,
                pts: info.pts,
                duration: info.duration,
                keyframe: info.flags & FRAME_IS_KEY != 0,
                invisible: info.flags & FRAME_IS_INVISIBLE != 0,
            });
        }
    }

    fn error_detail(&self) -> Option<&CStr> {
        // SAFETY: The handle owns a live encoder and the returned message is borrowed only while
        // that context and its retained runtime remain live.
        let value = unsafe { superi_vpx_encoder_error(self.native.as_ptr()) };
        if value.is_null() {
            None
        } else {
            // SAFETY: The checked shim returns a NUL-terminated libvpx or static error string.
            Some(unsafe { CStr::from_ptr(value) })
        }
    }
}

impl Drop for EncoderHandle {
    fn drop(&mut self) {
        // SAFETY: This handle uniquely owns the live encoder and destroys it exactly once.
        unsafe { superi_vpx_encoder_destroy(self.native.as_ptr()) };
    }
}

#[repr(C)]
struct NativeDecoder {
    _private: [u8; 0],
}

#[repr(C)]
struct NativeEncoder {
    _private: [u8; 0],
}

#[derive(Default)]
#[repr(C)]
struct NativeFrameInfo {
    width: u32,
    height: u32,
    format: i32,
    bit_depth: u32,
    color_space: i32,
    color_range: i32,
}

#[repr(C)]
struct NativePacketInfo {
    data: *const u8,
    size: usize,
    pts: i64,
    duration: u64,
    flags: u32,
}

impl Default for NativePacketInfo {
    fn default() -> Self {
        Self {
            data: ptr::null(),
            size: 0,
            pts: 0,
            duration: 0,
            flags: 0,
        }
    }
}

extern "C" {
    fn superi_vpx_status_string(api: *const VpxApi, status: c_int) -> *const c_char;
    fn superi_vpx_decoder_create(
        api: *const VpxApi,
        codec: c_int,
        threads: u32,
        decoder: *mut *mut NativeDecoder,
    ) -> c_int;
    fn superi_vpx_decoder_decode(
        decoder: *mut NativeDecoder,
        data: *const u8,
        size: usize,
    ) -> c_int;
    fn superi_vpx_decoder_next(decoder: *mut NativeDecoder, frame: *mut NativeFrameInfo) -> c_int;
    fn superi_vpx_decoder_copy_plane(
        decoder: *const NativeDecoder,
        plane: u32,
        destination: *mut u8,
        destination_stride: usize,
        destination_rows: u32,
        destination_row_bytes: usize,
    ) -> c_int;
    fn superi_vpx_decoder_error(decoder: *const NativeDecoder) -> *const c_char;
    fn superi_vpx_decoder_destroy(decoder: *mut NativeDecoder);
    fn superi_vpx_encoder_create(
        api: *const VpxApi,
        codec: c_int,
        format: c_int,
        width: u32,
        height: u32,
        timebase_numerator: u32,
        timebase_denominator: u32,
        target_bitrate_kbps: u32,
        threads: u32,
        encoder: *mut *mut NativeEncoder,
    ) -> c_int;
    fn superi_vpx_encoder_encode(
        encoder: *mut NativeEncoder,
        data: *const u8,
        data_size: usize,
        format: c_int,
        width: u32,
        height: u32,
        pts: i64,
        duration: u64,
        force_keyframe: c_int,
        color_space: c_int,
        color_range: c_int,
    ) -> c_int;
    fn superi_vpx_encoder_flush(encoder: *mut NativeEncoder) -> c_int;
    fn superi_vpx_encoder_next(encoder: *mut NativeEncoder, packet: *mut NativePacketInfo)
        -> c_int;
    fn superi_vpx_encoder_error(encoder: *const NativeEncoder) -> *const c_char;
    fn superi_vpx_encoder_destroy(encoder: *mut NativeEncoder);
}
