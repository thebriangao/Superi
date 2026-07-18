//! Engine-tier reconciliation between timeline edit outcomes and audio-owned clip mix intent.

use std::collections::BTreeSet;

use superi_audio::mixing::{ClipMixMutation, ClipMixState};
use superi_core::error::Result;
use superi_core::ids::ClipId;
use superi_timeline::edit_ops::{apply_edit_batch, EditBatchResult, EditKind, EditOperation};
use superi_timeline::model::{EditorialObjectId, EditorialProject};

/// Applies timeline edits and their clip-mix identity changes as one publication.
///
/// Both subsystem states are cloned, validated, and reconciled before either caller-owned value is
/// replaced. A timeline validation error, a mix revision conflict, or an identity reconciliation
/// error therefore leaves both published revisions unchanged.
pub fn apply_edit_batch_with_clip_mix(
    project: &mut EditorialProject,
    expected_project_revision: u64,
    mix_state: &mut ClipMixState,
    expected_mix_revision: u64,
    operations: &[EditOperation],
) -> Result<EditBatchResult> {
    let mut next_project = project.clone();
    let edit = apply_edit_batch(&mut next_project, expected_project_revision, operations)?;
    let mut next_mix = mix_state.clone();
    reconcile_clip_mix_edit_batch(&mut next_mix, expected_mix_revision, &edit)?;
    *project = next_project;
    *mix_state = next_mix;
    Ok(edit)
}

/// Atomically reconciles clip identities published by one timeline edit batch.
///
/// Retained identities need no mix mutation. Right fragments inherit complete user intent,
/// replacements transfer it, and wholly removed clips release it. Clips without explicit mix
/// state remain absent instead of acquiring synthesized defaults.
pub fn reconcile_clip_mix_edit_batch(
    state: &mut ClipMixState,
    expected_revision: u64,
    edit: &EditBatchResult,
) -> Result<u64> {
    state.require_revision(expected_revision)?;
    let mut mutations = Vec::new();
    let mut added = BTreeSet::new();
    let mut removed = BTreeSet::new();

    for outcome in edit.outcomes() {
        for fragment in outcome.fragments() {
            let (EditorialObjectId::Clip(original), EditorialObjectId::Clip(created)) =
                (fragment.original(), fragment.created())
            else {
                continue;
            };
            if owns_intent(state, original, &added, &removed) {
                mutations.push(ClipMixMutation::inherit(original, created));
                added.insert(created);
            }
        }

        if outcome.kind() == EditKind::Replace {
            if let ([EditorialObjectId::Clip(old)], [EditorialObjectId::Clip(new)]) =
                (outcome.removed_ids(), outcome.inserted_ids())
            {
                if owns_intent(state, *old, &added, &removed) {
                    mutations.push(ClipMixMutation::transfer(*old, *new));
                    removed.insert(*old);
                    added.insert(*new);
                }
                continue;
            }
        }

        for id in outcome.removed_ids() {
            let EditorialObjectId::Clip(clip_id) = id else {
                continue;
            };
            if owns_intent(state, *clip_id, &added, &removed) {
                mutations.push(ClipMixMutation::remove(*clip_id));
                removed.insert(*clip_id);
                added.remove(clip_id);
            }
        }
    }

    if mutations.is_empty() {
        return Ok(state.revision());
    }
    state.apply(expected_revision, &mutations)
}

/// Releases authored audio intent for clips removed by a track mutation batch.
///
/// Track deletion owns every contained editorial object. This reconciliation compares the
/// published editorial identities before and after the batch, then removes only mix controls whose
/// clips disappeared from the complete project. The caller's outer project transaction supplies
/// atomic rollback when either the track batch or this audio mutation fails.
pub fn reconcile_removed_track_clip_mix(
    previous: &EditorialProject,
    next: &EditorialProject,
    state: &mut ClipMixState,
) -> Result<u64> {
    let previous_clip_ids = clip_ids(previous);
    let next_clip_ids = clip_ids(next);
    let mutations = previous_clip_ids
        .difference(&next_clip_ids)
        .filter(|clip_id| state.controls(**clip_id).is_some())
        .copied()
        .map(ClipMixMutation::remove)
        .collect::<Vec<_>>();
    if mutations.is_empty() {
        return Ok(state.revision());
    }
    state.apply(state.revision(), &mutations)
}

fn clip_ids(project: &EditorialProject) -> BTreeSet<ClipId> {
    project
        .timelines()
        .flat_map(|timeline| timeline.tracks())
        .flat_map(|track| track.items())
        .filter_map(|item| item.as_clip())
        .map(|clip| clip.id())
        .collect()
}

fn owns_intent(
    state: &ClipMixState,
    clip_id: ClipId,
    added: &BTreeSet<ClipId>,
    removed: &BTreeSet<ClipId>,
) -> bool {
    !removed.contains(&clip_id) && (added.contains(&clip_id) || state.controls(clip_id).is_some())
}
