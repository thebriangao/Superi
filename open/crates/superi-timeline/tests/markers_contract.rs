use superi_core::diagnostics::FiniteF64;
use superi_core::error::ErrorCategory;
use superi_core::ids::{ClipId, MarkerId, MediaId, ProjectId, TimelineId, TrackId};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_timeline::edit_ops::{apply_edit_batch, EditOperation};
use superi_timeline::marker_ops::{
    apply_marker_mutation_batch, MarkerMutation, MarkerMutationKind,
};
use superi_timeline::markers::{
    Marker, MarkerFlag, MarkerLabel, MarkerNote, MarkerOwner, MetadataKey, MetadataOwner,
    MetadataValue, SnapRequest, SnapTarget, SnapTargetKind, TimelineMetadata,
};
use superi_timeline::model::{
    Clip, ClipSource, EditorialObjectId, EditorialProject, LinkedMediaReference, Timeline, Track,
    TrackItem, TrackSemantics, VideoCompositing, VideoTrackSemantics,
};

const MEDIA: MediaId = MediaId::from_raw(1);
const TIMELINE: TimelineId = TimelineId::from_raw(2);
const TRACK: TrackId = TrackId::from_raw(3);
const A: ClipId = ClipId::from_raw(4);
const B: ClipId = ClipId::from_raw(5);
const INSERTED: ClipId = ClipId::from_raw(6);
const OBJECT_MARKER: MarkerId = MarkerId::from_raw(10);
const TIMELINE_MARKER: MarkerId = MarkerId::from_raw(11);
const OVERSCAN_MARKER: MarkerId = MarkerId::from_raw(12);
const TRACK_MARKER: MarkerId = MarkerId::from_raw(13);
const CREATED_MARKER: MarkerId = MarkerId::from_raw(14);
const MISSING_MARKER: MarkerId = MarkerId::from_raw(99);

fn edit_rate() -> Timebase {
    Timebase::integer(24).unwrap()
}

fn source_rate() -> Timebase {
    Timebase::integer(48).unwrap()
}

fn range(start: i64, duration: u64, timebase: Timebase) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, timebase),
        Duration::new(duration, timebase).unwrap(),
    )
    .unwrap()
}

fn clip(id: ClipId, source_start: i64, record_start: i64, duration: u64) -> TrackItem {
    TrackItem::Clip(
        Clip::new(
            id,
            format!("clip {}", id.raw()),
            ClipSource::Media(MEDIA),
            range(source_start, duration * 2, source_rate()),
            range(record_start, duration, edit_rate()),
        )
        .unwrap(),
    )
}

fn project_fixture() -> EditorialProject {
    let semantics = TrackSemantics::Video(VideoTrackSemantics::new(
        FrameRate::FPS_24,
        VideoCompositing::Over,
    ));
    let timeline = Timeline::new(
        TIMELINE,
        "main",
        edit_rate(),
        RationalTime::zero(edit_rate()),
        vec![Track::new(
            TRACK,
            "V1",
            semantics,
            vec![clip(A, 0, 0, 8), clip(B, 16, 8, 4)],
        )],
    );
    EditorialProject::new(
        ProjectId::from_raw(100),
        "marker contracts",
        [LinkedMediaReference::new(
            MEDIA,
            "camera",
            "urn:superi:test:camera",
            Some(range(0, 400, source_rate())),
        )],
        [timeline],
    )
    .unwrap()
}

fn metadata(entries: &[(&str, MetadataValue)]) -> TimelineMetadata {
    let mut metadata = TimelineMetadata::new();
    for (key, value) in entries {
        metadata.insert(MetadataKey::new(*key).unwrap(), value.clone());
    }
    metadata
}

fn annotated_project() -> EditorialProject {
    let mut project = project_fixture();
    project
        .edit(0, |draft| {
            let timeline = draft.timeline_mut(TIMELINE)?;
            let mut object_marker = Marker::new(
                OBJECT_MARKER,
                MarkerOwner::Object(EditorialObjectId::Clip(B)),
                range(1, 2, edit_rate()),
            )?;
            object_marker.set_label(Some(MarkerLabel::new("performance").unwrap()));
            object_marker.set_flag(Some(MarkerFlag::Green));
            object_marker.set_note(Some(MarkerNote::new("hold this beat").unwrap()));
            object_marker.set_metadata(metadata(&[
                (
                    "superi.identity",
                    MetadataValue::Unsigned(OBJECT_MARKER.raw() as u64),
                ),
                (
                    "superi.intent",
                    MetadataValue::Map(metadata(&[(
                        "department",
                        MetadataValue::Text("editorial".to_owned()),
                    )])),
                ),
            ]));
            timeline.upsert_marker(object_marker)?;
            timeline.upsert_marker(Marker::new(
                TIMELINE_MARKER,
                MarkerOwner::Timeline,
                range(6, 0, edit_rate()),
            )?)?;
            timeline.upsert_marker(Marker::new(
                OVERSCAN_MARKER,
                MarkerOwner::Object(EditorialObjectId::Clip(B)),
                range(5, 1, edit_rate()),
            )?)?;
            timeline.upsert_marker(Marker::new(
                TRACK_MARKER,
                MarkerOwner::Track(TRACK),
                range(5, 0, edit_rate()),
            )?)?;
            timeline.set_metadata(
                MetadataOwner::Timeline,
                metadata(&[("sequence", MetadataValue::Text("main".to_owned()))]),
            )?;
            timeline.set_metadata(
                MetadataOwner::Track(TRACK),
                metadata(&[("role", MetadataValue::Text("picture".to_owned()))]),
            )?;
            timeline.set_metadata(
                MetadataOwner::Object(EditorialObjectId::Clip(B)),
                metadata(&[("sync", MetadataValue::Range(range(0, 4, edit_rate())))]),
            )?;
            Ok(())
        })
        .unwrap();
    project
}

#[test]
fn marker_mutation_batch_authors_every_visible_field_without_losing_owner_or_metadata() {
    let mut project = annotated_project();
    let retained_metadata = project
        .timeline(TIMELINE)
        .unwrap()
        .marker(OBJECT_MARKER)
        .unwrap()
        .metadata()
        .clone();
    let mut created = Marker::new(
        CREATED_MARKER,
        MarkerOwner::Track(TRACK),
        range(3, 2, edit_rate()),
    )
    .unwrap();
    created.set_label(Some(MarkerLabel::new("review").unwrap()));
    created.set_flag(Some(MarkerFlag::Yellow));
    created.set_note(Some(MarkerNote::new("check the cut").unwrap()));

    let result = apply_marker_mutation_batch(
        &mut project,
        1,
        &[
            MarkerMutation::Create {
                timeline_id: TIMELINE,
                marker: created,
            },
            MarkerMutation::SetRange {
                timeline_id: TIMELINE,
                marker_id: OBJECT_MARKER,
                marked_range: range(2, 1, edit_rate()),
            },
            MarkerMutation::SetLabel {
                timeline_id: TIMELINE,
                marker_id: OBJECT_MARKER,
                label: Some(MarkerLabel::new("take two").unwrap()),
            },
            MarkerMutation::SetFlag {
                timeline_id: TIMELINE,
                marker_id: OBJECT_MARKER,
                flag: Some(MarkerFlag::Blue),
            },
            MarkerMutation::SetNote {
                timeline_id: TIMELINE,
                marker_id: OBJECT_MARKER,
                note: None,
            },
            MarkerMutation::Remove {
                timeline_id: TIMELINE,
                marker_id: TIMELINE_MARKER,
            },
        ],
    )
    .unwrap();

    assert_eq!(result.revision(), 2);
    assert_eq!(
        result
            .outcomes()
            .iter()
            .map(|outcome| outcome.kind())
            .collect::<Vec<_>>(),
        [
            MarkerMutationKind::Create,
            MarkerMutationKind::SetRange,
            MarkerMutationKind::SetLabel,
            MarkerMutationKind::SetFlag,
            MarkerMutationKind::SetNote,
            MarkerMutationKind::Remove,
        ]
    );
    let timeline = project.timeline(TIMELINE).unwrap();
    let marker = timeline.marker(OBJECT_MARKER).unwrap();
    assert_eq!(
        marker.owner(),
        MarkerOwner::Object(EditorialObjectId::Clip(B))
    );
    assert_eq!(marker.marked_range(), range(2, 1, edit_rate()));
    assert_eq!(marker.label().unwrap().as_str(), "take two");
    assert_eq!(marker.flag(), Some(MarkerFlag::Blue));
    assert_eq!(marker.note(), None);
    assert_eq!(marker.metadata(), &retained_metadata);
    assert_eq!(
        timeline.marker(CREATED_MARKER).unwrap().owner(),
        MarkerOwner::Track(TRACK)
    );
    assert!(timeline.marker(TIMELINE_MARKER).is_none());
}

#[test]
fn marker_mutation_batch_rejects_empty_duplicate_and_missing_targets_atomically() {
    let mut project = annotated_project();
    let before = project.clone();
    assert_eq!(
        apply_marker_mutation_batch(&mut project, 1, &[])
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(project, before);

    let error = apply_marker_mutation_batch(
        &mut project,
        1,
        &[
            MarkerMutation::SetLabel {
                timeline_id: TIMELINE,
                marker_id: OBJECT_MARKER,
                label: Some(MarkerLabel::new("must roll back").unwrap()),
            },
            MarkerMutation::SetRange {
                timeline_id: TIMELINE,
                marker_id: MISSING_MARKER,
                marked_range: range(1, 0, edit_rate()),
            },
        ],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotFound);
    assert_eq!(project, before);

    let duplicate = Marker::new(
        OBJECT_MARKER,
        MarkerOwner::Timeline,
        range(0, 0, edit_rate()),
    )
    .unwrap();
    assert_eq!(
        apply_marker_mutation_batch(
            &mut project,
            1,
            &[MarkerMutation::Create {
                timeline_id: TIMELINE,
                marker: duplicate,
            }],
        )
        .unwrap_err()
        .category(),
        ErrorCategory::Conflict
    );
    assert_eq!(project, before);

    assert_eq!(
        apply_marker_mutation_batch(
            &mut project,
            1,
            &[MarkerMutation::Remove {
                timeline_id: TIMELINE,
                marker_id: MISSING_MARKER,
            }],
        )
        .unwrap_err()
        .category(),
        ErrorCategory::NotFound
    );
    assert_eq!(project, before);
}

#[test]
fn markers_keep_direct_identity_visible_semantics_and_deterministic_metadata() {
    let project = annotated_project();
    let timeline = project.timeline(TIMELINE).unwrap();
    let marker = timeline.marker(OBJECT_MARKER).unwrap();

    assert_eq!(
        marker.owner(),
        MarkerOwner::Object(EditorialObjectId::Clip(B))
    );
    assert_eq!(marker.marked_range(), range(1, 2, edit_rate()));
    assert_eq!(marker.label().unwrap().as_str(), "performance");
    assert_eq!(marker.flag(), Some(MarkerFlag::Green));
    assert_eq!(marker.note().unwrap().as_str(), "hold this beat");
    assert_eq!(
        marker
            .metadata()
            .get(&MetadataKey::new("superi.intent").unwrap()),
        Some(&MetadataValue::Map(metadata(&[(
            "department",
            MetadataValue::Text("editorial".to_owned()),
        )])))
    );
    assert_eq!(
        timeline.markers().map(Marker::id).collect::<Vec<_>>(),
        [
            OBJECT_MARKER,
            TIMELINE_MARKER,
            OVERSCAN_MARKER,
            TRACK_MARKER
        ]
    );
    assert_eq!(
        timeline.resolved_marker_range(OBJECT_MARKER).unwrap(),
        Some(range(9, 2, edit_rate()))
    );
    assert_eq!(
        timeline.resolved_marker_range(OVERSCAN_MARKER).unwrap(),
        None
    );

    let keys = timeline
        .metadata(MetadataOwner::Timeline)
        .unwrap()
        .keys()
        .map(MetadataKey::as_str)
        .collect::<Vec<_>>();
    assert_eq!(keys, ["sequence"]);
}

#[test]
fn annotations_publish_atomically_and_follow_surviving_owner_identity() {
    let mut project = annotated_project();
    project
        .edit(1, |draft| {
            let timeline = draft.timeline_mut(TIMELINE)?;
            timeline.set_track_targeted(TRACK, true)?;
            timeline.set_track_sync_locked(TRACK, true)?;
            timeline.link_clips([A, B])?;
            timeline.group_clips([A, B])?;
            let marker = timeline.marker_mut(OBJECT_MARKER)?;
            marker.set_label(Some(MarkerLabel::new("revised performance").unwrap()));
            marker.set_flag(Some(MarkerFlag::Yellow));
            marker.set_note(Some(MarkerNote::new("preserve revised intent").unwrap()));
            Ok(())
        })
        .unwrap();

    let before = project.clone();
    let error = project
        .edit(2, |draft| {
            draft.timeline_mut(TIMELINE)?.upsert_marker(Marker::new(
                MarkerId::from_raw(99),
                MarkerOwner::Timeline,
                range(0, 1, Timebase::integer(25).unwrap()),
            )?)?;
            Ok(())
        })
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(project, before);

    apply_edit_batch(
        &mut project,
        2,
        &[EditOperation::insert(
            TIMELINE,
            TRACK,
            RationalTime::zero(edit_rate()),
            clip(INSERTED, 100, 99, 2),
            [],
        )],
    )
    .unwrap();

    let timeline = project.timeline(TIMELINE).unwrap();
    assert_eq!(
        timeline.resolved_marker_range(OBJECT_MARKER).unwrap(),
        Some(range(11, 2, edit_rate()))
    );
    assert_eq!(
        timeline.marker(OBJECT_MARKER).unwrap().marked_range(),
        range(1, 2, edit_rate())
    );
    assert_eq!(
        timeline
            .marker(OBJECT_MARKER)
            .unwrap()
            .label()
            .unwrap()
            .as_str(),
        "revised performance"
    );
    assert_eq!(
        timeline.marker(OBJECT_MARKER).unwrap().flag(),
        Some(MarkerFlag::Yellow)
    );
    assert_eq!(
        timeline
            .marker(OBJECT_MARKER)
            .unwrap()
            .note()
            .unwrap()
            .as_str(),
        "preserve revised intent"
    );
    let moved = timeline
        .track(TRACK)
        .unwrap()
        .item(EditorialObjectId::Clip(B))
        .unwrap()
        .as_clip()
        .unwrap();
    assert_eq!(moved.source_range(), range(16, 8, source_rate()));
    assert_eq!(moved.record_range(), range(10, 4, edit_rate()));
    assert!(timeline.edit_state().selected_objects().next().is_none());
    assert!(timeline.edit_state().track_state(TRACK).unwrap().targeted());
    assert!(timeline
        .edit_state()
        .track_state(TRACK)
        .unwrap()
        .sync_locked());
    assert_eq!(
        timeline
            .edit_state()
            .link_for(A)
            .unwrap()
            .members()
            .collect::<Vec<_>>(),
        [A, B]
    );
    assert_eq!(
        timeline
            .edit_state()
            .group_for(A)
            .unwrap()
            .members()
            .collect::<Vec<_>>(),
        [A, B]
    );
}

#[test]
fn deleting_an_owner_reconciles_only_its_annotations() {
    let mut project = annotated_project();
    apply_edit_batch(
        &mut project,
        1,
        &[EditOperation::extract(
            TIMELINE,
            TRACK,
            range(8, 4, edit_rate()),
            [],
        )],
    )
    .unwrap();

    let timeline = project.timeline(TIMELINE).unwrap();
    assert!(timeline.marker(OBJECT_MARKER).is_none());
    assert!(timeline.marker(OVERSCAN_MARKER).is_none());
    assert!(timeline.marker(TIMELINE_MARKER).is_some());
    assert!(timeline.marker(TRACK_MARKER).is_some());
    assert!(timeline
        .metadata(MetadataOwner::Object(EditorialObjectId::Clip(B)))
        .is_none());
    assert!(timeline.metadata(MetadataOwner::Timeline).is_some());
    assert!(timeline.metadata(MetadataOwner::Track(TRACK)).is_some());
}

#[test]
fn snapping_is_exact_filterable_persistent_and_deterministic() {
    let mut project = annotated_project();
    let timeline = project.timeline(TIMELINE).unwrap();

    let marker_hit = timeline
        .snap(&SnapRequest::new(
            RationalTime::new(9, edit_rate()),
            Duration::new(1, edit_rate()).unwrap(),
        ))
        .unwrap()
        .unwrap();
    assert_eq!(marker_hit.target(), SnapTarget::MarkerStart(OBJECT_MARKER));
    assert_eq!(marker_hit.time(), RationalTime::new(9, edit_rate()));

    let edge_hit = timeline
        .snap(
            &SnapRequest::new(
                RationalTime::new(17, Timebase::integer(48).unwrap()),
                Duration::new(2, Timebase::integer(48).unwrap()).unwrap(),
            )
            .with_target_kinds([SnapTargetKind::ItemStart]),
        )
        .unwrap()
        .unwrap();
    assert_eq!(
        edge_hit.target(),
        SnapTarget::ItemStart(EditorialObjectId::Clip(B))
    );
    assert_eq!(
        edge_hit.time(),
        RationalTime::new(16, Timebase::integer(48).unwrap())
    );

    let tie = timeline
        .snap(&SnapRequest::new(
            RationalTime::new(7, edit_rate()),
            Duration::new(1, edit_rate()).unwrap(),
        ))
        .unwrap()
        .unwrap();
    assert_eq!(
        tie.target(),
        SnapTarget::ItemStart(EditorialObjectId::Clip(B))
    );

    assert_eq!(
        timeline
            .snap(
                &SnapRequest::new(
                    RationalTime::new(9, edit_rate()),
                    Duration::new(0, edit_rate()).unwrap(),
                )
                .excluding_marker(OBJECT_MARKER),
            )
            .unwrap(),
        None
    );
    assert_eq!(
        timeline
            .snap(
                &SnapRequest::new(
                    RationalTime::new(8, edit_rate()),
                    Duration::new(0, edit_rate()).unwrap(),
                )
                .with_target_kinds([SnapTargetKind::ItemStart])
                .excluding_object(EditorialObjectId::Clip(B)),
            )
            .unwrap(),
        None
    );
    assert_eq!(
        timeline
            .snap(
                &SnapRequest::new(
                    RationalTime::new(10, edit_rate()),
                    Duration::new(0, edit_rate()).unwrap(),
                )
                .with_playhead(RationalTime::new(10, edit_rate()))
                .with_target_kinds([SnapTargetKind::Playhead]),
            )
            .unwrap()
            .unwrap()
            .target(),
        SnapTarget::Playhead
    );

    let inexact = timeline
        .snap(
            &SnapRequest::new(
                RationalTime::new(4, Timebase::integer(10).unwrap()),
                Duration::new(1, Timebase::integer(10).unwrap()).unwrap(),
            )
            .with_target_kinds([SnapTargetKind::ItemStart, SnapTargetKind::MarkerStart]),
        )
        .unwrap();
    assert_eq!(inexact, None);

    project
        .edit(1, |draft| {
            draft.timeline_mut(TIMELINE)?.set_snapping_enabled(false);
            Ok(())
        })
        .unwrap();
    assert!(!project.timeline(TIMELINE).unwrap().snapping_enabled());
    assert_eq!(
        project
            .timeline(TIMELINE)
            .unwrap()
            .snap(&SnapRequest::new(
                RationalTime::new(9, edit_rate()),
                Duration::new(1, edit_rate()).unwrap(),
            ))
            .unwrap(),
        None
    );
}

#[test]
fn metadata_keys_and_visible_text_reject_ambiguous_values() {
    assert!(MetadataKey::new(" superi.bad").is_err());
    assert!(MetadataKey::new("superi bad").is_err());
    assert!(MarkerLabel::new("  ").is_err());
    assert!(MarkerNote::new("\0hidden").is_err());
    assert_eq!(
        TimelineMetadata::from_entries([
            (MetadataKey::new("b").unwrap(), MetadataValue::Boolean(true)),
            (MetadataKey::new("a").unwrap(), MetadataValue::Signed(-1)),
            (
                MetadataKey::new("c").unwrap(),
                MetadataValue::Float(FiniteF64::new(1.25).unwrap()),
            ),
        ])
        .keys()
        .map(MetadataKey::as_str)
        .collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    let codes = MarkerFlag::ALL.map(MarkerFlag::code);
    assert_eq!(codes[0], "red");
    assert_eq!(codes[10], "white");
    assert_eq!(MarkerFlag::from_code("magenta"), Some(MarkerFlag::Magenta));
    assert_eq!(MarkerFlag::from_code("unknown"), None);
}

#[test]
fn marker_identity_is_unique_across_the_complete_project() {
    let timeline = |id| {
        let mut timeline = Timeline::new(
            id,
            "empty",
            edit_rate(),
            RationalTime::zero(edit_rate()),
            Vec::new(),
        );
        timeline
            .upsert_marker(
                Marker::new(
                    OBJECT_MARKER,
                    MarkerOwner::Timeline,
                    range(0, 0, edit_rate()),
                )
                .unwrap(),
            )
            .unwrap();
        timeline
    };
    let error = EditorialProject::new(
        ProjectId::from_raw(101),
        "duplicate markers",
        std::iter::empty(),
        [
            timeline(TimelineId::from_raw(20)),
            timeline(TimelineId::from_raw(21)),
        ],
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
}
