use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use superi_audio::graph::AudioGraphId;
use superi_concurrency::lifecycle::LifecyclePhase;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::PixelBounds;
use superi_core::ids::{ClipId, JobId, MediaId, ProjectId, TimelineId, TrackId};
use superi_core::pixel::{AlphaMode, ChannelLayout, ChannelPosition, PixelFormat, SampleFormat};
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_core::time::{
    Duration, FrameRate, RationalTime, SampleTime, TimeRange, TimeRounding, Timebase,
};
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult, EngineEvent,
    EngineTransactionId,
};
use superi_engine::export_dispatch::EngineExportJobCommand;
use superi_engine::export_jobs::{
    ExportJobExecutionContext, ExportJobQueueConfig, ExportJobStatus,
};
use superi_engine::export_queue::{
    render_and_export, render_and_export_tracked, ExportAudioGraph, ExportInputProvenance,
    ExportMediaSession, ExportStreamRoute, ExportVideoDelivery, HeadlessGraphVideoRenderer,
    RenderExportRequest, RenderExportStages, RenderedExportAudio, SharedExportDecodedFrame,
};
use superi_engine::lifecycle::{
    EngineLifecycle, EngineLifecycleActionKind, EngineSubsystem, EngineWorkKind, EngineWorkPermit,
};
use superi_engine::media::media_backend_registry;
use superi_engine::playback::PlaybackSceneFrame;
use superi_engine::resources::{
    acquire_timeline_resources, DecoderResourceRequest, MediaResourceRequest,
    ResourceAcquisitionPolicy,
};
use superi_graph::dag::GraphEndpoint;
use superi_graph::eval::{EvaluateNode, EvaluationContext};
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
};
use superi_media_io::audio_io::{AudioBlock, AudioFormat, AudioPlane};
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendDescriptor, BackendRegistration,
    BackendRegistry, BackendTier, FallbackPolicy, MediaBackend,
};
use superi_media_io::decode::{
    CpuVideoBuffer, DecodeOutput, Decoder, DecoderConfig, VideoFormat, VideoFrame, VideoPlane,
};
use superi_media_io::demux::{
    BackendId, CodecId, MediaSource, MetadataValue, Packet, PacketTiming, SeekRequest,
    SourceIdentity, SourceInfo, SourceLocation, SourceProbe, SourceProbeResult, SourceRequest,
    StreamId, StreamInfo, StreamKind,
};
use superi_media_io::encode::{
    EncodeInput, EncodeOutput, Encoder, EncoderConfig, EncoderMediaFormat,
};
use superi_media_io::operation::{MediaPriority, OperationContext};
use superi_media_io::read::{CorruptionReport, ReadOutcome};
use superi_timeline::model::{
    AudioChannelRoute, AudioChannelTarget, AudioRouteDestination, AudioRouting,
    AudioTrackSemantics, Clip, ClipSource, EditorialProject, LinkedMediaReference, Timeline, Track,
    TrackItem, TrackSemantics, VideoCompositing, VideoTrackSemantics,
};

const VIDEO_SOURCE: StreamId = StreamId::new(11);
const AUDIO_SOURCE: StreamId = StreamId::new(12);
const VIDEO_OUTPUT: StreamId = StreamId::new(21);
const AUDIO_OUTPUT: StreamId = StreamId::new(22);
const FIXTURE_MEDIA: MediaId = MediaId::from_raw(0x100);
const FIXTURE_TIMELINE: TimelineId = TimelineId::from_raw(0x200);
const FIXTURE_STREAM: StreamId = StreamId::new(1);
const FIXTURE_FINGERPRINT: &str =
    "sha256:117f5cebcaaf788d1891e84aec57066c73e33d4af308368f640f28a8419f4bbc";
const PCM_MEDIA: MediaId = MediaId::from_raw(0x601);
const PCM_TIMELINE: TimelineId = TimelineId::from_raw(0x602);
const PCM_STREAM: StreamId = StreamId::new(0);

#[test]
fn paired_export_uses_one_graph_engine_and_preserves_exact_media_semantics() {
    let _domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let engine = running_engine();
    let snapshot = engine.snapshot().unwrap();
    let permit = snapshot.admit(EngineWorkKind::Export).permit().unwrap();

    let decoded_video = video_frame(0);
    let decoded_audio = audio_block(0);
    let scene_pipeline = scene_pipeline();
    let decoded_binding = SharedExportDecodedFrame::new();
    let graph_calls = Arc::new(AtomicUsize::new(0));
    let (graph_snapshot, output) = graph_snapshot(
        decoded_binding.clone(),
        scene_pipeline.clone(),
        AlphaMode::Premultiplied,
        graph_calls.clone(),
    );
    let mut video_graph = HeadlessGraphVideoRenderer::new(
        graph_snapshot,
        output,
        PixelBounds::from_origin_size(0, 0, 1, 1).unwrap(),
        scene_pipeline.clone(),
        AlphaMode::Premultiplied,
        decoded_binding,
    )
    .unwrap();
    let delivery_calls = Arc::new(AtomicUsize::new(0));
    let mut delivery = TestDelivery::new(scene_pipeline.clone(), delivery_calls.clone());
    let audio_calls = Arc::new(AtomicUsize::new(0));
    let mut audio_graph = TestAudioGraph {
        id: AudioGraphId::from_raw(77),
        calls: audio_calls.clone(),
    };

    let video_config = video_encoder_config();
    let audio_config = audio_encoder_config();
    let request = RenderExportRequest::new(
        RationalTime::zero(Timebase::integer(24).unwrap()),
        vec![
            ExportStreamRoute::new(VIDEO_SOURCE, video_config.clone()),
            ExportStreamRoute::new(AUDIO_SOURCE, audio_config.clone()),
        ],
        FallbackPolicy::AllowRegistered,
        permit,
    )
    .unwrap();
    let resets = Arc::new(AtomicUsize::new(0));
    let attempts = Arc::new(AtomicUsize::new(0));
    let mut registry = BackendRegistry::new();
    registry
        .register(
            BackendRegistration::new(
                Arc::new(TestBackend::new(
                    "registered-fallback",
                    false,
                    attempts.clone(),
                    resets.clone(),
                )),
                encode_capabilities(),
                10,
                BackendTier::Fallback,
            )
            .unwrap(),
        )
        .unwrap();
    let mut session = TestSession::paired(decoded_video, decoded_audio, false);
    let lifecycle = || Ok(snapshot.value().clone());
    let artifact = render_and_export(
        &request,
        lifecycle,
        &mut session,
        &registry,
        RenderExportStages::av(&mut video_graph, &mut delivery, &mut audio_graph),
        &OperationContext::new(MediaPriority::Export),
    )
    .unwrap();

    assert_eq!(
        artifact.source_identity().media_id(),
        MediaId::from_raw(501)
    );
    assert_eq!(artifact.streams().len(), 2);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(resets.load(Ordering::SeqCst), 4);
    assert_eq!(session.reset_calls, 4);
    assert_eq!(graph_calls.load(Ordering::SeqCst), 1);
    assert_eq!(delivery_calls.load(Ordering::SeqCst), 1);
    assert_eq!(audio_calls.load(Ordering::SeqCst), 1);

    let video = &artifact.streams()[0];
    assert_eq!(video.config(), &video_config);
    assert_eq!(
        video.selection().selected_backend_id().as_str(),
        "registered-fallback"
    );
    assert!(video.selection().fallback_used());
    assert!(video.selection().fallback_backend_ids().is_empty());
    assert_eq!(video.packets().len(), 1);
    assert_eq!(
        video.packets()[0].metadata().get("source.camera"),
        Some(&MetadataValue::Text("camera-a".to_owned()))
    );
    let ExportInputProvenance::Video(video_input) = &video.inputs()[0] else {
        panic!("video route did not retain video provenance")
    };
    assert_eq!(
        video_input.timestamp(),
        RationalTime::new(0, Timebase::integer(24).unwrap())
    );
    assert_eq!(
        video_input.duration(),
        Duration::new(1, Timebase::integer(24).unwrap()).unwrap()
    );
    assert_eq!(video_input.decoded().len(), 1);
    assert_eq!(
        video_input.output_format().pixel_format(),
        PixelFormat::Rgba16Float
    );
    assert_eq!(video_input.output_format().color_space(), ColorSpace::BT709);
    assert_eq!(video_input.scene_alpha_mode(), AlphaMode::Premultiplied);
    assert_eq!(video_input.output_alpha_mode(), AlphaMode::Premultiplied);
    assert_eq!(
        video_input.color_pipeline().delivery_space(),
        Some(ColorSpace::BT709)
    );

    let audio = &artifact.streams()[1];
    assert_eq!(audio.config(), &audio_config);
    assert!(audio.selection().fallback_used());
    assert_eq!(audio.packets().len(), 1);
    assert_eq!(
        audio.packets()[0].metadata().get("source.microphone"),
        Some(&MetadataValue::Text("boom-a".to_owned()))
    );
    let ExportInputProvenance::Audio(audio_input) = &audio.inputs()[0] else {
        panic!("audio route did not retain audio provenance")
    };
    assert_eq!(audio_input.timestamp(), SampleTime::new(0, 48_000).unwrap());
    assert_eq!(
        audio_input.duration(),
        Duration::new(4, Timebase::integer(48_000).unwrap()).unwrap()
    );
    assert_eq!(audio_input.graph_id(), AudioGraphId::from_raw(77));
    assert_eq!(
        audio_input.output_format().sample_format(),
        SampleFormat::F32
    );
}

#[test]
fn queued_export_runs_the_real_transaction_and_retains_only_complete_tracked_results() {
    let domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let mut dispatcher = EngineCommandDispatcher::new().unwrap();
    let runtime = dispatcher
        .attach_export_jobs::<superi_engine::export_queue::RenderExportArtifact>(
            ExportJobQueueConfig::new(1, 2, 4).unwrap(),
        )
        .unwrap();
    let mut lifecycle_snapshot = match dispatch_engine(
        &mut dispatcher,
        "inspect-render-export-startup",
        EngineCommand::InspectLifecycle,
    )
    .result()
    {
        EngineCommandResult::Lifecycle(snapshot) => snapshot.as_ref().clone(),
        _ => panic!("expected lifecycle state"),
    };
    while lifecycle_snapshot.phase() == LifecyclePhase::Starting {
        let action = lifecycle_snapshot.pending_action().unwrap();
        lifecycle_snapshot = match dispatch_engine(
            &mut dispatcher,
            &format!("initialize-render-export-{}", action.subsystem().code()),
            EngineCommand::CompleteLifecycleAction(action),
        )
        .result()
        {
            EngineCommandResult::Lifecycle(snapshot) => snapshot.as_ref().clone(),
            _ => panic!("expected lifecycle state"),
        };
    }
    dispatcher.drain_events().unwrap();

    let attempts = Arc::new(AtomicUsize::new(0));
    let resets = Arc::new(AtomicUsize::new(0));
    let attempts_in_job = attempts.clone();
    let resets_in_job = resets.clone();
    let job_id = JobId::from_raw(0x800);

    runtime
        .prepare_executor(
            job_id,
            move |context: &ExportJobExecutionContext, permit: EngineWorkPermit| {
                let request = RenderExportRequest::new(
                    RationalTime::zero(Timebase::integer(24).unwrap()),
                    vec![
                        ExportStreamRoute::new(VIDEO_SOURCE, video_encoder_config()),
                        ExportStreamRoute::new(AUDIO_SOURCE, audio_encoder_config()),
                    ],
                    FallbackPolicy::Disallow,
                    permit,
                )?;
                let decoded_binding = SharedExportDecodedFrame::new();
                let scene_pipeline = scene_pipeline();
                let (graph_snapshot, output) = graph_snapshot(
                    decoded_binding.clone(),
                    scene_pipeline.clone(),
                    AlphaMode::Premultiplied,
                    Arc::new(AtomicUsize::new(0)),
                );
                let mut video_graph = HeadlessGraphVideoRenderer::new(
                    graph_snapshot,
                    output,
                    PixelBounds::from_origin_size(0, 0, 1, 1).unwrap(),
                    scene_pipeline.clone(),
                    AlphaMode::Premultiplied,
                    decoded_binding,
                )?;
                let mut delivery = TestDelivery::new(scene_pipeline, Arc::new(AtomicUsize::new(0)));
                let mut audio_graph = TestAudioGraph {
                    id: AudioGraphId::from_raw(78),
                    calls: Arc::new(AtomicUsize::new(0)),
                };
                let mut session = TestSession::paired(video_frame(0), audio_block(0), false);
                let registry = primary_registry(attempts_in_job.clone(), resets_in_job.clone());
                render_and_export_tracked(
                    &request,
                    || Ok(lifecycle_snapshot.clone()),
                    &mut session,
                    &registry,
                    RenderExportStages::av(&mut video_graph, &mut delivery, &mut audio_graph),
                    context.progress(),
                    context.operation(),
                )
            },
        )
        .unwrap();

    dispatch_engine(
        &mut dispatcher,
        "submit-real-render-export",
        EngineCommand::ExecuteExportJob(EngineExportJobCommand::submit(job_id, []).unwrap()),
    );

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    let mut poll = 0_u64;
    let snapshot = loop {
        let outcome = dispatch_engine(
            &mut dispatcher,
            &format!("poll-real-render-export-{poll}"),
            EngineCommand::ExecuteExportJob(EngineExportJobCommand::Poll),
        );
        poll += 1;
        let EngineCommandResult::ExportJobs(state) = outcome.result() else {
            panic!("expected logical export state")
        };
        let snapshot = state.job(job_id).unwrap();
        if snapshot.status() == ExportJobStatus::Completed {
            break snapshot.clone();
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::yield_now();
    };

    let artifact = runtime.result(job_id).unwrap().unwrap();
    assert_eq!(snapshot.attempt(), 1);
    assert_eq!(snapshot.progress().completed_units(), 11);
    assert_eq!(snapshot.progress().revision(), 11);
    assert_eq!(snapshot.progress().total_units(), None);
    assert_eq!(artifact.streams().len(), 2);
    assert_eq!(attempts.load(Ordering::SeqCst), 2);
    assert_eq!(resets.load(Ordering::SeqCst), 4);
    assert!(dispatcher
        .drain_events()
        .unwrap()
        .iter()
        .any(|event| matches!(event.event(), EngineEvent::ExportJobsStateChanged(_))));

    drop(domain);
    runtime.shutdown().unwrap();
}

#[test]
fn encoder_color_interpretation_must_match_the_terminal_delivery_space() {
    let _domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let engine = running_engine();
    let snapshot = engine.snapshot().unwrap();
    let permit = snapshot.admit(EngineWorkKind::Export).permit().unwrap();
    let decoded = video_frame(0);
    let scene_pipeline = scene_pipeline();
    let decoded_binding = SharedExportDecodedFrame::new();
    let (graph_snapshot, output) = graph_snapshot(
        decoded_binding.clone(),
        scene_pipeline.clone(),
        AlphaMode::Premultiplied,
        Arc::new(AtomicUsize::new(0)),
    );
    let mut graph = HeadlessGraphVideoRenderer::new(
        graph_snapshot,
        output,
        PixelBounds::from_origin_size(0, 0, 1, 1).unwrap(),
        scene_pipeline.clone(),
        AlphaMode::Premultiplied,
        decoded_binding,
    )
    .unwrap();
    let mut delivery = TestDelivery::new(scene_pipeline, Arc::new(AtomicUsize::new(0)));
    let mismatched = EncoderConfig::video(
        VIDEO_OUTPUT,
        CodecId::new("test-video").unwrap(),
        Timebase::integer(24).unwrap(),
        video_format(),
    );
    let request = RenderExportRequest::new(
        RationalTime::zero(Timebase::integer(24).unwrap()),
        vec![ExportStreamRoute::new(VIDEO_SOURCE, mismatched)],
        FallbackPolicy::Disallow,
        permit,
    )
    .unwrap();
    let attempts = Arc::new(AtomicUsize::new(0));
    let mut session = TestSession::video_only(decoded, false);
    let error = render_and_export(
        &request,
        || Ok(snapshot.value().clone()),
        &mut session,
        &primary_registry(attempts.clone(), Arc::new(AtomicUsize::new(0))),
        RenderExportStages::video(&mut graph, &mut delivery),
        &OperationContext::new(MediaPriority::Export),
    )
    .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(
        error.message(),
        "encoder color interpretation differs from the terminal delivery space"
    );
    assert_eq!(attempts.load(Ordering::SeqCst), 0);
    assert_eq!(session.reset_calls, 0);
}

#[test]
fn real_vp9_timing_rounding_is_rejected_without_publishing_the_acquired_webm() {
    let _domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let engine = running_engine();
    let snapshot = engine.snapshot().unwrap();
    let permit = snapshot.admit(EngineWorkKind::Export).permit().unwrap();
    let registry = media_backend_registry().unwrap();
    let project = fixture_project();
    let mut resources = acquire_timeline_resources(
        &project,
        FIXTURE_TIMELINE,
        &registry,
        [fixture_resource_request()],
        ResourceAcquisitionPolicy::default(),
        &OperationContext::new(MediaPriority::Interactive),
    )
    .unwrap();

    let scene_pipeline =
        ColorPipelineMetadata::new(ImageColorTags::new(fixture_color_space())).unwrap();
    let decoded_binding = SharedExportDecodedFrame::new();
    let graph_calls = Arc::new(AtomicUsize::new(0));
    let (graph_snapshot, output) = graph_snapshot(
        decoded_binding.clone(),
        scene_pipeline.clone(),
        AlphaMode::Opaque,
        graph_calls.clone(),
    );
    let mut graph = HeadlessGraphVideoRenderer::new(
        graph_snapshot,
        output,
        PixelBounds::from_origin_size(0, 0, 96, 54).unwrap(),
        scene_pipeline.clone(),
        AlphaMode::Opaque,
        decoded_binding,
    )
    .unwrap();
    let delivery_calls = Arc::new(AtomicUsize::new(0));
    let mut delivery =
        TestDelivery::with_alpha(scene_pipeline, AlphaMode::Opaque, delivery_calls.clone());
    let config = EncoderConfig::video(
        StreamId::new(31),
        CodecId::new("vp9").unwrap(),
        Timebase::NANOSECONDS,
        fixture_video_format(),
    );
    let request = RenderExportRequest::new(
        RationalTime::zero(Timebase::NANOSECONDS),
        vec![ExportStreamRoute::new(FIXTURE_STREAM, config.clone())],
        FallbackPolicy::Disallow,
        permit,
    )
    .unwrap();
    let error = render_and_export(
        &request,
        || Ok(snapshot.value().clone()),
        resources.media_mut(FIXTURE_MEDIA).unwrap(),
        &registry,
        RenderExportStages::video(&mut graph, &mut delivery),
        &OperationContext::new(MediaPriority::Export),
    )
    .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::CorruptData);
    assert_eq!(error.recoverability(), Recoverability::Degraded);
    assert_eq!(
        error.message(),
        "encoded packets do not preserve the exact union of input intervals"
    );
    assert_eq!(graph_calls.load(Ordering::SeqCst), 96);
    assert_eq!(delivery_calls.load(Ordering::SeqCst), 96);
}

#[test]
fn acquired_wave_pcm_decode_audio_graph_and_pcm_encode_complete_with_exact_samples() {
    let _domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let engine = running_engine();
    let snapshot = engine.snapshot().unwrap();
    let permit = snapshot.admit(EngineWorkKind::Export).permit().unwrap();
    let registry = media_backend_registry().unwrap();
    let project = pcm_project();
    let mut resources = acquire_timeline_resources(
        &project,
        PCM_TIMELINE,
        &registry,
        [pcm_resource_request()],
        ResourceAcquisitionPolicy::default(),
        &OperationContext::new(MediaPriority::Interactive),
    )
    .unwrap();
    let format = AudioFormat::new(44_100, SampleFormat::I16, ChannelLayout::stereo()).unwrap();
    let config = EncoderConfig::audio(
        StreamId::new(32),
        CodecId::new("pcm_s16le").unwrap(),
        format.clone(),
    );
    let request = RenderExportRequest::new(
        RationalTime::zero(Timebase::integer(44_100).unwrap()),
        vec![ExportStreamRoute::new(PCM_STREAM, config.clone())],
        FallbackPolicy::Disallow,
        permit,
    )
    .unwrap();
    let audio_calls = Arc::new(AtomicUsize::new(0));
    let mut audio_graph = TestAudioGraph {
        id: AudioGraphId::from_raw(0x603),
        calls: audio_calls.clone(),
    };
    let artifact = render_and_export::<_, (), _>(
        &request,
        || Ok(snapshot.value().clone()),
        resources.media_mut(PCM_MEDIA).unwrap(),
        &registry,
        RenderExportStages::audio(&mut audio_graph),
        &OperationContext::new(MediaPriority::Export),
    )
    .unwrap();

    let stream = &artifact.streams()[0];
    assert_eq!(stream.config(), &config);
    assert_eq!(
        stream.selection().selected_backend_id().as_str(),
        "rust-pcm"
    );
    assert_eq!(stream.inputs().len(), 1);
    assert_eq!(stream.packets().len(), 1);
    assert_eq!(stream.packets()[0].data().len(), 17_640);
    assert_eq!(
        stream.packets()[0].timing().duration(),
        Some(Duration::new(4_410, Timebase::integer(44_100).unwrap()).unwrap())
    );
    assert_eq!(audio_calls.load(Ordering::SeqCst), 1);
    let ExportInputProvenance::Audio(input) = &stream.inputs()[0] else {
        panic!("real PCM export did not retain audio provenance")
    };
    assert_eq!(input.graph_id(), AudioGraphId::from_raw(0x603));
    assert_eq!(input.output_format(), &format);
}

#[test]
fn backend_policy_never_retries_after_the_selected_factory_fails() {
    let _domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let engine = running_engine();
    let snapshot = engine.snapshot().unwrap();
    let permit = snapshot.admit(EngineWorkKind::Export).permit().unwrap();
    let decoded = video_frame(0);
    let scene_pipeline = scene_pipeline();
    let decoded_binding = SharedExportDecodedFrame::new();
    let (graph_snapshot, output) = graph_snapshot(
        decoded_binding.clone(),
        scene_pipeline.clone(),
        AlphaMode::Premultiplied,
        Arc::new(AtomicUsize::new(0)),
    );
    let mut graph = HeadlessGraphVideoRenderer::new(
        graph_snapshot,
        output,
        PixelBounds::from_origin_size(0, 0, 1, 1).unwrap(),
        scene_pipeline.clone(),
        AlphaMode::Premultiplied,
        decoded_binding,
    )
    .unwrap();
    let mut delivery = TestDelivery::new(scene_pipeline, Arc::new(AtomicUsize::new(0)));
    let request = video_request(permit, FallbackPolicy::AllowRegistered);

    let denied_attempts = Arc::new(AtomicUsize::new(0));
    let mut fallback_only = BackendRegistry::new();
    fallback_only
        .register(
            BackendRegistration::new(
                Arc::new(TestBackend::new(
                    "fallback-only",
                    false,
                    denied_attempts.clone(),
                    Arc::new(AtomicUsize::new(0)),
                )),
                encode_capabilities(),
                100,
                BackendTier::Fallback,
            )
            .unwrap(),
        )
        .unwrap();
    let mut denied_session = TestSession::video_only(decoded.clone(), false);
    let denied = render_and_export(
        &video_request(permit, FallbackPolicy::Disallow),
        || Ok(snapshot.value().clone()),
        &mut denied_session,
        &fallback_only,
        RenderExportStages::video(&mut graph, &mut delivery),
        &OperationContext::new(MediaPriority::Export),
    )
    .unwrap_err();
    assert_eq!(denied.category(), ErrorCategory::Unsupported);
    assert_eq!(denied_attempts.load(Ordering::SeqCst), 0);
    assert_eq!(denied_session.reset_calls, 0);

    let primary_attempts = Arc::new(AtomicUsize::new(0));
    let fallback_attempts = Arc::new(AtomicUsize::new(0));
    let resets = Arc::new(AtomicUsize::new(0));
    let mut registry = BackendRegistry::new();
    registry
        .register(
            BackendRegistration::new(
                Arc::new(TestBackend::new(
                    "selected-primary",
                    true,
                    primary_attempts.clone(),
                    resets.clone(),
                )),
                encode_capabilities(),
                20,
                BackendTier::Primary,
            )
            .unwrap(),
        )
        .unwrap();
    registry
        .register(
            BackendRegistration::new(
                Arc::new(TestBackend::new(
                    "unused-fallback",
                    false,
                    fallback_attempts.clone(),
                    resets,
                )),
                encode_capabilities(),
                100,
                BackendTier::Fallback,
            )
            .unwrap(),
        )
        .unwrap();
    let mut session = TestSession::video_only(decoded, false);
    let error = render_and_export(
        &request,
        || Ok(snapshot.value().clone()),
        &mut session,
        &registry,
        RenderExportStages::video(&mut graph, &mut delivery),
        &OperationContext::new(MediaPriority::Export),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unavailable);
    assert_eq!(primary_attempts.load(Ordering::SeqCst), 1);
    assert_eq!(fallback_attempts.load(Ordering::SeqCst), 0);
    assert_eq!(session.reset_calls, 0);
}

#[test]
fn degraded_lifecycle_partial_media_and_fresh_recovery_publish_only_complete_output() {
    let _domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let mut engine = running_engine();
    let running = engine.snapshot().unwrap();
    let stale_permit = running.admit(EngineWorkKind::Export).permit().unwrap();
    let degraded = engine
        .report_runtime_failure(
            EngineSubsystem::Rendering,
            Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "render device is rebuilding",
            ),
        )
        .unwrap();
    let decoded = video_frame(0);
    let scene_pipeline = scene_pipeline();
    let decoded_binding = SharedExportDecodedFrame::new();
    let (graph_snapshot, output) = graph_snapshot(
        decoded_binding.clone(),
        scene_pipeline.clone(),
        AlphaMode::Premultiplied,
        Arc::new(AtomicUsize::new(0)),
    );
    let mut graph = HeadlessGraphVideoRenderer::new(
        graph_snapshot,
        output,
        PixelBounds::from_origin_size(0, 0, 1, 1).unwrap(),
        scene_pipeline.clone(),
        AlphaMode::Premultiplied,
        decoded_binding,
    )
    .unwrap();
    let mut delivery = TestDelivery::new(scene_pipeline, Arc::new(AtomicUsize::new(0)));
    let mut session = TestSession::video_only(decoded, true);
    let attempts = Arc::new(AtomicUsize::new(0));
    let resets = Arc::new(AtomicUsize::new(0));
    let registry = primary_registry(attempts.clone(), resets.clone());

    let stale = render_and_export(
        &video_request(stale_permit, FallbackPolicy::Disallow),
        || Ok(degraded.value().clone()),
        &mut session,
        &registry,
        RenderExportStages::video(&mut graph, &mut delivery),
        &OperationContext::new(MediaPriority::Export),
    )
    .unwrap_err();
    assert_eq!(stale.category(), ErrorCategory::Conflict);
    assert_eq!(attempts.load(Ordering::SeqCst), 0);

    let recovery_action = engine
        .begin_recovery(EngineSubsystem::Rendering)
        .unwrap()
        .pending_action()
        .unwrap();
    let recovered = engine.complete_action(recovery_action).unwrap();
    let permit = recovered.admit(EngineWorkKind::Export).permit().unwrap();
    let request = video_request(permit, FallbackPolicy::Disallow);
    let partial = render_and_export(
        &request,
        || Ok(recovered.value().clone()),
        &mut session,
        &registry,
        RenderExportStages::video(&mut graph, &mut delivery),
        &OperationContext::new(MediaPriority::Export),
    )
    .unwrap_err();
    assert_eq!(partial.category(), ErrorCategory::CorruptData);
    assert_eq!(session.reset_calls, 2);
    assert_eq!(resets.load(Ordering::SeqCst), 2);

    session.partial_on_first_read = false;
    let artifact = render_and_export(
        &request,
        || Ok(recovered.value().clone()),
        &mut session,
        &registry,
        RenderExportStages::video(&mut graph, &mut delivery),
        &OperationContext::new(MediaPriority::Export),
    )
    .unwrap();
    assert_eq!(artifact.streams().len(), 1);
    assert_eq!(artifact.streams()[0].packets().len(), 1);
    assert_eq!(session.reset_calls, 4);
    assert_eq!(resets.load(Ordering::SeqCst), 4);
}

fn dispatch_engine(
    dispatcher: &mut EngineCommandDispatcher,
    transaction_id: &str,
    command: EngineCommand,
) -> superi_engine::dispatcher::EngineCommandOutcome {
    dispatcher
        .dispatch(EngineCommandRequest::new(
            EngineTransactionId::new(transaction_id).unwrap(),
            command,
        ))
        .unwrap()
}

fn running_engine() -> EngineLifecycle {
    let mut engine = EngineLifecycle::new().unwrap();
    while engine.snapshot().unwrap().phase() == LifecyclePhase::Starting {
        let action = engine.snapshot().unwrap().pending_action().unwrap();
        assert_eq!(action.kind(), EngineLifecycleActionKind::Initialize);
        engine.complete_action(action).unwrap();
    }
    engine
}

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/slice/video-cfr/v1/input.webm")
}

fn fixture_range(start: i64, duration: u64, timebase: Timebase) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, timebase),
        Duration::new(duration, timebase).unwrap(),
    )
    .unwrap()
}

fn fixture_project() -> EditorialProject {
    let source_timebase = Timebase::NANOSECONDS;
    let edit_timebase = Timebase::integer(24).unwrap();
    let media = LinkedMediaReference::with_fingerprint(
        FIXTURE_MEDIA,
        "canonical source",
        fixture_path().to_string_lossy(),
        Some(fixture_range(0, 4_000_000_000, source_timebase)),
        FIXTURE_FINGERPRINT,
    )
    .unwrap();
    let clip = Clip::new(
        ClipId::from_raw(0x300),
        "canonical source",
        ClipSource::Media(FIXTURE_MEDIA),
        fixture_range(0, 4_000_000_000, source_timebase),
        fixture_range(0, 96, edit_timebase),
    )
    .unwrap();
    let timeline = Timeline::new(
        FIXTURE_TIMELINE,
        "canonical",
        edit_timebase,
        RationalTime::zero(edit_timebase),
        vec![Track::new(
            TrackId::from_raw(0x400),
            "V1",
            TrackSemantics::Video(VideoTrackSemantics::new(
                FrameRate::FPS_24,
                VideoCompositing::Over,
            )),
            vec![TrackItem::Clip(clip)],
        )],
    );
    EditorialProject::new(
        ProjectId::from_raw(0x500),
        "export project",
        [media],
        [timeline],
    )
    .unwrap()
}

fn fixture_resource_request() -> MediaResourceRequest {
    MediaResourceRequest::new(
        SourceRequest::new(FIXTURE_MEDIA, SourceLocation::Path(fixture_path())),
        [DecoderResourceRequest::new(FIXTURE_STREAM)],
    )
    .unwrap()
}

fn fixture_color_space() -> ColorSpace {
    ColorSpace::new(
        ColorPrimaries::Unspecified,
        TransferFunction::Unspecified,
        MatrixCoefficients::Bt709,
        ColorRange::Limited,
    )
}

fn fixture_video_format() -> VideoFormat {
    VideoFormat::new(
        96,
        54,
        PixelFormat::Yuv420p8,
        ColorSpace::BT709,
        AlphaMode::Opaque,
    )
    .unwrap()
}

fn pcm_fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/audio/synchronized-multichannel/v1/stereo-44100.wav")
}

fn pcm_project() -> EditorialProject {
    let timebase = Timebase::integer(44_100).unwrap();
    let media = LinkedMediaReference::new(
        PCM_MEDIA,
        "canonical PCM source",
        pcm_fixture_path().to_string_lossy(),
        Some(fixture_range(0, 4_410, timebase)),
    );
    let clip = Clip::new(
        ClipId::from_raw(0x604),
        "canonical PCM source",
        ClipSource::Media(PCM_MEDIA),
        fixture_range(0, 4_410, timebase),
        fixture_range(0, 4_410, timebase),
    )
    .unwrap();
    let layout = ChannelLayout::stereo();
    let routing = AudioRouting::new(
        AudioRouteDestination::Main,
        layout.clone(),
        [
            AudioChannelRoute::new(
                ChannelPosition::FrontLeft,
                AudioChannelTarget::Channel(ChannelPosition::FrontLeft),
            ),
            AudioChannelRoute::new(
                ChannelPosition::FrontRight,
                AudioChannelTarget::Channel(ChannelPosition::FrontRight),
            ),
        ],
    )
    .unwrap();
    let timeline = Timeline::new(
        PCM_TIMELINE,
        "PCM export",
        timebase,
        RationalTime::zero(timebase),
        vec![Track::new(
            TrackId::from_raw(0x605),
            "A1",
            TrackSemantics::Audio(AudioTrackSemantics::new(44_100, layout, routing).unwrap()),
            vec![TrackItem::Clip(clip)],
        )],
    );
    EditorialProject::new(
        ProjectId::from_raw(0x606),
        "PCM export project",
        [media],
        [timeline],
    )
    .unwrap()
}

fn pcm_resource_request() -> MediaResourceRequest {
    MediaResourceRequest::new(
        SourceRequest::new(PCM_MEDIA, SourceLocation::Path(pcm_fixture_path())),
        [DecoderResourceRequest::new(PCM_STREAM).with_audio_format(
            AudioFormat::new(44_100, SampleFormat::I16, ChannelLayout::stereo()).unwrap(),
        )],
    )
    .unwrap()
}

fn video_request(
    permit: superi_engine::lifecycle::EngineWorkPermit,
    fallback: FallbackPolicy,
) -> RenderExportRequest {
    RenderExportRequest::new(
        RationalTime::zero(Timebase::integer(24).unwrap()),
        vec![ExportStreamRoute::new(VIDEO_SOURCE, video_encoder_config())],
        fallback,
        permit,
    )
    .unwrap()
}

fn video_encoder_config() -> EncoderConfig {
    EncoderConfig::video(
        VIDEO_OUTPUT,
        CodecId::new("test-video").unwrap(),
        Timebase::integer(24).unwrap(),
        delivery_video_format(video_format(), ColorSpace::BT709, AlphaMode::Premultiplied),
    )
}

fn audio_encoder_config() -> EncoderConfig {
    EncoderConfig::audio(
        AUDIO_OUTPUT,
        CodecId::new("test-audio").unwrap(),
        audio_format(),
    )
}

fn video_format() -> VideoFormat {
    VideoFormat::new(
        1,
        1,
        PixelFormat::Rgba16Float,
        ColorSpace::DISPLAY_P3,
        AlphaMode::Premultiplied,
    )
    .unwrap()
}

fn delivery_video_format(source: VideoFormat, output: ColorSpace, alpha: AlphaMode) -> VideoFormat {
    VideoFormat::new(
        source.width(),
        source.height(),
        source.pixel_format(),
        output,
        alpha,
    )
    .unwrap()
}

fn audio_format() -> AudioFormat {
    AudioFormat::new(48_000, SampleFormat::F32, ChannelLayout::stereo()).unwrap()
}

fn video_frame(value: i64) -> VideoFrame {
    let format = video_format();
    let plane = VideoPlane::new(Arc::from([0_u8; 8]), 8, 1).unwrap();
    let buffer =
        Arc::new(CpuVideoBuffer::new(1, 1, PixelFormat::Rgba16Float, vec![plane]).unwrap());
    VideoFrame::new(
        format,
        RationalTime::new(value, Timebase::integer(24).unwrap()),
        Duration::new(1, Timebase::integer(24).unwrap()).unwrap(),
        buffer,
    )
    .unwrap()
    .with_metadata("source.camera", MetadataValue::Text("camera-a".to_owned()))
    .unwrap()
    .with_color_pipeline(decoded_pipeline())
    .unwrap()
}

fn audio_block(sample: i64) -> AudioBlock {
    AudioBlock::new(
        audio_format(),
        SampleTime::new(sample, 48_000).unwrap(),
        4,
        vec![AudioPlane::new(Arc::from([0_u8; 32]))],
    )
    .unwrap()
    .with_metadata(
        "source.microphone",
        MetadataValue::Text("boom-a".to_owned()),
    )
    .unwrap()
}

fn decoded_pipeline() -> ColorPipelineMetadata {
    ColorPipelineMetadata::new(ImageColorTags::new(ColorSpace::DISPLAY_P3))
        .unwrap()
        .with_stage(color_stage(
            ColorTransformStageKind::Input,
            "display-p3-to-acescg",
            ColorSpace::DISPLAY_P3,
            ColorSpace::ACESCG,
        ))
        .unwrap()
}

fn scene_pipeline() -> ColorPipelineMetadata {
    decoded_pipeline()
}

fn delivery_pipeline(scene: &ColorPipelineMetadata) -> ColorPipelineMetadata {
    scene
        .clone()
        .with_stage(color_stage(
            ColorTransformStageKind::Output,
            "scene-to-bt709",
            scene.current_space(),
            ColorSpace::BT709,
        ))
        .unwrap()
}

fn color_stage(
    kind: ColorTransformStageKind,
    name: &str,
    source: ColorSpace,
    destination: ColorSpace,
) -> ColorTransformStage {
    ColorTransformStage::new(kind, name, source, destination).unwrap()
}

struct TestDelivery {
    scene: ColorPipelineMetadata,
    output: ColorPipelineMetadata,
    alpha_mode: AlphaMode,
    calls: Arc<AtomicUsize>,
}

impl TestDelivery {
    fn new(scene: ColorPipelineMetadata, calls: Arc<AtomicUsize>) -> Self {
        Self::with_alpha(scene, AlphaMode::Premultiplied, calls)
    }

    fn with_alpha(
        scene: ColorPipelineMetadata,
        alpha_mode: AlphaMode,
        calls: Arc<AtomicUsize>,
    ) -> Self {
        let output = delivery_pipeline(&scene);
        Self {
            scene,
            output,
            alpha_mode,
            calls,
        }
    }
}

impl ExportVideoDelivery<VideoFrame> for TestDelivery {
    fn scene_pipeline(&self) -> &ColorPipelineMetadata {
        &self.scene
    }

    fn scene_alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }

    fn output_pipeline(&self) -> &ColorPipelineMetadata {
        &self.output
    }

    fn output_alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }

    fn transform(
        &mut self,
        scene: &PlaybackSceneFrame<VideoFrame>,
        operation: &OperationContext,
    ) -> Result<VideoFrame> {
        operation.check("test_delivery")?;
        self.calls.fetch_add(1, Ordering::SeqCst);
        let source = scene.value();
        let destination = self.output.delivery_space().ok_or_else(|| {
            test_error("test delivery output omitted its terminal delivery space")
        })?;
        let mut output = VideoFrame::new(
            delivery_video_format(source.format(), destination, self.alpha_mode),
            source.timestamp(),
            source.duration(),
            source.shared_buffer(),
        )?;
        for (key, value) in source.metadata().iter() {
            output = output.with_metadata(key, value.clone())?;
        }
        Ok(output)
    }
}

struct TestAudioGraph {
    id: AudioGraphId,
    calls: Arc<AtomicUsize>,
}

impl ExportAudioGraph for TestAudioGraph {
    fn render(
        &mut self,
        decoded: &AudioBlock,
        operation: &OperationContext,
    ) -> Result<RenderedExportAudio> {
        operation.check("test_audio_graph")?;
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(RenderedExportAudio::new(decoded.clone(), self.id))
    }
}

#[derive(Clone)]
struct SceneNode {
    decoded: SharedExportDecodedFrame,
    scene_pipeline: ColorPipelineMetadata,
    alpha_mode: AlphaMode,
    calls: Arc<AtomicUsize>,
}

impl EvaluateNode<PlaybackSceneFrame<VideoFrame>> for SceneNode {
    fn evaluate(
        &self,
        context: &EvaluationContext<'_, PlaybackSceneFrame<VideoFrame>>,
    ) -> Result<PlaybackSceneFrame<VideoFrame>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let decoded = self.decoded.current()?;
        if decoded.timestamp() != context.request().frame() {
            return Err(test_error("bound decoded frame differs from graph request"));
        }
        PlaybackSceneFrame::from_decoded(
            decoded.clone(),
            &decoded,
            self.scene_pipeline.clone(),
            self.alpha_mode,
        )
    }
}

struct SceneCompiler {
    decoded: SharedExportDecodedFrame,
    scene_pipeline: ColorPipelineMetadata,
    alpha_mode: AlphaMode,
    calls: Arc<AtomicUsize>,
}

impl NodeCompiler<(), SceneNode> for SceneCompiler {
    fn compile_node(
        &mut self,
        _snapshot: &GraphSnapshot<()>,
        _node_id: NodeId,
        _node: &EditableNode<()>,
    ) -> Result<SceneNode> {
        Ok(SceneNode {
            decoded: self.decoded.clone(),
            scene_pipeline: self.scene_pipeline.clone(),
            alpha_mode: self.alpha_mode,
            calls: self.calls.clone(),
        })
    }
}

fn graph_snapshot(
    decoded: SharedExportDecodedFrame,
    scene_pipeline: ColorPipelineMetadata,
    alpha_mode: AlphaMode,
    calls: Arc<AtomicUsize>,
) -> (Arc<GraphEvaluationSnapshot<(), SceneNode>>, GraphEndpoint) {
    let value_type = ValueTypeId::new("superi.value.export-scene-frame").unwrap();
    let port_name = PortName::new("frame").unwrap();
    let schema = Arc::new(
        NodeSchema::new(
            NodeSchemaId::new(
                NodeTypeId::new("superi.test.export-scene-frame").unwrap(),
                "1.0.0".parse::<SemanticVersion>().unwrap(),
            ),
            [],
            [PortSchema::new(
                port_name.clone(),
                value_type,
                PortCardinality::Single,
            )],
            [],
            NodeBehavior::new(
                TimeBehavior::CurrentFrame,
                RoiBehavior::InputBounds,
                ColorRequirements::Exact(scene_pipeline.current_space()),
                Determinism::Deterministic,
                CachePolicy::PerRegion,
            ),
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
    let mut graph = EditableGraph::new(GraphId::from_raw(700));
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
            GraphEvaluationSnapshot::compile(
                graph.snapshot(),
                SceneCompiler {
                    decoded,
                    scene_pipeline,
                    alpha_mode,
                    calls,
                },
            )
            .unwrap(),
        ),
        output,
    )
}

struct TestSession {
    info: SourceInfo,
    configs: BTreeMap<StreamId, DecoderConfig>,
    packets: Vec<Packet>,
    packet_cursor: usize,
    outputs: BTreeMap<StreamId, Vec<DecodeOutput>>,
    output_indexes: BTreeMap<StreamId, usize>,
    ready: BTreeMap<StreamId, VecDeque<DecodeOutput>>,
    flushed: BTreeSet<StreamId>,
    partial_on_first_read: bool,
    reset_calls: usize,
}

impl TestSession {
    fn paired(video: VideoFrame, audio: AudioBlock, partial: bool) -> Self {
        Self::new(Some(video), Some(audio), partial)
    }

    fn video_only(video: VideoFrame, partial: bool) -> Self {
        Self::new(Some(video), None, partial)
    }

    fn new(video: Option<VideoFrame>, audio: Option<AudioBlock>, partial: bool) -> Self {
        let mut streams = Vec::new();
        let mut packets = Vec::new();
        let mut configs = BTreeMap::new();
        let mut outputs = BTreeMap::new();
        if let Some(frame) = video {
            let stream = StreamInfo::new(
                VIDEO_SOURCE,
                StreamKind::Video,
                CodecId::new("source-video").unwrap(),
                frame.timestamp().timebase(),
            );
            configs.insert(VIDEO_SOURCE, DecoderConfig::new(stream.clone()));
            packets.push(source_packet(
                VIDEO_SOURCE,
                frame.timestamp(),
                frame.duration(),
            ));
            outputs.insert(VIDEO_SOURCE, vec![DecodeOutput::Frame(frame)]);
            streams.push(stream);
        }
        if let Some(block) = audio {
            let stream = StreamInfo::new(
                AUDIO_SOURCE,
                StreamKind::Audio,
                CodecId::new("source-audio").unwrap(),
                Timebase::integer(block.format().sample_rate()).unwrap(),
            );
            configs.insert(
                AUDIO_SOURCE,
                DecoderConfig::new(stream.clone())
                    .with_audio_format(block.format().clone())
                    .unwrap(),
            );
            packets.push(source_packet(
                AUDIO_SOURCE,
                RationalTime::from_sample_time(block.timestamp()),
                block.duration(),
            ));
            outputs.insert(AUDIO_SOURCE, vec![DecodeOutput::Audio(block)]);
            streams.push(stream);
        }
        let info = SourceInfo::new(
            SourceIdentity::new(MediaId::from_raw(501), "sha256:render-export-source").unwrap(),
            streams,
        )
        .unwrap();
        Self {
            info,
            configs,
            packets,
            packet_cursor: 0,
            outputs,
            output_indexes: BTreeMap::new(),
            ready: BTreeMap::new(),
            flushed: BTreeSet::new(),
            partial_on_first_read: partial,
            reset_calls: 0,
        }
    }
}

impl ExportMediaSession for TestSession {
    fn source_info(&self) -> &SourceInfo {
        &self.info
    }

    fn decoder_config(&self, stream_id: StreamId) -> Option<DecoderConfig> {
        self.configs.get(&stream_id).cloned()
    }

    fn seek(&mut self, request: SeekRequest, operation: &OperationContext) -> Result<RationalTime> {
        operation.check("test_seek")?;
        self.packet_cursor = 0;
        Ok(request.target())
    }

    fn read_packet(&mut self, operation: &OperationContext) -> Result<ReadOutcome<Packet>> {
        operation.check("test_read")?;
        let Some(packet) = self.packets.get(self.packet_cursor).cloned() else {
            return Ok(ReadOutcome::EndOfStream);
        };
        if self.partial_on_first_read && self.packet_cursor == 0 {
            return Ok(ReadOutcome::Partial {
                value: packet.clone(),
                report: CorruptionReport::truncated(0, 2, 1)?.with_stream(packet.stream_id()),
            });
        }
        self.packet_cursor += 1;
        Ok(ReadOutcome::Complete(packet))
    }

    fn reset_decoder(&mut self, stream_id: StreamId, operation: &OperationContext) -> Result<()> {
        operation.check("test_reset_decoder")?;
        if !self.configs.contains_key(&stream_id) {
            return Err(test_error("decoder is unavailable"));
        }
        self.output_indexes.insert(stream_id, 0);
        self.ready.entry(stream_id).or_default().clear();
        self.flushed.remove(&stream_id);
        self.reset_calls += 1;
        Ok(())
    }

    fn send_packet(
        &mut self,
        stream_id: StreamId,
        packet: Packet,
        operation: &OperationContext,
    ) -> Result<()> {
        operation.check("test_send_packet")?;
        if packet.stream_id() != stream_id {
            return Err(test_error("packet route changed"));
        }
        let index = self.output_indexes.entry(stream_id).or_default();
        let output = self
            .outputs
            .get(&stream_id)
            .and_then(|outputs| outputs.get(*index))
            .cloned()
            .ok_or_else(|| test_error("decoder received an extra packet"))?;
        *index += 1;
        self.ready.entry(stream_id).or_default().push_back(output);
        Ok(())
    }

    fn receive_decoder(
        &mut self,
        stream_id: StreamId,
        operation: &OperationContext,
    ) -> Result<DecodeOutput> {
        operation.check("test_receive_decoder")?;
        if let Some(output) = self.ready.entry(stream_id).or_default().pop_front() {
            return Ok(output);
        }
        if self.flushed.contains(&stream_id) {
            return Ok(DecodeOutput::EndOfStream);
        }
        Ok(DecodeOutput::NeedInput)
    }

    fn flush_decoder(&mut self, stream_id: StreamId, operation: &OperationContext) -> Result<()> {
        operation.check("test_flush_decoder")?;
        self.flushed.insert(stream_id);
        Ok(())
    }
}

fn source_packet(stream_id: StreamId, timestamp: RationalTime, duration: Duration) -> Packet {
    Packet::new(
        stream_id,
        Arc::from([stream_id.value() as u8]),
        PacketTiming::new(
            timestamp.timebase(),
            Some(timestamp.value()),
            Some(timestamp.value()),
            Some(duration.value()),
        )
        .unwrap(),
    )
}

struct TestBackend {
    descriptor: BackendDescriptor,
    fail_factory: bool,
    attempts: Arc<AtomicUsize>,
    resets: Arc<AtomicUsize>,
}

impl TestBackend {
    fn new(
        id: &str,
        fail_factory: bool,
        attempts: Arc<AtomicUsize>,
        resets: Arc<AtomicUsize>,
    ) -> Self {
        Self {
            descriptor: BackendDescriptor::new(
                BackendId::new(id).unwrap(),
                "Render export contract backend",
            )
            .unwrap(),
            fail_factory,
            attempts,
            resets,
        }
    }
}

impl MediaBackend for TestBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        _probe: &SourceProbe<'_>,
        _operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        Ok(SourceProbeResult::NoMatch)
    }

    fn open_source(
        &self,
        _request: &SourceRequest,
        _operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        Err(test_error("test backend does not open sources"))
    }

    fn create_decoder(
        &self,
        _config: &DecoderConfig,
        _operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>> {
        Err(test_error("test backend does not create decoders"))
    }

    fn create_encoder(
        &self,
        config: &EncoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        operation.check("test_create_encoder")?;
        self.attempts.fetch_add(1, Ordering::SeqCst);
        if self.fail_factory {
            return Err(Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "selected encoder factory is unavailable",
            ));
        }
        Ok(Box::new(TestEncoder {
            config: config.clone(),
            packets: VecDeque::new(),
            flushed: false,
            resets: self.resets.clone(),
        }))
    }
}

struct TestEncoder {
    config: EncoderConfig,
    packets: VecDeque<Packet>,
    flushed: bool,
    resets: Arc<AtomicUsize>,
}

impl Encoder for TestEncoder {
    fn config(&self) -> &EncoderConfig {
        &self.config
    }

    fn send(&mut self, input: EncodeInput, operation: &OperationContext) -> Result<()> {
        operation.check("test_encode_send")?;
        let (timestamp, duration, metadata, marker) = match (&self.config.media_format(), input) {
            (EncoderMediaFormat::Video(_), EncodeInput::Video(frame)) => (
                frame.timestamp(),
                frame.duration(),
                frame.metadata().clone(),
                1_u8,
            ),
            (EncoderMediaFormat::Audio(_), EncodeInput::Audio(block)) => (
                RationalTime::from_sample_time(block.timestamp()),
                block.duration(),
                block.metadata().clone(),
                2_u8,
            ),
            _ => return Err(test_error("encoder input kind changed")),
        };
        let timestamp = timestamp.checked_rescale(self.config.timebase(), TimeRounding::Exact)?;
        let duration = duration.checked_rescale(self.config.timebase(), TimeRounding::Exact)?;
        let mut packet = Packet::new(
            self.config.stream_id(),
            Arc::from([marker]),
            PacketTiming::new(
                self.config.timebase(),
                Some(timestamp.value()),
                Some(timestamp.value()),
                Some(duration.value()),
            )?,
        );
        for (key, value) in metadata.iter() {
            packet = packet.with_metadata(key, value.clone())?;
        }
        self.packets.push_back(packet);
        Ok(())
    }

    fn receive(&mut self, operation: &OperationContext) -> Result<EncodeOutput> {
        operation.check("test_encode_receive")?;
        if let Some(packet) = self.packets.pop_front() {
            return Ok(EncodeOutput::Packet(packet));
        }
        if self.flushed {
            return Ok(EncodeOutput::EndOfStream);
        }
        Ok(EncodeOutput::NeedInput)
    }

    fn flush(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("test_encode_flush")?;
        self.flushed = true;
        Ok(())
    }

    fn reset(&mut self, operation: &OperationContext) -> Result<()> {
        operation.check("test_encode_reset")?;
        self.packets.clear();
        self.flushed = false;
        self.resets.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn encode_capabilities() -> BackendCapabilities {
    BackendCapabilities::new([
        BackendCapability::Encode(CodecId::new("test-video").unwrap()),
        BackendCapability::Encode(CodecId::new("test-audio").unwrap()),
    ])
}

fn primary_registry(attempts: Arc<AtomicUsize>, resets: Arc<AtomicUsize>) -> BackendRegistry {
    let mut registry = BackendRegistry::new();
    registry
        .register(
            BackendRegistration::new(
                Arc::new(TestBackend::new("primary", false, attempts, resets)),
                encode_capabilities(),
                10,
                BackendTier::Primary,
            )
            .unwrap(),
        )
        .unwrap();
    registry
}

fn test_error(message: &'static str) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message).with_context(
        ErrorContext::new("superi-engine.render-export-test", "contract"),
    )
}
