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
        "{surface} must contain the frozen C007 contract marker {needle:?}"
    );
}

#[test]
fn duplicate_identity_and_editable_tracked_selections_extend_c006_state() {
    let native = source("src-tauri/src/project_lifecycle.rs");
    for marker in [
        "pub struct MediaIdentityTracking",
        "pub struct MediaSelection",
        "pub struct MediaTrackedRegion",
        "pub struct TrackedRegionObservation",
        "pub struct MediaIdentityUpdate",
        "refresh_identity_tracking",
        "apply_identity_update",
        "mutate_project_media_identity",
        "content_fingerprint",
        "annotations",
        "usage",
    ] {
        require(&native, marker, "native C007 identity owner");
    }

    let bridge = source("src/project-lifecycle.ts");
    for marker in [
        "export interface MediaIdentityTracking",
        "export interface MediaSelection",
        "export interface MediaTrackedRegion",
        "export interface TrackedRegionObservation",
        "mutateProjectMediaIdentity",
    ] {
        require(&bridge, marker, "typed application bridge");
    }

    let consumer = source("src/App.tsx");
    for marker in [
        ">Identity and selections<",
        "mutateProjectMediaIdentity",
        "duplicate_media_ids",
        "tracked_regions",
        "Refine tracked region",
    ] {
        require(&consumer, marker, "production media-browser consumer");
    }

    assert!(
        !native.contains("proxy_status")
            && !native.contains("optimized_media")
            && !native.contains("relink_candidates")
            && !native.contains("search_index"),
        "C007 must not absorb C008 or later media work"
    );
}
