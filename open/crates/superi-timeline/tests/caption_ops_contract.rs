use superi_core::error::ErrorCategory;
use superi_core::ids::{CaptionId, GapId, ProjectId, TimelineId, TrackId};
use superi_core::time::{Duration, RationalTime, TimeRange, Timebase};
use superi_timeline::caption_ops::{
    apply_caption_mutation_batch, CaptionAlignment, CaptionAttributes, CaptionMutation,
    CaptionPosition, CaptionStyle, CaptionTimelineRelationship,
};
use superi_timeline::edit_ops::{apply_edit_batch, EditOperation};
use superi_timeline::model::{
    Caption, CaptionPurpose, CaptionTrackSemantics, EditorialObjectId, EditorialProject, Gap,
    LanguageTag, Timeline, Track, TrackItem, TrackSemantics,
};
use superi_timeline::serialize::{deserialize_timeline_state, serialize_timeline_state};

const PROJECT: ProjectId = ProjectId::from_raw(1);
const TIMELINE: TimelineId = TimelineId::from_raw(2);
const TRACK: TrackId = TrackId::from_raw(3);
const CAPTION: CaptionId = CaptionId::from_raw(4);
const MISSING_CAPTION: CaptionId = CaptionId::from_raw(5);
const GAP: GapId = GapId::from_raw(6);
const CAPTION_FRAGMENT: CaptionId = CaptionId::from_raw(7);

fn caption_range(start: i64, duration: u64) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, Timebase::MILLISECONDS),
        Duration::new(duration, Timebase::MILLISECONDS).unwrap(),
    )
    .unwrap()
}

fn project() -> EditorialProject {
    EditorialProject::new(
        PROJECT,
        "caption contract",
        [],
        [Timeline::new(
            TIMELINE,
            "main",
            Timebase::MILLISECONDS,
            RationalTime::zero(Timebase::MILLISECONDS),
            vec![Track::new(
                TRACK,
                "English captions",
                TrackSemantics::Caption(CaptionTrackSemantics::new(
                    Timebase::MILLISECONDS,
                    LanguageTag::new("en-US").unwrap(),
                    CaptionPurpose::Captions,
                )),
                vec![
                    TrackItem::Gap(Gap::new(GAP, "leader", caption_range(0, 1_000))),
                    TrackItem::Caption(Caption::new(
                        CAPTION,
                        "Caption 1",
                        "Original text",
                        Some("en-US".to_owned()),
                        caption_range(1_000, 2_000),
                    )),
                ],
            )],
        )],
    )
    .unwrap()
}

#[test]
fn caption_mutations_publish_editable_semantics_and_round_trip() {
    let mut project = project();
    let style = CaptionStyle::new(
        Some("Inter".to_owned()),
        Some(42),
        Some("#ffffffff".to_owned()),
        Some("#000000cc".to_owned()),
        true,
        false,
        CaptionAlignment::Center,
        CaptionPosition::Bottom,
    )
    .unwrap();
    let relationship = CaptionTimelineRelationship::new(TIMELINE, None);
    let result = apply_caption_mutation_batch(
        &mut project,
        0,
        &[
            CaptionMutation::SetText {
                timeline_id: TIMELINE,
                caption_id: CAPTION,
                text: "Editable analysis text".to_owned(),
            },
            CaptionMutation::SetSpeaker {
                timeline_id: TIMELINE,
                caption_id: CAPTION,
                speaker: Some("Speaker A".to_owned()),
            },
            CaptionMutation::SetStyle {
                timeline_id: TIMELINE,
                caption_id: CAPTION,
                style: Some(style.clone()),
            },
            CaptionMutation::SetTimelineRelationships {
                timeline_id: TIMELINE,
                caption_id: CAPTION,
                relationships: vec![relationship],
            },
        ],
    )
    .unwrap();

    assert_eq!(result.revision(), 1);
    assert_eq!(result.outcomes().len(), 4);
    let timeline = project.timeline(TIMELINE).unwrap();
    let caption = timeline.track(TRACK).unwrap().items()[1]
        .as_caption()
        .unwrap();
    assert_eq!(caption.text(), "Editable analysis text");
    let attributes = CaptionAttributes::from_timeline(timeline, CAPTION).unwrap();
    assert_eq!(attributes.speaker(), Some("Speaker A"));
    assert_eq!(attributes.style(), Some(&style));
    assert_eq!(attributes.timeline_relationships(), &[relationship]);

    let encoded = serialize_timeline_state(&project).unwrap();
    let decoded = deserialize_timeline_state(&encoded).unwrap();
    let attributes =
        CaptionAttributes::from_timeline(decoded.project().timeline(TIMELINE).unwrap(), CAPTION)
            .unwrap();
    assert_eq!(attributes.speaker(), Some("Speaker A"));
    assert_eq!(attributes.style(), Some(&style));
    assert_eq!(attributes.timeline_relationships(), &[relationship]);
}

#[test]
fn caption_batches_are_atomic_when_a_later_target_is_missing() {
    let mut project = project();
    let original = project.clone();
    let error = apply_caption_mutation_batch(
        &mut project,
        0,
        &[
            CaptionMutation::SetText {
                timeline_id: TIMELINE,
                caption_id: CAPTION,
                text: "must roll back".to_owned(),
            },
            CaptionMutation::SetSpeaker {
                timeline_id: TIMELINE,
                caption_id: MISSING_CAPTION,
                speaker: Some("Nobody".to_owned()),
            },
        ],
    )
    .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::NotFound);
    assert_eq!(project, original);
}

#[test]
fn caption_style_rejects_ambiguous_colors_and_sizes() {
    assert!(CaptionStyle::new(
        None,
        Some(7),
        Some("white".to_owned()),
        None,
        false,
        false,
        CaptionAlignment::Start,
        CaptionPosition::Top,
    )
    .is_err());
}

#[test]
fn caption_fragments_inherit_durable_speaker_and_style_metadata() {
    let mut project = project();
    let style = CaptionStyle::new(
        None,
        Some(36),
        Some("#ffff00ff".to_owned()),
        None,
        false,
        true,
        CaptionAlignment::Start,
        CaptionPosition::Top,
    )
    .unwrap();
    apply_caption_mutation_batch(
        &mut project,
        0,
        &[
            CaptionMutation::SetSpeaker {
                timeline_id: TIMELINE,
                caption_id: CAPTION,
                speaker: Some("Narrator".to_owned()),
            },
            CaptionMutation::SetStyle {
                timeline_id: TIMELINE,
                caption_id: CAPTION,
                style: Some(style.clone()),
            },
        ],
    )
    .unwrap();

    apply_edit_batch(
        &mut project,
        1,
        &[EditOperation::razor(
            TIMELINE,
            TRACK,
            EditorialObjectId::Caption(CAPTION),
            RationalTime::new(2_000, Timebase::MILLISECONDS),
            EditorialObjectId::Caption(CAPTION_FRAGMENT),
        )],
    )
    .unwrap();

    let timeline = project.timeline(TIMELINE).unwrap();
    let original = CaptionAttributes::from_timeline(timeline, CAPTION).unwrap();
    let fragment = CaptionAttributes::from_timeline(timeline, CAPTION_FRAGMENT).unwrap();
    assert_eq!(fragment, original);
    assert_eq!(fragment.speaker(), Some("Narrator"));
    assert_eq!(fragment.style(), Some(&style));
}
