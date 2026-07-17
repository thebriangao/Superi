use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;
use superi_api::commands::ApiCommand;
use superi_api::editor::{
    EditorChannelMap, EditorChannelPosition, EditorClipMixControls, EditorClipMixMutation,
    EditorExtensionMutation, EditorGraphMutation, EditorMediaMutation, ExecuteProjectCommand,
    GetProjectCommandLog, GetProjectCommandLogResult, ProjectAction, ProjectCommand,
    ProjectCommandEvidence, ProjectCommandLogDetail, TimelineEditOperation,
};
use superi_api::events::{ApiEvent, ProjectStateChanged};
use superi_api::permissions::{
    ApiPermissionContext, ApiPermissionEffect, ApiPermissionRule, ApiPluginOperation,
    ApiPluginScope,
};
use superi_api::schema::ApiResource;
use superi_api::version::{
    EXECUTE_PROJECT_COMMAND_METHOD, GET_PROJECT_COMMAND_LOG_METHOD, PROJECT_COMMAND_LOG_RESOURCE,
    PROJECT_EDITOR_SCHEMA_VERSION, PROJECT_HISTORY_RESOURCE, PROJECT_STATE_CHANGED_EVENT,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_engine::editor as engine;

const PROJECT: engine::ProjectId = engine::ProjectId::from_raw(0xe000);
const MEDIA: engine::MediaId = engine::MediaId::from_raw(0xe001);
const ROOT: engine::TimelineId = engine::TimelineId::from_raw(0xe002);
const TRACK: engine::TrackId = engine::TrackId::from_raw(0xe003);
const CLIP: engine::ClipId = engine::ClipId::from_raw(0xe004);

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
        "urn:public-editor:source",
        Some(range(0, 480, source_rate)),
    );
    let clip = engine::Clip::new(
        CLIP,
        "original name",
        engine::ClipSource::Media(MEDIA),
        range(48, 96, source_rate),
        range(0, 48, edit_rate),
    )
    .unwrap();
    let timeline = engine::Timeline::new(
        ROOT,
        "public editor timeline",
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
        engine::EditorialProject::new(PROJECT, "public editor project", [media], [timeline])
            .unwrap();
    engine::ProjectDocument::new(editorial, ROOT).unwrap()
}

fn project_api() -> superi_api::editor::ProjectEditorApi {
    let mut dispatcher = engine::EngineCommandDispatcher::new().unwrap();
    dispatcher.attach_project(project_document()).unwrap();
    superi_api::editor::ProjectEditorApi::new(dispatcher).unwrap()
}

fn extension_permissions() -> Arc<ApiPermissionContext> {
    Arc::new(
        ApiPermissionContext::new(
            "superi.test.editor-contract",
            [
                ApiPermissionRule::plugin(
                    ApiPermissionEffect::Allow,
                    ApiPluginOperation::ManageState,
                    ApiPluginScope::exact("example.public-editor").unwrap(),
                )
                .unwrap(),
                ApiPermissionRule::plugin(
                    ApiPermissionEffect::Allow,
                    ApiPluginOperation::ManageLifecycle,
                    ApiPluginScope::exact("example.public-editor").unwrap(),
                )
                .unwrap(),
            ],
        )
        .unwrap(),
    )
}

fn exact_time(value: i64, numerator: u32) -> Value {
    json!({
        "value": value,
        "timebase": {"numerator": numerator, "denominator": 1}
    })
}

fn exact_duration(value: u64, numerator: u32) -> Value {
    json!({
        "value": value,
        "timebase": {"numerator": numerator, "denominator": 1}
    })
}

fn exact_range(start: i64, duration: u64, numerator: u32) -> Value {
    json!({
        "start": exact_time(start, numerator),
        "duration": exact_duration(duration, numerator)
    })
}

fn object_id(kind: &str, id: &str) -> Value {
    json!({"kind": kind, "id": id})
}

fn gap_item() -> Value {
    json!({
        "kind": "gap",
        "id": "gap:00000000000000000000000000000031",
        "name": "public gap",
        "record_range": exact_range(0, 12, 24)
    })
}

fn extension_key() -> Value {
    json!({
        "extension_id": "example.public-editor",
        "record_id": "state"
    })
}

fn extension_failure() -> Value {
    json!({
        "category": "invalid_input",
        "recoverability": "user_correctable",
        "message": "invalid extension state",
        "contexts": [],
        "total_failures": 1,
        "consecutive_failures": 1
    })
}

fn extension_record() -> Value {
    json!({
        "extension_id": "example.public-editor",
        "record_id": "state",
        "extension_version": "1.0.0",
        "extension_kind": "superi.extension.state",
        "payload_schema": "example.public-editor-state@1.0.0",
        "requested_capabilities": [],
        "granted_capabilities": [],
        "lifecycle": "enabled",
        "failure": null,
        "payload": [112, 117, 98, 108, 105, 99]
    })
}

fn stereo_controls_json() -> Value {
    let left = json!({"kind": "front_left"});
    let right = json!({"kind": "front_right"});
    json!({
        "input_layout": [left.clone(), right.clone()],
        "output_layout": [left.clone(), right.clone()],
        "channel_map": [
            {"source": left.clone(), "destination": left, "gain": 1.0},
            {"source": right.clone(), "destination": right.clone(), "gain": 1.0}
        ],
        "gain": 0.5,
        "fade_in_frames": 4,
        "fade_out_frames": 4,
        "pan": 0.0,
        "muted": false,
        "solo": false,
        "phase_inverted": [right]
    })
}

fn editable_node() -> Value {
    json!({
        "schema": {
            "node_type": "example.public-node",
            "schema_version": "1.0.0",
            "inputs": [],
            "outputs": [],
            "parameters": [],
            "behavior": {
                "time": {"kind": "invariant"},
                "roi": {"kind": "full_frame"},
                "color": {"kind": "not_applicable"},
                "determinism": "deterministic",
                "cache_policy": "disabled"
            },
            "required_capabilities": []
        },
        "inputs": [],
        "outputs": [],
        "parameters": []
    })
}

fn graph_parameter_value() -> Value {
    json!({
        "value_type": "superi.value.text",
        "value": {"kind": "domain", "value": {"kind": "text", "value": "public name"}}
    })
}

fn parameter_address() -> Value {
    json!({
        "node_id": "node:00000000000000000000000000000041",
        "parameter_id": "parameter:00000000000000000000000000000042"
    })
}

fn operation_tags<T>(values: Vec<Value>, field: &str) -> Vec<String>
where
    T: DeserializeOwned + Serialize,
{
    values
        .into_iter()
        .map(|value| {
            let decoded: T = serde_json::from_value(value).unwrap();
            serde_json::to_value(decoded).unwrap()[field]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect()
}

#[test]
fn editor_contract_has_permanent_names_and_strict_project_commands() {
    assert_eq!(
        EXECUTE_PROJECT_COMMAND_METHOD,
        "superi.project.command.execute"
    );
    assert_eq!(PROJECT_STATE_CHANGED_EVENT, "superi.project.state.changed");
    assert_eq!(PROJECT_HISTORY_RESOURCE, "superi.project.history");
    assert_eq!(
        ExecuteProjectCommand::METHOD,
        EXECUTE_PROJECT_COMMAND_METHOD
    );
    assert_eq!(
        ExecuteProjectCommand::SCHEMA_VERSION,
        PROJECT_EDITOR_SCHEMA_VERSION
    );
    assert_eq!(ProjectStateChanged::NAME, PROJECT_STATE_CHANGED_EVENT);
    assert_eq!(
        ProjectStateChanged::SCHEMA_VERSION,
        PROJECT_EDITOR_SCHEMA_VERSION
    );

    let value = json!({
        "transaction_id": "editor-command-1",
        "expected_project_revision": 0,
        "command": {
            "command": "apply",
            "actions": [{
                "action": "select_root_timeline",
                "timeline_id": ROOT.to_string()
            }]
        }
    });
    let command: ExecuteProjectCommand = serde_json::from_value(value.clone()).unwrap();
    assert!(matches!(command.command(), ProjectCommand::Apply { .. }));
    assert_eq!(serde_json::to_value(command).unwrap(), value);

    let mut unknown = value;
    unknown["guessed"] = json!(true);
    assert!(serde_json::from_value::<ExecuteProjectCommand>(unknown).is_err());
    assert!(serde_json::from_value::<ExecuteProjectCommand>(json!({
        "transaction_id": "unsupported",
        "expected_project_revision": 0,
        "command": {"command": "guess"}
    }))
    .is_err());
}

#[test]
fn every_current_editor_operation_has_one_strict_typed_discriminant() {
    let timeline_id = ROOT.to_string();
    let track_id = TRACK.to_string();
    let clip_id = CLIP.to_string();
    let gap_id = "gap:00000000000000000000000000000031";
    let clip_object = object_id("clip", &clip_id);
    let gap_object = object_id("gap", gap_id);
    let time = exact_time(12, 24);
    let range = exact_range(0, 12, 24);
    let material = gap_item();
    let timeline_operations = vec![
        json!({"operation":"insert","timeline_id":timeline_id,"track_id":track_id,"at":time,"material":material,"fragment_ids":[]}),
        json!({"operation":"overwrite","timeline_id":timeline_id,"track_id":track_id,"at":time,"material":material,"fragment_ids":[]}),
        json!({"operation":"append","timeline_id":timeline_id,"track_id":track_id,"material":material}),
        json!({"operation":"replace","timeline_id":timeline_id,"track_id":track_id,"target_id":clip_object,"material":material}),
        json!({"operation":"lift","timeline_id":timeline_id,"track_id":track_id,"range":range,"gap_id":gap_id,"gap_name":"lifted","fragment_ids":[]}),
        json!({"operation":"extract","timeline_id":timeline_id,"track_id":track_id,"range":range,"fragment_ids":[]}),
        json!({"operation":"ripple","timeline_id":timeline_id,"track_id":track_id,"target_id":clip_object,"side":"end","to":time,"sync_adjustments":[]}),
        json!({"operation":"roll","timeline_id":timeline_id,"track_id":track_id,"left_id":clip_object,"right_id":gap_object,"to":time}),
        json!({"operation":"slip","timeline_id":timeline_id,"track_id":track_id,"clip_id":clip_id,"source_start":time}),
        json!({"operation":"slide","timeline_id":timeline_id,"track_id":track_id,"clip_id":clip_id,"to":time}),
        json!({"operation":"razor","timeline_id":timeline_id,"track_id":track_id,"target_id":clip_object,"at":time,"fragment_id":gap_object}),
        json!({"operation":"trim","timeline_id":timeline_id,"track_id":track_id,"target_id":clip_object,"side":"end","to":time,"gap_id":null}),
        json!({"operation":"extend","timeline_id":timeline_id,"track_id":track_id,"target_id":clip_object,"side":"end","to":time,"mode":"ripple","sync_adjustments":[]}),
        json!({"operation":"three_point","timeline_id":timeline_id,"track_id":track_id,"clip":material,"placement":{"placement":"source_range_at_record_start","source_range":range,"record_start":time},"fragment_ids":[]}),
        json!({"operation":"four_point","timeline_id":timeline_id,"track_id":track_id,"clip":material,"source_range":range,"record_range":range,"fragment_ids":[]}),
    ];
    assert_eq!(
        operation_tags::<TimelineEditOperation>(timeline_operations, "operation"),
        [
            "insert",
            "overwrite",
            "append",
            "replace",
            "lift",
            "extract",
            "ripple",
            "roll",
            "slip",
            "slide",
            "razor",
            "trim",
            "extend",
            "three_point",
            "four_point"
        ]
    );

    let node_id = "node:00000000000000000000000000000041";
    let parameter_id = "parameter:00000000000000000000000000000042";
    let port_id = "port:00000000000000000000000000000043";
    let source = json!({"node_id": node_id, "port_id": port_id});
    let reference = json!({
        "address": parameter_address(),
        "value_type": "superi.value.text"
    });
    let graph_operations = vec![
        json!({"operation":"add","node_id":node_id,"node":editable_node(),"position":0}),
        json!({"operation":"remove","node_id":node_id}),
        json!({"operation":"connect","edge":{"edge_id":"edge:00000000000000000000000000000044","source":source,"destination":source}}),
        json!({"operation":"disconnect","edge_id":"edge:00000000000000000000000000000044"}),
        json!({"operation":"reorder","node_id":node_id,"position":0}),
        json!({"operation":"set_parameter","node_id":node_id,"parameter_id":parameter_id,"value":graph_parameter_value()}),
        json!({"operation":"set_parameter_driver","target":parameter_address(),"driver":{"kind":"link","value_type":"superi.value.text","source":reference}}),
        json!({"operation":"clear_parameter_driver","target":parameter_address()}),
    ];
    assert_eq!(
        operation_tags::<EditorGraphMutation>(graph_operations, "operation"),
        [
            "add",
            "remove",
            "connect",
            "disconnect",
            "reorder",
            "set_parameter",
            "set_parameter_driver",
            "clear_parameter_driver"
        ]
    );

    let media_operations = vec![
        json!({"operation":"set_path","media_id":MEDIA.to_string(),"path":{"kind":"project_relative","path":"media/source.mov"}}),
        json!({"operation":"mark_missing","media_id":MEDIA.to_string()}),
        json!({"operation":"consider_relink","media_id":MEDIA.to_string(),"path":{"kind":"absolute","platform":"unix","path":"/media/source.mov"},"observed_fingerprint":"sha256:test"}),
    ];
    assert_eq!(
        operation_tags::<EditorMediaMutation>(media_operations, "operation"),
        ["set_path", "mark_missing", "consider_relink"]
    );

    let clip_mix_operations = vec![
        json!({"operation":"set","clip_id":clip_id,"controls":stereo_controls_json()}),
        json!({"operation":"inherit","original":clip_id,"created":"clip:00000000000000000000000000000045"}),
        json!({"operation":"transfer","removed":clip_id,"inserted":"clip:00000000000000000000000000000046"}),
        json!({"operation":"remove","clip_id":clip_id}),
    ];
    assert_eq!(
        operation_tags::<EditorClipMixMutation>(clip_mix_operations, "operation"),
        ["set", "inherit", "transfer", "remove"]
    );

    let extension_operations = vec![
        json!({"operation":"upsert","record":extension_record()}),
        json!({"operation":"remove","key":extension_key()}),
        json!({"operation":"set_lifecycle","key":extension_key(),"lifecycle":"disabled"}),
        json!({"operation":"set_granted_capabilities","key":extension_key(),"capabilities":["project.read"]}),
        json!({"operation":"record_failure","key":extension_key(),"failure":extension_failure(),"quarantine":true}),
        json!({"operation":"clear_failure","key":extension_key()}),
    ];
    assert_eq!(
        operation_tags::<EditorExtensionMutation>(extension_operations, "operation"),
        [
            "upsert",
            "remove",
            "set_lifecycle",
            "set_granted_capabilities",
            "record_failure",
            "clear_failure"
        ]
    );

    let project_actions = vec![
        json!({"action":"select_root_timeline","timeline_id":ROOT.to_string()}),
        json!({"action":"edit_timeline","operations":[{"operation":"append","timeline_id":ROOT.to_string(),"track_id":TRACK.to_string(),"material":gap_item()}]}),
        json!({"action":"mutate_graph","graph_id":"graph:00000000000000000000000000000047","mutations":[{"operation":"reorder","node_id":node_id,"position":0}]}),
        json!({"action":"mutate_media","mutation":{"operation":"mark_missing","media_id":MEDIA.to_string()}}),
        json!({"action":"mutate_clip_mix","mutations":[{"operation":"remove","clip_id":CLIP.to_string()}]}),
        json!({"action":"mutate_extension","mutation":{"operation":"clear_failure","key":extension_key()}}),
    ];
    assert_eq!(
        operation_tags::<ProjectAction>(project_actions, "action"),
        [
            "select_root_timeline",
            "edit_timeline",
            "mutate_graph",
            "mutate_media",
            "mutate_clip_mix",
            "mutate_extension"
        ]
    );

    assert!(serde_json::from_value::<TimelineEditOperation>(json!({
        "operation": "unsupported"
    }))
    .is_err());
    assert!(serde_json::from_value::<EditorGraphMutation>(json!({
        "operation": "unsupported"
    }))
    .is_err());
}

#[test]
fn one_public_command_is_atomic_durable_correlated_and_undoable() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let document = project_document();
    let initial = document.snapshot();
    let compilation = initial.timeline_graph(ROOT).unwrap();
    let graph = compilation.snapshot();
    let graph_id = graph.graph_id();
    let node_id = compilation
        .index()
        .node(engine::TimelineGraphOrigin::Object(
            engine::EditorialObjectId::Clip(CLIP),
        ))
        .unwrap();
    let parameter = graph
        .node(node_id)
        .unwrap()
        .parameters()
        .values()
        .find(|parameter| parameter.name().as_str() == "name")
        .unwrap();
    let parameter_id = parameter.id();
    let value_type = parameter.value().value_type().as_str().to_owned();

    let mut dispatcher = engine::EngineCommandDispatcher::new().unwrap();
    dispatcher.attach_project(document).unwrap();
    let mut api = superi_api::editor::ProjectEditorApi::new_with_permissions(
        dispatcher,
        extension_permissions(),
    )
    .unwrap();
    let request: ExecuteProjectCommand = serde_json::from_value(json!({
        "transaction_id": "public-mixed-apply",
        "expected_project_revision": 0,
        "command": {
            "command": "apply",
            "actions": [
                {
                    "action": "mutate_graph",
                    "graph_id": graph_id.to_string(),
                    "mutations": [{
                        "operation": "set_parameter",
                        "node_id": node_id.to_string(),
                        "parameter_id": parameter_id.to_string(),
                        "value": {
                            "value_type": value_type,
                            "value": {
                                "kind": "domain",
                                "value": {"kind": "text", "value": "public editor name"}
                            }
                        }
                    }]
                },
                {
                    "action": "edit_timeline",
                    "operations": [{
                        "operation": "slip",
                        "timeline_id": ROOT.to_string(),
                        "track_id": TRACK.to_string(),
                        "clip_id": CLIP.to_string(),
                        "source_start": exact_time(96, 48)
                    }]
                },
                {
                    "action": "mutate_media",
                    "mutation": {"operation": "mark_missing", "media_id": MEDIA.to_string()}
                },
                {
                    "action": "mutate_clip_mix",
                    "mutations": [{
                        "operation": "set",
                        "clip_id": CLIP.to_string(),
                        "controls": stereo_controls_json()
                    }]
                },
                {
                    "action": "mutate_extension",
                    "mutation": {"operation": "upsert", "record": extension_record()}
                },
                {"action": "select_root_timeline", "timeline_id": ROOT.to_string()}
            ]
        }
    }))
    .unwrap();

    let applied = api.execute(request).unwrap();
    assert!(applied.authored_state_changed());
    assert_eq!(applied.state().project_revision(), 1);
    assert_eq!(applied.state().undo_depth(), 1);
    let ProjectCommandEvidence::Applied { actions } = applied.evidence() else {
        panic!("mixed public apply must retain per-action evidence");
    };
    assert_eq!(actions.len(), 6);

    let events = api.drain_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].command_sequence(), applied.command_sequence());
    assert_eq!(events[0].transaction_id(), "public-mixed-apply");
    assert_eq!(events[0].project_revision(), 1);
    assert_eq!(events[0].evidence(), applied.evidence());

    let published = api.project_snapshot().unwrap();
    let clip = published
        .editorial_project()
        .timeline(ROOT)
        .unwrap()
        .track(TRACK)
        .unwrap()
        .item(engine::EditorialObjectId::Clip(CLIP))
        .unwrap()
        .as_clip()
        .unwrap();
    assert_eq!(clip.source_range().start().value(), 96);
    assert_eq!(
        published
            .editorial_project()
            .media_reference(MEDIA)
            .unwrap()
            .relink_state()
            .status(),
        engine::RelinkStatus::Missing
    );
    assert!(published.clip_mix_state().controls(CLIP).is_some());
    assert_eq!(published.extension_records().len(), 1);

    let mut database = engine::ProjectDatabase::memory().unwrap();
    database.replace(&published).unwrap();
    assert_eq!(database.load().unwrap().snapshot(), published);

    let undone = api
        .execute(ExecuteProjectCommand::new(
            "public-undo",
            1,
            ProjectCommand::Undo {},
        ))
        .unwrap();
    assert_eq!(undone.state().project_revision(), 2);
    assert_eq!(undone.state().undo_depth(), 0);
    assert_eq!(undone.state().redo_depth(), 1);
    assert_eq!(api.drain_events().unwrap().len(), 1);
    assert!(api
        .project_snapshot()
        .unwrap()
        .extension_records()
        .is_empty());

    let redone = api
        .execute(ExecuteProjectCommand::new(
            "public-redo",
            2,
            ProjectCommand::Redo {},
        ))
        .unwrap();
    assert_eq!(redone.state().project_revision(), 3);
    assert_eq!(redone.state().undo_depth(), 1);
    assert_eq!(redone.state().redo_depth(), 0);
    assert_eq!(api.drain_events().unwrap().len(), 1);
    assert_eq!(api.project_snapshot().unwrap().extension_records().len(), 1);

    let no_op = api
        .execute(ExecuteProjectCommand::new(
            "public-no-op",
            3,
            ProjectCommand::Apply {
                actions: vec![ProjectAction::SelectRootTimeline {
                    timeline_id: ROOT.to_string(),
                }],
            },
        ))
        .unwrap();
    assert!(!no_op.authored_state_changed());
    assert_eq!(no_op.state().project_revision(), 3);
    assert_eq!(no_op.state().undo_depth(), 1);
    let events = api.drain_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(
        events[0].command_log_sequence(),
        no_op.command_log_sequence()
    );
    assert_eq!(events[0].project_revision(), 3);
}

#[test]
fn malformed_public_values_fail_before_dispatch_without_partial_state() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut api = project_api();
    let before = api.project_snapshot().unwrap();

    let invalid_id = ExecuteProjectCommand::new(
        "invalid-id",
        0,
        ProjectCommand::Apply {
            actions: vec![ProjectAction::SelectRootTimeline {
                timeline_id: "guessed".to_owned(),
            }],
        },
    );
    assert_eq!(
        api.execute(invalid_id).unwrap_err().category(),
        engine::ErrorCategory::InvalidInput
    );

    let invalid_time: ExecuteProjectCommand = serde_json::from_value(json!({
        "transaction_id": "invalid-timebase",
        "expected_project_revision": 0,
        "command": {
            "command": "apply",
            "actions": [{
                "action": "edit_timeline",
                "operations": [{
                    "operation": "slip",
                    "timeline_id": ROOT.to_string(),
                    "track_id": TRACK.to_string(),
                    "clip_id": CLIP.to_string(),
                    "source_start": {
                        "value": 96,
                        "timebase": {"numerator": 48, "denominator": 0}
                    }
                }]
            }]
        }
    }))
    .unwrap();
    assert_eq!(
        api.execute(invalid_time).unwrap_err().category(),
        engine::ErrorCategory::InvalidInput
    );

    let left = EditorChannelPosition::FrontLeft {};
    let right = EditorChannelPosition::FrontRight {};
    let invalid_float = ExecuteProjectCommand::new(
        "invalid-float",
        0,
        ProjectCommand::Apply {
            actions: vec![ProjectAction::MutateClipMix {
                mutations: vec![EditorClipMixMutation::Set {
                    clip_id: CLIP.to_string(),
                    controls: EditorClipMixControls {
                        input_layout: vec![left, right],
                        output_layout: vec![left, right],
                        channel_map: vec![
                            EditorChannelMap {
                                source: left,
                                destination: left,
                                gain: 1.0,
                            },
                            EditorChannelMap {
                                source: right,
                                destination: right,
                                gain: 1.0,
                            },
                        ],
                        gain: f32::NAN,
                        fade_in_frames: 0,
                        fade_out_frames: 0,
                        pan: 0.0,
                        muted: false,
                        solo: false,
                        phase_inverted: vec![],
                    },
                }],
            }],
        },
    );
    assert_eq!(
        api.execute(invalid_float).unwrap_err().category(),
        engine::ErrorCategory::InvalidInput
    );

    let empty =
        ExecuteProjectCommand::new("empty-apply", 0, ProjectCommand::Apply { actions: vec![] });
    assert_eq!(
        api.execute(empty).unwrap_err().category(),
        engine::ErrorCategory::InvalidInput
    );

    let late_failure: ExecuteProjectCommand = serde_json::from_value(json!({
        "transaction_id": "late-failure",
        "expected_project_revision": 0,
        "command": {
            "command": "apply",
            "actions": [
                {
                    "action": "edit_timeline",
                    "operations": [{
                        "operation": "slip",
                        "timeline_id": ROOT.to_string(),
                        "track_id": TRACK.to_string(),
                        "clip_id": CLIP.to_string(),
                        "source_start": exact_time(96, 48)
                    }]
                },
                {
                    "action": "mutate_media",
                    "mutation": {
                        "operation": "mark_missing",
                        "media_id": engine::MediaId::from_raw(0xffff).to_string()
                    }
                }
            ]
        }
    }))
    .unwrap();
    assert_eq!(
        api.execute(late_failure).unwrap_err().category(),
        engine::ErrorCategory::NotFound
    );

    let stale = ExecuteProjectCommand::new(
        "stale-command",
        99,
        ProjectCommand::Apply {
            actions: vec![ProjectAction::SelectRootTimeline {
                timeline_id: ROOT.to_string(),
            }],
        },
    );
    assert_eq!(
        api.execute(stale).unwrap_err().category(),
        engine::ErrorCategory::Conflict
    );

    assert_eq!(api.project_snapshot().unwrap(), before);
    assert!(api.drain_events().unwrap().is_empty());
    let inspected = api
        .execute(ExecuteProjectCommand::new(
            "inspect-after-failures",
            0,
            ProjectCommand::Inspect {},
        ))
        .unwrap();
    assert_eq!(inspected.command_sequence(), 1);
    assert!(!inspected.authored_state_changed());
    let events = api.drain_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].command_log_sequence(), 1);
}

#[test]
fn project_history_resource_is_registered_as_a_typed_resource() {
    assert_eq!(
        superi_api::editor::ProjectHistorySnapshot::RESOURCE,
        PROJECT_HISTORY_RESOURCE
    );
    assert_eq!(
        superi_api::editor::ProjectHistorySnapshot::SCHEMA_VERSION,
        PROJECT_EDITOR_SCHEMA_VERSION
    );
}

#[test]
fn command_log_query_is_bounded_cursor_safe_and_returns_typed_replay() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut api = project_api();
    let first = api
        .execute(ExecuteProjectCommand::new(
            "query-inspect",
            0,
            ProjectCommand::Inspect {},
        ))
        .unwrap();
    let second = api
        .execute(ExecuteProjectCommand::new(
            "query-no-op",
            0,
            ProjectCommand::Apply {
                actions: vec![ProjectAction::SelectRootTimeline {
                    timeline_id: ROOT.to_string(),
                }],
            },
        ))
        .unwrap();
    assert_eq!(first.command_log_sequence(), 1);
    assert_eq!(second.command_log_sequence(), 2);
    assert_eq!(api.drain_events().unwrap().len(), 2);

    let metadata = api
        .execute(GetProjectCommandLog::new(
            0,
            1,
            ProjectCommandLogDetail::Metadata,
        ))
        .unwrap();
    let GetProjectCommandLogResult::Records(metadata) = metadata else {
        panic!("initial command log must be replayable from sequence zero");
    };
    assert_eq!(metadata.records().len(), 1);
    assert_eq!(metadata.records()[0].sequence(), 1);
    assert!(metadata.records()[0].replay_request().is_none());
    assert_eq!(metadata.through_sequence(), 1);
    assert_eq!(metadata.latest_sequence(), 2);

    let replay = api
        .execute(GetProjectCommandLog::new(
            1,
            256,
            ProjectCommandLogDetail::Replayable,
        ))
        .unwrap();
    let GetProjectCommandLogResult::Records(replay) = replay else {
        panic!("retained suffix must not require resynchronization");
    };
    assert_eq!(replay.records().len(), 1);
    assert_eq!(replay.records()[0].transaction_id(), "query-no-op");
    assert_eq!(
        replay.records()[0]
            .replay_request()
            .unwrap()
            .transaction_id(),
        "query-no-op"
    );
    assert_eq!(GetProjectCommandLog::METHOD, GET_PROJECT_COMMAND_LOG_METHOD);
    assert_eq!(
        superi_api::editor::ProjectCommandLogBatch::RESOURCE,
        PROJECT_COMMAND_LOG_RESOURCE
    );

    let before = api.project_snapshot().unwrap();
    assert_eq!(
        api.execute(GetProjectCommandLog::new(
            0,
            0,
            ProjectCommandLogDetail::Metadata,
        ))
        .unwrap_err()
        .category(),
        engine::ErrorCategory::InvalidInput
    );
    assert_eq!(api.project_snapshot().unwrap(), before);
}
