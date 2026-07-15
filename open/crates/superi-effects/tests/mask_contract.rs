use std::str::FromStr;

use superi_core::color_space::ColorSpace;
use superi_core::geometry::{Point2, Vector2};
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_core::time::{RationalTime, Timebase};
use superi_effects::authoring::{
    EffectInstanceBindings, EffectMetadata, EffectNodeDefinition, EffectParameterBinding,
    EffectParameterDefinition, ParameterControl,
};
use superi_effects::control::{ControlRelationship, ParameterControlRig, ReusableControl};
use superi_effects::keyframe::{
    AnimationCurve, AnimationValue, Easing, Interpolation, Keyframe, KeyframeTiming, TimeExpression,
};
use superi_effects::mask::{
    Mask, MaskBooleanOperation, MaskFillRule, MaskPathAnimation, MaskStack, MaskVertexAnimation,
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
    Timebase::integer(10).unwrap()
}

fn at(value: i64) -> RationalTime {
    RationalTime::new(value, clock())
}

fn key(time: i64, components: &[f64], interpolation: Interpolation) -> Keyframe {
    Keyframe::new(
        KeyframeTiming::Fixed(at(time)),
        AnimationValue::new(components.iter().copied()).unwrap(),
        interpolation,
        Easing::Linear,
        None,
        None,
    )
    .unwrap()
}

fn curve(start: f64, end: f64) -> AnimationCurve {
    AnimationCurve::new(
        clock(),
        [
            key(0, &[start], Interpolation::Linear),
            key(10, &[end], Interpolation::Linear),
        ],
        None,
    )
    .unwrap()
}

fn toggle(value: bool) -> AnimationCurve {
    let component = f64::from(u8::from(value));
    AnimationCurve::new(
        clock(),
        [
            key(0, &[component], Interpolation::Hold),
            key(10, &[component], Interpolation::Hold),
        ],
        None,
    )
    .unwrap()
}

fn vertex_curve(start: [f64; 6], end: [f64; 6]) -> MaskVertexAnimation {
    MaskVertexAnimation::new(
        AnimationCurve::new(
            clock(),
            [
                key(0, &start, Interpolation::Linear),
                key(10, &end, Interpolation::Linear),
            ],
            None,
        )
        .unwrap(),
    )
    .unwrap()
}

fn triangle() -> MaskPathAnimation {
    MaskPathAnimation::new(
        MaskFillRule::EvenOdd,
        [
            vertex_curve(
                [0.0, 0.0, -1.0, 0.0, 1.0, 0.0],
                [2.0, 2.0, -1.0, 0.0, 1.0, 0.0],
            ),
            vertex_curve(
                [10.0, 0.0, -1.0, -1.0, 1.0, 1.0],
                [12.0, 2.0, -1.0, -1.0, 1.0, 1.0],
            ),
            vertex_curve(
                [5.0, 10.0, -1.0, 1.0, 1.0, -1.0],
                [7.0, 12.0, -1.0, 1.0, 1.0, -1.0],
            ),
        ],
    )
    .unwrap()
}

fn mask(operation: MaskBooleanOperation) -> Mask {
    Mask::new(
        triangle(),
        curve(0.0, 10.0),
        curve(-2.0, 2.0),
        curve(0.5, 1.0),
        toggle(false),
        operation,
    )
    .unwrap()
}

fn assert_close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1.0e-9,
        "expected {expected}, got {actual}"
    );
}

#[test]
fn cubic_paths_and_every_mask_control_are_sampled_and_inspectable() {
    let path = triangle();
    assert_eq!(path.fill_rule(), MaskFillRule::EvenOdd);
    assert_eq!(path.vertices().len(), 3);
    assert_eq!(path.timebase(), clock());

    let sampled_path = path.sample(at(5)).unwrap();
    assert_eq!(sampled_path.fill_rule(), MaskFillRule::EvenOdd);
    assert_eq!(sampled_path.vertices().len(), 3);
    assert_eq!(sampled_path.segments().len(), 3);
    assert_eq!(
        sampled_path.vertices()[0].anchor(),
        Point2::new(1.0, 1.0).unwrap()
    );
    assert_eq!(
        sampled_path.vertices()[0].incoming_handle(),
        Vector2::new(-1.0, 0.0).unwrap()
    );
    assert_eq!(
        sampled_path.vertices()[0].outgoing_handle(),
        Vector2::new(1.0, 0.0).unwrap()
    );
    assert_eq!(
        sampled_path.segments()[0].start(),
        Point2::new(1.0, 1.0).unwrap()
    );
    assert_eq!(
        sampled_path.segments()[0].control_1(),
        Point2::new(2.0, 1.0).unwrap()
    );
    assert_eq!(
        sampled_path.segments()[0].control_2(),
        Point2::new(10.0, 0.0).unwrap()
    );
    assert_eq!(
        sampled_path.segments()[0].end(),
        Point2::new(11.0, 1.0).unwrap()
    );
    assert_eq!(
        sampled_path.segments()[2].end(),
        sampled_path.segments()[0].start()
    );
    let nonzero_path =
        MaskPathAnimation::new(MaskFillRule::NonZero, path.vertices().iter().cloned()).unwrap();
    assert_eq!(
        nonzero_path.sample(at(5)).unwrap().fill_rule(),
        MaskFillRule::NonZero
    );

    let sampled = mask(MaskBooleanOperation::Union).sample(at(5)).unwrap();
    assert_eq!(sampled.operation(), MaskBooleanOperation::Union);
    assert_eq!(sampled.path(), &sampled_path);
    assert_close(sampled.feather_pixels(), 5.0);
    assert_close(sampled.expansion_pixels(), 0.0);
    assert_close(sampled.opacity(), 0.75);
    assert!(!sampled.is_inverted());

    let animated_inversion = AnimationCurve::new(
        clock(),
        [
            key(0, &[0.0], Interpolation::Hold),
            key(10, &[1.0], Interpolation::Hold),
        ],
        None,
    )
    .unwrap();
    let animated = Mask::new(
        triangle(),
        curve(0.0, 0.0),
        curve(0.0, 0.0),
        curve(1.0, 1.0),
        animated_inversion,
        MaskBooleanOperation::Replace,
    )
    .unwrap();
    assert!(!animated.sample(at(5)).unwrap().is_inverted());
    assert!(animated.sample(at(10)).unwrap().is_inverted());
}

#[test]
fn topology_and_stack_edits_are_immutable_and_checked() {
    let original_path = triangle();
    let inserted_vertex = vertex_curve(
        [2.0, 4.0, 0.0, 0.0, 0.0, 0.0],
        [4.0, 6.0, 0.0, 0.0, 0.0, 0.0],
    );
    let inserted = original_path
        .with_vertex_inserted(1, inserted_vertex.clone())
        .unwrap();
    assert_eq!(original_path.vertices().len(), 3);
    assert_eq!(inserted.vertices().len(), 4);
    assert_eq!(inserted.vertices()[1], inserted_vertex);

    let replacement = vertex_curve(
        [3.0, 5.0, 0.0, 0.0, 0.0, 0.0],
        [5.0, 7.0, 0.0, 0.0, 0.0, 0.0],
    );
    let replaced = inserted
        .with_vertex_replaced(1, replacement.clone())
        .unwrap();
    assert_eq!(inserted.vertices()[1], inserted_vertex);
    assert_eq!(replaced.vertices()[1], replacement);
    assert_eq!(replaced.with_vertex_removed(1).unwrap(), original_path);
    assert!(original_path.with_vertex_removed(0).is_err());
    assert!(original_path
        .with_vertex_inserted(4, replacement.clone())
        .is_err());
    assert!(original_path.with_vertex_replaced(3, replacement).is_err());

    let original_mask = mask(MaskBooleanOperation::Replace);
    let edited_path = original_mask
        .path()
        .with_fill_rule(MaskFillRule::NonZero)
        .unwrap();
    let edited_mask = original_mask
        .with_path(edited_path)
        .unwrap()
        .with_feather(curve(2.0, 4.0))
        .unwrap()
        .with_expansion(curve(5.0, 7.0))
        .unwrap()
        .with_opacity(curve(0.2, 0.4))
        .unwrap()
        .with_inversion(toggle(true))
        .unwrap()
        .with_operation(MaskBooleanOperation::Exclude)
        .unwrap();
    assert_eq!(original_mask.path().fill_rule(), MaskFillRule::EvenOdd);
    assert_eq!(original_mask.operation(), MaskBooleanOperation::Replace);
    assert_eq!(edited_mask.path().fill_rule(), MaskFillRule::NonZero);
    assert_eq!(edited_mask.feather(), &curve(2.0, 4.0));
    assert_eq!(edited_mask.expansion(), &curve(5.0, 7.0));
    assert_eq!(edited_mask.opacity(), &curve(0.2, 0.4));
    assert_eq!(edited_mask.inversion(), &toggle(true));
    assert_eq!(edited_mask.operation(), MaskBooleanOperation::Exclude);
    let edited_sample = edited_mask.sample(at(5)).unwrap();
    assert_close(edited_sample.feather_pixels(), 3.0);
    assert_close(edited_sample.expansion_pixels(), 6.0);
    assert_close(edited_sample.opacity(), 0.3);
    assert!(edited_sample.is_inverted());

    let original_stack = MaskStack::new([mask(MaskBooleanOperation::Replace)]).unwrap();
    let inserted_mask = mask(MaskBooleanOperation::Union);
    let inserted_stack = original_stack
        .with_mask_inserted(1, inserted_mask.clone())
        .unwrap();
    assert_eq!(original_stack.masks().len(), 1);
    assert_eq!(inserted_stack.masks().len(), 2);
    assert_eq!(inserted_stack.masks()[1], inserted_mask);
    let replacement_mask = mask(MaskBooleanOperation::Intersect);
    let replaced_stack = inserted_stack
        .with_mask_replaced(1, replacement_mask.clone())
        .unwrap();
    assert_eq!(inserted_stack.masks()[1], inserted_mask);
    assert_eq!(replaced_stack.masks()[1], replacement_mask);
    assert_eq!(replaced_stack.with_mask_removed(1).unwrap(), original_stack);
    assert!(original_stack.with_mask_removed(1).is_err());
}

#[test]
fn soft_coverage_boolean_operations_are_deterministic() {
    let base = Mask::new(
        triangle(),
        curve(0.0, 0.0),
        curve(0.0, 0.0),
        curve(1.0, 1.0),
        toggle(false),
        MaskBooleanOperation::Replace,
    )
    .unwrap();

    let expected = [
        (MaskBooleanOperation::Replace, 0.5),
        (MaskBooleanOperation::Union, 0.7),
        (MaskBooleanOperation::Subtract, 0.2),
        (MaskBooleanOperation::Intersect, 0.2),
        (MaskBooleanOperation::Exclude, 0.5),
    ];
    for (operation, result) in expected {
        let second = Mask::new(
            triangle(),
            curve(0.0, 0.0),
            curve(0.0, 0.0),
            curve(1.0, 1.0),
            toggle(false),
            operation,
        )
        .unwrap();
        let stack = MaskStack::new([base.clone(), second]).unwrap();
        let sample = stack.sample(at(5)).unwrap();
        assert_close(
            sample.compose_rasterized_coverages(&[0.4, 0.5]).unwrap(),
            result,
        );
    }

    let inverted = Mask::new(
        triangle(),
        curve(0.0, 0.0),
        curve(0.0, 0.0),
        curve(0.5, 0.5),
        toggle(true),
        MaskBooleanOperation::Replace,
    )
    .unwrap();
    let sample = MaskStack::new([inverted]).unwrap().sample(at(5)).unwrap();
    assert_close(sample.compose_rasterized_coverages(&[0.25]).unwrap(), 0.375);
    assert!(sample.compose_rasterized_coverages(&[]).is_err());
    assert!(sample.compose_rasterized_coverages(&[1.1]).is_err());
    assert_close(
        MaskStack::new([])
            .unwrap()
            .sample(at(5))
            .unwrap()
            .compose_rasterized_coverages(&[])
            .unwrap(),
        1.0,
    );
}

#[test]
fn illegal_controls_and_sampled_overshoot_fail_before_publication() {
    assert!(MaskVertexAnimation::new(curve(0.0, 1.0)).is_err());
    assert!(MaskPathAnimation::new(
        MaskFillRule::NonZero,
        triangle().vertices()[..2].iter().cloned(),
    )
    .is_err());

    assert!(Mask::new(
        triangle(),
        curve(-1.0, 0.0),
        curve(0.0, 0.0),
        curve(1.0, 1.0),
        toggle(false),
        MaskBooleanOperation::Replace,
    )
    .is_err());
    assert!(Mask::new(
        triangle(),
        curve(0.0, 0.0),
        curve(0.0, 0.0),
        curve(0.0, 1.1),
        toggle(false),
        MaskBooleanOperation::Replace,
    )
    .is_err());

    let expressive_opacity = AnimationCurve::new(
        clock(),
        [
            key(0, &[0.25], Interpolation::Linear),
            key(10, &[0.25], Interpolation::Linear),
        ],
        Some(TimeExpression::compile("value + time").unwrap()),
    )
    .unwrap();
    let mask = Mask::new(
        triangle(),
        curve(0.0, 0.0),
        curve(0.0, 0.0),
        expressive_opacity,
        toggle(false),
        MaskBooleanOperation::Replace,
    )
    .unwrap();
    assert!(mask.sample(at(10)).is_err());

    let invalid_toggle = AnimationCurve::new(
        clock(),
        [
            key(0, &[0.0], Interpolation::Linear),
            key(10, &[1.0], Interpolation::Linear),
        ],
        None,
    )
    .unwrap();
    assert!(Mask::new(
        triangle(),
        curve(0.0, 0.0),
        curve(0.0, 0.0),
        curve(1.0, 1.0),
        invalid_toggle,
        MaskBooleanOperation::Replace,
    )
    .is_err());
}

#[test]
fn standalone_wire_is_versioned_strict_and_revalidates_nested_state() {
    let stack = MaskStack::new([
        mask(MaskBooleanOperation::Replace),
        mask(MaskBooleanOperation::Exclude),
    ])
    .unwrap();
    let document = serde_json::to_value(&stack).unwrap();
    assert_eq!(document["schema_revision"], 1);
    assert_eq!(
        serde_json::from_value::<MaskStack>(document.clone()).unwrap(),
        stack
    );

    let mut future = document.clone();
    future["schema_revision"] = 2.into();
    assert!(serde_json::from_value::<MaskStack>(future).is_err());

    let mut unknown = document.clone();
    unknown["unexpected"] = true.into();
    assert!(serde_json::from_value::<MaskStack>(unknown).is_err());

    let mut invalid = document;
    invalid["masks"][0]["opacity"]["keyframes"][0]["value"][0] = 2.0.into();
    assert!(serde_json::from_value::<MaskStack>(invalid).is_err());
}

fn value_type(input: &str) -> ValueTypeId {
    ValueTypeId::from_str(input).unwrap()
}

fn parameter(input: &str) -> ParameterName {
    ParameterName::new(input).unwrap()
}

fn mask_node(stack: MaskStack, parameter_id: ParameterId) -> EditableNode<GraphValue<MaskStack>> {
    let mask_type = value_type("superi.value.mask_stack");
    let definition = EffectNodeDefinition::new(
        NodeSchemaId::new(
            NodeTypeId::from_str("superi.effects.mask_parameter").unwrap(),
            SemanticVersion::new(1, 0, 0),
        ),
        EffectMetadata::new(
            "Mask Parameter",
            "Stores one reusable editable mask stack.",
            "Mask",
        )
        .unwrap(),
        [],
        [],
        [EffectParameterDefinition::new(
            ParameterSchema::new(parameter("masks"), mask_type.clone(), true),
            "Masks",
            "Animated paths and complete composition controls.",
            ParameterControl::Automatic,
            TypedParameterValue::new(mask_type, GraphValue::domain(stack)),
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
        .parameter(&parameter("masks"))
        .unwrap()
        .schema()
        .is_animatable());
    definition
        .instantiate(
            EffectInstanceBindings::new(
                [],
                [],
                [EffectParameterBinding::new(
                    parameter_id,
                    parameter("masks"),
                )],
            ),
            [],
        )
        .unwrap()
}

#[test]
fn one_mask_payload_survives_reusable_timeline_and_node_graph_editing_and_reload() {
    let initial = MaskStack::new([mask(MaskBooleanOperation::Replace)]).unwrap();
    let edited = initial
        .with_mask_inserted(1, mask(MaskBooleanOperation::Union))
        .unwrap();
    let mask_type = value_type("superi.value.mask_stack");

    for (graph_id, source_node_id, source_parameter_id, target_node_id, target_parameter_id) in [
        (
            GraphId::from_raw(500),
            NodeId::from_raw(501),
            ParameterId::from_raw(502),
            NodeId::from_raw(503),
            ParameterId::from_raw(504),
        ),
        (
            GraphId::from_raw(600),
            NodeId::from_raw(601),
            ParameterId::from_raw(602),
            NodeId::from_raw(603),
            ParameterId::from_raw(604),
        ),
    ] {
        let mut graph = EditableGraph::new(graph_id);
        graph
            .apply(GraphTransaction::with_mutations(
                0,
                [
                    GraphMutation::Add {
                        node_id: source_node_id,
                        node: mask_node(initial.clone(), source_parameter_id),
                        position: 0,
                    },
                    GraphMutation::Add {
                        node_id: target_node_id,
                        node: mask_node(initial.clone(), target_parameter_id),
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
                        mask_type.clone(),
                        GraphValue::domain(edited.clone()),
                    ),
                }],
            ))
            .unwrap();

        let source = ParameterAddress::new(source_node_id, source_parameter_id);
        let target = ParameterAddress::new(target_node_id, target_parameter_id);
        let control_name = parameter("shared-mask");
        let rig = ParameterControlRig::new(
            [ReusableControl::new(
                control_name.clone(),
                "Shared Mask",
                "Reuses one complete editable mask stack.",
                ParameterControl::Automatic,
                ParameterReference::new(source, mask_type.clone()),
            )
            .unwrap()],
            [ControlRelationship::link(target, control_name)],
        )
        .unwrap();
        graph
            .apply(rig.transaction(&graph.snapshot()).unwrap())
            .unwrap();

        let expected_sample = graph
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
        let loaded = deserialize_graph::<GraphValue<MaskStack>>(&bytes).unwrap();
        let loaded_snapshot = loaded.graph().snapshot();
        let reloaded_evaluation = loaded_snapshot.evaluate_parameter(target).unwrap();
        let reloaded = reloaded_evaluation.value().payload().as_domain().unwrap();

        assert_eq!(reloaded.sample(at(5)).unwrap(), expected_sample);
        assert_eq!(serialize_graph(&loaded.graph().snapshot()).unwrap(), bytes);
    }
}
