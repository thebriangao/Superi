use superi_core::error::ErrorCategory;
use superi_core::ids::{
    BinId, ClipId, GapId, MediaId, ProjectId, SmartCollectionId, TimelineId, TrackId,
};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_timeline::edit_ops::{apply_edit_batch, EditOperation};
use superi_timeline::markers::{MetadataKey, MetadataValue};
use superi_timeline::media::{
    MediaBin, MediaPredicate, RelinkDecision, RelinkStatus, SmartCollection, SmartCollectionMatch,
};
use superi_timeline::model::{
    Clip, ClipSource, EditorialProject, Gap, LinkedMediaReference, Timeline, Track, TrackItem,
    TrackSemantics, VideoCompositing, VideoTrackSemantics,
};
use superi_timeline::retime::{ClipTimeMap, PlaybackRate};

const CAMERA_A: MediaId = MediaId::from_raw(1);
const CAMERA_B: MediaId = MediaId::from_raw(2);
const ROOT_BIN: BinId = BinId::from_raw(10);
const DAY_BIN: BinId = BinId::from_raw(11);
const SMART: SmartCollectionId = SmartCollectionId::from_raw(12);
const CHILD_TIMELINE: TimelineId = TimelineId::from_raw(20);
const MAIN_TIMELINE: TimelineId = TimelineId::from_raw(21);
const CHILD_TRACK: TrackId = TrackId::from_raw(30);
const MAIN_TRACK: TrackId = TrackId::from_raw(31);
const LEFT_CLIP: ClipId = ClipId::from_raw(40);
const RIGHT_CLIP: ClipId = ClipId::from_raw(41);
const NESTED_CLIP: ClipId = ClipId::from_raw(42);

fn range(start: i64, duration: u64, rate: Timebase) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, rate),
        Duration::new(duration, rate).unwrap(),
    )
    .unwrap()
}

fn video_semantics() -> TrackSemantics {
    TrackSemantics::Video(VideoTrackSemantics::new(
        FrameRate::FPS_24,
        VideoCompositing::Over,
    ))
}

fn media(id: MediaId, name: &str, target: &str, fingerprint: &str) -> LinkedMediaReference {
    LinkedMediaReference::with_fingerprint(
        id,
        name,
        target,
        Some(range(0, 480, Timebase::integer(48).unwrap())),
        fingerprint,
    )
    .unwrap()
}

fn project() -> EditorialProject {
    let record_rate = FrameRate::FPS_24.timebase();
    let source_rate = Timebase::integer(48).unwrap();
    let mut left = Clip::new(
        LEFT_CLIP,
        "left",
        ClipSource::Media(CAMERA_A),
        range(0, 16, source_rate),
        range(0, 8, record_rate),
    )
    .unwrap();
    left.set_time_map(
        ClipTimeMap::speed(
            left.record_range().duration(),
            RationalTime::zero(source_rate),
            PlaybackRate::new(2, 1).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let right = Clip::new(
        RIGHT_CLIP,
        "right",
        ClipSource::Media(CAMERA_B),
        range(16, 16, source_rate),
        range(8, 8, record_rate),
    )
    .unwrap();
    let child = Timeline::new(
        CHILD_TIMELINE,
        "child",
        record_rate,
        RationalTime::zero(record_rate),
        vec![Track::new(
            CHILD_TRACK,
            "V1",
            video_semantics(),
            vec![TrackItem::Gap(Gap::new(
                GapId::from_raw(50),
                "child content",
                range(0, 8, record_rate),
            ))],
        )],
    );
    let nested = Clip::new(
        NESTED_CLIP,
        "nested",
        ClipSource::Timeline(CHILD_TIMELINE),
        range(0, 8, record_rate),
        range(16, 8, record_rate),
    )
    .unwrap();
    let mut main = Timeline::new(
        MAIN_TIMELINE,
        "main",
        record_rate,
        RationalTime::zero(record_rate),
        vec![Track::new(
            MAIN_TRACK,
            "V1",
            video_semantics(),
            vec![
                TrackItem::Clip(left),
                TrackItem::Clip(right),
                TrackItem::Clip(nested),
            ],
        )],
    );
    main.link_clips([LEFT_CLIP, RIGHT_CLIP]).unwrap();
    main.group_clips([LEFT_CLIP, RIGHT_CLIP]).unwrap();
    main.set_track_targeted(MAIN_TRACK, true).unwrap();
    main.set_track_sync_locked(MAIN_TRACK, false).unwrap();

    EditorialProject::new(
        ProjectId::from_raw(60),
        "media library",
        [
            media(CAMERA_A, "A camera", "file:///old/a.mov", "sha256:a"),
            media(CAMERA_B, "B camera", "file:///old/b.mov", "sha256:b"),
        ],
        [child, main],
    )
    .unwrap()
}

#[test]
fn bins_metadata_and_smart_collections_remain_directly_editable() {
    let mut project = project();
    let timelines_before = project.timelines().cloned().collect::<Vec<_>>();
    let scene = MetadataKey::new("scene").unwrap();

    project
        .edit(0, |draft| {
            draft
                .media_reference_mut(CAMERA_A)?
                .metadata_mut()
                .insert(scene.clone(), MetadataValue::Text("exterior".into()));
            let library = draft.media_library_mut();
            library.upsert_bin(MediaBin::new(ROOT_BIN, "Production", None)?);
            library.upsert_bin(MediaBin::new(DAY_BIN, "Day 1", Some(ROOT_BIN))?);
            library.move_media(CAMERA_A, Some(DAY_BIN))?;
            library.upsert_smart_collection(SmartCollection::new(
                SMART,
                "Exterior online",
                SmartCollectionMatch::All,
                [
                    MediaPredicate::MetadataEquals {
                        key: scene.clone(),
                        value: MetadataValue::Text("exterior".into()),
                    },
                    MediaPredicate::RelinkStatus(RelinkStatus::Online),
                ],
            )?);
            Ok(())
        })
        .unwrap();

    assert_eq!(
        project.timelines().cloned().collect::<Vec<_>>(),
        timelines_before
    );
    assert_eq!(
        project.media_library().bin_path(DAY_BIN).unwrap(),
        [ROOT_BIN, DAY_BIN]
    );
    assert_eq!(
        project
            .media_library()
            .child_bins(Some(ROOT_BIN))
            .map(MediaBin::id)
            .collect::<Vec<_>>(),
        [DAY_BIN]
    );
    assert_eq!(
        project.media_library().bin(DAY_BIN).unwrap().media_ids(),
        &[CAMERA_A]
    );
    assert_eq!(project.smart_collection_members(SMART).unwrap(), [CAMERA_A]);

    project
        .edit(1, |draft| {
            draft
                .media_library_mut()
                .move_media(CAMERA_A, Some(ROOT_BIN))?;
            draft
                .media_reference_mut(CAMERA_A)?
                .metadata_mut()
                .remove(&scene);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        project.media_library().bin(ROOT_BIN).unwrap().media_ids(),
        &[CAMERA_A]
    );
    assert!(project
        .media_library()
        .bin(DAY_BIN)
        .unwrap()
        .media_ids()
        .is_empty());
    assert!(project.smart_collection_members(SMART).unwrap().is_empty());
    assert_eq!(
        project.timelines().cloned().collect::<Vec<_>>(),
        timelines_before
    );
}

#[test]
fn invalid_bin_graphs_and_membership_roll_back_atomically() {
    let mut project = project();
    project
        .edit(0, |draft| {
            draft
                .media_library_mut()
                .upsert_bin(MediaBin::new(ROOT_BIN, "Production", None)?);
            draft
                .media_library_mut()
                .upsert_bin(MediaBin::new(DAY_BIN, "Day 1", Some(ROOT_BIN))?);
            Ok(())
        })
        .unwrap();
    let before = project.clone();

    let cycle = project
        .edit(1, |draft| {
            draft
                .media_library_mut()
                .bin_mut(ROOT_BIN)?
                .set_parent(Some(DAY_BIN));
            Ok(())
        })
        .unwrap_err();
    assert_eq!(cycle.category(), ErrorCategory::Conflict);
    assert_eq!(project, before);

    let duplicate = project
        .edit(1, |draft| {
            draft
                .media_library_mut()
                .bin_mut(ROOT_BIN)?
                .add_media(CAMERA_B);
            draft
                .media_library_mut()
                .bin_mut(DAY_BIN)?
                .add_media(CAMERA_B);
            Ok(())
        })
        .unwrap_err();
    assert_eq!(duplicate.category(), ErrorCategory::Conflict);
    assert_eq!(project, before);

    let missing = project
        .edit(1, |draft| {
            draft
                .media_library_mut()
                .bin_mut(ROOT_BIN)?
                .add_media(MediaId::from_raw(999));
            Ok(())
        })
        .unwrap_err();
    assert_eq!(missing.category(), ErrorCategory::NotFound);
    assert_eq!(project, before);
}

#[test]
fn relink_evidence_preserves_identity_sequence_state_and_future_edits() {
    let mut project = project();
    let timelines_before = project.timelines().cloned().collect::<Vec<_>>();

    project
        .edit(0, |draft| {
            let media = draft.media_reference_mut(CAMERA_A)?;
            assert_eq!(
                media.consider_relink("file:///candidate/wrong.mov", "sha256:wrong")?,
                RelinkDecision::RejectedFingerprintMismatch
            );
            Ok(())
        })
        .unwrap();
    let rejected = project.media_reference(CAMERA_A).unwrap();
    assert_eq!(rejected.id(), CAMERA_A);
    assert_eq!(rejected.target(), "file:///old/a.mov");
    assert_eq!(
        rejected.relink_state().status(),
        RelinkStatus::FingerprintMismatch
    );
    assert_eq!(
        rejected.relink_state().rejected_target(),
        Some("file:///candidate/wrong.mov")
    );
    assert_eq!(
        project.timelines().cloned().collect::<Vec<_>>(),
        timelines_before
    );

    project
        .edit(1, |draft| {
            assert_eq!(
                draft
                    .media_reference_mut(CAMERA_A)?
                    .consider_relink("file:///new/a.mov", "sha256:a")?,
                RelinkDecision::Accepted
            );
            Ok(())
        })
        .unwrap();
    let accepted = project.media_reference(CAMERA_A).unwrap();
    assert_eq!(accepted.id(), CAMERA_A);
    assert_eq!(accepted.target(), "file:///new/a.mov");
    assert_eq!(accepted.relink_state().status(), RelinkStatus::Online);
    assert_eq!(
        project.timelines().cloned().collect::<Vec<_>>(),
        timelines_before
    );

    apply_edit_batch(
        &mut project,
        2,
        &[EditOperation::append(
            MAIN_TIMELINE,
            MAIN_TRACK,
            TrackItem::Gap(Gap::new(
                GapId::from_raw(51),
                "tail",
                range(0, 4, FrameRate::FPS_24.timebase()),
            )),
        )],
    )
    .unwrap();

    assert_eq!(project.media_reference(CAMERA_A).unwrap().id(), CAMERA_A);
    assert_eq!(
        project.media_reference(CAMERA_A).unwrap().target(),
        "file:///new/a.mov"
    );
    let main = project.timeline(MAIN_TIMELINE).unwrap();
    let left = main
        .track(MAIN_TRACK)
        .unwrap()
        .items()
        .iter()
        .find_map(TrackItem::as_clip)
        .unwrap();
    assert_eq!(left.source(), ClipSource::Media(CAMERA_A));
    assert_eq!(
        left.source_range(),
        range(0, 16, Timebase::integer(48).unwrap())
    );
    assert_eq!(
        left.record_range(),
        range(0, 8, FrameRate::FPS_24.timebase())
    );
    assert_eq!(
        left.time_map(),
        timelines_before[1]
            .track(MAIN_TRACK)
            .unwrap()
            .items()
            .iter()
            .find_map(TrackItem::as_clip)
            .unwrap()
            .time_map()
    );
    let state = main.edit_state();
    assert_eq!(
        state
            .link_for(LEFT_CLIP)
            .unwrap()
            .members()
            .collect::<Vec<_>>(),
        [LEFT_CLIP, RIGHT_CLIP]
    );
    assert_eq!(
        state
            .group_for(RIGHT_CLIP)
            .unwrap()
            .members()
            .collect::<Vec<_>>(),
        [LEFT_CLIP, RIGHT_CLIP]
    );
    assert!(state.track_state(MAIN_TRACK).unwrap().targeted());
    assert!(!state.track_state(MAIN_TRACK).unwrap().sync_locked());
    assert_eq!(
        main.track(MAIN_TRACK)
            .unwrap()
            .items()
            .iter()
            .filter_map(TrackItem::as_clip)
            .find(|clip| clip.id() == NESTED_CLIP)
            .unwrap()
            .source(),
        ClipSource::Timeline(CHILD_TIMELINE)
    );
}

#[test]
fn missing_and_unverified_states_remain_explicit_without_changing_identity() {
    let mut project = project();

    project
        .edit(0, |draft| {
            draft.media_reference_mut(CAMERA_B)?.mark_missing();
            Ok(())
        })
        .unwrap();
    let missing = project.media_reference(CAMERA_B).unwrap();
    assert_eq!(missing.id(), CAMERA_B);
    assert_eq!(missing.target(), "file:///old/b.mov");
    assert_eq!(missing.relink_state().status(), RelinkStatus::Missing);
    assert_eq!(
        missing.relink_state().expected_fingerprint(),
        Some("sha256:b")
    );

    project
        .edit(1, |draft| {
            draft
                .media_reference_mut(CAMERA_B)?
                .set_target("file:///unchecked/b.mov");
            Ok(())
        })
        .unwrap();
    let unverified = project.media_reference(CAMERA_B).unwrap();
    assert_eq!(unverified.id(), CAMERA_B);
    assert_eq!(unverified.target(), "file:///unchecked/b.mov");
    assert_eq!(unverified.relink_state().status(), RelinkStatus::Unverified);
    assert_eq!(
        unverified.relink_state().expected_fingerprint(),
        Some("sha256:b")
    );
}
