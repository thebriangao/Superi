use std::io::Cursor;

use superi_core::color_space::ColorSpace;
use superi_core::error::ErrorCategory;
use superi_core::geometry::PixelBounds;
use superi_core::pixel::{AlphaMode, ChannelLayout, PixelFormat};
use superi_core::time::SampleTime;
use superi_image::alpha::AlphaTransform;
use superi_image::io::{read, ReadOptions, StillImageFormat};
use superi_image::limits::ImageLimits;
use superi_image::ops::crop_with_limits;
use superi_image::preview::{
    render_waveform_image_with_limits, WaveformEnvelope, WaveformPeak, WaveformRasterStyle,
};
use superi_image::value::{Image, ImageDescriptor, ImageSamples};

fn rgba_f32_image() -> Image {
    let bounds = PixelBounds::from_origin_size(-1, 2, 2, 2).unwrap();
    let descriptor = ImageDescriptor::new(
        bounds,
        PixelBounds::from_origin_size(-4, -3, 8, 7).unwrap(),
        PixelFormat::Rgba32Float,
        ColorSpace::ACESCG,
        AlphaMode::Straight,
    )
    .unwrap();
    let values = [
        1.5_f32, 0.25, -0.0, 0.5, -2.0, 3.0, 4.0, 1.0, 0.0, 1.0, 2.0, 0.25, 8.0, -4.0, 0.75, 0.0,
    ];
    Image::new(descriptor, ImageSamples::from_f32(values)).unwrap()
}

fn limits(max_memory_bytes: u64) -> ImageLimits {
    ImageLimits::new(64, 64, max_memory_bytes)
        .unwrap()
        .with_max_channels(8)
        .unwrap()
        .with_max_layers(4)
        .unwrap()
        .with_max_metadata_bytes(256)
        .unwrap()
        .with_max_tiles(256)
        .unwrap()
}

#[test]
fn finite_limits_are_configurable_and_reject_invalid_policies() {
    let policy = limits(1024);
    assert_eq!(policy.max_width(), 64);
    assert_eq!(policy.max_height(), 64);
    assert_eq!(policy.max_memory_bytes(), 1024);
    assert_eq!(policy.max_channels(), 8);
    assert_eq!(policy.max_layers(), 4);
    assert_eq!(policy.max_metadata_bytes(), 256);
    assert_eq!(policy.max_tiles(), 256);

    assert_eq!(
        ImageLimits::new(0, 1, 1).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        ImageLimits::new(1, 1, 1)
            .unwrap()
            .with_max_channels(0)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
}

#[test]
fn dense_and_alpha_operations_fail_before_exceeding_the_budget() {
    let source = rgba_f32_image();
    let bounds = source.descriptor().data_window();

    let error = crop_with_limits(&source, bounds, &limits(63)).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    let narrow = ImageLimits::new(1, 64, 64).unwrap();
    let error = crop_with_limits(&source, bounds, &narrow).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);

    let accepted = crop_with_limits(&source, bounds, &limits(64)).unwrap();
    assert_eq!(accepted.descriptor(), source.descriptor());
    assert_eq!(accepted.samples(), source.samples());
    assert_eq!(accepted.metadata(), source.metadata());

    let transform = AlphaTransform::new(
        source.descriptor().channels(),
        AlphaMode::Straight,
        AlphaMode::Premultiplied,
    )
    .unwrap();
    let error = transform
        .transform_image_with_limits(&source, &limits(63))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
}

#[test]
fn waveform_rasterization_obeys_the_same_memory_policy() {
    let envelope = || {
        WaveformEnvelope::new(
            SampleTime::new(0, 48_000).unwrap(),
            4,
            ChannelLayout::stereo(),
            vec![
                vec![
                    WaveformPeak::new(-1.0, 1.0).unwrap(),
                    WaveformPeak::new(-0.5, 0.5).unwrap(),
                ];
                4
            ],
        )
        .unwrap()
    };
    let style = WaveformRasterStyle::new(4, 0, [255, 255, 255, 255], [0, 0, 0, 0]).unwrap();
    let error = render_waveform_image_with_limits(envelope(), style, &limits(127)).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);

    let image = render_waveform_image_with_limits(envelope(), style, &limits(128)).unwrap();
    assert_eq!(
        image.image().descriptor().pixel_format(),
        PixelFormat::Rgba8Unorm
    );
    assert_eq!(image.image().descriptor().alpha_mode(), AlphaMode::Straight);
    assert_eq!(image.image().descriptor().color_space(), ColorSpace::SRGB);
}

#[test]
fn malformed_and_structure_heavy_inputs_return_errors_without_panicking() {
    let cases = [
        (StillImageFormat::Png, b"not a png".to_vec()),
        (StillImageFormat::Dpx, b"SDPX".to_vec()),
        (StillImageFormat::Exr, b"not an exr".to_vec()),
        (
            StillImageFormat::Exr,
            [20000630_u32.to_le_bytes(), 2_u32.to_le_bytes()].concat(),
        ),
    ];
    for (format, bytes) in cases {
        let outcome = std::panic::catch_unwind(|| {
            read(
                &mut Cursor::new(bytes),
                format,
                &ReadOptions::from_limits(limits(1024)),
            )
        });
        let error = outcome
            .expect("malformed image input must not panic")
            .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::CorruptData);
    }

    let mut exr = Vec::new();
    exr.extend_from_slice(&20000630_u32.to_le_bytes());
    exr.extend_from_slice(&2_u32.to_le_bytes());
    exr.extend_from_slice(b"channels\0chlist\0");
    exr.extend_from_slice(&i32::MAX.to_le_bytes());
    exr.extend(std::iter::repeat(0).take(512));
    let constrained = limits(1024).with_max_metadata_bytes(64).unwrap();
    let outcome = std::panic::catch_unwind(|| {
        read(
            &mut Cursor::new(exr),
            StillImageFormat::Exr,
            &ReadOptions::from_limits(constrained),
        )
    });
    let error = outcome
        .expect("oversized EXR metadata must not panic")
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);

    let mut oversized_dpx = vec![0_u8; 2048];
    oversized_dpx[..4].copy_from_slice(b"SDPX");
    oversized_dpx[772..776].copy_from_slice(&65_u32.to_be_bytes());
    oversized_dpx[776..780].copy_from_slice(&1_u32.to_be_bytes());
    let outcome = std::panic::catch_unwind(|| {
        read(
            &mut Cursor::new(oversized_dpx),
            StillImageFormat::Dpx,
            &ReadOptions::from_limits(limits(4096)),
        )
    });
    let error = outcome
        .expect("oversized DPX dimensions must not panic")
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
}
