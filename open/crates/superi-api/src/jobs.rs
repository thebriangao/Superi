//! Stable public projection of engine-owned asynchronous job state and controls.

use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use superi_core::diagnostics::UserSafeError;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::JobId;
use superi_core::settings::SemanticVersion;
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult, EngineEvent,
    EngineTransactionId,
};
use superi_engine::export_dispatch::{
    EngineExportJobCommand, EngineExportJobSnapshot, EngineExportJobState,
};
use superi_engine::export_jobs::ExportJobStatus;

use crate::commands::{
    CancelAllAsyncJobs, CancelAsyncJob, GetAsyncJobs, PauseAsyncJob, RemoveAsyncJob,
    ResumeAsyncJob, RetryAsyncJob,
};
use crate::events::AsyncJobsChanged;
use crate::permissions::ApiPermissionContext;
use crate::version::ASYNC_JOBS_SCHEMA_VERSION;

const COMPONENT: &str = "superi-api.jobs";

/// Canonical opaque handle for one retained asynchronous job.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct AsyncJobHandle(String);

impl AsyncJobHandle {
    /// Validates one canonical lowercase `job:` identifier.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let job_id = JobId::from_str(&value).map_err(|_| invalid_handle())?;
        if job_id.to_string() != value {
            return Err(invalid_handle());
        }
        Ok(Self(value))
    }

    /// Returns the complete canonical handle.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn job_id(&self) -> JobId {
        JobId::from_str(&self.0).expect("validated asynchronous job handle remains canonical")
    }
}

impl From<JobId> for AsyncJobHandle {
    fn from(job_id: JobId) -> Self {
        Self(job_id.to_string())
    }
}

impl<'de> Deserialize<'de> for AsyncJobHandle {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::new(value).map_err(D::Error::custom)
    }
}

impl fmt::Display for AsyncJobHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Stable kind of schedulable media work.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AsyncJobKind {
    /// Produce one complete video frame.
    Frame,
    /// Produce one independently schedulable image tile.
    Tile,
    /// Produce replaceable optimized or proxy media.
    Proxy,
    /// Analyze media without changing authoritative project state.
    Analysis,
    /// Populate or refresh replaceable cached data.
    Cache,
    /// Produce user-requested delivery output.
    Export,
}

impl AsyncJobKind {
    /// Returns the permanent public code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Frame => "frame",
            Self::Tile => "tile",
            Self::Proxy => "proxy",
            Self::Analysis => "analysis",
            Self::Cache => "cache",
            Self::Export => "export",
        }
    }
}

/// User-visible scheduling priority for asynchronous work.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AsyncJobPriority {
    /// Replaceable previews, analysis, cache, and unattended work.
    Background,
    /// User-requested offline render or export work.
    Export,
    /// Time-sensitive playback and prefetch work.
    Playback,
    /// Direct user interaction such as a seek or requested frame.
    Interactive,
}

impl AsyncJobPriority {
    /// Returns the permanent public code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Export => "export",
            Self::Playback => "playback",
            Self::Interactive => "interactive",
        }
    }

    /// Returns the stable ascending urgency rank.
    #[must_use]
    pub const fn rank(self) -> u8 {
        match self {
            Self::Background => 0,
            Self::Export => 1,
            Self::Playback => 2,
            Self::Interactive => 3,
        }
    }

    /// Returns dispatches granted in one saturated 15-job service cycle.
    #[must_use]
    pub const fn service_weight(self) -> u8 {
        match self {
            Self::Background => 1,
            Self::Export => 2,
            Self::Playback => 4,
            Self::Interactive => 8,
        }
    }
}

/// Complete observable lifecycle state for one asynchronous job.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AsyncJobStatus {
    /// At least one prerequisite has not finalized successfully.
    Waiting,
    /// Dependencies are ready but bounded capacity has not admitted an attempt.
    Pending,
    /// The attempt is admitted and waiting for a worker.
    Queued,
    /// The current attempt is executing.
    Running,
    /// Pause was requested and the current attempt is acknowledging it.
    Pausing,
    /// No attempt is active and explicit resume is required.
    Paused,
    /// The most recent attempt failed.
    Failed,
    /// Cancellation was requested and the current attempt is acknowledging it.
    Cancelling,
    /// The logical job was cancelled.
    Cancelled,
    /// A finalized prerequisite did not succeed.
    DependencyFailed,
    /// One complete attempt succeeded.
    Completed,
}

impl AsyncJobStatus {
    /// Returns the permanent public code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Pending => "pending",
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Pausing => "pausing",
            Self::Paused => "paused",
            Self::Failed => "failed",
            Self::Cancelling => "cancelling",
            Self::Cancelled => "cancelled",
            Self::DependencyFailed => "dependency_failed",
            Self::Completed => "completed",
        }
    }

    fn from_engine(status: ExportJobStatus) -> Result<Self> {
        match status {
            ExportJobStatus::Waiting => Ok(Self::Waiting),
            ExportJobStatus::Pending => Ok(Self::Pending),
            ExportJobStatus::Queued => Ok(Self::Queued),
            ExportJobStatus::Running => Ok(Self::Running),
            ExportJobStatus::Pausing => Ok(Self::Pausing),
            ExportJobStatus::Paused => Ok(Self::Paused),
            ExportJobStatus::Failed => Ok(Self::Failed),
            ExportJobStatus::Cancelling => Ok(Self::Cancelling),
            ExportJobStatus::Cancelled => Ok(Self::Cancelled),
            ExportJobStatus::DependencyFailed => Ok(Self::DependencyFailed),
            ExportJobStatus::Completed => Ok(Self::Completed),
            _ => Err(unexpected_projection("async_job_status")),
        }
    }
}

/// Coherent numeric progress for one current or most recent attempt.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AsyncJobProgressSnapshot {
    completed_units: u64,
    total_units: Option<u64>,
    revision: u64,
}

impl AsyncJobProgressSnapshot {
    /// Returns the monotonically nondecreasing completed-unit count.
    #[must_use]
    pub const fn completed_units(&self) -> u64 {
        self.completed_units
    }

    /// Returns the known total, or `None` for indeterminate work.
    #[must_use]
    pub const fn total_units(&self) -> Option<u64> {
        self.total_units
    }

    /// Returns the progress update revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }
}

/// User-safe classified failure retained for one job attempt.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AsyncJobFailureSnapshot {
    category: String,
    recoverability: String,
    code: String,
    title: String,
    action: String,
}

impl AsyncJobFailureSnapshot {
    /// Returns the stable failure category.
    #[must_use]
    pub fn category(&self) -> &str {
        &self.category
    }

    /// Returns the explicit recovery classification.
    #[must_use]
    pub fn recoverability(&self) -> &str {
        &self.recoverability
    }

    /// Returns the stable localization and automation code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the safe default title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the safe default recovery action.
    #[must_use]
    pub fn action(&self) -> &str {
        &self.action
    }
}

/// Terminal state of one prerequisite that prevented dependent work.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum AsyncJobDependencyStatus {
    /// The prerequisite failed with a classified error.
    Failed,
    /// The prerequisite was cancelled.
    Cancelled,
    /// The prerequisite exceeded its deadline.
    DeadlineExceeded,
    /// The prerequisite itself had an unsuccessful prerequisite.
    DependencyFailed,
}

impl AsyncJobDependencyStatus {
    fn from_code(code: &str) -> Result<Self> {
        match code {
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "deadline_exceeded" => Ok(Self::DeadlineExceeded),
            "dependency_failed" => Ok(Self::DependencyFailed),
            _ => Err(unexpected_projection("async_job_dependency_status")),
        }
    }
}

/// One unsuccessful prerequisite retained on a blocked job.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AsyncJobDependencyFailureSnapshot {
    handle: AsyncJobHandle,
    status: AsyncJobDependencyStatus,
}

impl AsyncJobDependencyFailureSnapshot {
    /// Returns the prerequisite handle.
    #[must_use]
    pub const fn handle(&self) -> &AsyncJobHandle {
        &self.handle
    }

    /// Returns the prerequisite's unsuccessful terminal state.
    #[must_use]
    pub const fn status(&self) -> AsyncJobDependencyStatus {
        self.status
    }
}

/// Stable full-state projection of one retained asynchronous job.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AsyncJobSnapshot {
    handle: AsyncJobHandle,
    kind: AsyncJobKind,
    priority: AsyncJobPriority,
    status: AsyncJobStatus,
    attempt: u32,
    progress: AsyncJobProgressSnapshot,
    dependencies: Vec<AsyncJobHandle>,
    failure: Option<AsyncJobFailureSnapshot>,
    dependency_failures: Vec<AsyncJobDependencyFailureSnapshot>,
    has_result: bool,
    retry_allowed: bool,
    is_final: bool,
}

impl AsyncJobSnapshot {
    /// Returns the stable logical job handle.
    #[must_use]
    pub const fn handle(&self) -> &AsyncJobHandle {
        &self.handle
    }

    /// Returns the schedulable work kind.
    #[must_use]
    pub const fn kind(&self) -> AsyncJobKind {
        self.kind
    }

    /// Returns the user-visible scheduling priority.
    #[must_use]
    pub const fn priority(&self) -> AsyncJobPriority {
        self.priority
    }

    /// Returns the complete observable lifecycle state.
    #[must_use]
    pub const fn status(&self) -> AsyncJobStatus {
        self.status
    }

    /// Returns the number of admitted attempts.
    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }

    /// Returns coherent progress for the current or most recent attempt.
    #[must_use]
    pub const fn progress(&self) -> &AsyncJobProgressSnapshot {
        &self.progress
    }

    /// Returns direct prerequisite handles in deterministic order.
    #[must_use]
    pub fn dependencies(&self) -> &[AsyncJobHandle] {
        &self.dependencies
    }

    /// Returns user-safe failure evidence from the most recent attempt.
    #[must_use]
    pub const fn failure(&self) -> Option<&AsyncJobFailureSnapshot> {
        self.failure.as_ref()
    }

    /// Returns every unsuccessful prerequisite.
    #[must_use]
    pub fn dependency_failures(&self) -> &[AsyncJobDependencyFailureSnapshot] {
        &self.dependency_failures
    }

    /// Returns whether the runtime retains a complete typed result.
    #[must_use]
    pub const fn has_result(&self) -> bool {
        self.has_result
    }

    /// Returns whether the retained failure permits a fresh attempt.
    #[must_use]
    pub const fn retry_allowed(&self) -> bool {
        self.retry_allowed
    }

    /// Returns whether dependency history treats this job as finalized.
    #[must_use]
    pub const fn is_final(&self) -> bool {
        self.is_final
    }
}

/// Complete replacement state for every retained asynchronous job.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AsyncJobsSnapshot {
    #[cfg_attr(feature = "typescript-bindings", specta(type = crate::typescript::SemanticVersionBinding))]
    schema_version: SemanticVersion,
    revision: u64,
    jobs: Vec<AsyncJobSnapshot>,
}

impl AsyncJobsSnapshot {
    /// Returns the public asynchronous job schema version.
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    /// Returns the engine-owned replacement-state revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns every retained job in deterministic handle order.
    #[must_use]
    pub fn jobs(&self) -> &[AsyncJobSnapshot] {
        &self.jobs
    }

    /// Returns one retained job by exact handle.
    #[must_use]
    pub fn job(&self, handle: &AsyncJobHandle) -> Option<&AsyncJobSnapshot> {
        self.jobs.iter().find(|job| job.handle == *handle)
    }
}

/// Successful asynchronous job query or control result.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AsyncJobsResult {
    transaction_id: String,
    command_sequence: u64,
    snapshot: AsyncJobsSnapshot,
}

impl AsyncJobsResult {
    fn new(transaction_id: String, command_sequence: u64, snapshot: AsyncJobsSnapshot) -> Self {
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

    /// Returns the engine's successful command sequence.
    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    /// Returns complete retained replacement state.
    #[must_use]
    pub const fn snapshot(&self) -> &AsyncJobsSnapshot {
        &self.snapshot
    }
}

/// Mutable public facade around one engine dispatcher with an attached export queue.
pub struct AsyncJobsApi {
    dispatcher: EngineCommandDispatcher,
    permissions: Arc<ApiPermissionContext>,
}

impl AsyncJobsApi {
    /// Takes ownership of one dispatcher used for all public job operations.
    #[must_use]
    pub fn new(dispatcher: EngineCommandDispatcher) -> Self {
        Self::new_with_permissions(dispatcher, Arc::new(ApiPermissionContext::default()))
    }

    /// Takes ownership of one dispatcher and binds explicit host permission authority.
    #[must_use]
    pub const fn new_with_permissions(
        dispatcher: EngineCommandDispatcher,
        permissions: Arc<ApiPermissionContext>,
    ) -> Self {
        Self {
            dispatcher,
            permissions,
        }
    }

    /// Queries complete retained job replacement state without changing it.
    pub fn execute(&mut self, command: GetAsyncJobs) -> Result<AsyncJobsResult> {
        self.permissions.authorize_command(&command)?;
        self.dispatch_export(
            command.into_transaction_id(),
            EngineExportJobCommand::InspectAll,
            "get_async_jobs",
        )
    }

    /// Requests cooperative pause for one job.
    pub fn pause(&mut self, command: PauseAsyncJob) -> Result<AsyncJobsResult> {
        self.permissions.authorize_command(&command)?;
        let (transaction_id, handle) = command.into_parts();
        self.dispatch_export(
            transaction_id,
            EngineExportJobCommand::Pause(handle.job_id()),
            "pause_async_job",
        )
    }

    /// Resumes one fully paused job with a fresh attempt.
    pub fn resume(&mut self, command: ResumeAsyncJob) -> Result<AsyncJobsResult> {
        self.permissions.authorize_command(&command)?;
        let (transaction_id, handle) = command.into_parts();
        self.dispatch_export(
            transaction_id,
            EngineExportJobCommand::Resume(handle.job_id()),
            "resume_async_job",
        )
    }

    /// Retries one nonterminal failed job with a fresh attempt.
    pub fn retry(&mut self, command: RetryAsyncJob) -> Result<AsyncJobsResult> {
        self.permissions.authorize_command(&command)?;
        let (transaction_id, handle) = command.into_parts();
        self.dispatch_export(
            transaction_id,
            EngineExportJobCommand::Retry(handle.job_id()),
            "retry_async_job",
        )
    }

    /// Requests cooperative cancellation for one job.
    pub fn cancel(&mut self, command: CancelAsyncJob) -> Result<AsyncJobsResult> {
        self.permissions.authorize_command(&command)?;
        let (transaction_id, handle) = command.into_parts();
        self.dispatch_export(
            transaction_id,
            EngineExportJobCommand::Cancel(handle.job_id()),
            "cancel_async_job",
        )
    }

    /// Requests cooperative cancellation for every unfinished job.
    pub fn cancel_all(&mut self, command: CancelAllAsyncJobs) -> Result<AsyncJobsResult> {
        self.permissions.authorize_command(&command)?;
        self.dispatch_export(
            command.into_transaction_id(),
            EngineExportJobCommand::CancelAll,
            "cancel_all_async_jobs",
        )
    }

    /// Removes one finalized job with no retained dependent.
    pub fn remove(&mut self, command: RemoveAsyncJob) -> Result<AsyncJobsResult> {
        self.permissions.authorize_command(&command)?;
        let (transaction_id, handle) = command.into_parts();
        self.dispatch_export(
            transaction_id,
            EngineExportJobCommand::Remove(handle.job_id()),
            "remove_async_job",
        )
    }

    /// Nonblocking host seam that observes worker transitions and returns replacement state.
    ///
    /// This operation is intentionally absent from the transport method catalog. Hosts call it
    /// from their control loop so worker completion, cancellation acknowledgement, and progress
    /// become ordered public events without exposing runtime executor or result ownership.
    pub fn poll_runtime(&mut self, transaction_id: impl Into<String>) -> Result<AsyncJobsResult> {
        self.dispatch_export(
            transaction_id.into(),
            EngineExportJobCommand::Poll,
            "poll_async_jobs",
        )
    }

    /// Drains ordered complete asynchronous job replacement events.
    pub fn drain_events(&mut self) -> Result<Vec<AsyncJobsChanged>> {
        self.dispatcher
            .drain_events()?
            .into_iter()
            .map(|envelope| match envelope.event() {
                EngineEvent::ExportJobsStateChanged(state) => {
                    let revision = envelope
                        .export_state_revision()
                        .ok_or_else(|| unexpected_projection("read_event_revision"))?;
                    if revision != state.revision() {
                        return Err(unexpected_projection("verify_event_revision"));
                    }
                    Ok(AsyncJobsChanged::new(
                        envelope.sequence(),
                        envelope.command_sequence(),
                        envelope.transaction_id().as_str().to_owned(),
                        revision,
                        public_snapshot(state)?,
                    ))
                }
                _ => Err(unexpected_projection("drain_events")),
            })
            .collect()
    }

    fn dispatch_export(
        &mut self,
        transaction_id: String,
        command: EngineExportJobCommand,
        operation: &'static str,
    ) -> Result<AsyncJobsResult> {
        let outcome = self.dispatcher.dispatch(EngineCommandRequest::new(
            EngineTransactionId::new(transaction_id)?,
            EngineCommand::ExecuteExportJob(command),
        ))?;
        let EngineCommandResult::ExportJobs(state) = outcome.result() else {
            return Err(unexpected_projection(operation));
        };
        Ok(AsyncJobsResult::new(
            outcome.transaction_id().as_str().to_owned(),
            outcome.command_sequence(),
            public_snapshot(state)?,
        ))
    }
}

impl fmt::Debug for AsyncJobsApi {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AsyncJobsApi")
            .finish_non_exhaustive()
    }
}

fn public_snapshot(state: &EngineExportJobState) -> Result<AsyncJobsSnapshot> {
    Ok(AsyncJobsSnapshot {
        schema_version: ASYNC_JOBS_SCHEMA_VERSION,
        revision: state.revision(),
        jobs: state
            .jobs()
            .iter()
            .map(public_job_snapshot)
            .collect::<Result<Vec<_>>>()?,
    })
}

fn public_job_snapshot(job: &EngineExportJobSnapshot) -> Result<AsyncJobSnapshot> {
    let progress = job.progress();
    let failure = job.failure().map(|failure| {
        let safe_source = Error::new(
            failure.category(),
            failure.recoverability(),
            "asynchronous job failed",
        );
        let safe = UserSafeError::from_error(&safe_source);
        AsyncJobFailureSnapshot {
            category: safe.category().code().to_owned(),
            recoverability: safe.recoverability().code().to_owned(),
            code: safe.code().to_owned(),
            title: safe.title().to_owned(),
            action: safe.action().to_owned(),
        }
    });
    Ok(AsyncJobSnapshot {
        handle: AsyncJobHandle::from(job.job_id()),
        kind: AsyncJobKind::Export,
        priority: AsyncJobPriority::Export,
        status: AsyncJobStatus::from_engine(job.status())?,
        attempt: job.attempt(),
        progress: AsyncJobProgressSnapshot {
            completed_units: progress.completed_units(),
            total_units: progress.total_units(),
            revision: progress.revision(),
        },
        dependencies: job
            .dependencies()
            .iter()
            .copied()
            .map(AsyncJobHandle::from)
            .collect(),
        failure,
        dependency_failures: job
            .dependency_failures()
            .iter()
            .map(|dependency| {
                Ok(AsyncJobDependencyFailureSnapshot {
                    handle: AsyncJobHandle::from(dependency.job_id()),
                    status: AsyncJobDependencyStatus::from_code(dependency.state().code())?,
                })
            })
            .collect::<Result<Vec<_>>>()?,
        has_result: job.has_result(),
        retry_allowed: job.retry_allowed(),
        is_final: job.is_final(),
    })
}

fn invalid_handle() -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "asynchronous job handle must be one canonical lowercase job identifier",
    )
    .with_context(ErrorContext::new(COMPONENT, "validate_handle"))
}

fn unexpected_projection(operation: &'static str) -> Error {
    Error::new(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "asynchronous job API received an unsupported engine projection",
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

#[cfg(test)]
mod tests {
    use super::{AsyncJobStatus, ExportJobStatus};

    #[test]
    fn every_engine_export_status_has_one_exact_public_value() {
        for (engine, public) in [
            (ExportJobStatus::Waiting, AsyncJobStatus::Waiting),
            (ExportJobStatus::Pending, AsyncJobStatus::Pending),
            (ExportJobStatus::Queued, AsyncJobStatus::Queued),
            (ExportJobStatus::Running, AsyncJobStatus::Running),
            (ExportJobStatus::Pausing, AsyncJobStatus::Pausing),
            (ExportJobStatus::Paused, AsyncJobStatus::Paused),
            (ExportJobStatus::Failed, AsyncJobStatus::Failed),
            (ExportJobStatus::Cancelling, AsyncJobStatus::Cancelling),
            (ExportJobStatus::Cancelled, AsyncJobStatus::Cancelled),
            (
                ExportJobStatus::DependencyFailed,
                AsyncJobStatus::DependencyFailed,
            ),
            (ExportJobStatus::Completed, AsyncJobStatus::Completed),
        ] {
            assert_eq!(AsyncJobStatus::from_engine(engine).unwrap(), public);
            assert_eq!(public.code(), engine.code());
        }
    }
}
