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
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::device::GpuDevice;

const COMPONENT: &str = "superi-gpu.submission";
const POLL_INTERVAL: Duration = Duration::from_millis(1);
const FENCE_PENDING: u8 = 0;
const FENCE_COMPLETED: u8 = 1;
const FENCE_ABORTED: u8 = 2;
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
    status: Arc<AtomicU8>,
}

impl fmt::Debug for GpuFence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GpuFence")
            .field("scope", &self.scope)
            .field("value", &self.value)
            .field("submission_index", &self.submission_index)
            .field("status", &self.status())
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
        self.status() == GpuFenceStatus::Completed
    }

    /// Returns whether work is pending, completed, or aborted by device loss.
    #[must_use]
    pub fn status(&self) -> GpuFenceStatus {
        match self.status.load(Ordering::Acquire) {
            FENCE_PENDING => GpuFenceStatus::Pending,
            FENCE_COMPLETED => GpuFenceStatus::Completed,
            FENCE_ABORTED => GpuFenceStatus::Aborted,
            _ => unreachable!("GPU fence status is written only by this module"),
        }
    }
}

/// Completion state of one queue-scoped fence.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum GpuFenceStatus {
    /// Submitted work has not completed.
    Pending,
    /// Submitted work completed normally.
    Completed,
    /// Submitted work was abandoned after confirmed device loss.
    Aborted,
}

/// An exact snapshot of queue submission and retirement progress.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct GpuSubmissionProgress {
    last_submitted: u64,
    last_retired: u64,
    in_flight: u64,
    retained_resources: u64,
    aborted_submissions: u64,
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

    /// Returns the number of submissions abandoned after device loss.
    #[must_use]
    pub const fn aborted_submissions(self) -> u64 {
        self.aborted_submissions
    }
}

struct InFlightSubmission<'device> {
    value: u64,
    status: Arc<AtomicU8>,
    retained: GpuSubmissionResources<'device>,
}

struct SubmissionState<'device> {
    next_fence: u64,
    last_submitted: u64,
    last_retired: u64,
    retained_resources: u64,
    aborted_submissions: u64,
    in_flight: VecDeque<InFlightSubmission<'device>>,
}

impl Default for SubmissionState<'_> {
    fn default() -> Self {
        Self {
            next_fence: 1,
            last_submitted: 0,
            last_retired: 0,
            retained_resources: 0,
            aborted_submissions: 0,
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
        let _ = device.poll_status();
        device.ensure_available_for("create_submission_queue")?;
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
        self.device.ensure_available_for("submit_commands")?;
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
        let status = Arc::new(AtomicU8::new(FENCE_PENDING));
        let completion_signal = Arc::clone(&status);
        self.device.on_submitted_work_done(move || {
            let _ = completion_signal.compare_exchange(
                FENCE_PENDING,
                FENCE_COMPLETED,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
        });

        let mut state = self.state.borrow_mut();
        state.next_fence = next_fence;
        state.last_submitted = value;
        state.retained_resources = next_retained;
        state.in_flight.push_back(InFlightSubmission {
            value,
            status: Arc::clone(&status),
            retained,
        });

        Ok(GpuFence {
            scope: self.scope,
            value,
            submission_index,
            status,
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
        if let crate::device::GpuDeviceStatus::Lost(loss) = self.device.status() {
            return self.retire_lost(loss.reason());
        }
        let _ = self.device.poll_submissions();
        match self.device.status() {
            crate::device::GpuDeviceStatus::Available { .. } => self.retire_ready(),
            crate::device::GpuDeviceStatus::Lost(loss) => self.retire_lost(loss.reason()),
        }
    }

    /// Polls until the given fence signals, then retires every ready predecessor.
    ///
    /// This blocking operation belongs only on the GPU submission thread, never
    /// the UI, audio, playback, render-coordinator, or background-job threads.
    pub fn wait(&self, fence: &GpuFence) -> Result<GpuSubmissionProgress> {
        self.validate_fence(fence, "wait_for_fence")?;
        loop {
            let progress = self.poll();
            if fence.status() == GpuFenceStatus::Aborted {
                return Err(aborted_fence(self.scope, fence.value, "wait_for_fence"));
            }
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
            if fence.status() == GpuFenceStatus::Aborted {
                return Err(aborted_fence(
                    self.scope,
                    fence.value,
                    "wait_for_fence_timeout",
                ));
            }
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
            .is_some_and(|submission| submission.status.load(Ordering::Acquire) == FENCE_COMPLETED)
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

    fn retire_lost(&self, reason: crate::device::GpuDeviceLossReason) -> GpuSubmissionProgress {
        match reason {
            crate::device::GpuDeviceLossReason::Destroyed => {
                let _ = self.retire_ready();
                self.abort_pending(false)
            }
            crate::device::GpuDeviceLossReason::Unknown => self.abort_pending(true),
        }
    }

    fn abort_pending(&self, force_abort: bool) -> GpuSubmissionProgress {
        let mut state = self.state.borrow_mut();
        let mut completed_prefix = true;
        while let Some(submission) = state.in_flight.pop_front() {
            if force_abort {
                let previous = submission.status.swap(FENCE_ABORTED, Ordering::AcqRel);
                if previous != FENCE_ABORTED {
                    state.aborted_submissions = state
                        .aborted_submissions
                        .checked_add(1)
                        .expect("aborted submissions cannot exceed submitted fences");
                }
                completed_prefix = false;
            } else {
                match submission.status.compare_exchange(
                    FENCE_PENDING,
                    FENCE_ABORTED,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                ) {
                    Ok(_) => {
                        state.aborted_submissions = state
                            .aborted_submissions
                            .checked_add(1)
                            .expect("aborted submissions cannot exceed submitted fences");
                        completed_prefix = false;
                    }
                    Err(FENCE_COMPLETED) if completed_prefix => {
                        state.last_retired = submission.value;
                    }
                    Err(FENCE_COMPLETED) => {
                        submission.status.store(FENCE_ABORTED, Ordering::Release);
                        state.aborted_submissions = state
                            .aborted_submissions
                            .checked_add(1)
                            .expect("aborted submissions cannot exceed submitted fences");
                    }
                    Err(FENCE_ABORTED) => {
                        state.aborted_submissions = state
                            .aborted_submissions
                            .checked_add(1)
                            .expect("aborted submissions cannot exceed submitted fences");
                        completed_prefix = false;
                    }
                    Err(_) => unreachable!("GPU fence status is written only by this module"),
                }
            }
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
        aborted_submissions: state.aborted_submissions,
    }
}

fn aborted_fence(scope: u64, value: u64, operation: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "GPU submission was aborted because the device was lost",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("queue_scope", scope.to_string())
            .with_field("fence_value", value.to_string()),
    )
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

#[cfg(test)]
mod loss_contract {
    use super::*;
    use crate::device::{
        AdapterSelection, DeviceRequest, GpuDeviceLossReason, GpuInstance, InstanceOptions,
    };

    #[test]
    fn confirmed_loss_aborts_pending_fences_and_releases_submission_ownership() {
        let instance = GpuInstance::new(InstanceOptions::default()).unwrap();
        let Ok(adapter) = instance
            .enumerate_adapters()
            .select(&AdapterSelection::default())
        else {
            return;
        };
        let device = pollster::block_on(adapter.create_device(&DeviceRequest::default())).unwrap();
        let submissions = GpuSubmissionQueue::new(&device).unwrap();
        let fence = submissions
            .submit(std::iter::empty(), submissions.resources())
            .unwrap();

        device.record_loss_for_test(GpuDeviceLossReason::Unknown, "test driver reset");
        let progress = submissions.poll();

        assert_eq!(fence.status(), GpuFenceStatus::Aborted);
        assert_eq!(progress.in_flight(), 0);
        assert_eq!(progress.aborted_submissions(), 1);
        let error = submissions.wait(&fence).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Unavailable);
        assert_eq!(error.recoverability(), Recoverability::Retryable);
        drop(submissions);
        GpuSubmissionQueue::new(&device).unwrap_err();
    }

    #[test]
    fn loss_aborts_completed_callbacks_after_the_first_unfinished_fence() {
        let instance = GpuInstance::new(InstanceOptions::default()).unwrap();
        let Ok(adapter) = instance
            .enumerate_adapters()
            .select(&AdapterSelection::default())
        else {
            return;
        };
        let device = pollster::block_on(adapter.create_device(&DeviceRequest::default())).unwrap();
        let submissions = GpuSubmissionQueue::new(&device).unwrap();
        let first = submissions
            .submit(std::iter::empty(), submissions.resources())
            .unwrap();
        let second = submissions
            .submit(std::iter::empty(), submissions.resources())
            .unwrap();

        first.status.store(FENCE_PENDING, Ordering::Release);
        second.status.store(FENCE_COMPLETED, Ordering::Release);
        device.record_loss_for_test(GpuDeviceLossReason::Destroyed, "test device destroyed");

        let progress = submissions.poll();
        assert_eq!(first.status(), GpuFenceStatus::Aborted);
        assert_eq!(second.status(), GpuFenceStatus::Aborted);
        assert_eq!(progress.in_flight(), 0);
        assert_eq!(progress.aborted_submissions(), 2);
        let error = submissions.wait(&second).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Unavailable);
        assert_eq!(error.recoverability(), Recoverability::Retryable);
    }
}
