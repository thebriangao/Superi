//! Long-lived headless-engine ownership behind the desktop application connection.

use std::path::Path;
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::sync::Arc;
use std::time::{Duration, Instant};

use superi_api::commands::{
    GetEditorState, GetEditorStateResult, GetEngineIntegrationValidation,
    GetEngineIntegrationValidationResult,
};
use superi_api::editor::{ExecuteProjectCommand, ExecuteProjectCommandResult, ProjectEditorApi};
use superi_api::events::ProjectStateChanged;
use superi_api::playback::{ExecutePlaybackTransport, ExecutePlaybackTransportResult};
use superi_api::validation::IntegrationValidationApi;
use superi_concurrency::lifecycle::LifecyclePhase;
use superi_concurrency::jobs::{BoundedWorkerPool, WorkerPoolConfig};
use superi_concurrency::threads::{ExecutionDomain, ExecutionDomainThread};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult,
    EngineTransactionId,
};
use superi_engine::editor::ProjectDatabase;
use superi_engine::introspection::MediaCapabilities;
use superi_engine::media::media_backend_registry;
use superi_engine::playback_runtime::PlaybackControlRuntime;
use superi_engine::dispatcher::PlaybackCommandExecutor;

use crate::lifecycle::{
    ApplicationLifecycle, ApplicationLifecyclePhase, HeadlessEngineFailure,
    HeadlessEngineLifecycleParticipant, LifecycleIntent,
};
use crate::process_runtime::{
    DesktopProcessRuntime, DesktopProcessServiceId, DesktopProcessServicePhase,
};
use crate::project_lifecycle::{DesktopProjectEditorRoute, DesktopProjectIdentity};

const COMPONENT: &str = "superi-desktop.engine";
const REQUEST_CAPACITY: usize = 8;
const CONTROL_WAIT: Duration = Duration::from_millis(10);
const PLAYBACK_WAIT: Duration = Duration::from_millis(2);
const PLAYBACK_CONTROL_TIMEOUT: Duration = Duration::from_secs(2);
const PLAYBACK_CONTROL_CAPACITY: usize = 2;
const PLAYBACK_WORK_QUEUE_CAPACITY: usize = 64;
const PLAYBACK_JOB_NAMESPACE: u64 = 0x5355_5045_5249_5042;

enum EngineRequest {
    IntegrationValidation {
        response: SyncSender<Result<GetEngineIntegrationValidationResult>>,
    },
    EditorState {
        route: DesktopProjectEditorRoute,
        request: GetEditorState,
        response: SyncSender<Result<GetEditorStateResult>>,
    },
    ProjectCommand {
        route: DesktopProjectEditorRoute,
        request: ExecuteProjectCommand,
        response: SyncSender<Result<ProjectEditorCommandOutput>>,
    },
    PlaybackTransport {
        route: DesktopProjectEditorRoute,
        request: ExecutePlaybackTransport,
        response: SyncSender<Result<ExecutePlaybackTransportResult>>,
    },
}

enum PlaybackOwnerRequest {
    Attach {
        executor: PlaybackCommandExecutor,
        bounds: superi_engine::editor::TimeRange,
        response: SyncSender<Result<()>>,
    },
    Reconfigure {
        bounds: superi_engine::editor::TimeRange,
        response: SyncSender<Result<()>>,
    },
}

#[derive(Clone)]
struct PlaybackOwnerConnection {
    requests: SyncSender<PlaybackOwnerRequest>,
}

impl PlaybackOwnerConnection {
    fn attach(
        &self,
        executor: PlaybackCommandExecutor,
        bounds: superi_engine::editor::TimeRange,
    ) -> Result<()> {
        let (response, receiver) = sync_channel(1);
        self.requests
            .try_send(PlaybackOwnerRequest::Attach {
                executor,
                bounds,
                response,
            })
            .map_err(playback_control_admission_error)?;
        wait_for_engine_response(receiver, PLAYBACK_CONTROL_TIMEOUT)
    }

    fn reconfigure(&self, bounds: superi_engine::editor::TimeRange) -> Result<()> {
        let (response, receiver) = sync_channel(1);
        self.requests
            .try_send(PlaybackOwnerRequest::Reconfigure { bounds, response })
            .map_err(playback_control_admission_error)?;
        wait_for_engine_response(receiver, PLAYBACK_CONTROL_TIMEOUT)
    }
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

    /// Admits one complete editor-state query for the exact active durable project route.
    pub fn request_editor_state(
        &self,
        route: DesktopProjectEditorRoute,
        request: GetEditorState,
    ) -> Result<PendingEditorState> {
        let (response, receiver) = sync_channel(1);
        self.requests
            .try_send(EngineRequest::EditorState {
                route,
                request,
                response,
            })
            .map_err(connection_admission_error)?;
        Ok(PendingEditorState { receiver })
    }

    /// Admits one revision-fenced project command for durable execution and event capture.
    pub fn request_project_command(
        &self,
        route: DesktopProjectEditorRoute,
        request: ExecuteProjectCommand,
    ) -> Result<PendingProjectCommand> {
        let (response, receiver) = sync_channel(1);
        self.requests
            .try_send(EngineRequest::ProjectCommand {
                route,
                request,
                response,
            })
            .map_err(connection_admission_error)?;
        Ok(PendingProjectCommand { receiver })
    }

    /// Admits one strict interactive transport command for the active durable project route.
    pub fn request_playback_transport(
        &self,
        route: DesktopProjectEditorRoute,
        request: ExecutePlaybackTransport,
    ) -> Result<PendingPlaybackTransport> {
        let (response, receiver) = sync_channel(1);
        self.requests
            .try_send(EngineRequest::PlaybackTransport {
                route,
                request,
                response,
            })
            .map_err(connection_admission_error)?;
        Ok(PendingPlaybackTransport { receiver })
    }
}

/// Durable project command result and its exact ordered public events.
pub struct ProjectEditorCommandOutput {
    result: ExecuteProjectCommandResult,
    events: Vec<ProjectStateChanged>,
    identity: DesktopProjectIdentity,
}

impl ProjectEditorCommandOutput {
    #[must_use]
    pub const fn result(&self) -> &ExecuteProjectCommandResult {
        &self.result
    }

    #[must_use]
    pub fn events(&self) -> &[ProjectStateChanged] {
        &self.events
    }

    #[must_use]
    pub const fn identity(&self) -> &DesktopProjectIdentity {
        &self.identity
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

/// Blocking-safe observation of one previously admitted editor-state query.
#[must_use = "the admitted editor response must be observed on a blocking-safe thread"]
pub struct PendingEditorState {
    receiver: Receiver<Result<GetEditorStateResult>>,
}

impl PendingEditorState {
    pub fn wait(self, timeout: Duration) -> Result<GetEditorStateResult> {
        wait_for_engine_response(self.receiver, timeout)
    }
}

/// Blocking-safe observation of one previously admitted durable project command.
#[must_use = "the admitted project command must be observed on a blocking-safe thread"]
pub struct PendingProjectCommand {
    receiver: Receiver<Result<ProjectEditorCommandOutput>>,
}

impl PendingProjectCommand {
    pub fn wait(self) -> Result<ProjectEditorCommandOutput> {
        self.receiver.recv().map_err(|_| {
            engine_error(
                ErrorCategory::Unavailable,
                Recoverability::Terminal,
                "the headless engine project command channel closed",
                "wait_for_project_command",
            )
        })?
    }
}

/// Blocking-safe observation of one immediately admitted playback transport command.
#[must_use = "the admitted playback response must be observed on a blocking-safe thread"]
pub struct PendingPlaybackTransport {
    receiver: Receiver<Result<ExecutePlaybackTransportResult>>,
}

impl PendingPlaybackTransport {
    pub fn wait(self, timeout: Duration) -> Result<ExecutePlaybackTransportResult> {
        wait_for_engine_response(self.receiver, timeout)
    }
}

/// Explicit owner of one managed EngineControl thread for the desktop process.
#[must_use = "retain the linked engine owner until application shutdown is complete"]
pub struct LinkedEngineProcess {
    connection: EngineConnection,
    worker: ExecutionDomainThread<()>,
    playback_worker: ExecutionDomainThread<()>,
    worker_pool: Arc<BoundedWorkerPool>,
    runtime: DesktopProcessRuntime,
}

impl LinkedEngineProcess {
    /// Links one lifecycle-attached dispatcher to the exact C001 participant seam.
    pub fn launch(lifecycle: ApplicationLifecycle) -> Result<Self> {
        Self::launch_with_runtime(lifecycle, DesktopProcessRuntime::new())
    }

    /// Links the dispatcher while reporting every retained execution owner to the desktop shell.
    pub fn launch_with_runtime(
        lifecycle: ApplicationLifecycle,
        runtime: DesktopProcessRuntime,
    ) -> Result<Self> {
        let participant = lifecycle.headless_engine_participant()?;
        let (requests, receiver) = sync_channel(REQUEST_CAPACITY);
        runtime.update_service(
            DesktopProcessServiceId::BackgroundWorkers,
            DesktopProcessServicePhase::Starting,
            0,
            0,
            false,
            Vec::new(),
            "Starting the bounded background worker pool",
        );
        let pool_config =
            WorkerPoolConfig::recommended(PLAYBACK_WORK_QUEUE_CAPACITY).map_err(|error| {
                runtime.update_service(
                    DesktopProcessServiceId::BackgroundWorkers,
                    DesktopProcessServicePhase::Failed,
                    0,
                    0,
                    false,
                    Vec::new(),
                    error.message(),
                );
                error
            })?;
        let worker_count = pool_config.worker_count();
        let worker_names = background_worker_names(worker_count);
        let worker_pool = Arc::new(BoundedWorkerPool::new(pool_config).map_err(|error| {
            runtime.update_service(
                DesktopProcessServiceId::BackgroundWorkers,
                DesktopProcessServicePhase::Failed,
                0,
                0,
                false,
                worker_names.clone(),
                error.message(),
            );
            error
        })?);
        runtime.update_service(
            DesktopProcessServiceId::BackgroundWorkers,
            DesktopProcessServicePhase::Running,
            worker_count,
            worker_count,
            true,
            worker_names.clone(),
            "Bounded background workers are accepting jobs",
        );
        let (playback_requests, playback_receiver) = sync_channel(PLAYBACK_CONTROL_CAPACITY);
        let playback_pool = Arc::clone(&worker_pool);
        runtime.update_service(
            DesktopProcessServiceId::Playback,
            DesktopProcessServicePhase::Starting,
            1,
            1,
            true,
            vec![ExecutionDomain::Playback.thread_name().to_owned()],
            "Starting the playback execution owner",
        );
        let playback_runtime = runtime.clone();
        let playback_worker = match ExecutionDomain::Playback.spawn(move |_| {
            let result = run_playback_control(playback_pool, playback_receiver);
            let (phase, summary) = match &result {
                Ok(()) => (
                    DesktopProcessServicePhase::Stopped,
                    "Playback execution owner finished".to_owned(),
                ),
                Err(error) => (
                    DesktopProcessServicePhase::Failed,
                    error.message().to_owned(),
                ),
            };
            playback_runtime.update_service(
                DesktopProcessServiceId::Playback,
                phase,
                1,
                0,
                true,
                vec![ExecutionDomain::Playback.thread_name().to_owned()],
                summary,
            );
            result
        }) {
            Ok(worker) => worker,
            Err(error) => {
                runtime.update_service(
                    DesktopProcessServiceId::Playback,
                    DesktopProcessServicePhase::Failed,
                    0,
                    0,
                    false,
                    vec![ExecutionDomain::Playback.thread_name().to_owned()],
                    error.message(),
                );
                if let Ok(pool) = Arc::try_unwrap(worker_pool) {
                    let pool_result = pool.shutdown();
                    runtime.update_service(
                        DesktopProcessServiceId::BackgroundWorkers,
                        if pool_result.is_ok() {
                            DesktopProcessServicePhase::Stopped
                        } else {
                            DesktopProcessServicePhase::Failed
                        },
                        0,
                        0,
                        false,
                        worker_names,
                        "Background workers cleaned up after playback startup failed",
                    );
                }
                return Err(error);
            }
        };
        runtime.update_service(
            DesktopProcessServiceId::Playback,
            DesktopProcessServicePhase::Running,
            1,
            1,
            true,
            vec![playback_worker.thread_name().to_owned()],
            "Playback execution owner is running",
        );
        let playback = PlaybackOwnerConnection {
            requests: playback_requests,
        };
        runtime.update_service(
            DesktopProcessServiceId::EngineControl,
            DesktopProcessServicePhase::Starting,
            1,
            1,
            true,
            vec![ExecutionDomain::EngineControl.thread_name().to_owned()],
            "Starting the engine control execution owner",
        );
        let engine_runtime = runtime.clone();
        let worker = match ExecutionDomain::EngineControl.spawn(move |_| {
            let result = run_engine_control(participant, receiver, playback);
            let (phase, summary) = match &result {
                Ok(()) => (
                    DesktopProcessServicePhase::Stopped,
                    "Engine control execution owner finished".to_owned(),
                ),
                Err(error) => (
                    DesktopProcessServicePhase::Failed,
                    error.message().to_owned(),
                ),
            };
            engine_runtime.update_service(
                DesktopProcessServiceId::EngineControl,
                phase,
                1,
                0,
                true,
                vec![ExecutionDomain::EngineControl.thread_name().to_owned()],
                summary,
            );
            result
        }) {
            Ok(worker) => worker,
            Err(error) => {
                runtime.update_service(
                    DesktopProcessServiceId::EngineControl,
                    DesktopProcessServicePhase::Failed,
                    0,
                    0,
                    false,
                    vec![ExecutionDomain::EngineControl.thread_name().to_owned()],
                    error.message(),
                );
                drop(requests);
                runtime.update_service(
                    DesktopProcessServiceId::Playback,
                    DesktopProcessServicePhase::Stopping,
                    1,
                    1,
                    true,
                    vec![playback_worker.thread_name().to_owned()],
                    "Joining playback after engine control startup failed",
                );
                let playback_result = playback_worker.join();
                runtime.update_service(
                    DesktopProcessServiceId::Playback,
                    if playback_result.is_ok() {
                        DesktopProcessServicePhase::Stopped
                    } else {
                        DesktopProcessServicePhase::Failed
                    },
                    0,
                    0,
                    false,
                    vec![ExecutionDomain::Playback.thread_name().to_owned()],
                    "Playback cleanup after engine control startup completed",
                );
                if let Ok(pool) = Arc::try_unwrap(worker_pool) {
                    let pool_result = pool.shutdown();
                    runtime.update_service(
                        DesktopProcessServiceId::BackgroundWorkers,
                        if pool_result.is_ok() {
                            DesktopProcessServicePhase::Stopped
                        } else {
                            DesktopProcessServicePhase::Failed
                        },
                        0,
                        0,
                        false,
                        worker_names,
                        "Background worker cleanup after engine control startup completed",
                    );
                }
                return Err(error);
            }
        };
        runtime.update_service(
            DesktopProcessServiceId::EngineControl,
            DesktopProcessServicePhase::Running,
            1,
            1,
            true,
            vec![worker.thread_name().to_owned()],
            "Engine control execution owner is running",
        );
        Ok(Self {
            connection: EngineConnection { requests },
            worker,
            playback_worker,
            worker_pool,
            runtime,
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
        let Self {
            connection,
            worker,
            playback_worker,
            worker_pool,
            runtime,
        } = self;
        let engine_thread_name = worker.thread_name().to_owned();
        let playback_thread_name = playback_worker.thread_name().to_owned();
        let worker_count = worker_pool.snapshot().worker_count();
        let worker_names = background_worker_names(worker_count);
        runtime.update_service(
            DesktopProcessServiceId::EngineControl,
            DesktopProcessServicePhase::Stopping,
            1,
            1,
            true,
            vec![engine_thread_name.clone()],
            "Joining the engine control execution owner",
        );
        runtime.update_service(
            DesktopProcessServiceId::Playback,
            DesktopProcessServicePhase::Stopping,
            1,
            1,
            true,
            vec![playback_thread_name.clone()],
            "Joining the playback execution owner",
        );
        runtime.update_service(
            DesktopProcessServiceId::BackgroundWorkers,
            DesktopProcessServicePhase::Stopping,
            worker_count,
            worker_count,
            true,
            worker_names.clone(),
            "Draining and joining bounded background workers",
        );
        let mut first_error = None;
        match worker.join() {
            Ok(()) => runtime.update_service(
                DesktopProcessServiceId::EngineControl,
                DesktopProcessServicePhase::Stopped,
                0,
                0,
                false,
                vec![engine_thread_name],
                "Engine control execution owner joined",
            ),
            Err(error) => {
                runtime.update_service(
                    DesktopProcessServiceId::EngineControl,
                    DesktopProcessServicePhase::Failed,
                    0,
                    0,
                    false,
                    vec![engine_thread_name],
                    error.message(),
                );
                first_error = Some(error);
            }
        }
        drop(connection);
        match playback_worker.join() {
            Ok(()) => runtime.update_service(
                DesktopProcessServiceId::Playback,
                DesktopProcessServicePhase::Stopped,
                0,
                0,
                false,
                vec![playback_thread_name],
                "Playback execution owner joined",
            ),
            Err(error) => {
                runtime.update_service(
                    DesktopProcessServiceId::Playback,
                    DesktopProcessServicePhase::Failed,
                    0,
                    0,
                    false,
                    vec![playback_thread_name],
                    error.message(),
                );
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        let pool_result = Arc::try_unwrap(worker_pool)
            .map_err(|_| {
                engine_error(
                    ErrorCategory::Internal,
                    Recoverability::Terminal,
                    "the playback worker pool retained an unexpected owner at shutdown",
                    "shutdown_playback_pool",
                )
            })
            .and_then(|worker_pool| worker_pool.shutdown().map(|_| ()));
        match pool_result {
            Ok(()) => runtime.update_service(
                DesktopProcessServiceId::BackgroundWorkers,
                DesktopProcessServicePhase::Stopped,
                0,
                0,
                false,
                worker_names,
                "Bounded background workers joined",
            ),
            Err(error) => {
                runtime.update_service(
                    DesktopProcessServiceId::BackgroundWorkers,
                    DesktopProcessServicePhase::Failed,
                    0,
                    0,
                    false,
                    worker_names,
                    error.message(),
                );
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

fn background_worker_names(worker_count: usize) -> Vec<String> {
    (0..worker_count)
        .map(|index| format!("superi-background-job-{index}"))
        .collect()
}

fn run_engine_control(
    participant: HeadlessEngineLifecycleParticipant,
    requests: Receiver<EngineRequest>,
    playback: PlaybackOwnerConnection,
) -> Result<()> {
    let signal = participant.signal();
    let mut dispatcher = None;
    let mut editor_session = None;

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
                editor_session = None;
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
            Ok(request) => execute_request(
                request,
                dispatcher.as_ref(),
                &mut editor_session,
                &playback,
            ),
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

fn run_playback_control(
    worker_pool: Arc<BoundedWorkerPool>,
    requests: Receiver<PlaybackOwnerRequest>,
) -> Result<()> {
    let mut runtime: Option<PlaybackControlRuntime> = None;
    loop {
        match requests.recv_timeout(PLAYBACK_WAIT) {
            Ok(PlaybackOwnerRequest::Attach {
                executor,
                bounds,
                response,
            }) => {
                let result = PlaybackControlRuntime::new(
                    &worker_pool,
                    executor,
                    bounds,
                    PLAYBACK_JOB_NAMESPACE,
                    Instant::now(),
                );
                match result {
                    Ok(owner) => {
                        runtime = Some(owner);
                        let _ = response.try_send(Ok(()));
                    }
                    Err(error) => {
                        let _ = response.try_send(Err(error));
                    }
                }
            }
            Ok(PlaybackOwnerRequest::Reconfigure { bounds, response }) => {
                let result = runtime
                    .as_mut()
                    .ok_or_else(|| {
                        engine_error(
                            ErrorCategory::Unavailable,
                            Recoverability::Retryable,
                            "the desktop playback owner is not attached",
                            "reconfigure_playback",
                        )
                    })
                    .and_then(|owner| owner.reconfigure_bounds(bounds, Instant::now()));
                let _ = response.try_send(result);
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Ok(()),
        }
        if runtime
            .as_mut()
            .is_some_and(|owner| owner.tick(Instant::now()).is_err())
        {
            runtime = None;
        }
    }
}

fn execute_request(
    request: EngineRequest,
    dispatcher: Option<&EngineCommandDispatcher>,
    editor_session: &mut Option<ProjectEditorSession>,
    playback: &PlaybackOwnerConnection,
) {
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
        EngineRequest::EditorState {
            route,
            request,
            response,
        } => {
            let result = require_connected(dispatcher)
                .and_then(|()| editor_session_for(editor_session, &route, playback))
                .and_then(|session| session.editor_state(&route, request));
            let _ = response.try_send(result);
        }
        EngineRequest::ProjectCommand {
            route,
            request,
            response,
        } => {
            let result = require_connected(dispatcher)
                .and_then(|()| editor_session_for(editor_session, &route, playback))
                .and_then(|session| session.execute_project_command(&route, request));
            if result.is_err()
                && editor_session
                    .as_ref()
                    .is_some_and(ProjectEditorSession::durable_state_changed)
            {
                *editor_session = None;
            }
            let _ = response.try_send(result);
        }
        EngineRequest::PlaybackTransport {
            route,
            request,
            response,
        } => {
            let result = require_connected(dispatcher)
                .and_then(|()| editor_session_for(editor_session, &route, playback))
                .and_then(|session| session.execute_playback_transport(&route, request));
            let _ = response.try_send(result);
        }
    }
}

struct ProjectEditorSession {
    path: String,
    durable_snapshot: superi_engine::editor::ProjectSnapshot,
    api: ProjectEditorApi,
    playback: PlaybackOwnerConnection,
    playback_bounds: superi_engine::editor::TimeRange,
}

impl ProjectEditorSession {
    fn load(
        route: &DesktopProjectEditorRoute,
        playback: &PlaybackOwnerConnection,
    ) -> Result<Self> {
        let database = ProjectDatabase::open_read_only(Path::new(route.path()))?;
        let document = database.load()?;
        let snapshot = document.snapshot();
        require_route_snapshot(route, &snapshot)?;
        let playback_bounds = project_playback_bounds(&snapshot)?;
        let (mut dispatcher, executor) = EngineCommandDispatcher::new_with_playback_bridge()?;
        dispatcher.attach_project(document)?;
        drive_project_dispatcher_to_running(&mut dispatcher)?;
        dispatcher.drain_events()?;
        playback.attach(executor, playback_bounds)?;
        Ok(Self {
            path: route.path().to_owned(),
            durable_snapshot: snapshot,
            api: ProjectEditorApi::new(dispatcher)?,
            playback: playback.clone(),
            playback_bounds,
        })
    }

    fn matches_route(&self, route: &DesktopProjectEditorRoute) -> bool {
        self.path == route.path()
            && self.durable_snapshot.project_id().to_string() == route.project_id()
            && self.durable_snapshot.revision() == route.project_revision()
            && self.durable_snapshot.root_timeline_id().to_string() == route.root_timeline_id()
    }

    fn editor_state(
        &mut self,
        route: &DesktopProjectEditorRoute,
        request: GetEditorState,
    ) -> Result<GetEditorStateResult> {
        self.require_durable_match(route)?;
        self.api.execute(request)
    }

    fn execute_project_command(
        &mut self,
        route: &DesktopProjectEditorRoute,
        request: ExecuteProjectCommand,
    ) -> Result<ProjectEditorCommandOutput> {
        self.require_durable_match(route)?;
        let result = self.api.execute(request)?;
        let events = self.api.drain_events()?;
        let published = self.api.project_snapshot()?;
        let playback_bounds = project_playback_bounds(&published)?;
        if playback_bounds != self.playback_bounds {
            self.playback.reconfigure(playback_bounds)?;
        }
        let identity = DesktopProjectIdentity::new(
            published.project_id().to_string(),
            published.revision(),
            published.root_timeline_id().to_string(),
        )
        .map_err(|_| {
            engine_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "the published project identity could not be projected",
                "project_identity",
            )
        })?;

        let mut database = ProjectDatabase::open(Path::new(&self.path))?;
        let durable = database.load()?.snapshot();
        if durable != self.durable_snapshot {
            return Err(editor_conflict(
                "persist_project_command",
                "the durable project changed before the editor command could be published",
            ));
        }
        database.replace(&published)?;
        self.durable_snapshot = published.clone();
        self.playback_bounds = playback_bounds;
        Ok(ProjectEditorCommandOutput {
            result,
            events,
            identity,
        })
    }

    fn execute_playback_transport(
        &mut self,
        route: &DesktopProjectEditorRoute,
        request: ExecutePlaybackTransport,
    ) -> Result<ExecutePlaybackTransportResult> {
        self.require_durable_match(route)?;
        self.api.execute(request)
    }

    fn require_durable_match(&self, route: &DesktopProjectEditorRoute) -> Result<()> {
        require_route_snapshot(route, &self.durable_snapshot)?;
        let database = ProjectDatabase::open_read_only(Path::new(&self.path))?;
        let current = database.load()?.snapshot();
        if current != self.durable_snapshot {
            return Err(editor_conflict(
                "validate_project_command",
                "the durable project changed outside the retained editor session",
            ));
        }
        Ok(())
    }

    fn durable_state_changed(&self) -> bool {
        self.api
            .project_snapshot()
            .is_ok_and(|snapshot| snapshot != self.durable_snapshot)
    }
}

fn editor_session_for<'a>(
    session: &'a mut Option<ProjectEditorSession>,
    route: &DesktopProjectEditorRoute,
    playback: &PlaybackOwnerConnection,
) -> Result<&'a mut ProjectEditorSession> {
    if !session
        .as_ref()
        .is_some_and(|current| current.matches_route(route))
    {
        *session = Some(ProjectEditorSession::load(route, playback)?);
    }
    session.as_mut().ok_or_else(|| {
        engine_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "the project editor session was not retained",
            "load_project_editor",
        )
    })
}

fn require_route_snapshot(
    route: &DesktopProjectEditorRoute,
    snapshot: &superi_engine::editor::ProjectSnapshot,
) -> Result<()> {
    if snapshot.project_id().to_string() == route.project_id()
        && snapshot.revision() == route.project_revision()
        && snapshot.root_timeline_id().to_string() == route.root_timeline_id()
    {
        return Ok(());
    }
    Err(editor_conflict(
        "validate_project_route",
        "the active project route no longer matches durable editor state",
    ))
}

fn project_playback_bounds(
    snapshot: &superi_engine::editor::ProjectSnapshot,
) -> Result<superi_engine::editor::TimeRange> {
    let timeline = snapshot
        .editorial_project()
        .timeline(snapshot.root_timeline_id())
        .ok_or_else(|| {
            engine_error(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                "the project root timeline is unavailable for playback",
                "playback_bounds",
            )
        })?;
    let duration = timeline.duration()?;
    let duration = superi_engine::editor::Duration::new(
        duration.value().max(1),
        timeline.edit_rate(),
    )?;
    superi_engine::editor::TimeRange::new(
        superi_engine::editor::RationalTime::zero(timeline.edit_rate()),
        duration,
    )
}

fn drive_project_dispatcher_to_running(dispatcher: &mut EngineCommandDispatcher) -> Result<()> {
    loop {
        let outcome = dispatcher.dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("superi-desktop.editor.startup.inspect")?,
            EngineCommand::InspectLifecycle,
        ))?;
        let EngineCommandResult::Lifecycle(snapshot) = outcome.result() else {
            return Err(engine_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "project dispatcher startup returned an unexpected result",
                "start_project_dispatcher",
            ));
        };
        if snapshot.phase() == LifecyclePhase::Running {
            return Ok(());
        }
        let action = snapshot.pending_action().ok_or_else(|| {
            engine_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "project dispatcher startup has no pending action",
                "start_project_dispatcher",
            )
        })?;
        dispatcher.dispatch(EngineCommandRequest::new(
            EngineTransactionId::new("superi-desktop.editor.startup.complete")?,
            EngineCommand::CompleteLifecycleAction(action),
        ))?;
    }
}

fn require_connected(dispatcher: Option<&EngineCommandDispatcher>) -> Result<()> {
    dispatcher.map(|_| ()).ok_or_else(|| {
        engine_error(
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "the headless engine is not connected",
            "project_editor",
        )
    })
}

fn editor_conflict(operation: &'static str, message: &'static str) -> Error {
    engine_error(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
        operation,
    )
}

fn integration_validation(
    dispatcher: &EngineCommandDispatcher,
) -> Result<GetEngineIntegrationValidationResult> {
    let capabilities = MediaCapabilities::from_registry(&media_backend_registry()?)?;
    let snapshot = dispatcher.integration_validation_snapshot(&capabilities, None)?;
    Ok(IntegrationValidationApi::new(&snapshot).execute(GetEngineIntegrationValidation::new()))
}

fn wait_for_engine_response<T>(receiver: Receiver<Result<T>>, timeout: Duration) -> Result<T> {
    match receiver.recv_timeout(timeout) {
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

fn playback_control_admission_error(error: TrySendError<PlaybackOwnerRequest>) -> Error {
    match error {
        TrySendError::Full(_) => engine_error(
            ErrorCategory::ResourceExhausted,
            Recoverability::Retryable,
            "the desktop playback control queue is full",
            "admit_playback_control",
        ),
        TrySendError::Disconnected(_) => engine_error(
            ErrorCategory::Unavailable,
            Recoverability::Terminal,
            "the desktop playback control owner is closed",
            "admit_playback_control",
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

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use superi_api::editor::{
        EditorTrackItem, ExactDuration, ExactTime, ExactTimeRange, ExactTimebase, ProjectAction,
        ProjectCommand, TimelineEditOperation,
    };
    use superi_engine::editor;

    use super::*;

    const PROJECT: editor::ProjectId = editor::ProjectId::from_raw(0xd001);
    const ROOT: editor::TimelineId = editor::TimelineId::from_raw(0xd002);
    const TRACK: editor::TrackId = editor::TrackId::from_raw(0xd003);
    const GAP: editor::GapId = editor::GapId::from_raw(0xd004);
    const APPENDED: editor::GapId = editor::GapId::from_raw(0xd005);

    struct TemporaryProject {
        path: PathBuf,
    }

    impl TemporaryProject {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time is available")
                .as_nanos();
            Self {
                path: std::env::temp_dir().join(format!(
                    "superi-desktop-editor-{}-{nonce}.superi",
                    std::process::id()
                )),
            }
        }
    }

    impl Drop for TemporaryProject {
        fn drop(&mut self) {
            let _ = fs::remove_file(&self.path);
        }
    }

    fn exact_range(start: i64, duration: u64) -> editor::TimeRange {
        let timebase = editor::FrameRate::FPS_24.timebase();
        editor::TimeRange::new(
            editor::RationalTime::new(start, timebase),
            editor::Duration::new(duration, timebase).unwrap(),
        )
        .unwrap()
    }

    fn project_document() -> editor::ProjectDocument {
        let timebase = editor::FrameRate::FPS_24.timebase();
        let timeline = editor::Timeline::new(
            ROOT,
            "Desktop editor test",
            timebase,
            editor::RationalTime::zero(timebase),
            vec![editor::Track::new(
                TRACK,
                "V1",
                editor::TrackSemantics::Video(editor::VideoTrackSemantics::new(
                    editor::FrameRate::FPS_24,
                    editor::VideoCompositing::Over,
                )),
                vec![editor::TrackItem::Gap(editor::Gap::new(
                    GAP,
                    "Initial gap",
                    exact_range(0, 48),
                ))],
            )],
        );
        let editorial = editor::EditorialProject::new(
            PROJECT,
            "Desktop editor project",
            std::iter::empty::<editor::LinkedMediaReference>(),
            [timeline],
        )
        .unwrap();
        editor::ProjectDocument::new(editorial, ROOT).unwrap()
    }

    fn route(path: &Path, revision: u64) -> DesktopProjectEditorRoute {
        let identity =
            DesktopProjectIdentity::new(PROJECT.to_string(), revision, ROOT.to_string()).unwrap();
        DesktopProjectEditorRoute::new(path.to_string_lossy(), &identity)
    }

    fn public_range(start: i64, duration: u64) -> ExactTimeRange {
        let timebase = ExactTimebase {
            numerator: 24,
            denominator: 1,
        };
        ExactTimeRange {
            start: ExactTime {
                value: start,
                timebase,
            },
            duration: ExactDuration {
                value: duration,
                timebase,
            },
        }
    }

    #[test]
    fn retained_editor_session_persists_apply_undo_redo_and_exact_events() {
        let worker_pool = Arc::new(
            BoundedWorkerPool::new(WorkerPoolConfig::new(2, 32).unwrap()).unwrap(),
        );
        let (playback_requests, playback_receiver) = sync_channel(PLAYBACK_CONTROL_CAPACITY);
        let playback = PlaybackOwnerConnection {
            requests: playback_requests,
        };
        let playback_pool = Arc::clone(&worker_pool);
        let playback_worker = ExecutionDomain::Playback
            .spawn(move |_| run_playback_control(playback_pool, playback_receiver))
            .unwrap();
        let domain = ExecutionDomain::EngineControl
            .enter_current()
            .expect("test owns engine control");
        let temporary = TemporaryProject::new();
        let initial = project_document().snapshot();
        let mut database = ProjectDatabase::create(&temporary.path).unwrap();
        database.replace(&initial).unwrap();
        drop(database);

        let route0 = route(&temporary.path, 0);
        let mut session = ProjectEditorSession::load(&route0, &playback).unwrap();
        let state = session
            .editor_state(&route0, GetEditorState::new("desktop-state"))
            .unwrap();
        assert_eq!(state.snapshot().project().project_revision(), 0);

        let applied = session
            .execute_project_command(
                &route0,
                ExecuteProjectCommand::new(
                    "desktop-append",
                    0,
                    ProjectCommand::Apply {
                        actions: vec![ProjectAction::EditTimeline {
                            operations: vec![TimelineEditOperation::Append {
                                timeline_id: ROOT.to_string(),
                                track_id: TRACK.to_string(),
                                material: EditorTrackItem::Gap {
                                    id: APPENDED.to_string(),
                                    name: "Appended gap".to_owned(),
                                    record_range: public_range(0, 12),
                                },
                            }],
                        }],
                    },
                ),
            )
            .unwrap();
        assert_eq!(applied.result().state().project_revision(), 1);
        assert_eq!(applied.result().state().undo_depth(), 1);
        assert_eq!(applied.events().len(), 1);
        assert_eq!(applied.events()[0].project_revision(), 1);
        let route1 =
            DesktopProjectEditorRoute::new(temporary.path.to_string_lossy(), applied.identity());

        let undone = session
            .execute_project_command(
                &route1,
                ExecuteProjectCommand::new("desktop-undo", 1, ProjectCommand::Undo {}),
            )
            .unwrap();
        assert_eq!(undone.result().state().project_revision(), 2);
        assert_eq!(undone.result().state().redo_depth(), 1);
        let route2 =
            DesktopProjectEditorRoute::new(temporary.path.to_string_lossy(), undone.identity());

        let redone = session
            .execute_project_command(
                &route2,
                ExecuteProjectCommand::new("desktop-redo", 2, ProjectCommand::Redo {}),
            )
            .unwrap();
        assert_eq!(redone.result().state().project_revision(), 3);
        assert_eq!(redone.result().state().undo_depth(), 1);
        assert_eq!(redone.events().len(), 1);

        let reopened = ProjectDatabase::open_read_only(&temporary.path)
            .unwrap()
            .load()
            .unwrap()
            .snapshot();
        assert_eq!(reopened, session.api.project_snapshot().unwrap());
        assert_eq!(
            reopened
                .editorial_project()
                .timeline(ROOT)
                .unwrap()
                .track(TRACK)
                .unwrap()
                .items()
                .len(),
            2
        );
        assert_eq!(
            session
                .editor_state(&route0, GetEditorState::new("stale-state"))
                .unwrap_err()
                .category(),
            ErrorCategory::Conflict
        );

        drop(session);
        drop(domain);
        drop(playback);
        playback_worker.join().unwrap();
        match Arc::try_unwrap(worker_pool) {
            Ok(pool) => {
                pool.shutdown().unwrap();
            }
            Err(_) => panic!("test playback pool retained an unexpected owner"),
        }
    }
}
