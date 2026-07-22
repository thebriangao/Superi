use std::time::{Duration, Instant};

use superi_desktop::engine::LinkedEngineProcess;
use superi_desktop::lifecycle::{ApplicationLifecycle, ApplicationLifecyclePhase};
use superi_desktop::process_runtime::{
    DesktopProcessRuntime, DesktopProcessServiceId, DesktopProcessServicePhase,
};
use superi_desktop::viewport::DesktopViewportState;
use superi_desktop::window_session::DesktopWindowState;

const WAIT_TIMEOUT: Duration = Duration::from_secs(2);

#[test]
fn linked_engine_reports_every_owned_execution_service_and_joins_them_all() {
    let runtime = DesktopProcessRuntime::new();
    let lifecycle = ApplicationLifecycle::new().unwrap();
    let process =
        LinkedEngineProcess::launch_with_runtime(lifecycle.clone(), runtime.clone()).unwrap();

    wait_for_running(&lifecycle);
    let running = runtime.snapshot();
    let engine = running
        .service(DesktopProcessServiceId::EngineControl)
        .unwrap();
    let playback = running.service(DesktopProcessServiceId::Playback).unwrap();
    let workers = running
        .service(DesktopProcessServiceId::BackgroundWorkers)
        .unwrap();
    assert_eq!(engine.phase(), DesktopProcessServicePhase::Running);
    assert_eq!(engine.active_units(), 1);
    assert!(engine.join_pending());
    assert_eq!(playback.phase(), DesktopProcessServicePhase::Running);
    assert_eq!(playback.active_units(), 1);
    assert!(playback.join_pending());
    assert_eq!(workers.phase(), DesktopProcessServicePhase::Running);
    assert!(workers.active_units() > 0);
    assert_eq!(workers.active_units(), workers.owned_units());

    lifecycle.request_shutdown().unwrap();
    process.join().unwrap();

    let stopped = runtime.snapshot();
    for service_id in [
        DesktopProcessServiceId::EngineControl,
        DesktopProcessServiceId::Playback,
        DesktopProcessServiceId::BackgroundWorkers,
    ] {
        let service = stopped.service(service_id).unwrap();
        assert_eq!(service.phase(), DesktopProcessServicePhase::Stopped);
        assert_eq!(service.active_units(), 0);
        assert!(!service.join_pending());
    }
}

#[test]
fn native_shell_owners_have_idempotent_explicit_cleanup_without_initialization() {
    let runtime = DesktopProcessRuntime::new();
    let viewport = DesktopViewportState::with_runtime(runtime.clone());
    let windows = DesktopWindowState::with_runtime(runtime.clone());

    viewport.shutdown_and_join().unwrap();
    viewport.shutdown_and_join().unwrap();
    windows.shutdown_and_join().unwrap();
    windows.shutdown_and_join().unwrap();

    let snapshot = runtime.snapshot();
    for service_id in [
        DesktopProcessServiceId::GpuSubmission,
        DesktopProcessServiceId::WindowPersistence,
    ] {
        let service = snapshot.service(service_id).unwrap();
        assert_eq!(service.phase(), DesktopProcessServicePhase::Stopped);
        assert_eq!(service.owned_units(), 0);
        assert!(!service.join_pending());
    }
}

fn wait_for_running(lifecycle: &ApplicationLifecycle) {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    loop {
        let snapshot = lifecycle.snapshot();
        if snapshot.application_phase() == ApplicationLifecyclePhase::Running {
            return;
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .expect("linked engine lifecycle transition timed out");
        lifecycle
            .wait_for_change(snapshot.revision(), remaining)
            .expect("linked engine lifecycle wait should succeed");
    }
}
