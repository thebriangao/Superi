//! Stable public event vocabulary.

use serde::{Deserialize, Serialize};
use superi_core::settings::SemanticVersion;

use crate::api::{EngineIntrospectionSnapshot, MediaCapabilitiesSnapshot};
use crate::audio_automation::AudioAutomationSnapshot;
use crate::editor::{ProjectCommandEvidence, ProjectHistorySnapshot};
use crate::extensions::ExtensionRegistrySnapshot;
use crate::jobs::AsyncJobsSnapshot;
use crate::project::ProjectSettingsSnapshot;
use crate::recovery::ProjectRecoverySnapshot;
use crate::scenario::ScenarioStateSnapshot;
use crate::version::{
    ASYNC_JOBS_CHANGED_EVENT, ASYNC_JOBS_SCHEMA_VERSION, AUDIO_AUTOMATION_CHANGED_EVENT,
    AUDIO_AUTOMATION_SCHEMA_VERSION, ENGINE_INTROSPECTION_CHANGED_EVENT,
    ENGINE_INTROSPECTION_SCHEMA_VERSION, EXTENSIONS_CHANGED_EVENT, EXTENSIONS_SCHEMA_VERSION,
    MEDIA_CAPABILITIES_CHANGED_EVENT, MEDIA_CAPABILITIES_SCHEMA_VERSION,
    PROJECT_EDITOR_SCHEMA_VERSION, PROJECT_RECOVERY_CHANGED_EVENT, PROJECT_RECOVERY_SCHEMA_VERSION,
    PROJECT_SETTINGS_CHANGED_EVENT, PROJECT_SETTINGS_SCHEMA_VERSION, PROJECT_STATE_CHANGED_EVENT,
    SCENARIO_STATE_CHANGED_EVENT, SLICE_SCENARIO_SCHEMA_VERSION,
};

/// One typed event carried by the ordered public API event channel.
pub trait ApiEvent {
    /// Permanent namespaced event name.
    const NAME: &'static str;

    /// Version of the event payload contract.
    const SCHEMA_VERSION: SemanticVersion;
}

/// Full replacement generic project history state after one recorded command.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectStateChanged {
    sequence: u64,
    command_sequence: u64,
    transaction_id: String,
    project_revision: u64,
    command_log_sequence: u64,
    state: ProjectHistorySnapshot,
    evidence: ProjectCommandEvidence,
}

impl ProjectStateChanged {
    pub(crate) const fn new(
        sequence: u64,
        command_sequence: u64,
        transaction_id: String,
        project_revision: u64,
        command_log_sequence: u64,
        state: ProjectHistorySnapshot,
        evidence: ProjectCommandEvidence,
    ) -> Self {
        Self {
            sequence,
            command_sequence,
            transaction_id,
            project_revision,
            command_log_sequence,
            state,
            evidence,
        }
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    #[must_use]
    pub const fn command_log_sequence(&self) -> u64 {
        self.command_log_sequence
    }

    #[must_use]
    pub const fn state(&self) -> &ProjectHistorySnapshot {
        &self.state
    }

    #[must_use]
    pub const fn evidence(&self) -> &ProjectCommandEvidence {
        &self.evidence
    }
}

impl ApiEvent for ProjectStateChanged {
    const NAME: &'static str = PROJECT_STATE_CHANGED_EVENT;
    const SCHEMA_VERSION: SemanticVersion = PROJECT_EDITOR_SCHEMA_VERSION;
}

/// Complete asynchronous job replacement state after one observable transition.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AsyncJobsChanged {
    sequence: u64,
    command_sequence: u64,
    transaction_id: String,
    jobs_revision: u64,
    snapshot: AsyncJobsSnapshot,
}

impl AsyncJobsChanged {
    pub(crate) const fn new(
        sequence: u64,
        command_sequence: u64,
        transaction_id: String,
        jobs_revision: u64,
        snapshot: AsyncJobsSnapshot,
    ) -> Self {
        Self {
            sequence,
            command_sequence,
            transaction_id,
            jobs_revision,
            snapshot,
        }
    }

    /// Returns the monotonic engine event sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the successful engine command sequence.
    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    /// Returns the caller-owned transaction identity.
    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    /// Returns the engine-owned asynchronous job state revision.
    #[must_use]
    pub const fn jobs_revision(&self) -> u64 {
        self.jobs_revision
    }

    /// Returns complete retained replacement state.
    #[must_use]
    pub const fn snapshot(&self) -> &AsyncJobsSnapshot {
        &self.snapshot
    }
}

impl ApiEvent for AsyncJobsChanged {
    const NAME: &'static str = ASYNC_JOBS_CHANGED_EVENT;
    const SCHEMA_VERSION: SemanticVersion = ASYNC_JOBS_SCHEMA_VERSION;
}

/// Full replacement project recovery state after discovery, restore, or dismissal.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRecoveryChanged {
    sequence: u64,
    command_sequence: u64,
    transaction_id: String,
    project_revision: u64,
    catalog_revision: u64,
    snapshot: ProjectRecoverySnapshot,
}

impl ProjectRecoveryChanged {
    pub(crate) const fn new(
        sequence: u64,
        command_sequence: u64,
        transaction_id: String,
        project_revision: u64,
        catalog_revision: u64,
        snapshot: ProjectRecoverySnapshot,
    ) -> Self {
        Self {
            sequence,
            command_sequence,
            transaction_id,
            project_revision,
            catalog_revision,
            snapshot,
        }
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    #[must_use]
    pub const fn catalog_revision(&self) -> u64 {
        self.catalog_revision
    }

    #[must_use]
    pub const fn snapshot(&self) -> &ProjectRecoverySnapshot {
        &self.snapshot
    }
}

impl ApiEvent for ProjectRecoveryChanged {
    const NAME: &'static str = PROJECT_RECOVERY_CHANGED_EVENT;
    const SCHEMA_VERSION: SemanticVersion = PROJECT_RECOVERY_SCHEMA_VERSION;
}

/// Full replacement authored automation state emitted after one committed transaction.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioAutomationChanged {
    sequence: u64,
    command_sequence: u64,
    transaction_id: String,
    audio_automation_revision: u64,
    snapshot: AudioAutomationSnapshot,
}

impl AudioAutomationChanged {
    pub(crate) const fn new(
        sequence: u64,
        command_sequence: u64,
        transaction_id: String,
        audio_automation_revision: u64,
        snapshot: AudioAutomationSnapshot,
    ) -> Self {
        Self {
            sequence,
            command_sequence,
            transaction_id,
            audio_automation_revision,
            snapshot,
        }
    }

    /// Returns the monotonic engine event sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the successful engine command sequence.
    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    /// Returns the caller-owned transaction identity.
    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    /// Returns the resulting authored automation revision.
    #[must_use]
    pub const fn audio_automation_revision(&self) -> u64 {
        self.audio_automation_revision
    }

    /// Returns complete replacement authored automation state.
    #[must_use]
    pub const fn snapshot(&self) -> &AudioAutomationSnapshot {
        &self.snapshot
    }
}

impl ApiEvent for AudioAutomationChanged {
    const NAME: &'static str = AUDIO_AUTOMATION_CHANGED_EVENT;
    const SCHEMA_VERSION: SemanticVersion = AUDIO_AUTOMATION_SCHEMA_VERSION;
}

/// Full replacement project settings state emitted after one committed transaction.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSettingsChanged {
    sequence: u64,
    command_sequence: u64,
    transaction_id: String,
    project_revision: u64,
    snapshot: ProjectSettingsSnapshot,
}

impl ProjectSettingsChanged {
    pub(crate) const fn new(
        sequence: u64,
        command_sequence: u64,
        transaction_id: String,
        project_revision: u64,
        snapshot: ProjectSettingsSnapshot,
    ) -> Self {
        Self {
            sequence,
            command_sequence,
            transaction_id,
            project_revision,
            snapshot,
        }
    }

    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    #[must_use]
    pub const fn snapshot(&self) -> &ProjectSettingsSnapshot {
        &self.snapshot
    }
}

impl ApiEvent for ProjectSettingsChanged {
    const NAME: &'static str = PROJECT_SETTINGS_CHANGED_EVENT;
    const SCHEMA_VERSION: SemanticVersion = PROJECT_SETTINGS_SCHEMA_VERSION;
}

/// Full replacement state emitted when media capabilities change.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaCapabilitiesChanged {
    snapshot: MediaCapabilitiesSnapshot,
}

impl MediaCapabilitiesChanged {
    pub(crate) const fn new(snapshot: MediaCapabilitiesSnapshot) -> Self {
        Self { snapshot }
    }

    /// Returns the new complete capability state.
    #[must_use]
    pub const fn snapshot(&self) -> &MediaCapabilitiesSnapshot {
        &self.snapshot
    }
}

impl ApiEvent for MediaCapabilitiesChanged {
    const NAME: &'static str = MEDIA_CAPABILITIES_CHANGED_EVENT;
    const SCHEMA_VERSION: SemanticVersion = MEDIA_CAPABILITIES_SCHEMA_VERSION;
}

/// Full replacement state emitted when engine capability or health state changes.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineIntrospectionChanged {
    snapshot: EngineIntrospectionSnapshot,
}

impl EngineIntrospectionChanged {
    pub(crate) const fn new(snapshot: EngineIntrospectionSnapshot) -> Self {
        Self { snapshot }
    }

    /// Returns the new complete engine capability and health state.
    #[must_use]
    pub const fn snapshot(&self) -> &EngineIntrospectionSnapshot {
        &self.snapshot
    }
}

impl ApiEvent for EngineIntrospectionChanged {
    const NAME: &'static str = ENGINE_INTROSPECTION_CHANGED_EVENT;
    const SCHEMA_VERSION: SemanticVersion = ENGINE_INTROSPECTION_SCHEMA_VERSION;
}

/// Complete extension registration and capability state after a runtime change.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtensionsChanged {
    snapshot: ExtensionRegistrySnapshot,
}

impl ExtensionsChanged {
    pub(crate) const fn new(snapshot: ExtensionRegistrySnapshot) -> Self {
        Self { snapshot }
    }

    /// Returns complete process-lifetime extension replacement state.
    #[must_use]
    pub const fn snapshot(&self) -> &ExtensionRegistrySnapshot {
        &self.snapshot
    }
}

impl ApiEvent for ExtensionsChanged {
    const NAME: &'static str = EXTENSIONS_CHANGED_EVENT;
    const SCHEMA_VERSION: SemanticVersion = EXTENSIONS_SCHEMA_VERSION;
}

/// Full replacement scenario state emitted after one committed transaction.
#[cfg_attr(feature = "typescript-bindings", derive(specta::Type))]
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScenarioStateChanged {
    sequence: u64,
    command_sequence: u64,
    transaction_id: String,
    project_revision: u64,
    state: ScenarioStateSnapshot,
}

impl ScenarioStateChanged {
    pub(crate) fn new(
        sequence: u64,
        command_sequence: u64,
        transaction_id: String,
        project_revision: u64,
        state: ScenarioStateSnapshot,
    ) -> Self {
        Self {
            sequence,
            command_sequence,
            transaction_id,
            project_revision,
            state,
        }
    }

    /// Returns the monotonic event sequence from the scenario dispatcher.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the successful command sequence that produced this event.
    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    /// Returns the transaction that committed this state.
    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    /// Returns the resulting scenario revision.
    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    /// Returns the complete replacement state for deterministic resynchronization.
    #[must_use]
    pub const fn state(&self) -> &ScenarioStateSnapshot {
        &self.state
    }
}

impl ApiEvent for ScenarioStateChanged {
    const NAME: &'static str = SCENARIO_STATE_CHANGED_EVENT;
    const SCHEMA_VERSION: SemanticVersion = SLICE_SCENARIO_SCHEMA_VERSION;
}
