use std::f32::consts::TAU;

use superi_audio::resample::{
    DeviceClockErrorPpm, PreparedSampleRateConverter, SampleRateConverterConfig,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::pixel::ChannelLayout;
use superi_core::time::SampleTime;

fn converter(
    source_rate: u32,
    device_rate: u32,
    output_frames: usize,
    max_clock_error_ppm: f64,
) -> PreparedSampleRateConverter {
    PreparedSampleRateConverter::new(
        SampleRateConverterConfig::new(
            source_rate,
            device_rate,
            ChannelLayout::stereo(),
            output_frames,
            SampleTime::new(0, source_rate).unwrap(),
            SampleTime::new(0, device_rate).unwrap(),
            max_clock_error_ppm,
        )
        .unwrap(),
    )
    .unwrap()
}

fn constant_stereo(frames: usize, left: f32, right: f32) -> Vec<f32> {
    (0..frames).flat_map(|_| [left, right]).collect()
}

fn sine_stereo(start: i64, frames: usize, rate: u32, frequency: f32) -> Vec<f32> {
    (0..frames)
        .flat_map(|offset| {
            let phase = TAU * frequency * (start as f32 + offset as f32) / rate as f32;
            let sample = phase.sin();
            [sample, sample]
        })
        .collect()
}

#[test]
fn conversion_preserves_channel_order_exact_clocks_and_continuity() {
    let mut converter = converter(44_100, 48_000, 480, 1_000.0);
    let delay = converter.output_delay_frames();
    assert!(delay > 0);
    let _audio = ExecutionDomain::Audio.enter_current().unwrap();

    let mut rendered = Vec::new();
    for _ in 0..8 {
        let source_start = converter.source_position();
        let device_start = converter.device_position();
        let input_frames = converter.next_input_frames();
        let input = constant_stereo(converter.maximum_input_frames(), 0.25, -0.5);
        let mut output = vec![0.0; converter.output_frames() * 2];
        let report = converter
            .process_interleaved(
                source_start,
                device_start,
                &input,
                &mut output,
                DeviceClockErrorPpm::ZERO,
            )
            .unwrap();

        assert_eq!(report.source_start(), source_start);
        assert_eq!(report.device_start(), device_start);
        assert_eq!(report.source_frames(), input_frames);
        assert_eq!(report.device_frames(), 480);
        assert_eq!(report.source_end(), converter.source_position());
        assert_eq!(report.device_end(), converter.device_position());
        rendered.extend(output);
    }

    let settled = &rendered[(delay + 480) * 2..];
    for frame in settled.chunks_exact(2) {
        assert!(
            (frame[0] - 0.25).abs() < 0.002,
            "left channel changed: {frame:?}"
        );
        assert!(
            (frame[1] + 0.5).abs() < 0.002,
            "right channel changed: {frame:?}"
        );
    }
}

#[test]
fn downsampling_suppresses_out_of_band_energy() {
    fn render(frequency: f32) -> f32 {
        let mut converter = converter(96_000, 48_000, 480, 500.0);
        let delay = converter.output_delay_frames();
        let _audio = ExecutionDomain::Audio.enter_current().unwrap();
        let mut rendered = Vec::new();
        for _ in 0..12 {
            let source_start = converter.source_position();
            let input = sine_stereo(
                source_start.sample(),
                converter.maximum_input_frames(),
                96_000,
                frequency,
            );
            let mut output = vec![0.0; converter.output_frames() * 2];
            converter
                .process_interleaved(
                    source_start,
                    converter.device_position(),
                    &input,
                    &mut output,
                    DeviceClockErrorPpm::ZERO,
                )
                .unwrap();
            rendered.extend(output.into_iter().step_by(2));
        }
        let samples = &rendered[(delay + 960)..];
        (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32).sqrt()
    }

    let passband_rms = render(1_000.0);
    let stopband_rms = render(30_000.0);
    assert!(passband_rms > 0.65, "passband was damaged: {passband_rms}");
    assert!(
        stopband_rms < passband_rms * 0.02,
        "out-of-band signal aliased: pass={passband_rms}, stop={stopband_rms}"
    );
}

#[test]
fn validation_failures_do_not_advance_converter_state() {
    let mut converter = converter(44_100, 48_000, 480, 500.0);
    let source_start = converter.source_position();
    let device_start = converter.device_position();
    let input = constant_stereo(converter.maximum_input_frames(), 0.1, -0.1);
    let mut output = vec![0.0; converter.output_frames() * 2];

    let wrong_domain = converter
        .process_interleaved(
            source_start,
            device_start,
            &input,
            &mut output,
            DeviceClockErrorPpm::ZERO,
        )
        .unwrap_err();
    assert!(wrong_domain.message().contains("execution domain"));
    assert_eq!(converter.source_position(), source_start);
    assert_eq!(converter.device_position(), device_start);

    let _audio = ExecutionDomain::Audio.enter_current().unwrap();
    let discontinuous = converter
        .process_interleaved(
            SampleTime::new(1, 44_100).unwrap(),
            device_start,
            &input,
            &mut output,
            DeviceClockErrorPpm::ZERO,
        )
        .unwrap_err();
    assert!(discontinuous.message().contains("source sample position"));

    let excessive = DeviceClockErrorPpm::new(501.0).unwrap();
    let error = converter
        .process_interleaved(source_start, device_start, &input, &mut output, excessive)
        .unwrap_err();
    assert!(error.message().contains("clock error"));
    assert_eq!(converter.source_position(), source_start);
    assert_eq!(converter.device_position(), device_start);
}

#[test]
fn device_clock_error_is_ramped_with_the_documented_sign() {
    fn consumed_after_blocks(parts_per_million: f64) -> i64 {
        let mut converter = converter(48_000, 48_000, 4_800, 2_000.0);
        let _audio = ExecutionDomain::Audio.enter_current().unwrap();
        for _ in 0..6 {
            let input = constant_stereo(converter.maximum_input_frames(), 0.2, -0.2);
            let mut output = vec![0.0; converter.output_frames() * 2];
            converter
                .process_interleaved(
                    converter.source_position(),
                    converter.device_position(),
                    &input,
                    &mut output,
                    DeviceClockErrorPpm::new(parts_per_million).unwrap(),
                )
                .unwrap();
        }
        converter.source_position().sample()
    }

    let faster = consumed_after_blocks(1_000.0);
    let nominal = consumed_after_blocks(0.0);
    let slower = consumed_after_blocks(-1_000.0);
    assert!(
        faster > nominal,
        "a faster device clock must consume more source frames"
    );
    assert!(
        slower < nominal,
        "a slower device clock must consume fewer source frames"
    );
}
