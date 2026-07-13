use std::sync::{Arc, OnceLock};

use superi_codecs_rs::mp3::Mp3Backend;
use superi_codecs_rs::register::register_default_backends;
use superi_core::error::ErrorCategory;
use superi_core::pixel::{ChannelLayout, SampleFormat};
use superi_core::time::{SampleTime, Timebase};
use superi_media_io::audio_io::{AudioBlock, AudioFormat, AudioPlane};
use superi_media_io::backend::{BackendRegistry, BackendRequirement, FallbackPolicy, MediaBackend};
use superi_media_io::decode::{DecodeOutput, DecoderConfig};
use superi_media_io::demux::{
    MetadataValue, Packet, PacketTiming, StreamId, StreamInfo, StreamKind,
};
use superi_media_io::encode::{EncodeInput, EncodeOutput, EncoderConfig};
use superi_media_io::operation::{CancellationToken, MediaPriority, OperationContext};

fn operation() -> &'static OperationContext {
    static OPERATION: OnceLock<OperationContext> = OnceLock::new();
    OPERATION.get_or_init(|| OperationContext::new(MediaPriority::Interactive))
}

fn mp3_backend() -> Arc<dyn MediaBackend> {
    let mut registry = BackendRegistry::new();
    register_default_backends(&mut registry).unwrap();
    registry
        .select(
            &BackendRequirement::encode(Mp3Backend::codec_id()),
            FallbackPolicy::Disallow,
        )
        .unwrap()
        .primary()
        .clone()
}

fn stream(stream_id: StreamId, sample_rate: u32) -> StreamInfo {
    StreamInfo::new(
        stream_id,
        StreamKind::Audio,
        Mp3Backend::codec_id(),
        Timebase::integer(sample_rate).unwrap(),
    )
}

fn sine_block(
    format: AudioFormat,
    timestamp: i64,
    frames: usize,
    frequencies: &[f32],
    metadata: &str,
) -> AudioBlock {
    let channels = format.channel_layout().len();
    assert_eq!(channels, frequencies.len());
    let sample = |frame: usize, channel: usize| {
        let phase = 2.0 * std::f32::consts::PI * frequencies[channel] * frame as f32
            / format.sample_rate() as f32;
        (phase.sin() * 20_000.0).round() as i16
    };
    let planes = if format.sample_format().is_planar() {
        (0..channels)
            .map(|channel| {
                let bytes = (0..frames)
                    .flat_map(|frame| sample(frame, channel).to_le_bytes())
                    .collect::<Vec<_>>();
                AudioPlane::new(Arc::from(bytes))
            })
            .collect()
    } else {
        let bytes = (0..frames)
            .flat_map(|frame| {
                (0..channels).flat_map(move |channel| sample(frame, channel).to_le_bytes())
            })
            .collect::<Vec<_>>();
        vec![AudioPlane::new(Arc::from(bytes))]
    };
    AudioBlock::new(
        format.clone(),
        SampleTime::new(timestamp, format.sample_rate()).unwrap(),
        frames as u64,
        planes,
    )
    .unwrap()
    .with_metadata("mp3.origin", MetadataValue::Text(metadata.into()))
    .unwrap()
}

fn encode_block(backend: &dyn MediaBackend, stream_id: StreamId, block: AudioBlock) -> Vec<Packet> {
    let config = EncoderConfig::audio(stream_id, Mp3Backend::codec_id(), block.format().clone());
    let mut encoder = backend.create_encoder(&config, operation()).unwrap();
    assert_eq!(
        encoder.receive(operation()).unwrap(),
        EncodeOutput::NeedInput
    );
    encoder
        .send(EncodeInput::Audio(block), operation())
        .unwrap();
    assert_eq!(
        encoder.receive(operation()).unwrap(),
        EncodeOutput::NeedInput
    );
    encoder.flush(operation()).unwrap();
    let mut packets = Vec::new();
    loop {
        match encoder.receive(operation()).unwrap() {
            EncodeOutput::Packet(packet) => packets.push(packet),
            EncodeOutput::EndOfStream => break,
            EncodeOutput::NeedInput => panic!("flushed MP3 encoder requested more input"),
            _ => panic!("MP3 encoder returned an unknown output"),
        }
    }
    packets
}

fn decode_packets(
    backend: &dyn MediaBackend,
    stream_id: StreamId,
    format: AudioFormat,
    packets: Vec<Packet>,
) -> Vec<AudioBlock> {
    let config = DecoderConfig::new(stream(stream_id, format.sample_rate()))
        .with_audio_format(format)
        .unwrap();
    let mut decoder = backend.create_decoder(&config, operation()).unwrap();
    let mut blocks = Vec::new();
    for packet in packets {
        decoder.send_packet(packet, operation()).unwrap();
        loop {
            match decoder.receive(operation()).unwrap() {
                DecodeOutput::Audio(block) => blocks.push(block),
                DecodeOutput::NeedInput => break,
                DecodeOutput::EndOfStream => panic!("MP3 decoder ended before flush"),
                DecodeOutput::Frame(_) => panic!("MP3 decoder returned video"),
                _ => panic!("MP3 decoder returned an unknown output"),
            }
        }
    }
    decoder.flush(operation()).unwrap();
    loop {
        match decoder.receive(operation()).unwrap() {
            DecodeOutput::Audio(block) => blocks.push(block),
            DecodeOutput::EndOfStream => break,
            DecodeOutput::NeedInput => panic!("flushed MP3 decoder requested more input"),
            DecodeOutput::Frame(_) => panic!("MP3 decoder returned video"),
            _ => panic!("MP3 decoder returned an unknown output"),
        }
    }
    blocks
}

fn drain_encoder(encoder: &mut dyn superi_media_io::encode::Encoder) -> Vec<Packet> {
    encoder.flush(operation()).unwrap();
    let mut packets = Vec::new();
    loop {
        match encoder.receive(operation()).unwrap() {
            EncodeOutput::Packet(packet) => packets.push(packet),
            EncodeOutput::EndOfStream => return packets,
            EncodeOutput::NeedInput => panic!("flushed MP3 encoder requested more input"),
            _ => panic!("MP3 encoder returned an unknown output"),
        }
    }
}

#[test]
fn default_registry_exposes_deterministic_mp3_decode_and_encode() {
    let mut registry = BackendRegistry::new();
    register_default_backends(&mut registry).unwrap();

    for requirement in [
        BackendRequirement::decode(Mp3Backend::codec_id()),
        BackendRequirement::encode(Mp3Backend::codec_id()),
    ] {
        let selection = registry
            .select(&requirement, FallbackPolicy::Disallow)
            .unwrap();
        assert_eq!(selection.primary().descriptor().id().as_str(), "rust-mp3");
        assert!(!selection.fallback_used());
        assert!(selection.fallbacks().is_empty());
    }
}

#[test]
fn mono_round_trip_preserves_sample_timing_and_metadata() {
    let backend = mp3_backend();
    let stream_id = StreamId::new(16);
    let format = AudioFormat::new(44_100, SampleFormat::I16, ChannelLayout::mono()).unwrap();
    let start = 8_820;
    let packets = encode_block(
        backend.as_ref(),
        stream_id,
        sine_block(format.clone(), start, 2_304, &[440.0], "mono"),
    );

    assert!(!packets.is_empty());
    assert_eq!(
        packets[0].timing().presentation_time().unwrap(),
        SampleTime::new(start, 44_100).unwrap().rational_time()
    );
    assert_eq!(packets[0].timing().duration().unwrap().value(), 1_152);
    assert_eq!(
        packets[0].metadata().get("mp3.origin"),
        Some(&MetadataValue::Text("mono".into()))
    );
    assert!(packets[0].is_keyframe());

    let blocks = decode_packets(backend.as_ref(), stream_id, format.clone(), packets);
    assert!(!blocks.is_empty());
    assert_eq!(blocks[0].format(), &format);
    assert_eq!(blocks[0].timestamp().sample(), start);
    assert_eq!(
        blocks[0].metadata().get("mp3.origin"),
        Some(&MetadataValue::Text("mono".into()))
    );
    assert!(blocks
        .iter()
        .flat_map(|block| block.planes()[0].bytes().chunks_exact(2))
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
        .any(|sample| sample != 0));
}

#[test]
fn delayed_encode_assigns_metadata_by_packet_sample_range() {
    let backend = mp3_backend();
    let stream_id = StreamId::new(21);
    let format = AudioFormat::new(44_100, SampleFormat::I16, ChannelLayout::mono()).unwrap();
    let config = EncoderConfig::audio(stream_id, Mp3Backend::codec_id(), format.clone());
    let mut encoder = backend.create_encoder(&config, operation()).unwrap();
    for (start, label, frequency) in [(0, "first", 440.0), (1_152, "second", 880.0)] {
        encoder
            .send(
                EncodeInput::Audio(sine_block(
                    format.clone(),
                    start,
                    1_152,
                    &[frequency],
                    label,
                )),
                operation(),
            )
            .unwrap();
    }
    let packets = drain_encoder(encoder.as_mut());
    assert!(packets.len() >= 2);
    assert_eq!(
        packets[0].metadata().get("mp3.origin"),
        Some(&MetadataValue::Text("first".into()))
    );
    assert_eq!(
        packets[1].metadata().get("mp3.origin"),
        Some(&MetadataValue::Text("second".into()))
    );
}

#[test]
fn stereo_planar_round_trip_retains_channel_layout() {
    let backend = mp3_backend();
    let stream_id = StreamId::new(17);
    let format =
        AudioFormat::new(48_000, SampleFormat::I16Planar, ChannelLayout::stereo()).unwrap();
    let packets = encode_block(
        backend.as_ref(),
        stream_id,
        sine_block(format.clone(), -1_152, 2_304, &[330.0, 660.0], "stereo"),
    );
    let blocks = decode_packets(backend.as_ref(), stream_id, format.clone(), packets);

    assert!(!blocks.is_empty());
    assert!(blocks.iter().all(|block| block.format() == &format));
    assert!(blocks.iter().all(|block| block.planes().len() == 2));
    assert!(blocks
        .iter()
        .any(|block| block.planes()[0].bytes() != block.planes()[1].bytes()));
}

#[test]
fn mpeg1_mpeg2_and_mpeg25_rate_families_encode_and_decode() {
    let backend = mp3_backend();
    for (index, (sample_rate, samples_per_packet)) in [(8_000, 576), (16_000, 576), (32_000, 1_152)]
        .into_iter()
        .enumerate()
    {
        let stream_id = StreamId::new(30 + index as u32);
        let format =
            AudioFormat::new(sample_rate, SampleFormat::I16, ChannelLayout::mono()).unwrap();
        let packets = encode_block(
            backend.as_ref(),
            stream_id,
            sine_block(
                format.clone(),
                0,
                samples_per_packet * 2,
                &[sample_rate as f32 / 20.0],
                "rate-family",
            ),
        );
        assert!(!packets.is_empty());
        assert_eq!(
            packets[0].timing().duration().unwrap().value(),
            samples_per_packet as u64
        );
        assert!(!decode_packets(backend.as_ref(), stream_id, format, packets).is_empty());
    }
}

#[test]
fn decoder_rescales_exact_container_timing() {
    let backend = mp3_backend();
    let stream_id = StreamId::new(19);
    let format = AudioFormat::new(48_000, SampleFormat::I16, ChannelLayout::mono()).unwrap();
    let packets = encode_block(
        backend.as_ref(),
        stream_id,
        sine_block(format.clone(), 9_600, 2_304, &[220.0], "rescaled"),
    );
    let container_timebase = Timebase::integer(96_000).unwrap();
    let packets = packets
        .into_iter()
        .map(|packet| {
            let pts = packet.timing().presentation_time().unwrap().value() * 2;
            let duration = packet.timing().duration().unwrap().value() * 2;
            let mut rescaled = Packet::new(
                stream_id,
                Arc::from(packet.data()),
                PacketTiming::new(container_timebase, Some(pts), Some(pts), Some(duration))
                    .unwrap(),
            )
            .with_keyframe(packet.is_keyframe());
            for (key, value) in packet.metadata().iter() {
                rescaled.metadata_mut().insert(key, value.clone()).unwrap();
            }
            rescaled
        })
        .collect::<Vec<_>>();
    let config = DecoderConfig::new(StreamInfo::new(
        stream_id,
        StreamKind::Audio,
        Mp3Backend::codec_id(),
        container_timebase,
    ))
    .with_audio_format(format)
    .unwrap();
    let mut decoder = backend.create_decoder(&config, operation()).unwrap();
    decoder
        .send_packet(packets[0].clone(), operation())
        .unwrap();
    let DecodeOutput::Audio(block) = decoder.receive(operation()).unwrap() else {
        panic!("MP3 decoder did not return exact rescaled audio");
    };
    assert_eq!(block.timestamp().sample(), 9_600);
    assert_eq!(block.frame_count(), 1_152);

    let fractional_timebase = Timebase::integer(90_000).unwrap();
    let fractional_config = DecoderConfig::new(StreamInfo::new(
        stream_id,
        StreamKind::Audio,
        Mp3Backend::codec_id(),
        fractional_timebase,
    ))
    .with_audio_format(AudioFormat::new(48_000, SampleFormat::I16, ChannelLayout::mono()).unwrap())
    .unwrap();
    let mut decoder = backend
        .create_decoder(&fractional_config, operation())
        .unwrap();
    let fractional = Packet::new(
        stream_id,
        Arc::from(packets[0].data()),
        PacketTiming::new(fractional_timebase, Some(1), Some(1), None).unwrap(),
    );
    assert_eq!(
        decoder
            .send_packet(fractional, operation())
            .unwrap_err()
            .category(),
        ErrorCategory::CorruptData
    );
}

#[test]
fn reset_starts_fresh_deterministic_encoder_and_decoder_lifetimes() {
    let backend = mp3_backend();
    let stream_id = StreamId::new(20);
    let format = AudioFormat::new(44_100, SampleFormat::I16, ChannelLayout::mono()).unwrap();
    let block = sine_block(format.clone(), 441, 2_304, &[550.0], "reset");
    let encoder_config = EncoderConfig::audio(stream_id, Mp3Backend::codec_id(), format.clone());
    let mut encoder = backend
        .create_encoder(&encoder_config, operation())
        .unwrap();
    encoder
        .send(EncodeInput::Audio(block.clone()), operation())
        .unwrap();
    let first_packets = drain_encoder(encoder.as_mut());
    assert_eq!(
        encoder
            .send(EncodeInput::Audio(block.clone()), operation())
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );
    encoder.reset(operation()).unwrap();
    encoder
        .send(EncodeInput::Audio(block), operation())
        .unwrap();
    let second_packets = drain_encoder(encoder.as_mut());
    assert_eq!(first_packets, second_packets);

    let decoder_config = DecoderConfig::new(stream(stream_id, 44_100))
        .with_audio_format(format)
        .unwrap();
    let mut decoder = backend
        .create_decoder(&decoder_config, operation())
        .unwrap();
    decoder
        .send_packet(first_packets[0].clone(), operation())
        .unwrap();
    let DecodeOutput::Audio(first_block) = decoder.receive(operation()).unwrap() else {
        panic!("MP3 decoder did not return audio before reset");
    };
    decoder.reset(operation()).unwrap();
    decoder
        .send_packet(first_packets[0].clone(), operation())
        .unwrap();
    let DecodeOutput::Audio(second_block) = decoder.receive(operation()).unwrap() else {
        panic!("MP3 decoder did not return audio after reset");
    };
    assert_eq!(first_block, second_block);
}

#[test]
fn mp3_rejects_incompatible_or_corrupt_input_and_honors_cancellation() {
    let backend = mp3_backend();
    let stream_id = StreamId::new(18);
    let float_format = AudioFormat::new(44_100, SampleFormat::F32, ChannelLayout::mono()).unwrap();
    let error = backend
        .create_encoder(
            &EncoderConfig::audio(stream_id, Mp3Backend::codec_id(), float_format),
            operation(),
        )
        .err()
        .expect("floating-point MP3 input must be rejected");
    assert_eq!(error.category(), ErrorCategory::Unsupported);

    let format = AudioFormat::new(44_100, SampleFormat::I16, ChannelLayout::mono()).unwrap();
    let config = DecoderConfig::new(stream(stream_id, 44_100))
        .with_audio_format(format)
        .unwrap();
    let mut decoder = backend.create_decoder(&config, operation()).unwrap();
    let corrupt = Packet::new(
        stream_id,
        Arc::from([0, 0, 0, 0]),
        PacketTiming::new(Timebase::integer(44_100).unwrap(), Some(0), Some(0), None).unwrap(),
    );
    let error = decoder.send_packet(corrupt, operation()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);

    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let cancelled =
        OperationContext::new(MediaPriority::Interactive).with_cancellation(cancellation);
    let error = backend.create_decoder(&config, &cancelled).err().unwrap();
    assert_eq!(error.category(), ErrorCategory::Cancelled);
}
