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
        "{surface} must contain the frozen C006 contract marker {needle:?}"
    );
}

#[test]
fn editorial_annotations_and_derived_usage_extend_c005_media_identity() {
    let native = source("src-tauri/src/project_lifecycle.rs");
    for marker in [
        "pub struct MediaEditorialAnnotations",
        "pub struct MediaUsageIndicator",
        "pub struct MediaAnnotationUpdate",
        "mutate_project_media_annotations",
        "apply_annotations",
        "derive_media_usage",
        "ClipSource::Media",
        "source_metadata",
        "user_metadata",
    ] {
        require(&native, marker, "native media annotation owner");
    }

    let bridge = source("src/project-lifecycle.ts");
    for marker in [
        "export interface MediaEditorialAnnotations",
        "export interface MediaUsageIndicator",
        "mutateProjectMediaAnnotations",
        "annotations",
        "usage",
    ] {
        require(&bridge, marker, "typed application bridge");
    }

    let consumer = source("src/App.tsx");
    for marker in [
        ">Editorial annotations<",
        ">Usage<",
        "media-annotation-editor",
        "mutateProjectMediaAnnotations",
        "selectedMedia.usage",
    ] {
        require(&consumer, marker, "production media-browser consumer");
    }

    for invariant in [
        "expected_project_revision",
        "expected_library_revision",
        "content_fingerprint",
        "bin_id",
    ] {
        require(
            &native,
            invariant,
            "revision, identity, and organization preservation",
        );
    }
    assert!(
        !native.contains("duplicate_group") && !native.contains("duplicate_score"),
        "C006 must not absorb C007 duplicate detection"
    );
}
