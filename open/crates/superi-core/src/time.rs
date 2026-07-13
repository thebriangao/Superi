//! Exact shared time values for media, editorial, audio, and automation.
//!
//! Time is represented as signed integer coordinates in an explicit rational
//! timebase. Conversions never round implicitly, so frame and sample clocks can
//! remain synchronized without accumulating floating point drift.

use std::cmp::Ordering;
use std::fmt;

use crate::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

/// A reduced positive rational count of units per second.
///
/// A time coordinate with value `v` in this timebase represents
/// `v * denominator / numerator` seconds.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Timebase {
    numerator: u32,
    denominator: u32,
}

impl Timebase {
    /// One unit per second.
    pub const SECONDS: Self = Self::from_reduced(1, 1);
    /// One thousand units per second.
    pub const MILLISECONDS: Self = Self::from_reduced(1_000, 1);
    /// One million units per second.
    pub const MICROSECONDS: Self = Self::from_reduced(1_000_000, 1);
    /// One billion units per second.
    pub const NANOSECONDS: Self = Self::from_reduced(1_000_000_000, 1);

    /// Creates a positive rational timebase and reduces it to canonical form.
    pub fn new(units_per_second_numerator: u32, units_per_second_denominator: u32) -> Result<Self> {
        if units_per_second_numerator == 0 {
            return Err(invalid_time(
                "create_timebase",
                "timebase numerator must be greater than zero",
            ));
        }
        if units_per_second_denominator == 0 {
            return Err(invalid_time(
                "create_timebase",
                "timebase denominator must be greater than zero",
            ));
        }

        let divisor =
            greatest_common_divisor(units_per_second_numerator, units_per_second_denominator);
        Ok(Self::from_reduced(
            units_per_second_numerator / divisor,
            units_per_second_denominator / divisor,
        ))
    }

    /// Creates an integral number of units per second.
    pub fn integer(units_per_second: u32) -> Result<Self> {
        Self::new(units_per_second, 1)
    }

    const fn from_reduced(numerator: u32, denominator: u32) -> Self {
        Self {
            numerator,
            denominator,
        }
    }

    /// Returns the units-per-second numerator.
    #[must_use]
    pub const fn numerator(self) -> u32 {
        self.numerator
    }

    /// Returns the units-per-second denominator.
    #[must_use]
    pub const fn denominator(self) -> u32 {
        self.denominator
    }

    /// Returns true when the timebase has an integral number of units per second.
    #[must_use]
    pub const fn is_integral(self) -> bool {
        self.denominator == 1
    }
}

impl fmt::Display for Timebase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.denominator == 1 {
            write!(formatter, "{} units/s", self.numerator)
        } else {
            write!(formatter, "{}/{} units/s", self.numerator, self.denominator)
        }
    }
}

/// An exact positive rational video frame rate.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FrameRate(Timebase);

impl FrameRate {
    /// 24 frames per second.
    pub const FPS_24: Self = Self(Timebase::from_reduced(24, 1));
    /// 25 frames per second.
    pub const FPS_25: Self = Self(Timebase::from_reduced(25, 1));
    /// 30 frames per second.
    pub const FPS_30: Self = Self(Timebase::from_reduced(30, 1));
    /// 48 frames per second.
    pub const FPS_48: Self = Self(Timebase::from_reduced(48, 1));
    /// 50 frames per second.
    pub const FPS_50: Self = Self(Timebase::from_reduced(50, 1));
    /// 60 frames per second.
    pub const FPS_60: Self = Self(Timebase::from_reduced(60, 1));
    /// 24,000/1,001 frames per second.
    pub const FPS_24000_1001: Self = Self(Timebase::from_reduced(24_000, 1_001));
    /// 30,000/1,001 frames per second.
    pub const FPS_30000_1001: Self = Self(Timebase::from_reduced(30_000, 1_001));
    /// 60,000/1,001 frames per second.
    pub const FPS_60000_1001: Self = Self(Timebase::from_reduced(60_000, 1_001));

    /// Creates an exact rational frame rate.
    pub fn new(
        frames_per_second_numerator: u32,
        frames_per_second_denominator: u32,
    ) -> Result<Self> {
        Timebase::new(frames_per_second_numerator, frames_per_second_denominator).map(Self)
    }

    /// Creates a frame rate from an existing timebase.
    #[must_use]
    pub const fn from_timebase(timebase: Timebase) -> Self {
        Self(timebase)
    }

    /// Returns the exact frames-per-second numerator.
    #[must_use]
    pub const fn numerator(self) -> u32 {
        self.0.numerator()
    }

    /// Returns the exact frames-per-second denominator.
    #[must_use]
    pub const fn denominator(self) -> u32 {
        self.0.denominator()
    }

    /// Returns this frame rate as a general timebase.
    #[must_use]
    pub const fn timebase(self) -> Timebase {
        self.0
    }

    /// Returns the nearest integral counting rate used by timecode labels.
    ///
    /// This returns 24 for 24,000/1,001 and 30 for 30,000/1,001. It does not
    /// change the exact rate used for time conversion.
    #[must_use]
    pub fn nominal_fps(self) -> u32 {
        let numerator = u64::from(self.numerator());
        let denominator = u64::from(self.denominator());
        ((numerator + denominator / 2) / denominator) as u32
    }
}

impl fmt::Display for FrameRate {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.denominator() == 1 {
            write!(formatter, "{} fps", self.numerator())
        } else {
            write!(formatter, "{}/{} fps", self.numerator(), self.denominator())
        }
    }
}

/// The explicit rounding rule for conversion between discrete timebases.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum TimeRounding {
    /// Reject a conversion that cannot be represented exactly.
    Exact,
    /// Round toward negative infinity.
    Floor,
    /// Round toward positive infinity.
    Ceil,
    /// Round toward zero.
    TowardZero,
    /// Round to the nearest coordinate, resolving exact ties to an even value.
    NearestTiesEven,
}

/// A signed coordinate in an explicit rational timebase.
///
/// Equality and ordering compare physical time, not the stored coordinate pair.
/// For example, `24 @ 24 fps` equals `48 @ 48 fps`.
#[derive(Clone, Copy, Debug)]
pub struct RationalTime {
    value: i64,
    timebase: Timebase,
}

impl RationalTime {
    /// Creates a time coordinate in the supplied timebase.
    #[must_use]
    pub const fn new(value: i64, timebase: Timebase) -> Self {
        Self { value, timebase }
    }

    /// Creates the zero coordinate in the supplied timebase.
    #[must_use]
    pub const fn zero(timebase: Timebase) -> Self {
        Self::new(0, timebase)
    }

    /// Creates a coordinate from a frame number and exact frame rate.
    #[must_use]
    pub const fn from_frames(frame: i64, frame_rate: FrameRate) -> Self {
        Self::new(frame, frame_rate.timebase())
    }

    /// Creates a coordinate from a sample position.
    #[must_use]
    pub const fn from_sample_time(sample_time: SampleTime) -> Self {
        sample_time.rational_time()
    }

    /// Returns the stored coordinate.
    #[must_use]
    pub const fn value(self) -> i64 {
        self.value
    }

    /// Returns the coordinate's timebase.
    #[must_use]
    pub const fn timebase(self) -> Timebase {
        self.timebase
    }

    /// Returns true when this is the zero coordinate.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.value == 0
    }

    /// Returns true when this coordinate occurs before time zero.
    #[must_use]
    pub const fn is_negative(self) -> bool {
        self.value < 0
    }

    /// Converts the coordinate to another timebase using an explicit policy.
    pub fn checked_rescale(self, target: Timebase, rounding: TimeRounding) -> Result<Self> {
        if self.timebase == target {
            return Ok(self);
        }

        let numerator = i128::from(self.value)
            * i128::from(self.timebase.denominator())
            * i128::from(target.numerator());
        let denominator = i128::from(self.timebase.numerator()) * i128::from(target.denominator());
        let value = divide_time(numerator, denominator, rounding, "rescale_time")?;
        Ok(Self::new(value, target))
    }

    /// Converts this coordinate to a frame number at the requested frame rate.
    pub fn to_frames(self, frame_rate: FrameRate, rounding: TimeRounding) -> Result<i64> {
        self.checked_rescale(frame_rate.timebase(), rounding)
            .map(Self::value)
    }

    /// Adds another time value and returns a coordinate in `result_timebase`.
    pub fn checked_add_at(
        self,
        other: Self,
        result_timebase: Timebase,
        rounding: TimeRounding,
    ) -> Result<Self> {
        self.checked_binary_at(other, result_timebase, rounding, BinaryTimeOperation::Add)
    }

    /// Subtracts another time value and returns a coordinate in `result_timebase`.
    pub fn checked_sub_at(
        self,
        other: Self,
        result_timebase: Timebase,
        rounding: TimeRounding,
    ) -> Result<Self> {
        self.checked_binary_at(
            other,
            result_timebase,
            rounding,
            BinaryTimeOperation::Subtract,
        )
    }

    fn checked_binary_at(
        self,
        other: Self,
        result_timebase: Timebase,
        rounding: TimeRounding,
        operation: BinaryTimeOperation,
    ) -> Result<Self> {
        let (left_numerator, left_denominator) = self.units_at(result_timebase)?;
        let (right_numerator, right_denominator) = other.units_at(result_timebase)?;
        let common_divisor = greatest_common_divisor_i128(left_denominator, right_denominator);
        let left_multiplier = right_denominator / common_divisor;
        let right_multiplier = left_denominator / common_divisor;

        let left = left_numerator
            .checked_mul(left_multiplier)
            .ok_or_else(|| overflow_time(operation.name()))?;
        let right = right_numerator
            .checked_mul(right_multiplier)
            .ok_or_else(|| overflow_time(operation.name()))?;
        let numerator = match operation {
            BinaryTimeOperation::Add => left.checked_add(right),
            BinaryTimeOperation::Subtract => left.checked_sub(right),
        }
        .ok_or_else(|| overflow_time(operation.name()))?;
        let denominator = left_denominator
            .checked_mul(left_multiplier)
            .ok_or_else(|| overflow_time(operation.name()))?;
        let value = divide_time(numerator, denominator, rounding, operation.name())?;
        Ok(Self::new(value, result_timebase))
    }

    fn units_at(self, target: Timebase) -> Result<(i128, i128)> {
        let numerator = i128::from(self.value)
            .checked_mul(i128::from(self.timebase.denominator()))
            .and_then(|value| value.checked_mul(i128::from(target.numerator())))
            .ok_or_else(|| overflow_time("convert_time"))?;
        let denominator = i128::from(self.timebase.numerator())
            .checked_mul(i128::from(target.denominator()))
            .ok_or_else(|| overflow_time("convert_time"))?;
        let common_divisor = greatest_common_divisor_i128(numerator.abs(), denominator);
        Ok((numerator / common_divisor, denominator / common_divisor))
    }

    fn compare(self, other: Self) -> Ordering {
        let left = i128::from(self.value)
            * i128::from(self.timebase.denominator())
            * i128::from(other.timebase.numerator());
        let right = i128::from(other.value)
            * i128::from(other.timebase.denominator())
            * i128::from(self.timebase.numerator());
        left.cmp(&right)
    }
}

#[derive(Clone, Copy)]
enum BinaryTimeOperation {
    Add,
    Subtract,
}

impl BinaryTimeOperation {
    const fn name(self) -> &'static str {
        match self {
            Self::Add => "add_time",
            Self::Subtract => "subtract_time",
        }
    }
}

impl PartialEq for RationalTime {
    fn eq(&self, other: &Self) -> bool {
        self.compare(*other) == Ordering::Equal
    }
}

impl Eq for RationalTime {}

impl PartialOrd for RationalTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.compare(*other))
    }
}

impl fmt::Display for RationalTime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} @ {}", self.value, self.timebase)
    }
}

/// A signed audio sample position paired with its integral sample rate.
#[derive(Clone, Copy, Debug)]
pub struct SampleTime {
    sample: i64,
    sample_rate: u32,
}

impl PartialEq for SampleTime {
    fn eq(&self, other: &Self) -> bool {
        self.rational_time() == other.rational_time()
    }
}

impl Eq for SampleTime {}

impl PartialOrd for SampleTime {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.rational_time().partial_cmp(&other.rational_time())
    }
}

impl SampleTime {
    /// Creates an audio sample coordinate.
    pub fn new(sample: i64, sample_rate: u32) -> Result<Self> {
        if sample_rate == 0 {
            return Err(invalid_time(
                "create_sample_time",
                "sample rate must be greater than zero",
            ));
        }
        Ok(Self {
            sample,
            sample_rate,
        })
    }

    /// Returns the signed sample position.
    #[must_use]
    pub const fn sample(self) -> i64 {
        self.sample
    }

    /// Returns the integral number of samples per second.
    #[must_use]
    pub const fn sample_rate(self) -> u32 {
        self.sample_rate
    }

    /// Returns this sample clock as a general timebase.
    #[must_use]
    pub const fn timebase(self) -> Timebase {
        Timebase::from_reduced(self.sample_rate, 1)
    }

    /// Returns the exact rational time represented by this sample position.
    #[must_use]
    pub const fn rational_time(self) -> RationalTime {
        RationalTime::new(self.sample, self.timebase())
    }

    /// Converts this position to another integral sample rate.
    pub fn checked_resample(self, target_sample_rate: u32, rounding: TimeRounding) -> Result<Self> {
        let target = Timebase::integer(target_sample_rate)?;
        let value = self
            .rational_time()
            .checked_rescale(target, rounding)?
            .value();
        Self::new(value, target_sample_rate)
    }
}

impl fmt::Display for SampleTime {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} samples @ {} Hz",
            self.sample, self.sample_rate
        )
    }
}

/// A nonnegative amount of time in an explicit rational timebase.
#[derive(Clone, Copy, Debug)]
pub struct Duration {
    value: u64,
    timebase: Timebase,
}

impl PartialEq for Duration {
    fn eq(&self, other: &Self) -> bool {
        self.rational_time() == other.rational_time()
    }
}

impl Eq for Duration {}

impl PartialOrd for Duration {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.rational_time().partial_cmp(&other.rational_time())
    }
}

impl Duration {
    /// Creates a duration.
    ///
    /// Values above `i64::MAX` are rejected so all time arithmetic uses one
    /// checked signed coordinate domain.
    pub fn new(value: u64, timebase: Timebase) -> Result<Self> {
        if value > i64::MAX as u64 {
            return Err(overflow_time("create_duration"));
        }
        Ok(Self { value, timebase })
    }

    /// Creates a zero duration in the supplied timebase.
    #[must_use]
    pub const fn zero(timebase: Timebase) -> Self {
        Self { value: 0, timebase }
    }

    /// Creates a frame duration.
    pub fn from_frames(frame_count: u64, frame_rate: FrameRate) -> Result<Self> {
        Self::new(frame_count, frame_rate.timebase())
    }

    /// Creates an audio sample duration.
    pub fn from_samples(sample_count: u64, sample_rate: u32) -> Result<Self> {
        let timebase = Timebase::integer(sample_rate)?;
        Self::new(sample_count, timebase)
    }

    /// Returns the stored nonnegative coordinate.
    #[must_use]
    pub const fn value(self) -> u64 {
        self.value
    }

    /// Returns the duration's timebase.
    #[must_use]
    pub const fn timebase(self) -> Timebase {
        self.timebase
    }

    /// Returns true when this duration is empty.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.value == 0
    }

    /// Returns this duration as a nonnegative rational time.
    #[must_use]
    pub const fn rational_time(self) -> RationalTime {
        RationalTime::new(self.value as i64, self.timebase)
    }

    /// Converts the duration to another timebase using an explicit policy.
    pub fn checked_rescale(self, target: Timebase, rounding: TimeRounding) -> Result<Self> {
        let converted = self.rational_time().checked_rescale(target, rounding)?;
        let value = u64::try_from(converted.value())
            .map_err(|_| invalid_time("rescale_duration", "duration became negative"))?;
        Self::new(value, target)
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} @ {}", self.value, self.timebase)
    }
}

/// A half-open time interval represented by a start and duration.
///
/// The interval contains its start and excludes its computed end. Start and
/// duration must use the same timebase.
#[derive(Clone, Copy, Debug)]
pub struct TimeRange {
    start: RationalTime,
    duration: Duration,
}

impl PartialEq for TimeRange {
    fn eq(&self, other: &Self) -> bool {
        self.start == other.start && self.duration == other.duration
    }
}

impl Eq for TimeRange {}

impl TimeRange {
    /// Creates a half-open range from a start and duration in one timebase.
    pub fn new(start: RationalTime, duration: Duration) -> Result<Self> {
        if start.timebase() != duration.timebase() {
            return Err(invalid_time(
                "create_time_range",
                "range start and duration must use the same timebase",
            ));
        }
        start
            .value()
            .checked_add(duration.value() as i64)
            .ok_or_else(|| overflow_time("create_time_range"))?;
        Ok(Self { start, duration })
    }

    /// Creates a range from inclusive start and exclusive end coordinates.
    pub fn from_start_end(start: RationalTime, end_exclusive: RationalTime) -> Result<Self> {
        if start.timebase() != end_exclusive.timebase() {
            return Err(invalid_time(
                "create_time_range_from_end",
                "range endpoints must use the same timebase",
            ));
        }
        let value = end_exclusive
            .value()
            .checked_sub(start.value())
            .ok_or_else(|| overflow_time("create_time_range_from_end"))?;
        let value = u64::try_from(value).map_err(|_| {
            invalid_time(
                "create_time_range_from_end",
                "range end must not occur before its start",
            )
        })?;
        Self::new(start, Duration::new(value, start.timebase())?)
    }

    /// Returns the inclusive start coordinate.
    #[must_use]
    pub const fn start(self) -> RationalTime {
        self.start
    }

    /// Returns the nonnegative range duration.
    #[must_use]
    pub const fn duration(self) -> Duration {
        self.duration
    }

    /// Returns the shared timebase for the range.
    #[must_use]
    pub const fn timebase(self) -> Timebase {
        self.start.timebase()
    }

    /// Returns true when this range contains no time coordinates.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.duration.is_zero()
    }

    /// Returns the exclusive end coordinate.
    pub fn end_exclusive(self) -> Result<RationalTime> {
        let value = self
            .start
            .value()
            .checked_add(self.duration.value() as i64)
            .ok_or_else(|| overflow_time("time_range_end"))?;
        Ok(RationalTime::new(value, self.timebase()))
    }

    /// Returns true when `time` lies inside the half-open interval.
    pub fn contains(self, time: RationalTime) -> Result<bool> {
        let end = self.end_exclusive()?;
        Ok(time >= self.start && time < end)
    }

    /// Returns true when this range and `other` share at least one coordinate.
    pub fn intersects(self, other: Self) -> Result<bool> {
        if self.is_empty() || other.is_empty() {
            return Ok(false);
        }
        let self_end = self.end_exclusive()?;
        let other_end = other.end_exclusive()?;
        Ok(self.start < other_end && other.start < self_end)
    }

    /// Converts the complete range to another timebase.
    pub fn checked_rescale(self, target: Timebase, rounding: TimeRounding) -> Result<Self> {
        let start = self.start.checked_rescale(target, rounding)?;
        let end = self.end_exclusive()?.checked_rescale(target, rounding)?;
        Self::from_start_end(start, end)
    }
}

fn greatest_common_divisor(mut left: u32, mut right: u32) -> u32 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn greatest_common_divisor_i128(mut left: i128, mut right: i128) -> i128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn divide_time(
    numerator: i128,
    denominator: i128,
    rounding: TimeRounding,
    operation: &'static str,
) -> Result<i64> {
    debug_assert!(denominator > 0);
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;

    let rounded = match rounding {
        TimeRounding::Exact => {
            if remainder != 0 {
                return Err(invalid_time(
                    operation,
                    "time conversion is not exact at the requested timebase",
                ));
            }
            quotient
        }
        TimeRounding::Floor if remainder < 0 => quotient - 1,
        TimeRounding::Floor => quotient,
        TimeRounding::Ceil if remainder > 0 => quotient + 1,
        TimeRounding::Ceil => quotient,
        TimeRounding::TowardZero => quotient,
        TimeRounding::NearestTiesEven => {
            let doubled_remainder = remainder.abs() * 2;
            match doubled_remainder.cmp(&denominator) {
                Ordering::Less => quotient,
                Ordering::Greater => quotient + numerator.signum(),
                Ordering::Equal if quotient % 2 == 0 => quotient,
                Ordering::Equal => quotient + numerator.signum(),
            }
        }
    };

    i64::try_from(rounded).map_err(|_| overflow_time(operation))
}

fn invalid_time(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-core.time", operation))
}

fn overflow_time(operation: &'static str) -> Error {
    invalid_time(
        operation,
        "time value exceeds the supported coordinate range",
    )
}
