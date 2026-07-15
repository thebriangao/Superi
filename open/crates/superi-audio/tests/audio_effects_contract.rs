use std::collections::BTreeMap;

use superi_audio::effects::{
    Compressor, CompressorConfig, DelayConfig, DelayEffect, Equalizer, EqualizerBand,
    LimiterConfig, PeakLimiter, Saturator, SaturatorConfig,
};
use superi_audio::graph::{
    AudioEdge, AudioEdgeId, AudioGraph, AudioGraphId, AudioNode, AudioNodeId, AudioProcessBlock,
    AudioProcessor, PreparedAudioGraph,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, Result};
use superi_core::pixel::ChannelLayout;
use superi_core::time::SampleTime;

const RATE: u32 = 48_000;

struct Samples(Vec<f32>);

impl AudioProcessor for Samples {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        let channels = block.output_layout.len();
        let start = usize::try_from(block.start_time.sample()).unwrap() * channels;
        let end = start + block.output.len();
        block.output.copy_from_slice(&self.0[start..end]);
        Ok(())
    }
}

fn prepared(samples: Vec<f32>, effect: impl AudioProcessor + 'static) -> PreparedAudioGraph {
    let stereo = ChannelLayout::stereo();
    let mut graph = AudioGraph::new(AudioGraphId::from_raw(8), RATE, 512).unwrap();
    graph
        .insert_node(AudioNode::new(
            AudioNodeId::from_raw(1),
            None,
            stereo.clone(),
        ))
        .unwrap();
    graph
        .insert_node(AudioNode::new(
            AudioNodeId::from_raw(2),
            Some(stereo.clone()),
            stereo,
        ))
        .unwrap();
    graph
        .insert_edge(AudioEdge::new(
            AudioEdgeId::from_raw(1),
            AudioNodeId::from_raw(1),
            AudioNodeId::from_raw(2),
        ))
        .unwrap();
    let mut processors: BTreeMap<AudioNodeId, Box<dyn AudioProcessor>> = BTreeMap::new();
    processors.insert(AudioNodeId::from_raw(1), Box::new(Samples(samples)));
    processors.insert(AudioNodeId::from_raw(2), Box::new(effect));
    graph.prepare(AudioNodeId::from_raw(2), processors).unwrap()
}

fn render(mut graph: PreparedAudioGraph, blocks: &[usize]) -> Vec<f32> {
    let _audio = ExecutionDomain::Audio.enter_current().unwrap();
    let mut start = 0_i64;
    let mut result = Vec::new();
    for &frames in blocks {
        let mut output = vec![0.0; frames * 2];
        graph
            .process(SampleTime::new(start, RATE).unwrap(), frames, &mut output)
            .unwrap();
        result.extend(output);
        start += i64::try_from(frames).unwrap();
    }
    assert_eq!(graph.next_sample(), Some(start));
    result
}

fn assert_close(left: &[f32], right: &[f32], tolerance: f32) {
    assert_eq!(left.len(), right.len());
    for (index, (left, right)) in left.iter().zip(right).enumerate() {
        assert!(
            (left - right).abs() <= tolerance,
            "sample {index}: {left} != {right}"
        );
    }
}

#[test]
fn equalizer_is_stable_channel_preserving_and_partition_independent() {
    let stereo = ChannelLayout::stereo();
    let bands = vec![
        EqualizerBand::high_pass(80.0, 0.707).unwrap(),
        EqualizerBand::peaking(1_000.0, 1.0, 9.0).unwrap(),
        EqualizerBand::high_shelf(8_000.0, 0.707, -3.0).unwrap(),
    ];
    let frames = 480;
    let mut input = Vec::with_capacity(frames * 2);
    for frame in 0..frames {
        let tone = (std::f32::consts::TAU * 1_000.0 * frame as f32 / RATE as f32).sin() * 0.1;
        input.extend([tone, tone * 0.5]);
    }

    let whole = render(
        prepared(
            input.clone(),
            Equalizer::new(stereo.clone(), RATE, bands.clone()).unwrap(),
        ),
        &[480],
    );
    let split = render(
        prepared(input, Equalizer::new(stereo, RATE, bands).unwrap()),
        &[127, 113, 240],
    );
    assert_close(&whole, &split, 1.0e-6);
    assert!(whole.iter().all(|sample| sample.is_finite()));
    for frame in whole.chunks_exact(2).skip(200) {
        assert!((frame[1] - frame[0] * 0.5).abs() < 1.0e-5);
    }
    let input_rms = 0.1 / 2.0_f32.sqrt();
    let output_rms = (whole[400..]
        .iter()
        .step_by(2)
        .map(|sample| sample * sample)
        .sum::<f32>()
        / 280.0)
        .sqrt();
    assert!(output_rms > input_rms * 1.8);

    let nyquist_band = EqualizerBand::peaking(24_000.0, 1.0, 3.0).unwrap();
    assert_eq!(
        Equalizer::new(ChannelLayout::stereo(), RATE, vec![nyquist_band])
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        Equalizer::new(ChannelLayout::mono(), 0, Vec::new())
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
}

#[test]
fn compressor_and_limiter_link_channels_and_hold_the_peak_ceiling() {
    let stereo = ChannelLayout::stereo();
    let input = [0.1, 0.05, 0.8, 0.4, -1.0, -0.5, 0.25, 0.125].repeat(2);
    let compressor = Compressor::new(
        stereo.clone(),
        RATE,
        CompressorConfig::new(-12.0, 4.0, 0.0, 0.0, 0.0, 0.0).unwrap(),
    )
    .unwrap();
    let compressed = render(prepared(input.clone(), compressor), &[3, 5]);
    for frame in compressed.chunks_exact(2) {
        assert!((frame[1] - frame[0] * 0.5).abs() < 1.0e-6);
    }
    assert!(compressed[4].abs() < input[4].abs());

    let ceiling_db = -6.0;
    let ceiling = 10.0_f32.powf(ceiling_db / 20.0);
    let limiter =
        PeakLimiter::new(stereo, RATE, LimiterConfig::new(ceiling_db, 0.05).unwrap()).unwrap();
    let limited = render(prepared(input, limiter), &[8]);
    assert!(limited
        .iter()
        .all(|sample| sample.abs() <= ceiling + 1.0e-6));
    for frame in limited.chunks_exact(2) {
        assert!((frame[1] - frame[0] * 0.5).abs() < 1.0e-6);
    }
}

#[test]
fn delay_and_saturation_preserve_exact_timing_continuity_and_channel_meaning() {
    let stereo = ChannelLayout::stereo();
    let input = vec![1.0, 0.0, 0.0, -0.5, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
    let config = DelayConfig::new(3, 0.5, 1.0).unwrap();
    let whole = render(
        prepared(
            input.clone(),
            DelayEffect::new(stereo.clone(), config).unwrap(),
        ),
        &[6],
    );
    let split = render(
        prepared(input, DelayEffect::new(stereo.clone(), config).unwrap()),
        &[2, 1, 3],
    );
    assert_eq!(whole, split);
    assert_eq!(
        whole,
        [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, -0.5, 0.0, 0.0]
    );

    let saturation_input = [-2.0, 2.0, -0.5, 0.5, 0.0, 0.0].repeat(2);
    let saturated = render(
        prepared(
            saturation_input,
            Saturator::new(stereo, SaturatorConfig::new(4.0, 1.0).unwrap()).unwrap(),
        ),
        &[6],
    );
    assert!(saturated
        .iter()
        .all(|sample| sample.is_finite() && sample.abs() <= 1.0));
    for frame in saturated.chunks_exact(2) {
        assert!((frame[0] + frame[1]).abs() < 1.0e-6);
    }

    assert_eq!(
        DelayConfig::new(0, 0.0, 1.0).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        SaturatorConfig::new(f32::NAN, 1.0).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
}

#[test]
fn processors_reject_non_finite_input_before_advancing_the_graph_clock() {
    let stereo = ChannelLayout::stereo();
    let input = vec![0.0, 0.0, f32::NAN, 0.0];
    let mut graph = prepared(
        input,
        PeakLimiter::new(stereo, RATE, LimiterConfig::new(-1.0, 0.05).unwrap()).unwrap(),
    );
    let _audio = ExecutionDomain::Audio.enter_current().unwrap();
    let error = graph
        .process(SampleTime::new(0, RATE).unwrap(), 2, &mut [0.0; 4])
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(graph.next_sample(), None);
}
