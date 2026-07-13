use std::f32::consts::TAU;
use std::sync::{Arc, OnceLock};

use superi_codecs_rs::register::register_default_backends;
use superi_core::error::ErrorCategory;
use superi_core::pixel::{ChannelLayout, ChannelPosition, SampleFormat};
use superi_core::time::{SampleTime, TimeRounding, Timebase};
use superi_media_io::audio_io::{AudioBlock, AudioFormat, AudioPlane};
use superi_media_io::backend::{BackendRegistry, BackendRequirement, FallbackPolicy, MediaBackend};
use superi_media_io::decode::{DecodeOutput, DecoderConfig};
use superi_media_io::demux::{
    CodecId, MetadataValue, Packet, PacketTiming, StreamId, StreamInfo, StreamKind,
};
use superi_media_io::encode::{EncodeInput, EncodeOutput, Encoder, EncoderConfig};
use superi_media_io::operation::{CancellationToken, MediaPriority, OperationContext};

const SAMPLE_RATE: u32 = 48_000;
const STREAM_ID: StreamId = StreamId::new(18);

fn operation() -> &'static OperationContext {
    static OPERATION: OnceLock<OperationContext> = OnceLock::new();
    OPERATION.get_or_init(|| OperationContext::new(MediaPriority::Export))
}

fn vorbis() -> CodecId {
    CodecId::new("vorbis").unwrap()
}

fn registry() -> BackendRegistry {
    let mut registry = BackendRegistry::new();
    register_default_backends(&mut registry).unwrap();
    registry
}

fn backend() -> Arc<dyn MediaBackend> {
    registry()
        .select(
            &BackendRequirement::encode(vorbis()),
            FallbackPolicy::Disallow,
        )
        .unwrap()
        .primary()
        .clone()
}

fn stream() -> StreamInfo {
    stream_with_timing(SAMPLE_RATE, Timebase::integer(SAMPLE_RATE).unwrap())
}

fn stream_with_timing(sample_rate: u32, timebase: Timebase) -> StreamInfo {
    StreamInfo::new(STREAM_ID, StreamKind::Audio, vorbis(), timebase)
        .with_metadata(
            "audio.sample-rate",
            MetadataValue::Unsigned(u64::from(sample_rate)),
        )
        .unwrap()
}

fn stereo_signal(start: i64, frames: usize) -> AudioBlock {
    stereo_signal_at_rate(start, frames, SAMPLE_RATE)
}

fn stereo_signal_at_rate(start: i64, frames: usize, sample_rate: u32) -> AudioBlock {
    let format = AudioFormat::new(
        sample_rate,
        SampleFormat::F32Planar,
        ChannelLayout::stereo(),
    )
    .unwrap();
    let channels = [220.0_f32, 440.0_f32]
        .into_iter()
        .map(|frequency| {
            (0..frames)
                .flat_map(|frame| {
                    let sample = 0.35 * (TAU * frequency * frame as f32 / sample_rate as f32).sin();
                    sample.to_le_bytes()
                })
                .collect::<Vec<_>>()
        })
        .map(|bytes| AudioPlane::new(Arc::from(bytes)))
        .collect::<Vec<_>>();
    AudioBlock::new(
        format,
        SampleTime::new(start, sample_rate).unwrap(),
        frames as u64,
        channels,
    )
    .unwrap()
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

fn encode(blocks: &[AudioBlock]) -> Vec<Packet> {
    let backend = backend();
    let config = EncoderConfig::audio(STREAM_ID, vorbis(), blocks[0].format().clone());
    let mut encoder = backend.create_encoder(&config, operation()).unwrap();
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
                DecodeOutput::Frame(_) => panic!("Vorbis decoder returned video"),
                DecodeOutput::EndOfStream => panic!("decoder ended before flush"),
                _ => panic!("unknown decoder output"),
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

fn header_name(packet: &Packet) -> Option<&str> {
    match packet.metadata().get("codec.header") {
        Some(MetadataValue::Text(value)) => Some(value),
        _ => None,
    }
}

fn xiph_laced_headers(headers: &[Packet]) -> Arc<[u8]> {
    assert_eq!(headers.len(), 3);
    let mut private = vec![2];
    for header in &headers[..2] {
        let mut length = header.data().len();
        while length >= 255 {
            private.push(255);
            length -= 255;
        }
        private.push(length as u8);
    }
    for header in headers {
        private.extend_from_slice(header.data());
    }
    Arc::from(private)
}

fn total_frames(blocks: &[AudioBlock]) -> u64 {
    blocks.iter().map(AudioBlock::frame_count).sum()
}

#[test]
fn default_registry_exposes_primary_vorbis_decode_and_encode() {
    let registry = registry();

    for requirement in [
        BackendRequirement::decode(vorbis()),
        BackendRequirement::encode(vorbis()),
    ] {
        let selection = registry
            .select(&requirement, FallbackPolicy::Disallow)
            .unwrap();
        assert_eq!(
            selection.primary().descriptor().id().as_str(),
            "rust-vorbis"
        );
        assert!(!selection.fallback_used());
        assert!(selection.fallbacks().is_empty());
    }
}

#[test]
fn deterministic_raw_packets_round_trip_exact_sample_timing() {
    let blocks = [stereo_signal(2_400, 8_192), stereo_signal(10_592, 8_192)];
    let packets = encode(&blocks);
    assert_eq!(packets, encode(&blocks));

    assert_eq!(packets.len().min(3), 3);
    assert_eq!(header_name(&packets[0]), Some("identification"));
    assert_eq!(header_name(&packets[1]), Some("comment"));
    assert_eq!(header_name(&packets[2]), Some("setup"));
    assert!(packets[0].data().starts_with(b"\x01vorbis"));
    assert!(packets[1].data().starts_with(b"\x03vorbis"));
    assert!(packets[2].data().starts_with(b"\x05vorbis"));

    let audio = &packets[3..];
    assert!(!audio.is_empty());
    assert_eq!(
        audio
            .iter()
            .map(|packet| packet.timing().duration().unwrap().value())
            .sum::<u64>(),
        16_384
    );
    assert_eq!(
        audio.first().unwrap().timing().presentation_time().unwrap(),
        SampleTime::new(2_400, SAMPLE_RATE).unwrap().rational_time()
    );
    let last = audio.last().unwrap();
    let end = last.timing().presentation_time().unwrap().value()
        + last.timing().duration().unwrap().value() as i64;
    assert_eq!(end, 18_784);

    let decoded = decode(DecoderConfig::new(stream()), packets);
    assert_eq!(total_frames(&decoded), 16_384);
    assert_eq!(decoded[0].timestamp().sample(), 2_400);
    assert_eq!(
        decoded.last().unwrap().duration().timebase(),
        Timebase::integer(SAMPLE_RATE).unwrap()
    );
    assert_eq!(decoded[0].format().sample_format(), SampleFormat::F32Planar);
    assert_eq!(
        decoded[0].format().channel_layout(),
        &ChannelLayout::stereo()
    );
    for pair in decoded.windows(2) {
        assert_eq!(
            pair[0].timestamp().sample() + pair[0].frame_count() as i64,
            pair[1].timestamp().sample()
        );
    }

    let decoded_energy = decoded
        .iter()
        .flat_map(|block| block.planes()[0].bytes().chunks_exact(4))
        .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()).abs())
        .sum::<f32>();
    assert!(decoded_energy > 100.0);
}

#[test]
fn matroska_configuration_and_rounded_packet_timing_decode_predictably() {
    let blocks = [stereo_signal(-960, 12_288)];
    let mut packets = encode(&blocks);
    let headers = packets.drain(..3).collect::<Vec<_>>();
    let first_audio = packets
        .iter_mut()
        .find(|packet| {
            packet
                .timing()
                .duration()
                .is_some_and(|duration| !duration.is_zero())
        })
        .unwrap();
    first_audio
        .metadata_mut()
        .insert("source.marker", MetadataValue::Text("preserved".to_owned()))
        .unwrap();

    let configured_stream = stream()
        .with_metadata(
            "codec.configuration",
            MetadataValue::Bytes(xiph_laced_headers(&headers)),
        )
        .unwrap();
    let decoded = decode(DecoderConfig::new(configured_stream), packets);

    assert_eq!(total_frames(&decoded), 12_288);
    assert_eq!(decoded[0].timestamp().sample(), -960);
    assert!(decoded.iter().any(|block| {
        block.metadata().get("source.marker") == Some(&MetadataValue::Text("preserved".to_owned()))
    }));

    const MATROSKA_RATE: u32 = 44_100;
    let mut rounded_packets = encode(&[stereo_signal_at_rate(137, 12_288, MATROSKA_RATE)]);
    let rounded_headers = rounded_packets.drain(..3).collect::<Vec<_>>();
    let rounded_packets = rounded_packets
        .into_iter()
        .map(|packet| {
            let timing = packet.timing();
            let presentation = timing.presentation_time().map(|timestamp| {
                timestamp
                    .checked_rescale(Timebase::NANOSECONDS, TimeRounding::NearestTiesEven)
                    .unwrap()
                    .value()
            });
            let duration = timing.duration().map(|duration| {
                duration
                    .checked_rescale(Timebase::NANOSECONDS, TimeRounding::NearestTiesEven)
                    .unwrap()
                    .value()
            });
            let mut retimed = Packet::new(
                packet.stream_id(),
                Arc::from(packet.data()),
                PacketTiming::new(Timebase::NANOSECONDS, presentation, presentation, duration)
                    .unwrap(),
            )
            .with_keyframe(packet.is_keyframe());
            for (key, value) in packet.metadata().iter() {
                retimed.metadata_mut().insert(key, value.clone()).unwrap();
            }
            retimed
        })
        .collect::<Vec<_>>();
    let rounded_stream = stream_with_timing(MATROSKA_RATE, Timebase::NANOSECONDS)
        .with_metadata(
            "codec.configuration",
            MetadataValue::Bytes(xiph_laced_headers(&rounded_headers)),
        )
        .unwrap();
    let rounded = decode(DecoderConfig::new(rounded_stream), rounded_packets);
    assert_eq!(total_frames(&rounded), 12_288);
    assert_eq!(rounded[0].timestamp().sample(), 137);
    for pair in rounded.windows(2) {
        assert_eq!(
            pair[0].timestamp().sample() + pair[0].frame_count() as i64,
            pair[1].timestamp().sample()
        );
    }
}

#[test]
fn standard_surround_positions_survive_vorbis_channel_order() {
    let vorbis_order = ChannelLayout::new([
        ChannelPosition::FrontLeft,
        ChannelPosition::FrontCenter,
        ChannelPosition::FrontRight,
        ChannelPosition::BackLeft,
        ChannelPosition::BackRight,
        ChannelPosition::LowFrequency,
    ])
    .unwrap();
    let format = AudioFormat::new(SAMPLE_RATE, SampleFormat::F32Planar, vorbis_order).unwrap();
    let frames = 16_384;
    let planes = [0.05_f32, 0.10, 0.15, 0.20, 0.25, 0.30]
        .into_iter()
        .map(|amplitude| {
            let bytes = (0..frames)
                .flat_map(|frame| {
                    let sample = amplitude * (TAU * 80.0 * frame as f32 / SAMPLE_RATE as f32).sin();
                    sample.to_le_bytes()
                })
                .collect::<Vec<_>>();
            AudioPlane::new(Arc::from(bytes))
        })
        .collect::<Vec<_>>();
    let block = AudioBlock::new(
        format,
        SampleTime::new(0, SAMPLE_RATE).unwrap(),
        frames as u64,
        planes,
    )
    .unwrap();

    let decoded = decode(DecoderConfig::new(stream()), encode(&[block]));
    assert_eq!(
        decoded[0].format().channel_layout(),
        &ChannelLayout::surround_5_1()
    );

    let mut squares = vec![0.0_f64; 6];
    let mut samples = 0_u64;
    for block in &decoded {
        samples += block.frame_count();
        for (channel, plane) in block.planes().iter().enumerate() {
            squares[channel] += plane
                .bytes()
                .chunks_exact(4)
                .map(|bytes| f32::from_le_bytes(bytes.try_into().unwrap()) as f64)
                .map(|sample| sample * sample)
                .sum::<f64>();
        }
    }
    let rms = squares
        .into_iter()
        .map(|sum| (sum / samples as f64).sqrt())
        .collect::<Vec<_>>();
    let canonical_amplitudes = [0.05_f64, 0.15, 0.10, 0.30, 0.20, 0.25];
    for (index, (actual, amplitude)) in rms.iter().zip(canonical_amplitudes).enumerate() {
        if index == 3 {
            assert!(
                *actual
                    > rms
                        .iter()
                        .copied()
                        .filter(|value| value != actual)
                        .fold(0.0, f64::max),
                "LFE channel moved from its canonical position: {rms:?}"
            );
        } else {
            let expected = amplitude / 2.0_f64.sqrt();
            assert!(
                (actual - expected).abs() < 0.02,
                "channel {index} energy changed position: {rms:?}"
            );
        }
    }
}

#[test]
fn encoder_accepts_every_public_integer_float_and_storage_representation() {
    for sample_format in SampleFormat::ALL {
        let format = AudioFormat::new(SAMPLE_RATE, *sample_format, ChannelLayout::mono()).unwrap();
        let samples = (0..4_096)
            .map(|frame| 0.25 * (TAU * 330.0 * frame as f32 / SAMPLE_RATE as f32).sin())
            .collect::<Vec<_>>();
        let bytes = samples
            .iter()
            .flat_map(|sample| encoded_sample(*sample, *sample_format))
            .collect::<Vec<_>>();
        let block = AudioBlock::new(
            format,
            SampleTime::new(0, SAMPLE_RATE).unwrap(),
            samples.len() as u64,
            vec![AudioPlane::new(Arc::from(bytes))],
        )
        .unwrap();

        let packets = encode(&[block]);
        assert!(packets.len() > 3, "no audio packets for {sample_format:?}");
        assert_eq!(
            packets[3..]
                .iter()
                .map(|packet| packet.timing().duration().unwrap().value())
                .sum::<u64>(),
            samples.len() as u64,
            "wrong duration for {sample_format:?}"
        );
    }
}

#[test]
fn lifecycle_corruption_and_cancellation_are_explicit() {
    let backend = backend();
    let format = AudioFormat::new(
        SAMPLE_RATE,
        SampleFormat::F32Planar,
        ChannelLayout::stereo(),
    )
    .unwrap();
    let encoder_config = EncoderConfig::audio(STREAM_ID, vorbis(), format);
    let mut encoder = backend
        .create_encoder(&encoder_config, operation())
        .unwrap();
    let initial_headers = drain_encoder(encoder.as_mut()).0;
    assert_eq!(initial_headers.len(), 3);
    encoder.flush(operation()).unwrap();
    let _ = drain_encoder(encoder.as_mut());
    let error = encoder
        .send(EncodeInput::Audio(stereo_signal(0, 1_024)), operation())
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    encoder.reset(operation()).unwrap();
    assert_eq!(drain_encoder(encoder.as_mut()).0, initial_headers);

    let packets = encode(&[stereo_signal(768, 8_192)]);
    let mut resettable_decoder = backend
        .create_decoder(&DecoderConfig::new(stream()), operation())
        .unwrap();
    for packet in packets.clone() {
        resettable_decoder.send_packet(packet, operation()).unwrap();
        while matches!(
            resettable_decoder.receive(operation()).unwrap(),
            DecodeOutput::Audio(_)
        ) {}
    }
    resettable_decoder.flush(operation()).unwrap();
    assert!(matches!(
        resettable_decoder.receive(operation()).unwrap(),
        DecodeOutput::EndOfStream
    ));
    resettable_decoder.reset(operation()).unwrap();
    let mut replayed = Vec::new();
    for packet in packets {
        resettable_decoder.send_packet(packet, operation()).unwrap();
        loop {
            match resettable_decoder.receive(operation()).unwrap() {
                DecodeOutput::Audio(block) => replayed.push(block),
                DecodeOutput::NeedInput => break,
                output => panic!("unexpected decoder output after reset: {output:?}"),
            }
        }
    }
    assert_eq!(total_frames(&replayed), 8_192);
    assert_eq!(replayed[0].timestamp().sample(), 768);

    let mut decoder = backend
        .create_decoder(&DecoderConfig::new(stream()), operation())
        .unwrap();
    let malformed = Packet::new(
        STREAM_ID,
        Arc::from(&b"\x01not-vorbis"[..]),
        PacketTiming::new(Timebase::integer(SAMPLE_RATE).unwrap(), None, None, None).unwrap(),
    );
    let error = decoder.send_packet(malformed, operation()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);

    let wrong_stream = Packet::new(
        StreamId::new(99),
        Arc::from(&b"\x01vorbis"[..]),
        PacketTiming::new(Timebase::integer(SAMPLE_RATE).unwrap(), None, None, None).unwrap(),
    );
    let error = decoder.send_packet(wrong_stream, operation()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);

    let token = CancellationToken::new();
    token.cancel();
    let cancelled = OperationContext::new(MediaPriority::Interactive).with_cancellation(token);
    let error = backend
        .create_encoder(&encoder_config, &cancelled)
        .err()
        .expect("cancelled encoder creation unexpectedly succeeded");
    assert_eq!(error.category(), ErrorCategory::Cancelled);

    let undefined_layout = ChannelLayout::new([
        ChannelPosition::FrontLeft,
        ChannelPosition::FrontRight,
        ChannelPosition::FrontCenter,
        ChannelPosition::LowFrequency,
        ChannelPosition::BackLeft,
        ChannelPosition::BackRight,
        ChannelPosition::SideLeft,
        ChannelPosition::SideRight,
        ChannelPosition::TopCenter,
    ])
    .unwrap();
    let undefined_format =
        AudioFormat::new(SAMPLE_RATE, SampleFormat::F32Planar, undefined_layout).unwrap();
    let error = backend
        .create_encoder(
            &EncoderConfig::audio(STREAM_ID, vorbis(), undefined_format),
            operation(),
        )
        .err()
        .expect("undefined nine-channel mapping unexpectedly succeeded");
    assert_eq!(error.category(), ErrorCategory::Unsupported);
}

fn encoded_sample(sample: f32, format: SampleFormat) -> Vec<u8> {
    match format {
        SampleFormat::U8 | SampleFormat::U8Planar => {
            vec![((sample.clamp(-1.0, 1.0) * 128.0) + 128.0)
                .round()
                .clamp(0.0, 255.0) as u8]
        }
        SampleFormat::I16 | SampleFormat::I16Planar => ((sample.clamp(-1.0, 1.0) * 32_768.0)
            .round()
            .clamp(i16::MIN as f32, i16::MAX as f32)
            as i16)
            .to_le_bytes()
            .to_vec(),
        SampleFormat::I24 | SampleFormat::I24Planar => {
            let value = (sample.clamp(-1.0, 1.0) * 8_388_608.0)
                .round()
                .clamp(-8_388_608.0, 8_388_607.0) as i32;
            value.to_le_bytes()[..3].to_vec()
        }
        SampleFormat::I32 | SampleFormat::I32Planar => {
            ((f64::from(sample.clamp(-1.0, 1.0)) * 2_147_483_648.0)
                .round()
                .clamp(i32::MIN as f64, i32::MAX as f64) as i32)
                .to_le_bytes()
                .to_vec()
        }
        SampleFormat::F32 | SampleFormat::F32Planar => sample.to_le_bytes().to_vec(),
        SampleFormat::F64 | SampleFormat::F64Planar => f64::from(sample).to_le_bytes().to_vec(),
        _ => panic!("unsupported test sample format"),
    }
}
