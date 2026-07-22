//! Operating-system project file association ingress for the desktop shell.

use std::collections::{HashSet, VecDeque};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State, Url};

use crate::process_runtime::DesktopProcessRuntime;
use crate::project_lifecycle::{DesktopProjectFailure, DesktopProjectFailureClass};

pub const PROJECT_FILE_EXTENSION: &str = "superi";
pub const PROJECT_OPEN_REQUESTED_EVENT: &str = "superi://project-open-requested";
const MAX_PENDING_PROJECT_OPENS: usize = 32;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectFileOpenSource {
    StartupArgument,
    OperatingSystem,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopProjectOpenRequest {
    sequence: u64,
    source: ProjectFileOpenSource,
    path: String,
}

impl DesktopProjectOpenRequest {
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn source(&self) -> ProjectFileOpenSource {
        self.source
    }

    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }
}

#[derive(Default)]
struct ProjectAssociationQueue {
    next_sequence: u64,
    pending: VecDeque<DesktopProjectOpenRequest>,
}

/// Process-local queue that retains operating-system open intent until React safely resolves it.
#[derive(Clone, Default)]
pub struct DesktopProjectAssociationState {
    queue: Arc<Mutex<ProjectAssociationQueue>>,
}

impl DesktopProjectAssociationState {
    pub fn enqueue(
        &self,
        paths: Vec<PathBuf>,
        source: ProjectFileOpenSource,
    ) -> Result<Vec<DesktopProjectOpenRequest>, DesktopProjectFailure> {
        let paths = unique_project_paths(paths)
            .into_iter()
            .map(|path| {
                path.to_str().map(str::to_owned).ok_or_else(|| {
                    DesktopProjectFailure::new(
                        DesktopProjectFailureClass::UserCorrectable,
                        "project_path_encoding_invalid",
                        "Project path cannot be opened",
                        "Move the project to a path supported by this platform and try again.",
                    )
                    .with_context("operation", "queue_associated_project")
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let mut queue = self.lock()?;
        let pending_paths = queue
            .pending
            .iter()
            .map(|request| request.path.clone())
            .collect::<HashSet<_>>();
        let paths = paths
            .into_iter()
            .filter(|path| !pending_paths.contains(path))
            .collect::<Vec<_>>();
        if queue.pending.len().saturating_add(paths.len()) > MAX_PENDING_PROJECT_OPENS {
            return Err(DesktopProjectFailure::new(
                DesktopProjectFailureClass::Retryable,
                "project_open_queue_full",
                "Too many project open requests are pending",
                "Resolve the current project prompt before opening another project.",
            )
            .with_context("operation", "queue_associated_project"));
        }

        let mut requests = Vec::with_capacity(paths.len());
        for path in paths {
            queue.next_sequence = queue.next_sequence.checked_add(1).ok_or_else(|| {
                DesktopProjectFailure::new(
                    DesktopProjectFailureClass::Terminal,
                    "project_open_sequence_exhausted",
                    "Project open requests cannot continue",
                    "Restart Superi before opening another project.",
                )
                .with_context("operation", "queue_associated_project")
            })?;
            let request = DesktopProjectOpenRequest {
                sequence: queue.next_sequence,
                source,
                path,
            };
            queue.pending.push_back(request.clone());
            requests.push(request);
        }
        Ok(requests)
    }

    pub fn pending(&self) -> Result<Vec<DesktopProjectOpenRequest>, DesktopProjectFailure> {
        Ok(self.lock()?.pending.iter().cloned().collect())
    }

    pub fn resolve(&self, sequence: u64) -> Result<bool, DesktopProjectFailure> {
        let mut queue = self.lock()?;
        let Some(index) = queue
            .pending
            .iter()
            .position(|request| request.sequence == sequence)
        else {
            return Ok(false);
        };
        queue.pending.remove(index);
        Ok(true)
    }

    fn lock(&self) -> Result<MutexGuard<'_, ProjectAssociationQueue>, DesktopProjectFailure> {
        self.queue.lock().map_err(|_| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Retryable,
                "project_open_queue_unavailable",
                "Project open requests are temporarily unavailable",
                "Retry the project open request.",
            )
            .with_context("operation", "project_open_queue")
        })
    }
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

/// Routes startup association paths after all managed desktop state is initialized.
pub fn route_startup_project_files<R: Runtime>(
    app: &AppHandle<R>,
    paths: Vec<PathBuf>,
    runtime: &DesktopProcessRuntime,
) {
    route_project_files(app, paths, ProjectFileOpenSource::StartupArgument, runtime);
}

/// Queues macOS resource-open events for the existing safe React project owner.
pub fn route_opened_project_urls<R: Runtime>(
    app: &AppHandle<R>,
    urls: Vec<Url>,
    runtime: &DesktopProcessRuntime,
) {
    route_project_files(
        app,
        project_paths_from_urls(urls),
        ProjectFileOpenSource::OperatingSystem,
        runtime,
    );
}

#[tauri::command]
pub fn desktop_project_open_requests(
    state: State<'_, DesktopProjectAssociationState>,
) -> Result<Vec<DesktopProjectOpenRequest>, DesktopProjectFailure> {
    state.pending()
}

#[tauri::command]
pub fn desktop_project_open_request_resolve(
    sequence: u64,
    state: State<'_, DesktopProjectAssociationState>,
) -> Result<bool, DesktopProjectFailure> {
    state.resolve(sequence)
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
    runtime: &DesktopProcessRuntime,
) {
    if paths.is_empty() {
        return;
    }
    let app = app.clone();
    let rejection_app = app.clone();
    if let Err(error) = runtime.spawn_background_task(move || {
        let state = app.state::<DesktopProjectAssociationState>();
        match state.enqueue(paths, source) {
            Ok(requests) => {
                for request in requests {
                    if let Err(error) = app.emit(PROJECT_OPEN_REQUESTED_EVENT, request) {
                        eprintln!("could not emit associated project request: {error}");
                    }
                }
            }
            Err(error) => eprintln!("could not queue associated project request: {error}"),
        }
        focus_main_window(&app);
    }) {
        eprintln!("could not retain associated project request task: {error}");
        focus_main_window(&rejection_app);
    }
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
