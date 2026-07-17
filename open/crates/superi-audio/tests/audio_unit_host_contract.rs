use superi_audio::hosting::{
    AudioUnitComponentId, AudioUnitExecutionPolicy, AudioUnitHostConfig, PreparedAudioUnit,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::ErrorCategory;
use superi_core::pixel::{ChannelLayout, ChannelPosition};

const RATE: u32 = 48_000;

fn peak_limiter() -> AudioUnitComponentId {
    AudioUnitComponentId::effect(*b"lmtr", *b"appl").unwrap()
}

fn stereo_config(policy: AudioUnitExecutionPolicy) -> AudioUnitHostConfig {
    AudioUnitHostConfig::new(
        peak_limiter(),
        RATE,
        256,
        ChannelLayout::stereo(),
        ChannelLayout::stereo(),
        policy,
    )
    .unwrap()
}

#[test]
fn component_identity_and_prepared_configuration_are_exact_and_bounded() {
    let component = peak_limiter();
    assert_eq!(component.component_type(), *b"aufx");
    assert_eq!(component.subtype(), *b"lmtr");
    assert_eq!(component.manufacturer(), *b"appl");
    assert_eq!(component.raw_component_type(), u32::from_be_bytes(*b"aufx"));
    assert_eq!(component.raw_subtype(), u32::from_be_bytes(*b"lmtr"));
    assert_eq!(component.raw_manufacturer(), u32::from_be_bytes(*b"appl"));
    assert_eq!(
        AudioUnitComponentId::effect([0; 4], *b"appl")
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );

    let config = stereo_config(AudioUnitExecutionPolicy::RequireOutOfProcess);
    assert_eq!(config.component(), component);
    assert_eq!(config.sample_rate(), RATE);
    assert_eq!(config.maximum_frames(), 256);
    assert_eq!(config.input_layout(), &ChannelLayout::stereo());
    assert_eq!(config.output_layout(), &ChannelLayout::stereo());
    assert_eq!(
        config.execution_policy(),
        AudioUnitExecutionPolicy::RequireOutOfProcess
    );
    assert_eq!(
        AudioUnitExecutionPolicy::default(),
        AudioUnitExecutionPolicy::RequireOutOfProcess
    );
}

#[test]
fn configuration_rejects_timing_and_channel_contract_ambiguity() {
    let invalid_rate = AudioUnitHostConfig::new(
        peak_limiter(),
        0,
        256,
        ChannelLayout::stereo(),
        ChannelLayout::stereo(),
        AudioUnitExecutionPolicy::default(),
    )
    .unwrap_err();
    assert_eq!(invalid_rate.category(), ErrorCategory::InvalidInput);

    let invalid_maximum = AudioUnitHostConfig::new(
        peak_limiter(),
        RATE,
        0,
        ChannelLayout::stereo(),
        ChannelLayout::stereo(),
        AudioUnitExecutionPolicy::default(),
    )
    .unwrap_err();
    assert_eq!(invalid_maximum.category(), ErrorCategory::InvalidInput);

    let mismatched_layouts = AudioUnitHostConfig::new(
        peak_limiter(),
        RATE,
        256,
        ChannelLayout::mono(),
        ChannelLayout::stereo(),
        AudioUnitExecutionPolicy::default(),
    )
    .unwrap_err();
    assert_eq!(mismatched_layouts.category(), ErrorCategory::InvalidInput);

    let too_many_channels = ChannelLayout::new((0..65).map(ChannelPosition::Discrete)).unwrap();
    let excessive_layout = AudioUnitHostConfig::new(
        peak_limiter(),
        RATE,
        256,
        too_many_channels.clone(),
        too_many_channels,
        AudioUnitExecutionPolicy::default(),
    )
    .unwrap_err();
    assert_eq!(
        excessive_layout.category(),
        ErrorCategory::ResourceExhausted
    );
}

#[test]
fn preparation_is_confined_to_the_blocking_background_domain() {
    let error = PreparedAudioUnit::prepare(stereo_config(
        AudioUnitExecutionPolicy::AllowAuditedInProcess,
    ))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
}

#[cfg(not(target_os = "macos"))]
#[test]
fn non_macos_preparation_reports_the_platform_boundary_explicitly() {
    let _background = ExecutionDomain::BackgroundJob.enter_current().unwrap();
    let error = PreparedAudioUnit::prepare(stereo_config(
        AudioUnitExecutionPolicy::AllowAuditedInProcess,
    ))
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);
}

#[cfg(target_os = "macos")]
mod macos {
    use std::collections::BTreeMap;

    use super::*;
    use superi_audio::graph::{
        AudioBusKind, AudioEdge, AudioEdgeId, AudioGraph, AudioGraphId, AudioNode, AudioNodeId,
        AudioProcessBlock, AudioProcessor, PreparedAudioGraph,
    };
    use superi_audio::routing::SummingBus;
    use superi_core::error::Result;
    use superi_core::time::SampleTime;

    const FRAMES: usize = 512;

    #[derive(Clone)]
    struct DeterministicStereo;

    impl AudioProcessor for DeterministicStereo {
        fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
            for (frame_offset, frame) in block.output.chunks_exact_mut(2).enumerate() {
                let sample = block.start_time.sample() + i64::try_from(frame_offset).unwrap();
                let phase = std::f32::consts::TAU * 997.0 * sample as f32 / RATE as f32;
                frame[0] = phase.sin() * 0.75;
                frame[1] = phase.cos() * 0.25;
            }
            Ok(())
        }
    }

    fn prepare_audio_unit() -> PreparedAudioUnit {
        let _background = ExecutionDomain::BackgroundJob.enter_current().unwrap();
        PreparedAudioUnit::prepare(stereo_config(
            AudioUnitExecutionPolicy::AllowAuditedInProcess,
        ))
        .unwrap()
    }

    #[test]
    fn required_out_of_process_instantiation_is_verified_from_the_native_instance() {
        let _background = ExecutionDomain::BackgroundJob.enter_current().unwrap();
        let unit = PreparedAudioUnit::prepare(stereo_config(
            AudioUnitExecutionPolicy::RequireOutOfProcess,
        ))
        .unwrap();
        assert!(unit.loaded_out_of_process());
        assert_eq!(unit.component(), peak_limiter());
    }

    #[test]
    fn class_info_state_and_native_latency_survive_a_fresh_instance() {
        let mut original = prepare_audio_unit();
        let original_version = original.component_version();
        let original_latency = original.latency_samples();
        let state = {
            let _background = ExecutionDomain::BackgroundJob.enter_current().unwrap();
            original.capture_state().unwrap()
        };
        assert!(!state.bytes().is_empty());

        let mut restored = {
            let _background = ExecutionDomain::BackgroundJob.enter_current().unwrap();
            PreparedAudioUnit::prepare(
                stereo_config(AudioUnitExecutionPolicy::AllowAuditedInProcess)
                    .with_initial_state(state),
            )
            .unwrap()
        };
        assert_eq!(restored.component_version(), original_version);
        assert_eq!(restored.latency_samples(), original_latency);
        let recaptured = {
            let _background = ExecutionDomain::BackgroundJob.enter_current().unwrap();
            restored.capture_state().unwrap()
        };
        assert!(!recaptured.bytes().is_empty());
    }

    fn graph(audio_unit: PreparedAudioUnit) -> PreparedAudioGraph {
        let stereo = ChannelLayout::stereo();
        let source = AudioNodeId::from_raw(1);
        let effect = AudioNodeId::from_raw(2);
        let master = AudioNodeId::from_raw(3);
        let mut graph = AudioGraph::new(AudioGraphId::from_raw(13), RATE, 256).unwrap();
        graph
            .insert_node(AudioNode::new(source, None, stereo.clone()))
            .unwrap();
        graph
            .insert_node(AudioNode::new(effect, Some(stereo.clone()), stereo.clone()))
            .unwrap();
        graph
            .insert_node(AudioNode::bus(master, AudioBusKind::Master, stereo))
            .unwrap();
        graph
            .insert_edge(AudioEdge::new(AudioEdgeId::from_raw(1), source, effect))
            .unwrap();
        graph
            .insert_edge(AudioEdge::new(AudioEdgeId::from_raw(2), effect, master))
            .unwrap();
        let mut processors: BTreeMap<AudioNodeId, Box<dyn AudioProcessor>> = BTreeMap::new();
        processors.insert(source, Box::new(DeterministicStereo));
        processors.insert(effect, Box::new(audio_unit));
        processors.insert(master, Box::new(SummingBus));
        graph.prepare_master(processors).unwrap()
    }

    fn render(mut graph: PreparedAudioGraph, partitions: &[usize]) -> Vec<f32> {
        let _audio = ExecutionDomain::Audio.enter_current().unwrap();
        let mut start = 0_i64;
        let mut output = Vec::with_capacity(FRAMES * 2);
        for &frames in partitions {
            let mut block = vec![0.0; frames * 2];
            graph
                .process(SampleTime::new(start, RATE).unwrap(), frames, &mut block)
                .unwrap();
            output.extend(block);
            start += i64::try_from(frames).unwrap();
        }
        assert_eq!(start, FRAMES as i64);
        assert_eq!(graph.next_sample(), Some(FRAMES as i64));
        output
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
    fn apple_peak_limiter_runs_through_the_graph_and_final_mix_continuously() {
        let whole_unit = prepare_audio_unit();
        assert_eq!(whole_unit.component(), peak_limiter());
        assert!(whole_unit.component_version() > 0);
        assert!(!whole_unit.loaded_out_of_process());
        assert!(!whole_unit.is_poisoned());
        let whole = render(graph(whole_unit), &[256, 256]);

        let split = render(graph(prepare_audio_unit()), &[73, 127, 56, 64, 192]);
        assert_close(&whole, &split, 1.0e-5);
        assert!(whole.iter().all(|sample| sample.is_finite()));
        assert!(whole.iter().any(|sample| sample.abs() > 1.0e-4));
        assert!(whole
            .chunks_exact(2)
            .any(|frame| (frame[0] - frame[1]).abs() > 1.0e-3));
    }

    #[test]
    fn unrepresentable_sample_time_keeps_graph_time_and_output_uncommitted() {
        let stereo = ChannelLayout::stereo();
        let source = AudioNodeId::from_raw(1);
        let effect = AudioNodeId::from_raw(2);
        let mut graph = AudioGraph::new(AudioGraphId::from_raw(14), RATE, 8).unwrap();
        graph
            .insert_node(AudioNode::new(source, None, stereo.clone()))
            .unwrap();
        graph
            .insert_node(AudioNode::new(effect, Some(stereo.clone()), stereo))
            .unwrap();
        graph
            .insert_edge(AudioEdge::new(AudioEdgeId::from_raw(1), source, effect))
            .unwrap();
        let mut processors: BTreeMap<AudioNodeId, Box<dyn AudioProcessor>> = BTreeMap::new();
        processors.insert(source, Box::new(DeterministicStereo));
        processors.insert(effect, Box::new(prepare_audio_unit()));
        let mut prepared = graph.prepare(effect, processors).unwrap();

        let _audio = ExecutionDomain::Audio.enter_current().unwrap();
        let mut output = [7.0; 8];
        let error = prepared
            .process(
                SampleTime::new((1_i64 << 53) + 1, RATE).unwrap(),
                4,
                &mut output,
            )
            .unwrap_err();
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
        assert_eq!(prepared.next_sample(), None);
        assert_eq!(output, [7.0; 8]);
    }
}
