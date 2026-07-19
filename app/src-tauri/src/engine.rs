//! Long-lived headless-engine ownership behind the desktop application connection.

use std::path::Path;
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TrySendError};
use std::time::Duration;

use superi_api::commands::{
    GetEditorState, GetEditorStateResult, GetEngineIntegrationValidation,
    GetEngineIntegrationValidationResult,
};
use superi_api::editor::{ExecuteProjectCommand, ExecuteProjectCommandResult, ProjectEditorApi};
use superi_api::events::ProjectStateChanged;
use superi_api::validation::IntegrationValidationApi;
use superi_concurrency::lifecycle::LifecyclePhase;
use superi_concurrency::threads::{ExecutionDomain, ExecutionDomainThread};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult,
    EngineTransactionId,
};
use superi_engine::editor::ProjectDatabase;
use superi_engine::introspection::MediaCapabilities;
use superi_engine::media::media_backend_registry;

use crate::lifecycle::{
    ApplicationLifecycle, ApplicationLifecyclePhase, HeadlessEngineFailure,
    HeadlessEngineLifecycleParticipant, LifecycleIntent,
};
use crate::project_lifecycle::{DesktopProjectEditorRoute, DesktopProjectIdentity};

const COMPONENT: &str = "superi-desktop.engine";
const REQUEST_CAPACITY: usize = 8;
const CONTROL_WAIT: Duration = Duration::from_millis(10);

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
            Ok(request) => execute_request(request, dispatcher.as_ref(), &mut editor_session),
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

fn execute_request(
    request: EngineRequest,
    dispatcher: Option<&EngineCommandDispatcher>,
    editor_session: &mut Option<ProjectEditorSession>,
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
                .and_then(|()| editor_session_for(editor_session, &route))
                .and_then(|session| session.editor_state(&route, request));
            let _ = response.try_send(result);
        }
        EngineRequest::ProjectCommand {
            route,
            request,
            response,
        } => {
            let result = require_connected(dispatcher)
                .and_then(|()| editor_session_for(editor_session, &route))
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
    }
}

struct ProjectEditorSession {
    path: String,
    durable_snapshot: superi_engine::editor::ProjectSnapshot,
    api: ProjectEditorApi,
}

impl ProjectEditorSession {
    fn load(route: &DesktopProjectEditorRoute) -> Result<Self> {
        let database = ProjectDatabase::open_read_only(Path::new(route.path()))?;
        let document = database.load()?;
        let snapshot = document.snapshot();
        require_route_snapshot(route, &snapshot)?;
        let mut dispatcher = EngineCommandDispatcher::new()?;
        dispatcher.attach_project(document)?;
        Ok(Self {
            path: route.path().to_owned(),
            durable_snapshot: snapshot,
            api: ProjectEditorApi::new(dispatcher)?,
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
        Ok(ProjectEditorCommandOutput {
            result,
            events,
            identity,
        })
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
) -> Result<&'a mut ProjectEditorSession> {
    if !session
        .as_ref()
        .is_some_and(|current| current.matches_route(route))
    {
        *session = Some(ProjectEditorSession::load(route)?);
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
        let _domain = ExecutionDomain::EngineControl
            .enter_current()
            .expect("test owns engine control");
        let temporary = TemporaryProject::new();
        let initial = project_document().snapshot();
        let mut database = ProjectDatabase::create(&temporary.path).unwrap();
        database.replace(&initial).unwrap();
        drop(database);

        let route0 = route(&temporary.path, 0);
        let mut session = ProjectEditorSession::load(&route0).unwrap();
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
    }
}
