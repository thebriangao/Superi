//! Durable project-owned state for plugins, effects, AI artifacts, and future extensions.
//!
//! Extension payloads are intentionally opaque to the project crate. The typed envelope owns
//! identity, schema, capability grants, lifecycle, and structured failure state while preserving
//! every payload byte for extensions that this build does not understand.

use std::fmt;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::{CapabilitySet, ComponentId, SemanticVersion, VersionIdentifier};

use crate::document::{ProjectDocument, ProjectDraft, ProjectSnapshot};

const COMPONENT: &str = "superi-project.extensions";
const PLUGIN_KIND: &str = "superi.extension.plugin";
const EFFECT_KIND: &str = "superi.extension.effect";
const AI_ARTIFACT_KIND: &str = "superi.extension.ai-artifact";

/// Maximum number of durable extension records accepted by one project.
pub const MAX_PROJECT_EXTENSION_RECORDS: usize = 4_096;
/// Maximum opaque payload size accepted for one extension record.
pub const MAX_PROJECT_EXTENSION_PAYLOAD_BYTES: usize = 64 * 1024 * 1024;
/// Maximum UTF-8 byte length of a record identity.
pub const MAX_PROJECT_EXTENSION_RECORD_ID_BYTES: usize = 128;
/// Maximum number of declared capabilities in either capability set.
pub const MAX_PROJECT_EXTENSION_CAPABILITIES: usize = 1_024;
/// Maximum number of structured context frames retained for one failure.
pub const MAX_PROJECT_EXTENSION_FAILURE_CONTEXTS: usize = 16;
/// Maximum UTF-8 byte length of a retained failure message.
pub const MAX_PROJECT_EXTENSION_FAILURE_MESSAGE_BYTES: usize = 1_024;

/// Stable extension-owned identity for one durable state record.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectExtensionRecordId(String);

impl ProjectExtensionRecordId {
    /// Validates a nonempty bounded identifier without changing its bytes.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        if value.is_empty() || value.len() > MAX_PROJECT_EXTENSION_RECORD_ID_BYTES {
            return Err(extension_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_record_id",
                "extension record identity is empty or exceeds the supported bound",
            ));
        }
        if value.chars().any(char::is_control) {
            return Err(extension_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_record_id",
                "extension record identity contains a control character",
            ));
        }
        Ok(Self(value))
    }

    /// Returns the retained identity text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for ProjectExtensionRecordId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Stable compound identity for one extension record.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectExtensionKey {
    extension_id: ComponentId,
    record_id: ProjectExtensionRecordId,
}

impl ProjectExtensionKey {
    /// Creates a compound identity from canonical extension and record identities.
    #[must_use]
    pub const fn new(extension_id: ComponentId, record_id: ProjectExtensionRecordId) -> Self {
        Self {
            extension_id,
            record_id,
        }
    }

    /// Returns the extension that owns this state.
    #[must_use]
    pub const fn extension_id(&self) -> &ComponentId {
        &self.extension_id
    }

    /// Returns the extension-local record identity.
    #[must_use]
    pub const fn record_id(&self) -> &ProjectExtensionRecordId {
        &self.record_id
    }
}

/// Open, namespaced classification for one extension record.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ProjectExtensionKind(ComponentId);

impl ProjectExtensionKind {
    /// Preserves any canonical current or future extension kind.
    #[must_use]
    pub const fn new(kind: ComponentId) -> Self {
        Self(kind)
    }

    /// Returns the canonical plugin-state classification.
    #[must_use]
    pub fn plugin() -> Self {
        Self(ComponentId::new(PLUGIN_KIND).expect("built-in plugin kind is canonical"))
    }

    /// Returns the canonical classification for auxiliary extension-owned effect state.
    ///
    /// Authored graph node parameters remain graph-owned and are not duplicated here.
    #[must_use]
    pub fn effect() -> Self {
        Self(ComponentId::new(EFFECT_KIND).expect("built-in effect kind is canonical"))
    }

    /// Returns the canonical AI artifact metadata classification.
    ///
    /// Generated output remains an ordinary editable artifact; this record holds only
    /// supplementary provenance or lifecycle metadata owned by the extension.
    #[must_use]
    pub fn ai_artifact() -> Self {
        Self(ComponentId::new(AI_ARTIFACT_KIND).expect("built-in AI artifact kind is canonical"))
    }

    /// Returns the retained canonical kind text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }

    /// Returns the retained canonical identifier.
    #[must_use]
    pub const fn component_id(&self) -> &ComponentId {
        &self.0
    }
}

/// User-controlled durable lifecycle for one project extension record.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ProjectExtensionLifecycle {
    /// The owning extension may use this state when its runtime is available.
    Enabled,
    /// The user disabled the owning extension state without deleting it.
    Disabled,
    /// Repeated or severe failures prevent activation until the user intervenes.
    Quarantined,
}

impl ProjectExtensionLifecycle {
    /// Returns the permanent persistence and public-surface code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
            Self::Quarantined => "quarantined",
        }
    }

    /// Decodes one permanent lifecycle code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "enabled" => Some(Self::Enabled),
            "disabled" => Some(Self::Disabled),
            "quarantined" => Some(Self::Quarantined),
            _ => None,
        }
    }
}

/// Durable structured failure state for one extension record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectExtensionFailure {
    category: ErrorCategory,
    recoverability: Recoverability,
    message: String,
    contexts: Vec<ErrorContext>,
    total_failures: u64,
    consecutive_failures: u32,
}

impl ProjectExtensionFailure {
    /// Creates bounded, actionable failure evidence.
    pub fn new(
        category: ErrorCategory,
        recoverability: Recoverability,
        message: impl Into<String>,
        contexts: impl IntoIterator<Item = ErrorContext>,
        total_failures: u64,
        consecutive_failures: u32,
    ) -> Result<Self> {
        let failure = Self {
            category,
            recoverability,
            message: message.into(),
            contexts: contexts.into_iter().collect(),
            total_failures,
            consecutive_failures,
        };
        failure.validate()?;
        Ok(failure)
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        self.category
    }

    /// Returns the advertised recovery classification.
    #[must_use]
    pub const fn recoverability(&self) -> Recoverability {
        self.recoverability
    }

    /// Returns the bounded user-facing diagnostic message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns context frames in retained order.
    #[must_use]
    pub fn contexts(&self) -> &[ErrorContext] {
        &self.contexts
    }

    /// Returns the lifetime project failure count observed for this record.
    #[must_use]
    pub const fn total_failures(&self) -> u64 {
        self.total_failures
    }

    /// Returns the current consecutive failure count.
    #[must_use]
    pub const fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    fn validate(&self) -> Result<()> {
        if self.message.is_empty()
            || self.message.len() > MAX_PROJECT_EXTENSION_FAILURE_MESSAGE_BYTES
        {
            return Err(extension_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_failure",
                "extension failure message is empty or exceeds the supported bound",
            ));
        }
        if self.contexts.len() > MAX_PROJECT_EXTENSION_FAILURE_CONTEXTS {
            return Err(extension_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "validate_failure",
                "extension failure context count exceeds the supported bound",
            ));
        }
        if self.total_failures == 0
            || self.consecutive_failures == 0
            || u64::from(self.consecutive_failures) > self.total_failures
        {
            return Err(extension_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_failure",
                "extension failure counters are inconsistent",
            ));
        }
        for context in &self.contexts {
            if context.component().is_empty()
                || context.operation().is_empty()
                || context.component().len() > 256
                || context.operation().len() > 256
                || context.fields().len() > 64
                || context
                    .fields()
                    .iter()
                    .any(|(key, value)| key.is_empty() || key.len() > 256 || value.len() > 4_096)
            {
                return Err(extension_error(
                    ErrorCategory::ResourceExhausted,
                    Recoverability::UserCorrectable,
                    "validate_failure",
                    "extension failure context exceeds the supported bound",
                ));
            }
        }
        Ok(())
    }
}

/// Complete durable envelope for opaque extension-owned project state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectExtensionRecord {
    key: ProjectExtensionKey,
    extension_version: SemanticVersion,
    kind: ProjectExtensionKind,
    payload_schema: VersionIdentifier,
    requested_capabilities: CapabilitySet,
    granted_capabilities: CapabilitySet,
    lifecycle: ProjectExtensionLifecycle,
    failure: Option<ProjectExtensionFailure>,
    payload: Vec<u8>,
}

impl ProjectExtensionRecord {
    /// Creates and validates a durable typed envelope around opaque payload bytes.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        extension_id: ComponentId,
        record_id: ProjectExtensionRecordId,
        extension_version: SemanticVersion,
        kind: ProjectExtensionKind,
        payload_schema: VersionIdentifier,
        requested_capabilities: CapabilitySet,
        granted_capabilities: CapabilitySet,
        lifecycle: ProjectExtensionLifecycle,
        failure: Option<ProjectExtensionFailure>,
        payload: Vec<u8>,
    ) -> Result<Self> {
        let record = Self {
            key: ProjectExtensionKey::new(extension_id, record_id),
            extension_version,
            kind,
            payload_schema,
            requested_capabilities,
            granted_capabilities,
            lifecycle,
            failure,
            payload,
        };
        record.validate()?;
        Ok(record)
    }

    /// Returns the stable compound identity.
    #[must_use]
    pub const fn key(&self) -> &ProjectExtensionKey {
        &self.key
    }

    /// Returns the owner extension's schema-aware version.
    #[must_use]
    pub const fn extension_version(&self) -> &SemanticVersion {
        &self.extension_version
    }

    /// Returns the open classification retained for this record.
    #[must_use]
    pub const fn kind(&self) -> &ProjectExtensionKind {
        &self.kind
    }

    /// Returns the declared payload schema.
    #[must_use]
    pub const fn payload_schema(&self) -> &VersionIdentifier {
        &self.payload_schema
    }

    /// Returns all capabilities requested by the owning extension.
    #[must_use]
    pub const fn requested_capabilities(&self) -> &CapabilitySet {
        &self.requested_capabilities
    }

    /// Returns the user-approved subset available to the owning extension.
    #[must_use]
    pub const fn granted_capabilities(&self) -> &CapabilitySet {
        &self.granted_capabilities
    }

    /// Returns user-controlled durable lifecycle state.
    #[must_use]
    pub const fn lifecycle(&self) -> ProjectExtensionLifecycle {
        self.lifecycle
    }

    /// Returns retained structured failure evidence, when present.
    #[must_use]
    pub const fn failure(&self) -> Option<&ProjectExtensionFailure> {
        self.failure.as_ref()
    }

    /// Returns opaque payload bytes without parsing or reinterpretation.
    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    fn validate(&self) -> Result<()> {
        if self.payload.len() > MAX_PROJECT_EXTENSION_PAYLOAD_BYTES {
            return Err(extension_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "validate_record",
                "extension payload exceeds the supported bound",
            ));
        }
        if self.requested_capabilities.len() > MAX_PROJECT_EXTENSION_CAPABILITIES
            || self.granted_capabilities.len() > MAX_PROJECT_EXTENSION_CAPABILITIES
        {
            return Err(extension_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "validate_record",
                "extension capability count exceeds the supported bound",
            ));
        }
        if !self
            .requested_capabilities
            .contains_all(&self.granted_capabilities)
        {
            return Err(extension_error(
                ErrorCategory::PermissionDenied,
                Recoverability::UserCorrectable,
                "validate_record",
                "extension grant contains a capability that was not requested",
            ));
        }
        if let Some(failure) = &self.failure {
            failure.validate()?;
        }
        if self.lifecycle == ProjectExtensionLifecycle::Quarantined && self.failure.is_none() {
            return Err(extension_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "validate_record",
                "quarantined extension state requires retained failure evidence",
            ));
        }
        Ok(())
    }
}

/// Stable operation classification for the one public extension command surface.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum ProjectExtensionOperation {
    /// Create or replace one complete state envelope.
    Upsert,
    /// Remove one state envelope.
    Remove,
    /// Change user-controlled lifecycle state.
    SetLifecycle,
    /// Replace the approved capability subset.
    SetGrantedCapabilities,
    /// Retain structured failure evidence, optionally quarantining the record.
    RecordFailure,
    /// Clear retained failure evidence after quarantine is lifted.
    ClearFailure,
}

impl ProjectExtensionOperation {
    /// Returns the permanent public command code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Upsert => "upsert",
            Self::Remove => "remove",
            Self::SetLifecycle => "set_lifecycle",
            Self::SetGrantedCapabilities => "set_granted_capabilities",
            Self::RecordFailure => "record_failure",
            Self::ClearFailure => "clear_failure",
        }
    }
}

/// One extension-state mutation accepted by project transactions and history.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectExtensionCommand {
    /// Create or replace one complete state envelope.
    Upsert(Box<ProjectExtensionRecord>),
    /// Remove one state envelope.
    Remove(ProjectExtensionKey),
    /// Change user-controlled lifecycle state.
    SetLifecycle {
        /// Addressed state record.
        key: ProjectExtensionKey,
        /// New durable lifecycle.
        lifecycle: ProjectExtensionLifecycle,
    },
    /// Replace the approved capability subset.
    SetGrantedCapabilities {
        /// Addressed state record.
        key: ProjectExtensionKey,
        /// New user-approved subset.
        capabilities: CapabilitySet,
    },
    /// Retain failure evidence and optionally quarantine the record.
    RecordFailure {
        /// Addressed state record.
        key: ProjectExtensionKey,
        /// Complete durable failure evidence.
        failure: ProjectExtensionFailure,
        /// Whether this failure immediately quarantines the record.
        quarantine: bool,
    },
    /// Clear retained failure evidence after quarantine is lifted.
    ClearFailure(ProjectExtensionKey),
}

impl ProjectExtensionCommand {
    /// Creates an upsert command.
    #[must_use]
    pub fn upsert(record: ProjectExtensionRecord) -> Self {
        Self::Upsert(Box::new(record))
    }

    /// Creates a remove command.
    #[must_use]
    pub const fn remove(key: ProjectExtensionKey) -> Self {
        Self::Remove(key)
    }

    /// Creates a lifecycle command.
    #[must_use]
    pub const fn set_lifecycle(
        key: ProjectExtensionKey,
        lifecycle: ProjectExtensionLifecycle,
    ) -> Self {
        Self::SetLifecycle { key, lifecycle }
    }

    /// Creates a capability-grant command.
    #[must_use]
    pub const fn set_granted_capabilities(
        key: ProjectExtensionKey,
        capabilities: CapabilitySet,
    ) -> Self {
        Self::SetGrantedCapabilities { key, capabilities }
    }

    /// Creates a failure command.
    #[must_use]
    pub const fn record_failure(
        key: ProjectExtensionKey,
        failure: ProjectExtensionFailure,
        quarantine: bool,
    ) -> Self {
        Self::RecordFailure {
            key,
            failure,
            quarantine,
        }
    }

    /// Creates a failure-clear command.
    #[must_use]
    pub const fn clear_failure(key: ProjectExtensionKey) -> Self {
        Self::ClearFailure(key)
    }

    /// Returns the stable addressed record identity.
    #[must_use]
    pub const fn key(&self) -> &ProjectExtensionKey {
        match self {
            Self::Upsert(record) => record.key(),
            Self::Remove(key)
            | Self::SetLifecycle { key, .. }
            | Self::SetGrantedCapabilities { key, .. }
            | Self::RecordFailure { key, .. }
            | Self::ClearFailure(key) => key,
        }
    }

    /// Returns the stable semantic operation.
    #[must_use]
    pub const fn operation(&self) -> ProjectExtensionOperation {
        match self {
            Self::Upsert(_) => ProjectExtensionOperation::Upsert,
            Self::Remove(_) => ProjectExtensionOperation::Remove,
            Self::SetLifecycle { .. } => ProjectExtensionOperation::SetLifecycle,
            Self::SetGrantedCapabilities { .. } => {
                ProjectExtensionOperation::SetGrantedCapabilities
            }
            Self::RecordFailure { .. } => ProjectExtensionOperation::RecordFailure,
            Self::ClearFailure(_) => ProjectExtensionOperation::ClearFailure,
        }
    }
}

/// Typed semantic result of one successful extension command.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectExtensionCommandResult {
    /// A complete state envelope was created or replaced.
    Upserted {
        /// Addressed record identity.
        key: ProjectExtensionKey,
        /// Whether a record with this identity already existed.
        replaced: bool,
    },
    /// One state envelope was removed.
    Removed(ProjectExtensionKey),
    /// Lifecycle state was accepted.
    LifecycleSet(ProjectExtensionKey),
    /// Capability grants were accepted.
    GrantedCapabilitiesSet(ProjectExtensionKey),
    /// Failure evidence was retained.
    FailureRecorded(ProjectExtensionKey),
    /// Failure evidence was cleared.
    FailureCleared(ProjectExtensionKey),
}

/// One extension command result paired with the exact published project snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectExtensionCommandOutcome {
    snapshot: ProjectSnapshot,
    result: ProjectExtensionCommandResult,
}

impl ProjectExtensionCommandOutcome {
    /// Returns the immutable project snapshot published by the command.
    #[must_use]
    pub const fn snapshot(&self) -> &ProjectSnapshot {
        &self.snapshot
    }

    /// Returns the semantic command result.
    #[must_use]
    pub const fn result(&self) -> &ProjectExtensionCommandResult {
        &self.result
    }

    /// Consumes the outcome and returns its published snapshot.
    #[must_use]
    pub fn into_snapshot(self) -> ProjectSnapshot {
        self.snapshot
    }
}

impl ProjectDocument {
    /// Applies one extension-state command through the whole-project revision fence.
    pub fn execute_extension_command(
        &mut self,
        expected_revision: u64,
        command: ProjectExtensionCommand,
    ) -> Result<ProjectExtensionCommandOutcome> {
        self.check_revision("execute_extension_command", expected_revision)?;
        let mut result = None;
        let snapshot = self.edit(expected_revision, |draft| {
            result = Some(draft.execute_extension_command(&command)?);
            Ok(())
        })?;
        let result = result.ok_or_else(|| {
            extension_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "execute_extension_command",
                "published extension command did not retain its semantic result",
            )
        })?;
        Ok(ProjectExtensionCommandOutcome { snapshot, result })
    }
}

impl ProjectDraft<'_> {
    /// Applies one extension-state command inside an existing whole-project transaction.
    pub fn execute_extension_command(
        &mut self,
        command: &ProjectExtensionCommand,
    ) -> Result<ProjectExtensionCommandResult> {
        let key = command.key().clone();
        match command {
            ProjectExtensionCommand::Upsert(record) => {
                record.validate()?;
                let replaced = self.extension_records().contains_key(&key);
                if !replaced && self.extension_records().len() >= MAX_PROJECT_EXTENSION_RECORDS {
                    return Err(extension_error(
                        ErrorCategory::ResourceExhausted,
                        Recoverability::UserCorrectable,
                        "upsert_extension_record",
                        "project extension record count exceeds the supported bound",
                    ));
                }
                self.extension_records_mut()
                    .insert(key.clone(), record.as_ref().clone());
                Ok(ProjectExtensionCommandResult::Upserted { key, replaced })
            }
            ProjectExtensionCommand::Remove(_) => {
                self.extension_records_mut()
                    .remove(&key)
                    .ok_or_else(|| missing_record(&key, "remove_extension_record"))?;
                Ok(ProjectExtensionCommandResult::Removed(key))
            }
            ProjectExtensionCommand::SetLifecycle { lifecycle, .. } => {
                let record = self
                    .extension_records_mut()
                    .get_mut(&key)
                    .ok_or_else(|| missing_record(&key, "set_extension_lifecycle"))?;
                if *lifecycle == ProjectExtensionLifecycle::Quarantined && record.failure.is_none()
                {
                    return Err(extension_error(
                        ErrorCategory::Conflict,
                        Recoverability::UserCorrectable,
                        "set_extension_lifecycle",
                        "quarantining extension state requires retained failure evidence",
                    ));
                }
                record.lifecycle = *lifecycle;
                Ok(ProjectExtensionCommandResult::LifecycleSet(key))
            }
            ProjectExtensionCommand::SetGrantedCapabilities { capabilities, .. } => {
                let record = self
                    .extension_records_mut()
                    .get_mut(&key)
                    .ok_or_else(|| missing_record(&key, "set_extension_capabilities"))?;
                if capabilities.len() > MAX_PROJECT_EXTENSION_CAPABILITIES {
                    return Err(extension_error(
                        ErrorCategory::ResourceExhausted,
                        Recoverability::UserCorrectable,
                        "set_extension_capabilities",
                        "extension capability count exceeds the supported bound",
                    ));
                }
                if !record.requested_capabilities.contains_all(capabilities) {
                    return Err(extension_error(
                        ErrorCategory::PermissionDenied,
                        Recoverability::UserCorrectable,
                        "set_extension_capabilities",
                        "extension grant contains a capability that was not requested",
                    ));
                }
                record.granted_capabilities = capabilities.clone();
                Ok(ProjectExtensionCommandResult::GrantedCapabilitiesSet(key))
            }
            ProjectExtensionCommand::RecordFailure {
                failure,
                quarantine,
                ..
            } => {
                failure.validate()?;
                let record = self
                    .extension_records_mut()
                    .get_mut(&key)
                    .ok_or_else(|| missing_record(&key, "record_extension_failure"))?;
                record.failure = Some(failure.clone());
                if *quarantine {
                    record.lifecycle = ProjectExtensionLifecycle::Quarantined;
                }
                Ok(ProjectExtensionCommandResult::FailureRecorded(key))
            }
            ProjectExtensionCommand::ClearFailure(_) => {
                let record = self
                    .extension_records_mut()
                    .get_mut(&key)
                    .ok_or_else(|| missing_record(&key, "clear_extension_failure"))?;
                if record.lifecycle == ProjectExtensionLifecycle::Quarantined {
                    return Err(extension_error(
                        ErrorCategory::Conflict,
                        Recoverability::UserCorrectable,
                        "clear_extension_failure",
                        "quarantine must be lifted before failure evidence is cleared",
                    ));
                }
                record.failure = None;
                Ok(ProjectExtensionCommandResult::FailureCleared(key))
            }
        }
    }
}

fn missing_record(key: &ProjectExtensionKey, operation: &'static str) -> Error {
    extension_error(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        operation,
        "extension record was not found in the project",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("extension_id", key.extension_id().to_string())
            .with_field("record_id", key.record_id().to_string()),
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
