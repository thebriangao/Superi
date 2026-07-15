use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use superi_concurrency::lifecycle::LifecyclePhase;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, ErrorContext, Recoverability};
use superi_core::time::FrameRate;
use superi_engine::command::{
    GraphEffect, ScenarioAction, ScenarioPhase, CANONICAL_FIXTURE_ID, CANONICAL_FIXTURE_VERSION,
    CANONICAL_HEIGHT, CANONICAL_SOURCE_FRAMES, CANONICAL_WIDTH,
};
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult, EngineEvent,
    EngineReportedFailure, EngineTransactionId, ScenarioTransaction, MAX_PENDING_ENGINE_EVENTS,
};
use superi_engine::lifecycle::{
    EngineHealth, EngineLifecycleActionKind, EngineSubsystem, EngineWorkAdmission, EngineWorkKind,
    EngineWorkPermit,
};

#[test]
fn scenario_transactions_publish_once_rollback_completely_and_undo_as_one_unit() {
    assert_eq!(
        EngineTransactionId::new("").unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        EngineTransactionId::new("bad\nidentity")
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        EngineTransactionId::new("x".repeat(129))
            .unwrap_err()
            .category(),
        ErrorCategory::ResourceExhausted
    );
    assert_eq!(
        ScenarioTransaction::new(0, Vec::new())
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        ScenarioTransaction::new(0, vec![ScenarioAction::Undo; 65])
            .unwrap_err()
            .category(),
        ErrorCategory::ResourceExhausted
    );
    assert_eq!(
        ScenarioTransaction::new(0, vec![ScenarioAction::Undo, ScenarioAction::Redo])
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );

    let directory = test_directory("atomic");
    let source = directory.join("input.webm");
    let source_bytes = b"dispatcher transaction fixture";
    fs::write(&source, source_bytes).unwrap();
    let mut dispatcher = EngineCommandDispatcher::scenario_only();

    let invalid = ScenarioTransaction::new(
        0,
        vec![
            import_action(source.clone(), source_bytes),
            ScenarioAction::PlaceClip {
                timeline_start_frame: 1,
            },
        ],
    )
    .unwrap();
    let error = dispatch(
        &mut dispatcher,
        "scenario-invalid",
        EngineCommand::ExecuteScenario(invalid),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(dispatcher.scenario_snapshot().unwrap().revision(), 0);
    assert_eq!(
        dispatcher.scenario_snapshot().unwrap().phase(),
        ScenarioPhase::Empty
    );
    assert!(dispatcher.drain_events().unwrap().is_empty());

    let transaction = ScenarioTransaction::new(
        0,
        vec![
            import_action(source, source_bytes),
            ScenarioAction::PlaceClip {
                timeline_start_frame: 0,
            },
            ScenarioAction::TrimClip {
                source_start_frame: 24,
                source_end_frame: 72,
            },
            ScenarioAction::ApplyGraphEffect {
                effect: GraphEffect::HorizontalMirror,
            },
        ],
    )
    .unwrap();
    let committed = dispatch(
        &mut dispatcher,
        "scenario-commit",
        EngineCommand::ExecuteScenario(transaction),
    )
    .unwrap();
    assert_eq!(committed.command_sequence(), 1);
    let state = scenario_result(committed.result());
    assert_eq!(state.revision(), 1);
    assert_eq!(state.phase(), ScenarioPhase::Effected);
    assert_eq!(state.undo_depth(), 1);
    assert_eq!(state.operation_log().len(), 4);
    assert!(state
        .operation_log()
        .iter()
        .all(|operation| operation.resulting_revision() == 1));

    let events = dispatcher.drain_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].sequence(), 1);
    assert_eq!(events[0].command_sequence(), 1);
    assert_eq!(events[0].transaction_id().as_str(), "scenario-commit");
    assert_eq!(events[0].project_revision(), Some(1));
    match events[0].event() {
        EngineEvent::ScenarioStateChanged(snapshot) => {
            assert_eq!(snapshot.as_ref(), state);
        }
        EngineEvent::LifecycleStateChanged(_) => panic!("expected scenario state event"),
        _ => panic!("unexpected future event kind"),
    }

    let stale = ScenarioTransaction::new(0, vec![ScenarioAction::Undo]).unwrap();
    let error = dispatch(
        &mut dispatcher,
        "scenario-stale",
        EngineCommand::ExecuteScenario(stale),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(dispatcher.scenario_snapshot().unwrap().revision(), 1);
    assert!(dispatcher.drain_events().unwrap().is_empty());

    let undo = ScenarioTransaction::new(1, vec![ScenarioAction::Undo]).unwrap();
    let undone = dispatch(
        &mut dispatcher,
        "scenario-undo",
        EngineCommand::ExecuteScenario(undo),
    )
    .unwrap();
    assert_eq!(undone.command_sequence(), 2);
    let state = scenario_result(undone.result());
    assert_eq!(state.revision(), 2);
    assert_eq!(state.phase(), ScenarioPhase::Empty);
    assert_eq!(state.undo_depth(), 0);
    assert_eq!(state.redo_depth(), 1);

    for offset in 0..(MAX_PENDING_ENGINE_EVENTS - 1) {
        let action = if offset % 2 == 0 {
            ScenarioAction::Redo
        } else {
            ScenarioAction::Undo
        };
        let expected_revision = 2 + u64::try_from(offset).unwrap();
        let transaction = ScenarioTransaction::new(expected_revision, vec![action]).unwrap();
        dispatch(
            &mut dispatcher,
            &format!("fill-event-queue-{offset}"),
            EngineCommand::ExecuteScenario(transaction),
        )
        .unwrap();
    }
    let before_blocked = dispatcher.scenario_snapshot().unwrap();
    assert_eq!(before_blocked.revision(), 65);
    assert_eq!(before_blocked.phase(), ScenarioPhase::Effected);
    let blocked = ScenarioTransaction::new(65, vec![ScenarioAction::Undo]).unwrap();
    let error = dispatch(
        &mut dispatcher,
        "event-queue-capacity",
        EngineCommand::ExecuteScenario(blocked),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(dispatcher.scenario_snapshot().unwrap(), before_blocked);
    let events = dispatcher.drain_events().unwrap();
    assert_eq!(events.len(), MAX_PENDING_ENGINE_EVENTS);
    assert_eq!(events.first().unwrap().sequence(), 2);
    assert_eq!(events.last().unwrap().sequence(), 65);
    assert_eq!(events.last().unwrap().command_sequence(), 65);
    fs::remove_dir_all(directory).unwrap();
}

#[test]
fn lifecycle_commands_keep_playback_rendering_and_export_coherent_through_recovery() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut dispatcher = EngineCommandDispatcher::new().expect("dispatcher owns one lifecycle");

    let mut lifecycle = lifecycle_result(
        dispatch(
            &mut dispatcher,
            "lifecycle-inspect",
            EngineCommand::InspectLifecycle,
        )
        .unwrap()
        .result(),
    )
    .clone();
    assert_eq!(lifecycle.phase(), LifecyclePhase::Starting);

    while lifecycle.phase() == LifecyclePhase::Starting {
        let action = lifecycle
            .pending_action()
            .expect("startup has one exact action");
        assert_eq!(action.kind(), EngineLifecycleActionKind::Initialize);
        lifecycle = lifecycle_result(
            dispatch(
                &mut dispatcher,
                &format!("initialize-{}", action.subsystem().code()),
                EngineCommand::CompleteLifecycleAction(action),
            )
            .unwrap()
            .result(),
        )
        .clone();
    }
    assert_eq!(lifecycle.phase(), LifecyclePhase::Running);
    assert_eq!(lifecycle.health(), EngineHealth::Healthy);

    let playback_permit = granted_permit(admit(
        &mut dispatcher,
        "admit-playback",
        EngineWorkKind::Playback,
    ));
    granted_permit(admit(
        &mut dispatcher,
        "admit-rendering",
        EngineWorkKind::Rendering,
    ));
    granted_permit(admit(
        &mut dispatcher,
        "admit-export",
        EngineWorkKind::Export,
    ));

    let playback_failure = EngineReportedFailure::new(
        ErrorCategory::Unavailable,
        Recoverability::Degraded,
        "playback device temporarily unavailable",
    )
    .unwrap();
    let playback_degraded = lifecycle_result(
        dispatch(
            &mut dispatcher,
            "degrade-playback",
            EngineCommand::ReportRuntimeFailure {
                subsystem: EngineSubsystem::Playback,
                failure: playback_failure,
            },
        )
        .unwrap()
        .result(),
    )
    .clone();
    assert_eq!(playback_degraded.health(), EngineHealth::Degraded);
    denied_by(
        admit(&mut dispatcher, "deny-playback", EngineWorkKind::Playback),
        EngineSubsystem::Playback,
    );
    granted_permit(admit(
        &mut dispatcher,
        "render-during-playback-degrade",
        EngineWorkKind::Rendering,
    ));
    granted_permit(admit(
        &mut dispatcher,
        "export-during-playback-degrade",
        EngineWorkKind::Export,
    ));
    assert!(!permit_current(
        &mut dispatcher,
        "stale-playback-permit",
        playback_permit,
    ));

    let recovering = lifecycle_result(
        dispatch(
            &mut dispatcher,
            "recover-playback",
            EngineCommand::BeginRecovery(EngineSubsystem::Playback),
        )
        .unwrap()
        .result(),
    )
    .clone();
    let recovery_action = recovering.pending_action().unwrap();
    assert_eq!(recovery_action.kind(), EngineLifecycleActionKind::Recover);
    let recovered = lifecycle_result(
        dispatch(
            &mut dispatcher,
            "complete-playback-recovery",
            EngineCommand::CompleteLifecycleAction(recovery_action),
        )
        .unwrap()
        .result(),
    )
    .clone();
    assert_eq!(recovered.health(), EngineHealth::Healthy);
    granted_permit(admit(
        &mut dispatcher,
        "playback-recovered",
        EngineWorkKind::Playback,
    ));

    let render_failure = EngineReportedFailure::new(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "render device is rebuilding",
    )
    .unwrap();
    dispatch(
        &mut dispatcher,
        "degrade-rendering",
        EngineCommand::ReportRuntimeFailure {
            subsystem: EngineSubsystem::Rendering,
            failure: render_failure,
        },
    )
    .unwrap();
    granted_permit(admit(
        &mut dispatcher,
        "playback-during-render-degrade",
        EngineWorkKind::Playback,
    ));
    denied_by(
        admit(&mut dispatcher, "deny-rendering", EngineWorkKind::Rendering),
        EngineSubsystem::Rendering,
    );
    denied_by(
        admit(&mut dispatcher, "deny-export", EngineWorkKind::Export),
        EngineSubsystem::Rendering,
    );

    let recovering = lifecycle_result(
        dispatch(
            &mut dispatcher,
            "recover-rendering",
            EngineCommand::BeginRecovery(EngineSubsystem::Rendering),
        )
        .unwrap()
        .result(),
    )
    .clone();
    let action = recovering.pending_action().unwrap();
    dispatch(
        &mut dispatcher,
        "complete-rendering-recovery",
        EngineCommand::CompleteLifecycleAction(action),
    )
    .unwrap();
    granted_permit(admit(
        &mut dispatcher,
        "export-recovered",
        EngineWorkKind::Export,
    ));

    let events = dispatcher.drain_events().unwrap();
    assert_eq!(events.len(), 12);
    assert_eq!(events[0].command_sequence(), 2);
    assert!(events
        .windows(2)
        .all(|pair| pair[0].command_sequence() < pair[1].command_sequence()));
    assert_eq!(
        events
            .iter()
            .map(|event| event.sequence())
            .collect::<Vec<_>>(),
        (1_u64..=12).collect::<Vec<_>>()
    );
    assert!(events[..6]
        .iter()
        .all(|event| matches!(event.event(), EngineEvent::LifecycleStateChanged(_))));
    assert!(events[6..]
        .iter()
        .all(|event| matches!(event.event(), EngineEvent::RecoveryStateChanged(_))));
}

#[test]
fn sleep_and_wake_commands_publish_coherent_lifecycle_events_and_invalidate_work() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut dispatcher = EngineCommandDispatcher::new().expect("dispatcher owns one lifecycle");

    let mut lifecycle = lifecycle_result(
        dispatch(
            &mut dispatcher,
            "inspect-power-startup",
            EngineCommand::InspectLifecycle,
        )
        .unwrap()
        .result(),
    )
    .clone();
    while lifecycle.phase() == LifecyclePhase::Starting {
        let action = lifecycle.pending_action().unwrap();
        lifecycle = lifecycle_result(
            dispatch(
                &mut dispatcher,
                &format!("power-initialize-{}", action.subsystem().code()),
                EngineCommand::CompleteLifecycleAction(action),
            )
            .unwrap()
            .result(),
        )
        .clone();
    }
    assert_eq!(lifecycle.phase(), LifecyclePhase::Running);
    dispatcher.drain_events().unwrap();

    let permit = granted_permit(admit(
        &mut dispatcher,
        "power-admit-export",
        EngineWorkKind::Export,
    ));
    lifecycle = lifecycle_result(
        dispatch(
            &mut dispatcher,
            "begin-system-sleep",
            EngineCommand::BeginSleep,
        )
        .unwrap()
        .result(),
    )
    .clone();
    assert_eq!(lifecycle.phase(), LifecyclePhase::PreparingSleep);
    assert_eq!(
        lifecycle.pending_action().unwrap().kind(),
        EngineLifecycleActionKind::PrepareSleep
    );
    assert!(!permit_current(
        &mut dispatcher,
        "power-stale-export-permit",
        permit,
    ));
    while lifecycle.phase() == LifecyclePhase::PreparingSleep {
        let action = lifecycle.pending_action().unwrap();
        lifecycle = lifecycle_result(
            dispatch(
                &mut dispatcher,
                &format!("prepare-sleep-{}", action.subsystem().code()),
                EngineCommand::CompleteLifecycleAction(action),
            )
            .unwrap()
            .result(),
        )
        .clone();
    }
    assert_eq!(lifecycle.phase(), LifecyclePhase::Sleeping);

    lifecycle = lifecycle_result(
        dispatch(
            &mut dispatcher,
            "begin-system-wake",
            EngineCommand::BeginWake,
        )
        .unwrap()
        .result(),
    )
    .clone();
    assert_eq!(lifecycle.phase(), LifecyclePhase::Waking);
    assert_eq!(
        lifecycle.pending_action().unwrap().kind(),
        EngineLifecycleActionKind::Wake
    );
    while lifecycle.phase() == LifecyclePhase::Waking {
        let action = lifecycle.pending_action().unwrap();
        lifecycle = lifecycle_result(
            dispatch(
                &mut dispatcher,
                &format!("wake-{}", action.subsystem().code()),
                EngineCommand::CompleteLifecycleAction(action),
            )
            .unwrap()
            .result(),
        )
        .clone();
    }
    assert_eq!(lifecycle.phase(), LifecyclePhase::Running);
    granted_permit(admit(
        &mut dispatcher,
        "power-readmit-export",
        EngineWorkKind::Export,
    ));

    let events = dispatcher.drain_events().unwrap();
    assert_eq!(events.len(), 12);
    assert!(events
        .iter()
        .all(|event| matches!(event.event(), EngineEvent::LifecycleStateChanged(_))));
    assert!(events
        .windows(2)
        .all(|pair| pair[0].sequence() < pair[1].sequence()));
}

#[test]
fn lifecycle_attached_dispatcher_rejects_commands_outside_engine_control() {
    let construction_error = match EngineCommandDispatcher::new() {
        Ok(_) => panic!("lifecycle dispatcher must require engine control at construction"),
        Err(error) => error,
    };
    assert_eq!(construction_error.category(), ErrorCategory::Conflict);

    let mut dispatcher = {
        let _domain = ExecutionDomain::EngineControl
            .enter_current()
            .expect("test temporarily owns engine control");
        EngineCommandDispatcher::new().expect("owner constructs dispatcher")
    };

    let error = dispatch(
        &mut dispatcher,
        "off-domain-inspect",
        EngineCommand::InspectLifecycle,
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);

    assert_eq!(
        dispatcher.scenario_snapshot().unwrap_err().category(),
        ErrorCategory::Conflict
    );
    assert_eq!(
        dispatcher.drain_events().unwrap_err().category(),
        ErrorCategory::Conflict
    );
}

#[test]
fn lifecycle_action_failure_restart_shutdown_and_inspection_all_route_through_commands() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut dispatcher = EngineCommandDispatcher::new().expect("dispatcher owns one lifecycle");

    let initial = lifecycle_result(
        dispatch(
            &mut dispatcher,
            "inspect-initial-lifecycle",
            EngineCommand::InspectLifecycle,
        )
        .unwrap()
        .result(),
    )
    .clone();
    let shared_action = initial.pending_action().unwrap();
    let after_shared = lifecycle_result(
        dispatch(
            &mut dispatcher,
            "complete-shared-state",
            EngineCommand::CompleteLifecycleAction(shared_action),
        )
        .unwrap()
        .result(),
    )
    .clone();
    let mut after_dependencies = after_shared;
    for subsystem in [EngineSubsystem::Projects, EngineSubsystem::Devices] {
        let action = after_dependencies.pending_action().unwrap();
        assert_eq!(action.subsystem(), subsystem);
        after_dependencies = lifecycle_result(
            dispatch(
                &mut dispatcher,
                &format!("complete-{}", subsystem.code()),
                EngineCommand::CompleteLifecycleAction(action),
            )
            .unwrap()
            .result(),
        )
        .clone();
    }
    let playback_action = after_dependencies.pending_action().unwrap();
    let failure = EngineReportedFailure::new(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "playback initialization failed",
    )
    .unwrap()
    .with_context(
        ErrorContext::new("superi-engine.dispatcher-test", "open_playback")
            .with_field("device", "default"),
    )
    .unwrap();
    let failed = lifecycle_result(
        dispatch(
            &mut dispatcher,
            "fail-playback-initialization",
            EngineCommand::FailLifecycleAction {
                action: playback_action,
                failure,
            },
        )
        .unwrap()
        .result(),
    )
    .clone();
    assert_eq!(failed.phase(), LifecyclePhase::Failed);
    assert_eq!(
        failed
            .subsystem(EngineSubsystem::Playback)
            .unwrap()
            .failure()
            .unwrap()
            .contexts()[0]
            .field("device"),
        Some("default")
    );
    let mut rolled_back = failed;
    let mut rollback_order = Vec::new();
    while let Some(rollback) = rolled_back.pending_action() {
        rollback_order.push(rollback.subsystem());
        rolled_back = lifecycle_result(
            dispatch(
                &mut dispatcher,
                &format!("complete-startup-rollback-{}", rollback.subsystem().code()),
                EngineCommand::CompleteLifecycleAction(rollback),
            )
            .unwrap()
            .result(),
        )
        .clone();
    }
    assert_eq!(
        rollback_order,
        [
            EngineSubsystem::Devices,
            EngineSubsystem::Projects,
            EngineSubsystem::SharedState,
        ]
    );
    assert!(rolled_back.pending_action().is_none());

    let mut lifecycle = lifecycle_result(
        dispatch(
            &mut dispatcher,
            "restart-failed-engine",
            EngineCommand::BeginRestart,
        )
        .unwrap()
        .result(),
    )
    .clone();
    while lifecycle.phase() == LifecyclePhase::Starting {
        let action = lifecycle.pending_action().unwrap();
        lifecycle = lifecycle_result(
            dispatch(
                &mut dispatcher,
                &format!("restart-initialize-{}", action.subsystem().code()),
                EngineCommand::CompleteLifecycleAction(action),
            )
            .unwrap()
            .result(),
        )
        .clone();
    }
    assert_eq!(lifecycle.phase(), LifecyclePhase::Running);
    assert_eq!(lifecycle.lifetime(), 2);

    lifecycle = lifecycle_result(
        dispatch(
            &mut dispatcher,
            "shutdown-restarted-engine",
            EngineCommand::BeginShutdown,
        )
        .unwrap()
        .result(),
    )
    .clone();
    while let Some(action) = lifecycle.pending_action() {
        lifecycle = lifecycle_result(
            dispatch(
                &mut dispatcher,
                &format!("shutdown-{}", action.subsystem().code()),
                EngineCommand::CompleteLifecycleAction(action),
            )
            .unwrap()
            .result(),
        )
        .clone();
    }
    assert_eq!(lifecycle.phase(), LifecyclePhase::Stopped);

    let scenario = scenario_result(
        dispatch(
            &mut dispatcher,
            "inspect-scenario",
            EngineCommand::InspectScenario,
        )
        .unwrap()
        .result(),
    )
    .clone();
    assert_eq!(scenario.revision(), 0);
    assert_eq!(scenario.phase(), ScenarioPhase::Empty);

    let events = dispatcher.drain_events().unwrap();
    assert_eq!(
        events
            .iter()
            .map(|event| event.sequence())
            .collect::<Vec<_>>(),
        (1_u64..=events.len() as u64).collect::<Vec<_>>()
    );
}

fn dispatch(
    dispatcher: &mut EngineCommandDispatcher,
    transaction_id: &str,
    command: EngineCommand,
) -> superi_core::error::Result<superi_engine::dispatcher::EngineCommandOutcome> {
    dispatcher.dispatch(EngineCommandRequest::new(
        EngineTransactionId::new(transaction_id).unwrap(),
        command,
    ))
}

fn scenario_result(result: &EngineCommandResult) -> &superi_engine::command::ScenarioSnapshot {
    match result {
        EngineCommandResult::Scenario(snapshot) => snapshot,
        _ => panic!("expected scenario result"),
    }
}

fn lifecycle_result(
    result: &EngineCommandResult,
) -> &superi_engine::lifecycle::EngineLifecycleSnapshot {
    match result {
        EngineCommandResult::Lifecycle(snapshot) => snapshot,
        _ => panic!("expected lifecycle result"),
    }
}

fn admit(
    dispatcher: &mut EngineCommandDispatcher,
    transaction_id: &str,
    work: EngineWorkKind,
) -> EngineWorkAdmission {
    let outcome = dispatch(dispatcher, transaction_id, EngineCommand::AdmitWork(work)).unwrap();
    match outcome.result() {
        EngineCommandResult::WorkAdmission(admission) => admission.clone(),
        _ => panic!("expected work admission result"),
    }
}

fn granted_permit(admission: EngineWorkAdmission) -> EngineWorkPermit {
    admission.permit().expect("work should be admitted")
}

fn denied_by(admission: EngineWorkAdmission, subsystem: EngineSubsystem) {
    assert_eq!(
        admission
            .denial()
            .expect("work should be denied")
            .blocking_subsystem(),
        Some(subsystem)
    );
}

fn permit_current(
    dispatcher: &mut EngineCommandDispatcher,
    transaction_id: &str,
    permit: EngineWorkPermit,
) -> bool {
    let outcome = dispatch(
        dispatcher,
        transaction_id,
        EngineCommand::ValidateWorkPermit(permit),
    )
    .unwrap();
    match outcome.result() {
        EngineCommandResult::WorkPermitCurrent(current) => *current,
        _ => panic!("expected permit validation result"),
    }
}

fn import_action(path: PathBuf, payload: &[u8]) -> ScenarioAction {
    ScenarioAction::ImportClip {
        path,
        fixture_id: CANONICAL_FIXTURE_ID.to_owned(),
        fixture_version: CANONICAL_FIXTURE_VERSION,
        manifest_sha256: "1d2b28b5f44c7f86dce50d67b718b0fad967d267d9016961e3d71bb9dab94419"
            .to_owned(),
        payload_sha256: format!("{:x}", Sha256::digest(payload)),
        frame_rate: FrameRate::FPS_24,
        frame_count: CANONICAL_SOURCE_FRAMES,
        width: CANONICAL_WIDTH,
        height: CANONICAL_HEIGHT,
    }
}

fn test_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "superi-p2-w06-c007-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
