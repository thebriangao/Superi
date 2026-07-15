use std::collections::BTreeMap;

use superi_audio::graph::{
    AudioBusKind, AudioEdge, AudioEdgeId, AudioGraph, AudioGraphId, AudioNode, AudioNodeId,
    AudioProcessBlock, AudioProcessor, AudioRouteKind,
};
use superi_audio::routing::SummingBus;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, Result};
use superi_core::pixel::ChannelLayout;
use superi_core::time::SampleTime;

struct ConstantSource([f32; 2]);

impl AudioProcessor for ConstantSource {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        assert!(block.input.is_none());
        for frame in block.output.chunks_exact_mut(2) {
            frame.copy_from_slice(&self.0);
        }
        Ok(())
    }
}

fn id(raw: u128) -> AudioNodeId {
    AudioNodeId::from_raw(raw)
}

fn direct(raw: u128, source: u128, destination: u128) -> AudioEdge {
    AudioEdge::new(AudioEdgeId::from_raw(raw), id(source), id(destination))
}

fn send(raw: u128, source: u128, auxiliary: u128) -> AudioEdge {
    AudioEdge::send(AudioEdgeId::from_raw(raw), id(source), id(auxiliary))
}

fn aux_return(raw: u128, auxiliary: u128, destination: u128) -> AudioEdge {
    AudioEdge::aux_return(AudioEdgeId::from_raw(raw), id(auxiliary), id(destination))
}

fn source(raw: u128, layout: &ChannelLayout) -> AudioNode {
    AudioNode::new(id(raw), None, layout.clone())
}

fn bus(raw: u128, kind: AudioBusKind, layout: &ChannelLayout) -> AudioNode {
    AudioNode::bus(id(raw), kind, layout.clone())
}

fn graph() -> AudioGraph {
    AudioGraph::new(AudioGraphId::from_raw(55), 48_000, 8).unwrap()
}

#[test]
fn buses_render_dry_submix_auxiliary_send_return_and_single_master_deterministically() {
    let stereo = ChannelLayout::stereo();
    let mut graph = graph();
    graph.insert_node(source(1, &stereo)).unwrap();
    graph.insert_node(source(2, &stereo)).unwrap();
    graph
        .insert_node(bus(10, AudioBusKind::Submix, &stereo))
        .unwrap();
    graph
        .insert_node(bus(20, AudioBusKind::Auxiliary, &stereo))
        .unwrap();
    graph
        .insert_node(bus(30, AudioBusKind::Master, &stereo))
        .unwrap();

    // Deliberately insert routes out of identity order. Prepared inputs must remain ordered by
    // stable route identity, including the auxiliary return.
    graph.insert_edge(direct(300, 10, 30)).unwrap();
    graph.insert_edge(direct(200, 2, 10)).unwrap();
    graph.insert_edge(send(50, 1, 20)).unwrap();
    graph.insert_edge(direct(100, 1, 10)).unwrap();
    graph.insert_edge(aux_return(75, 20, 10)).unwrap();

    assert_eq!(graph.master_node(), Some(id(30)));
    assert_eq!(
        graph.edges()[&AudioEdgeId::from_raw(50)].kind(),
        AudioRouteKind::Send
    );
    assert_eq!(
        graph.edges()[&AudioEdgeId::from_raw(75)].kind(),
        AudioRouteKind::AuxReturn
    );

    let mut processors: BTreeMap<AudioNodeId, Box<dyn AudioProcessor>> = BTreeMap::new();
    processors.insert(id(1), Box::new(ConstantSource([1.0, 2.0])));
    processors.insert(id(2), Box::new(ConstantSource([10.0, 20.0])));
    processors.insert(id(10), Box::new(SummingBus));
    processors.insert(id(20), Box::new(SummingBus));
    processors.insert(id(30), Box::new(SummingBus));
    let mut prepared = graph.prepare_master(processors).unwrap();

    assert_eq!(prepared.node_order(), [1, 2, 20, 10, 30].map(id));
    assert_eq!(
        prepared
            .input_routes(id(10))
            .unwrap()
            .iter()
            .map(|route| (route.edge_id(), route.source_node()))
            .collect::<Vec<_>>(),
        [
            (AudioEdgeId::from_raw(75), id(20)),
            (AudioEdgeId::from_raw(100), id(1)),
            (AudioEdgeId::from_raw(200), id(2)),
        ]
    );

    let _audio = ExecutionDomain::Audio.enter_current().unwrap();
    let mut first = [0.0; 8];
    prepared
        .process(SampleTime::new(0, 48_000).unwrap(), 4, &mut first)
        .unwrap();
    assert_eq!(first, [12.0, 24.0, 12.0, 24.0, 12.0, 24.0, 12.0, 24.0]);

    let mut second = [0.0; 8];
    prepared
        .process(SampleTime::new(4, 48_000).unwrap(), 4, &mut second)
        .unwrap();
    assert_eq!(second, first);
    assert_eq!(prepared.next_sample(), Some(8));
}

#[test]
fn routing_roles_and_master_constraints_reject_invalid_edits_atomically() {
    let stereo = ChannelLayout::stereo();
    let mono = ChannelLayout::mono();
    let mut graph = graph();
    graph.insert_node(source(1, &stereo)).unwrap();
    graph
        .insert_node(bus(10, AudioBusKind::Submix, &stereo))
        .unwrap();
    graph
        .insert_node(bus(20, AudioBusKind::Auxiliary, &stereo))
        .unwrap();
    graph
        .insert_node(bus(30, AudioBusKind::Master, &stereo))
        .unwrap();

    let before = graph.clone();
    let error = graph
        .insert_node(bus(31, AudioBusKind::Master, &stereo))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(graph, before);

    for invalid in [
        direct(1, 1, 20),
        send(2, 1, 10),
        aux_return(3, 1, 10),
        direct(4, 30, 10),
    ] {
        let before = graph.clone();
        assert_eq!(
            graph.insert_edge(invalid).unwrap_err().category(),
            ErrorCategory::InvalidInput
        );
        assert_eq!(graph, before);
    }

    graph.insert_node(source(2, &mono)).unwrap();
    let before = graph.clone();
    assert_eq!(
        graph.insert_edge(direct(5, 2, 10)).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(graph, before);

    graph.insert_edge(send(10, 1, 20)).unwrap();
    graph.insert_edge(aux_return(11, 20, 10)).unwrap();
    graph.insert_edge(direct(12, 10, 30)).unwrap();
    let before = graph.clone();
    assert_eq!(
        graph.insert_edge(send(13, 10, 20)).unwrap_err().category(),
        ErrorCategory::Conflict
    );
    assert_eq!(graph, before);
}

#[test]
fn bus_summing_order_depends_on_route_identity_not_edit_history() {
    fn render(edge_insertion_order: [u128; 3]) -> f32 {
        let mono = ChannelLayout::mono();
        let mut graph = graph();
        for source_id in 1..=3 {
            graph.insert_node(source(source_id, &mono)).unwrap();
        }
        graph
            .insert_node(bus(30, AudioBusKind::Master, &mono))
            .unwrap();
        for edge_id in edge_insertion_order {
            graph.insert_edge(direct(edge_id, edge_id, 30)).unwrap();
        }

        struct MonoSource(f32);
        impl AudioProcessor for MonoSource {
            fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
                block.output.fill(self.0);
                Ok(())
            }
        }

        let mut processors: BTreeMap<AudioNodeId, Box<dyn AudioProcessor>> = BTreeMap::new();
        processors.insert(id(1), Box::new(MonoSource(1.0e20)));
        processors.insert(id(2), Box::new(MonoSource(-1.0e20)));
        processors.insert(id(3), Box::new(MonoSource(1.0)));
        processors.insert(id(30), Box::new(SummingBus));
        let mut prepared = graph.prepare_master(processors).unwrap();
        let _audio = ExecutionDomain::Audio.enter_current().unwrap();
        let mut output = [0.0];
        prepared
            .process(SampleTime::new(0, 48_000).unwrap(), 1, &mut output)
            .unwrap();
        output[0]
    }

    assert_eq!(render([3, 1, 2]), 1.0);
    assert_eq!(render([2, 3, 1]), 1.0);
}
