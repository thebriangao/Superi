//! Wide-gamut scene-linear RGB conversion and explicit gamut mapping.
//!
//! Primary conversion is performed in binary64 using normalized primary
//! matrices. Bradford chromatic adaptation is an explicit transform choice.
//! Gamut mapping is also explicit and deliberately does not tone map values
//! above one, so scene-linear HDR headroom remains available to later output
//! transforms.

use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::PixelFormat;
use superi_image::value::{Image, ImageDescriptor, ImageSamples};

use crate::working_space::{WorkingImage, WorkingImageF32, WorkingSpace};

const COMPONENT: &str = "superi-color.gamut";
const MATRIX_EPSILON: f64 = 1.0e-14;
const IDENTITY: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
const BRADFORD: [[f64; 3]; 3] = [
    [0.8951, 0.2664, -0.1614],
    [-0.7502, 1.7135, 0.0367],
    [0.0389, -0.0685, 1.0296],
];

/// A finite CIE 1931 xy chromaticity.
///
/// RGB primaries are allowed outside the spectral locus. This is required for
/// ACES AP0, whose blue primary has a negative y coordinate.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Chromaticity {
    x: f64,
    y: f64,
}

impl Chromaticity {
    const fn declared(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Returns the CIE x coordinate.
    #[must_use]
    pub const fn x(self) -> f64 {
        self.x
    }

    /// Returns the CIE y coordinate.
    #[must_use]
    pub const fn y(self) -> f64 {
        self.y
    }

    fn xyz_with_unit_y(self, role: &'static str) -> Result<[f64; 3]> {
        if !self.x.is_finite() || !self.y.is_finite() || self.y.abs() <= MATRIX_EPSILON {
            return Err(invalid(
                "derive_primary_matrix",
                "chromaticity must be finite and have a nonzero y coordinate",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "inspect_chromaticity")
                    .with_field("role", role)
                    .with_field("x", self.x.to_string())
                    .with_field("y", self.y.to_string()),
            ));
        }
        let xyz = [self.x / self.y, 1.0, (1.0 - self.x - self.y) / self.y];
        validate_vector("derive_primary_matrix", "chromaticity_xyz", xyz)?;
        Ok(xyz)
    }
}

/// Published RGB primaries and reference white for one supported tag.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RgbColorimetry {
    primaries: ColorPrimaries,
    red: Chromaticity,
    green: Chromaticity,
    blue: Chromaticity,
    white: Chromaticity,
}

impl RgbColorimetry {
    /// Resolves the immutable colorimetry for a supported primary tag.
    pub fn from_primaries(primaries: ColorPrimaries) -> Result<Self> {
        let (red, green, blue, white) = match primaries {
            ColorPrimaries::Bt709 => (
                (0.640, 0.330),
                (0.300, 0.600),
                (0.150, 0.060),
                (0.3127, 0.3290),
            ),
            ColorPrimaries::Bt2020 => (
                (0.708, 0.292),
                (0.170, 0.797),
                (0.131, 0.046),
                (0.3127, 0.3290),
            ),
            ColorPrimaries::DisplayP3 => (
                (0.680, 0.320),
                (0.265, 0.690),
                (0.150, 0.060),
                (0.3127, 0.3290),
            ),
            ColorPrimaries::AcesAp0 => (
                (0.73470, 0.26530),
                (0.00000, 1.00000),
                (0.00010, -0.07700),
                (0.32168, 0.33767),
            ),
            ColorPrimaries::AcesAp1 => (
                (0.713, 0.293),
                (0.165, 0.830),
                (0.128, 0.044),
                (0.32168, 0.33767),
            ),
            _ => {
                return Err(invalid(
                    "resolve_colorimetry",
                    "color primaries must identify a supported RGB colorimetry",
                )
                .with_context(primary_context("source_or_destination", primaries)));
            }
        };
        Ok(Self {
            primaries,
            red: Chromaticity::declared(red.0, red.1),
            green: Chromaticity::declared(green.0, green.1),
            blue: Chromaticity::declared(blue.0, blue.1),
            white: Chromaticity::declared(white.0, white.1),
        })
    }

    /// Returns the stable primary tag associated with this definition.
    #[must_use]
    pub const fn primaries(self) -> ColorPrimaries {
        self.primaries
    }

    /// Returns the red primary chromaticity.
    #[must_use]
    pub const fn red(self) -> Chromaticity {
        self.red
    }

    /// Returns the green primary chromaticity.
    #[must_use]
    pub const fn green(self) -> Chromaticity {
        self.green
    }

    /// Returns the blue primary chromaticity.
    #[must_use]
    pub const fn blue(self) -> Chromaticity {
        self.blue
    }

    /// Returns the reference-white chromaticity.
    #[must_use]
    pub const fn white(self) -> Chromaticity {
        self.white
    }

    fn rgb_to_xyz(self) -> Result<[[f64; 3]; 3]> {
        let red = self.red.xyz_with_unit_y("red")?;
        let green = self.green.xyz_with_unit_y("green")?;
        let blue = self.blue.xyz_with_unit_y("blue")?;
        let primaries = [
            [red[0], green[0], blue[0]],
            [red[1], green[1], blue[1]],
            [red[2], green[2], blue[2]],
        ];
        let white = self.white.xyz_with_unit_y("white")?;
        let scale = matrix_vector(inverse(primaries, "invert_primary_chromaticities")?, white);
        validate_vector("derive_primary_matrix", "primary_scale", scale)?;
        let normalized = [
            [
                primaries[0][0] * scale[0],
                primaries[0][1] * scale[1],
                primaries[0][2] * scale[2],
            ],
            [
                primaries[1][0] * scale[0],
                primaries[1][1] * scale[1],
                primaries[1][2] * scale[2],
            ],
            [
                primaries[2][0] * scale[0],
                primaries[2][1] * scale[1],
                primaries[2][2] * scale[2],
            ],
        ];
        validate_matrix("derive_primary_matrix", normalized)?;
        Ok(normalized)
    }
}

/// The explicit reference-white adaptation used during primary conversion.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChromaticAdaptation {
    /// Keep XYZ tristimulus values unchanged across different reference whites.
    None,
    /// Apply linear Bradford cone-response scaling.
    Bradford,
}

/// Explicit handling for negative destination RGB components.
///
/// All policies preserve values above one. Tone mapping, display range, and
/// legal-range encoding belong to later output transforms.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GamutMapping {
    /// Retain all finite scene-linear components without gamut mapping.
    Preserve,
    /// Clamp only negative components to zero while retaining HDR headroom.
    ClipNegative,
    /// Compress chroma toward the neutral axis, preserving CIE Y and headroom.
    PreserveLuminance,
}

/// Three finite scene-linear RGB components.
#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(transparent)]
pub struct LinearRgb([f64; 3]);

impl LinearRgb {
    /// Creates an RGB value after rejecting nonfinite components.
    pub fn new(values: [f64; 3]) -> Result<Self> {
        validate_vector("construct_linear_rgb", "rgb", values)?;
        Ok(Self(values))
    }

    /// Returns the scene-linear components.
    #[must_use]
    pub const fn values(self) -> [f64; 3] {
        self.0
    }
}

/// A deterministic transform between two supported RGB primary sets.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WideGamutTransform {
    source: RgbColorimetry,
    destination: RgbColorimetry,
    adaptation: ChromaticAdaptation,
    mapping: GamutMapping,
    matrix: [[f64; 3]; 3],
    destination_luma: [f64; 3],
}

impl WideGamutTransform {
    /// Builds a scene-linear primary conversion with explicit adaptation and mapping.
    pub fn new(
        source: ColorPrimaries,
        destination: ColorPrimaries,
        adaptation: ChromaticAdaptation,
        mapping: GamutMapping,
    ) -> Result<Self> {
        let source = RgbColorimetry::from_primaries(source)?;
        let destination = RgbColorimetry::from_primaries(destination)?;
        let source_rgb_to_xyz = source.rgb_to_xyz()?;
        let destination_rgb_to_xyz = destination.rgb_to_xyz()?;
        let destination_luma = destination_rgb_to_xyz[1];
        let matrix = if source.primaries == destination.primaries {
            IDENTITY
        } else {
            let adaptation_matrix = adaptation_matrix(source.white, destination.white, adaptation)?;
            matrix_multiply(
                inverse(destination_rgb_to_xyz, "invert_destination_primary_matrix")?,
                matrix_multiply(adaptation_matrix, source_rgb_to_xyz),
            )
        };
        validate_matrix("compose_wide_gamut_transform", matrix)?;
        validate_vector(
            "compose_wide_gamut_transform",
            "destination_luma",
            destination_luma,
        )?;
        Ok(Self {
            source,
            destination,
            adaptation,
            mapping,
            matrix,
            destination_luma,
        })
    }

    /// Returns the source primary definition.
    #[must_use]
    pub const fn source(self) -> RgbColorimetry {
        self.source
    }

    /// Returns the destination primary definition.
    #[must_use]
    pub const fn destination(self) -> RgbColorimetry {
        self.destination
    }

    /// Returns the configured reference-white adaptation.
    #[must_use]
    pub const fn adaptation(self) -> ChromaticAdaptation {
        self.adaptation
    }

    /// Returns the configured negative-gamut policy.
    #[must_use]
    pub const fn mapping(self) -> GamutMapping {
        self.mapping
    }

    /// Returns the row-major binary64 RGB conversion matrix.
    #[must_use]
    pub const fn matrix(self) -> [[f64; 3]; 3] {
        self.matrix
    }

    /// Returns destination RGB coefficients for CIE Y.
    #[must_use]
    pub const fn destination_luma(self) -> [f64; 3] {
        self.destination_luma
    }

    /// Applies the primary conversion followed by the explicit gamut policy.
    pub fn apply_rgb(self, rgb: LinearRgb) -> Result<LinearRgb> {
        let converted = matrix_vector(self.matrix, rgb.values());
        validate_vector("apply_primary_transform", "converted_rgb", converted)?;
        LinearRgb::new(self.map_gamut(converted)?)
    }

    /// Transforms one premultiplied scene-linear RGBA value.
    ///
    /// Nonlinear gamut mapping is applied to unassociated RGB and then
    /// reassociated. Alpha is retained bit for bit.
    pub fn apply_premultiplied_rgba(self, rgba: [f64; 4]) -> Result<[f64; 4]> {
        if rgba.iter().any(|component| !component.is_finite()) {
            return Err(invalid(
                "apply_premultiplied_transform",
                "premultiplied RGBA components must be finite",
            ));
        }
        let alpha = rgba[3];
        if alpha < 0.0 {
            return Err(invalid(
                "apply_premultiplied_transform",
                "premultiplied alpha must be non-negative",
            ));
        }
        let associated = [rgba[0], rgba[1], rgba[2]];
        if alpha == 0.0 {
            if associated.iter().any(|component| *component != 0.0) {
                return Err(invalid(
                    "apply_premultiplied_transform",
                    "zero-alpha premultiplied RGB must also be zero",
                ));
            }
            return Ok(rgba);
        }

        let output = if self.mapping == GamutMapping::Preserve {
            self.apply_rgb(LinearRgb::new(associated)?)?.values()
        } else {
            let straight = associated.map(|component| component / alpha);
            self.apply_rgb(LinearRgb::new(straight)?)?
                .values()
                .map(|component| component * alpha)
        };
        validate_vector("apply_premultiplied_transform", "output_rgb", output)?;
        Ok([output[0], output[1], output[2], alpha])
    }

    /// Transforms a binary32 working image and preserves its artifact metadata.
    pub fn apply_working_f32(self, source: &WorkingImageF32) -> Result<WorkingImageF32> {
        self.validate_working_source(source.space())?;
        let destination = self.destination_working_space()?;
        let samples = source.image().samples();
        let mut output = Vec::with_capacity(samples.len());
        for pixel_start in (0..samples.len()).step_by(4) {
            let rgba = [
                sample_as_f64(samples, pixel_start)?,
                sample_as_f64(samples, pixel_start + 1)?,
                sample_as_f64(samples, pixel_start + 2)?,
                sample_as_f64(samples, pixel_start + 3)?,
            ];
            let transformed = self.apply_premultiplied_rgba(rgba)?;
            for component in transformed {
                let narrowed = component as f32;
                if !narrowed.is_finite() {
                    return Err(invalid(
                        "transform_working_image",
                        "transformed component cannot be represented as binary32",
                    )
                    .with_context(transform_context(self)));
                }
                output.push(narrowed);
            }
        }

        let source_descriptor = source.image().descriptor();
        let color_tags = source_descriptor
            .color_tags()
            .clone()
            .with_interpretation(destination.color_space());
        let descriptor = ImageDescriptor::new_with_color_tags(
            source_descriptor.data_window(),
            source_descriptor.display_window(),
            PixelFormat::Rgba32Float,
            color_tags,
            source_descriptor.alpha_mode(),
        )?
        .with_channels(source_descriptor.channels().clone())?;
        let image = Image::new_with_metadata(
            descriptor,
            ImageSamples::from_f32(output),
            source.image().metadata().clone(),
        )?;
        WorkingImageF32::new(destination, image)
    }

    /// Promotes, transforms, and requantizes a canonical binary16 working image.
    pub fn apply_working_f16(self, source: &WorkingImage) -> Result<WorkingImage> {
        self.validate_working_source(source.space())?;
        self.apply_working_f32(&source.promote_f32()?)?
            .quantize_f16()
    }

    fn validate_working_source(self, space: WorkingSpace) -> Result<()> {
        if space.color_space().primaries() != self.source.primaries {
            return Err(invalid(
                "transform_working_image",
                "working image primaries do not match the transform source",
            )
            .with_context(transform_context(self))
            .with_context(primary_context(
                "actual_working_primaries",
                space.color_space().primaries(),
            )));
        }
        Ok(())
    }

    fn destination_working_space(self) -> Result<WorkingSpace> {
        WorkingSpace::new(ColorSpace::new(
            self.destination.primaries,
            TransferFunction::Linear,
            MatrixCoefficients::Rgb,
            ColorRange::Full,
        ))
        .map_err(|error| error.with_context(transform_context(self)))
    }

    fn map_gamut(self, rgb: [f64; 3]) -> Result<[f64; 3]> {
        match self.mapping {
            GamutMapping::Preserve => Ok(rgb),
            GamutMapping::ClipNegative => Ok(rgb.map(|component| component.max(0.0))),
            GamutMapping::PreserveLuminance => {
                let minimum = rgb.into_iter().fold(f64::INFINITY, f64::min);
                if minimum >= 0.0 {
                    return Ok(rgb);
                }
                let luminance = dot(self.destination_luma, rgb);
                if !luminance.is_finite() || luminance <= 0.0 {
                    return Err(invalid(
                        "map_destination_gamut",
                        "luminance-preserving gamut mapping requires positive destination luminance",
                    )
                    .with_context(transform_context(self)));
                }
                let scale = luminance / (luminance - minimum);
                if !scale.is_finite() || !(0.0..=1.0).contains(&scale) {
                    return Err(invalid(
                        "map_destination_gamut",
                        "gamut compression scale is outside its valid domain",
                    )
                    .with_context(transform_context(self)));
                }
                let mut mapped = rgb.map(|component| luminance + scale * (component - luminance));
                for component in &mut mapped {
                    if *component < 0.0 && component.abs() <= 1.0e-12 {
                        *component = 0.0;
                    }
                }
                validate_vector("map_destination_gamut", "mapped_rgb", mapped)?;
                Ok(mapped)
            }
        }
    }
}

fn adaptation_matrix(
    source_white: Chromaticity,
    destination_white: Chromaticity,
    adaptation: ChromaticAdaptation,
) -> Result<[[f64; 3]; 3]> {
    if adaptation == ChromaticAdaptation::None || source_white == destination_white {
        return Ok(IDENTITY);
    }
    let source_xyz = source_white.xyz_with_unit_y("source_white")?;
    let destination_xyz = destination_white.xyz_with_unit_y("destination_white")?;
    let source_cone = matrix_vector(BRADFORD, source_xyz);
    let destination_cone = matrix_vector(BRADFORD, destination_xyz);
    validate_vector("adapt_reference_white", "source_cone", source_cone)?;
    validate_vector(
        "adapt_reference_white",
        "destination_cone",
        destination_cone,
    )?;
    if source_cone
        .iter()
        .any(|response| response.abs() <= MATRIX_EPSILON)
    {
        return Err(invalid(
            "adapt_reference_white",
            "source white has a zero Bradford cone response",
        ));
    }
    let scale = [
        [destination_cone[0] / source_cone[0], 0.0, 0.0],
        [0.0, destination_cone[1] / source_cone[1], 0.0],
        [0.0, 0.0, destination_cone[2] / source_cone[2]],
    ];
    let matrix = matrix_multiply(
        inverse(BRADFORD, "invert_bradford_matrix")?,
        matrix_multiply(scale, BRADFORD),
    );
    validate_matrix("adapt_reference_white", matrix)?;
    Ok(matrix)
}

fn sample_as_f64(samples: &ImageSamples, index: usize) -> Result<f64> {
    let value = samples.float_value(index).ok_or_else(|| {
        invalid(
            "transform_working_image",
            "working image samples must be floating point",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "inspect_sample")
                .with_field("sample_index", index.to_string()),
        )
    })? as f64;
    if !value.is_finite() {
        return Err(invalid(
            "transform_working_image",
            "working image color samples must be finite",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "inspect_sample")
                .with_field("sample_index", index.to_string()),
        ));
    }
    Ok(value)
}

fn inverse(matrix: [[f64; 3]; 3], operation: &'static str) -> Result<[[f64; 3]; 3]> {
    let determinant = matrix[0][0] * (matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1])
        - matrix[0][1] * (matrix[1][0] * matrix[2][2] - matrix[1][2] * matrix[2][0])
        + matrix[0][2] * (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0]);
    if !determinant.is_finite() || determinant.abs() <= MATRIX_EPSILON {
        return Err(invalid(operation, "color transform matrix is singular"));
    }
    let reciprocal = determinant.recip();
    let inverse = [
        [
            (matrix[1][1] * matrix[2][2] - matrix[1][2] * matrix[2][1]) * reciprocal,
            (matrix[0][2] * matrix[2][1] - matrix[0][1] * matrix[2][2]) * reciprocal,
            (matrix[0][1] * matrix[1][2] - matrix[0][2] * matrix[1][1]) * reciprocal,
        ],
        [
            (matrix[1][2] * matrix[2][0] - matrix[1][0] * matrix[2][2]) * reciprocal,
            (matrix[0][0] * matrix[2][2] - matrix[0][2] * matrix[2][0]) * reciprocal,
            (matrix[0][2] * matrix[1][0] - matrix[0][0] * matrix[1][2]) * reciprocal,
        ],
        [
            (matrix[1][0] * matrix[2][1] - matrix[1][1] * matrix[2][0]) * reciprocal,
            (matrix[0][1] * matrix[2][0] - matrix[0][0] * matrix[2][1]) * reciprocal,
            (matrix[0][0] * matrix[1][1] - matrix[0][1] * matrix[1][0]) * reciprocal,
        ],
    ];
    validate_matrix(operation, inverse)?;
    Ok(inverse)
}

fn matrix_multiply(left: [[f64; 3]; 3], right: [[f64; 3]; 3]) -> [[f64; 3]; 3] {
    let mut output = [[0.0; 3]; 3];
    for row in 0..3 {
        for column in 0..3 {
            output[row][column] = left[row][0] * right[0][column]
                + left[row][1] * right[1][column]
                + left[row][2] * right[2][column];
        }
    }
    output
}

fn matrix_vector(matrix: [[f64; 3]; 3], vector: [f64; 3]) -> [f64; 3] {
    [
        dot(matrix[0], vector),
        dot(matrix[1], vector),
        dot(matrix[2], vector),
    ]
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}

fn validate_matrix(operation: &'static str, matrix: [[f64; 3]; 3]) -> Result<()> {
    if matrix.into_iter().flatten().any(|value| !value.is_finite()) {
        return Err(invalid(operation, "color transform matrix must be finite"));
    }
    Ok(())
}

fn validate_vector(operation: &'static str, field: &'static str, values: [f64; 3]) -> Result<()> {
    if values.into_iter().any(|value| !value.is_finite()) {
        return Err(
            invalid(operation, "color components must be finite").with_context(
                ErrorContext::new(COMPONENT, "inspect_color_components").with_field("field", field),
            ),
        );
    }
    Ok(())
}

fn transform_context(transform: WideGamutTransform) -> ErrorContext {
    ErrorContext::new(COMPONENT, "inspect_transform")
        .with_field("source_primaries", transform.source.primaries.code())
        .with_field(
            "destination_primaries",
            transform.destination.primaries.code(),
        )
        .with_field("adaptation", adaptation_code(transform.adaptation))
        .with_field("gamut_mapping", mapping_code(transform.mapping))
}

fn primary_context(role: &'static str, primaries: ColorPrimaries) -> ErrorContext {
    ErrorContext::new(COMPONENT, "inspect_primaries")
        .with_field("role", role)
        .with_field("primaries", primaries.code())
}

const fn adaptation_code(adaptation: ChromaticAdaptation) -> &'static str {
    match adaptation {
        ChromaticAdaptation::None => "none",
        ChromaticAdaptation::Bradford => "bradford",
    }
}

const fn mapping_code(mapping: GamutMapping) -> &'static str {
    match mapping {
        GamutMapping::Preserve => "preserve",
        GamutMapping::ClipNegative => "clip_negative",
        GamutMapping::PreserveLuminance => "preserve_luminance",
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
