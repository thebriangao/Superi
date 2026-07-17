use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::ids::{ProjectId, TimelineId};
use superi_core::settings::SettingValue;
use superi_core::time::{RationalTime, Timebase};
use superi_project::autosave::{
    ProjectAutosaveCommand, ProjectAutosaveController, ProjectAutosavePolicy,
};
use superi_project::document::{ProjectDocument, ProjectSnapshot};
use superi_project::recovery::{
    ProjectRecoveryCandidateId, ProjectRecoveryController, ProjectRecoveryNextAction,
};
use superi_project::settings::{
    ProjectSettingMutation, ProjectSettingsTransaction, AUDIO_SAMPLE_RATE_KEY,
};
use superi_project::{ProjectDatabase, ProjectSaveCommand};
use superi_timeline::model::{EditorialProject, Timeline};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

const PROJECT: ProjectId = ProjectId::from_raw(0xc100);
const ROOT: TimelineId = TimelineId::from_raw(0xc101);

struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "superi-recovery-{label}-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn document(project_id: ProjectId, root: TimelineId) -> ProjectDocument {
    let rate = Timebase::integer(24).unwrap();
    let timeline = Timeline::new(
        root,
        "recovery timeline",
        rate,
        RationalTime::zero(rate),
        vec![],
    );
    let editorial = EditorialProject::new(project_id, "recovery project", [], [timeline]).unwrap();
    ProjectDocument::new(editorial, root).unwrap()
}

fn changed_snapshot(mut document: ProjectDocument, sample_rate: i64) -> ProjectSnapshot {
    let revision = document.revision();
    document
        .execute_settings_transaction(
            ProjectSettingsTransaction::new(
                revision,
                vec![ProjectSettingMutation::set(
                    AUDIO_SAMPLE_RATE_KEY,
                    SettingValue::Integer(sample_rate),
                )
                .unwrap()],
            )
            .unwrap(),
        )
        .unwrap();
    document.snapshot()
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

fn publish(root: &Path, snapshots: &[ProjectSnapshot]) -> Vec<PathBuf> {
    let mut autosave = ProjectAutosaveController::new(PROJECT).unwrap();
    autosave
        .execute(ProjectAutosaveCommand::Configure {
            policy: ProjectAutosavePolicy::new(false, Duration::from_secs(60), root, 16).unwrap(),
            elapsed: Duration::ZERO,
        })
        .unwrap();
    snapshots
        .iter()
        .enumerate()
        .map(|(index, snapshot)| {
            autosave
                .execute(ProjectAutosaveCommand::SaveNow {
                    elapsed: Duration::from_secs(index as u64 + 1),
                    snapshot: snapshot.clone(),
                })
                .unwrap()
                .published()
                .unwrap()
                .path()
                .to_path_buf()
        })
        .collect()
}

#[test]
fn discovery_comparison_and_failure_findings_share_the_c009_namespace() {
    let directory = TempDirectory::new("discover");
    let first = document(PROJECT, ROOT).snapshot();
    let second = changed_snapshot(document(PROJECT, ROOT), 96_000);
    let paths = publish(directory.path(), &[first.clone(), second.clone()]);
    let project_directory = paths[0].parent().unwrap();
    fs::write(project_directory.join("foreign.txt"), b"foreign").unwrap();
    fs::write(
        project_directory.join(".autosave.superi-save-candidate-7"),
        b"candidate",
    )
    .unwrap();
    fs::write(
        project_directory.join("autosave-g00000000000000000002.superi-wal"),
        b"sidecar",
    )
    .unwrap();

    let mut recovery = ProjectRecoveryController::new(PROJECT, directory.path()).unwrap();
    let catalog = recovery.discover().unwrap().clone();
    assert_eq!(catalog.revision(), 1);
    assert_eq!(catalog.candidates().len(), 2);
    assert!(catalog.findings().is_empty());
    assert_eq!(
        catalog
            .candidates()
            .iter()
            .map(|candidate| candidate.id().generation())
            .collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(
        recovery.discover().unwrap().revision(),
        catalog.revision(),
        "an unchanged scan keeps the catalog revision stable"
    );

    let current = changed_snapshot(with_mix_revision(document(PROJECT, ROOT), 1), 44_100);
    let comparison = recovery
        .compare(
            catalog.revision(),
            ProjectRecoveryCandidateId::new(2).unwrap(),
            &current,
        )
        .unwrap();
    assert_eq!(comparison.candidate_revision(), second.revision());
    assert!(comparison.settings_changed());
    assert!(comparison.clip_mix_changed());
    assert!(comparison.has_changes());

    fs::write(&paths[1], b"not sqlite").unwrap();
    let degraded = recovery.discover().unwrap().clone();
    assert_eq!(degraded.candidates().len(), 1);
    assert_eq!(degraded.findings().len(), 1);
    assert_eq!(
        degraded.findings()[0].diagnostic().recoverability(),
        Recoverability::Degraded
    );
    assert_eq!(
        degraded.findings()[0].next_action(),
        ProjectRecoveryNextAction::ContinueWithOtherCandidate
    );
    assert!(degraded.findings()[0]
        .diagnostic()
        .contexts()
        .iter()
        .any(|context| context.field("path").is_some()));
}

#[test]
fn wrong_project_and_symlink_entries_are_user_correctable_without_following_them() {
    let directory = TempDirectory::new("unsafe-entry");
    let snapshot = document(PROJECT, ROOT).snapshot();
    let paths = publish(directory.path(), &[snapshot]);
    let project_directory = paths[0].parent().unwrap();
    let other = document(ProjectId::from_raw(0xc200), TimelineId::from_raw(0xc201)).snapshot();
    let mut publisher = ProjectDatabase::memory().unwrap();
    publisher
        .execute_save_command(
            ProjectSaveCommand::Backup {
                destination: project_directory.join("autosave-g00000000000000000002.superi"),
            },
            &other,
        )
        .unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(
            &paths[0],
            project_directory.join("autosave-g00000000000000000003.superi"),
        )
        .unwrap();
    }

    let mut recovery = ProjectRecoveryController::new(PROJECT, directory.path()).unwrap();
    let catalog = recovery.discover().unwrap();
    assert_eq!(catalog.candidates().len(), 1);
    assert!(catalog
        .findings()
        .iter()
        .all(|finding| finding.diagnostic().recoverability() == Recoverability::UserCorrectable));
    assert!(catalog
        .findings()
        .iter()
        .all(|finding| finding.next_action() == ProjectRecoveryNextAction::CorrectRecoveryStore));
}

#[test]
fn dismissal_is_exact_durable_and_preserves_every_other_entry() {
    let directory = TempDirectory::new("dismiss");
    let first = document(PROJECT, ROOT).snapshot();
    let second = changed_snapshot(document(PROJECT, ROOT), 96_000);
    let paths = publish(directory.path(), &[first, second]);
    let project_directory = paths[0].parent().unwrap();
    let foreign = project_directory.join("foreign.txt");
    fs::write(&foreign, b"preserve").unwrap();

    let mut recovery = ProjectRecoveryController::new(PROJECT, directory.path()).unwrap();
    let revision = recovery.discover().unwrap().revision();
    let catalog = recovery
        .dismiss(revision, ProjectRecoveryCandidateId::new(1).unwrap())
        .unwrap();
    assert_eq!(catalog.candidates().len(), 1);
    assert_eq!(catalog.candidates()[0].id().generation(), 2);
    assert!(!paths[0].exists());
    assert!(paths[1].exists());
    assert_eq!(fs::read(&foreign).unwrap(), b"preserve");
    assert!(fs::read_dir(project_directory).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .contains("dismissed")));

    drop(recovery);
    let mut reopened = ProjectRecoveryController::new(PROJECT, directory.path()).unwrap();
    let reopened = reopened.discover().unwrap();
    assert_eq!(reopened.candidates().len(), 1);
    assert_eq!(reopened.candidates()[0].id().generation(), 2);
}

#[test]
fn stale_file_identity_is_retryable_and_restart_cleans_dismissal_tombstones() {
    let directory = TempDirectory::new("stale-and-restart");
    let snapshot = document(PROJECT, ROOT).snapshot();
    let paths = publish(directory.path(), std::slice::from_ref(&snapshot));
    let candidate = &paths[0];
    let project_directory = candidate.parent().unwrap();

    let mut recovery = ProjectRecoveryController::new(PROJECT, directory.path()).unwrap();
    let catalog = recovery.discover().unwrap().clone();
    fs::write(candidate, b"changed after discovery").unwrap();
    let changed_bytes = fs::read(candidate).unwrap();
    let error = recovery
        .compare(
            catalog.revision(),
            ProjectRecoveryCandidateId::new(1).unwrap(),
            &snapshot,
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::Retryable);
    assert_eq!(fs::read(candidate).unwrap(), changed_bytes);
    assert_eq!(recovery.catalog(), &catalog);

    let tombstone = project_directory.join(".autosave-dismissed-g00000000000000000001.superi");
    fs::rename(candidate, &tombstone).unwrap();
    drop(recovery);

    let mut reopened = ProjectRecoveryController::new(PROJECT, directory.path()).unwrap();
    let catalog = reopened.discover().unwrap();
    assert!(catalog.candidates().is_empty());
    assert!(catalog.findings().is_empty());
    assert!(!tombstone.exists());
}
