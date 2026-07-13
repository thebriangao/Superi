//! VP8 and VP9 decode and encode through the official permissive libvpx runtime.
//!
//! Superi compiles a narrow C shim against the pinned libvpx 1.16 public headers. The runtime is
//! loaded from `SUPERI_LIBVPX_PATH`, beside the executable, or from a platform library location.
//! This keeps the open Rust dependency graph permissive without adopting an MPL binding crate.

use std::collections::VecDeque;
use std::sync::Arc;

use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::time::{Duration, RationalTime, Timebase};
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendDescriptor, BackendRegistration, BackendTier,
    ChromaSampling, CodecCapability, CodecOperation, HardwareAcceleration, MediaBackend,
};
use superi_media_io::decode::{
    CpuVideoBuffer, DecodeOutput, Decoder, DecoderConfig, VideoFormat, VideoFrame, VideoPlane,
};
use superi_media_io::demux::{
    BackendId, CodecId, MediaMetadata, MediaSource, MetadataValue, Packet, PacketTiming,
    SourceProbe, SourceProbeResult, SourceRequest, StreamKind,
};
use superi_media_io::encode::{
    EncodeInput, EncodeOutput, Encoder, EncoderConfig, EncoderMediaFormat,
};
use superi_media_io::operation::OperationContext;

use crate::vpx_ffi::{
    Codec as NativeCodec, DecoderHandle, EncoderHandle, FfiError, Format as NativeFormat, RawFrame,
    RawPacket, Runtime,
};

const COMPONENT: &str = "superi-codecs-rs.vpx";

/// One royalty-free WebM video codec implemented by libvpx.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum VpxCodec {
    /// The VP8 video codec.
    Vp8,
    /// The VP9 video codec.
    Vp9,
}

impl VpxCodec {
    /// Returns the stable codec identifier used by containers and backend selection.
    #[must_use]
    pub fn codec_id(self) -> CodecId {
        CodecId::new(self.code()).expect("static VPx codec identifier is valid")
    }

    /// Returns the permanent lowercase codec code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Vp8 => "vp8",
            Self::Vp9 => "vp9",
        }
    }

    fn from_id(codec: &CodecId) -> Option<Self> {
        match codec.as_str() {
            "vp8" => Some(Self::Vp8),
            "vp9" => Some(Self::Vp9),
            _ => None,
        }
    }

    const fn native(self) -> NativeCodec {
        match self {
            Self::Vp8 => NativeCodec::Vp8,
            Self::Vp9 => NativeCodec::Vp9,
        }
    }
}

/// Primary VP8 and VP9 backend over the pinned official libvpx ABI.
pub struct VpxBackend {
    descriptor: BackendDescriptor,
    runtime: Arc<Runtime>,
}

impl VpxBackend {
    /// Loads the approved libvpx runtime and creates the backend.
    pub fn new() -> Result<Self> {
        let runtime = Runtime::load().map_err(|error| {
            unavailable(
                "load_libvpx_runtime",
                format!("the approved libvpx 1.16 runtime is unavailable: {error}"),
            )
        })?;
        Ok(Self {
            descriptor: BackendDescriptor::new(BackendId::new("libvpx")?, "libvpx VP8 and VP9")?,
            runtime,
        })
    }

    /// Builds the deterministic primary registration for VP8 and VP9 decode and encode.
    pub fn registration() -> Result<BackendRegistration> {
        let codec_capabilities = vpx_capabilities()?;
        BackendRegistration::new(
            Arc::new(Self::new()?),
            BackendCapabilities::new([
                BackendCapability::Decode(VpxCodec::Vp8.codec_id()),
                BackendCapability::Encode(VpxCodec::Vp8.codec_id()),
                BackendCapability::Decode(VpxCodec::Vp9.codec_id()),
                BackendCapability::Encode(VpxCodec::Vp9.codec_id()),
            ])
            .with_hardware_acceleration(HardwareAcceleration::Software)
            .with_codec_capabilities(codec_capabilities)?,
            100,
            BackendTier::Primary,
        )
    }

    /// Returns the loaded libvpx runtime version.
    #[must_use]
    pub fn runtime_version(&self) -> &str {
        self.runtime.version()
    }
}

fn vpx_capabilities() -> Result<Vec<CodecCapability>> {
    let mut values = Vec::new();
    for operation in [CodecOperation::Decode, CodecOperation::Encode] {
        values.push(
            CodecCapability::new(operation, VpxCodec::Vp8.codec_id())
                .with_profiles_not_applicable()
                .with_levels_not_applicable()
                .with_bit_depths([8])?
                .with_chroma_sampling([ChromaSampling::Cs420])?,
        );
        for (profile, bit_depths, chroma_sampling) in [
            ("profile_0", &[8][..], &[ChromaSampling::Cs420][..]),
            (
                "profile_1",
                &[8][..],
                &[ChromaSampling::Cs422, ChromaSampling::Cs444][..],
            ),
            ("profile_2", &[10][..], &[ChromaSampling::Cs420][..]),
            (
                "profile_3",
                &[10][..],
                &[ChromaSampling::Cs422, ChromaSampling::Cs444][..],
            ),
        ] {
            values.push(
                CodecCapability::new(operation, VpxCodec::Vp9.codec_id())
                    .with_profiles([profile])?
                    .with_levels_runtime()
                    .with_bit_depths(bit_depths.iter().copied())?
                    .with_chroma_sampling(chroma_sampling.iter().copied())?,
            );
        }
    }
    Ok(values)
}

impl MediaBackend for VpxBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        _probe: &SourceProbe<'_>,
        operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        operation.check("probe_vpx_source")?;
        Ok(SourceProbeResult::NoMatch)
    }

    fn open_source(
        &self,
        _request: &SourceRequest,
        operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        operation.check("open_vpx_source")?;
        Err(unsupported(
            "open_vpx_source",
            "the VP8 and VP9 codec backend does not open media containers",
        ))
    }

    fn create_decoder(
        &self,
        config: &DecoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>> {
        operation.check("create_vpx_decoder")?;
        Ok(Box::new(VpxDecoder::new(
            config.clone(),
            Arc::clone(&self.runtime),
        )?))
    }

    fn create_encoder(
        &self,
        config: &EncoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        operation.check("create_vpx_encoder")?;
        Ok(Box::new(VpxEncoder::new(
            config.clone(),
            Arc::clone(&self.runtime),
        )?))
    }
}

struct VpxDecoder {
    config: DecoderConfig,
    codec: VpxCodec,
    stream_color: Option<ColorSpace>,
    runtime: Arc<Runtime>,
    native: DecoderHandle,
    output: VecDeque<VideoFrame>,
    flushed: bool,
}

impl VpxDecoder {
    fn new(config: DecoderConfig, runtime: Arc<Runtime>) -> Result<Self> {
        if config.stream().kind() != StreamKind::Video {
            return Err(invalid(
                "create_vpx_decoder",
                "VP8 and VP9 decoding requires a video stream",
            ));
        }
        let codec = VpxCodec::from_id(config.stream().codec()).ok_or_else(|| {
            unsupported(
                "create_vpx_decoder",
                "the requested codec is neither VP8 nor VP9",
            )
        })?;
        validate_alpha_metadata(config.stream().metadata())?;
        let stream_color = declared_color(config.stream().metadata())?;
        let native = runtime.decoder(codec.native()).map_err(|error| {
            ffi_failure("create_vpx_decoder", ErrorCategory::Unavailable, error)
        })?;
        Ok(Self {
            config,
            codec,
            stream_color,
            runtime,
            native,
            output: VecDeque::new(),
            flushed: false,
        })
    }

    fn decode_packet(
        &mut self,
        packet: &Packet,
        operation: &OperationContext,
    ) -> Result<Vec<VideoFrame>> {
        if packet.stream_id() != self.config.stream().id() {
            return Err(conflict(
                "decode_vpx_packet",
                "VPx packet belongs to a different stream",
            ));
        }
        if packet.timing().timebase() != self.config.stream().timebase() {
            return Err(corrupt(
                "decode_vpx_packet",
                "VPx packet timebase does not match its stream",
            ));
        }
        if packet.data().is_empty() {
            return Err(corrupt(
                "decode_vpx_packet",
                "VPx compressed packet must not be empty",
            ));
        }
        let timestamp = packet
            .timing()
            .presentation_time()
            .or_else(|| packet.timing().decode_time())
            .ok_or_else(|| {
                corrupt(
                    "decode_vpx_packet",
                    "VPx packet requires a presentation or decode timestamp",
                )
            })?;
        let duration = packet.timing().duration().ok_or_else(|| {
            corrupt(
                "decode_vpx_packet",
                "VPx packet requires an exact frame duration",
            )
        })?;
        let declared_color = declared_color(packet.metadata())?.or(self.stream_color);
        operation.check("decode_vpx_packet")?;
        let decoded = self
            .native
            .decode(packet.data())
            .map_err(|error| ffi_failure("decode_vpx_packet", ErrorCategory::CorruptData, error))?;
        operation.check("decode_vpx_packet")?;
        timed_frames(
            decoded,
            FrameContext {
                timestamp,
                duration,
                metadata: packet.metadata(),
                declared_color,
                keyframe: packet.is_keyframe(),
                codec: self.codec,
                runtime_version: self.runtime.version(),
            },
        )
    }
}

impl Decoder for VpxDecoder {
    fn config(&self) -> &DecoderConfig {
        &self.config
    }

    fn send_packet(&mut self, packet: Packet, operation: &OperationContext) -> Result<()> {
        operation.check("send_vpx_packet")?;
        if self.flushed {
            return Err(conflict(
                "send_vpx_packet",
                "VPx decoder requires reset before accepting packets after flush",
            ));
        }
        let frames = self.decode_packet(&packet, operation)?;
        self.output.extend(frames);
        Ok(())
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<DecodeOutput> {
        operation.check("receive_vpx_frame")?;
        if let Some(frame) = self.output.pop_front() {
            return Ok(DecodeOutput::Frame(frame));
        }
        if self.flushed {
            Ok(DecodeOutput::EndOfStream)
        } else {
            Ok(DecodeOutput::NeedInput)
        }
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("flush_vpx_decoder")?;
        if self.flushed {
            return Ok(());
        }
        let delayed = self
            .native
            .decode(&[])
            .map_err(|error| ffi_failure("flush_vpx_decoder", ErrorCategory::CorruptData, error))?;
        operation.check("flush_vpx_decoder")?;
        if !delayed.is_empty() {
            return Err(corrupt(
                "flush_vpx_decoder",
                "libvpx returned delayed frames without packet timing",
            ));
        }
        self.flushed = true;
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_vpx_decoder")?;
        self.native = self
            .runtime
            .decoder(self.codec.native())
            .map_err(|error| ffi_failure("reset_vpx_decoder", ErrorCategory::Unavailable, error))?;
        self.output.clear();
        self.flushed = false;
        Ok(())
    }
}

struct VpxEncoder {
    config: EncoderConfig,
    codec: VpxCodec,
    format: VideoFormat,
    native_format: NativeFormat,
    runtime: Arc<Runtime>,
    native: EncoderHandle,
    output: VecDeque<Packet>,
    flushed: bool,
    frame_index: u64,
}

impl VpxEncoder {
    fn new(config: EncoderConfig, runtime: Arc<Runtime>) -> Result<Self> {
        let codec = VpxCodec::from_id(config.codec()).ok_or_else(|| {
            unsupported(
                "create_vpx_encoder",
                "the requested codec is neither VP8 nor VP9",
            )
        })?;
        let format = match config.media_format() {
            EncoderMediaFormat::Video(format) => *format,
            _ => {
                return Err(invalid(
                    "create_vpx_encoder",
                    "VP8 and VP9 encoding requires a video format",
                ));
            }
        };
        if format.alpha_mode() != AlphaMode::Opaque {
            return Err(unsupported(
                "create_vpx_encoder",
                "VP8 and VP9 primary video encoding does not accept alpha",
            ));
        }
        let native_format = native_format(codec, format.pixel_format())?;
        if config.timebase().numerator() > i32::MAX as u32
            || config.timebase().denominator() > i32::MAX as u32
        {
            return Err(invalid(
                "create_vpx_encoder",
                "VPx encoder timebase exceeds the libvpx integer domain",
            ));
        }
        let bitrate = target_bitrate(format)?;
        let native = runtime
            .encoder(
                codec.native(),
                native_format,
                format.width(),
                format.height(),
                config.timebase().denominator(),
                config.timebase().numerator(),
                bitrate,
            )
            .map_err(|error| {
                ffi_failure("create_vpx_encoder", ErrorCategory::Unsupported, error)
            })?;
        Ok(Self {
            config,
            codec,
            format,
            native_format,
            runtime,
            native,
            output: VecDeque::new(),
            flushed: false,
            frame_index: 0,
        })
    }

    fn encode_frame(
        &mut self,
        frame: VideoFrame,
        operation: &OperationContext,
    ) -> Result<Vec<Packet>> {
        if frame.format() != self.format {
            return Err(invalid(
                "encode_vpx_frame",
                "VPx input frame format does not match the encoder configuration",
            ));
        }
        if frame.timestamp().timebase() != self.config.timebase()
            || frame.duration().timebase() != self.config.timebase()
        {
            return Err(invalid(
                "encode_vpx_frame",
                "VPx input timing does not match the encoder timebase",
            ));
        }
        if frame.duration().value() == 0 {
            return Err(invalid(
                "encode_vpx_frame",
                "VPx input frame duration must be greater than zero",
            ));
        }
        let pixels = contiguous_pixels(&frame, self.native_format)?;
        let (color_space, color_range) = native_color(frame.format().color_space());
        operation.check("encode_vpx_frame")?;
        let encoded = self
            .native
            .encode(
                &pixels,
                self.native_format,
                self.format.width(),
                self.format.height(),
                frame.timestamp().value(),
                frame.duration().value(),
                self.frame_index == 0,
                color_space,
                color_range,
            )
            .map_err(|error| ffi_failure("encode_vpx_frame", ErrorCategory::InvalidInput, error))?;
        operation.check("encode_vpx_frame")?;
        self.frame_index = self
            .frame_index
            .checked_add(1)
            .ok_or_else(|| internal("encode_vpx_frame", "VPx encoder frame counter overflowed"))?;
        encoded
            .into_iter()
            .map(|packet| {
                packet_from_raw(
                    packet,
                    self.config.stream_id(),
                    self.config.timebase(),
                    frame.metadata(),
                    frame.format().color_space(),
                    self.codec,
                    self.runtime.version(),
                )
            })
            .collect()
    }

    fn rebuild(&self) -> Result<EncoderHandle> {
        self.runtime
            .encoder(
                self.codec.native(),
                self.native_format,
                self.format.width(),
                self.format.height(),
                self.config.timebase().denominator(),
                self.config.timebase().numerator(),
                target_bitrate(self.format)?,
            )
            .map_err(|error| ffi_failure("reset_vpx_encoder", ErrorCategory::Unavailable, error))
    }
}

impl Encoder for VpxEncoder {
    fn config(&self) -> &EncoderConfig {
        &self.config
    }

    fn send(&mut self, input: EncodeInput, operation: &OperationContext) -> Result<()> {
        operation.check("send_vpx_frame")?;
        if self.flushed {
            return Err(conflict(
                "send_vpx_frame",
                "VPx encoder requires reset before accepting frames after flush",
            ));
        }
        let EncodeInput::Video(frame) = input else {
            return Err(invalid(
                "send_vpx_frame",
                "VP8 and VP9 encoding accepts video frames only",
            ));
        };
        let packets = self.encode_frame(frame, operation)?;
        self.output.extend(packets);
        Ok(())
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<EncodeOutput> {
        operation.check("receive_vpx_packet")?;
        if let Some(packet) = self.output.pop_front() {
            return Ok(EncodeOutput::Packet(packet));
        }
        if self.flushed {
            Ok(EncodeOutput::EndOfStream)
        } else {
            Ok(EncodeOutput::NeedInput)
        }
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("flush_vpx_encoder")?;
        if self.flushed {
            return Ok(());
        }
        let packets = self
            .native
            .flush()
            .map_err(|error| ffi_failure("flush_vpx_encoder", ErrorCategory::Internal, error))?;
        operation.check("flush_vpx_encoder")?;
        for packet in packets {
            self.output.push_back(packet_from_raw(
                packet,
                self.config.stream_id(),
                self.config.timebase(),
                &MediaMetadata::new(),
                self.format.color_space(),
                self.codec,
                self.runtime.version(),
            )?);
        }
        self.flushed = true;
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_vpx_encoder")?;
        self.native = self.rebuild()?;
        self.output.clear();
        self.flushed = false;
        self.frame_index = 0;
        Ok(())
    }
}

#[derive(Clone, Copy)]
struct FrameContext<'a> {
    timestamp: RationalTime,
    duration: Duration,
    metadata: &'a MediaMetadata,
    declared_color: Option<ColorSpace>,
    keyframe: bool,
    codec: VpxCodec,
    runtime_version: &'a str,
}

fn timed_frames(raw_frames: Vec<RawFrame>, context: FrameContext<'_>) -> Result<Vec<VideoFrame>> {
    if raw_frames.is_empty() {
        return Ok(Vec::new());
    }
    let frame_count = u64::try_from(raw_frames.len())
        .map_err(|_| corrupt("decode_vpx_packet", "decoded frame count overflowed"))?;
    if context.duration.value() % frame_count != 0 {
        return Err(corrupt(
            "decode_vpx_packet",
            "VPx packet duration cannot be divided across decoded frames",
        ));
    }
    let per_frame = context.duration.value() / frame_count;
    if per_frame == 0 {
        return Err(corrupt(
            "decode_vpx_packet",
            "VPx decoded frame duration must be greater than zero",
        ));
    }
    let frame_duration = Duration::new(per_frame, context.duration.timebase())?;
    raw_frames
        .into_iter()
        .enumerate()
        .map(|(index, raw)| {
            let offset = i64::try_from(index)
                .ok()
                .and_then(|index| index.checked_mul(i64::try_from(per_frame).ok()?))
                .ok_or_else(|| corrupt("decode_vpx_packet", "decoded frame time overflowed"))?;
            let frame_time = RationalTime::new(
                context
                    .timestamp
                    .value()
                    .checked_add(offset)
                    .ok_or_else(|| corrupt("decode_vpx_packet", "decoded frame time overflowed"))?,
                context.timestamp.timebase(),
            );
            video_frame_from_raw(
                raw,
                FrameContext {
                    timestamp: frame_time,
                    duration: frame_duration,
                    ..context
                },
            )
        })
        .collect()
}

fn video_frame_from_raw(raw: RawFrame, context: FrameContext<'_>) -> Result<VideoFrame> {
    let pixel_format = pixel_format(raw.format);
    if raw.bit_depth != u32::from(pixel_format.bits_per_component()) {
        return Err(corrupt(
            "decode_vpx_packet",
            "libvpx frame bit depth does not match its pixel format",
        ));
    }
    let layouts = plane_layout(raw.width, raw.height, raw.format)?;
    if raw.planes.len() != layouts.len() {
        return Err(internal(
            "decode_vpx_packet",
            "libvpx frame plane count does not match its pixel format",
        ));
    }
    let planes = raw
        .planes
        .into_iter()
        .zip(layouts)
        .map(|(bytes, (stride, rows))| VideoPlane::new(Arc::from(bytes), stride, rows))
        .collect::<Result<Vec<_>>>()?;
    let color_space = context
        .declared_color
        .unwrap_or_else(|| decoded_color(raw.color_space, raw.color_range, raw.bit_depth));
    let format = VideoFormat::new(
        raw.width,
        raw.height,
        pixel_format,
        color_space,
        AlphaMode::Opaque,
    )?;
    let buffer = CpuVideoBuffer::new(raw.width, raw.height, pixel_format, planes)?;
    let mut frame = VideoFrame::new(
        format,
        context.timestamp,
        context.duration,
        Arc::new(buffer),
    )?;
    frame = copy_metadata_to_frame(frame, context.metadata)?;
    frame = frame
        .with_metadata("video.keyframe", MetadataValue::Boolean(context.keyframe))?
        .with_metadata(
            "vpx.codec",
            MetadataValue::Text(context.codec.code().to_owned()),
        )?
        .with_metadata(
            "vpx.runtime-version",
            MetadataValue::Text(context.runtime_version.to_owned()),
        )?
        .with_metadata(
            "vpx.bit-depth",
            MetadataValue::Unsigned(u64::from(raw.bit_depth)),
        )?
        .with_metadata(
            "vpx.color-space",
            MetadataValue::Signed(i64::from(raw.color_space)),
        )?
        .with_metadata(
            "vpx.color-range",
            MetadataValue::Signed(i64::from(raw.color_range)),
        )?;
    Ok(frame)
}

fn copy_metadata_to_frame(mut frame: VideoFrame, metadata: &MediaMetadata) -> Result<VideoFrame> {
    for (key, value) in metadata.iter() {
        frame = frame.with_metadata(key, value.clone())?;
    }
    Ok(frame)
}

fn packet_from_raw(
    raw: RawPacket,
    stream_id: superi_media_io::demux::StreamId,
    timebase: Timebase,
    metadata: &MediaMetadata,
    color_space: ColorSpace,
    codec: VpxCodec,
    runtime_version: &str,
) -> Result<Packet> {
    if raw.data.is_empty() {
        return Err(internal(
            "encode_vpx_frame",
            "libvpx returned an empty compressed packet",
        ));
    }
    let timing = PacketTiming::new(timebase, Some(raw.pts), Some(raw.pts), Some(raw.duration))?;
    let mut packet =
        Packet::new(stream_id, Arc::from(raw.data), timing).with_keyframe(raw.keyframe);
    for (key, value) in metadata.iter() {
        packet = packet.with_metadata(key, value.clone())?;
    }
    packet = packet
        .with_metadata(
            "video.color-primaries",
            MetadataValue::Text(color_space.primaries().code().to_owned()),
        )?
        .with_metadata(
            "video.transfer-function",
            MetadataValue::Text(color_space.transfer().code().to_owned()),
        )?
        .with_metadata(
            "video.matrix-coefficients",
            MetadataValue::Text(color_space.matrix().code().to_owned()),
        )?
        .with_metadata(
            "video.color-range",
            MetadataValue::Text(color_space.range().code().to_owned()),
        )?
        .with_metadata("vpx.codec", MetadataValue::Text(codec.code().to_owned()))?
        .with_metadata(
            "vpx.runtime-version",
            MetadataValue::Text(runtime_version.to_owned()),
        )?
        .with_metadata("vpx.invisible", MetadataValue::Boolean(raw.invisible))?;
    Ok(packet)
}

fn declared_color(metadata: &MediaMetadata) -> Result<Option<ColorSpace>> {
    let primaries = text_metadata(metadata, "video.color-primaries")?;
    let transfer = text_metadata(metadata, "video.transfer-function")?;
    let matrix = text_metadata(metadata, "video.matrix-coefficients")?;
    let range = text_metadata(metadata, "video.color-range")?;
    match (primaries, transfer, matrix, range) {
        (None, None, None, None) => Ok(None),
        (Some(primaries), Some(transfer), Some(matrix), Some(range)) => {
            let primaries = ColorPrimaries::from_code(primaries).ok_or_else(|| {
                invalid("decode_vpx_packet", "unknown VPx color primaries metadata")
            })?;
            let transfer = TransferFunction::from_code(transfer).ok_or_else(|| {
                invalid(
                    "decode_vpx_packet",
                    "unknown VPx transfer function metadata",
                )
            })?;
            let matrix = MatrixCoefficients::from_code(matrix).ok_or_else(|| {
                invalid(
                    "decode_vpx_packet",
                    "unknown VPx matrix coefficients metadata",
                )
            })?;
            let range = ColorRange::from_code(range)
                .ok_or_else(|| invalid("decode_vpx_packet", "unknown VPx color range metadata"))?;
            Ok(Some(ColorSpace::new(primaries, transfer, matrix, range)))
        }
        _ => Err(invalid(
            "decode_vpx_packet",
            "VPx color metadata must declare every interpretation axis",
        )),
    }
}

fn text_metadata<'a>(metadata: &'a MediaMetadata, key: &str) -> Result<Option<&'a str>> {
    match metadata.get(key) {
        None => Ok(None),
        Some(MetadataValue::Text(value)) => Ok(Some(value)),
        Some(_) => Err(invalid(
            "decode_vpx_packet",
            format!("{key} metadata must contain text"),
        )),
    }
}

fn contiguous_pixels(frame: &VideoFrame, format: NativeFormat) -> Result<Vec<u8>> {
    let cpu = frame
        .buffer()
        .as_any()
        .downcast_ref::<CpuVideoBuffer>()
        .ok_or_else(|| {
            unsupported(
                "encode_vpx_frame",
                "VPx encoding currently requires CPU-addressable video planes",
            )
        })?;
    let layouts = plane_layout(frame.format().width(), frame.format().height(), format)?;
    if cpu.planes().len() != layouts.len() {
        return Err(invalid(
            "encode_vpx_frame",
            "VPx input plane count does not match the configured pixel format",
        ));
    }
    let capacity = layouts
        .iter()
        .try_fold(0_usize, |total, (row_bytes, rows)| {
            let size = row_bytes
                .checked_mul(usize::try_from(*rows).map_err(|_| {
                    invalid("encode_vpx_frame", "VPx input plane height overflowed")
                })?)
                .ok_or_else(|| invalid("encode_vpx_frame", "VPx input plane size overflowed"))?;
            total
                .checked_add(size)
                .ok_or_else(|| invalid("encode_vpx_frame", "VPx input frame size overflowed"))
        })?;
    let mut pixels = Vec::with_capacity(capacity);
    for (plane, (row_bytes, rows)) in cpu.planes().iter().zip(layouts) {
        if plane.stride() < row_bytes || plane.row_count() != rows {
            return Err(invalid(
                "encode_vpx_frame",
                "VPx input plane geometry is inconsistent",
            ));
        }
        for row in 0..usize::try_from(rows)
            .map_err(|_| invalid("encode_vpx_frame", "VPx input row count overflowed"))?
        {
            let start = row
                .checked_mul(plane.stride())
                .ok_or_else(|| invalid("encode_vpx_frame", "VPx input row offset overflowed"))?;
            let end = start
                .checked_add(row_bytes)
                .ok_or_else(|| invalid("encode_vpx_frame", "VPx input row size overflowed"))?;
            pixels.extend_from_slice(&plane.bytes()[start..end]);
        }
    }
    Ok(pixels)
}

fn native_format(codec: VpxCodec, format: PixelFormat) -> Result<NativeFormat> {
    match (codec, format) {
        (VpxCodec::Vp8, PixelFormat::Yuv420p8) => Ok(NativeFormat::I420_8),
        (VpxCodec::Vp9, PixelFormat::Yuv420p8) => Ok(NativeFormat::I420_8),
        (VpxCodec::Vp9, PixelFormat::Yuv422p8) => Ok(NativeFormat::I422_8),
        (VpxCodec::Vp9, PixelFormat::Yuv444p8) => Ok(NativeFormat::I444_8),
        (VpxCodec::Vp9, PixelFormat::Yuv420p10) => Ok(NativeFormat::I420_10),
        (VpxCodec::Vp9, PixelFormat::Yuv422p10) => Ok(NativeFormat::I422_10),
        (VpxCodec::Vp9, PixelFormat::Yuv444p10) => Ok(NativeFormat::I444_10),
        (VpxCodec::Vp8, _) => Err(unsupported(
            "create_vpx_encoder",
            "VP8 encoding supports opaque 8-bit planar YUV 4:2:0 only",
        )),
        (VpxCodec::Vp9, _) => Err(unsupported(
            "create_vpx_encoder",
            "VP9 encoding supports opaque planar YUV 4:2:0, 4:2:2, or 4:4:4 at 8 or 10 bits",
        )),
    }
}

fn pixel_format(format: NativeFormat) -> PixelFormat {
    match format {
        NativeFormat::I420_8 => PixelFormat::Yuv420p8,
        NativeFormat::I422_8 => PixelFormat::Yuv422p8,
        NativeFormat::I444_8 => PixelFormat::Yuv444p8,
        NativeFormat::I420_10 => PixelFormat::Yuv420p10,
        NativeFormat::I422_10 => PixelFormat::Yuv422p10,
        NativeFormat::I444_10 => PixelFormat::Yuv444p10,
    }
}

fn plane_layout(width: u32, height: u32, format: NativeFormat) -> Result<Vec<(usize, u32)>> {
    let bytes = format.bits_per_sample();
    let width = usize::try_from(width)
        .map_err(|_| invalid("map_vpx_planes", "VPx frame width overflowed"))?;
    let luma_stride = width
        .checked_mul(bytes)
        .ok_or_else(|| invalid("map_vpx_planes", "VPx luma stride overflowed"))?;
    let (horizontal, vertical) = match format {
        NativeFormat::I420_8 | NativeFormat::I420_10 => (1, 1),
        NativeFormat::I422_8 | NativeFormat::I422_10 => (1, 0),
        NativeFormat::I444_8 | NativeFormat::I444_10 => (0, 0),
    };
    let chroma_width = if horizontal == 0 {
        width
    } else {
        width.div_ceil(2)
    };
    let chroma_height = if vertical == 0 {
        height
    } else {
        height.div_ceil(2)
    };
    let chroma_stride = chroma_width
        .checked_mul(bytes)
        .ok_or_else(|| invalid("map_vpx_planes", "VPx chroma stride overflowed"))?;
    Ok(vec![
        (luma_stride, height),
        (chroma_stride, chroma_height),
        (chroma_stride, chroma_height),
    ])
}

fn decoded_color(color_space: i32, color_range: i32, bit_depth: u32) -> ColorSpace {
    let range = if color_range == 1 {
        ColorRange::Full
    } else {
        ColorRange::Limited
    };
    match color_space {
        1 | 3 | 4 => ColorSpace::new(
            ColorPrimaries::Unspecified,
            TransferFunction::Bt709,
            MatrixCoefficients::Bt601,
            range,
        ),
        2 => ColorSpace::new(
            ColorPrimaries::Bt709,
            TransferFunction::Bt709,
            MatrixCoefficients::Bt709,
            range,
        ),
        5 => ColorSpace::new(
            ColorPrimaries::Bt2020,
            if bit_depth == 10 {
                TransferFunction::Bt2020TenBit
            } else {
                TransferFunction::Unspecified
            },
            MatrixCoefficients::Bt2020NonConstant,
            range,
        ),
        7 => ColorSpace::new(
            ColorPrimaries::Bt709,
            TransferFunction::Srgb,
            MatrixCoefficients::Rgb,
            range,
        ),
        _ => ColorSpace::new(
            ColorPrimaries::Unspecified,
            TransferFunction::Unspecified,
            MatrixCoefficients::Unspecified,
            range,
        ),
    }
}

fn native_color(color: ColorSpace) -> (i32, i32) {
    let space = match color.matrix() {
        MatrixCoefficients::Rgb => 7,
        MatrixCoefficients::Bt601 => 1,
        MatrixCoefficients::Bt709 => 2,
        MatrixCoefficients::Bt2020NonConstant | MatrixCoefficients::Bt2020Constant => 5,
        MatrixCoefficients::Unspecified => 0,
        _ => 0,
    };
    let range = i32::from(color.range() == ColorRange::Full);
    (space, range)
}

fn target_bitrate(format: VideoFormat) -> Result<u32> {
    let pixels = u64::from(format.width())
        .checked_mul(u64::from(format.height()))
        .ok_or_else(|| invalid("create_vpx_encoder", "VPx frame area overflowed"))?;
    let estimate = pixels
        .checked_mul(30)
        .and_then(|value| value.checked_mul(8))
        .map(|value| value / 1_000)
        .ok_or_else(|| invalid("create_vpx_encoder", "VPx bitrate estimate overflowed"))?;
    u32::try_from(estimate.clamp(256, u64::from(u32::MAX))).map_err(|_| {
        invalid(
            "create_vpx_encoder",
            "VPx bitrate exceeds the supported domain",
        )
    })
}

fn validate_alpha_metadata(metadata: &MediaMetadata) -> Result<()> {
    match metadata.get("video.alpha-mode") {
        None | Some(MetadataValue::Unsigned(0)) => Ok(()),
        Some(MetadataValue::Unsigned(_)) => Err(unsupported(
            "create_vpx_decoder",
            "VP8 and VP9 primary video decoding cannot discard a declared alpha payload",
        )),
        Some(_) => Err(invalid(
            "create_vpx_decoder",
            "video.alpha-mode metadata must contain an unsigned integer",
        )),
    }
}

fn ffi_failure(operation: &'static str, category: ErrorCategory, error: FfiError) -> Error {
    let recoverability = match category {
        ErrorCategory::Unavailable => Recoverability::Degraded,
        ErrorCategory::CorruptData | ErrorCategory::InvalidInput => Recoverability::UserCorrectable,
        _ => Recoverability::Terminal,
    };
    Error::new(category, recoverability, error.message().to_owned())
        .with_context(ErrorContext::new(COMPONENT, operation))
}

fn invalid(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unsupported(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unavailable(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn conflict(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn corrupt(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn internal(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
