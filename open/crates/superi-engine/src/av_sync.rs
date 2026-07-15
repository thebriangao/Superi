//! Engine ownership for audio-master video presentation coordination.
//!
//! The reusable clock and scheduling policy live in `superi-concurrency`. This
//! module binds them into one concrete engine owner that observes the active
//! playback clock, applies explicit discontinuity rebases, and returns visible
//! nonblocking outcomes. Live presentation corrections never rewrite media
//! timestamps or durations, so the same exact frame timing remains available
//! to deterministic render and export paths.

use std::sync::Arc;
use std::time::{Duration as StdDuration, Instant};

use superi_concurrency::clock::{
    AudioMasterClock, AvDriftMeasurement, AvFrameAction, AvFrameCorrection, AvFrameObservation,
    AvPresentationReason, AvSyncPolicy, AvSyncScheduler, AvSyncStatistics, PlaybackClock,
    PlaybackClockMode,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result, ResultExt};
use superi_core::time::{Duration as MediaDuration, RationalTime, TimeRounding};

const COMPONENT: &str = "superi-engine.av-sync";
const INTERACTIVE_PRESENTATION_TOLERANCE: StdDuration = StdDuration::from_millis(8);
const INTERACTIVE_DROP_LATENESS: StdDuration = StdDuration::from_millis(40);
const INTERACTIVE_DISCONTINUITY_THRESHOLD: StdDuration = StdDuration::from_secs(10);
const INTERACTIVE_MAXIMUM_CORRECTION: StdDuration = StdDuration::from_millis(20);
const INTERACTIVE_MAX_CONSECUTIVE_DROPS: u32 = 4;

/// Returns the engine's real-time video policy for the audio-master clock.
///
/// The eight millisecond tolerance is deliberately inside the product's
/// sub-ten-millisecond scheduling target. It is a deterministic scheduling
/// bound, not a substitute for physical device and display measurements.
pub fn interactive_av_sync_policy() -> Result<AvSyncPolicy> {
    AvSyncPolicy::new(
        INTERACTIVE_PRESENTATION_TOLERANCE,
        INTERACTIVE_DROP_LATENESS,
        INTERACTIVE_DISCONTINUITY_THRESHOLD,
        INTERACTIVE_MAXIMUM_CORRECTION,
        INTERACTIVE_MAX_CONSECUTIVE_DROPS,
    )
    .with_error_context(ErrorContext::new(COMPONENT, "create_interactive_policy"))
}

/// Exact immutable media timing retained through live synchronization.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AvSyncFrameTiming {
    presentation_time: RationalTime,
    duration: MediaDuration,
}

impl AvSyncFrameTiming {
    /// Binds one exact presentation timestamp and nominal media duration.
    #[must_use]
    pub const fn new(presentation_time: RationalTime, duration: MediaDuration) -> Self {
        Self {
            presentation_time,
            duration,
        }
    }

    /// Returns the immutable media presentation timestamp.
    #[must_use]
    pub const fn presentation_time(self) -> RationalTime {
        self.presentation_time
    }

    /// Returns the immutable nominal media duration.
    #[must_use]
    pub const fn duration(self) -> MediaDuration {
        self.duration
    }
}

/// Caller-owned evidence governing whether a late video frame may be dropped.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AvSyncFrameReadiness {
    following_frame_ready: bool,
    drop_eligible: bool,
}

impl AvSyncFrameReadiness {
    /// Creates explicit successor and semantic drop evidence.
    #[must_use]
    pub const fn new(following_frame_ready: bool, drop_eligible: bool) -> Self {
        Self {
            following_frame_ready,
            drop_eligible,
        }
    }

    /// Protects the only ready frame from dropping.
    #[must_use]
    pub const fn last_ready() -> Self {
        Self::new(false, false)
    }

    /// Returns whether presentation can immediately advance to a successor.
    #[must_use]
    pub const fn following_frame_ready(self) -> bool {
        self.following_frame_ready
    }

    /// Returns whether caller policy permits this frame to be discarded.
    #[must_use]
    pub const fn drop_eligible(self) -> bool {
        self.drop_eligible
    }
}

/// Evidence for one discontinuity that the coordinator recovered in place.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AvSyncRecovery {
    previous_master_time: RationalTime,
    target_master_time: RationalTime,
    discontinuity: AvDriftMeasurement,
}

impl AvSyncRecovery {
    /// Returns the master position observed before recovery.
    #[must_use]
    pub const fn previous_master_time(self) -> RationalTime {
        self.previous_master_time
    }

    /// Returns the exact master position applied by the clock owner.
    #[must_use]
    pub const fn target_master_time(self) -> RationalTime {
        self.target_master_time
    }

    /// Returns the signed frame drift that triggered recovery.
    #[must_use]
    pub const fn discontinuity(self) -> AvDriftMeasurement {
        self.discontinuity
    }
}

/// Complete nonblocking engine action for one ready video frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AvSyncOutcome {
    /// Retain the frame and poll again after the bounded remaining interval.
    Hold {
        /// Exact media timing, unchanged by synchronization.
        timing: AvSyncFrameTiming,
        /// Master position used for this decision.
        master_time: RationalTime,
        /// Signed video PTS minus master time.
        drift: AvDriftMeasurement,
        /// Remaining interval outside the presentation tolerance.
        wait_for: StdDuration,
    },
    /// Present the frame using an explicit live display interval.
    Present {
        /// Exact media timing, unchanged by synchronization.
        timing: AvSyncFrameTiming,
        /// Master position after any applied discontinuity recovery.
        master_time: RationalTime,
        /// Signed drift after any applied discontinuity recovery.
        drift: AvDriftMeasurement,
        /// Live display interval before targeting the next frame.
        display_for: StdDuration,
        /// Bounded video-only correction for this interval.
        correction: AvFrameCorrection,
        /// Why the frame remains visible.
        reason: AvPresentationReason,
        /// Applied recovery evidence, when a discontinuity was reanchored.
        recovery: Option<AvSyncRecovery>,
    },
    /// Discard this late frame and immediately inspect its ready successor.
    Drop {
        /// Exact media timing of the discarded frame.
        timing: AvSyncFrameTiming,
        /// Master position used for this decision.
        master_time: RationalTime,
        /// Signed video PTS minus master time.
        drift: AvDriftMeasurement,
    },
}

impl AvSyncOutcome {
    /// Returns the immutable media timing carried into the decision.
    #[must_use]
    pub const fn timing(self) -> AvSyncFrameTiming {
        match self {
            Self::Hold { timing, .. }
            | Self::Present { timing, .. }
            | Self::Drop { timing, .. } => timing,
        }
    }

    /// Returns the master position used by the final action.
    #[must_use]
    pub const fn master_time(self) -> RationalTime {
        match self {
            Self::Hold { master_time, .. }
            | Self::Present { master_time, .. }
            | Self::Drop { master_time, .. } => master_time,
        }
    }

    /// Returns signed video PTS minus the final master observation.
    #[must_use]
    pub const fn drift(self) -> AvDriftMeasurement {
        match self {
            Self::Hold { drift, .. } | Self::Present { drift, .. } | Self::Drop { drift, .. } => {
                drift
            }
        }
    }

    /// Returns the stable action code used by diagnostics and future APIs.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Hold { .. } => "hold",
            Self::Present {
                recovery: Some(_), ..
            } => "resynchronized",
            Self::Present { .. } => "present",
            Self::Drop { .. } => "drop",
        }
    }
}

/// Playback-domain owner for one master clock and one video scheduling stream.
#[derive(Debug)]
pub struct AvSyncCoordinator {
    clock: PlaybackClock,
    scheduler: AvSyncScheduler,
}

impl AvSyncCoordinator {
    /// Creates an audio-master coordinator with the engine interactive policy.
    pub fn audio_master(
        timeline_anchor: RationalTime,
        source: Arc<AudioMasterClock>,
    ) -> Result<Self> {
        Self::audio_master_with_policy(timeline_anchor, source, interactive_av_sync_policy()?)
    }

    /// Creates an audio-master coordinator with an explicit validated policy.
    pub fn audio_master_with_policy(
        timeline_anchor: RationalTime,
        source: Arc<AudioMasterClock>,
        policy: AvSyncPolicy,
    ) -> Result<Self> {
        Ok(Self {
            clock: PlaybackClock::audio_master(timeline_anchor, source)
                .with_error_context(ErrorContext::new(COMPONENT, "create_audio_master"))?,
            scheduler: AvSyncScheduler::new(policy)
                .with_error_context(ErrorContext::new(COMPONENT, "create_scheduler"))?,
        })
    }

    /// Returns the active clock source.
    #[must_use]
    pub const fn clock_mode(&self) -> PlaybackClockMode {
        self.clock.mode()
    }

    /// Returns the immutable scheduling policy.
    #[must_use]
    pub const fn policy(&self) -> AvSyncPolicy {
        self.scheduler.policy()
    }

    /// Returns a coherent copy of scheduler diagnostics.
    #[must_use]
    pub const fn statistics(&self) -> AvSyncStatistics {
        self.scheduler.statistics()
    }

    /// Reads the shared timeline clock at an explicit process-local instant.
    pub fn clock_position_at(&self, now: Instant) -> Result<RationalTime> {
        ExecutionDomain::Playback.require_current()?;
        self.clock
            .position_at(now)
            .with_error_context(ErrorContext::new(COMPONENT, "read_clock"))
    }

    /// Falls back to monotonic playback without changing timeline position.
    pub fn fallback_to_playback_at(&mut self, now: Instant) -> Result<RationalTime> {
        ExecutionDomain::Playback.require_current()?;
        self.clock
            .switch_to_playback_at(now)
            .with_error_context(ErrorContext::new(COMPONENT, "fallback_to_playback"))
    }

    /// Restores an audio device clock without changing timeline position.
    pub fn recover_audio_master_at(
        &mut self,
        source: Arc<AudioMasterClock>,
        now: Instant,
    ) -> Result<RationalTime> {
        ExecutionDomain::Playback.require_current()?;
        self.clock
            .switch_to_audio_master_at(source, now)
            .with_error_context(ErrorContext::new(COMPONENT, "recover_audio_master"))
    }

    /// Coordinates one ready frame without sleeping, blocking, or touching audio.
    pub fn coordinate_at(
        &mut self,
        timing: AvSyncFrameTiming,
        readiness: AvSyncFrameReadiness,
        now: Instant,
    ) -> Result<AvSyncOutcome> {
        ExecutionDomain::Playback.require_current()?;
        let master_time = self
            .clock
            .position_at(now)
            .with_error_context(ErrorContext::new(COMPONENT, "observe_master"))?;
        let decision = self
            .schedule(timing, readiness, master_time)
            .with_error_context(ErrorContext::new(COMPONENT, "schedule_frame"))?;

        match decision.action() {
            AvFrameAction::Wait { duration } => Ok(AvSyncOutcome::Hold {
                timing,
                master_time,
                drift: decision.drift(),
                wait_for: duration,
            }),
            AvFrameAction::Present {
                display_for,
                correction,
                reason,
            } => Ok(AvSyncOutcome::Present {
                timing,
                master_time,
                drift: decision.drift(),
                display_for,
                correction,
                reason,
                recovery: None,
            }),
            AvFrameAction::DropLate => Ok(AvSyncOutcome::Drop {
                timing,
                master_time,
                drift: decision.drift(),
            }),
            AvFrameAction::Rebase { target_master_time } => self.recover_discontinuity(
                timing,
                readiness,
                now,
                master_time,
                target_master_time,
                decision.drift(),
            ),
            _ => Err(Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "scheduler returned an unknown frame action",
            )
            .with_context(ErrorContext::new(COMPONENT, "schedule_frame"))),
        }
    }

    fn schedule(
        &mut self,
        timing: AvSyncFrameTiming,
        readiness: AvSyncFrameReadiness,
        master_time: RationalTime,
    ) -> Result<superi_concurrency::clock::AvScheduleDecision> {
        self.scheduler.schedule(
            AvFrameObservation::new(timing.presentation_time(), master_time, timing.duration())
                .with_following_frame_ready(readiness.following_frame_ready())
                .with_drop_eligible(readiness.drop_eligible()),
        )
    }

    fn recover_discontinuity(
        &mut self,
        timing: AvSyncFrameTiming,
        readiness: AvSyncFrameReadiness,
        now: Instant,
        previous_master_time: RationalTime,
        requested_target: RationalTime,
        discontinuity: AvDriftMeasurement,
    ) -> Result<AvSyncOutcome> {
        let target_master_time = requested_target
            .checked_rescale(self.clock.timeline_timebase(), TimeRounding::Exact)
            .with_error_context(ErrorContext::new(COMPONENT, "rescale_rebase_target"))?;
        self.clock
            .reanchor_at(target_master_time, now)
            .with_error_context(ErrorContext::new(COMPONENT, "apply_rebase"))?;
        let aligned = self
            .schedule(timing, readiness, target_master_time)
            .with_error_context(ErrorContext::new(COMPONENT, "schedule_rebased_frame"))?;

        let AvFrameAction::Present {
            display_for,
            correction,
            reason,
        } = aligned.action()
        else {
            return Err(Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "rebased frame did not produce an aligned presentation",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "schedule_rebased_frame")
                    .with_field("action", aligned.action().code()),
            ));
        };

        Ok(AvSyncOutcome::Present {
            timing,
            master_time: target_master_time,
            drift: aligned.drift(),
            display_for,
            correction,
            reason,
            recovery: Some(AvSyncRecovery {
                previous_master_time,
                target_master_time,
                discontinuity,
            }),
        })
    }
}
