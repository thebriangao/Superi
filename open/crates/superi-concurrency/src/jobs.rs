//! Cooperative job lifecycle contracts shared by schedulers and workers.
//!
//! This module owns cancellation, monotonic deadlines, dependency resolution, progress, and
//! terminal results. Queue selection, priority policy, worker execution, and work stealing remain
//! separate scheduler responsibilities. Workers must call [`JobControl::check`] between bounded
//! units of work; cancellation cannot preempt an unbounded foreign or blocking call by itself.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::JobId;

/// Cloneable one-way cancellation shared by one job and its bounded child work.
///
/// The flag carries no adjacent data and therefore needs atomicity but no publication barrier.
/// Cancellation is idempotent and cannot be reset for an unrelated job.
#[derive(Clone, Debug, Default)]
pub struct JobCancellationToken(Arc<AtomicBool>);

impl JobCancellationToken {
    /// Creates an active token.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Requests cancellation. Repeated requests have no additional effect.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Returns whether cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}

/// The cooperative reason a job must stop before producing a normal result.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum JobInterruption {
    /// The owner or a related operation requested cancellation.
    Cancelled,
    /// The job reached its monotonic deadline.
    DeadlineExceeded,
}

impl JobInterruption {
    /// Returns the stable machine code used by diagnostics and future API projection.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Cancelled => "cancelled",
            Self::DeadlineExceeded => "deadline_exceeded",
        }
    }
}

/// Immutable cooperative cancellation and deadline state passed to one job.
#[derive(Clone, Debug, Default)]
pub struct JobControl {
    cancellation: JobCancellationToken,
    deadline: Option<Instant>,
}

impl JobControl {
    /// Creates active controls with no deadline and a fresh cancellation token.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the token so related work can observe the same cancellation request.
    #[must_use]
    pub fn with_cancellation(mut self, cancellation: JobCancellationToken) -> Self {
        self.cancellation = cancellation;
        self
    }

    /// Sets an absolute in-process monotonic deadline.
    #[must_use]
    pub fn with_deadline(mut self, deadline: Instant) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// Sets a deadline relative to now and rejects values the platform cannot represent.
    pub fn with_timeout(mut self, timeout: Duration) -> Result<Self> {
        let deadline = Instant::now().checked_add(timeout).ok_or_else(|| {
            job_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "job timeout is outside the supported monotonic clock range",
                "set_timeout",
                None,
            )
        })?;
        self.deadline = Some(deadline);
        Ok(self)
    }

    /// Returns the shared cancellation token.
    #[must_use]
    pub const fn cancellation_token(&self) -> &JobCancellationToken {
        &self.cancellation
    }

    /// Returns the opaque in-process deadline when one exists.
    #[must_use]
    pub const fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Returns time remaining, saturated to zero once the deadline passes.
    #[must_use]
    pub fn remaining(&self) -> Option<Duration> {
        self.deadline
            .map(|deadline| deadline.saturating_duration_since(Instant::now()))
    }

    /// Returns the interruption currently visible to the worker.
    ///
    /// Explicit cancellation takes precedence if cancellation and expiry are observed together.
    #[must_use]
    pub fn interruption(&self) -> Option<JobInterruption> {
        if self.cancellation.is_cancelled() {
            return Some(JobInterruption::Cancelled);
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Some(JobInterruption::DeadlineExceeded);
        }
        None
    }

    /// Checks for cancellation or deadline expiry at one bounded worker checkpoint.
    pub fn check(&self, job_id: JobId, operation: &'static str) -> Result<()> {
        match self.interruption() {
            None => Ok(()),
            Some(JobInterruption::Cancelled) => Err(job_error(
                ErrorCategory::Cancelled,
                Recoverability::Degraded,
                "job was cancelled",
                operation,
                Some(job_id),
            )),
            Some(JobInterruption::DeadlineExceeded) => Err(job_error(
                ErrorCategory::Timeout,
                Recoverability::Retryable,
                "job exceeded its deadline",
                operation,
                Some(job_id),
            )),
        }
    }
}

/// An immutable numeric progress snapshot suitable for polling or event publication.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct JobProgressSnapshot {
    completed_units: u64,
    total_units: Option<u64>,
    revision: u64,
}

impl JobProgressSnapshot {
    /// Returns the monotonically nondecreasing completed-unit count.
    #[must_use]
    pub const fn completed_units(self) -> u64 {
        self.completed_units
    }

    /// Returns the known total, or `None` for indeterminate work.
    #[must_use]
    pub const fn total_units(self) -> Option<u64> {
        self.total_units
    }

    /// Returns the update revision, incremented by each state-changing progress update.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }

    /// Returns whether determinate work has reached its exact total.
    #[must_use]
    pub const fn is_complete(self) -> bool {
        matches!(self.total_units, Some(total) if self.completed_units == total)
    }
}

#[derive(Debug)]
struct JobProgressState {
    completed_units: u64,
    total_units: Option<u64>,
    revision: u64,
}

/// Cloneable, contention-safe progress shared by the workers of one job.
///
/// Updates are validated and committed while holding one short mutex. No user code or blocking
/// operation runs while the lock is held, so every snapshot observes a complete update.
#[derive(Clone, Debug)]
pub struct JobProgress(Arc<Mutex<JobProgressState>>);

impl JobProgress {
    /// Creates indeterminate progress with no known total.
    #[must_use]
    pub fn indeterminate() -> Self {
        Self(Arc::new(Mutex::new(JobProgressState {
            completed_units: 0,
            total_units: None,
            revision: 0,
        })))
    }

    /// Creates determinate progress. A zero total is invalid; use [`Self::indeterminate`] instead.
    pub fn with_total(total_units: u64) -> Result<Self> {
        if total_units == 0 {
            return Err(progress_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "job progress total must be greater than zero",
                "create",
            ));
        }
        Ok(Self(Arc::new(Mutex::new(JobProgressState {
            completed_units: 0,
            total_units: Some(total_units),
            revision: 0,
        }))))
    }

    /// Returns a coherent snapshot without changing progress.
    #[must_use]
    pub fn snapshot(&self) -> JobProgressSnapshot {
        snapshot(&lock_unpoisoned(&self.0))
    }

    /// Advances to an absolute completed-unit count.
    ///
    /// Repeating the current value is idempotent. Moving backwards or beyond a known total is
    /// rejected rather than silently producing misleading progress.
    pub fn advance_to(&self, completed_units: u64) -> Result<JobProgressSnapshot> {
        let mut state = lock_unpoisoned(&self.0);
        if completed_units < state.completed_units {
            return Err(progress_error(
                ErrorCategory::Conflict,
                Recoverability::Degraded,
                "job progress cannot move backwards",
                "advance",
            )
            .with_context(
                ErrorContext::new("superi-concurrency.job_progress", "current")
                    .with_field("completed_units", state.completed_units.to_string())
                    .with_field("requested_units", completed_units.to_string()),
            ));
        }
        validate_progress_total(&state, completed_units)?;
        if completed_units == state.completed_units {
            return Ok(snapshot(&state));
        }
        state.revision = next_progress_revision(state.revision)?;
        state.completed_units = completed_units;
        Ok(snapshot(&state))
    }

    /// Atomically increments completed units under contention.
    pub fn increment(&self, units: u64) -> Result<JobProgressSnapshot> {
        let mut state = lock_unpoisoned(&self.0);
        if units == 0 {
            return Ok(snapshot(&state));
        }
        let completed_units = state.completed_units.checked_add(units).ok_or_else(|| {
            progress_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "job progress increment overflows the supported unit range",
                "increment",
            )
        })?;
        validate_progress_total(&state, completed_units)?;
        state.revision = next_progress_revision(state.revision)?;
        state.completed_units = completed_units;
        Ok(snapshot(&state))
    }

    /// Marks determinate progress complete and leaves indeterminate progress unchanged.
    pub fn finish(&self) -> Result<JobProgressSnapshot> {
        let mut state = lock_unpoisoned(&self.0);
        let Some(total_units) = state.total_units else {
            return Ok(snapshot(&state));
        };
        if state.completed_units == total_units {
            return Ok(snapshot(&state));
        }
        state.revision = next_progress_revision(state.revision)?;
        state.completed_units = total_units;
        Ok(snapshot(&state))
    }
}

impl Default for JobProgress {
    fn default() -> Self {
        Self::indeterminate()
    }
}

/// A validated, deterministic set of prerequisite jobs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobDependencies {
    owner: JobId,
    required: Vec<JobId>,
}

impl JobDependencies {
    /// Validates and sorts a job's direct prerequisites.
    ///
    /// Self-dependencies and duplicate identifiers are rejected. Cycle detection across multiple
    /// submitted jobs remains the scheduler's responsibility when it assembles the global graph.
    pub fn new<I>(owner: JobId, dependencies: I) -> Result<Self>
    where
        I: IntoIterator<Item = JobId>,
    {
        let mut required = BTreeSet::new();
        for dependency in dependencies {
            if dependency == owner {
                return Err(dependency_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "job cannot depend on itself",
                    "validate",
                    owner,
                    dependency,
                ));
            }
            if !required.insert(dependency) {
                return Err(dependency_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "job dependency is duplicated",
                    "validate",
                    owner,
                    dependency,
                ));
            }
        }
        Ok(Self {
            owner,
            required: required.into_iter().collect(),
        })
    }

    /// Creates a job with no prerequisites.
    #[must_use]
    pub const fn none(owner: JobId) -> Self {
        Self {
            owner,
            required: Vec::new(),
        }
    }

    /// Returns the job that owns this prerequisite set.
    #[must_use]
    pub const fn owner(&self) -> JobId {
        self.owner
    }

    /// Returns prerequisite identifiers in deterministic ascending order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &JobId> + DoubleEndedIterator {
        self.required.iter()
    }

    /// Returns the number of direct prerequisites.
    #[must_use]
    pub fn len(&self) -> usize {
        self.required.len()
    }

    /// Returns whether the job has no direct prerequisites.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.required.is_empty()
    }
}

/// The terminal state of a job without its type-specific value or failure detail.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum JobTerminalState {
    /// Work finished successfully.
    Succeeded,
    /// Work produced a classified shared error.
    Failed,
    /// Work stopped after cancellation.
    Cancelled,
    /// Work stopped after reaching its deadline.
    DeadlineExceeded,
    /// Work did not run because a prerequisite did not succeed.
    DependencyFailed,
}

impl JobTerminalState {
    /// Returns the stable machine code used by diagnostics and future API projection.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::DeadlineExceeded => "deadline_exceeded",
            Self::DependencyFailed => "dependency_failed",
        }
    }
}

/// A prerequisite that prevents its dependent job from running.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobDependencyFailure {
    job_id: JobId,
    state: JobTerminalState,
}

impl JobDependencyFailure {
    /// Creates a blocking dependency from a non-successful terminal state.
    pub fn new(job_id: JobId, state: JobTerminalState) -> Result<Self> {
        if state == JobTerminalState::Succeeded {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "a successful dependency cannot block a job",
            )
            .with_context(
                ErrorContext::new("superi-concurrency.job_dependency", "record_failure")
                    .with_field("dependency_id", job_id.to_string())
                    .with_field("state", state.code()),
            ));
        }
        Ok(Self { job_id, state })
    }

    /// Returns the blocking prerequisite identifier.
    #[must_use]
    pub const fn job_id(&self) -> JobId {
        self.job_id
    }

    /// Returns the prerequisite's terminal state.
    #[must_use]
    pub const fn state(&self) -> JobTerminalState {
        self.state
    }
}

/// A deterministic snapshot of prerequisite resolution.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum JobDependencyStatus {
    /// Every prerequisite succeeded and the dependent job may run.
    Ready,
    /// These prerequisites have not produced terminal results yet.
    Waiting(Vec<JobId>),
    /// These prerequisites ended without success, so the dependent job must not run.
    Blocked(Vec<JobDependencyFailure>),
}

/// Cloneable prerequisite state for one job.
///
/// Recording the same terminal state twice is idempotent. Replacing an observed terminal state is
/// rejected so retries cannot rewrite the dependency history underneath a waiting job.
#[derive(Clone, Debug)]
pub struct JobDependencyTracker {
    dependencies: JobDependencies,
    states: Arc<Mutex<BTreeMap<JobId, Option<JobTerminalState>>>>,
}

impl JobDependencyTracker {
    /// Creates a tracker whose prerequisites are all pending.
    #[must_use]
    pub fn new(dependencies: JobDependencies) -> Self {
        let states = dependencies
            .iter()
            .copied()
            .map(|job_id| (job_id, None))
            .collect();
        Self {
            dependencies,
            states: Arc::new(Mutex::new(states)),
        }
    }

    /// Returns the immutable dependency declaration.
    #[must_use]
    pub const fn dependencies(&self) -> &JobDependencies {
        &self.dependencies
    }

    /// Records one prerequisite's terminal state and returns the new resolution snapshot.
    pub fn record(
        &self,
        dependency: JobId,
        state: JobTerminalState,
    ) -> Result<JobDependencyStatus> {
        let mut states = lock_unpoisoned(&self.states);
        let recorded = states.get_mut(&dependency).ok_or_else(|| {
            dependency_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "terminal result does not belong to this job's dependencies",
                "record",
                self.dependencies.owner,
                dependency,
            )
        })?;
        match *recorded {
            Some(existing) if existing != state => {
                return Err(dependency_error(
                    ErrorCategory::Conflict,
                    Recoverability::Degraded,
                    "job dependency already has a different terminal result",
                    "record",
                    self.dependencies.owner,
                    dependency,
                )
                .with_context(
                    ErrorContext::new("superi-concurrency.job_dependency", "existing_result")
                        .with_field("existing_state", existing.code())
                        .with_field("requested_state", state.code()),
                ));
            }
            Some(_) => {}
            None => *recorded = Some(state),
        }
        Ok(dependency_status(&states))
    }

    /// Returns a coherent resolution snapshot without changing dependency state.
    #[must_use]
    pub fn status(&self) -> JobDependencyStatus {
        dependency_status(&lock_unpoisoned(&self.states))
    }
}

/// A type-specific terminal outcome with classified failure detail.
#[derive(Debug)]
#[non_exhaustive]
pub enum JobOutcome<T> {
    /// Work finished and produced its value.
    Succeeded(T),
    /// Work failed with the shared actionable error contract.
    Failed(Error),
    /// Work cooperatively stopped after cancellation.
    Cancelled,
    /// Work cooperatively stopped after reaching its deadline.
    DeadlineExceeded,
    /// Work did not run because this prerequisite did not succeed.
    DependencyFailed(JobDependencyFailure),
}

impl<T> JobOutcome<T> {
    /// Returns the outcome's stable terminal classification.
    #[must_use]
    pub const fn status(&self) -> JobTerminalState {
        match self {
            Self::Succeeded(_) => JobTerminalState::Succeeded,
            Self::Failed(_) => JobTerminalState::Failed,
            Self::Cancelled => JobTerminalState::Cancelled,
            Self::DeadlineExceeded => JobTerminalState::DeadlineExceeded,
            Self::DependencyFailed(_) => JobTerminalState::DependencyFailed,
        }
    }

    /// Returns the shared error when this is a failed outcome.
    #[must_use]
    pub const fn error(&self) -> Option<&Error> {
        match self {
            Self::Failed(error) => Some(error),
            _ => None,
        }
    }

    /// Returns the blocking prerequisite when dependency resolution failed.
    #[must_use]
    pub const fn dependency_failure(&self) -> Option<&JobDependencyFailure> {
        match self {
            Self::DependencyFailed(failure) => Some(failure),
            _ => None,
        }
    }

    /// Consumes the outcome and returns its successful value, when present.
    #[must_use]
    pub fn into_success(self) -> Option<T> {
        match self {
            Self::Succeeded(value) => Some(value),
            _ => None,
        }
    }
}

/// The complete observable result of one terminal job.
#[derive(Debug)]
pub struct JobCompletion<T> {
    job_id: JobId,
    outcome: JobOutcome<T>,
    progress: JobProgressSnapshot,
    elapsed: Duration,
}

impl<T> JobCompletion<T> {
    /// Creates a terminal report from the scheduler's final coherent observations.
    #[must_use]
    pub const fn new(
        job_id: JobId,
        outcome: JobOutcome<T>,
        progress: JobProgressSnapshot,
        elapsed: Duration,
    ) -> Self {
        Self {
            job_id,
            outcome,
            progress,
            elapsed,
        }
    }

    /// Returns the completed job identifier.
    #[must_use]
    pub const fn job_id(&self) -> JobId {
        self.job_id
    }

    /// Returns the structured terminal outcome.
    #[must_use]
    pub const fn outcome(&self) -> &JobOutcome<T> {
        &self.outcome
    }

    /// Returns the terminal classification without discarding failure detail.
    #[must_use]
    pub const fn status(&self) -> JobTerminalState {
        self.outcome.status()
    }

    /// Returns the final coherent progress snapshot.
    #[must_use]
    pub const fn progress(&self) -> JobProgressSnapshot {
        self.progress
    }

    /// Returns monotonic execution time measured by the scheduler.
    #[must_use]
    pub const fn elapsed(&self) -> Duration {
        self.elapsed
    }

    /// Consumes the completion and returns the type-specific outcome.
    #[must_use]
    pub fn into_outcome(self) -> JobOutcome<T> {
        self.outcome
    }
}

fn validate_progress_total(state: &JobProgressState, completed_units: u64) -> Result<()> {
    if state
        .total_units
        .is_some_and(|total_units| completed_units > total_units)
    {
        return Err(progress_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "job progress cannot exceed its known total",
            "advance",
        )
        .with_context(
            ErrorContext::new("superi-concurrency.job_progress", "bounds")
                .with_field("completed_units", completed_units.to_string())
                .with_field(
                    "total_units",
                    state.total_units.unwrap_or_default().to_string(),
                ),
        ));
    }
    Ok(())
}

fn next_progress_revision(revision: u64) -> Result<u64> {
    revision.checked_add(1).ok_or_else(|| {
        progress_error(
            ErrorCategory::ResourceExhausted,
            Recoverability::Terminal,
            "job progress revision space is exhausted",
            "advance",
        )
    })
}

fn snapshot(state: &JobProgressState) -> JobProgressSnapshot {
    JobProgressSnapshot {
        completed_units: state.completed_units,
        total_units: state.total_units,
        revision: state.revision,
    }
}

fn dependency_status(states: &BTreeMap<JobId, Option<JobTerminalState>>) -> JobDependencyStatus {
    let blocked = states
        .iter()
        .filter_map(|(&job_id, &state)| match state {
            Some(JobTerminalState::Succeeded) | None => None,
            Some(state) => Some(JobDependencyFailure { job_id, state }),
        })
        .collect::<Vec<_>>();
    if !blocked.is_empty() {
        return JobDependencyStatus::Blocked(blocked);
    }
    let waiting = states
        .iter()
        .filter_map(|(&job_id, state)| state.is_none().then_some(job_id))
        .collect::<Vec<_>>();
    if waiting.is_empty() {
        JobDependencyStatus::Ready
    } else {
        JobDependencyStatus::Waiting(waiting)
    }
}

fn job_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
    job_id: Option<JobId>,
) -> Error {
    let mut context = ErrorContext::new("superi-concurrency.job", operation);
    if let Some(job_id) = job_id {
        context.insert_field("job_id", job_id.to_string());
    }
    Error::new(category, recoverability, message).with_context(context)
}

fn progress_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
) -> Error {
    Error::new(category, recoverability, message).with_context(ErrorContext::new(
        "superi-concurrency.job_progress",
        operation,
    ))
}

fn dependency_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
    owner: JobId,
    dependency: JobId,
) -> Error {
    Error::new(category, recoverability, message).with_context(
        ErrorContext::new("superi-concurrency.job_dependency", operation)
            .with_field("job_id", owner.to_string())
            .with_field("dependency_id", dependency.to_string()),
    )
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    // State mutations in this module perform all fallible validation before changing a field and
    // never call user code while locked, so a caught panic cannot leave a partial invariant behind.
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
