//! Thumbnail and waveform image generation outside the ordinary render path.
//!
//! Thumbnails consume an already visible [`Image`] and retain its complete
//! pixel, channel, color, alpha, metadata, and spatial contract. A GPU caller
//! reaches this module only after using the explicit thumbnail readback
//! boundary owned by `superi-gpu`.
//!
//! Waveform images consume validated per-channel peak envelopes. Decoded PCM
//! interpretation stays in `superi-media-io`, below the audio graph and outside
//! playback. The returned [`WaveformImage`] keeps the exact source sample clock
//! and ordered channel layout alongside its UI raster.

use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::{Matrix3, PixelBounds};
use superi_core::pixel::{AlphaMode, ChannelLayout, PixelFormat};
use superi_core::time::SampleTime;

use crate::metadata::{ImageMetadata, ImageMetadataValue};
use crate::ops::{transform, ResampleFilter};
use crate::value::{Image, ImageDescriptor, ImageSamples};

const COMPONENT: &str = "superi-image.preview";

/// Maximum display dimensions for one aspect-fit thumbnail.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ThumbnailRequest {
    max_width: u32,
    max_height: u32,
}

impl ThumbnailRequest {
    /// Creates a nonempty thumbnail bound.
    pub fn new(max_width: u32, max_height: u32) -> Result<Self> {
        if max_width == 0 || max_height == 0 {
            return Err(invalid(
                "create_thumbnail_request",
                "thumbnail dimensions must be greater than zero",
            ));
        }
        Ok(Self {
            max_width,
            max_height,
        })
    }

    /// Returns the maximum output display width.
    #[must_use]
    pub const fn max_width(self) -> u32 {
        self.max_width
    }

    /// Returns the maximum output display height.
    #[must_use]
    pub const fn max_height(self) -> u32 {
        self.max_height
    }
}

/// Generates an aspect-fit thumbnail and never enlarges the source display.
///
/// The source display origin maps to `(0, 0)`. Data outside or inside that
/// display remains in the same relative position. Scaling uses alpha-aware
/// bilinear reconstruction, while a translation-only request keeps exact
/// sample payloads through nearest reconstruction.
pub fn generate_thumbnail(source: &Image, request: ThumbnailRequest) -> Result<Image> {
    let display = source.descriptor().display_window();
    let fit_scale = (f64::from(request.max_width) / f64::from(display.width()))
        .min(f64::from(request.max_height) / f64::from(display.height()))
        .min(1.0);
    let output_width = (f64::from(display.width()) * fit_scale).round().max(1.0);
    let output_height = (f64::from(display.height()) * fit_scale).round().max(1.0);
    let scale_x = output_width / f64::from(display.width());
    let scale_y = output_height / f64::from(display.height());
    let source_to_thumbnail = Matrix3::from_rows([
        [scale_x, 0.0, -f64::from(display.min_x()) * scale_x],
        [0.0, scale_y, -f64::from(display.min_y()) * scale_y],
        [0.0, 0.0, 1.0],
    ])
    .map_err(|error| with_context(error, "generate_thumbnail"))?;
    let data_window = transformed_bounds(
        source.descriptor().data_window(),
        source_to_thumbnail,
        "generate_thumbnail",
    )?;
    let filter = if scale_x == 1.0 && scale_y == 1.0 {
        ResampleFilter::Nearest
    } else {
        ResampleFilter::Bilinear
    };
    transform(source, source_to_thumbnail, data_window, filter)
        .map_err(|error| with_context(error, "generate_thumbnail"))
}

/// Minimum and maximum normalized sample amplitudes for one channel and column.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WaveformPeak {
    minimum: f32,
    maximum: f32,
}

impl WaveformPeak {
    /// Creates a finite ordered peak inside the normalized audio range.
    pub fn new(minimum: f32, maximum: f32) -> Result<Self> {
        if !minimum.is_finite()
            || !maximum.is_finite()
            || minimum < -1.0
            || maximum > 1.0
            || minimum > maximum
        {
            return Err(invalid(
                "create_waveform_peak",
                "waveform peaks must be finite ordered values from negative one through one",
            ));
        }
        Ok(Self { minimum, maximum })
    }

    /// Returns the least sample amplitude in the source column.
    #[must_use]
    pub const fn minimum(self) -> f32 {
        self.minimum
    }

    /// Returns the greatest sample amplitude in the source column.
    #[must_use]
    pub const fn maximum(self) -> f32 {
        self.maximum
    }
}

/// Exact timed, channel-ordered peaks ready for preview rasterization.
#[derive(Clone, Debug, PartialEq)]
pub struct WaveformEnvelope {
    start: SampleTime,
    frame_count: u64,
    channel_layout: ChannelLayout,
    columns: Vec<Vec<WaveformPeak>>,
}

impl WaveformEnvelope {
    /// Creates a nonempty envelope whose columns partition the source frames.
    pub fn new(
        start: SampleTime,
        frame_count: u64,
        channel_layout: ChannelLayout,
        columns: Vec<Vec<WaveformPeak>>,
    ) -> Result<Self> {
        if frame_count == 0 || columns.is_empty() {
            return Err(invalid(
                "create_waveform_envelope",
                "waveform envelopes require source frames and output columns",
            ));
        }
        let column_count = u64::try_from(columns.len()).map_err(|_| {
            exhausted(
                "create_waveform_envelope",
                "waveform column count cannot be represented",
            )
        })?;
        if column_count > frame_count {
            return Err(invalid(
                "create_waveform_envelope",
                "waveform columns must not exceed source frame count",
            ));
        }
        if columns
            .iter()
            .any(|column| column.len() != channel_layout.len())
        {
            return Err(invalid(
                "create_waveform_envelope",
                "every waveform column must contain one peak per ordered audio channel",
            ));
        }
        let frame_count_i64 = i64::try_from(frame_count).map_err(|_| {
            exhausted(
                "create_waveform_envelope",
                "waveform frame count exceeds the signed sample clock",
            )
        })?;
        start.sample().checked_add(frame_count_i64).ok_or_else(|| {
            exhausted(
                "create_waveform_envelope",
                "waveform sample range exceeds the signed sample clock",
            )
        })?;
        Ok(Self {
            start,
            frame_count,
            channel_layout,
            columns,
        })
    }

    /// Returns the exact first source sample.
    #[must_use]
    pub const fn start(&self) -> SampleTime {
        self.start
    }

    /// Returns the total source frames summarized by the envelope.
    #[must_use]
    pub const fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Returns channel positions in source routing order.
    #[must_use]
    pub const fn channel_layout(&self) -> &ChannelLayout {
        &self.channel_layout
    }

    /// Returns the number of raster columns.
    #[must_use]
    pub fn column_count(&self) -> usize {
        self.columns.len()
    }

    /// Returns one channel peak by output column and routing index.
    #[must_use]
    pub fn peak(&self, column: usize, channel: usize) -> Option<WaveformPeak> {
        self.columns.get(column)?.get(channel).copied()
    }

    /// Returns the exact half-open source sample range summarized by a column.
    #[must_use]
    pub fn source_range_for_column(&self, column: usize) -> Option<(SampleTime, SampleTime)> {
        if column >= self.columns.len() {
            return None;
        }
        let width = u128::try_from(self.columns.len()).ok()?;
        let column = u128::try_from(column).ok()?;
        let frames = u128::from(self.frame_count);
        let first_offset = u64::try_from(column.checked_mul(frames)? / width).ok()?;
        let end_offset = u64::try_from((column + 1).checked_mul(frames)? / width).ok()?;
        let first = self
            .start
            .sample()
            .checked_add(i64::try_from(first_offset).ok()?)?;
        let end = self
            .start
            .sample()
            .checked_add(i64::try_from(end_offset).ok()?)?;
        Some((
            SampleTime::new(first, self.start.sample_rate()).ok()?,
            SampleTime::new(end, self.start.sample_rate()).ok()?,
        ))
    }
}

/// Raster dimensions and unassociated sRGB colors for a waveform preview.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct WaveformRasterStyle {
    channel_height: u32,
    channel_gap: u32,
    foreground: [u8; 4],
    background: [u8; 4],
}

impl WaveformRasterStyle {
    /// Creates a style with one horizontal band per audio channel.
    pub fn new(
        channel_height: u32,
        channel_gap: u32,
        foreground: [u8; 4],
        background: [u8; 4],
    ) -> Result<Self> {
        if channel_height == 0 {
            return Err(invalid(
                "create_waveform_style",
                "waveform channel height must be greater than zero",
            ));
        }
        Ok(Self {
            channel_height,
            channel_gap,
            foreground,
            background,
        })
    }

    /// Returns pixels allocated to each channel band.
    #[must_use]
    pub const fn channel_height(self) -> u32 {
        self.channel_height
    }

    /// Returns separator pixels between adjacent channels.
    #[must_use]
    pub const fn channel_gap(self) -> u32 {
        self.channel_gap
    }

    /// Returns unassociated sRGB waveform color.
    #[must_use]
    pub const fn foreground(self) -> [u8; 4] {
        self.foreground
    }

    /// Returns unassociated sRGB background color.
    #[must_use]
    pub const fn background(self) -> [u8; 4] {
        self.background
    }
}

impl Default for WaveformRasterStyle {
    fn default() -> Self {
        Self {
            channel_height: 32,
            channel_gap: 1,
            foreground: [255, 255, 255, 255],
            background: [0, 0, 0, 0],
        }
    }
}

/// A UI raster paired with the exact audio range and channel routing it depicts.
#[derive(Clone, Debug, PartialEq)]
pub struct WaveformImage {
    envelope: WaveformEnvelope,
    image: Image,
}

impl WaveformImage {
    /// Returns the generated sRGB image artifact.
    #[must_use]
    pub const fn image(&self) -> &Image {
        &self.image
    }

    /// Returns the first depicted source sample.
    #[must_use]
    pub const fn start(&self) -> SampleTime {
        self.envelope.start()
    }

    /// Returns the exact number of depicted source frames.
    #[must_use]
    pub const fn frame_count(&self) -> u64 {
        self.envelope.frame_count()
    }

    /// Returns channel positions in source routing order.
    #[must_use]
    pub const fn channel_layout(&self) -> &ChannelLayout {
        self.envelope.channel_layout()
    }

    /// Returns one summarized peak by output column and routing index.
    #[must_use]
    pub fn peak(&self, column: usize, channel: usize) -> Option<WaveformPeak> {
        self.envelope.peak(column, channel)
    }

    /// Returns the exact half-open source sample range summarized by a column.
    #[must_use]
    pub fn source_range_for_column(&self, column: usize) -> Option<(SampleTime, SampleTime)> {
        self.envelope.source_range_for_column(column)
    }
}

/// Renders validated peaks into one sRGB channel-band image.
pub fn render_waveform_image(
    envelope: WaveformEnvelope,
    style: WaveformRasterStyle,
) -> Result<WaveformImage> {
    let width = u32::try_from(envelope.column_count()).map_err(|_| {
        exhausted(
            "render_waveform_image",
            "waveform width exceeds the image coordinate range",
        )
    })?;
    let channel_count = u32::try_from(envelope.channel_layout().len()).map_err(|_| {
        exhausted(
            "render_waveform_image",
            "waveform channel count exceeds the image coordinate range",
        )
    })?;
    let gaps = channel_count.saturating_sub(1);
    let height = channel_count
        .checked_mul(style.channel_height)
        .and_then(|value| value.checked_add(gaps.checked_mul(style.channel_gap)?))
        .ok_or_else(|| {
            exhausted(
                "render_waveform_image",
                "waveform height exceeds the image coordinate range",
            )
        })?;
    let bounds = PixelBounds::from_origin_size(0, 0, width, height)
        .map_err(|error| with_context(error, "render_waveform_image"))?;
    let pixel_count = usize::try_from(width)
        .ok()
        .and_then(|width| {
            usize::try_from(height)
                .ok()
                .and_then(|height| width.checked_mul(height))
        })
        .ok_or_else(|| {
            exhausted(
                "render_waveform_image",
                "waveform pixel count exceeds the platform address space",
            )
        })?;
    let sample_count = pixel_count.checked_mul(4).ok_or_else(|| {
        exhausted(
            "render_waveform_image",
            "waveform sample count exceeds the platform address space",
        )
    })?;
    let mut samples = Vec::new();
    samples.try_reserve_exact(sample_count).map_err(|_| {
        exhausted(
            "render_waveform_image",
            "waveform image allocation exceeds available memory",
        )
    })?;
    for _ in 0..pixel_count {
        samples.extend_from_slice(&style.background);
    }

    let width_usize = usize::try_from(width).map_err(|_| {
        exhausted(
            "render_waveform_image",
            "waveform width cannot be represented on this platform",
        )
    })?;
    for column in 0..envelope.column_count() {
        for channel in 0..envelope.channel_layout().len() {
            let peak = envelope
                .peak(column, channel)
                .expect("validated waveform envelope shape");
            let channel_u32 = u32::try_from(channel).map_err(|_| {
                exhausted(
                    "render_waveform_image",
                    "waveform channel index cannot be represented",
                )
            })?;
            let band_top = channel_u32
                .checked_mul(
                    style
                        .channel_height
                        .checked_add(style.channel_gap)
                        .ok_or_else(|| {
                            exhausted("render_waveform_image", "waveform band stride overflowed")
                        })?,
                )
                .ok_or_else(|| {
                    exhausted("render_waveform_image", "waveform band position overflowed")
                })?;
            let first_row = band_top + amplitude_row(peak.maximum, style.channel_height);
            let last_row = band_top + amplitude_row(peak.minimum, style.channel_height);
            for row in first_row..=last_row {
                let row = usize::try_from(row).map_err(|_| {
                    exhausted(
                        "render_waveform_image",
                        "waveform row cannot be represented on this platform",
                    )
                })?;
                let pixel = row
                    .checked_mul(width_usize)
                    .and_then(|row_start| row_start.checked_add(column))
                    .and_then(|pixel| pixel.checked_mul(4))
                    .ok_or_else(|| {
                        exhausted("render_waveform_image", "waveform pixel index overflowed")
                    })?;
                samples[pixel..pixel + 4].copy_from_slice(&style.foreground);
            }
        }
    }

    let descriptor = ImageDescriptor::new(
        bounds,
        bounds,
        PixelFormat::Rgba8Unorm,
        ColorSpace::SRGB,
        AlphaMode::Straight,
    )?;
    let mut metadata = ImageMetadata::new();
    metadata.insert(
        "superi.preview.kind",
        ImageMetadataValue::Text("waveform".into()),
    )?;
    metadata.insert(
        "superi.audio.start_sample",
        ImageMetadataValue::Signed(envelope.start().sample()),
    )?;
    metadata.insert(
        "superi.audio.sample_rate",
        ImageMetadataValue::Unsigned(u64::from(envelope.start().sample_rate())),
    )?;
    metadata.insert(
        "superi.audio.frame_count",
        ImageMetadataValue::Unsigned(envelope.frame_count()),
    )?;
    metadata.insert(
        "superi.audio.channel_count",
        ImageMetadataValue::Unsigned(u64::from(channel_count)),
    )?;
    let image = Image::new_with_metadata(descriptor, ImageSamples::from_u8(samples), metadata)?;
    Ok(WaveformImage { envelope, image })
}

fn transformed_bounds(
    bounds: PixelBounds,
    transform: Matrix3,
    operation: &'static str,
) -> Result<PixelBounds> {
    let transformed = bounds
        .to_rect()
        .checked_transform_bounds(transform)
        .map_err(|error| with_context(error, operation))?;
    let min_x = outward_edge(transformed.min().x().floor(), operation)?;
    let min_y = outward_edge(transformed.min().y().floor(), operation)?;
    let max_x = outward_edge(transformed.max().x().ceil(), operation)?;
    let max_y = outward_edge(transformed.max().y().ceil(), operation)?;
    PixelBounds::new(min_x, min_y, max_x, max_y).map_err(|error| with_context(error, operation))
}

fn outward_edge(value: f64, operation: &'static str) -> Result<i32> {
    if !value.is_finite() || value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
        return Err(exhausted(
            operation,
            "preview bounds exceed the supported coordinate range",
        ));
    }
    Ok(value as i32)
}

fn amplitude_row(amplitude: f32, channel_height: u32) -> u32 {
    if channel_height == 1 {
        return 0;
    }
    let span = f64::from(channel_height - 1);
    ((1.0 - f64::from(amplitude)) * 0.5 * span).round() as u32
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn exhausted(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn with_context(error: Error, operation: &'static str) -> Error {
    error.with_context(ErrorContext::new(COMPONENT, operation))
}
