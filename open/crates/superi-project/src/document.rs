//! Whole-project state, immutable snapshots, and atomic publication.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{GraphId, ProjectId, TimelineId};
use superi_graph::mutate::{EditableGraph, GraphSnapshot};
use superi_timeline::compile::{
    compile_timeline, CompiledTimelineGraphValue, TimelineGraphCompilation,
};
use superi_timeline::model::EditorialProject;

use crate::settings::{ProjectSettings, ProjectSettingsTransaction};

const COMPONENT: &str = "superi-project.document";

/// One graph retained as ordinary editable project state.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProjectGraph {
    /// A graph compiled from one timeline with editorial provenance retained.
    Timeline(TimelineGraphCompilation),
    /// A named graph that is not owned by an editorial timeline.
    Standalone(StandaloneProjectGraph),
}

impl ProjectGraph {
    /// Reconstructs one timeline graph around checked externally decoded editable state.
    ///
    /// Provenance is regenerated deterministically from the validated editorial project. The
    /// supplied graph must retain the stable identity derived for the same project and root.
    pub fn restore_timeline(
        editorial_project: &EditorialProject,
        root_timeline_id: TimelineId,
        graph: EditableGraph<CompiledTimelineGraphValue>,
    ) -> Result<Self> {
        let compilation =
            compile_timeline(editorial_project, root_timeline_id)?.with_graph(graph)?;
        Ok(Self::Timeline(compilation))
    }

    /// Returns the stable graph identity.
    #[must_use]
    pub fn graph_id(&self) -> GraphId {
        self.graph().snapshot().graph_id()
    }

    /// Returns the editable graph document.
    #[must_use]
    pub const fn graph(&self) -> &EditableGraph<CompiledTimelineGraphValue> {
        match self {
            Self::Timeline(compilation) => compilation.graph(),
            Self::Standalone(standalone) => standalone.graph(),
        }
    }

    /// Returns mutable access for checked graph transactions.
    pub fn graph_mut(&mut self) -> &mut EditableGraph<CompiledTimelineGraphValue> {
        match self {
            Self::Timeline(compilation) => compilation.graph_mut(),
            Self::Standalone(standalone) => standalone.graph_mut(),
        }
    }

    /// Captures an immutable graph snapshot.
    #[must_use]
    pub fn snapshot(&self) -> GraphSnapshot<CompiledTimelineGraphValue> {
        self.graph().snapshot()
    }

    /// Returns the timeline compilation when this is a timeline graph.
    #[must_use]
    pub const fn as_timeline(&self) -> Option<&TimelineGraphCompilation> {
        match self {
            Self::Timeline(compilation) => Some(compilation),
            Self::Standalone(_) => None,
        }
    }

    /// Returns mutable timeline compilation access when applicable.
    pub fn as_timeline_mut(&mut self) -> Option<&mut TimelineGraphCompilation> {
        match self {
            Self::Timeline(compilation) => Some(compilation),
            Self::Standalone(_) => None,
        }
    }

    /// Returns the standalone graph when this is not timeline-owned.
    #[must_use]
    pub const fn as_standalone(&self) -> Option<&StandaloneProjectGraph> {
        match self {
            Self::Timeline(_) => None,
            Self::Standalone(standalone) => Some(standalone),
        }
    }

    /// Returns mutable standalone graph access when applicable.
    pub fn as_standalone_mut(&mut self) -> Option<&mut StandaloneProjectGraph> {
        match self {
            Self::Timeline(_) => None,
            Self::Standalone(standalone) => Some(standalone),
        }
    }

    /// Returns the editorial root when this is a timeline graph.
    #[must_use]
    pub const fn root_timeline_id(&self) -> Option<TimelineId> {
        match self {
            Self::Timeline(compilation) => Some(compilation.root_timeline_id()),
            Self::Standalone(_) => None,
        }
    }
}

/// A named project graph with no required editorial owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StandaloneProjectGraph {
    name: String,
    graph: EditableGraph<CompiledTimelineGraphValue>,
}

impl StandaloneProjectGraph {
    /// Creates a named standalone graph.
    pub fn new(
        name: impl Into<String>,
        graph: EditableGraph<CompiledTimelineGraphValue>,
    ) -> Result<Self> {
        let graph = Self {
            name: name.into(),
            graph,
        };
        graph.validate("create_standalone_graph")?;
        Ok(graph)
    }

    /// Returns the editor-facing graph name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Replaces the editor-facing graph name.
    pub fn set_name(&mut self, name: impl Into<String>) -> Result<()> {
        let name = name.into();
        validate_name(&name, "rename_standalone_graph")?;
        self.name = name;
        Ok(())
    }

    /// Returns the stable graph identity.
    #[must_use]
    pub fn graph_id(&self) -> GraphId {
        self.graph.snapshot().graph_id()
    }

    /// Returns the editable graph document.
    #[must_use]
    pub const fn graph(&self) -> &EditableGraph<CompiledTimelineGraphValue> {
        &self.graph
    }

    /// Returns mutable access for checked graph transactions.
    pub fn graph_mut(&mut self) -> &mut EditableGraph<CompiledTimelineGraphValue> {
        &mut self.graph
    }

    /// Captures an immutable graph snapshot.
    #[must_use]
    pub fn snapshot(&self) -> GraphSnapshot<CompiledTimelineGraphValue> {
        self.graph.snapshot()
    }

    fn validate(&self, operation: &'static str) -> Result<()> {
        validate_name(&self.name, operation)
    }
}

/// The mutable owner of one coherent project revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectDocument {
    revision: u64,
    state: Arc<ProjectState>,
}

impl ProjectDocument {
    /// Creates a project document and compiles its selected root timeline.
    pub fn new(editorial_project: EditorialProject, root_timeline_id: TimelineId) -> Result<Self> {
        let compilation = compile_timeline(&editorial_project, root_timeline_id)?;
        let settings = ProjectSettings::defaults(
            editorial_project
                .timeline(root_timeline_id)
                .expect("timeline compilation validated the selected root")
                .edit_rate(),
        )?;
        Self::from_parts_with_settings(
            0,
            editorial_project,
            root_timeline_id,
            settings,
            [ProjectGraph::Timeline(compilation)],
        )
    }

    /// Restores a complete project aggregate at an explicit document revision.
    ///
    /// This constructor validates the cross-object relationships but performs
    /// no persistence or migration work. Serialization layers remain
    /// responsible for decoding and version migration before calling it.
    pub fn from_parts<G>(
        revision: u64,
        editorial_project: EditorialProject,
        root_timeline_id: TimelineId,
        graphs: G,
    ) -> Result<Self>
    where
        G: IntoIterator<Item = ProjectGraph>,
    {
        let root_edit_rate = editorial_project
            .timeline(root_timeline_id)
            .ok_or_else(|| {
                Error::new(
                    ErrorCategory::NotFound,
                    Recoverability::UserCorrectable,
                    "selected root timeline was not found in the editorial project",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "restore_document")
                        .with_field("timeline_id", root_timeline_id.to_string()),
                )
            })?
            .edit_rate();
        Self::from_parts_with_settings(
            revision,
            editorial_project,
            root_timeline_id,
            ProjectSettings::defaults(root_edit_rate)?,
            graphs,
        )
    }

    /// Restores a complete aggregate with explicit validated project settings.
    pub fn from_parts_with_settings<G>(
        revision: u64,
        editorial_project: EditorialProject,
        root_timeline_id: TimelineId,
        settings: ProjectSettings,
        graphs: G,
    ) -> Result<Self>
    where
        G: IntoIterator<Item = ProjectGraph>,
    {
        let mut graphs_by_id = BTreeMap::new();
        for graph in graphs {
            let graph_id = graph.graph_id();
            if graphs_by_id.insert(graph_id, graph).is_some() {
                return Err(conflict(
                    "restore_document",
                    "duplicate graph identity in project document",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "restore_document")
                        .with_field("graph_id", graph_id.to_string()),
                ));
            }
        }

        let state = ProjectState {
            editorial_project,
            root_timeline_id,
            settings,
            graphs: graphs_by_id,
        };
        state.validate("restore_document")?;
        Ok(Self {
            revision,
            state: Arc::new(state),
        })
    }

    /// Returns the stable project identity.
    #[must_use]
    pub fn project_id(&self) -> ProjectId {
        self.state.editorial_project.id()
    }

    /// Returns the latest successfully published document revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the timeline selected as the project root.
    #[must_use]
    pub fn root_timeline_id(&self) -> TimelineId {
        self.state.root_timeline_id
    }

    /// Returns the editorial project at this document revision.
    #[must_use]
    pub fn editorial_project(&self) -> &EditorialProject {
        &self.state.editorial_project
    }

    /// Returns the authoritative project settings at this revision.
    #[must_use]
    pub fn settings(&self) -> &ProjectSettings {
        &self.state.settings
    }

    /// Iterates retained graphs in stable identity order.
    pub fn graphs(&self) -> impl ExactSizeIterator<Item = &ProjectGraph> {
        self.state.graphs.values()
    }

    /// Looks up any retained graph by stable identity.
    #[must_use]
    pub fn graph(&self, graph_id: GraphId) -> Option<&ProjectGraph> {
        self.state.graphs.get(&graph_id)
    }

    /// Looks up the retained compilation for an editorial root.
    #[must_use]
    pub fn timeline_graph(
        &self,
        root_timeline_id: TimelineId,
    ) -> Option<&TimelineGraphCompilation> {
        self.state.timeline_graph(root_timeline_id)
    }

    /// Captures an immutable view unaffected by later document edits.
    #[must_use]
    pub fn snapshot(&self) -> ProjectSnapshot {
        ProjectSnapshot {
            revision: self.revision,
            state: Arc::clone(&self.state),
        }
    }

    /// Applies one whole-project edit or publishes none of it.
    ///
    /// Empty edits do not advance the revision. Failed closures, stale
    /// revisions, invalid relationships, and revision exhaustion preserve the
    /// previously published snapshot.
    pub fn edit<F>(&mut self, expected_revision: u64, edit: F) -> Result<ProjectSnapshot>
    where
        F: FnOnce(&mut ProjectDraft<'_>) -> Result<()>,
    {
        self.check_revision("edit_document", expected_revision)?;

        let mut candidate = self.state.as_ref().clone();
        edit(&mut ProjectDraft {
            state: &mut candidate,
        })?;
        candidate.validate("edit_document")?;

        if candidate == *self.state {
            return Ok(self.snapshot());
        }

        let next_revision = self.revision.checked_add(1).ok_or_else(|| {
            Error::new(
                ErrorCategory::ResourceExhausted,
                Recoverability::Terminal,
                "project document revision is exhausted",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "edit_document")
                    .with_field("revision", self.revision.to_string()),
            )
        })?;
        self.revision = next_revision;
        self.state = Arc::new(candidate);
        Ok(self.snapshot())
    }

    /// Applies one strict project settings transaction through whole-project publication.
    pub fn execute_settings_transaction(
        &mut self,
        transaction: ProjectSettingsTransaction,
    ) -> Result<ProjectSnapshot> {
        let expected_revision = transaction.expected_revision();
        self.edit(expected_revision, move |draft| {
            let settings = draft.settings().apply(transaction)?;
            draft.replace_settings(settings);
            Ok(())
        })
    }

    /// Restores one validated historical state under a fresh document revision.
    ///
    /// The target's captured revision is evidence only and is never republished. Stale fences,
    /// cross-project targets, invalid state, semantic no-ops, and revision exhaustion preserve the
    /// current document and every previously captured snapshot.
    pub fn restore_snapshot(
        &mut self,
        expected_revision: u64,
        target: &ProjectSnapshot,
    ) -> Result<ProjectSnapshot> {
        self.check_revision("restore_snapshot", expected_revision)?;
        if target.project_id() != self.project_id() {
            return Err(conflict(
                "restore_snapshot",
                "historical snapshot belongs to another project",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "restore_snapshot")
                    .with_field("project_id", self.project_id().to_string())
                    .with_field("target_project_id", target.project_id().to_string()),
            ));
        }
        target.state.validate("restore_snapshot")?;
        if target.state == self.state {
            return Ok(self.snapshot());
        }

        let next_revision = self.revision.checked_add(1).ok_or_else(|| {
            Error::new(
                ErrorCategory::ResourceExhausted,
                Recoverability::Terminal,
                "project document revision is exhausted",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "restore_snapshot")
                    .with_field("revision", self.revision.to_string()),
            )
        })?;
        self.revision = next_revision;
        self.state = Arc::clone(&target.state);
        Ok(self.snapshot())
    }

    pub(crate) fn check_revision(
        &self,
        operation: &'static str,
        expected_revision: u64,
    ) -> Result<()> {
        if expected_revision != self.revision {
            return Err(stale_revision(operation, expected_revision, self.revision));
        }
        Ok(())
    }
}

/// An immutable, thread-safe view of one published project revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSnapshot {
    revision: u64,
    state: Arc<ProjectState>,
}

impl ProjectSnapshot {
    /// Returns the stable project identity.
    #[must_use]
    pub fn project_id(&self) -> ProjectId {
        self.state.editorial_project.id()
    }

    /// Returns the captured document revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the selected root timeline.
    #[must_use]
    pub fn root_timeline_id(&self) -> TimelineId {
        self.state.root_timeline_id
    }

    /// Returns the captured editorial project.
    #[must_use]
    pub fn editorial_project(&self) -> &EditorialProject {
        &self.state.editorial_project
    }

    /// Returns the captured authoritative project settings.
    #[must_use]
    pub fn settings(&self) -> &ProjectSettings {
        &self.state.settings
    }

    /// Iterates captured graphs in stable identity order.
    pub fn graphs(&self) -> impl ExactSizeIterator<Item = &ProjectGraph> {
        self.state.graphs.values()
    }

    /// Looks up any captured graph by stable identity.
    #[must_use]
    pub fn graph(&self, graph_id: GraphId) -> Option<&ProjectGraph> {
        self.state.graphs.get(&graph_id)
    }

    /// Looks up a captured compilation by editorial root.
    #[must_use]
    pub fn timeline_graph(
        &self,
        root_timeline_id: TimelineId,
    ) -> Option<&TimelineGraphCompilation> {
        self.state.timeline_graph(root_timeline_id)
    }
}

/// A private candidate exposed only during one atomic document edit.
pub struct ProjectDraft<'a> {
    state: &'a mut ProjectState,
}

impl ProjectDraft<'_> {
    /// Returns the stable project identity.
    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.state.editorial_project.id()
    }

    /// Returns the selected root timeline.
    #[must_use]
    pub const fn root_timeline_id(&self) -> TimelineId {
        self.state.root_timeline_id
    }

    /// Selects a new root timeline.
    pub fn set_root_timeline_id(&mut self, root_timeline_id: TimelineId) {
        self.state.root_timeline_id = root_timeline_id;
    }

    /// Returns the candidate editorial project.
    #[must_use]
    pub const fn editorial_project(&self) -> &EditorialProject {
        &self.state.editorial_project
    }

    /// Returns mutable editorial access through its checked edit surface.
    pub fn editorial_project_mut(&mut self) -> &mut EditorialProject {
        &mut self.state.editorial_project
    }

    /// Returns the candidate project settings.
    #[must_use]
    pub const fn settings(&self) -> &ProjectSettings {
        &self.state.settings
    }

    /// Replaces the candidate with another fully validated settings snapshot.
    pub fn replace_settings(&mut self, settings: ProjectSettings) {
        self.state.settings = settings;
    }

    /// Iterates candidate graphs in stable identity order.
    pub fn graphs(&self) -> impl ExactSizeIterator<Item = &ProjectGraph> {
        self.state.graphs.values()
    }

    /// Looks up any candidate graph.
    #[must_use]
    pub fn graph(&self, graph_id: GraphId) -> Option<&ProjectGraph> {
        self.state.graphs.get(&graph_id)
    }

    /// Looks up any candidate graph for checked mutation.
    pub fn graph_mut(&mut self, graph_id: GraphId) -> Result<&mut ProjectGraph> {
        self.state.graphs.get_mut(&graph_id).ok_or_else(|| {
            Error::new(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "graph was not found in the project document",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "edit_graph")
                    .with_field("graph_id", graph_id.to_string()),
            )
        })
    }

    /// Looks up a candidate compilation by editorial root.
    #[must_use]
    pub fn timeline_graph(
        &self,
        root_timeline_id: TimelineId,
    ) -> Option<&TimelineGraphCompilation> {
        self.state.timeline_graph(root_timeline_id)
    }

    /// Looks up a candidate compilation for checked graph transactions.
    pub fn timeline_graph_mut(
        &mut self,
        root_timeline_id: TimelineId,
    ) -> Result<&mut TimelineGraphCompilation> {
        self.state
            .graphs
            .values_mut()
            .find_map(|graph| match graph {
                ProjectGraph::Timeline(compilation)
                    if compilation.root_timeline_id() == root_timeline_id =>
                {
                    Some(compilation)
                }
                _ => None,
            })
            .ok_or_else(|| {
                Error::new(
                    ErrorCategory::NotFound,
                    Recoverability::UserCorrectable,
                    "timeline graph was not found in the project document",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "edit_timeline_graph")
                        .with_field("timeline_id", root_timeline_id.to_string()),
                )
            })
    }

    /// Inserts a new graph without replacing an existing identity.
    pub fn insert_graph(&mut self, graph: ProjectGraph) -> Result<()> {
        let graph_id = graph.graph_id();
        if self.state.graphs.contains_key(&graph_id) {
            return Err(conflict(
                "insert_graph",
                "graph identity already exists in the project document",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "insert_graph")
                    .with_field("graph_id", graph_id.to_string()),
            ));
        }
        self.state.graphs.insert(graph_id, graph);
        Ok(())
    }

    /// Replaces or inserts a graph by its stable identity.
    pub fn replace_graph(&mut self, graph: ProjectGraph) -> Option<ProjectGraph> {
        self.state.graphs.insert(graph.graph_id(), graph)
    }

    /// Removes a non-root graph by stable identity.
    pub fn remove_graph(&mut self, graph_id: GraphId) -> Result<ProjectGraph> {
        if self
            .state
            .graphs
            .get(&graph_id)
            .and_then(ProjectGraph::root_timeline_id)
            == Some(self.state.root_timeline_id)
        {
            return Err(conflict(
                "remove_graph",
                "selected root timeline graph cannot be removed",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "remove_graph")
                    .with_field("graph_id", graph_id.to_string()),
            ));
        }
        self.state.graphs.remove(&graph_id).ok_or_else(|| {
            Error::new(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "graph was not found in the project document",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "remove_graph")
                    .with_field("graph_id", graph_id.to_string()),
            )
        })
    }

    /// Recompiles one timeline from the candidate editorial revision.
    pub fn recompile_timeline(&mut self, root_timeline_id: TimelineId) -> Result<()> {
        let compilation = compile_timeline(&self.state.editorial_project, root_timeline_id)?;
        self.replace_timeline_compilation(compilation)
    }

    /// Replaces the retained compilation for the same editorial root.
    pub fn replace_timeline_compilation(
        &mut self,
        compilation: TimelineGraphCompilation,
    ) -> Result<()> {
        let root_timeline_id = compilation.root_timeline_id();
        let new_graph_id = compilation.snapshot().graph_id();
        let old_graph_id = self.state.graphs.iter().find_map(|(graph_id, graph)| {
            (graph.root_timeline_id() == Some(root_timeline_id)).then_some(*graph_id)
        });

        if self.state.graphs.contains_key(&new_graph_id) && old_graph_id != Some(new_graph_id) {
            return Err(conflict(
                "replace_timeline_compilation",
                "compiled graph identity conflicts with another project graph",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "replace_timeline_compilation")
                    .with_field("timeline_id", root_timeline_id.to_string())
                    .with_field("graph_id", new_graph_id.to_string()),
            ));
        }

        if let Some(old_graph_id) = old_graph_id {
            self.state.graphs.remove(&old_graph_id);
        }
        self.state
            .graphs
            .insert(new_graph_id, ProjectGraph::Timeline(compilation));
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ProjectState {
    editorial_project: EditorialProject,
    root_timeline_id: TimelineId,
    settings: ProjectSettings,
    graphs: BTreeMap<GraphId, ProjectGraph>,
}

impl ProjectState {
    fn timeline_graph(&self, root_timeline_id: TimelineId) -> Option<&TimelineGraphCompilation> {
        self.graphs.values().find_map(|graph| match graph {
            ProjectGraph::Timeline(compilation)
                if compilation.root_timeline_id() == root_timeline_id =>
            {
                Some(compilation)
            }
            _ => None,
        })
    }

    fn validate(&self, operation: &'static str) -> Result<()> {
        if self
            .editorial_project
            .timeline(self.root_timeline_id)
            .is_none()
        {
            return Err(Error::new(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "selected root timeline was not found in the editorial project",
            )
            .with_context(
                ErrorContext::new(COMPONENT, operation)
                    .with_field("project_id", self.editorial_project.id().to_string())
                    .with_field("timeline_id", self.root_timeline_id.to_string()),
            ));
        }

        let mut timeline_roots = BTreeSet::new();
        let mut selected_root_found = false;
        for (graph_id, graph) in &self.graphs {
            let actual_graph_id = graph.graph_id();
            if *graph_id != actual_graph_id {
                return Err(Error::new(
                    ErrorCategory::CorruptData,
                    Recoverability::UserCorrectable,
                    "project graph map key does not match the graph identity",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, operation)
                        .with_field("map_graph_id", graph_id.to_string())
                        .with_field("actual_graph_id", actual_graph_id.to_string()),
                ));
            }

            match graph {
                ProjectGraph::Timeline(compilation) => {
                    validate_compilation(&self.editorial_project, compilation, operation)?;
                    let root_timeline_id = compilation.root_timeline_id();
                    if !timeline_roots.insert(root_timeline_id) {
                        return Err(conflict(
                            operation,
                            "multiple project graphs compile the same timeline root",
                        )
                        .with_context(
                            ErrorContext::new(COMPONENT, operation)
                                .with_field("timeline_id", root_timeline_id.to_string()),
                        ));
                    }
                    selected_root_found |= root_timeline_id == self.root_timeline_id;
                }
                ProjectGraph::Standalone(standalone) => standalone.validate(operation)?,
            }
        }

        if !selected_root_found {
            return Err(conflict(
                operation,
                "selected root timeline has no retained compiled graph",
            )
            .with_context(
                ErrorContext::new(COMPONENT, operation)
                    .with_field("timeline_id", self.root_timeline_id.to_string()),
            ));
        }
        Ok(())
    }
}

fn validate_compilation(
    editorial_project: &EditorialProject,
    compilation: &TimelineGraphCompilation,
    operation: &'static str,
) -> Result<()> {
    if compilation.project_id() != editorial_project.id() {
        return Err(
            conflict(operation, "timeline compilation belongs to another project").with_context(
                ErrorContext::new(COMPONENT, operation)
                    .with_field("project_id", editorial_project.id().to_string())
                    .with_field("compiled_project_id", compilation.project_id().to_string()),
            ),
        );
    }
    if editorial_project
        .timeline(compilation.root_timeline_id())
        .is_none()
    {
        return Err(Error::new(
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            "compiled timeline root was not found in the editorial project",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation)
                .with_field("timeline_id", compilation.root_timeline_id().to_string()),
        ));
    }
    if compilation.project_revision() != editorial_project.revision() {
        return Err(conflict(
            operation,
            "timeline compilation is stale for the editorial project revision",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation)
                .with_field("project_revision", editorial_project.revision().to_string())
                .with_field(
                    "compiled_project_revision",
                    compilation.project_revision().to_string(),
                ),
        ));
    }
    Ok(())
}

fn validate_name(name: &str, operation: &'static str) -> Result<()> {
    if name.trim().is_empty() {
        return Err(Error::new(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "standalone project graph name cannot be blank",
        )
        .with_context(ErrorContext::new(COMPONENT, operation)));
    }
    Ok(())
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn stale_revision(operation: &'static str, expected_revision: u64, actual_revision: u64) -> Error {
    conflict(
        operation,
        "project document revision does not match the expected revision",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("expected_revision", expected_revision.to_string())
            .with_field("actual_revision", actual_revision.to_string()),
    )
}
