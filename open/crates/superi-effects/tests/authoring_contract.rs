use std::sync::{Arc, Mutex};

use superi_core::error::ErrorCategory;
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_effects::authoring::{
    EffectCatalog, EffectInstanceBindings, EffectMetadata, EffectNodeCompiler,
    EffectNodeDefinition, EffectParameterBinding, EffectParameterDefinition, EffectPortDefinition,
    ParameterControl,
};
use superi_graph::headless::GraphEvaluationSnapshot;
use superi_graph::ids::{GraphId, NodeId, ParameterId, PortId};
use superi_graph::mutate::{
    EditableGraph, EditableNode, GraphMutation, GraphSnapshot, GraphTransaction, InstancePort,
    TypedParameterValue,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchemaId, NodeTypeId,
    ParameterName, ParameterSchema, PortCardinality, PortName, PortSchema, RoiBehavior,
    TimeBehavior, ValueTypeId,
};

#[derive(Clone, Debug, Eq, PartialEq)]
enum AuthorValue {
    Scalar(i64),
}

fn value_type(value: &str) -> ValueTypeId {
    ValueTypeId::new(value).unwrap()
}

fn port_name(value: &str) -> PortName {
    PortName::new(value).unwrap()
}

fn parameter_name(value: &str) -> ParameterName {
    ParameterName::new(value).unwrap()
}

fn schema_id(node_type: &str) -> NodeSchemaId {
    NodeSchemaId::new(
        NodeTypeId::new(node_type).unwrap(),
        SemanticVersion::new(1, 0, 0),
    )
}

fn effect_definition(node_type: &str, label: &str) -> EffectNodeDefinition<AuthorValue> {
    let image = value_type("superi.value.image");
    let scalar = value_type("superi.value.scalar");
    EffectNodeDefinition::new(
        schema_id(node_type),
        EffectMetadata::new(label, "Adjusts one visual property.", "Adjust").unwrap(),
        [EffectPortDefinition::new(
            PortSchema::new(port_name("source"), image.clone(), PortCardinality::Single),
            "Source",
            "Input image in the current working space.",
        )
        .unwrap()],
        [EffectPortDefinition::new(
            PortSchema::new(port_name("output"), image, PortCardinality::Single),
            "Output",
            "Processed image with unchanged graph ownership.",
        )
        .unwrap()],
        [EffectParameterDefinition::new(
            ParameterSchema::new(parameter_name("amount"), scalar.clone(), true),
            "Amount",
            "Strength of the visual operation.",
            ParameterControl::Slider,
            TypedParameterValue::new(scalar, AuthorValue::Scalar(100)),
        )
        .unwrap()],
        NodeBehavior::new(
            TimeBehavior::CurrentFrame,
            RoiBehavior::InputBounds,
            ColorRequirements::Tagged,
            Determinism::Deterministic,
            CachePolicy::PerRegion,
        ),
        CapabilitySet::default(),
    )
    .unwrap()
}

fn bindings(seed: u128) -> EffectInstanceBindings {
    EffectInstanceBindings::new(
        [InstancePort::new(
            PortId::from_raw(seed + 1),
            port_name("source"),
        )],
        [InstancePort::new(
            PortId::from_raw(seed + 2),
            port_name("output"),
        )],
        [EffectParameterBinding::new(
            ParameterId::from_raw(seed + 3),
            parameter_name("amount"),
        )],
    )
}

fn graph_with_effect(
    definition: &EffectNodeDefinition<AuthorValue>,
    graph_id: GraphId,
    node_id: NodeId,
    seed: u128,
) -> EditableGraph<AuthorValue> {
    let node = definition
        .instantiate(
            bindings(seed),
            Vec::<(ParameterName, TypedParameterValue<AuthorValue>)>::new(),
        )
        .unwrap();
    let mut graph = EditableGraph::new(graph_id);
    graph
        .apply(GraphTransaction::with_mutations(
            0,
            [GraphMutation::Add {
                node_id,
                node,
                position: 0,
            }],
        ))
        .unwrap();
    graph
}

#[test]
fn definitions_are_typed_inspectable_and_discovered_in_canonical_order() {
    let amount = effect_definition("superi.effect.amount", "Amount");

    assert_eq!(amount.metadata().label(), "Amount");
    assert_eq!(amount.metadata().summary(), "Adjusts one visual property.");
    assert_eq!(amount.metadata().category(), "Adjust");
    assert_eq!(amount.inputs().len(), 1);
    assert_eq!(amount.outputs().len(), 1);
    assert_eq!(amount.parameters().len(), 1);
    assert_eq!(
        amount.input(&port_name("source")).unwrap().label(),
        "Source"
    );
    let parameter = amount.parameter(&parameter_name("amount")).unwrap();
    assert_eq!(parameter.label(), "Amount");
    assert_eq!(parameter.control(), ParameterControl::Slider);
    assert!(parameter.schema().is_animatable());
    assert_eq!(
        parameter.default_value().payload(),
        &AuthorValue::Scalar(100)
    );

    let mut catalog = EffectCatalog::new();
    catalog
        .register_batch([
            effect_definition("superi.effect.zebra", "Zebra"),
            amount.clone(),
        ])
        .unwrap();
    let first = catalog.snapshot();
    assert_eq!(first.revision(), 1);
    assert_eq!(first.len(), 2);
    assert_eq!(first.node_schemas().revision(), first.revision());
    assert_eq!(
        first
            .iter()
            .map(|definition| definition.schema().id().node_type().as_str())
            .collect::<Vec<_>>(),
        vec!["superi.effect.amount", "superi.effect.zebra"]
    );
    assert_eq!(
        first.node_schemas().get(amount.schema().id()).unwrap(),
        amount.schema()
    );

    catalog
        .register(effect_definition("superi.effect.blur", "Blur"))
        .unwrap();
    assert_eq!(catalog.snapshot().revision(), 2);
    assert_eq!(catalog.snapshot().len(), 3);
    assert_eq!(first.revision(), 1);
    assert_eq!(first.len(), 2);
    assert!(first.get(&schema_id("superi.effect.blur")).is_none());
}

#[test]
fn one_definition_remains_ordinary_editable_state_in_both_workflows() {
    let definition = effect_definition("superi.effect.amount", "Amount");
    let timeline_node = NodeId::from_raw(101);
    let graph_node = NodeId::from_raw(201);
    let mut timeline = graph_with_effect(&definition, GraphId::from_raw(100), timeline_node, 1_000);
    let mut node_graph = graph_with_effect(&definition, GraphId::from_raw(200), graph_node, 2_000);

    assert_eq!(
        timeline.snapshot().node(timeline_node).unwrap().schema(),
        node_graph.snapshot().node(graph_node).unwrap().schema()
    );
    assert_eq!(
        timeline
            .snapshot()
            .node(timeline_node)
            .unwrap()
            .parameter(ParameterId::from_raw(1_003))
            .unwrap()
            .value()
            .payload(),
        &AuthorValue::Scalar(100)
    );

    let scalar = value_type("superi.value.scalar");
    timeline
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::SetParameter {
                node_id: timeline_node,
                parameter_id: ParameterId::from_raw(1_003),
                value: TypedParameterValue::new(scalar.clone(), AuthorValue::Scalar(25)),
            }],
        ))
        .unwrap();
    node_graph
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::SetParameter {
                node_id: graph_node,
                parameter_id: ParameterId::from_raw(2_003),
                value: TypedParameterValue::new(scalar, AuthorValue::Scalar(25)),
            }],
        ))
        .unwrap();

    let timeline_value = timeline
        .snapshot()
        .node(timeline_node)
        .unwrap()
        .parameter(ParameterId::from_raw(1_003))
        .unwrap()
        .value()
        .payload()
        .clone();
    let graph_value = node_graph
        .snapshot()
        .node(graph_node)
        .unwrap()
        .parameter(ParameterId::from_raw(2_003))
        .unwrap()
        .value()
        .payload()
        .clone();
    assert_eq!(timeline_value, AuthorValue::Scalar(25));
    assert_eq!(timeline_value, graph_value);
}

#[test]
fn one_exact_factory_compiles_timeline_and_node_graph_snapshots() {
    let definition = effect_definition("superi.effect.amount", "Amount");
    let id = definition.schema().id().clone();
    let mut catalog = EffectCatalog::new();
    catalog.register(definition.clone()).unwrap();

    let calls = Arc::new(Mutex::new(Vec::new()));
    let observed = Arc::clone(&calls);
    let mut compiler = EffectNodeCompiler::<AuthorValue, ()>::new(catalog.snapshot());
    compiler
        .register_factory(
            id.clone(),
            move |snapshot: &GraphSnapshot<AuthorValue>,
                  node_id: NodeId,
                  node: &EditableNode<AuthorValue>| {
                assert_eq!(node.schema().id(), &id);
                observed
                    .lock()
                    .unwrap()
                    .push((snapshot.graph_id(), node_id));
                Ok(())
            },
        )
        .unwrap();

    let timeline = graph_with_effect(
        &definition,
        GraphId::from_raw(300),
        NodeId::from_raw(301),
        3_000,
    );
    let node_graph = graph_with_effect(
        &definition,
        GraphId::from_raw(400),
        NodeId::from_raw(401),
        4_000,
    );
    GraphEvaluationSnapshot::compile(timeline.snapshot(), compiler.clone()).unwrap();
    GraphEvaluationSnapshot::compile(node_graph.snapshot(), compiler).unwrap();

    assert_eq!(
        *calls.lock().unwrap(),
        vec![
            (GraphId::from_raw(300), NodeId::from_raw(301)),
            (GraphId::from_raw(400), NodeId::from_raw(401)),
        ]
    );
}

#[test]
fn runtime_compilation_rejects_missing_factories_and_same_id_schema_drift() {
    let definition = effect_definition("superi.effect.amount", "Amount");
    let mut catalog = EffectCatalog::new();
    catalog.register(definition.clone()).unwrap();
    let graph = graph_with_effect(
        &definition,
        GraphId::from_raw(500),
        NodeId::from_raw(501),
        5_000,
    );
    let missing = EffectNodeCompiler::<AuthorValue, ()>::new(catalog.snapshot());
    let missing_error = GraphEvaluationSnapshot::compile(graph.snapshot(), missing).unwrap_err();
    assert_eq!(missing_error.category(), ErrorCategory::Unavailable);

    let drifted = EffectNodeDefinition::new(
        definition.schema().id().clone(),
        EffectMetadata::new("Amount", "Drifted contract.", "Adjust").unwrap(),
        [],
        [],
        [],
        NodeBehavior::new(
            TimeBehavior::Invariant,
            RoiBehavior::FullFrame,
            ColorRequirements::NotApplicable,
            Determinism::Deterministic,
            CachePolicy::PerFrame,
        ),
        CapabilitySet::default(),
    )
    .unwrap();
    let drifted_node = drifted
        .instantiate(
            EffectInstanceBindings::new([], [], []),
            Vec::<(ParameterName, TypedParameterValue<AuthorValue>)>::new(),
        )
        .unwrap();
    let mut drifted_graph = EditableGraph::new(GraphId::from_raw(600));
    drifted_graph
        .apply(GraphTransaction::with_mutations(
            0,
            [GraphMutation::Add {
                node_id: NodeId::from_raw(601),
                node: drifted_node,
                position: 0,
            }],
        ))
        .unwrap();
    let mut compiler = EffectNodeCompiler::<AuthorValue, ()>::new(catalog.snapshot());
    compiler
        .register_factory(
            definition.schema().id().clone(),
            |_snapshot: &GraphSnapshot<AuthorValue>,
             _node_id: NodeId,
             _node: &EditableNode<AuthorValue>| Ok(()),
        )
        .unwrap();
    let drift_error =
        GraphEvaluationSnapshot::compile(drifted_graph.snapshot(), compiler).unwrap_err();
    assert_eq!(drift_error.category(), ErrorCategory::InvalidInput);
}

#[test]
fn invalid_authoring_registration_and_instantiation_are_atomic() {
    let metadata_error = EffectMetadata::new(" ", "summary", "Adjust").unwrap_err();
    assert_eq!(metadata_error.category(), ErrorCategory::InvalidInput);

    let scalar = value_type("superi.value.scalar");
    let mistyped_default = EffectParameterDefinition::new(
        ParameterSchema::new(parameter_name("amount"), scalar, true),
        "Amount",
        "Strength.",
        ParameterControl::Slider,
        TypedParameterValue::new(value_type("superi.value.boolean"), AuthorValue::Scalar(1)),
    )
    .unwrap_err();
    assert_eq!(mistyped_default.category(), ErrorCategory::InvalidInput);

    let definition = effect_definition("superi.effect.amount", "Amount");
    let duplicate = definition.clone();
    let mut catalog = EffectCatalog::new();
    let registration_error = catalog
        .register_batch([definition.clone(), duplicate])
        .unwrap_err();
    assert_eq!(registration_error.category(), ErrorCategory::Conflict);
    assert_eq!(catalog.snapshot().revision(), 0);
    assert!(catalog.snapshot().is_empty());

    catalog.register(definition.clone()).unwrap();
    let before = catalog.snapshot();
    let duplicate_error = catalog.register(definition.clone()).unwrap_err();
    assert_eq!(duplicate_error.category(), ErrorCategory::Conflict);
    assert_eq!(catalog.snapshot().revision(), before.revision());
    assert_eq!(catalog.snapshot().len(), before.len());

    let missing_binding = EffectInstanceBindings::new(
        [InstancePort::new(PortId::from_raw(1), port_name("source"))],
        [InstancePort::new(PortId::from_raw(2), port_name("output"))],
        [],
    );
    let binding_error = definition
        .instantiate(
            missing_binding,
            Vec::<(ParameterName, TypedParameterValue<AuthorValue>)>::new(),
        )
        .unwrap_err();
    assert_eq!(binding_error.category(), ErrorCategory::InvalidInput);

    let override_error = definition
        .instantiate(
            bindings(5_000),
            [(
                parameter_name("unknown"),
                TypedParameterValue::new(value_type("superi.value.scalar"), AuthorValue::Scalar(1)),
            )],
        )
        .unwrap_err();
    assert_eq!(override_error.category(), ErrorCategory::InvalidInput);

    let mut compiler = EffectNodeCompiler::<AuthorValue, ()>::new(catalog.snapshot());
    let unknown_factory = compiler
        .register_factory(
            schema_id("superi.effect.unknown"),
            |_snapshot: &GraphSnapshot<AuthorValue>,
             _node_id: NodeId,
             _node: &EditableNode<AuthorValue>| Ok(()),
        )
        .unwrap_err();
    assert_eq!(unknown_factory.category(), ErrorCategory::NotFound);
    compiler
        .register_factory(
            definition.schema().id().clone(),
            |_snapshot: &GraphSnapshot<AuthorValue>,
             _node_id: NodeId,
             _node: &EditableNode<AuthorValue>| Ok(()),
        )
        .unwrap();
    let duplicate_factory = compiler
        .register_factory(
            definition.schema().id().clone(),
            |_snapshot: &GraphSnapshot<AuthorValue>,
             _node_id: NodeId,
             _node: &EditableNode<AuthorValue>| Ok(()),
        )
        .unwrap_err();
    assert_eq!(duplicate_factory.category(), ErrorCategory::Conflict);
}

#[test]
fn authoring_catalog_and_compiler_are_shareable() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<EffectCatalog<AuthorValue>>();
    assert_send_sync::<EffectNodeCompiler<AuthorValue, ()>>();
}
