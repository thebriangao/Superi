use std::sync::Arc;

use superi_color::gamut::{
    ChromaticAdaptation, GamutMapping, LinearRgb, RgbColorimetry, WideGamutTransform,
};
use superi_color::working_space::{WorkingImageF32, WorkingSpace};
use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::geometry::PixelBounds;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_image::metadata::{ImageColorTags, ImageMetadata, ImageMetadataValue};
use superi_image::value::{Image, ImageDescriptor, ImageSamples};

const MATRIX_TOLERANCE: f64 = 8.0e-10;

#[test]
fn built_in_colorimetry_matches_published_wide_gamut_definitions() {
    let bt2020 = RgbColorimetry::from_primaries(ColorPrimaries::Bt2020).unwrap();
    assert_xy(bt2020.red(), [0.708, 0.292]);
    assert_xy(bt2020.green(), [0.170, 0.797]);
    assert_xy(bt2020.blue(), [0.131, 0.046]);
    assert_xy(bt2020.white(), [0.3127, 0.3290]);

    let display_p3 = RgbColorimetry::from_primaries(ColorPrimaries::DisplayP3).unwrap();
    assert_xy(display_p3.red(), [0.680, 0.320]);
    assert_xy(display_p3.green(), [0.265, 0.690]);
    assert_xy(display_p3.blue(), [0.150, 0.060]);
    assert_xy(display_p3.white(), [0.3127, 0.3290]);

    let ap0 = RgbColorimetry::from_primaries(ColorPrimaries::AcesAp0).unwrap();
    assert_xy(ap0.red(), [0.73470, 0.26530]);
    assert_xy(ap0.green(), [0.0, 1.0]);
    assert_xy(ap0.blue(), [0.00010, -0.07700]);
    assert_xy(ap0.white(), [0.32168, 0.33767]);

    let ap1 = RgbColorimetry::from_primaries(ColorPrimaries::AcesAp1).unwrap();
    assert_xy(ap1.red(), [0.713, 0.293]);
    assert_xy(ap1.green(), [0.165, 0.830]);
    assert_xy(ap1.blue(), [0.128, 0.044]);
    assert_xy(ap1.white(), [0.32168, 0.33767]);
}

#[test]
fn ap0_to_ap1_matrix_matches_the_academy_reference() {
    let transform = WideGamutTransform::new(
        ColorPrimaries::AcesAp0,
        ColorPrimaries::AcesAp1,
        ChromaticAdaptation::Bradford,
        GamutMapping::Preserve,
    )
    .unwrap();
    let expected = [
        [
            1.451_439_316_145_67,
            -0.236_510_746_893_740,
            -0.214_928_569_251_925,
        ],
        [
            -0.076_553_773_396_020_4,
            1.176_229_699_833_57,
            -0.099_675_926_437_552_2,
        ],
        [
            0.008_316_148_425_697_72,
            -0.006_032_449_791_021_03,
            0.997_716_301_365_324,
        ],
    ];
    assert_matrix_close(transform.matrix(), expected, MATRIX_TOLERANCE);
}

#[test]
fn bradford_adaptation_keeps_neutral_values_neutral_across_d60_and_d65() {
    let adapted = WideGamutTransform::new(
        ColorPrimaries::AcesAp1,
        ColorPrimaries::Bt2020,
        ChromaticAdaptation::Bradford,
        GamutMapping::Preserve,
    )
    .unwrap();
    assert_rgb_close(
        adapted.apply_rgb(rgb([1.0, 1.0, 1.0])).unwrap().values(),
        [1.0, 1.0, 1.0],
        2.0e-12,
    );

    let unadapted = WideGamutTransform::new(
        ColorPrimaries::AcesAp1,
        ColorPrimaries::Bt2020,
        ChromaticAdaptation::None,
        GamutMapping::Preserve,
    )
    .unwrap();
    let unadapted_white = unadapted.apply_rgb(rgb([1.0, 1.0, 1.0])).unwrap().values();
    assert!(
        unadapted_white
            .into_iter()
            .any(|component| (component - 1.0).abs() > 1.0e-3),
        "different white points must not be silently adapted when adaptation is disabled"
    );
}

#[test]
fn primary_conversions_round_trip_scene_linear_negative_and_hdr_values() {
    for destination in [
        ColorPrimaries::Bt2020,
        ColorPrimaries::DisplayP3,
        ColorPrimaries::AcesAp0,
    ] {
        let forward = WideGamutTransform::new(
            ColorPrimaries::AcesAp1,
            destination,
            ChromaticAdaptation::Bradford,
            GamutMapping::Preserve,
        )
        .unwrap();
        let reverse = WideGamutTransform::new(
            destination,
            ColorPrimaries::AcesAp1,
            ChromaticAdaptation::Bradford,
            GamutMapping::Preserve,
        )
        .unwrap();
        for original in [[-0.25, 0.5, 4.0], [0.0, 0.18, 1.0], [8.0, 2.0, -0.01]] {
            let converted = forward.apply_rgb(rgb(original)).unwrap();
            let restored = reverse.apply_rgb(converted).unwrap();
            assert_rgb_close(restored.values(), original, 3.0e-12);
        }
    }
}

#[test]
fn gamut_policy_is_explicit_and_preserves_hdr_headroom() {
    let original = [-0.25, 0.5, 2.0];
    let preserve = identity(GamutMapping::Preserve);
    assert_eq!(
        preserve.apply_rgb(rgb(original)).unwrap().values(),
        original
    );

    let clipped = identity(GamutMapping::ClipNegative)
        .apply_rgb(rgb(original))
        .unwrap()
        .values();
    assert_eq!(clipped, [0.0, 0.5, 2.0]);

    let compressed_transform = identity(GamutMapping::PreserveLuminance);
    let compressed = compressed_transform
        .apply_rgb(rgb(original))
        .unwrap()
        .values();
    assert_close(compressed[0], 0.0, 2.0e-14);
    assert!(
        compressed[2] > 1.0,
        "gamut mapping must not tone map HDR headroom"
    );
    assert_close(
        dot(compressed_transform.destination_luma(), compressed),
        dot(compressed_transform.destination_luma(), original),
        2.0e-13,
    );
}

#[test]
fn premultiplied_alpha_is_unassociated_for_nonlinear_mapping_and_preserved() {
    let transform = identity(GamutMapping::PreserveLuminance);
    let alpha = 0.25_f64;
    let straight = [-0.25, 0.5, 2.0];
    let premultiplied = [
        straight[0] * alpha,
        straight[1] * alpha,
        straight[2] * alpha,
        alpha,
    ];
    let transformed = transform.apply_premultiplied_rgba(premultiplied).unwrap();
    let expected = transform.apply_rgb(rgb(straight)).unwrap().values();
    assert_rgb_close(
        [
            transformed[0] / alpha,
            transformed[1] / alpha,
            transformed[2] / alpha,
        ],
        expected,
        2.0e-13,
    );
    assert_eq!(transformed[3].to_bits(), alpha.to_bits());
    assert_eq!(
        transform
            .apply_premultiplied_rgba([0.0, 0.0, 0.0, 0.0])
            .unwrap(),
        [0.0; 4]
    );
    assert_invalid(transform.apply_premultiplied_rgba([0.01, 0.0, 0.0, 0.0]));
}

#[test]
fn working_images_retain_artifact_identity_and_change_only_processing_interpretation() {
    let source_space = linear_space(ColorPrimaries::AcesAp0);
    let destination_space = WorkingSpace::ACESCG;
    let source_working = WorkingSpace::new(source_space).unwrap();
    let bounds = PixelBounds::from_origin_size(-3, 7, 1, 1).unwrap();
    let tags = ImageColorTags::new(source_space)
        .with_named_space("ACES - ACES2065-1")
        .unwrap()
        .with_icc_profile(Arc::from([1_u8, 3, 7, 9]))
        .unwrap();
    let mut metadata = ImageMetadata::new();
    metadata
        .insert(
            "source.camera",
            ImageMetadataValue::Text("camera-a".to_owned()),
        )
        .unwrap();
    let descriptor = ImageDescriptor::new_with_color_tags(
        bounds,
        bounds,
        PixelFormat::Rgba32Float,
        tags,
        AlphaMode::Premultiplied,
    )
    .unwrap();
    let image = Image::new_with_metadata(
        descriptor,
        ImageSamples::from_f32([-0.125, 0.25, 1.0, 0.5]),
        metadata.clone(),
    )
    .unwrap();
    let source = WorkingImageF32::new(source_working, image).unwrap();
    let transform = WideGamutTransform::new(
        ColorPrimaries::AcesAp0,
        ColorPrimaries::AcesAp1,
        ChromaticAdaptation::Bradford,
        GamutMapping::Preserve,
    )
    .unwrap();

    let output = transform.apply_working_f32(&source).unwrap();
    assert_eq!(output.space(), destination_space);
    assert_eq!(output.image().descriptor().data_window(), bounds);
    assert_eq!(output.image().descriptor().display_window(), bounds);
    assert_eq!(
        output.image().descriptor().color_space(),
        ColorSpace::ACESCG
    );
    assert_eq!(
        output.image().descriptor().color_tags().named_space(),
        Some("ACES - ACES2065-1")
    );
    assert_eq!(
        output.image().descriptor().color_tags().icc_profile(),
        Some([1_u8, 3, 7, 9].as_slice())
    );
    assert_eq!(output.image().metadata(), &metadata);
    assert_eq!(
        output.image().samples().float_value(3).unwrap().to_bits(),
        0.5_f32.to_bits()
    );

    let f16 = source.quantize_f16().unwrap();
    let transformed_f16 = transform.apply_working_f16(&f16).unwrap();
    assert_eq!(transformed_f16.space(), destination_space);
    assert_eq!(
        transformed_f16.image().descriptor().pixel_format(),
        PixelFormat::Rgba16Float
    );
    assert_eq!(
        transformed_f16
            .image()
            .samples()
            .float_value(3)
            .unwrap()
            .to_bits(),
        f16.image().samples().float_value(3).unwrap().to_bits()
    );
}

#[test]
fn ambiguous_or_nonfinite_inputs_fail_with_shared_context() {
    assert_invalid(WideGamutTransform::new(
        ColorPrimaries::Unspecified,
        ColorPrimaries::AcesAp1,
        ChromaticAdaptation::Bradford,
        GamutMapping::Preserve,
    ));
    assert_invalid(LinearRgb::new([f64::NAN, 0.0, 0.0]));
    assert_invalid(LinearRgb::new([0.0, f64::INFINITY, 0.0]));

    let transform = identity(GamutMapping::PreserveLuminance);
    assert_invalid(transform.apply_rgb(rgb([-1.0, -0.5, -0.25])));

    let wrong_source = WorkingImageF32::new(
        WorkingSpace::ACESCG,
        image(PixelFormat::Rgba32Float, ColorSpace::ACESCG),
    )
    .unwrap();
    let ap0_to_ap1 = WideGamutTransform::new(
        ColorPrimaries::AcesAp0,
        ColorPrimaries::AcesAp1,
        ChromaticAdaptation::Bradford,
        GamutMapping::Preserve,
    )
    .unwrap();
    assert_invalid(ap0_to_ap1.apply_working_f32(&wrong_source));
}

#[test]
fn gamut_contracts_are_safe_to_share_between_engine_owners() {
    fn assert_send_sync_copy<T: Send + Sync + Copy>() {}
    assert_send_sync_copy::<LinearRgb>();
    assert_send_sync_copy::<RgbColorimetry>();
    assert_send_sync_copy::<ChromaticAdaptation>();
    assert_send_sync_copy::<GamutMapping>();
    assert_send_sync_copy::<WideGamutTransform>();
}

fn identity(mapping: GamutMapping) -> WideGamutTransform {
    WideGamutTransform::new(
        ColorPrimaries::AcesAp1,
        ColorPrimaries::AcesAp1,
        ChromaticAdaptation::Bradford,
        mapping,
    )
    .unwrap()
}

fn linear_space(primaries: ColorPrimaries) -> ColorSpace {
    ColorSpace::new(
        primaries,
        TransferFunction::Linear,
        MatrixCoefficients::Rgb,
        ColorRange::Full,
    )
}

fn image(pixel_format: PixelFormat, color_space: ColorSpace) -> Image {
    let bounds = PixelBounds::from_origin_size(0, 0, 1, 1).unwrap();
    let descriptor = ImageDescriptor::new(
        bounds,
        bounds,
        pixel_format,
        color_space,
        AlphaMode::Premultiplied,
    )
    .unwrap();
    let samples = match pixel_format {
        PixelFormat::Rgba16Float => ImageSamples::from_f16_bits([0, 0, 0, 0]),
        PixelFormat::Rgba32Float => ImageSamples::from_f32([0.0, 0.0, 0.0, 0.0]),
        _ => unreachable!("test helper accepts only RGBA float images"),
    };
    Image::new(descriptor, samples).unwrap()
}

fn rgb(values: [f64; 3]) -> LinearRgb {
    LinearRgb::new(values).unwrap()
}

fn assert_xy(actual: superi_color::gamut::Chromaticity, expected: [f64; 2]) {
    assert_close(actual.x(), expected[0], 0.0);
    assert_close(actual.y(), expected[1], 0.0);
}

fn assert_matrix_close(actual: [[f64; 3]; 3], expected: [[f64; 3]; 3], tolerance: f64) {
    for (actual_row, expected_row) in actual.into_iter().zip(expected) {
        assert_rgb_close(actual_row, expected_row, tolerance);
    }
}

fn assert_rgb_close(actual: [f64; 3], expected: [f64; 3], tolerance: f64) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert_close(actual, expected, tolerance);
    }
}

fn assert_close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected:.17}, got {actual:.17}, tolerance {tolerance:.3e}"
    );
}

fn assert_invalid<T>(result: superi_core::error::Result<T>) {
    let error = result.err().expect("operation must reject invalid input");
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert!(!error.contexts().is_empty());
}

fn dot(left: [f64; 3], right: [f64; 3]) -> f64 {
    left[0] * right[0] + left[1] * right[1] + left[2] * right[2]
}
