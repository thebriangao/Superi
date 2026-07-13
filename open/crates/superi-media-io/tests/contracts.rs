use std::any::Any;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use superi_core::color_space::ColorSpace;
use superi_core::error::{ErrorCategory, Result};
use superi_core::ids::MediaId;
use superi_core::pixel::{AlphaMode, ChannelLayout, PixelFormat, SampleFormat};
use superi_core::time::{Duration, RationalTime, SampleTime, Timebase};
use superi_media_io::audio_io::{AudioBlock, AudioFormat, AudioPlane};
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendDescriptor, BackendRegistration,
    BackendRegistry, BackendRequirement, BackendTier, FallbackPolicy, MediaBackend,
};
use superi_media_io::decode::{
    CpuVideoBuffer, DecodeOutput, Decoder, DecoderConfig, FrameStorageKind, VideoFormat,
    VideoFrame, VideoFrameBuffer, VideoPlane,
};
use superi_media_io::demux::{
    BackendId, CodecId, MediaMetadata, MediaSource, MetadataValue, Packet, PacketTiming, SeekMode,
    SeekRequest, SourceIdentity, SourceInfo, SourceLocation, SourceProbe, SourceProbeResult,
    SourceRequest, StreamId, StreamInfo, StreamKind,
};
use superi_media_io::encode::{EncodeInput, EncodeOutput, Encoder, EncoderConfig};
use superi_media_io::operation::{MediaPriority, OperationContext};
use superi_media_io::read::ReadOutcome;

fn video_format() -> VideoFormat {
    VideoFormat::new(
        2,
        2,
        PixelFormat::Rgba8Unorm,
        ColorSpace::SRGB,
        AlphaMode::Straight,
    )
    .unwrap()
}

fn video_stream() -> StreamInfo {
    StreamInfo::new(
        StreamId::new(3),
        StreamKind::Video,
        CodecId::new("av1").unwrap(),
        Timebase::integer(24).unwrap(),
    )
}

fn packet(value: u8, frame: i64) -> Packet {
    let timing = PacketTiming::new(
        Timebase::integer(24).unwrap(),
        Some(frame),
        Some(frame - 1),
        Some(1),
    )
    .unwrap();
    Packet::new(StreamId::new(3), Arc::from([value]), timing)
        .with_keyframe(frame == 0)
        .with_metadata("container.offset", MetadataValue::Unsigned(42))
        .unwrap()
}

fn frame(value: u8, frame: i64) -> VideoFrame {
    let format = video_format();
    let plane = VideoPlane::new(Arc::from(vec![value; 16]), 8, 2).unwrap();
    let buffer = Arc::new(CpuVideoBuffer::new(2, 2, PixelFormat::Rgba8Unorm, vec![plane]).unwrap());
    VideoFrame::new(
        format,
        RationalTime::new(frame, Timebase::integer(24).unwrap()),
        Duration::new(1, Timebase::integer(24).unwrap()).unwrap(),
        buffer,
    )
    .unwrap()
}

#[test]
fn values_preserve_timing_metadata_precision_and_alpha() {
    assert!(BackendId::new("FFmpeg").is_err());
    assert!(CodecId::new("video/av1").is_err());

    let mut packet = packet(9, 7);
    packet
        .metadata_mut()
        .insert("hdr.mastering", MetadataValue::Bytes(Arc::from([1, 2, 3])))
        .unwrap();
    assert_eq!(packet.timing().presentation_time().unwrap().value(), 7);
    assert_eq!(packet.timing().decode_time().unwrap().value(), 6);
    assert_eq!(packet.timing().duration().unwrap().value(), 1);
    assert!(!packet.is_keyframe());
    assert_eq!(
        packet.metadata().get("container.offset"),
        Some(&MetadataValue::Unsigned(42))
    );
    assert_eq!(
        packet.metadata().get("hdr.mastering"),
        Some(&MetadataValue::Bytes(Arc::from([1, 2, 3])))
    );

    let video = frame(255, 7);
    assert_eq!(video.format().pixel_format(), PixelFormat::Rgba8Unorm);
    assert_eq!(video.format().alpha_mode(), AlphaMode::Straight);
    assert_eq!(video.format().color_space(), ColorSpace::SRGB);
    let cpu = video
        .buffer()
        .as_any()
        .downcast_ref::<CpuVideoBuffer>()
        .unwrap();
    assert_eq!(cpu.planes()[0].bytes().len(), 16);
}

#[test]
fn frame_and_audio_buffers_reject_ambiguous_layouts() {
    assert!(VideoPlane::new(Arc::from([0_u8; 7]), 8, 1).is_err());
    assert!(CpuVideoBuffer::new(2, 2, PixelFormat::Rgba8Unorm, vec![]).is_err());

    let format =
        AudioFormat::new(48_000, SampleFormat::F32Planar, ChannelLayout::stereo()).unwrap();
    let left = AudioPlane::new(Arc::from(vec![0_u8; 16]));
    assert!(AudioBlock::new(
        format.clone(),
        SampleTime::new(0, 48_000).unwrap(),
        4,
        vec![left.clone()],
    )
    .is_err());
    let right = AudioPlane::new(Arc::from(vec![0_u8; 16]));
    let audio = AudioBlock::new(
        format,
        SampleTime::new(96_000, 48_000).unwrap(),
        4,
        vec![left, right],
    )
    .unwrap()
    .with_metadata("track.language", MetadataValue::Text("en".into()))
    .unwrap();
    assert_eq!(audio.timestamp().sample(), 96_000);
    assert_eq!(audio.duration().value(), 4);
    assert_eq!(audio.planes().len(), 2);
}

#[derive(Debug)]
struct GpuBuffer {
    token: u64,
}

impl VideoFrameBuffer for GpuBuffer {
    fn storage_kind(&self) -> FrameStorageKind {
        FrameStorageKind::Gpu
    }

    fn width(&self) -> u32 {
        2
    }

    fn height(&self) -> u32 {
        2
    }

    fn pixel_format(&self) -> PixelFormat {
        PixelFormat::Rgba8Unorm
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[test]
fn backend_owned_frames_cross_the_interface_without_a_cpu_copy() {
    let buffer = Arc::new(GpuBuffer { token: 0xfeed });
    let video = VideoFrame::new(
        video_format(),
        RationalTime::new(0, Timebase::integer(24).unwrap()),
        Duration::new(1, Timebase::integer(24).unwrap()).unwrap(),
        buffer,
    )
    .unwrap();
    assert_eq!(video.buffer().storage_kind(), FrameStorageKind::Gpu);
    assert_eq!(
        video
            .buffer()
            .as_any()
            .downcast_ref::<GpuBuffer>()
            .unwrap()
            .token,
        0xfeed
    );
}

#[test]
fn shared_media_values_are_safe_across_thread_owners() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<Packet>();
    assert_send_sync::<VideoFrame>();
    assert_send_sync::<AudioBlock>();
    assert_send_sync::<SourceInfo>();
    assert_send_sync::<EncoderConfig>();
    assert_send_sync::<DecoderConfig>();
}

struct MemorySource {
    info: SourceInfo,
    packets: VecDeque<Packet>,
}

impl MediaSource for MemorySource {
    fn info(&self) -> &SourceInfo {
        &self.info
    }

    fn read_packet(&mut self, operation: &OperationContext) -> Result<ReadOutcome<Packet>> {
        operation.check("read_packet")?;
        Ok(match self.packets.pop_front() {
            Some(packet) => ReadOutcome::Complete(packet),
            None => ReadOutcome::EndOfStream,
        })
    }

    fn seek(&mut self, request: SeekRequest, operation: &OperationContext) -> Result<RationalTime> {
        operation.check("seek")?;
        Ok(request.target())
    }
}

#[derive(Default)]
struct MemoryDecoder {
    queued: VecDeque<Packet>,
    draining: bool,
}

impl Decoder for MemoryDecoder {
    fn config(&self) -> &DecoderConfig {
        static CONFIG: std::sync::OnceLock<DecoderConfig> = std::sync::OnceLock::new();
        CONFIG.get_or_init(|| DecoderConfig::new(video_stream()))
    }

    fn send_packet(&mut self, packet: Packet, operation: &OperationContext) -> Result<()> {
        operation.check("send_packet")?;
        self.queued.push_back(packet);
        Ok(())
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<DecodeOutput> {
        operation.check("receive_decoded")?;
        if let Some(packet) = self.queued.pop_front() {
            let timestamp = packet.timing().presentation_time().unwrap().value();
            return Ok(DecodeOutput::Frame(frame(packet.data()[0], timestamp)));
        }
        if self.draining {
            Ok(DecodeOutput::EndOfStream)
        } else {
            Ok(DecodeOutput::NeedInput)
        }
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("flush_decoder")?;
        self.draining = true;
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_decoder")?;
        self.queued.clear();
        self.draining = false;
        Ok(())
    }
}

struct MemoryEncoder {
    config: EncoderConfig,
    queued: VecDeque<Packet>,
    draining: bool,
}

impl Encoder for MemoryEncoder {
    fn config(&self) -> &EncoderConfig {
        &self.config
    }

    fn send(&mut self, input: EncodeInput, operation: &OperationContext) -> Result<()> {
        operation.check("send_encoded_input")?;
        let EncodeInput::Video(frame) = input else {
            panic!("video-only test encoder received audio")
        };
        let cpu = frame
            .buffer()
            .as_any()
            .downcast_ref::<CpuVideoBuffer>()
            .unwrap();
        self.queued.push_back(packet(
            cpu.planes()[0].bytes()[0],
            frame.timestamp().value(),
        ));
        Ok(())
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<EncodeOutput> {
        operation.check("receive_encoded")?;
        if let Some(packet) = self.queued.pop_front() {
            return Ok(EncodeOutput::Packet(packet));
        }
        if self.draining {
            Ok(EncodeOutput::EndOfStream)
        } else {
            Ok(EncodeOutput::NeedInput)
        }
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("flush_encoder")?;
        self.draining = true;
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("reset_encoder")?;
        self.queued.clear();
        self.draining = false;
        Ok(())
    }
}

struct MemoryBackend {
    descriptor: BackendDescriptor,
}

impl MemoryBackend {
    fn new() -> Self {
        Self::with_id("memory")
    }

    fn with_id(id: &str) -> Self {
        Self {
            descriptor: BackendDescriptor::new(
                BackendId::new(id).unwrap(),
                "Memory contract backend",
            )
            .unwrap(),
        }
    }
}

fn memory_capabilities() -> BackendCapabilities {
    BackendCapabilities::new([
        BackendCapability::Source,
        BackendCapability::Decode(CodecId::new("av1").unwrap()),
        BackendCapability::Encode(CodecId::new("av1").unwrap()),
    ])
}

#[test]
fn registry_ranks_capable_backends_and_only_exposes_explicit_fallbacks() {
    let primary = Arc::new(MemoryBackend::with_id("primary"));
    let lower_ranked_primary = Arc::new(MemoryBackend::with_id("lower-ranked-primary"));
    let tied_primary = Arc::new(MemoryBackend::with_id("z-tied-primary"));
    let fallback = Arc::new(MemoryBackend::with_id("fallback"));
    let unavailable_codec = Arc::new(MemoryBackend::with_id("vp9-only"));
    let platform_only = Arc::new(MemoryBackend::with_id("platform-only"));
    let mut registry = BackendRegistry::new();

    registry
        .register(
            BackendRegistration::new(primary, memory_capabilities(), 10, BackendTier::Primary)
                .unwrap(),
        )
        .unwrap();
    registry
        .register(
            BackendRegistration::new(
                tied_primary,
                memory_capabilities(),
                10,
                BackendTier::Primary,
            )
            .unwrap(),
        )
        .unwrap();
    registry
        .register(
            BackendRegistration::new(
                lower_ranked_primary,
                memory_capabilities(),
                9,
                BackendTier::Primary,
            )
            .unwrap(),
        )
        .unwrap();
    registry
        .register(
            BackendRegistration::new(
                platform_only,
                BackendCapabilities::new([BackendCapability::Decode(
                    CodecId::new("h264").unwrap(),
                )]),
                50,
                BackendTier::Fallback,
            )
            .unwrap(),
        )
        .unwrap();
    registry
        .register(
            BackendRegistration::new(fallback, memory_capabilities(), 100, BackendTier::Fallback)
                .unwrap(),
        )
        .unwrap();
    registry
        .register(
            BackendRegistration::new(
                unavailable_codec,
                BackendCapabilities::new([BackendCapability::Decode(CodecId::new("vp9").unwrap())]),
                1,
                BackendTier::Primary,
            )
            .unwrap(),
        )
        .unwrap();

    let decode_av1 = BackendRequirement::decode(CodecId::new("av1").unwrap());
    let no_fallback = registry
        .select(&decode_av1, FallbackPolicy::Disallow)
        .unwrap();
    assert_eq!(no_fallback.primary().descriptor().id().as_str(), "primary");
    assert!(no_fallback.fallbacks().is_empty());
    assert!(!no_fallback.fallback_used());

    let source_selection = registry
        .select(&BackendRequirement::source(), FallbackPolicy::Disallow)
        .unwrap();
    assert_eq!(
        source_selection.primary().descriptor().id().as_str(),
        "primary"
    );
    let relink = SourceRequest::new(
        MediaId::from_raw(48),
        SourceLocation::Path(PathBuf::from("relinked.mov")),
    )
    .with_expected_fingerprint("sha256:test")
    .unwrap();
    let operation = OperationContext::new(MediaPriority::Interactive);
    let source = source_selection
        .primary()
        .open_source(&relink, &operation)
        .unwrap();
    assert_eq!(source.info().identity().media_id(), relink.media_id());
    assert_eq!(source.info().identity().fingerprint(), "sha256:test");

    let with_fallback = registry
        .select(&decode_av1, FallbackPolicy::AllowRegistered)
        .unwrap();
    assert_eq!(
        with_fallback.primary().descriptor().id().as_str(),
        "primary"
    );
    assert_eq!(
        with_fallback
            .fallbacks()
            .iter()
            .map(|backend| backend.descriptor().id().as_str())
            .collect::<Vec<_>>(),
        ["fallback"]
    );
    assert!(!with_fallback.fallback_used());

    assert!(registry
        .select(
            &BackendRequirement::decode(CodecId::new("h264").unwrap()),
            FallbackPolicy::Disallow,
        )
        .is_err());
    let platform_fallback = registry
        .select(
            &BackendRequirement::decode(CodecId::new("h264").unwrap()),
            FallbackPolicy::AllowRegistered,
        )
        .unwrap();
    assert_eq!(
        platform_fallback.primary().descriptor().id().as_str(),
        "platform-only"
    );
    assert!(platform_fallback.fallback_used());
    assert!(platform_fallback.fallbacks().is_empty());

    let unsupported = registry.select(
        &BackendRequirement::encode(CodecId::new("vp9").unwrap()),
        FallbackPolicy::AllowRegistered,
    );
    let Err(error) = unsupported else {
        panic!("a vp9 encoder must not be selected from av1-only registrations")
    };
    assert_eq!(error.category(), ErrorCategory::Unsupported);
    assert_eq!(
        error.recoverability(),
        superi_core::error::Recoverability::Degraded
    );
    assert_eq!(error.contexts()[0].field("operation"), Some("encode"));
    assert_eq!(error.contexts()[0].field("codec"), Some("vp9"));
}

impl MediaBackend for MemoryBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        _probe: &SourceProbe<'_>,
        operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        operation.check("probe_source")?;
        Ok(SourceProbeResult::NoMatch)
    }

    fn open_source(
        &self,
        request: &SourceRequest,
        operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        operation.check("open_source")?;
        let identity = SourceIdentity::new(request.media_id(), "sha256:test").unwrap();
        let info = SourceInfo::new(identity, vec![video_stream()])
            .unwrap()
            .with_metadata("container", MetadataValue::Text("memory".into()))
            .unwrap();
        Ok(Box::new(MemorySource {
            info,
            packets: VecDeque::from([packet(11, 0), packet(22, 1)]),
        }))
    }

    fn create_decoder(
        &self,
        _config: &DecoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>> {
        operation.check("create_decoder")?;
        Ok(Box::new(MemoryDecoder::default()))
    }

    fn create_encoder(
        &self,
        config: &EncoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        operation.check("create_encoder")?;
        Ok(Box::new(MemoryEncoder {
            config: config.clone(),
            queued: VecDeque::new(),
            draining: false,
        }))
    }
}

#[test]
fn public_backend_contract_drives_ingest_seek_playback_relink_and_export() {
    let backend = MemoryBackend::new();
    let media_id = MediaId::from_raw(99);
    let first_request = SourceRequest::new(
        media_id,
        SourceLocation::Path(PathBuf::from("original.mov")),
    );
    let relink_request = SourceRequest::new(
        media_id,
        SourceLocation::Path(PathBuf::from("relinked.mov")),
    );
    assert_eq!(first_request.media_id(), relink_request.media_id());

    let operation = OperationContext::new(MediaPriority::Interactive);
    let mut source = backend.open_source(&relink_request, &operation).unwrap();
    assert_eq!(source.info().identity().media_id(), media_id);
    assert_eq!(source.info().streams()[0].kind(), StreamKind::Video);
    let actual = source
        .seek(
            SeekRequest::new(
                RationalTime::new(0, Timebase::integer(24).unwrap()),
                SeekMode::NearestKeyframe,
            ),
            &operation,
        )
        .unwrap();
    assert_eq!(actual.value(), 0);

    let decoder_config = DecoderConfig::new(source.info().streams()[0].clone());
    let encoder_config = EncoderConfig::video(
        StreamId::new(8),
        CodecId::new("av1").unwrap(),
        Timebase::integer(24).unwrap(),
        video_format(),
    );
    let mut decoder = backend.create_decoder(&decoder_config, &operation).unwrap();
    let mut encoder = backend.create_encoder(&encoder_config, &operation).unwrap();

    let mut exported = Vec::new();
    loop {
        let packet = match source.read_packet(&operation).unwrap() {
            ReadOutcome::Complete(packet) => packet,
            ReadOutcome::Partial { .. } => panic!("test source returned a partial packet"),
            ReadOutcome::EndOfStream => break,
            _ => panic!("test source returned an unknown read outcome"),
        };
        decoder.send_packet(packet, &operation).unwrap();
        let DecodeOutput::Frame(frame) = decoder.receive(&operation).unwrap() else {
            panic!("decoder did not produce a frame")
        };
        encoder.send(EncodeInput::Video(frame), &operation).unwrap();
        let EncodeOutput::Packet(packet) = encoder.receive(&operation).unwrap() else {
            panic!("encoder did not produce a packet")
        };
        exported.push(packet.data()[0]);
    }
    decoder.flush(&operation).unwrap();
    encoder.flush(&operation).unwrap();
    assert!(matches!(
        decoder.receive(&operation).unwrap(),
        DecodeOutput::EndOfStream
    ));
    assert!(matches!(
        encoder.receive(&operation).unwrap(),
        EncodeOutput::EndOfStream
    ));
    assert_eq!(exported, [11, 22]);
    decoder.reset(&operation).unwrap();
    encoder.reset(&operation).unwrap();
    assert!(matches!(
        decoder.receive(&operation).unwrap(),
        DecodeOutput::NeedInput
    ));
    assert!(matches!(
        encoder.receive(&operation).unwrap(),
        EncodeOutput::NeedInput
    ));
}

#[test]
fn source_info_rejects_duplicate_streams_and_invalid_metadata_keys() {
    let identity = SourceIdentity::new(MediaId::from_raw(1), "sha256:one").unwrap();
    assert!(SourceInfo::new(identity.clone(), vec![video_stream(), video_stream()]).is_err());
    let mut metadata = MediaMetadata::new();
    assert!(metadata
        .insert("Invalid Key", MetadataValue::Text("x".into()))
        .is_err());
    assert!(SourceInfo::new(identity, vec![]).is_err());
}

#[test]
fn public_media_operations_propagate_cancellation_and_timeout() {
    let backend = MemoryBackend::new();
    let request = SourceRequest::new(
        MediaId::from_raw(101),
        SourceLocation::Path(PathBuf::from("interrupted.mov")),
    );
    let cancelled = OperationContext::new(MediaPriority::Interactive);
    cancelled.cancellation_token().cancel();
    let error = match backend.open_source(&request, &cancelled) {
        Ok(_) => panic!("a cancelled source open must not begin"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::Cancelled);

    let active = OperationContext::new(MediaPriority::Playback);
    let mut source = backend.open_source(&request, &active).unwrap();
    let mut decoder = backend
        .create_decoder(&DecoderConfig::new(video_stream()), &active)
        .unwrap();
    let mut encoder = backend
        .create_encoder(
            &EncoderConfig::video(
                StreamId::new(9),
                CodecId::new("av1").unwrap(),
                Timebase::integer(24).unwrap(),
                video_format(),
            ),
            &active,
        )
        .unwrap();
    active.cancellation_token().cancel();
    assert_eq!(
        source.read_packet(&active).unwrap_err().category(),
        ErrorCategory::Cancelled
    );
    assert_eq!(
        decoder.receive(&active).unwrap_err().category(),
        ErrorCategory::Cancelled
    );
    assert_eq!(
        encoder.receive(&active).unwrap_err().category(),
        ErrorCategory::Cancelled
    );

    let expired =
        OperationContext::new(MediaPriority::Export).with_deadline(std::time::Instant::now());
    let error = match backend.create_decoder(&DecoderConfig::new(video_stream()), &expired) {
        Ok(_) => panic!("an expired decoder creation must not begin"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::Timeout);
}
