use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use superi_core::diagnostics::FiniteF64;
use superi_core::error::ErrorCategory;
use superi_core::ids::{
    BinId, ClipId, GapId, MarkerId, MediaId, ProjectId, SmartCollectionId, TimelineId, TrackId,
};
use superi_core::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION;
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_graph::serialize::{deserialize_graph, serialize_graph};
use superi_timeline::compile::compile_timeline;
use superi_timeline::edit_state::{SelectionExpansion, SelectionUpdate};
use superi_timeline::ids::MulticamAngleId;
use superi_timeline::markers::{
    Marker, MarkerFlag, MarkerLabel, MarkerNote, MarkerOwner, MetadataKey, MetadataOwner,
    MetadataValue, TimelineMetadata,
};
use superi_timeline::media::{
    MediaBin, MediaPredicate, RelinkDecision, SmartCollection, SmartCollectionMatch,
};
use superi_timeline::model::{
    Clip, ClipSource, EditorialObjectId, EditorialProject, Gap, LinkedMediaReference, Timeline,
    Track, TrackItem, TrackSemantics, VideoCompositing, VideoTrackSemantics,
};
use superi_timeline::multicam::{
    resolve_multicam_frame, MulticamAngle, MulticamAudioPolicy, MulticamClip, MulticamSource,
    MulticamSyncMethod,
};
use superi_timeline::retime::{ClipTimeMap, PlaybackRate};
use superi_timeline::serialize::{
    deserialize_timeline_state, serialize_timeline_state, TIMELINE_STATE_FORMAT_REVISION,
};

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

fn previous_payload_json(project: &EditorialProject) -> String {
    let current = String::from_utf8(serialize_timeline_state(project).unwrap()).unwrap();
    let payload_start = current.find("\"payload\":").unwrap() + "\"payload\":".len();
    let payload = &current[payload_start..current.len() - 1];
    payload
        .replace(",\"height\":72", "")
        .replace(",\"locked\":false", "")
        .replace(",\"muted\":false,\"solo\":false,\"enabled\":true", "")
}

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

fn complete_project() -> EditorialProject {
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
        RationalTime::new(86_400, record_rate()),
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
    let mut project = EditorialProject::new(
        ProjectId::from_raw(50),
        "complete timeline state",
        [
            LinkedMediaReference::with_fingerprint(
                CAMERA_A,
                "camera a",
                "urn:camera:a",
                Some(range(0, 400, media_rate())),
                "fingerprint-a",
            )
            .unwrap(),
            LinkedMediaReference::with_fingerprint(
                CAMERA_B,
                "camera b",
                "urn:camera:b",
                Some(range(0, 400, media_rate())),
                "fingerprint-b",
            )
            .unwrap(),
        ],
        [source, target],
    )
    .unwrap();

    project
        .edit(0, |draft| {
            let media_b = draft.media_reference_mut(CAMERA_B)?;
            assert_eq!(
                media_b.consider_relink("urn:camera:b:replacement", "wrong-fingerprint")?,
                RelinkDecision::RejectedFingerprintMismatch
            );
            let mut media_metadata = TimelineMetadata::new();
            media_metadata.insert(
                MetadataKey::new("camera.iso")?,
                MetadataValue::Unsigned(800),
            );
            media_metadata.insert(
                MetadataKey::new("camera.gain")?,
                MetadataValue::Float(FiniteF64::new(1.25)?),
            );
            *media_b.metadata_mut() = media_metadata;

            let library = draft.media_library_mut();
            let mut bin = MediaBin::new(BinId::from_raw(60), "interview", None)?;
            bin.add_media(CAMERA_A);
            bin.add_media(CAMERA_B);
            library.upsert_bin(bin);
            library.upsert_smart_collection(SmartCollection::new(
                SmartCollectionId::from_raw(61),
                "online cameras",
                SmartCollectionMatch::Any,
                [
                    MediaPredicate::NameContains("camera".to_owned()),
                    MediaPredicate::RelinkStatus(
                        superi_timeline::media::RelinkStatus::FingerprintMismatch,
                    ),
                ],
            )?);

            let mut angle_metadata = TimelineMetadata::new();
            angle_metadata.insert(
                MetadataKey::new("camera.serial")?,
                MetadataValue::Text("A-001".to_owned()),
            );
            let mut angle_a = MulticamAngle::new(ANGLE_A, "wide", "A", [SOURCE_CLIP_A])?;
            angle_a.set_metadata(angle_metadata);
            let angle_b = MulticamAngle::new(ANGLE_B, "close", "B", [SOURCE_CLIP_B])?;
            let source = draft.timeline_mut(SOURCE)?;
            source.set_multicam_source(MulticamSource::new(
                MulticamSyncMethod::ClipMarker("sync".to_owned()),
                [angle_a, angle_b],
            )?)?;
            source.link_clips([SOURCE_CLIP_A, SOURCE_CLIP_B])?;
            source.group_clips([SOURCE_CLIP_A, SOURCE_CLIP_B])?;
            source.set_track_targeted(SOURCE_TRACK_A, true)?;
            source.set_track_sync_locked(SOURCE_TRACK_B, false)?;
            source.update_selection(
                [EditorialObjectId::Clip(SOURCE_CLIP_A)],
                SelectionUpdate::Replace,
                SelectionExpansion::Direct,
            )?;

            let target = draft.timeline_mut(TARGET)?;
            target.set_snapping_enabled(false);
            let mut metadata = TimelineMetadata::new();
            metadata.insert(
                MetadataKey::new("edit.intent")?,
                MetadataValue::List(vec![
                    MetadataValue::Text("protect interview".to_owned()),
                    MetadataValue::Range(range(10, 12, record_rate())),
                ]),
            );
            target.set_metadata(MetadataOwner::Timeline, metadata)?;
            let mut marker = Marker::new(
                MarkerId::from_raw(70),
                MarkerOwner::Object(EditorialObjectId::Clip(TARGET_CLIP)),
                range(2, 3, record_rate()),
            )?;
            marker.set_label(Some(MarkerLabel::new("preferred cut")?));
            marker.set_flag(Some(MarkerFlag::Cyan));
            marker.set_note(Some(MarkerNote::new("retain this alternate")?));
            target.upsert_marker(marker)?;
            let mut multicam = MulticamClip::new(
                TARGET_CLIP,
                range(0, 24, record_rate()),
                ANGLE_A,
                MulticamAudioPolicy::Fixed(ANGLE_A),
            )?;
            multicam.switch_range(range(12, 8, record_rate()), ANGLE_B)?;
            target.upsert_multicam_clip(multicam)?;
            Ok(())
        })
        .unwrap();
    project
}

#[test]
fn current_document_is_canonical_and_round_trips_complete_editable_state() {
    let project = complete_project();
    let first = serialize_timeline_state(&project).unwrap();
    let second = serialize_timeline_state(&project).unwrap();
    assert_eq!(first, second);

    let document: Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(document["format"], "superi.timeline");
    assert_eq!(document["format_revision"], TIMELINE_STATE_FORMAT_REVISION);
    assert_eq!(
        document["primitive_schema_revision"],
        STABLE_PRIMITIVE_SCHEMA_REVISION
    );
    assert_eq!(document["payload_sha256"].as_str().unwrap().len(), 64);

    let loaded = deserialize_timeline_state(&first).unwrap();
    assert_eq!(
        loaded.source_format_revision(),
        TIMELINE_STATE_FORMAT_REVISION
    );
    assert!(!loaded.was_migrated());
    assert_eq!(loaded.project(), &project);
    assert_eq!(loaded.canonical_document(), first);
    assert_eq!(serialize_timeline_state(loaded.project()).unwrap(), first);
}

#[test]
fn supported_legacy_state_migrates_without_losing_edit_or_multicam_intent() {
    let project = complete_project();
    let previous_payload = previous_payload_json(&project);
    let digest = format!("{:x}", Sha256::digest(previous_payload.as_bytes()));
    let previous = format!(
        "{{\"format\":\"superi.timeline\",\"format_revision\":1,\"primitive_schema_revision\":{},\"payload_sha256\":\"{}\",\"payload\":{}}}",
        STABLE_PRIMITIVE_SCHEMA_REVISION, digest, previous_payload
    );
    let previous = deserialize_timeline_state(previous.as_bytes()).unwrap();
    assert_eq!(previous.source_format_revision(), 1);
    assert!(previous.was_migrated());
    assert_eq!(previous.project(), &project);

    let legacy = format!(
        "{{\"format\":\"superi.timeline\",\"format_revision\":0,\"timeline_state\":{previous_payload}}}"
    );

    let loaded = deserialize_timeline_state(legacy.as_bytes()).unwrap();
    assert_eq!(loaded.source_format_revision(), 0);
    assert!(loaded.was_migrated());
    assert_eq!(loaded.project(), &project);
    assert_eq!(
        loaded.canonical_document(),
        serialize_timeline_state(&project).unwrap()
    );

    let resolved = resolve_multicam_frame(
        loaded.project(),
        TARGET,
        TARGET_CLIP,
        RationalTime::new(16, record_rate()),
    )
    .unwrap();
    assert_eq!(resolved.angle_id(), ANGLE_B);
    assert_eq!(resolved.source_clip_id(), SOURCE_CLIP_B);

    let original_revision = loaded.project().revision();
    let mut editable = loaded.into_project();
    editable
        .edit(original_revision, |draft| {
            draft
                .timeline_mut(TARGET)?
                .set_name("reshaped after recovery");
            Ok(())
        })
        .unwrap();
    assert_eq!(editable.revision(), original_revision + 1);
    assert_eq!(
        editable.timeline(TARGET).unwrap().name(),
        "reshaped after recovery"
    );
}

#[test]
fn interrupted_tampered_future_and_semantically_invalid_state_is_rejected() {
    let project = complete_project();
    let current = serialize_timeline_state(&project).unwrap();

    let truncated = deserialize_timeline_state(&current[..current.len() / 2]).unwrap_err();
    assert_eq!(truncated.category(), ErrorCategory::CorruptData);

    let mut tampered: Value = serde_json::from_slice(&current).unwrap();
    tampered["payload"]["name"] = json!("tampered without a new checksum");
    let tampered = deserialize_timeline_state(&serde_json::to_vec(&tampered).unwrap()).unwrap_err();
    assert_eq!(tampered.category(), ErrorCategory::CorruptData);

    let mut unknown: Value = serde_json::from_slice(&current).unwrap();
    unknown["unexpected"] = json!(true);
    let unknown = deserialize_timeline_state(&serde_json::to_vec(&unknown).unwrap()).unwrap_err();
    assert_eq!(unknown.category(), ErrorCategory::CorruptData);

    let mut future: Value = serde_json::from_slice(&current).unwrap();
    future["format_revision"] = json!(TIMELINE_STATE_FORMAT_REVISION + 1);
    let future = deserialize_timeline_state(&serde_json::to_vec(&future).unwrap()).unwrap_err();
    assert_eq!(future.category(), ErrorCategory::Unsupported);

    let previous_payload = previous_payload_json(&project);
    let mut legacy_payload: Value = serde_json::from_str(&previous_payload).unwrap();
    legacy_payload["timelines"][1]["multicam_clips"][0]["clip_id"] =
        json!(ClipId::from_raw(0x00ff_ffff).to_string());
    let invalid = serde_json::to_vec(&json!({
        "format": "superi.timeline",
        "format_revision": 0,
        "timeline_state": legacy_payload,
    }))
    .unwrap();
    let invalid = deserialize_timeline_state(&invalid).unwrap_err();
    assert!(matches!(
        invalid.category(),
        ErrorCategory::InvalidInput | ErrorCategory::NotFound
    ));
}

#[test]
fn compiled_multicam_graph_round_trips_through_the_public_graph_codec() {
    let project = complete_project();
    let compilation = compile_timeline(&project, TARGET).unwrap();
    let encoded = serialize_graph(&compilation.snapshot()).unwrap();
    let loaded = deserialize_graph(&encoded).unwrap();

    assert_eq!(loaded.graph(), compilation.graph());
    assert_eq!(loaded.canonical_document(), encoded);

    let unknown_field =
        serde_json::from_value::<superi_timeline::compile::TimelineGraphValue>(json!({
            "kind": "text",
            "value": "strict value",
            "unexpected": true,
        }));
    assert!(unknown_field.is_err());
    let unknown_tag =
        serde_json::from_value::<superi_timeline::compile::TimelineGraphValue>(json!({
            "kind": "future_value",
            "value": "strict value",
        }));
    assert!(unknown_tag.is_err());
}
