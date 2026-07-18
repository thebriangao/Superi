use std::time::Duration;

use superi_core::error::{ErrorCategory, Recoverability};
use superi_desktop::lifecycle::{
    ApplicationLifecycle, ApplicationLifecyclePhase, EngineLifecyclePhase, HeadlessEngineFailure,
    LifecycleIntent,
};

#[test]
fn startup_and_shutdown_require_exact_engine_acknowledgement() {
    let lifecycle = ApplicationLifecycle::new().unwrap();
    let engine = lifecycle.headless_engine_participant().unwrap();

    let starting = lifecycle.snapshot();
    assert_eq!(
        starting.application_phase(),
        ApplicationLifecyclePhase::Starting
    );
    assert_eq!(starting.engine_phase(), EngineLifecyclePhase::Starting);
    assert_eq!(starting.intent(), LifecycleIntent::Start);
    assert_eq!(starting.engine_generation(), 1);
    assert!(starting.engine_acknowledgement_pending());

    let start_token = engine.signal().load();
    let running = engine.acknowledge(start_token).unwrap();
    assert_eq!(
        running.application_phase(),
        ApplicationLifecyclePhase::Running
    );
    assert_eq!(running.engine_phase(), EngineLifecyclePhase::Ready);
    assert!(!running.engine_acknowledgement_pending());

    let stopping = lifecycle.request_shutdown().unwrap();
    assert_eq!(
        stopping.application_phase(),
        ApplicationLifecyclePhase::Stopping
    );
    assert_eq!(stopping.engine_phase(), EngineLifecyclePhase::Stopping);
    let stopped = engine.acknowledge(engine.signal().load()).unwrap();
    assert_eq!(
        stopped.application_phase(),
        ApplicationLifecyclePhase::Stopped
    );
    assert_eq!(stopped.engine_phase(), EngineLifecyclePhase::Stopped);
}

#[test]
fn restart_is_ordered_and_rejects_a_stale_completion() {
    let lifecycle = ApplicationLifecycle::new().unwrap();
    let engine = lifecycle.headless_engine_participant().unwrap();
    let stale_start = engine.signal().load();
    engine.acknowledge(stale_start).unwrap();

    let restarting = lifecycle.request_restart().unwrap();
    assert_eq!(
        restarting.application_phase(),
        ApplicationLifecyclePhase::Restarting
    );
    assert_eq!(restarting.engine_phase(), EngineLifecyclePhase::Stopping);
    assert!(engine.acknowledge(stale_start).is_err());
    assert_eq!(lifecycle.snapshot().engine_generation(), 1);

    let starting_again = engine.acknowledge(engine.signal().load()).unwrap();
    assert_eq!(
        starting_again.application_phase(),
        ApplicationLifecyclePhase::Restarting
    );
    assert_eq!(
        starting_again.engine_phase(),
        EngineLifecyclePhase::Starting
    );
    assert_eq!(starting_again.engine_generation(), 2);

    let running_again = engine.acknowledge(engine.signal().load()).unwrap();
    assert_eq!(
        running_again.application_phase(),
        ApplicationLifecyclePhase::Running
    );
    assert_eq!(running_again.engine_phase(), EngineLifecyclePhase::Ready);
    assert_eq!(running_again.engine_generation(), 2);
}

#[test]
fn classified_failure_retains_safe_context_and_recovers_in_a_new_generation() {
    let lifecycle = ApplicationLifecycle::new().unwrap();
    let engine = lifecycle.headless_engine_participant().unwrap();
    let failed = engine
        .fail(
            HeadlessEngineFailure::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "The headless engine connection is unavailable.",
            )
            .with_context("superi-desktop.engine", "connect"),
        )
        .unwrap();

    assert_eq!(
        failed.application_phase(),
        ApplicationLifecyclePhase::Failed
    );
    assert_eq!(failed.engine_phase(), EngineLifecyclePhase::Failed);
    assert!(failed.can_retry());
    let failure = failed.failure().unwrap();
    assert_eq!(failure.category(), "unavailable");
    assert_eq!(failure.recoverability(), "retryable");
    assert_eq!(
        failure.summary(),
        "The headless engine connection is unavailable."
    );
    assert_eq!(failure.contexts()[0].component(), "superi-desktop.engine");
    assert_eq!(failure.contexts()[0].operation(), "connect");

    let stopping = lifecycle.request_recovery().unwrap();
    assert_eq!(
        stopping.application_phase(),
        ApplicationLifecyclePhase::Recovering
    );
    assert_eq!(stopping.engine_phase(), EngineLifecyclePhase::Stopping);
    let starting = engine.acknowledge(engine.signal().load()).unwrap();
    assert_eq!(
        starting.application_phase(),
        ApplicationLifecyclePhase::Recovering
    );
    assert_eq!(starting.engine_phase(), EngineLifecyclePhase::Starting);
    assert_eq!(starting.engine_generation(), 2);
    let recovered = engine.acknowledge(engine.signal().load()).unwrap();
    assert_eq!(
        recovered.application_phase(),
        ApplicationLifecyclePhase::Running
    );
    assert_eq!(recovered.engine_phase(), EngineLifecyclePhase::Ready);
    assert!(recovered.failure().is_none());
}

#[test]
fn terminal_failure_disables_retry_but_preserves_restart_and_shutdown() {
    let lifecycle = ApplicationLifecycle::new().unwrap();
    let engine = lifecycle.headless_engine_participant().unwrap();
    let failed = engine
        .fail(HeadlessEngineFailure::new(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "The engine lifetime cannot continue safely.",
        ))
        .unwrap();

    assert!(!failed.can_retry());
    assert!(failed.can_restart());
    assert!(failed.can_shutdown());
    assert!(lifecycle.request_recovery().is_err());
    assert!(lifecycle.request_restart().is_ok());
}

#[test]
fn wait_for_change_is_blocking_safe_and_returns_new_state_only() {
    let lifecycle = ApplicationLifecycle::new().unwrap();
    let revision = lifecycle.snapshot().revision();
    assert!(lifecycle
        .wait_for_change(revision, Duration::from_millis(1))
        .unwrap()
        .is_none());

    lifecycle.request_shutdown().unwrap();
    let changed = lifecycle
        .wait_for_change(revision, Duration::from_millis(1))
        .unwrap()
        .unwrap();
    assert!(changed.revision() > revision);
    assert_eq!(
        changed.application_phase(),
        ApplicationLifecyclePhase::Stopping
    );
}
