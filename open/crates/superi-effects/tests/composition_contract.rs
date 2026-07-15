use std::str::FromStr;

use serde::{Deserialize, Serialize};
use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory};
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_core::time::{RationalTime, TimeRange, TimeRounding, Timebase};
use superi_effects::authoring::{
    EffectInstanceBindings, EffectMetadata, EffectNodeDefinition, EffectParameterBinding,
    EffectParameterDefinition, ParameterControl,
};
use superi_effects::composition::{
    Composition, CompositionArtifact, CompositionId, CompositionLayer, CompositionLayerId,
    LayerIsolation, PrecompositionBoundaryReason, PrecompositionCollapse, ResolvedLayerKind,
    TimeRemap, TimeRemapInterpolation, TimeRemapKeyframe, MAX_COMPOSITIONS,
};
use superi_graph::ids::{GraphId, NodeId, ParameterId};
use superi_graph::mutate::{
    EditableGraph, EditableNode, GraphMutation, GraphTransaction, TypedParameterValue,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchemaId, NodeTypeId,
    ParameterName, ParameterSchema, RoiBehavior, TimeBehavior, ValueTypeId,
};
use superi_graph::serialize::{deserialize_graph, serialize_graph};
use superi_graph::value::GraphValue;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct TestVisual {
    label: String,
    masks: Vec<String>,
    effects: Vec<String>,
}

impl TestVisual {
    fn named(label: &str) -> Self {
        Self {
            label: label.to_owned(),
            masks: vec![format!("mask:{label}")],
            effects: vec![format!("effect:{label}")],
        }
    }
}

fn clock(rate: u32) -> Timebase {
    Timebase::integer(rate).unwrap()
}

fn at(value: i64, timebase: Timebase) -> RationalTime {
    RationalTime::new(value, timebase)
}

fn range(start: i64, end_exclusive: i64, timebase: Timebase) -> TimeRange {
    TimeRange::from_start_end(at(start, timebase), at(end_exclusive, timebase)).unwrap()
}

fn key(
    layer: i64,
    source: i64,
    timebase: Timebase,
    interpolation: TimeRemapInterpolation,
) -> TimeRemapKeyframe {
    TimeRemapKeyframe::new(at(layer, timebase), at(source, timebase), interpolation)
}

fn direct_remap(start: i64, end: i64, timebase: Timebase) -> TimeRemap {
    TimeRemap::new([
        key(start, start, timebase, TimeRemapInterpolation::Linear),
        key(end, end, timebase, TimeRemapInterpolation::Hold),
    ])
    .unwrap()
}

fn reason(error: &Error) -> Option<&str> {
    error
        .contexts()
        .iter()
        .find_map(|context| context.field("reason"))
}

fn fixture() -> CompositionArtifact<TestVisual> {
    let timebase = clock(24);
    let child = Composition::new(
        CompositionId::from_raw(2),
        "Reusable child",
        range(0, 4, timebase),
        [
            CompositionLayer::visual(
                CompositionLayerId::from_raw(21),
                "Child back",
                range(0, 4, timebase),
                TestVisual::named("child-back"),
                direct_remap(0, 3, timebase),
            )
            .unwrap(),
            CompositionLayer::visual(
                CompositionLayerId::from_raw(22),
                "Child front",
                range(0, 4, timebase),
                TestVisual::named("child-front"),
                direct_remap(0, 3, timebase),
            )
            .unwrap()
            .with_parent(Some(CompositionLayerId::from_raw(21))),
        ],
    )
    .unwrap();

    let nested_remap = TimeRemap::new([
        key(0, 0, timebase, TimeRemapInterpolation::Linear),
        key(8, 4, timebase, TimeRemapInterpolation::Hold),
    ])
    .unwrap();
    let root = Composition::new(
        CompositionId::from_raw(1),
        "Root",
        range(0, 8, timebase),
        [
            CompositionLayer::visual(
                CompositionLayerId::from_raw(11),
                "Background",
                range(0, 8, timebase),
                TestVisual::named("root-background"),
                direct_remap(0, 7, timebase),
            )
            .unwrap(),
            CompositionLayer::precomposition(
                CompositionLayerId::from_raw(12),
                "Collapsed A",
                range(0, 8, timebase),
                CompositionId::from_raw(2),
                TestVisual::named("collapsed-a"),
                nested_remap.clone(),
                PrecompositionCollapse::Collapse,
                LayerIsolation::PassThrough,
            )
            .unwrap(),
            CompositionLayer::precomposition(
                CompositionLayerId::from_raw(14),
                "Collapsed B",
                range(0, 8, timebase),
                CompositionId::from_raw(2),
                TestVisual::named("collapsed-b"),
                nested_remap.clone(),
                PrecompositionCollapse::Collapse,
                LayerIsolation::PassThrough,
            )
            .unwrap(),
            CompositionLayer::precomposition(
                CompositionLayerId::from_raw(13),
                "Isolated",
                range(0, 8, timebase),
                CompositionId::from_raw(2),
                TestVisual::named("isolated-instance"),
                nested_remap,
                PrecompositionCollapse::Collapse,
                LayerIsolation::RequiresIntermediateSurface,
            )
            .unwrap(),
        ],
    )
    .unwrap();
    CompositionArtifact::new(CompositionId::from_raw(1), [root, child]).unwrap()
}

#[test]
fn exact_time_remapping_supports_speed_reverse_freeze_and_explicit_rounding() {
    let timebase = clock(24);
    let remap = TimeRemap::new([
        key(0, 10, timebase, TimeRemapInterpolation::Linear),
        key(4, 18, timebase, TimeRemapInterpolation::Linear),
        key(8, 14, timebase, TimeRemapInterpolation::Hold),
        key(12, 14, timebase, TimeRemapInterpolation::Hold),
    ])
    .unwrap();

    let speed = remap.sample(at(2, timebase), TimeRounding::Exact).unwrap();
    assert_eq!(speed.source_time(), at(14, timebase));
    assert_eq!(speed.segment_index(), 0);
    assert_eq!(speed.interpolation(), TimeRemapInterpolation::Linear);

    let reverse = remap.sample(at(6, timebase), TimeRounding::Exact).unwrap();
    assert_eq!(reverse.source_time(), at(16, timebase));
    assert_eq!(reverse.segment_index(), 1);

    let freeze = remap.sample(at(10, timebase), TimeRounding::Exact).unwrap();
    assert_eq!(freeze.source_time(), at(14, timebase));
    assert_eq!(freeze.interpolation(), TimeRemapInterpolation::Hold);
    assert_eq!(
        remap
            .sample(at(-2, timebase), TimeRounding::Exact)
            .unwrap()
            .source_time(),
        at(10, timebase)
    );
    assert_eq!(
        remap
            .sample(at(20, timebase), TimeRounding::Exact)
            .unwrap()
            .source_time(),
        at(14, timebase)
    );

    let fractional = TimeRemap::new([
        key(0, 0, timebase, TimeRemapInterpolation::Linear),
        key(3, 1, timebase, TimeRemapInterpolation::Hold),
    ])
    .unwrap();
    let exact = fractional
        .sample(at(1, timebase), TimeRounding::Exact)
        .unwrap_err();
    assert_eq!(reason(&exact), Some("inexact_time_remap"));
    assert_eq!(
        fractional
            .sample(at(1, timebase), TimeRounding::Ceil)
            .unwrap()
            .source_time(),
        at(1, timebase)
    );

    let wide = TimeRemap::new([
        TimeRemapKeyframe::new(
            at(i64::MIN, timebase),
            at(0, timebase),
            TimeRemapInterpolation::Linear,
        ),
        TimeRemapKeyframe::new(
            at(i64::MAX, timebase),
            at(1, timebase),
            TimeRemapInterpolation::Hold,
        ),
    ])
    .unwrap();
    assert_eq!(
        wide.sample(at(0, timebase), TimeRounding::Floor)
            .unwrap()
            .source_time(),
        at(0, timebase)
    );
}

#[test]
fn parenting_precomposition_and_collapse_resolution_preserve_complete_layer_paths() {
    let artifact = fixture();
    let frame = artifact
        .resolve_frame(at(4, clock(24)), TimeRounding::Exact)
        .unwrap();
    assert_eq!(frame.composition_id(), CompositionId::from_raw(1));
    assert_eq!(frame.time(), at(4, clock(24)));
    assert_eq!(frame.layers().len(), 6);

    let labels = frame
        .layers()
        .iter()
        .map(|layer| layer.payload().label.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        labels,
        [
            "root-background",
            "child-back",
            "child-front",
            "child-back",
            "child-front",
            "isolated-instance",
        ]
    );

    let first_collapsed = &frame.layers()[1];
    assert_eq!(first_collapsed.kind(), ResolvedLayerKind::Visual);
    assert_eq!(first_collapsed.source_time(), at(2, clock(24)));
    assert_eq!(first_collapsed.path().len(), 2);
    assert_eq!(
        first_collapsed.path()[0].layer().payload().label,
        "collapsed-a"
    );
    assert_eq!(
        first_collapsed.path()[1].layer().payload().masks,
        ["mask:child-back"]
    );
    assert_eq!(
        frame.layers()[2].path()[1].parent_chain(),
        [CompositionLayerId::from_raw(21)]
    );

    let second_instance = &frame.layers()[3];
    assert_eq!(
        second_instance.path()[0].layer().payload().label,
        "collapsed-b"
    );
    assert_eq!(
        second_instance.path()[1].layer().payload().label,
        "child-back"
    );

    let isolated = &frame.layers()[5];
    assert_eq!(isolated.payload().effects, ["effect:isolated-instance"]);
    assert_eq!(isolated.path().len(), 1);
    assert_eq!(
        isolated.kind(),
        ResolvedLayerKind::PrecompositionBoundary {
            composition_id: CompositionId::from_raw(2),
            reason: PrecompositionBoundaryReason::IntermediateSurfaceRequired,
        }
    );
}

#[test]
fn immutable_edits_are_reusable_and_invalid_relationships_never_publish() {
    let artifact = fixture();
    let child_id = CompositionId::from_raw(2);
    let front_id = CompositionLayerId::from_raw(22);
    let child = artifact.composition(child_id).unwrap();
    let edited_front = child
        .layer(front_id)
        .unwrap()
        .clone()
        .with_payload(TestVisual::named("child-front-v2"));
    let edited_child = child
        .with_replaced_layer(edited_front)
        .unwrap()
        .with_reordered_layer(front_id, 0)
        .unwrap();
    let edited = artifact.with_replaced_composition(edited_child).unwrap();

    assert_eq!(artifact.revision(), 0);
    assert_eq!(edited.revision(), 1);
    assert_eq!(
        artifact
            .composition(child_id)
            .unwrap()
            .layer(front_id)
            .unwrap()
            .payload()
            .label,
        "child-front"
    );
    let resolved = edited
        .resolve_frame(at(4, clock(24)), TimeRounding::Exact)
        .unwrap();
    let updated_instances = resolved
        .layers()
        .iter()
        .filter(|layer| layer.payload().label == "child-front-v2")
        .count();
    assert_eq!(updated_instances, 2);

    let child = artifact.composition(child_id).unwrap();
    let cycle = child
        .with_replaced_layer(
            child
                .layer(CompositionLayerId::from_raw(21))
                .unwrap()
                .clone()
                .with_parent(Some(front_id)),
        )
        .unwrap_err();
    assert_eq!(reason(&cycle), Some("layer_parent_cycle"));

    let visual = child
        .layer(CompositionLayerId::from_raw(21))
        .unwrap()
        .clone();
    let invalid_collapse = visual
        .with_collapse(PrecompositionCollapse::Collapse)
        .unwrap_err();
    assert_eq!(
        reason(&invalid_collapse),
        Some("collapse_requires_precomposition")
    );

    let root_reference = CompositionLayer::precomposition(
        CompositionLayerId::from_raw(23),
        "Back to root",
        range(0, 4, clock(24)),
        CompositionId::from_raw(1),
        TestVisual::named("cycle"),
        direct_remap(0, 3, clock(24)),
        PrecompositionCollapse::PreserveBoundary,
        LayerIsolation::PassThrough,
    )
    .unwrap();
    let cyclic_child = child.with_layer(root_reference, 2).unwrap();
    let cyclic = CompositionArtifact::new(
        CompositionId::from_raw(1),
        [
            artifact
                .composition(CompositionId::from_raw(1))
                .unwrap()
                .clone(),
            cyclic_child,
        ],
    )
    .unwrap_err();
    assert_eq!(reason(&cyclic), Some("composition_cycle"));
}

#[test]
fn authored_boundary_preservation_stops_nested_expansion_without_losing_state() {
    let artifact = fixture();
    let root_id = CompositionId::from_raw(1);
    let layer_id = CompositionLayerId::from_raw(12);
    let root = artifact.composition(root_id).unwrap();
    let preserved = root
        .layer(layer_id)
        .unwrap()
        .clone()
        .with_collapse(PrecompositionCollapse::PreserveBoundary)
        .unwrap();
    let edited = artifact
        .with_replaced_composition(root.with_replaced_layer(preserved).unwrap())
        .unwrap();
    let frame = edited
        .resolve_frame(at(4, clock(24)), TimeRounding::Exact)
        .unwrap();
    assert_eq!(
        frame.layers()[1].kind(),
        ResolvedLayerKind::PrecompositionBoundary {
            composition_id: CompositionId::from_raw(2),
            reason: PrecompositionBoundaryReason::CollapseDisabled,
        }
    );
    assert_eq!(
        frame.layers()[1].payload(),
        &TestVisual::named("collapsed-a")
    );
}

#[test]
fn standalone_wire_is_versioned_bounded_and_revalidates_every_relationship() {
    let artifact = fixture();
    let document = serde_json::to_value(&artifact).unwrap();
    assert_eq!(document["schema_revision"], 1);
    assert_eq!(
        serde_json::from_value::<CompositionArtifact<TestVisual>>(document.clone()).unwrap(),
        artifact
    );

    let mut future = document.clone();
    future["schema_revision"] = 2.into();
    assert!(serde_json::from_value::<CompositionArtifact<TestVisual>>(future).is_err());

    let mut unknown = document.clone();
    unknown["future_control"] = true.into();
    assert!(serde_json::from_value::<CompositionArtifact<TestVisual>>(unknown).is_err());

    let mut oversized = document.clone();
    oversized["compositions"] = serde_json::Value::Array(vec![
        document["compositions"][0].clone();
        MAX_COMPOSITIONS + 1
    ]);
    assert!(serde_json::from_value::<CompositionArtifact<TestVisual>>(oversized).is_err());

    let mut missing_parent = document.clone();
    missing_parent["compositions"][1]["layers"][0]["parent_id"] = 999.into();
    assert!(serde_json::from_value::<CompositionArtifact<TestVisual>>(missing_parent).is_err());

    let mut exhausted = document;
    exhausted["content_revision"] = serde_json::json!(u64::MAX);
    let exhausted: CompositionArtifact<TestVisual> = serde_json::from_value(exhausted).unwrap();
    let overflow = exhausted
        .with_replaced_composition(
            exhausted
                .composition(CompositionId::from_raw(2))
                .unwrap()
                .clone(),
        )
        .unwrap_err();
    assert_eq!(reason(&overflow), Some("revision_overflow"));
}

fn value_type(input: &str) -> ValueTypeId {
    ValueTypeId::from_str(input).unwrap()
}

fn parameter(input: &str) -> ParameterName {
    ParameterName::new(input).unwrap()
}

fn composition_node(
    node_type: &str,
    artifact: CompositionArtifact<TestVisual>,
) -> EditableNode<CompositionArtifact<TestVisual>> {
    let artifact_type = value_type("superi.value.composition_artifact");
    let definition = EffectNodeDefinition::new(
        NodeSchemaId::new(
            NodeTypeId::from_str(node_type).unwrap(),
            SemanticVersion::new(1, 0, 0),
        ),
        EffectMetadata::new(
            "Composition",
            "Stores editable visual layers and nested composition relationships.",
            "Compositing",
        )
        .unwrap(),
        [],
        [],
        [EffectParameterDefinition::new(
            ParameterSchema::new(parameter("artifact"), artifact_type.clone(), true),
            "Artifact",
            "Editable exact-time composition state.",
            ParameterControl::Automatic,
            TypedParameterValue::new(artifact_type, artifact),
        )
        .unwrap()],
        NodeBehavior::new(
            TimeBehavior::CurrentFrame,
            RoiBehavior::InputBounds,
            ColorRequirements::Exact(ColorSpace::ACESCG),
            Determinism::Deterministic,
            CachePolicy::PerRegion,
        ),
        CapabilitySet::default(),
    )
    .unwrap();
    assert!(definition
        .parameter(&parameter("artifact"))
        .unwrap()
        .schema()
        .is_animatable());
    definition
        .instantiate(
            EffectInstanceBindings::new(
                [],
                [],
                [EffectParameterBinding::new(
                    ParameterId::from_raw(701),
                    parameter("artifact"),
                )],
            ),
            [],
        )
        .unwrap()
}

fn workflow_graph(
    graph_id: u128,
    node_id: u128,
    node_type: &str,
    artifact: CompositionArtifact<TestVisual>,
) -> EditableGraph<CompositionArtifact<TestVisual>> {
    let mut graph = EditableGraph::new(GraphId::from_raw(graph_id));
    graph
        .apply(GraphTransaction::with_mutations(
            0,
            [GraphMutation::Add {
                node_id: NodeId::from_raw(node_id),
                node: composition_node(node_type, artifact),
                position: 0,
            }],
        ))
        .unwrap();
    graph
}

#[test]
fn one_artifact_round_trips_identically_through_timeline_and_graph_workflow_roles() {
    let artifact = fixture();
    let graph_value = GraphValue::Domain(artifact.clone());
    assert_eq!(graph_value.as_domain(), Some(&artifact));

    let timeline = workflow_graph(
        710,
        71,
        "superi.effects.timeline_composition",
        artifact.clone(),
    );
    let node_graph = workflow_graph(
        720,
        72,
        "superi.effects.graph_composition",
        artifact.clone(),
    );

    let timeline_bytes = serialize_graph(&timeline.snapshot()).unwrap();
    let graph_bytes = serialize_graph(&node_graph.snapshot()).unwrap();
    let timeline_loaded =
        deserialize_graph::<CompositionArtifact<TestVisual>>(&timeline_bytes).unwrap();
    let graph_loaded = deserialize_graph::<CompositionArtifact<TestVisual>>(&graph_bytes).unwrap();
    let timeline_artifact = timeline_loaded
        .graph()
        .snapshot()
        .node(NodeId::from_raw(71))
        .unwrap()
        .parameter(ParameterId::from_raw(701))
        .unwrap()
        .value()
        .payload()
        .clone();
    let graph_artifact = graph_loaded
        .graph()
        .snapshot()
        .node(NodeId::from_raw(72))
        .unwrap()
        .parameter(ParameterId::from_raw(701))
        .unwrap()
        .value()
        .payload()
        .clone();

    assert_eq!(timeline_artifact, artifact);
    assert_eq!(graph_artifact, artifact);
    assert_eq!(
        timeline_artifact
            .resolve_frame(at(4, clock(24)), TimeRounding::Exact)
            .unwrap(),
        graph_artifact
            .resolve_frame(at(4, clock(24)), TimeRounding::Exact)
            .unwrap()
    );
    assert_eq!(
        serialize_graph(&timeline_loaded.graph().snapshot()).unwrap(),
        timeline_bytes
    );
    assert_eq!(
        serialize_graph(&graph_loaded.graph().snapshot()).unwrap(),
        graph_bytes
    );
}

#[test]
fn malformed_time_and_layer_controls_are_rejected_before_publication() {
    let timebase = clock(24);
    let duplicate = TimeRemap::new([
        key(0, 0, timebase, TimeRemapInterpolation::Linear),
        key(0, 1, timebase, TimeRemapInterpolation::Hold),
    ])
    .unwrap_err();
    assert_eq!(reason(&duplicate), Some("unordered_time_remap_keys"));

    let wrong_clock = TimeRemap::new([
        TimeRemapKeyframe::new(
            at(0, timebase),
            at(0, clock(30)),
            TimeRemapInterpolation::Linear,
        ),
        TimeRemapKeyframe::new(
            at(1, timebase),
            at(1, timebase),
            TimeRemapInterpolation::Hold,
        ),
    ])
    .unwrap_err();
    assert_eq!(reason(&wrong_clock), Some("mixed_source_timebases"));

    let layer = CompositionLayer::visual(
        CompositionLayerId::from_raw(1),
        "Layer",
        range(0, 2, timebase),
        TestVisual::named("layer"),
        TimeRemap::new([key(0, 0, clock(30), TimeRemapInterpolation::Hold)]).unwrap(),
    )
    .unwrap_err();
    assert_eq!(reason(&layer), Some("layer_timebase_mismatch"));

    let outside = fixture()
        .resolve_frame(at(8, timebase), TimeRounding::Exact)
        .unwrap_err();
    assert_eq!(outside.category(), ErrorCategory::InvalidInput);
    assert_eq!(reason(&outside), Some("time_outside_composition"));

    let rounded_outside = fixture()
        .resolve_frame(at(7, timebase), TimeRounding::Ceil)
        .unwrap_err();
    assert_eq!(
        reason(&rounded_outside),
        Some("mapped_time_outside_precomposition")
    );
}
