use std::sync::{Arc, OnceLock};

use flacenc::bitsink::ByteSink;
use flacenc::component::BitRepr;
use flacenc::error::Verify;
use flacenc::source::MemSource;
use superi_codecs_rs::flac::FlacBackend;
use superi_codecs_rs::register::register_default_backends;
use superi_core::error::ErrorCategory;
use superi_core::ids::MediaId;
use superi_core::pixel::{ChannelLayout, ChannelPosition, SampleFormat};
use superi_core::time::{RationalTime, SampleTime, Timebase};
use superi_media_io::audio_io::{AudioBlock, AudioFormat, AudioPlane};
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendRegistration, BackendRegistry,
    BackendRequirement, BackendTier, FallbackPolicy,
};
use superi_media_io::decode::{DecodeOutput, DecoderConfig};
use superi_media_io::demux::{
    MetadataValue, Packet, PacketTiming, SeekMode, SeekRequest, SourceLocation, SourceProbeLimits,
    SourceRequest, StreamId, StreamInfo, StreamKind,
};
use superi_media_io::encode::{EncodeInput, EncodeOutput, EncoderConfig};
use superi_media_io::mkv_webm::MkvWebmBackend;
use superi_media_io::operation::{CancellationToken, MediaPriority, OperationContext};
use superi_media_io::read::ReadOutcome;

fn operation() -> &'static OperationContext {
    static OPERATION: OnceLock<OperationContext> = OnceLock::new();
    OPERATION.get_or_init(|| OperationContext::new(MediaPriority::Interactive))
}

fn flac_backend() -> Arc<dyn superi_media_io::backend::MediaBackend> {
    let mut registry = BackendRegistry::new();
    register_default_backends(&mut registry).unwrap();
    registry
        .select(
            &BackendRequirement::decode(FlacBackend::codec_id()),
            FallbackPolicy::Disallow,
        )
        .unwrap()
        .primary()
        .clone()
}

fn reference_flac(samples: &[i32], channels: usize, bits: usize, rate: usize) -> Vec<u8> {
    let config = flacenc::config::Encoder::default().into_verified().unwrap();
    let source = MemSource::from_samples(samples, channels, bits, rate);
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size).unwrap();
    let mut sink = ByteSink::new();
    stream.write(&mut sink).unwrap();
    sink.as_slice().to_vec()
}

fn stream(stream_id: StreamId, sample_rate: u32) -> StreamInfo {
    StreamInfo::new(
        stream_id,
        StreamKind::Audio,
        FlacBackend::codec_id(),
        Timebase::integer(sample_rate).unwrap(),
    )
}

fn layout(channels: usize) -> ChannelLayout {
    use ChannelPosition::{
        BackCenter, BackLeft, BackRight, FrontCenter, FrontLeft, FrontRight, LowFrequency,
        SideLeft, SideRight,
    };
    ChannelLayout::new(match channels {
        1 => vec![FrontCenter],
        2 => vec![FrontLeft, FrontRight],
        3 => vec![FrontLeft, FrontRight, FrontCenter],
        4 => vec![FrontLeft, FrontRight, BackLeft, BackRight],
        5 => vec![FrontLeft, FrontRight, FrontCenter, BackLeft, BackRight],
        6 => vec![
            FrontLeft,
            FrontRight,
            FrontCenter,
            LowFrequency,
            BackLeft,
            BackRight,
        ],
        7 => vec![
            FrontLeft,
            FrontRight,
            FrontCenter,
            LowFrequency,
            BackCenter,
            SideLeft,
            SideRight,
        ],
        8 => vec![
            FrontLeft,
            FrontRight,
            FrontCenter,
            LowFrequency,
            BackLeft,
            BackRight,
            SideLeft,
            SideRight,
        ],
        _ => panic!("unsupported FLAC test channel count"),
    })
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
    panic!("FLAC fixture element is too large");
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
    panic!("FLAC fixture VINT is too large");
}

fn flac_mkv_fixture(native: &[u8]) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let mut frame_offset = 4;
    loop {
        let header = native[frame_offset];
        let length = usize::from(native[frame_offset + 1]) << 16
            | usize::from(native[frame_offset + 2]) << 8
            | usize::from(native[frame_offset + 3]);
        frame_offset += 4 + length;
        if header & 0x80 != 0 {
            break;
        }
    }
    let codec_private = native[..frame_offset].to_vec();
    let frame = native[frame_offset..].to_vec();

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

    let mut audio = ebml_float(&[0xB5], 48_000.0);
    audio.extend(ebml_unsigned(&[0x9F], 2));
    audio.extend(ebml_unsigned(&[0x62, 0x64], 16));
    let mut track = ebml_unsigned(&[0xD7], 1);
    track.extend(ebml_unsigned(&[0x73, 0xC5], 17));
    track.extend(ebml_unsigned(&[0x83], 2));
    track.extend(ebml_text(&[0x86], "A_FLAC"));
    track.extend(ebml_element(&[0x63, 0xA2], &codec_private));
    track.extend(ebml_element(&[0xE1], audio));

    let simple_block = |relative_time: i16| {
        let mut payload = ebml_data_vint(1);
        payload.extend_from_slice(&relative_time.to_be_bytes());
        payload.push(0x80);
        payload.extend_from_slice(&frame);
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
    (bytes, codec_private, frame)
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
        MediaId::from_raw(0x17),
        SourceLocation::Memory {
            name: "flac.mkv".into(),
            data: Arc::from(bytes),
        },
    )
}

#[test]
fn default_registry_exposes_primary_flac_decode_and_encode() {
    let mut registry = BackendRegistry::new();
    register_default_backends(&mut registry).unwrap();

    for requirement in [
        BackendRequirement::decode(FlacBackend::codec_id()),
        BackendRequirement::encode(FlacBackend::codec_id()),
    ] {
        let selection = registry
            .select(&requirement, FallbackPolicy::Disallow)
            .unwrap();
        assert_eq!(selection.primary().descriptor().id().as_str(), "rust-flac");
        assert!(!selection.fallback_used());
        assert!(selection.fallbacks().is_empty());
    }
}

#[test]
fn decodes_native_flac_with_exact_timing_samples_and_packet_metadata() {
    let backend = flac_backend();
    let stream_id = StreamId::new(17);
    let samples = [-32_768, 32_767, -1, 1, 0, 17, -1234, 2345];
    let encoded = reference_flac(&samples, 2, 16, 48_000);
    let mut packet = Packet::new(
        stream_id,
        Arc::from(encoded),
        PacketTiming::new(
            Timebase::integer(48_000).unwrap(),
            Some(120),
            Some(120),
            Some(4),
        )
        .unwrap(),
    );
    packet
        .metadata_mut()
        .insert("edit.origin", MetadataValue::Text("timeline".into()))
        .unwrap();

    let config = DecoderConfig::new(stream(stream_id, 48_000));
    let mut decoder = backend.create_decoder(&config, operation()).unwrap();
    decoder.send_packet(packet, operation()).unwrap();

    let DecodeOutput::Audio(block) = decoder.receive(operation()).unwrap() else {
        panic!("FLAC decoder did not return audio");
    };
    assert_eq!(block.format().sample_rate(), 48_000);
    assert_eq!(block.format().sample_format(), SampleFormat::I16);
    assert_eq!(block.format().channel_layout(), &ChannelLayout::stereo());
    assert_eq!(block.timestamp().sample(), 120);
    assert_eq!(block.frame_count(), 4);
    let expected = samples
        .iter()
        .flat_map(|sample| (*sample as i16).to_le_bytes())
        .collect::<Vec<_>>();
    assert_eq!(block.planes()[0].bytes(), expected);
    assert_eq!(
        block.metadata().get("edit.origin"),
        Some(&MetadataValue::Text("timeline".into()))
    );
    assert_eq!(
        block.metadata().get("flac.bits-per-sample"),
        Some(&MetadataValue::Unsigned(16))
    );
}

#[test]
fn encodes_native_flac_on_flush_and_round_trips_exact_audio() {
    let backend = flac_backend();
    let stream_id = StreamId::new(21);
    let format = AudioFormat::new(48_000, SampleFormat::I16, ChannelLayout::stereo()).unwrap();
    let samples = [-32_768_i16, 32_767, -1, 1, 0, 17, -1234, 2345];
    let bytes = samples
        .iter()
        .flat_map(|sample| sample.to_le_bytes())
        .collect::<Vec<_>>();
    let block = AudioBlock::new(
        format.clone(),
        SampleTime::new(-48, 48_000).unwrap(),
        4,
        vec![AudioPlane::new(Arc::from(bytes.clone()))],
    )
    .unwrap()
    .with_metadata("project.reel", MetadataValue::Text("A001".into()))
    .unwrap();
    let mut encoder = backend
        .create_encoder(
            &EncoderConfig::audio(stream_id, FlacBackend::codec_id(), format),
            operation(),
        )
        .unwrap();
    encoder
        .send(EncodeInput::Audio(block), operation())
        .unwrap();
    assert!(matches!(
        encoder.receive(operation()).unwrap(),
        EncodeOutput::NeedInput
    ));
    encoder.flush(operation()).unwrap();
    let EncodeOutput::Packet(packet) = encoder.receive(operation()).unwrap() else {
        panic!("FLAC encoder did not return a packet after flush");
    };
    assert!(packet.data().starts_with(b"fLaC"));
    assert_eq!(packet.timing().presentation_time().unwrap().value(), -48);
    assert_eq!(packet.timing().duration().unwrap().value(), 4);
    assert!(packet.is_keyframe());
    assert_eq!(
        packet.metadata().get("project.reel"),
        Some(&MetadataValue::Text("A001".into()))
    );
    let clean_packet = Packet::new(
        packet.stream_id(),
        Arc::from(packet.data()),
        packet.timing(),
    )
    .with_keyframe(packet.is_keyframe());

    let mut decoder = backend
        .create_decoder(&DecoderConfig::new(stream(stream_id, 48_000)), operation())
        .unwrap();
    decoder.send_packet(clean_packet, operation()).unwrap();
    let DecodeOutput::Audio(decoded) = decoder.receive(operation()).unwrap() else {
        panic!("FLAC round trip did not return audio");
    };
    assert_eq!(decoded.timestamp().sample(), -48);
    assert_eq!(decoded.frame_count(), 4);
    assert_eq!(decoded.planes()[0].bytes(), bytes);
    assert_eq!(
        decoded.metadata().get("project.reel"),
        Some(&MetadataValue::Text("A001".into()))
    );
}

#[test]
fn decodes_every_supported_flac_precision_without_sample_loss() {
    let backend = flac_backend();
    for (index, bits) in [8_usize, 12, 16, 20, 24].into_iter().enumerate() {
        let limit = 1_i32 << (bits - 1);
        let samples = [-limit, limit - 1, -1, 1];
        let encoded = reference_flac(&samples, 1, bits, 44_100);
        let stream_id = StreamId::new(100 + index as u32);
        let packet = Packet::new(
            stream_id,
            Arc::from(encoded),
            PacketTiming::new(
                Timebase::integer(44_100).unwrap(),
                Some(7),
                Some(7),
                Some(4),
            )
            .unwrap(),
        );
        let mut decoder = backend
            .create_decoder(&DecoderConfig::new(stream(stream_id, 44_100)), operation())
            .unwrap();
        decoder.send_packet(packet, operation()).unwrap();
        let DecodeOutput::Audio(block) = decoder.receive(operation()).unwrap() else {
            panic!("FLAC decoder did not return precision test audio");
        };
        let expected_format = if bits <= 16 {
            SampleFormat::I16
        } else {
            SampleFormat::I24
        };
        assert_eq!(block.format().sample_format(), expected_format);
        assert_eq!(
            block.metadata().get("flac.bits-per-sample"),
            Some(&MetadataValue::Unsigned(bits as u64))
        );
        let expected = if bits <= 16 {
            samples
                .iter()
                .flat_map(|sample| (*sample as i16).to_le_bytes())
                .collect::<Vec<_>>()
        } else {
            samples
                .iter()
                .flat_map(|sample| sample.to_le_bytes()[..3].to_vec())
                .collect::<Vec<_>>()
        };
        assert_eq!(block.planes()[0].bytes(), expected);
    }
}

#[test]
fn maps_all_flac_channel_assignments_to_canonical_editor_layouts() {
    let backend = flac_backend();
    for channels in 1..=8 {
        let samples = (0..channels as i32).collect::<Vec<_>>();
        let encoded = reference_flac(&samples, channels, 16, 48_000);
        let stream_id = StreamId::new(200 + channels as u32);
        let packet = Packet::new(
            stream_id,
            Arc::from(encoded),
            PacketTiming::new(
                Timebase::integer(48_000).unwrap(),
                Some(0),
                Some(0),
                Some(1),
            )
            .unwrap(),
        );
        let mut decoder = backend
            .create_decoder(&DecoderConfig::new(stream(stream_id, 48_000)), operation())
            .unwrap();
        decoder.send_packet(packet, operation()).unwrap();
        let DecodeOutput::Audio(block) = decoder.receive(operation()).unwrap() else {
            panic!("FLAC decoder did not return channel test audio");
        };
        assert_eq!(block.format().channel_layout(), &layout(channels));
    }
}

#[test]
fn round_trips_planar_twenty_bit_audio_and_reset_starts_a_new_stream() {
    let backend = flac_backend();
    let stream_id = StreamId::new(310);
    let format = AudioFormat::new(96_000, SampleFormat::I24Planar, layout(2)).unwrap();
    let left = [-524_288_i32, -1, 17];
    let right = [524_287_i32, 1, -17];
    let plane = |samples: &[i32]| {
        AudioPlane::new(Arc::from(
            samples
                .iter()
                .flat_map(|sample| sample.to_le_bytes()[..3].to_vec())
                .collect::<Vec<_>>(),
        ))
    };
    let make_block = |timestamp| {
        AudioBlock::new(
            format.clone(),
            SampleTime::new(timestamp, 96_000).unwrap(),
            3,
            vec![plane(&left), plane(&right)],
        )
        .unwrap()
        .with_metadata("flac.bits-per-sample", MetadataValue::Unsigned(20))
        .unwrap()
    };
    let mut encoder = backend
        .create_encoder(
            &EncoderConfig::audio(stream_id, FlacBackend::codec_id(), format.clone()),
            operation(),
        )
        .unwrap();
    encoder
        .send(EncodeInput::Audio(make_block(30)), operation())
        .unwrap();
    encoder.flush(operation()).unwrap();
    let EncodeOutput::Packet(first) = encoder.receive(operation()).unwrap() else {
        panic!("FLAC encoder did not return first planar stream");
    };
    assert!(matches!(
        encoder.receive(operation()).unwrap(),
        EncodeOutput::EndOfStream
    ));
    assert_eq!(
        encoder
            .send(EncodeInput::Audio(make_block(33)), operation())
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );
    encoder.reset(operation()).unwrap();
    encoder
        .send(EncodeInput::Audio(make_block(-3)), operation())
        .unwrap();
    encoder.flush(operation()).unwrap();
    let EncodeOutput::Packet(second) = encoder.receive(operation()).unwrap() else {
        panic!("FLAC encoder did not return reset planar stream");
    };
    assert_eq!(first.timing().presentation_time().unwrap().value(), 30);
    assert_eq!(second.timing().presentation_time().unwrap().value(), -3);

    let mut decoder = backend
        .create_decoder(
            &DecoderConfig::new(stream(stream_id, 96_000))
                .with_audio_format(format)
                .unwrap(),
            operation(),
        )
        .unwrap();
    decoder.send_packet(second, operation()).unwrap();
    let DecodeOutput::Audio(decoded) = decoder.receive(operation()).unwrap() else {
        panic!("FLAC decoder did not return planar audio");
    };
    assert_eq!(decoded.timestamp().sample(), -3);
    assert_eq!(decoded.planes()[0], plane(&left));
    assert_eq!(decoded.planes()[1], plane(&right));
    decoder.flush(operation()).unwrap();
    assert!(matches!(
        decoder.receive(operation()).unwrap(),
        DecodeOutput::EndOfStream
    ));
    decoder.reset(operation()).unwrap();
    assert!(matches!(
        decoder.receive(operation()).unwrap(),
        DecodeOutput::NeedInput
    ));
}

#[test]
fn rejects_corrupt_mismatched_and_unsupported_flac_inputs() {
    let backend = flac_backend();
    let stream_id = StreamId::new(400);
    let encoded = reference_flac(&[1, 2, 3, 4], 1, 16, 48_000);
    let packet = |data: Vec<u8>| {
        Packet::new(
            stream_id,
            Arc::from(data),
            PacketTiming::new(
                Timebase::integer(48_000).unwrap(),
                Some(0),
                Some(0),
                Some(4),
            )
            .unwrap(),
        )
    };
    let wrong_format = AudioFormat::new(44_100, SampleFormat::I16, layout(1)).unwrap();
    let mut decoder = backend
        .create_decoder(
            &DecoderConfig::new(stream(stream_id, 48_000))
                .with_audio_format(wrong_format)
                .unwrap(),
            operation(),
        )
        .unwrap();
    assert_eq!(
        decoder
            .send_packet(packet(encoded.clone()), operation())
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );

    let mut corrupt_bytes = encoded;
    let last = corrupt_bytes.last_mut().unwrap();
    *last ^= 0xff;
    let mut decoder = backend
        .create_decoder(&DecoderConfig::new(stream(stream_id, 48_000)), operation())
        .unwrap();
    assert_eq!(
        decoder
            .send_packet(packet(corrupt_bytes), operation())
            .unwrap_err()
            .category(),
        ErrorCategory::CorruptData
    );

    let mut encoded_32 = reference_flac(&[1, -1, 2, -2], 1, 24, 48_000);
    let mut stream_fields = u64::from_be_bytes(encoded_32[18..26].try_into().unwrap());
    stream_fields = (stream_fields & !(0x1f_u64 << 36)) | (31_u64 << 36);
    encoded_32[18..26].copy_from_slice(&stream_fields.to_be_bytes());
    let mut decoder = backend
        .create_decoder(&DecoderConfig::new(stream(stream_id, 48_000)), operation())
        .unwrap();
    assert_eq!(
        decoder
            .send_packet(packet(encoded_32), operation())
            .unwrap_err()
            .category(),
        ErrorCategory::Unsupported
    );
}

#[test]
fn honors_cancellation_before_codec_work() {
    let backend = flac_backend();
    let token = CancellationToken::new();
    token.cancel();
    let cancelled = OperationContext::new(MediaPriority::Interactive).with_cancellation(token);
    let error = backend
        .create_decoder(
            &DecoderConfig::new(stream(StreamId::new(500), 48_000)),
            &cancelled,
        )
        .err()
        .expect("cancelled decoder creation should fail");
    assert_eq!(error.category(), ErrorCategory::Cancelled);
}

#[test]
fn ingests_relinks_seeks_and_decodes_matroska_flac_frames() {
    let samples = [-32_768_i32, 32_767, -1, 1, 0, 17, -1234, 2345];
    let native = reference_flac(&samples, 2, 16, 48_000);
    let (bytes, codec_private, frame) = flac_mkv_fixture(&native);
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
    assert_eq!(first.info().streams().len(), 1);
    assert_eq!(first.info().streams()[0].codec(), &FlacBackend::codec_id());
    assert_eq!(
        first.info().streams()[0]
            .metadata()
            .get("codec.configuration"),
        Some(&MetadataValue::Bytes(Arc::from(codec_private)))
    );

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
        _ => panic!("Matroska source did not return the sought FLAC packet"),
    };
    assert_eq!(packet.data(), frame);
    assert_eq!(
        packet.timing().presentation_time().unwrap().value(),
        1_000_000_000
    );
    assert!(packet.timing().duration().is_none());

    let backend = flac_backend();
    let mut decoder = backend
        .create_decoder(
            &DecoderConfig::new(source.info().streams()[0].clone()),
            operation(),
        )
        .unwrap();
    decoder.send_packet(packet, operation()).unwrap();
    let DecodeOutput::Audio(block) = decoder.receive(operation()).unwrap() else {
        panic!("Matroska FLAC playback did not return audio");
    };
    assert_eq!(block.timestamp().sample(), 48_000);
    assert_eq!(block.frame_count(), 4);
    let expected = samples
        .iter()
        .flat_map(|sample| (*sample as i16).to_le_bytes())
        .collect::<Vec<_>>();
    assert_eq!(block.planes()[0].bytes(), expected);

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
