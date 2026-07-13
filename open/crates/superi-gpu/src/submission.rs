//! Explicit GPU submission, fences, completion polling, and resource retirement.
//!
//! One [`GpuSubmissionQueue`] exclusively owns submission for one [`GpuDevice`]
//! lifetime. Queue-scoped retention bundles move resource owners into an
//! in-flight record. Those owners are released only after wgpu reports that the
//! matching submission completed, preventing reusable allocations from
//! returning to a pool while the GPU can still access them.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::device::GpuDevice;

const COMPONENT: &str = "superi-gpu.submission";
const POLL_INTERVAL: Duration = Duration::from_millis(1);
static NEXT_SUBMISSION_SCOPE: AtomicU64 = AtomicU64::new(1);

trait RetainedOwner: Send + Sync {}

impl<T> RetainedOwner for T where T: Send + Sync {}

/// Resource owners retained by exactly one submission queue.
///
/// Add managed handles, pooled texture checkouts, uploaded frames, conversion
/// leases, or compound owners that must not be dropped until GPU completion.
/// The bundle is tagged for the queue that created it and cannot be submitted
/// through a replacement or foreign queue.
pub struct GpuSubmissionResources<'device> {
    scope: u64,
    owners: Vec<Box<dyn RetainedOwner + 'device>>,
}

impl fmt::Debug for GpuSubmissionResources<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GpuSubmissionResources")
            .field("scope", &self.scope)
            .field("len", &self.owners.len())
            .finish()
    }
}

impl<'device> GpuSubmissionResources<'device> {
    /// Retains one owner until the submission that consumes this bundle retires.
    pub fn retain<T>(&mut self, owner: T)
    where
        T: Send + Sync + 'device,
    {
        self.owners.push(Box::new(owner));
    }

    /// Returns the number of retained owners in this bundle.
    #[must_use]
    pub fn len(&self) -> usize {
        self.owners.len()
    }

    /// Returns whether this bundle retains no owners.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.owners.is_empty()
    }
}

/// A queue-scoped monotonic fence backed by one wgpu submission index.
///
/// Clones identify the same submission and share its completion signal. A
/// fence can be inspected from another thread, but only its owning
/// [`GpuSubmissionQueue`] may drive polling, waiting, and resource retirement.
#[derive(Clone)]
#[must_use = "retain the fence to inspect or wait for GPU completion"]
pub struct GpuFence {
    scope: u64,
    value: u64,
    submission_index: wgpu::SubmissionIndex,
    completed: Arc<AtomicBool>,
}

impl fmt::Debug for GpuFence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GpuFence")
            .field("scope", &self.scope)
            .field("value", &self.value)
            .field("submission_index", &self.submission_index)
            .field("completed", &self.is_signaled())
            .finish()
    }
}

impl GpuFence {
    /// Returns the queue-local monotonic fence value, starting at one.
    #[must_use]
    pub const fn value(&self) -> u64 {
        self.value
    }

    /// Reports whether wgpu has signaled completion.
    ///
    /// This does not drive wgpu callbacks or retire resources. Call
    /// [`GpuSubmissionQueue::poll`] on the GPU submission thread to make
    /// progress.
    #[must_use]
    pub fn is_signaled(&self) -> bool {
        self.completed.load(Ordering::Acquire)
    }
}

/// An exact snapshot of queue submission and retirement progress.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuSubmissionProgress {
    last_submitted: u64,
    last_retired: u64,
    in_flight: u64,
    retained_resources: u64,
}

impl GpuSubmissionProgress {
    /// Returns the newest fence value submitted through this owner.
    #[must_use]
    pub const fn last_submitted(self) -> u64 {
        self.last_submitted
    }

    /// Returns the newest consecutively retired fence value.
    #[must_use]
    pub const fn last_retired(self) -> u64 {
        self.last_retired
    }

    /// Returns the number of submitted records not yet retired.
    #[must_use]
    pub const fn in_flight(self) -> u64 {
        self.in_flight
    }

    /// Returns the number of owners retained across in-flight submissions.
    #[must_use]
    pub const fn retained_resources(self) -> u64 {
        self.retained_resources
    }
}

struct InFlightSubmission<'device> {
    value: u64,
    completed: Arc<AtomicBool>,
    retained: GpuSubmissionResources<'device>,
}

struct SubmissionState<'device> {
    next_fence: u64,
    last_submitted: u64,
    last_retired: u64,
    retained_resources: u64,
    in_flight: VecDeque<InFlightSubmission<'device>>,
}

impl Default for SubmissionState<'_> {
    fn default() -> Self {
        Self {
            next_fence: 1,
            last_submitted: 0,
            last_retired: 0,
            retained_resources: 0,
            in_flight: VecDeque::new(),
        }
    }
}

/// The exclusive command submission and retirement owner for one GPU device.
///
/// This type is intentionally neither `Send` nor `Sync`. Construct and retain
/// it on the dedicated GPU submission thread. Command encoders may be recorded
/// elsewhere and moved to that thread as finished wgpu command buffers.
///
/// ```compile_fail
/// use superi_gpu::submission::GpuSubmissionQueue;
///
/// fn require_send<T: Send>() {}
/// require_send::<GpuSubmissionQueue<'static>>();
/// ```
pub struct GpuSubmissionQueue<'device> {
    device: &'device GpuDevice,
    scope: u64,
    state: RefCell<SubmissionState<'device>>,
    thread_owner: PhantomData<Rc<()>>,
}

impl fmt::Debug for GpuSubmissionQueue<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GpuSubmissionQueue")
            .field("adapter", self.device.adapter())
            .field("scope", &self.scope)
            .field("progress", &self.progress())
            .finish_non_exhaustive()
    }
}

impl<'device> GpuSubmissionQueue<'device> {
    /// Claims the sole submission-owner role for this device lifetime.
    pub fn new(device: &'device GpuDevice) -> Result<Self> {
        device.claim_submission_owner()?;
        let scope = match NEXT_SUBMISSION_SCOPE.fetch_update(
            Ordering::Relaxed,
            Ordering::Relaxed,
            |current| current.checked_add(1),
        ) {
            Ok(scope) => scope,
            Err(_) => {
                device.release_submission_owner();
                return Err(exhausted(
                    "create_submission_queue",
                    "GPU submission queue identifiers",
                ));
            }
        };

        Ok(Self {
            device,
            scope,
            state: RefCell::new(SubmissionState::default()),
            thread_owner: PhantomData,
        })
    }

    /// Creates an empty resource-retention bundle scoped to this queue owner.
    #[must_use]
    pub fn resources(&self) -> GpuSubmissionResources<'device> {
        GpuSubmissionResources {
            scope: self.scope,
            owners: Vec::new(),
        }
    }

    /// Submits command buffers in iterator order and returns their completion fence.
    ///
    /// Empty command-buffer iterators are valid and create a fence for prior
    /// queue writes. The retention bundle is consumed only by its originating
    /// queue and remains owned by the in-flight record until retirement.
    pub fn submit<I>(
        &self,
        command_buffers: I,
        retained: GpuSubmissionResources<'device>,
    ) -> Result<GpuFence>
    where
        I: IntoIterator<Item = wgpu::CommandBuffer>,
    {
        if retained.scope != self.scope {
            return Err(conflict(
                "submit_commands",
                "submission resources belong to a different queue owner",
                self.scope,
                retained.scope,
            ));
        }

        let _ = self.poll();
        let retained_count = u64::try_from(retained.len())
            .map_err(|_| exhausted("submit_commands", "retained GPU submission resource count"))?;
        let (value, next_fence, next_retained) = {
            let state = self.state.borrow();
            let next_fence = state
                .next_fence
                .checked_add(1)
                .ok_or_else(|| exhausted("submit_commands", "GPU submission fence identifiers"))?;
            let next_retained = state
                .retained_resources
                .checked_add(retained_count)
                .ok_or_else(|| {
                    exhausted("submit_commands", "retained GPU submission resource count")
                })?;
            (state.next_fence, next_fence, next_retained)
        };

        let submission_index = self.device.submit_commands(command_buffers);
        let completed = Arc::new(AtomicBool::new(false));
        let completion_signal = Arc::clone(&completed);
        self.device.on_submitted_work_done(move || {
            completion_signal.store(true, Ordering::Release);
        });

        let mut state = self.state.borrow_mut();
        state.next_fence = next_fence;
        state.last_submitted = value;
        state.retained_resources = next_retained;
        state.in_flight.push_back(InFlightSubmission {
            value,
            completed: Arc::clone(&completed),
            retained,
        });

        Ok(GpuFence {
            scope: self.scope,
            value,
            submission_index,
            completed,
        })
    }

    /// Returns the current progress snapshot without polling wgpu.
    #[must_use]
    pub fn progress(&self) -> GpuSubmissionProgress {
        progress(&self.state.borrow())
    }

    /// Polls wgpu once, invokes ready completion callbacks, and retires ready owners.
    #[must_use]
    pub fn poll(&self) -> GpuSubmissionProgress {
        let _ = self.device.poll_submissions();
        self.retire_ready()
    }

    /// Polls until the given fence signals, then retires every ready predecessor.
    ///
    /// This blocking operation belongs only on the GPU submission thread, never
    /// the UI, audio, playback, render-coordinator, or background-job threads.
    pub fn wait(&self, fence: &GpuFence) -> Result<GpuSubmissionProgress> {
        self.validate_fence(fence, "wait_for_fence")?;
        loop {
            let progress = self.poll();
            if progress.last_retired >= fence.value {
                return Ok(progress);
            }
            std::thread::park_timeout(POLL_INTERVAL);
        }
    }

    /// Polls for at most `timeout`, returning `None` when the fence remains pending.
    pub fn wait_timeout(
        &self,
        fence: &GpuFence,
        timeout: Duration,
    ) -> Result<Option<GpuSubmissionProgress>> {
        self.validate_fence(fence, "wait_for_fence_timeout")?;
        let deadline = Instant::now().checked_add(timeout);
        loop {
            let progress = self.poll();
            if progress.last_retired >= fence.value {
                return Ok(Some(progress));
            }
            if deadline.map_or(true, |deadline| Instant::now() >= deadline) {
                return Ok(None);
            }
            std::thread::park_timeout(POLL_INTERVAL.min(timeout));
        }
    }

    pub(crate) fn ensure_device(&self, device: &GpuDevice) -> Result<()> {
        self.ensure_device_identity(device.identity(), "validate_submission_device")
    }

    pub(crate) fn ensure_device_identity(
        &self,
        identity: &Arc<()>,
        operation: &'static str,
    ) -> Result<()> {
        if Arc::ptr_eq(self.device.identity(), identity) {
            return Ok(());
        }
        Err(Error::new(
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            "submission queue and GPU work belong to different device lifetimes",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation)
                .with_field("queue_scope", self.scope.to_string()),
        ))
    }

    fn validate_fence(&self, fence: &GpuFence, operation: &'static str) -> Result<()> {
        let last_submitted = self.state.borrow().last_submitted;
        if fence.scope == self.scope && fence.value <= last_submitted {
            return Ok(());
        }
        Err(conflict(
            operation,
            "GPU fence belongs to a different or obsolete submission queue",
            self.scope,
            fence.scope,
        ))
    }

    fn retire_ready(&self) -> GpuSubmissionProgress {
        let mut state = self.state.borrow_mut();
        while state
            .in_flight
            .front()
            .is_some_and(|submission| submission.completed.load(Ordering::Acquire))
        {
            let submission = state
                .in_flight
                .pop_front()
                .expect("front was present while retiring GPU submission");
            state.last_retired = submission.value;
            let retained = u64::try_from(submission.retained.len())
                .expect("retained owner count fits in GPU diagnostics");
            state.retained_resources = state
                .retained_resources
                .checked_sub(retained)
                .expect("retained owner accounting remains balanced");
        }
        progress(&state)
    }
}

impl Drop for GpuSubmissionQueue<'_> {
    fn drop(&mut self) {
        while self.progress().in_flight != 0 {
            let _ = self.poll();
            if self.progress().in_flight != 0 {
                std::thread::park_timeout(POLL_INTERVAL);
            }
        }
        self.device.release_submission_owner();
    }
}

fn progress(state: &SubmissionState<'_>) -> GpuSubmissionProgress {
    GpuSubmissionProgress {
        last_submitted: state.last_submitted,
        last_retired: state.last_retired,
        in_flight: u64::try_from(state.in_flight.len())
            .expect("in-flight submission count fits in GPU diagnostics"),
        retained_resources: state.retained_resources,
    }
}

fn conflict(
    operation: &'static str,
    message: &'static str,
    expected_scope: u64,
    actual_scope: u64,
) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("expected_scope", expected_scope.to_string())
            .with_field("actual_scope", actual_scope.to_string()),
    )
}

fn exhausted(operation: &'static str, resource: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::Terminal,
        format!("{resource} are exhausted"),
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
