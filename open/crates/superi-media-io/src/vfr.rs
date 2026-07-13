//! Exact variable-frame-rate presentation mapping.
//!
//! Compressed packets may arrive in decode order, while editor transport and
//! duration calculations operate in presentation order. This module builds one
//! contiguous presentation timeline from exact sample timestamps. It derives
//! each non-final frame duration from the following presentation timestamp and
//! requires either a duration or an exclusive end for the final frame.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::time::{Duration, RationalTime, TimeRange, Timebase};

use crate::demux::PacketTiming;

const COMPONENT: &str = "superi-media-io.vfr";

/// Container or decoder timing used to build a presentation map.
///
/// A missing timestamp remains explicit so malformed or incomplete streams are
/// rejected at the point where an exact presentation map is required. Decode
/// timestamps do not belong here because they never define editor frame order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentationSample {
    presentation_time: Option<RationalTime>,
    duration: Option<Duration>,
}

impl PresentationSample {
    /// Preserves the timing fields supplied by a media backend.
    #[must_use]
    pub const fn new(presentation_time: Option<RationalTime>, duration: Option<Duration>) -> Self {
        Self {
            presentation_time,
            duration,
        }
    }

    /// Returns the exact presentation timestamp when supplied.
    #[must_use]
    pub const fn presentation_time(self) -> Option<RationalTime> {
        self.presentation_time
    }

    /// Returns the backend-supplied duration when known.
    #[must_use]
    pub const fn duration(self) -> Option<Duration> {
        self.duration
    }
}

impl From<PacketTiming> for PresentationSample {
    fn from(timing: PacketTiming) -> Self {
        Self::new(timing.presentation_time(), timing.duration())
    }
}

/// One validated frame interval on a presentation timeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentationFrame {
    presentation_time: RationalTime,
    duration: Duration,
}

impl PresentationFrame {
    /// Creates one positive half-open frame interval in a single timebase.
    pub fn new(presentation_time: RationalTime, duration: Duration) -> Result<Self> {
        if duration.timebase() != presentation_time.timebase() {
            return Err(invalid(
                "create_presentation_frame",
                "frame timestamp and duration must use the same timebase",
            ));
        }
        if duration.is_zero() {
            return Err(invalid(
                "create_presentation_frame",
                "frame duration must be greater than zero",
            ));
        }
        presentation_time
            .value()
            .checked_add(duration.value() as i64)
            .ok_or_else(|| {
                invalid(
                    "create_presentation_frame",
                    "frame end exceeds the supported coordinate range",
                )
            })?;
        Ok(Self {
            presentation_time,
            duration,
        })
    }

    /// Returns the inclusive presentation timestamp.
    #[must_use]
    pub const fn presentation_time(self) -> RationalTime {
        self.presentation_time
    }

    /// Returns the exact positive display duration.
    #[must_use]
    pub const fn duration(self) -> Duration {
        self.duration
    }

    /// Returns the exclusive end of this frame interval.
    #[must_use]
    pub fn end_exclusive(self) -> RationalTime {
        let value = self
            .presentation_time
            .value()
            .checked_add(self.duration.value() as i64)
            .expect("validated presentation frame end");
        RationalTime::new(value, self.presentation_time.timebase())
    }
}

/// Exact presentation-order mapping for one contiguous video stream.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VariableFrameRateMap {
    frames: Vec<PresentationFrame>,
    presentation_range: TimeRange,
    presentation_end: RationalTime,
    variable_frame_rate: bool,
}

impl VariableFrameRateMap {
    /// Creates a map from frames already arranged in presentation order.
    ///
    /// Frame intervals must use one timebase, have positive duration, and meet
    /// exactly at half-open boundaries. Timestamp normalization and edit-list
    /// gaps are separate operations and must be resolved before construction.
    pub fn new(frames: Vec<PresentationFrame>) -> Result<Self> {
        let first = frames.first().copied().ok_or_else(|| {
            invalid(
                "create_variable_frame_rate_map",
                "a presentation map requires at least one frame",
            )
        })?;
        let timebase = first.presentation_time().timebase();
        for (index, frame) in frames.iter().copied().enumerate() {
            if frame.presentation_time().timebase() != timebase
                || frame.duration().timebase() != timebase
            {
                return Err(invalid_at(
                    "create_variable_frame_rate_map",
                    "all frames must use one presentation timebase",
                    index,
                ));
            }
        }
        for (index, pair) in frames.windows(2).enumerate() {
            if pair[0].end_exclusive() != pair[1].presentation_time() {
                return Err(invalid_at(
                    "create_variable_frame_rate_map",
                    "frame presentation intervals must be ordered and contiguous",
                    index + 1,
                ));
            }
        }

        let presentation_end = frames
            .last()
            .copied()
            .expect("nonempty presentation frames")
            .end_exclusive();
        let presentation_range =
            TimeRange::from_start_end(first.presentation_time(), presentation_end).map_err(
                |error| {
                    error.with_context(ErrorContext::new(
                        COMPONENT,
                        "calculate_presentation_duration",
                    ))
                },
            )?;
        let first_duration = first.duration();
        let variable_frame_rate = frames
            .iter()
            .skip(1)
            .any(|frame| frame.duration() != first_duration);

        Ok(Self {
            frames,
            presentation_range,
            presentation_end,
            variable_frame_rate,
        })
    }

    /// Builds a map from samples supplied in any decode order.
    ///
    /// Every sample requires a presentation timestamp. Non-final durations are
    /// derived from adjacent timestamps in presentation order and any supplied
    /// values must agree. The last sample requires its own explicit duration.
    pub fn from_samples(samples: impl IntoIterator<Item = PresentationSample>) -> Result<Self> {
        Self::from_samples_internal(samples, None)
    }

    /// Builds a map using an exclusive presentation end for the final sample.
    ///
    /// The boundary supplies a missing final duration and validates a supplied
    /// one. It must use the same timebase and occur after the final timestamp.
    pub fn from_samples_with_end(
        samples: impl IntoIterator<Item = PresentationSample>,
        presentation_end: RationalTime,
    ) -> Result<Self> {
        Self::from_samples_internal(samples, Some(presentation_end))
    }

    /// Builds a map directly from normalized backend packet timing.
    ///
    /// Decode timestamps remain available on [`PacketTiming`] but are
    /// deliberately excluded from presentation ordering and duration.
    pub fn from_packet_timings(timings: impl IntoIterator<Item = PacketTiming>) -> Result<Self> {
        Self::from_samples(timings.into_iter().map(PresentationSample::from))
    }

    /// Builds a packet-timing map with an exclusive final presentation end.
    pub fn from_packet_timings_with_end(
        timings: impl IntoIterator<Item = PacketTiming>,
        presentation_end: RationalTime,
    ) -> Result<Self> {
        Self::from_samples_with_end(
            timings.into_iter().map(PresentationSample::from),
            presentation_end,
        )
    }

    fn from_samples_internal(
        samples: impl IntoIterator<Item = PresentationSample>,
        presentation_end: Option<RationalTime>,
    ) -> Result<Self> {
        let mut samples: Vec<_> = samples.into_iter().collect();
        let first_time = samples
            .iter()
            .find_map(|sample| sample.presentation_time())
            .ok_or_else(|| {
                invalid(
                    "map_presentation_samples",
                    "every sample requires a presentation timestamp",
                )
            })?;
        let timebase = first_time.timebase();

        for (index, sample) in samples.iter().copied().enumerate() {
            let timestamp = sample.presentation_time().ok_or_else(|| {
                invalid_at(
                    "map_presentation_samples",
                    "every sample requires a presentation timestamp",
                    index,
                )
            })?;
            if timestamp.timebase() != timebase {
                return Err(invalid_at(
                    "map_presentation_samples",
                    "all samples must use one presentation timebase",
                    index,
                ));
            }
            if let Some(duration) = sample.duration() {
                if duration.timebase() != timebase {
                    return Err(invalid_at(
                        "map_presentation_samples",
                        "sample durations must use the presentation timebase",
                        index,
                    ));
                }
                if duration.is_zero() {
                    return Err(invalid_at(
                        "map_presentation_samples",
                        "sample duration must be greater than zero when supplied",
                        index,
                    ));
                }
            }
        }
        if let Some(end) = presentation_end {
            if end.timebase() != timebase {
                return Err(invalid(
                    "map_presentation_samples",
                    "presentation end must use the sample timebase",
                ));
            }
        }

        samples.sort_by_key(|sample| {
            sample
                .presentation_time()
                .expect("validated presentation timestamp")
                .value()
        });
        for (index, pair) in samples.windows(2).enumerate() {
            let left = pair[0]
                .presentation_time()
                .expect("validated presentation timestamp");
            let right = pair[1]
                .presentation_time()
                .expect("validated presentation timestamp");
            if left == right {
                return Err(invalid_at(
                    "map_presentation_samples",
                    "presentation timestamps must be unique",
                    index + 1,
                ));
            }
        }

        let mut frames = Vec::with_capacity(samples.len());
        for (index, sample) in samples.iter().copied().enumerate() {
            let start = sample
                .presentation_time()
                .expect("validated presentation timestamp");
            let end = match samples.get(index + 1) {
                Some(next) => next
                    .presentation_time()
                    .expect("validated presentation timestamp"),
                None => match presentation_end {
                    Some(end) => end,
                    None => {
                        let duration = sample.duration().ok_or_else(|| {
                            invalid_at(
                                "map_presentation_samples",
                                "final sample requires a duration or presentation end",
                                index,
                            )
                        })?;
                        let frame = PresentationFrame::new(start, duration)?;
                        frames.push(frame);
                        break;
                    }
                },
            };
            let delta = end.value().checked_sub(start.value()).ok_or_else(|| {
                invalid_at(
                    "map_presentation_samples",
                    "presentation timestamp delta exceeds the supported coordinate range",
                    index,
                )
            })?;
            let duration_value = u64::try_from(delta).map_err(|_| {
                invalid_at(
                    "map_presentation_samples",
                    "presentation timestamps must increase",
                    index,
                )
            })?;
            let duration = Duration::new(duration_value, timebase).map_err(|error| {
                error.with_context(
                    ErrorContext::new(COMPONENT, "calculate_frame_duration")
                        .with_field("presentation_index", index.to_string()),
                )
            })?;
            if duration.is_zero() {
                return Err(invalid_at(
                    "map_presentation_samples",
                    "presentation timestamps must increase",
                    index,
                ));
            }
            if sample
                .duration()
                .is_some_and(|supplied| supplied != duration)
            {
                return Err(invalid_at(
                    "map_presentation_samples",
                    "sample duration does not match its presentation interval",
                    index,
                ));
            }
            frames.push(PresentationFrame::new(start, duration)?);
        }
        Self::new(frames)
    }

    /// Returns the shared exact presentation timebase.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.presentation_range.timebase()
    }

    /// Returns the number of mapped presentation frames.
    #[must_use]
    pub fn frame_count(&self) -> u64 {
        u64::try_from(self.frames.len()).expect("frame count fits the public u64 representation")
    }

    /// Returns one frame by zero-based presentation index.
    #[must_use]
    pub fn frame(&self, index: u64) -> Option<PresentationFrame> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.frames.get(index))
            .copied()
    }

    /// Maps a presentation coordinate to its zero-based frame index.
    ///
    /// Frame intervals are half-open, so an exact boundary belongs to the next
    /// frame and the final exclusive end is outside the map.
    #[must_use]
    pub fn frame_index_at(&self, presentation_time: RationalTime) -> Option<u64> {
        if presentation_time < self.presentation_start()
            || presentation_time >= self.presentation_end
        {
            return None;
        }
        let insertion = self
            .frames
            .partition_point(|frame| frame.presentation_time() <= presentation_time);
        let index = insertion.checked_sub(1)?;
        u64::try_from(index).ok()
    }

    /// Returns the inclusive start of the presentation timeline.
    #[must_use]
    pub const fn presentation_start(&self) -> RationalTime {
        self.presentation_range.start()
    }

    /// Returns the exclusive end of the presentation timeline.
    #[must_use]
    pub const fn presentation_end(&self) -> RationalTime {
        self.presentation_end
    }

    /// Returns the exact total duration from first start to final end.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        self.presentation_range.duration()
    }

    /// Returns the complete half-open presentation range.
    #[must_use]
    pub const fn presentation_range(&self) -> TimeRange {
        self.presentation_range
    }

    /// Returns whether at least one frame duration differs from the first.
    #[must_use]
    pub const fn is_variable_frame_rate(&self) -> bool {
        self.variable_frame_rate
    }
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn invalid_at(operation: &'static str, message: &'static str, index: usize) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation).with_field("presentation_index", index.to_string()),
    )
}
