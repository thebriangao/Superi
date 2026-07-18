use superi_audio::graph::{AudioProcessBlock, AudioProcessor};
use superi_audio::mixing::{ChannelMap, ClipMixControls, ClipMixMutation};
use superi_core::error::ErrorCategory;
use superi_core::ids::{ClipId, GraphId, MediaId, ProjectId, TimelineId, TrackId};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::settings::{ComponentId, SemanticVersion, VersionIdentifier};
use superi_core::time::FrameRate;
use superi_core::time::{Duration, RationalTime, SampleTime, TimeRange, Timebase};
use superi_effects::ofx::OfxPluginIdentity;
use superi_engine::history::{
    ProjectCommandHistory, ProjectHistoryActionResult, ProjectHistoryCommand, ProjectMutation,
    ProjectMutationKind,
};
use superi_engine::project_transaction::{
    CompoundProjectAction, CompoundProjectActionResult, CompoundProjectTransaction,
    MAX_COMPOUND_PROJECT_ACTIONS,
};
use superi_graph::mutate::{GraphMutation, TypedParameterValue};
use superi_graph::value::GraphValue;
use superi_project::document::{ProjectDocument, ProjectSnapshot};
use superi_project::extensions::{
    ProjectExtensionCommand, ProjectExtensionKey, ProjectExtensionKind, ProjectExtensionLifecycle,
    ProjectExtensionRecord, ProjectExtensionRecordId,
};
use superi_project::media::ProjectMediaCommand;
use superi_project::ProjectDatabase;
use superi_timeline::compile::{TimelineGraphOrigin, TimelineGraphValue};
use superi_timeline::edit_ops::EditOperation;
use superi_timeline::media::RelinkStatus;
use superi_timeline::model::{
    Clip, ClipSource, EditorialObjectId, EditorialProject, LinkedMediaReference, Timeline, Track,
    TrackItem, TrackSemantics, VideoCompositing, VideoTrackSemantics,
};
use superi_timeline::track_ops::TrackMutation;

const PROJECT: ProjectId = ProjectId::from_raw(0xd000);
const MEDIA: MediaId = MediaId::from_raw(0xd001);
const ROOT: TimelineId = TimelineId::from_raw(0xd002);
const TRACK: TrackId = TrackId::from_raw(0xd003);
const CLIP: ClipId = ClipId::from_raw(0xd004);

fn range(start: i64, duration: u64, timebase: Timebase) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, timebase),
        Duration::new(duration, timebase).unwrap(),
    )
    .unwrap()
}

fn document() -> ProjectDocument {
    let source_rate = Timebase::integer(48).unwrap();
    let edit_rate = FrameRate::FPS_24.timebase();
    let media = LinkedMediaReference::new(
        MEDIA,
        "source",
        "urn:compound:source",
        Some(range(0, 480, source_rate)),
    );
    let clip = Clip::new(
        CLIP,
        "original name",
        ClipSource::Media(MEDIA),
        range(48, 96, source_rate),
        range(0, 48, edit_rate),
    )
    .unwrap();
    let timeline = Timeline::new(
        ROOT,
        "compound timeline",
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
    let editorial =
        EditorialProject::new(PROJECT, "compound project", [media], [timeline]).unwrap();
    ProjectDocument::new(editorial, ROOT).unwrap()
}

fn controls() -> ClipMixControls {
    let stereo = ChannelLayout::stereo();
    ClipMixControls::new(
        stereo.clone(),
        stereo,
        [
            ChannelMap::new(ChannelPosition::FrontLeft, ChannelPosition::FrontLeft, 1.0).unwrap(),
            ChannelMap::new(
                ChannelPosition::FrontRight,
                ChannelPosition::FrontRight,
                1.0,
            )
            .unwrap(),
        ],
    )
    .unwrap()
    .with_gain(0.5)
    .unwrap()
    .with_fades(4, 4)
    .unwrap()
    .with_phase_inverted([ChannelPosition::FrontRight])
    .unwrap()
}

fn graph_name(snapshot: &ProjectSnapshot) -> String {
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

fn extension_record(
    extension_id: &str,
    record_id: &str,
    kind: ProjectExtensionKind,
) -> ProjectExtensionRecord {
    ProjectExtensionRecord::new(
        ComponentId::new(extension_id).unwrap(),
        ProjectExtensionRecordId::new(record_id).unwrap(),
        SemanticVersion::new(1, 0, 0),
        kind,
        VersionIdentifier::new(
            ComponentId::new("example.compound-state").unwrap(),
            SemanticVersion::new(1, 0, 0),
        ),
        Default::default(),
        Default::default(),
        ProjectExtensionLifecycle::Enabled,
        None,
        format!("{extension_id}:{record_id}").into_bytes(),
    )
    .unwrap()
}

fn ofx_identity() -> OfxPluginIdentity {
    OfxPluginIdentity::new("com.example.superi-gain", SemanticVersion::new(1, 2, 0)).unwrap()
}

fn ofx_extension_record() -> ProjectExtensionRecord {
    let identity = ofx_identity();
    ProjectExtensionRecord::new(
        identity.identifier().clone(),
        ProjectExtensionRecordId::new("plugin-state").unwrap(),
        identity.version().clone(),
        ProjectExtensionKind::plugin(),
        VersionIdentifier::new(
            ComponentId::new("superi.ofx.project-state").unwrap(),
            SemanticVersion::new(1, 0, 0),
        ),
        Default::default(),
        Default::default(),
        ProjectExtensionLifecycle::Enabled,
        None,
        b"isolated-worker-state".to_vec(),
    )
    .unwrap()
}

fn compound(snapshot: &ProjectSnapshot) -> CompoundProjectTransaction {
    let compilation = snapshot.timeline_graph(ROOT).unwrap();
    let graph = compilation.snapshot();
    let graph_id = graph.graph_id();
    let node_id = compilation
        .index()
        .node(TimelineGraphOrigin::Object(EditorialObjectId::Clip(CLIP)))
        .unwrap();
    let name = graph
        .node(node_id)
        .unwrap()
        .parameters()
        .values()
        .find(|parameter| parameter.name().as_str() == "name")
        .unwrap();
    CompoundProjectTransaction::new([
        CompoundProjectAction::mutate_graph(
            graph_id,
            [GraphMutation::SetParameter {
                node_id,
                parameter_id: name.id(),
                value: TypedParameterValue::new(
                    name.value().value_type().clone(),
                    GraphValue::domain(TimelineGraphValue::Text("direct compound name".to_owned())),
                ),
            }],
        ),
        CompoundProjectAction::edit_timeline([EditOperation::slip(
            ROOT,
            TRACK,
            CLIP,
            RationalTime::new(96, Timebase::integer(48).unwrap()),
        )]),
        CompoundProjectAction::MutateMedia(ProjectMediaCommand::mark_missing(MEDIA)),
        CompoundProjectAction::mutate_clip_mix([ClipMixMutation::set(CLIP, controls())]),
        CompoundProjectAction::mutate_extension(ProjectExtensionCommand::upsert(
            ofx_extension_record(),
        )),
        CompoundProjectAction::mutate_extension(ProjectExtensionCommand::upsert(extension_record(
            "example.compound-effect",
            "effect-host-state",
            ProjectExtensionKind::effect(),
        ))),
        CompoundProjectAction::mutate_extension(ProjectExtensionCommand::upsert(extension_record(
            "example.compound-ai",
            "artifact-provenance",
            ProjectExtensionKind::ai_artifact(),
        ))),
        CompoundProjectAction::SelectRootTimeline(ROOT),
    ])
    .unwrap()
}

#[test]
fn one_history_command_publishes_and_restores_every_authored_subsystem() {
    let initial = document();
    let initial_snapshot = initial.snapshot();
    let transaction = compound(&initial_snapshot);
    let mut history = ProjectCommandHistory::new(initial);

    let applied = history
        .execute(ProjectHistoryCommand::apply(
            0,
            ProjectMutation::compound(transaction),
        ))
        .unwrap();
    assert!(applied.authored_state_changed());
    assert_eq!(applied.state().snapshot().revision(), 1);
    assert_eq!(applied.state().undo_depth(), 1);
    assert_eq!(
        applied.state().next_undo(),
        Some(ProjectMutationKind::Compound)
    );
    let ProjectHistoryActionResult::AppliedCompound { actions } = applied.action_result().unwrap()
    else {
        panic!("compound command must retain compound action evidence");
    };
    assert_eq!(actions.len(), 8);
    assert!(matches!(
        actions[0],
        CompoundProjectActionResult::GraphMutated { .. }
    ));
    assert!(matches!(
        actions[1],
        CompoundProjectActionResult::TimelineEdited(_)
    ));
    assert!(matches!(
        actions[2],
        CompoundProjectActionResult::MediaMutated(_)
    ));
    assert!(matches!(
        actions[3],
        CompoundProjectActionResult::ClipMixMutated(1)
    ));
    assert!(matches!(
        actions[4],
        CompoundProjectActionResult::ExtensionMutated(_)
    ));
    assert!(matches!(
        actions[5],
        CompoundProjectActionResult::ExtensionMutated(_)
    ));
    assert!(matches!(
        actions[6],
        CompoundProjectActionResult::ExtensionMutated(_)
    ));
    assert!(matches!(
        actions[7],
        CompoundProjectActionResult::RootTimelineSelected(ROOT)
    ));

    let published = applied.state().snapshot();
    assert_eq!(graph_name(published), "direct compound name");
    assert_eq!(published.extension_records().len(), 3);
    let ofx = ofx_identity();
    let ofx_key = ProjectExtensionKey::new(
        ofx.identifier().clone(),
        ProjectExtensionRecordId::new("plugin-state").unwrap(),
    );
    let stored_ofx = published.extension_record(&ofx_key).unwrap();
    assert_eq!(stored_ofx.extension_version(), ofx.version());
    assert_eq!(stored_ofx.payload(), b"isolated-worker-state");
    let clip = published
        .editorial_project()
        .timeline(ROOT)
        .unwrap()
        .track(TRACK)
        .unwrap()
        .item(EditorialObjectId::Clip(CLIP))
        .unwrap()
        .as_clip()
        .unwrap();
    assert_eq!(clip.source_range().start().value(), 96);
    assert_eq!(
        published
            .editorial_project()
            .media_reference(MEDIA)
            .unwrap()
            .relink_state()
            .status(),
        RelinkStatus::Missing
    );
    assert_eq!(published.clip_mix_state().controls(CLIP), Some(&controls()));

    let mut database = ProjectDatabase::memory().unwrap();
    database.replace(published).unwrap();
    let reopened = database.load().unwrap();
    assert_eq!(reopened.snapshot(), *published);
    assert_audio_blocks_are_continuous(reopened.snapshot().clip_mix_state());

    let undone = history.execute(ProjectHistoryCommand::undo(1)).unwrap();
    assert_eq!(undone.state().snapshot().revision(), 2);
    assert_eq!(undone.state().undo_depth(), 0);
    assert_eq!(undone.state().redo_depth(), 1);
    assert_eq!(graph_name(undone.state().snapshot()), "original name");
    assert!(undone
        .state()
        .snapshot()
        .clip_mix_state()
        .controls(CLIP)
        .is_none());
    assert!(undone.state().snapshot().extension_records().is_empty());

    let redone = history.execute(ProjectHistoryCommand::redo(2)).unwrap();
    assert_eq!(redone.state().snapshot().revision(), 3);
    assert_eq!(
        graph_name(redone.state().snapshot()),
        "direct compound name"
    );
    assert_eq!(
        redone.state().snapshot().clip_mix_state().controls(CLIP),
        Some(&controls())
    );
    assert_eq!(redone.state().snapshot().extension_records().len(), 3);
}

#[test]
fn track_mutations_publish_once_recompile_and_round_trip_through_history() {
    let mut history = ProjectCommandHistory::new(document());
    let transaction = CompoundProjectTransaction::new([CompoundProjectAction::mutate_tracks([
        TrackMutation::Rename {
            timeline_id: ROOT,
            track_id: TRACK,
            name: "Picture".to_owned(),
        },
        TrackMutation::SetHeight {
            timeline_id: ROOT,
            track_id: TRACK,
            height: 104,
        },
        TrackMutation::SetEnabled {
            timeline_id: ROOT,
            track_id: TRACK,
            enabled: false,
        },
    ])])
    .unwrap();

    let applied = history
        .execute(ProjectHistoryCommand::apply(
            0,
            ProjectMutation::compound(transaction),
        ))
        .unwrap();
    let ProjectHistoryActionResult::AppliedCompound { actions } = applied.action_result().unwrap()
    else {
        panic!("track command must retain compound action evidence");
    };
    assert!(matches!(
        actions.as_slice(),
        [CompoundProjectActionResult::TracksMutated(result)] if result.outcomes().len() == 3
    ));
    let published = applied.state().snapshot();
    let timeline = published.editorial_project().timeline(ROOT).unwrap();
    assert_eq!(timeline.track(TRACK).unwrap().name(), "Picture");
    let state = timeline.edit_state().track_state(TRACK).unwrap();
    assert_eq!(state.height(), 104);
    assert!(!state.enabled());
    let undone = history.execute(ProjectHistoryCommand::undo(1)).unwrap();
    assert_eq!(
        undone
            .state()
            .snapshot()
            .editorial_project()
            .timeline(ROOT)
            .unwrap()
            .track(TRACK)
            .unwrap()
            .name(),
        "V1"
    );
    let redone = history.execute(ProjectHistoryCommand::redo(2)).unwrap();
    assert_eq!(
        redone
            .state()
            .snapshot()
            .editorial_project()
            .timeline(ROOT)
            .unwrap()
            .track(TRACK)
            .unwrap()
            .name(),
        "Picture"
    );
}

#[test]
fn deleting_a_populated_track_releases_clip_mix_intent_and_restores_it_with_history() {
    let mut history = ProjectCommandHistory::new(document());
    let mix = CompoundProjectTransaction::new([CompoundProjectAction::mutate_clip_mix([
        ClipMixMutation::set(CLIP, controls()),
    ])])
    .unwrap();
    history
        .execute(ProjectHistoryCommand::apply(
            0,
            ProjectMutation::compound(mix),
        ))
        .unwrap();

    let delete = CompoundProjectTransaction::new([CompoundProjectAction::mutate_tracks([
        TrackMutation::Delete {
            timeline_id: ROOT,
            track_id: TRACK,
        },
    ])])
    .unwrap();
    let deleted = history
        .execute(ProjectHistoryCommand::apply(
            1,
            ProjectMutation::compound(delete),
        ))
        .unwrap();
    let deleted_snapshot = deleted.state().snapshot();
    assert!(deleted_snapshot
        .editorial_project()
        .timeline(ROOT)
        .unwrap()
        .track(TRACK)
        .is_none());
    assert!(deleted_snapshot.clip_mix_state().controls(CLIP).is_none());

    let restored = history.execute(ProjectHistoryCommand::undo(2)).unwrap();
    let restored_snapshot = restored.state().snapshot();
    assert!(restored_snapshot
        .editorial_project()
        .timeline(ROOT)
        .unwrap()
        .track(TRACK)
        .is_some());
    assert_eq!(
        restored_snapshot.clip_mix_state().controls(CLIP),
        Some(&controls())
    );

    let redone = history.execute(ProjectHistoryCommand::redo(3)).unwrap();
    assert!(redone
        .state()
        .snapshot()
        .clip_mix_state()
        .controls(CLIP)
        .is_none());
}

#[test]
fn late_failure_rolls_back_state_history_and_prior_action_results() {
    let mut history = ProjectCommandHistory::new(document());
    let before = history.state();
    let transaction = CompoundProjectTransaction::new([
        CompoundProjectAction::mutate_clip_mix([ClipMixMutation::set(CLIP, controls())]),
        CompoundProjectAction::mutate_graph(
            GraphId::from_raw(0xffff),
            [GraphMutation::Remove {
                node_id: superi_graph::ids::NodeId::from_raw(0xffff),
            }],
        ),
    ])
    .unwrap();

    let error = history
        .execute(ProjectHistoryCommand::apply(
            0,
            ProjectMutation::compound(transaction),
        ))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotFound);
    assert_eq!(history.state(), before);
}

#[test]
fn compound_bounds_and_empty_subsystem_batches_fail_before_publication() {
    assert_eq!(MAX_COMPOUND_PROJECT_ACTIONS, 64);
    assert_eq!(
        CompoundProjectTransaction::new([]).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        CompoundProjectTransaction::new([CompoundProjectAction::edit_timeline([])])
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        CompoundProjectTransaction::new(std::iter::repeat_n(
            CompoundProjectAction::SelectRootTimeline(ROOT),
            65
        ))
        .unwrap_err()
        .category(),
        ErrorCategory::ResourceExhausted
    );
}

fn assert_audio_blocks_are_continuous(state: &superi_audio::mixing::ClipMixState) {
    let stereo = ChannelLayout::stereo();
    let mut split = state
        .snapshot()
        .prepare_processor(CLIP, SampleTime::new(100, 48_000).unwrap(), 4)
        .unwrap();
    let mut split_output = Vec::new();
    for start in [100, 102] {
        let input = [1.0; 4];
        let mut output = [0.0; 4];
        split
            .process(AudioProcessBlock {
                start_time: SampleTime::new(start, 48_000).unwrap(),
                frame_count: 2,
                input: Some(&input),
                input_layout: Some(&stereo),
                output: &mut output,
                output_layout: &stereo,
            })
            .unwrap();
        split_output.extend(output);
    }

    let mut whole = state
        .snapshot()
        .prepare_processor(CLIP, SampleTime::new(100, 48_000).unwrap(), 4)
        .unwrap();
    let input = [1.0; 8];
    let mut whole_output = [0.0; 8];
    whole
        .process(AudioProcessBlock {
            start_time: SampleTime::new(100, 48_000).unwrap(),
            frame_count: 4,
            input: Some(&input),
            input_layout: Some(&stereo),
            output: &mut whole_output,
            output_layout: &stereo,
        })
        .unwrap();
    assert_eq!(split_output, whole_output);
}
