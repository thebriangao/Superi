use std::str::FromStr;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_graph::expr::{
    ExpressionInstruction, ExpressionParameterValue, ExpressionVariableName, ParameterAddress,
    ParameterDriver, ParameterExpression, ParameterReference, ScalarExpression,
};
use superi_graph::ids::{GraphId, NodeId, ParameterId};
use superi_graph::mutate::{
    EditableGraph, EditableNode, EditableParameter, GraphMutation, GraphTransaction,
    TypedParameterValue,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchema, NodeSchemaId,
    NodeTypeId, ParameterName, ParameterSchema, RoiBehavior, TimeBehavior, ValueTypeId,
};
use superi_graph::serialize::{deserialize_graph, serialize_graph};

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize)]
struct Scalar(f64);

impl ExpressionParameterValue for Scalar {
    fn to_expression_scalar(&self, _value_type: &ValueTypeId) -> Result<f64> {
        Ok(self.0)
    }

    fn from_expression_scalar(_value_type: &ValueTypeId, value: f64) -> Result<Self> {
        Ok(Self(value))
    }
}

fn value_type(input: &str) -> ValueTypeId {
    ValueTypeId::from_str(input).expect("valid value type")
}

fn parameter(input: &str) -> ParameterName {
    ParameterName::new(input).expect("valid parameter name")
}

fn variable(input: &str) -> ExpressionVariableName {
    ExpressionVariableName::new(input).expect("valid variable")
}

fn address(node: u128, parameter: u128) -> ParameterAddress {
    ParameterAddress::new(NodeId::from_raw(node), ParameterId::from_raw(parameter))
}

fn reference(node: u128, parameter: u128, value_type_name: &str) -> ParameterReference {
    ParameterReference::new(address(node, parameter), value_type(value_type_name))
}

fn scalar_node(node_type: &str, parameter_id: u128, initial: f64) -> EditableNode<Scalar> {
    let scalar_type = value_type("superi.value.scalar");
    let schema = Arc::new(
        NodeSchema::new(
            NodeSchemaId::new(
                NodeTypeId::from_str(node_type).expect("valid node type"),
                SemanticVersion::new(1, 0, 0),
            ),
            [],
            [],
            [ParameterSchema::new(
                parameter("value"),
                scalar_type.clone(),
                true,
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
        .expect("valid schema"),
    );
    EditableNode::new(
        schema,
        [],
        [],
        [EditableParameter::new(
            ParameterId::from_raw(parameter_id),
            parameter("value"),
            TypedParameterValue::new(scalar_type, Scalar(initial)),
        )],
    )
    .expect("valid scalar node")
}

fn four_node_graph() -> EditableGraph<Scalar> {
    let mut graph = EditableGraph::new(GraphId::from_raw(70));
    graph
        .apply(GraphTransaction::with_mutations(
            0,
            [
                GraphMutation::Add {
                    node_id: NodeId::from_raw(1),
                    node: scalar_node("superi.expression.source_a", 101, 4.0),
                    position: 0,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(2),
                    node: scalar_node("superi.expression.source_b", 201, 3.0),
                    position: 1,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(3),
                    node: scalar_node("superi.expression.linked", 301, 0.0),
                    position: 2,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(4),
                    node: scalar_node("superi.expression.computed", 401, 0.0),
                    position: 3,
                },
            ],
        ))
        .expect("seed graph");
    graph
}

fn expression() -> ParameterExpression {
    ParameterExpression::compile(
        "linked * 2 + offset",
        [
            (variable("offset"), reference(2, 201, "superi.value.scalar")),
            (variable("linked"), reference(3, 301, "superi.value.scalar")),
        ],
    )
    .expect("valid expression")
}

#[test]
fn scalar_expression_reuses_the_bounded_language_without_graph_bindings() {
    let expression =
        ScalarExpression::compile("value + time * 2", [variable("time"), variable("value")])
            .expect("valid scalar expression");

    assert_eq!(expression.source(), "value + time * 2");
    assert_eq!(
        expression
            .variables()
            .iter()
            .map(ExpressionVariableName::as_str)
            .collect::<Vec<_>>(),
        ["time", "value"]
    );
    assert_eq!(
        expression
            .evaluate_with(|name| match name.as_str() {
                "time" => Ok(1.5),
                "value" => Ok(4.0),
                _ => unreachable!("compiled expression exposes only allowed variables"),
            })
            .unwrap(),
        7.0
    );

    let value_only =
        ScalarExpression::compile("value * 3", [variable("time"), variable("value")]).unwrap();
    assert_eq!(
        value_only
            .variables()
            .iter()
            .map(ExpressionVariableName::as_str)
            .collect::<Vec<_>>(),
        ["value"]
    );

    let missing = ScalarExpression::compile("other + 1", [variable("value")]).unwrap_err();
    assert_eq!(
        missing.contexts()[0].field("reason"),
        Some("missing_variable_binding")
    );
}

#[test]
fn expression_source_and_checked_program_are_editable_and_inspectable() {
    let expression = expression();

    assert_eq!(expression.source(), "linked * 2 + offset");
    assert_eq!(
        expression
            .variables()
            .keys()
            .map(ExpressionVariableName::as_str)
            .collect::<Vec<_>>(),
        ["linked", "offset"]
    );
    assert_eq!(
        expression.instructions(),
        [
            ExpressionInstruction::Variable(variable("linked")),
            ExpressionInstruction::Constant("2".into()),
            ExpressionInstruction::Multiply,
            ExpressionInstruction::Variable(variable("offset")),
            ExpressionInstruction::Add,
        ]
    );
}

#[test]
fn expression_compilation_rejects_ambiguous_or_unbounded_programs() {
    let missing = ParameterExpression::compile("missing + 1", []).unwrap_err();
    assert_eq!(missing.category(), ErrorCategory::InvalidInput);
    assert_eq!(missing.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(
        missing.contexts()[0].field("reason"),
        Some("missing_variable_binding")
    );

    let unused = ParameterExpression::compile(
        "1 + 2",
        [(variable("unused"), reference(1, 101, "superi.value.scalar"))],
    )
    .unwrap_err();
    assert_eq!(
        unused.contexts()[0].field("reason"),
        Some("unused_variable_binding")
    );

    let duplicate = ParameterExpression::compile(
        "value",
        [
            (variable("value"), reference(1, 101, "superi.value.scalar")),
            (variable("value"), reference(2, 201, "superi.value.scalar")),
        ],
    )
    .unwrap_err();
    assert_eq!(
        duplicate.contexts()[0].field("reason"),
        Some("duplicate_variable_binding")
    );

    let too_deep = format!("{}1{}", "(".repeat(65), ")".repeat(65));
    let bounded = ParameterExpression::compile(&too_deep, []).unwrap_err();
    assert_eq!(
        bounded.contexts()[0].field("reason"),
        Some("expression_depth_exceeded")
    );
}

#[test]
fn expression_evaluation_rejects_nonfinite_values_and_results() {
    let division = ParameterExpression::compile("1 / 0", []).unwrap();
    let error = division
        .evaluate_with(|_, _| unreachable!("constant expression has no variables"))
        .unwrap_err();
    assert_eq!(
        error.contexts()[0].field("reason"),
        Some("division_by_zero")
    );

    let variable_expression = ParameterExpression::compile(
        "value + 1",
        [(variable("value"), reference(1, 101, "superi.value.scalar"))],
    )
    .unwrap();
    let error = variable_expression
        .evaluate_with(|_, _| Ok(f64::INFINITY))
        .unwrap_err();
    assert_eq!(
        error.contexts()[0].field("reason"),
        Some("nonfinite_variable_value")
    );
}

#[test]
fn links_and_expressions_share_one_typed_editable_snapshot_and_result() {
    let scalar_type = value_type("superi.value.scalar");
    let mut graph = four_node_graph();
    let committed = graph
        .apply(GraphTransaction::with_mutations(
            1,
            [
                GraphMutation::SetParameterDriver {
                    target: address(3, 301),
                    driver: ParameterDriver::link(
                        scalar_type.clone(),
                        reference(1, 101, "superi.value.scalar"),
                    ),
                },
                GraphMutation::SetParameterDriver {
                    target: address(4, 401),
                    driver: ParameterDriver::expression(scalar_type, expression()),
                },
            ],
        ))
        .expect("driver transaction");

    assert_eq!(committed.parameter_drivers().len(), 2);
    assert_eq!(
        committed
            .parameter_driver(address(3, 301))
            .unwrap()
            .as_link()
            .unwrap()
            .address(),
        address(1, 101)
    );
    assert_eq!(
        committed
            .parameter_driver(address(4, 401))
            .unwrap()
            .as_expression()
            .unwrap()
            .source(),
        "linked * 2 + offset"
    );

    let observed = std::thread::scope(|scope| {
        ["editor", "script", "headless"]
            .into_iter()
            .map(|_| {
                scope.spawn(|| {
                    committed
                        .evaluate_parameter(address(4, 401))
                        .expect("parameter evaluation")
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });

    assert_eq!(observed[0], observed[1]);
    assert_eq!(observed[1], observed[2]);
    assert_eq!(
        observed[0].value().value_type(),
        &value_type("superi.value.scalar")
    );
    assert_eq!(observed[0].value().payload(), &Scalar(11.0));
    assert_eq!(
        observed[0].evaluated_parameters(),
        [
            address(2, 201),
            address(1, 101),
            address(3, 301),
            address(4, 401),
        ]
    );
}

#[test]
fn parameter_evaluation_projects_only_literal_dependencies_into_the_result_domain() {
    let scalar_type = value_type("superi.value.scalar");
    let mut graph = four_node_graph();
    let snapshot = graph
        .apply(GraphTransaction::with_mutations(
            1,
            [
                GraphMutation::SetParameterDriver {
                    target: address(3, 301),
                    driver: ParameterDriver::link(
                        scalar_type.clone(),
                        reference(1, 101, "superi.value.scalar"),
                    ),
                },
                GraphMutation::SetParameterDriver {
                    target: address(4, 401),
                    driver: ParameterDriver::expression(scalar_type, expression()),
                },
            ],
        ))
        .unwrap();

    let mut projected = Vec::new();
    let result = snapshot
        .evaluate_parameter_with(address(4, 401), |parameter, literal| {
            projected.push(parameter);
            Ok(Scalar(literal.payload().0 + 1.0))
        })
        .unwrap();

    assert_eq!(projected, [address(2, 201), address(1, 101)]);
    assert_eq!(result.value().payload(), &Scalar(14.0));
    assert_eq!(
        result.evaluated_parameters(),
        [
            address(2, 201),
            address(1, 101),
            address(3, 301),
            address(4, 401),
        ]
    );
    assert_eq!(
        snapshot
            .evaluate_parameter(address(4, 401))
            .unwrap()
            .value()
            .payload(),
        &Scalar(11.0)
    );

    let error = snapshot
        .evaluate_parameter_with(address(4, 401), |parameter, literal| {
            if parameter == address(2, 201) {
                return Err(Error::new(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "test projection failed",
                ));
            }
            Ok(Scalar(literal.payload().0))
        })
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert!(error.contexts().iter().any(|context| {
        context.field("reason") == Some("parameter_projection_failed")
            && context.field("parameter") == Some(&address(2, 201).to_string())
    }));
}

#[test]
fn graph_documents_preserve_editable_driver_state_and_parameter_results() {
    let scalar_type = value_type("superi.value.scalar");
    let mut graph = four_node_graph();
    let authored = graph
        .apply(GraphTransaction::with_mutations(
            1,
            [
                GraphMutation::SetParameterDriver {
                    target: address(3, 301),
                    driver: ParameterDriver::link(
                        scalar_type.clone(),
                        reference(1, 101, "superi.value.scalar"),
                    ),
                },
                GraphMutation::SetParameterDriver {
                    target: address(4, 401),
                    driver: ParameterDriver::expression(scalar_type, expression()),
                },
            ],
        ))
        .unwrap();

    let bytes = serialize_graph(&authored).unwrap();
    let document: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        document["payload"]["parameter_drivers"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        document["payload"]["parameter_drivers"][1]["driver"]["source"],
        "linked * 2 + offset"
    );

    let observations = ["editor", "script", "headless"].map(|_| {
        let loaded = deserialize_graph::<Scalar>(&bytes).unwrap();
        assert_eq!(loaded.graph().snapshot(), authored);
        assert_eq!(loaded.canonical_document(), bytes);
        loaded
            .graph()
            .snapshot()
            .evaluate_parameter(address(4, 401))
            .unwrap()
    });
    assert_eq!(observations[0], observations[1]);
    assert_eq!(observations[1], observations[2]);
    assert_eq!(observations[0].value().payload(), &Scalar(11.0));
}

#[test]
fn drivers_can_be_replaced_and_cleared_without_changing_old_snapshots() {
    let scalar_type = value_type("superi.value.scalar");
    let mut graph = four_node_graph();
    let linked = graph
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::SetParameterDriver {
                target: address(3, 301),
                driver: ParameterDriver::link(
                    scalar_type.clone(),
                    reference(1, 101, "superi.value.scalar"),
                ),
            }],
        ))
        .unwrap();
    let replaced = graph
        .apply(GraphTransaction::with_mutations(
            2,
            [GraphMutation::SetParameterDriver {
                target: address(3, 301),
                driver: ParameterDriver::link(
                    scalar_type,
                    reference(2, 201, "superi.value.scalar"),
                ),
            }],
        ))
        .unwrap();
    let cleared = graph
        .apply(GraphTransaction::with_mutations(
            3,
            [GraphMutation::ClearParameterDriver {
                target: address(3, 301),
            }],
        ))
        .unwrap();

    assert_eq!(
        linked
            .evaluate_parameter(address(3, 301))
            .unwrap()
            .value()
            .payload(),
        &Scalar(4.0)
    );
    assert_eq!(
        replaced
            .evaluate_parameter(address(3, 301))
            .unwrap()
            .value()
            .payload(),
        &Scalar(3.0)
    );
    assert_eq!(
        cleared
            .evaluate_parameter(address(3, 301))
            .unwrap()
            .value()
            .payload(),
        &Scalar(0.0)
    );
    assert!(cleared.parameter_driver(address(3, 301)).is_none());
}

#[test]
fn missing_types_and_parameter_cycles_reject_the_complete_candidate() {
    let scalar_type = value_type("superi.value.scalar");
    let mut graph = four_node_graph();
    graph
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::SetParameterDriver {
                target: address(3, 301),
                driver: ParameterDriver::link(
                    scalar_type.clone(),
                    reference(1, 101, "superi.value.scalar"),
                ),
            }],
        ))
        .unwrap();
    let before = graph.snapshot();

    let self_cycle = graph
        .apply(GraphTransaction::with_mutations(
            2,
            [GraphMutation::SetParameterDriver {
                target: address(2, 201),
                driver: ParameterDriver::link(
                    scalar_type.clone(),
                    reference(2, 201, "superi.value.scalar"),
                ),
            }],
        ))
        .unwrap_err();
    assert_eq!(self_cycle.category(), ErrorCategory::Conflict);
    assert_eq!(
        self_cycle.contexts()[0].field("reason"),
        Some("parameter_dependency_cycle")
    );
    assert_eq!(graph.snapshot(), before);

    let cycle = graph
        .apply(GraphTransaction::with_mutations(
            2,
            [
                GraphMutation::SetParameterDriver {
                    target: address(1, 101),
                    driver: ParameterDriver::link(
                        scalar_type.clone(),
                        reference(3, 301, "superi.value.scalar"),
                    ),
                },
                GraphMutation::SetParameter {
                    node_id: NodeId::from_raw(2),
                    parameter_id: ParameterId::from_raw(201),
                    value: TypedParameterValue::new(scalar_type.clone(), Scalar(99.0)),
                },
            ],
        ))
        .unwrap_err();
    assert_eq!(cycle.category(), ErrorCategory::Conflict);
    assert_eq!(cycle.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(
        cycle.contexts()[0].field("reason"),
        Some("parameter_dependency_cycle")
    );
    assert_eq!(graph.snapshot(), before);

    let missing = graph
        .apply(GraphTransaction::with_mutations(
            2,
            [GraphMutation::SetParameterDriver {
                target: address(4, 401),
                driver: ParameterDriver::link(
                    scalar_type.clone(),
                    reference(99, 999, "superi.value.scalar"),
                ),
            }],
        ))
        .unwrap_err();
    assert_eq!(missing.category(), ErrorCategory::NotFound);
    assert_eq!(
        missing.contexts()[0].field("reason"),
        Some("missing_parameter_dependency")
    );

    let wrong_type = graph
        .apply(GraphTransaction::with_mutations(
            2,
            [GraphMutation::SetParameterDriver {
                target: address(4, 401),
                driver: ParameterDriver::link(
                    scalar_type,
                    reference(2, 201, "superi.value.integer"),
                ),
            }],
        ))
        .unwrap_err();
    assert_eq!(wrong_type.category(), ErrorCategory::InvalidInput);
    assert_eq!(
        wrong_type.contexts()[0].field("reason"),
        Some("parameter_dependency_type_mismatch")
    );
}

#[test]
fn referenced_nodes_require_explicit_driver_cleanup_before_removal() {
    let mut graph = four_node_graph();
    graph
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::SetParameterDriver {
                target: address(3, 301),
                driver: ParameterDriver::link(
                    value_type("superi.value.scalar"),
                    reference(1, 101, "superi.value.scalar"),
                ),
            }],
        ))
        .unwrap();

    let error = graph
        .apply(GraphTransaction::with_mutations(
            2,
            [GraphMutation::Remove {
                node_id: NodeId::from_raw(1),
            }],
        ))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(
        error.contexts()[0].field("reason"),
        Some("parameter_driver_references_node")
    );

    let removed = graph
        .apply(GraphTransaction::with_mutations(
            2,
            [
                GraphMutation::ClearParameterDriver {
                    target: address(3, 301),
                },
                GraphMutation::Remove {
                    node_id: NodeId::from_raw(1),
                },
            ],
        ))
        .unwrap();
    assert!(removed.node(NodeId::from_raw(1)).is_none());
    assert!(removed.parameter_drivers().is_empty());
}
