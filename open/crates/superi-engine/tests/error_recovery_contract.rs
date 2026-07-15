use std::io;

use superi_concurrency::lifecycle::LifecyclePhase;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::diagnostics::{DiagnosticSeverity, TraceValue};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability};
use superi_engine::error::{
    EngineErrorCoordinator, EngineFailureDisposition, EngineFailureRecord, EngineFailureReport,
    EngineRecoveryReport, EngineRecoveryRequest, EngineRecoveryStart,
};
use superi_engine::lifecycle::{
    EngineLifecycle, EngineLifecycleAction, EngineLifecycleActionKind, EngineLifecycleSnapshot,
    EngineSubsystem, EngineWorkKind,
};

fn current_action(engine: &EngineLifecycle) -> EngineLifecycleAction {
    engine
        .snapshot()
        .expect("engine-control owner can inspect lifecycle state")
        .pending_action()
        .expect("the lifecycle has one pending action")
}

fn running_engine() -> EngineLifecycle {
    let mut engine = EngineLifecycle::new().expect("canonical lifecycle plan is valid");
    while engine
        .snapshot()
        .expect("startup snapshot is available")
        .phase()
        == LifecyclePhase::Starting
    {
        let action = current_action(&engine);
        assert_eq!(action.kind(), EngineLifecycleActionKind::Initialize);
        engine
            .complete_action(action)
            .expect("the exact initialization action completes");
    }
    engine
}

fn assert_work(snapshot: &EngineLifecycleSnapshot, work: EngineWorkKind, granted: bool) {
    assert_eq!(
        snapshot.admit(work).permit().is_some(),
        granted,
        "unexpected admission for {}",
        work.code()
    );
}

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn retryable_failure_retains_evidence_and_recovers_without_blocking_unrelated_work() {
    assert_send_sync::<EngineFailureRecord>();
    assert_send_sync::<EngineFailureReport>();
    assert_send_sync::<EngineRecoveryStart>();
    assert_send_sync::<EngineRecoveryReport>();

    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns the engine-control domain");
    let mut engine = running_engine();
    let mut errors = EngineErrorCoordinator::new().expect("default history is valid");

    let failure = Error::with_source(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "playback output could not present a frame",
        io::Error::other("device vanished"),
    )
    .with_context(
        ErrorContext::new("superi-engine.playback", "present_frame")
            .with_field("device", "studio-monitor"),
    );
    let report = errors
        .report_failure(
            &mut engine,
            EngineSubsystem::Playback,
            "present_frame",
            failure,
        )
        .expect("ready playback can propagate a runtime failure");

    let record = report.failure();
    assert_eq!(record.sequence().get(), 1);
    assert_eq!(record.subsystem(), EngineSubsystem::Playback);
    assert_eq!(record.operation(), "present_frame");
    assert_eq!(record.recovery_attempts(), 0);
    assert_eq!(
        record.disposition(),
        EngineFailureDisposition::RetryAvailable
    );
    assert_eq!(record.disposition().code(), "retry_available");
    assert_eq!(record.diagnostic().name(), "engine.subsystem.failed");
    assert_eq!(
        record
            .diagnostic()
            .field("subsystem")
            .expect("subsystem is structured")
            .value(),
        &TraceValue::Text("playback".to_owned())
    );
    let diagnostic = record
        .diagnostic()
        .failure()
        .expect("raw failure evidence is retained");
    assert_eq!(diagnostic.source_summaries(), &["device vanished"]);
    assert_eq!(
        diagnostic.contexts()[0].field("device"),
        Some("studio-monitor")
    );
    assert_eq!(
        diagnostic.contexts()[1].field("failure_sequence"),
        Some("1")
    );
    let user_safe = record
        .diagnostic()
        .user_safe_error()
        .expect("a separate safe projection is available");
    assert_eq!(user_safe.code(), "error.unavailable.retryable");
    assert!(!user_safe.to_string().contains("studio-monitor"));
    assert!(!user_safe.to_string().contains("device vanished"));

    assert_work(report.lifecycle(), EngineWorkKind::Playback, false);
    assert_work(report.lifecycle(), EngineWorkKind::Rendering, true);
    assert_work(report.lifecycle(), EngineWorkKind::Export, true);
    assert_eq!(
        errors
            .active_failure(EngineSubsystem::Playback)
            .expect("owner can inspect active failures")
            .expect("playback failure is active")
            .sequence(),
        record.sequence()
    );

    let before_wrong_request = engine
        .snapshot()
        .expect("lifecycle snapshot is available")
        .state_revision();
    let wrong_request = errors
        .begin_recovery(
            &mut engine,
            EngineSubsystem::Playback,
            record.sequence(),
            EngineRecoveryRequest::RestoreSubsystem,
        )
        .expect_err("retryable failures reject degraded recovery intent");
    assert_eq!(wrong_request.category(), ErrorCategory::InvalidInput);
    assert_eq!(
        wrong_request.recoverability(),
        Recoverability::UserCorrectable
    );
    assert_eq!(
        engine
            .snapshot()
            .expect("rejected intent leaves lifecycle unchanged")
            .state_revision(),
        before_wrong_request
    );

    let start = errors
        .begin_recovery(
            &mut engine,
            EngineSubsystem::Playback,
            record.sequence(),
            EngineRecoveryRequest::RetryOperation,
        )
        .expect("retry intent emits one exact lifecycle action");
    assert_eq!(start.action().sequence(), record.sequence());
    assert_eq!(start.action().subsystem(), EngineSubsystem::Playback);
    assert_eq!(
        start.action().request(),
        EngineRecoveryRequest::RetryOperation
    );
    assert_eq!(start.action().attempt(), 1);
    assert_eq!(
        start
            .lifecycle()
            .pending_action()
            .expect("recovery action is published")
            .kind(),
        EngineLifecycleActionKind::Recover
    );
    assert_work(start.lifecycle(), EngineWorkKind::Playback, false);
    assert_work(start.lifecycle(), EngineWorkKind::Rendering, true);
    assert_work(start.lifecycle(), EngineWorkKind::Export, true);

    let recovered = errors
        .complete_recovery(&mut engine, start.action())
        .expect("the exact retry action completes recovery");
    assert_eq!(recovered.sequence(), record.sequence());
    assert_eq!(recovered.request(), EngineRecoveryRequest::RetryOperation);
    assert_eq!(recovered.attempt(), 1);
    assert_eq!(recovered.diagnostic().name(), "engine.subsystem.recovered");
    assert_work(recovered.lifecycle(), EngineWorkKind::Playback, true);
    assert_work(recovered.lifecycle(), EngineWorkKind::Rendering, true);
    assert_work(recovered.lifecycle(), EngineWorkKind::Export, true);
    assert!(errors
        .active_failure(EngineSubsystem::Playback)
        .expect("owner can inspect active failures")
        .is_none());
    assert_eq!(
        errors
            .diagnostic_history()
            .expect("owner can inspect bounded diagnostics")
            .iter()
            .map(|event| event.name())
            .collect::<Vec<_>>(),
        vec!["engine.subsystem.failed", "engine.subsystem.recovered"]
    );
}

#[test]
fn degraded_and_user_correctable_failures_require_distinct_recovery_intent() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns the engine-control domain");
    let mut engine = running_engine();
    let mut errors = EngineErrorCoordinator::new().expect("default history is valid");

    let degraded = errors
        .report_failure(
            &mut engine,
            EngineSubsystem::Rendering,
            "submit_frame",
            Error::new(
                ErrorCategory::ResourceExhausted,
                Recoverability::Degraded,
                "rendering is continuing without the preferred device",
            ),
        )
        .expect("rendering can enter a degraded state");
    assert_eq!(
        degraded.failure().disposition(),
        EngineFailureDisposition::ContinueDegraded
    );
    assert_work(degraded.lifecycle(), EngineWorkKind::Playback, true);
    assert_work(degraded.lifecycle(), EngineWorkKind::Rendering, false);
    assert_work(degraded.lifecycle(), EngineWorkKind::Export, false);

    let first_start = errors
        .begin_recovery(
            &mut engine,
            EngineSubsystem::Rendering,
            degraded.failure().sequence(),
            EngineRecoveryRequest::RestoreSubsystem,
        )
        .expect("degraded rendering accepts restore intent");
    let failed_recovery = errors
        .fail_recovery(
            &mut engine,
            first_start.action(),
            Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "preferred render device is still unavailable",
            ),
        )
        .expect("failed recovery is reclassified through the lifecycle");
    assert_eq!(failed_recovery.failure().sequence().get(), 2);
    assert_eq!(failed_recovery.failure().recovery_attempts(), 1);
    assert_eq!(
        failed_recovery.failure().disposition(),
        EngineFailureDisposition::RetryAvailable
    );
    assert_eq!(
        failed_recovery
            .failure()
            .diagnostic()
            .failure()
            .expect("failed recovery retains context")
            .contexts()[0]
            .field("previous_failure_sequence"),
        Some("1")
    );

    let before_stale_completion = engine
        .snapshot()
        .expect("lifecycle snapshot is available")
        .state_revision();
    let stale = errors
        .complete_recovery(&mut engine, first_start.action())
        .expect_err("an earlier recovery action cannot complete newer state");
    assert_eq!(stale.category(), ErrorCategory::Conflict);
    assert_eq!(
        engine
            .snapshot()
            .expect("stale action leaves lifecycle unchanged")
            .state_revision(),
        before_stale_completion
    );
    let stale_failure = errors
        .fail_recovery(
            &mut engine,
            first_start.action(),
            Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "late recovery result",
            ),
        )
        .expect_err("a stale failed action is also rejected before mutation");
    assert_eq!(stale_failure.contexts()[0].operation(), "fail_recovery");

    let retry_start = errors
        .begin_recovery(
            &mut engine,
            EngineSubsystem::Rendering,
            failed_recovery.failure().sequence(),
            EngineRecoveryRequest::RetryOperation,
        )
        .expect("reclassified failure accepts retry intent");
    assert_eq!(retry_start.action().attempt(), 2);
    errors
        .complete_recovery(&mut engine, retry_start.action())
        .expect("second rendering recovery succeeds");

    let correctable = errors
        .report_failure(
            &mut engine,
            EngineSubsystem::Export,
            "open_destination",
            Error::new(
                ErrorCategory::PermissionDenied,
                Recoverability::UserCorrectable,
                "export destination is not writable",
            ),
        )
        .expect("export can await user correction");
    assert_eq!(
        correctable.failure().disposition(),
        EngineFailureDisposition::UserCorrectionRequired
    );
    assert_work(correctable.lifecycle(), EngineWorkKind::Playback, true);
    assert_work(correctable.lifecycle(), EngineWorkKind::Rendering, true);
    assert_work(correctable.lifecycle(), EngineWorkKind::Export, false);
    errors
        .begin_recovery(
            &mut engine,
            EngineSubsystem::Export,
            correctable.failure().sequence(),
            EngineRecoveryRequest::RetryOperation,
        )
        .expect_err("user-correctable failures reject an unconfirmed retry");
    let correction = errors
        .begin_recovery(
            &mut engine,
            EngineSubsystem::Export,
            correctable.failure().sequence(),
            EngineRecoveryRequest::ApplyUserCorrection,
        )
        .expect("confirmed correction can revalidate export");
    let recovered = errors
        .complete_recovery(&mut engine, correction.action())
        .expect("corrected export recovers");
    assert_work(recovered.lifecycle(), EngineWorkKind::Playback, true);
    assert_work(recovered.lifecycle(), EngineWorkKind::Rendering, true);
    assert_work(recovered.lifecycle(), EngineWorkKind::Export, true);
}

#[test]
fn terminal_failure_requires_restart_and_denies_every_workflow() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns the engine-control domain");
    let mut engine = running_engine();
    let mut errors = EngineErrorCoordinator::new().expect("default history is valid");

    let terminal = errors
        .report_failure(
            &mut engine,
            EngineSubsystem::SharedState,
            "publish_project_state",
            Error::new(
                ErrorCategory::CorruptData,
                Recoverability::Terminal,
                "authoritative project state failed validation",
            ),
        )
        .expect("terminal shared-state failure is retained");
    assert_eq!(
        terminal.failure().disposition(),
        EngineFailureDisposition::RestartRequired
    );
    assert_eq!(
        terminal.failure().diagnostic().severity(),
        DiagnosticSeverity::Error
    );
    assert_eq!(terminal.lifecycle().phase(), LifecyclePhase::Failed);
    assert_work(terminal.lifecycle(), EngineWorkKind::Playback, false);
    assert_work(terminal.lifecycle(), EngineWorkKind::Rendering, false);
    assert_work(terminal.lifecycle(), EngineWorkKind::Export, false);

    let before_recovery_attempt = engine
        .snapshot()
        .expect("terminal snapshot is available")
        .state_revision();
    let rejected = errors
        .begin_recovery(
            &mut engine,
            EngineSubsystem::SharedState,
            terminal.failure().sequence(),
            EngineRecoveryRequest::RetryOperation,
        )
        .expect_err("terminal failures cannot recover in the current lifetime");
    assert_eq!(rejected.category(), ErrorCategory::Conflict);
    assert_eq!(rejected.recoverability(), Recoverability::Terminal);
    assert_eq!(
        engine
            .snapshot()
            .expect("rejected terminal recovery leaves state unchanged")
            .state_revision(),
        before_recovery_attempt
    );
}

#[test]
fn diagnostic_history_is_bounded_and_coordinator_access_is_engine_control_owned() {
    let coordinator = {
        let _domain = ExecutionDomain::EngineControl
            .enter_current()
            .expect("test owns the engine-control domain");
        let capacity_error = EngineErrorCoordinator::with_history_capacity(0)
            .expect_err("zero capacity cannot retain actionable diagnostics");
        assert_eq!(capacity_error.category(), ErrorCategory::InvalidInput);
        EngineErrorCoordinator::with_history_capacity(2).expect("small bounded history is valid")
    };
    let off_domain = std::thread::spawn(move || {
        coordinator
            .active_failure(EngineSubsystem::Playback)
            .expect_err("non-owner thread cannot inspect mutable coordinator state")
    })
    .join()
    .expect("off-domain proof thread joins");
    assert_eq!(off_domain.category(), ErrorCategory::Conflict);

    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns the engine-control domain");
    let mut engine = running_engine();
    let mut errors =
        EngineErrorCoordinator::with_history_capacity(2).expect("small bounded history is valid");
    let first = errors
        .report_failure(
            &mut engine,
            EngineSubsystem::Export,
            "write_packet",
            Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "destination paused writes",
            ),
        )
        .expect("first failure is recorded");
    let first_start = errors
        .begin_recovery(
            &mut engine,
            EngineSubsystem::Export,
            first.failure().sequence(),
            EngineRecoveryRequest::RetryOperation,
        )
        .expect("export retry starts");
    errors
        .complete_recovery(&mut engine, first_start.action())
        .expect("export retry succeeds");
    let second = errors
        .report_failure(
            &mut engine,
            EngineSubsystem::Playback,
            "refill_audio",
            Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Degraded,
                "audio output is temporarily silent",
            ),
        )
        .expect("second failure is recorded");
    assert_eq!(
        errors
            .diagnostic_history()
            .expect("owner can inspect bounded diagnostics")
            .iter()
            .map(|event| event.name())
            .collect::<Vec<_>>(),
        vec!["engine.subsystem.recovered", "engine.subsystem.failed"]
    );
    let second_start = errors
        .begin_recovery(
            &mut engine,
            EngineSubsystem::Playback,
            second.failure().sequence(),
            EngineRecoveryRequest::RestoreSubsystem,
        )
        .expect("playback restoration starts");
    errors
        .complete_recovery(&mut engine, second_start.action())
        .expect("playback restoration succeeds");
    assert_eq!(
        errors
            .diagnostic_history()
            .expect("owner can inspect bounded diagnostics")
            .iter()
            .map(|event| event.name())
            .collect::<Vec<_>>(),
        vec!["engine.subsystem.failed", "engine.subsystem.recovered"]
    );
}
