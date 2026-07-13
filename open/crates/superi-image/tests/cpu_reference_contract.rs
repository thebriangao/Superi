use std::sync::Arc;

use half::f16;
use superi_core::color_space::ColorSpace;
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::geometry::{Matrix3, PixelBounds, Vector2};
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_image::alpha::PremultiplicationRule;
use superi_image::channels::{ChannelIndex, ChannelList};
use superi_image::metadata::{ImageColorTags, ImageMetadata, ImageMetadataValue, ImageOrientation};
use superi_image::ops::{ChannelSource, FlipAxis, QuarterTurn, ResampleFilter};
use superi_image::reference::{
    compare_images, BinaryReferenceOperation, ReferenceComparison, ReferenceSample,
    ReferenceTolerance, SampleMismatch, UnaryReferenceOperation,
};
use superi_image::value::{Image, ImageDescriptor, ImageSamples};

#[test]
fn unary_reference_operations_are_repeatable_and_preserve_owned_semantics() {
    let source = tagged_rgba_f32(
        bounds(-1, 2, 2, 2),
        bounds(-3, -2, 8, 7),
        [
            1.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 0.5, 0.0, 0.0, 1.0, 0.25, 2.0, 1.0, 0.5, 0.0,
        ],
        AlphaMode::Straight,
        "unary-source",
    );
    let translation = Matrix3::translation(Vector2::new(4.0, -3.0).unwrap());
    let renamed =
        ChannelList::from_full_names(["beauty.R", "beauty.G", "beauty.B", "beauty.A"]).unwrap();

    let operations = [
        UnaryReferenceOperation::Alpha {
            destination_mode: AlphaMode::Premultiplied,
            rule: PremultiplicationRule::OneTime,
        },
        UnaryReferenceOperation::Crop {
            data_window: bounds(-2, 2, 3, 2),
        },
        UnaryReferenceOperation::Resize {
            data_window: bounds(5, 7, 3, 3),
            filter: ResampleFilter::Bilinear,
        },
        UnaryReferenceOperation::Transform {
            source_to_destination: translation,
            data_window: bounds(3, -1, 2, 2),
            filter: ResampleFilter::Nearest,
        },
        UnaryReferenceOperation::Flip(FlipAxis::Horizontal),
        UnaryReferenceOperation::Rotate(QuarterTurn::Clockwise90),
        UnaryReferenceOperation::RemapChannels {
            pixel_format: PixelFormat::Rgba32Float,
            alpha_mode: AlphaMode::Straight,
            channels: renamed.clone(),
            sources: vec![
                ChannelSource::Copy(ChannelIndex::new(2)),
                ChannelSource::Copy(ChannelIndex::new(1)),
                ChannelSource::Copy(ChannelIndex::new(0)),
                ChannelSource::Constant(1.0),
            ],
        },
        UnaryReferenceOperation::RenameChannels { channels: renamed },
    ];

    for operation in operations {
        let first = operation.execute(&source).unwrap();
        let second = operation.execute(&source).unwrap();
        assert_eq!(first, second, "{operation:?}");
        assert_eq!(first.metadata(), source.metadata(), "{operation:?}");
        assert_eq!(
            first.descriptor().color_tags(),
            source.descriptor().color_tags(),
            "{operation:?}"
        );
    }
}

#[test]
fn binary_reference_operations_are_repeatable_and_keep_composition_identity() {
    let first = tagged_rgba_f32(
        bounds(0, 0, 1, 1),
        bounds(0, 0, 2, 1),
        [1.0, 0.0, 0.0, 0.5],
        AlphaMode::Straight,
        "first",
    );
    let matching = tagged_rgba_f32(
        bounds(0, 0, 1, 1),
        bounds(0, 0, 2, 1),
        [0.0, 0.0, 1.0, 1.0],
        AlphaMode::Straight,
        "matching",
    );
    let background = tagged_rgba_f32(
        bounds(0, 0, 2, 1),
        bounds(0, 0, 2, 1),
        [0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 1.0],
        AlphaMode::Straight,
        "background",
    );

    let blend = BinaryReferenceOperation::Blend { amount: 0.25 };
    let blended = blend.execute(&first, &matching).unwrap();
    assert_eq!(blended, blend.execute(&first, &matching).unwrap());
    assert_eq!(blended.metadata(), first.metadata());
    assert_eq!(blended.descriptor(), first.descriptor());

    let composite = BinaryReferenceOperation::CompositeOver;
    let composited = composite.execute(&first, &background).unwrap();
    assert_eq!(composited, composite.execute(&first, &background).unwrap());
    assert_eq!(composited.metadata(), first.metadata());
    assert_eq!(composited.descriptor().data_window(), bounds(0, 0, 2, 1));
    assert_eq!(
        composited.descriptor().channels(),
        first.descriptor().channels()
    );
    assert_eq!(
        composited.descriptor().alpha_mode(),
        first.descriptor().alpha_mode()
    );
}

#[test]
fn comparison_requires_exact_integer_output_and_supports_explicit_float_tolerance() {
    let integer_reference = gray_u8([10, 20]);
    let integer_candidate = gray_u8([10, 21]);
    let integer = compare_images(
        &integer_reference,
        &integer_candidate,
        ReferenceTolerance::general_normalized(),
    )
    .unwrap();
    assert!(!integer.matches());
    assert_eq!(integer.compared_samples(), 2);
    assert_eq!(integer.mismatched_samples(), 1);
    assert_eq!(
        integer.first_mismatch().unwrap().expected(),
        ReferenceSample::U8(20)
    );
    assert_eq!(
        integer.first_mismatch().unwrap().actual(),
        ReferenceSample::U8(21)
    );

    let u16_reference = gray_u16([1024, u16::MAX]);
    let u16_candidate = gray_u16([1025, u16::MAX]);
    let u16_report = compare_images(
        &u16_reference,
        &u16_candidate,
        ReferenceTolerance::general_normalized(),
    )
    .unwrap();
    assert_eq!(u16_report.mismatched_samples(), 1);
    assert_eq!(
        u16_report.first_mismatch().unwrap().expected(),
        ReferenceSample::U16(1024)
    );

    let reference = tagged_rgba_f32(
        bounds(-2, 4, 1, 1),
        bounds(-2, 4, 1, 1),
        [0.25, 0.5, 1.0, 1.0],
        AlphaMode::Straight,
        "float",
    );
    let within = reference
        .clone()
        .replace_samples(ImageSamples::from_f32([0.2505, 0.5, 1.0, 1.0]))
        .unwrap();
    let outside = reference
        .clone()
        .replace_samples(ImageSamples::from_f32([0.252, 0.5, 1.0, 1.0]))
        .unwrap();

    assert!(
        !compare_images(&reference, &within, ReferenceTolerance::exact())
            .unwrap()
            .matches()
    );
    let within_report = compare_images(
        &reference,
        &within,
        ReferenceTolerance::general_normalized(),
    )
    .unwrap();
    assert!(within_report.matches());
    assert_eq!(within_report.mismatched_samples(), 0);
    assert!(within_report.maximum_absolute_error().unwrap() > 0.0);
    assert!(!compare_images(
        &reference,
        &outside,
        ReferenceTolerance::general_normalized(),
    )
    .unwrap()
    .matches());
}

#[test]
fn tolerant_float_comparison_handles_ieee_special_values_without_hiding_mismatches() {
    let descriptor = ImageDescriptor::new(
        bounds(0, 0, 4, 1),
        bounds(0, 0, 4, 1),
        PixelFormat::R32Float,
        ColorSpace::ACESCG,
        AlphaMode::Opaque,
    )
    .unwrap();
    let reference = Image::new(
        descriptor.clone(),
        ImageSamples::from_f32_bits([
            0x7fc0_0001,
            f32::INFINITY.to_bits(),
            0.0_f32.to_bits(),
            1.0_f32.to_bits(),
        ]),
    )
    .unwrap();
    let accepted = Image::new(
        descriptor.clone(),
        ImageSamples::from_f32_bits([
            0x7fc0_1234,
            f32::INFINITY.to_bits(),
            (-0.0_f32).to_bits(),
            1.0005_f32.to_bits(),
        ]),
    )
    .unwrap();
    let rejected = Image::new(
        descriptor,
        ImageSamples::from_f32_bits([
            0x7fc0_1234,
            f32::NEG_INFINITY.to_bits(),
            (-0.0_f32).to_bits(),
            1.0005_f32.to_bits(),
        ]),
    )
    .unwrap();

    assert!(
        !compare_images(&reference, &accepted, ReferenceTolerance::exact())
            .unwrap()
            .matches()
    );
    assert!(compare_images(
        &reference,
        &accepted,
        ReferenceTolerance::general_normalized(),
    )
    .unwrap()
    .matches());
    let report = compare_images(
        &reference,
        &rejected,
        ReferenceTolerance::general_normalized(),
    )
    .unwrap();
    assert!(!report.matches());
    assert_eq!(report.mismatched_samples(), 1);
    assert_eq!(report.first_mismatch().unwrap().sample_index(), 1);

    let half_descriptor = ImageDescriptor::new(
        bounds(0, 0, 1, 1),
        bounds(0, 0, 1, 1),
        PixelFormat::R16Float,
        ColorSpace::ACESCG,
        AlphaMode::Opaque,
    )
    .unwrap();
    let half_reference = Image::new(
        half_descriptor.clone(),
        ImageSamples::from_f16_bits([f16::from_f32(0.5).to_bits()]),
    )
    .unwrap();
    let half_candidate = Image::new(
        half_descriptor,
        ImageSamples::from_f16_bits([f16::from_f32(0.5005).to_bits()]),
    )
    .unwrap();
    assert!(!compare_images(
        &half_reference,
        &half_candidate,
        ReferenceTolerance::exact(),
    )
    .unwrap()
    .matches());
    assert!(compare_images(
        &half_reference,
        &half_candidate,
        ReferenceTolerance::general_normalized(),
    )
    .unwrap()
    .matches());
}

#[test]
fn comparison_reports_semantic_and_signed_pixel_channel_mismatches() {
    let reference = tagged_rgba_f32(
        bounds(-3, 7, 2, 1),
        bounds(-5, 4, 6, 5),
        [0.0, 0.0, 0.0, 1.0, 1.0, 0.5, 0.25, 1.0],
        AlphaMode::Straight,
        "reference",
    );
    let changed_sample = reference
        .clone()
        .replace_samples(ImageSamples::from_f32([
            0.0, 0.0, 0.0, 1.0, 1.0, 0.75, 0.25, 1.0,
        ]))
        .unwrap();
    let sample_report =
        compare_images(&reference, &changed_sample, ReferenceTolerance::exact()).unwrap();
    let mismatch = sample_report.first_mismatch().unwrap();
    assert_eq!(mismatch.sample_index(), 5);
    assert_eq!(mismatch.x(), -2);
    assert_eq!(mismatch.y(), 7);
    assert_eq!(mismatch.channel(), ChannelIndex::new(1));

    let changed_metadata = changed_sample
        .clone()
        .with_metadata("new.attribute", ImageMetadataValue::Unsigned(9))
        .unwrap();
    let metadata_report = compare_images(
        &changed_sample,
        &changed_metadata,
        ReferenceTolerance::exact(),
    )
    .unwrap();
    assert!(metadata_report.descriptor_matches());
    assert!(!metadata_report.metadata_matches());
    assert_eq!(metadata_report.mismatched_samples(), 0);
    assert!(!metadata_report.matches());

    let premultiplied = tagged_rgba_f32(
        bounds(-3, 7, 2, 1),
        bounds(-5, 4, 6, 5),
        [0.0, 0.0, 0.0, 1.0, 1.0, 0.5, 0.25, 1.0],
        AlphaMode::Premultiplied,
        "reference",
    );
    let descriptor_report =
        compare_images(&reference, &premultiplied, ReferenceTolerance::exact()).unwrap();
    assert!(!descriptor_report.descriptor_matches());
    assert!(!descriptor_report.matches());
    assert_eq!(descriptor_report.compared_samples(), 0);
}

#[test]
fn invalid_tolerances_and_failed_validation_are_actionable() {
    for invalid in [-1.0, f64::NAN, f64::INFINITY] {
        let error = ReferenceTolerance::absolute(invalid).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
        assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
        assert_eq!(error.contexts()[0].component(), "superi-image.reference");
    }

    let reference = gray_u8([1, 2]);
    let candidate = gray_u8([1, 3]);
    let report = compare_images(&reference, &candidate, ReferenceTolerance::exact()).unwrap();
    let error = report.require_match().unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Internal);
    assert_eq!(error.recoverability(), Recoverability::Degraded);
    assert_eq!(error.contexts()[0].component(), "superi-image.reference");
    assert_eq!(error.contexts()[0].field("mismatched_samples"), Some("1"));
}

#[test]
fn reference_contracts_are_safe_to_share_between_engine_owners() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<UnaryReferenceOperation>();
    assert_send_sync::<BinaryReferenceOperation>();
    assert_send_sync::<ReferenceTolerance>();
    assert_send_sync::<ReferenceSample>();
    assert_send_sync::<SampleMismatch>();
    assert_send_sync::<ReferenceComparison>();
}

fn tagged_rgba_f32(
    data_window: PixelBounds,
    display_window: PixelBounds,
    values: impl IntoIterator<Item = f32>,
    alpha_mode: AlphaMode,
    owner: &str,
) -> Image {
    Image::new_with_metadata(
        ImageDescriptor::new_with_color_tags(
            data_window,
            display_window,
            PixelFormat::Rgba32Float,
            tagged_color(),
            alpha_mode,
        )
        .unwrap(),
        ImageSamples::from_f32(values),
        tagged_metadata(owner),
    )
    .unwrap()
}

fn gray_u8(values: [u8; 2]) -> Image {
    Image::new(
        ImageDescriptor::new(
            bounds(0, 0, 2, 1),
            bounds(0, 0, 2, 1),
            PixelFormat::R8Unorm,
            ColorSpace::ACESCG,
            AlphaMode::Opaque,
        )
        .unwrap(),
        ImageSamples::from_u8(values),
    )
    .unwrap()
}

fn gray_u16(values: [u16; 2]) -> Image {
    Image::new(
        ImageDescriptor::new(
            bounds(0, 0, 2, 1),
            bounds(0, 0, 2, 1),
            PixelFormat::R16Unorm,
            ColorSpace::ACESCG,
            AlphaMode::Opaque,
        )
        .unwrap(),
        ImageSamples::from_u16(values),
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

fn tagged_metadata(owner: &str) -> ImageMetadata {
    let mut metadata = ImageMetadata::new().with_orientation(ImageOrientation::TopLeft);
    metadata
        .insert("test.owner", ImageMetadataValue::Text(owner.to_owned()))
        .unwrap();
    metadata
}

fn bounds(min_x: i32, min_y: i32, width: u32, height: u32) -> PixelBounds {
    PixelBounds::from_origin_size(min_x, min_y, width, height).unwrap()
}
