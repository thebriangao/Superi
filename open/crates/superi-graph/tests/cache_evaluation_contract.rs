use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use superi_core::geometry::PixelBounds;
use superi_core::settings::SemanticVersion;
use superi_core::time::{RationalTime, Timebase};
use superi_graph::dag::{DirectedAcyclicGraph, GraphEdge, GraphEndpoint};
use superi_graph::diagnostics::{
    EvaluationCacheKey, IntrospectNode, NodeIntrospection, NodeStateFingerprint,
};
use superi_graph::eval::{
    EvaluateNode, EvaluationCacheEntryKind, EvaluationCacheIdentity, EvaluationContext,
    EvaluationKey, EvaluationRequest, EvaluationValueCache, LazyEvaluator,
};
use superi_graph::ids::{EdgeId, GraphId, NodeId, PortId};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchemaId, NodeTypeId,
    RoiBehavior, TimeBehavior,
};

#[derive(Clone, Copy)]
enum Operation {
    Constant(i64),
    Sum,
    Fail,
}

#[derive(Clone)]
struct CacheNode {
    introspection: NodeIntrospection,
    operation: Operation,
    calls: Arc<AtomicUsize>,
}

impl CacheNode {
    fn new(node_type: &str, policy: CachePolicy, state: &[u8], operation: Operation) -> Self {
        Self {
            introspection: NodeIntrospection::new(
                NodeSchemaId::new(
                    NodeTypeId::from_str(node_type).unwrap(),
                    SemanticVersion::from_str("1.0.0").unwrap(),
                ),
                NodeBehavior::new(
                    TimeBehavior::CurrentFrame,
                    RoiBehavior::InputBounds,
                    ColorRequirements::NotApplicable,
                    Determinism::Deterministic,
                    policy,
                ),
                NodeStateFingerprint::from_canonical_bytes(state),
            ),
            operation,
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl IntrospectNode for CacheNode {
    fn introspection(&self) -> NodeIntrospection {
        self.introspection.clone()
    }
}

impl EvaluateNode<i64> for CacheNode {
    fn evaluate(&self, context: &EvaluationContext<'_, i64>) -> Result<i64> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(match self.operation {
            Operation::Constant(value) => value,
            Operation::Sum => context.inputs().iter().map(|input| input.value()).sum(),
            Operation::Fail => {
                return Err(Error::new(
                    ErrorCategory::Unavailable,
                    Recoverability::Retryable,
                    "cache probe failed",
                ));
            }
        })
    }
}

#[derive(Default)]
struct TestCache<V> {
    entries: Mutex<BTreeMap<(EvaluationCacheEntryKind, EvaluationCacheKey), V>>,
}

impl<V: Clone> EvaluationValueCache<V> for TestCache<V> {
    fn get(&self, kind: EvaluationCacheEntryKind, identity: EvaluationCacheIdentity) -> Option<V> {
        self.entries
            .lock()
            .unwrap()
            .get(&(kind, identity.graph_key()))
            .cloned()
    }

    fn insert(&self, kind: EvaluationCacheEntryKind, identity: EvaluationCacheIdentity, value: V) {
        self.entries
            .lock()
            .unwrap()
            .insert((kind, identity.graph_key()), value);
    }
}

impl<V> TestCache<V> {
    fn len(&self, kind: EvaluationCacheEntryKind) -> usize {
        self.entries
            .lock()
            .unwrap()
            .keys()
            .filter(|(entry_kind, _)| *entry_kind == kind)
            .count()
    }
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

fn frame() -> RationalTime {
    RationalTime::new(12, Timebase::integer(24).unwrap())
}

fn region(min_x: i32, max_x: i32) -> PixelBounds {
    PixelBounds::new(min_x, 0, max_x, 32).unwrap()
}

fn request(region: PixelBounds) -> EvaluationRequest {
    EvaluationRequest::new(endpoint(3, 301), frame(), region)
}

fn cacheable_graph(source_value: i64) -> DirectedAcyclicGraph<CacheNode> {
    let source = CacheNode::new(
        "superi.test.cache-source",
        CachePolicy::Static,
        &source_value.to_be_bytes(),
        Operation::Constant(source_value),
    );
    let temporal = CacheNode::new(
        "superi.test.cache-temporal",
        CachePolicy::PerFrame,
        b"gain=1",
        Operation::Sum,
    );
    let output = CacheNode::new(
        "superi.test.cache-output",
        CachePolicy::PerRegion,
        b"output=1",
        Operation::Sum,
    );
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(81));
    graph.insert_node(NodeId::from_raw(1), source).unwrap();
    graph.insert_node(NodeId::from_raw(2), temporal).unwrap();
    graph.insert_node(NodeId::from_raw(3), output).unwrap();
    graph.insert_edge(edge(10, (1, 101), (2, 201))).unwrap();
    graph.insert_edge(edge(20, (2, 202), (3, 302))).unwrap();
    graph
}

#[test]
fn final_hits_and_intermediate_hits_skip_their_complete_dependency_work() {
    let graph = cacheable_graph(7);
    let cache = TestCache::default();
    let first_request = request(region(0, 64));
    let second_region = request(region(16, 48));

    let uncached = LazyEvaluator::evaluate(&graph, first_request).unwrap();
    let first = LazyEvaluator::evaluate_with_cache(&graph, first_request, &cache).unwrap();
    let final_hit = LazyEvaluator::evaluate_with_cache(&graph, first_request, &cache).unwrap();
    let intermediate_hit =
        LazyEvaluator::evaluate_with_cache(&graph, second_region, &cache).unwrap();

    assert_eq!(first.value(), uncached.value());
    assert_eq!(*final_hit.value(), 7);
    assert_eq!(*intermediate_hit.value(), 7);
    assert_eq!(
        final_hit.evaluated_keys().collect::<Vec<_>>(),
        [first_request.key()]
    );
    assert_eq!(
        intermediate_hit.evaluated_keys().collect::<Vec<_>>(),
        [
            EvaluationKey::new(endpoint(2, 202), frame(), second_region.region()),
            second_region.key(),
        ]
    );
    assert_eq!(graph.node(NodeId::from_raw(1)).unwrap().calls(), 2);
    assert_eq!(graph.node(NodeId::from_raw(2)).unwrap().calls(), 2);
    assert_eq!(graph.node(NodeId::from_raw(3)).unwrap().calls(), 3);
    assert_eq!(cache.len(EvaluationCacheEntryKind::FinalFrame), 2);
    assert_eq!(cache.len(EvaluationCacheEntryKind::IntermediateNode), 2);
}

#[test]
fn disabled_nodes_bypass_memory_cache_on_every_pull() {
    let node = CacheNode::new(
        "superi.test.uncached",
        CachePolicy::Disabled,
        b"value=11",
        Operation::Constant(11),
    );
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(82));
    graph.insert_node(NodeId::from_raw(1), node).unwrap();
    let request = EvaluationRequest::new(endpoint(1, 101), frame(), region(0, 8));
    let cache = TestCache::default();

    assert_eq!(
        *LazyEvaluator::evaluate_with_cache(&graph, request, &cache)
            .unwrap()
            .value(),
        11
    );
    assert_eq!(
        *LazyEvaluator::evaluate_with_cache(&graph, request, &cache)
            .unwrap()
            .value(),
        11
    );
    assert_eq!(graph.node(NodeId::from_raw(1)).unwrap().calls(), 2);
    assert_eq!(cache.len(EvaluationCacheEntryKind::FinalFrame), 0);
    assert_eq!(cache.len(EvaluationCacheEntryKind::IntermediateNode), 0);
}

#[test]
fn failed_outputs_are_never_retained_as_successful_frames() {
    let node = CacheNode::new(
        "superi.test.cache-failure",
        CachePolicy::PerFrame,
        b"failure=1",
        Operation::Fail,
    );
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(83));
    graph.insert_node(NodeId::from_raw(1), node).unwrap();
    let request = EvaluationRequest::new(endpoint(1, 101), frame(), region(0, 8));
    let cache = TestCache::default();

    for _ in 0..2 {
        let error =
            LazyEvaluator::evaluate_with_cache::<_, i64, _>(&graph, request, &cache).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Unavailable);
        assert_eq!(error.recoverability(), Recoverability::Retryable);
        assert_eq!(error.contexts()[0].operation(), "evaluate_node");
    }
    assert_eq!(graph.node(NodeId::from_raw(1)).unwrap().calls(), 2);
    assert_eq!(cache.len(EvaluationCacheEntryKind::FinalFrame), 0);
}
