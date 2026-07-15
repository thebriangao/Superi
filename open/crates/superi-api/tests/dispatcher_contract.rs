use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use superi_api::commands::{ApiCommand, ExecuteScenarioAction, ExecuteScenarioTransaction};
use superi_api::events::{ApiEvent, ScenarioStateChanged};
use superi_api::scenario::{
    ExactFrameRate, ScenarioApi, ScenarioPhase, ScenarioTransactionResult, SliceAction,
    SliceActionKind, SliceGraphEffect,
};
use superi_api::version::{EXECUTE_SCENARIO_TRANSACTION_METHOD, SCENARIO_STATE_CHANGED_EVENT};

#[test]
fn typed_automation_transaction_is_atomic_revision_fenced_and_evented() {
    let directory = test_directory("atomic");
    let source = directory.join("input.webm");
    let source_bytes = b"public dispatcher transaction fixture";
    fs::write(&source, source_bytes).unwrap();
    let mut api = ScenarioApi::new();

    let invalid = ExecuteScenarioTransaction::new(
        "automation-invalid",
        0,
        vec![
            import_action(source.clone(), source_bytes),
            SliceAction::PlaceClip {
                timeline_start_frame: 1,
            },
        ],
    );
    let failure = api
        .execute_transaction(invalid)
        .expect_err("a failing trailing action rolls back the whole transaction");
    assert_eq!(failure.category(), "invalid_input");
    assert_eq!(failure.state().revision(), 0);
    assert_eq!(api.snapshot().phase(), ScenarioPhase::Empty);
    assert!(api.drain_events().is_empty());

    let command = ExecuteScenarioTransaction::new(
        "automation-commit",
        0,
        vec![
            import_action(source, source_bytes),
            SliceAction::PlaceClip {
                timeline_start_frame: 0,
            },
            SliceAction::TrimClip {
                source_start_frame: 24,
                source_end_frame: 72,
            },
            SliceAction::ApplyGraphEffect {
                effect: SliceGraphEffect::HorizontalMirror,
            },
        ],
    );
    let wire = serde_json::to_value(&command).unwrap();
    assert_eq!(wire["transaction_id"], "automation-commit");
    assert_eq!(wire["expected_revision"], 0);
    assert_eq!(wire["actions"].as_array().unwrap().len(), 4);
    let mut unknown_command = wire.clone();
    unknown_command["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<ExecuteScenarioTransaction>(unknown_command).is_err());
    let round_trip: ExecuteScenarioTransaction = serde_json::from_value(wire).unwrap();

    let result = api.execute_transaction(round_trip).unwrap();
    assert_transaction_result(&result);
    assert_eq!(result.transaction_id(), "automation-commit");
    assert_eq!(result.command_sequence(), 1);
    assert_eq!(result.state().revision(), 1);
    assert_eq!(result.state().phase(), ScenarioPhase::Effected);
    assert_eq!(result.actions().len(), 4);
    assert_eq!(
        result.actions(),
        [
            SliceActionKind::ImportClip,
            SliceActionKind::PlaceClip,
            SliceActionKind::TrimClip,
            SliceActionKind::ApplyGraphEffect,
        ]
    );

    let events = api.drain_events();
    assert_eq!(events.len(), 1);
    let event = &events[0];
    assert_eq!(event.sequence(), 1);
    assert_eq!(event.command_sequence(), result.command_sequence());
    assert_eq!(event.transaction_id(), "automation-commit");
    assert_eq!(event.project_revision(), 1);
    assert_eq!(event.state(), result.state());
    let event_wire = serde_json::to_value(event).unwrap();
    let mut unknown_event = event_wire.clone();
    unknown_event["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<ScenarioStateChanged>(unknown_event).is_err());
    let event_round_trip: ScenarioStateChanged = serde_json::from_value(event_wire).unwrap();
    assert_eq!(event_round_trip, *event);

    let stale = ExecuteScenarioTransaction::new("automation-stale", 0, vec![SliceAction::Undo {}]);
    let failure = api.execute_transaction(stale).unwrap_err();
    assert_eq!(failure.category(), "conflict");
    assert_eq!(failure.state().revision(), 1);
    assert!(api.drain_events().is_empty());

    let undo = ExecuteScenarioTransaction::new("automation-undo", 1, vec![SliceAction::Undo {}]);
    let undone = api.execute_transaction(undo).unwrap();
    assert_eq!(undone.state().revision(), 2);
    assert_eq!(undone.state().phase(), ScenarioPhase::Empty);
    let event = api.drain_events().pop().unwrap();
    assert_eq!(event.sequence(), 2);
    assert_eq!(event.command_sequence(), 2);

    let inspect =
        ExecuteScenarioTransaction::new("automation-inspect", 2, vec![SliceAction::Inspect {}]);
    let inspected = api.execute_transaction(inspect).unwrap();
    assert_eq!(inspected.command_sequence(), 3);
    assert_eq!(inspected.actions(), [SliceActionKind::Inspect]);
    assert_eq!(inspected.state(), undone.state());
    assert!(api.drain_events().is_empty());

    let redo = ExecuteScenarioTransaction::new("automation-redo", 2, vec![SliceAction::Redo {}]);
    let redone = api.execute_transaction(redo).unwrap();
    assert_eq!(redone.command_sequence(), 4);
    assert_eq!(redone.state().revision(), 3);
    let event = api.drain_events().pop().unwrap();
    assert_eq!(event.sequence(), 3);
    assert_eq!(event.command_sequence(), 4);
    assert_eq!(event.state(), redone.state());
}

#[test]
fn legacy_action_command_routes_through_the_same_dispatcher_and_event_vocabulary() {
    let directory = test_directory("legacy");
    let source = directory.join("input.webm");
    let source_bytes = b"legacy action fixture";
    fs::write(&source, source_bytes).unwrap();
    let mut api = ScenarioApi::new();

    let result = api
        .execute(ExecuteScenarioAction::new(import_action(
            source,
            source_bytes,
        )))
        .unwrap();
    assert_eq!(result.action(), SliceActionKind::ImportClip);
    assert_eq!(result.state().revision(), 1);

    let events = api.drain_events();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].sequence(), 1);
    assert_eq!(events[0].project_revision(), 1);
    assert_eq!(events[0].state(), result.state());
    assert!(events[0].transaction_id().starts_with("legacy-scenario-"));
}

#[test]
fn transaction_and_event_names_are_permanent_namespaced_contracts() {
    assert_eq!(
        ExecuteScenarioTransaction::METHOD,
        EXECUTE_SCENARIO_TRANSACTION_METHOD
    );
    assert_eq!(ScenarioStateChanged::NAME, SCENARIO_STATE_CHANGED_EVENT);
    assert_eq!(
        EXECUTE_SCENARIO_TRANSACTION_METHOD,
        "superi.slice.scenario.transaction.execute"
    );
    assert_eq!(
        SCENARIO_STATE_CHANGED_EVENT,
        "superi.slice.scenario.state.changed"
    );
}

fn assert_transaction_result(_: &ScenarioTransactionResult) {}

fn import_action(path: PathBuf, payload: &[u8]) -> SliceAction {
    SliceAction::ImportClip {
        path: path.display().to_string(),
        fixture_id: "slice/video-cfr".to_owned(),
        fixture_version: 1,
        manifest_sha256: "1d2b28b5f44c7f86dce50d67b718b0fad967d267d9016961e3d71bb9dab94419"
            .to_owned(),
        payload_sha256: format!("{:x}", Sha256::digest(payload)),
        frame_rate: ExactFrameRate::new(24, 1),
        frame_count: 96,
        width: 96,
        height: 54,
    }
}

fn test_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "superi-api-p2-w06-c007-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
