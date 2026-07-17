//! Stable public command vocabulary.

use serde::{Deserialize, Serialize};
use superi_core::error::Result;
use superi_core::settings::SemanticVersion;

use crate::api::{EngineIntrospectionSnapshot, MediaCapabilitiesSnapshot};
use crate::audio_automation::{AudioAutomationMutation, AudioAutomationSnapshot};
use crate::jobs::{AsyncJobHandle, AsyncJobsResult};
use crate::permissions::{
    ApiDestructiveOperation, ApiFilesystemAccess, ApiFilesystemPath, ApiPermissionKind,
    ApiPermissionRequirement, ApiPermissionRequirementMode, ApiPermissionRequirements,
};
use crate::project::{ProjectSettingMutation, ProjectSettingsSnapshot};
use crate::recovery::{ProjectRecoveryComparisonSnapshot, ProjectRecoverySnapshot};
use crate::scenario::{ScenarioActionResult, ScenarioTransactionResult, SliceAction};
use crate::schema::PublicMethodKind;
use crate::state::EditorStateSnapshot;
use crate::validation::IntegrationValidationSnapshot;
use crate::version::{
    ASYNC_JOBS_SCHEMA_VERSION, AUDIO_AUTOMATION_SCHEMA_VERSION, CANCEL_ALL_ASYNC_JOBS_METHOD,
    CANCEL_ASYNC_JOB_METHOD, COMPARE_PROJECT_RECOVERY_METHOD, DISMISS_PROJECT_RECOVERY_METHOD,
    EDITOR_STATE_SCHEMA_VERSION, ENGINE_INTEGRATION_VALIDATION_SCHEMA_VERSION,
    ENGINE_INTROSPECTION_SCHEMA_VERSION, EXECUTE_AUDIO_AUTOMATION_TRANSACTION_METHOD,
    EXECUTE_PROJECT_SETTINGS_TRANSACTION_METHOD, EXECUTE_SCENARIO_ACTION_METHOD,
    EXECUTE_SCENARIO_TRANSACTION_METHOD, GET_ASYNC_JOBS_METHOD, GET_AUDIO_AUTOMATION_METHOD,
    GET_EDITOR_STATE_METHOD, GET_ENGINE_INTEGRATION_VALIDATION_METHOD,
    GET_ENGINE_INTROSPECTION_METHOD, GET_MEDIA_CAPABILITIES_METHOD, GET_PROJECT_RECOVERY_METHOD,
    GET_PROJECT_SETTINGS_METHOD, MEDIA_CAPABILITIES_SCHEMA_VERSION, PAUSE_ASYNC_JOB_METHOD,
    PROJECT_RECOVERY_SCHEMA_VERSION, PROJECT_SETTINGS_SCHEMA_VERSION, REMOVE_ASYNC_JOB_METHOD,
    RESTORE_PROJECT_RECOVERY_METHOD, RESUME_ASYNC_JOB_METHOD, RETRY_ASYNC_JOB_METHOD,
    SLICE_SCENARIO_SCHEMA_VERSION,
};

/// One typed public API command and its response contract.
pub trait ApiCommand {
    /// Successful response returned by this command.
    type Response;

    /// Permanent namespaced JSON-RPC method name.
    const METHOD: &'static str;

    /// Whether the method observes state or mutates it.
    const KIND: PublicMethodKind;

    /// Version of the method request and response contract.
    const SCHEMA_VERSION: SemanticVersion;

    /// Whether permission requirements are absent, static, or payload-dependent.
    const PERMISSION_MODE: ApiPermissionRequirementMode;

    /// Every permission kind this method may require in stable order.
    const PERMISSION_KINDS: &'static [ApiPermissionKind];

    /// Derives the exact canonical requirements from this complete typed payload.
    fn permission_requirements(&self) -> Result<ApiPermissionRequirements>;
}

/// Strict query for complete retained asynchronous job state.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetAsyncJobs {
    transaction_id: String,
}

impl GetAsyncJobs {
    /// Creates one asynchronous job state query.
    #[must_use]
    pub fn new(transaction_id: impl Into<String>) -> Self {
        Self {
            transaction_id: transaction_id.into(),
        }
    }

    /// Returns the caller-owned transaction identity.
    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    pub(crate) fn into_transaction_id(self) -> String {
        self.transaction_id
    }
}

impl ApiCommand for GetAsyncJobs {
    type Response = AsyncJobsResult;

    const METHOD: &'static str = GET_ASYNC_JOBS_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Query;
    const SCHEMA_VERSION: SemanticVersion = ASYNC_JOBS_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::None;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        Ok(ApiPermissionRequirements::none())
    }
}

macro_rules! define_async_job_command {
    ($name:ident, $method:ident, $mode:expr, $kinds:expr, $operation:expr) => {
        /// Strict command for controlling one retained asynchronous job.
        #[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
        #[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
        #[serde(deny_unknown_fields)]
        pub struct $name {
            transaction_id: String,
            handle: AsyncJobHandle,
        }

        impl $name {
            /// Creates one strict asynchronous job control command.
            #[must_use]
            pub fn new(transaction_id: impl Into<String>, handle: AsyncJobHandle) -> Self {
                Self {
                    transaction_id: transaction_id.into(),
                    handle,
                }
            }

            /// Returns the caller-owned transaction identity.
            #[must_use]
            pub fn transaction_id(&self) -> &str {
                &self.transaction_id
            }

            /// Returns the exact logical job handle.
            #[must_use]
            pub const fn handle(&self) -> &AsyncJobHandle {
                &self.handle
            }

            pub(crate) fn into_parts(self) -> (String, AsyncJobHandle) {
                (self.transaction_id, self.handle)
            }
        }

        impl ApiCommand for $name {
            type Response = AsyncJobsResult;

            const METHOD: &'static str = $method;
            const KIND: PublicMethodKind = PublicMethodKind::Command;
            const SCHEMA_VERSION: SemanticVersion = ASYNC_JOBS_SCHEMA_VERSION;
            const PERMISSION_MODE: ApiPermissionRequirementMode = $mode;
            const PERMISSION_KINDS: &'static [ApiPermissionKind] = $kinds;

            fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
                let operation: Option<ApiDestructiveOperation> = $operation;
                match operation {
                    Some(operation) => {
                        ApiPermissionRequirements::new([ApiPermissionRequirement::destructive(
                            operation,
                        )])
                    }
                    None => Ok(ApiPermissionRequirements::none()),
                }
            }
        }
    };
}

define_async_job_command!(
    PauseAsyncJob,
    PAUSE_ASYNC_JOB_METHOD,
    ApiPermissionRequirementMode::None,
    &[],
    None
);
define_async_job_command!(
    ResumeAsyncJob,
    RESUME_ASYNC_JOB_METHOD,
    ApiPermissionRequirementMode::None,
    &[],
    None
);
define_async_job_command!(
    RetryAsyncJob,
    RETRY_ASYNC_JOB_METHOD,
    ApiPermissionRequirementMode::None,
    &[],
    None
);
define_async_job_command!(
    CancelAsyncJob,
    CANCEL_ASYNC_JOB_METHOD,
    ApiPermissionRequirementMode::Static,
    &[ApiPermissionKind::Destructive],
    Some(ApiDestructiveOperation::CancelAsyncJob)
);
define_async_job_command!(
    RemoveAsyncJob,
    REMOVE_ASYNC_JOB_METHOD,
    ApiPermissionRequirementMode::Static,
    &[ApiPermissionKind::Destructive],
    Some(ApiDestructiveOperation::RemoveAsyncJob)
);

/// Strict command for cooperatively cancelling every unfinished asynchronous job.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelAllAsyncJobs {
    transaction_id: String,
}

impl CancelAllAsyncJobs {
    /// Creates one cancel-all command.
    #[must_use]
    pub fn new(transaction_id: impl Into<String>) -> Self {
        Self {
            transaction_id: transaction_id.into(),
        }
    }

    /// Returns the caller-owned transaction identity.
    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    pub(crate) fn into_transaction_id(self) -> String {
        self.transaction_id
    }
}

impl ApiCommand for CancelAllAsyncJobs {
    type Response = AsyncJobsResult;

    const METHOD: &'static str = CANCEL_ALL_ASYNC_JOBS_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Command;
    const SCHEMA_VERSION: SemanticVersion = ASYNC_JOBS_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::Static;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[ApiPermissionKind::Destructive];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        ApiPermissionRequirements::new([ApiPermissionRequirement::destructive(
            ApiDestructiveOperation::CancelAsyncJob,
        )])
    }
}

/// Strict query for one complete editor replacement snapshot.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetEditorState {
    transaction_id: String,
}

impl GetEditorState {
    /// Creates one caller-correlated complete state query.
    #[must_use]
    pub fn new(transaction_id: impl Into<String>) -> Self {
        Self {
            transaction_id: transaction_id.into(),
        }
    }

    /// Returns the caller transaction identity.
    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    pub(crate) fn into_transaction_id(self) -> String {
        self.transaction_id
    }
}

impl ApiCommand for GetEditorState {
    type Response = GetEditorStateResult;

    const METHOD: &'static str = GET_EDITOR_STATE_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Query;
    const SCHEMA_VERSION: SemanticVersion = EDITOR_STATE_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::None;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        Ok(ApiPermissionRequirements::none())
    }
}

/// Successful complete editor-state query.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetEditorStateResult {
    transaction_id: String,
    command_sequence: u64,
    snapshot: EditorStateSnapshot,
}

impl GetEditorStateResult {
    pub(crate) const fn new(
        transaction_id: String,
        command_sequence: u64,
        snapshot: EditorStateSnapshot,
    ) -> Self {
        Self {
            transaction_id,
            command_sequence,
            snapshot,
        }
    }

    /// Returns the normalized caller transaction identity.
    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    /// Returns the engine command sequence that captured this snapshot.
    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    /// Returns the complete editor replacement snapshot.
    #[must_use]
    pub const fn snapshot(&self) -> &EditorStateSnapshot {
        &self.snapshot
    }
}

/// Structured command for refreshing project crash recovery state.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetProjectRecovery {
    transaction_id: String,
}

impl GetProjectRecovery {
    #[must_use]
    pub fn new(transaction_id: impl Into<String>) -> Self {
        Self {
            transaction_id: transaction_id.into(),
        }
    }

    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    pub(crate) fn into_transaction_id(self) -> String {
        self.transaction_id
    }
}

impl ApiCommand for GetProjectRecovery {
    type Response = GetProjectRecoveryResult;

    const METHOD: &'static str = GET_PROJECT_RECOVERY_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Query;
    const SCHEMA_VERSION: SemanticVersion = PROJECT_RECOVERY_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::None;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        Ok(ApiPermissionRequirements::none())
    }
}

/// Successful complete recovery-state query.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetProjectRecoveryResult {
    transaction_id: String,
    command_sequence: u64,
    snapshot: ProjectRecoverySnapshot,
}

impl GetProjectRecoveryResult {
    pub(crate) const fn new(
        transaction_id: String,
        command_sequence: u64,
        snapshot: ProjectRecoverySnapshot,
    ) -> Self {
        Self {
            transaction_id,
            command_sequence,
            snapshot,
        }
    }

    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    #[must_use]
    pub const fn snapshot(&self) -> &ProjectRecoverySnapshot {
        &self.snapshot
    }
}

/// Strict command for comparing one recovery candidate.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompareProjectRecovery {
    transaction_id: String,
    expected_catalog_revision: u64,
    candidate_id: String,
}

impl CompareProjectRecovery {
    #[must_use]
    pub fn new(
        transaction_id: impl Into<String>,
        expected_catalog_revision: u64,
        candidate_id: impl Into<String>,
    ) -> Self {
        Self {
            transaction_id: transaction_id.into(),
            expected_catalog_revision,
            candidate_id: candidate_id.into(),
        }
    }

    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    #[must_use]
    pub const fn expected_catalog_revision(&self) -> u64 {
        self.expected_catalog_revision
    }

    #[must_use]
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    pub(crate) fn into_parts(self) -> (String, u64, String) {
        (
            self.transaction_id,
            self.expected_catalog_revision,
            self.candidate_id,
        )
    }
}

impl ApiCommand for CompareProjectRecovery {
    type Response = CompareProjectRecoveryResult;

    const METHOD: &'static str = COMPARE_PROJECT_RECOVERY_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Query;
    const SCHEMA_VERSION: SemanticVersion = PROJECT_RECOVERY_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::None;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        Ok(ApiPermissionRequirements::none())
    }
}

/// Successful semantic recovery comparison.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompareProjectRecoveryResult {
    transaction_id: String,
    command_sequence: u64,
    snapshot: ProjectRecoverySnapshot,
    comparison: ProjectRecoveryComparisonSnapshot,
}

impl CompareProjectRecoveryResult {
    pub(crate) const fn new(
        transaction_id: String,
        command_sequence: u64,
        snapshot: ProjectRecoverySnapshot,
        comparison: ProjectRecoveryComparisonSnapshot,
    ) -> Self {
        Self {
            transaction_id,
            command_sequence,
            snapshot,
            comparison,
        }
    }

    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    #[must_use]
    pub const fn snapshot(&self) -> &ProjectRecoverySnapshot {
        &self.snapshot
    }

    #[must_use]
    pub const fn comparison(&self) -> &ProjectRecoveryComparisonSnapshot {
        &self.comparison
    }
}

/// Strict optimistic command for restoring one recovery candidate.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreProjectRecovery {
    transaction_id: String,
    expected_catalog_revision: u64,
    expected_project_revision: u64,
    candidate_id: String,
}

impl RestoreProjectRecovery {
    #[must_use]
    pub fn new(
        transaction_id: impl Into<String>,
        expected_catalog_revision: u64,
        expected_project_revision: u64,
        candidate_id: impl Into<String>,
    ) -> Self {
        Self {
            transaction_id: transaction_id.into(),
            expected_catalog_revision,
            expected_project_revision,
            candidate_id: candidate_id.into(),
        }
    }

    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    #[must_use]
    pub const fn expected_catalog_revision(&self) -> u64 {
        self.expected_catalog_revision
    }

    #[must_use]
    pub const fn expected_project_revision(&self) -> u64 {
        self.expected_project_revision
    }

    #[must_use]
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    pub(crate) fn into_parts(self) -> (String, u64, u64, String) {
        (
            self.transaction_id,
            self.expected_catalog_revision,
            self.expected_project_revision,
            self.candidate_id,
        )
    }
}

impl ApiCommand for RestoreProjectRecovery {
    type Response = RestoreProjectRecoveryResult;

    const METHOD: &'static str = RESTORE_PROJECT_RECOVERY_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Command;
    const SCHEMA_VERSION: SemanticVersion = PROJECT_RECOVERY_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::Static;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[ApiPermissionKind::Destructive];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        ApiPermissionRequirements::new([ApiPermissionRequirement::destructive(
            ApiDestructiveOperation::RestoreProjectRecovery,
        )])
    }
}

/// Successful durable recovery restore result.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreProjectRecoveryResult {
    transaction_id: String,
    command_sequence: u64,
    restored: bool,
    snapshot: ProjectRecoverySnapshot,
}

impl RestoreProjectRecoveryResult {
    pub(crate) const fn new(
        transaction_id: String,
        command_sequence: u64,
        restored: bool,
        snapshot: ProjectRecoverySnapshot,
    ) -> Self {
        Self {
            transaction_id,
            command_sequence,
            restored,
            snapshot,
        }
    }

    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    #[must_use]
    pub const fn restored(&self) -> bool {
        self.restored
    }

    #[must_use]
    pub const fn snapshot(&self) -> &ProjectRecoverySnapshot {
        &self.snapshot
    }
}

/// Strict optimistic command for dismissing one recovery candidate.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DismissProjectRecovery {
    transaction_id: String,
    expected_catalog_revision: u64,
    candidate_id: String,
}

impl DismissProjectRecovery {
    #[must_use]
    pub fn new(
        transaction_id: impl Into<String>,
        expected_catalog_revision: u64,
        candidate_id: impl Into<String>,
    ) -> Self {
        Self {
            transaction_id: transaction_id.into(),
            expected_catalog_revision,
            candidate_id: candidate_id.into(),
        }
    }

    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    #[must_use]
    pub const fn expected_catalog_revision(&self) -> u64 {
        self.expected_catalog_revision
    }

    #[must_use]
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    pub(crate) fn into_parts(self) -> (String, u64, String) {
        (
            self.transaction_id,
            self.expected_catalog_revision,
            self.candidate_id,
        )
    }
}

impl ApiCommand for DismissProjectRecovery {
    type Response = DismissProjectRecoveryResult;

    const METHOD: &'static str = DISMISS_PROJECT_RECOVERY_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Command;
    const SCHEMA_VERSION: SemanticVersion = PROJECT_RECOVERY_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::Static;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[ApiPermissionKind::Destructive];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        ApiPermissionRequirements::new([ApiPermissionRequirement::destructive(
            ApiDestructiveOperation::DismissProjectRecovery,
        )])
    }
}

/// Successful durable recovery dismissal result.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DismissProjectRecoveryResult {
    transaction_id: String,
    command_sequence: u64,
    dismissed: bool,
    snapshot: ProjectRecoverySnapshot,
}

impl DismissProjectRecoveryResult {
    pub(crate) const fn new(
        transaction_id: String,
        command_sequence: u64,
        dismissed: bool,
        snapshot: ProjectRecoverySnapshot,
    ) -> Self {
        Self {
            transaction_id,
            command_sequence,
            dismissed,
            snapshot,
        }
    }

    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    #[must_use]
    pub const fn dismissed(&self) -> bool {
        self.dismissed
    }

    #[must_use]
    pub const fn snapshot(&self) -> &ProjectRecoverySnapshot {
        &self.snapshot
    }
}

/// Structured parameters for the authored audio automation query.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetAudioAutomation {}

impl GetAudioAutomation {
    /// Creates an unfiltered complete automation query.
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }
}

impl ApiCommand for GetAudioAutomation {
    type Response = GetAudioAutomationResult;

    const METHOD: &'static str = GET_AUDIO_AUTOMATION_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Query;
    const SCHEMA_VERSION: SemanticVersion = AUDIO_AUTOMATION_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::None;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        Ok(ApiPermissionRequirements::none())
    }
}

/// Successful response to [`GetAudioAutomation`].
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetAudioAutomationResult {
    snapshot: AudioAutomationSnapshot,
}

impl GetAudioAutomationResult {
    pub(crate) const fn new(snapshot: AudioAutomationSnapshot) -> Self {
        Self { snapshot }
    }

    /// Returns complete authored automation state.
    #[must_use]
    pub const fn snapshot(&self) -> &AudioAutomationSnapshot {
        &self.snapshot
    }
}

/// One caller-owned optimistic authored audio automation transaction.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteAudioAutomationTransaction {
    transaction_id: String,
    expected_revision: u64,
    mutations: Vec<AudioAutomationMutation>,
}

impl Eq for ExecuteAudioAutomationTransaction {}

impl ExecuteAudioAutomationTransaction {
    /// Creates one ordered public automation transaction.
    #[must_use]
    pub fn new(
        transaction_id: impl Into<String>,
        expected_revision: u64,
        mutations: Vec<AudioAutomationMutation>,
    ) -> Self {
        Self {
            transaction_id: transaction_id.into(),
            expected_revision,
            mutations,
        }
    }

    /// Returns the caller-owned transaction identity.
    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    /// Returns the required authored state revision.
    #[must_use]
    pub const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    /// Returns mutations in exact execution order.
    #[must_use]
    pub fn mutations(&self) -> &[AudioAutomationMutation] {
        &self.mutations
    }

    pub(crate) fn into_parts(self) -> (String, u64, Vec<AudioAutomationMutation>) {
        (self.transaction_id, self.expected_revision, self.mutations)
    }
}

impl ApiCommand for ExecuteAudioAutomationTransaction {
    type Response = ExecuteAudioAutomationTransactionResult;

    const METHOD: &'static str = EXECUTE_AUDIO_AUTOMATION_TRANSACTION_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Command;
    const SCHEMA_VERSION: SemanticVersion = AUDIO_AUTOMATION_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode =
        ApiPermissionRequirementMode::PayloadDependent;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[ApiPermissionKind::Destructive];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        let destructive = self.mutations.iter().any(|mutation| {
            matches!(
                mutation,
                AudioAutomationMutation::RemoveLane { .. }
                    | AudioAutomationMutation::RemoveKeyframe { .. }
            )
        });
        if destructive {
            ApiPermissionRequirements::new([ApiPermissionRequirement::destructive(
                ApiDestructiveOperation::RemoveAudioAutomation,
            )])
        } else {
            Ok(ApiPermissionRequirements::none())
        }
    }
}

/// Successful response to one authored audio automation transaction.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteAudioAutomationTransactionResult {
    transaction_id: String,
    command_sequence: u64,
    snapshot: AudioAutomationSnapshot,
}

impl ExecuteAudioAutomationTransactionResult {
    pub(crate) const fn new(
        transaction_id: String,
        command_sequence: u64,
        snapshot: AudioAutomationSnapshot,
    ) -> Self {
        Self {
            transaction_id,
            command_sequence,
            snapshot,
        }
    }

    /// Returns the caller-owned transaction identity.
    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    /// Returns the successful engine command sequence.
    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    /// Returns complete replacement authored automation state.
    #[must_use]
    pub const fn snapshot(&self) -> &AudioAutomationSnapshot {
        &self.snapshot
    }
}

/// Structured parameters for the authoritative project settings query.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetProjectSettings {}

impl GetProjectSettings {
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }
}

impl ApiCommand for GetProjectSettings {
    type Response = GetProjectSettingsResult;

    const METHOD: &'static str = GET_PROJECT_SETTINGS_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Query;
    const SCHEMA_VERSION: SemanticVersion = PROJECT_SETTINGS_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::None;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        Ok(ApiPermissionRequirements::none())
    }
}

/// Successful response to [`GetProjectSettings`].
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetProjectSettingsResult {
    snapshot: ProjectSettingsSnapshot,
}

impl GetProjectSettingsResult {
    pub(crate) const fn new(snapshot: ProjectSettingsSnapshot) -> Self {
        Self { snapshot }
    }

    #[must_use]
    pub const fn snapshot(&self) -> &ProjectSettingsSnapshot {
        &self.snapshot
    }
}

/// One caller-owned optimistic project settings transaction.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteProjectSettingsTransaction {
    transaction_id: String,
    expected_revision: u64,
    mutations: Vec<ProjectSettingMutation>,
}

impl ExecuteProjectSettingsTransaction {
    #[must_use]
    pub fn new(
        transaction_id: impl Into<String>,
        expected_revision: u64,
        mutations: Vec<ProjectSettingMutation>,
    ) -> Self {
        Self {
            transaction_id: transaction_id.into(),
            expected_revision,
            mutations,
        }
    }

    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    #[must_use]
    pub const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    #[must_use]
    pub fn mutations(&self) -> &[ProjectSettingMutation] {
        &self.mutations
    }

    pub(crate) fn into_parts(self) -> (String, u64, Vec<ProjectSettingMutation>) {
        (self.transaction_id, self.expected_revision, self.mutations)
    }
}

impl ApiCommand for ExecuteProjectSettingsTransaction {
    type Response = ExecuteProjectSettingsTransactionResult;

    const METHOD: &'static str = EXECUTE_PROJECT_SETTINGS_TRANSACTION_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Command;
    const SCHEMA_VERSION: SemanticVersion = PROJECT_SETTINGS_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::None;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        Ok(ApiPermissionRequirements::none())
    }
}

/// Successful response to one project settings transaction.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteProjectSettingsTransactionResult {
    transaction_id: String,
    command_sequence: u64,
    snapshot: ProjectSettingsSnapshot,
}

impl ExecuteProjectSettingsTransactionResult {
    pub(crate) const fn new(
        transaction_id: String,
        command_sequence: u64,
        snapshot: ProjectSettingsSnapshot,
    ) -> Self {
        Self {
            transaction_id,
            command_sequence,
            snapshot,
        }
    }

    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    #[must_use]
    pub const fn snapshot(&self) -> &ProjectSettingsSnapshot {
        &self.snapshot
    }
}

/// Structured parameters for a media capability query.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetMediaCapabilities {}

impl GetMediaCapabilities {
    /// Creates an unfiltered media capability query.
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }
}

impl ApiCommand for GetMediaCapabilities {
    type Response = GetMediaCapabilitiesResult;

    const METHOD: &'static str = GET_MEDIA_CAPABILITIES_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Query;
    const SCHEMA_VERSION: SemanticVersion = MEDIA_CAPABILITIES_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::None;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        Ok(ApiPermissionRequirements::none())
    }
}

/// Typed request for one coherent engine integration validation snapshot.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetEngineIntegrationValidation {}

impl GetEngineIntegrationValidation {
    /// Creates an unfiltered integration validation query.
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }
}

impl ApiCommand for GetEngineIntegrationValidation {
    type Response = GetEngineIntegrationValidationResult;

    const METHOD: &'static str = GET_ENGINE_INTEGRATION_VALIDATION_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Query;
    const SCHEMA_VERSION: SemanticVersion = ENGINE_INTEGRATION_VALIDATION_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::None;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        Ok(ApiPermissionRequirements::none())
    }
}

/// Successful coherent engine integration validation query.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetEngineIntegrationValidationResult {
    snapshot: IntegrationValidationSnapshot,
}

impl GetEngineIntegrationValidationResult {
    pub(crate) const fn new(snapshot: IntegrationValidationSnapshot) -> Self {
        Self { snapshot }
    }

    /// Returns the complete strict integration validation state.
    #[must_use]
    pub const fn snapshot(&self) -> &IntegrationValidationSnapshot {
        &self.snapshot
    }
}

/// Successful response to [`GetMediaCapabilities`].
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetMediaCapabilitiesResult {
    snapshot: MediaCapabilitiesSnapshot,
}

impl GetMediaCapabilitiesResult {
    pub(crate) const fn new(snapshot: MediaCapabilitiesSnapshot) -> Self {
        Self { snapshot }
    }

    /// Returns the complete point-in-time capability state.
    #[must_use]
    pub const fn snapshot(&self) -> &MediaCapabilitiesSnapshot {
        &self.snapshot
    }
}

/// Structured parameters for a complete engine introspection query.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetEngineIntrospection {}

impl GetEngineIntrospection {
    /// Creates an unfiltered engine introspection query.
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }
}

impl ApiCommand for GetEngineIntrospection {
    type Response = GetEngineIntrospectionResult;

    const METHOD: &'static str = GET_ENGINE_INTROSPECTION_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Query;
    const SCHEMA_VERSION: SemanticVersion = ENGINE_INTROSPECTION_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::None;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        Ok(ApiPermissionRequirements::none())
    }
}

/// Successful response to [`GetEngineIntrospection`].
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetEngineIntrospectionResult {
    snapshot: EngineIntrospectionSnapshot,
}

impl GetEngineIntrospectionResult {
    pub(crate) const fn new(snapshot: EngineIntrospectionSnapshot) -> Self {
        Self { snapshot }
    }

    /// Returns the complete point-in-time capability and health state.
    #[must_use]
    pub const fn snapshot(&self) -> &EngineIntrospectionSnapshot {
        &self.snapshot
    }
}

/// Typed request to execute one precise reference slice action.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteScenarioAction {
    action: SliceAction,
}

impl ExecuteScenarioAction {
    /// Creates an action request.
    #[must_use]
    pub const fn new(action: SliceAction) -> Self {
        Self { action }
    }

    /// Returns the requested action.
    #[must_use]
    pub const fn action(&self) -> &SliceAction {
        &self.action
    }

    pub(crate) fn into_action(self) -> SliceAction {
        self.action
    }
}

impl ApiCommand for ExecuteScenarioAction {
    type Response = ScenarioActionResult;

    const METHOD: &'static str = EXECUTE_SCENARIO_ACTION_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Command;
    const SCHEMA_VERSION: SemanticVersion = SLICE_SCENARIO_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode =
        ApiPermissionRequirementMode::PayloadDependent;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[ApiPermissionKind::Filesystem];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        scenario_permission_requirements(std::slice::from_ref(&self.action))
    }
}

/// Typed request to commit one optimistic ordered scenario transaction.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteScenarioTransaction {
    transaction_id: String,
    expected_revision: u64,
    actions: Vec<SliceAction>,
}

impl ExecuteScenarioTransaction {
    /// Creates one public automation transaction.
    #[must_use]
    pub fn new(
        transaction_id: impl Into<String>,
        expected_revision: u64,
        actions: Vec<SliceAction>,
    ) -> Self {
        Self {
            transaction_id: transaction_id.into(),
            expected_revision,
            actions,
        }
    }

    /// Returns the exact caller-owned transaction identity.
    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    /// Returns the exact scenario revision required before execution.
    #[must_use]
    pub const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    /// Returns actions in deterministic execution order.
    #[must_use]
    pub fn actions(&self) -> &[SliceAction] {
        &self.actions
    }

    pub(crate) fn into_parts(self) -> (String, u64, Vec<SliceAction>) {
        (self.transaction_id, self.expected_revision, self.actions)
    }
}

impl ApiCommand for ExecuteScenarioTransaction {
    type Response = ScenarioTransactionResult;

    const METHOD: &'static str = EXECUTE_SCENARIO_TRANSACTION_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Command;
    const SCHEMA_VERSION: SemanticVersion = SLICE_SCENARIO_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode =
        ApiPermissionRequirementMode::PayloadDependent;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[ApiPermissionKind::Filesystem];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        scenario_permission_requirements(&self.actions)
    }
}

fn scenario_permission_requirements(actions: &[SliceAction]) -> Result<ApiPermissionRequirements> {
    let mut requirements = Vec::new();
    for action in actions {
        if let SliceAction::ImportClip { path, .. } = action {
            requirements.push(ApiPermissionRequirement::filesystem(
                ApiFilesystemAccess::Read,
                ApiFilesystemPath::native(path.clone())?,
            ));
        }
    }
    ApiPermissionRequirements::new(requirements)
}
