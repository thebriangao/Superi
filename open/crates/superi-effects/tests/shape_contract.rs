use std::str::FromStr;

use superi_core::color_space::ColorSpace;
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_core::time::{RationalTime, Timebase};
use superi_effects::authoring::{
    EffectInstanceBindings, EffectMetadata, EffectNodeDefinition, EffectParameterBinding,
    EffectParameterDefinition, ParameterControl,
};
use superi_effects::control::{ControlRelationship, ParameterControlRig, ReusableControl};
use superi_effects::keyframe::{
    AnimationCurve, AnimationValue, Easing, Interpolation, Keyframe, KeyframeTiming,
};
use superi_effects::shape::{
    FillRule, FillStyle, GradientGeometry, GradientPaint, GradientSpread, GradientStopAnimation,
    LineCap, LineJoin, Paint, PathAnimation, PathVertexAnimation, RepeaterComposite,
    RepeaterTransformAnimation, ShapeColorAnimation, ShapeRepeater, StrokeStyle, VectorShape,
    VectorShapeDocument,
};
use superi_graph::expr::{ParameterAddress, ParameterReference};
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

fn clock() -> Timebase {
    Timebase::integer(24).unwrap()
}

fn at(frame: i64) -> RationalTime {
    RationalTime::new(frame, clock())
}

fn curve(start: &[f64], end: &[f64]) -> AnimationCurve {
    AnimationCurve::new(
        clock(),
        [
            Keyframe::new(
                KeyframeTiming::Fixed(at(0)),
                AnimationValue::new(start.iter().copied()).unwrap(),
                Interpolation::Linear,
                Easing::Linear,
                None,
                None,
            )
            .unwrap(),
            Keyframe::new(
                KeyframeTiming::Fixed(at(10)),
                AnimationValue::new(end.iter().copied()).unwrap(),
                Interpolation::Linear,
                Easing::Linear,
                None,
                None,
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap()
}

fn vertex(start: [f64; 6], end: [f64; 6]) -> PathVertexAnimation {
    PathVertexAnimation::new(curve(&start, &end)).unwrap()
}

fn constant(values: &[f64]) -> AnimationCurve {
    curve(values, values)
}

fn held_scalar(start: f64, end: f64) -> AnimationCurve {
    AnimationCurve::new(
        clock(),
        [
            Keyframe::new(
                KeyframeTiming::Fixed(at(0)),
                AnimationValue::scalar(start).unwrap(),
                Interpolation::Hold,
                Easing::Linear,
                None,
                None,
            )
            .unwrap(),
            Keyframe::new(
                KeyframeTiming::Fixed(at(10)),
                AnimationValue::scalar(end).unwrap(),
                Interpolation::Hold,
                Easing::Linear,
                None,
                None,
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap()
}

fn assert_near(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() < 1.0e-12,
        "{actual} != {expected}"
    );
}

fn color(values: [f64; 4]) -> ShapeColorAnimation {
    ShapeColorAnimation::new(constant(&values)).unwrap()
}

fn stop(offset: f64, values: [f64; 4]) -> GradientStopAnimation {
    GradientStopAnimation::new(constant(&[offset]), color(values)).unwrap()
}

fn basic_path() -> PathAnimation {
    PathAnimation::new(
        true,
        [
            vertex(
                [0.0, 0.0, 0.0, 0.0, 2.0, 0.0],
                [10.0, 0.0, 0.0, 0.0, 2.0, 0.0],
            ),
            vertex(
                [20.0, 0.0, -2.0, 0.0, 0.0, 2.0],
                [20.0, 10.0, -2.0, 0.0, 0.0, 2.0],
            ),
            vertex(
                [20.0, 20.0, 0.0, -2.0, -2.0, 0.0],
                [10.0, 20.0, 0.0, -2.0, -2.0, 0.0],
            ),
        ],
    )
    .unwrap()
}

fn basic_shape() -> VectorShape {
    VectorShape::new(
        basic_path(),
        Some(
            FillStyle::new(
                FillRule::NonZero,
                Paint::Solid(color([0.2, 0.4, 0.8, 1.0])),
                constant(&[1.0]),
            )
            .unwrap(),
        ),
        Some(
            StrokeStyle::new(
                Paint::Solid(color([1.0, 1.0, 1.0, 1.0])),
                constant(&[1.0]),
                constant(&[2.0]),
                LineCap::Round,
                LineJoin::Round,
                constant(&[4.0]),
                [],
                constant(&[0.0]),
            )
            .unwrap(),
        ),
        None,
    )
    .unwrap()
}

fn document() -> VectorShapeDocument {
    VectorShapeDocument::new(clock(), [basic_shape()]).unwrap()
}

#[test]
fn stable_cubic_path_topology_samples_edits_and_retimes_exactly() {
    let moving = vertex(
        [0.0, 0.0, -1.0, 0.0, 1.0, 0.0],
        [10.0, 0.0, -1.0, 0.0, 1.0, 0.0],
    );
    let second = vertex(
        [20.0, 0.0, -2.0, 0.0, 2.0, 0.0],
        [20.0, 10.0, -2.0, 0.0, 2.0, 0.0],
    );
    let third = vertex(
        [20.0, 20.0, 0.0, -2.0, 0.0, 2.0],
        [10.0, 20.0, 0.0, -2.0, 0.0, 2.0],
    );
    let path = PathAnimation::new(true, [moving.clone(), second, third]).unwrap();

    let sampled = path.sample(at(5)).unwrap();
    assert!(sampled.is_closed());
    assert_eq!(sampled.vertices().len(), 3);
    assert_eq!(sampled.vertices()[0].anchor().x(), 5.0);
    assert_eq!(sampled.vertices()[1].anchor().y(), 5.0);
    assert_eq!(sampled.segments().len(), 3);
    assert_eq!(sampled.segments()[0].control_1().x(), 6.0);
    assert_eq!(sampled.segments()[0].control_2().x(), 18.0);

    let replaced = path
        .with_vertex_replaced(
            0,
            vertex(
                [100.0, 0.0, 0.0, 0.0, 0.0, 0.0],
                [100.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            ),
        )
        .unwrap();
    assert_eq!(path.sample(at(5)).unwrap().vertices()[0].anchor().x(), 5.0);
    assert_eq!(
        replaced.sample(at(5)).unwrap().vertices()[0].anchor().x(),
        100.0
    );

    let retimed = path.retimed(at(0), at(20)).unwrap();
    assert_eq!(retimed.sample(at(10)).unwrap(), sampled);
    assert_eq!(retimed.vertices().len(), path.vertices().len());
    assert_eq!(
        retimed.vertices()[0].curve().resolved_times(),
        &[at(0), at(20)]
    );

    assert!(PathAnimation::new(false, [moving.clone()]).is_err());
    assert!(PathAnimation::new(true, [moving.clone(), moving]).is_err());
}

#[test]
fn fills_and_gradients_sample_scene_linear_color_with_explicit_spread() {
    let stops = [
        stop(0.0, [0.0, 0.0, 0.0, 1.0]),
        stop(0.5, [1.0, 0.0, 0.0, 1.0]),
        stop(0.5, [0.0, 1.0, 0.0, 1.0]),
        stop(1.0, [0.0, 0.0, 1.0, 1.0]),
    ];
    let geometry = GradientGeometry::linear(constant(&[0.0, 0.0]), constant(&[10.0, 0.0])).unwrap();

    let pad = GradientPaint::new(geometry.clone(), stops.clone(), GradientSpread::Pad).unwrap();
    let pad_sample = pad.sample(at(5)).unwrap();
    assert_eq!(
        pad_sample
            .color_at(superi_core::geometry::Point2::new(5.0, 0.0).unwrap())
            .unwrap()
            .components(),
        [0.0, 1.0, 0.0, 1.0]
    );
    assert_eq!(
        pad_sample
            .color_at(superi_core::geometry::Point2::new(-5.0, 0.0).unwrap())
            .unwrap()
            .components(),
        [0.0, 0.0, 0.0, 1.0]
    );

    let repeat = GradientPaint::new(geometry.clone(), stops.clone(), GradientSpread::Repeat)
        .unwrap()
        .sample(at(5))
        .unwrap();
    assert_eq!(
        repeat
            .color_at(superi_core::geometry::Point2::new(12.5, 0.0).unwrap())
            .unwrap()
            .components(),
        [0.5, 0.0, 0.0, 1.0]
    );

    let reflect = GradientPaint::new(geometry, stops, GradientSpread::Reflect)
        .unwrap()
        .sample(at(5))
        .unwrap();
    assert_eq!(
        reflect
            .color_at(superi_core::geometry::Point2::new(12.5, 0.0).unwrap())
            .unwrap()
            .components(),
        [0.0, 0.5, 0.5, 1.0]
    );

    let fill = FillStyle::new(FillRule::EvenOdd, Paint::Gradient(pad), constant(&[0.5]))
        .unwrap()
        .sample(at(5))
        .unwrap();
    assert_eq!(fill.rule(), FillRule::EvenOdd);
    assert_eq!(fill.opacity(), 0.5);
    assert_eq!(
        fill.paint().gradient().unwrap().spread(),
        GradientSpread::Pad
    );

    assert!(ShapeColorAnimation::new(constant(&[1.0, 1.0, 1.0, 2.0])).is_err());
    assert!(GradientGeometry::radial(constant(&[0.0, 0.0]), constant(&[0.0]),).is_err());
}

#[test]
fn strokes_retain_geometry_choices_and_execute_svg_dash_phase() {
    let stroke = StrokeStyle::new(
        Paint::Solid(color([0.25, 0.5, 1.0, 1.0])),
        constant(&[0.75]),
        constant(&[4.0]),
        LineCap::Round,
        LineJoin::Bevel,
        constant(&[4.0]),
        [constant(&[2.0]), constant(&[1.0]), constant(&[3.0])],
        constant(&[0.0]),
    )
    .unwrap()
    .sample(at(5))
    .unwrap();

    assert_eq!(stroke.opacity(), 0.75);
    assert_eq!(stroke.width(), 4.0);
    assert_eq!(stroke.cap(), LineCap::Round);
    assert_eq!(stroke.join(), LineJoin::Bevel);
    assert_eq!(stroke.miter_limit(), 4.0);
    assert_eq!(stroke.dash_pattern(), &[2.0, 1.0, 3.0, 2.0, 1.0, 3.0]);
    assert!(stroke.is_dash_on(0.0).unwrap());
    assert!(!stroke.is_dash_on(2.5).unwrap());
    assert!(stroke.is_dash_on(3.5).unwrap());

    let solid = StrokeStyle::new(
        Paint::Solid(color([1.0, 1.0, 1.0, 1.0])),
        constant(&[1.0]),
        constant(&[1.0]),
        LineCap::Butt,
        LineJoin::Miter,
        constant(&[1.0]),
        [constant(&[0.0]), constant(&[0.0])],
        constant(&[100.0]),
    )
    .unwrap()
    .sample(at(5))
    .unwrap();
    assert!(solid.is_dash_on(1_000.0).unwrap());
    assert!(solid.is_dash_on(-1.0).is_err());

    assert!(StrokeStyle::new(
        Paint::Solid(color([1.0, 1.0, 1.0, 1.0])),
        constant(&[1.0]),
        constant(&[-1.0]),
        LineCap::Square,
        LineJoin::Round,
        constant(&[1.0]),
        [],
        constant(&[0.0]),
    )
    .is_err());
}

#[test]
fn repeater_emits_bounded_fractional_transform_copies_and_opacity() {
    let transform = RepeaterTransformAnimation::new(
        constant(&[0.0, 0.0]),
        constant(&[10.0, 0.0]),
        constant(&[2.0, 1.0]),
        constant(&[90.0]),
    )
    .unwrap();
    let repeater = ShapeRepeater::new(
        held_scalar(3.0, 3.0),
        constant(&[0.5]),
        transform,
        constant(&[1.0]),
        constant(&[0.5]),
        RepeaterComposite::Below,
    )
    .unwrap();
    let sample = repeater.sample(at(5)).unwrap();

    assert_eq!(sample.copy_count(), 3);
    assert_eq!(sample.composite(), RepeaterComposite::Below);
    assert_eq!(sample.copies().len(), 3);
    assert_eq!(sample.copies()[0].index(), 0);
    assert_eq!(sample.copies()[0].exponent(), 0.5);
    assert_eq!(sample.copies()[0].opacity(), 1.0);
    assert_eq!(sample.copies()[1].opacity(), 0.75);
    assert_eq!(sample.copies()[2].opacity(), 0.5);

    let mapped = sample.copies()[0]
        .transform()
        .checked_transform_point(superi_core::geometry::Point2::new(1.0, 0.0).unwrap())
        .unwrap();
    assert_near(mapped.x(), 6.0);
    assert_near(mapped.y(), 1.0);

    let retimed = repeater.retimed(at(10), at(30)).unwrap();
    assert_eq!(retimed.sample(at(20)).unwrap(), sample);

    assert!(ShapeRepeater::new(
        constant(&[3.0]),
        constant(&[0.0]),
        RepeaterTransformAnimation::new(
            constant(&[0.0, 0.0]),
            constant(&[0.0, 0.0]),
            constant(&[1.0, 1.0]),
            constant(&[0.0]),
        )
        .unwrap(),
        constant(&[1.0]),
        constant(&[1.0]),
        RepeaterComposite::Above,
    )
    .is_err());
    assert!(RepeaterTransformAnimation::new(
        constant(&[0.0, 0.0]),
        constant(&[0.0, 0.0]),
        constant(&[0.0, 1.0]),
        constant(&[0.0]),
    )
    .is_err());
    assert!(ShapeRepeater::new(
        held_scalar(4_097.0, 4_097.0),
        constant(&[0.0]),
        RepeaterTransformAnimation::new(
            constant(&[0.0, 0.0]),
            constant(&[0.0, 0.0]),
            constant(&[1.0, 1.0]),
            constant(&[0.0]),
        )
        .unwrap(),
        constant(&[1.0]),
        constant(&[1.0]),
        RepeaterComposite::Above,
    )
    .is_err());
}

#[test]
fn vector_document_retimes_edits_and_round_trips_strictly() {
    let original = document();
    let sampled = original.sample(at(5)).unwrap();
    assert_eq!(sampled.shapes().len(), 1);
    assert_eq!(sampled.shapes()[0].path().segments().len(), 3);
    assert!(sampled.shapes()[0].fill().is_some());
    assert!(sampled.shapes()[0].stroke().is_some());

    let edited = original.with_shape_inserted(1, basic_shape()).unwrap();
    assert_eq!(original.shapes().len(), 1);
    assert_eq!(edited.shapes().len(), 2);
    let retimed = edited.retimed(at(10), at(30)).unwrap();
    assert_eq!(
        retimed.sample(at(20)).unwrap().shapes(),
        edited.sample(at(5)).unwrap().shapes()
    );

    let bytes = serde_json::to_vec(&edited).unwrap();
    let reloaded = serde_json::from_slice::<VectorShapeDocument>(&bytes).unwrap();
    assert_eq!(reloaded, edited);
    assert_eq!(serde_json::to_vec(&reloaded).unwrap(), bytes);

    let mut future = serde_json::to_value(&edited).unwrap();
    future["schema_revision"] = 2.into();
    assert!(serde_json::from_value::<VectorShapeDocument>(future).is_err());

    let mut unknown = serde_json::to_value(&edited).unwrap();
    unknown["unexpected"] = true.into();
    assert!(serde_json::from_value::<VectorShapeDocument>(unknown).is_err());
}

#[test]
fn visual_operation_families_edit_immutably_without_losing_animation() {
    let gradient = GradientPaint::new(
        GradientGeometry::linear(constant(&[0.0, 0.0]), constant(&[10.0, 0.0])).unwrap(),
        [
            stop(0.0, [0.0, 0.0, 0.0, 1.0]),
            stop(1.0, [1.0, 0.0, 0.0, 1.0]),
        ],
        GradientSpread::Pad,
    )
    .unwrap();
    let edited_gradient = gradient
        .with_stop_replaced(1, stop(1.0, [0.0, 0.0, 1.0, 1.0]))
        .unwrap()
        .with_spread(GradientSpread::Reflect)
        .unwrap();
    assert_eq!(gradient.spread(), GradientSpread::Pad);
    assert_eq!(edited_gradient.spread(), GradientSpread::Reflect);
    assert_eq!(
        gradient
            .sample(at(5))
            .unwrap()
            .color_at(superi_core::geometry::Point2::new(10.0, 0.0).unwrap())
            .unwrap()
            .components(),
        [1.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(
        edited_gradient
            .sample(at(5))
            .unwrap()
            .color_at(superi_core::geometry::Point2::new(10.0, 0.0).unwrap())
            .unwrap()
            .components(),
        [0.0, 0.0, 1.0, 1.0]
    );

    let fill = FillStyle::new(
        FillRule::NonZero,
        Paint::Gradient(gradient),
        constant(&[1.0]),
    )
    .unwrap();
    let edited_fill = fill
        .with_rule(FillRule::EvenOdd)
        .unwrap()
        .with_opacity(constant(&[0.25]))
        .unwrap();
    assert_eq!(fill.sample(at(5)).unwrap().opacity(), 1.0);
    assert_eq!(edited_fill.sample(at(5)).unwrap().opacity(), 0.25);
    assert_eq!(edited_fill.rule(), FillRule::EvenOdd);

    let stroke = basic_shape().stroke().unwrap().clone();
    let edited_stroke = stroke
        .with_width(constant(&[8.0]))
        .unwrap()
        .with_cap(LineCap::Square)
        .unwrap()
        .with_dash_pattern([constant(&[4.0]), constant(&[2.0])])
        .unwrap();
    assert_eq!(stroke.sample(at(5)).unwrap().width(), 2.0);
    assert_eq!(edited_stroke.sample(at(5)).unwrap().width(), 8.0);
    assert_eq!(edited_stroke.cap(), LineCap::Square);

    let transform = RepeaterTransformAnimation::new(
        constant(&[0.0, 0.0]),
        constant(&[1.0, 0.0]),
        constant(&[1.0, 1.0]),
        constant(&[0.0]),
    )
    .unwrap();
    let edited_transform = transform.with_position(constant(&[3.0, 0.0])).unwrap();
    let repeater = ShapeRepeater::new(
        held_scalar(2.0, 2.0),
        constant(&[0.0]),
        transform,
        constant(&[1.0]),
        constant(&[1.0]),
        RepeaterComposite::Above,
    )
    .unwrap();
    let edited_repeater = repeater
        .with_transform(edited_transform)
        .unwrap()
        .with_offset(constant(&[1.0]))
        .unwrap()
        .with_composite(RepeaterComposite::Below)
        .unwrap();
    assert_eq!(repeater.sample(at(5)).unwrap().copies()[1].exponent(), 1.0);
    assert_eq!(
        edited_repeater.sample(at(5)).unwrap().copies()[1].exponent(),
        2.0
    );
    assert_eq!(edited_repeater.composite(), RepeaterComposite::Below);
}

fn value_type(input: &str) -> ValueTypeId {
    ValueTypeId::from_str(input).unwrap()
}

fn parameter(input: &str) -> ParameterName {
    ParameterName::new(input).unwrap()
}

fn shape_node(
    document: VectorShapeDocument,
    parameter_id: ParameterId,
) -> EditableNode<GraphValue<VectorShapeDocument>> {
    let shape_type = value_type("superi.value.vector_shape_document");
    let definition = EffectNodeDefinition::new(
        NodeSchemaId::new(
            NodeTypeId::from_str("superi.effects.vector_shape_parameter").unwrap(),
            SemanticVersion::new(1, 0, 0),
        ),
        EffectMetadata::new(
            "Vector Shape Parameter",
            "Stores one reusable editable vector-shape document.",
            "Shape",
        )
        .unwrap(),
        [],
        [],
        [EffectParameterDefinition::new(
            ParameterSchema::new(parameter("shapes"), shape_type.clone(), true),
            "Shapes",
            "Animated paths and complete visual-operation controls.",
            ParameterControl::Automatic,
            TypedParameterValue::new(shape_type, GraphValue::domain(document)),
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
    definition
        .instantiate(
            EffectInstanceBindings::new(
                [],
                [],
                [EffectParameterBinding::new(
                    parameter_id,
                    parameter("shapes"),
                )],
            ),
            [],
        )
        .unwrap()
}

#[test]
fn one_shape_payload_survives_timeline_and_node_graph_reuse_and_reload() {
    let initial = document();
    let edited = initial.with_shape_inserted(1, basic_shape()).unwrap();
    let shape_type = value_type("superi.value.vector_shape_document");

    for (graph_id, source_node_id, source_parameter_id, target_node_id, target_parameter_id) in [
        (
            GraphId::from_raw(700),
            NodeId::from_raw(701),
            ParameterId::from_raw(702),
            NodeId::from_raw(703),
            ParameterId::from_raw(704),
        ),
        (
            GraphId::from_raw(800),
            NodeId::from_raw(801),
            ParameterId::from_raw(802),
            NodeId::from_raw(803),
            ParameterId::from_raw(804),
        ),
    ] {
        let mut graph = EditableGraph::new(graph_id);
        graph
            .apply(GraphTransaction::with_mutations(
                0,
                [
                    GraphMutation::Add {
                        node_id: source_node_id,
                        node: shape_node(initial.clone(), source_parameter_id),
                        position: 0,
                    },
                    GraphMutation::Add {
                        node_id: target_node_id,
                        node: shape_node(initial.clone(), target_parameter_id),
                        position: 1,
                    },
                ],
            ))
            .unwrap();
        graph
            .apply(GraphTransaction::with_mutations(
                1,
                [GraphMutation::SetParameter {
                    node_id: source_node_id,
                    parameter_id: source_parameter_id,
                    value: TypedParameterValue::new(
                        shape_type.clone(),
                        GraphValue::domain(edited.clone()),
                    ),
                }],
            ))
            .unwrap();

        let source = ParameterAddress::new(source_node_id, source_parameter_id);
        let target = ParameterAddress::new(target_node_id, target_parameter_id);
        let control_name = parameter("shared-shapes");
        let rig = ParameterControlRig::new(
            [ReusableControl::new(
                control_name.clone(),
                "Shared Shapes",
                "Reuses one complete editable vector-shape document.",
                ParameterControl::Automatic,
                ParameterReference::new(source, shape_type.clone()),
            )
            .unwrap()],
            [ControlRelationship::link(target, control_name)],
        )
        .unwrap();
        graph
            .apply(rig.transaction(&graph.snapshot()).unwrap())
            .unwrap();

        let expected = graph
            .snapshot()
            .evaluate_parameter(target)
            .unwrap()
            .value()
            .payload()
            .as_domain()
            .unwrap()
            .sample(at(5))
            .unwrap();
        let bytes = serialize_graph(&graph.snapshot()).unwrap();
        let loaded = deserialize_graph::<GraphValue<VectorShapeDocument>>(&bytes).unwrap();
        let loaded_snapshot = loaded.graph().snapshot();
        let reloaded = loaded_snapshot
            .evaluate_parameter(target)
            .unwrap()
            .value()
            .payload()
            .as_domain()
            .unwrap()
            .sample(at(5))
            .unwrap();

        assert_eq!(reloaded, expected);
        assert_eq!(serialize_graph(&loaded.graph().snapshot()).unwrap(), bytes);
    }
}
