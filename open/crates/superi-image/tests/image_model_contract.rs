use std::sync::Arc;

use superi_core::color_space::ColorSpace;
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::geometry::PixelBounds;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_image::channels::{ChannelList, ChannelName, StandardChannel};
use superi_image::metadata::{ImageMetadataFloat, ImageMetadataValue};
use superi_image::value::{Image, ImageDescriptor, ImageSampleType, ImageSamples};

#[test]
fn half_float_images_preserve_hdr_values_and_complete_semantics() {
    let data_window = PixelBounds::from_origin_size(-2, 5, 1, 1).unwrap();
    let display_window = PixelBounds::from_origin_size(-10, -4, 20, 12).unwrap();
    let descriptor = ImageDescriptor::new(
        data_window,
        display_window,
        PixelFormat::Rgba16Float,
        ColorSpace::ACESCG,
        AlphaMode::Premultiplied,
    )
    .unwrap();
    let bits = [0xc000, 0x4400, 0x0001, 0x3c00];
    let image = Image::new(descriptor, ImageSamples::from_f16_bits(bits)).unwrap();

    assert_eq!(image.descriptor().data_window(), data_window);
    assert_eq!(image.descriptor().display_window(), display_window);
    assert_eq!(image.descriptor().pixel_format(), PixelFormat::Rgba16Float);
    assert_eq!(image.descriptor().color_space(), ColorSpace::ACESCG);
    assert_eq!(image.descriptor().alpha_mode(), AlphaMode::Premultiplied);
    assert_eq!(
        image
            .descriptor()
            .channels()
            .iter()
            .map(ChannelName::standard)
            .collect::<Vec<_>>(),
        &[
            Some(StandardChannel::Red),
            Some(StandardChannel::Green),
            Some(StandardChannel::Blue),
            Some(StandardChannel::Alpha),
        ]
    );
    assert_eq!(image.samples().sample_type(), ImageSampleType::F16);
    assert_eq!(image.samples().f16_bits(), Some(bits.as_slice()));
    assert_eq!(image.samples().float_value(0), Some(-2.0));
    assert_eq!(image.samples().float_value(1), Some(4.0));
    assert!(image.samples().float_value(2).unwrap() > 0.0);
    assert_eq!(image.samples().float_value(3), Some(1.0));
}

#[test]
fn full_float_images_retain_every_ieee_payload_bit() {
    let descriptor = rgba_descriptor(PixelFormat::Rgba32Float, AlphaMode::Straight);
    let bits = [
        (-0.0_f32).to_bits(),
        12_345.5_f32.to_bits(),
        f32::INFINITY.to_bits(),
        0x7fc0_1234,
    ];
    let image = Image::new(descriptor, ImageSamples::from_f32_bits(bits)).unwrap();

    assert_eq!(image.samples().f32_bits(), Some(bits.as_slice()));
    assert_eq!(image.samples().float_value(0).unwrap().to_bits(), bits[0]);
    assert_eq!(image.samples().float_value(1), Some(12_345.5));
    assert_eq!(image.samples().float_value(2), Some(f32::INFINITY));
    assert_eq!(image.samples().float_value(3).unwrap().to_bits(), bits[3]);
}

#[test]
fn high_bit_depth_integer_samples_remain_exact() {
    let descriptor = rgba_descriptor(PixelFormat::Rgba16Unorm, AlphaMode::Straight);
    let samples = [0, 1, 32_768, u16::MAX];
    let image = Image::new(descriptor, ImageSamples::from_u16(samples)).unwrap();

    assert_eq!(image.samples().sample_type(), ImageSampleType::U16);
    assert_eq!(image.samples().u16_values(), Some(samples.as_slice()));
    assert_eq!(image.samples().float_value(2), None);
}

#[test]
fn metadata_and_image_identity_survive_ordinary_edits() {
    let channels = ChannelList::from_full_names([
        "beauty.diffuse.R",
        "beauty.diffuse.G",
        "beauty.diffuse.B",
        "beauty.diffuse.A",
    ])
    .unwrap();
    let image = Image::new(
        rgba_descriptor(PixelFormat::Rgba16Float, AlphaMode::Straight)
            .with_channels(channels)
            .unwrap(),
        ImageSamples::from_f16_bits([0, 0, 0, 0x3c00]),
    )
    .unwrap()
    .with_metadata(
        "source.camera",
        ImageMetadataValue::Text("A camera".to_owned()),
    )
    .unwrap()
    .with_metadata(
        "source.exposure_ev",
        ImageMetadataValue::Float(ImageMetadataFloat::new(-1.25)),
    )
    .unwrap()
    .with_metadata(
        "source.private_blob",
        ImageMetadataValue::Bytes(Arc::from([0_u8, 255, 17])),
    )
    .unwrap();

    let edited = image
        .clone()
        .replace_samples(ImageSamples::from_f16_bits([
            0x4000, 0x4000, 0x4000, 0x3c00,
        ]))
        .unwrap();

    assert_eq!(edited.descriptor(), image.descriptor());
    assert_eq!(
        edited
            .descriptor()
            .channels()
            .iter()
            .map(ChannelName::as_str)
            .collect::<Vec<_>>(),
        [
            "beauty.diffuse.R",
            "beauty.diffuse.G",
            "beauty.diffuse.B",
            "beauty.diffuse.A",
        ]
    );
    assert_eq!(edited.metadata(), image.metadata());
    assert_eq!(
        edited.metadata().get("source.camera"),
        Some(&ImageMetadataValue::Text("A camera".to_owned()))
    );
    assert_eq!(
        edited
            .metadata()
            .get("source.exposure_ev")
            .and_then(ImageMetadataValue::as_f64),
        Some(-1.25)
    );
    assert_eq!(
        edited
            .metadata()
            .iter()
            .map(|(key, _)| key)
            .collect::<Vec<_>>(),
        ["source.camera", "source.exposure_ev", "source.private_blob"]
    );
    assert_ne!(edited.samples(), image.samples());
}

#[test]
fn packed_channel_order_is_explicit_for_every_supported_model() {
    let cases = [
        (PixelFormat::R16Float, &["Y"][..]),
        (PixelFormat::Rg16Float, &["R", "G"][..]),
        (PixelFormat::Rgb8Unorm, &["R", "G", "B"][..]),
        (PixelFormat::Bgr8Unorm, &["B", "G", "R"][..]),
        (PixelFormat::Bgra8Unorm, &["B", "G", "R", "A"][..]),
    ];

    for (format, channels) in cases {
        let alpha = if format.has_alpha() {
            AlphaMode::Straight
        } else {
            AlphaMode::Opaque
        };
        let descriptor = ImageDescriptor::new(
            PixelBounds::from_origin_size(0, 0, 1, 1).unwrap(),
            PixelBounds::from_origin_size(0, 0, 1, 1).unwrap(),
            format,
            ColorSpace::ACESCG,
            alpha,
        )
        .unwrap();
        assert_eq!(
            descriptor
                .channels()
                .iter()
                .map(ChannelName::as_str)
                .collect::<Vec<_>>(),
            channels
        );
    }
}

#[test]
fn invalid_or_not_yet_supported_representations_fail_actionably() {
    let bounds = PixelBounds::from_origin_size(0, 0, 1, 1).unwrap();
    let empty = PixelBounds::from_origin_size(0, 0, 0, 1).unwrap();

    let empty_error = ImageDescriptor::new(
        empty,
        bounds,
        PixelFormat::Rgba16Float,
        ColorSpace::ACESCG,
        AlphaMode::Straight,
    )
    .unwrap_err();
    assert_image_error(empty_error, ErrorCategory::InvalidInput);

    let alpha_error = ImageDescriptor::new(
        bounds,
        bounds,
        PixelFormat::Rgb8Unorm,
        ColorSpace::SRGB,
        AlphaMode::Straight,
    )
    .unwrap_err();
    assert_image_error(alpha_error, ErrorCategory::InvalidInput);

    let planar_error = ImageDescriptor::new(
        bounds,
        bounds,
        PixelFormat::Yuv420p10,
        ColorSpace::BT2020,
        AlphaMode::Opaque,
    )
    .unwrap_err();
    assert_image_error(planar_error, ErrorCategory::Unsupported);

    let channel_error = rgba_descriptor(PixelFormat::Rgba16Float, AlphaMode::Straight)
        .with_channels(ChannelList::from_full_names(["R", "G", "B"]).unwrap())
        .unwrap_err();
    assert_image_error(channel_error, ErrorCategory::InvalidInput);

    let descriptor = rgba_descriptor(PixelFormat::Rgba16Float, AlphaMode::Straight);
    let type_error =
        Image::new(descriptor.clone(), ImageSamples::from_u16([0, 0, 0, 0])).unwrap_err();
    assert_image_error(type_error, ErrorCategory::InvalidInput);

    let count_error =
        Image::new(descriptor.clone(), ImageSamples::from_f16_bits([0, 0, 0])).unwrap_err();
    assert_image_error(count_error, ErrorCategory::InvalidInput);

    let huge = PixelBounds::new(i32::MIN, i32::MIN, i32::MAX, i32::MAX).unwrap();
    let huge_descriptor = ImageDescriptor::new(
        huge,
        huge,
        PixelFormat::Rgba32Float,
        ColorSpace::ACESCG,
        AlphaMode::Straight,
    )
    .unwrap();
    let overflow_error = Image::new(huge_descriptor, ImageSamples::from_f32_bits([])).unwrap_err();
    assert_image_error(overflow_error, ErrorCategory::ResourceExhausted);

    let metadata_error = Image::new(descriptor, ImageSamples::from_f16_bits([0, 0, 0, 0]))
        .unwrap()
        .with_metadata("contains\0nul", ImageMetadataValue::Unsigned(1))
        .unwrap_err();
    assert_eq!(metadata_error.category(), ErrorCategory::InvalidInput);
    assert_eq!(
        metadata_error.recoverability(),
        Recoverability::UserCorrectable
    );
    assert_eq!(
        metadata_error.contexts()[0].component(),
        "superi-image.metadata"
    );
}

#[test]
fn image_contracts_are_safe_to_share_between_engine_owners() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<Image>();
    assert_send_sync::<ImageDescriptor>();
    assert_send_sync::<ImageSamples>();
    assert_send_sync::<ImageMetadataValue>();
}

fn rgba_descriptor(pixel_format: PixelFormat, alpha_mode: AlphaMode) -> ImageDescriptor {
    ImageDescriptor::new(
        PixelBounds::from_origin_size(0, 0, 1, 1).unwrap(),
        PixelBounds::from_origin_size(0, 0, 1, 1).unwrap(),
        pixel_format,
        ColorSpace::ACESCG,
        alpha_mode,
    )
    .unwrap()
}

fn assert_image_error(error: superi_core::error::Error, category: ErrorCategory) {
    assert_eq!(error.category(), category);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(error.contexts()[0].component(), "superi-image.value");
}
