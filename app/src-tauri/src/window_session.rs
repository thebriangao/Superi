//! Persistent native editor-window session ownership.

use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use tauri::window::Monitor;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, Runtime, State, WebviewUrl,
    WebviewWindowBuilder, Window, WindowEvent,
};

use crate::process_runtime::{
    DesktopProcessRuntime, DesktopProcessServiceId, DesktopProcessServicePhase,
};
use crate::transport::DesktopTransportState;

pub const DESKTOP_WINDOW_EVENT: &str = "superi://window-session-changed";
pub const SESSION_SCHEMA_VERSION: u32 = 1;

const COMPONENT: &str = "superi.desktop.window_session";
const PRIMARY_WINDOW_LABEL: &str = "main";
const NATIVE_VIEWPORT_OWNER: &str = PRIMARY_WINDOW_LABEL;
const SESSION_FILE_NAME: &str = "window-session-v1.json";
const MAX_EDITOR_WINDOWS: usize = 8;
const MAX_RECENTLY_CLOSED: usize = 8;
const MIN_WINDOW_WIDTH: u32 = 720;
const MIN_WINDOW_HEIGHT: u32 = 480;
const DEFAULT_WINDOW_WIDTH: u32 = 1_100;
const DEFAULT_WINDOW_HEIGHT: u32 = 720;
const PERSISTENCE_COALESCE: Duration = Duration::from_millis(120);
const WORKSPACES: &[&str] = &[
    "editing",
    "compositing",
    "color",
    "audio",
    "delivery",
    "system",
];

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedPlacement {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    fullscreen: bool,
    monitor_id: Option<String>,
}

impl PersistedPlacement {
    fn default_primary() -> Self {
        Self {
            x: 100,
            y: 100,
            width: DEFAULT_WINDOW_WIDTH,
            height: DEFAULT_WINDOW_HEIGHT,
            fullscreen: false,
            monitor_id: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedWindow {
    label: String,
    title: String,
    workspace: String,
    placement: PersistedPlacement,
    previous_placement: Option<PersistedPlacement>,
}

impl PersistedWindow {
    fn new(
        label: impl Into<String>,
        title: impl Into<String>,
        workspace: impl Into<String>,
        placement: PersistedPlacement,
    ) -> Self {
        Self {
            label: label.into(),
            title: title.into(),
            workspace: workspace.into(),
            placement,
            previous_placement: None,
        }
    }

    fn primary() -> Self {
        Self::new(
            PRIMARY_WINDOW_LABEL,
            "Superi",
            "editing",
            PersistedPlacement::default_primary(),
        )
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct PersistedWindowSession {
    schema_version: u32,
    next_window_id: u64,
    last_focused_label: Option<String>,
    windows: Vec<PersistedWindow>,
    recently_closed: Vec<PersistedWindow>,
}

impl Default for PersistedWindowSession {
    fn default() -> Self {
        Self {
            schema_version: SESSION_SCHEMA_VERSION,
            next_window_id: 1,
            last_focused_label: Some(PRIMARY_WINDOW_LABEL.to_owned()),
            windows: vec![PersistedWindow::primary()],
            recently_closed: Vec::new(),
        }
    }
}

struct LoadedPersistedSession {
    session: PersistedWindowSession,
    recovery_note: Option<String>,
}

fn normalize_persisted_session(
    mut session: PersistedWindowSession,
) -> Result<PersistedWindowSession> {
    if session.schema_version != SESSION_SCHEMA_VERSION {
        return Err(invalid(
            "normalize_session",
            "the window session schema version is unsupported",
        ));
    }
    if session.windows.is_empty() || session.windows.len() > MAX_EDITOR_WINDOWS {
        return Err(invalid(
            "normalize_session",
            "the window session contains an invalid active window count",
        ));
    }
    if session.recently_closed.len() > MAX_RECENTLY_CLOSED {
        return Err(invalid(
            "normalize_session",
            "the window session contains too many recently closed windows",
        ));
    }

    let mut active_labels = HashSet::new();
    let mut primary_count = 0;
    let mut largest_window_id = 0_u64;
    for window in &session.windows {
        validate_persisted_window(window)?;
        if !active_labels.insert(window.label.clone()) {
            return Err(invalid(
                "normalize_session",
                "the window session contains a duplicate active label",
            ));
        }
        if window.label == PRIMARY_WINDOW_LABEL {
            primary_count += 1;
        } else if let Some(id) = auxiliary_window_id(&window.label) {
            largest_window_id = largest_window_id.max(id);
        }
    }
    if primary_count != 1 {
        return Err(invalid(
            "normalize_session",
            "the window session must contain exactly one primary window",
        ));
    }

    let mut closed_labels = HashSet::new();
    for window in &session.recently_closed {
        validate_persisted_window(window)?;
        if window.label == PRIMARY_WINDOW_LABEL
            || active_labels.contains(&window.label)
            || !closed_labels.insert(window.label.clone())
        {
            return Err(invalid(
                "normalize_session",
                "the recently closed window identities are invalid",
            ));
        }
        largest_window_id = largest_window_id.max(auxiliary_window_id(&window.label).unwrap_or(0));
    }

    session.windows.sort_by(|left, right| {
        let left_id = auxiliary_window_id(&left.label).unwrap_or(0);
        let right_id = auxiliary_window_id(&right.label).unwrap_or(0);
        left_id
            .cmp(&right_id)
            .then_with(|| left.label.cmp(&right.label))
    });
    session.next_window_id = session
        .next_window_id
        .max(largest_window_id.saturating_add(1))
        .max(1);
    if !session
        .last_focused_label
        .as_ref()
        .is_some_and(|label| active_labels.contains(label))
    {
        session.last_focused_label = Some(PRIMARY_WINDOW_LABEL.to_owned());
    }
    Ok(session)
}

fn validate_persisted_window(window: &PersistedWindow) -> Result<()> {
    if !is_editor_window_label(&window.label)
        || window.title.trim().is_empty()
        || window.title.len() > 256
        || !is_workspace(&window.workspace)
    {
        return Err(invalid(
            "validate_window",
            "the persisted window identity, title, or workspace is invalid",
        ));
    }
    validate_placement(&window.placement)?;
    if let Some(previous) = &window.previous_placement {
        validate_placement(previous)?;
    }
    Ok(())
}

fn validate_placement(placement: &PersistedPlacement) -> Result<()> {
    if placement.width < MIN_WINDOW_WIDTH
        || placement.height < MIN_WINDOW_HEIGHT
        || placement.width > 32_768
        || placement.height > 32_768
        || placement
            .monitor_id
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.len() > 128 || !id.is_ascii())
    {
        return Err(invalid(
            "validate_placement",
            "the persisted window placement is outside supported bounds",
        ));
    }
    Ok(())
}

fn load_persisted_session(path: &Path) -> LoadedPersistedSession {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return LoadedPersistedSession {
                session: PersistedWindowSession::default(),
                recovery_note: None,
            };
        }
        Err(error) => {
            return LoadedPersistedSession {
                session: PersistedWindowSession::default(),
                recovery_note: Some(format!(
                    "Window restoration recovered to a safe primary because the saved session could not be read: {error}"
                )),
            };
        }
    };
    let decoded = serde_json::from_slice::<PersistedWindowSession>(&bytes)
        .map_err(|error| error.to_string())
        .and_then(|session| {
            normalize_persisted_session(session).map_err(|error| error.to_string())
        });
    match decoded {
        Ok(session) => LoadedPersistedSession {
            session,
            recovery_note: None,
        },
        Err(reason) => {
            let rejected = rejected_session_path(path);
            let note = match fs::rename(path, &rejected) {
                Ok(()) => format!(
                    "Window restoration recovered to a safe primary and preserved the rejected session at {}: {reason}",
                    rejected.display()
                ),
                Err(rename_error) => format!(
                    "Window restoration recovered to a safe primary, but the rejected session could not be preserved: {reason}; {rename_error}"
                ),
            };
            LoadedPersistedSession {
                session: PersistedWindowSession::default(),
                recovery_note: Some(note),
            }
        }
    }
}

fn rejected_session_path(path: &Path) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    path.with_file_name(format!(
        "window-session-v1.rejected-{}-{timestamp}.json",
        std::process::id()
    ))
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopMonitorTarget {
    id: String,
    name: String,
    position_x: i32,
    position_y: i32,
    physical_width: u32,
    physical_height: u32,
    scale_factor: f64,
    primary: bool,
}

impl DesktopMonitorTarget {
    fn from_monitor(monitor: &Monitor, primary_id: Option<&str>) -> Result<Self> {
        if monitor.size().width == 0
            || monitor.size().height == 0
            || !monitor.scale_factor().is_finite()
            || monitor.scale_factor() <= 0.0
        {
            return Err(unavailable(
                "describe_monitor",
                "the active monitor reported invalid geometry",
            ));
        }
        let id = monitor_routing_id(monitor);
        Ok(Self {
            name: monitor
                .name()
                .map(String::as_str)
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    format!(
                        "Display at {},{}",
                        monitor.position().x,
                        monitor.position().y
                    )
                }),
            position_x: monitor.position().x,
            position_y: monitor.position().y,
            physical_width: monitor.size().width,
            physical_height: monitor.size().height,
            scale_factor: monitor.scale_factor(),
            primary: primary_id.is_some_and(|primary| primary == id),
            id,
        })
    }
}

fn monitor_routing_id(monitor: &Monitor) -> String {
    let identity = format!(
        "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}\u{0}{:016x}",
        monitor.name().map_or("", String::as_str),
        monitor.position().x,
        monitor.position().y,
        monitor.size().width,
        monitor.size().height,
        monitor.scale_factor().to_bits(),
    );
    let digest = Sha256::digest(identity.as_bytes());
    let hexadecimal = digest
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("tauri-monitor:{hexadecimal}")
}

fn move_placement_to_monitor(
    current: &PersistedPlacement,
    source: Option<&DesktopMonitorTarget>,
    target: &DesktopMonitorTarget,
) -> PersistedPlacement {
    let width = current.physical_width_on(target);
    let height = current.physical_height_on(target);
    let target_available_x = target.physical_width.saturating_sub(width);
    let target_available_y = target.physical_height.saturating_sub(height);
    let (x_fraction, y_fraction) = source.map_or((0.5, 0.5), |source| {
        let source_available_x = source.physical_width.saturating_sub(current.width);
        let source_available_y = source.physical_height.saturating_sub(current.height);
        (
            relative_fraction(current.x, source.position_x, source_available_x),
            relative_fraction(current.y, source.position_y, source_available_y),
        )
    });
    PersistedPlacement {
        x: target
            .position_x
            .saturating_add((x_fraction * f64::from(target_available_x)).round() as i32),
        y: target
            .position_y
            .saturating_add((y_fraction * f64::from(target_available_y)).round() as i32),
        width,
        height,
        fullscreen: current.fullscreen,
        monitor_id: Some(target.id.clone()),
    }
}

impl PersistedPlacement {
    fn physical_width_on(&self, target: &DesktopMonitorTarget) -> u32 {
        self.width
            .max(MIN_WINDOW_WIDTH.min(target.physical_width))
            .min(target.physical_width)
    }

    fn physical_height_on(&self, target: &DesktopMonitorTarget) -> u32 {
        self.height
            .max(MIN_WINDOW_HEIGHT.min(target.physical_height))
            .min(target.physical_height)
    }
}

fn relative_fraction(value: i32, origin: i32, available: u32) -> f64 {
    if available == 0 {
        return 0.5;
    }
    (f64::from(value.saturating_sub(origin)) / f64::from(available)).clamp(0.0, 1.0)
}

fn clamp_placement_to_monitor(
    current: &PersistedPlacement,
    target: &DesktopMonitorTarget,
) -> PersistedPlacement {
    let width = current.physical_width_on(target);
    let height = current.physical_height_on(target);
    let max_x = target.position_x.saturating_add(
        i32::try_from(target.physical_width.saturating_sub(width)).unwrap_or(i32::MAX),
    );
    let max_y = target.position_y.saturating_add(
        i32::try_from(target.physical_height.saturating_sub(height)).unwrap_or(i32::MAX),
    );
    PersistedPlacement {
        x: current.x.clamp(target.position_x, max_x),
        y: current.y.clamp(target.position_y, max_y),
        width,
        height,
        fullscreen: current.fullscreen,
        monitor_id: Some(target.id.clone()),
    }
}

fn swap_reversible_placement(
    current: PersistedPlacement,
    previous: PersistedPlacement,
) -> (PersistedPlacement, PersistedPlacement) {
    (previous, current)
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopWindowFailure {
    category: String,
    recoverability: String,
    summary: String,
}

impl From<&Error> for DesktopWindowFailure {
    fn from(error: &Error) -> Self {
        Self {
            category: error.category().code().to_owned(),
            recoverability: error.recoverability().code().to_owned(),
            summary: error.message().to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopWindowRecord {
    label: String,
    title: String,
    workspace: String,
    primary: bool,
    focused: bool,
    fullscreen: bool,
    monitor_id: Option<String>,
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    can_undo_placement: bool,
    can_close: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DesktopWindowSnapshot {
    schema_version: u32,
    revision: u64,
    phase: String,
    persistence_phase: String,
    native_viewport_owner: String,
    last_focused_label: Option<String>,
    recovery_note: Option<String>,
    failure: Option<DesktopWindowFailure>,
    recently_closed_count: usize,
    monitors: Vec<DesktopMonitorTarget>,
    windows: Vec<DesktopWindowRecord>,
}

struct DesktopWindowInner {
    session: PersistedWindowSession,
    revision: u64,
    phase: String,
    persistence_phase: String,
    restoration_complete: bool,
    focused_label: Option<String>,
    recovery_note: Option<String>,
    failure: Option<DesktopWindowFailure>,
    monitors: Vec<DesktopMonitorTarget>,
    shutting_down: bool,
}

impl Default for DesktopWindowInner {
    fn default() -> Self {
        Self {
            session: PersistedWindowSession::default(),
            revision: 0,
            phase: "loading".to_owned(),
            persistence_phase: "loading".to_owned(),
            restoration_complete: false,
            focused_label: None,
            recovery_note: None,
            failure: None,
            monitors: Vec::new(),
            shutting_down: false,
        }
    }
}

enum PersistenceSignal {
    Wake,
    Shutdown,
}

struct PersistenceControl {
    sender: SyncSender<PersistenceSignal>,
    worker: JoinHandle<()>,
}

/// Process-owned native editor-window state and persistence coordination.
#[derive(Clone)]
pub struct DesktopWindowState {
    inner: Arc<Mutex<DesktopWindowInner>>,
    persistence: Arc<Mutex<Option<PersistenceControl>>>,
    runtime: DesktopProcessRuntime,
}

impl Default for DesktopWindowState {
    fn default() -> Self {
        Self::with_runtime(DesktopProcessRuntime::new())
    }
}

impl DesktopWindowState {
    /// Creates native window state attached to the shared desktop process owner.
    #[must_use]
    pub fn with_runtime(runtime: DesktopProcessRuntime) -> Self {
        Self {
            inner: Arc::new(Mutex::new(DesktopWindowInner::default())),
            persistence: Arc::new(Mutex::new(None)),
            runtime,
        }
    }

    /// Starts the bounded persistence worker and restores saved windows asynchronously.
    pub fn initialize<R: Runtime>(&self, app: AppHandle<R>, data_dir: PathBuf) -> Result<()> {
        let mut persistence = self.lock_persistence("initialize")?;
        if persistence.is_some() {
            return Err(conflict(
                "initialize",
                "the desktop window session is already initialized",
            ));
        }
        let path = data_dir.join(SESSION_FILE_NAME);
        let (sender, receiver) = mpsc::sync_channel(1);
        let state = self.clone();
        self.runtime.update_service(
            DesktopProcessServiceId::WindowPersistence,
            DesktopProcessServicePhase::Starting,
            1,
            1,
            true,
            vec!["superi-window-persistence".to_owned()],
            "Starting window session persistence",
        );
        let worker_runtime = self.runtime.clone();
        let worker = thread::Builder::new()
            .name("superi-window-persistence".to_owned())
            .spawn(move || {
                worker_runtime.update_service(
                    DesktopProcessServiceId::WindowPersistence,
                    DesktopProcessServicePhase::Running,
                    1,
                    1,
                    true,
                    vec!["superi-window-persistence".to_owned()],
                    "Window session persistence is running",
                );
                persistence_loop(state, app, path, receiver);
                worker_runtime.update_service(
                    DesktopProcessServiceId::WindowPersistence,
                    DesktopProcessServicePhase::Stopped,
                    1,
                    0,
                    true,
                    vec!["superi-window-persistence".to_owned()],
                    "Window session persistence finished and awaits join",
                );
            })
            .map_err(|source| {
                self.runtime.update_service(
                    DesktopProcessServiceId::WindowPersistence,
                    DesktopProcessServicePhase::Failed,
                    0,
                    0,
                    false,
                    vec!["superi-window-persistence".to_owned()],
                    "Window session persistence could not start",
                );
                Error::with_source(
                    ErrorCategory::Unavailable,
                    Recoverability::Retryable,
                    "the window persistence worker could not start",
                    source,
                )
                .with_context(ErrorContext::new(COMPONENT, "initialize"))
            })?;
        *persistence = Some(PersistenceControl { sender, worker });
        self.runtime.update_service(
            DesktopProcessServiceId::WindowPersistence,
            DesktopProcessServicePhase::Running,
            1,
            1,
            true,
            vec!["superi-window-persistence".to_owned()],
            "Window session persistence is running",
        );
        Ok(())
    }

    /// Captures one relevant native event without file I/O or blocking waits.
    pub fn handle_window_event<R: Runtime>(
        &self,
        window: &Window<R>,
        event: &WindowEvent,
        transport: &DesktopTransportState,
    ) {
        let label = window.label();
        if !is_editor_window_label(label) {
            return;
        }
        let result = match event {
            WindowEvent::CloseRequested { .. } if label == PRIMARY_WINDOW_LABEL => {
                self.capture_window(window)
            }
            WindowEvent::CloseRequested { .. } => self
                .capture_window(window)
                .and_then(|_| self.close_record(label))
                .and_then(|_| {
                    transport
                        .disconnect_client(label)
                        .map_err(public_transport_error)
                }),
            WindowEvent::Destroyed if label != PRIMARY_WINDOW_LABEL => {
                self.close_record(label).and_then(|_| {
                    transport
                        .disconnect_client(label)
                        .map_err(public_transport_error)
                })
            }
            WindowEvent::Moved(_)
            | WindowEvent::Resized(_)
            | WindowEvent::ScaleFactorChanged { .. }
            | WindowEvent::Focused(_) => self.capture_window(window),
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.record_failure(&error);
        }
        self.signal_persist();
        self.emit_snapshot(window.app_handle());
    }

    /// Marks orderly application shutdown and requests one final background flush.
    pub fn begin_shutdown(&self) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.shutting_down = true;
            inner.phase = "shutting_down".to_owned();
            bump_revision(&mut inner);
        }
        self.signal_persist();
    }

    /// Flushes and joins the persistence worker after the native event loop returns.
    pub fn shutdown_and_join(&self) -> Result<()> {
        let control = self.lock_persistence("shutdown_and_join")?.take();
        let mut first_error = None;
        if let Some(control) = control {
            self.runtime.update_service(
                DesktopProcessServiceId::WindowPersistence,
                DesktopProcessServicePhase::Stopping,
                1,
                1,
                true,
                vec!["superi-window-persistence".to_owned()],
                "Flushing and joining window session persistence",
            );
            if control.sender.send(PersistenceSignal::Shutdown).is_err() {
                first_error = Some(
                    Error::new(
                        ErrorCategory::Unavailable,
                        Recoverability::Retryable,
                        "the window persistence shutdown signal could not be delivered",
                    )
                    .with_context(ErrorContext::new(COMPONENT, "shutdown_and_join")),
                );
            }
            if control.worker.join().is_err() && first_error.is_none() {
                first_error = Some(
                    Error::new(
                        ErrorCategory::Internal,
                        Recoverability::Terminal,
                        "the window persistence worker panicked during shutdown",
                    )
                    .with_context(ErrorContext::new(COMPONENT, "shutdown_and_join")),
                );
            }
        }
        if let Ok(mut inner) = self.inner.lock() {
            inner.persistence_phase = if first_error.is_some() {
                "failed".to_owned()
            } else {
                "stopped".to_owned()
            };
        }
        match first_error {
            Some(error) => {
                self.runtime.update_service(
                    DesktopProcessServiceId::WindowPersistence,
                    DesktopProcessServicePhase::Failed,
                    0,
                    0,
                    false,
                    vec!["superi-window-persistence".to_owned()],
                    error.message(),
                );
                Err(error)
            }
            None => {
                self.runtime.update_service(
                    DesktopProcessServiceId::WindowPersistence,
                    DesktopProcessServicePhase::Stopped,
                    0,
                    0,
                    false,
                    vec!["superi-window-persistence".to_owned()],
                    "Window session persistence joined",
                );
                Ok(())
            }
        }
    }

    fn install_restored<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        loaded: LoadedPersistedSession,
    ) -> Result<()> {
        let main = app.get_window(PRIMARY_WINDOW_LABEL).ok_or_else(|| {
            unavailable(
                "restore_session",
                "the primary application window is unavailable",
            )
        })?;
        let monitors = discover_monitors(&main)?;
        let mut session = loaded.session;
        let mut recovery_note = loaded.recovery_note;
        let mut reconciled_labels = Vec::new();
        for record in &mut session.windows {
            let restored = reconcile_placement(&record.placement, &monitors);
            if restored != record.placement {
                reconciled_labels.push(record.label.clone());
            }
            record.placement = restored;
            if record.label == PRIMARY_WINDOW_LABEL {
                apply_window_placement(&main, &record.placement)?;
                main.show()
                    .map_err(|source| native_error("show_primary_window", source))?;
            } else if app.get_window(&record.label).is_none() {
                let window = build_editor_window(app, record)?;
                apply_window_placement(&window, &record.placement)?;
                window
                    .show()
                    .map_err(|source| native_error("show_restored_window", source))?;
            }
        }
        if !reconciled_labels.is_empty() {
            let topology_note = format!(
                "Window restoration reconciled {} into the current monitor topology.",
                reconciled_labels.join(", ")
            );
            recovery_note = Some(match recovery_note {
                Some(note) => format!("{note} {topology_note}"),
                None => topology_note,
            });
        }
        let restored_focus = session.last_focused_label.as_ref().and_then(|label| {
            app.get_window(label)
                .filter(|window| window.set_focus().is_ok())
                .map(|_| label.clone())
        });
        {
            let mut inner = self.lock_inner("restore_session")?;
            inner.session = session;
            inner.monitors = monitors;
            inner.recovery_note = recovery_note;
            inner.phase = if inner.recovery_note.is_some() {
                "recovered"
            } else {
                "ready"
            }
            .to_owned();
            inner.persistence_phase = "ready".to_owned();
            inner.restoration_complete = true;
            inner.focused_label = restored_focus;
            inner.failure = None;
            bump_revision(&mut inner);
        }
        self.signal_persist();
        self.emit_snapshot(app);
        Ok(())
    }

    fn snapshot(&self) -> Result<DesktopWindowSnapshot> {
        let inner = self.lock_inner("snapshot")?;
        Ok(snapshot_from_inner(&inner))
    }

    /// Selects the active editor webview for one process-wide native menu command.
    pub(crate) fn command_target(&self) -> String {
        self.inner
            .lock()
            .ok()
            .and_then(|inner| {
                inner
                    .focused_label
                    .as_ref()
                    .or(inner.session.last_focused_label.as_ref())
                    .filter(|label| {
                        inner
                            .session
                            .windows
                            .iter()
                            .any(|window| &window.label == *label)
                    })
                    .cloned()
            })
            .unwrap_or_else(|| PRIMARY_WINDOW_LABEL.to_owned())
    }

    fn create<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        workspace: &str,
    ) -> Result<DesktopWindowSnapshot> {
        require_workspace(workspace)?;
        let (record, monitors) = {
            let mut inner = self.lock_inner("create_window")?;
            require_ready(&inner, "create_window")?;
            if inner.session.windows.len() >= MAX_EDITOR_WINDOWS {
                return Err(resource_exhausted(
                    "create_window",
                    "the editor window limit is reached",
                ));
            }
            let id = inner.session.next_window_id;
            inner.session.next_window_id = id.checked_add(1).ok_or_else(|| {
                resource_exhausted("create_window", "the editor window identity is exhausted")
            })?;
            let label = format!("workspace-{id}");
            let mut placement = inner
                .session
                .last_focused_label
                .as_ref()
                .and_then(|focused| {
                    inner
                        .session
                        .windows
                        .iter()
                        .find(|window| &window.label == focused)
                })
                .map(|window| window.placement.clone())
                .unwrap_or_else(PersistedPlacement::default_primary);
            placement.x = placement.x.saturating_add(32);
            placement.y = placement.y.saturating_add(32);
            placement.fullscreen = false;
            placement = reconcile_placement(&placement, &inner.monitors);
            (
                PersistedWindow::new(
                    &label,
                    format!("Superi Workspace {id}"),
                    workspace,
                    placement,
                ),
                inner.monitors.clone(),
            )
        };
        let window = build_editor_window(app, &record)?;
        if let Err(error) = apply_window_placement(&window, &record.placement) {
            let _ = window.close();
            return Err(error);
        }
        let publish_created = (|| {
            let mut inner = self.lock_inner("publish_created_window")?;
            if inner.session.windows.len() >= MAX_EDITOR_WINDOWS {
                return Err(resource_exhausted(
                    "create_window",
                    "the editor window limit is reached",
                ));
            }
            if inner
                .session
                .windows
                .iter()
                .any(|active| active.label == record.label)
            {
                return Err(conflict(
                    "create_window",
                    "the editor window identity is already active",
                ));
            }
            inner.session.windows.push(record.clone());
            inner.monitors = monitors;
            bump_revision(&mut inner);
            Ok(())
        })();
        if let Err(error) = publish_created {
            let _ = window.close();
            return Err(error);
        }
        let show_and_focus = window
            .show()
            .map_err(|source| native_error("show_created_window", source))
            .and_then(|_| {
                window
                    .set_focus()
                    .map_err(|source| native_error("focus_created_window", source))
            });
        if let Err(error) = show_and_focus {
            let cleanup = self.discard_active_record(&record.label, None);
            let _ = window.close();
            cleanup?;
            return Err(error);
        }
        {
            let mut inner = self.lock_inner("focus_created_window")?;
            inner.session.last_focused_label = Some(record.label.clone());
            inner.focused_label = Some(record.label.clone());
            bump_revision(&mut inner);
        }
        self.signal_persist();
        self.emit_snapshot(app);
        self.snapshot()
    }

    fn focus<R: Runtime>(&self, app: &AppHandle<R>, label: &str) -> Result<DesktopWindowSnapshot> {
        {
            let inner = self.lock_inner("focus_window")?;
            require_active_label(&inner, label)?;
        }
        let window = app.get_window(label).ok_or_else(|| {
            unavailable("focus_window", "the requested editor window is unavailable")
        })?;
        window
            .show()
            .map_err(|source| native_error("show_focused_window", source))?;
        window
            .set_focus()
            .map_err(|source| native_error("focus_window", source))?;
        self.capture_window(&window)?;
        {
            let mut inner = self.lock_inner("publish_focus")?;
            inner.session.last_focused_label = Some(label.to_owned());
            inner.focused_label = Some(label.to_owned());
            bump_revision(&mut inner);
        }
        self.signal_persist();
        self.emit_snapshot(app);
        self.snapshot()
    }

    fn set_fullscreen<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        label: &str,
        fullscreen: bool,
    ) -> Result<DesktopWindowSnapshot> {
        let window = require_live_window(app, label, "set_fullscreen")?;
        self.capture_window(&window)?;
        let target = {
            let mut inner = self.lock_inner("set_fullscreen")?;
            let record = active_record_mut(&mut inner, label, "set_fullscreen")?;
            if record.placement.fullscreen == fullscreen {
                None
            } else {
                let previous = record.placement.clone();
                record.previous_placement = Some(previous);
                record.placement.fullscreen = fullscreen;
                Some(record.placement.clone())
            }
        };
        if let Some(target) = target {
            if let Err(error) = apply_window_placement(&window, &target) {
                let _ = self.capture_window(&window);
                return Err(error);
            }
            let mut inner = self.lock_inner("publish_fullscreen")?;
            bump_revision(&mut inner);
        }
        self.signal_persist();
        self.emit_snapshot(app);
        self.snapshot()
    }

    fn move_to_monitor<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        label: &str,
        monitor_id: &str,
    ) -> Result<DesktopWindowSnapshot> {
        let window = require_live_window(app, label, "move_to_monitor")?;
        self.capture_window(&window)?;
        let monitors = discover_monitors(&window)?;
        let target_monitor = monitors
            .iter()
            .find(|monitor| monitor.id == monitor_id)
            .ok_or_else(|| {
                conflict(
                    "move_to_monitor",
                    "the selected monitor is no longer active",
                )
            })?;
        let target = {
            let mut inner = self.lock_inner("move_to_monitor")?;
            let placement = {
                let record = active_record_mut(&mut inner, label, "move_to_monitor")?;
                let source = record
                    .placement
                    .monitor_id
                    .as_ref()
                    .and_then(|id| monitors.iter().find(|monitor| &monitor.id == id));
                let current = record.placement.clone();
                record.previous_placement = Some(current.clone());
                record.placement = move_placement_to_monitor(&current, source, target_monitor);
                record.placement.clone()
            };
            inner.monitors = monitors;
            placement
        };
        if let Err(error) = apply_window_placement(&window, &target) {
            let _ = self.capture_window(&window);
            return Err(error);
        }
        {
            let mut inner = self.lock_inner("publish_monitor_move")?;
            bump_revision(&mut inner);
        }
        self.signal_persist();
        self.emit_snapshot(app);
        self.snapshot()
    }

    fn undo_placement<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        label: &str,
    ) -> Result<DesktopWindowSnapshot> {
        let window = require_live_window(app, label, "undo_placement")?;
        self.capture_window(&window)?;
        let target = {
            let mut inner = self.lock_inner("undo_placement")?;
            let monitors = inner.monitors.clone();
            let record = active_record_mut(&mut inner, label, "undo_placement")?;
            let previous = record.previous_placement.take().ok_or_else(|| {
                conflict("undo_placement", "this window has no reversible placement")
            })?;
            let current = record.placement.clone();
            let (restored, redo) = swap_reversible_placement(current, previous);
            record.placement = reconcile_placement(&restored, &monitors);
            record.previous_placement = Some(redo);
            record.placement.clone()
        };
        if let Err(error) = apply_window_placement(&window, &target) {
            let _ = self.capture_window(&window);
            return Err(error);
        }
        {
            let mut inner = self.lock_inner("publish_placement_undo")?;
            bump_revision(&mut inner);
        }
        self.signal_persist();
        self.emit_snapshot(app);
        self.snapshot()
    }

    fn update_workspace<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        label: &str,
        workspace: &str,
    ) -> Result<DesktopWindowSnapshot> {
        require_workspace(workspace)?;
        {
            let mut inner = self.lock_inner("update_workspace")?;
            let record = active_record_mut(&mut inner, label, "update_workspace")?;
            if record.workspace != workspace {
                record.workspace = workspace.to_owned();
                bump_revision(&mut inner);
            }
        }
        self.signal_persist();
        self.emit_snapshot(app);
        self.snapshot()
    }

    fn close<R: Runtime>(
        &self,
        app: &AppHandle<R>,
        label: &str,
        transport: &DesktopTransportState,
    ) -> Result<DesktopWindowSnapshot> {
        if label == PRIMARY_WINDOW_LABEL {
            return Err(conflict(
                "close_window",
                "the primary window closes only through orderly application shutdown",
            ));
        }
        let window = require_live_window(app, label, "close_window")?;
        if let Err(error) = self.capture_window(&window) {
            self.record_failure(&error);
        }
        window
            .close()
            .map_err(|source| native_error("close_window", source))?;
        self.close_record(label)?;
        transport
            .disconnect_client(label)
            .map_err(public_transport_error)?;
        self.signal_persist();
        self.emit_snapshot(app);
        self.snapshot()
    }

    fn reopen<R: Runtime>(&self, app: &AppHandle<R>) -> Result<DesktopWindowSnapshot> {
        let mut record = {
            let inner = self.lock_inner("reopen_window")?;
            require_ready(&inner, "reopen_window")?;
            if inner.session.windows.len() >= MAX_EDITOR_WINDOWS {
                return Err(resource_exhausted(
                    "reopen_window",
                    "the editor window limit is reached",
                ));
            }
            inner
                .session
                .recently_closed
                .last()
                .cloned()
                .ok_or_else(|| conflict("reopen_window", "there is no recently closed window"))?
        };
        let anchor = require_live_window(app, PRIMARY_WINDOW_LABEL, "reopen_window")?;
        let monitors = discover_monitors(&anchor)?;
        record.placement = reconcile_placement(&record.placement, &monitors);
        let window = build_editor_window(app, &record)?;
        if let Err(error) = apply_window_placement(&window, &record.placement) {
            let _ = window.close();
            return Err(error);
        }
        let publish_reopened = (|| {
            let mut inner = self.lock_inner("publish_reopened_window")?;
            if inner.session.windows.len() >= MAX_EDITOR_WINDOWS {
                return Err(resource_exhausted(
                    "reopen_window",
                    "the editor window limit is reached",
                ));
            }
            activate_recently_closed(&mut inner.session, &record.label)?;
            let active = inner
                .session
                .windows
                .iter_mut()
                .find(|active| active.label == record.label)
                .ok_or_else(|| {
                    conflict("reopen_window", "the reopened window record is unavailable")
                })?;
            *active = record.clone();
            inner.monitors = monitors;
            bump_revision(&mut inner);
            Ok(())
        })();
        if let Err(error) = publish_reopened {
            let _ = window.close();
            return Err(error);
        }
        let show_and_focus = window
            .show()
            .map_err(|source| native_error("show_reopened_window", source))
            .and_then(|_| {
                window
                    .set_focus()
                    .map_err(|source| native_error("focus_reopened_window", source))
            });
        if let Err(error) = show_and_focus {
            let label = record.label.clone();
            let cleanup = self.discard_active_record(&label, Some(record));
            let _ = window.close();
            cleanup?;
            return Err(error);
        }
        {
            let mut inner = self.lock_inner("focus_reopened_window")?;
            inner.session.last_focused_label = Some(record.label.clone());
            inner.focused_label = Some(record.label.clone());
            bump_revision(&mut inner);
        }
        self.signal_persist();
        self.emit_snapshot(app);
        self.snapshot()
    }

    fn capture_window<R: Runtime>(&self, window: &Window<R>) -> Result<()> {
        let label = window.label();
        let fullscreen = window
            .is_fullscreen()
            .map_err(|source| native_error("read_fullscreen", source))?;
        let focused = window
            .is_focused()
            .map_err(|source| native_error("read_focus", source))?;
        let position = (!fullscreen)
            .then(|| window.outer_position())
            .transpose()
            .map_err(|source| native_error("read_window_position", source))?;
        let size = (!fullscreen)
            .then(|| window.inner_size())
            .transpose()
            .map_err(|source| native_error("read_window_size", source))?;
        let current_monitor = window
            .current_monitor()
            .map_err(|source| native_error("read_current_monitor", source))?
            .map(|monitor| monitor_routing_id(&monitor));
        let monitors = discover_monitors(window)?;
        let mut inner = self.lock_inner("capture_window")?;
        if inner.shutting_down && label != PRIMARY_WINDOW_LABEL {
            return Ok(());
        }
        let Some(record) = inner
            .session
            .windows
            .iter_mut()
            .find(|record| record.label == label)
        else {
            return Ok(());
        };
        record.placement.fullscreen = fullscreen;
        if let (Some(position), Some(size)) = (position, size) {
            record.placement.x = position.x;
            record.placement.y = position.y;
            record.placement.width = size.width.max(MIN_WINDOW_WIDTH);
            record.placement.height = size.height.max(MIN_WINDOW_HEIGHT);
        }
        if current_monitor.is_some() {
            record.placement.monitor_id = current_monitor;
        }
        if focused {
            inner.session.last_focused_label = Some(label.to_owned());
            inner.focused_label = Some(label.to_owned());
        } else if inner.focused_label.as_deref() == Some(label) {
            inner.focused_label = None;
        }
        inner.monitors = monitors;
        bump_revision(&mut inner);
        Ok(())
    }

    fn close_record(&self, label: &str) -> Result<()> {
        let mut inner = self.lock_inner("close_window")?;
        if inner.shutting_down || label == PRIMARY_WINDOW_LABEL {
            return Ok(());
        }
        let Some(index) = inner
            .session
            .windows
            .iter()
            .position(|record| record.label == label)
        else {
            return Ok(());
        };
        let record = inner.session.windows.remove(index);
        inner
            .session
            .recently_closed
            .retain(|closed| closed.label != label);
        inner.session.recently_closed.push(record);
        if inner.session.recently_closed.len() > MAX_RECENTLY_CLOSED {
            inner.session.recently_closed.remove(0);
        }
        if inner.session.last_focused_label.as_deref() == Some(label) {
            inner.session.last_focused_label = Some(PRIMARY_WINDOW_LABEL.to_owned());
        }
        if inner.focused_label.as_deref() == Some(label) {
            inner.focused_label = None;
        }
        bump_revision(&mut inner);
        Ok(())
    }

    fn discard_active_record(
        &self,
        label: &str,
        restore_recent: Option<PersistedWindow>,
    ) -> Result<()> {
        let mut inner = self.lock_inner("discard_window")?;
        inner.session.windows.retain(|record| record.label != label);
        if let Some(record) = restore_recent {
            inner
                .session
                .recently_closed
                .retain(|closed| closed.label != record.label);
            inner.session.recently_closed.push(record);
        }
        if inner.session.last_focused_label.as_deref() == Some(label) {
            inner.session.last_focused_label = Some(PRIMARY_WINDOW_LABEL.to_owned());
        }
        if inner.focused_label.as_deref() == Some(label) {
            inner.focused_label = None;
        }
        bump_revision(&mut inner);
        Ok(())
    }

    fn record_failure(&self, error: &Error) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.failure = Some(DesktopWindowFailure::from(error));
            if inner.phase == "loading" {
                inner.phase = "failed".to_owned();
            }
            bump_revision(&mut inner);
        }
    }

    fn record_persistence_result<R: Runtime>(&self, app: &AppHandle<R>, result: Result<()>) {
        if let Ok(mut inner) = self.inner.lock() {
            match result {
                Ok(()) => {
                    inner.persistence_phase = "ready".to_owned();
                    if inner
                        .failure
                        .as_ref()
                        .is_some_and(|failure| failure.summary.contains("persist"))
                    {
                        inner.failure = None;
                    }
                }
                Err(error) => {
                    inner.persistence_phase = "degraded".to_owned();
                    inner.failure = Some(DesktopWindowFailure::from(&error));
                }
            }
            bump_revision(&mut inner);
        }
        self.emit_snapshot(app);
    }

    fn signal_persist(&self) {
        if !self
            .inner
            .lock()
            .is_ok_and(|inner| inner.restoration_complete)
        {
            return;
        }
        let sender = self
            .persistence
            .lock()
            .ok()
            .and_then(|persistence| persistence.as_ref().map(|control| control.sender.clone()));
        if let Some(sender) = sender {
            match sender.try_send(PersistenceSignal::Wake) {
                Ok(()) | Err(TrySendError::Full(PersistenceSignal::Wake)) => {}
                Err(TrySendError::Disconnected(_)) => {
                    if let Ok(mut inner) = self.inner.lock() {
                        inner.persistence_phase = "degraded".to_owned();
                    }
                }
                Err(TrySendError::Full(PersistenceSignal::Shutdown)) => {}
            }
        }
    }

    fn emit_snapshot<R: Runtime>(&self, app: &AppHandle<R>) {
        if let Ok(snapshot) = self.snapshot() {
            let _ = app.emit(DESKTOP_WINDOW_EVENT, snapshot);
        }
    }

    fn lock_inner(&self, operation: &'static str) -> Result<MutexGuard<'_, DesktopWindowInner>> {
        self.inner.lock().map_err(|_| lock_error(operation))
    }

    fn lock_persistence(
        &self,
        operation: &'static str,
    ) -> Result<MutexGuard<'_, Option<PersistenceControl>>> {
        self.persistence.lock().map_err(|_| lock_error(operation))
    }
}

fn persistence_loop<R: Runtime>(
    state: DesktopWindowState,
    app: AppHandle<R>,
    path: PathBuf,
    receiver: Receiver<PersistenceSignal>,
) {
    let loaded = load_persisted_session(&path);
    let restore_state = state.clone();
    let restore_app = app.clone();
    let schedule = app.run_on_main_thread(move || {
        if let Err(error) = restore_state.install_restored(&restore_app, loaded) {
            restore_state.record_failure(&error);
            if let Some(main) = restore_app.get_window(PRIMARY_WINDOW_LABEL) {
                let _ = main.show();
            }
            restore_state.emit_snapshot(&restore_app);
        }
    });
    if let Err(error) = schedule {
        state.record_failure(&native_error("schedule_restore", error));
        state.emit_snapshot(&app);
    }

    while let Ok(signal) = receiver.recv() {
        let mut shutdown = matches!(signal, PersistenceSignal::Shutdown);
        if !shutdown {
            loop {
                match receiver.recv_timeout(PERSISTENCE_COALESCE) {
                    Ok(PersistenceSignal::Wake) => {}
                    Ok(PersistenceSignal::Shutdown) => {
                        shutdown = true;
                        break;
                    }
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => {
                        shutdown = true;
                        break;
                    }
                }
            }
        }
        let session = state
            .inner
            .lock()
            .ok()
            .and_then(|inner| inner.restoration_complete.then(|| inner.session.clone()));
        if let Some(session) = session {
            state.record_persistence_result(&app, persist_session(&path, &session));
        }
        if shutdown {
            break;
        }
    }
}

fn persist_session(path: &Path, session: &PersistedWindowSession) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(session).map_err(|source| {
        Error::with_source(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "the window session could not be encoded for persistence",
            source,
        )
        .with_context(ErrorContext::new(COMPONENT, "persist_session"))
    })?;
    let parent = path.parent().ok_or_else(|| {
        invalid(
            "persist_session",
            "the window session path has no parent directory",
        )
    })?;
    fs::create_dir_all(parent)
        .map_err(|source| persistence_error("create_session_directory", source))?;
    let temporary = path.with_file_name(format!("{SESSION_FILE_NAME}.{}.tmp", std::process::id()));
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(&temporary)
        .map_err(|source| persistence_error("create_temporary_session", source))?;
    file.write_all(&bytes)
        .map_err(|source| persistence_error("write_temporary_session", source))?;
    file.sync_all()
        .map_err(|source| persistence_error("sync_temporary_session", source))?;
    drop(file);
    fs::rename(&temporary, path)
        .map_err(|source| persistence_error("replace_window_session", source))?;
    if let Ok(directory) = OpenOptions::new().read(true).open(parent) {
        let _ = directory.sync_all();
    }
    Ok(())
}

fn discover_monitors<R: Runtime>(window: &Window<R>) -> Result<Vec<DesktopMonitorTarget>> {
    let primary = window
        .primary_monitor()
        .map_err(|source| native_error("read_primary_monitor", source))?;
    let primary_id = primary.as_ref().map(monitor_routing_id);
    let mut monitors = window
        .available_monitors()
        .map_err(|source| native_error("enumerate_monitors", source))?
        .iter()
        .map(|monitor| DesktopMonitorTarget::from_monitor(monitor, primary_id.as_deref()))
        .collect::<Result<Vec<_>>>()?;
    monitors.sort_by(|left, right| {
        (
            !left.primary,
            left.position_x,
            left.position_y,
            left.id.as_str(),
        )
            .cmp(&(
                !right.primary,
                right.position_x,
                right.position_y,
                right.id.as_str(),
            ))
    });
    monitors.dedup_by(|left, right| left.id == right.id);
    if monitors.is_empty() {
        return Err(unavailable(
            "enumerate_monitors",
            "no active monitor is available for editor windows",
        ));
    }
    Ok(monitors)
}

fn reconcile_placement(
    placement: &PersistedPlacement,
    monitors: &[DesktopMonitorTarget],
) -> PersistedPlacement {
    let target = placement
        .monitor_id
        .as_ref()
        .and_then(|id| monitors.iter().find(|monitor| &monitor.id == id))
        .or_else(|| monitors.iter().find(|monitor| monitor.primary))
        .or_else(|| monitors.first());
    target.map_or_else(
        || placement.clone(),
        |monitor| clamp_placement_to_monitor(placement, monitor),
    )
}

fn build_editor_window<R: Runtime>(
    app: &AppHandle<R>,
    record: &PersistedWindow,
) -> Result<Window<R>> {
    let url = format!("index.html?workspace={}", record.workspace);
    let webview = WebviewWindowBuilder::new(app, &record.label, WebviewUrl::App(url.into()))
        .title(&record.title)
        .inner_size(
            f64::from(record.placement.width),
            f64::from(record.placement.height),
        )
        .min_inner_size(f64::from(MIN_WINDOW_WIDTH), f64::from(MIN_WINDOW_HEIGHT))
        .position(f64::from(record.placement.x), f64::from(record.placement.y))
        .visible(false)
        .build()
        .map_err(|source| native_error("create_editor_window", source))?;
    Ok(webview.as_ref().window().clone())
}

fn apply_window_placement<R: Runtime>(
    window: &Window<R>,
    placement: &PersistedPlacement,
) -> Result<()> {
    window
        .set_fullscreen(false)
        .map_err(|source| native_error("leave_fullscreen", source))?;
    window
        .set_position(PhysicalPosition::new(placement.x, placement.y))
        .map_err(|source| native_error("position_editor_window", source))?;
    window
        .set_size(PhysicalSize::new(placement.width, placement.height))
        .map_err(|source| native_error("size_editor_window", source))?;
    if placement.fullscreen {
        window
            .set_fullscreen(true)
            .map_err(|source| native_error("enter_fullscreen", source))?;
    }
    Ok(())
}

fn snapshot_from_inner(inner: &DesktopWindowInner) -> DesktopWindowSnapshot {
    let focused = inner.focused_label.as_deref();
    DesktopWindowSnapshot {
        schema_version: SESSION_SCHEMA_VERSION,
        revision: inner.revision,
        phase: inner.phase.clone(),
        persistence_phase: inner.persistence_phase.clone(),
        native_viewport_owner: NATIVE_VIEWPORT_OWNER.to_owned(),
        last_focused_label: inner.session.last_focused_label.clone(),
        recovery_note: inner.recovery_note.clone(),
        failure: inner.failure.clone(),
        recently_closed_count: inner.session.recently_closed.len(),
        monitors: inner.monitors.clone(),
        windows: inner
            .session
            .windows
            .iter()
            .map(|record| DesktopWindowRecord {
                label: record.label.clone(),
                title: record.title.clone(),
                workspace: record.workspace.clone(),
                primary: record.label == PRIMARY_WINDOW_LABEL,
                focused: focused == Some(record.label.as_str()),
                fullscreen: record.placement.fullscreen,
                monitor_id: record.placement.monitor_id.clone(),
                x: record.placement.x,
                y: record.placement.y,
                width: record.placement.width,
                height: record.placement.height,
                can_undo_placement: record.previous_placement.is_some(),
                can_close: record.label != PRIMARY_WINDOW_LABEL,
            })
            .collect(),
    }
}

fn require_live_window<R: Runtime>(
    app: &AppHandle<R>,
    label: &str,
    operation: &'static str,
) -> Result<Window<R>> {
    if !is_editor_window_label(label) {
        return Err(invalid(operation, "the editor window label is invalid"));
    }
    app.get_window(label)
        .ok_or_else(|| unavailable(operation, "the requested editor window is unavailable"))
}

fn activate_recently_closed(
    session: &mut PersistedWindowSession,
    expected_label: &str,
) -> Result<PersistedWindow> {
    let record = session
        .recently_closed
        .pop()
        .ok_or_else(|| conflict("reopen_window", "the recently closed window changed"))?;
    if record.label != expected_label {
        session.recently_closed.push(record);
        return Err(conflict(
            "reopen_window",
            "the recently closed window identity changed",
        ));
    }
    session.windows.push(record.clone());
    Ok(record)
}

fn active_record_mut<'a>(
    inner: &'a mut DesktopWindowInner,
    label: &str,
    operation: &'static str,
) -> Result<&'a mut PersistedWindow> {
    require_ready(inner, operation)?;
    inner
        .session
        .windows
        .iter_mut()
        .find(|record| record.label == label)
        .ok_or_else(|| conflict(operation, "the editor window is not active"))
}

fn require_active_label(inner: &DesktopWindowInner, label: &str) -> Result<()> {
    require_ready(inner, "focus_window")?;
    if inner
        .session
        .windows
        .iter()
        .any(|window| window.label == label)
    {
        Ok(())
    } else {
        Err(conflict("focus_window", "the editor window is not active"))
    }
}

fn require_ready(inner: &DesktopWindowInner, operation: &'static str) -> Result<()> {
    if matches!(inner.phase.as_str(), "ready" | "recovered") && !inner.shutting_down {
        Ok(())
    } else {
        Err(unavailable(
            operation,
            "the window session has not completed restoration",
        ))
    }
}

fn require_workspace(workspace: &str) -> Result<()> {
    if is_workspace(workspace) {
        Ok(())
    } else {
        Err(invalid(
            "validate_workspace",
            "the requested workspace is not registered",
        ))
    }
}

fn is_workspace(workspace: &str) -> bool {
    WORKSPACES.contains(&workspace)
}

fn is_editor_window_label(label: &str) -> bool {
    label == PRIMARY_WINDOW_LABEL || auxiliary_window_id(label).is_some()
}

fn auxiliary_window_id(label: &str) -> Option<u64> {
    label
        .strip_prefix("workspace-")
        .filter(|suffix| !suffix.is_empty() && !suffix.starts_with('0'))
        .and_then(|suffix| suffix.parse::<u64>().ok())
        .filter(|id| *id > 0)
}

fn bump_revision(inner: &mut DesktopWindowInner) {
    inner.revision = inner.revision.saturating_add(1);
}

fn public_transport_error(error: superi_api::schema::PublicApiError) -> Error {
    Error::new(
        error.data().category(),
        error.data().recoverability(),
        error.message().to_owned(),
    )
    .with_context(ErrorContext::new(COMPONENT, "disconnect_transport_client"))
}

fn lock_error(operation: &'static str) -> Error {
    Error::new(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "the desktop window session lock was poisoned",
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unavailable(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn resource_exhausted(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::Retryable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn native_error(operation: &'static str, source: tauri::Error) -> Error {
    Error::with_source(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "the native editor window operation failed",
        source,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn persistence_error(operation: &'static str, source: std::io::Error) -> Error {
    Error::with_source(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "the window session could not be persisted",
        source,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

#[derive(Debug, Serialize)]
pub struct DesktopWindowCommandError {
    category: String,
    recoverability: String,
    summary: String,
}

impl From<Error> for DesktopWindowCommandError {
    fn from(error: Error) -> Self {
        Self {
            category: error.category().code().to_owned(),
            recoverability: error.recoverability().code().to_owned(),
            summary: error.message().to_owned(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopWindowCreateRequest {
    workspace: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopWindowLabelRequest {
    label: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopWindowFullscreenRequest {
    label: String,
    fullscreen: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopWindowMonitorRequest {
    label: String,
    monitor_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopWindowWorkspaceRequest {
    label: String,
    workspace: String,
}

#[tauri::command]
pub fn desktop_window_session_snapshot(
    state: State<'_, DesktopWindowState>,
) -> std::result::Result<DesktopWindowSnapshot, DesktopWindowCommandError> {
    state.snapshot().map_err(Into::into)
}

#[tauri::command]
pub async fn desktop_window_create<R: Runtime>(
    app: AppHandle<R>,
    request: DesktopWindowCreateRequest,
    state: State<'_, DesktopWindowState>,
) -> std::result::Result<DesktopWindowSnapshot, DesktopWindowCommandError> {
    state.create(&app, &request.workspace).map_err(Into::into)
}

#[tauri::command]
pub async fn desktop_window_focus<R: Runtime>(
    app: AppHandle<R>,
    request: DesktopWindowLabelRequest,
    state: State<'_, DesktopWindowState>,
) -> std::result::Result<DesktopWindowSnapshot, DesktopWindowCommandError> {
    state.focus(&app, &request.label).map_err(Into::into)
}

#[tauri::command]
pub async fn desktop_window_fullscreen<R: Runtime>(
    app: AppHandle<R>,
    request: DesktopWindowFullscreenRequest,
    state: State<'_, DesktopWindowState>,
) -> std::result::Result<DesktopWindowSnapshot, DesktopWindowCommandError> {
    state
        .set_fullscreen(&app, &request.label, request.fullscreen)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn desktop_window_move_to_monitor<R: Runtime>(
    app: AppHandle<R>,
    request: DesktopWindowMonitorRequest,
    state: State<'_, DesktopWindowState>,
) -> std::result::Result<DesktopWindowSnapshot, DesktopWindowCommandError> {
    state
        .move_to_monitor(&app, &request.label, &request.monitor_id)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn desktop_window_undo_placement<R: Runtime>(
    app: AppHandle<R>,
    request: DesktopWindowLabelRequest,
    state: State<'_, DesktopWindowState>,
) -> std::result::Result<DesktopWindowSnapshot, DesktopWindowCommandError> {
    state
        .undo_placement(&app, &request.label)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn desktop_window_workspace_update<R: Runtime>(
    app: AppHandle<R>,
    request: DesktopWindowWorkspaceRequest,
    state: State<'_, DesktopWindowState>,
) -> std::result::Result<DesktopWindowSnapshot, DesktopWindowCommandError> {
    state
        .update_workspace(&app, &request.label, &request.workspace)
        .map_err(Into::into)
}

#[tauri::command]
pub async fn desktop_window_close<R: Runtime>(
    app: AppHandle<R>,
    request: DesktopWindowLabelRequest,
    state: State<'_, DesktopWindowState>,
    transport: State<'_, DesktopTransportState>,
) -> std::result::Result<DesktopWindowSnapshot, DesktopWindowCommandError> {
    state
        .close(&app, &request.label, transport.inner())
        .map_err(Into::into)
}

#[tauri::command]
pub async fn desktop_window_reopen<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopWindowState>,
) -> std::result::Result<DesktopWindowSnapshot, DesktopWindowCommandError> {
    state.reopen(&app).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use tauri::Manager;

    use super::{
        activate_recently_closed, load_persisted_session, move_placement_to_monitor,
        normalize_persisted_session, persist_session, reconcile_placement,
        swap_reversible_placement, DesktopMonitorTarget, DesktopWindowState, PersistedPlacement,
        PersistedWindow, PersistedWindowSession, SESSION_SCHEMA_VERSION,
    };

    fn monitor(
        id: &str,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        primary: bool,
    ) -> DesktopMonitorTarget {
        DesktopMonitorTarget {
            id: id.to_owned(),
            name: id.to_owned(),
            position_x: x,
            position_y: y,
            physical_width: width,
            physical_height: height,
            scale_factor: 1.0,
            primary,
        }
    }

    fn placement(x: i32, y: i32, width: u32, height: u32, monitor_id: &str) -> PersistedPlacement {
        PersistedPlacement {
            x,
            y,
            width,
            height,
            fullscreen: false,
            monitor_id: Some(monitor_id.to_owned()),
        }
    }

    #[test]
    fn strict_session_normalization_preserves_primary_and_bounded_window_identity() {
        let session = PersistedWindowSession {
            schema_version: SESSION_SCHEMA_VERSION,
            next_window_id: 3,
            last_focused_label: Some("workspace-2".to_owned()),
            windows: vec![
                PersistedWindow::new(
                    "main",
                    "Superi",
                    "editing",
                    placement(20, 30, 1100, 720, "primary"),
                ),
                PersistedWindow::new(
                    "workspace-2",
                    "Superi Workspace 2",
                    "color",
                    placement(1200, 40, 900, 700, "reference"),
                ),
            ],
            recently_closed: Vec::new(),
        };

        let normalized = normalize_persisted_session(session).unwrap();
        assert_eq!(normalized.windows[0].label, "main");
        assert_eq!(normalized.windows[1].workspace, "color");
        assert_eq!(normalized.next_window_id, 3);

        let unknown = r#"{"schema_version":1,"next_window_id":1,"last_focused_label":null,"windows":[],"recently_closed":[],"unknown":true}"#;
        assert!(serde_json::from_str::<PersistedWindowSession>(unknown).is_err());
    }

    #[test]
    fn monitor_movement_clamps_size_and_preserves_relative_position() {
        let source = monitor("source", 0, 0, 2000, 1200, true);
        let target = monitor("target", 2000, -200, 1000, 800, false);
        let current = placement(750, 300, 1000, 600, "source");

        let moved = move_placement_to_monitor(&current, Some(&source), &target);

        assert_eq!(moved.width, 1000);
        assert_eq!(moved.height, 600);
        assert_eq!(moved.x, 2000);
        assert_eq!(moved.y, -100);
        assert_eq!(moved.monitor_id.as_deref(), Some("target"));
    }

    #[test]
    fn reversible_placement_swaps_current_and_previous_for_redo() {
        let current = placement(0, 0, 900, 700, "primary");
        let previous = PersistedPlacement {
            fullscreen: true,
            ..placement(2000, 0, 1000, 800, "reference")
        };

        let (restored, redo) = swap_reversible_placement(current.clone(), previous.clone());

        assert_eq!(restored, previous);
        assert_eq!(redo, current);
    }

    #[test]
    fn missing_monitor_identity_reconciles_into_visible_primary_bounds() {
        let primary = monitor("primary", -1920, 0, 1920, 1080, true);
        let missing = placement(4000, -900, 2400, 1600, "missing");

        let restored = reconcile_placement(&missing, &[primary]);

        assert_eq!(restored.monitor_id.as_deref(), Some("primary"));
        assert_eq!(restored.x, -1920);
        assert_eq!(restored.y, 0);
        assert_eq!(restored.width, 1920);
        assert_eq!(restored.height, 1080);
    }

    #[test]
    fn native_menu_commands_follow_the_focused_then_last_active_webview() {
        let state = DesktopWindowState::default();
        assert_eq!(state.command_target(), "main");
        {
            let mut inner = state.inner.lock().unwrap();
            inner.session.windows.push(PersistedWindow::new(
                "workspace-1",
                "Superi Workspace 1",
                "color",
                placement(1200, 80, 1000, 800, "reference"),
            ));
            inner.session.last_focused_label = Some("workspace-1".to_owned());
            inner.focused_label = Some("workspace-1".to_owned());
        }
        assert_eq!(state.command_target(), "workspace-1");
        state.inner.lock().unwrap().focused_label = None;
        assert_eq!(state.command_target(), "workspace-1");
        state
            .inner
            .lock()
            .unwrap()
            .session
            .windows
            .retain(|window| window.label == "main");
        assert_eq!(state.command_target(), "main");
    }

    #[test]
    fn corrupt_session_is_preserved_and_recovers_to_a_safe_primary() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "superi-window-session-test-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("window-session-v1.json");
        fs::write(&path, b"not json").unwrap();

        let loaded = load_persisted_session(&path);

        assert_eq!(loaded.session.windows[0].label, "main");
        let rejected = loaded
            .recovery_note
            .as_deref()
            .expect("corrupt state should report preserved recovery evidence");
        assert!(rejected.contains("preserved"));
        assert!(!path.exists());
        assert_eq!(fs::read_dir(&root).unwrap().count(), 1);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn close_and_reopen_preserve_workspace_and_reversible_placement() {
        let state = DesktopWindowState::default();
        {
            let mut inner = state.inner.lock().unwrap();
            inner.phase = "ready".to_owned();
            inner.restoration_complete = true;
            inner.session.windows.push(PersistedWindow {
                previous_placement: Some(placement(40, 50, 900, 700, "primary")),
                ..PersistedWindow::new(
                    "workspace-1",
                    "Superi Workspace 1",
                    "color",
                    placement(1200, 80, 1000, 800, "reference"),
                )
            });
            inner.session.last_focused_label = Some("workspace-1".to_owned());
            inner.focused_label = Some("workspace-1".to_owned());
        }

        state.close_record("workspace-1").unwrap();
        {
            let mut inner = state.inner.lock().unwrap();
            assert_eq!(inner.session.windows.len(), 1);
            assert_eq!(inner.session.recently_closed.len(), 1);
            let reopened = activate_recently_closed(&mut inner.session, "workspace-1").unwrap();
            assert_eq!(reopened.workspace, "color");
            assert!(reopened.previous_placement.is_some());
            assert_eq!(inner.session.windows.len(), 2);
            assert!(inner.session.recently_closed.is_empty());
            normalize_persisted_session(inner.session.clone()).unwrap();
        }
    }

    #[test]
    fn valid_session_persistence_round_trips_without_recovery() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "superi-window-session-roundtrip-{}-{nonce}",
            std::process::id()
        ));
        let path = root.join("window-session-v1.json");
        let session = PersistedWindowSession::default();

        persist_session(&path, &session).unwrap();
        let loaded = load_persisted_session(&path);

        assert_eq!(loaded.session, session);
        assert!(loaded.recovery_note.is_none());
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn mock_tauri_runtime_creates_a_real_auxiliary_webview_window() {
        let app = crate::configure(
            tauri::test::mock_builder(),
            crate::lifecycle::ApplicationLifecycle::new().unwrap(),
        )
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();
        let state = app.state::<DesktopWindowState>();
        {
            let mut inner = state.inner.lock().unwrap();
            inner.phase = "ready".to_owned();
            inner.persistence_phase = "ready".to_owned();
            inner.restoration_complete = true;
        }

        let created = state.create(app.handle(), "color").unwrap();

        assert!(app.get_window("workspace-1").is_some());
        let record = created
            .windows
            .iter()
            .find(|record| record.label == "workspace-1")
            .unwrap();
        assert_eq!(record.workspace, "color");
        assert!(record.focused);
    }
}
