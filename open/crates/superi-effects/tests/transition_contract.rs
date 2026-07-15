use std::sync::Arc;

use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use superi_core::geometry::PixelBounds;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_core::time::{Duration, RationalTime, Timebase};
use superi_effects::authoring::ParameterControl;
use superi_effects::reference::{
    compile_reference_node, evaluate_reference, required_input_regions, ReferenceEffectNode,
    ReferenceEffectState,
};
use superi_effects::transition::{
    TransitionCatalog, TransitionKind, TransitionNodeBindings, TransitionParameterBinding,
    TransitionTiming, WipeDirection,
};
use superi_graph::dag::{GraphEdge, GraphEndpoint};
use superi_graph::diagnostics::{IntrospectNode, NodeIntrospection, NodeStateFingerprint};
use superi_graph::eval::{
    EvaluateNode, EvaluationContext, EvaluationDependency, EvaluationRequest,
};
use superi_graph::headless::{GraphEvaluationSnapshot, NodeCompiler};
use superi_graph::ids::{EdgeId, GraphId, NodeId, ParameterId, PortId};
use superi_graph::mutate::{
    EditableGraph, EditableNode, GraphMutation, GraphSnapshot, GraphTransaction, InstancePort,
    TypedParameterValue,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeRegistry, NodeSchema,
    NodeSchemaId, NodeTypeId, ParameterName, PortCardinality, PortName, PortSchema, RoiBehavior,
    TimeBehavior, ValueTypeId,
};
use superi_graph::value::GraphValue;
use superi_image::limits::ImageLimits;
use superi_image::value::{Image, ImageDescriptor, ImageSamples};

const FROM_NODE: NodeId = NodeId::from_raw(10);
const FROM_OUTPUT: PortId = PortId::from_raw(11);
const TO_NODE: NodeId = NodeId::from_raw(20);
const TO_OUTPUT: PortId = PortId::from_raw(21);
const TRANSITION_NODE: NodeId = NodeId::from_raw(30);
const FROM_INPUT: PortId = PortId::from_raw(31);
const TO_INPUT: PortId = PortId::from_raw(32);
const RESULT_OUTPUT: PortId = PortId::from_raw(33);
const PROGRESS: ParameterId = ParameterId::from_raw(34);
const DIRECTION: ParameterId = ParameterId::from_raw(35);
const SOFTNESS: ParameterId = ParameterId::from_raw(36);

fn bounds(width: u32, height: u32) -> PixelBounds {
    PixelBounds::from_origin_size(0, 0, width, height).unwrap()
}

fn image32(width: u32, height: u32, pixels: &[[f32; 4]]) -> Image {
    let region = bounds(width, height);
    let descriptor = ImageDescriptor::new(
        region,
        region,
        PixelFormat::Rgba32Float,
        ColorSpace::ACESCG,
        AlphaMode::Premultiplied,
    )
    .unwrap();
    Image::new(
        descriptor,
        ImageSamples::from_f32(pixels.iter().flat_map(|pixel| pixel.iter().copied())),
    )
    .unwrap()
}

fn solid(width: u32, height: u32, pixel: [f32; 4]) -> Image {
    image32(width, height, &vec![pixel; (width * height) as usize])
}

fn pixels(image: &Image) -> Vec<[f32; 4]> {
    (0..image.samples().len() / 4)
        .map(|pixel| {
            [
                image.samples().float_value(pixel * 4).unwrap(),
                image.samples().float_value(pixel * 4 + 1).unwrap(),
                image.samples().float_value(pixel * 4 + 2).unwrap(),
                image.samples().float_value(pixel * 4 + 3).unwrap(),
            ]
        })
        .collect()
}

fn close(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() <= 0.000_1,
        "expected {expected}, observed {actual}"
    );
}

fn parameter_id(kind: TransitionKind, name: &str) -> ParameterId {
    match (kind, name) {
        (_, "progress") => PROGRESS,
        (TransitionKind::DirectionalWipe, "direction") => DIRECTION,
        (TransitionKind::DirectionalWipe, "softness") => SOFTNESS,
        _ => panic!("unknown transition parameter binding"),
    }
}

fn bindings(kind: TransitionKind, catalog: &TransitionCatalog) -> TransitionNodeBindings {
    let schema = catalog.schema(kind);
    TransitionNodeBindings::new(
        [
            InstancePort::new(FROM_INPUT, PortName::new("from").unwrap()),
            InstancePort::new(TO_INPUT, PortName::new("to").unwrap()),
        ],
        [InstancePort::new(
            RESULT_OUTPUT,
            PortName::new("result").unwrap(),
        )],
        schema.parameters().map(|parameter| {
            TransitionParameterBinding::new(
                parameter_id(kind, parameter.name().as_str()),
                ParameterName::new(parameter.name().as_str()).unwrap(),
            )
        }),
    )
}

#[test]
fn exact_transition_timing_maps_handles_to_clamped_progress() {
    let clock = Timebase::integer(24).unwrap();
    let timing = TransitionTiming::new(
        RationalTime::new(100, clock),
        Duration::new(12, clock).unwrap(),
        Duration::new(8, clock).unwrap(),
    )
    .unwrap();

    assert_eq!(timing.cut(), RationalTime::new(100, clock));
    assert_eq!(timing.from_offset(), Duration::new(12, clock).unwrap());
    assert_eq!(timing.to_offset(), Duration::new(8, clock).unwrap());
    assert_eq!(timing.range().start(), RationalTime::new(88, clock));
    assert_eq!(timing.range().duration(), Duration::new(20, clock).unwrap());
    assert_eq!(
        timing.range().end_exclusive().unwrap(),
        RationalTime::new(108, clock)
    );
    assert_eq!(
        timing.progress_at(RationalTime::new(80, clock)).unwrap(),
        0.0
    );
    assert_eq!(
        timing.progress_at(RationalTime::new(88, clock)).unwrap(),
        0.0
    );
    assert_eq!(
        timing.progress_at(RationalTime::new(98, clock)).unwrap(),
        0.5
    );
    assert_eq!(
        timing.progress_at(RationalTime::new(100, clock)).unwrap(),
        0.6
    );
    assert_eq!(
        timing.progress_at(RationalTime::new(108, clock)).unwrap(),
        1.0
    );
    assert_eq!(
        timing.progress_at(RationalTime::new(120, clock)).unwrap(),
        1.0
    );

    let other_clock = Timebase::integer(30).unwrap();
    assert_eq!(
        TransitionTiming::new(
            RationalTime::new(100, clock),
            Duration::new(12, other_clock).unwrap(),
            Duration::new(8, clock).unwrap(),
        )
        .unwrap_err()
        .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        TransitionTiming::new(
            RationalTime::new(100, clock),
            Duration::zero(clock),
            Duration::zero(clock),
        )
        .unwrap_err()
        .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        timing
            .progress_at(RationalTime::new(100, other_clock))
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        TransitionTiming::new(
            RationalTime::new(i64::MIN, clock),
            Duration::new(1, clock).unwrap(),
            Duration::zero(clock),
        )
        .unwrap_err()
        .category(),
        ErrorCategory::InvalidInput
    );
}

#[test]
fn transition_catalog_is_stable_inspectable_and_atomic() {
    let catalog = TransitionCatalog::new().unwrap();
    assert_eq!(
        TransitionKind::ALL
            .iter()
            .map(|kind| kind.code())
            .collect::<Vec<_>>(),
        [
            "superi.transition.cross-dissolve",
            "superi.transition.directional-wipe",
        ]
    );
    assert_eq!(
        catalog
            .schemas()
            .map(|schema| schema.id().node_type().as_str())
            .collect::<Vec<_>>(),
        [
            "superi.transition.cross-dissolve",
            "superi.transition.directional-wipe",
        ]
    );
    assert_eq!(
        WipeDirection::ALL
            .iter()
            .map(|direction| direction.code())
            .collect::<Vec<_>>(),
        [
            "left-to-right",
            "right-to-left",
            "top-to-bottom",
            "bottom-to-top",
        ]
    );
    assert_eq!(
        WipeDirection::from_code("bottom-to-top"),
        Some(WipeDirection::BottomToTop)
    );
    assert_eq!(WipeDirection::from_code("diagonal"), None);

    for kind in TransitionKind::ALL {
        let schema = catalog.schema(*kind);
        assert_eq!(schema.id().schema_version(), &SemanticVersion::new(1, 0, 0));
        assert_eq!(
            schema
                .inputs()
                .map(|port| port.name().as_str())
                .collect::<Vec<_>>(),
            ["from", "to"]
        );
        assert_eq!(schema.outputs().next().unwrap().name().as_str(), "result");
        assert_eq!(schema.behavior().time(), TimeBehavior::CurrentFrame);
        assert_eq!(schema.behavior().roi(), RoiBehavior::InputBounds);
        assert_eq!(
            schema.behavior().color(),
            ColorRequirements::Exact(ColorSpace::ACESCG)
        );
        assert_eq!(schema.behavior().determinism(), Determinism::Deterministic);
        assert_eq!(schema.behavior().cache_policy(), CachePolicy::PerRegion);
        assert!(schema
            .parameters()
            .all(|parameter| parameter.is_animatable()));
        assert_eq!(
            schema
                .required_capabilities()
                .iter()
                .map(|capability| capability.as_str())
                .collect::<Vec<_>>(),
            ["superi.render.gpu"]
        );
    }

    let dissolve = catalog
        .definition::<String>(TransitionKind::CrossDissolve)
        .unwrap();
    assert_eq!(dissolve.metadata().label(), "Cross Dissolve");
    assert_eq!(dissolve.metadata().category(), "Transitions");
    let progress = dissolve
        .parameter(&ParameterName::new("progress").unwrap())
        .unwrap();
    assert_eq!(progress.control(), ParameterControl::Percentage);
    assert_eq!(progress.default_value().payload().as_scalar(), Some(0.0));

    let wipe = catalog
        .definition::<String>(TransitionKind::DirectionalWipe)
        .unwrap();
    assert_eq!(
        wipe.schema()
            .parameters()
            .map(|parameter| parameter.name().as_str())
            .collect::<Vec<_>>(),
        ["direction", "progress", "softness"]
    );
    assert_eq!(
        wipe.parameter(&ParameterName::new("direction").unwrap())
            .unwrap()
            .control(),
        ParameterControl::Choice
    );
    assert_eq!(
        wipe.parameter(&ParameterName::new("direction").unwrap())
            .unwrap()
            .default_value()
            .payload()
            .as_choice(),
        Some("left-to-right")
    );
    assert_eq!(
        wipe.parameter(&ParameterName::new("softness").unwrap())
            .unwrap()
            .control(),
        ParameterControl::Percentage
    );

    let node = catalog
        .instantiate::<String>(
            TransitionKind::DirectionalWipe,
            bindings(TransitionKind::DirectionalWipe, &catalog),
        )
        .unwrap();
    assert_eq!(
        node.schema(),
        catalog.schema(TransitionKind::DirectionalWipe)
    );
    assert_eq!(
        node.parameters()
            .values()
            .find(|parameter| parameter.id() == PROGRESS)
            .unwrap()
            .value()
            .payload()
            .as_scalar(),
        Some(0.0)
    );

    let mut registry = NodeRegistry::new();
    catalog.register(&mut registry).unwrap();
    let before = registry.snapshot();
    assert_eq!(before.revision(), 1);
    assert_eq!(before.len(), TransitionKind::ALL.len());
    assert_eq!(
        catalog.register(&mut registry).unwrap_err().category(),
        ErrorCategory::Conflict
    );
    assert_eq!(registry.snapshot(), before);

    let incomplete = TransitionNodeBindings::new(
        [InstancePort::new(
            FROM_INPUT,
            PortName::new("from").unwrap(),
        )],
        [InstancePort::new(
            RESULT_OUTPUT,
            PortName::new("result").unwrap(),
        )],
        [TransitionParameterBinding::new(
            PROGRESS,
            ParameterName::new("progress").unwrap(),
        )],
    );
    assert_eq!(
        catalog
            .instantiate::<String>(TransitionKind::CrossDissolve, incomplete)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
}

#[test]
fn reference_transitions_blend_premultiplied_pixels_and_are_tile_stable() {
    let from = solid(4, 1, [1.0, 0.0, 0.0, 1.0]);
    let to = solid(4, 1, [0.0, 0.0, 1.0, 1.0]);
    let region = bounds(4, 1);

    for (progress, expected) in [
        (0.0, [1.0, 0.0, 0.0, 1.0]),
        (0.5, [0.5, 0.0, 0.5, 1.0]),
        (1.0, [0.0, 0.0, 1.0, 1.0]),
    ] {
        let output = evaluate_reference(
            &ReferenceEffectState::CrossDissolve { progress },
            &[from.clone(), to.clone()],
            region,
            &ImageLimits::default(),
        )
        .unwrap();
        assert!(pixels(&output).iter().all(|pixel| *pixel == expected));
    }

    let expected_blue = [
        (WipeDirection::LeftToRight, vec![0, 2]),
        (WipeDirection::RightToLeft, vec![1, 3]),
        (WipeDirection::TopToBottom, vec![0, 1]),
        (WipeDirection::BottomToTop, vec![2, 3]),
    ];
    let from_2d = solid(2, 2, [1.0, 0.0, 0.0, 1.0]);
    let to_2d = solid(2, 2, [0.0, 0.0, 1.0, 1.0]);
    for (direction, blue_pixels) in expected_blue {
        let output = evaluate_reference(
            &ReferenceEffectState::DirectionalWipe {
                progress: 0.5,
                direction,
                softness: 0.0,
            },
            &[from_2d.clone(), to_2d.clone()],
            bounds(2, 2),
            &ImageLimits::default(),
        )
        .unwrap();
        for (index, pixel) in pixels(&output).iter().enumerate() {
            if blue_pixels.contains(&index) {
                assert_eq!(*pixel, [0.0, 0.0, 1.0, 1.0]);
            } else {
                assert_eq!(*pixel, [1.0, 0.0, 0.0, 1.0]);
            }
        }
    }

    let soft = evaluate_reference(
        &ReferenceEffectState::DirectionalWipe {
            progress: 0.5,
            direction: WipeDirection::LeftToRight,
            softness: 1.0,
        },
        &[
            solid(1, 1, [1.0, 0.0, 0.0, 1.0]),
            solid(1, 1, [0.0, 0.0, 1.0, 1.0]),
        ],
        bounds(1, 1),
        &ImageLimits::default(),
    )
    .unwrap();
    close(pixels(&soft)[0][0], 0.5);
    close(pixels(&soft)[0][2], 0.5);

    let state = ReferenceEffectState::DirectionalWipe {
        progress: 0.55,
        direction: WipeDirection::LeftToRight,
        softness: 0.4,
    };
    assert_eq!(
        required_input_regions(&state, region).unwrap(),
        [region, region]
    );
    let full = evaluate_reference(
        &state,
        &[from.clone(), to.clone()],
        region,
        &ImageLimits::default(),
    )
    .unwrap();
    let left_region = PixelBounds::new(0, 0, 2, 1).unwrap();
    let right_region = PixelBounds::new(2, 0, 4, 1).unwrap();
    let left = evaluate_reference(
        &state,
        &[from.clone(), to.clone()],
        left_region,
        &ImageLimits::default(),
    )
    .unwrap();
    let right = evaluate_reference(
        &state,
        &[from.clone(), to.clone()],
        right_region,
        &ImageLimits::default(),
    )
    .unwrap();
    let mut tiled = pixels(&left);
    tiled.extend(pixels(&right));
    assert_eq!(pixels(&full), tiled);

    for progress in [0.0, 1.0] {
        let output = evaluate_reference(
            &ReferenceEffectState::DirectionalWipe {
                progress,
                direction: WipeDirection::LeftToRight,
                softness: 1.0,
            },
            &[from.clone(), to.clone()],
            region,
            &ImageLimits::default(),
        )
        .unwrap();
        let expected = if progress == 0.0 {
            [1.0, 0.0, 0.0, 1.0]
        } else {
            [0.0, 0.0, 1.0, 1.0]
        };
        assert!(pixels(&output).iter().all(|pixel| *pixel == expected));
    }

    assert_eq!(
        evaluate_reference(
            &ReferenceEffectState::CrossDissolve { progress: 1.1 },
            &[from.clone(), to.clone()],
            region,
            &ImageLimits::default(),
        )
        .unwrap_err()
        .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        evaluate_reference(
            &ReferenceEffectState::DirectionalWipe {
                progress: 0.5,
                direction: WipeDirection::LeftToRight,
                softness: -0.1,
            },
            &[from, to],
            region,
            &ImageLimits::default(),
        )
        .unwrap_err()
        .category(),
        ErrorCategory::InvalidInput
    );

    let mismatched_display = PixelBounds::from_origin_size(0, 0, 5, 1).unwrap();
    let mismatched_descriptor = ImageDescriptor::new(
        region,
        mismatched_display,
        PixelFormat::Rgba32Float,
        ColorSpace::ACESCG,
        AlphaMode::Premultiplied,
    )
    .unwrap();
    let mismatched = Image::new(
        mismatched_descriptor,
        ImageSamples::from_f32([0.0, 0.0, 1.0, 1.0].repeat(4)),
    )
    .unwrap();
    assert_eq!(
        evaluate_reference(
            &ReferenceEffectState::CrossDissolve { progress: 0.5 },
            &[solid(4, 1, [1.0, 0.0, 0.0, 1.0]), mismatched],
            region,
            &ImageLimits::default(),
        )
        .unwrap_err()
        .category(),
        ErrorCategory::InvalidInput
    );
}

fn schema_id(code: &str) -> NodeSchemaId {
    NodeSchemaId::new(
        NodeTypeId::new(code).unwrap(),
        SemanticVersion::new(1, 0, 0),
    )
}

fn image_behavior() -> NodeBehavior {
    NodeBehavior::new(
        TimeBehavior::CurrentFrame,
        RoiBehavior::InputBounds,
        ColorRequirements::Exact(ColorSpace::ACESCG),
        Determinism::Deterministic,
        CachePolicy::PerRegion,
    )
}

fn source_node(code: &str, output: PortId) -> EditableNode<GraphValue<String>> {
    let image_type = ValueTypeId::new("superi.value.image").unwrap();
    let schema = Arc::new(
        NodeSchema::new(
            schema_id(code),
            [],
            [PortSchema::new(
                PortName::new("result").unwrap(),
                image_type,
                PortCardinality::Single,
            )],
            [],
            image_behavior(),
            CapabilitySet::default(),
        )
        .unwrap(),
    );
    EditableNode::new(
        schema,
        [],
        [InstancePort::new(output, PortName::new("result").unwrap())],
        [],
    )
    .unwrap()
}

fn transition_graph(kind: TransitionKind) -> EditableGraph<GraphValue<String>> {
    let catalog = TransitionCatalog::new().unwrap();
    let transition = catalog.instantiate(kind, bindings(kind, &catalog)).unwrap();
    let mut graph = EditableGraph::new(GraphId::from_raw(1));
    graph
        .apply(GraphTransaction::with_mutations(
            0,
            [
                GraphMutation::Add {
                    node_id: FROM_NODE,
                    node: source_node("superi.test.transition-from", FROM_OUTPUT),
                    position: 0,
                },
                GraphMutation::Add {
                    node_id: TO_NODE,
                    node: source_node("superi.test.transition-to", TO_OUTPUT),
                    position: 1,
                },
                GraphMutation::Add {
                    node_id: TRANSITION_NODE,
                    node: transition,
                    position: 2,
                },
                GraphMutation::Connect {
                    edge: GraphEdge::new(
                        EdgeId::from_raw(40),
                        GraphEndpoint::new(FROM_NODE, FROM_OUTPUT),
                        GraphEndpoint::new(TRANSITION_NODE, FROM_INPUT),
                    ),
                },
                GraphMutation::Connect {
                    edge: GraphEdge::new(
                        EdgeId::from_raw(41),
                        GraphEndpoint::new(TO_NODE, TO_OUTPUT),
                        GraphEndpoint::new(TRANSITION_NODE, TO_INPUT),
                    ),
                },
            ],
        ))
        .unwrap();
    graph
}

#[derive(Clone, Debug)]
enum RuntimeNode {
    Source(Image, NodeIntrospection),
    Transition(ReferenceEffectNode),
}

impl EvaluateNode<Image> for RuntimeNode {
    fn dependencies(
        &self,
        request: EvaluationRequest,
        incoming: &[GraphEdge],
    ) -> Result<Vec<EvaluationDependency>> {
        match self {
            Self::Source(_, _) => Ok(Vec::new()),
            Self::Transition(node) => node.dependencies(request, incoming),
        }
    }

    fn evaluate(&self, context: &EvaluationContext<'_, Image>) -> Result<Image> {
        match self {
            Self::Source(image, _) => Ok(image.clone()),
            Self::Transition(node) => node.evaluate(context),
        }
    }
}

impl IntrospectNode for RuntimeNode {
    fn introspection(&self) -> NodeIntrospection {
        match self {
            Self::Source(_, introspection) => introspection.clone(),
            Self::Transition(node) => node.introspection(),
        }
    }
}

struct RuntimeCompiler<'a> {
    from: &'a Image,
    to: &'a Image,
}

impl NodeCompiler<GraphValue<String>, RuntimeNode> for RuntimeCompiler<'_> {
    fn compile_node(
        &mut self,
        snapshot: &GraphSnapshot<GraphValue<String>>,
        node_id: NodeId,
        node: &EditableNode<GraphValue<String>>,
    ) -> Result<RuntimeNode> {
        match node_id {
            FROM_NODE => Ok(RuntimeNode::Source(
                self.from.clone(),
                NodeIntrospection::new(
                    node.schema().id().clone(),
                    node.schema().behavior(),
                    NodeStateFingerprint::from_canonical_bytes(b"transition-from-v1"),
                ),
            )),
            TO_NODE => Ok(RuntimeNode::Source(
                self.to.clone(),
                NodeIntrospection::new(
                    node.schema().id().clone(),
                    node.schema().behavior(),
                    NodeStateFingerprint::from_canonical_bytes(b"transition-to-v1"),
                ),
            )),
            TRANSITION_NODE => {
                compile_reference_node(snapshot, node_id, node).map(RuntimeNode::Transition)
            }
            _ => Err(Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "unexpected transition contract node",
            )),
        }
    }
}

fn compile_graph(
    snapshot: GraphSnapshot<GraphValue<String>>,
    from: &Image,
    to: &Image,
) -> Result<GraphEvaluationSnapshot<GraphValue<String>, RuntimeNode>> {
    GraphEvaluationSnapshot::compile(snapshot, RuntimeCompiler { from, to })
}

fn request() -> EvaluationRequest {
    EvaluationRequest::new(
        GraphEndpoint::new(TRANSITION_NODE, RESULT_OUTPUT),
        RationalTime::zero(Timebase::integer(24).unwrap()),
        bounds(1, 1),
    )
}

#[test]
fn one_editable_transition_graph_drives_pixels_introspection_and_snapshot_isolation() {
    let from = solid(1, 1, [1.0, 0.0, 0.0, 1.0]);
    let to = solid(1, 1, [0.0, 0.0, 1.0, 1.0]);
    let mut editable = transition_graph(TransitionKind::CrossDissolve);
    let old_snapshot = editable.snapshot();
    let old_runtime = compile_graph(old_snapshot, &from, &to).unwrap();

    assert_eq!(
        pixels(old_runtime.evaluate::<Image>(request()).unwrap().value())[0],
        [1.0, 0.0, 0.0, 1.0]
    );
    let old_inspection = old_runtime.inspect::<Image>(request()).unwrap();
    let old_key = old_inspection
        .node(request().key())
        .unwrap()
        .cache_status()
        .key()
        .unwrap();

    let scalar = ValueTypeId::new("superi.value.scalar").unwrap();
    let new_snapshot = editable
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::SetParameter {
                node_id: TRANSITION_NODE,
                parameter_id: PROGRESS,
                value: TypedParameterValue::new(scalar, GraphValue::<String>::scalar(0.5).unwrap()),
            }],
        ))
        .unwrap();
    let current = compile_graph(new_snapshot, &from, &to).unwrap();
    assert_eq!(
        pixels(current.evaluate::<Image>(request()).unwrap().value())[0],
        [0.5, 0.0, 0.5, 1.0]
    );
    let current_key = current
        .inspect::<Image>(request())
        .unwrap()
        .node(request().key())
        .unwrap()
        .cache_status()
        .key()
        .unwrap();
    assert_ne!(old_key, current_key);
    assert_eq!(
        pixels(old_runtime.evaluate::<Image>(request()).unwrap().value())[0],
        [1.0, 0.0, 0.0, 1.0]
    );

    let mut second_workflow = transition_graph(TransitionKind::CrossDissolve);
    let second_snapshot = second_workflow
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::SetParameter {
                node_id: TRANSITION_NODE,
                parameter_id: PROGRESS,
                value: TypedParameterValue::new(
                    ValueTypeId::new("superi.value.scalar").unwrap(),
                    GraphValue::<String>::scalar(0.5).unwrap(),
                ),
            }],
        ))
        .unwrap();
    let second = compile_graph(second_snapshot, &from, &to).unwrap();
    assert_eq!(
        pixels(second.evaluate::<Image>(request()).unwrap().value()),
        pixels(current.evaluate::<Image>(request()).unwrap().value())
    );
}

#[test]
fn reference_compiler_rejects_unknown_transition_choices() {
    let from = solid(1, 1, [1.0, 0.0, 0.0, 1.0]);
    let to = solid(1, 1, [0.0, 0.0, 1.0, 1.0]);
    let mut editable = transition_graph(TransitionKind::DirectionalWipe);
    let choice = ValueTypeId::new("superi.value.choice").unwrap();
    let snapshot = editable
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::SetParameter {
                node_id: TRANSITION_NODE,
                parameter_id: DIRECTION,
                value: TypedParameterValue::new(
                    choice,
                    GraphValue::<String>::choice("diagonal").unwrap(),
                ),
            }],
        ))
        .unwrap();
    assert_eq!(
        compile_graph(snapshot, &from, &to).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
}
