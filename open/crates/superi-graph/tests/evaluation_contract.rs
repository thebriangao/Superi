use std::collections::BTreeSet;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use superi_core::geometry::PixelBounds;
use superi_core::time::{RationalTime, Timebase};
use superi_graph::dag::{DirectedAcyclicGraph, GraphEdge, GraphEndpoint};
use superi_graph::eval::{
    EvaluateNode, EvaluationContext, EvaluationDependency, EvaluationKey, EvaluationRequest,
    LazyEvaluator,
};
use superi_graph::ids::{EdgeId, GraphId, NodeId, PortId};
use superi_graph::invalidation::{
    propagate_dependency_invalidation, DirtyRegion, InvalidationSeed,
};

#[derive(Clone)]
enum DependencyPlan {
    Default,
    Only(EdgeId),
    Declared(Vec<EvaluationDependency>),
}

#[derive(Clone, Copy)]
enum OutputPlan {
    Value(i64),
    Sum,
    Digits,
    Fail,
}

#[derive(Clone)]
struct ProbeNode {
    dependencies: DependencyPlan,
    output: OutputPlan,
    calls: Arc<Mutex<Vec<EvaluationKey>>>,
}

impl ProbeNode {
    fn new(dependencies: DependencyPlan, output: OutputPlan) -> Self {
        Self {
            dependencies,
            output,
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn calls(&self) -> Vec<EvaluationKey> {
        self.calls.lock().unwrap().clone()
    }
}

impl EvaluateNode<i64> for ProbeNode {
    fn dependencies(
        &self,
        request: EvaluationRequest,
        incoming: &[GraphEdge],
    ) -> Result<Vec<EvaluationDependency>> {
        Ok(match &self.dependencies {
            DependencyPlan::Default => incoming
                .iter()
                .map(|edge| EvaluationDependency::same_request(edge.id(), request))
                .collect(),
            DependencyPlan::Only(edge_id) => {
                vec![EvaluationDependency::same_request(*edge_id, request)]
            }
            DependencyPlan::Declared(dependencies) => dependencies.clone(),
        })
    }

    fn evaluate(&self, context: &EvaluationContext<'_, i64>) -> Result<i64> {
        self.calls.lock().unwrap().push(context.request().key());
        match self.output {
            OutputPlan::Value(value) => Ok(value),
            OutputPlan::Sum => Ok(context.inputs().iter().map(|input| *input.value()).sum()),
            OutputPlan::Digits => Ok(context
                .inputs()
                .iter()
                .fold(0, |value, input| value * 10 + *input.value())),
            OutputPlan::Fail => Err(Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "probe node failed",
            )),
        }
    }
}

#[derive(Clone)]
struct DefaultNode(ProbeNode);

impl EvaluateNode<i64> for DefaultNode {
    fn evaluate(&self, context: &EvaluationContext<'_, i64>) -> Result<i64> {
        self.0.evaluate(context)
    }
}

#[derive(Clone)]
struct EditableNode {
    value: Arc<AtomicI64>,
    calls: Arc<AtomicI64>,
}

impl EvaluateNode<i64> for EditableNode {
    fn evaluate(&self, _context: &EvaluationContext<'_, i64>) -> Result<i64> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(self.value.load(Ordering::SeqCst))
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

fn frame(value: i64, units_per_second: u32) -> RationalTime {
    RationalTime::new(value, Timebase::integer(units_per_second).unwrap())
}

fn region(min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> PixelBounds {
    PixelBounds::new(min_x, min_y, max_x, max_y).unwrap()
}

fn request(node: u128, port: u128, frame: RationalTime, region: PixelBounds) -> EvaluationRequest {
    EvaluationRequest::new(endpoint(node, port), frame, region)
}

#[test]
fn default_dependencies_pull_every_incoming_edge_at_the_same_request() {
    let source = DefaultNode(ProbeNode::new(
        DependencyPlan::Default,
        OutputPlan::Value(4),
    ));
    let output = DefaultNode(ProbeNode::new(DependencyPlan::Default, OutputPlan::Sum));
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(6));
    graph
        .insert_node(NodeId::from_raw(1), source.clone())
        .unwrap();
    graph.insert_node(NodeId::from_raw(2), output).unwrap();
    graph.insert_edge(edge(10, (1, 101), (2, 201))).unwrap();
    let frame = frame(9, 24);
    let region = region(-2, 4, 30, 28);

    let result = LazyEvaluator::evaluate(&graph, request(2, 202, frame, region)).unwrap();

    assert_eq!(*result.value(), 4);
    assert_eq!(
        source.0.calls(),
        [EvaluationKey::new(endpoint(1, 101), frame, region)]
    );
}

#[test]
fn separate_pulls_observe_current_editable_state_without_stale_reuse() {
    let value = Arc::new(AtomicI64::new(11));
    let calls = Arc::new(AtomicI64::new(0));
    let node = EditableNode {
        value: value.clone(),
        calls: calls.clone(),
    };
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(7));
    graph.insert_node(NodeId::from_raw(1), node).unwrap();
    let request = request(1, 101, frame(0, 24), region(0, 0, 8, 8));

    let before = LazyEvaluator::evaluate(&graph, request).unwrap();
    assert_eq!(*before.value(), 11);

    value.store(29, Ordering::SeqCst);
    let after = LazyEvaluator::evaluate(&graph, request).unwrap();

    assert_eq!(*after.value(), 29);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
}

#[test]
fn evaluation_resolves_only_node_declared_work() {
    let selected = ProbeNode::new(DependencyPlan::Default, OutputPlan::Value(7));
    let unused_failure = ProbeNode::new(DependencyPlan::Default, OutputPlan::Fail);
    let selector = ProbeNode::new(DependencyPlan::Only(EdgeId::from_raw(10)), OutputPlan::Sum);
    let output = ProbeNode::new(DependencyPlan::Default, OutputPlan::Sum);

    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(1));
    for (id, node) in [
        (1, selected.clone()),
        (2, unused_failure.clone()),
        (3, selector.clone()),
        (4, output.clone()),
    ] {
        graph.insert_node(NodeId::from_raw(id), node).unwrap();
    }
    for route in [
        edge(20, (2, 201), (3, 302)),
        edge(10, (1, 101), (3, 301)),
        edge(30, (3, 303), (4, 401)),
    ] {
        graph.insert_edge(route).unwrap();
    }

    let request = request(4, 402, frame(12, 24), region(16, 8, 80, 48));
    let result = LazyEvaluator::evaluate(&graph, request).unwrap();

    assert_eq!(*result.value(), 7);
    assert_eq!(result.request(), request);
    assert_eq!(selected.calls().len(), 1);
    assert!(unused_failure.calls().is_empty());
    assert_eq!(selector.calls().len(), 1);
    assert_eq!(output.calls().len(), 1);
    assert_eq!(
        result
            .evaluated_keys()
            .map(|key| key.output().node_id())
            .collect::<Vec<_>>(),
        [1, 3, 4].map(NodeId::from_raw)
    );
}

#[test]
fn frame_and_region_keys_reuse_only_physically_identical_work() {
    let region_a = region(0, 0, 64, 32);
    let region_b = region(8, 4, 32, 20);
    let one_second_24 = frame(24, 24);
    let one_second_48 = frame(48, 48);
    let two_seconds = frame(48, 24);
    let input_edge = EdgeId::from_raw(10);

    let source = ProbeNode::new(DependencyPlan::Default, OutputPlan::Value(5));
    let temporal = ProbeNode::new(
        DependencyPlan::Declared(vec![
            EvaluationDependency::new(input_edge, one_second_48, region_a),
            EvaluationDependency::new(input_edge, two_seconds, region_a),
            EvaluationDependency::new(input_edge, one_second_24, region_b),
            EvaluationDependency::new(input_edge, one_second_24, region_a),
            EvaluationDependency::new(input_edge, one_second_48, region_a),
        ]),
        OutputPlan::Sum,
    );

    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(2));
    graph
        .insert_node(NodeId::from_raw(1), source.clone())
        .unwrap();
    graph
        .insert_node(NodeId::from_raw(2), temporal.clone())
        .unwrap();
    graph.insert_edge(edge(10, (1, 101), (2, 201))).unwrap();

    let result =
        LazyEvaluator::evaluate(&graph, request(2, 202, frame(0, 24), region(0, 0, 128, 64)))
            .unwrap();

    assert_eq!(*result.value(), 15);
    assert_eq!(source.calls().len(), 3);
    assert_eq!(temporal.calls().len(), 1);
    assert_eq!(result.evaluated_keys().len(), 4);
    assert!(result
        .value_for(EvaluationKey::new(
            endpoint(1, 101),
            one_second_24,
            region_a
        ))
        .is_some());
    assert!(result
        .value_for(EvaluationKey::new(
            endpoint(1, 101),
            one_second_48,
            region_a
        ))
        .is_some());
    assert_eq!(
        BTreeSet::from([
            EvaluationKey::new(endpoint(1, 101), one_second_24, region_a),
            EvaluationKey::new(endpoint(1, 101), one_second_48, region_a),
        ])
        .len(),
        1
    );
}

#[test]
fn scheduling_reuses_work_without_collapsing_distinct_input_edges() {
    let source = ProbeNode::new(DependencyPlan::Default, OutputPlan::Value(5));
    let output = ProbeNode::new(DependencyPlan::Default, OutputPlan::Sum);
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(8));
    graph
        .insert_node(NodeId::from_raw(1), source.clone())
        .unwrap();
    graph.insert_node(NodeId::from_raw(2), output).unwrap();
    graph.insert_edge(edge(10, (1, 101), (2, 201))).unwrap();
    graph.insert_edge(edge(20, (1, 101), (2, 202))).unwrap();
    let request = request(2, 203, frame(0, 24), region(0, 0, 8, 8));

    let result = LazyEvaluator::evaluate(&graph, request).unwrap();

    assert_eq!(*result.value(), 10);
    assert_eq!(source.calls().len(), 1);
    assert_eq!(result.schedule().len(), 2);
    assert_eq!(
        result
            .schedule()
            .prerequisites(request.key())
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn scheduling_preserves_exact_invalidated_requested_work() {
    let source = ProbeNode::new(DependencyPlan::Default, OutputPlan::Value(3));
    let output = ProbeNode::new(DependencyPlan::Default, OutputPlan::Sum);
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(9));
    graph.insert_node(NodeId::from_raw(1), source).unwrap();
    graph.insert_node(NodeId::from_raw(2), output).unwrap();
    graph.insert_edge(edge(10, (1, 101), (2, 201))).unwrap();
    let invalidation = propagate_dependency_invalidation(
        &graph,
        [InvalidationSeed::from_region(
            NodeId::from_raw(1),
            DirtyRegion::Bounds(region(0, 0, 16, 16)),
        )],
    )
    .unwrap();
    let requested = invalidation.requested_work(NodeId::from_raw(2), region(8, 8, 24, 24));
    assert_eq!(requested.regions(), &[region(8, 8, 16, 16)]);
    let request = request(2, 202, frame(4, 24), requested.regions()[0]);

    let result = LazyEvaluator::evaluate(&graph, request).unwrap();

    assert_eq!(*result.value(), 3);
    assert_eq!(result.schedule().len(), 2);
    assert!(result
        .evaluated_keys()
        .all(|key| key.region() == region(8, 8, 16, 16)));
    assert_eq!(
        invalidation
            .dirty_regions(NodeId::from_raw(2))
            .unwrap()
            .regions(),
        &[region(0, 0, 16, 16)]
    );
}

fn deterministic_graph(
    node_order: &[u128],
    edge_order: &[u128],
) -> DirectedAcyclicGraph<ProbeNode> {
    let declarations = vec![
        EvaluationDependency::same_request(
            EdgeId::from_raw(20),
            request(3, 301, frame(3, 24), region(0, 0, 16, 16)),
        ),
        EvaluationDependency::same_request(
            EdgeId::from_raw(10),
            request(3, 301, frame(3, 24), region(0, 0, 16, 16)),
        ),
        EvaluationDependency::same_request(
            EdgeId::from_raw(20),
            request(3, 301, frame(3, 24), region(0, 0, 16, 16)),
        ),
    ];
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(3));
    for id in node_order {
        let node = match id {
            1 => ProbeNode::new(DependencyPlan::Default, OutputPlan::Value(1)),
            2 => ProbeNode::new(DependencyPlan::Default, OutputPlan::Value(2)),
            3 => ProbeNode::new(
                DependencyPlan::Declared(declarations.clone()),
                OutputPlan::Digits,
            ),
            _ => panic!("unexpected node"),
        };
        graph.insert_node(NodeId::from_raw(*id), node).unwrap();
    }
    for id in edge_order {
        let route = match id {
            10 => edge(10, (1, 101), (3, 302)),
            20 => edge(20, (2, 201), (3, 303)),
            _ => panic!("unexpected edge"),
        };
        graph.insert_edge(route).unwrap();
    }
    graph
}

#[test]
fn dependency_order_and_results_are_insertion_independent() {
    let first = deterministic_graph(&[3, 2, 1], &[20, 10]);
    let second = deterministic_graph(&[1, 2, 3], &[10, 20]);
    let request = request(3, 301, frame(3, 24), region(0, 0, 16, 16));

    let first = LazyEvaluator::evaluate(&first, request).unwrap();
    let second = LazyEvaluator::evaluate(&second, request).unwrap();

    assert_eq!(*first.value(), 12);
    assert_eq!(first, second);
    assert_eq!(
        first
            .evaluated_keys()
            .map(|key| key.output().node_id())
            .collect::<Vec<_>>(),
        [1, 2, 3].map(NodeId::from_raw)
    );
}

#[test]
fn scheduling_publishes_stable_ready_batches() {
    let first = deterministic_graph(&[3, 2, 1], &[20, 10]);
    let second = deterministic_graph(&[1, 2, 3], &[10, 20]);
    let request = request(3, 301, frame(3, 24), region(0, 0, 16, 16));

    let first_schedule = LazyEvaluator::schedule::<_, i64>(&first, request).unwrap();
    let second_schedule = LazyEvaluator::schedule::<_, i64>(&second, request).unwrap();

    assert_eq!(first_schedule, second_schedule);
    for node_id in [1, 2, 3].map(NodeId::from_raw) {
        assert!(first.node(node_id).unwrap().calls().is_empty());
        assert!(second.node(node_id).unwrap().calls().is_empty());
    }
    assert_eq!(first_schedule.batches().len(), 2);
    assert_eq!(
        first_schedule.batches()[0]
            .keys()
            .iter()
            .map(|key| key.output().node_id())
            .collect::<Vec<_>>(),
        [1, 2].map(NodeId::from_raw)
    );
    assert_eq!(
        first_schedule.batches()[1]
            .keys()
            .iter()
            .map(|key| key.output().node_id())
            .collect::<Vec<_>>(),
        [NodeId::from_raw(3)]
    );

    let result = LazyEvaluator::evaluate(&first, request).unwrap();
    assert_eq!(result.schedule(), &first_schedule);
}

#[test]
fn every_caller_uses_the_same_graph_and_evaluator_result() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<EvaluationRequest>();
    assert_send_sync::<superi_graph::eval::EvaluationSchedule>();
    assert_send_sync::<superi_graph::eval::EvaluationResult<i64>>();

    let graph = deterministic_graph(&[3, 1, 2], &[20, 10]);
    let request = request(3, 301, frame(3, 24), region(0, 0, 16, 16));
    let observed = std::thread::scope(|scope| {
        ["editor", "script", "headless"]
            .into_iter()
            .map(|_| scope.spawn(|| LazyEvaluator::evaluate(&graph, request).unwrap()))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });

    assert_eq!(observed[0], observed[1]);
    assert_eq!(observed[1], observed[2]);
}

#[test]
fn invalid_targets_and_node_declared_routes_are_actionable() {
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(4));
    graph
        .insert_node(
            NodeId::from_raw(1),
            ProbeNode::new(DependencyPlan::Default, OutputPlan::Value(1)),
        )
        .unwrap();
    graph
        .insert_node(
            NodeId::from_raw(2),
            ProbeNode::new(
                DependencyPlan::Declared(vec![EvaluationDependency::new(
                    EdgeId::from_raw(999),
                    frame(0, 24),
                    region(0, 0, 1, 1),
                )]),
                OutputPlan::Sum,
            ),
        )
        .unwrap();
    graph
        .insert_node(
            NodeId::from_raw(3),
            ProbeNode::new(DependencyPlan::Default, OutputPlan::Sum),
        )
        .unwrap();
    graph
        .insert_node(
            NodeId::from_raw(4),
            ProbeNode::new(
                DependencyPlan::Declared(vec![EvaluationDependency::new(
                    EdgeId::from_raw(10),
                    frame(0, 24),
                    region(0, 0, 1, 1),
                )]),
                OutputPlan::Sum,
            ),
        )
        .unwrap();
    graph.insert_edge(edge(10, (1, 101), (3, 301))).unwrap();

    let missing = LazyEvaluator::evaluate::<_, i64>(
        &graph,
        request(9, 901, frame(0, 24), region(0, 0, 1, 1)),
    )
    .unwrap_err();
    assert_eq!(missing.category(), ErrorCategory::NotFound);
    assert_eq!(missing.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(missing.contexts()[0].component(), "superi-graph.eval");
    assert_eq!(missing.contexts()[0].operation(), "evaluate");
    assert_eq!(
        missing.contexts()[0].field("reason"),
        Some("missing_target_node")
    );

    let invalid = LazyEvaluator::evaluate::<_, i64>(
        &graph,
        request(2, 201, frame(0, 24), region(0, 0, 1, 1)),
    )
    .unwrap_err();
    assert_eq!(invalid.category(), ErrorCategory::Internal);
    assert_eq!(invalid.recoverability(), Recoverability::Terminal);
    assert_eq!(invalid.contexts()[0].component(), "superi-graph.eval");
    assert_eq!(
        invalid.contexts()[0].field("reason"),
        Some("missing_dependency_edge")
    );

    let not_incoming = LazyEvaluator::evaluate::<_, i64>(
        &graph,
        request(4, 401, frame(0, 24), region(0, 0, 1, 1)),
    )
    .unwrap_err();
    assert_eq!(not_incoming.category(), ErrorCategory::Internal);
    assert_eq!(not_incoming.recoverability(), Recoverability::Terminal);
    assert_eq!(
        not_incoming.contexts()[0].field("reason"),
        Some("dependency_edge_not_incoming")
    );
}

#[test]
fn node_failures_retain_classification_and_add_request_context() {
    let failure = ProbeNode::new(DependencyPlan::Default, OutputPlan::Fail);
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(5));
    graph.insert_node(NodeId::from_raw(1), failure).unwrap();

    let error = LazyEvaluator::evaluate::<_, i64>(
        &graph,
        request(1, 101, frame(7, 24), region(-8, -4, 24, 20)),
    )
    .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::Unavailable);
    assert_eq!(error.recoverability(), Recoverability::Retryable);
    assert_eq!(error.contexts()[0].component(), "superi-graph.eval");
    assert_eq!(error.contexts()[0].operation(), "evaluate_node");
    assert_eq!(
        error.contexts()[0].field("node_id"),
        Some("node:00000000000000000000000000000001")
    );
    assert_eq!(error.contexts()[0].field("frame"), Some("7 @ 24 units/s"));
    assert_eq!(error.contexts()[0].field("region"), Some("[-8,-4,24,20)"));
}
