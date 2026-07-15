use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use superi_cache::eviction::{
    CacheBudgetLimit, CacheBudgetManager, CacheCost, CacheMemoryBudgets, MemoryCacheTier,
};
use superi_cache::frame::{CacheMemoryPlacement, FrameMemoryCache, FrameMemoryCacheContext};
use superi_cache::key::{ParameterStateFingerprint, RenderSettingsFingerprint};
use superi_core::color_space::ColorSpace;
use superi_core::error::Result;
use superi_core::geometry::PixelBounds;
use superi_core::ids::ProjectId;
use superi_core::settings::SemanticVersion;
use superi_core::time::{RationalTime, Timebase};
use superi_graph::dag::{DirectedAcyclicGraph, GraphEdge, GraphEndpoint};
use superi_graph::diagnostics::{IntrospectNode, NodeIntrospection, NodeStateFingerprint};
use superi_graph::eval::EvaluationCacheEntryKind;
use superi_graph::eval::{EvaluateNode, EvaluationContext, EvaluationRequest, LazyEvaluator};
use superi_graph::ids::{EdgeId, GraphId, NodeId, PortId};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchemaId, NodeTypeId,
    RoiBehavior, TimeBehavior,
};
use superi_image::metadata::{ColorPipelineMetadata, ImageColorTags};

#[derive(Clone)]
struct ValueNode {
    node_type: &'static str,
    value: i64,
    policy: CachePolicy,
    calls: Arc<AtomicUsize>,
}

impl EvaluateNode<i64> for ValueNode {
    fn evaluate(&self, _context: &EvaluationContext<'_, i64>) -> Result<i64> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.value)
    }
}

impl IntrospectNode for ValueNode {
    fn introspection(&self) -> NodeIntrospection {
        NodeIntrospection::new(
            NodeSchemaId::new(
                NodeTypeId::from_str(self.node_type).unwrap(),
                SemanticVersion::from_str("1.0.0").unwrap(),
            ),
            NodeBehavior::new(
                if self.policy == CachePolicy::Static {
                    TimeBehavior::Invariant
                } else {
                    TimeBehavior::CurrentFrame
                },
                RoiBehavior::InputBounds,
                ColorRequirements::NotApplicable,
                Determinism::Deterministic,
                self.policy,
            ),
            NodeStateFingerprint::from_canonical_bytes(self.value.to_be_bytes()),
        )
    }
}

fn request(node: u128, port: u128, min_x: i32, max_x: i32) -> EvaluationRequest {
    EvaluationRequest::new(
        GraphEndpoint::new(NodeId::from_raw(node), PortId::from_raw(port)),
        RationalTime::new(0, Timebase::integer(24).unwrap()),
        PixelBounds::new(min_x, 0, max_x, 64).unwrap(),
    )
}

fn color_pipeline() -> ColorPipelineMetadata {
    ColorPipelineMetadata::new(ImageColorTags::new(ColorSpace::UNSPECIFIED)).unwrap()
}

fn value_cost(_kind: EvaluationCacheEntryKind, _value: &i64) -> CacheCost {
    CacheCost::new(8, 1).unwrap()
}

fn cache(bytes: u64, frames: u64) -> FrameMemoryCache<i64> {
    let limit = CacheBudgetLimit::new(bytes, frames).unwrap();
    let budgets = CacheMemoryBudgets::new(limit, limit, limit).unwrap();
    FrameMemoryCache::new(CacheBudgetManager::new(budgets), value_cost)
}

fn context(color: &ColorPipelineMetadata) -> FrameMemoryCacheContext<'_> {
    context_for(ProjectId::from_raw(1), color)
}

fn context_for(
    project_id: ProjectId,
    color: &ColorPipelineMetadata,
) -> FrameMemoryCacheContext<'_> {
    FrameMemoryCacheContext::new(
        project_id,
        CacheMemoryPlacement::Host,
        &[],
        ParameterStateFingerprint::from_canonical_bytes(b"eviction-test-parameters-v1"),
        color,
        RenderSettingsFingerprint::from_canonical_bytes(b"eviction-test-render-v1"),
    )
}

#[test]
fn project_pressure_evicts_only_that_projects_lru_result() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(503));
    graph
        .insert_node(
            NodeId::from_raw(1),
            ValueNode {
                node_type: "superi.test.eviction-project",
                value: 83,
                policy: CachePolicy::PerRegion,
                calls: calls.clone(),
            },
        )
        .unwrap();

    let total = CacheBudgetLimit::new(24, 3).unwrap();
    let project = CacheBudgetLimit::new(8, 1).unwrap();
    let budgets = CacheMemoryBudgets::new(total, project, total).unwrap();
    let cache = FrameMemoryCache::new(CacheBudgetManager::new(budgets), value_cost);
    let color = color_pipeline();
    let first_project = cache.scope(context_for(ProjectId::from_raw(10), &color));
    let second_project = cache.scope(context_for(ProjectId::from_raw(11), &color));
    let first_request = request(1, 101, 0, 32);
    let replacement_request = request(1, 101, 32, 64);

    LazyEvaluator::evaluate_with_cache(&graph, first_request, &first_project).unwrap();
    LazyEvaluator::evaluate_with_cache(&graph, first_request, &second_project).unwrap();
    LazyEvaluator::evaluate_with_cache(&graph, first_request, &first_project).unwrap();
    LazyEvaluator::evaluate_with_cache(&graph, replacement_request, &first_project).unwrap();
    LazyEvaluator::evaluate_with_cache(&graph, first_request, &second_project).unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(cache.final_frame_len(), 2);

    LazyEvaluator::evaluate_with_cache(&graph, first_request, &first_project).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 4);
}

#[test]
fn automatic_pressure_reclaims_intermediates_before_final_frames() {
    let source_calls = Arc::new(AtomicUsize::new(0));
    let output_calls = Arc::new(AtomicUsize::new(0));
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(504));
    graph
        .insert_node(
            NodeId::from_raw(1),
            ValueNode {
                node_type: "superi.test.eviction-priority-source",
                value: 89,
                policy: CachePolicy::Static,
                calls: source_calls.clone(),
            },
        )
        .unwrap();
    graph
        .insert_node(
            NodeId::from_raw(2),
            ValueNode {
                node_type: "superi.test.eviction-priority-output",
                value: 89,
                policy: CachePolicy::PerRegion,
                calls: output_calls.clone(),
            },
        )
        .unwrap();
    graph
        .insert_edge(GraphEdge::new(
            EdgeId::from_raw(5002),
            GraphEndpoint::new(NodeId::from_raw(1), PortId::from_raw(101)),
            GraphEndpoint::new(NodeId::from_raw(2), PortId::from_raw(201)),
        ))
        .unwrap();

    let cache = cache(16, 2);
    let color = color_pipeline();
    let scope = cache.scope(context(&color));

    let first = LazyEvaluator::evaluate_with_cache(&graph, request(2, 202, 0, 32), &scope).unwrap();
    let second =
        LazyEvaluator::evaluate_with_cache(&graph, request(2, 202, 32, 64), &scope).unwrap();

    assert_eq!(*first.value(), 89);
    assert_eq!(first.value(), second.value());
    assert_eq!(source_calls.load(Ordering::SeqCst), 1);
    assert_eq!(output_calls.load(Ordering::SeqCst), 2);
    assert_eq!(cache.intermediate_node_len(), 0);
    assert_eq!(cache.final_frame_len(), 2);

    let third =
        LazyEvaluator::evaluate_with_cache(&graph, request(2, 202, 64, 96), &scope).unwrap();
    assert_eq!(*third.value(), 89);
    assert_eq!(source_calls.load(Ordering::SeqCst), 2);
    assert_eq!(output_calls.load(Ordering::SeqCst), 3);
    assert_eq!(cache.intermediate_node_len(), 0);
    assert_eq!(cache.final_frame_len(), 2);
}

#[test]
fn final_frame_lru_promotes_hits_and_reports_exact_victim_order() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(501));
    graph
        .insert_node(
            NodeId::from_raw(1),
            ValueNode {
                node_type: "superi.test.eviction-final",
                value: 41,
                policy: CachePolicy::PerRegion,
                calls: calls.clone(),
            },
        )
        .unwrap();

    let cache = cache(1_024, 128);
    let color = color_pipeline();
    let scope = cache.scope(context(&color));
    let first_request = request(1, 101, 0, 32);
    let second_request = request(1, 101, 32, 64);
    let third_request = request(1, 101, 64, 96);

    assert_eq!(
        *LazyEvaluator::evaluate_with_cache(&graph, first_request, &scope)
            .unwrap()
            .value(),
        41
    );
    let first_key = cache.final_frame_keys()[0];

    LazyEvaluator::evaluate_with_cache(&graph, second_request, &scope).unwrap();
    let second_key = cache
        .final_frame_keys()
        .into_iter()
        .find(|key| *key != first_key)
        .unwrap();

    LazyEvaluator::evaluate_with_cache(&graph, third_request, &scope).unwrap();
    let third_key = cache
        .final_frame_keys()
        .into_iter()
        .find(|key| *key != first_key && *key != second_key)
        .unwrap();

    LazyEvaluator::evaluate_with_cache(&graph, first_request, &scope).unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "the first request was a hit"
    );

    assert_eq!(
        cache.evict_lru(MemoryCacheTier::FinalFrame, 2),
        vec![second_key, third_key]
    );
    assert_eq!(cache.final_frame_keys(), vec![first_key]);
    assert!(cache
        .evict_lru(MemoryCacheTier::IntermediateNode, usize::MAX)
        .is_empty());

    LazyEvaluator::evaluate_with_cache(&graph, first_request, &scope).unwrap();
    assert_eq!(
        calls.load(Ordering::SeqCst),
        3,
        "the promoted value remained"
    );

    let recomputed = LazyEvaluator::evaluate_with_cache(&graph, second_request, &scope).unwrap();
    assert_eq!(*recomputed.value(), 41);
    assert_eq!(calls.load(Ordering::SeqCst), 4);

    let remaining = cache.evict_lru(MemoryCacheTier::FinalFrame, usize::MAX);
    assert_eq!(remaining.len(), 2);
    assert_eq!(remaining[0], first_key);
    assert!(cache.final_frame_keys().is_empty());
}

#[test]
fn evicted_intermediate_work_recomputes_without_changing_result_meaning() {
    let source_calls = Arc::new(AtomicUsize::new(0));
    let output_calls = Arc::new(AtomicUsize::new(0));
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(502));
    graph
        .insert_node(
            NodeId::from_raw(1),
            ValueNode {
                node_type: "superi.test.eviction-source",
                value: 73,
                policy: CachePolicy::Static,
                calls: source_calls.clone(),
            },
        )
        .unwrap();
    graph
        .insert_node(
            NodeId::from_raw(2),
            ValueNode {
                node_type: "superi.test.eviction-output",
                value: 73,
                policy: CachePolicy::PerRegion,
                calls: output_calls.clone(),
            },
        )
        .unwrap();
    graph
        .insert_edge(GraphEdge::new(
            EdgeId::from_raw(5001),
            GraphEndpoint::new(NodeId::from_raw(1), PortId::from_raw(101)),
            GraphEndpoint::new(NodeId::from_raw(2), PortId::from_raw(201)),
        ))
        .unwrap();

    let cache = cache(1_024, 128);
    let color = color_pipeline();
    let scope = cache.scope(context(&color));
    let first = LazyEvaluator::evaluate_with_cache(&graph, request(2, 202, 0, 32), &scope).unwrap();
    let retained =
        LazyEvaluator::evaluate_with_cache(&graph, request(2, 202, 32, 64), &scope).unwrap();

    assert_eq!(first.value(), retained.value());
    assert_eq!(*retained.value(), 73);
    assert_eq!(source_calls.load(Ordering::SeqCst), 1);
    assert_eq!(output_calls.load(Ordering::SeqCst), 2);
    assert_eq!(cache.intermediate_node_len(), 1);
    assert_eq!(cache.final_frame_len(), 2);

    let intermediate_key = cache.intermediate_node_keys()[0];
    assert_eq!(
        cache.evict_lru(MemoryCacheTier::IntermediateNode, 1),
        vec![intermediate_key]
    );
    assert_eq!(cache.intermediate_node_len(), 0);
    assert_eq!(cache.final_frame_len(), 2, "tiers remain independent");

    let recomputed =
        LazyEvaluator::evaluate_with_cache(&graph, request(2, 202, 64, 96), &scope).unwrap();
    assert_eq!(*recomputed.value(), 73);
    assert_eq!(source_calls.load(Ordering::SeqCst), 2);
    assert_eq!(output_calls.load(Ordering::SeqCst), 3);
}
