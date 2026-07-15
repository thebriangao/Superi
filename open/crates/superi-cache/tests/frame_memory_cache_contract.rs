use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use superi_cache::eviction::{CacheBudgetLimit, CacheBudgetManager, CacheCost, CacheMemoryBudgets};
use superi_cache::frame::{
    CacheMemoryPlacement, FrameMemoryCache, FrameMemoryCacheContext, OwnedHostFrameMemoryCache,
};
use superi_cache::key::{ParameterStateFingerprint, RenderSettingsFingerprint};
use superi_core::color_space::ColorSpace;
use superi_core::error::Result;
use superi_core::geometry::PixelBounds;
use superi_core::ids::{DeviceId, ProjectId};
use superi_core::settings::SemanticVersion;
use superi_core::time::{RationalTime, Timebase};
use superi_gpu::pool::{GpuMemoryPool, MemoryBudget, MemoryClass};
use superi_graph::dag::{DirectedAcyclicGraph, GraphEdge, GraphEndpoint};
use superi_graph::diagnostics::{IntrospectNode, NodeIntrospection, NodeStateFingerprint};
use superi_graph::eval::{
    EvaluateNode, EvaluationCacheEntryKind, EvaluationContext, EvaluationRequest, LazyEvaluator,
};
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

fn request() -> EvaluationRequest {
    EvaluationRequest::new(
        GraphEndpoint::new(NodeId::from_raw(1), PortId::from_raw(101)),
        RationalTime::new(0, Timebase::integer(24).unwrap()),
        PixelBounds::new(0, 0, 1920, 1080).unwrap(),
    )
}

fn request_for(node: u128, port: u128, min_x: i32, max_x: i32) -> EvaluationRequest {
    EvaluationRequest::new(
        GraphEndpoint::new(NodeId::from_raw(node), PortId::from_raw(port)),
        RationalTime::new(0, Timebase::integer(24).unwrap()),
        PixelBounds::new(min_x, 0, max_x, 64).unwrap(),
    )
}

fn color_pipeline() -> ColorPipelineMetadata {
    ColorPipelineMetadata::new(ImageColorTags::new(ColorSpace::UNSPECIFIED)).unwrap()
}

fn budgets(bytes: u64, frames: u64) -> CacheMemoryBudgets {
    let limit = CacheBudgetLimit::new(bytes, frames).unwrap();
    CacheMemoryBudgets::new(limit, limit, limit).unwrap()
}

fn value_cost(_kind: EvaluationCacheEntryKind, _value: &i64) -> CacheCost {
    CacheCost::new(8, 1).unwrap()
}

fn cache() -> FrameMemoryCache<i64> {
    FrameMemoryCache::new(CacheBudgetManager::new(budgets(1_024, 128)), value_cost)
}

fn context<'a>(
    color: &'a ColorPipelineMetadata,
    render_settings: &[u8],
) -> FrameMemoryCacheContext<'a> {
    FrameMemoryCacheContext::new(
        ProjectId::from_raw(1),
        CacheMemoryPlacement::Host,
        &[],
        ParameterStateFingerprint::from_canonical_bytes(b"memory-cache-test-parameters-v1"),
        color,
        RenderSettingsFingerprint::from_canonical_bytes(render_settings),
    )
}

#[test]
fn concrete_final_frame_cache_reuses_the_exact_evaluator_value() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<FrameMemoryCache<i64>>();

    let calls = Arc::new(AtomicUsize::new(0));
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(91));
    graph
        .insert_node(
            NodeId::from_raw(1),
            ValueNode {
                node_type: "superi.test.memory-cache",
                value: 29,
                policy: CachePolicy::PerRegion,
                calls: calls.clone(),
            },
        )
        .unwrap();
    let cache = cache();
    let color = color_pipeline();
    let scope = cache.scope(context(&color, b"memory-cache-test-render-v1"));

    let first = LazyEvaluator::evaluate_with_cache(&graph, request(), &scope).unwrap();
    let second = LazyEvaluator::evaluate_with_cache(&graph, request(), &scope).unwrap();

    assert_eq!(first.value(), second.value());
    assert_eq!(*second.value(), 29);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(cache.final_frame_len(), 1);
    assert_eq!(cache.intermediate_node_len(), 0);
    assert_eq!(cache.final_frame_keys().len(), 1);
    assert!(cache.intermediate_node_keys().is_empty());
}

#[test]
fn owned_host_adapter_is_send_sync_and_reuses_the_exact_evaluator_value() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<OwnedHostFrameMemoryCache<i64>>();

    let calls = Arc::new(AtomicUsize::new(0));
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(98));
    graph
        .insert_node(
            NodeId::from_raw(1),
            ValueNode {
                node_type: "superi.test.owned-host-memory-cache",
                value: 53,
                policy: CachePolicy::PerRegion,
                calls: calls.clone(),
            },
        )
        .unwrap();
    let retained = Arc::new(cache());
    let adapter = OwnedHostFrameMemoryCache::new(
        retained.clone(),
        ProjectId::from_raw(1),
        Vec::new(),
        ParameterStateFingerprint::from_canonical_bytes(b"owned-host-cache-parameters-v1"),
        color_pipeline(),
        RenderSettingsFingerprint::from_canonical_bytes(b"owned-host-cache-render-v1"),
    );

    let first = LazyEvaluator::evaluate_with_cache(&graph, request(), &adapter).unwrap();
    let second = LazyEvaluator::evaluate_with_cache(&graph, request(), &adapter).unwrap();

    assert_eq!(*first.value(), 53);
    assert_eq!(first.value(), second.value());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(retained.final_frame_len(), 1);
    assert!(Arc::ptr_eq(adapter.cache(), &retained));
}

#[test]
fn inspection_and_clear_report_exact_shared_cache_contents() {
    let calls = Arc::new(AtomicUsize::new(0));
    let graph = single_value_graph_for_management(calls.clone());
    let cache = cache();
    let color = color_pipeline();
    let scope = cache.scope(context(&color, b"memory-cache-management-v1"));

    LazyEvaluator::evaluate_with_cache(&graph, request(), &scope).unwrap();
    let inspection = cache.inspect().unwrap();
    assert_eq!(inspection.final_frame_keys().len(), 1);
    assert!(inspection.intermediate_node_keys().is_empty());
    assert_eq!(inspection.budget_stats().total_usage().bytes(), 8);
    assert_eq!(inspection.budget_stats().active_reservations(), 1);

    let report = cache.clear();
    assert_eq!(
        report.removed_final_frame_keys(),
        inspection.final_frame_keys()
    );
    assert!(report.removed_intermediate_node_keys().is_empty());
    assert_eq!(
        cache
            .inspect()
            .unwrap()
            .budget_stats()
            .total_usage()
            .bytes(),
        0
    );

    LazyEvaluator::evaluate_with_cache(&graph, request(), &scope).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

fn single_value_graph_for_management(calls: Arc<AtomicUsize>) -> DirectedAcyclicGraph<ValueNode> {
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(99));
    graph
        .insert_node(
            NodeId::from_raw(1),
            ValueNode {
                node_type: "superi.test.memory-cache-management",
                value: 59,
                policy: CachePolicy::PerRegion,
                calls,
            },
        )
        .unwrap();
    graph
}

#[test]
fn concrete_intermediate_cache_prunes_the_retained_nodes_upstream_work() {
    let source_calls = Arc::new(AtomicUsize::new(0));
    let output_calls = Arc::new(AtomicUsize::new(0));
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(93));
    graph
        .insert_node(
            NodeId::from_raw(1),
            ValueNode {
                node_type: "superi.test.memory-source",
                value: 29,
                policy: CachePolicy::Static,
                calls: source_calls.clone(),
            },
        )
        .unwrap();
    graph
        .insert_node(
            NodeId::from_raw(2),
            ValueNode {
                node_type: "superi.test.memory-output",
                value: 29,
                policy: CachePolicy::PerRegion,
                calls: output_calls.clone(),
            },
        )
        .unwrap();
    graph
        .insert_edge(GraphEdge::new(
            EdgeId::from_raw(10),
            GraphEndpoint::new(NodeId::from_raw(1), PortId::from_raw(101)),
            GraphEndpoint::new(NodeId::from_raw(2), PortId::from_raw(201)),
        ))
        .unwrap();
    let cache = cache();
    let color = color_pipeline();
    let scope = cache.scope(context(&color, b"memory-cache-test-render-v1"));

    let first =
        LazyEvaluator::evaluate_with_cache(&graph, request_for(2, 202, 0, 128), &scope).unwrap();
    let reused =
        LazyEvaluator::evaluate_with_cache(&graph, request_for(2, 202, 32, 96), &scope).unwrap();

    assert_eq!(*first.value(), 29);
    assert_eq!(*reused.value(), 29);
    assert_eq!(source_calls.load(Ordering::SeqCst), 1);
    assert_eq!(output_calls.load(Ordering::SeqCst), 2);
    assert_eq!(cache.final_frame_len(), 2);
    assert_eq!(cache.intermediate_node_len(), 1);
}

#[test]
fn changed_editable_state_never_reuses_a_stale_final_value() {
    let cache = cache();
    let color = color_pipeline();
    let scope = cache.scope(context(&color, b"memory-cache-test-render-v1"));
    let first_calls = Arc::new(AtomicUsize::new(0));
    let second_calls = Arc::new(AtomicUsize::new(0));
    let mut first_graph = DirectedAcyclicGraph::new(GraphId::from_raw(92));
    first_graph
        .insert_node(
            NodeId::from_raw(1),
            ValueNode {
                node_type: "superi.test.memory-cache",
                value: 7,
                policy: CachePolicy::PerRegion,
                calls: first_calls.clone(),
            },
        )
        .unwrap();
    let mut edited_graph = DirectedAcyclicGraph::new(GraphId::from_raw(92));
    edited_graph
        .insert_node(
            NodeId::from_raw(1),
            ValueNode {
                node_type: "superi.test.memory-cache",
                value: 31,
                policy: CachePolicy::PerRegion,
                calls: second_calls.clone(),
            },
        )
        .unwrap();

    assert_eq!(
        *LazyEvaluator::evaluate_with_cache(&first_graph, request(), &scope)
            .unwrap()
            .value(),
        7
    );
    assert_eq!(
        *LazyEvaluator::evaluate_with_cache(&edited_graph, request(), &scope)
            .unwrap()
            .value(),
        31
    );
    assert_eq!(first_calls.load(Ordering::SeqCst), 1);
    assert_eq!(second_calls.load(Ordering::SeqCst), 1);
    assert_eq!(cache.final_frame_len(), 2);
    assert!(cache
        .final_frame_keys()
        .windows(2)
        .all(|keys| keys[0] < keys[1]));
}

#[test]
fn changed_outer_render_identity_never_reuses_the_same_graph_value() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(94));
    graph
        .insert_node(
            NodeId::from_raw(1),
            ValueNode {
                node_type: "superi.test.memory-cache-context",
                value: 37,
                policy: CachePolicy::PerRegion,
                calls: calls.clone(),
            },
        )
        .unwrap();
    let cache = cache();
    let color = color_pipeline();

    {
        let preview = cache.scope(context(&color, b"artifact=preview;precision=f16"));
        LazyEvaluator::evaluate_with_cache(&graph, request(), &preview).unwrap();
    }
    {
        let export = cache.scope(context(&color, b"artifact=export;precision=f32"));
        LazyEvaluator::evaluate_with_cache(&graph, request(), &export).unwrap();
    }

    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(cache.final_frame_len(), 2);
    assert!(cache
        .final_frame_keys()
        .windows(2)
        .all(|keys| keys[0] < keys[1]));
}

#[test]
fn real_frame_budget_pressure_evicts_and_recomputes_without_changing_fresh_results() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(95));
    graph
        .insert_node(
            NodeId::from_raw(1),
            ValueNode {
                node_type: "superi.test.budgeted-memory-cache",
                value: 41,
                policy: CachePolicy::PerRegion,
                calls: calls.clone(),
            },
        )
        .unwrap();
    let manager = CacheBudgetManager::new(budgets(8, 1));
    let cache = FrameMemoryCache::new(manager.clone(), value_cost);
    let color = color_pipeline();
    let scope = cache.scope(context(&color, b"budgeted-memory-cache-test-v1"));
    let first_request = request_for(1, 101, 0, 64);
    let second_request = request_for(1, 101, 64, 128);

    assert_eq!(
        *LazyEvaluator::evaluate_with_cache(&graph, first_request, &scope)
            .unwrap()
            .value(),
        41
    );
    assert_eq!(
        *LazyEvaluator::evaluate_with_cache(&graph, second_request, &scope)
            .unwrap()
            .value(),
        41
    );
    assert_eq!(
        *LazyEvaluator::evaluate_with_cache(&graph, second_request, &scope)
            .unwrap()
            .value(),
        41
    );
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(cache.final_frame_len(), 1);
    let stats = cache.budget_stats().unwrap();
    assert_eq!(stats.total_usage().bytes(), 8);
    assert_eq!(stats.total_usage().frames(), 1);
    assert_eq!(stats.denied_reservations(), 1);

    assert_eq!(
        *LazyEvaluator::evaluate_with_cache(&graph, first_request, &scope)
            .unwrap()
            .value(),
        41
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(cache.final_frame_len(), 1);
    assert_eq!(cache.budget_stats().unwrap().denied_reservations(), 2);

    drop(cache);
    assert_eq!(manager.stats().unwrap().total_usage().bytes(), 0);
    assert_eq!(manager.stats().unwrap().active_reservations(), 0);
}

#[test]
fn identical_results_are_retained_under_their_exact_project_scope() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(97));
    graph
        .insert_node(
            NodeId::from_raw(1),
            ValueNode {
                node_type: "superi.test.project-memory-cache",
                value: 47,
                policy: CachePolicy::PerRegion,
                calls: calls.clone(),
            },
        )
        .unwrap();
    let total = CacheBudgetLimit::new(16, 2).unwrap();
    let project = CacheBudgetLimit::new(8, 1).unwrap();
    let manager = CacheBudgetManager::new(CacheMemoryBudgets::new(total, project, total).unwrap());
    let cache = FrameMemoryCache::new(manager.clone(), value_cost);
    let color = color_pipeline();
    let first_context = FrameMemoryCacheContext::new(
        ProjectId::from_raw(10),
        CacheMemoryPlacement::Host,
        &[],
        ParameterStateFingerprint::from_canonical_bytes(b"project-cache-parameters-v1"),
        &color,
        RenderSettingsFingerprint::from_canonical_bytes(b"project-cache-render-v1"),
    );
    let second_context = FrameMemoryCacheContext::new(
        ProjectId::from_raw(11),
        CacheMemoryPlacement::Host,
        &[],
        ParameterStateFingerprint::from_canonical_bytes(b"project-cache-parameters-v1"),
        &color,
        RenderSettingsFingerprint::from_canonical_bytes(b"project-cache-render-v1"),
    );
    let first_scope = cache.scope(first_context);
    let second_scope = cache.scope(second_context);

    LazyEvaluator::evaluate_with_cache(&graph, request(), &first_scope).unwrap();
    LazyEvaluator::evaluate_with_cache(&graph, request(), &second_scope).unwrap();
    LazyEvaluator::evaluate_with_cache(&graph, request(), &second_scope).unwrap();

    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(cache.final_frame_len(), 2);
    let stats = manager.stats().unwrap();
    assert_eq!(stats.project_usage(ProjectId::from_raw(10)).bytes(), 8);
    assert_eq!(stats.project_usage(ProjectId::from_raw(11)).bytes(), 8);
    assert_eq!(stats.active_projects(), 2);
}

#[test]
fn device_gpu_pressure_evicts_and_releases_the_shared_cache_class() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(96));
    graph
        .insert_node(
            NodeId::from_raw(1),
            ValueNode {
                node_type: "superi.test.device-memory-cache",
                value: 43,
                policy: CachePolicy::PerRegion,
                calls: calls.clone(),
            },
        )
        .unwrap();
    let manager = CacheBudgetManager::new(budgets(64, 8));
    let cache = FrameMemoryCache::new(manager.clone(), value_cost);
    let gpu = GpuMemoryPool::new(MemoryBudget::new(8, 8).unwrap());
    let color = color_pipeline();
    let context = FrameMemoryCacheContext::new(
        ProjectId::from_raw(2),
        CacheMemoryPlacement::Device {
            device_id: DeviceId::from_raw(3),
            gpu_memory: &gpu,
            evictors: &[],
        },
        &[],
        ParameterStateFingerprint::from_canonical_bytes(b"device-cache-parameters-v1"),
        &color,
        RenderSettingsFingerprint::from_canonical_bytes(b"device-cache-render-v1"),
    );
    let scope = cache.scope(context);

    assert_eq!(
        *LazyEvaluator::evaluate_with_cache(&graph, request(), &scope)
            .unwrap()
            .value(),
        43
    );
    assert_eq!(
        gpu.stats().unwrap().resident_bytes_for(MemoryClass::Cache),
        8
    );
    assert_eq!(
        manager
            .stats()
            .unwrap()
            .device_usage(DeviceId::from_raw(3))
            .bytes(),
        8
    );

    let second_request = request_for(1, 101, 0, 64);
    assert_eq!(
        *LazyEvaluator::evaluate_with_cache(&graph, second_request, &scope)
            .unwrap()
            .value(),
        43
    );
    LazyEvaluator::evaluate_with_cache(&graph, second_request, &scope).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(cache.final_frame_len(), 1);
    assert_eq!(gpu.stats().unwrap().resident_bytes(), 8);
    assert_eq!(gpu.stats().unwrap().denied_reservations(), 1);
    assert_eq!(manager.stats().unwrap().denied_reservations(), 0);

    LazyEvaluator::evaluate_with_cache(&graph, request(), &scope).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(cache.final_frame_len(), 1);
    assert_eq!(gpu.stats().unwrap().resident_bytes(), 8);

    cache.clear();
    assert_eq!(gpu.stats().unwrap().resident_bytes(), 0);
    assert_eq!(manager.stats().unwrap().total_usage().bytes(), 0);
    assert_eq!(manager.stats().unwrap().active_devices(), 0);
}
