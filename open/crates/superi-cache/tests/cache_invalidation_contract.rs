use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use superi_cache::eviction::{CacheBudgetLimit, CacheBudgetManager, CacheCost, CacheMemoryBudgets};
use superi_cache::frame::{CacheMemoryPlacement, FrameMemoryCache, FrameMemoryCacheContext};
use superi_cache::key::{ParameterStateFingerprint, RenderSettingsFingerprint};
use superi_core::color_space::ColorSpace;
use superi_core::error::Result;
use superi_core::geometry::PixelBounds;
use superi_core::ids::ProjectId;
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_core::time::{RationalTime, Timebase};
use superi_graph::dag::{GraphEdge, GraphEndpoint};
use superi_graph::diagnostics::{IntrospectNode, NodeIntrospection, NodeStateFingerprint};
use superi_graph::eval::{
    EvaluateNode, EvaluationCacheEntryKind, EvaluationContext, EvaluationRequest,
};
use superi_graph::headless::GraphEvaluationSnapshot;
use superi_graph::ids::{EdgeId, GraphId, NodeId, ParameterId, PortId};
use superi_graph::invalidation::derive_edit_invalidation;
use superi_graph::mutate::{
    EditableGraph, EditableNode, EditableParameter, GraphMutation, GraphSnapshot, GraphTransaction,
    InstancePort, TypedParameterValue,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchema, NodeSchemaId,
    NodeTypeId, ParameterName, ParameterSchema, PortCardinality, PortName, PortSchema, RoiBehavior,
    TimeBehavior, ValueTypeId,
};
use superi_image::metadata::{ColorPipelineMetadata, ImageColorTags};

const VALUE_PARAMETER: ParameterId = ParameterId::from_raw(900);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Scalar(i64);

#[derive(Clone)]
struct RuntimeNode {
    introspection: NodeIntrospection,
    value: i64,
    calls: Arc<AtomicUsize>,
}

impl EvaluateNode<i64> for RuntimeNode {
    fn evaluate(&self, context: &EvaluationContext<'_, i64>) -> Result<i64> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.value
            + context
                .inputs()
                .iter()
                .map(|input| input.value())
                .sum::<i64>())
    }
}

impl IntrospectNode for RuntimeNode {
    fn introspection(&self) -> NodeIntrospection {
        self.introspection.clone()
    }
}

fn value_type() -> ValueTypeId {
    ValueTypeId::new("superi.value.cache-invalidation").unwrap()
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

fn editable_graph() -> EditableGraph<Scalar> {
    let mut graph = EditableGraph::new(GraphId::from_raw(95));
    graph
        .apply(GraphTransaction::with_mutations(
            0,
            [
                GraphMutation::Add {
                    node_id: NodeId::from_raw(1),
                    node: node("superi.test.cache-source", None, PortId::from_raw(101), 5),
                    position: 0,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(2),
                    node: node(
                        "superi.test.cache-process",
                        Some(PortId::from_raw(201)),
                        PortId::from_raw(202),
                        2,
                    ),
                    position: 1,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(3),
                    node: node(
                        "superi.test.cache-output",
                        Some(PortId::from_raw(301)),
                        PortId::from_raw(302),
                        0,
                    ),
                    position: 2,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(4),
                    node: node(
                        "superi.test.cache-unrelated",
                        None,
                        PortId::from_raw(401),
                        40,
                    ),
                    position: 3,
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
    graph
}

fn compile(
    snapshot: GraphSnapshot<Scalar>,
    calls: &[Arc<AtomicUsize>; 4],
) -> GraphEvaluationSnapshot<Scalar, RuntimeNode> {
    GraphEvaluationSnapshot::compile(
        snapshot,
        |_snapshot: &GraphSnapshot<Scalar>, node_id: NodeId, node: &EditableNode<Scalar>| {
            let value = node
                .parameter(VALUE_PARAMETER)
                .expect("every test node owns a value parameter")
                .value()
                .payload()
                .0;
            Ok(RuntimeNode {
                introspection: NodeIntrospection::new(
                    node.schema().id().clone(),
                    node.schema().behavior(),
                    NodeStateFingerprint::from_canonical_bytes(value.to_be_bytes()),
                ),
                value,
                calls: calls[(node_id.raw() - 1) as usize].clone(),
            })
        },
    )
    .unwrap()
}

fn request(node: u128, port: u128) -> EvaluationRequest {
    EvaluationRequest::new(
        endpoint(node, port),
        RationalTime::new(0, Timebase::integer(24).unwrap()),
        PixelBounds::new(0, 0, 1920, 1080).unwrap(),
    )
}

fn color_pipeline() -> ColorPipelineMetadata {
    ColorPipelineMetadata::new(ImageColorTags::new(ColorSpace::UNSPECIFIED)).unwrap()
}

fn cache_budgets() -> CacheMemoryBudgets {
    let limit = CacheBudgetLimit::new(1_024, 128).unwrap();
    CacheMemoryBudgets::new(limit, limit, limit).unwrap()
}

fn value_cost(_kind: EvaluationCacheEntryKind, _value: &i64) -> CacheCost {
    CacheCost::new(8, 1).unwrap()
}

fn context(color: &ColorPipelineMetadata) -> FrameMemoryCacheContext<'_> {
    FrameMemoryCacheContext::new(
        ProjectId::from_raw(1),
        CacheMemoryPlacement::Host,
        &[],
        ParameterStateFingerprint::from_canonical_bytes(b"cache-invalidation-parameters-v1"),
        color,
        RenderSettingsFingerprint::from_canonical_bytes(b"cache-invalidation-render-v1"),
    )
}

#[test]
fn edit_invalidation_removes_only_affected_lineage_and_fences_old_snapshots() {
    let mut graph = editable_graph();
    let before = graph.snapshot();
    let calls = std::array::from_fn(|_| Arc::new(AtomicUsize::new(0)));
    let old = compile(before.clone(), &calls);
    let manager = CacheBudgetManager::new(cache_budgets());
    let cache = FrameMemoryCache::new(manager.clone(), value_cost);
    let color = color_pipeline();

    {
        let scope = cache.scope(context(&color));
        assert_eq!(
            *old.evaluate_with_cache(request(3, 302), &scope)
                .unwrap()
                .value(),
            7
        );
        assert_eq!(
            *old.evaluate_with_cache(request(4, 401), &scope)
                .unwrap()
                .value(),
            40
        );
    }
    assert_eq!(cache.final_frame_len(), 2);
    assert_eq!(cache.intermediate_node_len(), 2);
    assert_eq!(manager.stats().unwrap().total_usage().bytes(), 32);
    assert_eq!(manager.stats().unwrap().active_reservations(), 4);
    assert_eq!(
        calls.each_ref().map(|calls| calls.load(Ordering::SeqCst)),
        [1, 1, 1, 1]
    );

    let after = graph
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::SetParameter {
                node_id: NodeId::from_raw(2),
                parameter_id: VALUE_PARAMETER,
                value: TypedParameterValue::new(value_type(), Scalar(9)),
            }],
        ))
        .unwrap();
    let invalidation = derive_edit_invalidation(&before, &after).unwrap();
    assert_eq!(invalidation.roots(), [NodeId::from_raw(2)]);
    assert_eq!(
        invalidation.affected_nodes(),
        [NodeId::from_raw(2), NodeId::from_raw(3)]
    );

    let report = cache.apply_invalidation(&invalidation);
    assert_eq!(report.graph_id(), GraphId::from_raw(95));
    assert_eq!(report.from_revision(), 1);
    assert_eq!(report.to_revision(), 2);
    assert_eq!(report.removed_final_frame_keys().len(), 1);
    assert_eq!(report.removed_intermediate_node_keys().len(), 1);
    assert_eq!(cache.final_frame_len(), 1);
    assert_eq!(cache.intermediate_node_len(), 1);
    assert_eq!(manager.stats().unwrap().total_usage().bytes(), 16);
    assert_eq!(manager.stats().unwrap().active_reservations(), 2);

    let new = compile(after, &calls);
    {
        let scope = cache.scope(context(&color));
        assert_eq!(
            *new.evaluate_with_cache(request(3, 302), &scope)
                .unwrap()
                .value(),
            14
        );
        assert_eq!(
            *new.evaluate_with_cache(request(4, 401), &scope)
                .unwrap()
                .value(),
            40
        );
    }
    assert_eq!(
        calls.each_ref().map(|calls| calls.load(Ordering::SeqCst)),
        [1, 2, 2, 1]
    );
    assert_eq!(cache.final_frame_len(), 2);
    assert_eq!(cache.intermediate_node_len(), 2);
    assert_eq!(manager.stats().unwrap().total_usage().bytes(), 32);
    assert_eq!(manager.stats().unwrap().active_reservations(), 4);

    for expected_calls in [3, 4] {
        let scope = cache.scope(context(&color));
        assert_eq!(
            *old.evaluate_with_cache(request(3, 302), &scope)
                .unwrap()
                .value(),
            7
        );
        assert_eq!(calls[0].load(Ordering::SeqCst), 1);
        assert_eq!(calls[1].load(Ordering::SeqCst), expected_calls);
        assert_eq!(calls[2].load(Ordering::SeqCst), expected_calls);
        assert_eq!(calls[3].load(Ordering::SeqCst), 1);
        assert_eq!(cache.final_frame_len(), 2);
        assert_eq!(cache.intermediate_node_len(), 2);
        assert_eq!(manager.stats().unwrap().total_usage().bytes(), 32);
        assert_eq!(manager.stats().unwrap().active_reservations(), 4);
    }
}
