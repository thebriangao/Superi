use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use superi_core::geometry::PixelBounds;
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_core::time::{RationalTime, Timebase};
use superi_graph::dag::{GraphEdge, GraphEndpoint};
use superi_graph::diagnostics::{
    EvaluationCacheKey, IntrospectNode, NodeIntrospection, NodeStateFingerprint,
};
use superi_graph::eval::{
    EvaluateNode, EvaluationCacheEntryKind, EvaluationCacheIdentity, EvaluationContext,
    EvaluationDependency, EvaluationRequest, EvaluationValueCache,
};
use superi_graph::expr::{
    ExpressionParameterValue, ParameterAddress, ParameterDriver, ParameterReference,
};
use superi_graph::headless::GraphEvaluationSnapshot;
use superi_graph::ids::{EdgeId, GraphId, NodeId, ParameterId, PortId};
use superi_graph::invalidation::{
    propagate_dependency_invalidation, DirtyRegion, InvalidationSeed,
};
use superi_graph::mutate::{
    EditableGraph, EditableNode, EditableParameter, GraphMutation, GraphTransaction, InstancePort,
    TypedParameterValue,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchema, NodeSchemaId,
    NodeTypeId, ParameterName, ParameterSchema, PortCardinality, PortName, PortSchema, RoiBehavior,
    TimeBehavior, ValueTypeId,
};

const SELECTED_EDGE: EdgeId = EdgeId::from_raw(10);
const VALUE_PARAMETER: ParameterId = ParameterId::from_raw(110);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Scalar(i64);

impl ExpressionParameterValue for Scalar {
    fn to_expression_scalar(&self, _value_type: &ValueTypeId) -> Result<f64> {
        Ok(self.0 as f64)
    }

    fn from_expression_scalar(_value_type: &ValueTypeId, value: f64) -> Result<Self> {
        Ok(Self(value as i64))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum RuntimeNode {
    Constant(i64),
    Select(EdgeId),
    Fail,
}

impl EvaluateNode<i64> for RuntimeNode {
    fn dependencies(
        &self,
        request: EvaluationRequest,
        incoming: &[GraphEdge],
    ) -> Result<Vec<EvaluationDependency>> {
        match self {
            Self::Select(edge_id) => {
                Ok(vec![EvaluationDependency::same_request(*edge_id, request)])
            }
            _ => Ok(incoming
                .iter()
                .map(|edge| EvaluationDependency::same_request(edge.id(), request))
                .collect()),
        }
    }

    fn evaluate(&self, context: &EvaluationContext<'_, i64>) -> Result<i64> {
        match self {
            Self::Constant(value) => Ok(*value),
            Self::Select(_) => Ok(*context.inputs()[0].value()),
            Self::Fail => Err(Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "unused failure branch evaluated",
            )),
        }
    }
}

impl IntrospectNode for RuntimeNode {
    fn introspection(&self) -> NodeIntrospection {
        let (node_type, state) = match self {
            Self::Constant(value) => ("superi.test.constant", value.to_be_bytes().to_vec()),
            Self::Select(edge_id) => ("superi.test.selector", edge_id.to_string().into_bytes()),
            Self::Fail => ("superi.test.failure", Vec::new()),
        };
        NodeIntrospection::new(
            NodeSchemaId::new(
                NodeTypeId::new(node_type).unwrap(),
                SemanticVersion::from_str("1.0.0").unwrap(),
            ),
            behavior(),
            NodeStateFingerprint::from_canonical_bytes(state),
        )
    }
}

fn value_type() -> ValueTypeId {
    ValueTypeId::new("superi.value.scalar").unwrap()
}

fn port(name: &str) -> PortName {
    PortName::new(name).unwrap()
}

fn parameter(name: &str) -> ParameterName {
    ParameterName::new(name).unwrap()
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

#[derive(Default)]
struct SnapshotCache {
    entries: Mutex<BTreeMap<(EvaluationCacheEntryKind, EvaluationCacheKey), i64>>,
}

impl EvaluationValueCache<i64> for SnapshotCache {
    fn get(
        &self,
        kind: EvaluationCacheEntryKind,
        identity: EvaluationCacheIdentity,
    ) -> Option<i64> {
        self.entries
            .lock()
            .unwrap()
            .get(&(kind, identity.graph_key()))
            .copied()
    }

    fn insert(
        &self,
        kind: EvaluationCacheEntryKind,
        identity: EvaluationCacheIdentity,
        value: i64,
    ) {
        self.entries
            .lock()
            .unwrap()
            .insert((kind, identity.graph_key()), value);
    }
}

fn schema(
    node_type: &str,
    inputs: impl IntoIterator<Item = PortSchema>,
    outputs: impl IntoIterator<Item = PortSchema>,
    parameters: impl IntoIterator<Item = ParameterSchema>,
) -> Arc<NodeSchema> {
    Arc::new(
        NodeSchema::new(
            NodeSchemaId::new(
                NodeTypeId::new(node_type).unwrap(),
                SemanticVersion::from_str("1.0.0").unwrap(),
            ),
            inputs,
            outputs,
            parameters,
            behavior(),
            CapabilitySet::new([]),
        )
        .unwrap(),
    )
}

fn constant_node(value: i64, output: u128) -> EditableNode<Scalar> {
    EditableNode::new(
        schema(
            "superi.test.constant",
            [],
            [PortSchema::new(
                port("value"),
                value_type(),
                PortCardinality::Single,
            )],
            [ParameterSchema::new(parameter("value"), value_type(), true)],
        ),
        [],
        [InstancePort::new(PortId::from_raw(output), port("value"))],
        [EditableParameter::new(
            VALUE_PARAMETER,
            parameter("value"),
            TypedParameterValue::new(value_type(), Scalar(value)),
        )],
    )
    .unwrap()
}

fn failure_node(output: u128) -> EditableNode<Scalar> {
    EditableNode::new(
        schema(
            "superi.test.failure",
            [],
            [PortSchema::new(
                port("value"),
                value_type(),
                PortCardinality::Single,
            )],
            [],
        ),
        [],
        [InstancePort::new(PortId::from_raw(output), port("value"))],
        [],
    )
    .unwrap()
}

fn selector_node() -> EditableNode<Scalar> {
    EditableNode::new(
        schema(
            "superi.test.selector",
            [
                PortSchema::new(port("selected"), value_type(), PortCardinality::Single),
                PortSchema::new(port("unused"), value_type(), PortCardinality::Single),
            ],
            [PortSchema::new(
                port("value"),
                value_type(),
                PortCardinality::Single,
            )],
            [],
        ),
        [
            InstancePort::new(PortId::from_raw(301), port("selected")),
            InstancePort::new(PortId::from_raw(302), port("unused")),
        ],
        [InstancePort::new(PortId::from_raw(303), port("value"))],
        [],
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

fn editable_graph() -> EditableGraph<Scalar> {
    let mut graph = EditableGraph::new(GraphId::from_raw(9));
    graph
        .apply(GraphTransaction::with_mutations(
            0,
            [
                GraphMutation::Add {
                    node_id: NodeId::from_raw(3),
                    node: selector_node(),
                    position: 0,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(2),
                    node: failure_node(201),
                    position: 0,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(1),
                    node: constant_node(7, 101),
                    position: 1,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(4),
                    node: constant_node(19, 401),
                    position: 2,
                },
                GraphMutation::Connect {
                    edge: edge(20, (2, 201), (3, 302)),
                },
                GraphMutation::Connect {
                    edge: edge(10, (1, 101), (3, 301)),
                },
                GraphMutation::SetParameterDriver {
                    target: ParameterAddress::new(NodeId::from_raw(1), VALUE_PARAMETER),
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

fn compile_node(
    snapshot: &superi_graph::mutate::GraphSnapshot<Scalar>,
    node_id: NodeId,
    node: &EditableNode<Scalar>,
) -> Result<RuntimeNode> {
    match node.schema().id().node_type().as_str() {
        "superi.test.constant" => Ok(RuntimeNode::Constant(
            snapshot
                .evaluate_parameter(ParameterAddress::new(node_id, VALUE_PARAMETER))?
                .value()
                .payload()
                .0,
        )),
        "superi.test.selector" => Ok(RuntimeNode::Select(SELECTED_EDGE)),
        "superi.test.failure" => Ok(RuntimeNode::Fail),
        unexpected => panic!("unexpected test node schema {unexpected}"),
    }
}

fn request(region: PixelBounds) -> EvaluationRequest {
    EvaluationRequest::new(
        endpoint(3, 303),
        RationalTime::new(12, Timebase::integer(24).unwrap()),
        region,
    )
}

#[test]
fn editor_script_and_headless_share_one_lazy_revisioned_evaluation_snapshot() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<GraphEvaluationSnapshot<Scalar, RuntimeNode>>();

    let graph = editable_graph();
    let editable = graph.snapshot();
    let expected_snapshot = editable.clone();
    let evaluation = GraphEvaluationSnapshot::compile(editable, compile_node).unwrap();

    assert_eq!(evaluation.graph_id(), GraphId::from_raw(9));
    assert_eq!(evaluation.graph_revision(), 1);
    assert_eq!(evaluation.editable_snapshot(), &expected_snapshot);

    let invalidation = propagate_dependency_invalidation(
        expected_snapshot.dag(),
        [InvalidationSeed::from_region(
            NodeId::from_raw(1),
            DirtyRegion::Bounds(PixelBounds::new(0, 0, 16, 16).unwrap()),
        )],
    )
    .unwrap();
    let exact_work =
        invalidation.requested_work(NodeId::from_raw(3), PixelBounds::new(8, 8, 24, 24).unwrap());
    assert_eq!(
        exact_work.regions(),
        &[PixelBounds::new(8, 8, 16, 16).unwrap()]
    );
    let request = request(exact_work.regions()[0]);

    let schedule = evaluation.schedule::<i64>(request).unwrap();
    assert_eq!(schedule.len(), 2);
    let inspection = evaluation.inspect::<i64>(request).unwrap();
    assert_eq!(inspection.schedule(), &schedule);
    let reports = std::thread::scope(|scope| {
        ["editor", "script", "headless"]
            .map(|_| {
                scope.spawn(|| {
                    evaluation
                        .evaluate_with_diagnostics::<i64>(request)
                        .unwrap()
                })
            })
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });

    assert_eq!(reports[0].result(), reports[1].result());
    assert_eq!(reports[1].result(), reports[2].result());
    assert_eq!(reports[0].diagnostics().inspection(), &inspection);
    assert_eq!(reports[1].diagnostics().inspection(), &inspection);
    assert_eq!(reports[2].diagnostics().inspection(), &inspection);
    assert_eq!(*reports[0].result().value(), 19);
    assert_eq!(reports[0].result().schedule(), &schedule);
    assert_eq!(
        reports[0]
            .result()
            .evaluated_keys()
            .map(|key| key.output().node_id())
            .collect::<Vec<_>>(),
        [NodeId::from_raw(1), NodeId::from_raw(3)]
    );
    assert!(reports[0]
        .result()
        .evaluated_keys()
        .all(|key| key.region() == PixelBounds::new(8, 8, 16, 16).unwrap()));
}

#[test]
fn a_fresh_editable_revision_changes_new_evaluation_without_mutating_old_readers() {
    let mut graph = editable_graph();
    let old = GraphEvaluationSnapshot::compile(graph.snapshot(), compile_node).unwrap();
    let request = request(PixelBounds::new(0, 0, 32, 18).unwrap());
    assert_eq!(*old.evaluate::<i64>(request).unwrap().value(), 19);

    let new_snapshot = graph
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::SetParameter {
                node_id: NodeId::from_raw(4),
                parameter_id: VALUE_PARAMETER,
                value: TypedParameterValue::new(value_type(), Scalar(23)),
            }],
        ))
        .unwrap();
    let new = GraphEvaluationSnapshot::compile(new_snapshot, compile_node).unwrap();

    assert_eq!(old.graph_revision(), 1);
    assert_eq!(new.graph_revision(), 2);
    assert_eq!(*old.evaluate::<i64>(request).unwrap().value(), 19);
    assert_eq!(*new.evaluate::<i64>(request).unwrap().value(), 23);
    assert_eq!(*new.evaluate::<i64>(request).unwrap().value(), 23);
}

#[test]
fn compilation_failure_preserves_classification_and_identifies_exact_editable_state() {
    let snapshot = editable_graph().snapshot();
    let error = GraphEvaluationSnapshot::<Scalar, RuntimeNode>::compile(
        snapshot,
        |snapshot: &superi_graph::mutate::GraphSnapshot<Scalar>,
         node_id: NodeId,
         node: &EditableNode<Scalar>| {
            if node_id == NodeId::from_raw(2) {
                return Err(Error::new(
                    ErrorCategory::Unsupported,
                    Recoverability::Degraded,
                    "test catalog does not provide this node implementation",
                ));
            }
            compile_node(snapshot, node_id, node)
        },
    )
    .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::Unsupported);
    assert_eq!(error.recoverability(), Recoverability::Degraded);
    let context = error.contexts().last().unwrap();
    assert_eq!(context.component(), "superi-graph.headless");
    assert_eq!(context.operation(), "compile_node");
    assert_eq!(
        context.field("graph_id"),
        Some("graph:00000000000000000000000000000009")
    );
    assert_eq!(context.field("graph_revision"), Some("1"));
    assert_eq!(
        context.field("node_id"),
        Some("node:00000000000000000000000000000002")
    );
    assert_eq!(
        context.field("schema_id"),
        Some("superi.test.failure@1.0.0")
    );
}

#[test]
fn role_neutral_snapshot_delegates_to_the_same_retained_evaluation_path() {
    let evaluation =
        GraphEvaluationSnapshot::compile(editable_graph().snapshot(), compile_node).unwrap();
    let request = request(PixelBounds::new(0, 0, 32, 18).unwrap());
    let cache = SnapshotCache::default();

    let first = evaluation.evaluate_with_cache(request, &cache).unwrap();
    let retained = evaluation.evaluate_with_cache(request, &cache).unwrap();

    assert_eq!(*first.value(), 19);
    assert_eq!(retained.value(), first.value());
    assert_eq!(first.evaluated_keys().len(), 2);
    assert_eq!(
        retained.evaluated_keys().collect::<Vec<_>>(),
        [request.key()]
    );
}
