//! Stable transport-neutral project settings command adapter.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::{SemanticVersion, SettingValue};
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult, EngineEvent,
    EngineTransactionId,
};
use superi_engine::project_settings::ProjectSettingsState as EngineProjectSettingsState;
use superi_engine::project_settings::{
    ProjectSettingMutation as EngineSettingMutation,
    ProjectSettingsTransaction as EngineSettingsTransaction,
};

use crate::commands::{
    ExecuteProjectSettingsTransaction, ExecuteProjectSettingsTransactionResult, GetProjectSettings,
    GetProjectSettingsResult,
};
use crate::events::ProjectSettingsChanged;
use crate::version::PROJECT_SETTINGS_SCHEMA_VERSION;

const COMPONENT: &str = "superi-api.project-settings";

/// One strict shared value carried by public project settings JSON.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProjectSettingValue {
    Boolean(bool),
    Integer(i64),
    Text(String),
}

impl ProjectSettingValue {
    fn into_engine(self) -> SettingValue {
        match self {
            Self::Boolean(value) => SettingValue::Boolean(value),
            Self::Integer(value) => SettingValue::Integer(value),
            Self::Text(value) => SettingValue::Text(value),
        }
    }

    fn from_engine(value: &SettingValue) -> Self {
        match value {
            SettingValue::Boolean(value) => Self::Boolean(*value),
            SettingValue::Integer(value) => Self::Integer(*value),
            SettingValue::Text(value) => Self::Text(value.clone()),
            _ => unreachable!("public API and shared settings define the same value kinds"),
        }
    }
}

/// One strict ordered mutation carried by a public transaction.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case", deny_unknown_fields)]
#[non_exhaustive]
pub enum ProjectSettingMutation {
    Set {
        key: String,
        value: ProjectSettingValue,
    },
    Remove {
        key: String,
    },
}

impl ProjectSettingMutation {
    fn into_engine(self) -> Result<EngineSettingMutation> {
        match self {
            Self::Set { key, value } => EngineSettingMutation::set(key, value.into_engine()),
            Self::Remove { key } => EngineSettingMutation::remove(key),
        }
    }
}

/// Complete strict public replacement snapshot for project settings.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectSettingsSnapshot {
    schema_version: SemanticVersion,
    project_id: String,
    project_revision: u64,
    values: BTreeMap<String, ProjectSettingValue>,
}

impl ProjectSettingsSnapshot {
    #[must_use]
    pub const fn schema_version(&self) -> &SemanticVersion {
        &self.schema_version
    }

    #[must_use]
    pub fn project_id(&self) -> &str {
        &self.project_id
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    #[must_use]
    pub const fn values(&self) -> &BTreeMap<String, ProjectSettingValue> {
        &self.values
    }

    #[must_use]
    pub fn value(&self, key: &str) -> Option<&ProjectSettingValue> {
        self.values.get(key)
    }
}

/// Mutable public facade around one caller-supplied full engine dispatcher.
pub struct ProjectSettingsApi {
    dispatcher: EngineCommandDispatcher,
}

impl ProjectSettingsApi {
    /// Takes ownership of a dispatcher with one attached project.
    pub fn new(dispatcher: EngineCommandDispatcher) -> Result<Self> {
        let _ = dispatcher.project_snapshot()?;
        Ok(Self { dispatcher })
    }

    /// Executes the stable project settings query through the engine dispatcher.
    pub fn execute(&mut self, _command: GetProjectSettings) -> Result<GetProjectSettingsResult> {
        let outcome = self.dispatcher.dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("project-settings-get")?,
            EngineCommand::InspectProjectSettings,
        ))?;
        let EngineCommandResult::ProjectSettings(state) = outcome.result() else {
            return Err(unexpected_result("get_project_settings"));
        };
        Ok(GetProjectSettingsResult::new(public_snapshot(state)))
    }

    /// Executes one strict optimistic settings transaction through the engine owner.
    pub fn execute_transaction(
        &mut self,
        command: ExecuteProjectSettingsTransaction,
    ) -> Result<ExecuteProjectSettingsTransactionResult> {
        let (transaction_id, expected_revision, mutations) = command.into_parts();
        let mutations = mutations
            .into_iter()
            .map(ProjectSettingMutation::into_engine)
            .collect::<Result<Vec<_>>>()?;
        let transaction = EngineSettingsTransaction::new(expected_revision, mutations)?;
        let transaction_id = EngineTransactionId::new(transaction_id)?;
        let outcome = self.dispatcher.dispatch(EngineCommandRequest::new(
            transaction_id,
            EngineCommand::ExecuteProjectSettings(transaction),
        ))?;
        let EngineCommandResult::ProjectSettings(state) = outcome.result() else {
            return Err(unexpected_result("execute_project_settings_transaction"));
        };
        Ok(ExecuteProjectSettingsTransactionResult::new(
            outcome.transaction_id().as_str().to_owned(),
            outcome.command_sequence(),
            public_snapshot(state),
        ))
    }

    /// Drains ordered full replacement project settings events.
    pub fn drain_events(&mut self) -> Result<Vec<ProjectSettingsChanged>> {
        self.dispatcher
            .drain_events()?
            .into_iter()
            .map(|envelope| match envelope.event() {
                EngineEvent::ProjectSettingsChanged(state) => Ok(ProjectSettingsChanged::new(
                    envelope.sequence(),
                    envelope.command_sequence(),
                    envelope.transaction_id().as_str().to_owned(),
                    state.project_revision(),
                    public_snapshot(state),
                )),
                _ => Err(Error::new(
                    ErrorCategory::Internal,
                    Recoverability::Terminal,
                    "project settings API received an unrelated engine event",
                )
                .with_context(ErrorContext::new(COMPONENT, "drain_events"))),
            })
            .collect()
    }
}

impl std::fmt::Debug for ProjectSettingsApi {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectSettingsApi")
            .finish_non_exhaustive()
    }
}

fn public_snapshot(state: &EngineProjectSettingsState) -> ProjectSettingsSnapshot {
    ProjectSettingsSnapshot {
        schema_version: PROJECT_SETTINGS_SCHEMA_VERSION.clone(),
        project_id: state.project_id().to_string(),
        project_revision: state.project_revision(),
        values: state
            .settings()
            .iter()
            .map(|(key, value)| {
                (
                    key.as_str().to_owned(),
                    ProjectSettingValue::from_engine(value),
                )
            })
            .collect(),
    }
}

fn unexpected_result(operation: &'static str) -> Error {
    Error::new(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "project settings dispatcher returned an unrelated result",
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
