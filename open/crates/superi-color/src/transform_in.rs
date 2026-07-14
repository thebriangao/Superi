//! Explicit transforms from supported RGB inputs into scene-linear working space.
//!
//! Input transforms are ordered as source numeric interpretation, alpha
//! unassociation, transfer decoding, primary conversion, explicit gamut policy,
//! alpha association, and canonical working-image storage. Source categories
//! are part of the public contract so a display signal cannot silently be
//! interpreted as camera or scene light.

use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{AlphaMode, PixelFormat, PixelModel};
use superi_image::value::{Image, ImageDescriptor, ImageSamples};

use crate::gamut::{ChromaticAdaptation, GamutMapping, LinearRgb, WideGamutTransform};
use crate::hdr::{decode_relative_transfer, pq_eotf, EncodedSignal, Nits, NormalizedSignal};
use crate::working_space::{WorkingImage, WorkingImageF32, WorkingSpace};

const COMPONENT: &str = "superi-color.transform-in";
const BINARY16_MAX_FINITE: f32 = 65_504.0;

/// The semantic source family selected for an input transform.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum InputSourceKind {
    /// Camera interchange data in a supported linear ACES encoding.
    Camera,
    /// An image whose nonlinear signal represents display light.
    DisplayReferred,
    /// An image whose nonlinear signal represents scene light.
    SceneReferred,
}

impl InputSourceKind {
    /// Returns the stable diagnostic code for this source family.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Camera => "camera",
            Self::DisplayReferred => "display_referred",
            Self::SceneReferred => "scene_referred",
        }
    }
}

/// Explicit policy inputs used to compose an input transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InputTransformOptions {
    chromatic_adaptation: ChromaticAdaptation,
    gamut_mapping: GamutMapping,
    pq_reference_white: Option<Nits>,
}

impl InputTransformOptions {
    /// Creates the default preserving transform policy.
    ///
    /// Bradford adaptation is used between different reference whites. Negative
    /// and above-one scene-linear values are retained. PQ reference white stays
    /// absent until explicitly provided by the caller.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            chromatic_adaptation: ChromaticAdaptation::Bradford,
            gamut_mapping: GamutMapping::Preserve,
            pq_reference_white: None,
        }
    }

    /// Replaces the reference-white chromatic adaptation policy.
    #[must_use]
    pub const fn with_chromatic_adaptation(
        mut self,
        chromatic_adaptation: ChromaticAdaptation,
    ) -> Self {
        self.chromatic_adaptation = chromatic_adaptation;
        self
    }

    /// Replaces the destination gamut policy.
    #[must_use]
    pub const fn with_gamut_mapping(mut self, gamut_mapping: GamutMapping) -> Self {
        self.gamut_mapping = gamut_mapping;
        self
    }

    /// Sets the absolute PQ luminance that becomes relative working value one.
    #[must_use]
    pub const fn with_pq_reference_white(mut self, reference_white: Nits) -> Self {
        self.pq_reference_white = Some(reference_white);
        self
    }

    /// Returns the configured chromatic adaptation policy.
    #[must_use]
    pub const fn chromatic_adaptation(self) -> ChromaticAdaptation {
        self.chromatic_adaptation
    }

    /// Returns the configured gamut policy.
    #[must_use]
    pub const fn gamut_mapping(self) -> GamutMapping {
        self.gamut_mapping
    }

    /// Returns the explicit PQ reference white, when configured.
    #[must_use]
    pub const fn pq_reference_white(self) -> Option<Nits> {
        self.pq_reference_white
    }
}

impl Default for InputTransformOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// A deterministic transform from one explicit RGB interpretation to working space.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InputColorTransform {
    source_kind: InputSourceKind,
    source: ColorSpace,
    destination: WorkingSpace,
    options: InputTransformOptions,
    gamut: WideGamutTransform,
}

impl InputColorTransform {
    /// Composes and validates an explicit input transform.
    ///
    /// Input numeric range and YUV matrix conversion are intentionally outside
    /// this boundary. Sources must already be full-range RGB, and unsupported or
    /// ambiguous combinations fail instead of receiving an inferred transform.
    pub fn new(
        source_kind: InputSourceKind,
        source: ColorSpace,
        destination: WorkingSpace,
        options: InputTransformOptions,
    ) -> Result<Self> {
        validate_source(source_kind, source, options)?;
        let gamut = WideGamutTransform::new(
            source.primaries(),
            destination.color_space().primaries(),
            options.chromatic_adaptation,
            options.gamut_mapping,
        )
        .map_err(|error| error.with_context(transform_context(source_kind, source, destination)))?;
        Ok(Self {
            source_kind,
            source,
            destination,
            options,
            gamut,
        })
    }

    /// Returns the explicit source family.
    #[must_use]
    pub const fn source_kind(self) -> InputSourceKind {
        self.source_kind
    }

    /// Returns the exact source color interpretation.
    #[must_use]
    pub const fn source(self) -> ColorSpace {
        self.source
    }

    /// Returns the destination working space.
    #[must_use]
    pub const fn destination(self) -> WorkingSpace {
        self.destination
    }

    /// Returns the transform policy.
    #[must_use]
    pub const fn options(self) -> InputTransformOptions {
        self.options
    }

    /// Applies the transform and emits canonical binary16 working storage.
    pub fn apply(self, source: &Image) -> Result<WorkingImage> {
        let transformed = self.apply_f32(source)?;
        validate_binary16_storage(&transformed)?;
        transformed.quantize_f16()
    }

    /// Applies the transform into binary32 working computation storage.
    ///
    /// Spatial windows, source color payloads, and image metadata are retained.
    /// The authoritative interpretation becomes the destination working space.
    pub fn apply_f32(self, source: &Image) -> Result<WorkingImageF32> {
        validate_image(self, source)?;
        let descriptor = source.descriptor();
        let component_count = descriptor.channels().len();
        let mut output = Vec::with_capacity(
            source
                .samples()
                .len()
                .checked_div(component_count)
                .and_then(|pixels| pixels.checked_mul(4))
                .ok_or_else(|| invalid("transform_input_image", "image sample count overflowed"))?,
        );

        for pixel_start in (0..source.samples().len()).step_by(component_count) {
            let (encoded, alpha) = read_rgba(source, pixel_start)?;
            let straight_encoded = unassociate(encoded, alpha, descriptor.alpha_mode())?;
            let linear = [
                self.decode_component(straight_encoded[0])?,
                self.decode_component(straight_encoded[1])?,
                self.decode_component(straight_encoded[2])?,
            ];
            let converted = self.gamut.apply_rgb(LinearRgb::new(linear)?)?.values();
            let premultiplied = converted.map(|component| component * alpha);
            for component in [premultiplied[0], premultiplied[1], premultiplied[2], alpha] {
                let narrowed = component as f32;
                if !narrowed.is_finite() {
                    return Err(invalid(
                        "transform_input_image",
                        "transformed component cannot be represented as binary32",
                    )
                    .with_context(transform_context(
                        self.source_kind,
                        self.source,
                        self.destination,
                    )));
                }
                output.push(narrowed);
            }
        }

        let color_tags = descriptor
            .color_tags()
            .clone()
            .with_interpretation(self.destination.color_space());
        let output_descriptor = ImageDescriptor::new_with_color_tags(
            descriptor.data_window(),
            descriptor.display_window(),
            PixelFormat::Rgba32Float,
            color_tags,
            AlphaMode::Premultiplied,
        )?;
        let image = Image::new_with_metadata(
            output_descriptor,
            ImageSamples::from_f32(output),
            source.metadata().clone(),
        )?;
        WorkingImageF32::new(self.destination, image)
    }

    fn decode_component(self, encoded: f64) -> Result<f64> {
        let decoded = if self.source.transfer() == TransferFunction::Pq {
            let reference_white = self
                .options
                .pq_reference_white
                .expect("validated PQ transforms have a reference white")
                .value();
            pq_eotf(NormalizedSignal::new(encoded)?)?.value() / reference_white
        } else {
            decode_relative_transfer(self.source.transfer(), EncodedSignal::new(encoded)?)?.value()
        };
        if !decoded.is_finite() {
            return Err(invalid(
                "decode_input_transfer",
                "decoded source component must be finite",
            ));
        }
        Ok(decoded)
    }
}

fn validate_source(
    kind: InputSourceKind,
    source: ColorSpace,
    options: InputTransformOptions,
) -> Result<()> {
    let context = source_context(kind, source);
    if source.matrix() != MatrixCoefficients::Rgb || source.range() != ColorRange::Full {
        return Err(unsupported(
            "compose_input_transform",
            "input color transforms require prior conversion to full-range RGB",
        )
        .with_context(context));
    }
    if source.primaries() == ColorPrimaries::Unspecified
        || source.transfer() == TransferFunction::Unspecified
    {
        return Err(invalid(
            "compose_input_transform",
            "input primaries and transfer function must be explicitly declared",
        )
        .with_context(context));
    }
    if kind == InputSourceKind::DisplayReferred
        && matches!(
            source.transfer(),
            TransferFunction::Bt709
                | TransferFunction::Bt2020TenBit
                | TransferFunction::Bt2020TwelveBit
        )
    {
        return Err(unsupported(
            "compose_input_transform",
            "display-referred BT.709 and BT.2020 EOTFs are not implemented and cannot use the scene OETF decode path",
        )
        .with_context(context));
    }

    let supported = match kind {
        InputSourceKind::Camera => {
            source.transfer() == TransferFunction::Linear
                && matches!(
                    source.primaries(),
                    ColorPrimaries::AcesAp0 | ColorPrimaries::AcesAp1
                )
        }
        InputSourceKind::DisplayReferred => matches!(
            source.transfer(),
            TransferFunction::Srgb
                | TransferFunction::Gamma22
                | TransferFunction::Gamma24
                | TransferFunction::Pq
        ),
        InputSourceKind::SceneReferred => {
            matches!(
                source.transfer(),
                TransferFunction::Linear
                    | TransferFunction::Bt709
                    | TransferFunction::Bt2020TenBit
                    | TransferFunction::Bt2020TwelveBit
                    | TransferFunction::Hlg
            ) && !matches!(
                source.primaries(),
                ColorPrimaries::AcesAp0 | ColorPrimaries::AcesAp1
            )
        }
    };
    if !supported {
        return Err(unsupported(
            "compose_input_transform",
            "source interpretation is not supported for the selected input family",
        )
        .with_context(context));
    }

    match (source.transfer(), options.pq_reference_white) {
        (TransferFunction::Pq, Some(reference_white)) if reference_white.value() > 0.0 => {}
        (TransferFunction::Pq, _) => {
            return Err(invalid(
                "compose_input_transform",
                "PQ input transforms require a positive reference-white luminance",
            )
            .with_context(context));
        }
        (_, Some(_)) => {
            return Err(invalid(
                "compose_input_transform",
                "PQ reference white may be set only for PQ source signals",
            )
            .with_context(context));
        }
        (_, None) => {}
    }
    Ok(())
}

fn validate_binary16_storage(image: &WorkingImageF32) -> Result<()> {
    for index in 0..image.image().samples().len() {
        let value = image
            .image()
            .samples()
            .float_value(index)
            .expect("working binary32 images contain floating-point samples");
        if value.abs() > BINARY16_MAX_FINITE {
            return Err(invalid(
                "validate_binary16_storage",
                "working component exceeds the finite binary16 storage range",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "inspect_binary16_component")
                    .with_field("sample_index", index.to_string())
                    .with_field("value", value.to_string())
                    .with_field("maximum_finite_magnitude", BINARY16_MAX_FINITE.to_string()),
            ));
        }
    }
    Ok(())
}

fn validate_image(transform: InputColorTransform, source: &Image) -> Result<()> {
    let descriptor = source.descriptor();
    if descriptor.color_space() != transform.source {
        return Err(invalid(
            "transform_input_image",
            "image color interpretation does not match the input transform source",
        )
        .with_context(transform_context(
            transform.source_kind,
            transform.source,
            transform.destination,
        ))
        .with_context(source_context(
            transform.source_kind,
            descriptor.color_space(),
        )));
    }
    let supported_format = matches!(
        descriptor.pixel_format(),
        PixelFormat::Rgb8Unorm
            | PixelFormat::Bgr8Unorm
            | PixelFormat::Rgba8Unorm
            | PixelFormat::Bgra8Unorm
            | PixelFormat::Rgba16Unorm
            | PixelFormat::Rgba16Float
            | PixelFormat::Rgba32Float
    );
    if !supported_format {
        return Err(unsupported(
            "transform_input_image",
            "input image must use a supported packed RGB or RGBA representation",
        )
        .with_context(image_context(descriptor)));
    }
    if matches!(
        descriptor.pixel_format().model(),
        PixelModel::Rgb | PixelModel::Bgr
    ) && descriptor.alpha_mode() != AlphaMode::Opaque
    {
        return Err(invalid(
            "transform_input_image",
            "RGB input without an alpha channel must declare opaque alpha",
        )
        .with_context(image_context(descriptor)));
    }
    Ok(())
}

fn read_rgba(source: &Image, pixel_start: usize) -> Result<([f64; 3], f64)> {
    let descriptor = source.descriptor();
    let samples = source.samples();
    let (red, green, blue, alpha_index) = match descriptor.pixel_format().model() {
        PixelModel::Rgb => (0, 1, 2, None),
        PixelModel::Bgr => (2, 1, 0, None),
        PixelModel::Rgba => (0, 1, 2, Some(3)),
        PixelModel::Bgra => (2, 1, 0, Some(3)),
        _ => {
            return Err(unsupported(
                "read_input_pixel",
                "input pixel model must provide RGB components",
            )
            .with_context(image_context(descriptor)));
        }
    };
    let encoded = [
        sample_value(descriptor.pixel_format(), samples, pixel_start + red)?,
        sample_value(descriptor.pixel_format(), samples, pixel_start + green)?,
        sample_value(descriptor.pixel_format(), samples, pixel_start + blue)?,
    ];
    let alpha = if descriptor.alpha_mode() == AlphaMode::Opaque {
        1.0
    } else {
        sample_value(
            descriptor.pixel_format(),
            samples,
            pixel_start + alpha_index.expect("validated alpha format has an alpha component"),
        )?
    };
    if !(0.0..=1.0).contains(&alpha) {
        return Err(invalid(
            "read_input_pixel",
            "input alpha must be finite and within zero to one",
        )
        .with_context(image_context(descriptor)));
    }
    Ok((encoded, alpha))
}

fn sample_value(format: PixelFormat, samples: &ImageSamples, index: usize) -> Result<f64> {
    let value = match samples {
        ImageSamples::U8(values) => values.get(index).map(|value| f64::from(*value) / 255.0),
        ImageSamples::U16(values) => values.get(index).map(|value| f64::from(*value) / 65_535.0),
        ImageSamples::F16(_) | ImageSamples::F32(_) => samples.float_value(index).map(f64::from),
        _ => None,
    }
    .ok_or_else(|| {
        invalid(
            "read_input_sample",
            "input sample representation does not match its descriptor",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "inspect_sample")
                .with_field("sample_index", index.to_string())
                .with_field("pixel_format", format.code()),
        )
    })?;
    if !value.is_finite() {
        return Err(
            invalid("read_input_sample", "input color samples must be finite").with_context(
                ErrorContext::new(COMPONENT, "inspect_sample")
                    .with_field("sample_index", index.to_string())
                    .with_field("pixel_format", format.code()),
            ),
        );
    }
    Ok(value)
}

fn unassociate(encoded: [f64; 3], alpha: f64, mode: AlphaMode) -> Result<[f64; 3]> {
    if mode != AlphaMode::Premultiplied {
        return Ok(encoded);
    }
    if alpha == 0.0 {
        if encoded.into_iter().any(|component| component != 0.0) {
            return Err(invalid(
                "unassociate_input_alpha",
                "zero-alpha premultiplied input must also have zero RGB",
            ));
        }
        return Ok([0.0; 3]);
    }
    let straight = encoded.map(|component| component / alpha);
    if straight.into_iter().any(|component| !component.is_finite()) {
        return Err(invalid(
            "unassociate_input_alpha",
            "unassociated input color must be finite",
        ));
    }
    Ok(straight)
}

fn source_context(kind: InputSourceKind, source: ColorSpace) -> ErrorContext {
    ErrorContext::new(COMPONENT, "inspect_source")
        .with_field("source_kind", kind.code())
        .with_field("primaries", source.primaries().code())
        .with_field("transfer", source.transfer().code())
        .with_field("matrix", source.matrix().code())
        .with_field("range", source.range().code())
}

fn transform_context(
    kind: InputSourceKind,
    source: ColorSpace,
    destination: WorkingSpace,
) -> ErrorContext {
    ErrorContext::new(COMPONENT, "inspect_transform")
        .with_field("source_kind", kind.code())
        .with_field("source_primaries", source.primaries().code())
        .with_field("source_transfer", source.transfer().code())
        .with_field(
            "destination_primaries",
            destination.color_space().primaries().code(),
        )
}

fn image_context(descriptor: &ImageDescriptor) -> ErrorContext {
    ErrorContext::new(COMPONENT, "inspect_image")
        .with_field("pixel_format", descriptor.pixel_format().code())
        .with_field(
            "color_space_transfer",
            descriptor.color_space().transfer().code(),
        )
        .with_field("alpha_mode", descriptor.alpha_mode().code())
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
