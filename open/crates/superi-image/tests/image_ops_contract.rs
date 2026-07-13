use std::sync::Arc;

use superi_core::color_space::ColorSpace;
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::geometry::{Matrix3, PixelBounds, Vector2};
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_image::channels::{ChannelIndex, ChannelList};
use superi_image::metadata::{ImageColorTags, ImageMetadata, ImageMetadataValue, ImageOrientation};
use superi_image::ops::{
    blend, composite_over, crop, flip, remap_channels, rename_channels, resize, rotate, transform,
    ChannelSource, FlipAxis, QuarterTurn, ResampleFilter,
};
use superi_image::value::{Image, ImageDescriptor, ImageSamples};

#[test]
fn crop_keeps_coordinates_identity_and_metadata_with_defined_outside_fill() {
    let data = bounds(-1, 2, 2, 2);
    let display = bounds(-3, -2, 8, 7);
    let image = tagged_rgba_f32(
        data,
        display,
        [
            1.0, 2.0, 3.0, 1.0, 4.0, 5.0, 6.0, 0.5, 7.0, 8.0, 9.0, 0.25, 10.0, 11.0, 12.0, 0.0,
        ],
        AlphaMode::Straight,
        "crop-source",
    );
    let requested = bounds(-2, 2, 3, 2);

    let result = crop(&image, requested).unwrap();

    assert_eq!(result.descriptor().data_window(), requested);
    assert_eq!(result.descriptor().display_window(), display);
    assert_eq!(
        result.descriptor().channels(),
        image.descriptor().channels()
    );
    assert_eq!(
        result.descriptor().color_tags(),
        image.descriptor().color_tags()
    );
    assert_eq!(result.descriptor().alpha_mode(), AlphaMode::Straight);
    assert_eq!(result.metadata(), image.metadata());
    assert_eq!(
        float_values(&result),
        [
            0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 1.0, 4.0, 5.0, 6.0, 0.5, 0.0, 0.0, 0.0, 0.0, 7.0,
            8.0, 9.0, 0.25, 10.0, 11.0, 12.0, 0.0,
        ]
    );
}

#[test]
fn resize_supports_exact_nearest_and_alpha_aware_bilinear_sampling() {
    let gray = Image::new(
        ImageDescriptor::new(
            bounds(5, -1, 2, 1),
            bounds(5, -1, 2, 1),
            PixelFormat::R8Unorm,
            ColorSpace::ACESCG,
            AlphaMode::Opaque,
        )
        .unwrap(),
        ImageSamples::from_u8([10, 20]),
    )
    .unwrap();
    let nearest = resize(&gray, bounds(10, 3, 4, 1), ResampleFilter::Nearest).unwrap();
    assert_eq!(nearest.samples().u8_values(), Some(&[10, 10, 20, 20][..]));
    assert_eq!(nearest.descriptor().data_window(), bounds(10, 3, 4, 1));
    assert_eq!(nearest.descriptor().display_window(), bounds(10, 3, 4, 1));

    let straight = tagged_rgba_f32(
        bounds(0, 0, 2, 1),
        bounds(0, 0, 2, 1),
        [1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0],
        AlphaMode::Straight,
        "resize-source",
    );
    let bilinear = resize(&straight, bounds(0, 0, 1, 1), ResampleFilter::Bilinear).unwrap();

    assert_close(&float_values(&bilinear), &[1.0, 0.0, 0.0, 0.5]);
    assert_eq!(bilinear.descriptor().alpha_mode(), AlphaMode::Straight);
    assert_eq!(bilinear.metadata(), straight.metadata());
}

#[test]
fn affine_transform_uses_forward_order_and_explicit_destination_extent() {
    let image = Image::new(
        ImageDescriptor::new(
            bounds(-2, 4, 2, 1),
            bounds(-4, 2, 6, 5),
            PixelFormat::R16Unorm,
            ColorSpace::ACESCG,
            AlphaMode::Opaque,
        )
        .unwrap(),
        ImageSamples::from_u16([123, 456]),
    )
    .unwrap();
    let forward = Matrix3::translation(Vector2::new(7.0, -3.0).unwrap());

    let result = transform(&image, forward, bounds(5, 1, 2, 1), ResampleFilter::Nearest).unwrap();

    assert_eq!(result.samples().u16_values(), Some(&[123, 456][..]));
    assert_eq!(result.descriptor().data_window(), bounds(5, 1, 2, 1));
    assert_eq!(result.descriptor().display_window(), bounds(3, -1, 6, 5));
}

#[test]
fn flip_and_quarter_rotation_are_exact_and_display_relative() {
    let image = Image::new(
        ImageDescriptor::new(
            bounds(10, 20, 2, 2),
            bounds(10, 20, 2, 2),
            PixelFormat::R8Unorm,
            ColorSpace::ACESCG,
            AlphaMode::Opaque,
        )
        .unwrap(),
        ImageSamples::from_u8([1, 2, 3, 4]),
    )
    .unwrap();

    assert_eq!(
        flip(&image, FlipAxis::Horizontal)
            .unwrap()
            .samples()
            .u8_values(),
        Some(&[2, 1, 4, 3][..])
    );
    assert_eq!(
        flip(&image, FlipAxis::Vertical)
            .unwrap()
            .samples()
            .u8_values(),
        Some(&[3, 4, 1, 2][..])
    );
    assert_eq!(
        rotate(&image, QuarterTurn::Clockwise90)
            .unwrap()
            .samples()
            .u8_values(),
        Some(&[3, 1, 4, 2][..])
    );
    assert_eq!(
        rotate(&image, QuarterTurn::Half)
            .unwrap()
            .samples()
            .u8_values(),
        Some(&[4, 3, 2, 1][..])
    );
    assert_eq!(
        rotate(&image, QuarterTurn::CounterClockwise90)
            .unwrap()
            .samples()
            .u8_values(),
        Some(&[2, 4, 1, 3][..])
    );

    let rectangular = Image::new(
        ImageDescriptor::new(
            bounds(10, 20, 2, 3),
            bounds(10, 20, 2, 3),
            PixelFormat::R8Unorm,
            ColorSpace::ACESCG,
            AlphaMode::Opaque,
        )
        .unwrap(),
        ImageSamples::from_u8([1, 2, 3, 4, 5, 6]),
    )
    .unwrap();
    let rotated = rotate(&rectangular, QuarterTurn::Clockwise90).unwrap();
    assert_eq!(rotated.samples().u8_values(), Some(&[5, 3, 1, 6, 4, 2][..]));
    assert_eq!(rotated.descriptor().data_window(), bounds(10, 20, 3, 2));
    assert_eq!(rotated.descriptor().display_window(), bounds(10, 20, 3, 2));
}

#[test]
fn blend_interpolates_associated_color_without_transparent_color_fringes() {
    let first = tagged_rgba_f32(
        bounds(0, 0, 1, 1),
        bounds(0, 0, 1, 1),
        [1.0, 0.0, 0.0, 1.0],
        AlphaMode::Straight,
        "first",
    );
    let second = tagged_rgba_f32(
        bounds(0, 0, 1, 1),
        bounds(0, 0, 1, 1),
        [0.0, 0.0, 1.0, 0.0],
        AlphaMode::Straight,
        "second",
    );

    let result = blend(&first, &second, 0.5).unwrap();

    assert_close(&float_values(&result), &[1.0, 0.0, 0.0, 0.5]);
    assert_eq!(result.metadata(), first.metadata());
}

#[test]
fn blend_endpoints_preserve_exact_ieee_payloads() {
    let descriptor = ImageDescriptor::new(
        bounds(0, 0, 1, 1),
        bounds(0, 0, 1, 1),
        PixelFormat::Rgba32Float,
        ColorSpace::ACESCG,
        AlphaMode::Premultiplied,
    )
    .unwrap();
    let first_bits = [
        0x7fc0_1234,
        (-0.0_f32).to_bits(),
        2.0_f32.to_bits(),
        1.0_f32.to_bits(),
    ];
    let second_bits = [
        (-3.0_f32).to_bits(),
        4.0_f32.to_bits(),
        0x7fc0_5678,
        0.5_f32.to_bits(),
    ];
    let first = Image::new_with_metadata(
        descriptor.clone(),
        ImageSamples::from_f32_bits(first_bits),
        tagged_metadata("first-endpoint"),
    )
    .unwrap();
    let second = Image::new_with_metadata(
        descriptor,
        ImageSamples::from_f32_bits(second_bits),
        tagged_metadata("second-endpoint"),
    )
    .unwrap();

    let at_first = blend(&first, &second, 0.0).unwrap();
    let at_second = blend(&first, &second, 1.0).unwrap();

    assert_eq!(at_first.samples().f32_bits(), Some(first_bits.as_slice()));
    assert_eq!(at_second.samples().f32_bits(), Some(second_bits.as_slice()));
    assert_eq!(at_second.metadata(), first.metadata());
}

#[test]
fn source_over_composite_uses_union_windows_and_porter_duff_alpha() {
    let foreground = tagged_rgba_f32(
        bounds(1, 0, 1, 1),
        bounds(0, 0, 2, 1),
        [1.0, 0.0, 0.0, 0.5],
        AlphaMode::Straight,
        "foreground",
    );
    let background = tagged_rgba_f32(
        bounds(0, 0, 2, 1),
        bounds(0, 0, 2, 1),
        [0.0, 0.0, 1.0, 1.0, 0.0, 0.0, 1.0, 1.0],
        AlphaMode::Straight,
        "background",
    );

    let result = composite_over(&foreground, &background).unwrap();

    assert_eq!(result.descriptor().data_window(), bounds(0, 0, 2, 1));
    assert_eq!(result.descriptor().display_window(), bounds(0, 0, 2, 1));
    assert_close(
        &float_values(&result),
        &[0.0, 0.0, 1.0, 1.0, 0.5, 0.0, 0.5, 1.0],
    );
    assert_eq!(result.metadata(), foreground.metadata());
}

#[test]
fn opaque_composite_treats_pixels_outside_foreground_data_as_uncovered() {
    let foreground = Image::new(
        ImageDescriptor::new(
            bounds(1, 0, 1, 1),
            bounds(0, 0, 2, 1),
            PixelFormat::R8Unorm,
            ColorSpace::ACESCG,
            AlphaMode::Opaque,
        )
        .unwrap(),
        ImageSamples::from_u8([100]),
    )
    .unwrap();
    let background = Image::new(
        ImageDescriptor::new(
            bounds(0, 0, 2, 1),
            bounds(0, 0, 2, 1),
            PixelFormat::R8Unorm,
            ColorSpace::ACESCG,
            AlphaMode::Opaque,
        )
        .unwrap(),
        ImageSamples::from_u8([10, 20]),
    )
    .unwrap();

    let result = composite_over(&foreground, &background).unwrap();

    assert_eq!(result.samples().u8_values(), Some(&[10, 100][..]));
}

#[test]
fn opaque_composite_fills_union_gaps_with_opaque_black() {
    let foreground = Image::new(
        ImageDescriptor::new(
            bounds(2, 0, 1, 1),
            bounds(0, 0, 3, 1),
            PixelFormat::Rgba8Unorm,
            ColorSpace::ACESCG,
            AlphaMode::Opaque,
        )
        .unwrap(),
        ImageSamples::from_u8([100, 0, 0, 7]),
    )
    .unwrap();
    let background = Image::new(
        ImageDescriptor::new(
            bounds(0, 0, 1, 1),
            bounds(0, 0, 3, 1),
            PixelFormat::Rgba8Unorm,
            ColorSpace::ACESCG,
            AlphaMode::Opaque,
        )
        .unwrap(),
        ImageSamples::from_u8([10, 0, 0, 9]),
    )
    .unwrap();

    let result = composite_over(&foreground, &background).unwrap();

    assert_eq!(result.descriptor().data_window(), bounds(0, 0, 3, 1));
    assert_eq!(
        result.samples().u8_values(),
        Some(&[10, 0, 0, 255, 0, 0, 0, 255, 100, 0, 0, 255][..])
    );
}

#[test]
fn channel_mapping_reorders_copies_fills_and_renames_without_losing_identity() {
    let source = Image::new_with_metadata(
        ImageDescriptor::new_with_color_tags(
            bounds(-1, -1, 1, 1),
            bounds(-2, -2, 3, 3),
            PixelFormat::Rgba8Unorm,
            tagged_color(),
            AlphaMode::Straight,
        )
        .unwrap(),
        ImageSamples::from_u8([10, 20, 30, 40]),
        tagged_metadata("channels"),
    )
    .unwrap();
    let destination_channels =
        ChannelList::from_full_names(["beauty.B", "beauty.G", "beauty.R", "beauty.A"]).unwrap();
    let result = remap_channels(
        &source,
        PixelFormat::Bgra8Unorm,
        AlphaMode::Straight,
        destination_channels.clone(),
        &[
            ChannelSource::Copy(ChannelIndex::new(2)),
            ChannelSource::Copy(ChannelIndex::new(1)),
            ChannelSource::Copy(ChannelIndex::new(0)),
            ChannelSource::Constant(1.0),
        ],
    )
    .unwrap();

    assert_eq!(result.samples().u8_values(), Some(&[30, 20, 10, 255][..]));
    assert_eq!(result.descriptor().channels(), &destination_channels);
    assert_eq!(
        result.descriptor().color_tags(),
        source.descriptor().color_tags()
    );
    assert_eq!(result.metadata(), source.metadata());
    assert_eq!(
        result.descriptor().data_window(),
        source.descriptor().data_window()
    );
    assert_eq!(
        result.descriptor().display_window(),
        source.descriptor().display_window()
    );

    let renamed = rename_channels(
        &source,
        ChannelList::from_full_names(["left.R", "left.G", "left.B", "left.A"]).unwrap(),
    )
    .unwrap();
    assert_eq!(renamed.samples(), source.samples());
    assert_eq!(renamed.metadata(), source.metadata());
}

#[test]
fn invalid_operations_fail_with_actionable_shared_errors() {
    let image = tagged_rgba_f32(
        bounds(0, 0, 1, 1),
        bounds(0, 0, 1, 1),
        [0.0, 0.0, 0.0, 1.0],
        AlphaMode::Straight,
        "errors",
    );
    let empty = PixelBounds::from_origin_size(0, 0, 0, 1).unwrap();

    assert_ops_error(
        crop(&image, empty).unwrap_err(),
        ErrorCategory::InvalidInput,
    );
    assert_ops_error(
        resize(&image, empty, ResampleFilter::Nearest).unwrap_err(),
        ErrorCategory::InvalidInput,
    );
    let huge = PixelBounds::new(i32::MIN, i32::MIN, i32::MAX, i32::MAX).unwrap();
    assert_ops_error(
        crop(&image, huge).unwrap_err(),
        ErrorCategory::ResourceExhausted,
    );
    assert_ops_error(
        blend(&image, &image, 1.5).unwrap_err(),
        ErrorCategory::InvalidInput,
    );

    let singular = Matrix3::scale(Vector2::new(0.0, 1.0).unwrap());
    let error = transform(
        &image,
        singular,
        bounds(0, 0, 1, 1),
        ResampleFilter::Nearest,
    )
    .unwrap_err();
    assert!(error
        .contexts()
        .iter()
        .any(|context| context.component() == "superi-image.ops"));

    let bad_map = remap_channels(
        &image,
        PixelFormat::R8Unorm,
        AlphaMode::Opaque,
        ChannelList::from_full_names(["Y"]).unwrap(),
        &[ChannelSource::Copy(ChannelIndex::new(99))],
    )
    .unwrap_err();
    assert_ops_error(bad_map, ErrorCategory::InvalidInput);

    let custom = rename_channels(
        &image,
        ChannelList::from_full_names(["one", "two", "three", "four"]).unwrap(),
    )
    .unwrap();
    assert_ops_error(
        composite_over(&custom, &custom).unwrap_err(),
        ErrorCategory::Unsupported,
    );
}

#[test]
fn operation_contracts_are_safe_to_share_between_engine_owners() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<ResampleFilter>();
    assert_send_sync::<FlipAxis>();
    assert_send_sync::<QuarterTurn>();
    assert_send_sync::<ChannelSource>();
}

fn tagged_rgba_f32(
    data: PixelBounds,
    display: PixelBounds,
    values: impl IntoIterator<Item = f32>,
    alpha_mode: AlphaMode,
    tag: &str,
) -> Image {
    Image::new_with_metadata(
        ImageDescriptor::new_with_color_tags(
            data,
            display,
            PixelFormat::Rgba32Float,
            tagged_color(),
            alpha_mode,
        )
        .unwrap(),
        ImageSamples::from_f32(values),
        tagged_metadata(tag),
    )
    .unwrap()
}

fn tagged_color() -> ImageColorTags {
    ImageColorTags::new(ColorSpace::ACESCG)
        .with_named_space("ACEScg")
        .unwrap()
        .with_icc_profile(Arc::from([1_u8, 3, 5, 7]))
        .unwrap()
}

fn tagged_metadata(tag: &str) -> ImageMetadata {
    let mut metadata = ImageMetadata::new().with_orientation(ImageOrientation::TopLeft);
    metadata
        .insert("test.owner", ImageMetadataValue::Text(tag.to_owned()))
        .unwrap();
    metadata
}

fn float_values(image: &Image) -> Vec<f32> {
    (0..image.samples().len())
        .map(|index| image.samples().float_value(index).unwrap())
        .collect()
}

fn bounds(min_x: i32, min_y: i32, width: u32, height: u32) -> PixelBounds {
    PixelBounds::from_origin_size(min_x, min_y, width, height).unwrap()
}

fn assert_close(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert!(
            (actual - expected).abs() <= 1.0e-6,
            "expected {expected}, got {actual}"
        );
    }
}

fn assert_ops_error(error: superi_core::error::Error, category: ErrorCategory) {
    assert_eq!(error.category(), category);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(error.contexts()[0].component(), "superi-image.ops");
}
