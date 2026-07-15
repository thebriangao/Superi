use std::sync::Arc;

use superi_cache::key::MediaCacheIdentity;
use superi_cache::proxy::{
    DerivedMediaCatalog, DerivedMediaLookup, DerivedMediaPurpose, DerivedMediaQuality,
    DerivedMediaRequest,
};
use superi_core::color_space::ColorSpace;
use superi_core::error::ErrorCategory;
use superi_core::ids::MediaId;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::time::{Duration, RationalTime, Timebase};
use superi_engine::derived_media::{
    derived_media_render_settings, generate_derived_media, EncodedDerivedMedia,
};
use superi_engine::media::media_backend_registry;
use superi_media_io::decode::{CpuVideoBuffer, VideoFormat, VideoFrame, VideoPlane};
use superi_media_io::demux::{CodecId, MetadataValue, StreamId};
use superi_media_io::encode::{EncodeInput, EncoderConfig};
use superi_media_io::operation::{CancellationToken, MediaPriority, OperationContext};

const STREAM_ID: StreamId = StreamId::new(31);
const WIDTH: u32 = 16;
const HEIGHT: u32 = 16;

fn plane(value: u8, stride: usize, rows: u32) -> VideoPlane {
    VideoPlane::new(Arc::from(vec![value; stride * rows as usize]), stride, rows).unwrap()
}

fn frame(timestamp: i64, luma: u8) -> VideoFrame {
    let timebase = Timebase::integer(24).unwrap();
    let format = VideoFormat::new(
        WIDTH,
        HEIGHT,
        PixelFormat::Yuv420p8,
        ColorSpace::BT709,
        AlphaMode::Opaque,
    )
    .unwrap();
    let buffer = CpuVideoBuffer::new(
        WIDTH,
        HEIGHT,
        PixelFormat::Yuv420p8,
        vec![
            plane(luma, WIDTH as usize, HEIGHT),
            plane(96, WIDTH.div_ceil(2) as usize, HEIGHT.div_ceil(2)),
            plane(160, WIDTH.div_ceil(2) as usize, HEIGHT.div_ceil(2)),
        ],
    )
    .unwrap();
    VideoFrame::new(
        format,
        RationalTime::new(timestamp, timebase),
        Duration::new(1, timebase).unwrap(),
        Arc::new(buffer),
    )
    .unwrap()
    .with_metadata(
        "source.marker",
        MetadataValue::Text(format!("frame-{timestamp}")),
    )
    .unwrap()
}

fn config() -> EncoderConfig {
    EncoderConfig::video(
        STREAM_ID,
        CodecId::new("av1").unwrap(),
        Timebase::integer(24).unwrap(),
        frame(0, 32).format(),
    )
}

fn make_request(
    source_revision: u64,
    purpose: DerivedMediaPurpose,
    quality: DerivedMediaQuality,
    config: &EncoderConfig,
) -> DerivedMediaRequest {
    DerivedMediaRequest::new(
        MediaCacheIdentity::new(MediaId::from_raw(51), "sha256:camera-source").unwrap(),
        source_revision,
        purpose,
        quality,
        derived_media_render_settings(config, purpose, quality).unwrap(),
    )
}

fn inputs(first_luma: u8) -> Vec<EncodeInput> {
    vec![
        EncodeInput::Video(frame(0, first_luma)),
        EncodeInput::Video(frame(1, first_luma.saturating_add(16))),
    ]
}

#[test]
fn real_av1_generation_publishes_complete_deterministic_and_reusable_media() {
    let registry = media_backend_registry().unwrap();
    let config = config();
    let request = make_request(
        7,
        DerivedMediaPurpose::Proxy,
        DerivedMediaQuality::Quarter,
        &config,
    );
    let operation = OperationContext::new(MediaPriority::Background);
    let mut catalog = DerivedMediaCatalog::<EncodedDerivedMedia>::new();

    let first = generate_derived_media(
        &mut catalog,
        request,
        &registry,
        config.clone(),
        inputs(32),
        &operation,
    )
    .unwrap();

    assert!(!first.payload().packets().is_empty());
    assert_eq!(first.byte_len(), first.payload().encoded_byte_len());
    assert_eq!(first.request(), request);
    assert!(first.is_fresh(request.media(), 7));
    assert_eq!(first.payload().config(), &config);
    assert_eq!(
        first
            .payload()
            .packets()
            .iter()
            .map(|packet| packet.timing().presentation_time().unwrap().value())
            .collect::<Vec<_>>(),
        [0, 1]
    );
    assert_eq!(
        first.payload().packets()[0].metadata().get("source.marker"),
        Some(&MetadataValue::Text("frame-0".to_owned()))
    );

    let reused = generate_derived_media(
        &mut catalog,
        request,
        &registry,
        config.clone(),
        inputs(224),
        &operation,
    )
    .unwrap();
    assert!(Arc::ptr_eq(&first, &reused));

    let cancelled_token = CancellationToken::new();
    cancelled_token.cancel();
    let cancelled =
        OperationContext::new(MediaPriority::Background).with_cancellation(cancelled_token);
    let cancelled_hit = generate_derived_media(
        &mut catalog,
        request,
        &registry,
        config.clone(),
        inputs(32),
        &cancelled,
    )
    .unwrap_err();
    assert_eq!(cancelled_hit.category(), ErrorCategory::Cancelled);

    let mut second_catalog = DerivedMediaCatalog::<EncodedDerivedMedia>::new();
    let deterministic = generate_derived_media(
        &mut second_catalog,
        request,
        &registry,
        config.clone(),
        inputs(32),
        &operation,
    )
    .unwrap();
    assert_eq!(first.cache_id(), deterministic.cache_id());
    assert_eq!(
        first.content_fingerprint(),
        deterministic.content_fingerprint()
    );
    assert_eq!(first.payload().packets(), deterministic.payload().packets());

    let optimized = make_request(
        7,
        DerivedMediaPurpose::Optimized,
        DerivedMediaQuality::Full,
        &config,
    );
    let optimized_media = generate_derived_media(
        &mut catalog,
        optimized,
        &registry,
        config.clone(),
        inputs(32),
        &operation,
    )
    .unwrap();
    assert_ne!(request.key(), optimized.key());
    assert_ne!(request.render_settings(), optimized.render_settings());
    assert_eq!(
        optimized_media.request().purpose(),
        DerivedMediaPurpose::Optimized
    );
    assert_eq!(catalog.len(), 2);
}

#[test]
fn mismatch_or_cancellation_leaves_the_original_source_as_fallback() {
    let registry = media_backend_registry().unwrap();
    let config = config();
    let request = make_request(
        9,
        DerivedMediaPurpose::Proxy,
        DerivedMediaQuality::Half,
        &config,
    );
    let mut catalog = DerivedMediaCatalog::<EncodedDerivedMedia>::new();

    let mismatched = DerivedMediaRequest::new(
        request.media(),
        request.source_revision(),
        request.purpose(),
        request.quality(),
        derived_media_render_settings(&config, request.purpose(), DerivedMediaQuality::Quarter)
            .unwrap(),
    );
    let mismatch = generate_derived_media(
        &mut catalog,
        mismatched,
        &registry,
        config.clone(),
        inputs(48),
        &OperationContext::new(MediaPriority::Background),
    )
    .unwrap_err();
    assert_eq!(mismatch.category(), ErrorCategory::Conflict);
    assert!(catalog.is_empty());

    let cancellation = CancellationToken::new();
    cancellation.cancel();
    let cancelled =
        OperationContext::new(MediaPriority::Background).with_cancellation(cancellation);
    let failure = generate_derived_media(
        &mut catalog,
        request,
        &registry,
        config,
        inputs(48),
        &cancelled,
    )
    .unwrap_err();
    assert_eq!(failure.category(), ErrorCategory::Cancelled);
    assert!(catalog.is_empty());
    match catalog.lookup(request) {
        DerivedMediaLookup::OriginalSource(identity) => assert_eq!(identity, request.media()),
        DerivedMediaLookup::Generated(_) => panic!("cancelled generation published media"),
    }
}
