//! Public extension registration, capability discovery, lifecycle, and user-control read model.
//!
//! This domain is read-only. Durable extension state continues to mutate only through
//! `superi.project.command.execute`, under the existing typed permission and revision fences.

use std::str::FromStr;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::{
    CapabilityId, FeatureAvailability, FeatureId, SemanticVersion, VersionIdentifier,
};
use superi_engine::extensions::{
    ExtensionControlOperation as EngineControlOperation,
    ExtensionFailureSummary as EngineFailureSummary, ExtensionLifecycle as EngineLifecycle,
    ExtensionRegistration as EngineRegistration, ExtensionRegistrySnapshot as EngineSnapshot,
    ExtensionRuntimeIdentity as EngineIdentity, ExtensionUserAction as EngineUserAction,
};

use crate::commands::ApiCommand;
use crate::events::ExtensionsChanged;
use crate::permissions::{
    ApiPermissionKind, ApiPermissionRequirementMode, ApiPermissionRequirements,
};
use crate::schema::{ApiResource, PublicMethodKind};
use crate::version::{EXTENSIONS_RESOURCE, EXTENSIONS_SCHEMA_VERSION, GET_EXTENSIONS_METHOD};

const COMPONENT: &str = "superi-api.extensions";
const PROJECT_COMMAND_METHOD: &str = "superi.project.command.execute";
const EDITOR_STATE_RESOURCE: &str = "superi.editor.state";
const MAX_REGISTRATIONS: usize = 4_096;
const MAX_CAPABILITIES: usize = 1_024;
const MAX_FEATURES: usize = 1_024;

/// Exact public runtime identity.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum ExtensionIdentity {
    Versioned {
        producer: String,
    },
    NativeAudio {
        provider: String,
        format: String,
        vendor: String,
        identifier: String,
        version: String,
    },
}

impl ExtensionIdentity {
    /// Returns the canonical producer used by feature discovery.
    #[must_use]
    pub fn producer(&self) -> &str {
        match self {
            Self::Versioned { producer } => producer,
            Self::NativeAudio { provider, .. } => provider,
        }
    }

    fn producer_identifier(&self) -> Result<VersionIdentifier> {
        VersionIdentifier::from_str(self.producer()).map_err(|_| {
            invalid(
                "validate_identity",
                "extension producer is not a canonical version identifier",
            )
        })
    }

    fn validate(&self) -> Result<()> {
        let _ = self.producer_identifier()?;
        if let Self::NativeAudio {
            format,
            vendor,
            identifier,
            version,
            ..
        } = self
        {
            if !matches!(format.as_str(), "audio_unit" | "vst3")
                || [vendor, identifier, version].iter().any(|value| {
                    value.is_empty() || value.len() > 1_024 || value.chars().any(char::is_control)
                })
            {
                return Err(invalid(
                    "validate_identity",
                    "native audio identity contains an unknown format or invalid exact field",
                ));
            }
        }
        Ok(())
    }

    fn sort_key(&self) -> (u8, &str, &str, &str, &str, &str) {
        match self {
            Self::Versioned { producer } => (0, producer, "", "", "", ""),
            Self::NativeAudio {
                provider,
                format,
                vendor,
                identifier,
                version,
            } => (1, provider, format, vendor, identifier, version),
        }
    }
}

/// Current runtime lifecycle, distinct from durable project record lifecycle.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionLifecycle {
    Registered,
    Disabled,
    Ready,
    Faulted,
    Quarantined,
    Unavailable,
}

/// Recommended user response to one safe failure summary.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionUserAction {
    Retry,
    ContinueDegraded,
    CorrectConfiguration,
    Stop,
}

/// Safe extension failure with no path, raw message, adapter, or private context.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionFailure {
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
    stage: String,
    total_failures: u64,
    consecutive_failures: u32,
    recommended_action: ExtensionUserAction,
}

impl ExtensionFailure {
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        self.category
    }

    #[must_use]
    pub const fn recoverability(&self) -> Recoverability {
        self.recoverability
    }

    #[must_use]
    pub fn stage(&self) -> &str {
        &self.stage
    }

    #[must_use]
    pub const fn total_failures(&self) -> u64 {
        self.total_failures
    }

    #[must_use]
    pub const fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    #[must_use]
    pub const fn recommended_action(&self) -> ExtensionUserAction {
        self.recommended_action
    }

    fn validate(&self) -> Result<()> {
        let expected = action_for(self.recoverability);
        if self.stage.is_empty()
            || self.stage.len() > 128
            || self.stage.chars().any(char::is_control)
            || self.total_failures == 0
            || self.consecutive_failures == 0
            || u64::from(self.consecutive_failures) > self.total_failures
            || self.recommended_action != expected
        {
            return Err(invalid(
                "validate_failure",
                "extension failure classification, stage, counters, or action is inconsistent",
            ));
        }
        Ok(())
    }
}

/// One discoverable extension feature.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionFeature {
    id: String,
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = crate::typescript::SemanticVersionBinding)
    )]
    version: SemanticVersion,
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = crate::typescript::FeatureAvailabilityBinding)
    )]
    availability: FeatureAvailability,
    required_capabilities: Vec<String>,
}

impl ExtensionFeature {
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub const fn version(&self) -> &SemanticVersion {
        &self.version
    }

    #[must_use]
    pub const fn availability(&self) -> FeatureAvailability {
        self.availability
    }

    #[must_use]
    pub fn required_capabilities(&self) -> &[String] {
        &self.required_capabilities
    }

    fn validate(&self, capabilities: &[String]) -> Result<()> {
        FeatureId::new(&self.id).map_err(|_| {
            invalid(
                "validate_feature",
                "extension feature identity is not canonical",
            )
        })?;
        validate_capabilities(&self.required_capabilities)?;
        if self.availability == FeatureAvailability::Available
            && self
                .required_capabilities
                .iter()
                .any(|value| capabilities.binary_search(value).is_err())
        {
            return Err(invalid(
                "validate_feature",
                "available extension feature requires an ungranted capability",
            ));
        }
        Ok(())
    }
}

/// One core-compatible discovery snapshot with capabilities equal to the granted set.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionDiscovery {
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = crate::typescript::SemanticVersionBinding)
    )]
    schema_version: SemanticVersion,
    producer: String,
    capabilities: Vec<String>,
    features: Vec<ExtensionFeature>,
}

impl ExtensionDiscovery {
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    #[must_use]
    pub fn producer(&self) -> &str {
        &self.producer
    }

    #[must_use]
    pub fn capabilities(&self) -> &[String] {
        &self.capabilities
    }

    #[must_use]
    pub fn features(&self) -> &[ExtensionFeature] {
        &self.features
    }

    fn validate(&self) -> Result<()> {
        if self.schema_version != EXTENSIONS_SCHEMA_VERSION {
            return Err(invalid(
                "validate_discovery",
                "extension discovery schema version is incompatible",
            ));
        }
        VersionIdentifier::from_str(&self.producer).map_err(|_| {
            invalid(
                "validate_discovery",
                "extension discovery producer is not canonical",
            )
        })?;
        validate_capabilities(&self.capabilities)?;
        if self.features.len() > MAX_FEATURES
            || self
                .features
                .windows(2)
                .any(|pair| pair[0].id >= pair[1].id)
        {
            return Err(invalid(
                "validate_discovery",
                "extension feature list is oversized, duplicated, or unsorted",
            ));
        }
        for feature in &self.features {
            feature.validate(&self.capabilities)?;
        }
        Ok(())
    }
}

/// Durable operation available through the stable project command surface.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtensionControlOperation {
    Upsert,
    Remove,
    SetLifecycle,
    SetGrantedCapabilities,
    RecordFailure,
    ClearFailure,
}

const CONTROL_OPERATIONS: [ExtensionControlOperation; 6] = [
    ExtensionControlOperation::Upsert,
    ExtensionControlOperation::Remove,
    ExtensionControlOperation::SetLifecycle,
    ExtensionControlOperation::SetGrantedCapabilities,
    ExtensionControlOperation::RecordFailure,
    ExtensionControlOperation::ClearFailure,
];

/// Explicit stable route for user-controlled durable extension state.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionControl {
    extension_id: String,
    command_method: String,
    state_resource: String,
    operations: Vec<ExtensionControlOperation>,
}

impl ExtensionControl {
    #[must_use]
    pub fn extension_id(&self) -> &str {
        &self.extension_id
    }

    #[must_use]
    pub fn command_method(&self) -> &str {
        &self.command_method
    }

    #[must_use]
    pub fn state_resource(&self) -> &str {
        &self.state_resource
    }

    #[must_use]
    pub fn operations(&self) -> &[ExtensionControlOperation] {
        &self.operations
    }

    fn validate(&self, identity: &ExtensionIdentity) -> Result<()> {
        let producer = identity.producer_identifier()?;
        if self.extension_id != producer.component().as_str()
            || self.command_method != PROJECT_COMMAND_METHOD
            || self.state_resource != EDITOR_STATE_RESOURCE
            || self.operations != CONTROL_OPERATIONS
        {
            return Err(invalid(
                "validate_control",
                "extension control does not use the canonical project command surface",
            ));
        }
        Ok(())
    }
}

/// One complete public runtime registration.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionRegistration {
    identity: ExtensionIdentity,
    display_name: String,
    requested_capabilities: Vec<String>,
    granted_capabilities: Vec<String>,
    discovery: ExtensionDiscovery,
    lifecycle: ExtensionLifecycle,
    failure: Option<ExtensionFailure>,
    control: ExtensionControl,
}

impl ExtensionRegistration {
    #[must_use]
    pub const fn identity(&self) -> &ExtensionIdentity {
        &self.identity
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub fn requested_capabilities(&self) -> &[String] {
        &self.requested_capabilities
    }

    #[must_use]
    pub fn granted_capabilities(&self) -> &[String] {
        &self.granted_capabilities
    }

    #[must_use]
    pub const fn discovery(&self) -> &ExtensionDiscovery {
        &self.discovery
    }

    #[must_use]
    pub const fn lifecycle(&self) -> ExtensionLifecycle {
        self.lifecycle
    }

    #[must_use]
    pub const fn failure(&self) -> Option<&ExtensionFailure> {
        self.failure.as_ref()
    }

    #[must_use]
    pub const fn control(&self) -> &ExtensionControl {
        &self.control
    }

    fn validate(&self) -> Result<()> {
        self.identity.validate()?;
        self.discovery.validate()?;
        self.control.validate(&self.identity)?;
        validate_capabilities(&self.requested_capabilities)?;
        validate_capabilities(&self.granted_capabilities)?;
        if self.display_name.is_empty()
            || self.display_name.len() > 512
            || self.display_name.chars().any(char::is_control)
            || self.discovery.producer != self.identity.producer()
            || self.discovery.capabilities != self.granted_capabilities
            || self
                .granted_capabilities
                .iter()
                .any(|value| self.requested_capabilities.binary_search(value).is_err())
        {
            return Err(invalid(
                "validate_registration",
                "extension registration identity, capabilities, discovery, or display name is inconsistent",
            ));
        }
        if self.lifecycle != ExtensionLifecycle::Ready
            && self
                .discovery
                .features
                .iter()
                .any(|feature| feature.availability == FeatureAvailability::Available)
        {
            return Err(invalid(
                "validate_registration",
                "only a ready extension may advertise an available feature",
            ));
        }
        if matches!(
            self.lifecycle,
            ExtensionLifecycle::Faulted | ExtensionLifecycle::Quarantined
        ) && self.failure.is_none()
        {
            return Err(invalid(
                "validate_registration",
                "faulted or quarantined extension requires safe failure evidence",
            ));
        }
        if let Some(failure) = &self.failure {
            failure.validate()?;
        }
        Ok(())
    }
}

/// Complete deterministic public replacement state.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct ExtensionRegistrySnapshot {
    #[cfg_attr(
        feature = "typescript-bindings",
        specta(type = crate::typescript::SemanticVersionBinding)
    )]
    schema_version: SemanticVersion,
    revision: u64,
    registrations: Vec<ExtensionRegistration>,
}

impl ExtensionRegistrySnapshot {
    /// Converts complete engine-owned state without copying privileged runtime handles.
    #[must_use]
    pub fn from_engine(value: &EngineSnapshot) -> Self {
        Self {
            schema_version: EXTENSIONS_SCHEMA_VERSION,
            revision: value.revision(),
            registrations: value
                .registrations()
                .iter()
                .map(public_registration)
                .collect(),
        }
    }

    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn registrations(&self) -> &[ExtensionRegistration] {
        &self.registrations
    }

    fn same_state(&self, other: &Self) -> bool {
        self.schema_version == other.schema_version && self.registrations == other.registrations
    }

    fn validate(&self) -> Result<()> {
        if self.schema_version != EXTENSIONS_SCHEMA_VERSION
            || self.registrations.len() > MAX_REGISTRATIONS
        {
            return Err(invalid(
                "validate_snapshot",
                "extension registry schema or registration bound is invalid",
            ));
        }
        let mut previous = None;
        for registration in &self.registrations {
            registration.validate()?;
            let key = registration.identity.sort_key();
            if previous.as_ref().is_some_and(|value| value >= &key) {
                return Err(invalid(
                    "validate_snapshot",
                    "extension registrations are duplicated or not in canonical identity order",
                ));
            }
            previous = Some(key);
        }
        Ok(())
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ExtensionRegistrySnapshotWire {
    schema_version: SemanticVersion,
    revision: u64,
    registrations: Vec<ExtensionRegistration>,
}

impl<'de> Deserialize<'de> for ExtensionRegistrySnapshot {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ExtensionRegistrySnapshotWire::deserialize(deserializer)?;
        let value = Self {
            schema_version: wire.schema_version,
            revision: wire.revision,
            registrations: wire.registrations,
        };
        value.validate().map_err(D::Error::custom)?;
        Ok(value)
    }
}

impl ApiResource for ExtensionRegistrySnapshot {
    const RESOURCE: &'static str = EXTENSIONS_RESOURCE;
    const SCHEMA_VERSION: SemanticVersion = EXTENSIONS_SCHEMA_VERSION;
}

/// Strict empty query for complete extension discovery state.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetExtensions {}

impl GetExtensions {
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }
}

impl ApiCommand for GetExtensions {
    type Response = GetExtensionsResult;
    const METHOD: &'static str = GET_EXTENSIONS_METHOD;
    const KIND: PublicMethodKind = PublicMethodKind::Query;
    const SCHEMA_VERSION: SemanticVersion = EXTENSIONS_SCHEMA_VERSION;
    const PERMISSION_MODE: ApiPermissionRequirementMode = ApiPermissionRequirementMode::None;
    const PERMISSION_KINDS: &'static [ApiPermissionKind] = &[];

    fn permission_requirements(&self) -> Result<ApiPermissionRequirements> {
        Ok(ApiPermissionRequirements::none())
    }
}

/// Successful extension discovery response.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetExtensionsResult {
    snapshot: ExtensionRegistrySnapshot,
}

impl GetExtensionsResult {
    #[must_use]
    pub const fn snapshot(&self) -> &ExtensionRegistrySnapshot {
        &self.snapshot
    }
}

/// Public read owner that emits full replacement events only on semantic change.
#[derive(Clone, Debug)]
pub struct ExtensionRegistryApi {
    snapshot: ExtensionRegistrySnapshot,
}

impl ExtensionRegistryApi {
    #[must_use]
    pub fn new(engine: &EngineSnapshot) -> Self {
        Self {
            snapshot: ExtensionRegistrySnapshot::from_engine(engine),
        }
    }

    #[must_use]
    pub fn execute(&self, _query: GetExtensions) -> GetExtensionsResult {
        GetExtensionsResult {
            snapshot: self.snapshot.clone(),
        }
    }

    /// Synchronizes one engine snapshot and emits only a complete semantic replacement.
    pub fn synchronize(&mut self, engine: &EngineSnapshot) -> Result<Option<ExtensionsChanged>> {
        let next = ExtensionRegistrySnapshot::from_engine(engine);
        if self.snapshot.same_state(&next) {
            return Ok(None);
        }
        if next.revision <= self.snapshot.revision {
            return Err(Error::new(
                ErrorCategory::CorruptData,
                Recoverability::Terminal,
                "extension registry revision did not advance with changed state",
            )
            .with_context(ErrorContext::new(COMPONENT, "synchronize")));
        }
        self.snapshot = next.clone();
        Ok(Some(ExtensionsChanged::new(next)))
    }
}

fn public_registration(value: &EngineRegistration) -> ExtensionRegistration {
    ExtensionRegistration {
        identity: match value.identity() {
            EngineIdentity::Versioned { producer } => ExtensionIdentity::Versioned {
                producer: producer.to_string(),
            },
            EngineIdentity::NativeAudio {
                provider,
                format,
                vendor,
                identifier,
                version,
            } => ExtensionIdentity::NativeAudio {
                provider: provider.to_string(),
                format: format.code().to_owned(),
                vendor: vendor.clone(),
                identifier: identifier.clone(),
                version: version.clone(),
            },
        },
        display_name: value.display_name().to_owned(),
        requested_capabilities: capability_strings(value.requested_capabilities()),
        granted_capabilities: capability_strings(value.granted_capabilities()),
        discovery: ExtensionDiscovery {
            schema_version: value.discovery().schema_version().clone(),
            producer: value.discovery().producer().to_string(),
            capabilities: capability_strings(value.discovery().capabilities()),
            features: value
                .discovery()
                .iter()
                .map(|feature| ExtensionFeature {
                    id: feature.id().as_str().to_owned(),
                    version: feature.version().clone(),
                    availability: feature.availability(),
                    required_capabilities: capability_strings(feature.required_capabilities()),
                })
                .collect(),
        },
        lifecycle: public_lifecycle(value.lifecycle()),
        failure: value.failure().map(public_failure),
        control: ExtensionControl {
            extension_id: value.control().extension_id().as_str().to_owned(),
            command_method: value.control().command_method().to_owned(),
            state_resource: value.control().state_resource().to_owned(),
            operations: value
                .control()
                .operations()
                .iter()
                .copied()
                .map(public_control_operation)
                .collect(),
        },
    }
}

fn public_failure(value: &EngineFailureSummary) -> ExtensionFailure {
    ExtensionFailure {
        category: value.category(),
        recoverability: value.recoverability(),
        stage: value.stage().to_owned(),
        total_failures: value.total_failures(),
        consecutive_failures: value.consecutive_failures(),
        recommended_action: match value.recommended_action() {
            EngineUserAction::Retry => ExtensionUserAction::Retry,
            EngineUserAction::ContinueDegraded => ExtensionUserAction::ContinueDegraded,
            EngineUserAction::CorrectConfiguration => ExtensionUserAction::CorrectConfiguration,
            EngineUserAction::Stop => ExtensionUserAction::Stop,
            _ => ExtensionUserAction::Stop,
        },
    }
}

fn public_lifecycle(value: EngineLifecycle) -> ExtensionLifecycle {
    match value {
        EngineLifecycle::Registered => ExtensionLifecycle::Registered,
        EngineLifecycle::Disabled => ExtensionLifecycle::Disabled,
        EngineLifecycle::Ready => ExtensionLifecycle::Ready,
        EngineLifecycle::Faulted => ExtensionLifecycle::Faulted,
        EngineLifecycle::Quarantined => ExtensionLifecycle::Quarantined,
        EngineLifecycle::Unavailable => ExtensionLifecycle::Unavailable,
        _ => ExtensionLifecycle::Unavailable,
    }
}

const fn public_control_operation(value: EngineControlOperation) -> ExtensionControlOperation {
    match value {
        EngineControlOperation::Upsert => ExtensionControlOperation::Upsert,
        EngineControlOperation::Remove => ExtensionControlOperation::Remove,
        EngineControlOperation::SetLifecycle => ExtensionControlOperation::SetLifecycle,
        EngineControlOperation::SetGrantedCapabilities => {
            ExtensionControlOperation::SetGrantedCapabilities
        }
        EngineControlOperation::RecordFailure => ExtensionControlOperation::RecordFailure,
        EngineControlOperation::ClearFailure => ExtensionControlOperation::ClearFailure,
    }
}

fn capability_strings(value: &superi_core::settings::CapabilitySet) -> Vec<String> {
    value.iter().map(|item| item.as_str().to_owned()).collect()
}

fn validate_capabilities(values: &[String]) -> Result<()> {
    if values.len() > MAX_CAPABILITIES
        || values.windows(2).any(|pair| pair[0] >= pair[1])
        || values.iter().any(|value| CapabilityId::new(value).is_err())
    {
        return Err(invalid(
            "validate_capabilities",
            "extension capabilities are oversized, invalid, duplicated, or unsorted",
        ));
    }
    Ok(())
}

const fn action_for(value: Recoverability) -> ExtensionUserAction {
    match value {
        Recoverability::Retryable => ExtensionUserAction::Retry,
        Recoverability::Degraded => ExtensionUserAction::ContinueDegraded,
        Recoverability::UserCorrectable => ExtensionUserAction::CorrectConfiguration,
        Recoverability::Terminal => ExtensionUserAction::Stop,
        _ => ExtensionUserAction::Stop,
    }
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
