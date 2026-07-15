//! Exact clip-local playback timing and deterministic transport queries.

use std::cmp::Ordering;
use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::time::{Duration, RationalTime, TimeRange, TimeRounding, Timebase};

/// A reduced signed ratio of sampled source time to elapsed record time.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct PlaybackRate {
    numerator: i64,
    denominator: u64,
}

impl PlaybackRate {
    /// Normal forward playback.
    pub const NORMAL: Self = Self {
        numerator: 1,
        denominator: 1,
    };

    /// A held source sample.
    pub const FREEZE: Self = Self {
        numerator: 0,
        denominator: 1,
    };

    /// Normal reverse playback.
    pub const REVERSE: Self = Self {
        numerator: -1,
        denominator: 1,
    };

    /// Creates a reduced signed playback ratio.
    pub fn new(numerator: i64, denominator: u64) -> Result<Self> {
        if denominator == 0 {
            return Err(invalid_retime(
                "create_playback_rate",
                "playback rate denominator must be greater than zero",
            ));
        }
        if numerator == 0 {
            return Ok(Self::FREEZE);
        }
        let divisor = greatest_common_divisor(numerator.unsigned_abs(), denominator);
        let magnitude = numerator.unsigned_abs() / divisor;
        let magnitude = i64::try_from(magnitude).map_err(|_| {
            invalid_retime(
                "create_playback_rate",
                "playback rate exceeds the supported signed ratio range",
            )
        })?;
        Ok(Self {
            numerator: if numerator.is_negative() {
                -magnitude
            } else {
                magnitude
            },
            denominator: denominator / divisor,
        })
    }

    /// Returns the signed source-time numerator.
    #[must_use]
    pub const fn numerator(self) -> i64 {
        self.numerator
    }

    /// Returns the positive record-time denominator.
    #[must_use]
    pub const fn denominator(self) -> u64 {
        self.denominator
    }

    /// Returns true for a held source sample.
    #[must_use]
    pub const fn is_freeze(self) -> bool {
        self.numerator == 0
    }

    /// Returns true for reverse playback.
    #[must_use]
    pub const fn is_reverse(self) -> bool {
        self.numerator < 0
    }
}

/// One directly inspectable linear section of a clip time map.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RetimeSegment {
    record_range: TimeRange,
    source_start: RationalTime,
    rate: PlaybackRate,
}

impl RetimeSegment {
    /// Creates one nonempty clip-local linear timing section.
    pub fn new(
        record_range: TimeRange,
        source_start: RationalTime,
        rate: PlaybackRate,
    ) -> Result<Self> {
        if record_range.is_empty() {
            return Err(invalid_retime(
                "create_retime_segment",
                "retime segment record range must be nonempty",
            ));
        }
        if record_range.start().is_negative() {
            return Err(invalid_retime(
                "create_retime_segment",
                "retime segment record range must not start before clip-local zero",
            ));
        }
        let segment = Self {
            record_range,
            source_start,
            rate,
        };
        segment.source_time_at_offset(
            record_range.duration().rational_time(),
            TimeRounding::Exact,
            "create_retime_segment",
        )?;
        Ok(segment)
    }

    /// Returns this section in clip-local record coordinates.
    #[must_use]
    pub const fn record_range(&self) -> TimeRange {
        self.record_range
    }

    /// Returns the source sample selected at the section start.
    #[must_use]
    pub const fn source_start(&self) -> RationalTime {
        self.source_start
    }

    /// Returns the exact signed playback ratio.
    #[must_use]
    pub const fn rate(&self) -> PlaybackRate {
        self.rate
    }

    fn source_time_at(
        &self,
        record_time: RationalTime,
        rounding: TimeRounding,
        operation: &'static str,
    ) -> Result<(RationalTime, bool)> {
        let offset = record_time.checked_sub_at(
            self.record_range.start(),
            self.record_range.timebase(),
            TimeRounding::Exact,
        )?;
        self.source_time_at_offset(offset, rounding, operation)
    }

    fn source_time_at_offset(
        &self,
        offset: RationalTime,
        rounding: TimeRounding,
        operation: &'static str,
    ) -> Result<(RationalTime, bool)> {
        if self.rate.is_freeze() {
            return Ok((self.source_start, true));
        }
        let record_timebase = offset.timebase();
        let source_timebase = self.source_start.timebase();
        let numerator = i128::from(offset.value())
            .checked_mul(i128::from(record_timebase.denominator()))
            .and_then(|value| value.checked_mul(i128::from(self.rate.numerator)))
            .and_then(|value| value.checked_mul(i128::from(source_timebase.numerator())))
            .ok_or_else(|| overflow_retime(operation))?;
        let denominator = i128::from(record_timebase.numerator())
            .checked_mul(i128::from(self.rate.denominator))
            .and_then(|value| value.checked_mul(i128::from(source_timebase.denominator())))
            .ok_or_else(|| overflow_retime(operation))?;
        let (delta, exact) = divide_retime(numerator, denominator, rounding, operation)?;
        let value = self
            .source_start
            .value()
            .checked_add(delta)
            .ok_or_else(|| overflow_retime(operation))?;
        Ok((RationalTime::new(value, source_timebase), exact))
    }

    fn source_end(&self) -> Result<RationalTime> {
        self.source_time_at_offset(
            self.record_range.duration().rational_time(),
            TimeRounding::Exact,
            "validate_retime_continuity",
        )
        .map(|(time, _)| time)
    }
}

/// The directly inspectable high-level shape of one clip time map.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RetimeMode {
    /// One normal forward section.
    Identity,
    /// One non-unit forward section.
    SpeedChange,
    /// One reverse section.
    Reverse,
    /// One held source sample.
    Freeze,
    /// Multiple continuous sections.
    TimeRemap,
}

/// How a discrete source coordinate was resolved for transport.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RetimeResolution {
    /// The requested mapping landed exactly on the source clock.
    Exact,
    /// A freeze section intentionally held one source sample.
    Held,
    /// The caller explicitly selected this quantization policy.
    Rounded(TimeRounding),
}

/// One resolved source coordinate with its visible timing behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MappedSourceTime {
    time: RationalTime,
    resolution: RetimeResolution,
    segment_index: usize,
}

impl MappedSourceTime {
    /// Returns the source coordinate selected for playback.
    #[must_use]
    pub const fn time(self) -> RationalTime {
        self.time
    }

    /// Returns exact, held, or explicitly rounded behavior.
    #[must_use]
    pub const fn resolution(self) -> RetimeResolution {
        self.resolution
    }

    /// Returns the zero-based directly inspectable segment index.
    #[must_use]
    pub const fn segment_index(self) -> usize {
        self.segment_index
    }
}

/// An immutable complete record-to-source map for one clip.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipTimeMap {
    record_duration: Duration,
    source_timebase: Timebase,
    segments: Arc<[RetimeSegment]>,
}

impl ClipTimeMap {
    /// Creates a complete continuous piecewise-linear clip time map.
    pub fn new<I>(record_duration: Duration, source_timebase: Timebase, segments: I) -> Result<Self>
    where
        I: IntoIterator<Item = RetimeSegment>,
    {
        if record_duration.is_zero() {
            return Err(invalid_retime(
                "create_clip_time_map",
                "clip time map duration must be nonzero",
            ));
        }
        let segments: Vec<_> = segments.into_iter().collect();
        if segments.is_empty() {
            return Err(invalid_retime(
                "create_clip_time_map",
                "clip time map must contain at least one segment",
            ));
        }
        let mut expected_start = RationalTime::zero(record_duration.timebase());
        for (index, segment) in segments.iter().enumerate() {
            if segment.record_range.timebase() != record_duration.timebase() {
                return Err(invalid_retime(
                    "create_clip_time_map",
                    "every retime segment must use the clip record clock",
                ));
            }
            if segment.source_start.timebase() != source_timebase {
                return Err(invalid_retime(
                    "create_clip_time_map",
                    "every retime segment must use the clip source clock",
                ));
            }
            if segment.record_range.start() != expected_start {
                return Err(invalid_retime(
                    "create_clip_time_map",
                    "retime segments must provide contiguous gapless record coverage",
                ));
            }
            expected_start = segment.record_range.end_exclusive()?;
            if let Some(next) = segments.get(index + 1) {
                if segment.source_end()? != next.source_start {
                    return Err(invalid_retime(
                        "create_clip_time_map",
                        "retime segment source mapping must remain continuous at every seam",
                    ));
                }
            }
        }
        let expected_end = RationalTime::new(
            i64::try_from(record_duration.value())
                .map_err(|_| overflow_retime("create_clip_time_map"))?,
            record_duration.timebase(),
        );
        if expected_start != expected_end {
            return Err(invalid_retime(
                "create_clip_time_map",
                "retime segments must exactly cover the complete clip duration",
            ));
        }
        Ok(Self {
            record_duration,
            source_timebase,
            segments: Arc::from(segments),
        })
    }

    /// Creates one constant speed, reverse, or freeze section.
    pub fn speed(
        record_duration: Duration,
        source_start: RationalTime,
        rate: PlaybackRate,
    ) -> Result<Self> {
        let record_range = TimeRange::new(
            RationalTime::zero(record_duration.timebase()),
            record_duration,
        )?;
        let segment = RetimeSegment::new(record_range, source_start, rate)?;
        Self::new(record_duration, source_start.timebase(), [segment])
    }

    /// Creates a held source sample for the complete clip duration.
    pub fn freeze(record_duration: Duration, source_time: RationalTime) -> Result<Self> {
        Self::speed(record_duration, source_time, PlaybackRate::FREEZE)
    }

    /// Creates normal reverse playback from an explicit first source sample.
    pub fn reverse(record_duration: Duration, first_source_sample: RationalTime) -> Result<Self> {
        Self::speed(record_duration, first_source_sample, PlaybackRate::REVERSE)
    }

    /// Returns the clip-local record duration.
    #[must_use]
    pub const fn record_duration(&self) -> Duration {
        self.record_duration
    }

    /// Returns the exact source clock produced by every section.
    #[must_use]
    pub const fn source_timebase(&self) -> Timebase {
        self.source_timebase
    }

    /// Returns every section in clip-local record order.
    #[must_use]
    pub fn segments(&self) -> &[RetimeSegment] {
        &self.segments
    }

    /// Classifies the directly editable timing shape.
    #[must_use]
    pub fn mode(&self) -> RetimeMode {
        if self.segments.len() > 1 {
            return RetimeMode::TimeRemap;
        }
        let rate = self.segments[0].rate;
        if rate.is_freeze() {
            RetimeMode::Freeze
        } else if rate.is_reverse() {
            RetimeMode::Reverse
        } else if rate == PlaybackRate::NORMAL {
            RetimeMode::Identity
        } else {
            RetimeMode::SpeedChange
        }
    }

    /// Resolves one clip-local record coordinate with no allocation.
    pub fn source_time_at(
        &self,
        record_time: RationalTime,
        rounding: TimeRounding,
    ) -> Result<MappedSourceTime> {
        let complete = TimeRange::new(
            RationalTime::zero(self.record_duration.timebase()),
            self.record_duration,
        )?;
        if !complete.contains(record_time)? {
            return Err(invalid_retime(
                "resolve_retime_source",
                "retime query must lie inside the half-open clip record duration",
            ));
        }
        let index = self
            .segments
            .partition_point(|segment| segment.record_range.start() <= record_time)
            .saturating_sub(1);
        let segment = &self.segments[index];
        if !segment.record_range.contains(record_time)? {
            return Err(invalid_retime(
                "resolve_retime_source",
                "retime query did not resolve to complete segment coverage",
            ));
        }
        let (time, exact) =
            segment.source_time_at(record_time, rounding, "resolve_retime_source")?;
        let resolution = if segment.rate.is_freeze() {
            RetimeResolution::Held
        } else if exact {
            RetimeResolution::Exact
        } else {
            RetimeResolution::Rounded(rounding)
        };
        Ok(MappedSourceTime {
            time,
            resolution,
            segment_index: index,
        })
    }

    pub(crate) fn identity(record_duration: Duration, source_start: RationalTime) -> Result<Self> {
        Self::speed(record_duration, source_start, PlaybackRate::NORMAL)
    }

    pub(crate) fn validate_binding(
        &self,
        record_duration: Duration,
        source_timebase: Timebase,
    ) -> Result<()> {
        if self.record_duration != record_duration
            || self.record_duration.timebase() != record_duration.timebase()
        {
            return Err(invalid_retime(
                "bind_clip_time_map",
                "clip time map must use and exactly cover the clip record duration",
            ));
        }
        if self.source_timebase != source_timebase {
            return Err(invalid_retime(
                "bind_clip_time_map",
                "clip time map must use the clip source clock",
            ));
        }
        Ok(())
    }

    pub(crate) fn slice(&self, range: TimeRange) -> Result<Self> {
        let complete = TimeRange::new(
            RationalTime::zero(self.record_duration.timebase()),
            self.record_duration,
        )?;
        if range.is_empty()
            || range.timebase() != complete.timebase()
            || range.start() < complete.start()
            || range.end_exclusive()? > complete.end_exclusive()?
        {
            return Err(invalid_retime(
                "slice_clip_time_map",
                "retime slice must be a nonempty subrange of the clip duration",
            ));
        }
        let range_end = range.end_exclusive()?;
        let mut segments = Vec::new();
        for segment in self.segments.iter() {
            let segment_end = segment.record_range.end_exclusive()?;
            let start = if segment.record_range.start() > range.start() {
                segment.record_range.start()
            } else {
                range.start()
            };
            let end = if segment_end < range_end {
                segment_end
            } else {
                range_end
            };
            if start >= end {
                continue;
            }
            let source_start = segment
                .source_time_at(start, TimeRounding::Exact, "slice_clip_time_map")?
                .0;
            let rebased_start =
                start.checked_sub_at(range.start(), range.timebase(), TimeRounding::Exact)?;
            let rebased_end =
                end.checked_sub_at(range.start(), range.timebase(), TimeRounding::Exact)?;
            segments.push(RetimeSegment::new(
                TimeRange::from_start_end(rebased_start, rebased_end)?,
                source_start,
                segment.rate,
            )?);
        }
        Self::new(range.duration(), self.source_timebase, segments)
    }

    pub(crate) fn rescale_record_clock(&self, duration: Duration) -> Result<Self> {
        if self.record_duration != duration {
            return Err(invalid_retime(
                "rescale_clip_time_map",
                "record clock replacement must preserve the clip duration",
            ));
        }
        let mut segments = Vec::with_capacity(self.segments.len());
        for segment in self.segments.iter() {
            segments.push(RetimeSegment::new(
                segment
                    .record_range
                    .checked_rescale(duration.timebase(), TimeRounding::Exact)?,
                segment.source_start,
                segment.rate,
            )?);
        }
        Self::new(duration, self.source_timebase, segments)
    }

    pub(crate) fn translate_source(
        &self,
        old_anchor: RationalTime,
        new_anchor: RationalTime,
    ) -> Result<Self> {
        if old_anchor.timebase() != self.source_timebase {
            return Err(invalid_retime(
                "translate_clip_time_map",
                "old source anchor must use the current retime source clock",
            ));
        }
        let mut segments = Vec::with_capacity(self.segments.len());
        for segment in self.segments.iter() {
            let offset = segment.source_start.checked_sub_at(
                old_anchor,
                old_anchor.timebase(),
                TimeRounding::Exact,
            )?;
            let offset = offset.checked_rescale(new_anchor.timebase(), TimeRounding::Exact)?;
            let source_start =
                new_anchor.checked_add_at(offset, new_anchor.timebase(), TimeRounding::Exact)?;
            segments.push(RetimeSegment::new(
                segment.record_range,
                source_start,
                segment.rate,
            )?);
        }
        Self::new(self.record_duration, new_anchor.timebase(), segments)
    }
}

fn divide_retime(
    numerator: i128,
    denominator: i128,
    rounding: TimeRounding,
    operation: &'static str,
) -> Result<(i64, bool)> {
    debug_assert!(denominator > 0);
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    if remainder == 0 {
        return i64::try_from(quotient)
            .map(|value| (value, true))
            .map_err(|_| overflow_retime(operation));
    }
    let rounded = match rounding {
        TimeRounding::Exact => {
            return Err(invalid_retime(
                operation,
                "retime mapping is not exact at the requested source clock",
            ));
        }
        TimeRounding::Floor if remainder < 0 => quotient - 1,
        TimeRounding::Floor => quotient,
        TimeRounding::Ceil if remainder > 0 => quotient + 1,
        TimeRounding::Ceil | TimeRounding::TowardZero => quotient,
        TimeRounding::NearestTiesEven => match (remainder.abs() * 2).cmp(&denominator) {
            Ordering::Less => quotient,
            Ordering::Greater => quotient + numerator.signum(),
            Ordering::Equal if quotient % 2 == 0 => quotient,
            Ordering::Equal => quotient + numerator.signum(),
        },
        _ => {
            return Err(invalid_retime(
                operation,
                "retime mapping received an unsupported rounding policy",
            ));
        }
    };
    i64::try_from(rounded)
        .map(|value| (value, false))
        .map_err(|_| overflow_retime(operation))
}

fn greatest_common_divisor(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn invalid_retime(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-timeline.retime", operation))
}

fn overflow_retime(operation: &'static str) -> Error {
    invalid_retime(
        operation,
        "retime value exceeds the supported exact coordinate range",
    )
}
