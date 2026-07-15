use std::collections::BTreeMap;

use superi_audio::graph::{
    AudioBusKind, AudioEdge, AudioEdgeId, AudioGraph, AudioGraphId, AudioNode, AudioNodeId,
    AudioProcessBlock, AudioProcessor,
};
use superi_audio::metering::{MeterConfig, PreparedMeter};
use superi_audio::routing::SummingBus;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, Result};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::SampleTime;

const RATE: u32 = 48_000;
const BLOCK: usize = 480;

struct StereoTone {
    frequency: f64,
    amplitude: f64,
    initial_phase: f64,
    right_phase: f64,
}

impl AudioProcessor for StereoTone {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        for (offset, frame) in block.output.chunks_exact_mut(2).enumerate() {
            let sample = block.start_time.sample() + i64::try_from(offset).unwrap();
            let phase = self.initial_phase
                + std::f64::consts::TAU * self.frequency * sample as f64 / f64::from(RATE);
            frame[0] = (self.amplitude * phase.sin()) as f32;
            frame[1] = (self.amplitude * (phase + self.right_phase).sin()) as f32;
        }
        Ok(())
    }
}

fn prepared_tone(
    tone: StereoTone,
) -> (
    superi_audio::graph::PreparedAudioGraph,
    superi_audio::metering::MeterReadings,
) {
    let stereo = ChannelLayout::stereo();
    let config = MeterConfig::new(RATE, stereo.clone(), BLOCK, 256)
        .unwrap()
        .with_spectrum(256, 64)
        .unwrap();
    let (meter, readings) = PreparedMeter::new(config).unwrap();

    let source = AudioNodeId::from_raw(1);
    let meter_node = AudioNodeId::from_raw(2);
    let master = AudioNodeId::from_raw(3);
    let mut graph = AudioGraph::new(AudioGraphId::from_raw(9), RATE, BLOCK).unwrap();
    graph
        .insert_node(AudioNode::new(source, None, stereo.clone()))
        .unwrap();
    graph
        .insert_node(AudioNode::new(
            meter_node,
            Some(stereo.clone()),
            stereo.clone(),
        ))
        .unwrap();
    graph
        .insert_node(AudioNode::bus(master, AudioBusKind::Master, stereo))
        .unwrap();
    graph
        .insert_edge(AudioEdge::new(AudioEdgeId::from_raw(1), source, meter_node))
        .unwrap();
    graph
        .insert_edge(AudioEdge::new(AudioEdgeId::from_raw(2), meter_node, master))
        .unwrap();

    let mut processors: BTreeMap<AudioNodeId, Box<dyn AudioProcessor>> = BTreeMap::new();
    processors.insert(source, Box::new(tone));
    processors.insert(meter_node, Box::new(meter));
    processors.insert(master, Box::new(SummingBus));
    (graph.prepare_master(processors).unwrap(), readings)
}

#[test]
fn meter_is_a_sample_exact_graph_consumer_and_publishes_level_phase_and_spectrum() {
    let (mut graph, readings) = prepared_tone(StereoTone {
        frequency: 1_500.0,
        amplitude: 0.5,
        initial_phase: 0.0,
        right_phase: 0.0,
    });
    let _audio = ExecutionDomain::Audio.enter_current().unwrap();
    let mut output = vec![0.0; BLOCK * 2];
    graph
        .process(SampleTime::new(0, RATE).unwrap(), BLOCK, &mut output)
        .unwrap();

    for (frame, samples) in output.chunks_exact(2).enumerate() {
        let phase = std::f64::consts::TAU * 1_500.0 * frame as f64 / f64::from(RATE);
        assert!((samples[0] - (0.5 * phase.sin()) as f32).abs() < 1.0e-6);
        assert!((samples[1] - (0.5 * phase.sin()) as f32).abs() < 1.0e-6);
    }

    let snapshot = readings.snapshot();
    assert_eq!(snapshot.start_time(), SampleTime::new(0, RATE).unwrap());
    assert_eq!(snapshot.frame_count(), BLOCK);
    assert_eq!(snapshot.channels().len(), 2);
    assert_eq!(
        snapshot.channels()[0].position(),
        ChannelPosition::FrontLeft
    );
    assert!((snapshot.channels()[0].sample_peak() - 0.5).abs() < 1.0e-6);
    assert!((snapshot.channels()[0].rms() - 0.5 / 2.0_f64.sqrt()).abs() < 1.0e-4);
    assert!(snapshot.channels()[0].true_peak() >= snapshot.channels()[0].sample_peak());
    assert!(snapshot.phase_correlation().unwrap() > 0.999);
    let strongest = snapshot
        .spectrum()
        .iter()
        .max_by(|left, right| left.magnitude().total_cmp(&right.magnitude()))
        .unwrap();
    assert!((strongest.center_hz() - 1_500.0).abs() <= 190.0);
}

#[test]
fn true_peak_finds_intersample_headroom_above_the_largest_pcm_sample() {
    let (mut graph, readings) = prepared_tone(StereoTone {
        frequency: 12_000.0,
        amplitude: 1.0,
        initial_phase: std::f64::consts::FRAC_PI_4,
        right_phase: 0.0,
    });
    let _audio = ExecutionDomain::Audio.enter_current().unwrap();
    let mut output = vec![0.0; BLOCK * 2];
    graph
        .process(SampleTime::new(1, RATE).unwrap(), BLOCK, &mut output)
        .unwrap();
    let snapshot = readings.snapshot();
    let channel = &snapshot.channels()[0];
    assert!(channel.sample_peak() < 0.708);
    assert!(channel.true_peak() > 0.95);
    assert!(channel.maximum_true_peak() >= channel.true_peak());
}

#[test]
fn ebu_windows_and_itu_gating_publish_calibrated_loudness_without_changing_continuity() {
    let amplitude = 10.0_f64.powf(-18.0 / 20.0);
    let (mut graph, readings) = prepared_tone(StereoTone {
        frequency: 1_000.0,
        amplitude,
        initial_phase: 0.0,
        right_phase: 0.0,
    });
    let _audio = ExecutionDomain::Audio.enter_current().unwrap();
    let mut output = vec![0.0; BLOCK * 2];
    for block in 0..300 {
        let start = i64::try_from(block * BLOCK).unwrap();
        graph
            .process(SampleTime::new(start, RATE).unwrap(), BLOCK, &mut output)
            .unwrap();
    }

    let snapshot = readings.snapshot();
    assert_eq!(snapshot.programme_blocks(), 27);
    assert!(!snapshot.programme_history_saturated());
    assert!((snapshot.momentary_lufs().unwrap() + 18.0).abs() < 0.2);
    assert!((snapshot.short_term_lufs().unwrap() + 18.0).abs() < 0.2);
    assert!((snapshot.integrated_lufs().unwrap() + 18.0).abs() < 0.2);
    assert_eq!(graph.next_sample(), Some(144_000));
}

#[test]
fn preparation_rejects_unbounded_or_nonsensical_meter_storage() {
    let error = MeterConfig::new(RATE, ChannelLayout::stereo(), 0, 10).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    let error = MeterConfig::new(RATE, ChannelLayout::stereo(), BLOCK, 0).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    let error = MeterConfig::new(RATE, ChannelLayout::stereo(), BLOCK, 1_000_001).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    let config = MeterConfig::new(RATE, ChannelLayout::stereo(), BLOCK, 10).unwrap();
    assert_eq!(
        config.with_spectrum(1, 2).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    let one_frame = MeterConfig::new(RATE, ChannelLayout::stereo(), 1, 1).unwrap();
    PreparedMeter::new(one_frame).unwrap();
}
