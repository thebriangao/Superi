//! Production timing owner for desktop playback control.
//!
//! This runtime executes the existing bounded dispatcher bridge against the authoritative exact
//! transport. Its viewport values contain timing identity only. Missing decoded pixels and
//! device-backed audio remain explicit transport degradation until their dedicated owners attach.

use std::sync::Arc;
use std::time::Instant;

use superi_audio::playback::{
    create_output_buffer, OutputBufferConfig, OutputConsumer, OutputRenderReport,
};
use superi_cache::prefetch::PlaybackPrefetchConfig;
use superi_concurrency::backpressure::{
    bounded_handoff, BackpressureConfig, HandoffReceiver, PipelineRoute, PipelineStage,
};
use superi_concurrency::jobs::BoundedWorkerPool;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::AlphaMode;
use superi_core::time::{Duration, RationalTime, TimeRange, Timebase};

use crate::dispatcher::{PlaybackCommandExecution, PlaybackCommandExecutor};
use crate::playback::{
    PlaybackAudioOutput, PlaybackFrameEvaluator, PlaybackOrchestrator, PlaybackPrefetchEvaluator,
    PlaybackPrefetcher, PlaybackViewportFrame,
};
use crate::render::ViewportColorMetadata;
use crate::transport::{
    DroppedFramePolicy, PlaybackTransport, PlaybackTransportConfig, PlaybackTransportSnapshot,
    PlaybackTransportUpdate, TransportDegradation, TransportDegradationCode,
};

const COMPONENT: &str = "superi-engine.playback-runtime";
const AUDIO_CHANNELS: u16 = 2;
const AUDIO_SAMPLE_RATE: u32 = 48_000;
const AUDIO_CAPACITY_FRAMES: usize = 4_096;
const VIEWPORT_CAPACITY: usize = 2;

#[derive(Clone)]
struct TimingFrameEvaluator {
    color: ViewportColorMetadata,
}

impl PlaybackFrameEvaluator<RationalTime> for TimingFrameEvaluator {
    fn evaluate_frame(&self, frame: RationalTime) -> Result<PlaybackViewportFrame<RationalTime>> {
        PlaybackViewportFrame::new(
            frame,
            frame,
            Duration::new(1, frame.timebase())?,
            Vec::new(),
            self.color.clone(),
            AlphaMode::Premultiplied,
        )
    }
}

struct TimingPrefetchEvaluator;

impl PlaybackPrefetchEvaluator for TimingPrefetchEvaluator {
    fn prefetch(&self, _frame: RationalTime) -> Result<()> {
        Ok(())
    }
}

/// One nonblocking playback-domain tick result.
pub struct PlaybackRuntimeTick {
    command: Option<PlaybackCommandExecution>,
    transport: PlaybackTransportUpdate,
    latest_viewport_frame: Option<RationalTime>,
    audio: OutputRenderReport,
}

impl PlaybackRuntimeTick {
    /// Returns the accepted bridge command completed by this tick, if any.
    #[must_use]
    pub const fn command(&self) -> Option<&PlaybackCommandExecution> {
        self.command.as_ref()
    }

    /// Returns the complete exact transport update.
    #[must_use]
    pub const fn transport(&self) -> &PlaybackTransportUpdate {
        &self.transport
    }

    /// Returns the latest timing-only viewport frame drained by this tick.
    #[must_use]
    pub const fn latest_viewport_frame(&self) -> Option<RationalTime> {
        self.latest_viewport_frame
    }

    /// Returns the callback-shaped silence render that acknowledges audio discontinuities.
    #[must_use]
    pub const fn audio(&self) -> OutputRenderReport {
        self.audio
    }
}

/// Playback-domain composition for the real dispatcher bridge and exact transport owner.
pub struct PlaybackControlRuntime {
    transport: PlaybackTransport<RationalTime>,
    executor: PlaybackCommandExecutor,
    viewport: HandoffReceiver<PlaybackViewportFrame<RationalTime>>,
    audio_consumer: OutputConsumer,
}

impl PlaybackControlRuntime {
    /// Creates one timing runtime over a caller-owned bounded worker pool.
    pub fn new(
        pool: &Arc<BoundedWorkerPool>,
        executor: PlaybackCommandExecutor,
        bounds: TimeRange,
        job_namespace: u64,
        now: Instant,
    ) -> Result<Self> {
        ExecutionDomain::Playback.require_current()?;
        if job_namespace == 0 {
            return Err(runtime_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "playback runtime job namespace must be nonzero",
                "create",
            ));
        }
        if bounds.is_empty() {
            return Err(runtime_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "playback runtime bounds must be nonempty",
                "create",
            ));
        }

        let (producer, audio_consumer, _telemetry) = create_output_buffer(OutputBufferConfig {
            channels: AUDIO_CHANNELS,
            sample_rate: AUDIO_SAMPLE_RATE,
            capacity_frames: AUDIO_CAPACITY_FRAMES,
            initial_sample: 0,
        })
        .map_err(|error| {
            runtime_error(
                ErrorCategory::Unavailable,
                Recoverability::Degraded,
                "playback timing audio buffer could not be created",
                "create_audio_buffer",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "audio_buffer")
                    .with_field("source", error.to_string()),
            )
        })?;
        let route = PipelineRoute::new(PipelineStage::Graph, PipelineStage::Viewport)?;
        let (viewport_sender, viewport) =
            bounded_handoff(BackpressureConfig::new(route, VIEWPORT_CAPACITY)?);
        let audio = PlaybackAudioOutput::new(producer, audio_consumer.clock().clone());
        let mut foreground = PlaybackOrchestrator::audio_master(
            pool,
            Arc::new(TimingFrameEvaluator {
                color: ViewportColorMetadata::standard_srgb_monitoring()?,
            }),
            viewport_sender,
            audio,
            RationalTime::zero(Timebase::integer(AUDIO_SAMPLE_RATE)?),
        )?;
        foreground.fallback_to_playback_at(now)?;
        let prefetcher = PlaybackPrefetcher::new(pool, Arc::new(TimingPrefetchEvaluator));
        let baseline =
            TransportDegradation::with(TransportDegradationCode::ViewportOutputUnavailable)
                .and(TransportDegradationCode::AudioOutputUnavailable);
        let config = PlaybackTransportConfig::new(
            bounds,
            bounds.start(),
            PlaybackPrefetchConfig::new(2, 2, 1)?,
            DroppedFramePolicy::DropLate { max_consecutive: 2 },
        )?
        .with_baseline_degradation(baseline);
        let transport = PlaybackTransport::new(foreground, prefetcher, config, job_namespace, now)?;
        Ok(Self {
            transport,
            executor,
            viewport,
            audio_consumer,
        })
    }

    /// Executes one bridge request, advances exact timing, and drains bounded output ownership.
    pub fn tick(&mut self, now: Instant) -> Result<PlaybackRuntimeTick> {
        ExecutionDomain::Playback.require_current()?;
        let mut silence = [0.0_f32; 2];
        let audio = self
            .audio_consumer
            .render_f32(&mut silence)
            .map_err(|error| {
                runtime_error(
                    ErrorCategory::Unavailable,
                    Recoverability::Degraded,
                    "playback timing audio callback failed",
                    "tick_audio",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "audio_callback")
                        .with_field("source", error.to_string()),
                )
            })?;
        let command = self.executor.execute_next(&mut self.transport, now)?;
        let transport = self.transport.update_at(now)?;
        let mut latest_viewport_frame = None;
        while let Some(frame) = self.viewport.try_receive() {
            latest_viewport_frame = Some(frame.timestamp());
        }
        Ok(PlaybackRuntimeTick {
            command,
            transport,
            latest_viewport_frame,
            audio,
        })
    }

    /// Replaces authored bounds on the playback domain in one exact discontinuity.
    pub fn reconfigure_bounds(&mut self, bounds: TimeRange, now: Instant) -> Result<()> {
        self.transport.reconfigure_bounds(bounds, now)
    }

    /// Returns a complete immutable transport observation.
    #[must_use]
    pub fn snapshot(&self) -> PlaybackTransportSnapshot {
        self.transport.snapshot()
    }
}

fn runtime_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
