use serde::{Deserialize, Serialize};
use superi_core::error::ErrorCategory;
use superi_graph::expr::ExpressionParameterValue;
use superi_graph::node::ValueTypeId;
use superi_graph::value::{FiniteF64, GraphValue};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
struct DomainValue {
    label: String,
}

fn value_type(input: &str) -> ValueTypeId {
    ValueTypeId::new(input).unwrap()
}

#[test]
fn finite_values_preserve_exact_binary64_state() {
    let negative_zero = FiniteF64::new(-0.0).unwrap();
    let precise = FiniteF64::new(f64::from_bits(0x3fd5_5555_5555_5555)).unwrap();

    assert_eq!(negative_zero.to_bits(), (-0.0_f64).to_bits());
    assert!(negative_zero.get().is_sign_negative());
    assert_eq!(precise.get().to_bits(), 0x3fd5_5555_5555_5555);
    assert_ne!(FiniteF64::new(0.0).unwrap(), negative_zero);
    assert_eq!(FiniteF64::from_bits(precise.to_bits()).unwrap(), precise);

    assert_eq!(
        FiniteF64::new(f64::NAN).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        FiniteF64::new(f64::INFINITY).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        FiniteF64::from_bits(f64::NEG_INFINITY.to_bits())
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
}

#[test]
fn graph_values_round_trip_without_erasing_variant_or_bits() {
    let values = [
        GraphValue::domain(DomainValue {
            label: "timeline clip".to_owned(),
        }),
        GraphValue::scalar(-0.0).unwrap(),
        GraphValue::vec2([1.0, -2.0]).unwrap(),
        GraphValue::vec3([1.0, 2.0, 3.0]).unwrap(),
        GraphValue::color([0.25, -0.5, 4.0, 1.0]).unwrap(),
        GraphValue::matrix3([1.0, 0.0, 3.0, 0.0, 1.0, 4.0, 0.0, 0.0, 1.0]).unwrap(),
        GraphValue::boolean(true),
        GraphValue::choice("source-over").unwrap(),
    ];

    for value in values {
        let encoded = serde_json::to_vec(&value).unwrap();
        let decoded: GraphValue<DomainValue> = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, value);
    }

    let negative_zero = GraphValue::<DomainValue>::scalar(-0.0).unwrap();
    assert_eq!(
        negative_zero.as_scalar().unwrap().to_bits(),
        (-0.0_f64).to_bits()
    );
    assert_eq!(
        GraphValue::domain(DomainValue {
            label: "domain".to_owned(),
        })
        .as_domain()
        .unwrap()
        .label,
        "domain"
    );
    assert_eq!(
        GraphValue::<DomainValue>::choice("multiply")
            .unwrap()
            .as_choice(),
        Some("multiply")
    );
}

#[test]
fn validated_deserialization_rejects_invalid_numeric_and_choice_state() {
    let nonfinite_bits = format!(r#"{{"scalar":{}}}"#, f64::NAN.to_bits());
    assert!(serde_json::from_str::<GraphValue<DomainValue>>(&nonfinite_bits).is_err());

    let oversized = "x".repeat(257);
    let choice = serde_json::json!({"choice": oversized});
    assert!(serde_json::from_value::<GraphValue<DomainValue>>(choice).is_err());
}

#[test]
fn expressions_accept_only_the_explicit_scalar_variant() {
    let scalar_type = value_type("superi.value.effect.scalar");
    let scalar = GraphValue::<DomainValue>::scalar(2.5).unwrap();
    assert_eq!(
        scalar.to_expression_scalar(&scalar_type).unwrap().to_bits(),
        2.5_f64.to_bits()
    );
    assert_eq!(
        GraphValue::<DomainValue>::from_expression_scalar(&scalar_type, -0.0)
            .unwrap()
            .as_scalar()
            .unwrap()
            .to_bits(),
        (-0.0_f64).to_bits()
    );

    for value in [
        GraphValue::boolean(true),
        GraphValue::choice("normal").unwrap(),
        GraphValue::color([1.0, 0.0, 0.0, 1.0]).unwrap(),
        GraphValue::domain(DomainValue {
            label: "not numeric".to_owned(),
        }),
    ] {
        let error = value.to_expression_scalar(&scalar_type).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
    }

    assert_eq!(
        GraphValue::<DomainValue>::from_expression_scalar(&scalar_type, f64::NAN)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
}
