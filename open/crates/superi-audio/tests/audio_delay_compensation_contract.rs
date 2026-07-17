use std::collections::BTreeMap;

use superi_audio::graph::{
    AudioBusKind, AudioEdge, AudioEdgeId, AudioGraph, AudioGraphId, AudioNode, AudioNodeId,
    AudioProcessBlock, AudioProcessor,
};
use superi_audio::routing::SummingBus;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, Result};
use superi_core::pixel::ChannelLayout;
use superi_core::time::SampleTime;

const RATE: u32 = 48_000;
const MAXIMUM_FRAMES: usize = 8;

struct ImpulseSource;

impl AudioProcessor for ImpulseSource {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        for (frame, sample) in block.output.iter_mut().enumerate() {
            let absolute_sample = block.start_time.sample() + i64::try_from(frame).unwrap();
            *sample = f32::from(absolute_sample == 0);
        }
        Ok(())
    }
}

struct SilentSource;

impl AudioProcessor for SilentSource {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        block.output.fill(0.0);
        Ok(())
    }
}

struct FixedDelay {
    samples: usize,
    ring: Vec<f32>,
    cursor: usize,
}

impl FixedDelay {
    fn new(samples: usize) -> Self {
        Self {
            samples,
            ring: vec![0.0; samples],
            cursor: 0,
        }
    }
}

impl AudioProcessor for FixedDelay {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        let input = block.input.expect("delay requires one connected input");
        if self.ring.is_empty() {
            block.output.copy_from_slice(input);
            return Ok(());
        }
        for (input, output) in input.iter().copied().zip(block.output.iter_mut()) {
            *output = self.ring[self.cursor];
            self.ring[self.cursor] = input;
            self.cursor = (self.cursor + 1) % self.ring.len();
        }
        Ok(())
    }

    fn latency_samples(&self) -> usize {
        self.samples
    }
}

struct ImpossibleLatency;

impl AudioProcessor for ImpossibleLatency {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        block.output.fill(0.0);
        Ok(())
    }

    fn latency_samples(&self) -> usize {
        usize::MAX
    }
}

fn node(raw: u128, input: bool, bus: Option<AudioBusKind>) -> AudioNode {
    let layout = ChannelLayout::mono();
    match bus {
        Some(kind) => AudioNode::bus(AudioNodeId::from_raw(raw), kind, layout),
        None => AudioNode::new(
            AudioNodeId::from_raw(raw),
            input.then(|| layout.clone()),
            layout,
        ),
    }
}

fn edge(raw: u128, source: u128, destination: u128) -> AudioEdge {
    AudioEdge::new(
        AudioEdgeId::from_raw(raw),
        AudioNodeId::from_raw(source),
        AudioNodeId::from_raw(destination),
    )
}

fn parallel_graph(
    first: Box<dyn AudioProcessor>,
    second: Box<dyn AudioProcessor>,
) -> superi_audio::graph::PreparedAudioGraph {
    let mut graph = AudioGraph::new(AudioGraphId::from_raw(0xc014), RATE, MAXIMUM_FRAMES).unwrap();
    graph.insert_node(node(1, false, None)).unwrap();
    graph.insert_node(node(2, false, None)).unwrap();
    graph.insert_node(node(3, true, None)).unwrap();
    graph
        .insert_node(node(4, true, Some(AudioBusKind::Master)))
        .unwrap();
    graph.insert_edge(edge(10, 1, 3)).unwrap();
    graph.insert_edge(edge(20, 3, 4)).unwrap();
    graph.insert_edge(edge(30, 2, 4)).unwrap();

    let mut processors = BTreeMap::<AudioNodeId, Box<dyn AudioProcessor>>::new();
    processors.insert(AudioNodeId::from_raw(1), first);
    processors.insert(AudioNodeId::from_raw(2), second);
    processors.insert(AudioNodeId::from_raw(3), Box::new(FixedDelay::new(3)));
    processors.insert(AudioNodeId::from_raw(4), Box::new(SummingBus));
    graph.prepare_master(processors).unwrap()
}

fn render_partitioned(partitions: &[usize]) -> Vec<f32> {
    let mut graph = parallel_graph(Box::new(ImpulseSource), Box::new(ImpulseSource));
    let mut rendered = Vec::new();
    let mut start = 0_i64;
    let _audio = ExecutionDomain::Audio.enter_current().unwrap();
    for frames in partitions {
        let mut block = vec![0.0; *frames];
        graph
            .process(SampleTime::new(start, RATE).unwrap(), *frames, &mut block)
            .unwrap();
        rendered.extend(block);
        start += i64::try_from(*frames).unwrap();
    }
    rendered
}

#[test]
fn parallel_paths_are_compensated_to_the_longest_arrival_without_changing_authored_delay() {
    let graph = parallel_graph(Box::new(ImpulseSource), Box::new(ImpulseSource));
    assert_eq!(graph.latency_samples(), 3);

    let master_routes = graph.input_routes(AudioNodeId::from_raw(4)).unwrap();
    assert_eq!(master_routes.len(), 2);
    assert_eq!(master_routes[0].edge_id(), AudioEdgeId::from_raw(20));
    assert_eq!(master_routes[0].compensation_delay_samples(), 0);
    assert_eq!(master_routes[1].edge_id(), AudioEdgeId::from_raw(30));
    assert_eq!(master_routes[1].compensation_delay_samples(), 3);

    let whole = render_partitioned(&[8]);
    let partitioned = render_partitioned(&[2, 1, 3, 2]);
    assert_eq!(whole, partitioned);
    assert_eq!(whole, [0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0]);
}

#[test]
fn impossible_compensation_storage_is_rejected_during_preparation() {
    let mut graph =
        AudioGraph::new(AudioGraphId::from_raw(0xc014_ffff), RATE, MAXIMUM_FRAMES).unwrap();
    graph.insert_node(node(1, false, None)).unwrap();
    graph.insert_node(node(2, false, None)).unwrap();
    graph
        .insert_node(node(3, true, Some(AudioBusKind::Master)))
        .unwrap();
    graph.insert_edge(edge(10, 1, 3)).unwrap();
    graph.insert_edge(edge(20, 2, 3)).unwrap();

    let mut processors = BTreeMap::<AudioNodeId, Box<dyn AudioProcessor>>::new();
    processors.insert(AudioNodeId::from_raw(1), Box::new(ImpossibleLatency));
    processors.insert(AudioNodeId::from_raw(2), Box::new(SilentSource));
    processors.insert(AudioNodeId::from_raw(3), Box::new(SummingBus));
    let error = match graph.prepare_master(processors) {
        Ok(_) => panic!("impossible route compensation unexpectedly prepared"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
}

#[test]
fn dry_and_auxiliary_return_paths_rejoin_at_the_same_sample() {
    let mono = ChannelLayout::mono();
    let mut graph =
        AudioGraph::new(AudioGraphId::from_raw(0xc014_0002), RATE, MAXIMUM_FRAMES).unwrap();
    graph.insert_node(node(1, false, None)).unwrap();
    graph
        .insert_node(AudioNode::bus(
            AudioNodeId::from_raw(20),
            AudioBusKind::Auxiliary,
            mono.clone(),
        ))
        .unwrap();
    graph
        .insert_node(AudioNode::bus(
            AudioNodeId::from_raw(30),
            AudioBusKind::Submix,
            mono.clone(),
        ))
        .unwrap();
    graph
        .insert_node(AudioNode::bus(
            AudioNodeId::from_raw(40),
            AudioBusKind::Master,
            mono,
        ))
        .unwrap();
    graph
        .insert_edge(AudioEdge::send(
            AudioEdgeId::from_raw(10),
            AudioNodeId::from_raw(1),
            AudioNodeId::from_raw(20),
        ))
        .unwrap();
    graph
        .insert_edge(AudioEdge::aux_return(
            AudioEdgeId::from_raw(20),
            AudioNodeId::from_raw(20),
            AudioNodeId::from_raw(30),
        ))
        .unwrap();
    graph.insert_edge(edge(30, 1, 30)).unwrap();
    graph.insert_edge(edge(40, 30, 40)).unwrap();

    let mut processors = BTreeMap::<AudioNodeId, Box<dyn AudioProcessor>>::new();
    processors.insert(AudioNodeId::from_raw(1), Box::new(ImpulseSource));
    processors.insert(AudioNodeId::from_raw(20), Box::new(FixedDelay::new(3)));
    processors.insert(AudioNodeId::from_raw(30), Box::new(SummingBus));
    processors.insert(AudioNodeId::from_raw(40), Box::new(SummingBus));
    let mut prepared = graph.prepare_master(processors).unwrap();

    assert_eq!(prepared.latency_samples(), 3);
    let routes = prepared.input_routes(AudioNodeId::from_raw(30)).unwrap();
    assert_eq!(routes[0].compensation_delay_samples(), 0);
    assert_eq!(routes[1].compensation_delay_samples(), 3);

    let _audio = ExecutionDomain::Audio.enter_current().unwrap();
    let mut output = [0.0; 8];
    prepared
        .process(SampleTime::new(0, RATE).unwrap(), 8, &mut output)
        .unwrap();
    assert_eq!(output, [0.0, 0.0, 0.0, 2.0, 0.0, 0.0, 0.0, 0.0]);
}
