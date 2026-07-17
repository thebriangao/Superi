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
use crate::event_stream::{
    CloseEventSubscription, EventStreamSnapshot, OpenEventSubscription, PollEvents,
};
use crate::events::{
    ApiEvent, AsyncJobsChanged, AudioAutomationChanged, EngineIntrospectionChanged,
    MediaCapabilitiesChanged, ProjectRecoveryChanged, ProjectSettingsChanged, ProjectStateChanged,
    ScenarioStateChanged,
};
use crate::jobs::AsyncJobsSnapshot;
use crate::permissions::{
    ApiDestructiveOperation, ApiFilesystemAccess, ApiFilesystemPlatform, ApiPermissionEffect,
    ApiPermissionKind, ApiPermissionRequirementMode, ApiPermissionRequirements, ApiPluginOperation,
};
use crate::project::ProjectSettingsSnapshot;
use crate::recovery::ProjectRecoverySnapshot;
use crate::scenario::ScenarioStateSnapshot;
use crate::scripting::RunProjectScript;
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
const PUBLIC_PERMISSION_SCHEMA_NAME: &str = "superi.api.permission";

/// Whether a public method observes state or mutates it through the engine owner.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PublicMethodKind {
    /// A state-changing operation.
    Command,
    /// A read-only operation.
    Query,
}

/// A stable named payload schema.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicSchemaReference {
    name: String,
    #[cfg_attr(feature = "typescript-bindings", specta(type = crate::typescript::SemanticVersionBinding))]
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
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicMethodSchema {
    method: String,
    request: PublicSchemaReference,
    response: PublicSchemaReference,
    permission: PublicMethodPermissionSchema,
}

impl PublicMethodSchema {
    /// Creates one validated method descriptor.
    pub fn new(
        method: impl Into<String>,
        request: PublicSchemaReference,
        response: PublicSchemaReference,
        permission: PublicMethodPermissionSchema,
    ) -> CoreResult<Self> {
        let value = Self {
            method: method.into(),
            request,
            response,
            permission,
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

    /// Returns how this method derives exact permission requirements.
    #[must_use]
    pub const fn permission(&self) -> &PublicMethodPermissionSchema {
        &self.permission
    }

    fn validate(&self) -> CoreResult<()> {
        validate_public_name("validate_method_schema", &self.method)?;
        self.request.validate()?;
        self.response.validate()?;
        self.permission.validate()
    }
}

/// Permission metadata for one public method.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicMethodPermissionSchema {
    requirement_mode: ApiPermissionRequirementMode,
    possible_kinds: Vec<ApiPermissionKind>,
}

impl PublicMethodPermissionSchema {
    /// Creates canonical method permission metadata.
    pub fn new(
        requirement_mode: ApiPermissionRequirementMode,
        possible_kinds: impl IntoIterator<Item = ApiPermissionKind>,
    ) -> CoreResult<Self> {
        let value = Self {
            requirement_mode,
            possible_kinds: possible_kinds.into_iter().collect(),
        };
        value.validate()?;
        Ok(value)
    }

    /// Returns whether requirements are absent, static, or payload-dependent.
    #[must_use]
    pub const fn requirement_mode(&self) -> ApiPermissionRequirementMode {
        self.requirement_mode
    }

    /// Returns every permission kind this method may require.
    #[must_use]
    pub fn possible_kinds(&self) -> &[ApiPermissionKind] {
        &self.possible_kinds
    }

    fn validate(&self) -> CoreResult<()> {
        let canonical = self
            .possible_kinds
            .iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        if canonical != self.possible_kinds {
            return Err(schema_error(
                "validate_method_permission_schema",
                "method permission kinds are not canonical",
            ));
        }
        match self.requirement_mode {
            ApiPermissionRequirementMode::None if self.possible_kinds.is_empty() => Ok(()),
            ApiPermissionRequirementMode::None => Err(schema_error(
                "validate_method_permission_schema",
                "permission-free method declares possible permission kinds",
            )),
            _ if self.possible_kinds.is_empty() => Err(schema_error(
                "validate_method_permission_schema",
                "protected method omits its possible permission kinds",
            )),
            _ => Ok(()),
        }
    }
}

/// One ordered replacement event and payload contract.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicErrorSchema {
    schema: PublicSchemaReference,
    json_rpc_application_code: i32,
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = Vec<crate::typescript::ErrorCategoryBinding>)
    )]
    categories: Vec<ErrorCategory>,
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = Vec<crate::typescript::RecoverabilityBinding>)
    )]
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
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicCapabilitySchema {
    schema: PublicSchemaReference,
    capability_id_schema: String,
    semantic_version_schema: String,
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = Vec<crate::typescript::FeatureAvailabilityBinding>)
    )]
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

/// Stable permission vocabulary published for hosts and automation clients.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicPermissionSchema {
    schema: PublicSchemaReference,
    requirement_modes: Vec<ApiPermissionRequirementMode>,
    kinds: Vec<ApiPermissionKind>,
    effects: Vec<ApiPermissionEffect>,
    filesystem_accesses: Vec<ApiFilesystemAccess>,
    filesystem_platforms: Vec<ApiFilesystemPlatform>,
    filesystem_path_kinds: Vec<String>,
    filesystem_scope_kinds: Vec<String>,
    plugin_operations: Vec<ApiPluginOperation>,
    plugin_scope_kinds: Vec<String>,
    destructive_operations: Vec<ApiDestructiveOperation>,
}

impl PublicPermissionSchema {
    fn current() -> Self {
        Self {
            schema: PublicSchemaReference::new(
                PUBLIC_PERMISSION_SCHEMA_NAME,
                PUBLIC_API_SCHEMA_VERSION.clone(),
            )
            .expect("the built-in public permission schema name is valid"),
            requirement_modes: ApiPermissionRequirementMode::ALL.to_vec(),
            kinds: ApiPermissionKind::ALL.to_vec(),
            effects: ApiPermissionEffect::ALL.to_vec(),
            filesystem_accesses: ApiFilesystemAccess::ALL.to_vec(),
            filesystem_platforms: ApiFilesystemPlatform::ALL.to_vec(),
            filesystem_path_kinds: vec!["project_relative".to_owned(), "absolute".to_owned()],
            filesystem_scope_kinds: vec!["exact".to_owned(), "recursive".to_owned()],
            plugin_operations: ApiPluginOperation::ALL.to_vec(),
            plugin_scope_kinds: vec!["exact".to_owned(), "all".to_owned()],
            destructive_operations: ApiDestructiveOperation::ALL.to_vec(),
        }
    }

    /// Returns the public permission schema identity.
    #[must_use]
    pub const fn schema(&self) -> &PublicSchemaReference {
        &self.schema
    }

    /// Returns every permission requirement mode.
    #[must_use]
    pub fn requirement_modes(&self) -> &[ApiPermissionRequirementMode] {
        &self.requirement_modes
    }

    /// Returns every protected permission kind.
    #[must_use]
    pub fn kinds(&self) -> &[ApiPermissionKind] {
        &self.kinds
    }

    /// Returns every allow or deny rule effect.
    #[must_use]
    pub fn effects(&self) -> &[ApiPermissionEffect] {
        &self.effects
    }

    /// Returns every filesystem access operation.
    #[must_use]
    pub fn filesystem_accesses(&self) -> &[ApiFilesystemAccess] {
        &self.filesystem_accesses
    }

    /// Returns every filesystem platform syntax.
    #[must_use]
    pub fn filesystem_platforms(&self) -> &[ApiFilesystemPlatform] {
        &self.filesystem_platforms
    }

    /// Returns every filesystem path discriminator.
    #[must_use]
    pub fn filesystem_path_kinds(&self) -> &[String] {
        &self.filesystem_path_kinds
    }

    /// Returns every filesystem scope discriminator.
    #[must_use]
    pub fn filesystem_scope_kinds(&self) -> &[String] {
        &self.filesystem_scope_kinds
    }

    /// Returns every plugin permission operation.
    #[must_use]
    pub fn plugin_operations(&self) -> &[ApiPluginOperation] {
        &self.plugin_operations
    }

    /// Returns every plugin scope discriminator.
    #[must_use]
    pub fn plugin_scope_kinds(&self) -> &[String] {
        &self.plugin_scope_kinds
    }

    /// Returns every destructive operation.
    #[must_use]
    pub fn destructive_operations(&self) -> &[ApiDestructiveOperation] {
        &self.destructive_operations
    }

    fn validate(&self) -> CoreResult<()> {
        self.schema.validate()?;
        if self != &Self::current() {
            return Err(schema_error(
                "validate_permission_schema",
                "public permission schema identity or vocabulary is incompatible",
            ));
        }
        Ok(())
    }
}

/// The deterministic complete public API catalog.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PublicApiSchemaSnapshot {
    #[cfg_attr(feature = "typescript-bindings", specta(type = crate::typescript::SemanticVersionBinding))]
    schema_version: SemanticVersion,
    primitive_schema_revision: u32,
    json_rpc_version: String,
    commands: Vec<PublicMethodSchema>,
    queries: Vec<PublicMethodSchema>,
    events: Vec<PublicEventSchema>,
    resources: Vec<PublicResourceSchema>,
    error: PublicErrorSchema,
    capability: PublicCapabilitySchema,
    permission: PublicPermissionSchema,
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
        permission: PublicPermissionSchema,
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
        permission.validate()?;

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
            permission,
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

    /// Returns the stable permission vocabulary.
    #[must_use]
    pub const fn permission(&self) -> &PublicPermissionSchema {
        &self.permission
    }
}

#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicApiSchemaSnapshotWire {
    #[cfg_attr(feature = "typescript-bindings", specta(type = crate::typescript::SemanticVersionBinding))]
    schema_version: SemanticVersion,
    primitive_schema_revision: u32,
    json_rpc_version: String,
    commands: Vec<PublicMethodSchema>,
    queries: Vec<PublicMethodSchema>,
    events: Vec<PublicEventSchema>,
    resources: Vec<PublicResourceSchema>,
    error: PublicErrorSchema,
    capability: PublicCapabilitySchema,
    permission: PublicPermissionSchema,
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
            wire.permission,
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
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
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
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::None;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[];

    fn permission_requirements(&self) -> CoreResult<ApiPermissionRequirements> {
        Ok(ApiPermissionRequirements::none())
    }
}

/// Successful public schema discovery response.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicErrorContext {
    component: String,
    operation: String,
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = crate::typescript::TraceValueMapBinding)
    )]
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
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicResourceReference {
    resource: String,
    #[cfg_attr(feature = "typescript-bindings", specta(type = crate::typescript::SemanticVersionBinding))]
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
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicErrorData {
    #[cfg_attr(feature = "typescript-bindings", specta(type = crate::typescript::SemanticVersionBinding))]
    schema_version: SemanticVersion,
    primitive_schema_revision: u32,
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = crate::typescript::ErrorCategoryBinding)
    )]
    category: ErrorCategory,
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = crate::typescript::RecoverabilityBinding)
    )]
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
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
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

#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
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

#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
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

#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
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

#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
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
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum JsonRpcResponse<T> {
    /// Successful typed response.
    Success(JsonRpcSuccessResponse<T>),
    /// Structured public failure response.
    Failure(Box<JsonRpcFailureResponse>),
}

macro_rules! for_each_public_method {
    ($callback:ident, $($argument:expr),+ $(,)?) => {
        $callback::<GetAsyncJobs>($($argument),+)?;
        $callback::<PauseAsyncJob>($($argument),+)?;
        $callback::<ResumeAsyncJob>($($argument),+)?;
        $callback::<RetryAsyncJob>($($argument),+)?;
        $callback::<CancelAsyncJob>($($argument),+)?;
        $callback::<CancelAllAsyncJobs>($($argument),+)?;
        $callback::<RemoveAsyncJob>($($argument),+)?;
        $callback::<GetProjectRecovery>($($argument),+)?;
        $callback::<ExecuteProjectCommand>($($argument),+)?;
        $callback::<CompareProjectRecovery>($($argument),+)?;
        $callback::<RestoreProjectRecovery>($($argument),+)?;
        $callback::<DismissProjectRecovery>($($argument),+)?;
        $callback::<RunProjectScript>($($argument),+)?;
        $callback::<GetAudioAutomation>($($argument),+)?;
        $callback::<GetEditorState>($($argument),+)?;
        $callback::<ExecuteAudioAutomationTransaction>($($argument),+)?;
        $callback::<GetProjectSettings>($($argument),+)?;
        $callback::<ExecuteProjectSettingsTransaction>($($argument),+)?;
        $callback::<GetMediaCapabilities>($($argument),+)?;
        $callback::<GetEngineIntegrationValidation>($($argument),+)?;
        $callback::<GetEngineIntrospection>($($argument),+)?;
        $callback::<ExecuteScenarioAction>($($argument),+)?;
        $callback::<ExecuteScenarioTransaction>($($argument),+)?;
        $callback::<OpenEventSubscription>($($argument),+)?;
        $callback::<CloseEventSubscription>($($argument),+)?;
        $callback::<PollEvents>($($argument),+)?;
        $callback::<GetPublicApiSchema>($($argument),+)?;
    };
}

macro_rules! for_each_public_event {
    ($callback:ident, $argument:expr $(,)?) => {
        $callback::<ProjectStateChanged>($argument)?;
        $callback::<AsyncJobsChanged>($argument)?;
        $callback::<ProjectRecoveryChanged>($argument)?;
        $callback::<AudioAutomationChanged>($argument)?;
        $callback::<ProjectSettingsChanged>($argument)?;
        $callback::<MediaCapabilitiesChanged>($argument)?;
        $callback::<EngineIntrospectionChanged>($argument)?;
        $callback::<ScenarioStateChanged>($argument)?;
    };
}

macro_rules! for_each_public_resource {
    ($callback:ident, $argument:expr $(,)?) => {
        $callback::<ProjectHistorySnapshot>($argument)?;
        $callback::<AsyncJobsSnapshot>($argument)?;
        $callback::<ProjectRecoverySnapshot>($argument)?;
        $callback::<AudioAutomationSnapshot>($argument)?;
        $callback::<EditorStateSnapshot>($argument)?;
        $callback::<ProjectSettingsSnapshot>($argument)?;
        $callback::<MediaCapabilitiesSnapshot>($argument)?;
        $callback::<EngineIntrospectionSnapshot>($argument)?;
        $callback::<IntegrationValidationSnapshot>($argument)?;
        $callback::<ScenarioStateSnapshot>($argument)?;
        $callback::<EventStreamSnapshot>($argument)?;
    };
}

fn current_catalog() -> CoreResult<PublicApiSchemaSnapshot> {
    let mut commands = Vec::new();
    let mut queries = Vec::new();
    let mut events = Vec::new();
    let mut resources = Vec::new();

    for_each_public_method!(register_method, &mut commands, &mut queries);
    for_each_public_event!(register_event, &mut events);
    for_each_public_resource!(register_resource, &mut resources);

    PublicApiSchemaSnapshot::try_new(
        PUBLIC_API_SCHEMA_VERSION.clone(),
        STABLE_PRIMITIVE_SCHEMA_REVISION,
        JSON_RPC_VERSION,
        commands,
        queries,
        events,
        resources,
        PublicErrorSchema::current(),
        PublicCapabilitySchema::current(),
        PublicPermissionSchema::current(),
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
        PublicMethodPermissionSchema::new(C::PERMISSION_MODE, C::PERMISSION_KINDS.iter().copied())?,
    )?;
    match C::KIND {
        PublicMethodKind::Command => commands.push(descriptor),
        PublicMethodKind::Query => queries.push(descriptor),
    }
    Ok(())
}

fn register_event<E: ApiEvent>(events: &mut Vec<PublicEventSchema>) -> CoreResult<()> {
    events.push(event_schema::<E>()?);
    Ok(())
}

fn register_resource<R: ApiResource>(resources: &mut Vec<PublicResourceSchema>) -> CoreResult<()> {
    resources.push(resource_schema::<R>()?);
    Ok(())
}

#[cfg(feature = "typescript-bindings")]
pub(crate) fn register_typescript_surface(
    registry: &mut crate::typescript::BindingRegistry,
) -> Result<(), crate::typescript::TypeScriptBindingError> {
    use crate::typescript::{
        register_event as register_typescript_event, register_method as register_typescript_method,
        register_resource as register_typescript_resource,
    };

    for_each_public_method!(register_typescript_method, registry);
    for_each_public_event!(register_typescript_event, registry);
    for_each_public_resource!(register_typescript_resource, registry);
    registry.register_common::<PublicApiError>()?;
    registry.register_common::<crate::scripting::ProjectScriptProgram>()?;
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
