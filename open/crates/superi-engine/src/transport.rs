//! Exact interactive transport policy over the shared playback owners.
//!
//! This module owns control-state discontinuities, exact frame cadence, looping, prediction
//! replanning, and bounded late-frame skipping. Frame evaluation remains in [`crate::playback`],
//! cache identity remains in `superi-cache`, and dropping changes scheduling only. Render and export
//! orchestration intentionally remain outside this checkpoint.

use std::time::Instant;

use superi_audio::playback::{OutputDiscardStatus, OutputWrite};
use superi_cache::prefetch::{PlaybackPrefetchConfig, PlaybackPrefetchInput, PlaybackPrefetchPlan};
use superi_concurrency::jobs::JobTerminalState;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::JobId;
use superi_core::time::{Duration, RationalTime, TimeRange};
use superi_timeline::retime::PlaybackRate;

use crate::playback::{
    PlaybackOrchestrator, PlaybackPoll, PlaybackPrefetchCompletion, PlaybackPrefetcher,
};

const COMPONENT: &str = "superi-engine.transport";

/// Stable interactive playback state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum PlaybackTransportMode {
    /// One exact frame is retained without clock advancement.
    Paused,
    /// Frames advance from the shared playback clock.
    Playing,
    /// User-authored scrub observations supersede one another.
    Scrubbing,
    /// The active nonlooping range reached its terminal frame.
    Ended,
}

/// Signed traversal direction independent of rate magnitude.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaybackDirection {
    /// Increasing exact frame coordinates.
    Forward,
    /// Decreasing exact frame coordinates.
    Reverse,
}

impl PlaybackDirection {
    const fn sign(self) -> i64 {
        match self {
            Self::Forward => 1,
            Self::Reverse => -1,
        }
    }
}

/// Explicit scheduling policy for ordinary late playback frames.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DroppedFramePolicy {
    /// Evaluate and present every exact frame regardless of lateness.
    PreserveEveryFrame,
    /// Skip only ordinary playback frames, with a forced-presentation ceiling.
    DropLate {
        /// Maximum ordinary frames skipped before one visible frame is forced.
        max_consecutive: u32,
    },
}

/// Cumulative scheduling-only drop evidence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct DropStatistics {
    total_dropped: u64,
    consecutive_dropped: u32,
    forced_presentations: u64,
}

impl DropStatistics {
    /// Returns every ordinary frame skipped by policy.
    #[must_use]
    pub const fn total_dropped(self) -> u64 {
        self.total_dropped
    }

    /// Returns skips since the most recent visible presentation.
    #[must_use]
    pub const fn consecutive_dropped(self) -> u32 {
        self.consecutive_dropped
    }

    /// Returns presentations forced because the consecutive ceiling was reached.
    #[must_use]
    pub const fn forced_presentations(self) -> u64 {
        self.forced_presentations
    }
}

/// Current audio behavior for the active transport state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TransportAudioState {
    /// Forward one-times playback may admit synchronized device samples.
    Synchronized,
    /// The callback has not yet acknowledged a queued-audio discontinuity.
    DiscardPending(OutputDiscardStatus),
    /// Paused, scrubbing, or ended transport intentionally admits no device samples.
    MutedInactive(PlaybackTransportMode),
    /// Untimestamped samples are muted because this rate cannot be paced correctly.
    MutedUnsupportedRate(PlaybackRate),
}

/// Stable degraded condition exposed by one transport snapshot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransportDegradationCode {
    /// A foreground frame failed and awaits explicit recovery.
    FrameFailure,
    /// A complete due frame is retained behind bounded viewport backpressure.
    ViewportBackpressure,
    /// Replaceable prediction work failed while foreground evaluation remained authoritative.
    PrefetchFailure,
    /// Queued pre-discontinuity audio has not yet been discarded by the callback.
    AudioDiscardPending,
    /// Current speed or direction cannot use the untimestamped output admission seam.
    AudioRateUnsupported,
}

impl TransportDegradationCode {
    const fn bit(self) -> u8 {
        match self {
            Self::FrameFailure => 1 << 0,
            Self::ViewportBackpressure => 1 << 1,
            Self::PrefetchFailure => 1 << 2,
            Self::AudioDiscardPending => 1 << 3,
            Self::AudioRateUnsupported => 1 << 4,
        }
    }
}

/// Compact set of simultaneously active degraded conditions.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TransportDegradation(u8);

impl TransportDegradation {
    /// Reports whether one stable condition is active.
    #[must_use]
    pub const fn contains(self, code: TransportDegradationCode) -> bool {
        self.0 & code.bit() != 0
    }

    /// Reports whether transport currently has no degraded condition.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Validated immutable setup for one transport owner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaybackTransportConfig {
    bounds: TimeRange,
    initial_playhead: RationalTime,
    prefetch: PlaybackPrefetchConfig,
    drop_policy: DroppedFramePolicy,
}

impl PlaybackTransportConfig {
    /// Creates one exact bounded frame-coordinate transport configuration.
    pub fn new(
        bounds: TimeRange,
        initial_playhead: RationalTime,
        prefetch: PlaybackPrefetchConfig,
        drop_policy: DroppedFramePolicy,
    ) -> Result<Self> {
        if bounds.is_empty() {
            return Err(invalid(
                "configure",
                "playback transport bounds must be nonempty",
            ));
        }
        if initial_playhead.timebase() != bounds.timebase() || !bounds.contains(initial_playhead)? {
            return Err(invalid(
                "configure",
                "initial playback playhead must lie inside the exact transport bounds",
            ));
        }
        if matches!(
            drop_policy,
            DroppedFramePolicy::DropLate { max_consecutive: 0 }
        ) {
            return Err(invalid(
                "configure",
                "late-frame policy requires a positive consecutive-drop ceiling",
            ));
        }
        Ok(Self {
            bounds,
            initial_playhead,
            prefetch,
            drop_policy,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FrameProtection {
    Ordinary,
    Discontinuity,
    LoopBoundary,
}

impl FrameProtection {
    const fn is_droppable(self) -> bool {
        matches!(self, Self::Ordinary)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ScheduledFrame {
    frame: RationalTime,
    due_clock: RationalTime,
    protection: FrameProtection,
}

/// Immutable complete transport state after an operation or update.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PlaybackTransportSnapshot {
    mode: PlaybackTransportMode,
    playhead: RationalTime,
    scheduled: Option<ScheduledFrame>,
    rate: PlaybackRate,
    loop_range: Option<TimeRange>,
    epoch: u64,
    drop_statistics: DropStatistics,
    audio_state: TransportAudioState,
    audio_discard_status: OutputDiscardStatus,
    degradation: TransportDegradation,
}

impl PlaybackTransportSnapshot {
    /// Returns the active interaction mode.
    #[must_use]
    pub const fn mode(self) -> PlaybackTransportMode {
        self.mode
    }

    /// Returns the last exact requested or presented frame coordinate.
    #[must_use]
    pub const fn playhead(self) -> RationalTime {
        self.playhead
    }

    /// Returns the exact foreground frame currently owned by playback.
    #[must_use]
    pub const fn scheduled_frame(self) -> Option<RationalTime> {
        match self.scheduled {
            Some(scheduled) => Some(scheduled.frame),
            None => None,
        }
    }

    /// Returns the distinct shared-clock deadline for the owned frame.
    #[must_use]
    pub const fn scheduled_due_clock(self) -> Option<RationalTime> {
        match self.scheduled {
            Some(scheduled) => Some(scheduled.due_clock),
            None => None,
        }
    }

    /// Returns the exact reduced signed rate.
    #[must_use]
    pub const fn rate(self) -> PlaybackRate {
        self.rate
    }

    /// Returns the traversal direction derived from the signed rate.
    #[must_use]
    pub const fn direction(self) -> PlaybackDirection {
        if self.rate.is_reverse() {
            PlaybackDirection::Reverse
        } else {
            PlaybackDirection::Forward
        }
    }

    /// Returns the active half-open loop range.
    #[must_use]
    pub const fn loop_range(self) -> Option<TimeRange> {
        self.loop_range
    }

    /// Returns the monotonically increasing discontinuity generation.
    #[must_use]
    pub const fn epoch(self) -> u64 {
        self.epoch
    }

    /// Returns cumulative drop-policy evidence.
    #[must_use]
    pub const fn drop_statistics(self) -> DropStatistics {
        self.drop_statistics
    }

    /// Returns synchronized, discard-pending, or explicitly muted audio behavior.
    #[must_use]
    pub const fn audio_state(self) -> TransportAudioState {
        self.audio_state
    }

    /// Returns callback acknowledgement independently of current rate support.
    #[must_use]
    pub const fn audio_discard_status(self) -> OutputDiscardStatus {
        self.audio_discard_status
    }

    /// Returns all current degraded conditions.
    #[must_use]
    pub const fn degradation(self) -> TransportDegradation {
        self.degradation
    }
}

/// One nonblocking foreground and prediction observation.
pub struct PlaybackTransportUpdate {
    playback: PlaybackPoll,
    prefetch: Vec<PlaybackPrefetchCompletion>,
    snapshot: PlaybackTransportSnapshot,
}

impl PlaybackTransportUpdate {
    /// Returns the exact foreground worker, clock, or viewport observation.
    #[must_use]
    pub const fn playback(&self) -> &PlaybackPoll {
        &self.playback
    }

    /// Returns every prediction generation that became terminal during this update.
    #[must_use]
    pub fn prefetch_completions(&self) -> &[PlaybackPrefetchCompletion] {
        &self.prefetch
    }

    /// Returns the immutable state after processing this update.
    #[must_use]
    pub const fn snapshot(&self) -> PlaybackTransportSnapshot {
        self.snapshot
    }
}

/// Playback-domain owner for exact interactive transport policy.
pub struct PlaybackTransport<O> {
    foreground: PlaybackOrchestrator<O>,
    prefetcher: PlaybackPrefetcher,
    prefetch_config: PlaybackPrefetchConfig,
    bounds: TimeRange,
    loop_range: Option<TimeRange>,
    mode: PlaybackTransportMode,
    playhead: RationalTime,
    rate: PlaybackRate,
    drop_policy: DroppedFramePolicy,
    drop_statistics: DropStatistics,
    forced_noted: bool,
    clock_anchor: RationalTime,
    frame_anchor: RationalTime,
    cadence_offset: u64,
    scheduled: Option<ScheduledFrame>,
    epoch: u64,
    job_namespace: u64,
    next_job_counter: u64,
    frame_failed: bool,
    viewport_backpressured: bool,
    prefetch_failed: bool,
    latest_prefetch_generation: Option<u64>,
}

impl<O: Send + 'static> PlaybackTransport<O> {
    /// Binds foreground, prediction, exact bounds, and deterministic job identity ownership.
    pub fn new(
        foreground: PlaybackOrchestrator<O>,
        prefetcher: PlaybackPrefetcher,
        config: PlaybackTransportConfig,
        job_namespace: u64,
        now: Instant,
    ) -> Result<Self> {
        ExecutionDomain::Playback.require_current()?;
        let clock_anchor = foreground.clock_position_at(now)?;
        Ok(Self {
            foreground,
            prefetcher,
            prefetch_config: config.prefetch,
            bounds: config.bounds,
            loop_range: None,
            mode: PlaybackTransportMode::Paused,
            playhead: config.initial_playhead,
            rate: PlaybackRate::NORMAL,
            drop_policy: config.drop_policy,
            drop_statistics: DropStatistics::default(),
            forced_noted: false,
            clock_anchor,
            frame_anchor: config.initial_playhead,
            cadence_offset: 0,
            scheduled: None,
            epoch: 0,
            job_namespace,
            next_job_counter: 0,
            frame_failed: false,
            viewport_backpressured: false,
            prefetch_failed: false,
            latest_prefetch_generation: None,
        })
    }

    /// Returns a complete immutable observation without polling worker state.
    #[must_use]
    pub fn snapshot(&self) -> PlaybackTransportSnapshot {
        let audio_discard_status = self.foreground.audio_discard_status();
        let audio_state = self.audio_state(audio_discard_status);
        PlaybackTransportSnapshot {
            mode: self.mode,
            playhead: self.playhead,
            scheduled: self.scheduled,
            rate: self.rate,
            loop_range: self.loop_range,
            epoch: self.epoch,
            drop_statistics: self.drop_statistics,
            audio_state,
            audio_discard_status,
            degradation: self.degradation(audio_state, audio_discard_status),
        }
    }

    /// Starts clock-driven playback from the exact current frame.
    pub fn play(&mut self, now: Instant) -> Result<()> {
        self.require_not_scrubbing("play")?;
        if self.mode == PlaybackTransportMode::Playing {
            return Ok(());
        }
        self.mode = PlaybackTransportMode::Playing;
        self.discontinuity_to(self.playhead, FrameProtection::Discontinuity, now)
    }

    /// Stops clock-driven playback at the exact frame intended at this instant.
    pub fn pause(&mut self, now: Instant) -> Result<()> {
        self.require_not_scrubbing("pause")?;
        if self.mode == PlaybackTransportMode::Paused {
            return Ok(());
        }
        let target = self.intended_frame_at(now)?;
        self.mode = PlaybackTransportMode::Paused;
        self.discontinuity_to(target, FrameProtection::Discontinuity, now)
    }

    /// Seeks immediately to one exact frame while preserving play or pause state.
    pub fn seek(&mut self, target: RationalTime, now: Instant) -> Result<()> {
        self.require_not_scrubbing("seek")?;
        self.validate_target(target)?;
        if self.mode == PlaybackTransportMode::Ended {
            self.mode = PlaybackTransportMode::Paused;
        }
        self.discontinuity_to(target, FrameProtection::Discontinuity, now)
    }

    /// Enters superseding scrub mode and remembers no implicit resume policy.
    pub fn begin_scrub(&mut self, now: Instant) -> Result<()> {
        ExecutionDomain::Playback.require_current()?;
        if self.mode == PlaybackTransportMode::Scrubbing {
            return Err(conflict(
                "begin_scrub",
                "playback transport is already scrubbing",
            ));
        }
        let target = if self.mode == PlaybackTransportMode::Playing {
            self.intended_frame_at(now)?
        } else {
            self.playhead
        };
        self.mode = PlaybackTransportMode::Scrubbing;
        self.discontinuity_to(target, FrameProtection::Discontinuity, now)
    }

    /// Supersedes prior work with one exact scrub coordinate.
    pub fn scrub_to(&mut self, target: RationalTime, now: Instant) -> Result<()> {
        ExecutionDomain::Playback.require_current()?;
        if self.mode != PlaybackTransportMode::Scrubbing {
            return Err(conflict(
                "scrub",
                "playback transport must enter scrub mode before moving the scrubber",
            ));
        }
        self.discontinuity_to(target, FrameProtection::Discontinuity, now)
    }

    /// Leaves scrub mode paused or resumes from the latest exact scrub coordinate.
    pub fn end_scrub(&mut self, resume: bool, now: Instant) -> Result<()> {
        ExecutionDomain::Playback.require_current()?;
        if self.mode != PlaybackTransportMode::Scrubbing {
            return Err(conflict("end_scrub", "playback transport is not scrubbing"));
        }
        self.mode = if resume {
            PlaybackTransportMode::Playing
        } else {
            PlaybackTransportMode::Paused
        };
        if resume {
            self.discontinuity_to(self.playhead, FrameProtection::Discontinuity, now)?;
        }
        Ok(())
    }

    /// Enables or disables one validated half-open loop without changing frame identity.
    pub fn set_loop(&mut self, loop_range: Option<TimeRange>, now: Instant) -> Result<()> {
        self.require_not_scrubbing("set_loop")?;
        if let Some(range) = loop_range {
            self.validate_loop(range)?;
            if !range.contains(self.playhead)? {
                return Err(invalid(
                    "set_loop",
                    "current playhead must lie inside a newly enabled loop",
                ));
            }
        }
        self.loop_range = loop_range;
        self.discontinuity_to(self.playhead, FrameProtection::Discontinuity, now)
    }

    /// Changes the exact signed speed while preserving the current intended frame.
    pub fn set_rate(&mut self, rate: PlaybackRate, now: Instant) -> Result<()> {
        self.require_not_scrubbing("set_rate")?;
        if rate.is_freeze() {
            return Err(invalid(
                "set_rate",
                "freeze is represented by paused transport rather than a zero playback rate",
            ));
        }
        let target = if self.mode == PlaybackTransportMode::Playing {
            self.intended_frame_at(now)?
        } else {
            self.playhead
        };
        self.rate = rate;
        self.discontinuity_to(target, FrameProtection::Discontinuity, now)
    }

    /// Changes only traversal direction while preserving rate magnitude.
    pub fn set_direction(&mut self, direction: PlaybackDirection, now: Instant) -> Result<()> {
        let magnitude = i64::try_from(self.rate.numerator().unsigned_abs()).map_err(|_| {
            overflow(
                "set_direction",
                "playback rate magnitude exceeds signed range",
            )
        })?;
        let numerator = magnitude
            .checked_mul(direction.sign())
            .ok_or_else(|| overflow("set_direction", "signed playback rate overflowed"))?;
        let rate = PlaybackRate::new(numerator, self.rate.denominator())?;
        self.set_rate(rate, now)
    }

    /// Moves by an exact signed number of frame coordinates and remains paused.
    pub fn step_frames(&mut self, delta: i64, now: Instant) -> Result<()> {
        self.require_not_scrubbing("step_frames")?;
        if delta == 0 {
            return Err(invalid("step_frames", "frame-step delta must be nonzero"));
        }
        let target = self.offset_frame(self.playhead, delta)?;
        self.mode = PlaybackTransportMode::Paused;
        self.discontinuity_to(target, FrameProtection::Discontinuity, now)
    }

    /// Admits synchronized audio only when the current rate can preserve its meaning.
    pub fn queue_audio(&mut self, samples: &[f32]) -> Result<OutputWrite> {
        ExecutionDomain::Playback.require_current()?;
        if self.mode != PlaybackTransportMode::Playing {
            return Err(transport_error(
                ErrorCategory::Conflict,
                Recoverability::Retryable,
                "playback audio is muted unless transport is playing",
                "queue_audio",
            ));
        }
        if self.rate != PlaybackRate::NORMAL {
            return Err(transport_error(
                ErrorCategory::Unavailable,
                Recoverability::Degraded,
                "untimestamped playback audio is muted outside normal forward speed",
                "queue_audio",
            ));
        }
        self.foreground.queue_audio(samples)
    }

    /// Polls prediction, lateness policy, foreground work, clock pacing, and viewport admission.
    pub fn update_at(&mut self, now: Instant) -> Result<PlaybackTransportUpdate> {
        ExecutionDomain::Playback.require_current()?;
        let prefetch = self.prefetcher.poll()?;
        self.observe_prefetch(&prefetch);
        self.apply_drop_policy(now)?;
        let playback = self.foreground.poll_at(now)?;
        self.observe_playback(&playback, now)?;
        Ok(PlaybackTransportUpdate {
            playback,
            prefetch,
            snapshot: self.snapshot(),
        })
    }

    fn discontinuity_to(
        &mut self,
        target: RationalTime,
        protection: FrameProtection,
        now: Instant,
    ) -> Result<()> {
        ExecutionDomain::Playback.require_current()?;
        self.validate_target(target)?;
        self.foreground.cancel_frame()?;
        self.scheduled = None;
        self.prefetcher.cancel()?;
        self.epoch = self.epoch.checked_add(1).ok_or_else(|| {
            overflow(
                "discontinuity",
                "playback transport epoch space is exhausted",
            )
        })?;
        let clock = self.foreground.clock_position_at(now)?;
        self.foreground.reanchor_clock_at(clock, now)?;
        self.clock_anchor = clock;
        self.frame_anchor = target;
        self.cadence_offset = 0;
        self.playhead = target;
        self.drop_statistics.consecutive_dropped = 0;
        self.forced_noted = false;
        self.frame_failed = false;
        self.viewport_backpressured = false;
        self.foreground.request_audio_discard()?;
        self.schedule(target, clock, protection)?;
        self.replan(target)?;
        Ok(())
    }

    fn schedule(
        &mut self,
        frame: RationalTime,
        due_clock: RationalTime,
        protection: FrameProtection,
    ) -> Result<()> {
        let job_id = self.next_job_id()?;
        let presentation_duration = self.presentation_duration_for(frame, due_clock)?;
        self.foreground.submit_frame_at_with_duration(
            job_id,
            frame,
            due_clock,
            presentation_duration,
        )?;
        self.scheduled = Some(ScheduledFrame {
            frame,
            due_clock,
            protection,
        });
        Ok(())
    }

    fn observe_playback(&mut self, poll: &PlaybackPoll, now: Instant) -> Result<()> {
        match poll {
            PlaybackPoll::ViewportBackpressured { .. } => {
                self.viewport_backpressured = true;
            }
            PlaybackPoll::Presented { frame, .. } => {
                let presented = self.scheduled.take().ok_or_else(|| {
                    transport_error(
                        ErrorCategory::Internal,
                        Recoverability::Terminal,
                        "foreground presented a frame not owned by transport",
                        "observe_playback",
                    )
                })?;
                if presented.frame != *frame {
                    return Err(transport_error(
                        ErrorCategory::CorruptData,
                        Recoverability::Degraded,
                        "foreground presentation did not match the transport request",
                        "observe_playback",
                    ));
                }
                self.playhead = *frame;
                self.drop_statistics.consecutive_dropped = 0;
                self.forced_noted = false;
                self.frame_failed = false;
                self.viewport_backpressured = false;
                if self.mode == PlaybackTransportMode::Playing {
                    self.schedule_after_presentation(presented, now)?;
                }
            }
            PlaybackPoll::Failed { .. } => {
                self.scheduled = None;
                self.frame_failed = true;
                if self.mode == PlaybackTransportMode::Playing {
                    self.mode = PlaybackTransportMode::Paused;
                    self.foreground.request_audio_discard()?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn schedule_after_presentation(
        &mut self,
        presented: ScheduledFrame,
        _now: Instant,
    ) -> Result<()> {
        let range = self.active_range();
        let end = range.end_exclusive()?;
        let direction = self.direction();
        let boundary = match direction {
            PlaybackDirection::Forward => RationalTime::new(
                end.value()
                    .checked_sub(1)
                    .ok_or_else(|| overflow("advance", "loop boundary underflowed"))?,
                end.timebase(),
            ),
            PlaybackDirection::Reverse => range.start(),
        };
        if presented.frame == boundary {
            let Some(loop_range) = self.loop_range else {
                self.mode = PlaybackTransportMode::Ended;
                self.foreground.request_audio_discard()?;
                return Ok(());
            };
            let wrapped = match direction {
                PlaybackDirection::Forward => loop_range.start(),
                PlaybackDirection::Reverse => RationalTime::new(
                    loop_range
                        .end_exclusive()?
                        .value()
                        .checked_sub(1)
                        .ok_or_else(|| overflow("loop", "reverse loop boundary underflowed"))?,
                    loop_range.timebase(),
                ),
            };
            self.epoch = self
                .epoch
                .checked_add(1)
                .ok_or_else(|| overflow("loop", "playback transport epoch space is exhausted"))?;
            self.foreground.request_audio_discard()?;
            let boundary_steps = self.directional_distance(self.frame_anchor, presented.frame)?;
            let boundary_steps = u64::try_from(boundary_steps)
                .map_err(|_| overflow("loop", "loop boundary lies behind the cadence anchor"))?;
            self.cadence_offset = self
                .cadence_offset
                .checked_add(boundary_steps)
                .and_then(|step| step.checked_add(1))
                .ok_or_else(|| overflow("loop", "loop cadence step space is exhausted"))?;
            self.frame_anchor = wrapped;
            self.playhead = presented.frame;
            let next_due = self.due_clock_for(wrapped)?;
            self.schedule(wrapped, next_due, FrameProtection::LoopBoundary)?;
            self.replan(wrapped)?;
            return Ok(());
        }

        let next_value = presented
            .frame
            .value()
            .checked_add(direction.sign())
            .ok_or_else(|| overflow("advance", "next playback frame overflowed"))?;
        let next = RationalTime::new(next_value, presented.frame.timebase());
        let protection = if self.loop_range.is_some() && next == boundary {
            FrameProtection::LoopBoundary
        } else {
            FrameProtection::Ordinary
        };
        let due = self.due_clock_for(next)?;
        self.schedule(next, due, protection)?;
        self.replan(next)?;
        Ok(())
    }

    fn apply_drop_policy(&mut self, now: Instant) -> Result<()> {
        if self.mode != PlaybackTransportMode::Playing {
            return Ok(());
        }
        let DroppedFramePolicy::DropLate { max_consecutive } = self.drop_policy else {
            return Ok(());
        };
        let Some(scheduled) = self.scheduled else {
            return Ok(());
        };
        if !scheduled.protection.is_droppable() {
            return Ok(());
        }
        let desired = self.intended_frame_at(now)?;
        let distance = self.directional_distance(scheduled.frame, desired)?;
        if distance <= 0 {
            return Ok(());
        }
        let remaining = max_consecutive.saturating_sub(self.drop_statistics.consecutive_dropped);
        if remaining == 0 {
            if !self.forced_noted {
                self.drop_statistics.forced_presentations =
                    self.drop_statistics.forced_presentations.saturating_add(1);
                self.forced_noted = true;
            }
            return Ok(());
        }
        let skipped = u32::try_from(distance).unwrap_or(u32::MAX).min(remaining);
        let target_value = scheduled
            .frame
            .value()
            .checked_add(i64::from(skipped) * self.direction().sign())
            .ok_or_else(|| overflow("drop_late", "late-frame target overflowed"))?;
        let target = RationalTime::new(target_value, scheduled.frame.timebase());
        self.foreground.cancel_frame()?;
        self.scheduled = None;
        self.drop_statistics.total_dropped = self
            .drop_statistics
            .total_dropped
            .saturating_add(u64::from(skipped));
        self.drop_statistics.consecutive_dropped = self
            .drop_statistics
            .consecutive_dropped
            .saturating_add(skipped);
        let forced = self.drop_statistics.consecutive_dropped >= max_consecutive;
        if forced && !self.forced_noted {
            self.drop_statistics.forced_presentations =
                self.drop_statistics.forced_presentations.saturating_add(1);
            self.forced_noted = true;
        }
        let boundary = self.is_loop_boundary(target)?;
        let protection = if boundary {
            FrameProtection::LoopBoundary
        } else if forced {
            FrameProtection::Discontinuity
        } else {
            FrameProtection::Ordinary
        };
        self.schedule(target, self.due_clock_for(target)?, protection)?;
        self.replan(target)?;
        Ok(())
    }

    fn intended_frame_at(&self, now: Instant) -> Result<RationalTime> {
        let clock = self.foreground.clock_position_at(now)?;
        let elapsed = clock
            .value()
            .checked_sub(self.clock_anchor.value())
            .ok_or_else(|| overflow("map_clock", "shared clock delta overflowed"))?;
        if elapsed <= 0 {
            return Ok(self.frame_anchor);
        }
        let clock_timebase = clock.timebase();
        let frame_timebase = self.frame_anchor.timebase();
        let numerator = i128::from(elapsed)
            .checked_mul(i128::from(clock_timebase.denominator()))
            .and_then(|value| value.checked_mul(i128::from(frame_timebase.numerator())))
            .and_then(|value| value.checked_mul(i128::from(self.rate.numerator().unsigned_abs())))
            .ok_or_else(|| overflow("map_clock", "clock-to-frame numerator overflowed"))?;
        let denominator = i128::from(clock_timebase.numerator())
            .checked_mul(i128::from(frame_timebase.denominator()))
            .and_then(|value| value.checked_mul(i128::from(self.rate.denominator())))
            .ok_or_else(|| overflow("map_clock", "clock-to-frame denominator overflowed"))?;
        let total_steps = numerator / denominator;
        let cadence_offset = i128::from(self.cadence_offset);
        if total_steps < cadence_offset {
            return Ok(self.playhead);
        }
        let local_steps = total_steps - cadence_offset;
        let signed_steps = local_steps
            .checked_mul(i128::from(self.direction().sign()))
            .ok_or_else(|| overflow("map_clock", "signed frame delta overflowed"))?;
        let intended = i128::from(self.frame_anchor.value())
            .checked_add(signed_steps)
            .ok_or_else(|| overflow("map_clock", "intended frame overflowed"))?;
        let range = self.active_range();
        let end = range.end_exclusive()?;
        let clamped = intended.clamp(
            i128::from(range.start().value()),
            i128::from(
                end.value()
                    .checked_sub(1)
                    .ok_or_else(|| overflow("map_clock", "active range boundary underflowed"))?,
            ),
        );
        let value = i64::try_from(clamped)
            .map_err(|_| overflow("map_clock", "intended frame exceeds signed range"))?;
        Ok(RationalTime::new(value, frame_timebase))
    }

    fn due_clock_for(&self, frame: RationalTime) -> Result<RationalTime> {
        self.due_clock_for_step(self.cadence_step_for(frame)?)
    }

    fn cadence_step_for(&self, frame: RationalTime) -> Result<u64> {
        let steps = self.directional_distance(self.frame_anchor, frame)?;
        if steps < 0 {
            return Err(invalid(
                "map_frame",
                "scheduled frame lies behind the current transport anchor",
            ));
        }
        let local_steps = u64::try_from(steps)
            .map_err(|_| overflow("map_frame", "frame cadence step exceeds unsigned range"))?;
        self.cadence_offset
            .checked_add(local_steps)
            .ok_or_else(|| overflow("map_frame", "frame cadence step space is exhausted"))
    }

    fn due_clock_for_step(&self, total_steps: u64) -> Result<RationalTime> {
        let frame_timebase = self.frame_anchor.timebase();
        let clock_timebase = self.clock_anchor.timebase();
        let numerator = i128::from(total_steps)
            .checked_mul(i128::from(frame_timebase.denominator()))
            .and_then(|value| value.checked_mul(i128::from(clock_timebase.numerator())))
            .and_then(|value| value.checked_mul(i128::from(self.rate.denominator())))
            .ok_or_else(|| overflow("map_frame", "frame-to-clock numerator overflowed"))?;
        let denominator = i128::from(frame_timebase.numerator())
            .checked_mul(i128::from(clock_timebase.denominator()))
            .and_then(|value| value.checked_mul(i128::from(self.rate.numerator().unsigned_abs())))
            .ok_or_else(|| overflow("map_frame", "frame-to-clock denominator overflowed"))?;
        let delta = ceil_div_positive(numerator, denominator)?;
        let value = i128::from(self.clock_anchor.value())
            .checked_add(delta)
            .ok_or_else(|| overflow("map_frame", "presentation deadline overflowed"))?;
        Ok(RationalTime::new(
            i64::try_from(value)
                .map_err(|_| overflow("map_frame", "presentation deadline exceeds signed range"))?,
            clock_timebase,
        ))
    }

    fn presentation_duration_for(
        &self,
        frame: RationalTime,
        due_clock: RationalTime,
    ) -> Result<Duration> {
        let next_step = self
            .cadence_step_for(frame)?
            .checked_add(1)
            .ok_or_else(|| overflow("map_frame", "frame cadence step space is exhausted"))?;
        let next_due = self.due_clock_for_step(next_step)?;
        let duration = next_due
            .value()
            .checked_sub(due_clock.value())
            .ok_or_else(|| overflow("map_frame", "presentation duration underflowed"))?;
        Duration::new(
            u64::try_from(duration)
                .map_err(|_| overflow("map_frame", "presentation duration is not positive"))?,
            due_clock.timebase(),
        )
    }

    fn replan(&mut self, playhead: RationalTime) -> Result<()> {
        let magnitude = u128::from(self.rate.numerator().unsigned_abs());
        let denominator = u128::from(self.rate.denominator());
        let step_magnitude = magnitude
            .checked_add(denominator - 1)
            .map(|value| value / denominator)
            .ok_or_else(|| overflow("prefetch", "prefetch step ceiling overflowed"))?
            .max(1);
        let signed_step = i64::try_from(step_magnitude)
            .map_err(|_| overflow("prefetch", "prefetch step exceeds signed range"))?
            .checked_mul(self.direction().sign())
            .ok_or_else(|| overflow("prefetch", "signed prefetch step overflowed"))?;
        let input = PlaybackPrefetchInput::new(playhead, self.active_range(), signed_step)?;
        let plan: PlaybackPrefetchPlan = self.prefetch_config.plan(input);
        let job_id = self.next_job_id()?;
        match self.prefetcher.submit(job_id, plan) {
            Ok(submission) => {
                self.latest_prefetch_generation = Some(submission.generation());
                self.prefetch_failed = false;
            }
            Err(_) => {
                self.latest_prefetch_generation = None;
                self.prefetch_failed = true;
            }
        }
        Ok(())
    }

    fn observe_prefetch(&mut self, completions: &[PlaybackPrefetchCompletion]) {
        for completion in completions {
            if Some(completion.generation()) != self.latest_prefetch_generation {
                continue;
            }
            if completion.error().is_some()
                || !matches!(
                    completion.status(),
                    JobTerminalState::Succeeded | JobTerminalState::Cancelled
                )
            {
                self.prefetch_failed = true;
            } else if completion.status() == JobTerminalState::Succeeded {
                self.prefetch_failed = false;
            }
        }
    }

    fn next_job_id(&mut self) -> Result<JobId> {
        self.next_job_counter = self.next_job_counter.checked_add(1).ok_or_else(|| {
            overflow(
                "allocate_job",
                "playback transport job identity space is exhausted",
            )
        })?;
        Ok(JobId::from_raw(
            (u128::from(self.job_namespace) << 64) | u128::from(self.next_job_counter),
        ))
    }

    fn direction(&self) -> PlaybackDirection {
        if self.rate.is_reverse() {
            PlaybackDirection::Reverse
        } else {
            PlaybackDirection::Forward
        }
    }

    fn active_range(&self) -> TimeRange {
        self.loop_range.unwrap_or(self.bounds)
    }

    fn validate_target(&self, target: RationalTime) -> Result<()> {
        let range = self.active_range();
        if target.timebase() != range.timebase() || !range.contains(target)? {
            return Err(invalid(
                "target",
                "transport target must lie inside the active exact frame range",
            ));
        }
        Ok(())
    }

    fn validate_loop(&self, range: TimeRange) -> Result<()> {
        if range.is_empty()
            || range.timebase() != self.bounds.timebase()
            || range.start() < self.bounds.start()
            || range.end_exclusive()? > self.bounds.end_exclusive()?
        {
            return Err(invalid(
                "set_loop",
                "loop must be a nonempty subrange of the exact transport bounds",
            ));
        }
        Ok(())
    }

    fn offset_frame(&self, frame: RationalTime, delta: i64) -> Result<RationalTime> {
        let range = self.active_range();
        let value = if self.loop_range.is_some() {
            let length = i128::from(range.duration().value());
            let relative = i128::from(frame.value())
                .checked_sub(i128::from(range.start().value()))
                .and_then(|value| value.checked_add(i128::from(delta)))
                .ok_or_else(|| overflow("step_frames", "looped frame-step target overflowed"))?;
            let wrapped = relative.rem_euclid(length);
            let value = i128::from(range.start().value())
                .checked_add(wrapped)
                .ok_or_else(|| overflow("step_frames", "looped frame-step target overflowed"))?;
            i64::try_from(value)
                .map_err(|_| overflow("step_frames", "looped frame-step exceeds signed range"))?
        } else {
            frame
                .value()
                .checked_add(delta)
                .ok_or_else(|| overflow("step_frames", "frame-step target overflowed"))?
        };
        let target = RationalTime::new(value, frame.timebase());
        if !range.contains(target)? {
            return Err(invalid(
                "step_frames",
                "frame-step target lies outside the active range",
            ));
        }
        Ok(target)
    }

    fn directional_distance(&self, from: RationalTime, to: RationalTime) -> Result<i64> {
        if from.timebase() != to.timebase() {
            return Err(invalid(
                "distance",
                "transport frame coordinates must share one timebase",
            ));
        }
        let raw = to
            .value()
            .checked_sub(from.value())
            .ok_or_else(|| overflow("distance", "transport frame distance overflowed"))?;
        raw.checked_mul(self.direction().sign())
            .ok_or_else(|| overflow("distance", "signed transport distance overflowed"))
    }

    fn is_loop_boundary(&self, frame: RationalTime) -> Result<bool> {
        let Some(range) = self.loop_range else {
            return Ok(false);
        };
        let boundary = match self.direction() {
            PlaybackDirection::Forward => RationalTime::new(
                range
                    .end_exclusive()?
                    .value()
                    .checked_sub(1)
                    .ok_or_else(|| overflow("loop", "loop boundary underflowed"))?,
                range.timebase(),
            ),
            PlaybackDirection::Reverse => range.start(),
        };
        Ok(frame == boundary)
    }

    fn audio_state(&self, status: OutputDiscardStatus) -> TransportAudioState {
        if self.mode != PlaybackTransportMode::Playing {
            return TransportAudioState::MutedInactive(self.mode);
        }
        if self.rate != PlaybackRate::NORMAL {
            return TransportAudioState::MutedUnsupportedRate(self.rate);
        }
        if status.is_pending() {
            TransportAudioState::DiscardPending(status)
        } else {
            TransportAudioState::Synchronized
        }
    }

    fn degradation(
        &self,
        audio_state: TransportAudioState,
        audio_discard_status: OutputDiscardStatus,
    ) -> TransportDegradation {
        let mut bits = 0;
        if self.frame_failed {
            bits |= TransportDegradationCode::FrameFailure.bit();
        }
        if self.viewport_backpressured {
            bits |= TransportDegradationCode::ViewportBackpressure.bit();
        }
        if self.prefetch_failed {
            bits |= TransportDegradationCode::PrefetchFailure.bit();
        }
        if audio_discard_status.is_pending() {
            bits |= TransportDegradationCode::AudioDiscardPending.bit();
        }
        match audio_state {
            TransportAudioState::DiscardPending(_) => {}
            TransportAudioState::MutedInactive(_) => {}
            TransportAudioState::MutedUnsupportedRate(_) => {
                bits |= TransportDegradationCode::AudioRateUnsupported.bit();
            }
            TransportAudioState::Synchronized => {}
        }
        TransportDegradation(bits)
    }

    fn require_not_scrubbing(&self, operation: &'static str) -> Result<()> {
        ExecutionDomain::Playback.require_current()?;
        if self.mode == PlaybackTransportMode::Scrubbing {
            return Err(conflict(
                operation,
                "operation is unavailable until the active scrub ends",
            ));
        }
        Ok(())
    }
}

fn ceil_div_positive(numerator: i128, denominator: i128) -> Result<i128> {
    if numerator < 0 || denominator <= 0 {
        return Err(invalid(
            "divide",
            "transport cadence division requires nonnegative values",
        ));
    }
    let adjustment = denominator
        .checked_sub(1)
        .ok_or_else(|| overflow("divide", "cadence divisor underflowed"))?;
    numerator
        .checked_add(adjustment)
        .map(|value| value / denominator)
        .ok_or_else(|| overflow("divide", "cadence ceiling overflowed"))
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    transport_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
        operation,
    )
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    transport_error(
        ErrorCategory::Conflict,
        Recoverability::Retryable,
        message,
        operation,
    )
}

fn overflow(operation: &'static str, message: &'static str) -> Error {
    transport_error(
        ErrorCategory::ResourceExhausted,
        Recoverability::Terminal,
        message,
        operation,
    )
}

fn transport_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
