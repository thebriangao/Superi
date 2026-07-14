use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const CANONICAL_SLICE: &str = "canonical-slice.otio";
const COVERAGE: &str = "interchange-coverage.otio";
const EXPECTATIONS: &str = "expectations.json";

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/timeline/otio-interchange/v1")
}

fn load(name: &str) -> Value {
    let bytes = fs::read(fixture_root().join(name)).expect("canonical OTIO fixture must exist");
    serde_json::from_slice(&bytes).expect("canonical OTIO fixture must be valid JSON")
}

fn assert_time(value: &Value, expected_value: f64, expected_rate: f64) {
    assert_eq!(value["OTIO_SCHEMA"], "RationalTime.1");
    assert_eq!(value["value"], expected_value);
    assert_eq!(value["rate"], expected_rate);
}

fn assert_range(value: &Value, start: f64, duration: f64, rate: f64) {
    assert_eq!(value["OTIO_SCHEMA"], "TimeRange.1");
    assert_time(&value["start_time"], start, rate);
    assert_time(&value["duration"], duration, rate);
}

fn assert_id(value: &Value, expected: &str) {
    assert_eq!(value["metadata"]["superi"]["id"], expected);
}

#[test]
fn canonical_slice_preserves_the_first_editorial_state() {
    let timeline = load(CANONICAL_SLICE);

    assert_eq!(timeline["OTIO_SCHEMA"], "Timeline.1");
    assert_eq!(timeline["name"], "canonical");
    assert_id(&timeline, "timeline.canonical");
    assert_time(&timeline["global_start_time"], 0.0, 24.0);

    let tracks = timeline["tracks"]["children"]
        .as_array()
        .expect("timeline tracks must be an array");
    assert_eq!(tracks.len(), 1);
    let track = &tracks[0];
    assert_eq!(track["OTIO_SCHEMA"], "Track.1");
    assert_eq!(track["kind"], "Video");
    assert_eq!(track["name"], "V1");
    assert_id(track, "track.canonical.v1");

    let children = track["children"]
        .as_array()
        .expect("track children must be an array");
    assert_eq!(children.len(), 1);
    let clip = &children[0];
    assert_eq!(clip["OTIO_SCHEMA"], "Clip.2");
    assert_eq!(clip["name"], "clip-1");
    assert_id(clip, "clip.canonical.1");
    assert_range(&clip["source_range"], 24.0, 48.0, 24.0);

    let media = &clip["media_references"]["DEFAULT_MEDIA"];
    assert_eq!(media["OTIO_SCHEMA"], "ExternalReference.1");
    assert_id(media, "media.slice.video-cfr.v1");
    assert_range(&media["available_range"], 0.0, 96.0, 24.0);
    assert_eq!(
        media["target_url"],
        "../../../slice/video-cfr/v1/input.webm"
    );
    assert_eq!(
        media["metadata"]["superi"]["manifest_sha256"],
        "fc76adeced535ff05e6adb36c2549939618cfd0f73de7de5fa9d7f7f4301dc08"
    );

    let effects = clip["effects"]
        .as_array()
        .expect("clip effects must be an array");
    assert_eq!(effects.len(), 1);
    assert_eq!(effects[0]["OTIO_SCHEMA"], "Effect.1");
    assert_eq!(effects[0]["effect_name"], "superi.effect.transform");
    assert_id(&effects[0], "effect.canonical.transform");
    assert_eq!(
        effects[0]["metadata"]["superi"]["matrix"],
        serde_json::json!([-1, 0, 95, 0, 1, 0, 0, 0, 1])
    );
}

#[test]
fn interchange_coverage_preserves_identity_timing_and_relationships() {
    let timeline = load(COVERAGE);

    assert_eq!(timeline["OTIO_SCHEMA"], "Timeline.1");
    assert_id(&timeline, "timeline.coverage");
    assert_time(&timeline["global_start_time"], 0.0, 24.0);
    let track = &timeline["tracks"]["children"][0];
    assert_eq!(track["OTIO_SCHEMA"], "Track.1");
    assert_id(track, "track.coverage.v1");

    let markers = track["markers"]
        .as_array()
        .expect("track markers must be an array");
    assert_eq!(markers.len(), 1);
    assert_eq!(markers[0]["OTIO_SCHEMA"], "Marker.2");
    assert_id(&markers[0], "marker.sequence-in");
    assert_range(&markers[0]["marked_range"], 0.0, 0.0, 24.0);

    let children = track["children"]
        .as_array()
        .expect("track children must be an array");
    assert_eq!(children.len(), 5);

    let clip_a = &children[0];
    assert_eq!(clip_a["OTIO_SCHEMA"], "Clip.2");
    assert_id(clip_a, "clip.a");
    assert_range(&clip_a["source_range"], 24.0, 48.0, 24.0);
    assert_id(&clip_a["markers"][0], "marker.clip-a.select");
    assert_range(&clip_a["markers"][0]["marked_range"], 12.0, 1.0, 24.0);

    let transition = &children[1];
    assert_eq!(transition["OTIO_SCHEMA"], "Transition.1");
    assert_id(transition, "transition.a-b");
    assert_time(&transition["in_offset"], 6.0, 24.0);
    assert_time(&transition["out_offset"], 6.0, 24.0);
    assert_eq!(transition["metadata"]["superi"]["from_clip_id"], "clip.a");
    assert_eq!(
        transition["metadata"]["superi"]["to_clip_id"],
        "clip.b-fast"
    );

    let clip_b = &children[2];
    assert_id(clip_b, "clip.b-fast");
    assert_range(&clip_b["source_range"], 48.0, 36.0, 24.0);
    assert_eq!(clip_b["effects"][0]["OTIO_SCHEMA"], "LinearTimeWarp.1");
    assert_eq!(clip_b["effects"][0]["time_scalar"], 2.0);
    assert_id(&clip_b["effects"][0], "effect.double-speed");

    let gap = &children[3];
    assert_eq!(gap["OTIO_SCHEMA"], "Gap.1");
    assert_id(gap, "gap.coverage.1");
    assert_range(&gap["source_range"], 0.0, 12.0, 24.0);

    let nested = &children[4];
    assert_eq!(nested["OTIO_SCHEMA"], "Stack.1");
    assert_id(nested, "sequence.nested");
    assert_range(&nested["source_range"], 0.0, 24.0, 24.0);
    let nested_track = &nested["children"][0];
    assert_eq!(nested_track["OTIO_SCHEMA"], "Track.1");
    assert_id(nested_track, "track.nested.v1");
    let nested_clip = &nested_track["children"][0];
    assert_id(nested_clip, "clip.c-slow");
    assert_eq!(nested_clip["effects"][0]["OTIO_SCHEMA"], "LinearTimeWarp.1");
    assert_eq!(nested_clip["effects"][0]["time_scalar"], 0.5);
    assert_id(&nested_clip["effects"][0], "effect.half-speed");

    let encoded = serde_json::to_vec(&timeline).expect("fixture must serialize");
    let decoded: Value = serde_json::from_slice(&encoded).expect("fixture must deserialize");
    assert_eq!(
        decoded, timeline,
        "opaque JSON data must survive round trip"
    );
}

#[test]
fn unsupported_constructs_have_explicit_preserve_and_diagnose_contracts() {
    let timeline = load(COVERAGE);
    let expectations = load(EXPECTATIONS);

    assert_eq!(expectations["schema_version"], 1);
    assert_eq!(
        expectations["reference"]["implementation"],
        "OpenTimelineIO"
    );
    assert_eq!(expectations["reference"]["version"], "0.18.1");
    assert_eq!(expectations["reference"]["schema_family"], "OTIO_CORE");
    assert_eq!(expectations["reference"]["schema_label"], "0.18.1");
    assert_eq!(
        expectations["timelines"][0]["expected_duration"]["value"],
        48
    );
    assert_eq!(
        expectations["timelines"][1]["expected_duration"]["value"],
        120
    );

    let unsupported = expectations["unsupported_constructs"]
        .as_array()
        .expect("unsupported constructs must be an array");
    assert_eq!(unsupported.len(), 2);
    for contract in unsupported {
        let pointer = contract["json_pointer"]
            .as_str()
            .expect("unsupported contract requires a JSON pointer");
        let object = timeline
            .pointer(pointer)
            .expect("unsupported object pointer must resolve");
        assert_eq!(object["OTIO_SCHEMA"], contract["otio_schema"]);
        assert_eq!(object["metadata"]["superi"]["id"], contract["object_id"]);
        assert_eq!(contract["handling"], "preserve_opaque");
        assert_eq!(
            contract["diagnostic"]["code"],
            "timeline.otio.unsupported_construct"
        );
        assert_eq!(contract["diagnostic"]["severity"], "warning");
    }
}
