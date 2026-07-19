//! Durable local project hosting for headless API consumers.
//!
//! This adapter owns no authored state. Each call loads one project through the project database,
//! executes existing typed public commands through a temporary EngineControl dispatcher, and
//! completes required durable publication before returning a success value.

use std::path::Path;
use std::str::FromStr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_engine::editor as engine;

use crate::commands::{
    CompareProjectRecovery, CompareProjectRecoveryResult, DismissProjectRecovery,
    DismissProjectRecoveryResult, ExecuteProjectSettingsTransaction,
    ExecuteProjectSettingsTransactionResult, GetEditorState, GetEditorStateResult,
    GetProjectRecovery, GetProjectRecoveryResult, GetProjectSettings, GetProjectSettingsResult,
    RestoreProjectRecovery, RestoreProjectRecoveryResult,
};
use crate::editor::{
    ExecuteProjectCommand, ExecuteProjectCommandResult, GetProjectCommandLog,
    GetProjectCommandLogResult, ProjectAction, ProjectCommand, ProjectEditorApi,
};
use crate::events::{ProjectRecoveryChanged, ProjectSettingsChanged, ProjectStateChanged};
use crate::permissions::ApiPermissionContext;
use crate::project::{ProjectSettingMutation, ProjectSettingsApi};
use crate::recovery::ProjectRecoveryApi;
use crate::scripting::{RunProjectScript, RunProjectScriptResult};

const COMPONENT: &str = "superi-api.local";
const RENDER_SETTING_PREFIX: &str = "superi.project.render.";

/// Strict request for creating one minimal durable project.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalProjectCreateRequest {
    pub project_id: String,
    pub project_name: String,
    pub root_timeline_id: String,
    pub root_timeline_name: String,
    pub edit_rate_numerator: u32,
    pub edit_rate_denominator: u32,
}

/// Stable summary of one completely loaded project snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalProjectSummary {
    project_id: String,
    project_revision: u64,
    root_timeline_id: String,
}

impl LocalProjectSummary {
    fn from_snapshot(snapshot: &engine::ProjectSnapshot) -> Self {
        Self {
            project_id: snapshot.project_id().to_string(),
            project_revision: snapshot.revision(),
            root_timeline_id: snapshot.root_timeline_id().to_string(),
        }
    }

    #[must_use]
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    #[must_use]
    pub fn root_timeline_id(&self) -> &str {
        &self.root_timeline_id
    }
}

/// Read-only validation evidence returned after complete current-schema reconstruction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalProjectValidation {
    valid: bool,
    schema_revision: u32,
    project: LocalProjectSummary,
}

/// Coherent project editor state and authoritative render settings from one loaded revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalRenderInspection {
    editor: GetEditorStateResult,
    settings: GetProjectSettingsResult,
}

impl LocalRenderInspection {
    #[must_use]
    pub const fn editor(&self) -> &GetEditorStateResult {
        &self.editor
    }

    #[must_use]
    pub const fn settings(&self) -> &GetProjectSettingsResult {
        &self.settings
    }
}

impl LocalProjectValidation {
    #[must_use]
    pub const fn valid(&self) -> bool {
        self.valid
    }

    #[must_use]
    pub const fn schema_revision(&self) -> u32 {
        self.schema_revision
    }

    #[must_use]
    pub const fn project(&self) -> &LocalProjectSummary {
        &self.project
    }
}

/// Explicit collision policy for a local save-copy command.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LocalProjectCollision {
    RequireAbsent,
    ReplaceExisting,
}

impl LocalProjectCollision {
    const fn into_engine(self) -> engine::ProjectDestinationCollision {
        match self {
            Self::RequireAbsent => engine::ProjectDestinationCollision::RequireAbsent,
            Self::ReplaceExisting => engine::ProjectDestinationCollision::ReplaceExisting,
        }
    }
}

/// Safe process-facing evidence for a completed project publication.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalProjectSaveResult {
    operation: String,
    destination_display: String,
    active_path_display: Option<String>,
    replaced_existing: bool,
}

impl LocalProjectSaveResult {
    fn from_engine(outcome: &engine::ProjectSaveOutcome) -> Self {
        let operation = match outcome.operation() {
            engine::ProjectSaveOperation::Save => "save",
            engine::ProjectSaveOperation::SaveAs => "save_as",
            engine::ProjectSaveOperation::SaveCopy => "save_copy",
            engine::ProjectSaveOperation::Backup => "backup",
            _ => "unknown",
        };
        Self {
            operation: operation.to_owned(),
            destination_display: outcome.destination().to_string_lossy().into_owned(),
            active_path_display: outcome
                .active_path()
                .map(|path| path.to_string_lossy().into_owned()),
            replaced_existing: outcome.replaced_existing(),
        }
    }

    #[must_use]
    pub fn operation(&self) -> &str {
        &self.operation
    }

    #[must_use]
    pub const fn replaced_existing(&self) -> bool {
        self.replaced_existing
    }
}

/// One typed response plus every correlated replacement event drained before return.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalProjectExecution<R, E> {
    result: R,
    events: Vec<E>,
}

impl<R, E> LocalProjectExecution<R, E> {
    const fn new(result: R, events: Vec<E>) -> Self {
        Self { result, events }
    }

    #[must_use]
    pub const fn result(&self) -> &R {
        &self.result
    }

    #[must_use]
    pub fn events(&self) -> &[E] {
        &self.events
    }
}

/// Caller-owned JSON-RPC identifier echoed without coercion in the response.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum LocalAutomationId {
    String(String),
    Number(serde_json::Number),
}

impl LocalAutomationId {
    fn validate(&self) -> Result<()> {
        match self {
            Self::String(_) => Ok(()),
            Self::Number(value) if value.is_i64() || value.is_u64() => Ok(()),
            Self::Number(_) => Err(invalid(
                "execute_automation",
                "JSON-RPC identifiers must not use fractional numbers",
            )),
        }
    }
}

/// One strict JSON-RPC request in the local automation JSONL stream.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "method", deny_unknown_fields)]
pub enum LocalAutomationRequest {
    #[serde(rename = "superi.project.command.execute")]
    ProjectCommand {
        jsonrpc: String,
        id: LocalAutomationId,
        params: ExecuteProjectCommand,
    },
    #[serde(rename = "superi.project.settings.transaction.execute")]
    ProjectSettingsTransaction {
        jsonrpc: String,
        id: LocalAutomationId,
        params: ExecuteProjectSettingsTransaction,
    },
    #[serde(rename = "superi.editor.state.get")]
    EditorState {
        jsonrpc: String,
        id: LocalAutomationId,
        params: GetEditorState,
    },
    #[serde(rename = "superi.project.settings.get")]
    ProjectSettings {
        jsonrpc: String,
        id: LocalAutomationId,
        params: GetProjectSettings,
    },
    #[serde(rename = "superi.project.script.run")]
    ProjectScript {
        jsonrpc: String,
        id: LocalAutomationId,
        params: RunProjectScript,
    },
    #[serde(rename = "superi.project.command_log.get")]
    ProjectCommandLog {
        jsonrpc: String,
        id: LocalAutomationId,
        params: GetProjectCommandLog,
    },
}

enum LocalAutomationCall {
    ProjectCommand(ExecuteProjectCommand),
    ProjectSettingsTransaction(ExecuteProjectSettingsTransaction),
    EditorState(GetEditorState),
    ProjectSettings(GetProjectSettings),
    ProjectScript(RunProjectScript),
    ProjectCommandLog(GetProjectCommandLog),
}

impl LocalAutomationRequest {
    fn into_call(self) -> Result<(LocalAutomationId, LocalAutomationCall)> {
        let (jsonrpc, id, call) = match self {
            Self::ProjectCommand {
                jsonrpc,
                id,
                params,
            } => (jsonrpc, id, LocalAutomationCall::ProjectCommand(params)),
            Self::ProjectSettingsTransaction {
                jsonrpc,
                id,
                params,
            } => (
                jsonrpc,
                id,
                LocalAutomationCall::ProjectSettingsTransaction(params),
            ),
            Self::EditorState {
                jsonrpc,
                id,
                params,
            } => (jsonrpc, id, LocalAutomationCall::EditorState(params)),
            Self::ProjectSettings {
                jsonrpc,
                id,
                params,
            } => (jsonrpc, id, LocalAutomationCall::ProjectSettings(params)),
            Self::ProjectScript {
                jsonrpc,
                id,
                params,
            } => (jsonrpc, id, LocalAutomationCall::ProjectScript(params)),
            Self::ProjectCommandLog {
                jsonrpc,
                id,
                params,
            } => (jsonrpc, id, LocalAutomationCall::ProjectCommandLog(params)),
        };
        if jsonrpc != "2.0" {
            return Err(invalid(
                "execute_automation",
                "automation requests require JSON-RPC version 2.0",
            ));
        }
        id.validate()?;
        Ok((id, call))
    }
}

/// Typed JSON-RPC result payload from one local automation request.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum LocalAutomationResult {
    ProjectCommand(LocalProjectExecution<ExecuteProjectCommandResult, ProjectStateChanged>),
    ProjectSettingsTransaction(
        LocalProjectExecution<ExecuteProjectSettingsTransactionResult, ProjectSettingsChanged>,
    ),
    EditorState(Box<GetEditorStateResult>),
    ProjectSettings(GetProjectSettingsResult),
    ProjectScript(Box<LocalProjectExecution<RunProjectScriptResult, ProjectStateChanged>>),
    ProjectCommandLog(GetProjectCommandLogResult),
}

/// One successful JSON-RPC response with the exact caller identifier.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalAutomationResponse {
    jsonrpc: String,
    id: LocalAutomationId,
    result: LocalAutomationResult,
}

impl LocalAutomationResponse {
    #[must_use]
    pub const fn id(&self) -> &LocalAutomationId {
        &self.id
    }

    #[must_use]
    pub const fn result(&self) -> &LocalAutomationResult {
        &self.result
    }
}

/// Stateless durable adapter used by the local CLI and headless scripting hosts.
#[derive(Clone, Copy, Debug, Default)]
pub struct LocalProjectHost;

impl LocalProjectHost {
    /// Creates one current-schema project with no partially initialized destination on failure.
    pub fn create(
        project_path: impl AsRef<Path>,
        request: LocalProjectCreateRequest,
    ) -> Result<LocalProjectSummary> {
        let project_id = parse_id(&request.project_id, "project_id")?;
        let root_timeline_id = parse_id(&request.root_timeline_id, "root_timeline_id")?;
        let edit_rate =
            engine::Timebase::new(request.edit_rate_numerator, request.edit_rate_denominator)?;
        let root = engine::Timeline::new(
            root_timeline_id,
            request.root_timeline_name,
            edit_rate,
            engine::RationalTime::zero(edit_rate),
            Vec::new(),
        );
        let editorial =
            engine::EditorialProject::new(project_id, request.project_name, [], [root])?;
        let document = engine::ProjectDocument::new(editorial, root_timeline_id)?;
        let snapshot = document.snapshot();
        let summary = LocalProjectSummary::from_snapshot(&snapshot);
        let mut database = engine::ProjectDatabase::memory()?;
        database.replace(&snapshot)?;
        database.execute_save_command(
            engine::ProjectSaveCommand::SaveAs {
                destination: project_path.as_ref().to_path_buf(),
                collision: engine::ProjectDestinationCollision::RequireAbsent,
            },
            &snapshot,
        )?;
        Ok(summary)
    }

    /// Captures one complete editor replacement state without changing the project file.
    pub fn inspect_editor(
        project_path: impl AsRef<Path>,
        request: GetEditorState,
    ) -> Result<GetEditorStateResult> {
        let database = engine::ProjectDatabase::open_read_only(project_path)?;
        let document = database.load()?;
        engine::EngineCommandDispatcher::with_standalone_engine_control(|mut dispatcher| {
            dispatcher.attach_project(document)?;
            ProjectEditorApi::new(dispatcher)?.execute(request)
        })
    }

    /// Reads one bounded command-log batch through the public project API.
    pub fn inspect_command_log(
        project_path: impl AsRef<Path>,
        request: GetProjectCommandLog,
        permissions: Arc<ApiPermissionContext>,
    ) -> Result<GetProjectCommandLogResult> {
        let database = engine::ProjectDatabase::open_read_only(project_path)?;
        let document = database.load()?;
        engine::EngineCommandDispatcher::with_standalone_engine_control(|mut dispatcher| {
            dispatcher.attach_project(document)?;
            ProjectEditorApi::new_with_permissions(dispatcher, permissions)?.execute(request)
        })
    }

    /// Executes any current generic project command and persists authored changes before return.
    pub fn execute_project(
        project_path: impl AsRef<Path>,
        request: ExecuteProjectCommand,
        permissions: Arc<ApiPermissionContext>,
    ) -> Result<LocalProjectExecution<ExecuteProjectCommandResult, ProjectStateChanged>> {
        execute_project_command(project_path.as_ref(), request, permissions)
    }

    /// Executes only media-owned actions through the generic project command contract.
    pub fn execute_media(
        project_path: impl AsRef<Path>,
        request: ExecuteProjectCommand,
        permissions: Arc<ApiPermissionContext>,
    ) -> Result<LocalProjectExecution<ExecuteProjectCommandResult, ProjectStateChanged>> {
        require_media_command(&request)?;
        execute_project_command(project_path.as_ref(), request, permissions)
    }

    /// Executes only timeline-owned apply actions.
    pub fn execute_timeline(
        project_path: impl AsRef<Path>,
        request: ExecuteProjectCommand,
        permissions: Arc<ApiPermissionContext>,
    ) -> Result<LocalProjectExecution<ExecuteProjectCommandResult, ProjectStateChanged>> {
        require_timeline_command(&request)?;
        execute_project_command(project_path.as_ref(), request, permissions)
    }

    /// Returns current project settings without changing the project file.
    pub fn inspect_settings(
        project_path: impl AsRef<Path>,
        request: GetProjectSettings,
    ) -> Result<GetProjectSettingsResult> {
        let database = engine::ProjectDatabase::open_read_only(project_path)?;
        let document = database.load()?;
        engine::EngineCommandDispatcher::with_standalone_engine_control(|mut dispatcher| {
            dispatcher.attach_project(document)?;
            ProjectSettingsApi::new(dispatcher)?.execute(request)
        })
    }

    /// Returns editor replacement state and render settings from the same loaded project revision.
    pub fn inspect_render(
        project_path: impl AsRef<Path>,
        request: GetEditorState,
    ) -> Result<LocalRenderInspection> {
        let database = engine::ProjectDatabase::open_read_only(project_path)?;
        let document = database.load()?;
        engine::EngineCommandDispatcher::with_standalone_engine_control(|mut dispatcher| {
            dispatcher.attach_project(document.clone())?;
            let editor = ProjectEditorApi::new(dispatcher)?.execute(request)?;
            let mut settings_dispatcher = engine::EngineCommandDispatcher::new()?;
            settings_dispatcher.attach_project(document)?;
            let settings =
                ProjectSettingsApi::new(settings_dispatcher)?.execute(GetProjectSettings::new())?;
            Ok(LocalRenderInspection { editor, settings })
        })
    }

    /// Executes one project settings transaction and persists it before return.
    pub fn execute_settings(
        project_path: impl AsRef<Path>,
        request: ExecuteProjectSettingsTransaction,
        permissions: Arc<ApiPermissionContext>,
    ) -> Result<
        LocalProjectExecution<ExecuteProjectSettingsTransactionResult, ProjectSettingsChanged>,
    > {
        execute_settings_command(project_path.as_ref(), request, permissions)
    }

    /// Runs one bounded local project script and persists every committed prefix before return.
    pub fn execute_script(
        project_path: impl AsRef<Path>,
        request: RunProjectScript,
        permissions: Arc<ApiPermissionContext>,
    ) -> Result<LocalProjectExecution<RunProjectScriptResult, ProjectStateChanged>> {
        execute_script_command(project_path.as_ref(), request, permissions)
    }

    /// Executes a settings transaction only when every mutation belongs to render configuration.
    pub fn configure_render(
        project_path: impl AsRef<Path>,
        request: ExecuteProjectSettingsTransaction,
    ) -> Result<
        LocalProjectExecution<ExecuteProjectSettingsTransactionResult, ProjectSettingsChanged>,
    > {
        if request.mutations().is_empty()
            || request.mutations().iter().any(|mutation| {
                let key = match mutation {
                    ProjectSettingMutation::Set { key, .. }
                    | ProjectSettingMutation::Remove { key } => key,
                };
                !key.starts_with(RENDER_SETTING_PREFIX)
            })
        {
            return Err(invalid(
                "configure_render",
                "render configure accepts only nonempty superi.project.render setting batches",
            ));
        }
        execute_settings_command(
            project_path.as_ref(),
            request,
            Arc::new(ApiPermissionContext::default()),
        )
    }

    /// Publishes a copy while preserving active project identity.
    pub fn save(project_path: impl AsRef<Path>) -> Result<LocalProjectSaveResult> {
        let mut database = engine::ProjectDatabase::open(project_path)?;
        let snapshot = database.load()?.snapshot();
        let outcome = database.execute_save_command(engine::ProjectSaveCommand::Save, &snapshot)?;
        Ok(LocalProjectSaveResult::from_engine(&outcome))
    }

    /// Publishes a complete project and rebinds active identity only after the commit point.
    pub fn save_as(
        project_path: impl AsRef<Path>,
        destination: impl AsRef<Path>,
        collision: LocalProjectCollision,
    ) -> Result<LocalProjectSaveResult> {
        let mut database = engine::ProjectDatabase::open(project_path)?;
        let snapshot = database.load()?.snapshot();
        let outcome = database.execute_save_command(
            engine::ProjectSaveCommand::SaveAs {
                destination: destination.as_ref().to_path_buf(),
                collision: collision.into_engine(),
            },
            &snapshot,
        )?;
        Ok(LocalProjectSaveResult::from_engine(&outcome))
    }

    /// Publishes a copy while preserving active project identity.
    pub fn save_copy(
        project_path: impl AsRef<Path>,
        destination: impl AsRef<Path>,
        collision: LocalProjectCollision,
    ) -> Result<LocalProjectSaveResult> {
        let mut database = engine::ProjectDatabase::open(project_path)?;
        let snapshot = database.load()?.snapshot();
        let outcome = database.execute_save_command(
            engine::ProjectSaveCommand::SaveCopy {
                destination: destination.as_ref().to_path_buf(),
                collision: collision.into_engine(),
            },
            &snapshot,
        )?;
        Ok(LocalProjectSaveResult::from_engine(&outcome))
    }

    /// Publishes a no-clobber backup while preserving active project identity.
    pub fn backup(
        project_path: impl AsRef<Path>,
        destination: impl AsRef<Path>,
    ) -> Result<LocalProjectSaveResult> {
        let mut database = engine::ProjectDatabase::open(project_path)?;
        let snapshot = database.load()?.snapshot();
        let outcome = database.execute_save_command(
            engine::ProjectSaveCommand::Backup {
                destination: destination.as_ref().to_path_buf(),
            },
            &snapshot,
        )?;
        Ok(LocalProjectSaveResult::from_engine(&outcome))
    }

    /// Reconstructs one current-schema project through the read-only database owner.
    pub fn validate(project_path: impl AsRef<Path>) -> Result<LocalProjectValidation> {
        let database = engine::ProjectDatabase::open_read_only(project_path)?;
        let schema_revision = database.source_schema_revision();
        let snapshot = database.load()?.snapshot();
        Ok(LocalProjectValidation {
            valid: true,
            schema_revision,
            project: LocalProjectSummary::from_snapshot(&snapshot),
        })
    }

    pub fn recovery_get(
        project_path: impl AsRef<Path>,
        recovery_root: impl AsRef<Path>,
        request: GetProjectRecovery,
        permissions: Arc<ApiPermissionContext>,
    ) -> Result<LocalProjectExecution<GetProjectRecoveryResult, ProjectRecoveryChanged>> {
        execute_recovery(
            project_path.as_ref(),
            recovery_root.as_ref(),
            permissions,
            false,
            |api| api.execute(request),
        )
    }

    pub fn recovery_compare(
        project_path: impl AsRef<Path>,
        recovery_root: impl AsRef<Path>,
        request: CompareProjectRecovery,
        permissions: Arc<ApiPermissionContext>,
    ) -> Result<LocalProjectExecution<CompareProjectRecoveryResult, ProjectRecoveryChanged>> {
        execute_recovery(
            project_path.as_ref(),
            recovery_root.as_ref(),
            permissions,
            true,
            |api| api.compare(request),
        )
    }

    pub fn recovery_restore(
        project_path: impl AsRef<Path>,
        recovery_root: impl AsRef<Path>,
        request: RestoreProjectRecovery,
        permissions: Arc<ApiPermissionContext>,
    ) -> Result<LocalProjectExecution<RestoreProjectRecoveryResult, ProjectRecoveryChanged>> {
        execute_recovery(
            project_path.as_ref(),
            recovery_root.as_ref(),
            permissions,
            true,
            |api| api.restore(request),
        )
    }

    pub fn recovery_dismiss(
        project_path: impl AsRef<Path>,
        recovery_root: impl AsRef<Path>,
        request: DismissProjectRecovery,
        permissions: Arc<ApiPermissionContext>,
    ) -> Result<LocalProjectExecution<DismissProjectRecoveryResult, ProjectRecoveryChanged>> {
        execute_recovery(
            project_path.as_ref(),
            recovery_root.as_ref(),
            permissions,
            true,
            |api| api.dismiss(request),
        )
    }

    /// Executes one typed automation request through its existing permanent API method.
    pub fn execute_automation(
        project_path: impl AsRef<Path>,
        request: LocalAutomationRequest,
        permissions: Arc<ApiPermissionContext>,
    ) -> Result<LocalAutomationResponse> {
        let (id, call) = request.into_call()?;
        let result = match call {
            LocalAutomationCall::ProjectCommand(request) => LocalAutomationResult::ProjectCommand(
                Self::execute_project(project_path, request, permissions)?,
            ),
            LocalAutomationCall::ProjectSettingsTransaction(request) => {
                LocalAutomationResult::ProjectSettingsTransaction(Self::execute_settings(
                    project_path,
                    request,
                    permissions,
                )?)
            }
            LocalAutomationCall::EditorState(request) => LocalAutomationResult::EditorState(
                Box::new(Self::inspect_editor(project_path, request)?),
            ),
            LocalAutomationCall::ProjectSettings(request) => {
                LocalAutomationResult::ProjectSettings(Self::inspect_settings(
                    project_path,
                    request,
                )?)
            }
            LocalAutomationCall::ProjectScript(request) => LocalAutomationResult::ProjectScript(
                Box::new(Self::execute_script(project_path, request, permissions)?),
            ),
            LocalAutomationCall::ProjectCommandLog(request) => {
                LocalAutomationResult::ProjectCommandLog(Self::inspect_command_log(
                    project_path,
                    request,
                    permissions,
                )?)
            }
        };
        Ok(LocalAutomationResponse {
            jsonrpc: "2.0".to_owned(),
            id,
            result,
        })
    }
}

fn execute_project_command(
    project_path: &Path,
    request: ExecuteProjectCommand,
    permissions: Arc<ApiPermissionContext>,
) -> Result<LocalProjectExecution<ExecuteProjectCommandResult, ProjectStateChanged>> {
    let mut database = engine::ProjectDatabase::open(project_path)?;
    let document = database.load()?;
    let (result, events, snapshot) =
        engine::EngineCommandDispatcher::with_standalone_engine_control(|mut dispatcher| {
            dispatcher.attach_project(document)?;
            let mut api = ProjectEditorApi::new_with_permissions(dispatcher, permissions)?;
            let result = api.execute(request)?;
            let events = api.drain_events()?;
            let snapshot = api.project_snapshot()?;
            Ok((result, events, snapshot))
        })?;
    database.replace(&snapshot)?;
    Ok(LocalProjectExecution::new(result, events))
}

fn execute_settings_command(
    project_path: &Path,
    request: ExecuteProjectSettingsTransaction,
    permissions: Arc<ApiPermissionContext>,
) -> Result<LocalProjectExecution<ExecuteProjectSettingsTransactionResult, ProjectSettingsChanged>>
{
    let mut database = engine::ProjectDatabase::open(project_path)?;
    let document = database.load()?;
    let initial_revision = document.revision();
    let (result, events, snapshot) =
        engine::EngineCommandDispatcher::with_standalone_engine_control(|mut dispatcher| {
            dispatcher.attach_project(document)?;
            let mut api = ProjectSettingsApi::new_with_permissions(dispatcher, permissions)?;
            let result = api.execute_transaction(request)?;
            let events = api.drain_events()?;
            let snapshot = api.project_snapshot()?;
            Ok((result, events, snapshot))
        })?;
    if snapshot.revision() != initial_revision {
        database.replace(&snapshot)?;
    }
    Ok(LocalProjectExecution::new(result, events))
}

fn execute_script_command(
    project_path: &Path,
    request: RunProjectScript,
    permissions: Arc<ApiPermissionContext>,
) -> Result<LocalProjectExecution<RunProjectScriptResult, ProjectStateChanged>> {
    let mut database = engine::ProjectDatabase::open(project_path)?;
    let document = database.load()?;
    let initial_revision = document.revision();
    let initial_log_sequence = document.command_log().latest_sequence();
    let (result, events, snapshot) =
        engine::EngineCommandDispatcher::with_standalone_engine_control(|mut dispatcher| {
            dispatcher.attach_project(document)?;
            let mut api = ProjectEditorApi::new_with_permissions(dispatcher, permissions)?;
            let result = api.execute(request)?;
            let events = api.drain_events()?;
            let snapshot = api.project_snapshot()?;
            Ok((result, events, snapshot))
        })?;
    if snapshot.revision() != initial_revision
        || snapshot.command_log().latest_sequence() != initial_log_sequence
    {
        database.replace(&snapshot)?;
    }
    Ok(LocalProjectExecution::new(result, events))
}

fn execute_recovery<R>(
    project_path: &Path,
    recovery_root: &Path,
    permissions: Arc<ApiPermissionContext>,
    discover_first: bool,
    operation: impl FnOnce(&mut ProjectRecoveryApi) -> Result<R>,
) -> Result<LocalProjectExecution<R, ProjectRecoveryChanged>> {
    let database = engine::ProjectDatabase::open(project_path)?;
    let document = database.load()?;
    engine::EngineCommandDispatcher::with_standalone_engine_control(|mut dispatcher| {
        dispatcher.attach_project(document)?;
        dispatcher.attach_project_recovery(database, recovery_root)?;
        let mut api = ProjectRecoveryApi::new_with_permissions(dispatcher, permissions)?;
        if discover_first {
            api.execute(GetProjectRecovery::new(
                "superi-local-recovery-prerequisite",
            ))?;
            let _ = api.drain_events()?;
        }
        let result = operation(&mut api)?;
        let events = api.drain_events()?;
        Ok(LocalProjectExecution::new(result, events))
    })
}

fn require_media_command(request: &ExecuteProjectCommand) -> Result<()> {
    match request.command() {
        ProjectCommand::Apply { actions }
            if !actions.is_empty()
                && actions.iter().all(|action| {
                    matches!(
                        action,
                        ProjectAction::MutateMedia { .. } | ProjectAction::ImportMedia { .. }
                    )
                }) =>
        {
            Ok(())
        }
        _ => Err(invalid(
            "execute_media",
            "media execute accepts only a nonempty apply command of media actions",
        )),
    }
}

fn require_timeline_command(request: &ExecuteProjectCommand) -> Result<()> {
    match request.command() {
        ProjectCommand::Apply { actions }
            if !actions.is_empty()
                && actions.iter().all(|action| {
                    matches!(
                        action,
                        ProjectAction::SelectRootTimeline { .. }
                            | ProjectAction::EditTimeline { .. }
                            | ProjectAction::MutateTracks { .. }
                            | ProjectAction::MutateCaptions { .. }
                            | ProjectAction::MutateMarkers { .. }
                            | ProjectAction::MutateGraph { .. }
                            | ProjectAction::MutateClipMix { .. }
                    )
                }) =>
        {
            Ok(())
        }
        _ => Err(invalid(
            "execute_timeline",
            "timeline execute accepts only root, timeline, track, caption, marker, graph, or clip-mix apply actions",
        )),
    }
}

fn parse_id<T>(value: &str, field: &'static str) -> Result<T>
where
    T: FromStr,
{
    value.parse().map_err(|_| {
        invalid("create_project", "project creation identifier is invalid")
            .with_context(ErrorContext::new(COMPONENT, "create_project").with_field("field", field))
    })
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
