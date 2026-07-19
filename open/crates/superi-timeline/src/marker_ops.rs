//! Atomic marker creation, visible-field editing, and removal.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result, ResultExt};
use superi_core::ids::{MarkerId, TimelineId};
use superi_core::time::TimeRange;

use crate::markers::{Marker, MarkerFlag, MarkerLabel, MarkerNote};
use crate::model::{EditorialProject, ProjectDraft};

/// One canonical authored marker mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MarkerMutation {
    /// Creates one marker with complete owner, range, and visible semantics.
    Create {
        timeline_id: TimelineId,
        marker: Marker,
    },
    /// Replaces one marker's exact owner-relative range.
    SetRange {
        timeline_id: TimelineId,
        marker_id: MarkerId,
        marked_range: TimeRange,
    },
    /// Replaces or clears one marker's visible label.
    SetLabel {
        timeline_id: TimelineId,
        marker_id: MarkerId,
        label: Option<MarkerLabel>,
    },
    /// Replaces or clears one marker's visible flag.
    SetFlag {
        timeline_id: TimelineId,
        marker_id: MarkerId,
        flag: Option<MarkerFlag>,
    },
    /// Replaces or clears one marker's authored note.
    SetNote {
        timeline_id: TimelineId,
        marker_id: MarkerId,
        note: Option<MarkerNote>,
    },
    /// Removes one marker and its attached metadata.
    Remove {
        timeline_id: TimelineId,
        marker_id: MarkerId,
    },
}

impl MarkerMutation {
    /// Returns the containing timeline identity.
    #[must_use]
    pub const fn timeline_id(&self) -> TimelineId {
        match self {
            Self::Create { timeline_id, .. }
            | Self::SetRange { timeline_id, .. }
            | Self::SetLabel { timeline_id, .. }
            | Self::SetFlag { timeline_id, .. }
            | Self::SetNote { timeline_id, .. }
            | Self::Remove { timeline_id, .. } => *timeline_id,
        }
    }

    /// Returns the stable marker identity addressed by this mutation.
    #[must_use]
    pub const fn marker_id(&self) -> MarkerId {
        match self {
            Self::Create { marker, .. } => marker.id(),
            Self::SetRange { marker_id, .. }
            | Self::SetLabel { marker_id, .. }
            | Self::SetFlag { marker_id, .. }
            | Self::SetNote { marker_id, .. }
            | Self::Remove { marker_id, .. } => *marker_id,
        }
    }

    const fn kind(&self) -> MarkerMutationKind {
        match self {
            Self::Create { .. } => MarkerMutationKind::Create,
            Self::SetRange { .. } => MarkerMutationKind::SetRange,
            Self::SetLabel { .. } => MarkerMutationKind::SetLabel,
            Self::SetFlag { .. } => MarkerMutationKind::SetFlag,
            Self::SetNote { .. } => MarkerMutationKind::SetNote,
            Self::Remove { .. } => MarkerMutationKind::Remove,
        }
    }
}

/// Stable category for one applied marker mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MarkerMutationKind {
    Create,
    SetRange,
    SetLabel,
    SetFlag,
    SetNote,
    Remove,
}

/// Semantic result for one mutation in an atomic batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MarkerMutationOutcome {
    timeline_id: TimelineId,
    marker_id: MarkerId,
    kind: MarkerMutationKind,
}

impl MarkerMutationOutcome {
    #[must_use]
    pub const fn timeline_id(self) -> TimelineId {
        self.timeline_id
    }

    #[must_use]
    pub const fn marker_id(self) -> MarkerId {
        self.marker_id
    }

    #[must_use]
    pub const fn kind(self) -> MarkerMutationKind {
        self.kind
    }
}

/// Results published together at one editorial project revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MarkerMutationBatchResult {
    revision: u64,
    outcomes: Vec<MarkerMutationOutcome>,
}

impl MarkerMutationBatchResult {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn outcomes(&self) -> &[MarkerMutationOutcome] {
        &self.outcomes
    }
}

/// Applies a nonempty ordered marker mutation batch atomically.
pub fn apply_marker_mutation_batch(
    project: &mut EditorialProject,
    expected_revision: u64,
    mutations: &[MarkerMutation],
) -> Result<MarkerMutationBatchResult> {
    if mutations.is_empty() {
        return Err(marker_error(
            ErrorCategory::InvalidInput,
            "apply_marker_mutation_batch",
            "a marker mutation batch must contain at least one mutation",
        ));
    }
    let mut outcomes = Vec::with_capacity(mutations.len());
    project.edit(expected_revision, |draft| {
        for (index, mutation) in mutations.iter().enumerate() {
            apply_mutation(draft, mutation).with_error_context(
                ErrorContext::new("superi-timeline.marker-ops", "apply_marker_mutation")
                    .with_field("index", index.to_string())
                    .with_field("timeline", mutation.timeline_id().to_string())
                    .with_field("marker", mutation.marker_id().to_string()),
            )?;
            outcomes.push(MarkerMutationOutcome {
                timeline_id: mutation.timeline_id(),
                marker_id: mutation.marker_id(),
                kind: mutation.kind(),
            });
        }
        Ok(())
    })?;
    Ok(MarkerMutationBatchResult {
        revision: project.revision(),
        outcomes,
    })
}

fn apply_mutation(draft: &mut ProjectDraft, mutation: &MarkerMutation) -> Result<()> {
    let timeline = draft.timeline_mut(mutation.timeline_id())?;
    match mutation {
        MarkerMutation::Create { marker, .. } => {
            if timeline.marker(marker.id()).is_some() {
                return Err(marker_error(
                    ErrorCategory::Conflict,
                    "create_marker",
                    "editorial marker identity already exists on the timeline",
                ));
            }
            timeline.upsert_marker(marker.clone()).map(drop)
        }
        MarkerMutation::SetRange {
            marker_id,
            marked_range,
            ..
        } => timeline
            .marker_mut(*marker_id)?
            .set_marked_range(*marked_range),
        MarkerMutation::SetLabel {
            marker_id, label, ..
        } => {
            timeline.marker_mut(*marker_id)?.set_label(label.clone());
            Ok(())
        }
        MarkerMutation::SetFlag {
            marker_id, flag, ..
        } => {
            timeline.marker_mut(*marker_id)?.set_flag(*flag);
            Ok(())
        }
        MarkerMutation::SetNote {
            marker_id, note, ..
        } => {
            timeline.marker_mut(*marker_id)?.set_note(note.clone());
            Ok(())
        }
        MarkerMutation::Remove { marker_id, .. } => {
            timeline.remove_marker(*marker_id).map(drop).ok_or_else(|| {
                marker_error(
                    ErrorCategory::NotFound,
                    "remove_marker",
                    "editorial marker was not found",
                )
            })
        }
    }
}

fn marker_error(category: ErrorCategory, operation: &'static str, message: &'static str) -> Error {
    Error::new(category, Recoverability::UserCorrectable, message)
        .with_context(ErrorContext::new("superi-timeline.marker-ops", operation))
}
