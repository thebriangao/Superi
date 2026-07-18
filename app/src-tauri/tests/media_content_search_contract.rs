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
fn content_search_keeps_language_analysis_editable_and_offline() {
    let native = source("src-tauri/src/project_lifecycle.rs");
    for marker in [
        "pub struct MediaContentAnalysis",
        "pub struct MediaTranscriptSegment",
        "pub struct MediaTimelineRelationship",
        "pub struct MediaLocalAiContent",
        "pub struct MediaContentAnalysisUpdate",
        "pub struct MediaContentSearchRequest",
        "pub enum MediaContentSearchSignal",
        "apply_content_analysis",
        "search_content",
        "mutate_project_media_content_analysis",
        "search_project_media_content",
        "source_fingerprint",
        "start_frame",
        "end_frame",
        "speaker",
        "timeline_id",
        "clip_id",
    ] {
        require(&native, marker, "native content-search owner");
    }

    let host = source("src-tauri/src/lib.rs");
    for marker in [
        "mutate_project_media_content_analysis",
        "search_project_media_content",
    ] {
        require(&host, marker, "Tauri command registration");
    }

    let bridge = source("src/project-lifecycle.ts");
    for marker in [
        "export interface MediaContentAnalysis",
        "export interface MediaTranscriptSegment",
        "export interface MediaContentSearchMatch",
        "mutateProjectMediaContentAnalysis",
        "searchProjectMediaContent",
    ] {
        require(&bridge, marker, "typed local bridge");
    }

    let consumer = source("src/App.tsx");
    for marker in [
        "Search metadata, transcript, or local AI content",
        "Editable language analysis",
        "mutateProjectMediaContentAnalysis",
        "searchProjectMediaContent",
        "Transcript segments",
        "Local AI content",
        "Clear content analysis",
    ] {
        require(&consumer, marker, "production media-browser consumer");
    }

    for forbidden in [
        "reqwest",
        "remote inference",
        "server fallback",
        "Superi Max",
        "https://",
    ] {
        assert!(
            !native.contains(forbidden),
            "native content search must remain offline and model-independent: {forbidden}"
        );
    }
}
