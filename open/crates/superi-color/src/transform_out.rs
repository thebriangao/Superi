//! Explicit transforms from scene-linear working space to display and delivery RGB.
//!
//! Output transforms operate on promoted binary32 working images. Their order
//! is alpha unassociation, primary conversion, explicit gamut policy, transfer
//! encoding, alpha reassociation, and authoritative output tagging. Matrix,
//! legal-range, integer, and texture packing remain separate storage stages.

use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_image::value::{Image, ImageDescriptor, ImageSamples};

use crate::gamut::{ChromaticAdaptation, GamutMapping, LinearRgb, WideGamutTransform};
use crate::hdr::{encode_relative_transfer, pq_inverse_eotf, Nits, RelativeLight};
use crate::working_space::{WorkingImage, WorkingImageF32, WorkingSpace};

const COMPONENT: &str = "superi-color.transform-out";

/// The semantic consumer of an output color transform.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum OutputTargetKind {
    /// A presentation image intended for a display or native viewport.
    Display,
    /// An encoded master, interchange image, or other delivery artifact.
    Deliverable,
}

impl OutputTargetKind {
    /// Returns the stable diagnostic code for this output family.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Display => "display",
            Self::Deliverable => "deliverable",
        }
    }
}

/// Explicit policy used to compose an output transform.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutputTransformOptions {
    chromatic_adaptation: ChromaticAdaptation,
    gamut_mapping: GamutMapping,
    pq_reference_white: Option<Nits>,
}

impl OutputTransformOptions {
    /// Creates the default preserving output policy.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            chromatic_adaptation: ChromaticAdaptation::Bradford,
            gamut_mapping: GamutMapping::Preserve,
            pq_reference_white: None,
        }
    }

    /// Replaces the reference-white adaptation policy.
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

    /// Sets the absolute PQ luminance represented by working value one.
    #[must_use]
    pub const fn with_pq_reference_white(mut self, reference_white: Nits) -> Self {
        self.pq_reference_white = Some(reference_white);
        self
    }

    /// Returns the reference-white adaptation policy.
    #[must_use]
    pub const fn chromatic_adaptation(self) -> ChromaticAdaptation {
        self.chromatic_adaptation
    }

    /// Returns the destination gamut policy.
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

impl Default for OutputTransformOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// A deterministic transform from one working space to explicit output RGB.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutputColorTransform {
    target_kind: OutputTargetKind,
    source: WorkingSpace,
    destination: ColorSpace,
    options: OutputTransformOptions,
    gamut: WideGamutTransform,
}

impl OutputColorTransform {
    /// Composes and validates an explicit display or delivery transform.
    ///
    /// This color boundary emits full-range floating RGB. YUV conversion,
    /// legal range, and integer quantization are explicit downstream storage
    /// operations and are rejected here instead of being silently inferred.
    pub fn new(
        target_kind: OutputTargetKind,
        source: WorkingSpace,
        destination: ColorSpace,
        options: OutputTransformOptions,
    ) -> Result<Self> {
        validate_destination(target_kind, destination, options)?;
        let gamut = WideGamutTransform::new(
            source.color_space().primaries(),
            destination.primaries(),
            options.chromatic_adaptation,
            options.gamut_mapping,
        )
        .map_err(|error| error.with_context(transform_context(target_kind, source, destination)))?;
        Ok(Self {
            target_kind,
            source,
            destination,
            options,
            gamut,
        })
    }

    /// Returns the semantic output consumer.
    #[must_use]
    pub const fn target_kind(self) -> OutputTargetKind {
        self.target_kind
    }

    /// Returns the exact working source space.
    #[must_use]
    pub const fn source(self) -> WorkingSpace {
        self.source
    }

    /// Returns the authoritative output RGB interpretation.
    #[must_use]
    pub const fn destination(self) -> ColorSpace {
        self.destination
    }

    /// Returns the transform policy.
    #[must_use]
    pub const fn options(self) -> OutputTransformOptions {
        self.options
    }

    /// Applies this transform to canonical binary16 working storage.
    pub fn apply(self, source: &WorkingImage) -> Result<Image> {
        if source.space() != self.source {
            return Err(source_mismatch(self, source.space()));
        }
        self.apply_f32(&source.promote_f32()?)
    }

    /// Applies this transform to promoted binary32 working computation storage.
    ///
    /// Windows, channel identity, image metadata, and retained source color
    /// payloads survive. The authoritative interpretation becomes the output
    /// destination and samples remain premultiplied RGBA binary32.
    pub fn apply_f32(self, source: &WorkingImageF32) -> Result<Image> {
        if source.space() != self.source {
            return Err(source_mismatch(self, source.space()));
        }
        let image = source.image();
        let samples = image.samples();
        let mut output = Vec::with_capacity(samples.len());
        for pixel_start in (0..samples.len()).step_by(4) {
            let alpha = sample(samples, pixel_start + 3)?;
            if !(0.0..=1.0).contains(&alpha) {
                return Err(invalid(
                    "transform_output_image",
                    "working alpha must be finite and within zero to one",
                )
                .with_context(sample_context(pixel_start + 3, alpha)));
            }
            let premultiplied = [
                sample(samples, pixel_start)?,
                sample(samples, pixel_start + 1)?,
                sample(samples, pixel_start + 2)?,
            ];
            let straight = unassociate(premultiplied, alpha)?;
            let converted = self.gamut.apply_rgb(LinearRgb::new(straight)?)?.values();
            let encoded = [
                self.encode_component(converted[0])?,
                self.encode_component(converted[1])?,
                self.encode_component(converted[2])?,
            ];
            for component in [
                encoded[0] * alpha,
                encoded[1] * alpha,
                encoded[2] * alpha,
                alpha,
            ] {
                let narrowed = component as f32;
                if !narrowed.is_finite() {
                    return Err(invalid(
                        "transform_output_image",
                        "output component cannot be represented as binary32",
                    )
                    .with_context(transform_context(
                        self.target_kind,
                        self.source,
                        self.destination,
                    )));
                }
                output.push(narrowed);
            }
        }

        let descriptor = image.descriptor();
        let color_tags = descriptor
            .color_tags()
            .clone()
            .with_interpretation(self.destination);
        let output_descriptor = ImageDescriptor::new_with_color_tags(
            descriptor.data_window(),
            descriptor.display_window(),
            PixelFormat::Rgba32Float,
            color_tags,
            AlphaMode::Premultiplied,
        )?
        .with_channels(descriptor.channels().clone())?;
        Image::new_with_metadata(
            output_descriptor,
            ImageSamples::from_f32(output),
            image.metadata().clone(),
        )
    }

    fn encode_component(self, linear: f64) -> Result<f64> {
        let encoded = if self.destination.transfer() == TransferFunction::Pq {
            let reference_white = self
                .options
                .pq_reference_white
                .expect("validated PQ transforms have a reference white")
                .value();
            let luminance = Nits::new(linear * reference_white).map_err(|error| {
                error.with_context(transform_context(
                    self.target_kind,
                    self.source,
                    self.destination,
                ))
            })?;
            pq_inverse_eotf(luminance)?.value()
        } else {
            encode_relative_transfer(self.destination.transfer(), RelativeLight::new(linear)?)?
                .value()
        };
        if !encoded.is_finite() {
            return Err(invalid(
                "encode_output_transfer",
                "encoded output component must be finite",
            ));
        }
        Ok(encoded)
    }
}

fn validate_destination(
    target_kind: OutputTargetKind,
    destination: ColorSpace,
    options: OutputTransformOptions,
) -> Result<()> {
    let context = destination_context(target_kind, destination);
    if destination.matrix() != MatrixCoefficients::Rgb || destination.range() != ColorRange::Full {
        return Err(unsupported(
            "compose_output_transform",
            "output color transforms emit full-range RGB before storage conversion",
        )
        .with_context(context));
    }
    if destination.primaries() == ColorPrimaries::Unspecified
        || destination.transfer() == TransferFunction::Unspecified
    {
        return Err(invalid(
            "compose_output_transform",
            "output primaries and transfer function must be explicitly declared",
        )
        .with_context(context));
    }
    if target_kind == OutputTargetKind::Display
        && destination.transfer() == TransferFunction::Linear
    {
        return Err(unsupported(
            "compose_output_transform",
            "display output requires an explicit display transfer function",
        )
        .with_context(context));
    }
    if !matches!(
        destination.transfer(),
        TransferFunction::Linear
            | TransferFunction::Srgb
            | TransferFunction::Bt709
            | TransferFunction::Bt2020TenBit
            | TransferFunction::Bt2020TwelveBit
            | TransferFunction::Gamma22
            | TransferFunction::Gamma24
            | TransferFunction::Pq
            | TransferFunction::Hlg
    ) {
        return Err(unsupported(
            "compose_output_transform",
            "destination transfer function is not supported by output transforms",
        )
        .with_context(context));
    }
    match (destination.transfer(), options.pq_reference_white) {
        (TransferFunction::Pq, Some(reference_white)) if reference_white.value() > 0.0 => {}
        (TransferFunction::Pq, _) => {
            return Err(invalid(
                "compose_output_transform",
                "PQ output transforms require a positive reference-white luminance",
            )
            .with_context(context));
        }
        (_, Some(_)) => {
            return Err(invalid(
                "compose_output_transform",
                "PQ reference white may be set only for PQ output signals",
            )
            .with_context(context));
        }
        (_, None) => {}
    }
    Ok(())
}

fn sample(samples: &ImageSamples, index: usize) -> Result<f64> {
    let value = samples.float_value(index).map(f64::from).ok_or_else(|| {
        invalid(
            "read_output_sample",
            "working output source must contain floating-point samples",
        )
        .with_context(sample_context(index, f64::NAN))
    })?;
    if !value.is_finite() {
        return Err(invalid(
            "read_output_sample",
            "working output samples must be finite",
        )
        .with_context(sample_context(index, value)));
    }
    Ok(value)
}

fn unassociate(premultiplied: [f64; 3], alpha: f64) -> Result<[f64; 3]> {
    if alpha == 0.0 {
        if premultiplied.into_iter().any(|component| component != 0.0) {
            return Err(invalid(
                "unassociate_output_alpha",
                "zero-alpha premultiplied working RGB must also be zero",
            ));
        }
        return Ok([0.0; 3]);
    }
    let straight = premultiplied.map(|component| component / alpha);
    if straight.into_iter().any(|component| !component.is_finite()) {
        return Err(invalid(
            "unassociate_output_alpha",
            "unassociated working RGB must be finite",
        ));
    }
    Ok(straight)
}

fn source_mismatch(transform: OutputColorTransform, actual: WorkingSpace) -> Error {
    invalid(
        "transform_output_image",
        "working image space does not match the output transform source",
    )
    .with_context(transform_context(
        transform.target_kind,
        transform.source,
        transform.destination,
    ))
    .with_context(
        ErrorContext::new(COMPONENT, "inspect_actual_source")
            .with_field("primaries", actual.color_space().primaries().code()),
    )
}

fn transform_context(
    target_kind: OutputTargetKind,
    source: WorkingSpace,
    destination: ColorSpace,
) -> ErrorContext {
    ErrorContext::new(COMPONENT, "inspect_transform")
        .with_field("target_kind", target_kind.code())
        .with_field("source_primaries", source.color_space().primaries().code())
        .with_field("destination_primaries", destination.primaries().code())
        .with_field("destination_transfer", destination.transfer().code())
}

fn destination_context(target_kind: OutputTargetKind, destination: ColorSpace) -> ErrorContext {
    ErrorContext::new(COMPONENT, "inspect_destination")
        .with_field("target_kind", target_kind.code())
        .with_field("primaries", destination.primaries().code())
        .with_field("transfer", destination.transfer().code())
        .with_field("matrix", destination.matrix().code())
        .with_field("range", destination.range().code())
}

fn sample_context(index: usize, value: f64) -> ErrorContext {
    ErrorContext::new(COMPONENT, "inspect_sample")
        .with_field("sample_index", index.to_string())
        .with_field("value", value.to_string())
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
