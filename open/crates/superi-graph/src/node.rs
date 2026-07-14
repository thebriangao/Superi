//! Versioned node schemas and deterministic schema discovery.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::{CapabilitySet, ComponentId, ParseSharedNameError, SemanticVersion};

macro_rules! define_namespaced_identifier {
    ($name:ident, $summary:literal) => {
        #[doc = $summary]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(ComponentId);

        impl $name {
            /// Parses a strict canonical namespaced identifier.
            pub fn new(input: impl AsRef<str>) -> std::result::Result<Self, ParseSharedNameError> {
                ComponentId::new(input).map(Self)
            }

            /// Returns the canonical identifier text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }

            /// Consumes the identifier and returns its canonical text.
            #[must_use]
            pub fn into_string(self) -> String {
                self.0.into_string()
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = ParseSharedNameError;

            fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
                Self::new(input)
            }
        }
    };
}

define_namespaced_identifier!(
    NodeTypeId,
    "A stable namespaced identity for one family of node schemas."
);
define_namespaced_identifier!(
    ValueTypeId,
    "A stable namespaced identity for values carried by ports or parameters."
);

/// A failure while parsing a schema-local port or parameter name.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ParseSchemaFieldNameError {
    /// The name was empty.
    Empty,
    /// The first byte was not a lowercase ASCII letter.
    InvalidStart,
    /// A later byte was not lowercase ASCII, a digit, `_`, or `-`.
    InvalidCharacter {
        /// The invalid byte offset.
        index: usize,
    },
}

impl fmt::Display for ParseSchemaFieldNameError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("schema field name cannot be empty"),
            Self::InvalidStart => {
                formatter.write_str("schema field name must begin with a lowercase ASCII letter")
            }
            Self::InvalidCharacter { index } => write!(
                formatter,
                "schema field name contains a non-canonical character at byte {index}"
            ),
        }
    }
}

impl std::error::Error for ParseSchemaFieldNameError {}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct SchemaFieldName(String);

impl SchemaFieldName {
    fn parse(input: &str) -> std::result::Result<Self, ParseSchemaFieldNameError> {
        let mut bytes = input.bytes();
        let Some(first) = bytes.next() else {
            return Err(ParseSchemaFieldNameError::Empty);
        };
        if !first.is_ascii_lowercase() {
            return Err(ParseSchemaFieldNameError::InvalidStart);
        }
        for (index, byte) in bytes.enumerate() {
            if !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-'))
            {
                return Err(ParseSchemaFieldNameError::InvalidCharacter { index: index + 1 });
            }
        }
        Ok(Self(input.to_owned()))
    }

    fn as_str(&self) -> &str {
        &self.0
    }
}

macro_rules! define_schema_field_name {
    ($name:ident, $summary:literal) => {
        #[doc = $summary]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(SchemaFieldName);

        impl $name {
            /// Parses a canonical schema-local name.
            pub fn new(
                input: impl AsRef<str>,
            ) -> std::result::Result<Self, ParseSchemaFieldNameError> {
                SchemaFieldName::parse(input.as_ref()).map(Self)
            }

            /// Returns the canonical name text.
            #[must_use]
            pub fn as_str(&self) -> &str {
                self.0.as_str()
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str(self.as_str())
            }
        }

        impl FromStr for $name {
            type Err = ParseSchemaFieldNameError;

            fn from_str(input: &str) -> std::result::Result<Self, Self::Err> {
                Self::new(input)
            }
        }
    };
}

define_schema_field_name!(PortName, "A stable schema-local port name.");
define_schema_field_name!(ParameterName, "A stable schema-local parameter name.");

/// The exact identity of one version of a node schema.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct NodeSchemaId {
    node_type: NodeTypeId,
    schema_version: SemanticVersion,
}

impl NodeSchemaId {
    /// Creates a schema identity from a node family and semantic version.
    #[must_use]
    pub const fn new(node_type: NodeTypeId, schema_version: SemanticVersion) -> Self {
        Self {
            node_type,
            schema_version,
        }
    }

    /// Returns the node family identity.
    #[must_use]
    pub const fn node_type(&self) -> &NodeTypeId {
        &self.node_type
    }

    /// Returns the exact schema version.
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }
}

impl Ord for NodeSchemaId {
    fn cmp(&self, other: &Self) -> Ordering {
        self.node_type
            .cmp(&other.node_type)
            .then_with(|| self.schema_version.precedence_cmp(&other.schema_version))
            .then_with(|| {
                self.schema_version
                    .to_string()
                    .cmp(&other.schema_version.to_string())
            })
    }
}

impl PartialOrd for NodeSchemaId {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for NodeSchemaId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}@{}", self.node_type, self.schema_version)
    }
}

/// The number of values accepted by a port.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum PortCardinality {
    /// Exactly one value is accepted.
    Single,
    /// Zero or one value is accepted.
    Optional,
    /// Zero or more values are accepted in stable graph order.
    Variadic,
}

/// A typed input or output declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortSchema {
    name: PortName,
    value_type: ValueTypeId,
    cardinality: PortCardinality,
}

impl PortSchema {
    /// Creates a complete port declaration.
    #[must_use]
    pub const fn new(
        name: PortName,
        value_type: ValueTypeId,
        cardinality: PortCardinality,
    ) -> Self {
        Self {
            name,
            value_type,
            cardinality,
        }
    }

    /// Returns the schema-local port name.
    #[must_use]
    pub const fn name(&self) -> &PortName {
        &self.name
    }

    /// Returns the stable value type.
    #[must_use]
    pub const fn value_type(&self) -> &ValueTypeId {
        &self.value_type
    }

    /// Returns the accepted value cardinality.
    #[must_use]
    pub const fn cardinality(&self) -> PortCardinality {
        self.cardinality
    }
}

/// A typed editable parameter declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ParameterSchema {
    name: ParameterName,
    value_type: ValueTypeId,
    animatable: bool,
}

impl ParameterSchema {
    /// Creates a complete parameter declaration.
    #[must_use]
    pub const fn new(name: ParameterName, value_type: ValueTypeId, animatable: bool) -> Self {
        Self {
            name,
            value_type,
            animatable,
        }
    }

    /// Returns the schema-local parameter name.
    #[must_use]
    pub const fn name(&self) -> &ParameterName {
        &self.name
    }

    /// Returns the stable parameter value type.
    #[must_use]
    pub const fn value_type(&self) -> &ValueTypeId {
        &self.value_type
    }

    /// Returns whether editors may animate this parameter.
    #[must_use]
    pub const fn is_animatable(&self) -> bool {
        self.animatable
    }
}

/// How evaluation time can affect a node result.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum TimeBehavior {
    /// The result does not depend on time.
    Invariant,
    /// The result depends only on the requested frame.
    CurrentFrame,
    /// The result depends on a bounded frame window around the requested frame.
    FrameWindow {
        /// Earlier frames required by evaluation.
        frames_before: u32,
        /// Later frames required by evaluation.
        frames_after: u32,
    },
    /// The node may request any project time.
    Unbounded,
}

/// How a node maps a requested output region to input work.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum RoiBehavior {
    /// Every request requires the full frame.
    FullFrame,
    /// The output request maps directly to input bounds.
    InputBounds,
    /// Input work expands uniformly around the output request.
    Expanded {
        /// Expansion in whole pixels on every side.
        pixels: u32,
    },
    /// The evaluator must ask the node implementation for its mapping.
    Custom,
}

/// Color interpretation required by a node schema.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ColorRequirements {
    /// The node does not consume color-bearing values.
    NotApplicable,
    /// Color-bearing values must retain an explicit tag, but any tag is accepted.
    Tagged,
    /// Color-bearing values must use one exact interpretation.
    Exact(ColorSpace),
}

/// Whether equal inputs and state always produce equal results.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Determinism {
    /// Equal inputs and state always produce equal results.
    Deterministic,
    /// Equal inputs and explicit seed state produce equal results.
    Seeded,
    /// Results may vary even when editable state is unchanged.
    NonDeterministic,
}

/// The finest reusable evaluation result a node permits.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum CachePolicy {
    /// Results must not be retained by the evaluator.
    Disabled,
    /// One time-invariant result may be retained.
    Static,
    /// Results may be retained independently for each frame.
    PerFrame,
    /// Results may be retained independently for each frame and region.
    PerRegion,
}

/// Complete evaluation behavior declared by every node schema.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct NodeBehavior {
    time: TimeBehavior,
    roi: RoiBehavior,
    color: ColorRequirements,
    determinism: Determinism,
    cache_policy: CachePolicy,
}

impl NodeBehavior {
    /// Creates a complete behavior declaration.
    #[must_use]
    pub const fn new(
        time: TimeBehavior,
        roi: RoiBehavior,
        color: ColorRequirements,
        determinism: Determinism,
        cache_policy: CachePolicy,
    ) -> Self {
        Self {
            time,
            roi,
            color,
            determinism,
            cache_policy,
        }
    }

    /// Returns the time dependency declaration.
    #[must_use]
    pub const fn time(self) -> TimeBehavior {
        self.time
    }

    /// Returns the region-of-interest declaration.
    #[must_use]
    pub const fn roi(self) -> RoiBehavior {
        self.roi
    }

    /// Returns the color requirement declaration.
    #[must_use]
    pub const fn color(self) -> ColorRequirements {
        self.color
    }

    /// Returns the determinism declaration.
    #[must_use]
    pub const fn determinism(self) -> Determinism {
        self.determinism
    }

    /// Returns the cache declaration.
    #[must_use]
    pub const fn cache_policy(self) -> CachePolicy {
        self.cache_policy
    }
}

/// One immutable, versioned node definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeSchema {
    id: NodeSchemaId,
    inputs: BTreeMap<PortName, PortSchema>,
    outputs: BTreeMap<PortName, PortSchema>,
    parameters: BTreeMap<ParameterName, ParameterSchema>,
    behavior: NodeBehavior,
    required_capabilities: CapabilitySet,
}

impl NodeSchema {
    /// Creates a schema and rejects duplicate fields within each namespace.
    pub fn new(
        id: NodeSchemaId,
        inputs: impl IntoIterator<Item = PortSchema>,
        outputs: impl IntoIterator<Item = PortSchema>,
        parameters: impl IntoIterator<Item = ParameterSchema>,
        behavior: NodeBehavior,
        required_capabilities: CapabilitySet,
    ) -> Result<Self> {
        let input_map = collect_ports(&id, "inputs", inputs)?;
        let output_map = collect_ports(&id, "outputs", outputs)?;
        let parameter_map = collect_parameters(&id, parameters)?;
        Ok(Self {
            id,
            inputs: input_map,
            outputs: output_map,
            parameters: parameter_map,
            behavior,
            required_capabilities,
        })
    }

    /// Returns the exact schema identity.
    #[must_use]
    pub const fn id(&self) -> &NodeSchemaId {
        &self.id
    }

    /// Looks up one input by its schema-local name.
    #[must_use]
    pub fn input(&self, name: &PortName) -> Option<&PortSchema> {
        self.inputs.get(name)
    }

    /// Returns inputs in canonical name order.
    pub fn inputs(&self) -> impl ExactSizeIterator<Item = &PortSchema> {
        self.inputs.values()
    }

    /// Looks up one output by its schema-local name.
    #[must_use]
    pub fn output(&self, name: &PortName) -> Option<&PortSchema> {
        self.outputs.get(name)
    }

    /// Returns outputs in canonical name order.
    pub fn outputs(&self) -> impl ExactSizeIterator<Item = &PortSchema> {
        self.outputs.values()
    }

    /// Looks up one parameter by its schema-local name.
    #[must_use]
    pub fn parameter(&self, name: &ParameterName) -> Option<&ParameterSchema> {
        self.parameters.get(name)
    }

    /// Returns parameters in canonical name order.
    pub fn parameters(&self) -> impl ExactSizeIterator<Item = &ParameterSchema> {
        self.parameters.values()
    }

    /// Returns the complete evaluation behavior declaration.
    #[must_use]
    pub const fn behavior(&self) -> NodeBehavior {
        self.behavior
    }

    /// Returns the symbolic capabilities required to instantiate or evaluate this schema.
    #[must_use]
    pub const fn required_capabilities(&self) -> &CapabilitySet {
        &self.required_capabilities
    }
}

fn collect_ports(
    id: &NodeSchemaId,
    collection: &'static str,
    ports: impl IntoIterator<Item = PortSchema>,
) -> Result<BTreeMap<PortName, PortSchema>> {
    let mut values = BTreeMap::new();
    for port in ports {
        let name = port.name.clone();
        if values.insert(name.clone(), port).is_some() {
            return Err(duplicate_field_error(id, collection, name.as_str()));
        }
    }
    Ok(values)
}

fn collect_parameters(
    id: &NodeSchemaId,
    parameters: impl IntoIterator<Item = ParameterSchema>,
) -> Result<BTreeMap<ParameterName, ParameterSchema>> {
    let mut values = BTreeMap::new();
    for parameter in parameters {
        let name = parameter.name.clone();
        if values.insert(name.clone(), parameter).is_some() {
            return Err(duplicate_field_error(id, "parameters", name.as_str()));
        }
    }
    Ok(values)
}

fn duplicate_field_error(id: &NodeSchemaId, collection: &str, field: &str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        format!("node schema {id} declares duplicate {collection} field {field}"),
    )
    .with_context(
        ErrorContext::new("superi-graph.node-schema", "create")
            .with_field("schema_id", id.to_string())
            .with_field("collection", collection)
            .with_field("field", field),
    )
}

/// A mutable source of registered node schemas.
#[derive(Clone, Debug, Default)]
pub struct NodeRegistry {
    revision: u64,
    schemas: Arc<BTreeMap<NodeSchemaId, Arc<NodeSchema>>>,
}

impl NodeRegistry {
    /// Creates an empty registry at revision zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers one immutable schema.
    pub fn register(&mut self, schema: NodeSchema) -> Result<()> {
        self.register_batch([schema])
    }

    /// Atomically registers a batch of immutable schemas.
    pub fn register_batch(&mut self, schemas: impl IntoIterator<Item = NodeSchema>) -> Result<()> {
        let mut pending = BTreeMap::new();
        for schema in schemas {
            let id = schema.id.clone();
            if self.schemas.contains_key(&id) || pending.contains_key(&id) {
                return Err(duplicate_registration_error(&id));
            }
            pending.insert(id, Arc::new(schema));
        }
        if pending.is_empty() {
            return Ok(());
        }

        let next_revision = self.revision.checked_add(1).ok_or_else(revision_error)?;
        Arc::make_mut(&mut self.schemas).extend(pending);
        self.revision = next_revision;
        Ok(())
    }

    /// Captures an immutable discovery view that is unaffected by later registrations.
    #[must_use]
    pub fn snapshot(&self) -> NodeRegistrySnapshot {
        NodeRegistrySnapshot {
            revision: self.revision,
            schemas: Arc::clone(&self.schemas),
        }
    }
}

fn duplicate_registration_error(id: &NodeSchemaId) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        format!("node schema {id} is already registered"),
    )
    .with_context(
        ErrorContext::new("superi-graph.node-registry", "register_batch")
            .with_field("schema_id", id.to_string()),
    )
}

fn revision_error() -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::Terminal,
        "node registry revision space is exhausted",
    )
    .with_context(ErrorContext::new(
        "superi-graph.node-registry",
        "register_batch",
    ))
}

/// An immutable, deterministic schema-discovery snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NodeRegistrySnapshot {
    revision: u64,
    schemas: Arc<BTreeMap<NodeSchemaId, Arc<NodeSchema>>>,
}

impl NodeRegistrySnapshot {
    /// Returns the successful registration transaction revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the number of exact schema versions in this snapshot.
    #[must_use]
    pub fn len(&self) -> usize {
        self.schemas.len()
    }

    /// Returns whether no schemas are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.schemas.is_empty()
    }

    /// Looks up one exact schema identity.
    #[must_use]
    pub fn get(&self, id: &NodeSchemaId) -> Option<&NodeSchema> {
        self.schemas.get(id).map(Arc::as_ref)
    }

    /// Returns all schemas in canonical node-type and semantic-version order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &NodeSchema> {
        self.schemas.values().map(Arc::as_ref)
    }

    /// Returns every registered version for one node type in semantic-version order.
    pub fn versions<'a>(
        &'a self,
        node_type: &NodeTypeId,
    ) -> impl DoubleEndedIterator<Item = &'a NodeSchema> + 'a {
        let node_type = node_type.clone();
        self.schemas
            .iter()
            .filter(move |(id, _)| id.node_type() == &node_type)
            .map(|(_, schema)| schema.as_ref())
    }

    /// Returns the highest registered semantic version for one node type.
    #[must_use]
    pub fn latest(&self, node_type: &NodeTypeId) -> Option<&NodeSchema> {
        self.versions(node_type).next_back()
    }
}
