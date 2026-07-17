use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::ids::{ProjectId, TimelineId};
use superi_core::settings::SettingValue;
use superi_core::time::{RationalTime, Timebase};
use superi_engine::dispatcher::{
    EngineCommand, EngineCommandDispatcher, EngineCommandRequest, EngineCommandResult, EngineEvent,
    EngineTransactionId, MAX_PENDING_ENGINE_EVENTS,
};
use superi_engine::project_recovery::{ProjectRecoveryCommand, ProjectRecoveryOperation};
use superi_project::autosave::{
    ProjectAutosaveCommand, ProjectAutosaveController, ProjectAutosavePolicy,
};
use superi_project::document::{ProjectDocument, ProjectGraph, ProjectSnapshot};
use superi_project::recovery::ProjectRecoveryCandidateId;
use superi_project::settings::{
    ProjectSettingMutation, ProjectSettingsTransaction, AUDIO_SAMPLE_RATE_KEY,
};
use superi_project::ProjectDatabase;
use superi_timeline::model::{EditorialProject, Timeline};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

const PROJECT: ProjectId = ProjectId::from_raw(0xd100);
const ROOT: TimelineId = TimelineId::from_raw(0xd101);

struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "superi-engine-recovery-{label}-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn child(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn document() -> ProjectDocument {
    let rate = Timebase::integer(24).unwrap();
    let timeline = Timeline::new(
        ROOT,
        "engine recovery timeline",
        rate,
        RationalTime::zero(rate),
        vec![],
    );
    let editorial =
        EditorialProject::new(PROJECT, "engine recovery project", [], [timeline]).unwrap();
    ProjectDocument::new(editorial, ROOT).unwrap()
}

fn changed_document(sample_rate: i64) -> ProjectDocument {
    let mut document = document();
    document
        .execute_settings_transaction(
            ProjectSettingsTransaction::new(
                0,
                vec![ProjectSettingMutation::set(
                    AUDIO_SAMPLE_RATE_KEY,
                    SettingValue::Integer(sample_rate),
                )
                .unwrap()],
            )
            .unwrap(),
        )
        .unwrap();
    document
}

fn with_mix_revision(document: ProjectDocument, revision: u64) -> ProjectDocument {
    let snapshot = document.snapshot();
    ProjectDocument::from_complete_parts_with_settings(
        snapshot.revision(),
        snapshot.editorial_project().clone(),
        snapshot.root_timeline_id(),
        snapshot.settings().clone(),
        snapshot.graphs().cloned(),
        superi_audio::mixing::ClipMixState::from_parts(
            revision,
            std::iter::empty::<(
                superi_core::ids::ClipId,
                superi_audio::mixing::ClipMixControls,
            )>(),
        )
        .unwrap(),
    )
    .unwrap()
}

fn publish_recovery(root: &Path, snapshot: &ProjectSnapshot) -> PathBuf {
    let mut autosave = ProjectAutosaveController::new(PROJECT).unwrap();
    autosave
        .execute(ProjectAutosaveCommand::Configure {
            policy: ProjectAutosavePolicy::new(false, Duration::from_secs(60), root, 8).unwrap(),
            elapsed: Duration::ZERO,
        })
        .unwrap();
    autosave
        .execute(ProjectAutosaveCommand::SaveNow {
            elapsed: Duration::from_secs(1),
            snapshot: snapshot.clone(),
        })
        .unwrap()
        .published()
        .unwrap()
        .path()
        .to_path_buf()
}

fn dispatch(
    dispatcher: &mut EngineCommandDispatcher,
    transaction_id: &str,
    command: ProjectRecoveryCommand,
) -> superi_core::error::Result<superi_engine::dispatcher::EngineCommandOutcome> {
    dispatcher.dispatch(EngineCommandRequest::new(
        EngineTransactionId::new(transaction_id).unwrap(),
        EngineCommand::ExecuteProjectRecovery(command),
    ))
}

#[test]
fn discover_compare_restore_and_dismiss_are_one_atomic_engine_surface() {
    let _domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let directory = TempDirectory::new("round-trip");
    let active_path = directory.child("active.superi");
    let recovery_snapshot = document().snapshot();
    let candidate_path = publish_recovery(directory.path(), &recovery_snapshot);
    let current = with_mix_revision(changed_document(96_000), 1);
    let mut database = ProjectDatabase::create(&active_path).unwrap();
    database.replace(&current.snapshot()).unwrap();

    let mut dispatcher = EngineCommandDispatcher::new().unwrap();
    dispatcher.attach_project(current).unwrap();
    dispatcher
        .attach_project_recovery(database, directory.path())
        .unwrap();

    let discovered = dispatch(
        &mut dispatcher,
        "discover-recovery",
        ProjectRecoveryCommand::discover(),
    )
    .unwrap();
    let EngineCommandResult::ProjectRecovery(discovered) = discovered.result() else {
        panic!("expected project recovery result");
    };
    assert_eq!(discovered.operation(), ProjectRecoveryOperation::Discover);
    assert_eq!(discovered.state().catalog().candidates().len(), 1);
    assert_eq!(discovered.state().project_revision(), 1);
    let catalog_revision = discovered.state().catalog().revision();
    let candidate_id = discovered.state().catalog().candidates()[0].id();
    let events = dispatcher.drain_events().unwrap();
    assert_eq!(events.len(), 1);
    match events[0].event() {
        EngineEvent::ProjectRecoveryStateChanged(state) => {
            assert_eq!(state.as_ref(), discovered.state());
        }
        _ => panic!("expected project recovery replacement event"),
    }

    let compared = dispatch(
        &mut dispatcher,
        "compare-recovery",
        ProjectRecoveryCommand::compare(catalog_revision, candidate_id),
    )
    .unwrap();
    let EngineCommandResult::ProjectRecovery(compared) = compared.result() else {
        panic!("expected project recovery result");
    };
    assert_eq!(compared.operation(), ProjectRecoveryOperation::Compare);
    assert!(compared.comparison().unwrap().settings_changed());
    assert!(compared.comparison().unwrap().clip_mix_changed());
    assert!(dispatcher.drain_events().unwrap().is_empty());

    let restored = dispatch(
        &mut dispatcher,
        "restore-recovery",
        ProjectRecoveryCommand::restore(catalog_revision, 1, candidate_id),
    )
    .unwrap();
    let EngineCommandResult::ProjectRecovery(restored) = restored.result() else {
        panic!("expected project recovery result");
    };
    assert!(restored.restored());
    assert_eq!(restored.state().project_revision(), 2);
    assert_eq!(
        restored
            .state()
            .snapshot()
            .settings()
            .integer(AUDIO_SAMPLE_RATE_KEY),
        Some(48_000)
    );
    assert_eq!(restored.state().history().undo_depth(), 0);
    assert_eq!(restored.state().history().redo_depth(), 0);
    assert_eq!(restored.state().snapshot().clip_mix_state().revision(), 0);
    assert!(
        candidate_path.exists(),
        "restore retains the source candidate"
    );
    assert_eq!(
        ProjectDatabase::open_read_only(&active_path)
            .unwrap()
            .load()
            .unwrap()
            .snapshot(),
        restored.state().snapshot().clone()
    );
    let events = dispatcher.drain_events().unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].project_revision(), Some(2));

    let dismissed = dispatch(
        &mut dispatcher,
        "dismiss-recovery",
        ProjectRecoveryCommand::dismiss(catalog_revision, candidate_id),
    )
    .unwrap();
    let EngineCommandResult::ProjectRecovery(dismissed) = dismissed.result() else {
        panic!("expected project recovery result");
    };
    assert!(dismissed.dismissed());
    assert!(dismissed.state().catalog().candidates().is_empty());
    assert!(!candidate_path.exists());
}

#[test]
fn persistence_failure_and_event_backpressure_precede_all_authority_changes() {
    let _domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let directory = TempDirectory::new("failure");
    let active_path = directory.child("active.superi");
    let recovery_snapshot = document().snapshot();
    let candidate_path = publish_recovery(directory.path(), &recovery_snapshot);
    let current = changed_document(96_000);
    let mut writable = ProjectDatabase::create(&active_path).unwrap();
    writable.replace(&current.snapshot()).unwrap();
    drop(writable);
    let active_before = fs::read(&active_path).unwrap();

    let read_only = ProjectDatabase::open_read_only(&active_path).unwrap();
    let mut dispatcher = EngineCommandDispatcher::new().unwrap();
    dispatcher.attach_project(current).unwrap();
    dispatcher
        .attach_project_recovery(read_only, directory.path())
        .unwrap();
    let discovered = dispatch(
        &mut dispatcher,
        "discover-read-only",
        ProjectRecoveryCommand::discover(),
    )
    .unwrap();
    let EngineCommandResult::ProjectRecovery(discovered) = discovered.result() else {
        panic!("expected recovery result");
    };
    let catalog_revision = discovered.state().catalog().revision();
    let candidate_id = discovered.state().catalog().candidates()[0].id();
    dispatcher.drain_events().unwrap();

    let error = dispatch(
        &mut dispatcher,
        "restore-read-only",
        ProjectRecoveryCommand::restore(catalog_revision, 1, candidate_id),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::PermissionDenied);
    assert_eq!(dispatcher.project_snapshot().unwrap().revision(), 1);
    assert_eq!(fs::read(&active_path).unwrap(), active_before);
    assert!(candidate_path.exists());
    assert!(dispatcher.drain_events().unwrap().is_empty());

    for index in 0..MAX_PENDING_ENGINE_EVENTS {
        dispatch(
            &mut dispatcher,
            &format!("fill-recovery-events-{index}"),
            ProjectRecoveryCommand::discover(),
        )
        .unwrap();
    }
    let error = dispatch(
        &mut dispatcher,
        "blocked-dismissal",
        ProjectRecoveryCommand::dismiss(catalog_revision, candidate_id),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    assert!(candidate_path.exists());
    assert_eq!(fs::read(&active_path).unwrap(), active_before);
    assert_eq!(
        dispatcher.drain_events().unwrap().len(),
        MAX_PENDING_ENGINE_EVENTS
    );
}

#[test]
fn stale_and_terminal_restore_failures_publish_nothing() {
    let _domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let directory = TempDirectory::new("classified");
    let active_path = directory.child("active.superi");
    let candidate = changed_document(96_000).snapshot();
    let candidate_path = publish_recovery(directory.path(), &candidate);
    let baseline = document().snapshot();
    let exhausted = ProjectDocument::from_parts_with_settings(
        u64::MAX,
        baseline.editorial_project().clone(),
        baseline.root_timeline_id(),
        baseline.settings().clone(),
        baseline.graphs().cloned().collect::<Vec<ProjectGraph>>(),
    )
    .unwrap();
    let mut database = ProjectDatabase::create(&active_path).unwrap();
    database.replace(&exhausted.snapshot()).unwrap();
    let bytes_before = fs::read(&active_path).unwrap();

    let mut dispatcher = EngineCommandDispatcher::new().unwrap();
    dispatcher.attach_project(exhausted).unwrap();
    dispatcher
        .attach_project_recovery(database, directory.path())
        .unwrap();
    let discovered = dispatch(
        &mut dispatcher,
        "discover-classified",
        ProjectRecoveryCommand::discover(),
    )
    .unwrap();
    let EngineCommandResult::ProjectRecovery(discovered) = discovered.result() else {
        panic!("expected recovery result");
    };
    let catalog_revision = discovered.state().catalog().revision();
    let candidate_id = ProjectRecoveryCandidateId::new(1).unwrap();
    dispatcher.drain_events().unwrap();

    let stale = dispatch(
        &mut dispatcher,
        "stale-project-revision",
        ProjectRecoveryCommand::restore(catalog_revision, 0, candidate_id),
    )
    .unwrap_err();
    assert_eq!(stale.category(), ErrorCategory::Conflict);
    assert_eq!(stale.recoverability(), Recoverability::UserCorrectable);

    let terminal = dispatch(
        &mut dispatcher,
        "terminal-project-revision",
        ProjectRecoveryCommand::restore(catalog_revision, u64::MAX, candidate_id),
    )
    .unwrap_err();
    assert_eq!(terminal.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(terminal.recoverability(), Recoverability::Terminal);
    assert_eq!(fs::read(&active_path).unwrap(), bytes_before);
    assert!(candidate_path.exists());
    assert!(dispatcher.drain_events().unwrap().is_empty());
}
