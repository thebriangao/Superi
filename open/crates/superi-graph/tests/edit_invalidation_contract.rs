use std::str::FromStr;
use std::sync::Arc;

use superi_core::error::ErrorCategory;
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_graph::dag::{GraphEdge, GraphEndpoint};
use superi_graph::expr::{ParameterAddress, ParameterDriver, ParameterReference};
use superi_graph::ids::{EdgeId, GraphId, NodeId, ParameterId, PortId};
use superi_graph::invalidation::derive_edit_invalidation;
use superi_graph::mutate::{
    EditableGraph, EditableNode, EditableParameter, GraphMutation, GraphTransaction, InstancePort,
    TypedParameterValue,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchema, NodeSchemaId,
    NodeTypeId, ParameterName, ParameterSchema, PortCardinality, PortName, PortSchema, RoiBehavior,
    TimeBehavior, ValueTypeId,
};

const VALUE_PARAMETER: ParameterId = ParameterId::from_raw(900);

#[derive(Clone, Debug, Eq, PartialEq)]
struct Scalar(i64);

fn value_type() -> ValueTypeId {
    ValueTypeId::new("superi.value.edit-invalidation").unwrap()
}

fn behavior() -> NodeBehavior {
    NodeBehavior::new(
        TimeBehavior::CurrentFrame,
        RoiBehavior::InputBounds,
        ColorRequirements::NotApplicable,
        Determinism::Deterministic,
        CachePolicy::PerRegion,
    )
}

fn node(
    node_type: &str,
    input: Option<PortId>,
    output: PortId,
    value: i64,
) -> EditableNode<Scalar> {
    let inputs = input
        .map(|_| {
            vec![PortSchema::new(
                PortName::new("input").unwrap(),
                value_type(),
                PortCardinality::Single,
            )]
        })
        .unwrap_or_default();
    let schema = Arc::new(
        NodeSchema::new(
            NodeSchemaId::new(
                NodeTypeId::new(node_type).unwrap(),
                SemanticVersion::from_str("1.0.0").unwrap(),
            ),
            inputs,
            [PortSchema::new(
                PortName::new("output").unwrap(),
                value_type(),
                PortCardinality::Single,
            )],
            [ParameterSchema::new(
                ParameterName::new("value").unwrap(),
                value_type(),
                true,
            )],
            behavior(),
            CapabilitySet::new([]),
        )
        .unwrap(),
    );

    EditableNode::new(
        schema,
        input.map(|id| InstancePort::new(id, PortName::new("input").unwrap())),
        [InstancePort::new(output, PortName::new("output").unwrap())],
        [EditableParameter::new(
            VALUE_PARAMETER,
            ParameterName::new("value").unwrap(),
            TypedParameterValue::new(value_type(), Scalar(value)),
        )],
    )
    .unwrap()
}

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

fn graph(graph_id: GraphId, driver_value: i64) -> EditableGraph<Scalar> {
    let mut graph = EditableGraph::new(graph_id);
    graph
        .apply(GraphTransaction::with_mutations(
            0,
            [
                GraphMutation::Add {
                    node_id: NodeId::from_raw(1),
                    node: node("superi.test.edit-source", None, PortId::from_raw(101), 5),
                    position: 0,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(2),
                    node: node(
                        "superi.test.edit-process",
                        Some(PortId::from_raw(201)),
                        PortId::from_raw(202),
                        2,
                    ),
                    position: 1,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(3),
                    node: node(
                        "superi.test.edit-sink",
                        Some(PortId::from_raw(301)),
                        PortId::from_raw(302),
                        0,
                    ),
                    position: 2,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(4),
                    node: node(
                        "superi.test.edit-driver",
                        None,
                        PortId::from_raw(401),
                        driver_value,
                    ),
                    position: 3,
                },
                GraphMutation::Connect {
                    edge: edge(10, (1, 101), (2, 201)),
                },
                GraphMutation::Connect {
                    edge: edge(20, (2, 202), (3, 301)),
                },
                GraphMutation::SetParameterDriver {
                    target: ParameterAddress::new(NodeId::from_raw(2), VALUE_PARAMETER),
                    driver: ParameterDriver::link(
                        value_type(),
                        ParameterReference::new(
                            ParameterAddress::new(NodeId::from_raw(4), VALUE_PARAMETER),
                            value_type(),
                        ),
                    ),
                },
            ],
        ))
        .unwrap();
    graph
}

#[test]
fn parameter_edits_expand_through_driver_targets_and_pixel_descendants() {
    let mut graph = graph(GraphId::from_raw(71), 7);
    let before = graph.snapshot();
    let after = graph
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::SetParameter {
                node_id: NodeId::from_raw(4),
                parameter_id: VALUE_PARAMETER,
                value: TypedParameterValue::new(value_type(), Scalar(11)),
            }],
        ))
        .unwrap();

    let invalidation = derive_edit_invalidation(&before, &after).unwrap();

    assert_eq!(invalidation.graph_id(), GraphId::from_raw(71));
    assert_eq!(invalidation.from_revision(), 1);
    assert_eq!(invalidation.to_revision(), 2);
    assert_eq!(
        invalidation.roots(),
        [NodeId::from_raw(2), NodeId::from_raw(4)]
    );
    assert_eq!(
        invalidation.affected_nodes(),
        [
            NodeId::from_raw(2),
            NodeId::from_raw(3),
            NodeId::from_raw(4),
        ]
    );
    assert!(!invalidation.affects(NodeId::from_raw(1)));
    assert!(invalidation.affects(NodeId::from_raw(3)));
    assert_eq!(
        invalidation,
        derive_edit_invalidation(&before, &after).unwrap()
    );
}

#[test]
fn disconnection_and_removal_include_prior_topology_without_unrelated_sources() {
    let mut disconnected_graph = graph(GraphId::from_raw(72), 7);
    let before_disconnect = disconnected_graph.snapshot();
    let after_disconnect = disconnected_graph
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::Disconnect {
                edge_id: EdgeId::from_raw(10),
            }],
        ))
        .unwrap();
    let disconnected = derive_edit_invalidation(&before_disconnect, &after_disconnect).unwrap();
    assert_eq!(disconnected.roots(), [NodeId::from_raw(2)]);
    assert_eq!(
        disconnected.affected_nodes(),
        [NodeId::from_raw(2), NodeId::from_raw(3)]
    );

    let mut removed_graph = graph(GraphId::from_raw(73), 7);
    let before_remove = removed_graph.snapshot();
    let after_remove = removed_graph
        .apply(GraphTransaction::with_mutations(
            1,
            [
                GraphMutation::ClearParameterDriver {
                    target: ParameterAddress::new(NodeId::from_raw(2), VALUE_PARAMETER),
                },
                GraphMutation::Disconnect {
                    edge_id: EdgeId::from_raw(10),
                },
                GraphMutation::Disconnect {
                    edge_id: EdgeId::from_raw(20),
                },
                GraphMutation::Remove {
                    node_id: NodeId::from_raw(2),
                },
            ],
        ))
        .unwrap();
    let removed = derive_edit_invalidation(&before_remove, &after_remove).unwrap();
    assert_eq!(removed.roots(), [NodeId::from_raw(2), NodeId::from_raw(3)]);
    assert_eq!(
        removed.affected_nodes(),
        [NodeId::from_raw(2), NodeId::from_raw(3)]
    );
    assert!(!removed.affects(NodeId::from_raw(1)));
    assert!(!removed.affects(NodeId::from_raw(4)));
}

#[test]
fn presentation_only_and_skipped_forward_revisions_produce_no_processing_work() {
    let mut graph = graph(GraphId::from_raw(74), 7);
    let before = graph.snapshot();
    let reordered = graph
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::Reorder {
                node_id: NodeId::from_raw(4),
                position: 0,
            }],
        ))
        .unwrap();
    let restored = graph
        .apply(GraphTransaction::with_mutations(
            2,
            [GraphMutation::Reorder {
                node_id: NodeId::from_raw(4),
                position: 3,
            }],
        ))
        .unwrap();

    assert!(derive_edit_invalidation(&before, &before)
        .unwrap()
        .is_empty());
    assert!(derive_edit_invalidation(&before, &reordered)
        .unwrap()
        .is_empty());
    let skipped = derive_edit_invalidation(&before, &restored).unwrap();
    assert_eq!(skipped.from_revision(), 1);
    assert_eq!(skipped.to_revision(), 3);
    assert!(skipped.is_empty());
}

#[test]
fn invalid_graph_and_revision_pairs_fail_without_guessing() {
    let first = graph(GraphId::from_raw(75), 7).snapshot();
    let other_graph = graph(GraphId::from_raw(76), 7).snapshot();
    let same_revision_other_state = graph(GraphId::from_raw(75), 9).snapshot();

    let graph_error = derive_edit_invalidation(&first, &other_graph).unwrap_err();
    assert_eq!(graph_error.category(), ErrorCategory::InvalidInput);
    assert_eq!(
        graph_error.contexts()[0].field("reason"),
        Some("graph_identity_mismatch")
    );

    let revision_error = derive_edit_invalidation(&first, &same_revision_other_state).unwrap_err();
    assert_eq!(revision_error.category(), ErrorCategory::Conflict);
    assert_eq!(
        revision_error.contexts()[0].field("reason"),
        Some("revision_not_advanced")
    );

    let mut reversed_graph = graph(GraphId::from_raw(77), 7);
    let earlier = reversed_graph.snapshot();
    let later = reversed_graph
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::Reorder {
                node_id: NodeId::from_raw(4),
                position: 0,
            }],
        ))
        .unwrap();
    let reversed = derive_edit_invalidation(&later, &earlier).unwrap_err();
    assert_eq!(reversed.category(), ErrorCategory::Conflict);
    assert_eq!(
        reversed.contexts()[0].field("reason"),
        Some("revision_reversed")
    );
}
