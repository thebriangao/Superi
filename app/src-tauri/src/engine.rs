//! Long-lived headless-engine ownership behind the desktop application connection.

use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::time::Duration;

use superi_api::commands::{GetEngineIntegrationValidation, GetEngineIntegrationValidationResult};
use superi_api::validation::IntegrationValidationApi;
use superi_concurrency::lifecycle::LifecyclePhase;
use superi_concurrency::threads::{ExecutionDomain, ExecutionDomainThread};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult,
    EngineTransactionId,
};
use superi_engine::introspection::MediaCapabilities;
use superi_engine::media::media_backend_registry;

use crate::lifecycle::{
    ApplicationLifecycle, ApplicationLifecyclePhase, HeadlessEngineFailure,
    HeadlessEngineLifecycleParticipant, LifecycleIntent,
};

const COMPONENT: &str = "superi-desktop.engine";
const REQUEST_CAPACITY: usize = 8;
const CONTROL_WAIT: Duration = Duration::from_millis(10);

enum EngineRequest {
    IntegrationValidation {
        response: SyncSender<Result<GetEngineIntegrationValidationResult>>,
    },
}

/// Cloneable, transport-neutral handle to the engine process owned by the Tauri shell.
#[derive(Clone)]
pub struct EngineConnection {
    requests: SyncSender<EngineRequest>,
}

impl EngineConnection {
    /// Admits one existing integration-validation query without waiting for queue capacity.
    pub fn request_integration_validation(&self) -> Result<PendingEngineValidation> {
        let (response, receiver) = sync_channel(1);
        self.requests
            .try_send(EngineRequest::IntegrationValidation { response })
            .map_err(connection_admission_error)?;
        Ok(PendingEngineValidation { receiver })
    }
}

/// Blocking-safe observation of one previously admitted engine validation query.
#[must_use = "the admitted engine response must be observed on a blocking-safe thread"]
pub struct PendingEngineValidation {
    receiver: Receiver<Result<GetEngineIntegrationValidationResult>>,
}

impl PendingEngineValidation {
    /// Waits for the typed result on a blocking-safe caller.
    pub fn wait(self, timeout: Duration) -> Result<GetEngineIntegrationValidationResult> {
        match self.receiver.recv_timeout(timeout) {
            Ok(result) => result,
            Err(RecvTimeoutError::Timeout) => Err(engine_error(
                ErrorCategory::Timeout,
                Recoverability::Retryable,
                "timed out waiting for the headless engine response",
                "wait_for_response",
            )),
            Err(RecvTimeoutError::Disconnected) => Err(engine_error(
                ErrorCategory::Unavailable,
                Recoverability::Terminal,
                "the headless engine response channel closed",
                "wait_for_response",
            )),
        }
    }
}

/// Explicit owner of one managed EngineControl thread for the desktop process.
#[must_use = "retain the linked engine owner until application shutdown is complete"]
pub struct LinkedEngineProcess {
    connection: EngineConnection,
    worker: ExecutionDomainThread<()>,
}

impl LinkedEngineProcess {
    /// Links one lifecycle-attached dispatcher to the exact C001 participant seam.
    pub fn launch(lifecycle: ApplicationLifecycle) -> Result<Self> {
        let participant = lifecycle.headless_engine_participant()?;
        let (requests, receiver) = sync_channel(REQUEST_CAPACITY);
        let worker = ExecutionDomain::EngineControl
            .spawn(move |_| run_engine_control(participant, receiver))?;
        Ok(Self {
            connection: EngineConnection { requests },
            worker,
        })
    }

    /// Returns the stable shell-owned connection retained across engine generations.
    #[must_use]
    pub fn connection(&self) -> EngineConnection {
        self.connection.clone()
    }

    /// Returns the exact managed EngineControl thread name.
    #[must_use]
    pub fn thread_name(&self) -> &str {
        self.worker.thread_name()
    }

    /// Joins the engine owner after application lifecycle shutdown has completed.
    pub fn join(self) -> Result<()> {
        self.worker.join()
    }
}

fn run_engine_control(
    participant: HeadlessEngineLifecycleParticipant,
    requests: Receiver<EngineRequest>,
) -> Result<()> {
    let signal = participant.signal();
    let mut dispatcher = None;

    loop {
        let observed = signal.load();
        match observed.phase() {
            LifecyclePhase::Starting => {
                if dispatcher.is_none() {
                    match EngineCommandDispatcher::new() {
                        Ok(owner) => dispatcher = Some(owner),
                        Err(error) => {
                            participant.fail(lifecycle_failure(&error, "link"))?;
                            continue;
                        }
                    }
                }
                match participant.acknowledge(observed) {
                    Ok(_) => {}
                    Err(_error) if signal.load() != observed => continue,
                    Err(error) => return Err(error),
                }
            }
            LifecyclePhase::Stopping => {
                if let Some(mut owner) = dispatcher.take() {
                    if let Err(error) = stop_dispatcher(&mut owner) {
                        participant.fail(lifecycle_failure(&error, "shutdown"))?;
                        continue;
                    }
                }
                let snapshot = match participant.acknowledge(observed) {
                    Ok(snapshot) => snapshot,
                    Err(_error) if signal.load() != observed => continue,
                    Err(error) => return Err(error),
                };
                if snapshot.application_phase() == ApplicationLifecyclePhase::Stopped
                    && snapshot.intent() == LifecycleIntent::Shutdown
                {
                    return Ok(());
                }
                continue;
            }
            LifecyclePhase::Stopped => return Ok(()),
            _ => {}
        }

        match requests.recv_timeout(CONTROL_WAIT) {
            Ok(request) => execute_request(request, dispatcher.as_ref()),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                let error = engine_error(
                    ErrorCategory::Unavailable,
                    Recoverability::Terminal,
                    "the desktop engine connection owner was dropped",
                    "receive_request",
                );
                participant.fail(lifecycle_failure(&error, "connection"))?;
                return Err(error);
            }
        }
    }
}

fn execute_request(request: EngineRequest, dispatcher: Option<&EngineCommandDispatcher>) {
    match request {
        EngineRequest::IntegrationValidation { response } => {
            let result = dispatcher
                .ok_or_else(|| {
                    engine_error(
                        ErrorCategory::Unavailable,
                        Recoverability::Retryable,
                        "the headless engine is not connected",
                        "integration_validation",
                    )
                })
                .and_then(integration_validation);
            let _ = response.try_send(result);
        }
    }
}

fn integration_validation(
    dispatcher: &EngineCommandDispatcher,
) -> Result<GetEngineIntegrationValidationResult> {
    let capabilities = MediaCapabilities::from_registry(&media_backend_registry()?)?;
    let snapshot = dispatcher.integration_validation_snapshot(&capabilities, None)?;
    Ok(IntegrationValidationApi::new(&snapshot).execute(GetEngineIntegrationValidation::new()))
}

fn stop_dispatcher(dispatcher: &mut EngineCommandDispatcher) -> Result<()> {
    let outcome = dispatcher.dispatch(EngineCommandRequest::new(
        EngineTransactionId::new("superi-desktop.engine.shutdown")?,
        EngineCommand::BeginShutdown,
    ))?;
    let EngineCommandResult::Lifecycle(snapshot) = outcome.result() else {
        return Err(engine_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "headless engine shutdown returned an unexpected result",
            "shutdown",
        ));
    };
    if snapshot.phase() != LifecyclePhase::Stopped {
        return Err(engine_error(
            ErrorCategory::Conflict,
            Recoverability::Terminal,
            "headless engine shutdown still requires a subsystem owner",
            "shutdown",
        ));
    }
    Ok(())
}

fn connection_admission_error(error: TrySendError<EngineRequest>) -> Error {
    match error {
        TrySendError::Full(_) => engine_error(
            ErrorCategory::ResourceExhausted,
            Recoverability::Retryable,
            "the headless engine request queue is full",
            "admit_request",
        ),
        TrySendError::Disconnected(_) => engine_error(
            ErrorCategory::Unavailable,
            Recoverability::Terminal,
            "the headless engine connection is closed",
            "admit_request",
        ),
    }
}

fn lifecycle_failure(error: &Error, operation: &'static str) -> HeadlessEngineFailure {
    HeadlessEngineFailure::new(error.category(), error.recoverability(), error.message())
        .with_context(COMPONENT, operation)
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
