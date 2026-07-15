use std::path::Path;

use superi_core::error::ErrorCategory;
use superi_core::ids::{ClipId, MediaId, ProjectId, TimelineId, TrackId};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_graph::mutate::{GraphMutation, GraphTransaction, TypedParameterValue};
use superi_graph::value::GraphValue;
use superi_project::document::ProjectDocument;
use superi_project::media::{
    MediaPathPlatform, PortableRelativePath, ProjectMediaCommand, ProjectMediaCommandResult,
    ReferencedMediaPath, MEDIA_PATH_TARGET_FORMAT,
};
use superi_project::ProjectDatabase;
use superi_timeline::compile::{TimelineGraphOrigin, TimelineGraphValue};
use superi_timeline::media::{RelinkDecision, RelinkStatus};
use superi_timeline::model::{
    Clip, ClipSource, EditorialObjectId, EditorialProject, LinkedMediaReference, Timeline, Track,
    TrackItem, TrackSemantics, VideoCompositing, VideoTrackSemantics,
};

const PROJECT: ProjectId = ProjectId::from_raw(1);
const MEDIA: MediaId = MediaId::from_raw(2);
const ROOT: TimelineId = TimelineId::from_raw(3);
const TRACK: TrackId = TrackId::from_raw(4);
const CLIP: ClipId = ClipId::from_raw(5);
const EXPECTED_FINGERPRINT: &str = "sha256:expected-media";

fn range(start: i64, duration: u64, timebase: Timebase) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, timebase),
        Duration::new(duration, timebase).unwrap(),
    )
    .unwrap()
}

fn document(path: &ReferencedMediaPath) -> ProjectDocument {
    let source_rate = Timebase::integer(48).unwrap();
    let edit_rate = FrameRate::FPS_24.timebase();
    let media = LinkedMediaReference::with_fingerprint(
        MEDIA,
        "camera original",
        path.to_target(),
        Some(range(0, 480, source_rate)),
        EXPECTED_FINGERPRINT,
    )
    .unwrap();
    let clip = Clip::new(
        CLIP,
        "camera edit",
        ClipSource::Media(MEDIA),
        range(0, 96, source_rate),
        range(0, 48, edit_rate),
    )
    .unwrap();
    let timeline = Timeline::new(
        ROOT,
        "main",
        edit_rate,
        RationalTime::zero(edit_rate),
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
    let editorial = EditorialProject::new(PROJECT, "media project", [media], [timeline]).unwrap();
    ProjectDocument::new(editorial, ROOT).unwrap()
}

fn clip_name_address(
    document: &ProjectDocument,
) -> (
    superi_graph::ids::NodeId,
    superi_graph::ids::ParameterId,
    superi_graph::node::ValueTypeId,
) {
    let compilation = document.timeline_graph(ROOT).unwrap();
    let node_id = compilation
        .index()
        .node(TimelineGraphOrigin::Object(EditorialObjectId::Clip(CLIP)))
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

fn compiled_clip_name(document: &ProjectDocument) -> String {
    let compilation = document.timeline_graph(ROOT).unwrap();
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
fn portable_relative_paths_have_one_versioned_cross_platform_representation() {
    assert_eq!(MEDIA_PATH_TARGET_FORMAT, "superi.media-path.v1");

    let portable = PortableRelativePath::new("./Media//day-01/../clip.mov").unwrap();
    assert_eq!(portable.as_str(), "Media/clip.mov");
    let reference = ReferencedMediaPath::project_relative(portable.clone());
    assert_eq!(
        reference.to_target(),
        "superi.media-path.v1:relative:Media/clip.mov"
    );
    assert_eq!(
        ReferencedMediaPath::from_target(&reference.to_target()).unwrap(),
        Some(reference.clone())
    );
    assert_eq!(
        ReferencedMediaPath::from_target("../../../Media/clip.mov").unwrap(),
        Some(ReferencedMediaPath::project_relative(
            PortableRelativePath::new("../../../Media/clip.mov").unwrap()
        ))
    );
    assert_eq!(
        ReferencedMediaPath::from_target("urn:camera:original").unwrap(),
        None
    );

    for invalid in [
        "",
        "/absolute.mov",
        "C:/absolute.mov",
        "Media\\clip.mov",
        "Media/clip?.mov",
        "Media/clip.mov ",
        "Media/clip.",
        "Media/CON.mov",
        "Media/LPT9",
        "Media/control\u{1f}.mov",
    ] {
        assert!(PortableRelativePath::new(invalid).is_err(), "{invalid:?}");
    }
}

#[test]
fn resolution_is_lexical_project_relative_and_foreign_absolute_paths_stay_visible() {
    let project_file = std::env::temp_dir()
        .join("superi-media-reference-contract")
        .join("edit.superi");
    assert!(project_file.is_absolute());

    let relative = ReferencedMediaPath::project_relative(
        PortableRelativePath::new("../shared/clip.mov").unwrap(),
    );
    assert_eq!(
        relative.resolve(&project_file).unwrap(),
        project_file.parent().unwrap().join("../shared/clip.mov")
    );
    assert_eq!(
        relative
            .resolve(Path::new("relative/edit.superi"))
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );

    let absolute = ReferencedMediaPath::host_absolute(&project_file).unwrap();
    assert_eq!(absolute.platform(), Some(MediaPathPlatform::current()));
    assert_eq!(
        absolute.resolve(Path::new("unused.superi")).unwrap(),
        project_file
    );
    assert_eq!(
        ReferencedMediaPath::from_target(&absolute.to_target()).unwrap(),
        Some(absolute)
    );

    let foreign_target = if cfg!(windows) {
        "superi.media-path.v1:absolute:unix:/var/media/clip.mov"
    } else {
        "superi.media-path.v1:absolute:windows:C:\\Media\\clip.mov"
    };
    let foreign = ReferencedMediaPath::from_target(foreign_target)
        .unwrap()
        .unwrap();
    assert_ne!(foreign.platform(), Some(MediaPathPlatform::current()));
    assert_eq!(
        foreign.resolve(&project_file).unwrap_err().category(),
        ErrorCategory::Unsupported
    );

    assert_eq!(
        ReferencedMediaPath::from_target(&project_file.to_string_lossy()).unwrap(),
        Some(ReferencedMediaPath::host_absolute(&project_file).unwrap())
    );
}

#[test]
fn media_commands_preserve_identity_graph_edits_and_atomic_revision_fences() {
    let original = ReferencedMediaPath::project_relative(
        PortableRelativePath::new("Media/original.mov").unwrap(),
    );
    let mut document = document(&original);
    let (node_id, parameter_id, value_type) = clip_name_address(&document);
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
                                "direct graph result".to_owned(),
                            )),
                        ),
                    }],
                ))?;
            Ok(())
        })
        .unwrap();
    assert_eq!(document.revision(), 1);
    assert_eq!(document.timeline_graph(ROOT).unwrap().graph().revision(), 2);

    let replacement = ReferencedMediaPath::project_relative(
        PortableRelativePath::new("Media/relinked.mov").unwrap(),
    );
    let outcome = document
        .execute_media_command(1, ProjectMediaCommand::set_path(MEDIA, replacement.clone()))
        .unwrap();
    assert_eq!(outcome.result(), ProjectMediaCommandResult::PathUpdated);
    assert_eq!(outcome.snapshot().revision(), 2);
    assert_eq!(document.revision(), 2);
    assert_eq!(document.editorial_project().revision(), 1);
    assert_eq!(document.media_path(MEDIA).unwrap(), replacement);

    let media = document.editorial_project().media_reference(MEDIA).unwrap();
    assert_eq!(media.id(), MEDIA);
    assert_eq!(media.relink_state().status(), RelinkStatus::Unverified);
    assert_eq!(compiled_clip_name(&document), "direct graph result");
    assert_eq!(document.timeline_graph(ROOT).unwrap().graph().revision(), 2);
    assert_eq!(document.timeline_graph(ROOT).unwrap().project_revision(), 1);

    let before_stale = document.snapshot();
    let stale = document
        .execute_media_command(1, ProjectMediaCommand::mark_missing(MEDIA))
        .unwrap_err();
    assert_eq!(stale.category(), ErrorCategory::Conflict);
    assert_eq!(document.snapshot(), before_stale);

    let missing = document
        .execute_media_command(2, ProjectMediaCommand::mark_missing(MEDIA))
        .unwrap();
    assert_eq!(missing.result(), ProjectMediaCommandResult::MarkedMissing);
    assert_eq!(
        document
            .editorial_project()
            .media_reference(MEDIA)
            .unwrap()
            .relink_state()
            .status(),
        RelinkStatus::Missing
    );
    assert_eq!(compiled_clip_name(&document), "direct graph result");

    let missing_id = document
        .execute_media_command(3, ProjectMediaCommand::mark_missing(MediaId::from_raw(999)))
        .unwrap_err();
    assert_eq!(missing_id.category(), ErrorCategory::NotFound);
    assert_eq!(document.revision(), 3);
}

#[test]
fn relink_conflicts_and_paths_round_trip_through_the_project_database() {
    let original = ReferencedMediaPath::project_relative(
        PortableRelativePath::new("Media/original.mov").unwrap(),
    );
    let candidate = ReferencedMediaPath::project_relative(
        PortableRelativePath::new("Recovered/candidate.mov").unwrap(),
    );
    let mut document = document(&original);

    let rejected = document
        .execute_media_command(
            0,
            ProjectMediaCommand::consider_relink(
                MEDIA,
                candidate.clone(),
                "sha256:different-media",
            ),
        )
        .unwrap();
    assert_eq!(
        rejected.result(),
        ProjectMediaCommandResult::Relink(RelinkDecision::RejectedFingerprintMismatch)
    );
    let media = rejected
        .snapshot()
        .editorial_project()
        .media_reference(MEDIA)
        .unwrap();
    assert_eq!(media.id(), MEDIA);
    assert_eq!(media.target(), original.to_target());
    assert_eq!(
        media.relink_state().status(),
        RelinkStatus::FingerprintMismatch
    );
    assert_eq!(
        media.relink_state().expected_fingerprint(),
        Some(EXPECTED_FINGERPRINT)
    );
    assert_eq!(
        media.relink_state().observed_fingerprint(),
        Some("sha256:different-media")
    );
    assert_eq!(
        media.relink_state().rejected_target(),
        Some(candidate.to_target().as_str())
    );

    let mut database = ProjectDatabase::memory().unwrap();
    database.replace(rejected.snapshot()).unwrap();
    let loaded = database.load().unwrap();
    let loaded_media = loaded.editorial_project().media_reference(MEDIA).unwrap();
    assert_eq!(loaded_media, media);
    assert_eq!(loaded.media_path(MEDIA).unwrap(), original);
    let rejected_path =
        ReferencedMediaPath::from_target(loaded_media.relink_state().rejected_target().unwrap())
            .unwrap()
            .unwrap();
    assert_eq!(rejected_path, candidate);

    let accepted = document
        .execute_media_command(
            1,
            ProjectMediaCommand::consider_relink(MEDIA, candidate.clone(), EXPECTED_FINGERPRINT),
        )
        .unwrap();
    assert_eq!(
        accepted.result(),
        ProjectMediaCommandResult::Relink(RelinkDecision::Accepted)
    );
    assert_eq!(accepted.snapshot().media_path(MEDIA).unwrap(), candidate);
    let accepted_media = accepted
        .snapshot()
        .editorial_project()
        .media_reference(MEDIA)
        .unwrap();
    assert_eq!(accepted_media.id(), MEDIA);
    assert_eq!(accepted_media.relink_state().status(), RelinkStatus::Online);
    assert_eq!(accepted_media.relink_state().rejected_target(), None);
}

#[test]
fn unknown_or_future_target_syntax_is_preserved_but_not_misresolved() {
    assert_eq!(
        ReferencedMediaPath::from_target("https://media.example/clip.mov").unwrap(),
        None
    );
    assert_eq!(
        ReferencedMediaPath::from_target("superi.media-path.v2:relative:Media/clip.mov")
            .unwrap_err()
            .category(),
        ErrorCategory::Unsupported
    );

    let opaque = LinkedMediaReference::new(MEDIA, "remote", "urn:remote:clip", None);
    let edit_rate = FrameRate::FPS_24.timebase();
    let clip = Clip::new(
        CLIP,
        "remote",
        ClipSource::Media(MEDIA),
        range(0, 24, edit_rate),
        range(0, 24, edit_rate),
    )
    .unwrap();
    let timeline = Timeline::new(
        ROOT,
        "opaque",
        edit_rate,
        RationalTime::zero(edit_rate),
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
    let project = EditorialProject::new(PROJECT, "opaque", [opaque], [timeline]).unwrap();
    let document = ProjectDocument::new(project, ROOT).unwrap();
    assert_eq!(
        document.media_path(MEDIA).unwrap_err().category(),
        ErrorCategory::Unsupported
    );
    assert_eq!(
        document
            .editorial_project()
            .media_reference(MEDIA)
            .unwrap()
            .target(),
        "urn:remote:clip"
    );
}
