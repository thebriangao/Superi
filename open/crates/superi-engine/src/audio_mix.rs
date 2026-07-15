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

fn owns_intent(
    state: &ClipMixState,
    clip_id: ClipId,
    added: &BTreeSet<ClipId>,
    removed: &BTreeSet<ClipId>,
) -> bool {
    !removed.contains(&clip_id) && (added.contains(&clip_id) || state.controls(clip_id).is_some())
}
