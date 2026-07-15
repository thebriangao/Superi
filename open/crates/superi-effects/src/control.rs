//! Exact-time parameter animation and reusable graph-native control rigs.
//!
//! A rig is an immutable workflow-neutral authoring description. Applying it emits only ordinary
//! graph parameter-driver mutations, so links, parent formulas, cycle validation, persistence,
//! invalidation, scripts, and headless readers retain one source of truth. Exact-time evaluation
//! projects reached literal payloads into sampled animation values while the graph remains the sole
//! owner of dependency traversal and expression meaning.

use std::collections::BTreeMap;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::time::RationalTime;
use superi_graph::expr::{
    ExpressionParameterValue, ExpressionVariableName, ParameterAddress, ParameterDriver,
    ParameterExpression, ParameterReference, ScalarExpression,
};
use superi_graph::mutate::{
    EditableParameter, GraphMutation, GraphSnapshot, GraphTransaction, ParameterEvaluation,
};
use superi_graph::node::{ParameterName, ParameterSchema, ValueTypeId};

use crate::authoring::{required_text, ParameterControl};
use crate::keyframe::{AnimationCurve, AnimationValue};

const COMPONENT: &str = "superi-effects.control";
const PARENT_VARIABLE: &str = "parent";
const LOCAL_VARIABLE: &str = "local";

/// A host payload that can expose one exact animation value at project time.
///
/// Production catalogs may implement this trait for their own parameter-value enum. The returned
/// components enter the graph's ordinary link and bounded expression resolver, and no sampled value
/// is retained as authored state.
pub trait ParameterAnimationValue {
    /// Samples this stored literal at one exact time.
    ///
    /// # Errors
    ///
    /// Returns a classified domain error when the payload cannot produce the declared value type at
    /// this time.
    fn evaluate_animation_value(
        &self,
        value_type: &ValueTypeId,
        time: RationalTime,
    ) -> Result<AnimationValue>;
}

impl ParameterAnimationValue for AnimationCurve {
    fn evaluate_animation_value(
        &self,
        _value_type: &ValueTypeId,
        time: RationalTime,
    ) -> Result<AnimationValue> {
        self.evaluate(time)
    }
}

impl ParameterAnimationValue for AnimationValue {
    fn evaluate_animation_value(
        &self,
        _value_type: &ValueTypeId,
        _time: RationalTime,
    ) -> Result<AnimationValue> {
        Ok(self.clone())
    }
}

impl ExpressionParameterValue for AnimationValue {
    fn to_expression_scalar(&self, value_type: &ValueTypeId) -> Result<f64> {
        if self.component_count() != 1 {
            return Err(control_error(
                "evaluate_expression",
                "non_scalar_animation_value",
                "parameter expression requires a single-component animation value",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "evaluate_expression")
                    .with_field("value_type", value_type.as_str())
                    .with_field("component_count", self.component_count().to_string()),
            ));
        }
        Ok(self.components()[0])
    }

    fn from_expression_scalar(_value_type: &ValueTypeId, value: f64) -> Result<Self> {
        Self::scalar(value)
    }
}

/// Evaluates one animated parameter through the exact immutable graph snapshot.
///
/// Only reached undriven literals are sampled at `time`. Direct links preserve every component,
/// while expression drivers use the scalar boundary implemented by [`AnimationValue`]. The result
/// carries the graph's deterministic dependency-completion order.
///
/// # Errors
///
/// Returns user-correctable errors for a missing or nonanimatable target, sampling failure, or
/// nonscalar expression input. Published graph invariant failures retain their graph classification.
pub fn evaluate_animated_parameter<T: ParameterAnimationValue>(
    snapshot: &GraphSnapshot<T>,
    target: ParameterAddress,
    time: RationalTime,
) -> Result<ParameterEvaluation<AnimationValue>> {
    let (_, schema) = parameter_contract(snapshot, target, "evaluate_animated_parameter")?;
    if !schema.is_animatable() {
        return Err(parameter_error(
            "evaluate_animated_parameter",
            target,
            "target_parameter_not_animatable",
            "requested parameter is not declared animatable",
        ));
    }
    snapshot.evaluate_parameter_with(target, |_, literal| {
        literal
            .payload()
            .evaluate_animation_value(literal.value_type(), time)
    })
}

/// Inspectable workflow-neutral presentation and typed source for one reusable control.
///
/// The source is an ordinary animatable graph parameter. Presentation does not change graph type,
/// storage, animation, or evaluation semantics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReusableControl {
    name: ParameterName,
    label: String,
    summary: String,
    control: ParameterControl,
    source: ParameterReference,
}

impl ReusableControl {
    /// Creates one reusable control and rejects empty presentation fields.
    pub fn new(
        name: ParameterName,
        label: impl Into<String>,
        summary: impl Into<String>,
        control: ParameterControl,
        source: ParameterReference,
    ) -> Result<Self> {
        Ok(Self {
            name,
            label: required_text("create_reusable_control", "label", label.into())?,
            summary: required_text("create_reusable_control", "summary", summary.into())?,
            control,
            source,
        })
    }

    /// Returns the stable rig-local control name.
    #[must_use]
    pub const fn name(&self) -> &ParameterName {
        &self.name
    }

    /// Returns the user-facing label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the concise user-facing description.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Returns the editor presentation hint.
    #[must_use]
    pub const fn control(&self) -> ParameterControl {
        self.control
    }

    /// Returns the exact typed source parameter.
    #[must_use]
    pub const fn source(&self) -> &ParameterReference {
        &self.source
    }
}

/// One checked scalar composition over explicit `parent` and `local` controls.
///
/// Both variables must be used. Editable source is retained and later bound to exact typed graph
/// references when a rig creates its transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParentExpression(ScalarExpression);

impl ParentExpression {
    /// Compiles bounded pure parent composition through the shared graph expression language.
    ///
    /// # Errors
    ///
    /// Returns invalid input for malformed or unbounded source, or when either required variable is
    /// unused.
    pub fn compile(source: &str) -> Result<Self> {
        let parent = ExpressionVariableName::new(PARENT_VARIABLE)?;
        let local = ExpressionVariableName::new(LOCAL_VARIABLE)?;
        let expression = ScalarExpression::compile(source, [parent.clone(), local.clone()])?;
        if !expression.variables().contains(&parent) || !expression.variables().contains(&local) {
            return Err(control_error(
                "compile_parent_expression",
                "incomplete_parent_expression",
                "parent expression must use both parent and local controls",
            ));
        }
        Ok(Self(expression))
    }

    /// Returns editable source after outer whitespace trimming.
    #[must_use]
    pub fn source(&self) -> &str {
        self.0.source()
    }

    fn bind(
        &self,
        parent: ParameterReference,
        local: ParameterReference,
    ) -> Result<ParameterExpression> {
        ParameterExpression::compile(
            self.source(),
            [
                (ExpressionVariableName::new(PARENT_VARIABLE)?, parent),
                (ExpressionVariableName::new(LOCAL_VARIABLE)?, local),
            ],
        )
    }
}

/// One reusable-control relationship targeting ordinary graph parameter state.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ControlRelationship {
    /// Copies one control's complete sampled value to a target.
    Link {
        /// Exact graph parameter receiving the control.
        target: ParameterAddress,
        /// Rig-local reusable control name.
        control: ParameterName,
    },
    /// Derives a scalar target from explicitly named parent and local controls.
    Parent {
        /// Exact graph parameter receiving the composed control value.
        target: ParameterAddress,
        /// Rig-local parent control name.
        parent: ParameterName,
        /// Rig-local local-offset control name.
        local: ParameterName,
        /// Editable bounded composition source.
        expression: ParentExpression,
    },
}

impl ControlRelationship {
    /// Creates one lossless direct link relationship.
    #[must_use]
    pub const fn link(target: ParameterAddress, control: ParameterName) -> Self {
        Self::Link { target, control }
    }

    /// Creates one parent plus local relationship.
    #[must_use]
    pub const fn parent(
        target: ParameterAddress,
        parent: ParameterName,
        local: ParameterName,
        expression: ParentExpression,
    ) -> Self {
        Self::Parent {
            target,
            parent,
            local,
            expression,
        }
    }

    /// Returns the exact graph target.
    #[must_use]
    pub const fn target(&self) -> ParameterAddress {
        match self {
            Self::Link { target, .. } | Self::Parent { target, .. } => *target,
        }
    }
}

/// A deterministic set of reusable controls and graph-native relationships.
///
/// Controls are canonical by name and relationships by target address. The rig contains no values,
/// evaluated hierarchy, graph revision, or workflow branch. Each application validates the exact
/// snapshot and emits one optimistic graph transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParameterControlRig {
    controls: BTreeMap<ParameterName, ReusableControl>,
    relationships: BTreeMap<ParameterAddress, ControlRelationship>,
}

impl ParameterControlRig {
    /// Creates one checked reusable rig.
    ///
    /// # Errors
    ///
    /// Returns invalid input for duplicate control names, duplicate targets, or relationships that
    /// name an absent control.
    pub fn new(
        controls: impl IntoIterator<Item = ReusableControl>,
        relationships: impl IntoIterator<Item = ControlRelationship>,
    ) -> Result<Self> {
        let mut control_map = BTreeMap::new();
        for control in controls {
            let name = control.name().clone();
            if control_map.insert(name.clone(), control).is_some() {
                return Err(control_error(
                    "create_rig",
                    "duplicate_control_name",
                    "reusable control name appears more than once",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "create_rig").with_field("control", name.as_str()),
                ));
            }
        }

        let mut relationship_map = BTreeMap::new();
        for relationship in relationships {
            let target = relationship.target();
            if relationship_map.insert(target, relationship).is_some() {
                return Err(parameter_error(
                    "create_rig",
                    target,
                    "duplicate_relationship_target",
                    "control rig targets one parameter more than once",
                ));
            }
        }

        for relationship in relationship_map.values() {
            match relationship {
                ControlRelationship::Link { control, target } => {
                    require_control(&control_map, control, *target, "control")?;
                }
                ControlRelationship::Parent {
                    parent,
                    local,
                    target,
                    ..
                } => {
                    require_control(&control_map, parent, *target, "parent")?;
                    require_control(&control_map, local, *target, "local")?;
                }
            }
        }

        Ok(Self {
            controls: control_map,
            relationships: relationship_map,
        })
    }

    /// Looks up one reusable control by stable rig-local name.
    #[must_use]
    pub fn control(&self, name: &ParameterName) -> Option<&ReusableControl> {
        self.controls.get(name)
    }

    /// Returns reusable controls in canonical name order.
    pub fn controls(&self) -> impl ExactSizeIterator<Item = &ReusableControl> {
        self.controls.values()
    }

    /// Returns relationships in canonical target-address order.
    pub fn relationships(&self) -> impl ExactSizeIterator<Item = &ControlRelationship> {
        self.relationships.values()
    }

    /// Compiles the rig into ordinary parameter-driver mutations for one exact graph revision.
    ///
    /// Every reusable source and target must exist, retain its declared exact value type, and be
    /// animatable. The returned transaction remains subject to the graph's atomic cycle and stale
    /// revision validation when applied.
    ///
    /// # Errors
    ///
    /// Returns user-correctable errors for missing, mistyped, or nonanimatable controls or targets.
    pub fn transaction<T>(&self, snapshot: &GraphSnapshot<T>) -> Result<GraphTransaction<T>> {
        for control in self.controls.values() {
            let (parameter, schema) =
                parameter_contract(snapshot, control.source().address(), "compile_rig")?;
            if parameter.value().value_type() != control.source().value_type() {
                return Err(control_parameter_error(
                    control,
                    "control_parameter_type_mismatch",
                    "reusable control source does not match its declared value type",
                ));
            }
            if !schema.is_animatable() {
                return Err(control_parameter_error(
                    control,
                    "control_parameter_not_animatable",
                    "reusable control source is not declared animatable",
                ));
            }
        }

        let mut mutations = Vec::with_capacity(self.relationships.len());
        for relationship in self.relationships.values() {
            let target = relationship.target();
            let (target_parameter, target_schema) =
                parameter_contract(snapshot, target, "compile_rig")?;
            if !target_schema.is_animatable() {
                return Err(parameter_error(
                    "compile_rig",
                    target,
                    "target_parameter_not_animatable",
                    "control relationship target is not declared animatable",
                ));
            }
            let target_type = target_parameter.value().value_type();
            let driver = match relationship {
                ControlRelationship::Link { control, .. } => {
                    let source = self
                        .controls
                        .get(control)
                        .expect("rig construction validates control references")
                        .source();
                    require_target_type(target, target_type, source, "control")?;
                    ParameterDriver::link(target_type.clone(), source.clone())
                }
                ControlRelationship::Parent {
                    parent,
                    local,
                    expression,
                    ..
                } => {
                    let parent = self
                        .controls
                        .get(parent)
                        .expect("rig construction validates parent control")
                        .source();
                    let local = self
                        .controls
                        .get(local)
                        .expect("rig construction validates local control")
                        .source();
                    require_target_type(target, target_type, parent, "parent")?;
                    require_target_type(target, target_type, local, "local")?;
                    ParameterDriver::expression(
                        target_type.clone(),
                        expression.bind(parent.clone(), local.clone())?,
                    )
                }
            };
            mutations.push(GraphMutation::SetParameterDriver { target, driver });
        }
        Ok(GraphTransaction::with_mutations(
            snapshot.revision(),
            mutations,
        ))
    }
}

fn parameter_contract<'a, T>(
    snapshot: &'a GraphSnapshot<T>,
    address: ParameterAddress,
    operation: &'static str,
) -> Result<(&'a EditableParameter<T>, &'a ParameterSchema)> {
    let node = snapshot.node(address.node_id()).ok_or_else(|| {
        parameter_not_found_error(
            operation,
            address,
            "missing_parameter_node",
            "parameter node does not exist in the graph snapshot",
        )
    })?;
    let parameter = node.parameter(address.parameter_id()).ok_or_else(|| {
        parameter_not_found_error(
            operation,
            address,
            "missing_parameter",
            "parameter does not exist in the graph snapshot",
        )
    })?;
    let schema = node
        .schema()
        .parameter(parameter.name())
        .expect("editable parameter binding retains its checked schema declaration");
    Ok((parameter, schema))
}

fn require_control<'a>(
    controls: &'a BTreeMap<ParameterName, ReusableControl>,
    name: &ParameterName,
    target: ParameterAddress,
    role: &'static str,
) -> Result<&'a ReusableControl> {
    controls.get(name).ok_or_else(|| {
        parameter_error(
            "create_rig",
            target,
            "missing_control",
            "control relationship names a control absent from the rig",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "create_rig")
                .with_field("role", role)
                .with_field("control", name.as_str()),
        )
    })
}

fn require_target_type(
    target: ParameterAddress,
    target_type: &ValueTypeId,
    source: &ParameterReference,
    role: &'static str,
) -> Result<()> {
    if source.value_type() == target_type {
        return Ok(());
    }
    Err(parameter_error(
        "compile_rig",
        target,
        "control_target_type_mismatch",
        "reusable control and target parameter types do not match",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "compile_rig")
            .with_field("role", role)
            .with_field("source", source.address().to_string())
            .with_field("source_type", source.value_type().as_str())
            .with_field("target_type", target_type.as_str()),
    ))
}

fn control_parameter_error(
    control: &ReusableControl,
    reason: &'static str,
    message: &'static str,
) -> Error {
    parameter_error("compile_rig", control.source().address(), reason, message).with_context(
        ErrorContext::new(COMPONENT, "compile_rig")
            .with_field("control", control.name().as_str())
            .with_field("declared_type", control.source().value_type().as_str()),
    )
}

fn parameter_not_found_error(
    operation: &'static str,
    parameter: ParameterAddress,
    reason: &'static str,
    message: &'static str,
) -> Error {
    Error::new(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("parameter", parameter.to_string())
            .with_field("reason", reason),
    )
}

fn parameter_error(
    operation: &'static str,
    parameter: ParameterAddress,
    reason: &'static str,
    message: &'static str,
) -> Error {
    control_error(operation, reason, message).with_context(
        ErrorContext::new(COMPONENT, operation).with_field("parameter", parameter.to_string()),
    )
}

fn control_error(operation: &'static str, reason: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation).with_field("reason", reason))
}
