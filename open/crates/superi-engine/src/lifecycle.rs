//! Subsystem lifecycle and shared-state management.
//!
//! The engine-control domain owns one explicit lifecycle state machine. It emits one immutable,
//! revision-scoped action at a time so subsystem owners can acquire, recover, and release resources
//! on their legal execution domains without blocking engine control. Playback, rendering, and
//! export consume the same generated snapshot and cannot admit work unless every dependency they
//! require is ready in the current engine lifetime.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use superi_concurrency::lifecycle::{
    LifecycleCoordinator, LifecycleParticipant, LifecyclePhase, LifecycleSignal,
};
use superi_concurrency::shared::{DomainOwned, SharedSnapshot, SnapshotPublisher};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

const COMPONENT: &str = "superi-engine.lifecycle";
const PARTICIPANT: &str = "superi-engine";

const NO_DEPENDENCIES: &[EngineSubsystem] = &[];
const SHARED_STATE_DEPENDENCY: &[EngineSubsystem] = &[EngineSubsystem::SharedState];
const RENDERING_DEPENDENCY: &[EngineSubsystem] = &[EngineSubsystem::Rendering];
const PLAYBACK_REQUIREMENTS: &[EngineSubsystem] =
    &[EngineSubsystem::SharedState, EngineSubsystem::Playback];
const RENDERING_REQUIREMENTS: &[EngineSubsystem] =
    &[EngineSubsystem::SharedState, EngineSubsystem::Rendering];
const EXPORT_REQUIREMENTS: &[EngineSubsystem] = &[
    EngineSubsystem::SharedState,
    EngineSubsystem::Rendering,
    EngineSubsystem::Export,
];

/// One canonical subsystem in the engine lifecycle plan.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum EngineSubsystem {
    /// Authoritative engine state and immutable publication infrastructure.
    SharedState,
    /// Interactive playback ownership and resources.
    Playback,
    /// Frame rendering ownership and resources.
    Rendering,
    /// Export ownership and resources built on rendering.
    Export,
}

impl EngineSubsystem {
    /// Every subsystem in deterministic initialization order.
    pub const ALL: [Self; 4] = [
        Self::SharedState,
        Self::Playback,
        Self::Rendering,
        Self::Export,
    ];

    /// Returns the stable diagnostic code for this subsystem.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::SharedState => "shared_state",
            Self::Playback => "playback",
            Self::Rendering => "rendering",
            Self::Export => "export",
        }
    }

    /// Returns direct dependencies in deterministic order.
    #[must_use]
    pub const fn dependencies(self) -> &'static [Self] {
        match self {
            Self::SharedState => NO_DEPENDENCIES,
            Self::Playback | Self::Rendering => SHARED_STATE_DEPENDENCY,
            Self::Export => RENDERING_DEPENDENCY,
        }
    }
}

impl fmt::Display for EngineSubsystem {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// One explicit resource state for a canonical engine subsystem.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum EngineSubsystemState {
    /// No resource from this subsystem is owned in the current lifetime.
    Offline,
    /// The subsystem owner is acquiring its resources.
    Initializing,
    /// The subsystem is ready for dependent work.
    Ready,
    /// The subsystem retains resources but cannot currently serve dependent work.
    Degraded,
    /// The subsystem owner is revalidating or reacquiring degraded resources.
    Recovering,
    /// The subsystem owner is releasing resources.
    Stopping,
    /// The subsystem failed and retains explicit diagnostic state.
    Failed,
}

impl EngineSubsystemState {
    /// Returns the stable diagnostic code for this state.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Offline => "offline",
            Self::Initializing => "initializing",
            Self::Ready => "ready",
            Self::Degraded => "degraded",
            Self::Recovering => "recovering",
            Self::Stopping => "stopping",
            Self::Failed => "failed",
        }
    }
}

/// Aggregate engine health independent of the application lifecycle phase.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum EngineHealth {
    /// No subsystem has an unresolved failure.
    Healthy,
    /// At least one recoverable subsystem failure limits dependent work.
    Degraded,
    /// A terminal or lifecycle-level failure prevents all work.
    Failed,
}

impl EngineHealth {
    /// Returns the stable diagnostic code for this health state.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Degraded => "degraded",
            Self::Failed => "failed",
        }
    }
}

/// Work performed by a subsystem owner for one lifecycle transition.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum EngineLifecycleActionKind {
    /// Acquire and validate the subsystem's resources.
    Initialize,
    /// Revalidate or reacquire resources after a recoverable failure.
    Recover,
    /// Release every resource still owned by the subsystem.
    Teardown,
}

impl EngineLifecycleActionKind {
    /// Returns the stable diagnostic code for this action kind.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Initialize => "initialize",
            Self::Recover => "recover",
            Self::Teardown => "teardown",
        }
    }
}

/// An immutable token for exactly one lifecycle action.
///
/// Subsystem owners must return this complete token after doing the requested work. Tokens from an
/// older action or lifetime are rejected so late work cannot advance newer state.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EngineLifecycleAction {
    subsystem: EngineSubsystem,
    kind: EngineLifecycleActionKind,
    lifetime: u64,
    revision: u64,
}

impl EngineLifecycleAction {
    /// Returns the subsystem that owns the work.
    #[must_use]
    pub const fn subsystem(self) -> EngineSubsystem {
        self.subsystem
    }

    /// Returns the requested kind of lifecycle work.
    #[must_use]
    pub const fn kind(self) -> EngineLifecycleActionKind {
        self.kind
    }

    /// Returns the engine lifetime that issued this action.
    #[must_use]
    pub const fn lifetime(self) -> u64 {
        self.lifetime
    }

    /// Returns the monotonic action revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
}

/// Cloneable failure evidence retained in immutable engine snapshots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineFailureSnapshot {
    subsystem: EngineSubsystem,
    category: ErrorCategory,
    recoverability: Recoverability,
    message: String,
    contexts: Vec<ErrorContext>,
}

impl EngineFailureSnapshot {
    fn from_error(subsystem: EngineSubsystem, error: &Error) -> Self {
        Self {
            subsystem,
            category: error.category(),
            recoverability: error.recoverability(),
            message: error.message().to_owned(),
            contexts: error.contexts().to_vec(),
        }
    }

    /// Returns the subsystem that reported this failure.
    #[must_use]
    pub const fn subsystem(&self) -> EngineSubsystem {
        self.subsystem
    }

    /// Returns the stable error category.
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        self.category
    }

    /// Returns the explicit recovery classification.
    #[must_use]
    pub const fn recoverability(&self) -> Recoverability {
        self.recoverability
    }

    /// Returns the concise diagnostic message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns context frames from the failing operation toward outer callers.
    #[must_use]
    pub fn contexts(&self) -> &[ErrorContext] {
        &self.contexts
    }
}

/// Immutable state for one subsystem in an engine snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineSubsystemSnapshot {
    subsystem: EngineSubsystem,
    state: EngineSubsystemState,
    owns_resources: bool,
    failure: Option<EngineFailureSnapshot>,
}

impl EngineSubsystemSnapshot {
    /// Returns the canonical subsystem.
    #[must_use]
    pub const fn subsystem(&self) -> EngineSubsystem {
        self.subsystem
    }

    /// Returns the current resource state.
    #[must_use]
    pub const fn state(&self) -> EngineSubsystemState {
        self.state
    }

    /// Returns whether teardown remains required for this subsystem.
    #[must_use]
    pub const fn owns_resources(&self) -> bool {
        self.owns_resources
    }

    /// Returns retained failure evidence, when present.
    #[must_use]
    pub const fn failure(&self) -> Option<&EngineFailureSnapshot> {
        self.failure.as_ref()
    }
}

/// One engine workflow admitted from the shared lifecycle snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum EngineWorkKind {
    /// Interactive playback work.
    Playback,
    /// Frame rendering work.
    Rendering,
    /// Export work, including its rendering dependency.
    Export,
}

impl EngineWorkKind {
    /// Returns the stable diagnostic code for this work kind.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Playback => "playback",
            Self::Rendering => "rendering",
            Self::Export => "export",
        }
    }

    /// Returns every subsystem required by this work in dependency order.
    #[must_use]
    pub const fn required_subsystems(self) -> &'static [EngineSubsystem] {
        match self {
            Self::Playback => PLAYBACK_REQUIREMENTS,
            Self::Rendering => RENDERING_REQUIREMENTS,
            Self::Export => EXPORT_REQUIREMENTS,
        }
    }
}

/// A revision-scoped permit for one coherent engine snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct EngineWorkPermit {
    work: EngineWorkKind,
    lifetime: u64,
    state_revision: u64,
}

impl EngineWorkPermit {
    /// Returns the permitted work kind.
    #[must_use]
    pub const fn work(self) -> EngineWorkKind {
        self.work
    }

    /// Returns the engine lifetime that issued the permit.
    #[must_use]
    pub const fn lifetime(self) -> u64 {
        self.lifetime
    }

    /// Returns the exact engine state revision that issued the permit.
    #[must_use]
    pub const fn state_revision(self) -> u64 {
        self.state_revision
    }
}

/// Diagnostic evidence for work denied by the current engine snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineWorkDenial {
    work: EngineWorkKind,
    phase: LifecyclePhase,
    blocking_subsystem: Option<EngineSubsystem>,
    failure: Option<EngineFailureSnapshot>,
}

impl EngineWorkDenial {
    /// Returns the denied work kind.
    #[must_use]
    pub const fn work(&self) -> EngineWorkKind {
        self.work
    }

    /// Returns the application and engine lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> LifecyclePhase {
        self.phase
    }

    /// Returns the first unavailable dependency, or `None` when the lifecycle phase blocks work.
    #[must_use]
    pub const fn blocking_subsystem(&self) -> Option<EngineSubsystem> {
        self.blocking_subsystem
    }

    /// Returns retained failure evidence associated with the denial.
    #[must_use]
    pub const fn failure(&self) -> Option<&EngineFailureSnapshot> {
        self.failure.as_ref()
    }
}

/// The coherent admission result for one engine workflow.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum EngineWorkAdmission {
    /// Every required subsystem is ready in the current running lifetime.
    Granted(EngineWorkPermit),
    /// The phase or a required subsystem prevents work.
    Denied(EngineWorkDenial),
}

impl EngineWorkAdmission {
    /// Returns the permit when work is granted.
    #[must_use]
    pub const fn permit(&self) -> Option<EngineWorkPermit> {
        match self {
            Self::Granted(permit) => Some(*permit),
            Self::Denied(_) => None,
        }
    }

    /// Returns denial evidence when work is unavailable.
    #[must_use]
    pub const fn denial(&self) -> Option<&EngineWorkDenial> {
        match self {
            Self::Granted(_) => None,
            Self::Denied(denial) => Some(denial),
        }
    }
}

/// One immutable, generated view of the complete engine lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EngineLifecycleSnapshot {
    phase: LifecyclePhase,
    lifecycle_revision: u64,
    state_revision: u64,
    lifetime: u64,
    health: EngineHealth,
    subsystems: Vec<EngineSubsystemSnapshot>,
    pending_action: Option<EngineLifecycleAction>,
    failure: Option<EngineFailureSnapshot>,
}

impl EngineLifecycleSnapshot {
    /// Returns the acknowledged application and engine lifecycle phase.
    #[must_use]
    pub const fn phase(&self) -> LifecyclePhase {
        self.phase
    }

    /// Returns the revision from the shared lifecycle coordinator.
    #[must_use]
    pub const fn lifecycle_revision(&self) -> u64 {
        self.lifecycle_revision
    }

    /// Returns the engine-control state revision.
    #[must_use]
    pub const fn state_revision(&self) -> u64 {
        self.state_revision
    }

    /// Returns the current engine lifetime.
    #[must_use]
    pub const fn lifetime(&self) -> u64 {
        self.lifetime
    }

    /// Returns aggregate engine health.
    #[must_use]
    pub const fn health(&self) -> EngineHealth {
        self.health
    }

    /// Returns subsystem snapshots in deterministic initialization order.
    #[must_use]
    pub fn subsystems(&self) -> &[EngineSubsystemSnapshot] {
        &self.subsystems
    }

    /// Returns one subsystem snapshot.
    #[must_use]
    pub fn subsystem(&self, subsystem: EngineSubsystem) -> Option<&EngineSubsystemSnapshot> {
        self.subsystems
            .iter()
            .find(|snapshot| snapshot.subsystem == subsystem)
    }

    /// Returns the exact outstanding lifecycle action, when one exists.
    #[must_use]
    pub const fn pending_action(&self) -> Option<EngineLifecycleAction> {
        self.pending_action
    }

    /// Returns the most recently reported failure in this engine lifetime.
    #[must_use]
    pub const fn failure(&self) -> Option<&EngineFailureSnapshot> {
        self.failure.as_ref()
    }

    /// Evaluates one workflow against this complete immutable snapshot.
    #[must_use]
    pub fn admit(&self, work: EngineWorkKind) -> EngineWorkAdmission {
        if self.phase != LifecyclePhase::Running {
            return EngineWorkAdmission::Denied(EngineWorkDenial {
                work,
                phase: self.phase,
                blocking_subsystem: None,
                failure: self.failure.clone(),
            });
        }

        for subsystem in work.required_subsystems() {
            let Some(snapshot) = self.subsystem(*subsystem) else {
                return EngineWorkAdmission::Denied(EngineWorkDenial {
                    work,
                    phase: self.phase,
                    blocking_subsystem: Some(*subsystem),
                    failure: self.failure.clone(),
                });
            };
            if snapshot.state != EngineSubsystemState::Ready {
                return EngineWorkAdmission::Denied(EngineWorkDenial {
                    work,
                    phase: self.phase,
                    blocking_subsystem: Some(*subsystem),
                    failure: snapshot.failure.clone().or_else(|| self.failure.clone()),
                });
            }
        }

        EngineWorkAdmission::Granted(EngineWorkPermit {
            work,
            lifetime: self.lifetime,
            state_revision: self.state_revision,
        })
    }

    /// Returns whether a previously issued permit still names this exact healthy state revision.
    #[must_use]
    pub fn is_permit_current(&self, permit: EngineWorkPermit) -> bool {
        permit.lifetime == self.lifetime
            && permit.state_revision == self.state_revision
            && self.admit(permit.work).permit() == Some(permit)
    }
}

/// EngineControl-owned lifecycle coordinator for the canonical engine subsystems.
///
/// Construction and every mutating or snapshot operation require the engine-control execution
/// domain. The retained lock-free [`LifecycleSignal`] may be cloned once and observed from
/// latency-sensitive paths without calling back into this owner.
pub struct EngineLifecycle {
    owned: DomainOwned<EngineOwnedState>,
    signal: LifecycleSignal,
}

impl EngineLifecycle {
    /// Creates the canonical dependency plan and emits the first initialization action.
    pub fn new() -> Result<Self> {
        ExecutionDomain::EngineControl.require_current()?;
        validate_dependency_plan()?;

        let mut state = EngineOwnedState::new()?;
        let signal = state.coordinator.signal();
        state.schedule_next_initialization()?;
        state.publish()?;
        Ok(Self {
            owned: DomainOwned::new(ExecutionDomain::EngineControl, state),
            signal,
        })
    }

    /// Returns the latest immutable generated snapshot.
    pub fn snapshot(&self) -> Result<SharedSnapshot<EngineLifecycleSnapshot>> {
        self.owned.with(|state| state.latest())
    }

    /// Returns the lock-free application and engine lifecycle signal.
    #[must_use]
    pub fn signal(&self) -> LifecycleSignal {
        self.signal.clone()
    }

    /// Completes the exact outstanding initialization, recovery, or teardown action.
    pub fn complete_action(
        &mut self,
        action: EngineLifecycleAction,
    ) -> Result<SharedSnapshot<EngineLifecycleSnapshot>> {
        self.owned.with_mut(|state| state.complete_action(action))?
    }

    /// Reports failure of the exact outstanding lifecycle action.
    pub fn fail_action(
        &mut self,
        action: EngineLifecycleAction,
        error: Error,
    ) -> Result<SharedSnapshot<EngineLifecycleSnapshot>> {
        self.owned
            .with_mut(|state| state.fail_action(action, error))?
    }

    /// Reports a failure from a ready subsystem during normal work.
    pub fn report_runtime_failure(
        &mut self,
        subsystem: EngineSubsystem,
        error: Error,
    ) -> Result<SharedSnapshot<EngineLifecycleSnapshot>> {
        self.owned
            .with_mut(|state| state.report_runtime_failure(subsystem, error))?
    }

    /// Emits a recovery action for one degraded subsystem.
    pub fn begin_recovery(
        &mut self,
        subsystem: EngineSubsystem,
    ) -> Result<SharedSnapshot<EngineLifecycleSnapshot>> {
        self.owned
            .with_mut(|state| state.begin_recovery(subsystem))?
    }

    /// Requests deterministic reverse teardown.
    pub fn begin_shutdown(&mut self) -> Result<SharedSnapshot<EngineLifecycleSnapshot>> {
        self.owned.with_mut(|state| state.begin_shutdown(false))?
    }

    /// Requests orderly teardown followed by a fresh engine lifetime.
    pub fn begin_restart(&mut self) -> Result<SharedSnapshot<EngineLifecycleSnapshot>> {
        self.owned.with_mut(|state| state.begin_restart())?
    }
}

impl fmt::Debug for EngineLifecycle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EngineLifecycle")
            .field("signal", &self.signal)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EngineMode {
    Starting,
    Running,
    Recovering,
    Stopping,
    RollingBack,
    Idle,
}

#[derive(Clone, Debug)]
struct SubsystemRecord {
    state: EngineSubsystemState,
    owns_resources: bool,
    failure: Option<EngineFailureSnapshot>,
}

impl SubsystemRecord {
    const fn offline() -> Self {
        Self {
            state: EngineSubsystemState::Offline,
            owns_resources: false,
            failure: None,
        }
    }
}

struct EngineOwnedState {
    coordinator: LifecycleCoordinator,
    participant: LifecycleParticipant,
    publisher: SnapshotPublisher<EngineLifecycleSnapshot>,
    latest: Option<SharedSnapshot<EngineLifecycleSnapshot>>,
    records: BTreeMap<EngineSubsystem, SubsystemRecord>,
    mode: EngineMode,
    lifetime: u64,
    state_revision: u64,
    action_revision: u64,
    current_action: Option<EngineLifecycleAction>,
    teardown_attempted: BTreeSet<EngineSubsystem>,
    restart_after_stop: bool,
    last_failure: Option<EngineFailureSnapshot>,
}

impl EngineOwnedState {
    fn new() -> Result<Self> {
        let coordinator = LifecycleCoordinator::new([PARTICIPANT])?;
        let participant = coordinator.participant(PARTICIPANT)?;
        let records = EngineSubsystem::ALL
            .into_iter()
            .map(|subsystem| (subsystem, SubsystemRecord::offline()))
            .collect();
        Ok(Self {
            coordinator,
            participant,
            publisher: SnapshotPublisher::new(ExecutionDomain::EngineControl),
            latest: None,
            records,
            mode: EngineMode::Starting,
            lifetime: 1,
            state_revision: 0,
            action_revision: 0,
            current_action: None,
            teardown_attempted: BTreeSet::new(),
            restart_after_stop: false,
            last_failure: None,
        })
    }

    fn latest(&self) -> SharedSnapshot<EngineLifecycleSnapshot> {
        self.latest
            .as_ref()
            .expect("engine lifecycle publishes before becoming observable")
            .clone()
    }

    fn publish(&mut self) -> Result<SharedSnapshot<EngineLifecycleSnapshot>> {
        let state_revision = self.state_revision.checked_add(1).ok_or_else(|| {
            engine_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::Terminal,
                "engine lifecycle state revision exhausted",
                "publish",
            )
        })?;
        let lifecycle = self.coordinator.snapshot();
        let subsystems = EngineSubsystem::ALL
            .into_iter()
            .map(|subsystem| {
                let record = self.record(subsystem);
                EngineSubsystemSnapshot {
                    subsystem,
                    state: record.state,
                    owns_resources: record.owns_resources,
                    failure: record.failure.clone(),
                }
            })
            .collect::<Vec<_>>();
        let health = self.health(lifecycle.phase());
        let value = Arc::new(EngineLifecycleSnapshot {
            phase: lifecycle.phase(),
            lifecycle_revision: lifecycle.revision(),
            state_revision,
            lifetime: self.lifetime,
            health,
            subsystems,
            pending_action: self.current_action,
            failure: self.last_failure.clone(),
        });
        let published = self.publisher.publish(&value)?;
        self.state_revision = state_revision;
        self.latest = Some(published.clone());
        Ok(published)
    }

    fn health(&self, phase: LifecyclePhase) -> EngineHealth {
        if phase == LifecyclePhase::Failed
            || self.records.values().any(|record| {
                record.state == EngineSubsystemState::Failed
                    && record
                        .failure
                        .as_ref()
                        .is_some_and(|failure| failure.recoverability == Recoverability::Terminal)
            })
        {
            return EngineHealth::Failed;
        }
        if self.records.values().any(|record| {
            matches!(
                record.state,
                EngineSubsystemState::Degraded
                    | EngineSubsystemState::Recovering
                    | EngineSubsystemState::Failed
            )
        }) {
            return EngineHealth::Degraded;
        }
        EngineHealth::Healthy
    }

    fn record(&self, subsystem: EngineSubsystem) -> &SubsystemRecord {
        self.records
            .get(&subsystem)
            .expect("canonical subsystem record exists")
    }

    fn record_mut(&mut self, subsystem: EngineSubsystem) -> &mut SubsystemRecord {
        self.records
            .get_mut(&subsystem)
            .expect("canonical subsystem record exists")
    }

    fn issue_action(
        &mut self,
        subsystem: EngineSubsystem,
        kind: EngineLifecycleActionKind,
    ) -> Result<()> {
        let revision = self.action_revision.checked_add(1).ok_or_else(|| {
            engine_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::Terminal,
                "engine lifecycle action revision exhausted",
                "issue_action",
            )
        })?;
        self.action_revision = revision;
        self.current_action = Some(EngineLifecycleAction {
            subsystem,
            kind,
            lifetime: self.lifetime,
            revision,
        });
        Ok(())
    }

    fn validate_action(&self, action: EngineLifecycleAction) -> Result<()> {
        if self.current_action == Some(action) {
            return Ok(());
        }
        let mut context = ErrorContext::new(COMPONENT, "stale_action")
            .with_field("actual_subsystem", action.subsystem.code())
            .with_field("actual_kind", action.kind.code())
            .with_field("actual_lifetime", action.lifetime.to_string())
            .with_field("actual_revision", action.revision.to_string());
        if let Some(expected) = self.current_action {
            context.insert_field("expected_subsystem", expected.subsystem.code());
            context.insert_field("expected_kind", expected.kind.code());
            context.insert_field("expected_lifetime", expected.lifetime.to_string());
            context.insert_field("expected_revision", expected.revision.to_string());
        } else {
            context.insert_field("expected_action", "none");
        }
        Err(engine_error(
            ErrorCategory::Conflict,
            Recoverability::Retryable,
            "lifecycle completion refers to a stale or out-of-order action",
            "validate_action",
        )
        .with_context(context))
    }

    fn schedule_next_initialization(&mut self) -> Result<()> {
        for subsystem in EngineSubsystem::ALL {
            let record = self.record(subsystem);
            if record.owns_resources || record.state != EngineSubsystemState::Offline {
                continue;
            }
            if subsystem.dependencies().iter().all(|dependency| {
                let dependency = self.record(*dependency);
                dependency.owns_resources && dependency.state == EngineSubsystemState::Ready
            }) {
                let record = self.record_mut(subsystem);
                record.state = EngineSubsystemState::Initializing;
                record.failure = None;
                self.issue_action(subsystem, EngineLifecycleActionKind::Initialize)?;
                return Ok(());
            }
        }

        let unfinished = EngineSubsystem::ALL.into_iter().any(|subsystem| {
            let record = self.record(subsystem);
            !record.owns_resources && record.state == EngineSubsystemState::Offline
        });
        if unfinished {
            return Err(engine_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "engine lifecycle dependency plan cannot make initialization progress",
                "schedule_initialization",
            ));
        }

        let observed = self.participant.signal().load();
        let completed = self.participant.acknowledge(observed)?;
        if completed.phase() != LifecyclePhase::Running {
            return Err(engine_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "engine initialization acknowledgement did not reach running",
                "complete_initialization",
            ));
        }
        self.mode = EngineMode::Running;
        Ok(())
    }

    fn complete_action(
        &mut self,
        action: EngineLifecycleAction,
    ) -> Result<SharedSnapshot<EngineLifecycleSnapshot>> {
        self.validate_action(action)?;
        self.current_action = None;
        match action.kind {
            EngineLifecycleActionKind::Initialize => {
                if self.mode != EngineMode::Starting
                    || self.record(action.subsystem).state != EngineSubsystemState::Initializing
                {
                    return Err(invalid_state("complete_initialization"));
                }
                let record = self.record_mut(action.subsystem);
                record.state = EngineSubsystemState::Ready;
                record.owns_resources = true;
                record.failure = None;
                self.schedule_next_initialization()?;
            }
            EngineLifecycleActionKind::Recover => {
                if self.mode != EngineMode::Recovering
                    || self.record(action.subsystem).state != EngineSubsystemState::Recovering
                {
                    return Err(invalid_state("complete_recovery"));
                }
                let record = self.record_mut(action.subsystem);
                record.state = EngineSubsystemState::Ready;
                record.failure = None;
                self.mode = EngineMode::Running;
            }
            EngineLifecycleActionKind::Teardown => {
                if !matches!(self.mode, EngineMode::Stopping | EngineMode::RollingBack)
                    || self.record(action.subsystem).state != EngineSubsystemState::Stopping
                {
                    return Err(invalid_state("complete_teardown"));
                }
                self.teardown_attempted.insert(action.subsystem);
                let record = self.record_mut(action.subsystem);
                record.state = EngineSubsystemState::Offline;
                record.owns_resources = false;
                record.failure = None;
                self.schedule_next_teardown()?;
            }
        }
        self.publish()
    }

    fn fail_action(
        &mut self,
        action: EngineLifecycleAction,
        error: Error,
    ) -> Result<SharedSnapshot<EngineLifecycleSnapshot>> {
        self.validate_action(action)?;
        let failure = EngineFailureSnapshot::from_error(action.subsystem, &error);
        self.last_failure = Some(failure.clone());
        self.current_action = None;

        match action.kind {
            EngineLifecycleActionKind::Initialize => {
                if self.mode != EngineMode::Starting {
                    return Err(invalid_state("fail_initialization"));
                }
                let record = self.record_mut(action.subsystem);
                record.state = EngineSubsystemState::Failed;
                record.owns_resources = false;
                record.failure = Some(failure);
                self.participant.fail(error)?;
                self.mode = EngineMode::RollingBack;
                self.teardown_attempted.clear();
                self.schedule_next_teardown()?;
            }
            EngineLifecycleActionKind::Recover => {
                if self.mode != EngineMode::Recovering {
                    return Err(invalid_state("fail_recovery"));
                }
                let terminal = error.recoverability() == Recoverability::Terminal;
                let record = self.record_mut(action.subsystem);
                record.state = if terminal {
                    EngineSubsystemState::Failed
                } else {
                    EngineSubsystemState::Degraded
                };
                record.failure = Some(failure);
                if terminal {
                    self.participant.fail(error)?;
                    self.mode = EngineMode::Idle;
                } else {
                    self.mode = EngineMode::Running;
                }
            }
            EngineLifecycleActionKind::Teardown => {
                if !matches!(self.mode, EngineMode::Stopping | EngineMode::RollingBack) {
                    return Err(invalid_state("fail_teardown"));
                }
                let record = self.record_mut(action.subsystem);
                record.state = EngineSubsystemState::Failed;
                record.owns_resources = true;
                record.failure = Some(failure);
                self.teardown_attempted.insert(action.subsystem);
                self.participant.fail(error)?;
                self.schedule_next_teardown()?;
            }
        }
        self.publish()
    }

    fn report_runtime_failure(
        &mut self,
        subsystem: EngineSubsystem,
        error: Error,
    ) -> Result<SharedSnapshot<EngineLifecycleSnapshot>> {
        if self.coordinator.snapshot().phase() != LifecyclePhase::Running
            || self.mode != EngineMode::Running
            || self.current_action.is_some()
        {
            return Err(invalid_state("report_runtime_failure"));
        }
        if self.record(subsystem).state != EngineSubsystemState::Ready
            || !self.record(subsystem).owns_resources
        {
            return Err(engine_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "only a ready owned subsystem may report a runtime failure",
                "report_runtime_failure",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "runtime_failure")
                    .with_field("subsystem", subsystem.code()),
            ));
        }

        let terminal = error.recoverability() == Recoverability::Terminal;
        let failure = EngineFailureSnapshot::from_error(subsystem, &error);
        self.last_failure = Some(failure.clone());
        let record = self.record_mut(subsystem);
        record.state = if terminal {
            EngineSubsystemState::Failed
        } else {
            EngineSubsystemState::Degraded
        };
        record.failure = Some(failure);
        if terminal {
            self.participant.fail(error)?;
            self.mode = EngineMode::Idle;
        }
        self.publish()
    }

    fn begin_recovery(
        &mut self,
        subsystem: EngineSubsystem,
    ) -> Result<SharedSnapshot<EngineLifecycleSnapshot>> {
        if self.coordinator.snapshot().phase() != LifecyclePhase::Running
            || self.mode != EngineMode::Running
            || self.current_action.is_some()
            || self.record(subsystem).state != EngineSubsystemState::Degraded
        {
            return Err(invalid_state("begin_recovery"));
        }
        if !subsystem.dependencies().iter().all(|dependency| {
            let record = self.record(*dependency);
            record.owns_resources && record.state == EngineSubsystemState::Ready
        }) {
            return Err(engine_error(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "subsystem recovery dependencies are not ready",
                "begin_recovery",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "recovery_dependency")
                    .with_field("subsystem", subsystem.code()),
            ));
        }

        self.record_mut(subsystem).state = EngineSubsystemState::Recovering;
        self.mode = EngineMode::Recovering;
        self.issue_action(subsystem, EngineLifecycleActionKind::Recover)?;
        self.publish()
    }

    fn begin_restart(&mut self) -> Result<SharedSnapshot<EngineLifecycleSnapshot>> {
        if self.coordinator.snapshot().phase() == LifecyclePhase::Stopped {
            self.restart_after_stop = false;
            self.start_new_lifetime()?;
            return self.publish();
        }
        self.begin_shutdown(true)
    }

    fn begin_shutdown(
        &mut self,
        restart_after_stop: bool,
    ) -> Result<SharedSnapshot<EngineLifecycleSnapshot>> {
        let phase = self.coordinator.snapshot().phase();
        if phase == LifecyclePhase::Stopped {
            if restart_after_stop {
                self.restart_after_stop = false;
                self.start_new_lifetime()?;
                return self.publish();
            }
            return Ok(self.latest());
        }
        if self.mode == EngineMode::Stopping && self.current_action.is_some() {
            if restart_after_stop {
                self.restart_after_stop = true;
            }
            return Ok(self.latest());
        }

        self.restart_after_stop = restart_after_stop;
        self.cancel_pending_action();
        self.coordinator.request_shutdown()?;
        self.mode = EngineMode::Stopping;
        self.teardown_attempted.clear();
        self.schedule_next_teardown()?;
        self.publish()
    }

    fn cancel_pending_action(&mut self) {
        let Some(action) = self.current_action.take() else {
            return;
        };
        let record = self.record_mut(action.subsystem);
        match action.kind {
            EngineLifecycleActionKind::Initialize => {
                record.state = EngineSubsystemState::Offline;
                record.owns_resources = false;
            }
            EngineLifecycleActionKind::Recover => {
                record.state = EngineSubsystemState::Degraded;
                record.owns_resources = true;
            }
            EngineLifecycleActionKind::Teardown => {
                record.state = if record.failure.is_some() {
                    EngineSubsystemState::Failed
                } else {
                    EngineSubsystemState::Ready
                };
                record.owns_resources = true;
            }
        }
    }

    fn schedule_next_teardown(&mut self) -> Result<()> {
        for subsystem in EngineSubsystem::ALL.into_iter().rev() {
            let record = self.record(subsystem);
            if !record.owns_resources || self.teardown_attempted.contains(&subsystem) {
                continue;
            }
            if self.has_owned_dependent(subsystem) {
                continue;
            }
            self.record_mut(subsystem).state = EngineSubsystemState::Stopping;
            self.issue_action(subsystem, EngineLifecycleActionKind::Teardown)?;
            return Ok(());
        }

        self.current_action = None;
        if self.records.values().any(|record| record.owns_resources) {
            self.mode = EngineMode::Idle;
            return Ok(());
        }

        match self.mode {
            EngineMode::Stopping => {
                if self.coordinator.snapshot().phase() == LifecyclePhase::Stopping {
                    let observed = self.participant.signal().load();
                    let completed = self.participant.acknowledge(observed)?;
                    if completed.phase() != LifecyclePhase::Stopped {
                        return Err(engine_error(
                            ErrorCategory::Internal,
                            Recoverability::Terminal,
                            "engine teardown acknowledgement did not reach stopped",
                            "complete_teardown",
                        ));
                    }
                }
                self.mode = EngineMode::Idle;
                if self.restart_after_stop {
                    self.restart_after_stop = false;
                    self.start_new_lifetime()?;
                }
            }
            EngineMode::RollingBack => self.mode = EngineMode::Idle,
            _ => return Err(invalid_state("schedule_teardown")),
        }
        Ok(())
    }

    fn has_owned_dependent(&self, dependency: EngineSubsystem) -> bool {
        EngineSubsystem::ALL.into_iter().any(|subsystem| {
            subsystem != dependency
                && self.record(subsystem).owns_resources
                && subsystem.dependencies().contains(&dependency)
        })
    }

    fn start_new_lifetime(&mut self) -> Result<()> {
        if self.coordinator.snapshot().phase() != LifecyclePhase::Stopped {
            return Err(invalid_state("start_new_lifetime"));
        }
        self.coordinator.request_restart()?;
        self.lifetime = self.lifetime.checked_add(1).ok_or_else(|| {
            engine_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::Terminal,
                "engine lifecycle lifetime exhausted",
                "restart",
            )
        })?;
        for record in self.records.values_mut() {
            *record = SubsystemRecord::offline();
        }
        self.mode = EngineMode::Starting;
        self.current_action = None;
        self.teardown_attempted.clear();
        self.last_failure = None;
        self.schedule_next_initialization()
    }
}

fn validate_dependency_plan() -> Result<()> {
    let mut seen = BTreeSet::new();
    for subsystem in EngineSubsystem::ALL {
        if !seen.insert(subsystem) {
            return Err(engine_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "engine lifecycle plan contains a duplicate subsystem",
                "validate_plan",
            ));
        }
        for dependency in subsystem.dependencies() {
            if *dependency == subsystem || !seen.contains(dependency) {
                return Err(engine_error(
                    ErrorCategory::Internal,
                    Recoverability::Terminal,
                    "engine lifecycle dependency must precede its consumer",
                    "validate_plan",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "dependency_order")
                        .with_field("subsystem", subsystem.code())
                        .with_field("dependency", dependency.code()),
                ));
            }
        }
    }
    Ok(())
}

fn invalid_state(operation: &'static str) -> Error {
    engine_error(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        "engine lifecycle operation is invalid in the current state",
        operation,
    )
}

fn engine_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
