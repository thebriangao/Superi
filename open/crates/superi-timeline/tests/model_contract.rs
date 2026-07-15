use std::collections::BTreeMap;

use superi_core::error::ErrorCategory;
use superi_core::ids::{
    CaptionId, ClipId, GapId, GeneratorId, MediaId, ProjectId, TimelineId, TrackId, TransitionId,
};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_timeline::model::{
    Caption, CaptionPurpose, CaptionTrackSemantics, Clip, ClipSource, EditorialObjectId,
    EditorialProject, Gap, Generator, LanguageTag, LinkedMediaReference, Timeline, Track,
    TrackItem, TrackSemantics, Transition, VideoCompositing, VideoTrackSemantics,
};

const MEDIA: MediaId = MediaId::from_raw(1);
const SUB_TIMELINE: TimelineId = TimelineId::from_raw(10);
const MAIN_TIMELINE: TimelineId = TimelineId::from_raw(11);
const VIDEO_TRACK: TrackId = TrackId::from_raw(20);
const CAPTION_TRACK: TrackId = TrackId::from_raw(21);
const MAIN_TRACK: TrackId = TrackId::from_raw(22);
const CLIP: ClipId = ClipId::from_raw(30);
const NESTED_CLIP: ClipId = ClipId::from_raw(31);
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
    let gap = Gap::new(GAP, "tail gap", range(72, 12, edit_rate));
    let video = Track::new(
        VIDEO_TRACK,
        "V1",
        video_semantics(),
        vec![
            TrackItem::Clip(clip),
            TrackItem::Transition(transition),
            TrackItem::Generator(generator),
            TrackItem::Gap(gap),
        ],
    );
    let captions = Track::new(
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
    );
    let sub = Timeline::new(
        SUB_TIMELINE,
        "subsequence",
        edit_rate,
        RationalTime::zero(edit_rate),
        vec![video, captions],
    );
    let nested = Clip::new(
        NESTED_CLIP,
        "nested subsequence",
        ClipSource::Timeline(SUB_TIMELINE),
        range(0, 84, edit_rate),
        range(0, 84, edit_rate),
    )
    .unwrap();
    let main = Timeline::new(
        MAIN_TIMELINE,
        "main",
        edit_rate,
        RationalTime::zero(edit_rate),
        vec![Track::new(
            MAIN_TRACK,
            "V1",
            video_semantics(),
            vec![TrackItem::Clip(nested)],
        )],
    );

    EditorialProject::new(
        ProjectId::from_raw(100),
        "editorial project",
        [media],
        [sub, main],
    )
    .unwrap()
}

#[test]
fn project_preserves_every_editorial_object_and_rational_relationship() {
    let project = project_fixture();
    let edit_rate = Timebase::integer(24).unwrap();
    let source_rate = Timebase::integer(48).unwrap();

    assert_eq!(project.revision(), 0);
    assert_eq!(project.media_reference(MEDIA).unwrap().name(), "camera a");
    assert_eq!(
        project.timeline(SUB_TIMELINE).unwrap().duration().unwrap(),
        Duration::new(84, edit_rate).unwrap()
    );

    let track = project
        .timeline(SUB_TIMELINE)
        .unwrap()
        .track(VIDEO_TRACK)
        .unwrap();
    let clip = track
        .item(EditorialObjectId::Clip(CLIP))
        .unwrap()
        .as_clip()
        .unwrap();
    assert_eq!(clip.source(), ClipSource::Media(MEDIA));
    assert_eq!(clip.source_range(), range(48, 96, source_rate));
    assert_eq!(clip.record_range(), range(0, 48, edit_rate));
    assert_eq!(
        clip.source_range().duration().rational_time(),
        clip.record_range().duration().rational_time()
    );
    assert!(track.item(EditorialObjectId::Gap(GAP)).is_some());
    assert!(track
        .item(EditorialObjectId::Transition(TRANSITION))
        .is_some());
    assert!(track
        .item(EditorialObjectId::Generator(GENERATOR))
        .is_some());
    let caption_track = project
        .timeline(SUB_TIMELINE)
        .unwrap()
        .track(CAPTION_TRACK)
        .unwrap();
    assert_eq!(caption_track.semantics().timebase(), Timebase::MILLISECONDS);
    assert!(caption_track
        .item(EditorialObjectId::Caption(CAPTION))
        .is_some());

    let nested = project
        .timeline(MAIN_TIMELINE)
        .unwrap()
        .track(MAIN_TRACK)
        .unwrap()
        .item(EditorialObjectId::Clip(NESTED_CLIP))
        .unwrap()
        .as_clip()
        .unwrap();
    assert_eq!(nested.source(), ClipSource::Timeline(SUB_TIMELINE));
}

#[test]
fn validated_edits_are_atomic_revisioned_and_direct() {
    let mut project = project_fixture();
    let source_rate = Timebase::integer(48).unwrap();
    let before = project.clone();

    project
        .edit(0, |draft| {
            let clip = draft
                .timeline_mut(SUB_TIMELINE)?
                .track_mut(VIDEO_TRACK)?
                .item_mut(EditorialObjectId::Clip(CLIP))?
                .as_clip_mut()
                .expect("clip identity resolves to a clip");
            clip.set_source_range(range(96, 96, source_rate))?;

            let caption = draft
                .timeline_mut(SUB_TIMELINE)?
                .track_mut(CAPTION_TRACK)?
                .item_mut(EditorialObjectId::Caption(CAPTION))?
                .as_caption_mut()
                .expect("caption identity resolves to a caption");
            caption.set_text("A directly edited caption");
            Ok(())
        })
        .unwrap();

    assert_eq!(project.revision(), 1);
    assert_ne!(project, before);
    assert_eq!(
        project
            .timeline(SUB_TIMELINE)
            .unwrap()
            .track(VIDEO_TRACK)
            .unwrap()
            .item(EditorialObjectId::Clip(CLIP))
            .unwrap()
            .as_clip()
            .unwrap()
            .source_range(),
        range(96, 96, source_rate)
    );
    assert_eq!(project.media_reference(MEDIA).unwrap().name(), "camera a");

    let valid = project.clone();
    let edit_rate = Timebase::integer(24).unwrap();
    let error = project
        .edit(1, |draft| {
            draft
                .timeline_mut(SUB_TIMELINE)?
                .track_mut(VIDEO_TRACK)?
                .item_mut(EditorialObjectId::Clip(CLIP))?
                .as_clip_mut()
                .unwrap()
                .set_record_range(range(0, 47, edit_rate))?;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(
        project, valid,
        "a rejected edit must not publish partial state"
    );

    let error = project.edit(0, |_| Ok(())).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(project, valid);
}

#[test]
fn construction_rejects_missing_links_and_discontinuous_tracks() {
    let edit_rate = Timebase::integer(24).unwrap();
    let missing = Timeline::new(
        TimelineId::from_raw(200),
        "missing media",
        edit_rate,
        RationalTime::zero(edit_rate),
        vec![Track::new(
            TrackId::from_raw(201),
            "V1",
            video_semantics(),
            vec![TrackItem::Clip(
                Clip::new(
                    ClipId::from_raw(202),
                    "missing",
                    ClipSource::Media(MediaId::from_raw(999)),
                    range(0, 24, edit_rate),
                    range(0, 24, edit_rate),
                )
                .unwrap(),
            )],
        )],
    );
    let error =
        EditorialProject::new(ProjectId::from_raw(203), "broken", [], [missing]).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotFound);

    let discontinuous = Timeline::new(
        TimelineId::from_raw(210),
        "discontinuous",
        edit_rate,
        RationalTime::zero(edit_rate),
        vec![Track::new(
            TrackId::from_raw(211),
            "V1",
            video_semantics(),
            vec![
                TrackItem::Gap(Gap::new(
                    GapId::from_raw(212),
                    "first",
                    range(0, 12, edit_rate),
                )),
                TrackItem::Gap(Gap::new(
                    GapId::from_raw(213),
                    "unrepresented hole",
                    range(18, 6, edit_rate),
                )),
            ],
        )],
    );
    let error =
        EditorialProject::new(ProjectId::from_raw(214), "broken", [], [discontinuous]).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
}

#[test]
fn construction_rejects_invalid_transitions_and_nesting_cycles() {
    let edit_rate = Timebase::integer(24).unwrap();
    let first = Gap::new(GapId::from_raw(300), "first", range(0, 12, edit_rate));
    let second = Gap::new(GapId::from_raw(301), "second", range(12, 12, edit_rate));
    let invalid_transition = Timeline::new(
        TimelineId::from_raw(302),
        "invalid transition",
        edit_rate,
        RationalTime::zero(edit_rate),
        vec![Track::new(
            TrackId::from_raw(303),
            "V1",
            video_semantics(),
            vec![
                TrackItem::Gap(first),
                TrackItem::Transition(Transition::new(
                    TransitionId::from_raw(304),
                    "too long",
                    EditorialObjectId::Gap(GapId::from_raw(300)),
                    EditorialObjectId::Gap(GapId::from_raw(301)),
                    Duration::new(13, edit_rate).unwrap(),
                    Duration::new(1, edit_rate).unwrap(),
                )),
                TrackItem::Gap(second),
            ],
        )],
    );
    let error = EditorialProject::new(ProjectId::from_raw(305), "broken", [], [invalid_transition])
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let timeline_a = Timeline::new(
        TimelineId::from_raw(310),
        "a",
        edit_rate,
        RationalTime::zero(edit_rate),
        vec![Track::new(
            TrackId::from_raw(311),
            "V1",
            video_semantics(),
            vec![TrackItem::Clip(
                Clip::new(
                    ClipId::from_raw(312),
                    "a to b",
                    ClipSource::Timeline(TimelineId::from_raw(320)),
                    range(0, 24, edit_rate),
                    range(0, 24, edit_rate),
                )
                .unwrap(),
            )],
        )],
    );
    let timeline_b = Timeline::new(
        TimelineId::from_raw(320),
        "b",
        edit_rate,
        RationalTime::zero(edit_rate),
        vec![Track::new(
            TrackId::from_raw(321),
            "V1",
            video_semantics(),
            vec![TrackItem::Clip(
                Clip::new(
                    ClipId::from_raw(322),
                    "b to a",
                    ClipSource::Timeline(TimelineId::from_raw(310)),
                    range(0, 24, edit_rate),
                    range(0, 24, edit_rate),
                )
                .unwrap(),
            )],
        )],
    );
    let error = EditorialProject::new(
        ProjectId::from_raw(330),
        "cycle",
        [],
        [timeline_a, timeline_b],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
}
