use std::collections::BTreeMap;

use superi_core::error::ErrorCategory;
use superi_core::ids::{
    CaptionId, ClipId, GapId, GeneratorId, MediaId, MulticamAngleId, ProjectId, TimelineId,
    TrackId, TransitionId,
};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_graph::mutate::{GraphMutation, GraphTransaction, TypedParameterValue};
use superi_timeline::compile::{compile_timeline, TimelineGraphOrigin, TimelineGraphValue};
use superi_timeline::edit_state::{SelectionExpansion, SelectionUpdate};
use superi_timeline::model::{
    Caption, CaptionPurpose, CaptionTrackSemantics, Clip, ClipSource, EditorialObjectId,
    EditorialProject, Gap, Generator, LanguageTag, LinkedMediaReference, Timeline, Track,
    TrackItem, TrackSemantics, Transition, VideoCompositing, VideoTrackSemantics,
};
use superi_timeline::multicam::{
    MulticamAngle, MulticamAudioPolicy, MulticamClip, MulticamSource, MulticamSyncMethod,
};

const MEDIA: MediaId = MediaId::from_raw(1);
const CHILD: TimelineId = TimelineId::from_raw(10);
const ROOT: TimelineId = TimelineId::from_raw(11);
const VIDEO_TRACK: TrackId = TrackId::from_raw(20);
const CAPTION_TRACK: TrackId = TrackId::from_raw(21);
const ROOT_TRACK: TrackId = TrackId::from_raw(22);
const CLIP: ClipId = ClipId::from_raw(30);
const NESTED_CLIP: ClipId = ClipId::from_raw(31);
const SECOND_NESTED_CLIP: ClipId = ClipId::from_raw(32);
const GAP: GapId = GapId::from_raw(40);
const TRANSITION: TransitionId = TransitionId::from_raw(50);
const GENERATOR: GeneratorId = GeneratorId::from_raw(60);
const CAPTION: CaptionId = CaptionId::from_raw(70);

fn range(start: i64, duration: u64, timebase: Timebase) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, timebase),
        Duration::new(duration, timebase).unwrap(),
    )
    .unwrap()
}

fn video_semantics() -> TrackSemantics {
    TrackSemantics::Video(VideoTrackSemantics::new(
        FrameRate::FPS_24,
        VideoCompositing::Over,
    ))
}

fn caption_semantics() -> TrackSemantics {
    TrackSemantics::Caption(CaptionTrackSemantics::new(
        Timebase::MILLISECONDS,
        LanguageTag::new("en-US").unwrap(),
        CaptionPurpose::Captions,
    ))
}

fn project_fixture() -> EditorialProject {
    let edit_rate = Timebase::integer(24).unwrap();
    let source_rate = Timebase::integer(48).unwrap();
    let media = LinkedMediaReference::new(
        MEDIA,
        "camera a",
        "urn:superi:test:camera-a",
        Some(range(0, 480, source_rate)),
    );
    let clip = Clip::new(
        CLIP,
        "shot a",
        ClipSource::Media(MEDIA),
        range(48, 96, source_rate),
        range(0, 48, edit_rate),
    )
    .unwrap();
    let generator = Generator::new(
        GENERATOR,
        "solid black",
        "solid_color",
        BTreeMap::from([("rgba".to_owned(), "0,0,0,1".to_owned())]),
        range(48, 24, edit_rate),
    );
    let transition = Transition::new(
        TRANSITION,
        "dip to black",
        EditorialObjectId::Clip(CLIP),
        EditorialObjectId::Generator(GENERATOR),
        Duration::new(6, edit_rate).unwrap(),
        Duration::new(6, edit_rate).unwrap(),
    );
    let child = Timeline::new(
        CHILD,
        "child",
        edit_rate,
        RationalTime::zero(edit_rate),
        vec![
            Track::new(
                VIDEO_TRACK,
                "V1",
                video_semantics(),
                vec![
                    TrackItem::Clip(clip),
                    TrackItem::Transition(transition),
                    TrackItem::Generator(generator),
                    TrackItem::Gap(Gap::new(GAP, "tail gap", range(72, 12, edit_rate))),
                ],
            ),
            Track::new(
                CAPTION_TRACK,
                "C1",
                caption_semantics(),
                vec![TrackItem::Caption(Caption::new(
                    CAPTION,
                    "opening caption",
                    "Hello, Superi",
                    Some("en-US".to_owned()),
                    range(0, 3_500, Timebase::MILLISECONDS),
                ))],
            ),
        ],
    );
    let nested = Clip::new(
        NESTED_CLIP,
        "nested child",
        ClipSource::Timeline(CHILD),
        range(0, 84, edit_rate),
        range(0, 84, edit_rate),
    )
    .unwrap();
    let second_nested = Clip::new(
        SECOND_NESTED_CLIP,
        "second nested child",
        ClipSource::Timeline(CHILD),
        range(0, 84, edit_rate),
        range(84, 84, edit_rate),
    )
    .unwrap();
    let root = Timeline::new(
        ROOT,
        "root",
        edit_rate,
        RationalTime::zero(edit_rate),
        vec![Track::new(
            ROOT_TRACK,
            "V1",
            video_semantics(),
            vec![TrackItem::Clip(nested), TrackItem::Clip(second_nested)],
        )],
    );

    EditorialProject::new(
        ProjectId::from_raw(100),
        "editorial project",
        [media],
        [child, root],
    )
    .unwrap()
}

fn parameter_value(
    compilation: &superi_timeline::compile::TimelineGraphCompilation,
    origin: TimelineGraphOrigin,
    name: &str,
) -> TimelineGraphValue {
    let node_id = compilation.index().node(origin).unwrap();
    compilation
        .snapshot()
        .node(node_id)
        .unwrap()
        .parameters()
        .values()
        .find(|parameter| parameter.name().as_str() == name)
        .unwrap()
        .value()
        .payload()
        .clone()
}

#[test]
fn compilation_is_typed_deterministic_inspectable_and_nested() {
    let project = project_fixture();
    let first = compile_timeline(&project, ROOT).unwrap();
    let second = compile_timeline(&project, ROOT).unwrap();

    assert_eq!(first.project_id(), project.id());
    assert_eq!(first.root_timeline_id(), ROOT);
    assert_eq!(first.project_revision(), 0);
    assert_eq!(first.index(), second.index());
    assert_eq!(first.snapshot(), second.snapshot());
    assert_eq!(first.snapshot().revision(), 1);
    assert_eq!(first.snapshot().dag().node_count(), 12);
    assert_eq!(first.snapshot().dag().edge_count(), 14);

    let child_output = first.index().timeline_output(CHILD).unwrap();
    let nested_node = first
        .index()
        .object_node(EditorialObjectId::Clip(NESTED_CLIP))
        .unwrap();
    let nested_sources: Vec<_> = first
        .snapshot()
        .dag()
        .incoming_edge_ids(nested_node)
        .unwrap()
        .iter()
        .map(|edge_id| {
            first
                .snapshot()
                .dag()
                .edge(*edge_id)
                .unwrap()
                .source()
                .node_id()
        })
        .collect();
    assert_eq!(nested_sources, vec![child_output]);
    let second_nested_node = first
        .index()
        .object_node(EditorialObjectId::Clip(SECOND_NESTED_CLIP))
        .unwrap();
    let second_nested_sources: Vec<_> = first
        .snapshot()
        .dag()
        .incoming_edge_ids(second_nested_node)
        .unwrap()
        .iter()
        .map(|edge_id| {
            first
                .snapshot()
                .dag()
                .edge(*edge_id)
                .unwrap()
                .source()
                .node_id()
        })
        .collect();
    assert_eq!(second_nested_sources, vec![child_output]);

    let transition_node = first
        .index()
        .object_node(EditorialObjectId::Transition(TRANSITION))
        .unwrap();
    let transition_sources: Vec<_> = first
        .snapshot()
        .dag()
        .incoming_edge_ids(transition_node)
        .unwrap()
        .iter()
        .map(|edge_id| {
            first
                .snapshot()
                .dag()
                .edge(*edge_id)
                .unwrap()
                .source()
                .node_id()
        })
        .collect();
    assert_eq!(transition_sources.len(), 2);
    assert!(transition_sources.contains(
        &first
            .index()
            .object_node(EditorialObjectId::Clip(CLIP))
            .unwrap()
    ));
    assert!(transition_sources.contains(
        &first
            .index()
            .object_node(EditorialObjectId::Generator(GENERATOR))
            .unwrap()
    ));

    assert_eq!(
        parameter_value(&first, TimelineGraphOrigin::Timeline(CHILD), "track-order",),
        TimelineGraphValue::TrackOrder(vec![VIDEO_TRACK, CAPTION_TRACK])
    );
    assert_eq!(
        parameter_value(
            &first,
            TimelineGraphOrigin::Object(EditorialObjectId::Caption(CAPTION)),
            "caption-text",
        ),
        TimelineGraphValue::Text("Hello, Superi".to_owned())
    );
    assert_eq!(
        first.index().origin(nested_node),
        Some(TimelineGraphOrigin::Object(EditorialObjectId::Clip(
            NESTED_CLIP
        )))
    );
}

#[test]
fn semantic_edits_change_payloads_without_changing_stable_graph_addresses() {
    let mut project = project_fixture();
    let before = compile_timeline(&project, ROOT).unwrap();
    project
        .edit(0, |draft| {
            draft
                .timeline_mut(CHILD)?
                .track_mut(VIDEO_TRACK)?
                .item_mut(EditorialObjectId::Clip(CLIP))?
                .as_clip_mut()
                .unwrap()
                .set_source_range(range(96, 96, Timebase::integer(48).unwrap()))
        })
        .unwrap();
    let after = compile_timeline(&project, ROOT).unwrap();

    assert_eq!(after.project_revision(), 1);
    assert_eq!(before.index(), after.index());
    assert_eq!(before.snapshot().graph_id(), after.snapshot().graph_id());
    assert_ne!(
        parameter_value(
            &before,
            TimelineGraphOrigin::Object(EditorialObjectId::Clip(CLIP)),
            "source-range",
        ),
        parameter_value(
            &after,
            TimelineGraphOrigin::Object(EditorialObjectId::Clip(CLIP)),
            "source-range",
        )
    );
}

#[test]
fn nonprocessing_authoring_state_does_not_change_compiled_graph_state() {
    let mut project = project_fixture();
    let before = compile_timeline(&project, ROOT).unwrap();
    project
        .edit(0, |draft| {
            draft.timeline_mut(ROOT)?.update_selection(
                [EditorialObjectId::Clip(NESTED_CLIP)],
                SelectionUpdate::Replace,
                SelectionExpansion::Direct,
            )
        })
        .unwrap();
    let after = compile_timeline(&project, ROOT).unwrap();

    assert_eq!(after.project_revision(), 1);
    assert_eq!(before.index(), after.index());
    assert_eq!(before.snapshot(), after.snapshot());
}

#[test]
fn multicam_source_switching_and_audio_intent_remain_typed_graph_state() {
    let mut project = project_fixture();
    let angle_a = MulticamAngleId::from_raw(80);
    let angle_b = MulticamAngleId::from_raw(81);
    let source = MulticamSource::new(
        MulticamSyncMethod::Timecode,
        [
            MulticamAngle::new(angle_a, "wide", "A", []).unwrap(),
            MulticamAngle::new(angle_b, "close", "B", []).unwrap(),
        ],
    )
    .unwrap();
    let mut switch = MulticamClip::new(
        NESTED_CLIP,
        range(0, 84, Timebase::integer(24).unwrap()),
        angle_a,
        MulticamAudioPolicy::Fixed(angle_a),
    )
    .unwrap();
    switch
        .switch_range(range(42, 42, Timebase::integer(24).unwrap()), angle_b)
        .unwrap();
    project
        .edit(0, |draft| {
            draft
                .timeline_mut(CHILD)?
                .set_multicam_source(source.clone())?;
            draft
                .timeline_mut(ROOT)?
                .upsert_multicam_clip(switch.clone())?;
            Ok(())
        })
        .unwrap();

    let compilation = compile_timeline(&project, ROOT).unwrap();
    assert_eq!(
        parameter_value(
            &compilation,
            TimelineGraphOrigin::Timeline(CHILD),
            "multicam-source",
        ),
        TimelineGraphValue::OptionalMulticamSource(Some(source))
    );
    assert_eq!(
        parameter_value(
            &compilation,
            TimelineGraphOrigin::Object(EditorialObjectId::Clip(NESTED_CLIP)),
            "multicam-clip",
        ),
        TimelineGraphValue::OptionalMulticamClip(Some(switch))
    );
}

#[test]
fn compiled_parameters_remain_directly_editable_graph_state() {
    let project = project_fixture();
    let mut compilation = compile_timeline(&project, ROOT).unwrap();
    let clip_node = compilation
        .index()
        .object_node(EditorialObjectId::Clip(CLIP))
        .unwrap();
    let snapshot = compilation.snapshot();
    let name_parameter = snapshot
        .node(clip_node)
        .unwrap()
        .parameters()
        .values()
        .find(|parameter| parameter.name().as_str() == "name")
        .unwrap();
    let parameter_id = name_parameter.id();
    let value_type = name_parameter.value().value_type().clone();

    let edited = compilation
        .graph_mut()
        .apply(GraphTransaction::with_mutations(
            1,
            [GraphMutation::SetParameter {
                node_id: clip_node,
                parameter_id,
                value: TypedParameterValue::new(
                    value_type,
                    TimelineGraphValue::Text("direct graph edit".to_owned()),
                ),
            }],
        ))
        .unwrap();

    assert_eq!(edited.revision(), 2);
    assert_eq!(
        edited
            .node(clip_node)
            .unwrap()
            .parameter(parameter_id)
            .unwrap()
            .value()
            .payload(),
        &TimelineGraphValue::Text("direct graph edit".to_owned())
    );
}

#[test]
fn missing_root_timeline_fails_without_partial_graph_state() {
    let error = compile_timeline(&project_fixture(), TimelineId::from_raw(999)).unwrap_err();

    assert_eq!(error.category(), ErrorCategory::NotFound);
}
