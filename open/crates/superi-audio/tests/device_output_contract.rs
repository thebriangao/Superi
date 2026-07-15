use superi_audio::playback::{
    create_output_buffer, discover_output_devices, OutputBufferConfig, OutputBufferError,
    OutputBufferSize, OutputCapability, OutputConsumer, OutputDeviceId, OutputProducer,
    OutputSampleFormat, OutputStreamConfig, OutputTelemetry,
};
use superi_concurrency::threads::ExecutionDomain;

#[test]
fn capability_ranges_preserve_device_constraints() {
    let capability = OutputCapability {
        channels: 2,
        min_sample_rate: 44_100,
        max_sample_rate: 96_000,
        sample_format: OutputSampleFormat::F32,
        buffer_size: OutputBufferSize::Range { min: 64, max: 512 },
    };

    assert!(capability.supports(&OutputStreamConfig {
        channels: 2,
        sample_rate: 48_000,
        sample_format: OutputSampleFormat::F32,
        buffer_frames: Some(128),
    }));
    assert!(!capability.supports(&OutputStreamConfig {
        channels: 2,
        sample_rate: 192_000,
        sample_format: OutputSampleFormat::F32,
        buffer_frames: Some(128),
    }));
    assert!(!capability.supports(&OutputStreamConfig {
        channels: 2,
        sample_rate: 48_000,
        sample_format: OutputSampleFormat::F32,
        buffer_frames: Some(1_024),
    }));
}

#[test]
fn bounded_output_preserves_whole_frames_and_reports_backpressure() {
    let (mut producer, _consumer, telemetry) = create_output_buffer(OutputBufferConfig {
        channels: 2,
        sample_rate: 48_000,
        capacity_frames: 2,
        initial_sample: 0,
    })
    .expect("valid bounded output buffer");

    let accepted = producer
        .push_interleaved(&[0.25, -0.25, 0.5, -0.5])
        .expect("two frames fit exactly");
    assert_eq!(accepted.frames, 2);
    assert_eq!(accepted.samples, 4);

    assert_eq!(
        producer.push_interleaved(&[0.75, -0.75]),
        Err(OutputBufferError::InsufficientCapacity {
            requested_samples: 2,
            available_samples: 0,
        })
    );
    assert_eq!(telemetry.snapshot().dropped_samples, 2);

    assert_eq!(
        producer.push_interleaved(&[0.1]),
        Err(OutputBufferError::SampleCountNotFrameAligned {
            samples: 1,
            channels: 2,
        })
    );
}

#[test]
fn producer_rejects_non_normalized_samples_before_queue_mutation() {
    let (mut producer, mut consumer, _telemetry) = create_output_buffer(OutputBufferConfig {
        channels: 2,
        sample_rate: 48_000,
        capacity_frames: 2,
        initial_sample: 0,
    })
    .expect("valid bounded output buffer");

    assert_eq!(
        producer.push_interleaved(&[f32::NAN, 0.0]),
        Err(OutputBufferError::NonFiniteSample { index: 0 })
    );
    assert_eq!(
        producer.push_interleaved(&[0.0, 1.1]),
        Err(OutputBufferError::SampleOutOfRange { index: 1 })
    );

    let mut device_buffer = [1.0; 2];
    let report = consumer
        .render_f32(&mut device_buffer)
        .expect("callback remains aligned");
    assert_eq!(device_buffer, [0.0; 2]);
    assert_eq!(report.consumed_samples, 0);
}

#[test]
fn buffer_creation_rejects_unbounded_allocation_requests() {
    assert_eq!(
        create_output_buffer(OutputBufferConfig {
            channels: 2,
            sample_rate: 48_000,
            capacity_frames: 1_000_000,
            initial_sample: 0,
        })
        .err(),
        Some(OutputBufferError::CapacityTooLarge {
            requested_samples: 2_000_000,
            max_samples: 1_048_576,
        })
    );
}

#[test]
fn callback_renders_silence_on_starvation_and_advances_complete_frames() {
    let (mut producer, mut consumer, telemetry) = create_output_buffer(OutputBufferConfig {
        channels: 2,
        sample_rate: 48_000,
        capacity_frames: 2,
        initial_sample: 100,
    })
    .expect("valid bounded output buffer");
    producer
        .push_interleaved(&[0.25, -0.25, 0.5, -0.5])
        .expect("two frames fit");

    let mut device_buffer = [9.0; 6];
    let report = consumer
        .render_f32(&mut device_buffer)
        .expect("device callback is frame aligned");

    assert_eq!(device_buffer, [0.25, -0.25, 0.5, -0.5, 0.0, 0.0]);
    assert_eq!(report.frames, 3);
    assert_eq!(report.consumed_samples, 4);
    assert_eq!(report.silence_samples, 2);
    assert!(report.underrun);
    assert_eq!(consumer.clock().position().sample(), 103);

    let snapshot = telemetry.snapshot();
    assert_eq!(snapshot.rendered_frames, 3);
    assert_eq!(snapshot.silence_samples, 2);
    assert_eq!(snapshot.underruns, 1);
}

#[test]
fn discontinuity_discards_only_pre_acknowledgement_audio_and_recovers() {
    let (mut producer, mut consumer, telemetry) = create_output_buffer(OutputBufferConfig {
        channels: 2,
        sample_rate: 48_000,
        capacity_frames: 4,
        initial_sample: 200,
    })
    .expect("valid bounded output buffer");
    producer
        .push_interleaved(&[0.25, -0.25, 0.5, -0.5])
        .expect("old epoch audio fits");

    let first = producer
        .request_discard()
        .expect("first discard generation is available");
    let second = producer
        .request_discard()
        .expect("coalesced discard generation is available");
    assert_eq!(first.requested_generation, 1);
    assert_eq!(second.requested_generation, 2);
    assert!(second.is_pending());
    assert_eq!(
        producer.push_interleaved(&[0.75, -0.75]),
        Err(OutputBufferError::DiscardPending {
            requested_generation: 2,
            applied_generation: 0,
        })
    );

    let mut discarded_callback = [1.0; 2];
    let discarded_report = consumer
        .render_f32(&mut discarded_callback)
        .expect("callback applies the pending discard");
    assert_eq!(discarded_callback, [0.0; 2]);
    assert_eq!(discarded_report.consumed_samples, 0);
    assert_eq!(consumer.clock().position().sample(), 201);

    let acknowledged = producer.discard_status();
    assert_eq!(acknowledged.requested_generation, 2);
    assert_eq!(acknowledged.applied_generation, 2);
    assert!(!acknowledged.is_pending());
    producer
        .push_interleaved(&[0.75, -0.75])
        .expect("new epoch audio is admitted after acknowledgement");
    let mut recovered_callback = [0.0; 2];
    let recovered_report = consumer
        .render_f32(&mut recovered_callback)
        .expect("callback renders only new epoch audio");
    assert_eq!(recovered_callback, [0.75, -0.75]);
    assert_eq!(recovered_report.consumed_samples, 2);

    let snapshot = telemetry.snapshot();
    assert_eq!(snapshot.discard_requests, 2);
    assert_eq!(snapshot.discarded_samples, 4);
}

#[test]
fn invalid_callback_shape_is_silenced_without_moving_the_clock() {
    let (_producer, mut consumer, telemetry) = create_output_buffer(OutputBufferConfig {
        channels: 2,
        sample_rate: 48_000,
        capacity_frames: 2,
        initial_sample: 50,
    })
    .expect("valid bounded output buffer");
    let mut malformed = [1.0; 3];

    assert_eq!(
        consumer.render_f32(&mut malformed),
        Err(OutputBufferError::CallbackNotFrameAligned {
            samples: 3,
            channels: 2,
        })
    );
    assert_eq!(malformed, [0.0; 3]);
    assert_eq!(consumer.clock().position().sample(), 50);
    assert_eq!(telemetry.snapshot().callback_shape_errors, 1);
}

#[test]
fn production_host_discovery_exposes_stable_well_formed_capabilities() {
    match discover_output_devices() {
        Ok(discovery) => {
            for device in discovery.devices {
                assert!(!device.id.as_str().is_empty());
                assert!(!device.name.is_empty());
                for capability in device.capabilities {
                    assert!(capability.channels > 0);
                    assert!(capability.min_sample_rate > 0);
                    assert!(capability.min_sample_rate <= capability.max_sample_rate);
                }
            }
        }
        Err(error) => assert!(
            error.is_environmental(),
            "unexpected discovery error: {error}"
        ),
    }
}

#[test]
fn persisted_device_locators_round_trip_without_losing_backend_identity() {
    let id: OutputDeviceId = "backend:opaque:device"
        .parse()
        .expect("valid backend locator");
    assert_eq!(id.as_str(), "backend:opaque:device");
    assert!("missing-separator".parse::<OutputDeviceId>().is_err());
    assert!(":missing-backend".parse::<OutputDeviceId>().is_err());
    assert!("missing-device:".parse::<OutputDeviceId>().is_err());
}

#[test]
fn domain_conflict_outputs_silence_but_preserves_physical_clock_progress() {
    let (_producer, mut consumer, telemetry) = create_output_buffer(OutputBufferConfig {
        channels: 2,
        sample_rate: 48_000,
        capacity_frames: 2,
        initial_sample: 10,
    })
    .expect("valid bounded output buffer");
    let _wrong_domain = ExecutionDomain::Ui
        .enter_current()
        .expect("test thread enters a conflicting domain");
    let mut device_buffer = [1.0; 4];

    let report = consumer
        .render_f32(&mut device_buffer)
        .expect("domain failure degrades to timed silence");

    assert_eq!(device_buffer, [0.0; 4]);
    assert_eq!(report.frames, 2);
    assert_eq!(report.silence_samples, 4);
    assert!(report.underrun);
    assert_eq!(consumer.clock().position().sample(), 12);
    let snapshot = telemetry.snapshot();
    assert_eq!(snapshot.rendered_frames, 2);
    assert_eq!(snapshot.silence_samples, 4);
    assert_eq!(snapshot.underruns, 1);
    assert_eq!(snapshot.callback_domain_errors, 1);
}

#[test]
fn output_endpoints_cross_threads_and_long_sessions_do_not_accumulate_clock_drift() {
    fn assert_send<T: Send>() {}
    fn assert_send_and_sync<T: Send + Sync>() {}
    assert_send::<OutputProducer>();
    assert_send::<OutputConsumer>();
    assert_send_and_sync::<OutputTelemetry>();

    const FRAMES_PER_CALLBACK: usize = 256;
    const CALLBACKS: usize = 20_000;
    let (mut producer, mut consumer, telemetry) = create_output_buffer(OutputBufferConfig {
        channels: 2,
        sample_rate: 48_000,
        capacity_frames: FRAMES_PER_CALLBACK,
        initial_sample: 1_000,
    })
    .expect("valid bounded output buffer");
    let source = vec![0.125; FRAMES_PER_CALLBACK * 2];
    let mut device = vec![0.0; FRAMES_PER_CALLBACK * 2];

    for _ in 0..CALLBACKS {
        producer
            .push_interleaved(&source)
            .expect("one callback fits exactly");
        let report = consumer
            .render_f32(&mut device)
            .expect("callback remains aligned");
        assert!(!report.underrun);
    }

    let expected_frames = FRAMES_PER_CALLBACK * CALLBACKS;
    assert_eq!(
        consumer.clock().position().sample(),
        1_000 + i64::try_from(expected_frames).expect("test frame count fits")
    );
    let snapshot = telemetry.snapshot();
    assert_eq!(
        snapshot.rendered_frames,
        u64::try_from(expected_frames).expect("test frame count fits")
    );
    assert_eq!(snapshot.underruns, 0);
    assert_eq!(snapshot.dropped_samples, 0);
}
