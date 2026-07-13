use std::sync::{Arc, OnceLock};

use superi_codecs_rs::pcm::PcmEncoding;
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
use superi_media_io::operation::{MediaPriority, OperationContext};

fn operation() -> &'static OperationContext {
    static OPERATION: OnceLock<OperationContext> = OnceLock::new();
    OPERATION.get_or_init(|| OperationContext::new(MediaPriority::Interactive))
}

fn pcm_backend() -> Arc<dyn MediaBackend> {
    let mut registry = BackendRegistry::new();
    register_default_backends(&mut registry).unwrap();
    registry
        .select(
            &BackendRequirement::decode(PcmEncoding::I16LittleEndian.codec_id()),
            FallbackPolicy::Disallow,
        )
        .unwrap()
        .primary()
        .clone()
}

fn stream(encoding: PcmEncoding, stream_id: StreamId, sample_rate: u32) -> StreamInfo {
    StreamInfo::new(
        stream_id,
        StreamKind::Audio,
        encoding.codec_id(),
        Timebase::integer(sample_rate).unwrap(),
    )
}

#[test]
fn default_backend_registers_every_pcm_decode_and_encode_capability() {
    let mut registry = BackendRegistry::new();
    register_default_backends(&mut registry).unwrap();

    for encoding in PcmEncoding::ALL {
        for requirement in [
            BackendRequirement::decode(encoding.codec_id()),
            BackendRequirement::encode(encoding.codec_id()),
        ] {
            let selection = registry
                .select(&requirement, FallbackPolicy::Disallow)
                .unwrap();
            assert_eq!(selection.primary().descriptor().id().as_str(), "rust-pcm");
            assert!(!selection.fallback_used());
            assert!(selection.fallbacks().is_empty());
        }
    }
}

#[test]
fn pcm_round_trips_every_explicit_integer_and_float_representation() {
    let backend = pcm_backend();
    let stream_id = StreamId::new(4);

    for encoding in PcmEncoding::ALL
        .iter()
        .copied()
        .filter(|encoding| *encoding != PcmEncoding::Canonical)
    {
        let sample_format = encoding.sample_format().unwrap();
        let format = AudioFormat::new(48_000, sample_format, ChannelLayout::stereo()).unwrap();
        let byte_count = 4 * usize::from(sample_format.bytes_per_sample());
        let bytes = (0..byte_count)
            .map(|index| (index as u8).wrapping_mul(29).wrapping_add(7))
            .collect::<Vec<_>>();
        let block = AudioBlock::new(
            format.clone(),
            SampleTime::new(120, 48_000).unwrap(),
            2,
            vec![AudioPlane::new(Arc::from(bytes))],
        )
        .unwrap()
        .with_metadata("pcm.origin", MetadataValue::Text("contract".into()))
        .unwrap();

        let encoder_config = EncoderConfig::audio(stream_id, encoding.codec_id(), format.clone());
        let mut encoder = backend
            .create_encoder(&encoder_config, operation())
            .unwrap();
        assert_eq!(
            encoder.receive(operation()).unwrap(),
            EncodeOutput::NeedInput
        );
        encoder
            .send(EncodeInput::Audio(block.clone()), operation())
            .unwrap();
        let EncodeOutput::Packet(packet) = encoder.receive(operation()).unwrap() else {
            panic!("PCM encoder did not return a packet");
        };
        assert!(packet.is_keyframe());
        assert_eq!(
            packet.timing().presentation_time().unwrap(),
            SampleTime::new(120, 48_000).unwrap().rational_time()
        );
        assert_eq!(packet.timing().duration().unwrap(), block.duration());

        let decoder_config = DecoderConfig::new(stream(encoding, stream_id, 48_000))
            .with_audio_format(format)
            .unwrap();
        let mut decoder = backend
            .create_decoder(&decoder_config, operation())
            .unwrap();
        decoder.send_packet(packet, operation()).unwrap();
        let DecodeOutput::Audio(decoded) = decoder.receive(operation()).unwrap() else {
            panic!("PCM decoder did not return an audio block");
        };
        assert_eq!(decoded, block, "failed round trip for {encoding:?}");
    }
}

#[test]
fn big_endian_pcm_interleaves_and_restores_planar_channels_without_precision_loss() {
    let backend = pcm_backend();
    let stream_id = StreamId::new(9);
    let format =
        AudioFormat::new(96_000, SampleFormat::I16Planar, ChannelLayout::stereo()).unwrap();
    let block = AudioBlock::new(
        format.clone(),
        SampleTime::new(-2, 96_000).unwrap(),
        2,
        vec![
            AudioPlane::new(Arc::from([0x01, 0x00, 0x02, 0x00])),
            AudioPlane::new(Arc::from([0x03, 0x00, 0x04, 0x00])),
        ],
    )
    .unwrap();
    let encoding = PcmEncoding::I16BigEndian;
    let encoder_config = EncoderConfig::audio(stream_id, encoding.codec_id(), format.clone());
    let mut encoder = backend
        .create_encoder(&encoder_config, operation())
        .unwrap();
    encoder
        .send(EncodeInput::Audio(block.clone()), operation())
        .unwrap();
    let EncodeOutput::Packet(packet) = encoder.receive(operation()).unwrap() else {
        panic!("PCM encoder did not return a packet");
    };
    assert_eq!(
        packet.data(),
        [0x00, 0x01, 0x00, 0x03, 0x00, 0x02, 0x00, 0x04]
    );

    let decoder_config = DecoderConfig::new(stream(encoding, stream_id, 96_000))
        .with_audio_format(format)
        .unwrap();
    let mut decoder = backend
        .create_decoder(&decoder_config, operation())
        .unwrap();
    decoder.send_packet(packet, operation()).unwrap();
    let DecodeOutput::Audio(decoded) = decoder.receive(operation()).unwrap() else {
        panic!("PCM decoder did not return an audio block");
    };
    assert_eq!(decoded, block);
}

#[test]
fn canonical_pcm_preserves_planar_storage_including_empty_blocks() {
    let backend = pcm_backend();
    let stream_id = StreamId::new(12);
    let encoding = PcmEncoding::Canonical;
    let format =
        AudioFormat::new(48_000, SampleFormat::F32Planar, ChannelLayout::stereo()).unwrap();
    let config = EncoderConfig::audio(stream_id, encoding.codec_id(), format.clone());
    let mut encoder = backend.create_encoder(&config, operation()).unwrap();

    for block in [
        AudioBlock::new(
            format.clone(),
            SampleTime::new(5, 48_000).unwrap(),
            1,
            vec![
                AudioPlane::new(Arc::from([1, 2, 3, 4])),
                AudioPlane::new(Arc::from([5, 6, 7, 8])),
            ],
        )
        .unwrap(),
        AudioBlock::new(
            format.clone(),
            SampleTime::new(6, 48_000).unwrap(),
            0,
            vec![
                AudioPlane::new(Arc::from([])),
                AudioPlane::new(Arc::from([])),
            ],
        )
        .unwrap(),
    ] {
        encoder
            .send(EncodeInput::Audio(block.clone()), operation())
            .unwrap();
        let EncodeOutput::Packet(packet) = encoder.receive(operation()).unwrap() else {
            panic!("PCM encoder did not return a packet");
        };
        let decoder_config = DecoderConfig::new(stream(encoding, stream_id, 48_000))
            .with_audio_format(format.clone())
            .unwrap();
        let mut decoder = backend
            .create_decoder(&decoder_config, operation())
            .unwrap();
        decoder.send_packet(packet, operation()).unwrap();
        let DecodeOutput::Audio(decoded) = decoder.receive(operation()).unwrap() else {
            panic!("PCM decoder did not return an audio block");
        };
        assert_eq!(decoded, block);
    }
}

#[test]
fn decoder_infers_contiguous_sample_timing_and_reset_restores_the_lifecycle() {
    let backend = pcm_backend();
    let stream_id = StreamId::new(2);
    let encoding = PcmEncoding::I16LittleEndian;
    let format = AudioFormat::new(44_100, SampleFormat::I16, ChannelLayout::stereo()).unwrap();
    let config = DecoderConfig::new(stream(encoding, stream_id, 44_100))
        .with_audio_format(format)
        .unwrap();
    let mut decoder = backend.create_decoder(&config, operation()).unwrap();

    assert!(matches!(
        decoder.receive(operation()).unwrap(),
        DecodeOutput::NeedInput
    ));
    for expected_timestamp in [0, 1] {
        let packet = Packet::new(
            stream_id,
            Arc::from([1, 0, 2, 0]),
            PacketTiming::new(Timebase::integer(44_100).unwrap(), None, None, None).unwrap(),
        );
        decoder.send_packet(packet, operation()).unwrap();
        let DecodeOutput::Audio(block) = decoder.receive(operation()).unwrap() else {
            panic!("PCM decoder did not return an audio block");
        };
        assert_eq!(block.timestamp().sample(), expected_timestamp);
        assert_eq!(block.frame_count(), 1);
    }

    decoder.flush(operation()).unwrap();
    assert!(matches!(
        decoder.receive(operation()).unwrap(),
        DecodeOutput::EndOfStream
    ));
    let error = decoder
        .send_packet(
            Packet::new(
                stream_id,
                Arc::from([1, 0, 2, 0]),
                PacketTiming::new(Timebase::integer(44_100).unwrap(), None, None, None).unwrap(),
            ),
            operation(),
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);

    decoder.reset(operation()).unwrap();
    assert!(matches!(
        decoder.receive(operation()).unwrap(),
        DecodeOutput::NeedInput
    ));
}

#[test]
fn decoder_reports_partial_sample_frames_as_corrupt_data() {
    let backend = pcm_backend();
    let stream_id = StreamId::new(3);
    let encoding = PcmEncoding::I24LittleEndian;
    let format = AudioFormat::new(48_000, SampleFormat::I24, ChannelLayout::stereo()).unwrap();
    let config = DecoderConfig::new(stream(encoding, stream_id, 48_000))
        .with_audio_format(format)
        .unwrap();
    let mut decoder = backend.create_decoder(&config, operation()).unwrap();
    let packet = Packet::new(
        stream_id,
        Arc::from([1, 2, 3, 4, 5]),
        PacketTiming::new(Timebase::integer(48_000).unwrap(), Some(0), Some(0), None).unwrap(),
    );

    let error = decoder.send_packet(packet, operation()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);
    assert_eq!(
        error.contexts().last().unwrap().operation(),
        "decode_pcm_packet"
    );
}

#[test]
fn pcm_factories_reject_incompatible_configs_and_honor_cancellation() {
    let backend = pcm_backend();
    let stream_id = StreamId::new(15);
    let i16_format = AudioFormat::new(48_000, SampleFormat::I16, ChannelLayout::stereo()).unwrap();
    let f32_format = AudioFormat::new(48_000, SampleFormat::F32, ChannelLayout::stereo()).unwrap();

    let missing_format =
        DecoderConfig::new(stream(PcmEncoding::I16LittleEndian, stream_id, 48_000));
    let error = match backend.create_decoder(&missing_format, operation()) {
        Ok(_) => panic!("PCM decoding without an audio format must fail"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let video_stream = StreamInfo::new(
        stream_id,
        StreamKind::Video,
        PcmEncoding::I16LittleEndian.codec_id(),
        Timebase::integer(48_000).unwrap(),
    );
    let video_config = DecoderConfig::new(video_stream);
    let error = match backend.create_decoder(&video_config, operation()) {
        Ok(_) => panic!("PCM decoding a video stream must fail"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let mismatched_decoder =
        DecoderConfig::new(stream(PcmEncoding::I16LittleEndian, stream_id, 48_000))
            .with_audio_format(f32_format.clone())
            .unwrap();
    let error = match backend.create_decoder(&mismatched_decoder, operation()) {
        Ok(_) => panic!("a PCM decoder codec and sample format mismatch must fail"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let mismatched_encoder = EncoderConfig::audio(
        stream_id,
        PcmEncoding::I16LittleEndian.codec_id(),
        f32_format,
    );
    let error = match backend.create_encoder(&mismatched_encoder, operation()) {
        Ok(_) => panic!("a PCM encoder codec and sample format mismatch must fail"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let cancelled = OperationContext::new(MediaPriority::Interactive);
    cancelled.cancellation_token().cancel();
    let valid_decoder = DecoderConfig::new(stream(PcmEncoding::I16LittleEndian, stream_id, 48_000))
        .with_audio_format(i16_format)
        .unwrap();
    let error = match backend.create_decoder(&valid_decoder, &cancelled) {
        Ok(_) => panic!("a cancelled PCM decoder creation must fail"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::Cancelled);
}

#[test]
fn pcm_decoder_rejects_stream_timing_and_duration_corruption() {
    let backend = pcm_backend();
    let stream_id = StreamId::new(21);
    let encoding = PcmEncoding::I16LittleEndian;
    let format = AudioFormat::new(48_000, SampleFormat::I16, ChannelLayout::stereo()).unwrap();

    let make_decoder = |timebase| {
        let source = StreamInfo::new(stream_id, StreamKind::Audio, encoding.codec_id(), timebase);
        let config = DecoderConfig::new(source)
            .with_audio_format(format.clone())
            .unwrap();
        backend.create_decoder(&config, operation()).unwrap()
    };

    let mut decoder = make_decoder(Timebase::integer(48_000).unwrap());
    let wrong_stream = Packet::new(
        StreamId::new(22),
        Arc::from([1, 0, 2, 0]),
        PacketTiming::new(
            Timebase::integer(48_000).unwrap(),
            Some(0),
            Some(0),
            Some(1),
        )
        .unwrap(),
    );
    let error = decoder.send_packet(wrong_stream, operation()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);

    let wrong_timebase = Packet::new(
        stream_id,
        Arc::from([1, 0, 2, 0]),
        PacketTiming::new(
            Timebase::integer(44_100).unwrap(),
            Some(0),
            Some(0),
            Some(1),
        )
        .unwrap(),
    );
    let error = decoder
        .send_packet(wrong_timebase, operation())
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);

    let wrong_duration = Packet::new(
        stream_id,
        Arc::from([1, 0, 2, 0]),
        PacketTiming::new(
            Timebase::integer(48_000).unwrap(),
            Some(0),
            Some(0),
            Some(2),
        )
        .unwrap(),
    );
    let error = decoder
        .send_packet(wrong_duration, operation())
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);

    let mut fractional_decoder = make_decoder(Timebase::integer(44_100).unwrap());
    let fractional_timestamp = Packet::new(
        stream_id,
        Arc::from([1, 0, 2, 0]),
        PacketTiming::new(Timebase::integer(44_100).unwrap(), Some(1), Some(1), None).unwrap(),
    );
    let error = fractional_decoder
        .send_packet(fractional_timestamp, operation())
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);
    assert_eq!(
        error.contexts().last().unwrap().operation(),
        "decode_pcm_packet"
    );
}

#[test]
fn pcm_encoder_cancellation_flush_and_reset_preserve_timing_and_metadata() {
    let backend = pcm_backend();
    let stream_id = StreamId::new(31);
    let encoding = PcmEncoding::I24BigEndian;
    let format = AudioFormat::new(96_000, SampleFormat::I24, ChannelLayout::stereo()).unwrap();
    let config = EncoderConfig::audio(stream_id, encoding.codec_id(), format.clone());
    let mut encoder = backend.create_encoder(&config, operation()).unwrap();
    let block = AudioBlock::new(
        format,
        SampleTime::new(960, 96_000).unwrap(),
        1,
        vec![AudioPlane::new(Arc::from([1, 2, 3, 4, 5, 6]))],
    )
    .unwrap()
    .with_metadata("pcm.source", MetadataValue::Text("timeline".into()))
    .unwrap();

    let cancelled = OperationContext::new(MediaPriority::Export);
    cancelled.cancellation_token().cancel();
    let error = encoder
        .send(EncodeInput::Audio(block.clone()), &cancelled)
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(
        encoder.receive(operation()).unwrap(),
        EncodeOutput::NeedInput
    );

    encoder.flush(operation()).unwrap();
    assert_eq!(
        encoder.receive(operation()).unwrap(),
        EncodeOutput::EndOfStream
    );
    let error = encoder
        .send(EncodeInput::Audio(block.clone()), operation())
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);

    encoder.reset(operation()).unwrap();
    encoder
        .send(EncodeInput::Audio(block.clone()), operation())
        .unwrap();
    let EncodeOutput::Packet(packet) = encoder.receive(operation()).unwrap() else {
        panic!("reset PCM encoder did not return a packet");
    };
    assert_eq!(
        packet.timing().presentation_time().unwrap(),
        block.timestamp().rational_time()
    );
    assert_eq!(packet.timing().duration().unwrap(), block.duration());
    assert_eq!(packet.metadata(), block.metadata());
}
