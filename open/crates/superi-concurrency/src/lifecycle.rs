//! Explicit application and engine lifecycle coordination.
//!
//! Platform event loops request transitions without waiting for subsystem work. Registered
//! participants observe a revision-scoped signal, perform the bounded work required by that phase,
//! and acknowledge the exact transition they observed. The coordinator advances only after every
//! participant acknowledges, so startup, quiescence, device release, and teardown remain ordered.
//!
//! Latency-sensitive paths retain a [`LifecycleSignal`] before entering their callback or loop and
//! call only [`LifecycleSignal::load`]. Blocking-safe owners may instead use
//! [`LifecycleCoordinator::wait_for_change`] to sleep without polling.

use std::collections::BTreeSet;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::Duration;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::threads::current_execution_domain;

const COMPONENT: &str = "superi-concurrency.lifecycle";
const PHASE_BITS: u32 = 8;
const PHASE_MASK: u64 = (1 << PHASE_BITS) - 1;
const MAX_REVISION: u64 = u64::MAX >> PHASE_BITS;

/// One explicit lifecycle phase shared by application and engine owners.
///
/// Transitional phases require one acknowledgement from every registered participant. Stable
/// phases describe what consumers may do after that transition completes.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
#[non_exhaustive]
pub enum LifecyclePhase {
    /// Participants are acquiring their initial resources.
    Starting = 0,
    /// Normal interactive and background work may run.
    Running = 1,
    /// Participants must stop starting work and reach a resumable boundary.
    Pausing = 2,
    /// Work is quiescent but resources remain available for a fast resume.
    Paused = 3,
    /// Participants are reacquiring work needed after a manual pause.
    Resuming = 4,
    /// Participants must quiesce and release resources that cannot survive system sleep.
    PreparingSleep = 5,
    /// The process is prepared for system sleep.
    Sleeping = 6,
    /// Participants are validating and reacquiring resources after system wake.
    Waking = 7,
    /// Participants must stop accepting work and release owned resources.
    Stopping = 8,
    /// Every participant completed teardown.
    Stopped = 9,
    /// A classified participant failure prevents normal work in this lifetime.
    Failed = 10,
}

impl LifecyclePhase {
    /// Every phase in stable diagnostic order.
    pub const ALL: &'static [Self] = &[
        Self::Starting,
        Self::Running,
        Self::Pausing,
        Self::Paused,
        Self::Resuming,
        Self::PreparingSleep,
        Self::Sleeping,
        Self::Waking,
        Self::Stopping,
        Self::Stopped,
        Self::Failed,
    ];

    /// Returns the stable diagnostic code for this phase.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Starting => "starting",
            Self::Running => "running",
            Self::Pausing => "pausing",
            Self::Paused => "paused",
            Self::Resuming => "resuming",
            Self::PreparingSleep => "preparing_sleep",
            Self::Sleeping => "sleeping",
            Self::Waking => "waking",
            Self::Stopping => "stopping",
            Self::Stopped => "stopped",
            Self::Failed => "failed",
        }
    }

    /// Returns whether this phase requires every participant to acknowledge the current revision.
    #[must_use]
    pub const fn requires_acknowledgement(self) -> bool {
        matches!(
            self,
            Self::Starting
                | Self::Pausing
                | Self::Resuming
                | Self::PreparingSleep
                | Self::Waking
                | Self::Stopping
        )
    }

    fn from_discriminant(value: u8) -> Self {
        match value {
            0 => Self::Starting,
            1 => Self::Running,
            2 => Self::Pausing,
            3 => Self::Paused,
            4 => Self::Resuming,
            5 => Self::PreparingSleep,
            6 => Self::Sleeping,
            7 => Self::Waking,
            8 => Self::Stopping,
            9 => Self::Stopped,
            10 => Self::Failed,
            _ => unreachable!("lifecycle signal contains an internal invalid phase"),
        }
    }
}

impl fmt::Display for LifecyclePhase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// One coherent lock-free lifecycle observation.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct LifecycleSignalSnapshot {
    phase: LifecyclePhase,
    revision: u64,
}

impl LifecycleSignalSnapshot {
    /// Returns the observed phase.
    #[must_use]
    pub const fn phase(self) -> LifecyclePhase {
        self.phase
    }

    /// Returns the observed transition revision.
    #[must_use]
    pub const fn revision(self) -> u64 {
        self.revision
    }
}

/// A retained lock-free signal for latency-sensitive consumers.
///
/// Cloning the handle may allocate reference-count state, so callers create and retain it before
/// entering a real-time callback. [`Self::load`] itself performs one atomic load and does not lock
/// or allocate.
#[derive(Clone)]
pub struct LifecycleSignal(Arc<AtomicU64>);

impl LifecycleSignal {
    fn new(phase: LifecyclePhase, revision: u64) -> Self {
        Self(Arc::new(AtomicU64::new(encode_signal(phase, revision))))
    }

    /// Loads the current phase and revision as one coherent atomic value.
    #[must_use]
    pub fn load(&self) -> LifecycleSignalSnapshot {
        decode_signal(self.0.load(Ordering::Acquire))
    }

    fn publish(&self, phase: LifecyclePhase, revision: u64) {
        self.0
            .store(encode_signal(phase, revision), Ordering::Release);
    }
}

impl fmt::Debug for LifecycleSignal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("LifecycleSignal")
            .field(&self.load())
            .finish()
    }
}

/// A retained classified failure associated with one lifecycle participant.
#[derive(Clone, Debug)]
pub struct LifecycleFailure {
    participant: String,
    error: Arc<Error>,
}

impl LifecycleFailure {
    /// Returns the registered participant that reported the failure.
    #[must_use]
    pub fn participant(&self) -> &str {
        &self.participant
    }

    /// Returns the original classified shared error.
    #[must_use]
    pub fn error(&self) -> &Error {
        &self.error
    }
}

/// An owned, coherent lifecycle snapshot suitable for diagnostics and events.
#[derive(Clone, Debug)]
pub struct LifecycleSnapshot {
    phase: LifecyclePhase,
    revision: u64,
    pending_participants: Vec<String>,
    failure: Option<LifecycleFailure>,
}

impl LifecycleSnapshot {
    fn from_state(state: &LifecycleState) -> Self {
        Self {
            phase: state.phase,
            revision: state.revision,
            pending_participants: state.pending.iter().cloned().collect(),
            failure: state.failure.clone(),
        }
    }

    /// Returns the current phase.
    #[must_use]
    pub const fn phase(&self) -> LifecyclePhase {
        self.phase
    }

    /// Returns the current transition revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns participants that have not acknowledged the current transitional phase.
    #[must_use]
    pub fn pending_participants(&self) -> &[String] {
        &self.pending_participants
    }

    /// Returns the retained classified failure, if this lifetime has reported one.
    #[must_use]
    pub const fn failure(&self) -> Option<&LifecycleFailure> {
        self.failure.as_ref()
    }
}

struct LifecycleState {
    phase: LifecyclePhase,
    revision: u64,
    participants: BTreeSet<String>,
    pending: BTreeSet<String>,
    wake_target: LifecyclePhase,
    failure: Option<LifecycleFailure>,
}

struct SharedLifecycle {
    state: Mutex<LifecycleState>,
    changed: Condvar,
    signal: LifecycleSignal,
}

/// Cloneable owner for application and engine lifecycle transitions.
#[derive(Clone)]
pub struct LifecycleCoordinator {
    shared: Arc<SharedLifecycle>,
}

impl LifecycleCoordinator {
    /// Creates a coordinator with a fixed, deterministically ordered participant set.
    ///
    /// Nonempty participant sets begin in [`LifecyclePhase::Starting`] and reach running after
    /// every participant acknowledges revision zero. An empty set begins running immediately.
    pub fn new<I, S>(participants: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut registered = BTreeSet::new();
        for participant in participants {
            let participant = participant.into();
            if participant.is_empty() || participant.trim() != participant {
                return Err(lifecycle_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "lifecycle participant names must be nonempty and have no surrounding whitespace",
                    "configure",
                ));
            }
            if !registered.insert(participant.clone()) {
                return Err(lifecycle_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "lifecycle participant names must be unique",
                    "configure",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "duplicate_participant")
                        .with_field("participant", participant),
                ));
            }
        }

        let phase = if registered.is_empty() {
            LifecyclePhase::Running
        } else {
            LifecyclePhase::Starting
        };
        let pending = if phase.requires_acknowledgement() {
            registered.clone()
        } else {
            BTreeSet::new()
        };
        Ok(Self {
            shared: Arc::new(SharedLifecycle {
                state: Mutex::new(LifecycleState {
                    phase,
                    revision: 0,
                    participants: registered,
                    pending,
                    wake_target: LifecyclePhase::Running,
                    failure: None,
                }),
                changed: Condvar::new(),
                signal: LifecycleSignal::new(phase, 0),
            }),
        })
    }

    /// Returns a retained handle for one registered participant.
    pub fn participant(&self, participant: &str) -> Result<LifecycleParticipant> {
        let state = lock_unpoisoned(&self.shared.state);
        if !state.participants.contains(participant) {
            return Err(lifecycle_error(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "lifecycle participant is not registered",
                "participant",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "lookup").with_field("participant", participant),
            ));
        }
        Ok(LifecycleParticipant {
            name: participant.to_owned(),
            coordinator: self.clone(),
            signal: self.shared.signal.clone(),
        })
    }

    /// Returns a retained lock-free signal for latency-sensitive observation.
    #[must_use]
    pub fn signal(&self) -> LifecycleSignal {
        self.shared.signal.clone()
    }

    /// Returns a coherent diagnostic snapshot without running participant code.
    #[must_use]
    pub fn snapshot(&self) -> LifecycleSnapshot {
        LifecycleSnapshot::from_state(&lock_unpoisoned(&self.shared.state))
    }

    /// Requests an acknowledged transition from active work to a resumable pause.
    pub fn request_pause(&self) -> Result<LifecycleSnapshot> {
        self.update("request_pause", |state| match state.phase {
            LifecyclePhase::Running | LifecyclePhase::Starting | LifecyclePhase::Resuming => {
                begin_transition(state, LifecyclePhase::Pausing)
            }
            LifecyclePhase::Waking => {
                state.wake_target = LifecyclePhase::Paused;
                begin_transition(state, LifecyclePhase::Waking)
            }
            LifecyclePhase::Pausing | LifecyclePhase::Paused => Ok(false),
            _ => Err(invalid_transition(state.phase, "pause")),
        })
    }

    /// Requests an acknowledged transition from a manual pause to normal running.
    pub fn request_resume(&self) -> Result<LifecycleSnapshot> {
        self.update("request_resume", |state| match state.phase {
            LifecyclePhase::Paused | LifecyclePhase::Pausing => {
                begin_transition(state, LifecyclePhase::Resuming)
            }
            LifecyclePhase::Waking => {
                state.wake_target = LifecyclePhase::Running;
                begin_transition(state, LifecyclePhase::Waking)
            }
            LifecyclePhase::Running | LifecyclePhase::Resuming => Ok(false),
            _ => Err(invalid_transition(state.phase, "resume")),
        })
    }

    /// Requests fast, acknowledged preparation for system sleep.
    ///
    /// The method only records intent and publishes the new signal. It never waits for subsystem
    /// acknowledgements, so a platform power callback can return immediately.
    pub fn request_sleep(&self) -> Result<LifecycleSnapshot> {
        self.update("request_sleep", |state| match state.phase {
            LifecyclePhase::Starting | LifecyclePhase::Running | LifecyclePhase::Resuming => {
                state.wake_target = LifecyclePhase::Running;
                begin_transition(state, LifecyclePhase::PreparingSleep)
            }
            LifecyclePhase::Pausing | LifecyclePhase::Paused => {
                state.wake_target = LifecyclePhase::Paused;
                begin_transition(state, LifecyclePhase::PreparingSleep)
            }
            LifecyclePhase::Waking => begin_transition(state, LifecyclePhase::PreparingSleep),
            LifecyclePhase::PreparingSleep | LifecyclePhase::Sleeping => Ok(false),
            _ => Err(invalid_transition(state.phase, "sleep")),
        })
    }

    /// Requests acknowledged recovery after system wake.
    ///
    /// A critical wake received without a preceding normal sleep still revalidates participant
    /// resources. Repeated notifications are idempotent while that recovery remains in progress.
    pub fn request_wake(&self) -> Result<LifecycleSnapshot> {
        self.update("request_wake", |state| match state.phase {
            LifecyclePhase::PreparingSleep | LifecyclePhase::Sleeping => {
                begin_transition(state, LifecyclePhase::Waking)
            }
            LifecyclePhase::Starting | LifecyclePhase::Running | LifecyclePhase::Resuming => {
                state.wake_target = LifecyclePhase::Running;
                begin_transition(state, LifecyclePhase::Waking)
            }
            LifecyclePhase::Pausing | LifecyclePhase::Paused => {
                state.wake_target = LifecyclePhase::Paused;
                begin_transition(state, LifecyclePhase::Waking)
            }
            LifecyclePhase::Waking => Ok(false),
            _ => Err(invalid_transition(state.phase, "wake")),
        })
    }

    /// Requests orderly teardown from any live or failed phase.
    pub fn request_shutdown(&self) -> Result<LifecycleSnapshot> {
        self.update("request_shutdown", |state| match state.phase {
            LifecyclePhase::Stopping | LifecyclePhase::Stopped => Ok(false),
            _ => begin_transition(state, LifecyclePhase::Stopping),
        })
    }

    /// Starts a fresh lifecycle after every participant completed teardown.
    pub fn request_restart(&self) -> Result<LifecycleSnapshot> {
        self.update("request_restart", |state| {
            if state.phase != LifecyclePhase::Stopped {
                return Err(invalid_transition(state.phase, "restart"));
            }
            state.failure = None;
            state.wake_target = LifecyclePhase::Running;
            begin_transition(state, LifecyclePhase::Starting)
        })
    }

    /// Waits until the transition revision changes or the monotonic timeout elapses.
    ///
    /// This is available only to callers outside a Superi domain or inside a domain whose policy
    /// permits blocking. Latency-sensitive domains must retain and poll [`Self::signal`] instead.
    pub fn wait_for_change(
        &self,
        after_revision: u64,
        timeout: Duration,
    ) -> Result<Option<LifecycleSnapshot>> {
        if let Some(domain) = current_execution_domain() {
            if !domain.policy().allows_blocking() {
                return Err(lifecycle_error(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "current execution domain cannot block on lifecycle changes",
                    "wait_for_change",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "blocking_domain")
                        .with_field("domain", domain.code()),
                ));
            }
        }

        let state = lock_unpoisoned(&self.shared.state);
        if state.revision != after_revision {
            return Ok(Some(LifecycleSnapshot::from_state(&state)));
        }
        let (state, wait) = self
            .shared
            .changed
            .wait_timeout_while(state, timeout, |state| state.revision == after_revision)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if wait.timed_out() && state.revision == after_revision {
            return Ok(None);
        }
        Ok(Some(LifecycleSnapshot::from_state(&state)))
    }

    fn acknowledge(
        &self,
        participant: &str,
        observed: LifecycleSignalSnapshot,
    ) -> Result<LifecycleSnapshot> {
        self.update("acknowledge", |state| {
            if state.phase != observed.phase || state.revision != observed.revision {
                return Err(lifecycle_error(
                    ErrorCategory::Conflict,
                    Recoverability::Retryable,
                    "lifecycle acknowledgement refers to a stale transition",
                    "acknowledge",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "stale_acknowledgement")
                        .with_field("participant", participant)
                        .with_field("observed_phase", observed.phase.code())
                        .with_field("observed_revision", observed.revision.to_string())
                        .with_field("current_phase", state.phase.code())
                        .with_field("current_revision", state.revision.to_string()),
                ));
            }
            if !state.phase.requires_acknowledgement() {
                return Err(invalid_transition(state.phase, "acknowledge"));
            }
            if !state.pending.remove(participant) {
                return Ok(false);
            }
            if state.pending.is_empty() {
                complete_transition(state)?;
                return Ok(true);
            }
            Ok(false)
        })
    }

    fn fail(&self, participant: &str, error: Error) -> Result<LifecycleSnapshot> {
        self.update("fail", |state| {
            if state.phase == LifecyclePhase::Stopped {
                return Err(invalid_transition(state.phase, "fail"));
            }
            advance_revision(state)?;
            state.phase = LifecyclePhase::Failed;
            state.pending.clear();
            state.failure = Some(LifecycleFailure {
                participant: participant.to_owned(),
                error: Arc::new(error),
            });
            Ok(true)
        })
    }

    fn update<F>(&self, operation: &'static str, update: F) -> Result<LifecycleSnapshot>
    where
        F: FnOnce(&mut LifecycleState) -> Result<bool>,
    {
        let mut state = lock_unpoisoned(&self.shared.state);
        let changed = update(&mut state).map_err(|mut error| {
            error.push_context(ErrorContext::new(COMPONENT, operation));
            error
        })?;
        let snapshot = LifecycleSnapshot::from_state(&state);
        if changed {
            self.shared.signal.publish(state.phase, state.revision);
            self.shared.changed.notify_all();
        }
        Ok(snapshot)
    }
}

impl fmt::Debug for LifecycleCoordinator {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("LifecycleCoordinator")
            .field(&self.snapshot())
            .finish()
    }
}

/// One registered subsystem's acknowledgement and failure handle.
pub struct LifecycleParticipant {
    name: String,
    coordinator: LifecycleCoordinator,
    signal: LifecycleSignal,
}

impl LifecycleParticipant {
    /// Returns the stable participant name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns a retained lock-free lifecycle signal.
    #[must_use]
    pub fn signal(&self) -> LifecycleSignal {
        self.signal.clone()
    }

    /// Acknowledges the exact transitional phase and revision the participant completed.
    ///
    /// Repeated acknowledgement is harmless while other participants remain pending. A token from
    /// an older transition is rejected so late work cannot complete a newer request accidentally.
    pub fn acknowledge(&self, observed: LifecycleSignalSnapshot) -> Result<LifecycleSnapshot> {
        self.coordinator.acknowledge(&self.name, observed)
    }

    /// Reports a classified failure and stops normal lifecycle progress.
    pub fn fail(&self, error: Error) -> Result<LifecycleSnapshot> {
        self.coordinator.fail(&self.name, error)
    }
}

impl fmt::Debug for LifecycleParticipant {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LifecycleParticipant")
            .field("name", &self.name)
            .field("signal", &self.signal)
            .finish_non_exhaustive()
    }
}

fn begin_transition(state: &mut LifecycleState, phase: LifecyclePhase) -> Result<bool> {
    debug_assert!(phase.requires_acknowledgement());
    advance_revision(state)?;
    state.phase = phase;
    state.pending.clone_from(&state.participants);
    if state.pending.is_empty() {
        complete_transition(state)?;
    }
    Ok(true)
}

fn complete_transition(state: &mut LifecycleState) -> Result<()> {
    let completed = match state.phase {
        LifecyclePhase::Starting | LifecyclePhase::Resuming => LifecyclePhase::Running,
        LifecyclePhase::Pausing => LifecyclePhase::Paused,
        LifecyclePhase::PreparingSleep => LifecyclePhase::Sleeping,
        LifecyclePhase::Waking => state.wake_target,
        LifecyclePhase::Stopping => LifecyclePhase::Stopped,
        _ => return Err(invalid_transition(state.phase, "complete")),
    };
    advance_revision(state)?;
    state.phase = completed;
    state.pending.clear();
    Ok(())
}

fn advance_revision(state: &mut LifecycleState) -> Result<()> {
    state.revision = state
        .revision
        .checked_add(1)
        .filter(|revision| *revision <= MAX_REVISION)
        .ok_or_else(|| {
            lifecycle_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "lifecycle transition revision space is exhausted",
                "advance_revision",
            )
        })?;
    Ok(())
}

fn encode_signal(phase: LifecyclePhase, revision: u64) -> u64 {
    debug_assert!(revision <= MAX_REVISION);
    (revision << PHASE_BITS) | u64::from(phase as u8)
}

fn decode_signal(encoded: u64) -> LifecycleSignalSnapshot {
    LifecycleSignalSnapshot {
        phase: LifecyclePhase::from_discriminant((encoded & PHASE_MASK) as u8),
        revision: encoded >> PHASE_BITS,
    }
}

fn invalid_transition(current: LifecyclePhase, requested: &'static str) -> Error {
    lifecycle_error(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        "lifecycle request conflicts with the current phase",
        "transition",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "phase_conflict")
            .with_field("current", current.code())
            .with_field("requested", requested),
    )
}

fn lifecycle_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}
