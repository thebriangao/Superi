use superi_core::error::{ErrorCategory, Recoverability};
use superi_graph::dag::{DirectedAcyclicGraph, GraphEdge, GraphEndpoint};
use superi_graph::ids::{EdgeId, GraphId, NodeId, PortId};

fn endpoint(node: u128, port: u128) -> GraphEndpoint {
    GraphEndpoint::new(NodeId::from_raw(node), PortId::from_raw(port))
}

fn edge(id: u128, source: (u128, u128), destination: (u128, u128)) -> GraphEdge {
    GraphEdge::new(
        EdgeId::from_raw(id),
        endpoint(source.0, source.1),
        endpoint(destination.0, destination.1),
    )
}

fn graph_with_nodes(order: &[u128]) -> DirectedAcyclicGraph<&'static str> {
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(7));
    for raw in order {
        let label = match raw {
            1 => "source-a",
            2 => "source-b",
            3 => "mix",
            4 => "output",
            _ => panic!("unexpected test node"),
        };
        graph.insert_node(NodeId::from_raw(*raw), label).unwrap();
    }
    graph
}

#[test]
fn storage_is_typed_inspectable_and_deterministic() {
    let mut graph = graph_with_nodes(&[4, 2, 1, 3]);
    let routes = [
        edge(30, (3, 301), (4, 401)),
        edge(10, (1, 101), (3, 302)),
        edge(20, (2, 201), (3, 303)),
    ];
    for route in routes {
        graph.insert_edge(route).unwrap();
    }

    assert_eq!(graph.id(), GraphId::from_raw(7));
    assert_eq!(graph.node_count(), 4);
    assert_eq!(graph.edge_count(), 3);
    assert_eq!(
        graph.nodes().keys().copied().collect::<Vec<_>>(),
        [1, 2, 3, 4].map(NodeId::from_raw)
    );
    assert_eq!(
        graph.edges().keys().copied().collect::<Vec<_>>(),
        [10, 20, 30].map(EdgeId::from_raw)
    );
    assert_eq!(
        graph.topological_order(),
        [1, 2, 3, 4].map(NodeId::from_raw)
    );
    assert_eq!(graph.node(NodeId::from_raw(3)), Some(&"mix"));
    assert_eq!(
        graph
            .incoming_edge_ids(NodeId::from_raw(3))
            .unwrap()
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [10, 20].map(EdgeId::from_raw)
    );
    assert_eq!(
        graph
            .outgoing_edge_ids(NodeId::from_raw(3))
            .unwrap()
            .iter()
            .copied()
            .collect::<Vec<_>>(),
        [EdgeId::from_raw(30)]
    );
    let mix_to_output = graph.edge(EdgeId::from_raw(30)).unwrap();
    assert_eq!(mix_to_output.source().node_id(), NodeId::from_raw(3));
    assert_eq!(mix_to_output.source().port_id(), PortId::from_raw(301));
    assert_eq!(mix_to_output.destination().node_id(), NodeId::from_raw(4));

    let mut same = graph_with_nodes(&[1, 2, 3, 4]);
    for route in [
        edge(20, (2, 201), (3, 303)),
        edge(10, (1, 101), (3, 302)),
        edge(30, (3, 301), (4, 401)),
    ] {
        same.insert_edge(route).unwrap();
    }
    assert_eq!(graph, same);
    assert_eq!(graph.topological_order(), same.topological_order());
}

#[test]
fn direct_transitive_and_self_cycles_are_rejected_atomically() {
    let mut graph = graph_with_nodes(&[1, 2, 3, 4]);
    graph.insert_edge(edge(10, (1, 101), (2, 201))).unwrap();
    graph.insert_edge(edge(20, (2, 202), (3, 301))).unwrap();
    let before = graph.clone();

    let error = graph.insert_edge(edge(30, (3, 302), (1, 102))).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(error.contexts()[0].component(), "superi-graph.dag");
    assert_eq!(error.contexts()[0].operation(), "insert_edge");
    assert_eq!(
        error.contexts()[0]
            .fields()
            .get("edge_id")
            .map(String::as_str),
        Some("edge:0000000000000000000000000000001e")
    );
    assert_eq!(graph, before);

    let error = graph.insert_edge(edge(31, (2, 203), (1, 103))).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(graph, before);

    let error = graph.insert_edge(edge(32, (4, 401), (4, 402))).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(graph, before);
}

#[test]
fn invalid_identity_and_endpoint_operations_preserve_state() {
    let mut graph = graph_with_nodes(&[1, 2, 3, 4]);
    graph.insert_edge(edge(10, (1, 101), (2, 201))).unwrap();
    let before = graph.clone();

    let error = graph
        .insert_node(NodeId::from_raw(1), "duplicate")
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(graph, before);

    let error = graph.insert_edge(edge(20, (9, 901), (2, 202))).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotFound);
    assert_eq!(graph, before);

    let error = graph.insert_edge(edge(10, (2, 203), (3, 301))).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(graph, before);
}

#[test]
fn explicit_removal_keeps_primary_and_adjacency_storage_consistent() {
    let mut graph = graph_with_nodes(&[1, 2, 3, 4]);
    graph.insert_edge(edge(10, (1, 101), (2, 201))).unwrap();
    graph.insert_edge(edge(20, (2, 202), (3, 301))).unwrap();

    let before = graph.clone();
    let error = graph.remove_node(NodeId::from_raw(2)).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(graph, before);

    assert_eq!(
        graph.remove_edge(EdgeId::from_raw(10)).unwrap().id(),
        EdgeId::from_raw(10)
    );
    assert_eq!(
        graph.remove_edge(EdgeId::from_raw(20)).unwrap().id(),
        EdgeId::from_raw(20)
    );
    assert_eq!(graph.remove_node(NodeId::from_raw(2)).unwrap(), "source-b");
    assert!(!graph.nodes().contains_key(&NodeId::from_raw(2)));
    assert_eq!(graph.edge_count(), 0);
    assert_eq!(graph.topological_order(), [1, 3, 4].map(NodeId::from_raw));
}
