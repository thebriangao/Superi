use std::str::FromStr;

use superi_core::color_space::ColorSpace;
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchema, NodeSchemaId,
    NodeTypeId, PortCardinality, PortName, PortSchema, RoiBehavior, TimeBehavior, ValueTypeId,
};
use superi_graph::validation::{
    validate_connection, validate_inputs, validate_outputs, PortBinding, PortDirection,
    TypedPortValue, ValidatedPortBindings,
};

fn value_type(input: &str) -> ValueTypeId {
    ValueTypeId::from_str(input).expect("valid value type")
}

fn port(input: &str) -> PortName {
    PortName::new(input).expect("valid port name")
}

fn schema(node_type: &str, inputs: Vec<PortSchema>, outputs: Vec<PortSchema>) -> NodeSchema {
    NodeSchema::new(
        NodeSchemaId::new(
            NodeTypeId::from_str(node_type).expect("valid node type"),
            SemanticVersion::new(1, 0, 0),
        ),
        inputs,
        outputs,
        [],
        NodeBehavior::new(
            TimeBehavior::CurrentFrame,
            RoiBehavior::InputBounds,
            ColorRequirements::Exact(ColorSpace::ACESCG),
            Determinism::Deterministic,
            CachePolicy::PerRegion,
        ),
        CapabilitySet::default(),
    )
    .expect("valid schema")
}

fn effect_schema() -> NodeSchema {
    schema(
        "superi.effect.composite",
        vec![
            PortSchema::new(
                port("image"),
                value_type("superi.value.image"),
                PortCardinality::Single,
            ),
            PortSchema::new(
                port("mask"),
                value_type("superi.value.mask"),
                PortCardinality::Optional,
            ),
            PortSchema::new(
                port("layers"),
                value_type("superi.value.image"),
                PortCardinality::Variadic,
            ),
        ],
        vec![
            PortSchema::new(
                port("image"),
                value_type("superi.value.image"),
                PortCardinality::Single,
            ),
            PortSchema::new(
                port("analysis"),
                value_type("superi.value.scalar"),
                PortCardinality::Optional,
            ),
        ],
    )
}

fn typed<T>(value_type_id: &str, payload: T) -> TypedPortValue<T> {
    TypedPortValue::new(value_type(value_type_id), payload)
}

#[test]
fn input_validation_is_canonical_without_touching_opaque_payloads() {
    struct OpaquePayload {
        sequence: u32,
    }

    let validated = validate_inputs(
        &effect_schema(),
        [
            PortBinding::new(
                port("layers"),
                [
                    typed("superi.value.image", OpaquePayload { sequence: 20 }),
                    typed("superi.value.image", OpaquePayload { sequence: 10 }),
                ],
            ),
            PortBinding::new(
                port("image"),
                [typed("superi.value.image", OpaquePayload { sequence: 1 })],
            ),
        ],
    )
    .expect("valid input bindings");

    assert_eq!(validated.direction(), PortDirection::Input);
    assert_eq!(validated.len(), 3);
    assert_eq!(
        validated
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>(),
        ["image", "layers", "mask"]
    );
    assert!(validated.get(&port("mask")).unwrap().is_empty());
    assert_eq!(
        validated
            .get(&port("layers"))
            .unwrap()
            .iter()
            .map(|value| value.payload().sequence)
            .collect::<Vec<_>>(),
        [20, 10]
    );
}

#[test]
fn input_failures_are_actionable_and_caller_correctable() {
    let schema = effect_schema();

    let missing = validate_inputs::<u32>(&schema, []).unwrap_err();
    assert_input_error(&missing, "image", "single", "0");

    let wrong_type = validate_inputs(
        &schema,
        [PortBinding::new(
            port("image"),
            [typed("superi.value.mask", 1_u32)],
        )],
    )
    .unwrap_err();
    assert_eq!(
        wrong_type.contexts()[0].field("expected_type"),
        Some("superi.value.image")
    );
    assert_eq!(
        wrong_type.contexts()[0].field("actual_type"),
        Some("superi.value.mask")
    );
    assert_eq!(wrong_type.contexts()[0].field("value_index"), Some("0"));

    let too_many = validate_inputs(
        &schema,
        [
            PortBinding::new(port("image"), [typed("superi.value.image", 1_u32)]),
            PortBinding::new(
                port("mask"),
                [
                    typed("superi.value.mask", 2_u32),
                    typed("superi.value.mask", 3_u32),
                ],
            ),
        ],
    )
    .unwrap_err();
    assert_input_error(&too_many, "mask", "optional", "2");

    let duplicate = validate_inputs(
        &schema,
        [
            PortBinding::new(port("image"), [typed("superi.value.image", 1_u32)]),
            PortBinding::new(port("image"), [typed("superi.value.image", 2_u32)]),
        ],
    )
    .unwrap_err();
    assert_eq!(duplicate.contexts()[0].field("port"), Some("image"));
    assert_eq!(
        duplicate.contexts()[0].field("reason"),
        Some("duplicate_binding")
    );

    let unknown = validate_inputs(
        &schema,
        [
            PortBinding::new(port("image"), [typed("superi.value.image", 1_u32)]),
            PortBinding::new(port("depth"), [typed("superi.value.scalar", 2_u32)]),
        ],
    )
    .unwrap_err();
    assert_eq!(unknown.contexts()[0].field("port"), Some("depth"));
    assert_eq!(unknown.contexts()[0].field("reason"), Some("unknown_port"));
}

fn assert_input_error(
    error: &superi_core::error::Error,
    port_name: &str,
    cardinality: &str,
    count: &str,
) {
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(
        error.contexts()[0].component(),
        "superi-graph.port-validation"
    );
    assert_eq!(error.contexts()[0].operation(), "validate_inputs");
    assert_eq!(error.contexts()[0].field("direction"), Some("input"));
    assert_eq!(error.contexts()[0].field("port"), Some(port_name));
    assert_eq!(error.contexts()[0].field("cardinality"), Some(cardinality));
    assert_eq!(error.contexts()[0].field("actual_count"), Some(count));
}

#[test]
fn invalid_outputs_are_terminal_implementation_failures() {
    let schema = effect_schema();
    let missing = validate_outputs::<u32>(&schema, []).unwrap_err();
    assert_eq!(missing.category(), ErrorCategory::Internal);
    assert_eq!(missing.recoverability(), Recoverability::Terminal);
    assert_eq!(missing.contexts()[0].field("port"), Some("image"));
    assert_eq!(
        missing.contexts()[0].field("reason"),
        Some("invalid_cardinality")
    );
    assert_eq!(missing.contexts()[0].field("actual_count"), Some("0"));

    let error = validate_outputs(
        &schema,
        [PortBinding::new(
            port("image"),
            [typed("superi.value.mask", 1_u32)],
        )],
    )
    .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::Internal);
    assert_eq!(error.recoverability(), Recoverability::Terminal);
    assert_eq!(
        error.contexts()[0].component(),
        "superi-graph.port-validation"
    );
    assert_eq!(error.contexts()[0].operation(), "validate_outputs");
    assert_eq!(error.contexts()[0].field("direction"), Some("output"));
    assert_eq!(error.contexts()[0].field("port"), Some("image"));
    assert_eq!(
        error.contexts()[0].field("expected_type"),
        Some("superi.value.image")
    );
    assert_eq!(
        error.contexts()[0].field("actual_type"),
        Some("superi.value.mask")
    );
}

#[test]
fn connection_validation_preserves_direction_and_exact_type_identity() {
    let source = effect_schema();
    let matching_target = schema(
        "superi.output.preview",
        vec![PortSchema::new(
            port("source"),
            value_type("superi.value.image"),
            PortCardinality::Single,
        )],
        vec![],
    );
    validate_connection(&source, &port("image"), &matching_target, &port("source"))
        .expect("compatible connection");

    let mismatched_target = schema(
        "superi.output.matte",
        vec![PortSchema::new(
            port("source"),
            value_type("superi.value.mask"),
            PortCardinality::Single,
        )],
        vec![],
    );
    let mismatch =
        validate_connection(&source, &port("image"), &mismatched_target, &port("source"))
            .unwrap_err();
    assert_eq!(mismatch.category(), ErrorCategory::InvalidInput);
    assert_eq!(mismatch.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(mismatch.contexts()[0].operation(), "validate_connection");
    assert_eq!(mismatch.contexts()[0].field("source_port"), Some("image"));
    assert_eq!(
        mismatch.contexts()[0].field("source_type"),
        Some("superi.value.image")
    );
    assert_eq!(mismatch.contexts()[0].field("target_port"), Some("source"));
    assert_eq!(
        mismatch.contexts()[0].field("target_type"),
        Some("superi.value.mask")
    );

    let unknown = validate_connection(&source, &port("missing"), &matching_target, &port("source"))
        .unwrap_err();
    assert_eq!(
        unknown.contexts()[0].field("reason"),
        Some("unknown_source_output")
    );

    let unknown = validate_connection(&source, &port("image"), &matching_target, &port("missing"))
        .unwrap_err();
    assert_eq!(
        unknown.contexts()[0].field("reason"),
        Some("unknown_target_input")
    );
    assert_eq!(
        unknown.contexts()[0].field("source_type"),
        Some("superi.value.image")
    );
}

#[test]
fn every_reader_observes_the_same_validated_result() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ValidatedPortBindings<u32>>();

    let schema = effect_schema();
    let observed = std::thread::scope(|scope| {
        ["editor", "script", "headless"]
            .into_iter()
            .map(|_| {
                let schema = schema.clone();
                scope.spawn(move || {
                    validate_inputs(
                        &schema,
                        [
                            PortBinding::new(
                                port("layers"),
                                [
                                    typed("superi.value.image", 2_u32),
                                    typed("superi.value.image", 3_u32),
                                ],
                            ),
                            PortBinding::new(port("image"), [typed("superi.value.image", 1_u32)]),
                        ],
                    )
                    .unwrap()
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
