use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use superi_desktop::project_lifecycle::{
    DesktopProjectBackend, DesktopProjectCommand, DesktopProjectCreateRequest,
    DesktopProjectFailure, DesktopProjectFailureClass, DesktopProjectIdentity,
    DesktopProjectLifecycle, DesktopProjectState, DesktopRecoveryCandidate, DesktopRecoveryCatalog,
};

#[derive(Clone, Default)]
struct OperationLog(Arc<Mutex<Vec<String>>>);

impl OperationLog {
    fn push(&self, value: impl Into<String>) {
        self.0.lock().unwrap().push(value.into());
    }

    fn snapshot(&self) -> Vec<String> {
        self.0.lock().unwrap().clone()
    }
}

struct DeterministicBackend {
    log: OperationLog,
}

impl DeterministicBackend {
    fn identity(path: &Path, revision: u64) -> DesktopProjectIdentity {
        let stem = path.file_stem().unwrap().to_string_lossy();
        DesktopProjectIdentity::new(
            format!("project-{stem}"),
            revision,
            format!("timeline-{stem}"),
        )
        .unwrap()
    }

    fn classified_open_failure(path: &Path) -> Option<DesktopProjectFailure> {
        let display = path.to_string_lossy();
        let (class, code, action) = if display.contains("retryable") {
            (
                DesktopProjectFailureClass::Retryable,
                "project_busy",
                "Retry after the other project writer finishes.",
            )
        } else if display.contains("degraded") {
            (
                DesktopProjectFailureClass::Degraded,
                "recovery_partial",
                "Continue with the last valid project and inspect another recovery point.",
            )
        } else if display.contains("correctable") {
            (
                DesktopProjectFailureClass::UserCorrectable,
                "project_path_invalid",
                "Choose an accessible Superi project file.",
            )
        } else if display.contains("terminal") {
            (
                DesktopProjectFailureClass::Terminal,
                "project_runtime_failed",
                "Restart Superi before opening another project.",
            )
        } else {
            return None;
        };
        Some(
            DesktopProjectFailure::new(class, code, "Project open failed", action)
                .with_context("operation", "open"),
        )
    }
}

impl DesktopProjectBackend for DeterministicBackend {
    fn create(
        &mut self,
        path: &Path,
        _request: &DesktopProjectCreateRequest,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        self.log.push(format!("create:{}", path.display()));
        Ok(Self::identity(path, 0))
    }

    fn open(&mut self, path: &Path) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        self.log.push(format!("open:{}", path.display()));
        if let Some(failure) = Self::classified_open_failure(path) {
            return Err(failure);
        }
        Ok(Self::identity(path, 2))
    }

    fn save(&mut self, path: &Path) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        self.log.push(format!("save:{}", path.display()));
        Ok(Self::identity(path, 3))
    }

    fn save_as(
        &mut self,
        path: &Path,
        destination: &Path,
        replace_existing: bool,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        self.log.push(format!(
            "save_as:{}:{}:{replace_existing}",
            path.display(),
            destination.display()
        ));
        Ok(Self::identity(destination, 4))
    }

    fn discover_recovery(
        &mut self,
        path: &Path,
    ) -> Result<DesktopRecoveryCatalog, DesktopProjectFailure> {
        self.log
            .push(format!("discover_recovery:{}", path.display()));
        DesktopRecoveryCatalog::new(
            9,
            vec![
                DesktopRecoveryCandidate::new("candidate-7", 7, "Recover autosaved revision 7")
                    .unwrap(),
            ],
        )
    }

    fn restore_recovery(
        &mut self,
        path: &Path,
        catalog_revision: u64,
        candidate_id: &str,
    ) -> Result<DesktopProjectIdentity, DesktopProjectFailure> {
        self.log.push(format!(
            "restore_recovery:{}:{catalog_revision}:{candidate_id}",
            path.display()
        ));
        Ok(Self::identity(path, 7))
    }
}

fn create_request(seed: &str) -> DesktopProjectCreateRequest {
    DesktopProjectCreateRequest {
        project_id: format!("project-{seed}"),
        project_name: format!("Project {seed}"),
        root_timeline_id: format!("timeline-{seed}"),
        root_timeline_name: format!("Timeline {seed}"),
        edit_rate_numerator: 24,
        edit_rate_denominator: 1,
    }
}

struct TemporaryProjectSession {
    root: PathBuf,
}

static NEXT_TEMPORARY_PROJECT_SESSION: AtomicU64 = AtomicU64::new(0);

impl TemporaryProjectSession {
    fn new() -> Self {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let ordinal = NEXT_TEMPORARY_PROJECT_SESSION.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "superi-desktop-session-{}-{suffix}-{ordinal}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn recovery_root(&self) -> PathBuf {
        self.root.join("recovery")
    }

    fn project(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }
}

impl Drop for TemporaryProjectSession {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn project_lifecycle_commits_authority_and_preserves_actionable_failure_context() {
    let log = OperationLog::default();
    let mut lifecycle =
        DesktopProjectLifecycle::new(DeterministicBackend { log: log.clone() }, 2).unwrap();

    let created = lifecycle
        .execute(DesktopProjectCommand::Create {
            path: "alpha.superi".into(),
            project: create_request("alpha"),
        })
        .unwrap();
    assert_eq!(created.active().unwrap().path(), "alpha.superi");
    assert_eq!(created.active().unwrap().project_revision(), 0);
    assert_eq!(created.recent()[0].path(), "alpha.superi");

    let saved = lifecycle.execute(DesktopProjectCommand::Save).unwrap();
    assert_eq!(saved.active().unwrap().project_revision(), 3);

    let editor_identity =
        DesktopProjectIdentity::new("project-alpha", 4, "timeline-alpha").unwrap();
    let edited = lifecycle
        .refresh_active_from_editor(
            "alpha.superi",
            "project-alpha",
            3,
            "timeline-alpha",
            editor_identity,
        )
        .unwrap();
    assert_eq!(edited.active().unwrap().project_revision(), 4);
    let stale = lifecycle
        .refresh_active_from_editor(
            "alpha.superi",
            "project-alpha",
            3,
            "timeline-alpha",
            DesktopProjectIdentity::new("project-alpha", 5, "timeline-alpha").unwrap(),
        )
        .unwrap_err();
    assert_eq!(stale.class(), DesktopProjectFailureClass::UserCorrectable);
    assert_eq!(lifecycle.snapshot().active().unwrap().project_revision(), 4);
    let skipped_revision = lifecycle
        .refresh_active_from_editor(
            "alpha.superi",
            "project-alpha",
            4,
            "timeline-alpha",
            DesktopProjectIdentity::new("project-alpha", 6, "timeline-alpha").unwrap(),
        )
        .unwrap_err();
    assert_eq!(
        skipped_revision.class(),
        DesktopProjectFailureClass::UserCorrectable
    );
    assert_eq!(lifecycle.snapshot().active().unwrap().project_revision(), 4);
    let unversioned_root_change = lifecycle
        .refresh_active_from_editor(
            "alpha.superi",
            "project-alpha",
            4,
            "timeline-alpha",
            DesktopProjectIdentity::new("project-alpha", 4, "timeline-alpha-alternate").unwrap(),
        )
        .unwrap_err();
    assert_eq!(
        unversioned_root_change.class(),
        DesktopProjectFailureClass::UserCorrectable
    );
    let changed_root = lifecycle
        .refresh_active_from_editor(
            "alpha.superi",
            "project-alpha",
            4,
            "timeline-alpha",
            DesktopProjectIdentity::new("project-alpha", 5, "timeline-alpha-alternate").unwrap(),
        )
        .unwrap();
    assert_eq!(changed_root.active().unwrap().project_revision(), 5);
    assert_eq!(
        changed_root.active().unwrap().root_timeline_id(),
        "timeline-alpha-alternate"
    );

    let saved_as = lifecycle
        .execute(DesktopProjectCommand::SaveAs {
            destination: "beta.superi".into(),
            replace_existing: false,
        })
        .unwrap();
    assert_eq!(saved_as.active().unwrap().path(), "beta.superi");
    assert_eq!(saved_as.active().unwrap().project_revision(), 4);
    assert_eq!(saved_as.recent()[0].path(), "beta.superi");
    assert_eq!(saved_as.recent()[1].path(), "alpha.superi");

    let opened = lifecycle
        .execute(DesktopProjectCommand::Open {
            path: "gamma.superi".into(),
        })
        .unwrap();
    assert_eq!(opened.active().unwrap().path(), "gamma.superi");
    assert_eq!(opened.recent().len(), 2);
    assert_eq!(opened.recent()[0].path(), "gamma.superi");
    assert_eq!(opened.recent()[1].path(), "beta.superi");

    let reopened = lifecycle
        .execute(DesktopProjectCommand::OpenRecent {
            path: "beta.superi".into(),
        })
        .unwrap();
    assert_eq!(reopened.active().unwrap().path(), "beta.superi");
    assert_eq!(reopened.recent()[0].path(), "beta.superi");
    assert_eq!(reopened.recent()[1].path(), "gamma.superi");

    let discovered = lifecycle
        .execute(DesktopProjectCommand::DiscoverRecovery)
        .unwrap();
    let recovery = discovered.recovery().unwrap();
    assert_eq!(recovery.catalog_revision(), 9);
    assert_eq!(recovery.candidates()[0].candidate_id(), "candidate-7");
    assert_eq!(recovery.candidates()[0].project_revision(), 7);

    let restored = lifecycle
        .execute(DesktopProjectCommand::RestoreRecovery {
            catalog_revision: 9,
            candidate_id: "candidate-7".into(),
        })
        .unwrap();
    assert_eq!(restored.active().unwrap().path(), "beta.superi");
    assert_eq!(restored.active().unwrap().project_revision(), 7);
    assert!(restored.recovery().is_none());

    let stable_active = restored.active().unwrap().clone();
    let stable_recent = restored.recent().to_vec();
    for (path, class) in [
        ("retryable.superi", DesktopProjectFailureClass::Retryable),
        ("degraded.superi", DesktopProjectFailureClass::Degraded),
        (
            "correctable.superi",
            DesktopProjectFailureClass::UserCorrectable,
        ),
        ("terminal.superi", DesktopProjectFailureClass::Terminal),
    ] {
        let failure = lifecycle
            .execute(DesktopProjectCommand::Open { path: path.into() })
            .unwrap_err();
        assert_eq!(failure.class(), class);
        assert!(!failure.code().is_empty());
        assert!(!failure.title().is_empty());
        assert!(!failure.action().is_empty());
        assert_eq!(failure.context("operation"), Some("open"));
        assert_eq!(lifecycle.snapshot().active(), Some(&stable_active));
        assert_eq!(lifecycle.snapshot().recent(), stable_recent);
        assert_eq!(lifecycle.snapshot().failure(), Some(&failure));
    }

    let closed = lifecycle.execute(DesktopProjectCommand::Close).unwrap();
    assert!(closed.active().is_none());
    assert!(closed.recovery().is_none());
    assert!(closed.failure().is_none());
    assert_eq!(closed.recent(), stable_recent);

    assert_eq!(
        log.snapshot(),
        vec![
            "create:alpha.superi",
            "save:alpha.superi",
            "save_as:alpha.superi:beta.superi:false",
            "open:gamma.superi",
            "open:beta.superi",
            "discover_recovery:beta.superi",
            "restore_recovery:beta.superi:9:candidate-7",
            "open:retryable.superi",
            "open:degraded.superi",
            "open:correctable.superi",
            "open:terminal.superi",
        ]
    );
}

#[test]
fn desktop_project_state_restores_active_identity_and_recents_across_launches() {
    let temporary = TemporaryProjectSession::new();
    let recovery_root = temporary.recovery_root();
    let project_path = temporary.project("session.superi");

    let first = DesktopProjectState::default();
    first.initialize(recovery_root.clone()).unwrap();
    let created = first
        .execute(DesktopProjectCommand::Create {
            path: project_path.to_string_lossy().into_owned(),
            project: DesktopProjectCreateRequest {
                project_id: "project:00000000000000000000000000000602".into(),
                project_name: "Session Project".into(),
                root_timeline_id: "timeline:00000000000000000000000000010602".into(),
                root_timeline_name: "Session Timeline".into(),
                edit_rate_numerator: 24,
                edit_rate_denominator: 1,
            },
        })
        .unwrap();
    assert_eq!(
        created.active().unwrap().project_id(),
        "project:00000000000000000000000000000602"
    );
    drop(first);

    let restored = DesktopProjectState::default();
    restored.initialize(recovery_root).unwrap();
    let snapshot = restored.snapshot().unwrap();
    assert_eq!(
        snapshot.active().unwrap().path(),
        project_path.to_string_lossy()
    );
    assert_eq!(
        snapshot.active().unwrap().project_id(),
        "project:00000000000000000000000000000602"
    );
    assert_eq!(snapshot.recent().len(), 1);
    assert_eq!(snapshot.recent()[0].path(), project_path.to_string_lossy());
    assert!(snapshot.failure().is_none());
}

#[test]
fn missing_session_document_degrades_without_discarding_recent_identity() {
    let temporary = TemporaryProjectSession::new();
    let recovery_root = temporary.recovery_root();
    let project_path = temporary.project("missing.superi");

    let first = DesktopProjectState::default();
    first.initialize(recovery_root.clone()).unwrap();
    first
        .execute(DesktopProjectCommand::Create {
            path: project_path.to_string_lossy().into_owned(),
            project: DesktopProjectCreateRequest {
                project_id: "project:00000000000000000000000000000603".into(),
                project_name: "Missing Session Project".into(),
                root_timeline_id: "timeline:00000000000000000000000000010603".into(),
                root_timeline_name: "Missing Session Timeline".into(),
                edit_rate_numerator: 24,
                edit_rate_denominator: 1,
            },
        })
        .unwrap();
    drop(first);
    std::fs::remove_file(&project_path).unwrap();

    let restored = DesktopProjectState::default();
    restored.initialize(recovery_root).unwrap();
    let snapshot = restored.snapshot().unwrap();
    assert!(snapshot.active().is_none());
    assert_eq!(snapshot.recent().len(), 1);
    assert_eq!(snapshot.recent()[0].path(), project_path.to_string_lossy());
    assert!(snapshot.failure().is_some());
}

#[test]
fn session_record_failure_does_not_report_a_durable_project_create_as_failed() {
    let temporary = TemporaryProjectSession::new();
    let recovery_root = temporary.recovery_root();
    let project_path = temporary.project("available.superi");

    let state = DesktopProjectState::default();
    state.initialize(recovery_root.clone()).unwrap();
    std::fs::create_dir(recovery_root.join("project-session.json")).unwrap();
    let snapshot = state
        .execute(DesktopProjectCommand::Create {
            path: project_path.to_string_lossy().into_owned(),
            project: DesktopProjectCreateRequest {
                project_id: "project:00000000000000000000000000000604".into(),
                project_name: "Available Project".into(),
                root_timeline_id: "timeline:00000000000000000000000000010604".into(),
                root_timeline_name: "Available Timeline".into(),
                edit_rate_numerator: 24,
                edit_rate_denominator: 1,
            },
        })
        .unwrap();
    assert_eq!(
        snapshot.active().unwrap().path(),
        project_path.to_string_lossy()
    );
    let failure = snapshot.failure().unwrap();
    assert_eq!(failure.code(), "error.unavailable.retryable");
    assert_eq!(failure.context("operation"), Some("publish"));
    assert!(project_path.is_file());
}
