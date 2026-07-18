use std::fs;
use std::path::PathBuf;

fn app_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("desktop crate must live below app")
        .join(relative)
}

fn source(relative: &str) -> String {
    fs::read_to_string(app_path(relative))
        .unwrap_or_else(|error| panic!("failed to read {relative}: {error}"))
}

fn require(haystack: &str, needle: &str, surface: &str) {
    assert!(
        haystack.contains(needle),
        "{surface} must contain {needle:?}"
    );
}

#[test]
fn offline_media_flows_preserve_c008_representation_contract() {
    let native = source("src-tauri/src/project_lifecycle.rs");
    for marker in [
        "pub enum OfflineMediaStatus",
        "pub struct OfflineMediaState",
        "pub enum OfflineMediaMutation",
        "pub struct OfflineMediaUpdate",
        "refresh_offline_state",
        "apply_offline_media",
        "mutate_project_offline_media",
        "derived_media",
        "representation_choice",
        "resolve_media_representation",
    ] {
        require(&native, marker, "native offline-media owner");
    }

    let bridge = source("src/project-lifecycle.ts");
    for marker in [
        "export type OfflineMediaStatus",
        "export interface OfflineMediaState",
        "export type OfflineMediaMutation",
        "mutateProjectOfflineMedia",
        "localSearchMedia",
    ] {
        require(&bridge, marker, "typed local bridge");
    }

    let consumer = source("src/App.tsx");
    for marker in [
        ">Offline media<",
        "Search local media",
        "mutateProjectOfflineMedia",
        "Relink source",
        "Replace source",
        "Conform source",
    ] {
        require(&consumer, marker, "production media-browser consumer");
    }

    assert!(!native.contains("filmstrip_generation") && !native.contains("waveform_generation"));
}
