use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use superi_desktop::project_lifecycle::{
    DerivedMediaQuality, DerivedMediaStatus, DesktopMediaImportOrigin, DesktopMediaImportRequest,
    DesktopProjectCommand, DesktopProjectCreateRequest, DesktopProjectState, MediaBatchOperation,
    MediaBatchUpdate, MediaLibraryMutation, MediaLibraryUpdate,
};

fn owned_test_root() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time must follow the Unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("superi-media-batch-{}-{nonce}", std::process::id()))
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
        "{surface} must contain the frozen C012 marker {needle:?}"
    );
}

fn snapshot_item<'a>(snapshot: &'a Value, media_id: &str) -> &'a Value {
    snapshot["items"]
        .as_array()
        .expect("snapshot items must be an array")
        .iter()
        .find(|item| item["media_id"] == media_id)
        .unwrap_or_else(|| panic!("snapshot must contain media {media_id}"))
}

fn derived_attachment<'a>(item: &'a Value, purpose: &str) -> &'a Value {
    item["derived_media"]
        .as_array()
        .expect("derived media must be an array")
        .iter()
        .find(|attachment| attachment["purpose"] == purpose)
        .unwrap_or_else(|| panic!("item must contain {purpose} derived media"))
}

fn import_request(paths: &[&Path]) -> DesktopMediaImportRequest {
    DesktopMediaImportRequest {
        expected_project_revision: 0,
        origin: DesktopMediaImportOrigin::Api,
        paths: paths
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect(),
        recursive: false,
        detect_image_sequences: false,
    }
}

#[test]
fn ordered_batch_is_atomic_durable_and_transparent_about_derived_fallback() {
    let root = owned_test_root();
    let recovery_root = root.join("recovery");
    let project_path = root.join("batch.superi");
    let first_source = root.join("camera-a.mov");
    let second_source = root.join("sound-b.wav");
    let relocated_root = root.join("relocated");
    let relocated_first = relocated_root.join("camera-a.mov");
    fs::create_dir_all(&relocated_root).expect("test source root must be created");
    fs::write(&first_source, b"superi-camera-a").expect("first test source must be written");
    fs::write(&second_source, b"superi-sound-b").expect("second test source must be written");
    fs::write(&relocated_first, b"superi-camera-a").expect("relocated test source must be written");

    let state = DesktopProjectState::default();
    state
        .initialize(recovery_root.clone())
        .expect("desktop state must initialize");
    state
        .execute(DesktopProjectCommand::Create {
            path: project_path.to_string_lossy().into_owned(),
            project: DesktopProjectCreateRequest {
                project_id: "project:00000000000000000000000000000312".into(),
                project_name: "Batch Contract".into(),
                root_timeline_id: "timeline:00000000000000000000000000010312".into(),
                root_timeline_name: "Batch Timeline".into(),
                edit_rate_numerator: 24,
                edit_rate_denominator: 1,
            },
        })
        .expect("test project must be created");
    let imported = state
        .import_media(import_request(&[&first_source, &second_source]))
        .expect("test media must import");
    assert_eq!(imported.project_revision(), 1);
    assert_eq!(imported.imported().len(), 2);

    let media_ids: Vec<String> = imported
        .imported()
        .iter()
        .map(|media| media.media_id().to_owned())
        .collect();
    let initial = serde_json::to_value(state.media_library().expect("library must load"))
        .expect("initial library must serialize");
    assert_eq!(initial["revision"], 1);
    let fingerprints: Vec<String> = media_ids
        .iter()
        .map(|media_id| {
            snapshot_item(&initial, media_id)["content_fingerprint"]
                .as_str()
                .expect("fingerprint must be text")
                .to_owned()
        })
        .collect();

    let organized = state
        .mutate_media_library(MediaLibraryUpdate {
            expected_project_revision: 1,
            expected_library_revision: 1,
            mutation: MediaLibraryMutation::CreateBin {
                bin_id: "bin:interviews".into(),
                name: "Interviews".into(),
                parent_id: None,
            },
        })
        .expect("batch target bin must be created");
    let organized = serde_json::to_value(organized).expect("organized library must serialize");
    assert_eq!(organized["revision"], 2);

    let result = state
        .mutate_media_batch(MediaBatchUpdate {
            expected_project_revision: 1,
            expected_library_revision: 2,
            operations: vec![
                MediaBatchOperation::Rename {
                    media_id: media_ids[0].clone(),
                    name: "Interview 001".into(),
                },
                MediaBatchOperation::Rename {
                    media_id: media_ids[1].clone(),
                    name: "Interview 002".into(),
                },
                MediaBatchOperation::Organize {
                    media_id: media_ids[0].clone(),
                    bin_id: Some("bin:interviews".into()),
                },
                MediaBatchOperation::Organize {
                    media_id: media_ids[1].clone(),
                    bin_id: Some("bin:interviews".into()),
                },
                MediaBatchOperation::Transcode {
                    media_id: media_ids[0].clone(),
                    artifact_id: "transcode:camera-a:full".into(),
                    quality: DerivedMediaQuality::Full,
                    status: DerivedMediaStatus::Generating,
                    source_fingerprint: fingerprints[0].clone(),
                    source_revision: 1,
                    byte_len: 0,
                    select: true,
                },
                MediaBatchOperation::Proxy {
                    media_id: media_ids[1].clone(),
                    artifact_id: "proxy:sound-b:quarter".into(),
                    quality: DerivedMediaQuality::Quarter,
                    status: DerivedMediaStatus::Generating,
                    source_fingerprint: fingerprints[1].clone(),
                    source_revision: 1,
                    byte_len: 0,
                    select: true,
                },
                MediaBatchOperation::Relink {
                    media_id: media_ids[0].clone(),
                    source_paths: vec![relocated_first.to_string_lossy().into_owned()],
                    candidate_fingerprint: fingerprints[0].clone(),
                },
                MediaBatchOperation::MetadataUpsert {
                    media_id: media_ids[0].clone(),
                    key: "production.unit".into(),
                    value: "alpha".into(),
                },
                MediaBatchOperation::MetadataUpsert {
                    media_id: media_ids[1].clone(),
                    key: "production.unit".into(),
                    value: "beta".into(),
                },
            ],
        })
        .expect("valid media batch must commit");
    let result = serde_json::to_value(result).expect("batch result must serialize");
    assert_eq!(result["operation_count"], 9);
    assert_eq!(
        result["affected_media_ids"],
        json!([media_ids[0], media_ids[1]])
    );
    let snapshot = &result["snapshot"];
    assert_eq!(snapshot["revision"], 3);

    let first = snapshot_item(snapshot, &media_ids[0]);
    assert_eq!(first["name"], "Interview 001");
    assert_eq!(first["bin_id"], "bin:interviews");
    assert_eq!(first["content_fingerprint"], fingerprints[0]);
    assert_eq!(first["source_paths"], json!([relocated_first]));
    assert_eq!(first["user_metadata"]["production.unit"], "alpha");
    assert_eq!(first["representation_choice"]["kind"], "optimized");
    assert_eq!(
        first["resolved_representation"]["representation"],
        "original"
    );
    assert_eq!(
        first["resolved_representation"]["fallback_to_original"],
        true
    );
    let transcode = derived_attachment(first, "optimized");
    assert_eq!(transcode["quality"], "full");
    assert_eq!(transcode["status"], "generating");
    assert_eq!(transcode["source_fingerprint"], fingerprints[0]);

    let second = snapshot_item(snapshot, &media_ids[1]);
    assert_eq!(second["name"], "Interview 002");
    assert_eq!(second["bin_id"], "bin:interviews");
    assert_eq!(second["content_fingerprint"], fingerprints[1]);
    assert_eq!(second["user_metadata"]["production.unit"], "beta");
    assert_eq!(second["representation_choice"]["kind"], "proxy");
    assert_eq!(
        second["resolved_representation"]["representation"],
        "original"
    );
    assert_eq!(
        second["resolved_representation"]["fallback_to_original"],
        true
    );
    let proxy = derived_attachment(second, "proxy");
    assert_eq!(proxy["quality"], "quarter");
    assert_eq!(proxy["status"], "generating");
    assert_eq!(proxy["source_fingerprint"], fingerprints[1]);

    let failed = state.mutate_media_batch(MediaBatchUpdate {
        expected_project_revision: 1,
        expected_library_revision: 3,
        operations: vec![
            MediaBatchOperation::Rename {
                media_id: media_ids[0].clone(),
                name: "Must Roll Back".into(),
            },
            MediaBatchOperation::MetadataRemove {
                media_id: media_ids[1].clone(),
                key: "missing.key".into(),
            },
        ],
    });
    let failed = failed.expect_err("one invalid operation must reject the batch");
    assert_eq!(failed.context("batch_operation_index"), Some("1"));
    let after_failure = serde_json::to_value(
        state
            .media_library()
            .expect("library must remain available after rejection"),
    )
    .expect("post-failure library must serialize");
    assert_eq!(after_failure["revision"], 3);
    assert_eq!(
        snapshot_item(&after_failure, &media_ids[0])["name"],
        "Interview 001"
    );

    let sidecar = fs::read_to_string(recovery_root.join("media-library-views.json"))
        .expect("media-library sidecar must be readable");
    for derived_only_field in [
        "\"usage\"",
        "\"identity_tracking\"",
        "\"resolved_representation\"",
        "\"offline\"",
        "\"thumbnail\"",
        "\"media_ids\"",
    ] {
        assert!(
            !sidecar.contains(derived_only_field),
            "sidecar must not persist derived-only field {derived_only_field}"
        );
    }

    let restored = DesktopProjectState::default();
    restored
        .initialize(recovery_root)
        .expect("restored desktop state must initialize");
    restored
        .execute(DesktopProjectCommand::Open {
            path: project_path.to_string_lossy().into_owned(),
        })
        .expect("test project must reopen");
    let restored = serde_json::to_value(
        restored
            .media_library()
            .expect("restored media library must load"),
    )
    .expect("restored library must serialize");
    assert_eq!(restored["revision"], 3);
    assert_eq!(
        snapshot_item(&restored, &media_ids[0])["name"],
        "Interview 001"
    );
    assert_eq!(
        snapshot_item(&restored, &media_ids[1])["user_metadata"]["production.unit"],
        "beta"
    );

    fs::remove_dir_all(root).expect("owned test root must be removable");
}

#[test]
fn native_bridge_and_real_editor_consumer_expose_every_c012_operation() {
    let native = source("src-tauri/src/project_lifecycle.rs");
    for marker in [
        "pub enum MediaBatchOperation",
        "pub struct MediaBatchUpdate",
        "pub struct MediaBatchResult",
        "MediaBatchOperation::Rename",
        "MediaBatchOperation::Organize",
        "MediaBatchOperation::Transcode",
        "MediaBatchOperation::Proxy",
        "MediaBatchOperation::Relink",
        "MediaBatchOperation::MetadataUpsert",
        "MediaBatchOperation::MetadataRemove",
        "apply_media_batch",
        "mutate_project_media_batch",
    ] {
        require(&native, marker, "native media batch owner");
    }

    let host = source("src-tauri/src/lib.rs");
    require(
        &host,
        "project_lifecycle::mutate_project_media_batch",
        "Tauri command registry",
    );

    let bridge = source("src/project-lifecycle.ts");
    for marker in [
        "export type MediaBatchOperation",
        "export interface MediaBatchUpdate",
        "export interface MediaBatchResult",
        "mutateProjectMediaBatch",
        "metadata_upsert",
        "metadata_remove",
    ] {
        require(&bridge, marker, "typed application bridge");
    }

    let consumer = source("src/App.tsx");
    for marker in [
        "data-testid=\"media-batch-operations\"",
        ">Batch rename<",
        ">Organize selected<",
        ">Queue optimized transcode<",
        ">Queue proxy<",
        ">Batch relink<",
        ">Update metadata<",
        "mutateProjectMediaBatch",
    ] {
        require(&consumer, marker, "production media-browser consumer");
    }
}
