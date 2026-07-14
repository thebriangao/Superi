use std::str::FromStr;
use std::sync::Arc;

use superi_core::color_space::ColorSpace;
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_graph::dag::{GraphEdge, GraphEndpoint};
use superi_graph::ids::{EdgeId, GraphId, NodeId, ParameterId, PortId};
use superi_graph::mutate::{
    EditableGraph, EditableNode, EditableParameter, GraphMutation, GraphTransaction, InstancePort,
    TypedParameterValue,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchema, NodeSchemaId,
    NodeTypeId, ParameterName, ParameterSchema, PortCardinality, PortName, PortSchema, RoiBehavior,
    TimeBehavior, ValueTypeId,
};

type Payload = String;

fn value_type(input: &str) -> ValueTypeId {
    ValueTypeId::from_str(input).expect("valid value type")
}

fn port(input: &str) -> PortName {
    PortName::new(input).expect("valid port name")
}

fn parameter(input: &str) -> ParameterName {
    ParameterName::new(input).expect("valid parameter name")
}

fn schema(
    node_type: &str,
    inputs: Vec<PortSchema>,
    outputs: Vec<PortSchema>,
    parameters: Vec<ParameterSchema>,
) -> Arc<NodeSchema> {
    Arc::new(
        NodeSchema::new(
            NodeSchemaId::new(
                NodeTypeId::from_str(node_type).expect("valid node type"),
                SemanticVersion::new(1, 0, 0),
            ),
            inputs,
            outputs,
            parameters,
            NodeBehavior::new(
                TimeBehavior::CurrentFrame,
                RoiBehavior::InputBounds,
                ColorRequirements::Exact(ColorSpace::ACESCG),
                Determinism::Deterministic,
                CachePolicy::PerRegion,
            ),
            CapabilitySet::default(),
        )
        .expect("valid schema"),
    )
}

fn source_node() -> EditableNode<Payload> {
    EditableNode::new(
        schema(
            "superi.source.image",
            vec![],
            vec![PortSchema::new(
                port("image"),
                value_type("superi.value.image"),
                PortCardinality::Single,
            )],
            vec![],
        ),
        [],
        [InstancePort::new(PortId::from_raw(101), port("image"))],
        [],
    )
    .expect("valid source instance")
}

fn transform_node() -> EditableNode<Payload> {
    EditableNode::new(
        schema(
            "superi.effect.transform",
            vec![PortSchema::new(
                port("image"),
                value_type("superi.value.image"),
                PortCardinality::Single,
            )],
            vec![PortSchema::new(
                port("image"),
                value_type("superi.value.image"),
                PortCardinality::Single,
            )],
            vec![ParameterSchema::new(
                parameter("opacity"),
                value_type("superi.value.scalar"),
                true,
            )],
        ),
        [InstancePort::new(PortId::from_raw(201), port("image"))],
        [InstancePort::new(PortId::from_raw(202), port("image"))],
        [EditableParameter::new(
            ParameterId::from_raw(210),
            parameter("opacity"),
            TypedParameterValue::new(value_type("superi.value.scalar"), "1.0".into()),
        )],
    )
    .expect("valid transform instance")
}

fn sink_node() -> EditableNode<Payload> {
    EditableNode::new(
        schema(
            "superi.output.image",
            vec![PortSchema::new(
                port("image"),
                value_type("superi.value.image"),
                PortCardinality::Single,
            )],
            vec![],
            vec![],
        ),
        [InstancePort::new(PortId::from_raw(301), port("image"))],
        [],
        [],
    )
    .expect("valid sink instance")
}

fn scalar_sink_node() -> EditableNode<Payload> {
    EditableNode::new(
        schema(
            "superi.output.scalar",
            vec![PortSchema::new(
                port("value"),
                value_type("superi.value.scalar"),
                PortCardinality::Single,
            )],
            vec![],
            vec![],
        ),
        [InstancePort::new(PortId::from_raw(401), port("value"))],
        [],
        [],
    )
    .expect("valid scalar sink instance")
}

fn edge(id: u128, source: (u128, u128), destination: (u128, u128)) -> GraphEdge {
    GraphEdge::new(
        EdgeId::from_raw(id),
        GraphEndpoint::new(NodeId::from_raw(source.0), PortId::from_raw(source.1)),
        GraphEndpoint::new(
            NodeId::from_raw(destination.0),
            PortId::from_raw(destination.1),
        ),
    )
}

fn complete_transaction(expected_revision: u64) -> GraphTransaction<Payload> {
    GraphTransaction::with_mutations(
        expected_revision,
        [
            GraphMutation::Add {
                node_id: NodeId::from_raw(1),
                node: source_node(),
                position: 0,
            },
            GraphMutation::Add {
                node_id: NodeId::from_raw(2),
                node: transform_node(),
                position: 1,
            },
            GraphMutation::Add {
                node_id: NodeId::from_raw(3),
                node: sink_node(),
                position: 2,
            },
            GraphMutation::Connect {
                edge: edge(10, (1, 101), (2, 201)),
            },
            GraphMutation::Connect {
                edge: edge(20, (2, 202), (3, 301)),
            },
            GraphMutation::SetParameter {
                node_id: NodeId::from_raw(2),
                parameter_id: ParameterId::from_raw(210),
                value: TypedParameterValue::new(value_type("superi.value.scalar"), "0.5".into()),
            },
            GraphMutation::Reorder {
                node_id: NodeId::from_raw(3),
                position: 0,
            },
        ],
    )
}

#[test]
fn one_transaction_publishes_the_complete_typed_editable_state() {
    let mut graph = EditableGraph::new(GraphId::from_raw(7));
    let initial = graph.snapshot();
    let committed = graph
        .apply(complete_transaction(0))
        .expect("complete transaction");

    assert_eq!(initial.revision(), 0);
    assert_eq!(initial.dag().node_count(), 0);
    assert_eq!(committed.revision(), 1);
    assert_eq!(graph.revision(), 1);
    assert_eq!(committed.dag().node_count(), 3);
    assert_eq!(committed.dag().edge_count(), 2);
    assert_eq!(
        committed.node_order(),
        [3, 1, 2].map(NodeId::from_raw).as_slice()
    );
    assert_eq!(
        committed.dag().topological_order(),
        [1, 2, 3].map(NodeId::from_raw)
    );

    let transform = committed.node(NodeId::from_raw(2)).unwrap();
    assert_eq!(
        transform.input_name(PortId::from_raw(201)),
        Some(&port("image"))
    );
    assert_eq!(
        transform.output_name(PortId::from_raw(202)),
        Some(&port("image"))
    );
    assert_eq!(
        transform
            .parameter(ParameterId::from_raw(210))
            .unwrap()
            .value()
            .payload(),
        "0.5"
    );

    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<superi_graph::mutate::GraphSnapshot<Payload>>();
    let observed = std::thread::scope(|scope| {
        ["editor", "script", "headless"]
            .into_iter()
            .map(|_| {
                let snapshot = committed.clone();
                scope.spawn(move || snapshot)
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(observed[0], observed[1]);
    assert_eq!(observed[1], observed[2]);
    assert_eq!(initial.dag().node_count(), 0);
}

#[test]
fn mid_batch_cycle_failure_discards_parameter_and_topology_changes() {
    let mut graph = EditableGraph::new(GraphId::from_raw(7));
    graph.apply(complete_transaction(0)).unwrap();
    let before = graph.snapshot();

    let error = graph
        .apply(GraphTransaction::with_mutations(
            1,
            [
                GraphMutation::SetParameter {
                    node_id: NodeId::from_raw(2),
                    parameter_id: ParameterId::from_raw(210),
                    value: TypedParameterValue::new(
                        value_type("superi.value.scalar"),
                        "0.25".into(),
                    ),
                },
                GraphMutation::Connect {
                    edge: edge(30, (2, 202), (2, 201)),
                },
            ],
        ))
        .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    let transaction = error.contexts().last().unwrap();
    assert_eq!(transaction.component(), "superi-graph.mutation");
    assert_eq!(transaction.operation(), "apply");
    assert_eq!(transaction.field("mutation_index"), Some("1"));
    assert_eq!(transaction.field("mutation"), Some("connect"));
    assert_eq!(graph.snapshot(), before);
    assert_eq!(graph.revision(), 1);
}

#[test]
fn input_cardinality_and_parameter_types_are_enforced_atomically() {
    let mut graph = EditableGraph::new(GraphId::from_raw(7));
    graph.apply(complete_transaction(0)).unwrap();
    let before = graph.snapshot();

    let error = graph
        .apply(GraphTransaction::with_mutations(
            1,
            [
                GraphMutation::Add {
                    node_id: NodeId::from_raw(4),
                    node: source_node(),
                    position: 3,
                },
                GraphMutation::Connect {
                    edge: edge(30, (4, 101), (2, 201)),
                },
            ],
        ))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(
        error.contexts().last().unwrap().field("mutation_index"),
        Some("1")
    );
    assert_eq!(graph.snapshot(), before);

    let error = graph
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::SetParameter {
                node_id: NodeId::from_raw(2),
                parameter_id: ParameterId::from_raw(210),
                value: TypedParameterValue::new(value_type("superi.value.image"), "wrong".into()),
            }],
        ))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(
        error.contexts().last().unwrap().field("mutation"),
        Some("set_parameter")
    );
    assert_eq!(graph.snapshot(), before);

    let error = graph
        .apply(GraphTransaction::with_mutations(
            1,
            [
                GraphMutation::Add {
                    node_id: NodeId::from_raw(4),
                    node: scalar_sink_node(),
                    position: 3,
                },
                GraphMutation::Connect {
                    edge: edge(40, (1, 101), (4, 401)),
                },
            ],
        ))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.contexts()[0].field("reason"), Some("type_mismatch"));
    assert_eq!(
        error.contexts().last().unwrap().field("mutation"),
        Some("connect")
    );
    assert_eq!(graph.snapshot(), before);
}

#[test]
fn disconnect_then_remove_is_explicit_and_stale_revisions_are_rejected() {
    let mut graph = EditableGraph::new(GraphId::from_raw(7));
    graph.apply(complete_transaction(0)).unwrap();

    let committed = graph
        .apply(GraphTransaction::with_mutations(
            1,
            [
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
        .expect("explicit disconnect and remove");

    assert_eq!(committed.revision(), 2);
    assert_eq!(committed.dag().edge_count(), 0);
    assert!(committed.node(NodeId::from_raw(2)).is_none());
    assert_eq!(
        committed.node_order(),
        [3, 1].map(NodeId::from_raw).as_slice()
    );

    let before = graph.snapshot();
    let empty = graph
        .apply(GraphTransaction::new(2))
        .expect("empty current-revision transaction");
    assert_eq!(empty, before);
    assert_eq!(graph.revision(), 2);

    let error = graph
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::Reorder {
                node_id: NodeId::from_raw(1),
                position: 0,
            }],
        ))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.contexts()[0].field("expected_revision"), Some("1"));
    assert_eq!(error.contexts()[0].field("actual_revision"), Some("2"));
    assert_eq!(graph.snapshot(), before);
}

#[test]
fn explicit_positions_make_equivalent_transactions_deterministic() {
    let mut first = EditableGraph::new(GraphId::from_raw(9));
    let mut second = EditableGraph::new(GraphId::from_raw(9));

    first
        .apply(GraphTransaction::with_mutations(
            0,
            [
                GraphMutation::Add {
                    node_id: NodeId::from_raw(1),
                    node: source_node(),
                    position: 0,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(2),
                    node: transform_node(),
                    position: 1,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(3),
                    node: sink_node(),
                    position: 2,
                },
                GraphMutation::Connect {
                    edge: edge(10, (1, 101), (2, 201)),
                },
                GraphMutation::Connect {
                    edge: edge(20, (2, 202), (3, 301)),
                },
            ],
        ))
        .unwrap();

    second
        .apply(GraphTransaction::with_mutations(
            0,
            [
                GraphMutation::Add {
                    node_id: NodeId::from_raw(3),
                    node: sink_node(),
                    position: 0,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(2),
                    node: transform_node(),
                    position: 0,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(1),
                    node: source_node(),
                    position: 0,
                },
                GraphMutation::Connect {
                    edge: edge(20, (2, 202), (3, 301)),
                },
                GraphMutation::Connect {
                    edge: edge(10, (1, 101), (2, 201)),
                },
            ],
        ))
        .unwrap();

    assert_eq!(first.snapshot(), second.snapshot());
}

#[test]
fn node_instance_rejects_incomplete_or_mistyped_schema_bindings() {
    let transform_schema = schema(
        "superi.effect.transform",
        vec![PortSchema::new(
            port("image"),
            value_type("superi.value.image"),
            PortCardinality::Single,
        )],
        vec![PortSchema::new(
            port("image"),
            value_type("superi.value.image"),
            PortCardinality::Single,
        )],
        vec![ParameterSchema::new(
            parameter("opacity"),
            value_type("superi.value.scalar"),
            true,
        )],
    );

    let error = EditableNode::<Payload>::new(
        Arc::clone(&transform_schema),
        [],
        [InstancePort::new(PortId::from_raw(202), port("image"))],
        [EditableParameter::new(
            ParameterId::from_raw(210),
            parameter("opacity"),
            TypedParameterValue::new(value_type("superi.value.scalar"), "1.0".into()),
        )],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.contexts()[0].field("collection"), Some("inputs"));
    assert_eq!(error.contexts()[0].field("reason"), Some("missing_binding"));

    let error = EditableNode::<Payload>::new(
        transform_schema,
        [InstancePort::new(PortId::from_raw(201), port("image"))],
        [InstancePort::new(PortId::from_raw(202), port("image"))],
        [EditableParameter::new(
            ParameterId::from_raw(210),
            parameter("opacity"),
            TypedParameterValue::new(value_type("superi.value.image"), "wrong".into()),
        )],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.contexts()[0].field("parameter"), Some("opacity"));
    assert_eq!(
        error.contexts()[0].field("expected_type"),
        Some("superi.value.scalar")
    );
    assert_eq!(
        error.contexts()[0].field("actual_type"),
        Some("superi.value.image")
    );
}
