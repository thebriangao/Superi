use std::str::FromStr;

use superi_core::color_space::ColorSpace;
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::settings::{CapabilityId, CapabilitySet, SemanticVersion};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeRegistry, NodeRegistrySnapshot,
    NodeSchema, NodeSchemaId, NodeTypeId, ParameterName, ParameterSchema, PortCardinality,
    PortName, PortSchema, RoiBehavior, TimeBehavior, ValueTypeId,
};

fn version(input: &str) -> SemanticVersion {
    SemanticVersion::from_str(input).expect("valid semantic version")
}

fn node_type(input: &str) -> NodeTypeId {
    NodeTypeId::from_str(input).expect("valid node type")
}

fn value_type(input: &str) -> ValueTypeId {
    ValueTypeId::from_str(input).expect("valid value type")
}

fn capability(input: &str) -> CapabilityId {
    CapabilityId::from_str(input).expect("valid capability")
}

fn behavior() -> NodeBehavior {
    NodeBehavior::new(
        TimeBehavior::FrameWindow {
            frames_before: 1,
            frames_after: 1,
        },
        RoiBehavior::Expanded { pixels: 8 },
        ColorRequirements::Exact(ColorSpace::ACESCG),
        Determinism::Deterministic,
        CachePolicy::PerRegion,
    )
}

fn schema(node: &str, release: &str) -> NodeSchema {
    let image = value_type("superi.value.image");
    NodeSchema::new(
        NodeSchemaId::new(node_type(node), version(release)),
        [PortSchema::new(
            PortName::new("source").unwrap(),
            image.clone(),
            PortCardinality::Single,
        )],
        [PortSchema::new(
            PortName::new("result").unwrap(),
            image,
            PortCardinality::Single,
        )],
        [ParameterSchema::new(
            ParameterName::new("radius").unwrap(),
            value_type("superi.value.scalar"),
            true,
        )],
        behavior(),
        CapabilitySet::new([capability("superi.render.gpu")]),
    )
    .unwrap()
}

#[test]
fn schema_identity_and_fields_are_typed_complete_and_inspectable() {
    assert!(NodeTypeId::new("blur").is_err());
    assert!(NodeTypeId::new("Superi.blur").is_err());
    assert!(ValueTypeId::new("superi..image").is_err());
    assert!(PortName::new("Source").is_err());
    assert!(PortName::new("source.image").is_err());
    assert!(ParameterName::new("2radius").is_err());

    let schema = schema("superi.effect.blur", "2.1.0");
    assert_eq!(schema.id().node_type().as_str(), "superi.effect.blur");
    assert_eq!(schema.id().schema_version(), &version("2.1.0"));
    assert_eq!(schema.id().to_string(), "superi.effect.blur@2.1.0");

    let input = schema.input(&PortName::new("source").unwrap()).unwrap();
    assert_eq!(input.value_type().as_str(), "superi.value.image");
    assert_eq!(input.cardinality(), PortCardinality::Single);
    assert_eq!(
        schema
            .inputs()
            .map(|port| port.name().as_str())
            .collect::<Vec<_>>(),
        ["source"]
    );
    assert_eq!(
        schema
            .outputs()
            .map(|port| port.name().as_str())
            .collect::<Vec<_>>(),
        ["result"]
    );

    let parameter = schema
        .parameter(&ParameterName::new("radius").unwrap())
        .unwrap();
    assert_eq!(parameter.value_type().as_str(), "superi.value.scalar");
    assert!(parameter.is_animatable());
    assert_eq!(
        schema
            .parameters()
            .map(|parameter| parameter.name().as_str())
            .collect::<Vec<_>>(),
        ["radius"]
    );

    assert_eq!(
        schema.behavior().time(),
        TimeBehavior::FrameWindow {
            frames_before: 1,
            frames_after: 1,
        }
    );
    assert_eq!(schema.behavior().roi(), RoiBehavior::Expanded { pixels: 8 });
    assert_eq!(
        schema.behavior().color(),
        ColorRequirements::Exact(ColorSpace::ACESCG)
    );
    assert_eq!(schema.behavior().determinism(), Determinism::Deterministic);
    assert_eq!(schema.behavior().cache_policy(), CachePolicy::PerRegion);
    assert_eq!(
        schema
            .required_capabilities()
            .iter()
            .map(CapabilityId::as_str)
            .collect::<Vec<_>>(),
        ["superi.render.gpu"]
    );
}

#[test]
fn registration_discovers_versions_in_one_deterministic_order() {
    let mut registry = NodeRegistry::new();
    registry
        .register_batch([
            schema("vendor.effect.zeta", "1.0.0"),
            schema("superi.effect.blur", "2.0.0+metal"),
            schema("superi.effect.blur", "1.5.0"),
            schema("superi.effect.blur", "2.0.0+cpu"),
        ])
        .unwrap();

    let snapshot = registry.snapshot();
    assert_eq!(snapshot.revision(), 1);
    assert_eq!(snapshot.len(), 4);
    assert_eq!(
        snapshot
            .iter()
            .map(|schema| schema.id().to_string())
            .collect::<Vec<_>>(),
        [
            "superi.effect.blur@1.5.0",
            "superi.effect.blur@2.0.0+cpu",
            "superi.effect.blur@2.0.0+metal",
            "vendor.effect.zeta@1.0.0",
        ]
    );

    let blur = node_type("superi.effect.blur");
    assert_eq!(
        snapshot
            .versions(&blur)
            .map(|schema| schema.id().schema_version().to_string())
            .collect::<Vec<_>>(),
        ["1.5.0", "2.0.0+cpu", "2.0.0+metal"]
    );
    assert_eq!(
        snapshot.latest(&blur).unwrap().id().schema_version(),
        &version("2.0.0+metal")
    );
    assert_eq!(
        snapshot
            .get(&NodeSchemaId::new(blur, version("2.0.0+cpu")))
            .unwrap()
            .id()
            .to_string(),
        "superi.effect.blur@2.0.0+cpu"
    );
}

#[test]
fn snapshots_are_immutable_and_identical_for_every_reader() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<NodeRegistrySnapshot>();

    let mut registry = NodeRegistry::new();
    registry
        .register(schema("superi.effect.blur", "1.0.0"))
        .unwrap();
    let original = registry.snapshot();

    registry
        .register(schema("superi.effect.blur", "2.0.0"))
        .unwrap();
    let current = registry.snapshot();
    assert_eq!(original.revision(), 1);
    assert_eq!(original.len(), 1);
    assert_eq!(current.revision(), 2);
    assert_eq!(current.len(), 2);

    let observed = std::thread::scope(|scope| {
        ["editor", "script", "headless"]
            .into_iter()
            .map(|_| {
                let snapshot = current.clone();
                scope.spawn(move || {
                    snapshot
                        .iter()
                        .map(|schema| schema.id().to_string())
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(observed[0], observed[1]);
    assert_eq!(observed[1], observed[2]);
}

#[test]
fn failed_registration_is_actionable_and_never_partially_mutates() {
    let mut registry = NodeRegistry::new();
    registry
        .register(schema("superi.effect.blur", "1.0.0"))
        .unwrap();
    let before = registry.snapshot();

    let error = registry
        .register_batch([
            schema("superi.effect.sharpen", "1.0.0"),
            schema("superi.effect.blur", "1.0.0"),
        ])
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(
        error.contexts()[0].component(),
        "superi-graph.node-registry"
    );
    assert_eq!(error.contexts()[0].operation(), "register_batch");
    assert_eq!(
        error.contexts()[0].field("schema_id"),
        Some("superi.effect.blur@1.0.0")
    );
    assert_eq!(registry.snapshot(), before);
    assert!(registry
        .snapshot()
        .latest(&node_type("superi.effect.sharpen"))
        .is_none());

    let duplicate = schema("superi.effect.glow", "1.0.0");
    let error = registry
        .register_batch([duplicate.clone(), duplicate])
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(registry.snapshot(), before);
}

#[test]
fn duplicate_schema_fields_are_rejected_before_registration() {
    let port = PortSchema::new(
        PortName::new("source").unwrap(),
        value_type("superi.value.image"),
        PortCardinality::Single,
    );
    let parameter = ParameterSchema::new(
        ParameterName::new("radius").unwrap(),
        value_type("superi.value.scalar"),
        true,
    );
    let error = NodeSchema::new(
        NodeSchemaId::new(node_type("superi.effect.blur"), version("1.0.0")),
        [port.clone(), port],
        [],
        [parameter.clone(), parameter],
        behavior(),
        CapabilitySet::default(),
    )
    .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(error.contexts()[0].component(), "superi-graph.node-schema");
    assert_eq!(error.contexts()[0].operation(), "create");
    assert_eq!(error.contexts()[0].field("collection"), Some("inputs"));
    assert_eq!(error.contexts()[0].field("field"), Some("source"));
}
