use std::time::{Duration, Instant};

use superi_api::commands::GetEngineIntegrationValidationResult;
use superi_api::validation::IntegrationCondition;
use superi_core::error::ErrorCategory;
use superi_desktop::engine::LinkedEngineProcess;
use superi_desktop::lifecycle::{ApplicationLifecycle, ApplicationLifecyclePhase};

const WAIT_TIMEOUT: Duration = Duration::from_secs(2);

#[test]
fn linked_process_owns_engine_control_and_projects_the_existing_validation_contract() {
    let lifecycle = ApplicationLifecycle::new().unwrap();
    let process = LinkedEngineProcess::launch(lifecycle.clone()).unwrap();
    let connection = process.connection();

    assert_eq!(process.thread_name(), "superi-engine-control");
    wait_for_running_generation(&lifecycle, 1);

    let pending = connection.request_integration_validation().unwrap();
    let result: GetEngineIntegrationValidationResult = pending.wait(WAIT_TIMEOUT).unwrap();
    assert_eq!(
        result.snapshot().condition(),
        IntegrationCondition::Starting
    );
    assert!(result.snapshot().is_coherent());
    assert!(result.snapshot().pending_action().is_some());

    lifecycle.request_shutdown().unwrap();
    process.join().unwrap();
    assert_eq!(
        lifecycle.snapshot().application_phase(),
        ApplicationLifecyclePhase::Stopped
    );
}

#[test]
fn stable_connection_reaches_a_fresh_dispatcher_after_application_restart() {
    let lifecycle = ApplicationLifecycle::new().unwrap();
    let process = LinkedEngineProcess::launch(lifecycle.clone()).unwrap();
    let connection = process.connection();

    wait_for_running_generation(&lifecycle, 1);
    let first = connection
        .request_integration_validation()
        .unwrap()
        .wait(WAIT_TIMEOUT)
        .unwrap();
    assert_eq!(first.snapshot().condition(), IntegrationCondition::Starting);

    lifecycle.request_restart().unwrap();
    wait_for_running_generation(&lifecycle, 2);
    let restarted = connection
        .request_integration_validation()
        .unwrap()
        .wait(WAIT_TIMEOUT)
        .unwrap();
    assert_eq!(
        restarted.snapshot().condition(),
        IntegrationCondition::Starting
    );
    assert!(restarted.snapshot().is_coherent());

    lifecycle.request_shutdown().unwrap();
    process.join().unwrap();
}

#[test]
fn connection_admission_is_bounded_and_never_waits_for_capacity() {
    let lifecycle = ApplicationLifecycle::new().unwrap();
    let process = LinkedEngineProcess::launch(lifecycle.clone()).unwrap();
    let connection = process.connection();
    wait_for_running_generation(&lifecycle, 1);

    let mut pending = Vec::new();
    let saturated = loop {
        match connection.request_integration_validation() {
            Ok(response) => pending.push(response),
            Err(error) => break error,
        }
        assert!(
            pending.len() < 10_000,
            "bounded request queue never saturated"
        );
    };
    assert_eq!(saturated.category(), ErrorCategory::ResourceExhausted);

    for response in pending {
        let _ = response.wait(WAIT_TIMEOUT);
    }
    lifecycle.request_shutdown().unwrap();
    process.join().unwrap();
}

fn wait_for_running_generation(lifecycle: &ApplicationLifecycle, generation: u64) {
    let deadline = Instant::now() + WAIT_TIMEOUT;
    loop {
        let snapshot = lifecycle.snapshot();
        if snapshot.application_phase() == ApplicationLifecyclePhase::Running
            && snapshot.engine_generation() == generation
        {
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
