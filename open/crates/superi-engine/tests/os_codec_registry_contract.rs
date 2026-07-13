#![cfg(feature = "os-codecs")]

use superi_engine::introspection::{MediaCapabilities, MediaHardwareAcceleration, MediaOperation};
use superi_engine::media::media_backend_registry;

#[test]
fn engine_registry_includes_default_and_host_discovered_platform_backends() {
    let registry = media_backend_registry().unwrap();
    let ids = registry
        .registrations()
        .map(|registration| registration.backend().descriptor().id().as_str())
        .collect::<Vec<_>>();

    assert!(ids.contains(&"rust-av1"));
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    assert!(!ids.iter().any(|id| id.contains("media-foundation")));
    #[cfg(target_os = "windows")]
    if ids.contains(&"windows-media-foundation") {
        let registration = registry
            .registrations()
            .find(|registration| {
                registration.backend().descriptor().id().as_str() == "windows-media-foundation"
            })
            .unwrap();
        assert!(!registration.capabilities().is_empty());
    }

    let capabilities = MediaCapabilities::from_registry(&registry).unwrap();
    let av1 = capabilities
        .backends()
        .iter()
        .find(|backend| backend.id() == "rust-av1")
        .unwrap();
    assert_eq!(
        av1.hardware_acceleration(),
        MediaHardwareAcceleration::Software
    );
    assert!(av1.codec_capabilities().iter().any(|detail| {
        detail.operation()
            == &MediaOperation::Encode {
                codec: "av1".to_owned(),
            }
    }));

    #[cfg(target_os = "macos")]
    {
        let platform = capabilities
            .backends()
            .iter()
            .find(|backend| backend.id() == "apple-videotoolbox")
            .unwrap();
        assert_eq!(
            platform.hardware_acceleration(),
            MediaHardwareAcceleration::PlatformManaged
        );
        assert!(platform.codec_capabilities().iter().any(|detail| {
            detail.operation()
                == &MediaOperation::Encode {
                    codec: "h264".to_owned(),
                }
        }));
    }

    #[cfg(target_os = "windows")]
    if let Some(platform) = capabilities
        .backends()
        .iter()
        .find(|backend| backend.id() == "windows-media-foundation")
    {
        assert_eq!(
            platform.hardware_acceleration(),
            MediaHardwareAcceleration::Software
        );
    }

    #[cfg(target_os = "linux")]
    if let Some(platform) = capabilities
        .backends()
        .iter()
        .find(|backend| backend.id() == "linux-vaapi")
    {
        assert_eq!(
            platform.hardware_acceleration(),
            MediaHardwareAcceleration::Hardware
        );
    }
}
