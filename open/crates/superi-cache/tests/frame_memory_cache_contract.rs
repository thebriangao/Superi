use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use superi_cache::frame::{FrameMemoryCache, FrameMemoryCacheContext};
use superi_cache::key::{ParameterStateFingerprint, RenderSettingsFingerprint};
use superi_core::color_space::ColorSpace;
use superi_core::error::Result;
use superi_core::geometry::PixelBounds;
use superi_core::settings::SemanticVersion;
use superi_core::time::{RationalTime, Timebase};
use superi_graph::dag::{DirectedAcyclicGraph, GraphEdge, GraphEndpoint};
use superi_graph::diagnostics::{IntrospectNode, NodeIntrospection, NodeStateFingerprint};
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

fn context<'a>(
    color: &'a ColorPipelineMetadata,
    render_settings: &[u8],
) -> FrameMemoryCacheContext<'a> {
    FrameMemoryCacheContext::new(
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
    let cache = FrameMemoryCache::new();
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
    let cache = FrameMemoryCache::new();
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
    let cache = FrameMemoryCache::new();
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
    let cache = FrameMemoryCache::new();
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
