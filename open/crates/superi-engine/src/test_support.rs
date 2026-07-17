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
use superi_project::document::{ProjectDocument, ProjectSnapshot};
use superi_project::settings::{
    ProjectSettingMutation, ProjectSettingsTransaction, AUDIO_SAMPLE_RATE_KEY,
};
use superi_project::{
    execute_project_integrity_command, ProjectDatabase, ProjectIntegrityCommand,
    ProjectIntegrityStatus, ProjectRecoveryController,
};
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

/// Proves that one selected snapshot survives the real integrity and recovery owners.
pub fn verify_project_snapshot_integrity_and_recovery(
    active_path: impl AsRef<Path>,
    recovery_root: impl AsRef<Path>,
    snapshot: &ProjectSnapshot,
) -> Result<ProjectSnapshot> {
    let integrity = execute_project_integrity_command(ProjectIntegrityCommand::Validate {
        path: active_path.as_ref().to_path_buf(),
    })?;
    let identity = integrity.identity().ok_or_else(|| {
        Error::new(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "project integrity proof did not return verified identity",
        )
    })?;
    if integrity.status() != ProjectIntegrityStatus::Valid
        || !integrity.inspection_complete()
        || !integrity.findings().is_empty()
        || identity.project_id() != snapshot.project_id()
        || identity.document_revision() != snapshot.revision()
        || identity.root_timeline_id() != snapshot.root_timeline_id()
    {
        return Err(Error::new(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "project integrity proof did not preserve the selected snapshot",
        ));
    }

    let recovery_root = recovery_root.as_ref();
    let mut autosave = ProjectAutosaveController::new(snapshot.project_id())?;
    autosave.execute(ProjectAutosaveCommand::Configure {
        policy: ProjectAutosavePolicy::new(false, Duration::from_secs(60), recovery_root, 2)?,
        elapsed: Duration::ZERO,
    })?;
    autosave.execute(ProjectAutosaveCommand::SaveNow {
        elapsed: Duration::from_secs(1),
        snapshot: snapshot.clone(),
    })?;

    let mut recovery = ProjectRecoveryController::new(snapshot.project_id(), recovery_root)?;
    let catalog = recovery.discover()?.clone();
    if !catalog.findings().is_empty() || catalog.candidates().len() != 1 {
        return Err(Error::new(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "project recovery proof did not discover one valid candidate",
        ));
    }
    let candidate_id = catalog.candidates()[0].id();
    let comparison = recovery.compare(catalog.revision(), candidate_id, snapshot)?;
    if comparison.has_changes() {
        return Err(Error::new(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "project recovery proof changed selected snapshot meaning",
        ));
    }
    recovery.load_candidate_for_restore(catalog.revision(), candidate_id, snapshot)
}
