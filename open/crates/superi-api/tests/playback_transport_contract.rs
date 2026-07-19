use std::sync::Arc;
use std::time::Instant;

use serde_json::json;
use superi_api::commands::{ApiCommand, GetEditorState};
use superi_api::editor::ProjectEditorApi;
use superi_api::playback::{ExecutePlaybackTransport, PlaybackTransportAction};
use superi_api::version::{EXECUTE_PLAYBACK_TRANSPORT_METHOD, PLAYBACK_TRANSPORT_SCHEMA_VERSION};
use superi_concurrency::jobs::{BoundedWorkerPool, WorkerPoolConfig};
use superi_concurrency::lifecycle::LifecyclePhase;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::ids::{ProjectId, TimelineId};
use superi_core::time::{RationalTime, TimeRange, Timebase};
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult,
    EngineTransactionId,
};
use superi_engine::playback_runtime::PlaybackControlRuntime;
use superi_engine::test_support::empty_project_document;

const PROJECT: ProjectId = ProjectId::from_raw(0xb501);
const ROOT: TimelineId = TimelineId::from_raw(0xb502);

#[test]
fn strict_public_transport_is_immediately_accepted_and_completes_in_editor_state() {
    let base = Instant::now();
    let frame_clock = Timebase::integer(24).unwrap();
    let pool = Arc::new(BoundedWorkerPool::new(WorkerPoolConfig::new(2, 32).unwrap()).unwrap());

    let engine_domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let (mut dispatcher, executor) = EngineCommandDispatcher::new_with_playback_bridge().unwrap();
    dispatcher
        .attach_project(empty_project_document(PROJECT, ROOT, frame_clock).unwrap())
        .unwrap();
    drive_running(&mut dispatcher);
    dispatcher.drain_events().unwrap();
    let mut api = ProjectEditorApi::new(dispatcher).unwrap();
    drop(engine_domain);

    let bounds = TimeRange::from_start_end(
        RationalTime::new(0, frame_clock),
        RationalTime::new(24, frame_clock),
    )
    .unwrap();
    let playback_domain = ExecutionDomain::Playback.enter_current().unwrap();
    let mut runtime = PlaybackControlRuntime::new(&pool, executor, bounds, 0xb503, base).unwrap();
    drop(playback_domain);

    let engine_domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let accepted = api
        .execute(ExecutePlaybackTransport::new(
            "public-play",
            PlaybackTransportAction::Play {},
        ))
        .unwrap();
    assert_eq!(
        ExecutePlaybackTransport::METHOD,
        EXECUTE_PLAYBACK_TRANSPORT_METHOD
    );
    assert_eq!(
        ExecutePlaybackTransport::SCHEMA_VERSION,
        PLAYBACK_TRANSPORT_SCHEMA_VERSION
    );
    assert_eq!(accepted.schema_version().to_string(), "1.0.0");
    assert_eq!(accepted.transaction_id(), "public-play");
    assert!(accepted.accepted());
    assert!(accepted.pending_command());
    drop(engine_domain);

    let playback_domain = ExecutionDomain::Playback.enter_current().unwrap();
    let tick = runtime.tick(base).unwrap();
    assert_eq!(
        tick.command().unwrap().command_sequence(),
        accepted.command_sequence()
    );
    drop(playback_domain);

    let engine_domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let state = api
        .execute(GetEditorState::new("state-after-play"))
        .unwrap();
    let value = serde_json::to_value(state.snapshot()).unwrap();
    assert_eq!(value["playback"]["status"], "attached");
    assert_eq!(value["playback"]["pending_command"], false);
    assert_eq!(value["playback"]["latest"]["mode"], "playing");
    assert_eq!(value["playback"]["latest"]["playhead"]["value"], 0);
    assert_eq!(
        value["playback"]["latest"]["degradation"],
        json!([
            "audio_discard_pending",
            "viewport_output_unavailable",
            "audio_output_unavailable"
        ])
    );
    assert!(api.drain_events().unwrap().is_empty());
    drop(engine_domain);

    drop(runtime);
    match Arc::try_unwrap(pool) {
        Ok(pool) => {
            pool.shutdown().unwrap();
        }
        Err(_) => panic!("playback runtime retained the worker pool"),
    }
}

#[test]
fn playback_wire_rejects_unknown_fields_and_covers_every_navigation_action() {
    let actions = [
        json!({"action":"inspect"}),
        json!({"action":"play"}),
        json!({"action":"pause"}),
        json!({"action":"stop"}),
        json!({"action":"seek","target":{"value":12,"timebase":{"numerator":24,"denominator":1}}}),
        json!({"action":"begin_scrub"}),
        json!({"action":"scrub_to","target":{"value":13,"timebase":{"numerator":24,"denominator":1}}}),
        json!({"action":"end_scrub","resume":true}),
        json!({"action":"set_loop","enabled":true}),
        json!({"action":"set_rate","numerator":3,"denominator":2}),
        json!({"action":"set_direction","direction":"reverse"}),
        json!({"action":"shuttle","numerator":-4,"denominator":1}),
        json!({"action":"step_frames","delta":-1}),
    ];
    for (index, command) in actions.into_iter().enumerate() {
        serde_json::from_value::<ExecutePlaybackTransport>(json!({
            "transaction_id":format!("action-{index}"),
            "command":command,
        }))
        .unwrap();
    }
    assert!(serde_json::from_value::<ExecutePlaybackTransport>(json!({
        "transaction_id":"unknown",
        "command":{"action":"play","guessed":true},
    }))
    .is_err());
    assert!(serde_json::from_value::<ExecutePlaybackTransport>(json!({
        "transaction_id":"unknown-root",
        "command":{"action":"pause"},
        "guessed":true,
    }))
    .is_err());
}

fn drive_running(dispatcher: &mut EngineCommandDispatcher) {
    loop {
        let inspected = dispatcher
            .dispatch(EngineCommandRequest::new(
                EngineTransactionId::new("inspect-startup").unwrap(),
                EngineCommand::InspectLifecycle,
            ))
            .unwrap();
        let EngineCommandResult::Lifecycle(snapshot) = inspected.result() else {
            panic!("lifecycle inspection returned an unexpected result")
        };
        if snapshot.phase() == LifecyclePhase::Running {
            return;
        }
        dispatcher
            .dispatch(EngineCommandRequest::new(
                EngineTransactionId::new("complete-startup").unwrap(),
                EngineCommand::CompleteLifecycleAction(snapshot.pending_action().unwrap()),
            ))
            .unwrap();
    }
}
