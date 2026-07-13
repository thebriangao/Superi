use std::any::Any;
use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;

use superi_core::color_space::ColorSpace;
use superi_core::error::Result;
use superi_core::ids::MediaId;
use superi_core::pixel::{AlphaMode, ChannelLayout, PixelFormat, SampleFormat};
use superi_core::time::{Duration, RationalTime, SampleTime, Timebase};
use superi_media_io::audio_io::{AudioBlock, AudioFormat, AudioPlane};
use superi_media_io::backend::{BackendDescriptor, MediaBackend};
use superi_media_io::decode::{
    CpuVideoBuffer, DecodeOutput, Decoder, DecoderConfig, FrameStorageKind, VideoFormat,
    VideoFrame, VideoFrameBuffer, VideoPlane,
};
use superi_media_io::demux::{
    BackendId, CodecId, MediaMetadata, MediaSource, MetadataValue, Packet, PacketTiming, SeekMode,
    SeekRequest, SourceIdentity, SourceInfo, SourceLocation, SourceRequest, StreamId, StreamInfo,
    StreamKind,
};
use superi_media_io::encode::{EncodeInput, EncodeOutput, Encoder, EncoderConfig};

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

    fn read_packet(&mut self) -> Result<Option<Packet>> {
        Ok(self.packets.pop_front())
    }

    fn seek(&mut self, request: SeekRequest) -> Result<RationalTime> {
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

    fn send_packet(&mut self, packet: Packet) -> Result<()> {
        self.queued.push_back(packet);
        Ok(())
    }

    fn receive(&mut self) -> Result<DecodeOutput> {
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

    fn flush(&mut self) -> Result<()> {
        self.draining = true;
        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
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

    fn send(&mut self, input: EncodeInput) -> Result<()> {
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

    fn receive(&mut self) -> Result<EncodeOutput> {
        if let Some(packet) = self.queued.pop_front() {
            return Ok(EncodeOutput::Packet(packet));
        }
        if self.draining {
            Ok(EncodeOutput::EndOfStream)
        } else {
            Ok(EncodeOutput::NeedInput)
        }
    }

    fn flush(&mut self) -> Result<()> {
        self.draining = true;
        Ok(())
    }

    fn reset(&mut self) -> Result<()> {
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
        Self {
            descriptor: BackendDescriptor::new(
                BackendId::new("memory").unwrap(),
                "Memory contract backend",
            )
            .unwrap(),
        }
    }
}

impl MediaBackend for MemoryBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn open_source(&self, request: &SourceRequest) -> Result<Box<dyn MediaSource>> {
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

    fn create_decoder(&self, _config: &DecoderConfig) -> Result<Box<dyn Decoder>> {
        Ok(Box::new(MemoryDecoder::default()))
    }

    fn create_encoder(&self, config: &EncoderConfig) -> Result<Box<dyn Encoder>> {
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

    let mut source = backend.open_source(&relink_request).unwrap();
    assert_eq!(source.info().identity().media_id(), media_id);
    assert_eq!(source.info().streams()[0].kind(), StreamKind::Video);
    let actual = source
        .seek(SeekRequest::new(
            RationalTime::new(0, Timebase::integer(24).unwrap()),
            SeekMode::NearestKeyframe,
        ))
        .unwrap();
    assert_eq!(actual.value(), 0);

    let decoder_config = DecoderConfig::new(source.info().streams()[0].clone());
    let encoder_config = EncoderConfig::video(
        StreamId::new(8),
        CodecId::new("av1").unwrap(),
        Timebase::integer(24).unwrap(),
        video_format(),
    );
    let mut decoder = backend.create_decoder(&decoder_config).unwrap();
    let mut encoder = backend.create_encoder(&encoder_config).unwrap();

    let mut exported = Vec::new();
    while let Some(packet) = source.read_packet().unwrap() {
        decoder.send_packet(packet).unwrap();
        let DecodeOutput::Frame(frame) = decoder.receive().unwrap() else {
            panic!("decoder did not produce a frame")
        };
        encoder.send(EncodeInput::Video(frame)).unwrap();
        let EncodeOutput::Packet(packet) = encoder.receive().unwrap() else {
            panic!("encoder did not produce a packet")
        };
        exported.push(packet.data()[0]);
    }
    decoder.flush().unwrap();
    encoder.flush().unwrap();
    assert!(matches!(
        decoder.receive().unwrap(),
        DecodeOutput::EndOfStream
    ));
    assert!(matches!(
        encoder.receive().unwrap(),
        EncodeOutput::EndOfStream
    ));
    assert_eq!(exported, [11, 22]);
    decoder.reset().unwrap();
    encoder.reset().unwrap();
    assert!(matches!(
        decoder.receive().unwrap(),
        DecodeOutput::NeedInput
    ));
    assert!(matches!(
        encoder.receive().unwrap(),
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
