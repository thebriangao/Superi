#![cfg(feature = "vendor-codecs")]

use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use superi_codecs_vendor::VendorPluginConfig;
use superi_engine::introspection::{
    MediaCapabilities, MediaCapabilityConstraint, MediaHardwareAcceleration, MediaOperation,
};
use superi_engine::media::{media_backend_registry, media_backend_registry_with_vendor_plugins};
use superi_media_io::operation::{MediaPriority, OperationContext};

#[test]
fn default_registry_is_vendor_free_and_explicit_registry_reaches_introspection() {
    let ordinary = media_backend_registry().unwrap();
    assert!(!ordinary.registrations().any(|registration| registration
        .backend()
        .descriptor()
        .id()
        .as_str()
        == "test-vendor-raw"));
    let ordinary_capabilities = MediaCapabilities::from_registry(&ordinary).unwrap();
    for codec in ["arriraw", "r3d", "braw"] {
        assert!(!ordinary_capabilities.operations().iter().any(|support| {
            support.operation()
                == &MediaOperation::Decode {
                    codec: codec.to_owned(),
                }
        }));
    }

    let worker = compile_worker();
    let operation = OperationContext::new(MediaPriority::Interactive)
        .with_timeout(Duration::from_secs(5))
        .unwrap();
    let explicit =
        media_backend_registry_with_vendor_plugins(&[VendorPluginConfig::new(worker)], &operation)
            .unwrap();
    let capabilities = MediaCapabilities::from_registry(&explicit).unwrap();
    let vendor = capabilities
        .backends()
        .iter()
        .find(|backend| backend.id() == "test-vendor-raw")
        .unwrap();
    assert_eq!(vendor.display_name(), "Test Vendor RAW");
    assert_eq!(
        vendor.hardware_acceleration(),
        MediaHardwareAcceleration::Unreported
    );
    for codec in ["arriraw", "r3d", "braw"] {
        let support = capabilities
            .operations()
            .iter()
            .find(|support| {
                support.operation()
                    == &MediaOperation::Decode {
                        codec: codec.to_owned(),
                    }
            })
            .unwrap();
        assert_eq!(support.primary_backends(), ["test-vendor-raw"]);
        let detail = vendor
            .codec_capabilities()
            .iter()
            .find(|detail| detail.operation() == support.operation())
            .unwrap();
        assert_eq!(detail.profiles(), &MediaCapabilityConstraint::Unreported);
    }
}

fn compile_worker() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source = manifest.join("../superi-codecs-vendor/tests/fixtures/mock_vendor_worker.rs");
    let directory = std::env::temp_dir().join(format!(
        "superi-engine-vendor-worker-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let executable = directory.join(if cfg!(windows) {
        "mock_vendor_worker.exe"
    } else {
        "mock_vendor_worker"
    });
    if !executable.exists() {
        let status = Command::new("rustc")
            .arg("--edition=2021")
            .arg(source)
            .arg("-o")
            .arg(&executable)
            .status()
            .unwrap();
        assert!(status.success());
    }
    executable
}
