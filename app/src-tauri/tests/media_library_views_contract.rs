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
        "{surface} must contain the frozen C004 contract marker {needle:?}"
    );
}

#[test]
fn bins_views_and_replaceable_thumbnails_share_the_imported_media_identity() {
    let native = source("src-tauri/src/project_lifecycle.rs");
    for marker in [
        "pub struct MediaLibrarySnapshot",
        "pub struct MediaBrowserItem",
        "pub struct MediaBinView",
        "pub struct SmartCollectionView",
        "pub enum MediaLibraryMutation",
        "pub enum ThumbnailPresentation",
        "project_media_library",
        "mutate_project_media_library",
        "thumbnail_fallback",
    ] {
        require(&native, marker, "native project lifecycle");
    }

    let bridge = source("src/project-lifecycle.ts");
    for marker in [
        "export type MediaLibrarySnapshot",
        "export type MediaLibraryMutation",
        "readProjectMediaLibrary",
        "mutateProjectMediaLibrary",
        "thumbnail_fallback",
    ] {
        require(&bridge, marker, "typed application bridge");
    }

    let consumer = source("src/App.tsx");
    for marker in [
        "data-testid=\"media-browser\"",
        ">Bins<",
        ">Smart collections<",
        ">List<",
        ">Grid<",
        ">Metadata<",
        "thumbnail_fallback",
    ] {
        require(&consumer, marker, "production editing workspace");
    }

    require(
        &native,
        "media_id",
        "thumbnail identity must attach to imported media",
    );
    require(
        &native,
        "freshness",
        "replaceable thumbnails must reject stale derived state",
    );
    assert!(
        !native.contains("proxy_path") && !native.contains("search_index"),
        "C004 must not absorb later proxy or search ownership"
    );
}
