//! Feature-gated fixtures for downstream contract tests.

use superi_core::error::Result;
use superi_core::ids::{ProjectId, TimelineId};
use superi_core::time::{RationalTime, Timebase};
use superi_project::document::ProjectDocument;
use superi_timeline::model::{EditorialProject, LinkedMediaReference, Timeline};

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
