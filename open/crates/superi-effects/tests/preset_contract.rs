use std::sync::Arc;

use serde_json::{json, Value};
use superi_core::error::ErrorCategory;
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_effects::authoring::{EffectInstanceBindings, EffectParameterBinding};
use superi_effects::preset::{
    deserialize_effect_preset, serialize_effect_preset, EffectPreset, EffectPresetMigrations,
    EFFECT_PRESET_DOCUMENT_FORMAT_REVISION,
};
use superi_graph::ids::{GraphId, NodeId, ParameterId, PortId};
use superi_graph::missing::{resolve_graph, MissingNodeReason};
use superi_graph::mutate::{
    EditableGraph, GraphMutation, GraphTransaction, InstancePort, TypedParameterValue,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeRegistry, NodeSchema,
    NodeSchemaId, NodeTypeId, ParameterName, ParameterSchema, PortCardinality, PortName,
    PortSchema, RoiBehavior, TimeBehavior, ValueTypeId,
};
use superi_graph::serialize::{deserialize_graph, serialize_graph};
use superi_graph::value::GraphValue;

type PresetValue = GraphValue<String>;

const NODE_ID: NodeId = NodeId::from_raw(500);

fn value_type(value: &str) -> ValueTypeId {
    ValueTypeId::new(value).unwrap()
}

fn port(value: &str) -> PortName {
    PortName::new(value).unwrap()
}

fn parameter(value: &str) -> ParameterName {
    ParameterName::new(value).unwrap()
}

fn effect_schema(
    version: SemanticVersion,
    amount_name: &str,
    include_mix: bool,
) -> Arc<NodeSchema> {
    let image = value_type("superi.value.image");
    let scalar = value_type("superi.value.scalar");
    let choice = value_type("superi.value.choice");
    let mut parameters = vec![
        ParameterSchema::new(parameter(amount_name), scalar, true),
        ParameterSchema::new(parameter("mode"), choice, true),
    ];
    if include_mix {
        parameters.push(ParameterSchema::new(
            parameter("mix"),
            value_type("superi.value.scalar"),
            true,
        ));
    }
    Arc::new(
        NodeSchema::new(
            NodeSchemaId::new(NodeTypeId::new("vendor.effect.film-look").unwrap(), version),
            [PortSchema::new(
                port("source"),
                image.clone(),
                PortCardinality::Single,
            )],
            [PortSchema::new(
                port("output"),
                image,
                PortCardinality::Single,
            )],
            parameters,
            NodeBehavior::new(
                TimeBehavior::CurrentFrame,
                RoiBehavior::InputBounds,
                ColorRequirements::Tagged,
                Determinism::Deterministic,
                CachePolicy::PerRegion,
            ),
            CapabilitySet::default(),
        )
        .unwrap(),
    )
}

fn scalar(value: f64) -> TypedParameterValue<PresetValue> {
    TypedParameterValue::new(
        value_type("superi.value.scalar"),
        PresetValue::scalar(value).unwrap(),
    )
}

fn choice(value: &str) -> TypedParameterValue<PresetValue> {
    TypedParameterValue::new(
        value_type("superi.value.choice"),
        PresetValue::choice(value).unwrap(),
    )
}

fn preset_for(schema: Arc<NodeSchema>) -> EffectPreset<PresetValue> {
    let parameters = schema
        .parameters()
        .map(|declaration| {
            let value = match declaration.name().as_str() {
                "amount" | "intensity" | "strength" => scalar(0.75),
                "mode" => choice("cinematic"),
                "mix" => scalar(1.0),
                other => panic!("unexpected parameter {other}"),
            };
            (declaration.name().clone(), value)
        })
        .collect::<Vec<_>>();
    EffectPreset::new("Warm film", schema, parameters).unwrap()
}

fn bindings(seed: u128, schema: &NodeSchema) -> EffectInstanceBindings {
    EffectInstanceBindings::new(
        [InstancePort::new(
            PortId::from_raw(seed + 1),
            port("source"),
        )],
        [InstancePort::new(
            PortId::from_raw(seed + 2),
            port("output"),
        )],
        schema.parameters().enumerate().map(|(index, declaration)| {
            EffectParameterBinding::new(
                ParameterId::from_raw(seed + 10 + index as u128),
                declaration.name().clone(),
            )
        }),
    )
}

fn graph_with_preset(
    graph_id: GraphId,
    preset: &EffectPreset<PresetValue>,
    seed: u128,
) -> EditableGraph<PresetValue> {
    let node = preset.instantiate(bindings(seed, preset.schema())).unwrap();
    let mut graph = EditableGraph::new(graph_id);
    graph
        .apply(GraphTransaction::with_mutations(
            0,
            [GraphMutation::Add {
                node_id: NODE_ID,
                node,
                position: 0,
            }],
        ))
        .unwrap();
    graph
}

#[test]
fn presets_capture_complete_parameter_schemas_and_reuse_ordinary_editable_nodes() {
    let schema = effect_schema(SemanticVersion::new(1, 0, 0), "amount", false);
    let preset = preset_for(Arc::clone(&schema));
    assert_eq!(preset.label(), "Warm film");
    assert_eq!(preset.schema(), schema.as_ref());
    assert_eq!(preset.parameters().len(), 2);
    assert!(preset
        .schema()
        .parameter(&parameter("amount"))
        .unwrap()
        .is_animatable());

    let first_node = preset.instantiate(bindings(100, &schema)).unwrap();
    let captured = EffectPreset::capture("Captured film", &first_node).unwrap();
    assert_eq!(captured.schema(), schema.as_ref());
    assert_eq!(
        captured.parameter(&parameter("amount")),
        preset.parameter(&parameter("amount"))
    );

    let mut timeline_graph = graph_with_preset(GraphId::from_raw(1), &captured, 1_000);
    let node_graph = graph_with_preset(GraphId::from_raw(2), &captured, 2_000);
    assert_eq!(
        timeline_graph.snapshot().node(NODE_ID).unwrap().schema(),
        node_graph.snapshot().node(NODE_ID).unwrap().schema()
    );

    let amount_id = timeline_graph
        .snapshot()
        .node(NODE_ID)
        .unwrap()
        .parameters()
        .values()
        .find(|value| value.name() == &parameter("amount"))
        .unwrap()
        .id();
    timeline_graph
        .apply(GraphTransaction::with_mutations(
            timeline_graph.snapshot().revision(),
            [GraphMutation::SetParameter {
                node_id: NODE_ID,
                parameter_id: amount_id,
                value: scalar(0.25),
            }],
        ))
        .unwrap();
    assert_eq!(
        timeline_graph
            .snapshot()
            .node(NODE_ID)
            .unwrap()
            .parameter(amount_id)
            .unwrap()
            .value()
            .payload()
            .as_scalar(),
        Some(0.25)
    );
    assert_eq!(
        captured
            .parameter(&parameter("amount"))
            .unwrap()
            .payload()
            .as_scalar(),
        Some(0.75)
    );
}

#[test]
fn preset_documents_are_deterministic_integrity_checked_and_upgrade_legacy_state() {
    let preset = preset_for(effect_schema(
        SemanticVersion::new(1, 0, 0),
        "amount",
        false,
    ));
    let current = serialize_effect_preset(&preset).unwrap();
    assert_eq!(current, serialize_effect_preset(&preset).unwrap());

    let loaded = deserialize_effect_preset::<PresetValue>(&current).unwrap();
    assert_eq!(loaded.preset(), &preset);
    assert_eq!(
        loaded.source_format_revision(),
        EFFECT_PRESET_DOCUMENT_FORMAT_REVISION
    );
    assert!(!loaded.was_migrated());
    assert_eq!(loaded.canonical_document(), current);

    let current_value: Value = serde_json::from_slice(&current).unwrap();
    let legacy = serde_json::to_vec(&json!({
        "format": "superi.effect-preset",
        "format_revision": 0,
        "preset": current_value.get("payload").unwrap(),
    }))
    .unwrap();
    let migrated = deserialize_effect_preset::<PresetValue>(&legacy).unwrap();
    assert!(migrated.was_migrated());
    assert_eq!(migrated.source_format_revision(), 0);
    assert_eq!(migrated.preset(), &preset);
    assert_eq!(migrated.canonical_document(), current);

    let mut corrupted: Value = serde_json::from_slice(&current).unwrap();
    corrupted["payload"]["label"] = json!("Changed without integrity update");
    let error = deserialize_effect_preset::<PresetValue>(&serde_json::to_vec(&corrupted).unwrap())
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);

    let truncated = &current[..current.len() / 2];
    assert_eq!(
        deserialize_effect_preset::<PresetValue>(truncated)
            .unwrap_err()
            .category(),
        ErrorCategory::CorruptData
    );

    let mut unknown: Value = serde_json::from_slice(&current).unwrap();
    unknown["unexpected"] = json!(true);
    assert_eq!(
        deserialize_effect_preset::<PresetValue>(&serde_json::to_vec(&unknown).unwrap())
            .unwrap_err()
            .category(),
        ErrorCategory::CorruptData
    );

    let mut future: Value = serde_json::from_slice(&current).unwrap();
    future["format_revision"] = json!(99);
    assert_eq!(
        deserialize_effect_preset::<PresetValue>(&serde_json::to_vec(&future).unwrap())
            .unwrap_err()
            .category(),
        ErrorCategory::Unsupported
    );
}

#[test]
fn registered_schema_migrations_are_forward_deterministic_and_transactional() {
    let version_one = effect_schema(SemanticVersion::new(1, 0, 0), "amount", false);
    let version_two = effect_schema(SemanticVersion::new(2, 0, 0), "intensity", true);
    let version_three = effect_schema(SemanticVersion::new(3, 0, 0), "strength", true);
    let source = preset_for(Arc::clone(&version_one));

    let mut migrations = EffectPresetMigrations::new();
    migrations
        .register(
            Arc::clone(&version_one),
            Arc::clone(&version_two),
            |preset| {
                Ok(vec![
                    (
                        parameter("intensity"),
                        preset.parameter(&parameter("amount")).unwrap().clone(),
                    ),
                    (
                        parameter("mode"),
                        preset.parameter(&parameter("mode")).unwrap().clone(),
                    ),
                    (parameter("mix"), scalar(1.0)),
                ])
            },
        )
        .unwrap();
    migrations
        .register(
            Arc::clone(&version_two),
            Arc::clone(&version_three),
            |preset| {
                Ok(vec![
                    (
                        parameter("strength"),
                        preset.parameter(&parameter("intensity")).unwrap().clone(),
                    ),
                    (
                        parameter("mode"),
                        preset.parameter(&parameter("mode")).unwrap().clone(),
                    ),
                    (
                        parameter("mix"),
                        preset.parameter(&parameter("mix")).unwrap().clone(),
                    ),
                ])
            },
        )
        .unwrap();

    let migrated = migrations
        .migrate_to(&source, version_three.as_ref())
        .unwrap();
    assert_eq!(migrated.schema(), version_three.as_ref());
    assert_eq!(
        migrated
            .parameter(&parameter("strength"))
            .unwrap()
            .payload()
            .as_scalar(),
        Some(0.75)
    );
    assert_eq!(
        migrated
            .parameter(&parameter("mix"))
            .unwrap()
            .payload()
            .as_scalar(),
        Some(1.0)
    );
    assert_eq!(source.schema(), version_one.as_ref());
    assert!(source.parameter(&parameter("amount")).is_some());

    let unsupported = EffectPresetMigrations::<PresetValue>::new()
        .migrate_to(&source, version_three.as_ref())
        .unwrap_err();
    assert_eq!(unsupported.category(), ErrorCategory::Unsupported);

    let mut invalid = EffectPresetMigrations::new();
    invalid
        .register(version_one, version_two.clone(), |preset| {
            Ok(vec![
                (
                    parameter("intensity"),
                    preset.parameter(&parameter("amount")).unwrap().clone(),
                ),
                (
                    parameter("mode"),
                    preset.parameter(&parameter("mode")).unwrap().clone(),
                ),
            ])
        })
        .unwrap();
    assert_eq!(
        invalid
            .migrate_to(&source, version_two.as_ref())
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        source.schema().id().schema_version(),
        &SemanticVersion::new(1, 0, 0)
    );
}

#[test]
fn absent_plugins_keep_presets_and_graph_nodes_editable_until_exact_schema_returns() {
    let schema = effect_schema(SemanticVersion::new(1, 0, 0), "amount", false);
    let document = serialize_effect_preset(&preset_for(Arc::clone(&schema))).unwrap();
    let loaded = deserialize_effect_preset::<PresetValue>(&document).unwrap();
    let mut graph = graph_with_preset(GraphId::from_raw(50), loaded.preset(), 5_000);
    let graph_document = serialize_graph(&graph.snapshot()).unwrap();
    let recovered = deserialize_graph::<PresetValue>(&graph_document).unwrap();

    let empty_registry = NodeRegistry::new();
    let missing = resolve_graph(&recovered.graph().snapshot(), &empty_registry.snapshot());
    assert_eq!(missing.missing_node_count(), 1);
    let placeholder = missing.node(NODE_ID).unwrap().placeholder().unwrap();
    assert_eq!(placeholder.reason(), MissingNodeReason::UnregisteredSchema);
    assert_eq!(placeholder.schema_id(), schema.id());
    assert_eq!(serialize_graph(missing.graph()).unwrap(), graph_document);

    let amount_id = graph
        .snapshot()
        .node(NODE_ID)
        .unwrap()
        .parameters()
        .values()
        .find(|value| value.name() == &parameter("amount"))
        .unwrap()
        .id();
    graph
        .apply(GraphTransaction::with_mutations(
            graph.snapshot().revision(),
            [GraphMutation::SetParameter {
                node_id: NODE_ID,
                parameter_id: amount_id,
                value: scalar(0.5),
            }],
        ))
        .unwrap();
    let edited_document = serialize_graph(&graph.snapshot()).unwrap();
    let edited = deserialize_graph::<PresetValue>(&edited_document).unwrap();
    assert_eq!(
        edited
            .graph()
            .snapshot()
            .node(NODE_ID)
            .unwrap()
            .parameter(amount_id)
            .unwrap()
            .value()
            .payload()
            .as_scalar(),
        Some(0.5)
    );

    let incompatible = NodeSchema::new(
        schema.id().clone(),
        schema.inputs().cloned(),
        schema.outputs().cloned(),
        schema.parameters().map(|declaration| {
            ParameterSchema::new(
                declaration.name().clone(),
                declaration.value_type().clone(),
                false,
            )
        }),
        schema.behavior(),
        schema.required_capabilities().clone(),
    )
    .unwrap();
    let mut incompatible_registry = NodeRegistry::new();
    incompatible_registry.register(incompatible).unwrap();
    assert_eq!(
        resolve_graph(&graph.snapshot(), &incompatible_registry.snapshot())
            .node(NODE_ID)
            .unwrap()
            .placeholder()
            .unwrap()
            .reason(),
        MissingNodeReason::IncompatibleSchema
    );

    let mut restored_registry = NodeRegistry::new();
    restored_registry.register(schema.as_ref().clone()).unwrap();
    let restored = resolve_graph(&graph.snapshot(), &restored_registry.snapshot());
    assert!(restored.is_evaluable());
    assert_eq!(serialize_graph(restored.graph()).unwrap(), edited_document);
    assert_eq!(serialize_effect_preset(loaded.preset()).unwrap(), document);
}
