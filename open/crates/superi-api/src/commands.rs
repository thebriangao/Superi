//! Stable public command vocabulary.

use serde::{Deserialize, Serialize};

use crate::api::{EngineIntrospectionSnapshot, MediaCapabilitiesSnapshot};
use crate::scenario::{ScenarioActionResult, ScenarioTransactionResult, SliceAction};
use crate::version::{
    EXECUTE_SCENARIO_ACTION_METHOD, EXECUTE_SCENARIO_TRANSACTION_METHOD,
    GET_ENGINE_INTROSPECTION_METHOD, GET_MEDIA_CAPABILITIES_METHOD,
};

/// One typed public API command and its response contract.
pub trait ApiCommand {
    /// Successful response returned by this command.
    type Response;

    /// Permanent namespaced JSON-RPC method name.
    const METHOD: &'static str;
}

/// Structured parameters for a media capability query.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetMediaCapabilities {}

impl GetMediaCapabilities {
    /// Creates an unfiltered media capability query.
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }
}

impl ApiCommand for GetMediaCapabilities {
    type Response = GetMediaCapabilitiesResult;

    const METHOD: &'static str = GET_MEDIA_CAPABILITIES_METHOD;
}

/// Successful response to [`GetMediaCapabilities`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetMediaCapabilitiesResult {
    snapshot: MediaCapabilitiesSnapshot,
}

impl GetMediaCapabilitiesResult {
    pub(crate) const fn new(snapshot: MediaCapabilitiesSnapshot) -> Self {
        Self { snapshot }
    }

    /// Returns the complete point-in-time capability state.
    #[must_use]
    pub const fn snapshot(&self) -> &MediaCapabilitiesSnapshot {
        &self.snapshot
    }
}

/// Structured parameters for a complete engine introspection query.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetEngineIntrospection {}

impl GetEngineIntrospection {
    /// Creates an unfiltered engine introspection query.
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }
}

impl ApiCommand for GetEngineIntrospection {
    type Response = GetEngineIntrospectionResult;

    const METHOD: &'static str = GET_ENGINE_INTROSPECTION_METHOD;
}

/// Successful response to [`GetEngineIntrospection`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetEngineIntrospectionResult {
    snapshot: EngineIntrospectionSnapshot,
}

impl GetEngineIntrospectionResult {
    pub(crate) const fn new(snapshot: EngineIntrospectionSnapshot) -> Self {
        Self { snapshot }
    }

    /// Returns the complete point-in-time capability and health state.
    #[must_use]
    pub const fn snapshot(&self) -> &EngineIntrospectionSnapshot {
        &self.snapshot
    }
}

/// Typed request to execute one precise reference slice action.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteScenarioAction {
    action: SliceAction,
}

impl ExecuteScenarioAction {
    /// Creates an action request.
    #[must_use]
    pub const fn new(action: SliceAction) -> Self {
        Self { action }
    }

    /// Returns the requested action.
    #[must_use]
    pub const fn action(&self) -> &SliceAction {
        &self.action
    }

    pub(crate) fn into_action(self) -> SliceAction {
        self.action
    }
}

impl ApiCommand for ExecuteScenarioAction {
    type Response = ScenarioActionResult;

    const METHOD: &'static str = EXECUTE_SCENARIO_ACTION_METHOD;
}

/// Typed request to commit one optimistic ordered scenario transaction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExecuteScenarioTransaction {
    transaction_id: String,
    expected_revision: u64,
    actions: Vec<SliceAction>,
}

impl ExecuteScenarioTransaction {
    /// Creates one public automation transaction.
    #[must_use]
    pub fn new(
        transaction_id: impl Into<String>,
        expected_revision: u64,
        actions: Vec<SliceAction>,
    ) -> Self {
        Self {
            transaction_id: transaction_id.into(),
            expected_revision,
            actions,
        }
    }

    /// Returns the exact caller-owned transaction identity.
    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    /// Returns the exact scenario revision required before execution.
    #[must_use]
    pub const fn expected_revision(&self) -> u64 {
        self.expected_revision
    }

    /// Returns actions in deterministic execution order.
    #[must_use]
    pub fn actions(&self) -> &[SliceAction] {
        &self.actions
    }

    pub(crate) fn into_parts(self) -> (String, u64, Vec<SliceAction>) {
        (self.transaction_id, self.expected_revision, self.actions)
    }
}

impl ApiCommand for ExecuteScenarioTransaction {
    type Response = ScenarioTransactionResult;

    const METHOD: &'static str = EXECUTE_SCENARIO_TRANSACTION_METHOD;
}
