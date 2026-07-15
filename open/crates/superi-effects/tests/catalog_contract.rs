use superi_core::error::ErrorCategory;
use superi_effects::authoring::ParameterControl;
use superi_effects::catalog::{
    EffectCatalog, EffectNodeBindings, EffectNodeFamily, EffectNodeKind, EffectParameterBinding,
};
use superi_graph::ids::{GraphId, ParameterId, PortId};
use superi_graph::mutate::{EditableGraph, GraphMutation, GraphTransaction, InstancePort};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeRegistry, ParameterName, PortName,
    RoiBehavior, TimeBehavior,
};
use superi_graph::value::GraphValue;

fn bindings(kind: EffectNodeKind, catalog: &EffectCatalog) -> EffectNodeBindings {
    let schema = catalog.schema(kind);
    EffectNodeBindings::new(
        schema.inputs().enumerate().map(|(index, port)| {
            InstancePort::new(
                PortId::from_raw(100 + index as u128),
                PortName::new(port.name().as_str()).unwrap(),
            )
        }),
        schema.outputs().enumerate().map(|(index, port)| {
            InstancePort::new(
                PortId::from_raw(200 + index as u128),
                PortName::new(port.name().as_str()).unwrap(),
            )
        }),
        schema.parameters().enumerate().map(|(index, parameter)| {
            EffectParameterBinding::new(
                ParameterId::from_raw(300 + index as u128),
                ParameterName::new(parameter.name().as_str()).unwrap(),
            )
        }),
    )
}

#[test]
fn catalog_covers_every_required_operation_in_stable_order() {
    let catalog = EffectCatalog::new().unwrap();
    let expected = [
        "superi.effect.transform",
        "superi.effect.crop",
        "superi.effect.opacity",
        "superi.effect.blend",
        "superi.effect.composite",
        "superi.effect.gaussian-blur",
        "superi.effect.sharpen",
        "superi.effect.radial-distortion",
        "superi.effect.chroma-key",
        "superi.effect.invert",
        "superi.effect.grade",
    ];

    assert_eq!(EffectNodeKind::ALL.len(), expected.len());
    assert_eq!(
        catalog
            .schemas()
            .map(|schema| schema.id().node_type().as_str())
            .collect::<Vec<_>>(),
        expected
    );
    for schema in catalog.schemas() {
        assert_eq!(schema.id().schema_version().to_string(), "1.0.0");
        assert_eq!(schema.outputs().len(), 1);
        assert_eq!(schema.outputs().next().unwrap().name().as_str(), "result");
        assert_eq!(schema.behavior().time(), TimeBehavior::CurrentFrame);
        assert_eq!(
            schema.behavior().color(),
            ColorRequirements::Exact(superi_core::color_space::ColorSpace::ACESCG)
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
}

#[test]
fn families_ports_parameters_and_roi_behavior_are_inspectable() {
    let catalog = EffectCatalog::new().unwrap();
    assert_eq!(
        EffectNodeKind::Transform.family(),
        EffectNodeFamily::Geometry
    );
    assert_eq!(
        EffectNodeKind::Blend.family(),
        EffectNodeFamily::Compositing
    );
    assert_eq!(
        EffectNodeKind::GaussianBlur.family(),
        EffectNodeFamily::Filter
    );
    assert_eq!(EffectNodeKind::ChromaKey.family(), EffectNodeFamily::Keying);
    assert_eq!(EffectNodeKind::Grade.family(), EffectNodeFamily::Utility);

    let transform = catalog.schema(EffectNodeKind::Transform);
    assert_eq!(transform.inputs().next().unwrap().name().as_str(), "source");
    assert_eq!(transform.parameters().len(), 10);
    assert_eq!(transform.behavior().roi(), RoiBehavior::Custom);
    assert!(transform
        .parameters()
        .any(|parameter| parameter.name().as_str() == "m22"));
    assert!(transform
        .parameters()
        .any(|parameter| parameter.name().as_str() == "sampling"));

    let blend = catalog.schema(EffectNodeKind::Blend);
    assert_eq!(
        blend
            .inputs()
            .map(|port| port.name().as_str())
            .collect::<Vec<_>>(),
        ["backdrop", "source"]
    );
    assert_eq!(
        blend
            .parameters()
            .map(|parameter| parameter.name().as_str())
            .collect::<Vec<_>>(),
        ["mode", "opacity"]
    );
    let definition = catalog.definition::<String>(EffectNodeKind::Blend).unwrap();
    assert_eq!(definition.schema(), blend);
    assert_eq!(definition.metadata().label(), "Blend");
    assert_eq!(definition.metadata().category(), "Compositing");
    assert!(definition.metadata().summary().contains("source image"));
    let mode = definition
        .parameter(&ParameterName::new("mode").unwrap())
        .unwrap();
    assert_eq!(mode.control(), ParameterControl::Choice);
    assert_eq!(mode.default_value().payload().as_choice(), Some("normal"));
    let opacity = definition
        .parameter(&ParameterName::new("opacity").unwrap())
        .unwrap();
    assert_eq!(opacity.control(), ParameterControl::Slider);
    assert_eq!(opacity.default_value().payload().as_scalar(), Some(1.0));

    assert_eq!(
        catalog
            .schema(EffectNodeKind::GaussianBlur)
            .behavior()
            .roi(),
        RoiBehavior::Custom
    );
    assert_eq!(
        catalog.schema(EffectNodeKind::Sharpen).behavior().roi(),
        RoiBehavior::Custom
    );
    assert_eq!(
        catalog
            .schema(EffectNodeKind::RadialDistortion)
            .behavior()
            .roi(),
        RoiBehavior::Custom
    );
}

#[test]
fn generic_nodes_retain_typed_defaults_and_publish_atomically() {
    let catalog = EffectCatalog::new().unwrap();
    let kind = EffectNodeKind::Blend;
    let node = catalog
        .instantiate::<String>(kind, bindings(kind, &catalog))
        .unwrap();
    assert_eq!(node.schema(), catalog.schema(kind));
    let mode = node
        .parameters()
        .values()
        .find(|parameter| parameter.name().as_str() == "mode")
        .unwrap();
    assert_eq!(mode.value().payload().as_choice(), Some("normal"));
    let opacity = node
        .parameters()
        .values()
        .find(|parameter| parameter.name().as_str() == "opacity")
        .unwrap();
    assert_eq!(opacity.value().payload().as_scalar(), Some(1.0));

    let mut graph = EditableGraph::<GraphValue<String>>::new(GraphId::from_raw(1));
    let snapshot = graph
        .apply(GraphTransaction::with_mutations(
            0,
            [GraphMutation::Add {
                node_id: superi_graph::ids::NodeId::from_raw(2),
                node,
                position: 0,
            }],
        ))
        .unwrap();
    assert_eq!(snapshot.revision(), 1);
    assert_eq!(snapshot.dag().node_count(), 1);
}

#[test]
fn registration_is_one_atomic_catalog_revision() {
    let catalog = EffectCatalog::new().unwrap();
    let mut registry = NodeRegistry::new();
    catalog.register(&mut registry).unwrap();
    let before = registry.snapshot();
    assert_eq!(before.revision(), 1);
    assert_eq!(before.len(), EffectNodeKind::ALL.len());

    let error = catalog.register(&mut registry).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(registry.snapshot(), before);
}

#[test]
fn incomplete_instance_bindings_fail_without_partial_state() {
    let catalog = EffectCatalog::new().unwrap();
    let kind = EffectNodeKind::Opacity;
    let schema = catalog.schema(kind);
    let incomplete = EffectNodeBindings::new(
        schema.inputs().map(|port| {
            InstancePort::new(
                PortId::from_raw(10),
                PortName::new(port.name().as_str()).unwrap(),
            )
        }),
        schema.outputs().map(|port| {
            InstancePort::new(
                PortId::from_raw(11),
                PortName::new(port.name().as_str()).unwrap(),
            )
        }),
        [],
    );

    let error = catalog.instantiate::<()>(kind, incomplete).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
}
