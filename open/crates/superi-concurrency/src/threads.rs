//! Explicit execution domains and thread ownership.
//!
//! Platform integrations attach the operating-system UI thread and real-time
//! audio callback with [`ExecutionDomain::enter_current`]. Superi owns named
//! threads for engine control, playback scheduling, render coordination,
//! background jobs, and GPU submission through [`ExecutionDomain::spawn`]. A
//! thread can enter only one domain at a time, and consumers can enforce their
//! ownership contract with [`ExecutionDomain::require_current`].
//!
//! This module establishes ownership only. Job priorities, cancellation,
//! deadlines, dependencies, work stealing, clocks, and backpressure are built
//! by their focused scheduling modules.

use std::cell::Cell;
use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;
use std::thread::{self, JoinHandle, ThreadId};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result, ResultExt};

const COMPONENT: &str = "superi-concurrency.threads";

std::thread_local! {
    static CURRENT_DOMAIN: Cell<Option<ExecutionDomain>> = const { Cell::new(None) };
}

/// A distinct execution ownership boundary in the open engine.
///
/// A domain is not a priority class or a general worker pool. It identifies
/// which responsibility owns the currently executing thread. Render and
/// background work may use multiple threads, but every one of those threads
/// remains in exactly one domain while it executes Superi work.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ExecutionDomain {
    /// Operating-system main thread for Tauri, windows, input, and viewport geometry.
    Ui,
    /// Single serialized owner of authoritative mutable engine and project state.
    EngineControl,
    /// Dedicated playback scheduler and master playback-clock owner.
    Playback,
    /// Render coordinator operating on immutable project snapshots.
    Render,
    /// Platform-owned real-time audio callback.
    Audio,
    /// Blocking I/O, analysis, caching, proxy, and export workers.
    BackgroundJob,
    /// Sole GPU queue submission, presentation, retirement, and recovery owner.
    GpuSubmission,
}

impl ExecutionDomain {
    /// Every domain in stable diagnostic order.
    pub const ALL: &'static [Self] = &[
        Self::Ui,
        Self::EngineControl,
        Self::Playback,
        Self::Render,
        Self::Audio,
        Self::BackgroundJob,
        Self::GpuSubmission,
    ];

    /// Returns the stable diagnostic code for this domain.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Ui => "ui",
            Self::EngineControl => "engine_control",
            Self::Playback => "playback",
            Self::Render => "render",
            Self::Audio => "audio",
            Self::BackgroundJob => "background_job",
            Self::GpuSubmission => "gpu_submission",
        }
    }

    /// Returns the stable name assigned to a managed domain thread.
    ///
    /// UI and audio are platform-owned, so their names are diagnostic labels
    /// rather than names applied to those external threads.
    #[must_use]
    pub const fn thread_name(self) -> &'static str {
        match self {
            Self::Ui => "superi-ui",
            Self::EngineControl => "superi-engine-control",
            Self::Playback => "superi-playback",
            Self::Render => "superi-render",
            Self::Audio => "superi-audio",
            Self::BackgroundJob => "superi-background-job",
            Self::GpuSubmission => "superi-gpu-submission",
        }
    }

    /// Returns the execution restrictions attached to this domain.
    #[must_use]
    pub const fn policy(self) -> ExecutionPolicy {
        match self {
            Self::Ui => ExecutionPolicy::new(true, false, true),
            Self::EngineControl | Self::Playback => ExecutionPolicy::new(false, false, true),
            Self::Render | Self::BackgroundJob | Self::GpuSubmission => {
                ExecutionPolicy::new(false, true, true)
            }
            Self::Audio => ExecutionPolicy::new(true, false, false),
        }
    }

    /// Enters this domain on the current thread until the returned guard drops.
    ///
    /// The UI shell calls this once around its event loop. Audio backends call
    /// it at the platform callback boundary. The successful audio path performs
    /// no allocation and takes no lock. Entering any second domain before the
    /// first guard drops is rejected.
    ///
    /// The guard is intentionally neither `Send` nor `Sync`, so ownership cannot
    /// be transferred away from the thread that entered the domain.
    ///
    /// ```compile_fail
    /// use superi_concurrency::threads::ExecutionDomain;
    ///
    /// let guard = ExecutionDomain::Ui.enter_current().unwrap();
    /// std::thread::spawn(move || drop(guard));
    /// ```
    pub fn enter_current(self) -> Result<ExecutionDomainGuard> {
        let existing = CURRENT_DOMAIN.with(|current| {
            let existing = current.get();
            if existing.is_none() {
                current.set(Some(self));
            }
            existing
        });

        if let Some(actual) = existing {
            return Err(domain_conflict("enter_domain", self, Some(actual)));
        }

        Ok(ExecutionDomainGuard {
            domain: self,
            thread_id: self
                .policy()
                .allows_allocation()
                .then(|| thread::current().id()),
            thread_owner: PhantomData,
        })
    }

    /// Verifies that the current thread is executing in this domain.
    pub fn require_current(self) -> Result<()> {
        let actual = current_execution_domain();
        if actual == Some(self) {
            return Ok(());
        }
        Err(domain_conflict("require_domain", self, actual))
    }

    /// Spawns one named Superi-owned thread in this execution domain.
    ///
    /// The worker receives a thread-bound guard and may retain it for a complete
    /// event loop. UI and audio must be attached by their platform owners with
    /// [`Self::enter_current`] and are rejected here. This function does not
    /// create a job queue or choose worker counts.
    pub fn spawn<T, F>(self, worker: F) -> Result<ExecutionDomainThread<T>>
    where
        T: Send + 'static,
        F: FnOnce(&ExecutionDomainGuard) -> Result<T> + Send + 'static,
    {
        if self.policy().is_platform_owned() {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "platform-owned execution domains cannot spawn managed threads",
            )
            .with_context(domain_context("spawn_domain", self)));
        }

        let handle = thread::Builder::new()
            .name(self.thread_name().to_owned())
            .spawn(move || {
                let guard = self.enter_current()?;
                worker(&guard).with_error_context(domain_context("execute_domain", self))
            })
            .map_err(|source| {
                Error::with_source(
                    ErrorCategory::Unavailable,
                    Recoverability::Retryable,
                    "failed to spawn execution domain thread",
                    source,
                )
                .with_context(domain_context("spawn_domain", self))
            })?;

        Ok(ExecutionDomainThread {
            domain: self,
            handle,
        })
    }
}

impl fmt::Display for ExecutionDomain {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// Static restrictions for one execution domain.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ExecutionPolicy {
    platform_owned: bool,
    allows_blocking: bool,
    allows_allocation: bool,
}

impl ExecutionPolicy {
    const fn new(platform_owned: bool, allows_blocking: bool, allows_allocation: bool) -> Self {
        Self {
            platform_owned,
            allows_blocking,
            allows_allocation,
        }
    }

    /// Returns whether an external platform thread must enter this domain.
    #[must_use]
    pub const fn is_platform_owned(self) -> bool {
        self.platform_owned
    }

    /// Returns whether the domain may perform bounded blocking operations.
    ///
    /// A `false` result prohibits waiting on rendering, I/O, GPU completion, or
    /// locks. It does not prohibit the platform UI event wait or playback clock
    /// pacing performed by the owner of those loops.
    #[must_use]
    pub const fn allows_blocking(self) -> bool {
        self.allows_blocking
    }

    /// Returns whether work in the domain may allocate or free memory.
    ///
    /// The audio callback is the only allocation-free domain. Its buffers and
    /// state must be prepared outside the callback.
    #[must_use]
    pub const fn allows_allocation(self) -> bool {
        self.allows_allocation
    }
}

/// Thread-bound proof that the current thread entered one execution domain.
///
/// Dropping the guard exits the domain. The `Rc` marker prevents `Send` and
/// `Sync` implementations, while [`ThreadId`] makes ownership inspectable for
/// diagnostics and tests.
#[derive(Debug)]
pub struct ExecutionDomainGuard {
    domain: ExecutionDomain,
    thread_id: Option<ThreadId>,
    thread_owner: PhantomData<Rc<()>>,
}

impl ExecutionDomainGuard {
    /// Returns the domain owned by this guard.
    #[must_use]
    pub const fn domain(&self) -> ExecutionDomain {
        self.domain
    }

    /// Returns the opaque process-lifetime identity of the owning thread.
    ///
    /// The real-time audio guard returns `None` because obtaining a standard
    /// library thread handle is not part of its allocation-free callback path.
    #[must_use]
    pub const fn thread_id(&self) -> Option<ThreadId> {
        self.thread_id
    }
}

impl Drop for ExecutionDomainGuard {
    fn drop(&mut self) {
        CURRENT_DOMAIN.with(|current| {
            debug_assert_eq!(current.get(), Some(self.domain));
            current.set(None);
        });
    }
}

/// Join permission for one managed execution-domain thread.
#[must_use = "retain the handle to observe completion and worker failure"]
pub struct ExecutionDomainThread<T> {
    domain: ExecutionDomain,
    handle: JoinHandle<Result<T>>,
}

impl<T> ExecutionDomainThread<T> {
    /// Returns the domain hosted by this thread.
    #[must_use]
    pub const fn domain(&self) -> ExecutionDomain {
        self.domain
    }

    /// Returns the name assigned to this thread.
    #[must_use]
    pub fn thread_name(&self) -> &str {
        self.handle
            .thread()
            .name()
            .unwrap_or_else(|| self.domain.thread_name())
    }

    /// Returns the opaque process-lifetime identity of this thread.
    #[must_use]
    pub fn thread_id(&self) -> ThreadId {
        self.handle.thread().id()
    }

    /// Waits for worker completion and preserves structured worker failures.
    ///
    /// Joining is intended for shutdown, tests, and other blocking-safe callers.
    /// UI, engine-control, playback, and audio code must observe completion
    /// asynchronously instead of synchronously joining a worker.
    pub fn join(self) -> Result<T> {
        let domain = self.domain;
        match self.handle.join() {
            Ok(result) => result,
            Err(_) => Err(Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "execution domain thread panicked",
            )
            .with_context(domain_context("join_domain", domain))),
        }
    }
}

impl<T> fmt::Debug for ExecutionDomainThread<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ExecutionDomainThread")
            .field("domain", &self.domain)
            .field("thread_name", &self.thread_name())
            .field("thread_id", &self.thread_id())
            .finish_non_exhaustive()
    }
}

/// Returns the domain currently entered on this thread, if any.
#[must_use]
pub fn current_execution_domain() -> Option<ExecutionDomain> {
    CURRENT_DOMAIN.get()
}

fn domain_context(operation: &'static str, domain: ExecutionDomain) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation)
        .with_field("domain", domain.code())
        .with_field("thread_name", domain.thread_name())
}

fn domain_conflict(
    operation: &'static str,
    expected: ExecutionDomain,
    actual: Option<ExecutionDomain>,
) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        "current thread does not own the required execution domain",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("expected", expected.code())
            .with_field("actual", actual.map_or("unassigned", ExecutionDomain::code)),
    )
}
