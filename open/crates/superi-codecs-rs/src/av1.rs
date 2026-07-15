//! AV1 video decode and encode through permissive Rust implementations.
//!
//! Decode uses rav1d with its assembly features disabled. rav1d currently exposes its
//! dav1d-compatible boundary, so the small private ownership wrapper below contains every unsafe
//! call and copies decoded planes into validated Superi storage before releasing the picture.
//! Encode uses rav1e with optional binaries, assembly, signal handling, and global threading
//! disabled. Both paths exchange raw AV1 temporal-unit packets through `superi-media-io`.

use std::collections::VecDeque;
use std::mem::MaybeUninit;
use std::ptr::NonNull;
use std::sync::Arc;

use rav1d::include::dav1d::data::Dav1dData;
use rav1d::include::dav1d::dav1d::{Dav1dContext, Dav1dSettings};
use rav1d::include::dav1d::headers::{
    Dav1dSequenceHeader, Rav1dMatrixCoefficients, DAV1D_COLOR_PRI_BT2020, DAV1D_COLOR_PRI_BT709,
    DAV1D_COLOR_PRI_SMPTE432, DAV1D_MC_BT601, DAV1D_MC_BT709, DAV1D_MC_IDENTITY,
    DAV1D_PIXEL_LAYOUT_I400, DAV1D_PIXEL_LAYOUT_I420, DAV1D_PIXEL_LAYOUT_I422,
    DAV1D_PIXEL_LAYOUT_I444, DAV1D_TRC_BT2020_10BIT, DAV1D_TRC_BT2020_12BIT, DAV1D_TRC_BT470M,
    DAV1D_TRC_BT709, DAV1D_TRC_HLG, DAV1D_TRC_LINEAR, DAV1D_TRC_SMPTE2084, DAV1D_TRC_SRGB,
};
use rav1d::include::dav1d::picture::Dav1dPicture;
use rav1d::src::lib::{
    dav1d_close, dav1d_data_create, dav1d_data_unref, dav1d_default_settings, dav1d_flush,
    dav1d_get_picture, dav1d_open, dav1d_picture_unref, dav1d_send_data,
};
use rav1e::color::{
    ChromaSamplePosition, ColorDescription, ColorPrimaries as Rav1eColorPrimaries,
    MatrixCoefficients as Rav1eMatrixCoefficients, PixelRange,
    TransferCharacteristics as Rav1eTransferCharacteristics,
};
use rav1e::data::{FrameType, Rational};
use rav1e::prelude::{
    ChromaSampling, Config as Rav1eConfig, Context as Rav1eContext,
    EncoderConfig as Rav1eEncoderConfig, EncoderStatus, FrameParameters, Opaque, Pixel,
};
use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::time::{Duration, RationalTime, Timebase};
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendDescriptor, BackendRegistration, BackendTier,
    ChromaSampling as CapabilityChromaSampling, CodecCapability, CodecOperation,
    HardwareAcceleration, MediaBackend,
};
use superi_media_io::decode::{
    CpuVideoBuffer, DecodeOutput, Decoder, DecoderConfig, VideoFormat, VideoFrame,
    VideoFrameBuffer, VideoPlane,
};
use superi_media_io::demux::{
    BackendId, CodecId, MediaMetadata, MediaSource, MetadataValue, Packet, PacketTiming,
    SourceProbe, SourceProbeResult, SourceRequest, StreamKind,
};
use superi_media_io::encode::{
    EncodeInput, EncodeOutput, Encoder, EncoderConfig, EncoderMediaFormat,
};
use superi_media_io::operation::OperationContext;

/// Stable codec identifier used by streams, selection, and capability introspection.
pub const AV1_CODEC_ID: &str = "av1";

const MAX_PENDING_PACKETS: usize = 64;
// rav1d 1.0.0 accidentally aliases its exported BT.2020 NCL constant to CL. The typed values are
// correct, so derive both public H.273 codes from them until the Rust floor permits an upgrade.
const RAV1D_MC_BT2020_NCL: u32 = Rav1dMatrixCoefficients::BT2020_NCL.0 as u32;
const RAV1D_MC_BT2020_CL: u32 = Rav1dMatrixCoefficients::BT2020_CL.0 as u32;

/// Default permissive AV1 backend.
pub struct Av1Backend {
    descriptor: BackendDescriptor,
}

impl Av1Backend {
    /// Creates the backend with its stable identity.
    pub fn new() -> Result<Self> {
        Ok(Self {
            descriptor: BackendDescriptor::new(BackendId::new("rust-av1")?, "Rust AV1")?,
        })
    }

    /// Builds the primary registration for AV1 decode and encode.
    pub fn registration() -> Result<BackendRegistration> {
        let codec = CodecId::new(AV1_CODEC_ID)?;
        let codec_capabilities = av1_capabilities(&codec)?;
        BackendRegistration::new(
            Arc::new(Self::new()?),
            BackendCapabilities::new([
                BackendCapability::Decode(codec.clone()),
                BackendCapability::Encode(codec),
            ])
            .with_hardware_acceleration(HardwareAcceleration::Software)
            .with_codec_capabilities(codec_capabilities)?,
            100,
            BackendTier::Primary,
        )
    }
}

fn av1_capabilities(codec: &CodecId) -> Result<Vec<CodecCapability>> {
    let mut values = Vec::new();
    for operation in [CodecOperation::Decode, CodecOperation::Encode] {
        values.push(
            CodecCapability::new(operation, codec.clone())
                .with_profiles(["main"])?
                .with_levels_runtime()
                .with_bit_depths([8, 10])?
                .with_chroma_sampling([
                    CapabilityChromaSampling::Monochrome,
                    CapabilityChromaSampling::Cs420,
                ])?,
        );
        values.push(
            CodecCapability::new(operation, codec.clone())
                .with_profiles(["high"])?
                .with_levels_runtime()
                .with_bit_depths([8, 10])?
                .with_chroma_sampling([CapabilityChromaSampling::Cs444])?,
        );
        values.push(
            CodecCapability::new(operation, codec.clone())
                .with_profiles(["professional"])?
                .with_levels_runtime()
                .with_bit_depths([8, 10])?
                .with_chroma_sampling([CapabilityChromaSampling::Cs422])?,
        );
    }
    Ok(values)
}

impl MediaBackend for Av1Backend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        _probe: &SourceProbe<'_>,
        operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        operation.check("probe_av1_source")?;
        Ok(SourceProbeResult::NoMatch)
    }

    fn open_source(
        &self,
        _request: &SourceRequest,
        operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        operation.check("open_av1_source")?;
        Err(unsupported(
            "open_av1_source",
            "the AV1 codec backend does not open media containers",
        ))
    }

    fn create_decoder(
        &self,
        config: &DecoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>> {
        operation.check("create_av1_decoder")?;
        Ok(Box::new(Av1Decoder::new(config.clone())?))
    }

    fn create_encoder(
        &self,
        config: &EncoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        operation.check("create_av1_encoder")?;
        Ok(Box::new(Av1Encoder::new(config.clone())?))
    }
}

struct PendingDecodedFrame {
    sequence: i64,
    timestamp: i64,
    duration: u64,
    metadata: MediaMetadata,
}

struct Av1Decoder {
    config: DecoderConfig,
    rav1d: Rav1dHandle,
    input: VecDeque<Packet>,
    pending: VecDeque<PendingDecodedFrame>,
    next_sequence: i64,
    flushing: bool,
    ended: bool,
}

impl Av1Decoder {
    fn new(config: DecoderConfig) -> Result<Self> {
        require_av1_stream(&config)?;
        Ok(Self {
            config,
            rav1d: Rav1dHandle::new()?,
            input: VecDeque::new(),
            pending: VecDeque::new(),
            next_sequence: 0,
            flushing: false,
            ended: false,
        })
    }

    fn convert_picture(&mut self, picture: &Dav1dPicture) -> Result<VideoFrame> {
        let width = u32::try_from(picture.p.w).map_err(|_| {
            corrupt(
                "decode_av1_picture",
                "AV1 decoder returned an invalid frame width",
            )
        })?;
        let height = u32::try_from(picture.p.h).map_err(|_| {
            corrupt(
                "decode_av1_picture",
                "AV1 decoder returned an invalid frame height",
            )
        })?;
        if width == 0 || height == 0 {
            return Err(corrupt(
                "decode_av1_picture",
                "AV1 decoder returned empty frame dimensions",
            ));
        }

        let pixel_format = decoded_pixel_format(picture.p.layout, picture.p.bpc)?;
        let sequence = sequence_header(picture)?;
        let color_space = decoded_color_space(sequence);
        let planes = copy_picture_planes(picture, width, height, pixel_format)?;
        let buffer = Arc::new(CpuVideoBuffer::new(width, height, pixel_format, planes)?);

        let duration = u64::try_from(picture.m.duration).map_err(|_| {
            corrupt(
                "decode_av1_picture",
                "AV1 decoder returned a negative frame duration",
            )
        })?;
        #[cfg(target_os = "windows")]
        let picture_sequence = i64::from(picture.m.offset);
        #[cfg(not(target_os = "windows"))]
        let picture_sequence = picture.m.offset;
        let pending_index = self
            .pending
            .iter()
            .position(|pending| pending.sequence == picture_sequence)
            .ok_or_else(|| {
                internal(
                    "decode_av1_picture",
                    "AV1 decoder output could not be matched to packet timing",
                )
            })?;
        let pending = self.pending.remove(pending_index).ok_or_else(|| {
            internal(
                "decode_av1_picture",
                "AV1 packet timing disappeared before frame construction",
            )
        })?;
        if picture.m.timestamp != pending.timestamp || duration != pending.duration {
            return Err(internal(
                "decode_av1_picture",
                "rav1d changed the submitted AV1 packet timing properties",
            ));
        }
        let bit_depth = u64::try_from(picture.p.bpc).map_err(|_| {
            corrupt(
                "decode_av1_picture",
                "decoded AV1 picture reported a negative bit depth",
            )
        })?;
        let timebase = self.config.stream().timebase();
        let mut frame = VideoFrame::new(
            VideoFormat::new(width, height, pixel_format, color_space, AlphaMode::Opaque)?,
            RationalTime::new(pending.timestamp, timebase),
            Duration::new(pending.duration, timebase)?,
            buffer,
        )?;
        for (key, value) in pending.metadata.iter() {
            frame = frame.with_metadata(key, value.clone())?;
        }
        frame = frame
            .with_metadata(
                "codec.av1.profile",
                MetadataValue::Unsigned(u64::from(sequence.profile)),
            )?
            .with_metadata("codec.av1.bit-depth", MetadataValue::Unsigned(bit_depth))?
            .with_metadata(
                "codec.av1.color-primaries",
                MetadataValue::Unsigned(u64::from(sequence.pri)),
            )?
            .with_metadata(
                "codec.av1.transfer-characteristics",
                MetadataValue::Unsigned(u64::from(sequence.trc)),
            )?
            .with_metadata(
                "codec.av1.matrix-coefficients",
                MetadataValue::Unsigned(u64::from(sequence.mtrx)),
            )?
            .with_metadata(
                "codec.av1.color-range",
                MetadataValue::Unsigned(u64::from(sequence.color_range)),
            )?
            .with_metadata(
                "codec.av1.chroma-sample-position",
                MetadataValue::Unsigned(u64::from(sequence.chr)),
            )?;
        Ok(frame)
    }
}

impl Decoder for Av1Decoder {
    fn config(&self) -> &DecoderConfig {
        &self.config
    }

    fn send_packet(&mut self, packet: Packet, operation: &OperationContext) -> Result<()> {
        operation.check("send_av1_packet")?;
        if self.flushing {
            return Err(conflict(
                "send_av1_packet",
                "AV1 decoder cannot accept packets after flush",
            ));
        }
        if packet.stream_id() != self.config.stream().id() {
            return Err(conflict(
                "send_av1_packet",
                "AV1 packet stream does not match decoder configuration",
            ));
        }
        if packet.timing().timebase() != self.config.stream().timebase() {
            return Err(conflict(
                "send_av1_packet",
                "AV1 packet timebase does not match its stream",
            ));
        }
        if packet.data().is_empty() {
            return Err(corrupt(
                "send_av1_packet",
                "AV1 packet payload must not be empty",
            ));
        }
        if packet.timing().presentation_time().is_none() {
            return Err(corrupt(
                "send_av1_packet",
                "AV1 packet requires a presentation timestamp",
            ));
        }
        let duration = packet.timing().duration().ok_or_else(|| {
            corrupt(
                "send_av1_packet",
                "AV1 packet requires an exact frame duration",
            )
        })?;
        i64::try_from(duration.value()).map_err(|_| {
            corrupt(
                "send_av1_packet",
                "AV1 packet duration exceeds the decoder property range",
            )
        })?;
        if self.input.len() >= MAX_PENDING_PACKETS {
            return Err(resource_exhausted(
                "send_av1_packet",
                "AV1 decoder input queue is full and must be drained",
            ));
        }
        self.input.push_back(packet);
        Ok(())
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<DecodeOutput> {
        operation.check("receive_av1_frame")?;
        if self.ended {
            return Ok(DecodeOutput::EndOfStream);
        }

        loop {
            if let Some(picture) = self.rav1d.get_picture()? {
                let frame = self.convert_picture(&picture.0)?;
                return Ok(DecodeOutput::Frame(frame));
            }

            if let Some(packet) = self.input.front().cloned() {
                let sequence = self.next_sequence;
                let next_sequence = sequence.checked_add(1).ok_or_else(|| {
                    resource_exhausted(
                        "receive_av1_frame",
                        "AV1 decoder packet sequence exceeded its property range",
                    )
                })?;
                match self.rav1d.send_packet(&packet, sequence)? {
                    Rav1dSend::Consumed => {
                        let packet = self.input.pop_front().ok_or_else(|| {
                            internal(
                                "receive_av1_frame",
                                "AV1 input queue changed while submitting a packet",
                            )
                        })?;
                        self.next_sequence = next_sequence;
                        self.pending.push_back(PendingDecodedFrame {
                            sequence,
                            timestamp: packet
                                .timing()
                                .presentation_time()
                                .expect("validated AV1 packet presentation timestamp")
                                .value(),
                            duration: packet
                                .timing()
                                .duration()
                                .expect("validated AV1 packet duration")
                                .value(),
                            metadata: packet.metadata().clone(),
                        });
                        operation.check("receive_av1_frame")?;
                        continue;
                    }
                    Rav1dSend::NeedsDrain => {
                        return Err(internal(
                            "receive_av1_frame",
                            "AV1 decoder requested draining without exposing a frame",
                        ));
                    }
                }
            }

            if self.flushing {
                self.ended = true;
                return Ok(DecodeOutput::EndOfStream);
            }
            return Ok(DecodeOutput::NeedInput);
        }
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("flush_av1_decoder")?;
        self.flushing = true;
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_av1_decoder")?;
        self.rav1d.reset();
        self.input.clear();
        self.pending.clear();
        self.next_sequence = 0;
        self.flushing = false;
        self.ended = false;
        Ok(())
    }
}

enum Rav1dSend {
    Consumed,
    NeedsDrain,
}

struct Rav1dHandle {
    context: Option<Dav1dContext>,
}

// SAFETY: The raw context has exactly one owner, is never accessed concurrently, and is closed
// only by that owner. Moving the owner between threads does not create concurrent API calls.
#[allow(unsafe_code)]
unsafe impl Send for Rav1dHandle {}

#[allow(unsafe_code)]
impl Rav1dHandle {
    fn new() -> Result<Self> {
        let mut settings = MaybeUninit::<Dav1dSettings>::uninit();
        // SAFETY: The pointer names uninitialized storage for exactly one Dav1dSettings value.
        unsafe { dav1d_default_settings(NonNull::new_unchecked(settings.as_mut_ptr())) };
        // SAFETY: dav1d_default_settings initialized every field in the value above.
        let mut settings = unsafe { settings.assume_init() };
        settings.n_threads = 1;
        let mut context = None;
        // SAFETY: Both pointers reference live local values and rav1d writes only through them.
        let result = unsafe {
            dav1d_open(
                Some(NonNull::from(&mut context)),
                Some(NonNull::from(&mut settings)),
            )
        };
        if result.0 != 0 {
            return Err(rav1d_failure(
                "create_av1_decoder",
                "rav1d could not create a decoder context",
                result.0,
            ));
        }
        if context.is_none() {
            return Err(internal(
                "create_av1_decoder",
                "rav1d reported success without a decoder context",
            ));
        }
        Ok(Self { context })
    }

    fn send_packet(&mut self, packet: &Packet, sequence: i64) -> Result<Rav1dSend> {
        let mut data = Dav1dData::default();
        // SAFETY: The output pointer references live storage and the requested allocation length
        // is the exact packet byte length copied below.
        let destination =
            unsafe { dav1d_data_create(Some(NonNull::from(&mut data)), packet.data().len()) };
        if destination.is_null() {
            return Err(resource_exhausted(
                "send_av1_packet",
                "rav1d could not allocate packet storage",
            ));
        }
        // SAFETY: dav1d_data_create allocated packet.data().len() writable bytes at destination.
        unsafe {
            destination.copy_from_nonoverlapping(packet.data().as_ptr(), packet.data().len())
        };
        data.m.timestamp = packet
            .timing()
            .presentation_time()
            .expect("validated AV1 packet presentation timestamp")
            .value();
        data.m.duration = i64::try_from(
            packet
                .timing()
                .duration()
                .expect("validated AV1 packet duration")
                .value(),
        )
        .expect("validated AV1 packet duration range");
        #[cfg(target_os = "windows")]
        let rav1d_offset = sequence.try_into().map_err(|_| {
            resource_exhausted(
                "send_av1_packet",
                "AV1 decoder packet sequence exceeds the rav1d offset range",
            )
        })?;
        #[cfg(not(target_os = "windows"))]
        let rav1d_offset = sequence;
        data.m.offset = rav1d_offset;
        data.m.size = packet.data().len();

        let context = self
            .context
            .ok_or_else(|| internal("send_av1_packet", "rav1d decoder context is not available"))?;
        // SAFETY: context is owned by self and data is a live rav1d allocation for this call.
        let result = unsafe { dav1d_send_data(Some(context), Some(NonNull::from(&mut data))) };
        // SAFETY: data remains a live Dav1dData value. A consumed value is empty, while an
        // unconsumed value still owns the allocation and must release it here.
        unsafe { dav1d_data_unref(Some(NonNull::from(&mut data))) };
        if result.0 == 0 {
            Ok(Rav1dSend::Consumed)
        } else if result.0 == -libc::EAGAIN {
            Ok(Rav1dSend::NeedsDrain)
        } else {
            Err(rav1d_failure(
                "send_av1_packet",
                "AV1 packet data is malformed or unsupported",
                result.0,
            ))
        }
    }

    fn get_picture(&mut self) -> Result<Option<OwnedDav1dPicture>> {
        let context = self.context.ok_or_else(|| {
            internal(
                "receive_av1_frame",
                "rav1d decoder context is not available",
            )
        })?;
        let mut picture = Dav1dPicture::default();
        // SAFETY: context is owned by self and picture is live writable output storage.
        let result = unsafe { dav1d_get_picture(Some(context), Some(NonNull::from(&mut picture))) };
        if result.0 == 0 {
            Ok(Some(OwnedDav1dPicture(picture)))
        } else if result.0 == -libc::EAGAIN {
            Ok(None)
        } else {
            Err(rav1d_failure(
                "receive_av1_frame",
                "rav1d could not decode the submitted AV1 data",
                result.0,
            ))
        }
    }

    fn reset(&mut self) {
        if let Some(context) = self.context {
            // SAFETY: context is owned by self and remains open for reuse after flush.
            unsafe { dav1d_flush(context) };
        }
    }
}

#[allow(unsafe_code)]
impl Drop for Rav1dHandle {
    fn drop(&mut self) {
        if self.context.is_some() {
            // SAFETY: self owns the only live context handle and will not use it after this call.
            unsafe { dav1d_close(Some(NonNull::from(&mut self.context))) };
        }
    }
}

struct OwnedDav1dPicture(Dav1dPicture);

#[allow(unsafe_code)]
impl Drop for OwnedDav1dPicture {
    fn drop(&mut self) {
        // SAFETY: self owns the picture returned by rav1d and releases it exactly once here.
        unsafe { dav1d_picture_unref(Some(NonNull::from(&mut self.0))) };
    }
}

fn require_av1_stream(config: &DecoderConfig) -> Result<()> {
    if config.stream().codec().as_str() != AV1_CODEC_ID {
        return Err(conflict(
            "create_av1_decoder",
            "decoder configuration does not select AV1",
        ));
    }
    if config.stream().kind() != StreamKind::Video {
        return Err(conflict(
            "create_av1_decoder",
            "AV1 decoder requires a video stream",
        ));
    }
    Ok(())
}

fn sequence_header(picture: &Dav1dPicture) -> Result<&Dav1dSequenceHeader> {
    let pointer = picture.seq_hdr.ok_or_else(|| {
        corrupt(
            "decode_av1_picture",
            "AV1 decoder returned a picture without a sequence header",
        )
    })?;
    // SAFETY: The pointer is owned by picture and remains valid until picture is released after
    // the returned reference is no longer used.
    #[allow(unsafe_code)]
    let sequence = unsafe { pointer.as_ref() };
    Ok(sequence)
}

fn decoded_pixel_format(layout: u32, bit_depth: i32) -> Result<PixelFormat> {
    match (layout, bit_depth) {
        (DAV1D_PIXEL_LAYOUT_I400, 8) => Ok(PixelFormat::R8Unorm),
        (DAV1D_PIXEL_LAYOUT_I420, 8) => Ok(PixelFormat::Yuv420p8),
        (DAV1D_PIXEL_LAYOUT_I422, 8) => Ok(PixelFormat::Yuv422p8),
        (DAV1D_PIXEL_LAYOUT_I444, 8) => Ok(PixelFormat::Yuv444p8),
        (DAV1D_PIXEL_LAYOUT_I420, 10) => Ok(PixelFormat::Yuv420p10),
        (DAV1D_PIXEL_LAYOUT_I422, 10) => Ok(PixelFormat::Yuv422p10),
        (DAV1D_PIXEL_LAYOUT_I444, 10) => Ok(PixelFormat::Yuv444p10),
        (DAV1D_PIXEL_LAYOUT_I400, 10) => Err(unsupported(
            "decode_av1_picture",
            "10-bit monochrome AV1 has no exact public pixel format",
        )),
        (_, 12) => Err(unsupported(
            "decode_av1_picture",
            "12-bit AV1 has no exact public pixel format",
        )),
        _ => Err(unsupported(
            "decode_av1_picture",
            "AV1 pixel layout or precision is not supported",
        )),
    }
}

fn decoded_color_space(sequence: &Dav1dSequenceHeader) -> ColorSpace {
    let primaries = match sequence.pri {
        DAV1D_COLOR_PRI_BT709 => ColorPrimaries::Bt709,
        DAV1D_COLOR_PRI_BT2020 => ColorPrimaries::Bt2020,
        DAV1D_COLOR_PRI_SMPTE432 => ColorPrimaries::DisplayP3,
        _ => ColorPrimaries::Unspecified,
    };
    let transfer = match sequence.trc {
        DAV1D_TRC_LINEAR => TransferFunction::Linear,
        DAV1D_TRC_SRGB => TransferFunction::Srgb,
        DAV1D_TRC_BT709 => TransferFunction::Bt709,
        DAV1D_TRC_BT2020_10BIT => TransferFunction::Bt2020TenBit,
        DAV1D_TRC_BT2020_12BIT => TransferFunction::Bt2020TwelveBit,
        DAV1D_TRC_BT470M => TransferFunction::Gamma22,
        DAV1D_TRC_SMPTE2084 => TransferFunction::Pq,
        DAV1D_TRC_HLG => TransferFunction::Hlg,
        _ => TransferFunction::Unspecified,
    };
    let matrix = match sequence.mtrx {
        DAV1D_MC_IDENTITY => MatrixCoefficients::Rgb,
        DAV1D_MC_BT601 => MatrixCoefficients::Bt601,
        DAV1D_MC_BT709 => MatrixCoefficients::Bt709,
        RAV1D_MC_BT2020_NCL => MatrixCoefficients::Bt2020NonConstant,
        RAV1D_MC_BT2020_CL => MatrixCoefficients::Bt2020Constant,
        _ => MatrixCoefficients::Unspecified,
    };
    let range = if sequence.color_range == 0 {
        ColorRange::Limited
    } else {
        ColorRange::Full
    };
    ColorSpace::new(primaries, transfer, matrix, range)
}

#[allow(unsafe_code)]
fn copy_picture_planes(
    picture: &Dav1dPicture,
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
) -> Result<Vec<VideoPlane>> {
    let bytes_per_sample = if pixel_format.bits_per_component() > 8 {
        2_usize
    } else {
        1_usize
    };
    let chroma_width = match pixel_format {
        PixelFormat::Yuv420p8
        | PixelFormat::Yuv420p10
        | PixelFormat::Yuv422p8
        | PixelFormat::Yuv422p10 => width.div_ceil(2),
        _ => width,
    };
    let chroma_height = match pixel_format {
        PixelFormat::Yuv420p8 | PixelFormat::Yuv420p10 => height.div_ceil(2),
        _ => height,
    };
    let geometry = if pixel_format == PixelFormat::R8Unorm {
        vec![(width, height, picture.stride[0])]
    } else {
        vec![
            (width, height, picture.stride[0]),
            (chroma_width, chroma_height, picture.stride[1]),
            (chroma_width, chroma_height, picture.stride[1]),
        ]
    };
    let mut planes = Vec::with_capacity(geometry.len());
    for (index, (plane_width, rows, source_stride)) in geometry.into_iter().enumerate() {
        let row_bytes = usize::try_from(plane_width)
            .ok()
            .and_then(|width| width.checked_mul(bytes_per_sample))
            .ok_or_else(|| {
                resource_exhausted(
                    "decode_av1_picture",
                    "AV1 decoded plane row size exceeds this platform",
                )
            })?;
        let absolute_stride = source_stride
            .checked_abs()
            .and_then(|stride| usize::try_from(stride).ok())
            .ok_or_else(|| {
                corrupt(
                    "decode_av1_picture",
                    "AV1 decoder returned an invalid plane stride",
                )
            })?;
        if absolute_stride < row_bytes {
            return Err(corrupt(
                "decode_av1_picture",
                "AV1 decoder returned a plane stride smaller than its visible row",
            ));
        }
        let base = picture.data[index].ok_or_else(|| {
            corrupt(
                "decode_av1_picture",
                "AV1 decoder returned a missing visible plane",
            )
        })?;
        let capacity = row_bytes.checked_mul(rows as usize).ok_or_else(|| {
            resource_exhausted(
                "decode_av1_picture",
                "AV1 decoded plane size exceeds this platform",
            )
        })?;
        let mut bytes = Vec::with_capacity(capacity);
        for row in 0..rows as isize {
            // SAFETY: rav1d guarantees each returned visible plane has rows addressable by its
            // signed stride until the picture is released. Stride and row width were checked.
            let row_pointer = unsafe {
                base.as_ptr()
                    .cast::<u8>()
                    .offset(row.checked_mul(source_stride).ok_or_else(|| {
                        corrupt(
                            "decode_av1_picture",
                            "AV1 decoded plane row offset overflowed",
                        )
                    })?)
            };
            if bytes_per_sample == 1 {
                // SAFETY: row_pointer names at least row_bytes visible bytes for this row.
                let row = unsafe { std::slice::from_raw_parts(row_pointer, row_bytes) };
                bytes.extend_from_slice(row);
            } else {
                // SAFETY: rav1d 16-bit pictures store aligned native u16 samples and plane_width
                // samples are visible in this row.
                let row = unsafe {
                    std::slice::from_raw_parts(row_pointer.cast::<u16>(), plane_width as usize)
                };
                bytes.extend(row.iter().flat_map(|sample| sample.to_le_bytes()));
            }
        }
        planes.push(VideoPlane::new(Arc::from(bytes), row_bytes, rows)?);
    }
    Ok(planes)
}

#[derive(Clone)]
struct EncodedFrameContext {
    timestamp: i64,
    duration: u64,
    metadata: MediaMetadata,
}

enum Rav1eState {
    Eight(Rav1eContext<u8>),
    Ten(Rav1eContext<u16>),
}

impl Rav1eState {
    fn new(format: VideoFormat, timebase: Timebase) -> Result<(Self, Arc<[u8]>)> {
        let (chroma_sampling, bit_depth) = rav1e_pixel_format(format.pixel_format())?;
        if format.alpha_mode() != AlphaMode::Opaque {
            return Err(unsupported(
                "create_av1_encoder",
                "AV1 video encoding does not silently discard alpha",
            ));
        }
        let mut encoder = Rav1eEncoderConfig::with_speed_preset(10);
        encoder.width = format.width() as usize;
        encoder.height = format.height() as usize;
        encoder.time_base = Rational::new(
            u64::from(timebase.denominator()),
            u64::from(timebase.numerator()),
        );
        encoder.bit_depth = bit_depth;
        encoder.chroma_sampling = chroma_sampling;
        encoder.chroma_sample_position = ChromaSamplePosition::Unknown;
        encoder.pixel_range = rav1e_range(format.color_space().range());
        encoder.color_description = rav1e_color_description(format.color_space())?;
        encoder.enable_timing_info = true;
        encoder.low_latency = true;
        encoder.quantizer = 80;
        let config = Rav1eConfig::new()
            .with_encoder_config(encoder)
            .with_threads(1);
        if bit_depth == 8 {
            let context = config.new_context::<u8>().map_err(|error| {
                invalid_with_detail(
                    "create_av1_encoder",
                    "rav1e rejected the AV1 encoder configuration",
                    error.to_string(),
                )
            })?;
            let header = Arc::from(context.container_sequence_header());
            Ok((Self::Eight(context), header))
        } else {
            let context = config.new_context::<u16>().map_err(|error| {
                invalid_with_detail(
                    "create_av1_encoder",
                    "rav1e rejected the AV1 encoder configuration",
                    error.to_string(),
                )
            })?;
            let header = Arc::from(context.container_sequence_header());
            Ok((Self::Ten(context), header))
        }
    }

    fn send(
        &mut self,
        input: &CpuVideoBuffer,
        context: EncodedFrameContext,
        operation: &OperationContext,
    ) -> Result<()> {
        match self {
            Self::Eight(encoder) => fill_and_send(encoder, input, 1, context, operation),
            Self::Ten(encoder) => fill_and_send(encoder, input, 2, context, operation),
        }
    }

    fn receive(&mut self, config: &EncoderConfig, header: &Arc<[u8]>) -> Result<EncodeOutput> {
        match self {
            Self::Eight(encoder) => receive_rav1e_packet(encoder, config, header),
            Self::Ten(encoder) => receive_rav1e_packet(encoder, config, header),
        }
    }

    fn flush(&mut self) {
        match self {
            Self::Eight(encoder) => encoder.flush(),
            Self::Ten(encoder) => encoder.flush(),
        }
    }
}

struct Av1Encoder {
    config: EncoderConfig,
    rav1e: Rav1eState,
    sequence_header: Arc<[u8]>,
    flushing: bool,
    ended: bool,
}

impl Av1Encoder {
    fn new(config: EncoderConfig) -> Result<Self> {
        if config.codec().as_str() != AV1_CODEC_ID {
            return Err(conflict(
                "create_av1_encoder",
                "encoder configuration does not select AV1",
            ));
        }
        let EncoderMediaFormat::Video(format) = config.media_format() else {
            return Err(conflict(
                "create_av1_encoder",
                "AV1 encoder requires a video format",
            ));
        };
        let (rav1e, sequence_header) = Rav1eState::new(*format, config.timebase())?;
        Ok(Self {
            config,
            rav1e,
            sequence_header,
            flushing: false,
            ended: false,
        })
    }
}

impl Encoder for Av1Encoder {
    fn config(&self) -> &EncoderConfig {
        &self.config
    }

    fn send(&mut self, input: EncodeInput, operation: &OperationContext) -> Result<()> {
        operation.check("send_av1_frame")?;
        if self.flushing {
            return Err(conflict(
                "send_av1_frame",
                "AV1 encoder cannot accept frames after flush",
            ));
        }
        let EncodeInput::Video(frame) = input else {
            return Err(conflict(
                "send_av1_frame",
                "AV1 encoder requires video frames",
            ));
        };
        let EncoderMediaFormat::Video(expected) = self.config.media_format() else {
            return Err(internal(
                "send_av1_frame",
                "AV1 encoder lost its video configuration",
            ));
        };
        if frame.format() != *expected {
            return Err(conflict(
                "send_av1_frame",
                "AV1 frame format does not match encoder configuration",
            ));
        }
        if frame.timestamp().timebase() != self.config.timebase()
            || frame.duration().timebase() != self.config.timebase()
        {
            return Err(conflict(
                "send_av1_frame",
                "AV1 frame timing does not match encoder timebase",
            ));
        }
        let cpu = frame
            .buffer()
            .as_any()
            .downcast_ref::<CpuVideoBuffer>()
            .ok_or_else(|| {
                unsupported(
                    "send_av1_frame",
                    "software AV1 encoding requires CPU-addressable frame planes",
                )
            })?;
        let context = EncodedFrameContext {
            timestamp: frame.timestamp().value(),
            duration: frame.duration().value(),
            metadata: frame.metadata().clone(),
        };
        self.rav1e.send(cpu, context, operation)
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<EncodeOutput> {
        operation.check("receive_av1_packet")?;
        if self.ended {
            return Ok(EncodeOutput::EndOfStream);
        }
        let output = self.rav1e.receive(&self.config, &self.sequence_header)?;
        if matches!(output, EncodeOutput::EndOfStream) {
            self.ended = true;
        }
        Ok(output)
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("flush_av1_encoder")?;
        if !self.flushing {
            self.rav1e.flush();
            self.flushing = true;
        }
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_av1_encoder")?;
        let EncoderMediaFormat::Video(format) = self.config.media_format() else {
            return Err(internal(
                "reset_av1_encoder",
                "AV1 encoder lost its video configuration",
            ));
        };
        let (rav1e, header) = Rav1eState::new(*format, self.config.timebase())?;
        self.rav1e = rav1e;
        self.sequence_header = header;
        self.flushing = false;
        self.ended = false;
        Ok(())
    }
}

fn rav1e_pixel_format(format: PixelFormat) -> Result<(ChromaSampling, usize)> {
    match format {
        PixelFormat::R8Unorm => Ok((ChromaSampling::Cs400, 8)),
        PixelFormat::Yuv420p8 | PixelFormat::Nv12 => Ok((ChromaSampling::Cs420, 8)),
        PixelFormat::Yuv422p8 => Ok((ChromaSampling::Cs422, 8)),
        PixelFormat::Yuv444p8 => Ok((ChromaSampling::Cs444, 8)),
        PixelFormat::Yuv420p10 | PixelFormat::P010 => Ok((ChromaSampling::Cs420, 10)),
        PixelFormat::Yuv422p10 => Ok((ChromaSampling::Cs422, 10)),
        PixelFormat::Yuv444p10 => Ok((ChromaSampling::Cs444, 10)),
        _ => Err(unsupported(
            "create_av1_encoder",
            "AV1 encoding supports opaque monochrome and 8-bit or 10-bit YUV input",
        )),
    }
}

fn rav1e_range(range: ColorRange) -> PixelRange {
    match range {
        ColorRange::Full => PixelRange::Full,
        ColorRange::Limited | ColorRange::Unspecified => PixelRange::Limited,
        _ => PixelRange::Limited,
    }
}

fn rav1e_color_description(color: ColorSpace) -> Result<Option<ColorDescription>> {
    if color == ColorSpace::UNSPECIFIED {
        return Ok(None);
    }
    let color_primaries = match color.primaries() {
        ColorPrimaries::Unspecified => Rav1eColorPrimaries::Unspecified,
        ColorPrimaries::Bt709 => Rav1eColorPrimaries::BT709,
        ColorPrimaries::Bt2020 => Rav1eColorPrimaries::BT2020,
        ColorPrimaries::DisplayP3 => Rav1eColorPrimaries::SMPTE432,
        ColorPrimaries::AcesAp0 | ColorPrimaries::AcesAp1 => {
            return Err(unsupported(
                "create_av1_encoder",
                "AV1 bitstream signaling cannot represent the requested ACES primaries",
            ));
        }
        _ => {
            return Err(unsupported(
                "create_av1_encoder",
                "AV1 bitstream signaling cannot represent the requested color primaries",
            ));
        }
    };
    let transfer_characteristics = match color.transfer() {
        TransferFunction::Unspecified => Rav1eTransferCharacteristics::Unspecified,
        TransferFunction::Linear => Rav1eTransferCharacteristics::Linear,
        TransferFunction::Srgb => Rav1eTransferCharacteristics::SRGB,
        TransferFunction::Bt709 => Rav1eTransferCharacteristics::BT709,
        TransferFunction::Bt2020TenBit => Rav1eTransferCharacteristics::BT2020_10Bit,
        TransferFunction::Bt2020TwelveBit => Rav1eTransferCharacteristics::BT2020_12Bit,
        TransferFunction::Gamma22 => Rav1eTransferCharacteristics::BT470M,
        TransferFunction::Pq => Rav1eTransferCharacteristics::SMPTE2084,
        TransferFunction::Hlg => Rav1eTransferCharacteristics::HLG,
        TransferFunction::Gamma24 => {
            return Err(unsupported(
                "create_av1_encoder",
                "AV1 bitstream signaling has no exact gamma 2.4 transfer code",
            ));
        }
        _ => {
            return Err(unsupported(
                "create_av1_encoder",
                "AV1 bitstream signaling cannot represent the requested transfer function",
            ));
        }
    };
    let matrix_coefficients = match color.matrix() {
        MatrixCoefficients::Unspecified => Rav1eMatrixCoefficients::Unspecified,
        MatrixCoefficients::Rgb => Rav1eMatrixCoefficients::Identity,
        MatrixCoefficients::Bt601 => Rav1eMatrixCoefficients::BT601,
        MatrixCoefficients::Bt709 => Rav1eMatrixCoefficients::BT709,
        MatrixCoefficients::Bt2020NonConstant => Rav1eMatrixCoefficients::BT2020NCL,
        MatrixCoefficients::Bt2020Constant => Rav1eMatrixCoefficients::BT2020CL,
        _ => {
            return Err(unsupported(
                "create_av1_encoder",
                "AV1 bitstream signaling cannot represent the requested matrix coefficients",
            ));
        }
    };
    Ok(Some(ColorDescription {
        color_primaries,
        transfer_characteristics,
        matrix_coefficients,
    }))
}

fn fill_and_send<T: Pixel>(
    encoder: &mut Rav1eContext<T>,
    input: &CpuVideoBuffer,
    bytes_per_sample: usize,
    frame_context: EncodedFrameContext,
    operation: &OperationContext,
) -> Result<()> {
    let mut frame = encoder.new_frame();
    let format = input.pixel_format();
    match format {
        PixelFormat::Nv12 | PixelFormat::P010 => {
            copy_plane(
                &mut frame.planes[0],
                &input.planes()[0],
                bytes_per_sample,
                operation,
            )?;
            let (u, v, stride) = deinterleave_chroma(
                &input.planes()[1],
                input.width(),
                input.height(),
                bytes_per_sample,
                operation,
            )?;
            frame.planes[1].copy_from_raw_u8(&u, stride, bytes_per_sample);
            frame.planes[2].copy_from_raw_u8(&v, stride, bytes_per_sample);
        }
        PixelFormat::R8Unorm => {
            copy_plane(
                &mut frame.planes[0],
                &input.planes()[0],
                bytes_per_sample,
                operation,
            )?;
        }
        _ => {
            for (target, source) in frame.planes.iter_mut().zip(input.planes()) {
                copy_plane(target, source, bytes_per_sample, operation)?;
            }
        }
    }
    let parameters = FrameParameters {
        opaque: Some(Opaque::new(frame_context)),
        ..FrameParameters::default()
    };
    encoder
        .send_frame((frame, parameters))
        .map_err(|status| rav1e_status("send_av1_frame", status))
}

fn copy_plane<T: Pixel>(
    target: &mut rav1e::prelude::Plane<T>,
    source: &VideoPlane,
    bytes_per_sample: usize,
    operation: &OperationContext,
) -> Result<()> {
    operation.check("copy_av1_input_plane")?;
    target.copy_from_raw_u8(source.bytes(), source.stride(), bytes_per_sample);
    operation.check("copy_av1_input_plane")
}

fn deinterleave_chroma(
    source: &VideoPlane,
    width: u32,
    height: u32,
    bytes_per_sample: usize,
    operation: &OperationContext,
) -> Result<(Vec<u8>, Vec<u8>, usize)> {
    let chroma_width = width.div_ceil(2) as usize;
    let rows = height.div_ceil(2) as usize;
    let plane_stride = chroma_width.checked_mul(bytes_per_sample).ok_or_else(|| {
        resource_exhausted(
            "copy_av1_input_plane",
            "AV1 chroma row size exceeds this platform",
        )
    })?;
    let interleaved_row = plane_stride.checked_mul(2).ok_or_else(|| {
        resource_exhausted(
            "copy_av1_input_plane",
            "AV1 interleaved chroma row size exceeds this platform",
        )
    })?;
    if source.stride() < interleaved_row {
        return Err(conflict(
            "copy_av1_input_plane",
            "semiplanar AV1 input stride is smaller than its chroma row",
        ));
    }
    let capacity = plane_stride.checked_mul(rows).ok_or_else(|| {
        resource_exhausted(
            "copy_av1_input_plane",
            "AV1 chroma plane size exceeds this platform",
        )
    })?;
    let mut u = Vec::with_capacity(capacity);
    let mut v = Vec::with_capacity(capacity);
    for row in source.bytes().chunks_exact(source.stride()).take(rows) {
        operation.check("copy_av1_input_plane")?;
        for pair in row[..interleaved_row].chunks_exact(bytes_per_sample * 2) {
            u.extend_from_slice(&pair[..bytes_per_sample]);
            v.extend_from_slice(&pair[bytes_per_sample..]);
        }
    }
    Ok((u, v, plane_stride))
}

fn receive_rav1e_packet<T: Pixel>(
    encoder: &mut Rav1eContext<T>,
    config: &EncoderConfig,
    sequence_header: &Arc<[u8]>,
) -> Result<EncodeOutput> {
    loop {
        let packet = match encoder.receive_packet() {
            Ok(packet) => packet,
            Err(EncoderStatus::Encoded) => continue,
            Err(EncoderStatus::NeedMoreData) => return Ok(EncodeOutput::NeedInput),
            Err(EncoderStatus::LimitReached) => return Ok(EncodeOutput::EndOfStream),
            Err(status) => return Err(rav1e_status("receive_av1_packet", status)),
        };
        let context = packet
            .opaque
            .ok_or_else(|| {
                internal(
                    "receive_av1_packet",
                    "rav1e packet lost its frame timing context",
                )
            })?
            .downcast::<EncodedFrameContext>()
            .map_err(|_| {
                internal(
                    "receive_av1_packet",
                    "rav1e packet returned an unexpected frame context type",
                )
            })?;
        let timing = PacketTiming::new(
            config.timebase(),
            Some(context.timestamp),
            Some(context.timestamp),
            Some(context.duration),
        )?;
        let mut output = Packet::new(config.stream_id(), Arc::from(packet.data), timing)
            .with_keyframe(packet.frame_type == FrameType::KEY);
        for (key, value) in context.metadata.iter() {
            output.metadata_mut().insert(key, value.clone())?;
        }
        output.metadata_mut().insert(
            "codec.av1.input-frame",
            MetadataValue::Unsigned(packet.input_frameno),
        )?;
        output.metadata_mut().insert(
            "codec.av1.quantizer",
            MetadataValue::Unsigned(u64::from(packet.qp)),
        )?;
        if packet.input_frameno == 0 {
            output.metadata_mut().insert(
                "codec.configuration",
                MetadataValue::Bytes(Arc::clone(sequence_header)),
            )?;
        }
        return Ok(EncodeOutput::Packet(output));
    }
}

fn rav1e_status(operation: &'static str, status: EncoderStatus) -> Error {
    let (category, recoverability, message) = match status {
        EncoderStatus::EnoughData => (
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            "rav1e cannot accept more AV1 frames in this stream lifetime",
        ),
        EncoderStatus::NeedMoreData | EncoderStatus::Encoded => (
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "rav1e returned an unexpected nonterminal lifecycle state",
        ),
        EncoderStatus::LimitReached => (
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            "rav1e reached its AV1 stream frame limit",
        ),
        EncoderStatus::NotReady => (
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "rav1e unexpectedly requested two-pass state",
        ),
        EncoderStatus::Failure => (
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "rav1e failed while processing AV1 data",
        ),
    };
    Error::new(category, recoverability, message).with_context(
        ErrorContext::new("superi-codecs-rs.av1", operation)
            .with_field("rav1e_status", status.to_string()),
    )
}

fn rav1d_failure(operation: &'static str, message: &'static str, code: i32) -> Error {
    let (category, recoverability) = if code == -libc::ENOMEM {
        (ErrorCategory::ResourceExhausted, Recoverability::Retryable)
    } else {
        (ErrorCategory::CorruptData, Recoverability::Degraded)
    };
    Error::new(category, recoverability, message).with_context(
        ErrorContext::new("superi-codecs-rs.av1", operation)
            .with_field("rav1d_code", code.to_string()),
    )
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-rs.av1", operation))
}

fn invalid_with_detail(operation: &'static str, message: &'static str, detail: String) -> Error {
    invalid(operation, message).with_context(
        ErrorContext::new("superi-codecs-rs.av1", operation).with_field("detail", detail),
    )
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-rs.av1", operation))
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-rs.av1", operation))
}

fn corrupt(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-rs.av1", operation))
}

fn resource_exhausted(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::Retryable,
        message,
    )
    .with_context(ErrorContext::new("superi-codecs-rs.av1", operation))
}

fn internal(operation: &'static str, message: &'static str) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(ErrorContext::new("superi-codecs-rs.av1", operation))
}
