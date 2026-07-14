use std::str::FromStr;
use std::sync::Arc;

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use superi_core::color_space::ColorSpace;
use superi_core::error::{ErrorCategory, Result};
use superi_core::geometry::PixelBounds;
use superi_core::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION;
use superi_core::settings::{CapabilityId, CapabilitySet, SemanticVersion};
use superi_core::time::{RationalTime, Timebase};
use superi_graph::dag::{DirectedAcyclicGraph, GraphEdge, GraphEndpoint};
use superi_graph::eval::{EvaluateNode, EvaluationContext, EvaluationRequest, LazyEvaluator};
use superi_graph::ids::{EdgeId, GraphId, NodeId, ParameterId, PortId};
use superi_graph::mutate::{
    EditableGraph, EditableNode, EditableParameter, GraphMutation, GraphSnapshot, GraphTransaction,
    InstancePort, TypedParameterValue,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchema, NodeSchemaId,
    NodeTypeId, ParameterName, ParameterSchema, PortCardinality, PortName, PortSchema, RoiBehavior,
    TimeBehavior, ValueTypeId,
};
use superi_graph::serialize::{deserialize_graph, serialize_graph, GRAPH_DOCUMENT_FORMAT_REVISION};

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
    behavior: NodeBehavior,
    capabilities: &[&str],
) -> Arc<NodeSchema> {
    Arc::new(
        NodeSchema::new(
            NodeSchemaId::new(
                NodeTypeId::from_str(node_type).expect("valid node type"),
                SemanticVersion::from_str("1.2.3-rc.1+serialization").unwrap(),
            ),
            inputs,
            outputs,
            parameters,
            behavior,
            CapabilitySet::new(
                capabilities
                    .iter()
                    .map(|value| CapabilityId::from_str(value).unwrap()),
            ),
        )
        .expect("valid schema"),
    )
}

fn source_node() -> EditableNode<String> {
    EditableNode::new(
        schema(
            "superi.source.value",
            vec![],
            vec![PortSchema::new(
                port("value"),
                value_type("superi.value.scalar"),
                PortCardinality::Single,
            )],
            vec![ParameterSchema::new(
                parameter("constant"),
                value_type("superi.value.scalar"),
                false,
            )],
            NodeBehavior::new(
                TimeBehavior::Invariant,
                RoiBehavior::FullFrame,
                ColorRequirements::NotApplicable,
                Determinism::Deterministic,
                CachePolicy::Static,
            ),
            &["superi.capability.source"],
        ),
        [],
        [InstancePort::new(PortId::from_raw(101), port("value"))],
        [EditableParameter::new(
            ParameterId::from_raw(110),
            parameter("constant"),
            TypedParameterValue::new(value_type("superi.value.scalar"), "10".into()),
        )],
    )
    .unwrap()
}

fn transform_node() -> EditableNode<String> {
    EditableNode::new(
        schema(
            "superi.effect.multiply",
            vec![PortSchema::new(
                port("value"),
                value_type("superi.value.scalar"),
                PortCardinality::Single,
            )],
            vec![PortSchema::new(
                port("value"),
                value_type("superi.value.scalar"),
                PortCardinality::Single,
            )],
            vec![ParameterSchema::new(
                parameter("factor"),
                value_type("superi.value.scalar"),
                true,
            )],
            NodeBehavior::new(
                TimeBehavior::FrameWindow {
                    frames_before: 2,
                    frames_after: 3,
                },
                RoiBehavior::Expanded { pixels: 4 },
                ColorRequirements::Exact(ColorSpace::ACESCG),
                Determinism::Seeded,
                CachePolicy::PerRegion,
            ),
            &["superi.capability.gpu", "superi.capability.scalar"],
        ),
        [InstancePort::new(PortId::from_raw(201), port("value"))],
        [InstancePort::new(PortId::from_raw(202), port("value"))],
        [EditableParameter::new(
            ParameterId::from_raw(210),
            parameter("factor"),
            TypedParameterValue::new(value_type("superi.value.scalar"), "1".into()),
        )],
    )
    .unwrap()
}

fn sink_node() -> EditableNode<String> {
    EditableNode::new(
        schema(
            "superi.output.value",
            vec![PortSchema::new(
                port("value"),
                value_type("superi.value.scalar"),
                PortCardinality::Single,
            )],
            vec![PortSchema::new(
                port("value"),
                value_type("superi.value.scalar"),
                PortCardinality::Single,
            )],
            vec![],
            NodeBehavior::new(
                TimeBehavior::Unbounded,
                RoiBehavior::Custom,
                ColorRequirements::Tagged,
                Determinism::NonDeterministic,
                CachePolicy::Disabled,
            ),
            &[],
        ),
        [InstancePort::new(PortId::from_raw(301), port("value"))],
        [InstancePort::new(PortId::from_raw(302), port("value"))],
        [],
    )
    .unwrap()
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

fn editable_graph() -> EditableGraph<String> {
    let mut graph = EditableGraph::new(GraphId::from_raw(7));
    graph
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
                GraphMutation::Reorder {
                    node_id: NodeId::from_raw(3),
                    position: 0,
                },
            ],
        ))
        .unwrap();
    graph
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::SetParameter {
                node_id: NodeId::from_raw(2),
                parameter_id: ParameterId::from_raw(210),
                value: TypedParameterValue::new(value_type("superi.value.scalar"), "2".into()),
            }],
        ))
        .unwrap();
    graph
}

#[test]
fn current_document_is_deterministic_and_round_trips_complete_editable_state() {
    let graph = editable_graph();
    let snapshot = graph.snapshot();

    let first = serialize_graph(&snapshot).unwrap();
    let second = serialize_graph(&snapshot).unwrap();
    assert_eq!(first, second);

    let document: Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(document["format"], "superi.graph");
    assert_eq!(document["format_revision"], GRAPH_DOCUMENT_FORMAT_REVISION);
    assert_eq!(
        document["primitive_schema_revision"],
        STABLE_PRIMITIVE_SCHEMA_REVISION
    );
    assert_eq!(document["payload_sha256"].as_str().unwrap().len(), 64);

    let loaded = deserialize_graph::<String>(&first).unwrap();
    assert_eq!(
        loaded.source_format_revision(),
        GRAPH_DOCUMENT_FORMAT_REVISION
    );
    assert!(!loaded.was_migrated());
    assert_eq!(loaded.graph().snapshot(), snapshot);
    assert_eq!(loaded.canonical_document(), first);
    assert_eq!(serialize_graph(&loaded.graph().snapshot()).unwrap(), first);

    let mut reordered: Value = serde_json::from_slice(&first).unwrap();
    reordered["payload"]["schemas"]
        .as_array_mut()
        .unwrap()
        .reverse();
    reordered["payload"]["nodes"]
        .as_array_mut()
        .unwrap()
        .reverse();
    reordered["payload"]["edges"]
        .as_array_mut()
        .unwrap()
        .reverse();
    for schema in reordered["payload"]["schemas"].as_array_mut().unwrap() {
        schema["inputs"].as_array_mut().unwrap().reverse();
        schema["outputs"].as_array_mut().unwrap().reverse();
        schema["parameters"].as_array_mut().unwrap().reverse();
        schema["required_capabilities"]
            .as_array_mut()
            .unwrap()
            .reverse();
    }
    refresh_checksum(&mut reordered);
    let normalized = deserialize_graph::<String>(&serde_json::to_vec(&reordered).unwrap()).unwrap();
    assert_eq!(normalized.graph().snapshot(), snapshot);
    assert_eq!(normalized.canonical_document(), first);
}

#[test]
fn legacy_revision_zero_migrates_losslessly_to_validated_canonical_bytes() {
    let snapshot = editable_graph().snapshot();
    let current = serialize_graph(&snapshot).unwrap();
    let current: Value = serde_json::from_slice(&current).unwrap();
    let legacy = serde_json::to_vec(&json!({
        "format": "superi.graph",
        "format_revision": 0,
        "graph": current["payload"].clone(),
    }))
    .unwrap();

    let loaded = deserialize_graph::<String>(&legacy).unwrap();
    assert_eq!(loaded.source_format_revision(), 0);
    assert!(loaded.was_migrated());
    assert_eq!(loaded.graph().snapshot(), snapshot);

    let migrated: Value = serde_json::from_slice(loaded.canonical_document()).unwrap();
    assert_eq!(migrated["format_revision"], GRAPH_DOCUMENT_FORMAT_REVISION);
    assert_eq!(
        migrated["primitive_schema_revision"],
        STABLE_PRIMITIVE_SCHEMA_REVISION
    );
    let reloaded = deserialize_graph::<String>(loaded.canonical_document()).unwrap();
    assert!(!reloaded.was_migrated());
    assert_eq!(reloaded.graph().snapshot(), snapshot);
    assert_eq!(reloaded.canonical_document(), loaded.canonical_document());
}

#[test]
fn corruption_future_revisions_and_unknown_fields_fail_without_partial_state() {
    let bytes = serialize_graph(&editable_graph().snapshot()).unwrap();

    let mut corrupt: Value = serde_json::from_slice(&bytes).unwrap();
    corrupt["payload"]["nodes"][0]["parameters"][0]["payload"] = json!(999);
    let error = deserialize_graph::<String>(&serde_json::to_vec(&corrupt).unwrap()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);

    let mut future: Value = serde_json::from_slice(&bytes).unwrap();
    future["format_revision"] = json!(GRAPH_DOCUMENT_FORMAT_REVISION + 1);
    let error = deserialize_graph::<String>(&serde_json::to_vec(&future).unwrap()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);

    let mut primitive: Value = serde_json::from_slice(&bytes).unwrap();
    primitive["primitive_schema_revision"] = json!(STABLE_PRIMITIVE_SCHEMA_REVISION + 1);
    let error = deserialize_graph::<String>(&serde_json::to_vec(&primitive).unwrap()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);

    let mut unknown: Value = serde_json::from_slice(&bytes).unwrap();
    unknown["future"] = json!(true);
    let error = deserialize_graph::<String>(&serde_json::to_vec(&unknown).unwrap()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);

    let truncated = &bytes[..bytes.len() / 2];
    let error = deserialize_graph::<String>(truncated).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);
}

#[test]
fn semantic_validation_rejects_invalid_order_bindings_and_cycles_after_integrity_passes() {
    let bytes = serialize_graph(&editable_graph().snapshot()).unwrap();

    let mut order: Value = serde_json::from_slice(&bytes).unwrap();
    order["payload"]["node_order"][1] = order["payload"]["node_order"][0].clone();
    refresh_checksum(&mut order);
    let error = deserialize_graph::<String>(&serde_json::to_vec(&order).unwrap()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let mut binding: Value = serde_json::from_slice(&bytes).unwrap();
    binding["payload"]["nodes"][1]["parameters"][0]["value_type"] = json!("superi.value.image");
    refresh_checksum(&mut binding);
    let error = deserialize_graph::<String>(&serde_json::to_vec(&binding).unwrap()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let mut cycle: Value = serde_json::from_slice(&bytes).unwrap();
    cycle["payload"]["edges"].as_array_mut().unwrap().remove(0);
    cycle["payload"]["edges"]
        .as_array_mut()
        .unwrap()
        .push(json!({
            "id": "edge:0000000000000000000000000000001e",
            "source": {
                "node_id": "node:00000000000000000000000000000002",
                "port_id": "port:000000000000000000000000000000ca"
            },
            "destination": {
                "node_id": "node:00000000000000000000000000000002",
                "port_id": "port:000000000000000000000000000000c9"
            }
        }));
    refresh_checksum(&mut cycle);
    let error = deserialize_graph::<String>(&serde_json::to_vec(&cycle).unwrap()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
}

#[derive(Clone)]
enum ExecutableNode {
    Constant(i64),
    Multiply(i64),
    Output,
}

impl EvaluateNode<i64> for ExecutableNode {
    fn evaluate(&self, context: &EvaluationContext<'_, i64>) -> Result<i64> {
        match self {
            Self::Constant(value) => Ok(*value),
            Self::Multiply(factor) => Ok(context.inputs()[0].value() * factor),
            Self::Output => Ok(*context.inputs()[0].value()),
        }
    }
}

fn compile(snapshot: &GraphSnapshot<String>) -> DirectedAcyclicGraph<ExecutableNode> {
    let mut executable = DirectedAcyclicGraph::new(snapshot.graph_id());
    for (node_id, node) in snapshot.dag().nodes() {
        let implementation = match node.schema().id().node_type().as_str() {
            "superi.source.value" => ExecutableNode::Constant(
                node.parameter(ParameterId::from_raw(110))
                    .unwrap()
                    .value()
                    .payload()
                    .parse()
                    .unwrap(),
            ),
            "superi.effect.multiply" => ExecutableNode::Multiply(
                node.parameter(ParameterId::from_raw(210))
                    .unwrap()
                    .value()
                    .payload()
                    .parse()
                    .unwrap(),
            ),
            "superi.output.value" => ExecutableNode::Output,
            unexpected => panic!("unexpected schema {unexpected}"),
        };
        executable.insert_node(*node_id, implementation).unwrap();
    }
    for edge in snapshot.dag().edges().values() {
        executable.insert_edge(*edge).unwrap();
    }
    executable
}

#[test]
fn editor_script_and_headless_load_the_same_state_and_evaluation_result() {
    let bytes = serialize_graph(&editable_graph().snapshot()).unwrap();
    let observations = ["editor", "script", "headless"].map(|_| {
        let loaded = deserialize_graph::<String>(&bytes).unwrap();
        let snapshot = loaded.graph().snapshot();
        let executable = compile(&snapshot);
        let result = LazyEvaluator::evaluate(
            &executable,
            EvaluationRequest::new(
                GraphEndpoint::new(NodeId::from_raw(3), PortId::from_raw(302)),
                RationalTime::new(7, Timebase::integer(24).unwrap()),
                PixelBounds::new(0, 0, 1920, 1080).unwrap(),
            ),
        )
        .unwrap();
        (snapshot, *result.value())
    });

    assert_eq!(observations[0], observations[1]);
    assert_eq!(observations[1], observations[2]);
    assert_eq!(observations[0].1, 20);
}

fn refresh_checksum(document: &mut Value) {
    let payload = serde_json::to_vec(&document["payload"]).unwrap();
    document["payload_sha256"] = json!(format!("{:x}", Sha256::digest(payload)));
}
