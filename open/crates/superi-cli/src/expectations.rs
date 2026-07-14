//! Strict repository-local expectations for the canonical editorial slice.

use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const EXPECTATION_FIXTURE_ID: &str = "slice/expectations";
const EXPECTATION_FIXTURE_VERSION: u32 = 2;
const EXPECTATION_RECORD_NAME: &str = "expectations.json";
const EXPECTED_FRAMES_NAME: &str = "expected-frames.rgba";
const SOURCE_MANIFEST_PATH: &str = "open/test-fixtures/slice/video-cfr/v1/fixture.json";
const AUDIO_MANIFEST_PATH: &str =
    "open/test-fixtures/audio/synchronized-multichannel/v1/fixture.json";
const AUDIO_FIXTURE_DIRECTORY: &str = "open/test-fixtures/audio/synchronized-multichannel/v1";
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_RECORD_BYTES: u64 = 1024 * 1024;
const MAX_FRAME_BYTES: u64 = 2 * 1024 * 1024;
const MAX_AUDIO_BYTES: u64 = 256 * 1024;
const PCM_SUBFORMAT_GUID: [u8; 16] = [
    0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71,
];

pub(crate) struct ContractObservations<'a> {
    pub source_manifest_sha256: &'a str,
    pub source_payload_sha256: &'a str,
    pub project_state_sha256: &'a str,
    pub timeline_sha256: &'a str,
    pub graph_sha256: &'a str,
    pub operation_log_sha256: &'a str,
    pub undo_redo_recovered: bool,
    pub timestamps: &'a [Value],
    pub export: &'a Value,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ExpectationFailureKind {
    Unavailable,
    Corrupt,
    Mismatch,
}

#[derive(Debug)]
pub(crate) struct ExpectationFailure {
    kind: ExpectationFailureKind,
    message: String,
}

impl ExpectationFailure {
    pub(crate) const fn kind(&self) -> ExpectationFailureKind {
        self.kind
    }

    pub(crate) fn message(&self) -> &str {
        &self.message
    }

    fn unavailable(message: impl Into<String>) -> Self {
        Self {
            kind: ExpectationFailureKind::Unavailable,
            message: message.into(),
        }
    }

    fn corrupt(message: impl Into<String>) -> Self {
        Self {
            kind: ExpectationFailureKind::Corrupt,
            message: message.into(),
        }
    }

    fn mismatch(message: impl Into<String>) -> Self {
        Self {
            kind: ExpectationFailureKind::Mismatch,
            message: message.into(),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FixtureManifest {
    schema_version: u32,
    fixture_id: String,
    fixture_version: u32,
    description: String,
    provenance: Provenance,
    files: Vec<Payload>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Provenance {
    kind: String,
    source: String,
    author: String,
    created_on: String,
    license: String,
    rights: String,
    generator: Generator,
    parents: Vec<Parent>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Generator {
    name: String,
    version: String,
    command: String,
    seed: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Parent {
    fixture_id: String,
    fixture_version: u32,
    manifest_sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Payload {
    path: String,
    media_type: String,
    bytes: u64,
    sha256: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectationRecord {
    schema_version: u32,
    scenario_id: String,
    scenario_revision: u32,
    source: SourceExpectation,
    frames: FrameExpectations,
    audio: AudioExpectations,
    project_states: ProjectStateExpectations,
    export: ExportExpectation,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceExpectation {
    fixture_id: String,
    fixture_version: u32,
    manifest_sha256: String,
    payload_sha256: String,
    source_start_frame: u64,
    source_end_frame: u64,
    effect: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FrameExpectations {
    path: String,
    pixel_format: String,
    color_space: String,
    width: u32,
    height: u32,
    frame_count: usize,
    bytes_per_frame: usize,
    sha256: String,
    hash_algorithm: String,
    frame_hashes: Vec<String>,
    timestamps: TimestampExpectations,
    tolerance: PixelTolerance,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct TimestampExpectations {
    time_base: Rational,
    values: Vec<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct PixelTolerance {
    mode: String,
    maximum_absolute_error: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AudioExpectations {
    slice_stream_count: u32,
    fixture_id: String,
    fixture_version: u32,
    manifest_sha256: String,
    tolerance: AudioTolerance,
    maximum_adjacent_delta_pcm16: i32,
    cases: Vec<AudioCase>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AudioTolerance {
    mode: String,
    maximum_absolute_error_pcm16: i32,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AudioCase {
    path: String,
    sha256: String,
    sample_rate: u32,
    channel_mask: u32,
    channel_labels: Vec<String>,
    frame_count: usize,
    active_start_frame: usize,
    active_end_frame: usize,
    routing_probe_frame: usize,
    probes: Vec<AudioProbe>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AudioProbe {
    frame: usize,
    samples: Vec<i16>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProjectStateExpectations {
    project_state_sha256: String,
    timeline_sha256: String,
    graph_sha256: String,
    operation_log_sha256: String,
    undo_redo_recovered: bool,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ExportExpectation {
    container: String,
    codec: String,
    encoder: String,
    pixel_format: String,
    color_space: String,
    range: String,
    matrix: String,
    alpha: String,
    frame_rate: Rational,
    time_base: Rational,
    width: u32,
    height: u32,
    frame_count: u64,
    audio_streams: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
struct Rational {
    numerator: u32,
    denominator: u32,
}

pub(crate) fn resolve_expectations(
    repository_root: &Path,
    observations: &ContractObservations<'_>,
) -> Result<Value, ExpectationFailure> {
    let version_directory = repository_root.join("open/test-fixtures/slice/expectations/v2");
    let manifest_path = version_directory.join("fixture.json");
    let manifest_bytes = read_regular(&manifest_path, MAX_MANIFEST_BYTES)?;
    let manifest: FixtureManifest = serde_json::from_slice(&manifest_bytes).map_err(|error| {
        ExpectationFailure::corrupt(format!("expectation manifest is invalid: {error}"))
    })?;
    validate_manifest(repository_root, &manifest)?;

    let record_entry = payload(&manifest, EXPECTATION_RECORD_NAME)?;
    let frame_entry = payload(&manifest, EXPECTED_FRAMES_NAME)?;
    let record_bytes = read_payload(
        &version_directory.join(EXPECTATION_RECORD_NAME),
        record_entry,
        MAX_RECORD_BYTES,
    )?;
    let frame_bytes = read_payload(
        &version_directory.join(EXPECTED_FRAMES_NAME),
        frame_entry,
        MAX_FRAME_BYTES,
    )?;
    let record: ExpectationRecord = serde_json::from_slice(&record_bytes).map_err(|error| {
        ExpectationFailure::corrupt(format!("expectation record is invalid: {error}"))
    })?;

    validate_record(repository_root, &record, &frame_bytes, observations)?;

    Ok(json!({
        "identity": {
            "fixture_id": EXPECTATION_FIXTURE_ID,
            "fixture_version": EXPECTATION_FIXTURE_VERSION,
            "manifest_sha256": sha256_hex(&manifest_bytes),
            "record_sha256": sha256_hex(&record_bytes),
            "reference_frames_sha256": record.frames.sha256
        },
        "status": "contract_passed",
        "reference_frames": {
            "path": "open/test-fixtures/slice/expectations/v2/expected-frames.rgba",
            "pixel_format": record.frames.pixel_format,
            "color_space": record.frames.color_space,
            "width": record.frames.width,
            "height": record.frames.height,
            "frame_count": record.frames.frame_count,
            "bytes_per_frame": record.frames.bytes_per_frame,
            "hash_algorithm": record.frames.hash_algorithm
        },
        "audio_samples": {
            "fixture_id": record.audio.fixture_id,
            "fixture_version": record.audio.fixture_version,
            "case_count": record.audio.cases.len(),
            "maximum_adjacent_delta_pcm16": record.audio.maximum_adjacent_delta_pcm16,
            "slice_stream_count": record.audio.slice_stream_count
        },
        "tolerances": {
            "pixel": record.frames.tolerance,
            "audio": record.audio.tolerance
        },
        "results": [
            {"expectation": "record_integrity", "status": "passed", "comparison": "exact"},
            {"expectation": "reference_frames", "status": "passed", "comparison": "sha256", "compared_frames": record.frames.frame_count},
            {"expectation": "audio_samples", "status": "passed", "comparison": "exact_pcm16", "compared_cases": record.audio.cases.len()},
            {"expectation": "timestamps", "status": "passed", "comparison": "exact", "compared_timestamps": record.frames.timestamps.values.len()},
            {"expectation": "project_state", "status": "passed", "comparison": "sha256", "compared_digests": 4},
            {"expectation": "export_metadata", "status": "passed", "comparison": "exact"},
            {"expectation": "rendered_pixels", "status": "not_evaluated", "comparison": "normalized_absolute", "maximum_absolute_error": record.frames.tolerance.maximum_absolute_error},
            {"expectation": "rendered_audio", "status": "not_applicable", "comparison": "pcm16_absolute", "reason": "canonical slice source and target contain no audio stream"}
        ],
        "diagnostics": [
            "All applicable contract expectations passed.",
            "Rendered pixel comparison was not evaluated because graph, color, and export stages remain stubs.",
            "The video-only slice expects zero audio streams; synchronized audio fixtures validate sample timing and routing independently."
        ]
    }))
}

fn validate_manifest(
    repository_root: &Path,
    manifest: &FixtureManifest,
) -> Result<(), ExpectationFailure> {
    if manifest.schema_version != 1
        || manifest.fixture_id != EXPECTATION_FIXTURE_ID
        || manifest.fixture_version != EXPECTATION_FIXTURE_VERSION
        || manifest.files.len() != 2
        || manifest.provenance.kind != "derived"
        || manifest.provenance.parents.len() != 2
    {
        return Err(ExpectationFailure::corrupt(
            "expectation manifest identity, lineage, or inventory is incorrect",
        ));
    }
    let required_text = [
        manifest.description.as_str(),
        manifest.provenance.source.as_str(),
        manifest.provenance.author.as_str(),
        manifest.provenance.created_on.as_str(),
        manifest.provenance.license.as_str(),
        manifest.provenance.rights.as_str(),
        manifest.provenance.generator.name.as_str(),
        manifest.provenance.generator.version.as_str(),
        manifest.provenance.generator.command.as_str(),
        manifest.provenance.generator.seed.as_str(),
    ];
    if required_text.iter().any(|value| value.trim().is_empty()) {
        return Err(ExpectationFailure::corrupt(
            "expectation manifest provenance is incomplete",
        ));
    }
    let paths = manifest
        .files
        .iter()
        .map(|entry| entry.path.as_str())
        .collect::<BTreeSet<_>>();
    if paths != BTreeSet::from([EXPECTATION_RECORD_NAME, EXPECTED_FRAMES_NAME]) {
        return Err(ExpectationFailure::corrupt(
            "expectation manifest payload names are incorrect",
        ));
    }
    if payload(manifest, EXPECTATION_RECORD_NAME)?.media_type != "application/json"
        || payload(manifest, EXPECTED_FRAMES_NAME)?.media_type != "application/octet-stream"
    {
        return Err(ExpectationFailure::corrupt(
            "expectation manifest payload media types are incorrect",
        ));
    }

    let expected_parents = [
        ("slice/video-cfr", 1, SOURCE_MANIFEST_PATH),
        ("audio/synchronized-multichannel", 1, AUDIO_MANIFEST_PATH),
    ];
    for (parent, (fixture_id, fixture_version, path)) in
        manifest.provenance.parents.iter().zip(expected_parents)
    {
        if parent.fixture_id != fixture_id
            || parent.fixture_version != fixture_version
            || !is_lower_sha256(&parent.manifest_sha256)
        {
            return Err(ExpectationFailure::corrupt(
                "expectation manifest parent identity is incorrect",
            ));
        }
        let parent_bytes = read_regular(&repository_root.join(path), MAX_MANIFEST_BYTES)?;
        if sha256_hex(&parent_bytes) != parent.manifest_sha256 {
            return Err(ExpectationFailure::corrupt(format!(
                "expectation parent manifest {fixture_id} has drifted"
            )));
        }
    }
    Ok(())
}

fn validate_record(
    repository_root: &Path,
    record: &ExpectationRecord,
    frame_bytes: &[u8],
    observations: &ContractObservations<'_>,
) -> Result<(), ExpectationFailure> {
    if record.schema_version != 1
        || record.scenario_id != "superi.slice.canonical.v1"
        || record.scenario_revision != 1
    {
        return Err(ExpectationFailure::corrupt(
            "expectation record scenario identity is incorrect",
        ));
    }
    validate_source(&record.source, observations)?;
    validate_frames(&record.frames, frame_bytes, observations.timestamps)?;
    validate_audio(repository_root, &record.audio)?;
    validate_project_states(&record.project_states, observations)?;
    let expected_export =
        serde_json::to_value(&record.export).expect("expectation export metadata must serialize");
    if &expected_export != observations.export {
        return Err(ExpectationFailure::mismatch(
            "canonical export metadata does not match the expectation record",
        ));
    }
    Ok(())
}

fn validate_source(
    source: &SourceExpectation,
    observations: &ContractObservations<'_>,
) -> Result<(), ExpectationFailure> {
    if source.fixture_id != "slice/video-cfr"
        || source.fixture_version != 1
        || source.source_start_frame != 24
        || source.source_end_frame != 72
        || source.effect != "horizontal_mirror"
        || !is_lower_sha256(&source.manifest_sha256)
        || !is_lower_sha256(&source.payload_sha256)
    {
        return Err(ExpectationFailure::corrupt(
            "expectation source contract is incorrect",
        ));
    }
    if source.manifest_sha256 != observations.source_manifest_sha256
        || source.payload_sha256 != observations.source_payload_sha256
    {
        return Err(ExpectationFailure::mismatch(
            "resolved source identity does not match the expectation record",
        ));
    }
    Ok(())
}

fn validate_frames(
    frames: &FrameExpectations,
    bytes: &[u8],
    observed_timestamps: &[Value],
) -> Result<(), ExpectationFailure> {
    if frames.path != EXPECTED_FRAMES_NAME
        || frames.pixel_format != "rgba8"
        || frames.color_space != "srgb"
        || frames.width != 96
        || frames.height != 54
        || frames.frame_count != 48
        || frames.bytes_per_frame != 96 * 54 * 4
        || frames.hash_algorithm != "sha256"
        || frames.tolerance.mode != "normalized_absolute"
        || frames.tolerance.maximum_absolute_error != 0.001
        || !is_lower_sha256(&frames.sha256)
    {
        return Err(ExpectationFailure::corrupt(
            "reference frame contract is incorrect",
        ));
    }
    let expected_bytes = frames
        .frame_count
        .checked_mul(frames.bytes_per_frame)
        .ok_or_else(|| ExpectationFailure::corrupt("reference frame size overflowed"))?;
    if bytes.len() != expected_bytes || sha256_hex(bytes) != frames.sha256 {
        return Err(ExpectationFailure::corrupt(
            "reference frame payload identity is incorrect",
        ));
    }
    if frames.frame_hashes.len() != frames.frame_count
        || frames
            .frame_hashes
            .iter()
            .any(|digest| !is_lower_sha256(digest))
    {
        return Err(ExpectationFailure::corrupt(
            "reference frame hash inventory is incorrect",
        ));
    }
    for (index, (frame, expected)) in bytes
        .chunks_exact(frames.bytes_per_frame)
        .zip(&frames.frame_hashes)
        .enumerate()
    {
        if sha256_hex(frame) != *expected {
            return Err(ExpectationFailure::corrupt(format!(
                "reference frame {index} digest is incorrect"
            )));
        }
    }
    if frames.timestamps.time_base
        != (Rational {
            numerator: 1,
            denominator: 24,
        })
        || frames.timestamps.values != (0_u64..48).collect::<Vec<_>>()
    {
        return Err(ExpectationFailure::corrupt(
            "reference frame timestamps are incorrect",
        ));
    }
    let expected_timestamps = frames
        .timestamps
        .values
        .iter()
        .map(|value| {
            json!({
                "value": value,
                "time_base": frames.timestamps.time_base
            })
        })
        .collect::<Vec<_>>();
    if expected_timestamps != observed_timestamps {
        return Err(ExpectationFailure::mismatch(
            "modeled output timestamps do not match the expectation record",
        ));
    }
    Ok(())
}

fn validate_audio(
    repository_root: &Path,
    audio: &AudioExpectations,
) -> Result<(), ExpectationFailure> {
    if audio.slice_stream_count != 0
        || audio.fixture_id != "audio/synchronized-multichannel"
        || audio.fixture_version != 1
        || audio.tolerance.mode != "pcm16_absolute"
        || audio.tolerance.maximum_absolute_error_pcm16 != 0
        || audio.maximum_adjacent_delta_pcm16 != 600
        || audio.cases.len() != 3
        || !is_lower_sha256(&audio.manifest_sha256)
    {
        return Err(ExpectationFailure::corrupt(
            "audio expectation contract is incorrect",
        ));
    }
    let manifest = read_regular(
        &repository_root.join(AUDIO_MANIFEST_PATH),
        MAX_MANIFEST_BYTES,
    )?;
    if sha256_hex(&manifest) != audio.manifest_sha256 {
        return Err(ExpectationFailure::corrupt(
            "audio expectation parent manifest has drifted",
        ));
    }

    const STEREO_LABELS: &[&str] = &["front_left", "front_right"];
    const SURROUND_5_1_LABELS: &[&str] = &[
        "front_left",
        "front_right",
        "front_center",
        "low_frequency",
        "back_left",
        "back_right",
    ];
    const SURROUND_7_1_LABELS: &[&str] = &[
        "front_left",
        "front_right",
        "front_center",
        "low_frequency",
        "back_left",
        "back_right",
        "side_left",
        "side_right",
    ];
    let expected = [
        ("stereo-44100.wav", 44_100, 0x0003, STEREO_LABELS),
        (
            "surround-5-1-48000.wav",
            48_000,
            0x003f,
            SURROUND_5_1_LABELS,
        ),
        (
            "surround-7-1-96000.wav",
            96_000,
            0x063f,
            SURROUND_7_1_LABELS,
        ),
    ];
    for (case, (path, sample_rate, channel_mask, labels)) in audio.cases.iter().zip(expected) {
        if case.path != path
            || case.sample_rate != sample_rate
            || case.channel_mask != channel_mask
            || case
                .channel_labels
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                != labels
            || !is_lower_sha256(&case.sha256)
        {
            return Err(ExpectationFailure::corrupt(format!(
                "audio case {} identity or routing is incorrect",
                case.path
            )));
        }
        let bytes = read_regular(
            &repository_root
                .join(AUDIO_FIXTURE_DIRECTORY)
                .join(&case.path),
            MAX_AUDIO_BYTES,
        )?;
        if sha256_hex(&bytes) != case.sha256 {
            return Err(ExpectationFailure::corrupt(format!(
                "audio case {} payload has drifted",
                case.path
            )));
        }
        validate_wave(case, &bytes, audio.maximum_adjacent_delta_pcm16)?;
    }
    Ok(())
}

fn validate_wave(
    case: &AudioCase,
    bytes: &[u8],
    maximum_adjacent_delta: i32,
) -> Result<(), ExpectationFailure> {
    if bytes.len() < 68
        || &bytes[0..4] != b"RIFF"
        || &bytes[8..12] != b"WAVE"
        || &bytes[12..16] != b"fmt "
        || u32_at(bytes, 16)? != 40
        || u16_at(bytes, 20)? != 0xfffe
        || u32_at(bytes, 24)? != case.sample_rate
        || u16_at(bytes, 34)? != 16
        || u16_at(bytes, 38)? != 16
        || u32_at(bytes, 40)? != case.channel_mask
        || bytes[44..60] != PCM_SUBFORMAT_GUID
        || &bytes[60..64] != b"data"
    {
        return Err(ExpectationFailure::corrupt(format!(
            "audio case {} WAVE metadata is incorrect",
            case.path
        )));
    }
    let channel_count = case.channel_labels.len();
    let declared_channels = usize::from(u16_at(bytes, 22)?);
    let block_align = usize::from(u16_at(bytes, 32)?);
    let data_bytes = usize::try_from(u32_at(bytes, 64)?)
        .map_err(|_| ExpectationFailure::corrupt("audio data size overflowed"))?;
    if declared_channels != channel_count
        || block_align != channel_count * 2
        || bytes.len() != 68 + data_bytes
        || data_bytes != case.frame_count * block_align
        || case.frame_count != usize::try_from(case.sample_rate / 10).unwrap()
        || case.active_start_frame != usize::try_from(case.sample_rate / 100).unwrap()
        || case.active_end_frame != usize::try_from(case.sample_rate * 9 / 100).unwrap()
        || case.routing_probe_frame
            != case.active_start_frame + usize::try_from(case.sample_rate / 4000).unwrap()
        || case.probes.len() != 6
    {
        return Err(ExpectationFailure::corrupt(format!(
            "audio case {} timing or layout is incorrect",
            case.path
        )));
    }
    let samples = bytes[68..]
        .chunks_exact(2)
        .map(|sample| i16::from_le_bytes([sample[0], sample[1]]))
        .collect::<Vec<_>>();
    let expected_probe_frames = [
        0,
        case.active_start_frame,
        case.active_start_frame + 1,
        case.routing_probe_frame,
        case.active_end_frame - 1,
        case.active_end_frame,
    ];
    if case
        .probes
        .iter()
        .map(|probe| probe.frame)
        .ne(expected_probe_frames)
    {
        return Err(ExpectationFailure::corrupt(format!(
            "audio case {} probe timing is incorrect",
            case.path
        )));
    }
    if !samples[..case.active_start_frame * channel_count]
        .iter()
        .all(|sample| *sample == 0)
        || !samples[case.active_end_frame * channel_count..]
            .iter()
            .all(|sample| *sample == 0)
    {
        return Err(ExpectationFailure::corrupt(format!(
            "audio case {} silence boundaries are incorrect",
            case.path
        )));
    }
    for probe in &case.probes {
        if probe.frame >= case.frame_count || probe.samples.len() != channel_count {
            return Err(ExpectationFailure::corrupt(format!(
                "audio case {} probe shape is incorrect",
                case.path
            )));
        }
        let start = probe.frame * channel_count;
        if samples[start..start + channel_count] != probe.samples {
            return Err(ExpectationFailure::corrupt(format!(
                "audio case {} probe at frame {} is incorrect",
                case.path, probe.frame
            )));
        }
    }
    let route_start = case.routing_probe_frame * channel_count;
    if !samples[route_start..route_start + channel_count]
        .windows(2)
        .all(|pair| pair[0].abs() < pair[1].abs())
    {
        return Err(ExpectationFailure::corrupt(format!(
            "audio case {} routing signature is incorrect",
            case.path
        )));
    }
    for channel in 0..channel_count {
        let maximum = (1..case.frame_count)
            .map(|frame| {
                let current = samples[frame * channel_count + channel];
                let previous = samples[(frame - 1) * channel_count + channel];
                (i32::from(current) - i32::from(previous)).abs()
            })
            .max()
            .unwrap_or(0);
        if maximum > maximum_adjacent_delta {
            return Err(ExpectationFailure::corrupt(format!(
                "audio case {} channel {channel} continuity exceeded its bound",
                case.path
            )));
        }
    }
    Ok(())
}

fn validate_project_states(
    expected: &ProjectStateExpectations,
    actual: &ContractObservations<'_>,
) -> Result<(), ExpectationFailure> {
    let digests = [
        (
            "project_state_sha256",
            &expected.project_state_sha256,
            actual.project_state_sha256,
        ),
        (
            "timeline_sha256",
            &expected.timeline_sha256,
            actual.timeline_sha256,
        ),
        ("graph_sha256", &expected.graph_sha256, actual.graph_sha256),
        (
            "operation_log_sha256",
            &expected.operation_log_sha256,
            actual.operation_log_sha256,
        ),
    ];
    for (name, expected, actual) in digests {
        if !is_lower_sha256(expected) {
            return Err(ExpectationFailure::corrupt(format!(
                "canonical project state digest {name} is invalid"
            )));
        }
        if expected != actual {
            return Err(ExpectationFailure::mismatch(format!(
                "canonical project state digest {name} differs: expected {expected}, observed {actual}"
            )));
        }
    }
    if expected.undo_redo_recovered != actual.undo_redo_recovered {
        return Err(ExpectationFailure::mismatch(
            "canonical project undo and redo state does not match the expectation record",
        ));
    }
    Ok(())
}

fn payload<'a>(
    manifest: &'a FixtureManifest,
    name: &str,
) -> Result<&'a Payload, ExpectationFailure> {
    manifest
        .files
        .iter()
        .find(|entry| entry.path == name)
        .ok_or_else(|| ExpectationFailure::corrupt(format!("manifest omits {name}")))
}

fn read_payload(path: &Path, entry: &Payload, limit: u64) -> Result<Vec<u8>, ExpectationFailure> {
    if !is_lower_sha256(&entry.sha256) || entry.bytes > limit {
        return Err(ExpectationFailure::corrupt(format!(
            "payload {} declaration is invalid",
            entry.path
        )));
    }
    let bytes = read_regular(path, limit)?;
    if bytes.len() as u64 != entry.bytes || sha256_hex(&bytes) != entry.sha256 {
        return Err(ExpectationFailure::corrupt(format!(
            "payload {} does not match its manifest",
            entry.path
        )));
    }
    Ok(bytes)
}

fn read_regular(path: &Path, limit: u64) -> Result<Vec<u8>, ExpectationFailure> {
    let metadata = fs::symlink_metadata(path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ExpectationFailure::unavailable(format!("{} is unavailable", path.display()))
        } else {
            ExpectationFailure::unavailable(format!(
                "{} could not be inspected: {error}",
                path.display()
            ))
        }
    })?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(ExpectationFailure::corrupt(format!(
            "{} must be a non-symlink regular file",
            path.display()
        )));
    }
    if metadata.len() > limit {
        return Err(ExpectationFailure::corrupt(format!(
            "{} exceeds its byte bound",
            path.display()
        )));
    }
    let file = File::open(path).map_err(|error| {
        ExpectationFailure::unavailable(format!("{} could not be opened: {error}", path.display()))
    })?;
    let mut bytes = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            ExpectationFailure::unavailable(format!(
                "{} could not be read: {error}",
                path.display()
            ))
        })?;
    if bytes.len() as u64 > limit {
        return Err(ExpectationFailure::corrupt(format!(
            "{} grew beyond its byte bound",
            path.display()
        )));
    }
    Ok(bytes)
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, ExpectationFailure> {
    let value = bytes
        .get(offset..offset + 2)
        .ok_or_else(|| ExpectationFailure::corrupt("WAVE field is truncated"))?;
    Ok(u16::from_le_bytes([value[0], value[1]]))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, ExpectationFailure> {
    let value = bytes
        .get(offset..offset + 4)
        .ok_or_else(|| ExpectationFailure::corrupt("WAVE field is truncated"))?;
    Ok(u32::from_le_bytes([value[0], value[1], value[2], value[3]]))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn is_lower_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

    struct TemporaryRepository(PathBuf);

    impl TemporaryRepository {
        fn copy_from(source: &Path) -> Self {
            let suffix = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "superi-expectations-{}-{suffix}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&root);
            for relative in [
                "open/test-fixtures/slice/expectations/v2",
                "open/test-fixtures/slice/video-cfr/v1",
                "open/test-fixtures/audio/synchronized-multichannel/v1",
            ] {
                copy_directory(&source.join(relative), &root.join(relative));
            }
            Self(root)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TemporaryRepository {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn canonical_record_validates_every_applicable_contract_observation() {
        let timestamps = canonical_timestamps();
        let export = canonical_export();
        let observations = canonical_observations(&timestamps, &export);

        let evidence = resolve_expectations(&repository_root(), &observations).unwrap();

        assert_eq!(evidence["status"], "contract_passed");
        assert_eq!(evidence["reference_frames"]["frame_count"], 48);
        assert_eq!(evidence["audio_samples"]["case_count"], 3);
        assert_eq!(evidence["results"].as_array().unwrap().len(), 8);
    }

    #[test]
    fn reference_payload_drift_is_rejected_before_comparison() {
        let root = TemporaryRepository::copy_from(&repository_root());
        let path = root
            .path()
            .join("open/test-fixtures/slice/expectations/v2/expected-frames.rgba");
        let mut bytes = fs::read(&path).unwrap();
        bytes[0] ^= 1;
        fs::write(path, bytes).unwrap();
        let timestamps = canonical_timestamps();
        let export = canonical_export();
        let observations = canonical_observations(&timestamps, &export);

        let error = resolve_expectations(root.path(), &observations).unwrap_err();

        assert_eq!(error.kind(), ExpectationFailureKind::Corrupt);
        assert!(error.message().contains("does not match its manifest"));
    }

    #[test]
    fn state_drift_is_a_contract_mismatch_not_fixture_corruption() {
        let timestamps = canonical_timestamps();
        let export = canonical_export();
        let mut observations = canonical_observations(&timestamps, &export);
        observations.project_state_sha256 =
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

        let error = resolve_expectations(&repository_root(), &observations).unwrap_err();

        assert_eq!(error.kind(), ExpectationFailureKind::Mismatch);
        assert!(error.message().contains("project state"));
    }

    fn repository_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .unwrap()
            .to_path_buf()
    }

    fn canonical_observations<'a>(
        timestamps: &'a [Value],
        export: &'a Value,
    ) -> ContractObservations<'a> {
        ContractObservations {
            source_manifest_sha256:
                "fc76adeced535ff05e6adb36c2549939618cfd0f73de7de5fa9d7f7f4301dc08",
            source_payload_sha256:
                "117f5cebcaaf788d1891e84aec57066c73e33d4af308368f640f28a8419f4bbc",
            project_state_sha256:
                "15628621f9e49cdab04ff1623474f7cc4ea6f175a38d7b6ab95722b84403a63b",
            timeline_sha256: "0b55ecf025fea4b20f09fe3ffdd3ab8a3ec5d2e1b85833f1b89db1c9ee04269f",
            graph_sha256: "f1f10d90cc7418f8cd7476a49340a461384c92c036a57258f3097450e538de65",
            operation_log_sha256:
                "cac8f7891e1d5e7609a440b65cdf50714e75fee4c85ab8d40527d6f08436b899",
            undo_redo_recovered: true,
            timestamps,
            export,
        }
    }

    fn canonical_timestamps() -> Vec<Value> {
        (0_u64..48)
            .map(|value| {
                json!({
                    "value": value,
                    "time_base": {"numerator": 1, "denominator": 24}
                })
            })
            .collect()
    }

    fn canonical_export() -> Value {
        json!({
            "container": "webm",
            "codec": "av1",
            "encoder": "rust-av1",
            "pixel_format": "yuv420p8",
            "color_space": "srgb",
            "range": "limited",
            "matrix": "bt709",
            "alpha": "opaque",
            "frame_rate": {"numerator": 24, "denominator": 1},
            "time_base": {"numerator": 1, "denominator": 24},
            "width": 96,
            "height": 54,
            "frame_count": 48,
            "audio_streams": 0
        })
    }

    fn copy_directory(source: &Path, destination: &Path) {
        fs::create_dir_all(destination).unwrap();
        for entry in fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            let destination = destination.join(entry.file_name());
            if entry.file_type().unwrap().is_dir() {
                copy_directory(&entry.path(), &destination);
            } else {
                fs::copy(entry.path(), destination).unwrap();
            }
        }
    }
}
