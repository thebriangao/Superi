//! Stable engine command and state projection for bounded logical export jobs.
//!
//! Generic executor closures and typed artifacts remain behind a runtime handle. The serialized
//! dispatcher owns every queue mutation through a type-erased controller, while commands and
//! replacement events carry only stable identities, lifecycle permits, and bounded job state.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, RwLock, TryLockError};
use std::time::Duration;

use superi_concurrency::jobs::{JobDependencies, JobDependencyFailure, JobProgressSnapshot};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::JobId;

use crate::dispatcher::EngineReportedFailure;
use crate::export_jobs::{
    ExportJobExecutionContext, ExportJobExecutor, ExportJobQueue, ExportJobQueueConfig,
    ExportJobSnapshot, ExportJobStatus,
};
use crate::lifecycle::{EngineWorkKind, EngineWorkPermit};

const COMPONENT: &str = "superi-engine.export_dispatch";

/// Maximum direct prerequisites accepted by one stable export submission command.
pub const MAX_ENGINE_EXPORT_DEPENDENCIES: usize = 64;

/// Validated stable submission state for one previously prepared export executor.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineExportJobSubmission {
    job_id: JobId,
    dependencies: Vec<JobId>,
}

impl EngineExportJobSubmission {
    /// Validates one bounded deterministic dependency set.
    pub fn new<I>(job_id: JobId, dependencies: I) -> Result<Self>
    where
        I: IntoIterator<Item = JobId>,
    {
        let dependencies = dependencies.into_iter().collect::<Vec<_>>();
        if dependencies.len() > MAX_ENGINE_EXPORT_DEPENDENCIES {
            return Err(export_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "validate_submission",
                "export submission exceeds the direct dependency bound",
            ));
        }
        let dependencies = JobDependencies::new(job_id, dependencies)?;
        Ok(Self {
            job_id,
            dependencies: dependencies.iter().copied().collect(),
        })
    }

    /// Returns the stable logical job identity.
    #[must_use]
    pub const fn job_id(&self) -> JobId {
        self.job_id
    }

    /// Returns direct prerequisites in deterministic identity order.
    #[must_use]
    pub fn dependencies(&self) -> &[JobId] {
        &self.dependencies
    }
}

/// Stable export queue commands owned by the engine dispatcher.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EngineExportJobCommand {
    /// Submit one prepared executor under a unique logical identity.
    Submit(EngineExportJobSubmission),
    /// Poll worker attempts and publish every newly observed automated transition.
    Poll,
    /// Inspect one retained job and return complete queue state without mutation.
    Inspect(JobId),
    /// Inspect every retained job without mutation.
    InspectAll,
    /// Request or preserve cooperative pause state.
    Pause(JobId),
    /// Start a fresh attempt after pause acknowledgement.
    Resume(JobId),
    /// Start a fresh attempt after a nonterminal failure.
    Retry(JobId),
    /// Request or preserve cooperative cancellation state.
    Cancel(JobId),
    /// Request cancellation for every unfinished retained job.
    CancelAll,
    /// Remove one finalized job with no retained dependent.
    Remove(JobId),
}

impl EngineExportJobCommand {
    /// Creates one validated stable submit command.
    pub fn submit<I>(job_id: JobId, dependencies: I) -> Result<Self>
    where
        I: IntoIterator<Item = JobId>,
    {
        Ok(Self::Submit(EngineExportJobSubmission::new(
            job_id,
            dependencies,
        )?))
    }

    pub(crate) const fn may_change_state(&self) -> bool {
        !matches!(self, Self::Inspect(_) | Self::InspectAll)
    }

    pub(crate) const fn requires_fresh_admission(&self) -> bool {
        matches!(self, Self::Submit(_) | Self::Resume(_) | Self::Retry(_))
    }
}

/// Stable full-state projection of one logical export job.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineExportJobSnapshot {
    job_id: JobId,
    status: ExportJobStatus,
    attempt: u32,
    progress: JobProgressSnapshot,
    elapsed: Option<Duration>,
    dependencies: Vec<JobId>,
    failure: Option<EngineReportedFailure>,
    dependency_failures: Vec<JobDependencyFailure>,
    has_result: bool,
    retry_allowed: bool,
    is_final: bool,
}

impl EngineExportJobSnapshot {
    /// Returns the stable logical job identity.
    #[must_use]
    pub const fn job_id(&self) -> JobId {
        self.job_id
    }

    /// Returns the complete logical state.
    #[must_use]
    pub const fn status(&self) -> ExportJobStatus {
        self.status
    }

    /// Returns the number of worker attempts admitted for this identity.
    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }

    /// Returns coherent progress for the current or most recent attempt.
    #[must_use]
    pub const fn progress(&self) -> JobProgressSnapshot {
        self.progress
    }

    /// Returns execution time for the most recently completed attempt.
    #[must_use]
    pub const fn elapsed(&self) -> Option<Duration> {
        self.elapsed
    }

    /// Returns direct prerequisites in deterministic identity order.
    #[must_use]
    pub fn dependencies(&self) -> &[JobId] {
        &self.dependencies
    }

    /// Returns bounded classified failure evidence from the most recent attempt.
    #[must_use]
    pub const fn failure(&self) -> Option<&EngineReportedFailure> {
        self.failure.as_ref()
    }

    /// Returns every prerequisite that finalized without success.
    #[must_use]
    pub fn dependency_failures(&self) -> &[JobDependencyFailure] {
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

    /// Returns whether dependency history treats this state as finalized.
    #[must_use]
    pub const fn is_final(&self) -> bool {
        self.is_final
    }

    fn from_queue(snapshot: &ExportJobSnapshot) -> Self {
        Self {
            job_id: snapshot.job_id(),
            status: snapshot.status(),
            attempt: snapshot.attempt(),
            progress: snapshot.progress(),
            elapsed: snapshot.elapsed(),
            dependencies: snapshot.dependencies().to_vec(),
            failure: snapshot.failure().map(EngineReportedFailure::from_error),
            dependency_failures: snapshot.dependency_failures().to_vec(),
            has_result: snapshot.has_result(),
            retry_allowed: snapshot.retry_allowed(),
            is_final: snapshot.is_final(),
        }
    }
}

/// Complete replacement state for the engine-owned export queue.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineExportJobState {
    revision: u64,
    jobs: Vec<EngineExportJobSnapshot>,
}

impl EngineExportJobState {
    pub(crate) const fn new(revision: u64, jobs: Vec<EngineExportJobSnapshot>) -> Self {
        Self { revision, jobs }
    }

    /// Returns the dispatcher-owned export state revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns every retained job in deterministic identity order.
    #[must_use]
    pub fn jobs(&self) -> &[EngineExportJobSnapshot] {
        &self.jobs
    }

    /// Returns one retained job by stable identity.
    #[must_use]
    pub fn job(&self, job_id: JobId) -> Option<&EngineExportJobSnapshot> {
        self.jobs.iter().find(|job| job.job_id == job_id)
    }
}

/// Runtime export work that receives the exact lifecycle permit for each fresh attempt.
pub trait EngineExportJobExecutor<R>: Send + Sync + 'static {
    /// Executes one fresh attempt without publishing partial typed output.
    fn execute(&self, context: &ExportJobExecutionContext, permit: EngineWorkPermit) -> Result<R>;
}

impl<R, F> EngineExportJobExecutor<R> for F
where
    F: Fn(&ExportJobExecutionContext, EngineWorkPermit) -> Result<R> + Send + Sync + 'static,
{
    fn execute(&self, context: &ExportJobExecutionContext, permit: EngineWorkPermit) -> Result<R> {
        self(context, permit)
    }
}

struct ExportJobRuntimeState<R>
where
    R: Send + Sync + 'static,
{
    queue: ExportJobQueue<R>,
    prepared: BTreeMap<JobId, Arc<dyn EngineExportJobExecutor<R>>>,
    permits: BTreeMap<JobId, Arc<RwLock<EngineWorkPermit>>>,
    prepared_capacity: usize,
    logical_jobs_final: bool,
}

/// Typed runtime half of the export dispatcher boundary.
///
/// Preparing an executor binds a runtime implementation to a stable identity but does not admit or
/// mutate a logical job. Submit and every later queue action still route through
/// [`crate::dispatcher::EngineCommandDispatcher`]. Typed artifacts remain available here after the
/// dispatcher reports completion. Worker shutdown remains explicit because it may block.
pub struct EngineExportJobRuntime<R>
where
    R: Send + Sync + 'static,
{
    shared: Arc<Mutex<ExportJobRuntimeState<R>>>,
}

impl<R> EngineExportJobRuntime<R>
where
    R: Send + Sync + 'static,
{
    /// Binds one reusable runtime executor to a stable identity before submit.
    pub fn prepare_executor<E>(&self, job_id: JobId, executor: E) -> Result<()>
    where
        E: EngineExportJobExecutor<R>,
    {
        ExecutionDomain::EngineControl.require_current()?;
        let mut runtime = try_runtime(&self.shared, "prepare_executor")?;
        match runtime.queue.snapshot(job_id) {
            Ok(_) => {
                return Err(job_error(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "prepare_executor",
                    "export job identity is already retained",
                    job_id,
                ));
            }
            Err(error) if error.category() == ErrorCategory::NotFound => {}
            Err(error) => return Err(error),
        }
        if runtime.prepared.contains_key(&job_id) {
            return Err(job_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "prepare_executor",
                "export job identity already has a prepared executor",
                job_id,
            ));
        }
        if runtime.prepared.len() >= runtime.prepared_capacity {
            return Err(job_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::Retryable,
                "prepare_executor",
                "prepared export executor capacity is full",
                job_id,
            ));
        }
        runtime.prepared.insert(job_id, Arc::new(executor));
        Ok(())
    }

    /// Returns a retained typed result after the dispatcher reports completion.
    pub fn result(&self, job_id: JobId) -> Result<Option<Arc<R>>> {
        ExecutionDomain::EngineControl.require_current()?;
        try_runtime(&self.shared, "read_result")?
            .queue
            .result(job_id)
    }

    /// Cancels unfinished work, drains workers, and joins them from a blocking-safe caller.
    pub fn shutdown(&self) -> Result<superi_concurrency::jobs::WorkerPoolShutdownReport> {
        let mut runtime = try_runtime(&self.shared, "shutdown")?;
        if !runtime.logical_jobs_final {
            return Err(export_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "shutdown",
                "logical export jobs must be cancelled and polled to final state before worker shutdown",
            ));
        }
        runtime.queue.shutdown()
    }
}

struct AdmittedExportExecutor<R>
where
    R: Send + Sync + 'static,
{
    executor: Arc<dyn EngineExportJobExecutor<R>>,
    permit: Arc<RwLock<EngineWorkPermit>>,
}

impl<R> ExportJobExecutor<R> for AdmittedExportExecutor<R>
where
    R: Send + Sync + 'static,
{
    fn execute(&self, context: &ExportJobExecutionContext) -> Result<R> {
        let permit = *self.permit.read().map_err(|_| {
            export_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "read_attempt_permit",
                "export attempt permit lock is poisoned",
            )
        })?;
        self.executor.execute(context, permit)
    }
}

pub(crate) struct ExportCommandExecution {
    pub(crate) jobs: Vec<EngineExportJobSnapshot>,
    pub(crate) changed: bool,
}

pub(crate) trait ErasedExportJobController: Send {
    fn execute(
        &mut self,
        command: EngineExportJobCommand,
        permit: Option<EngineWorkPermit>,
    ) -> Result<ExportCommandExecution>;

    fn logical_jobs_final(&self) -> Result<bool>;
}

struct SharedExportJobController<R>
where
    R: Send + Sync + 'static,
{
    shared: Arc<Mutex<ExportJobRuntimeState<R>>>,
}

impl<R> ErasedExportJobController for SharedExportJobController<R>
where
    R: Send + Sync + 'static,
{
    fn execute(
        &mut self,
        command: EngineExportJobCommand,
        permit: Option<EngineWorkPermit>,
    ) -> Result<ExportCommandExecution> {
        let mut runtime = try_runtime(&self.shared, "execute_command")?;
        let mut observed_jobs = None;
        let changed = match command {
            EngineExportJobCommand::Submit(submission) => {
                let permit = require_export_permit(permit, "submit")?;
                let job_id = submission.job_id;
                let executor = runtime.prepared.get(&job_id).cloned().ok_or_else(|| {
                    job_error(
                        ErrorCategory::NotFound,
                        Recoverability::UserCorrectable,
                        "submit",
                        "export job has no prepared runtime executor",
                        job_id,
                    )
                })?;
                let permit_slot = Arc::new(RwLock::new(permit));
                runtime.queue.submit_executor(
                    job_id,
                    submission.dependencies,
                    AdmittedExportExecutor {
                        executor,
                        permit: Arc::clone(&permit_slot),
                    },
                )?;
                runtime.prepared.remove(&job_id);
                runtime.permits.insert(job_id, permit_slot);
                true
            }
            EngineExportJobCommand::Poll => {
                let before = project(runtime.queue.snapshots()?);
                runtime.queue.poll()?;
                let after = project(runtime.queue.snapshots()?);
                let changed = before != after;
                observed_jobs = Some(after);
                changed
            }
            EngineExportJobCommand::Inspect(job_id) => {
                runtime.queue.snapshot(job_id)?;
                false
            }
            EngineExportJobCommand::InspectAll => false,
            EngineExportJobCommand::Pause(job_id) => {
                runtime.queue.pause(job_id)?;
                true
            }
            EngineExportJobCommand::Resume(job_id) => {
                let permit = require_export_permit(permit, "resume")?;
                update_attempt_permit(&mut runtime, job_id, permit, |queue| {
                    queue.resume(job_id).map(|_| ())
                })?;
                true
            }
            EngineExportJobCommand::Retry(job_id) => {
                let permit = require_export_permit(permit, "retry")?;
                update_attempt_permit(&mut runtime, job_id, permit, |queue| {
                    queue.retry(job_id).map(|_| ())
                })?;
                true
            }
            EngineExportJobCommand::Cancel(job_id) => {
                runtime.queue.cancel(job_id)?;
                true
            }
            EngineExportJobCommand::CancelAll => {
                let before = project(runtime.queue.snapshots()?);
                for job in before.iter().filter(|job| !job.is_final()) {
                    runtime.queue.cancel(job.job_id())?;
                }
                let after = project(runtime.queue.snapshots()?);
                let changed = before != after;
                observed_jobs = Some(after);
                changed
            }
            EngineExportJobCommand::Remove(job_id) => {
                runtime.queue.remove(job_id)?;
                runtime.permits.remove(&job_id);
                true
            }
        };
        let jobs = match observed_jobs {
            Some(jobs) => jobs,
            None => project(runtime.queue.snapshots()?),
        };
        runtime.logical_jobs_final = jobs.iter().all(EngineExportJobSnapshot::is_final);
        Ok(ExportCommandExecution { jobs, changed })
    }

    fn logical_jobs_final(&self) -> Result<bool> {
        Ok(try_runtime(&self.shared, "inspect_shutdown_readiness")?.logical_jobs_final)
    }
}

pub(crate) fn create_export_runtime<R>(
    config: ExportJobQueueConfig,
) -> Result<(
    Box<dyn ErasedExportJobController>,
    EngineExportJobRuntime<R>,
)>
where
    R: Send + Sync + 'static,
{
    let shared = Arc::new(Mutex::new(ExportJobRuntimeState {
        queue: ExportJobQueue::new(config)?,
        prepared: BTreeMap::new(),
        permits: BTreeMap::new(),
        prepared_capacity: config.retained_capacity(),
        logical_jobs_final: true,
    }));
    Ok((
        Box::new(SharedExportJobController {
            shared: Arc::clone(&shared),
        }),
        EngineExportJobRuntime { shared },
    ))
}

fn project(snapshots: Vec<ExportJobSnapshot>) -> Vec<EngineExportJobSnapshot> {
    snapshots
        .iter()
        .map(EngineExportJobSnapshot::from_queue)
        .collect()
}

fn update_attempt_permit<R, F>(
    runtime: &mut ExportJobRuntimeState<R>,
    job_id: JobId,
    permit: EngineWorkPermit,
    action: F,
) -> Result<()>
where
    R: Send + Sync + 'static,
    F: FnOnce(&mut ExportJobQueue<R>) -> Result<()>,
{
    let permit_slot = runtime.permits.get(&job_id).cloned().ok_or_else(|| {
        job_error(
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            "refresh_attempt_permit",
            "export job has no retained lifecycle permit",
            job_id,
        )
    })?;
    let previous = {
        let mut current = permit_slot.write().map_err(|_| {
            job_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "refresh_attempt_permit",
                "export attempt permit lock is poisoned",
                job_id,
            )
        })?;
        let previous = *current;
        *current = permit;
        previous
    };
    if let Err(error) = action(&mut runtime.queue) {
        *permit_slot.write().map_err(|_| {
            job_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "restore_attempt_permit",
                "export attempt permit lock is poisoned",
                job_id,
            )
        })? = previous;
        return Err(error);
    }
    Ok(())
}

fn require_export_permit(
    permit: Option<EngineWorkPermit>,
    operation: &'static str,
) -> Result<EngineWorkPermit> {
    let permit = permit.ok_or_else(|| {
        export_error(
            ErrorCategory::Conflict,
            Recoverability::Retryable,
            operation,
            "current engine state does not admit fresh export execution",
        )
    })?;
    if permit.work() != EngineWorkKind::Export {
        return Err(export_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            operation,
            "dispatcher supplied a non-export lifecycle permit",
        ));
    }
    Ok(permit)
}

fn try_runtime<'a, R>(
    shared: &'a Arc<Mutex<ExportJobRuntimeState<R>>>,
    operation: &'static str,
) -> Result<MutexGuard<'a, ExportJobRuntimeState<R>>>
where
    R: Send + Sync + 'static,
{
    match shared.try_lock() {
        Ok(runtime) => Ok(runtime),
        Err(TryLockError::WouldBlock) => Err(export_error(
            ErrorCategory::Conflict,
            Recoverability::Retryable,
            operation,
            "export runtime is already borrowed by its serialized owner",
        )),
        Err(TryLockError::Poisoned(_)) => Err(export_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            operation,
            "export runtime ownership lock is poisoned",
        )),
    }
}

fn job_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
    job_id: JobId,
) -> Error {
    export_error(category, recoverability, operation, message)
        .with_context(ErrorContext::new(COMPONENT, "job").with_field("job_id", job_id.to_string()))
}

fn export_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
