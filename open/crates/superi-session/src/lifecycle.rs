//! Application-owned lifecycle coordination independent of presentation host.
//!
//! The operating-system main thread records intent and returns immediately. One headless-engine
//! participant observes exact revision-scoped signals, performs engine-side work on its own legal
//! execution domain, and acknowledges the observed transition. Application state never claims the
//! engine is ready until that acknowledgement arrives.

use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use superi_concurrency::lifecycle::{
    LifecycleCoordinator, LifecycleParticipant, LifecyclePhase, LifecycleSignal,
    LifecycleSignalSnapshot, LifecycleSnapshot,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

const COMPONENT: &str = "superi-desktop.lifecycle";
const HEADLESS_ENGINE_PARTICIPANT: &str = "headless-engine";
const MAX_FAILURE_SUMMARY_BYTES: usize = 1_024;
const MAX_CONTEXT_VALUE_BYTES: usize = 128;

/// The application-visible phase of the native desktop process.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ApplicationLifecyclePhase {
    /// The native shell is waiting for the headless engine to become ready.
    Starting,
    /// The native shell and headless engine are ready for client work.
    Running,
    /// The shell is quiescing for a platform suspension.
    Suspending,
    /// The shell is suspended with no new engine work admitted.
    Suspended,
    /// The shell is reacquiring resources after suspension.
    Resuming,
    /// The shell is waiting for orderly engine teardown before process exit.
    Stopping,
    /// The shell remains responsive while the engine stops and starts in a new generation.
    Restarting,
    /// The shell remains responsive while a failed engine is replaced in a new generation.
    Recovering,
    /// A classified engine or lifecycle failure requires retry, restart, correction, or exit.
    Failed,
    /// The headless engine acknowledged teardown and the application may exit.
    Stopped,
}

/// The application-visible phase of the headless engine connection.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EngineLifecyclePhase {
    /// The engine participant is acquiring or validating a connection.
    Starting,
    /// The engine participant acknowledged readiness in this generation.
    Ready,
    /// The engine participant is quiescing for platform suspension.
    Suspending,
    /// The engine participant is suspended.
    Suspended,
    /// The engine participant is reacquiring resources.
    Resuming,
    /// The engine participant is releasing its connection and owned process resources.
    Stopping,
    /// The engine participant reported a classified failure.
    Failed,
    /// The engine participant acknowledged complete teardown.
    Stopped,
}

/// The current application-owned lifecycle goal.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum LifecycleIntent {
    /// Complete the first engine startup.
    Start,
    /// Continue normal operation.
    Run,
    /// Stop the engine and then exit the application.
    Shutdown,
    /// Stop and start the engine in a fresh generation.
    Restart,
    /// Replace a failed engine with a fresh generation.
    Recover,
}

/// A session-local lifecycle request. Project and editor operations use the public engine API.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ApplicationLifecycleRequest {
    /// Retry a recoverable failed engine in a fresh generation.
    Recover,
    /// Orderly stop and start the engine in a fresh generation.
    Restart,
    /// Orderly stop the engine before application exit.
    Shutdown,
}

/// One user-safe failure context frame.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopLifecycleFailureContext {
    component: String,
    operation: String,
}

impl DesktopLifecycleFailureContext {
    /// Returns the stable component code.
    #[must_use]
    pub fn component(&self) -> &str {
        &self.component
    }

    /// Returns the stable operation code.
    #[must_use]
    pub fn operation(&self) -> &str {
        &self.operation
    }
}

/// User-safe classified failure retained by the application lifecycle.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopLifecycleFailure {
    category: String,
    recoverability: String,
    summary: String,
    contexts: Vec<DesktopLifecycleFailureContext>,
}

impl DesktopLifecycleFailure {
    /// Returns the stable shared error category.
    #[must_use]
    pub fn category(&self) -> &str {
        &self.category
    }

    /// Returns the stable recovery classification.
    #[must_use]
    pub fn recoverability(&self) -> &str {
        &self.recoverability
    }

    /// Returns the reviewed, user-safe summary.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Returns user-safe component and operation context in failure order.
    #[must_use]
    pub fn contexts(&self) -> &[DesktopLifecycleFailureContext] {
        &self.contexts
    }
}

/// One immutable application and engine lifecycle view for the desktop client.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopLifecycleSnapshot {
    revision: u64,
    engine_generation: u64,
    application_phase: ApplicationLifecyclePhase,
    engine_phase: EngineLifecyclePhase,
    intent: LifecycleIntent,
    engine_acknowledgement_pending: bool,
    failure: Option<DesktopLifecycleFailure>,
    can_retry: bool,
    can_restart: bool,
    can_shutdown: bool,
}

impl DesktopLifecycleSnapshot {
    /// Returns the shared lifecycle transition revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the monotonic engine generation, beginning at one.
    #[must_use]
    pub const fn engine_generation(&self) -> u64 {
        self.engine_generation
    }

    /// Returns the explicit native application phase.
    #[must_use]
    pub const fn application_phase(&self) -> ApplicationLifecyclePhase {
        self.application_phase
    }

    /// Returns the explicit headless-engine phase.
    #[must_use]
    pub const fn engine_phase(&self) -> EngineLifecyclePhase {
        self.engine_phase
    }

    /// Returns the current application-owned goal.
    #[must_use]
    pub const fn intent(&self) -> LifecycleIntent {
        self.intent
    }

    /// Returns whether the headless-engine owner still owes the exact transition acknowledgement.
    #[must_use]
    pub const fn engine_acknowledgement_pending(&self) -> bool {
        self.engine_acknowledgement_pending
    }

    /// Returns retained user-safe failure evidence.
    #[must_use]
    pub const fn failure(&self) -> Option<&DesktopLifecycleFailure> {
        self.failure.as_ref()
    }

    /// Returns whether the same failed operation may be retried in a new generation.
    #[must_use]
    pub const fn can_retry(&self) -> bool {
        self.can_retry
    }

    /// Returns whether an orderly full engine restart may be requested.
    #[must_use]
    pub const fn can_restart(&self) -> bool {
        self.can_restart
    }

    /// Returns whether orderly application shutdown may be requested.
    #[must_use]
    pub const fn can_shutdown(&self) -> bool {
        self.can_shutdown
    }
}

/// A reviewed failure supplied by the headless-engine process owner.
pub struct HeadlessEngineFailure {
    category: ErrorCategory,
    recoverability: Recoverability,
    summary: String,
    context: Option<DesktopLifecycleFailureContext>,
}

impl HeadlessEngineFailure {
    /// Creates user-safe classified failure evidence.
    pub fn new(
        category: ErrorCategory,
        recoverability: Recoverability,
        summary: impl Into<String>,
    ) -> Self {
        Self {
            category,
            recoverability,
            summary: summary.into(),
            context: None,
        }
    }

    /// Adds one bounded user-safe component and operation frame.
    #[must_use]
    pub fn with_context(
        mut self,
        component: impl Into<String>,
        operation: impl Into<String>,
    ) -> Self {
        self.context = Some(DesktopLifecycleFailureContext {
            component: component.into(),
            operation: operation.into(),
        });
        self
    }

    fn into_error(self) -> Result<Error> {
        validate_failure_value("summary", &self.summary, MAX_FAILURE_SUMMARY_BYTES)?;
        let mut error = Error::new(self.category, self.recoverability, self.summary);
        if let Some(context) = self.context {
            validate_failure_value("component", &context.component, MAX_CONTEXT_VALUE_BYTES)?;
            validate_failure_value("operation", &context.operation, MAX_CONTEXT_VALUE_BYTES)?;
            error.push_context(ErrorContext::new(context.component, context.operation));
        }
        Ok(error)
    }
}

#[derive(Clone, Copy)]
struct LifecycleOwnerState {
    intent: LifecycleIntent,
    engine_generation: u64,
}

struct ApplicationLifecycleInner {
    coordinator: LifecycleCoordinator,
    owner: Mutex<LifecycleOwnerState>,
}

/// Cloneable application owner for one long-lived desktop lifecycle.
#[derive(Clone)]
pub struct ApplicationLifecycle {
    inner: Arc<ApplicationLifecycleInner>,
}

impl ApplicationLifecycle {
    /// Creates one lifecycle waiting for the headless-engine startup acknowledgement.
    pub fn new() -> Result<Self> {
        Ok(Self {
            inner: Arc::new(ApplicationLifecycleInner {
                coordinator: LifecycleCoordinator::new([HEADLESS_ENGINE_PARTICIPANT])?,
                owner: Mutex::new(LifecycleOwnerState {
                    intent: LifecycleIntent::Start,
                    engine_generation: 1,
                }),
            }),
        })
    }

    /// Returns a handle used only by the headless-engine process owner.
    pub fn headless_engine_participant(&self) -> Result<HeadlessEngineLifecycleParticipant> {
        Ok(HeadlessEngineLifecycleParticipant {
            participant: self
                .inner
                .coordinator
                .participant(HEADLESS_ENGINE_PARTICIPANT)?,
            lifecycle: self.clone(),
        })
    }

    /// Returns the current immutable application and engine state.
    #[must_use]
    pub fn snapshot(&self) -> DesktopLifecycleSnapshot {
        let owner = *lock_unpoisoned(&self.inner.owner);
        project_snapshot(&self.inner.coordinator.snapshot(), owner)
    }

    /// Requests orderly engine teardown before application exit.
    pub fn request_shutdown(&self) -> Result<DesktopLifecycleSnapshot> {
        self.request_intent(LifecycleIntent::Shutdown)
    }

    /// Requests orderly engine teardown followed by a fresh engine generation.
    pub fn request_restart(&self) -> Result<DesktopLifecycleSnapshot> {
        self.request_intent(LifecycleIntent::Restart)
    }

    /// Retries a nonterminal failed engine in a fresh generation.
    pub fn request_recovery(&self) -> Result<DesktopLifecycleSnapshot> {
        let snapshot = self.inner.coordinator.snapshot();
        let Some(failure) = snapshot.failure() else {
            return Err(lifecycle_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "engine recovery requires a classified lifecycle failure",
                "request_recovery",
            ));
        };
        if snapshot.phase() != LifecyclePhase::Failed
            || failure.error().recoverability() == Recoverability::Terminal
        {
            return Err(lifecycle_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "the current lifecycle failure cannot be retried",
                "request_recovery",
            ));
        }
        self.request_intent(LifecycleIntent::Recover)
    }

    /// Waits on a blocking-safe thread for a newer shared lifecycle revision.
    pub fn wait_for_change(
        &self,
        after_revision: u64,
        timeout: Duration,
    ) -> Result<Option<DesktopLifecycleSnapshot>> {
        let Some(snapshot) = self
            .inner
            .coordinator
            .wait_for_change(after_revision, timeout)?
        else {
            return Ok(None);
        };
        let owner = *lock_unpoisoned(&self.inner.owner);
        Ok(Some(project_snapshot(&snapshot, owner)))
    }

    fn request_intent(&self, intent: LifecycleIntent) -> Result<DesktopLifecycleSnapshot> {
        let mut owner = lock_unpoisoned(&self.inner.owner);
        let previous = *owner;
        owner.intent = intent;
        let phase = self.inner.coordinator.snapshot().phase();
        let transition = match intent {
            LifecycleIntent::Shutdown => self.inner.coordinator.request_shutdown(),
            LifecycleIntent::Restart | LifecycleIntent::Recover => {
                if phase == LifecyclePhase::Stopped {
                    owner.engine_generation = next_generation(owner.engine_generation)?;
                    self.inner.coordinator.request_restart()
                } else {
                    self.inner.coordinator.request_shutdown()
                }
            }
            LifecycleIntent::Start | LifecycleIntent::Run => Err(lifecycle_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "start and run intents are lifecycle-owned states, not client requests",
                "request_intent",
            )),
        };
        match transition {
            Ok(snapshot) => Ok(project_snapshot(&snapshot, *owner)),
            Err(error) => {
                *owner = previous;
                Err(error)
            }
        }
    }

    fn after_engine_acknowledgement(
        &self,
        mut snapshot: LifecycleSnapshot,
    ) -> Result<DesktopLifecycleSnapshot> {
        let mut owner = lock_unpoisoned(&self.inner.owner);
        if snapshot.phase() == LifecyclePhase::Stopped
            && matches!(
                owner.intent,
                LifecycleIntent::Restart | LifecycleIntent::Recover
            )
        {
            owner.engine_generation = next_generation(owner.engine_generation)?;
            snapshot = self.inner.coordinator.request_restart()?;
        }
        if snapshot.phase() == LifecyclePhase::Running {
            owner.intent = LifecycleIntent::Run;
        }
        Ok(project_snapshot(&snapshot, *owner))
    }
}

/// Exact lifecycle acknowledgement and failure handle for the C002 process owner.
pub struct HeadlessEngineLifecycleParticipant {
    participant: LifecycleParticipant,
    lifecycle: ApplicationLifecycle,
}

impl HeadlessEngineLifecycleParticipant {
    /// Returns the retained lock-free transition signal.
    #[must_use]
    pub fn signal(&self) -> LifecycleSignal {
        self.participant.signal()
    }

    /// Acknowledges the exact transition observed before the engine-side work began.
    pub fn acknowledge(
        &self,
        observed: LifecycleSignalSnapshot,
    ) -> Result<DesktopLifecycleSnapshot> {
        let snapshot = self.participant.acknowledge(observed)?;
        self.lifecycle.after_engine_acknowledgement(snapshot)
    }

    /// Reports reviewed classified engine failure evidence.
    pub fn fail(&self, failure: HeadlessEngineFailure) -> Result<DesktopLifecycleSnapshot> {
        let snapshot = self.participant.fail(failure.into_error()?)?;
        let owner = *lock_unpoisoned(&self.lifecycle.inner.owner);
        Ok(project_snapshot(&snapshot, owner))
    }
}

fn project_snapshot(
    snapshot: &LifecycleSnapshot,
    owner: LifecycleOwnerState,
) -> DesktopLifecycleSnapshot {
    let failure = snapshot.failure().map(|failure| DesktopLifecycleFailure {
        category: failure.error().category().code().to_owned(),
        recoverability: failure.error().recoverability().code().to_owned(),
        summary: failure.error().message().to_owned(),
        contexts: failure
            .error()
            .contexts()
            .iter()
            .map(|context| DesktopLifecycleFailureContext {
                component: context.component().to_owned(),
                operation: context.operation().to_owned(),
            })
            .collect(),
    });
    let (application_phase, engine_phase) = visible_phases(snapshot.phase(), owner.intent);
    let can_retry = snapshot
        .failure()
        .is_some_and(|failure| failure.error().recoverability() != Recoverability::Terminal);
    DesktopLifecycleSnapshot {
        revision: snapshot.revision(),
        engine_generation: owner.engine_generation,
        application_phase,
        engine_phase,
        intent: owner.intent,
        engine_acknowledgement_pending: snapshot
            .pending_participants()
            .iter()
            .any(|participant| participant == HEADLESS_ENGINE_PARTICIPANT),
        failure,
        can_retry,
        can_restart: snapshot.phase() != LifecyclePhase::Stopping,
        can_shutdown: snapshot.phase() != LifecyclePhase::Stopped
            && (owner.intent != LifecycleIntent::Shutdown
                || snapshot.phase() == LifecyclePhase::Failed),
    }
}

fn visible_phases(
    phase: LifecyclePhase,
    intent: LifecycleIntent,
) -> (ApplicationLifecyclePhase, EngineLifecyclePhase) {
    match phase {
        LifecyclePhase::Starting => (
            match intent {
                LifecycleIntent::Restart => ApplicationLifecyclePhase::Restarting,
                LifecycleIntent::Recover => ApplicationLifecyclePhase::Recovering,
                _ => ApplicationLifecyclePhase::Starting,
            },
            EngineLifecyclePhase::Starting,
        ),
        LifecyclePhase::Running => (
            ApplicationLifecyclePhase::Running,
            EngineLifecyclePhase::Ready,
        ),
        LifecyclePhase::Pausing | LifecyclePhase::PreparingSleep => (
            ApplicationLifecyclePhase::Suspending,
            EngineLifecyclePhase::Suspending,
        ),
        LifecyclePhase::Paused | LifecyclePhase::Sleeping => (
            ApplicationLifecyclePhase::Suspended,
            EngineLifecyclePhase::Suspended,
        ),
        LifecyclePhase::Resuming | LifecyclePhase::Waking => (
            ApplicationLifecyclePhase::Resuming,
            EngineLifecyclePhase::Resuming,
        ),
        LifecyclePhase::Stopping => (
            match intent {
                LifecycleIntent::Restart => ApplicationLifecyclePhase::Restarting,
                LifecycleIntent::Recover => ApplicationLifecyclePhase::Recovering,
                _ => ApplicationLifecyclePhase::Stopping,
            },
            EngineLifecyclePhase::Stopping,
        ),
        LifecyclePhase::Stopped => (
            match intent {
                LifecycleIntent::Restart => ApplicationLifecyclePhase::Restarting,
                LifecycleIntent::Recover => ApplicationLifecyclePhase::Recovering,
                _ => ApplicationLifecyclePhase::Stopped,
            },
            EngineLifecyclePhase::Stopped,
        ),
        LifecyclePhase::Failed => (
            ApplicationLifecyclePhase::Failed,
            EngineLifecyclePhase::Failed,
        ),
        _ => (
            ApplicationLifecyclePhase::Failed,
            EngineLifecyclePhase::Failed,
        ),
    }
}

fn next_generation(current: u64) -> Result<u64> {
    current.checked_add(1).ok_or_else(|| {
        lifecycle_error(
            ErrorCategory::ResourceExhausted,
            Recoverability::Terminal,
            "desktop engine generation space is exhausted",
            "next_generation",
        )
    })
}

fn validate_failure_value(field: &'static str, value: &str, maximum: usize) -> Result<()> {
    if value.is_empty() || value.trim() != value || value.len() > maximum {
        return Err(lifecycle_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "headless engine failure text is empty, untrimmed, or oversized",
            "validate_failure",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "failure_text")
                .with_field("field", field)
                .with_field("maximum_bytes", maximum.to_string()),
        ));
    }
    Ok(())
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
