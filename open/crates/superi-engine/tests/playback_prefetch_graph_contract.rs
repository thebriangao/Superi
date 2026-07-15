use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

use superi_cache::eviction::{CacheBudgetLimit, CacheBudgetManager, CacheCost, CacheMemoryBudgets};
use superi_cache::frame::{FrameMemoryCache, OwnedHostFrameMemoryCache};
use superi_cache::key::{ParameterStateFingerprint, RenderSettingsFingerprint};
use superi_cache::prefetch::{PlaybackPrefetchConfig, PlaybackPrefetchInput};
use superi_concurrency::jobs::{BoundedWorkerPool, JobTerminalState, WorkerPoolConfig};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::color_space::ColorSpace;
use superi_core::error::Result;
use superi_core::geometry::PixelBounds;
use superi_core::ids::{JobId, ProjectId};
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_core::time::{Duration, RationalTime, TimeRange, Timebase};
use superi_engine::playback::{GraphPlaybackPrefetchEvaluator, PlaybackPrefetcher};
use superi_graph::dag::GraphEndpoint;
use superi_graph::diagnostics::{IntrospectNode, NodeIntrospection, NodeStateFingerprint};
use superi_graph::eval::{
    EvaluateNode, EvaluationCacheEntryKind, EvaluationContext, EvaluationRequest,
};
use superi_graph::expr::ExpressionParameterValue;
use superi_graph::headless::{GraphEvaluationSnapshot, NodeCompiler};
use superi_graph::ids::{GraphId, NodeId, PortId};
use superi_graph::mutate::{
    EditableGraph, EditableNode, GraphMutation, GraphSnapshot, GraphTransaction, InstancePort,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchema, NodeSchemaId,
    NodeTypeId, PortCardinality, PortName, PortSchema, RoiBehavior, TimeBehavior, ValueTypeId,
};
use superi_image::metadata::{ColorPipelineMetadata, ImageColorTags};

#[derive(Clone, Debug, Eq, PartialEq)]
struct Scalar(i64);

impl ExpressionParameterValue for Scalar {
    fn to_expression_scalar(&self, _value_type: &ValueTypeId) -> Result<f64> {
        Ok(self.0 as f64)
    }

    fn from_expression_scalar(_value_type: &ValueTypeId, value: f64) -> Result<Self> {
        Ok(Self(value as i64))
    }
}

#[derive(Clone)]
struct FrameValueNode {
    calls: Arc<AtomicUsize>,
}

impl EvaluateNode<i64> for FrameValueNode {
    fn evaluate(&self, context: &EvaluationContext<'_, i64>) -> Result<i64> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(context.request().frame().value())
    }
}

impl IntrospectNode for FrameValueNode {
    fn introspection(&self) -> NodeIntrospection {
        NodeIntrospection::new(
            NodeSchemaId::new(
                NodeTypeId::new("superi.test.playback-prefetch-frame").unwrap(),
                SemanticVersion::from_str("1.0.0").unwrap(),
            ),
            behavior(),
            NodeStateFingerprint::from_canonical_bytes(b"playback-prefetch-frame-v1"),
        )
    }
}

struct FrameNodeCompiler {
    calls: Arc<AtomicUsize>,
}

impl NodeCompiler<Scalar, FrameValueNode> for FrameNodeCompiler {
    fn compile_node(
        &mut self,
        _snapshot: &GraphSnapshot<Scalar>,
        _node_id: NodeId,
        _node: &EditableNode<Scalar>,
    ) -> Result<FrameValueNode> {
        Ok(FrameValueNode {
            calls: self.calls.clone(),
        })
    }
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

fn evaluation_snapshot(
    calls: Arc<AtomicUsize>,
) -> Arc<GraphEvaluationSnapshot<Scalar, FrameValueNode>> {
    let value_type = ValueTypeId::new("superi.value.playback-prefetch-frame").unwrap();
    let port_name = PortName::new("frame").unwrap();
    let schema = Arc::new(
        NodeSchema::new(
            NodeSchemaId::new(
                NodeTypeId::new("superi.test.playback-prefetch-frame").unwrap(),
                SemanticVersion::from_str("1.0.0").unwrap(),
            ),
            [],
            [PortSchema::new(
                port_name.clone(),
                value_type,
                PortCardinality::Single,
            )],
            [],
            behavior(),
            CapabilitySet::new([]),
        )
        .unwrap(),
    );
    let node = EditableNode::new(
        schema,
        [],
        [InstancePort::new(PortId::from_raw(101), port_name)],
        [],
    )
    .unwrap();
    let mut graph = EditableGraph::new(GraphId::from_raw(901));
    graph
        .apply(GraphTransaction::with_mutations(
            0,
            [GraphMutation::Add {
                node_id: NodeId::from_raw(1),
                node,
                position: 0,
            }],
        ))
        .unwrap();
    Arc::new(
        GraphEvaluationSnapshot::compile(graph.snapshot(), FrameNodeCompiler { calls }).unwrap(),
    )
}

fn memory_cache() -> Arc<FrameMemoryCache<i64>> {
    let limit = CacheBudgetLimit::new(1_024, 128).unwrap();
    let budgets = CacheMemoryBudgets::new(limit, limit, limit).unwrap();
    Arc::new(FrameMemoryCache::new(
        CacheBudgetManager::new(budgets),
        |_kind: EvaluationCacheEntryKind, _value: &i64| CacheCost::new(8, 1).unwrap(),
    ))
}

fn poll_one(
    prefetcher: &mut PlaybackPrefetcher,
) -> superi_engine::playback::PlaybackPrefetchCompletion {
    for _ in 0..50_000 {
        if let Some(completion) = prefetcher.poll().unwrap().into_iter().next() {
            return completion;
        }
        thread::yield_now();
    }
    panic!("graph prefetch did not complete within the bounded poll loop");
}

#[test]
fn shared_graph_prefetch_reuses_exact_frames_without_changing_evaluator_output() {
    let calls = Arc::new(AtomicUsize::new(0));
    let snapshot = evaluation_snapshot(calls.clone());
    let retained = memory_cache();
    let color = ColorPipelineMetadata::new(ImageColorTags::new(ColorSpace::UNSPECIFIED)).unwrap();
    let cache = Arc::new(OwnedHostFrameMemoryCache::new(
        retained.clone(),
        ProjectId::from_raw(91),
        Vec::new(),
        ParameterStateFingerprint::from_canonical_bytes(b"playback-prefetch-parameters-v1"),
        color,
        RenderSettingsFingerprint::from_canonical_bytes(b"playback-prefetch-render-v1"),
    ));
    let output = GraphEndpoint::new(NodeId::from_raw(1), PortId::from_raw(101));
    let region = PixelBounds::new(0, 0, 64, 64).unwrap();
    let evaluator = Arc::new(GraphPlaybackPrefetchEvaluator::new(
        snapshot.clone(),
        cache.clone(),
        output,
        region,
    ));
    let pool = Arc::new(BoundedWorkerPool::new(WorkerPoolConfig::new(1, 8).unwrap()).unwrap());
    let mut prefetcher = PlaybackPrefetcher::new(&pool, evaluator);
    let timebase = Timebase::integer(24).unwrap();
    let plan = PlaybackPrefetchConfig::new(2, 1, 0).unwrap().plan(
        PlaybackPrefetchInput::new(
            RationalTime::new(2, timebase),
            TimeRange::new(
                RationalTime::new(0, timebase),
                Duration::new(20, timebase).unwrap(),
            )
            .unwrap(),
            1,
        )
        .unwrap(),
    );
    let _playback = ExecutionDomain::Playback.enter_current().unwrap();

    prefetcher
        .submit(JobId::from_raw(100), plan.clone())
        .unwrap();
    assert_eq!(
        poll_one(&mut prefetcher).status(),
        JobTerminalState::Succeeded
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(retained.final_frame_len(), 3);

    prefetcher.submit(JobId::from_raw(101), plan).unwrap();
    assert_eq!(
        poll_one(&mut prefetcher).status(),
        JobTerminalState::Succeeded
    );
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    let foreground = snapshot
        .evaluate_with_cache(
            EvaluationRequest::new(output, RationalTime::new(3, timebase), region),
            cache.as_ref(),
        )
        .unwrap();
    assert_eq!(*foreground.value(), 3);
    assert_eq!(calls.load(Ordering::SeqCst), 3);

    drop(prefetcher);
    drop(_playback);
    Arc::try_unwrap(pool).ok().unwrap().shutdown().unwrap();
}
