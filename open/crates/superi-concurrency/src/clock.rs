//! Playback clocks plus exact A/V drift measurement and video scheduling.
//!
//! The playback scheduler owns [`PlaybackClock`]. In playback mode, positions
//! advance from an opaque monotonic [`Instant`] anchor. In audio-master mode,
//! positions advance from the sample currently presented by the audio device
//! through [`AudioMasterClock`]. Both modes recompute from a fixed anchor so
//! quantization never accumulates across reads.
//!
//! Platform audio callbacks publish only one fixed-frequency sample counter.
//! Successful publication uses one atomic value and neither locks nor
//! allocates. Device resets and frequency changes require an explicit new
//! source and clock re-anchor.
//!
//! [`AvSyncScheduler`] is mutable state owned by the playback execution domain.
//! It compares one video presentation timestamp with a caller-supplied clock
//! observation, then returns an explicit wait, present, correction, drop, or
//! rebase action. It never sleeps, waits on a lock, submits work, or changes the
//! audio clock.

use std::fmt;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result, ResultExt};
use superi_core::time::{
    Duration as MediaDuration, RationalTime, SampleTime, TimeRounding, Timebase,
};

use crate::threads::ExecutionDomain;

const COMPONENT: &str = "superi-concurrency.clock";
const DEFAULT_PRESENTATION_TOLERANCE_NS: u64 = 20_000_000;
const DEFAULT_DROP_LATENESS_NS: u64 = 40_000_000;
const DEFAULT_DISCONTINUITY_THRESHOLD_NS: u64 = 10_000_000_000;
const DEFAULT_MAX_CORRECTION_NS: u64 = 20_000_000;
const DEFAULT_MAX_CONSECUTIVE_DROPS: u32 = 4;

/// The source that advances a playback timeline.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum PlaybackClockMode {
    /// Advance from an in-process monotonic clock.
    Playback,
    /// Advance from the audio device's presented sample position.
    AudioMaster,
}

impl PlaybackClockMode {
    /// Every mode in stable diagnostic order.
    pub const ALL: &'static [Self] = &[Self::Playback, Self::AudioMaster];

    /// Returns the stable machine code used by diagnostics and future APIs.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Playback => "playback",
            Self::AudioMaster => "audio_master",
        }
    }

    /// Looks up a mode by its stable machine code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "playback" => Some(Self::Playback),
            "audio_master" => Some(Self::AudioMaster),
            _ => None,
        }
    }
}

/// The result of publishing an audio device position.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum AudioClockUpdate {
    /// The presented sample position advanced.
    Advanced,
    /// The device repeated its current position.
    Unchanged,
}

impl AudioClockUpdate {
    /// Returns the stable machine code used by diagnostics and future APIs.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Advanced => "advanced",
            Self::Unchanged => "unchanged",
        }
    }
}

/// A fixed-frequency audio device position shared with the playback scheduler.
///
/// The platform callback is the sole writer. The playback scheduler and
/// diagnostics may read from any thread. The sample rate never changes during
/// this source's lifetime, and the sample position never decreases. Create a
/// new source after a device reset or format change.
#[derive(Debug)]
pub struct AudioMasterClock {
    sample_rate: u32,
    sample: AtomicI64,
}

impl AudioMasterClock {
    /// Creates an audio source at its first valid presented sample position.
    #[must_use]
    pub fn new(initial_position: SampleTime) -> Self {
        Self {
            sample_rate: initial_position.sample_rate(),
            sample: AtomicI64::new(initial_position.sample()),
        }
    }

    /// Returns the fixed number of device clock units per second.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the latest presented audio position.
    #[must_use]
    pub fn position(&self) -> SampleTime {
        SampleTime::new(self.sample.load(Ordering::Relaxed), self.sample_rate)
            .expect("AudioMasterClock retains a validated nonzero sample rate")
    }

    /// Publishes a typed device position.
    ///
    /// A frequency mismatch is not converted implicitly because a reset or
    /// device change needs an explicit timeline re-anchor.
    pub fn publish(&self, position: SampleTime) -> Result<AudioClockUpdate> {
        if position.sample_rate() != self.sample_rate {
            return Err(clock_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "audio clock frequency changed without an explicit re-anchor",
                "publish_audio_position",
                PlaybackClockMode::AudioMaster,
            )
            .with_context_fields([
                ("expected_sample_rate", self.sample_rate.to_string()),
                ("actual_sample_rate", position.sample_rate().to_string()),
            ]));
        }
        self.publish_sample(position.sample())
    }

    /// Publishes one raw position in this source's fixed sample clock.
    ///
    /// The successful path performs only atomic operations and does not lock or
    /// allocate. Equal readings are valid because platform device positions may
    /// remain unchanged between adjacent observations.
    pub fn publish_sample(&self, observed_sample: i64) -> Result<AudioClockUpdate> {
        let mut current_sample = self.sample.load(Ordering::Relaxed);
        loop {
            if observed_sample < current_sample {
                return Err(clock_error(
                    ErrorCategory::Conflict,
                    Recoverability::Retryable,
                    "audio clock position moved backwards and requires re-anchoring",
                    "publish_audio_position",
                    PlaybackClockMode::AudioMaster,
                )
                .with_context_fields([
                    ("current_sample", current_sample.to_string()),
                    ("observed_sample", observed_sample.to_string()),
                ]));
            }
            if observed_sample == current_sample {
                return Ok(AudioClockUpdate::Unchanged);
            }

            match self.sample.compare_exchange_weak(
                current_sample,
                observed_sample,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(AudioClockUpdate::Advanced),
                Err(observed_current) => current_sample = observed_current,
            }
        }
    }
}

#[derive(Debug)]
enum ClockSource {
    Playback {
        monotonic_anchor: Instant,
    },
    AudioMaster {
        source: Arc<AudioMasterClock>,
        sample_anchor: SampleTime,
    },
}

/// A scheduler-owned mapping from one progressing source to timeline time.
///
/// Reads are deterministic relative to the stored anchors. They never update
/// the anchor or integrate a previous rounded position, so frequent polling
/// cannot introduce drift. Mutation is limited to explicit seeks and mode
/// changes owned by the playback scheduler.
#[derive(Debug)]
pub struct PlaybackClock {
    timeline_anchor: RationalTime,
    source: ClockSource,
}

impl PlaybackClock {
    /// Creates a monotonic playback clock at an explicit process-local instant.
    #[must_use]
    pub fn playback(timeline_anchor: RationalTime, monotonic_anchor: Instant) -> Self {
        Self {
            timeline_anchor,
            source: ClockSource::Playback { monotonic_anchor },
        }
    }

    /// Creates an audio-master clock at the source's current audible position.
    ///
    /// Timeline and audio timebases must match so every presented sample maps
    /// exactly without rounding.
    pub fn audio_master(
        timeline_anchor: RationalTime,
        source: Arc<AudioMasterClock>,
    ) -> Result<Self> {
        let sample_anchor = source.position();
        validate_audio_timebase(timeline_anchor, sample_anchor, "anchor_audio_master")?;
        Ok(Self {
            timeline_anchor,
            source: ClockSource::AudioMaster {
                source,
                sample_anchor,
            },
        })
    }

    /// Returns the active clock source.
    #[must_use]
    pub const fn mode(&self) -> PlaybackClockMode {
        match &self.source {
            ClockSource::Playback { .. } => PlaybackClockMode::Playback,
            ClockSource::AudioMaster { .. } => PlaybackClockMode::AudioMaster,
        }
    }

    /// Returns the timeline units emitted by this clock.
    #[must_use]
    pub const fn timeline_timebase(&self) -> Timebase {
        self.timeline_anchor.timebase()
    }

    /// Reads the current timeline position.
    ///
    /// Audio-master reads do not sample the process clock. Playback reads use a
    /// fresh process-local monotonic instant.
    pub fn position(&self) -> Result<RationalTime> {
        match &self.source {
            ClockSource::Playback { .. } => self.position_at(Instant::now()),
            ClockSource::AudioMaster { .. } => self.audio_master_position(),
        }
    }

    /// Reads the position at an explicit process-local instant.
    ///
    /// The supplied instant is ignored in audio-master mode because the audio
    /// device position is authoritative.
    pub fn position_at(&self, now: Instant) -> Result<RationalTime> {
        match &self.source {
            ClockSource::Playback { monotonic_anchor } => {
                self.playback_position(*monotonic_anchor, now)
            }
            ClockSource::AudioMaster { .. } => self.audio_master_position(),
        }
    }

    /// Switches to the audio device while preserving the current timeline position.
    ///
    /// `now` samples the prior playback mode if it is active. The operation
    /// fails without mutation when the timeline and device timebases differ.
    pub fn switch_to_audio_master_at(
        &mut self,
        source: Arc<AudioMasterClock>,
        now: Instant,
    ) -> Result<RationalTime> {
        let timeline_anchor = self.position_at(now)?;
        let sample_anchor = source.position();
        validate_audio_timebase(timeline_anchor, sample_anchor, "switch_to_audio_master")?;
        self.timeline_anchor = timeline_anchor;
        self.source = ClockSource::AudioMaster {
            source,
            sample_anchor,
        };
        Ok(timeline_anchor)
    }

    /// Switches to the process clock while preserving the current timeline position.
    pub fn switch_to_playback_at(&mut self, now: Instant) -> Result<RationalTime> {
        let timeline_anchor = self.position_at(now)?;
        self.timeline_anchor = timeline_anchor;
        self.source = ClockSource::Playback {
            monotonic_anchor: now,
        };
        Ok(timeline_anchor)
    }

    /// Re-anchors the active mode at a seek or other explicit discontinuity.
    ///
    /// Audio-master mode retains exact sample timing and therefore rejects a
    /// target expressed in a different timebase.
    pub fn reanchor_at(&mut self, timeline_anchor: RationalTime, now: Instant) -> Result<()> {
        match &mut self.source {
            ClockSource::Playback { monotonic_anchor } => {
                *monotonic_anchor = now;
            }
            ClockSource::AudioMaster {
                source,
                sample_anchor,
            } => {
                let observed = source.position();
                validate_audio_timebase(timeline_anchor, observed, "reanchor_audio_master")?;
                *sample_anchor = observed;
            }
        }
        self.timeline_anchor = timeline_anchor;
        Ok(())
    }

    fn playback_position(&self, anchor: Instant, now: Instant) -> Result<RationalTime> {
        let elapsed = now.checked_duration_since(anchor).ok_or_else(|| {
            clock_error(
                ErrorCategory::Conflict,
                Recoverability::Retryable,
                "monotonic playback clock moved backwards",
                "read_position",
                PlaybackClockMode::Playback,
            )
        })?;
        let elapsed = duration_time(elapsed)?;
        self.timeline_anchor
            .checked_add_at(
                elapsed,
                self.timeline_timebase(),
                TimeRounding::NearestTiesEven,
            )
            .with_error_context(clock_context("read_position", PlaybackClockMode::Playback))
    }

    fn audio_master_position(&self) -> Result<RationalTime> {
        let ClockSource::AudioMaster {
            source,
            sample_anchor,
        } = &self.source
        else {
            return Err(clock_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "audio clock source was unavailable in audio-master mode",
                "read_position",
                PlaybackClockMode::AudioMaster,
            )
            .into());
        };

        let observed = source.position();
        let elapsed_samples = observed
            .sample()
            .checked_sub(sample_anchor.sample())
            .ok_or_else(|| {
                clock_error(
                    ErrorCategory::Internal,
                    Recoverability::Terminal,
                    "audio clock elapsed sample count overflowed",
                    "read_position",
                    PlaybackClockMode::AudioMaster,
                )
            })?;
        if elapsed_samples < 0 {
            return Err(clock_error(
                ErrorCategory::Conflict,
                Recoverability::Retryable,
                "audio clock position moved behind its timeline anchor",
                "read_position",
                PlaybackClockMode::AudioMaster,
            )
            .into());
        }

        self.timeline_anchor
            .checked_add_at(
                RationalTime::new(elapsed_samples, observed.timebase()),
                self.timeline_timebase(),
                TimeRounding::Exact,
            )
            .with_error_context(clock_context(
                "read_position",
                PlaybackClockMode::AudioMaster,
            ))
    }
}

fn duration_time(duration: Duration) -> Result<RationalTime> {
    let nanoseconds = i64::try_from(duration.as_nanos()).map_err(|_| {
        clock_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "playback clock duration exceeds the shared signed time domain",
            "read_position",
            PlaybackClockMode::Playback,
        )
    })?;
    Ok(RationalTime::new(nanoseconds, Timebase::NANOSECONDS))
}

fn validate_audio_timebase(
    timeline_anchor: RationalTime,
    sample_anchor: SampleTime,
    operation: &'static str,
) -> Result<()> {
    if timeline_anchor.timebase() == sample_anchor.timebase() {
        return Ok(());
    }
    Err(clock_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "audio-master timeline timebase must match the device sample clock",
        operation,
        PlaybackClockMode::AudioMaster,
    )
    .with_context_fields([
        ("timeline_timebase", timeline_anchor.timebase().to_string()),
        ("audio_timebase", sample_anchor.timebase().to_string()),
    ]))
}

fn clock_context(operation: &'static str, mode: PlaybackClockMode) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation).with_field("mode", mode.code())
}

fn clock_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
    mode: PlaybackClockMode,
) -> ClockErrorBuilder {
    ClockErrorBuilder {
        error: Error::new(category, recoverability, message),
        context: clock_context(operation, mode),
    }
}

struct ClockErrorBuilder {
    error: Error,
    context: ErrorContext,
}

impl ClockErrorBuilder {
    fn with_context_fields<const N: usize>(mut self, fields: [(&'static str, String); N]) -> Error {
        for (key, value) in fields {
            self.context.insert_field(key, value);
        }
        self.error.with_context(self.context)
    }
}

impl From<ClockErrorBuilder> for Error {
    fn from(builder: ClockErrorBuilder) -> Self {
        builder.error.with_context(builder.context)
    }
}

/// Whether video leads, matches, or trails the observed master timeline.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum AvDriftDirection {
    /// The video presentation timestamp is later than master time.
    VideoAhead,
    /// Video and master time are equal at nanosecond precision.
    Aligned,
    /// The video presentation timestamp is earlier than master time.
    VideoBehind,
}

impl AvDriftDirection {
    /// Returns the stable diagnostic code for this direction.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::VideoAhead => "video_ahead",
            Self::Aligned => "aligned",
            Self::VideoBehind => "video_behind",
        }
    }
}

impl fmt::Display for AvDriftDirection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// One exact signed comparison of video PTS against master time.
///
/// Positive values mean video is ahead and must wait. Negative values mean
/// video is late. The measurement uses one checked rescale into nanoseconds,
/// with nearest-even rounding only at that public precision boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AvDriftMeasurement {
    nanoseconds: i64,
}

impl AvDriftMeasurement {
    const fn new(nanoseconds: i64) -> Self {
        Self { nanoseconds }
    }

    /// Returns signed video PTS minus master time in nanoseconds.
    #[must_use]
    pub const fn nanoseconds(self) -> i64 {
        self.nanoseconds
    }

    /// Returns the direction of this measurement.
    #[must_use]
    pub const fn direction(self) -> AvDriftDirection {
        if self.nanoseconds > 0 {
            AvDriftDirection::VideoAhead
        } else if self.nanoseconds < 0 {
            AvDriftDirection::VideoBehind
        } else {
            AvDriftDirection::Aligned
        }
    }

    /// Returns the magnitude without overflowing for the minimum signed value.
    #[must_use]
    pub const fn absolute_duration(self) -> Duration {
        Duration::from_nanos(self.nanoseconds.unsigned_abs())
    }
}

impl fmt::Display for AvDriftMeasurement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} ns ({})",
            self.nanoseconds,
            self.direction().code()
        )
    }
}

/// Measures video PTS against a master-clock observation.
///
/// This pure operation has no execution-domain requirement. Scheduling a frame
/// from the result is restricted to [`ExecutionDomain::Playback`].
pub fn measure_av_drift(
    video_presentation_time: RationalTime,
    master_time: RationalTime,
) -> Result<AvDriftMeasurement> {
    video_presentation_time
        .checked_sub_at(
            master_time,
            Timebase::NANOSECONDS,
            TimeRounding::NearestTiesEven,
        )
        .map(|drift| AvDriftMeasurement::new(drift.value()))
        .with_error_context(ErrorContext::new(COMPONENT, "measure_av_drift"))
}

/// Validated thresholds governing one A/V scheduling stream.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AvSyncPolicy {
    presentation_tolerance: Duration,
    drop_lateness: Duration,
    discontinuity_threshold: Duration,
    maximum_correction: Duration,
    max_consecutive_drops: u32,
}

impl AvSyncPolicy {
    /// Creates a deterministic policy.
    ///
    /// Frames within `presentation_tolerance` are presented unchanged. Later
    /// frames may shorten one display interval up to `maximum_correction`.
    /// Frames at least `drop_lateness` late may be dropped when the observation
    /// permits it. Drift at least `discontinuity_threshold` requests a rebase.
    pub fn new(
        presentation_tolerance: Duration,
        drop_lateness: Duration,
        discontinuity_threshold: Duration,
        maximum_correction: Duration,
        max_consecutive_drops: u32,
    ) -> Result<Self> {
        let policy = Self {
            presentation_tolerance,
            drop_lateness,
            discontinuity_threshold,
            maximum_correction,
            max_consecutive_drops,
        };
        policy.validate()?;
        Ok(policy)
    }

    /// Returns the allowed early or late region requiring no correction.
    #[must_use]
    pub const fn presentation_tolerance(self) -> Duration {
        self.presentation_tolerance
    }

    /// Returns the lateness at which eligible video may be dropped.
    #[must_use]
    pub const fn drop_lateness(self) -> Duration {
        self.drop_lateness
    }

    /// Returns the magnitude treated as a timeline discontinuity.
    #[must_use]
    pub const fn discontinuity_threshold(self) -> Duration {
        self.discontinuity_threshold
    }

    /// Returns the largest presentation-interval correction for one frame.
    #[must_use]
    pub const fn maximum_correction(self) -> Duration {
        self.maximum_correction
    }

    /// Returns the drop streak cap that guarantees visible progress.
    #[must_use]
    pub const fn max_consecutive_drops(self) -> u32 {
        self.max_consecutive_drops
    }

    fn validate(self) -> Result<()> {
        let tolerance_ns = duration_nanoseconds(
            self.presentation_tolerance,
            "validate_policy",
            "presentation_tolerance",
        )?;
        let drop_ns = duration_nanoseconds(self.drop_lateness, "validate_policy", "drop_lateness")?;
        let discontinuity_ns = duration_nanoseconds(
            self.discontinuity_threshold,
            "validate_policy",
            "discontinuity_threshold",
        )?;
        let correction_ns = duration_nanoseconds(
            self.maximum_correction,
            "validate_policy",
            "maximum_correction",
        )?;

        if tolerance_ns == 0 {
            return Err(policy_error(
                "presentation tolerance must be greater than zero",
                "presentation_tolerance_ns",
                tolerance_ns,
            ));
        }
        if drop_ns <= tolerance_ns {
            return Err(policy_error(
                "drop lateness must exceed presentation tolerance",
                "drop_lateness_ns",
                drop_ns,
            ));
        }
        if discontinuity_ns <= drop_ns {
            return Err(policy_error(
                "discontinuity threshold must exceed drop lateness",
                "discontinuity_threshold_ns",
                discontinuity_ns,
            ));
        }
        if correction_ns == 0 || correction_ns > drop_ns {
            return Err(policy_error(
                "maximum correction must be nonzero and no greater than drop lateness",
                "maximum_correction_ns",
                correction_ns,
            ));
        }
        if self.max_consecutive_drops == 0 {
            return Err(policy_error(
                "maximum consecutive drops must be greater than zero",
                "max_consecutive_drops",
                0,
            ));
        }
        Ok(())
    }
}

impl Default for AvSyncPolicy {
    fn default() -> Self {
        Self {
            presentation_tolerance: Duration::from_nanos(DEFAULT_PRESENTATION_TOLERANCE_NS),
            drop_lateness: Duration::from_nanos(DEFAULT_DROP_LATENESS_NS),
            discontinuity_threshold: Duration::from_nanos(DEFAULT_DISCONTINUITY_THRESHOLD_NS),
            maximum_correction: Duration::from_nanos(DEFAULT_MAX_CORRECTION_NS),
            max_consecutive_drops: DEFAULT_MAX_CONSECUTIVE_DROPS,
        }
    }
}

/// One queued video frame compared with the current master timeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AvFrameObservation {
    video_presentation_time: RationalTime,
    master_time: RationalTime,
    frame_duration: MediaDuration,
    following_frame_ready: bool,
    drop_eligible: bool,
}

impl AvFrameObservation {
    /// Creates an observation that conservatively preserves the frame.
    ///
    /// Dropping remains disabled until the caller explicitly marks both the
    /// frame eligible and a following frame ready.
    #[must_use]
    pub const fn new(
        video_presentation_time: RationalTime,
        master_time: RationalTime,
        frame_duration: MediaDuration,
    ) -> Self {
        Self {
            video_presentation_time,
            master_time,
            frame_duration,
            following_frame_ready: false,
            drop_eligible: false,
        }
    }

    /// Marks whether presentation can advance immediately after this frame.
    #[must_use]
    pub const fn with_following_frame_ready(mut self, ready: bool) -> Self {
        self.following_frame_ready = ready;
        self
    }

    /// Marks whether the caller permits this video frame to be dropped.
    #[must_use]
    pub const fn with_drop_eligible(mut self, eligible: bool) -> Self {
        self.drop_eligible = eligible;
        self
    }

    /// Returns the frame presentation timestamp.
    #[must_use]
    pub const fn video_presentation_time(self) -> RationalTime {
        self.video_presentation_time
    }

    /// Returns the observed master time.
    #[must_use]
    pub const fn master_time(self) -> RationalTime {
        self.master_time
    }

    /// Returns the nominal frame duration.
    #[must_use]
    pub const fn frame_duration(self) -> MediaDuration {
        self.frame_duration
    }

    /// Returns whether a successor can be displayed immediately.
    #[must_use]
    pub const fn following_frame_ready(self) -> bool {
        self.following_frame_ready
    }

    /// Returns whether policy may drop this frame.
    #[must_use]
    pub const fn drop_eligible(self) -> bool {
        self.drop_eligible
    }
}

/// A bounded correction applied to one presented video frame.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum AvFrameCorrection {
    /// Keep the nominal presentation interval.
    None,
    /// Reduce the next video presentation interval by this amount.
    ShortenBy(Duration),
}

impl AvFrameCorrection {
    /// Returns the stable diagnostic code for this correction.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::ShortenBy(_) => "shorten",
        }
    }
}

/// Why a frame was presented rather than held or dropped.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum AvPresentationReason {
    /// Drift fell inside the configured tolerance.
    InTolerance,
    /// Moderate lateness was corrected by shortening one video interval.
    CorrectedLate,
    /// The caller marked the frame as semantically protected from dropping.
    ProtectedFromDrop,
    /// Dropping would empty ready video output, so the late frame was shown.
    NoReadySuccessor,
    /// The drop streak reached its cap, so one frame was shown for progress.
    StarvationGuard,
}

impl AvPresentationReason {
    /// Returns the stable diagnostic code for this reason.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InTolerance => "in_tolerance",
            Self::CorrectedLate => "corrected_late",
            Self::ProtectedFromDrop => "protected_from_drop",
            Self::NoReadySuccessor => "no_ready_successor",
            Self::StarvationGuard => "starvation_guard",
        }
    }
}

/// The complete nonblocking instruction returned to the playback loop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AvFrameAction {
    /// Keep the previously presented frame and revisit this frame later.
    Wait {
        /// Remaining time outside the allowed early tolerance.
        duration: Duration,
    },
    /// Present the frame now using an explicit next display interval.
    Present {
        /// Duration before the playback loop should target the next frame.
        display_for: Duration,
        /// Bounded video-only correction applied to this interval.
        correction: AvFrameCorrection,
        /// Why the frame was presented.
        reason: AvPresentationReason,
    },
    /// Discard this late video frame and immediately inspect its successor.
    DropLate,
    /// Ask the concrete clock owner to anchor master time at this frame.
    Rebase {
        /// New master timeline position requested after discontinuity.
        target_master_time: RationalTime,
    },
}

impl AvFrameAction {
    /// Returns the stable diagnostic code for this action.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Wait { .. } => "wait",
            Self::Present { .. } => "present",
            Self::DropLate => "drop_late",
            Self::Rebase { .. } => "rebase",
        }
    }
}

/// One action paired with the exact drift that caused it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AvScheduleDecision {
    drift: AvDriftMeasurement,
    action: AvFrameAction,
}

impl AvScheduleDecision {
    const fn new(drift: AvDriftMeasurement, action: AvFrameAction) -> Self {
        Self { drift, action }
    }

    /// Returns signed video drift for diagnostics and upstream QoS decisions.
    #[must_use]
    pub const fn drift(self) -> AvDriftMeasurement {
        self.drift
    }

    /// Returns the action the playback owner must perform.
    #[must_use]
    pub const fn action(self) -> AvFrameAction {
        self.action
    }
}

/// Deterministic counters for one scheduler lifetime.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct AvSyncStatistics {
    observations: u64,
    waited: u64,
    presented: u64,
    corrected: u64,
    dropped: u64,
    rebases: u64,
    protected_presentations: u64,
    starvation_presentations: u64,
    consecutive_drops: u32,
    maximum_absolute_drift_ns: u64,
    last_drift: Option<AvDriftMeasurement>,
}

impl AvSyncStatistics {
    /// Returns valid scheduling observations processed.
    #[must_use]
    pub const fn observations(self) -> u64 {
        self.observations
    }

    /// Returns wait decisions.
    #[must_use]
    pub const fn waited(self) -> u64 {
        self.waited
    }

    /// Returns frames presented, including corrected and protected frames.
    #[must_use]
    pub const fn presented(self) -> u64 {
        self.presented
    }

    /// Returns presentations whose interval was shortened.
    #[must_use]
    pub const fn corrected(self) -> u64 {
        self.corrected
    }

    /// Returns late video frames dropped.
    #[must_use]
    pub const fn dropped(self) -> u64 {
        self.dropped
    }

    /// Returns discontinuities that requested a clock rebase.
    #[must_use]
    pub const fn rebases(self) -> u64 {
        self.rebases
    }

    /// Returns late presentations caused by drop protection or readiness.
    #[must_use]
    pub const fn protected_presentations(self) -> u64 {
        self.protected_presentations
    }

    /// Returns presentations forced by the consecutive-drop cap.
    #[must_use]
    pub const fn starvation_presentations(self) -> u64 {
        self.starvation_presentations
    }

    /// Returns the current consecutive late-drop streak.
    #[must_use]
    pub const fn consecutive_drops(self) -> u32 {
        self.consecutive_drops
    }

    /// Returns the largest drift magnitude observed.
    #[must_use]
    pub const fn maximum_absolute_drift(self) -> Duration {
        Duration::from_nanos(self.maximum_absolute_drift_ns)
    }

    /// Returns the most recent drift measurement.
    #[must_use]
    pub const fn last_drift(self) -> Option<AvDriftMeasurement> {
        self.last_drift
    }

    fn record(&mut self, decision: AvScheduleDecision) {
        self.observations = self.observations.saturating_add(1);
        self.maximum_absolute_drift_ns = self
            .maximum_absolute_drift_ns
            .max(decision.drift.nanoseconds().unsigned_abs());
        self.last_drift = Some(decision.drift);

        match decision.action {
            AvFrameAction::Wait { .. } => {
                self.waited = self.waited.saturating_add(1);
            }
            AvFrameAction::Present {
                correction, reason, ..
            } => {
                self.presented = self.presented.saturating_add(1);
                self.consecutive_drops = 0;
                if matches!(correction, AvFrameCorrection::ShortenBy(_)) {
                    self.corrected = self.corrected.saturating_add(1);
                }
                if matches!(
                    reason,
                    AvPresentationReason::ProtectedFromDrop
                        | AvPresentationReason::NoReadySuccessor
                        | AvPresentationReason::StarvationGuard
                ) {
                    self.protected_presentations = self.protected_presentations.saturating_add(1);
                }
                if reason == AvPresentationReason::StarvationGuard {
                    self.starvation_presentations = self.starvation_presentations.saturating_add(1);
                }
            }
            AvFrameAction::DropLate => {
                self.dropped = self.dropped.saturating_add(1);
                self.consecutive_drops = self.consecutive_drops.saturating_add(1);
            }
            AvFrameAction::Rebase { .. } => {
                self.rebases = self.rebases.saturating_add(1);
                self.consecutive_drops = 0;
            }
        }
    }
}

/// Single-owner A/V scheduler for one playback stream.
///
/// The scheduler contains no mutex, condition variable, worker, or callback.
/// [`Self::schedule`] is constant-time and requires the current thread to own
/// [`ExecutionDomain::Playback`]. This keeps timing work away from UI and audio
/// domains while making every wait and correction visible to the caller.
#[derive(Debug)]
pub struct AvSyncScheduler {
    policy: AvSyncPolicy,
    statistics: AvSyncStatistics,
}

impl AvSyncScheduler {
    /// Creates an empty scheduler after revalidating all policy invariants.
    pub fn new(policy: AvSyncPolicy) -> Result<Self> {
        policy.validate()?;
        Ok(Self {
            policy,
            statistics: AvSyncStatistics::default(),
        })
    }

    /// Returns the immutable policy for this stream.
    #[must_use]
    pub const fn policy(&self) -> AvSyncPolicy {
        self.policy
    }

    /// Returns a coherent copy of the playback-owned counters.
    #[must_use]
    pub const fn statistics(&self) -> AvSyncStatistics {
        self.statistics
    }

    /// Clears diagnostics and drop history after a seek or new stream epoch.
    pub fn reset(&mut self) {
        self.statistics = AvSyncStatistics::default();
    }

    /// Produces one bounded scheduling action without waiting or mutating audio.
    pub fn schedule(&mut self, observation: AvFrameObservation) -> Result<AvScheduleDecision> {
        ExecutionDomain::Playback.require_current()?;

        let frame_duration = observation
            .frame_duration
            .checked_rescale(Timebase::NANOSECONDS, TimeRounding::NearestTiesEven)
            .with_error_context(ErrorContext::new(COMPONENT, "validate_observation"))?;
        let frame_duration_ns = frame_duration.value();
        if frame_duration_ns == 0 {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "frame duration must be positive at scheduler precision",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "validate_observation")
                    .with_field("frame_duration_ns", "0"),
            ));
        }

        let drift = measure_av_drift(observation.video_presentation_time, observation.master_time)?;
        let action = self.choose_action(observation, frame_duration_ns, drift);
        let decision = AvScheduleDecision::new(drift, action);
        self.statistics.record(decision);
        Ok(decision)
    }

    fn choose_action(
        &self,
        observation: AvFrameObservation,
        frame_duration_ns: u64,
        drift: AvDriftMeasurement,
    ) -> AvFrameAction {
        let drift_ns = drift.nanoseconds();
        let magnitude_ns = drift_ns.unsigned_abs();
        let tolerance_ns = self.policy.presentation_tolerance.as_nanos() as u64;
        let drop_ns = self.policy.drop_lateness.as_nanos() as u64;
        let discontinuity_ns = self.policy.discontinuity_threshold.as_nanos() as u64;

        if magnitude_ns >= discontinuity_ns {
            return AvFrameAction::Rebase {
                target_master_time: observation.video_presentation_time,
            };
        }

        if drift_ns > tolerance_ns as i64 {
            return AvFrameAction::Wait {
                duration: Duration::from_nanos(drift_ns as u64 - tolerance_ns),
            };
        }

        if drift_ns >= -(tolerance_ns as i64) {
            return AvFrameAction::Present {
                display_for: Duration::from_nanos(frame_duration_ns),
                correction: AvFrameCorrection::None,
                reason: AvPresentationReason::InTolerance,
            };
        }

        let reason = if magnitude_ns < drop_ns {
            AvPresentationReason::CorrectedLate
        } else if !observation.drop_eligible {
            AvPresentationReason::ProtectedFromDrop
        } else if !observation.following_frame_ready {
            AvPresentationReason::NoReadySuccessor
        } else if self.statistics.consecutive_drops >= self.policy.max_consecutive_drops {
            AvPresentationReason::StarvationGuard
        } else {
            return AvFrameAction::DropLate;
        };

        self.corrected_presentation(frame_duration_ns, magnitude_ns, tolerance_ns, reason)
    }

    fn corrected_presentation(
        &self,
        frame_duration_ns: u64,
        lateness_ns: u64,
        tolerance_ns: u64,
        reason: AvPresentationReason,
    ) -> AvFrameAction {
        let maximum_correction_ns = self.policy.maximum_correction.as_nanos() as u64;
        let correction_ns = lateness_ns
            .saturating_sub(tolerance_ns)
            .min(maximum_correction_ns)
            .min(frame_duration_ns);

        AvFrameAction::Present {
            display_for: Duration::from_nanos(frame_duration_ns - correction_ns),
            correction: if correction_ns == 0 {
                AvFrameCorrection::None
            } else {
                AvFrameCorrection::ShortenBy(Duration::from_nanos(correction_ns))
            },
            reason,
        }
    }
}

fn duration_nanoseconds(
    duration: Duration,
    operation: &'static str,
    field: &'static str,
) -> Result<u64> {
    let nanoseconds = u64::try_from(duration.as_nanos()).map_err(|_| {
        Error::new(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "A/V policy duration exceeds scheduler precision",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation)
                .with_field("field", field)
                .with_field("duration_seconds", duration.as_secs().to_string()),
        )
    })?;
    if nanoseconds > i64::MAX as u64 {
        return Err(Error::new(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "A/V policy duration exceeds signed scheduler precision",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation)
                .with_field("field", field)
                .with_field("duration_ns", nanoseconds.to_string()),
        ));
    }
    Ok(nanoseconds)
}

fn policy_error(message: &'static str, field: &'static str, value: u64) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new(COMPONENT, "validate_policy").with_field(field, value.to_string()),
    )
}
