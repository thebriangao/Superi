//! Exact timestamp, edit-list, and source timecode metadata contracts.
//!
//! Compressed packet timestamps stay in their media timebase. [`TimestampNormalizer`] moves the
//! first presentation timestamp to zero without changing relative presentation/decode offsets,
//! while [`EditTimeline`] maps those normalized media coordinates to and from the edited movie
//! presentation timeline. All conversion uses integer rational arithmetic and an explicit rounding
//! policy; no floating-point clock conversion is involved. Source timecode descriptions preserve
//! their original frame timing, flags, sample width, source reference, and content associations.
//! [`SourceTimecode`] keeps that physical frame rate authoritative while using a separate canonical
//! counting format only to project source labels.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::time::{FrameRate, RationalTime, TimeRange, TimeRounding, Timebase};
use superi_core::timecode::{Timecode, TimecodeFormat};

use crate::demux::{StreamEdit, StreamId};

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

/// The integer representation used by one source timecode sample.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum TimecodeSampleEncoding {
    /// Big-endian signed 32-bit frame numbers, or unsigned counters.
    Timecode32,
    /// Big-endian signed 64-bit frame numbers, or unsigned counters.
    Timecode64,
}

impl TimecodeSampleEncoding {
    /// Returns the stable machine code for this representation.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Timecode32 => "timecode32",
            Self::Timecode64 => "timecode64",
        }
    }

    /// Returns the exact encoded sample width.
    #[must_use]
    pub const fn byte_width(self) -> usize {
        match self {
            Self::Timecode32 => 4,
            Self::Timecode64 => 8,
        }
    }
}

/// Losslessly preserved source timecode control flags.
///
/// The four known bits follow the QuickTime timecode definition. Unknown bits
/// remain available through [`Self::bits`] so a future exporter can reproduce
/// metadata it does not yet interpret.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct TimecodeFlags(u32);

impl TimecodeFlags {
    /// The label sequence uses drop-frame counting.
    pub const DROP_FRAME: u32 = 0x0001;
    /// Display values wrap after 24 hours.
    pub const WRAPS_AT_24_HOURS: u32 = 0x0002;
    /// Negative sample frame numbers are permitted.
    pub const NEGATIVE_TIMES_ALLOWED: u32 = 0x0004;
    /// Sample integers represent counters rather than timecode frame numbers.
    pub const COUNTER: u32 = 0x0008;

    const KNOWN: u32 =
        Self::DROP_FRAME | Self::WRAPS_AT_24_HOURS | Self::NEGATIVE_TIMES_ALLOWED | Self::COUNTER;

    /// Preserves a raw container flag word.
    #[must_use]
    pub const fn new(bits: u32) -> Self {
        Self(bits)
    }

    /// Returns every original flag bit, including unknown bits.
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Returns flag bits that this version does not interpret.
    #[must_use]
    pub const fn unknown_bits(self) -> u32 {
        self.0 & !Self::KNOWN
    }

    /// Returns whether labels use drop-frame counting.
    #[must_use]
    pub const fn is_drop_frame(self) -> bool {
        self.contains(Self::DROP_FRAME)
    }

    /// Returns whether display values wrap after 24 hours.
    #[must_use]
    pub const fn wraps_at_24_hours(self) -> bool {
        self.contains(Self::WRAPS_AT_24_HOURS)
    }

    /// Returns whether negative frame numbers are valid.
    #[must_use]
    pub const fn negative_times_allowed(self) -> bool {
        self.contains(Self::NEGATIVE_TIMES_ALLOWED)
    }

    /// Returns whether samples contain counter values instead of timecode.
    #[must_use]
    pub const fn is_counter(self) -> bool {
        self.contains(Self::COUNTER)
    }

    const fn contains(self, bit: u32) -> bool {
        self.0 & bit != 0
    }
}

/// Complete format metadata required to interpret and reproduce source samples.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimecodeDescription {
    encoding: TimecodeSampleEncoding,
    flags: TimecodeFlags,
    time_scale: u32,
    frame_duration: u32,
    frame_quanta: u32,
    media_frame_rate: FrameRate,
    timecode_format: TimecodeFormat,
    source_reference: Arc<[u8]>,
}

impl TimecodeDescription {
    /// Validates one container timecode definition without discarding raw fields.
    ///
    /// `time_scale / frame_duration` is the physical media frame rate, while
    /// `frame_quanta` is the nominal label count. QuickTime's documented
    /// `2997 / 100` and `5994 / 100` drop-frame forms are mapped to canonical
    /// `30000 / 1001` and `60000 / 1001` label formats, but their original
    /// fields remain unchanged for export.
    pub fn new(
        encoding: TimecodeSampleEncoding,
        flags: TimecodeFlags,
        time_scale: u32,
        frame_duration: u32,
        frame_quanta: u32,
        source_reference: Arc<[u8]>,
    ) -> Result<Self> {
        if frame_quanta == 0 {
            return Err(invalid_description(
                "timecode frame quanta must be greater than zero",
            ));
        }
        if flags.is_drop_frame() && flags.is_counter() {
            return Err(invalid_description(
                "drop-frame and counter flags cannot describe the same sample values",
            ));
        }

        let media_frame_rate = FrameRate::new(time_scale, frame_duration).map_err(|error| {
            error.with_context(ErrorContext::new(COMPONENT, "create_timecode_description"))
        })?;
        if !flags.is_counter() && media_frame_rate.nominal_fps() != frame_quanta {
            return Err(invalid_description(
                "timecode frame quanta must match the nominal media frame rate",
            ));
        }

        let timecode_format = if flags.is_drop_frame() {
            drop_frame_format(media_frame_rate, frame_quanta)?
        } else {
            TimecodeFormat::non_drop(media_frame_rate)
        };

        Ok(Self {
            encoding,
            flags,
            time_scale,
            frame_duration,
            frame_quanta,
            media_frame_rate,
            timecode_format,
            source_reference,
        })
    }

    /// Returns the integer sample representation.
    #[must_use]
    pub const fn encoding(&self) -> TimecodeSampleEncoding {
        self.encoding
    }

    /// Returns all preserved source flags.
    #[must_use]
    pub const fn flags(&self) -> TimecodeFlags {
        self.flags
    }

    /// Returns the original number of time units per second.
    #[must_use]
    pub const fn time_scale(&self) -> u32 {
        self.time_scale
    }

    /// Returns the original time units occupied by one media frame.
    #[must_use]
    pub const fn frame_duration(&self) -> u32 {
        self.frame_duration
    }

    /// Returns the nominal number of labels per second, or frames per counter tick.
    #[must_use]
    pub const fn frame_quanta(&self) -> u32 {
        self.frame_quanta
    }

    /// Returns the exact physical rate recorded by the container.
    #[must_use]
    pub const fn media_frame_rate(&self) -> FrameRate {
        self.media_frame_rate
    }

    /// Returns the canonical format used only for editorial label counting.
    ///
    /// Use [`SourceTimecode::to_rational_time`] for physical time conversion; this counting format
    /// may intentionally differ from the exact source rate.
    #[must_use]
    pub const fn timecode_format(&self) -> TimecodeFormat {
        self.timecode_format
    }

    /// Creates a source timecode value without conflating physical time with label counting.
    ///
    /// Encoding-width validation is deferred until the value is encoded or attached to a
    /// [`SourceTimecodeTrack`], allowing callers to receive an actionable width error at that
    /// boundary.
    pub fn timecode_from_frames(&self, frames: i64) -> Result<SourceTimecode> {
        if self.flags.is_counter() {
            return Err(invalid_sample(
                "counter description cannot create an editorial timecode value",
            ));
        }
        if frames < 0 && !self.flags.negative_times_allowed() {
            return Err(invalid_sample(
                "negative timecode value is forbidden by its format description",
            ));
        }
        Ok(self.source_timecode(frames))
    }

    /// Parses a source label using this description's counting convention and physical clock.
    pub fn timecode_from_label(&self, label: &str) -> Result<SourceTimecode> {
        if self.flags.is_counter() {
            return Err(invalid_sample(
                "counter description cannot parse an editorial timecode label",
            ));
        }
        let timecode = Timecode::parse(label, self.timecode_format).map_err(|source| {
            Error::with_source(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "source timecode label is invalid for its format description",
                source,
            )
            .with_context(ErrorContext::new(COMPONENT, "parse_source_timecode"))
        })?;
        self.timecode_from_frames(timecode.frames())
    }

    /// Returns the opaque source-tape or source-device reference.
    #[must_use]
    pub fn source_reference(&self) -> &[u8] {
        &self.source_reference
    }

    /// Decodes one complete big-endian source sample.
    pub fn decode_sample(&self, bytes: &[u8]) -> Result<TimecodeValue> {
        if bytes.len() != self.encoding.byte_width() {
            return Err(corrupt_sample(
                "timecode sample width does not match its format description",
            ));
        }

        if self.flags.is_counter() {
            let raw_counter = match self.encoding {
                TimecodeSampleEncoding::Timecode32 => u64::from(u32::from_be_bytes(
                    bytes
                        .try_into()
                        .expect("sample width was checked for timecode32"),
                )),
                TimecodeSampleEncoding::Timecode64 => u64::from_be_bytes(
                    bytes
                        .try_into()
                        .expect("sample width was checked for timecode64"),
                ),
            };
            let quanta = u64::from(self.frame_quanta);
            if raw_counter % quanta != 0 {
                return Err(corrupt_sample(
                    "timecode counter sample is not divisible by its frame quanta",
                ));
            }
            return Ok(TimecodeValue::Counter(raw_counter / quanta));
        }

        let frames = match self.encoding {
            TimecodeSampleEncoding::Timecode32 => i64::from(i32::from_be_bytes(
                bytes
                    .try_into()
                    .expect("sample width was checked for timecode32"),
            )),
            TimecodeSampleEncoding::Timecode64 => i64::from_be_bytes(
                bytes
                    .try_into()
                    .expect("sample width was checked for timecode64"),
            ),
        };
        if frames < 0 && !self.flags.negative_times_allowed() {
            return Err(corrupt_sample(
                "negative timecode sample is forbidden by its format description",
            ));
        }
        Ok(TimecodeValue::Timecode(self.source_timecode(frames)))
    }

    /// Encodes one value for a container exporter without changing its meaning.
    pub fn encode_sample(&self, value: TimecodeValue) -> Result<Vec<u8>> {
        match (self.flags.is_counter(), value) {
            (true, TimecodeValue::Counter(counter)) => {
                let raw_counter = counter
                    .checked_mul(u64::from(self.frame_quanta))
                    .ok_or_else(|| {
                        invalid_sample(
                            "counter multiplied by frame quanta exceeds its sample width",
                        )
                    })?;
                match self.encoding {
                    TimecodeSampleEncoding::Timecode32 => {
                        let counter = u32::try_from(raw_counter).map_err(|_| {
                            invalid_sample("counter does not fit the timecode32 sample width")
                        })?;
                        Ok(counter.to_be_bytes().to_vec())
                    }
                    TimecodeSampleEncoding::Timecode64 => Ok(raw_counter.to_be_bytes().to_vec()),
                }
            }
            (false, TimecodeValue::Timecode(timecode)) => {
                if timecode.counting_format() != self.timecode_format
                    || timecode.physical_frame_rate() != self.media_frame_rate
                {
                    return Err(invalid_sample(
                        "timecode value clocks do not match its source description",
                    ));
                }
                if timecode.frames() < 0 && !self.flags.negative_times_allowed() {
                    return Err(invalid_sample(
                        "negative timecode value is forbidden by its format description",
                    ));
                }
                match self.encoding {
                    TimecodeSampleEncoding::Timecode32 => {
                        let frames = i32::try_from(timecode.frames()).map_err(|_| {
                            invalid_sample("frame number does not fit the timecode32 sample width")
                        })?;
                        Ok(frames.to_be_bytes().to_vec())
                    }
                    TimecodeSampleEncoding::Timecode64 => {
                        Ok(timecode.frames().to_be_bytes().to_vec())
                    }
                }
            }
            (true, TimecodeValue::Timecode(_)) => Err(invalid_sample(
                "counter description cannot encode a timecode frame number",
            )),
            (false, TimecodeValue::Counter(_)) => Err(invalid_sample(
                "timecode description cannot encode a counter value",
            )),
        }
    }

    fn source_timecode(&self, frames: i64) -> SourceTimecode {
        SourceTimecode {
            frames,
            physical_frame_rate: self.media_frame_rate,
            counting_format: self.timecode_format,
        }
    }

    fn project_timecode(&self, timecode: SourceTimecode) -> Result<SourceTimecode> {
        if !self.flags.wraps_at_24_hours() {
            return Ok(timecode);
        }
        let frames_per_day = frames_per_24_hours(self)?;
        Ok(SourceTimecode {
            frames: timecode.frames.rem_euclid(frames_per_day),
            ..timecode
        })
    }
}

/// One source frame position with separate physical-time and label-counting clocks.
///
/// The physical rate is authoritative for time conversion. The counting format exists only to
/// parse and display SMPTE-style labels, preventing decimal QuickTime rates such as `2997/100`
/// from being silently converted using the canonical `30000/1001` drop-frame counting rate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SourceTimecode {
    frames: i64,
    physical_frame_rate: FrameRate,
    counting_format: TimecodeFormat,
}

impl SourceTimecode {
    /// Returns the authoritative continuous source frame index.
    #[must_use]
    pub const fn frames(self) -> i64 {
        self.frames
    }

    /// Returns the exact physical rate recorded by the source description.
    #[must_use]
    pub const fn physical_frame_rate(self) -> FrameRate {
        self.physical_frame_rate
    }

    /// Returns the convention used only to parse and display source labels.
    #[must_use]
    pub const fn counting_format(self) -> TimecodeFormat {
        self.counting_format
    }

    /// Projects the source frame index into time using the authoritative physical rate.
    #[must_use]
    pub fn to_rational_time(self) -> RationalTime {
        RationalTime::from_frames(self.frames, self.physical_frame_rate)
    }

    /// Adds physical source frames without applying container display wrapping.
    pub fn checked_add_frames(self, delta: i64) -> Result<Self> {
        let frames = self.frames.checked_add(delta).ok_or_else(|| {
            Error::new(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "source timecode frame calculation exceeded its range",
            )
            .with_context(ErrorContext::new(COMPONENT, "add_source_timecode_frames"))
        })?;
        Ok(Self { frames, ..self })
    }
}

impl fmt::Display for SourceTimecode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        Timecode::from_frames(self.frames, self.counting_format).fmt(formatter)
    }
}

/// The interpreted value stored in one source sample.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TimecodeValue {
    /// A continuous source frame number with explicit physical and label clocks.
    Timecode(SourceTimecode),
    /// A logical counter value after applying the description's frame quanta.
    Counter(u64),
}

/// One contiguous mapping from media time to a starting source value.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimecodeSegment {
    media_range: TimeRange,
    start_value: TimecodeValue,
}

impl TimecodeSegment {
    /// Creates one source segment. Track construction validates ordering and format.
    #[must_use]
    pub const fn new(media_range: TimeRange, start_value: TimecodeValue) -> Self {
        Self {
            media_range,
            start_value,
        }
    }

    /// Returns the half-open media interval covered by this source value.
    #[must_use]
    pub const fn media_range(self) -> TimeRange {
        self.media_range
    }

    /// Returns the value at the segment's inclusive start.
    #[must_use]
    pub const fn start_value(self) -> TimecodeValue {
        self.start_value
    }
}

/// Validated timecode metadata and mappings for one source track.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SourceTimecodeTrack {
    stream_id: StreamId,
    description: TimecodeDescription,
    associated_stream_ids: Vec<StreamId>,
    segments: Vec<TimecodeSegment>,
}

impl SourceTimecodeTrack {
    /// Creates a source timecode track with deterministic content associations.
    pub fn new(
        stream_id: StreamId,
        description: TimecodeDescription,
        associated_stream_ids: Vec<StreamId>,
        segments: Vec<TimecodeSegment>,
    ) -> Result<Self> {
        let mut associations = BTreeSet::new();
        if associated_stream_ids
            .iter()
            .any(|id| *id == stream_id || !associations.insert(*id))
        {
            return Err(invalid_track(
                "timecode content associations must be unique and cannot reference the timecode track itself",
            ));
        }
        if segments.is_empty() {
            return Err(invalid_track(
                "timecode track must contain at least one mapped segment",
            ));
        }

        let timebase = segments[0].media_range.timebase();
        let mut previous_end = None;
        for segment in &segments {
            if segment.media_range.is_empty() {
                return Err(invalid_track(
                    "timecode segments must cover a nonempty media interval",
                ));
            }
            if segment.media_range.timebase() != timebase {
                return Err(invalid_track(
                    "timecode segments must use one source-track timebase",
                ));
            }
            if previous_end.is_some_and(|end| segment.media_range.start() < end) {
                return Err(invalid_track(
                    "timecode segments must be ordered and must not overlap",
                ));
            }
            previous_end = Some(segment.media_range.end_exclusive().map_err(|error| {
                error.with_context(ErrorContext::new(COMPONENT, "create_timecode_track"))
            })?);
            validate_segment_value(&description, segment.start_value)?;
        }

        Ok(Self {
            stream_id,
            description,
            associated_stream_ids,
            segments,
        })
    }

    /// Returns the source-local timecode stream identifier.
    #[must_use]
    pub const fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    /// Returns the exact source format description.
    #[must_use]
    pub const fn description(&self) -> &TimecodeDescription {
        &self.description
    }

    /// Returns content streams that explicitly reference this timecode track.
    #[must_use]
    pub fn associated_stream_ids(&self) -> &[StreamId] {
        &self.associated_stream_ids
    }

    /// Returns mappings in ascending media-time order.
    #[must_use]
    pub fn segments(&self) -> &[TimecodeSegment] {
        &self.segments
    }

    /// Resolves source timecode at a media position, rounding within a frame down.
    ///
    /// A position outside every segment returns `None`. Counter-mode tracks do
    /// not have editorial labels and return an explicit unsupported error.
    pub fn timecode_at(&self, media_time: RationalTime) -> Result<Option<SourceTimecode>> {
        if self.description.flags.is_counter() {
            return Err(Error::new(
                ErrorCategory::Unsupported,
                Recoverability::Degraded,
                "counter-mode source metadata does not define editorial timecode labels",
            )
            .with_context(ErrorContext::new(COMPONENT, "resolve_timecode")));
        }

        let mut selected = None;
        for segment in &self.segments {
            let contains = segment.media_range.contains(media_time).map_err(|error| {
                error.with_context(ErrorContext::new(COMPONENT, "resolve_timecode"))
            })?;
            if contains {
                selected = Some(segment);
                break;
            }
        }
        let Some(segment) = selected else {
            return Ok(None);
        };
        let TimecodeValue::Timecode(start) = segment.start_value else {
            return Err(internal_track(
                "validated timecode track contains a counter segment",
            ));
        };
        let elapsed_frames = media_time
            .checked_sub_at(
                segment.media_range.start(),
                self.description.media_frame_rate.timebase(),
                TimeRounding::Floor,
            )
            .map_err(|error| error.with_context(ErrorContext::new(COMPONENT, "resolve_timecode")))?
            .value();
        let timecode = start.checked_add_frames(elapsed_frames)?;
        self.description.project_timecode(timecode).map(Some)
    }
}

fn frames_per_24_hours(description: &TimecodeDescription) -> Result<i64> {
    const MINUTES_PER_DAY: u64 = 24 * 60;
    const TENTH_MINUTES_PER_DAY: u64 = 24 * 6;
    const SECONDS_PER_DAY: u64 = 24 * 60 * 60;

    let nominal = u64::from(description.frame_quanta)
        .checked_mul(SECONDS_PER_DAY)
        .ok_or_else(|| invalid_description("24-hour timecode frame count overflows"))?;
    let dropped = u64::from(description.timecode_format.drop_frames_per_minute())
        .checked_mul(MINUTES_PER_DAY - TENTH_MINUTES_PER_DAY)
        .ok_or_else(|| invalid_description("24-hour drop-frame count overflows"))?;
    let frames = nominal
        .checked_sub(dropped)
        .ok_or_else(|| invalid_description("24-hour timecode frame count is invalid"))?;
    i64::try_from(frames)
        .map_err(|_| invalid_description("24-hour timecode frame count exceeds i64"))
}

fn drop_frame_format(media_rate: FrameRate, frame_quanta: u32) -> Result<TimecodeFormat> {
    if frame_quanta % 30 != 0 {
        return Err(invalid_description(
            "drop-frame labels require a nominal rate that is a multiple of 30",
        ));
    }
    let canonical_numerator = frame_quanta
        .checked_mul(1_000)
        .ok_or_else(|| invalid_description("drop-frame nominal rate is too large"))?;
    let decimal_numerator = frame_quanta
        .checked_mul(999)
        .ok_or_else(|| invalid_description("drop-frame nominal rate is too large"))?;
    let raw_numerator = u128::from(media_rate.numerator());
    let raw_denominator = u128::from(media_rate.denominator());
    let canonical_matches =
        raw_numerator * 1_001 == u128::from(canonical_numerator) * raw_denominator;
    let decimal_matches = raw_numerator * 1_000 == u128::from(decimal_numerator) * raw_denominator;
    if !canonical_matches && !decimal_matches {
        return Err(invalid_description(
            "drop-frame media rate is not an NTSC-related rate",
        ));
    }

    let canonical_rate = FrameRate::new(canonical_numerator, 1_001).map_err(|error| {
        error.with_context(ErrorContext::new(COMPONENT, "create_timecode_description"))
    })?;
    TimecodeFormat::drop_frame(canonical_rate).map_err(|source| {
        Error::with_source(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "drop-frame timecode format is unsupported",
            source,
        )
        .with_context(ErrorContext::new(COMPONENT, "create_timecode_description"))
    })
}

fn validate_segment_value(description: &TimecodeDescription, value: TimecodeValue) -> Result<()> {
    match (description.flags.is_counter(), value) {
        (true, TimecodeValue::Counter(_)) => Ok(()),
        (false, TimecodeValue::Timecode(timecode))
            if timecode.counting_format() == description.timecode_format
                && timecode.physical_frame_rate() == description.media_frame_rate =>
        {
            if timecode.frames() < 0 && !description.flags.negative_times_allowed() {
                Err(invalid_track(
                    "negative timecode segment is forbidden by its format description",
                ))
            } else {
                Ok(())
            }
        }
        (false, TimecodeValue::Timecode(_)) => Err(invalid_track(
            "timecode segment format does not match its source description",
        )),
        (true, TimecodeValue::Timecode(_)) | (false, TimecodeValue::Counter(_)) => Err(
            invalid_track("timecode segment value kind does not match its source description"),
        ),
    }?;
    description
        .encode_sample(value)
        .map(|_| ())
        .map_err(|source| {
            Error::with_source(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "timecode segment value cannot round-trip through its declared sample encoding",
                source,
            )
            .with_context(ErrorContext::new(COMPONENT, "create_timecode_track"))
        })
}

fn invalid_description(message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, "create_timecode_description"))
}

fn corrupt_sample(message: &'static str) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, "decode_timecode_sample"))
}

fn invalid_sample(message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, "encode_timecode_sample"))
}

fn invalid_track(message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, "create_timecode_track"))
}

fn internal_track(message: &'static str) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(ErrorContext::new(COMPONENT, "resolve_timecode"))
}
