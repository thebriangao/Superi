use std::path::PathBuf;
use std::process::Command;
use std::sync::{Arc, OnceLock};
use std::time::Duration as StdDuration;

use superi_codecs_vendor::protocol::{
    Envelope, PluginManifest, ProtocolRequest, ProtocolResponse, PROTOCOL_REVISION,
};
use superi_codecs_vendor::{
    register_vendor_plugins, VendorPluginConfig, VendorRawFormat, VENDOR_RAW_FORMATS,
};
use superi_core::color_space::ColorSpace;
use superi_core::error::ErrorCategory;
use superi_core::ids::MediaId;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::time::{RationalTime, Timebase};
use superi_media_io::backend::{
    BackendCapability, BackendRegistry, BackendRequirement, CapabilityConstraint, CodecOperation,
    FallbackPolicy, HardwareAcceleration,
};
use superi_media_io::decode::{DecodeOutput, DecoderConfig, FrameStorageKind};
use superi_media_io::demux::{
    CodecId, MetadataValue, SeekMode, SeekRequest, SourceLocation, SourceProbeLimits,
    SourceRequest, StreamId,
};
use superi_media_io::encode::EncoderConfig;
use superi_media_io::operation::{MediaPriority, OperationContext};
use superi_media_io::read::ReadOutcome;

#[test]
fn vendor_raw_identities_are_complete_stable_and_decode_only() {
    assert_eq!(
        VENDOR_RAW_FORMATS,
        [
            VendorRawFormat::Arriraw,
            VendorRawFormat::R3d,
            VendorRawFormat::Braw,
        ]
    );
    assert_eq!(VendorRawFormat::Arriraw.code(), "arriraw");
    assert_eq!(VendorRawFormat::R3d.code(), "r3d");
    assert_eq!(VendorRawFormat::Braw.code(), "braw");
    assert_eq!(
        VendorRawFormat::from_code("arriraw"),
        Some(VendorRawFormat::Arriraw)
    );
    assert_eq!(
        VendorRawFormat::from_code("r3d"),
        Some(VendorRawFormat::R3d)
    );
    assert_eq!(
        VendorRawFormat::from_code("braw"),
        Some(VendorRawFormat::Braw)
    );
    assert_eq!(VendorRawFormat::from_code("h264"), None);
}

#[test]
fn protocol_manifest_is_revisioned_strict_and_round_trips_all_vendor_formats() {
    let request = Envelope {
        id: 7,
        payload: ProtocolRequest::Handshake {
            protocol_revision: PROTOCOL_REVISION,
        },
    };
    let request_json = serde_json::to_value(&request).unwrap();
    assert_eq!(request_json["id"], 7);
    assert_eq!(request_json["payload"]["kind"], "handshake");

    let response = Envelope {
        id: 7,
        payload: ProtocolResponse::Handshake {
            manifest: PluginManifest {
                protocol_revision: PROTOCOL_REVISION,
                backend_id: "blackmagic-braw-sdk".to_owned(),
                display_name: "Blackmagic RAW SDK".to_owned(),
                plugin_version: "1.2.3".to_owned(),
                sdk_version: "5.1".to_owned(),
                formats: VENDOR_RAW_FORMATS.to_vec(),
            },
        },
    };
    let json = serde_json::to_string(&response).unwrap();
    let decoded: Envelope<ProtocolResponse> = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, response);

    assert!(
        serde_json::from_value::<Envelope<ProtocolResponse>>(serde_json::json!({
            "id": 7,
            "payload": {
                "kind": "handshake",
                "manifest": {
                    "protocol_revision": 1,
                    "backend_id": "blackmagic-braw-sdk",
                    "display_name": "Blackmagic RAW SDK",
                    "plugin_version": "1.2.3",
                    "sdk_version": "5.1",
                    "formats": ["braw"],
                    "unexpected": true
                }
            }
        }))
        .is_err()
    );
}

#[test]
fn installed_worker_runs_the_real_probe_source_seek_decode_and_relink_path() {
    let worker = compile_worker();
    let operation = OperationContext::new(MediaPriority::Interactive)
        .with_timeout(StdDuration::from_secs(5))
        .unwrap();
    let mut registry = BackendRegistry::new();
    register_vendor_plugins(
        &mut registry,
        &[VendorPluginConfig::new(worker)],
        &operation,
    )
    .unwrap();

    let registration = registry.registrations().next().unwrap();
    assert_eq!(
        registration.backend().descriptor().id().as_str(),
        "test-vendor-raw"
    );
    assert!(registration
        .capabilities()
        .contains(&BackendCapability::Source));
    assert_eq!(
        registration.capabilities().hardware_acceleration(),
        HardwareAcceleration::Unreported
    );
    for codec in ["arriraw", "r3d", "braw"] {
        assert!(registration
            .capabilities()
            .contains(&BackendCapability::Decode(CodecId::new(codec).unwrap())));
        assert!(!registration
            .capabilities()
            .contains(&BackendCapability::Encode(CodecId::new(codec).unwrap())));
        let detail = registration
            .capabilities()
            .codec_capabilities()
            .find(|detail| {
                detail.operation() == CodecOperation::Decode && detail.codec().as_str() == codec
            })
            .unwrap();
        assert!(matches!(
            detail.profiles(),
            CapabilityConstraint::Unreported
        ));
        assert!(matches!(detail.levels(), CapabilityConstraint::Unreported));
        assert!(matches!(
            detail.bit_depths(),
            CapabilityConstraint::Unreported
        ));
        assert!(matches!(
            detail.chroma_sampling(),
            CapabilityConstraint::Unreported
        ));
    }

    let request = SourceRequest::new(
        MediaId::from_raw(28),
        SourceLocation::Memory {
            name: "camera-original.braw".to_owned(),
            data: Arc::from([0x42_u8, 0x52, 0x41, 0x57]),
        },
    );
    let selection = registry
        .probe_source(
            request,
            SourceProbeLimits::new(4, 64).unwrap(),
            FallbackPolicy::Disallow,
            &operation,
        )
        .unwrap();
    assert_eq!(selection.primary().container().as_str(), "braw");
    assert_eq!(selection.primary().confidence().value(), 100);

    let mut source = selection.open(&operation).unwrap();
    assert_eq!(source.info().identity().media_id(), MediaId::from_raw(28));
    assert_eq!(source.info().identity().fingerprint(), "sha256:fixture");
    assert_eq!(source.info().streams()[0].codec().as_str(), "braw");
    assert_eq!(
        source.info().metadata().get("vendor.sidecar"),
        Some(&MetadataValue::Boolean(true))
    );

    let packet = match source.read_packet(&operation).unwrap() {
        ReadOutcome::Complete(packet) => packet,
        other => panic!("expected complete packet, found {other:?}"),
    };
    assert_eq!(packet.data(), [1, 2]);
    assert_eq!(packet.timing().presentation_time().unwrap().value(), 0);
    assert_eq!(packet.timing().duration().unwrap().value(), 1);
    assert!(packet.is_keyframe());

    let backend = registry
        .select(
            &BackendRequirement::decode(CodecId::new("braw").unwrap()),
            FallbackPolicy::Disallow,
        )
        .unwrap()
        .primary()
        .clone();
    let mut decoder = backend
        .create_decoder(
            &DecoderConfig::new(source.info().streams()[0].clone()),
            &operation,
        )
        .unwrap();
    decoder.send_packet(packet, &operation).unwrap();
    let frame = match decoder.receive(&operation).unwrap() {
        DecodeOutput::Frame(frame) => frame,
        other => panic!("expected decoded frame, found {other:?}"),
    };
    assert_eq!(frame.format().pixel_format(), PixelFormat::Rgba16Float);
    assert_eq!(frame.format().color_space(), ColorSpace::ACESCG);
    assert_eq!(frame.format().alpha_mode(), AlphaMode::Straight);
    assert_eq!(frame.buffer().storage_kind(), FrameStorageKind::Cpu);
    assert_eq!(
        frame.metadata().get("vendor.iso"),
        Some(&MetadataValue::Unsigned(800))
    );
    let export = EncoderConfig::video(
        StreamId::new(1),
        CodecId::new("braw").unwrap(),
        Timebase::integer(24).unwrap(),
        frame.format(),
    );
    let error = match backend.create_encoder(&export, &operation) {
        Ok(_) => panic!("vendor RAW export unexpectedly created an encoder"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::Unsupported);

    assert_eq!(
        source
            .seek(
                SeekRequest::new(
                    RationalTime::new(1, Timebase::integer(24).unwrap()),
                    SeekMode::Exact,
                ),
                &operation,
            )
            .unwrap()
            .value(),
        1
    );
    decoder.reset(&operation).unwrap();
    decoder.flush(&operation).unwrap();
    assert!(matches!(
        decoder.receive(&operation).unwrap(),
        DecodeOutput::EndOfStream
    ));

    let mismatch = SourceRequest::new(
        MediaId::from_raw(28),
        SourceLocation::Memory {
            name: "relinked.braw".to_owned(),
            data: Arc::from([0x42_u8, 0x52, 0x41, 0x57]),
        },
    )
    .with_expected_fingerprint("sha256:different")
    .unwrap();
    let error = match backend.open_source(&mismatch, &operation) {
        Ok(_) => panic!("relink mismatch unexpectedly opened"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::Conflict);
}

#[test]
fn unresponsive_worker_is_terminated_at_the_handshake_deadline() {
    let worker = compile_worker();
    let operation = OperationContext::new(MediaPriority::Interactive);
    let config = VendorPluginConfig::new(worker)
        .with_arguments(["--hang"])
        .with_startup_timeout(StdDuration::from_millis(50))
        .unwrap();
    let mut registry = BackendRegistry::new();
    let error = register_vendor_plugins(&mut registry, &[config], &operation).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Timeout);
    assert_eq!(registry.registrations().count(), 0);
}

#[test]
fn malformed_worker_responses_fail_as_terminal_protocol_errors() {
    let worker = compile_worker();
    for argument in ["--invalid-json", "--unterminated"] {
        let operation = OperationContext::new(MediaPriority::Interactive)
            .with_timeout(StdDuration::from_secs(5))
            .unwrap();
        let config = VendorPluginConfig::new(worker.clone()).with_arguments([argument]);
        let mut registry = BackendRegistry::new();
        let error = register_vendor_plugins(&mut registry, &[config], &operation).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::CorruptData);
        assert_eq!(registry.registrations().count(), 0);
    }
}

#[test]
fn registration_is_atomic_and_protocol_messages_are_bounded() {
    let worker = compile_worker();
    let operation = OperationContext::new(MediaPriority::Interactive)
        .with_timeout(StdDuration::from_secs(5))
        .unwrap();
    let mut registry = BackendRegistry::new();
    let bounded = VendorPluginConfig::new(worker.clone())
        .with_maximum_message_bytes(128)
        .unwrap();
    let error = register_vendor_plugins(&mut registry, &[bounded], &operation).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(registry.registrations().count(), 0);

    let error = register_vendor_plugins(
        &mut registry,
        &[
            VendorPluginConfig::new(worker.clone()),
            VendorPluginConfig::new(worker),
        ],
        &operation,
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(registry.registrations().count(), 0);
}

#[test]
fn cancellation_precedes_executable_discovery() {
    let operation = OperationContext::new(MediaPriority::Interactive);
    operation.cancellation_token().cancel();
    let mut registry = BackendRegistry::new();
    let error = register_vendor_plugins(
        &mut registry,
        &[VendorPluginConfig::new("missing-vendor-worker")],
        &operation,
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(registry.registrations().count(), 0);
}

fn compile_worker() -> PathBuf {
    static WORKER: OnceLock<PathBuf> = OnceLock::new();
    WORKER
        .get_or_init(|| {
            let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            let source = manifest.join("tests/fixtures/mock_vendor_worker.rs");
            let directory =
                std::env::temp_dir().join(format!("superi-vendor-worker-{}", std::process::id()));
            std::fs::create_dir_all(&directory).unwrap();
            let executable = directory.join(if cfg!(windows) {
                "mock_vendor_worker.exe"
            } else {
                "mock_vendor_worker"
            });
            let status = Command::new("rustc")
                .arg("--edition=2021")
                .arg(&source)
                .arg("-o")
                .arg(&executable)
                .status()
                .unwrap();
            assert!(status.success());
            executable
        })
        .clone()
}
