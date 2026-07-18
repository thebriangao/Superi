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
        "{surface} must contain the frozen C008 contract marker {needle:?}"
    );
}

#[test]
fn derived_media_lifecycle_is_transparent_and_preserves_c007_identity() {
    let native = source("src-tauri/src/project_lifecycle.rs");
    for marker in [
        "pub enum DerivedMediaPurpose",
        "pub enum DerivedMediaQuality",
        "pub enum DerivedMediaStatus",
        "pub enum MediaRepresentationChoice",
        "pub struct DerivedMediaAttachment",
        "pub struct DerivedMediaUpdate",
        "normalize_derived_media",
        "resolve_media_representation",
        "mutate_project_derived_media",
        "content_fingerprint",
        "identity_tracking",
        "selections",
    ] {
        require(&native, marker, "native C008 derived-media owner");
    }

    let bridge = source("src/project-lifecycle.ts");
    for marker in [
        "export type DerivedMediaPurpose",
        "export type DerivedMediaQuality",
        "export type DerivedMediaStatus",
        "export type MediaRepresentationChoice",
        "export interface DerivedMediaAttachment",
        "mutateProjectDerivedMedia",
    ] {
        require(&bridge, marker, "typed application bridge");
    }

    let consumer = source("src/App.tsx");
    for marker in [
        ">Proxy and optimized media<",
        "mutateProjectDerivedMedia",
        "resolved_representation",
        "Create or replace proxy",
        "Use original",
    ] {
        require(&consumer, marker, "production media-browser consumer");
    }

    assert!(
        !native.contains("relink_candidates")
            && !native.contains("replace_source_media")
            && !native.contains("conform_media")
            && !native.contains("search_index"),
        "C008 must not absorb C009 or later media work"
    );
}
