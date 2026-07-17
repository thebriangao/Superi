use superi_core::error::{Error, ErrorCategory, Recoverability};
use superi_core::ids::{GeneratorId, GraphId, ProjectId, TimelineId, TrackId};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_graph::mutate::{EditableGraph, GraphMutation, GraphTransaction, TypedParameterValue};
use superi_graph::value::GraphValue;
use superi_project::document::{ProjectDocument, ProjectGraph, StandaloneProjectGraph};
use superi_timeline::compile::{
    CompiledTimelineGraphValue, TimelineGraphOrigin, TimelineGraphValue,
};
use superi_timeline::model::{
    EditorialObjectId, EditorialProject, Generator, LinkedMediaReference, Timeline, Track,
    TrackItem, TrackSemantics, VideoCompositing, VideoTrackSemantics,
};

const PROJECT: ProjectId = ProjectId::from_raw(1);
const ROOT: TimelineId = TimelineId::from_raw(2);
const TRACK: TrackId = TrackId::from_raw(3);
const GENERATOR: GeneratorId = GeneratorId::from_raw(4);

fn range(start: i64, duration: u64, timebase: Timebase) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, timebase),
        Duration::new(duration, timebase).unwrap(),
    )
    .unwrap()
}

fn editorial_project() -> EditorialProject {
    editorial_project_with_id(PROJECT)
}

fn editorial_project_with_id(project_id: ProjectId) -> EditorialProject {
    let edit_rate = Timebase::integer(24).unwrap();
    let generator = Generator::new(
        GENERATOR,
        "generated title",
        "solid_color",
        [("rgba".to_owned(), "0.1,0.2,0.3,1".to_owned())].into(),
        range(0, 48, edit_rate),
    );
    let timeline = Timeline::new(
        ROOT,
        "main timeline",
        edit_rate,
        RationalTime::zero(edit_rate),
        vec![Track::new(
            TRACK,
            "V1",
            TrackSemantics::Video(VideoTrackSemantics::new(
                FrameRate::FPS_24,
                VideoCompositing::Over,
            )),
            vec![TrackItem::Generator(generator)],
        )],
    );
    EditorialProject::new(
        project_id,
        "whole project",
        std::iter::empty::<LinkedMediaReference>(),
        [timeline],
    )
    .unwrap()
}

#[test]
fn snapshot_restoration_is_revision_fenced_monotonic_and_atomic() {
    let mut document = ProjectDocument::new(editorial_project(), ROOT).unwrap();
    let original = document.snapshot();
    let (node_id, parameter_id, value_type) = generator_name_address(&document);
    document
        .edit(0, |draft| {
            let compilation = draft.timeline_graph_mut(ROOT)?;
            let graph_revision = compilation.graph().revision();
            compilation
                .graph_mut()
                .apply(GraphTransaction::with_mutations(
                    graph_revision,
                    [GraphMutation::SetParameter {
                        node_id,
                        parameter_id,
                        value: TypedParameterValue::new(
                            value_type,
                            GraphValue::domain(TimelineGraphValue::Text(
                                "later editable state".to_owned(),
                            )),
                        ),
                    }],
                ))?;
            Ok(())
        })
        .unwrap();
    let later = document.snapshot();

    let stale = document.restore_snapshot(0, &original).unwrap_err();
    assert_eq!(stale.category(), ErrorCategory::Conflict);
    assert_eq!(document.snapshot(), later);

    let other_project =
        ProjectDocument::new(editorial_project_with_id(ProjectId::from_raw(999)), ROOT).unwrap();
    let cross_project = document
        .restore_snapshot(1, &other_project.snapshot())
        .unwrap_err();
    assert_eq!(cross_project.category(), ErrorCategory::Conflict);
    assert_eq!(document.snapshot(), later);

    let restored = document.restore_snapshot(1, &original).unwrap();
    assert_eq!(restored.revision(), 2);
    assert_eq!(restored.project_id(), original.project_id());
    assert_eq!(restored.root_timeline_id(), original.root_timeline_id());
    assert_eq!(restored.editorial_project(), original.editorial_project());
    assert_eq!(
        restored.graphs().cloned().collect::<Vec<_>>(),
        original.graphs().cloned().collect::<Vec<_>>()
    );
    assert_eq!(generator_name(&document), "generated title");
    assert_eq!(
        later
            .timeline_graph(ROOT)
            .unwrap()
            .snapshot()
            .node(node_id)
            .unwrap()
            .parameters()
            .get(&parameter_id)
            .unwrap()
            .value()
            .payload()
            .as_domain(),
        Some(&TimelineGraphValue::Text("later editable state".to_owned()))
    );

    let unchanged = document.restore_snapshot(2, &original).unwrap();
    assert_eq!(unchanged, restored);
    assert_eq!(document.revision(), 2);

    let mut exhausted = ProjectDocument::from_parts(
        u64::MAX,
        original.editorial_project().clone(),
        original.root_timeline_id(),
        original.graphs().cloned(),
    )
    .unwrap();
    let exhausted_before = exhausted.snapshot();
    let error = exhausted.restore_snapshot(u64::MAX, &later).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(exhausted.snapshot(), exhausted_before);
}

fn generator_name_address(
    document: &ProjectDocument,
) -> (
    superi_graph::ids::NodeId,
    superi_graph::ids::ParameterId,
    superi_graph::node::ValueTypeId,
) {
    let compilation = document.timeline_graph(ROOT).unwrap();
    let node_id = compilation
        .index()
        .node(TimelineGraphOrigin::Object(EditorialObjectId::Generator(
            GENERATOR,
        )))
        .unwrap();
    let snapshot = compilation.snapshot();
    let parameter = snapshot
        .node(node_id)
        .unwrap()
        .parameters()
        .values()
        .find(|parameter| parameter.name().as_str() == "name")
        .unwrap();
    (
        node_id,
        parameter.id(),
        parameter.value().value_type().clone(),
    )
}

fn generator_name(document: &ProjectDocument) -> String {
    let compilation = document.timeline_graph(ROOT).unwrap();
    let node_id = compilation
        .index()
        .node(TimelineGraphOrigin::Object(EditorialObjectId::Generator(
            GENERATOR,
        )))
        .unwrap();
    compilation
        .snapshot()
        .node(node_id)
        .unwrap()
        .parameters()
        .values()
        .find(|parameter| parameter.name().as_str() == "name")
        .unwrap()
        .value()
        .payload()
        .as_domain()
        .and_then(|value| match value {
            TimelineGraphValue::Text(value) => Some(value.clone()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn document_owns_one_coherent_editorial_and_graph_snapshot() {
    let document = ProjectDocument::new(editorial_project(), ROOT).unwrap();

    assert_eq!(document.project_id(), PROJECT);
    assert_eq!(document.revision(), 0);
    assert_eq!(document.root_timeline_id(), ROOT);
    assert_eq!(document.editorial_project().revision(), 0);
    assert_eq!(document.graphs().len(), 1);

    let compilation = document.timeline_graph(ROOT).unwrap();
    assert_eq!(compilation.project_id(), PROJECT);
    assert_eq!(compilation.root_timeline_id(), ROOT);
    assert_eq!(compilation.project_revision(), 0);
    assert_eq!(compilation.snapshot().revision(), 1);
    assert_eq!(generator_name(&document), "generated title");

    let snapshot = document.snapshot();
    fn assert_send_sync<T: Send + Sync>(_value: &T) {}
    assert_send_sync(&snapshot);
    let observed = std::thread::scope(|scope| {
        ["editor", "script", "headless"]
            .map(|_| {
                let snapshot = snapshot.clone();
                scope.spawn(move || snapshot)
            })
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect::<Vec<_>>()
    });
    assert_eq!(observed[0], observed[1]);
    assert_eq!(observed[1], observed[2]);
}

#[test]
fn intelligent_results_remain_ordinary_editable_graph_state() {
    let mut document = ProjectDocument::new(editorial_project(), ROOT).unwrap();
    let before = document.snapshot();
    let (node_id, parameter_id, value_type) = generator_name_address(&document);

    let published = document
        .edit(0, |draft| {
            let compilation = draft.timeline_graph_mut(ROOT)?;
            let graph_revision = compilation.graph().revision();
            compilation
                .graph_mut()
                .apply(GraphTransaction::with_mutations(
                    graph_revision,
                    [GraphMutation::SetParameter {
                        node_id,
                        parameter_id,
                        value: TypedParameterValue::new(
                            value_type,
                            GraphValue::domain(TimelineGraphValue::Text(
                                "editable generated result".to_owned(),
                            )),
                        ),
                    }],
                ))?;
            Ok(())
        })
        .unwrap();

    assert_eq!(document.revision(), 1);
    assert_eq!(published.revision(), 1);
    assert_eq!(generator_name(&document), "editable generated result");
    assert_eq!(
        before.timeline_graph(ROOT).unwrap().snapshot().revision(),
        1
    );
    assert_eq!(
        document.timeline_graph(ROOT).unwrap().snapshot().revision(),
        2
    );
}

#[test]
fn stale_or_failed_edits_publish_nothing() {
    let mut document = ProjectDocument::new(editorial_project(), ROOT).unwrap();
    let (node_id, parameter_id, value_type) = generator_name_address(&document);
    let before = document.snapshot();

    let stale = document.edit(9, |_| Ok(())).unwrap_err();
    assert_eq!(stale.category(), ErrorCategory::Conflict);
    assert_eq!(document.snapshot(), before);

    let failure = document
        .edit(0, |draft| {
            let compilation = draft.timeline_graph_mut(ROOT)?;
            let graph_revision = compilation.graph().revision();
            compilation
                .graph_mut()
                .apply(GraphTransaction::with_mutations(
                    graph_revision,
                    [GraphMutation::SetParameter {
                        node_id,
                        parameter_id,
                        value: TypedParameterValue::new(
                            value_type,
                            GraphValue::domain(TimelineGraphValue::Text("unpublished".to_owned())),
                        ),
                    }],
                ))?;
            Err(Error::new(
                ErrorCategory::Cancelled,
                Recoverability::Retryable,
                "test edit cancelled before publication",
            ))
        })
        .unwrap_err();
    assert_eq!(failure.category(), ErrorCategory::Cancelled);
    assert_eq!(document.snapshot(), before);

    let unchanged = document.edit(0, |_| Ok(())).unwrap();
    assert_eq!(unchanged.revision(), 0);
    assert_eq!(document.snapshot(), before);
}

#[test]
fn editorial_changes_require_a_matching_fresh_compilation() {
    let mut document = ProjectDocument::new(editorial_project(), ROOT).unwrap();
    let before = document.snapshot();

    let stale_graph = document
        .edit(0, |draft| {
            let revision = draft.editorial_project().revision();
            draft.editorial_project_mut().edit(revision, |editorial| {
                editorial.set_name("renamed project");
                Ok(())
            })
        })
        .unwrap_err();
    assert_eq!(stale_graph.category(), ErrorCategory::Conflict);
    assert_eq!(document.snapshot(), before);

    document
        .edit(0, |draft| {
            let revision = draft.editorial_project().revision();
            draft.editorial_project_mut().edit(revision, |editorial| {
                editorial.set_name("renamed project");
                Ok(())
            })?;
            draft.recompile_timeline(ROOT)
        })
        .unwrap();

    assert_eq!(document.revision(), 1);
    assert_eq!(document.editorial_project().revision(), 1);
    assert_eq!(document.editorial_project().name(), "renamed project");
    assert_eq!(document.timeline_graph(ROOT).unwrap().project_revision(), 1);
    assert_eq!(before.editorial_project().name(), "whole project");
}

#[test]
fn standalone_graphs_are_named_stable_project_state() {
    let mut document = ProjectDocument::new(editorial_project(), ROOT).unwrap();
    let graph_id = GraphId::from_raw(99);
    let standalone = StandaloneProjectGraph::new(
        "reusable analysis",
        EditableGraph::<CompiledTimelineGraphValue>::new(graph_id),
    )
    .unwrap();

    document
        .edit(0, |draft| {
            draft.insert_graph(ProjectGraph::Standalone(standalone))
        })
        .unwrap();

    assert_eq!(document.graphs().len(), 2);
    let ProjectGraph::Standalone(stored) = document.graph(graph_id).unwrap() else {
        panic!("standalone graph changed kind");
    };
    assert_eq!(stored.name(), "reusable analysis");
    assert_eq!(stored.graph().snapshot().graph_id(), graph_id);

    let before = document.snapshot();
    let duplicate = document
        .edit(1, |draft| {
            draft.insert_graph(ProjectGraph::Standalone(
                StandaloneProjectGraph::new(
                    "duplicate",
                    EditableGraph::<CompiledTimelineGraphValue>::new(graph_id),
                )
                .unwrap(),
            ))
        })
        .unwrap_err();
    assert_eq!(duplicate.category(), ErrorCategory::Conflict);
    assert_eq!(document.snapshot(), before);

    let blank = StandaloneProjectGraph::new(
        "   ",
        EditableGraph::<CompiledTimelineGraphValue>::new(GraphId::from_raw(100)),
    )
    .unwrap_err();
    assert_eq!(blank.category(), ErrorCategory::InvalidInput);

    let missing_root =
        ProjectDocument::new(editorial_project(), TimelineId::from_raw(999)).unwrap_err();
    assert_eq!(missing_root.category(), ErrorCategory::NotFound);
}

#[test]
fn checked_parts_restoration_preserves_edited_timeline_graph_state() {
    let mut document = ProjectDocument::new(editorial_project(), ROOT).unwrap();
    let (node_id, parameter_id, value_type) = generator_name_address(&document);
    document
        .edit(0, |draft| {
            let compilation = draft.timeline_graph_mut(ROOT)?;
            let graph_revision = compilation.graph().revision();
            compilation
                .graph_mut()
                .apply(GraphTransaction::with_mutations(
                    graph_revision,
                    [GraphMutation::SetParameter {
                        node_id,
                        parameter_id,
                        value: TypedParameterValue::new(
                            value_type,
                            GraphValue::domain(TimelineGraphValue::Text(
                                "durable editable result".to_owned(),
                            )),
                        ),
                    }],
                ))?;
            Ok(())
        })
        .unwrap();

    let snapshot = document.snapshot();
    let restored_graph = ProjectGraph::restore_timeline(
        snapshot.editorial_project(),
        ROOT,
        snapshot.timeline_graph(ROOT).unwrap().graph().clone(),
    )
    .unwrap();
    let mut restored = ProjectDocument::from_parts(
        41,
        snapshot.editorial_project().clone(),
        ROOT,
        [restored_graph],
    )
    .unwrap();

    assert_eq!(restored.revision(), 41);
    assert_eq!(
        restored.timeline_graph(ROOT).unwrap(),
        snapshot.timeline_graph(ROOT).unwrap()
    );
    assert_eq!(generator_name(&restored), "durable editable result");
    assert_eq!(restored.edit(41, |_| Ok(())).unwrap().revision(), 41);

    let mismatched = ProjectGraph::restore_timeline(
        snapshot.editorial_project(),
        ROOT,
        EditableGraph::<CompiledTimelineGraphValue>::new(GraphId::from_raw(999)),
    )
    .unwrap_err();
    assert_eq!(mismatched.category(), ErrorCategory::Conflict);
}
