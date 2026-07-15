use std::str::FromStr;

use serde::{Deserialize, Serialize};
use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, Result};
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_core::time::{RationalTime, TimeRange, Timebase};
use superi_effects::authoring::{
    EffectInstanceBindings, EffectMetadata, EffectNodeDefinition, EffectParameterBinding,
    EffectParameterDefinition, ParameterControl,
};
use superi_effects::rotoscope::{
    PropagationDirection, PropagationRequest, PropagationResult, RotoscopeArtifact, RotoscopeFrame,
    RotoscopeFrameSource, RotoscopePropagator, RotoscopeSpan, RotoscopeSpanId,
    MAX_ROTOSCOPE_FRAMES_PER_SPAN,
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
struct TestMask {
    layers: Vec<String>,
    composition: String,
}

impl TestMask {
    fn named(name: &str) -> Self {
        Self {
            layers: vec![name.to_owned(), format!("{name}-feather")],
            composition: format!("over:{name}"),
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

fn sample(frame: i64, timebase: Timebase, name: &str) -> RotoscopeFrame<TestMask> {
    RotoscopeFrame::new(at(frame, timebase), TestMask::named(name))
}

fn span(
    id: u64,
    start: i64,
    end_exclusive: i64,
    base_frame: i64,
    timebase: Timebase,
    name: &str,
) -> RotoscopeSpan<TestMask> {
    RotoscopeSpan::new(
        RotoscopeSpanId::from_raw(id),
        range(start, end_exclusive, timebase),
        sample(base_frame, timebase, name),
    )
    .unwrap()
}

fn reason(error: &Error) -> Option<&str> {
    error
        .contexts()
        .iter()
        .find_map(|context| context.field("reason"))
}

fn frame_values(frames: &[RationalTime]) -> Vec<i64> {
    frames.iter().map(|frame| frame.value()).collect()
}

fn anchor_values(request: &PropagationRequest<TestMask>) -> Vec<i64> {
    request
        .anchors()
        .iter()
        .map(|anchor| anchor.frame().value())
        .collect()
}

fn artifact_with_bidirectional_corrections() -> RotoscopeArtifact<TestMask> {
    let timebase = clock(24);
    RotoscopeArtifact::new(timebase, [span(7, -2, 5, 1, timebase, "base")])
        .unwrap()
        .with_correction(
            RotoscopeSpanId::from_raw(7),
            sample(3, timebase, "forward-fix"),
        )
        .unwrap()
        .with_correction(
            RotoscopeSpanId::from_raw(7),
            sample(-1, timebase, "backward-fix"),
        )
        .unwrap()
}

#[test]
fn propagation_requests_expose_exact_directional_targets_and_authored_anchors() {
    let artifact = artifact_with_bidirectional_corrections();
    let span_id = RotoscopeSpanId::from_raw(7);

    let forward = artifact
        .propagation_request(span_id, PropagationDirection::Forward)
        .unwrap();
    assert_eq!(forward.source_revision(), artifact.revision());
    assert_eq!(forward.span_id(), span_id);
    assert_eq!(forward.direction(), PropagationDirection::Forward);
    assert_eq!(anchor_values(&forward), vec![1, 3]);
    assert_eq!(frame_values(forward.target_frames()), vec![2, 4]);

    let backward = artifact
        .propagation_request(span_id, PropagationDirection::Backward)
        .unwrap();
    assert_eq!(backward.source_revision(), artifact.revision());
    assert_eq!(backward.direction(), PropagationDirection::Backward);
    assert_eq!(anchor_values(&backward), vec![1, -1]);
    assert_eq!(frame_values(backward.target_frames()), vec![0, -2]);

    assert_eq!(forward.anchors()[0].payload(), &TestMask::named("base"));
    assert_eq!(
        forward.anchors()[1].payload(),
        &TestMask::named("forward-fix")
    );
    assert_eq!(
        backward.anchors()[1].payload(),
        &TestMask::named("backward-fix")
    );
}

struct AnchorPropagator;

impl RotoscopePropagator<TestMask> for AnchorPropagator {
    fn propagate(
        &self,
        request: &PropagationRequest<TestMask>,
    ) -> Result<PropagationResult<TestMask>> {
        let frames = request.target_frames().iter().map(|target| {
            let anchor = request
                .anchors()
                .iter()
                .rfind(|anchor| match request.direction() {
                    PropagationDirection::Forward => anchor.frame().value() < target.value(),
                    PropagationDirection::Backward => anchor.frame().value() > target.value(),
                })
                .expect("every target follows a directional anchor");
            RotoscopeFrame::new(
                *target,
                TestMask {
                    layers: anchor.payload().layers.clone(),
                    composition: format!("propagated:{}", anchor.payload().composition),
                },
            )
        });
        PropagationResult::new(request, frames)
    }
}

#[test]
fn hook_results_remain_inspectable_and_manual_corrections_win_after_repropagation() {
    let span_id = RotoscopeSpanId::from_raw(7);
    let timebase = clock(24);
    let artifact = artifact_with_bidirectional_corrections()
        .propagate_with(span_id, PropagationDirection::Forward, &AnchorPropagator)
        .unwrap()
        .propagate_with(span_id, PropagationDirection::Backward, &AnchorPropagator)
        .unwrap();

    let base = artifact.resolved_frame(span_id, at(1, timebase)).unwrap();
    assert_eq!(base.source(), RotoscopeFrameSource::Base);
    assert_eq!(base.payload(), &TestMask::named("base"));

    let correction = artifact.resolved_frame(span_id, at(3, timebase)).unwrap();
    assert_eq!(correction.source(), RotoscopeFrameSource::Correction);
    assert_eq!(correction.payload(), &TestMask::named("forward-fix"));

    let propagated = artifact.resolved_frame(span_id, at(4, timebase)).unwrap();
    assert_eq!(propagated.source(), RotoscopeFrameSource::Propagation);
    assert_eq!(
        propagated.payload().composition,
        "propagated:over:forward-fix"
    );

    let edited = artifact
        .with_correction(span_id, sample(3, timebase, "forward-fix-v2"))
        .unwrap();
    assert!(edited.resolved_frame(span_id, at(4, timebase)).is_none());
    assert_eq!(
        edited
            .resolved_frame(span_id, at(2, timebase))
            .unwrap()
            .source(),
        RotoscopeFrameSource::Propagation
    );

    let regenerated = edited
        .propagate_with(span_id, PropagationDirection::Forward, &AnchorPropagator)
        .unwrap();
    assert_eq!(
        regenerated
            .resolved_frame(span_id, at(3, timebase))
            .unwrap()
            .payload(),
        &TestMask::named("forward-fix-v2")
    );
    assert_eq!(
        regenerated
            .resolved_frame(span_id, at(4, timebase))
            .unwrap()
            .payload()
            .composition,
        "propagated:over:forward-fix-v2"
    );

    let removed = regenerated
        .without_correction(span_id, at(3, timebase))
        .unwrap();
    assert!(removed.resolved_frame(span_id, at(3, timebase)).is_none());
    assert!(removed.resolved_frame(span_id, at(4, timebase)).is_none());
    assert_eq!(removed.span(span_id).unwrap().corrections().len(), 1);

    let cleared = removed
        .without_propagation(span_id, PropagationDirection::Backward)
        .unwrap();
    assert_eq!(cleared.span(span_id).unwrap().propagated_frames().len(), 1);
    assert_eq!(
        cleared.span(span_id).unwrap().propagated_frames()[0]
            .frame()
            .value(),
        2
    );
}

#[test]
fn span_and_base_edits_are_atomic_canonical_and_fully_inspectable() {
    let timebase = clock(24);
    let initial = RotoscopeArtifact::new(timebase, [span(10, 5, 8, 5, timebase, "later")]).unwrap();
    let added = initial
        .with_span(span(2, 0, 5, 2, timebase, "earlier"))
        .unwrap();
    assert_eq!(
        added
            .spans()
            .iter()
            .map(RotoscopeSpan::id)
            .collect::<Vec<_>>(),
        vec![RotoscopeSpanId::from_raw(2), RotoscopeSpanId::from_raw(10)]
    );

    let rebased = added
        .with_base(
            RotoscopeSpanId::from_raw(2),
            sample(1, timebase, "new-base"),
        )
        .unwrap();
    assert_eq!(
        rebased.span(RotoscopeSpanId::from_raw(2)).unwrap().base(),
        &sample(1, timebase, "new-base")
    );

    let replacement = span(10, 6, 9, 6, timebase, "replacement");
    let replaced = rebased.with_replaced_span(replacement).unwrap();
    assert_eq!(
        replaced
            .span(RotoscopeSpanId::from_raw(10))
            .unwrap()
            .range(),
        range(6, 9, timebase)
    );
    let overlap = replaced
        .with_replaced_span(span(10, 4, 9, 4, timebase, "overlap"))
        .unwrap_err();
    assert_eq!(reason(&overlap), Some("overlapping_spans"));
    assert_eq!(replaced.spans().len(), 2);

    let removed = replaced.without_span(RotoscopeSpanId::from_raw(2)).unwrap();
    assert_eq!(removed.spans().len(), 1);
    assert_eq!(removed.spans()[0].id(), RotoscopeSpanId::from_raw(10));
    assert_eq!(removed.revision(), 4);
}

#[test]
fn propagation_application_is_atomic_revision_fenced_and_exact() {
    let span_id = RotoscopeSpanId::from_raw(7);
    let timebase = clock(24);
    let artifact = artifact_with_bidirectional_corrections();
    let request = artifact
        .propagation_request(span_id, PropagationDirection::Forward)
        .unwrap();

    let incomplete =
        PropagationResult::new(&request, [sample(2, timebase, "only-one-target")]).unwrap_err();
    assert_eq!(reason(&incomplete), Some("result_frame_set_mismatch"));

    let unbounded = PropagationResult::new(
        &request,
        std::iter::repeat_with(|| sample(2, timebase, "too-many-targets")),
    )
    .unwrap_err();
    assert_eq!(reason(&unbounded), Some("result_frame_set_mismatch"));

    let wrong_clock = PropagationResult::new(
        &request,
        [
            sample(2, clock(25), "wrong-clock"),
            sample(4, timebase, "right-clock"),
        ],
    )
    .unwrap_err();
    assert_eq!(reason(&wrong_clock), Some("timebase_mismatch"));

    let result = AnchorPropagator.propagate(&request).unwrap();
    let edited = artifact
        .with_correction(span_id, sample(2, timebase, "late-edit"))
        .unwrap();
    let stale = edited.apply_propagation(result).unwrap_err();
    assert_eq!(reason(&stale), Some("stale_propagation_result"));

    let foreign_timebase = clock(24);
    let foreign = RotoscopeArtifact::new(
        foreign_timebase,
        [span(99, 0, 3, 0, foreign_timebase, "foreign")],
    )
    .unwrap();
    let foreign_result = AnchorPropagator
        .propagate(
            &foreign
                .propagation_request(RotoscopeSpanId::from_raw(99), PropagationDirection::Forward)
                .unwrap(),
        )
        .unwrap();
    let wrong_span = RotoscopeArtifact::new(timebase, [span(7, 0, 3, 0, timebase, "local")])
        .unwrap()
        .apply_propagation(foreign_result)
        .unwrap_err();
    assert_eq!(reason(&wrong_span), Some("unknown_span"));
}

#[test]
fn checked_state_rejects_overlaps_wrong_clocks_large_spans_and_revision_overflow() {
    let timebase = clock(24);
    let wrong_clock =
        RotoscopeArtifact::new(timebase, [span(1, 0, 3, 0, clock(25), "wrong-clock")]).unwrap_err();
    assert_eq!(reason(&wrong_clock), Some("timebase_mismatch"));

    let overlap = RotoscopeArtifact::new(
        timebase,
        [
            span(1, 0, 5, 0, timebase, "first"),
            span(2, 4, 8, 4, timebase, "second"),
        ],
    )
    .unwrap_err();
    assert_eq!(reason(&overlap), Some("overlapping_spans"));

    let too_large_end = i64::try_from(MAX_ROTOSCOPE_FRAMES_PER_SPAN + 1).unwrap();
    let too_large = RotoscopeSpan::new(
        RotoscopeSpanId::from_raw(1),
        range(0, too_large_end, timebase),
        sample(0, timebase, "base"),
    )
    .unwrap_err();
    assert_eq!(reason(&too_large), Some("span_frame_limit_exceeded"));

    let artifact = RotoscopeArtifact::new(timebase, [span(1, 0, 3, 0, timebase, "base")]).unwrap();
    let mut wire = serde_json::to_value(artifact).unwrap();
    wire["content_revision"] = serde_json::json!(u64::MAX);
    let exhausted: RotoscopeArtifact<TestMask> = serde_json::from_value(wire).unwrap();
    let overflow = exhausted
        .with_correction(RotoscopeSpanId::from_raw(1), sample(1, timebase, "edit"))
        .unwrap_err();
    assert_eq!(reason(&overflow), Some("revision_overflow"));
}

#[test]
fn standalone_wire_is_versioned_strict_and_checked_on_reload() {
    let artifact = artifact_with_bidirectional_corrections();
    let document = serde_json::to_value(&artifact).unwrap();
    assert_eq!(document["schema_revision"], 1);
    assert_eq!(
        serde_json::from_value::<RotoscopeArtifact<TestMask>>(document.clone()).unwrap(),
        artifact
    );

    let mut future = document.clone();
    future["schema_revision"] = 2.into();
    assert!(serde_json::from_value::<RotoscopeArtifact<TestMask>>(future).is_err());

    let mut unknown = document.clone();
    unknown["unexpected"] = true.into();
    assert!(serde_json::from_value::<RotoscopeArtifact<TestMask>>(unknown).is_err());

    let mut invalid_base = document.clone();
    invalid_base["spans"][0]["base"]["frame"]["value"] = "99".into();
    assert!(serde_json::from_value::<RotoscopeArtifact<TestMask>>(invalid_base).is_err());

    let mut duplicate_frame = document;
    let duplicate = duplicate_frame["spans"][0]["corrections"][0].clone();
    duplicate_frame["spans"][0]["corrections"]
        .as_array_mut()
        .unwrap()
        .push(duplicate);
    assert!(serde_json::from_value::<RotoscopeArtifact<TestMask>>(duplicate_frame).is_err());
}

fn value_type(input: &str) -> ValueTypeId {
    ValueTypeId::from_str(input).unwrap()
}

fn parameter(input: &str) -> ParameterName {
    ParameterName::new(input).unwrap()
}

fn rotoscope_node(
    artifact: RotoscopeArtifact<TestMask>,
) -> EditableNode<RotoscopeArtifact<TestMask>> {
    let rotoscope_type = value_type("superi.value.rotoscope_artifact");
    let definition = EffectNodeDefinition::new(
        NodeSchemaId::new(
            NodeTypeId::from_str("superi.effects.rotoscope").unwrap(),
            SemanticVersion::new(1, 0, 0),
        ),
        EffectMetadata::new(
            "Rotoscope",
            "Stores editable mask anchors, corrections, and propagation results.",
            "Masking",
        )
        .unwrap(),
        [],
        [],
        [EffectParameterDefinition::new(
            ParameterSchema::new(parameter("artifact"), rotoscope_type.clone(), true),
            "Artifact",
            "Editable exact-frame rotoscope state.",
            ParameterControl::Automatic,
            TypedParameterValue::new(rotoscope_type, artifact),
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
                    ParameterId::from_raw(601),
                    parameter("artifact"),
                )],
            ),
            [],
        )
        .unwrap()
}

#[test]
fn rotoscope_state_remains_editable_through_graph_value_and_real_graph_reload() {
    let artifact = artifact_with_bidirectional_corrections()
        .propagate_with(
            RotoscopeSpanId::from_raw(7),
            PropagationDirection::Forward,
            &AnchorPropagator,
        )
        .unwrap();
    let graph_value = GraphValue::Domain(artifact.clone());
    assert_eq!(graph_value.as_domain(), Some(&artifact));

    let mut graph = EditableGraph::new(GraphId::from_raw(606));
    graph
        .apply(GraphTransaction::with_mutations(
            0,
            [GraphMutation::Add {
                node_id: NodeId::from_raw(6),
                node: rotoscope_node(artifact.clone()),
                position: 0,
            }],
        ))
        .unwrap();

    let bytes = serialize_graph(&graph.snapshot()).unwrap();
    let loaded = deserialize_graph::<RotoscopeArtifact<TestMask>>(&bytes).unwrap();
    let loaded_snapshot = loaded.graph().snapshot();
    let reloaded = loaded_snapshot
        .node(NodeId::from_raw(6))
        .unwrap()
        .parameter(ParameterId::from_raw(601))
        .unwrap()
        .value()
        .payload();

    assert_eq!(reloaded, &artifact);
    assert_eq!(
        reloaded
            .span(RotoscopeSpanId::from_raw(7))
            .unwrap()
            .corrections()
            .len(),
        2
    );
    assert_eq!(
        reloaded
            .resolved_frame(RotoscopeSpanId::from_raw(7), at(4, clock(24)))
            .unwrap()
            .payload()
            .composition,
        "propagated:over:forward-fix"
    );
    assert_eq!(serialize_graph(&loaded.graph().snapshot()).unwrap(), bytes);
}
