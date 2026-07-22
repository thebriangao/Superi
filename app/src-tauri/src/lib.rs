#![forbid(unsafe_code)]

//! Native Tauri ownership for the Superi desktop lifecycle.

pub mod capabilities;
pub mod crash_diagnostics;
pub mod desktop_shell;
pub mod engine;
pub mod file_associations;
pub mod lifecycle;
pub mod process_runtime;
pub mod project_lifecycle;
pub mod transport;
pub mod viewport;
pub mod window_session;

use std::time::Duration;

use crash_diagnostics::{
    DesktopCrashDiagnostics, DesktopCrashDiagnosticsSnapshot, DesktopProjectContinuity,
    DesktopWorkspaceContinuity,
};
use engine::{EngineConnection, LinkedEngineProcess};
use lifecycle::{
    ApplicationLifecycle, ApplicationLifecyclePhase, ApplicationLifecycleRequest,
    DesktopLifecycleSnapshot, LifecycleIntent,
};
use process_runtime::{DesktopProcessRuntime, DesktopProcessSnapshot};
use project_lifecycle::DesktopProjectState;
use serde::Serialize;
use superi_core::error::Error;
use tauri::{AppHandle, Builder, Emitter, Manager, RunEvent, Runtime, State};
use transport::{
    transport_task_error, DesktopTransportCommand, DesktopTransportReply, DesktopTransportState,
    DESKTOP_API_EVENT,
};

#[derive(Clone)]
pub(crate) struct ManagedLifecycle(pub(crate) ApplicationLifecycle);

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
fn desktop_process_snapshot(state: State<'_, DesktopProcessRuntime>) -> DesktopProcessSnapshot {
    state.snapshot()
}

#[tauri::command]
fn desktop_crash_diagnostics_snapshot(
    state: State<'_, DesktopCrashDiagnostics>,
) -> DesktopCrashDiagnosticsSnapshot {
    state.snapshot()
}

#[tauri::command]
async fn desktop_crash_workspace_update(
    workspace: DesktopWorkspaceContinuity,
    state: State<'_, DesktopCrashDiagnostics>,
) -> Result<DesktopCrashDiagnosticsSnapshot, LifecycleCommandError> {
    let diagnostics = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || diagnostics.update_workspace(workspace))
        .await
        .map_err(|_| crash_task_join_error("update_workspace"))?
        .map_err(Into::into)
}

#[tauri::command]
async fn desktop_crash_project_update(
    project: Option<DesktopProjectContinuity>,
    state: State<'_, DesktopCrashDiagnostics>,
) -> Result<DesktopCrashDiagnosticsSnapshot, LifecycleCommandError> {
    let diagnostics = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || diagnostics.update_project(project))
        .await
        .map_err(|_| crash_task_join_error("update_project"))?
        .map_err(Into::into)
}

#[tauri::command]
async fn desktop_crash_diagnostic_dismiss(
    diagnostic_id: String,
    state: State<'_, DesktopCrashDiagnostics>,
) -> Result<DesktopCrashDiagnosticsSnapshot, LifecycleCommandError> {
    let diagnostics = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || diagnostics.dismiss(&diagnostic_id))
        .await
        .map_err(|_| crash_task_join_error("dismiss"))?
        .map_err(Into::into)
}

fn crash_task_join_error(operation: &'static str) -> LifecycleCommandError {
    LifecycleCommandError {
        category: "internal".to_owned(),
        recoverability: "retryable".to_owned(),
        summary: format!("the crash diagnostic {operation} task could not be joined"),
    }
}

#[tauri::command]
async fn desktop_api_dispatch<R: Runtime>(
    app: AppHandle<R>,
    window: tauri::WebviewWindow<R>,
    command: DesktopTransportCommand,
    transport: State<'_, DesktopTransportState>,
    engine: State<'_, EngineConnection>,
    projects: State<'_, DesktopProjectState>,
) -> Result<DesktopTransportReply, superi_api::schema::PublicApiError> {
    let transport = transport.inner().clone();
    let engine = engine.inner().clone();
    let projects = projects.inner().clone();
    let client_id = window.label().to_owned();
    let worker_client_id = client_id.clone();
    let worker_transport = transport.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        worker_transport.dispatch_blocking_for(&worker_client_id, &engine, &projects, command)
    })
    .await
    .map_err(|_| transport_task_error("join_dispatch"))??;
    let (reply, events) = outcome.into_targeted_parts();
    for event in events {
        if app
            .emit_to(event.client_id(), DESKTOP_API_EVENT, event.envelope())
            .is_err()
        {
            let _ = transport.disconnect_client(event.client_id());
        }
    }
    Ok(reply)
}

/// Adds the application-owned lifecycle state and shell-local commands to a Tauri builder.
///
/// C002 supplies the headless-engine process participant through
/// [`ApplicationLifecycle::headless_engine_participant`] without changing this ownership.
pub fn configure<R: Runtime>(builder: Builder<R>, lifecycle: ApplicationLifecycle) -> Builder<R> {
    let runtime = DesktopProcessRuntime::new();
    let viewport = viewport::DesktopViewportState::with_runtime(runtime.clone());
    let windows = window_session::DesktopWindowState::with_runtime(runtime.clone());
    configure_with_owners(builder, lifecycle, runtime, viewport, windows)
}

fn configure_with_owners<R: Runtime>(
    builder: Builder<R>,
    lifecycle: ApplicationLifecycle,
    runtime: DesktopProcessRuntime,
    viewport: viewport::DesktopViewportState,
    windows: window_session::DesktopWindowState,
) -> Builder<R> {
    builder
        .plugin(tauri_plugin_dialog::init())
        .manage(ManagedLifecycle(lifecycle))
        .manage(capabilities::DesktopCapabilityState::default())
        .manage(runtime)
        .manage(desktop_shell::DesktopShellState::default())
        .manage(DesktopCrashDiagnostics::default())
        .manage(DesktopTransportState::new())
        .manage(DesktopProjectState::default())
        .manage(viewport)
        .manage(windows)
        .on_menu_event(desktop_shell::handle_menu_event)
        .invoke_handler(tauri::generate_handler![
            desktop_lifecycle_snapshot,
            desktop_lifecycle_request,
            desktop_process_snapshot,
            desktop_crash_diagnostics_snapshot,
            desktop_crash_workspace_update,
            desktop_crash_project_update,
            desktop_crash_diagnostic_dismiss,
            desktop_api_dispatch,
            capabilities::desktop_capabilities_discover,
            desktop_shell::desktop_shell_snapshot,
            desktop_shell::desktop_shell_sync,
            desktop_shell::desktop_shell_resolve_close,
            desktop_shell::desktop_shell_request_close,
            project_lifecycle::desktop_project_snapshot,
            project_lifecycle::desktop_project_execute,
            project_lifecycle::desktop_project_settings,
            project_lifecycle::desktop_project_settings_update,
            project_lifecycle::desktop_project_media_import,
            project_lifecycle::project_media_library,
            project_lifecycle::desktop_generate_media_preview,
            project_lifecycle::source_monitor::desktop_source_monitor_snapshot,
            project_lifecycle::source_monitor::desktop_source_monitor_load,
            project_lifecycle::source_monitor::desktop_source_monitor_seek,
            project_lifecycle::source_monitor::desktop_source_monitor_update_marks,
            project_lifecycle::source_monitor::desktop_source_monitor_unload,
            project_lifecycle::mutate_project_media_library,
            project_lifecycle::inspect_project_media_source,
            project_lifecycle::mutate_project_media_metadata,
            project_lifecycle::mutate_project_media_annotations,
            project_lifecycle::mutate_project_media_identity,
            project_lifecycle::mutate_project_media_content_analysis,
            project_lifecycle::search_project_media_content,
            project_lifecycle::mutate_project_derived_media,
            project_lifecycle::mutate_project_offline_media,
            project_lifecycle::scan_project_media_sources,
            project_lifecycle::mutate_project_media_batch,
            viewport::desktop_viewport_update,
            viewport::desktop_viewport_color_update,
            window_session::desktop_window_session_snapshot,
            window_session::desktop_window_create,
            window_session::desktop_window_focus,
            window_session::desktop_window_fullscreen,
            window_session::desktop_window_move_to_monitor,
            window_session::desktop_window_undo_placement,
            window_session::desktop_window_workspace_update,
            window_session::desktop_window_close,
            window_session::desktop_window_reopen
        ])
        .on_window_event(|window, event| {
            desktop_shell::handle_window_event(window, event);
            let state = window.state::<window_session::DesktopWindowState>();
            let transport = window.state::<DesktopTransportState>();
            state.handle_window_event(window, event, transport.inner());
        })
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
    let current_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let startup_project_paths =
        file_associations::project_paths_from_arguments(std::env::args_os().skip(1), &current_dir);
    let lifecycle = ApplicationLifecycle::new().expect("desktop lifecycle should initialize");
    let setup_lifecycle = lifecycle.clone();
    let event_lifecycle = lifecycle.clone();
    let shutdown_lifecycle = lifecycle.clone();
    let runtime = DesktopProcessRuntime::new();
    let setup_runtime = runtime.clone();
    let event_runtime = runtime.clone();
    let viewport_state = viewport::DesktopViewportState::with_runtime(runtime.clone());
    let window_state = window_session::DesktopWindowState::with_runtime(runtime.clone());
    let engine = LinkedEngineProcess::launch_with_runtime(lifecycle.clone(), runtime.clone())
        .expect("headless engine process should initialize");
    let app = configure_with_owners(
        tauri::Builder::default(),
        lifecycle,
        runtime.clone(),
        viewport_state.clone(),
        window_state.clone(),
    )
    .manage(engine.connection())
    .setup(move |app| {
        let app_data_dir = app.path().app_data_dir()?;
        let recovery_root = app_data_dir.join("recovery");
        let diagnostics = app.state::<DesktopCrashDiagnostics>().inner().clone();
        let _ = diagnostics.initialize(app_data_dir.join("crash-diagnostics"));
        diagnostics.install_panic_hook();
        app.state::<DesktopProjectState>()
            .initialize(recovery_root.clone())?;
        app.state::<desktop_shell::DesktopShellState>()
            .initialize(&recovery_root)?;
        app.state::<capabilities::DesktopCapabilityState>()
            .initialize(&recovery_root);
        let shell_snapshot = app.state::<desktop_shell::DesktopShellState>().snapshot()?;
        let menu = desktop_shell::build_menu(app.handle(), &shell_snapshot)?;
        app.set_menu(menu)?;
        app.state::<viewport::DesktopViewportState>()
            .initialize(app)?;
        app.state::<window_session::DesktopWindowState>()
            .initialize(app.handle().clone(), app.path().app_data_dir()?)?;
        file_associations::route_startup_project_files(
            app.handle(),
            startup_project_paths.clone(),
            &setup_runtime,
        );
        spawn_exit_monitor(
            app.handle().clone(),
            setup_lifecycle.clone(),
            diagnostics,
            &setup_runtime,
        )?;
        setup_runtime.finish_startup();
        Ok(())
    })
    .build(tauri::generate_context!());
    let app = match app {
        Ok(app) => app,
        Err(error) => {
            runtime.begin_shutdown();
            window_state.begin_shutdown();
            let _ = shutdown_lifecycle.request_shutdown();
            if let Err(cleanup_error) =
                join_process_owners(&runtime, &window_state, &viewport_state, engine)
            {
                eprintln!("desktop setup cleanup did not complete: {cleanup_error}");
            }
            panic!("Superi Tauri application should build: {error}");
        }
    };
    let diagnostics = app.state::<DesktopCrashDiagnostics>().inner().clone();
    let event_window_state = window_state.clone();

    app.run(move |handle, event| match event {
        RunEvent::ExitRequested { api, .. } => {
            let snapshot = event_lifecycle.snapshot();
            if snapshot.application_phase() != ApplicationLifecyclePhase::Stopped {
                api.prevent_exit();
                if snapshot.intent() == LifecycleIntent::Shutdown {
                    return;
                }
                let shell = handle.state::<desktop_shell::DesktopShellState>();
                if let Err(error) = desktop_shell::emit_close_request(
                    handle,
                    &shell,
                    desktop_shell::DesktopCloseReason::Quit,
                ) {
                    eprintln!("could not request safe Superi shutdown: {error}");
                }
            } else {
                event_window_state.begin_shutdown();
            }
        }
        #[cfg(target_os = "macos")]
        RunEvent::Opened { urls } => {
            file_associations::route_opened_project_urls(handle, urls, &event_runtime);
        }
        _ => {}
    });
    match join_process_owners(&runtime, &window_state, &viewport_state, engine) {
        Ok(()) => {
            let snapshot = shutdown_lifecycle.snapshot();
            if snapshot.application_phase() == ApplicationLifecyclePhase::Stopped
                && snapshot.intent() == LifecycleIntent::Shutdown
            {
                if let Err(error) = diagnostics.finish_session() {
                    runtime.fail_shutdown(error.message());
                    eprintln!("could not close the Superi crash session marker: {error}");
                }
            }
        }
        Err(error) => {
            eprintln!("desktop process did not stop cleanly: {error}");
        }
    }
}

fn spawn_exit_monitor<R: Runtime>(
    app: AppHandle<R>,
    lifecycle: ApplicationLifecycle,
    diagnostics: DesktopCrashDiagnostics,
    runtime: &DesktopProcessRuntime,
) -> Result<(), Error> {
    runtime.spawn_application_exit(move || loop {
        let snapshot = lifecycle.snapshot();
        if let Err(error) = diagnostics.observe_lifecycle(&snapshot) {
            eprintln!("could not observe the Superi lifecycle for crash recovery: {error}");
        }
        if snapshot.application_phase() == ApplicationLifecyclePhase::Stopped
            && snapshot.intent() == LifecycleIntent::Shutdown
        {
            app.state::<window_session::DesktopWindowState>()
                .begin_shutdown();
            app.exit(0);
            return Ok(());
        }
        if let Err(error) = lifecycle.wait_for_change(snapshot.revision(), Duration::from_secs(60))
        {
            eprintln!("could not continue observing desktop lifecycle shutdown: {error}");
            return Err("application lifecycle observation failed".to_owned());
        }
    })
}

fn join_process_owners(
    runtime: &DesktopProcessRuntime,
    windows: &window_session::DesktopWindowState,
    viewport: &viewport::DesktopViewportState,
    engine: LinkedEngineProcess,
) -> Result<(), Error> {
    runtime.begin_shutdown();
    windows.begin_shutdown();
    let mut first_error = None;
    retain_first_error(&mut first_error, runtime.join_application_exit());
    retain_first_error(&mut first_error, runtime.join_background_tasks());
    retain_first_error(&mut first_error, windows.shutdown_and_join());
    retain_first_error(&mut first_error, viewport.shutdown_and_join());
    retain_first_error(&mut first_error, engine.join());
    match first_error {
        Some(error) => {
            runtime.fail_shutdown(error.message());
            Err(error)
        }
        None => {
            runtime.finish_shutdown();
            Ok(())
        }
    }
}

fn retain_first_error(first_error: &mut Option<Error>, result: Result<(), Error>) {
    if let Err(error) = result {
        if first_error.is_none() {
            *first_error = Some(error);
        }
    }
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
