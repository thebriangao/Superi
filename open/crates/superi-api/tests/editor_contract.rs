use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;
use superi_api::commands::ApiCommand;
use superi_api::editor::{
    EditorChannelMap, EditorChannelPosition, EditorClipMixControls, EditorClipMixMutation,
    EditorExtensionMutation, EditorGraphMutation, EditorMediaMutation, ExecuteProjectCommand,
    GetProjectCommandLog, GetProjectCommandLogResult, ProjectAction, ProjectCommand,
    ProjectCommandEvidence, ProjectCommandLogDetail, TimelineEditOperation, TimelineMarkerMutation,
    TimelineMulticamMutation, TimelineTrackMutation,
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
const SECOND_CLIP: engine::ClipId = engine::ClipId::from_raw(0xe005);
const TRANSITION: engine::TransitionId = engine::TransitionId::from_raw(0xe006);
const MARKER: engine::MarkerId = engine::MarkerId::from_raw(0xe007);
const MULTICAM_SOURCE: engine::TimelineId = engine::TimelineId::from_raw(0xe100);
const MULTICAM_TARGET: engine::TimelineId = engine::TimelineId::from_raw(0xe101);
const MULTICAM_SOURCE_TRACK_A: engine::TrackId = engine::TrackId::from_raw(0xe102);
const MULTICAM_SOURCE_TRACK_B: engine::TrackId = engine::TrackId::from_raw(0xe103);
const MULTICAM_TARGET_TRACK: engine::TrackId = engine::TrackId::from_raw(0xe104);
const MULTICAM_SOURCE_CLIP_A: engine::ClipId = engine::ClipId::from_raw(0xe105);
const MULTICAM_SOURCE_CLIP_B: engine::ClipId = engine::ClipId::from_raw(0xe106);
const MULTICAM_TARGET_CLIP: engine::ClipId = engine::ClipId::from_raw(0xe107);
const MULTICAM_ANGLE_A: engine::MulticamAngleId = engine::MulticamAngleId::from_raw(0xe108);
const MULTICAM_ANGLE_B: engine::MulticamAngleId = engine::MulticamAngleId::from_raw(0xe109);

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
    let second_clip = engine::Clip::new(
        SECOND_CLIP,
        "second name",
        engine::ClipSource::Media(MEDIA),
        range(144, 48, source_rate),
        range(48, 24, edit_rate),
    )
    .unwrap();
    let transition = engine::Transition::new(
        TRANSITION,
        "public dissolve",
        engine::EditorialObjectId::Clip(CLIP),
        engine::EditorialObjectId::Clip(SECOND_CLIP),
        engine::Duration::new(4, edit_rate).unwrap(),
        engine::Duration::new(4, edit_rate).unwrap(),
    );
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
            vec![
                engine::TrackItem::Clip(clip),
                engine::TrackItem::Transition(transition),
                engine::TrackItem::Clip(second_clip),
            ],
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

fn multicam_project_api() -> superi_api::editor::ProjectEditorApi {
    let edit_rate = engine::FrameRate::FPS_24.timebase();
    let second_media = engine::MediaId::from_raw(0xe110);
    let semantics = || {
        engine::TrackSemantics::Video(engine::VideoTrackSemantics::new(
            engine::FrameRate::FPS_24,
            engine::VideoCompositing::Over,
        ))
    };
    let source_clip = |id, media| {
        engine::TrackItem::Clip(
            engine::Clip::new(
                id,
                format!("source {}", id.raw()),
                engine::ClipSource::Media(media),
                range(0, 24, edit_rate),
                range(0, 24, edit_rate),
            )
            .unwrap(),
        )
    };
    let source = engine::Timeline::new(
        MULTICAM_SOURCE,
        "synchronized source",
        edit_rate,
        engine::RationalTime::zero(edit_rate),
        vec![
            engine::Track::new(
                MULTICAM_SOURCE_TRACK_A,
                "camera a",
                semantics(),
                vec![source_clip(MULTICAM_SOURCE_CLIP_A, MEDIA)],
            ),
            engine::Track::new(
                MULTICAM_SOURCE_TRACK_B,
                "camera b",
                semantics(),
                vec![source_clip(MULTICAM_SOURCE_CLIP_B, second_media)],
            ),
        ],
    );
    let target = engine::Timeline::new(
        MULTICAM_TARGET,
        "multicam edit",
        edit_rate,
        engine::RationalTime::zero(edit_rate),
        vec![engine::Track::new(
            MULTICAM_TARGET_TRACK,
            "V1",
            semantics(),
            vec![engine::TrackItem::Clip(
                engine::Clip::new(
                    MULTICAM_TARGET_CLIP,
                    "multicam target",
                    engine::ClipSource::Timeline(MULTICAM_SOURCE),
                    range(0, 24, edit_rate),
                    range(0, 24, edit_rate),
                )
                .unwrap(),
            )],
        )],
    );
    let editorial = engine::EditorialProject::new(
        PROJECT,
        "public multicam project",
        [
            engine::LinkedMediaReference::new(
                MEDIA,
                "camera a",
                "urn:camera:a",
                Some(range(0, 24, edit_rate)),
            ),
            engine::LinkedMediaReference::new(
                second_media,
                "camera b",
                "urn:camera:b",
                Some(range(0, 24, edit_rate)),
            ),
        ],
        [source, target],
    )
    .unwrap();
    let document = engine::ProjectDocument::new(editorial, MULTICAM_TARGET).unwrap();
    let mut dispatcher = engine::EngineCommandDispatcher::new().unwrap();
    dispatcher.attach_project(document).unwrap();
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

fn retime_map() -> Value {
    json!({
        "record_duration": exact_duration(48, 24),
        "source_timebase": {"numerator": 48, "denominator": 1},
        "segments": [
            {
                "record_range": exact_range(0, 24, 24),
                "source_start": exact_time(48, 48),
                "rate_numerator": 2,
                "rate_denominator": 1
            },
            {
                "record_range": exact_range(24, 24, 24),
                "source_start": exact_time(144, 48),
                "rate_numerator": 0,
                "rate_denominator": 1
            }
        ]
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
    assert_eq!(PROJECT_EDITOR_SCHEMA_VERSION.to_string(), "1.6.0");
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
    let transition_id = TRANSITION.to_string();
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
        json!({"operation":"set_transition","timeline_id":timeline_id,"track_id":track_id,"transition_id":transition_id,"from_offset":exact_duration(6,24),"to_offset":exact_duration(2,24)}),
        json!({"operation":"three_point","timeline_id":timeline_id,"track_id":track_id,"clip":material,"placement":{"placement":"source_range_at_record_start","source_range":range,"record_start":time},"fragment_ids":[]}),
        json!({"operation":"four_point","timeline_id":timeline_id,"track_id":track_id,"clip":material,"source_range":range,"record_range":range,"fragment_ids":[]}),
        json!({
            "operation": "retime",
            "timeline_id": timeline_id,
            "track_id": track_id,
            "clip_id": clip_id,
            "time_map": {
                "record_duration": exact_duration(48, 24),
                "source_timebase": {"numerator": 48, "denominator": 1},
                "segments": [{
                    "record_range": exact_range(0, 48, 24),
                    "source_start": exact_time(48, 48),
                    "rate_numerator": 1,
                    "rate_denominator": 1
                }]
            }
        }),
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
            "set_transition",
            "three_point",
            "four_point",
            "retime"
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

    let track_operations = vec![
        json!({"operation":"create","timeline_id":ROOT.to_string(),"track_id":"track:0000000000000000000000000000e010","name":"V2","kind":"video","position":1,"height":72}),
        json!({"operation":"delete","timeline_id":ROOT.to_string(),"track_id":TRACK.to_string()}),
        json!({"operation":"rename","timeline_id":ROOT.to_string(),"track_id":TRACK.to_string(),"name":"Picture"}),
        json!({"operation":"set_height","timeline_id":ROOT.to_string(),"track_id":TRACK.to_string(),"height":96}),
        json!({"operation":"reorder","timeline_id":ROOT.to_string(),"track_id":TRACK.to_string(),"position":0}),
        json!({"operation":"set_targeted","timeline_id":ROOT.to_string(),"track_id":TRACK.to_string(),"targeted":true}),
        json!({"operation":"set_locked","timeline_id":ROOT.to_string(),"track_id":TRACK.to_string(),"locked":true}),
        json!({"operation":"set_sync_locked","timeline_id":ROOT.to_string(),"track_id":TRACK.to_string(),"sync_locked":false}),
        json!({"operation":"set_muted","timeline_id":ROOT.to_string(),"track_id":TRACK.to_string(),"muted":true}),
        json!({"operation":"set_solo","timeline_id":ROOT.to_string(),"track_id":TRACK.to_string(),"solo":true}),
        json!({"operation":"set_enabled","timeline_id":ROOT.to_string(),"track_id":TRACK.to_string(),"enabled":false}),
    ];
    assert_eq!(
        operation_tags::<TimelineTrackMutation>(track_operations, "operation"),
        [
            "create",
            "delete",
            "rename",
            "set_height",
            "reorder",
            "set_targeted",
            "set_locked",
            "set_sync_locked",
            "set_muted",
            "set_solo",
            "set_enabled"
        ]
    );

    let marker_operations = vec![
        json!({"operation":"create","timeline_id":ROOT.to_string(),"marker_id":MARKER.to_string(),"owner":{"kind":"timeline"},"marked_range":exact_range(12,4,24),"label":"review","flag":"yellow","note":"check the cut","metadata":{}}),
        json!({"operation":"set_range","timeline_id":ROOT.to_string(),"marker_id":MARKER.to_string(),"marked_range":exact_range(13,2,24)}),
        json!({"operation":"set_label","timeline_id":ROOT.to_string(),"marker_id":MARKER.to_string(),"label":null}),
        json!({"operation":"set_flag","timeline_id":ROOT.to_string(),"marker_id":MARKER.to_string(),"flag":"blue"}),
        json!({"operation":"set_note","timeline_id":ROOT.to_string(),"marker_id":MARKER.to_string(),"note":null}),
        json!({"operation":"remove","timeline_id":ROOT.to_string(),"marker_id":MARKER.to_string()}),
    ];
    assert_eq!(
        operation_tags::<TimelineMarkerMutation>(marker_operations, "operation"),
        [
            "create",
            "set_range",
            "set_label",
            "set_flag",
            "set_note",
            "remove"
        ]
    );

    let multicam_source = json!({
        "sync_method":{"kind":"timecode"},
        "angles":[
            {"angle_id":MULTICAM_ANGLE_A.to_string(),"name":"Wide","camera_label":"A","enabled":true,"metadata":{},"source_clip_ids":[MULTICAM_SOURCE_CLIP_A.to_string()]},
            {"angle_id":MULTICAM_ANGLE_B.to_string(),"name":"Close","camera_label":"B","enabled":true,"metadata":{},"source_clip_ids":[MULTICAM_SOURCE_CLIP_B.to_string()]}
        ]
    });
    let multicam_operations = vec![
        json!({"operation":"set_source","timeline_id":MULTICAM_SOURCE.to_string(),"source":multicam_source}),
        json!({"operation":"set_sync_method","timeline_id":MULTICAM_SOURCE.to_string(),"sync_method":{"kind":"manual"}}),
        json!({"operation":"attach_clip","timeline_id":MULTICAM_TARGET.to_string(),"clip_id":MULTICAM_TARGET_CLIP.to_string(),"initial_angle_id":MULTICAM_ANGLE_A.to_string(),"audio_policy":{"kind":"follow_video"}}),
        json!({"operation":"switch_at","timeline_id":MULTICAM_TARGET.to_string(),"clip_id":MULTICAM_TARGET_CLIP.to_string(),"record_time":exact_time(12,24),"angle_id":MULTICAM_ANGLE_B.to_string()}),
        json!({"operation":"move_cut","timeline_id":MULTICAM_TARGET.to_string(),"clip_id":MULTICAM_TARGET_CLIP.to_string(),"at_record_time":exact_time(12,24),"to_record_time":exact_time(10,24)}),
        json!({"operation":"set_audio_policy","timeline_id":MULTICAM_TARGET.to_string(),"clip_id":MULTICAM_TARGET_CLIP.to_string(),"audio_policy":{"kind":"all_angles"}}),
        json!({"operation":"detach_clip","timeline_id":MULTICAM_TARGET.to_string(),"clip_id":MULTICAM_TARGET_CLIP.to_string()}),
    ];
    assert_eq!(
        operation_tags::<TimelineMulticamMutation>(multicam_operations, "operation"),
        [
            "set_source",
            "set_sync_method",
            "attach_clip",
            "switch_at",
            "move_cut",
            "set_audio_policy",
            "detach_clip"
        ]
    );

    let project_actions = vec![
        json!({"action":"select_root_timeline","timeline_id":ROOT.to_string()}),
        json!({"action":"edit_timeline","operations":[{"operation":"append","timeline_id":ROOT.to_string(),"track_id":TRACK.to_string(),"material":gap_item()}]}),
        json!({"action":"mutate_tracks","mutations":[{"operation":"rename","timeline_id":ROOT.to_string(),"track_id":TRACK.to_string(),"name":"Picture"}]}),
        json!({"action":"mutate_markers","mutations":[{"operation":"set_label","timeline_id":ROOT.to_string(),"marker_id":MARKER.to_string(),"label":"Review"}]}),
        json!({"action":"mutate_multicam","mutations":[{"operation":"set_sync_method","timeline_id":MULTICAM_SOURCE.to_string(),"sync_method":{"kind":"manual"}}]}),
        json!({"action":"place_nested_sequence","source_timeline_id":"timeline:0000000000000000000000000000e020","request":{"parent_timeline_id":ROOT.to_string(),"parent_track_id":TRACK.to_string(),"clip_id":"clip:0000000000000000000000000000e022","name":"Nested scene","source_range":exact_range(0,72,24),"placement":{"placement":"replace","target_id":object_id("clip",&CLIP.to_string())}}}),
        json!({"action":"create_compound_clip","request":{"parent_timeline_id":ROOT.to_string(),"compound_timeline_id":"timeline:0000000000000000000000000000e020","name":"Nested scene","selected_objects":[object_id("clip",&CLIP.to_string()),object_id("clip",&SECOND_CLIP.to_string())],"tracks":[{"parent_track_id":TRACK.to_string(),"compound_track_id":"track:0000000000000000000000000000e021","clip_id":"clip:0000000000000000000000000000e022"}]}}),
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
            "mutate_tracks",
            "mutate_markers",
            "mutate_multicam",
            "place_nested_sequence",
            "create_compound_clip",
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
fn public_marker_command_is_exact_visible_evidenced_and_undoable() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut api = project_api();
    let request: ExecuteProjectCommand = serde_json::from_value(json!({
        "transaction_id": "public-marker-controls",
        "expected_project_revision": 0,
        "command": {
            "command": "apply",
            "actions": [{
                "action": "mutate_markers",
                "mutations": [
                    {
                        "operation": "create",
                        "timeline_id": ROOT.to_string(),
                        "marker_id": MARKER.to_string(),
                        "owner": {"kind": "timeline"},
                        "marked_range": exact_range(12, 4, 24),
                        "label": "review",
                        "flag": "yellow",
                        "note": "check the cut",
                        "metadata": {
                            "review.status": {"kind": "text", "value": "pending"}
                        }
                    },
                    {
                        "operation": "set_range",
                        "timeline_id": ROOT.to_string(),
                        "marker_id": MARKER.to_string(),
                        "marked_range": exact_range(13, 2, 24)
                    },
                    {
                        "operation": "set_label",
                        "timeline_id": ROOT.to_string(),
                        "marker_id": MARKER.to_string(),
                        "label": "director review"
                    },
                    {
                        "operation": "set_flag",
                        "timeline_id": ROOT.to_string(),
                        "marker_id": MARKER.to_string(),
                        "flag": "blue"
                    },
                    {
                        "operation": "set_note",
                        "timeline_id": ROOT.to_string(),
                        "marker_id": MARKER.to_string(),
                        "note": "hold this beat"
                    }
                ]
            }]
        }
    }))
    .unwrap();

    let applied = api.execute(request).unwrap();
    assert_eq!(applied.state().project_revision(), 1);
    let ProjectCommandEvidence::Applied { actions } = applied.evidence() else {
        panic!("marker mutation command must retain evidence");
    };
    assert_eq!(
        serde_json::to_value(&actions[0]).unwrap(),
        json!({
            "result":"markers_mutated",
            "revision":1,
            "mutations":["create","set_range","set_label","set_flag","set_note"]
        })
    );
    let snapshot = api.project_snapshot().unwrap();
    let marker = snapshot
        .editorial_project()
        .timeline(ROOT)
        .unwrap()
        .marker(MARKER)
        .unwrap();
    assert_eq!(marker.owner(), engine::MarkerOwner::Timeline);
    assert_eq!(
        marker.marked_range(),
        range(13, 2, engine::FrameRate::FPS_24.timebase())
    );
    assert_eq!(marker.label().unwrap().as_str(), "director review");
    assert_eq!(marker.flag(), Some(engine::MarkerFlag::Blue));
    assert_eq!(marker.note().unwrap().as_str(), "hold this beat");
    assert_eq!(
        marker
            .metadata()
            .get(&engine::MetadataKey::new("review.status").unwrap()),
        Some(&engine::MetadataValue::Text("pending".to_owned()))
    );
    assert_eq!(api.drain_events().unwrap().len(), 1);

    api.execute(ExecuteProjectCommand::new(
        "undo-marker-controls",
        1,
        ProjectCommand::Undo {},
    ))
    .unwrap();
    assert!(api
        .project_snapshot()
        .unwrap()
        .editorial_project()
        .timeline(ROOT)
        .unwrap()
        .marker(MARKER)
        .is_none());
}

#[test]
fn public_multicam_command_is_strict_evidenced_compiled_and_undoable() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut api = multicam_project_api();
    let request: ExecuteProjectCommand = serde_json::from_value(json!({
        "transaction_id":"public-multicam-controls",
        "expected_project_revision":0,
        "command":{
            "command":"apply",
            "actions":[{
                "action":"mutate_multicam",
                "mutations":[
                    {
                        "operation":"set_source",
                        "timeline_id":MULTICAM_SOURCE.to_string(),
                        "source":{
                            "sync_method":{"kind":"timecode"},
                            "angles":[
                                {"angle_id":MULTICAM_ANGLE_A.to_string(),"name":"Wide","camera_label":"A","enabled":true,"metadata":{},"source_clip_ids":[MULTICAM_SOURCE_CLIP_A.to_string()]},
                                {"angle_id":MULTICAM_ANGLE_B.to_string(),"name":"Close","camera_label":"B","enabled":true,"metadata":{},"source_clip_ids":[MULTICAM_SOURCE_CLIP_B.to_string()]}
                            ]
                        }
                    },
                    {
                        "operation":"attach_clip",
                        "timeline_id":MULTICAM_TARGET.to_string(),
                        "clip_id":MULTICAM_TARGET_CLIP.to_string(),
                        "initial_angle_id":MULTICAM_ANGLE_A.to_string(),
                        "audio_policy":{"kind":"follow_video"}
                    },
                    {
                        "operation":"switch_at",
                        "timeline_id":MULTICAM_TARGET.to_string(),
                        "clip_id":MULTICAM_TARGET_CLIP.to_string(),
                        "record_time":exact_time(12,24),
                        "angle_id":MULTICAM_ANGLE_B.to_string()
                    },
                    {
                        "operation":"set_audio_policy",
                        "timeline_id":MULTICAM_TARGET.to_string(),
                        "clip_id":MULTICAM_TARGET_CLIP.to_string(),
                        "audio_policy":{"kind":"all_angles"}
                    }
                ]
            }]
        }
    }))
    .unwrap();

    let applied = api.execute(request).unwrap();
    assert_eq!(applied.state().project_revision(), 1);
    let ProjectCommandEvidence::Applied { actions } = applied.evidence() else {
        panic!("multicam mutation command must retain evidence");
    };
    assert_eq!(
        serde_json::to_value(&actions[0]).unwrap(),
        json!({
            "result":"multicam_mutated",
            "revision":1,
            "mutations":["set_source","attach_clip","switch_at","set_audio_policy"]
        })
    );
    let snapshot = api.project_snapshot().unwrap();
    let source = snapshot
        .editorial_project()
        .timeline(MULTICAM_SOURCE)
        .unwrap()
        .multicam_source()
        .unwrap();
    assert_eq!(source.angles().len(), 2);
    let clip = snapshot
        .editorial_project()
        .timeline(MULTICAM_TARGET)
        .unwrap()
        .multicam_clip(MULTICAM_TARGET_CLIP)
        .unwrap();
    assert_eq!(clip.switches().len(), 2);
    assert_eq!(clip.audio_policy(), &engine::MulticamAudioPolicy::AllAngles);

    api.execute(ExecuteProjectCommand::new(
        "undo-multicam-controls",
        1,
        ProjectCommand::Undo {},
    ))
    .unwrap();
    let snapshot = api.project_snapshot().unwrap();
    assert!(snapshot
        .editorial_project()
        .timeline(MULTICAM_SOURCE)
        .unwrap()
        .multicam_source()
        .is_none());
    assert!(snapshot
        .editorial_project()
        .timeline(MULTICAM_TARGET)
        .unwrap()
        .multicam_clip(MULTICAM_TARGET_CLIP)
        .is_none());
}

#[test]
fn public_nested_and_compound_actions_are_strict_evidenced_and_undoable() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut api = project_api();
    let child_timeline = engine::TimelineId::from_raw(0xe020);
    let child_track = engine::TrackId::from_raw(0xe021);
    let compound_instance = engine::ClipId::from_raw(0xe022);
    let replacement_instance = engine::ClipId::from_raw(0xe023);
    let compound: ExecuteProjectCommand = serde_json::from_value(json!({
        "transaction_id": "public-create-compound",
        "expected_project_revision": 0,
        "command": {
            "command": "apply",
            "actions": [{
                "action": "create_compound_clip",
                "request": {
                    "parent_timeline_id": ROOT.to_string(),
                    "compound_timeline_id": child_timeline.to_string(),
                    "name": "Nested scene",
                    "selected_objects": [
                        object_id("clip", &CLIP.to_string()),
                        object_id("clip", &SECOND_CLIP.to_string())
                    ],
                    "tracks": [{
                        "parent_track_id": TRACK.to_string(),
                        "compound_track_id": child_track.to_string(),
                        "clip_id": compound_instance.to_string()
                    }]
                }
            }]
        }
    }))
    .unwrap();
    let serialized = serde_json::to_value(&compound).unwrap();
    let mut unknown = serialized.clone();
    unknown["command"]["actions"][0]["request"]["guessed"] = json!(true);
    assert!(serde_json::from_value::<ExecuteProjectCommand>(unknown).is_err());

    let applied = api.execute(compound).unwrap();
    assert_eq!(applied.state().project_revision(), 1);
    let ProjectCommandEvidence::Applied { actions } = applied.evidence() else {
        panic!("compound command must retain action evidence");
    };
    assert_eq!(
        serde_json::to_value(&actions[0]).unwrap(),
        json!({
            "result": "compound_clip_created",
            "revision": 1,
            "compound_timeline_id": child_timeline.to_string(),
            "clip_ids": [compound_instance.to_string()]
        })
    );
    let snapshot = api.project_snapshot().unwrap();
    assert!(snapshot
        .editorial_project()
        .timeline(child_timeline)
        .unwrap()
        .track(child_track)
        .unwrap()
        .item(engine::EditorialObjectId::Clip(CLIP))
        .is_some());
    assert_eq!(api.drain_events().unwrap().len(), 1);

    let place: ExecuteProjectCommand = serde_json::from_value(json!({
        "transaction_id": "public-place-nested",
        "expected_project_revision": 1,
        "command": {
            "command": "apply",
            "actions": [{
                "action": "place_nested_sequence",
                "source_timeline_id": child_timeline.to_string(),
                "request": {
                    "parent_timeline_id": ROOT.to_string(),
                    "parent_track_id": TRACK.to_string(),
                    "clip_id": replacement_instance.to_string(),
                    "name": "Nested scene replacement",
                    "source_range": exact_range(0, 72, 24),
                    "placement": {
                        "placement": "replace",
                        "target_id": object_id("clip", &compound_instance.to_string())
                    }
                }
            }]
        }
    }))
    .unwrap();
    let placed = api.execute(place).unwrap();
    let ProjectCommandEvidence::Applied { actions } = placed.evidence() else {
        panic!("nested placement must retain action evidence");
    };
    assert_eq!(
        serde_json::to_value(&actions[0]).unwrap(),
        json!({
            "result": "nested_sequence_placed",
            "revision": 2,
            "source_timeline_id": child_timeline.to_string(),
            "clip_ids": [replacement_instance.to_string()]
        })
    );
    assert!(api
        .project_snapshot()
        .unwrap()
        .editorial_project()
        .timeline(ROOT)
        .unwrap()
        .track(TRACK)
        .unwrap()
        .item(engine::EditorialObjectId::Clip(replacement_instance))
        .is_some());
    assert_eq!(api.drain_events().unwrap().len(), 1);

    api.execute(ExecuteProjectCommand::new(
        "undo-public-place-nested",
        2,
        ProjectCommand::Undo {},
    ))
    .unwrap();
    assert!(api
        .project_snapshot()
        .unwrap()
        .editorial_project()
        .timeline(ROOT)
        .unwrap()
        .track(TRACK)
        .unwrap()
        .item(engine::EditorialObjectId::Clip(compound_instance))
        .is_some());
}

#[test]
fn public_track_command_is_revision_fenced_evidenced_and_undoable() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut api = project_api();
    let request: ExecuteProjectCommand = serde_json::from_value(json!({
        "transaction_id": "public-track-controls",
        "expected_project_revision": 0,
        "command": {
            "command": "apply",
            "actions": [{
                "action": "mutate_tracks",
                "mutations": [
                    {"operation":"rename","timeline_id":ROOT.to_string(),"track_id":TRACK.to_string(),"name":"Picture"},
                    {"operation":"set_height","timeline_id":ROOT.to_string(),"track_id":TRACK.to_string(),"height":104},
                    {"operation":"set_targeted","timeline_id":ROOT.to_string(),"track_id":TRACK.to_string(),"targeted":true},
                    {"operation":"set_sync_locked","timeline_id":ROOT.to_string(),"track_id":TRACK.to_string(),"sync_locked":false},
                    {"operation":"set_enabled","timeline_id":ROOT.to_string(),"track_id":TRACK.to_string(),"enabled":false}
                ]
            }]
        }
    }))
    .unwrap();

    let applied = api.execute(request).unwrap();
    assert_eq!(applied.state().project_revision(), 1);
    let ProjectCommandEvidence::Applied { actions } = applied.evidence() else {
        panic!("track mutation command must retain evidence");
    };
    assert_eq!(
        serde_json::to_value(&actions[0]).unwrap(),
        json!({
            "result":"tracks_mutated",
            "revision":1,
            "mutations":["rename","set_height","set_targeted","set_sync_locked","set_enabled"]
        })
    );
    let snapshot = api.project_snapshot().unwrap();
    let timeline = snapshot.editorial_project().timeline(ROOT).unwrap();
    assert_eq!(timeline.track(TRACK).unwrap().name(), "Picture");
    let state = timeline.edit_state().track_state(TRACK).unwrap();
    assert_eq!(state.height(), 104);
    assert!(state.targeted());
    assert!(!state.sync_locked());
    assert!(!state.enabled());
    assert_eq!(api.drain_events().unwrap().len(), 1);

    api.execute(ExecuteProjectCommand::new(
        "undo-track-controls",
        1,
        ProjectCommand::Undo {},
    ))
    .unwrap();
    assert_eq!(
        api.project_snapshot()
            .unwrap()
            .editorial_project()
            .timeline(ROOT)
            .unwrap()
            .track(TRACK)
            .unwrap()
            .name(),
        "V1"
    );
}

#[test]
fn public_retime_is_evidenced_compiled_persisted_replayed_and_reversible() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut api = project_api();
    let initial = api.project_snapshot().unwrap();
    let initial_map = initial
        .editorial_project()
        .timeline(ROOT)
        .unwrap()
        .track(TRACK)
        .unwrap()
        .item(engine::EditorialObjectId::Clip(CLIP))
        .unwrap()
        .as_clip()
        .unwrap()
        .time_map()
        .clone();
    let request_value = json!({
        "transaction_id": "public-retime",
        "expected_project_revision": 0,
        "command": {
            "command": "apply",
            "actions": [{
                "action": "edit_timeline",
                "operations": [{
                    "operation": "retime",
                    "timeline_id": ROOT.to_string(),
                    "track_id": TRACK.to_string(),
                    "clip_id": CLIP.to_string(),
                    "time_map": retime_map()
                }]
            }]
        }
    });
    let request: ExecuteProjectCommand = serde_json::from_value(request_value.clone()).unwrap();

    let applied = api.execute(request).unwrap();
    assert_eq!(applied.state().project_revision(), 1);
    assert_eq!(applied.state().undo_depth(), 1);
    assert_eq!(
        serde_json::to_value(applied.evidence()).unwrap(),
        json!({
            "result": "applied",
            "actions": [{
                "result": "timeline_edited",
                "revision": 1,
                "operations": ["retime"]
            }]
        })
    );

    let edit_rate = engine::FrameRate::FPS_24.timebase();
    let source_rate = engine::Timebase::integer(48).unwrap();
    let expected = engine::ClipTimeMap::new(
        engine::Duration::new(48, edit_rate).unwrap(),
        source_rate,
        [
            engine::RetimeSegment::new(
                range(0, 24, edit_rate),
                engine::RationalTime::new(48, source_rate),
                engine::PlaybackRate::new(2, 1).unwrap(),
            )
            .unwrap(),
            engine::RetimeSegment::new(
                range(24, 24, edit_rate),
                engine::RationalTime::new(144, source_rate),
                engine::PlaybackRate::new(0, 1).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap();
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
    assert_eq!(clip.time_map(), &expected);
    assert_eq!(clip.record_range(), range(0, 48, edit_rate));

    let compilation = published.timeline_graph(ROOT).unwrap();
    let clip_node = compilation
        .index()
        .node(engine::TimelineGraphOrigin::Object(
            engine::EditorialObjectId::Clip(CLIP),
        ))
        .unwrap();
    let compiled_graph = compilation.snapshot();
    let graph_time_map = compiled_graph
        .node(clip_node)
        .unwrap()
        .parameters()
        .values()
        .find(|parameter| parameter.name().as_str() == "time-map")
        .unwrap()
        .value()
        .payload()
        .as_domain()
        .unwrap();
    assert_eq!(
        graph_time_map,
        &engine::TimelineGraphValue::ClipTimeMap(expected.clone())
    );

    let mut database = engine::ProjectDatabase::memory().unwrap();
    database.replace(&published).unwrap();
    assert_eq!(database.load().unwrap().snapshot(), published);

    let log = api
        .execute(GetProjectCommandLog::new(
            0,
            256,
            ProjectCommandLogDetail::Replayable,
        ))
        .unwrap();
    let GetProjectCommandLogResult::Records(log) = log else {
        panic!("retime command must have replayable command-log evidence");
    };
    assert_eq!(log.records().len(), 1);
    assert_eq!(
        serde_json::to_value(log.records()[0].replay_request().unwrap()).unwrap(),
        request_value
    );

    api.execute(ExecuteProjectCommand::new(
        "undo-public-retime",
        1,
        ProjectCommand::Undo {},
    ))
    .unwrap();
    let undone = api.project_snapshot().unwrap();
    assert_eq!(
        undone
            .editorial_project()
            .timeline(ROOT)
            .unwrap()
            .track(TRACK)
            .unwrap()
            .item(engine::EditorialObjectId::Clip(CLIP))
            .unwrap()
            .as_clip()
            .unwrap()
            .time_map(),
        &initial_map
    );

    api.execute(ExecuteProjectCommand::new(
        "redo-public-retime",
        2,
        ProjectCommand::Redo {},
    ))
    .unwrap();
    let redone = api.project_snapshot().unwrap();
    assert_eq!(
        redone
            .editorial_project()
            .timeline(ROOT)
            .unwrap()
            .track(TRACK)
            .unwrap()
            .item(engine::EditorialObjectId::Clip(CLIP))
            .unwrap()
            .as_clip()
            .unwrap()
            .time_map(),
        &expected
    );
}

#[test]
fn inexact_public_retime_fails_before_project_state_changes() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut api = project_api();
    let before = api.project_snapshot().unwrap();
    let request: ExecuteProjectCommand = serde_json::from_value(json!({
        "transaction_id": "inexact-public-retime",
        "expected_project_revision": 0,
        "command": {
            "command": "apply",
            "actions": [{
                "action": "edit_timeline",
                "operations": [{
                    "operation": "retime",
                    "timeline_id": ROOT.to_string(),
                    "track_id": TRACK.to_string(),
                    "clip_id": CLIP.to_string(),
                    "time_map": {
                        "record_duration": exact_duration(48, 24),
                        "source_timebase": {"numerator": 48, "denominator": 1},
                        "segments": [
                            {
                                "record_range": exact_range(0, 1, 24),
                                "source_start": exact_time(48, 48),
                                "rate_numerator": 1,
                                "rate_denominator": 3
                            },
                            {
                                "record_range": exact_range(1, 47, 24),
                                "source_start": exact_time(49, 48),
                                "rate_numerator": 1,
                                "rate_denominator": 1
                            }
                        ]
                    }
                }]
            }]
        }
    }))
    .unwrap();

    assert_eq!(
        api.execute(request).unwrap_err().category(),
        engine::ErrorCategory::InvalidInput
    );
    assert_eq!(api.project_snapshot().unwrap(), before);
    assert!(api.drain_events().unwrap().is_empty());
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
                    }, {
                        "operation": "set_transition",
                        "timeline_id": ROOT.to_string(),
                        "track_id": TRACK.to_string(),
                        "transition_id": TRANSITION.to_string(),
                        "from_offset": exact_duration(6, 24),
                        "to_offset": exact_duration(2, 24)
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
    let transition = published
        .editorial_project()
        .timeline(ROOT)
        .unwrap()
        .track(TRACK)
        .unwrap()
        .item(engine::EditorialObjectId::Transition(TRANSITION))
        .unwrap();
    let engine::TrackItem::Transition(transition) = transition else {
        panic!("public transition identity must remain a transition");
    };
    assert_eq!(transition.from_offset().value(), 6);
    assert_eq!(transition.to_offset().value(), 2);
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
