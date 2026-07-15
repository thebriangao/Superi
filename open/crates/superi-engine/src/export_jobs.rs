//! Bounded logical export jobs over the shared background worker substrate.
//!
//! The generic worker pool owns one execution attempt. This module owns the longer engine-level
//! lifetime that can wait for dependencies, pause, resume, retry, retain a result, and expose a
//! classified failure without reusing logical identity accidentally.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use superi_concurrency::jobs::{
    BoundedWorkerPool, JobCancellationToken, JobControl, JobDependencies, JobDependencyFailure,
    JobDependencyStatus, JobDependencyTracker, JobExecution, JobExecutionContext, JobKind,
    JobOutcome, JobPriority, JobProgress, JobProgressSnapshot, JobTaskHandle, JobTerminalState,
    ScheduledJob, WorkerPoolConfig, WorkerPoolShutdownReport, WorkerPoolSnapshot,
};
use superi_concurrency::threads::{current_execution_domain, ExecutionDomain};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::JobId;
use superi_media_io::operation::{
    CancellationToken as MediaCancellationToken, MediaPriority, OperationContext,
};

const COMPONENT: &str = "superi-engine.export_jobs";

/// Explicit bounds for one engine-owned export queue.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ExportJobQueueConfig {
    worker_count: usize,
    pending_capacity: usize,
    retained_capacity: usize,
}

impl ExportJobQueueConfig {
    /// Validates worker, pending, and retained logical-job bounds.
    pub fn new(
        worker_count: usize,
        pending_capacity: usize,
        retained_capacity: usize,
    ) -> Result<Self> {
        WorkerPoolConfig::new(worker_count, pending_capacity)?;
        let minimum_retained = worker_count.checked_add(pending_capacity).ok_or_else(|| {
            queue_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "export queue capacity exceeds the supported range",
                "configure",
            )
        })?;
        if retained_capacity < minimum_retained {
            return Err(queue_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "retained export capacity must cover active and pending worker capacity",
                "configure",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "retained_capacity")
                    .with_field("worker_count", worker_count.to_string())
                    .with_field("pending_capacity", pending_capacity.to_string())
                    .with_field("retained_capacity", retained_capacity.to_string()),
            ));
        }
        Ok(Self {
            worker_count,
            pending_capacity,
            retained_capacity,
        })
    }

    /// Returns the fixed number of background workers.
    #[must_use]
    pub const fn worker_count(self) -> usize {
        self.worker_count
    }

    /// Returns the hard pending capacity of the shared worker pool.
    #[must_use]
    pub const fn pending_capacity(self) -> usize {
        self.pending_capacity
    }

    /// Returns the maximum number of logical jobs retained by this queue.
    #[must_use]
    pub const fn retained_capacity(self) -> usize {
        self.retained_capacity
    }

    fn worker_pool(self) -> Result<WorkerPoolConfig> {
        WorkerPoolConfig::new(self.worker_count, self.pending_capacity)
    }
}

/// Observable state of one logical export job.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ExportJobStatus {
    /// At least one prerequisite has not finalized successfully.
    Waiting,
    /// Prerequisites are ready but the bounded worker queue has not admitted an attempt.
    Pending,
    /// The attempt is admitted but has not started executing user work.
    Queued,
    /// The current attempt is executing in the background-job domain.
    Running,
    /// Pause was requested and the current attempt is acknowledging cancellation.
    Pausing,
    /// No attempt is active and an explicit resume is required.
    Paused,
    /// The most recent attempt failed with retained actionable context.
    Failed,
    /// Cancellation was requested and the current attempt is acknowledging it.
    Cancelling,
    /// The logical job was cancelled and will not run again.
    Cancelled,
    /// A finalized prerequisite did not succeed.
    DependencyFailed,
    /// One complete attempt succeeded and published a typed result.
    Completed,
}

impl ExportJobStatus {
    /// Returns the stable diagnostic code for this state.
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
}

/// Immutable engine-control observation of one logical export job.
#[derive(Clone, Debug)]
pub struct ExportJobSnapshot {
    job_id: JobId,
    status: ExportJobStatus,
    attempt: u32,
    progress: JobProgressSnapshot,
    elapsed: Option<Duration>,
    dependencies: Vec<JobId>,
    failure: Option<Arc<Error>>,
    dependency_failures: Vec<JobDependencyFailure>,
    has_result: bool,
    retry_allowed: bool,
}

impl ExportJobSnapshot {
    /// Returns the stable logical job identity.
    #[must_use]
    pub const fn job_id(&self) -> JobId {
        self.job_id
    }

    /// Returns the current logical state.
    #[must_use]
    pub const fn status(&self) -> ExportJobStatus {
        self.status
    }

    /// Returns the number of attempts admitted to the worker pool.
    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }

    /// Returns a coherent progress observation for the current or most recent attempt.
    #[must_use]
    pub const fn progress(&self) -> JobProgressSnapshot {
        self.progress
    }

    /// Returns worker execution time for the most recent completed attempt.
    #[must_use]
    pub const fn elapsed(&self) -> Option<Duration> {
        self.elapsed
    }

    /// Returns direct prerequisite identities in deterministic order.
    #[must_use]
    pub fn dependencies(&self) -> &[JobId] {
        &self.dependencies
    }

    /// Returns the complete classified error from the most recent failed attempt.
    #[must_use]
    pub fn failure(&self) -> Option<&Error> {
        self.failure.as_deref()
    }

    /// Returns every prerequisite that finalized without success.
    #[must_use]
    pub fn dependency_failures(&self) -> &[JobDependencyFailure] {
        &self.dependency_failures
    }

    /// Returns whether a complete typed result is retained.
    #[must_use]
    pub const fn has_result(&self) -> bool {
        self.has_result
    }

    /// Returns whether an explicit retry is legal for the retained failure.
    #[must_use]
    pub const fn retry_allowed(&self) -> bool {
        self.retry_allowed
    }

    /// Returns whether dependency history may treat this logical job as finalized.
    #[must_use]
    pub const fn is_final(&self) -> bool {
        matches!(
            self.status,
            ExportJobStatus::Completed
                | ExportJobStatus::Cancelled
                | ExportJobStatus::DependencyFailed
        ) || matches!(self.status, ExportJobStatus::Failed) && !self.retry_allowed
    }
}

/// Background context for one fresh export attempt.
#[derive(Clone, Debug)]
pub struct ExportJobExecutionContext {
    attempt: u32,
    job: JobExecutionContext,
    operation: OperationContext,
}

impl ExportJobExecutionContext {
    /// Returns the stable logical job identity.
    #[must_use]
    pub const fn job_id(&self) -> JobId {
        self.job.job_id()
    }

    /// Returns the one-based attempt number.
    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }

    /// Returns the shared monotonic progress owner for this attempt.
    #[must_use]
    pub const fn progress(&self) -> &JobProgress {
        self.job.progress()
    }

    /// Returns the export-priority media operation used by codec and source work.
    #[must_use]
    pub const fn operation(&self) -> &OperationContext {
        &self.operation
    }

    /// Checks both generic job and media-operation cancellation at one bounded work boundary.
    pub fn check(&self, operation: &'static str) -> Result<()> {
        self.job.check(operation)?;
        self.operation.check(operation)
    }
}

/// Reusable work that can create a fresh result for every attempt.
pub trait ExportJobExecutor<R>: Send + Sync + 'static {
    /// Executes one fresh attempt and publishes no result outside the returned value.
    fn execute(&self, context: &ExportJobExecutionContext) -> Result<R>;
}

impl<R, F> ExportJobExecutor<R> for F
where
    F: Fn(&ExportJobExecutionContext) -> Result<R> + Send + Sync + 'static,
{
    fn execute(&self, context: &ExportJobExecutionContext) -> Result<R> {
        self(context)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AttemptIntent {
    None,
    Pause,
    Cancel,
}

struct ActiveAttempt<R> {
    handle: JobTaskHandle<R>,
    job_cancellation: JobCancellationToken,
    media_cancellation: MediaCancellationToken,
    started: Arc<AtomicBool>,
    intent: AttemptIntent,
}

struct ExportJobRecord<R> {
    dependencies: JobDependencyTracker,
    executor: Arc<dyn ExportJobExecutor<R>>,
    status: ExportJobStatus,
    attempt: u32,
    progress: JobProgress,
    elapsed: Option<Duration>,
    active: Option<ActiveAttempt<R>>,
    result: Option<Arc<R>>,
    failure: Option<Arc<Error>>,
    dependency_failures: Vec<JobDependencyFailure>,
}

impl<R> ExportJobRecord<R> {
    fn snapshot(&self, job_id: JobId) -> ExportJobSnapshot {
        let retry_allowed = self.status == ExportJobStatus::Failed
            && self
                .failure
                .as_deref()
                .is_some_and(|error| error.recoverability() != Recoverability::Terminal);
        ExportJobSnapshot {
            job_id,
            status: self.status,
            attempt: self.attempt,
            progress: self.progress.snapshot(),
            elapsed: self.elapsed,
            dependencies: self.dependencies.dependencies().iter().copied().collect(),
            failure: self.failure.clone(),
            dependency_failures: self.dependency_failures.clone(),
            has_result: self.result.is_some(),
            retry_allowed,
        }
    }
}

enum AttemptObservation<R> {
    Pending,
    Completed(JobExecution<R>),
    Disconnected(Error),
}

/// Engine-owned bounded logical queue for typed export results.
pub struct ExportJobQueue<R>
where
    R: Send + Sync + 'static,
{
    config: ExportJobQueueConfig,
    pool: Option<BoundedWorkerPool>,
    jobs: BTreeMap<JobId, ExportJobRecord<R>>,
}

impl<R> ExportJobQueue<R>
where
    R: Send + Sync + 'static,
{
    /// Creates the worker pool on the serialized engine-control owner.
    pub fn new(config: ExportJobQueueConfig) -> Result<Self> {
        ExecutionDomain::EngineControl.require_current()?;
        let pool = BoundedWorkerPool::new(config.worker_pool()?)?;
        Ok(Self {
            config,
            pool: Some(pool),
            jobs: BTreeMap::new(),
        })
    }

    /// Admits one unique logical job and dispatches it when prerequisites and capacity allow.
    pub fn submit<I, E>(
        &mut self,
        job_id: JobId,
        dependencies: I,
        executor: E,
    ) -> Result<ExportJobSnapshot>
    where
        I: IntoIterator<Item = JobId>,
        E: Fn(&ExportJobExecutionContext) -> Result<R> + Send + Sync + 'static,
    {
        self.submit_shared(job_id, dependencies, Arc::new(executor))
    }

    /// Admits a named reusable executor implementation.
    pub fn submit_executor<I, E>(
        &mut self,
        job_id: JobId,
        dependencies: I,
        executor: E,
    ) -> Result<ExportJobSnapshot>
    where
        I: IntoIterator<Item = JobId>,
        E: ExportJobExecutor<R>,
    {
        self.submit_shared(job_id, dependencies, Arc::new(executor))
    }

    fn submit_shared<I>(
        &mut self,
        job_id: JobId,
        dependencies: I,
        executor: Arc<dyn ExportJobExecutor<R>>,
    ) -> Result<ExportJobSnapshot>
    where
        I: IntoIterator<Item = JobId>,
    {
        self.require_control("submit")?;
        if self.jobs.len() >= self.config.retained_capacity {
            return Err(job_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::Retryable,
                "export queue retained-job capacity is full",
                "submit",
                job_id,
            ));
        }
        if self.jobs.contains_key(&job_id) {
            return Err(job_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "export job identity is already retained",
                "submit",
                job_id,
            ));
        }

        let dependencies = JobDependencies::new(job_id, dependencies)?;
        for dependency in dependencies.iter() {
            if !self.jobs.contains_key(dependency) {
                return Err(job_error(
                    ErrorCategory::NotFound,
                    Recoverability::UserCorrectable,
                    "export dependency must be admitted before its dependent",
                    "submit_dependency",
                    job_id,
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "missing_dependency")
                        .with_field("dependency_id", dependency.to_string()),
                ));
            }
        }

        let status = if dependencies.is_empty() {
            ExportJobStatus::Pending
        } else {
            ExportJobStatus::Waiting
        };
        self.jobs.insert(
            job_id,
            ExportJobRecord {
                dependencies: JobDependencyTracker::new(dependencies),
                executor,
                status,
                attempt: 0,
                progress: JobProgress::indeterminate(),
                elapsed: None,
                active: None,
                result: None,
                failure: None,
                dependency_failures: Vec::new(),
            },
        );
        self.refresh_until_stable()?;
        self.snapshot(job_id)
    }

    /// Polls active attempts and dispatches newly ready jobs without blocking engine control.
    pub fn poll(&mut self) -> Result<usize> {
        self.require_control("poll")?;
        let mut changes = 0_usize;
        let job_ids = self.jobs.keys().copied().collect::<Vec<_>>();
        for job_id in job_ids {
            let observation = {
                let record = self.jobs.get(&job_id).expect("job identity came from map");
                let Some(active) = record.active.as_ref() else {
                    continue;
                };
                match active.handle.try_completion() {
                    Ok(Some(execution)) => AttemptObservation::Completed(execution),
                    Ok(None) => AttemptObservation::Pending,
                    Err(error) => AttemptObservation::Disconnected(error),
                }
            };

            match observation {
                AttemptObservation::Pending => {
                    let record = self
                        .jobs
                        .get_mut(&job_id)
                        .expect("job identity came from map");
                    let Some(active) = record.active.as_ref() else {
                        continue;
                    };
                    if active.intent == AttemptIntent::None
                        && active.started.load(Ordering::Acquire)
                        && record.status == ExportJobStatus::Queued
                    {
                        record.status = ExportJobStatus::Running;
                        changes += 1;
                    }
                }
                AttemptObservation::Completed(execution) => {
                    let active = self
                        .jobs
                        .get_mut(&job_id)
                        .expect("job identity came from map")
                        .active
                        .take()
                        .expect("observed attempt is active");
                    self.finish_attempt(job_id, active.intent, execution);
                    changes += 1;
                }
                AttemptObservation::Disconnected(mut error) => {
                    let active = self
                        .jobs
                        .get_mut(&job_id)
                        .expect("job identity came from map")
                        .active
                        .take()
                        .expect("observed attempt is active");
                    error.push_context(attempt_context(job_id, self.jobs[&job_id].attempt));
                    self.finish_disconnected(job_id, active.intent, error);
                    changes += 1;
                }
            }
        }
        changes += self.refresh_until_stable()?;
        Ok(changes)
    }

    /// Returns one immutable logical-job observation.
    pub fn snapshot(&self, job_id: JobId) -> Result<ExportJobSnapshot> {
        self.require_control("snapshot")?;
        self.jobs
            .get(&job_id)
            .map(|record| record.snapshot(job_id))
            .ok_or_else(|| missing_job("snapshot", job_id))
    }

    /// Returns every retained job in deterministic identity order.
    pub fn snapshots(&self) -> Result<Vec<ExportJobSnapshot>> {
        self.require_control("snapshots")?;
        Ok(self
            .jobs
            .iter()
            .map(|(job_id, record)| record.snapshot(*job_id))
            .collect())
    }

    /// Returns the complete typed result only after logical completion.
    pub fn result(&self, job_id: JobId) -> Result<Option<Arc<R>>> {
        self.require_control("result")?;
        self.jobs
            .get(&job_id)
            .map(|record| record.result.clone())
            .ok_or_else(|| missing_job("result", job_id))
    }

    /// Returns the bounded worker-pool state without waiting.
    pub fn worker_snapshot(&self) -> Result<WorkerPoolSnapshot> {
        self.require_control("worker_snapshot")?;
        self.pool
            .as_ref()
            .map(BoundedWorkerPool::snapshot)
            .ok_or_else(|| {
                queue_error(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "export queue is shut down",
                    "worker_snapshot",
                )
            })
    }

    /// Requests a cooperative pause, or pauses pending work immediately.
    pub fn pause(&mut self, job_id: JobId) -> Result<ExportJobSnapshot> {
        self.poll()?;
        let record = self
            .jobs
            .get_mut(&job_id)
            .ok_or_else(|| missing_job("pause", job_id))?;
        match record.status {
            ExportJobStatus::Waiting | ExportJobStatus::Pending => {
                record.status = ExportJobStatus::Paused;
            }
            ExportJobStatus::Queued | ExportJobStatus::Running => {
                let active = record
                    .active
                    .as_mut()
                    .expect("active status owns an attempt");
                active.intent = AttemptIntent::Pause;
                active.job_cancellation.cancel();
                active.media_cancellation.cancel();
                record.status = ExportJobStatus::Pausing;
            }
            ExportJobStatus::Pausing | ExportJobStatus::Paused => {}
            _ => {
                return Err(invalid_transition(
                    "pause",
                    job_id,
                    record.status,
                    "job is not waiting, pending, queued, running, or paused",
                ));
            }
        }
        Ok(record.snapshot(job_id))
    }

    /// Starts a fresh attempt after a pause acknowledgement.
    pub fn resume(&mut self, job_id: JobId) -> Result<ExportJobSnapshot> {
        self.poll()?;
        let record = self
            .jobs
            .get_mut(&job_id)
            .ok_or_else(|| missing_job("resume", job_id))?;
        if record.status != ExportJobStatus::Paused {
            return Err(invalid_transition(
                "resume",
                job_id,
                record.status,
                "only a fully paused export job can resume",
            ));
        }
        reset_for_attempt(record);
        record.status = ExportJobStatus::Waiting;
        self.refresh_until_stable()?;
        self.snapshot(job_id)
    }

    /// Starts a fresh attempt after a nonterminal classified failure.
    pub fn retry(&mut self, job_id: JobId) -> Result<ExportJobSnapshot> {
        self.poll()?;
        let record = self
            .jobs
            .get_mut(&job_id)
            .ok_or_else(|| missing_job("retry", job_id))?;
        if record.status != ExportJobStatus::Failed {
            return Err(invalid_transition(
                "retry",
                job_id,
                record.status,
                "only a failed export job can retry",
            ));
        }
        if match record.failure.as_deref() {
            Some(error) => error.recoverability() == Recoverability::Terminal,
            None => true,
        } {
            return Err(invalid_transition(
                "retry",
                job_id,
                record.status,
                "terminal export failures cannot retry in the current logical lifetime",
            ));
        }
        reset_for_attempt(record);
        record.status = ExportJobStatus::Waiting;
        self.refresh_until_stable()?;
        self.snapshot(job_id)
    }

    /// Requests cooperative cancellation and gives it precedence over an in-flight pause.
    pub fn cancel(&mut self, job_id: JobId) -> Result<ExportJobSnapshot> {
        self.poll()?;
        let record = self
            .jobs
            .get_mut(&job_id)
            .ok_or_else(|| missing_job("cancel", job_id))?;
        match record.status {
            ExportJobStatus::Waiting | ExportJobStatus::Pending | ExportJobStatus::Paused => {
                record.status = ExportJobStatus::Cancelled;
                record.result = None;
            }
            ExportJobStatus::Failed
                if record
                    .failure
                    .as_deref()
                    .is_some_and(|error| error.recoverability() != Recoverability::Terminal) =>
            {
                record.status = ExportJobStatus::Cancelled;
                record.result = None;
            }
            ExportJobStatus::Queued | ExportJobStatus::Running | ExportJobStatus::Pausing => {
                let active = record
                    .active
                    .as_mut()
                    .expect("active status owns an attempt");
                active.intent = AttemptIntent::Cancel;
                active.job_cancellation.cancel();
                active.media_cancellation.cancel();
                record.status = ExportJobStatus::Cancelling;
            }
            ExportJobStatus::Cancelling | ExportJobStatus::Cancelled => {}
            _ => {
                return Err(invalid_transition(
                    "cancel",
                    job_id,
                    record.status,
                    "final export job state cannot be replaced by cancellation",
                ));
            }
        }
        Ok(record.snapshot(job_id))
    }

    /// Removes a finalized job that no retained job still names as a prerequisite.
    pub fn remove(&mut self, job_id: JobId) -> Result<ExportJobSnapshot> {
        self.poll()?;
        let snapshot = self.snapshot(job_id)?;
        if !snapshot.is_final() {
            return Err(invalid_transition(
                "remove",
                job_id,
                snapshot.status(),
                "only a finalized export job can be removed",
            ));
        }
        if let Some((dependent_id, _)) = self.jobs.iter().find(|(dependent_id, record)| {
            **dependent_id != job_id
                && record
                    .dependencies
                    .dependencies()
                    .iter()
                    .any(|dependency| *dependency == job_id)
        }) {
            return Err(job_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "retained dependent still references export job",
                "remove",
                job_id,
            )
            .with_context(
                ErrorContext::new(COMPONENT, "dependent_reference")
                    .with_field("dependent_id", dependent_id.to_string()),
            ));
        }
        self.jobs.remove(&job_id);
        Ok(snapshot)
    }

    /// Returns the number of retained logical jobs.
    pub fn len(&self) -> Result<usize> {
        self.require_control("len")?;
        Ok(self.jobs.len())
    }

    /// Returns whether no logical jobs are retained.
    pub fn is_empty(&self) -> Result<bool> {
        self.require_control("is_empty")?;
        Ok(self.jobs.is_empty())
    }

    /// Cancels unfinished work, drains workers, and joins them from a blocking-safe caller.
    pub fn shutdown(&mut self) -> Result<WorkerPoolShutdownReport> {
        if current_execution_domain().is_some_and(|domain| !domain.policy().allows_blocking()) {
            return Err(queue_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "current execution domain cannot block while shutting down export workers",
                "shutdown",
            ));
        }
        let pool = self.pool.take().ok_or_else(|| {
            queue_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "export queue is already shut down",
                "shutdown",
            )
        })?;

        for record in self.jobs.values_mut() {
            if let Some(active) = record.active.as_mut() {
                active.intent = AttemptIntent::Cancel;
                active.job_cancellation.cancel();
                active.media_cancellation.cancel();
                record.status = ExportJobStatus::Cancelling;
            } else if !record_is_final(record) {
                record.status = ExportJobStatus::Cancelled;
                record.result = None;
            }
        }

        let report = pool.shutdown()?;
        let job_ids = self.jobs.keys().copied().collect::<Vec<_>>();
        for job_id in job_ids {
            let observation = {
                let record = self.jobs.get(&job_id).expect("job identity came from map");
                record
                    .active
                    .as_ref()
                    .map(|active| active.handle.try_completion())
            };
            match observation {
                Some(Ok(Some(execution))) => {
                    let active = self
                        .jobs
                        .get_mut(&job_id)
                        .expect("job identity came from map")
                        .active
                        .take()
                        .expect("shutdown attempt is active");
                    self.finish_attempt(job_id, active.intent, execution);
                }
                Some(Ok(None)) | Some(Err(_)) => {
                    let record = self
                        .jobs
                        .get_mut(&job_id)
                        .expect("job identity came from map");
                    record.active = None;
                    record.status = ExportJobStatus::Cancelled;
                    record.result = None;
                }
                None => {}
            }
        }
        Ok(report)
    }

    fn require_control(&self, operation: &'static str) -> Result<()> {
        ExecutionDomain::EngineControl
            .require_current()
            .map_err(|mut error| {
                error.push_context(ErrorContext::new(COMPONENT, operation));
                error
            })?;
        if self.pool.is_none() {
            return Err(queue_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "export queue is shut down",
                operation,
            ));
        }
        Ok(())
    }

    fn refresh_until_stable(&mut self) -> Result<usize> {
        let mut changes = 0_usize;
        for _ in 0..=self.jobs.len() {
            let mut pass_changes = 0_usize;
            let job_ids = self.jobs.keys().copied().collect::<Vec<_>>();
            for job_id in job_ids {
                let status = self.jobs[&job_id].status;
                if !matches!(status, ExportJobStatus::Waiting | ExportJobStatus::Pending) {
                    continue;
                }
                let dependency_ids = self.jobs[&job_id]
                    .dependencies
                    .dependencies()
                    .iter()
                    .copied()
                    .collect::<Vec<_>>();
                let finalized = dependency_ids
                    .iter()
                    .filter_map(|dependency| {
                        self.logical_terminal_state(*dependency)
                            .map(|state| (*dependency, state))
                    })
                    .collect::<Vec<_>>();

                let dependency_status = {
                    let record = self
                        .jobs
                        .get_mut(&job_id)
                        .expect("job identity came from map");
                    for (dependency, state) in finalized {
                        record.dependencies.record(dependency, state)?;
                    }
                    record.dependencies.status()
                };
                match dependency_status {
                    JobDependencyStatus::Ready => {
                        if self.jobs[&job_id].status == ExportJobStatus::Waiting {
                            self.jobs
                                .get_mut(&job_id)
                                .expect("job identity came from map")
                                .status = ExportJobStatus::Pending;
                            pass_changes += 1;
                        }
                        if self.jobs[&job_id].status == ExportJobStatus::Pending
                            && self.dispatch(job_id)?
                        {
                            pass_changes += 1;
                        }
                    }
                    JobDependencyStatus::Waiting(_) => {
                        if self.jobs[&job_id].status != ExportJobStatus::Waiting {
                            self.jobs
                                .get_mut(&job_id)
                                .expect("job identity came from map")
                                .status = ExportJobStatus::Waiting;
                            pass_changes += 1;
                        }
                    }
                    JobDependencyStatus::Blocked(failures) => {
                        let record = self
                            .jobs
                            .get_mut(&job_id)
                            .expect("job identity came from map");
                        record.status = ExportJobStatus::DependencyFailed;
                        record.dependency_failures = failures;
                        record.result = None;
                        pass_changes += 1;
                    }
                    _ => {
                        return Err(job_error(
                            ErrorCategory::Internal,
                            Recoverability::Terminal,
                            "export queue does not support this dependency status",
                            "refresh_dependencies",
                            job_id,
                        ));
                    }
                }
            }
            changes += pass_changes;
            if pass_changes == 0 {
                break;
            }
        }
        Ok(changes)
    }

    fn dispatch(&mut self, job_id: JobId) -> Result<bool> {
        let next_attempt = self.jobs[&job_id].attempt.checked_add(1).ok_or_else(|| {
            job_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "export attempt counter is exhausted",
                "dispatch",
                job_id,
            )
        })?;
        let progress = JobProgress::indeterminate();
        let job_cancellation = JobCancellationToken::new();
        let media_cancellation = MediaCancellationToken::new();
        let started = Arc::new(AtomicBool::new(false));
        let executor = Arc::clone(&self.jobs[&job_id].executor);
        let worker_started = Arc::clone(&started);
        let worker_media_cancellation = media_cancellation.clone();
        let scheduled = ScheduledJob::new(
            job_id,
            JobKind::Export,
            JobPriority::Export,
            move |job: JobExecutionContext| {
                worker_started.store(true, Ordering::Release);
                let context = ExportJobExecutionContext {
                    attempt: next_attempt,
                    job,
                    operation: OperationContext::new(MediaPriority::Export)
                        .with_cancellation(worker_media_cancellation),
                };
                context.check("begin_export_attempt")?;
                executor.execute(&context)
            },
        );
        let control = JobControl::new().with_cancellation(job_cancellation.clone());
        let submission = self
            .pool
            .as_ref()
            .expect("active export queue owns a worker pool")
            .submit(scheduled, control, progress.clone());
        let handle = match submission {
            Ok(handle) => handle,
            Err(error) if error.category() == ErrorCategory::ResourceExhausted => return Ok(false),
            Err(mut error) => {
                error.push_context(attempt_context(job_id, next_attempt));
                let record = self
                    .jobs
                    .get_mut(&job_id)
                    .expect("job identity came from map");
                record.status = ExportJobStatus::Failed;
                record.failure = Some(Arc::new(error));
                record.result = None;
                return Ok(true);
            }
        };

        let record = self
            .jobs
            .get_mut(&job_id)
            .expect("job identity came from map");
        record.attempt = next_attempt;
        record.progress = progress;
        record.elapsed = None;
        record.failure = None;
        record.dependency_failures.clear();
        record.result = None;
        record.active = Some(ActiveAttempt {
            handle,
            job_cancellation,
            media_cancellation,
            started,
            intent: AttemptIntent::None,
        });
        record.status = ExportJobStatus::Queued;
        Ok(true)
    }

    fn finish_attempt(&mut self, job_id: JobId, intent: AttemptIntent, execution: JobExecution<R>) {
        let completion = execution.into_completion();
        let elapsed = completion.elapsed();
        let outcome = completion.into_outcome();
        let record = self
            .jobs
            .get_mut(&job_id)
            .expect("job identity came from map");
        record.elapsed = Some(elapsed);
        match intent {
            AttemptIntent::Pause => {
                record.status = ExportJobStatus::Paused;
                record.result = None;
                record.failure = None;
            }
            AttemptIntent::Cancel => {
                record.status = ExportJobStatus::Cancelled;
                record.result = None;
            }
            AttemptIntent::None => match outcome {
                JobOutcome::Succeeded(result) => {
                    record.status = ExportJobStatus::Completed;
                    record.result = Some(Arc::new(result));
                    record.failure = None;
                }
                JobOutcome::Failed(mut error) => {
                    error.push_context(attempt_context(job_id, record.attempt));
                    record.status = ExportJobStatus::Failed;
                    record.failure = Some(Arc::new(error));
                    record.result = None;
                }
                JobOutcome::Cancelled => {
                    record.status = ExportJobStatus::Failed;
                    record.failure = Some(Arc::new(
                        job_error(
                            ErrorCategory::Cancelled,
                            Recoverability::Degraded,
                            "export attempt stopped without a pause or cancel request",
                            "finish_attempt",
                            job_id,
                        )
                        .with_context(attempt_context(job_id, record.attempt)),
                    ));
                    record.result = None;
                }
                JobOutcome::DeadlineExceeded => {
                    record.status = ExportJobStatus::Failed;
                    record.failure = Some(Arc::new(
                        job_error(
                            ErrorCategory::Timeout,
                            Recoverability::Retryable,
                            "export attempt exceeded its deadline",
                            "finish_attempt",
                            job_id,
                        )
                        .with_context(attempt_context(job_id, record.attempt)),
                    ));
                    record.result = None;
                }
                JobOutcome::DependencyFailed(failure) => {
                    record.status = ExportJobStatus::DependencyFailed;
                    record.dependency_failures = vec![failure];
                    record.result = None;
                }
                _ => {
                    record.status = ExportJobStatus::Failed;
                    record.failure = Some(Arc::new(
                        job_error(
                            ErrorCategory::Internal,
                            Recoverability::Terminal,
                            "export queue does not support this worker outcome",
                            "finish_attempt",
                            job_id,
                        )
                        .with_context(attempt_context(job_id, record.attempt)),
                    ));
                    record.result = None;
                }
            },
        }
    }

    fn finish_disconnected(&mut self, job_id: JobId, intent: AttemptIntent, error: Error) {
        let record = self
            .jobs
            .get_mut(&job_id)
            .expect("job identity came from map");
        match intent {
            AttemptIntent::Pause => record.status = ExportJobStatus::Paused,
            AttemptIntent::Cancel => record.status = ExportJobStatus::Cancelled,
            AttemptIntent::None => {
                record.status = ExportJobStatus::Failed;
                record.failure = Some(Arc::new(error));
            }
        }
        record.result = None;
    }

    fn logical_terminal_state(&self, job_id: JobId) -> Option<JobTerminalState> {
        let record = self.jobs.get(&job_id)?;
        match record.status {
            ExportJobStatus::Completed => Some(JobTerminalState::Succeeded),
            ExportJobStatus::Cancelled => Some(JobTerminalState::Cancelled),
            ExportJobStatus::DependencyFailed => Some(JobTerminalState::DependencyFailed),
            ExportJobStatus::Failed
                if record
                    .failure
                    .as_deref()
                    .is_some_and(|error| error.recoverability() == Recoverability::Terminal) =>
            {
                Some(JobTerminalState::Failed)
            }
            _ => None,
        }
    }
}

fn reset_for_attempt<R>(record: &mut ExportJobRecord<R>) {
    record.progress = JobProgress::indeterminate();
    record.elapsed = None;
    record.result = None;
    record.failure = None;
    record.dependency_failures.clear();
}

fn record_is_final<R>(record: &ExportJobRecord<R>) -> bool {
    matches!(
        record.status,
        ExportJobStatus::Completed | ExportJobStatus::Cancelled | ExportJobStatus::DependencyFailed
    ) || record.status == ExportJobStatus::Failed
        && record
            .failure
            .as_deref()
            .is_some_and(|error| error.recoverability() == Recoverability::Terminal)
}

fn queue_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

fn job_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
    job_id: JobId,
) -> Error {
    queue_error(category, recoverability, message, operation)
        .with_context(ErrorContext::new(COMPONENT, "job").with_field("job_id", job_id.to_string()))
}

fn attempt_context(job_id: JobId, attempt: u32) -> ErrorContext {
    ErrorContext::new(COMPONENT, "attempt")
        .with_field("job_id", job_id.to_string())
        .with_field("attempt", attempt.to_string())
}

fn missing_job(operation: &'static str, job_id: JobId) -> Error {
    job_error(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        "export job is not retained",
        operation,
        job_id,
    )
}

fn invalid_transition(
    operation: &'static str,
    job_id: JobId,
    status: ExportJobStatus,
    message: &'static str,
) -> Error {
    job_error(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
        operation,
        job_id,
    )
    .with_context(ErrorContext::new(COMPONENT, "state").with_field("status", status.code()))
}
