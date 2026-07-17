use std::path::PathBuf;

use superi_core::error::ErrorCategory;
use superi_core::ids::{ClipId, MediaId, ProjectId, TimelineId, TrackId};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_engine::history::{
    ProjectCommandHistory, ProjectHistoryActionResult, ProjectHistoryCommand, ProjectMutation,
    ProjectMutationKind, DEFAULT_PROJECT_HISTORY_CAPACITY, MAX_PROJECT_HISTORY_CAPACITY,
};
use superi_engine::media::media_backend_registry;
use superi_engine::resources::{
    acquire_project_resources, DecoderResourceRequest, MediaResourceRequest,
    ResourceAcquisitionPolicy,
};
use superi_graph::mutate::{GraphMutation, GraphTransaction, TypedParameterValue};
use superi_graph::value::GraphValue;
use superi_media_io::demux::{SourceLocation, SourceRequest, StreamId};
use superi_media_io::operation::{MediaPriority, OperationContext};
use superi_project::document::{ProjectDocument, ProjectGraph, ProjectSnapshot};
use superi_project::media::{
    PortableRelativePath, ProjectMediaCommand, ProjectMediaCommandResult, ReferencedMediaPath,
};
use superi_project::ProjectDatabase;
use superi_timeline::compile::{TimelineGraphOrigin, TimelineGraphValue};
use superi_timeline::model::{
    Clip, ClipSource, EditorialObjectId, EditorialProject, LinkedMediaReference, Timeline, Track,
    TrackItem, TrackSemantics, VideoCompositing, VideoTrackSemantics,
};

const PROJECT: ProjectId = ProjectId::from_raw(0x700);
const MEDIA: MediaId = MediaId::from_raw(0x701);
const ROOT: TimelineId = TimelineId::from_raw(0x702);
const TRACK: TrackId = TrackId::from_raw(0x703);
const CLIP: ClipId = ClipId::from_raw(0x704);
const STREAM: StreamId = StreamId::new(1);
const FINGERPRINT: &str = "sha256:117f5cebcaaf788d1891e84aec57066c73e33d4af308368f640f28a8419f4bbc";

fn fixture_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/slice/video-cfr/v1/input.webm")
}

fn range(start: i64, duration: u64, timebase: Timebase) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, timebase),
        Duration::new(duration, timebase).unwrap(),
    )
    .unwrap()
}

fn document() -> ProjectDocument {
    let source_timebase = Timebase::NANOSECONDS;
    let edit_timebase = FrameRate::FPS_24.timebase();
    let path = ReferencedMediaPath::host_absolute(fixture_path()).unwrap();
    let media = LinkedMediaReference::with_fingerprint(
        MEDIA,
        "canonical source",
        path.to_target(),
        Some(range(0, 4_000_000_000, source_timebase)),
        FINGERPRINT,
    )
    .unwrap();
    let clip = Clip::new(
        CLIP,
        "history source",
        ClipSource::Media(MEDIA),
        range(1_000_000_000, 2_000_000_000, source_timebase),
        range(0, 48, edit_timebase),
    )
    .unwrap();
    let timeline = Timeline::new(
        ROOT,
        "history timeline",
        edit_timebase,
        RationalTime::zero(edit_timebase),
        vec![Track::new(
            TRACK,
            "V1",
            TrackSemantics::Video(VideoTrackSemantics::new(
                FrameRate::FPS_24,
                VideoCompositing::Over,
            )),
            vec![TrackItem::Clip(clip)],
        )],
    );
    let editorial = EditorialProject::new(PROJECT, "history project", [media], [timeline]).unwrap();
    let mut document = ProjectDocument::new(editorial, ROOT).unwrap();
    let compilation = document.timeline_graph(ROOT).unwrap();
    let node_id = compilation
        .index()
        .node(TimelineGraphOrigin::Object(EditorialObjectId::Clip(CLIP)))
        .unwrap();
    let graph = compilation.snapshot();
    let parameter = graph
        .node(node_id)
        .unwrap()
        .parameters()
        .values()
        .find(|parameter| parameter.name().as_str() == "name")
        .unwrap();
    let parameter_id = parameter.id();
    let value_type = parameter.value().value_type().clone();
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
                                "retained graph result".to_owned(),
                            )),
                        ),
                    }],
                ))?;
            Ok(())
        })
        .unwrap();
    document
}

fn portable(path: &str) -> ReferencedMediaPath {
    ReferencedMediaPath::project_relative(PortableRelativePath::new(path).unwrap())
}

fn compiled_clip_name(snapshot: &ProjectSnapshot) -> String {
    let compilation = snapshot.timeline_graph(ROOT).unwrap();
    let node_id = compilation
        .index()
        .node(TimelineGraphOrigin::Object(EditorialObjectId::Clip(CLIP)))
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
fn project_history_applies_undoes_redoes_and_preserves_failed_branches() {
    assert_eq!(DEFAULT_PROJECT_HISTORY_CAPACITY, 64);
    assert_eq!(MAX_PROJECT_HISTORY_CAPACITY, 4096);
    assert_eq!(
        ProjectCommandHistory::with_capacity(document(), 0)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        ProjectCommandHistory::with_capacity(document(), MAX_PROJECT_HISTORY_CAPACITY + 1)
            .unwrap_err()
            .category(),
        ErrorCategory::ResourceExhausted
    );

    let mut history = ProjectCommandHistory::with_capacity(document(), 2).unwrap();
    let original = history.state();
    assert_eq!(original.snapshot().revision(), 1);
    assert_eq!(
        compiled_clip_name(original.snapshot()),
        "retained graph result"
    );

    let relinked = portable("Media/relinked.webm");
    let applied = history
        .execute(ProjectHistoryCommand::apply(
            1,
            ProjectMutation::media(ProjectMediaCommand::set_path(MEDIA, relinked.clone())),
        ))
        .unwrap();
    assert!(applied.authored_state_changed());
    assert_eq!(applied.state().snapshot().revision(), 2);
    assert_eq!(
        applied.state().snapshot().media_path(MEDIA).unwrap(),
        relinked
    );
    assert_eq!(applied.state().undo_depth(), 1);
    assert_eq!(applied.state().redo_depth(), 0);
    assert_eq!(
        applied.state().next_undo(),
        Some(ProjectMutationKind::SetMediaPath)
    );
    assert_eq!(applied.state().next_redo(), None);
    assert_eq!(
        applied.action_result(),
        Some(&ProjectHistoryActionResult::Applied {
            mutation: ProjectMutationKind::SetMediaPath,
            media_result: ProjectMediaCommandResult::PathUpdated,
        })
    );

    let undone = history.execute(ProjectHistoryCommand::undo(2)).unwrap();
    assert_eq!(undone.state().snapshot().revision(), 3);
    assert_eq!(
        undone.state().snapshot().media_path(MEDIA).unwrap(),
        original.snapshot().media_path(MEDIA).unwrap()
    );
    assert_eq!(
        compiled_clip_name(undone.state().snapshot()),
        "retained graph result"
    );
    assert_eq!(undone.state().undo_depth(), 0);
    assert_eq!(undone.state().redo_depth(), 1);
    assert_eq!(
        undone.action_result(),
        Some(&ProjectHistoryActionResult::Undone(
            ProjectMutationKind::SetMediaPath
        ))
    );

    let before_failure = history.state();
    let error = history
        .execute(ProjectHistoryCommand::apply(
            3,
            ProjectMutation::media(ProjectMediaCommand::mark_missing(MediaId::from_raw(999))),
        ))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotFound);
    assert_eq!(history.state(), before_failure);

    let redone = history.execute(ProjectHistoryCommand::redo(3)).unwrap();
    assert_eq!(redone.state().snapshot().revision(), 4);
    assert_eq!(
        redone.state().snapshot().media_path(MEDIA).unwrap(),
        relinked
    );
    assert_eq!(
        redone.action_result(),
        Some(&ProjectHistoryActionResult::Redone(
            ProjectMutationKind::SetMediaPath
        ))
    );

    history.execute(ProjectHistoryCommand::undo(4)).unwrap();
    let branched = history
        .execute(ProjectHistoryCommand::apply(
            5,
            ProjectMutation::media(ProjectMediaCommand::mark_missing(MEDIA)),
        ))
        .unwrap();
    assert_eq!(branched.state().snapshot().revision(), 6);
    assert_eq!(branched.state().redo_depth(), 0);
    let before_empty_redo = history.state();
    let error = history.execute(ProjectHistoryCommand::redo(6)).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(history.state(), before_empty_redo);

    let no_op = history
        .execute(ProjectHistoryCommand::apply(
            6,
            ProjectMutation::media(ProjectMediaCommand::mark_missing(MEDIA)),
        ))
        .unwrap();
    assert!(!no_op.authored_state_changed());
    assert_eq!(no_op.state(), &before_empty_redo);
}

#[test]
fn bounded_history_evicts_oldest_and_failures_move_no_entries() {
    let mut history = ProjectCommandHistory::with_capacity(document(), 2).unwrap();
    for (expected_revision, path) in [
        (1, "Media/a.webm"),
        (2, "Media/b.webm"),
        (3, "Media/c.webm"),
    ] {
        history
            .execute(ProjectHistoryCommand::apply(
                expected_revision,
                ProjectMutation::media(ProjectMediaCommand::set_path(MEDIA, portable(path))),
            ))
            .unwrap();
    }
    assert_eq!(history.state().snapshot().revision(), 4);
    assert_eq!(history.state().undo_depth(), 2);

    history.execute(ProjectHistoryCommand::undo(4)).unwrap();
    let second_undo = history.execute(ProjectHistoryCommand::undo(5)).unwrap();
    assert_eq!(second_undo.state().snapshot().revision(), 6);
    assert_eq!(
        second_undo.state().snapshot().media_path(MEDIA).unwrap(),
        portable("Media/a.webm")
    );
    let before_empty = history.state();
    assert_eq!(
        history
            .execute(ProjectHistoryCommand::undo(6))
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );
    assert_eq!(history.state(), before_empty);
    assert_eq!(
        history
            .execute(ProjectHistoryCommand::redo(5))
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );
    assert_eq!(history.state(), before_empty);

    let snapshot = document().snapshot();
    let exhausted_document = ProjectDocument::from_parts(
        u64::MAX,
        snapshot.editorial_project().clone(),
        snapshot.root_timeline_id(),
        snapshot.graphs().cloned().collect::<Vec<ProjectGraph>>(),
    )
    .unwrap();
    let mut exhausted = ProjectCommandHistory::new(exhausted_document);
    let before = exhausted.state();
    let error = exhausted
        .execute(ProjectHistoryCommand::apply(
            u64::MAX,
            ProjectMutation::media(ProjectMediaCommand::set_path(
                MEDIA,
                portable("Media/x.webm"),
            )),
        ))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(exhausted.state(), before);
}

#[test]
fn selected_historical_state_is_durable_and_reaches_the_real_resource_consumer() {
    let mut history = ProjectCommandHistory::new(document());
    let original = history.state();
    history
        .execute(ProjectHistoryCommand::apply(
            1,
            ProjectMutation::media(ProjectMediaCommand::mark_missing(MEDIA)),
        ))
        .unwrap();
    let restored = history.execute(ProjectHistoryCommand::undo(2)).unwrap();
    assert_eq!(restored.state().snapshot().revision(), 3);
    assert_eq!(
        restored.state().snapshot().media_path(MEDIA).unwrap(),
        original.snapshot().media_path(MEDIA).unwrap()
    );

    let mut database = ProjectDatabase::memory().unwrap();
    database.replace(restored.state().snapshot()).unwrap();
    assert_eq!(
        database.load().unwrap().snapshot(),
        restored.state().snapshot().clone()
    );

    let request = MediaResourceRequest::new(
        SourceRequest::new(MEDIA, SourceLocation::Path(fixture_path())),
        [DecoderResourceRequest::new(STREAM)],
    )
    .unwrap();
    let resources = acquire_project_resources(
        restored.state().snapshot(),
        &media_backend_registry().unwrap(),
        [request],
        ResourceAcquisitionPolicy::default(),
        &OperationContext::new(MediaPriority::Interactive),
    )
    .unwrap();
    assert_eq!(
        resources.compilation(),
        restored.state().snapshot().timeline_graph(ROOT).unwrap()
    );
    assert_eq!(
        compiled_clip_name(restored.state().snapshot()),
        "retained graph result"
    );
    assert_eq!(
        resources.media(MEDIA).unwrap().info().identity().media_id(),
        MEDIA
    );
    assert_eq!(
        resources
            .media(MEDIA)
            .unwrap()
            .info()
            .identity()
            .fingerprint(),
        FINGERPRINT
    );
}
