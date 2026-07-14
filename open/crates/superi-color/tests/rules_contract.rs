use superi_color::lut::{DomainPolicy, Lut, LutInterpolation};
use superi_color::rules::{
    ColorRuleSet, DisplayRule, LookRule, OutputRule, SourceRole, ViewApplicability, ViewRule,
};
use superi_color::transform_in::InputSourceKind;
use superi_color::transform_out::{OutputColorTransform, OutputTargetKind, OutputTransformOptions};
use superi_color::working_space::{WorkingImage, WorkingImageF32, WorkingSpace};
use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::geometry::PixelBounds;
use superi_image::metadata::{ImageMetadata, ImageMetadataValue};
use superi_image::value::{Image, ImageSamples};

#[test]
fn first_applicable_view_is_default_and_explicit_selection_is_checked() {
    let rules = ColorRuleSet::new(
        vec![],
        vec![DisplayRule::new(
            "reference-monitor",
            vec![
                view(
                    "scene-rendering",
                    ViewApplicability::Only(SourceRole::SceneReferred),
                    vec![],
                ),
                view(
                    "display-bypass",
                    ViewApplicability::Only(SourceRole::DisplayReferred),
                    vec![],
                ),
            ],
        )
        .unwrap()],
        vec![],
    )
    .unwrap();

    assert_eq!(
        rules
            .select_view("reference-monitor", None, InputSourceKind::Camera.into(),)
            .unwrap()
            .name(),
        "scene-rendering"
    );
    assert_eq!(
        rules
            .select_view("reference-monitor", None, SourceRole::DisplayReferred)
            .unwrap()
            .name(),
        "display-bypass"
    );

    let error = rules
        .select_view(
            "reference-monitor",
            Some("scene-rendering"),
            SourceRole::DisplayReferred,
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
}

#[test]
fn display_pipeline_applies_named_looks_in_declared_order_before_encoding() {
    let looks = vec![
        look("halve", "LUT_1D_SIZE 2\n0 0 0\n0.5 0.5 0.5\n"),
        look("lift", "LUT_1D_SIZE 2\n0.25 0.25 0.25\n1 1 1\n"),
    ];
    let rules = ColorRuleSet::new(
        looks,
        vec![DisplayRule::new(
            "reference-monitor",
            vec![view(
                "graded",
                ViewApplicability::Any,
                vec!["halve".into(), "lift".into()],
            )],
        )
        .unwrap()],
        vec![],
    )
    .unwrap();
    let source = working_image([0.8, 0.8, 0.8, 1.0]);

    let output = rules
        .render_display(
            "reference-monitor",
            None,
            SourceRole::SceneReferred,
            &source,
        )
        .unwrap();

    let expected = 0.55_f64.powf(1.0 / 2.2);
    for channel in 0..3 {
        assert_close(sample(&output, channel), expected);
    }
}

#[test]
fn display_pipeline_preserves_image_identity_payloads() {
    let rules = ColorRuleSet::new(
        vec![look("identity", "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n")],
        vec![DisplayRule::new(
            "monitor",
            vec![view(
                "main",
                ViewApplicability::Any,
                vec!["identity".into()],
            )],
        )
        .unwrap()],
        vec![],
    )
    .unwrap();
    let bounds = PixelBounds::from_origin_size(-4, 7, 1, 1).unwrap();
    let descriptor = WorkingSpace::ACESCG
        .image_descriptor(bounds, bounds)
        .unwrap();
    let mut metadata = ImageMetadata::new();
    metadata
        .insert("grade.id", ImageMetadataValue::Text("approved-7".into()))
        .unwrap();
    let image = Image::new_with_metadata(
        descriptor,
        ImageSamples::f16_from_f32([0.2, 0.3, 0.4, 0.5]),
        metadata.clone(),
    )
    .unwrap();
    let source = WorkingImage::acescg(image).unwrap().promote_f32().unwrap();

    let output = rules
        .render_display("monitor", Some("main"), SourceRole::SceneReferred, &source)
        .unwrap();

    assert_eq!(output.descriptor().data_window(), bounds);
    assert_eq!(output.descriptor().display_window(), bounds);
    assert_eq!(output.metadata(), &metadata);
    assert_eq!(sample(&output, 3), 0.5);
}

#[test]
fn delivery_rules_are_independent_from_display_selection() {
    let rules = ColorRuleSet::new(
        vec![look(
            "delivery-grade",
            "LUT_1D_SIZE 2\n0 0 0\n0.5 0.5 0.5\n",
        )],
        vec![],
        vec![OutputRule::new(
            "archive-master",
            ViewApplicability::Only(SourceRole::SceneReferred),
            vec!["delivery-grade".into()],
            output_transform(OutputTargetKind::Deliverable, linear_acescg()),
        )
        .unwrap()],
    )
    .unwrap();

    let output = rules
        .render_output(
            "archive-master",
            SourceRole::SceneReferred,
            &working_image([0.8, 0.8, 0.8, 1.0]),
        )
        .unwrap();

    assert_close(sample(&output, 0), 0.4);
}

#[test]
fn construction_rejects_duplicate_names_and_missing_look_references() {
    let duplicate = ColorRuleSet::new(
        vec![
            look("grade", "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n"),
            look("grade", "LUT_1D_SIZE 2\n0 0 0\n1 1 1\n"),
        ],
        vec![],
        vec![],
    )
    .unwrap_err();
    assert_eq!(duplicate.category(), ErrorCategory::Conflict);

    let missing = ColorRuleSet::new(
        vec![],
        vec![DisplayRule::new(
            "monitor",
            vec![view(
                "main",
                ViewApplicability::Any,
                vec!["unregistered".into()],
            )],
        )
        .unwrap()],
        vec![],
    )
    .unwrap_err();
    assert_eq!(missing.category(), ErrorCategory::InvalidInput);
    assert!(missing
        .contexts()
        .iter()
        .any(|context| context.component() == "superi-color.rules"));
}

#[test]
fn display_and_output_rules_reject_the_wrong_transform_role() {
    let display_error = ViewRule::new(
        "main",
        ViewApplicability::Any,
        vec![],
        output_transform(OutputTargetKind::Deliverable, linear_acescg()),
    )
    .unwrap_err();
    assert_eq!(display_error.category(), ErrorCategory::InvalidInput);

    let output_error = OutputRule::new(
        "master",
        ViewApplicability::Any,
        vec![],
        output_transform(OutputTargetKind::Display, display_acescg()),
    )
    .unwrap_err();
    assert_eq!(output_error.category(), ErrorCategory::InvalidInput);
}

#[test]
fn look_process_space_must_match_the_working_image() {
    let p3_linear = WorkingSpace::new(ColorSpace::new(
        ColorPrimaries::DisplayP3,
        TransferFunction::Linear,
        MatrixCoefficients::Rgb,
        ColorRange::Full,
    ))
    .unwrap();
    let look = LookRule::new(
        "p3-grade",
        p3_linear,
        Lut::parse_cube("LUT_1D_SIZE 2\n0 0 0\n1 1 1\n").unwrap(),
        LutInterpolation::Linear,
        DomainPolicy::Reject,
    )
    .unwrap();
    let rules = ColorRuleSet::new(
        vec![look],
        vec![DisplayRule::new(
            "monitor",
            vec![view(
                "main",
                ViewApplicability::Any,
                vec!["p3-grade".into()],
            )],
        )
        .unwrap()],
        vec![],
    )
    .unwrap();

    let error = rules
        .render_display(
            "monitor",
            None,
            SourceRole::SceneReferred,
            &working_image([0.2, 0.2, 0.2, 1.0]),
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
}

fn look(name: &str, source: &str) -> LookRule {
    LookRule::new(
        name,
        WorkingSpace::ACESCG,
        Lut::parse_cube(source).unwrap(),
        LutInterpolation::Linear,
        DomainPolicy::Reject,
    )
    .unwrap()
}

fn view(name: &str, applicability: ViewApplicability, looks: Vec<String>) -> ViewRule {
    ViewRule::new(
        name,
        applicability,
        looks,
        output_transform(OutputTargetKind::Display, display_acescg()),
    )
    .unwrap()
}

fn output_transform(kind: OutputTargetKind, destination: ColorSpace) -> OutputColorTransform {
    OutputColorTransform::new(
        kind,
        WorkingSpace::ACESCG,
        destination,
        OutputTransformOptions::new(),
    )
    .unwrap()
}

fn display_acescg() -> ColorSpace {
    ColorSpace::new(
        ColorPrimaries::AcesAp1,
        TransferFunction::Gamma22,
        MatrixCoefficients::Rgb,
        ColorRange::Full,
    )
}

fn linear_acescg() -> ColorSpace {
    ColorSpace::new(
        ColorPrimaries::AcesAp1,
        TransferFunction::Linear,
        MatrixCoefficients::Rgb,
        ColorRange::Full,
    )
}

fn working_image(rgba: [f32; 4]) -> WorkingImageF32 {
    let bounds = PixelBounds::from_origin_size(0, 0, 1, 1).unwrap();
    let descriptor = WorkingSpace::ACESCG
        .image_descriptor(bounds, bounds)
        .unwrap();
    WorkingImage::acescg(Image::new(descriptor, ImageSamples::f16_from_f32(rgba)).unwrap())
        .unwrap()
        .promote_f32()
        .unwrap()
}

fn sample(image: &Image, index: usize) -> f64 {
    f64::from(image.samples().float_value(index).unwrap())
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 2.0e-4,
        "expected {expected}, got {actual}"
    );
}
