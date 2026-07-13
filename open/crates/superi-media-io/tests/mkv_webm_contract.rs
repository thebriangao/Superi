use std::fs::File;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use superi_core::error::ErrorCategory;
use superi_core::ids::MediaId;
use superi_core::time::{RationalTime, Timebase};
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendRegistration, BackendRegistry, BackendTier,
    FallbackPolicy, MediaBackend,
};
use superi_media_io::demux::{
    MediaSource, MetadataValue, Packet, SeekMode, SeekRequest, SourceLocation, SourceProbeLimits,
    SourceRequest, StreamKind,
};
use superi_media_io::mkv_webm::MkvWebmBackend;
use superi_media_io::operation::{CancellationToken, MediaPriority, OperationContext};
use superi_media_io::read::ReadOutcome;

const EBML: &[u8] = &[0x1A, 0x45, 0xDF, 0xA3];
const SEGMENT: &[u8] = &[0x18, 0x53, 0x80, 0x67];
const INFO: &[u8] = &[0x15, 0x49, 0xA9, 0x66];
const TRACKS: &[u8] = &[0x16, 0x54, 0xAE, 0x6B];
const CLUSTER: &[u8] = &[0x1F, 0x43, 0xB6, 0x75];
const CUES: &[u8] = &[0x1C, 0x53, 0xBB, 0x6B];

fn operation() -> OperationContext {
    OperationContext::new(MediaPriority::Interactive)
}

fn element(id: &[u8], payload: Vec<u8>) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(id.len() + 8 + payload.len());
    bytes.extend_from_slice(id);
    bytes.extend(size_vint(payload.len()));
    bytes.extend(payload);
    bytes
}

fn size_vint(value: usize) -> Vec<u8> {
    let value = u64::try_from(value).unwrap();
    for width in 1..=8 {
        let bits = width * 7;
        let unknown = (1_u64 << bits) - 1;
        if value < unknown {
            let mut encoded = value.to_be_bytes()[8 - width..].to_vec();
            encoded[0] |= 1 << (8 - width);
            return encoded;
        }
    }
    panic!("fixture element is too large");
}

fn data_vint(value: u64) -> Vec<u8> {
    for width in 1..=8 {
        let bits = width * 7;
        if value < (1_u64 << bits) - 1 {
            let mut encoded = value.to_be_bytes()[8 - width..].to_vec();
            encoded[0] |= 1 << (8 - width);
            return encoded;
        }
    }
    panic!("fixture VINT is too large");
}

fn signed_lace_vint(value: i64) -> Vec<u8> {
    for width in 1..=8 {
        let bias = (1_i128 << (width * 7 - 1)) - 1;
        let encoded = i128::from(value) + bias;
        if encoded >= 0 && encoded < (1_i128 << (width * 7)) - 1 {
            return data_vint(u64::try_from(encoded).unwrap());
        }
    }
    panic!("fixture signed VINT is too large");
}

fn unsigned(id: &[u8], value: u64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let first = bytes
        .iter()
        .position(|byte| *byte != 0)
        .unwrap_or(bytes.len() - 1);
    element(id, bytes[first..].to_vec())
}

fn signed(id: &[u8], value: i64) -> Vec<u8> {
    let bytes = value.to_be_bytes();
    let mut first = 0;
    while first < 7 {
        let redundant_positive = bytes[first] == 0 && bytes[first + 1] & 0x80 == 0;
        let redundant_negative = bytes[first] == 0xFF && bytes[first + 1] & 0x80 != 0;
        if !redundant_positive && !redundant_negative {
            break;
        }
        first += 1;
    }
    element(id, bytes[first..].to_vec())
}

fn float(id: &[u8], value: f64) -> Vec<u8> {
    element(id, value.to_be_bytes().to_vec())
}

fn text(id: &[u8], value: &str) -> Vec<u8> {
    element(id, value.as_bytes().to_vec())
}

fn ebml_header(doc_type: &str) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend(unsigned(&[0x42, 0x86], 1));
    payload.extend(unsigned(&[0x42, 0xF7], 1));
    payload.extend(unsigned(&[0x42, 0xF2], 4));
    payload.extend(unsigned(&[0x42, 0xF3], 8));
    payload.extend(text(&[0x42, 0x82], doc_type));
    payload.extend(unsigned(&[0x42, 0x87], 4));
    payload.extend(unsigned(&[0x42, 0x85], 2));
    element(EBML, payload)
}

fn info(duration_ticks: f64) -> Vec<u8> {
    let mut payload = Vec::new();
    payload.extend(element(&[0x73, 0xA4], vec![0xA5; 16]));
    payload.extend(unsigned(&[0x2A, 0xD7, 0xB1], 1_000_000));
    payload.extend(float(&[0x44, 0x89], duration_ticks));
    payload.extend(text(&[0x7B, 0xA9], "Contract fixture"));
    payload.extend(text(&[0x4D, 0x80], "superi-test-mux"));
    payload.extend(text(&[0x57, 0x41], "superi-test-writer"));
    element(INFO, payload)
}

fn video_track(codec: &str) -> Vec<u8> {
    let mut video = Vec::new();
    video.extend(unsigned(&[0xB0], 1_920));
    video.extend(unsigned(&[0xBA], 1_080));
    video.extend(unsigned(&[0x54, 0xB0], 1_920));
    video.extend(unsigned(&[0x54, 0xBA], 1_080));
    video.extend(unsigned(&[0x53, 0xB8], 0));
    video.extend(unsigned(&[0x53, 0xC0], 1));

    let mut track = Vec::new();
    track.extend(unsigned(&[0xD7], 1));
    track.extend(unsigned(&[0x73, 0xC5], 101));
    track.extend(unsigned(&[0x83], 1));
    track.extend(unsigned(&[0xB9], 1));
    track.extend(unsigned(&[0x88], 1));
    track.extend(unsigned(&[0x55, 0xAA], 0));
    track.extend(unsigned(&[0x9C], 0));
    track.extend(unsigned(&[0x23, 0xE3, 0x83], 40_000_000));
    track.extend(text(&[0x53, 0x6E], "Picture"));
    track.extend(text(&[0x22, 0xB5, 0x9C], "eng"));
    track.extend(text(&[0x86], codec));
    track.extend(element(&[0x63, 0xA2], vec![1, 100, 0, 31]));
    track.extend(element(&[0xE0], video));
    element(&[0xAE], track)
}

fn audio_track(codec: &str) -> Vec<u8> {
    audio_track_with_default_duration(codec, Some(20_000_000))
}

fn audio_track_with_default_duration(codec: &str, default_duration_ns: Option<u64>) -> Vec<u8> {
    let mut audio = Vec::new();
    audio.extend(float(&[0xB5], 48_000.0));
    audio.extend(float(&[0x78, 0xB5], 48_000.0));
    audio.extend(unsigned(&[0x9F], 2));
    audio.extend(unsigned(&[0x62, 0x64], 24));

    let mut track = Vec::new();
    track.extend(unsigned(&[0xD7], 2));
    track.extend(unsigned(&[0x73, 0xC5], 202));
    track.extend(unsigned(&[0x83], 2));
    track.extend(unsigned(&[0xB9], 1));
    track.extend(unsigned(&[0x88], 1));
    track.extend(unsigned(&[0x9C], 1));
    if let Some(duration) = default_duration_ns {
        track.extend(unsigned(&[0x23, 0xE3, 0x83], duration));
    }
    track.extend(text(&[0x53, 0x6E], "Sound"));
    track.extend(text(&[0x22, 0xB5, 0x9C], "eng"));
    track.extend(text(&[0x86], codec));
    track.extend(element(&[0x63, 0xA2], vec![79, 112, 117, 115]));
    track.extend(unsigned(&[0x56, 0xAA], 6_500_000));
    track.extend(unsigned(&[0x56, 0xBB], 80_000_000));
    track.extend(element(&[0xE1], audio));
    element(&[0xAE], track)
}

fn block(track: u64, relative_time: i16, flags: u8, payload: &[u8]) -> Vec<u8> {
    let mut bytes = data_vint(track);
    bytes.extend_from_slice(&relative_time.to_be_bytes());
    bytes.push(flags);
    bytes.extend_from_slice(payload);
    bytes
}

fn simple_block(track: u64, relative_time: i16, flags: u8, payload: &[u8]) -> Vec<u8> {
    element(&[0xA3], block(track, relative_time, flags, payload))
}

fn block_group(
    track: u64,
    relative_time: i16,
    payload: &[u8],
    duration_ticks: u64,
    reference_ticks: i64,
) -> Vec<u8> {
    let mut group = Vec::new();
    group.extend(element(&[0xA1], block(track, relative_time, 0, payload)));
    group.extend(unsigned(&[0x9B], duration_ticks));
    group.extend(signed(&[0xFB], reference_ticks));
    group.extend(signed(&[0x75, 0xA2], -5));
    group.extend(element(&[0xA4], vec![0xCC, 0xDD]));
    let mut block_more = unsigned(&[0xEE], 7);
    block_more.extend(element(&[0xA5], vec![0xBE, 0xEF]));
    group.extend(element(&[0x75, 0xA1], element(&[0xA6], block_more)));
    element(&[0xA0], group)
}

fn standard_fixture(doc_type: &str) -> Vec<u8> {
    let video_codec = if doc_type == "webm" {
        "V_VP9"
    } else {
        "V_MPEG4/ISO/AVC"
    };
    let mut tracks = Vec::new();
    tracks.extend(video_track(video_codec));
    tracks.extend(audio_track("A_OPUS"));

    let mut fixed_lace = vec![1];
    fixed_lace.extend_from_slice(&[0xA1, 0xA2, 0xB1, 0xB2]);

    let mut cluster = Vec::new();
    cluster.extend(unsigned(&[0xE7], 0));
    cluster.extend(simple_block(1, 0, 0x80, &[0x10, 0x11, 0x12]));
    cluster.extend(simple_block(2, 0, 0x84, &fixed_lace));
    cluster.extend(block_group(1, 40, &[0x20, 0x21], 40, -40));
    cluster.extend(simple_block(1, 80, 0x80, &[0x30, 0x31, 0x32]));

    let mut cue_track_positions = Vec::new();
    cue_track_positions.extend(unsigned(&[0xF7], 1));
    cue_track_positions.extend(unsigned(&[0xF1], 0));
    let mut cue_point = Vec::new();
    cue_point.extend(unsigned(&[0xB3], 0));
    cue_point.extend(element(&[0xB7], cue_track_positions));

    let mut segment = Vec::new();
    segment.extend(info(120.0));
    segment.extend(element(TRACKS, tracks));
    segment.extend(element(CLUSTER, cluster));
    segment.extend(element(CUES, element(&[0xBB], cue_point)));

    let mut bytes = ebml_header(doc_type);
    bytes.extend(element(SEGMENT, segment));
    bytes
}

fn lacing_fixture() -> Vec<u8> {
    let mut track = audio_track("A_OPUS");
    track = element(TRACKS, std::mem::take(&mut track));

    let no_lace = simple_block(2, 0, 0x80, b"n");

    let mut xiph = vec![1, 2];
    xiph.extend_from_slice(b"xxyyy");
    let xiph_lace = simple_block(2, 10, 0x82, &xiph);

    let mut fixed = vec![1];
    fixed.extend_from_slice(b"aabb");
    let fixed_lace = simple_block(2, 30, 0x84, &fixed);

    let mut ebml = vec![2];
    ebml.extend(data_vint(2));
    ebml.extend(signed_lace_vint(1));
    ebml.extend_from_slice(b"eekkkq");
    let ebml_lace = simple_block(2, 50, 0x86, &ebml);

    let mut cluster = unsigned(&[0xE7], 0);
    cluster.extend(no_lace);
    cluster.extend(xiph_lace);
    cluster.extend(fixed_lace);
    cluster.extend(ebml_lace);

    let mut segment = info(100.0);
    segment.extend(track);
    segment.extend(element(CLUSTER, cluster));

    let mut bytes = ebml_header("matroska");
    bytes.extend(element(SEGMENT, segment));
    bytes
}

fn registry() -> BackendRegistry {
    let mut registry = BackendRegistry::new();
    registry
        .register(
            BackendRegistration::new(
                Arc::new(MkvWebmBackend::new().unwrap()),
                BackendCapabilities::new([BackendCapability::Source]),
                100,
                BackendTier::Primary,
            )
            .unwrap(),
        )
        .unwrap();
    registry
}

fn request(media_id: u128, name: &str, bytes: Vec<u8>) -> SourceRequest {
    SourceRequest::new(
        MediaId::from_raw(media_id),
        SourceLocation::Memory {
            name: name.to_owned(),
            data: Arc::from(bytes),
        },
    )
}

fn open(bytes: Vec<u8>, name: &str) -> Box<dyn MediaSource> {
    registry()
        .probe_source(
            request(0x505, name, bytes),
            SourceProbeLimits::new(4, 4_096).unwrap(),
            FallbackPolicy::Disallow,
            &operation(),
        )
        .unwrap()
        .open(&operation())
        .unwrap()
}

fn open_error_category(bytes: Vec<u8>, name: &str) -> ErrorCategory {
    registry()
        .probe_source(
            request(0x505, name, bytes),
            SourceProbeLimits::new(4, 4_096).unwrap(),
            FallbackPolicy::Disallow,
            &operation(),
        )
        .unwrap()
        .open(&operation())
        .err()
        .expect("fixture should fail to open")
        .category()
}

struct TempSource(PathBuf);

impl TempSource {
    fn write(extension: &str, bytes: &[u8]) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "superi-mkv-webm-{}-{nonce}.{extension}",
            std::process::id()
        ));
        std::fs::write(&path, bytes).unwrap();
        Self(path)
    }

    fn sparse_mkv(length: u64) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "superi-mkv-webm-{}-{nonce}.mkv",
            std::process::id()
        ));
        let mut file = File::create(&path).unwrap();
        file.write_all(&ebml_header("matroska")).unwrap();
        file.set_len(length).unwrap();
        Self(path)
    }
}

impl Drop for TempSource {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

fn next_packet(source: &mut dyn MediaSource) -> Option<Packet> {
    match source.read_packet(&operation()).unwrap() {
        ReadOutcome::Complete(packet) => Some(packet),
        ReadOutcome::EndOfStream => None,
        ReadOutcome::Partial { .. } => panic!("Matroska packets are atomic"),
        _ => panic!("unknown Matroska packet outcome"),
    }
}

#[test]
fn exposes_a_codec_neutral_mkv_and_webm_backend() {
    let backend = MkvWebmBackend::new().expect("backend descriptor should be valid");
    assert_eq!(backend.descriptor().id().as_str(), "mkv-webm");
}

#[test]
fn probes_and_opens_mkv_with_exact_stream_packet_and_seek_contracts() {
    let bytes = standard_fixture("matroska");
    let selection = registry()
        .probe_source(
            request(0x44, "misleading.webm", bytes),
            SourceProbeLimits::new(4, 4_096).unwrap(),
            FallbackPolicy::Disallow,
            &operation(),
        )
        .unwrap();
    assert_eq!(selection.primary().container().as_str(), "mkv");
    assert!(selection.bytes_examined() > 4);

    let mut source = selection.open(&operation()).unwrap();
    let info = source.info();
    assert_eq!(info.identity().media_id(), MediaId::from_raw(0x44));
    assert_eq!(info.duration().unwrap().value(), 120_000_000);
    assert_eq!(info.duration().unwrap().timebase(), Timebase::NANOSECONDS);
    assert_eq!(
        info.metadata().get("container.kind"),
        Some(&MetadataValue::Text("mkv".into()))
    );
    assert_eq!(
        info.metadata().get("container.title"),
        Some(&MetadataValue::Text("Contract fixture".into()))
    );
    assert_eq!(
        info.metadata().get("container.cue-point-count"),
        Some(&MetadataValue::Unsigned(1))
    );
    assert_eq!(info.streams().len(), 2);
    assert_eq!(info.streams()[0].kind(), StreamKind::Video);
    assert_eq!(info.streams()[0].codec().as_str(), "h264");
    assert_eq!(
        info.streams()[0].metadata().get("video.pixel-width"),
        Some(&MetadataValue::Unsigned(1_920))
    );
    assert_eq!(info.streams()[1].kind(), StreamKind::Audio);
    assert_eq!(info.streams()[1].codec().as_str(), "opus");
    assert_eq!(
        info.streams()[1].metadata().get("audio.channels"),
        Some(&MetadataValue::Unsigned(2))
    );
    assert_eq!(
        info.streams()[1].metadata().get("codec.delay-ns"),
        Some(&MetadataValue::Unsigned(6_500_000))
    );

    let packets = std::iter::from_fn(|| next_packet(source.as_mut())).collect::<Vec<_>>();
    assert_eq!(packets.len(), 5);
    assert_eq!(packets[0].data(), &[0x10, 0x11, 0x12]);
    assert!(packets[0].is_keyframe());
    assert_eq!(packets[1].data(), &[0xA1, 0xA2]);
    assert_eq!(packets[1].timing().presentation_time().unwrap().value(), 0);
    assert_eq!(packets[2].data(), &[0xB1, 0xB2]);
    assert_eq!(
        packets[2].timing().presentation_time().unwrap().value(),
        20_000_000
    );
    assert_eq!(packets[3].data(), &[0x20, 0x21]);
    assert!(!packets[3].is_keyframe());
    assert_eq!(
        packets[3].metadata().get("container.reference-blocks"),
        Some(&MetadataValue::Text("-40".into()))
    );
    assert_eq!(
        packets[3].metadata().get("container.discard-padding-ns"),
        Some(&MetadataValue::Signed(-5))
    );
    assert_eq!(
        packets[3].metadata().get("codec.state"),
        Some(&MetadataValue::Bytes(Arc::from([0xCC, 0xDD])))
    );
    assert_eq!(
        packets[3].metadata().get("container.block-addition.0.id"),
        Some(&MetadataValue::Unsigned(7))
    );
    assert_eq!(
        packets[3].metadata().get("container.block-addition.0.data"),
        Some(&MetadataValue::Bytes(Arc::from([0xBE, 0xEF])))
    );
    assert_eq!(packets[4].data(), &[0x30, 0x31, 0x32]);
    assert!(packets[4].is_keyframe());

    let actual = source
        .seek(
            SeekRequest::new(
                RationalTime::new(70_000_000, Timebase::NANOSECONDS),
                SeekMode::PreviousKeyframe,
            ),
            &operation(),
        )
        .unwrap();
    assert_eq!(actual.value(), 0);
    let actual = source
        .seek(
            SeekRequest::new(
                RationalTime::new(80_000_000, Timebase::NANOSECONDS),
                SeekMode::Exact,
            ),
            &operation(),
        )
        .unwrap();
    assert_eq!(actual.value(), 80_000_000);
    assert_eq!(
        next_packet(source.as_mut()).unwrap().data(),
        &[0x30, 0x31, 0x32]
    );
}

#[test]
fn recognizes_webm_by_doctype_and_preserves_profile_codecs() {
    let source = open(standard_fixture("webm"), "wrong.mkv");
    assert_eq!(
        source.info().metadata().get("container.kind"),
        Some(&MetadataValue::Text("webm".into()))
    );
    assert_eq!(source.info().streams()[0].codec().as_str(), "vp9");
    assert_eq!(source.info().streams()[1].codec().as_str(), "opus");
}

#[test]
fn applies_matroska_track_defaults_without_losing_declared_dimensions() {
    let mut video = Vec::new();
    video.extend(unsigned(&[0xB0], 640));
    video.extend(unsigned(&[0xBA], 360));
    let mut video_entry = Vec::new();
    video_entry.extend(unsigned(&[0xD7], 1));
    video_entry.extend(unsigned(&[0x73, 0xC5], 101));
    video_entry.extend(unsigned(&[0x83], 1));
    video_entry.extend(text(&[0x86], "V_VP9"));
    video_entry.extend(element(&[0xE0], video));

    let mut audio_entry = Vec::new();
    audio_entry.extend(unsigned(&[0xD7], 2));
    audio_entry.extend(unsigned(&[0x73, 0xC5], 202));
    audio_entry.extend(unsigned(&[0x83], 2));
    audio_entry.extend(text(&[0x86], "A_OPUS"));
    audio_entry.extend(element(&[0xE1], Vec::new()));

    let mut segment = info(0.0);
    let mut tracks = element(&[0xAE], video_entry);
    tracks.extend(element(&[0xAE], audio_entry));
    segment.extend(element(TRACKS, tracks));
    let mut bytes = ebml_header("matroska");
    bytes.extend(element(SEGMENT, segment));

    let source = open(bytes, "defaults.mkv");
    let streams = source.info().streams();
    assert_eq!(
        streams[0].metadata().get("video.display-width"),
        Some(&MetadataValue::Unsigned(640))
    );
    assert_eq!(
        streams[0].metadata().get("video.display-height"),
        Some(&MetadataValue::Unsigned(360))
    );
    assert_eq!(
        streams[1].metadata().get("audio.sampling-frequency"),
        Some(&MetadataValue::Text("8000".into()))
    );
    assert_eq!(
        streams[1].metadata().get("audio.output-sampling-frequency"),
        Some(&MetadataValue::Text("8000".into()))
    );
    assert_eq!(
        streams[1].metadata().get("audio.channels"),
        Some(&MetadataValue::Unsigned(1))
    );
}

#[test]
fn expands_no_xiph_fixed_and_ebml_lacing_without_losing_frame_order() {
    let mut source = open(lacing_fixture(), "lacing.mkv");
    let packets = std::iter::from_fn(|| next_packet(source.as_mut())).collect::<Vec<_>>();
    let payloads = packets
        .iter()
        .map(|packet| packet.data().to_vec())
        .collect::<Vec<_>>();
    assert_eq!(
        payloads,
        vec![
            b"n".to_vec(),
            b"xx".to_vec(),
            b"yyy".to_vec(),
            b"aa".to_vec(),
            b"bb".to_vec(),
            b"ee".to_vec(),
            b"kkk".to_vec(),
            b"q".to_vec(),
        ]
    );
    let times = packets
        .iter()
        .map(|packet| packet.timing().presentation_time().unwrap().value())
        .collect::<Vec<_>>();
    assert_eq!(
        times,
        vec![
            0, 10_000_000, 30_000_000, 30_000_000, 50_000_000, 50_000_000, 70_000_000, 90_000_000,
        ]
    );
    assert_eq!(
        packets[6].metadata().get("container.lace-index"),
        Some(&MetadataValue::Unsigned(1))
    );
}

#[test]
fn accepts_unknown_sized_segment_and_cluster_with_schema_aware_boundaries() {
    let mut tracks = Vec::new();
    tracks.extend(video_track("V_VP9"));

    let mut cluster_payload = unsigned(&[0xE7], 5);
    cluster_payload.extend(simple_block(1, 0, 0x80, b"frame"));

    let mut cue_track_positions = unsigned(&[0xF7], 1);
    cue_track_positions.extend(unsigned(&[0xF1], 0));
    let mut cue_point = unsigned(&[0xB3], 5);
    cue_point.extend(element(&[0xB7], cue_track_positions));

    let mut bytes = ebml_header("webm");
    bytes.extend_from_slice(SEGMENT);
    bytes.push(0xFF);
    bytes.extend(info(50.0));
    bytes.extend(element(TRACKS, tracks));
    bytes.extend_from_slice(CLUSTER);
    bytes.push(0xFF);
    bytes.extend(cluster_payload);
    bytes.extend(element(CUES, element(&[0xBB], cue_point)));

    let mut source = open(bytes, "live.webm");
    assert_eq!(
        source.info().metadata().get("container.cue-point-count"),
        Some(&MetadataValue::Unsigned(1))
    );
    assert_eq!(next_packet(source.as_mut()).unwrap().data(), b"frame");
    assert!(next_packet(source.as_mut()).is_none());
}

#[test]
fn path_ingest_uses_the_same_content_identity_and_packet_contract() {
    let file = TempSource::write("bin", &standard_fixture("webm"));
    let selection = registry()
        .probe_source(
            SourceRequest::new(
                MediaId::from_raw(0x77),
                SourceLocation::Path(file.0.clone()),
            ),
            SourceProbeLimits::new(8, 4_096).unwrap(),
            FallbackPolicy::Disallow,
            &operation(),
        )
        .unwrap();
    assert_eq!(selection.primary().container().as_str(), "webm");
    let mut source = selection.open(&operation()).unwrap();
    assert_eq!(source.info().identity().media_id(), MediaId::from_raw(0x77));
    assert_eq!(
        next_packet(source.as_mut()).unwrap().data(),
        &[0x10, 0x11, 0x12]
    );
}

#[test]
fn leaves_laced_followup_timestamps_absent_when_the_container_cannot_determine_them() {
    let track = element(TRACKS, audio_track_with_default_duration("A_OPUS", None));
    let mut xiph = vec![1, 2];
    xiph.extend_from_slice(b"aabbb");
    let mut cluster = unsigned(&[0xE7], 0);
    cluster.extend(simple_block(2, 10, 0x82, &xiph));
    let mut segment = info(50.0);
    segment.extend(track);
    segment.extend(element(CLUSTER, cluster));
    let mut bytes = ebml_header("matroska");
    bytes.extend(element(SEGMENT, segment));

    let mut source = open(bytes, "underdetermined.mkv");
    let first = next_packet(source.as_mut()).unwrap();
    let second = next_packet(source.as_mut()).unwrap();
    assert_eq!(
        first.timing().presentation_time().unwrap().value(),
        10_000_000
    );
    assert!(first.timing().duration().is_none());
    assert!(second.timing().presentation_time().is_none());
    assert!(second.timing().duration().is_none());
}

#[test]
fn rejects_unbounded_source_and_block_metadata_inputs() {
    let file = TempSource::sparse_mkv(513 * 1024 * 1024);
    let selection = registry()
        .probe_source(
            SourceRequest::new(
                MediaId::from_raw(0x88),
                SourceLocation::Path(file.0.clone()),
            ),
            SourceProbeLimits::new(8, 4_096).unwrap(),
            FallbackPolicy::Disallow,
            &operation(),
        )
        .unwrap();
    let error = selection
        .open(&operation())
        .err()
        .expect("oversized source should be rejected before residency allocation");
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);

    let fixture = |group: Vec<u8>| {
        let mut segment = info(1.0);
        segment.extend(element(TRACKS, video_track("V_VP9")));
        let mut cluster = unsigned(&[0xE7], 0);
        cluster.extend(element(&[0xA0], group));
        segment.extend(element(CLUSTER, cluster));
        let mut bytes = ebml_header("matroska");
        bytes.extend(element(SEGMENT, segment));
        bytes
    };

    let mut references = element(&[0xA1], block(1, 0, 0, b"frame"));
    for _ in 0..=256 {
        references.extend(signed(&[0xFB], -1));
    }
    assert_eq!(
        open_error_category(fixture(references), "references.mkv"),
        ErrorCategory::ResourceExhausted
    );

    let mut additions = Vec::new();
    for _ in 0..=256 {
        additions.extend(element(&[0xA6], element(&[0xA5], vec![0x01])));
    }
    let mut group = element(&[0xA1], block(1, 0, 0, b"frame"));
    group.extend(element(&[0x75, 0xA1], additions));
    assert_eq!(
        open_error_category(fixture(group), "additions.mkv"),
        ErrorCategory::ResourceExhausted
    );
}

#[test]
fn rejects_duration_at_the_exclusive_u64_nanosecond_boundary() {
    let mut info_payload = unsigned(&[0x2A, 0xD7, 0xB1], 1);
    info_payload.extend(float(&[0x44, 0x89], 18_446_744_073_709_551_616.0));
    let mut segment = element(INFO, info_payload);
    segment.extend(element(TRACKS, video_track("V_VP9")));
    let mut bytes = ebml_header("matroska");
    bytes.extend(element(SEGMENT, segment));
    assert_eq!(
        open_error_category(bytes, "duration-overflow.mkv"),
        ErrorCategory::CorruptData
    );
}

#[test]
fn enforces_relink_cancellation_corruption_and_container_only_boundaries() {
    let bytes = standard_fixture("matroska");
    let first = open(bytes.clone(), "source.mkv");
    let fingerprint = first.info().identity().fingerprint().to_owned();
    let good = request(0x55, "relink.mkv", bytes.clone())
        .with_expected_fingerprint(fingerprint)
        .unwrap();
    registry()
        .probe_source(
            good,
            SourceProbeLimits::default(),
            FallbackPolicy::Disallow,
            &operation(),
        )
        .unwrap()
        .open(&operation())
        .unwrap();

    let bad = request(0x55, "relink.mkv", bytes.clone())
        .with_expected_fingerprint("sha256:not-the-source")
        .unwrap();
    let error = registry()
        .probe_source(
            bad,
            SourceProbeLimits::default(),
            FallbackPolicy::Disallow,
            &operation(),
        )
        .unwrap()
        .open(&operation())
        .err()
        .expect("relink mismatch should fail");
    assert_eq!(error.category(), ErrorCategory::Conflict);

    let token = CancellationToken::new();
    token.cancel();
    let cancelled = OperationContext::new(MediaPriority::Interactive).with_cancellation(token);
    let error = registry()
        .probe_source(
            request(0x55, "cancelled.mkv", bytes),
            SourceProbeLimits::default(),
            FallbackPolicy::Disallow,
            &cancelled,
        )
        .err()
        .expect("cancelled probing should fail");
    assert_eq!(error.category(), ErrorCategory::Cancelled);

    let mut corrupt = ebml_header("matroska");
    corrupt.extend_from_slice(SEGMENT);
    corrupt.push(0x88);
    corrupt.extend_from_slice(&[0x1F, 0x43]);
    let error = registry()
        .probe_source(
            request(0x55, "corrupt.mkv", corrupt),
            SourceProbeLimits::default(),
            FallbackPolicy::Disallow,
            &operation(),
        )
        .unwrap()
        .open(&operation())
        .err()
        .expect("truncated elements should fail");
    assert_eq!(error.category(), ErrorCategory::CorruptData);

    let backend = MkvWebmBackend::new().unwrap();
    let decoder_error = backend
        .create_decoder(
            &superi_media_io::decode::DecoderConfig::new(superi_media_io::demux::StreamInfo::new(
                superi_media_io::demux::StreamId::new(1),
                StreamKind::Video,
                superi_media_io::demux::CodecId::new("vp9").unwrap(),
                Timebase::NANOSECONDS,
            )),
            &operation(),
        )
        .err()
        .expect("container backend must not decode");
    assert_eq!(decoder_error.category(), ErrorCategory::Unsupported);
}
