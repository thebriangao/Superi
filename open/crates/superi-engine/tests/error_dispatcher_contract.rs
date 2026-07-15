use superi_concurrency::lifecycle::LifecyclePhase;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, Recoverability};
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult, EngineEvent,
    EngineFailureOperation, EngineRecoveryState, EngineReportedFailure, EngineTransactionId,
};
use superi_engine::error::{EngineFailureDisposition, EngineRecoveryRequest};
use superi_engine::lifecycle::{EngineHealth, EngineSubsystem, EngineWorkKind};

#[test]
fn classified_failure_and_recovery_use_one_revisioned_dispatcher_state() {
    assert_eq!(
        EngineFailureOperation::new("   ").unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        EngineFailureOperation::new("x".repeat(129))
            .unwrap_err()
            .category(),
        ErrorCategory::ResourceExhausted
    );
    assert_eq!(
        EngineFailureOperation::new("  present_frame  ")
            .unwrap()
            .as_str(),
        "present_frame"
    );

    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut dispatcher = running_dispatcher();

    let reported = dispatch(
        &mut dispatcher,
        "report-playback",
        EngineCommand::ReportClassifiedFailure {
            subsystem: EngineSubsystem::Playback,
            operation: EngineFailureOperation::new("present_frame").unwrap(),
            failure: EngineReportedFailure::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "playback device vanished",
            )
            .unwrap(),
        },
    )
    .unwrap();
    let failed = recovery_state(reported.result());
    assert_eq!(failed.revision(), 1);
    assert_eq!(failed.lifecycle().health(), EngineHealth::Degraded);
    assert!(failed
        .lifecycle()
        .admit(EngineWorkKind::Playback)
        .permit()
        .is_none());
    assert!(failed
        .lifecycle()
        .admit(EngineWorkKind::Rendering)
        .permit()
        .is_some());
    let record = &failed.active_failures()[0];
    assert_eq!(record.sequence().get(), 1);
    assert_eq!(record.operation(), "present_frame");
    assert_eq!(
        record.disposition(),
        EngineFailureDisposition::RetryAvailable
    );
    assert_eq!(record.recovery_attempts(), 0);
    assert_eq!(failed.diagnostic_history().len(), 1);
    assert!(failed.pending_recovery().is_none());

    let events = dispatcher.drain_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].recovery_state_revision(), Some(1));
    assert_eq!(
        events[0].lifecycle_state_revision(),
        Some(failed.lifecycle().state_revision())
    );
    match events[0].event() {
        EngineEvent::RecoveryStateChanged(state) => assert_eq!(state.as_ref(), failed),
        event => panic!("unexpected event: {event:?}"),
    }

    let wrong_request = dispatch(
        &mut dispatcher,
        "wrong-recovery-request",
        EngineCommand::BeginClassifiedRecovery {
            subsystem: EngineSubsystem::Playback,
            sequence: record.sequence(),
            request: EngineRecoveryRequest::RestoreSubsystem,
        },
    )
    .unwrap_err();
    assert_eq!(wrong_request.category(), ErrorCategory::InvalidInput);
    assert!(dispatcher.drain_events().unwrap().is_empty());

    let started = dispatch(
        &mut dispatcher,
        "begin-playback-recovery",
        EngineCommand::BeginClassifiedRecovery {
            subsystem: EngineSubsystem::Playback,
            sequence: record.sequence(),
            request: EngineRecoveryRequest::RetryOperation,
        },
    )
    .unwrap();
    let recovering = recovery_state(started.result());
    assert_eq!(recovering.revision(), 2);
    assert_eq!(recovering.active_failures()[0].recovery_attempts(), 1);
    let recovery = recovering
        .pending_recovery()
        .expect("exact recovery token is published");
    assert_eq!(recovery.sequence(), record.sequence());
    assert_eq!(recovery.request(), EngineRecoveryRequest::RetryOperation);
    dispatcher.drain_events().unwrap();

    let recovered = dispatch(
        &mut dispatcher,
        "complete-playback-recovery",
        EngineCommand::CompleteRecovery(recovery),
    )
    .unwrap();
    let state = recovery_state(recovered.result());
    assert_eq!(state.revision(), 3);
    assert_eq!(state.lifecycle().health(), EngineHealth::Healthy);
    assert!(state.active_failures().is_empty());
    assert!(state.pending_recovery().is_none());
    assert_eq!(
        state
            .diagnostic_history()
            .iter()
            .map(|event| event.name())
            .collect::<Vec<_>>(),
        vec!["engine.subsystem.failed", "engine.subsystem.recovered"]
    );
    dispatcher.drain_events().unwrap();

    let inspected = dispatch(
        &mut dispatcher,
        "inspect-recovery-state",
        EngineCommand::InspectRecoveryState,
    )
    .unwrap();
    assert_eq!(recovery_state(inspected.result()), state);
    assert!(dispatcher.drain_events().unwrap().is_empty());
}

#[test]
fn failed_recovery_reclassifies_and_legacy_commands_cannot_bypass_the_coordinator() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns engine control");
    let mut dispatcher = running_dispatcher();

    let legacy_failure = EngineReportedFailure::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::Degraded,
        "render device is rebuilding",
    )
    .unwrap();
    let degraded = dispatch(
        &mut dispatcher,
        "legacy-report-rendering",
        EngineCommand::ReportRuntimeFailure {
            subsystem: EngineSubsystem::Rendering,
            failure: legacy_failure,
        },
    )
    .unwrap();
    assert!(matches!(
        degraded.result(),
        EngineCommandResult::Lifecycle(_)
    ));
    let event = dispatcher.drain_events().unwrap().pop().unwrap();
    assert!(matches!(
        event.event(),
        EngineEvent::RecoveryStateChanged(_)
    ));

    let state = recovery_state(
        dispatch(
            &mut dispatcher,
            "inspect-legacy-failure",
            EngineCommand::InspectRecoveryState,
        )
        .unwrap()
        .result(),
    )
    .clone();
    let first_sequence = state.active_failures()[0].sequence();
    assert_eq!(state.active_failures()[0].operation(), "runtime_failure");

    let legacy_start = dispatch(
        &mut dispatcher,
        "legacy-begin-rendering-recovery",
        EngineCommand::BeginRecovery(EngineSubsystem::Rendering),
    )
    .unwrap();
    let lifecycle = match legacy_start.result() {
        EngineCommandResult::Lifecycle(snapshot) => snapshot,
        result => panic!("unexpected legacy result: {result:?}"),
    };
    let lifecycle_action = lifecycle.pending_action().unwrap();
    let event = dispatcher.drain_events().unwrap().pop().unwrap();
    let first_recovery = match event.event() {
        EngineEvent::RecoveryStateChanged(state) => state.pending_recovery().unwrap(),
        other => panic!("unexpected event: {other:?}"),
    };

    let failed = dispatch(
        &mut dispatcher,
        "legacy-fail-rendering-recovery",
        EngineCommand::FailLifecycleAction {
            action: lifecycle_action,
            failure: EngineReportedFailure::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "render device is still unavailable",
            )
            .unwrap(),
        },
    )
    .unwrap();
    assert!(matches!(failed.result(), EngineCommandResult::Lifecycle(_)));
    let reclassified = match dispatcher.drain_events().unwrap().pop().unwrap().event() {
        EngineEvent::RecoveryStateChanged(state) => state.clone(),
        other => panic!("unexpected event: {other:?}"),
    };
    assert_eq!(reclassified.revision(), 3);
    assert_eq!(reclassified.active_failures()[0].sequence().get(), 2);
    assert_eq!(
        reclassified.active_failures()[0].disposition(),
        EngineFailureDisposition::RetryAvailable
    );
    assert_eq!(reclassified.active_failures()[0].recovery_attempts(), 1);
    assert!(reclassified.pending_recovery().is_none());

    let stale = dispatch(
        &mut dispatcher,
        "stale-recovery-token",
        EngineCommand::CompleteRecovery(first_recovery),
    )
    .unwrap_err();
    assert_eq!(stale.category(), ErrorCategory::Conflict);
    assert!(dispatcher.drain_events().unwrap().is_empty());

    let second = dispatch(
        &mut dispatcher,
        "retry-reclassified-rendering",
        EngineCommand::BeginClassifiedRecovery {
            subsystem: EngineSubsystem::Rendering,
            sequence: reclassified.active_failures()[0].sequence(),
            request: EngineRecoveryRequest::RetryOperation,
        },
    )
    .unwrap();
    let second_recovery = recovery_state(second.result()).pending_recovery().unwrap();
    assert_ne!(second_recovery.sequence(), first_sequence);
    dispatcher.drain_events().unwrap();
    dispatch(
        &mut dispatcher,
        "complete-reclassified-rendering",
        EngineCommand::CompleteRecovery(second_recovery),
    )
    .unwrap();
    let final_state = match dispatcher.drain_events().unwrap().pop().unwrap().event() {
        EngineEvent::RecoveryStateChanged(state) => state.clone(),
        other => panic!("unexpected event: {other:?}"),
    };
    assert_eq!(final_state.lifecycle().health(), EngineHealth::Healthy);
    assert!(final_state.active_failures().is_empty());
}

fn running_dispatcher() -> EngineCommandDispatcher {
    let mut dispatcher = EngineCommandDispatcher::new().expect("dispatcher is constructed");
    loop {
        let lifecycle = match dispatch(
            &mut dispatcher,
            "inspect-startup",
            EngineCommand::InspectLifecycle,
        )
        .unwrap()
        .result()
        {
            EngineCommandResult::Lifecycle(snapshot) => snapshot.clone(),
            result => panic!("unexpected lifecycle result: {result:?}"),
        };
        if lifecycle.phase() != LifecyclePhase::Starting {
            break;
        }
        dispatch(
            &mut dispatcher,
            &format!(
                "initialize-{}",
                lifecycle.pending_action().unwrap().subsystem().code()
            ),
            EngineCommand::CompleteLifecycleAction(lifecycle.pending_action().unwrap()),
        )
        .unwrap();
    }
    dispatcher.drain_events().unwrap();
    dispatcher
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

fn recovery_state(result: &EngineCommandResult) -> &EngineRecoveryState {
    match result {
        EngineCommandResult::Recovery(state) => state,
        result => panic!("unexpected recovery result: {result:?}"),
    }
}
