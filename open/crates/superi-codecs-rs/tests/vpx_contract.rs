use std::sync::{Arc, OnceLock};

use superi_codecs_rs::vp9::{VpxBackend, VpxCodec};
use superi_core::color_space::ColorSpace;
use superi_core::error::ErrorCategory;
use superi_core::ids::MediaId;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::time::{Duration, RationalTime, Timebase};
use superi_media_io::backend::{
    BackendCapability, BackendRequirement, BackendTier, FallbackPolicy, MediaBackend,
};
use superi_media_io::decode::{
    CpuVideoBuffer, DecodeOutput, Decoder, DecoderConfig, VideoFormat, VideoFrame, VideoPlane,
};
use superi_media_io::demux::{
    MetadataValue, Packet, PacketTiming, SourceLocation, SourceRequest, StreamId, StreamInfo,
    StreamKind,
};
use superi_media_io::encode::{EncodeInput, EncodeOutput, EncoderConfig};
use superi_media_io::mkv_webm::MkvWebmBackend;
use superi_media_io::operation::{CancellationToken, MediaPriority, OperationContext};
use superi_media_io::read::ReadOutcome;

const WIDTH: u32 = 17;
const HEIGHT: u32 = 15;
const STREAM_ID: StreamId = StreamId::new(7);

fn operation() -> &'static OperationContext {
    static OPERATION: OnceLock<OperationContext> = OnceLock::new();
    OPERATION.get_or_init(|| OperationContext::new(MediaPriority::Interactive))
}

fn backend() -> Arc<dyn MediaBackend> {
    Arc::new(VpxBackend::new().unwrap())
}

fn format(pixel_format: PixelFormat, color_space: ColorSpace) -> VideoFormat {
    VideoFormat::new(WIDTH, HEIGHT, pixel_format, color_space, AlphaMode::Opaque).unwrap()
}

fn frame(pixel_format: PixelFormat, color_space: ColorSpace, timestamp: i64) -> VideoFrame {
    let format = format(pixel_format, color_space);
    let (chroma_width, chroma_height) = match pixel_format {
        PixelFormat::Yuv420p8 | PixelFormat::Yuv420p10 => (WIDTH.div_ceil(2), HEIGHT.div_ceil(2)),
        PixelFormat::Yuv422p8 | PixelFormat::Yuv422p10 => (WIDTH.div_ceil(2), HEIGHT),
        PixelFormat::Yuv444p8 | PixelFormat::Yuv444p10 => (WIDTH, HEIGHT),
        _ => panic!("unsupported VPx fixture pixel format"),
    };
    let bytes_per_sample = usize::from(pixel_format.bits_per_component() > 8) + 1;
    let plane = |width: u32, height: u32, value: u16| {
        let mut bytes =
            Vec::with_capacity(usize::try_from(width * height).unwrap() * bytes_per_sample);
        for _ in 0..width * height {
            if bytes_per_sample == 1 {
                bytes.push(u8::try_from(value).unwrap());
            } else {
                bytes.extend_from_slice(&value.to_le_bytes());
            }
        }
        VideoPlane::new(
            Arc::from(bytes),
            usize::try_from(width).unwrap() * bytes_per_sample,
            height,
        )
        .unwrap()
    };
    let max = if bytes_per_sample == 1 { 255 } else { 1_023 };
    let planes = vec![
        plane(WIDTH, HEIGHT, max * 3 / 5),
        plane(chroma_width, chroma_height, max * 2 / 5),
        plane(chroma_width, chroma_height, max * 4 / 5),
    ];
    let buffer = CpuVideoBuffer::new(WIDTH, HEIGHT, pixel_format, planes).unwrap();
    VideoFrame::new(
        format,
        RationalTime::new(timestamp, Timebase::integer(30).unwrap()),
        Duration::new(1, Timebase::integer(30).unwrap()).unwrap(),
        Arc::new(buffer),
    )
    .unwrap()
    .with_metadata("source.frame-number", MetadataValue::Signed(timestamp))
    .unwrap()
}

fn encode(codec: VpxCodec, input: VideoFrame) -> Vec<Packet> {
    let format = input.format();
    let mut encoder = backend()
        .create_encoder(
            &EncoderConfig::video(
                STREAM_ID,
                codec.codec_id(),
                Timebase::integer(30).unwrap(),
                format,
            ),
            operation(),
        )
        .unwrap();
    encoder
        .send(EncodeInput::Video(input), operation())
        .unwrap();
    let mut packets = Vec::new();
    loop {
        match encoder.receive(operation()).unwrap() {
            EncodeOutput::Packet(packet) => packets.push(packet),
            EncodeOutput::NeedInput => break,
            EncodeOutput::EndOfStream => panic!("encoder ended before flush"),
            _ => panic!("unknown VPx encode output"),
        }
    }
    encoder.flush(operation()).unwrap();
    loop {
        match encoder.receive(operation()).unwrap() {
            EncodeOutput::Packet(packet) => packets.push(packet),
            EncodeOutput::EndOfStream => break,
            EncodeOutput::NeedInput => panic!("flushed encoder requested input"),
            _ => panic!("unknown VPx encode output"),
        }
    }
    packets
}

fn decoder(codec: VpxCodec, timebase: Timebase) -> Box<dyn Decoder> {
    let stream = StreamInfo::new(STREAM_ID, StreamKind::Video, codec.codec_id(), timebase);
    backend()
        .create_decoder(&DecoderConfig::new(stream), operation())
        .unwrap()
}

fn decode(codec: VpxCodec, packets: Vec<Packet>) -> Vec<VideoFrame> {
    let timebase = packets[0].timing().timebase();
    let mut decoder = decoder(codec, timebase);
    let mut frames = Vec::new();
    for packet in packets {
        decoder.send_packet(packet, operation()).unwrap();
        loop {
            match decoder.receive(operation()).unwrap() {
                DecodeOutput::Frame(frame) => frames.push(frame),
                DecodeOutput::NeedInput => break,
                DecodeOutput::EndOfStream => panic!("decoder ended before flush"),
                DecodeOutput::Audio(_) => panic!("VPx decoder returned audio"),
                _ => panic!("unknown VPx decode output"),
            }
        }
    }
    decoder.flush(operation()).unwrap();
    loop {
        match decoder.receive(operation()).unwrap() {
            DecodeOutput::Frame(frame) => frames.push(frame),
            DecodeOutput::EndOfStream => break,
            DecodeOutput::NeedInput => panic!("flushed decoder requested input"),
            DecodeOutput::Audio(_) => panic!("VPx decoder returned audio"),
            _ => panic!("unknown VPx decode output"),
        }
    }
    frames
}

fn mean_sample(plane: &VideoPlane, bits: u8) -> u64 {
    if bits == 8 {
        plane
            .bytes()
            .iter()
            .map(|value| u64::from(*value))
            .sum::<u64>()
            / u64::try_from(plane.bytes().len()).unwrap()
    } else {
        let samples = plane
            .bytes()
            .chunks_exact(2)
            .map(|bytes| u64::from(u16::from_le_bytes([bytes[0], bytes[1]])))
            .collect::<Vec<_>>();
        samples.iter().sum::<u64>() / u64::try_from(samples.len()).unwrap()
    }
}

#[test]
fn registration_declares_primary_vp8_and_vp9_decode_and_encode() {
    let registration = VpxBackend::registration().unwrap();

    assert_eq!(registration.backend().descriptor().id().as_str(), "libvpx");
    assert_eq!(registration.tier(), BackendTier::Primary);
    assert_eq!(registration.priority(), 100);
    for codec in [VpxCodec::Vp8, VpxCodec::Vp9] {
        let id = codec.codec_id();
        assert!(registration
            .capabilities()
            .contains(&BackendCapability::Decode(id.clone())));
        assert!(registration
            .capabilities()
            .contains(&BackendCapability::Encode(id.clone())));
    }

    let mut registry = superi_media_io::backend::BackendRegistry::new();
    registry.register(registration).unwrap();
    let selected = registry
        .select(
            &BackendRequirement::decode(VpxCodec::Vp9.codec_id()),
            FallbackPolicy::Disallow,
        )
        .unwrap();
    assert_eq!(selected.primary().descriptor().id().as_str(), "libvpx");
}

#[test]
fn vp8_and_vp9_round_trip_real_frames_with_exact_timing_and_metadata() {
    for codec in [VpxCodec::Vp8, VpxCodec::Vp9] {
        let packets = encode(codec, frame(PixelFormat::Yuv420p8, ColorSpace::BT709, 7));
        assert!(!packets.is_empty());
        assert!(packets[0].is_keyframe());
        assert_eq!(
            packets[0].timing().presentation_time().unwrap(),
            RationalTime::new(7, Timebase::integer(30).unwrap())
        );
        assert_eq!(packets[0].timing().duration().unwrap().value(), 1);
        assert_eq!(
            packets[0].metadata().get("source.frame-number"),
            Some(&MetadataValue::Signed(7))
        );

        let decoded = decode(codec, packets);
        assert_eq!(decoded.len(), 1);
        let decoded = &decoded[0];
        assert_eq!(decoded.format().width(), WIDTH);
        assert_eq!(decoded.format().height(), HEIGHT);
        assert_eq!(decoded.format().pixel_format(), PixelFormat::Yuv420p8);
        assert_eq!(
            decoded.timestamp(),
            RationalTime::new(7, Timebase::integer(30).unwrap())
        );
        assert_eq!(decoded.duration().value(), 1);
        assert_eq!(
            decoded.metadata().get("source.frame-number"),
            Some(&MetadataValue::Signed(7))
        );
        assert_eq!(
            decoded.metadata().get("vpx.codec"),
            Some(&MetadataValue::Text(codec.code().to_owned()))
        );
        assert_eq!(
            decoded.metadata().get("video.keyframe"),
            Some(&MetadataValue::Boolean(true))
        );
        let cpu = decoded
            .buffer()
            .as_any()
            .downcast_ref::<CpuVideoBuffer>()
            .unwrap();
        assert_eq!(cpu.planes().len(), 3);
        assert!(mean_sample(&cpu.planes()[0], 8).abs_diff(153) <= 10);
        assert!(mean_sample(&cpu.planes()[1], 8).abs_diff(102) <= 10);
        assert!(mean_sample(&cpu.planes()[2], 8).abs_diff(204) <= 10);
    }
}

#[test]
fn vp9_round_trips_every_supported_planar_format_without_downconversion() {
    for pixel_format in [
        PixelFormat::Yuv420p8,
        PixelFormat::Yuv422p8,
        PixelFormat::Yuv444p8,
        PixelFormat::Yuv420p10,
        PixelFormat::Yuv422p10,
        PixelFormat::Yuv444p10,
    ] {
        let bits = pixel_format.bits_per_component();
        let color_space = if bits == 10 {
            ColorSpace::BT2020
        } else {
            ColorSpace::BT709
        };
        let packets = encode(VpxCodec::Vp9, frame(pixel_format, color_space, 11));
        let decoded = decode(VpxCodec::Vp9, packets);
        assert_eq!(decoded.len(), 1);
        let decoded = &decoded[0];
        assert_eq!(decoded.format().pixel_format(), pixel_format);
        assert_eq!(decoded.format().color_space(), color_space);
        assert_eq!(
            decoded.metadata().get("vpx.bit-depth"),
            Some(&MetadataValue::Unsigned(u64::from(bits)))
        );
        let cpu = decoded
            .buffer()
            .as_any()
            .downcast_ref::<CpuVideoBuffer>()
            .unwrap();
        let maximum = if bits == 8 { 255 } else { 1_023 };
        let tolerance = if bits == 8 { 10 } else { 40 };
        assert!(mean_sample(&cpu.planes()[0], bits).abs_diff(maximum * 3 / 5) <= tolerance);
        assert!(mean_sample(&cpu.planes()[1], bits).abs_diff(maximum * 2 / 5) <= tolerance);
        assert!(mean_sample(&cpu.planes()[2], bits).abs_diff(maximum * 4 / 5) <= tolerance);
    }
}

#[test]
fn rejects_alpha_loss_unsupported_formats_and_corrupt_packets() {
    let alpha_format = VideoFormat::new(
        WIDTH,
        HEIGHT,
        PixelFormat::Rgba8Unorm,
        ColorSpace::SRGB,
        AlphaMode::Straight,
    )
    .unwrap();
    let alpha_error = backend()
        .create_encoder(
            &EncoderConfig::video(
                STREAM_ID,
                VpxCodec::Vp9.codec_id(),
                Timebase::integer(30).unwrap(),
                alpha_format,
            ),
            operation(),
        )
        .err()
        .unwrap();
    assert_eq!(alpha_error.category(), ErrorCategory::Unsupported);

    let vp8_format_error = backend()
        .create_encoder(
            &EncoderConfig::video(
                STREAM_ID,
                VpxCodec::Vp8.codec_id(),
                Timebase::integer(30).unwrap(),
                format(PixelFormat::Yuv444p8, ColorSpace::BT709),
            ),
            operation(),
        )
        .err()
        .unwrap();
    assert_eq!(vp8_format_error.category(), ErrorCategory::Unsupported);

    let alpha_stream = StreamInfo::new(
        STREAM_ID,
        StreamKind::Video,
        VpxCodec::Vp9.codec_id(),
        Timebase::integer(30).unwrap(),
    )
    .with_metadata("video.alpha-mode", MetadataValue::Unsigned(1))
    .unwrap();
    let alpha_decode_error = backend()
        .create_decoder(&DecoderConfig::new(alpha_stream), operation())
        .err()
        .unwrap();
    assert_eq!(alpha_decode_error.category(), ErrorCategory::Unsupported);

    let mut decoder = decoder(VpxCodec::Vp9, Timebase::integer(30).unwrap());
    let corrupt_packet = Packet::new(
        STREAM_ID,
        Arc::from([0xFF; 16]),
        PacketTiming::new(Timebase::integer(30).unwrap(), Some(0), Some(0), Some(1)).unwrap(),
    );
    let corrupt_error = decoder
        .send_packet(corrupt_packet, operation())
        .unwrap_err();
    assert_eq!(corrupt_error.category(), ErrorCategory::CorruptData);
}

#[test]
fn cancellation_flush_and_reset_preserve_deterministic_lifecycles() {
    let video_format = format(PixelFormat::Yuv420p8, ColorSpace::BT709);
    let encoder_config = EncoderConfig::video(
        STREAM_ID,
        VpxCodec::Vp9.codec_id(),
        Timebase::integer(30).unwrap(),
        video_format,
    );
    let mut encoder = backend()
        .create_encoder(&encoder_config, operation())
        .unwrap();
    encoder
        .send(
            EncodeInput::Video(frame(PixelFormat::Yuv420p8, ColorSpace::BT709, 0)),
            operation(),
        )
        .unwrap();
    let packet = match encoder.receive(operation()).unwrap() {
        EncodeOutput::Packet(packet) => packet,
        _ => panic!("VP9 encoder did not return its first packet"),
    };
    encoder.flush(operation()).unwrap();
    while !matches!(
        encoder.receive(operation()).unwrap(),
        EncodeOutput::EndOfStream
    ) {}
    assert_eq!(
        encoder
            .send(
                EncodeInput::Video(frame(PixelFormat::Yuv420p8, ColorSpace::BT709, 1)),
                operation(),
            )
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );
    encoder.reset(operation()).unwrap();
    encoder
        .send(
            EncodeInput::Video(frame(PixelFormat::Yuv420p8, ColorSpace::BT709, 1)),
            operation(),
        )
        .unwrap();

    let mut decoder = decoder(VpxCodec::Vp9, Timebase::integer(30).unwrap());
    decoder.send_packet(packet.clone(), operation()).unwrap();
    assert!(matches!(
        decoder.receive(operation()).unwrap(),
        DecodeOutput::Frame(_)
    ));
    decoder.flush(operation()).unwrap();
    assert!(matches!(
        decoder.receive(operation()).unwrap(),
        DecodeOutput::EndOfStream
    ));
    decoder.reset(operation()).unwrap();
    decoder.send_packet(packet, operation()).unwrap();
    assert!(matches!(
        decoder.receive(operation()).unwrap(),
        DecodeOutput::Frame(_)
    ));

    let token = CancellationToken::new();
    token.cancel();
    let cancelled = OperationContext::new(MediaPriority::Interactive).with_cancellation(token);
    assert_eq!(
        decoder.receive(&cancelled).unwrap_err().category(),
        ErrorCategory::Cancelled
    );
}

fn element(id: &[u8], payload: impl AsRef<[u8]>) -> Vec<u8> {
    let payload = payload.as_ref();
    let mut bytes = Vec::with_capacity(id.len() + 8 + payload.len());
    bytes.extend_from_slice(id);
    bytes.extend(size_vint(payload.len()));
    bytes.extend_from_slice(payload);
    bytes
}

fn size_vint(value: usize) -> Vec<u8> {
    let value = u64::try_from(value).unwrap();
    for width in 1..=8 {
        if value < (1_u64 << (width * 7)) - 1 {
            let mut encoded = value.to_be_bytes()[8 - width..].to_vec();
            encoded[0] |= 1 << (8 - width);
            return encoded;
        }
    }
    panic!("fixture element is too large");
}

fn data_vint(value: u64) -> Vec<u8> {
    for width in 1..=8 {
        if value < (1_u64 << (width * 7)) - 1 {
            let mut encoded = value.to_be_bytes()[8 - width..].to_vec();
            encoded[0] |= 1 << (8 - width);
            return encoded;
        }
    }
    panic!("fixture VINT is too large");
}

fn unsigned(id: &[u8], value: u64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let first = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len() - 1);
    element(id, &bytes[first..])
}

fn text(id: &[u8], value: &str) -> Vec<u8> {
    element(id, value.as_bytes())
}

fn webm_fixture(codec: VpxCodec, packet: &Packet) -> Vec<u8> {
    let mut ebml = Vec::new();
    ebml.extend(unsigned(&[0x42, 0x86], 1));
    ebml.extend(unsigned(&[0x42, 0xF7], 1));
    ebml.extend(unsigned(&[0x42, 0xF2], 4));
    ebml.extend(unsigned(&[0x42, 0xF3], 8));
    ebml.extend(text(&[0x42, 0x82], "webm"));
    ebml.extend(unsigned(&[0x42, 0x87], 4));
    ebml.extend(unsigned(&[0x42, 0x85], 2));

    let mut info = unsigned(&[0x2A, 0xD7, 0xB1], 1_000_000);
    info.extend(element(&[0x44, 0x89], 34_f64.to_be_bytes()));

    let mut video = unsigned(&[0xB0], u64::from(WIDTH));
    video.extend(unsigned(&[0xBA], u64::from(HEIGHT)));
    let mut track = unsigned(&[0xD7], u64::from(STREAM_ID.value()));
    track.extend(unsigned(&[0x73, 0xC5], 21));
    track.extend(unsigned(&[0x83], 1));
    track.extend(unsigned(&[0x23, 0xE3, 0x83], 33_333_333));
    track.extend(text(
        &[0x86],
        if codec == VpxCodec::Vp8 {
            "V_VP8"
        } else {
            "V_VP9"
        },
    ));
    track.extend(element(&[0xE0], video));

    let mut block = data_vint(u64::from(STREAM_ID.value()));
    block.extend_from_slice(&0_i16.to_be_bytes());
    block.push(0x80);
    block.extend_from_slice(packet.data());
    let mut cluster = unsigned(&[0xE7], 0);
    cluster.extend(element(&[0xA3], block));

    let mut segment = element(&[0x15, 0x49, 0xA9, 0x66], info);
    segment.extend(element(&[0x16, 0x54, 0xAE, 0x6B], element(&[0xAE], track)));
    segment.extend(element(&[0x1F, 0x43, 0xB6, 0x75], cluster));
    let mut bytes = element(&[0x1A, 0x45, 0xDF, 0xA3], ebml);
    bytes.extend(element(&[0x18, 0x53, 0x80, 0x67], segment));
    bytes
}

#[test]
fn webm_ingest_packets_flow_through_the_real_decoder() {
    for codec in [VpxCodec::Vp8, VpxCodec::Vp9] {
        let encoded = encode(codec, frame(PixelFormat::Yuv420p8, ColorSpace::BT709, 0));
        assert_eq!(encoded.len(), 1);
        let request = SourceRequest::new(
            MediaId::from_raw(0xC021),
            SourceLocation::Memory {
                name: "fixture.webm".to_owned(),
                data: Arc::from(webm_fixture(codec, &encoded[0])),
            },
        );
        let mut source = MkvWebmBackend::new()
            .unwrap()
            .open_source(&request, operation())
            .unwrap();
        assert_eq!(source.info().streams()[0].codec(), &codec.codec_id());
        let stream = source.info().streams()[0].clone();
        let packet = match source.read_packet(operation()).unwrap() {
            ReadOutcome::Complete(packet) => packet,
            _ => panic!("WebM source did not return its video packet"),
        };
        let mut decoder = backend()
            .create_decoder(&DecoderConfig::new(stream), operation())
            .unwrap();
        decoder.send_packet(packet, operation()).unwrap();
        let frame = match decoder.receive(operation()).unwrap() {
            DecodeOutput::Frame(frame) => frame,
            _ => panic!("WebM packet did not decode to a frame"),
        };
        assert_eq!(frame.format().width(), WIDTH);
        assert_eq!(frame.format().height(), HEIGHT);
        if codec == VpxCodec::Vp9 {
            assert_eq!(frame.format().color_space(), ColorSpace::BT709);
        }
        assert_eq!(frame.timestamp().value(), 0);
        assert_eq!(frame.duration().value(), 33_333_333);
    }
}
