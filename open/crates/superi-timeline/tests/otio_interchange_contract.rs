use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use superi_core::time::{RationalTime, TimeRange, Timebase};
use superi_timeline::markers::{MetadataKey, MetadataOwner, MetadataValue, TimelineMetadata};
use superi_timeline::model::{ClipSource, EditorialObjectId, TrackItem, TrackKind, TrackSemantics};
use superi_timeline::otio::{
    export_otio, import_otio, OtioDiagnosticSeverity, OtioImportOptions, OtioSchemaTarget,
    UNSUPPORTED_CONSTRUCT_CODE,
};
use superi_timeline::retime::{ClipTimeMap, PlaybackRate};

const CANONICAL_SLICE: &str = "canonical-slice.otio";
const COVERAGE: &str = "interchange-coverage.otio";

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/timeline/otio-interchange/v1")
}

fn fixture(name: &str) -> Vec<u8> {
    fs::read(fixture_root().join(name)).expect("OTIO fixture must exist")
}

#[test]
fn canonical_otio_imports_into_the_native_editorial_model() {
    let document = import_otio(&fixture(CANONICAL_SLICE), OtioImportOptions::default())
        .expect("canonical OTIO must import");
    let timeline = document
        .project()
        .timeline(document.root_timeline_id())
        .expect("root timeline must exist");

    assert_eq!(timeline.name(), "canonical");
    assert_eq!(timeline.edit_rate(), Timebase::integer(24).unwrap());
    assert_eq!(timeline.duration().unwrap().value(), 48);
    assert_eq!(timeline.tracks().len(), 1);
    assert_eq!(timeline.tracks()[0].kind(), TrackKind::Video);
    assert_eq!(timeline.tracks()[0].name(), "V1");
    assert_eq!(timeline.tracks()[0].items().len(), 1);

    let clip = timeline.tracks()[0].items()[0]
        .as_clip()
        .expect("fixture item must be a native clip");
    assert_eq!(clip.name(), "clip-1");
    assert_eq!(clip.source_range().start().value(), 24);
    assert_eq!(clip.source_range().duration().value(), 48);
    let ClipSource::Media(media_id) = clip.source() else {
        panic!("canonical clip must link native media");
    };
    let media = document
        .project()
        .media_reference(media_id)
        .expect("linked media must exist");
    assert_eq!(media.name(), "media.slice.video-cfr.v1");
    assert_eq!(media.target(), "../../../slice/video-cfr/v1/input.webm");
    assert!(document.diagnostics().is_empty());
}

#[test]
fn coverage_otio_maps_nesting_retime_markers_and_stable_diagnostics() {
    let document = import_otio(&fixture(COVERAGE), OtioImportOptions::default())
        .expect("coverage OTIO must import");
    let root = document
        .project()
        .timeline(document.root_timeline_id())
        .expect("root timeline must exist");

    assert_eq!(root.duration().unwrap().value(), 120);
    assert_eq!(root.tracks().len(), 1);
    assert_eq!(root.tracks()[0].items().len(), 5);
    assert_eq!(root.markers().count(), 2);
    assert_eq!(document.project().timelines().count(), 2);

    let root_items = root.tracks()[0].items();
    let clip_a = root_items[0].as_clip().expect("first item must be clip-a");
    assert_eq!(clip_a.record_range().start().value(), 0);
    assert_eq!(clip_a.record_range().duration().value(), 48);
    assert!(matches!(root_items[1], TrackItem::Transition(_)));
    let clip_b = root_items[2].as_clip().expect("third item must be clip-b");
    assert_eq!(clip_b.record_range().start().value(), 48);
    assert_eq!(clip_b.time_map().segments()[0].rate().numerator(), 2);
    assert_eq!(clip_b.time_map().segments()[0].rate().denominator(), 1);
    assert!(matches!(root_items[3], TrackItem::Gap(_)));

    let nested_clip = root_items[4]
        .as_clip()
        .expect("OTIO stack must be a native nested clip");
    let ClipSource::Timeline(nested_id) = nested_clip.source() else {
        panic!("nested stack must link a native child timeline");
    };
    let nested = document
        .project()
        .timeline(nested_id)
        .expect("nested timeline must exist");
    assert_eq!(nested.name(), "nested-sequence");
    assert_eq!(nested.duration().unwrap().value(), 48);
    let slow_clip = nested.tracks()[0].items()[0]
        .as_clip()
        .expect("nested first item must be a clip");
    assert_eq!(slow_clip.time_map().segments()[0].rate().numerator(), 1);
    assert_eq!(slow_clip.time_map().segments()[0].rate().denominator(), 2);

    let diagnostics = document.diagnostics();
    assert_eq!(diagnostics.len(), 2);
    assert!(diagnostics.iter().all(|diagnostic| {
        diagnostic.code() == UNSUPPORTED_CONSTRUCT_CODE
            && diagnostic.severity() == OtioDiagnosticSeverity::Warning
    }));
    assert_eq!(
        diagnostics[0].json_pointer(),
        "/tracks/children/0/children/4/children/0/children/2/effects/0"
    );
    assert_eq!(diagnostics[0].object_id(), Some("effect.freeze-frame"));
    assert_eq!(diagnostics[0].otio_schema(), "FreezeFrame.1");
    assert_eq!(
        diagnostics[1].json_pointer(),
        "/tracks/children/0/children/4/children/0/children/2/effects/1"
    );
    assert_eq!(diagnostics[1].object_id(), Some("effect.lens-warp"));
    assert_eq!(diagnostics[1].otio_schema(), "Effect.1");
}

#[test]
fn native_edits_export_deterministically_without_dropping_opaque_otio() {
    let mut document = import_otio(&fixture(COVERAGE), OtioImportOptions::default())
        .expect("coverage OTIO must import");
    let root_id = document.root_timeline_id();
    let root = document.project().timeline(root_id).unwrap();
    let track_id = root.tracks()[0].id();
    let clip_id = root.tracks()[0].items()[0].id();
    let sequence_marker_id = root
        .markers()
        .find(|marker| {
            marker
                .label()
                .is_some_and(|label| label.as_str() == "sequence-in")
        })
        .unwrap()
        .id();
    assert!(matches!(clip_id, EditorialObjectId::Clip(_)));

    let revision = document.project().revision();
    document
        .project_mut()
        .edit(revision, |draft| {
            let item = draft
                .timeline_mut(root_id)?
                .track_mut(track_id)?
                .item_mut(clip_id)?;
            item.as_clip_mut().unwrap().set_name("clip-a-edited");
            let mut metadata = TimelineMetadata::new();
            metadata.insert(
                MetadataKey::new("review.state")?,
                MetadataValue::Text("approved".into()),
            );
            draft
                .timeline_mut(root_id)?
                .set_metadata(MetadataOwner::Marker(sequence_marker_id), metadata)?;
            Ok(())
        })
        .expect("ordinary native edit must commit");

    let first = export_otio(&document, OtioSchemaTarget::OtioCore0181)
        .expect("edited document must export");
    let second = export_otio(&document, OtioSchemaTarget::OtioCore0181)
        .expect("repeated export must succeed");
    assert_eq!(first, second, "export must be byte deterministic");

    let json: Value = serde_json::from_slice(&first).expect("export must be JSON");
    assert_eq!(
        json["tracks"]["children"][0]["children"][0]["name"],
        "clip-a-edited"
    );
    assert_eq!(
        json["tracks"]["children"][0]["markers"][0]["metadata"]["review.state"],
        "approved"
    );
    assert_eq!(
        json.pointer("/tracks/children/0/children/4/children/0/children/2/effects/0/OTIO_SCHEMA"),
        Some(&Value::String("FreezeFrame.1".into()))
    );
    assert_eq!(
        json.pointer("/tracks/children/0/children/4/children/0/children/2/effects/1/metadata/superi/parameters/strength"),
        Some(&serde_json::json!(0.25))
    );

    let reimported =
        import_otio(&first, OtioImportOptions::default()).expect("exported OTIO must reimport");
    let root = reimported
        .project()
        .timeline(reimported.root_timeline_id())
        .unwrap();
    assert_eq!(
        root.tracks()[0].items()[0].as_clip().unwrap().name(),
        "clip-a-edited"
    );
    assert_eq!(root.duration().unwrap().value(), 120);
    assert_eq!(reimported.diagnostics().len(), 2);
}

#[test]
fn duplicate_otio_identity_is_rejected_instead_of_aliased() {
    let mut json: Value = serde_json::from_slice(&fixture(CANONICAL_SLICE)).unwrap();
    json["tracks"]["children"][0]["children"][0]["metadata"]["superi"]["id"] =
        Value::String("track.canonical.v1".into());

    let error = import_otio(
        &serde_json::to_vec(&json).unwrap(),
        OtioImportOptions::default(),
    )
    .expect_err("duplicate source identity must fail");
    assert!(error.to_string().contains("duplicate OTIO identity"));
}

#[test]
fn native_linear_retime_edits_replace_the_otio_scalar() {
    let mut document = import_otio(&fixture(COVERAGE), OtioImportOptions::default())
        .expect("coverage OTIO must import");
    let root_id = document.root_timeline_id();
    let root = document.project().timeline(root_id).unwrap();
    let track_id = root.tracks()[0].id();
    let clip_id = root.tracks()[0].items()[2].id();
    let clip = root.tracks()[0].items()[2].as_clip().unwrap();
    let record_duration = clip.record_range().duration();
    let source_start = clip.source_range().start();

    let revision = document.project().revision();
    document
        .project_mut()
        .edit(revision, |draft| {
            let clip = draft
                .timeline_mut(root_id)?
                .track_mut(track_id)?
                .item_mut(clip_id)?
                .as_clip_mut()
                .unwrap();
            clip.set_time_map(ClipTimeMap::speed(
                record_duration,
                source_start,
                PlaybackRate::new(3, 1)?,
            )?)
        })
        .expect("native retime edit must commit");

    let encoded = export_otio(&document, OtioSchemaTarget::OtioCore0181).unwrap();
    let json: Value = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(
        json.pointer("/tracks/children/0/children/2/effects/0/time_scalar"),
        Some(&serde_json::json!(3.0))
    );
}

#[test]
fn native_sequence_reshape_rebuilds_the_otio_hierarchy() {
    let mut document = import_otio(&fixture(COVERAGE), OtioImportOptions::default())
        .expect("coverage OTIO must import");
    let root_id = document.root_timeline_id();
    let root = document.project().timeline(root_id).unwrap();
    let track_id = root.tracks()[0].id();
    let nested_duration = root.tracks()[0].items()[4]
        .as_clip()
        .unwrap()
        .record_range()
        .duration();

    let revision = document.project().revision();
    document
        .project_mut()
        .edit(revision, |draft| {
            let track = draft.timeline_mut(root_id)?.track_mut(track_id)?;
            let mut items = track.items().to_vec();
            items.retain(|item| {
                !matches!(item, TrackItem::Transition(_))
                    && !matches!(item, TrackItem::Gap(gap) if gap.name() == "gap-12")
            });
            items[2]
                .as_clip_mut()
                .unwrap()
                .set_record_range(TimeRange::new(
                    RationalTime::new(84, nested_duration.timebase()),
                    nested_duration,
                )?)?;
            track.replace_items(items);
            Ok(())
        })
        .expect("native sequence reshape must commit");

    let encoded = export_otio(&document, OtioSchemaTarget::OtioCore0181).unwrap();
    let json: Value = serde_json::from_slice(&encoded).unwrap();
    let children = json["tracks"]["children"][0]["children"]
        .as_array()
        .unwrap();
    assert_eq!(children.len(), 3);
    assert_eq!(children[0]["OTIO_SCHEMA"], "Clip.2");
    assert_eq!(children[1]["OTIO_SCHEMA"], "Clip.2");
    assert_eq!(children[2]["OTIO_SCHEMA"], "Stack.1");

    let reimported = import_otio(&encoded, OtioImportOptions::default()).unwrap();
    let root = reimported
        .project()
        .timeline(reimported.root_timeline_id())
        .unwrap();
    assert_eq!(root.duration().unwrap().value(), 108);
    assert_eq!(root.tracks()[0].items().len(), 3);
    assert_eq!(reimported.diagnostics().len(), 2);
}

#[test]
fn generic_audio_tracks_receive_explicit_native_defaults() {
    let mut json: Value = serde_json::from_slice(&fixture(CANONICAL_SLICE)).unwrap();
    json["tracks"]["children"][0]["kind"] = Value::String("Audio".into());
    let document = import_otio(
        &serde_json::to_vec(&json).unwrap(),
        OtioImportOptions::default(),
    )
    .expect("generic OTIO audio track must import");
    let root = document
        .project()
        .timeline(document.root_timeline_id())
        .unwrap();
    let TrackSemantics::Audio(audio) = root.tracks()[0].semantics() else {
        panic!("OTIO Audio kind must map to native audio semantics");
    };
    assert_eq!(audio.sample_rate(), 48_000);
    assert_eq!(audio.channel_layout().positions().len(), 2);
    assert_eq!(
        root.tracks()[0].items()[0]
            .as_clip()
            .unwrap()
            .record_range()
            .duration()
            .value(),
        96_000
    );
    assert_eq!(root.duration().unwrap().value(), 48);
}

#[test]
fn inexact_record_coordinates_are_rejected_with_their_json_pointer() {
    let mut json: Value = serde_json::from_slice(&fixture(CANONICAL_SLICE)).unwrap();
    json["tracks"]["children"][0]["children"][0]["source_range"]["duration"]["value"] =
        serde_json::json!(1.5);
    let error = import_otio(
        &serde_json::to_vec(&json).unwrap(),
        OtioImportOptions::default(),
    )
    .expect_err("fractional frame must not be silently rounded");
    assert!(error
        .to_string()
        .contains("not exactly representable on the native target clock"));
    assert!(error
        .to_string()
        .contains("/tracks/children/0/children/0/source_range/duration"));
}

#[test]
fn conflicting_repeated_media_identity_is_rejected() {
    let mut json: Value = serde_json::from_slice(&fixture(CANONICAL_SLICE)).unwrap();
    let mut second = json["tracks"]["children"][0]["children"][0].clone();
    second["name"] = Value::String("clip-2".into());
    second["metadata"]["superi"]["id"] = Value::String("clip.canonical.2".into());
    second["effects"] = Value::Array(Vec::new());
    second["media_references"]["DEFAULT_MEDIA"]["target_url"] =
        Value::String("urn:superi:conflicting-media".into());
    json["tracks"]["children"][0]["children"]
        .as_array_mut()
        .unwrap()
        .push(second);

    let error = import_otio(
        &serde_json::to_vec(&json).unwrap(),
        OtioImportOptions::default(),
    )
    .expect_err("one OTIO media identity must not describe two resources");
    assert!(error
        .to_string()
        .contains("conflicting repeated OTIO media identity"));
}
