use std::fs::{self, File, FileTimes};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use superi_core::error::ErrorCategory;
use superi_core::ids::{MediaId, ProjectId, TimelineId};
use superi_core::settings::SettingValue;
use superi_core::settings::{ComponentId, SemanticVersion, VersionIdentifier};
use superi_core::time::{RationalTime, Timebase};
use superi_project::autosave::{
    ProjectAutosaveCommand, ProjectAutosaveController, ProjectAutosaveDisposition,
    ProjectAutosaveOperation, ProjectAutosavePolicy,
};
use superi_project::document::{ProjectDocument, ProjectSnapshot};
use superi_project::extensions::{
    ProjectExtensionCommand, ProjectExtensionKind, ProjectExtensionLifecycle,
    ProjectExtensionRecord, ProjectExtensionRecordId,
};
use superi_project::media::{PortableRelativePath, ReferencedMediaPath};
use superi_project::settings::{
    ProjectSettingMutation, ProjectSettingsTransaction, AUDIO_SAMPLE_RATE_KEY,
};
use superi_project::ProjectDatabase;
use superi_timeline::model::{EditorialProject, LinkedMediaReference, Timeline};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

const PROJECT: ProjectId = ProjectId::from_raw(0x9000);
const ROOT: TimelineId = TimelineId::from_raw(0x9001);
const MEDIA: MediaId = MediaId::from_raw(0x9002);

struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "superi-autosave-{label}-{}-{}",
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
        "autosave timeline",
        rate,
        RationalTime::zero(rate),
        vec![],
    );
    let media_path = ReferencedMediaPath::project_relative(
        PortableRelativePath::new("Media/source.webm").unwrap(),
    );
    let media = LinkedMediaReference::with_fingerprint(
        MEDIA,
        "autosave source",
        media_path.to_target(),
        None,
        "sha256:autosave-contract",
    )
    .unwrap();
    let editorial =
        EditorialProject::new(PROJECT, "autosave project", [media], [timeline]).unwrap();
    let mut document = ProjectDocument::new(editorial, ROOT).unwrap();
    document
        .execute_settings_transaction(
            ProjectSettingsTransaction::new(
                0,
                vec![ProjectSettingMutation::set(
                    AUDIO_SAMPLE_RATE_KEY,
                    SettingValue::Integer(96_000),
                )
                .unwrap()],
            )
            .unwrap(),
        )
        .unwrap();
    let extension = ProjectExtensionRecord::new(
        ComponentId::new("example.autosave-extension").unwrap(),
        ProjectExtensionRecordId::new("opaque-recovery-state").unwrap(),
        SemanticVersion::new(1, 2, 3),
        ProjectExtensionKind::new(ComponentId::new("example.future-kind").unwrap()),
        VersionIdentifier::new(
            ComponentId::new("example.future-state").unwrap(),
            SemanticVersion::new(4, 5, 6),
        ),
        Default::default(),
        Default::default(),
        ProjectExtensionLifecycle::Enabled,
        None,
        vec![0, 1, 0xfe, 0xff],
    )
    .unwrap();
    document
        .execute_extension_command(
            document.revision(),
            ProjectExtensionCommand::upsert(extension),
        )
        .unwrap();
    document
}

fn policy(root: &Path, enabled: bool, interval: u64, retention: usize) -> ProjectAutosavePolicy {
    ProjectAutosavePolicy::new(enabled, Duration::from_secs(interval), root, retention).unwrap()
}

fn assert_artifact(artifact: &Path, expected: &ProjectSnapshot) {
    let database = ProjectDatabase::open_read_only(artifact).unwrap();
    assert_eq!(database.load().unwrap().snapshot(), *expected);
}

fn project_directory(root: &Path) -> PathBuf {
    root.join(format!("project-{:032x}", PROJECT.raw()))
}

fn managed_names(root: &Path) -> Vec<String> {
    let mut names = fs::read_dir(project_directory(root))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .filter(|name| {
            let Some(digits) = name
                .strip_prefix("autosave-g")
                .and_then(|value| value.strip_suffix(".superi"))
            else {
                return false;
            };
            digits.len() == 20
                && digits.bytes().all(|byte| byte.is_ascii_digit())
                && digits.parse::<u64>().is_ok_and(|generation| generation > 0)
        })
        .collect::<Vec<_>>();
    names.sort();
    names
}

fn set_modified(path: &Path, seconds: u64) -> bool {
    File::options()
        .write(true)
        .open(path)
        .and_then(|file| {
            file.set_times(
                FileTimes::new()
                    .set_modified(SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)),
            )
        })
        .is_ok()
}

#[test]
fn scheduling_manual_control_and_current_format_publication_share_one_command_surface() {
    let directory = TempDirectory::new("scheduling");
    let active = directory.child("active.superi");
    let mut document = document();
    let original = document.snapshot();
    let mut active_database = ProjectDatabase::create(&active).unwrap();
    active_database.replace(&original).unwrap();
    let active_bytes = fs::read(&active).unwrap();

    let mut controller = ProjectAutosaveController::new(PROJECT).unwrap();
    let configured = controller
        .execute(ProjectAutosaveCommand::Configure {
            policy: policy(directory.path(), true, 10, 2),
            elapsed: Duration::ZERO,
        })
        .unwrap();
    assert_eq!(configured.operation(), ProjectAutosaveOperation::Configure);
    assert_eq!(
        configured.disposition(),
        ProjectAutosaveDisposition::Configured
    );
    assert_eq!(configured.state().next_due(), Some(Duration::from_secs(10)));
    assert_eq!(configured.state().managed_count(), 0);

    let early = controller
        .execute(ProjectAutosaveCommand::Tick {
            elapsed: Duration::from_secs(9),
            snapshot: original.clone(),
        })
        .unwrap();
    assert_eq!(early.disposition(), ProjectAutosaveDisposition::NotDue);
    assert!(early.published().is_none());

    let due = controller
        .execute(ProjectAutosaveCommand::Tick {
            elapsed: Duration::from_secs(10),
            snapshot: original.clone(),
        })
        .unwrap();
    assert_eq!(due.disposition(), ProjectAutosaveDisposition::Published);
    let first = due.published().unwrap();
    assert_eq!(first.generation(), 1);
    assert_eq!(first.project_revision(), original.revision());
    assert_artifact(first.path(), &original);
    assert_eq!(fs::read(&active).unwrap(), active_bytes);

    let unchanged = controller
        .execute(ProjectAutosaveCommand::Tick {
            elapsed: Duration::from_secs(20),
            snapshot: original.clone(),
        })
        .unwrap();
    assert_eq!(
        unchanged.disposition(),
        ProjectAutosaveDisposition::Unchanged
    );
    assert_eq!(unchanged.state().managed_count(), 1);
    assert_eq!(unchanged.state().next_due(), Some(Duration::from_secs(30)));

    let manual = controller
        .execute(ProjectAutosaveCommand::SaveNow {
            elapsed: Duration::from_secs(21),
            snapshot: original.clone(),
        })
        .unwrap();
    assert_eq!(manual.disposition(), ProjectAutosaveDisposition::Published);
    assert_eq!(manual.published().unwrap().generation(), 2);
    assert_artifact(manual.published().unwrap().path(), &original);

    let before_backward = controller
        .execute(ProjectAutosaveCommand::Inspect)
        .unwrap()
        .state()
        .clone();
    let backward = controller
        .execute(ProjectAutosaveCommand::Tick {
            elapsed: Duration::from_secs(20),
            snapshot: original.clone(),
        })
        .unwrap_err();
    assert_eq!(backward.category(), ErrorCategory::Conflict);
    assert_eq!(
        controller
            .execute(ProjectAutosaveCommand::Inspect)
            .unwrap()
            .state(),
        &before_backward
    );

    let disabled = controller
        .execute(ProjectAutosaveCommand::Configure {
            policy: policy(directory.path(), false, 10, 1),
            elapsed: Duration::from_secs(22),
        })
        .unwrap();
    assert_eq!(disabled.state().managed_count(), 1);
    assert_eq!(disabled.removed_count(), 1);
    assert_eq!(disabled.state().next_due(), None);

    let disabled_tick = controller
        .execute(ProjectAutosaveCommand::Tick {
            elapsed: Duration::from_secs(100),
            snapshot: original.clone(),
        })
        .unwrap();
    assert_eq!(
        disabled_tick.disposition(),
        ProjectAutosaveDisposition::Disabled
    );

    let disabled_manual = controller
        .execute(ProjectAutosaveCommand::SaveNow {
            elapsed: Duration::from_secs(101),
            snapshot: original.clone(),
        })
        .unwrap();
    assert_eq!(
        disabled_manual.disposition(),
        ProjectAutosaveDisposition::Published
    );
    assert_eq!(disabled_manual.state().managed_count(), 1);

    let revision = document.revision();
    document
        .execute_settings_transaction(
            ProjectSettingsTransaction::new(
                revision,
                vec![ProjectSettingMutation::set(
                    AUDIO_SAMPLE_RATE_KEY,
                    SettingValue::Integer(48_000),
                )
                .unwrap()],
            )
            .unwrap(),
        )
        .unwrap();
    let changed = document.snapshot();
    controller
        .execute(ProjectAutosaveCommand::Configure {
            policy: policy(directory.path(), true, 10, 1),
            elapsed: Duration::from_secs(102),
        })
        .unwrap();
    let jump = controller
        .execute(ProjectAutosaveCommand::Tick {
            elapsed: Duration::from_secs(1_000),
            snapshot: changed.clone(),
        })
        .unwrap();
    assert_eq!(jump.disposition(), ProjectAutosaveDisposition::Published);
    assert_eq!(jump.state().managed_count(), 1);
    assert_eq!(jump.state().next_due(), Some(Duration::from_secs(1_010)));
    assert_artifact(jump.published().unwrap().path(), &changed);
    assert_eq!(managed_names(directory.path()).len(), 1);
    assert_eq!(fs::read(&active).unwrap(), active_bytes);
}

#[test]
fn missing_periodic_recovery_is_republished_at_the_next_due_tick() {
    let directory = TempDirectory::new("missing-periodic");
    let snapshot = document().snapshot();
    let mut controller = ProjectAutosaveController::new(PROJECT).unwrap();
    controller
        .execute(ProjectAutosaveCommand::Configure {
            policy: policy(directory.path(), true, 5, 1),
            elapsed: Duration::ZERO,
        })
        .unwrap();

    let first = controller
        .execute(ProjectAutosaveCommand::Tick {
            elapsed: Duration::from_secs(5),
            snapshot: snapshot.clone(),
        })
        .unwrap();
    let first_path = first.published().unwrap().path().to_path_buf();
    fs::remove_file(&first_path).unwrap();

    let replacement = controller
        .execute(ProjectAutosaveCommand::Tick {
            elapsed: Duration::from_secs(10),
            snapshot: snapshot.clone(),
        })
        .unwrap();
    assert_eq!(
        replacement.disposition(),
        ProjectAutosaveDisposition::Published
    );
    assert_eq!(replacement.published().unwrap().generation(), 1);
    assert_artifact(replacement.published().unwrap().path(), &snapshot);
}

#[test]
fn retention_pruning_is_generation_ordered_and_preserves_foreign_entries() {
    let directory = TempDirectory::new("retention");
    let snapshot = document().snapshot();
    let mut controller = ProjectAutosaveController::new(PROJECT).unwrap();
    controller
        .execute(ProjectAutosaveCommand::Configure {
            policy: policy(directory.path(), false, 30, 3),
            elapsed: Duration::ZERO,
        })
        .unwrap();

    let project_directory = project_directory(directory.path());
    fs::write(project_directory.join("foreign.txt"), b"keep me").unwrap();
    fs::write(
        project_directory.join(".project.superi-save-candidate-owned"),
        b"candidate evidence",
    )
    .unwrap();
    let lookalikes = [
        "autosave-g1.superi",
        "autosave-g00000000000000000000.superi",
        "autosave-g000000000000000000001.superi",
        "autosave-g00000000000000000001.superi.bak",
    ];
    for lookalike in lookalikes {
        fs::write(project_directory.join(lookalike), b"foreign lookalike").unwrap();
    }

    for second in 1..=4 {
        controller
            .execute(ProjectAutosaveCommand::SaveNow {
                elapsed: Duration::from_secs(second),
                snapshot: snapshot.clone(),
            })
            .unwrap();
    }
    assert_eq!(
        managed_names(directory.path()),
        vec![
            "autosave-g00000000000000000002.superi",
            "autosave-g00000000000000000003.superi",
            "autosave-g00000000000000000004.superi",
        ]
    );
    assert_eq!(
        fs::read(project_directory.join("foreign.txt")).unwrap(),
        b"keep me"
    );
    assert_eq!(
        fs::read(project_directory.join(".project.superi-save-candidate-owned")).unwrap(),
        b"candidate evidence"
    );
    for lookalike in lookalikes {
        assert_eq!(
            fs::read(project_directory.join(lookalike)).unwrap(),
            b"foreign lookalike"
        );
    }

    let reversed_mtimes = [
        ("autosave-g00000000000000000002.superi", 300),
        ("autosave-g00000000000000000003.superi", 200),
        ("autosave-g00000000000000000004.superi", 100),
    ]
    .into_iter()
    .all(|(name, seconds)| set_modified(&project_directory.join(name), seconds));
    if reversed_mtimes {
        assert!(
            fs::metadata(project_directory.join("autosave-g00000000000000000002.superi"))
                .unwrap()
                .modified()
                .unwrap()
                > fs::metadata(project_directory.join("autosave-g00000000000000000004.superi"))
                    .unwrap()
                    .modified()
                    .unwrap()
        );
    }

    let lowered = controller
        .execute(ProjectAutosaveCommand::Configure {
            policy: policy(directory.path(), false, 30, 1),
            elapsed: Duration::from_secs(5),
        })
        .unwrap();
    assert_eq!(lowered.removed_count(), 2);
    assert_eq!(
        managed_names(directory.path()),
        vec!["autosave-g00000000000000000004.superi"]
    );

    fs::write(
        project_directory.join("autosave-g00000000000000000005.superi"),
        b"older schema bytes remain opaque",
    )
    .unwrap();
    let bytes = fs::read(project_directory.join("autosave-g00000000000000000005.superi")).unwrap();
    let inspected = controller.execute(ProjectAutosaveCommand::Inspect).unwrap();
    assert_eq!(
        inspected.disposition(),
        ProjectAutosaveDisposition::Inspected
    );
    assert_eq!(inspected.state().managed_count(), 2);
    assert_eq!(
        fs::read(project_directory.join("autosave-g00000000000000000005.superi")).unwrap(),
        bytes
    );

    let pruned = controller.execute(ProjectAutosaveCommand::Prune).unwrap();
    assert_eq!(pruned.disposition(), ProjectAutosaveDisposition::Pruned);
    assert_eq!(pruned.removed_count(), 1);
    assert_eq!(
        managed_names(directory.path()),
        vec!["autosave-g00000000000000000005.superi"]
    );
}

#[cfg(unix)]
#[test]
fn managed_namespace_tampering_blocks_every_delete() {
    use std::os::unix::fs::symlink;

    let directory = TempDirectory::new("tamper");
    let project_directory = project_directory(directory.path());
    fs::create_dir(&project_directory).unwrap();
    let valid = project_directory.join("autosave-g00000000000000000001.superi");
    fs::write(&valid, b"must remain").unwrap();
    let link = project_directory.join("autosave-g00000000000000000002.superi");
    symlink(&valid, &link).unwrap();

    let mut controller = ProjectAutosaveController::new(PROJECT).unwrap();
    let error = controller
        .execute(ProjectAutosaveCommand::Configure {
            policy: policy(directory.path(), false, 30, 1),
            elapsed: Duration::ZERO,
        })
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(fs::read(&valid).unwrap(), b"must remain");
    assert!(fs::symlink_metadata(&link)
        .unwrap()
        .file_type()
        .is_symlink());
}

#[test]
fn policy_identity_time_and_generation_bounds_fail_without_clobber() {
    let directory = TempDirectory::new("bounds");
    let root_file = directory.child("not-a-directory");
    fs::write(&root_file, b"root file").unwrap();
    assert_eq!(
        ProjectAutosavePolicy::new(true, Duration::ZERO, directory.path(), 1)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        ProjectAutosavePolicy::new(true, Duration::from_secs(1), directory.path(), 0)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        ProjectAutosavePolicy::new(true, Duration::from_secs(1), directory.path(), 4_097)
            .unwrap_err()
            .category(),
        ErrorCategory::ResourceExhausted
    );
    assert_eq!(
        ProjectAutosavePolicy::new(true, Duration::from_secs(1), directory.child("missing"), 1,)
            .unwrap_err()
            .category(),
        ErrorCategory::NotFound
    );
    assert_eq!(
        ProjectAutosavePolicy::new(true, Duration::from_secs(1), &root_file, 1)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );

    let mut overflow_controller = ProjectAutosaveController::new(PROJECT).unwrap();
    let overflow = overflow_controller
        .execute(ProjectAutosaveCommand::Configure {
            policy: ProjectAutosavePolicy::new(true, Duration::MAX, directory.path(), 1).unwrap(),
            elapsed: Duration::from_nanos(1),
        })
        .unwrap_err();
    assert_eq!(overflow.category(), ErrorCategory::ResourceExhausted);
    assert!(!project_directory(directory.path()).exists());
    assert!(overflow_controller
        .execute(ProjectAutosaveCommand::Inspect)
        .unwrap()
        .state()
        .policy()
        .is_none());

    let snapshot = document().snapshot();
    let other = {
        let rate = Timebase::integer(24).unwrap();
        let timeline = Timeline::new(
            TimelineId::from_raw(0xa001),
            "other timeline",
            rate,
            RationalTime::zero(rate),
            vec![],
        );
        let editorial =
            EditorialProject::new(ProjectId::from_raw(0xa000), "other project", [], [timeline])
                .unwrap();
        ProjectDocument::new(editorial, TimelineId::from_raw(0xa001))
            .unwrap()
            .snapshot()
    };
    let mut controller = ProjectAutosaveController::new(PROJECT).unwrap();
    controller
        .execute(ProjectAutosaveCommand::Configure {
            policy: policy(directory.path(), false, 1, 2),
            elapsed: Duration::ZERO,
        })
        .unwrap();
    let mismatch = controller
        .execute(ProjectAutosaveCommand::SaveNow {
            elapsed: Duration::from_secs(1),
            snapshot: other,
        })
        .unwrap_err();
    assert_eq!(mismatch.category(), ErrorCategory::Conflict);
    assert!(managed_names(directory.path()).is_empty());

    let project_directory = project_directory(directory.path());
    let first = project_directory.join("autosave-g00000000000000000001.superi");
    fs::write(&first, b"preexisting recovery evidence").unwrap();
    let published = controller
        .execute(ProjectAutosaveCommand::SaveNow {
            elapsed: Duration::from_secs(1),
            snapshot: snapshot.clone(),
        })
        .unwrap();
    assert_eq!(published.published().unwrap().generation(), 2);
    assert_eq!(fs::read(&first).unwrap(), b"preexisting recovery evidence");

    let exhausted = project_directory.join("autosave-g18446744073709551615.superi");
    fs::write(&exhausted, b"generation sentinel").unwrap();
    let before_failure = controller
        .execute(ProjectAutosaveCommand::Inspect)
        .unwrap()
        .state()
        .clone();
    let error = controller
        .execute(ProjectAutosaveCommand::SaveNow {
            elapsed: Duration::from_secs(2),
            snapshot: snapshot.clone(),
        })
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(fs::read(&exhausted).unwrap(), b"generation sentinel");
    assert_eq!(
        controller
            .execute(ProjectAutosaveCommand::Inspect)
            .unwrap()
            .state(),
        &before_failure
    );

    fs::remove_file(&exhausted).unwrap();
    let retried = controller
        .execute(ProjectAutosaveCommand::SaveNow {
            elapsed: Duration::from_secs(2),
            snapshot,
        })
        .unwrap();
    assert_eq!(retried.published().unwrap().generation(), 3);
}
