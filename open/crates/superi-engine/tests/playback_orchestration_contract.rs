use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration as MonotonicDuration, Instant};

use superi_audio::playback::{create_output_buffer, OutputBufferConfig};
use superi_cache::eviction::{CacheBudgetLimit, CacheBudgetManager, CacheCost, CacheMemoryBudgets};
use superi_cache::frame::FrameMemoryCache;
use superi_cache::key::{MediaCacheIdentity, ParameterStateFingerprint, RenderSettingsFingerprint};
use superi_color::transform_out::{OutputColorTransform, OutputTargetKind, OutputTransformOptions};
use superi_color::working_space::{WorkingImage, WorkingSpace};
use superi_concurrency::backpressure::{
    bounded_handoff, BackpressureConfig, PipelineRoute, PipelineStage,
};
use superi_concurrency::clock::{AvFrameCorrection, AvPresentationReason, PlaybackClockMode};
use superi_concurrency::jobs::{BoundedWorkerPool, WorkerPoolConfig};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use superi_core::geometry::PixelBounds;
use superi_core::ids::{JobId, MediaId, ProjectId};
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_core::time::{Duration, RationalTime, Timebase};
use superi_engine::av_sync::AvSyncOutcome;
use superi_engine::playback::{
    CpuPlaybackDisplayTransform, GraphPlaybackFrameEvaluator, PlaybackAudioOutput,
    PlaybackCacheIdentity, PlaybackOrchestrator, PlaybackPoll, PlaybackSceneFrame,
};
use superi_graph::dag::GraphEndpoint;
use superi_graph::diagnostics::{IntrospectNode, NodeIntrospection, NodeStateFingerprint};
use superi_graph::eval::{EvaluateNode, EvaluationCacheEntryKind, EvaluationContext};
use superi_graph::expr::ExpressionParameterValue;
use superi_graph::headless::{GraphEvaluationSnapshot, NodeCompiler};
use superi_graph::ids::{GraphId, NodeId, PortId};
use superi_graph::mutate::{
    EditableGraph, EditableNode, GraphMutation, GraphSnapshot, GraphTransaction, InstancePort,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchema, NodeSchemaId,
    NodeTypeId, PortCardinality, PortName, PortSchema, RoiBehavior, TimeBehavior, ValueTypeId,
};
use superi_image::metadata::{
    ColorPipelineMetadata, ColorTransformStage, ColorTransformStageKind, ImageColorTags,
    ImageMetadata, ImageMetadataValue,
};
use superi_image::value::{Image, ImageDescriptor, ImageSamples};
use superi_media_io::decode::{CpuVideoBuffer, VideoFormat, VideoFrame, VideoPlane};
use superi_media_io::demux::MetadataValue;

#[derive(Clone, Debug, Eq, PartialEq)]
struct Scalar(i64);

impl ExpressionParameterValue for Scalar {
    fn to_expression_scalar(&self, _value_type: &ValueTypeId) -> Result<f64> {
        Ok(self.0 as f64)
    }

    fn from_expression_scalar(_value_type: &ValueTypeId, value: f64) -> Result<Self> {
        Ok(Self(value as i64))
    }
}

#[derive(Clone)]
struct SceneNode {
    frames: Arc<Vec<PlaybackSceneFrame<WorkingImage>>>,
    calls: Arc<AtomicUsize>,
}

impl EvaluateNode<PlaybackSceneFrame<WorkingImage>> for SceneNode {
    fn evaluate(
        &self,
        context: &EvaluationContext<'_, PlaybackSceneFrame<WorkingImage>>,
    ) -> Result<PlaybackSceneFrame<WorkingImage>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.frames
            .iter()
            .find(|frame| frame.timestamp() == context.request().frame())
            .cloned()
            .ok_or_else(|| {
                Error::new(
                    ErrorCategory::NotFound,
                    Recoverability::Degraded,
                    "test scene has no frame at the requested time",
                )
            })
    }
}

impl IntrospectNode for SceneNode {
    fn introspection(&self) -> NodeIntrospection {
        NodeIntrospection::new(
            scene_schema_id(),
            scene_behavior(),
            NodeStateFingerprint::from_canonical_bytes(b"playback-orchestration-scenes-v1"),
        )
    }
}

struct SceneCompiler {
    frames: Arc<Vec<PlaybackSceneFrame<WorkingImage>>>,
    calls: Arc<AtomicUsize>,
}

impl NodeCompiler<Scalar, SceneNode> for SceneCompiler {
    fn compile_node(
        &mut self,
        _snapshot: &GraphSnapshot<Scalar>,
        _node_id: NodeId,
        _node: &EditableNode<Scalar>,
    ) -> Result<SceneNode> {
        Ok(SceneNode {
            frames: self.frames.clone(),
            calls: self.calls.clone(),
        })
    }
}

#[test]
fn playback_is_one_coherent_bounded_engine_across_normal_degraded_and_recovery_paths() {
    let timebase = Timebase::integer(48_000).unwrap();
    let first_time = RationalTime::new(2_000, timebase);
    let bad_time = RationalTime::new(4_000, timebase);
    let bad_alpha_time = RationalTime::new(5_000, timebase);
    let recovered_time = RationalTime::new(6_000, timebase);
    let stressed_time = RationalTime::new(7_000, timebase);
    let discontinuity_time = RationalTime::new(600_000, timebase);
    let scene_pipeline = decoded_pipeline();
    let bad_pipeline = scene_pipeline
        .clone()
        .with_stage(stage(
            ColorTransformStageKind::Creative,
            "unexpected-grade",
            ColorSpace::ACESCG,
            ColorSpace::ACESCG,
        ))
        .unwrap();
    let frames = Arc::new(vec![
        scene_frame(first_time, scene_pipeline.clone()),
        scene_frame(bad_time, bad_pipeline),
        scene_frame_with_alpha(bad_alpha_time, scene_pipeline.clone(), AlphaMode::Straight),
        scene_frame(recovered_time, scene_pipeline.clone()),
        scene_frame(stressed_time, scene_pipeline.clone()),
        scene_frame(discontinuity_time, scene_pipeline.clone()),
    ]);
    let calls = Arc::new(AtomicUsize::new(0));
    let (snapshot, output) = evaluation_snapshot(frames, calls.clone());
    let retained = memory_cache();
    let identity = PlaybackCacheIdentity::new(
        ProjectId::from_raw(41),
        vec![MediaCacheIdentity::new(MediaId::from_raw(17), "source-fingerprint-v1").unwrap()],
        ParameterStateFingerprint::from_canonical_bytes(b"playback-parameters-v1"),
        scene_pipeline.clone(),
        RenderSettingsFingerprint::from_canonical_bytes(b"playback-display-v1"),
    );
    let display_stage = stage(
        ColorTransformStageKind::Display,
        "srgb-monitor",
        ColorSpace::ACESCG,
        ColorSpace::SRGB,
    );
    let output_transform = OutputColorTransform::new(
        OutputTargetKind::Display,
        WorkingSpace::ACESCG,
        ColorSpace::SRGB,
        OutputTransformOptions::new(),
    )
    .unwrap();
    let display =
        CpuPlaybackDisplayTransform::new(scene_pipeline.clone(), display_stage, output_transform)
            .unwrap();
    let evaluator = Arc::new(
        GraphPlaybackFrameEvaluator::new(
            snapshot,
            retained.clone(),
            identity,
            output,
            PixelBounds::from_origin_size(0, 0, 1, 1).unwrap(),
            display,
        )
        .unwrap(),
    );
    let (producer, mut consumer, _telemetry) = create_output_buffer(OutputBufferConfig {
        channels: 1,
        sample_rate: 48_000,
        capacity_frames: 16_384,
        initial_sample: 0,
    })
    .unwrap();
    let audio = PlaybackAudioOutput::new(producer, consumer.clock().clone());
    let route = PipelineRoute::new(PipelineStage::Graph, PipelineStage::Viewport).unwrap();
    let (viewport_sender, viewport_receiver) =
        bounded_handoff(BackpressureConfig::new(route, 1).unwrap());
    let pool = Arc::new(BoundedWorkerPool::new(WorkerPoolConfig::new(1, 8).unwrap()).unwrap());
    let mut playback = PlaybackOrchestrator::audio_master(
        &pool,
        evaluator,
        viewport_sender,
        audio,
        RationalTime::new(0, timebase),
    )
    .unwrap();

    let wrong_domain = playback
        .submit_frame(JobId::from_raw(900), first_time)
        .unwrap_err();
    assert_eq!(wrong_domain.category(), ErrorCategory::Conflict);

    let playback_domain = ExecutionDomain::Playback.enter_current().unwrap();
    playback
        .submit_frame(JobId::from_raw(901), first_time)
        .unwrap();
    match poll_ready(&mut playback, Instant::now()) {
        PlaybackPoll::Waiting {
            frame, clock, sync, ..
        } => {
            assert_eq!(frame, first_time);
            assert_eq!(clock, RationalTime::new(0, timebase));
            assert_eq!(sync.timing().presentation_time(), first_time);
            assert_eq!(
                sync.timing().duration(),
                Duration::new(1_000, timebase).unwrap()
            );
            assert!(sync.drift().nanoseconds() > 0);
            assert_eq!(sync.code(), "hold");
        }
        other => panic!("expected synchronized wait, received {other:?}"),
    }
    assert_eq!(
        playback.queue_audio(&vec![0.0; 2_000]).unwrap().frames,
        2_000
    );
    let mut first_audio = vec![1.0; 2_000];
    assert_eq!(consumer.render_f32(&mut first_audio).unwrap().frames, 2_000);
    match poll_ready(&mut playback, Instant::now()) {
        PlaybackPoll::Presented {
            frame,
            sync:
                AvSyncOutcome::Present {
                    correction,
                    reason,
                    recovery,
                    ..
                },
        } => {
            assert_eq!(frame, first_time);
            assert_eq!(correction, AvFrameCorrection::None);
            assert_eq!(reason, AvPresentationReason::InTolerance);
            assert_eq!(recovery, None);
        }
        other => panic!("expected synchronized presentation, received {other:?}"),
    }

    let first = viewport_receiver.try_receive().unwrap();
    assert_eq!(first.timestamp(), first_time);
    assert_eq!(first.duration(), Duration::new(1_000, timebase).unwrap());
    assert_eq!(first.decoded().len(), 1);
    assert_eq!(
        first.decoded()[0].format().pixel_format(),
        PixelFormat::Rgba16Unorm
    );
    assert_eq!(
        first.decoded()[0].format().alpha_mode(),
        AlphaMode::Straight
    );
    assert_eq!(
        first.decoded()[0].metadata().get("source.camera"),
        Some(&MetadataValue::Text("camera-a".to_owned()))
    );
    assert_eq!(first.decoded()[0].color_pipeline(), &scene_pipeline);
    assert_eq!(
        first.output().descriptor().pixel_format(),
        PixelFormat::Rgba32Float
    );
    assert_eq!(
        first.output().descriptor().alpha_mode(),
        AlphaMode::Premultiplied
    );
    assert_eq!(first.output().descriptor().color_space(), ColorSpace::SRGB);
    assert_eq!(
        first.output().metadata().get("source.camera"),
        Some(&ImageMetadataValue::Text("camera-a".to_owned()))
    );
    assert_eq!(
        first.color_metadata().pipeline().display_space(),
        Some(ColorSpace::SRGB)
    );
    assert_eq!(first.alpha_mode(), AlphaMode::Premultiplied);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(retained.final_frame_len(), 1);

    playback
        .submit_frame(JobId::from_raw(902), first_time)
        .unwrap();
    assert!(matches!(
        poll_ready(&mut playback, Instant::now()),
        PlaybackPoll::Presented { frame, .. } if frame == first_time
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    playback
        .submit_frame(JobId::from_raw(903), bad_time)
        .unwrap();
    match poll_ready(&mut playback, Instant::now()) {
        PlaybackPoll::Failed { frame, error, .. } => {
            assert_eq!(frame, bad_time);
            assert_eq!(error.category(), ErrorCategory::CorruptData);
            assert_eq!(error.recoverability(), Recoverability::Degraded);
        }
        other => panic!("expected degraded frame failure, received {other:?}"),
    }
    assert_eq!(retained.final_frame_len(), 1);

    playback
        .submit_frame(JobId::from_raw(905), bad_alpha_time)
        .unwrap();
    match poll_ready(&mut playback, Instant::now()) {
        PlaybackPoll::Failed { frame, error, .. } => {
            assert_eq!(frame, bad_alpha_time);
            assert_eq!(error.category(), ErrorCategory::CorruptData);
            assert_eq!(error.recoverability(), Recoverability::Degraded);
        }
        other => panic!("expected degraded alpha failure, received {other:?}"),
    }
    assert_eq!(retained.final_frame_len(), 1);

    playback
        .submit_frame(JobId::from_raw(906), recovered_time)
        .unwrap();
    assert_eq!(
        playback.queue_audio(&vec![0.0; 4_000]).unwrap().frames,
        4_000
    );
    let mut recovered_audio = vec![1.0; 4_000];
    assert_eq!(
        consumer.render_f32(&mut recovered_audio).unwrap().frames,
        4_000
    );
    let backpressured_sync = match poll_ready(&mut playback, Instant::now()) {
        PlaybackPoll::ViewportBackpressured { frame, sync } => {
            assert_eq!(frame, recovered_time);
            assert!(matches!(sync, AvSyncOutcome::Present { .. }));
            sync
        }
        other => panic!("expected synchronized viewport backpressure, received {other:?}"),
    };
    let observations_before_retry = playback.av_sync_statistics().observations();
    assert_eq!(
        viewport_receiver.try_receive().unwrap().timestamp(),
        first_time
    );
    match playback.poll_at(Instant::now()).unwrap() {
        PlaybackPoll::Presented { frame, sync } => {
            assert_eq!(frame, recovered_time);
            assert_eq!(sync, backpressured_sync);
        }
        other => panic!("expected retained synchronized presentation, received {other:?}"),
    }
    assert_eq!(
        playback.av_sync_statistics().observations(),
        observations_before_retry
    );
    assert_eq!(
        viewport_receiver.try_receive().unwrap().timestamp(),
        recovered_time
    );
    assert_eq!(calls.load(Ordering::SeqCst), 4);
    assert_eq!(retained.final_frame_len(), 2);

    playback
        .submit_frame(JobId::from_raw(907), stressed_time)
        .unwrap();
    assert_eq!(
        playback.queue_audio(&vec![0.0; 2_440]).unwrap().frames,
        2_440
    );
    let mut stressed_audio = vec![1.0; 2_440];
    assert_eq!(
        consumer.render_f32(&mut stressed_audio).unwrap().frames,
        2_440
    );
    match poll_ready(&mut playback, Instant::now()) {
        PlaybackPoll::Presented {
            frame,
            sync:
                AvSyncOutcome::Present {
                    display_for,
                    correction,
                    reason,
                    recovery,
                    ..
                },
        } => {
            assert_eq!(frame, stressed_time);
            assert_eq!(display_for, MonotonicDuration::from_nanos(833_333));
            assert_eq!(
                correction,
                AvFrameCorrection::ShortenBy(MonotonicDuration::from_millis(20))
            );
            assert_eq!(reason, AvPresentationReason::CorrectedLate);
            assert_eq!(recovery, None);
        }
        other => panic!("expected corrected stressed presentation, received {other:?}"),
    }
    let stressed = viewport_receiver.try_receive().unwrap();
    assert_eq!(stressed.timestamp(), stressed_time);
    assert_eq!(stressed.duration(), Duration::new(1_000, timebase).unwrap());

    playback
        .submit_frame(JobId::from_raw(908), discontinuity_time)
        .unwrap();
    match poll_ready(&mut playback, Instant::now()) {
        PlaybackPoll::Presented {
            frame,
            sync:
                AvSyncOutcome::Present {
                    correction,
                    reason,
                    recovery: Some(recovery),
                    ..
                },
        } => {
            assert_eq!(frame, discontinuity_time);
            assert_eq!(correction, AvFrameCorrection::None);
            assert_eq!(reason, AvPresentationReason::InTolerance);
            assert_eq!(recovery.target_master_time(), discontinuity_time);
            assert!(recovery.discontinuity().nanoseconds() > 10_000_000_000);
        }
        other => panic!("expected discontinuity recovery, received {other:?}"),
    }
    assert_eq!(
        viewport_receiver.try_receive().unwrap().timestamp(),
        discontinuity_time
    );
    assert_eq!(playback.av_sync_statistics().rebases(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 6);
    assert_eq!(retained.final_frame_len(), 4);

    assert_eq!(
        playback.queue_audio(&vec![0.0; 7_000]).unwrap().frames,
        7_000
    );
    let saturated = playback.queue_audio(&vec![0.0; 8_192]).unwrap_err();
    assert_eq!(saturated.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(saturated.recoverability(), Recoverability::Retryable);

    let fallback_at = Instant::now();
    let fallback_position = playback.fallback_to_playback_at(fallback_at).unwrap();
    assert_eq!(fallback_position, discontinuity_time);
    assert_eq!(playback.clock_mode(), PlaybackClockMode::Playback);
    assert_eq!(
        playback.clock_position_at(fallback_at).unwrap(),
        discontinuity_time
    );
    let recovery_at = fallback_at + MonotonicDuration::from_millis(10);
    let expected_recovery = RationalTime::new(600_480, timebase);
    assert_eq!(
        playback.recover_audio_master_at(recovery_at).unwrap(),
        expected_recovery
    );
    assert_eq!(playback.clock_mode(), PlaybackClockMode::AudioMaster);
    assert_eq!(
        playback.clock_position_at(recovery_at).unwrap(),
        expected_recovery
    );

    drop(playback_domain);
    drop(playback);
    Arc::try_unwrap(pool).ok().unwrap().shutdown().unwrap();
}

fn poll_ready(playback: &mut PlaybackOrchestrator<Image>, now: Instant) -> PlaybackPoll {
    for _ in 0..50_000 {
        let state = playback.poll_at(now).unwrap();
        if !matches!(state, PlaybackPoll::Rendering { .. }) {
            return state;
        }
        thread::yield_now();
    }
    panic!("playback orchestration did not leave rendering within the bounded poll loop");
}

fn scene_frame(
    timestamp: RationalTime,
    scene_pipeline: ColorPipelineMetadata,
) -> PlaybackSceneFrame<WorkingImage> {
    scene_frame_with_alpha(timestamp, scene_pipeline, AlphaMode::Premultiplied)
}

fn scene_frame_with_alpha(
    timestamp: RationalTime,
    scene_pipeline: ColorPipelineMetadata,
    alpha_mode: AlphaMode,
) -> PlaybackSceneFrame<WorkingImage> {
    let decoded = decoded_frame(timestamp);
    PlaybackSceneFrame::from_decoded(working_image(), &decoded, scene_pipeline, alpha_mode).unwrap()
}

fn decoded_frame(timestamp: RationalTime) -> VideoFrame {
    let format = VideoFormat::new(
        1,
        1,
        PixelFormat::Rgba16Unorm,
        ColorSpace::DISPLAY_P3,
        AlphaMode::Straight,
    )
    .unwrap();
    let plane = VideoPlane::new(Arc::from([0_u8; 8]), 8, 1).unwrap();
    let buffer =
        Arc::new(CpuVideoBuffer::new(1, 1, PixelFormat::Rgba16Unorm, vec![plane]).unwrap());
    VideoFrame::new(
        format,
        timestamp,
        Duration::new(1_000, timestamp.timebase()).unwrap(),
        buffer,
    )
    .unwrap()
    .with_metadata("source.camera", MetadataValue::Text("camera-a".to_owned()))
    .unwrap()
    .with_color_pipeline(decoded_pipeline())
    .unwrap()
}

fn decoded_pipeline() -> ColorPipelineMetadata {
    ColorPipelineMetadata::new(
        ImageColorTags::new(ColorSpace::DISPLAY_P3)
            .with_named_space("Display P3 source")
            .unwrap(),
    )
    .unwrap()
    .with_stage(stage(
        ColorTransformStageKind::Input,
        "display-p3-to-acescg",
        ColorSpace::DISPLAY_P3,
        ColorSpace::ACESCG,
    ))
    .unwrap()
}

fn working_image() -> WorkingImage {
    let bounds = PixelBounds::from_origin_size(0, 0, 1, 1).unwrap();
    let descriptor = ImageDescriptor::new_with_color_tags(
        bounds,
        bounds,
        PixelFormat::Rgba16Float,
        ImageColorTags::new(ColorSpace::ACESCG),
        AlphaMode::Premultiplied,
    )
    .unwrap();
    let mut metadata = ImageMetadata::new();
    metadata
        .insert(
            "source.camera",
            ImageMetadataValue::Text("camera-a".to_owned()),
        )
        .unwrap();
    WorkingImage::acescg(
        Image::new_with_metadata(
            descriptor,
            ImageSamples::f16_from_f32([0.1, 0.2, 0.3, 0.5]),
            metadata,
        )
        .unwrap(),
    )
    .unwrap()
}

fn memory_cache() -> Arc<FrameMemoryCache<PlaybackSceneFrame<WorkingImage>>> {
    let limit = CacheBudgetLimit::new(1_048_576, 128).unwrap();
    let budgets = CacheMemoryBudgets::new(limit, limit, limit).unwrap();
    Arc::new(FrameMemoryCache::new(
        CacheBudgetManager::new(budgets),
        |_kind: EvaluationCacheEntryKind, _value: &PlaybackSceneFrame<WorkingImage>| {
            CacheCost::new(256, 1).unwrap()
        },
    ))
}

fn evaluation_snapshot(
    frames: Arc<Vec<PlaybackSceneFrame<WorkingImage>>>,
    calls: Arc<AtomicUsize>,
) -> (
    Arc<GraphEvaluationSnapshot<Scalar, SceneNode>>,
    GraphEndpoint,
) {
    let value_type = ValueTypeId::new("superi.value.playback-scene-frame").unwrap();
    let port_name = PortName::new("frame").unwrap();
    let schema = Arc::new(
        NodeSchema::new(
            scene_schema_id(),
            [],
            [PortSchema::new(
                port_name.clone(),
                value_type,
                PortCardinality::Single,
            )],
            [],
            scene_behavior(),
            CapabilitySet::new([]),
        )
        .unwrap(),
    );
    let node = EditableNode::new(
        schema,
        [],
        [InstancePort::new(PortId::from_raw(101), port_name)],
        [],
    )
    .unwrap();
    let mut graph = EditableGraph::new(GraphId::from_raw(951));
    graph
        .apply(GraphTransaction::with_mutations(
            0,
            [GraphMutation::Add {
                node_id: NodeId::from_raw(1),
                node,
                position: 0,
            }],
        ))
        .unwrap();
    let output = GraphEndpoint::new(NodeId::from_raw(1), PortId::from_raw(101));
    (
        Arc::new(
            GraphEvaluationSnapshot::compile(graph.snapshot(), SceneCompiler { frames, calls })
                .unwrap(),
        ),
        output,
    )
}

fn scene_schema_id() -> NodeSchemaId {
    NodeSchemaId::new(
        NodeTypeId::new("superi.test.playback-scene-frame").unwrap(),
        SemanticVersion::from_str("1.0.0").unwrap(),
    )
}

fn scene_behavior() -> NodeBehavior {
    NodeBehavior::new(
        TimeBehavior::CurrentFrame,
        RoiBehavior::InputBounds,
        ColorRequirements::Exact(ColorSpace::ACESCG),
        Determinism::Deterministic,
        CachePolicy::PerRegion,
    )
}

fn stage(
    kind: ColorTransformStageKind,
    name: &str,
    source: ColorSpace,
    destination: ColorSpace,
) -> ColorTransformStage {
    ColorTransformStage::new(kind, name, source, destination).unwrap()
}
