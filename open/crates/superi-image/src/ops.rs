//! Dense image operations with explicit spatial and channel semantics.
//!
//! Operations in this module work on [`Image`] values without changing their
//! color interpretation, channel identity, sample representation, or metadata
//! unless the caller explicitly requests a channel mapping. Spatial operations
//! use signed pixel windows and transparent-black sampling outside the data
//! window. Images declared opaque use opaque black instead.

use half::f16;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::{Matrix3, PixelBounds, Point2};
use superi_core::pixel::{AlphaMode, PixelFormat};

use crate::alpha::AlphaLayout;
use crate::channels::{ChannelIndex, ChannelList};
use crate::value::{Image, ImageDescriptor, ImageSampleType, ImageSamples};

const COMPONENT: &str = "superi-image.ops";

/// Reconstruction used by resize and general spatial transforms.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ResampleFilter {
    /// Copy the closest source sample without changing its payload bits.
    Nearest,
    /// Interpolate the four closest source samples in associated-alpha space.
    Bilinear,
}

/// One display-relative mirror operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum FlipAxis {
    /// Exchange columns across the display window's vertical center line.
    Horizontal,
    /// Exchange rows across the display window's horizontal center line.
    Vertical,
}

/// An exact rotation in 90 degree increments.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum QuarterTurn {
    /// Rotate 90 degrees clockwise in image coordinates.
    Clockwise90,
    /// Rotate 180 degrees.
    Half,
    /// Rotate 90 degrees counterclockwise in image coordinates.
    CounterClockwise90,
}

/// The value written to one destination channel by [`remap_channels`].
#[derive(Clone, Copy, Debug, PartialEq)]
#[non_exhaustive]
pub enum ChannelSource {
    /// Copy one source channel by stable ordered index.
    Copy(ChannelIndex),
    /// Fill every pixel with one numeric value.
    ///
    /// Unsigned image representations interpret constants as normalized values
    /// and require the inclusive range 0 through 1. Floating representations
    /// retain the value using their declared IEEE precision.
    Constant(f32),
}

/// Crops to an exact signed data window without rebasing its coordinates.
///
/// The display window and all non-spatial identity are retained. Requested
/// pixels outside the source data window use the documented outside fill.
pub fn crop(source: &Image, data_window: PixelBounds) -> Result<Image> {
    require_nonempty(data_window, "crop_image")?;
    let samples = resample_exact(source, data_window, |x, y| {
        Point2::new(f64::from(x) + 0.5, f64::from(y) + 0.5)
            .map_err(|error| with_ops_context(error, "crop_image"))
    })?;
    rebuild_image(
        source,
        data_window,
        source.descriptor().display_window(),
        samples,
    )
}

/// Resizes the source data window into `data_window`.
///
/// The source data edges map exactly to the destination data edges. The same
/// forward mapping is applied to the display window, rounded outward to retain
/// its full spatial coverage.
pub fn resize(source: &Image, data_window: PixelBounds, filter: ResampleFilter) -> Result<Image> {
    require_nonempty(data_window, "resize_image")?;
    let source_window = source.descriptor().data_window();
    let scale_x = f64::from(data_window.width()) / f64::from(source_window.width());
    let scale_y = f64::from(data_window.height()) / f64::from(source_window.height());
    let translate_x = f64::from(data_window.min_x()) - scale_x * f64::from(source_window.min_x());
    let translate_y = f64::from(data_window.min_y()) - scale_y * f64::from(source_window.min_y());
    let forward = Matrix3::from_rows([
        [scale_x, 0.0, translate_x],
        [0.0, scale_y, translate_y],
        [0.0, 0.0, 1.0],
    ])
    .map_err(|error| with_ops_context(error, "resize_image"))?;
    transform(source, forward, data_window, filter)
}

/// Applies a forward source-to-destination transform into an explicit window.
///
/// Pixel centers are inverse-mapped for sampling, so transform composition uses
/// the same application order as [`Matrix3`]. The display window is transformed
/// by the same matrix and rounded outward. The source remains unchanged.
pub fn transform(
    source: &Image,
    source_to_destination: Matrix3,
    data_window: PixelBounds,
    filter: ResampleFilter,
) -> Result<Image> {
    require_nonempty(data_window, "transform_image")?;
    let destination_to_source = source_to_destination
        .checked_inverse()
        .map_err(|error| with_ops_context(error, "transform_image"))?;
    let display_window = transform_bounds_outward(
        source.descriptor().display_window(),
        source_to_destination,
        "transform_image",
    )?;

    let samples = match filter {
        ResampleFilter::Nearest => resample_exact(source, data_window, |x, y| {
            destination_to_source
                .checked_transform_point(pixel_center(x, y)?)
                .map_err(|error| with_ops_context(error, "transform_image"))
        })?,
        ResampleFilter::Bilinear => resample_bilinear(source, data_window, destination_to_source)?,
    };
    rebuild_image(source, data_window, display_window, samples)
}

/// Mirrors an image exactly within its display window.
pub fn flip(source: &Image, axis: FlipAxis) -> Result<Image> {
    let display = source.descriptor().display_window();
    let forward = match axis {
        FlipAxis::Horizontal => Matrix3::from_rows([
            [
                -1.0,
                0.0,
                f64::from(display.min_x()) + f64::from(display.max_x()),
            ],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ]),
        FlipAxis::Vertical => Matrix3::from_rows([
            [1.0, 0.0, 0.0],
            [
                0.0,
                -1.0,
                f64::from(display.min_y()) + f64::from(display.max_y()),
            ],
            [0.0, 0.0, 1.0],
        ]),
    }
    .map_err(|error| with_ops_context(error, "flip_image"))?;
    let data_window =
        transform_bounds_outward(source.descriptor().data_window(), forward, "flip_image")?;
    transform(source, forward, data_window, ResampleFilter::Nearest)
}

/// Rotates an image exactly in 90 degree increments around its display window.
///
/// Quarter turns keep the display origin and exchange its width and height.
pub fn rotate(source: &Image, turn: QuarterTurn) -> Result<Image> {
    let display = source.descriptor().display_window();
    let min_x = f64::from(display.min_x());
    let min_y = f64::from(display.min_y());
    let width = f64::from(display.width());
    let height = f64::from(display.height());
    let forward = match turn {
        QuarterTurn::Clockwise90 => Matrix3::from_rows([
            [0.0, -1.0, min_x + height + min_y],
            [1.0, 0.0, min_y - min_x],
            [0.0, 0.0, 1.0],
        ]),
        QuarterTurn::Half => Matrix3::from_rows([
            [
                -1.0,
                0.0,
                f64::from(display.min_x()) + f64::from(display.max_x()),
            ],
            [
                0.0,
                -1.0,
                f64::from(display.min_y()) + f64::from(display.max_y()),
            ],
            [0.0, 0.0, 1.0],
        ]),
        QuarterTurn::CounterClockwise90 => Matrix3::from_rows([
            [0.0, 1.0, min_x - min_y],
            [-1.0, 0.0, min_y + width + min_x],
            [0.0, 0.0, 1.0],
        ]),
    }
    .map_err(|error| with_ops_context(error, "rotate_image"))?;
    let data_window =
        transform_bounds_outward(source.descriptor().data_window(), forward, "rotate_image")?;
    transform(source, forward, data_window, ResampleFilter::Nearest)
}

/// Interpolates two images with a finite inclusive amount from 0 to 1.
///
/// Descriptors must match exactly. Recognized straight color is associated
/// before interpolation and restored afterward, preventing hidden transparent
/// color from creating fringes. The first image owns result metadata.
pub fn blend(first: &Image, second: &Image, amount: f32) -> Result<Image> {
    if !amount.is_finite() || !(0.0..=1.0).contains(&amount) {
        return Err(invalid(
            "blend_images",
            "image blend amount must be finite and between zero and one",
        ));
    }
    if first.descriptor() != second.descriptor() {
        return Err(invalid(
            "blend_images",
            "blended images must have identical descriptors",
        ));
    }
    if amount == 0.0 {
        return Ok(first.clone());
    }
    if amount == 1.0 {
        return Image::new_with_metadata(
            first.descriptor().clone(),
            second.samples().clone(),
            first.metadata().clone(),
        )
        .map_err(|error| with_ops_context(error, "blend_images"));
    }

    let descriptor = first.descriptor();
    let bounds = descriptor.data_window();
    let layout = AlphaLayout::from_channels(descriptor.channels());
    let mut first_pixel = vec![0.0; descriptor.channels().len()];
    let mut second_pixel = vec![0.0; descriptor.channels().len()];
    let inverse = 1.0 - f64::from(amount);
    let amount = f64::from(amount);
    let samples = generate_samples(descriptor, bounds, |x, y, output| {
        read_stored_pixel(first, i64::from(x), i64::from(y), &layout, &mut first_pixel)?;
        read_stored_pixel(
            second,
            i64::from(x),
            i64::from(y),
            &layout,
            &mut second_pixel,
        )?;
        associate_straight(&mut first_pixel, descriptor.alpha_mode(), &layout);
        associate_straight(&mut second_pixel, descriptor.alpha_mode(), &layout);
        for channel in 0..output.len() {
            output[channel] = first_pixel[channel] * inverse + second_pixel[channel] * amount;
        }
        unassociate_straight(output, descriptor.alpha_mode(), &layout);
        Ok(())
    })?;
    rebuild_image(first, bounds, descriptor.display_window(), samples)
}

/// Places `foreground` over `background` using Porter-Duff source-over.
///
/// Inputs must share pixel, channel, color, and alpha semantics. Their data and
/// display windows may differ; each result window is the union of its inputs.
/// Composite color channels must be recognized by [`AlphaLayout`], which keeps
/// multilayer and component-alpha bindings explicit. The foreground owns result
/// metadata.
pub fn composite_over(foreground: &Image, background: &Image) -> Result<Image> {
    require_binary_semantics(foreground, background, "composite_images")?;
    let descriptor = foreground.descriptor();
    let layout = AlphaLayout::from_channels(descriptor.channels());
    validate_composite_layout(descriptor.alpha_mode(), &layout)?;
    let data_window = descriptor
        .data_window()
        .union(background.descriptor().data_window());
    let display_window = descriptor
        .display_window()
        .union(background.descriptor().display_window());
    let mut foreground_pixel = vec![0.0; descriptor.channels().len()];
    let mut background_pixel = vec![0.0; descriptor.channels().len()];

    let samples = generate_samples(descriptor, data_window, |x, y, output| {
        let foreground_covered = read_composite_pixel(
            foreground,
            i64::from(x),
            i64::from(y),
            &layout,
            &mut foreground_pixel,
        )?;
        let background_covered = read_composite_pixel(
            background,
            i64::from(x),
            i64::from(y),
            &layout,
            &mut background_pixel,
        )?;
        associate_straight(&mut foreground_pixel, descriptor.alpha_mode(), &layout);
        associate_straight(&mut background_pixel, descriptor.alpha_mode(), &layout);

        for &channel in layout.alpha_channels() {
            let source_alpha = semantic_alpha(
                &foreground_pixel,
                Some(channel),
                descriptor.alpha_mode(),
                foreground_covered,
            );
            let background_alpha = semantic_alpha(
                &background_pixel,
                Some(channel),
                descriptor.alpha_mode(),
                background_covered,
            );
            output[channel.get()] = source_alpha + background_alpha * (1.0 - source_alpha);
        }
        if descriptor.alpha_mode() == AlphaMode::Opaque {
            for &channel in layout.alpha_channels() {
                output[channel.get()] = 1.0;
            }
        }
        for &channel in layout.color_channels() {
            let source_alpha = semantic_alpha(
                &foreground_pixel,
                layout.alpha_for(channel),
                descriptor.alpha_mode(),
                foreground_covered,
            );
            output[channel.get()] = foreground_pixel[channel.get()]
                + background_pixel[channel.get()] * (1.0 - source_alpha);
        }
        unassociate_straight(output, descriptor.alpha_mode(), &layout);
        Ok(())
    })?;
    rebuild_image(foreground, data_window, display_window, samples)
}

/// Reorders, selects, or fills channels into a compatible packed format.
///
/// Copied channel payloads remain bit-exact. Constants use the destination
/// representation's documented conversion. Spatial windows, color tags, and
/// metadata remain unchanged.
pub fn remap_channels(
    source: &Image,
    destination_format: PixelFormat,
    destination_alpha_mode: AlphaMode,
    destination_channels: ChannelList,
    mapping: &[ChannelSource],
) -> Result<Image> {
    if mapping.len() != destination_channels.len() {
        return Err(invalid(
            "remap_image_channels",
            "channel mapping count must match destination channel count",
        ));
    }
    for entry in mapping {
        match *entry {
            ChannelSource::Copy(index) if index.get() >= source.descriptor().channels().len() => {
                return Err(invalid(
                    "remap_image_channels",
                    "channel mapping references a source channel outside the image",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "channel_mapping")
                        .with_field("channel_index", index.get().to_string()),
                ));
            }
            ChannelSource::Constant(value)
                if matches!(
                    source.descriptor().sample_type(),
                    ImageSampleType::U8 | ImageSampleType::U16
                ) && (!value.is_finite() || !(0.0..=1.0).contains(&value)) =>
            {
                return Err(invalid(
                    "remap_image_channels",
                    "unsigned channel constants must be finite normalized values",
                ));
            }
            _ => {}
        }
    }

    let source_descriptor = source.descriptor();
    let descriptor = ImageDescriptor::new_with_color_tags(
        source_descriptor.data_window(),
        source_descriptor.display_window(),
        destination_format,
        source_descriptor.color_tags().clone(),
        destination_alpha_mode,
    )
    .and_then(|descriptor| descriptor.with_channels(destination_channels))
    .map_err(|error| with_ops_context(error, "remap_image_channels"))?;
    if descriptor.sample_type() != source_descriptor.sample_type() {
        return Err(invalid(
            "remap_image_channels",
            "channel remapping cannot change the scalar sample representation",
        ));
    }

    let samples = remap_sample_payloads(source, &descriptor, mapping)?;
    Image::new_with_metadata(descriptor, samples, source.metadata().clone())
        .map_err(|error| with_ops_context(error, "remap_image_channels"))
}

/// Replaces channel names without changing order, pixels, or image identity.
pub fn rename_channels(source: &Image, channels: ChannelList) -> Result<Image> {
    if channels.len() != source.descriptor().channels().len() {
        return Err(invalid(
            "rename_image_channels",
            "renamed channel count must match the source image",
        ));
    }
    let descriptor = descriptor_with_windows(
        source.descriptor(),
        source.descriptor().data_window(),
        source.descriptor().display_window(),
    )?
    .with_channels(channels)
    .map_err(|error| with_ops_context(error, "rename_image_channels"))?;
    Image::new_with_metadata(
        descriptor,
        source.samples().clone(),
        source.metadata().clone(),
    )
    .map_err(|error| with_ops_context(error, "rename_image_channels"))
}

fn resample_exact<F>(source: &Image, bounds: PixelBounds, mut mapping: F) -> Result<ImageSamples>
where
    F: FnMut(i32, i32) -> Result<Point2>,
{
    let channel_count = source.descriptor().channels().len();
    let sample_count = checked_sample_count(bounds, channel_count, "resample_image")?;
    let alpha_layout = AlphaLayout::from_channels(source.descriptor().channels());
    let alpha_channels = alpha_flags(channel_count, &alpha_layout);

    macro_rules! exact_values {
        ($values:expr, $zero:expr, $one:expr, $constructor:expr) => {{
            let mut output = Vec::new();
            try_reserve(&mut output, sample_count, "resample_image")?;
            for y in bounds.min_y()..bounds.max_y() {
                for x in bounds.min_x()..bounds.max_x() {
                    let point = mapping(x, y)?;
                    let source_base = nearest_source_base(source, point)?;
                    for channel in 0..channel_count {
                        let value = source_base
                            .map(|base| $values[base + channel])
                            .unwrap_or_else(|| {
                                if source.descriptor().alpha_mode() == AlphaMode::Opaque
                                    && alpha_channels[channel]
                                {
                                    $one
                                } else {
                                    $zero
                                }
                            });
                        output.push(value);
                    }
                }
            }
            $constructor(output)
        }};
    }

    Ok(match source.samples() {
        ImageSamples::U8(values) => {
            exact_values!(values, 0_u8, u8::MAX, ImageSamples::from_u8)
        }
        ImageSamples::U16(values) => {
            exact_values!(values, 0_u16, u16::MAX, ImageSamples::from_u16)
        }
        ImageSamples::F16(bits) => exact_values!(
            bits,
            0_u16,
            f16::from_f32(1.0).to_bits(),
            ImageSamples::from_f16_bits
        ),
        ImageSamples::F32(bits) => {
            exact_values!(bits, 0_u32, 1.0_f32.to_bits(), ImageSamples::from_f32_bits)
        }
    })
}

fn resample_bilinear(
    source: &Image,
    bounds: PixelBounds,
    destination_to_source: Matrix3,
) -> Result<ImageSamples> {
    let descriptor = source.descriptor();
    let layout = AlphaLayout::from_channels(descriptor.channels());
    let mut neighbor = vec![0.0; descriptor.channels().len()];
    generate_samples(descriptor, bounds, |x, y, output| {
        let source_point = destination_to_source
            .checked_transform_point(pixel_center(x, y)?)
            .map_err(|error| with_ops_context(error, "transform_image"))?;
        let lattice_x = source_point.x() - 0.5;
        let lattice_y = source_point.y() - 0.5;
        let Some(base_x) = finite_floor_i64(lattice_x) else {
            fill_stored_pixel(output, descriptor.alpha_mode(), &layout);
            return Ok(());
        };
        let Some(base_y) = finite_floor_i64(lattice_y) else {
            fill_stored_pixel(output, descriptor.alpha_mode(), &layout);
            return Ok(());
        };
        let fraction_x = lattice_x - base_x as f64;
        let fraction_y = lattice_y - base_y as f64;
        output.fill(0.0);
        for (offset_y, weight_y) in [(0_i64, 1.0 - fraction_y), (1, fraction_y)] {
            for (offset_x, weight_x) in [(0_i64, 1.0 - fraction_x), (1, fraction_x)] {
                let sample_x = base_x.checked_add(offset_x);
                let sample_y = base_y.checked_add(offset_y);
                if let (Some(sample_x), Some(sample_y)) = (sample_x, sample_y) {
                    read_stored_pixel(source, sample_x, sample_y, &layout, &mut neighbor)?;
                } else {
                    fill_stored_pixel(&mut neighbor, descriptor.alpha_mode(), &layout);
                }
                associate_straight(&mut neighbor, descriptor.alpha_mode(), &layout);
                let weight = weight_x * weight_y;
                for channel in 0..output.len() {
                    output[channel] += neighbor[channel] * weight;
                }
            }
        }
        unassociate_straight(output, descriptor.alpha_mode(), &layout);
        Ok(())
    })
}

fn generate_samples<F>(
    descriptor: &ImageDescriptor,
    bounds: PixelBounds,
    mut generate: F,
) -> Result<ImageSamples>
where
    F: FnMut(i32, i32, &mut [f64]) -> Result<()>,
{
    let channel_count = descriptor.channels().len();
    let sample_count = checked_sample_count(bounds, channel_count, "generate_image_samples")?;
    let mut pixel = vec![0.0; channel_count];

    macro_rules! generate_values {
        ($type:ty, $convert:expr, $constructor:expr) => {{
            let mut output: Vec<$type> = Vec::new();
            try_reserve(&mut output, sample_count, "generate_image_samples")?;
            for y in bounds.min_y()..bounds.max_y() {
                for x in bounds.min_x()..bounds.max_x() {
                    pixel.fill(0.0);
                    generate(x, y, &mut pixel)?;
                    output.extend(pixel.iter().copied().map($convert));
                }
            }
            $constructor(output)
        }};
    }

    Ok(match descriptor.sample_type() {
        ImageSampleType::U8 => generate_values!(u8, normalized_u8, ImageSamples::from_u8),
        ImageSampleType::U16 => generate_values!(u16, normalized_u16, ImageSamples::from_u16),
        ImageSampleType::U32 => {
            return Err(unsupported(
                "generate_image_samples",
                "dense image generation does not support unsigned 32-bit samples",
            ))
        }
        ImageSampleType::F16 => generate_values!(
            u16,
            |value: f64| f16::from_f32(value as f32).to_bits(),
            ImageSamples::from_f16_bits
        ),
        ImageSampleType::F32 => generate_values!(
            u32,
            |value: f64| (value as f32).to_bits(),
            ImageSamples::from_f32_bits
        ),
    })
}

fn read_stored_pixel(
    image: &Image,
    x: i64,
    y: i64,
    layout: &AlphaLayout,
    output: &mut [f64],
) -> Result<()> {
    if output.len() != image.descriptor().channels().len() {
        return Err(invalid(
            "read_image_pixel",
            "pixel scratch storage does not match the image channel count",
        ));
    }
    let bounds = image.descriptor().data_window();
    if x < i64::from(bounds.min_x())
        || x >= i64::from(bounds.max_x())
        || y < i64::from(bounds.min_y())
        || y >= i64::from(bounds.max_y())
    {
        fill_stored_pixel(output, image.descriptor().alpha_mode(), layout);
        return Ok(());
    }
    let x = i32::try_from(x).map_err(|_| {
        exhausted(
            "read_image_pixel",
            "pixel coordinate cannot be represented by the image model",
        )
    })?;
    let y = i32::try_from(y).map_err(|_| {
        exhausted(
            "read_image_pixel",
            "pixel coordinate cannot be represented by the image model",
        )
    })?;
    let base = source_pixel_base(image, x, y)?;
    for (channel, value) in output.iter_mut().enumerate() {
        *value = decode_sample(image.samples(), base + channel);
    }
    Ok(())
}

fn read_composite_pixel(
    image: &Image,
    x: i64,
    y: i64,
    layout: &AlphaLayout,
    output: &mut [f64],
) -> Result<bool> {
    let bounds = image.descriptor().data_window();
    let covered = x >= i64::from(bounds.min_x())
        && x < i64::from(bounds.max_x())
        && y >= i64::from(bounds.min_y())
        && y < i64::from(bounds.max_y());
    if covered {
        read_stored_pixel(image, x, y, layout, output)?;
        if image.descriptor().alpha_mode() == AlphaMode::Opaque {
            for &alpha in layout.alpha_channels() {
                output[alpha.get()] = 1.0;
            }
        }
    } else {
        output.fill(0.0);
    }
    Ok(covered)
}

fn semantic_alpha(
    pixel: &[f64],
    alpha: Option<ChannelIndex>,
    alpha_mode: AlphaMode,
    covered: bool,
) -> f64 {
    if !covered {
        0.0
    } else if alpha_mode == AlphaMode::Opaque {
        1.0
    } else {
        alpha.map_or(0.0, |alpha| pixel[alpha.get()])
    }
}

fn decode_sample(samples: &ImageSamples, index: usize) -> f64 {
    match samples {
        ImageSamples::U8(values) => f64::from(values[index]) / f64::from(u8::MAX),
        ImageSamples::U16(values) => f64::from(values[index]) / f64::from(u16::MAX),
        ImageSamples::F16(bits) => f64::from(f16::from_bits(bits[index]).to_f32()),
        ImageSamples::F32(bits) => f64::from(f32::from_bits(bits[index])),
    }
}

fn fill_stored_pixel(output: &mut [f64], alpha_mode: AlphaMode, layout: &AlphaLayout) {
    output.fill(0.0);
    if alpha_mode == AlphaMode::Opaque {
        for &alpha in layout.alpha_channels() {
            output[alpha.get()] = 1.0;
        }
    }
}

fn associate_straight(pixel: &mut [f64], alpha_mode: AlphaMode, layout: &AlphaLayout) {
    if alpha_mode != AlphaMode::Straight {
        return;
    }
    for &color in layout.color_channels() {
        if let Some(alpha) = layout.alpha_for(color) {
            pixel[color.get()] *= pixel[alpha.get()];
        }
    }
}

fn unassociate_straight(pixel: &mut [f64], alpha_mode: AlphaMode, layout: &AlphaLayout) {
    if alpha_mode != AlphaMode::Straight {
        return;
    }
    for &color in layout.color_channels() {
        if let Some(alpha) = layout.alpha_for(color) {
            let alpha = pixel[alpha.get()];
            pixel[color.get()] = if alpha == 0.0 {
                0.0
            } else {
                pixel[color.get()] / alpha
            };
        }
    }
}

fn remap_sample_payloads(
    source: &Image,
    destination: &ImageDescriptor,
    mapping: &[ChannelSource],
) -> Result<ImageSamples> {
    let pixel_count =
        checked_sample_count(source.descriptor().data_window(), 1, "remap_image_channels")?;
    let destination_count = destination.required_sample_count()?;
    let source_channels = source.descriptor().channels().len();

    macro_rules! remap_values {
        ($values:expr, $constant:expr, $constructor:expr, $type:ty) => {{
            let mut output: Vec<$type> = Vec::new();
            try_reserve(&mut output, destination_count, "remap_image_channels")?;
            for pixel in 0..pixel_count {
                let base = pixel
                    .checked_mul(source_channels)
                    .ok_or_else(|| exhausted("remap_image_channels", "source index overflowed"))?;
                for entry in mapping {
                    output.push(match *entry {
                        ChannelSource::Copy(index) => $values[base + index.get()],
                        ChannelSource::Constant(value) => $constant(value),
                    });
                }
            }
            $constructor(output)
        }};
    }

    Ok(match source.samples() {
        ImageSamples::U8(values) => remap_values!(
            values,
            |value: f32| normalized_u8(f64::from(value)),
            ImageSamples::from_u8,
            u8
        ),
        ImageSamples::U16(values) => remap_values!(
            values,
            |value: f32| normalized_u16(f64::from(value)),
            ImageSamples::from_u16,
            u16
        ),
        ImageSamples::F16(bits) => remap_values!(
            bits,
            |value: f32| f16::from_f32(value).to_bits(),
            ImageSamples::from_f16_bits,
            u16
        ),
        ImageSamples::F32(bits) => remap_values!(
            bits,
            |value: f32| value.to_bits(),
            ImageSamples::from_f32_bits,
            u32
        ),
    })
}

fn require_binary_semantics(first: &Image, second: &Image, operation: &'static str) -> Result<()> {
    let first = first.descriptor();
    let second = second.descriptor();
    if first.pixel_format() != second.pixel_format()
        || first.color_tags() != second.color_tags()
        || first.alpha_mode() != second.alpha_mode()
        || first.channels() != second.channels()
        || first.sample_type() != second.sample_type()
    {
        return Err(invalid(
            operation,
            "binary image inputs must share pixel, channel, color, and alpha semantics",
        ));
    }
    Ok(())
}

fn validate_composite_layout(alpha_mode: AlphaMode, layout: &AlphaLayout) -> Result<()> {
    let mut supported = vec![false; layout.channel_count()];
    for &channel in layout.color_channels() {
        supported[channel.get()] = true;
        if alpha_mode != AlphaMode::Opaque && layout.alpha_for(channel).is_none() {
            return Err(invalid(
                "composite_images",
                "every non-opaque composite color channel requires explicit alpha",
            ));
        }
    }
    for &channel in layout.alpha_channels() {
        supported[channel.get()] = true;
    }
    if supported.iter().any(|supported| !supported) {
        return Err(unsupported(
            "composite_images",
            "source-over supports recognized color and alpha channels only",
        ));
    }
    Ok(())
}

fn rebuild_image(
    source: &Image,
    data_window: PixelBounds,
    display_window: PixelBounds,
    samples: ImageSamples,
) -> Result<Image> {
    let descriptor = descriptor_with_windows(source.descriptor(), data_window, display_window)?;
    Image::new_with_metadata(descriptor, samples, source.metadata().clone())
        .map_err(|error| with_ops_context(error, "build_image_result"))
}

fn descriptor_with_windows(
    source: &ImageDescriptor,
    data_window: PixelBounds,
    display_window: PixelBounds,
) -> Result<ImageDescriptor> {
    ImageDescriptor::new_with_color_tags(
        data_window,
        display_window,
        source.pixel_format(),
        source.color_tags().clone(),
        source.alpha_mode(),
    )
    .and_then(|descriptor| descriptor.with_channels(source.channels().clone()))
    .map_err(|error| with_ops_context(error, "build_image_descriptor"))
}

fn nearest_source_base(source: &Image, point: Point2) -> Result<Option<usize>> {
    let Some(x) = finite_floor_i64(point.x()) else {
        return Ok(None);
    };
    let Some(y) = finite_floor_i64(point.y()) else {
        return Ok(None);
    };
    let bounds = source.descriptor().data_window();
    if x < i64::from(bounds.min_x())
        || x >= i64::from(bounds.max_x())
        || y < i64::from(bounds.min_y())
        || y >= i64::from(bounds.max_y())
    {
        return Ok(None);
    }
    let x = i32::try_from(x).map_err(|_| {
        exhausted(
            "resample_image",
            "source coordinate cannot be represented by the image model",
        )
    })?;
    let y = i32::try_from(y).map_err(|_| {
        exhausted(
            "resample_image",
            "source coordinate cannot be represented by the image model",
        )
    })?;
    source_pixel_base(source, x, y).map(Some)
}

fn source_pixel_base(source: &Image, x: i32, y: i32) -> Result<usize> {
    let bounds = source.descriptor().data_window();
    let local_x = usize::try_from(i64::from(x) - i64::from(bounds.min_x())).map_err(|_| {
        exhausted(
            "calculate_image_index",
            "pixel x coordinate cannot be represented on this platform",
        )
    })?;
    let local_y = usize::try_from(i64::from(y) - i64::from(bounds.min_y())).map_err(|_| {
        exhausted(
            "calculate_image_index",
            "pixel y coordinate cannot be represented on this platform",
        )
    })?;
    let width = usize::try_from(bounds.width()).map_err(|_| {
        exhausted(
            "calculate_image_index",
            "image width cannot be represented on this platform",
        )
    })?;
    local_y
        .checked_mul(width)
        .and_then(|row| row.checked_add(local_x))
        .and_then(|pixel| pixel.checked_mul(source.descriptor().channels().len()))
        .ok_or_else(|| exhausted("calculate_image_index", "image sample index overflowed"))
}

fn transform_bounds_outward(
    bounds: PixelBounds,
    transform: Matrix3,
    operation: &'static str,
) -> Result<PixelBounds> {
    let transformed = bounds
        .to_rect()
        .checked_transform_bounds(transform)
        .map_err(|error| with_ops_context(error, operation))?;
    let min_x = outward_edge(transformed.min().x().floor(), operation)?;
    let min_y = outward_edge(transformed.min().y().floor(), operation)?;
    let max_x = outward_edge(transformed.max().x().ceil(), operation)?;
    let max_y = outward_edge(transformed.max().y().ceil(), operation)?;
    PixelBounds::new(min_x, min_y, max_x, max_y).map_err(|error| with_ops_context(error, operation))
}

fn outward_edge(value: f64, operation: &'static str) -> Result<i32> {
    if !value.is_finite() || value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
        return Err(exhausted(
            operation,
            "transformed image bounds exceed the supported coordinate range",
        ));
    }
    Ok(value as i32)
}

fn checked_sample_count(
    bounds: PixelBounds,
    channel_count: usize,
    operation: &'static str,
) -> Result<usize> {
    let width = usize::try_from(bounds.width()).map_err(|_| {
        exhausted(
            operation,
            "image width cannot be represented on this platform",
        )
    })?;
    let height = usize::try_from(bounds.height()).map_err(|_| {
        exhausted(
            operation,
            "image height cannot be represented on this platform",
        )
    })?;
    width
        .checked_mul(height)
        .and_then(|pixels| pixels.checked_mul(channel_count))
        .ok_or_else(|| exhausted(operation, "image sample count overflowed"))
}

fn try_reserve<T>(values: &mut Vec<T>, count: usize, operation: &'static str) -> Result<()> {
    values.try_reserve_exact(count).map_err(|_| {
        exhausted(
            operation,
            "image result allocation exceeds available memory",
        )
    })
}

fn alpha_flags(channel_count: usize, layout: &AlphaLayout) -> Vec<bool> {
    let mut flags = vec![false; channel_count];
    for &alpha in layout.alpha_channels() {
        flags[alpha.get()] = true;
    }
    flags
}

fn pixel_center(x: i32, y: i32) -> Result<Point2> {
    Point2::new(f64::from(x) + 0.5, f64::from(y) + 0.5)
        .map_err(|error| with_ops_context(error, "calculate_pixel_center"))
}

fn finite_floor_i64(value: f64) -> Option<i64> {
    let value = value.floor();
    if value.is_finite() && value >= i64::MIN as f64 && value <= i64::MAX as f64 {
        Some(value as i64)
    } else {
        None
    }
}

fn normalized_u8(value: f64) -> u8 {
    (value.clamp(0.0, 1.0) * f64::from(u8::MAX) + 0.5).floor() as u8
}

fn normalized_u16(value: f64) -> u16 {
    (value.clamp(0.0, 1.0) * f64::from(u16::MAX) + 0.5).floor() as u16
}

fn require_nonempty(bounds: PixelBounds, operation: &'static str) -> Result<()> {
    if bounds.is_empty() {
        Err(invalid(
            operation,
            "image operation destination bounds must not be empty",
        ))
    } else {
        Ok(())
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

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
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

fn with_ops_context(error: Error, operation: &'static str) -> Error {
    error.with_context(ErrorContext::new(COMPONENT, operation))
}
