#![cfg(target_os = "macos")]

use std::collections::BTreeSet;

use std::sync::Arc;
use superi_codecs_platform::register::{platform_backend_registry, register_platform_backends};
use superi_codecs_platform::videotoolbox::{
    parse_avcc, parse_hvcc, ProResProfile, VideoToolboxBackend, AAC_CODEC_ID, H264_CODEC_ID,
    HEVC_CODEC_ID,
};
use superi_core::color_space::ColorSpace;
use superi_core::error::ErrorCategory;
use superi_core::pixel::{AlphaMode, ChannelLayout, PixelFormat, SampleFormat};
use superi_core::time::{Duration, RationalTime, SampleTime, Timebase};
use superi_media_io::audio_io::{AudioBlock, AudioFormat, AudioPlane};
use superi_media_io::backend::{
    BackendCapability, BackendRegistry, BackendRequirement, BackendTier, FallbackPolicy,
    MediaBackend,
};
use superi_media_io::decode::{
    CpuVideoBuffer, DecodeOutput, DecoderConfig, FrameStorageKind, VideoFormat, VideoFrame,
    VideoPlane,
};
use superi_media_io::demux::{CodecId, MetadataValue, StreamId, StreamInfo, StreamKind};
use superi_media_io::encode::{EncodeInput, EncodeOutput, EncoderConfig};
use superi_media_io::operation::{CancellationToken, MediaPriority, OperationContext};

#[test]
fn stable_codec_identifiers_cover_the_supported_launch_operations() {
    assert_eq!(H264_CODEC_ID, "h264");
    assert_eq!(HEVC_CODEC_ID, "hevc");
    assert_eq!(AAC_CODEC_ID, "aac");
    assert_eq!(
        ProResProfile::ALL
            .iter()
            .map(|profile| profile.codec_id().as_str().to_owned())
            .collect::<Vec<_>>(),
        [
            "prores-422-proxy",
            "prores-422-lt",
            "prores-422",
            "prores-422-hq",
            "prores-4444",
        ]
    );
}

#[cfg(target_os = "macos")]
#[test]
fn macos_registry_exposes_one_deterministic_primary_native_backend() {
    let registry = platform_backend_registry().unwrap();
    let registrations = registry.registrations().collect::<Vec<_>>();
    assert_eq!(registrations.len(), 1);
    let registration = registrations[0];
    assert_eq!(
        registration.backend().descriptor().id().as_str(),
        "apple-videotoolbox"
    );
    assert_eq!(
        registration.backend().descriptor().display_name(),
        "Apple VideoToolbox"
    );
    assert_eq!(registration.priority(), 200);
    assert_eq!(registration.tier(), BackendTier::Primary);

    let expected = std::iter::once(H264_CODEC_ID)
        .chain(std::iter::once(HEVC_CODEC_ID))
        .chain(std::iter::once(AAC_CODEC_ID))
        .chain(ProResProfile::ALL.iter().map(|profile| profile.code()))
        .collect::<BTreeSet<_>>();
    let mut decoded = BTreeSet::new();
    let mut encoded = BTreeSet::new();
    for capability in registration.capabilities().iter() {
        let requirement = match capability {
            BackendCapability::Decode(codec) => {
                decoded.insert(codec.as_str());
                BackendRequirement::decode(codec.clone())
            }
            BackendCapability::Encode(codec) => {
                encoded.insert(codec.as_str());
                BackendRequirement::encode(codec.clone())
            }
            BackendCapability::Source => panic!("codec backends do not own container probing"),
            _ => panic!("unexpected backend capability"),
        };
        let selection = registry
            .select(&requirement, FallbackPolicy::Disallow)
            .unwrap();
        assert_eq!(
            selection.primary().descriptor().id().as_str(),
            "apple-videotoolbox"
        );
        assert!(!selection.fallback_used());
        assert!(selection.fallbacks().is_empty());
    }
    assert_eq!(decoded, expected);
    assert_eq!(encoded, expected);
}

#[test]
fn platform_registration_is_all_or_nothing_on_backend_identity_conflict() {
    let mut registry = BackendRegistry::new();
    register_platform_backends(&mut registry).unwrap();
    let before = registry.registrations().count();

    #[cfg(target_os = "macos")]
    {
        let error = register_platform_backends(&mut registry).unwrap_err();
        assert_eq!(registry.registrations().count(), before);
        assert_eq!(
            error.contexts().last().unwrap().field("backend_id"),
            Some("apple-videotoolbox")
        );
    }

    #[cfg(not(target_os = "macos"))]
    {
        register_platform_backends(&mut registry).unwrap();
        assert_eq!(registry.registrations().count(), before);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn backend_construction_retains_the_stable_identity() {
    let backend = VideoToolboxBackend::new().unwrap();
    assert_eq!(backend.descriptor().id().as_str(), "apple-videotoolbox");
}

#[cfg(target_os = "macos")]
#[test]
fn iso_video_configuration_parsers_reject_truncation_and_preserve_parameter_sets() {
    let avcc = [
        1, 100, 0, 31, 0xff, 0xe1, 0, 4, 0x67, 100, 0, 31, 1, 0, 2, 0x68, 0xee,
    ];
    let parsed = parse_avcc(&avcc).unwrap();
    assert_eq!(parsed.nal_length_size(), 4);
    assert_eq!(
        parsed.parameter_sets(),
        [&[0x67, 100, 0, 31][..], &[0x68, 0xee][..]]
    );
    assert!(parse_avcc(&avcc[..avcc.len() - 1]).is_err());

    let mut hvcc = vec![
        1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 3, 3,
    ];
    for (nal_type, payload) in [
        (32, vec![0x40, 1]),
        (33, vec![0x42, 1]),
        (34, vec![0x44, 1]),
    ] {
        hvcc.push(nal_type);
        hvcc.extend_from_slice(&1_u16.to_be_bytes());
        hvcc.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        hvcc.extend_from_slice(&payload);
    }
    let parsed = parse_hvcc(&hvcc).unwrap();
    assert_eq!(parsed.nal_length_size(), 4);
    assert_eq!(parsed.parameter_sets().len(), 3);
    assert!(parse_hvcc(&hvcc[..hvcc.len() - 1]).is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn h264_native_roundtrip_preserves_timing_and_external_frame_ownership() {
    let backend = VideoToolboxBackend::new().unwrap();
    let operation = OperationContext::new(MediaPriority::Export);
    let timebase = Timebase::integer(30).unwrap();
    let format = VideoFormat::new(
        64,
        64,
        PixelFormat::Bgra8Unorm,
        ColorSpace::SRGB,
        AlphaMode::Opaque,
    )
    .unwrap();
    let plane = VideoPlane::new(Arc::from(vec![0_u8; 64 * 64 * 4]), 64 * 4, 64).unwrap();
    let buffer =
        Arc::new(CpuVideoBuffer::new(64, 64, PixelFormat::Bgra8Unorm, vec![plane]).unwrap());
    let frame = VideoFrame::new(
        format,
        RationalTime::new(7, timebase),
        Duration::new(1, timebase).unwrap(),
        buffer,
    )
    .unwrap();
    let encoder_config = EncoderConfig::video(
        StreamId::new(9),
        CodecId::new(H264_CODEC_ID).unwrap(),
        timebase,
        format,
    );
    let mut encoder = backend.create_encoder(&encoder_config, &operation).unwrap();
    encoder.send(EncodeInput::Video(frame), &operation).unwrap();
    encoder.flush(&operation).unwrap();
    let packet = loop {
        match encoder.receive(&operation).unwrap() {
            EncodeOutput::Packet(packet) => break packet,
            EncodeOutput::NeedInput => continue,
            EncodeOutput::EndOfStream => panic!("VideoToolbox emitted no H.264 packet"),
            _ => panic!("unexpected encode output"),
        }
    };
    assert_eq!(packet.timing().presentation_time().unwrap().value(), 7);
    assert_eq!(packet.timing().duration().unwrap().value(), 1);
    assert!(packet.is_keyframe());
    let configuration = match packet.metadata().get("codec.configuration") {
        Some(MetadataValue::Bytes(bytes)) => Arc::clone(bytes),
        value => panic!("missing H.264 codec configuration: {value:?}"),
    };
    parse_avcc(&configuration).unwrap();

    let stream = StreamInfo::new(
        StreamId::new(9),
        StreamKind::Video,
        CodecId::new(H264_CODEC_ID).unwrap(),
        timebase,
    )
    .with_metadata("codec.configuration", MetadataValue::Bytes(configuration))
    .unwrap();
    let mut decoder = backend
        .create_decoder(&DecoderConfig::new(stream), &operation)
        .unwrap();
    decoder.send_packet(packet, &operation).unwrap();
    decoder.flush(&operation).unwrap();
    let decoded = loop {
        match decoder.receive(&operation).unwrap() {
            DecodeOutput::Frame(frame) => break frame,
            DecodeOutput::NeedInput => continue,
            DecodeOutput::EndOfStream => panic!("VideoToolbox emitted no decoded frame"),
            _ => panic!("unexpected decode output"),
        }
    };
    assert_eq!(
        (decoded.format().width(), decoded.format().height()),
        (64, 64)
    );
    assert_eq!(decoded.timestamp().value(), 7);
    assert_eq!(decoded.duration().value(), 1);
    assert_eq!(decoded.buffer().storage_kind(), FrameStorageKind::External);

    let native_encoder_config = EncoderConfig::video(
        StreamId::new(10),
        CodecId::new(H264_CODEC_ID).unwrap(),
        timebase,
        decoded.format(),
    );
    let mut native_encoder = backend
        .create_encoder(&native_encoder_config, &operation)
        .unwrap();
    native_encoder
        .send(EncodeInput::Video(decoded), &operation)
        .unwrap();
    native_encoder.flush(&operation).unwrap();
    assert!(matches!(
        native_encoder.receive(&operation).unwrap(),
        EncodeOutput::Packet(_)
    ));
}

#[cfg(target_os = "macos")]
#[test]
fn hevc_and_every_advertised_prores_profile_complete_native_roundtrips() {
    for codec in std::iter::once(HEVC_CODEC_ID)
        .chain(ProResProfile::ALL.iter().map(|profile| profile.code()))
    {
        native_video_roundtrip(codec);
    }
}

#[cfg(target_os = "macos")]
fn native_video_roundtrip(codec: &str) {
    let backend = VideoToolboxBackend::new().unwrap();
    let operation = OperationContext::new(MediaPriority::Export);
    let timebase = Timebase::integer(24).unwrap();
    let format = VideoFormat::new(
        64,
        64,
        PixelFormat::Bgra8Unorm,
        ColorSpace::SRGB,
        AlphaMode::Opaque,
    )
    .unwrap();
    let mut pixels = vec![0_u8; 64 * 64 * 4];
    for (index, pixel) in pixels.chunks_exact_mut(4).enumerate() {
        pixel.copy_from_slice(&[
            (index % 251) as u8,
            ((index / 7) % 241) as u8,
            ((index / 13) % 239) as u8,
            255,
        ]);
    }
    let plane = VideoPlane::new(Arc::from(pixels), 64 * 4, 64).unwrap();
    let buffer =
        Arc::new(CpuVideoBuffer::new(64, 64, PixelFormat::Bgra8Unorm, vec![plane]).unwrap());
    let frame = VideoFrame::new(
        format,
        RationalTime::new(11, timebase),
        Duration::new(1, timebase).unwrap(),
        buffer,
    )
    .unwrap();
    let encoder_config = EncoderConfig::video(
        StreamId::new(3),
        CodecId::new(codec).unwrap(),
        timebase,
        format,
    );
    let mut encoder = backend.create_encoder(&encoder_config, &operation).unwrap();
    encoder.send(EncodeInput::Video(frame), &operation).unwrap();
    encoder.flush(&operation).unwrap();
    let packet = loop {
        match encoder.receive(&operation).unwrap() {
            EncodeOutput::Packet(packet) => break packet,
            EncodeOutput::NeedInput => continue,
            EncodeOutput::EndOfStream => panic!("VideoToolbox emitted no packet for {codec}"),
            _ => panic!("unexpected encode output for {codec}"),
        }
    };
    assert!(
        !packet.data().is_empty(),
        "empty encoded packet for {codec}"
    );
    assert_eq!(packet.timing().presentation_time().unwrap().value(), 11);
    assert_eq!(packet.timing().duration().unwrap().value(), 1);

    let mut stream = StreamInfo::new(
        StreamId::new(3),
        StreamKind::Video,
        CodecId::new(codec).unwrap(),
        timebase,
    );
    if codec == HEVC_CODEC_ID {
        let configuration = match packet.metadata().get("codec.configuration") {
            Some(MetadataValue::Bytes(bytes)) => Arc::clone(bytes),
            value => panic!("missing HEVC codec configuration: {value:?}"),
        };
        parse_hvcc(&configuration).unwrap();
        stream = stream
            .with_metadata("codec.configuration", MetadataValue::Bytes(configuration))
            .unwrap();
    } else {
        stream = stream
            .with_metadata("video.width", MetadataValue::Unsigned(64))
            .unwrap()
            .with_metadata("video.height", MetadataValue::Unsigned(64))
            .unwrap();
    }
    let mut decoder = backend
        .create_decoder(&DecoderConfig::new(stream), &operation)
        .unwrap();
    decoder.send_packet(packet, &operation).unwrap();
    decoder.flush(&operation).unwrap();
    let decoded = loop {
        match decoder.receive(&operation).unwrap() {
            DecodeOutput::Frame(frame) => break frame,
            DecodeOutput::NeedInput => continue,
            DecodeOutput::EndOfStream => panic!("VideoToolbox emitted no frame for {codec}"),
            _ => panic!("unexpected decode output for {codec}"),
        }
    };
    assert_eq!(
        (decoded.format().width(), decoded.format().height()),
        (64, 64)
    );
    assert_eq!(decoded.timestamp().value(), 11);
    assert_eq!(decoded.duration().value(), 1);
    assert_eq!(decoded.buffer().storage_kind(), FrameStorageKind::External);
}

#[cfg(target_os = "macos")]
#[test]
fn aac_audio_converter_roundtrip_preserves_sample_timing_and_channel_layout() {
    let backend = VideoToolboxBackend::new().unwrap();
    let operation = OperationContext::new(MediaPriority::Export);
    let format = AudioFormat::new(48_000, SampleFormat::F32, ChannelLayout::stereo()).unwrap();
    let frame_count = 2_048_u64;
    let mut samples = Vec::with_capacity(frame_count as usize * 2 * 4);
    for frame in 0..frame_count {
        let sample = ((frame as f32 * 0.03125).sin() * 0.25).to_ne_bytes();
        samples.extend_from_slice(&sample);
        samples.extend_from_slice(&sample);
    }
    let block = AudioBlock::new(
        format.clone(),
        SampleTime::new(4_096, 48_000).unwrap(),
        frame_count,
        vec![AudioPlane::new(Arc::from(samples))],
    )
    .unwrap();
    let encoder_config = EncoderConfig::audio(
        StreamId::new(12),
        CodecId::new(AAC_CODEC_ID).unwrap(),
        format.clone(),
    );
    let mut encoder = backend.create_encoder(&encoder_config, &operation).unwrap();
    encoder.send(EncodeInput::Audio(block), &operation).unwrap();
    encoder.flush(&operation).unwrap();
    let mut packets = Vec::new();
    loop {
        match encoder.receive(&operation).unwrap() {
            EncodeOutput::Packet(packet) => packets.push(packet),
            EncodeOutput::EndOfStream => break,
            EncodeOutput::NeedInput => continue,
            _ => panic!("unexpected AAC encode output"),
        }
    }
    assert!(!packets.is_empty());
    let configuration = packets
        .iter()
        .find_map(
            |packet| match packet.metadata().get("codec.configuration") {
                Some(MetadataValue::Bytes(bytes)) => Some(Arc::clone(bytes)),
                _ => None,
            },
        )
        .expect("AAC encoder must expose its magic cookie");
    let stream = StreamInfo::new(
        StreamId::new(12),
        StreamKind::Audio,
        CodecId::new(AAC_CODEC_ID).unwrap(),
        Timebase::integer(48_000).unwrap(),
    )
    .with_metadata("codec.configuration", MetadataValue::Bytes(configuration))
    .unwrap();
    let decoder_config = DecoderConfig::new(stream)
        .with_audio_format(format.clone())
        .unwrap();
    let mut decoder = backend.create_decoder(&decoder_config, &operation).unwrap();
    for packet in packets {
        decoder.send_packet(packet, &operation).unwrap();
    }
    decoder.flush(&operation).unwrap();
    let mut blocks = Vec::new();
    loop {
        match decoder.receive(&operation).unwrap() {
            DecodeOutput::Audio(block) => blocks.push(block),
            DecodeOutput::EndOfStream => break,
            DecodeOutput::NeedInput => continue,
            _ => panic!("unexpected AAC decode output"),
        }
    }
    assert!(!blocks.is_empty());
    assert_eq!(blocks[0].timestamp().sample(), 4_096);
    assert_eq!(
        blocks[0].format().channel_layout(),
        &ChannelLayout::stereo()
    );
    assert!(blocks.iter().all(|block| block.frame_count() > 0));
}

#[cfg(target_os = "macos")]
#[test]
fn malformed_configuration_and_cancellation_fail_before_native_work() {
    let backend = VideoToolboxBackend::new().unwrap();
    let operation = OperationContext::new(MediaPriority::Interactive);
    let stream = StreamInfo::new(
        StreamId::new(1),
        StreamKind::Video,
        CodecId::new(H264_CODEC_ID).unwrap(),
        Timebase::integer(30).unwrap(),
    );
    let error = backend
        .create_decoder(&DecoderConfig::new(stream), &operation)
        .err()
        .expect("missing H.264 configuration must fail");
    assert_eq!(error.category(), ErrorCategory::CorruptData);

    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let cancelled =
        OperationContext::new(MediaPriority::Interactive).with_cancellation(cancellation);
    let format = VideoFormat::new(
        64,
        64,
        PixelFormat::Bgra8Unorm,
        ColorSpace::SRGB,
        AlphaMode::Opaque,
    )
    .unwrap();
    let encoder_config = EncoderConfig::video(
        StreamId::new(1),
        CodecId::new(H264_CODEC_ID).unwrap(),
        Timebase::integer(30).unwrap(),
        format,
    );
    let error = backend
        .create_encoder(&encoder_config, &cancelled)
        .err()
        .expect("cancelled encoder creation must fail");
    assert_eq!(error.category(), ErrorCategory::Cancelled);
}
