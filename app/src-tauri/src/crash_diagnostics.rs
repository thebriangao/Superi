//! Shell-local crash diagnostics and cross-session recovery intent.
//!
//! This owner retains one bounded private journal and one active-session marker under the Tauri
//! application data directory. Public snapshots contain reviewed recovery guidance and stable
//! context only. Raw panic detail remains local and never crosses the Tauri response boundary.

use std::collections::BTreeSet;
use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, Once, OnceLock, TryLockError};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::lifecycle::{ApplicationLifecyclePhase, DesktopLifecycleSnapshot};

const COMPONENT: &str = "superi-desktop.crash-diagnostics";
const STORE_SCHEMA_VERSION: u32 = 1;
const STORE_FILE: &str = "diagnostics.json";
const SESSION_FILE: &str = "active-session.json";
const MAX_RETAINED_DIAGNOSTICS: usize = 32;
const MAX_CONTEXTS: usize = 16;
const MAX_HIDDEN_PANELS: usize = 64;
const MAX_IDENTITY_BYTES: usize = 128;
const MAX_CODE_BYTES: usize = 128;
const MAX_TITLE_BYTES: usize = 1_024;
const MAX_ACTION_BYTES: usize = 2_048;
const MAX_PROJECT_PATH_BYTES: usize = 4_096;
const MAX_PRIVATE_DETAIL_BYTES: usize = 16_384;
const MAX_STORE_FILE_BYTES: usize = 2 * 1_024 * 1_024;
const MAX_SESSION_FILE_BYTES: usize = 128 * 1_024;

static INSTALL_PANIC_HOOK: Once = Once::new();

/// The exact four-class recovery vocabulary presented by local crash diagnostics.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DesktopCrashFailureClass {
    Retryable,
    Degraded,
    UserCorrectable,
    Terminal,
}

impl From<Recoverability> for DesktopCrashFailureClass {
    fn from(value: Recoverability) -> Self {
        match value {
            Recoverability::Retryable => Self::Retryable,
            Recoverability::Degraded => Self::Degraded,
            Recoverability::UserCorrectable => Self::UserCorrectable,
            Recoverability::Terminal => Self::Terminal,
            _ => Self::Terminal,
        }
    }
}

/// One explicit existing owner that can continue recovery from a retained diagnostic.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum DesktopCrashRecoveryEntryPoint {
    RestoreWorkspace,
    RetryEngine,
    ContinueDegraded,
    ReviewProjectRecovery,
    RestartEngine,
}

/// One reviewed stable component and operation frame.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopCrashContext {
    component: String,
    operation: String,
}

impl DesktopCrashContext {
    fn new(component: impl Into<String>, operation: impl Into<String>) -> Result<Self> {
        let value = Self {
            component: component.into(),
            operation: operation.into(),
        };
        validate_bounded_text(
            "context component",
            &value.component,
            MAX_IDENTITY_BYTES,
            "validate_context",
        )?;
        validate_bounded_text(
            "context operation",
            &value.operation,
            MAX_IDENTITY_BYTES,
            "validate_context",
        )?;
        Ok(value)
    }

    #[must_use]
    pub fn component(&self) -> &str {
        &self.component
    }

    #[must_use]
    pub fn operation(&self) -> &str {
        &self.operation
    }
}

/// Application route, panel visibility, and focus intent retained across sessions.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopWorkspaceContinuity {
    route_id: String,
    hidden_panel_ids: Vec<String>,
    focused_panel_id: Option<String>,
}

impl DesktopWorkspaceContinuity {
    pub fn new<I, S, F>(
        route_id: impl Into<String>,
        hidden_panel_ids: I,
        focused_panel_id: Option<F>,
    ) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
        F: Into<String>,
    {
        let value = Self {
            route_id: route_id.into(),
            hidden_panel_ids: hidden_panel_ids.into_iter().map(Into::into).collect(),
            focused_panel_id: focused_panel_id.map(Into::into),
        };
        value.validate()?;
        Ok(value)
    }

    #[must_use]
    pub fn route_id(&self) -> &str {
        &self.route_id
    }

    #[must_use]
    pub fn hidden_panel_ids(&self) -> &[String] {
        &self.hidden_panel_ids
    }

    #[must_use]
    pub fn focused_panel_id(&self) -> Option<&str> {
        self.focused_panel_id.as_deref()
    }

    fn validate(&self) -> Result<()> {
        validate_application_identity("route_id", &self.route_id)?;
        if self.hidden_panel_ids.len() > MAX_HIDDEN_PANELS {
            return Err(diagnostics_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "workspace continuity contains too many hidden panels",
                "validate_workspace",
            ));
        }
        let mut unique = BTreeSet::new();
        for panel_id in &self.hidden_panel_ids {
            validate_application_identity("hidden_panel_id", panel_id)?;
            if !unique.insert(panel_id) {
                return Err(diagnostics_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "workspace continuity contains a duplicate hidden panel",
                    "validate_workspace",
                ));
            }
        }
        if let Some(panel_id) = &self.focused_panel_id {
            validate_application_identity("focused_panel_id", panel_id)?;
            if unique.contains(panel_id) {
                return Err(diagnostics_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "the focused panel cannot also be hidden",
                    "validate_workspace",
                ));
            }
        }
        Ok(())
    }
}

impl Default for DesktopWorkspaceContinuity {
    fn default() -> Self {
        Self {
            route_id: "editing".to_owned(),
            hidden_panel_ids: Vec::new(),
            focused_panel_id: Some("workspace.editing".to_owned()),
        }
    }
}

/// Active project intent retained locally for reopen and project recovery discovery.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopProjectContinuity {
    path: String,
    project_revision: u64,
}

impl DesktopProjectContinuity {
    pub fn new(path: impl Into<String>, project_revision: u64) -> Result<Self> {
        let value = Self {
            path: path.into(),
            project_revision,
        };
        value.validate()?;
        Ok(value)
    }

    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    fn validate(&self) -> Result<()> {
        if self.path.is_empty()
            || self.path.len() > MAX_PROJECT_PATH_BYTES
            || self.path.contains('\0')
        {
            return Err(diagnostics_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "project continuity contains an empty, oversized, or invalid path",
                "validate_project",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct DesktopLifecycleContinuity {
    revision: u64,
    engine_generation: u64,
    application_phase: String,
}

/// One exact set of shell presentation and project intent captured with a diagnostic.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DesktopSessionContinuity {
    workspace: DesktopWorkspaceContinuity,
    project: Option<DesktopProjectContinuity>,
    lifecycle: Option<DesktopLifecycleContinuity>,
}

impl DesktopSessionContinuity {
    #[must_use]
    pub const fn workspace(&self) -> &DesktopWorkspaceContinuity {
        &self.workspace
    }

    #[must_use]
    pub const fn project(&self) -> Option<&DesktopProjectContinuity> {
        self.project.as_ref()
    }
}

/// One reviewed diagnostic record safe for shell presentation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopCrashDiagnostic {
    diagnostic_id: String,
    captured_unix_millis: u64,
    failure_class: DesktopCrashFailureClass,
    code: String,
    title: String,
    action: String,
    contexts: Vec<DesktopCrashContext>,
    continuity: DesktopSessionContinuity,
    recovery_entry_point: DesktopCrashRecoveryEntryPoint,
}

impl DesktopCrashDiagnostic {
    #[must_use]
    pub fn diagnostic_id(&self) -> &str {
        &self.diagnostic_id
    }

    #[must_use]
    pub const fn captured_unix_millis(&self) -> u64 {
        self.captured_unix_millis
    }

    #[must_use]
    pub const fn failure_class(&self) -> DesktopCrashFailureClass {
        self.failure_class
    }

    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn action(&self) -> &str {
        &self.action
    }

    #[must_use]
    pub fn contexts(&self) -> &[DesktopCrashContext] {
        &self.contexts
    }

    #[must_use]
    pub const fn continuity(&self) -> &DesktopSessionContinuity {
        &self.continuity
    }

    #[must_use]
    pub const fn recovery_entry_point(&self) -> DesktopCrashRecoveryEntryPoint {
        self.recovery_entry_point
    }
}

/// Complete current session and retained diagnostic state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopCrashDiagnosticsSnapshot {
    revision: u64,
    current_session_id: String,
    persistence_available: bool,
    continuity: DesktopSessionContinuity,
    diagnostics: Vec<DesktopCrashDiagnostic>,
}

impl DesktopCrashDiagnosticsSnapshot {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn current_session_id(&self) -> &str {
        &self.current_session_id
    }

    #[must_use]
    pub const fn persistence_available(&self) -> bool {
        self.persistence_available
    }

    #[must_use]
    pub const fn continuity(&self) -> &DesktopSessionContinuity {
        &self.continuity
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[DesktopCrashDiagnostic] {
        &self.diagnostics
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct StoredDiagnostic {
    diagnostic_id: String,
    session_id: String,
    captured_unix_millis: u64,
    failure_class: DesktopCrashFailureClass,
    code: String,
    title: String,
    action: String,
    contexts: Vec<DesktopCrashContext>,
    continuity: DesktopSessionContinuity,
    recovery_entry_point: DesktopCrashRecoveryEntryPoint,
    private_detail: Option<String>,
}

impl StoredDiagnostic {
    fn public(&self) -> DesktopCrashDiagnostic {
        DesktopCrashDiagnostic {
            diagnostic_id: self.diagnostic_id.clone(),
            captured_unix_millis: self.captured_unix_millis,
            failure_class: self.failure_class,
            code: self.code.clone(),
            title: self.title.clone(),
            action: self.action.clone(),
            contexts: self.contexts.clone(),
            continuity: self.continuity.clone(),
            recovery_entry_point: self.recovery_entry_point,
        }
    }

    fn validate(&self) -> Result<()> {
        validate_bounded_text(
            "diagnostic id",
            &self.diagnostic_id,
            MAX_PROJECT_PATH_BYTES,
            "validate_store",
        )?;
        validate_bounded_text(
            "session id",
            &self.session_id,
            MAX_IDENTITY_BYTES,
            "validate_store",
        )?;
        validate_bounded_text(
            "diagnostic code",
            &self.code,
            MAX_CODE_BYTES,
            "validate_store",
        )?;
        validate_bounded_text(
            "diagnostic title",
            &self.title,
            MAX_TITLE_BYTES,
            "validate_store",
        )?;
        validate_bounded_text(
            "diagnostic action",
            &self.action,
            MAX_ACTION_BYTES,
            "validate_store",
        )?;
        if self.contexts.len() > MAX_CONTEXTS {
            return Err(diagnostics_error(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                "the crash diagnostic store contains too many context frames",
                "validate_store",
            ));
        }
        for context in &self.contexts {
            DesktopCrashContext::new(context.component.clone(), context.operation.clone())?;
        }
        self.continuity.workspace.validate()?;
        if let Some(project) = &self.continuity.project {
            project.validate()?;
        }
        if let Some(lifecycle) = &self.continuity.lifecycle {
            validate_lifecycle_continuity(lifecycle)?;
        }
        if self
            .private_detail
            .as_ref()
            .is_some_and(|detail| detail.len() > MAX_PRIVATE_DETAIL_BYTES)
        {
            return Err(diagnostics_error(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                "the crash diagnostic store contains oversized private detail",
                "validate_store",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct DiagnosticStore {
    schema_version: u32,
    revision: u64,
    next_sequence: u64,
    diagnostics: Vec<StoredDiagnostic>,
}

impl Default for DiagnosticStore {
    fn default() -> Self {
        Self {
            schema_version: STORE_SCHEMA_VERSION,
            revision: 0,
            next_sequence: 1,
            diagnostics: Vec::new(),
        }
    }
}

impl DiagnosticStore {
    fn validate(&self) -> Result<()> {
        if self.schema_version != STORE_SCHEMA_VERSION
            || self.next_sequence == 0
            || self.diagnostics.len() > MAX_RETAINED_DIAGNOSTICS
        {
            return Err(diagnostics_error(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                "the crash diagnostic store has an unsupported or invalid shape",
                "validate_store",
            ));
        }
        let mut ids = BTreeSet::new();
        for diagnostic in &self.diagnostics {
            diagnostic.validate()?;
            if !ids.insert(diagnostic.diagnostic_id.as_str()) {
                return Err(diagnostics_error(
                    ErrorCategory::CorruptData,
                    Recoverability::UserCorrectable,
                    "the crash diagnostic store contains duplicate identities",
                    "validate_store",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ActiveSession {
    schema_version: u32,
    session_id: String,
    started_unix_millis: u64,
    updated_unix_millis: u64,
    continuity: DesktopSessionContinuity,
}

impl ActiveSession {
    fn validate(&self) -> Result<()> {
        if self.schema_version != STORE_SCHEMA_VERSION
            || self.updated_unix_millis < self.started_unix_millis
        {
            return Err(diagnostics_error(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                "the active crash session marker has an invalid shape",
                "validate_session",
            ));
        }
        validate_bounded_text(
            "session id",
            &self.session_id,
            MAX_IDENTITY_BYTES,
            "validate_session",
        )?;
        self.continuity.workspace.validate()?;
        if let Some(project) = &self.continuity.project {
            project.validate()?;
        }
        if let Some(lifecycle) = &self.continuity.lifecycle {
            validate_lifecycle_continuity(lifecycle)?;
        }
        Ok(())
    }
}

struct DiagnosticDraft {
    session_id: String,
    captured_unix_millis: u64,
    failure_class: DesktopCrashFailureClass,
    code: String,
    title: String,
    action: String,
    contexts: Vec<DesktopCrashContext>,
    continuity: DesktopSessionContinuity,
    recovery_entry_point: DesktopCrashRecoveryEntryPoint,
    private_detail: Option<String>,
}

struct DiagnosticsOwner {
    initialized: bool,
    finished: bool,
    persistence_available: bool,
    storage_failure_recorded: bool,
    view_revision: u64,
    current_session_id: String,
    continuity: DesktopSessionContinuity,
    store: DiagnosticStore,
    observed_lifecycle_failure: Option<(u64, u64)>,
}

impl Default for DiagnosticsOwner {
    fn default() -> Self {
        Self {
            initialized: false,
            finished: false,
            persistence_available: false,
            storage_failure_recorded: false,
            view_revision: 0,
            current_session_id: "uninitialized".to_owned(),
            continuity: DesktopSessionContinuity::default(),
            store: DiagnosticStore::default(),
            observed_lifecycle_failure: None,
        }
    }
}

struct DiagnosticsInner {
    root: OnceLock<PathBuf>,
    owner: Mutex<DiagnosticsOwner>,
}

/// Cloneable application owner for local crash diagnostics.
#[derive(Clone)]
pub struct DesktopCrashDiagnostics {
    inner: Arc<DiagnosticsInner>,
}

impl Default for DesktopCrashDiagnostics {
    fn default() -> Self {
        Self {
            inner: Arc::new(DiagnosticsInner {
                root: OnceLock::new(),
                owner: Mutex::new(DiagnosticsOwner::default()),
            }),
        }
    }
}

impl DesktopCrashDiagnostics {
    /// Initializes the private store and begins a new exact process session.
    ///
    /// Storage failure is degraded diagnostic state, not an application startup failure.
    #[must_use]
    pub fn initialize(&self, root: PathBuf) -> DesktopCrashDiagnosticsSnapshot {
        let mut owner = lock_unpoisoned(&self.inner.owner);
        if owner.initialized {
            return project_snapshot(&owner);
        }
        let _ = self.inner.root.set(root.clone());
        let now = unix_millis();
        owner.current_session_id = session_id();
        owner.continuity = DesktopSessionContinuity::default();
        owner.initialized = true;
        owner.finished = false;

        if let Err(error) = fs::create_dir_all(&root) {
            owner.persistence_available = false;
            note_storage_failure(&mut owner, "initialize_directory", error.to_string());
            return project_snapshot(&owner);
        }
        owner.persistence_available = true;

        let store_path = root.join(STORE_FILE);
        owner.store = match read_json::<DiagnosticStore>(&store_path, MAX_STORE_FILE_BYTES) {
            Ok(Some(store)) => match store.validate() {
                Ok(()) => store,
                Err(error) => corrupt_store_replacement(
                    &store_path,
                    now,
                    &owner.current_session_id,
                    &owner.continuity,
                    &error.to_string(),
                ),
            },
            Ok(None) => DiagnosticStore::default(),
            Err(ReadJsonError::Json(error)) => corrupt_store_replacement(
                &store_path,
                now,
                &owner.current_session_id,
                &owner.continuity,
                &error,
            ),
            Err(ReadJsonError::Io(error)) => {
                let mut replacement = DiagnosticStore::default();
                owner.persistence_available = false;
                let session = owner.current_session_id.clone();
                let continuity = owner.continuity.clone();
                let _ = push_diagnostic(
                    &mut replacement,
                    DiagnosticDraft {
                        session_id: session,
                        captured_unix_millis: now,
                        failure_class: DesktopCrashFailureClass::Degraded,
                        code: "diagnostic_storage_unavailable".to_owned(),
                        title: "Crash diagnostics are available in memory only".to_owned(),
                        action:
                            "Continue working and restore access to the application data directory."
                                .to_owned(),
                        contexts: stable_context(COMPONENT, "load_store"),
                        continuity,
                        recovery_entry_point: DesktopCrashRecoveryEntryPoint::ContinueDegraded,
                        private_detail: Some(truncate_private(&error)),
                    },
                );
                owner.storage_failure_recorded = true;
                replacement
            }
        };

        if owner.persistence_available {
            let marker_path = root.join(SESSION_FILE);
            match read_json::<ActiveSession>(&marker_path, MAX_SESSION_FILE_BYTES) {
                Ok(Some(prior)) => match prior.validate() {
                    Ok(()) => {
                        let already_recorded = owner.store.diagnostics.iter().any(|diagnostic| {
                            diagnostic.session_id == prior.session_id
                                && diagnostic.code == "application_panic"
                        });
                        if !already_recorded {
                            let _ = push_diagnostic(
                                &mut owner.store,
                                DiagnosticDraft {
                                    session_id: prior.session_id,
                                    captured_unix_millis: now,
                                    failure_class: DesktopCrashFailureClass::Retryable,
                                    code: "unexpected_exit".to_owned(),
                                    title: "Superi did not finish its prior session".to_owned(),
                                    action: "Restore the retained workspace and review project recovery before resuming edits."
                                        .to_owned(),
                                    contexts: stable_context(COMPONENT, "detect_unclean_session"),
                                    continuity: prior.continuity,
                                    recovery_entry_point:
                                        DesktopCrashRecoveryEntryPoint::RestoreWorkspace,
                                    private_detail: None,
                                },
                            );
                        }
                    }
                    Err(error) => {
                        preserve_corrupt_file(&marker_path, now);
                        let session = owner.current_session_id.clone();
                        let continuity = owner.continuity.clone();
                        let _ = push_diagnostic(
                            &mut owner.store,
                            DiagnosticDraft {
                                session_id: session,
                                captured_unix_millis: now,
                                failure_class: DesktopCrashFailureClass::Degraded,
                                code: "session_marker_corrupt".to_owned(),
                                title: "The prior session marker needs attention".to_owned(),
                                action: "Continue working and review the preserved local marker before removing it."
                                    .to_owned(),
                                contexts: stable_context(COMPONENT, "load_session"),
                                continuity,
                                recovery_entry_point:
                                    DesktopCrashRecoveryEntryPoint::ContinueDegraded,
                                private_detail: Some(truncate_private(&error.to_string())),
                            },
                        );
                    }
                },
                Ok(None) => {}
                Err(ReadJsonError::Json(error)) => {
                    preserve_corrupt_file(&marker_path, now);
                    let session = owner.current_session_id.clone();
                    let continuity = owner.continuity.clone();
                    let _ = push_diagnostic(
                        &mut owner.store,
                        DiagnosticDraft {
                            session_id: session,
                            captured_unix_millis: now,
                            failure_class: DesktopCrashFailureClass::Degraded,
                            code: "session_marker_corrupt".to_owned(),
                            title: "The prior session marker needs attention".to_owned(),
                            action: "Continue working and review the preserved local marker before removing it."
                                .to_owned(),
                            contexts: stable_context(COMPONENT, "load_session"),
                            continuity,
                            recovery_entry_point:
                                DesktopCrashRecoveryEntryPoint::ContinueDegraded,
                            private_detail: Some(truncate_private(&error)),
                        },
                    );
                }
                Err(ReadJsonError::Io(error)) => {
                    note_storage_failure(&mut owner, "load_session", error);
                }
            }
        }

        owner.view_revision = owner.store.revision.saturating_add(1);
        if owner.persistence_available {
            if let Err(error) = persist_store_and_session(&root, &owner, now) {
                note_storage_failure(&mut owner, "begin_session", error);
            }
        }
        project_snapshot(&owner)
    }

    /// Returns the current in-memory safe projection without filesystem I/O.
    #[must_use]
    pub fn snapshot(&self) -> DesktopCrashDiagnosticsSnapshot {
        project_snapshot(&lock_unpoisoned(&self.inner.owner))
    }

    /// Replaces current route and panel continuity after strict validation.
    pub fn update_workspace(
        &self,
        workspace: DesktopWorkspaceContinuity,
    ) -> Result<DesktopCrashDiagnosticsSnapshot> {
        workspace.validate()?;
        let mut owner = lock_unpoisoned(&self.inner.owner);
        require_initialized(&owner, "update_workspace")?;
        if owner.continuity.workspace == workspace {
            return Ok(project_snapshot(&owner));
        }
        owner.continuity.workspace = workspace;
        advance_view_revision(&mut owner)?;
        persist_marker_or_degrade(self.inner.root.get(), &mut owner, "update_workspace");
        Ok(project_snapshot(&owner))
    }

    /// Replaces or clears active project continuity without changing project ownership.
    pub fn update_project(
        &self,
        project: Option<DesktopProjectContinuity>,
    ) -> Result<DesktopCrashDiagnosticsSnapshot> {
        if let Some(project) = &project {
            project.validate()?;
        }
        let mut owner = lock_unpoisoned(&self.inner.owner);
        require_initialized(&owner, "update_project")?;
        if owner.continuity.project == project {
            return Ok(project_snapshot(&owner));
        }
        owner.continuity.project = project;
        advance_view_revision(&mut owner)?;
        persist_marker_or_degrade(self.inner.root.get(), &mut owner, "update_project");
        Ok(project_snapshot(&owner))
    }

    /// Records one distinct classified lifecycle failure and refreshes the active marker.
    pub fn observe_lifecycle(
        &self,
        lifecycle: &DesktopLifecycleSnapshot,
    ) -> Result<DesktopCrashDiagnosticsSnapshot> {
        let mut owner = lock_unpoisoned(&self.inner.owner);
        require_initialized(&owner, "observe_lifecycle")?;
        owner.continuity.lifecycle = Some(DesktopLifecycleContinuity {
            revision: lifecycle.revision(),
            engine_generation: lifecycle.engine_generation(),
            application_phase: application_phase_code(lifecycle.application_phase()).to_owned(),
        });
        let mut store_changed = false;
        if let Some(failure) = lifecycle.failure() {
            let key = (lifecycle.engine_generation(), lifecycle.revision());
            if owner.observed_lifecycle_failure != Some(key) {
                let recoverability = Recoverability::from_code(failure.recoverability())
                    .ok_or_else(|| {
                        diagnostics_error(
                            ErrorCategory::CorruptData,
                            Recoverability::Terminal,
                            "the lifecycle exposed an unknown recovery classification",
                            "observe_lifecycle",
                        )
                    })?;
                let failure_class = DesktopCrashFailureClass::from(recoverability);
                let contexts = if failure.contexts().is_empty() {
                    stable_context(COMPONENT, "lifecycle_failure")
                } else {
                    failure
                        .contexts()
                        .iter()
                        .take(MAX_CONTEXTS)
                        .map(|context| {
                            DesktopCrashContext::new(context.component(), context.operation())
                        })
                        .collect::<Result<Vec<_>>>()?
                };
                let session = owner.current_session_id.clone();
                let continuity = owner.continuity.clone();
                push_diagnostic(
                    &mut owner.store,
                    DiagnosticDraft {
                        session_id: session,
                        captured_unix_millis: unix_millis(),
                        failure_class,
                        code: "lifecycle_failure".to_owned(),
                        title: failure.summary().to_owned(),
                        action: recovery_action(failure_class).to_owned(),
                        contexts,
                        continuity,
                        recovery_entry_point: recovery_entry_point(failure_class),
                        private_detail: None,
                    },
                )?;
                owner.observed_lifecycle_failure = Some(key);
                store_changed = true;
            }
        }
        advance_view_revision(&mut owner)?;
        if let Some(root) = self
            .inner
            .root
            .get()
            .filter(|_| owner.persistence_available)
        {
            let now = unix_millis();
            let result = if store_changed {
                persist_store_and_session(root, &owner, now)
            } else {
                write_active_session(root, &owner, now)
            };
            if let Err(error) = result {
                note_storage_failure(&mut owner, "observe_lifecycle", error);
            }
        }
        Ok(project_snapshot(&owner))
    }

    /// Retains one bounded private panic detail while returning only its safe projection.
    pub fn record_panic(
        &self,
        detail: &str,
        location: Option<&str>,
    ) -> Result<DesktopCrashDiagnosticsSnapshot> {
        let mut owner = lock_unpoisoned(&self.inner.owner);
        require_initialized(&owner, "record_panic")?;
        record_panic_locked(self.inner.root.get(), &mut owner, detail, location)?;
        Ok(project_snapshot(&owner))
    }

    /// Installs one process-wide chained hook for best-effort private panic capture.
    pub fn install_panic_hook(&self) {
        let diagnostics = self.clone();
        INSTALL_PANIC_HOOK.call_once(move || {
            let prior = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |information| {
                let detail = information
                    .payload()
                    .downcast_ref::<&str>()
                    .map(|value| (*value).to_owned())
                    .or_else(|| information.payload().downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "non-text panic payload".to_owned());
                let location = information.location().map(|location| {
                    format!(
                        "{}:{}:{}",
                        location.file(),
                        location.line(),
                        location.column()
                    )
                });
                diagnostics.record_panic_best_effort(&detail, location.as_deref());
                prior(information);
            }));
        });
    }

    /// Removes one retained diagnostic after exact identity validation.
    pub fn dismiss(&self, diagnostic_id: &str) -> Result<DesktopCrashDiagnosticsSnapshot> {
        validate_bounded_text(
            "diagnostic id",
            diagnostic_id,
            MAX_PROJECT_PATH_BYTES,
            "dismiss",
        )?;
        let mut owner = lock_unpoisoned(&self.inner.owner);
        require_initialized(&owner, "dismiss")?;
        let Some(index) = owner
            .store
            .diagnostics
            .iter()
            .position(|diagnostic| diagnostic.diagnostic_id == diagnostic_id)
        else {
            return Err(diagnostics_error(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "the selected crash diagnostic is no longer retained",
                "dismiss",
            ));
        };
        owner.store.diagnostics.remove(index);
        owner.store.revision = owner.store.revision.checked_add(1).ok_or_else(|| {
            diagnostics_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::Terminal,
                "the crash diagnostic revision space is exhausted",
                "dismiss",
            )
        })?;
        advance_view_revision(&mut owner)?;
        if let Some(root) = self
            .inner
            .root
            .get()
            .filter(|_| owner.persistence_available)
        {
            if let Err(error) = persist_store_and_session(root, &owner, unix_millis()) {
                note_storage_failure(&mut owner, "dismiss", error);
            }
        }
        Ok(project_snapshot(&owner))
    }

    /// Removes only this exact active-session marker after orderly shutdown completes.
    pub fn finish_session(&self) -> Result<()> {
        let mut owner = lock_unpoisoned(&self.inner.owner);
        require_initialized(&owner, "finish_session")?;
        if owner.finished {
            return Ok(());
        }
        let Some(root) = self.inner.root.get() else {
            owner.finished = true;
            return Ok(());
        };
        let path = root.join(SESSION_FILE);
        let Some(marker) =
            read_json::<ActiveSession>(&path, MAX_SESSION_FILE_BYTES).map_err(|detail| {
                diagnostics_error(
                    ErrorCategory::CorruptData,
                    Recoverability::UserCorrectable,
                    "the active session marker could not be verified for removal",
                    "finish_session",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "read_marker")
                        .with_field("detail", detail.to_string()),
                )
            })?
        else {
            owner.finished = true;
            return Ok(());
        };
        if marker.session_id != owner.current_session_id {
            return Err(diagnostics_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "another session owns the active crash marker",
                "finish_session",
            ));
        }
        fs::remove_file(&path).map_err(|source| {
            Error::with_source(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "the active crash session marker could not be removed",
                source,
            )
            .with_context(ErrorContext::new(COMPONENT, "finish_session"))
        })?;
        owner.finished = true;
        Ok(())
    }

    fn record_panic_best_effort(&self, detail: &str, location: Option<&str>) {
        match self.inner.owner.try_lock() {
            Ok(mut owner) => {
                let _ = record_panic_locked(self.inner.root.get(), &mut owner, detail, location);
            }
            Err(TryLockError::Poisoned(poisoned)) => {
                let mut owner = poisoned.into_inner();
                let _ = record_panic_locked(self.inner.root.get(), &mut owner, detail, location);
            }
            Err(TryLockError::WouldBlock) => {}
        }
    }
}

fn record_panic_locked(
    root: Option<&PathBuf>,
    owner: &mut DiagnosticsOwner,
    detail: &str,
    location: Option<&str>,
) -> Result<()> {
    let private_detail = match location {
        Some(location) => format!("{detail}\nlocation: {location}"),
        None => detail.to_owned(),
    };
    let session = owner.current_session_id.clone();
    let continuity = owner.continuity.clone();
    push_diagnostic(
        &mut owner.store,
        DiagnosticDraft {
            session_id: session,
            captured_unix_millis: unix_millis(),
            failure_class: DesktopCrashFailureClass::Terminal,
            code: "application_panic".to_owned(),
            title: "Superi stopped unexpectedly".to_owned(),
            action:
                "Start a fresh application lifetime, then restore the retained workspace and review project recovery."
                    .to_owned(),
            contexts: stable_context(COMPONENT, "panic_hook"),
            continuity,
            recovery_entry_point: DesktopCrashRecoveryEntryPoint::RestoreWorkspace,
            private_detail: Some(truncate_private(&private_detail)),
        },
    )?;
    advance_view_revision(owner)?;
    if let Some(root) = root.filter(|_| owner.persistence_available) {
        if let Err(error) = write_json_atomic(&root.join(STORE_FILE), &owner.store) {
            note_storage_failure(owner, "record_panic", error);
        }
    }
    Ok(())
}

fn push_diagnostic(store: &mut DiagnosticStore, draft: DiagnosticDraft) -> Result<()> {
    validate_bounded_text(
        "session id",
        &draft.session_id,
        MAX_IDENTITY_BYTES,
        "append",
    )?;
    validate_bounded_text("diagnostic code", &draft.code, MAX_CODE_BYTES, "append")?;
    validate_bounded_text("diagnostic title", &draft.title, MAX_TITLE_BYTES, "append")?;
    validate_bounded_text(
        "diagnostic action",
        &draft.action,
        MAX_ACTION_BYTES,
        "append",
    )?;
    if draft.contexts.len() > MAX_CONTEXTS {
        return Err(diagnostics_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "a crash diagnostic contains too many context frames",
            "append",
        ));
    }
    let sequence = store.next_sequence;
    store.next_sequence = sequence.checked_add(1).ok_or_else(|| {
        diagnostics_error(
            ErrorCategory::ResourceExhausted,
            Recoverability::Terminal,
            "the crash diagnostic identity space is exhausted",
            "append",
        )
    })?;
    store.revision = store.revision.checked_add(1).ok_or_else(|| {
        diagnostics_error(
            ErrorCategory::ResourceExhausted,
            Recoverability::Terminal,
            "the crash diagnostic revision space is exhausted",
            "append",
        )
    })?;
    store.diagnostics.push(StoredDiagnostic {
        diagnostic_id: format!("crash:{}:{sequence}", draft.session_id),
        session_id: draft.session_id,
        captured_unix_millis: draft.captured_unix_millis,
        failure_class: draft.failure_class,
        code: draft.code,
        title: draft.title,
        action: draft.action,
        contexts: draft.contexts,
        continuity: draft.continuity,
        recovery_entry_point: draft.recovery_entry_point,
        private_detail: draft.private_detail,
    });
    if store.diagnostics.len() > MAX_RETAINED_DIAGNOSTICS {
        let excess = store.diagnostics.len() - MAX_RETAINED_DIAGNOSTICS;
        store.diagnostics.drain(0..excess);
    }
    Ok(())
}

fn persist_marker_or_degrade(
    root: Option<&PathBuf>,
    owner: &mut DiagnosticsOwner,
    operation: &'static str,
) {
    if let Some(root) = root.filter(|_| owner.persistence_available) {
        if let Err(error) = write_active_session(root, owner, unix_millis()) {
            note_storage_failure(owner, operation, error);
        }
    }
}

fn persist_store_and_session(
    root: &Path,
    owner: &DiagnosticsOwner,
    now: u64,
) -> std::result::Result<(), String> {
    write_json_atomic(&root.join(STORE_FILE), &owner.store)?;
    write_active_session(root, owner, now)
}

fn write_active_session(
    root: &Path,
    owner: &DiagnosticsOwner,
    now: u64,
) -> std::result::Result<(), String> {
    let started = read_json::<ActiveSession>(&root.join(SESSION_FILE), MAX_SESSION_FILE_BYTES)
        .ok()
        .flatten()
        .filter(|marker| marker.session_id == owner.current_session_id)
        .map_or(now, |marker| marker.started_unix_millis);
    write_json_atomic(
        &root.join(SESSION_FILE),
        &ActiveSession {
            schema_version: STORE_SCHEMA_VERSION,
            session_id: owner.current_session_id.clone(),
            started_unix_millis: started,
            updated_unix_millis: now,
            continuity: owner.continuity.clone(),
        },
    )
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> std::result::Result<(), String> {
    let bytes = serde_json::to_vec_pretty(value).map_err(|error| error.to_string())?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("diagnostics");
    let temporary = path.with_file_name(format!(
        ".{file_name}.tmp-{}-{}",
        std::process::id(),
        unix_nanos()
    ));
    let result = (|| {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary)
            .map_err(|error| error.to_string())?;
        file.write_all(&bytes).map_err(|error| error.to_string())?;
        file.sync_all().map_err(|error| error.to_string())?;
        drop(file);
        publish_temporary(&temporary, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn publish_temporary(temporary: &Path, path: &Path) -> std::result::Result<(), String> {
    match fs::rename(temporary, path) {
        Ok(()) => {
            remove_stale_backup(path);
            sync_parent_best_effort(path);
            Ok(())
        }
        Err(initial) if path.is_file() => {
            let backup = atomic_backup_path(path);
            if backup.exists() {
                fs::remove_file(&backup).map_err(|error| {
                    format!(
                        "atomic replacement could not remove stale backup after {initial}: {error}"
                    )
                })?;
            }
            fs::rename(path, &backup).map_err(|error| {
                format!(
                    "atomic replacement could not preserve the prior file after {initial}: {error}"
                )
            })?;
            match fs::rename(temporary, path) {
                Ok(()) => {
                    let _ = fs::remove_file(&backup);
                    sync_parent_best_effort(path);
                    Ok(())
                }
                Err(error) => {
                    let restoration = fs::rename(&backup, path);
                    Err(match restoration {
                        Ok(()) => format!(
                            "atomic replacement failed after preserving and restoring the prior file: {error}"
                        ),
                        Err(restore_error) => format!(
                            "atomic replacement failed and left the prior file in {}: {error}; restoration failed: {restore_error}",
                            backup.display()
                        ),
                    })
                }
            }
        }
        Err(error) => Err(error.to_string()),
    }
}

fn atomic_backup_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("diagnostics");
    path.with_file_name(format!(".{file_name}.previous"))
}

fn recover_interrupted_publication(path: &Path) -> std::result::Result<(), String> {
    match fs::metadata(path) {
        Ok(_) => {
            remove_stale_backup(path);
            return Ok(());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.to_string()),
    }
    let backup = atomic_backup_path(path);
    match fs::rename(&backup, path) {
        Ok(()) => {
            sync_parent_best_effort(path);
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}

fn remove_stale_backup(path: &Path) {
    let backup = atomic_backup_path(path);
    if backup.is_file() {
        let _ = fs::remove_file(backup);
    }
}

fn sync_parent_best_effort(path: &Path) {
    if let Some(parent) = path.parent() {
        if let Ok(directory) = fs::File::open(parent) {
            let _ = directory.sync_all();
        }
    }
}

fn corrupt_store_replacement(
    store_path: &Path,
    now: u64,
    session_id: &str,
    continuity: &DesktopSessionContinuity,
    detail: &str,
) -> DiagnosticStore {
    preserve_corrupt_file(store_path, now);
    let mut replacement = DiagnosticStore::default();
    let _ = push_diagnostic(
        &mut replacement,
        DiagnosticDraft {
            session_id: session_id.to_owned(),
            captured_unix_millis: now,
            failure_class: DesktopCrashFailureClass::Degraded,
            code: "diagnostic_store_corrupt".to_owned(),
            title: "Previous crash diagnostics need attention".to_owned(),
            action: "Continue working, then review the preserved local diagnostic store."
                .to_owned(),
            contexts: stable_context(COMPONENT, "load_store"),
            continuity: continuity.clone(),
            recovery_entry_point: DesktopCrashRecoveryEntryPoint::ContinueDegraded,
            private_detail: Some(truncate_private(detail)),
        },
    );
    replacement
}

#[derive(Debug)]
enum ReadJsonError {
    Io(String),
    Json(String),
}

impl fmt::Display for ReadJsonError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(detail) | Self::Json(detail) => formatter.write_str(detail),
        }
    }
}

fn read_json<T>(path: &Path, maximum_bytes: usize) -> std::result::Result<Option<T>, ReadJsonError>
where
    T: for<'de> Deserialize<'de>,
{
    recover_interrupted_publication(path).map_err(ReadJsonError::Io)?;
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(ReadJsonError::Io(error.to_string())),
    };
    let limit = u64::try_from(maximum_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut bytes = Vec::new();
    Read::by_ref(&mut file)
        .take(limit)
        .read_to_end(&mut bytes)
        .map_err(|error| ReadJsonError::Io(error.to_string()))?;
    if bytes.len() > maximum_bytes {
        return Err(ReadJsonError::Json(format!(
            "the local diagnostic file exceeds its {maximum_bytes}-byte limit"
        )));
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|error| ReadJsonError::Json(error.to_string()))
}

fn preserve_corrupt_file(path: &Path, now: u64) {
    if path.exists() {
        let archive = path.with_file_name(format!(
            "{}.corrupt-{now}-{}-{}",
            path.file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("diagnostics"),
            std::process::id(),
            unix_nanos()
        ));
        let _ = fs::copy(path, archive);
    }
}

fn note_storage_failure(owner: &mut DiagnosticsOwner, operation: &'static str, detail: String) {
    owner.persistence_available = false;
    if owner.storage_failure_recorded {
        return;
    }
    owner.storage_failure_recorded = true;
    let session = owner.current_session_id.clone();
    let continuity = owner.continuity.clone();
    let _ = push_diagnostic(
        &mut owner.store,
        DiagnosticDraft {
            session_id: session,
            captured_unix_millis: unix_millis(),
            failure_class: DesktopCrashFailureClass::Degraded,
            code: "diagnostic_storage_unavailable".to_owned(),
            title: "Crash diagnostics are available in memory only".to_owned(),
            action: "Continue working and restore access to the application data directory."
                .to_owned(),
            contexts: stable_context(COMPONENT, operation),
            continuity,
            recovery_entry_point: DesktopCrashRecoveryEntryPoint::ContinueDegraded,
            private_detail: Some(truncate_private(&detail)),
        },
    );
    owner.view_revision = owner.view_revision.saturating_add(1);
}

fn project_snapshot(owner: &DiagnosticsOwner) -> DesktopCrashDiagnosticsSnapshot {
    DesktopCrashDiagnosticsSnapshot {
        revision: owner.view_revision,
        current_session_id: owner.current_session_id.clone(),
        persistence_available: owner.persistence_available,
        continuity: owner.continuity.clone(),
        diagnostics: owner
            .store
            .diagnostics
            .iter()
            .map(StoredDiagnostic::public)
            .collect(),
    }
}

fn require_initialized(owner: &DiagnosticsOwner, operation: &'static str) -> Result<()> {
    if owner.initialized {
        Ok(())
    } else {
        Err(diagnostics_error(
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "crash diagnostics have not been initialized",
            operation,
        ))
    }
}

fn advance_view_revision(owner: &mut DiagnosticsOwner) -> Result<()> {
    owner.view_revision = owner.view_revision.checked_add(1).ok_or_else(|| {
        diagnostics_error(
            ErrorCategory::ResourceExhausted,
            Recoverability::Terminal,
            "the crash diagnostic view revision space is exhausted",
            "advance_revision",
        )
    })?;
    Ok(())
}

fn recovery_entry_point(class: DesktopCrashFailureClass) -> DesktopCrashRecoveryEntryPoint {
    match class {
        DesktopCrashFailureClass::Retryable => DesktopCrashRecoveryEntryPoint::RetryEngine,
        DesktopCrashFailureClass::Degraded => DesktopCrashRecoveryEntryPoint::ContinueDegraded,
        DesktopCrashFailureClass::UserCorrectable => {
            DesktopCrashRecoveryEntryPoint::ReviewProjectRecovery
        }
        DesktopCrashFailureClass::Terminal => DesktopCrashRecoveryEntryPoint::RestartEngine,
    }
}

fn recovery_action(class: DesktopCrashFailureClass) -> &'static str {
    match class {
        DesktopCrashFailureClass::Retryable => {
            "Retry the engine operation without changing the retained workspace or project."
        }
        DesktopCrashFailureClass::Degraded => {
            "Continue unaffected work, or restart the engine when the degraded feature is needed."
        }
        DesktopCrashFailureClass::UserCorrectable => {
            "Review the project recovery options or correct the reported input before retrying."
        }
        DesktopCrashFailureClass::Terminal => {
            "Restart the engine in a fresh lifetime before continuing project work."
        }
    }
}

fn application_phase_code(phase: ApplicationLifecyclePhase) -> &'static str {
    match phase {
        ApplicationLifecyclePhase::Starting => "starting",
        ApplicationLifecyclePhase::Running => "running",
        ApplicationLifecyclePhase::Suspending => "suspending",
        ApplicationLifecyclePhase::Suspended => "suspended",
        ApplicationLifecyclePhase::Resuming => "resuming",
        ApplicationLifecyclePhase::Stopping => "stopping",
        ApplicationLifecyclePhase::Restarting => "restarting",
        ApplicationLifecyclePhase::Recovering => "recovering",
        ApplicationLifecyclePhase::Failed => "failed",
        ApplicationLifecyclePhase::Stopped => "stopped",
    }
}

fn validate_lifecycle_continuity(lifecycle: &DesktopLifecycleContinuity) -> Result<()> {
    if !matches!(
        lifecycle.application_phase.as_str(),
        "starting"
            | "running"
            | "suspending"
            | "suspended"
            | "resuming"
            | "stopping"
            | "restarting"
            | "recovering"
            | "failed"
            | "stopped"
    ) {
        return Err(diagnostics_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "the crash diagnostic store contains an unknown lifecycle phase",
            "validate_lifecycle",
        ));
    }
    Ok(())
}

fn stable_context(component: &str, operation: &str) -> Vec<DesktopCrashContext> {
    DesktopCrashContext::new(component, operation)
        .into_iter()
        .collect()
}

fn validate_application_identity(field: &'static str, value: &str) -> Result<()> {
    validate_bounded_text(field, value, MAX_IDENTITY_BYTES, "validate_identity")?;
    if !value.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    }) {
        return Err(diagnostics_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "workspace continuity contains a malformed application identity",
            "validate_identity",
        ));
    }
    Ok(())
}

fn validate_bounded_text(
    field: &'static str,
    value: &str,
    maximum: usize,
    operation: &'static str,
) -> Result<()> {
    if value.is_empty() || value.trim() != value || value.len() > maximum {
        return Err(diagnostics_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "crash diagnostic text is empty, untrimmed, or oversized",
            operation,
        )
        .with_context(
            ErrorContext::new(COMPONENT, "bounded_text")
                .with_field("field", field)
                .with_field("maximum_bytes", maximum.to_string()),
        ));
    }
    Ok(())
}

fn truncate_private(value: &str) -> String {
    if value.len() <= MAX_PRIVATE_DETAIL_BYTES {
        return value.to_owned();
    }
    let mut boundary = MAX_PRIVATE_DETAIL_BYTES;
    while !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    value[..boundary].to_owned()
}

fn session_id() -> String {
    format!("session-{}-{}", std::process::id(), unix_nanos())
}

fn unix_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

fn unix_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
}

fn diagnostics_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_store_evicts_oldest_diagnostic() {
        let mut store = DiagnosticStore::default();
        for index in 0..(MAX_RETAINED_DIAGNOSTICS + 3) {
            push_diagnostic(
                &mut store,
                DiagnosticDraft {
                    session_id: "session-test".to_owned(),
                    captured_unix_millis: index as u64,
                    failure_class: DesktopCrashFailureClass::Retryable,
                    code: format!("failure_{index}"),
                    title: "Retryable failure".to_owned(),
                    action: "Retry the operation.".to_owned(),
                    contexts: stable_context(COMPONENT, "test"),
                    continuity: DesktopSessionContinuity::default(),
                    recovery_entry_point: DesktopCrashRecoveryEntryPoint::RetryEngine,
                    private_detail: None,
                },
            )
            .unwrap();
        }
        assert_eq!(store.diagnostics.len(), MAX_RETAINED_DIAGNOSTICS);
        assert_eq!(store.diagnostics[0].code, "failure_3");
    }

    #[test]
    fn workspace_validation_rejects_hidden_focus_and_duplicates() {
        assert!(DesktopWorkspaceContinuity::new(
            "editing",
            ["workspace.editing"],
            Some("workspace.editing")
        )
        .is_err());
        assert!(DesktopWorkspaceContinuity::new(
            "editing",
            ["application.selection", "application.selection"],
            Some("workspace.editing")
        )
        .is_err());
    }
}
