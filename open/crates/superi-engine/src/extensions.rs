//! Engine-owned extension registration and capability discovery.
//!
//! Registration is a declarative process-lifetime read model. It never gives an extension a
//! dispatcher, callback, project path, worker adapter, or lower-domain mutation handle. Durable
//! extension state remains project-owned and changes only through the stable public project command
//! surface.

use std::collections::BTreeMap;

use superi_audio::plugins::AudioPluginFormat;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::{
    CapabilitySet, ComponentId, FeatureAvailability, FeatureDescriptor, FeatureDiscovery,
    FeatureId, SemanticVersion, VersionIdentifier,
};
use superi_effects::ofx::OfxPluginLifecycle;

use crate::audio_plugins::{
    AudioPluginLifecycle, AudioPluginStatusSnapshot, AudioPluginSupervisor,
};
use crate::plugins::{PluginStatusSnapshot, PluginSupervisor};

const COMPONENT: &str = "superi-engine.extensions";
const AUDIO_PLUGIN_PROVIDER: &str = "superi.audio-plugin-host";
const PROJECT_COMMAND_METHOD: &str = "superi.project.command.execute";
const EDITOR_STATE_RESOURCE: &str = "superi.editor.state";
const MAX_REGISTRATIONS: usize = 4_096;
const MAX_CAPABILITIES: usize = 1_024;
const MAX_FEATURES: usize = 1_024;
const MAX_DISPLAY_NAME_BYTES: usize = 512;
const MAX_FAILURE_STAGE_BYTES: usize = 128;

/// Schema version for the process-lifetime extension registry.
pub const EXTENSION_REGISTRY_SCHEMA_VERSION: SemanticVersion = SemanticVersion::new(1, 0, 0);

/// Exact runtime identity without a synthesized native semantic version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExtensionRuntimeIdentity {
    /// Built-in, WASM, OpenFX, or another component with a canonical semantic version.
    Versioned { producer: VersionIdentifier },
    /// Native audio identity retained byte-for-byte under the canonical audio host provider.
    NativeAudio {
        provider: VersionIdentifier,
        format: AudioPluginFormat,
        vendor: String,
        identifier: String,
        version: String,
    },
}

impl ExtensionRuntimeIdentity {
    /// Creates one versioned runtime identity.
    #[must_use]
    pub const fn versioned(producer: VersionIdentifier) -> Self {
        Self::Versioned { producer }
    }

    /// Returns the component that produced the discovery snapshot.
    #[must_use]
    pub const fn producer(&self) -> &VersionIdentifier {
        match self {
            Self::Versioned { producer }
            | Self::NativeAudio {
                provider: producer, ..
            } => producer,
        }
    }

    /// Returns exact native audio fields when this identity is format-owned.
    #[must_use]
    pub fn native_audio(&self) -> Option<(AudioPluginFormat, &str, &str, &str)> {
        match self {
            Self::NativeAudio {
                format,
                vendor,
                identifier,
                version,
                ..
            } => Some((*format, vendor, identifier, version)),
            Self::Versioned { .. } => None,
        }
    }

    fn key(&self) -> ExtensionIdentityKey {
        match self {
            Self::Versioned { producer } => ExtensionIdentityKey::Versioned {
                component: producer.component().as_str().to_owned(),
                version: producer.version().to_string(),
            },
            Self::NativeAudio {
                provider,
                format,
                vendor,
                identifier,
                version,
            } => ExtensionIdentityKey::NativeAudio {
                provider_component: provider.component().as_str().to_owned(),
                provider_version: provider.version().to_string(),
                format: format.code().to_owned(),
                vendor: vendor.clone(),
                identifier: identifier.clone(),
                version: version.clone(),
            },
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ExtensionIdentityKey {
    Versioned {
        component: String,
        version: String,
    },
    NativeAudio {
        provider_component: String,
        provider_version: String,
        format: String,
        vendor: String,
        identifier: String,
        version: String,
    },
}

/// Current process-lifetime extension state.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ExtensionLifecycle {
    Registered,
    Disabled,
    Ready,
    Faulted,
    Quarantined,
    Unavailable,
}

/// User-facing response implied by a safe failure classification.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ExtensionUserAction {
    Retry,
    ContinueDegraded,
    CorrectConfiguration,
    Stop,
}

impl ExtensionUserAction {
    fn from_recoverability(value: Recoverability) -> Self {
        match value {
            Recoverability::Retryable => Self::Retry,
            Recoverability::Degraded => Self::ContinueDegraded,
            Recoverability::UserCorrectable => Self::CorrectConfiguration,
            Recoverability::Terminal => Self::Stop,
            _ => Self::Stop,
        }
    }
}

/// Bounded failure state with no path, raw message, worker, or internal context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionFailureSummary {
    category: ErrorCategory,
    recoverability: Recoverability,
    stage: String,
    total_failures: u64,
    consecutive_failures: u32,
    recommended_action: ExtensionUserAction,
}

impl ExtensionFailureSummary {
    /// Creates one safe failure projection.
    pub fn new(
        category: ErrorCategory,
        recoverability: Recoverability,
        stage: impl Into<String>,
        total_failures: u64,
        consecutive_failures: u32,
    ) -> Result<Self> {
        let stage = stage.into();
        if stage.is_empty()
            || stage.len() > MAX_FAILURE_STAGE_BYTES
            || stage.chars().any(char::is_control)
            || total_failures == 0
            || consecutive_failures == 0
            || u64::from(consecutive_failures) > total_failures
        {
            return Err(extension_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_failure",
                "extension failure stage or counters are invalid",
            ));
        }
        Ok(Self {
            category,
            recoverability,
            stage,
            total_failures,
            consecutive_failures,
            recommended_action: ExtensionUserAction::from_recoverability(recoverability),
        })
    }

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
}

/// Durable extension operations available through the one public project command.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExtensionControlOperation {
    Upsert,
    Remove,
    SetLifecycle,
    SetGrantedCapabilities,
    RecordFailure,
    ClearFailure,
}

/// Explicit pointer from runtime discovery to durable, permission-checked user control.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionControlSurface {
    extension_id: ComponentId,
    command_method: &'static str,
    state_resource: &'static str,
    operations: [ExtensionControlOperation; 6],
}

impl ExtensionControlSurface {
    fn project_extensions(extension_id: ComponentId) -> Self {
        Self {
            extension_id,
            command_method: PROJECT_COMMAND_METHOD,
            state_resource: EDITOR_STATE_RESOURCE,
            operations: [
                ExtensionControlOperation::Upsert,
                ExtensionControlOperation::Remove,
                ExtensionControlOperation::SetLifecycle,
                ExtensionControlOperation::SetGrantedCapabilities,
                ExtensionControlOperation::RecordFailure,
                ExtensionControlOperation::ClearFailure,
            ],
        }
    }

    #[must_use]
    pub const fn extension_id(&self) -> &ComponentId {
        &self.extension_id
    }

    #[must_use]
    pub const fn command_method(&self) -> &'static str {
        self.command_method
    }

    #[must_use]
    pub const fn state_resource(&self) -> &'static str {
        self.state_resource
    }

    #[must_use]
    pub const fn operations(&self) -> &[ExtensionControlOperation; 6] {
        &self.operations
    }
}

/// One complete declarative runtime registration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionRegistration {
    identity: ExtensionRuntimeIdentity,
    display_name: String,
    requested_capabilities: CapabilitySet,
    granted_capabilities: CapabilitySet,
    discovery: FeatureDiscovery,
    lifecycle: ExtensionLifecycle,
    failure: Option<ExtensionFailureSummary>,
    control: ExtensionControlSurface,
}

impl ExtensionRegistration {
    /// Creates one versioned built-in, WASM, OpenFX, or other declarative registration.
    #[allow(clippy::too_many_arguments)]
    pub fn versioned(
        producer: VersionIdentifier,
        display_name: impl Into<String>,
        requested_capabilities: CapabilitySet,
        granted_capabilities: CapabilitySet,
        lifecycle: ExtensionLifecycle,
        features: impl IntoIterator<Item = FeatureDescriptor>,
        failure: Option<ExtensionFailureSummary>,
    ) -> Result<Self> {
        let discovery = FeatureDiscovery::new(
            EXTENSION_REGISTRY_SCHEMA_VERSION,
            producer.clone(),
            granted_capabilities.clone(),
            features,
        )?;
        Self::new(
            ExtensionRuntimeIdentity::versioned(producer),
            display_name,
            requested_capabilities,
            granted_capabilities,
            discovery,
            lifecycle,
            failure,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new(
        identity: ExtensionRuntimeIdentity,
        display_name: impl Into<String>,
        requested_capabilities: CapabilitySet,
        granted_capabilities: CapabilitySet,
        discovery: FeatureDiscovery,
        lifecycle: ExtensionLifecycle,
        failure: Option<ExtensionFailureSummary>,
    ) -> Result<Self> {
        let display_name = display_name.into();
        let control =
            ExtensionControlSurface::project_extensions(identity.producer().component().clone());
        let value = Self {
            identity,
            display_name,
            requested_capabilities,
            granted_capabilities,
            discovery,
            lifecycle,
            failure,
            control,
        };
        value.validate()?;
        Ok(value)
    }

    /// Converts a real supervised OpenFX status without retaining its bundle path.
    pub fn from_openfx(status: &PluginStatusSnapshot) -> Result<Self> {
        let descriptor = status.descriptor();
        let lifecycle = public_ofx_lifecycle(status.lifecycle());
        let availability = feature_availability(lifecycle);
        let requested = descriptor.requested_permissions().clone();
        let granted = status.ofx_status().granted_permissions().clone();
        let features = descriptor
            .contexts()
            .map(|context| {
                let id = FeatureId::new(format!(
                    "{}.context.{}",
                    descriptor.identity().identifier(),
                    context.code()
                ))
                .map_err(|_| invalid_feature("OpenFX context feature identity is invalid"))?;
                Ok(FeatureDescriptor::new(
                    id,
                    descriptor.identity().version().clone(),
                    availability,
                    requested.clone(),
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        let failure = status
            .ofx_status()
            .last_failure()
            .map(|failure| {
                ExtensionFailureSummary::new(
                    ErrorCategory::from_code(failure.category()).unwrap_or(ErrorCategory::Internal),
                    Recoverability::from_code(failure.recoverability())
                        .unwrap_or(Recoverability::Terminal),
                    failure.action().code(),
                    status.total_failures(),
                    status.consecutive_failures(),
                )
            })
            .transpose()?;
        Self::versioned(
            VersionIdentifier::new(
                descriptor.identity().identifier().clone(),
                descriptor.identity().version().clone(),
            ),
            descriptor.label(),
            requested,
            granted,
            lifecycle,
            features,
            failure,
        )
    }

    /// Converts a real supervised native audio status without retaining its candidate source.
    pub fn from_native_audio(status: &AudioPluginStatusSnapshot) -> Result<Self> {
        let descriptor = status.descriptor();
        let native = descriptor.identity();
        let provider = VersionIdentifier::new(
            ComponentId::new(AUDIO_PLUGIN_PROVIDER)
                .expect("the built-in audio plugin provider is canonical"),
            SemanticVersion::new(1, 0, 0),
        );
        let lifecycle = public_audio_lifecycle(status.lifecycle());
        let discovery = FeatureDiscovery::new(
            EXTENSION_REGISTRY_SCHEMA_VERSION,
            provider.clone(),
            CapabilitySet::default(),
            [FeatureDescriptor::new(
                FeatureId::new("superi.audio-plugin-host.process")
                    .expect("the built-in audio process feature is canonical"),
                SemanticVersion::new(1, 0, 0),
                feature_availability(lifecycle),
                CapabilitySet::default(),
            )],
        )?;
        let failure = status
            .last_failure()
            .map(|failure| {
                ExtensionFailureSummary::new(
                    failure.category(),
                    failure.recoverability(),
                    failure.stage(),
                    failure.total_failures(),
                    failure.consecutive_failures(),
                )
            })
            .transpose()?;
        Self::new(
            ExtensionRuntimeIdentity::NativeAudio {
                provider,
                format: native.format(),
                vendor: native.vendor().to_owned(),
                identifier: native.identifier().to_owned(),
                version: native.version().to_owned(),
            },
            descriptor.name(),
            CapabilitySet::default(),
            CapabilitySet::default(),
            discovery,
            lifecycle,
            failure,
        )
    }

    #[must_use]
    pub const fn identity(&self) -> &ExtensionRuntimeIdentity {
        &self.identity
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub const fn requested_capabilities(&self) -> &CapabilitySet {
        &self.requested_capabilities
    }

    #[must_use]
    pub const fn granted_capabilities(&self) -> &CapabilitySet {
        &self.granted_capabilities
    }

    #[must_use]
    pub const fn discovery(&self) -> &FeatureDiscovery {
        &self.discovery
    }

    #[must_use]
    pub const fn lifecycle(&self) -> ExtensionLifecycle {
        self.lifecycle
    }

    #[must_use]
    pub const fn failure(&self) -> Option<&ExtensionFailureSummary> {
        self.failure.as_ref()
    }

    #[must_use]
    pub const fn control(&self) -> &ExtensionControlSurface {
        &self.control
    }

    fn validate(&self) -> Result<()> {
        if self.display_name.is_empty()
            || self.display_name.len() > MAX_DISPLAY_NAME_BYTES
            || self.display_name.chars().any(char::is_control)
        {
            return Err(extension_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_registration",
                "extension display name is empty, invalid, or exceeds its bound",
            ));
        }
        if self.requested_capabilities.len() > MAX_CAPABILITIES
            || self.granted_capabilities.len() > MAX_CAPABILITIES
            || self.discovery.len() > MAX_FEATURES
        {
            return Err(extension_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "validate_registration",
                "extension capability or feature count exceeds its bound",
            ));
        }
        if !self
            .requested_capabilities
            .contains_all(&self.granted_capabilities)
        {
            return Err(extension_error(
                ErrorCategory::PermissionDenied,
                Recoverability::UserCorrectable,
                "validate_registration",
                "extension grant contains a capability that was not requested",
            ));
        }
        if self.discovery.producer() != self.identity.producer()
            || self.discovery.capabilities() != &self.granted_capabilities
            || self.discovery.schema_version() != &EXTENSION_REGISTRY_SCHEMA_VERSION
        {
            return Err(extension_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "validate_registration",
                "extension discovery producer, capabilities, or schema do not match registration",
            ));
        }
        if self.lifecycle != ExtensionLifecycle::Ready
            && self
                .discovery
                .iter()
                .any(|feature| feature.availability() == FeatureAvailability::Available)
        {
            return Err(extension_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "validate_registration",
                "only a ready extension may advertise an available feature",
            ));
        }
        if matches!(
            self.lifecycle,
            ExtensionLifecycle::Faulted | ExtensionLifecycle::Quarantined
        ) && self.failure.is_none()
        {
            return Err(extension_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "validate_registration",
                "faulted or quarantined extension registration requires safe failure evidence",
            ));
        }
        Ok(())
    }
}

/// Immutable process-lifetime replacement state in canonical exact-identity order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ExtensionRegistrySnapshot {
    schema_version: SemanticVersion,
    revision: u64,
    registrations: Vec<ExtensionRegistration>,
}

impl ExtensionRegistrySnapshot {
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
}

/// Host-owned registry with bounded state and change-only revision increments.
#[derive(Clone, Debug, Default)]
pub struct ExtensionRegistry {
    revision: u64,
    registrations: BTreeMap<ExtensionIdentityKey, ExtensionRegistration>,
}

impl ExtensionRegistry {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            revision: 0,
            registrations: BTreeMap::new(),
        }
    }

    /// Registers one exact runtime identity. Duplicate exact identities are rejected.
    pub fn register(&mut self, registration: ExtensionRegistration) -> Result<()> {
        registration.validate()?;
        if self.registrations.len() >= MAX_REGISTRATIONS {
            return Err(extension_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "register",
                "extension registry reached its bounded capacity",
            ));
        }
        let key = registration.identity.key();
        if self.registrations.contains_key(&key) {
            return Err(extension_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "register",
                "extension runtime identity is already registered",
            ));
        }
        let revision = next_revision(self.revision)?;
        self.registrations.insert(key, registration);
        self.revision = revision;
        Ok(())
    }

    /// Replaces complete observed runtime state only when semantic content changed.
    pub fn synchronize(
        &mut self,
        registrations: impl IntoIterator<Item = ExtensionRegistration>,
    ) -> Result<bool> {
        let mut next = BTreeMap::new();
        for registration in registrations {
            registration.validate()?;
            if next.len() >= MAX_REGISTRATIONS {
                return Err(extension_error(
                    ErrorCategory::ResourceExhausted,
                    Recoverability::UserCorrectable,
                    "synchronize",
                    "extension registry replacement exceeds its bounded capacity",
                ));
            }
            let key = registration.identity.key();
            if next.insert(key, registration).is_some() {
                return Err(extension_error(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "synchronize",
                    "extension registry replacement contains a duplicate exact identity",
                ));
            }
        }
        if next == self.registrations {
            return Ok(false);
        }
        let revision = next_revision(self.revision)?;
        self.registrations = next;
        self.revision = revision;
        Ok(true)
    }

    /// Removes one process-lifetime registration without touching durable project state.
    pub fn unregister(&mut self, identity: &ExtensionRuntimeIdentity) -> Result<()> {
        let key = identity.key();
        if !self.registrations.contains_key(&key) {
            return Err(extension_error(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "unregister",
                "extension runtime identity is not registered",
            ));
        }
        let revision = next_revision(self.revision)?;
        self.registrations.remove(&key);
        self.revision = revision;
        Ok(())
    }

    /// Returns complete canonical replacement state.
    #[must_use]
    pub fn snapshot(&self) -> ExtensionRegistrySnapshot {
        ExtensionRegistrySnapshot {
            schema_version: EXTENSION_REGISTRY_SCHEMA_VERSION,
            revision: self.revision,
            registrations: self.registrations.values().cloned().collect(),
        }
    }
}

/// Reads every accepted OpenFX status without exposing a supervisor mutation handle.
pub fn registrations_from_openfx(
    supervisor: &PluginSupervisor,
) -> Result<Vec<ExtensionRegistration>> {
    supervisor
        .plugin_statuses()
        .iter()
        .map(ExtensionRegistration::from_openfx)
        .collect()
}

/// Reads every accepted native audio status without exposing a supervisor mutation handle.
pub fn registrations_from_native_audio(
    supervisor: &AudioPluginSupervisor,
) -> Result<Vec<ExtensionRegistration>> {
    supervisor
        .statuses()
        .iter()
        .map(ExtensionRegistration::from_native_audio)
        .collect()
}

fn public_ofx_lifecycle(value: OfxPluginLifecycle) -> ExtensionLifecycle {
    match value {
        OfxPluginLifecycle::Disabled => ExtensionLifecycle::Disabled,
        OfxPluginLifecycle::Ready => ExtensionLifecycle::Ready,
        OfxPluginLifecycle::Faulted => ExtensionLifecycle::Faulted,
        OfxPluginLifecycle::Quarantined => ExtensionLifecycle::Quarantined,
        _ => ExtensionLifecycle::Unavailable,
    }
}

fn public_audio_lifecycle(value: AudioPluginLifecycle) -> ExtensionLifecycle {
    match value {
        AudioPluginLifecycle::Disabled => ExtensionLifecycle::Disabled,
        AudioPluginLifecycle::Ready => ExtensionLifecycle::Ready,
        AudioPluginLifecycle::Faulted => ExtensionLifecycle::Faulted,
        AudioPluginLifecycle::Quarantined => ExtensionLifecycle::Quarantined,
    }
}

fn feature_availability(lifecycle: ExtensionLifecycle) -> FeatureAvailability {
    match lifecycle {
        ExtensionLifecycle::Ready => FeatureAvailability::Available,
        ExtensionLifecycle::Registered | ExtensionLifecycle::Disabled => {
            FeatureAvailability::Disabled
        }
        ExtensionLifecycle::Faulted
        | ExtensionLifecycle::Quarantined
        | ExtensionLifecycle::Unavailable => FeatureAvailability::Unavailable,
    }
}

fn next_revision(current: u64) -> Result<u64> {
    current.checked_add(1).ok_or_else(|| {
        extension_error(
            ErrorCategory::ResourceExhausted,
            Recoverability::Terminal,
            "advance_revision",
            "extension registry revision space is exhausted",
        )
    })
}

fn invalid_feature(message: &'static str) -> Error {
    extension_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "create_feature",
        message,
    )
}

fn extension_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
