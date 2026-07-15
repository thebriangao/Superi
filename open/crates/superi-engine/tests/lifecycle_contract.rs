use superi_concurrency::lifecycle::LifecyclePhase;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability};
use superi_engine::lifecycle::{
    EngineHealth, EngineLifecycle, EngineLifecycleAction, EngineLifecycleActionKind,
    EngineSubsystem, EngineSubsystemState, EngineWorkAdmission, EngineWorkKind,
};

const START_ORDER: [EngineSubsystem; 6] = [
    EngineSubsystem::SharedState,
    EngineSubsystem::Projects,
    EngineSubsystem::Devices,
    EngineSubsystem::Playback,
    EngineSubsystem::Rendering,
    EngineSubsystem::Export,
];
const STOP_ORDER: [EngineSubsystem; 6] = [
    EngineSubsystem::Export,
    EngineSubsystem::Rendering,
    EngineSubsystem::Playback,
    EngineSubsystem::Devices,
    EngineSubsystem::Projects,
    EngineSubsystem::SharedState,
];
const SLEEP_ORDER: [EngineSubsystem; 5] = [
    EngineSubsystem::Export,
    EngineSubsystem::Rendering,
    EngineSubsystem::Playback,
    EngineSubsystem::Devices,
    EngineSubsystem::Projects,
];
const WAKE_ORDER: [EngineSubsystem; 5] = [
    EngineSubsystem::Projects,
    EngineSubsystem::Devices,
    EngineSubsystem::Playback,
    EngineSubsystem::Rendering,
    EngineSubsystem::Export,
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

fn complete_transition(
    engine: &mut EngineLifecycle,
    kind: EngineLifecycleActionKind,
) -> Vec<EngineSubsystem> {
    let mut order = Vec::new();
    loop {
        let snapshot = engine
            .snapshot()
            .expect("engine-control owner can inspect lifecycle state");
        let Some(action) = snapshot.pending_action() else {
            break;
        };
        assert_eq!(action.kind(), kind);
        order.push(action.subsystem());
        engine
            .complete_action(action)
            .expect("the exact transition action completes");
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
fn sleep_and_wake_quiesce_projects_devices_and_every_engine_workflow() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns the engine-control domain");
    let mut engine = EngineLifecycle::new().expect("canonical lifecycle plan is valid");
    assert_eq!(complete_start(&mut engine), START_ORDER);

    let running = engine.snapshot().expect("running snapshot is published");
    let old_permit = running
        .admit(EngineWorkKind::Export)
        .permit()
        .expect("running export receives a permit");
    let preparing = engine
        .begin_sleep()
        .expect("system sleep preparation begins");
    assert_eq!(preparing.phase(), LifecyclePhase::PreparingSleep);
    assert_eq!(
        engine.signal().load().phase(),
        LifecyclePhase::PreparingSleep
    );
    assert_eq!(
        preparing.pending_action().unwrap().subsystem(),
        EngineSubsystem::Export
    );
    assert_eq!(
        preparing.pending_action().unwrap().kind(),
        EngineLifecycleActionKind::PrepareSleep
    );
    expect_denied(preparing.admit(EngineWorkKind::Playback), None);
    expect_denied(preparing.admit(EngineWorkKind::Rendering), None);
    expect_denied(preparing.admit(EngineWorkKind::Export), None);
    assert!(!preparing.is_permit_current(old_permit));

    let repeated_sleep = engine
        .begin_sleep()
        .expect("repeated sleep notification is idempotent");
    assert_eq!(repeated_sleep.state_revision(), preparing.state_revision());
    assert_eq!(
        complete_transition(&mut engine, EngineLifecycleActionKind::PrepareSleep),
        SLEEP_ORDER
    );

    let sleeping = engine.snapshot().expect("sleeping snapshot is published");
    assert_eq!(sleeping.phase(), LifecyclePhase::Sleeping);
    assert_eq!(engine.signal().load().phase(), LifecyclePhase::Sleeping);
    let shared = sleeping.subsystem(EngineSubsystem::SharedState).unwrap();
    assert_eq!(shared.state(), EngineSubsystemState::Ready);
    assert!(shared.owns_resources());
    let projects = sleeping.subsystem(EngineSubsystem::Projects).unwrap();
    assert_eq!(projects.state(), EngineSubsystemState::Sleeping);
    assert!(projects.owns_resources());
    for subsystem in [
        EngineSubsystem::Devices,
        EngineSubsystem::Playback,
        EngineSubsystem::Rendering,
        EngineSubsystem::Export,
    ] {
        let state = sleeping.subsystem(subsystem).unwrap();
        assert_eq!(state.state(), EngineSubsystemState::Sleeping);
        assert!(!state.owns_resources());
    }

    let waking = engine
        .begin_wake()
        .expect("system wake revalidation begins");
    assert_eq!(waking.phase(), LifecyclePhase::Waking);
    assert_eq!(engine.signal().load().phase(), LifecyclePhase::Waking);
    assert_eq!(
        waking.pending_action().unwrap().subsystem(),
        EngineSubsystem::Projects
    );
    assert_eq!(
        waking.pending_action().unwrap().kind(),
        EngineLifecycleActionKind::Wake
    );
    let repeated_wake = engine
        .begin_wake()
        .expect("repeated wake notification is idempotent");
    assert_eq!(repeated_wake.state_revision(), waking.state_revision());
    assert_eq!(
        complete_transition(&mut engine, EngineLifecycleActionKind::Wake),
        WAKE_ORDER
    );

    let recovered = engine.snapshot().expect("wake completes to running");
    assert_eq!(recovered.phase(), LifecyclePhase::Running);
    assert_eq!(recovered.health(), EngineHealth::Healthy);
    for subsystem in EngineSubsystem::ALL {
        let state = recovered.subsystem(subsystem).unwrap();
        assert_eq!(state.state(), EngineSubsystemState::Ready);
        assert!(state.owns_resources());
    }
    expect_granted(recovered.admit(EngineWorkKind::Playback));
    expect_granted(recovered.admit(EngineWorkKind::Rendering));
    expect_granted(recovered.admit(EngineWorkKind::Export));

    let critical_wake = engine
        .begin_wake()
        .expect("wake without prior preparation still revalidates resources");
    assert_eq!(critical_wake.phase(), LifecyclePhase::Waking);
    assert_eq!(
        complete_transition(&mut engine, EngineLifecycleActionKind::Wake),
        WAKE_ORDER
    );
    assert_eq!(engine.snapshot().unwrap().phase(), LifecyclePhase::Running);
}

#[test]
fn recoverable_wake_failure_degrades_all_device_dependent_work_until_recovery() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns the engine-control domain");
    let mut engine = EngineLifecycle::new().expect("canonical lifecycle plan is valid");
    assert_eq!(complete_start(&mut engine), START_ORDER);
    engine.begin_sleep().expect("sleep preparation begins");
    assert_eq!(
        complete_transition(&mut engine, EngineLifecycleActionKind::PrepareSleep),
        SLEEP_ORDER
    );

    engine.begin_wake().expect("wake revalidation begins");
    let projects = current_action(&engine);
    assert_eq!(projects.subsystem(), EngineSubsystem::Projects);
    engine
        .complete_action(projects)
        .expect("project state revalidates");
    let devices = current_action(&engine);
    assert_eq!(devices.subsystem(), EngineSubsystem::Devices);
    let degraded = engine
        .fail_action(
            devices,
            Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "device resources are not ready after wake",
            ),
        )
        .expect("recoverable device wake failure stays explicit");
    assert_eq!(degraded.phase(), LifecyclePhase::Waking);
    assert_eq!(degraded.health(), EngineHealth::Degraded);
    assert_eq!(
        complete_transition(&mut engine, EngineLifecycleActionKind::Wake),
        [
            EngineSubsystem::Playback,
            EngineSubsystem::Rendering,
            EngineSubsystem::Export,
        ]
    );

    let running_degraded = engine
        .snapshot()
        .expect("wake reaches a degraded running state");
    assert_eq!(running_degraded.phase(), LifecyclePhase::Running);
    assert_eq!(running_degraded.health(), EngineHealth::Degraded);
    let devices = running_degraded
        .subsystem(EngineSubsystem::Devices)
        .unwrap();
    assert_eq!(devices.state(), EngineSubsystemState::Degraded);
    assert!(devices.owns_resources());
    expect_denied(
        running_degraded.admit(EngineWorkKind::Playback),
        Some(EngineSubsystem::Devices),
    );
    expect_denied(
        running_degraded.admit(EngineWorkKind::Rendering),
        Some(EngineSubsystem::Devices),
    );
    expect_denied(
        running_degraded.admit(EngineWorkKind::Export),
        Some(EngineSubsystem::Devices),
    );

    let recovering = engine
        .begin_recovery(EngineSubsystem::Devices)
        .expect("degraded devices can be reacquired");
    let action = recovering.pending_action().unwrap();
    assert_eq!(action.kind(), EngineLifecycleActionKind::Recover);
    let recovered = engine
        .complete_action(action)
        .expect("device recovery restores dependent work");
    assert_eq!(recovered.health(), EngineHealth::Healthy);
    expect_granted(recovered.admit(EngineWorkKind::Playback));
    expect_granted(recovered.admit(EngineWorkKind::Rendering));
    expect_granted(recovered.admit(EngineWorkKind::Export));
}

#[test]
fn newer_power_notifications_supersede_only_the_exact_pending_transition() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns the engine-control domain");
    let mut engine = EngineLifecycle::new().expect("canonical lifecycle plan is valid");
    assert_eq!(complete_start(&mut engine), START_ORDER);

    engine.begin_sleep().expect("sleep preparation begins");
    let export_sleep = current_action(&engine);
    engine
        .complete_action(export_sleep)
        .expect("export releases before sleep");
    let stale_render_sleep = current_action(&engine);
    assert_eq!(stale_render_sleep.subsystem(), EngineSubsystem::Rendering);

    let waking = engine
        .begin_wake()
        .expect("wake supersedes incomplete sleep preparation");
    assert_eq!(waking.phase(), LifecyclePhase::Waking);
    assert_eq!(
        waking.pending_action().unwrap().subsystem(),
        EngineSubsystem::Projects
    );
    assert_eq!(
        engine
            .complete_action(stale_render_sleep)
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );
    let project_wake = current_action(&engine);
    engine
        .complete_action(project_wake)
        .expect("project state revalidates");
    let stale_device_wake = current_action(&engine);
    assert_eq!(stale_device_wake.subsystem(), EngineSubsystem::Devices);

    let preparing_again = engine
        .begin_sleep()
        .expect("new sleep notification supersedes incomplete wake");
    assert_eq!(preparing_again.phase(), LifecyclePhase::PreparingSleep);
    assert_eq!(
        preparing_again.pending_action().unwrap().subsystem(),
        EngineSubsystem::Rendering
    );
    assert_eq!(
        engine
            .complete_action(stale_device_wake)
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );
    assert_eq!(
        complete_transition(&mut engine, EngineLifecycleActionKind::PrepareSleep),
        [
            EngineSubsystem::Rendering,
            EngineSubsystem::Playback,
            EngineSubsystem::Devices,
            EngineSubsystem::Projects,
        ]
    );
    assert_eq!(engine.snapshot().unwrap().phase(), LifecyclePhase::Sleeping);

    engine.begin_wake().expect("final wake begins");
    assert_eq!(
        complete_transition(&mut engine, EngineLifecycleActionKind::Wake),
        WAKE_ORDER
    );
    assert_eq!(engine.snapshot().unwrap().phase(), LifecyclePhase::Running);
}

#[test]
fn terminal_wake_failure_enters_failed_lifecycle_and_requires_a_new_lifetime() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns the engine-control domain");
    let mut engine = EngineLifecycle::new().expect("canonical lifecycle plan is valid");
    assert_eq!(complete_start(&mut engine), START_ORDER);
    engine.begin_sleep().expect("sleep preparation begins");
    complete_transition(&mut engine, EngineLifecycleActionKind::PrepareSleep);
    engine.begin_wake().expect("wake revalidation begins");

    let project_wake = current_action(&engine);
    let failed = engine
        .fail_action(
            project_wake,
            Error::new(
                ErrorCategory::CorruptData,
                Recoverability::Terminal,
                "retained project state failed wake validation",
            ),
        )
        .expect("terminal wake failure enters failed lifecycle");
    assert_eq!(failed.phase(), LifecyclePhase::Failed);
    assert_eq!(failed.health(), EngineHealth::Failed);
    assert!(failed.pending_action().is_none());
    expect_denied(failed.admit(EngineWorkKind::Playback), None);
    assert_eq!(
        engine.begin_wake().unwrap_err().recoverability(),
        Recoverability::UserCorrectable
    );

    let stopping = engine
        .begin_shutdown()
        .expect("failed wake can shut down retained resources");
    assert_eq!(stopping.phase(), LifecyclePhase::Stopping);
    assert_eq!(
        complete_stop(&mut engine),
        [EngineSubsystem::Projects, EngineSubsystem::SharedState]
    );
    let restarted = engine
        .begin_restart()
        .expect("stopped engine starts a new lifetime");
    assert_eq!(restarted.phase(), LifecyclePhase::Starting);
    assert_eq!(restarted.lifetime(), 2);
    assert_eq!(complete_start(&mut engine), START_ORDER);
    assert_eq!(engine.snapshot().unwrap().phase(), LifecyclePhase::Running);
}

#[test]
fn shutdown_interrupts_sleep_and_wake_without_reviving_released_resources() {
    let _domain = ExecutionDomain::EngineControl
        .enter_current()
        .expect("test owns the engine-control domain");
    let mut engine = EngineLifecycle::new().expect("canonical lifecycle plan is valid");
    assert_eq!(complete_start(&mut engine), START_ORDER);

    engine.begin_sleep().expect("sleep preparation begins");
    let export_sleep = current_action(&engine);
    engine
        .complete_action(export_sleep)
        .expect("export releases before sleep");
    let stale_sleep = current_action(&engine);
    assert_eq!(stale_sleep.subsystem(), EngineSubsystem::Rendering);
    let stopping = engine
        .begin_shutdown()
        .expect("application shutdown supersedes sleep preparation");
    assert_eq!(stopping.phase(), LifecyclePhase::Stopping);
    assert_eq!(
        stopping.pending_action().unwrap().subsystem(),
        EngineSubsystem::Rendering
    );
    assert_eq!(
        complete_stop(&mut engine),
        [
            EngineSubsystem::Rendering,
            EngineSubsystem::Playback,
            EngineSubsystem::Devices,
            EngineSubsystem::Projects,
            EngineSubsystem::SharedState,
        ]
    );
    assert_eq!(
        engine.complete_action(stale_sleep).unwrap_err().category(),
        ErrorCategory::Conflict
    );
    for subsystem in EngineSubsystem::ALL {
        let state = engine
            .snapshot()
            .unwrap()
            .subsystem(subsystem)
            .unwrap()
            .clone();
        assert_eq!(state.state(), EngineSubsystemState::Offline);
        assert!(!state.owns_resources());
    }

    engine.begin_restart().expect("stopped engine restarts");
    assert_eq!(complete_start(&mut engine), START_ORDER);
    engine
        .begin_sleep()
        .expect("second sleep preparation begins");
    complete_transition(&mut engine, EngineLifecycleActionKind::PrepareSleep);
    engine.begin_wake().expect("wake revalidation begins");
    let projects_wake = current_action(&engine);
    engine
        .complete_action(projects_wake)
        .expect("retained project state revalidates");
    let stale_wake = current_action(&engine);
    assert_eq!(stale_wake.subsystem(), EngineSubsystem::Devices);
    engine
        .begin_shutdown()
        .expect("application shutdown supersedes wake revalidation");
    assert_eq!(
        complete_stop(&mut engine),
        [EngineSubsystem::Projects, EngineSubsystem::SharedState]
    );
    assert_eq!(
        engine.complete_action(stale_wake).unwrap_err().category(),
        ErrorCategory::Conflict
    );
    assert_eq!(engine.snapshot().unwrap().phase(), LifecyclePhase::Stopped);
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
    for subsystem in [EngineSubsystem::Projects, EngineSubsystem::Devices] {
        let action = current_action(&engine);
        assert_eq!(action.subsystem(), subsystem);
        engine
            .complete_action(action)
            .expect("playback dependency initializes");
    }
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
    assert_eq!(rollback.subsystem(), EngineSubsystem::Devices);

    let stale = engine
        .complete_action(failed_action)
        .expect_err("late initialization cannot complete rollback state");
    assert_eq!(stale.category(), ErrorCategory::Conflict);
    assert_eq!(
        complete_stop(&mut engine),
        [
            EngineSubsystem::Devices,
            EngineSubsystem::Projects,
            EngineSubsystem::SharedState,
        ]
    );
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
        [
            EngineSubsystem::Rendering,
            EngineSubsystem::Devices,
            EngineSubsystem::Projects,
            EngineSubsystem::SharedState,
        ]
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
