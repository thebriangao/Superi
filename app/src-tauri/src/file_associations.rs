//! Operating-system project file association ingress for the desktop shell.

use std::collections::HashSet;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, Url};

use crate::project_lifecycle::{
    DesktopProjectCommand, DesktopProjectFailure, DesktopProjectFailureClass,
    DesktopProjectSnapshot, DesktopProjectState,
};

pub const PROJECT_FILE_EXTENSION: &str = "superi";
pub const PROJECT_OPENED_EVENT: &str = "superi://project-opened";

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "snake_case")]
enum ProjectFileOpenSource {
    StartupArgument,
    OperatingSystem,
}

#[derive(Clone, Debug, Serialize)]
struct DesktopProjectOpenedEvent {
    source: ProjectFileOpenSource,
    path: String,
    snapshot: Option<DesktopProjectSnapshot>,
    failure: Option<DesktopProjectFailure>,
}

/// Selects unique project paths from process arguments in first-seen order.
pub fn project_paths_from_arguments<I>(arguments: I, current_dir: &Path) -> Vec<PathBuf>
where
    I: IntoIterator<Item = OsString>,
{
    unique_project_paths(arguments.into_iter().map(|argument| {
        let path = PathBuf::from(argument);
        if path.is_absolute() {
            path
        } else {
            current_dir.join(path)
        }
    }))
}

/// Selects unique local project paths from operating-system resource URLs.
pub fn project_paths_from_urls<I>(urls: I) -> Vec<PathBuf>
where
    I: IntoIterator<Item = Url>,
{
    unique_project_paths(urls.into_iter().filter_map(|url| url.to_file_path().ok()))
}

/// Opens one associated file through the sole durable desktop project owner.
pub fn open_project_file(
    state: &DesktopProjectState,
    path: &Path,
) -> Result<DesktopProjectSnapshot, DesktopProjectFailure> {
    if !is_project_path(path) {
        return Err(DesktopProjectFailure::new(
            DesktopProjectFailureClass::UserCorrectable,
            "project_file_type_invalid",
            "Project file type is invalid",
            "Choose a Superi project file.",
        )
        .with_context("operation", "open_associated_project"));
    }
    let path = path.to_str().ok_or_else(|| {
        DesktopProjectFailure::new(
            DesktopProjectFailureClass::UserCorrectable,
            "project_path_encoding_invalid",
            "Project path cannot be opened",
            "Move the project to a path supported by this platform and try again.",
        )
        .with_context("operation", "open_associated_project")
    })?;
    state.execute(DesktopProjectCommand::Open {
        path: path.to_owned(),
    })
}

/// Routes startup association paths after all managed desktop state is initialized.
pub fn route_startup_project_files<R: Runtime>(app: &AppHandle<R>, paths: Vec<PathBuf>) {
    route_project_files(app, paths, ProjectFileOpenSource::StartupArgument);
}

/// Routes macOS resource-open events through the same project lifecycle owner.
pub fn route_opened_project_urls<R: Runtime>(app: &AppHandle<R>, urls: Vec<Url>) {
    route_project_files(
        app,
        project_paths_from_urls(urls),
        ProjectFileOpenSource::OperatingSystem,
    );
}

fn unique_project_paths<I>(paths: I) -> Vec<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    let mut seen = HashSet::new();
    let mut selected = Vec::new();
    for path in paths {
        if !is_project_path(&path) {
            continue;
        }
        let normalized = std::fs::canonicalize(&path).unwrap_or(path);
        if seen.insert(normalized.clone()) {
            selected.push(normalized);
        }
    }
    selected
}

fn is_project_path(path: &Path) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case(PROJECT_FILE_EXTENSION))
}

fn route_project_files<R: Runtime>(
    app: &AppHandle<R>,
    paths: Vec<PathBuf>,
    source: ProjectFileOpenSource,
) {
    if paths.is_empty() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        for path in paths {
            let state = app.state::<DesktopProjectState>();
            let (snapshot, failure) = match open_project_file(&state, &path) {
                Ok(snapshot) => (Some(snapshot), None),
                Err(failure) => (state.snapshot().ok(), Some(failure)),
            };
            let event = DesktopProjectOpenedEvent {
                source,
                path: path.to_string_lossy().into_owned(),
                snapshot,
                failure,
            };
            if let Err(error) = app.emit(PROJECT_OPENED_EVENT, event) {
                eprintln!("could not emit associated project state: {error}");
            }
        }
        focus_main_window(&app);
    });
}

fn focus_main_window<R: Runtime>(app: &AppHandle<R>) {
    let main_thread = app.clone();
    if let Err(error) = app.run_on_main_thread(move || {
        if let Some(window) = main_thread.get_webview_window("main") {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
        }
    }) {
        eprintln!("could not focus the associated project window: {error}");
    }
}
