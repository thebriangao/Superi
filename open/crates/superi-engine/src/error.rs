//! Cross-subsystem error propagation and recovery.
//!
//! [`EngineErrorCoordinator`] is owned by EngineControl and routes classified subsystem failures
//! through the canonical [`EngineLifecycle`]. It retains bounded structured diagnostics and exact
//! recovery intent without copying playback, rendering, export, or lifecycle state.

use std::collections::{BTreeMap, VecDeque};
use std::fmt;

use superi_concurrency::shared::{DomainOwned, SharedSnapshot};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::diagnostics::{DiagnosticEvent, DiagnosticSeverity, TraceField};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::lifecycle::{
    EngineLifecycle, EngineLifecycleAction, EngineLifecycleActionKind, EngineLifecycleSnapshot,
    EngineSubsystem,
};

const COMPONENT: &str = "superi-engine.error";
const FAILURE_EVENT: &str = "engine.subsystem.failed";
const RECOVERY_EVENT: &str = "engine.subsystem.recovered";
const MAX_OPERATION_BYTES: usize = 128;

/// Default number of cross-subsystem diagnostic events retained by EngineControl.
pub const DEFAULT_ERROR_HISTORY_CAPACITY: usize = 64;

/// Maximum diagnostic history capacity accepted by the coordinator.
pub const MAX_ERROR_HISTORY_CAPACITY: usize = 4096;

/// Monotonic identity for one propagated engine failure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct EngineFailureSequence(u64);

impl EngineFailureSequence {
    /// Returns the stable integer value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for EngineFailureSequence {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Stable engine-level response derived from explicit core recoverability.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum EngineFailureDisposition {
    /// The same operation may be attempted again without user intervention.
    RetryAvailable,
    /// Unrelated workflows may continue while the unavailable capability is restored.
    ContinueDegraded,
    /// Input, permissions, configuration, or project state must be corrected first.
    UserCorrectionRequired,
    /// The current engine lifetime cannot continue safely.
    RestartRequired,
}

impl EngineFailureDisposition {
    /// Returns the stable diagnostic and automation code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::RetryAvailable => "retry_available",
            Self::ContinueDegraded => "continue_degraded",
            Self::UserCorrectionRequired => "user_correction_required",
            Self::RestartRequired => "restart_required",
        }
    }

    fn from_recoverability(recoverability: Recoverability) -> Self {
        match recoverability {
            Recoverability::Retryable => Self::RetryAvailable,
            Recoverability::Degraded => Self::ContinueDegraded,
            Recoverability::UserCorrectable => Self::UserCorrectionRequired,
            Recoverability::Terminal => Self::RestartRequired,
            _ => Self::ContinueDegraded,
        }
    }

    const fn expected_request(self) -> Option<EngineRecoveryRequest> {
        match self {
            Self::RetryAvailable => Some(EngineRecoveryRequest::RetryOperation),
            Self::ContinueDegraded => Some(EngineRecoveryRequest::RestoreSubsystem),
            Self::UserCorrectionRequired => Some(EngineRecoveryRequest::ApplyUserCorrection),
            Self::RestartRequired => None,
        }
    }
}

impl fmt::Display for EngineFailureDisposition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// Explicit recovery intent authorized for one failure disposition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum EngineRecoveryRequest {
    /// Retry the same operation after a transient failure.
    RetryOperation,
    /// Revalidate or reacquire a degraded subsystem capability.
    RestoreSubsystem,
    /// Confirm that the user-correctable condition was addressed before revalidation.
    ApplyUserCorrection,
}

impl EngineRecoveryRequest {
    /// Returns the stable diagnostic and automation code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::RetryOperation => "retry_operation",
            Self::RestoreSubsystem => "restore_subsystem",
            Self::ApplyUserCorrection => "apply_user_correction",
        }
    }
}

impl fmt::Display for EngineRecoveryRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// Immutable actionable evidence for one active subsystem failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineFailureRecord {
    sequence: EngineFailureSequence,
    subsystem: EngineSubsystem,
    operation: String,
    recovery_attempts: u32,
    disposition: EngineFailureDisposition,
    engine_lifetime: u64,
    state_revision: u64,
    diagnostic: DiagnosticEvent,
}

impl EngineFailureRecord {
    /// Returns the monotonic failure identity.
    #[must_use]
    pub const fn sequence(&self) -> EngineFailureSequence {
        self.sequence
    }

    /// Returns the subsystem that reported the failure.
    #[must_use]
    pub const fn subsystem(&self) -> EngineSubsystem {
        self.subsystem
    }

    /// Returns the bounded operation label supplied by the subsystem owner.
    #[must_use]
    pub fn operation(&self) -> &str {
        &self.operation
    }

    /// Returns the number of recovery actions already issued for this operation.
    #[must_use]
    pub const fn recovery_attempts(&self) -> u32 {
        self.recovery_attempts
    }

    /// Returns the engine-level response derived from the error classification.
    #[must_use]
    pub const fn disposition(&self) -> EngineFailureDisposition {
        self.disposition
    }

    /// Returns the engine lifetime in which the failure was published.
    #[must_use]
    pub const fn engine_lifetime(&self) -> u64 {
        self.engine_lifetime
    }

    /// Returns the lifecycle state revision that contains the failure.
    #[must_use]
    pub const fn state_revision(&self) -> u64 {
        self.state_revision
    }

    /// Returns the structured internal and user-safe diagnostic projections.
    #[must_use]
    pub const fn diagnostic(&self) -> &DiagnosticEvent {
        &self.diagnostic
    }
}

/// One propagated failure paired with the coherent lifecycle snapshot that contains it.
#[derive(Clone, Debug)]
pub struct EngineFailureReport {
    failure: EngineFailureRecord,
    lifecycle: SharedSnapshot<EngineLifecycleSnapshot>,
}

impl EngineFailureReport {
    /// Returns the immutable failure evidence.
    #[must_use]
    pub const fn failure(&self) -> &EngineFailureRecord {
        &self.failure
    }

    /// Returns the coherent workflow-admission state after propagation.
    #[must_use]
    pub const fn lifecycle(&self) -> &SharedSnapshot<EngineLifecycleSnapshot> {
        &self.lifecycle
    }
}

/// Exact token for one classified subsystem recovery attempt.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EngineRecoveryAction {
    sequence: EngineFailureSequence,
    subsystem: EngineSubsystem,
    request: EngineRecoveryRequest,
    attempt: u32,
    lifecycle_action: EngineLifecycleAction,
}

impl EngineRecoveryAction {
    /// Returns the failure identity this action addresses.
    #[must_use]
    pub const fn sequence(self) -> EngineFailureSequence {
        self.sequence
    }

    /// Returns the subsystem that must execute the recovery work.
    #[must_use]
    pub const fn subsystem(self) -> EngineSubsystem {
        self.subsystem
    }

    /// Returns the validated recovery intent.
    #[must_use]
    pub const fn request(self) -> EngineRecoveryRequest {
        self.request
    }

    /// Returns the monotonic attempt number for the original operation.
    #[must_use]
    pub const fn attempt(self) -> u32 {
        self.attempt
    }
}

/// One issued recovery action paired with the recovering lifecycle snapshot.
#[derive(Clone, Debug)]
pub struct EngineRecoveryStart {
    action: EngineRecoveryAction,
    lifecycle: SharedSnapshot<EngineLifecycleSnapshot>,
}

impl EngineRecoveryStart {
    /// Returns the exact token the subsystem owner must complete or fail.
    #[must_use]
    pub const fn action(&self) -> EngineRecoveryAction {
        self.action
    }

    /// Returns the coherent workflow-admission state while recovery runs.
    #[must_use]
    pub const fn lifecycle(&self) -> &SharedSnapshot<EngineLifecycleSnapshot> {
        &self.lifecycle
    }
}

/// Evidence that one exact recovery action restored its subsystem.
#[derive(Clone, Debug)]
pub struct EngineRecoveryReport {
    sequence: EngineFailureSequence,
    subsystem: EngineSubsystem,
    request: EngineRecoveryRequest,
    attempt: u32,
    diagnostic: DiagnosticEvent,
    lifecycle: SharedSnapshot<EngineLifecycleSnapshot>,
}

impl EngineRecoveryReport {
    /// Returns the failure identity that was resolved.
    #[must_use]
    pub const fn sequence(&self) -> EngineFailureSequence {
        self.sequence
    }

    /// Returns the restored subsystem.
    #[must_use]
    pub const fn subsystem(&self) -> EngineSubsystem {
        self.subsystem
    }

    /// Returns the recovery intent that succeeded.
    #[must_use]
    pub const fn request(&self) -> EngineRecoveryRequest {
        self.request
    }

    /// Returns the completed recovery attempt number.
    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }

    /// Returns the structured recovery event.
    #[must_use]
    pub const fn diagnostic(&self) -> &DiagnosticEvent {
        &self.diagnostic
    }

    /// Returns the coherent workflow-admission state after recovery.
    #[must_use]
    pub const fn lifecycle(&self) -> &SharedSnapshot<EngineLifecycleSnapshot> {
        &self.lifecycle
    }
}

/// EngineControl-owned cross-subsystem failure and recovery coordinator.
///
/// Mutable bookkeeping stays behind [`DomainOwned`] and is intentionally not `Sync`. Immutable
/// records and returned lifecycle snapshots are safe to pass to dispatchers and observers.
///
/// ```compile_fail
/// use superi_engine::error::EngineErrorCoordinator;
///
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<EngineErrorCoordinator>();
/// ```
#[derive(Debug)]
pub struct EngineErrorCoordinator {
    owned: DomainOwned<EngineErrorState>,
}

impl EngineErrorCoordinator {
    /// Creates a coordinator with the default bounded diagnostic history.
    pub fn new() -> Result<Self> {
        Self::with_history_capacity(DEFAULT_ERROR_HISTORY_CAPACITY)
    }

    /// Creates a coordinator with an explicit bounded diagnostic history.
    pub fn with_history_capacity(history_capacity: usize) -> Result<Self> {
        ExecutionDomain::EngineControl.require_current()?;
        if !(1..=MAX_ERROR_HISTORY_CAPACITY).contains(&history_capacity) {
            return Err(coordinator_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "engine diagnostic history capacity is outside the supported range",
                "create_coordinator",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "history_capacity")
                    .with_field("capacity", history_capacity.to_string())
                    .with_field("minimum", "1")
                    .with_field("maximum", MAX_ERROR_HISTORY_CAPACITY.to_string()),
            ));
        }
        Ok(Self {
            owned: DomainOwned::new(
                ExecutionDomain::EngineControl,
                EngineErrorState::new(history_capacity),
            ),
        })
    }

    /// Returns a copy of the active failure for one subsystem.
    pub fn active_failure(
        &self,
        subsystem: EngineSubsystem,
    ) -> Result<Option<EngineFailureRecord>> {
        self.owned
            .with(|state| state.active.get(&subsystem).cloned())
    }

    /// Returns retained events from oldest to newest.
    pub fn diagnostic_history(&self) -> Result<Vec<DiagnosticEvent>> {
        self.owned
            .with(|state| state.history.iter().cloned().collect())
    }

    /// Propagates one ready subsystem's classified runtime failure into the shared lifecycle.
    pub fn report_failure(
        &mut self,
        lifecycle: &mut EngineLifecycle,
        subsystem: EngineSubsystem,
        operation: impl Into<String>,
        mut error: Error,
    ) -> Result<EngineFailureReport> {
        let operation = validate_operation(operation.into())?;
        self.owned.with_mut(|state| {
            if let Some(active) = state.active.get(&subsystem) {
                return Err(active_failure_conflict(active, "report_failure"));
            }
            let sequence = state.next_sequence()?;
            let disposition = EngineFailureDisposition::from_recoverability(error.recoverability());
            error.push_context(failure_context(
                "propagate_failure",
                subsystem,
                &operation,
                sequence,
                0,
            ));
            let diagnostic =
                failure_event(&error, subsystem, &operation, sequence, 0, disposition)?;
            let lifecycle = lifecycle.report_runtime_failure(subsystem, error)?;
            let failure = EngineFailureRecord {
                sequence,
                subsystem,
                operation,
                recovery_attempts: 0,
                disposition,
                engine_lifetime: lifecycle.lifetime(),
                state_revision: lifecycle.state_revision(),
                diagnostic: diagnostic.clone(),
            };
            state.last_sequence = sequence.get();
            let replaced = state.active.insert(subsystem, failure.clone());
            debug_assert!(replaced.is_none());
            state.push_history(diagnostic);
            Ok(EngineFailureReport { failure, lifecycle })
        })?
    }

    /// Validates recovery intent and emits the exact lifecycle action for the subsystem owner.
    pub fn begin_recovery(
        &mut self,
        lifecycle: &mut EngineLifecycle,
        subsystem: EngineSubsystem,
        sequence: EngineFailureSequence,
        request: EngineRecoveryRequest,
    ) -> Result<EngineRecoveryStart> {
        self.owned.with_mut(|state| {
            let active = state
                .active_failure(subsystem, sequence, "begin_recovery")?
                .clone();
            if active.disposition == EngineFailureDisposition::RestartRequired {
                return Err(coordinator_error(
                    ErrorCategory::Conflict,
                    Recoverability::Terminal,
                    "terminal engine failure requires a new engine lifetime",
                    "begin_recovery",
                )
                .with_context(recovery_context(
                    subsystem,
                    sequence,
                    active.recovery_attempts,
                    request,
                )));
            }
            let expected = active
                .disposition
                .expected_request()
                .expect("nonterminal failure has recovery intent");
            if request != expected {
                return Err(coordinator_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "recovery request does not match the active failure classification",
                    "begin_recovery",
                )
                .with_context(
                    recovery_context(subsystem, sequence, active.recovery_attempts, request)
                        .with_field("expected_request", expected.code())
                        .with_field("disposition", active.disposition.code()),
                ));
            }
            let attempt = active.recovery_attempts.checked_add(1).ok_or_else(|| {
                coordinator_error(
                    ErrorCategory::ResourceExhausted,
                    Recoverability::Terminal,
                    "engine recovery attempt counter is exhausted",
                    "begin_recovery",
                )
                .with_context(recovery_context(
                    subsystem,
                    sequence,
                    active.recovery_attempts,
                    request,
                ))
            })?;

            let snapshot = lifecycle.begin_recovery(subsystem)?;
            let lifecycle_action = snapshot.pending_action().ok_or_else(|| {
                coordinator_error(
                    ErrorCategory::Internal,
                    Recoverability::Terminal,
                    "engine lifecycle did not publish the requested recovery action",
                    "begin_recovery",
                )
            })?;
            if lifecycle_action.subsystem() != subsystem
                || lifecycle_action.kind() != EngineLifecycleActionKind::Recover
            {
                return Err(coordinator_error(
                    ErrorCategory::Internal,
                    Recoverability::Terminal,
                    "engine lifecycle published a mismatched recovery action",
                    "begin_recovery",
                )
                .with_context(
                    recovery_context(subsystem, sequence, attempt, request)
                        .with_field("actual_subsystem", lifecycle_action.subsystem().code())
                        .with_field("actual_action", lifecycle_action.kind().code()),
                ));
            }
            state
                .active
                .get_mut(&subsystem)
                .expect("validated active failure remains present")
                .recovery_attempts = attempt;
            Ok(EngineRecoveryStart {
                action: EngineRecoveryAction {
                    sequence,
                    subsystem,
                    request,
                    attempt,
                    lifecycle_action,
                },
                lifecycle: snapshot,
            })
        })?
    }

    /// Completes one exact recovery action and clears its active failure.
    pub fn complete_recovery(
        &mut self,
        lifecycle: &mut EngineLifecycle,
        action: EngineRecoveryAction,
    ) -> Result<EngineRecoveryReport> {
        self.owned.with_mut(|state| {
            let active = state
                .validate_recovery_action(action, "complete_recovery")?
                .clone();
            let diagnostic = recovery_event(&active, action)?;
            let snapshot = lifecycle.complete_action(action.lifecycle_action)?;
            let removed = state.active.remove(&action.subsystem);
            debug_assert!(removed.is_some());
            state.push_history(diagnostic.clone());
            Ok(EngineRecoveryReport {
                sequence: action.sequence,
                subsystem: action.subsystem,
                request: action.request,
                attempt: action.attempt,
                diagnostic,
                lifecycle: snapshot,
            })
        })?
    }

    /// Propagates a failed exact recovery action as a newly classified active failure.
    pub fn fail_recovery(
        &mut self,
        lifecycle: &mut EngineLifecycle,
        action: EngineRecoveryAction,
        mut error: Error,
    ) -> Result<EngineFailureReport> {
        self.owned.with_mut(|state| {
            let active = state
                .validate_recovery_action(action, "fail_recovery")?
                .clone();
            let sequence = state.next_sequence()?;
            let disposition = EngineFailureDisposition::from_recoverability(error.recoverability());
            error.push_context(
                failure_context(
                    "fail_recovery",
                    action.subsystem,
                    &active.operation,
                    sequence,
                    action.attempt,
                )
                .with_field("previous_failure_sequence", action.sequence.to_string())
                .with_field("recovery_request", action.request.code()),
            );
            let diagnostic = failure_event(
                &error,
                action.subsystem,
                &active.operation,
                sequence,
                action.attempt,
                disposition,
            )?;
            let snapshot = lifecycle.fail_action(action.lifecycle_action, error)?;
            let failure = EngineFailureRecord {
                sequence,
                subsystem: action.subsystem,
                operation: active.operation,
                recovery_attempts: action.attempt,
                disposition,
                engine_lifetime: snapshot.lifetime(),
                state_revision: snapshot.state_revision(),
                diagnostic: diagnostic.clone(),
            };
            state.last_sequence = sequence.get();
            let replaced = state.active.insert(action.subsystem, failure.clone());
            debug_assert!(replaced.is_some());
            state.push_history(diagnostic);
            Ok(EngineFailureReport {
                failure,
                lifecycle: snapshot,
            })
        })?
    }
}

struct EngineErrorState {
    last_sequence: u64,
    active: BTreeMap<EngineSubsystem, EngineFailureRecord>,
    history: VecDeque<DiagnosticEvent>,
    history_capacity: usize,
}

impl EngineErrorState {
    fn new(history_capacity: usize) -> Self {
        Self {
            last_sequence: 0,
            active: BTreeMap::new(),
            history: VecDeque::with_capacity(history_capacity),
            history_capacity,
        }
    }

    fn next_sequence(&self) -> Result<EngineFailureSequence> {
        self.last_sequence
            .checked_add(1)
            .map(EngineFailureSequence)
            .ok_or_else(|| {
                coordinator_error(
                    ErrorCategory::ResourceExhausted,
                    Recoverability::Terminal,
                    "engine failure sequence is exhausted",
                    "allocate_failure_sequence",
                )
            })
    }

    fn active_failure(
        &self,
        subsystem: EngineSubsystem,
        sequence: EngineFailureSequence,
        operation: &'static str,
    ) -> Result<&EngineFailureRecord> {
        let Some(active) = self.active.get(&subsystem) else {
            return Err(stale_failure_error(subsystem, sequence, None, operation));
        };
        if active.sequence != sequence {
            return Err(stale_failure_error(
                subsystem,
                sequence,
                Some(active.sequence),
                operation,
            ));
        }
        Ok(active)
    }

    fn validate_recovery_action(
        &self,
        action: EngineRecoveryAction,
        operation: &'static str,
    ) -> Result<&EngineFailureRecord> {
        let active = self.active_failure(action.subsystem, action.sequence, operation)?;
        if active.recovery_attempts != action.attempt
            || active.disposition.expected_request() != Some(action.request)
        {
            return Err(coordinator_error(
                ErrorCategory::Conflict,
                Recoverability::Retryable,
                "recovery action refers to stale or reclassified failure state",
                operation,
            )
            .with_context(
                recovery_context(
                    action.subsystem,
                    action.sequence,
                    action.attempt,
                    action.request,
                )
                .with_field("active_attempt", active.recovery_attempts.to_string())
                .with_field("active_disposition", active.disposition.code()),
            ));
        }
        Ok(active)
    }

    fn push_history(&mut self, diagnostic: DiagnosticEvent) {
        if self.history.len() == self.history_capacity {
            self.history.pop_front();
        }
        self.history.push_back(diagnostic);
    }
}

fn failure_event(
    error: &Error,
    subsystem: EngineSubsystem,
    operation: &str,
    sequence: EngineFailureSequence,
    recovery_attempts: u32,
    disposition: EngineFailureDisposition,
) -> Result<DiagnosticEvent> {
    DiagnosticEvent::from_error(FAILURE_EVENT, COMPONENT, error)?
        .with_field("subsystem", TraceField::user_safe(subsystem.code()))?
        .with_field(
            "recoverability",
            TraceField::user_safe(error.recoverability().code()),
        )?
        .with_field("disposition", TraceField::user_safe(disposition.code()))?
        .with_field("operation", TraceField::internal(operation))?
        .with_field("failure_sequence", TraceField::internal(sequence.get()))?
        .with_field(
            "recovery_attempt",
            TraceField::internal(u64::from(recovery_attempts)),
        )
}

fn recovery_event(
    active: &EngineFailureRecord,
    action: EngineRecoveryAction,
) -> Result<DiagnosticEvent> {
    DiagnosticEvent::new(
        RECOVERY_EVENT,
        COMPONENT,
        DiagnosticSeverity::Info,
        "engine subsystem recovered",
    )?
    .with_field("subsystem", TraceField::user_safe(action.subsystem.code()))?
    .with_field(
        "disposition",
        TraceField::user_safe(active.disposition.code()),
    )?
    .with_field(
        "recovery_request",
        TraceField::user_safe(action.request.code()),
    )?
    .with_field("operation", TraceField::internal(active.operation.clone()))?
    .with_field(
        "failure_sequence",
        TraceField::internal(action.sequence.get()),
    )?
    .with_field(
        "recovery_attempt",
        TraceField::internal(u64::from(action.attempt)),
    )
}

fn failure_context(
    operation_name: &'static str,
    subsystem: EngineSubsystem,
    failed_operation: &str,
    sequence: EngineFailureSequence,
    recovery_attempts: u32,
) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation_name)
        .with_field("subsystem", subsystem.code())
        .with_field("operation", failed_operation)
        .with_field("failure_sequence", sequence.to_string())
        .with_field("recovery_attempt", recovery_attempts.to_string())
}

fn recovery_context(
    subsystem: EngineSubsystem,
    sequence: EngineFailureSequence,
    recovery_attempts: u32,
    request: EngineRecoveryRequest,
) -> ErrorContext {
    ErrorContext::new(COMPONENT, "recover_subsystem")
        .with_field("subsystem", subsystem.code())
        .with_field("failure_sequence", sequence.to_string())
        .with_field("recovery_attempt", recovery_attempts.to_string())
        .with_field("recovery_request", request.code())
}

fn validate_operation(operation: String) -> Result<String> {
    let trimmed = operation.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_OPERATION_BYTES {
        return Err(coordinator_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "engine failure operation must be a nonempty bounded label",
            "validate_operation",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "operation_label")
                .with_field("bytes", trimmed.len().to_string())
                .with_field("maximum", MAX_OPERATION_BYTES.to_string()),
        ));
    }
    Ok(trimmed.to_owned())
}

fn active_failure_conflict(active: &EngineFailureRecord, operation: &'static str) -> Error {
    coordinator_error(
        ErrorCategory::Conflict,
        Recoverability::Retryable,
        "subsystem already has an active propagated failure",
        operation,
    )
    .with_context(
        ErrorContext::new(COMPONENT, "active_failure")
            .with_field("subsystem", active.subsystem.code())
            .with_field("failure_sequence", active.sequence.to_string()),
    )
}

fn stale_failure_error(
    subsystem: EngineSubsystem,
    actual: EngineFailureSequence,
    expected: Option<EngineFailureSequence>,
    operation: &'static str,
) -> Error {
    let mut context = ErrorContext::new(COMPONENT, "stale_failure")
        .with_field("subsystem", subsystem.code())
        .with_field("actual_sequence", actual.to_string());
    if let Some(expected) = expected {
        context.insert_field("expected_sequence", expected.to_string());
    } else {
        context.insert_field("expected_sequence", "none");
    }
    coordinator_error(
        ErrorCategory::Conflict,
        Recoverability::Retryable,
        "failure sequence is stale or no longer active",
        operation,
    )
    .with_context(context)
}

fn coordinator_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
