use std::sync::Arc;

use superi_color::gamut::{ChromaticAdaptation, GamutMapping};
use superi_color::hdr::{pq_inverse_eotf, Nits};
use superi_color::transform_out::{
    LegalRangeEncoder, OutputColorTransform, OutputTargetKind, OutputTransformOptions,
    ToneMapParameters, ToneMapping,
};
use superi_color::working_space::{WorkingImage, WorkingImageF32, WorkingSpace};
use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::geometry::PixelBounds;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_image::metadata::{ImageColorTags, ImageMetadata, ImageMetadataValue};
use superi_image::value::{Image, ImageDescriptor, ImageSamples};

const STRICT: f32 = 3.0e-6;

#[test]
fn legal_range_uses_normative_rgb_anchor_codes_at_each_depth() {
    for (depth, black, white) in [(8, 16, 235), (10, 64, 940), (12, 256, 3760)] {
        let range = LegalRangeEncoder::new(depth).unwrap();
        assert_eq!(range.bit_depth(), depth);
        assert_eq!(range.black_code(), black);
        assert_eq!(range.white_code(), white);
        assert_eq!(range.encode_code_value(0.0).unwrap(), black);
        assert_eq!(range.encode_code_value(1.0).unwrap(), white);
        assert_close(
            range.encode_normalized(0.0).unwrap() as f32,
            f32::from(black) / f32::from(range.maximum_code()),
        );
        assert_close(
            range.encode_normalized(1.0).unwrap() as f32,
            f32::from(white) / f32::from(range.maximum_code()),
        );
    }
    let ten_bit = LegalRangeEncoder::new(10).unwrap();
    assert_eq!(ten_bit.encode_code_value(0.5).unwrap(), 502);
    assert_close(
        ten_bit.encode_normalized(0.5).unwrap() as f32,
        502.0 / 1023.0,
    );
}

#[test]
fn luminance_shoulder_is_identity_below_knee_and_preserves_rgb_ratios() {
    let space = rgb_space(ColorPrimaries::Bt2020, TransferFunction::Linear);
    let working_space = WorkingSpace::new(space).unwrap();
    let source = working_f32_pixels(
        [0.05, 0.1, 0.15, 1.0, 2.0, 4.0, 8.0, 1.0, 8.0, 8.0, 8.0, 1.0],
        working_space,
    );
    let parameters = ToneMapParameters::new(0.2, 8.0, 1.0).unwrap();
    let options =
        OutputTransformOptions::new().with_tone_mapping(ToneMapping::LuminanceShoulder(parameters));
    let transform =
        OutputColorTransform::new(OutputTargetKind::Deliverable, working_space, space, options)
            .unwrap();

    let output = transform.apply_f32(&source).unwrap();
    for (actual, expected) in output_rgb(&output, 0).into_iter().zip([0.05, 0.1, 0.15]) {
        assert_close(actual, expected);
    }
    let mapped = output_rgb(&output, 4);
    assert_close(mapped[1] / mapped[0], 2.0);
    assert_close(mapped[2] / mapped[0], 4.0);
    assert!(mapped[2] < 8.0);
    for component in output_rgb(&output, 8) {
        assert_close(component, 1.0);
    }
    assert_eq!(output.samples().float_value(7), Some(1.0));
    assert_eq!(parameters.knee(), 0.2);
    assert_eq!(parameters.source_peak(), 8.0);
    assert_eq!(parameters.destination_peak(), 1.0);
    assert!(parameters.shoulder_scale() > 0.0);
    assert_eq!(transform.options().tone_mapping(), options.tone_mapping());
}

#[test]
fn legal_range_is_a_separate_storage_stage_after_authoritative_output() {
    let space = rgb_space(ColorPrimaries::Bt2020, TransferFunction::Linear);
    let working_space = WorkingSpace::new(space).unwrap();
    let source = working_f32_pixels([0.0, 0.0, 0.0, 0.25, 0.5, 0.5, 0.5, 0.5], working_space);
    let output = OutputColorTransform::new(
        OutputTargetKind::Deliverable,
        working_space,
        space,
        OutputTransformOptions::new(),
    )
    .unwrap()
    .apply_f32(&source)
    .unwrap();
    assert_eq!(output.descriptor().color_space().range(), ColorRange::Full);
    assert_eq!(output.descriptor().alpha_mode(), AlphaMode::Premultiplied);

    let encoder = LegalRangeEncoder::new(10).unwrap();
    let limited = encoder.apply(&output).unwrap();
    let black = 64.0_f32 / 1023.0;
    let white = 940.0_f32 / 1023.0;
    for value in output_rgb(&limited, 0) {
        assert_close(value, black);
    }
    for value in output_rgb(&limited, 4) {
        assert_close(value, white);
    }
    assert_eq!(limited.samples().float_value(3), Some(0.25));
    assert_eq!(limited.samples().float_value(7), Some(0.5));
    assert_eq!(
        limited.descriptor().color_space().range(),
        ColorRange::Limited
    );
    assert_eq!(limited.descriptor().alpha_mode(), AlphaMode::Straight);
}

#[test]
fn tone_and_legal_configuration_fail_closed_and_remain_shareable() {
    for depth in [0, 7, 17, u8::MAX] {
        assert_invalid(LegalRangeEncoder::new(depth));
    }
    assert_invalid(ToneMapParameters::new(1.0, 1.0, 1.0));
    assert_invalid(ToneMapParameters::new(1.0, 4.0, 4.0));

    let encoder = LegalRangeEncoder::new(10).unwrap();
    let extended = output_image(
        [1.1, 0.0, 0.0, 1.0],
        rgb_space(ColorPrimaries::Bt2020, TransferFunction::Gamma24),
    );
    assert_invalid(encoder.apply(&extended));

    fn assert_send_sync_copy<T: Send + Sync + Copy>() {}
    assert_send_sync_copy::<ToneMapParameters>();
    assert_send_sync_copy::<ToneMapping>();
    assert_send_sync_copy::<LegalRangeEncoder>();
}

#[test]
fn display_output_converts_primaries_then_encodes_and_preserves_artifact_identity() {
    let bounds = PixelBounds::from_origin_size(-3, 7, 1, 1).unwrap();
    let tags = ImageColorTags::new(ColorSpace::ACESCG)
        .with_named_space("ACEScg working")
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
        ImageSamples::from_f32([0.09, 0.09, 0.09, 0.5]),
        metadata.clone(),
    )
    .unwrap();
    let working = WorkingImageF32::new(WorkingSpace::ACESCG, image).unwrap();
    let transform = OutputColorTransform::new(
        OutputTargetKind::Display,
        WorkingSpace::ACESCG,
        ColorSpace::SRGB,
        OutputTransformOptions::new(),
    )
    .unwrap();

    let output = transform.apply_f32(&working).unwrap();
    let expected = 0.461_356_13_f32 * 0.5;
    for index in 0..3 {
        assert_close(output.samples().float_value(index).unwrap(), expected);
    }
    assert_eq!(output.samples().float_value(3), Some(0.5));
    assert_eq!(output.descriptor().data_window(), bounds);
    assert_eq!(output.descriptor().display_window(), bounds);
    assert_eq!(output.descriptor().pixel_format(), PixelFormat::Rgba32Float);
    assert_eq!(output.descriptor().color_space(), ColorSpace::SRGB);
    assert_eq!(output.descriptor().alpha_mode(), AlphaMode::Premultiplied);
    assert_eq!(
        output.descriptor().color_tags().named_space(),
        Some("ACEScg working")
    );
    assert_eq!(
        output.descriptor().color_tags().icc_profile(),
        Some([1_u8, 3, 7, 9].as_slice())
    );
    assert_eq!(output.metadata(), &metadata);
}

#[test]
fn delivery_output_accepts_canonical_f16_and_uses_absolute_pq_reference_white() {
    let working = working_f16([1.0, 1.0, 1.0, 1.0], WorkingSpace::ACESCG);
    let destination = rgb_space(ColorPrimaries::Bt2020, TransferFunction::Pq);
    let reference_white = Nits::new(203.0).unwrap();
    let transform = OutputColorTransform::new(
        OutputTargetKind::Deliverable,
        WorkingSpace::ACESCG,
        destination,
        OutputTransformOptions::new().with_pq_reference_white(reference_white),
    )
    .unwrap();

    let output = transform.apply(&working).unwrap();
    let expected = pq_inverse_eotf(reference_white).unwrap().value() as f32;
    for index in 0..3 {
        assert_close(output.samples().float_value(index).unwrap(), expected);
    }
    assert_eq!(output.descriptor().color_space(), destination);
    assert_eq!(output.descriptor().pixel_format(), PixelFormat::Rgba32Float);
}

#[test]
fn hlg_and_extended_sdr_targets_keep_explicit_output_semantics() {
    let source = working_f32([-0.25, 0.18, 2.0, 1.0], WorkingSpace::ACESCG);
    let sdr = OutputColorTransform::new(
        OutputTargetKind::Deliverable,
        WorkingSpace::ACESCG,
        rgb_space(ColorPrimaries::AcesAp1, TransferFunction::Gamma24),
        OutputTransformOptions::new(),
    )
    .unwrap()
    .apply_f32(&source)
    .unwrap();
    assert!(sdr.samples().float_value(0).unwrap() < 0.0);
    assert!(sdr.samples().float_value(2).unwrap() > 1.0);

    let hlg_source = working_f32([0.0, 1.0 / 12.0, 1.0, 1.0], WorkingSpace::ACESCG);
    let hlg = OutputColorTransform::new(
        OutputTargetKind::Deliverable,
        WorkingSpace::ACESCG,
        rgb_space(ColorPrimaries::AcesAp1, TransferFunction::Hlg),
        OutputTransformOptions::new(),
    )
    .unwrap()
    .apply_f32(&hlg_source)
    .unwrap();
    assert_close(hlg.samples().float_value(0).unwrap(), 0.0);
    assert_close(hlg.samples().float_value(1).unwrap(), 0.5);
    assert_close(hlg.samples().float_value(2).unwrap(), 1.0);
}

#[test]
fn output_configuration_and_source_mismatches_fail_explicitly() {
    let pq = rgb_space(ColorPrimaries::Bt2020, TransferFunction::Pq);
    assert_invalid(OutputColorTransform::new(
        OutputTargetKind::Display,
        WorkingSpace::ACESCG,
        pq,
        OutputTransformOptions::new(),
    ));
    assert_invalid(OutputColorTransform::new(
        OutputTargetKind::Display,
        WorkingSpace::ACESCG,
        ColorSpace::BT709,
        OutputTransformOptions::new(),
    ));
    assert_invalid(OutputColorTransform::new(
        OutputTargetKind::Display,
        WorkingSpace::ACESCG,
        ColorSpace::new(
            ColorPrimaries::Bt709,
            TransferFunction::Linear,
            MatrixCoefficients::Rgb,
            ColorRange::Full,
        ),
        OutputTransformOptions::new(),
    ));
    assert_invalid(OutputColorTransform::new(
        OutputTargetKind::Deliverable,
        WorkingSpace::ACESCG,
        ColorSpace::SRGB,
        OutputTransformOptions::new().with_pq_reference_white(Nits::new(203.0).unwrap()),
    ));

    let other_space =
        WorkingSpace::new(rgb_space(ColorPrimaries::Bt2020, TransferFunction::Linear)).unwrap();
    let wrong_source = working_f32([0.0, 0.0, 0.0, 1.0], other_space);
    let transform = OutputColorTransform::new(
        OutputTargetKind::Deliverable,
        WorkingSpace::ACESCG,
        ColorSpace::SRGB,
        OutputTransformOptions::new(),
    )
    .unwrap();
    assert_invalid(transform.apply_f32(&wrong_source));
}

#[test]
fn pq_rejects_negative_and_over_peak_values_instead_of_hiding_clipping() {
    let destination = rgb_space(ColorPrimaries::AcesAp1, TransferFunction::Pq);
    let transform = OutputColorTransform::new(
        OutputTargetKind::Deliverable,
        WorkingSpace::ACESCG,
        destination,
        OutputTransformOptions::new().with_pq_reference_white(Nits::new(203.0).unwrap()),
    )
    .unwrap();
    assert_invalid(
        transform.apply_f32(&working_f32([-0.001, 0.0, 0.0, 1.0], WorkingSpace::ACESCG)),
    );
    assert_invalid(
        transform.apply_f32(&working_f32([50.0, 50.0, 50.0, 1.0], WorkingSpace::ACESCG)),
    );
}

#[test]
fn output_contracts_are_deterministic_copyable_and_shareable() {
    fn assert_send_sync_copy<T: Send + Sync + Copy>() {}
    assert_send_sync_copy::<OutputTargetKind>();
    assert_send_sync_copy::<OutputTransformOptions>();
    assert_send_sync_copy::<OutputColorTransform>();
    assert_send_sync_copy::<ToneMapParameters>();
    assert_send_sync_copy::<ToneMapping>();
    assert_send_sync_copy::<LegalRangeEncoder>();

    let options = OutputTransformOptions::new()
        .with_chromatic_adaptation(ChromaticAdaptation::Bradford)
        .with_gamut_mapping(GamutMapping::PreserveLuminance);
    let transform = OutputColorTransform::new(
        OutputTargetKind::Display,
        WorkingSpace::ACESCG,
        ColorSpace::DISPLAY_P3,
        options,
    )
    .unwrap();
    assert_eq!(transform.target_kind(), OutputTargetKind::Display);
    assert_eq!(transform.source(), WorkingSpace::ACESCG);
    assert_eq!(transform.destination(), ColorSpace::DISPLAY_P3);
    assert_eq!(transform.options(), options);

    let source = working_f32([0.1, 0.2, 0.3, 0.5], WorkingSpace::ACESCG);
    let first = transform.apply_f32(&source).unwrap();
    let second = transform.apply_f32(&source).unwrap();
    assert_eq!(first.samples().f32_bits(), second.samples().f32_bits());
}

fn rgb_space(primaries: ColorPrimaries, transfer: TransferFunction) -> ColorSpace {
    ColorSpace::new(
        primaries,
        transfer,
        MatrixCoefficients::Rgb,
        ColorRange::Full,
    )
}

fn working_f16(samples: [f32; 4], space: WorkingSpace) -> WorkingImage {
    let bounds = PixelBounds::from_origin_size(0, 0, 1, 1).unwrap();
    let descriptor = space.image_descriptor(bounds, bounds).unwrap();
    let image = Image::new(descriptor, ImageSamples::f16_from_f32(samples)).unwrap();
    WorkingImage::new(space, image).unwrap()
}

fn working_f32(samples: [f32; 4], space: WorkingSpace) -> WorkingImageF32 {
    working_f32_pixels(samples, space)
}

fn working_f32_pixels(
    samples: impl IntoIterator<Item = f32>,
    space: WorkingSpace,
) -> WorkingImageF32 {
    let samples = samples.into_iter().collect::<Vec<_>>();
    let width = u32::try_from(samples.len() / 4).unwrap();
    let bounds = PixelBounds::from_origin_size(0, 0, width, 1).unwrap();
    let descriptor = ImageDescriptor::new(
        bounds,
        bounds,
        PixelFormat::Rgba32Float,
        space.color_space(),
        AlphaMode::Premultiplied,
    )
    .unwrap();
    let image = Image::new(descriptor, ImageSamples::from_f32(samples)).unwrap();
    WorkingImageF32::new(space, image).unwrap()
}

fn output_image(samples: [f32; 4], color_space: ColorSpace) -> Image {
    let bounds = PixelBounds::from_origin_size(0, 0, 1, 1).unwrap();
    let descriptor = ImageDescriptor::new(
        bounds,
        bounds,
        PixelFormat::Rgba32Float,
        color_space,
        AlphaMode::Premultiplied,
    )
    .unwrap();
    Image::new(descriptor, ImageSamples::from_f32(samples)).unwrap()
}

fn output_rgb(image: &Image, start: usize) -> [f32; 3] {
    [
        image.samples().float_value(start).unwrap(),
        image.samples().float_value(start + 1).unwrap(),
        image.samples().float_value(start + 2).unwrap(),
    ]
}

fn assert_invalid<T>(result: superi_core::error::Result<T>) {
    let error = result.err().expect("operation must reject invalid input");
    assert!(matches!(
        error.category(),
        ErrorCategory::InvalidInput | ErrorCategory::Unsupported
    ));
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert!(!error.contexts().is_empty());
}

fn assert_close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= STRICT,
        "expected {expected:.8}, got {actual:.8}"
    );
}
