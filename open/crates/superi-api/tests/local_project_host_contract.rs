use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use superi_api::commands::{
    CompareProjectRecovery, DismissProjectRecovery, ExecuteProjectSettingsTransaction,
    GetEditorState, GetProjectRecovery, GetProjectSettings, RestoreProjectRecovery,
};
use superi_api::editor::{
    EditorMediaMutation, EditorMediaPath, ExactDuration, ExactTime, ExactTimeRange, ExactTimebase,
    ExecuteProjectCommand, ProjectAction, ProjectCommand, TimelineMarkerFlag,
    TimelineMarkerMutation, TimelineMarkerOwner,
};
use superi_api::local::{
    LocalAutomationRequest, LocalAutomationResult, LocalProjectCollision,
    LocalProjectCreateRequest, LocalProjectHost,
};
use superi_api::permissions::{
    ApiDestructiveOperation, ApiPermissionContext, ApiPermissionEffect, ApiPermissionRule,
};
use superi_api::project::{ProjectSettingMutation, ProjectSettingValue};
use superi_api::scripting::ProjectScriptRunStatus;
use superi_core::error::ErrorCategory;
use superi_engine::editor as engine;

static NEXT_PATH: AtomicU64 = AtomicU64::new(0);

const PROJECT: engine::ProjectId = engine::ProjectId::from_raw(0xc023_0001);
const MEDIA: engine::MediaId = engine::MediaId::from_raw(0xc023_0002);
const ROOT: engine::TimelineId = engine::TimelineId::from_raw(0xc023_0003);
const TRACK: engine::TrackId = engine::TrackId::from_raw(0xc023_0004);
const FIRST_CLIP: engine::ClipId = engine::ClipId::from_raw(0xc023_0005);
const SECOND_CLIP: engine::ClipId = engine::ClipId::from_raw(0xc023_0006);
const MARKER: engine::MarkerId = engine::MarkerId::from_raw(0xc023_0007);

struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "superi-local-host-{label}-{}-{}",
            std::process::id(),
            NEXT_PATH.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self { path }
    }

    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

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
    let media = engine::LinkedMediaReference::new(
        MEDIA,
        "source",
        "media/source.mov",
        Some(range(0, 480, source_rate)),
    );
    let first = engine::Clip::new(
        FIRST_CLIP,
        "first",
        engine::ClipSource::Media(MEDIA),
        range(48, 96, source_rate),
        range(0, 48, edit_rate),
    )
    .unwrap();
    let second = engine::Clip::new(
        SECOND_CLIP,
        "second",
        engine::ClipSource::Media(MEDIA),
        range(144, 96, source_rate),
        range(48, 48, edit_rate),
    )
    .unwrap();
    let mut timeline = engine::Timeline::new(
        ROOT,
        "durable timeline",
        edit_rate,
        engine::RationalTime::zero(edit_rate),
        vec![engine::Track::new(
            TRACK,
            "V1",
            engine::TrackSemantics::Video(engine::VideoTrackSemantics::new(
                engine::FrameRate::FPS_24,
                engine::VideoCompositing::Over,
            )),
            vec![
                engine::TrackItem::Clip(first),
                engine::TrackItem::Clip(second),
            ],
        )],
    );
    timeline.link_clips([FIRST_CLIP, SECOND_CLIP]).unwrap();
    timeline.group_clips([FIRST_CLIP, SECOND_CLIP]).unwrap();
    timeline.set_track_targeted(TRACK, true).unwrap();
    timeline.set_track_sync_locked(TRACK, false).unwrap();
    let editorial =
        engine::EditorialProject::new(PROJECT, "durable project", [media], [timeline]).unwrap();
    engine::ProjectDocument::new(editorial, ROOT).unwrap()
}

fn create_project(path: &Path) -> engine::ProjectSnapshot {
    let snapshot = project_document().snapshot();
    let mut database = engine::ProjectDatabase::create(path).unwrap();
    database.replace(&snapshot).unwrap();
    snapshot
}

fn load(path: &Path) -> engine::ProjectSnapshot {
    engine::ProjectDatabase::open_read_only(path)
        .unwrap()
        .load()
        .unwrap()
        .snapshot()
}

fn untrusted() -> Arc<ApiPermissionContext> {
    Arc::new(ApiPermissionContext::default())
}

fn recovery_permissions() -> Arc<ApiPermissionContext> {
    Arc::new(
        ApiPermissionContext::new(
            "superi.test.local-project-host",
            [
                ApiPermissionRule::destructive(
                    ApiPermissionEffect::Allow,
                    ApiDestructiveOperation::RestoreProjectRecovery,
                ),
                ApiPermissionRule::destructive(
                    ApiPermissionEffect::Allow,
                    ApiDestructiveOperation::DismissProjectRecovery,
                ),
            ],
        )
        .unwrap(),
    )
}

#[test]
fn production_create_is_no_clobber_and_immediately_inspectable() {
    let directory = TempDirectory::new("create");
    let project = directory.join("created.superi");
    let request = LocalProjectCreateRequest {
        project_id: PROJECT.to_string(),
        project_name: "created project".into(),
        root_timeline_id: ROOT.to_string(),
        root_timeline_name: "created timeline".into(),
        edit_rate_numerator: 24,
        edit_rate_denominator: 1,
    };

    let summary = LocalProjectHost::create(&project, request.clone()).unwrap();
    assert_eq!(summary.project_id(), PROJECT.to_string());
    assert_eq!(summary.project_revision(), 0);
    assert_eq!(summary.root_timeline_id(), ROOT.to_string());
    assert!(LocalProjectHost::create(&project, request).is_err());

    let state =
        LocalProjectHost::inspect_editor(&project, GetEditorState::new("inspect-created")).unwrap();
    assert_eq!(state.transaction_id(), "inspect-created");
    let state = serde_json::to_value(state).unwrap();
    assert_eq!(
        state["snapshot"]["project"]["project_id"],
        PROJECT.to_string()
    );
}

#[test]
fn timeline_marker_commands_persist_and_reverse_through_typed_inverse_batches() {
    let directory = TempDirectory::new("timeline-markers");
    let project = directory.join("markers.superi");
    create_project(&project);
    let timebase = ExactTimebase {
        numerator: 24,
        denominator: 1,
    };
    let mutation = ExecuteProjectCommand::new(
        "create-marker",
        0,
        ProjectCommand::Apply {
            actions: vec![ProjectAction::MutateMarkers {
                mutations: vec![TimelineMarkerMutation::Create {
                    timeline_id: ROOT.to_string(),
                    marker_id: MARKER.to_string(),
                    owner: TimelineMarkerOwner::Timeline {},
                    marked_range: ExactTimeRange {
                        start: ExactTime {
                            value: 12,
                            timebase,
                        },
                        duration: ExactDuration { value: 4, timebase },
                    },
                    label: Some("review".to_owned()),
                    flag: Some(TimelineMarkerFlag::Yellow),
                    note: Some("check the cut".to_owned()),
                    metadata: Default::default(),
                }],
            }],
        },
    );
    let applied = LocalProjectHost::execute_timeline(&project, mutation, untrusted()).unwrap();
    assert_eq!(applied.result().state().project_revision(), 1);
    assert_eq!(applied.events().len(), 1);
    let marker = load(&project)
        .editorial_project()
        .timeline(ROOT)
        .unwrap()
        .marker(MARKER)
        .unwrap()
        .clone();
    assert_eq!(marker.label().unwrap().as_str(), "review");
    assert_eq!(marker.flag(), Some(engine::MarkerFlag::Yellow));
    assert_eq!(marker.note().unwrap().as_str(), "check the cut");

    let reversed = LocalProjectHost::execute_timeline(
        &project,
        ExecuteProjectCommand::new(
            "reverse-marker-create",
            1,
            ProjectCommand::Apply {
                actions: vec![ProjectAction::MutateMarkers {
                    mutations: vec![TimelineMarkerMutation::Remove {
                        timeline_id: ROOT.to_string(),
                        marker_id: MARKER.to_string(),
                    }],
                }],
            },
        ),
        untrusted(),
    )
    .unwrap();
    assert_eq!(reversed.result().state().project_revision(), 2);
    assert!(load(&project)
        .editorial_project()
        .timeline(ROOT)
        .unwrap()
        .marker(MARKER)
        .is_none());

    let restored = LocalProjectHost::execute_timeline(
        &project,
        ExecuteProjectCommand::new(
            "reverse-marker-remove",
            2,
            ProjectCommand::Apply {
                actions: vec![ProjectAction::MutateMarkers {
                    mutations: vec![TimelineMarkerMutation::Create {
                        timeline_id: ROOT.to_string(),
                        marker_id: MARKER.to_string(),
                        owner: TimelineMarkerOwner::Timeline {},
                        marked_range: ExactTimeRange {
                            start: ExactTime {
                                value: 12,
                                timebase,
                            },
                            duration: ExactDuration { value: 4, timebase },
                        },
                        label: Some("review".to_owned()),
                        flag: Some(TimelineMarkerFlag::Yellow),
                        note: Some("check the cut".to_owned()),
                        metadata: Default::default(),
                    }],
                }],
            },
        ),
        untrusted(),
    )
    .unwrap();
    assert_eq!(restored.result().state().project_revision(), 3);
    assert_eq!(
        load(&project)
            .editorial_project()
            .timeline(ROOT)
            .unwrap()
            .marker(MARKER)
            .unwrap(),
        &marker
    );
}

#[test]
fn durable_workflows_preserve_editorial_intent_and_gate_success_on_publication() {
    let directory = TempDirectory::new("durable");
    let project = directory.join("active.superi");
    let copy = directory.join("copy.superi");
    let backup = directory.join("backup.superi");
    let recovery = directory.join("recovery");
    fs::create_dir(&recovery).unwrap();
    let initial = create_project(&project);
    let original_timeline = initial.editorial_project().timeline(ROOT).unwrap().clone();
    let recovery_project = recovery.join(format!("project-{:032x}", PROJECT.raw()));
    fs::create_dir(&recovery_project).unwrap();
    let recovery_candidate = recovery_project.join("autosave-g00000000000000000001.superi");
    LocalProjectHost::backup(&project, &recovery_candidate).unwrap();

    let denied = ExecuteProjectCommand::new(
        "permission-denied",
        0,
        ProjectCommand::Apply {
            actions: vec![ProjectAction::MutateMedia {
                mutation: EditorMediaMutation::SetPath {
                    media_id: MEDIA.to_string(),
                    path: EditorMediaPath::ProjectRelative {
                        path: "media/relinked.mov".into(),
                    },
                },
            }],
        },
    );
    let error = LocalProjectHost::execute_media(&project, denied, untrusted()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::PermissionDenied);
    assert_eq!(load(&project), initial);

    let atomic_failure = ExecuteProjectCommand::new(
        "atomic-failure",
        0,
        ProjectCommand::Apply {
            actions: vec![
                ProjectAction::MutateMedia {
                    mutation: EditorMediaMutation::MarkMissing {
                        media_id: MEDIA.to_string(),
                    },
                },
                ProjectAction::MutateMedia {
                    mutation: EditorMediaMutation::MarkMissing {
                        media_id: engine::MediaId::from_raw(0xffff).to_string(),
                    },
                },
            ],
        },
    );
    assert!(LocalProjectHost::execute_media(&project, atomic_failure, untrusted()).is_err());
    assert_eq!(load(&project), initial);

    let mutation = ExecuteProjectCommand::new(
        "mark-missing",
        0,
        ProjectCommand::Apply {
            actions: vec![ProjectAction::MutateMedia {
                mutation: EditorMediaMutation::MarkMissing {
                    media_id: MEDIA.to_string(),
                },
            }],
        },
    );
    let result = LocalProjectHost::execute_media(&project, mutation, untrusted()).unwrap();
    assert!(result.result().authored_state_changed());
    assert_eq!(result.result().state().project_revision(), 1);
    assert_eq!(result.events().len(), 1);

    let after_media = load(&project);
    assert_eq!(after_media.revision(), 1);
    assert_eq!(
        after_media.editorial_project().timeline(ROOT).unwrap(),
        &original_timeline
    );
    let original_media = initial.editorial_project().media_reference(MEDIA).unwrap();
    let current_media = after_media
        .editorial_project()
        .media_reference(MEDIA)
        .unwrap();
    assert_eq!(current_media.id(), original_media.id());
    assert_eq!(current_media.name(), original_media.name());
    assert_eq!(current_media.target(), original_media.target());
    assert_eq!(
        current_media.available_range(),
        original_media.available_range()
    );
    assert_eq!(
        current_media.relink_state().status(),
        engine::RelinkStatus::Missing
    );

    let timeline_noop = ExecuteProjectCommand::new(
        "timeline-noop",
        1,
        ProjectCommand::Apply {
            actions: vec![ProjectAction::SelectRootTimeline {
                timeline_id: ROOT.to_string(),
            }],
        },
    );
    let result = LocalProjectHost::execute_timeline(&project, timeline_noop, untrusted()).unwrap();
    assert!(!result.result().authored_state_changed());
    assert_eq!(result.events().len(), 1);
    assert_eq!(result.result().command_log_sequence(), 2);
    assert_eq!(result.events()[0].command_log_sequence(), 2);
    let after_noop = load(&project);
    assert_eq!(after_noop.revision(), after_media.revision());
    assert_eq!(
        after_noop.editorial_project(),
        after_media.editorial_project()
    );
    assert_eq!(after_noop.settings(), after_media.settings());
    assert_eq!(
        after_noop.graphs().collect::<Vec<_>>(),
        after_media.graphs().collect::<Vec<_>>()
    );
    assert_eq!(after_noop.command_log().latest_sequence(), 2);

    let render = ExecuteProjectSettingsTransaction::new(
        "render-delivery",
        1,
        vec![ProjectSettingMutation::Set {
            key: "superi.project.render.color_target".into(),
            value: ProjectSettingValue::Text("delivery".into()),
        }],
    );
    let render = LocalProjectHost::configure_render(&project, render).unwrap();
    assert_eq!(render.result().snapshot().project_revision(), 2);
    assert_eq!(render.events().len(), 1);
    let settings = LocalProjectHost::inspect_settings(&project, GetProjectSettings::new()).unwrap();
    assert_eq!(settings.snapshot().project_revision(), 2);
    assert_eq!(
        settings
            .snapshot()
            .value("superi.project.render.color_target"),
        Some(&ProjectSettingValue::Text("delivery".into()))
    );

    let validation = LocalProjectHost::validate(&project).unwrap();
    assert!(validation.valid());
    assert_eq!(validation.project().project_revision(), 2);

    let copy_result =
        LocalProjectHost::save_copy(&project, &copy, LocalProjectCollision::RequireAbsent).unwrap();
    assert_eq!(copy_result.operation(), "save_copy");
    let backup_result = LocalProjectHost::backup(&project, &backup).unwrap();
    assert_eq!(backup_result.operation(), "backup");
    assert_eq!(load(&copy), load(&project));
    assert_eq!(load(&backup), load(&project));

    let recovery_state = LocalProjectHost::recovery_get(
        &project,
        &recovery,
        GetProjectRecovery::new("recovery-get"),
        untrusted(),
    )
    .unwrap();
    assert_eq!(recovery_state.result().transaction_id(), "recovery-get");
    assert_eq!(recovery_state.events().len(), 1);
    assert_eq!(recovery_state.result().snapshot().candidates().len(), 1);
    let catalog_revision = recovery_state.result().snapshot().catalog_revision();
    let candidate_id = recovery_state.result().snapshot().candidates()[0]
        .candidate_id()
        .to_owned();
    let comparison = LocalProjectHost::recovery_compare(
        &project,
        &recovery,
        CompareProjectRecovery::new("recovery-compare", catalog_revision, &candidate_id),
        untrusted(),
    )
    .unwrap();
    assert!(comparison.result().comparison().editorial_changed());
    assert!(comparison.result().comparison().settings_changed());
    assert!(comparison.events().is_empty());

    let automation_request: LocalAutomationRequest = serde_json::from_value(serde_json::json!({
        "jsonrpc": "2.0",
        "id": "api-automation-editor",
        "method": "superi.editor.state.get",
        "params": {"transaction_id": "automation-editor"}
    }))
    .unwrap();
    let automation =
        LocalProjectHost::execute_automation(&project, automation_request, untrusted()).unwrap();
    let LocalAutomationResult::EditorState(state) = automation.result() else {
        panic!("unexpected automation result");
    };
    assert_eq!(state.transaction_id(), "automation-editor");
    let state = serde_json::to_value(state).unwrap();
    assert_eq!(state["snapshot"]["project"]["project_revision"], 2);

    let script_source = serde_json::to_string(&serde_json::json!({
        "language": "superi-json",
        "language_version": "1.0.0",
        "script_id": "api.automation.script",
        "expected_initial_project_revision": 2,
        "steps": [{
            "method": "superi.editor.state.get",
            "params": {"transaction_id": "automation-script-editor"}
        }]
    }))
    .unwrap();
    let script_digest = format!("{:x}", Sha256::digest(script_source.as_bytes()));
    let script_request: LocalAutomationRequest = serde_json::from_value(serde_json::json!({
        "jsonrpc": "2.0",
        "id": 23,
        "method": "superi.project.script.run",
        "params": {
            "source": script_source,
            "expected_source_sha256": script_digest
        }
    }))
    .unwrap();
    let automation =
        LocalProjectHost::execute_automation(&project, script_request, untrusted()).unwrap();
    let LocalAutomationResult::ProjectScript(script) = automation.result() else {
        panic!("unexpected automation result");
    };
    assert_eq!(script.result().status(), ProjectScriptRunStatus::Completed);
    assert_eq!(script.result().final_project_revision(), 2);
    assert!(!script.result().effects_committed());
    assert!(script.events().is_empty());

    let restored = LocalProjectHost::recovery_restore(
        &project,
        &recovery,
        RestoreProjectRecovery::new("recovery-restore", catalog_revision, 2, &candidate_id),
        recovery_permissions(),
    )
    .unwrap();
    assert!(restored.result().restored());
    assert_eq!(restored.events().len(), 1);
    let restored_project = load(&project);
    assert_eq!(restored_project.revision(), 3);
    assert_eq!(
        restored_project.editorial_project(),
        initial.editorial_project()
    );
    assert_eq!(restored_project.settings(), initial.settings());

    let dismissed = LocalProjectHost::recovery_dismiss(
        &project,
        &recovery,
        DismissProjectRecovery::new("recovery-dismiss", catalog_revision, &candidate_id),
        recovery_permissions(),
    )
    .unwrap();
    assert!(dismissed.result().dismissed());
    assert!(dismissed.result().snapshot().candidates().is_empty());
    assert_eq!(dismissed.events().len(), 1);
    assert!(!recovery_candidate.exists());
}
