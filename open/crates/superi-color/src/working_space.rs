//! Canonical scene-linear working image representation.
//!
//! Superi stores working pixels as premultiplied RGBA binary16 values and
//! promotes them to RGBA binary32 only for computations that require the
//! additional precision. Color transforms are deliberately outside this
//! module: an image is accepted only when it already carries the requested
//! scene-linear interpretation, so source data can never be silently retagged.

use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::PixelBounds;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_gpu::convert::GpuFrameDescriptor;
use superi_gpu::wgpu::TextureFormat;
use superi_image::value::{Image, ImageDescriptor, ImageSamples};

const COMPONENT: &str = "superi-color.working-space";
const RGBA_CHANNELS: [&str; 4] = ["R", "G", "B", "A"];

/// An explicit scene-linear RGB space used by canonical working images.
///
/// The type validates all independent color axes. It does not infer missing
/// metadata, substitute a display transfer, or perform a color transform.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WorkingSpace {
    color_space: ColorSpace,
}

impl WorkingSpace {
    /// The built-in default ACEScg scene-linear working space.
    pub const ACESCG: Self = Self {
        color_space: ColorSpace::ACESCG,
    };

    /// Creates a working space from a fully declared scene-linear RGB tag.
    pub fn new(color_space: ColorSpace) -> Result<Self> {
        validate_scene_linear(color_space)?;
        Ok(Self { color_space })
    }

    /// Returns the exact color interpretation carried by working pixels.
    #[must_use]
    pub const fn color_space(self) -> ColorSpace {
        self.color_space
    }

    /// Creates the canonical CPU storage descriptor for two explicit windows.
    pub fn image_descriptor(
        self,
        data_window: PixelBounds,
        display_window: PixelBounds,
    ) -> Result<ImageDescriptor> {
        ImageDescriptor::new(
            data_window,
            display_window,
            PixelFormat::Rgba16Float,
            self.color_space,
            AlphaMode::Premultiplied,
        )
    }

    /// Creates the canonical GPU storage descriptor for a working frame.
    pub fn gpu_frame_descriptor(self, width: u32, height: u32) -> Result<GpuFrameDescriptor> {
        GpuFrameDescriptor::new(
            width,
            height,
            PixelFormat::Rgba16Float,
            self.color_space,
            AlphaMode::Premultiplied,
            None,
        )
    }

    /// Validates that a GPU frame is the exact canonical representation.
    pub fn validate_gpu_frame_descriptor(self, descriptor: &GpuFrameDescriptor) -> Result<()> {
        if descriptor.pixel_format() != PixelFormat::Rgba16Float {
            return Err(invalid(
                "validate_gpu_working_frame",
                "working GPU frames must use RGBA 16-bit-float storage",
            )
            .with_context(gpu_context(descriptor)));
        }
        if descriptor.color_space() != self.color_space {
            return Err(unsupported(
                "validate_gpu_working_frame",
                "working GPU frame color interpretation requires an explicit transform",
            )
            .with_context(gpu_context(descriptor)));
        }
        if descriptor.alpha_mode() != AlphaMode::Premultiplied {
            return Err(invalid(
                "validate_gpu_working_frame",
                "working GPU frames must use premultiplied alpha",
            )
            .with_context(gpu_context(descriptor)));
        }
        let layouts = descriptor.plane_layouts();
        if layouts.len() != 1 || layouts[0].texture_format() != TextureFormat::Rgba16Float {
            return Err(invalid(
                "validate_gpu_working_frame",
                "working GPU frames must resolve to one Rgba16Float texture plane",
            )
            .with_context(gpu_context(descriptor)));
        }
        Ok(())
    }
}

impl Default for WorkingSpace {
    fn default() -> Self {
        Self::ACESCG
    }
}

/// A canonical stored working image.
///
/// Construction preserves the owned image exactly after validating binary16
/// RGBA storage, premultiplied alpha, base-layer channel identity, and the
/// requested scene-linear color tag. Negative values, values above one, HDR
/// values, signed zero, infinities, and NaN payloads are never clamped or
/// normalized by this boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingImage {
    space: WorkingSpace,
    image: Image,
}

impl WorkingImage {
    /// Accepts an already canonical working image without rewriting it.
    pub fn new(space: WorkingSpace, image: Image) -> Result<Self> {
        validate_image_descriptor(space, image.descriptor(), PixelFormat::Rgba16Float)?;
        Ok(Self { space, image })
    }

    /// Accepts an already canonical image in the default ACEScg space.
    pub fn acescg(image: Image) -> Result<Self> {
        Self::new(WorkingSpace::ACESCG, image)
    }

    /// Returns the explicit scene-linear working space.
    #[must_use]
    pub const fn space(&self) -> WorkingSpace {
        self.space
    }

    /// Returns the exact stored image artifact.
    #[must_use]
    pub const fn image(&self) -> &Image {
        &self.image
    }

    /// Releases the exact stored image artifact.
    #[must_use]
    pub fn into_image(self) -> Image {
        self.image
    }

    /// Promotes samples to binary32 for numerically sensitive computation.
    ///
    /// Windows, color tags, channel identity, alpha association, and image
    /// metadata are retained exactly. This is a numeric representation change,
    /// not a color transform.
    pub fn promote_f32(&self) -> Result<WorkingImageF32> {
        let samples = floating_values(self.image.samples(), "promote_working_image")?;
        let descriptor = ImageDescriptor::new_with_color_tags(
            self.image.descriptor().data_window(),
            self.image.descriptor().display_window(),
            PixelFormat::Rgba32Float,
            self.image.descriptor().color_tags().clone(),
            AlphaMode::Premultiplied,
        )?
        .with_channels(self.image.descriptor().channels().clone())?;
        let image = Image::new_with_metadata(
            descriptor,
            ImageSamples::from_f32(samples),
            self.image.metadata().clone(),
        )?;
        WorkingImageF32::new(self.space, image)
    }
}

/// A temporary binary32 working image for numerically sensitive computation.
///
/// This type is deliberately distinct from [`WorkingImage`], so computation
/// precision cannot be mislabeled as the canonical stored representation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkingImageF32 {
    space: WorkingSpace,
    image: Image,
}

impl WorkingImageF32 {
    /// Accepts an already promoted RGBA binary32 computation image.
    pub fn new(space: WorkingSpace, image: Image) -> Result<Self> {
        validate_image_descriptor(space, image.descriptor(), PixelFormat::Rgba32Float)?;
        Ok(Self { space, image })
    }

    /// Returns the scene-linear working space shared with canonical storage.
    #[must_use]
    pub const fn space(&self) -> WorkingSpace {
        self.space
    }

    /// Returns the binary32 computation image.
    #[must_use]
    pub const fn image(&self) -> &Image {
        &self.image
    }

    /// Releases the binary32 computation image.
    #[must_use]
    pub fn into_image(self) -> Image {
        self.image
    }

    /// Quantizes binary32 results into canonical binary16 storage.
    ///
    /// Conversion uses the image substrate's IEEE binary16 round-to-nearest-even
    /// rule and never clamps negative or above-one values.
    pub fn quantize_f16(&self) -> Result<WorkingImage> {
        let samples = floating_values(self.image.samples(), "quantize_working_image")?;
        let descriptor = ImageDescriptor::new_with_color_tags(
            self.image.descriptor().data_window(),
            self.image.descriptor().display_window(),
            PixelFormat::Rgba16Float,
            self.image.descriptor().color_tags().clone(),
            AlphaMode::Premultiplied,
        )?
        .with_channels(self.image.descriptor().channels().clone())?;
        let image = Image::new_with_metadata(
            descriptor,
            ImageSamples::f16_from_f32(samples),
            self.image.metadata().clone(),
        )?;
        WorkingImage::new(self.space, image)
    }
}

fn validate_scene_linear(color_space: ColorSpace) -> Result<()> {
    if color_space.primaries() == ColorPrimaries::Unspecified {
        return Err(invalid(
            "create_working_space",
            "working-space primaries must be explicitly declared",
        )
        .with_context(color_context(color_space)));
    }
    if !matches!(
        color_space.primaries(),
        ColorPrimaries::Bt2020
            | ColorPrimaries::DisplayP3
            | ColorPrimaries::AcesAp0
            | ColorPrimaries::AcesAp1
    ) {
        return Err(unsupported(
            "create_working_space",
            "working-space primaries must identify a supported wide-gamut space",
        )
        .with_context(color_context(color_space)));
    }
    if color_space.transfer() != TransferFunction::Linear
        || color_space.matrix() != MatrixCoefficients::Rgb
        || color_space.range() != ColorRange::Full
    {
        return Err(unsupported(
            "create_working_space",
            "working spaces must be scene-linear full-range RGB",
        )
        .with_context(color_context(color_space)));
    }
    Ok(())
}

fn validate_image_descriptor(
    space: WorkingSpace,
    descriptor: &ImageDescriptor,
    expected_format: PixelFormat,
) -> Result<()> {
    if descriptor.pixel_format() != expected_format {
        return Err(invalid(
            "validate_working_image",
            "working image pixel precision does not match its representation",
        )
        .with_context(image_context(descriptor, expected_format)));
    }
    if descriptor.color_space() != space.color_space {
        return Err(unsupported(
            "validate_working_image",
            "working image color interpretation requires an explicit transform",
        )
        .with_context(image_context(descriptor, expected_format)));
    }
    validate_scene_linear(descriptor.color_space())?;
    if descriptor.alpha_mode() != AlphaMode::Premultiplied {
        return Err(invalid(
            "validate_working_image",
            "working images must use premultiplied alpha",
        )
        .with_context(image_context(descriptor, expected_format)));
    }
    if !descriptor
        .channels()
        .iter()
        .map(|channel| channel.as_str())
        .eq(RGBA_CHANNELS)
    {
        return Err(invalid(
            "validate_working_image",
            "working images must use unqualified RGBA channels in canonical order",
        )
        .with_context(image_context(descriptor, expected_format)));
    }
    Ok(())
}

fn floating_values(samples: &ImageSamples, operation: &'static str) -> Result<Vec<f32>> {
    (0..samples.len())
        .map(|index| {
            samples.float_value(index).ok_or_else(|| {
                invalid(operation, "working image samples must be floating point").with_context(
                    ErrorContext::new(COMPONENT, "inspect_sample")
                        .with_field("sample_index", index.to_string())
                        .with_field("sample_type", samples.sample_type().code()),
                )
            })
        })
        .collect()
}

fn color_context(color_space: ColorSpace) -> ErrorContext {
    ErrorContext::new(COMPONENT, "inspect_color_space")
        .with_field("primaries", color_space.primaries().code())
        .with_field("transfer", color_space.transfer().code())
        .with_field("matrix", color_space.matrix().code())
        .with_field("range", color_space.range().code())
}

fn image_context(descriptor: &ImageDescriptor, expected_format: PixelFormat) -> ErrorContext {
    ErrorContext::new(COMPONENT, "inspect_image_descriptor")
        .with_field("expected_pixel_format", expected_format.code())
        .with_field("actual_pixel_format", descriptor.pixel_format().code())
        .with_field("alpha_mode", descriptor.alpha_mode().code())
        .with_field(
            "color_primaries",
            descriptor.color_space().primaries().code(),
        )
        .with_field("color_transfer", descriptor.color_space().transfer().code())
}

fn gpu_context(descriptor: &GpuFrameDescriptor) -> ErrorContext {
    ErrorContext::new(COMPONENT, "inspect_gpu_frame_descriptor")
        .with_field("pixel_format", descriptor.pixel_format().code())
        .with_field("alpha_mode", descriptor.alpha_mode().code())
        .with_field(
            "color_primaries",
            descriptor.color_space().primaries().code(),
        )
        .with_field("color_transfer", descriptor.color_space().transfer().code())
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
