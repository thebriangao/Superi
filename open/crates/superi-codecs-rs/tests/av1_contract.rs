use std::sync::{Arc, OnceLock};

use superi_codecs_rs::register::register_default_backends;
use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::ErrorCategory;
use superi_core::ids::MediaId;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::time::{Duration, RationalTime, Timebase};
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendRegistration, BackendRegistry,
    BackendRequirement, BackendTier, FallbackPolicy, MediaBackend,
};
use superi_media_io::decode::{
    CpuVideoBuffer, DecodeOutput, DecoderConfig, VideoFormat, VideoFrame, VideoPlane,
};
use superi_media_io::demux::{
    CodecId, MetadataValue, Packet, PacketTiming, SeekMode, SeekRequest, SourceLocation,
    SourceProbeLimits, SourceRequest, StreamId, StreamInfo, StreamKind,
};
use superi_media_io::encode::{EncodeInput, EncodeOutput, Encoder, EncoderConfig};
use superi_media_io::mkv_webm::MkvWebmBackend;
use superi_media_io::operation::{CancellationToken, MediaPriority, OperationContext};
use superi_media_io::read::ReadOutcome;

const STREAM_ID: StreamId = StreamId::new(20);
const WIDTH: u32 = 16;
const HEIGHT: u32 = 16;

fn operation() -> &'static OperationContext {
    static OPERATION: OnceLock<OperationContext> = OnceLock::new();
    OPERATION.get_or_init(|| OperationContext::new(MediaPriority::Export))
}

fn av1() -> CodecId {
    CodecId::new("av1").unwrap()
}

fn registry() -> BackendRegistry {
    let mut registry = BackendRegistry::new();
    register_default_backends(&mut registry).unwrap();
    registry
}

fn backend(requirement: BackendRequirement) -> Arc<dyn MediaBackend> {
    registry()
        .select(&requirement, FallbackPolicy::Disallow)
        .unwrap()
        .primary()
        .clone()
}

fn video_stream(timebase: Timebase) -> StreamInfo {
    StreamInfo::new(STREAM_ID, StreamKind::Video, av1(), timebase)
}

fn plane_u8(value: u8, stride: usize, rows: u32) -> VideoPlane {
    VideoPlane::new(Arc::from(vec![value; stride * rows as usize]), stride, rows).unwrap()
}

fn plane_u16(value: u16, width: usize, rows: u32) -> VideoPlane {
    let bytes = (0..width * rows as usize)
        .flat_map(|_| value.to_le_bytes())
        .collect::<Vec<_>>();
    VideoPlane::new(Arc::from(bytes), width * 2, rows).unwrap()
}

fn yuv420_frame(timestamp: i64, duration: u64, luma: u8) -> VideoFrame {
    let timebase = Timebase::integer(24).unwrap();
    let format = VideoFormat::new(
        WIDTH,
        HEIGHT,
        PixelFormat::Yuv420p8,
        ColorSpace::BT709,
        AlphaMode::Opaque,
    )
    .unwrap();
    let buffer = CpuVideoBuffer::new(
        WIDTH,
        HEIGHT,
        PixelFormat::Yuv420p8,
        vec![
            plane_u8(luma, WIDTH as usize, HEIGHT),
            plane_u8(96, WIDTH.div_ceil(2) as usize, HEIGHT.div_ceil(2)),
            plane_u8(160, WIDTH.div_ceil(2) as usize, HEIGHT.div_ceil(2)),
        ],
    )
    .unwrap();
    VideoFrame::new(
        format,
        RationalTime::new(timestamp, timebase),
        Duration::new(duration, timebase).unwrap(),
        Arc::new(buffer),
    )
    .unwrap()
    .with_metadata(
        "source.marker",
        MetadataValue::Text(format!("frame-{timestamp}")),
    )
    .unwrap()
}

fn yuv444p10_frame() -> VideoFrame {
    yuv444p10_frame_with_color(ColorSpace::BT2100_PQ)
}

fn yuv444p10_frame_with_color(color_space: ColorSpace) -> VideoFrame {
    let timebase = Timebase::new(30_000, 1_001).unwrap();
    let format = VideoFormat::new(
        WIDTH,
        HEIGHT,
        PixelFormat::Yuv444p10,
        color_space,
        AlphaMode::Opaque,
    )
    .unwrap();
    let buffer = CpuVideoBuffer::new(
        WIDTH,
        HEIGHT,
        PixelFormat::Yuv444p10,
        vec![
            plane_u16(700, WIDTH as usize, HEIGHT),
            plane_u16(400, WIDTH as usize, HEIGHT),
            plane_u16(600, WIDTH as usize, HEIGHT),
        ],
    )
    .unwrap();
    VideoFrame::new(
        format,
        RationalTime::new(1_001, timebase),
        Duration::new(1_001, timebase).unwrap(),
        Arc::new(buffer),
    )
    .unwrap()
}

fn supported_format_frame(pixel_format: PixelFormat) -> VideoFrame {
    let timebase = Timebase::integer(30).unwrap();
    let format = VideoFormat::new(
        WIDTH,
        HEIGHT,
        pixel_format,
        ColorSpace::BT709,
        AlphaMode::Opaque,
    )
    .unwrap();
    let half_width = WIDTH.div_ceil(2) as usize;
    let half_height = HEIGHT.div_ceil(2);
    let planes = match pixel_format {
        PixelFormat::R8Unorm => vec![plane_u8(100, WIDTH as usize, HEIGHT)],
        PixelFormat::Yuv420p8 => vec![
            plane_u8(100, WIDTH as usize, HEIGHT),
            plane_u8(90, half_width, half_height),
            plane_u8(170, half_width, half_height),
        ],
        PixelFormat::Yuv422p8 => vec![
            plane_u8(100, WIDTH as usize, HEIGHT),
            plane_u8(90, half_width, HEIGHT),
            plane_u8(170, half_width, HEIGHT),
        ],
        PixelFormat::Yuv444p8 => vec![
            plane_u8(100, WIDTH as usize, HEIGHT),
            plane_u8(90, WIDTH as usize, HEIGHT),
            plane_u8(170, WIDTH as usize, HEIGHT),
        ],
        PixelFormat::Nv12 => vec![
            plane_u8(100, WIDTH as usize, HEIGHT),
            plane_u8(128, WIDTH as usize, half_height),
        ],
        PixelFormat::Yuv420p10 => vec![
            plane_u16(500, WIDTH as usize, HEIGHT),
            plane_u16(400, half_width, half_height),
            plane_u16(600, half_width, half_height),
        ],
        PixelFormat::Yuv422p10 => vec![
            plane_u16(500, WIDTH as usize, HEIGHT),
            plane_u16(400, half_width, HEIGHT),
            plane_u16(600, half_width, HEIGHT),
        ],
        PixelFormat::Yuv444p10 => vec![
            plane_u16(500, WIDTH as usize, HEIGHT),
            plane_u16(400, WIDTH as usize, HEIGHT),
            plane_u16(600, WIDTH as usize, HEIGHT),
        ],
        PixelFormat::P010 => vec![
            plane_u16(500, WIDTH as usize, HEIGHT),
            plane_u16(512, WIDTH as usize, half_height),
        ],
        _ => panic!("unsupported AV1 contract fixture format"),
    };
    let buffer = CpuVideoBuffer::new(WIDTH, HEIGHT, pixel_format, planes).unwrap();
    VideoFrame::new(
        format,
        RationalTime::new(4, timebase),
        Duration::new(1, timebase).unwrap(),
        Arc::new(buffer),
    )
    .unwrap()
}

fn ebml_size(value: usize) -> Vec<u8> {
    let value = u64::try_from(value).unwrap();
    for width in 1..=8 {
        let bits = width * 7;
        if value < (1_u64 << bits) - 1 {
            let mut encoded = value.to_be_bytes()[8 - width..].to_vec();
            encoded[0] |= 1 << (8 - width);
            return encoded;
        }
    }
    panic!("AV1 fixture element is too large");
}

fn ebml_element(id: &[u8], payload: impl AsRef<[u8]>) -> Vec<u8> {
    let payload = payload.as_ref();
    let mut bytes = Vec::with_capacity(id.len() + 8 + payload.len());
    bytes.extend_from_slice(id);
    bytes.extend(ebml_size(payload.len()));
    bytes.extend_from_slice(payload);
    bytes
}

fn ebml_unsigned(id: &[u8], value: u64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let first = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len() - 1);
    ebml_element(id, &bytes[first..])
}

fn ebml_float(id: &[u8], value: f64) -> Vec<u8> {
    ebml_element(id, value.to_be_bytes())
}

fn ebml_text(id: &[u8], value: &str) -> Vec<u8> {
    ebml_element(id, value.as_bytes())
}

fn ebml_data_vint(value: u64) -> Vec<u8> {
    for width in 1..=8 {
        let bits = width * 7;
        if value < (1_u64 << bits) - 1 {
            let mut encoded = value.to_be_bytes()[8 - width..].to_vec();
            encoded[0] |= 1 << (8 - width);
            return encoded;
        }
    }
    panic!("AV1 fixture VINT is too large");
}

fn av1_mkv_fixture(packet: &Packet) -> Vec<u8> {
    let mut ebml = Vec::new();
    ebml.extend(ebml_unsigned(&[0x42, 0x86], 1));
    ebml.extend(ebml_unsigned(&[0x42, 0xF7], 1));
    ebml.extend(ebml_unsigned(&[0x42, 0xF2], 4));
    ebml.extend(ebml_unsigned(&[0x42, 0xF3], 8));
    ebml.extend(ebml_text(&[0x42, 0x82], "matroska"));
    ebml.extend(ebml_unsigned(&[0x42, 0x87], 4));
    ebml.extend(ebml_unsigned(&[0x42, 0x85], 2));

    let mut info = ebml_unsigned(&[0x2A, 0xD7, 0xB1], 1_000_000);
    info.extend(ebml_float(&[0x44, 0x89], 2_000.0));

    let mut video = ebml_unsigned(&[0xB0], WIDTH.into());
    video.extend(ebml_unsigned(&[0xBA], HEIGHT.into()));
    let mut track = ebml_unsigned(&[0xD7], 1);
    track.extend(ebml_unsigned(&[0x73, 0xC5], 20));
    track.extend(ebml_unsigned(&[0x83], 1));
    track.extend(ebml_unsigned(&[0x23, 0xE3, 0x83], 41_666_667));
    track.extend(ebml_text(&[0x86], "V_AV1"));
    if let Some(MetadataValue::Bytes(configuration)) = packet.metadata().get("codec.configuration")
    {
        track.extend(ebml_element(&[0x63, 0xA2], configuration));
    }
    track.extend(ebml_element(&[0xE0], video));

    let simple_block = |relative_time: i16| {
        let mut payload = ebml_data_vint(1);
        payload.extend_from_slice(&relative_time.to_be_bytes());
        payload.push(0x80);
        payload.extend_from_slice(packet.data());
        ebml_element(&[0xA3], payload)
    };
    let mut cluster = ebml_unsigned(&[0xE7], 0);
    cluster.extend(simple_block(0));
    cluster.extend(simple_block(1_000));

    let mut segment = ebml_element(&[0x15, 0x49, 0xA9, 0x66], info);
    segment.extend(ebml_element(
        &[0x16, 0x54, 0xAE, 0x6B],
        ebml_element(&[0xAE], track),
    ));
    segment.extend(ebml_element(&[0x1F, 0x43, 0xB6, 0x75], cluster));

    let mut bytes = ebml_element(&[0x1A, 0x45, 0xDF, 0xA3], ebml);
    bytes.extend(ebml_element(&[0x18, 0x53, 0x80, 0x67], segment));
    bytes
}

fn mkv_registry() -> BackendRegistry {
    let mut registry = BackendRegistry::new();
    registry
        .register(
            BackendRegistration::new(
                Arc::new(MkvWebmBackend::new().unwrap()),
                BackendCapabilities::new([BackendCapability::Source]),
                100,
                BackendTier::Primary,
            )
            .unwrap(),
        )
        .unwrap();
    registry
}

fn mkv_request(bytes: Vec<u8>) -> SourceRequest {
    SourceRequest::new(
        MediaId::from_raw(20),
        SourceLocation::Memory {
            name: "av1.mkv".into(),
            data: Arc::from(bytes),
        },
    )
}

fn drain_encoder(encoder: &mut dyn Encoder) -> (Vec<Packet>, bool) {
    let mut packets = Vec::new();
    loop {
        match encoder.receive(operation()).unwrap() {
            EncodeOutput::Packet(packet) => packets.push(packet),
            EncodeOutput::NeedInput => return (packets, false),
            EncodeOutput::EndOfStream => return (packets, true),
            _ => panic!("unknown encoder output"),
        }
    }
}

fn encode(format: VideoFormat, timebase: Timebase, frames: &[VideoFrame]) -> Vec<Packet> {
    let backend = backend(BackendRequirement::encode(av1()));
    let config = EncoderConfig::video(STREAM_ID, av1(), timebase, format);
    let mut encoder = backend.create_encoder(&config, operation()).unwrap();
    let mut packets = Vec::new();
    for frame in frames {
        encoder
            .send(EncodeInput::Video(frame.clone()), operation())
            .unwrap();
        let (ready, ended) = drain_encoder(encoder.as_mut());
        assert!(!ended);
        packets.extend(ready);
    }
    encoder.flush(operation()).unwrap();
    let (ready, ended) = drain_encoder(encoder.as_mut());
    assert!(ended);
    packets.extend(ready);
    packets
}

fn decode(timebase: Timebase, packets: &[Packet]) -> Vec<VideoFrame> {
    let backend = backend(BackendRequirement::decode(av1()));
    let mut decoder = backend
        .create_decoder(&DecoderConfig::new(video_stream(timebase)), operation())
        .unwrap();
    let mut frames = Vec::new();
    for packet in packets {
        decoder.send_packet(packet.clone(), operation()).unwrap();
        loop {
            match decoder.receive(operation()).unwrap() {
                DecodeOutput::Frame(frame) => frames.push(frame),
                DecodeOutput::NeedInput => break,
                DecodeOutput::Audio(_) => panic!("AV1 decoder returned audio"),
                DecodeOutput::EndOfStream => panic!("AV1 decoder ended before flush"),
                _ => panic!("unknown decoder output"),
            }
        }
    }
    decoder.flush(operation()).unwrap();
    loop {
        match decoder.receive(operation()).unwrap() {
            DecodeOutput::Frame(frame) => frames.push(frame),
            DecodeOutput::EndOfStream => break,
            DecodeOutput::NeedInput => panic!("flushed AV1 decoder requested input"),
            DecodeOutput::Audio(_) => panic!("AV1 decoder returned audio"),
            _ => panic!("unknown decoder output"),
        }
    }
    frames
}

#[test]
fn default_registry_exposes_primary_av1_decode_and_encode() {
    let registry = registry();

    for requirement in [
        BackendRequirement::decode(av1()),
        BackendRequirement::encode(av1()),
    ] {
        let selection = registry
            .select(&requirement, FallbackPolicy::Disallow)
            .unwrap();
        assert_eq!(selection.primary().descriptor().id().as_str(), "rust-av1");
        assert!(!selection.fallback_used());
        assert!(selection.fallbacks().is_empty());
    }
}

#[test]
fn deterministic_round_trip_preserves_timing_metadata_color_and_visible_frames() {
    let frames = [yuv420_frame(-2, 1, 40), yuv420_frame(-1, 3, 210)];
    let format = frames[0].format();
    let timebase = frames[0].timestamp().timebase();
    let packets = encode(format, timebase, &frames);
    assert_eq!(packets, encode(format, timebase, &frames));
    assert_eq!(packets.len(), frames.len());
    assert!(packets[0].is_keyframe());
    for (packet, frame) in packets.iter().zip(&frames) {
        assert_eq!(packet.timing().presentation_time(), Some(frame.timestamp()));
        assert_eq!(packet.timing().duration(), Some(frame.duration()));
        assert_eq!(
            packet.metadata().get("source.marker"),
            frame.metadata().get("source.marker")
        );
    }

    let decoded = decode(timebase, &packets);
    assert_eq!(decoded.len(), frames.len());
    for (actual, expected) in decoded.iter().zip(&frames) {
        assert_eq!(actual.timestamp(), expected.timestamp());
        assert_eq!(actual.duration(), expected.duration());
        assert_eq!(actual.format(), expected.format());
        assert_eq!(
            actual.metadata().get("source.marker"),
            expected.metadata().get("source.marker")
        );
    }

    let luma_means = decoded
        .iter()
        .map(|frame| {
            let cpu = frame
                .buffer()
                .as_any()
                .downcast_ref::<CpuVideoBuffer>()
                .unwrap();
            let bytes = cpu.planes()[0].bytes();
            bytes.iter().map(|value| u64::from(*value)).sum::<u64>() / bytes.len() as u64
        })
        .collect::<Vec<_>>();
    assert!(luma_means[0] < luma_means[1]);
}

#[test]
fn ten_bit_yuv444_and_hdr_color_signaling_survive_round_trip() {
    let frame = yuv444p10_frame();
    let packets = encode(
        frame.format(),
        frame.timestamp().timebase(),
        std::slice::from_ref(&frame),
    );
    let decoded = decode(frame.timestamp().timebase(), &packets);

    assert_eq!(decoded.len(), 1);
    assert_eq!(decoded[0].format(), frame.format());
    assert_eq!(decoded[0].format().pixel_format().bits_per_component(), 10);
    let cpu = decoded[0]
        .buffer()
        .as_any()
        .downcast_ref::<CpuVideoBuffer>()
        .unwrap();
    let first_luma = u16::from_le_bytes(cpu.planes()[0].bytes()[..2].try_into().unwrap());
    assert!(first_luma > 500);
    assert_eq!(
        decoded[0].metadata().get("codec.av1.bit-depth"),
        Some(&MetadataValue::Unsigned(10))
    );

    let constant_luminance = ColorSpace::new(
        ColorPrimaries::Bt2020,
        TransferFunction::Pq,
        MatrixCoefficients::Bt2020Constant,
        ColorRange::Limited,
    );
    let frame = yuv444p10_frame_with_color(constant_luminance);
    let packets = encode(
        frame.format(),
        frame.timestamp().timebase(),
        std::slice::from_ref(&frame),
    );
    let decoded = decode(frame.timestamp().timebase(), &packets);
    assert_eq!(decoded[0].format().color_space(), constant_luminance);
}

#[test]
fn every_advertised_cpu_pixel_format_encodes_and_decodes_predictably() {
    for pixel_format in [
        PixelFormat::R8Unorm,
        PixelFormat::Yuv420p8,
        PixelFormat::Yuv422p8,
        PixelFormat::Yuv444p8,
        PixelFormat::Nv12,
        PixelFormat::Yuv420p10,
        PixelFormat::Yuv422p10,
        PixelFormat::Yuv444p10,
        PixelFormat::P010,
    ] {
        let frame = supported_format_frame(pixel_format);
        let timebase = frame.timestamp().timebase();
        let packets = encode(frame.format(), timebase, std::slice::from_ref(&frame));
        assert_eq!(packets.len(), 1, "packet count for {pixel_format:?}");
        assert!(matches!(
            packets[0].metadata().get("codec.configuration"),
            Some(MetadataValue::Bytes(bytes)) if !bytes.is_empty()
        ));

        let decoded = decode(timebase, &packets);
        assert_eq!(decoded.len(), 1, "frame count for {pixel_format:?}");
        let expected = match pixel_format {
            PixelFormat::Nv12 => PixelFormat::Yuv420p8,
            PixelFormat::P010 => PixelFormat::Yuv420p10,
            _ => pixel_format,
        };
        assert_eq!(
            decoded[0].format().pixel_format(),
            expected,
            "decoded format for {pixel_format:?}"
        );
        assert_eq!(decoded[0].timestamp(), frame.timestamp());
        assert_eq!(decoded[0].duration(), frame.duration());
    }
}

#[test]
fn matroska_ingest_seek_relink_playback_and_export_use_the_real_av1_backend() {
    let frame = yuv420_frame(0, 1, 120);
    let encoded = encode(
        frame.format(),
        frame.timestamp().timebase(),
        std::slice::from_ref(&frame),
    );
    let bytes = av1_mkv_fixture(&encoded[0]);
    let registry = mkv_registry();
    let selection = registry
        .probe_source(
            mkv_request(bytes.clone()),
            SourceProbeLimits::new(4, 4_096).unwrap(),
            FallbackPolicy::Disallow,
            operation(),
        )
        .unwrap();
    assert_eq!(selection.primary().container().as_str(), "mkv");
    let first = selection.open(operation()).unwrap();
    let fingerprint = first.info().identity().fingerprint().to_owned();
    assert_eq!(first.info().streams()[0].codec(), &av1());

    let relink = mkv_request(bytes.clone())
        .with_expected_fingerprint(fingerprint)
        .unwrap();
    let selection = registry
        .probe_source(
            relink,
            SourceProbeLimits::default(),
            FallbackPolicy::Disallow,
            operation(),
        )
        .unwrap();
    let mut source = selection.open(operation()).unwrap();
    let target = RationalTime::new(1_000_000_000, Timebase::NANOSECONDS);
    assert_eq!(
        source
            .seek(SeekRequest::new(target, SeekMode::Exact), operation())
            .unwrap(),
        target
    );
    let packet = match source.read_packet(operation()).unwrap() {
        ReadOutcome::Complete(packet) => packet,
        _ => panic!("Matroska source did not return the sought AV1 packet"),
    };
    assert_eq!(packet.timing().presentation_time(), Some(target));
    assert_eq!(packet.timing().duration().unwrap().value(), 41_666_667);

    let backend = backend(BackendRequirement::decode(av1()));
    let mut decoder = backend
        .create_decoder(
            &DecoderConfig::new(source.info().streams()[0].clone()),
            operation(),
        )
        .unwrap();
    decoder.send_packet(packet, operation()).unwrap();
    let DecodeOutput::Frame(decoded) = decoder.receive(operation()).unwrap() else {
        panic!("Matroska AV1 playback did not return a frame");
    };
    assert_eq!(decoded.timestamp(), target);
    assert_eq!(decoded.format(), frame.format());

    let bad_relink = mkv_request(bytes)
        .with_expected_fingerprint("sha256:not-the-source")
        .unwrap();
    let error = registry
        .probe_source(
            bad_relink,
            SourceProbeLimits::default(),
            FallbackPolicy::Disallow,
            operation(),
        )
        .unwrap()
        .open(operation())
        .err()
        .expect("mismatched Matroska relink should fail");
    assert_eq!(error.category(), ErrorCategory::Conflict);
}

#[test]
fn reset_replays_after_seek_and_errors_are_explicit() {
    let frame = yuv420_frame(8, 2, 120);
    let packets = encode(
        frame.format(),
        frame.timestamp().timebase(),
        std::slice::from_ref(&frame),
    );
    let backend = backend(BackendRequirement::decode(av1()));
    let config = DecoderConfig::new(video_stream(frame.timestamp().timebase()));
    let mut decoder = backend.create_decoder(&config, operation()).unwrap();

    let wrong_stream = Packet::new(
        StreamId::new(99),
        Arc::from(packets[0].data()),
        packets[0].timing(),
    );
    let error = decoder.send_packet(wrong_stream, operation()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);

    let malformed = Packet::new(
        STREAM_ID,
        Arc::from(&b"not-an-av1-temporal-unit"[..]),
        PacketTiming::new(frame.timestamp().timebase(), Some(8), Some(8), Some(2)).unwrap(),
    );
    decoder.send_packet(malformed, operation()).unwrap();
    let error = decoder.receive(operation()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);

    decoder.reset(operation()).unwrap();
    for packet in &packets {
        decoder.send_packet(packet.clone(), operation()).unwrap();
    }
    decoder.flush(operation()).unwrap();
    let first = match decoder.receive(operation()).unwrap() {
        DecodeOutput::Frame(frame) => frame,
        DecodeOutput::EndOfStream => panic!("reset decoder produced no frame"),
        DecodeOutput::NeedInput => panic!("flushed reset decoder requested input"),
        DecodeOutput::Audio(_) => panic!("AV1 decoder returned audio"),
        _ => panic!("unknown decoder output"),
    };
    assert_eq!(first.timestamp(), frame.timestamp());

    let token = CancellationToken::new();
    token.cancel();
    let cancelled = OperationContext::new(MediaPriority::Interactive).with_cancellation(token);
    let encoder_config = EncoderConfig::video(
        STREAM_ID,
        av1(),
        frame.timestamp().timebase(),
        frame.format(),
    );
    let error = backend
        .create_encoder(&encoder_config, &cancelled)
        .err()
        .expect("cancelled AV1 encoder creation unexpectedly succeeded");
    assert_eq!(error.category(), ErrorCategory::Cancelled);
}

#[test]
fn alpha_is_never_silently_discarded() {
    let format = VideoFormat::new(
        WIDTH,
        HEIGHT,
        PixelFormat::Rgba8Unorm,
        ColorSpace::SRGB,
        AlphaMode::Straight,
    )
    .unwrap();
    let config = EncoderConfig::video(STREAM_ID, av1(), Timebase::integer(24).unwrap(), format);
    let backend = backend(BackendRequirement::encode(av1()));
    let error = backend
        .create_encoder(&config, operation())
        .err()
        .expect("AV1 encoder silently accepted straight alpha");
    assert_eq!(error.category(), ErrorCategory::Unsupported);
}
