use superi_codecs_platform::media_foundation::{
    AacConfiguration, AnnexBConfiguration, MediaFoundationCodec, MediaFoundationOperation,
    ProResProfile, AAC_CODEC_ID, H264_CODEC_ID, HEVC_CODEC_ID,
};
use superi_codecs_platform::register::platform_backend_registry;
#[cfg(not(target_os = "windows"))]
use superi_codecs_platform::register::register_platform_backends;
use superi_core::error::{ErrorCategory, Recoverability};
#[cfg(not(target_os = "windows"))]
use superi_media_io::backend::BackendRegistry;

#[test]
fn stable_codec_identifiers_cover_every_windows_operation_family() {
    assert_eq!(H264_CODEC_ID, "h264");
    assert_eq!(HEVC_CODEC_ID, "hevc");
    assert_eq!(AAC_CODEC_ID, "aac");
    assert_eq!(
        MediaFoundationCodec::H264.codec_id().as_str(),
        H264_CODEC_ID
    );
    assert_eq!(
        MediaFoundationCodec::Hevc.codec_id().as_str(),
        HEVC_CODEC_ID
    );
    assert_eq!(MediaFoundationCodec::Aac.codec_id().as_str(), AAC_CODEC_ID);
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
    assert_eq!(
        ProResProfile::ALL
            .iter()
            .map(|profile| profile.fourcc())
            .collect::<Vec<_>>(),
        [*b"apco", *b"apcs", *b"apcn", *b"apch", *b"ap4h"]
    );
    for profile in ProResProfile::ALL {
        assert_eq!(
            MediaFoundationCodec::from_codec_id(&profile.codec_id()),
            Some(MediaFoundationCodec::ProRes(*profile))
        );
    }
    assert_eq!(
        MediaFoundationOperation::ALL,
        &[
            MediaFoundationOperation::Decode,
            MediaFoundationOperation::Encode,
        ]
    );
}

#[test]
fn avcc_configuration_converts_length_prefixed_samples_to_annex_b() {
    let avcc = [
        1, 100, 0, 31, 0xff, 0xe1, 0x00, 0x04, 0x67, 0x64, 0x00, 0x1f, 0x01, 0x00, 0x02, 0x68, 0xee,
    ];
    let configuration = AnnexBConfiguration::parse(MediaFoundationCodec::H264, &avcc).unwrap();

    assert_eq!(configuration.nal_length_size(), 4);
    assert_eq!(
        configuration.parameter_sets(),
        &[0, 0, 0, 1, 0x67, 0x64, 0, 0x1f, 0, 0, 0, 1, 0x68, 0xee]
    );

    let sample = [0, 0, 0, 3, 0x65, 0x11, 0x22, 0, 0, 0, 2, 0x06, 0x05];
    assert_eq!(
        configuration.convert_sample(&sample, false).unwrap(),
        [0, 0, 0, 1, 0x65, 0x11, 0x22, 0, 0, 0, 1, 0x06, 0x05]
    );
    assert_eq!(
        configuration.convert_sample(&sample, true).unwrap(),
        [
            0, 0, 0, 1, 0x67, 0x64, 0, 0x1f, 0, 0, 0, 1, 0x68, 0xee, 0, 0, 0, 1, 0x65, 0x11, 0x22,
            0, 0, 0, 1, 0x06, 0x05,
        ]
    );
}

#[test]
fn hvcc_configuration_preserves_vps_sps_and_pps_in_annex_b_order() {
    let mut hvcc = vec![
        1, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 120, 0xf0, 0, 0xfc, 0xfd, 0xf8, 0xf8, 0, 0, 0x0f, 3,
    ];
    for (nal_type, payload) in [
        (32, &[0x40, 0x01][..]),
        (33, &[0x42, 0x01][..]),
        (34, &[0x44, 0x01][..]),
    ] {
        hvcc.extend_from_slice(&[0x80 | nal_type, 0, 1, 0, payload.len() as u8]);
        hvcc.extend_from_slice(payload);
    }
    let configuration = AnnexBConfiguration::parse(MediaFoundationCodec::Hevc, &hvcc).unwrap();

    assert_eq!(configuration.nal_length_size(), 4);
    assert_eq!(
        configuration.parameter_sets(),
        &[0, 0, 0, 1, 0x40, 0x01, 0, 0, 0, 1, 0x42, 0x01, 0, 0, 0, 1, 0x44, 0x01,]
    );
}

#[test]
fn malformed_nal_configuration_and_samples_are_classified_as_corrupt_data() {
    let error = AnnexBConfiguration::parse(MediaFoundationCodec::H264, &[1, 100]).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);
    assert_eq!(error.recoverability(), Recoverability::Degraded);
    assert_eq!(
        error.contexts().last().unwrap().component(),
        "superi-codecs-platform.media-foundation"
    );

    let configuration =
        AnnexBConfiguration::parse(MediaFoundationCodec::H264, &[1, 100, 0, 31, 0xff, 0xe0, 0])
            .unwrap();
    let error = configuration
        .convert_sample(&[0, 0, 0, 4, 0x65], false)
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);
}

#[test]
fn aac_configuration_extracts_audio_specific_config_from_raw_and_esds_metadata() {
    let raw = AacConfiguration::parse(&[0x12, 0x10]).unwrap();
    assert_eq!(raw.audio_object_type(), 2);
    assert_eq!(raw.sample_rate(), 44_100);
    assert_eq!(raw.channel_count(), 2);
    assert_eq!(raw.audio_specific_config(), [0x12, 0x10]);
    assert_eq!(
        raw.media_foundation_user_data(),
        [0, 0, 0xfe, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x12, 0x10]
    );

    let esds = [
        0, 0, 0, 0, 0x03, 0x16, 0, 1, 0, 0x04, 0x11, 0x40, 0x15, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        0x05, 0x02, 0x12, 0x10,
    ];
    let parsed = AacConfiguration::parse(&esds).unwrap();
    assert_eq!(parsed.audio_specific_config(), [0x12, 0x10]);
    assert_eq!(parsed.sample_rate(), 44_100);
    assert_eq!(parsed.channel_count(), 2);
}

#[test]
fn unsupported_aac_profiles_and_channel_configurations_fail_explicitly() {
    let main_profile = AacConfiguration::parse(&[0x0a, 0x10]).unwrap_err();
    assert_eq!(main_profile.category(), ErrorCategory::Unsupported);
    assert_eq!(main_profile.recoverability(), Recoverability::Degraded);

    let reserved_channel_configuration = AacConfiguration::parse(&[0x12, 0x00]).unwrap_err();
    assert_eq!(
        reserved_channel_configuration.category(),
        ErrorCategory::Unsupported
    );
}

#[cfg(not(target_os = "windows"))]
#[test]
fn non_windows_hosts_do_not_advertise_media_foundation_capabilities() {
    let mut registry = BackendRegistry::new();
    register_platform_backends(&mut registry).unwrap();
    assert!(registry.registrations().all(|registration| {
        registration.backend().descriptor().id().as_str() != "windows-media-foundation"
    }));
    assert!(platform_backend_registry()
        .unwrap()
        .registrations()
        .all(|registration| {
            registration.backend().descriptor().id().as_str() != "windows-media-foundation"
        }));
}

#[cfg(target_os = "windows")]
#[test]
fn windows_registry_advertises_only_operations_discovered_from_real_transforms() {
    use std::collections::BTreeSet;

    use superi_media_io::backend::{
        BackendCapability, BackendTier, CodecOperation, HardwareAcceleration,
    };

    let registry = platform_backend_registry().unwrap();
    let registrations = registry.registrations().collect::<Vec<_>>();
    assert_eq!(registrations.len(), 1);
    let registration = registrations[0];
    assert_eq!(
        registration.backend().descriptor().id().as_str(),
        "windows-media-foundation"
    );
    assert_eq!(registration.priority(), 200);
    assert_eq!(registration.tier(), BackendTier::Primary);
    assert_eq!(
        registration.capabilities().hardware_acceleration(),
        HardwareAcceleration::Software
    );

    let capabilities = registration
        .capabilities()
        .iter()
        .map(|capability| match capability {
            BackendCapability::Decode(codec) => ("decode", codec.as_str().to_owned()),
            BackendCapability::Encode(codec) => ("encode", codec.as_str().to_owned()),
            BackendCapability::Source => panic!("codec backends do not own containers"),
            _ => panic!("unexpected backend capability"),
        })
        .collect::<BTreeSet<_>>();
    assert!(!capabilities.is_empty());
    let details = registration
        .capabilities()
        .codec_capabilities()
        .map(|detail| {
            let operation = match detail.operation() {
                CodecOperation::Decode => "decode",
                CodecOperation::Encode => "encode",
                _ => panic!("unexpected codec operation"),
            };
            (operation, detail.codec().as_str().to_owned())
        })
        .collect::<BTreeSet<_>>();
    assert_eq!(details, capabilities);
}

#[cfg(target_os = "windows")]
#[test]
fn discovered_aac_transforms_complete_a_real_encode_decode_lifecycle() {
    use std::sync::Arc;

    use superi_core::pixel::{ChannelLayout, SampleFormat};
    use superi_core::time::{SampleTime, Timebase};
    use superi_media_io::audio_io::{AudioBlock, AudioFormat, AudioPlane};
    use superi_media_io::backend::BackendCapability;
    use superi_media_io::decode::{DecodeOutput, DecoderConfig};
    use superi_media_io::demux::{CodecId, MetadataValue, StreamId, StreamInfo, StreamKind};
    use superi_media_io::encode::{EncodeInput, EncodeOutput, EncoderConfig};
    use superi_media_io::operation::{MediaPriority, OperationContext};

    let registry = platform_backend_registry().unwrap();
    let Some(registration) = registry.registrations().next() else {
        return;
    };
    let codec = CodecId::new(AAC_CODEC_ID).unwrap();
    if !registration
        .capabilities()
        .contains(&BackendCapability::Encode(codec.clone()))
        || !registration
            .capabilities()
            .contains(&BackendCapability::Decode(codec.clone()))
    {
        return;
    }

    let operation = OperationContext::new(MediaPriority::Export);
    let format = AudioFormat::new(48_000, SampleFormat::I16, ChannelLayout::stereo()).unwrap();
    let encoder_config = EncoderConfig::audio(StreamId::new(17), codec.clone(), format.clone());
    let mut encoder = registration
        .backend()
        .create_encoder(&encoder_config, &operation)
        .unwrap();
    let mut packets = Vec::new();
    for block_index in 0..8_i64 {
        let block = AudioBlock::new(
            format.clone(),
            SampleTime::new(block_index * 1_024, 48_000).unwrap(),
            1_024,
            vec![AudioPlane::new(Arc::from(vec![0_u8; 1_024 * 2 * 2]))],
        )
        .unwrap()
        .with_metadata("test.block-index", MetadataValue::Signed(block_index))
        .unwrap();
        encoder.send(EncodeInput::Audio(block), &operation).unwrap();
        while let EncodeOutput::Packet(packet) = encoder.receive(&operation).unwrap() {
            packets.push(packet);
        }
    }
    encoder.flush(&operation).unwrap();
    loop {
        match encoder.receive(&operation).unwrap() {
            EncodeOutput::Packet(packet) => packets.push(packet),
            EncodeOutput::EndOfStream => break,
            EncodeOutput::NeedInput => panic!("flushed AAC encoder requested more input"),
            _ => panic!("unexpected AAC encoder output"),
        }
    }
    assert!(!packets.is_empty());
    assert_eq!(packets[0].timing().presentation_time().unwrap().value(), 0);
    assert_eq!(packets[0].timing().duration().unwrap().value(), 1_024);
    assert_eq!(
        packets[0].metadata().get("test.block-index"),
        Some(&MetadataValue::Signed(0))
    );
    let configuration = packets
        .iter()
        .find_map(
            |packet| match packet.metadata().get("codec.configuration") {
                Some(MetadataValue::Bytes(bytes)) => Some(bytes.clone()),
                _ => None,
            },
        )
        .expect("AAC encoder must publish AudioSpecificConfig");

    let stream = StreamInfo::new(
        StreamId::new(17),
        StreamKind::Audio,
        codec,
        Timebase::integer(48_000).unwrap(),
    )
    .with_metadata("codec.configuration", MetadataValue::Bytes(configuration))
    .unwrap();
    let decoder_config = DecoderConfig::new(stream)
        .with_audio_format(format)
        .unwrap();
    let mut decoder = registration
        .backend()
        .create_decoder(&decoder_config, &operation)
        .unwrap();
    let mut decoded_frames = 0_u64;
    let mut decoded_first_metadata = false;
    for packet in packets {
        decoder.send_packet(packet, &operation).unwrap();
        while let DecodeOutput::Audio(block) = decoder.receive(&operation).unwrap() {
            decoded_frames += block.frame_count();
            decoded_first_metadata |=
                block.metadata().get("test.block-index") == Some(&MetadataValue::Signed(0));
        }
    }
    decoder.flush(&operation).unwrap();
    loop {
        match decoder.receive(&operation).unwrap() {
            DecodeOutput::Audio(block) => {
                decoded_frames += block.frame_count();
                decoded_first_metadata |=
                    block.metadata().get("test.block-index") == Some(&MetadataValue::Signed(0));
            }
            DecodeOutput::EndOfStream => break,
            DecodeOutput::NeedInput => panic!("flushed AAC decoder requested more input"),
            _ => panic!("unexpected AAC decoder output"),
        }
    }
    assert!(decoded_frames > 0);
    assert!(decoded_first_metadata);
}
