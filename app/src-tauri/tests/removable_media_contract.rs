use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;
use superi_desktop::project_lifecycle::{
    DesktopMediaImportOrigin, DesktopMediaImportRequest, DesktopProjectCommand,
    DesktopProjectCreateRequest, DesktopProjectState, MediaPreviewRequest, MediaSourceScanRequest,
    OfflineMediaMutation, OfflineMediaUpdate,
};

fn owned_test_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must follow the Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "superi-source-monitor-{label}-{}-{nonce}",
        std::process::id()
    ))
}

fn create_project(state: &DesktopProjectState, root: &Path, project_id: &str) {
    state
        .initialize(root.join("recovery"))
        .expect("desktop state must initialize");
    state
        .execute(DesktopProjectCommand::Create {
            path: root.join("monitor.superi").to_string_lossy().into_owned(),
            project: DesktopProjectCreateRequest {
                project_id: project_id.into(),
                project_name: "Source Monitor Contract".into(),
                root_timeline_id: "timeline:00000000000000000000000000011313".into(),
                root_timeline_name: "Source Monitor Timeline".into(),
                edit_rate_numerator: 24,
                edit_rate_denominator: 1,
            },
        })
        .expect("test project must be created");
}

fn import_request(path: &Path) -> DesktopMediaImportRequest {
    DesktopMediaImportRequest {
        expected_project_revision: 0,
        origin: DesktopMediaImportOrigin::Api,
        paths: vec![path.to_string_lossy().into_owned()],
        recursive: false,
        detect_image_sequences: false,
    }
}

fn scan_request(
    library_revision: u64,
    media_id: &str,
    verify_content: bool,
) -> MediaSourceScanRequest {
    MediaSourceScanRequest {
        expected_project_revision: 1,
        expected_library_revision: library_revision,
        media_ids: vec![media_id.to_owned()],
        verify_content,
    }
}

fn item(snapshot: &Value) -> &Value {
    &snapshot["items"][0]
}

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
        "{surface} must contain the frozen C013 contract marker {needle:?}"
    );
}

#[test]
fn source_monitoring_extends_the_stable_media_identity_through_the_real_consumer() {
    let native = format!(
        "{}\n{}",
        source("src-tauri/src/project_lifecycle.rs"),
        source("src-tauri/src/project_lifecycle/source_monitoring.rs")
    );
    for marker in [
        "pub enum MediaVolumeKind",
        "pub enum MediaVolumeStatus",
        "pub enum MediaSourcePathStatus",
        "pub enum MediaRelinkIntent",
        "pub struct MediaSourceFingerprint",
        "pub struct MediaSourcePathState",
        "pub struct MediaSourceMonitoring",
        "pub struct MediaSourceScanRequest",
        "source_monitoring",
        "verify_content",
        "scan_media_sources",
        "scan_project_media_sources",
    ] {
        require(&native, marker, "native removable-media owner");
    }

    let host = source("src-tauri/src/lib.rs");
    require(
        &host,
        "project_lifecycle::scan_project_media_sources",
        "Tauri command registry",
    );

    let bridge = source("src/project-lifecycle.ts");
    for marker in [
        "export type MediaVolumeKind",
        "export type MediaSourcePathStatus",
        "export type MediaRelinkIntent",
        "export interface MediaSourceMonitoring",
        "export interface MediaSourceScanRequest",
        "scanProjectMediaSources",
        "scan_project_media_sources",
        "verify_content",
    ] {
        require(&bridge, marker, "typed application bridge");
    }

    let consumer = source("src/App.tsx");
    for marker in [
        "data-testid=\"source-monitoring\"",
        ">Check all sources<",
        ">Verify selected source bytes<",
        "scanProjectMediaSources",
        "source_monitoring.relink_intent",
    ] {
        require(&consumer, marker, "production media-browser consumer");
    }

    for identity in [
        "media_id",
        "content_fingerprint",
        "source_metadata",
        "user_metadata",
        "content_analysis",
        "derived_media",
        "representation_choice",
    ] {
        require(
            &native,
            identity,
            "preserved media identity and editorial state",
        );
    }

    assert!(
        !native.contains("use notify") && !native.contains("TcpStream"),
        "C013 must remain an explicit offline filesystem transaction without a watcher or network owner"
    );
}

#[test]
fn exact_scans_preserve_editor_state_across_change_loss_restore_and_restart() {
    let root = owned_test_root("change");
    fs::create_dir_all(&root).expect("owned root must be created");
    let source_path = root.join("camera.mov");
    fs::write(&source_path, b"source-version-a").expect("source must be written");

    let state = DesktopProjectState::default();
    create_project(&state, &root, "project:00000000000000000000000000001313");
    let imported = state
        .import_media(import_request(&source_path))
        .expect("source must import");
    let media_id = imported.imported()[0].media_id().to_owned();
    let freshness = imported.imported()[0].content_fingerprint().to_owned();
    let initial = serde_json::to_value(state.media_library().expect("library must load"))
        .expect("library must serialize");
    assert_eq!(initial["revision"], 1);
    assert_eq!(item(&initial)["source_monitoring"]["status"], "ready");
    assert_eq!(
        item(&initial)["source_monitoring"]["paths"][0]["status"],
        "unchanged"
    );

    let stable_fields = [
        "media_id",
        "name",
        "content_fingerprint",
        "bin_id",
        "source_metadata",
        "user_metadata",
        "annotations",
        "content_analysis",
        "selections",
        "derived_media",
        "representation_choice",
    ]
    .into_iter()
    .map(|field| (field, item(&initial)[field].clone()))
    .collect::<Vec<_>>();

    fs::write(&source_path, b"source-version-b").expect("source bytes must change");
    let changed = state
        .scan_media_sources(scan_request(1, &media_id, true))
        .expect("exact source scan must complete");
    let changed = serde_json::to_value(changed).expect("changed snapshot must serialize");
    assert_eq!(changed["revision"], 2);
    assert_eq!(item(&changed)["source_monitoring"]["status"], "attention");
    assert_eq!(
        item(&changed)["source_monitoring"]["relink_intent"],
        "review_changed_source"
    );
    assert_eq!(
        item(&changed)["source_monitoring"]["paths"][0]["status"],
        "changed"
    );
    assert_ne!(
        item(&changed)["source_monitoring"]["paths"][0]["baseline"]["content_fingerprint"],
        item(&changed)["source_monitoring"]["paths"][0]["observed"]["content_fingerprint"]
    );
    for (field, value) in &stable_fields {
        assert_eq!(
            &item(&changed)[*field],
            value,
            "source scan must preserve {field}"
        );
    }

    let preview = state
        .generate_media_preview(MediaPreviewRequest {
            expected_project_revision: 1,
            expected_library_revision: 2,
            media_id: media_id.clone(),
            expected_freshness: freshness.clone(),
        })
        .expect_err("changed bytes must not be presented under the accepted fingerprint");
    assert_eq!(preview.code(), "media_preview_source_changed");

    let before_stale = serde_json::to_value(state.media_library().unwrap()).unwrap();
    let stale = state
        .scan_media_sources(scan_request(1, &media_id, true))
        .expect_err("stale source scans must reject atomically");
    assert_eq!(stale.code(), "media_library_revision_stale");
    assert_eq!(
        serde_json::to_value(state.media_library().unwrap()).unwrap(),
        before_stale
    );

    fs::write(&source_path, b"source-version-a").expect("accepted bytes must return");
    let restored = state
        .scan_media_sources(scan_request(2, &media_id, true))
        .expect("restored accepted bytes must scan");
    let restored = serde_json::to_value(restored).unwrap();
    assert_eq!(item(&restored)["source_monitoring"]["status"], "ready");
    assert_eq!(
        item(&restored)["source_monitoring"]["relink_intent"],
        "none"
    );

    fs::remove_file(&source_path).expect("source must become missing");
    let missing = state
        .scan_media_sources(scan_request(3, &media_id, false))
        .expect("missing source must become inspectable state");
    let missing = serde_json::to_value(missing).unwrap();
    assert_eq!(
        item(&missing)["source_monitoring"]["relink_intent"],
        "locate_source"
    );
    assert_eq!(
        item(&missing)["source_monitoring"]["paths"][0]["status"],
        "missing"
    );

    fs::write(&source_path, b"source-version-a").expect("missing source must return");
    let returned = state
        .scan_media_sources(scan_request(4, &media_id, true))
        .expect("returned source must recover");
    let returned = serde_json::to_value(returned).unwrap();
    assert_eq!(item(&returned)["source_monitoring"]["status"], "ready");
    assert_eq!(item(&returned)["source_monitoring"]["scan_generation"], 4);

    let sidecar = fs::read_to_string(root.join("recovery/media-library-views.json"))
        .expect("source monitoring sidecar must be readable");
    assert!(sidecar.contains("\"source_monitoring\""));
    assert!(sidecar.contains(&freshness));

    let reopened = DesktopProjectState::default();
    reopened
        .initialize(root.join("recovery"))
        .expect("persisted source monitoring must initialize");
    reopened
        .execute(DesktopProjectCommand::Open {
            path: root.join("monitor.superi").to_string_lossy().into_owned(),
        })
        .expect("project must reopen");
    let reopened = serde_json::to_value(reopened.media_library().unwrap()).unwrap();
    assert_eq!(
        item(&reopened)["source_monitoring"],
        item(&returned)["source_monitoring"]
    );

    fs::remove_dir_all(root).expect("owned root must be removable");
}

#[test]
fn absent_conventional_removable_volume_retains_wait_for_volume_intent() {
    let root = owned_test_root("volume");
    fs::create_dir_all(&root).expect("owned root must be created");
    let source_path = root.join("camera.mov");
    fs::write(&source_path, b"removable-source").expect("source must be written");

    let state = DesktopProjectState::default();
    create_project(&state, &root, "project:00000000000000000000000000002313");
    let imported = state
        .import_media(import_request(&source_path))
        .expect("source must import");
    let media_id = imported.imported()[0].media_id().to_owned();
    let freshness = imported.imported()[0].content_fingerprint().to_owned();
    let absent = format!(
        "/Volumes/Superi-C013-Absent-{}/camera.mov",
        std::process::id()
    );
    assert!(!Path::new(&absent).exists());

    let relinked = state
        .mutate_offline_media(OfflineMediaUpdate {
            expected_project_revision: 1,
            expected_library_revision: 1,
            media_id: media_id.clone(),
            mutation: OfflineMediaMutation::Relink {
                source_paths: vec![absent.clone()],
                candidate_fingerprint: freshness.clone(),
            },
        })
        .expect("relink intent must preserve the stable identity");
    let relinked = serde_json::to_value(relinked).unwrap();
    assert_eq!(item(&relinked)["media_id"], media_id);
    assert_eq!(item(&relinked)["content_fingerprint"], freshness);
    assert_eq!(item(&relinked)["source_monitoring"]["status"], "unchecked");

    let scanned = state
        .scan_media_sources(scan_request(2, &media_id, true))
        .expect("offline removable volume must become inspectable state");
    let scanned = serde_json::to_value(scanned).unwrap();
    let monitoring = &item(&scanned)["source_monitoring"];
    assert_eq!(monitoring["status"], "attention");
    assert_eq!(monitoring["relink_intent"], "wait_for_volume");
    assert_eq!(monitoring["paths"][0]["status"], "volume_offline");
    assert_eq!(monitoring["paths"][0]["volume"]["kind"], "removable");
    assert_eq!(monitoring["paths"][0]["volume"]["status"], "offline");
    assert_eq!(monitoring["paths"][0]["path"], absent);
    assert_eq!(item(&scanned)["media_id"], media_id);
    assert_eq!(item(&scanned)["content_fingerprint"], freshness);

    fs::remove_dir_all(root).expect("owned root must be removable");
}
