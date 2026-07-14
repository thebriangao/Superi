use std::sync::Arc;

use superi_cache::frame::CachedFrameColorMetadata;
use superi_core::color_space::ColorSpace;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::time::{Duration, RationalTime, Timebase};
use superi_engine::render::{ExportColorMetadata, ViewportColorMetadata};
use superi_graph::node::GraphColorMetadata;
use superi_image::metadata::{
    ColorPipelineMetadata, ColorTransformStage, ColorTransformStageKind, ImageColorTags,
};
use superi_media_io::decode::{CpuVideoBuffer, VideoFormat, VideoFrame, VideoPlane};
use superi_timeline::model::TimelineColorMetadata;

#[test]
fn color_metadata_survives_media_timeline_graph_cache_viewport_and_export() {
    let source_tags = ImageColorTags::new(ColorSpace::DISPLAY_P3)
        .with_named_space("Display P3 source")
        .unwrap()
        .with_icc_profile(Arc::from(&b"source-icc-profile"[..]))
        .unwrap();
    let pipeline = ColorPipelineMetadata::new(source_tags.clone())
        .unwrap()
        .with_stage(stage(
            ColorTransformStageKind::Input,
            "display-p3-to-acescg",
            ColorSpace::DISPLAY_P3,
            ColorSpace::ACESCG,
        ))
        .unwrap();
    let frame = video_frame(ColorSpace::DISPLAY_P3)
        .with_color_pipeline(pipeline)
        .unwrap();

    let media_graph = GraphColorMetadata::new(frame.color_pipeline().clone());
    let timeline = TimelineColorMetadata::from_graph(media_graph);
    let graph = timeline
        .compile()
        .with_stage(stage(
            ColorTransformStageKind::Creative,
            "primary-grade-v3",
            ColorSpace::ACESCG,
            ColorSpace::ACESCG,
        ))
        .unwrap();
    let cached = CachedFrameColorMetadata::from_graph(&graph);

    assert!(cached.matches(graph.pipeline()));
    assert_eq!(cached.pipeline().source_tags(), &source_tags);
    assert_eq!(cached.pipeline().working_space(), Some(ColorSpace::ACESCG));
    assert_eq!(
        stage_names(cached.pipeline()),
        ["display-p3-to-acescg", "primary-grade-v3",]
    );

    let viewport = ViewportColorMetadata::from_cache(
        &cached,
        stage(
            ColorTransformStageKind::Display,
            "monitor-icc-view",
            ColorSpace::ACESCG,
            ColorSpace::SRGB,
        ),
    )
    .unwrap();
    let export = ExportColorMetadata::from_cache(
        &cached,
        stage(
            ColorTransformStageKind::Output,
            "bt2100-pq-delivery",
            ColorSpace::ACESCG,
            ColorSpace::BT2100_PQ,
        ),
    )
    .unwrap();

    assert_eq!(viewport.pipeline().source_tags(), &source_tags);
    assert_eq!(viewport.pipeline().display_space(), Some(ColorSpace::SRGB));
    assert_eq!(viewport.pipeline().delivery_space(), None);
    assert_eq!(viewport.pipeline().current_space(), ColorSpace::SRGB);
    assert_eq!(
        stage_names(viewport.pipeline()),
        [
            "display-p3-to-acescg",
            "primary-grade-v3",
            "monitor-icc-view",
        ]
    );

    assert_eq!(export.pipeline().source_tags(), &source_tags);
    assert_eq!(export.pipeline().display_space(), None);
    assert_eq!(
        export.pipeline().delivery_space(),
        Some(ColorSpace::BT2100_PQ)
    );
    assert_eq!(export.pipeline().current_space(), ColorSpace::BT2100_PQ);
    assert_eq!(
        stage_names(export.pipeline()),
        [
            "display-p3-to-acescg",
            "primary-grade-v3",
            "bt2100-pq-delivery",
        ]
    );

    assert_eq!(cached.pipeline().current_space(), ColorSpace::ACESCG);
    assert_eq!(cached.pipeline().display_space(), None);
    assert_eq!(cached.pipeline().delivery_space(), None);
}

#[test]
fn transform_order_source_mismatch_and_cache_collisions_are_rejected() {
    let base = ColorPipelineMetadata::new(ImageColorTags::new(ColorSpace::DISPLAY_P3))
        .unwrap()
        .with_stage(stage(
            ColorTransformStageKind::Input,
            "input",
            ColorSpace::DISPLAY_P3,
            ColorSpace::ACESCG,
        ))
        .unwrap();
    let graph = GraphColorMetadata::new(base.clone());
    let cached = CachedFrameColorMetadata::from_graph(&graph);
    let changed = base
        .clone()
        .with_stage(stage(
            ColorTransformStageKind::Creative,
            "different-grade",
            ColorSpace::ACESCG,
            ColorSpace::ACESCG,
        ))
        .unwrap();

    assert!(!cached.matches(&changed));
    assert!(base
        .clone()
        .with_stage(stage(
            ColorTransformStageKind::Creative,
            "wrong-source",
            ColorSpace::SRGB,
            ColorSpace::ACESCG,
        ))
        .is_err());
    assert!(base
        .with_stage(stage(
            ColorTransformStageKind::Input,
            "late-input",
            ColorSpace::ACESCG,
            ColorSpace::ACESCG,
        ))
        .is_err());

    let display = cached
        .pipeline()
        .clone()
        .with_stage(stage(
            ColorTransformStageKind::Display,
            "display",
            ColorSpace::ACESCG,
            ColorSpace::SRGB,
        ))
        .unwrap();
    assert!(display
        .with_stage(stage(
            ColorTransformStageKind::Output,
            "output-after-display",
            ColorSpace::SRGB,
            ColorSpace::BT2100_PQ,
        ))
        .is_err());
    assert!(ViewportColorMetadata::from_cache(
        &cached,
        stage(
            ColorTransformStageKind::Creative,
            "not-a-display",
            ColorSpace::ACESCG,
            ColorSpace::ACESCG,
        ),
    )
    .is_err());
    assert!(ColorTransformStage::new(
        ColorTransformStageKind::Creative,
        "",
        ColorSpace::ACESCG,
        ColorSpace::ACESCG,
    )
    .is_err());

    let wrong_frame = video_frame(ColorSpace::SRGB);
    let display_p3 =
        ColorPipelineMetadata::new(ImageColorTags::new(ColorSpace::DISPLAY_P3)).unwrap();
    assert!(wrong_frame.with_color_pipeline(display_p3).is_err());
}

fn stage(
    kind: ColorTransformStageKind,
    name: &str,
    source: ColorSpace,
    destination: ColorSpace,
) -> ColorTransformStage {
    ColorTransformStage::new(kind, name, source, destination).unwrap()
}

fn stage_names(metadata: &ColorPipelineMetadata) -> Vec<&str> {
    metadata
        .stages()
        .iter()
        .map(ColorTransformStage::transform_id)
        .collect()
}

fn video_frame(color_space: ColorSpace) -> VideoFrame {
    let format = VideoFormat::new(
        1,
        1,
        PixelFormat::Rgba8Unorm,
        color_space,
        AlphaMode::Straight,
    )
    .unwrap();
    let plane = VideoPlane::new(Arc::from(&[10_u8, 20, 30, 255][..]), 4, 1).unwrap();
    let buffer = Arc::new(CpuVideoBuffer::new(1, 1, PixelFormat::Rgba8Unorm, vec![plane]).unwrap());
    let timebase = Timebase::integer(24).unwrap();
    VideoFrame::new(
        format,
        RationalTime::new(0, timebase),
        Duration::new(1, timebase).unwrap(),
        buffer,
    )
    .unwrap()
}
