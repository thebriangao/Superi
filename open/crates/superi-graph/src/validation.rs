//! Typed node input, output, and connection validation.
//!
//! Validation operates on graph-level type identities and cardinality while retaining evaluator
//! payloads as opaque values. Payload construction remains a trusted boundary of the evaluator
//! that owns the concrete value representation.

use std::collections::BTreeMap;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::node::{NodeSchema, PortCardinality, PortName, PortSchema, ValueTypeId};

/// Whether a validated binding set belongs to node inputs or node outputs.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum PortDirection {
    /// Values supplied to a node implementation.
    Input,
    /// Values produced by a node implementation.
    Output,
}

impl PortDirection {
    /// Returns the stable diagnostic code for this direction.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::Output => "output",
        }
    }

    const fn operation(self) -> &'static str {
        match self {
            Self::Input => "validate_inputs",
            Self::Output => "validate_outputs",
        }
    }

    const fn classification(self) -> (ErrorCategory, Recoverability) {
        match self {
            Self::Input => (ErrorCategory::InvalidInput, Recoverability::UserCorrectable),
            Self::Output => (ErrorCategory::Internal, Recoverability::Terminal),
        }
    }
}

/// One opaque evaluator payload tagged with its graph-level value type.
///
/// The tag allows graph validation without importing a catalog or concrete payload enum. Creating
/// a truthful tag remains the responsibility of the evaluator value owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypedPortValue<T> {
    value_type: ValueTypeId,
    payload: T,
}

impl<T> TypedPortValue<T> {
    /// Associates an opaque payload with its exact graph value type.
    #[must_use]
    pub const fn new(value_type: ValueTypeId, payload: T) -> Self {
        Self {
            value_type,
            payload,
        }
    }

    /// Returns the graph-level type identity without inspecting the payload.
    #[must_use]
    pub const fn value_type(&self) -> &ValueTypeId {
        &self.value_type
    }

    /// Borrows the evaluator-owned payload.
    #[must_use]
    pub const fn payload(&self) -> &T {
        &self.payload
    }

    /// Consumes the wrapper and returns the evaluator-owned payload.
    #[must_use]
    pub fn into_payload(self) -> T {
        self.payload
    }
}

/// One named input or output binding group.
///
/// Values retain caller-supplied graph order. A variadic port uses one binding group so its order
/// cannot be changed by implicit merging.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortBinding<T> {
    port: PortName,
    values: Vec<TypedPortValue<T>>,
}

impl<T> PortBinding<T> {
    /// Creates one complete binding group.
    #[must_use]
    pub fn new(port: PortName, values: impl IntoIterator<Item = TypedPortValue<T>>) -> Self {
        Self {
            port,
            values: values.into_iter().collect(),
        }
    }

    /// Returns the schema-local port name.
    #[must_use]
    pub const fn port(&self) -> &PortName {
        &self.port
    }

    /// Returns values in stable graph order.
    #[must_use]
    pub fn values(&self) -> &[TypedPortValue<T>] {
        &self.values
    }
}

/// An immutable binding snapshot normalized into canonical schema port order.
///
/// Every declared port appears once. Missing optional and variadic ports appear with empty value
/// slices, while a missing single port is rejected before this value can be constructed.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidatedPortBindings<T> {
    direction: PortDirection,
    bindings: BTreeMap<PortName, Vec<TypedPortValue<T>>>,
}

impl<T> ValidatedPortBindings<T> {
    /// Returns whether this snapshot contains inputs or outputs.
    #[must_use]
    pub const fn direction(&self) -> PortDirection {
        self.direction
    }

    /// Returns values for one declared port in stable graph order.
    #[must_use]
    pub fn get(&self, port: &PortName) -> Option<&[TypedPortValue<T>]> {
        self.bindings.get(port).map(Vec::as_slice)
    }

    /// Returns every declared port and its values in canonical port-name order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = (&PortName, &[TypedPortValue<T>])> {
        self.bindings
            .iter()
            .map(|(port, values)| (port, values.as_slice()))
    }

    /// Returns the number of declared ports in this snapshot.
    #[must_use]
    pub fn len(&self) -> usize {
        self.bindings.len()
    }

    /// Returns whether the schema declares no ports in this direction.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.bindings.is_empty()
    }
}

impl<T> IntoIterator for ValidatedPortBindings<T> {
    type Item = (PortName, Vec<TypedPortValue<T>>);
    type IntoIter = std::collections::btree_map::IntoIter<PortName, Vec<TypedPortValue<T>>>;

    fn into_iter(self) -> Self::IntoIter {
        self.bindings.into_iter()
    }
}

/// Validates caller-authored inputs against one exact node schema.
///
/// Invalid input is classified as user-correctable. The function neither evaluates nor inspects
/// payloads and has no access to project or graph state.
pub fn validate_inputs<T>(
    schema: &NodeSchema,
    bindings: impl IntoIterator<Item = PortBinding<T>>,
) -> Result<ValidatedPortBindings<T>> {
    validate_bindings(schema, PortDirection::Input, bindings)
}

/// Validates values produced by a node implementation against its exact schema.
///
/// Invalid output is an internal terminal failure because admitting it into a cache or downstream
/// node would violate the registered node contract.
pub fn validate_outputs<T>(
    schema: &NodeSchema,
    bindings: impl IntoIterator<Item = PortBinding<T>>,
) -> Result<ValidatedPortBindings<T>> {
    validate_bindings(schema, PortDirection::Output, bindings)
}

/// Validates one source-output to target-input schema connection.
///
/// This checks direction and exact type identity only. DAG storage owns endpoint instances,
/// connection counts, stable variadic edge order, and cycle prevention.
pub fn validate_connection(
    source_schema: &NodeSchema,
    source_output: &PortName,
    target_schema: &NodeSchema,
    target_input: &PortName,
) -> Result<()> {
    let source = source_schema.output(source_output).ok_or_else(|| {
        connection_error(
            source_schema,
            source_output,
            target_schema,
            target_input,
            "unknown_source_output",
            None,
        )
    })?;
    let target = target_schema.input(target_input).ok_or_else(|| {
        connection_error(
            source_schema,
            source_output,
            target_schema,
            target_input,
            "unknown_target_input",
            Some((source.value_type(), None)),
        )
    })?;

    if source.value_type() != target.value_type() {
        return Err(connection_error(
            source_schema,
            source_output,
            target_schema,
            target_input,
            "type_mismatch",
            Some((source.value_type(), Some(target.value_type()))),
        ));
    }
    Ok(())
}

fn validate_bindings<T>(
    schema: &NodeSchema,
    direction: PortDirection,
    bindings: impl IntoIterator<Item = PortBinding<T>>,
) -> Result<ValidatedPortBindings<T>> {
    let mut groups = BTreeMap::<PortName, Vec<Vec<TypedPortValue<T>>>>::new();
    for binding in bindings {
        groups.entry(binding.port).or_default().push(binding.values);
    }

    for (port, port_groups) in &groups {
        if schema_port(schema, direction, port).is_none() {
            return Err(binding_error(
                schema,
                direction,
                port,
                "unknown_port",
                "binding names a port absent from the node schema",
            ));
        }
        if port_groups.len() != 1 {
            return Err(binding_error(
                schema,
                direction,
                port,
                "duplicate_binding",
                "binding contains more than one group for the same port",
            ));
        }
    }

    let declarations = match direction {
        PortDirection::Input => schema.inputs().collect::<Vec<_>>(),
        PortDirection::Output => schema.outputs().collect::<Vec<_>>(),
    };
    let mut validated = BTreeMap::new();
    for declaration in declarations {
        let values = groups
            .remove(declaration.name())
            .and_then(|mut values| values.pop())
            .unwrap_or_default();
        validate_cardinality(schema, direction, declaration, values.len())?;
        validate_types(schema, direction, declaration, &values)?;
        validated.insert(declaration.name().clone(), values);
    }

    Ok(ValidatedPortBindings {
        direction,
        bindings: validated,
    })
}

fn schema_port<'a>(
    schema: &'a NodeSchema,
    direction: PortDirection,
    port: &PortName,
) -> Option<&'a PortSchema> {
    match direction {
        PortDirection::Input => schema.input(port),
        PortDirection::Output => schema.output(port),
    }
}

fn validate_cardinality(
    schema: &NodeSchema,
    direction: PortDirection,
    declaration: &PortSchema,
    actual_count: usize,
) -> Result<()> {
    let valid = match declaration.cardinality() {
        PortCardinality::Single => actual_count == 1,
        PortCardinality::Optional => actual_count <= 1,
        PortCardinality::Variadic => true,
    };
    if valid {
        return Ok(());
    }

    let mut context = binding_context(schema, direction, declaration.name(), "invalid_cardinality");
    context.insert_field("cardinality", cardinality_code(declaration.cardinality()));
    context.insert_field("actual_count", actual_count.to_string());
    let (category, recoverability) = direction.classification();
    Err(Error::new(
        category,
        recoverability,
        "binding value count does not satisfy the node schema",
    )
    .with_context(context))
}

fn validate_types<T>(
    schema: &NodeSchema,
    direction: PortDirection,
    declaration: &PortSchema,
    values: &[TypedPortValue<T>],
) -> Result<()> {
    for (index, value) in values.iter().enumerate() {
        if value.value_type() == declaration.value_type() {
            continue;
        }
        let mut context = binding_context(schema, direction, declaration.name(), "type_mismatch");
        context.insert_field("expected_type", declaration.value_type().as_str());
        context.insert_field("actual_type", value.value_type().as_str());
        context.insert_field("value_index", index.to_string());
        let (category, recoverability) = direction.classification();
        return Err(Error::new(
            category,
            recoverability,
            "binding value type does not match the node schema",
        )
        .with_context(context));
    }
    Ok(())
}

fn binding_error(
    schema: &NodeSchema,
    direction: PortDirection,
    port: &PortName,
    reason: &'static str,
    message: &'static str,
) -> Error {
    let (category, recoverability) = direction.classification();
    Error::new(category, recoverability, message)
        .with_context(binding_context(schema, direction, port, reason))
}

fn binding_context(
    schema: &NodeSchema,
    direction: PortDirection,
    port: &PortName,
    reason: &'static str,
) -> ErrorContext {
    ErrorContext::new("superi-graph.port-validation", direction.operation())
        .with_field("schema_id", schema.id().to_string())
        .with_field("direction", direction.code())
        .with_field("port", port.as_str())
        .with_field("reason", reason)
}

const fn cardinality_code(cardinality: PortCardinality) -> &'static str {
    match cardinality {
        PortCardinality::Single => "single",
        PortCardinality::Optional => "optional",
        PortCardinality::Variadic => "variadic",
    }
}

fn connection_error(
    source_schema: &NodeSchema,
    source_output: &PortName,
    target_schema: &NodeSchema,
    target_input: &PortName,
    reason: &'static str,
    types: Option<(&ValueTypeId, Option<&ValueTypeId>)>,
) -> Error {
    let mut context = ErrorContext::new("superi-graph.port-validation", "validate_connection")
        .with_field("source_schema_id", source_schema.id().to_string())
        .with_field("source_port", source_output.as_str())
        .with_field("target_schema_id", target_schema.id().to_string())
        .with_field("target_port", target_input.as_str())
        .with_field("reason", reason);
    if let Some((source_type, target_type)) = types {
        context.insert_field("source_type", source_type.as_str());
        if let Some(target_type) = target_type {
            context.insert_field("target_type", target_type.as_str());
        }
    }
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "connection is incompatible with the registered node schemas",
    )
    .with_context(context)
}
