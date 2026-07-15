//! Reusable effect presets, strict documents, and explicit schema migration.
//!
//! A preset retains one complete graph schema and one literal value for every declared parameter.
//! Instantiation creates an ordinary editable graph node with fresh caller-owned identities. Plugin
//! availability remains derived by `superi_graph::missing`, so absent implementations never erase
//! saved schema or parameter meaning and never create a second editable model.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION;
use superi_core::settings::{CapabilityId, CapabilitySet, SemanticVersion};
use superi_graph::mutate::{EditableNode, EditableParameter, TypedParameterValue};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchema, NodeSchemaId,
    NodeTypeId, ParameterName, ParameterSchema, PortCardinality, PortName, PortSchema, RoiBehavior,
    TimeBehavior, ValueTypeId,
};

use crate::authoring::EffectInstanceBindings;

const COMPONENT: &str = "superi-effects.preset";
const EFFECT_PRESET_DOCUMENT_FORMAT: &str = "superi.effect-preset";
const LEGACY_EFFECT_PRESET_DOCUMENT_FORMAT_REVISION: u32 = 0;
const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
const MAX_LABEL_BYTES: usize = 4 * 1024;
const MAX_SCHEMA_PORTS: usize = 4 * 1024;
const MAX_SCHEMA_PARAMETERS: usize = 4 * 1024;
const MAX_SCHEMA_CAPABILITIES: usize = 4 * 1024;
const MAX_MIGRATION_STEPS: usize = 256;

/// The current incompatible revision of the effect preset document contract.
pub const EFFECT_PRESET_DOCUMENT_FORMAT_REVISION: u32 = 1;

/// One immutable reusable effect configuration with complete editable meaning.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectPreset<T> {
    label: String,
    schema: Arc<NodeSchema>,
    parameters: BTreeMap<ParameterName, TypedParameterValue<T>>,
}

impl<T> EffectPreset<T> {
    /// Creates a preset and validates complete exact parameter coverage.
    ///
    /// Every schema parameter must occur exactly once with its exact value type. Unknown,
    /// duplicate, missing, or excessive state fails before the preset exists.
    pub fn new(
        label: impl Into<String>,
        schema: Arc<NodeSchema>,
        parameters: impl IntoIterator<Item = (ParameterName, TypedParameterValue<T>)>,
    ) -> Result<Self> {
        let label = validate_label(label.into())?;
        validate_schema_bounds(schema.as_ref())?;

        let mut indexed = BTreeMap::new();
        for (name, value) in parameters {
            if indexed.len() >= MAX_SCHEMA_PARAMETERS {
                return Err(resource_error(
                    "create_preset",
                    "effect preset has too many parameter values",
                    [("limit", MAX_SCHEMA_PARAMETERS.to_string())],
                ));
            }
            let declaration = schema.parameter(&name).ok_or_else(|| {
                preset_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "create_preset",
                    "effect preset contains an unknown parameter",
                    [("parameter", name.as_str().to_owned())],
                )
            })?;
            if declaration.value_type() != value.value_type() {
                return Err(preset_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "create_preset",
                    "effect preset parameter type does not match its saved schema",
                    [
                        ("parameter", name.as_str().to_owned()),
                        (
                            "expected_type",
                            declaration.value_type().as_str().to_owned(),
                        ),
                        ("actual_type", value.value_type().as_str().to_owned()),
                    ],
                ));
            }
            if indexed.insert(name.clone(), value).is_some() {
                return Err(preset_error(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "create_preset",
                    "effect preset contains a duplicate parameter",
                    [("parameter", name.as_str().to_owned())],
                ));
            }
        }

        if indexed.len() != schema.parameters().len() {
            let missing = schema
                .parameters()
                .find(|declaration| !indexed.contains_key(declaration.name()))
                .map(|declaration| declaration.name().as_str().to_owned())
                .unwrap_or_else(|| "unknown".to_owned());
            return Err(preset_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "create_preset",
                "effect preset must retain one value for every schema parameter",
                [
                    ("missing_parameter", missing),
                    ("expected_count", schema.parameters().len().to_string()),
                    ("actual_count", indexed.len().to_string()),
                ],
            ));
        }

        Ok(Self {
            label,
            schema,
            parameters: indexed,
        })
    }

    /// Returns the user-facing preset label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the exact saved graph schema.
    #[must_use]
    pub fn schema(&self) -> &NodeSchema {
        self.schema.as_ref()
    }

    /// Returns one saved parameter by schema-local name.
    #[must_use]
    pub fn parameter(&self, name: &ParameterName) -> Option<&TypedParameterValue<T>> {
        self.parameters.get(name)
    }

    /// Returns saved parameters in canonical schema-local name order.
    pub fn parameters(
        &self,
    ) -> impl ExactSizeIterator<Item = (&ParameterName, &TypedParameterValue<T>)> {
        self.parameters.iter()
    }
}

impl<T: Clone> EffectPreset<T> {
    /// Captures one ordinary editable node without retaining its instance identities.
    ///
    /// Graph-owned drivers and topology remain graph state. The preset captures only the complete
    /// schema and its schema-local literal values so every later instance receives fresh IDs.
    pub fn capture(label: impl Into<String>, node: &EditableNode<T>) -> Result<Self> {
        Self::new(
            label,
            Arc::new(node.schema().clone()),
            node.parameters()
                .values()
                .map(|parameter| (parameter.name().clone(), parameter.value().clone())),
        )
    }

    /// Instantiates the saved schema and values as one ordinary editable graph node.
    ///
    /// This operation requires no catalog or plugin lookup. A graph may therefore retain and edit
    /// the node through the canonical missing-node placeholder path until the exact schema returns.
    pub fn instantiate(&self, bindings: EffectInstanceBindings) -> Result<EditableNode<T>> {
        let mut parameters = Vec::with_capacity(bindings.parameters().len());
        for binding in bindings.parameters() {
            let value = self.parameters.get(binding.name()).ok_or_else(|| {
                preset_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "instantiate_preset",
                    "effect preset binding names a parameter absent from the saved schema",
                    [
                        ("parameter", binding.name().as_str().to_owned()),
                        ("parameter_id", binding.id().to_string()),
                    ],
                )
            })?;
            parameters.push(EditableParameter::new(
                binding.id(),
                binding.name().clone(),
                value.clone(),
            ));
        }
        EditableNode::new(
            Arc::clone(&self.schema),
            bindings.inputs().iter().cloned(),
            bindings.outputs().iter().cloned(),
            parameters,
        )
    }
}

/// One validated preset load and its canonical current-format document.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectPresetLoad<T> {
    preset: EffectPreset<T>,
    source_format_revision: u32,
    canonical_document: Vec<u8>,
}

impl<T> EffectPresetLoad<T> {
    /// Returns the reconstructed preset.
    #[must_use]
    pub const fn preset(&self) -> &EffectPreset<T> {
        &self.preset
    }

    /// Consumes the load result and returns the reconstructed preset.
    #[must_use]
    pub fn into_preset(self) -> EffectPreset<T> {
        self.preset
    }

    /// Returns the format revision found in the supplied document.
    #[must_use]
    pub const fn source_format_revision(&self) -> u32 {
        self.source_format_revision
    }

    /// Returns whether a supported older document was migrated during loading.
    #[must_use]
    pub const fn was_migrated(&self) -> bool {
        self.source_format_revision != EFFECT_PRESET_DOCUMENT_FORMAT_REVISION
    }

    /// Returns deterministic current-format bytes produced after complete validation.
    #[must_use]
    pub fn canonical_document(&self) -> &[u8] {
        &self.canonical_document
    }
}

/// Serializes one effect preset into deterministic integrity-protected JSON.
pub fn serialize_effect_preset<T>(preset: &EffectPreset<T>) -> Result<Vec<u8>>
where
    T: Serialize,
{
    let mut payload = PresetPayloadWire::from_preset(preset)?;
    payload.canonicalize();
    encode_current_document(&payload)
}

/// Loads, migrates, validates, and reconstructs one effect preset document.
pub fn deserialize_effect_preset<T>(document: &[u8]) -> Result<EffectPresetLoad<T>>
where
    T: DeserializeOwned,
{
    if document.len() > MAX_DOCUMENT_BYTES {
        return Err(resource_error(
            "decode_document",
            "effect preset document exceeds the bounded input size",
            [
                ("document_bytes", document.len().to_string()),
                ("limit", MAX_DOCUMENT_BYTES.to_string()),
            ],
        ));
    }
    let raw = serde_json::from_slice::<Value>(document).map_err(|source| {
        preset_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "decode_document",
            "effect preset document is not valid JSON",
            [("source", source.to_string())],
        )
    })?;
    let source_format_revision = inspect_header(&raw)?;
    let mut payload = match source_format_revision {
        EFFECT_PRESET_DOCUMENT_FORMAT_REVISION => {
            let envelope =
                serde_json::from_value::<CurrentEnvelopeWire>(raw).map_err(|source| {
                    preset_error(
                        ErrorCategory::CorruptData,
                        Recoverability::UserCorrectable,
                        "decode_document",
                        "current effect preset document does not match its strict schema",
                        [("source", source.to_string())],
                    )
                })?;
            validate_current_envelope(&envelope)?;
            envelope.payload
        }
        LEGACY_EFFECT_PRESET_DOCUMENT_FORMAT_REVISION => {
            let envelope = serde_json::from_value::<LegacyEnvelopeWire>(raw).map_err(|source| {
                preset_error(
                    ErrorCategory::CorruptData,
                    Recoverability::UserCorrectable,
                    "migrate_document",
                    "legacy effect preset document does not match its strict schema",
                    [("source", source.to_string())],
                )
            })?;
            if envelope.format != EFFECT_PRESET_DOCUMENT_FORMAT
                || envelope.format_revision != LEGACY_EFFECT_PRESET_DOCUMENT_FORMAT_REVISION
            {
                return Err(unsupported_format_error(
                    &envelope.format,
                    envelope.format_revision,
                ));
            }
            envelope.preset
        }
        revision => {
            return Err(unsupported_format_error(
                EFFECT_PRESET_DOCUMENT_FORMAT,
                revision,
            ));
        }
    };
    payload.validate_bounds()?;
    payload.canonicalize();
    let canonical_document = encode_current_document(&payload)?;
    let preset = payload.into_preset()?;
    Ok(EffectPresetLoad {
        preset,
        source_format_revision,
        canonical_document,
    })
}

type MigrationTransform<T> =
    dyn Fn(&EffectPreset<T>) -> Result<Vec<(ParameterName, TypedParameterValue<T>)>> + Send + Sync;

struct MigrationStep<T> {
    source: Arc<NodeSchema>,
    target: Arc<NodeSchema>,
    transform: Arc<MigrationTransform<T>>,
}

/// A deterministic registry of explicit forward effect-schema migrations.
pub struct EffectPresetMigrations<T> {
    steps: BTreeMap<NodeSchemaId, MigrationStep<T>>,
}

impl<T> fmt::Debug for EffectPresetMigrations<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EffectPresetMigrations")
            .field("sources", &self.steps.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl<T> Default for EffectPresetMigrations<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> EffectPresetMigrations<T> {
    /// Creates an empty migration registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            steps: BTreeMap::new(),
        }
    }

    /// Registers one exact strictly forward migration step.
    ///
    /// One source schema may have only one successor, which makes every migration chain
    /// deterministic. The transform returns complete target parameter state and cannot publish a
    /// partially checked preset.
    pub fn register<F>(
        &mut self,
        source: Arc<NodeSchema>,
        target: Arc<NodeSchema>,
        transform: F,
    ) -> Result<()>
    where
        F: Fn(&EffectPreset<T>) -> Result<Vec<(ParameterName, TypedParameterValue<T>)>>
            + Send
            + Sync
            + 'static,
    {
        validate_schema_bounds(source.as_ref())?;
        validate_schema_bounds(target.as_ref())?;
        if source.id().node_type() != target.id().node_type() {
            return Err(migration_error(
                ErrorCategory::InvalidInput,
                "register_migration",
                "effect preset migration must remain in one node family",
                source.id(),
                target.id(),
            ));
        }
        if target
            .id()
            .schema_version()
            .precedence_cmp(source.id().schema_version())
            != Ordering::Greater
        {
            return Err(migration_error(
                ErrorCategory::InvalidInput,
                "register_migration",
                "effect preset migration target must have strictly newer semantic precedence",
                source.id(),
                target.id(),
            ));
        }
        if self.steps.contains_key(source.id()) {
            return Err(migration_error(
                ErrorCategory::Conflict,
                "register_migration",
                "effect preset migration source already has a registered successor",
                source.id(),
                target.id(),
            ));
        }
        self.steps.insert(
            source.id().clone(),
            MigrationStep {
                source,
                target,
                transform: Arc::new(transform),
            },
        );
        Ok(())
    }

    /// Returns the number of registered source schemas.
    #[must_use]
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Returns whether no schema migration is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

impl<T: Clone> EffectPresetMigrations<T> {
    /// Migrates one immutable preset through explicit checked steps to an exact target schema.
    ///
    /// The source is never changed. Every intermediate parameter set is validated completely
    /// against its registered target before the next step can observe it.
    pub fn migrate_to(
        &self,
        preset: &EffectPreset<T>,
        target: &NodeSchema,
    ) -> Result<EffectPreset<T>> {
        if preset.schema() == target {
            return Ok(preset.clone());
        }
        if preset.schema().id() == target.id() {
            return Err(migration_error(
                ErrorCategory::Conflict,
                "migrate_preset",
                "effect preset target reuses the schema identity with incompatible fields",
                preset.schema().id(),
                target.id(),
            ));
        }
        if preset.schema().id().node_type() != target.id().node_type()
            || target
                .id()
                .schema_version()
                .precedence_cmp(preset.schema().id().schema_version())
                != Ordering::Greater
        {
            return Err(migration_error(
                ErrorCategory::Unsupported,
                "migrate_preset",
                "effect preset cannot migrate to an unrelated or non-forward schema",
                preset.schema().id(),
                target.id(),
            ));
        }

        let mut current = preset.clone();
        for _ in 0..MAX_MIGRATION_STEPS {
            let step = self.steps.get(current.schema().id()).ok_or_else(|| {
                migration_error(
                    ErrorCategory::Unsupported,
                    "migrate_preset",
                    "effect preset has no registered migration path to the requested schema",
                    current.schema().id(),
                    target.id(),
                )
            })?;
            if current.schema() != step.source.as_ref() {
                return Err(migration_error(
                    ErrorCategory::Conflict,
                    "migrate_preset",
                    "effect preset source schema conflicts with the registered migration",
                    current.schema().id(),
                    step.target.id(),
                ));
            }
            match step
                .target
                .id()
                .schema_version()
                .precedence_cmp(target.id().schema_version())
            {
                Ordering::Greater => {
                    return Err(migration_error(
                        ErrorCategory::Unsupported,
                        "migrate_preset",
                        "effect preset migration path overshoots the requested schema",
                        current.schema().id(),
                        target.id(),
                    ));
                }
                Ordering::Equal if step.target.id() != target.id() => {
                    return Err(migration_error(
                        ErrorCategory::Unsupported,
                        "migrate_preset",
                        "effect preset migration path reaches a different exact schema identity",
                        current.schema().id(),
                        target.id(),
                    ));
                }
                _ => {}
            }

            let parameters = (step.transform)(&current)?;
            let next =
                EffectPreset::new(current.label.clone(), Arc::clone(&step.target), parameters)?;
            if next.schema() == target {
                return Ok(next);
            }
            if next.schema().id() == target.id() {
                return Err(migration_error(
                    ErrorCategory::Conflict,
                    "migrate_preset",
                    "effect preset migration reached incompatible target fields",
                    next.schema().id(),
                    target.id(),
                ));
            }
            current = next;
        }
        Err(migration_error(
            ErrorCategory::ResourceExhausted,
            "migrate_preset",
            "effect preset migration exceeds the bounded step count",
            preset.schema().id(),
            target.id(),
        ))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CurrentEnvelopeWire {
    format: String,
    format_revision: u32,
    primitive_schema_revision: u32,
    payload_sha256: String,
    payload: PresetPayloadWire,
}

#[derive(Serialize)]
struct CurrentEnvelopeRef<'a> {
    format: &'static str,
    format_revision: u32,
    primitive_schema_revision: u32,
    payload_sha256: String,
    payload: &'a PresetPayloadWire,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct LegacyEnvelopeWire {
    format: String,
    format_revision: u32,
    preset: PresetPayloadWire,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PresetPayloadWire {
    label: String,
    schema: NodeSchemaWire,
    parameters: Vec<PresetParameterWire>,
}

impl PresetPayloadWire {
    fn from_preset<T>(preset: &EffectPreset<T>) -> Result<Self>
    where
        T: Serialize,
    {
        let parameters = preset
            .parameters()
            .map(|(name, value)| {
                let payload = serde_json::to_value(value.payload()).map_err(|source| {
                    preset_error(
                        ErrorCategory::InvalidInput,
                        Recoverability::UserCorrectable,
                        "encode_document",
                        "effect preset parameter payload cannot be represented as JSON",
                        [
                            ("parameter", name.as_str().to_owned()),
                            ("source", source.to_string()),
                        ],
                    )
                })?;
                Ok(PresetParameterWire {
                    name: name.as_str().to_owned(),
                    value_type: value.value_type().as_str().to_owned(),
                    payload: canonicalize_value(payload),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            label: preset.label.clone(),
            schema: NodeSchemaWire::from_schema(preset.schema())?,
            parameters,
        })
    }

    fn validate_bounds(&self) -> Result<()> {
        if self.label.len() > MAX_LABEL_BYTES {
            return Err(resource_error(
                "validate_document",
                "effect preset label exceeds its bounded size",
                [
                    ("label_bytes", self.label.len().to_string()),
                    ("limit", MAX_LABEL_BYTES.to_string()),
                ],
            ));
        }
        if self.parameters.len() > MAX_SCHEMA_PARAMETERS {
            return Err(resource_error(
                "validate_document",
                "effect preset document has too many parameter values",
                [
                    ("parameter_count", self.parameters.len().to_string()),
                    ("limit", MAX_SCHEMA_PARAMETERS.to_string()),
                ],
            ));
        }
        self.schema.validate_bounds()
    }

    fn canonicalize(&mut self) {
        self.schema.canonicalize();
        self.parameters
            .sort_by(|left, right| left.name.cmp(&right.name));
        for parameter in &mut self.parameters {
            parameter.payload =
                canonicalize_value(std::mem::replace(&mut parameter.payload, Value::Null));
        }
    }

    fn into_preset<T>(self) -> Result<EffectPreset<T>>
    where
        T: DeserializeOwned,
    {
        let schema = Arc::new(self.schema.into_schema()?);
        let parameters = self
            .parameters
            .into_iter()
            .map(PresetParameterWire::into_parameter)
            .collect::<Result<Vec<_>>>()?;
        EffectPreset::new(self.label, schema, parameters)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PresetParameterWire {
    name: String,
    value_type: String,
    payload: Value,
}

impl PresetParameterWire {
    fn into_parameter<T>(self) -> Result<(ParameterName, TypedParameterValue<T>)>
    where
        T: DeserializeOwned,
    {
        let name = parse_parameter_name(&self.name)?;
        let value_type = parse_value_type(&self.value_type)?;
        let payload = serde_json::from_value(self.payload).map_err(|source| {
            preset_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_document",
                "effect preset parameter payload cannot be reconstructed",
                [
                    ("parameter", name.as_str().to_owned()),
                    ("source", source.to_string()),
                ],
            )
        })?;
        Ok((name, TypedParameterValue::new(value_type, payload)))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeSchemaIdWire {
    node_type: String,
    schema_version: SemanticVersion,
}

impl NodeSchemaIdWire {
    fn from_id(id: &NodeSchemaId) -> Self {
        Self {
            node_type: id.node_type().as_str().to_owned(),
            schema_version: id.schema_version().clone(),
        }
    }

    fn into_id(self) -> Result<NodeSchemaId> {
        let node_type = NodeTypeId::from_str(&self.node_type).map_err(|source| {
            invalid_field_error("node_type", &self.node_type, source.to_string())
        })?;
        Ok(NodeSchemaId::new(node_type, self.schema_version))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeSchemaWire {
    id: NodeSchemaIdWire,
    inputs: Vec<PortSchemaWire>,
    outputs: Vec<PortSchemaWire>,
    parameters: Vec<ParameterSchemaWire>,
    behavior: NodeBehaviorWire,
    required_capabilities: Vec<CapabilityId>,
}

impl NodeSchemaWire {
    fn from_schema(schema: &NodeSchema) -> Result<Self> {
        Ok(Self {
            id: NodeSchemaIdWire::from_id(schema.id()),
            inputs: schema
                .inputs()
                .map(PortSchemaWire::from_schema)
                .collect::<Result<Vec<_>>>()?,
            outputs: schema
                .outputs()
                .map(PortSchemaWire::from_schema)
                .collect::<Result<Vec<_>>>()?,
            parameters: schema.parameters().map(ParameterSchemaWire::from).collect(),
            behavior: NodeBehaviorWire::from_behavior(schema.behavior())?,
            required_capabilities: schema.required_capabilities().iter().cloned().collect(),
        })
    }

    fn validate_bounds(&self) -> Result<()> {
        for (collection, count) in [
            ("inputs", self.inputs.len()),
            ("outputs", self.outputs.len()),
        ] {
            if count > MAX_SCHEMA_PORTS {
                return Err(resource_error(
                    "validate_document",
                    "effect preset schema has too many ports",
                    [
                        ("collection", collection.to_owned()),
                        ("count", count.to_string()),
                        ("limit", MAX_SCHEMA_PORTS.to_string()),
                    ],
                ));
            }
        }
        if self.parameters.len() > MAX_SCHEMA_PARAMETERS {
            return Err(resource_error(
                "validate_document",
                "effect preset schema has too many parameters",
                [
                    ("count", self.parameters.len().to_string()),
                    ("limit", MAX_SCHEMA_PARAMETERS.to_string()),
                ],
            ));
        }
        if self.required_capabilities.len() > MAX_SCHEMA_CAPABILITIES {
            return Err(resource_error(
                "validate_document",
                "effect preset schema has too many capabilities",
                [
                    ("count", self.required_capabilities.len().to_string()),
                    ("limit", MAX_SCHEMA_CAPABILITIES.to_string()),
                ],
            ));
        }
        Ok(())
    }

    fn canonicalize(&mut self) {
        self.inputs
            .sort_by(|left, right| left.name.cmp(&right.name));
        self.outputs
            .sort_by(|left, right| left.name.cmp(&right.name));
        self.parameters
            .sort_by(|left, right| left.name.cmp(&right.name));
        self.required_capabilities.sort();
    }

    fn into_schema(self) -> Result<NodeSchema> {
        self.validate_bounds()?;
        let unique_capabilities = self
            .required_capabilities
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        if unique_capabilities.len() != self.required_capabilities.len() {
            return Err(preset_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_document",
                "effect preset schema contains duplicate capabilities",
                [],
            ));
        }
        NodeSchema::new(
            self.id.into_id()?,
            self.inputs
                .into_iter()
                .map(PortSchemaWire::into_schema)
                .collect::<Result<Vec<_>>>()?,
            self.outputs
                .into_iter()
                .map(PortSchemaWire::into_schema)
                .collect::<Result<Vec<_>>>()?,
            self.parameters
                .into_iter()
                .map(ParameterSchemaWire::into_schema)
                .collect::<Result<Vec<_>>>()?,
            self.behavior.into_behavior(),
            CapabilitySet::new(unique_capabilities),
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PortSchemaWire {
    name: String,
    value_type: String,
    cardinality: PortCardinalityWire,
}

impl PortSchemaWire {
    fn from_schema(schema: &PortSchema) -> Result<Self> {
        Ok(Self {
            name: schema.name().as_str().to_owned(),
            value_type: schema.value_type().as_str().to_owned(),
            cardinality: PortCardinalityWire::from_cardinality(schema.cardinality())?,
        })
    }

    fn into_schema(self) -> Result<PortSchema> {
        Ok(PortSchema::new(
            parse_port_name(&self.name)?,
            parse_value_type(&self.value_type)?,
            self.cardinality.into_cardinality(),
        ))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ParameterSchemaWire {
    name: String,
    value_type: String,
    animatable: bool,
}

impl From<&ParameterSchema> for ParameterSchemaWire {
    fn from(schema: &ParameterSchema) -> Self {
        Self {
            name: schema.name().as_str().to_owned(),
            value_type: schema.value_type().as_str().to_owned(),
            animatable: schema.is_animatable(),
        }
    }
}

impl ParameterSchemaWire {
    fn into_schema(self) -> Result<ParameterSchema> {
        Ok(ParameterSchema::new(
            parse_parameter_name(&self.name)?,
            parse_value_type(&self.value_type)?,
            self.animatable,
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum PortCardinalityWire {
    Single,
    Optional,
    Variadic,
}

impl PortCardinalityWire {
    fn from_cardinality(value: PortCardinality) -> Result<Self> {
        match value {
            PortCardinality::Single => Ok(Self::Single),
            PortCardinality::Optional => Ok(Self::Optional),
            PortCardinality::Variadic => Ok(Self::Variadic),
            _ => Err(unsupported_schema_variant("port_cardinality")),
        }
    }

    const fn into_cardinality(self) -> PortCardinality {
        match self {
            Self::Single => PortCardinality::Single,
            Self::Optional => PortCardinality::Optional,
            Self::Variadic => PortCardinality::Variadic,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeBehaviorWire {
    time: TimeBehaviorWire,
    roi: RoiBehaviorWire,
    color: ColorRequirementsWire,
    determinism: DeterminismWire,
    cache_policy: CachePolicyWire,
}

impl NodeBehaviorWire {
    fn from_behavior(behavior: NodeBehavior) -> Result<Self> {
        Ok(Self {
            time: TimeBehaviorWire::from_behavior(behavior.time())?,
            roi: RoiBehaviorWire::from_behavior(behavior.roi())?,
            color: ColorRequirementsWire::from_requirements(behavior.color())?,
            determinism: DeterminismWire::from_determinism(behavior.determinism())?,
            cache_policy: CachePolicyWire::from_policy(behavior.cache_policy())?,
        })
    }

    const fn into_behavior(self) -> NodeBehavior {
        NodeBehavior::new(
            self.time.into_behavior(),
            self.roi.into_behavior(),
            self.color.into_requirements(),
            self.determinism.into_determinism(),
            self.cache_policy.into_policy(),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum TimeBehaviorWire {
    Invariant,
    CurrentFrame,
    FrameWindow {
        frames_before: u32,
        frames_after: u32,
    },
    Unbounded,
}

impl TimeBehaviorWire {
    fn from_behavior(value: TimeBehavior) -> Result<Self> {
        match value {
            TimeBehavior::Invariant => Ok(Self::Invariant),
            TimeBehavior::CurrentFrame => Ok(Self::CurrentFrame),
            TimeBehavior::FrameWindow {
                frames_before,
                frames_after,
            } => Ok(Self::FrameWindow {
                frames_before,
                frames_after,
            }),
            TimeBehavior::Unbounded => Ok(Self::Unbounded),
            _ => Err(unsupported_schema_variant("time_behavior")),
        }
    }

    const fn into_behavior(self) -> TimeBehavior {
        match self {
            Self::Invariant => TimeBehavior::Invariant,
            Self::CurrentFrame => TimeBehavior::CurrentFrame,
            Self::FrameWindow {
                frames_before,
                frames_after,
            } => TimeBehavior::FrameWindow {
                frames_before,
                frames_after,
            },
            Self::Unbounded => TimeBehavior::Unbounded,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum RoiBehaviorWire {
    FullFrame,
    InputBounds,
    Expanded { pixels: u32 },
    Custom,
}

impl RoiBehaviorWire {
    fn from_behavior(value: RoiBehavior) -> Result<Self> {
        match value {
            RoiBehavior::FullFrame => Ok(Self::FullFrame),
            RoiBehavior::InputBounds => Ok(Self::InputBounds),
            RoiBehavior::Expanded { pixels } => Ok(Self::Expanded { pixels }),
            RoiBehavior::Custom => Ok(Self::Custom),
            _ => Err(unsupported_schema_variant("roi_behavior")),
        }
    }

    const fn into_behavior(self) -> RoiBehavior {
        match self {
            Self::FullFrame => RoiBehavior::FullFrame,
            Self::InputBounds => RoiBehavior::InputBounds,
            Self::Expanded { pixels } => RoiBehavior::Expanded { pixels },
            Self::Custom => RoiBehavior::Custom,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ColorRequirementsWire {
    NotApplicable,
    Tagged,
    Exact { color_space: ColorSpace },
}

impl ColorRequirementsWire {
    fn from_requirements(value: ColorRequirements) -> Result<Self> {
        match value {
            ColorRequirements::NotApplicable => Ok(Self::NotApplicable),
            ColorRequirements::Tagged => Ok(Self::Tagged),
            ColorRequirements::Exact(color_space) => Ok(Self::Exact { color_space }),
            _ => Err(unsupported_schema_variant("color_requirements")),
        }
    }

    const fn into_requirements(self) -> ColorRequirements {
        match self {
            Self::NotApplicable => ColorRequirements::NotApplicable,
            Self::Tagged => ColorRequirements::Tagged,
            Self::Exact { color_space } => ColorRequirements::Exact(color_space),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum DeterminismWire {
    Deterministic,
    Seeded,
    NonDeterministic,
}

impl DeterminismWire {
    fn from_determinism(value: Determinism) -> Result<Self> {
        match value {
            Determinism::Deterministic => Ok(Self::Deterministic),
            Determinism::Seeded => Ok(Self::Seeded),
            Determinism::NonDeterministic => Ok(Self::NonDeterministic),
            _ => Err(unsupported_schema_variant("determinism")),
        }
    }

    const fn into_determinism(self) -> Determinism {
        match self {
            Self::Deterministic => Determinism::Deterministic,
            Self::Seeded => Determinism::Seeded,
            Self::NonDeterministic => Determinism::NonDeterministic,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum CachePolicyWire {
    Disabled,
    Static,
    PerFrame,
    PerRegion,
}

impl CachePolicyWire {
    fn from_policy(value: CachePolicy) -> Result<Self> {
        match value {
            CachePolicy::Disabled => Ok(Self::Disabled),
            CachePolicy::Static => Ok(Self::Static),
            CachePolicy::PerFrame => Ok(Self::PerFrame),
            CachePolicy::PerRegion => Ok(Self::PerRegion),
            _ => Err(unsupported_schema_variant("cache_policy")),
        }
    }

    const fn into_policy(self) -> CachePolicy {
        match self {
            Self::Disabled => CachePolicy::Disabled,
            Self::Static => CachePolicy::Static,
            Self::PerFrame => CachePolicy::PerFrame,
            Self::PerRegion => CachePolicy::PerRegion,
        }
    }
}

fn validate_label(label: String) -> Result<String> {
    if label.trim().is_empty() {
        return Err(preset_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "create_preset",
            "effect preset label cannot be empty",
            [],
        ));
    }
    if label.len() > MAX_LABEL_BYTES {
        return Err(resource_error(
            "create_preset",
            "effect preset label exceeds its bounded size",
            [
                ("label_bytes", label.len().to_string()),
                ("limit", MAX_LABEL_BYTES.to_string()),
            ],
        ));
    }
    Ok(label)
}

fn validate_schema_bounds(schema: &NodeSchema) -> Result<()> {
    for (collection, count, limit) in [
        ("inputs", schema.inputs().len(), MAX_SCHEMA_PORTS),
        ("outputs", schema.outputs().len(), MAX_SCHEMA_PORTS),
        (
            "parameters",
            schema.parameters().len(),
            MAX_SCHEMA_PARAMETERS,
        ),
        (
            "required_capabilities",
            schema.required_capabilities().len(),
            MAX_SCHEMA_CAPABILITIES,
        ),
    ] {
        if count > limit {
            return Err(resource_error(
                "create_preset",
                "effect preset schema collection exceeds its bounded size",
                [
                    ("collection", collection.to_owned()),
                    ("count", count.to_string()),
                    ("limit", limit.to_string()),
                ],
            ));
        }
    }
    Ok(())
}

fn inspect_header(value: &Value) -> Result<u32> {
    let object = value.as_object().ok_or_else(|| {
        preset_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "decode_document",
            "effect preset document root must be an object",
            [],
        )
    })?;
    let format = object
        .get("format")
        .and_then(Value::as_str)
        .ok_or_else(|| {
            preset_error(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                "decode_document",
                "effect preset document is missing its string format tag",
                [],
            )
        })?;
    let revision = object
        .get("format_revision")
        .and_then(Value::as_u64)
        .ok_or_else(|| {
            preset_error(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                "decode_document",
                "effect preset document is missing its unsigned format revision",
                [],
            )
        })?;
    let revision =
        u32::try_from(revision).map_err(|_| unsupported_format_error(format, u32::MAX))?;
    if format != EFFECT_PRESET_DOCUMENT_FORMAT {
        return Err(unsupported_format_error(format, revision));
    }
    Ok(revision)
}

fn validate_current_envelope(envelope: &CurrentEnvelopeWire) -> Result<()> {
    if envelope.format != EFFECT_PRESET_DOCUMENT_FORMAT
        || envelope.format_revision != EFFECT_PRESET_DOCUMENT_FORMAT_REVISION
    {
        return Err(unsupported_format_error(
            &envelope.format,
            envelope.format_revision,
        ));
    }
    if envelope.primitive_schema_revision != STABLE_PRIMITIVE_SCHEMA_REVISION {
        return Err(preset_error(
            ErrorCategory::Unsupported,
            Recoverability::UserCorrectable,
            "decode_document",
            "effect preset uses an unsupported shared primitive schema revision",
            [
                (
                    "document_revision",
                    envelope.primitive_schema_revision.to_string(),
                ),
                (
                    "supported_revision",
                    STABLE_PRIMITIVE_SCHEMA_REVISION.to_string(),
                ),
            ],
        ));
    }
    if !is_lower_sha256(&envelope.payload_sha256) {
        return Err(preset_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "verify_integrity",
            "effect preset checksum is not canonical SHA-256",
            [("payload_sha256", envelope.payload_sha256.clone())],
        ));
    }
    let actual = payload_sha256(&envelope.payload)?;
    if actual != envelope.payload_sha256 {
        return Err(preset_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "verify_integrity",
            "effect preset checksum does not match its editable state",
            [
                ("expected_sha256", envelope.payload_sha256.clone()),
                ("actual_sha256", actual),
            ],
        ));
    }
    Ok(())
}

fn encode_current_document(payload: &PresetPayloadWire) -> Result<Vec<u8>> {
    let payload_sha256 = payload_sha256(payload)?;
    let document = serde_json::to_vec(&CurrentEnvelopeRef {
        format: EFFECT_PRESET_DOCUMENT_FORMAT,
        format_revision: EFFECT_PRESET_DOCUMENT_FORMAT_REVISION,
        primitive_schema_revision: STABLE_PRIMITIVE_SCHEMA_REVISION,
        payload_sha256,
        payload,
    })
    .map_err(|source| {
        preset_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "encode_document",
            "validated effect preset could not be encoded",
            [("source", source.to_string())],
        )
    })?;
    if document.len() > MAX_DOCUMENT_BYTES {
        return Err(resource_error(
            "encode_document",
            "effect preset document exceeds the bounded output size",
            [
                ("document_bytes", document.len().to_string()),
                ("limit", MAX_DOCUMENT_BYTES.to_string()),
            ],
        ));
    }
    Ok(document)
}

fn payload_sha256(payload: &PresetPayloadWire) -> Result<String> {
    let value = serde_json::to_value(payload)
        .map(canonicalize_value)
        .map_err(|source| {
            preset_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "hash_payload",
                "effect preset payload could not be encoded for integrity verification",
                [("source", source.to_string())],
            )
        })?;
    let bytes = serde_json::to_vec(&value).map_err(|source| {
        preset_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "hash_payload",
            "canonical effect preset payload could not be encoded",
            [("source", source.to_string())],
        )
    })?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn canonicalize_value(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(canonicalize_value).collect()),
        Value::Object(values) => {
            let mut entries = values.into_iter().collect::<Vec<_>>();
            entries.sort_by(|left, right| left.0.cmp(&right.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key, canonicalize_value(value)))
                    .collect(),
            )
        }
        value => value,
    }
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn parse_port_name(value: &str) -> Result<PortName> {
    PortName::from_str(value)
        .map_err(|source| invalid_field_error("port_name", value, source.to_string()))
}

fn parse_parameter_name(value: &str) -> Result<ParameterName> {
    ParameterName::from_str(value)
        .map_err(|source| invalid_field_error("parameter_name", value, source.to_string()))
}

fn parse_value_type(value: &str) -> Result<ValueTypeId> {
    ValueTypeId::from_str(value)
        .map_err(|source| invalid_field_error("value_type", value, source.to_string()))
}

fn invalid_field_error(field: &'static str, value: &str, source: String) -> Error {
    preset_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "validate_document",
        "effect preset contains a noncanonical field value",
        [
            ("field", field.to_owned()),
            ("value", value.to_owned()),
            ("source", source),
        ],
    )
}

fn unsupported_schema_variant(field: &'static str) -> Error {
    preset_error(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        "encode_document",
        "effect preset cannot encode an unknown graph schema variant",
        [("field", field.to_owned())],
    )
}

fn unsupported_format_error(format: &str, revision: u32) -> Error {
    preset_error(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        "decode_document",
        "effect preset document format or revision is unsupported",
        [
            ("format", format.to_owned()),
            ("format_revision", revision.to_string()),
            (
                "supported_revision",
                EFFECT_PRESET_DOCUMENT_FORMAT_REVISION.to_string(),
            ),
        ],
    )
}

fn migration_error(
    category: ErrorCategory,
    operation: &'static str,
    message: &'static str,
    source: &NodeSchemaId,
    target: &NodeSchemaId,
) -> Error {
    preset_error(
        category,
        Recoverability::UserCorrectable,
        operation,
        message,
        [
            ("source_schema", source.to_string()),
            ("target_schema", target.to_string()),
        ],
    )
}

fn resource_error<const N: usize>(
    operation: &'static str,
    message: &'static str,
    fields: [(&'static str, String); N],
) -> Error {
    preset_error(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        operation,
        message,
        fields,
    )
}

fn preset_error<const N: usize>(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
    fields: [(&'static str, String); N],
) -> Error {
    let mut context = ErrorContext::new(COMPONENT, operation);
    for (field, value) in fields {
        context.insert_field(field, value);
    }
    Error::new(category, recoverability, message).with_context(context)
}
