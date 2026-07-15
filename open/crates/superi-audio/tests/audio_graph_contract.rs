use std::collections::BTreeMap;

use superi_audio::graph::{
    AudioEdge, AudioEdgeId, AudioGraph, AudioGraphId, AudioNode, AudioNodeId, AudioProcessBlock,
    AudioProcessor,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, Result};
use superi_core::pixel::ChannelLayout;
use superi_core::time::SampleTime;

struct RampSource;

impl AudioProcessor for RampSource {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        assert!(block.input.is_none());
        let channels = block.output_layout.len();
        for (frame, samples) in block.output.chunks_exact_mut(channels).enumerate() {
            let sample = block.start_time.sample() + i64::try_from(frame).unwrap();
            for (channel, value) in samples.iter_mut().enumerate() {
                *value = sample as f32 + channel as f32 * 0.25;
            }
        }
        Ok(())
    }
}

struct Gain(f32);

impl AudioProcessor for Gain {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        let input = block.input.expect("gain requires its connected input");
        for (output, input) in block.output.iter_mut().zip(input) {
            *output = input * self.0;
        }
        Ok(())
    }
}

fn node(raw: u128, input: bool, layout: &ChannelLayout) -> AudioNode {
    AudioNode::new(
        AudioNodeId::from_raw(raw),
        input.then(|| layout.clone()),
        layout.clone(),
    )
}

fn edge(raw: u128, source: u128, destination: u128) -> AudioEdge {
    AudioEdge::new(
        AudioEdgeId::from_raw(raw),
        AudioNodeId::from_raw(source),
        AudioNodeId::from_raw(destination),
    )
}

fn graph() -> AudioGraph {
    AudioGraph::new(AudioGraphId::from_raw(7), 48_000, 8).unwrap()
}

#[test]
fn editable_graph_is_typed_deterministic_and_validates_routes_atomically() {
    let stereo = ChannelLayout::stereo();
    let mono = ChannelLayout::mono();
    let mut graph = graph();
    graph.insert_node(node(3, true, &stereo)).unwrap();
    graph.insert_node(node(1, false, &stereo)).unwrap();
    graph.insert_node(node(2, true, &stereo)).unwrap();

    graph.insert_edge(edge(20, 2, 3)).unwrap();
    graph.insert_edge(edge(10, 1, 2)).unwrap();

    assert_eq!(graph.id(), AudioGraphId::from_raw(7));
    assert_eq!(graph.sample_rate(), 48_000);
    assert_eq!(graph.maximum_frames(), 8);
    assert_eq!(
        graph.nodes().keys().copied().collect::<Vec<_>>(),
        [1, 2, 3].map(AudioNodeId::from_raw)
    );
    assert_eq!(
        graph.edges().keys().copied().collect::<Vec<_>>(),
        [10, 20].map(AudioEdgeId::from_raw)
    );
    assert_eq!(
        graph.topological_order(),
        [1, 2, 3].map(AudioNodeId::from_raw)
    );

    let before = graph.clone();
    let error = graph.insert_edge(edge(30, 3, 1)).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(graph, before);

    let error = graph.insert_edge(edge(31, 1, 3)).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(graph, before);

    graph.insert_node(node(4, true, &mono)).unwrap();
    let before = graph.clone();
    let error = graph.insert_edge(edge(40, 1, 4)).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(graph, before);

    let error = graph.insert_edge(edge(41, 9, 4)).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotFound);
    assert_eq!(graph, before);
}

#[test]
fn prepared_graph_processes_exact_consecutive_blocks_on_the_audio_domain() {
    let stereo = ChannelLayout::stereo();
    let mut graph = graph();
    graph.insert_node(node(1, false, &stereo)).unwrap();
    graph.insert_node(node(2, true, &stereo)).unwrap();
    graph.insert_node(node(3, true, &stereo)).unwrap();
    graph.insert_edge(edge(10, 1, 2)).unwrap();
    graph.insert_edge(edge(20, 2, 3)).unwrap();

    let mut processors: BTreeMap<AudioNodeId, Box<dyn AudioProcessor>> = BTreeMap::new();
    processors.insert(AudioNodeId::from_raw(1), Box::new(RampSource));
    processors.insert(AudioNodeId::from_raw(2), Box::new(Gain(2.0)));
    processors.insert(AudioNodeId::from_raw(3), Box::new(Gain(0.5)));
    let mut prepared = graph.prepare(AudioNodeId::from_raw(3), processors).unwrap();

    assert_eq!(prepared.node_order(), [1, 2, 3].map(AudioNodeId::from_raw));
    assert_eq!(prepared.output_layout(), &stereo);

    let error = prepared
        .process(SampleTime::new(0, 48_000).unwrap(), 4, &mut [0.0; 8])
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);

    let _audio = ExecutionDomain::Audio.enter_current().unwrap();
    let mut first = [0.0; 8];
    prepared
        .process(SampleTime::new(0, 48_000).unwrap(), 4, &mut first)
        .unwrap();
    assert_eq!(first, [0.0, 0.25, 1.0, 1.25, 2.0, 2.25, 3.0, 3.25]);

    let mut second = [0.0; 8];
    prepared
        .process(SampleTime::new(4, 48_000).unwrap(), 4, &mut second)
        .unwrap();
    assert_eq!(second, [4.0, 4.25, 5.0, 5.25, 6.0, 6.25, 7.0, 7.25]);
    assert_eq!(prepared.next_sample(), Some(8));
}

#[test]
fn preparation_and_block_validation_fail_before_processing_state_advances() {
    let stereo = ChannelLayout::stereo();
    let mut graph = graph();
    graph.insert_node(node(1, false, &stereo)).unwrap();
    graph.insert_node(node(2, true, &stereo)).unwrap();

    let mut processors: BTreeMap<AudioNodeId, Box<dyn AudioProcessor>> = BTreeMap::new();
    processors.insert(AudioNodeId::from_raw(1), Box::new(RampSource));
    processors.insert(AudioNodeId::from_raw(2), Box::new(Gain(1.0)));
    let error = match graph.prepare(AudioNodeId::from_raw(2), processors) {
        Ok(_) => panic!("unconnected input must not prepare"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::Conflict);

    graph.insert_edge(edge(10, 1, 2)).unwrap();
    let mut processors: BTreeMap<AudioNodeId, Box<dyn AudioProcessor>> = BTreeMap::new();
    processors.insert(AudioNodeId::from_raw(1), Box::new(RampSource));
    processors.insert(AudioNodeId::from_raw(2), Box::new(Gain(1.0)));
    let mut prepared = graph.prepare(AudioNodeId::from_raw(2), processors).unwrap();
    let _audio = ExecutionDomain::Audio.enter_current().unwrap();

    let error = prepared
        .process(SampleTime::new(0, 44_100).unwrap(), 4, &mut [0.0; 8])
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(prepared.next_sample(), None);

    let error = prepared
        .process(SampleTime::new(0, 48_000).unwrap(), 9, &mut [0.0; 18])
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(prepared.next_sample(), None);

    let error = prepared
        .process(SampleTime::new(0, 48_000).unwrap(), 4, &mut [0.0; 7])
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(prepared.next_sample(), None);

    prepared
        .process(SampleTime::new(0, 48_000).unwrap(), 4, &mut [0.0; 8])
        .unwrap();
    let error = prepared
        .process(SampleTime::new(5, 48_000).unwrap(), 3, &mut [0.0; 6])
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(prepared.next_sample(), Some(4));
}
