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
        "{surface} must contain the frozen C005 contract marker {needle:?}"
    );
}

#[test]
fn source_inspection_and_user_metadata_extend_the_authoritative_media_identity() {
    let native = source("src-tauri/src/project_lifecycle.rs");
    for marker in [
        "pub enum SourceMetadataStatus",
        "pub struct SourceMetadataInspection",
        "pub enum UserMetadataMutation",
        "pub struct UserMetadataUpdate",
        "source_metadata",
        "user_metadata",
        "expected_freshness",
        "inspection_generation",
        "inspect_project_media_source",
        "mutate_project_media_metadata",
    ] {
        require(&native, marker, "native media metadata owner");
    }

    let bridge = source("src/project-lifecycle.ts");
    for marker in [
        "export type SourceMetadataStatus",
        "export interface SourceMetadataInspection",
        "export type UserMetadataMutation",
        "inspectProjectMediaSource",
        "mutateProjectMediaMetadata",
        "source_metadata",
        "user_metadata",
    ] {
        require(&bridge, marker, "typed application bridge");
    }

    let consumer = source("src/App.tsx");
    for marker in [
        ">Source metadata<",
        ">User metadata<",
        "source-metadata-status",
        "user-metadata-editor",
        "inspectProjectMediaSource",
        "mutateProjectMediaMetadata",
    ] {
        require(&consumer, marker, "production media-browser consumer");
    }

    for invariant in ["media_id", "content_fingerprint", "bin_id"] {
        require(&native, invariant, "stable identity and organization state");
    }
    assert!(
        !native.contains("user_rating")
            && !native.contains("user_keywords")
            && !native.contains("duplicate_group"),
        "C005 must not absorb C006 annotations or C007 duplicate detection"
    );
}
