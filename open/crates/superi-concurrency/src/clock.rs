//! Playback and audio-master clock contracts.
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
//! source and clock re-anchor. A/V drift correction and frame-drop policy are
//! separate scheduler responsibilities.

use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result, ResultExt};
use superi_core::time::{RationalTime, SampleTime, TimeRounding, Timebase};

const COMPONENT: &str = "superi-concurrency.clock";

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
