use superi_core::error::ErrorCategory;
use superi_core::ids::{ClipId, GapId, MediaId, ProjectId, TimelineId, TrackId};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_timeline::edit_ops::{apply_edit_batch, EditOperation};
use superi_timeline::ids::MulticamAngleId;
use superi_timeline::markers::{MetadataKey, MetadataValue, TimelineMetadata};
use superi_timeline::model::{
    Clip, ClipSource, EditorialObjectId, EditorialProject, Gap, LinkedMediaReference, Timeline,
    Track, TrackItem, TrackSemantics, VideoCompositing, VideoTrackSemantics,
};
use superi_timeline::multicam::{
    resolve_multicam_frame, MulticamAngle, MulticamAudioPolicy, MulticamClip, MulticamSource,
    MulticamSyncMethod,
};
use superi_timeline::multicam_ops::{
    apply_multicam_mutation_batch, MulticamMutation, MulticamMutationKind,
};
use superi_timeline::retime::{ClipTimeMap, PlaybackRate};

const CAMERA_A: MediaId = MediaId::from_raw(1);
const CAMERA_B: MediaId = MediaId::from_raw(2);
const SOURCE: TimelineId = TimelineId::from_raw(10);
const TARGET: TimelineId = TimelineId::from_raw(11);
const SOURCE_TRACK_A: TrackId = TrackId::from_raw(20);
const SOURCE_TRACK_B: TrackId = TrackId::from_raw(21);
const TARGET_TRACK: TrackId = TrackId::from_raw(22);
const SOURCE_CLIP_A: ClipId = ClipId::from_raw(30);
const SOURCE_CLIP_B: ClipId = ClipId::from_raw(31);
const TARGET_CLIP: ClipId = ClipId::from_raw(32);
const ANGLE_A: MulticamAngleId = MulticamAngleId::from_raw(40);
const ANGLE_B: MulticamAngleId = MulticamAngleId::from_raw(41);

fn record_rate() -> Timebase {
    FrameRate::FPS_24.timebase()
}

fn media_rate() -> Timebase {
    FrameRate::FPS_48.timebase()
}

fn range(start: i64, duration: u64, timebase: Timebase) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, timebase),
        Duration::new(duration, timebase).unwrap(),
    )
    .unwrap()
}

fn semantics() -> TrackSemantics {
    TrackSemantics::Video(VideoTrackSemantics::new(
        FrameRate::FPS_24,
        VideoCompositing::Over,
    ))
}

fn source_clip(id: ClipId, media: MediaId, source_start: i64) -> TrackItem {
    TrackItem::Clip(
        Clip::new(
            id,
            format!("source {}", id.raw()),
            ClipSource::Media(media),
            range(source_start, 48, media_rate()),
            range(0, 24, record_rate()),
        )
        .unwrap(),
    )
}

fn project() -> EditorialProject {
    let source = Timeline::new(
        SOURCE,
        "synchronized cameras",
        record_rate(),
        RationalTime::zero(record_rate()),
        vec![
            Track::new(
                SOURCE_TRACK_A,
                "camera a",
                semantics(),
                vec![source_clip(SOURCE_CLIP_A, CAMERA_A, 100)],
            ),
            Track::new(
                SOURCE_TRACK_B,
                "camera b",
                semantics(),
                vec![source_clip(SOURCE_CLIP_B, CAMERA_B, 200)],
            ),
        ],
    );
    let mut target_clip = Clip::new(
        TARGET_CLIP,
        "multicam interview",
        ClipSource::Timeline(SOURCE),
        range(0, 12, record_rate()),
        range(10, 12, record_rate()),
    )
    .unwrap();
    target_clip
        .set_time_map(
            ClipTimeMap::speed(
                target_clip.record_range().duration(),
                RationalTime::zero(record_rate()),
                PlaybackRate::new(2, 1).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    let target = Timeline::new(
        TARGET,
        "edited interview",
        record_rate(),
        RationalTime::zero(record_rate()),
        vec![Track::new(
            TARGET_TRACK,
            "V1",
            semantics(),
            vec![
                TrackItem::Gap(Gap::new(
                    GapId::from_raw(33),
                    "leader",
                    range(0, 10, record_rate()),
                )),
                TrackItem::Clip(target_clip),
            ],
        )],
    );
    EditorialProject::new(
        ProjectId::from_raw(50),
        "multicam",
        [
            LinkedMediaReference::new(
                CAMERA_A,
                "camera a",
                "urn:camera:a",
                Some(range(0, 400, media_rate())),
            ),
            LinkedMediaReference::new(
                CAMERA_B,
                "camera b",
                "urn:camera:b",
                Some(range(0, 400, media_rate())),
            ),
        ],
        [source, target],
    )
    .unwrap()
}

fn configure(project: &mut EditorialProject) {
    project
        .edit(0, |draft| {
            let mut camera_metadata = TimelineMetadata::new();
            camera_metadata.insert(
                MetadataKey::new("camera.serial")?,
                MetadataValue::Text("A-001".to_owned()),
            );
            let mut angle_a = MulticamAngle::new(ANGLE_A, "wide", "A", [SOURCE_CLIP_A])?;
            angle_a.set_metadata(camera_metadata);
            let angle_b = MulticamAngle::new(ANGLE_B, "close", "B", [SOURCE_CLIP_B])?;
            draft
                .timeline_mut(SOURCE)?
                .set_multicam_source(MulticamSource::new(
                    MulticamSyncMethod::Timecode,
                    [angle_a, angle_b],
                )?)?;

            let mut clip = MulticamClip::new(
                TARGET_CLIP,
                range(0, 24, record_rate()),
                ANGLE_A,
                MulticamAudioPolicy::Fixed(ANGLE_A),
            )?;
            clip.switch_range(range(12, 8, record_rate()), ANGLE_B)?;
            draft.timeline_mut(TARGET)?.upsert_multicam_clip(clip)?;
            Ok(())
        })
        .unwrap();
}

#[test]
fn angle_metadata_switches_and_exact_nested_resolution_remain_directly_editable() {
    let mut project = project();
    configure(&mut project);

    let source = project.timeline(SOURCE).unwrap().multicam_source().unwrap();
    assert_eq!(source.sync_method(), &MulticamSyncMethod::Timecode);
    assert_eq!(
        source
            .angles()
            .iter()
            .map(MulticamAngle::id)
            .collect::<Vec<_>>(),
        vec![ANGLE_A, ANGLE_B]
    );
    let serial = MetadataKey::new("camera.serial").unwrap();
    assert_eq!(
        source.angle(ANGLE_A).unwrap().metadata().get(&serial),
        Some(&MetadataValue::Text("A-001".to_owned()))
    );

    let target = project.timeline(TARGET).unwrap();
    let clip = target.multicam_clip(TARGET_CLIP).unwrap();
    assert_eq!(clip.switches().len(), 3);
    assert_eq!(clip.audio_policy(), &MulticamAudioPolicy::Fixed(ANGLE_A));
    assert_eq!(
        clip.angle_at(RationalTime::new(11, record_rate())).unwrap(),
        ANGLE_A
    );
    assert_eq!(
        clip.angle_at(RationalTime::new(12, record_rate())).unwrap(),
        ANGLE_B
    );

    let resolved = resolve_multicam_frame(
        &project,
        TARGET,
        TARGET_CLIP,
        RationalTime::new(16, record_rate()),
    )
    .unwrap();
    assert_eq!(resolved.source_timeline_id(), SOURCE);
    assert_eq!(
        resolved.source_timeline_time(),
        RationalTime::new(12, record_rate())
    );
    assert_eq!(resolved.angle_id(), ANGLE_B);
    assert_eq!(resolved.source_clip_id(), SOURCE_CLIP_B);
    assert_eq!(resolved.source(), ClipSource::Media(CAMERA_B));
    assert_eq!(resolved.source_time(), RationalTime::new(224, media_rate()));
    assert_eq!(resolved.audio_angle_ids(), &[ANGLE_A]);

    project
        .edit(1, |draft| {
            draft
                .timeline_mut(TARGET)?
                .multicam_clip_mut(TARGET_CLIP)?
                .move_cut(
                    RationalTime::new(12, record_rate()),
                    RationalTime::new(14, record_rate()),
                )
        })
        .unwrap();
    let clip = project
        .timeline(TARGET)
        .unwrap()
        .multicam_clip(TARGET_CLIP)
        .unwrap();
    assert_eq!(
        clip.angle_at(RationalTime::new(13, record_rate())).unwrap(),
        ANGLE_A
    );
    assert_eq!(
        clip.angle_at(RationalTime::new(14, record_rate())).unwrap(),
        ANGLE_B
    );

    project
        .edit(2, |draft| {
            draft
                .timeline_mut(TARGET)?
                .multicam_clip_mut(TARGET_CLIP)?
                .set_audio_policy(MulticamAudioPolicy::AllAngles);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        resolve_multicam_frame(
            &project,
            TARGET,
            TARGET_CLIP,
            RationalTime::new(17, record_rate()),
        )
        .unwrap()
        .audio_angle_ids(),
        &[ANGLE_A, ANGLE_B]
    );
}

#[test]
fn structural_fragments_preserve_target_switches_and_source_angle_membership() {
    let mut project = project();
    configure(&mut project);
    let target_fragment = ClipId::from_raw(60);
    apply_edit_batch(
        &mut project,
        1,
        &[EditOperation::razor(
            TARGET,
            TARGET_TRACK,
            EditorialObjectId::Clip(TARGET_CLIP),
            RationalTime::new(16, record_rate()),
            EditorialObjectId::Clip(target_fragment),
        )],
    )
    .unwrap();

    let target = project.timeline(TARGET).unwrap();
    assert_eq!(
        target.multicam_clip(target_fragment).unwrap().switches(),
        target.multicam_clip(TARGET_CLIP).unwrap().switches()
    );

    let source_fragment = ClipId::from_raw(61);
    apply_edit_batch(
        &mut project,
        2,
        &[EditOperation::razor(
            SOURCE,
            SOURCE_TRACK_A,
            EditorialObjectId::Clip(SOURCE_CLIP_A),
            RationalTime::new(12, record_rate()),
            EditorialObjectId::Clip(source_fragment),
        )],
    )
    .unwrap();
    assert_eq!(
        project
            .timeline(SOURCE)
            .unwrap()
            .multicam_source()
            .unwrap()
            .angle(ANGLE_A)
            .unwrap()
            .source_clips(),
        &[SOURCE_CLIP_A, source_fragment]
    );
}

#[test]
fn replacements_transfer_target_switch_programs_and_source_angle_membership() {
    let mut project = project();
    configure(&mut project);
    let original_switches = project
        .timeline(TARGET)
        .unwrap()
        .multicam_clip(TARGET_CLIP)
        .unwrap()
        .switches()
        .to_vec();
    let replacement_target_id = ClipId::from_raw(70);
    let mut replacement_target = Clip::new(
        replacement_target_id,
        "replacement multicam interview",
        ClipSource::Timeline(SOURCE),
        range(0, 12, record_rate()),
        range(10, 12, record_rate()),
    )
    .unwrap();
    replacement_target
        .set_time_map(
            ClipTimeMap::speed(
                replacement_target.record_range().duration(),
                RationalTime::zero(record_rate()),
                PlaybackRate::new(2, 1).unwrap(),
            )
            .unwrap(),
        )
        .unwrap();
    apply_edit_batch(
        &mut project,
        1,
        &[EditOperation::replace(
            TARGET,
            TARGET_TRACK,
            EditorialObjectId::Clip(TARGET_CLIP),
            TrackItem::Clip(replacement_target),
        )],
    )
    .unwrap();
    let target = project.timeline(TARGET).unwrap();
    assert!(target.multicam_clip(TARGET_CLIP).is_none());
    assert_eq!(
        target
            .multicam_clip(replacement_target_id)
            .unwrap()
            .switches(),
        original_switches
    );

    let replacement_source_id = ClipId::from_raw(71);
    apply_edit_batch(
        &mut project,
        2,
        &[EditOperation::replace(
            SOURCE,
            SOURCE_TRACK_A,
            EditorialObjectId::Clip(SOURCE_CLIP_A),
            source_clip(replacement_source_id, CAMERA_A, 100),
        )],
    )
    .unwrap();
    assert_eq!(
        project
            .timeline(SOURCE)
            .unwrap()
            .multicam_source()
            .unwrap()
            .angle(ANGLE_A)
            .unwrap()
            .source_clips(),
        &[replacement_source_id]
    );
    assert_eq!(
        resolve_multicam_frame(
            &project,
            TARGET,
            replacement_target_id,
            RationalTime::new(16, record_rate()),
        )
        .unwrap()
        .angle_id(),
        ANGLE_B
    );
}

#[test]
fn invalid_multicam_state_and_stale_edits_roll_back_atomically() {
    let mut project = project();
    let before = project.clone();
    let error = project
        .edit(0, |draft| {
            let angle_a = MulticamAngle::new(ANGLE_A, "wide", "A", [SOURCE_CLIP_A])?;
            let angle_b = MulticamAngle::new(ANGLE_B, "close", "B", [SOURCE_CLIP_B])?;
            draft
                .timeline_mut(SOURCE)?
                .set_multicam_source(MulticamSource::new(
                    MulticamSyncMethod::Audio,
                    [angle_a, angle_b],
                )?)?;
            draft
                .timeline_mut(TARGET)?
                .upsert_multicam_clip(MulticamClip::new(
                    TARGET_CLIP,
                    range(0, 12, record_rate()),
                    ANGLE_A,
                    MulticamAudioPolicy::FollowVideo,
                )?)?;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(project, before);

    configure(&mut project);
    let configured = project.clone();
    let disabled = project
        .edit(1, |draft| {
            draft
                .timeline_mut(SOURCE)?
                .multicam_source_mut()?
                .angle_mut(ANGLE_B)?
                .set_enabled(false);
            Ok(())
        })
        .unwrap_err();
    assert_eq!(disabled.category(), ErrorCategory::InvalidInput);
    assert_eq!(project, configured);

    let stale = project.edit(0, |_| Ok(())).unwrap_err();
    assert_eq!(stale.category(), ErrorCategory::Conflict);
    assert_eq!(project, configured);
}

#[test]
fn authored_multicam_mutations_create_switch_refine_and_detach_atomically() {
    let mut project = project();
    let source = MulticamSource::new(
        MulticamSyncMethod::Timecode,
        [
            MulticamAngle::new(ANGLE_A, "wide", "A", [SOURCE_CLIP_A]).unwrap(),
            MulticamAngle::new(ANGLE_B, "close", "B", [SOURCE_CLIP_B]).unwrap(),
        ],
    )
    .unwrap();
    let created = apply_multicam_mutation_batch(
        &mut project,
        0,
        &[
            MulticamMutation::SetSource {
                timeline_id: SOURCE,
                source,
            },
            MulticamMutation::AttachClip {
                timeline_id: TARGET,
                clip_id: TARGET_CLIP,
                initial_angle_id: ANGLE_A,
                audio_policy: MulticamAudioPolicy::FollowVideo,
            },
        ],
    )
    .unwrap();
    assert_eq!(created.revision(), 1);
    assert_eq!(
        created
            .outcomes()
            .iter()
            .map(|outcome| outcome.kind())
            .collect::<Vec<_>>(),
        vec![
            MulticamMutationKind::SetSource,
            MulticamMutationKind::AttachClip,
        ]
    );

    let switched = apply_multicam_mutation_batch(
        &mut project,
        1,
        &[
            MulticamMutation::SwitchAt {
                timeline_id: TARGET,
                clip_id: TARGET_CLIP,
                record_time: RationalTime::new(16, record_rate()),
                angle_id: ANGLE_B,
            },
            MulticamMutation::SwitchAt {
                timeline_id: TARGET,
                clip_id: TARGET_CLIP,
                record_time: RationalTime::new(18, record_rate()),
                angle_id: ANGLE_B,
            },
            MulticamMutation::SwitchAt {
                timeline_id: TARGET,
                clip_id: TARGET_CLIP,
                record_time: RationalTime::new(20, record_rate()),
                angle_id: ANGLE_A,
            },
            MulticamMutation::MoveCut {
                timeline_id: TARGET,
                clip_id: TARGET_CLIP,
                at_record_time: RationalTime::new(20, record_rate()),
                to_record_time: RationalTime::new(19, record_rate()),
            },
            MulticamMutation::SetAudioPolicy {
                timeline_id: TARGET,
                clip_id: TARGET_CLIP,
                audio_policy: MulticamAudioPolicy::AllAngles,
            },
            MulticamMutation::SetSyncMethod {
                timeline_id: SOURCE,
                sync_method: MulticamSyncMethod::Manual,
            },
        ],
    )
    .unwrap();
    assert_eq!(switched.revision(), 2);
    let source = project.timeline(SOURCE).unwrap().multicam_source().unwrap();
    assert_eq!(source.sync_method(), &MulticamSyncMethod::Manual);
    let clip = project
        .timeline(TARGET)
        .unwrap()
        .multicam_clip(TARGET_CLIP)
        .unwrap();
    assert_eq!(clip.switches().len(), 3);
    assert_eq!(
        clip.switches()[1].source_range(),
        range(12, 6, record_rate())
    );
    assert_eq!(clip.switches()[1].angle_id(), ANGLE_B);
    assert_eq!(clip.audio_policy(), &MulticamAudioPolicy::AllAngles);

    let before_invalid = project.clone();
    let invalid = apply_multicam_mutation_batch(
        &mut project,
        2,
        &[
            MulticamMutation::SetSyncMethod {
                timeline_id: SOURCE,
                sync_method: MulticamSyncMethod::Audio,
            },
            MulticamMutation::SwitchAt {
                timeline_id: TARGET,
                clip_id: TARGET_CLIP,
                record_time: RationalTime::new(17, record_rate()),
                angle_id: MulticamAngleId::from_raw(999),
            },
        ],
    )
    .unwrap_err();
    assert_eq!(invalid.category(), ErrorCategory::NotFound);
    assert_eq!(project, before_invalid);

    let detached = apply_multicam_mutation_batch(
        &mut project,
        2,
        &[MulticamMutation::DetachClip {
            timeline_id: TARGET,
            clip_id: TARGET_CLIP,
        }],
    )
    .unwrap();
    assert_eq!(detached.revision(), 3);
    assert_eq!(
        detached.outcomes()[0].kind(),
        MulticamMutationKind::DetachClip
    );
    assert!(project
        .timeline(TARGET)
        .unwrap()
        .multicam_clip(TARGET_CLIP)
        .is_none());
}
