use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use superi_api::commands::{ApiCommand, GetEditorState};
use superi_api::editor::ProjectEditorApi;
use superi_api::state::EditorStateSnapshot;
use superi_api::version::{EDITOR_STATE_SCHEMA_VERSION, GET_EDITOR_STATE_METHOD};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::ids::{ProjectId, TimelineId};
use superi_core::time::Timebase;
use superi_engine::dispatcher::{
    AudioAutomationState, EngineCommand, EngineCommandDispatcher, EngineCommandRequest,
    EngineTransactionId,
};
use superi_engine::editor as engine;
use superi_engine::export_dispatch::EngineExportJobCommand;
use superi_engine::export_jobs::ExportJobQueueConfig;
use superi_engine::test_support::empty_project_document;
use superi_engine::transport::PlaybackTransportCommand;

const PROJECT: ProjectId = ProjectId::from_raw(0xa017);
const ROOT: TimelineId = TimelineId::from_raw(0xa018);
const SOURCE: TimelineId = TimelineId::from_raw(0xa019);
const AUDIO_TRACK: engine::TrackId = engine::TrackId::from_raw(0xa01a);
const SOURCE_TRACK: engine::TrackId = engine::TrackId::from_raw(0xa01b);
const LEFT_CLIP: engine::ClipId = engine::ClipId::from_raw(0xa01c);
const RIGHT_CLIP: engine::ClipId = engine::ClipId::from_raw(0xa01d);

fn audio_semantics() -> engine::TrackSemantics {
    let source = engine::ChannelLayout::new([
        engine::ChannelPosition::FrontLeft,
        engine::ChannelPosition::FrontRight,
        engine::ChannelPosition::FrontCenter,
    ])
    .unwrap();
    let routing = engine::AudioRouting::new(
        engine::AudioRouteDestination::Main,
        engine::ChannelLayout::stereo(),
        [
            engine::AudioChannelRoute::new(
                engine::ChannelPosition::FrontLeft,
                engine::AudioChannelTarget::Channel(engine::ChannelPosition::FrontLeft),
            ),
            engine::AudioChannelRoute::new(
                engine::ChannelPosition::FrontRight,
                engine::AudioChannelTarget::Channel(engine::ChannelPosition::FrontRight),
            ),
            engine::AudioChannelRoute::new(
                engine::ChannelPosition::FrontCenter,
                engine::AudioChannelTarget::Muted,
            ),
        ],
    )
    .unwrap();
    engine::TrackSemantics::Audio(
        engine::AudioTrackSemantics::new(48_000, source, routing).unwrap(),
    )
}

fn sample_range(start: i64, sample_count: u64) -> engine::TimeRange {
    let clock = engine::Timebase::integer(48_000).unwrap();
    engine::TimeRange::new(
        engine::RationalTime::new(start, clock),
        engine::Duration::new(sample_count, clock).unwrap(),
    )
    .unwrap()
}

fn audio_project_document() -> engine::ProjectDocument {
    let clock = engine::Timebase::integer(48_000).unwrap();
    let source = engine::Timeline::new(
        SOURCE,
        "source audio",
        clock,
        engine::RationalTime::zero(clock),
        vec![engine::Track::new(
            SOURCE_TRACK,
            "source audio track",
            audio_semantics(),
            vec![engine::TrackItem::Gap(engine::Gap::new(
                engine::GapId::from_raw(0xa01e),
                "source extent",
                sample_range(0, 2_000),
            ))],
        )],
    );
    let root = engine::Timeline::new(
        ROOT,
        "editor audio",
        clock,
        engine::RationalTime::zero(clock),
        vec![engine::Track::new(
            AUDIO_TRACK,
            "routed audio",
            audio_semantics(),
            vec![
                engine::TrackItem::Clip(
                    engine::Clip::new(
                        LEFT_CLIP,
                        "left",
                        engine::ClipSource::Timeline(SOURCE),
                        sample_range(0, 500),
                        sample_range(0, 500),
                    )
                    .unwrap(),
                ),
                engine::TrackItem::Gap(engine::Gap::new(
                    engine::GapId::from_raw(0xa01f),
                    "intentional silence",
                    sample_range(500, 100),
                )),
                engine::TrackItem::Clip(
                    engine::Clip::new(
                        RIGHT_CLIP,
                        "right",
                        engine::ClipSource::Timeline(SOURCE),
                        sample_range(500, 500),
                        sample_range(600, 500),
                    )
                    .unwrap(),
                ),
            ],
        )],
    );
    let project =
        engine::EditorialProject::new(PROJECT, "audio editor state", [], [root, source]).unwrap();
    engine::ProjectDocument::new(project, ROOT).unwrap()
}

#[test]
fn complete_editor_state_is_one_strict_deterministic_public_query() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut dispatcher = EngineCommandDispatcher::new().unwrap();
    dispatcher
        .attach_project(
            empty_project_document(PROJECT, ROOT, Timebase::integer(24).unwrap()).unwrap(),
        )
        .unwrap();
    let mut api = ProjectEditorApi::new(dispatcher).unwrap();

    let first = api
        .execute(GetEditorState::new("editor-state-first"))
        .unwrap();
    let second = api
        .execute(GetEditorState::new("editor-state-second"))
        .unwrap();

    assert_eq!(GetEditorState::METHOD, GET_EDITOR_STATE_METHOD);
    assert_eq!(GetEditorState::SCHEMA_VERSION, EDITOR_STATE_SCHEMA_VERSION);
    assert_eq!(first.transaction_id(), "editor-state-first");
    assert_eq!(second.transaction_id(), "editor-state-second");
    assert_eq!(second.command_sequence(), first.command_sequence() + 1);
    assert_eq!(first.snapshot(), second.snapshot());
    assert_eq!(first.snapshot().schema_version().to_string(), "1.0.0");

    let encoded = serde_json::to_vec(first.snapshot()).unwrap();
    let decoded: EditorStateSnapshot = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(&decoded, first.snapshot());
    let value: Value = serde_json::from_slice(&encoded).unwrap();
    let object = value.as_object().unwrap();
    assert_eq!(object.len(), 11);
    for root in [
        "project", "timeline", "graph", "media", "audio", "color", "effect", "ai", "playback",
        "export",
    ] {
        assert!(object.contains_key(root), "missing {root} state root");
    }
    assert_eq!(value["project"]["project_id"], PROJECT.to_string());
    assert_eq!(value["project"]["project_revision"], 0);
    assert_eq!(value["timeline"]["document"]["format"], "superi.timeline");
    let timeline_document = first.snapshot().timeline().document();
    let timeline_content = timeline_document.content_json().as_bytes();
    assert_eq!(
        timeline_document.byte_length(),
        timeline_content.len() as u64
    );
    assert_eq!(
        timeline_document.sha256(),
        format!("{:x}", Sha256::digest(timeline_content))
    );
    assert!(value["timeline"]["document"]["content"].is_object());
    assert_eq!(value["audio"]["audio_track_count"], 0);
    assert_eq!(value["color"]["project_revision"], 0);
    assert_eq!(value["color"]["management"]["kind"], "built_in_aces_cg");
    assert_eq!(value["color"]["management"]["working_space"], "acescg");
    assert_eq!(value["color"]["render_target"], "project_working");
    assert_eq!(value["playback"]["status"], "detached");
    assert_eq!(value["export"]["status"], "detached");
    assert_eq!(value["ai"]["runtime_availability"], "not_implemented");

    let mut unknown = value;
    unknown["guessed"] = json!(true);
    assert!(serde_json::from_value::<EditorStateSnapshot>(unknown).is_err());
}

#[test]
fn complete_editor_state_keeps_optional_runtime_owners_explicit_without_polling() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let (mut dispatcher, _playback_executor) =
        EngineCommandDispatcher::new_with_playback_bridge().unwrap();
    dispatcher
        .attach_project(
            empty_project_document(PROJECT, ROOT, Timebase::integer(24).unwrap()).unwrap(),
        )
        .unwrap();
    dispatcher
        .attach_audio_automation(AudioAutomationState::new())
        .unwrap();
    let _export_runtime = dispatcher
        .attach_export_jobs::<u64>(ExportJobQueueConfig::new(1, 1, 2).unwrap())
        .unwrap();
    dispatcher
        .dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("public-state-playback-pending").unwrap(),
            EngineCommand::ExecutePlayback(PlaybackTransportCommand::Inspect),
        ))
        .unwrap();
    dispatcher
        .dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("public-state-export-observed").unwrap(),
            EngineCommand::ExecuteExportJob(EngineExportJobCommand::InspectAll),
        ))
        .unwrap();

    let mut api = ProjectEditorApi::new(dispatcher).unwrap();
    let result = api
        .execute(GetEditorState::new("public-state-runtime"))
        .unwrap();
    let value = serde_json::to_value(result.snapshot()).unwrap();
    assert_eq!(value["audio"]["automation"]["status"], "attached");
    assert_eq!(value["audio"]["automation"]["state"]["revision"], 0);
    assert_eq!(value["playback"]["status"], "attached");
    assert_eq!(value["playback"]["pending_command"], true);
    assert!(value["playback"]["latest"].is_null());
    assert_eq!(value["export"]["status"], "attached");
    assert_eq!(value["export"]["latest"]["revision"], 0);
    assert!(value["export"]["latest"]["jobs"]
        .as_array()
        .unwrap()
        .is_empty());
}

#[test]
fn complete_editor_state_preserves_public_audio_timing_channels_and_routing() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut dispatcher = EngineCommandDispatcher::new().unwrap();
    dispatcher.attach_project(audio_project_document()).unwrap();

    let mut api = ProjectEditorApi::new(dispatcher).unwrap();
    let result = api
        .execute(GetEditorState::new("public-state-audio"))
        .unwrap();
    let value = serde_json::to_value(result.snapshot()).unwrap();
    let track = &value["audio"]["tracks"][0];

    assert_eq!(value["audio"]["audio_track_count"], 2);
    assert_eq!(track["timeline_id"], ROOT.to_string());
    assert_eq!(track["track_id"], AUDIO_TRACK.to_string());
    assert_eq!(track["sample_rate"], 48_000);
    assert_eq!(
        track["source_channels"],
        json!(["front_left", "front_right", "front_center"])
    );
    assert_eq!(track["destination"]["kind"], "main");
    assert_eq!(
        track["destination_channels"],
        json!(["front_left", "front_right"])
    );
    assert_eq!(track["routes"][2]["source"], "front_center");
    assert_eq!(track["routes"][2]["target"]["kind"], "muted");
    assert_eq!(track["clip_count"], 2);
    assert_eq!(track["continuity"]["status"], "audited");
    assert_eq!(track["continuity"]["uninterrupted_record_coverage"], false);
    assert_eq!(
        track["continuity"]["seams"][0]["record"],
        json!({"kind": "gap", "sample_count": 100})
    );
    assert_eq!(
        track["continuity"]["seams"][0]["source"],
        json!({
            "kind": "different_clip",
            "left": LEFT_CLIP.to_string(),
            "right": RIGHT_CLIP.to_string(),
        })
    );
}
