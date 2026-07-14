use std::str::FromStr;
use std::sync::Arc;

use superi_core::error::{ErrorCategory, Recoverability, Result};
use superi_core::geometry::PixelBounds;
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_core::time::{RationalTime, Timebase};
use superi_graph::dag::{GraphEdge, GraphEndpoint};
use superi_graph::eval::{EvaluateNode, EvaluationContext, EvaluationRequest};
use superi_graph::headless::GraphEvaluationSnapshot;
use superi_graph::ids::{EdgeId, GraphId, NodeId, ParameterId, PortId};
use superi_graph::missing::{resolve_graph, GraphResolution, MissingNodeReason, NodeAvailability};
use superi_graph::mutate::{
    EditableGraph, EditableNode, EditableParameter, GraphMutation, GraphSnapshot, GraphTransaction,
    InstancePort, TypedParameterValue,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeRegistry, NodeSchema,
    NodeSchemaId, NodeTypeId, ParameterName, ParameterSchema, PortCardinality, PortName,
    PortSchema, RoiBehavior, TimeBehavior, ValueTypeId,
};
use superi_graph::serialize::{deserialize_graph, serialize_graph};

const SOURCE_NODE: NodeId = NodeId::from_raw(1);
const PLUGIN_NODE: NodeId = NodeId::from_raw(2);
const OUTPUT_NODE: NodeId = NodeId::from_raw(3);
const SOURCE_PARAMETER: ParameterId = ParameterId::from_raw(110);
const PLUGIN_PARAMETER: ParameterId = ParameterId::from_raw(210);

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
                ColorRequirements::NotApplicable,
                Determinism::Deterministic,
                CachePolicy::PerRegion,
            ),
            CapabilitySet::default(),
        )
        .expect("valid schema"),
    )
}

fn source_schema() -> Arc<NodeSchema> {
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
    )
}

fn plugin_schema(parameter_type: &str) -> Arc<NodeSchema> {
    schema(
        "vendor.effect.multiply",
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
            value_type(parameter_type),
            true,
        )],
    )
}

fn output_schema() -> Arc<NodeSchema> {
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
    )
}

fn source_node(schema: Arc<NodeSchema>) -> EditableNode<String> {
    EditableNode::new(
        schema,
        [],
        [InstancePort::new(PortId::from_raw(101), port("value"))],
        [EditableParameter::new(
            SOURCE_PARAMETER,
            parameter("constant"),
            TypedParameterValue::new(value_type("superi.value.scalar"), "10".into()),
        )],
    )
    .expect("valid source node")
}

fn plugin_node(schema: Arc<NodeSchema>) -> EditableNode<String> {
    EditableNode::new(
        schema,
        [InstancePort::new(PortId::from_raw(201), port("value"))],
        [InstancePort::new(PortId::from_raw(202), port("value"))],
        [EditableParameter::new(
            PLUGIN_PARAMETER,
            parameter("factor"),
            TypedParameterValue::new(value_type("superi.value.scalar"), "2".into()),
        )],
    )
    .expect("valid plugin node")
}

fn output_node(schema: Arc<NodeSchema>) -> EditableNode<String> {
    EditableNode::new(
        schema,
        [InstancePort::new(PortId::from_raw(301), port("value"))],
        [InstancePort::new(PortId::from_raw(302), port("value"))],
        [],
    )
    .expect("valid output node")
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

struct Fixture {
    graph: EditableGraph<String>,
    source_schema: Arc<NodeSchema>,
    plugin_schema: Arc<NodeSchema>,
    output_schema: Arc<NodeSchema>,
}

fn fixture() -> Fixture {
    let source_schema = source_schema();
    let plugin_schema = plugin_schema("superi.value.scalar");
    let output_schema = output_schema();
    let mut graph = EditableGraph::new(GraphId::from_raw(7));
    graph
        .apply(GraphTransaction::with_mutations(
            0,
            [
                GraphMutation::Add {
                    node_id: SOURCE_NODE,
                    node: source_node(Arc::clone(&source_schema)),
                    position: 0,
                },
                GraphMutation::Add {
                    node_id: PLUGIN_NODE,
                    node: plugin_node(Arc::clone(&plugin_schema)),
                    position: 1,
                },
                GraphMutation::Add {
                    node_id: OUTPUT_NODE,
                    node: output_node(Arc::clone(&output_schema)),
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
        .expect("valid editable graph");
    Fixture {
        graph,
        source_schema,
        plugin_schema,
        output_schema,
    }
}

fn registry(schemas: impl IntoIterator<Item = Arc<NodeSchema>>) -> NodeRegistry {
    let mut registry = NodeRegistry::new();
    registry
        .register_batch(schemas.into_iter().map(|schema| schema.as_ref().clone()))
        .expect("unique schemas");
    registry
}

#[test]
fn unavailable_plugin_loads_as_an_inspectable_placeholder_without_rewriting_state() {
    let fixture = fixture();
    let original = fixture.graph.snapshot();
    let document = serialize_graph(&original).expect("serialize fixture");
    let loaded = deserialize_graph::<String>(&document).expect("load without plugin discovery");
    assert_eq!(loaded.graph().snapshot(), original);

    let registry = registry([fixture.source_schema, fixture.output_schema]);
    let resolution = resolve_graph(&loaded.graph().snapshot(), &registry.snapshot());
    assert_eq!(resolution.graph(), &original);
    assert_eq!(resolution.registry_revision(), 1);
    assert_eq!(resolution.missing_node_count(), 1);
    assert!(!resolution.is_evaluable());

    let resolved = resolution
        .node(PLUGIN_NODE)
        .expect("plugin node remains present");
    let placeholder = resolved.placeholder().expect("plugin is unavailable");
    assert_eq!(placeholder.node_id(), PLUGIN_NODE);
    assert_eq!(placeholder.schema_id(), fixture.plugin_schema.id());
    assert_eq!(placeholder.reason(), MissingNodeReason::UnregisteredSchema);
    assert_eq!(resolved.node().schema(), fixture.plugin_schema.as_ref());
    assert_eq!(
        resolved
            .node()
            .parameter(PLUGIN_PARAMETER)
            .expect("typed parameter remains")
            .value()
            .payload(),
        "2"
    );
    assert_eq!(
        resolved.node().inputs()[&PortId::from_raw(201)],
        port("value")
    );
    assert_eq!(
        resolved.node().outputs()[&PortId::from_raw(202)],
        port("value")
    );
    assert_eq!(resolution.graph().dag().edge_count(), 2);
    assert_eq!(serialize_graph(resolution.graph()).unwrap(), document);

    let error = resolution.require_evaluable().unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unavailable);
    assert_eq!(error.recoverability(), Recoverability::Degraded);
    assert_eq!(error.contexts()[0].component(), "superi-graph.missing-node");
    assert_eq!(error.contexts()[0].operation(), "require_evaluable");
    assert_eq!(error.contexts()[0].field("missing_node_count"), Some("1"));
    assert_eq!(
        error.contexts()[0].field("missing_nodes"),
        Some("node:00000000000000000000000000000002=vendor.effect.multiply@1.0.0:unregistered_schema")
    );
}

#[test]
fn missing_nodes_stay_editable_fail_closed_on_schema_collision_and_recover_exactly() {
    let fixture = fixture();
    let document = serialize_graph(&fixture.graph.snapshot()).unwrap();
    let mut graph = deserialize_graph::<String>(&document).unwrap().into_graph();
    graph
        .apply(GraphTransaction::with_mutations(
            graph.revision(),
            [GraphMutation::SetParameter {
                node_id: PLUGIN_NODE,
                parameter_id: PLUGIN_PARAMETER,
                value: TypedParameterValue::new(value_type("superi.value.scalar"), "3".into()),
            }],
        ))
        .expect("missing plugin state remains editable");

    let incompatible = plugin_schema("superi.value.image");
    let incompatible_registry = registry([
        Arc::clone(&fixture.source_schema),
        incompatible,
        Arc::clone(&fixture.output_schema),
    ]);
    let incompatible_resolution =
        resolve_graph(&graph.snapshot(), &incompatible_registry.snapshot());
    assert_eq!(
        incompatible_resolution
            .node(PLUGIN_NODE)
            .unwrap()
            .placeholder()
            .unwrap()
            .reason(),
        MissingNodeReason::IncompatibleSchema
    );
    assert_eq!(
        incompatible_resolution
            .node(PLUGIN_NODE)
            .unwrap()
            .node()
            .schema(),
        fixture.plugin_schema.as_ref()
    );

    let exact_registry = registry([
        fixture.source_schema,
        fixture.plugin_schema,
        fixture.output_schema,
    ]);
    let exact_resolution = resolve_graph(&graph.snapshot(), &exact_registry.snapshot());
    assert!(exact_resolution.is_evaluable());
    assert_eq!(exact_resolution.missing_node_count(), 0);
    assert_eq!(
        exact_resolution.node(PLUGIN_NODE).unwrap().availability(),
        &NodeAvailability::Available
    );
    assert_eq!(
        exact_resolution
            .require_evaluable()
            .unwrap()
            .node(PLUGIN_NODE)
            .unwrap()
            .parameter(PLUGIN_PARAMETER)
            .unwrap()
            .value()
            .payload(),
        "3"
    );
    assert_ne!(serialize_graph(exact_resolution.graph()).unwrap(), document);
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

fn compile(
    resolution: &GraphResolution<String>,
) -> Result<GraphEvaluationSnapshot<String, ExecutableNode>> {
    let snapshot = resolution.require_evaluable()?;
    GraphEvaluationSnapshot::compile(
        snapshot.clone(),
        |_snapshot: &GraphSnapshot<String>, _node_id, node: &EditableNode<String>| {
            Ok(match node.schema().id().node_type().as_str() {
                "superi.source.value" => ExecutableNode::Constant(
                    node.parameter(SOURCE_PARAMETER)
                        .expect("source parameter")
                        .value()
                        .payload()
                        .parse()
                        .expect("integer source"),
                ),
                "vendor.effect.multiply" => ExecutableNode::Multiply(
                    node.parameter(PLUGIN_PARAMETER)
                        .expect("plugin parameter")
                        .value()
                        .payload()
                        .parse()
                        .expect("integer factor"),
                ),
                "superi.output.value" => ExecutableNode::Output,
                unexpected => panic!("unexpected schema {unexpected}"),
            })
        },
    )
}

fn evaluate(resolution: &GraphResolution<String>) -> Result<i64> {
    let executable = compile(resolution)?;
    executable
        .evaluate(EvaluationRequest::new(
            GraphEndpoint::new(OUTPUT_NODE, PortId::from_raw(302)),
            RationalTime::new(7, Timebase::integer(24).unwrap()),
            PixelBounds::new(0, 0, 1920, 1080).unwrap(),
        ))
        .map(|result| *result.value())
}

#[test]
fn editor_script_and_headless_observe_one_state_and_evaluation_result() {
    let fixture = fixture();
    let snapshot = fixture.graph.snapshot();
    let incomplete_registry = NodeRegistry::new().snapshot();

    let failures = ["editor", "script", "headless"].map(|_| {
        let resolution = resolve_graph(&snapshot, &incomplete_registry);
        let node_order = resolution
            .nodes()
            .map(|node| node.node_id())
            .collect::<Vec<_>>();
        let missing_order = resolution
            .missing_nodes()
            .map(|node| node.node_id())
            .collect::<Vec<_>>();
        let error = evaluate(&resolution).unwrap_err();
        (
            resolution.graph().clone(),
            node_order,
            missing_order,
            error.category(),
            error.recoverability(),
            error.contexts()[0].fields().clone(),
        )
    });
    assert_eq!(failures[0], failures[1]);
    assert_eq!(failures[1], failures[2]);
    assert_eq!(
        failures[0].2,
        [SOURCE_NODE, PLUGIN_NODE, OUTPUT_NODE].as_slice()
    );
    assert_eq!(
        failures[0].5.get("missing_node_count").map(String::as_str),
        Some("3")
    );

    let complete_registry = registry([
        fixture.source_schema,
        fixture.plugin_schema,
        fixture.output_schema,
    ]);
    let complete_registry = complete_registry.snapshot();
    let results = ["editor", "script", "headless"].map(|_| {
        let resolution = resolve_graph(&snapshot, &complete_registry);
        (resolution.graph().clone(), evaluate(&resolution).unwrap())
    });
    assert_eq!(results[0], results[1]);
    assert_eq!(results[1], results[2]);
    assert_eq!(results[0].1, 20);
}
