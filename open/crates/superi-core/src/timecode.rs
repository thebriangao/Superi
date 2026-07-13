//! Frame-accurate editorial timecode.
//!
//! A timecode stores a continuous signed frame index and an explicit format.
//! Drop-frame mode omits labels, never media frames. This representation keeps
//! arithmetic exact and makes parsing and formatting deterministic.
//!
//! ```
//! use superi_core::time::FrameRate;
//! use superi_core::timecode::{Timecode, TimecodeFormat};
//!
//! let format = TimecodeFormat::drop_frame(FrameRate::FPS_30000_1001)?;
//! let last = Timecode::parse("00:00:59;29", format)?;
//! let next = last.checked_add_frames(1)?;
//! assert_eq!(next.to_string(), "00:01:00;02");
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::error::Error as StdError;
use std::fmt;

use crate::error::Error;
use crate::time::{FrameRate, RationalTime, TimeRounding};

/// The counting convention used by an editorial timecode label.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum TimecodeMode {
    /// Every nominal frame label is present.
    NonDropFrame,
    /// Selected labels are omitted to keep NTSC-related counts close to clock time.
    DropFrame,
}

impl TimecodeMode {
    /// Returns the canonical separator used before the frame field.
    #[must_use]
    pub const fn separator(self) -> char {
        match self {
            Self::NonDropFrame => ':',
            Self::DropFrame => ';',
        }
    }
}

/// An exact frame rate paired with a timecode counting convention.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct TimecodeFormat {
    rate: FrameRate,
    mode: TimecodeMode,
    drop_frames_per_minute: u32,
}

impl TimecodeFormat {
    /// Creates a non-drop-frame format for any valid frame rate.
    #[must_use]
    pub const fn non_drop(rate: FrameRate) -> Self {
        Self {
            rate,
            mode: TimecodeMode::NonDropFrame,
            drop_frames_per_minute: 0,
        }
    }

    /// Creates a drop-frame format for an exact NTSC-related rate.
    ///
    /// Supported rates have denominator 1001 and numerator nominal_fps * 1000,
    /// where the nominal rate is a multiple of 30.
    pub fn drop_frame(rate: FrameRate) -> Result<Self, TimecodeError> {
        let nominal_fps = rate.nominal_fps();
        let expected_numerator = nominal_fps.checked_mul(1_000);
        let supported = rate.denominator() == 1_001
            && expected_numerator == Some(rate.numerator())
            && nominal_fps % 30 == 0;

        if !supported {
            return Err(TimecodeError::UnsupportedDropFrameRate { rate });
        }

        Ok(Self {
            rate,
            mode: TimecodeMode::DropFrame,
            drop_frames_per_minute: nominal_fps / 15,
        })
    }

    /// Returns the exact physical frame rate.
    #[must_use]
    pub const fn rate(self) -> FrameRate {
        self.rate
    }

    /// Returns the counting convention.
    #[must_use]
    pub const fn mode(self) -> TimecodeMode {
        self.mode
    }

    /// Returns the number of labels omitted at each affected minute.
    ///
    /// Non-drop-frame formats return zero.
    #[must_use]
    pub const fn drop_frames_per_minute(self) -> u32 {
        self.drop_frames_per_minute
    }

    fn nominal_fps(self) -> u32 {
        self.rate.nominal_fps().max(1)
    }
}

/// A component within an HH:MM:SS:FF timecode label.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum TimecodeComponent {
    /// Hour component.
    Hours,
    /// Minute component.
    Minutes,
    /// Second component.
    Seconds,
    /// Nominal frame component.
    Frames,
}

impl fmt::Display for TimecodeComponent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Hours => "hours",
            Self::Minutes => "minutes",
            Self::Seconds => "seconds",
            Self::Frames => "frames",
        })
    }
}

/// A strict parsing, arithmetic, or conversion failure.
#[derive(Debug)]
#[non_exhaustive]
pub enum TimecodeError {
    /// The label does not have the canonical signed HH:MM:SS:FF shape.
    InvalidSyntax,
    /// The last separator does not match the explicit format.
    SeparatorMismatch {
        /// Separator required by the format.
        expected: char,
        /// Separator present in the label.
        found: char,
    },
    /// A numeric component is outside its allowed range.
    ComponentOutOfRange {
        /// Component that failed validation.
        component: TimecodeComponent,
        /// Parsed value.
        value: u64,
        /// Largest accepted value.
        maximum: u64,
    },
    /// The label names a frame number omitted by drop-frame counting.
    DroppedFrameLabel {
        /// Minute within the hour.
        minute: u8,
        /// Omitted nominal frame label.
        frame: u32,
    },
    /// Drop-frame counting is not defined for the supplied exact frame rate.
    UnsupportedDropFrameRate {
        /// Rejected frame rate.
        rate: FrameRate,
    },
    /// Arithmetic combined values with different formats.
    FormatMismatch {
        /// Format of the left-hand value.
        left: TimecodeFormat,
        /// Format of the right-hand value.
        right: TimecodeFormat,
    },
    /// A checked label or frame calculation exceeded its representation.
    ArithmeticOverflow,
    /// A negative zero label cannot be represented canonically.
    NegativeZero,
    /// Exact rational-time conversion or its selected rounding policy failed.
    Time(Error),
}

impl TimecodeError {
    /// Returns a stable diagnostic code for this failure.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidSyntax => "invalid_syntax",
            Self::SeparatorMismatch { .. } => "separator_mismatch",
            Self::ComponentOutOfRange { .. } => "component_out_of_range",
            Self::DroppedFrameLabel { .. } => "dropped_frame_label",
            Self::UnsupportedDropFrameRate { .. } => "unsupported_drop_frame_rate",
            Self::FormatMismatch { .. } => "format_mismatch",
            Self::ArithmeticOverflow => "arithmetic_overflow",
            Self::NegativeZero => "negative_zero",
            Self::Time(_) => "time_conversion",
        }
    }
}

impl fmt::Display for TimecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSyntax => formatter.write_str("invalid timecode syntax"),
            Self::SeparatorMismatch { expected, found } => write!(
                formatter,
                "timecode separator {found:?} does not match expected {expected:?}"
            ),
            Self::ComponentOutOfRange {
                component,
                value,
                maximum,
            } => write!(
                formatter,
                "timecode {component} value {value} exceeds maximum {maximum}"
            ),
            Self::DroppedFrameLabel { minute, frame } => write!(
                formatter,
                "timecode frame label {frame:02} is omitted at minute {minute:02}"
            ),
            Self::UnsupportedDropFrameRate { rate } => write!(
                formatter,
                "drop-frame counting is unsupported for {}/{} fps",
                rate.numerator(),
                rate.denominator()
            ),
            Self::FormatMismatch { left, right } => {
                write!(
                    formatter,
                    "timecode format mismatch: {left:?} and {right:?}"
                )
            }
            Self::ArithmeticOverflow => formatter.write_str("timecode arithmetic overflow"),
            Self::NegativeZero => formatter.write_str("negative zero timecode is not canonical"),
            Self::Time(error) => write!(formatter, "timecode conversion failed: {error}"),
        }
    }
}

impl StdError for TimecodeError {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        match self {
            Self::Time(error) => Some(error),
            _ => None,
        }
    }
}

impl From<Error> for TimecodeError {
    fn from(error: Error) -> Self {
        Self::Time(error)
    }
}

/// A continuous signed frame position with an explicit display format.
///
/// Labels are projections of the stored frame index. The value never wraps at
/// 24 hours, preserving long-form editorial positions and negative offsets.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Timecode {
    frames: i64,
    format: TimecodeFormat,
}

impl Timecode {
    /// Creates a timecode directly from a continuous signed frame index.
    #[must_use]
    pub const fn from_frames(frames: i64, format: TimecodeFormat) -> Self {
        Self { frames, format }
    }

    /// Parses a canonical label using an explicit format.
    pub fn parse(label: &str, format: TimecodeFormat) -> Result<Self, TimecodeError> {
        let (negative, unsigned) = match label.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, label),
        };

        let delimiter_positions: Vec<(usize, char)> = unsigned
            .char_indices()
            .filter(|(_, character)| *character == ':' || *character == ';')
            .collect();
        if delimiter_positions.len() != 3
            || delimiter_positions[0].1 != ':'
            || delimiter_positions[1].1 != ':'
        {
            return Err(TimecodeError::InvalidSyntax);
        }

        let found_separator = delimiter_positions[2].1;
        let expected_separator = format.mode.separator();
        if found_separator != expected_separator {
            return Err(TimecodeError::SeparatorMismatch {
                expected: expected_separator,
                found: found_separator,
            });
        }

        let first = delimiter_positions[0].0;
        let second = delimiter_positions[1].0;
        let third = delimiter_positions[2].0;
        let hours_text = &unsigned[..first];
        let minutes_text = &unsigned[first + 1..second];
        let seconds_text = &unsigned[second + 1..third];
        let frames_text = &unsigned[third + 1..];
        let frame_width = digit_count(format.nominal_fps().saturating_sub(1)).max(2);

        if hours_text.len() < 2
            || minutes_text.len() != 2
            || seconds_text.len() != 2
            || frames_text.len() != frame_width
        {
            return Err(TimecodeError::InvalidSyntax);
        }

        let hours = parse_digits(hours_text)?;
        let minutes = parse_digits(minutes_text)?;
        let seconds = parse_digits(seconds_text)?;
        let frames = parse_digits(frames_text)?;

        validate_component(TimecodeComponent::Minutes, minutes, 59)?;
        validate_component(TimecodeComponent::Seconds, seconds, 59)?;
        validate_component(
            TimecodeComponent::Frames,
            frames,
            u64::from(format.nominal_fps() - 1),
        )?;

        if format.mode == TimecodeMode::DropFrame
            && minutes % 10 != 0
            && seconds == 0
            && frames < u64::from(format.drop_frames_per_minute)
        {
            return Err(TimecodeError::DroppedFrameLabel {
                minute: minutes as u8,
                frame: frames as u32,
            });
        }

        let nominal_fps = i128::from(format.nominal_fps());
        let total_minutes = i128::from(hours)
            .checked_mul(60)
            .and_then(|value| value.checked_add(i128::from(minutes)))
            .ok_or(TimecodeError::ArithmeticOverflow)?;
        let total_seconds = total_minutes
            .checked_mul(60)
            .and_then(|value| value.checked_add(i128::from(seconds)))
            .ok_or(TimecodeError::ArithmeticOverflow)?;
        let nominal_count = total_seconds
            .checked_mul(nominal_fps)
            .and_then(|value| value.checked_add(i128::from(frames)))
            .ok_or(TimecodeError::ArithmeticOverflow)?;
        let dropped_minutes = total_minutes - total_minutes / 10;
        let dropped_labels = dropped_minutes
            .checked_mul(i128::from(format.drop_frames_per_minute))
            .ok_or(TimecodeError::ArithmeticOverflow)?;
        let magnitude = nominal_count
            .checked_sub(dropped_labels)
            .ok_or(TimecodeError::ArithmeticOverflow)?;
        let signed = if negative {
            magnitude
                .checked_neg()
                .ok_or(TimecodeError::ArithmeticOverflow)?
        } else {
            magnitude
        };
        let continuous_frames =
            i64::try_from(signed).map_err(|_| TimecodeError::ArithmeticOverflow)?;

        if negative && continuous_frames == 0 {
            return Err(TimecodeError::NegativeZero);
        }

        Ok(Self::from_frames(continuous_frames, format))
    }

    /// Returns the authoritative continuous signed frame index.
    #[must_use]
    pub const fn frames(self) -> i64 {
        self.frames
    }

    /// Returns the exact format retained by this value.
    #[must_use]
    pub const fn format(self) -> TimecodeFormat {
        self.format
    }

    /// Adds a signed number of physical frames without changing the format.
    pub fn checked_add_frames(self, delta: i64) -> Result<Self, TimecodeError> {
        self.frames
            .checked_add(delta)
            .map(|frames| Self::from_frames(frames, self.format))
            .ok_or(TimecodeError::ArithmeticOverflow)
    }

    /// Subtracts a signed number of physical frames without changing the format.
    pub fn checked_sub_frames(self, delta: i64) -> Result<Self, TimecodeError> {
        self.frames
            .checked_sub(delta)
            .map(|frames| Self::from_frames(frames, self.format))
            .ok_or(TimecodeError::ArithmeticOverflow)
    }

    /// Returns the signed physical-frame distance from another value.
    ///
    /// Both values must have the same exact rate and counting convention.
    pub fn checked_duration_since(self, earlier: Self) -> Result<i64, TimecodeError> {
        if self.format != earlier.format {
            return Err(TimecodeError::FormatMismatch {
                left: self.format,
                right: earlier.format,
            });
        }
        self.frames
            .checked_sub(earlier.frames)
            .ok_or(TimecodeError::ArithmeticOverflow)
    }

    /// Projects this frame position into exact rational time.
    #[must_use]
    pub fn to_rational_time(self) -> RationalTime {
        RationalTime::from_frames(self.frames, self.format.rate)
    }

    /// Converts this value to another format using an explicit rounding policy.
    pub fn converted_to(
        self,
        target: TimecodeFormat,
        rounding: TimeRounding,
    ) -> Result<Self, TimecodeError> {
        let frames = self
            .to_rational_time()
            .to_frames(target.rate, rounding)
            .map_err(TimecodeError::Time)?;
        Ok(Self::from_frames(frames, target))
    }

    fn display_count(self) -> i128 {
        let continuous = i128::from(self.frames.unsigned_abs());
        if self.format.mode == TimecodeMode::NonDropFrame {
            return continuous;
        }

        let nominal = i128::from(self.format.nominal_fps());
        let drop = i128::from(self.format.drop_frames_per_minute);
        let frames_per_minute = nominal * 60 - drop;
        let frames_per_ten_minutes = nominal * 600 - drop * 9;
        let ten_minute_cycles = continuous / frames_per_ten_minutes;
        let remainder = continuous % frames_per_ten_minutes;
        let mut label_count = continuous + ten_minute_cycles * drop * 9;

        if remainder >= drop {
            label_count += drop * ((remainder - drop) / frames_per_minute);
        }

        label_count
    }
}

impl fmt::Display for Timecode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let nominal = i128::from(self.format.nominal_fps());
        let count = self.display_count();
        let frames_per_hour = nominal * 3_600;
        let frames_per_minute = nominal * 60;
        let hours = count / frames_per_hour;
        let after_hours = count % frames_per_hour;
        let minutes = after_hours / frames_per_minute;
        let after_minutes = after_hours % frames_per_minute;
        let seconds = after_minutes / nominal;
        let frames = after_minutes % nominal;
        let frame_width = digit_count(self.format.nominal_fps().saturating_sub(1)).max(2);

        if self.frames < 0 {
            formatter.write_str("-")?;
        }
        write!(
            formatter,
            "{hours:02}:{minutes:02}:{seconds:02}{}{frames:0frame_width$}",
            self.format.mode.separator()
        )
    }
}

fn parse_digits(text: &str) -> Result<u64, TimecodeError> {
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(TimecodeError::InvalidSyntax);
    }
    text.parse().map_err(|_| TimecodeError::ArithmeticOverflow)
}

fn validate_component(
    component: TimecodeComponent,
    value: u64,
    maximum: u64,
) -> Result<(), TimecodeError> {
    if value > maximum {
        return Err(TimecodeError::ComponentOutOfRange {
            component,
            value,
            maximum,
        });
    }
    Ok(())
}

fn digit_count(value: u32) -> usize {
    if value == 0 {
        return 1;
    }

    let mut remaining = value;
    let mut digits = 0;
    while remaining != 0 {
        remaining /= 10;
        digits += 1;
    }
    digits
}
