use superi_audio::mixing::{ChannelMap, ClipMixControls, ClipMixMutation, ClipMixState};
use superi_core::ids::{ClipId, MediaId, ProjectId, TimelineId, TrackId};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_engine::audio_mix::apply_edit_batch_with_clip_mix;
use superi_timeline::edit_ops::EditOperation;
use superi_timeline::model::{
    Clip, ClipSource, EditorialObjectId, EditorialProject, LinkedMediaReference, Timeline, Track,
    TrackItem, TrackSemantics, VideoCompositing, VideoTrackSemantics,
};

const MEDIA: MediaId = MediaId::from_raw(1);
const TIMELINE: TimelineId = TimelineId::from_raw(2);
const TRACK: TrackId = TrackId::from_raw(3);
const ORIGINAL: ClipId = ClipId::from_raw(10);
const PARTNER: ClipId = ClipId::from_raw(11);
const FRAGMENT: ClipId = ClipId::from_raw(12);

fn rate() -> Timebase {
    Timebase::integer(24).unwrap()
}

fn range(start: i64, duration: u64) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, rate()),
        Duration::new(duration, rate()).unwrap(),
    )
    .unwrap()
}

fn clip(id: ClipId, source_start: i64, record_start: i64) -> TrackItem {
    TrackItem::Clip(
        Clip::new(
            id,
            format!("clip {id}"),
            ClipSource::Media(MEDIA),
            range(source_start, 8),
            range(record_start, 8),
        )
        .unwrap(),
    )
}

fn project() -> EditorialProject {
    let semantics = TrackSemantics::Video(VideoTrackSemantics::new(
        FrameRate::FPS_24,
        VideoCompositing::Over,
    ));
    let mut project = EditorialProject::new(
        ProjectId::from_raw(20),
        "audio editorial intent",
        [LinkedMediaReference::new(
            MEDIA,
            "media",
            "urn:superi:test:audio-editorial",
            Some(range(0, 100)),
        )],
        [Timeline::new(
            TIMELINE,
            "main",
            rate(),
            RationalTime::zero(rate()),
            vec![Track::new(
                TRACK,
                "V1",
                semantics,
                vec![clip(ORIGINAL, 20, 0), clip(PARTNER, 40, 8)],
            )],
        )],
    )
    .unwrap();
    project
        .edit(0, |draft| {
            let timeline = draft.timeline_mut(TIMELINE)?;
            timeline.group_clips([ORIGINAL, PARTNER])?;
            timeline.link_clips([ORIGINAL, PARTNER])?;
            timeline.set_track_sync_locked(TRACK, true)?;
            Ok(())
        })
        .unwrap();
    project
}

fn controls() -> ClipMixControls {
    let stereo = ChannelLayout::stereo();
    ClipMixControls::new(
        stereo.clone(),
        stereo,
        [
            ChannelMap::new(ChannelPosition::FrontLeft, ChannelPosition::FrontRight, 1.0).unwrap(),
            ChannelMap::new(ChannelPosition::FrontRight, ChannelPosition::FrontLeft, 1.0).unwrap(),
        ],
    )
    .unwrap()
    .with_gain(0.75)
    .unwrap()
    .with_fades(48, 96)
    .unwrap()
    .with_pan(-0.2)
    .unwrap()
    .with_muted(false)
    .with_solo(true)
    .with_phase_inverted([ChannelPosition::FrontRight])
    .unwrap()
}

#[test]
fn razor_preserves_editorial_state_and_inherits_complete_audio_intent() {
    let mut project = project();
    let mut mix = ClipMixState::new();
    let intent = controls();
    mix.apply(0, &[ClipMixMutation::set(ORIGINAL, intent.clone())])
        .unwrap();

    let result = apply_edit_batch_with_clip_mix(
        &mut project,
        1,
        &mut mix,
        1,
        &[EditOperation::razor(
            TIMELINE,
            TRACK,
            EditorialObjectId::Clip(ORIGINAL),
            RationalTime::new(4, rate()),
            EditorialObjectId::Clip(FRAGMENT),
        )],
    )
    .unwrap();
    assert_eq!(result.revision(), 2);

    let timeline = project.timeline(TIMELINE).unwrap();
    let original = timeline
        .track(TRACK)
        .unwrap()
        .item(EditorialObjectId::Clip(ORIGINAL))
        .unwrap()
        .as_clip()
        .unwrap();
    let fragment = timeline
        .track(TRACK)
        .unwrap()
        .item(EditorialObjectId::Clip(FRAGMENT))
        .unwrap()
        .as_clip()
        .unwrap();
    assert_eq!(original.source(), ClipSource::Media(MEDIA));
    assert_eq!(original.source_range(), range(20, 4));
    assert_eq!(original.record_range(), range(0, 4));
    assert_eq!(fragment.source(), ClipSource::Media(MEDIA));
    assert_eq!(fragment.source_range(), range(24, 4));
    assert_eq!(fragment.record_range(), range(4, 4));
    assert_eq!(mix.controls(ORIGINAL), Some(&intent));
    assert_eq!(mix.controls(FRAGMENT), Some(&intent));

    let state = timeline.edit_state();
    assert!(state
        .group_for(ORIGINAL)
        .unwrap()
        .members()
        .any(|member| member == FRAGMENT));
    assert!(state
        .link_for(ORIGINAL)
        .unwrap()
        .members()
        .any(|member| member == FRAGMENT));
    assert!(state.track_state(TRACK).unwrap().sync_locked());
}

#[test]
fn timeline_and_mix_publication_is_atomic_when_either_revision_conflicts() {
    let mut with_intent_project = project();
    let mut mix = ClipMixState::new();
    mix.apply(0, &[ClipMixMutation::set(ORIGINAL, controls())])
        .unwrap();
    let project_before = with_intent_project.clone();
    let mix_before = mix.clone();

    let error = apply_edit_batch_with_clip_mix(
        &mut with_intent_project,
        1,
        &mut mix,
        0,
        &[EditOperation::razor(
            TIMELINE,
            TRACK,
            EditorialObjectId::Clip(ORIGINAL),
            RationalTime::new(4, rate()),
            EditorialObjectId::Clip(FRAGMENT),
        )],
    )
    .unwrap_err();
    assert_eq!(
        error.category(),
        superi_core::error::ErrorCategory::Conflict
    );
    assert_eq!(with_intent_project, project_before);
    assert_eq!(mix, mix_before);

    let mut project = project();
    let mut empty_mix = ClipMixState::new();
    let project_before = project.clone();
    let error = apply_edit_batch_with_clip_mix(
        &mut project,
        1,
        &mut empty_mix,
        1,
        &[EditOperation::razor(
            TIMELINE,
            TRACK,
            EditorialObjectId::Clip(ORIGINAL),
            RationalTime::new(4, rate()),
            EditorialObjectId::Clip(FRAGMENT),
        )],
    )
    .unwrap_err();
    assert_eq!(
        error.category(),
        superi_core::error::ErrorCategory::Conflict
    );
    assert_eq!(project, project_before);
    assert_eq!(empty_mix.revision(), 0);
}
