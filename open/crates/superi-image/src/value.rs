//! HDR and high-bit-depth image values.
//!
//! The foundational model uses tightly packed logical samples. Arbitrary
//! interleaved or planar strides, named layers, tiled access, and deep samples
//! are separate contracts layered on top of this representation.

use std::sync::Arc;

use half::f16;
use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::PixelBounds;
use superi_core::pixel::{AlphaMode, PixelFormat, PixelModel, PixelPacking};

use crate::channels::ChannelList;
use crate::metadata::{ImageMetadata, ImageMetadataValue};

/// The exact scalar representation of dense image samples.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ImageSampleType {
    /// Unsigned 8-bit integer samples.
    U8,
    /// Unsigned 16-bit integer samples.
    U16,
    /// IEEE binary16 floating-point samples.
    F16,
    /// IEEE binary32 floating-point samples.
    F32,
}

impl ImageSampleType {
    /// Every sample representation defined by this version.
    pub const ALL: &'static [Self] = &[Self::U8, Self::U16, Self::F16, Self::F32];

    /// Returns the stable representation code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::F16 => "f16",
            Self::F32 => "f32",
        }
    }

    /// Returns the number of bits occupied by one sample.
    #[must_use]
    pub const fn bits(self) -> u8 {
        match self {
            Self::U8 => 8,
            Self::U16 | Self::F16 => 16,
            Self::F32 => 32,
        }
    }

    /// Returns true for a floating-point representation.
    #[must_use]
    pub const fn is_float(self) -> bool {
        matches!(self, Self::F16 | Self::F32)
    }
}

/// Canonical dense logical samples with exact floating-point payloads.
///
/// Floating values are owned as raw IEEE bits. This retains signed zero,
/// infinities, subnormals, and every NaN payload while avoiding accidental
/// normalization during storage and comparison.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ImageSamples {
    /// Unsigned 8-bit samples.
    U8(Arc<[u8]>),
    /// Unsigned 16-bit samples.
    U16(Arc<[u16]>),
    /// Raw IEEE binary16 payloads.
    F16(Arc<[u16]>),
    /// Raw IEEE binary32 payloads.
    F32(Arc<[u32]>),
}

impl ImageSamples {
    /// Owns unsigned 8-bit values without changing them.
    #[must_use]
    pub fn from_u8(values: impl IntoIterator<Item = u8>) -> Self {
        Self::U8(values.into_iter().collect::<Vec<_>>().into())
    }

    /// Owns unsigned 16-bit values without changing them.
    #[must_use]
    pub fn from_u16(values: impl IntoIterator<Item = u16>) -> Self {
        Self::U16(values.into_iter().collect::<Vec<_>>().into())
    }

    /// Owns exact IEEE binary16 payload bits.
    #[must_use]
    pub fn from_f16_bits(bits: impl IntoIterator<Item = u16>) -> Self {
        Self::F16(bits.into_iter().collect::<Vec<_>>().into())
    }

    /// Converts binary32 values to IEEE binary16 using round-to-nearest-even.
    #[must_use]
    pub fn f16_from_f32(values: impl IntoIterator<Item = f32>) -> Self {
        Self::F16(
            values
                .into_iter()
                .map(|value| f16::from_f32(value).to_bits())
                .collect::<Vec<_>>()
                .into(),
        )
    }

    /// Owns exact IEEE binary32 payload bits.
    #[must_use]
    pub fn from_f32_bits(bits: impl IntoIterator<Item = u32>) -> Self {
        Self::F32(bits.into_iter().collect::<Vec<_>>().into())
    }

    /// Stores binary32 values as their exact IEEE payloads.
    #[must_use]
    pub fn from_f32(values: impl IntoIterator<Item = f32>) -> Self {
        Self::F32(
            values
                .into_iter()
                .map(f32::to_bits)
                .collect::<Vec<_>>()
                .into(),
        )
    }

    /// Returns the exact scalar representation.
    #[must_use]
    pub const fn sample_type(&self) -> ImageSampleType {
        match self {
            Self::U8(_) => ImageSampleType::U8,
            Self::U16(_) => ImageSampleType::U16,
            Self::F16(_) => ImageSampleType::F16,
            Self::F32(_) => ImageSampleType::F32,
        }
    }

    /// Returns the number of logical scalar samples.
    #[must_use]
    pub fn len(&self) -> usize {
        match self {
            Self::U8(values) => values.len(),
            Self::U16(values) | Self::F16(values) => values.len(),
            Self::F32(values) => values.len(),
        }
    }

    /// Returns true when no samples are stored.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns unsigned 8-bit samples when represented that way.
    #[must_use]
    pub fn u8_values(&self) -> Option<&[u8]> {
        match self {
            Self::U8(values) => Some(values),
            _ => None,
        }
    }

    /// Returns unsigned 16-bit samples when represented that way.
    #[must_use]
    pub fn u16_values(&self) -> Option<&[u16]> {
        match self {
            Self::U16(values) => Some(values),
            _ => None,
        }
    }

    /// Returns exact IEEE binary16 payloads when represented that way.
    #[must_use]
    pub fn f16_bits(&self) -> Option<&[u16]> {
        match self {
            Self::F16(bits) => Some(bits),
            _ => None,
        }
    }

    /// Returns exact IEEE binary32 payloads when represented that way.
    #[must_use]
    pub fn f32_bits(&self) -> Option<&[u32]> {
        match self {
            Self::F32(bits) => Some(bits),
            _ => None,
        }
    }

    /// Converts one floating sample to binary32 for computation.
    ///
    /// Integer samples return `None` because this model does not silently choose
    /// normalized or numeric conversion semantics for them.
    #[must_use]
    pub fn float_value(&self, index: usize) -> Option<f32> {
        match self {
            Self::F16(bits) => bits
                .get(index)
                .copied()
                .map(|bits| f16::from_bits(bits).to_f32()),
            Self::F32(bits) => bits.get(index).copied().map(f32::from_bits),
            Self::U8(_) | Self::U16(_) => None,
        }
    }
}

const LUMINANCE_CHANNELS: &[&str] = &["Y"];
const RG_CHANNELS: &[&str] = &["R", "G"];
const RGB_CHANNELS: &[&str] = &["R", "G", "B"];
const BGR_CHANNELS: &[&str] = &["B", "G", "R"];
const RGBA_CHANNELS: &[&str] = &["R", "G", "B", "A"];
const BGRA_CHANNELS: &[&str] = &["B", "G", "R", "A"];

/// Complete semantic description of one dense image value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageDescriptor {
    data_window: PixelBounds,
    display_window: PixelBounds,
    pixel_format: PixelFormat,
    color_space: ColorSpace,
    alpha_mode: AlphaMode,
    channels: ChannelList,
    sample_type: ImageSampleType,
}

impl ImageDescriptor {
    /// Creates an image descriptor with explicit storage and interpretation.
    ///
    /// Data and display windows are independent signed extents. Neither is
    /// silently cropped or translated to the other.
    pub fn new(
        data_window: PixelBounds,
        display_window: PixelBounds,
        pixel_format: PixelFormat,
        color_space: ColorSpace,
        alpha_mode: AlphaMode,
    ) -> Result<Self> {
        if data_window.is_empty() || display_window.is_empty() {
            return Err(invalid(
                "create_image_descriptor",
                "image data and display windows must be nonempty",
            ));
        }
        if pixel_format.packing() != PixelPacking::Packed {
            return Err(unsupported(
                "create_image_descriptor",
                "dense image values currently require a packed pixel format",
            ));
        }
        if !pixel_format.has_alpha() && alpha_mode != AlphaMode::Opaque {
            return Err(invalid(
                "create_image_descriptor",
                "pixel formats without alpha must use opaque alpha mode",
            ));
        }
        let channel_names = channel_names_for(pixel_format).ok_or_else(|| {
            unsupported(
                "create_image_descriptor",
                "the packed pixel format has no dense image channel mapping",
            )
        })?;
        let channels = ChannelList::from_full_names(channel_names)?;
        let sample_type = sample_type_for(pixel_format).ok_or_else(|| {
            unsupported(
                "create_image_descriptor",
                "the packed pixel format has no dense image sample mapping",
            )
        })?;
        Ok(Self {
            data_window,
            display_window,
            pixel_format,
            color_space,
            alpha_mode,
            channels,
            sample_type,
        })
    }

    /// Replaces the ordered channel identities without changing storage shape.
    ///
    /// This supports exact custom and nested-layer names while retaining the
    /// pixel format's component count, precision, and order.
    pub fn with_channels(mut self, channels: ChannelList) -> Result<Self> {
        if channels.len() != self.channels.len() {
            return Err(invalid_with_descriptor(
                &self,
                "replace_image_channels",
                "channel count does not match the image pixel format",
            )
            .with_context(
                ErrorContext::new("superi-image.value", "compare_channel_count")
                    .with_field("expected", self.channels.len().to_string())
                    .with_field("actual", channels.len().to_string()),
            ));
        }
        self.channels = channels;
        Ok(self)
    }

    /// Returns the signed extent containing stored samples.
    #[must_use]
    pub const fn data_window(&self) -> PixelBounds {
        self.data_window
    }

    /// Returns the signed intended display extent.
    #[must_use]
    pub const fn display_window(&self) -> PixelBounds {
        self.display_window
    }

    /// Returns the exact packed component representation and order.
    #[must_use]
    pub const fn pixel_format(&self) -> PixelFormat {
        self.pixel_format
    }

    /// Returns the unchanged color interpretation.
    #[must_use]
    pub const fn color_space(&self) -> ColorSpace {
        self.color_space
    }

    /// Returns the unchanged alpha interpretation.
    #[must_use]
    pub const fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }

    /// Returns channels in their exact packed sample order.
    #[must_use]
    pub const fn channels(&self) -> &ChannelList {
        &self.channels
    }

    /// Returns the scalar representation required by the pixel format.
    #[must_use]
    pub const fn sample_type(&self) -> ImageSampleType {
        self.sample_type
    }

    /// Returns the exact number of logical scalar samples for the data window.
    pub fn required_sample_count(&self) -> Result<usize> {
        let width = usize::try_from(self.data_window.width()).map_err(|_| {
            exhausted(
                "calculate_image_sample_count",
                "image width cannot be represented on this platform",
            )
        })?;
        let height = usize::try_from(self.data_window.height()).map_err(|_| {
            exhausted(
                "calculate_image_sample_count",
                "image height cannot be represented on this platform",
            )
        })?;
        width
            .checked_mul(height)
            .and_then(|pixels| pixels.checked_mul(self.channels().len()))
            .ok_or_else(|| {
                exhausted(
                    "calculate_image_sample_count",
                    "image sample count overflowed the platform address space",
                )
            })
    }
}

/// One immutable, inspectable image artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Image {
    descriptor: ImageDescriptor,
    samples: ImageSamples,
    metadata: ImageMetadata,
}

impl Image {
    /// Creates an image and validates representation and exact sample count.
    pub fn new(descriptor: ImageDescriptor, samples: ImageSamples) -> Result<Self> {
        validate_samples(&descriptor, &samples, "create_image")?;
        Ok(Self {
            descriptor,
            samples,
            metadata: ImageMetadata::new(),
        })
    }

    /// Adds or replaces one preserved metadata attribute.
    pub fn with_metadata(
        mut self,
        key: impl Into<String>,
        value: ImageMetadataValue,
    ) -> Result<Self> {
        self.metadata.insert(key, value)?;
        Ok(self)
    }

    /// Replaces pixels after validating them against the unchanged descriptor.
    ///
    /// Metadata, channel meaning, alpha interpretation, color interpretation,
    /// and spatial windows remain unchanged.
    pub fn replace_samples(mut self, samples: ImageSamples) -> Result<Self> {
        validate_samples(&self.descriptor, &samples, "replace_image_samples")?;
        self.samples = samples;
        Ok(self)
    }

    /// Returns the complete semantic descriptor.
    #[must_use]
    pub const fn descriptor(&self) -> &ImageDescriptor {
        &self.descriptor
    }

    /// Returns immutable logical samples.
    #[must_use]
    pub const fn samples(&self) -> &ImageSamples {
        &self.samples
    }

    /// Returns deterministically ordered preserved metadata.
    #[must_use]
    pub const fn metadata(&self) -> &ImageMetadata {
        &self.metadata
    }
}

const fn channel_names_for(pixel_format: PixelFormat) -> Option<&'static [&'static str]> {
    match pixel_format.model() {
        PixelModel::Gray => Some(LUMINANCE_CHANNELS),
        PixelModel::Rg => Some(RG_CHANNELS),
        PixelModel::Rgb => Some(RGB_CHANNELS),
        PixelModel::Bgr => Some(BGR_CHANNELS),
        PixelModel::Rgba => Some(RGBA_CHANNELS),
        PixelModel::Bgra => Some(BGRA_CHANNELS),
        PixelModel::Yuv => None,
        _ => None,
    }
}

const fn sample_type_for(pixel_format: PixelFormat) -> Option<ImageSampleType> {
    match pixel_format {
        PixelFormat::R8Unorm
        | PixelFormat::Rg8Unorm
        | PixelFormat::Rgb8Unorm
        | PixelFormat::Bgr8Unorm
        | PixelFormat::Rgba8Unorm
        | PixelFormat::Bgra8Unorm
        | PixelFormat::Yuv420p8
        | PixelFormat::Yuv422p8
        | PixelFormat::Yuv444p8
        | PixelFormat::Nv12 => Some(ImageSampleType::U8),
        PixelFormat::R16Unorm
        | PixelFormat::Rg16Unorm
        | PixelFormat::Rgba16Unorm
        | PixelFormat::Yuv420p10
        | PixelFormat::Yuv422p10
        | PixelFormat::Yuv444p10
        | PixelFormat::P010 => Some(ImageSampleType::U16),
        PixelFormat::R16Float | PixelFormat::Rg16Float | PixelFormat::Rgba16Float => {
            Some(ImageSampleType::F16)
        }
        PixelFormat::R32Float | PixelFormat::Rg32Float | PixelFormat::Rgba32Float => {
            Some(ImageSampleType::F32)
        }
        _ => None,
    }
}

fn validate_samples(
    descriptor: &ImageDescriptor,
    samples: &ImageSamples,
    operation: &'static str,
) -> Result<()> {
    let expected_type = descriptor.sample_type();
    if samples.sample_type() != expected_type {
        return Err(invalid_with_descriptor(
            descriptor,
            operation,
            "image sample representation does not match its pixel format",
        )
        .with_context(
            ErrorContext::new("superi-image.value", "compare_sample_type")
                .with_field("expected", expected_type.code())
                .with_field("actual", samples.sample_type().code()),
        ));
    }
    let expected_count = descriptor.required_sample_count()?;
    if samples.len() != expected_count {
        return Err(invalid_with_descriptor(
            descriptor,
            operation,
            "image sample count does not match its data window and channel count",
        )
        .with_context(
            ErrorContext::new("superi-image.value", "compare_sample_count")
                .with_field("expected", expected_count.to_string())
                .with_field("actual", samples.len().to_string()),
        ));
    }
    Ok(())
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-image.value", operation))
}

fn invalid_with_descriptor(
    descriptor: &ImageDescriptor,
    operation: &'static str,
    message: &'static str,
) -> Error {
    invalid(operation, message).with_context(
        ErrorContext::new("superi-image.value", "inspect_image_descriptor")
            .with_field("pixel_format", descriptor.pixel_format.code())
            .with_field("width", descriptor.data_window.width().to_string())
            .with_field("height", descriptor.data_window.height().to_string()),
    )
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-image.value", operation))
}

fn exhausted(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-image.value", operation))
}
