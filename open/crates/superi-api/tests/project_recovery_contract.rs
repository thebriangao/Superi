use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use superi_api::commands::{
    ApiCommand, CompareProjectRecovery, DismissProjectRecovery, GetProjectRecovery,
    RestoreProjectRecovery,
};
use superi_api::events::{ApiEvent, ProjectRecoveryChanged};
use superi_api::recovery::ProjectRecoveryApi;
use superi_api::version::{
    COMPARE_PROJECT_RECOVERY_METHOD, DISMISS_PROJECT_RECOVERY_METHOD, GET_PROJECT_RECOVERY_METHOD,
    PROJECT_RECOVERY_CHANGED_EVENT, RESTORE_PROJECT_RECOVERY_METHOD,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::ErrorCategory;
use superi_core::ids::{ProjectId, TimelineId};
use superi_core::time::Timebase;
use superi_engine::test_support::project_recovery_dispatcher_fixture;

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

const PROJECT: ProjectId = ProjectId::from_raw(0xe100);
const ROOT: TimelineId = TimelineId::from_raw(0xe101);

struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "superi-api-recovery-{}-{}",
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

#[test]
fn one_strict_public_surface_discovers_compares_restores_and_dismisses_safely() {
    let _domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let directory = TempDirectory::new();
    let active_path = directory.child("active.superi");
    let (dispatcher, candidate) = project_recovery_dispatcher_fixture(
        PROJECT,
        ROOT,
        Timebase::integer(24).unwrap(),
        directory.path(),
        &active_path,
    )
    .unwrap();
    fs::write(
        candidate
            .parent()
            .unwrap()
            .join("autosave-g00000000000000000002.superi"),
        b"not a sqlite database and contains private source detail",
    )
    .unwrap();
    let mut api = ProjectRecoveryApi::new(dispatcher).unwrap();

    let discovered = api
        .execute(GetProjectRecovery::new("public-recovery-discover"))
        .unwrap();
    assert_eq!(discovered.transaction_id(), "public-recovery-discover");
    assert_eq!(discovered.snapshot().schema_version().to_string(), "1.0.0");
    assert_eq!(discovered.snapshot().project_id(), PROJECT.to_string());
    assert_eq!(discovered.snapshot().project_revision(), 1);
    assert_eq!(discovered.snapshot().candidates().len(), 1);
    assert_eq!(discovered.snapshot().findings().len(), 1);
    assert_eq!(
        discovered.snapshot().findings()[0].category(),
        "corrupt_data"
    );
    assert_eq!(
        discovered.snapshot().findings()[0].recoverability(),
        "degraded"
    );
    assert_eq!(
        discovered.snapshot().findings()[0].next_action(),
        "continue_with_other_candidate"
    );
    let public_json = serde_json::to_string(&discovered).unwrap();
    assert!(!public_json.contains(&directory.path().display().to_string()));
    assert!(!public_json.to_lowercase().contains("sqlite"));
    assert!(!public_json.contains("private source detail"));

    let invalid = api
        .compare(CompareProjectRecovery::new(
            "public-recovery-invalid-candidate",
            discovered.snapshot().catalog_revision(),
            candidate.display().to_string(),
        ))
        .unwrap_err();
    assert_eq!(invalid.category(), ErrorCategory::InvalidInput);
    assert!(candidate.exists());

    let candidate_id = discovered.snapshot().candidates()[0].candidate_id();
    let catalog_revision = discovered.snapshot().catalog_revision();
    let comparison = api
        .compare(CompareProjectRecovery::new(
            "public-recovery-compare",
            catalog_revision,
            candidate_id,
        ))
        .unwrap();
    assert_eq!(comparison.transaction_id(), "public-recovery-compare");
    assert!(comparison.comparison().settings_changed());
    assert!(comparison.comparison().clip_mix_changed());
    assert!(comparison.comparison().has_changes());

    let restored = api
        .restore(RestoreProjectRecovery::new(
            "public-recovery-restore",
            catalog_revision,
            1,
            candidate_id,
        ))
        .unwrap();
    assert!(restored.restored());
    assert_eq!(restored.snapshot().project_revision(), 2);
    assert!(candidate.exists());
    let events = api.drain_events().unwrap();
    assert_eq!(
        events.len(),
        2,
        "discover and restore each emit replacement state"
    );
    assert_eq!(events[1].transaction_id(), "public-recovery-restore");
    assert_eq!(events[1].snapshot(), restored.snapshot());
    let event_json = serde_json::to_value(&events[1]).unwrap();
    let round_trip: ProjectRecoveryChanged = serde_json::from_value(event_json).unwrap();
    assert_eq!(round_trip, events[1]);

    let dismissed = api
        .dismiss(DismissProjectRecovery::new(
            "public-recovery-dismiss",
            catalog_revision,
            candidate_id,
        ))
        .unwrap();
    assert!(dismissed.dismissed());
    assert!(dismissed.snapshot().candidates().is_empty());
    assert!(!candidate.exists());
}

#[test]
fn recovery_commands_and_event_names_are_permanent_strict_contracts() {
    assert_eq!(GetProjectRecovery::METHOD, GET_PROJECT_RECOVERY_METHOD);
    assert_eq!(
        CompareProjectRecovery::METHOD,
        COMPARE_PROJECT_RECOVERY_METHOD
    );
    assert_eq!(
        RestoreProjectRecovery::METHOD,
        RESTORE_PROJECT_RECOVERY_METHOD
    );
    assert_eq!(
        DismissProjectRecovery::METHOD,
        DISMISS_PROJECT_RECOVERY_METHOD
    );
    assert_eq!(ProjectRecoveryChanged::NAME, PROJECT_RECOVERY_CHANGED_EVENT);
    assert_eq!(GET_PROJECT_RECOVERY_METHOD, "superi.project.recovery.get");
    assert_eq!(
        COMPARE_PROJECT_RECOVERY_METHOD,
        "superi.project.recovery.compare"
    );
    assert_eq!(
        RESTORE_PROJECT_RECOVERY_METHOD,
        "superi.project.recovery.restore"
    );
    assert_eq!(
        DISMISS_PROJECT_RECOVERY_METHOD,
        "superi.project.recovery.dismiss"
    );
    assert_eq!(
        PROJECT_RECOVERY_CHANGED_EVENT,
        "superi.project.recovery.changed"
    );

    let command = RestoreProjectRecovery::new("strict", 1, 2, "recovery-g00000000000000000001");
    let mut wire = serde_json::to_value(command).unwrap();
    wire["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<RestoreProjectRecovery>(wire).is_err());
}
