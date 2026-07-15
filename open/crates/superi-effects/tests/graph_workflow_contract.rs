use std::sync::Arc;

use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use superi_core::geometry::PixelBounds;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_core::time::{RationalTime, Timebase};
use superi_effects::catalog::{
    EffectCatalog, EffectNodeBindings, EffectNodeKind, EffectParameterBinding,
};
use superi_effects::reference::{compile_reference_node, ReferenceEffectNode};
use superi_graph::dag::{GraphEdge, GraphEndpoint};
use superi_graph::diagnostics::{IntrospectNode, NodeIntrospection, NodeStateFingerprint};
use superi_graph::eval::{
    EvaluateNode, EvaluationContext, EvaluationDependency, EvaluationRequest,
};
use superi_graph::expr::{
    ExpressionVariableName, ParameterAddress, ParameterDriver, ParameterExpression,
    ParameterReference,
};
use superi_graph::headless::{GraphEvaluationSnapshot, NodeCompiler};
use superi_graph::ids::{EdgeId, GraphId, NodeId, ParameterId, PortId};
use superi_graph::mutate::{
    EditableGraph, EditableNode, EditableParameter, GraphMutation, GraphSnapshot, GraphTransaction,
    InstancePort, TypedParameterValue,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchema, NodeSchemaId,
    NodeTypeId, ParameterName, ParameterSchema, PortCardinality, PortName, PortSchema, RoiBehavior,
    TimeBehavior, ValueTypeId,
};
use superi_graph::value::GraphValue;
use superi_image::value::{Image, ImageDescriptor, ImageSamples};

const SOURCE_NODE: NodeId = NodeId::from_raw(10);
const SOURCE_OUTPUT: PortId = PortId::from_raw(11);
const EFFECT_NODE: NodeId = NodeId::from_raw(20);
const EFFECT_INPUT: PortId = PortId::from_raw(21);
const EFFECT_OUTPUT: PortId = PortId::from_raw(22);
const OPACITY: ParameterId = ParameterId::from_raw(23);
const CONTROL_NODE: NodeId = NodeId::from_raw(30);
const CONTROL_VALUE: ParameterId = ParameterId::from_raw(31);

#[derive(Clone, Debug)]
enum RuntimeNode {
    Source(Image),
    Effect(ReferenceEffectNode),
    Inert(NodeIntrospection),
}

impl EvaluateNode<Image> for RuntimeNode {
    fn dependencies(
        &self,
        request: EvaluationRequest,
        incoming: &[GraphEdge],
    ) -> Result<Vec<EvaluationDependency>> {
        match self {
            Self::Effect(node) => node.dependencies(request, incoming),
            Self::Source(_) | Self::Inert(_) => Ok(Vec::new()),
        }
    }

    fn evaluate(&self, context: &EvaluationContext<'_, Image>) -> Result<Image> {
        match self {
            Self::Source(image) => Ok(image.clone()),
            Self::Effect(node) => node.evaluate(context),
            Self::Inert(_) => Err(Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "inert control node cannot be evaluated as an image",
            )),
        }
    }
}

impl IntrospectNode for RuntimeNode {
    fn introspection(&self) -> NodeIntrospection {
        match self {
            Self::Effect(node) => node.introspection(),
            Self::Source(_) => NodeIntrospection::new(
                schema_id("superi.test.image-source"),
                image_behavior(),
                NodeStateFingerprint::from_canonical_bytes(b"fixture-image-v1"),
            ),
            Self::Inert(value) => value.clone(),
        }
    }
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

fn source_node() -> EditableNode<GraphValue<String>> {
    let image_type = ValueTypeId::new("superi.value.image").unwrap();
    let schema = Arc::new(
        NodeSchema::new(
            schema_id("superi.test.image-source"),
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
        [InstancePort::new(
            SOURCE_OUTPUT,
            PortName::new("result").unwrap(),
        )],
        [],
    )
    .unwrap()
}

fn control_node(value: f64) -> EditableNode<GraphValue<String>> {
    let value_type = ValueTypeId::new("superi.value.scalar").unwrap();
    let name = ParameterName::new("value").unwrap();
    let schema = Arc::new(
        NodeSchema::new(
            schema_id("superi.test.scalar-control"),
            [],
            [],
            [ParameterSchema::new(name.clone(), value_type.clone(), true)],
            NodeBehavior::new(
                TimeBehavior::Invariant,
                RoiBehavior::InputBounds,
                ColorRequirements::NotApplicable,
                Determinism::Deterministic,
                CachePolicy::Static,
            ),
            CapabilitySet::default(),
        )
        .unwrap(),
    );
    EditableNode::new(
        schema,
        [],
        [],
        [EditableParameter::new(
            CONTROL_VALUE,
            name,
            TypedParameterValue::new(value_type, GraphValue::scalar(value).unwrap()),
        )],
    )
    .unwrap()
}

fn effect_node(catalog: &EffectCatalog) -> EditableNode<GraphValue<String>> {
    catalog
        .instantiate(
            EffectNodeKind::Opacity,
            EffectNodeBindings::new(
                [InstancePort::new(
                    EFFECT_INPUT,
                    PortName::new("source").unwrap(),
                )],
                [InstancePort::new(
                    EFFECT_OUTPUT,
                    PortName::new("result").unwrap(),
                )],
                [EffectParameterBinding::new(
                    OPACITY,
                    ParameterName::new("opacity").unwrap(),
                )],
            ),
        )
        .unwrap()
}

fn fixture() -> Image {
    let bounds = PixelBounds::from_origin_size(0, 0, 1, 1).unwrap();
    let descriptor = ImageDescriptor::new(
        bounds,
        bounds,
        PixelFormat::Rgba32Float,
        ColorSpace::ACESCG,
        AlphaMode::Premultiplied,
    )
    .unwrap();
    Image::new(descriptor, ImageSamples::from_f32([1.0, 0.0, 0.0, 1.0])).unwrap()
}

fn graph() -> EditableGraph<GraphValue<String>> {
    let catalog = EffectCatalog::new().unwrap();
    let scalar = ValueTypeId::new("superi.value.scalar").unwrap();
    let expression = ParameterExpression::compile(
        "control * 2",
        [(
            ExpressionVariableName::new("control").unwrap(),
            ParameterReference::new(
                ParameterAddress::new(CONTROL_NODE, CONTROL_VALUE),
                scalar.clone(),
            ),
        )],
    )
    .unwrap();
    let edge = GraphEdge::new(
        EdgeId::from_raw(40),
        GraphEndpoint::new(SOURCE_NODE, SOURCE_OUTPUT),
        GraphEndpoint::new(EFFECT_NODE, EFFECT_INPUT),
    );
    let mut graph = EditableGraph::new(GraphId::from_raw(1));
    graph
        .apply(GraphTransaction::with_mutations(
            0,
            [
                GraphMutation::Add {
                    node_id: SOURCE_NODE,
                    node: source_node(),
                    position: 0,
                },
                GraphMutation::Add {
                    node_id: CONTROL_NODE,
                    node: control_node(0.25),
                    position: 1,
                },
                GraphMutation::Add {
                    node_id: EFFECT_NODE,
                    node: effect_node(&catalog),
                    position: 2,
                },
                GraphMutation::Connect { edge },
                GraphMutation::SetParameterDriver {
                    target: ParameterAddress::new(EFFECT_NODE, OPACITY),
                    driver: ParameterDriver::expression(scalar, expression),
                },
            ],
        ))
        .unwrap();
    graph
}

struct RuntimeCompiler<'a> {
    image: &'a Image,
}

impl NodeCompiler<GraphValue<String>, RuntimeNode> for RuntimeCompiler<'_> {
    fn compile_node(
        &mut self,
        snapshot: &GraphSnapshot<GraphValue<String>>,
        node_id: NodeId,
        node: &EditableNode<GraphValue<String>>,
    ) -> Result<RuntimeNode> {
        match node.schema().id().node_type().as_str() {
            "superi.test.image-source" => Ok(RuntimeNode::Source(self.image.clone())),
            "superi.test.scalar-control" => Ok(RuntimeNode::Inert(NodeIntrospection::new(
                node.schema().id().clone(),
                node.schema().behavior(),
                NodeStateFingerprint::from_canonical_bytes(b"scalar-control"),
            ))),
            _ => compile_reference_node(snapshot, node_id, node).map(RuntimeNode::Effect),
        }
    }
}

fn compile(
    snapshot: GraphSnapshot<GraphValue<String>>,
    image: &Image,
) -> GraphEvaluationSnapshot<GraphValue<String>, RuntimeNode> {
    GraphEvaluationSnapshot::compile(snapshot, RuntimeCompiler { image }).unwrap()
}

fn request() -> EvaluationRequest {
    EvaluationRequest::new(
        GraphEndpoint::new(EFFECT_NODE, EFFECT_OUTPUT),
        RationalTime::zero(Timebase::integer(24).unwrap()),
        PixelBounds::from_origin_size(0, 0, 1, 1).unwrap(),
    )
}

fn red(image: &Image) -> f32 {
    image.samples().float_value(0).unwrap()
}

#[test]
fn one_editable_graph_drives_pixels_expressions_introspection_and_revision_isolation() {
    let image = fixture();
    let mut editable = graph();
    let old_snapshot = editable.snapshot();
    let old = compile(old_snapshot.clone(), &image);

    let first = old.evaluate::<Image>(request()).unwrap();
    assert_eq!(red(first.value()), 0.5);
    let first_inspection = old.inspect::<Image>(request()).unwrap();
    let first_key = first_inspection
        .node(request().key())
        .unwrap()
        .cache_status()
        .key()
        .unwrap();
    let diagnostics = old.evaluate_with_diagnostics::<Image>(request()).unwrap();
    assert_eq!(red(diagnostics.result().value()), 0.5);
    assert_eq!(diagnostics.diagnostics().inspection(), &first_inspection);

    let scalar = ValueTypeId::new("superi.value.scalar").unwrap();
    let new_snapshot = editable
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::SetParameter {
                node_id: CONTROL_NODE,
                parameter_id: CONTROL_VALUE,
                value: TypedParameterValue::new(scalar, GraphValue::scalar(0.5).unwrap()),
            }],
        ))
        .unwrap();
    let current = compile(new_snapshot, &image);
    assert_eq!(current.graph_revision(), 2);
    assert_eq!(
        red(current.evaluate::<Image>(request()).unwrap().value()),
        1.0
    );
    let current_key = current
        .inspect::<Image>(request())
        .unwrap()
        .node(request().key())
        .unwrap()
        .cache_status()
        .key()
        .unwrap();
    assert_ne!(current_key, first_key);

    assert_eq!(old.graph_revision(), 1);
    assert_eq!(red(old.evaluate::<Image>(request()).unwrap().value()), 0.5);
    assert_eq!(
        old_snapshot
            .evaluate_parameter(ParameterAddress::new(EFFECT_NODE, OPACITY))
            .unwrap()
            .value()
            .payload()
            .as_scalar(),
        Some(0.5)
    );
}

#[test]
fn reference_compiler_accepts_only_the_exact_catalog_schema() {
    let catalog = EffectCatalog::new().unwrap();
    let official = catalog.schema(EffectNodeKind::Opacity);
    let schema = Arc::new(
        NodeSchema::new(
            NodeSchemaId::new(
                NodeTypeId::new(EffectNodeKind::Opacity.code()).unwrap(),
                SemanticVersion::new(2, 0, 0),
            ),
            official.inputs().cloned(),
            official.outputs().cloned(),
            official.parameters().cloned(),
            official.behavior(),
            official.required_capabilities().clone(),
        )
        .unwrap(),
    );
    let value_type = ValueTypeId::new("superi.value.scalar").unwrap();
    let node = EditableNode::new(
        schema,
        [InstancePort::new(
            EFFECT_INPUT,
            PortName::new("source").unwrap(),
        )],
        [InstancePort::new(
            EFFECT_OUTPUT,
            PortName::new("result").unwrap(),
        )],
        [EditableParameter::new(
            OPACITY,
            ParameterName::new("opacity").unwrap(),
            TypedParameterValue::new(value_type, GraphValue::<String>::scalar(1.0).unwrap()),
        )],
    )
    .unwrap();
    let mut graph = EditableGraph::new(GraphId::from_raw(99));
    let snapshot = graph
        .apply(GraphTransaction::with_mutations(
            0,
            [GraphMutation::Add {
                node_id: EFFECT_NODE,
                node,
                position: 0,
            }],
        ))
        .unwrap();

    let error = compile_reference_node(&snapshot, EFFECT_NODE, snapshot.node(EFFECT_NODE).unwrap())
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);
}
