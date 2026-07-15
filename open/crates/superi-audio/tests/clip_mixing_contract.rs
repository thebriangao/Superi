use std::collections::BTreeMap;

use superi_audio::graph::{
    AudioEdge, AudioEdgeId, AudioGraph, AudioGraphId, AudioNode, AudioNodeId, AudioProcessBlock,
    AudioProcessor,
};
use superi_audio::mixing::{ChannelMap, ClipMixControls, ClipMixMutation, ClipMixState};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, Result};
use superi_core::ids::ClipId;
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::SampleTime;

struct ConstantSource(Vec<f32>);

impl AudioProcessor for ConstantSource {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        assert!(block.input.is_none());
        for frame in block.output.chunks_exact_mut(self.0.len()) {
            frame.copy_from_slice(&self.0);
        }
        Ok(())
    }
}

fn route(source: ChannelPosition, destination: ChannelPosition) -> ChannelMap {
    ChannelMap::new(source, destination, 1.0).unwrap()
}

fn render(
    controls: ClipMixControls,
    state_controls: &[ClipMixControls],
    input: Vec<f32>,
    blocks: &[(i64, usize)],
) -> Vec<f32> {
    let clip_id = ClipId::from_raw(10);
    let mut state = ClipMixState::new();
    let mut mutations = Vec::new();
    mutations.push(ClipMixMutation::set(clip_id, controls.clone()));
    for (index, sibling) in state_controls.iter().cloned().enumerate() {
        mutations.push(ClipMixMutation::set(
            ClipId::from_raw(100 + index as u128),
            sibling,
        ));
    }
    state.apply(0, &mutations).unwrap();
    let snapshot = state.snapshot();

    let mut graph = AudioGraph::new(AudioGraphId::from_raw(1), 48_000, 4).unwrap();
    graph
        .insert_node(AudioNode::new(
            AudioNodeId::from_raw(1),
            None,
            controls.input_layout().clone(),
        ))
        .unwrap();
    graph
        .insert_node(AudioNode::new(
            AudioNodeId::from_raw(2),
            Some(controls.input_layout().clone()),
            controls.output_layout().clone(),
        ))
        .unwrap();
    graph
        .insert_edge(AudioEdge::new(
            AudioEdgeId::from_raw(1),
            AudioNodeId::from_raw(1),
            AudioNodeId::from_raw(2),
        ))
        .unwrap();

    let total_frames = blocks.iter().map(|(_, frames)| *frames as u64).sum();
    let processor = snapshot
        .prepare_processor(
            clip_id,
            SampleTime::new(blocks[0].0, 48_000).unwrap(),
            total_frames,
        )
        .unwrap();
    let mut processors: BTreeMap<AudioNodeId, Box<dyn AudioProcessor>> = BTreeMap::new();
    processors.insert(AudioNodeId::from_raw(1), Box::new(ConstantSource(input)));
    processors.insert(AudioNodeId::from_raw(2), Box::new(processor));
    let mut prepared = graph.prepare(AudioNodeId::from_raw(2), processors).unwrap();

    let _audio = ExecutionDomain::Audio.enter_current().unwrap();
    let mut rendered = Vec::new();
    for (start, frames) in blocks {
        let mut output = vec![0.0; frames * controls.output_layout().len()];
        prepared
            .process(
                SampleTime::new(*start, 48_000).unwrap(),
                *frames,
                &mut output,
            )
            .unwrap();
        rendered.extend(output);
    }
    rendered
}

#[test]
fn gain_fades_channel_mapping_and_phase_are_exact_across_blocks() {
    let stereo = ChannelLayout::stereo();
    let controls = ClipMixControls::new(
        stereo.clone(),
        stereo,
        [
            route(ChannelPosition::FrontLeft, ChannelPosition::FrontRight),
            route(ChannelPosition::FrontRight, ChannelPosition::FrontLeft),
        ],
    )
    .unwrap()
    .with_gain(0.5)
    .unwrap()
    .with_fades(3, 3)
    .unwrap()
    .with_phase_inverted([ChannelPosition::FrontRight])
    .unwrap();

    let rendered = render(controls, &[], vec![1.0, 2.0], &[(100, 4), (104, 4)]);
    assert_eq!(
        rendered,
        [0.0, 0.0, 0.5, -0.25, 1.0, -0.5, 1.0, -0.5, 1.0, -0.5, 1.0, -0.5, 0.5, -0.25, 0.0, 0.0,]
    );
}

#[test]
fn pan_mute_and_project_wide_solo_are_applied_from_one_snapshot() {
    let stereo = ChannelLayout::stereo();
    let routes = [
        route(ChannelPosition::FrontLeft, ChannelPosition::FrontLeft),
        route(ChannelPosition::FrontRight, ChannelPosition::FrontRight),
    ];
    let panned = ClipMixControls::new(stereo.clone(), stereo.clone(), routes)
        .unwrap()
        .with_pan(1.0)
        .unwrap();
    assert_eq!(
        render(panned.clone(), &[], vec![1.0, 2.0], &[(0, 2)]),
        [0.0, 3.0, 0.0, 3.0]
    );

    let muted = panned.clone().with_muted(true);
    assert_eq!(render(muted, &[], vec![1.0, 2.0], &[(0, 2)]), [0.0; 4]);

    let soloed_sibling = ClipMixControls::new(stereo.clone(), stereo, routes)
        .unwrap()
        .with_solo(true);
    assert_eq!(
        render(panned, &[soloed_sibling], vec![1.0, 2.0], &[(0, 2)]),
        [0.0; 4]
    );
}

#[test]
fn state_mutations_are_atomic_revisioned_and_preserve_user_intent() {
    let original = ClipId::from_raw(1);
    let fragment = ClipId::from_raw(2);
    let replacement = ClipId::from_raw(3);
    let stereo = ChannelLayout::stereo();
    let controls = ClipMixControls::new(
        stereo.clone(),
        stereo,
        [
            route(ChannelPosition::FrontLeft, ChannelPosition::FrontLeft),
            route(ChannelPosition::FrontRight, ChannelPosition::FrontRight),
        ],
    )
    .unwrap()
    .with_gain(0.75)
    .unwrap()
    .with_fades(12, 24)
    .unwrap()
    .with_pan(-0.25)
    .unwrap()
    .with_solo(true);
    let mut state = ClipMixState::new();

    assert_eq!(
        state
            .apply(0, &[ClipMixMutation::set(original, controls.clone())])
            .unwrap(),
        1
    );
    assert_eq!(
        state
            .apply(1, &[ClipMixMutation::inherit(original, fragment)])
            .unwrap(),
        2
    );
    assert_eq!(state.controls(fragment), Some(&controls));

    assert_eq!(
        state
            .apply(2, &[ClipMixMutation::transfer(fragment, replacement)])
            .unwrap(),
        3
    );
    assert!(state.controls(fragment).is_none());
    assert_eq!(state.controls(replacement), Some(&controls));

    let before = state.clone();
    let error = state
        .apply(
            3,
            &[
                ClipMixMutation::remove(replacement),
                ClipMixMutation::inherit(ClipId::from_raw(999), fragment),
            ],
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotFound);
    assert_eq!(state, before);

    let error = state
        .apply(2, &[ClipMixMutation::remove(replacement)])
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(state, before);
}

#[test]
fn invalid_controls_and_out_of_clip_processing_fail_closed() {
    let stereo = ChannelLayout::stereo();
    let error = ClipMixControls::new(
        stereo.clone(),
        stereo.clone(),
        [route(
            ChannelPosition::FrontCenter,
            ChannelPosition::FrontLeft,
        )],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let controls = ClipMixControls::new(
        stereo.clone(),
        stereo.clone(),
        [
            route(ChannelPosition::FrontLeft, ChannelPosition::FrontLeft),
            route(ChannelPosition::FrontRight, ChannelPosition::FrontRight),
        ],
    )
    .unwrap();
    assert_eq!(
        controls.clone().with_gain(f32::NAN).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        controls.clone().with_pan(1.01).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        controls.clone().with_fades(1, 5).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );

    let clip_id = ClipId::from_raw(77);
    let mut state = ClipMixState::new();
    state
        .apply(
            0,
            &[ClipMixMutation::set(
                clip_id,
                controls.clone().with_fades(4, 0).unwrap(),
            )],
        )
        .unwrap();
    let snapshot = state.snapshot();
    assert_eq!(
        snapshot
            .prepare_processor(clip_id, SampleTime::new(10, 48_000).unwrap(), 3)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );

    let mut processor = ClipMixState::new();
    processor
        .apply(0, &[ClipMixMutation::set(clip_id, controls)])
        .unwrap();
    let mut processor = processor
        .snapshot()
        .prepare_processor(clip_id, SampleTime::new(10, 48_000).unwrap(), 4)
        .unwrap();
    let input = [1.0; 4];
    let mut output = [0.0; 4];
    let error = processor
        .process(AudioProcessBlock {
            start_time: SampleTime::new(9, 48_000).unwrap(),
            frame_count: 2,
            input: Some(&input),
            input_layout: Some(&stereo),
            output: &mut output,
            output_layout: &stereo,
        })
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let short_input = [1.0; 3];
    let error = processor
        .process(AudioProcessBlock {
            start_time: SampleTime::new(10, 48_000).unwrap(),
            frame_count: 2,
            input: Some(&short_input),
            input_layout: Some(&stereo),
            output: &mut output,
            output_layout: &stereo,
        })
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
}
