use superi_color::lut::{DomainPolicy, Lut, LutInterpolation};
use superi_color::working_space::{WorkingImageF32, WorkingSpace};
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::geometry::PixelBounds;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_image::metadata::{ImageMetadata, ImageMetadataValue};
use superi_image::value::{Image, ImageDescriptor, ImageSamples};

const STRICT: f32 = 2.0e-6;

#[test]
fn cube_1d_parses_metadata_and_interpolates_independent_channels() {
    let source = r#"
        # the domain is intentionally extended
        TITLE "channel curves"
        LUT_1D_SIZE 3
        DOMAIN_MIN -1 -1 -1
        DOMAIN_MAX 1 1 1
        0.0  0.0 0.0
        0.25 0.5 0.75
        1.0  1.0 1.0 # inline comments are accepted
    "#;
    let lut = Lut::parse_cube(source).unwrap();

    assert!(matches!(lut, Lut::OneDimensional(_)));
    assert_eq!(lut.title(), Some("channel curves"));
    assert_eq!(lut.size(), 3);
    assert_eq!(lut.domain_min(), [-1.0; 3]);
    assert_eq!(lut.domain_max(), [1.0; 3]);
    assert_rgb_close(
        lut.apply(
            [-0.5, 0.5, 1.0],
            LutInterpolation::Linear,
            DomainPolicy::Reject,
        )
        .unwrap(),
        [0.125, 0.75, 1.0],
    );

    let error = lut
        .apply(
            [-1.01, 0.0, 0.0],
            LutInterpolation::Linear,
            DomainPolicy::Reject,
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_rgb_close(
        lut.apply(
            [-2.0, 2.0, 0.0],
            LutInterpolation::Linear,
            DomainPolicy::Clamp,
        )
        .unwrap(),
        [0.0, 1.0, 0.75],
    );
}

#[test]
fn cube_3d_uses_red_fastest_order_and_explicit_interpolation() {
    let identity = Lut::parse_cube(
        r#"
        LUT_3D_SIZE 2
        0 0 0
        1 0 0
        0 1 0
        1 1 0
        0 0 1
        1 0 1
        0 1 1
        1 1 1
        "#,
    )
    .unwrap();
    assert!(matches!(identity, Lut::ThreeDimensional(_)));
    assert_rgb_close(
        identity
            .apply(
                [0.25, 0.5, 0.75],
                LutInterpolation::Trilinear,
                DomainPolicy::Reject,
            )
            .unwrap(),
        [0.25, 0.5, 0.75],
    );
    assert_rgb_close(
        identity
            .apply(
                [0.25, 0.5, 0.75],
                LutInterpolation::Tetrahedral,
                DomainPolicy::Reject,
            )
            .unwrap(),
        [0.25, 0.5, 0.75],
    );

    let asymmetric = Lut::parse_cube(
        r#"
        LUT_3D_SIZE 2
        0 10 -5
        2 12 -1
        6 14 3
        10 20 9
        4 18 7
        12 24 15
        16 30 21
        30 40 35
        "#,
    )
    .unwrap();
    assert_rgb_close(
        asymmetric
            .apply(
                [0.2, 0.5, 0.8],
                LutInterpolation::Trilinear,
                DomainPolicy::Reject,
            )
            .unwrap(),
        [10.48, 23.04, 12.96],
    );
    for (input, expected) in [
        ([0.8, 0.5, 0.2], [9.6, 19.6, 8.4]),
        ([0.8, 0.2, 0.5], [10.2, 20.8, 10.2]),
        ([0.5, 0.2, 0.8], [10.8, 22.6, 12.6]),
        ([0.2, 0.5, 0.8], [12.0, 24.4, 14.4]),
        ([0.2, 0.8, 0.5], [12.6, 23.2, 13.2]),
        ([0.5, 0.8, 0.2], [10.8, 20.2, 9.6]),
    ] {
        assert_rgb_close(
            asymmetric
                .apply(input, LutInterpolation::Tetrahedral, DomainPolicy::Reject)
                .unwrap(),
            expected,
        );
    }

    assert!(asymmetric
        .apply([0.5; 3], LutInterpolation::Linear, DomainPolicy::Reject)
        .is_err());
    assert!(Lut::parse_cube("LUT_1D_SIZE 2\n0 0 0\n1 1 1")
        .unwrap()
        .apply([0.5; 3], LutInterpolation::Trilinear, DomainPolicy::Reject)
        .is_err());
}

#[test]
fn cube_parser_rejects_ambiguous_corrupt_and_unbounded_inputs() {
    let corrupt = [
        "0 0 0\n1 1 1",
        "LUT_1D_SIZE 2\n0 0 0",
        "LUT_1D_SIZE 2\nLUT_3D_SIZE 2\n0 0 0\n1 1 1",
        "LUT_1D_SIZE 2\nDOMAIN_MIN 0 0 0\nDOMAIN_MIN 0 0 0\n0 0 0\n1 1 1",
        "LUT_1D_SIZE 2\nDOMAIN_MIN 1 0 0\nDOMAIN_MAX 1 1 1\n0 0 0\n1 1 1",
        "LUT_1D_SIZE 2\n0 0 0\nNaN 1 1",
        "LUT_3D_SIZE 2\n0 0 0 0",
    ];
    for source in corrupt {
        let error = Lut::parse_cube(source).unwrap_err();
        assert!(matches!(
            error.category(),
            ErrorCategory::CorruptData | ErrorCategory::InvalidInput
        ));
        assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
        assert!(!error.contexts().is_empty());
    }

    let unsupported = Lut::parse_cube("LUT_3D_INPUT_RANGE 0 1").unwrap_err();
    assert_eq!(unsupported.category(), ErrorCategory::Unsupported);

    let exhausted = Lut::parse_cube("LUT_3D_SIZE 500").unwrap_err();
    assert_eq!(exhausted.category(), ErrorCategory::ResourceExhausted);
}

#[test]
fn lut_application_preserves_working_artifact_and_premultiplied_alpha() {
    let lut = Lut::parse_cube(
        r#"
        TITLE "nonlinear working look"
        LUT_1D_SIZE 3
        0 0 0
        0.25 0.25 0.25
        1 1 1
        "#,
    )
    .unwrap();
    let bounds = PixelBounds::from_origin_size(-2, 4, 2, 1).unwrap();
    let descriptor = ImageDescriptor::new(
        bounds,
        bounds,
        PixelFormat::Rgba32Float,
        WorkingSpace::ACESCG.color_space(),
        AlphaMode::Premultiplied,
    )
    .unwrap();
    let mut metadata = ImageMetadata::new();
    metadata
        .insert(
            "source.camera",
            ImageMetadataValue::Text("camera a".to_owned()),
        )
        .unwrap();
    let image = Image::new_with_metadata(
        descriptor,
        ImageSamples::from_f32([0.1, 0.2, 0.3, 0.5, -0.25, 2.0, 0.5, 0.0]),
        metadata,
    )
    .unwrap();
    let working = WorkingImageF32::new(WorkingSpace::ACESCG, image).unwrap();

    let transformed = lut
        .apply_to_working_image(&working, LutInterpolation::Linear, DomainPolicy::Reject)
        .unwrap();
    let expected = [0.05, 0.1, 0.2, 0.5, -0.25, 2.0, 0.5, 0.0];
    for (index, expected) in expected.into_iter().enumerate() {
        assert_close(
            transformed.image().samples().float_value(index).unwrap(),
            expected,
        );
    }
    assert_eq!(
        transformed.image().descriptor(),
        working.image().descriptor()
    );
    assert_eq!(transformed.image().metadata(), working.image().metadata());
    assert_eq!(
        &transformed.image().samples().f32_bits().unwrap()[4..8],
        &working.image().samples().f32_bits().unwrap()[4..8]
    );
}

#[test]
fn finite_failures_are_classified_at_rgb_and_working_image_boundaries() {
    let identity = Lut::parse_cube("LUT_1D_SIZE 2\n0 0 0\n1 1 1").unwrap();
    for nonfinite in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert_user_correctable_invalid(identity.apply(
            [nonfinite, 0.5, 0.5],
            LutInterpolation::Linear,
            DomainPolicy::Reject,
        ));
    }

    let nonfinite_working = working_pixel([0.25, f32::INFINITY, 0.75, 1.0]);
    assert_user_correctable_invalid(identity.apply_to_working_image(
        &nonfinite_working,
        LutInterpolation::Linear,
        DomainPolicy::Reject,
    ));

    let overflow =
        Lut::parse_cube("LUT_1D_SIZE 2\n0 0 0\n3.4028235e38 3.4028235e38 3.4028235e38").unwrap();
    let high_alpha = working_pixel([2.0, 2.0, 2.0, 2.0]);
    assert_user_correctable_invalid(overflow.apply_to_working_image(
        &high_alpha,
        LutInterpolation::Linear,
        DomainPolicy::Reject,
    ));
}

#[test]
fn lut_contracts_are_owned_deterministic_and_shareable() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<Lut>();

    let lut = Lut::parse_cube("\u{feff}LUT_1D_SIZE 2\n0 0 0\n1 1 1").unwrap();
    let first = lut
        .apply(
            [0.1, 0.2, 0.3],
            LutInterpolation::Linear,
            DomainPolicy::Reject,
        )
        .unwrap();
    let second = lut
        .apply(
            [0.1, 0.2, 0.3],
            LutInterpolation::Linear,
            DomainPolicy::Reject,
        )
        .unwrap();
    assert_eq!(first.map(f32::to_bits), second.map(f32::to_bits));

    let extreme = Lut::parse_cube(
        "LUT_1D_SIZE 2\n-3.4028235e38 -3.4028235e38 -3.4028235e38\n3.4028235e38 3.4028235e38 3.4028235e38",
    )
    .unwrap();
    assert_rgb_close(
        extreme
            .apply([0.5; 3], LutInterpolation::Linear, DomainPolicy::Reject)
            .unwrap(),
        [0.0; 3],
    );
}

fn assert_rgb_close(actual: [f32; 3], expected: [f32; 3]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert_close(actual, expected);
    }
}

fn working_pixel(samples: [f32; 4]) -> WorkingImageF32 {
    let bounds = PixelBounds::from_origin_size(0, 0, 1, 1).unwrap();
    let descriptor = ImageDescriptor::new(
        bounds,
        bounds,
        PixelFormat::Rgba32Float,
        WorkingSpace::ACESCG.color_space(),
        AlphaMode::Premultiplied,
    )
    .unwrap();
    let image = Image::new(descriptor, ImageSamples::from_f32(samples)).unwrap();
    WorkingImageF32::new(WorkingSpace::ACESCG, image).unwrap()
}

fn assert_user_correctable_invalid<T>(result: superi_core::error::Result<T>) {
    let error = result
        .err()
        .expect("operation must reject nonfinite output");
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= STRICT,
        "expected {expected:.8}, got {actual:.8}"
    );
}
