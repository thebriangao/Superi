use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use superi_core::ids::{MediaId, ProjectId, TimelineId};
use superi_core::settings::SettingValue;
use superi_core::time::{RationalTime, Timebase};
use superi_engine::history::{ProjectCommandHistory, ProjectHistoryCommand, ProjectMutation};
use superi_project::autosave::{
    ProjectAutosaveCommand, ProjectAutosaveController, ProjectAutosaveDisposition,
    ProjectAutosavePolicy,
};
use superi_project::document::{ProjectDocument, ProjectSnapshot};
use superi_project::media::{PortableRelativePath, ProjectMediaCommand, ReferencedMediaPath};
use superi_project::settings::{
    ProjectSettingMutation, ProjectSettingsTransaction, AUDIO_OUTPUT_LAYOUT_KEY,
    AUDIO_SAMPLE_RATE_KEY,
};
use superi_project::ProjectDatabase;
use superi_timeline::model::{EditorialProject, LinkedMediaReference, Timeline};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

const PROJECT: ProjectId = ProjectId::from_raw(0xb000);
const ROOT: TimelineId = TimelineId::from_raw(0xb001);
const MEDIA: MediaId = MediaId::from_raw(0xb002);

struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "superi-engine-autosave-{}-{}",
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

fn document() -> ProjectDocument {
    let rate = Timebase::integer(24).unwrap();
    let timeline = Timeline::new(
        ROOT,
        "engine autosave timeline",
        rate,
        RationalTime::zero(rate),
        vec![],
    );
    let original = ReferencedMediaPath::project_relative(
        PortableRelativePath::new("Media/original.webm").unwrap(),
    );
    let media = LinkedMediaReference::with_fingerprint(
        MEDIA,
        "engine autosave media",
        original.to_target(),
        None,
        "sha256:engine-autosave",
    )
    .unwrap();
    let editorial =
        EditorialProject::new(PROJECT, "engine autosave project", [media], [timeline]).unwrap();
    let mut document = ProjectDocument::new(editorial, ROOT).unwrap();
    document
        .execute_settings_transaction(
            ProjectSettingsTransaction::new(
                0,
                vec![
                    ProjectSettingMutation::set(
                        AUDIO_SAMPLE_RATE_KEY,
                        SettingValue::Integer(96_000),
                    )
                    .unwrap(),
                    ProjectSettingMutation::set(
                        AUDIO_OUTPUT_LAYOUT_KEY,
                        SettingValue::Text("surround_5_1".into()),
                    )
                    .unwrap(),
                ],
            )
            .unwrap(),
        )
        .unwrap();
    document
}

fn assert_artifact(path: &Path, expected: &ProjectSnapshot) {
    let database = ProjectDatabase::open_read_only(path).unwrap();
    assert_eq!(database.load().unwrap().snapshot(), *expected);
}

#[test]
fn selected_history_state_reaches_current_schema_autosaves_after_apply_undo_and_redo() {
    let directory = TempDirectory::new();
    let mut history = ProjectCommandHistory::new(document());
    let mut controller = ProjectAutosaveController::new(PROJECT).unwrap();
    controller
        .execute(ProjectAutosaveCommand::Configure {
            policy: ProjectAutosavePolicy::new(false, Duration::from_secs(60), directory.path(), 3)
                .unwrap(),
            elapsed: Duration::ZERO,
        })
        .unwrap();

    let original = history.state().snapshot().clone();
    let changed_path = ReferencedMediaPath::project_relative(
        PortableRelativePath::new("Media/relinked.webm").unwrap(),
    );
    let applied = history
        .execute(ProjectHistoryCommand::apply(
            original.revision(),
            ProjectMutation::media(ProjectMediaCommand::set_path(MEDIA, changed_path.clone())),
        ))
        .unwrap();
    let applied_snapshot = applied.state().snapshot().clone();
    let applied_autosave = controller
        .execute(ProjectAutosaveCommand::SaveNow {
            elapsed: Duration::from_secs(1),
            snapshot: applied_snapshot.clone(),
        })
        .unwrap();
    assert_eq!(
        applied_autosave.disposition(),
        ProjectAutosaveDisposition::Published
    );
    assert_eq!(applied_snapshot.media_path(MEDIA).unwrap(), changed_path);
    assert_eq!(
        applied_snapshot.settings().integer(AUDIO_SAMPLE_RATE_KEY),
        Some(96_000)
    );
    assert_artifact(
        applied_autosave.published().unwrap().path(),
        &applied_snapshot,
    );

    let undone = history
        .execute(ProjectHistoryCommand::undo(applied_snapshot.revision()))
        .unwrap();
    let undone_snapshot = undone.state().snapshot().clone();
    let undone_autosave = controller
        .execute(ProjectAutosaveCommand::SaveNow {
            elapsed: Duration::from_secs(2),
            snapshot: undone_snapshot.clone(),
        })
        .unwrap();
    assert_eq!(undone_snapshot.revision(), applied_snapshot.revision() + 1);
    assert_eq!(
        undone_snapshot.media_path(MEDIA).unwrap(),
        original.media_path(MEDIA).unwrap()
    );
    assert_eq!(
        undone_snapshot.settings().integer(AUDIO_SAMPLE_RATE_KEY),
        Some(96_000)
    );
    assert_artifact(
        undone_autosave.published().unwrap().path(),
        &undone_snapshot,
    );

    let redone = history
        .execute(ProjectHistoryCommand::redo(undone_snapshot.revision()))
        .unwrap();
    let redone_snapshot = redone.state().snapshot().clone();
    let redone_autosave = controller
        .execute(ProjectAutosaveCommand::SaveNow {
            elapsed: Duration::from_secs(3),
            snapshot: redone_snapshot.clone(),
        })
        .unwrap();
    assert_eq!(redone_snapshot.media_path(MEDIA).unwrap(), changed_path);
    assert_eq!(
        redone_snapshot.settings().text(AUDIO_OUTPUT_LAYOUT_KEY),
        Some("surround_5_1")
    );
    assert_artifact(
        redone_autosave.published().unwrap().path(),
        &redone_snapshot,
    );

    assert_eq!(redone_autosave.state().managed_count(), 3);
    assert_eq!(
        [
            applied_autosave.published().unwrap().generation(),
            undone_autosave.published().unwrap().generation(),
            redone_autosave.published().unwrap().generation(),
        ],
        [1, 2, 3]
    );
}
