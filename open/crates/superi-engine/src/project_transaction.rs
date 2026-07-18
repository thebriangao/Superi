//! Ordered whole-project transactions across authored subsystem state.

use superi_audio::mixing::ClipMixMutation;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result, ResultExt};
use superi_core::ids::{GraphId, TimelineId};
use superi_graph::mutate::{GraphMutation, GraphTransaction};
use superi_project::document::{ProjectDocument, ProjectSnapshot};
use superi_project::extensions::{ProjectExtensionCommand, ProjectExtensionCommandResult};
use superi_project::media::{
    ProjectMediaCommand, ProjectMediaCommandResult, ProjectMediaImportResult,
};
use superi_timeline::compile::{recompile_timeline_preserving_edits, CompiledTimelineGraphValue};
use superi_timeline::edit_ops::{EditBatchResult, EditOperation};
use superi_timeline::model::LinkedMediaReference;
use superi_timeline::track_ops::{
    apply_track_mutation_batch, TrackMutation, TrackMutationBatchResult,
};

use crate::audio_mix::{apply_edit_batch_with_clip_mix, reconcile_removed_track_clip_mix};

const COMPONENT: &str = "superi-engine.project-transaction";

/// Maximum ordered subsystem actions in one compound publication.
pub const MAX_COMPOUND_PROJECT_ACTIONS: usize = 64;

/// One authored action inside a compound whole-project transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CompoundProjectAction {
    /// Select an already compiled editorial timeline as the project root.
    SelectRootTimeline(TimelineId),
    /// Apply a nonempty atomic editorial operation batch.
    EditTimeline(Vec<EditOperation>),
    /// Apply a nonempty atomic track mutation batch.
    MutateTracks(Vec<TrackMutation>),
    /// Apply a nonempty ordered mutation batch to one retained graph.
    MutateGraph {
        /// Stable retained graph identity.
        graph_id: GraphId,
        /// Ordered graph mutations.
        mutations: Vec<GraphMutation<CompiledTimelineGraphValue>>,
    },
    /// Apply one referenced-media path or relink command.
    MutateMedia(ProjectMediaCommand),
    /// Insert one bounded referenced-media batch.
    ImportMedia(Vec<LinkedMediaReference>),
    /// Apply a nonempty authored clip-mix mutation batch.
    MutateClipMix(Vec<ClipMixMutation>),
    /// Apply one durable extension-state command.
    MutateExtension(ProjectExtensionCommand),
}

impl CompoundProjectAction {
    /// Creates a timeline edit action.
    #[must_use]
    pub fn edit_timeline(operations: impl IntoIterator<Item = EditOperation>) -> Self {
        Self::EditTimeline(operations.into_iter().collect())
    }

    /// Creates an atomic track mutation action.
    #[must_use]
    pub fn mutate_tracks(mutations: impl IntoIterator<Item = TrackMutation>) -> Self {
        Self::MutateTracks(mutations.into_iter().collect())
    }

    /// Creates a graph mutation action.
    #[must_use]
    pub fn mutate_graph(
        graph_id: GraphId,
        mutations: impl IntoIterator<Item = GraphMutation<CompiledTimelineGraphValue>>,
    ) -> Self {
        Self::MutateGraph {
            graph_id,
            mutations: mutations.into_iter().collect(),
        }
    }

    /// Creates a clip-mix mutation action.
    #[must_use]
    pub fn mutate_clip_mix(mutations: impl IntoIterator<Item = ClipMixMutation>) -> Self {
        Self::MutateClipMix(mutations.into_iter().collect())
    }

    /// Creates one atomic referenced-media import action.
    #[must_use]
    pub fn import_media(media: impl IntoIterator<Item = LinkedMediaReference>) -> Self {
        Self::ImportMedia(media.into_iter().collect())
    }

    /// Creates an extension-state mutation action.
    #[must_use]
    pub const fn mutate_extension(command: ProjectExtensionCommand) -> Self {
        Self::MutateExtension(command)
    }

    const fn code(&self) -> &'static str {
        match self {
            Self::SelectRootTimeline(_) => "select_root_timeline",
            Self::EditTimeline(_) => "edit_timeline",
            Self::MutateTracks(_) => "mutate_tracks",
            Self::MutateGraph { .. } => "mutate_graph",
            Self::MutateMedia(_) => "mutate_media",
            Self::ImportMedia(_) => "import_media",
            Self::MutateClipMix(_) => "mutate_clip_mix",
            Self::MutateExtension(_) => "mutate_extension",
        }
    }
}

/// One checked ordered compound transaction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompoundProjectTransaction {
    actions: Vec<CompoundProjectAction>,
}

impl CompoundProjectTransaction {
    /// Creates a bounded nonempty action sequence.
    pub fn new(actions: impl IntoIterator<Item = CompoundProjectAction>) -> Result<Self> {
        let actions: Vec<_> = actions.into_iter().collect();
        if actions.is_empty() {
            return Err(transaction_error(
                ErrorCategory::InvalidInput,
                "create_compound_transaction",
                "compound project transaction must contain at least one action",
            ));
        }
        if actions.len() > MAX_COMPOUND_PROJECT_ACTIONS {
            return Err(transaction_error(
                ErrorCategory::ResourceExhausted,
                "create_compound_transaction",
                "compound project transaction exceeds the supported action bound",
            ));
        }
        for action in &actions {
            let empty_batch = matches!(
                action,
                CompoundProjectAction::EditTimeline(operations) if operations.is_empty()
            ) || matches!(
                action,
                CompoundProjectAction::MutateTracks(mutations) if mutations.is_empty()
            ) || matches!(
                action,
                CompoundProjectAction::MutateGraph { mutations, .. } if mutations.is_empty()
            ) || matches!(
                action,
                CompoundProjectAction::MutateClipMix(mutations) if mutations.is_empty()
            ) || matches!(
                action,
                CompoundProjectAction::ImportMedia(media) if media.is_empty()
            );
            if empty_batch {
                return Err(transaction_error(
                    ErrorCategory::InvalidInput,
                    "create_compound_transaction",
                    "compound subsystem mutation batches must not be empty",
                ));
            }
        }
        Ok(Self { actions })
    }

    /// Returns subsystem actions in execution order.
    #[must_use]
    pub fn actions(&self) -> &[CompoundProjectAction] {
        &self.actions
    }
}

/// Semantic result of one ordered compound action.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CompoundProjectActionResult {
    /// The selected root was accepted.
    RootTimelineSelected(TimelineId),
    /// Editorial operations and clip identity reconciliation were published.
    TimelineEdited(EditBatchResult),
    /// Track mutations and graph reconciliation were published.
    TracksMutated(TrackMutationBatchResult),
    /// A retained graph published one new graph revision.
    GraphMutated {
        /// Stable retained graph identity.
        graph_id: GraphId,
        /// Published graph revision.
        revision: u64,
    },
    /// A media command was accepted.
    MediaMutated(ProjectMediaCommandResult),
    /// A referenced-media batch was accepted.
    MediaImported(ProjectMediaImportResult),
    /// Authored clip-mix state published one new mix revision.
    ClipMixMutated(u64),
    /// Durable extension state was accepted.
    ExtensionMutated(ProjectExtensionCommandResult),
}

/// One compound result paired with the exact published project snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompoundProjectTransactionOutcome {
    snapshot: ProjectSnapshot,
    actions: Vec<CompoundProjectActionResult>,
}

impl CompoundProjectTransactionOutcome {
    /// Returns the exact project snapshot after the transaction.
    #[must_use]
    pub const fn snapshot(&self) -> &ProjectSnapshot {
        &self.snapshot
    }

    /// Returns semantic action results in command order.
    #[must_use]
    pub fn actions(&self) -> &[CompoundProjectActionResult] {
        &self.actions
    }
}

/// Executes every subsystem action inside one outer project publication.
pub fn execute_compound_project_transaction(
    document: &mut ProjectDocument,
    expected_revision: u64,
    transaction: &CompoundProjectTransaction,
) -> Result<CompoundProjectTransactionOutcome> {
    let mut action_results = Vec::with_capacity(transaction.actions.len());
    let snapshot = document.edit(expected_revision, |draft| {
        for (index, action) in transaction.actions.iter().enumerate() {
            let result = match action {
                CompoundProjectAction::SelectRootTimeline(timeline_id) => {
                    draft.set_root_timeline_id(*timeline_id);
                    CompoundProjectActionResult::RootTimelineSelected(*timeline_id)
                }
                CompoundProjectAction::EditTimeline(operations) => {
                    let old_project = draft.editorial_project().clone();
                    let retained = draft
                        .graphs()
                        .filter_map(|graph| graph.as_timeline().cloned())
                        .collect::<Vec<_>>();
                    let (editorial, mix_state) = draft.editorial_and_clip_mix_mut();
                    let edit = apply_edit_batch_with_clip_mix(
                        editorial,
                        editorial.revision(),
                        mix_state,
                        mix_state.revision(),
                        operations,
                    )?;
                    for compilation in retained {
                        let reconciled = recompile_timeline_preserving_edits(
                            &old_project,
                            &compilation,
                            draft.editorial_project(),
                        )?;
                        draft.replace_timeline_compilation(reconciled)?;
                    }
                    CompoundProjectActionResult::TimelineEdited(edit)
                }
                CompoundProjectAction::MutateTracks(mutations) => {
                    let old_project = draft.editorial_project().clone();
                    let retained = draft
                        .graphs()
                        .filter_map(|graph| graph.as_timeline().cloned())
                        .collect::<Vec<_>>();
                    let (editorial, mix_state) = draft.editorial_and_clip_mix_mut();
                    let result =
                        apply_track_mutation_batch(editorial, editorial.revision(), mutations)?;
                    reconcile_removed_track_clip_mix(&old_project, editorial, mix_state)?;
                    for compilation in retained {
                        let reconciled = recompile_timeline_preserving_edits(
                            &old_project,
                            &compilation,
                            draft.editorial_project(),
                        )?;
                        draft.replace_timeline_compilation(reconciled)?;
                    }
                    CompoundProjectActionResult::TracksMutated(result)
                }
                CompoundProjectAction::MutateGraph {
                    graph_id,
                    mutations,
                } => {
                    let graph = draft.graph_mut(*graph_id)?.graph_mut();
                    let snapshot = graph.apply(GraphTransaction::with_mutations(
                        graph.revision(),
                        mutations.clone(),
                    ))?;
                    CompoundProjectActionResult::GraphMutated {
                        graph_id: *graph_id,
                        revision: snapshot.revision(),
                    }
                }
                CompoundProjectAction::MutateMedia(command) => {
                    CompoundProjectActionResult::MediaMutated(draft.execute_media_command(command)?)
                }
                CompoundProjectAction::ImportMedia(media) => {
                    CompoundProjectActionResult::MediaImported(draft.import_media(media)?)
                }
                CompoundProjectAction::MutateClipMix(mutations) => {
                    let mix_state = draft.clip_mix_state_mut();
                    let revision = mix_state.apply(mix_state.revision(), mutations)?;
                    CompoundProjectActionResult::ClipMixMutated(revision)
                }
                CompoundProjectAction::MutateExtension(command) => {
                    CompoundProjectActionResult::ExtensionMutated(
                        draft.execute_extension_command(command)?,
                    )
                }
            };
            draft.validate().with_error_context(
                ErrorContext::new(COMPONENT, "execute_compound_action")
                    .with_field("action_index", index.to_string())
                    .with_field("action", action.code()),
            )?;
            action_results.push(result);
        }
        Ok(())
    })?;
    Ok(CompoundProjectTransactionOutcome {
        snapshot,
        actions: action_results,
    })
}

fn transaction_error(
    category: ErrorCategory,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, Recoverability::UserCorrectable, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
