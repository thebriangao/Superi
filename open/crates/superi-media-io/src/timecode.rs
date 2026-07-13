//! Exact timestamp normalization and container edit-list mapping.
//!
//! Compressed packet timestamps stay in their media timebase. [`TimestampNormalizer`] moves the
//! first presentation timestamp to zero without changing relative presentation/decode offsets,
//! while [`EditTimeline`] maps those normalized media coordinates to and from the edited movie
//! presentation timeline. All conversion uses integer rational arithmetic and an explicit rounding
//! policy; no floating-point clock conversion is involved.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::time::{RationalTime, TimeRounding, Timebase};

use crate::demux::StreamEdit;

const COMPONENT: &str = "superi-media-io.timecode";
const FIXED_RATE_ONE: u64 = 1 << 16;

/// A reversible origin shift for timestamps in one media timebase.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimestampNormalizer {
    origin: RationalTime,
}

impl TimestampNormalizer {
    /// Creates a normalizer whose source `origin` becomes normalized time zero.
    #[must_use]
    pub const fn new(origin: RationalTime) -> Self {
        Self { origin }
    }

    /// Creates a normalizer from the earliest supplied presentation timestamp.
    ///
    /// An empty timestamp set uses zero, which makes normalization an identity operation.
    #[must_use]
    pub fn from_presentation_timestamps(
        timebase: Timebase,
        timestamps: impl IntoIterator<Item = i64>,
    ) -> Self {
        let origin = timestamps.into_iter().min().unwrap_or(0);
        Self::new(RationalTime::new(origin, timebase))
    }

    /// Returns the original timestamp that maps to normalized time zero.
    #[must_use]
    pub const fn origin(self) -> RationalTime {
        self.origin
    }

    /// Shifts a source timestamp into the normalized media timeline.
    pub fn normalize(self, timestamp: RationalTime) -> Result<RationalTime> {
        self.require_timebase(timestamp, "normalize_timestamp")?;
        let value = timestamp
            .value()
            .checked_sub(self.origin.value())
            .ok_or_else(|| overflow("normalize_timestamp"))?;
        Ok(RationalTime::new(value, self.origin.timebase()))
    }

    /// Restores a normalized timestamp to its original source coordinate.
    pub fn restore(self, timestamp: RationalTime) -> Result<RationalTime> {
        self.require_timebase(timestamp, "restore_timestamp")?;
        let value = timestamp
            .value()
            .checked_add(self.origin.value())
            .ok_or_else(|| overflow("restore_timestamp"))?;
        Ok(RationalTime::new(value, self.origin.timebase()))
    }

    fn require_timebase(self, timestamp: RationalTime, operation: &'static str) -> Result<()> {
        if timestamp.timebase() != self.origin.timebase() {
            return Err(invalid(
                operation,
                "timestamp must use the normalizer media timebase",
            ));
        }
        Ok(())
    }
}

/// The result of mapping one edited presentation coordinate into media time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EditMapping {
    /// The coordinate resolves to media in one edit-list entry.
    Media {
        /// Zero-based edit-list entry. An implicit identity map uses entry zero.
        edit_index: usize,
        /// Exact or explicitly rounded coordinate in the media timebase.
        media_time: RationalTime,
    },
    /// The coordinate lies in an empty edit and intentionally presents no media.
    Empty {
        /// Zero-based edit-list entry.
        edit_index: usize,
    },
    /// The coordinate lies outside the finite edited presentation.
    Outside,
}

/// A validated mapping between a movie presentation timeline and one stream's media timeline.
///
/// Edit durations use `presentation_timebase`; edit media starts use `media_timebase`. Positive
/// QuickTime/ISO 16.16 fixed-point rates are supported, including fractional and repeated edits.
/// A final zero-duration edit is treated as open-ended, matching fragmented-media initialization
/// semantics. Empty edit lists form an unbounded identity map between the two timebases.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditTimeline {
    presentation_timebase: Timebase,
    media_timebase: Timebase,
    edits: Vec<StreamEdit>,
    starts: Vec<i64>,
}

impl EditTimeline {
    /// Validates an ordered edit list and prepares exact bidirectional mapping.
    pub fn new(
        presentation_timebase: Timebase,
        media_timebase: Timebase,
        edits: Vec<StreamEdit>,
    ) -> Result<Self> {
        let mut starts = Vec::with_capacity(edits.len());
        let mut start = 0_i64;
        for (index, edit) in edits.iter().copied().enumerate() {
            if edit.segment_duration().timebase() != presentation_timebase {
                return Err(invalid(
                    "create_edit_timeline",
                    "edit duration must use the presentation timebase",
                ));
            }
            if edit
                .media_time()
                .is_some_and(|time| time.timebase() != media_timebase)
            {
                return Err(invalid(
                    "create_edit_timeline",
                    "edit media start must use the media timebase",
                ));
            }
            fixed_rate(edit)?;
            if edit.segment_duration().is_zero() && index + 1 != edits.len() {
                return Err(invalid(
                    "create_edit_timeline",
                    "only the final edit may have an open-ended zero duration",
                ));
            }
            starts.push(start);
            if !edit.segment_duration().is_zero() {
                start = start
                    .checked_add(edit.segment_duration().value() as i64)
                    .ok_or_else(|| overflow("create_edit_timeline"))?;
            }
        }
        Ok(Self {
            presentation_timebase,
            media_timebase,
            edits,
            starts,
        })
    }

    /// Returns the movie or edited-presentation timebase.
    #[must_use]
    pub const fn presentation_timebase(&self) -> Timebase {
        self.presentation_timebase
    }

    /// Returns the stream media timebase.
    #[must_use]
    pub const fn media_timebase(&self) -> Timebase {
        self.media_timebase
    }

    /// Returns the preserved ordered edit entries.
    #[must_use]
    pub fn edits(&self) -> &[StreamEdit] {
        &self.edits
    }

    /// Maps a movie presentation coordinate into the stream media timeline.
    pub fn presentation_to_media(
        &self,
        presentation_time: RationalTime,
        rounding: TimeRounding,
    ) -> Result<EditMapping> {
        let presentation_time = presentation_time
            .checked_rescale(self.presentation_timebase, rounding)
            .map_err(|error| {
                error.with_context(ErrorContext::new(COMPONENT, "map_presentation_to_media"))
            })?;
        if self.edits.is_empty() {
            return Ok(EditMapping::Media {
                edit_index: 0,
                media_time: presentation_time
                    .checked_rescale(self.media_timebase, rounding)
                    .map_err(|error| {
                        error
                            .with_context(ErrorContext::new(COMPONENT, "map_presentation_to_media"))
                    })?,
            });
        }
        if presentation_time.is_negative() {
            return Ok(EditMapping::Outside);
        }

        for (index, edit) in self.edits.iter().copied().enumerate() {
            let start = self.starts[index];
            let at_or_after_start = presentation_time.value() >= start;
            let open_ended = edit.segment_duration().is_zero();
            let before_end = open_ended
                || presentation_time.value()
                    < start
                        .checked_add(edit.segment_duration().value() as i64)
                        .ok_or_else(|| overflow("map_presentation_to_media"))?;
            if !at_or_after_start || !before_end {
                continue;
            }
            let Some(media_start) = edit.media_time() else {
                return Ok(EditMapping::Empty { edit_index: index });
            };
            let delta = presentation_time
                .value()
                .checked_sub(start)
                .ok_or_else(|| overflow("map_presentation_to_media"))?;
            let rate = fixed_rate(edit)?;
            let media_delta = if rate == 0 {
                0
            } else {
                presentation_delta_to_media(
                    delta,
                    self.presentation_timebase,
                    self.media_timebase,
                    rate,
                    rounding,
                )?
            };
            let media_value = media_start
                .value()
                .checked_add(media_delta)
                .ok_or_else(|| overflow("map_presentation_to_media"))?;
            return Ok(EditMapping::Media {
                edit_index: index,
                media_time: RationalTime::new(media_value, self.media_timebase),
            });
        }
        Ok(EditMapping::Outside)
    }

    /// Returns every edited presentation coordinate that refers to `media_time`.
    ///
    /// Multiple coordinates are returned when edits repeat a media range. Coordinates are ordered
    /// by presentation time and duplicate rounded coordinates are removed.
    pub fn media_to_presentations(
        &self,
        media_time: RationalTime,
        rounding: TimeRounding,
    ) -> Result<Vec<RationalTime>> {
        let media_time = media_time
            .checked_rescale(self.media_timebase, rounding)
            .map_err(|error| {
                error.with_context(ErrorContext::new(COMPONENT, "map_media_to_presentation"))
            })?;
        if self.edits.is_empty() {
            return Ok(vec![media_time
                .checked_rescale(self.presentation_timebase, rounding)
                .map_err(|error| {
                    error.with_context(ErrorContext::new(COMPONENT, "map_media_to_presentation"))
                })?]);
        }

        let mut presentations = Vec::new();
        for (index, edit) in self.edits.iter().copied().enumerate() {
            let Some(media_start) = edit.media_time() else {
                continue;
            };
            let media_delta = media_time
                .value()
                .checked_sub(media_start.value())
                .ok_or_else(|| overflow("map_media_to_presentation"))?;
            if media_delta < 0 {
                continue;
            }
            let rate = fixed_rate(edit)?;
            if rate == 0 {
                if media_time == media_start {
                    presentations.push(RationalTime::new(
                        self.starts[index],
                        self.presentation_timebase,
                    ));
                }
                continue;
            }
            let floor_delta = media_delta_to_presentation(
                media_delta,
                self.media_timebase,
                self.presentation_timebase,
                rate,
                TimeRounding::Floor,
            )?;
            if !edit.segment_duration().is_zero()
                && floor_delta >= edit.segment_duration().value() as i64
            {
                continue;
            }
            let presentation_delta = media_delta_to_presentation(
                media_delta,
                self.media_timebase,
                self.presentation_timebase,
                rate,
                rounding,
            )?;
            if !edit.segment_duration().is_zero()
                && presentation_delta >= edit.segment_duration().value() as i64
            {
                continue;
            }
            let value = self.starts[index]
                .checked_add(presentation_delta)
                .ok_or_else(|| overflow("map_media_to_presentation"))?;
            presentations.push(RationalTime::new(value, self.presentation_timebase));
        }
        presentations.sort_by_key(|time| time.value());
        presentations.dedup();
        Ok(presentations)
    }
}

fn fixed_rate(edit: StreamEdit) -> Result<u64> {
    if edit.rate_integer() < 0 {
        return Err(invalid(
            "create_edit_timeline",
            "edit playback rate must be positive",
        ));
    }
    let rate =
        (u64::from(edit.rate_integer() as u16) << 16) | u64::from(edit.rate_fraction() as u16);
    Ok(rate)
}

fn presentation_delta_to_media(
    value: i64,
    presentation_timebase: Timebase,
    media_timebase: Timebase,
    rate: u64,
    rounding: TimeRounding,
) -> Result<i64> {
    scaled_units(
        value,
        &[
            u64::from(presentation_timebase.denominator()),
            u64::from(media_timebase.numerator()),
            rate,
        ],
        &[
            u64::from(presentation_timebase.numerator()),
            u64::from(media_timebase.denominator()),
            FIXED_RATE_ONE,
        ],
        rounding,
        "map_presentation_to_media",
    )
}

fn media_delta_to_presentation(
    value: i64,
    media_timebase: Timebase,
    presentation_timebase: Timebase,
    rate: u64,
    rounding: TimeRounding,
) -> Result<i64> {
    scaled_units(
        value,
        &[
            u64::from(media_timebase.denominator()),
            u64::from(presentation_timebase.numerator()),
            FIXED_RATE_ONE,
        ],
        &[
            u64::from(media_timebase.numerator()),
            u64::from(presentation_timebase.denominator()),
            rate,
        ],
        rounding,
        "map_media_to_presentation",
    )
}

fn scaled_units(
    value: i64,
    numerator_factors: &[u64],
    denominator_factors: &[u64],
    rounding: TimeRounding,
    operation: &'static str,
) -> Result<i64> {
    if value == 0 {
        return Ok(0);
    }
    let mut numerators = Vec::with_capacity(numerator_factors.len() + 1);
    numerators.push(value.unsigned_abs());
    numerators.extend_from_slice(numerator_factors);
    let mut denominators = denominator_factors.to_vec();
    for numerator in &mut numerators {
        for denominator in &mut denominators {
            let divisor = greatest_common_divisor(*numerator, *denominator);
            *numerator /= divisor;
            *denominator /= divisor;
        }
    }
    let numerator = numerators.iter().try_fold(1_i128, |product, factor| {
        product.checked_mul(i128::from(*factor))
    });
    let denominator = denominators.iter().try_fold(1_i128, |product, factor| {
        product.checked_mul(i128::from(*factor))
    });
    let numerator = numerator.ok_or_else(|| overflow(operation))?;
    let denominator = denominator.ok_or_else(|| overflow(operation))?;
    let numerator = if value.is_negative() {
        -numerator
    } else {
        numerator
    };
    divide(numerator, denominator, rounding, operation)
}

fn divide(
    numerator: i128,
    denominator: i128,
    rounding: TimeRounding,
    operation: &'static str,
) -> Result<i64> {
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    let rounded = match rounding {
        TimeRounding::Exact => {
            if remainder != 0 {
                return Err(invalid(
                    operation,
                    "edit mapping is not exact at the requested timebase",
                ));
            }
            quotient
        }
        TimeRounding::Floor if remainder < 0 => quotient - 1,
        TimeRounding::Floor => quotient,
        TimeRounding::Ceil if remainder > 0 => quotient + 1,
        TimeRounding::Ceil | TimeRounding::TowardZero => quotient,
        TimeRounding::NearestTiesEven => {
            let doubled_remainder = remainder.abs() * 2;
            match doubled_remainder.cmp(&denominator) {
                std::cmp::Ordering::Less => quotient,
                std::cmp::Ordering::Greater => quotient + numerator.signum(),
                std::cmp::Ordering::Equal if quotient % 2 == 0 => quotient,
                std::cmp::Ordering::Equal => quotient + numerator.signum(),
            }
        }
        _ => {
            return Err(invalid(
                operation,
                "edit mapping does not support this rounding policy",
            ));
        }
    };
    i64::try_from(rounded).map_err(|_| overflow(operation))
}

fn greatest_common_divisor(mut left: u64, mut right: u64) -> u64 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn overflow(operation: &'static str) -> Error {
    invalid(
        operation,
        "timestamp exceeds the supported coordinate range",
    )
}
