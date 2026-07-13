//! Deterministic CPU image operations and bounded GPU validation reports.
//!
//! This module is an explicit validation boundary. Production render and
//! playback paths remain GPU-resident. Tests, diagnostics, and other callers
//! that already own a CPU image or explicit GPU readback can execute the
//! canonical CPU operation and compare complete image semantics plus samples.
//! No operation in this module performs an implicit readback or installs a CPU
//! production fallback.

use half::f16;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::{Matrix3, PixelBounds};
use superi_core::pixel::{AlphaMode, PixelFormat};

use crate::alpha::{AlphaTransform, PremultiplicationRule};
use crate::channels::{ChannelIndex, ChannelList};
use crate::ops::{
    blend, composite_over, crop, flip, remap_channels, rename_channels, resize, rotate, transform,
    ChannelSource, FlipAxis, QuarterTurn, ResampleFilter,
};
use crate::value::{Image, ImageSamples};

const COMPONENT: &str = "superi-image.reference";

/// One immutable, unary CPU operation used as a GPU validation oracle.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum UnaryReferenceOperation {
    /// Convert between explicit alpha associations.
    Alpha {
        /// Alpha interpretation produced by the operation.
        destination_mode: AlphaMode,
        /// Straight-to-premultiplied behavior when that conversion is requested.
        rule: PremultiplicationRule,
    },
    /// Crop to an exact signed data window.
    Crop {
        /// Destination data window.
        data_window: PixelBounds,
    },
    /// Resize into an exact signed data window.
    Resize {
        /// Destination data window.
        data_window: PixelBounds,
        /// Reconstruction filter.
        filter: ResampleFilter,
    },
    /// Apply one forward source-to-destination transform.
    Transform {
        /// Forward transform applied before destination sampling.
        source_to_destination: Matrix3,
        /// Destination data window.
        data_window: PixelBounds,
        /// Reconstruction filter.
        filter: ResampleFilter,
    },
    /// Mirror an image relative to its display window.
    Flip(FlipAxis),
    /// Rotate an image in an exact 90 degree increment.
    Rotate(QuarterTurn),
    /// Reorder, copy, fill, and rename channels explicitly.
    RemapChannels {
        /// Destination packed pixel format.
        pixel_format: PixelFormat,
        /// Destination alpha interpretation.
        alpha_mode: AlphaMode,
        /// Exact destination channel names and order.
        channels: ChannelList,
        /// One source expression per destination channel.
        sources: Vec<ChannelSource>,
    },
    /// Rename channels without changing sample order or payloads.
    RenameChannels {
        /// Exact replacement channel names and order.
        channels: ChannelList,
    },
}

impl UnaryReferenceOperation {
    /// Executes this operation synchronously through the canonical CPU kernels.
    ///
    /// Identical inputs and operation values produce identical owned image
    /// values. The source is never mutated.
    pub fn execute(&self, source: &Image) -> Result<Image> {
        match self {
            Self::Alpha {
                destination_mode,
                rule,
            } => AlphaTransform::with_rule(
                source.descriptor().channels(),
                source.descriptor().alpha_mode(),
                *destination_mode,
                *rule,
            )?
            .transform_image(source),
            Self::Crop { data_window } => crop(source, *data_window),
            Self::Resize {
                data_window,
                filter,
            } => resize(source, *data_window, *filter),
            Self::Transform {
                source_to_destination,
                data_window,
                filter,
            } => transform(source, *source_to_destination, *data_window, *filter),
            Self::Flip(axis) => flip(source, *axis),
            Self::Rotate(turn) => rotate(source, *turn),
            Self::RemapChannels {
                pixel_format,
                alpha_mode,
                channels,
                sources,
            } => remap_channels(
                source,
                *pixel_format,
                *alpha_mode,
                channels.clone(),
                sources,
            ),
            Self::RenameChannels { channels } => rename_channels(source, channels.clone()),
        }
    }
}

/// One immutable, binary CPU operation used as a GPU validation oracle.
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum BinaryReferenceOperation {
    /// Interpolate two images in associated-alpha space.
    Blend {
        /// Finite inclusive interpolation amount from 0 through 1.
        amount: f32,
    },
    /// Place the first image over the second using source-over composition.
    CompositeOver,
}

impl BinaryReferenceOperation {
    /// Executes this operation synchronously through the canonical CPU kernels.
    pub fn execute(&self, first: &Image, second: &Image) -> Result<Image> {
        match *self {
            Self::Blend { amount } => blend(first, second, amount),
            Self::CompositeOver => composite_over(first, second),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum ToleranceMode {
    Exact,
    Absolute(f64),
}

/// Explicit sample tolerance for comparing a GPU candidate to a CPU reference.
///
/// Integer samples are always compared exactly. Exact floating comparison uses
/// raw IEEE payload bits. Absolute floating comparison accepts finite values
/// within the configured distance, accepts NaN only when both values are NaN,
/// and requires infinities to have the same sign.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReferenceTolerance {
    mode: ToleranceMode,
}

impl ReferenceTolerance {
    /// Requires exact sample payloads for every representation.
    #[must_use]
    pub const fn exact() -> Self {
        Self {
            mode: ToleranceMode::Exact,
        }
    }

    /// Uses the repository's general normalized absolute-error ceiling.
    ///
    /// Nodes with stricter or domain-specific requirements should construct an
    /// explicit tolerance instead of silently inheriting this value.
    #[must_use]
    pub const fn general_normalized() -> Self {
        Self {
            mode: ToleranceMode::Absolute(0.001),
        }
    }

    /// Creates a finite nonnegative absolute tolerance for floating samples.
    pub fn absolute(maximum_absolute_error: f64) -> Result<Self> {
        if !maximum_absolute_error.is_finite() || maximum_absolute_error < 0.0 {
            return Err(invalid(
                "create_reference_tolerance",
                "reference tolerance must be finite and nonnegative",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "validate_absolute_tolerance")
                    .with_field("value", maximum_absolute_error.to_string()),
            ));
        }
        Ok(Self {
            mode: ToleranceMode::Absolute(if maximum_absolute_error == 0.0 {
                0.0
            } else {
                maximum_absolute_error
            }),
        })
    }

    /// Returns true when floating payload bits must match exactly.
    #[must_use]
    pub const fn is_exact(self) -> bool {
        matches!(self.mode, ToleranceMode::Exact)
    }

    /// Returns the floating absolute limit, or `None` for exact comparison.
    #[must_use]
    pub const fn maximum_absolute_error(self) -> Option<f64> {
        match self.mode {
            ToleranceMode::Exact => None,
            ToleranceMode::Absolute(value) => Some(value),
        }
    }
}

/// One exact sample payload retained in a validation mismatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ReferenceSample {
    /// Unsigned 8-bit value.
    U8(u8),
    /// Unsigned 16-bit value.
    U16(u16),
    /// Raw IEEE binary16 payload.
    F16(u16),
    /// Raw IEEE binary32 payload.
    F32(u32),
}

/// The first differing logical sample in row-major, channel-major order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SampleMismatch {
    sample_index: usize,
    x: i32,
    y: i32,
    channel: ChannelIndex,
    expected: ReferenceSample,
    actual: ReferenceSample,
    absolute_error: Option<f64>,
}

impl SampleMismatch {
    /// Returns the zero-based scalar sample index.
    #[must_use]
    pub const fn sample_index(self) -> usize {
        self.sample_index
    }

    /// Returns the signed image-space pixel x coordinate.
    #[must_use]
    pub const fn x(self) -> i32 {
        self.x
    }

    /// Returns the signed image-space pixel y coordinate.
    #[must_use]
    pub const fn y(self) -> i32 {
        self.y
    }

    /// Returns the stable ordered channel position.
    #[must_use]
    pub const fn channel(self) -> ChannelIndex {
        self.channel
    }

    /// Returns the exact CPU reference payload.
    #[must_use]
    pub const fn expected(self) -> ReferenceSample {
        self.expected
    }

    /// Returns the exact candidate payload.
    #[must_use]
    pub const fn actual(self) -> ReferenceSample {
        self.actual
    }

    /// Returns numeric distance when defined.
    ///
    /// Nonmatching NaN or infinity pairs report positive infinity. Matching NaN
    /// pairs under an absolute tolerance do not produce a mismatch.
    #[must_use]
    pub const fn absolute_error(self) -> Option<f64> {
        self.absolute_error
    }
}

/// Bounded evidence from comparing one candidate image to a CPU reference.
///
/// The report retains aggregate counts, the largest observed numeric distance,
/// and only the first mismatch. Memory use therefore does not grow with the
/// number of differing pixels.
#[derive(Clone, Debug, PartialEq)]
pub struct ReferenceComparison {
    tolerance: ReferenceTolerance,
    descriptor_matches: bool,
    metadata_matches: bool,
    compared_samples: usize,
    mismatched_samples: usize,
    maximum_absolute_error: Option<f64>,
    first_mismatch: Option<SampleMismatch>,
}

impl ReferenceComparison {
    /// Returns the tolerance used for sample validation.
    #[must_use]
    pub const fn tolerance(&self) -> ReferenceTolerance {
        self.tolerance
    }

    /// Returns whether channel, precision, alpha, color, and spatial semantics match.
    #[must_use]
    pub const fn descriptor_matches(&self) -> bool {
        self.descriptor_matches
    }

    /// Returns whether complete typed and source metadata match.
    #[must_use]
    pub const fn metadata_matches(&self) -> bool {
        self.metadata_matches
    }

    /// Returns the number of scalar samples compared.
    #[must_use]
    pub const fn compared_samples(&self) -> usize {
        self.compared_samples
    }

    /// Returns the number of scalar samples outside the tolerance.
    #[must_use]
    pub const fn mismatched_samples(&self) -> usize {
        self.mismatched_samples
    }

    /// Returns the largest defined absolute distance observed, including values
    /// that remain inside the tolerance.
    #[must_use]
    pub const fn maximum_absolute_error(&self) -> Option<f64> {
        self.maximum_absolute_error
    }

    /// Returns the first differing scalar sample in deterministic order.
    #[must_use]
    pub const fn first_mismatch(&self) -> Option<&SampleMismatch> {
        self.first_mismatch.as_ref()
    }

    /// Returns true only when complete semantics, metadata, and samples match.
    #[must_use]
    pub const fn matches(&self) -> bool {
        self.descriptor_matches && self.metadata_matches && self.mismatched_samples == 0
    }

    /// Converts a failed comparison into an actionable degraded internal error.
    ///
    /// A caller may use this result to reject a GPU implementation and continue
    /// with a separately selected safe path. This method does not choose or run
    /// such a fallback itself.
    pub fn require_match(&self) -> Result<()> {
        if self.matches() {
            return Ok(());
        }

        let tolerance = self
            .tolerance
            .maximum_absolute_error()
            .map_or_else(|| "exact".to_owned(), |value| value.to_string());
        let mut context = ErrorContext::new(COMPONENT, "validate_gpu_image")
            .with_field("compared_samples", self.compared_samples.to_string())
            .with_field("descriptor_matches", self.descriptor_matches.to_string())
            .with_field("metadata_matches", self.metadata_matches.to_string())
            .with_field("mismatched_samples", self.mismatched_samples.to_string())
            .with_field("tolerance", tolerance);
        if let Some(maximum) = self.maximum_absolute_error {
            context.insert_field("maximum_absolute_error", maximum.to_string());
        }
        if let Some(first) = self.first_mismatch {
            context.insert_field("first_channel", first.channel.get().to_string());
            context.insert_field("first_sample", first.sample_index.to_string());
            context.insert_field("first_x", first.x.to_string());
            context.insert_field("first_y", first.y.to_string());
        }

        Err(Error::new(
            ErrorCategory::Internal,
            Recoverability::Degraded,
            "GPU image output does not match the deterministic CPU reference",
        )
        .with_context(context))
    }
}

/// Compares complete candidate image semantics and samples to a CPU reference.
///
/// Descriptor mismatch prevents sample comparison because channel positions and
/// pixel coordinates would be ambiguous. Metadata mismatch remains independently
/// visible while samples are compared when descriptors match.
pub fn compare_images(
    reference: &Image,
    candidate: &Image,
    tolerance: ReferenceTolerance,
) -> Result<ReferenceComparison> {
    let descriptor_matches = reference.descriptor() == candidate.descriptor();
    let metadata_matches = reference.metadata() == candidate.metadata();
    let mut report = ReferenceComparison {
        tolerance,
        descriptor_matches,
        metadata_matches,
        compared_samples: 0,
        mismatched_samples: 0,
        maximum_absolute_error: None,
        first_mismatch: None,
    };
    if !descriptor_matches {
        return Ok(report);
    }

    match (reference.samples(), candidate.samples()) {
        (ImageSamples::U8(expected), ImageSamples::U8(actual)) => {
            compare_slices(&mut report, reference, expected, actual, |value| {
                ReferenceSample::U8(*value)
            })?;
        }
        (ImageSamples::U16(expected), ImageSamples::U16(actual)) => {
            compare_slices(&mut report, reference, expected, actual, |value| {
                ReferenceSample::U16(*value)
            })?;
        }
        (ImageSamples::F16(expected), ImageSamples::F16(actual)) => {
            compare_slices(&mut report, reference, expected, actual, |value| {
                ReferenceSample::F16(*value)
            })?;
        }
        (ImageSamples::F32(expected), ImageSamples::F32(actual)) => {
            compare_slices(&mut report, reference, expected, actual, |value| {
                ReferenceSample::F32(*value)
            })?;
        }
        _ => {
            return Err(Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "equal image descriptors contain different sample representations",
            )
            .with_context(ErrorContext::new(COMPONENT, "compare_image_samples")));
        }
    }
    Ok(report)
}

fn compare_slices<T, F>(
    report: &mut ReferenceComparison,
    reference: &Image,
    expected: &[T],
    actual: &[T],
    sample: F,
) -> Result<()>
where
    F: Fn(&T) -> ReferenceSample,
{
    if expected.len() != actual.len() {
        return Err(Error::new(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "equal image descriptors contain different sample counts",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "compare_image_samples")
                .with_field("actual", actual.len().to_string())
                .with_field("expected", expected.len().to_string()),
        ));
    }

    for (index, (expected, actual)) in expected.iter().zip(actual).enumerate() {
        let expected = sample(expected);
        let actual = sample(actual);
        let comparison = compare_sample(expected, actual, report.tolerance);
        report.compared_samples += 1;
        if let Some(error) = comparison.absolute_error {
            report.maximum_absolute_error = Some(
                report
                    .maximum_absolute_error
                    .map_or(error, |maximum| maximum.max(error)),
            );
        }
        if !comparison.matches {
            report.mismatched_samples += 1;
            if report.first_mismatch.is_none() {
                let (x, y, channel) = sample_position(reference, index)?;
                report.first_mismatch = Some(SampleMismatch {
                    sample_index: index,
                    x,
                    y,
                    channel,
                    expected,
                    actual,
                    absolute_error: comparison.absolute_error,
                });
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug)]
struct SampleComparison {
    matches: bool,
    absolute_error: Option<f64>,
}

fn compare_sample(
    expected: ReferenceSample,
    actual: ReferenceSample,
    tolerance: ReferenceTolerance,
) -> SampleComparison {
    match (expected, actual) {
        (ReferenceSample::U8(expected), ReferenceSample::U8(actual)) => {
            integer_comparison(expected == actual, f64::from(expected.abs_diff(actual)))
        }
        (ReferenceSample::U16(expected), ReferenceSample::U16(actual)) => {
            integer_comparison(expected == actual, f64::from(expected.abs_diff(actual)))
        }
        (ReferenceSample::F16(expected), ReferenceSample::F16(actual)) => float_comparison(
            u32::from(expected),
            u32::from(actual),
            f64::from(f16::from_bits(expected).to_f32()),
            f64::from(f16::from_bits(actual).to_f32()),
            tolerance,
        ),
        (ReferenceSample::F32(expected), ReferenceSample::F32(actual)) => float_comparison(
            expected,
            actual,
            f64::from(f32::from_bits(expected)),
            f64::from(f32::from_bits(actual)),
            tolerance,
        ),
        _ => SampleComparison {
            matches: false,
            absolute_error: Some(f64::INFINITY),
        },
    }
}

fn integer_comparison(matches: bool, absolute_error: f64) -> SampleComparison {
    SampleComparison {
        matches,
        absolute_error: Some(absolute_error),
    }
}

fn float_comparison(
    expected_bits: u32,
    actual_bits: u32,
    expected: f64,
    actual: f64,
    tolerance: ReferenceTolerance,
) -> SampleComparison {
    if expected_bits == actual_bits {
        return SampleComparison {
            matches: true,
            absolute_error: if expected.is_finite() {
                Some(0.0)
            } else {
                None
            },
        };
    }
    if tolerance.is_exact() {
        return SampleComparison {
            matches: false,
            absolute_error: special_or_absolute_error(expected, actual),
        };
    }
    if expected.is_nan() || actual.is_nan() {
        return SampleComparison {
            matches: expected.is_nan() && actual.is_nan(),
            absolute_error: if expected.is_nan() && actual.is_nan() {
                None
            } else {
                Some(f64::INFINITY)
            },
        };
    }
    if !expected.is_finite() || !actual.is_finite() {
        return SampleComparison {
            matches: false,
            absolute_error: Some(f64::INFINITY),
        };
    }

    let absolute_error = (expected - actual).abs();
    SampleComparison {
        matches: absolute_error
            <= tolerance
                .maximum_absolute_error()
                .expect("nonexact tolerances always have an absolute limit"),
        absolute_error: Some(absolute_error),
    }
}

fn special_or_absolute_error(expected: f64, actual: f64) -> Option<f64> {
    if expected.is_finite() && actual.is_finite() {
        Some((expected - actual).abs())
    } else {
        Some(f64::INFINITY)
    }
}

fn sample_position(reference: &Image, sample_index: usize) -> Result<(i32, i32, ChannelIndex)> {
    let descriptor = reference.descriptor();
    let channel_count = descriptor.channels().len();
    if channel_count == 0 {
        return Err(Error::new(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "image reference contains no channels",
        )
        .with_context(ErrorContext::new(COMPONENT, "locate_sample_mismatch")));
    }
    let width = usize::try_from(descriptor.data_window().width()).map_err(|_| {
        Error::new(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "image width cannot be represented while locating a mismatch",
        )
        .with_context(ErrorContext::new(COMPONENT, "locate_sample_mismatch"))
    })?;
    let pixel = sample_index / channel_count;
    let x_offset = pixel % width;
    let y_offset = pixel / width;
    let x = i64::from(descriptor.data_window().min_x())
        + i64::try_from(x_offset).map_err(|_| position_overflow())?;
    let y = i64::from(descriptor.data_window().min_y())
        + i64::try_from(y_offset).map_err(|_| position_overflow())?;
    Ok((
        i32::try_from(x).map_err(|_| position_overflow())?,
        i32::try_from(y).map_err(|_| position_overflow())?,
        ChannelIndex::new(sample_index % channel_count),
    ))
}

fn position_overflow() -> Error {
    Error::new(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "sample position overflowed its validated image bounds",
    )
    .with_context(ErrorContext::new(COMPONENT, "locate_sample_mismatch"))
}

fn invalid(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
