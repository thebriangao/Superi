use std::sync::Arc;

use superi_color::working_space::{WorkingImage, WorkingImageF32, WorkingSpace};
use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::ErrorCategory;
use superi_core::geometry::PixelBounds;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_gpu::wgpu::TextureFormat;
use superi_image::channels::ChannelList;
use superi_image::metadata::{ImageColorTags, ImageMetadata, ImageMetadataValue};
use superi_image::value::{Image, ImageDescriptor, ImageSamples};

#[test]
fn acescg_is_the_canonical_scene_linear_cpu_and_gpu_representation() {
    let space = WorkingSpace::default();
    assert_eq!(space, WorkingSpace::ACESCG);
    assert_eq!(space.color_space(), ColorSpace::ACESCG);

    let bounds = PixelBounds::from_origin_size(-2, 3, 4, 2).unwrap();
    let descriptor = space.image_descriptor(bounds, bounds).unwrap();
    assert_eq!(descriptor.pixel_format(), PixelFormat::Rgba16Float);
    assert_eq!(descriptor.color_space(), ColorSpace::ACESCG);
    assert_eq!(descriptor.alpha_mode(), AlphaMode::Premultiplied);

    let gpu = space.gpu_frame_descriptor(4, 2).unwrap();
    assert_eq!(gpu.pixel_format(), PixelFormat::Rgba16Float);
    assert_eq!(gpu.color_space(), ColorSpace::ACESCG);
    assert_eq!(gpu.alpha_mode(), AlphaMode::Premultiplied);
    assert_eq!(gpu.plane_layouts().len(), 1);
    assert_eq!(
        gpu.plane_layouts()[0].texture_format(),
        TextureFormat::Rgba16Float
    );
    space.validate_gpu_frame_descriptor(&gpu).unwrap();
}

#[test]
fn canonical_storage_retains_half_payloads_hdr_alpha_color_tags_and_metadata() {
    let bits = [
        0xb400, 0x4400, 0x3800, 0x3800, 0x8000, 0x7c00, 0x7e55, 0x3c00,
    ];
    let profile: Arc<[u8]> = Arc::from([1_u8, 2, 3, 255]);
    let tags = ImageColorTags::new(ColorSpace::ACESCG)
        .with_named_space("ACEScg")
        .unwrap()
        .with_icc_profile(profile)
        .unwrap();
    let metadata = ImageMetadata::new();
    let mut metadata = metadata;
    metadata
        .insert(
            "source.camera",
            ImageMetadataValue::Text("camera a".to_owned()),
        )
        .unwrap();
    let bounds = PixelBounds::from_origin_size(-1, 2, 2, 1).unwrap();
    let descriptor = ImageDescriptor::new_with_color_tags(
        bounds,
        bounds,
        PixelFormat::Rgba16Float,
        tags.clone(),
        AlphaMode::Premultiplied,
    )
    .unwrap();
    let image = Image::new_with_metadata(
        descriptor,
        ImageSamples::from_f16_bits(bits),
        metadata.clone(),
    )
    .unwrap();

    let working = WorkingImage::new(WorkingSpace::ACESCG, image).unwrap();
    assert_eq!(working.image().samples().f16_bits(), Some(bits.as_slice()));
    assert_eq!(working.image().descriptor().color_tags(), &tags);
    assert_eq!(working.image().metadata(), &metadata);
    assert_eq!(working.image().samples().float_value(0), Some(-0.25));
    assert_eq!(working.image().samples().float_value(1), Some(4.0));
    assert_eq!(
        working.image().samples().float_value(4).unwrap().to_bits(),
        (-0.0_f32).to_bits()
    );
}

#[test]
fn numerically_sensitive_work_is_explicitly_promoted_and_quantized() {
    let bits = [0xb400, 0x4000, 0x3800, 0x3800];
    let bounds = PixelBounds::from_origin_size(0, 0, 1, 1).unwrap();
    let image = Image::new(
        WorkingSpace::ACESCG
            .image_descriptor(bounds, bounds)
            .unwrap(),
        ImageSamples::from_f16_bits(bits),
    )
    .unwrap();
    let working = WorkingImage::acescg(image).unwrap();

    let promoted = working.promote_f32().unwrap();
    assert_eq!(promoted.space(), WorkingSpace::ACESCG);
    assert_eq!(
        promoted.image().descriptor().pixel_format(),
        PixelFormat::Rgba32Float
    );
    assert_eq!(promoted.image().samples().float_value(0), Some(-0.25_f32));
    assert_eq!(promoted.image().samples().float_value(1), Some(2.0_f32));
    assert_eq!(
        promoted.image().descriptor().color_tags(),
        working.image().descriptor().color_tags()
    );
    assert_eq!(promoted.image().metadata(), working.image().metadata());

    let quantized = promoted.quantize_f16().unwrap();
    assert_eq!(
        quantized.image().samples().f16_bits(),
        Some(bits.as_slice())
    );
    assert_eq!(
        quantized.image().descriptor().color_tags(),
        working.image().descriptor().color_tags()
    );
}

#[test]
fn noncanonical_storage_and_ambiguous_color_meaning_are_rejected() {
    let bounds = PixelBounds::from_origin_size(0, 0, 1, 1).unwrap();
    let cases = [
        image(
            bounds,
            PixelFormat::Rgba32Float,
            ColorSpace::ACESCG,
            AlphaMode::Premultiplied,
        ),
        image(
            bounds,
            PixelFormat::Rgba16Float,
            ColorSpace::ACESCG,
            AlphaMode::Straight,
        ),
        image(
            bounds,
            PixelFormat::Rgba16Float,
            ColorSpace::SRGB,
            AlphaMode::Premultiplied,
        ),
    ];
    for image in cases {
        let error = WorkingImage::new(WorkingSpace::ACESCG, image).unwrap_err();
        assert!(matches!(
            error.category(),
            ErrorCategory::InvalidInput | ErrorCategory::Unsupported
        ));
        assert_eq!(
            error.contexts()[0].component(),
            "superi-color.working-space"
        );
    }

    let custom_channels =
        ChannelList::from_full_names(["beauty.R", "beauty.G", "beauty.B", "beauty.A"]).unwrap();
    let image = Image::new(
        WorkingSpace::ACESCG
            .image_descriptor(bounds, bounds)
            .unwrap()
            .with_channels(custom_channels)
            .unwrap(),
        ImageSamples::from_f16_bits([0, 0, 0, 0]),
    )
    .unwrap();
    assert!(WorkingImage::new(WorkingSpace::ACESCG, image).is_err());

    let invalid_spaces = [
        ColorSpace::new(
            ColorPrimaries::AcesAp1,
            TransferFunction::Pq,
            MatrixCoefficients::Rgb,
            ColorRange::Full,
        ),
        ColorSpace::new(
            ColorPrimaries::AcesAp1,
            TransferFunction::Linear,
            MatrixCoefficients::Bt2020NonConstant,
            ColorRange::Full,
        ),
        ColorSpace::new(
            ColorPrimaries::AcesAp1,
            TransferFunction::Linear,
            MatrixCoefficients::Rgb,
            ColorRange::Limited,
        ),
        ColorSpace::new(
            ColorPrimaries::Unspecified,
            TransferFunction::Linear,
            MatrixCoefficients::Rgb,
            ColorRange::Full,
        ),
        ColorSpace::new(
            ColorPrimaries::Bt709,
            TransferFunction::Linear,
            MatrixCoefficients::Rgb,
            ColorRange::Full,
        ),
    ];
    for color_space in invalid_spaces {
        assert!(WorkingSpace::new(color_space).is_err());
    }

    let bt2020_linear = ColorSpace::new(
        ColorPrimaries::Bt2020,
        TransferFunction::Linear,
        MatrixCoefficients::Rgb,
        ColorRange::Full,
    );
    assert_eq!(
        WorkingSpace::new(bt2020_linear).unwrap().color_space(),
        bt2020_linear
    );
}

#[test]
fn f32_working_values_cannot_be_mislabeled_as_canonical_storage() {
    let bounds = PixelBounds::from_origin_size(0, 0, 1, 1).unwrap();
    let descriptor = ImageDescriptor::new(
        bounds,
        bounds,
        PixelFormat::Rgba32Float,
        ColorSpace::ACESCG,
        AlphaMode::Premultiplied,
    )
    .unwrap();
    let image = Image::new(descriptor, ImageSamples::from_f32([-0.25, 2.0, 0.5, 0.5])).unwrap();
    let promoted = WorkingImageF32::new(WorkingSpace::ACESCG, image).unwrap();
    assert_eq!(promoted.image().samples().float_value(1), Some(2.0));
    assert!(WorkingImage::new(WorkingSpace::ACESCG, promoted.into_image()).is_err());
}

#[test]
fn working_contracts_are_safe_to_share_between_engine_owners() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<WorkingSpace>();
    assert_send_sync::<WorkingImage>();
    assert_send_sync::<WorkingImageF32>();
}

fn image(
    bounds: PixelBounds,
    pixel_format: PixelFormat,
    color_space: ColorSpace,
    alpha_mode: AlphaMode,
) -> Image {
    let descriptor =
        ImageDescriptor::new(bounds, bounds, pixel_format, color_space, alpha_mode).unwrap();
    let samples = match pixel_format {
        PixelFormat::Rgba16Float => ImageSamples::from_f16_bits([0, 0, 0, 0]),
        PixelFormat::Rgba32Float => ImageSamples::from_f32([0.0, 0.0, 0.0, 0.0]),
        _ => unreachable!("test helper accepts only rgba float formats"),
    };
    Image::new(descriptor, samples).unwrap()
}
