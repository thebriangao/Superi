use std::collections::BTreeMap;

use superi_audio::automation::{
    AudioAutomationKeyframe, AudioAutomationMode, AudioAutomationMutation, AudioAutomationState,
    AudioAutomationTarget, AudioAutomationTransaction, MAX_AUDIO_AUTOMATION_KEYFRAMES,
    MAX_AUDIO_AUTOMATION_LANES, MAX_AUDIO_AUTOMATION_MUTATIONS,
};
use superi_audio::graph::{
    AudioBusKind, AudioEdge, AudioEdgeId, AudioGraph, AudioGraphId, AudioNode, AudioNodeId,
    AudioProcessBlock, AudioProcessor,
};
use superi_audio::mixing::{ChannelMap, ClipMixControls, ClipMixMutation, ClipMixState};
use superi_audio::routing::SummingBus;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, Result};
use superi_core::ids::ClipId;
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::SampleTime;

const RATE: u32 = 48_000;
const CLIP: ClipId = ClipId::from_raw(0xa011);

fn at(sample: i64) -> SampleTime {
    SampleTime::new(sample, RATE).unwrap()
}

fn target() -> AudioAutomationTarget {
    AudioAutomationTarget::clip_gain(CLIP)
}

fn keyframe(sample: i64, value: f32) -> AudioAutomationKeyframe {
    AudioAutomationKeyframe::new(at(sample), value).unwrap()
}

fn apply(
    state: &mut AudioAutomationState,
    mutations: Vec<AudioAutomationMutation>,
) -> superi_audio::automation::AudioAutomationSnapshot {
    state
        .apply(AudioAutomationTransaction::new(state.revision(), mutations).unwrap())
        .unwrap()
}

fn lane_with_baseline() -> AudioAutomationState {
    let mut state = AudioAutomationState::new();
    apply(
        &mut state,
        vec![AudioAutomationMutation::CreateLane {
            target: target(),
            sample_rate: RATE,
            default_gain: 0.0,
        }],
    );
    apply(
        &mut state,
        vec![
            AudioAutomationMutation::SetKeyframe {
                target: target(),
                keyframe: keyframe(0, 0.0),
            },
            AudioAutomationMutation::SetKeyframe {
                target: target(),
                keyframe: keyframe(10, 1.0),
            },
            AudioAutomationMutation::SetKeyframe {
                target: target(),
                keyframe: keyframe(20, 0.0),
            },
        ],
    );
    state
}

fn curve_value(state: &AudioAutomationState, sample: i64) -> f32 {
    state
        .snapshot()
        .prepare_curve(target(), RATE)
        .unwrap()
        .unwrap()
        .gain_at_sample(sample)
}

#[test]
fn keyframes_are_revisioned_canonical_and_sample_exact_in_read_mode() {
    assert_eq!(MAX_AUDIO_AUTOMATION_MUTATIONS, 64);
    assert_eq!(MAX_AUDIO_AUTOMATION_LANES, 4_096);
    assert_eq!(MAX_AUDIO_AUTOMATION_KEYFRAMES, 1_048_576);

    let mut state = AudioAutomationState::new();
    let first = apply(
        &mut state,
        vec![AudioAutomationMutation::CreateLane {
            target: target(),
            sample_rate: RATE,
            default_gain: 1.0,
        }],
    );
    assert_eq!(first.revision(), 1);
    assert_eq!(
        first.lane(target()).unwrap().mode(),
        AudioAutomationMode::Read
    );

    let second = apply(
        &mut state,
        vec![
            AudioAutomationMutation::SetKeyframe {
                target: target(),
                keyframe: keyframe(-4, 0.0),
            },
            AudioAutomationMutation::SetKeyframe {
                target: target(),
                keyframe: keyframe(4, 2.0),
            },
        ],
    );
    let lane = second.lane(target()).unwrap();
    assert_eq!(lane.keyframes(), [keyframe(-4, 0.0), keyframe(4, 2.0)]);
    let curve = second.prepare_curve(target(), RATE).unwrap().unwrap();
    assert_eq!(curve.gain_at_sample(-5), 1.0);
    assert_eq!(curve.gain_at_sample(-4), 0.0);
    assert_eq!(curve.gain_at_sample(0), 1.0);
    assert_eq!(curve.gain_at_sample(4), 2.0);
    assert_eq!(curve.gain_at_sample(5), 2.0);

    let before = state.snapshot();
    let no_op = apply(
        &mut state,
        vec![AudioAutomationMutation::SetKeyframe {
            target: target(),
            keyframe: keyframe(4, 2.0),
        }],
    );
    assert_eq!(no_op, before);

    let error = state
        .apply(
            AudioAutomationTransaction::new(
                state.revision(),
                vec![AudioAutomationMutation::BeginPass {
                    target: target(),
                    at: at(0),
                    current_value: 1.0,
                }],
            )
            .unwrap(),
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(state.snapshot(), before);
    assert_eq!(
        second
            .prepare_curve(target(), 44_100)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
}

#[test]
fn write_touch_and_latch_replace_only_their_exact_half_open_regions() {
    let mut write = lane_with_baseline();
    apply(
        &mut write,
        vec![AudioAutomationMutation::SetMode {
            target: target(),
            mode: AudioAutomationMode::Write,
        }],
    );
    apply(
        &mut write,
        vec![AudioAutomationMutation::BeginPass {
            target: target(),
            at: at(4),
            current_value: 2.0,
        }],
    );
    let active = apply(
        &mut write,
        vec![AudioAutomationMutation::SetControlValue {
            target: target(),
            at: at(6),
            value: 4.0,
        }],
    );
    assert_eq!(
        active
            .lane(target())
            .unwrap()
            .active_pass()
            .unwrap()
            .start(),
        at(4)
    );
    assert_eq!(
        active
            .lane(target())
            .unwrap()
            .active_pass()
            .unwrap()
            .current_value(),
        4.0
    );
    apply(
        &mut write,
        vec![AudioAutomationMutation::EndPass {
            target: target(),
            at: at(8),
        }],
    );
    assert_eq!(curve_value(&write, 3), 0.3);
    assert_eq!(curve_value(&write, 4), 2.0);
    assert_eq!(curve_value(&write, 6), 4.0);
    assert_eq!(curve_value(&write, 7), 4.0);
    assert_eq!(curve_value(&write, 8), 0.8);

    let mut touch = lane_with_baseline();
    apply(
        &mut touch,
        vec![AudioAutomationMutation::SetMode {
            target: target(),
            mode: AudioAutomationMode::Touch,
        }],
    );
    apply(
        &mut touch,
        vec![AudioAutomationMutation::BeginPass {
            target: target(),
            at: at(2),
            current_value: 0.2,
        }],
    );
    apply(
        &mut touch,
        vec![AudioAutomationMutation::BeginTouch {
            target: target(),
            at: at(4),
            value: 2.0,
        }],
    );
    apply(
        &mut touch,
        vec![AudioAutomationMutation::SetControlValue {
            target: target(),
            at: at(6),
            value: 4.0,
        }],
    );
    apply(
        &mut touch,
        vec![AudioAutomationMutation::EndTouch {
            target: target(),
            at: at(8),
        }],
    );
    apply(
        &mut touch,
        vec![AudioAutomationMutation::BeginTouch {
            target: target(),
            at: at(12),
            value: 3.0,
        }],
    );
    apply(
        &mut touch,
        vec![AudioAutomationMutation::EndTouch {
            target: target(),
            at: at(14),
        }],
    );
    apply(
        &mut touch,
        vec![AudioAutomationMutation::EndPass {
            target: target(),
            at: at(16),
        }],
    );
    assert_eq!(curve_value(&touch, 3), 0.3);
    assert_eq!(curve_value(&touch, 4), 2.0);
    assert_eq!(curve_value(&touch, 7), 4.0);
    assert_eq!(curve_value(&touch, 8), 0.8);
    assert_eq!(curve_value(&touch, 11), 0.9);
    assert_eq!(curve_value(&touch, 12), 3.0);
    assert_eq!(curve_value(&touch, 13), 3.0);
    assert_eq!(curve_value(&touch, 14), 0.6);

    let mut latch = lane_with_baseline();
    apply(
        &mut latch,
        vec![AudioAutomationMutation::SetMode {
            target: target(),
            mode: AudioAutomationMode::Latch,
        }],
    );
    apply(
        &mut latch,
        vec![AudioAutomationMutation::BeginPass {
            target: target(),
            at: at(2),
            current_value: 0.2,
        }],
    );
    apply(
        &mut latch,
        vec![AudioAutomationMutation::BeginTouch {
            target: target(),
            at: at(4),
            value: 2.0,
        }],
    );
    apply(
        &mut latch,
        vec![AudioAutomationMutation::EndTouch {
            target: target(),
            at: at(6),
        }],
    );
    apply(
        &mut latch,
        vec![AudioAutomationMutation::EndPass {
            target: target(),
            at: at(8),
        }],
    );
    assert_eq!(curve_value(&latch, 3), 0.3);
    assert_eq!(curve_value(&latch, 4), 2.0);
    assert_eq!(curve_value(&latch, 7), 2.0);
    assert_eq!(curve_value(&latch, 8), 0.8);
}

#[test]
fn invalid_transactions_leave_revision_lanes_and_active_passes_unchanged() {
    let mut state = lane_with_baseline();
    apply(
        &mut state,
        vec![AudioAutomationMutation::SetMode {
            target: target(),
            mode: AudioAutomationMode::Touch,
        }],
    );
    let before_stale = state.snapshot();
    let stale = AudioAutomationTransaction::new(
        state.revision() - 1,
        vec![AudioAutomationMutation::RemoveKeyframe {
            target: target(),
            at: at(10),
        }],
    )
    .unwrap();
    assert_eq!(
        state.apply(stale).unwrap_err().category(),
        ErrorCategory::Conflict
    );
    assert_eq!(state.snapshot(), before_stale);

    apply(
        &mut state,
        vec![AudioAutomationMutation::BeginPass {
            target: target(),
            at: at(4),
            current_value: 0.4,
        }],
    );
    let before_invalid = state.snapshot();
    let wrong_clock = SampleTime::new(5, 44_100).unwrap();
    let invalid = AudioAutomationTransaction::new(
        state.revision(),
        vec![
            AudioAutomationMutation::BeginTouch {
                target: target(),
                at: at(6),
                value: 1.0,
            },
            AudioAutomationMutation::SetControlValue {
                target: target(),
                at: wrong_clock,
                value: 2.0,
            },
        ],
    )
    .unwrap();
    assert_eq!(
        state.apply(invalid).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(state.snapshot(), before_invalid);

    assert_eq!(
        AudioAutomationKeyframe::new(at(0), f32::NAN)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        AudioAutomationTransaction::new(0, Vec::new())
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );

    let before_overflow = state.snapshot();
    let overflow = AudioAutomationTransaction::new(
        state.revision(),
        vec![AudioAutomationMutation::BeginTouch {
            target: target(),
            at: at(i64::MAX),
            value: 1.0,
        }],
    )
    .unwrap();
    assert_eq!(
        state.apply(overflow).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(state.snapshot(), before_overflow);

    let mut write = lane_with_baseline();
    apply(
        &mut write,
        vec![AudioAutomationMutation::SetMode {
            target: target(),
            mode: AudioAutomationMode::Write,
        }],
    );
    let before_start_overflow = write.snapshot();
    let overflow = AudioAutomationTransaction::new(
        write.revision(),
        vec![AudioAutomationMutation::BeginPass {
            target: target(),
            at: at(i64::MAX),
            current_value: 1.0,
        }],
    )
    .unwrap();
    assert_eq!(
        write.apply(overflow).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(write.snapshot(), before_start_overflow);
}

struct ConstantSource;

impl AudioProcessor for ConstantSource {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        block.output.fill(1.0);
        Ok(())
    }
}

fn render_automated(blocks: &[usize]) -> Vec<f32> {
    let stereo = ChannelLayout::stereo();
    let routes = [
        ChannelMap::new(ChannelPosition::FrontLeft, ChannelPosition::FrontLeft, 1.0).unwrap(),
        ChannelMap::new(
            ChannelPosition::FrontRight,
            ChannelPosition::FrontRight,
            1.0,
        )
        .unwrap(),
    ];
    let controls = ClipMixControls::new(stereo.clone(), stereo.clone(), routes)
        .unwrap()
        .with_gain(0.25)
        .unwrap();
    let mut mix = ClipMixState::new();
    mix.apply(0, &[ClipMixMutation::set(CLIP, controls)])
        .unwrap();

    let mut automation = AudioAutomationState::new();
    apply(
        &mut automation,
        vec![AudioAutomationMutation::CreateLane {
            target: target(),
            sample_rate: RATE,
            default_gain: 0.5,
        }],
    );
    apply(
        &mut automation,
        vec![
            AudioAutomationMutation::SetKeyframe {
                target: target(),
                keyframe: keyframe(0, 0.5),
            },
            AudioAutomationMutation::SetKeyframe {
                target: target(),
                keyframe: keyframe(4, 1.0),
            },
        ],
    );
    let processor = mix
        .snapshot()
        .prepare_processor_with_automation(CLIP, at(0), 8, &automation.snapshot())
        .unwrap();

    let source = AudioNodeId::from_raw(1);
    let clip = AudioNodeId::from_raw(2);
    let submix = AudioNodeId::from_raw(3);
    let master = AudioNodeId::from_raw(4);
    let mut graph = AudioGraph::new(AudioGraphId::from_raw(1), RATE, 8).unwrap();
    graph
        .insert_node(AudioNode::new(source, None, stereo.clone()))
        .unwrap();
    graph
        .insert_node(AudioNode::new(clip, Some(stereo.clone()), stereo.clone()))
        .unwrap();
    graph
        .insert_node(AudioNode::bus(submix, AudioBusKind::Submix, stereo.clone()))
        .unwrap();
    graph
        .insert_node(AudioNode::bus(master, AudioBusKind::Master, stereo.clone()))
        .unwrap();
    for (raw, from, to) in [(1, source, clip), (2, clip, submix), (3, submix, master)] {
        graph
            .insert_edge(AudioEdge::new(AudioEdgeId::from_raw(raw), from, to))
            .unwrap();
    }
    let mut processors: BTreeMap<AudioNodeId, Box<dyn AudioProcessor>> = BTreeMap::new();
    processors.insert(source, Box::new(ConstantSource));
    processors.insert(clip, Box::new(processor));
    processors.insert(submix, Box::new(SummingBus));
    processors.insert(master, Box::new(SummingBus));
    let mut graph = graph.prepare_master(processors).unwrap();

    let _audio = ExecutionDomain::Audio.enter_current().unwrap();
    let mut start = 0_i64;
    let mut rendered = Vec::new();
    for frames in blocks {
        let mut output = vec![0.0; frames * 2];
        graph.process(at(start), *frames, &mut output).unwrap();
        rendered.extend(output);
        start += i64::try_from(*frames).unwrap();
    }
    assert_eq!(graph.next_sample(), Some(8));
    rendered
}

#[test]
fn prepared_clip_gain_is_partition_independent_in_the_real_routed_graph() {
    let whole = render_automated(&[8]);
    let split = render_automated(&[1, 3, 2, 2]);
    assert_eq!(whole, split);
    assert_eq!(
        whole,
        [
            0.5, 0.5, 0.625, 0.625, 0.75, 0.75, 0.875, 0.875, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0,
            1.0,
        ]
    );
    assert!(whole
        .iter()
        .all(|sample| sample.is_finite() && *sample > 0.0));
}
