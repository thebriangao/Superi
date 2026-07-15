use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration as MonotonicDuration, Instant};

use superi_audio::playback::{create_output_buffer, OutputBufferConfig, OutputConsumer};
use superi_cache::frame::CachedFrameColorMetadata;
use superi_cache::prefetch::PlaybackPrefetchConfig;
use superi_color::working_space::WorkingSpace;
use superi_concurrency::backpressure::{
    bounded_handoff, BackpressureConfig, HandoffReceiver, PipelineRoute, PipelineStage,
};
use superi_concurrency::jobs::{BoundedWorkerPool, WorkerPoolConfig};
use superi_concurrency::lifecycle::LifecyclePhase;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, Recoverability};
use superi_core::ids::JobId;
use superi_core::pixel::AlphaMode;
use superi_core::time::{Duration, RationalTime, TimeRange, Timebase};
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult, EngineEvent,
    EngineTransactionId,
};
use superi_engine::introspection::MediaCapabilities;
use superi_engine::lifecycle::{EngineSubsystem, EngineWorkKind};
use superi_engine::media::media_backend_registry;
use superi_engine::playback::{
    PlaybackAudioOutput, PlaybackFrameEvaluator, PlaybackOrchestrator, PlaybackPoll,
    PlaybackPrefetchEvaluator, PlaybackPrefetcher, PlaybackViewportFrame,
};
use superi_engine::render::ViewportColorMetadata;
use superi_engine::transport::{
    DroppedFramePolicy, PlaybackDirection, PlaybackTransport, PlaybackTransportCommand,
    PlaybackTransportConfig, PlaybackTransportMode, PlaybackTransportSnapshot, TransportAudioState,
    TransportDegradationCode,
};
use superi_graph::node::GraphColorMetadata;
use superi_image::metadata::{
    ColorPipelineMetadata, ColorTransformStage, ColorTransformStageKind, ImageColorTags,
};
use superi_timeline::retime::PlaybackRate;

struct ExactFrameEvaluator {
    color: ViewportColorMetadata,
    fail_frame: Option<i64>,
}

impl PlaybackFrameEvaluator<i64> for ExactFrameEvaluator {
    fn evaluate_frame(
        &self,
        frame: RationalTime,
    ) -> superi_core::error::Result<PlaybackViewportFrame<i64>> {
        if self.fail_frame == Some(frame.value()) {
            return Err(Error::new(
                ErrorCategory::CorruptData,
                Recoverability::Degraded,
                "injected foreground frame failure",
            ));
        }
        PlaybackViewportFrame::new(
            frame.value(),
            frame,
            Duration::new(1, frame.timebase()).unwrap(),
            Vec::new(),
            self.color.clone(),
            AlphaMode::Premultiplied,
        )
    }
}

struct RecordingPrefetch {
    frames: Arc<Mutex<Vec<RationalTime>>>,
    fail_frame: Option<i64>,
}

impl PlaybackPrefetchEvaluator for RecordingPrefetch {
    fn prefetch(&self, frame: RationalTime) -> superi_core::error::Result<()> {
        self.frames.lock().unwrap().push(frame);
        if self.fail_frame == Some(frame.value()) {
            return Err(Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Degraded,
                "injected replaceable prefetch failure",
            ));
        }
        Ok(())
    }
}

#[test]
fn interactive_transport_preserves_exact_discontinuities_rate_cadence_and_loop_direction() {
    let base = Instant::now();
    let mut harness = Harness::new(
        DroppedFramePolicy::PreserveEveryFrame,
        RationalTime::new(5, frame_timebase()),
        base,
    );
    let playback_domain = ExecutionDomain::Playback.enter_current().unwrap();

    harness
        .transport
        .seek(RationalTime::new(5, frame_timebase()), base)
        .unwrap();
    assert_eq!(harness.transport.snapshot().epoch(), 1);
    assert!(matches!(
        harness.transport.snapshot().audio_state(),
        TransportAudioState::MutedInactive(PlaybackTransportMode::Paused)
    ));
    assert!(harness
        .transport
        .snapshot()
        .audio_discard_status()
        .is_pending());
    acknowledge_discard(&mut harness.consumer);
    poll_presented(&mut harness.transport, base, 5);

    harness.transport.begin_scrub(base).unwrap();
    harness
        .transport
        .scrub_to(RationalTime::new(8, frame_timebase()), base)
        .unwrap();
    harness
        .transport
        .scrub_to(RationalTime::new(9, frame_timebase()), base)
        .unwrap();
    acknowledge_discard(&mut harness.consumer);
    poll_presented(&mut harness.transport, base, 9);
    harness.transport.end_scrub(true, base).unwrap();
    assert!(matches!(
        harness.transport.snapshot().audio_state(),
        TransportAudioState::DiscardPending(_)
    ));
    acknowledge_discard(&mut harness.consumer);
    let resumed = poll_presented(&mut harness.transport, base, 9);
    assert_eq!(resumed.mode(), PlaybackTransportMode::Playing);
    assert_eq!(resumed.scheduled_frame().unwrap().value(), 10);
    assert_eq!(harness.transport.queue_audio(&[0.0]).unwrap().frames, 1);
    harness.transport.pause(base).unwrap();
    acknowledge_discard(&mut harness.consumer);
    let paused = poll_presented(&mut harness.transport, base, 9);
    assert_eq!(paused.mode(), PlaybackTransportMode::Paused);
    assert!(matches!(
        paused.audio_state(),
        TransportAudioState::MutedInactive(PlaybackTransportMode::Paused)
    ));
    assert!(harness.transport.queue_audio(&[0.0]).is_err());
    harness.transport.step_frames(-2, base).unwrap();
    acknowledge_discard(&mut harness.consumer);
    poll_presented(&mut harness.transport, base, 7);

    harness
        .transport
        .set_rate(PlaybackRate::new(1, 2).unwrap(), base)
        .unwrap();
    acknowledge_discard(&mut harness.consumer);
    poll_presented(&mut harness.transport, base, 7);
    harness.transport.play(base).unwrap();
    assert!(matches!(
        harness.transport.snapshot().audio_state(),
        TransportAudioState::MutedUnsupportedRate(rate)
            if rate == PlaybackRate::new(1, 2).unwrap()
    ));
    assert!(harness
        .transport
        .snapshot()
        .audio_discard_status()
        .is_pending());
    acknowledge_discard(&mut harness.consumer);
    let playing = poll_presented(&mut harness.transport, base, 7);
    assert_eq!(playing.scheduled_frame().unwrap().value(), 8);
    assert_eq!(playing.scheduled_due_clock().unwrap().value(), 4_000);

    harness
        .transport
        .set_rate(PlaybackRate::new(3, 2).unwrap(), base)
        .unwrap();
    acknowledge_discard(&mut harness.consumer);
    let fast = poll_presented(&mut harness.transport, base, 7);
    assert_eq!(fast.scheduled_frame().unwrap().value(), 8);
    assert_eq!(fast.scheduled_due_clock().unwrap().value(), 1_334);

    harness
        .transport
        .set_direction(PlaybackDirection::Reverse, base)
        .unwrap();
    let unsupported_pending = harness.transport.snapshot();
    assert!(matches!(
        unsupported_pending.audio_state(),
        TransportAudioState::MutedUnsupportedRate(rate)
            if rate == PlaybackRate::new(-3, 2).unwrap()
    ));
    assert!(unsupported_pending.audio_discard_status().is_pending());
    assert!(unsupported_pending
        .degradation()
        .contains(TransportDegradationCode::AudioDiscardPending));
    assert!(unsupported_pending
        .degradation()
        .contains(TransportDegradationCode::AudioRateUnsupported));
    acknowledge_discard(&mut harness.consumer);
    let reversed = poll_presented(&mut harness.transport, base, 7);
    assert_eq!(reversed.rate(), PlaybackRate::new(-3, 2).unwrap());
    assert_eq!(reversed.direction(), PlaybackDirection::Reverse);
    assert_eq!(reversed.scheduled_frame().unwrap().value(), 6);
    assert_eq!(reversed.scheduled_due_clock().unwrap().value(), 1_334);
    assert!(harness.transport.queue_audio(&[0.0]).is_err());

    let loop_range = TimeRange::from_start_end(
        RationalTime::new(4, frame_timebase()),
        RationalTime::new(8, frame_timebase()),
    )
    .unwrap();
    harness.transport.set_loop(Some(loop_range), base).unwrap();
    acknowledge_discard(&mut harness.consumer);
    poll_presented(&mut harness.transport, base, 7);
    harness
        .transport
        .seek(RationalTime::new(4, frame_timebase()), base)
        .unwrap();
    acknowledge_discard(&mut harness.consumer);
    let looped = poll_presented(&mut harness.transport, base, 4);
    assert_eq!(looped.scheduled_frame().unwrap().value(), 7);
    assert_eq!(looped.scheduled_due_clock().unwrap().value(), 1_334);
    assert!(looped.epoch() >= 9);

    let loop_7 = poll_presented(
        &mut harness.transport,
        base + MonotonicDuration::from_millis(28),
        7,
    );
    assert_eq!(loop_7.scheduled_due_clock().unwrap().value(), 2_667);
    let loop_6 = poll_presented(
        &mut harness.transport,
        base + MonotonicDuration::from_millis(56),
        6,
    );
    assert_eq!(loop_6.scheduled_due_clock().unwrap().value(), 4_000);
    let loop_5 = poll_presented(
        &mut harness.transport,
        base + MonotonicDuration::from_millis(84),
        5,
    );
    assert_eq!(loop_5.scheduled_due_clock().unwrap().value(), 5_334);
    let loop_4 = poll_presented(
        &mut harness.transport,
        base + MonotonicDuration::from_millis(112),
        4,
    );
    assert_eq!(loop_4.scheduled_frame().unwrap().value(), 7);
    assert_eq!(loop_4.scheduled_due_clock().unwrap().value(), 6_667);
    let loop_pause_time = base + MonotonicDuration::from_millis(112);
    harness.transport.pause(loop_pause_time).unwrap();
    acknowledge_discard(&mut harness.consumer);
    let paused_on_boundary = poll_presented(&mut harness.transport, loop_pause_time, 4);
    assert_eq!(paused_on_boundary.mode(), PlaybackTransportMode::Paused);

    assert_eq!(
        harness.viewport.try_receive().unwrap().timestamp().value(),
        5
    );
    assert_eq!(
        harness.viewport.try_receive().unwrap().timestamp().value(),
        9
    );

    drop(playback_domain);
    harness.shutdown();
}

#[test]
fn late_drop_policy_is_bounded_and_never_skips_an_exact_seek() {
    let base = Instant::now();
    let mut harness = Harness::new(
        DroppedFramePolicy::DropLate { max_consecutive: 2 },
        RationalTime::new(0, frame_timebase()),
        base,
    );
    let playback_domain = ExecutionDomain::Playback.enter_current().unwrap();

    harness.transport.play(base).unwrap();
    acknowledge_discard(&mut harness.consumer);
    poll_presented(&mut harness.transport, base, 0);
    let late = base + MonotonicDuration::from_secs(1);
    let first_late = harness.transport.update_at(late).unwrap();
    assert_eq!(first_late.snapshot().drop_statistics().total_dropped(), 2);
    let forced = if matches!(
        first_late.playback(),
        PlaybackPoll::Presented { frame, .. } if frame.value() == 3
    ) {
        first_late.snapshot()
    } else {
        assert_eq!(first_late.snapshot().scheduled_frame().unwrap().value(), 3);
        poll_presented(&mut harness.transport, late, 3)
    };
    assert_eq!(forced.drop_statistics().forced_presentations(), 1);

    harness
        .transport
        .seek(RationalTime::new(7, frame_timebase()), late)
        .unwrap();
    acknowledge_discard(&mut harness.consumer);
    let exact = poll_presented(&mut harness.transport, late, 7);
    assert_eq!(exact.playhead().value(), 7);
    assert_eq!(exact.drop_statistics().total_dropped(), 2);

    harness
        .transport
        .seek(RationalTime::new(11, frame_timebase()), late)
        .unwrap();
    acknowledge_discard(&mut harness.consumer);
    let ended = poll_presented(&mut harness.transport, late, 11);
    assert_eq!(ended.mode(), PlaybackTransportMode::Ended);
    assert!(ended.audio_discard_status().is_pending());
    assert!(harness
        .transport
        .seek(RationalTime::new(12, frame_timebase()), late)
        .is_err());
    assert_eq!(
        harness.transport.snapshot().mode(),
        PlaybackTransportMode::Ended
    );
    harness
        .transport
        .seek(RationalTime::new(6, frame_timebase()), late)
        .unwrap();
    acknowledge_discard(&mut harness.consumer);
    let recovered_from_end = poll_presented(&mut harness.transport, late, 6);
    assert_eq!(recovered_from_end.mode(), PlaybackTransportMode::Paused);

    drop(playback_domain);
    harness.shutdown();
}

#[test]
fn degraded_frame_prefetch_and_viewport_paths_recover_without_changing_transport_identity() {
    let base = Instant::now();
    let mut harness = Harness::new_with_options(
        DroppedFramePolicy::PreserveEveryFrame,
        RationalTime::new(0, frame_timebase()),
        base,
        1,
        Some(2),
        Some(1),
    );
    let playback_domain = ExecutionDomain::Playback.enter_current().unwrap();

    harness
        .transport
        .seek(RationalTime::new(0, frame_timebase()), base)
        .unwrap();
    acknowledge_discard(&mut harness.consumer);
    let initial = poll_presented(&mut harness.transport, base, 0);
    let prefetch_degraded = if initial
        .degradation()
        .contains(TransportDegradationCode::PrefetchFailure)
    {
        initial
    } else {
        poll_until_degradation(
            &mut harness.transport,
            base,
            TransportDegradationCode::PrefetchFailure,
        )
    };
    assert_eq!(prefetch_degraded.playhead().value(), 0);
    assert_eq!(
        harness.viewport.try_receive().unwrap().timestamp().value(),
        0
    );

    harness.transport.play(base).unwrap();
    harness
        .transport
        .seek(RationalTime::new(2, frame_timebase()), base)
        .unwrap();
    acknowledge_discard(&mut harness.consumer);
    let failed = poll_failed(&mut harness.transport, base, 2);
    assert_eq!(failed.mode(), PlaybackTransportMode::Paused);
    assert!(failed.audio_discard_status().is_pending());
    assert!(failed
        .degradation()
        .contains(TransportDegradationCode::FrameFailure));

    harness
        .transport
        .seek(RationalTime::new(3, frame_timebase()), base)
        .unwrap();
    acknowledge_discard(&mut harness.consumer);
    let recovered = poll_presented(&mut harness.transport, base, 3);
    assert!(!recovered
        .degradation()
        .contains(TransportDegradationCode::FrameFailure));
    assert!(!recovered
        .degradation()
        .contains(TransportDegradationCode::PrefetchFailure));
    assert_eq!(
        harness.viewport.try_receive().unwrap().timestamp().value(),
        3
    );

    harness.transport.play(base).unwrap();
    acknowledge_discard(&mut harness.consumer);
    poll_presented(&mut harness.transport, base, 3);
    let due = base + MonotonicDuration::from_millis(42);
    let backpressured = poll_until_degradation(
        &mut harness.transport,
        due,
        TransportDegradationCode::ViewportBackpressure,
    );
    assert_eq!(backpressured.scheduled_frame().unwrap().value(), 4);
    assert_eq!(
        harness.viewport.try_receive().unwrap().timestamp().value(),
        3
    );
    let recovered_viewport = poll_presented(&mut harness.transport, due, 4);
    assert!(!recovered_viewport
        .degradation()
        .contains(TransportDegradationCode::ViewportBackpressure));

    drop(playback_domain);
    harness.shutdown();
}

struct Harness {
    transport: PlaybackTransport<i64>,
    consumer: OutputConsumer,
    viewport: HandoffReceiver<PlaybackViewportFrame<i64>>,
    pool: Arc<BoundedWorkerPool>,
}

impl Harness {
    fn new(policy: DroppedFramePolicy, initial: RationalTime, base: Instant) -> Self {
        Self::new_with_options(policy, initial, base, 32, None, None)
    }

    fn new_with_options(
        policy: DroppedFramePolicy,
        initial: RationalTime,
        base: Instant,
        viewport_capacity: usize,
        fail_frame: Option<i64>,
        fail_prefetch: Option<i64>,
    ) -> Self {
        let (producer, consumer, _telemetry) = create_output_buffer(OutputBufferConfig {
            channels: 1,
            sample_rate: 48_000,
            capacity_frames: 32,
            initial_sample: 0,
        })
        .unwrap();
        let route = PipelineRoute::new(PipelineStage::Graph, PipelineStage::Viewport).unwrap();
        let (viewport_sender, viewport) =
            bounded_handoff(BackpressureConfig::new(route, viewport_capacity).unwrap());
        let pool = Arc::new(BoundedWorkerPool::new(WorkerPoolConfig::new(2, 32).unwrap()).unwrap());
        let audio = PlaybackAudioOutput::new(producer, consumer.clock().clone());
        let mut orchestrator = PlaybackOrchestrator::audio_master(
            &pool,
            Arc::new(ExactFrameEvaluator {
                color: viewport_color(),
                fail_frame,
            }),
            viewport_sender,
            audio,
            RationalTime::new(0, clock_timebase()),
        )
        .unwrap();
        let playback_domain = ExecutionDomain::Playback.enter_current().unwrap();
        orchestrator.fallback_to_playback_at(base).unwrap();
        let observations = Arc::new(Mutex::new(Vec::new()));
        let prefetcher = PlaybackPrefetcher::new(
            &pool,
            Arc::new(RecordingPrefetch {
                frames: observations,
                fail_frame: fail_prefetch,
            }),
        );
        let bounds = TimeRange::from_start_end(
            RationalTime::new(0, frame_timebase()),
            RationalTime::new(12, frame_timebase()),
        )
        .unwrap();
        let config = PlaybackTransportConfig::new(
            bounds,
            initial,
            PlaybackPrefetchConfig::new(1, 1, 0).unwrap(),
            policy,
        )
        .unwrap();
        let transport = PlaybackTransport::new(orchestrator, prefetcher, config, 91, base).unwrap();
        drop(playback_domain);
        Self {
            transport,
            consumer,
            viewport,
            pool,
        }
    }

    fn shutdown(self) {
        drop(self.transport);
        let pool = match Arc::try_unwrap(self.pool) {
            Ok(pool) => pool,
            Err(_) => panic!("transport released the worker pool"),
        };
        pool.shutdown().unwrap();
    }
}

fn poll_presented(
    transport: &mut PlaybackTransport<i64>,
    now: Instant,
    expected: i64,
) -> PlaybackTransportSnapshot {
    for _ in 0..5_000 {
        let update = transport.update_at(now).unwrap();
        if matches!(
            update.playback(),
            PlaybackPoll::Presented { frame, .. } if frame.value() == expected
        ) {
            return update.snapshot();
        }
        thread::yield_now();
    }
    panic!("frame {expected} did not present");
}

fn poll_failed(
    transport: &mut PlaybackTransport<i64>,
    now: Instant,
    expected: i64,
) -> PlaybackTransportSnapshot {
    for _ in 0..5_000 {
        let update = transport.update_at(now).unwrap();
        if matches!(
            update.playback(),
            PlaybackPoll::Failed { frame, .. } if frame.value() == expected
        ) {
            return update.snapshot();
        }
        thread::yield_now();
    }
    panic!("frame {expected} did not fail");
}

fn poll_until_degradation(
    transport: &mut PlaybackTransport<i64>,
    now: Instant,
    expected: TransportDegradationCode,
) -> PlaybackTransportSnapshot {
    for _ in 0..5_000 {
        let update = transport.update_at(now).unwrap();
        if update.snapshot().degradation().contains(expected) {
            return update.snapshot();
        }
        thread::yield_now();
    }
    panic!("degraded condition {expected:?} did not appear");
}

fn acknowledge_discard(consumer: &mut OutputConsumer) {
    consumer.render_f32(&mut [0.0]).unwrap();
}

fn frame_timebase() -> Timebase {
    Timebase::integer(24).unwrap()
}

fn clock_timebase() -> Timebase {
    Timebase::integer(48_000).unwrap()
}

fn viewport_color() -> ViewportColorMetadata {
    let scene = ColorPipelineMetadata::new(ImageColorTags::new(WorkingSpace::ACESCG.color_space()))
        .unwrap();
    let graph = GraphColorMetadata::new(scene);
    let cached = CachedFrameColorMetadata::from_graph(&graph);
    let display = ColorTransformStage::new(
        ColorTransformStageKind::Display,
        "test-display",
        ColorSpace::ACESCG,
        ColorSpace::SRGB,
    )
    .unwrap();
    ViewportColorMetadata::from_cache(&cached, display).unwrap()
}

#[test]
fn transport_job_namespace_is_deterministic_and_nonzero() {
    let job = JobId::from_raw((u128::from(91_u64) << 64) | 1);
    assert_ne!(job.raw(), 0);
}

#[test]
fn engine_dispatcher_routes_transport_commands_across_domains_with_ordered_state_events() {
    let base = Instant::now();
    let mut harness = Harness::new(
        DroppedFramePolicy::PreserveEveryFrame,
        RationalTime::new(0, frame_timebase()),
        base,
    );

    let engine_domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let (mut dispatcher, mut playback_executor) =
        EngineCommandDispatcher::new_with_playback_bridge().unwrap();
    drive_dispatcher_to_running(&mut dispatcher);
    dispatcher.drain_events().unwrap();

    let seek = dispatcher
        .dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("playback-seek").unwrap(),
            EngineCommand::ExecutePlayback(PlaybackTransportCommand::Seek(RationalTime::new(
                5,
                frame_timebase(),
            ))),
        ))
        .unwrap();
    let EngineCommandResult::PlaybackAccepted { permit } = seek.result() else {
        panic!("playback command did not return typed admission")
    };
    assert_eq!(permit.unwrap().work(), EngineWorkKind::Playback);

    let inspection = dispatcher
        .dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("inspect-while-playback-pending").unwrap(),
            EngineCommand::InspectLifecycle,
        ))
        .unwrap();
    assert!(matches!(
        inspection.result(),
        EngineCommandResult::Lifecycle(_)
    ));
    assert!(inspection.command_sequence() > seek.command_sequence());

    let blocked = dispatcher
        .dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("overtake-playback").unwrap(),
            EngineCommand::BeginShutdown,
        ))
        .unwrap_err();
    assert_eq!(blocked.category(), ErrorCategory::Conflict);
    drop(engine_domain);

    let off_domain = playback_executor
        .execute_next(&mut harness.transport, base)
        .unwrap_err();
    assert_eq!(off_domain.category(), ErrorCategory::Conflict);
    assert_eq!(harness.transport.snapshot().epoch(), 0);

    let playback_domain = ExecutionDomain::Playback.enter_current().unwrap();
    let execution = playback_executor
        .execute_next(&mut harness.transport, base)
        .unwrap()
        .unwrap();
    assert_eq!(execution.command_sequence(), seek.command_sequence());
    assert_eq!(execution.snapshot().playhead().value(), 5);
    assert_eq!(execution.snapshot().epoch(), 1);
    assert!(execution.failure().is_none());
    drop(playback_domain);

    let engine_domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let events = dispatcher.drain_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].command_sequence(), seek.command_sequence());
    assert_eq!(events[0].playback_epoch(), Some(1));
    match events[0].event() {
        EngineEvent::PlaybackStateChanged { snapshot, failure } => {
            assert_eq!(snapshot.playhead().value(), 5);
            assert!(failure.is_none());
        }
        event => panic!("unexpected playback event: {event:?}"),
    }
    let capabilities =
        MediaCapabilities::from_registry(&media_backend_registry().unwrap()).unwrap();
    let validation = dispatcher
        .integration_validation_snapshot(&capabilities, None)
        .unwrap();
    assert!(validation.playback().is_attached());
    assert!(!validation.playback().command_pending());
    assert_eq!(
        validation
            .playback()
            .latest_snapshot()
            .unwrap()
            .playhead()
            .value(),
        5
    );
    assert!(validation.playback().latest_failure().is_none());
    assert!(dispatcher.drain_events().unwrap().is_empty());

    dispatcher
        .dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("degrade-playback").unwrap(),
            EngineCommand::ReportRuntimeFailure {
                subsystem: EngineSubsystem::Playback,
                failure: superi_engine::dispatcher::EngineReportedFailure::new(
                    ErrorCategory::Unavailable,
                    Recoverability::Retryable,
                    "playback scheduler is rebuilding",
                )
                .unwrap(),
            },
        ))
        .unwrap();
    let denied = dispatcher
        .dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("degraded-playback-command").unwrap(),
            EngineCommand::ExecutePlayback(PlaybackTransportCommand::Play),
        ))
        .unwrap_err();
    assert_eq!(denied.category(), ErrorCategory::Conflict);

    let recovery = dispatcher
        .dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("recover-playback").unwrap(),
            EngineCommand::BeginRecovery(EngineSubsystem::Playback),
        ))
        .unwrap();
    let EngineCommandResult::Lifecycle(recovery) = recovery.result() else {
        panic!("recovery did not return lifecycle state")
    };
    let action = recovery.pending_action().unwrap();
    dispatcher
        .dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("complete-playback-recovery").unwrap(),
            EngineCommand::CompleteLifecycleAction(action),
        ))
        .unwrap();
    dispatcher.drain_events().unwrap();

    let invalid = dispatcher
        .dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("invalid-playback-seek").unwrap(),
            EngineCommand::ExecutePlayback(PlaybackTransportCommand::Seek(RationalTime::new(
                99,
                frame_timebase(),
            ))),
        ))
        .unwrap();
    drop(engine_domain);

    let playback_domain = ExecutionDomain::Playback.enter_current().unwrap();
    let execution = playback_executor
        .execute_next(&mut harness.transport, base)
        .unwrap()
        .unwrap();
    assert_eq!(execution.command_sequence(), invalid.command_sequence());
    assert_eq!(execution.snapshot().playhead().value(), 5);
    assert!(execution.failure().is_some());
    drop(playback_domain);

    let engine_domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let events = dispatcher.drain_events().unwrap();
    assert_eq!(events.len(), 1);
    match events[0].event() {
        EngineEvent::PlaybackStateChanged { snapshot, failure } => {
            assert_eq!(snapshot.playhead().value(), 5);
            assert_eq!(
                failure.as_ref().unwrap().category(),
                ErrorCategory::InvalidInput
            );
        }
        event => panic!("unexpected playback failure event: {event:?}"),
    }
    let validation = dispatcher
        .integration_validation_snapshot(&capabilities, None)
        .unwrap();
    assert_eq!(
        validation.playback().latest_failure().unwrap().category(),
        ErrorCategory::InvalidInput
    );
    drop(engine_domain);

    drop(playback_executor);
    harness.shutdown();
}

fn drive_dispatcher_to_running(dispatcher: &mut EngineCommandDispatcher) {
    loop {
        let inspection = dispatcher
            .dispatch(EngineCommandRequest::new(
                EngineTransactionId::new("inspect-startup").unwrap(),
                EngineCommand::InspectLifecycle,
            ))
            .unwrap();
        let EngineCommandResult::Lifecycle(snapshot) = inspection.result() else {
            panic!("lifecycle inspection returned another result")
        };
        if snapshot.phase() == LifecyclePhase::Running {
            break;
        }
        let action = snapshot.pending_action().unwrap();
        dispatcher
            .dispatch(EngineCommandRequest::new(
                EngineTransactionId::new("complete-startup").unwrap(),
                EngineCommand::CompleteLifecycleAction(action),
            ))
            .unwrap();
    }
}
