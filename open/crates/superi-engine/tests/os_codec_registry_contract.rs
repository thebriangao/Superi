#![cfg(feature = "os-codecs")]

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
}
