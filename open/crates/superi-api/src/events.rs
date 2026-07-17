//! Stable public event vocabulary.

use serde::{Deserialize, Serialize};

use crate::api::{EngineIntrospectionSnapshot, MediaCapabilitiesSnapshot};
use crate::project::ProjectSettingsSnapshot;
use crate::recovery::ProjectRecoverySnapshot;
use crate::scenario::ScenarioStateSnapshot;
use crate::version::{
    ENGINE_INTROSPECTION_CHANGED_EVENT, MEDIA_CAPABILITIES_CHANGED_EVENT,
    PROJECT_RECOVERY_CHANGED_EVENT, PROJECT_SETTINGS_CHANGED_EVENT, SCENARIO_STATE_CHANGED_EVENT,
};

/// One typed event carried by the ordered public API event channel.
pub trait ApiEvent {
    /// Permanent namespaced event name.
    const NAME: &'static str;
}

/// Full replacement project recovery state after discovery, restore, or dismissal.
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
}

/// Full replacement project settings state emitted after one committed transaction.
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
}

/// Full replacement state emitted when media capabilities change.
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
}

/// Full replacement state emitted when engine capability or health state changes.
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
}

/// Full replacement scenario state emitted after one committed transaction.
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
}
