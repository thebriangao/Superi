use superi_core::color_space::ColorSpace;
use superi_core::error::ErrorCategory;
use superi_core::geometry::{Matrix3, PixelBounds, Vector2};
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_effects::reference::{
    evaluate_reference, required_input_regions, ReferenceBlendMode, ReferenceCompositeOperator,
    ReferenceEffectState, ReferenceSampling,
};
use superi_image::limits::ImageLimits;
use superi_image::metadata::ImageMetadataValue;
use superi_image::value::{Image, ImageDescriptor, ImageSamples};

fn bounds(width: u32, height: u32) -> PixelBounds {
    PixelBounds::from_origin_size(0, 0, width, height).unwrap()
}

fn image32(width: u32, height: u32, pixels: &[[f32; 4]]) -> Image {
    let region = bounds(width, height);
    let descriptor = ImageDescriptor::new(
        region,
        region,
        PixelFormat::Rgba32Float,
        ColorSpace::ACESCG,
        AlphaMode::Premultiplied,
    )
    .unwrap();
    Image::new(
        descriptor,
        ImageSamples::from_f32(pixels.iter().flat_map(|pixel| pixel.iter().copied())),
    )
    .unwrap()
    .with_metadata(
        "fixture",
        ImageMetadataValue::Text("reference-contract".to_owned()),
    )
    .unwrap()
}

fn image16(width: u32, height: u32, pixels: &[[f32; 4]]) -> Image {
    let region = bounds(width, height);
    let descriptor = ImageDescriptor::new(
        region,
        region,
        PixelFormat::Rgba16Float,
        ColorSpace::ACESCG,
        AlphaMode::Premultiplied,
    )
    .unwrap();
    Image::new(
        descriptor,
        ImageSamples::f16_from_f32(pixels.iter().flat_map(|pixel| pixel.iter().copied())),
    )
    .unwrap()
    .with_metadata(
        "fixture",
        ImageMetadataValue::Text("reference-contract".to_owned()),
    )
    .unwrap()
}

fn pixels(image: &Image) -> Vec<[f32; 4]> {
    image
        .samples()
        .len()
        .checked_div(4)
        .map(|count| {
            (0..count)
                .map(|pixel| {
                    [
                        image.samples().float_value(pixel * 4).unwrap(),
                        image.samples().float_value(pixel * 4 + 1).unwrap(),
                        image.samples().float_value(pixel * 4 + 2).unwrap(),
                        image.samples().float_value(pixel * 4 + 3).unwrap(),
                    ]
                })
                .collect()
        })
        .unwrap()
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 0.002,
        "expected {expected}, observed {actual}"
    );
}

fn run(state: &ReferenceEffectState, inputs: &[Image], region: PixelBounds) -> Image {
    evaluate_reference(state, inputs, region, &ImageLimits::default()).unwrap()
}

#[test]
fn geometry_opacity_and_utilities_produce_real_extended_pixels() {
    let source = image32(
        2,
        2,
        &[
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 0.5, 0.0, 0.5],
            [0.0, 0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0, 1.0],
        ],
    );
    let region = bounds(2, 2);

    let translated = run(
        &ReferenceEffectState::Transform {
            matrix: Matrix3::translation(Vector2::new(1.0, 0.0).unwrap()),
            sampling: ReferenceSampling::Nearest,
        },
        std::slice::from_ref(&source),
        region,
    );
    assert_eq!(pixels(&translated)[0], [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(pixels(&translated)[1], [1.0, 0.0, 0.0, 1.0]);

    let cropped = run(
        &ReferenceEffectState::Crop {
            left: 1,
            top: 0,
            right: 0,
            bottom: 0,
        },
        std::slice::from_ref(&source),
        region,
    );
    assert_eq!(pixels(&cropped)[0], [0.0, 0.0, 0.0, 0.0]);
    assert_eq!(pixels(&cropped)[1], [0.0, 0.5, 0.0, 0.5]);

    let opacity = run(
        &ReferenceEffectState::Opacity { opacity: 0.5 },
        std::slice::from_ref(&source),
        region,
    );
    assert_eq!(pixels(&opacity)[0], [0.5, 0.0, 0.0, 0.5]);

    let inverted = run(
        &ReferenceEffectState::Invert { amount: 1.0 },
        std::slice::from_ref(&source),
        region,
    );
    assert_eq!(pixels(&inverted)[0], [0.0, 1.0, 1.0, 1.0]);

    let graded = run(
        &ReferenceEffectState::Grade {
            gain: [2.0, 1.0, 1.0],
            offset: [0.1, 0.0, 0.0],
        },
        std::slice::from_ref(&source),
        region,
    );
    close(pixels(&graded)[0][0], 2.1);
    assert_eq!(
        graded.descriptor().color_tags(),
        source.descriptor().color_tags()
    );
    assert_eq!(
        graded.descriptor().channels(),
        source.descriptor().channels()
    );
    assert_eq!(graded.metadata(), source.metadata());
}

#[test]
fn blend_and_porter_duff_composite_use_premultiplied_algebra() {
    let source = image32(1, 1, &[[0.5, 0.0, 0.0, 0.5]]);
    let backdrop = image32(1, 1, &[[0.0, 0.0, 1.0, 1.0]]);
    let region = bounds(1, 1);

    let normal = run(
        &ReferenceEffectState::Blend {
            mode: ReferenceBlendMode::Normal,
            opacity: 1.0,
        },
        &[source.clone(), backdrop.clone()],
        region,
    );
    assert_eq!(pixels(&normal)[0], [0.5, 0.0, 0.5, 1.0]);

    let multiply = run(
        &ReferenceEffectState::Blend {
            mode: ReferenceBlendMode::Multiply,
            opacity: 1.0,
        },
        &[source.clone(), backdrop.clone()],
        region,
    );
    assert_eq!(pixels(&multiply)[0], [0.0, 0.0, 0.5, 1.0]);

    let source_in = run(
        &ReferenceEffectState::Composite {
            operator: ReferenceCompositeOperator::SourceIn,
            opacity: 1.0,
        },
        &[source.clone(), backdrop.clone()],
        region,
    );
    assert_eq!(pixels(&source_in)[0], [0.5, 0.0, 0.0, 0.5]);

    let xor = run(
        &ReferenceEffectState::Composite {
            operator: ReferenceCompositeOperator::Xor,
            opacity: 1.0,
        },
        &[source, backdrop],
        region,
    );
    assert_eq!(pixels(&xor)[0], [0.0, 0.0, 0.5, 0.5]);
}

#[test]
fn filters_distortion_and_keying_are_bounded_and_exercised() {
    let impulse = image32(
        3,
        1,
        &[
            [0.0, 0.0, 0.0, 0.0],
            [1.0, 1.0, 1.0, 1.0],
            [0.0, 0.0, 0.0, 0.0],
        ],
    );
    let region = bounds(3, 1);
    let blurred = run(
        &ReferenceEffectState::GaussianBlur { sigma: 0.5 },
        std::slice::from_ref(&impulse),
        region,
    );
    let blur = pixels(&blurred);
    assert!(blur[0][0] > 0.0);
    assert!(blur[1][0] > blur[0][0] && blur[1][0] < 1.0);

    let sharpened = run(
        &ReferenceEffectState::Sharpen {
            sigma: 0.5,
            amount: 1.0,
        },
        std::slice::from_ref(&impulse),
        region,
    );
    assert!(pixels(&sharpened)[1][0] > 1.0);

    let distorted = run(
        &ReferenceEffectState::RadialDistortion {
            center: [1.5, 0.5],
            radius: 1.0,
            k1: 0.0,
            k2: 0.0,
        },
        std::slice::from_ref(&impulse),
        region,
    );
    assert_eq!(pixels(&distorted), pixels(&impulse));

    let keyed_source = image32(2, 1, &[[0.0, 1.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0]]);
    let keyed = run(
        &ReferenceEffectState::ChromaKey {
            key_color: [0.0, 1.0, 0.0],
            tolerance: 0.05,
            softness: 0.05,
            spill: 1.0,
        },
        &[keyed_source],
        bounds(2, 1),
    );
    close(pixels(&keyed)[0][3], 0.0);
    close(pixels(&keyed)[1][3], 1.0);
}

#[test]
fn binary16_representation_and_metadata_are_preserved() {
    let source = image16(1, 1, &[[0.5, 0.25, -0.5, 1.0]]);
    let output = run(
        &ReferenceEffectState::Opacity { opacity: 0.5 },
        std::slice::from_ref(&source),
        bounds(1, 1),
    );
    assert_eq!(output.descriptor().pixel_format(), PixelFormat::Rgba16Float);
    assert_eq!(output.metadata(), source.metadata());
    close(pixels(&output)[0][0], 0.25);
    close(pixels(&output)[0][2], -0.25);

    let extreme = image16(1, 1, &[[65_504.0, 0.0, 0.0, 1.0]]);
    let error = evaluate_reference(
        &ReferenceEffectState::Grade {
            gain: [2.0, 1.0, 1.0],
            offset: [0.0; 3],
        },
        &[extreme],
        bounds(1, 1),
        &ImageLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
}

#[test]
fn roi_expansion_invalid_semantics_and_resource_limits_are_actionable() {
    let output = PixelBounds::new(10, 20, 20, 30).unwrap();
    assert_eq!(
        required_input_regions(&ReferenceEffectState::GaussianBlur { sigma: 1.0 }, output).unwrap(),
        [PixelBounds::new(7, 17, 23, 33).unwrap()]
    );
    let translated = ReferenceEffectState::Transform {
        matrix: Matrix3::translation(Vector2::new(2.0, 3.0).unwrap()),
        sampling: ReferenceSampling::Nearest,
    };
    assert_eq!(
        required_input_regions(&translated, PixelBounds::new(2, 3, 4, 5).unwrap()).unwrap(),
        [PixelBounds::new(0, 0, 2, 2).unwrap()]
    );

    let invalid_descriptor = ImageDescriptor::new(
        bounds(1, 1),
        bounds(1, 1),
        PixelFormat::Rgba8Unorm,
        ColorSpace::ACESCG,
        AlphaMode::Premultiplied,
    )
    .unwrap();
    let invalid = Image::new(invalid_descriptor, ImageSamples::from_u8([0, 0, 0, 255])).unwrap();
    let error = evaluate_reference(
        &ReferenceEffectState::Opacity { opacity: 1.0 },
        &[invalid],
        bounds(1, 1),
        &ImageLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);

    let source = image32(2, 1, &[[1.0, 0.0, 0.0, 1.0]; 2]);
    let limits = ImageLimits::new(1, 1, 1024).unwrap();
    let error = evaluate_reference(
        &ReferenceEffectState::Opacity { opacity: 1.0 },
        std::slice::from_ref(&source),
        bounds(2, 1),
        &limits,
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);

    let working_limits = ImageLimits::new(2, 1, 40).unwrap();
    let error = evaluate_reference(
        &ReferenceEffectState::Opacity { opacity: 1.0 },
        std::slice::from_ref(&source),
        bounds(2, 1),
        &working_limits,
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    assert!(error.contexts().iter().any(|context| {
        context.component() == "superi-effects.reference"
            && context.field("reason") == Some("working_memory_limit")
    }));

    let error = evaluate_reference(
        &ReferenceEffectState::GaussianBlur { sigma: -1.0 },
        std::slice::from_ref(&source),
        bounds(2, 1),
        &ImageLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let error = evaluate_reference(
        &ReferenceEffectState::RadialDistortion {
            center: [0.0, 0.0],
            radius: 1.0,
            k1: -1.0,
            k2: 0.0,
        },
        &[source],
        bounds(2, 1),
        &ImageLimits::default(),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
}
