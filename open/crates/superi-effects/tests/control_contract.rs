use std::str::FromStr;

use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory};
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_core::time::{RationalTime, Timebase};
use superi_effects::authoring::ParameterControl;
use superi_effects::catalog::{
    EffectCatalog, EffectNodeBindings, EffectNodeKind, EffectParameterBinding,
};
use superi_effects::control::{
    evaluate_animated_parameter, ControlRelationship, ParameterControlRig, ParentExpression,
    ReusableControl,
};
use superi_effects::keyframe::{
    AnimationCurve, AnimationValue, Easing, Interpolation, Keyframe, KeyframeTiming,
};
use superi_effects::reference::{compile_reference_node, ReferenceEffectState};
use superi_graph::expr::{ParameterAddress, ParameterReference};
use superi_graph::ids::{GraphId, NodeId, ParameterId, PortId};
use superi_graph::mutate::{
    EditableGraph, EditableNode, EditableParameter, GraphMutation, GraphTransaction, InstancePort,
    TypedParameterValue,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchema, NodeSchemaId,
    NodeTypeId, ParameterName, ParameterSchema, PortName, RoiBehavior, TimeBehavior, ValueTypeId,
};
use superi_graph::serialize::{deserialize_graph, serialize_graph};
use superi_graph::value::GraphValue;

const PROCESSING_ROOT: u128 = 101;
const PROCESSING_LOCAL: u128 = 102;
const PROCESSING_EFFECT: u128 = 103;
const PROCESSING_INPUT: PortId = PortId::from_raw(10_401);
const PROCESSING_OUTPUT: PortId = PortId::from_raw(10_402);

fn clock() -> Timebase {
    Timebase::integer(10).unwrap()
}

fn at(value: i64) -> RationalTime {
    RationalTime::new(value, clock())
}

fn animation_value(components: &[f64]) -> AnimationValue {
    AnimationValue::new(components.to_vec()).unwrap()
}

fn key(time: i64, components: &[f64]) -> Keyframe {
    Keyframe::new(
        KeyframeTiming::Fixed(at(time)),
        animation_value(components),
        Interpolation::Linear,
        Easing::Linear,
        None,
        None,
    )
    .unwrap()
}

fn curve(start: &[f64], end: &[f64]) -> AnimationCurve {
    AnimationCurve::new(clock(), [key(0, start), key(10, end)], None).unwrap()
}

fn constant(components: &[f64]) -> AnimationCurve {
    AnimationCurve::new(clock(), [key(0, components)], None).unwrap()
}

fn value_type(input: &str) -> ValueTypeId {
    ValueTypeId::from_str(input).unwrap()
}

fn name(input: &str) -> ParameterName {
    ParameterName::new(input).unwrap()
}

fn address(node: u128) -> ParameterAddress {
    ParameterAddress::new(
        NodeId::from_raw(node),
        ParameterId::from_raw(node * 100 + 1),
    )
}

fn reference(node: u128, type_name: &str) -> ParameterReference {
    ParameterReference::new(address(node), value_type(type_name))
}

fn animation_node(
    node: u128,
    parameter_name: &str,
    type_name: &str,
    animation: AnimationCurve,
    animatable: bool,
) -> EditableNode<AnimationCurve> {
    let parameter_name = name(parameter_name);
    let value_type = value_type(type_name);
    let schema = NodeSchema::new(
        NodeSchemaId::new(
            NodeTypeId::from_str(&format!("superi.effects.control_{node}")).unwrap(),
            SemanticVersion::new(1, 0, 0),
        ),
        [],
        [],
        [ParameterSchema::new(
            parameter_name.clone(),
            value_type.clone(),
            animatable,
        )],
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
    EditableNode::new(
        schema.into(),
        [],
        [],
        [EditableParameter::new(
            address(node).parameter_id(),
            parameter_name,
            TypedParameterValue::new(value_type, animation),
        )],
    )
    .unwrap()
}

fn seeded_graph(graph_id: u128) -> EditableGraph<AnimationCurve> {
    let scalar = "superi.value.scalar";
    let point = "superi.value.point2";
    let nodes = [
        animation_node(1, "root", scalar, curve(&[10.0], &[20.0]), true),
        animation_node(2, "local", scalar, curve(&[2.0], &[4.0]), true),
        animation_node(3, "child", scalar, constant(&[0.0]), true),
        animation_node(4, "target-a", scalar, constant(&[0.0]), true),
        animation_node(5, "target-b", scalar, constant(&[0.0]), true),
        animation_node(6, "point", point, curve(&[1.0, 2.0], &[3.0, 4.0]), true),
        animation_node(7, "point-target", point, constant(&[0.0, 0.0]), true),
        animation_node(8, "static", scalar, constant(&[5.0]), false),
        animation_node(9, "point-local", point, constant(&[1.0, 1.0]), true),
        animation_node(10, "point-derived", point, constant(&[0.0, 0.0]), true),
    ];
    let mut graph = EditableGraph::new(GraphId::from_raw(graph_id));
    graph
        .apply(GraphTransaction::with_mutations(
            0,
            nodes
                .into_iter()
                .enumerate()
                .map(|(position, node)| GraphMutation::Add {
                    node_id: NodeId::from_raw((position + 1) as u128),
                    node,
                    position,
                }),
        ))
        .unwrap();
    graph
}

fn processing_control_node<T>(node: u128, value: f64) -> EditableNode<GraphValue<T>> {
    let parameter_name = name("value");
    let value_type = value_type("superi.value.scalar");
    let schema = NodeSchema::new(
        NodeSchemaId::new(
            NodeTypeId::from_str(&format!("superi.effects.processing_control_{node}")).unwrap(),
            SemanticVersion::new(1, 0, 0),
        ),
        [],
        [],
        [ParameterSchema::new(
            parameter_name.clone(),
            value_type.clone(),
            true,
        )],
        NodeBehavior::new(
            TimeBehavior::Invariant,
            RoiBehavior::InputBounds,
            ColorRequirements::NotApplicable,
            Determinism::Deterministic,
            CachePolicy::Static,
        ),
        CapabilitySet::default(),
    )
    .unwrap();
    EditableNode::new(
        schema.into(),
        [],
        [],
        [EditableParameter::new(
            address(node).parameter_id(),
            parameter_name,
            TypedParameterValue::new(value_type, GraphValue::scalar(value).unwrap()),
        )],
    )
    .unwrap()
}

fn shared_processing_graph<T: Clone>(graph_id: u128) -> EditableGraph<GraphValue<T>> {
    let catalog = EffectCatalog::new().unwrap();
    let effect = catalog
        .instantiate(
            EffectNodeKind::Opacity,
            EffectNodeBindings::new(
                [InstancePort::new(
                    PROCESSING_INPUT,
                    PortName::new("source").unwrap(),
                )],
                [InstancePort::new(
                    PROCESSING_OUTPUT,
                    PortName::new("result").unwrap(),
                )],
                [EffectParameterBinding::new(
                    address(PROCESSING_EFFECT).parameter_id(),
                    name("opacity"),
                )],
            ),
        )
        .unwrap();
    let mut graph = EditableGraph::new(GraphId::from_raw(graph_id));
    graph
        .apply(GraphTransaction::with_mutations(
            0,
            [
                GraphMutation::Add {
                    node_id: NodeId::from_raw(PROCESSING_ROOT),
                    node: processing_control_node(PROCESSING_ROOT, 0.25),
                    position: 0,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(PROCESSING_LOCAL),
                    node: processing_control_node(PROCESSING_LOCAL, 2.0),
                    position: 1,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(PROCESSING_EFFECT),
                    node: effect,
                    position: 2,
                },
            ],
        ))
        .unwrap();
    graph
}

fn shared_processing_rig() -> ParameterControlRig {
    ParameterControlRig::new(
        [
            control(
                "root",
                "Root",
                PROCESSING_ROOT,
                "superi.value.scalar",
                ParameterControl::Slider,
            ),
            control(
                "local",
                "Local",
                PROCESSING_LOCAL,
                "superi.value.scalar",
                ParameterControl::Slider,
            ),
        ],
        [ControlRelationship::parent(
            address(PROCESSING_EFFECT),
            name("root"),
            name("local"),
            ParentExpression::compile("parent * local").unwrap(),
        )],
    )
    .unwrap()
}

fn control(
    control_name: &str,
    label: &str,
    source_node: u128,
    type_name: &str,
    presentation: ParameterControl,
) -> ReusableControl {
    ReusableControl::new(
        name(control_name),
        label,
        format!("Reusable {label} control."),
        presentation,
        reference(source_node, type_name),
    )
    .unwrap()
}

fn rig() -> ParameterControlRig {
    let scalar = "superi.value.scalar";
    let point = "superi.value.point2";
    ParameterControlRig::new(
        [
            control("root", "Root", 1, scalar, ParameterControl::Slider),
            control("local", "Local", 2, scalar, ParameterControl::Slider),
            control("child", "Child", 3, scalar, ParameterControl::Slider),
            control("point", "Point", 6, point, ParameterControl::Point2),
            control(
                "point-local",
                "Point Local",
                9,
                point,
                ParameterControl::Point2,
            ),
        ],
        [
            ControlRelationship::link(address(5), name("child")),
            ControlRelationship::parent(
                address(3),
                name("root"),
                name("local"),
                ParentExpression::compile("parent + local").unwrap(),
            ),
            ControlRelationship::link(address(4), name("child")),
            ControlRelationship::link(address(7), name("point")),
            ControlRelationship::parent(
                address(10),
                name("point"),
                name("point-local"),
                ParentExpression::compile("parent + local").unwrap(),
            ),
        ],
    )
    .unwrap()
}

fn reason(error: &Error) -> Option<&str> {
    error
        .contexts()
        .iter()
        .find_map(|context| context.field("reason"))
}

#[test]
fn reusable_controls_compile_to_inspectable_graph_links_and_parenting() {
    let rig = rig();
    assert_eq!(
        rig.controls()
            .map(|control| control.name().as_str())
            .collect::<Vec<_>>(),
        ["child", "local", "point", "point-local", "root"]
    );
    assert_eq!(
        rig.relationships()
            .map(ControlRelationship::target)
            .collect::<Vec<_>>(),
        [address(3), address(4), address(5), address(7), address(10)]
    );
    let child = rig.control(&name("child")).unwrap();
    assert_eq!(child.label(), "Child");
    assert_eq!(child.summary(), "Reusable Child control.");
    assert_eq!(child.control(), ParameterControl::Slider);
    assert_eq!(child.source().address(), address(3));

    let mut graph = seeded_graph(90);
    let transaction = rig.transaction(&graph.snapshot()).unwrap();
    assert_eq!(transaction.expected_revision(), 1);
    assert_eq!(transaction.mutations().len(), 5);
    let snapshot = graph.apply(transaction).unwrap();

    assert_eq!(snapshot.parameter_drivers().len(), 5);
    let parent = snapshot
        .parameter_driver(address(3))
        .unwrap()
        .as_expression()
        .unwrap();
    assert_eq!(parent.source(), "parent + local");
    assert_eq!(
        parent
            .variables()
            .keys()
            .map(|variable| variable.as_str())
            .collect::<Vec<_>>(),
        ["local", "parent"]
    );
    assert_eq!(
        snapshot
            .parameter_driver(address(4))
            .unwrap()
            .as_link()
            .unwrap()
            .address(),
        address(3)
    );

    let first = evaluate_animated_parameter(&snapshot, address(4), at(5)).unwrap();
    let second = evaluate_animated_parameter(&snapshot, address(5), at(5)).unwrap();
    assert_eq!(first.value().payload().components(), &[18.0]);
    assert_eq!(second.value(), first.value());
    assert_eq!(
        first.evaluated_parameters(),
        [address(1), address(2), address(3), address(4)]
    );

    let point = evaluate_animated_parameter(&snapshot, address(7), at(5)).unwrap();
    assert_eq!(point.value().payload().components(), &[2.0, 3.0]);
    assert_eq!(point.evaluated_parameters(), [address(6), address(7)]);

    let vector_parent = evaluate_animated_parameter(&snapshot, address(10), at(5)).unwrap_err();
    assert_eq!(vector_parent.category(), ErrorCategory::InvalidInput);
    assert_eq!(reason(&vector_parent), Some("non_scalar_animation_value"));
}

#[test]
fn one_rig_and_persisted_driver_state_are_reusable_across_workflow_roles() {
    let rig = rig();
    let mut timeline_role = seeded_graph(91);
    let mut node_graph_role = seeded_graph(92);
    let timeline = timeline_role
        .apply(rig.transaction(&timeline_role.snapshot()).unwrap())
        .unwrap();
    let node_graph = node_graph_role
        .apply(rig.transaction(&node_graph_role.snapshot()).unwrap())
        .unwrap();

    assert_eq!(timeline.parameter_drivers(), node_graph.parameter_drivers());
    assert_eq!(
        evaluate_animated_parameter(&timeline, address(4), at(5))
            .unwrap()
            .value(),
        evaluate_animated_parameter(&node_graph, address(4), at(5))
            .unwrap()
            .value()
    );

    let observations = std::thread::scope(|scope| {
        ["editor", "script", "headless"]
            .map(|_| {
                scope.spawn(|| {
                    evaluate_animated_parameter(&timeline, address(4), at(5))
                        .unwrap()
                        .value()
                        .clone()
                })
            })
            .map(|handle| handle.join().unwrap())
    });
    assert_eq!(observations[0], observations[1]);
    assert_eq!(observations[1], observations[2]);

    let bytes = serialize_graph(&timeline).unwrap();
    let loaded = deserialize_graph::<AnimationCurve>(&bytes).unwrap();
    let reloaded = loaded.graph().snapshot();
    assert_eq!(reloaded.parameter_drivers(), timeline.parameter_drivers());
    assert_eq!(serialize_graph(&reloaded).unwrap(), bytes);
    assert_eq!(
        evaluate_animated_parameter(&reloaded, address(4), at(5))
            .unwrap()
            .value(),
        evaluate_animated_parameter(&timeline, address(4), at(5))
            .unwrap()
            .value()
    );

    let cleared = timeline_role
        .apply(GraphTransaction::with_mutations(
            timeline.revision(),
            [GraphMutation::ClearParameterDriver { target: address(5) }],
        ))
        .unwrap();
    assert_eq!(
        evaluate_animated_parameter(&cleared, address(5), at(5))
            .unwrap()
            .value()
            .payload()
            .components(),
        &[0.0]
    );
}

#[test]
fn reusable_parent_controls_compile_through_real_built_in_visual_state() {
    let rig = shared_processing_rig();
    let mut timeline_role = shared_processing_graph::<u64>(95);
    let mut node_graph_role = shared_processing_graph::<String>(96);
    let timeline = timeline_role
        .apply(rig.transaction(&timeline_role.snapshot()).unwrap())
        .unwrap();
    let node_graph = node_graph_role
        .apply(rig.transaction(&node_graph_role.snapshot()).unwrap())
        .unwrap();

    assert_eq!(timeline.parameter_drivers(), node_graph.parameter_drivers());
    assert_eq!(
        timeline
            .evaluate_parameter(address(PROCESSING_EFFECT))
            .unwrap()
            .value()
            .payload()
            .as_scalar(),
        Some(0.5)
    );
    assert_eq!(
        node_graph
            .evaluate_parameter(address(PROCESSING_EFFECT))
            .unwrap()
            .value()
            .payload()
            .as_scalar(),
        Some(0.5)
    );

    let timeline_effect = compile_reference_node(
        &timeline,
        NodeId::from_raw(PROCESSING_EFFECT),
        timeline.node(NodeId::from_raw(PROCESSING_EFFECT)).unwrap(),
    )
    .unwrap();
    let node_graph_effect = compile_reference_node(
        &node_graph,
        NodeId::from_raw(PROCESSING_EFFECT),
        node_graph
            .node(NodeId::from_raw(PROCESSING_EFFECT))
            .unwrap(),
    )
    .unwrap();
    assert_eq!(timeline_effect.state(), node_graph_effect.state());
    assert_eq!(
        timeline_effect.state(),
        &ReferenceEffectState::Opacity { opacity: 0.5 }
    );
}

#[test]
fn malformed_or_nonanimatable_rigs_fail_before_graph_publication() {
    let scalar = "superi.value.scalar";
    let empty_label = ReusableControl::new(
        name("empty"),
        " ",
        "summary",
        ParameterControl::Slider,
        reference(1, scalar),
    )
    .unwrap_err();
    assert_eq!(reason(&empty_label), Some("empty_text"));

    let incomplete = ParentExpression::compile("parent").unwrap_err();
    assert_eq!(reason(&incomplete), Some("incomplete_parent_expression"));

    let duplicate_control = ParameterControlRig::new(
        [
            control("same", "First", 1, scalar, ParameterControl::Slider),
            control("same", "Second", 2, scalar, ParameterControl::Slider),
        ],
        [],
    )
    .unwrap_err();
    assert_eq!(reason(&duplicate_control), Some("duplicate_control_name"));

    let missing_control = ParameterControlRig::new(
        [control("root", "Root", 1, scalar, ParameterControl::Slider)],
        [ControlRelationship::link(address(4), name("missing"))],
    )
    .unwrap_err();
    assert_eq!(reason(&missing_control), Some("missing_control"));

    let duplicate_target = ParameterControlRig::new(
        [control("root", "Root", 1, scalar, ParameterControl::Slider)],
        [
            ControlRelationship::link(address(4), name("root")),
            ControlRelationship::link(address(4), name("root")),
        ],
    )
    .unwrap_err();
    assert_eq!(
        reason(&duplicate_target),
        Some("duplicate_relationship_target")
    );

    let graph = seeded_graph(93);
    let static_source = ParameterControlRig::new(
        [control(
            "static",
            "Static",
            8,
            scalar,
            ParameterControl::Slider,
        )],
        [ControlRelationship::link(address(4), name("static"))],
    )
    .unwrap()
    .transaction(&graph.snapshot())
    .unwrap_err();
    assert_eq!(
        reason(&static_source),
        Some("control_parameter_not_animatable")
    );

    let static_target = ParameterControlRig::new(
        [control("root", "Root", 1, scalar, ParameterControl::Slider)],
        [ControlRelationship::link(address(8), name("root"))],
    )
    .unwrap()
    .transaction(&graph.snapshot())
    .unwrap_err();
    assert_eq!(
        reason(&static_target),
        Some("target_parameter_not_animatable")
    );

    let wrong_type = ParameterControlRig::new(
        [control(
            "point",
            "Point",
            6,
            "superi.value.point2",
            ParameterControl::Point2,
        )],
        [ControlRelationship::link(address(4), name("point"))],
    )
    .unwrap()
    .transaction(&graph.snapshot())
    .unwrap_err();
    assert_eq!(reason(&wrong_type), Some("control_target_type_mismatch"));
}

#[test]
fn control_parent_cycles_are_rejected_by_the_atomic_graph_boundary() {
    let scalar = "superi.value.scalar";
    let mut graph = seeded_graph(94);
    let snapshot = graph
        .apply(rig().transaction(&graph.snapshot()).unwrap())
        .unwrap();
    let before = snapshot.clone();

    let cycle = ParameterControlRig::new(
        [
            control("child", "Child", 3, scalar, ParameterControl::Slider),
            control("local", "Local", 2, scalar, ParameterControl::Slider),
        ],
        [ControlRelationship::parent(
            address(1),
            name("child"),
            name("local"),
            ParentExpression::compile("parent + local").unwrap(),
        )],
    )
    .unwrap();
    let error = graph
        .apply(cycle.transaction(&snapshot).unwrap())
        .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(reason(&error), Some("parameter_dependency_cycle"));
    assert_eq!(graph.snapshot(), before);
}
