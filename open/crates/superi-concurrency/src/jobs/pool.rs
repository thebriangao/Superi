//! Bounded parallel execution with explicit local ownership and work stealing.
//!
//! Every submitted job receives a preferred worker in round-robin order and enters that worker's
//! local priority queue. At dispatch, one global weighted service cursor selects the priority
//! class before a worker takes local work or steals from the next eligible victim. This preserves
//! the public priority starvation bounds across every local queue while making ownership and steals
//! observable.
//!
//! Queue state, dispatch accounting, and shutdown use one short mutex and one condition variable.
//! No user code runs while that mutex is held, and workers never acquire multiple queue locks, so
//! the pool has no internal lock-order cycle. Jobs remain cooperative: a running closure must call
//! [`JobExecutionContext::check`] between bounded units because safe preemption is not available.

use std::collections::BTreeSet;
use std::fmt;
use std::panic::{self, AssertUnwindSafe};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};
use std::time::Instant;

use super::priority::SERVICE_PATTERN;
use super::{JobCompletion, JobControl, JobOutcome, JobProgress, PriorityScheduler, ScheduledJob};
use crate::threads::{current_execution_domain, ExecutionDomain};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::JobId;

const COMPONENT: &str = "superi-concurrency.worker_pool";

/// Immutable resource bounds for one worker pool.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorkerPoolConfig {
    worker_count: usize,
    queue_capacity: usize,
}

impl WorkerPoolConfig {
    /// Hard safety ceiling for explicitly configured workers in one pool.
    pub const MAX_WORKERS: usize = 256;

    /// Creates explicit worker and pending-queue bounds.
    pub fn new(worker_count: usize, queue_capacity: usize) -> Result<Self> {
        if worker_count == 0 || worker_count > Self::MAX_WORKERS {
            return Err(pool_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "worker count is outside the supported bounded range",
                "configure",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "worker_bounds")
                    .with_field("worker_count", worker_count.to_string())
                    .with_field("maximum_workers", Self::MAX_WORKERS.to_string()),
            ));
        }
        if queue_capacity == 0 {
            return Err(pool_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "worker queue capacity must be greater than zero",
                "configure",
            ));
        }
        Ok(Self {
            worker_count,
            queue_capacity,
        })
    }

    /// Creates a resource-aware default while reserving capacity for UI, playback, and audio.
    ///
    /// The standard library value is an estimate and is intentionally sampled only during
    /// configuration. Two logical execution slots are reserved when available. Callers with a
    /// measured workload may use [`Self::new`] to select another explicit bound.
    pub fn recommended(queue_capacity: usize) -> Result<Self> {
        let available = thread::available_parallelism()
            .map_or(1, std::num::NonZeroUsize::get)
            .min(Self::MAX_WORKERS);
        let worker_count = available.saturating_sub(2).max(1);
        Self::new(worker_count, queue_capacity)
    }

    /// Returns the maximum number of tasks that may execute concurrently.
    #[must_use]
    pub const fn worker_count(self) -> usize {
        self.worker_count
    }

    /// Returns the maximum number of tasks waiting for dispatch.
    #[must_use]
    pub const fn queue_capacity(self) -> usize {
        self.queue_capacity
    }
}

/// How a worker obtained one queued job.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum DispatchOrigin {
    /// The executing worker matched the job's preferred local owner.
    Owned,
    /// An idle worker took work from this preferred owner.
    Stolen {
        /// Index of the local queue that originally owned the job.
        from_worker: usize,
    },
}

impl DispatchOrigin {
    /// Returns the stable diagnostic code for this dispatch path.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Owned => "owned",
            Self::Stolen { .. } => "stolen",
        }
    }
}

/// Immutable context available while one job executes.
#[derive(Clone, Debug)]
pub struct JobExecutionContext {
    job_id: JobId,
    worker_index: usize,
    preferred_worker: usize,
    dispatch_origin: DispatchOrigin,
    control: JobControl,
    progress: JobProgress,
}

impl JobExecutionContext {
    /// Returns the stable identity of the executing job.
    #[must_use]
    pub const fn job_id(&self) -> JobId {
        self.job_id
    }

    /// Returns the worker that is executing this job.
    #[must_use]
    pub const fn worker_index(&self) -> usize {
        self.worker_index
    }

    /// Returns the local queue selected when the job was submitted.
    #[must_use]
    pub const fn preferred_worker(&self) -> usize {
        self.preferred_worker
    }

    /// Returns whether the job stayed local or was stolen.
    #[must_use]
    pub const fn dispatch_origin(&self) -> DispatchOrigin {
        self.dispatch_origin
    }

    /// Returns the cooperative cancellation and deadline controls.
    #[must_use]
    pub const fn control(&self) -> &JobControl {
        &self.control
    }

    /// Returns the contention-safe progress state.
    #[must_use]
    pub const fn progress(&self) -> &JobProgress {
        &self.progress
    }

    /// Checks cancellation and the monotonic deadline at a bounded work boundary.
    pub fn check(&self, operation: &'static str) -> Result<()> {
        self.control.check(self.job_id, operation)
    }
}

/// A completed job plus its observable dispatch ownership.
#[derive(Debug)]
pub struct JobExecution<T> {
    completion: JobCompletion<T>,
    worker_index: usize,
    preferred_worker: usize,
    dispatch_origin: DispatchOrigin,
}

impl<T> JobExecution<T> {
    /// Returns the typed terminal completion.
    #[must_use]
    pub const fn completion(&self) -> &JobCompletion<T> {
        &self.completion
    }

    /// Returns the worker that executed the job.
    #[must_use]
    pub const fn worker_index(&self) -> usize {
        self.worker_index
    }

    /// Returns the job's preferred local owner.
    #[must_use]
    pub const fn preferred_worker(&self) -> usize {
        self.preferred_worker
    }

    /// Returns whether the dispatch was owned or stolen.
    #[must_use]
    pub const fn dispatch_origin(&self) -> DispatchOrigin {
        self.dispatch_origin
    }

    /// Consumes the dispatch report and returns the typed completion.
    #[must_use]
    pub fn into_completion(self) -> JobCompletion<T> {
        self.completion
    }
}

/// One-shot observation handle for a submitted job.
#[must_use = "retain the handle to observe the job's terminal result"]
pub struct JobTaskHandle<T> {
    job_id: JobId,
    receiver: Receiver<JobExecution<T>>,
}

impl<T> JobTaskHandle<T> {
    /// Returns the stable identity of the submitted job.
    #[must_use]
    pub const fn job_id(&self) -> JobId {
        self.job_id
    }

    /// Observes completion without blocking the caller.
    pub fn try_completion(&self) -> Result<Option<JobExecution<T>>> {
        match self.receiver.try_recv() {
            Ok(execution) => Ok(Some(execution)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(disconnected_error(self.job_id)),
        }
    }

    /// Waits for the terminal result from a blocking-safe caller.
    ///
    /// UI, engine-control, playback, and audio domains must poll with
    /// [`Self::try_completion`] or arrange asynchronous publication instead.
    pub fn wait(&self) -> Result<JobExecution<T>> {
        if let Some(domain) = current_execution_domain() {
            if !domain.policy().allows_blocking() {
                return Err(pool_error(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "current execution domain cannot block on a worker result",
                    "wait",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "blocking_domain")
                        .with_field("job_id", self.job_id.to_string())
                        .with_field("domain", domain.code()),
                ));
            }
        }
        self.receiver
            .recv()
            .map_err(|_| disconnected_error(self.job_id))
    }
}

impl<T> fmt::Debug for JobTaskHandle<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JobTaskHandle")
            .field("job_id", &self.job_id)
            .finish_non_exhaustive()
    }
}

/// Coherent worker-pool state for diagnostics and backpressure decisions.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorkerPoolSnapshot {
    worker_count: usize,
    queue_capacity: usize,
    queued_jobs: usize,
    active_workers: usize,
    maximum_active_workers: usize,
    owned_dispatches: u64,
    stolen_dispatches: u64,
    completed_jobs: u64,
    accepting_jobs: bool,
}

impl WorkerPoolSnapshot {
    /// Returns the configured parallel execution bound.
    #[must_use]
    pub const fn worker_count(self) -> usize {
        self.worker_count
    }

    /// Returns the configured pending queue bound.
    #[must_use]
    pub const fn queue_capacity(self) -> usize {
        self.queue_capacity
    }

    /// Returns the number of jobs waiting in local queues.
    #[must_use]
    pub const fn queued_jobs(self) -> usize {
        self.queued_jobs
    }

    /// Returns the number of workers currently running user jobs.
    #[must_use]
    pub const fn active_workers(self) -> usize {
        self.active_workers
    }

    /// Returns the highest observed active worker count.
    #[must_use]
    pub const fn maximum_active_workers(self) -> usize {
        self.maximum_active_workers
    }

    /// Returns the number of jobs dispatched by their preferred worker.
    #[must_use]
    pub const fn owned_dispatches(self) -> u64 {
        self.owned_dispatches
    }

    /// Returns the number of jobs dispatched by a stealing worker.
    #[must_use]
    pub const fn stolen_dispatches(self) -> u64 {
        self.stolen_dispatches
    }

    /// Returns the number of jobs whose task closure left the worker loop.
    #[must_use]
    pub const fn completed_jobs(self) -> u64 {
        self.completed_jobs
    }

    /// Returns whether new submissions are accepted.
    #[must_use]
    pub const fn is_accepting_jobs(self) -> bool {
        self.accepting_jobs
    }
}

/// Final diagnostics returned after every queued job drains and workers join.
pub type WorkerPoolShutdownReport = WorkerPoolSnapshot;

type ErasedTask = Box<dyn FnOnce(JobExecutionContext) + Send + 'static>;

struct PoolTask {
    control: JobControl,
    progress: JobProgress,
    run: ErasedTask,
}

struct PoolState {
    queues: Vec<PriorityScheduler<PoolTask>>,
    queued_ids: BTreeSet<JobId>,
    queue_capacity: usize,
    next_owner: usize,
    service_cursor: usize,
    active_workers: usize,
    maximum_active_workers: usize,
    owned_dispatches: u64,
    stolen_dispatches: u64,
    completed_jobs: u64,
    accepting_jobs: bool,
}

impl PoolState {
    fn new(config: WorkerPoolConfig) -> Self {
        Self {
            queues: (0..config.worker_count)
                .map(|_| PriorityScheduler::new())
                .collect(),
            queued_ids: BTreeSet::new(),
            queue_capacity: config.queue_capacity,
            next_owner: 0,
            service_cursor: 0,
            active_workers: 0,
            maximum_active_workers: 0,
            owned_dispatches: 0,
            stolen_dispatches: 0,
            completed_jobs: 0,
            accepting_jobs: true,
        }
    }

    fn snapshot(&self) -> WorkerPoolSnapshot {
        WorkerPoolSnapshot {
            worker_count: self.queues.len(),
            queue_capacity: self.queue_capacity,
            queued_jobs: self.queued_ids.len(),
            active_workers: self.active_workers,
            maximum_active_workers: self.maximum_active_workers,
            owned_dispatches: self.owned_dispatches,
            stolen_dispatches: self.stolen_dispatches,
            completed_jobs: self.completed_jobs,
            accepting_jobs: self.accepting_jobs,
        }
    }

    fn enqueue(&mut self, job: ScheduledJob<PoolTask>) -> Result<usize> {
        if !self.accepting_jobs {
            return Err(pool_error(
                ErrorCategory::Unavailable,
                Recoverability::Terminal,
                "worker pool is shutting down",
                "submit",
            ));
        }
        if self.queued_ids.len() >= self.queue_capacity {
            return Err(pool_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::Retryable,
                "worker queue reached its configured capacity",
                "submit",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "queue_capacity")
                    .with_field("queue_capacity", self.queue_capacity.to_string())
                    .with_field("queued_jobs", self.queued_ids.len().to_string()),
            ));
        }
        let job_id = job.id();
        if !self.queued_ids.insert(job_id) {
            return Err(pool_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "job identity is already queued in the worker pool",
                "submit",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "queued_identity")
                    .with_field("job_id", job_id.to_string()),
            ));
        }

        let owner = self.next_owner;
        self.next_owner = (self.next_owner + 1) % self.queues.len();
        if let Err(error) = self.queues[owner].enqueue(job) {
            self.queued_ids.remove(&job_id);
            return Err(error);
        }
        Ok(owner)
    }

    fn take(&mut self, worker_index: usize) -> Option<DispatchedTask> {
        if self.queued_ids.is_empty() {
            return None;
        }

        for _ in 0..SERVICE_PATTERN.len() {
            let priority = SERVICE_PATTERN[self.service_cursor];
            self.service_cursor = (self.service_cursor + 1) % SERVICE_PATTERN.len();
            if let Some(job) = self.queues[worker_index].next_job_for(priority) {
                return Some(self.record_dispatch(job, worker_index, worker_index));
            }
            for distance in 1..self.queues.len() {
                let victim = (worker_index + distance) % self.queues.len();
                if let Some(job) = self.queues[victim].next_job_for(priority) {
                    return Some(self.record_dispatch(job, worker_index, victim));
                }
            }
        }

        debug_assert!(false, "a nonempty pool must expose one weighted priority");
        None
    }

    fn record_dispatch(
        &mut self,
        job: ScheduledJob<PoolTask>,
        worker_index: usize,
        preferred_worker: usize,
    ) -> DispatchedTask {
        let removed = self.queued_ids.remove(&job.id());
        debug_assert!(removed, "pool identity index must match local queues");
        self.active_workers += 1;
        self.maximum_active_workers = self.maximum_active_workers.max(self.active_workers);
        let origin = if worker_index == preferred_worker {
            self.owned_dispatches = self.owned_dispatches.saturating_add(1);
            DispatchOrigin::Owned
        } else {
            self.stolen_dispatches = self.stolen_dispatches.saturating_add(1);
            DispatchOrigin::Stolen {
                from_worker: preferred_worker,
            }
        };
        DispatchedTask {
            job,
            preferred_worker,
            origin,
        }
    }

    fn record_completion(&mut self) {
        debug_assert!(self.active_workers > 0);
        self.active_workers = self.active_workers.saturating_sub(1);
        self.completed_jobs = self.completed_jobs.saturating_add(1);
    }
}

struct DispatchedTask {
    job: ScheduledJob<PoolTask>,
    preferred_worker: usize,
    origin: DispatchOrigin,
}

struct SharedPool {
    state: Mutex<PoolState>,
    work_available: Condvar,
}

/// A bounded pool of background-job workers with per-worker priority queues.
pub struct BoundedWorkerPool {
    shared: Arc<SharedPool>,
    workers: Vec<JoinHandle<()>>,
}

impl BoundedWorkerPool {
    /// Starts exactly the configured number of managed workers.
    pub fn new(config: WorkerPoolConfig) -> Result<Self> {
        let shared = Arc::new(SharedPool {
            state: Mutex::new(PoolState::new(config)),
            work_available: Condvar::new(),
        });
        let mut workers = Vec::with_capacity(config.worker_count);

        for worker_index in 0..config.worker_count {
            let worker_shared = Arc::clone(&shared);
            match thread::Builder::new()
                .name(format!("superi-background-job-{worker_index}"))
                .spawn(move || worker_loop(worker_index, &worker_shared))
            {
                Ok(worker) => workers.push(worker),
                Err(source) => {
                    lock_unpoisoned(&shared.state).accepting_jobs = false;
                    shared.work_available.notify_all();
                    for worker in workers {
                        let _ = worker.join();
                    }
                    return Err(Error::with_source(
                        ErrorCategory::Unavailable,
                        Recoverability::Retryable,
                        "failed to spawn a bounded worker thread",
                        source,
                    )
                    .with_context(
                        ErrorContext::new(COMPONENT, "spawn")
                            .with_field("worker_index", worker_index.to_string()),
                    ));
                }
            }
        }

        Ok(Self { shared, workers })
    }

    /// Submits one scheduled closure and returns its one-shot typed result handle.
    ///
    /// Submission never blocks for capacity. A full queue returns a retryable
    /// [`ErrorCategory::ResourceExhausted`] error so interactive callers retain control.
    pub fn submit<T, F>(
        &self,
        job: ScheduledJob<F>,
        control: JobControl,
        progress: JobProgress,
    ) -> Result<JobTaskHandle<T>>
    where
        T: Send + 'static,
        F: FnOnce(JobExecutionContext) -> Result<T> + Send + 'static,
    {
        let job_id = job.id();
        let kind = job.kind();
        let priority = job.priority();
        let derived_media = job.derived_media().copied();
        let task = job.into_payload();
        let (sender, receiver) = mpsc::sync_channel(1);

        let run = Box::new(move |context: JobExecutionContext| {
            let started = Instant::now();
            let worker_index = context.worker_index;
            let preferred_worker = context.preferred_worker;
            let dispatch_origin = context.dispatch_origin;
            let completion_progress = context.progress.clone();
            let result = panic::catch_unwind(AssertUnwindSafe(|| {
                context.check("begin_job")?;
                task(context)
            }));
            let outcome = match result {
                Ok(Ok(value)) => JobOutcome::Succeeded(value),
                Ok(Err(error)) => outcome_from_error(error),
                Err(_panic) => JobOutcome::Failed(
                    pool_error(
                        ErrorCategory::Internal,
                        Recoverability::Terminal,
                        "worker job panicked during execution",
                        "execute",
                    )
                    .with_context(
                        ErrorContext::new(COMPONENT, "panic")
                            .with_field("job_id", job_id.to_string())
                            .with_field("worker_index", worker_index.to_string()),
                    ),
                ),
            };
            let completion = JobCompletion::new(
                job_id,
                outcome,
                completion_progress.snapshot(),
                started.elapsed(),
            );
            let execution = JobExecution {
                completion,
                worker_index,
                preferred_worker,
                dispatch_origin,
            };
            let _ = sender.send(execution);
        });
        let payload = PoolTask {
            control,
            progress,
            run,
        };
        let mut erased = ScheduledJob::new(job_id, kind, priority, payload);
        if let Some(request) = derived_media {
            erased = erased.with_derived_media(request);
        }

        let mut state = lock_unpoisoned(&self.shared.state);
        state.enqueue(erased)?;
        drop(state);
        self.shared.work_available.notify_one();
        Ok(JobTaskHandle { job_id, receiver })
    }

    /// Returns a coherent snapshot without waiting for active work.
    #[must_use]
    pub fn snapshot(&self) -> WorkerPoolSnapshot {
        lock_unpoisoned(&self.shared.state).snapshot()
    }

    /// Stops new submissions, drains queued jobs, joins every worker, and returns diagnostics.
    ///
    /// This operation blocks and must be called only from a domain whose execution policy permits
    /// blocking. Pool owners should arrange shutdown from their lifecycle coordinator rather than
    /// dropping the final pool value on the UI, playback, engine-control, or audio thread.
    pub fn shutdown(mut self) -> Result<WorkerPoolShutdownReport> {
        self.begin_shutdown();
        self.join_workers()?;
        Ok(self.snapshot())
    }

    fn begin_shutdown(&self) {
        lock_unpoisoned(&self.shared.state).accepting_jobs = false;
        self.shared.work_available.notify_all();
    }

    fn join_workers(&mut self) -> Result<()> {
        let workers = std::mem::take(&mut self.workers);
        let mut worker_panicked = false;
        for worker in workers {
            if worker.join().is_err() {
                worker_panicked = true;
            }
        }
        if worker_panicked {
            return Err(pool_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "bounded worker thread panicked outside job containment",
                "shutdown",
            ));
        }
        Ok(())
    }
}

impl fmt::Debug for BoundedWorkerPool {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BoundedWorkerPool")
            .field("snapshot", &self.snapshot())
            .finish_non_exhaustive()
    }
}

impl Drop for BoundedWorkerPool {
    fn drop(&mut self) {
        self.begin_shutdown();
        let _ = self.join_workers();
    }
}

fn worker_loop(worker_index: usize, shared: &SharedPool) {
    let Ok(_domain) = ExecutionDomain::BackgroundJob.enter_current() else {
        return;
    };
    loop {
        let dispatched = {
            let mut state = lock_unpoisoned(&shared.state);
            loop {
                if let Some(dispatched) = state.take(worker_index) {
                    break Some(dispatched);
                }
                if !state.accepting_jobs {
                    break None;
                }
                state = wait_unpoisoned(&shared.work_available, state);
            }
        };
        let Some(dispatched) = dispatched else {
            return;
        };

        let job_id = dispatched.job.id();
        let task = dispatched.job.into_payload();
        let context = JobExecutionContext {
            job_id,
            worker_index,
            preferred_worker: dispatched.preferred_worker,
            dispatch_origin: dispatched.origin,
            control: task.control,
            progress: task.progress,
        };
        let _ = panic::catch_unwind(AssertUnwindSafe(|| (task.run)(context)));

        lock_unpoisoned(&shared.state).record_completion();
        shared.work_available.notify_all();
    }
}

fn outcome_from_error<T>(error: Error) -> JobOutcome<T> {
    match error.category() {
        ErrorCategory::Cancelled => JobOutcome::Cancelled,
        ErrorCategory::Timeout => JobOutcome::DeadlineExceeded,
        _ => JobOutcome::Failed(error),
    }
}

fn disconnected_error(job_id: JobId) -> Error {
    pool_error(
        ErrorCategory::Unavailable,
        Recoverability::Terminal,
        "worker result disconnected before terminal publication",
        "observe",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "disconnected").with_field("job_id", job_id.to_string()),
    )
}

fn pool_error(
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
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn wait_unpoisoned<'a, T>(condvar: &Condvar, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
    condvar
        .wait(guard)
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
