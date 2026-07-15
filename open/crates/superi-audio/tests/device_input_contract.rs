use superi_audio::capture::{
    create_capture_buffer, discover_input_devices, CaptureBufferConfig, CaptureBufferError,
    CaptureCallback, CaptureReader, CaptureTelemetry, CapturedSample, InputBufferSize,
    InputCapability, InputDeviceId, InputSampleFormat, InputStreamConfig, MonitorReader,
};
use superi_audio::playback::{create_output_buffer, OutputBufferConfig};
use superi_concurrency::threads::ExecutionDomain;

#[test]
fn capability_ranges_preserve_input_constraints() {
    let capability = InputCapability {
        channels: 2,
        min_sample_rate: 44_100,
        max_sample_rate: 96_000,
        sample_format: InputSampleFormat::F32,
        buffer_size: InputBufferSize::Range { min: 64, max: 512 },
    };

    assert!(capability.supports(&InputStreamConfig {
        channels: 2,
        sample_rate: 48_000,
        sample_format: InputSampleFormat::F32,
        buffer_frames: Some(128),
    }));
    assert!(!capability.supports(&InputStreamConfig {
        channels: 1,
        sample_rate: 48_000,
        sample_format: InputSampleFormat::F32,
        buffer_frames: Some(128),
    }));
    assert!(!capability.supports(&InputStreamConfig {
        channels: 2,
        sample_rate: 192_000,
        sample_format: InputSampleFormat::F32,
        buffer_frames: Some(128),
    }));
}

#[test]
fn arming_and_monitoring_are_independent_and_preserve_exact_sample_time() {
    let (control, mut callback, mut capture, mut monitor, telemetry) =
        create_capture_buffer(CaptureBufferConfig {
            channels: 2,
            sample_rate: 48_000,
            capacity_frames: 4,
            initial_sample: 100,
        })
        .expect("valid capture path");

    control.set_monitoring(true);
    callback
        .capture_f32(&[0.25, -0.25, 0.5, -0.5])
        .expect("monitor-only callback");
    assert!(capture.drain(8).is_empty());
    assert_eq!(monitor.drain(8), vec![0.25, -0.25, 0.5, -0.5]);

    control.arm();
    control.set_monitoring(false);
    callback
        .capture_f32(&[0.75, -0.75, 1.0, -1.0])
        .expect("armed callback");
    let recorded = capture.drain(8);
    assert_eq!(
        recorded,
        vec![
            CapturedSample::new(102, 48_000, 0, 0.75).unwrap(),
            CapturedSample::new(102, 48_000, 1, -0.75).unwrap(),
            CapturedSample::new(103, 48_000, 0, 1.0).unwrap(),
            CapturedSample::new(103, 48_000, 1, -1.0).unwrap(),
        ]
    );
    assert!(monitor.drain(8).is_empty());

    control.disarm();
    callback
        .capture_f32(&[0.0, 0.0])
        .expect("disarmed callback still advances physical time");
    control.arm();
    callback
        .capture_f32(&[0.125, -0.125])
        .expect("rearmed callback");
    assert_eq!(capture.drain(8)[0].sample_time().sample(), 105);

    let snapshot = telemetry.snapshot();
    assert_eq!(snapshot.input_frames, 6);
    assert_eq!(snapshot.recorded_frames, 3);
    assert_eq!(snapshot.monitored_frames, 2);
}

#[test]
fn monitoring_feeds_the_existing_bounded_device_output_without_changing_samples() {
    let (control, mut callback, _capture, mut monitor, _telemetry) =
        create_capture_buffer(CaptureBufferConfig {
            channels: 2,
            sample_rate: 48_000,
            capacity_frames: 2,
            initial_sample: 0,
        })
        .expect("valid capture path");
    let (mut output, mut device, _output_telemetry) = create_output_buffer(OutputBufferConfig {
        channels: 2,
        sample_rate: 48_000,
        capacity_frames: 2,
        initial_sample: 0,
    })
    .expect("matching bounded output path");
    control.set_monitoring(true);
    callback
        .capture_f32(&[0.25, -0.25, 0.5, -0.5])
        .expect("captured monitoring frames");

    let mut bridge = [0.0; 4];
    let read = monitor.read_interleaved(&mut bridge);
    output
        .push_interleaved(&bridge[..read.samples])
        .expect("monitor frames fit the existing output path");
    let mut rendered = [0.0; 4];
    let report = device
        .render_f32(&mut rendered)
        .expect("matching device callback");

    assert_eq!(read.frames, 2);
    assert_eq!(rendered, [0.25, -0.25, 0.5, -0.5]);
    assert!(!report.underrun);
}

#[test]
fn each_path_applies_whole_frame_backpressure_without_blocking_the_other() {
    let (control, mut callback, mut capture, mut monitor, telemetry) =
        create_capture_buffer(CaptureBufferConfig {
            channels: 2,
            sample_rate: 48_000,
            capacity_frames: 2,
            initial_sample: 0,
        })
        .expect("valid capture path");
    control.arm();
    control.set_monitoring(true);

    callback
        .capture_f32(&[0.1, -0.1, 0.2, -0.2])
        .expect("first callback fills both paths");
    assert!(
        callback
            .capture_f32(&[0.3, -0.3])
            .expect("backpressure is observable degradation, not callback failure")
            .record_dropped
    );

    assert_eq!(capture.drain(8).len(), 4);
    assert_eq!(monitor.drain(8).len(), 4);
    let snapshot = telemetry.snapshot();
    assert_eq!(snapshot.record_dropped_frames, 1);
    assert_eq!(snapshot.monitor_dropped_frames, 1);
}

#[test]
fn malformed_and_nonfinite_callbacks_fail_before_queue_or_time_mutation() {
    let (control, mut callback, mut capture, _monitor, telemetry) =
        create_capture_buffer(CaptureBufferConfig {
            channels: 2,
            sample_rate: 48_000,
            capacity_frames: 2,
            initial_sample: 50,
        })
        .expect("valid capture path");
    control.arm();

    assert_eq!(
        callback.capture_f32(&[0.0]),
        Err(CaptureBufferError::CallbackNotFrameAligned {
            samples: 1,
            channels: 2,
        })
    );
    assert_eq!(
        callback.capture_f32(&[f32::NAN, 0.0]),
        Err(CaptureBufferError::NonFiniteSample { index: 0 })
    );
    callback
        .capture_f32(&[0.25, -0.25])
        .expect("valid callback follows rejection");
    assert_eq!(capture.drain(8)[0].sample_time().sample(), 50);
    assert_eq!(telemetry.snapshot().callback_shape_errors, 2);
}

#[test]
fn domain_conflict_drops_input_but_preserves_physical_sample_progress() {
    let (control, mut callback, mut capture, _monitor, telemetry) =
        create_capture_buffer(CaptureBufferConfig {
            channels: 2,
            sample_rate: 48_000,
            capacity_frames: 2,
            initial_sample: 10,
        })
        .expect("valid capture path");
    control.arm();
    {
        let _wrong_domain = ExecutionDomain::Ui
            .enter_current()
            .expect("test owns conflicting domain");
        let report = callback
            .capture_f32(&[0.5, -0.5, 0.25, -0.25])
            .expect("domain conflict is degraded capture");
        assert!(report.domain_conflict);
    }
    callback
        .capture_f32(&[0.125, -0.125])
        .expect("later callback succeeds");
    assert_eq!(capture.drain(8)[0].sample_time().sample(), 12);
    assert_eq!(telemetry.snapshot().callback_domain_errors, 1);
}

#[test]
fn creation_is_bounded_and_public_endpoints_cross_threads() {
    fn assert_send<T: Send>() {}
    fn assert_send_and_sync<T: Send + Sync>() {}
    assert_send::<CaptureCallback>();
    assert_send::<CaptureReader>();
    assert_send::<MonitorReader>();
    assert_send_and_sync::<CaptureTelemetry>();

    assert_eq!(
        create_capture_buffer(CaptureBufferConfig {
            channels: 2,
            sample_rate: 48_000,
            capacity_frames: 1_000_000,
            initial_sample: 0,
        })
        .err(),
        Some(CaptureBufferError::CapacityTooLarge {
            requested_samples: 2_000_000,
            max_samples: 1_048_576,
        })
    );
}

#[test]
fn persisted_input_locators_round_trip_and_host_discovery_is_honest() {
    let id: InputDeviceId = "backend:opaque:input"
        .parse()
        .expect("valid backend locator");
    assert_eq!(id.as_str(), "backend:opaque:input");
    assert!("missing-separator".parse::<InputDeviceId>().is_err());

    match discover_input_devices() {
        Ok(discovery) => {
            for device in discovery.devices {
                assert!(!device.id.as_str().is_empty());
                assert!(!device.name.is_empty());
                assert!(device.capabilities.iter().all(|capability| {
                    capability.channels > 0
                        && capability.min_sample_rate > 0
                        && capability.min_sample_rate <= capability.max_sample_rate
                }));
            }
        }
        Err(error) => assert!(error.is_environmental(), "unexpected error: {error}"),
    }
}

#[test]
fn long_sessions_advance_exactly_without_accumulated_capture_drift() {
    const FRAMES_PER_CALLBACK: usize = 256;
    const CALLBACKS: usize = 2_000;
    let (control, mut callback, mut capture, _monitor, telemetry) =
        create_capture_buffer(CaptureBufferConfig {
            channels: 1,
            sample_rate: 48_000,
            capacity_frames: FRAMES_PER_CALLBACK,
            initial_sample: 1_000,
        })
        .expect("valid capture path");
    control.arm();
    let source = vec![0.125; FRAMES_PER_CALLBACK];

    for _ in 0..CALLBACKS {
        callback.capture_f32(&source).expect("callback succeeds");
        let block = capture.drain(FRAMES_PER_CALLBACK);
        assert_eq!(block.len(), FRAMES_PER_CALLBACK);
    }

    let expected_frames = FRAMES_PER_CALLBACK * CALLBACKS;
    assert_eq!(callback.next_sample(), 1_000 + expected_frames as i64);
    assert_eq!(telemetry.snapshot().recorded_frames, expected_frames as u64);
}
