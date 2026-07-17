//! Strict transport-neutral project crash recovery API.

use serde::{Deserialize, Serialize};
use superi_core::diagnostics::UserSafeError;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::SemanticVersion;
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult, EngineEvent,
    EngineTransactionId,
};
use superi_engine::project_recovery::{
    ProjectRecoveryCandidateId as EngineCandidateId,
    ProjectRecoveryCommand as EngineRecoveryCommand, ProjectRecoveryComparison as EngineComparison,
    ProjectRecoveryState as EngineRecoveryState,
};

use crate::commands::{
    CompareProjectRecovery, CompareProjectRecoveryResult, DismissProjectRecovery,
    DismissProjectRecoveryResult, GetProjectRecovery, GetProjectRecoveryResult,
    RestoreProjectRecovery, RestoreProjectRecoveryResult,
};
use crate::events::ProjectRecoveryChanged;
use crate::version::PROJECT_RECOVERY_SCHEMA_VERSION;

const COMPONENT: &str = "superi-api.project-recovery";

/// Strict public recovery candidate without any filesystem identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRecoveryCandidateSnapshot {
    candidate_id: String,
    captured_project_revision: u64,
}

impl ProjectRecoveryCandidateSnapshot {
    #[must_use]
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    #[must_use]
    pub const fn captured_project_revision(&self) -> u64 {
        self.captured_project_revision
    }
}

/// Strict user-safe projection of one internal recovery finding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRecoveryFindingSnapshot {
    candidate_id: String,
    category: String,
    recoverability: String,
    code: String,
    title: String,
    action: String,
    next_action: String,
}

impl ProjectRecoveryFindingSnapshot {
    #[must_use]
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    #[must_use]
    pub fn category(&self) -> &str {
        &self.category
    }

    #[must_use]
    pub fn recoverability(&self) -> &str {
        &self.recoverability
    }

    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn action(&self) -> &str {
        &self.action
    }

    #[must_use]
    pub fn next_action(&self) -> &str {
        &self.next_action
    }
}

/// Complete strict public replacement state for project crash recovery.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRecoverySnapshot {
    schema_version: SemanticVersion,
    project_id: String,
    project_revision: u64,
    catalog_revision: u64,
    candidates: Vec<ProjectRecoveryCandidateSnapshot>,
    findings: Vec<ProjectRecoveryFindingSnapshot>,
}

impl ProjectRecoverySnapshot {
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
    pub const fn catalog_revision(&self) -> u64 {
        self.catalog_revision
    }

    #[must_use]
    pub fn candidates(&self) -> &[ProjectRecoveryCandidateSnapshot] {
        &self.candidates
    }

    #[must_use]
    pub fn findings(&self) -> &[ProjectRecoveryFindingSnapshot] {
        &self.findings
    }
}

/// Strict public semantic comparison for one recovery candidate.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectRecoveryComparisonSnapshot {
    candidate_id: String,
    current_revision: u64,
    candidate_revision: u64,
    editorial_changed: bool,
    settings_changed: bool,
    clip_mix_changed: bool,
    root_timeline_changed: bool,
    changed_graph_count: usize,
    current_graph_count: usize,
    candidate_graph_count: usize,
}

impl ProjectRecoveryComparisonSnapshot {
    #[must_use]
    pub fn candidate_id(&self) -> &str {
        &self.candidate_id
    }

    #[must_use]
    pub const fn current_revision(&self) -> u64 {
        self.current_revision
    }

    #[must_use]
    pub const fn candidate_revision(&self) -> u64 {
        self.candidate_revision
    }

    #[must_use]
    pub const fn editorial_changed(&self) -> bool {
        self.editorial_changed
    }

    #[must_use]
    pub const fn settings_changed(&self) -> bool {
        self.settings_changed
    }

    #[must_use]
    pub const fn clip_mix_changed(&self) -> bool {
        self.clip_mix_changed
    }

    #[must_use]
    pub const fn root_timeline_changed(&self) -> bool {
        self.root_timeline_changed
    }

    #[must_use]
    pub const fn changed_graph_count(&self) -> usize {
        self.changed_graph_count
    }

    #[must_use]
    pub const fn current_graph_count(&self) -> usize {
        self.current_graph_count
    }

    #[must_use]
    pub const fn candidate_graph_count(&self) -> usize {
        self.candidate_graph_count
    }

    #[must_use]
    pub const fn has_changes(&self) -> bool {
        self.editorial_changed
            || self.settings_changed
            || self.clip_mix_changed
            || self.root_timeline_changed
            || self.changed_graph_count > 0
    }
}

/// Mutable public facade around one recovery-attached engine dispatcher.
pub struct ProjectRecoveryApi {
    dispatcher: EngineCommandDispatcher,
}

impl ProjectRecoveryApi {
    /// Takes ownership of a full dispatcher with one attached project.
    pub fn new(dispatcher: EngineCommandDispatcher) -> Result<Self> {
        let _ = dispatcher.project_snapshot()?;
        Ok(Self { dispatcher })
    }

    /// Refreshes and returns complete recovery state.
    pub fn execute(&mut self, command: GetProjectRecovery) -> Result<GetProjectRecoveryResult> {
        let transaction_id = EngineTransactionId::new(command.into_transaction_id())?;
        let outcome = self.dispatcher.dispatch(EngineCommandRequest::new(
            transaction_id,
            EngineCommand::ExecuteProjectRecovery(EngineRecoveryCommand::discover()),
        ))?;
        let EngineCommandResult::ProjectRecovery(recovery) = outcome.result() else {
            return Err(unexpected_result("get_project_recovery"));
        };
        Ok(GetProjectRecoveryResult::new(
            outcome.transaction_id().as_str().to_owned(),
            outcome.command_sequence(),
            public_snapshot(recovery.state()),
        ))
    }

    /// Compares one exact candidate with current project state.
    pub fn compare(
        &mut self,
        command: CompareProjectRecovery,
    ) -> Result<CompareProjectRecoveryResult> {
        let (transaction_id, catalog_revision, candidate_id) = command.into_parts();
        let candidate_id = parse_candidate_id(&candidate_id)?;
        let outcome = self.dispatcher.dispatch(EngineCommandRequest::new(
            EngineTransactionId::new(transaction_id)?,
            EngineCommand::ExecuteProjectRecovery(EngineRecoveryCommand::compare(
                catalog_revision,
                candidate_id,
            )),
        ))?;
        let EngineCommandResult::ProjectRecovery(recovery) = outcome.result() else {
            return Err(unexpected_result("compare_project_recovery"));
        };
        let comparison = recovery
            .comparison()
            .ok_or_else(|| unexpected_result("compare_project_recovery_evidence"))?;
        Ok(CompareProjectRecoveryResult::new(
            outcome.transaction_id().as_str().to_owned(),
            outcome.command_sequence(),
            public_snapshot(recovery.state()),
            public_comparison(comparison),
        ))
    }

    /// Restores one exact candidate through the durable engine transaction.
    pub fn restore(
        &mut self,
        command: RestoreProjectRecovery,
    ) -> Result<RestoreProjectRecoveryResult> {
        let (transaction_id, catalog_revision, project_revision, candidate_id) =
            command.into_parts();
        let candidate_id = parse_candidate_id(&candidate_id)?;
        let outcome = self.dispatcher.dispatch(EngineCommandRequest::new(
            EngineTransactionId::new(transaction_id)?,
            EngineCommand::ExecuteProjectRecovery(EngineRecoveryCommand::restore(
                catalog_revision,
                project_revision,
                candidate_id,
            )),
        ))?;
        let EngineCommandResult::ProjectRecovery(recovery) = outcome.result() else {
            return Err(unexpected_result("restore_project_recovery"));
        };
        Ok(RestoreProjectRecoveryResult::new(
            outcome.transaction_id().as_str().to_owned(),
            outcome.command_sequence(),
            recovery.restored(),
            public_snapshot(recovery.state()),
        ))
    }

    /// Durably dismisses one exact managed recovery entry.
    pub fn dismiss(
        &mut self,
        command: DismissProjectRecovery,
    ) -> Result<DismissProjectRecoveryResult> {
        let (transaction_id, catalog_revision, candidate_id) = command.into_parts();
        let candidate_id = parse_candidate_id(&candidate_id)?;
        let outcome = self.dispatcher.dispatch(EngineCommandRequest::new(
            EngineTransactionId::new(transaction_id)?,
            EngineCommand::ExecuteProjectRecovery(EngineRecoveryCommand::dismiss(
                catalog_revision,
                candidate_id,
            )),
        ))?;
        let EngineCommandResult::ProjectRecovery(recovery) = outcome.result() else {
            return Err(unexpected_result("dismiss_project_recovery"));
        };
        Ok(DismissProjectRecoveryResult::new(
            outcome.transaction_id().as_str().to_owned(),
            outcome.command_sequence(),
            recovery.dismissed(),
            public_snapshot(recovery.state()),
        ))
    }

    /// Drains ordered complete recovery replacement events.
    pub fn drain_events(&mut self) -> Result<Vec<ProjectRecoveryChanged>> {
        self.dispatcher
            .drain_events()?
            .into_iter()
            .map(|envelope| match envelope.event() {
                EngineEvent::ProjectRecoveryStateChanged(state) => {
                    let snapshot = public_snapshot(state);
                    Ok(ProjectRecoveryChanged::new(
                        envelope.sequence(),
                        envelope.command_sequence(),
                        envelope.transaction_id().as_str().to_owned(),
                        snapshot.project_revision(),
                        snapshot.catalog_revision(),
                        snapshot,
                    ))
                }
                _ => Err(Error::new(
                    ErrorCategory::Internal,
                    Recoverability::Terminal,
                    "project recovery API received an unrelated engine event",
                )
                .with_context(ErrorContext::new(COMPONENT, "drain_events"))),
            })
            .collect()
    }
}

impl std::fmt::Debug for ProjectRecoveryApi {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProjectRecoveryApi")
            .finish_non_exhaustive()
    }
}

fn public_snapshot(state: &EngineRecoveryState) -> ProjectRecoverySnapshot {
    ProjectRecoverySnapshot {
        schema_version: PROJECT_RECOVERY_SCHEMA_VERSION.clone(),
        project_id: state.project_id().to_string(),
        project_revision: state.project_revision(),
        catalog_revision: state.catalog().revision(),
        candidates: state
            .catalog()
            .candidates()
            .iter()
            .map(|candidate| ProjectRecoveryCandidateSnapshot {
                candidate_id: candidate.id().to_string(),
                captured_project_revision: candidate.project_revision(),
            })
            .collect(),
        findings: state
            .catalog()
            .findings()
            .iter()
            .map(|finding| {
                let diagnostic = finding.diagnostic();
                let safe_source = Error::new(
                    diagnostic.category(),
                    diagnostic.recoverability(),
                    "project recovery candidate needs attention",
                );
                let safe = UserSafeError::from_error(&safe_source);
                ProjectRecoveryFindingSnapshot {
                    candidate_id: finding.candidate_id().to_string(),
                    category: safe.category().code().to_owned(),
                    recoverability: safe.recoverability().code().to_owned(),
                    code: safe.code().to_owned(),
                    title: safe.title().to_owned(),
                    action: safe.action().to_owned(),
                    next_action: finding.next_action().code().to_owned(),
                }
            })
            .collect(),
    }
}

fn public_comparison(value: &EngineComparison) -> ProjectRecoveryComparisonSnapshot {
    ProjectRecoveryComparisonSnapshot {
        candidate_id: value.candidate_id().to_string(),
        current_revision: value.current_revision(),
        candidate_revision: value.candidate_revision(),
        editorial_changed: value.editorial_changed(),
        settings_changed: value.settings_changed(),
        clip_mix_changed: value.clip_mix_changed(),
        root_timeline_changed: value.root_timeline_changed(),
        changed_graph_count: value.changed_graph_count(),
        current_graph_count: value.current_graph_count(),
        candidate_graph_count: value.candidate_graph_count(),
    }
}

fn parse_candidate_id(value: &str) -> Result<EngineCandidateId> {
    let digits = value
        .strip_prefix("recovery-g")
        .ok_or_else(invalid_candidate_id)?;
    if digits.len() != 20 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(invalid_candidate_id());
    }
    let generation = digits.parse::<u64>().map_err(|_| invalid_candidate_id())?;
    let candidate_id = EngineCandidateId::new(generation)?;
    if candidate_id.to_string() != value {
        return Err(invalid_candidate_id());
    }
    Ok(candidate_id)
}

fn invalid_candidate_id() -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "project recovery candidate identity is invalid",
    )
    .with_context(ErrorContext::new(COMPONENT, "parse_candidate_identity"))
}

fn unexpected_result(operation: &'static str) -> Error {
    Error::new(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "project recovery dispatcher returned an unrelated result",
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
