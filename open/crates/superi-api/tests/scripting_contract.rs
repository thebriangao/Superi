use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use superi_api::commands::{ApiCommand, GetEditorState};
use superi_api::editor::ProjectEditorApi;
use superi_api::permissions::{ApiPermissionKind, ApiPermissionRequirementMode};
use superi_api::scripting::{
    ProjectScriptRunStatus, RunProjectScript, ScriptId, MAX_SCRIPT_IDENTIFIER_BYTES,
    MAX_SCRIPT_JSON_DEPTH, MAX_SCRIPT_SOURCE_BYTES, MAX_SCRIPT_STEPS, RUN_PROJECT_SCRIPT_METHOD,
    SCRIPTING_SCHEMA_VERSION,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::ErrorCategory;
use superi_engine::editor as engine;

const PROJECT: engine::ProjectId = engine::ProjectId::from_raw(0xc022_0001);
const MEDIA: engine::MediaId = engine::MediaId::from_raw(0xc022_0002);
const ROOT: engine::TimelineId = engine::TimelineId::from_raw(0xc022_0003);
const TRACK: engine::TrackId = engine::TrackId::from_raw(0xc022_0004);
const CLIP: engine::ClipId = engine::ClipId::from_raw(0xc022_0005);
const MEDIA_FINGERPRINT: &str = "sha256:c022-original-media";
const MEDIA_TARGET: &str = "superi.media-path.v1:relative:Media/original.mov";

fn range(start: i64, duration: u64, timebase: engine::Timebase) -> engine::TimeRange {
    engine::TimeRange::new(
        engine::RationalTime::new(start, timebase),
        engine::Duration::new(duration, timebase).unwrap(),
    )
    .unwrap()
}

fn project_document() -> engine::ProjectDocument {
    let source_rate = engine::Timebase::integer(48).unwrap();
    let edit_rate = engine::FrameRate::FPS_24.timebase();
    let media = engine::LinkedMediaReference::with_fingerprint(
        MEDIA,
        "camera original",
        MEDIA_TARGET,
        Some(range(0, 480, source_rate)),
        MEDIA_FINGERPRINT,
    )
    .unwrap();
    let clip = engine::Clip::new(
        CLIP,
        "camera edit",
        engine::ClipSource::Media(MEDIA),
        range(0, 96, source_rate),
        range(0, 48, edit_rate),
    )
    .unwrap();
    let timeline = engine::Timeline::new(
        ROOT,
        "scripted timeline",
        edit_rate,
        engine::RationalTime::zero(edit_rate),
        vec![engine::Track::new(
            TRACK,
            "V1",
            engine::TrackSemantics::Video(engine::VideoTrackSemantics::new(
                engine::FrameRate::FPS_24,
                engine::VideoCompositing::Over,
            )),
            vec![engine::TrackItem::Clip(clip)],
        )],
    );
    let editorial =
        engine::EditorialProject::new(PROJECT, "scripted project", [media], [timeline]).unwrap();
    engine::ProjectDocument::new(editorial, ROOT).unwrap()
}

fn project_api() -> ProjectEditorApi {
    let mut dispatcher = engine::EngineCommandDispatcher::new().unwrap();
    dispatcher.attach_project(project_document()).unwrap();
    ProjectEditorApi::new(dispatcher).unwrap()
}

fn source(script_id: &str, expected_revision: u64, steps: Vec<Value>) -> String {
    serde_json::to_string_pretty(&json!({
        "language": "superi-json",
        "language_version": "1.0.0",
        "script_id": script_id,
        "expected_initial_project_revision": expected_revision,
        "steps": steps,
    }))
    .unwrap()
}

fn digest(source: &str) -> String {
    format!("{:x}", Sha256::digest(source.as_bytes()))
}

fn request(source: String) -> RunProjectScript {
    let expected = digest(&source);
    RunProjectScript::new(source, expected)
}

fn mark_missing_step(transaction_id: &str, expected_revision: u64) -> Value {
    json!({
        "method": "superi.project.command.execute",
        "params": {
            "transaction_id": transaction_id,
            "expected_project_revision": expected_revision,
            "command": {
                "command": "apply",
                "actions": [{
                    "action": "mutate_media",
                    "mutation": {
                        "operation": "mark_missing",
                        "media_id": MEDIA.to_string(),
                    },
                }],
            },
        },
    })
}

fn inspect_step(transaction_id: &str) -> Value {
    json!({
        "method": "superi.editor.state.get",
        "params": {"transaction_id": transaction_id},
    })
}

fn temporary_project_path() -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let directory = std::env::temp_dir().join(format!(
        "superi-script-contract-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir(&directory).unwrap();
    directory.join("scripted.superi")
}

#[test]
fn supported_runtime_is_permanent_bounded_and_catalog_ready() {
    assert_eq!(RUN_PROJECT_SCRIPT_METHOD, "superi.project.script.run");
    assert_eq!(RunProjectScript::METHOD, RUN_PROJECT_SCRIPT_METHOD);
    assert_eq!(RunProjectScript::SCHEMA_VERSION, SCRIPTING_SCHEMA_VERSION);
    assert_eq!(
        RunProjectScript::KIND,
        superi_api::schema::PublicMethodKind::Command
    );
    assert_eq!(
        RunProjectScript::PERMISSION_MODE,
        ApiPermissionRequirementMode::PayloadDependent
    );
    assert_eq!(
        RunProjectScript::PERMISSION_KINDS,
        &[ApiPermissionKind::Filesystem, ApiPermissionKind::Plugin]
    );
    assert_eq!(SCRIPTING_SCHEMA_VERSION.to_string(), "1.0.0");
    assert_eq!(MAX_SCRIPT_SOURCE_BYTES, 1_048_576);
    assert_eq!(MAX_SCRIPT_STEPS, 256);
    assert_eq!(MAX_SCRIPT_IDENTIFIER_BYTES, 128);
    assert_eq!(MAX_SCRIPT_JSON_DEPTH, 128);
    assert_eq!(
        ScriptId::new("daily.conform-01").unwrap().as_str(),
        "daily.conform-01"
    );
    assert!(ScriptId::new("Daily Conform").is_err());
}

#[test]
fn script_executes_deterministically_and_preserves_durable_integrity_and_recovery() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let source = source(
        "daily.conform-01",
        0,
        vec![
            mark_missing_step("script-mark-missing", 0),
            inspect_step("script-inspect"),
        ],
    );

    let mut first_api = project_api();
    let first = first_api.execute(request(source.clone())).unwrap();
    let mut second_api = project_api();
    let second = second_api.execute(request(source.clone())).unwrap();
    assert_eq!(first, second);

    assert_eq!(first.status(), ProjectScriptRunStatus::Completed);
    assert_eq!(first.script_id().as_str(), "daily.conform-01");
    assert_eq!(first.source_sha256(), digest(&source));
    assert_eq!(first.initial_project_revision(), 0);
    assert_eq!(first.final_project_revision(), 1);
    assert_ne!(
        first.initial_project_semantic_hash(),
        first.final_project_semantic_hash()
    );
    assert_eq!(first.completed_steps().len(), 2);
    assert_eq!(first.failed_step_index(), None);
    assert!(first.failure().is_none());
    assert!(first.effects_committed());

    let events = first_api.drain_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].transaction_id(), "script-mark-missing");
    assert_eq!(events[0].project_revision(), 1);

    let snapshot = first_api.project_snapshot().unwrap();
    let media = snapshot.editorial_project().media_reference(MEDIA).unwrap();
    assert_eq!(media.id(), MEDIA);
    assert_eq!(media.target(), MEDIA_TARGET);
    assert_eq!(
        media.relink_state().expected_fingerprint(),
        Some(MEDIA_FINGERPRINT)
    );
    assert_eq!(media.relink_state().status(), engine::RelinkStatus::Missing);

    let path = temporary_project_path();
    let directory = path.parent().unwrap().to_path_buf();
    let mut database = engine::ProjectDatabase::create(&path).unwrap();
    database.replace(&snapshot).unwrap();
    drop(database);
    let reopened = engine::ProjectDatabase::open_read_only(&path).unwrap();
    let loaded = reopened.load().unwrap().snapshot();
    assert_eq!(loaded, snapshot);
    let loaded_media = loaded.editorial_project().media_reference(MEDIA).unwrap();
    assert_eq!(loaded_media.id(), MEDIA);
    assert_eq!(loaded_media.target(), MEDIA_TARGET);
    assert_eq!(
        loaded_media.relink_state().expected_fingerprint(),
        Some(MEDIA_FINGERPRINT)
    );
    drop(reopened);

    let recovered = superi_engine::test_support::verify_project_snapshot_integrity_and_recovery(
        &path, &directory, &loaded,
    )
    .unwrap();
    assert_eq!(recovered, loaded);

    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn later_conflict_reports_the_committed_prefix_and_never_executes_the_suffix() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut api = project_api();
    let source = source(
        "partial.conflict-01",
        0,
        vec![
            mark_missing_step("script-prefix", 0),
            json!({
                "method": "superi.project.command.execute",
                "params": {
                    "transaction_id": "script-stale",
                    "expected_project_revision": 0,
                    "command": {
                        "command": "apply",
                        "actions": [{
                            "action": "select_root_timeline",
                            "timeline_id": ROOT.to_string(),
                        }],
                    },
                },
            }),
            inspect_step("must-not-run"),
        ],
    );

    let result = api.execute(request(source)).unwrap();
    assert_eq!(result.status(), ProjectScriptRunStatus::Stopped);
    assert_eq!(result.completed_steps().len(), 1);
    assert_eq!(result.failed_step_index(), Some(1));
    assert_eq!(
        result.failure().unwrap().data().category(),
        ErrorCategory::Conflict
    );
    assert_eq!(result.initial_project_revision(), 0);
    assert_eq!(result.final_project_revision(), 1);
    assert!(result.effects_committed());
    assert_eq!(api.drain_events().unwrap().len(), 1);
    assert_eq!(api.project_snapshot().unwrap().revision(), 1);
}

#[test]
fn initial_conflict_and_invalid_sources_have_zero_authored_effects() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut stale_api = project_api();
    let stale = stale_api
        .execute(request(source(
            "initial.conflict-01",
            99,
            vec![inspect_step("must-not-run")],
        )))
        .unwrap();
    assert_eq!(stale.status(), ProjectScriptRunStatus::Rejected);
    assert_eq!(stale.completed_steps().len(), 0);
    assert_eq!(stale.failed_step_index(), None);
    assert_eq!(
        stale.failure().unwrap().data().category(),
        ErrorCategory::Conflict
    );
    assert!(!stale.effects_committed());
    assert!(stale_api.drain_events().unwrap().is_empty());
    assert_eq!(stale_api.project_snapshot().unwrap().revision(), 0);

    let duplicate = format!(
        "{{\"language\":\"superi-json\",\"language\":\"superi-json\",\"language_version\":\"1.0.0\",\"script_id\":\"duplicate\",\"expected_initial_project_revision\":0,\"steps\":[{}]}}",
        inspect_step("duplicate")
    );
    assert_invalid_before_dispatch(duplicate, ErrorCategory::InvalidInput);

    let unknown_method = source(
        "unknown.method-01",
        0,
        vec![json!({
            "method": "superi.host.filesystem.read",
            "params": {"path": "/private/source.mov"},
        })],
    );
    assert_invalid_before_dispatch(unknown_method, ErrorCategory::InvalidInput);

    let unsupported_version = source("unsupported.version-01", 0, vec![inspect_step("version")])
        .replace(
            "\"language_version\": \"1.0.0\"",
            "\"language_version\": \"2.0.0\"",
        );
    assert_invalid_before_dispatch(unsupported_version, ErrorCategory::Unsupported);

    assert_invalid_before_dispatch(
        source("empty.steps-01", 0, Vec::new()),
        ErrorCategory::InvalidInput,
    );
    let too_many_steps = (0..=MAX_SCRIPT_STEPS)
        .map(|index| inspect_step(&format!("bounded-step-{index}")))
        .collect();
    assert_invalid_before_dispatch(
        source("too.many.steps-01", 0, too_many_steps),
        ErrorCategory::ResourceExhausted,
    );
    let oversized_id = format!("a{}", "b".repeat(MAX_SCRIPT_IDENTIFIER_BYTES));
    assert_invalid_before_dispatch(
        source(&oversized_id, 0, vec![inspect_step("oversized-id")]),
        ErrorCategory::InvalidInput,
    );

    let mut excessive_depth = String::new();
    for _ in 0..=MAX_SCRIPT_JSON_DEPTH {
        excessive_depth.push('[');
    }
    excessive_depth.push_str("null");
    for _ in 0..=MAX_SCRIPT_JSON_DEPTH {
        excessive_depth.push(']');
    }
    assert_invalid_before_dispatch(excessive_depth, ErrorCategory::InvalidInput);

    let oversized = " ".repeat(MAX_SCRIPT_SOURCE_BYTES + 1);
    assert_invalid_before_dispatch(oversized, ErrorCategory::ResourceExhausted);
}

#[test]
fn exact_source_digest_and_nested_permissions_are_checked_before_dispatch() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let protected_source = source(
        "permission.preflight-01",
        0,
        vec![json!({
            "method": "superi.project.command.execute",
            "params": {
                "transaction_id": "script-protected",
                "expected_project_revision": 0,
                "command": {
                    "command": "apply",
                    "actions": [{
                        "action": "mutate_media",
                        "mutation": {
                            "operation": "set_path",
                            "media_id": MEDIA.to_string(),
                            "path": {
                                "kind": "project_relative",
                                "path": "Media/relinked.mov",
                            },
                        },
                    }],
                },
            },
        })],
    );
    let mut denied_api = project_api();
    let denied = denied_api.execute(request(protected_source)).unwrap_err();
    assert_eq!(denied.category(), ErrorCategory::PermissionDenied);
    assert_eq!(denied_api.project_snapshot().unwrap().revision(), 0);
    assert!(denied_api.drain_events().unwrap().is_empty());
    let first_dispatch = denied_api
        .execute(GetEditorState::new("after-permission-denial"))
        .unwrap();
    assert_eq!(first_dispatch.command_sequence(), 1);

    let digest_source = source("digest.exact-01", 0, vec![inspect_step("digest-inspect")]);
    let mut digest_api = project_api();
    let wrong = digest_api
        .execute(RunProjectScript::new(digest_source, "0".repeat(64)))
        .unwrap_err();
    assert_eq!(wrong.category(), ErrorCategory::InvalidInput);
    let first_dispatch = digest_api
        .execute(GetEditorState::new("after-digest-rejection"))
        .unwrap();
    assert_eq!(first_dispatch.command_sequence(), 1);
}

fn assert_invalid_before_dispatch(source: String, category: ErrorCategory) {
    let mut api = project_api();
    let error = api.execute(request(source)).unwrap_err();
    assert_eq!(error.category(), category);
    assert_eq!(api.project_snapshot().unwrap().revision(), 0);
    assert!(api.drain_events().unwrap().is_empty());
    let first_dispatch = api
        .execute(GetEditorState::new("after-invalid-script"))
        .unwrap();
    assert_eq!(first_dispatch.command_sequence(), 1);
}
