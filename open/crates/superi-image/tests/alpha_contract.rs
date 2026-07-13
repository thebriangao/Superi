use superi_core::color_space::ColorSpace;
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::geometry::PixelBounds;
use superi_core::pixel::AlphaMode;
use superi_core::pixel::PixelFormat;
use superi_image::alpha::{AlphaLayout, AlphaTransform, PremultiplicationRule};
use superi_image::channels::{ChannelIndex, ChannelList};
use superi_image::metadata::ImageMetadataValue;
use superi_image::value::{Image, ImageDescriptor, ImageSampleType, ImageSamples};

#[test]
fn alpha_layout_resolves_component_layer_and_enclosing_alpha() {
    let channels = ChannelList::from_full_names([
        "A",
        "beauty.A",
        "beauty.R",
        "beauty.G",
        "beauty.B",
        "beauty.specular.AR",
        "beauty.specular.A",
        "beauty.specular.R",
        "beauty.specular.G",
        "beauty.specular.B",
        "beauty.specular.Z",
        "beauty.specular.id",
        "emission.R",
    ])
    .unwrap();
    let layout = AlphaLayout::from_channels(&channels);

    assert_eq!(layout.channel_count(), channels.len());
    assert_eq!(
        layout.alpha_for(ChannelIndex::new(2)),
        Some(ChannelIndex::new(1))
    );
    assert_eq!(
        layout.alpha_for(ChannelIndex::new(7)),
        Some(ChannelIndex::new(5))
    );
    assert_eq!(
        layout.alpha_for(ChannelIndex::new(8)),
        Some(ChannelIndex::new(6))
    );
    assert_eq!(
        layout.alpha_for(ChannelIndex::new(9)),
        Some(ChannelIndex::new(6))
    );
    assert_eq!(layout.alpha_for(ChannelIndex::new(10)), None);
    assert_eq!(layout.alpha_for(ChannelIndex::new(11)), None);
    assert_eq!(
        layout.alpha_for(ChannelIndex::new(12)),
        Some(ChannelIndex::new(0))
    );
    assert_eq!(
        layout.color_channels(),
        &[
            ChannelIndex::new(2),
            ChannelIndex::new(3),
            ChannelIndex::new(4),
            ChannelIndex::new(7),
            ChannelIndex::new(8),
            ChannelIndex::new(9),
            ChannelIndex::new(12),
        ]
    );
    assert_eq!(
        layout.alpha_channels(),
        &[
            ChannelIndex::new(0),
            ChannelIndex::new(1),
            ChannelIndex::new(5),
            ChannelIndex::new(6),
        ]
    );
}

#[test]
fn ordinary_premultiplication_changes_only_recognized_color_channels() {
    let channels = ChannelList::from_full_names(["R", "G", "B", "A", "Z", "id"]).unwrap();
    let transform =
        AlphaTransform::new(&channels, AlphaMode::Straight, AlphaMode::Premultiplied).unwrap();
    let mut samples = [2.0, -4.0, 1.5, 0.25, 90.0, 42.0];

    transform.transform_pixel(&mut samples).unwrap();

    assert_eq!(samples, [0.5, -1.0, 0.375, 0.25, 90.0, 42.0]);
    assert_eq!(transform.source_mode(), AlphaMode::Straight);
    assert_eq!(transform.destination_mode(), AlphaMode::Premultiplied);
    assert_eq!(transform.rule(), PremultiplicationRule::OneTime);
}

#[test]
fn zero_alpha_rules_preserve_emission_only_for_temporary_round_trips() {
    let channels = ChannelList::from_full_names(["R", "G", "B", "A"]).unwrap();
    let unpremultiply =
        AlphaTransform::new(&channels, AlphaMode::Premultiplied, AlphaMode::Straight).unwrap();
    let repremultiply = AlphaTransform::with_rule(
        &channels,
        AlphaMode::Straight,
        AlphaMode::Premultiplied,
        PremultiplicationRule::PreserveZeroAlpha,
    )
    .unwrap();
    let premultiply =
        AlphaTransform::new(&channels, AlphaMode::Straight, AlphaMode::Premultiplied).unwrap();
    let original = [4.0, 0.5, -2.0, 0.0];

    let mut temporary = original;
    unpremultiply.transform_pixel(&mut temporary).unwrap();
    assert_eq!(temporary, original);
    repremultiply.transform_pixel(&mut temporary).unwrap();
    assert_eq!(temporary, original);

    let mut one_time = original;
    premultiply.transform_pixel(&mut one_time).unwrap();
    assert_eq!(one_time, [0.0, 0.0, -0.0, 0.0]);
}

#[test]
fn full_pixel_buffers_retain_order_count_and_independent_component_alpha() {
    let channels = ChannelList::from_full_names(["AR", "AG", "AB", "R", "G", "B", "A"]).unwrap();
    let transform =
        AlphaTransform::new(&channels, AlphaMode::Straight, AlphaMode::Premultiplied).unwrap();
    let mut samples = vec![
        0.25, 0.5, 0.75, 8.0, 6.0, 4.0, 0.125, 1.0, 0.0, 0.5, -2.0, 3.0, 10.0, 0.75,
    ];

    transform.transform_pixels(&mut samples).unwrap();

    assert_eq!(
        samples,
        vec![0.25, 0.5, 0.75, 2.0, 3.0, 3.0, 0.125, 1.0, 0.0, 0.5, -2.0, 0.0, 5.0, 0.75,]
    );
}

#[test]
fn opaque_conversion_is_explicit_and_does_not_unassociate_color() {
    let channels = ChannelList::from_full_names(["R", "G", "B", "A"]).unwrap();
    let to_opaque =
        AlphaTransform::new(&channels, AlphaMode::Premultiplied, AlphaMode::Opaque).unwrap();
    let from_opaque =
        AlphaTransform::new(&channels, AlphaMode::Opaque, AlphaMode::Premultiplied).unwrap();
    let mut pixel = [0.2, 0.1, -0.5, 0.25];

    to_opaque.transform_pixel(&mut pixel).unwrap();
    assert_eq!(pixel, [0.2, 0.1, -0.5, 1.0]);

    pixel[3] = 0.125;
    from_opaque.transform_pixel(&mut pixel).unwrap();
    assert_eq!(pixel, [0.2, 0.1, -0.5, 1.0]);
}

#[test]
fn malformed_buffers_and_missing_alpha_fail_actionably() {
    let rgba = ChannelList::from_full_names(["R", "G", "B", "A"]).unwrap();
    let transform =
        AlphaTransform::new(&rgba, AlphaMode::Straight, AlphaMode::Premultiplied).unwrap();
    let mut malformed = [0.0, 0.0, 0.0];
    let error = transform.transform_pixels(&mut malformed).unwrap_err();

    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(error.contexts()[0].component(), "superi-image.alpha");

    let rgb = ChannelList::from_full_names(["R", "G", "B"]).unwrap();
    let error =
        AlphaTransform::new(&rgb, AlphaMode::Straight, AlphaMode::Premultiplied).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(error.contexts()[0].component(), "superi-image.alpha");

    let auxiliary = ChannelList::from_full_names(["Z", "id"]).unwrap();
    AlphaTransform::new(&auxiliary, AlphaMode::Straight, AlphaMode::Premultiplied).unwrap();
}

#[test]
fn dense_half_float_images_preserve_identity_metadata_precision_and_extent() {
    let data_window = PixelBounds::from_origin_size(-3, 7, 2, 1).unwrap();
    let display_window = PixelBounds::from_origin_size(-10, -5, 30, 20).unwrap();
    let channels =
        ChannelList::from_full_names(["beauty.R", "beauty.G", "beauty.B", "beauty.A"]).unwrap();
    let descriptor = ImageDescriptor::new(
        data_window,
        display_window,
        PixelFormat::Rgba16Float,
        ColorSpace::ACESCG,
        AlphaMode::Straight,
    )
    .unwrap()
    .with_channels(channels.clone())
    .unwrap();
    let image = Image::new(
        descriptor,
        ImageSamples::f16_from_f32([2.0, -4.0, 0.5, 0.25, 8.0, 1.0, -2.0, 0.0]),
    )
    .unwrap()
    .with_metadata("source.camera", ImageMetadataValue::Text("A".to_owned()))
    .unwrap();
    let transform =
        AlphaTransform::new(&channels, AlphaMode::Straight, AlphaMode::Premultiplied).unwrap();

    let result = transform.transform_image(&image).unwrap();

    assert_eq!(image.descriptor().alpha_mode(), AlphaMode::Straight);
    assert_eq!(result.descriptor().alpha_mode(), AlphaMode::Premultiplied);
    assert_eq!(result.descriptor().data_window(), data_window);
    assert_eq!(result.descriptor().display_window(), display_window);
    assert_eq!(result.descriptor().pixel_format(), PixelFormat::Rgba16Float);
    assert_eq!(result.descriptor().color_space(), ColorSpace::ACESCG);
    assert_eq!(result.descriptor().channels(), &channels);
    assert_eq!(result.samples().sample_type(), ImageSampleType::F16);
    assert_eq!(result.metadata(), image.metadata());
    assert_eq!(
        (0..result.samples().len())
            .map(|index| result.samples().float_value(index).unwrap())
            .collect::<Vec<_>>(),
        [0.5, -1.0, 0.125, 0.25, 0.0, 0.0, -0.0, 0.0]
    );
}

#[test]
fn dense_integer_images_use_deterministic_normalized_rounding() {
    let bounds = PixelBounds::from_origin_size(0, 0, 1, 1).unwrap();
    let channels = ChannelList::from_full_names(["R", "G", "B", "A"]).unwrap();

    let image_u8 = Image::new(
        ImageDescriptor::new(
            bounds,
            bounds,
            PixelFormat::Rgba8Unorm,
            ColorSpace::SRGB,
            AlphaMode::Straight,
        )
        .unwrap(),
        ImageSamples::from_u8([255, 128, 64, 128]),
    )
    .unwrap();
    let premultiply =
        AlphaTransform::new(&channels, AlphaMode::Straight, AlphaMode::Premultiplied).unwrap();
    let premultiplied_u8 = premultiply.transform_image(&image_u8).unwrap();
    assert_eq!(
        premultiplied_u8.samples().u8_values(),
        Some(&[128, 64, 32, 128][..])
    );

    let unpremultiply =
        AlphaTransform::new(&channels, AlphaMode::Premultiplied, AlphaMode::Straight).unwrap();
    let restored_u8 = unpremultiply.transform_image(&premultiplied_u8).unwrap();
    assert_eq!(
        restored_u8.samples().u8_values(),
        Some(&[255, 128, 64, 128][..])
    );

    let image_u16 = Image::new(
        ImageDescriptor::new(
            bounds,
            bounds,
            PixelFormat::Rgba16Unorm,
            ColorSpace::ACESCG,
            AlphaMode::Straight,
        )
        .unwrap(),
        ImageSamples::from_u16([u16::MAX, 32_768, 1, 32_768]),
    )
    .unwrap();
    let result_u16 = premultiply.transform_image(&image_u16).unwrap();
    assert_eq!(
        result_u16.samples().u16_values(),
        Some(&[32_768, 16_384, 1, 32_768][..])
    );
}

#[test]
fn floating_bit_transforms_leave_alpha_and_auxiliary_payloads_exact() {
    let channels = ChannelList::from_full_names(["R", "A", "Z", "id"]).unwrap();
    let transform =
        AlphaTransform::new(&channels, AlphaMode::Straight, AlphaMode::Premultiplied).unwrap();
    let mut f16_bits = [0x4000, 0x3800, 0x7e11, 0x8000];
    let mut f32_bits = [
        4.0_f32.to_bits(),
        0.25_f32.to_bits(),
        0x7fc0_1234,
        (-0.0_f32).to_bits(),
    ];

    transform.transform_f16_bits(&mut f16_bits).unwrap();
    transform.transform_f32_bits(&mut f32_bits).unwrap();

    assert_eq!(f16_bits, [0x3c00, 0x3800, 0x7e11, 0x8000]);
    assert_eq!(
        f32_bits,
        [
            1.0_f32.to_bits(),
            0.25_f32.to_bits(),
            0x7fc0_1234,
            (-0.0_f32).to_bits(),
        ]
    );
}

#[test]
fn dense_image_transform_rejects_mismatched_semantics() {
    let bounds = PixelBounds::from_origin_size(0, 0, 1, 1).unwrap();
    let channels = ChannelList::from_full_names(["R", "G", "B", "A"]).unwrap();
    let transform =
        AlphaTransform::new(&channels, AlphaMode::Straight, AlphaMode::Premultiplied).unwrap();
    let already_premultiplied = Image::new(
        ImageDescriptor::new(
            bounds,
            bounds,
            PixelFormat::Rgba32Float,
            ColorSpace::ACESCG,
            AlphaMode::Premultiplied,
        )
        .unwrap(),
        ImageSamples::from_f32([0.0, 0.0, 0.0, 0.0]),
    )
    .unwrap();

    let error = transform
        .transform_image(&already_premultiplied)
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(error.contexts()[0].component(), "superi-image.alpha");
}

#[test]
fn alpha_contracts_are_safe_to_share_between_engine_owners() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<AlphaLayout>();
    assert_send_sync::<AlphaTransform>();
    assert_send_sync::<PremultiplicationRule>();
}
