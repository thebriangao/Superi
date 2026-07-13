use half::f16;
use superi_core::color_space::ColorSpace;
use superi_core::error::ErrorCategory;
use superi_core::geometry::PixelBounds;
use superi_core::pixel::{AlphaMode, ChannelLayout, PixelFormat};
use superi_core::time::SampleTime;
use superi_image::channels::ChannelList;
use superi_image::metadata::{ImageColorTags, ImageMetadata, ImageMetadataValue};
use superi_image::preview::{
    generate_thumbnail, render_waveform_image, ThumbnailRequest, WaveformEnvelope, WaveformImage,
    WaveformPeak, WaveformRasterStyle,
};
use superi_image::value::{Image, ImageDescriptor, ImageSamples};

#[test]
fn thumbnail_fits_the_display_and_preserves_the_complete_image_contract() {
    let data_window = bounds(-2, 1, 8, 4);
    let display_window = bounds(-4, -1, 16, 8);
    let channels =
        ChannelList::from_full_names(["beauty.R", "beauty.G", "beauty.B", "beauty.A"]).unwrap();
    let color_tags = ImageColorTags::new(ColorSpace::ACESCG)
        .with_named_space("ACEScg")
        .unwrap();
    let descriptor = ImageDescriptor::new_with_color_tags(
        data_window,
        display_window,
        PixelFormat::Rgba16Float,
        color_tags,
        AlphaMode::Premultiplied,
    )
    .unwrap()
    .with_channels(channels)
    .unwrap();
    let mut metadata = ImageMetadata::new();
    metadata
        .insert(
            "superi.test.identity",
            ImageMetadataValue::Text("thumbnail-source".into()),
        )
        .unwrap();
    let samples = (0..descriptor.required_sample_count().unwrap())
        .map(|index| f16::from_f32(index as f32 / 127.0).to_bits());
    let source =
        Image::new_with_metadata(descriptor, ImageSamples::from_f16_bits(samples), metadata)
            .unwrap();
    let original = source.clone();

    let thumbnail = generate_thumbnail(&source, ThumbnailRequest::new(8, 8).unwrap()).unwrap();

    assert_eq!(thumbnail.descriptor().display_window(), bounds(0, 0, 8, 4));
    assert_eq!(thumbnail.descriptor().data_window(), bounds(1, 1, 4, 2));
    assert_eq!(
        thumbnail.descriptor().pixel_format(),
        source.descriptor().pixel_format()
    );
    assert_eq!(
        thumbnail.descriptor().sample_type(),
        source.descriptor().sample_type()
    );
    assert_eq!(
        thumbnail.descriptor().channels(),
        source.descriptor().channels()
    );
    assert_eq!(
        thumbnail.descriptor().color_tags(),
        source.descriptor().color_tags()
    );
    assert_eq!(
        thumbnail.descriptor().alpha_mode(),
        source.descriptor().alpha_mode()
    );
    assert_eq!(thumbnail.metadata(), source.metadata());
    assert_eq!(source, original);
}

#[test]
fn thumbnail_does_not_upscale_and_filters_straight_alpha_without_color_fringes() {
    let descriptor = ImageDescriptor::new(
        bounds(0, 0, 2, 1),
        bounds(0, 0, 2, 1),
        PixelFormat::Rgba32Float,
        ColorSpace::SRGB,
        AlphaMode::Straight,
    )
    .unwrap();
    let source = Image::new(
        descriptor,
        ImageSamples::from_f32([1.0, 0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0]),
    )
    .unwrap();

    let small = generate_thumbnail(&source, ThumbnailRequest::new(1, 1).unwrap()).unwrap();
    let values: Vec<_> = small
        .samples()
        .f32_bits()
        .unwrap()
        .iter()
        .copied()
        .map(f32::from_bits)
        .collect();
    assert_close(&values, &[1.0, 0.0, 0.0, 0.5]);

    let large = generate_thumbnail(&source, ThumbnailRequest::new(64, 64).unwrap()).unwrap();
    assert_eq!(large.descriptor().display_window(), bounds(0, 0, 2, 1));
    assert_eq!(large.samples(), source.samples());
}

#[test]
fn waveform_raster_keeps_exact_timing_channel_order_and_separate_bands() {
    let start = SampleTime::new(-5, 48_000).unwrap();
    let envelope = WaveformEnvelope::new(
        start,
        5,
        ChannelLayout::stereo(),
        vec![
            vec![
                WaveformPeak::new(-1.0, 1.0).unwrap(),
                WaveformPeak::new(0.0, 0.0).unwrap(),
            ],
            vec![
                WaveformPeak::new(0.0, 0.0).unwrap(),
                WaveformPeak::new(-1.0, 1.0).unwrap(),
            ],
        ],
    )
    .unwrap();
    let style = WaveformRasterStyle::new(3, 1, [255, 255, 255, 255], [0, 0, 0, 0]).unwrap();

    let waveform = render_waveform_image(envelope, style).unwrap();

    assert_eq!(waveform.start(), start);
    assert_eq!(waveform.frame_count(), 5);
    assert_eq!(waveform.channel_layout(), &ChannelLayout::stereo());
    assert_eq!(
        waveform.image().descriptor().data_window(),
        bounds(0, 0, 2, 7)
    );
    assert_eq!(
        waveform.image().descriptor().display_window(),
        bounds(0, 0, 2, 7)
    );
    assert_eq!(
        waveform.image().descriptor().pixel_format(),
        PixelFormat::Rgba8Unorm
    );
    assert_eq!(
        waveform.image().descriptor().color_space(),
        ColorSpace::SRGB
    );
    assert_eq!(
        waveform.image().descriptor().alpha_mode(),
        AlphaMode::Straight
    );
    assert_eq!(
        waveform.source_range_for_column(0).unwrap(),
        (
            SampleTime::new(-5, 48_000).unwrap(),
            SampleTime::new(-3, 48_000).unwrap()
        )
    );
    assert_eq!(
        waveform.source_range_for_column(1).unwrap(),
        (
            SampleTime::new(-3, 48_000).unwrap(),
            SampleTime::new(0, 48_000).unwrap()
        )
    );
    assert!(waveform.source_range_for_column(2).is_none());
    assert_eq!(
        waveform.peak(0, 0),
        Some(WaveformPeak::new(-1.0, 1.0).unwrap())
    );
    assert_eq!(
        waveform.peak(0, 1),
        Some(WaveformPeak::new(0.0, 0.0).unwrap())
    );

    let alpha = waveform
        .image()
        .samples()
        .u8_values()
        .unwrap()
        .chunks_exact(4)
        .map(|pixel| pixel[3])
        .collect::<Vec<_>>();
    assert_eq!(
        alpha,
        [
            255, 0, // channel 0 top
            255, 255, // channel 0 center
            255, 0, // channel 0 bottom
            0, 0, // gap
            0, 255, // channel 1 top
            255, 255, // channel 1 center
            0, 255, // channel 1 bottom
        ]
    );
}

#[test]
fn preview_inputs_reject_empty_dimensions_and_malformed_envelopes() {
    assert_eq!(
        ThumbnailRequest::new(0, 10).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        WaveformRasterStyle::new(0, 0, [0; 4], [0; 4])
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        WaveformPeak::new(0.5, -0.5).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        WaveformPeak::new(f32::NAN, 1.0).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        WaveformEnvelope::new(
            SampleTime::new(0, 48_000).unwrap(),
            0,
            ChannelLayout::mono(),
            vec![]
        )
        .unwrap_err()
        .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        WaveformEnvelope::new(
            SampleTime::new(0, 48_000).unwrap(),
            1,
            ChannelLayout::stereo(),
            vec![vec![WaveformPeak::new(0.0, 0.0).unwrap()]]
        )
        .unwrap_err()
        .category(),
        ErrorCategory::InvalidInput
    );
}

#[test]
fn preview_contracts_are_safe_for_background_owners() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<ThumbnailRequest>();
    assert_send_sync::<WaveformPeak>();
    assert_send_sync::<WaveformEnvelope>();
    assert_send_sync::<WaveformRasterStyle>();
    assert_send_sync::<WaveformImage>();
}

fn bounds(min_x: i32, min_y: i32, width: u32, height: u32) -> PixelBounds {
    PixelBounds::from_origin_size(min_x, min_y, width, height).unwrap()
}

fn assert_close(actual: &[f32], expected: &[f32]) {
    assert_eq!(actual.len(), expected.len());
    for (actual, expected) in actual.iter().zip(expected) {
        assert!((actual - expected).abs() < 0.001, "{actual} != {expected}");
    }
}
