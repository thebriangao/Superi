//! Canonical versioned public API schema catalog and transport-neutral wire contracts.
//!
//! This module describes the stable typed API surface. It does not route arbitrary
//! JSON, start a server, own engine state, or expose bulk runtime resources.

use std::collections::{BTreeMap, BTreeSet};

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use superi_core::diagnostics::{DiagnosticEvent, TraceValue, UserSafeError};
use superi_core::error::{
    Error, ErrorCategory, ErrorContext, Recoverability, Result as CoreResult,
};
use superi_core::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION;
use superi_core::settings::{ComponentId, FeatureAvailability, SemanticVersion};

use crate::api::{EngineIntrospectionSnapshot, MediaCapabilitiesSnapshot};
use crate::audio_automation::AudioAutomationSnapshot;
use crate::commands::{
    ApiCommand, CancelAllAsyncJobs, CancelAsyncJob, CompareProjectRecovery, DismissProjectRecovery,
    ExecuteAudioAutomationTransaction, ExecuteProjectSettingsTransaction, ExecuteScenarioAction,
    ExecuteScenarioTransaction, GetAsyncJobs, GetAudioAutomation, GetEditorState,
    GetEngineIntegrationValidation, GetEngineIntrospection, GetMediaCapabilities,
    GetProjectRecovery, GetProjectSettings, PauseAsyncJob, RemoveAsyncJob, RestoreProjectRecovery,
    ResumeAsyncJob, RetryAsyncJob,
};
use crate::editor::{ExecuteProjectCommand, ProjectHistorySnapshot};
use crate::events::{
    ApiEvent, AsyncJobsChanged, AudioAutomationChanged, EngineIntrospectionChanged,
    MediaCapabilitiesChanged, ProjectRecoveryChanged, ProjectSettingsChanged, ProjectStateChanged,
    ScenarioStateChanged,
};
use crate::jobs::AsyncJobsSnapshot;
use crate::project::ProjectSettingsSnapshot;
use crate::recovery::ProjectRecoverySnapshot;
use crate::scenario::ScenarioStateSnapshot;
use crate::state::EditorStateSnapshot;
use crate::validation::IntegrationValidationSnapshot;
use crate::version::{
    ASYNC_JOBS_SCHEMA_VERSION, AUDIO_AUTOMATION_SCHEMA_VERSION, EDITOR_STATE_SCHEMA_VERSION,
    ENGINE_INTEGRATION_VALIDATION_SCHEMA_VERSION, ENGINE_INTROSPECTION_SCHEMA_VERSION,
    GET_PUBLIC_API_SCHEMA_METHOD, MEDIA_CAPABILITIES_SCHEMA_VERSION,
    PROJECT_RECOVERY_SCHEMA_VERSION, PROJECT_SETTINGS_SCHEMA_VERSION, PUBLIC_API_SCHEMA_VERSION,
    PUBLIC_ERROR_SCHEMA_VERSION, SLICE_SCENARIO_SCHEMA_VERSION,
};

const JSON_RPC_VERSION: &str = "2.0";
const PUBLIC_API_ERROR_CODE: i32 = -32000;
const PUBLIC_ERROR_SCHEMA_NAME: &str = "superi.api.error";
const PUBLIC_CAPABILITY_SCHEMA_NAME: &str = "superi.api.capability";

/// Whether a public method observes state or mutates it through the engine owner.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicMethodKind {
    /// A state-changing operation.
    Command,
    /// A read-only operation.
    Query,
}

/// A stable named payload schema.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicSchemaReference {
    name: String,
    version: SemanticVersion,
    primitive_schema_revision: u32,
}

impl PublicSchemaReference {
    /// Creates a reference using the current stable primitive contract.
    pub fn new(name: impl Into<String>, version: SemanticVersion) -> CoreResult<Self> {
        let value = Self {
            name: name.into(),
            version,
            primitive_schema_revision: STABLE_PRIMITIVE_SCHEMA_REVISION,
        };
        value.validate()?;
        Ok(value)
    }

    /// Returns the permanent schema name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the semantic schema version.
    #[must_use]
    pub const fn version(&self) -> &SemanticVersion {
        &self.version
    }

    /// Returns the embedded stable primitive revision.
    #[must_use]
    pub const fn primitive_schema_revision(&self) -> u32 {
        self.primitive_schema_revision
    }

    fn validate(&self) -> CoreResult<()> {
        validate_public_name("validate_schema_reference", &self.name)?;
        if self.primitive_schema_revision != STABLE_PRIMITIVE_SCHEMA_REVISION {
            return Err(schema_error(
                "validate_schema_reference",
                "schema reference uses an incompatible primitive revision",
            ));
        }
        Ok(())
    }
}

/// One public command or query request and response contract.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicMethodSchema {
    method: String,
    request: PublicSchemaReference,
    response: PublicSchemaReference,
}

impl PublicMethodSchema {
    /// Creates one validated method descriptor.
    pub fn new(
        method: impl Into<String>,
        request: PublicSchemaReference,
        response: PublicSchemaReference,
    ) -> CoreResult<Self> {
        let value = Self {
            method: method.into(),
            request,
            response,
        };
        value.validate()?;
        Ok(value)
    }

    /// Returns the permanent JSON-RPC method name.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Returns the strict request payload schema.
    #[must_use]
    pub const fn request(&self) -> &PublicSchemaReference {
        &self.request
    }

    /// Returns the successful response payload schema.
    #[must_use]
    pub const fn response(&self) -> &PublicSchemaReference {
        &self.response
    }

    fn validate(&self) -> CoreResult<()> {
        validate_public_name("validate_method_schema", &self.method)?;
        self.request.validate()?;
        self.response.validate()
    }
}

/// One ordered replacement event and payload contract.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicEventSchema {
    event: String,
    payload: PublicSchemaReference,
}

impl PublicEventSchema {
    /// Creates one validated event descriptor.
    pub fn new(event: impl Into<String>, payload: PublicSchemaReference) -> CoreResult<Self> {
        let value = Self {
            event: event.into(),
            payload,
        };
        value.validate()?;
        Ok(value)
    }

    /// Returns the permanent event name.
    #[must_use]
    pub fn event(&self) -> &str {
        &self.event
    }

    /// Returns the event payload schema.
    #[must_use]
    pub const fn payload(&self) -> &PublicSchemaReference {
        &self.payload
    }

    fn validate(&self) -> CoreResult<()> {
        validate_public_name("validate_event_schema", &self.event)?;
        self.payload.validate()
    }
}

/// One durable or inspectable public replacement-state identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicResourceSchema {
    resource: String,
    payload: PublicSchemaReference,
}

impl PublicResourceSchema {
    /// Creates one validated resource descriptor.
    pub fn new(resource: impl Into<String>, payload: PublicSchemaReference) -> CoreResult<Self> {
        let value = Self {
            resource: resource.into(),
            payload,
        };
        value.validate()?;
        Ok(value)
    }

    /// Returns the permanent resource identity.
    #[must_use]
    pub fn resource(&self) -> &str {
        &self.resource
    }

    /// Returns the replacement payload schema.
    #[must_use]
    pub const fn payload(&self) -> &PublicSchemaReference {
        &self.payload
    }

    fn validate(&self) -> CoreResult<()> {
        validate_public_name("validate_resource_schema", &self.resource)?;
        self.payload.validate()
    }
}

/// The stable structured public error vocabulary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicErrorSchema {
    schema: PublicSchemaReference,
    json_rpc_application_code: i32,
    categories: Vec<ErrorCategory>,
    recoverabilities: Vec<Recoverability>,
    context_field_policy: String,
}

impl PublicErrorSchema {
    fn current() -> Self {
        Self {
            schema: PublicSchemaReference::new(
                PUBLIC_ERROR_SCHEMA_NAME,
                PUBLIC_ERROR_SCHEMA_VERSION.clone(),
            )
            .expect("the built-in public error schema name is valid"),
            json_rpc_application_code: PUBLIC_API_ERROR_CODE,
            categories: ErrorCategory::ALL.to_vec(),
            recoverabilities: Recoverability::ALL.to_vec(),
            context_field_policy: "user_safe_only".to_owned(),
        }
    }

    /// Returns the public error payload schema.
    #[must_use]
    pub const fn schema(&self) -> &PublicSchemaReference {
        &self.schema
    }

    /// Returns the stable JSON-RPC application error code.
    #[must_use]
    pub const fn json_rpc_application_code(&self) -> i32 {
        self.json_rpc_application_code
    }

    /// Returns every permanent error category in stable order.
    #[must_use]
    pub fn categories(&self) -> &[ErrorCategory] {
        &self.categories
    }

    /// Returns every recovery classification in stable order.
    #[must_use]
    pub fn recoverabilities(&self) -> &[Recoverability] {
        &self.recoverabilities
    }

    fn validate(&self) -> CoreResult<()> {
        self.schema.validate()?;
        if self != &Self::current() {
            return Err(schema_error(
                "validate_error_schema",
                "public error schema identity or vocabulary is incompatible",
            ));
        }
        Ok(())
    }
}

/// The stable capability descriptor vocabulary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCapabilitySchema {
    schema: PublicSchemaReference,
    capability_id_schema: String,
    semantic_version_schema: String,
    availability: Vec<FeatureAvailability>,
    required_capabilities_schema: String,
}

impl PublicCapabilitySchema {
    fn current() -> Self {
        Self {
            schema: PublicSchemaReference::new(
                PUBLIC_CAPABILITY_SCHEMA_NAME,
                PUBLIC_API_SCHEMA_VERSION.clone(),
            )
            .expect("the built-in public capability schema name is valid"),
            capability_id_schema: "superi.core.capability_id".to_owned(),
            semantic_version_schema: "superi.core.semantic_version".to_owned(),
            availability: FeatureAvailability::ALL.to_vec(),
            required_capabilities_schema: "superi.core.capability_set".to_owned(),
        }
    }

    /// Returns the public capability schema identity.
    #[must_use]
    pub const fn schema(&self) -> &PublicSchemaReference {
        &self.schema
    }

    /// Returns every feature availability code in stable order.
    #[must_use]
    pub fn availability(&self) -> &[FeatureAvailability] {
        &self.availability
    }

    fn validate(&self) -> CoreResult<()> {
        self.schema.validate()?;
        if self != &Self::current() {
            return Err(schema_error(
                "validate_capability_schema",
                "public capability schema identity or vocabulary is incompatible",
            ));
        }
        Ok(())
    }
}

/// The deterministic complete public API catalog.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PublicApiSchemaSnapshot {
    schema_version: SemanticVersion,
    primitive_schema_revision: u32,
    json_rpc_version: String,
    commands: Vec<PublicMethodSchema>,
    queries: Vec<PublicMethodSchema>,
    events: Vec<PublicEventSchema>,
    resources: Vec<PublicResourceSchema>,
    error: PublicErrorSchema,
    capability: PublicCapabilitySchema,
}

impl PublicApiSchemaSnapshot {
    /// Builds, validates, and canonically sorts one public catalog snapshot.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        schema_version: SemanticVersion,
        primitive_schema_revision: u32,
        json_rpc_version: impl Into<String>,
        mut commands: Vec<PublicMethodSchema>,
        mut queries: Vec<PublicMethodSchema>,
        mut events: Vec<PublicEventSchema>,
        mut resources: Vec<PublicResourceSchema>,
        error: PublicErrorSchema,
        capability: PublicCapabilitySchema,
    ) -> CoreResult<Self> {
        if schema_version != PUBLIC_API_SCHEMA_VERSION {
            return Err(schema_error(
                "create_public_api_schema",
                "public API catalog version is incompatible",
            ));
        }
        if primitive_schema_revision != STABLE_PRIMITIVE_SCHEMA_REVISION {
            return Err(schema_error(
                "create_public_api_schema",
                "public API catalog primitive revision is incompatible",
            ));
        }
        let json_rpc_version = json_rpc_version.into();
        if json_rpc_version != JSON_RPC_VERSION {
            return Err(schema_error(
                "create_public_api_schema",
                "public API catalog JSON-RPC version is incompatible",
            ));
        }

        for descriptor in &commands {
            descriptor.validate()?;
        }
        for descriptor in &queries {
            descriptor.validate()?;
        }
        for descriptor in &events {
            descriptor.validate()?;
        }
        for descriptor in &resources {
            descriptor.validate()?;
        }
        error.validate()?;
        capability.validate()?;

        commands.sort_by(|left, right| left.method.cmp(&right.method));
        queries.sort_by(|left, right| left.method.cmp(&right.method));
        events.sort_by(|left, right| left.event.cmp(&right.event));
        resources.sort_by(|left, right| left.resource.cmp(&right.resource));

        reject_duplicate_names(
            "create_public_api_schema",
            commands.iter().map(|value| value.method.as_str()),
        )?;
        reject_duplicate_names(
            "create_public_api_schema",
            queries.iter().map(|value| value.method.as_str()),
        )?;
        reject_duplicate_names(
            "create_public_api_schema",
            events.iter().map(|value| value.event.as_str()),
        )?;
        reject_duplicate_names(
            "create_public_api_schema",
            resources.iter().map(|value| value.resource.as_str()),
        )?;
        let command_names = commands
            .iter()
            .map(|value| value.method.as_str())
            .collect::<BTreeSet<_>>();
        if queries
            .iter()
            .any(|value| command_names.contains(value.method.as_str()))
        {
            return Err(schema_error(
                "create_public_api_schema",
                "a public method appears as both a command and query",
            ));
        }

        Ok(Self {
            schema_version,
            primitive_schema_revision,
            json_rpc_version,
            commands,
            queries,
            events,
            resources,
            error,
            capability,
        })
    }

    /// Returns the public catalog schema version.
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    /// Returns the stable primitive revision embedded by every payload.
    #[must_use]
    pub const fn primitive_schema_revision(&self) -> u32 {
        self.primitive_schema_revision
    }

    /// Returns the JSON-RPC protocol version used by wire envelopes.
    #[must_use]
    pub fn json_rpc_version(&self) -> &str {
        &self.json_rpc_version
    }

    /// Returns mutating methods in canonical name order.
    #[must_use]
    pub fn commands(&self) -> &[PublicMethodSchema] {
        &self.commands
    }

    /// Returns read-only methods in canonical name order.
    #[must_use]
    pub fn queries(&self) -> &[PublicMethodSchema] {
        &self.queries
    }

    /// Returns public events in canonical name order.
    #[must_use]
    pub fn events(&self) -> &[PublicEventSchema] {
        &self.events
    }

    /// Returns public replacement resources in canonical name order.
    #[must_use]
    pub fn resources(&self) -> &[PublicResourceSchema] {
        &self.resources
    }

    /// Returns the one public error schema.
    #[must_use]
    pub const fn error(&self) -> &PublicErrorSchema {
        &self.error
    }

    /// Returns the one public capability schema.
    #[must_use]
    pub const fn capability(&self) -> &PublicCapabilitySchema {
        &self.capability
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicApiSchemaSnapshotWire {
    schema_version: SemanticVersion,
    primitive_schema_revision: u32,
    json_rpc_version: String,
    commands: Vec<PublicMethodSchema>,
    queries: Vec<PublicMethodSchema>,
    events: Vec<PublicEventSchema>,
    resources: Vec<PublicResourceSchema>,
    error: PublicErrorSchema,
    capability: PublicCapabilitySchema,
}

impl<'de> Deserialize<'de> for PublicApiSchemaSnapshot {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = PublicApiSchemaSnapshotWire::deserialize(deserializer)?;
        Self::try_new(
            wire.schema_version,
            wire.primitive_schema_revision,
            wire.json_rpc_version,
            wire.commands,
            wire.queries,
            wire.events,
            wire.resources,
            wire.error,
            wire.capability,
        )
        .map_err(D::Error::custom)
    }
}

/// Marker trait for public replacement-state resources.
pub trait ApiResource {
    /// Permanent resource identity.
    const RESOURCE: &'static str;
    /// Payload schema version.
    const SCHEMA_VERSION: SemanticVersion;
}

impl ApiResource for AsyncJobsSnapshot {
    const RESOURCE: &'static str = "superi.jobs";
    const SCHEMA_VERSION: SemanticVersion = ASYNC_JOBS_SCHEMA_VERSION;
}

impl ApiResource for ProjectRecoverySnapshot {
    const RESOURCE: &'static str = "superi.project.recovery";
    const SCHEMA_VERSION: SemanticVersion = PROJECT_RECOVERY_SCHEMA_VERSION;
}

impl ApiResource for EditorStateSnapshot {
    const RESOURCE: &'static str = "superi.editor.state";
    const SCHEMA_VERSION: SemanticVersion = EDITOR_STATE_SCHEMA_VERSION;
}

impl ApiResource for AudioAutomationSnapshot {
    const RESOURCE: &'static str = "superi.audio.automation";
    const SCHEMA_VERSION: SemanticVersion = AUDIO_AUTOMATION_SCHEMA_VERSION;
}

impl ApiResource for ProjectSettingsSnapshot {
    const RESOURCE: &'static str = "superi.project.settings";
    const SCHEMA_VERSION: SemanticVersion = PROJECT_SETTINGS_SCHEMA_VERSION;
}

impl ApiResource for MediaCapabilitiesSnapshot {
    const RESOURCE: &'static str = "superi.media.capabilities";
    const SCHEMA_VERSION: SemanticVersion = MEDIA_CAPABILITIES_SCHEMA_VERSION;
}

impl ApiResource for EngineIntrospectionSnapshot {
    const RESOURCE: &'static str = "superi.engine.introspection";
    const SCHEMA_VERSION: SemanticVersion = ENGINE_INTROSPECTION_SCHEMA_VERSION;
}

impl ApiResource for IntegrationValidationSnapshot {
    const RESOURCE: &'static str = "superi.engine.integration.validation";
    const SCHEMA_VERSION: SemanticVersion = ENGINE_INTEGRATION_VALIDATION_SCHEMA_VERSION;
}

impl ApiResource for ScenarioStateSnapshot {
    const RESOURCE: &'static str = "superi.slice.scenario.state";
    const SCHEMA_VERSION: SemanticVersion = SLICE_SCENARIO_SCHEMA_VERSION;
}

/// Strict empty query for retrieving the complete public API schema.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetPublicApiSchema {}

impl GetPublicApiSchema {
    /// Creates a schema discovery query.
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }
}

impl ApiCommand for GetPublicApiSchema {
    type Response = GetPublicApiSchemaResult;

    const METHOD: &'static str = GET_PUBLIC_API_SCHEMA_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Query;
    const SCHEMA_VERSION: SemanticVersion = PUBLIC_API_SCHEMA_VERSION;
}

/// Successful public schema discovery response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetPublicApiSchemaResult {
    snapshot: PublicApiSchemaSnapshot,
}

impl GetPublicApiSchemaResult {
    /// Returns the complete public catalog.
    #[must_use]
    pub const fn snapshot(&self) -> &PublicApiSchemaSnapshot {
        &self.snapshot
    }

    /// Consumes the response and returns the complete public catalog.
    #[must_use]
    pub fn into_snapshot(self) -> PublicApiSchemaSnapshot {
        self.snapshot
    }
}

/// Read-only owner for deterministic public schema discovery.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PublicApiSchemaApi {
    snapshot: PublicApiSchemaSnapshot,
}

impl PublicApiSchemaApi {
    /// Validates and constructs the built-in public catalog.
    pub fn new() -> CoreResult<Self> {
        Ok(Self {
            snapshot: current_catalog()?,
        })
    }

    /// Executes the typed schema discovery query.
    #[must_use]
    pub fn execute(&self, _query: GetPublicApiSchema) -> GetPublicApiSchemaResult {
        GetPublicApiSchemaResult {
            snapshot: self.snapshot.clone(),
        }
    }
}

/// One reviewed public context attached to a structured failure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicErrorContext {
    component: String,
    operation: String,
    fields: BTreeMap<String, TraceValue>,
}

impl PublicErrorContext {
    /// Creates an explicitly reviewed context without copying internal error fields.
    pub fn reviewed(
        component: impl Into<String>,
        operation: impl Into<String>,
    ) -> CoreResult<Self> {
        let value = Self {
            component: component.into(),
            operation: operation.into(),
            fields: BTreeMap::new(),
        };
        value.validate()?;
        Ok(value)
    }

    /// Adds one explicitly reviewed user-safe field.
    pub fn with_field(mut self, name: impl Into<String>, value: TraceValue) -> CoreResult<Self> {
        let name = name.into();
        validate_field_name(&name)?;
        self.fields.insert(name, value);
        Ok(self)
    }

    /// Returns the stable component name.
    #[must_use]
    pub fn component(&self) -> &str {
        &self.component
    }

    /// Returns the stable operation or event name.
    #[must_use]
    pub fn operation(&self) -> &str {
        &self.operation
    }

    /// Returns only fields explicitly approved for public presentation.
    #[must_use]
    pub const fn fields(&self) -> &BTreeMap<String, TraceValue> {
        &self.fields
    }

    fn validate(&self) -> CoreResult<()> {
        validate_public_name("validate_public_error_context", &self.component)?;
        validate_public_name("validate_public_error_context", &self.operation)?;
        for name in self.fields.keys() {
            validate_field_name(name)?;
        }
        Ok(())
    }
}

/// A last-valid public replacement resource retained after failure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicResourceReference {
    resource: String,
    schema_version: SemanticVersion,
    identity: String,
    revision: u64,
}

impl PublicResourceReference {
    /// Creates one stable last-valid resource reference.
    pub fn new(
        resource: impl Into<String>,
        schema_version: SemanticVersion,
        identity: impl Into<String>,
        revision: u64,
    ) -> CoreResult<Self> {
        let value = Self {
            resource: resource.into(),
            schema_version,
            identity: identity.into(),
            revision,
        };
        value.validate()?;
        Ok(value)
    }

    /// Returns the permanent resource name.
    #[must_use]
    pub fn resource(&self) -> &str {
        &self.resource
    }

    /// Returns the referenced resource schema version.
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    /// Returns the caller-visible resource identity.
    #[must_use]
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// Returns the last valid resource revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    fn validate(&self) -> CoreResult<()> {
        validate_public_name("validate_public_resource_reference", &self.resource)?;
        if self.identity.trim().is_empty() {
            return Err(schema_error(
                "validate_public_resource_reference",
                "public resource identity must not be empty",
            ));
        }
        Ok(())
    }
}

/// Structured versioned data carried by every public JSON-RPC failure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicErrorData {
    schema_version: SemanticVersion,
    primitive_schema_revision: u32,
    category: ErrorCategory,
    recoverability: Recoverability,
    code: String,
    title: String,
    action: String,
    contexts: Vec<PublicErrorContext>,
    last_valid_resource: Option<PublicResourceReference>,
}

impl PublicErrorData {
    /// Returns the stable shared error category.
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        self.category
    }

    /// Returns the explicit recovery classification.
    #[must_use]
    pub const fn recoverability(&self) -> Recoverability {
        self.recoverability
    }

    /// Returns the stable localization and automation code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the user-safe default title.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// Returns the user-safe recovery action.
    #[must_use]
    pub fn action(&self) -> &str {
        &self.action
    }

    /// Returns reviewed actionable contexts.
    #[must_use]
    pub fn contexts(&self) -> &[PublicErrorContext] {
        &self.contexts
    }

    /// Returns the optional last-valid resource reference.
    #[must_use]
    pub const fn last_valid_resource(&self) -> Option<&PublicResourceReference> {
        self.last_valid_resource.as_ref()
    }
}

/// One user-safe structured public JSON-RPC application error.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PublicApiError {
    code: i32,
    message: String,
    data: PublicErrorData,
}

impl PublicApiError {
    /// Converts a shared error only with caller-supplied reviewed public contexts.
    pub fn from_error(
        error: &Error,
        contexts: Vec<PublicErrorContext>,
        last_valid_resource: Option<PublicResourceReference>,
    ) -> CoreResult<Self> {
        let safe = UserSafeError::from_error(error);
        Self::from_safe(&safe, contexts, last_valid_resource)
    }

    /// Converts a diagnostic while copying only explicitly user-safe fields.
    pub fn from_diagnostic(
        event: &DiagnosticEvent,
        last_valid_resource: Option<PublicResourceReference>,
    ) -> CoreResult<Self> {
        let safe = event.user_safe_error().ok_or_else(|| {
            schema_error(
                "create_public_error_from_diagnostic",
                "diagnostic does not contain a user-safe error projection",
            )
        })?;
        let mut context = PublicErrorContext::reviewed(event.component(), event.name())?;
        for (name, value) in event.user_safe_fields() {
            context = context.with_field(name, value.clone())?;
        }
        Self::from_safe(safe, vec![context], last_valid_resource)
    }

    fn from_safe(
        safe: &UserSafeError,
        contexts: Vec<PublicErrorContext>,
        last_valid_resource: Option<PublicResourceReference>,
    ) -> CoreResult<Self> {
        let value = Self {
            code: PUBLIC_API_ERROR_CODE,
            message: safe.title().to_owned(),
            data: PublicErrorData {
                schema_version: PUBLIC_ERROR_SCHEMA_VERSION.clone(),
                primitive_schema_revision: STABLE_PRIMITIVE_SCHEMA_REVISION,
                category: safe.category(),
                recoverability: safe.recoverability(),
                code: safe.code().to_owned(),
                title: safe.title().to_owned(),
                action: safe.action().to_owned(),
                contexts,
                last_valid_resource,
            },
        };
        value.validate()?;
        Ok(value)
    }

    /// Returns the stable JSON-RPC application error code.
    #[must_use]
    pub const fn code(&self) -> i32 {
        self.code
    }

    /// Returns the concise user-safe message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns complete structured public failure data.
    #[must_use]
    pub const fn data(&self) -> &PublicErrorData {
        &self.data
    }

    fn validate(&self) -> CoreResult<()> {
        if self.code != PUBLIC_API_ERROR_CODE
            || self.data.schema_version != PUBLIC_ERROR_SCHEMA_VERSION
            || self.data.primitive_schema_revision != STABLE_PRIMITIVE_SCHEMA_REVISION
            || self.message != self.data.title
            || self.data.contexts.is_empty()
        {
            return Err(schema_error(
                "validate_public_api_error",
                "public API error identity or required context is invalid",
            ));
        }
        for context in &self.data.contexts {
            context.validate()?;
        }
        if let Some(resource) = &self.data.last_valid_resource {
            resource.validate()?;
        }
        let source = Error::new(
            self.data.category,
            self.data.recoverability,
            "classification validation only",
        );
        let safe = UserSafeError::from_error(&source);
        if self.data.code != safe.code()
            || self.data.title != safe.title()
            || self.data.action != safe.action()
        {
            return Err(schema_error(
                "validate_public_api_error",
                "public API error presentation does not match its classification",
            ));
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicApiErrorWire {
    code: i32,
    message: String,
    data: PublicErrorData,
}

impl<'de> Deserialize<'de> for PublicApiError {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = PublicApiErrorWire::deserialize(deserializer)?;
        let value = Self {
            code: wire.code,
            message: wire.message,
            data: wire.data,
        };
        value.validate().map_err(D::Error::custom)?;
        Ok(value)
    }
}

/// Strict typed JSON-RPC 2.0 request with a string identifier.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct JsonRpcRequest<T> {
    jsonrpc: String,
    id: String,
    method: String,
    params: T,
}

impl<T: ApiCommand> JsonRpcRequest<T> {
    /// Creates a request for the command or query type's permanent method.
    pub fn new(id: impl Into<String>, params: T) -> CoreResult<Self> {
        let id = id.into();
        validate_rpc_id(&id)?;
        Ok(Self {
            jsonrpc: JSON_RPC_VERSION.to_owned(),
            id,
            method: T::METHOD.to_owned(),
            params,
        })
    }

    /// Returns the JSON-RPC request identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the permanent method name.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Returns the typed parameters.
    #[must_use]
    pub const fn params(&self) -> &T {
        &self.params
    }

    /// Consumes the request and returns its typed parameters.
    #[must_use]
    pub fn into_params(self) -> T {
        self.params
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonRpcRequestWire<T> {
    jsonrpc: String,
    id: String,
    method: String,
    params: T,
}

impl<'de, T> Deserialize<'de> for JsonRpcRequest<T>
where
    T: ApiCommand + Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = JsonRpcRequestWire::<T>::deserialize(deserializer)?;
        if wire.jsonrpc != JSON_RPC_VERSION || wire.method != T::METHOD {
            return Err(D::Error::custom(
                "JSON-RPC request version or method does not match its typed contract",
            ));
        }
        validate_rpc_id(&wire.id).map_err(D::Error::custom)?;
        Ok(Self {
            jsonrpc: wire.jsonrpc,
            id: wire.id,
            method: wire.method,
            params: wire.params,
        })
    }
}

/// Strict typed JSON-RPC 2.0 success response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct JsonRpcSuccessResponse<T> {
    jsonrpc: String,
    id: String,
    result: T,
}

impl<T> JsonRpcSuccessResponse<T> {
    /// Creates a successful response for one request identifier.
    pub fn new(id: impl Into<String>, result: T) -> CoreResult<Self> {
        let id = id.into();
        validate_rpc_id(&id)?;
        Ok(Self {
            jsonrpc: JSON_RPC_VERSION.to_owned(),
            id,
            result,
        })
    }

    /// Returns the request identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the typed result.
    #[must_use]
    pub const fn result(&self) -> &T {
        &self.result
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonRpcSuccessResponseWire<T> {
    jsonrpc: String,
    id: String,
    result: T,
}

impl<'de, T> Deserialize<'de> for JsonRpcSuccessResponse<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = JsonRpcSuccessResponseWire::<T>::deserialize(deserializer)?;
        if wire.jsonrpc != JSON_RPC_VERSION {
            return Err(D::Error::custom("JSON-RPC response version is not 2.0"));
        }
        validate_rpc_id(&wire.id).map_err(D::Error::custom)?;
        Ok(Self {
            jsonrpc: wire.jsonrpc,
            id: wire.id,
            result: wire.result,
        })
    }
}

/// Strict JSON-RPC 2.0 failure response.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct JsonRpcFailureResponse {
    jsonrpc: String,
    id: String,
    error: PublicApiError,
}

impl JsonRpcFailureResponse {
    /// Creates a failure response for one request identifier.
    pub fn new(id: impl Into<String>, error: PublicApiError) -> CoreResult<Self> {
        let id = id.into();
        validate_rpc_id(&id)?;
        error.validate()?;
        Ok(Self {
            jsonrpc: JSON_RPC_VERSION.to_owned(),
            id,
            error,
        })
    }

    /// Returns the request identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the structured public failure.
    #[must_use]
    pub const fn error(&self) -> &PublicApiError {
        &self.error
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct JsonRpcFailureResponseWire {
    jsonrpc: String,
    id: String,
    error: PublicApiError,
}

impl<'de> Deserialize<'de> for JsonRpcFailureResponse {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = JsonRpcFailureResponseWire::deserialize(deserializer)?;
        if wire.jsonrpc != JSON_RPC_VERSION {
            return Err(D::Error::custom("JSON-RPC response version is not 2.0"));
        }
        validate_rpc_id(&wire.id).map_err(D::Error::custom)?;
        Ok(Self {
            jsonrpc: wire.jsonrpc,
            id: wire.id,
            error: wire.error,
        })
    }
}

/// A response that contains exactly one success result or structured failure.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcResponse<T> {
    /// Successful typed response.
    Success(JsonRpcSuccessResponse<T>),
    /// Structured public failure response.
    Failure(Box<JsonRpcFailureResponse>),
}

fn current_catalog() -> CoreResult<PublicApiSchemaSnapshot> {
    let mut commands = Vec::new();
    let mut queries = Vec::new();

    register_method::<GetAsyncJobs>(&mut commands, &mut queries)?;
    register_method::<PauseAsyncJob>(&mut commands, &mut queries)?;
    register_method::<ResumeAsyncJob>(&mut commands, &mut queries)?;
    register_method::<RetryAsyncJob>(&mut commands, &mut queries)?;
    register_method::<CancelAsyncJob>(&mut commands, &mut queries)?;
    register_method::<CancelAllAsyncJobs>(&mut commands, &mut queries)?;
    register_method::<RemoveAsyncJob>(&mut commands, &mut queries)?;
    register_method::<GetProjectRecovery>(&mut commands, &mut queries)?;
    register_method::<ExecuteProjectCommand>(&mut commands, &mut queries)?;
    register_method::<CompareProjectRecovery>(&mut commands, &mut queries)?;
    register_method::<RestoreProjectRecovery>(&mut commands, &mut queries)?;
    register_method::<DismissProjectRecovery>(&mut commands, &mut queries)?;
    register_method::<GetAudioAutomation>(&mut commands, &mut queries)?;
    register_method::<GetEditorState>(&mut commands, &mut queries)?;
    register_method::<ExecuteAudioAutomationTransaction>(&mut commands, &mut queries)?;
    register_method::<GetProjectSettings>(&mut commands, &mut queries)?;
    register_method::<ExecuteProjectSettingsTransaction>(&mut commands, &mut queries)?;
    register_method::<GetMediaCapabilities>(&mut commands, &mut queries)?;
    register_method::<GetEngineIntegrationValidation>(&mut commands, &mut queries)?;
    register_method::<GetEngineIntrospection>(&mut commands, &mut queries)?;
    register_method::<ExecuteScenarioAction>(&mut commands, &mut queries)?;
    register_method::<ExecuteScenarioTransaction>(&mut commands, &mut queries)?;
    register_method::<GetPublicApiSchema>(&mut commands, &mut queries)?;

    PublicApiSchemaSnapshot::try_new(
        PUBLIC_API_SCHEMA_VERSION.clone(),
        STABLE_PRIMITIVE_SCHEMA_REVISION,
        JSON_RPC_VERSION,
        commands,
        queries,
        vec![
            event_schema::<ProjectStateChanged>()?,
            event_schema::<AsyncJobsChanged>()?,
            event_schema::<ProjectRecoveryChanged>()?,
            event_schema::<AudioAutomationChanged>()?,
            event_schema::<ProjectSettingsChanged>()?,
            event_schema::<MediaCapabilitiesChanged>()?,
            event_schema::<EngineIntrospectionChanged>()?,
            event_schema::<ScenarioStateChanged>()?,
        ],
        vec![
            resource_schema::<ProjectHistorySnapshot>()?,
            resource_schema::<AsyncJobsSnapshot>()?,
            resource_schema::<ProjectRecoverySnapshot>()?,
            resource_schema::<AudioAutomationSnapshot>()?,
            resource_schema::<EditorStateSnapshot>()?,
            resource_schema::<ProjectSettingsSnapshot>()?,
            resource_schema::<MediaCapabilitiesSnapshot>()?,
            resource_schema::<EngineIntrospectionSnapshot>()?,
            resource_schema::<IntegrationValidationSnapshot>()?,
            resource_schema::<ScenarioStateSnapshot>()?,
        ],
        PublicErrorSchema::current(),
        PublicCapabilitySchema::current(),
    )
}

fn register_method<C: ApiCommand>(
    commands: &mut Vec<PublicMethodSchema>,
    queries: &mut Vec<PublicMethodSchema>,
) -> CoreResult<()> {
    let descriptor = PublicMethodSchema::new(
        C::METHOD,
        PublicSchemaReference::new(format!("{}.request", C::METHOD), C::SCHEMA_VERSION)?,
        PublicSchemaReference::new(format!("{}.response", C::METHOD), C::SCHEMA_VERSION)?,
    )?;
    match C::KIND {
        PublicMethodKind::Command => commands.push(descriptor),
        PublicMethodKind::Query => queries.push(descriptor),
    }
    Ok(())
}

fn event_schema<E: ApiEvent>() -> CoreResult<PublicEventSchema> {
    PublicEventSchema::new(
        E::NAME,
        PublicSchemaReference::new(format!("{}.payload", E::NAME), E::SCHEMA_VERSION)?,
    )
}

fn resource_schema<R: ApiResource>() -> CoreResult<PublicResourceSchema> {
    PublicResourceSchema::new(
        R::RESOURCE,
        PublicSchemaReference::new(format!("{}.snapshot", R::RESOURCE), R::SCHEMA_VERSION)?,
    )
}

fn validate_public_name(operation: &'static str, name: &str) -> CoreResult<()> {
    ComponentId::new(name)
        .map(|_| ())
        .map_err(|_| schema_error(operation, "public schema name is not canonical"))
}

fn validate_field_name(name: &str) -> CoreResult<()> {
    let bytes = name.as_bytes();
    if bytes.is_empty()
        || !bytes[0].is_ascii_lowercase()
        || bytes
            .iter()
            .copied()
            .any(|byte| !(byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'))
    {
        return Err(schema_error(
            "validate_public_error_field",
            "public error field name is not canonical",
        ));
    }
    Ok(())
}

fn validate_rpc_id(id: &str) -> CoreResult<()> {
    if id.is_empty() {
        return Err(schema_error(
            "validate_json_rpc_id",
            "JSON-RPC string identifier must not be empty",
        ));
    }
    Ok(())
}

fn reject_duplicate_names<'a>(
    operation: &'static str,
    names: impl IntoIterator<Item = &'a str>,
) -> CoreResult<()> {
    let mut seen = BTreeSet::new();
    if names.into_iter().any(|name| !seen.insert(name)) {
        return Err(schema_error(
            operation,
            "public schema catalog contains a duplicate permanent name",
        ));
    }
    Ok(())
}

fn schema_error(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-api.schema", operation))
}
