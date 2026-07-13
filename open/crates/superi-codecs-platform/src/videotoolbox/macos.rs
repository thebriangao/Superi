//! Private native macOS VideoToolbox and CoreMedia FFI boundary.
//!
//! Safe media contracts own all inputs and outputs. This module owns retained native sessions,
//! callback contexts, CoreVideo buffers, and every pointer conversion required by those sessions.
#![allow(unsafe_code)]

use std::collections::VecDeque;
use std::ffi::c_void;
use std::fmt;
use std::ptr::{self, NonNull};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use objc2_core_foundation::{CFData, CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_core_media::{
    kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms, kCMTimeInvalid,
    kCMVideoCodecType_AppleProRes422, kCMVideoCodecType_AppleProRes422HQ,
    kCMVideoCodecType_AppleProRes422LT, kCMVideoCodecType_AppleProRes422Proxy,
    kCMVideoCodecType_AppleProRes4444, kCMVideoCodecType_H264, kCMVideoCodecType_HEVC,
    CMBlockBuffer, CMFormatDescription, CMSampleBuffer, CMSampleTimingInfo, CMTime, CMTimeFlags,
    CMVideoCodecType, CMVideoFormatDescription, CMVideoFormatDescriptionCreate,
    CMVideoFormatDescriptionCreateFromH264ParameterSets,
    CMVideoFormatDescriptionCreateFromHEVCParameterSets,
};
use objc2_core_video::{
    kCVPixelBufferPixelFormatTypeKey, kCVPixelFormatType_32BGRA, kCVPixelFormatType_64RGBAHalf,
    CVImageBuffer, CVPixelBuffer, CVPixelBufferCreate, CVPixelBufferGetBaseAddress,
    CVPixelBufferGetBytesPerRow, CVPixelBufferGetHeight, CVPixelBufferGetPixelFormatType,
    CVPixelBufferGetWidth, CVPixelBufferLockBaseAddress, CVPixelBufferLockFlags,
    CVPixelBufferUnlockBaseAddress,
};
use objc2_video_toolbox::{
    VTCompressionSession, VTDecodeFrameFlags, VTDecodeInfoFlags,
    VTDecompressionOutputCallbackRecord, VTDecompressionSession, VTEncodeInfoFlags,
};
use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::time::{Duration, RationalTime, Timebase};
use superi_media_io::decode::{
    CpuVideoBuffer, DecodeOutput, Decoder, DecoderConfig, FrameStorageKind, VideoFormat,
    VideoFrame, VideoFrameBuffer,
};
use superi_media_io::demux::{CodecId, MetadataValue, Packet, PacketTiming, StreamKind};
use superi_media_io::encode::{
    EncodeInput, EncodeOutput, Encoder, EncoderConfig, EncoderMediaFormat,
};
use superi_media_io::operation::OperationContext;

use super::{ProResProfile, AAC_CODEC_ID, COMPONENT, H264_CODEC_ID, HEVC_CODEC_ID};

/// Checked parameter sets and packet framing parsed from an ISO codec record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoCodecConfiguration {
    nal_length_size: usize,
    parameter_sets: Vec<Vec<u8>>,
}

impl VideoCodecConfiguration {
    /// Returns the big-endian NAL length prefix size used by packets.
    #[must_use]
    pub const fn nal_length_size(&self) -> usize {
        self.nal_length_size
    }

    /// Returns parameter sets in the order stored by the configuration record.
    #[must_use]
    pub fn parameter_sets(&self) -> &[Vec<u8>] {
        &self.parameter_sets
    }
}

/// Parses and bounds-checks an AVC decoder configuration record.
pub fn parse_avcc(bytes: &[u8]) -> Result<VideoCodecConfiguration> {
    if bytes.len() < 7 || bytes[0] != 1 {
        return Err(corrupt(
            "parse_avcc",
            "invalid AVC decoder configuration record",
        ));
    }
    let nal_length_size = usize::from((bytes[4] & 0x03) + 1);
    let mut offset = 6;
    let mut parameter_sets = Vec::new();
    let sps_count = usize::from(bytes[5] & 0x1f);
    for _ in 0..sps_count {
        parameter_sets.push(read_u16_blob(bytes, &mut offset, "parse_avcc")?);
    }
    let pps_count = usize::from(
        *bytes
            .get(offset)
            .ok_or_else(|| corrupt("parse_avcc", "AVC configuration is missing the PPS count"))?,
    );
    offset += 1;
    for _ in 0..pps_count {
        parameter_sets.push(read_u16_blob(bytes, &mut offset, "parse_avcc")?);
    }
    if sps_count == 0 || pps_count == 0 {
        return Err(corrupt(
            "parse_avcc",
            "AVC configuration requires at least one SPS and PPS",
        ));
    }
    Ok(VideoCodecConfiguration {
        nal_length_size,
        parameter_sets,
    })
}

/// Parses and bounds-checks an HEVC decoder configuration record.
pub fn parse_hvcc(bytes: &[u8]) -> Result<VideoCodecConfiguration> {
    if bytes.len() < 23 || bytes[0] != 1 {
        return Err(corrupt(
            "parse_hvcc",
            "invalid HEVC decoder configuration record",
        ));
    }
    let nal_length_size = usize::from((bytes[21] & 0x03) + 1);
    let array_count = usize::from(bytes[22]);
    let mut offset = 23;
    let mut parameter_sets = Vec::new();
    let mut required = [false; 3];
    for _ in 0..array_count {
        let nal_type = *bytes
            .get(offset)
            .ok_or_else(|| corrupt("parse_hvcc", "HEVC configuration ends before a NAL array"))?
            & 0x3f;
        offset += 1;
        let count = read_u16(bytes, &mut offset, "parse_hvcc")?;
        for _ in 0..count {
            let blob = read_u16_blob(bytes, &mut offset, "parse_hvcc")?;
            if let Some(index) = match nal_type {
                32 => Some(0),
                33 => Some(1),
                34 => Some(2),
                _ => None,
            } {
                required[index] = true;
                parameter_sets.push(blob);
            }
        }
    }
    if required.iter().any(|present| !present) {
        return Err(corrupt(
            "parse_hvcc",
            "HEVC configuration requires VPS, SPS, and PPS parameter sets",
        ));
    }
    Ok(VideoCodecConfiguration {
        nal_length_size,
        parameter_sets,
    })
}

/// A retained CoreVideo image that can pass between native decoder and encoder without copying.
pub struct VideoToolboxFrameBuffer {
    image: CFRetained<CVImageBuffer>,
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
}

impl fmt::Debug for VideoToolboxFrameBuffer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VideoToolboxFrameBuffer")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("pixel_format", &self.pixel_format)
            .finish_non_exhaustive()
    }
}

// SAFETY: CoreVideo pixel buffers are retained shareable references. Superi exposes only immutable
// metadata and never exposes mutable base-address access through this wrapper.
unsafe impl Send for VideoToolboxFrameBuffer {}
// SAFETY: Shared access cannot mutate the pixel buffer through the safe wrapper, and CoreVideo owns
// the internal synchronization and retained storage lifetime.
unsafe impl Sync for VideoToolboxFrameBuffer {}

impl VideoFrameBuffer for VideoToolboxFrameBuffer {
    fn storage_kind(&self) -> FrameStorageKind {
        FrameStorageKind::External
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn pixel_format(&self) -> PixelFormat {
        self.pixel_format
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

pub(super) fn create_decoder(
    config: &DecoderConfig,
    operation: &OperationContext,
) -> Result<Box<dyn Decoder>> {
    operation.check("create_videotoolbox_decoder")?;
    let codec = config.stream().codec().as_str();
    if codec == AAC_CODEC_ID {
        return aac::create_decoder(config, operation);
    }
    if config.stream().kind() != StreamKind::Video || !is_video_codec(config.stream().codec()) {
        return Err(unsupported_codec("create_videotoolbox_decoder", codec));
    }
    Ok(Box::new(VideoDecoder::new(config.clone())?))
}

pub(super) fn create_encoder(
    config: &EncoderConfig,
    operation: &OperationContext,
) -> Result<Box<dyn Encoder>> {
    operation.check("create_videotoolbox_encoder")?;
    let codec = config.codec().as_str();
    if codec == AAC_CODEC_ID {
        return aac::create_encoder(config, operation);
    }
    if !is_video_codec(config.codec()) {
        return Err(unsupported_codec("create_videotoolbox_encoder", codec));
    }
    match config.media_format() {
        EncoderMediaFormat::Video(_) => Ok(Box::new(VideoEncoder::new(config.clone())?)),
        _ => Err(invalid(
            "create_videotoolbox_encoder",
            "a video codec requires a video encoder media format",
        )),
    }
}

mod aac;

struct DecodeState {
    codec: CodecId,
    timebase: Timebase,
    output: Mutex<VecDeque<DecodeCallbackOutput>>,
}

enum DecodeCallbackOutput {
    Frame(VideoFrame),
    Status(i32),
}

struct VideoDecoder {
    config: DecoderConfig,
    format_description: CFRetained<CMVideoFormatDescription>,
    destination_attributes: CFRetained<CFDictionary>,
    state: Box<DecodeState>,
    session: CFRetained<VTDecompressionSession>,
    flushed: bool,
}

// SAFETY: Session access is serialized by &mut self. VideoToolbox may invoke the synchronized state
// from internal threads, and drop invalidates the session before releasing that state.
unsafe impl Send for VideoDecoder {}

impl VideoDecoder {
    fn new(config: DecoderConfig) -> Result<Self> {
        let format_description = decoder_format_description(&config)?;
        let pixel_type = CFNumber::new_i32(kCVPixelFormatType_64RGBAHalf as i32);
        let pixel_type_value: &CFType = pixel_type.as_ref();
        // SAFETY: This immutable CoreVideo key is process-lifetime static data.
        let typed_attributes = CFDictionary::<CFString, CFType>::from_slices(
            &[unsafe { kCVPixelBufferPixelFormatTypeKey }],
            &[pixel_type_value],
        );
        // SAFETY: Erasing the typed dictionary preserves the same retained CFDictionary object;
        // VideoToolbox reads its key and CFType value without changing their concrete layout.
        let destination_attributes = unsafe { CFRetained::cast_unchecked(typed_attributes) };
        let state = Box::new(DecodeState {
            codec: config.stream().codec().clone(),
            timebase: config.stream().timebase(),
            output: Mutex::new(VecDeque::new()),
        });
        let session = create_decompression_session(
            &format_description,
            &destination_attributes,
            state.as_ref(),
        )?;
        Ok(Self {
            config,
            format_description,
            destination_attributes,
            state,
            session,
            flushed: false,
        })
    }

    fn recreate_session(&mut self) -> Result<()> {
        // SAFETY: self owns this live retained session and invalidates it before replacement.
        unsafe { self.session.invalidate() };
        self.session = create_decompression_session(
            &self.format_description,
            &self.destination_attributes,
            self.state.as_ref(),
        )?;
        Ok(())
    }
}

impl Drop for VideoDecoder {
    fn drop(&mut self) {
        // SAFETY: self owns the session and invalidation prevents callbacks after state teardown.
        unsafe { self.session.invalidate() };
    }
}

impl Decoder for VideoDecoder {
    fn config(&self) -> &DecoderConfig {
        &self.config
    }

    fn send_packet(&mut self, packet: Packet, operation: &OperationContext) -> Result<()> {
        operation.check("send_videotoolbox_packet")?;
        if self.flushed {
            return Err(conflict(
                "send_videotoolbox_packet",
                "cannot send a packet after decoder flush without reset",
            ));
        }
        if packet.stream_id() != self.config.stream().id() {
            return Err(invalid(
                "send_videotoolbox_packet",
                "packet stream identity does not match the decoder configuration",
            ));
        }
        if packet.data().is_empty() {
            return Err(corrupt(
                "send_videotoolbox_packet",
                "compressed video packet must not be empty",
            ));
        }
        let sample = make_sample_buffer(packet.data(), packet.timing(), &self.format_description)?;
        // SAFETY: The retained session and sample are live for the call. Null frame-refcon and
        // info outputs are explicitly permitted, while callback state is owned by self.
        let status = unsafe {
            self.session.decode_frame(
                &sample,
                VTDecodeFrameFlags(0),
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        check_status(status, "decode_videotoolbox_frame")
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<DecodeOutput> {
        operation.check("receive_videotoolbox_frame")?;
        match self
            .state
            .output
            .lock()
            .expect("decode output mutex poisoned")
            .pop_front()
        {
            Some(DecodeCallbackOutput::Frame(frame)) => Ok(DecodeOutput::Frame(frame)),
            Some(DecodeCallbackOutput::Status(status)) => {
                Err(status_error(status, "decode_videotoolbox_callback"))
            }
            None if self.flushed => Ok(DecodeOutput::EndOfStream),
            None => Ok(DecodeOutput::NeedInput),
        }
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("flush_videotoolbox_decoder")?;
        if !self.flushed {
            // SAFETY: The retained session is live and exclusively accessed through &mut self.
            let status = unsafe { self.session.wait_for_asynchronous_frames() };
            check_status(status, "flush_videotoolbox_decoder")?;
            self.flushed = true;
        }
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_videotoolbox_decoder")?;
        self.state
            .output
            .lock()
            .expect("decode output mutex poisoned")
            .clear();
        self.recreate_session()?;
        self.flushed = false;
        Ok(())
    }
}

// SAFETY: VideoToolbox calls this only with the DecodeState pointer registered during session
// creation. The owning decoder invalidates and drains the session before dropping that state.
unsafe extern "C-unwind" fn decompression_callback(
    refcon: *mut c_void,
    _source_frame_refcon: *mut c_void,
    status: i32,
    _info_flags: VTDecodeInfoFlags,
    image_buffer: *mut CVImageBuffer,
    presentation_time: CMTime,
    presentation_duration: CMTime,
) {
    // SAFETY: refcon is the live DecodeState pointer registered with this callback.
    let state = unsafe { &*(refcon.cast::<DecodeState>()) };
    let mut output = state.output.lock().expect("decode output mutex poisoned");
    if status != 0 {
        output.push_back(DecodeCallbackOutput::Status(status));
        return;
    }
    let Some(image_pointer) = NonNull::new(image_buffer) else {
        output.push_back(DecodeCallbackOutput::Status(-12909));
        return;
    };
    // SAFETY: A successful callback supplies a live CoreVideo object. Retaining it extends the
    // image lifetime beyond this callback until the safe VideoFrame releases it.
    let image = unsafe { CFRetained::retain(image_pointer) };
    match decoded_frame(state, image, presentation_time, presentation_duration) {
        Ok(frame) => output.push_back(DecodeCallbackOutput::Frame(frame)),
        Err(_) => output.push_back(DecodeCallbackOutput::Status(-12909)),
    }
}

fn decoded_frame(
    state: &DecodeState,
    image: CFRetained<CVImageBuffer>,
    presentation_time: CMTime,
    presentation_duration: CMTime,
) -> Result<VideoFrame> {
    let width = u32::try_from(CVPixelBufferGetWidth(&image))
        .map_err(|_| status_error(-12909, "receive_videotoolbox_frame_dimensions"))?;
    let height = u32::try_from(CVPixelBufferGetHeight(&image))
        .map_err(|_| status_error(-12909, "receive_videotoolbox_frame_dimensions"))?;
    let pixel_format = pixel_format(CVPixelBufferGetPixelFormatType(&image))?;
    let alpha_mode = if state.codec.as_str() == ProResProfile::FourFourFourFour.code() {
        AlphaMode::Straight
    } else {
        AlphaMode::Opaque
    };
    let format = VideoFormat::new(
        width,
        height,
        pixel_format,
        ColorSpace::UNSPECIFIED,
        alpha_mode,
    )?;
    let timestamp = from_cm_time(presentation_time, state.timebase, "decode_frame_timestamp")?;
    let duration = from_cm_duration(
        presentation_duration,
        state.timebase,
        "decode_frame_duration",
    )?;
    let buffer = Arc::new(VideoToolboxFrameBuffer {
        image,
        width,
        height,
        pixel_format,
    });
    VideoFrame::new(format, timestamp, duration, buffer)?.with_metadata(
        "platform.backend",
        MetadataValue::Text("apple-videotoolbox".to_owned()),
    )
}

struct EncodeState {
    config: EncoderConfig,
    output: Mutex<VecDeque<EncodeCallbackOutput>>,
    first_packet: AtomicBool,
}

enum EncodeCallbackOutput {
    Packet(Packet),
    Status(i32),
}

struct VideoEncoder {
    config: EncoderConfig,
    state: Box<EncodeState>,
    session: CFRetained<VTCompressionSession>,
    flushed: bool,
}

// SAFETY: Session access is serialized by &mut self, callback output uses a Mutex and AtomicBool,
// and drop invalidates the retained session before releasing its boxed callback state.
unsafe impl Send for VideoEncoder {}

impl VideoEncoder {
    fn new(config: EncoderConfig) -> Result<Self> {
        let state = Box::new(EncodeState {
            config: config.clone(),
            output: Mutex::new(VecDeque::new()),
            first_packet: AtomicBool::new(true),
        });
        let session = create_compression_session(&config, state.as_ref())?;
        Ok(Self {
            config,
            state,
            session,
            flushed: false,
        })
    }

    fn recreate_session(&mut self) -> Result<()> {
        // SAFETY: self owns this live retained session and invalidates it before replacement.
        unsafe { self.session.invalidate() };
        self.session = create_compression_session(&self.config, self.state.as_ref())?;
        Ok(())
    }
}

impl Drop for VideoEncoder {
    fn drop(&mut self) {
        // SAFETY: self owns the session and invalidation prevents callbacks after state teardown.
        unsafe { self.session.invalidate() };
    }
}

impl Encoder for VideoEncoder {
    fn config(&self) -> &EncoderConfig {
        &self.config
    }

    fn send(&mut self, input: EncodeInput, operation: &OperationContext) -> Result<()> {
        operation.check("send_videotoolbox_frame")?;
        if self.flushed {
            return Err(conflict(
                "send_videotoolbox_frame",
                "cannot send a frame after encoder flush without reset",
            ));
        }
        let EncodeInput::Video(frame) = input else {
            return Err(invalid(
                "send_videotoolbox_frame",
                "a VideoToolbox video encoder requires video frames",
            ));
        };
        if self.config.media_format() != &EncoderMediaFormat::Video(frame.format()) {
            return Err(invalid(
                "send_videotoolbox_frame",
                "video frame format does not match the encoder configuration",
            ));
        }
        let image = image_for_frame(&frame)?;
        let presentation = to_cm_time(frame.timestamp(), "encode_frame_timestamp")?;
        let duration = to_cm_duration(frame.duration(), "encode_frame_duration")?;
        // SAFETY: The retained session and image are live for the call. Null frame-refcon and
        // info outputs are permitted, and the registered callback state remains owned by self.
        let status = unsafe {
            self.session.encode_frame(
                &image,
                presentation,
                duration,
                None,
                ptr::null_mut(),
                ptr::null_mut(),
            )
        };
        check_status(status, "encode_videotoolbox_frame")
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<EncodeOutput> {
        operation.check("receive_videotoolbox_packet")?;
        match self
            .state
            .output
            .lock()
            .expect("encode output mutex poisoned")
            .pop_front()
        {
            Some(EncodeCallbackOutput::Packet(packet)) => Ok(EncodeOutput::Packet(packet)),
            Some(EncodeCallbackOutput::Status(status)) => {
                Err(status_error(status, "encode_videotoolbox_callback"))
            }
            None if self.flushed => Ok(EncodeOutput::EndOfStream),
            None => Ok(EncodeOutput::NeedInput),
        }
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("flush_videotoolbox_encoder")?;
        if !self.flushed {
            // SAFETY: The retained session is live and kCMTimeInvalid requests all pending frames.
            let status = unsafe { self.session.complete_frames(kCMTimeInvalid) };
            check_status(status, "flush_videotoolbox_encoder")?;
            self.flushed = true;
        }
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_videotoolbox_encoder")?;
        self.state
            .output
            .lock()
            .expect("encode output mutex poisoned")
            .clear();
        self.state.first_packet.store(true, Ordering::Release);
        self.recreate_session()?;
        self.flushed = false;
        Ok(())
    }
}

// SAFETY: VideoToolbox calls this only with the EncodeState pointer registered during session
// creation. The owning encoder invalidates and drains the session before dropping that state.
unsafe extern "C-unwind" fn compression_callback(
    refcon: *mut c_void,
    _source_frame_refcon: *mut c_void,
    status: i32,
    _info_flags: VTEncodeInfoFlags,
    sample_buffer: *mut CMSampleBuffer,
) {
    // SAFETY: refcon is the live EncodeState pointer registered with this callback.
    let state = unsafe { &*(refcon.cast::<EncodeState>()) };
    let mut output = state.output.lock().expect("encode output mutex poisoned");
    if status != 0 {
        output.push_back(EncodeCallbackOutput::Status(status));
        return;
    }
    let Some(sample_pointer) = NonNull::new(sample_buffer) else {
        output.push_back(EncodeCallbackOutput::Status(-12909));
        return;
    };
    // SAFETY: The successful callback supplies a live sample for the callback duration.
    let sample = unsafe { sample_pointer.as_ref() };
    match packet_from_sample(state, sample) {
        Ok(packet) => output.push_back(EncodeCallbackOutput::Packet(packet)),
        Err(_) => output.push_back(EncodeCallbackOutput::Status(-12909)),
    }
}

fn packet_from_sample(state: &EncodeState, sample: &CMSampleBuffer) -> Result<Packet> {
    // SAFETY: sample is a live callback object and CoreMedia owns its referenced data buffer.
    let data_buffer = unsafe { sample.data_buffer() }
        .ok_or_else(|| status_error(-12909, "read_videotoolbox_encoded_sample"))?;
    // SAFETY: data_buffer is live and immutable while sample remains borrowed.
    let length = unsafe { data_buffer.data_length() };
    let mut bytes = vec![0_u8; length];
    if length != 0 {
        let destination = NonNull::new(bytes.as_mut_ptr().cast::<c_void>())
            .expect("nonempty vector has a nonnull pointer");
        // SAFETY: bytes owns exactly length writable bytes, and the live data buffer reports that
        // same length as its complete readable range.
        let status = unsafe { data_buffer.copy_data_bytes(0, length, destination) };
        check_status(status, "copy_videotoolbox_encoded_sample")?;
    }
    // SAFETY: These accessors read scalar timing fields from the live sample.
    let presentation = unsafe { sample.presentation_time_stamp() };
    // SAFETY: The live sample owns this scalar timing field.
    let decode = unsafe { sample.decode_time_stamp() };
    // SAFETY: The live sample owns this scalar timing field.
    let duration = unsafe { sample.duration() };
    let timebase = state.config.timebase();
    let timing = PacketTiming::new(
        timebase,
        cm_time_value(presentation, timebase),
        cm_time_value(decode, timebase),
        cm_duration_value(duration, timebase),
    )?;
    let keyframe = ProResProfile::from_id(state.config.codec()).is_some()
        || state.first_packet.swap(false, Ordering::AcqRel);
    let mut packet =
        Packet::new(state.config.stream_id(), Arc::from(bytes), timing).with_keyframe(keyframe);
    if let Some(configuration) = encoded_codec_configuration(sample, state.config.codec()) {
        packet = packet.with_metadata(
            "codec.configuration",
            MetadataValue::Bytes(Arc::from(configuration)),
        )?;
    }
    Ok(packet)
}

fn encoded_codec_configuration(sample: &CMSampleBuffer, codec: &CodecId) -> Option<Vec<u8>> {
    let atom_name = match codec.as_str() {
        H264_CODEC_ID => "avcC",
        HEVC_CODEC_ID => "hvcC",
        _ => return None,
    };
    // SAFETY: sample is live, and the returned description is borrowed from that sample.
    let description = unsafe { sample.format_description() }?;
    // SAFETY: The immutable extension key and live format description are valid for this lookup.
    let property_list = unsafe {
        description.extension(kCMFormatDescriptionExtension_SampleDescriptionExtensionAtoms)
    }?;
    let atoms = property_list.downcast::<CFDictionary>().ok()?;
    // SAFETY: The checked downcast established a dictionary object. The extension contract uses
    // CFString keys and CFType values, so restoring those generic types preserves the layout.
    let atoms = unsafe { CFRetained::cast_unchecked::<CFDictionary<CFString, CFType>>(atoms) };
    let key = CFString::from_str(atom_name);
    atoms
        .get(&key)?
        .downcast::<CFData>()
        .ok()
        .map(|data| data.to_vec())
}

fn create_decompression_session(
    description: &CMVideoFormatDescription,
    attributes: &CFDictionary,
    state: &DecodeState,
) -> Result<CFRetained<VTDecompressionSession>> {
    let callback = VTDecompressionOutputCallbackRecord {
        decompressionOutputCallback: Some(decompression_callback),
        decompressionOutputRefCon: ptr::from_ref(state).cast_mut().cast(),
    };
    let mut session = ptr::null_mut();
    // SAFETY: All referenced objects and the boxed state outlive the retained session. session is
    // writable output storage, and the callback record uses the matching function signature.
    let status = unsafe {
        VTDecompressionSession::create(
            None,
            description,
            None,
            Some(attributes),
            &callback,
            NonNull::from(&mut session),
        )
    };
    check_status(status, "create_videotoolbox_decompression_session")?;
    let pointer = NonNull::new(session)
        .ok_or_else(|| status_error(-12909, "create_videotoolbox_decompression_session"))?;
    // SAFETY: A successful create returns one owned retained reference through session.
    Ok(unsafe { CFRetained::from_raw(pointer) })
}

fn create_compression_session(
    config: &EncoderConfig,
    state: &EncodeState,
) -> Result<CFRetained<VTCompressionSession>> {
    let EncoderMediaFormat::Video(format) = config.media_format() else {
        return Err(invalid(
            "create_videotoolbox_compression_session",
            "video encoder configuration is required",
        ));
    };
    let codec_type = codec_type(config.codec()).ok_or_else(|| {
        unsupported_codec(
            "create_videotoolbox_compression_session",
            config.codec().as_str(),
        )
    })?;
    let width = i32::try_from(format.width()).map_err(|_| {
        invalid(
            "create_videotoolbox_compression_session",
            "video width exceeds macOS limits",
        )
    })?;
    let height = i32::try_from(format.height()).map_err(|_| {
        invalid(
            "create_videotoolbox_compression_session",
            "video height exceeds macOS limits",
        )
    })?;
    let mut session = ptr::null_mut();
    // SAFETY: The validated dimensions and codec are passed with a boxed state that outlives the
    // retained session. session is writable output storage for one owned reference.
    let status = unsafe {
        VTCompressionSession::create(
            None,
            width,
            height,
            codec_type,
            None,
            None,
            None,
            Some(compression_callback),
            ptr::from_ref(state).cast_mut().cast(),
            NonNull::from(&mut session),
        )
    };
    check_status(status, "create_videotoolbox_compression_session")?;
    let pointer = NonNull::new(session)
        .ok_or_else(|| status_error(-12909, "create_videotoolbox_compression_session"))?;
    // SAFETY: A successful create returns one owned retained reference through session.
    let retained = unsafe { CFRetained::from_raw(pointer) };
    // SAFETY: retained is a live compression session and is not accessed concurrently here.
    let status = unsafe { retained.prepare_to_encode_frames() };
    check_status(status, "prepare_videotoolbox_compression_session")?;
    Ok(retained)
}

fn decoder_format_description(
    config: &DecoderConfig,
) -> Result<CFRetained<CMVideoFormatDescription>> {
    let codec = config.stream().codec();
    if codec.as_str() == H264_CODEC_ID || codec.as_str() == HEVC_CODEC_ID {
        let bytes = match config.stream().metadata().get("codec.configuration") {
            Some(MetadataValue::Bytes(bytes)) => bytes.as_ref(),
            Some(_) => {
                return Err(corrupt(
                    "create_video_format_description",
                    "codec.configuration metadata must contain bytes",
                ))
            }
            None => {
                return Err(corrupt(
                    "create_video_format_description",
                    "H.264 and HEVC streams require codec.configuration metadata",
                ))
            }
        };
        let configuration = if codec.as_str() == H264_CODEC_ID {
            parse_avcc(bytes)?
        } else {
            parse_hvcc(bytes)?
        };
        return parameter_set_format_description(codec, &configuration);
    }
    let codec_type = codec_type(codec)
        .ok_or_else(|| unsupported_codec("create_video_format_description", codec.as_str()))?;
    let width = metadata_dimension(config, "video.width")?;
    let height = metadata_dimension(config, "video.height")?;
    let mut output: *const CMVideoFormatDescription = ptr::null();
    // SAFETY: The codec and positive dimensions were validated, and output is writable storage
    // for one owned CoreMedia format-description reference.
    let status = unsafe {
        CMVideoFormatDescriptionCreate(
            None,
            codec_type,
            width,
            height,
            None,
            NonNull::from(&mut output),
        )
    };
    check_status(status, "create_video_format_description")?;
    let pointer = NonNull::new(output.cast_mut())
        .ok_or_else(|| status_error(-12909, "create_video_format_description"))?;
    // SAFETY: A successful create returned one owned retained reference through output.
    Ok(unsafe { CFRetained::from_raw(pointer) })
}

fn parameter_set_format_description(
    codec: &CodecId,
    configuration: &VideoCodecConfiguration,
) -> Result<CFRetained<CMVideoFormatDescription>> {
    let mut pointers = configuration
        .parameter_sets
        .iter()
        .map(|set| NonNull::new(set.as_ptr().cast_mut()).expect("nonempty parameter set"))
        .collect::<Vec<_>>();
    let mut sizes = configuration
        .parameter_sets
        .iter()
        .map(Vec::len)
        .collect::<Vec<_>>();
    let mut output: *const CMFormatDescription = ptr::null();
    let pointer_array = NonNull::new(pointers.as_mut_ptr()).expect("parameter sets are nonempty");
    let size_array = NonNull::new(sizes.as_mut_ptr()).expect("parameter sets are nonempty");
    let header_length =
        i32::try_from(configuration.nal_length_size).expect("NAL length is at most 4");
    // SAFETY: Every parameter-set pointer and size names a live validated nonempty vector for the
    // call, both arrays have pointers.len() entries, and output is writable result storage.
    let status = unsafe {
        if codec.as_str() == H264_CODEC_ID {
            CMVideoFormatDescriptionCreateFromH264ParameterSets(
                None,
                pointers.len(),
                pointer_array,
                size_array,
                header_length,
                NonNull::from(&mut output),
            )
        } else {
            CMVideoFormatDescriptionCreateFromHEVCParameterSets(
                None,
                pointers.len(),
                pointer_array,
                size_array,
                header_length,
                None,
                NonNull::from(&mut output),
            )
        }
    };
    check_status(status, "create_parameter_set_format_description")?;
    let pointer = NonNull::new(output.cast_mut())
        .ok_or_else(|| status_error(-12909, "create_parameter_set_format_description"))?;
    // SAFETY: A successful create returned one owned CMFormatDescription reference.
    let generic = unsafe { CFRetained::from_raw(pointer) };
    // SAFETY: The constructor used above creates a video format description, which is the concrete
    // CoreMedia subtype restored by this cast.
    Ok(unsafe { CFRetained::cast_unchecked(generic) })
}

fn make_sample_buffer(
    bytes: &[u8],
    timing: PacketTiming,
    description: &CMVideoFormatDescription,
) -> Result<CFRetained<CMSampleBuffer>> {
    let mut block_pointer = ptr::null_mut();
    // SAFETY: CoreMedia allocates bytes.len() owned bytes and writes one retained reference into
    // block_pointer. The null memory pointer selects CoreMedia-managed storage.
    let status = unsafe {
        CMBlockBuffer::create_with_memory_block(
            None,
            ptr::null_mut(),
            bytes.len(),
            None,
            ptr::null(),
            0,
            bytes.len(),
            0,
            NonNull::from(&mut block_pointer),
        )
    };
    check_status(status, "create_compressed_block_buffer")?;
    let block_pointer = NonNull::new(block_pointer)
        .ok_or_else(|| status_error(-12909, "create_compressed_block_buffer"))?;
    // SAFETY: A successful create returned one owned retained block-buffer reference.
    let block = unsafe { CFRetained::from_raw(block_pointer) };
    let source = NonNull::new(bytes.as_ptr().cast_mut().cast::<c_void>())
        .expect("compressed packet is nonempty");
    // SAFETY: source covers bytes.len() readable bytes, and block owns a writable range of the
    // same length beginning at offset zero.
    let status = unsafe { CMBlockBuffer::replace_data_bytes(source, &block, 0, bytes.len()) };
    check_status(status, "copy_compressed_packet")?;
    // SAFETY: kCMTimeInvalid is immutable process-lifetime CoreMedia constant data.
    let invalid_time = unsafe { kCMTimeInvalid };
    let sample_timing = CMSampleTimingInfo {
        duration: timing.duration().map_or(invalid_time, |value| {
            to_cm_duration(value, "packet_duration").expect("validated packet timebase")
        }),
        presentationTimeStamp: timing.presentation_time().map_or(invalid_time, |value| {
            to_cm_time(value, "packet_presentation_time").expect("validated packet timebase")
        }),
        decodeTimeStamp: timing.decode_time().map_or(invalid_time, |value| {
            to_cm_time(value, "packet_decode_time").expect("validated packet timebase")
        }),
    };
    let sample_size = bytes.len();
    let mut sample_pointer = ptr::null_mut();
    // SAFETY: block, description, timing and sample-size storage remain live for this call, and
    // sample_pointer is writable output storage for one retained sample.
    let status = unsafe {
        CMSampleBuffer::create_ready(
            None,
            Some(&block),
            Some(description),
            1,
            1,
            &sample_timing,
            1,
            &sample_size,
            NonNull::from(&mut sample_pointer),
        )
    };
    check_status(status, "create_compressed_sample_buffer")?;
    let sample_pointer = NonNull::new(sample_pointer)
        .ok_or_else(|| status_error(-12909, "create_compressed_sample_buffer"))?;
    // SAFETY: A successful create returned one owned retained sample reference.
    Ok(unsafe { CFRetained::from_raw(sample_pointer) })
}

fn image_for_frame(frame: &VideoFrame) -> Result<CFRetained<CVImageBuffer>> {
    if let Some(native) = frame
        .buffer()
        .as_any()
        .downcast_ref::<VideoToolboxFrameBuffer>()
    {
        return Ok(native.image.clone());
    }
    let Some(cpu) = frame.buffer().as_any().downcast_ref::<CpuVideoBuffer>() else {
        return Err(unsupported(
            "prepare_videotoolbox_frame",
            "VideoToolbox accepts native CoreVideo or CPU video buffers",
        ));
    };
    let pixel_type = match frame.format().pixel_format() {
        PixelFormat::Bgra8Unorm => kCVPixelFormatType_32BGRA,
        PixelFormat::Rgba16Float => kCVPixelFormatType_64RGBAHalf,
        _ => {
            return Err(unsupported(
                "prepare_videotoolbox_frame",
                "CPU VideoToolbox input supports BGRA8 and RGBA16F",
            ))
        }
    };
    let mut output = ptr::null_mut();
    // SAFETY: Validated dimensions and pixel type are passed with writable output storage. A null
    // attributes dictionary requests CoreVideo-managed allocation.
    let status = unsafe {
        CVPixelBufferCreate(
            None,
            usize::try_from(frame.format().width()).expect("u32 fits usize"),
            usize::try_from(frame.format().height()).expect("u32 fits usize"),
            pixel_type,
            None,
            NonNull::from(&mut output),
        )
    };
    check_status(status, "create_videotoolbox_source_pixel_buffer")?;
    let output = NonNull::new(output)
        .ok_or_else(|| status_error(-12909, "create_videotoolbox_source_pixel_buffer"))?;
    // SAFETY: A successful create returned one owned retained pixel-buffer reference.
    let image = unsafe { CFRetained::from_raw(output) };
    check_status(
        // SAFETY: image is live and exclusively used for this CPU write until the matching unlock.
        unsafe { CVPixelBufferLockBaseAddress(&image, CVPixelBufferLockFlags(0)) },
        "lock_videotoolbox_source_pixel_buffer",
    )?;
    let copy_result = copy_cpu_pixels(cpu, &image);
    // SAFETY: This balances the successful lock above on the same live image.
    let unlock_status =
        unsafe { CVPixelBufferUnlockBaseAddress(&image, CVPixelBufferLockFlags(0)) };
    copy_result?;
    check_status(unlock_status, "unlock_videotoolbox_source_pixel_buffer")?;
    Ok(image)
}

fn copy_cpu_pixels(cpu: &CpuVideoBuffer, image: &CVPixelBuffer) -> Result<()> {
    let source = &cpu.planes()[0];
    let destination = NonNull::new(CVPixelBufferGetBaseAddress(image).cast::<u8>())
        .ok_or_else(|| status_error(-12909, "access_videotoolbox_source_pixel_buffer"))?;
    let destination_stride = CVPixelBufferGetBytesPerRow(image);
    let copy_length = source.stride().min(destination_stride);
    for row in 0..usize::try_from(source.row_count()).expect("u32 fits usize") {
        // SAFETY: The source plane contract covers source.stride() bytes for every row. The locked
        // CoreVideo base address covers destination_stride bytes for every image row, and the
        // nonoverlapping buffers are copied for only their checked minimum row length.
        unsafe {
            ptr::copy_nonoverlapping(
                source.bytes().as_ptr().add(row * source.stride()),
                destination.as_ptr().add(row * destination_stride),
                copy_length,
            );
        }
    }
    Ok(())
}

fn codec_type(codec: &CodecId) -> Option<CMVideoCodecType> {
    match codec.as_str() {
        H264_CODEC_ID => Some(kCMVideoCodecType_H264),
        HEVC_CODEC_ID => Some(kCMVideoCodecType_HEVC),
        value if value == ProResProfile::Proxy.code() => {
            Some(kCMVideoCodecType_AppleProRes422Proxy)
        }
        value if value == ProResProfile::Lt.code() => Some(kCMVideoCodecType_AppleProRes422LT),
        value if value == ProResProfile::Standard.code() => Some(kCMVideoCodecType_AppleProRes422),
        value if value == ProResProfile::Hq.code() => Some(kCMVideoCodecType_AppleProRes422HQ),
        value if value == ProResProfile::FourFourFourFour.code() => {
            Some(kCMVideoCodecType_AppleProRes4444)
        }
        _ => None,
    }
}

fn is_video_codec(codec: &CodecId) -> bool {
    codec_type(codec).is_some()
}

fn pixel_format(pixel_type: u32) -> Result<PixelFormat> {
    if pixel_type == kCVPixelFormatType_32BGRA {
        Ok(PixelFormat::Bgra8Unorm)
    } else if pixel_type == kCVPixelFormatType_64RGBAHalf {
        Ok(PixelFormat::Rgba16Float)
    } else {
        Err(unsupported(
            "map_videotoolbox_pixel_format",
            "VideoToolbox returned an unsupported CoreVideo pixel format",
        ))
    }
}

fn metadata_dimension(config: &DecoderConfig, key: &'static str) -> Result<i32> {
    match config.stream().metadata().get(key) {
        Some(MetadataValue::Unsigned(value)) => i32::try_from(*value)
            .ok()
            .filter(|value| *value > 0)
            .ok_or_else(|| {
                corrupt(
                    "create_video_format_description",
                    "video dimension is invalid",
                )
            }),
        _ => Err(corrupt(
            "create_video_format_description",
            "ProRes streams require unsigned video.width and video.height metadata",
        )),
    }
}

fn to_cm_time(value: RationalTime, operation: &'static str) -> Result<CMTime> {
    let timebase = value.timebase();
    let timescale = i32::try_from(timebase.numerator())
        .map_err(|_| invalid(operation, "timebase numerator exceeds CoreMedia limits"))?;
    let coordinate = value
        .value()
        .checked_mul(i64::from(timebase.denominator()))
        .ok_or_else(|| invalid(operation, "time coordinate exceeds CoreMedia limits"))?;
    // SAFETY: timescale is the validated positive timebase numerator and coordinate fits i64.
    Ok(unsafe { CMTime::new(coordinate, timescale) })
}

fn to_cm_duration(value: Duration, operation: &'static str) -> Result<CMTime> {
    let timebase = value.timebase();
    let coordinate = i64::try_from(value.value())
        .ok()
        .and_then(|coordinate| coordinate.checked_mul(i64::from(timebase.denominator())))
        .ok_or_else(|| invalid(operation, "duration exceeds CoreMedia limits"))?;
    let timescale = i32::try_from(timebase.numerator())
        .map_err(|_| invalid(operation, "timebase numerator exceeds CoreMedia limits"))?;
    // SAFETY: timescale is the validated positive timebase numerator and coordinate fits i64.
    Ok(unsafe { CMTime::new(coordinate, timescale) })
}

fn from_cm_time(
    value: CMTime,
    timebase: Timebase,
    operation: &'static str,
) -> Result<RationalTime> {
    let coordinate = cm_time_value(value, timebase).ok_or_else(|| {
        corrupt(
            operation,
            "CoreMedia returned an invalid or inexact timestamp",
        )
    })?;
    Ok(RationalTime::new(coordinate, timebase))
}

fn from_cm_duration(
    value: CMTime,
    timebase: Timebase,
    operation: &'static str,
) -> Result<Duration> {
    let coordinate = cm_duration_value(value, timebase).unwrap_or(0);
    Duration::new(coordinate, timebase)
        .map_err(|_| corrupt(operation, "CoreMedia returned an invalid frame duration"))
}

fn cm_time_value(value: CMTime, timebase: Timebase) -> Option<i64> {
    if !value.flags.contains(CMTimeFlags::Valid) || value.timescale <= 0 {
        return None;
    }
    let numerator = i128::from(value.value) * i128::from(timebase.numerator());
    let denominator = i128::from(value.timescale) * i128::from(timebase.denominator());
    if numerator % denominator != 0 {
        return None;
    }
    i64::try_from(numerator / denominator).ok()
}

fn cm_duration_value(value: CMTime, timebase: Timebase) -> Option<u64> {
    cm_time_value(value, timebase).and_then(|coordinate| u64::try_from(coordinate).ok())
}

fn read_u16(bytes: &[u8], offset: &mut usize, operation: &'static str) -> Result<usize> {
    let end = offset
        .checked_add(2)
        .ok_or_else(|| corrupt(operation, "configuration overflow"))?;
    let value = bytes
        .get(*offset..end)
        .ok_or_else(|| corrupt(operation, "configuration ends inside a length field"))?;
    *offset = end;
    Ok(usize::from(u16::from_be_bytes([value[0], value[1]])))
}

fn read_u16_blob(bytes: &[u8], offset: &mut usize, operation: &'static str) -> Result<Vec<u8>> {
    let length = read_u16(bytes, offset, operation)?;
    if length == 0 {
        return Err(corrupt(
            operation,
            "configuration contains an empty parameter set",
        ));
    }
    let end = offset
        .checked_add(length)
        .ok_or_else(|| corrupt(operation, "configuration parameter set overflow"))?;
    let value = bytes
        .get(*offset..end)
        .ok_or_else(|| corrupt(operation, "configuration ends inside a parameter set"))?;
    *offset = end;
    Ok(value.to_vec())
}

fn check_status(status: i32, operation: &'static str) -> Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(status_error(status, operation))
    }
}

fn status_error(status: i32, operation: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Degraded,
        "the macOS media framework rejected the native codec operation",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation).with_field("os_status", status.to_string()),
    )
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn corrupt(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unsupported_codec(operation: &'static str, codec: &str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        "the Apple backend does not support the requested codec operation",
    )
    .with_context(ErrorContext::new(COMPONENT, operation).with_field("codec_id", codec))
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
