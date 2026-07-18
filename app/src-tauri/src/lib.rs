#![forbid(unsafe_code)]

//! Native Tauri ownership for the Superi desktop lifecycle.

pub mod engine;
pub mod lifecycle;
pub mod project_lifecycle;
pub mod transport;
pub mod viewport;

use std::thread;
use std::time::Duration;

use engine::{EngineConnection, LinkedEngineProcess};
use lifecycle::{
    ApplicationLifecycle, ApplicationLifecyclePhase, ApplicationLifecycleRequest,
    DesktopLifecycleSnapshot, LifecycleIntent,
};
use project_lifecycle::DesktopProjectState;
use serde::Serialize;
use superi_core::error::Error;
use tauri::{AppHandle, Builder, Emitter, Manager, RunEvent, Runtime, State};
use transport::{
    event_emission_error, transport_task_error, DesktopTransportCommand, DesktopTransportReply,
    DesktopTransportState, DESKTOP_API_EVENT,
};

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

#[tauri::command]
async fn desktop_api_dispatch<R: Runtime>(
    app: AppHandle<R>,
    command: DesktopTransportCommand,
    transport: State<'_, DesktopTransportState>,
    engine: State<'_, EngineConnection>,
) -> Result<DesktopTransportReply, superi_api::schema::PublicApiError> {
    let transport = transport.inner().clone();
    let engine = engine.inner().clone();
    let outcome =
        tauri::async_runtime::spawn_blocking(move || transport.dispatch_blocking(&engine, command))
            .await
            .map_err(|_| transport_task_error("join_dispatch"))??;
    let (reply, event) = outcome.into_parts();
    if let Some(event) = event {
        app.emit(DESKTOP_API_EVENT, event)
            .map_err(|_| event_emission_error())?;
    }
    Ok(reply)
}

/// Adds the application-owned lifecycle state and shell-local commands to a Tauri builder.
///
/// C002 supplies the headless-engine process participant through
/// [`ApplicationLifecycle::headless_engine_participant`] without changing this ownership.
pub fn configure<R: Runtime>(builder: Builder<R>, lifecycle: ApplicationLifecycle) -> Builder<R> {
    builder
        .manage(ManagedLifecycle(lifecycle))
        .manage(DesktopTransportState::new())
        .manage(DesktopProjectState::default())
        .manage(viewport::DesktopViewportState::default())
        .invoke_handler(tauri::generate_handler![
            desktop_lifecycle_snapshot,
            desktop_lifecycle_request,
            desktop_api_dispatch,
            project_lifecycle::desktop_project_snapshot,
            project_lifecycle::desktop_project_execute,
            viewport::desktop_viewport_update
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
    let engine = LinkedEngineProcess::launch(lifecycle.clone())
        .expect("headless engine process should initialize");
    let app = configure(tauri::Builder::default(), lifecycle)
        .manage(engine.connection())
        .setup(move |app| {
            app.state::<DesktopProjectState>()
                .initialize(app.path().app_data_dir()?.join("recovery"))?;
            app.state::<viewport::DesktopViewportState>()
                .initialize(app)?;
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
    if let Err(error) = engine.join() {
        eprintln!("headless engine process did not stop cleanly: {error}");
    }
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
    use super::{configure, ApplicationLifecycle, ManagedLifecycle};
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
}
