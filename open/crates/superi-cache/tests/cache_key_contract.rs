use std::str::FromStr;
use std::sync::Arc;

use superi_cache::key::{
    ColorPipelineFingerprint, FrameCacheKey, FrameCacheKeyInputs, MediaCacheIdentity,
    ParameterStateFingerprint, RenderSettingsFingerprint,
};
use superi_core::color_space::ColorSpace;
use superi_core::error::Result;
use superi_core::geometry::PixelBounds;
use superi_core::ids::MediaId;
use superi_core::settings::SemanticVersion;
use superi_core::time::{RationalTime, Timebase};
use superi_graph::dag::{DirectedAcyclicGraph, GraphEndpoint};
use superi_graph::diagnostics::{
    EvaluationCacheKey, IntrospectNode, NodeIntrospection, NodeStateFingerprint,
};
use superi_graph::eval::{EvaluateNode, EvaluationContext, EvaluationRequest, LazyEvaluator};
use superi_graph::ids::{GraphId, NodeId, PortId};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchemaId, NodeTypeId,
    RoiBehavior, TimeBehavior,
};
use superi_image::metadata::{
    ColorPipelineMetadata, ColorTransformStage, ColorTransformStageKind, ImageColorTags,
};

#[derive(Clone)]
struct KeyNode {
    state: NodeStateFingerprint,
}

impl IntrospectNode for KeyNode {
    fn introspection(&self) -> NodeIntrospection {
        NodeIntrospection::new(
            NodeSchemaId::new(
                NodeTypeId::from_str("superi.test.cache-key").unwrap(),
                SemanticVersion::from_str("1.0.0").unwrap(),
            ),
            NodeBehavior::new(
                TimeBehavior::CurrentFrame,
                RoiBehavior::FullFrame,
                ColorRequirements::Tagged,
                Determinism::Deterministic,
                CachePolicy::PerFrame,
            ),
            self.state,
        )
    }
}

impl EvaluateNode<()> for KeyNode {
    fn evaluate(&self, _context: &EvaluationContext<'_, ()>) -> Result<()> {
        Ok(())
    }
}

#[derive(Clone)]
struct KeyFixture {
    media: Vec<MediaCacheIdentity>,
    graph: EvaluationCacheKey,
    parameters: ParameterStateFingerprint,
    color: ColorPipelineMetadata,
    time: RationalTime,
    render_settings: RenderSettingsFingerprint,
}

impl KeyFixture {
    fn baseline() -> Self {
        Self {
            media: vec![MediaCacheIdentity::new(
                MediaId::from_raw(11),
                "sha256:source-a-v1",
            )
            .unwrap()],
            graph: graph_key(b"gain=1.0"),
            parameters: ParameterStateFingerprint::from_canonical_bytes(
                b"opacity=f64:3ff0000000000000",
            ),
            color: scene_pipeline(),
            time: frame(24, 24),
            render_settings: RenderSettingsFingerprint::from_canonical_bytes(
                b"artifact=preview;width=1920;height=1080;format=rgba16f;precision=f16;quality=full",
            ),
        }
    }

    fn key(&self) -> FrameCacheKey {
        FrameCacheKey::derive(FrameCacheKeyInputs::new(
            &self.media,
            self.graph,
            self.parameters,
            &self.color,
            self.time,
            self.render_settings,
        ))
    }
}

#[test]
fn every_result_affecting_identity_axis_invalidates_reuse() {
    let baseline = KeyFixture::baseline();
    let baseline_key = baseline.key();

    let mut changed = baseline.clone();
    changed.media[0] =
        MediaCacheIdentity::new(MediaId::from_raw(11), "sha256:source-a-v2").unwrap();
    assert_ne!(baseline_key, changed.key(), "media content must invalidate");

    let mut changed = baseline.clone();
    changed.media[0] =
        MediaCacheIdentity::new(MediaId::from_raw(12), "sha256:source-a-v1").unwrap();
    assert_ne!(
        baseline_key,
        changed.key(),
        "media identity must invalidate"
    );

    let mut changed = baseline.clone();
    changed.graph = graph_key(b"gain=1.1");
    assert_ne!(baseline_key, changed.key(), "graph state must invalidate");

    let mut changed = baseline.clone();
    changed.parameters =
        ParameterStateFingerprint::from_canonical_bytes(b"opacity=f64:3fe0000000000000");
    assert_ne!(baseline_key, changed.key(), "parameters must invalidate");

    let mut changed = baseline.clone();
    changed.color = creative_pipeline(&["look-v2", "grade-v1"]);
    assert_ne!(baseline_key, changed.key(), "color state must invalidate");

    let mut changed = baseline.clone();
    changed.time = frame(48, 24);
    assert_ne!(baseline_key, changed.key(), "physical time must invalidate");

    let mut changed = baseline.clone();
    changed.render_settings = RenderSettingsFingerprint::from_canonical_bytes(
        b"artifact=preview;width=1920;height=1080;format=rgba16f;precision=f32;quality=full",
    );
    assert_ne!(
        baseline_key,
        changed.key(),
        "precision and render policy must invalidate"
    );

    let mut changed = baseline.clone();
    changed.render_settings = RenderSettingsFingerprint::from_canonical_bytes(
        b"artifact=proxy;width=960;height=540;format=rgba16f;precision=f16;quality=half",
    );
    assert_ne!(
        baseline_key,
        changed.key(),
        "proxy quality must not collide with full-quality output"
    );
}

#[test]
fn media_sets_and_physical_time_have_canonical_identity() {
    let first = MediaCacheIdentity::new(MediaId::from_raw(2), "content-b").unwrap();
    let second = MediaCacheIdentity::new(MediaId::from_raw(1), "content-a").unwrap();
    let mut baseline = KeyFixture::baseline();
    baseline.media = vec![first, second];
    baseline.time = frame(24, 24);

    let mut reordered = baseline.clone();
    reordered.media = vec![second, first, second];
    reordered.time = frame(48, 48);

    assert_eq!(baseline.key(), reordered.key());
    assert_eq!(baseline.key(), baseline.key());
}

#[test]
fn complete_color_meaning_and_transform_order_participate() {
    let baseline = scene_pipeline();
    let baseline_fingerprint = ColorPipelineFingerprint::derive(&baseline);

    let changed_named_space = ColorPipelineMetadata::new(
        ImageColorTags::new(ColorSpace::DISPLAY_P3)
            .with_named_space("display-p3-source-v2")
            .unwrap()
            .with_icc_profile(Arc::from(&b"source-icc-v1"[..]))
            .unwrap(),
    )
    .unwrap()
    .with_stage(stage(
        ColorTransformStageKind::Input,
        "input-v1",
        ColorSpace::DISPLAY_P3,
        ColorSpace::ACESCG,
    ))
    .unwrap()
    .with_stage(stage(
        ColorTransformStageKind::Creative,
        "look-v1",
        ColorSpace::ACESCG,
        ColorSpace::ACESCG,
    ))
    .unwrap()
    .with_stage(stage(
        ColorTransformStageKind::Creative,
        "grade-v1",
        ColorSpace::ACESCG,
        ColorSpace::ACESCG,
    ))
    .unwrap();
    assert_ne!(
        baseline_fingerprint,
        ColorPipelineFingerprint::derive(&changed_named_space)
    );

    let changed_icc = ColorPipelineMetadata::new(
        ImageColorTags::new(ColorSpace::DISPLAY_P3)
            .with_named_space("display-p3-source-v1")
            .unwrap()
            .with_icc_profile(Arc::from(&b"source-icc-v2"[..]))
            .unwrap(),
    )
    .unwrap()
    .with_stage(stage(
        ColorTransformStageKind::Input,
        "input-v1",
        ColorSpace::DISPLAY_P3,
        ColorSpace::ACESCG,
    ))
    .unwrap()
    .with_stage(stage(
        ColorTransformStageKind::Creative,
        "look-v1",
        ColorSpace::ACESCG,
        ColorSpace::ACESCG,
    ))
    .unwrap()
    .with_stage(stage(
        ColorTransformStageKind::Creative,
        "grade-v1",
        ColorSpace::ACESCG,
        ColorSpace::ACESCG,
    ))
    .unwrap();
    assert_ne!(
        baseline_fingerprint,
        ColorPipelineFingerprint::derive(&changed_icc)
    );

    let changed_interpretation =
        ColorPipelineMetadata::new(ImageColorTags::new(ColorSpace::SRGB)).unwrap();
    assert_ne!(
        baseline_fingerprint,
        ColorPipelineFingerprint::derive(&changed_interpretation),
        "source interpretation must not collide"
    );

    assert_ne!(
        ColorPipelineFingerprint::derive(&creative_pipeline(&["look-v1", "grade-v1"])),
        ColorPipelineFingerprint::derive(&creative_pipeline(&["grade-v1", "look-v1"]))
    );

    let display = baseline
        .clone()
        .with_stage(stage(
            ColorTransformStageKind::Display,
            "terminal-v1",
            ColorSpace::ACESCG,
            ColorSpace::SRGB,
        ))
        .unwrap();
    let output = baseline
        .with_stage(stage(
            ColorTransformStageKind::Output,
            "terminal-v1",
            ColorSpace::ACESCG,
            ColorSpace::SRGB,
        ))
        .unwrap();
    assert_ne!(
        ColorPipelineFingerprint::derive(&display),
        ColorPipelineFingerprint::derive(&output),
        "monitoring and delivery intent must not collide"
    );
}

#[test]
fn fingerprints_are_domain_separated_and_media_identity_is_validated() {
    let bytes = b"same-canonical-payload";
    let parameters = ParameterStateFingerprint::from_canonical_bytes(bytes);
    let render = RenderSettingsFingerprint::from_canonical_bytes(bytes);
    assert_ne!(parameters.as_bytes(), render.as_bytes());

    assert!(MediaCacheIdentity::new(MediaId::from_raw(1), "").is_err());
    assert!(MediaCacheIdentity::new(MediaId::from_raw(1), " \t\n").is_err());
}

#[test]
fn cache_key_has_a_locked_stable_vector_and_thread_safe_values() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<MediaCacheIdentity>();
    assert_send_sync::<ParameterStateFingerprint>();
    assert_send_sync::<RenderSettingsFingerprint>();
    assert_send_sync::<ColorPipelineFingerprint>();
    assert_send_sync::<FrameCacheKey>();

    let key = KeyFixture::baseline().key();
    assert_eq!(key.as_bytes().len(), 32);
    assert_eq!(
        key.to_string(),
        "sha256:1d51fcc9bd5405156912b58b7c8d2f1da9715e77f66495df97fed1f7e37beaf1"
    );
}

fn graph_key(state: &[u8]) -> EvaluationCacheKey {
    let node_id = NodeId::from_raw(31);
    let port_id = PortId::from_raw(41);
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(21));
    graph
        .insert_node(
            node_id,
            KeyNode {
                state: NodeStateFingerprint::from_canonical_bytes(state),
            },
        )
        .unwrap();
    let request = EvaluationRequest::new(
        GraphEndpoint::new(node_id, port_id),
        frame(24, 24),
        PixelBounds::new(0, 0, 1920, 1080).unwrap(),
    );
    LazyEvaluator::inspect::<_, ()>(&graph, request)
        .unwrap()
        .node(request.key())
        .unwrap()
        .cache_status()
        .key()
        .unwrap()
}

fn scene_pipeline() -> ColorPipelineMetadata {
    creative_pipeline(&["look-v1", "grade-v1"])
}

fn creative_pipeline(stages: &[&str]) -> ColorPipelineMetadata {
    let mut pipeline = ColorPipelineMetadata::new(
        ImageColorTags::new(ColorSpace::DISPLAY_P3)
            .with_named_space("display-p3-source-v1")
            .unwrap()
            .with_icc_profile(Arc::from(&b"source-icc-v1"[..]))
            .unwrap(),
    )
    .unwrap()
    .with_stage(stage(
        ColorTransformStageKind::Input,
        "input-v1",
        ColorSpace::DISPLAY_P3,
        ColorSpace::ACESCG,
    ))
    .unwrap();
    for name in stages {
        pipeline = pipeline
            .with_stage(stage(
                ColorTransformStageKind::Creative,
                name,
                ColorSpace::ACESCG,
                ColorSpace::ACESCG,
            ))
            .unwrap();
    }
    pipeline
}

fn stage(
    kind: ColorTransformStageKind,
    id: &str,
    source: ColorSpace,
    destination: ColorSpace,
) -> ColorTransformStage {
    ColorTransformStage::new(kind, id, source, destination).unwrap()
}

fn frame(value: i64, units_per_second: u32) -> RationalTime {
    RationalTime::new(value, Timebase::integer(units_per_second).unwrap())
}
