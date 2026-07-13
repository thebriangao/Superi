use std::sync::Arc;

use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::ErrorCategory;
use superi_core::geometry::AspectRatio;
use superi_core::time::FrameRate;
use superi_core::timecode::{Timecode, TimecodeFormat};
use superi_image::metadata::{
    ImageColorTags, ImageMetadata, ImageMetadataFloat, ImageMetadataValue, ImageOrientation,
};

#[test]
fn orientation_uses_exact_tiff_exif_values_and_explicit_display_semantics() {
    let cases = [
        (1, ImageOrientation::TopLeft, false),
        (2, ImageOrientation::TopRight, false),
        (3, ImageOrientation::BottomRight, false),
        (4, ImageOrientation::BottomLeft, false),
        (5, ImageOrientation::LeftTop, true),
        (6, ImageOrientation::RightTop, true),
        (7, ImageOrientation::RightBottom, true),
        (8, ImageOrientation::LeftBottom, true),
    ];

    for (exif_value, orientation, swaps_axes) in cases {
        assert_eq!(ImageOrientation::try_from(exif_value).unwrap(), orientation);
        assert_eq!(orientation.exif_value(), exif_value);
        assert_eq!(orientation.swaps_axes(), swaps_axes);
    }

    let error = ImageOrientation::try_from(0).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.contexts()[0].component(), "superi-image.metadata");
}

#[test]
fn typed_metadata_preserves_absence_exact_values_and_industry_defaults() {
    let mut metadata = ImageMetadata::new();
    assert_eq!(metadata.orientation(), None);
    assert_eq!(metadata.effective_orientation(), ImageOrientation::TopLeft);
    assert_eq!(metadata.pixel_aspect_ratio(), None);
    assert_eq!(
        metadata.effective_pixel_aspect_ratio(),
        AspectRatio::new(1, 1).unwrap()
    );
    assert_eq!(metadata.timecode(), None);

    assert_eq!(metadata.set_orientation(ImageOrientation::RightTop), None);
    assert_eq!(
        metadata.set_pixel_aspect_ratio(AspectRatio::new(40, 33).unwrap()),
        None
    );
    let format = TimecodeFormat::drop_frame(FrameRate::FPS_30000_1001).unwrap();
    let timecode = Timecode::parse("01:00:00;00", format).unwrap();
    assert_eq!(metadata.set_timecode(timecode), None);

    assert_eq!(metadata.orientation(), Some(ImageOrientation::RightTop));
    assert_eq!(
        metadata.pixel_aspect_ratio(),
        Some(AspectRatio::new(40, 33).unwrap())
    );
    assert_eq!(metadata.timecode(), Some(timecode));
    assert_eq!(metadata.timecode().unwrap().format().rate(), format.rate());
    assert_eq!(metadata.timecode().unwrap().format().mode(), format.mode());
    assert!(!metadata.is_empty());
    assert_eq!(
        metadata.clear_orientation(),
        Some(ImageOrientation::RightTop)
    );
    assert_eq!(metadata.orientation(), None);
}

#[test]
fn arbitrary_attributes_round_trip_exact_payloads_in_deterministic_order() {
    let nan_bits = 0x7ff8_0000_0000_1234;
    let mut metadata = ImageMetadata::new();
    metadata
        .insert(
            "source.vendor_blob",
            ImageMetadataValue::Bytes(Arc::from([0_u8, 255, 17])),
        )
        .unwrap();
    metadata
        .insert("source.validated", ImageMetadataValue::Boolean(true))
        .unwrap();
    metadata
        .insert(
            "exr.exposure",
            ImageMetadataValue::Float(ImageMetadataFloat::from_bits(nan_bits)),
        )
        .unwrap();
    metadata
        .insert("artist", ImageMetadataValue::Text("A camera".to_owned()))
        .unwrap();

    assert_eq!(
        metadata.iter().map(|(key, _)| key).collect::<Vec<_>>(),
        [
            "artist",
            "exr.exposure",
            "source.validated",
            "source.vendor_blob"
        ]
    );
    assert_eq!(
        metadata
            .get("exr.exposure")
            .and_then(ImageMetadataValue::as_f64)
            .unwrap()
            .to_bits(),
        nan_bits
    );
    assert_eq!(
        metadata.get("source.vendor_blob"),
        Some(&ImageMetadataValue::Bytes(Arc::from([0_u8, 255, 17])))
    );
}

#[test]
fn color_tags_keep_authoritative_axes_and_source_payloads_separate() {
    let interpretation = ColorSpace::new(
        ColorPrimaries::AcesAp1,
        TransferFunction::Linear,
        MatrixCoefficients::Rgb,
        ColorRange::Full,
    );
    let profile: Arc<[u8]> = Arc::from([0_u8, 1, 2, 255]);
    let tags = ImageColorTags::new(interpretation)
        .with_named_space("ACEScg")
        .unwrap()
        .with_icc_profile(profile.clone())
        .unwrap();

    assert_eq!(tags.interpretation(), interpretation);
    assert_eq!(tags.named_space(), Some("ACEScg"));
    assert_eq!(tags.icc_profile(), Some(profile.as_ref()));
    assert!(ImageColorTags::new(ColorSpace::UNSPECIFIED)
        .with_named_space("")
        .is_err());
    assert!(ImageColorTags::new(ColorSpace::UNSPECIFIED)
        .with_icc_profile(Arc::from([]))
        .is_err());
}

#[test]
fn typed_and_untyped_metadata_remain_safe_to_share_across_image_workers() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<ImageOrientation>();
    assert_send_sync::<ImageColorTags>();
    assert_send_sync::<ImageMetadata>();
    assert_send_sync::<ImageMetadataValue>();
}
