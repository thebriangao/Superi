//! Predictive playback cache orchestration over the shared graph evaluator.
//!
//! The playback domain publishes bounded nearest-first plans to the shared worker pool and polls
//! structured completion without waiting. A newer observation cooperatively cancels older work at
//! frame boundaries. Evaluation failures remain degraded cache state, so transport and foreground
//! frame evaluation can continue through their authoritative paths.

use std::collections::VecDeque;
use std::sync::{Arc, Weak};

use superi_cache::frame::OwnedHostFrameMemoryCache;
use superi_cache::prefetch::PlaybackPrefetchPlan;
use superi_concurrency::jobs::{
    BoundedWorkerPool, JobCancellationToken, JobControl, JobKind, JobOutcome, JobPriority,
    JobProgress, JobTaskHandle, JobTerminalState, ScheduledJob,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::PixelBounds;
use superi_core::ids::JobId;
use superi_core::time::RationalTime;
use superi_graph::dag::GraphEndpoint;
use superi_graph::diagnostics::IntrospectNode;
use superi_graph::eval::{EvaluateNode, EvaluationRequest};
use superi_graph::headless::GraphEvaluationSnapshot;

const COMPONENT: &str = "superi-engine.playback_prefetch";

/// Exact frame-population seam executed by one playback prefetch worker.
///
/// Implementations must preserve project meaning and final output. Returning an error reports
/// replaceable cache degradation and never grants authority to alter transport state.
pub trait PlaybackPrefetchEvaluator: Send + Sync {
    /// Evaluates and retains one exact predicted frame.
    fn prefetch(&self, frame: RationalTime) -> Result<()>;
}

/// Shared-graph evaluator that populates one owned host cache identity.
pub struct GraphPlaybackPrefetchEvaluator<T, N, V> {
    snapshot: Arc<GraphEvaluationSnapshot<T, N>>,
    cache: Arc<OwnedHostFrameMemoryCache<V>>,
    output: GraphEndpoint,
    region: PixelBounds,
}

impl<T, N, V> GraphPlaybackPrefetchEvaluator<T, N, V> {
    /// Binds immutable graph state, exact output scope, and retained host-cache identity.
    #[must_use]
    pub const fn new(
        snapshot: Arc<GraphEvaluationSnapshot<T, N>>,
        cache: Arc<OwnedHostFrameMemoryCache<V>>,
        output: GraphEndpoint,
        region: PixelBounds,
    ) -> Self {
        Self {
            snapshot,
            cache,
            output,
            region,
        }
    }
}

impl<T, N, V> PlaybackPrefetchEvaluator for GraphPlaybackPrefetchEvaluator<T, N, V>
where
    T: Send + Sync + 'static,
    N: EvaluateNode<V> + IntrospectNode + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    fn prefetch(&self, frame: RationalTime) -> Result<()> {
        self.snapshot.evaluate_with_cache(
            EvaluationRequest::new(self.output, frame, self.region),
            self.cache.as_ref(),
        )?;
        Ok(())
    }
}

/// Immediate result of publishing one playback observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaybackPrefetchSubmission {
    generation: u64,
    job_id: JobId,
    planned_frames: usize,
    queued: bool,
}

impl PlaybackPrefetchSubmission {
    /// Returns the monotonically increasing prediction generation.
    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }

    /// Returns the caller-assigned worker identity.
    #[must_use]
    pub const fn job_id(self) -> JobId {
        self.job_id
    }

    /// Returns the bounded number of exact frames in this generation.
    #[must_use]
    pub const fn planned_frames(self) -> usize {
        self.planned_frames
    }

    /// Returns whether worker work was queued.
    #[must_use]
    pub const fn is_queued(self) -> bool {
        self.queued
    }
}

/// Nonblocking terminal observation for one prediction generation.
#[derive(Debug)]
pub struct PlaybackPrefetchCompletion {
    generation: u64,
    job_id: JobId,
    planned_frames: usize,
    completed_frames: usize,
    status: JobTerminalState,
    error: Option<Error>,
}

impl PlaybackPrefetchCompletion {
    /// Returns the prediction generation that produced this report.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the caller-assigned worker identity.
    #[must_use]
    pub const fn job_id(&self) -> JobId {
        self.job_id
    }

    /// Returns the planned frame count before timeline execution began.
    #[must_use]
    pub const fn planned_frames(&self) -> usize {
        self.planned_frames
    }

    /// Returns the exact count populated before the terminal state.
    #[must_use]
    pub const fn completed_frames(&self) -> usize {
        self.completed_frames
    }

    /// Returns success, cancellation, or degraded failure classification.
    #[must_use]
    pub const fn status(&self) -> JobTerminalState {
        self.status
    }

    /// Returns evaluator failure detail when cache population degraded.
    #[must_use]
    pub const fn error(&self) -> Option<&Error> {
        self.error.as_ref()
    }
}

struct PendingPrefetch {
    generation: u64,
    planned_frames: usize,
    handle: JobTaskHandle<usize>,
}

/// Playback-owned prediction publisher and nonblocking completion poller.
///
/// The controller stores a weak worker-pool reference so dropping it on the playback thread cannot
/// become worker-pool shutdown. The lifecycle owner must retain and shut down the pool from a
/// blocking-safe domain. Submission and polling require [`ExecutionDomain::Playback`].
pub struct PlaybackPrefetcher {
    pool: Weak<BoundedWorkerPool>,
    evaluator: Arc<dyn PlaybackPrefetchEvaluator>,
    generation: u64,
    active: Option<JobCancellationToken>,
    pending: Vec<PendingPrefetch>,
    ready: VecDeque<PlaybackPrefetchCompletion>,
}

impl PlaybackPrefetcher {
    /// Creates one playback-domain publisher without taking worker-pool lifecycle ownership.
    #[must_use]
    pub fn new(
        pool: &Arc<BoundedWorkerPool>,
        evaluator: Arc<dyn PlaybackPrefetchEvaluator>,
    ) -> Self {
        Self {
            pool: Arc::downgrade(pool),
            evaluator,
            generation: 0,
            active: None,
            pending: Vec::new(),
            ready: VecDeque::new(),
        }
    }

    /// Cancels older prediction and publishes one new bounded generation without waiting.
    pub fn submit(
        &mut self,
        job_id: JobId,
        plan: PlaybackPrefetchPlan,
    ) -> Result<PlaybackPrefetchSubmission> {
        ExecutionDomain::Playback.require_current()?;
        if let Some(active) = self.active.take() {
            active.cancel();
        }
        self.generation = self.generation.checked_add(1).ok_or_else(|| {
            playback_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::Terminal,
                "playback prefetch generation space is exhausted",
                "submit",
            )
        })?;
        let generation = self.generation;
        let planned_frames = plan.len();
        let submission = PlaybackPrefetchSubmission {
            generation,
            job_id,
            planned_frames,
            queued: !plan.is_empty(),
        };

        if plan.is_empty() {
            self.ready.push_back(PlaybackPrefetchCompletion {
                generation,
                job_id,
                planned_frames: 0,
                completed_frames: 0,
                status: JobTerminalState::Succeeded,
                error: None,
            });
            return Ok(submission);
        }

        let pool = self.pool.upgrade().ok_or_else(|| {
            playback_error(
                ErrorCategory::Unavailable,
                Recoverability::Degraded,
                "playback prefetch worker pool is no longer available",
                "submit",
            )
        })?;
        let requests = plan.requests().to_vec();
        let evaluator = self.evaluator.clone();
        let cancellation = JobCancellationToken::new();
        let control = JobControl::new().with_cancellation(cancellation.clone());
        let progress = JobProgress::with_total(planned_frames as u64)?;
        let task = move |context: superi_concurrency::jobs::JobExecutionContext| {
            for (index, request) in requests.into_iter().enumerate() {
                context.check("prefetch_frame")?;
                evaluator.prefetch(request.frame())?;
                context.progress().advance_to((index + 1) as u64)?;
            }
            Ok(planned_frames)
        };
        let handle = pool.submit::<usize, _>(
            ScheduledJob::new(job_id, JobKind::Cache, JobPriority::Playback, task),
            control,
            progress,
        )?;
        self.active = Some(cancellation);
        self.pending.push(PendingPrefetch {
            generation,
            planned_frames,
            handle,
        });
        Ok(submission)
    }

    /// Returns every currently terminal generation without blocking the playback thread.
    pub fn poll(&mut self) -> Result<Vec<PlaybackPrefetchCompletion>> {
        ExecutionDomain::Playback.require_current()?;
        let mut completed = self.ready.drain(..).collect::<Vec<_>>();
        let mut index = 0;
        while index < self.pending.len() {
            let Some(execution) = self.pending[index].handle.try_completion()? else {
                index += 1;
                continue;
            };
            let pending = self.pending.remove(index);
            let completion = execution.into_completion();
            let progress = completion.progress();
            let status = completion.status();
            let (completed_frames, error) = match completion.into_outcome() {
                JobOutcome::Succeeded(frame_count) => (frame_count, None),
                JobOutcome::Failed(error) => (progress.completed_units() as usize, Some(error)),
                JobOutcome::Cancelled
                | JobOutcome::DeadlineExceeded
                | JobOutcome::DependencyFailed(_) => (progress.completed_units() as usize, None),
                _ => (progress.completed_units() as usize, None),
            };
            completed.push(PlaybackPrefetchCompletion {
                generation: pending.generation,
                job_id: pending.handle.job_id(),
                planned_frames: pending.planned_frames,
                completed_frames,
                status,
                error,
            });
        }
        Ok(completed)
    }

    /// Cooperatively cancels the newest prediction without waiting for worker completion.
    pub fn cancel(&mut self) -> Result<()> {
        ExecutionDomain::Playback.require_current()?;
        if let Some(active) = self.active.take() {
            active.cancel();
        }
        Ok(())
    }
}

impl Drop for PlaybackPrefetcher {
    fn drop(&mut self) {
        if let Some(active) = self.active.take() {
            active.cancel();
        }
    }
}

fn playback_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
