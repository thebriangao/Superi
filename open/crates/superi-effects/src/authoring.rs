//! Typed, graph-native visual effect and node authoring.
//!
//! This module adds effect presentation, defaults, discovery, and runtime factory binding around
//! the canonical `superi-graph` schema and editable-state contracts. It does not create a second
//! graph model. Every instance is an ordinary [`EditableNode`], every parameter remains ordinary
//! typed graph state, and runtime compilation receives the exact immutable graph snapshot.

use std::collections::BTreeMap;
use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result, ResultExt};
use superi_core::settings::CapabilitySet;
use superi_graph::headless::NodeCompiler;
use superi_graph::ids::{NodeId, ParameterId};
use superi_graph::mutate::{
    EditableNode, EditableParameter, GraphSnapshot, InstancePort, TypedParameterValue,
};
use superi_graph::node::{
    NodeBehavior, NodeRegistry, NodeRegistrySnapshot, NodeSchema, NodeSchemaId, ParameterName,
    ParameterSchema, PortName, PortSchema,
};

const COMPONENT: &str = "superi-effects.authoring";

/// A stable editor presentation hint for one effect parameter.
///
/// The hint does not change parameter value type, storage, animation, or evaluation semantics.
/// Higher layers may choose a different accessible control while preserving the same graph state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ParameterControl {
    /// Choose a control from the exact value type and host policy.
    Automatic,
    /// Present a boolean toggle.
    Toggle,
    /// Present a bounded or unbounded scalar slider.
    Slider,
    /// Present an angular scalar control.
    Angle,
    /// Present a normalized or percentage scalar control.
    Percentage,
    /// Present a two-dimensional point control.
    Point2,
    /// Present a managed color control.
    Color,
    /// Present a choice control whose options are supplied by the owning effect contract.
    Choice,
    /// Present an editable text control.
    Text,
}

/// Inspectable presentation metadata for one effect node family.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectMetadata {
    label: String,
    summary: String,
    category: String,
}

impl EffectMetadata {
    /// Creates complete metadata and rejects empty presentation fields.
    pub fn new(
        label: impl Into<String>,
        summary: impl Into<String>,
        category: impl Into<String>,
    ) -> Result<Self> {
        Ok(Self {
            label: required_text("create_metadata", "label", label.into())?,
            summary: required_text("create_metadata", "summary", summary.into())?,
            category: required_text("create_metadata", "category", category.into())?,
        })
    }

    /// Returns the user-facing node label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the concise user-facing behavior description.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Returns the user-facing catalog category.
    #[must_use]
    pub fn category(&self) -> &str {
        &self.category
    }
}

/// One typed graph port plus the presentation information needed to inspect it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectPortDefinition {
    schema: PortSchema,
    label: String,
    summary: String,
}

impl EffectPortDefinition {
    /// Creates a port definition and rejects empty presentation fields.
    pub fn new(
        schema: PortSchema,
        label: impl Into<String>,
        summary: impl Into<String>,
    ) -> Result<Self> {
        Ok(Self {
            schema,
            label: required_text("create_port_definition", "label", label.into())?,
            summary: required_text("create_port_definition", "summary", summary.into())?,
        })
    }

    /// Returns the canonical typed graph declaration.
    #[must_use]
    pub const fn schema(&self) -> &PortSchema {
        &self.schema
    }

    /// Returns the user-facing port label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the concise user-facing port description.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }
}

/// One typed editable parameter declaration, presentation, and initial graph value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectParameterDefinition<T> {
    schema: ParameterSchema,
    label: String,
    summary: String,
    control: ParameterControl,
    default_value: TypedParameterValue<T>,
}

impl<T> EffectParameterDefinition<T> {
    /// Creates a parameter definition and validates the exact default value type.
    pub fn new(
        schema: ParameterSchema,
        label: impl Into<String>,
        summary: impl Into<String>,
        control: ParameterControl,
        default_value: TypedParameterValue<T>,
    ) -> Result<Self> {
        if schema.value_type() != default_value.value_type() {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "effect parameter default does not match its declared value type",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_parameter_definition")
                    .with_field("parameter", schema.name().as_str())
                    .with_field("expected_type", schema.value_type().as_str())
                    .with_field("actual_type", default_value.value_type().as_str()),
            ));
        }
        Ok(Self {
            schema,
            label: required_text("create_parameter_definition", "label", label.into())?,
            summary: required_text("create_parameter_definition", "summary", summary.into())?,
            control,
            default_value,
        })
    }

    /// Returns the canonical typed and animatable graph declaration.
    #[must_use]
    pub const fn schema(&self) -> &ParameterSchema {
        &self.schema
    }

    /// Returns the user-facing parameter label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the concise user-facing parameter description.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Returns the presentation hint without changing parameter semantics.
    #[must_use]
    pub const fn control(&self) -> ParameterControl {
        self.control
    }

    /// Returns the exact typed value copied into new editable instances.
    #[must_use]
    pub const fn default_value(&self) -> &TypedParameterValue<T> {
        &self.default_value
    }
}

/// One immutable, versioned, graph-native visual effect definition.
///
/// The same definition is consumed by timeline compilation, node-graph authoring, scripts, and
/// headless runtime preparation. It contains no instance state and has no workflow-specific branch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectNodeDefinition<T> {
    schema: Arc<NodeSchema>,
    metadata: EffectMetadata,
    inputs: BTreeMap<PortName, EffectPortDefinition>,
    outputs: BTreeMap<PortName, EffectPortDefinition>,
    parameters: BTreeMap<ParameterName, EffectParameterDefinition<T>>,
}

impl<T> EffectNodeDefinition<T> {
    /// Creates one complete effect definition around the canonical graph schema.
    ///
    /// Input, output, and parameter descriptions must be unique within their respective schema
    /// namespaces. The resulting `NodeSchema` remains the authority for graph validation.
    pub fn new(
        id: NodeSchemaId,
        metadata: EffectMetadata,
        inputs: impl IntoIterator<Item = EffectPortDefinition>,
        outputs: impl IntoIterator<Item = EffectPortDefinition>,
        parameters: impl IntoIterator<Item = EffectParameterDefinition<T>>,
        behavior: NodeBehavior,
        required_capabilities: CapabilitySet,
    ) -> Result<Self> {
        let input_map = collect_port_definitions(&id, "inputs", inputs)?;
        let output_map = collect_port_definitions(&id, "outputs", outputs)?;
        let parameter_map = collect_parameter_definitions(&id, parameters)?;
        let schema = Arc::new(NodeSchema::new(
            id,
            input_map
                .values()
                .map(|definition| definition.schema.clone()),
            output_map
                .values()
                .map(|definition| definition.schema.clone()),
            parameter_map
                .values()
                .map(|definition| definition.schema.clone()),
            behavior,
            required_capabilities,
        )?);
        Ok(Self {
            schema,
            metadata,
            inputs: input_map,
            outputs: output_map,
            parameters: parameter_map,
        })
    }

    /// Returns the exact immutable graph schema used by every instance.
    #[must_use]
    pub fn schema(&self) -> &NodeSchema {
        self.schema.as_ref()
    }

    /// Returns the inspectable node presentation metadata.
    #[must_use]
    pub const fn metadata(&self) -> &EffectMetadata {
        &self.metadata
    }

    /// Looks up one input description by canonical schema-local name.
    #[must_use]
    pub fn input(&self, name: &PortName) -> Option<&EffectPortDefinition> {
        self.inputs.get(name)
    }

    /// Returns inputs in canonical schema-local name order.
    pub fn inputs(&self) -> impl ExactSizeIterator<Item = &EffectPortDefinition> {
        self.inputs.values()
    }

    /// Looks up one output description by canonical schema-local name.
    #[must_use]
    pub fn output(&self, name: &PortName) -> Option<&EffectPortDefinition> {
        self.outputs.get(name)
    }

    /// Returns outputs in canonical schema-local name order.
    pub fn outputs(&self) -> impl ExactSizeIterator<Item = &EffectPortDefinition> {
        self.outputs.values()
    }

    /// Looks up one parameter description by canonical schema-local name.
    #[must_use]
    pub fn parameter(&self, name: &ParameterName) -> Option<&EffectParameterDefinition<T>> {
        self.parameters.get(name)
    }

    /// Returns parameters in canonical schema-local name order.
    pub fn parameters(&self) -> impl ExactSizeIterator<Item = &EffectParameterDefinition<T>> {
        self.parameters.values()
    }
}

impl<T: Clone> EffectNodeDefinition<T> {
    /// Creates a normal editable graph node from caller-owned stable bindings.
    ///
    /// Overrides are keyed by schema-local parameter name. Missing overrides use the definition's
    /// typed defaults. All bindings and values receive the same exact validation as any other graph
    /// node, and no state remains hidden in the definition or a runtime factory.
    pub fn instantiate(
        &self,
        bindings: EffectInstanceBindings,
        overrides: impl IntoIterator<Item = (ParameterName, TypedParameterValue<T>)>,
    ) -> Result<EditableNode<T>> {
        let mut override_map = BTreeMap::new();
        for (name, value) in overrides {
            let Some(definition) = self.parameters.get(&name) else {
                return Err(instance_error(
                    self.schema.id(),
                    "unknown_parameter_override",
                    "effect instance override is absent from the definition",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "instantiate")
                        .with_field("parameter", name.as_str()),
                ));
            };
            if definition.schema.value_type() != value.value_type() {
                return Err(instance_error(
                    self.schema.id(),
                    "override_type_mismatch",
                    "effect instance override does not match its declared value type",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "instantiate")
                        .with_field("parameter", name.as_str())
                        .with_field("expected_type", definition.schema.value_type().as_str())
                        .with_field("actual_type", value.value_type().as_str()),
                ));
            }
            if override_map.insert(name.clone(), value).is_some() {
                return Err(instance_error(
                    self.schema.id(),
                    "duplicate_parameter_override",
                    "effect instance parameter is overridden more than once",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "instantiate")
                        .with_field("parameter", name.as_str()),
                ));
            }
        }

        let mut parameters = Vec::with_capacity(bindings.parameters.len());
        for binding in bindings.parameters {
            let Some(definition) = self.parameters.get(&binding.name) else {
                return Err(instance_error(
                    self.schema.id(),
                    "unknown_parameter_binding",
                    "effect instance parameter binding is absent from the definition",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "instantiate")
                        .with_field("parameter_id", binding.id.to_string())
                        .with_field("parameter", binding.name.as_str()),
                ));
            };
            let value = override_map
                .remove(&binding.name)
                .unwrap_or_else(|| definition.default_value.clone());
            parameters.push(EditableParameter::new(binding.id, binding.name, value));
        }

        EditableNode::new(
            Arc::clone(&self.schema),
            bindings.inputs,
            bindings.outputs,
            parameters,
        )
    }
}

/// A caller-owned stable identity bound to one schema-local parameter name.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectParameterBinding {
    id: ParameterId,
    name: ParameterName,
}

impl EffectParameterBinding {
    /// Creates one parameter identity binding.
    #[must_use]
    pub const fn new(id: ParameterId, name: ParameterName) -> Self {
        Self { id, name }
    }

    /// Returns the stable instance parameter identity.
    #[must_use]
    pub const fn id(&self) -> ParameterId {
        self.id
    }

    /// Returns the exact schema-local parameter name.
    #[must_use]
    pub const fn name(&self) -> &ParameterName {
        &self.name
    }
}

/// Complete caller-owned port and parameter identities for one effect node instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectInstanceBindings {
    inputs: Vec<InstancePort>,
    outputs: Vec<InstancePort>,
    parameters: Vec<EffectParameterBinding>,
}

impl EffectInstanceBindings {
    /// Collects instance identities for later exact validation against one definition.
    #[must_use]
    pub fn new(
        inputs: impl IntoIterator<Item = InstancePort>,
        outputs: impl IntoIterator<Item = InstancePort>,
        parameters: impl IntoIterator<Item = EffectParameterBinding>,
    ) -> Self {
        Self {
            inputs: inputs.into_iter().collect(),
            outputs: outputs.into_iter().collect(),
            parameters: parameters.into_iter().collect(),
        }
    }

    /// Returns input bindings in caller-supplied order.
    #[must_use]
    pub fn inputs(&self) -> &[InstancePort] {
        &self.inputs
    }

    /// Returns output bindings in caller-supplied order.
    #[must_use]
    pub fn outputs(&self) -> &[InstancePort] {
        &self.outputs
    }

    /// Returns parameter bindings in caller-supplied order.
    #[must_use]
    pub fn parameters(&self) -> &[EffectParameterBinding] {
        &self.parameters
    }
}

/// A mutable source of exact visual effect definitions and graph schemas.
#[derive(Debug)]
pub struct EffectCatalog<T> {
    definitions: Arc<BTreeMap<NodeSchemaId, Arc<EffectNodeDefinition<T>>>>,
    node_schemas: NodeRegistry,
}

impl<T> Clone for EffectCatalog<T> {
    fn clone(&self) -> Self {
        Self {
            definitions: Arc::clone(&self.definitions),
            node_schemas: self.node_schemas.clone(),
        }
    }
}

impl<T> Default for EffectCatalog<T> {
    fn default() -> Self {
        Self {
            definitions: Arc::new(BTreeMap::new()),
            node_schemas: NodeRegistry::new(),
        }
    }
}

impl<T> EffectCatalog<T> {
    /// Creates an empty catalog at revision zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one immutable effect definition.
    pub fn register(&mut self, definition: EffectNodeDefinition<T>) -> Result<()> {
        self.register_batch([definition])
    }

    /// Atomically registers a batch of exact effect definitions and graph schemas.
    pub fn register_batch(
        &mut self,
        definitions: impl IntoIterator<Item = EffectNodeDefinition<T>>,
    ) -> Result<()> {
        let mut pending = BTreeMap::new();
        for definition in definitions {
            let id = definition.schema.id().clone();
            if self.definitions.contains_key(&id) || pending.contains_key(&id) {
                return Err(duplicate_definition_error(&id));
            }
            pending.insert(id, Arc::new(definition));
        }
        if pending.is_empty() {
            return Ok(());
        }

        let mut node_schemas = self.node_schemas.clone();
        node_schemas.register_batch(
            pending
                .values()
                .map(|definition| definition.schema.as_ref().clone()),
        )?;
        Arc::make_mut(&mut self.definitions).extend(pending);
        self.node_schemas = node_schemas;
        Ok(())
    }

    /// Captures an immutable definition and schema discovery view.
    #[must_use]
    pub fn snapshot(&self) -> EffectCatalogSnapshot<T> {
        EffectCatalogSnapshot {
            definitions: Arc::clone(&self.definitions),
            node_schemas: self.node_schemas.snapshot(),
        }
    }
}

/// An immutable deterministic discovery view over effect definitions and graph schemas.
#[derive(Debug)]
pub struct EffectCatalogSnapshot<T> {
    definitions: Arc<BTreeMap<NodeSchemaId, Arc<EffectNodeDefinition<T>>>>,
    node_schemas: NodeRegistrySnapshot,
}

impl<T> Clone for EffectCatalogSnapshot<T> {
    fn clone(&self) -> Self {
        Self {
            definitions: Arc::clone(&self.definitions),
            node_schemas: self.node_schemas.clone(),
        }
    }
}

impl<T> EffectCatalogSnapshot<T> {
    /// Returns the successful atomic registration revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.node_schemas.revision()
    }

    /// Returns the number of exact effect definition versions.
    #[must_use]
    pub fn len(&self) -> usize {
        self.definitions.len()
    }

    /// Returns whether no effects are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.definitions.is_empty()
    }

    /// Looks up one exact effect definition.
    #[must_use]
    pub fn get(&self, id: &NodeSchemaId) -> Option<&EffectNodeDefinition<T>> {
        self.definitions.get(id).map(Arc::as_ref)
    }

    /// Returns effect definitions in canonical node-type and semantic-version order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &EffectNodeDefinition<T>> {
        self.definitions.values().map(Arc::as_ref)
    }

    /// Returns the exact synchronized graph schema discovery snapshot.
    #[must_use]
    pub const fn node_schemas(&self) -> &NodeRegistrySnapshot {
        &self.node_schemas
    }
}

/// Compiles one exact authored effect node into a caller-owned runtime implementation.
///
/// Factories receive the complete immutable graph snapshot so authored parameter drivers and other
/// evaluation state cannot be replaced by hidden factory state.
pub trait EffectNodeFactory<T, N>: Send + Sync {
    /// Compiles one node from the exact immutable graph revision being prepared.
    fn compile(
        &self,
        snapshot: &GraphSnapshot<T>,
        node_id: NodeId,
        node: &EditableNode<T>,
    ) -> Result<N>;
}

impl<T, N, F> EffectNodeFactory<T, N> for F
where
    F: Fn(&GraphSnapshot<T>, NodeId, &EditableNode<T>) -> Result<N> + Send + Sync,
{
    fn compile(
        &self,
        snapshot: &GraphSnapshot<T>,
        node_id: NodeId,
        node: &EditableNode<T>,
    ) -> Result<N> {
        self(snapshot, node_id, node)
    }
}

/// Exact-schema effect runtime factories over one immutable catalog snapshot.
pub struct EffectNodeCompiler<T, N> {
    catalog: EffectCatalogSnapshot<T>,
    factories: BTreeMap<NodeSchemaId, Arc<dyn EffectNodeFactory<T, N>>>,
}

impl<T, N> Clone for EffectNodeCompiler<T, N> {
    fn clone(&self) -> Self {
        Self {
            catalog: self.catalog.clone(),
            factories: self.factories.clone(),
        }
    }
}

impl<T, N> EffectNodeCompiler<T, N> {
    /// Creates an empty runtime factory set for one immutable authoring catalog revision.
    #[must_use]
    pub fn new(catalog: EffectCatalogSnapshot<T>) -> Self {
        Self {
            catalog,
            factories: BTreeMap::new(),
        }
    }

    /// Registers one runtime factory for an exact definition in this catalog snapshot.
    pub fn register_factory<F>(&mut self, id: NodeSchemaId, factory: F) -> Result<()>
    where
        F: EffectNodeFactory<T, N> + 'static,
    {
        if self.catalog.get(&id).is_none() {
            return Err(Error::new(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "effect runtime factory has no matching authoring definition",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "register_factory")
                    .with_field("schema_id", id.to_string()),
            ));
        }
        if self.factories.contains_key(&id) {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "effect runtime factory is already registered",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "register_factory")
                    .with_field("schema_id", id.to_string()),
            ));
        }
        self.factories.insert(id, Arc::new(factory));
        Ok(())
    }

    /// Returns the immutable authoring catalog used for runtime translation.
    #[must_use]
    pub const fn catalog(&self) -> &EffectCatalogSnapshot<T> {
        &self.catalog
    }

    /// Returns the number of exact registered runtime factories.
    #[must_use]
    pub fn factory_count(&self) -> usize {
        self.factories.len()
    }
}

impl<T, N> NodeCompiler<T, N> for EffectNodeCompiler<T, N> {
    fn compile_node(
        &mut self,
        snapshot: &GraphSnapshot<T>,
        node_id: NodeId,
        node: &EditableNode<T>,
    ) -> Result<N> {
        let id = node.schema().id();
        let definition = self.catalog.get(id).ok_or_else(|| {
            Error::new(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "editable effect node is absent from the authoring catalog",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "compile_node")
                    .with_field("schema_id", id.to_string()),
            )
        })?;
        if definition.schema() != node.schema() {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "editable effect schema differs from its exact catalog definition",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "compile_node")
                    .with_field("schema_id", id.to_string())
                    .with_field("reason", "schema_drift"),
            ));
        }
        let factory = self.factories.get(id).ok_or_else(|| {
            Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Degraded,
                "effect definition has no available runtime factory",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "compile_node")
                    .with_field("schema_id", id.to_string()),
            )
        })?;
        factory.compile(snapshot, node_id, node).with_error_context(
            ErrorContext::new(COMPONENT, "compile_factory")
                .with_field("schema_id", id.to_string())
                .with_field("node_id", node_id.to_string()),
        )
    }
}

fn collect_port_definitions(
    id: &NodeSchemaId,
    collection: &'static str,
    definitions: impl IntoIterator<Item = EffectPortDefinition>,
) -> Result<BTreeMap<PortName, EffectPortDefinition>> {
    let mut values = BTreeMap::new();
    for definition in definitions {
        let name = definition.schema.name().clone();
        if values.insert(name.clone(), definition).is_some() {
            return Err(duplicate_description_error(id, collection, name.as_str()));
        }
    }
    Ok(values)
}

fn collect_parameter_definitions<T>(
    id: &NodeSchemaId,
    definitions: impl IntoIterator<Item = EffectParameterDefinition<T>>,
) -> Result<BTreeMap<ParameterName, EffectParameterDefinition<T>>> {
    let mut values = BTreeMap::new();
    for definition in definitions {
        let name = definition.schema.name().clone();
        if values.insert(name.clone(), definition).is_some() {
            return Err(duplicate_description_error(id, "parameters", name.as_str()));
        }
    }
    Ok(values)
}

fn duplicate_description_error(id: &NodeSchemaId, collection: &'static str, field: &str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "effect definition contains duplicate authoring descriptions",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "create_definition")
            .with_field("schema_id", id.to_string())
            .with_field("collection", collection)
            .with_field("field", field),
    )
}

fn duplicate_definition_error(id: &NodeSchemaId) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        "effect definition is already registered",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "register_batch").with_field("schema_id", id.to_string()),
    )
}

fn instance_error(id: &NodeSchemaId, reason: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new(COMPONENT, "instantiate")
            .with_field("schema_id", id.to_string())
            .with_field("reason", reason),
    )
}

pub(crate) fn required_text(
    operation: &'static str,
    field: &'static str,
    value: String,
) -> Result<String> {
    if value.trim().is_empty() {
        return Err(Error::new(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "effect authoring presentation text cannot be empty",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation)
                .with_field("field", field)
                .with_field("reason", "empty_text"),
        ));
    }
    Ok(value)
}
