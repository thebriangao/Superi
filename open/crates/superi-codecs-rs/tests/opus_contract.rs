use std::f32::consts::TAU;
use std::sync::{Arc, OnceLock};

use superi_codecs_rs::opus::{OpusBackend, OPUS_CODEC_ID};
use superi_codecs_rs::register::register_default_backends;
use superi_core::error::ErrorCategory;
use superi_core::ids::MediaId;
use superi_core::pixel::{ChannelLayout, ChannelPosition, SampleFormat};
use superi_core::time::{SampleTime, Timebase};
use superi_media_io::audio_io::{AudioBlock, AudioFormat, AudioPlane};
use superi_media_io::backend::{BackendRegistry, BackendRequirement, FallbackPolicy, MediaBackend};
use superi_media_io::decode::{DecodeOutput, Decoder, DecoderConfig};
use superi_media_io::demux::{
    CodecId, MetadataValue, Packet, PacketTiming, SourceLocation, SourceRequest, StreamId,
    StreamInfo, StreamKind,
};
use superi_media_io::encode::{EncodeInput, EncodeOutput, Encoder, EncoderConfig};
use superi_media_io::mkv_webm::MkvWebmBackend;
use superi_media_io::operation::{CancellationToken, MediaPriority, OperationContext};
use superi_media_io::read::ReadOutcome;

const SAMPLE_RATE: u32 = 48_000;
const STREAM_ID: StreamId = StreamId::new(19);

fn operation() -> &'static OperationContext {
    static OPERATION: OnceLock<OperationContext> = OnceLock::new();
    OPERATION.get_or_init(|| OperationContext::new(MediaPriority::Export))
}

fn opus() -> CodecId {
    CodecId::new(OPUS_CODEC_ID).unwrap()
}

fn registry() -> BackendRegistry {
    let mut registry = BackendRegistry::new();
    register_default_backends(&mut registry).unwrap();
    registry
}

fn backend() -> Arc<dyn MediaBackend> {
    registry()
        .select(
            &BackendRequirement::encode(opus()),
            FallbackPolicy::Disallow,
        )
        .unwrap()
        .primary()
        .clone()
}

fn stream(sample_rate: u32, timebase: Timebase) -> StreamInfo {
    StreamInfo::new(STREAM_ID, StreamKind::Audio, opus(), timebase)
        .with_metadata(
            "audio.sample-rate",
            MetadataValue::Unsigned(u64::from(sample_rate)),
        )
        .unwrap()
}

fn stream_with_header(sample_rate: u32, timebase: Timebase, header: &[u8]) -> StreamInfo {
    stream(sample_rate, timebase)
        .with_metadata(
            "codec.configuration",
            MetadataValue::Bytes(Arc::from(header)),
        )
        .unwrap()
}

fn format(sample_rate: u32, sample_format: SampleFormat, channels: usize) -> AudioFormat {
    AudioFormat::new(sample_rate, sample_format, canonical_layout(channels)).unwrap()
}

fn canonical_layout(channels: usize) -> ChannelLayout {
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
        _ => panic!("unsupported Opus test channel count"),
    })
    .unwrap()
}

fn signal_block(
    sample_rate: u32,
    sample_format: SampleFormat,
    channels: usize,
    start: i64,
    frames: usize,
) -> AudioBlock {
    let format = format(sample_rate, sample_format, channels);
    let sample = |frame: usize, channel: usize| {
        let frequency = 120.0 + channel as f32 * 37.0;
        0.22 * (TAU * frequency * frame as f32 / sample_rate as f32).sin()
    };
    let planes = if sample_format.is_planar() {
        (0..channels)
            .map(|channel| {
                let bytes = (0..frames)
                    .flat_map(|frame| sample_bytes(sample(frame, channel), sample_format))
                    .collect::<Vec<_>>();
                AudioPlane::new(Arc::from(bytes))
            })
            .collect()
    } else {
        let bytes = (0..frames)
            .flat_map(|frame| {
                (0..channels)
                    .flat_map(move |channel| sample_bytes(sample(frame, channel), sample_format))
            })
            .collect::<Vec<_>>();
        vec![AudioPlane::new(Arc::from(bytes))]
    };
    AudioBlock::new(
        format,
        SampleTime::new(start, sample_rate).unwrap(),
        frames as u64,
        planes,
    )
    .unwrap()
}

fn sample_bytes(sample: f32, format: SampleFormat) -> Vec<u8> {
    match format {
        SampleFormat::I16 | SampleFormat::I16Planar => ((sample * 32_768.0)
            .round()
            .clamp(i16::MIN as f32, i16::MAX as f32)
            as i16)
            .to_le_bytes()
            .to_vec(),
        SampleFormat::F32 | SampleFormat::F32Planar => sample.to_le_bytes().to_vec(),
        _ => panic!("unsupported Opus test sample format"),
    }
}

fn drain_encoder(encoder: &mut dyn Encoder) -> (Vec<Packet>, bool) {
    let mut packets = Vec::new();
    loop {
        match encoder.receive(operation()).unwrap() {
            EncodeOutput::Packet(packet) => packets.push(packet),
            EncodeOutput::NeedInput => return (packets, false),
            EncodeOutput::EndOfStream => return (packets, true),
            output => panic!("unexpected Opus encoder output: {output:?}"),
        }
    }
}

fn encode(blocks: &[AudioBlock]) -> Vec<Packet> {
    let backend = backend();
    let mut encoder = backend
        .create_encoder(
            &EncoderConfig::audio(STREAM_ID, opus(), blocks[0].format().clone()),
            operation(),
        )
        .unwrap();
    let (mut packets, ended) = drain_encoder(encoder.as_mut());
    assert!(!ended);
    for block in blocks {
        encoder
            .send(EncodeInput::Audio(block.clone()), operation())
            .unwrap();
        let (next, ended) = drain_encoder(encoder.as_mut());
        assert!(!ended);
        packets.extend(next);
    }
    encoder.flush(operation()).unwrap();
    let (next, ended) = drain_encoder(encoder.as_mut());
    assert!(ended);
    packets.extend(next);
    packets
}

fn decode(config: DecoderConfig, packets: Vec<Packet>) -> Vec<AudioBlock> {
    let backend = backend();
    let mut decoder = backend.create_decoder(&config, operation()).unwrap();
    let mut blocks = Vec::new();
    for packet in packets {
        decoder.send_packet(packet, operation()).unwrap();
        loop {
            match decoder.receive(operation()).unwrap() {
                DecodeOutput::Audio(block) => blocks.push(block),
                DecodeOutput::NeedInput => break,
                DecodeOutput::Frame(_) => panic!("Opus decoder returned video"),
                DecodeOutput::EndOfStream => panic!("Opus decoder ended before flush"),
                output => panic!("unexpected Opus decoder output: {output:?}"),
            }
        }
    }
    decoder.flush(operation()).unwrap();
    assert!(matches!(
        decoder.receive(operation()).unwrap(),
        DecodeOutput::EndOfStream
    ));
    blocks
}

fn total_frames(blocks: &[AudioBlock]) -> u64 {
    blocks.iter().map(AudioBlock::frame_count).sum()
}

fn header_and_audio(packets: &[Packet]) -> (&Packet, &[Packet]) {
    assert!(packets[0].data().starts_with(b"OpusHead"));
    (&packets[0], &packets[1..])
}

#[test]
fn default_registry_exposes_primary_opus_decode_and_encode() {
    let registry = registry();

    for requirement in [
        BackendRequirement::decode(opus()),
        BackendRequirement::encode(opus()),
    ] {
        let selection = registry
            .select(&requirement, FallbackPolicy::Disallow)
            .unwrap();
        assert_eq!(selection.primary().descriptor().id().as_str(), "rust-opus");
        assert_eq!(selection.primary().descriptor().display_name(), "Rust Opus");
        assert_eq!(OpusBackend::codec_id(), opus());
        assert!(!selection.fallback_used());
        assert!(selection.fallbacks().is_empty());
    }
}

#[test]
fn raw_packets_round_trip_exact_timing_metadata_and_padding() {
    let blocks = [
        signal_block(SAMPLE_RATE, SampleFormat::F32Planar, 2, -960, 1_337)
            .with_metadata("project.reel", MetadataValue::Text("A001".into()))
            .unwrap(),
        signal_block(SAMPLE_RATE, SampleFormat::F32Planar, 2, 377, 2_501),
    ];
    let packets = encode(&blocks);
    assert_eq!(packets, encode(&blocks));
    let (header, audio) = header_and_audio(&packets);

    assert_eq!(header.data()[8], 1);
    assert_eq!(header.data()[9], 2);
    assert_eq!(
        u32::from_le_bytes(header.data()[12..16].try_into().unwrap()),
        SAMPLE_RATE
    );
    assert_eq!(header.data()[18], 0);
    assert_eq!(
        header.metadata().get("codec.seek-pre-roll-ns"),
        Some(&MetadataValue::Unsigned(80_000_000))
    );
    let pre_skip = u64::from(u16::from_le_bytes([header.data()[10], header.data()[11]]));
    assert!(pre_skip > 0);
    assert!(!audio.is_empty());
    assert_eq!(
        audio
            .iter()
            .map(|packet| packet.timing().duration().unwrap().value())
            .sum::<u64>(),
        3_838
    );
    assert_eq!(audio[0].timing().presentation_time().unwrap().value(), -960);
    let last = audio.last().unwrap();
    assert_eq!(
        last.timing().presentation_time().unwrap().value()
            + last.timing().duration().unwrap().value() as i64,
        2_878
    );
    assert!(last.metadata().get("codec.opus.padding-samples").is_some());

    let config = DecoderConfig::new(stream_with_header(
        SAMPLE_RATE,
        Timebase::integer(SAMPLE_RATE).unwrap(),
        header.data(),
    ));
    let decoded = decode(config, packets);
    assert_eq!(total_frames(&decoded), 3_838);
    assert_eq!(decoded[0].timestamp().sample(), -960);
    assert_eq!(
        decoded[0].format(),
        &format(SAMPLE_RATE, SampleFormat::F32Planar, 2)
    );
    assert!(decoded.iter().any(|block| {
        block.metadata().get("project.reel") == Some(&MetadataValue::Text("A001".into()))
    }));
    for pair in decoded.windows(2) {
        assert_eq!(
            pair[0].timestamp().sample() + pair[0].frame_count() as i64,
            pair[1].timestamp().sample()
        );
    }
}

#[test]
fn every_supported_rate_and_sample_representation_round_trips() {
    for sample_rate in [8_000, 12_000, 16_000, 24_000, 48_000] {
        for sample_format in [
            SampleFormat::I16,
            SampleFormat::I16Planar,
            SampleFormat::F32,
            SampleFormat::F32Planar,
        ] {
            let block = signal_block(sample_rate, sample_format, 2, 37, 1_123);
            let expected_format = block.format().clone();
            let packets = encode(&[block]);
            let header = &packets[0];
            assert_eq!(
                u32::from_le_bytes(header.data()[12..16].try_into().unwrap()),
                sample_rate
            );
            let config =
                DecoderConfig::new(stream(sample_rate, Timebase::integer(sample_rate).unwrap()))
                    .with_audio_format(expected_format.clone())
                    .unwrap();
            let decoded = decode(config, packets);
            assert_eq!(total_frames(&decoded), 1_123);
            assert_eq!(decoded[0].timestamp().sample(), 37);
            assert_eq!(decoded[0].format(), &expected_format);
        }
    }
}

#[test]
fn opus_head_output_gain_is_applied_without_changing_timing() {
    let packets = encode(&[signal_block(
        SAMPLE_RATE,
        SampleFormat::F32Planar,
        1,
        0,
        4_800,
    )]);
    let (header, audio) = header_and_audio(&packets);
    let baseline = decode(
        DecoderConfig::new(stream_with_header(
            SAMPLE_RATE,
            Timebase::integer(SAMPLE_RATE).unwrap(),
            header.data(),
        )),
        audio.to_vec(),
    );
    let mut boosted_header = header.data().to_vec();
    boosted_header[16..18].copy_from_slice(&1_536_i16.to_le_bytes());
    let boosted = decode(
        DecoderConfig::new(stream_with_header(
            SAMPLE_RATE,
            Timebase::integer(SAMPLE_RATE).unwrap(),
            &boosted_header,
        )),
        audio.to_vec(),
    );
    assert_eq!(total_frames(&baseline), total_frames(&boosted));
    assert_eq!(baseline[0].timestamp(), boosted[0].timestamp());
    let baseline_energy = collect_planar_f32(&baseline, 1)[0]
        .iter()
        .map(|sample| sample.abs())
        .sum::<f32>();
    let boosted_energy = collect_planar_f32(&boosted, 1)[0]
        .iter()
        .map(|sample| sample.abs())
        .sum::<f32>();
    let ratio = boosted_energy / baseline_energy;
    assert!(
        (1.9..2.1).contains(&ratio),
        "unexpected 6 dB gain ratio: {ratio}"
    );
}

#[test]
fn standard_one_through_eight_channel_mappings_preserve_channel_identity() {
    for channels in 1..=8 {
        let input = signal_block(SAMPLE_RATE, SampleFormat::F32Planar, channels, 0, 9_600);
        let packets = encode(&[input]);
        let header = &packets[0];
        assert_eq!(usize::from(header.data()[9]), channels);
        assert_eq!(header.data()[18], u8::from(channels > 2));
        let decoded = decode(
            DecoderConfig::new(stream_with_header(
                SAMPLE_RATE,
                Timebase::integer(SAMPLE_RATE).unwrap(),
                header.data(),
            )),
            packets,
        );
        assert_eq!(total_frames(&decoded), 9_600);
        assert_eq!(
            decoded[0].format().channel_layout(),
            &canonical_layout(channels)
        );

        let samples = collect_planar_f32(&decoded, channels);
        for (channel, samples) in samples.iter().enumerate() {
            let correlations = (0..channels)
                .map(|candidate| {
                    frequency_energy(samples, 120.0 + candidate as f32 * 37.0, SAMPLE_RATE)
                })
                .collect::<Vec<_>>();
            let strongest = correlations
                .iter()
                .enumerate()
                .max_by(|left, right| left.1.total_cmp(right.1))
                .unwrap()
                .0;
            assert_eq!(
                strongest, channel,
                "Opus channel {channel} moved in {channels}-channel output: {correlations:?}"
            );
        }
    }
}

fn collect_planar_f32(blocks: &[AudioBlock], channels: usize) -> Vec<Vec<f32>> {
    let mut output = vec![Vec::new(); channels];
    for block in blocks {
        assert_eq!(block.planes().len(), channels);
        for (channel, plane) in block.planes().iter().enumerate() {
            output[channel].extend(
                plane
                    .bytes()
                    .chunks_exact(4)
                    .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap())),
            );
        }
    }
    output
}

fn frequency_energy(samples: &[f32], frequency: f32, sample_rate: u32) -> f64 {
    let (sin_sum, cos_sum) = samples.iter().enumerate().fold(
        (0.0_f64, 0.0_f64),
        |(sin_sum, cos_sum), (index, sample)| {
            let phase =
                f64::from(TAU) * f64::from(frequency) * index as f64 / f64::from(sample_rate);
            (
                sin_sum + f64::from(*sample) * phase.sin(),
                cos_sum + f64::from(*sample) * phase.cos(),
            )
        },
    );
    sin_sum.hypot(cos_sum)
}

#[test]
fn matroska_source_packets_decode_with_codec_delay_and_end_trim() {
    let input = signal_block(SAMPLE_RATE, SampleFormat::F32Planar, 2, 0, 1_920);
    let encoded = encode(&[input]);
    let (header, audio) = header_and_audio(&encoded);
    let bytes = opus_mkv_fixture(header.data(), audio);
    let request = SourceRequest::new(
        MediaId::from_raw(0x19),
        SourceLocation::Memory {
            name: "opus.mkv".into(),
            data: Arc::from(bytes),
        },
    );
    let mut source = MkvWebmBackend::new()
        .unwrap()
        .open_source(&request, operation())
        .unwrap();
    let source_stream = source.info().streams()[0].clone();
    assert_eq!(source_stream.codec().as_str(), OPUS_CODEC_ID);
    assert_eq!(
        source_stream.metadata().get("codec.configuration"),
        Some(&MetadataValue::Bytes(Arc::from(header.data())))
    );
    assert_eq!(
        source_stream.metadata().get("codec.seek-pre-roll-ns"),
        Some(&MetadataValue::Unsigned(80_000_000))
    );

    let mut packets = Vec::new();
    loop {
        match source.read_packet(operation()).unwrap() {
            ReadOutcome::Complete(packet) => packets.push(packet),
            ReadOutcome::EndOfStream => break,
            output => panic!("unexpected Matroska read output: {output:?}"),
        }
    }
    assert_eq!(packets.len(), audio.len());
    assert!(packets
        .last()
        .unwrap()
        .metadata()
        .get("container.discard-padding-ns")
        .is_some());
    let decoded = decode(DecoderConfig::new(source_stream), packets);
    assert_eq!(total_frames(&decoded), 1_920);
    assert_eq!(decoded[0].timestamp().sample(), 0);
    assert!(decoded.iter().any(|block| {
        block.metadata().get("container.offset").is_some()
            && block
                .metadata()
                .get("container.cluster-timestamp-ticks")
                .is_some()
    }));
}

#[test]
fn lifecycle_invalid_input_corruption_and_cancellation_are_explicit() {
    let backend = backend();
    let audio_format = format(SAMPLE_RATE, SampleFormat::F32Planar, 2);
    let encoder_config = EncoderConfig::audio(STREAM_ID, opus(), audio_format.clone());
    let mut encoder = backend
        .create_encoder(&encoder_config, operation())
        .unwrap();
    let initial_header = drain_encoder(encoder.as_mut()).0;
    assert_eq!(initial_header.len(), 1);
    encoder
        .send(
            EncodeInput::Audio(signal_block(
                SAMPLE_RATE,
                SampleFormat::F32Planar,
                2,
                0,
                960,
            )),
            operation(),
        )
        .unwrap();
    encoder.flush(operation()).unwrap();
    let _ = drain_encoder(encoder.as_mut());
    let error = encoder
        .send(
            EncodeInput::Audio(signal_block(
                SAMPLE_RATE,
                SampleFormat::F32Planar,
                2,
                960,
                960,
            )),
            operation(),
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    encoder.reset(operation()).unwrap();
    assert_eq!(drain_encoder(encoder.as_mut()).0, initial_header);

    let packets = encode(&[signal_block(
        SAMPLE_RATE,
        SampleFormat::F32Planar,
        2,
        11,
        1_920,
    )]);
    let configured_stream = stream_with_header(
        SAMPLE_RATE,
        Timebase::integer(SAMPLE_RATE).unwrap(),
        packets[0].data(),
    );
    let mut decoder = backend
        .create_decoder(&DecoderConfig::new(configured_stream), operation())
        .unwrap();
    for packet in packets.clone() {
        decoder.send_packet(packet, operation()).unwrap();
        while matches!(
            decoder.receive(operation()).unwrap(),
            DecodeOutput::Audio(_)
        ) {}
    }
    decoder.flush(operation()).unwrap();
    decoder.reset(operation()).unwrap();
    let replayed = replay_decoder(decoder.as_mut(), packets);
    assert_eq!(total_frames(&replayed), 1_920);
    assert_eq!(replayed[0].timestamp().sample(), 11);

    let malformed_stream = stream_with_header(
        SAMPLE_RATE,
        Timebase::integer(SAMPLE_RATE).unwrap(),
        b"OpusHead\x01",
    );
    let error = backend
        .create_decoder(&DecoderConfig::new(malformed_stream), operation())
        .err()
        .expect("truncated OpusHead unexpectedly created a decoder");
    assert_eq!(error.category(), ErrorCategory::CorruptData);

    let mut fresh_decoder = backend
        .create_decoder(
            &DecoderConfig::new(stream(SAMPLE_RATE, Timebase::integer(SAMPLE_RATE).unwrap()))
                .with_audio_format(audio_format)
                .unwrap(),
            operation(),
        )
        .unwrap();
    let corrupt_packet = Packet::new(
        STREAM_ID,
        Arc::from(&b"not-an-opus-packet"[..]),
        PacketTiming::new(
            Timebase::integer(SAMPLE_RATE).unwrap(),
            Some(0),
            Some(0),
            None,
        )
        .unwrap(),
    );
    let error = fresh_decoder
        .send_packet(corrupt_packet, operation())
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);

    let unsupported = format(44_100, SampleFormat::F32Planar, 2);
    let error = backend
        .create_encoder(
            &EncoderConfig::audio(STREAM_ID, opus(), unsupported),
            operation(),
        )
        .err()
        .expect("44.1 kHz Opus encoding unexpectedly succeeded");
    assert_eq!(error.category(), ErrorCategory::Unsupported);

    let token = CancellationToken::new();
    token.cancel();
    let cancelled = OperationContext::new(MediaPriority::Interactive).with_cancellation(token);
    let error = backend
        .create_encoder(&encoder_config, &cancelled)
        .err()
        .expect("cancelled Opus encoder creation unexpectedly succeeded");
    assert_eq!(error.category(), ErrorCategory::Cancelled);
}

fn replay_decoder(decoder: &mut dyn Decoder, packets: Vec<Packet>) -> Vec<AudioBlock> {
    let mut blocks = Vec::new();
    for packet in packets {
        decoder.send_packet(packet, operation()).unwrap();
        loop {
            match decoder.receive(operation()).unwrap() {
                DecodeOutput::Audio(block) => blocks.push(block),
                DecodeOutput::NeedInput => break,
                output => panic!("unexpected replay output: {output:?}"),
            }
        }
    }
    blocks
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
    panic!("Opus fixture element is too large");
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

fn ebml_signed(id: &[u8], value: i64) -> Vec<u8> {
    ebml_element(id, value.to_be_bytes())
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
    panic!("Opus fixture VINT is too large");
}

fn opus_mkv_fixture(header: &[u8], packets: &[Packet]) -> Vec<u8> {
    let pre_skip = u64::from(u16::from_le_bytes([header[10], header[11]]));
    let codec_delay_ns = pre_skip * 1_000_000_000 / u64::from(SAMPLE_RATE);
    let padding_samples = match packets
        .last()
        .unwrap()
        .metadata()
        .get("codec.opus.padding-samples")
    {
        Some(MetadataValue::Unsigned(value)) => *value,
        value => panic!("missing Opus end padding: {value:?}"),
    };
    let padding_ns =
        i64::try_from(padding_samples * 1_000_000_000 / u64::from(SAMPLE_RATE)).unwrap();

    let mut ebml = Vec::new();
    ebml.extend(ebml_unsigned(&[0x42, 0x86], 1));
    ebml.extend(ebml_unsigned(&[0x42, 0xF7], 1));
    ebml.extend(ebml_unsigned(&[0x42, 0xF2], 4));
    ebml.extend(ebml_unsigned(&[0x42, 0xF3], 8));
    ebml.extend(ebml_text(&[0x42, 0x82], "matroska"));
    ebml.extend(ebml_unsigned(&[0x42, 0x87], 4));
    ebml.extend(ebml_unsigned(&[0x42, 0x85], 2));

    let mut info = ebml_unsigned(&[0x2A, 0xD7, 0xB1], 1_000_000);
    info.extend(ebml_float(&[0x44, 0x89], 60.0));
    let mut audio = ebml_float(&[0xB5], f64::from(SAMPLE_RATE));
    audio.extend(ebml_unsigned(&[0x9F], 2));
    audio.extend(ebml_unsigned(&[0x62, 0x64], 32));
    let mut track = ebml_unsigned(&[0xD7], 1);
    track.extend(ebml_unsigned(&[0x73, 0xC5], 19));
    track.extend(ebml_unsigned(&[0x83], 2));
    track.extend(ebml_text(&[0x86], "A_OPUS"));
    track.extend(ebml_element(&[0x63, 0xA2], header));
    track.extend(ebml_unsigned(&[0x56, 0xAA], codec_delay_ns));
    track.extend(ebml_unsigned(&[0x56, 0xBB], 80_000_000));
    track.extend(ebml_element(&[0xE1], audio));

    let mut cluster = ebml_unsigned(&[0xE7], 0);
    for (index, packet) in packets.iter().enumerate() {
        let mut payload = ebml_data_vint(1);
        payload.extend_from_slice(&(i16::try_from(index * 20).unwrap()).to_be_bytes());
        payload.push(0x80);
        payload.extend_from_slice(packet.data());
        if index + 1 == packets.len() {
            let mut group = ebml_element(&[0xA1], payload);
            group.extend(ebml_signed(&[0x75, 0xA2], padding_ns));
            cluster.extend(ebml_element(&[0xA0], group));
        } else {
            cluster.extend(ebml_element(&[0xA3], payload));
        }
    }

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
