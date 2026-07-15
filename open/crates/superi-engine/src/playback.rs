//! Predictive and foreground playback orchestration over shared subsystem owners.
//!
//! The playback domain publishes bounded nearest-first plans to the shared worker pool and polls
//! structured completion without waiting. A newer observation cooperatively cancels older work at
//! frame boundaries. Evaluation failures remain degraded cache state, so transport and foreground
//! frame evaluation can continue through their authoritative paths. Foreground work evaluates one
//! exact graph and cache value, performs display color execution, follows the shared audio or
//! monotonic clock, and retains ownership when bounded audio or viewport sinks apply backpressure.

use std::collections::VecDeque;
use std::sync::{Arc, Weak};
use std::time::Instant;

use superi_audio::playback::{OutputBufferError, OutputProducer, OutputWrite};
use superi_cache::frame::{FrameMemoryCache, OwnedHostFrameMemoryCache};
use superi_cache::key::{MediaCacheIdentity, ParameterStateFingerprint, RenderSettingsFingerprint};
use superi_cache::prefetch::PlaybackPrefetchPlan;
use superi_color::transform_out::{OutputColorTransform, OutputTargetKind};
use superi_color::working_space::WorkingImage;
use superi_concurrency::backpressure::HandoffSender;
use superi_concurrency::clock::{AudioMasterClock, PlaybackClock, PlaybackClockMode};
use superi_concurrency::jobs::{
    BoundedWorkerPool, JobCancellationToken, JobControl, JobKind, JobOutcome, JobPriority,
    JobProgress, JobTaskHandle, JobTerminalState, ScheduledJob,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::PixelBounds;
use superi_core::ids::{JobId, ProjectId};
use superi_core::pixel::AlphaMode;
use superi_core::time::{Duration, RationalTime};
use superi_graph::dag::GraphEndpoint;
use superi_graph::diagnostics::IntrospectNode;
use superi_graph::eval::{
    EvaluateNode, EvaluationCacheEntryKind, EvaluationCacheIdentity, EvaluationRequest,
    EvaluationValueCache,
};
use superi_graph::headless::GraphEvaluationSnapshot;
use superi_graph::node::GraphColorMetadata;
use superi_image::metadata::{ColorPipelineMetadata, ColorTransformStage};
use superi_image::value::Image;
use superi_media_io::decode::{VideoFormat, VideoFrame};
use superi_media_io::demux::MediaMetadata;

use crate::render::ViewportColorMetadata;

const COMPONENT: &str = "superi-engine.playback_prefetch";
const ORCHESTRATION_COMPONENT: &str = "superi-engine.playback_orchestration";

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

/// Immutable decoded-frame semantics retained beside a processed scene value.
///
/// Pixel storage remains owned by the decode and graph paths. This snapshot preserves the exact
/// source representation, timing, metadata, color history, and alpha meaning without copying a
/// decoded pixel buffer into the orchestration layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DecodedFrameMetadata {
    format: VideoFormat,
    timestamp: RationalTime,
    duration: Duration,
    metadata: MediaMetadata,
    color_pipeline: ColorPipelineMetadata,
}

impl DecodedFrameMetadata {
    /// Captures complete semantic provenance from one decoded frame.
    #[must_use]
    pub fn from_frame(frame: &VideoFrame) -> Self {
        Self {
            format: frame.format(),
            timestamp: frame.timestamp(),
            duration: frame.duration(),
            metadata: frame.metadata().clone(),
            color_pipeline: frame.color_pipeline().clone(),
        }
    }

    /// Returns the exact decoded representation.
    #[must_use]
    pub const fn format(&self) -> VideoFormat {
        self.format
    }

    /// Returns the exact decoded presentation timestamp.
    #[must_use]
    pub const fn timestamp(&self) -> RationalTime {
        self.timestamp
    }

    /// Returns the exact decoded presentation duration.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    /// Returns preserved source and decoder metadata.
    #[must_use]
    pub const fn metadata(&self) -> &MediaMetadata {
        &self.metadata
    }

    /// Returns the exact decoded color identity and ordered transform history.
    #[must_use]
    pub const fn color_pipeline(&self) -> &ColorPipelineMetadata {
        &self.color_pipeline
    }
}

/// One graph-produced scene value with exact playback timing and semantic provenance.
///
/// The scene pipeline must remain nonterminal. Viewport and export transforms branch from this
/// cacheable value so both consumers share graph results without reinterpreting source meaning.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackSceneFrame<V> {
    value: V,
    timestamp: RationalTime,
    duration: Duration,
    decoded: Vec<DecodedFrameMetadata>,
    color_pipeline: ColorPipelineMetadata,
    alpha_mode: AlphaMode,
}

impl<V> PlaybackSceneFrame<V> {
    /// Creates one cacheable scene result with explicit provenance and alpha semantics.
    pub fn new(
        value: V,
        timestamp: RationalTime,
        duration: Duration,
        decoded: Vec<DecodedFrameMetadata>,
        color_pipeline: ColorPipelineMetadata,
        alpha_mode: AlphaMode,
    ) -> Result<Self> {
        if timestamp.timebase() != duration.timebase() || duration.is_zero() {
            return Err(orchestration_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "playback scene timing must use one exact timebase and a nonzero duration",
                "create_scene_frame",
            ));
        }
        if color_pipeline.display_space().is_some() || color_pipeline.delivery_space().is_some() {
            return Err(orchestration_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "cacheable playback scene color must not contain a terminal transform",
                "create_scene_frame",
            ));
        }
        Ok(Self {
            value,
            timestamp,
            duration,
            decoded,
            color_pipeline,
            alpha_mode,
        })
    }

    /// Creates a scene result from one decoded frame without copying its pixel storage.
    pub fn from_decoded(
        value: V,
        decoded: &VideoFrame,
        color_pipeline: ColorPipelineMetadata,
        alpha_mode: AlphaMode,
    ) -> Result<Self> {
        if !pipeline_extends(decoded.color_pipeline(), &color_pipeline) {
            return Err(orchestration_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "playback scene color history must extend decoded color history",
                "create_scene_from_decoded",
            ));
        }
        Self::new(
            value,
            decoded.timestamp(),
            decoded.duration(),
            vec![DecodedFrameMetadata::from_frame(decoded)],
            color_pipeline,
            alpha_mode,
        )
    }

    /// Returns the graph-produced scene value.
    #[must_use]
    pub const fn value(&self) -> &V {
        &self.value
    }

    /// Releases the graph-produced scene value.
    #[must_use]
    pub fn into_value(self) -> V {
        self.value
    }

    /// Returns the exact output timeline coordinate.
    #[must_use]
    pub const fn timestamp(&self) -> RationalTime {
        self.timestamp
    }

    /// Returns the exact output duration.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    /// Returns every decoded input provenance snapshot retained by the graph result.
    #[must_use]
    pub fn decoded(&self) -> &[DecodedFrameMetadata] {
        &self.decoded
    }

    /// Returns complete nonterminal scene color history.
    #[must_use]
    pub const fn color_pipeline(&self) -> &ColorPipelineMetadata {
        &self.color_pipeline
    }

    /// Returns alpha association at the scene output boundary.
    #[must_use]
    pub const fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }
}

/// Complete non-graph identity inputs for one playback cache scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackCacheIdentity {
    project_id: ProjectId,
    media: Vec<MediaCacheIdentity>,
    parameters: ParameterStateFingerprint,
    color_pipeline: ColorPipelineMetadata,
    render_settings: RenderSettingsFingerprint,
}

impl PlaybackCacheIdentity {
    /// Captures project, source, parameter, color, and render-setting identity.
    #[must_use]
    pub fn new(
        project_id: ProjectId,
        media: Vec<MediaCacheIdentity>,
        parameters: ParameterStateFingerprint,
        color_pipeline: ColorPipelineMetadata,
        render_settings: RenderSettingsFingerprint,
    ) -> Self {
        Self {
            project_id,
            media,
            parameters,
            color_pipeline,
            render_settings,
        }
    }

    fn into_host_cache<V>(
        self,
        cache: Arc<FrameMemoryCache<PlaybackSceneFrame<V>>>,
    ) -> OwnedHostFrameMemoryCache<PlaybackSceneFrame<V>> {
        OwnedHostFrameMemoryCache::new(
            cache,
            self.project_id,
            self.media,
            self.parameters,
            self.color_pipeline,
            self.render_settings,
        )
    }
}

/// Display execution seam that leaves viewport submission to the sink owner.
pub trait PlaybackDisplayTransform<V>: Send + Sync {
    /// Display-ready value passed through the bounded viewport handoff.
    type Output: Send + 'static;

    /// Returns the exact cacheable scene pipeline accepted by this transform.
    fn scene_pipeline(&self) -> &ColorPipelineMetadata;

    /// Returns the exact scene alpha association accepted by this transform.
    fn scene_alpha_mode(&self) -> AlphaMode;

    /// Returns the terminal viewport color metadata published beside the output.
    fn viewport_color_metadata(&self) -> &ViewportColorMetadata;

    /// Returns alpha association after display execution.
    fn output_alpha_mode(&self) -> AlphaMode;

    /// Applies display color execution without presenting a native surface.
    fn transform(&self, scene: &V) -> Result<Self::Output>;
}

/// Concrete CPU display transform for canonical binary16 working images.
#[derive(Clone, Debug)]
pub struct CpuPlaybackDisplayTransform {
    scene_pipeline: ColorPipelineMetadata,
    viewport_color: ViewportColorMetadata,
    output_transform: OutputColorTransform,
}

impl CpuPlaybackDisplayTransform {
    /// Binds one nonterminal scene identity to one explicit monitoring transform.
    pub fn new(
        scene_pipeline: ColorPipelineMetadata,
        display_stage: ColorTransformStage,
        output_transform: OutputColorTransform,
    ) -> Result<Self> {
        if scene_pipeline.display_space().is_some() || scene_pipeline.delivery_space().is_some() {
            return Err(orchestration_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "CPU playback display input must be a nonterminal scene pipeline",
                "create_cpu_display_transform",
            ));
        }
        if output_transform.target_kind() != OutputTargetKind::Display
            || output_transform.source().color_space() != scene_pipeline.current_space()
            || output_transform.destination() != display_stage.destination()
        {
            return Err(orchestration_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "CPU playback display transform endpoints do not match color metadata",
                "create_cpu_display_transform",
            ));
        }
        let graph_color = GraphColorMetadata::new(scene_pipeline.clone());
        let cached = superi_cache::frame::CachedFrameColorMetadata::from_graph(&graph_color);
        let viewport_color = ViewportColorMetadata::from_cache(&cached, display_stage)?;
        Ok(Self {
            scene_pipeline,
            viewport_color,
            output_transform,
        })
    }
}

impl PlaybackDisplayTransform<WorkingImage> for CpuPlaybackDisplayTransform {
    type Output = Image;

    fn scene_pipeline(&self) -> &ColorPipelineMetadata {
        &self.scene_pipeline
    }

    fn scene_alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    fn viewport_color_metadata(&self) -> &ViewportColorMetadata {
        &self.viewport_color
    }

    fn output_alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    fn transform(&self, scene: &WorkingImage) -> Result<Self::Output> {
        if scene.space().color_space() != self.scene_pipeline.current_space() {
            return Err(orchestration_error(
                ErrorCategory::CorruptData,
                Recoverability::Degraded,
                "working image interpretation does not match playback scene color metadata",
                "transform_cpu_display",
            ));
        }
        self.output_transform.apply(scene)
    }
}

/// One display-ready frame published to the viewport owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlaybackViewportFrame<O> {
    output: O,
    timestamp: RationalTime,
    duration: Duration,
    decoded: Vec<DecodedFrameMetadata>,
    color_metadata: ViewportColorMetadata,
    alpha_mode: AlphaMode,
}

impl<O> PlaybackViewportFrame<O> {
    /// Returns the display-ready output value.
    #[must_use]
    pub const fn output(&self) -> &O {
        &self.output
    }

    /// Releases the display-ready output value.
    #[must_use]
    pub fn into_output(self) -> O {
        self.output
    }

    /// Returns the exact output timeline coordinate.
    #[must_use]
    pub const fn timestamp(&self) -> RationalTime {
        self.timestamp
    }

    /// Returns the exact output duration.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.duration
    }

    /// Returns decoded input provenance retained through graph and display execution.
    #[must_use]
    pub fn decoded(&self) -> &[DecodedFrameMetadata] {
        &self.decoded
    }

    /// Returns complete monitoring color history.
    #[must_use]
    pub const fn color_metadata(&self) -> &ViewportColorMetadata {
        &self.color_metadata
    }

    /// Returns alpha association at the viewport boundary.
    #[must_use]
    pub const fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }
}

/// Exact foreground frame seam executed by a playback-priority worker.
pub trait PlaybackFrameEvaluator<O>: Send + Sync {
    /// Evaluates, validates, and display-transforms one exact graph frame.
    fn evaluate_frame(&self, frame: RationalTime) -> Result<PlaybackViewportFrame<O>>;
}

/// Shared graph, cache, and display implementation of foreground playback evaluation.
pub struct GraphPlaybackFrameEvaluator<T, N, V, D> {
    snapshot: Arc<GraphEvaluationSnapshot<T, N>>,
    cache: Arc<OwnedHostFrameMemoryCache<PlaybackSceneFrame<V>>>,
    output: GraphEndpoint,
    region: PixelBounds,
    display: D,
}

impl<T, N, V, D> GraphPlaybackFrameEvaluator<T, N, V, D>
where
    D: PlaybackDisplayTransform<V>,
{
    /// Binds immutable graph state, complete cache identity, and one display branch.
    pub fn new(
        snapshot: Arc<GraphEvaluationSnapshot<T, N>>,
        cache: Arc<FrameMemoryCache<PlaybackSceneFrame<V>>>,
        identity: PlaybackCacheIdentity,
        output: GraphEndpoint,
        region: PixelBounds,
        display: D,
    ) -> Result<Self> {
        if identity.color_pipeline != *display.scene_pipeline() {
            return Err(orchestration_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "playback cache and display scene color identities must match",
                "create_graph_frame_evaluator",
            ));
        }
        Ok(Self {
            snapshot,
            cache: Arc::new(identity.into_host_cache(cache)),
            output,
            region,
            display,
        })
    }
}

impl<T, N, V, D> PlaybackFrameEvaluator<D::Output> for GraphPlaybackFrameEvaluator<T, N, V, D>
where
    T: Send + Sync + 'static,
    N: EvaluateNode<PlaybackSceneFrame<V>> + IntrospectNode + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
    D: PlaybackDisplayTransform<V> + Send + Sync + 'static,
{
    fn evaluate_frame(&self, frame: RationalTime) -> Result<PlaybackViewportFrame<D::Output>> {
        let cache = ValidatedPlaybackCache {
            inner: self.cache.as_ref(),
            scene_pipeline: self.display.scene_pipeline(),
            scene_alpha_mode: self.display.scene_alpha_mode(),
        };
        let evaluation = self.snapshot.evaluate_with_cache(
            EvaluationRequest::new(self.output, frame, self.region),
            &cache,
        )?;
        let scene = evaluation.value();
        validate_scene_result(
            scene,
            frame,
            self.display.scene_pipeline(),
            self.display.scene_alpha_mode(),
        )?;
        let output = self.display.transform(scene.value())?;
        Ok(PlaybackViewportFrame {
            output,
            timestamp: scene.timestamp(),
            duration: scene.duration(),
            decoded: scene.decoded().to_vec(),
            color_metadata: self.display.viewport_color_metadata().clone(),
            alpha_mode: self.display.output_alpha_mode(),
        })
    }
}

struct ValidatedPlaybackCache<'a, V> {
    inner: &'a OwnedHostFrameMemoryCache<PlaybackSceneFrame<V>>,
    scene_pipeline: &'a ColorPipelineMetadata,
    scene_alpha_mode: AlphaMode,
}

impl<V: Clone> EvaluationValueCache<PlaybackSceneFrame<V>> for ValidatedPlaybackCache<'_, V> {
    fn get(
        &self,
        kind: EvaluationCacheEntryKind,
        identity: EvaluationCacheIdentity,
    ) -> Option<PlaybackSceneFrame<V>> {
        self.inner.get(kind, identity).filter(|scene| {
            scene.timestamp() == identity.evaluation_key().frame()
                && scene.color_pipeline() == self.scene_pipeline
                && scene.alpha_mode() == self.scene_alpha_mode
        })
    }

    fn insert(
        &self,
        kind: EvaluationCacheEntryKind,
        identity: EvaluationCacheIdentity,
        value: PlaybackSceneFrame<V>,
    ) {
        if value.timestamp() == identity.evaluation_key().frame()
            && value.color_pipeline() == self.scene_pipeline
            && value.alpha_mode() == self.scene_alpha_mode
        {
            self.inner.insert(kind, identity, value);
        }
    }
}

/// Bounded engine-to-device audio producer paired with its actual presentation clock.
pub struct PlaybackAudioOutput {
    producer: OutputProducer,
    clock: Arc<AudioMasterClock>,
}

impl PlaybackAudioOutput {
    /// Binds the producer and device clock created for one output buffer.
    #[must_use]
    pub fn new(producer: OutputProducer, clock: Arc<AudioMasterClock>) -> Self {
        Self { producer, clock }
    }

    /// Returns the actual device presentation clock.
    #[must_use]
    pub const fn clock(&self) -> &Arc<AudioMasterClock> {
        &self.clock
    }

    fn queue(&mut self, samples: &[f32]) -> Result<OutputWrite> {
        self.producer
            .push_interleaved(samples)
            .map_err(audio_output_error)
    }
}

/// Nonblocking playback observation after one poll.
#[derive(Debug)]
#[non_exhaustive]
pub enum PlaybackPoll {
    /// No frame is being evaluated or retained for presentation.
    Idle,
    /// One exact frame is executing on a playback-priority worker.
    Rendering {
        /// Caller-assigned frame job identity.
        job_id: JobId,
        /// Exact requested timeline coordinate.
        frame: RationalTime,
    },
    /// A complete frame is retained until the shared clock reaches it.
    Waiting {
        /// Exact frame presentation coordinate.
        frame: RationalTime,
        /// Current shared clock position.
        clock: RationalTime,
    },
    /// A due frame remains owned by playback because the viewport queue is full.
    ViewportBackpressured {
        /// Exact retained frame coordinate.
        frame: RationalTime,
    },
    /// A due frame entered the viewport queue.
    Presented {
        /// Exact published frame coordinate.
        frame: RationalTime,
    },
    /// One frame failed without stopping audio, clock, or later requests.
    Failed {
        /// Caller-assigned frame job identity.
        job_id: JobId,
        /// Exact failed frame coordinate.
        frame: RationalTime,
        /// Classified degradation detail.
        error: Error,
    },
}

struct PendingPlaybackFrame<O> {
    frame: RationalTime,
    handle: JobTaskHandle<PlaybackViewportFrame<O>>,
}

/// Playback-domain owner for one in-flight graph frame, audio admission, clock, and viewport handoff.
///
/// Submission and polling never wait. Exactly one frame may be evaluating or retained, so transport
/// policy remains outside this checkpoint and every saturated sink returns ownership explicitly.
pub struct PlaybackOrchestrator<O> {
    pool: Weak<BoundedWorkerPool>,
    evaluator: Arc<dyn PlaybackFrameEvaluator<O>>,
    viewport: HandoffSender<PlaybackViewportFrame<O>>,
    audio: PlaybackAudioOutput,
    clock: PlaybackClock,
    pending: Option<PendingPlaybackFrame<O>>,
    ready: Option<PlaybackViewportFrame<O>>,
}

impl<O: Send + 'static> PlaybackOrchestrator<O> {
    /// Creates an audio-master playback owner at one exact timeline anchor.
    pub fn audio_master(
        pool: &Arc<BoundedWorkerPool>,
        evaluator: Arc<dyn PlaybackFrameEvaluator<O>>,
        viewport: HandoffSender<PlaybackViewportFrame<O>>,
        audio: PlaybackAudioOutput,
        timeline_anchor: RationalTime,
    ) -> Result<Self> {
        let clock = PlaybackClock::audio_master(timeline_anchor, audio.clock().clone())?;
        Ok(Self {
            pool: Arc::downgrade(pool),
            evaluator,
            viewport,
            audio,
            clock,
            pending: None,
            ready: None,
        })
    }

    /// Publishes one exact foreground frame request without waiting for worker capacity or output.
    pub fn submit_frame(&mut self, job_id: JobId, frame: RationalTime) -> Result<()> {
        ExecutionDomain::Playback.require_current()?;
        if self.pending.is_some() || self.ready.is_some() {
            return Err(orchestration_error(
                ErrorCategory::Conflict,
                Recoverability::Retryable,
                "playback already owns an in-flight or unpresented frame",
                "submit_frame",
            ));
        }
        let pool = self.pool.upgrade().ok_or_else(|| {
            orchestration_error(
                ErrorCategory::Unavailable,
                Recoverability::Degraded,
                "playback worker pool is no longer available",
                "submit_frame",
            )
        })?;
        let evaluator = self.evaluator.clone();
        let progress = JobProgress::with_total(1)?;
        let handle = pool.submit(
            ScheduledJob::new(
                job_id,
                JobKind::Frame,
                JobPriority::Playback,
                move |context: superi_concurrency::jobs::JobExecutionContext| {
                    context.check("evaluate_playback_frame")?;
                    let output = evaluator.evaluate_frame(frame)?;
                    context.progress().advance_to(1)?;
                    Ok(output)
                },
            ),
            JobControl::new(),
            progress,
        )?;
        self.pending = Some(PendingPlaybackFrame { frame, handle });
        Ok(())
    }

    /// Polls worker completion, clock eligibility, and viewport admission without waiting.
    pub fn poll_at(&mut self, now: Instant) -> Result<PlaybackPoll> {
        ExecutionDomain::Playback.require_current()?;
        if self.ready.is_some() {
            return self.present_ready(now);
        }
        let Some(pending) = self.pending.as_ref() else {
            return Ok(PlaybackPoll::Idle);
        };
        let job_id = pending.handle.job_id();
        let frame = pending.frame;
        let Some(execution) = pending.handle.try_completion()? else {
            return Ok(PlaybackPoll::Rendering { job_id, frame });
        };
        self.pending = None;
        match execution.into_completion().into_outcome() {
            JobOutcome::Succeeded(output) => {
                if output.timestamp() != frame {
                    return Ok(PlaybackPoll::Failed {
                        job_id,
                        frame,
                        error: orchestration_error(
                            ErrorCategory::CorruptData,
                            Recoverability::Degraded,
                            "playback evaluator returned a different frame timestamp",
                            "poll_frame",
                        ),
                    });
                }
                self.ready = Some(output);
                self.present_ready(now)
            }
            JobOutcome::Failed(error) => Ok(PlaybackPoll::Failed {
                job_id,
                frame,
                error,
            }),
            JobOutcome::Cancelled => Ok(PlaybackPoll::Failed {
                job_id,
                frame,
                error: orchestration_error(
                    ErrorCategory::Cancelled,
                    Recoverability::Degraded,
                    "playback frame evaluation was cancelled",
                    "poll_frame",
                ),
            }),
            JobOutcome::DeadlineExceeded => Ok(PlaybackPoll::Failed {
                job_id,
                frame,
                error: orchestration_error(
                    ErrorCategory::Timeout,
                    Recoverability::Retryable,
                    "playback frame evaluation exceeded its deadline",
                    "poll_frame",
                ),
            }),
            JobOutcome::DependencyFailed(_) => Ok(PlaybackPoll::Failed {
                job_id,
                frame,
                error: orchestration_error(
                    ErrorCategory::Unavailable,
                    Recoverability::Degraded,
                    "playback frame dependency did not complete",
                    "poll_frame",
                ),
            }),
            _ => Ok(PlaybackPoll::Failed {
                job_id,
                frame,
                error: orchestration_error(
                    ErrorCategory::Internal,
                    Recoverability::Degraded,
                    "playback frame returned an unknown worker outcome",
                    "poll_frame",
                ),
            }),
        }
    }

    /// Atomically admits complete interleaved audio frames or returns the caller's borrowed input.
    pub fn queue_audio(&mut self, samples: &[f32]) -> Result<OutputWrite> {
        ExecutionDomain::Playback.require_current()?;
        self.audio.queue(samples)
    }

    /// Returns the active shared clock source.
    #[must_use]
    pub const fn clock_mode(&self) -> PlaybackClockMode {
        self.clock.mode()
    }

    /// Reads the shared timeline clock at an explicit process-local instant.
    pub fn clock_position_at(&self, now: Instant) -> Result<RationalTime> {
        ExecutionDomain::Playback.require_current()?;
        self.clock.position_at(now)
    }

    /// Falls back from a lost audio clock to monotonic time without a timeline jump.
    pub fn fallback_to_playback_at(&mut self, now: Instant) -> Result<RationalTime> {
        ExecutionDomain::Playback.require_current()?;
        self.clock.switch_to_playback_at(now)
    }

    /// Restores the paired audio device clock without a timeline jump.
    pub fn recover_audio_master_at(&mut self, now: Instant) -> Result<RationalTime> {
        ExecutionDomain::Playback.require_current()?;
        self.clock
            .switch_to_audio_master_at(self.audio.clock().clone(), now)
    }

    fn present_ready(&mut self, now: Instant) -> Result<PlaybackPoll> {
        let frame = self
            .ready
            .take()
            .expect("present_ready is called only with one retained frame");
        let timestamp = frame.timestamp();
        let clock = match self.clock.position_at(now) {
            Ok(clock) => clock,
            Err(error) => {
                self.ready = Some(frame);
                return Err(error);
            }
        };
        if timestamp > clock {
            self.ready = Some(frame);
            return Ok(PlaybackPoll::Waiting {
                frame: timestamp,
                clock,
            });
        }
        match self.viewport.try_send(frame) {
            Ok(()) => Ok(PlaybackPoll::Presented { frame: timestamp }),
            Err(backpressure) => {
                self.ready = Some(backpressure.into_item());
                Ok(PlaybackPoll::ViewportBackpressured { frame: timestamp })
            }
        }
    }
}

fn pipeline_extends(decoded: &ColorPipelineMetadata, scene: &ColorPipelineMetadata) -> bool {
    scene.source_tags() == decoded.source_tags()
        && scene.stages().starts_with(decoded.stages())
        && scene.display_space().is_none()
        && scene.delivery_space().is_none()
}

fn validate_scene_result<V>(
    scene: &PlaybackSceneFrame<V>,
    requested: RationalTime,
    expected_color: &ColorPipelineMetadata,
    expected_alpha: AlphaMode,
) -> Result<()> {
    if scene.timestamp() != requested
        || scene.color_pipeline() != expected_color
        || scene.alpha_mode() != expected_alpha
    {
        return Err(orchestration_error(
            ErrorCategory::CorruptData,
            Recoverability::Degraded,
            "graph playback result does not match its exact time, scene color, and alpha identity",
            "validate_scene_result",
        ));
    }
    Ok(())
}

fn audio_output_error(source: OutputBufferError) -> Error {
    let (category, recoverability, message) = match source {
        OutputBufferError::InsufficientCapacity { .. } => (
            ErrorCategory::ResourceExhausted,
            Recoverability::Retryable,
            "playback audio output buffer is full",
        ),
        _ => (
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "playback audio samples do not satisfy the output buffer contract",
        ),
    };
    orchestration_error(category, recoverability, message, "queue_audio").with_context(
        ErrorContext::new(ORCHESTRATION_COMPONENT, "audio_buffer")
            .with_field("source", source.to_string()),
    )
}

fn orchestration_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(ORCHESTRATION_COMPONENT, operation))
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
