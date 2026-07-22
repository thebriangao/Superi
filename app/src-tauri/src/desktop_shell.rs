//! Native desktop shell projection for menus, workspace continuity, and safe closing.
//!
//! Project contents and transaction history remain owned by the project and engine layers. This
//! module accepts bounded presentation snapshots from the frontend and turns native interactions
//! into typed intents without duplicating those authorities.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuBuilder, MenuEvent, MenuItem, SubmenuBuilder};
use tauri::{AppHandle, Emitter, Manager, Runtime, State, Window, WindowEvent};

pub const DESKTOP_SHELL_EVENT: &str = "superi://desktop-shell-intent";

const DESKTOP_SHELL_SCHEMA_VERSION: u32 = 2;
const LEGACY_DESKTOP_SHELL_SCHEMA_VERSION: u32 = 1;
const MAX_RECENT_PROJECTS: usize = 32;
const MAX_HIDDEN_PANELS: usize = 256;
const MAX_PANEL_LAYOUT_ROUTES: usize = 64;
const MAX_PANELS_PER_DOCK: usize = 256;
const MAX_IDENTITY_BYTES: usize = 512;
const MAX_PATH_BYTES: usize = 32 * 1024;
const DEFAULT_ROUTE_ID: &str = "editing";

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopCloseReason {
    Window,
    Quit,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DesktopShellIntent {
    NewProject,
    OpenProject,
    OpenRecent { path: String },
    SaveProject,
    SaveProjectAs,
    CloseProject,
    ImportMedia,
    ScanFolder,
    Undo,
    Redo,
    OpenWorkspace { route_id: String },
    RequestClose { reason: DesktopCloseReason },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopShellDocument {
    pub path: String,
    pub project_id: String,
    pub project_revision: u64,
}

impl DesktopShellDocument {
    #[must_use]
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopPanelDockId {
    Left,
    Center,
    Right,
    Bottom,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopPanelDockPresentation {
    pub dock_id: DesktopPanelDockId,
    pub panel_ids: Vec<String>,
    pub active_panel_id: Option<String>,
    pub size_basis_points: u16,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopRoutePanelLayoutPresentation {
    pub route_id: String,
    pub docks: Vec<DesktopPanelDockPresentation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopWorkspacePresentation {
    pub active_route_id: String,
    pub hidden_panel_ids: Vec<String>,
    pub focused_panel_id: Option<String>,
    #[serde(default)]
    pub panel_layouts: Vec<DesktopRoutePanelLayoutPresentation>,
}

impl Default for DesktopWorkspacePresentation {
    fn default() -> Self {
        Self {
            active_route_id: DEFAULT_ROUTE_ID.to_owned(),
            hidden_panel_ids: Vec::new(),
            focused_panel_id: None,
            panel_layouts: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopShellSync {
    pub client_sequence: u64,
    pub active: Option<DesktopShellDocument>,
    pub recent_paths: Vec<String>,
    pub undo_depth: u64,
    pub redo_depth: u64,
    pub busy: bool,
    pub workspace: DesktopWorkspacePresentation,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopShellFailure {
    pub code: String,
    pub title: String,
    pub action: String,
}

impl DesktopShellFailure {
    fn invalid(code: &str, title: &str, action: &str) -> Self {
        Self {
            code: code.to_owned(),
            title: title.to_owned(),
            action: action.to_owned(),
        }
    }

    fn storage(operation: &str) -> Self {
        Self::invalid(
            &format!("desktop_shell_storage_{operation}_failed"),
            "Desktop workspace continuity is unavailable",
            "Continue working, then restore the workspace manually if needed.",
        )
    }
}

impl std::fmt::Display for DesktopShellFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}: {}", self.code, self.title)
    }
}

impl std::error::Error for DesktopShellFailure {}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopShellSnapshot {
    revision: u64,
    client_sequence: u64,
    active: Option<DesktopShellDocument>,
    recent_paths: Vec<String>,
    undo_depth: u64,
    redo_depth: u64,
    busy: bool,
    workspace: DesktopWorkspacePresentation,
    failure: Option<DesktopShellFailure>,
}

impl Default for DesktopShellSnapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            client_sequence: 0,
            active: None,
            recent_paths: Vec::new(),
            undo_depth: 0,
            redo_depth: 0,
            busy: false,
            workspace: DesktopWorkspacePresentation::default(),
            failure: None,
        }
    }
}

impl DesktopShellSnapshot {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub const fn client_sequence(&self) -> u64 {
        self.client_sequence
    }

    #[must_use]
    pub const fn active(&self) -> Option<&DesktopShellDocument> {
        self.active.as_ref()
    }

    #[must_use]
    pub fn recent_paths(&self) -> &[String] {
        &self.recent_paths
    }

    #[must_use]
    pub const fn undo_depth(&self) -> u64 {
        self.undo_depth
    }

    #[must_use]
    pub const fn redo_depth(&self) -> u64 {
        self.redo_depth
    }

    #[must_use]
    pub const fn workspace(&self) -> &DesktopWorkspacePresentation {
        &self.workspace
    }

    #[must_use]
    pub const fn failure(&self) -> Option<&DesktopShellFailure> {
        self.failure.as_ref()
    }
}

#[derive(Clone, Debug, Default)]
pub struct DesktopShellModel {
    snapshot: DesktopShellSnapshot,
    close_pending: bool,
}

impl DesktopShellModel {
    #[must_use]
    pub const fn snapshot(&self) -> &DesktopShellSnapshot {
        &self.snapshot
    }

    pub fn synchronize(
        &mut self,
        sync: DesktopShellSync,
    ) -> Result<DesktopShellSnapshot, DesktopShellFailure> {
        if self.snapshot.client_sequence > 0
            && sync.client_sequence <= self.snapshot.client_sequence
        {
            return Ok(self.snapshot.clone());
        }
        validate_sync(&sync)?;
        self.snapshot = DesktopShellSnapshot {
            revision: self.snapshot.revision.checked_add(1).ok_or_else(|| {
                DesktopShellFailure::invalid(
                    "desktop_shell_revision_exhausted",
                    "Desktop shell state cannot continue",
                    "Restart Superi before continuing.",
                )
            })?,
            client_sequence: sync.client_sequence,
            active: sync.active,
            recent_paths: sync.recent_paths,
            undo_depth: sync.undo_depth,
            redo_depth: sync.redo_depth,
            busy: sync.busy,
            workspace: sync.workspace,
            failure: self.snapshot.failure.clone(),
        };
        Ok(self.snapshot.clone())
    }

    #[must_use]
    pub fn intent_for_menu_id(&self, menu_id: &str) -> Option<DesktopShellIntent> {
        match menu_id {
            "superi.file.new" => Some(DesktopShellIntent::NewProject),
            "superi.file.open" => Some(DesktopShellIntent::OpenProject),
            "superi.file.save" => Some(DesktopShellIntent::SaveProject),
            "superi.file.save_as" => Some(DesktopShellIntent::SaveProjectAs),
            "superi.file.close" => Some(DesktopShellIntent::CloseProject),
            "superi.file.import_media" => Some(DesktopShellIntent::ImportMedia),
            "superi.file.scan_folder" => Some(DesktopShellIntent::ScanFolder),
            "superi.edit.undo" => Some(DesktopShellIntent::Undo),
            "superi.edit.redo" => Some(DesktopShellIntent::Redo),
            id if id.starts_with("superi.file.recent.") => id
                .strip_prefix("superi.file.recent.")?
                .parse::<usize>()
                .ok()
                .and_then(|index| self.snapshot.recent_paths.get(index))
                .cloned()
                .map(|path| DesktopShellIntent::OpenRecent { path }),
            id if id.starts_with("superi.workspace.") => {
                let route_id = id.strip_prefix("superi.workspace.")?;
                valid_identity(route_id).then(|| DesktopShellIntent::OpenWorkspace {
                    route_id: route_id.to_owned(),
                })
            }
            _ => None,
        }
    }

    pub fn request_close(&mut self, reason: DesktopCloseReason) -> Option<DesktopShellIntent> {
        if self.close_pending {
            return None;
        }
        self.close_pending = true;
        Some(DesktopShellIntent::RequestClose { reason })
    }

    pub fn resolve_close(&mut self, allow: bool) -> bool {
        if !self.close_pending {
            return false;
        }
        self.close_pending = false;
        allow
    }

    fn restore_workspace(&mut self, workspace: DesktopWorkspacePresentation) {
        self.snapshot.workspace = workspace;
    }

    fn record_failure(&mut self, failure: DesktopShellFailure) {
        self.snapshot.failure = Some(failure);
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PersistedDesktopShell {
    schema_version: u32,
    workspace: DesktopWorkspacePresentation,
}

#[derive(Clone, Default)]
pub struct DesktopShellState {
    model: Arc<Mutex<DesktopShellModel>>,
    persistence_path: Arc<Mutex<Option<PathBuf>>>,
}

impl DesktopShellState {
    pub fn initialize(&self, recovery_root: &Path) -> Result<(), DesktopShellFailure> {
        std::fs::create_dir_all(recovery_root)
            .map_err(|_| DesktopShellFailure::storage("initialize"))?;
        let path = recovery_root.join("desktop-shell-state.json");
        let restored = if path.exists() {
            std::fs::read(&path)
                .map_err(|_| DesktopShellFailure::storage("read"))
                .and_then(|bytes| {
                    serde_json::from_slice::<PersistedDesktopShell>(&bytes).map_err(|_| {
                        DesktopShellFailure::invalid(
                            "desktop_shell_storage_invalid",
                            "Desktop workspace continuity could not be restored",
                            "Continue with the default workspace and choose panels again.",
                        )
                    })
                })
        } else {
            Ok(PersistedDesktopShell {
                schema_version: DESKTOP_SHELL_SCHEMA_VERSION,
                workspace: DesktopWorkspacePresentation::default(),
            })
        };

        let mut model = self.lock("initialize")?;
        match restored {
            Ok(restored)
                if matches!(
                    restored.schema_version,
                    LEGACY_DESKTOP_SHELL_SCHEMA_VERSION | DESKTOP_SHELL_SCHEMA_VERSION
                ) =>
            {
                if validate_workspace(&restored.workspace).is_ok() {
                    model.restore_workspace(restored.workspace);
                } else {
                    model.record_failure(DesktopShellFailure::invalid(
                        "desktop_shell_storage_invalid",
                        "Desktop workspace continuity could not be restored",
                        "Continue with the default workspace and choose panels again.",
                    ));
                }
            }
            Ok(_) => model.record_failure(DesktopShellFailure::invalid(
                "desktop_shell_storage_version_unsupported",
                "Desktop workspace continuity uses an unsupported version",
                "Continue with the default workspace and choose panels again.",
            )),
            Err(failure) => model.record_failure(failure),
        }
        drop(model);
        *self.path_lock("initialize")? = Some(path);
        Ok(())
    }

    pub fn snapshot(&self) -> Result<DesktopShellSnapshot, DesktopShellFailure> {
        Ok(self.lock("snapshot")?.snapshot().clone())
    }

    pub fn synchronize(
        &self,
        sync: DesktopShellSync,
    ) -> Result<DesktopShellSnapshot, DesktopShellFailure> {
        let snapshot = self.lock("synchronize")?.synchronize(sync)?;
        if let Err(failure) = self.persist_workspace(&snapshot.workspace) {
            let mut model = self.lock("record_persistence_failure")?;
            model.record_failure(failure);
            return Ok(model.snapshot().clone());
        }
        Ok(snapshot)
    }

    fn request_close(
        &self,
        reason: DesktopCloseReason,
    ) -> Result<Option<DesktopShellIntent>, DesktopShellFailure> {
        Ok(self.lock("request_close")?.request_close(reason))
    }

    fn resolve_close(&self, allow: bool) -> Result<bool, DesktopShellFailure> {
        Ok(self.lock("resolve_close")?.resolve_close(allow))
    }

    fn persist_workspace(
        &self,
        workspace: &DesktopWorkspacePresentation,
    ) -> Result<(), DesktopShellFailure> {
        let path = self
            .path_lock("persist_workspace")?
            .clone()
            .ok_or_else(|| DesktopShellFailure::storage("not_initialized"))?;
        let bytes = serde_json::to_vec_pretty(&PersistedDesktopShell {
            schema_version: DESKTOP_SHELL_SCHEMA_VERSION,
            workspace: workspace.clone(),
        })
        .map_err(|_| DesktopShellFailure::storage("serialize"))?;
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, bytes).map_err(|_| DesktopShellFailure::storage("write"))?;
        std::fs::rename(&temporary, path).map_err(|_| DesktopShellFailure::storage("publish"))
    }

    fn lock(
        &self,
        operation: &str,
    ) -> Result<std::sync::MutexGuard<'_, DesktopShellModel>, DesktopShellFailure> {
        self.model.lock().map_err(|_| {
            DesktopShellFailure::invalid(
                "desktop_shell_state_poisoned",
                "Desktop shell state cannot continue",
                &format!("Restart Superi after {operation}."),
            )
        })
    }

    fn path_lock(
        &self,
        operation: &str,
    ) -> Result<std::sync::MutexGuard<'_, Option<PathBuf>>, DesktopShellFailure> {
        self.persistence_path.lock().map_err(|_| {
            DesktopShellFailure::invalid(
                "desktop_shell_path_poisoned",
                "Desktop shell storage cannot continue",
                &format!("Restart Superi after {operation}."),
            )
        })
    }
}

#[tauri::command]
pub fn desktop_shell_snapshot(
    state: State<'_, DesktopShellState>,
) -> Result<DesktopShellSnapshot, DesktopShellFailure> {
    state.snapshot()
}

#[tauri::command]
pub fn desktop_shell_sync<R: Runtime>(
    app: AppHandle<R>,
    window: tauri::WebviewWindow<R>,
    sync: DesktopShellSync,
    state: State<'_, DesktopShellState>,
) -> Result<DesktopShellSnapshot, DesktopShellFailure> {
    if window.label() != "main" {
        return state.snapshot();
    }
    let snapshot = state.synchronize(sync)?;
    apply_native_projection(&app, &snapshot)?;
    Ok(snapshot)
}

#[tauri::command]
pub(crate) fn desktop_shell_resolve_close(
    allow: bool,
    state: State<'_, DesktopShellState>,
    lifecycle: State<'_, crate::ManagedLifecycle>,
) -> Result<bool, DesktopShellFailure> {
    let accepted = state.resolve_close(allow)?;
    if accepted {
        lifecycle.0.request_shutdown().map_err(|_| {
            DesktopShellFailure::invalid(
                "desktop_shell_shutdown_failed",
                "Superi could not begin orderly shutdown",
                "Keep Superi open and try closing again.",
            )
        })?;
    }
    Ok(accepted)
}

#[tauri::command]
pub fn desktop_shell_request_close<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, DesktopShellState>,
) -> Result<bool, DesktopShellFailure> {
    emit_close_request(&app, &state, DesktopCloseReason::Quit)
}

pub fn build_menu<R: Runtime>(
    app: &AppHandle<R>,
    snapshot: &DesktopShellSnapshot,
) -> tauri::Result<Menu<R>> {
    let active = snapshot.active.is_some();
    let available = active && !snapshot.busy;
    let new_project = MenuItem::with_id(
        app,
        "superi.file.new",
        "New Project",
        !snapshot.busy,
        Some("CmdOrCtrl+N"),
    )?;
    let open_project = MenuItem::with_id(
        app,
        "superi.file.open",
        "Open Project...",
        !snapshot.busy,
        Some("CmdOrCtrl+O"),
    )?;
    let save_project = MenuItem::with_id(
        app,
        "superi.file.save",
        "Save",
        available,
        Some("CmdOrCtrl+S"),
    )?;
    let save_as = MenuItem::with_id(
        app,
        "superi.file.save_as",
        "Save As...",
        available,
        Some("CmdOrCtrl+Shift+S"),
    )?;
    let close_project = MenuItem::with_id(
        app,
        "superi.file.close",
        "Close Project",
        available,
        Some("CmdOrCtrl+W"),
    )?;
    let import_media = MenuItem::with_id(
        app,
        "superi.file.import_media",
        "Import Media...",
        available,
        Some("CmdOrCtrl+I"),
    )?;
    let scan_folder = MenuItem::with_id(
        app,
        "superi.file.scan_folder",
        "Scan Folder...",
        available,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(
        app,
        "superi.file.quit",
        "Quit Superi",
        true,
        Some("CmdOrCtrl+Q"),
    )?;

    let mut recent = SubmenuBuilder::with_id(app, "superi.file.recent", "Open Recent");
    if snapshot.recent_paths.is_empty() {
        let empty = MenuItem::with_id(
            app,
            "superi.file.recent.empty",
            "No Recent Projects",
            false,
            None::<&str>,
        )?;
        recent = recent.item(&empty);
    } else {
        for (index, path) in snapshot.recent_paths.iter().enumerate() {
            let item = MenuItem::with_id(
                app,
                format!("superi.file.recent.{index}"),
                document_name(path),
                !snapshot.busy,
                None::<&str>,
            )?;
            recent = recent.item(&item);
        }
    }
    let recent = recent.build()?;
    let file = SubmenuBuilder::new(app, "File")
        .item(&new_project)
        .item(&open_project)
        .item(&recent)
        .separator()
        .item(&save_project)
        .item(&save_as)
        .item(&close_project)
        .separator()
        .item(&import_media)
        .item(&scan_folder)
        .separator()
        .item(&quit)
        .build()?;

    let undo = MenuItem::with_id(
        app,
        "superi.edit.undo",
        "Undo Project Change",
        snapshot.undo_depth > 0 && !snapshot.busy,
        Some("CmdOrCtrl+Z"),
    )?;
    let redo = MenuItem::with_id(
        app,
        "superi.edit.redo",
        "Redo Project Change",
        snapshot.redo_depth > 0 && !snapshot.busy,
        Some("CmdOrCtrl+Shift+Z"),
    )?;
    let edit = SubmenuBuilder::new(app, "Edit")
        .item(&undo)
        .item(&redo)
        .separator()
        .cut()
        .copy()
        .paste()
        .select_all()
        .build()?;

    let workspace = SubmenuBuilder::new(app, "Workspace")
        .text("superi.workspace.editing", "Editing")
        .text("superi.workspace.compositing", "Compositing")
        .text("superi.workspace.color", "Color")
        .text("superi.workspace.audio", "Audio")
        .text("superi.workspace.delivery", "Delivery")
        .text("superi.workspace.system", "System")
        .build()?;

    MenuBuilder::new(app)
        .item(&file)
        .item(&edit)
        .item(&workspace)
        .build()
}

pub fn handle_menu_event<R: Runtime>(app: &AppHandle<R>, event: MenuEvent) {
    let state = app.state::<DesktopShellState>();
    let target = app
        .state::<crate::window_session::DesktopWindowState>()
        .command_target();
    let result = if event.id().as_ref() == "superi.file.quit" {
        emit_close_request(app, &state, DesktopCloseReason::Quit).map(|_| ())
    } else {
        state
            .lock("menu_event")
            .ok()
            .and_then(|model| model.intent_for_menu_id(event.id().as_ref()))
            .map_or(Ok(()), |intent| {
                app.emit_to(&target, DESKTOP_SHELL_EVENT, intent)
                    .map_err(|_| {
                        DesktopShellFailure::invalid(
                            "desktop_shell_event_emit_failed",
                            "Desktop command could not be delivered",
                            "Try the command again.",
                        )
                    })
            })
    };
    if let Err(error) = result {
        eprintln!("desktop shell menu event failed: {error}");
    }
}

pub fn handle_window_event<R: Runtime>(window: &Window<R>, event: &WindowEvent) {
    if window.label() != "main" {
        return;
    }
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        let app = window.app_handle();
        let state = app.state::<DesktopShellState>();
        if let Err(error) = emit_close_request(app, &state, DesktopCloseReason::Window) {
            eprintln!("desktop shell close request failed: {error}");
        }
    }
}

pub(crate) fn emit_close_request<R: Runtime>(
    app: &AppHandle<R>,
    state: &DesktopShellState,
    reason: DesktopCloseReason,
) -> Result<bool, DesktopShellFailure> {
    let Some(intent) = state.request_close(reason)? else {
        return Ok(false);
    };
    let target = match reason {
        DesktopCloseReason::Window => "main".to_owned(),
        DesktopCloseReason::Quit => app
            .state::<crate::window_session::DesktopWindowState>()
            .command_target(),
    };
    if app.emit_to(&target, DESKTOP_SHELL_EVENT, intent).is_err() {
        let _ = state.resolve_close(false);
        return Err(DesktopShellFailure::invalid(
            "desktop_shell_event_emit_failed",
            "Desktop close request could not be delivered",
            "Return to Superi and try closing again.",
        ));
    }
    Ok(true)
}

fn apply_native_projection<R: Runtime>(
    app: &AppHandle<R>,
    snapshot: &DesktopShellSnapshot,
) -> Result<(), DesktopShellFailure> {
    let menu = build_menu(app, snapshot).map_err(|_| {
        DesktopShellFailure::invalid(
            "desktop_shell_menu_update_failed",
            "Desktop menus could not be updated",
            "Continue with the visible workspace controls and try again.",
        )
    })?;
    app.set_menu(menu).map_err(|_| {
        DesktopShellFailure::invalid(
            "desktop_shell_menu_install_failed",
            "Desktop menus could not be installed",
            "Continue with the visible workspace controls and try again.",
        )
    })?;
    if let Some(window) = app.get_webview_window("main") {
        window
            .set_title(&document_title(snapshot.active.as_ref()))
            .map_err(|_| {
                DesktopShellFailure::invalid(
                    "desktop_shell_title_update_failed",
                    "Desktop document title could not be updated",
                    "Continue working and save the active project normally.",
                )
            })?;
    }
    Ok(())
}

fn validate_sync(sync: &DesktopShellSync) -> Result<(), DesktopShellFailure> {
    if sync.client_sequence == 0 {
        return Err(invalid_presentation("client sequence"));
    }
    if let Some(active) = &sync.active {
        validate_path(&active.path, "active project path")?;
        validate_text(&active.project_id, "active project identity")?;
    }
    if sync.recent_paths.len() > MAX_RECENT_PROJECTS {
        return Err(invalid_presentation("recent project capacity"));
    }
    let mut recent = BTreeSet::new();
    for path in &sync.recent_paths {
        validate_path(path, "recent project path")?;
        if !recent.insert(path) {
            return Err(invalid_presentation("duplicate recent project path"));
        }
    }
    validate_workspace(&sync.workspace)
}

fn validate_workspace(workspace: &DesktopWorkspacePresentation) -> Result<(), DesktopShellFailure> {
    validate_text(&workspace.active_route_id, "workspace route")?;
    if workspace.hidden_panel_ids.len() > MAX_HIDDEN_PANELS {
        return Err(invalid_presentation("hidden panel capacity"));
    }
    let mut panels = BTreeSet::new();
    for panel_id in &workspace.hidden_panel_ids {
        validate_text(panel_id, "hidden panel identity")?;
        if !panels.insert(panel_id) {
            return Err(invalid_presentation("duplicate hidden panel identity"));
        }
    }
    if let Some(panel_id) = &workspace.focused_panel_id {
        validate_text(panel_id, "focused panel identity")?;
        if panels.contains(panel_id) {
            return Err(invalid_presentation("hidden focused panel"));
        }
    }
    if workspace.panel_layouts.is_empty() {
        return Ok(());
    }
    if workspace.panel_layouts.len() > MAX_PANEL_LAYOUT_ROUTES {
        return Err(invalid_presentation("panel layout route capacity"));
    }
    let mut routes = BTreeSet::new();
    let mut active_layout = None;
    for layout in &workspace.panel_layouts {
        validate_text(&layout.route_id, "panel layout route identity")?;
        if !routes.insert(layout.route_id.as_str()) {
            return Err(invalid_presentation("duplicate panel layout route"));
        }
        if layout.docks.len() != 4 {
            return Err(invalid_presentation("panel layout dock count"));
        }
        let mut dock_ids = BTreeSet::new();
        let mut route_panels = BTreeSet::new();
        for dock in &layout.docks {
            if !dock_ids.insert(dock.dock_id) {
                return Err(invalid_presentation("duplicate panel layout dock"));
            }
            validate_dock_size(dock.dock_id, dock.size_basis_points)?;
            if dock.panel_ids.len() > MAX_PANELS_PER_DOCK {
                return Err(invalid_presentation("panel layout dock capacity"));
            }
            for panel_id in &dock.panel_ids {
                validate_text(panel_id, "panel layout panel identity")?;
                if !route_panels.insert(panel_id.as_str()) {
                    return Err(invalid_presentation("duplicate routed panel identity"));
                }
            }
            let visible_panel = dock
                .panel_ids
                .iter()
                .find(|panel_id| !panels.contains(panel_id));
            match &dock.active_panel_id {
                Some(active_panel_id) => {
                    validate_text(active_panel_id, "active panel identity")?;
                    if !dock.panel_ids.contains(active_panel_id) || panels.contains(active_panel_id)
                    {
                        return Err(invalid_presentation("invalid active panel"));
                    }
                }
                None if visible_panel.is_some() => {
                    return Err(invalid_presentation("missing active panel"));
                }
                None => {}
            }
        }
        if layout.route_id == workspace.active_route_id {
            active_layout = Some(layout);
        }
    }
    let active_layout =
        active_layout.ok_or_else(|| invalid_presentation("missing active route layout"))?;
    if let Some(focused_panel_id) = &workspace.focused_panel_id {
        let focused_is_active = active_layout
            .docks
            .iter()
            .any(|dock| dock.active_panel_id.as_deref() == Some(focused_panel_id.as_str()));
        if !focused_is_active {
            return Err(invalid_presentation("focused panel is not an active tab"));
        }
    }
    Ok(())
}

fn validate_dock_size(
    dock_id: DesktopPanelDockId,
    size_basis_points: u16,
) -> Result<(), DesktopShellFailure> {
    let valid = match dock_id {
        DesktopPanelDockId::Left | DesktopPanelDockId::Right => {
            (1_500..=4_500).contains(&size_basis_points)
        }
        DesktopPanelDockId::Center => size_basis_points == 10_000,
        DesktopPanelDockId::Bottom => (1_800..=6_500).contains(&size_basis_points),
    };
    if valid {
        Ok(())
    } else {
        Err(invalid_presentation("panel dock size"))
    }
}

fn validate_text(value: &str, kind: &str) -> Result<(), DesktopShellFailure> {
    if !valid_identity(value) || value.len() > MAX_IDENTITY_BYTES {
        return Err(invalid_presentation(kind));
    }
    Ok(())
}

fn validate_path(value: &str, kind: &str) -> Result<(), DesktopShellFailure> {
    if !valid_identity(value) || value.len() > MAX_PATH_BYTES {
        return Err(invalid_presentation(kind));
    }
    Ok(())
}

fn valid_identity(value: &str) -> bool {
    !value.trim().is_empty() && !value.chars().any(char::is_control)
}

fn invalid_presentation(kind: &str) -> DesktopShellFailure {
    DesktopShellFailure::invalid(
        "desktop_shell_presentation_invalid",
        "Desktop shell presentation is invalid",
        &format!("Refresh the application after correcting the {kind}."),
    )
}

fn document_name(path: &str) -> String {
    path.rsplit(['/', '\\'])
        .find(|component| !component.is_empty())
        .unwrap_or(path)
        .to_owned()
}

fn document_title(active: Option<&DesktopShellDocument>) -> String {
    active.map_or_else(
        || "Superi".to_owned(),
        |active| {
            format!(
                "{} [r{}] - Superi",
                document_name(&active.path),
                active.project_revision
            )
        },
    )
}
