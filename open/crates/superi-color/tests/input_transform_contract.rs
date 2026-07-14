use superi_color::gamut::{ChromaticAdaptation, GamutMapping, LinearRgb, WideGamutTransform};
use superi_color::hdr::{pq_inverse_eotf, Nits};
use superi_color::transform_in::{InputColorTransform, InputSourceKind, InputTransformOptions};
use superi_color::working_space::WorkingSpace;
use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::geometry::PixelBounds;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_image::metadata::{ImageColorTags, ImageMetadata, ImageMetadataValue};
use superi_image::value::{Image, ImageDescriptor, ImageSamples};

const COMPONENT_TOLERANCE: f64 = 8.0e-4;

#[test]
fn display_referred_input_decodes_before_primary_conversion() {
    let transform = InputColorTransform::new(
        InputSourceKind::DisplayReferred,
        ColorSpace::SRGB,
        WorkingSpace::ACESCG,
        InputTransformOptions::default(),
    )
    .unwrap();
    let source = rgba_f32(
        ColorSpace::SRGB,
        AlphaMode::Straight,
        [0.735_357, 0.25, 0.040_45, 0.5],
    );

    let output = transform.apply(&source).unwrap();
    let expected_linear = [0.5, 0.050_876_088, 0.003_130_805];
    let expected = WideGamutTransform::new(
        ColorPrimaries::Bt709,
        ColorPrimaries::AcesAp1,
        ChromaticAdaptation::Bradford,
        GamutMapping::Preserve,
    )
    .unwrap()
    .apply_rgb(LinearRgb::new(expected_linear).unwrap())
    .unwrap()
    .values()
    .map(|component| component * 0.5);

    assert_eq!(output.space(), WorkingSpace::ACESCG);
    assert_eq!(
        output.image().descriptor().color_space(),
        ColorSpace::ACESCG
    );
    assert_rgba_close(output.image(), [expected[0], expected[1], expected[2], 0.5]);
}

#[test]
fn bt_scene_oetfs_are_never_used_as_display_eotfs() {
    let cases = [
        (
            ColorSpace::new(
                ColorPrimaries::Bt709,
                TransferFunction::Bt709,
                MatrixCoefficients::Rgb,
                ColorRange::Full,
            ),
            0.409_007_73_f32,
        ),
        (
            ColorSpace::new(
                ColorPrimaries::Bt2020,
                TransferFunction::Bt2020TenBit,
                MatrixCoefficients::Rgb,
                ColorRange::Full,
            ),
            0.408_848_1_f32,
        ),
    ];

    for (color_space, encoded_middle_gray) in cases {
        let display_error = InputColorTransform::new(
            InputSourceKind::DisplayReferred,
            color_space,
            WorkingSpace::ACESCG,
            InputTransformOptions::default(),
        )
        .unwrap_err();
        assert_eq!(display_error.category(), ErrorCategory::Unsupported);
        assert_eq!(
            display_error.recoverability(),
            Recoverability::UserCorrectable
        );
        assert!(display_error.contexts().iter().any(|context| {
            context.component() == "superi-color.transform-in"
                && context.field("source_kind") == Some("display_referred")
        }));

        let scene = InputColorTransform::new(
            InputSourceKind::SceneReferred,
            color_space,
            WorkingSpace::ACESCG,
            InputTransformOptions::default(),
        )
        .unwrap();
        let output = scene
            .apply_f32(&rgba_f32(
                color_space,
                AlphaMode::Opaque,
                [encoded_middle_gray; 4],
            ))
            .unwrap();
        assert_rgba_close_with(output.image(), [0.18, 0.18, 0.18, 1.0], 8.0e-7);
    }
}

#[test]
fn camera_aces_interchange_and_scene_hlg_are_explicit_sources() {
    let camera = InputColorTransform::new(
        InputSourceKind::Camera,
        ColorSpace::ACES2065_1,
        WorkingSpace::ACESCG,
        InputTransformOptions::default(),
    )
    .unwrap();
    let camera_source = rgba_f32(
        ColorSpace::ACES2065_1,
        AlphaMode::Premultiplied,
        [0.18, 0.09, 0.045, 1.0],
    );
    let camera_output = camera.apply_f32(&camera_source).unwrap();
    let expected = WideGamutTransform::new(
        ColorPrimaries::AcesAp0,
        ColorPrimaries::AcesAp1,
        ChromaticAdaptation::Bradford,
        GamutMapping::Preserve,
    )
    .unwrap()
    .apply_rgb(LinearRgb::new([0.18, 0.09, 0.045]).unwrap())
    .unwrap()
    .values();
    assert_rgba_close_with(
        camera_output.image(),
        [expected[0], expected[1], expected[2], 1.0],
        2.0e-7,
    );

    let hlg_rgb = ColorSpace::new(
        ColorPrimaries::Bt2020,
        TransferFunction::Hlg,
        MatrixCoefficients::Rgb,
        ColorRange::Full,
    );
    let scene = InputColorTransform::new(
        InputSourceKind::SceneReferred,
        hlg_rgb,
        WorkingSpace::ACESCG,
        InputTransformOptions::default(),
    )
    .unwrap();
    let scene_output = scene
        .apply_f32(&rgba_f32(hlg_rgb, AlphaMode::Opaque, [0.5, 0.5, 0.5, 1.0]))
        .unwrap();
    assert_rgba_close_with(
        scene_output.image(),
        [1.0 / 12.0, 1.0 / 12.0, 1.0 / 12.0, 1.0],
        3.0e-7,
    );
}

#[test]
fn pq_reference_white_is_explicit_and_does_not_tone_map() {
    let pq_rgb = ColorSpace::new(
        ColorPrimaries::Bt2020,
        TransferFunction::Pq,
        MatrixCoefficients::Rgb,
        ColorRange::Full,
    );
    assert!(InputColorTransform::new(
        InputSourceKind::DisplayReferred,
        pq_rgb,
        WorkingSpace::ACESCG,
        InputTransformOptions::default(),
    )
    .is_err());

    let options =
        InputTransformOptions::default().with_pq_reference_white(Nits::new(100.0).unwrap());
    let transform = InputColorTransform::new(
        InputSourceKind::DisplayReferred,
        pq_rgb,
        WorkingSpace::ACESCG,
        options,
    )
    .unwrap();
    let one_hundred_nits = pq_inverse_eotf(Nits::new(100.0).unwrap()).unwrap().value() as f32;
    let output = transform
        .apply_f32(&rgba_f32(pq_rgb, AlphaMode::Opaque, [one_hundred_nits; 4]))
        .unwrap();
    assert_rgba_close_with(output.image(), [1.0, 1.0, 1.0, 1.0], 5.0e-6);
}

#[test]
fn source_payloads_windows_metadata_and_alpha_semantics_survive() {
    let bounds = PixelBounds::from_origin_size(-3, 7, 1, 1).unwrap();
    let display = PixelBounds::from_origin_size(-10, 2, 20, 12).unwrap();
    let tags = ImageColorTags::new(ColorSpace::DISPLAY_P3)
        .with_named_space("display-p3 source")
        .unwrap()
        .with_icc_profile([1_u8, 4, 9].as_slice().into())
        .unwrap();
    let descriptor = ImageDescriptor::new_with_color_tags(
        bounds,
        display,
        PixelFormat::Rgba8Unorm,
        tags.clone(),
        AlphaMode::Straight,
    )
    .unwrap();
    let mut metadata = ImageMetadata::new();
    metadata
        .insert("camera.serial", ImageMetadataValue::Text("A-17".into()))
        .unwrap();
    let source = Image::new_with_metadata(
        descriptor,
        ImageSamples::from_u8([255, 0, 0, 64]),
        metadata.clone(),
    )
    .unwrap();
    let transform = InputColorTransform::new(
        InputSourceKind::DisplayReferred,
        ColorSpace::DISPLAY_P3,
        WorkingSpace::ACESCG,
        InputTransformOptions::default(),
    )
    .unwrap();

    let output = transform.apply(&source).unwrap();
    assert_eq!(output.image().descriptor().data_window(), bounds);
    assert_eq!(output.image().descriptor().display_window(), display);
    assert_eq!(output.image().metadata(), &metadata);
    assert_eq!(
        output.image().descriptor().color_tags().named_space(),
        tags.named_space()
    );
    assert_eq!(
        output.image().descriptor().color_tags().icc_profile(),
        tags.icc_profile()
    );
    assert_eq!(
        output.image().descriptor().alpha_mode(),
        AlphaMode::Premultiplied
    );
    assert_close(sample(output.image(), 3), 64.0 / 255.0, COMPONENT_TOLERANCE);
}

#[test]
fn canonical_binary16_storage_rejects_finite_overflow() {
    let transform = InputColorTransform::new(
        InputSourceKind::Camera,
        ColorSpace::ACESCG,
        WorkingSpace::ACESCG,
        InputTransformOptions::default(),
    )
    .unwrap();

    let boundary = transform
        .apply(&rgba_f32(
            ColorSpace::ACESCG,
            AlphaMode::Opaque,
            [65_504.0, -65_504.0, 0.0, 1.0],
        ))
        .unwrap();
    assert_eq!(sample(boundary.image(), 0), 65_504.0);
    assert_eq!(sample(boundary.image(), 1), -65_504.0);

    for component in [65_508.0, -65_508.0] {
        let error = transform
            .apply(&rgba_f32(
                ColorSpace::ACESCG,
                AlphaMode::Opaque,
                [component, 0.0, 0.0, 1.0],
            ))
            .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
        assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
        assert!(error.contexts().iter().any(|context| {
            context.component() == "superi-color.transform-in"
                && context.operation() == "validate_binary16_storage"
        }));
    }
}

#[test]
fn unsupported_or_mismatched_meaning_fails_with_actionable_context() {
    let limited_yuv = ColorSpace::BT709;
    let cases = [
        InputColorTransform::new(
            InputSourceKind::DisplayReferred,
            limited_yuv,
            WorkingSpace::ACESCG,
            InputTransformOptions::default(),
        ),
        InputColorTransform::new(
            InputSourceKind::Camera,
            ColorSpace::SRGB,
            WorkingSpace::ACESCG,
            InputTransformOptions::default(),
        ),
        InputColorTransform::new(
            InputSourceKind::SceneReferred,
            ColorSpace::SRGB,
            WorkingSpace::ACESCG,
            InputTransformOptions::default(),
        ),
    ];
    for result in cases {
        let error = result.unwrap_err();
        assert!(matches!(
            error.category(),
            ErrorCategory::InvalidInput | ErrorCategory::Unsupported
        ));
        assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
        assert!(error
            .contexts()
            .iter()
            .any(|context| context.component() == "superi-color.transform-in"));
    }
}

#[test]
fn input_transform_contracts_are_safe_to_share_between_engine_owners() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<InputColorTransform>();
    assert_send_sync::<InputTransformOptions>();
}

fn rgba_f32(color_space: ColorSpace, alpha: AlphaMode, rgba: [f32; 4]) -> Image {
    let bounds = PixelBounds::from_origin_size(0, 0, 1, 1).unwrap();
    Image::new(
        ImageDescriptor::new(bounds, bounds, PixelFormat::Rgba32Float, color_space, alpha).unwrap(),
        ImageSamples::from_f32(rgba),
    )
    .unwrap()
}

fn assert_rgba_close(image: &Image, expected: [f64; 4]) {
    assert_rgba_close_with(image, expected, COMPONENT_TOLERANCE);
}

fn assert_rgba_close_with(image: &Image, expected: [f64; 4], tolerance: f64) {
    for (index, expected) in expected.into_iter().enumerate() {
        assert_close(sample(image, index), expected, tolerance);
    }
}

fn sample(image: &Image, index: usize) -> f64 {
    image.samples().float_value(index).unwrap() as f64
}

fn assert_close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected:.12}, got {actual:.12}, tolerance {tolerance:.3e}"
    );
}
