use std::str::FromStr;
use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory};
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_core::time::{RationalTime, Timebase};
use superi_effects::authoring::{
    EffectInstanceBindings, EffectMetadata, EffectNodeDefinition, EffectParameterBinding,
    EffectParameterDefinition, ParameterControl,
};
use superi_effects::keyframe::{
    AnimationCurve, AnimationValue, CubicEasing, Easing, Interpolation, Keyframe, KeyframeTiming,
    TimeExpression, ValueTangent,
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

fn clock(rate: u32) -> Timebase {
    Timebase::integer(rate).unwrap()
}

fn at(value: i64, timebase: Timebase) -> RationalTime {
    RationalTime::new(value, timebase)
}

fn value(components: &[f64]) -> AnimationValue {
    AnimationValue::new(components.to_vec()).unwrap()
}

fn tangent(components_per_second: &[f64]) -> ValueTangent {
    ValueTangent::new(components_per_second.to_vec()).unwrap()
}

fn key(
    timing: KeyframeTiming,
    components: &[f64],
    outgoing: Interpolation,
    easing: Easing,
    incoming_tangent: Option<ValueTangent>,
    outgoing_tangent: Option<ValueTangent>,
) -> Keyframe {
    Keyframe::new(
        timing,
        value(components),
        outgoing,
        easing,
        incoming_tangent,
        outgoing_tangent,
    )
    .unwrap()
}

fn fixed(time: i64, timebase: Timebase, components: &[f64]) -> Keyframe {
    key(
        KeyframeTiming::Fixed(at(time, timebase)),
        components,
        Interpolation::Linear,
        Easing::Linear,
        None,
        None,
    )
}

fn roving(components: &[f64]) -> Keyframe {
    key(
        KeyframeTiming::Roving,
        components,
        Interpolation::Linear,
        Easing::Linear,
        None,
        None,
    )
}

fn reason(error: &Error) -> Option<&str> {
    error
        .contexts()
        .iter()
        .find_map(|context| context.field("reason"))
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1.0e-9,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn linear_hold_and_cubic_segments_evaluate_at_exact_rational_time() {
    let timebase = clock(10);
    let linear = AnimationCurve::new(
        timebase,
        [
            fixed(0, timebase, &[0.0, 10.0]),
            fixed(10, timebase, &[10.0, 20.0]),
        ],
        None,
    )
    .unwrap();

    assert_eq!(
        linear.evaluate(at(-1, timebase)).unwrap().components(),
        &[0.0, 10.0]
    );
    assert_eq!(
        linear.evaluate(at(5, timebase)).unwrap().components(),
        &[5.0, 15.0]
    );
    assert_eq!(
        linear.evaluate(at(11, timebase)).unwrap().components(),
        &[10.0, 20.0]
    );

    let hold = AnimationCurve::new(
        timebase,
        [
            key(
                KeyframeTiming::Fixed(at(0, timebase)),
                &[3.0],
                Interpolation::Hold,
                Easing::Linear,
                None,
                None,
            ),
            fixed(10, timebase, &[9.0]),
        ],
        None,
    )
    .unwrap();
    assert_eq!(hold.evaluate(at(9, timebase)).unwrap().components(), &[3.0]);
    assert_eq!(
        hold.evaluate(at(10, timebase)).unwrap().components(),
        &[9.0]
    );

    let cubic = AnimationCurve::new(
        timebase,
        [
            key(
                KeyframeTiming::Fixed(at(0, timebase)),
                &[0.0],
                Interpolation::Cubic,
                Easing::Linear,
                None,
                Some(tangent(&[20.0])),
            ),
            key(
                KeyframeTiming::Fixed(at(10, timebase)),
                &[10.0],
                Interpolation::Linear,
                Easing::Linear,
                Some(tangent(&[0.0])),
                None,
            ),
        ],
        None,
    )
    .unwrap();
    assert_close(
        cubic.evaluate(at(5, timebase)).unwrap().components()[0],
        7.5,
    );
}

#[test]
fn cubic_bezier_easing_is_inspectable_deterministic_and_can_overshoot() {
    let timebase = clock(100);
    let ease = CubicEasing::new(0.42, 0.0, 0.58, 1.0).unwrap();
    assert_eq!(ease.control_points(), [0.42, 0.0, 0.58, 1.0]);
    let curve = AnimationCurve::new(
        timebase,
        [
            key(
                KeyframeTiming::Fixed(at(0, timebase)),
                &[0.0],
                Interpolation::Linear,
                Easing::CubicBezier(ease),
                None,
                None,
            ),
            fixed(100, timebase, &[1.0]),
        ],
        None,
    )
    .unwrap();
    assert_close(
        curve.evaluate(at(50, timebase)).unwrap().components()[0],
        0.5,
    );
    assert!(curve.evaluate(at(25, timebase)).unwrap().components()[0] < 0.25);

    let overshoot = CubicEasing::new(0.2, 2.0, 0.8, 2.0).unwrap();
    assert!(overshoot.map(0.5) > 1.0);
    assert_eq!(
        CubicEasing::new(-0.1, 0.0, 0.5, 1.0)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
}

#[test]
fn roving_keys_recompute_stable_distinct_times_from_authored_values() {
    let timebase = clock(10);
    let curve = AnimationCurve::new(
        timebase,
        [
            fixed(0, timebase, &[0.0]),
            roving(&[2.0]),
            roving(&[8.0]),
            fixed(10, timebase, &[10.0]),
        ],
        None,
    )
    .unwrap();
    assert_eq!(
        curve
            .resolved_times()
            .iter()
            .map(|time| time.value())
            .collect::<Vec<_>>(),
        [0, 2, 8, 10]
    );
    assert_eq!(curve.keyframes()[1].timing(), KeyframeTiming::Roving);

    let stationary = AnimationCurve::new(
        timebase,
        [
            fixed(0, timebase, &[4.0]),
            roving(&[4.0]),
            roving(&[4.0]),
            fixed(10, timebase, &[4.0]),
        ],
        None,
    )
    .unwrap();
    assert_eq!(
        stationary
            .resolved_times()
            .iter()
            .map(|time| time.value())
            .collect::<Vec<_>>(),
        [0, 3, 7, 10]
    );

    let too_narrow = AnimationCurve::new(
        timebase,
        [
            fixed(0, timebase, &[0.0]),
            roving(&[1.0]),
            roving(&[2.0]),
            fixed(2, timebase, &[3.0]),
        ],
        None,
    )
    .unwrap_err();
    assert_eq!(reason(&too_narrow), Some("roving_span_too_narrow"));
}

#[test]
fn time_expressions_use_only_bounded_time_and_base_value_inputs() {
    let timebase = clock(1);
    let expression = TimeExpression::compile("value * 2 + time").unwrap();
    assert_eq!(expression.source(), "value * 2 + time");
    assert_eq!(expression.variables(), ["time", "value"]);
    let curve = AnimationCurve::new(
        timebase,
        [fixed(0, timebase, &[2.0, 3.0])],
        Some(expression),
    )
    .unwrap();
    assert_eq!(
        curve.evaluate(at(5, timebase)).unwrap().components(),
        &[9.0, 11.0]
    );

    let host_escape = TimeExpression::compile("system(1)").unwrap_err();
    assert_eq!(host_escape.category(), ErrorCategory::InvalidInput);

    let division = AnimationCurve::new(
        timebase,
        [fixed(0, timebase, &[2.0])],
        Some(TimeExpression::compile("value / (time - time)").unwrap()),
    )
    .unwrap();
    assert_eq!(
        reason(&division.evaluate(at(1, timebase)).unwrap_err()),
        Some("division_by_zero")
    );
}

#[test]
fn immutable_edits_revalidate_and_retiming_preserves_authored_intent() {
    let timebase = clock(10);
    let curve = AnimationCurve::new(
        timebase,
        [
            key(
                KeyframeTiming::Fixed(at(0, timebase)),
                &[0.0],
                Interpolation::Cubic,
                Easing::Linear,
                None,
                Some(tangent(&[10.0])),
            ),
            roving(&[5.0]),
            key(
                KeyframeTiming::Fixed(at(10, timebase)),
                &[10.0],
                Interpolation::Linear,
                Easing::Linear,
                Some(tangent(&[10.0])),
                None,
            ),
        ],
        None,
    )
    .unwrap();

    let retimed = curve.retimed(at(20, timebase), at(40, timebase)).unwrap();
    assert_eq!(
        retimed
            .resolved_times()
            .iter()
            .map(|time| time.value())
            .collect::<Vec<_>>(),
        [20, 30, 40]
    );
    assert_close(
        retimed.keyframes()[0]
            .outgoing_tangent()
            .unwrap()
            .components_per_second()[0],
        5.0,
    );
    assert_close(
        retimed.evaluate(at(30, timebase)).unwrap().components()[0],
        5.0,
    );

    let base = AnimationCurve::new(
        timebase,
        [fixed(0, timebase, &[0.0]), fixed(10, timebase, &[10.0])],
        None,
    )
    .unwrap();
    let inserted = base.with_inserted(1, roving(&[4.0])).unwrap();
    let replaced = inserted.with_replaced(1, roving(&[6.0])).unwrap();
    let expression = TimeExpression::compile("value + time").unwrap();
    let expressed = replaced.with_expression(Some(expression)).unwrap();
    assert_eq!(base.keyframes().len(), 2);
    assert_eq!(expressed.keyframes()[1].value().components(), &[6.0]);
    assert_eq!(expressed.without(1).unwrap().keyframes().len(), 2);

    let inexact = AnimationCurve::new(
        timebase,
        [
            fixed(0, timebase, &[0.0]),
            fixed(1, timebase, &[1.0]),
            fixed(3, timebase, &[3.0]),
        ],
        None,
    )
    .unwrap()
    .retimed(at(0, timebase), at(2, timebase))
    .unwrap_err();
    assert_eq!(reason(&inexact), Some("inexact_retime"));
}

#[test]
fn invalid_authored_state_is_rejected_before_evaluation_or_persistence() {
    let timebase = clock(24);
    assert_eq!(
        reason(&AnimationValue::new([]).unwrap_err()),
        Some("empty_value")
    );
    assert_eq!(
        reason(&AnimationValue::new([f64::NAN]).unwrap_err()),
        Some("nonfinite_value")
    );

    let roving_endpoint =
        AnimationCurve::new(timebase, [roving(&[0.0]), fixed(2, timebase, &[1.0])], None)
            .unwrap_err();
    assert_eq!(reason(&roving_endpoint), Some("roving_endpoint"));

    let mismatched_components = AnimationCurve::new(
        timebase,
        [fixed(0, timebase, &[0.0]), fixed(1, timebase, &[1.0, 2.0])],
        None,
    )
    .unwrap_err();
    assert_eq!(
        reason(&mismatched_components),
        Some("component_count_mismatch")
    );

    let wrong_clock = AnimationCurve::new(
        timebase,
        [fixed(0, timebase, &[0.0]), fixed(1, clock(25), &[1.0])],
        None,
    )
    .unwrap_err();
    assert_eq!(reason(&wrong_clock), Some("timebase_mismatch"));

    let unordered = AnimationCurve::new(
        timebase,
        [fixed(1, timebase, &[0.0]), fixed(1, timebase, &[1.0])],
        None,
    )
    .unwrap_err();
    assert_eq!(reason(&unordered), Some("fixed_time_not_increasing"));

    let tangent_dimensions = Keyframe::new(
        KeyframeTiming::Fixed(at(0, timebase)),
        value(&[0.0, 1.0]),
        Interpolation::Cubic,
        Easing::Linear,
        None,
        Some(tangent(&[1.0])),
    )
    .unwrap_err();
    assert_eq!(
        reason(&tangent_dimensions),
        Some("tangent_component_count_mismatch")
    );
}

fn value_type(input: &str) -> ValueTypeId {
    ValueTypeId::from_str(input).unwrap()
}

fn parameter(input: &str) -> ParameterName {
    ParameterName::new(input).unwrap()
}

fn animation_node(curve: AnimationCurve) -> EditableNode<AnimationCurve> {
    let animation_type = value_type("superi.value.animation_curve");
    let definition = EffectNodeDefinition::new(
        NodeSchemaId::new(
            NodeTypeId::from_str("superi.effects.animation_parameter").unwrap(),
            SemanticVersion::new(1, 0, 0),
        ),
        EffectMetadata::new(
            "Animation Parameter",
            "Stores one exact editable property animation.",
            "Animation",
        )
        .unwrap(),
        [],
        [],
        [EffectParameterDefinition::new(
            ParameterSchema::new(parameter("animation"), animation_type.clone(), true),
            "Animation",
            "Directly editable keyframes, interpolation, easing, tangents, and expressions.",
            ParameterControl::Automatic,
            TypedParameterValue::new(animation_type, curve),
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
        .parameter(&parameter("animation"))
        .unwrap()
        .schema()
        .is_animatable());
    definition
        .instantiate(
            EffectInstanceBindings::new(
                [],
                [],
                [EffectParameterBinding::new(
                    ParameterId::from_raw(101),
                    parameter("animation"),
                )],
            ),
            [],
        )
        .unwrap()
}

#[test]
fn animation_remains_editable_and_stable_through_real_graph_reload() {
    let timebase = clock(10);
    let curve = AnimationCurve::new(
        timebase,
        [
            key(
                KeyframeTiming::Fixed(at(0, timebase)),
                &[0.0, 10.0],
                Interpolation::Cubic,
                Easing::CubicBezier(CubicEasing::new(0.42, 0.0, 0.58, 1.0).unwrap()),
                None,
                Some(tangent(&[10.0, 10.0])),
            ),
            roving(&[5.0, 15.0]),
            key(
                KeyframeTiming::Fixed(at(10, timebase)),
                &[10.0, 20.0],
                Interpolation::Linear,
                Easing::Linear,
                Some(tangent(&[10.0, 10.0])),
                None,
            ),
        ],
        Some(TimeExpression::compile("value + time").unwrap()),
    )
    .unwrap();

    let before = curve.evaluate(at(4, timebase)).unwrap();
    let mut graph = EditableGraph::new(GraphId::from_raw(90));
    graph
        .apply(GraphTransaction::with_mutations(
            0,
            [GraphMutation::Add {
                node_id: NodeId::from_raw(1),
                node: animation_node(curve),
                position: 0,
            }],
        ))
        .unwrap();

    let bytes = serialize_graph(&graph.snapshot()).unwrap();
    let loaded = deserialize_graph::<AnimationCurve>(&bytes).unwrap();
    let loaded_snapshot = loaded.graph().snapshot();
    let reloaded = loaded_snapshot
        .node(NodeId::from_raw(1))
        .unwrap()
        .parameter(ParameterId::from_raw(101))
        .unwrap()
        .value()
        .payload();

    assert_eq!(reloaded.keyframes()[1].timing(), KeyframeTiming::Roving);
    assert_eq!(reloaded.expression().unwrap().source(), "value + time");
    assert_eq!(reloaded.evaluate(at(4, timebase)).unwrap(), before);
    assert_eq!(serialize_graph(&loaded.graph().snapshot()).unwrap(), bytes);
}

#[test]
fn standalone_animation_wire_is_versioned_strict_and_checked_on_reload() {
    let timebase = clock(10);
    let curve = AnimationCurve::new(
        timebase,
        [fixed(0, timebase, &[0.0]), fixed(10, timebase, &[10.0])],
        Some(TimeExpression::compile("value + time").unwrap()),
    )
    .unwrap();
    let document = serde_json::to_value(&curve).unwrap();
    assert_eq!(document["schema_revision"], 1);
    assert_eq!(
        serde_json::from_value::<AnimationCurve>(document.clone()).unwrap(),
        curve
    );

    let mut future = document.clone();
    future["schema_revision"] = 2.into();
    assert!(serde_json::from_value::<AnimationCurve>(future).is_err());

    let mut unknown = document.clone();
    unknown["unexpected"] = true.into();
    assert!(serde_json::from_value::<AnimationCurve>(unknown).is_err());

    let mut invalid = document;
    invalid["keyframes"][0]["timing"] = serde_json::json!({ "kind": "roving" });
    assert!(serde_json::from_value::<AnimationCurve>(invalid).is_err());
}
