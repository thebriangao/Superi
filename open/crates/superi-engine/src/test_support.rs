//! Feature-gated fixtures for downstream contract tests.

use std::path::{Path, PathBuf};
use std::time::Duration;

use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use superi_core::ids::{ProjectId, TimelineId};
use superi_core::settings::SettingValue;
use superi_core::time::{RationalTime, Timebase};
use superi_project::autosave::{
    ProjectAutosaveCommand, ProjectAutosaveController, ProjectAutosavePolicy,
};
use superi_project::document::ProjectDocument;
use superi_project::settings::{
    ProjectSettingMutation, ProjectSettingsTransaction, AUDIO_SAMPLE_RATE_KEY,
};
use superi_project::ProjectDatabase;
use superi_timeline::model::{EditorialProject, LinkedMediaReference, Timeline};

use crate::dispatcher::EngineCommandDispatcher;

/// Builds one empty real project aggregate for downstream engine-boundary tests.
pub fn empty_project_document(
    project_id: ProjectId,
    root_timeline_id: TimelineId,
    edit_rate: Timebase,
) -> Result<ProjectDocument> {
    let timeline = Timeline::new(
        root_timeline_id,
        "engine boundary test timeline",
        edit_rate,
        RationalTime::zero(edit_rate),
        vec![],
    );
    let editorial = EditorialProject::new(
        project_id,
        "engine boundary test project",
        std::iter::empty::<LinkedMediaReference>(),
        [timeline],
    )?;
    ProjectDocument::new(editorial, root_timeline_id)
}

/// Builds one full dispatcher with a changed active project and one real recovery candidate.
pub fn project_recovery_dispatcher_fixture(
    project_id: ProjectId,
    root_timeline_id: TimelineId,
    edit_rate: Timebase,
    recovery_root: impl AsRef<Path>,
    active_path: impl AsRef<Path>,
) -> Result<(EngineCommandDispatcher, PathBuf)> {
    let recovery_root = recovery_root.as_ref();
    let candidate_snapshot =
        empty_project_document(project_id, root_timeline_id, edit_rate)?.snapshot();
    let mut autosave = ProjectAutosaveController::new(project_id)?;
    autosave.execute(ProjectAutosaveCommand::Configure {
        policy: ProjectAutosavePolicy::new(false, Duration::from_secs(60), recovery_root, 8)?,
        elapsed: Duration::ZERO,
    })?;
    let published = autosave.execute(ProjectAutosaveCommand::SaveNow {
        elapsed: Duration::from_secs(1),
        snapshot: candidate_snapshot,
    })?;
    let candidate_path = published
        .published()
        .ok_or_else(|| {
            Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "recovery test fixture did not publish its forced candidate",
            )
        })?
        .path()
        .to_path_buf();

    let mut current = empty_project_document(project_id, root_timeline_id, edit_rate)?;
    current.execute_settings_transaction(ProjectSettingsTransaction::new(
        current.revision(),
        vec![ProjectSettingMutation::set(
            AUDIO_SAMPLE_RATE_KEY,
            SettingValue::Integer(96_000),
        )?],
    )?)?;
    let snapshot = current.snapshot();
    current = ProjectDocument::from_complete_parts_with_settings(
        snapshot.revision(),
        snapshot.editorial_project().clone(),
        snapshot.root_timeline_id(),
        snapshot.settings().clone(),
        snapshot.graphs().cloned(),
        superi_audio::mixing::ClipMixState::from_parts(
            1,
            std::iter::empty::<(
                superi_core::ids::ClipId,
                superi_audio::mixing::ClipMixControls,
            )>(),
        )?,
    )?;
    let mut database = ProjectDatabase::create(active_path)?;
    database.replace(&current.snapshot())?;

    let mut dispatcher = EngineCommandDispatcher::new()?;
    dispatcher.attach_project(current)?;
    dispatcher.attach_project_recovery(database, recovery_root)?;
    Ok((dispatcher, candidate_path))
}
