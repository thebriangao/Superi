use std::collections::BTreeMap;

use superi_audio::channels::{ChannelInterpretation, CommonChannelLayout, PreparedChannelMixer};
use superi_audio::graph::{
    AudioEdge, AudioEdgeId, AudioGraph, AudioGraphId, AudioNode, AudioNodeId, AudioProcessBlock,
    AudioProcessor,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::Result;
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::SampleTime;

fn process(
    mixer: &mut PreparedChannelMixer,
    input_layout: &ChannelLayout,
    output_layout: &ChannelLayout,
    input: &[f32],
    output: &mut [f32],
    frame_count: usize,
) {
    mixer
        .process(AudioProcessBlock {
            start_time: SampleTime::new(1_234, 48_000).expect("sample time"),
            frame_count,
            input: Some(input),
            input_layout: Some(input_layout),
            output,
            output_layout,
        })
        .expect("channel conversion");
}

struct TimedStereoSource;

impl AudioProcessor for TimedStereoSource {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        for (frame, samples) in block.output.chunks_exact_mut(2).enumerate() {
            let sample = block.start_time.sample() + i64::try_from(frame).expect("bounded frame");
            samples[0] = sample as f32;
            samples[1] = sample as f32 + 2.0;
        }
        Ok(())
    }
}

#[test]
fn common_layouts_preserve_canonical_semantic_order() {
    let cases = [
        (CommonChannelLayout::Mono, ChannelLayout::mono()),
        (CommonChannelLayout::Stereo, ChannelLayout::stereo()),
        (CommonChannelLayout::Quad, ChannelLayout::quad()),
        (
            CommonChannelLayout::Surround5_1,
            ChannelLayout::surround_5_1(),
        ),
        (
            CommonChannelLayout::Surround7_1,
            ChannelLayout::surround_7_1(),
        ),
    ];

    for (common, expected) in cases {
        assert_eq!(common.layout(), expected);
        assert_eq!(CommonChannelLayout::from_layout(&expected), Some(common));
    }
    assert_eq!(
        CommonChannelLayout::Surround5_1.layout().positions(),
        &[
            ChannelPosition::FrontLeft,
            ChannelPosition::FrontRight,
            ChannelPosition::FrontCenter,
            ChannelPosition::LowFrequency,
            ChannelPosition::BackLeft,
            ChannelPosition::BackRight,
        ]
    );
}

#[test]
fn speaker_conversion_uses_documented_surround_coefficients() {
    let input_layout = ChannelLayout::surround_5_1();
    let output_layout = ChannelLayout::stereo();
    let mut mixer = PreparedChannelMixer::new(
        input_layout.clone(),
        output_layout.clone(),
        ChannelInterpretation::Speakers,
    )
    .expect("prepare speaker conversion");
    let input = [1.0, 2.0, 4.0, 100.0, 8.0, 16.0];
    let mut output = [0.0; 2];

    process(
        &mut mixer,
        &input_layout,
        &output_layout,
        &input,
        &mut output,
        1,
    );

    let root_half = 0.5_f32.sqrt();
    assert!((output[0] - (1.0 + root_half * (4.0 + 8.0))).abs() < 1.0e-6);
    assert!((output[1] - (2.0 + root_half * (4.0 + 16.0))).abs() < 1.0e-6);
}

#[test]
fn discrete_conversion_copies_order_drops_excess_and_zero_fills() {
    let input_layout = ChannelLayout::surround_7_1();
    let output_layout = ChannelLayout::surround_5_1();
    let mut mixer = PreparedChannelMixer::new(
        input_layout.clone(),
        output_layout.clone(),
        ChannelInterpretation::Discrete,
    )
    .expect("prepare discrete conversion");
    let input = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
    let mut output = [0.0; 6];

    process(
        &mut mixer,
        &input_layout,
        &output_layout,
        &input,
        &mut output,
        1,
    );
    assert_eq!(output, [1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);

    let mono = ChannelLayout::mono();
    let mut expander = PreparedChannelMixer::new(
        mono.clone(),
        output_layout.clone(),
        ChannelInterpretation::Discrete,
    )
    .expect("prepare discrete expansion");
    let mut expanded = [9.0; 6];
    process(
        &mut expander,
        &mono,
        &output_layout,
        &[0.25],
        &mut expanded,
        1,
    );
    assert_eq!(expanded, [0.25, 0.0, 0.0, 0.0, 0.0, 0.0]);
}

#[test]
fn unsupported_speaker_conversion_and_wrong_runtime_layout_fail_closed() {
    let seven_one = ChannelLayout::surround_7_1();
    assert!(PreparedChannelMixer::new(
        seven_one,
        ChannelLayout::stereo(),
        ChannelInterpretation::Speakers,
    )
    .is_err());

    let input_layout = ChannelLayout::stereo();
    let output_layout = ChannelLayout::mono();
    let mut mixer = PreparedChannelMixer::new(
        input_layout.clone(),
        output_layout.clone(),
        ChannelInterpretation::Speakers,
    )
    .expect("prepare downmix");
    let mut output = [77.0];
    let error = mixer.process(AudioProcessBlock {
        start_time: SampleTime::new(0, 48_000).expect("sample time"),
        frame_count: 1,
        input: Some(&[1.0, 2.0]),
        input_layout: Some(&ChannelLayout::quad()),
        output: &mut output,
        output_layout: &output_layout,
    });
    assert!(error.is_err());
    assert_eq!(output, [77.0]);
}

#[test]
fn explicit_layout_node_preserves_sample_time_through_the_prepared_graph() {
    let stereo = ChannelLayout::stereo();
    let mono = ChannelLayout::mono();
    let source_id = AudioNodeId::from_raw(1);
    let converter_id = AudioNodeId::from_raw(2);
    let mut graph = AudioGraph::new(AudioGraphId::from_raw(7), 48_000, 4).expect("graph");
    graph
        .insert_node(AudioNode::new(source_id, None, stereo.clone()))
        .expect("source");
    graph
        .insert_node(AudioNode::new(
            converter_id,
            Some(stereo.clone()),
            mono.clone(),
        ))
        .expect("converter");
    graph
        .insert_edge(AudioEdge::new(
            AudioEdgeId::from_raw(1),
            source_id,
            converter_id,
        ))
        .expect("route");

    let mut processors: BTreeMap<AudioNodeId, Box<dyn AudioProcessor>> = BTreeMap::new();
    processors.insert(source_id, Box::new(TimedStereoSource));
    processors.insert(
        converter_id,
        Box::new(
            PreparedChannelMixer::new(stereo, mono, ChannelInterpretation::Speakers)
                .expect("mixer"),
        ),
    );
    let mut prepared = graph
        .prepare(converter_id, processors)
        .expect("prepared graph");
    let _audio = ExecutionDomain::Audio
        .enter_current()
        .expect("audio domain");
    let mut first = [0.0; 2];
    prepared
        .process(SampleTime::new(100, 48_000).expect("time"), 2, &mut first)
        .expect("first block");
    assert_eq!(first, [101.0, 102.0]);
    let mut second = [0.0; 2];
    prepared
        .process(SampleTime::new(102, 48_000).expect("time"), 2, &mut second)
        .expect("second block");
    assert_eq!(second, [103.0, 104.0]);
    assert_eq!(prepared.next_sample(), Some(104));
}
