use superi_concurrency::lifecycle::LifecyclePhase;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability};
use superi_engine::lifecycle::{
    EngineHealth, EngineLifecycle, EngineLifecycleAction, EngineLifecycleActionKind,
    EngineSubsystem, EngineSubsystemState, EngineWorkAdmission, EngineWorkKind,
};

const START_ORDER: [EngineSubsystem; 4] = [
    EngineSubsystem::SharedState,
    EngineSubsystem::Playback,
    EngineSubsystem::Rendering,
    EngineSubsystem::Export,
];
const STOP_ORDER: [EngineSubsystem; 4] = [
    EngineSubsystem::Export,
    EngineSubsystem::Rendering,
    EngineSubsystem::Playback,
    EngineSubsystem::SharedState,
];

fn current_action(engine: &EngineLifecycle) -> EngineLifecycleAction {
    engine
        .snapshot()
        .expect("engine-control owner can inspect lifecycle state")
        .pending_action()
        .expect("the lifecycle has one pending action")
}

fn complete_start(engine: &mut EngineLifecycle) -> Vec<EngineSubsystem> {
    let mut order = Vec::new();
    while engine
        .snapshot()
        .expect("engine-control owner can inspect lifecycle state")
        .phase()
        == LifecyclePhase::Starting
    {
        let action = current_action(engine);
        assert_eq!(action.kind(), EngineLifecycleActionKind::Initialize);
        order.push(action.subsystem());
        engine
            .complete_action(action)
            .expect("the exact initialization action completes");
    }
    order
}

fn complete_stop(engine: &mut EngineLifecycle) -> Vec<EngineSubsystem> {
    let mut order = Vec::new();
    loop {
        let snapshot = engine
            .snapshot()
            .expect("engine-control owner can inspect lifecycle state");
        let Some(action) = snapshot.pending_action() else {
            break;
        };
        assert_eq!(action.kind(), EngineLifecycleActionKind::Teardown);
        order.push(action.subsystem());
        engine
            .complete_action(action)
            .expect("the exact teardown action completes");
    }
    order
}

fn expect_granted(admission: EngineWorkAdmission) {
    assert!(admission.permit().is_some(), "work should be admitted");
    assert!(admission.denial().is_none());
}

fn expect_denied(admission: EngineWorkAdmission, blocker: Option<EngineSubsystem>) {
    assert!(admission.permit().is_none(), "work should be denied");
    assert_eq!(
        admission
            .denial()
            .expect("denied work retains a diagnostic")
            .blocking_subsystem(),
        blocker
    );
}

#[test]
fn startup_degradation_recovery_shutdown_and_restart_share_one_coherent_state() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns the engine-control domain");
    let mut engine = EngineLifecycle::new().expect("canonical lifecycle plan is valid");

    let initial = engine.snapshot().expect("initial snapshot is published");
    assert_eq!(initial.source_domain(), ExecutionDomain::EngineControl);
    assert_eq!(initial.phase(), LifecyclePhase::Starting);
    assert_eq!(initial.health(), EngineHealth::Healthy);
    assert_eq!(initial.lifetime(), 1);
    assert_eq!(
        initial.pending_action().unwrap().subsystem(),
        EngineSubsystem::SharedState
    );
    assert_eq!(initial.subsystems().len(), EngineSubsystem::ALL.len());
    expect_denied(initial.admit(EngineWorkKind::Playback), None);

    assert_eq!(complete_start(&mut engine), START_ORDER);
    let running = engine.snapshot().expect("running snapshot is published");
    assert_eq!(running.phase(), LifecyclePhase::Running);
    assert_eq!(engine.signal().load().phase(), LifecyclePhase::Running);
    let cross_thread = running.clone();
    std::thread::spawn(move || {
        assert_eq!(cross_thread.phase(), LifecyclePhase::Running);
        assert!(cross_thread
            .admit(EngineWorkKind::Export)
            .permit()
            .is_some());
    })
    .join()
    .expect("immutable lifecycle snapshots cross thread owners");
    for subsystem in EngineSubsystem::ALL {
        let state = running.subsystem(subsystem).unwrap();
        assert_eq!(state.state(), EngineSubsystemState::Ready);
        assert!(state.owns_resources());
        assert!(state.failure().is_none());
    }
    expect_granted(running.admit(EngineWorkKind::Playback));
    expect_granted(running.admit(EngineWorkKind::Rendering));
    expect_granted(running.admit(EngineWorkKind::Export));

    let old_running = running.clone();
    let old_playback_permit = old_running
        .admit(EngineWorkKind::Playback)
        .permit()
        .expect("running playback receives a permit");
    let degraded_error = Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Degraded,
        "playback device temporarily unavailable",
    )
    .with_context(
        ErrorContext::new("superi-engine.lifecycle-test", "open_playback_device")
            .with_field("device", "default"),
    );
    let degraded = engine
        .report_runtime_failure(EngineSubsystem::Playback, degraded_error)
        .expect("recoverable playback failure degrades the engine");
    assert_eq!(degraded.phase(), LifecyclePhase::Running);
    assert_eq!(degraded.health(), EngineHealth::Degraded);
    assert_eq!(
        degraded
            .subsystem(EngineSubsystem::Playback)
            .unwrap()
            .state(),
        EngineSubsystemState::Degraded
    );
    assert_eq!(
        degraded
            .subsystem(EngineSubsystem::Playback)
            .unwrap()
            .failure()
            .unwrap()
            .contexts()[0]
            .field("device"),
        Some("default")
    );
    expect_denied(
        degraded.admit(EngineWorkKind::Playback),
        Some(EngineSubsystem::Playback),
    );
    expect_granted(degraded.admit(EngineWorkKind::Rendering));
    expect_granted(degraded.admit(EngineWorkKind::Export));
    assert!(old_running.is_permit_current(old_playback_permit));
    assert!(!degraded.is_permit_current(old_playback_permit));
    assert_eq!(old_running.health(), EngineHealth::Healthy);

    let recovering = engine
        .begin_recovery(EngineSubsystem::Playback)
        .expect("degraded playback can begin recovery");
    let recovery_action = recovering.pending_action().unwrap();
    assert_eq!(recovery_action.kind(), EngineLifecycleActionKind::Recover);
    assert_eq!(recovery_action.subsystem(), EngineSubsystem::Playback);
    let recovered = engine
        .complete_action(recovery_action)
        .expect("revalidated playback completes recovery");
    assert_eq!(recovered.health(), EngineHealth::Healthy);
    assert!(recovered
        .subsystem(EngineSubsystem::Playback)
        .unwrap()
        .failure()
        .is_none());
    assert_eq!(
        recovered.failure().unwrap().message(),
        "playback device temporarily unavailable"
    );
    expect_granted(recovered.admit(EngineWorkKind::Playback));

    let render_error = Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "render device is rebuilding",
    );
    let render_degraded = engine
        .report_runtime_failure(EngineSubsystem::Rendering, render_error)
        .expect("retryable render failure degrades dependent work");
    expect_granted(render_degraded.admit(EngineWorkKind::Playback));
    expect_denied(
        render_degraded.admit(EngineWorkKind::Rendering),
        Some(EngineSubsystem::Rendering),
    );
    expect_denied(
        render_degraded.admit(EngineWorkKind::Export),
        Some(EngineSubsystem::Rendering),
    );

    let permit_before_shutdown = engine
        .begin_recovery(EngineSubsystem::Rendering)
        .expect("render recovery begins")
        .pending_action()
        .map(|action| {
            engine
                .complete_action(action)
                .expect("render recovery completes")
        })
        .unwrap()
        .admit(EngineWorkKind::Export)
        .permit()
        .unwrap();
    let stopping = engine.begin_shutdown().expect("shutdown begins");
    assert_eq!(stopping.phase(), LifecyclePhase::Stopping);
    assert!(!stopping.is_permit_current(permit_before_shutdown));
    assert_eq!(complete_stop(&mut engine), STOP_ORDER);
    let stopped = engine.snapshot().expect("stopped snapshot is published");
    assert_eq!(stopped.phase(), LifecyclePhase::Stopped);
    assert_eq!(engine.signal().load().phase(), LifecyclePhase::Stopped);
    expect_denied(stopped.admit(EngineWorkKind::Playback), None);
    for subsystem in EngineSubsystem::ALL {
        let state = stopped.subsystem(subsystem).unwrap();
        assert_eq!(state.state(), EngineSubsystemState::Offline);
        assert!(!state.owns_resources());
    }

    let restarting = engine.begin_restart().expect("stopped engine restarts");
    assert_eq!(restarting.phase(), LifecyclePhase::Starting);
    assert_eq!(restarting.lifetime(), 2);
    assert_eq!(complete_start(&mut engine), START_ORDER);
    let restarted = engine.snapshot().expect("restarted snapshot is published");
    assert_eq!(restarted.phase(), LifecyclePhase::Running);
    assert_eq!(restarted.lifetime(), 2);
    expect_granted(restarted.admit(EngineWorkKind::Export));

    let direct_restart = engine
        .begin_restart()
        .expect("running engine begins orderly restart teardown");
    assert_eq!(direct_restart.phase(), LifecyclePhase::Stopping);
    let mut direct_stop_order = Vec::new();
    loop {
        let action = current_action(&engine);
        if action.kind() != EngineLifecycleActionKind::Teardown {
            break;
        }
        direct_stop_order.push(action.subsystem());
        engine
            .complete_action(action)
            .expect("restart teardown action completes");
    }
    assert_eq!(direct_stop_order, STOP_ORDER);
    let fresh_start = engine.snapshot().expect("restart emits a fresh start");
    assert_eq!(fresh_start.phase(), LifecyclePhase::Starting);
    assert_eq!(fresh_start.lifetime(), 3);
    assert_eq!(complete_start(&mut engine), START_ORDER);
    assert_eq!(engine.snapshot().unwrap().phase(), LifecyclePhase::Running);
}

#[test]
fn initialization_failure_rolls_back_owned_dependencies_and_rejects_stale_work() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns the engine-control domain");
    let mut engine = EngineLifecycle::new().expect("canonical lifecycle plan is valid");

    let shared_action = current_action(&engine);
    engine
        .complete_action(shared_action)
        .expect("shared state initializes");
    let failed_action = current_action(&engine);
    assert_eq!(failed_action.subsystem(), EngineSubsystem::Playback);
    let failed = engine
        .fail_action(
            failed_action,
            Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "playback initialization failed",
            ),
        )
        .expect("initialization failure enters rollback");
    assert_eq!(failed.phase(), LifecyclePhase::Failed);
    assert_eq!(failed.health(), EngineHealth::Failed);
    assert_eq!(
        failed.subsystem(EngineSubsystem::Playback).unwrap().state(),
        EngineSubsystemState::Failed
    );
    let rollback = failed.pending_action().unwrap();
    assert_eq!(rollback.kind(), EngineLifecycleActionKind::Teardown);
    assert_eq!(rollback.subsystem(), EngineSubsystem::SharedState);

    let stale = engine
        .complete_action(failed_action)
        .expect_err("late initialization cannot complete rollback state");
    assert_eq!(stale.category(), ErrorCategory::Conflict);
    engine
        .complete_action(rollback)
        .expect("owned shared state rolls back");
    let rolled_back = engine.snapshot().expect("rollback snapshot is published");
    assert_eq!(rolled_back.phase(), LifecyclePhase::Failed);
    assert!(rolled_back.pending_action().is_none());
    assert!(!rolled_back
        .subsystem(EngineSubsystem::SharedState)
        .unwrap()
        .owns_resources());

    let restarting = engine
        .begin_restart()
        .expect("failed engine tears down and starts a fresh lifetime");
    assert_eq!(restarting.phase(), LifecyclePhase::Starting);
    assert_eq!(restarting.lifetime(), 2);
    assert_eq!(complete_start(&mut engine), START_ORDER);
    assert_eq!(engine.snapshot().unwrap().phase(), LifecyclePhase::Running);
}

#[test]
fn terminal_failure_closes_all_work_and_teardown_failure_requires_a_retry() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns the engine-control domain");
    let mut engine = EngineLifecycle::new().expect("canonical lifecycle plan is valid");
    assert_eq!(complete_start(&mut engine), START_ORDER);

    let terminal = engine
        .report_runtime_failure(
            EngineSubsystem::Playback,
            Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "playback ownership invariant failed",
            ),
        )
        .expect("terminal subsystem failure reaches engine lifecycle");
    assert_eq!(terminal.phase(), LifecyclePhase::Failed);
    assert_eq!(engine.signal().load().phase(), LifecyclePhase::Failed);
    expect_denied(terminal.admit(EngineWorkKind::Playback), None);
    expect_denied(terminal.admit(EngineWorkKind::Rendering), None);
    expect_denied(terminal.admit(EngineWorkKind::Export), None);

    let stopping = engine
        .begin_shutdown()
        .expect("failed engine can shut down");
    assert_eq!(stopping.phase(), LifecyclePhase::Stopping);
    let export_action = stopping.pending_action().unwrap();
    assert_eq!(export_action.subsystem(), EngineSubsystem::Export);
    let failed_teardown = engine
        .fail_action(
            export_action,
            Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "export resource release was interrupted",
            ),
        )
        .expect("teardown failure remains explicit while other teardown continues");
    assert_eq!(failed_teardown.phase(), LifecyclePhase::Failed);
    assert!(failed_teardown
        .subsystem(EngineSubsystem::Export)
        .unwrap()
        .owns_resources());

    let remaining = complete_stop(&mut engine);
    assert_eq!(remaining, [EngineSubsystem::Playback]);
    let retry_required = engine.snapshot().unwrap();
    assert_eq!(retry_required.phase(), LifecyclePhase::Failed);
    assert!(retry_required.pending_action().is_none());
    assert!(retry_required
        .subsystem(EngineSubsystem::Export)
        .unwrap()
        .owns_resources());

    let retry = engine
        .begin_shutdown()
        .expect("retained teardown is retried");
    assert_eq!(retry.phase(), LifecyclePhase::Stopping);
    let retry_action = retry.pending_action().unwrap();
    assert_eq!(retry_action.subsystem(), EngineSubsystem::Export);
    engine
        .complete_action(retry_action)
        .expect("successful retry releases the retained resource");
    assert_eq!(
        complete_stop(&mut engine),
        [EngineSubsystem::Rendering, EngineSubsystem::SharedState,]
    );
    assert_eq!(engine.snapshot().unwrap().phase(), LifecyclePhase::Stopped);
}

#[test]
fn lifecycle_mutation_is_rejected_outside_engine_control() {
    let construction_error = match EngineLifecycle::new() {
        Ok(_) => panic!("lifecycle construction must require engine-control ownership"),
        Err(error) => error,
    };
    assert_eq!(construction_error.category(), ErrorCategory::Conflict);
    assert_eq!(
        construction_error.recoverability(),
        Recoverability::UserCorrectable
    );

    let mut engine = {
        let _domain = ExecutionDomain::EngineControl
            .enter_current()
            .expect("test temporarily owns engine control");
        EngineLifecycle::new().expect("owner constructs lifecycle")
    };
    let mutation_error = engine
        .begin_shutdown()
        .expect_err("lifecycle mutation outside engine control is rejected");
    assert_eq!(mutation_error.category(), ErrorCategory::Conflict);
    let snapshot_error = engine
        .snapshot()
        .expect_err("owner state cannot be read through the mutable owner off-domain");
    assert_eq!(snapshot_error.category(), ErrorCategory::Conflict);

    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test reacquires engine control");
    let unchanged = engine
        .snapshot()
        .expect("owner can inspect lifecycle again");
    assert_eq!(unchanged.phase(), LifecyclePhase::Starting);
    assert_eq!(
        unchanged.pending_action().unwrap().subsystem(),
        EngineSubsystem::SharedState
    );
}
