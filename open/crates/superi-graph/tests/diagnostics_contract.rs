use std::str::FromStr;
use std::thread;
use std::time::Duration;

use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use superi_core::geometry::PixelBounds;
use superi_core::settings::SemanticVersion;
use superi_core::time::{RationalTime, Timebase};
use superi_graph::dag::{DirectedAcyclicGraph, GraphEdge, GraphEndpoint};
use superi_graph::diagnostics::{
    CacheKeyStatus, IntrospectNode, NodeIntrospection, NodeStateFingerprint,
};
use superi_graph::eval::{
    EvaluateNode, EvaluationContext, EvaluationKey, EvaluationRequest, LazyEvaluator,
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
struct ObservedNode {
    introspection: NodeIntrospection,
    operation: Operation,
    delay: Duration,
}

impl ObservedNode {
    fn new(
        node_type: &str,
        policy: CachePolicy,
        determinism: Determinism,
        canonical_state: &[u8],
        operation: Operation,
    ) -> Self {
        let time = if policy == CachePolicy::Static {
            TimeBehavior::Invariant
        } else {
            TimeBehavior::CurrentFrame
        };
        Self {
            introspection: NodeIntrospection::new(
                NodeSchemaId::new(
                    NodeTypeId::from_str(node_type).unwrap(),
                    SemanticVersion::from_str("1.0.0").unwrap(),
                ),
                NodeBehavior::new(
                    time,
                    RoiBehavior::InputBounds,
                    ColorRequirements::NotApplicable,
                    determinism,
                    policy,
                ),
                NodeStateFingerprint::from_canonical_bytes(canonical_state),
            ),
            operation,
            delay: Duration::ZERO,
        }
    }

    fn with_delay(mut self, delay: Duration) -> Self {
        self.delay = delay;
        self
    }
}

impl IntrospectNode for ObservedNode {
    fn introspection(&self) -> NodeIntrospection {
        self.introspection.clone()
    }
}

impl EvaluateNode<i64> for ObservedNode {
    fn evaluate(&self, context: &EvaluationContext<'_, i64>) -> Result<i64> {
        if !self.delay.is_zero() {
            thread::sleep(self.delay);
        }
        match self.operation {
            Operation::Constant(value) => Ok(value),
            Operation::Sum => Ok(context.inputs().iter().map(|input| input.value()).sum()),
            Operation::Fail => Err(Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "observed node failed",
            )),
        }
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

fn region(min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> PixelBounds {
    PixelBounds::new(min_x, min_y, max_x, max_y).unwrap()
}

fn frame(value: i64, units_per_second: u32) -> RationalTime {
    RationalTime::new(value, Timebase::integer(units_per_second).unwrap())
}

fn request(frame: RationalTime, region: PixelBounds) -> EvaluationRequest {
    EvaluationRequest::new(endpoint(3, 302), frame, region)
}

fn cacheable_graph(
    source_state: &[u8],
    node_order: &[u128],
    edge_order: &[u128],
    include_unreachable: bool,
    delay: Duration,
) -> DirectedAcyclicGraph<ObservedNode> {
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(42));
    let source_value = std::str::from_utf8(source_state)
        .unwrap()
        .strip_prefix("source=")
        .unwrap()
        .parse()
        .unwrap();
    for id in node_order {
        let node = match id {
            1 => ObservedNode::new(
                "superi.test.source",
                CachePolicy::Static,
                Determinism::Deterministic,
                source_state,
                Operation::Constant(source_value),
            ),
            2 => ObservedNode::new(
                "superi.test.frame",
                CachePolicy::PerFrame,
                Determinism::Deterministic,
                b"gain=2",
                Operation::Sum,
            ),
            3 => ObservedNode::new(
                "superi.test.region",
                CachePolicy::PerRegion,
                Determinism::Seeded,
                b"seed=99",
                Operation::Sum,
            ),
            unexpected => panic!("unexpected node {unexpected}"),
        }
        .with_delay(delay);
        graph.insert_node(NodeId::from_raw(*id), node).unwrap();
    }
    for id in edge_order {
        let route = match id {
            10 => edge(10, (1, 101), (2, 201)),
            20 => edge(20, (2, 202), (3, 301)),
            unexpected => panic!("unexpected edge {unexpected}"),
        };
        graph.insert_edge(route).unwrap();
    }
    if include_unreachable {
        graph
            .insert_node(
                NodeId::from_raw(99),
                ObservedNode::new(
                    "superi.test.unreachable",
                    CachePolicy::Disabled,
                    Determinism::NonDeterministic,
                    b"unused",
                    Operation::Fail,
                ),
            )
            .unwrap();
    }
    graph
}

fn cache_key(
    inspection: &superi_graph::diagnostics::EvaluationInspection,
    key: EvaluationKey,
) -> superi_graph::diagnostics::EvaluationCacheKey {
    inspection.node(key).unwrap().cache_status().key().unwrap()
}

#[test]
fn inspection_is_deterministic_and_cache_keys_follow_declared_scope() {
    let first = cacheable_graph(b"source=7", &[3, 1, 2], &[20, 10], false, Duration::ZERO);
    let second = cacheable_graph(b"source=7", &[1, 2, 3], &[10, 20], true, Duration::ZERO);
    let region_a = region(0, 0, 64, 32);
    let region_b = region(8, 4, 48, 24);
    let one_second_24 = frame(24, 24);
    let one_second_48 = frame(48, 48);
    let two_seconds = frame(48, 24);

    let first_inspection =
        LazyEvaluator::inspect::<_, i64>(&first, request(one_second_24, region_a)).unwrap();
    let reordered =
        LazyEvaluator::inspect::<_, i64>(&second, request(one_second_48, region_a)).unwrap();
    assert_eq!(first_inspection, reordered);
    assert_eq!(first_inspection.nodes().len(), 3);

    let other_region =
        LazyEvaluator::inspect::<_, i64>(&first, request(one_second_24, region_b)).unwrap();
    let other_time =
        LazyEvaluator::inspect::<_, i64>(&first, request(two_seconds, region_a)).unwrap();
    let source_a = EvaluationKey::new(endpoint(1, 101), one_second_24, region_a);
    let source_b = EvaluationKey::new(endpoint(1, 101), one_second_24, region_b);
    let frame_a = EvaluationKey::new(endpoint(2, 202), one_second_24, region_a);
    let frame_b = EvaluationKey::new(endpoint(2, 202), one_second_24, region_b);
    let region_key_a = request(one_second_24, region_a).key();
    let region_key_b = request(one_second_24, region_b).key();

    assert_eq!(
        cache_key(&first_inspection, source_a),
        cache_key(&other_region, source_b)
    );
    assert_eq!(
        cache_key(&first_inspection, frame_a),
        cache_key(&other_region, frame_b)
    );
    assert_ne!(
        cache_key(&first_inspection, region_key_a),
        cache_key(&other_region, region_key_b)
    );
    assert_eq!(
        cache_key(&first_inspection, source_a),
        cache_key(
            &other_time,
            EvaluationKey::new(endpoint(1, 101), two_seconds, region_a)
        )
    );
    assert_ne!(
        cache_key(&first_inspection, frame_a),
        cache_key(
            &other_time,
            EvaluationKey::new(endpoint(2, 202), two_seconds, region_a)
        )
    );
}

#[test]
fn state_edits_invalidate_only_reached_cache_lineage() {
    let before = cacheable_graph(b"source=7", &[1, 2, 3], &[10, 20], false, Duration::ZERO);
    let after = cacheable_graph(b"source=8", &[1, 2, 3], &[10, 20], true, Duration::ZERO);
    let frame = frame(12, 24);
    let region = region(0, 0, 1920, 1080);
    assert_eq!(
        *LazyEvaluator::evaluate(&before, request(frame, region))
            .unwrap()
            .value(),
        7
    );
    assert_eq!(
        *LazyEvaluator::evaluate(&after, request(frame, region))
            .unwrap()
            .value(),
        8
    );
    let before = LazyEvaluator::inspect::<_, i64>(&before, request(frame, region)).unwrap();
    let after = LazyEvaluator::inspect::<_, i64>(&after, request(frame, region)).unwrap();

    for key in [
        EvaluationKey::new(endpoint(1, 101), frame, region),
        EvaluationKey::new(endpoint(2, 202), frame, region),
        request(frame, region).key(),
    ] {
        assert_ne!(cache_key(&before, key), cache_key(&after, key));
    }
    assert_eq!(
        before
            .node(EvaluationKey::new(endpoint(2, 202), frame, region))
            .unwrap()
            .introspection()
            .state_fingerprint(),
        after
            .node(EvaluationKey::new(endpoint(2, 202), frame, region))
            .unwrap()
            .introspection()
            .state_fingerprint()
    );
}

fn blocked_graph(
    policy: CachePolicy,
    determinism: Determinism,
) -> DirectedAcyclicGraph<ObservedNode> {
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(43));
    graph
        .insert_node(
            NodeId::from_raw(1),
            ObservedNode::new(
                "superi.test.blocked-source",
                policy,
                determinism,
                b"source",
                Operation::Constant(3),
            ),
        )
        .unwrap();
    graph
        .insert_node(
            NodeId::from_raw(2),
            ObservedNode::new(
                "superi.test.blocked-output",
                CachePolicy::PerRegion,
                Determinism::Deterministic,
                b"output",
                Operation::Sum,
            ),
        )
        .unwrap();
    graph.insert_edge(edge(10, (1, 101), (2, 201))).unwrap();
    graph
}

#[test]
fn non_cacheable_nodes_publish_actionable_downstream_decisions() {
    let frame = frame(0, 24);
    let region = region(0, 0, 16, 16);
    let request = EvaluationRequest::new(endpoint(2, 202), frame, region);

    let disabled = LazyEvaluator::inspect::<_, i64>(
        &blocked_graph(CachePolicy::Disabled, Determinism::Deterministic),
        request,
    )
    .unwrap();
    assert_eq!(
        disabled
            .node(EvaluationKey::new(endpoint(1, 101), frame, region))
            .unwrap()
            .cache_status(),
        &CacheKeyStatus::DisabledByPolicy
    );
    assert_eq!(
        disabled.node(request.key()).unwrap().cache_status(),
        &CacheKeyStatus::BlockedByDependency {
            edge_id: EdgeId::from_raw(10)
        }
    );

    let nondeterministic = LazyEvaluator::inspect::<_, i64>(
        &blocked_graph(CachePolicy::PerFrame, Determinism::NonDeterministic),
        request,
    )
    .unwrap();
    assert_eq!(
        nondeterministic
            .node(EvaluationKey::new(endpoint(1, 101), frame, region))
            .unwrap()
            .cache_status(),
        &CacheKeyStatus::NonDeterministic
    );
    assert_eq!(
        nondeterministic
            .node(request.key())
            .unwrap()
            .cache_status()
            .blocking_edge(),
        Some(EdgeId::from_raw(10))
    );
}

#[test]
fn diagnostic_evaluation_preserves_results_and_separates_run_local_timing() {
    let graph = cacheable_graph(
        b"source=7",
        &[3, 2, 1],
        &[20, 10],
        false,
        Duration::from_millis(1),
    );
    let request = request(frame(3, 24), region(0, 0, 32, 16));
    let plain = LazyEvaluator::evaluate(&graph, request).unwrap();
    let reports = ["editor", "script", "headless"]
        .map(|_| LazyEvaluator::evaluate_with_diagnostics(&graph, request).unwrap());

    for report in &reports {
        assert_eq!(report.result(), &plain);
        assert_eq!(
            report.diagnostics().inspection().schedule(),
            report.result().schedule()
        );
        assert_eq!(report.diagnostics().node_timings().len(), 3);
        assert!(report
            .diagnostics()
            .node_timings()
            .iter()
            .all(|timing| timing.elapsed() >= Duration::from_millis(1)));
        let node_total = report
            .diagnostics()
            .node_timings()
            .iter()
            .fold(Duration::ZERO, |total, timing| total + timing.elapsed());
        assert!(report.diagnostics().execution_elapsed() >= node_total);
    }
    assert_eq!(reports[0].result(), reports[1].result());
    assert_eq!(reports[1].result(), reports[2].result());
    assert_eq!(
        reports[0].diagnostics().inspection(),
        reports[1].diagnostics().inspection()
    );
    assert_eq!(
        reports[1].diagnostics().inspection(),
        reports[2].diagnostics().inspection()
    );
}

#[test]
fn diagnostic_failures_keep_classification_and_add_node_context() {
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(44));
    graph
        .insert_node(
            NodeId::from_raw(1),
            ObservedNode::new(
                "superi.test.failure",
                CachePolicy::PerFrame,
                Determinism::Deterministic,
                b"failure=1",
                Operation::Fail,
            )
            .with_delay(Duration::from_millis(1)),
        )
        .unwrap();
    let request = EvaluationRequest::new(endpoint(1, 101), frame(7, 24), region(-8, -4, 24, 20));

    let error = LazyEvaluator::evaluate_with_diagnostics::<_, i64>(&graph, request).unwrap_err();

    assert_eq!(error.category(), ErrorCategory::Unavailable);
    assert_eq!(error.recoverability(), Recoverability::Retryable);
    assert_eq!(error.contexts()[0].operation(), "evaluate_node");
    assert_eq!(error.contexts()[1].operation(), "diagnose_node");
    assert_eq!(
        error.contexts()[1].field("schema_id"),
        Some("superi.test.failure@1.0.0")
    );
    assert_eq!(error.contexts()[1].field("cache_status"), Some("available"));
    assert!(error.contexts()[1]
        .field("cache_key")
        .unwrap()
        .starts_with("sha256:"));
    assert!(
        error.contexts()[1]
            .field("elapsed_ns")
            .unwrap()
            .parse::<u128>()
            .unwrap()
            > 0
    );
}

#[test]
fn diagnostic_public_values_are_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<NodeStateFingerprint>();
    assert_send_sync::<NodeIntrospection>();
    assert_send_sync::<superi_graph::diagnostics::EvaluationCacheKey>();
    assert_send_sync::<CacheKeyStatus>();
    assert_send_sync::<superi_graph::diagnostics::EvaluationInspection>();
    assert_send_sync::<superi_graph::diagnostics::EvaluationDiagnostics>();
    assert_send_sync::<superi_graph::diagnostics::EvaluationReport<i64>>();
}
