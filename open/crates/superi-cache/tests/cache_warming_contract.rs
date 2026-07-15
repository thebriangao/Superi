use std::str::FromStr;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use superi_cache::eviction::{CacheBudgetLimit, CacheBudgetManager, CacheCost, CacheMemoryBudgets};
use superi_cache::frame::{CacheMemoryPlacement, FrameMemoryCache, FrameMemoryCacheContext};
use superi_cache::key::{MediaCacheIdentity, ParameterStateFingerprint, RenderSettingsFingerprint};
use superi_cache::warming::{CacheWarmBounds, CacheWarmPlanner, CacheWarmPolicy, CacheWarmReason};
use superi_core::color_space::ColorSpace;
use superi_core::error::Result;
use superi_core::geometry::PixelBounds;
use superi_core::ids::{MediaId, ProjectId};
use superi_core::settings::SemanticVersion;
use superi_core::time::{RationalTime, Timebase};
use superi_graph::dag::{DirectedAcyclicGraph, GraphEndpoint};
use superi_graph::diagnostics::{IntrospectNode, NodeIntrospection, NodeStateFingerprint};
use superi_graph::eval::{
    EvaluateNode, EvaluationCacheEntryKind, EvaluationContext, EvaluationRequest, LazyEvaluator,
};
use superi_graph::ids::{GraphId, NodeId, PortId};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchemaId, NodeTypeId,
    RoiBehavior, TimeBehavior,
};
use superi_image::metadata::{ColorPipelineMetadata, ImageColorTags};

#[derive(Clone)]
struct FrameValueNode {
    calls: Arc<AtomicUsize>,
}

impl EvaluateNode<i64> for FrameValueNode {
    fn evaluate(&self, context: &EvaluationContext<'_, i64>) -> Result<i64> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(100 + context.request().frame().value())
    }
}

impl IntrospectNode for FrameValueNode {
    fn introspection(&self) -> NodeIntrospection {
        NodeIntrospection::new(
            NodeSchemaId::new(
                NodeTypeId::from_str("superi.test.cache-warming").unwrap(),
                SemanticVersion::from_str("1.0.0").unwrap(),
            ),
            NodeBehavior::new(
                TimeBehavior::CurrentFrame,
                RoiBehavior::InputBounds,
                ColorRequirements::NotApplicable,
                Determinism::Deterministic,
                CachePolicy::PerFrame,
            ),
            NodeStateFingerprint::from_canonical_bytes(b"cache-warming-frame-value-v1"),
        )
    }
}

fn policy(
    edit_radius: u32,
    scrub_ahead: u32,
    scrub_behind: u32,
    maximum_scrub_stride: u32,
    maximum_targets: usize,
) -> CacheWarmPolicy {
    CacheWarmPolicy::new(
        edit_radius,
        scrub_ahead,
        scrub_behind,
        maximum_scrub_stride,
        maximum_targets,
    )
    .unwrap()
}

fn frames(plan: &superi_cache::warming::CacheWarmPlan) -> Vec<i64> {
    plan.targets().iter().map(|target| target.frame()).collect()
}

fn request(frame: i64) -> EvaluationRequest {
    EvaluationRequest::new(
        GraphEndpoint::new(NodeId::from_raw(1), PortId::from_raw(101)),
        RationalTime::new(frame, Timebase::integer(24).unwrap()),
        PixelBounds::new(0, 0, 1920, 1080).unwrap(),
    )
}

fn color_pipeline() -> ColorPipelineMetadata {
    ColorPipelineMetadata::new(ImageColorTags::new(ColorSpace::UNSPECIFIED)).unwrap()
}

fn value_cost(_kind: EvaluationCacheEntryKind, _value: &i64) -> CacheCost {
    CacheCost::new(8, 1).unwrap()
}

fn cache() -> FrameMemoryCache<i64> {
    let limit = CacheBudgetLimit::new(24, 3).unwrap();
    FrameMemoryCache::new(
        CacheBudgetManager::new(CacheMemoryBudgets::new(limit, limit, limit).unwrap()),
        value_cost,
    )
}

fn context<'a>(
    media: &'a [MediaCacheIdentity],
    color: &'a ColorPipelineMetadata,
) -> FrameMemoryCacheContext<'a> {
    FrameMemoryCacheContext::new(
        ProjectId::from_raw(1),
        CacheMemoryPlacement::Host,
        media,
        ParameterStateFingerprint::from_canonical_bytes(b"cache-warming-parameters-v1"),
        color,
        RenderSettingsFingerprint::from_canonical_bytes(b"cache-warming-render-v1"),
    )
}

#[test]
fn policy_and_bounds_reject_unbounded_or_ambiguous_input() {
    assert!(CacheWarmPolicy::new(2, 3, 1, 0, 8).is_err());
    assert!(CacheWarmPolicy::new(2, 3, 1, 4, 0).is_err());
    assert!(CacheWarmBounds::new(10, 10).is_err());
    assert!(CacheWarmBounds::new(11, 10).is_err());

    let planner = CacheWarmPlanner::new(policy(2, 3, 1, 4, 8));
    let bounds = CacheWarmBounds::new(0, 10).unwrap();
    assert!(planner.plan_edit(bounds, [-1]).is_err());
    assert!(planner.plan_edit(bounds, [11]).is_err());
    assert!(planner.plan_scrub(bounds, -1, 2).is_err());
    assert!(planner.plan_scrub(bounds, 2, 10).is_err());
}

#[test]
fn edit_warming_is_canonical_nearest_first_and_hard_bounded() {
    let planner = CacheWarmPlanner::new(policy(2, 3, 2, 3, 8));
    let bounds = CacheWarmBounds::new(0, 10).unwrap();
    let plan = planner.plan_edit(bounds, [7, 3, 3]).unwrap();

    assert_eq!(frames(&plan), [3, 7, 2, 4, 6, 8, 1, 5]);
    assert_eq!(plan.len(), 8);
    assert!(!plan.is_empty());
    assert!(plan
        .targets()
        .iter()
        .all(|target| target.reason() == CacheWarmReason::EditBoundary));
    assert_eq!(
        plan.targets()
            .iter()
            .map(|target| target.rank())
            .collect::<Vec<_>>(),
        (0..8).collect::<Vec<_>>()
    );

    let clipped = planner.plan_edit(bounds, [10]).unwrap();
    assert_eq!(frames(&clipped), [9, 8]);
    assert!(planner.plan_edit(bounds, []).unwrap().is_empty());
}

#[test]
fn scrub_warming_follows_velocity_and_keeps_a_bounded_opposite_tail() {
    let planner = CacheWarmPlanner::new(policy(2, 3, 2, 3, 8));
    let bounds = CacheWarmBounds::new(0, 30).unwrap();

    let forward = planner.plan_scrub(bounds, 10, 14).unwrap();
    assert_eq!(frames(&forward), [14, 17, 20, 23, 11, 8]);
    assert!(forward
        .targets()
        .iter()
        .all(|target| target.reason() == CacheWarmReason::ScrubForward));

    let backward = planner.plan_scrub(bounds, 14, 10).unwrap();
    assert_eq!(frames(&backward), [10, 7, 4, 1, 13, 16]);
    assert!(backward
        .targets()
        .iter()
        .all(|target| target.reason() == CacheWarmReason::ScrubBackward));

    let stationary = planner.plan_scrub(bounds, 10, 10).unwrap();
    assert_eq!(frames(&stationary), [10, 9, 11, 8, 12, 7]);
    assert!(stationary
        .targets()
        .iter()
        .all(|target| target.reason() == CacheWarmReason::ScrubStationary));

    let clipped = planner
        .plan_scrub(CacheWarmBounds::new(0, 20).unwrap(), 16, 18)
        .unwrap();
    assert_eq!(frames(&clipped), [18, 16, 14]);
}

#[test]
fn real_graph_warming_reuses_exact_values_and_preserves_freshness_under_pressure() {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(501));
    graph
        .insert_node(
            NodeId::from_raw(1),
            FrameValueNode {
                calls: calls.clone(),
            },
        )
        .unwrap();
    let cache = cache();
    let color = color_pipeline();
    let media_v1 = [MediaCacheIdentity::new(MediaId::from_raw(7), "source-v1").unwrap()];
    let media_v2 = [MediaCacheIdentity::new(MediaId::from_raw(7), "source-v2").unwrap()];
    let planner = CacheWarmPlanner::new(policy(1, 2, 0, 1, 3));
    let plan = planner
        .plan_scrub(CacheWarmBounds::new(0, 20).unwrap(), 0, 1)
        .unwrap();
    assert_eq!(frames(&plan), [1, 2, 3]);

    {
        let scope = cache.scope(context(&media_v1, &color));
        for target in plan.targets() {
            assert_eq!(
                *LazyEvaluator::evaluate_with_cache(&graph, request(target.frame()), &scope)
                    .unwrap()
                    .value(),
                100 + target.frame()
            );
        }
        assert_eq!(calls.load(Ordering::SeqCst), 3);
        assert_eq!(cache.final_frame_len(), 3);

        assert_eq!(
            *LazyEvaluator::evaluate_with_cache(&graph, request(2), &scope)
                .unwrap()
                .value(),
            102
        );
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    {
        let fresh_scope = cache.scope(context(&media_v2, &color));
        assert_eq!(
            *LazyEvaluator::evaluate_with_cache(&graph, request(2), &fresh_scope)
                .unwrap()
                .value(),
            102
        );
        assert_eq!(calls.load(Ordering::SeqCst), 4);
        assert_eq!(cache.final_frame_len(), 3);
    }

    {
        let original_scope = cache.scope(context(&media_v1, &color));
        assert_eq!(
            *LazyEvaluator::evaluate_with_cache(&graph, request(2), &original_scope)
                .unwrap()
                .value(),
            102
        );
        assert_eq!(calls.load(Ordering::SeqCst), 4);

        assert_eq!(
            *LazyEvaluator::evaluate_with_cache(&graph, request(1), &original_scope)
                .unwrap()
                .value(),
            101
        );
        assert_eq!(calls.load(Ordering::SeqCst), 5);
        assert_eq!(cache.final_frame_len(), 3);
    }
}
