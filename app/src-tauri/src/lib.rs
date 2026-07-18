#![forbid(unsafe_code)]

//! Native Tauri ownership for the Superi desktop lifecycle.

pub mod lifecycle;

use std::thread;
use std::time::Duration;

use lifecycle::{
    ApplicationLifecycle, ApplicationLifecyclePhase, ApplicationLifecycleRequest,
    DesktopLifecycleSnapshot, HeadlessEngineFailure, LifecycleIntent,
};
use serde::Serialize;
use superi_concurrency::lifecycle::LifecyclePhase;
use superi_core::error::{Error, ErrorCategory, Recoverability};
use tauri::{AppHandle, Builder, RunEvent, Runtime, State};

#[derive(Clone)]
struct ManagedLifecycle(ApplicationLifecycle);

#[derive(Debug, Serialize)]
struct LifecycleCommandError {
    category: String,
    recoverability: String,
    summary: String,
}

impl From<Error> for LifecycleCommandError {
    fn from(error: Error) -> Self {
        Self {
            category: error.category().code().to_owned(),
            recoverability: error.recoverability().code().to_owned(),
            summary: error.message().to_owned(),
        }
    }
}

#[tauri::command]
fn desktop_lifecycle_snapshot(state: State<'_, ManagedLifecycle>) -> DesktopLifecycleSnapshot {
    state.0.snapshot()
}

#[tauri::command]
fn desktop_lifecycle_request(
    request: ApplicationLifecycleRequest,
    state: State<'_, ManagedLifecycle>,
) -> Result<DesktopLifecycleSnapshot, LifecycleCommandError> {
    match request {
        ApplicationLifecycleRequest::Recover => state.0.request_recovery(),
        ApplicationLifecycleRequest::Restart => state.0.request_restart(),
        ApplicationLifecycleRequest::Shutdown => state.0.request_shutdown(),
    }
    .map_err(Into::into)
}

/// Adds the application-owned lifecycle state and shell-local commands to a Tauri builder.
///
/// C002 supplies the headless-engine process participant through
/// [`ApplicationLifecycle::headless_engine_participant`] without changing this ownership.
pub fn configure<R: Runtime>(builder: Builder<R>, lifecycle: ApplicationLifecycle) -> Builder<R> {
    builder
        .manage(ManagedLifecycle(lifecycle))
        .invoke_handler(tauri::generate_handler![
            desktop_lifecycle_snapshot,
            desktop_lifecycle_request
        ])
}

/// Constructs the real native builder used by hosted desktop compilation.
pub fn native_builder() -> Result<Builder<tauri::Wry>, Error> {
    Ok(configure(
        tauri::Builder::default(),
        ApplicationLifecycle::new()?,
    ))
}

/// Runs the native desktop process and performs only nonblocking work on Tauri callbacks.
pub fn run() {
    let lifecycle = ApplicationLifecycle::new().expect("desktop lifecycle should initialize");
    let setup_lifecycle = lifecycle.clone();
    let event_lifecycle = lifecycle.clone();
    let app = configure(tauri::Builder::default(), lifecycle)
        .setup(move |app| {
            spawn_unattached_headless_engine(setup_lifecycle.clone());
            spawn_exit_monitor(app.handle().clone(), setup_lifecycle.clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Superi Tauri application should build");

    app.run(move |_handle, event| {
        if let RunEvent::ExitRequested { api, .. } = event {
            let snapshot = event_lifecycle.snapshot();
            if snapshot.application_phase() != ApplicationLifecyclePhase::Stopped {
                api.prevent_exit();
                if let Err(error) = event_lifecycle.request_shutdown() {
                    eprintln!("could not begin orderly Superi shutdown: {error}");
                }
            }
        }
    });
}

fn spawn_unattached_headless_engine(lifecycle: ApplicationLifecycle) {
    thread::Builder::new()
        .name("superi-unattached-engine".to_owned())
        .spawn(move || {
            let Ok(engine) = lifecycle.headless_engine_participant() else {
                return;
            };
            loop {
                let observed = engine.signal().load();
                match observed.phase() {
                    LifecyclePhase::Starting => {
                        let _ = engine.fail(
                            HeadlessEngineFailure::new(
                                ErrorCategory::Unavailable,
                                Recoverability::Retryable,
                                "The headless engine connection is not available in this build.",
                            )
                            .with_context("superi-desktop.engine", "connect"),
                        );
                    }
                    LifecyclePhase::Stopping => {
                        if let Ok(snapshot) = engine.acknowledge(observed) {
                            if snapshot.application_phase() == ApplicationLifecyclePhase::Stopped
                                && snapshot.intent() == LifecycleIntent::Shutdown
                            {
                                return;
                            }
                            continue;
                        }
                    }
                    LifecyclePhase::Stopped => return,
                    _ => {}
                }

                let revision = lifecycle.snapshot().revision();
                if lifecycle
                    .wait_for_change(revision, Duration::from_secs(60))
                    .is_err()
                {
                    return;
                }
            }
        })
        .expect("unattached engine lifecycle thread should start");
}

fn spawn_exit_monitor<R: Runtime>(app: AppHandle<R>, lifecycle: ApplicationLifecycle) {
    thread::Builder::new()
        .name("superi-application-exit".to_owned())
        .spawn(move || loop {
            let snapshot = lifecycle.snapshot();
            if snapshot.application_phase() == ApplicationLifecyclePhase::Stopped
                && snapshot.intent() == LifecycleIntent::Shutdown
            {
                app.exit(0);
                return;
            }
            if lifecycle
                .wait_for_change(snapshot.revision(), Duration::from_secs(60))
                .is_err()
            {
                return;
            }
        })
        .expect("application exit monitor should start");
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{
        configure, spawn_unattached_headless_engine, ApplicationLifecycle,
        ApplicationLifecyclePhase, ManagedLifecycle,
    };
    use tauri::Manager;

    #[test]
    fn mock_runtime_builds_with_managed_lifecycle_and_commands() {
        let lifecycle = ApplicationLifecycle::new().unwrap();
        let app = configure(tauri::test::mock_builder(), lifecycle)
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock Superi application should build");

        assert_eq!(
            app.state::<ManagedLifecycle>()
                .0
                .snapshot()
                .engine_generation(),
            1
        );
    }

    #[test]
    fn unattached_engine_recovery_observes_the_new_start_without_poll_delay() {
        let lifecycle = ApplicationLifecycle::new().unwrap();
        spawn_unattached_headless_engine(lifecycle.clone());

        wait_for_failed_generation(&lifecycle, 1);
        lifecycle.request_recovery().unwrap();
        wait_for_failed_generation(&lifecycle, 2);
        lifecycle.request_shutdown().unwrap();
    }

    fn wait_for_failed_generation(lifecycle: &ApplicationLifecycle, generation: u64) {
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            let snapshot = lifecycle.snapshot();
            if snapshot.application_phase() == ApplicationLifecyclePhase::Failed
                && snapshot.engine_generation() == generation
            {
                return;
            }
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .expect("unattached engine lifecycle transition timed out");
            lifecycle
                .wait_for_change(snapshot.revision(), remaining)
                .expect("unattached engine lifecycle wait should succeed");
        }
    }
}
