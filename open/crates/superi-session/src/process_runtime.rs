//! Explicit application process ownership and ordered session-task cleanup.

use std::panic::{self, AssertUnwindSafe};
use std::sync::{mpsc::sync_channel, Arc, Mutex, MutexGuard};
use std::thread::{self, JoinHandle};

use serde::Serialize;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

const COMPONENT: &str = "superi-desktop.process-runtime";
const MAX_BACKGROUND_TASKS: usize = 8;
const EXIT_MONITOR_THREAD: &str = "superi-application-exit";

/// Stable application process phases exposed to native and headless hosts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopProcessPhase {
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

/// Stable identities for every long-lived desktop execution owner.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopProcessServiceId {
    ApplicationExit,
    FileAssociationTasks,
    EngineControl,
    Playback,
    BackgroundWorkers,
    GpuSubmission,
    WindowPersistence,
}

impl DesktopProcessServiceId {
    const ALL: [Self; 7] = [
        Self::ApplicationExit,
        Self::FileAssociationTasks,
        Self::EngineControl,
        Self::Playback,
        Self::BackgroundWorkers,
        Self::GpuSubmission,
        Self::WindowPersistence,
    ];

    const fn label(self) -> &'static str {
        match self {
            Self::ApplicationExit => "Application exit",
            Self::FileAssociationTasks => "Project file association tasks",
            Self::EngineControl => "Engine control",
            Self::Playback => "Playback",
            Self::BackgroundWorkers => "Background workers",
            Self::GpuSubmission => "GPU submission",
            Self::WindowPersistence => "Window persistence",
        }
    }

    const fn kind(self) -> DesktopProcessServiceKind {
        match self {
            Self::ApplicationExit => DesktopProcessServiceKind::Monitor,
            Self::FileAssociationTasks => DesktopProcessServiceKind::TaskGroup,
            Self::EngineControl | Self::Playback | Self::GpuSubmission => {
                DesktopProcessServiceKind::ExecutionDomain
            }
            Self::BackgroundWorkers => DesktopProcessServiceKind::WorkerPool,
            Self::WindowPersistence => DesktopProcessServiceKind::Persistence,
        }
    }
}

/// Stable process-owner categories used for operational presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopProcessServiceKind {
    Monitor,
    TaskGroup,
    ExecutionDomain,
    WorkerPool,
    Persistence,
}

/// Explicit lifecycle for one process-owned service.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopProcessServicePhase {
    Pending,
    Starting,
    Running,
    Stopping,
    Stopped,
    Failed,
}

/// Related service-unit counts updated as one coherent ownership observation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DesktopProcessServiceCounts {
    owned_units: usize,
    active_units: usize,
}

impl DesktopProcessServiceCounts {
    pub(crate) const fn new(owned_units: usize, active_units: usize) -> Self {
        Self {
            owned_units,
            active_units,
        }
    }
}

/// Immutable operational state for one process-owned service.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopProcessServiceSnapshot {
    id: DesktopProcessServiceId,
    label: String,
    kind: DesktopProcessServiceKind,
    phase: DesktopProcessServicePhase,
    owned_units: usize,
    active_units: usize,
    join_pending: bool,
    thread_names: Vec<String>,
    summary: String,
}

impl DesktopProcessServiceSnapshot {
    /// Returns the stable service identity.
    #[must_use]
    pub const fn id(&self) -> DesktopProcessServiceId {
        self.id
    }

    /// Returns the user-safe service label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the service ownership category.
    #[must_use]
    pub const fn kind(&self) -> DesktopProcessServiceKind {
        self.kind
    }

    /// Returns the explicit service phase.
    #[must_use]
    pub const fn phase(&self) -> DesktopProcessServicePhase {
        self.phase
    }

    /// Returns the number of process-owned execution units.
    #[must_use]
    pub const fn owned_units(&self) -> usize {
        self.owned_units
    }

    /// Returns the number of units still executing.
    #[must_use]
    pub const fn active_units(&self) -> usize {
        self.active_units
    }

    /// Returns whether an owned handle still requires an explicit join.
    #[must_use]
    pub const fn join_pending(&self) -> bool {
        self.join_pending
    }

    /// Returns the exact operating-system thread names when they are known.
    #[must_use]
    pub fn thread_names(&self) -> &[String] {
        &self.thread_names
    }

    /// Returns a user-safe operational summary.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }
}

/// Immutable view of desktop process ownership and cleanup state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DesktopProcessSnapshot {
    revision: u64,
    phase: DesktopProcessPhase,
    accepting_background_tasks: bool,
    services: Vec<DesktopProcessServiceSnapshot>,
}

impl DesktopProcessSnapshot {
    /// Returns the monotonic process-state revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the explicit whole-process phase.
    #[must_use]
    pub const fn phase(&self) -> DesktopProcessPhase {
        self.phase
    }

    /// Returns whether new session background tasks may be admitted.
    #[must_use]
    pub const fn accepting_background_tasks(&self) -> bool {
        self.accepting_background_tasks
    }

    /// Returns every process service in stable startup order.
    #[must_use]
    pub fn services(&self) -> &[DesktopProcessServiceSnapshot] {
        &self.services
    }

    /// Finds one stable service snapshot.
    #[must_use]
    pub fn service(&self, id: DesktopProcessServiceId) -> Option<&DesktopProcessServiceSnapshot> {
        self.services.iter().find(|service| service.id == id)
    }
}

struct ProcessState {
    revision: u64,
    phase: DesktopProcessPhase,
    accepting_background_tasks: bool,
    services: Vec<DesktopProcessServiceSnapshot>,
}

impl Default for ProcessState {
    fn default() -> Self {
        Self {
            revision: 0,
            phase: DesktopProcessPhase::Starting,
            accepting_background_tasks: true,
            services: DesktopProcessServiceId::ALL
                .into_iter()
                .map(|id| DesktopProcessServiceSnapshot {
                    id,
                    label: id.label().to_owned(),
                    kind: id.kind(),
                    phase: DesktopProcessServicePhase::Pending,
                    owned_units: 0,
                    active_units: 0,
                    join_pending: false,
                    thread_names: Vec::new(),
                    summary: "Waiting for startup".to_owned(),
                })
                .collect(),
        }
    }
}

type ShellTaskOutcome = std::result::Result<(), String>;
type ShellTaskHandle = JoinHandle<ShellTaskOutcome>;

#[derive(Default)]
struct ProcessOwners {
    exit_monitor: Option<JoinHandle<ShellTaskOutcome>>,
    background_tasks: Vec<ShellTaskHandle>,
}

struct DesktopProcessRuntimeInner {
    state: Mutex<ProcessState>,
    owners: Mutex<ProcessOwners>,
}

/// Shared session-local owner registry for the complete application process.
#[derive(Clone)]
pub struct DesktopProcessRuntime {
    inner: Arc<DesktopProcessRuntimeInner>,
}

impl Default for DesktopProcessRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl DesktopProcessRuntime {
    /// Creates one process registry in explicit startup state.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(DesktopProcessRuntimeInner {
                state: Mutex::new(ProcessState::default()),
                owners: Mutex::new(ProcessOwners::default()),
            }),
        }
    }

    /// Returns one immutable operational process snapshot.
    #[must_use]
    pub fn snapshot(&self) -> DesktopProcessSnapshot {
        let state = lock_unpoisoned(&self.inner.state);
        DesktopProcessSnapshot {
            revision: state.revision,
            phase: state.phase,
            accepting_background_tasks: state.accepting_background_tasks,
            services: state.services.clone(),
        }
    }

    /// Marks successful completion of native startup.
    pub fn finish_startup(&self) {
        let mut state = lock_unpoisoned(&self.inner.state);
        if state.phase == DesktopProcessPhase::Starting {
            state.phase = DesktopProcessPhase::Running;
            bump_revision(&mut state);
        }
    }

    /// Closes session task admission before ordered owner teardown begins.
    pub fn begin_shutdown(&self) {
        let _owners = lock_unpoisoned(&self.inner.owners);
        let mut state = lock_unpoisoned(&self.inner.state);
        state.accepting_background_tasks = false;
        if !matches!(
            state.phase,
            DesktopProcessPhase::Stopped | DesktopProcessPhase::Failed
        ) {
            state.phase = DesktopProcessPhase::Stopping;
        }
        if let Some(service) =
            service_mut(&mut state, DesktopProcessServiceId::FileAssociationTasks)
        {
            if service.owned_units > 0 {
                service.phase = DesktopProcessServicePhase::Stopping;
                service.summary = "Joining admitted project file tasks".to_owned();
            } else if service.phase != DesktopProcessServicePhase::Failed {
                service.phase = DesktopProcessServicePhase::Stopped;
                service.summary = "No project file tasks remain".to_owned();
            }
        }
        bump_revision(&mut state);
    }

    /// Marks successful completion of all process-owned cleanup.
    pub fn finish_shutdown(&self) {
        let mut state = lock_unpoisoned(&self.inner.state);
        state.accepting_background_tasks = false;
        state.phase = DesktopProcessPhase::Stopped;
        bump_revision(&mut state);
    }

    /// Marks cleanup failure while preserving all service evidence.
    pub fn fail_shutdown(&self, summary: impl Into<String>) {
        let mut state = lock_unpoisoned(&self.inner.state);
        state.accepting_background_tasks = false;
        state.phase = DesktopProcessPhase::Failed;
        let summary = summary.into();
        if !summary.is_empty() {
            for service in &mut state.services {
                if service.phase == DesktopProcessServicePhase::Stopping {
                    service.summary = summary.clone();
                }
            }
        }
        bump_revision(&mut state);
    }

    /// Starts and retains the sole application-exit monitor.
    pub fn spawn_application_exit<F>(&self, task: F) -> Result<()>
    where
        F: FnOnce() -> std::result::Result<(), String> + Send + 'static,
    {
        let mut owners = lock_unpoisoned(&self.inner.owners);
        if owners.exit_monitor.is_some() {
            return Err(process_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "the application exit monitor is already owned",
                "spawn_application_exit",
            ));
        }
        self.update_service(
            DesktopProcessServiceId::ApplicationExit,
            DesktopProcessServicePhase::Starting,
            DesktopProcessServiceCounts::new(1, 1),
            true,
            vec![EXIT_MONITOR_THREAD.to_owned()],
            "Starting the application exit monitor",
        );
        let runtime = self.clone();
        let handle = thread::Builder::new()
            .name(EXIT_MONITOR_THREAD.to_owned())
            .spawn(move || {
                runtime.update_service(
                    DesktopProcessServiceId::ApplicationExit,
                    DesktopProcessServicePhase::Running,
                    DesktopProcessServiceCounts::new(1, 1),
                    true,
                    vec![EXIT_MONITOR_THREAD.to_owned()],
                    "Observing lifecycle shutdown",
                );
                match panic::catch_unwind(AssertUnwindSafe(task)) {
                    Ok(Ok(())) => {
                        runtime.update_service(
                            DesktopProcessServiceId::ApplicationExit,
                            DesktopProcessServicePhase::Stopped,
                            DesktopProcessServiceCounts::new(1, 0),
                            true,
                            vec![EXIT_MONITOR_THREAD.to_owned()],
                            "Application exit monitor finished",
                        );
                        Ok(())
                    }
                    Ok(Err(summary)) => {
                        runtime.update_service(
                            DesktopProcessServiceId::ApplicationExit,
                            DesktopProcessServicePhase::Failed,
                            DesktopProcessServiceCounts::new(1, 0),
                            true,
                            vec![EXIT_MONITOR_THREAD.to_owned()],
                            summary.clone(),
                        );
                        Err(summary)
                    }
                    Err(_) => {
                        runtime.update_service(
                            DesktopProcessServiceId::ApplicationExit,
                            DesktopProcessServicePhase::Failed,
                            DesktopProcessServiceCounts::new(1, 0),
                            true,
                            vec![EXIT_MONITOR_THREAD.to_owned()],
                            "Application exit monitor panicked",
                        );
                        Err("application exit monitor panicked".to_owned())
                    }
                }
            })
            .map_err(|source| {
                self.update_service(
                    DesktopProcessServiceId::ApplicationExit,
                    DesktopProcessServicePhase::Failed,
                    DesktopProcessServiceCounts::new(0, 0),
                    false,
                    vec![EXIT_MONITOR_THREAD.to_owned()],
                    "Application exit monitor could not start",
                );
                Error::with_source(
                    ErrorCategory::Unavailable,
                    Recoverability::Retryable,
                    "the application exit monitor could not start",
                    source,
                )
                .with_context(ErrorContext::new(COMPONENT, "spawn_application_exit"))
            })?;
        owners.exit_monitor = Some(handle);
        Ok(())
    }

    /// Joins the retained application-exit monitor exactly once.
    pub fn join_application_exit(&self) -> Result<()> {
        let handle = lock_unpoisoned(&self.inner.owners).exit_monitor.take();
        let Some(handle) = handle else {
            self.update_service(
                DesktopProcessServiceId::ApplicationExit,
                DesktopProcessServicePhase::Stopped,
                DesktopProcessServiceCounts::new(0, 0),
                false,
                vec![EXIT_MONITOR_THREAD.to_owned()],
                "Application exit monitor is not running",
            );
            return Ok(());
        };
        self.update_service(
            DesktopProcessServiceId::ApplicationExit,
            DesktopProcessServicePhase::Stopping,
            DesktopProcessServiceCounts::new(1, 1),
            true,
            vec![EXIT_MONITOR_THREAD.to_owned()],
            "Joining the application exit monitor",
        );
        match handle.join() {
            Ok(Ok(())) => {
                self.update_service(
                    DesktopProcessServiceId::ApplicationExit,
                    DesktopProcessServicePhase::Stopped,
                    DesktopProcessServiceCounts::new(0, 0),
                    false,
                    vec![EXIT_MONITOR_THREAD.to_owned()],
                    "Application exit monitor joined",
                );
                Ok(())
            }
            Ok(Err(summary)) => {
                self.update_service(
                    DesktopProcessServiceId::ApplicationExit,
                    DesktopProcessServicePhase::Failed,
                    DesktopProcessServiceCounts::new(0, 0),
                    false,
                    vec![EXIT_MONITOR_THREAD.to_owned()],
                    summary.clone(),
                );
                Err(process_error(
                    ErrorCategory::Internal,
                    Recoverability::Terminal,
                    summary,
                    "join_application_exit",
                ))
            }
            Err(_) => {
                self.update_service(
                    DesktopProcessServiceId::ApplicationExit,
                    DesktopProcessServicePhase::Failed,
                    DesktopProcessServiceCounts::new(0, 0),
                    false,
                    vec![EXIT_MONITOR_THREAD.to_owned()],
                    "Application exit monitor join failed",
                );
                Err(process_error(
                    ErrorCategory::Internal,
                    Recoverability::Terminal,
                    "the application exit monitor could not be joined",
                    "join_application_exit",
                ))
            }
        }
    }

    /// Admits one bounded blocking session task and retains its join handle.
    pub fn spawn_background_task<F>(&self, task: F) -> Result<()>
    where
        F: FnOnce() + Send + 'static,
    {
        self.reap_finished_background_tasks()?;
        let mut owners = lock_unpoisoned(&self.inner.owners);
        let mut state = lock_unpoisoned(&self.inner.state);
        if !state.accepting_background_tasks {
            return Err(process_error(
                ErrorCategory::Unavailable,
                Recoverability::UserCorrectable,
                "desktop shutdown has closed background task admission",
                "spawn_background_task",
            ));
        }
        if owners.background_tasks.len() >= MAX_BACKGROUND_TASKS {
            return Err(process_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::Retryable,
                "desktop background task capacity is exhausted",
                "spawn_background_task",
            ));
        }
        let (start_sender, start_receiver) = sync_channel(1);
        let runtime = self.clone();
        let handle = thread::Builder::new()
            .name("superi-session-background".to_owned())
            .spawn(move || {
                let outcome = match start_receiver.recv() {
                    Ok(()) => match panic::catch_unwind(AssertUnwindSafe(task)) {
                        Ok(()) => Ok(()),
                        Err(_) => Err("project file association task panicked".to_owned()),
                    },
                    Err(_) => {
                        Err("project file association task admission was interrupted".to_owned())
                    }
                };
                runtime.finish_background_task(outcome.as_ref().err());
                outcome
            })
            .map_err(|source| {
                Error::with_source(
                    ErrorCategory::Unavailable,
                    Recoverability::Retryable,
                    "desktop background task thread could not be created",
                    source,
                )
                .with_context(ErrorContext::new(COMPONENT, "spawn_background_task"))
            })?;
        owners.background_tasks.push(handle);
        let owned_units = owners.background_tasks.len();
        let service = service_mut(&mut state, DesktopProcessServiceId::FileAssociationTasks)
            .expect("the process service inventory is complete");
        service.phase = DesktopProcessServicePhase::Running;
        service.owned_units = owned_units;
        service.active_units = service.active_units.saturating_add(1);
        service.join_pending = true;
        service.summary = format!("{owned_units} retained project file task handle(s)");
        bump_revision(&mut state);
        drop(state);
        drop(owners);
        start_sender.send(()).map_err(|_| {
            process_error(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "desktop background task admission was interrupted",
                "spawn_background_task",
            )
        })
    }

    /// Joins every admitted blocking session task, even after an earlier failure.
    pub fn join_background_tasks(&self) -> Result<()> {
        self.begin_shutdown();
        let handles = {
            let mut owners = lock_unpoisoned(&self.inner.owners);
            std::mem::take(&mut owners.background_tasks)
        };
        if handles.is_empty() {
            self.update_service(
                DesktopProcessServiceId::FileAssociationTasks,
                DesktopProcessServicePhase::Stopped,
                DesktopProcessServiceCounts::new(0, 0),
                false,
                Vec::new(),
                "All project file tasks joined",
            );
            return Ok(());
        }
        let mut first_error = None;
        for handle in handles {
            if let Err(error) = join_background_handle(handle) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        match first_error {
            None => {
                self.update_service(
                    DesktopProcessServiceId::FileAssociationTasks,
                    DesktopProcessServicePhase::Stopped,
                    DesktopProcessServiceCounts::new(0, 0),
                    false,
                    Vec::new(),
                    "All project file tasks joined",
                );
                Ok(())
            }
            Some(error) => {
                self.update_service(
                    DesktopProcessServiceId::FileAssociationTasks,
                    DesktopProcessServicePhase::Failed,
                    DesktopProcessServiceCounts::new(0, 0),
                    false,
                    Vec::new(),
                    error.message(),
                );
                Err(error)
            }
        }
    }

    pub(crate) fn update_service(
        &self,
        id: DesktopProcessServiceId,
        phase: DesktopProcessServicePhase,
        counts: DesktopProcessServiceCounts,
        join_pending: bool,
        thread_names: Vec<String>,
        summary: impl Into<String>,
    ) {
        let mut state = lock_unpoisoned(&self.inner.state);
        let service =
            service_mut(&mut state, id).expect("the process service inventory is complete");
        service.phase = phase;
        service.owned_units = counts.owned_units;
        service.active_units = counts.active_units.min(counts.owned_units);
        service.join_pending = join_pending;
        service.thread_names = thread_names;
        service.summary = summary.into();
        bump_revision(&mut state);
    }

    fn finish_background_task(&self, failure: Option<&String>) {
        let mut state = lock_unpoisoned(&self.inner.state);
        let accepting = state.accepting_background_tasks;
        let service = service_mut(&mut state, DesktopProcessServiceId::FileAssociationTasks)
            .expect("the process service inventory is complete");
        service.active_units = service.active_units.saturating_sub(1);
        if let Some(failure) = failure {
            service.phase = DesktopProcessServicePhase::Failed;
            service.summary = failure.clone();
        } else if accepting {
            service.phase = DesktopProcessServicePhase::Running;
            service.summary = "Project file task finished and awaits join".to_owned();
        } else {
            service.phase = DesktopProcessServicePhase::Stopping;
            service.summary = "Project file task finished during shutdown".to_owned();
        }
        bump_revision(&mut state);
    }

    fn reap_finished_background_tasks(&self) -> Result<()> {
        let finished = {
            let mut owners = lock_unpoisoned(&self.inner.owners);
            let handles = std::mem::take(&mut owners.background_tasks);
            let (finished, pending): (Vec<_>, Vec<_>) =
                handles.into_iter().partition(JoinHandle::is_finished);
            owners.background_tasks = pending;
            finished
        };
        let mut first_error = None;
        for handle in finished {
            if let Err(error) = join_background_handle(handle) {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        let owned_units = lock_unpoisoned(&self.inner.owners).background_tasks.len();
        let mut state = lock_unpoisoned(&self.inner.state);
        let service = service_mut(&mut state, DesktopProcessServiceId::FileAssociationTasks)
            .expect("the process service inventory is complete");
        service.owned_units = owned_units;
        service.join_pending = owned_units > 0;
        if let Some(error) = first_error.as_ref() {
            service.phase = DesktopProcessServicePhase::Failed;
            service.active_units = service.active_units.min(owned_units);
            service.summary = error.message().to_owned();
        } else if owned_units == 0 && service.active_units == 0 {
            service.phase = DesktopProcessServicePhase::Stopped;
            service.summary = "Completed project file tasks were joined".to_owned();
        }
        bump_revision(&mut state);
        drop(state);
        match first_error {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

fn join_background_handle(handle: ShellTaskHandle) -> Result<()> {
    match handle.join() {
        Ok(Ok(())) => Ok(()),
        Ok(Err(summary)) => Err(process_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            summary,
            "join_background_task",
        )),
        Err(_) => Err(process_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "a desktop background task could not be joined",
            "join_background_task",
        )),
    }
}

fn service_mut(
    state: &mut ProcessState,
    id: DesktopProcessServiceId,
) -> Option<&mut DesktopProcessServiceSnapshot> {
    state.services.iter_mut().find(|service| service.id == id)
}

fn bump_revision(state: &mut ProcessState) {
    state.revision = state.revision.saturating_add(1);
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn process_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: impl Into<String>,
    operation: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc;

    use super::*;

    #[test]
    fn application_exit_thread_is_retained_until_join() {
        let runtime = DesktopProcessRuntime::new();
        let completed = Arc::new(AtomicBool::new(false));
        let task_completed = Arc::clone(&completed);
        runtime
            .spawn_application_exit(move || {
                task_completed.store(true, Ordering::SeqCst);
                Ok(())
            })
            .unwrap();

        runtime.join_application_exit().unwrap();

        assert!(completed.load(Ordering::SeqCst));
        let snapshot = runtime.snapshot();
        let service = snapshot
            .service(DesktopProcessServiceId::ApplicationExit)
            .unwrap();
        assert_eq!(service.phase(), DesktopProcessServicePhase::Stopped);
        assert!(!service.join_pending());
    }

    #[test]
    fn blocking_tasks_close_admission_and_all_join() {
        let runtime = DesktopProcessRuntime::new();
        let (release, wait) = mpsc::channel();
        runtime
            .spawn_background_task(move || {
                wait.recv().unwrap();
            })
            .unwrap();
        runtime.begin_shutdown();
        assert!(runtime.spawn_background_task(|| {}).is_err());
        release.send(()).unwrap();

        runtime.join_background_tasks().unwrap();

        let snapshot = runtime.snapshot();
        assert!(!snapshot.accepting_background_tasks());
        let service = snapshot
            .service(DesktopProcessServiceId::FileAssociationTasks)
            .unwrap();
        assert_eq!(service.phase(), DesktopProcessServicePhase::Stopped);
        assert_eq!(service.owned_units(), 0);
        assert!(!service.join_pending());
    }

    #[test]
    fn fast_background_tasks_never_finish_before_their_handle_is_registered() {
        let runtime = DesktopProcessRuntime::new();
        runtime.spawn_background_task(|| {}).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);

        loop {
            let snapshot = runtime.snapshot();
            let service = snapshot
                .service(DesktopProcessServiceId::FileAssociationTasks)
                .unwrap();
            assert!(service.active_units() <= service.owned_units());
            if service.active_units() == 0 {
                assert_eq!(service.owned_units(), 1);
                assert!(service.join_pending());
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }

        runtime.join_background_tasks().unwrap();
    }
}
