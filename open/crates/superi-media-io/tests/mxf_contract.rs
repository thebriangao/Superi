use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration as WallDuration;

use superi_core::error::{ErrorCategory, Recoverability};
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
use superi_media_io::mxf::MxfBackend;
use superi_media_io::operation::{MediaPriority, OperationContext};
use superi_media_io::read::ReadOutcome;

const PARTITION_PREFIX: [u8; 13] = [
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01,
];
const PRIMER_KEY: [u8; 16] = [
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x05, 0x01, 0x00,
];
const INDEX_KEY: [u8; 16] = [
    0x06, 0x0e, 0x2b, 0x34, 0x02, 0x53, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x10, 0x01, 0x00,
];
const OP1A: [u8; 16] = [
    0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x01, 0x01, 0x01, 0x00,
];
const GC_CONTAINER: [u8; 16] = [
    0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x01, 0x0d, 0x01, 0x03, 0x01, 0x02, 0x7f, 0x01, 0x00,
];
const PICTURE_DEF: [u8; 16] = [
    0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x01, 0x01, 0x03, 0x02, 0x02, 0x01, 0x00, 0x00, 0x00,
];
const SOUND_DEF: [u8; 16] = [
    0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x01, 0x01, 0x03, 0x02, 0x02, 0x02, 0x00, 0x00, 0x00,
];
const VIDEO_CODEC: [u8; 16] = [
    0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x01, 0x04, 0x01, 0x02, 0x02, 0x71, 0x00, 0x00, 0x00,
];
const AUDIO_CODEC: [u8; 16] = [
    0x06, 0x0e, 0x2b, 0x34, 0x04, 0x01, 0x01, 0x01, 0x04, 0x02, 0x02, 0x01, 0x7e, 0x00, 0x00, 0x00,
];
const VIDEO_TRACK_NUMBER: u32 = 0x1501_0500;
const AUDIO_TRACK_NUMBER: u32 = 0x1601_0100;

fn operation() -> OperationContext {
    OperationContext::new(MediaPriority::Interactive)
        .with_timeout(WallDuration::from_secs(5))
        .unwrap()
}

fn next_packet(source: &mut dyn MediaSource) -> Option<Packet> {
    match source.read_packet(&operation()).unwrap() {
        ReadOutcome::Complete(packet) => Some(packet),
        ReadOutcome::EndOfStream => None,
        ReadOutcome::Partial { .. } => panic!("fixture packets must be complete"),
        _ => panic!("unsupported read outcome"),
    }
}

fn uid(seed: u8) -> [u8; 16] {
    let mut value = [seed; 16];
    value[0] = 0x06;
    value[15] = seed;
    value
}

fn umid(seed: u8) -> [u8; 32] {
    let mut value = [seed; 32];
    value[..12].copy_from_slice(&[
        0x06, 0x0a, 0x2b, 0x34, 0x01, 0x01, 0x01, 0x05, 0x01, 0x01, 0x0f, 0x20,
    ]);
    value[31] = seed;
    value
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn push_i32(bytes: &mut Vec<u8>, value: i32) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn rational(numerator: i32, denominator: i32) -> Vec<u8> {
    let mut bytes = Vec::new();
    push_i32(&mut bytes, numerator);
    push_i32(&mut bytes, denominator);
    bytes
}

fn utf16(value: &str) -> Vec<u8> {
    let mut bytes = Vec::new();
    for unit in value.encode_utf16().chain(std::iter::once(0)) {
        push_u16(&mut bytes, unit);
    }
    bytes
}

fn refs(values: &[[u8; 16]]) -> Vec<u8> {
    let mut bytes = Vec::new();
    push_u32(&mut bytes, values.len() as u32);
    push_u32(&mut bytes, 16);
    for value in values {
        bytes.extend_from_slice(value);
    }
    bytes
}

fn ber_length(value: usize) -> Vec<u8> {
    if value < 0x80 {
        return vec![value as u8];
    }
    let encoded = (value as u64).to_be_bytes();
    let first = encoded.iter().position(|byte| *byte != 0).unwrap();
    let count = encoded.len() - first;
    let mut bytes = vec![0x80 | count as u8];
    bytes.extend_from_slice(&encoded[first..]);
    bytes
}

fn push_klv(file: &mut Vec<u8>, key: [u8; 16], value: &[u8]) -> u64 {
    file.extend_from_slice(&key);
    let length = ber_length(value.len());
    file.extend_from_slice(&length);
    let value_offset = file.len() as u64;
    file.extend_from_slice(value);
    value_offset
}

fn set_key(kind: u8, ber_items: bool) -> [u8; 16] {
    [
        0x06,
        0x0e,
        0x2b,
        0x34,
        0x02,
        if ber_items { 0x13 } else { 0x53 },
        0x01,
        0x01,
        0x0d,
        0x01,
        0x01,
        0x01,
        0x01,
        0x01,
        kind,
        0x00,
    ]
}

fn local_set(items: &[(u16, Vec<u8>)], ber_items: bool) -> Vec<u8> {
    let mut bytes = Vec::new();
    for (tag, value) in items {
        push_u16(&mut bytes, *tag);
        if ber_items {
            bytes.extend_from_slice(&ber_length(value.len()));
        } else {
            push_u16(&mut bytes, value.len() as u16);
        }
        bytes.extend_from_slice(value);
    }
    bytes
}

fn add_set(file: &mut Vec<u8>, kind: u8, items: &[(u16, Vec<u8>)], ber_items: bool) {
    push_klv(file, set_key(kind, ber_items), &local_set(items, ber_items));
}

fn partition_key(kind: u8) -> [u8; 16] {
    let mut key = [0; 16];
    key[..13].copy_from_slice(&PARTITION_PREFIX);
    key[13] = kind;
    key[14] = 0x04;
    key
}

fn partition_value(
    this_partition: u64,
    previous_partition: u64,
    footer_partition: u64,
    header_byte_count: u64,
    index_byte_count: u64,
    index_sid: u32,
    body_sid: u32,
) -> Vec<u8> {
    let mut bytes = Vec::new();
    push_u16(&mut bytes, 1);
    push_u16(&mut bytes, 3);
    push_u32(&mut bytes, 1);
    push_u64(&mut bytes, this_partition);
    push_u64(&mut bytes, previous_partition);
    push_u64(&mut bytes, footer_partition);
    push_u64(&mut bytes, header_byte_count);
    push_u64(&mut bytes, index_byte_count);
    push_u32(&mut bytes, index_sid);
    push_u64(&mut bytes, 0);
    push_u32(&mut bytes, body_sid);
    bytes.extend_from_slice(&OP1A);
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 16);
    bytes.extend_from_slice(&GC_CONTAINER);
    bytes
}

fn essence_key(track_number: u32) -> [u8; 16] {
    let mut key = [
        0x06, 0x0e, 0x2b, 0x34, 0x01, 0x02, 0x01, 0x01, 0x0d, 0x01, 0x03, 0x01, 0, 0, 0, 0,
    ];
    key[12..].copy_from_slice(&track_number.to_be_bytes());
    key
}

fn primer() -> Vec<u8> {
    let mut bytes = Vec::new();
    push_u32(&mut bytes, 1);
    push_u32(&mut bytes, 18);
    push_u16(&mut bytes, 0x8001);
    bytes.extend_from_slice(&[
        0x06, 0x0e, 0x2b, 0x34, 0x01, 0x01, 0x01, 0x02, 0x01, 0x03, 0x01, 0x01, 0x02, 0x01, 0x00,
        0x00,
    ]);
    bytes
}

fn index_segment() -> Vec<u8> {
    let mut entries = Vec::new();
    push_u32(&mut entries, 2);
    push_u32(&mut entries, 11);
    entries.extend_from_slice(&[0, 0, 0x80]);
    push_u64(&mut entries, 0);
    entries.extend_from_slice(&[0, 0xff, 0x20]);
    push_u64(&mut entries, 4);

    local_set(
        &[
            (0x3c0a, uid(0x61).to_vec()),
            (0x3f0b, rational(24, 1)),
            (0x3f0c, 0_i64.to_be_bytes().to_vec()),
            (0x3f0d, 2_i64.to_be_bytes().to_vec()),
            (0x3f05, 0_u32.to_be_bytes().to_vec()),
            (0x3f06, 2_u32.to_be_bytes().to_vec()),
            (0x3f07, 1_u32.to_be_bytes().to_vec()),
            (0x3f08, vec![0]),
            (0x3f0e, vec![0]),
            (0x3f0a, entries),
        ],
        false,
    )
}

struct Fixture {
    bytes: Vec<u8>,
    packet_offsets: [u64; 4],
}

fn fixture(run_in: usize) -> Fixture {
    let mut header_metadata = Vec::new();
    push_klv(&mut header_metadata, PRIMER_KEY, &primer());

    let content_storage = uid(0x10);
    let material_package = uid(0x11);
    let source_package = uid(0x12);
    let essence_container_data = uid(0x22);
    let material_video_track = uid(0x13);
    let material_audio_track = uid(0x14);
    let source_video_track = uid(0x15);
    let source_audio_track = uid(0x16);
    let material_video_sequence = uid(0x17);
    let material_audio_sequence = uid(0x18);
    let source_video_sequence = uid(0x19);
    let source_audio_sequence = uid(0x1a);
    let material_video_clip = uid(0x1b);
    let material_audio_clip = uid(0x1c);
    let source_video_clip = uid(0x1d);
    let source_audio_clip = uid(0x1e);
    let multiple_descriptor = uid(0x1f);
    let video_descriptor = uid(0x20);
    let audio_descriptor = uid(0x21);
    let material_umid = umid(0x31);
    let source_umid = umid(0x32);

    add_set(
        &mut header_metadata,
        0x2f,
        &[
            (0x3c0a, uid(0x01).to_vec()),
            (0x3b03, uid(0x02).to_vec()),
            (0x3b08, material_package.to_vec()),
            (0x3b09, OP1A.to_vec()),
            (0x3b0a, refs(&[GC_CONTAINER])),
            (0x3b07, 1_u32.to_be_bytes().to_vec()),
        ],
        false,
    );
    add_set(
        &mut header_metadata,
        0x18,
        &[
            (0x3c0a, content_storage.to_vec()),
            (0x1901, refs(&[material_package, source_package])),
            (0x1902, refs(&[essence_container_data])),
        ],
        false,
    );
    add_set(
        &mut header_metadata,
        0x23,
        &[
            (0x3c0a, essence_container_data.to_vec()),
            (0x2701, source_umid.to_vec()),
            (0x3f06, 2_u32.to_be_bytes().to_vec()),
            (0x3f07, 1_u32.to_be_bytes().to_vec()),
        ],
        false,
    );
    add_set(
        &mut header_metadata,
        0x36,
        &[
            (0x3c0a, material_package.to_vec()),
            (0x4401, material_umid.to_vec()),
            (0x4402, utf16("Timeline")),
            (0x4403, refs(&[material_video_track, material_audio_track])),
        ],
        false,
    );
    add_set(
        &mut header_metadata,
        0x37,
        &[
            (0x3c0a, source_package.to_vec()),
            (0x4401, source_umid.to_vec()),
            (0x4402, utf16("Camera A")),
            (0x4403, refs(&[source_video_track, source_audio_track])),
            (0x4701, multiple_descriptor.to_vec()),
        ],
        false,
    );

    for (instance, id, name, sequence, number) in [
        (
            material_video_track,
            1_u32,
            "Material picture",
            material_video_sequence,
            0_u32,
        ),
        (
            material_audio_track,
            2_u32,
            "Material sound",
            material_audio_sequence,
            0_u32,
        ),
        (
            source_video_track,
            10_u32,
            "Source picture",
            source_video_sequence,
            VIDEO_TRACK_NUMBER,
        ),
        (
            source_audio_track,
            20_u32,
            "Source sound",
            source_audio_sequence,
            AUDIO_TRACK_NUMBER,
        ),
    ] {
        add_set(
            &mut header_metadata,
            0x3b,
            &[
                (0x3c0a, instance.to_vec()),
                (0x4801, id.to_be_bytes().to_vec()),
                (0x4802, utf16(name)),
                (0x4803, sequence.to_vec()),
                (0x4804, number.to_be_bytes().to_vec()),
                (0x4b01, rational(24, 1)),
                (0x4b02, 0_i64.to_be_bytes().to_vec()),
            ],
            false,
        );
    }

    for (instance, definition, component) in [
        (material_video_sequence, PICTURE_DEF, material_video_clip),
        (material_audio_sequence, SOUND_DEF, material_audio_clip),
        (source_video_sequence, PICTURE_DEF, source_video_clip),
        (source_audio_sequence, SOUND_DEF, source_audio_clip),
    ] {
        add_set(
            &mut header_metadata,
            0x0f,
            &[
                (0x3c0a, instance.to_vec()),
                (0x0201, definition.to_vec()),
                (0x0202, 2_i64.to_be_bytes().to_vec()),
                (0x1001, refs(&[component])),
            ],
            false,
        );
    }

    for (instance, definition, start, package_id, source_track_id) in [
        (material_video_clip, PICTURE_DEF, 3_i64, source_umid, 10_u32),
        (material_audio_clip, SOUND_DEF, 0_i64, source_umid, 20_u32),
        (source_video_clip, PICTURE_DEF, 0_i64, [0; 32], 0_u32),
        (source_audio_clip, SOUND_DEF, 0_i64, [0; 32], 0_u32),
    ] {
        add_set(
            &mut header_metadata,
            0x11,
            &[
                (0x3c0a, instance.to_vec()),
                (0x0201, definition.to_vec()),
                (0x0202, 2_i64.to_be_bytes().to_vec()),
                (0x1201, start.to_be_bytes().to_vec()),
                (0x1101, package_id.to_vec()),
                (0x1102, source_track_id.to_be_bytes().to_vec()),
            ],
            false,
        );
    }

    add_set(
        &mut header_metadata,
        0x44,
        &[
            (0x3c0a, multiple_descriptor.to_vec()),
            (0x3001, rational(24, 1)),
            (0x3002, 2_i64.to_be_bytes().to_vec()),
            (0x3004, GC_CONTAINER.to_vec()),
            (0x3f02, refs(&[video_descriptor, audio_descriptor])),
        ],
        false,
    );
    add_set(
        &mut header_metadata,
        0x28,
        &[
            (0x3c0a, video_descriptor.to_vec()),
            (0x3001, rational(24, 1)),
            (0x3002, 2_i64.to_be_bytes().to_vec()),
            (0x3004, GC_CONTAINER.to_vec()),
            (0x3005, VIDEO_CODEC.to_vec()),
            (0x3006, 10_u32.to_be_bytes().to_vec()),
            (0x3202, 1_080_u32.to_be_bytes().to_vec()),
            (0x3203, 1_920_u32.to_be_bytes().to_vec()),
            (0x320e, rational(16, 9)),
        ],
        false,
    );
    add_set(
        &mut header_metadata,
        0x48,
        &[
            (0x3c0a, audio_descriptor.to_vec()),
            (0x3001, rational(24, 1)),
            (0x3002, 2_i64.to_be_bytes().to_vec()),
            (0x3004, GC_CONTAINER.to_vec()),
            (0x3005, AUDIO_CODEC.to_vec()),
            (0x3006, 20_u32.to_be_bytes().to_vec()),
            (0x3d03, rational(48_000, 1)),
            (0x3d07, 2_u32.to_be_bytes().to_vec()),
            (0x3d01, 24_u32.to_be_bytes().to_vec()),
        ],
        true,
    );

    let dark_key = [
        0x06, 0x0e, 0x2b, 0x34, 0x02, 0x05, 0x01, 0x01, 0x0d, 0x01, 0x02, 0x01, 0x7f, 0x7f, 0x01,
        0x00,
    ];
    push_klv(&mut header_metadata, dark_key, b"dark metadata");

    let index = index_segment();
    let header_pack_len = 16 + 1 + 104;
    let body_pack_len = header_pack_len;
    let footer_pack_len = header_pack_len;
    let header_offset = run_in as u64;
    let body_offset = header_offset + header_pack_len as u64 + header_metadata.len() as u64;
    let essence_sizes = [
        b"V0".as_slice(),
        b"A0".as_slice(),
        b"V1".as_slice(),
        b"A1".as_slice(),
    ]
    .iter()
    .map(|payload| 16 + ber_length(payload.len()).len() + payload.len())
    .sum::<usize>();
    let footer_offset = body_offset + body_pack_len as u64 + essence_sizes as u64;

    let mut bytes = vec![0xa5; run_in];
    push_klv(
        &mut bytes,
        partition_key(0x02),
        &partition_value(
            header_offset,
            0,
            footer_offset,
            header_metadata.len() as u64,
            0,
            0,
            0,
        ),
    );
    bytes.extend_from_slice(&header_metadata);
    push_klv(
        &mut bytes,
        partition_key(0x03),
        &partition_value(body_offset, header_offset, footer_offset, 0, 0, 0, 1),
    );
    let video0 = push_klv(&mut bytes, essence_key(VIDEO_TRACK_NUMBER), b"V0");
    let audio0 = push_klv(&mut bytes, essence_key(AUDIO_TRACK_NUMBER), b"A0");
    let video1 = push_klv(&mut bytes, essence_key(VIDEO_TRACK_NUMBER), b"V1");
    let audio1 = push_klv(&mut bytes, essence_key(AUDIO_TRACK_NUMBER), b"A1");
    push_klv(
        &mut bytes,
        partition_key(0x04),
        &partition_value(
            footer_offset,
            body_offset,
            footer_offset,
            0,
            (16 + ber_length(index.len()).len() + index.len()) as u64,
            2,
            0,
        ),
    );
    push_klv(&mut bytes, INDEX_KEY, &index);

    assert!(bytes.len() >= footer_pack_len);
    Fixture {
        bytes,
        packet_offsets: [video0, audio0, video1, audio1],
    }
}

fn memory_request(media_id: u128, name: &str, bytes: &[u8]) -> SourceRequest {
    SourceRequest::new(
        MediaId::from_raw(media_id),
        SourceLocation::Memory {
            name: name.to_owned(),
            data: Arc::from(bytes),
        },
    )
}

fn open_through_registry(request: &SourceRequest) -> Box<dyn MediaSource> {
    let backend = Arc::new(MxfBackend::new().unwrap());
    let mut registry = BackendRegistry::new();
    registry
        .register(
            BackendRegistration::new(
                backend,
                BackendCapabilities::new([BackendCapability::Source]),
                100,
                BackendTier::Primary,
            )
            .unwrap(),
        )
        .unwrap();
    let selection = registry
        .probe_source(
            request.clone(),
            SourceProbeLimits::default(),
            FallbackPolicy::Disallow,
            &operation(),
        )
        .unwrap();
    assert_eq!(
        selection.primary().backend().descriptor().id().as_str(),
        "mxf"
    );
    assert_eq!(selection.primary().container().as_str(), "mxf");
    selection.open(&operation()).unwrap()
}

#[test]
fn mxf_demux_preserves_streams_timing_edits_offsets_and_metadata() {
    let fixture = fixture(0);
    let request = memory_request(0x44, "camera.mxf", &fixture.bytes);
    let mut source = open_through_registry(&request);
    let info = source.info();

    assert_eq!(info.identity().media_id(), MediaId::from_raw(0x44));
    assert!(info.identity().fingerprint().starts_with("sha256:"));
    assert_eq!(info.duration().unwrap().value(), 2);
    assert_eq!(
        info.duration().unwrap().timebase(),
        Timebase::integer(24).unwrap()
    );
    assert_eq!(
        info.metadata().get("container.kind"),
        Some(&MetadataValue::Text("mxf".into()))
    );
    assert_eq!(
        info.metadata().get("mxf.operational-pattern"),
        Some(&MetadataValue::Text(
            "060e2b34040101010d01020101010100".into()
        ))
    );
    assert_eq!(
        info.metadata().get("mxf.material-package-name"),
        Some(&MetadataValue::Text("Timeline".into()))
    );
    assert_eq!(
        info.metadata().get("mxf.partition-count"),
        Some(&MetadataValue::Unsigned(3))
    );
    assert_eq!(
        info.metadata().get("mxf.dark-klv-count"),
        Some(&MetadataValue::Unsigned(1))
    );

    assert_eq!(info.streams().len(), 2);
    let video = &info.streams()[0];
    assert_eq!(video.id().value(), 10);
    assert_eq!(video.kind(), StreamKind::Video);
    assert_eq!(video.timebase(), Timebase::integer(24).unwrap());
    assert_eq!(video.duration().unwrap().value(), 2);
    assert_eq!(video.edits().len(), 1);
    assert_eq!(video.edits()[0].segment_duration().value(), 2);
    assert_eq!(video.edits()[0].media_time().unwrap().value(), 3);
    assert_eq!(
        video.metadata().get("mxf.track-number"),
        Some(&MetadataValue::Unsigned(u64::from(VIDEO_TRACK_NUMBER)))
    );
    assert_eq!(
        video.metadata().get("video.width"),
        Some(&MetadataValue::Unsigned(1_920))
    );
    assert_eq!(
        video.metadata().get("video.height"),
        Some(&MetadataValue::Unsigned(1_080))
    );

    let audio = &info.streams()[1];
    assert_eq!(audio.id().value(), 20);
    assert_eq!(audio.kind(), StreamKind::Audio);
    assert_eq!(audio.timebase(), Timebase::integer(24).unwrap());
    assert_eq!(
        audio.metadata().get("audio.sample-rate-numerator"),
        Some(&MetadataValue::Unsigned(48_000))
    );
    assert_eq!(
        audio.metadata().get("audio.channel-count"),
        Some(&MetadataValue::Unsigned(2))
    );

    let packets = std::iter::from_fn(|| next_packet(source.as_mut())).collect::<Vec<_>>();
    assert_eq!(packets.len(), 4);
    assert_eq!(
        packets
            .iter()
            .map(|packet| packet.stream_id().value())
            .collect::<Vec<_>>(),
        [10, 20, 10, 20]
    );
    assert_eq!(packets[0].data(), b"V0");
    assert_eq!(packets[0].timing().presentation_time().unwrap().value(), 0);
    assert_eq!(packets[0].timing().decode_time().unwrap().value(), 0);
    assert_eq!(packets[0].timing().duration().unwrap().value(), 1);
    assert!(packets[0].is_keyframe());
    assert_eq!(
        packets[0].metadata().get("container.offset"),
        Some(&MetadataValue::Unsigned(fixture.packet_offsets[0]))
    );
    assert_eq!(
        packets[0].metadata().get("mxf.body-sid"),
        Some(&MetadataValue::Unsigned(1))
    );
    assert_eq!(packets[1].data(), b"A0");
    assert_eq!(packets[2].timing().presentation_time().unwrap().value(), 1);
    assert!(!packets[2].is_keyframe());
    assert_eq!(
        packets[2].metadata().get("mxf.key-frame-offset"),
        Some(&MetadataValue::Signed(-1))
    );
}

#[test]
fn run_in_content_probe_seek_and_relink_are_predictable() {
    let fixture = fixture(127);
    let request = memory_request(0x55, "misleading.bin", &fixture.bytes);
    let mut source = open_through_registry(&request);
    assert_eq!(
        source.info().metadata().get("mxf.run-in-bytes"),
        Some(&MetadataValue::Unsigned(127))
    );

    let actual = source
        .seek(
            SeekRequest::new(
                RationalTime::new(1, Timebase::integer(24).unwrap()),
                SeekMode::PreviousKeyframe,
            ),
            &operation(),
        )
        .unwrap();
    assert_eq!(actual.value(), 0);
    assert_eq!(next_packet(source.as_mut()).unwrap().data(), b"V0");

    let backend = MxfBackend::new().unwrap();
    let fingerprint = source.info().identity().fingerprint().to_owned();
    let matching = request
        .clone()
        .with_expected_fingerprint(fingerprint)
        .unwrap();
    assert!(backend.open_source(&matching, &operation()).is_ok());

    let mismatched = request
        .with_expected_fingerprint("sha256:not-the-same-content")
        .unwrap();
    let error = match backend.open_source(&mismatched, &operation()) {
        Ok(_) => panic!("a relink with different content must fail"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
}

#[test]
fn path_ingest_uses_the_same_container_contract() {
    let fixture = fixture(0);
    let unique = format!("superi-mxf-{}-fixture.mxf", std::process::id());
    let path = std::env::temp_dir().join(unique);
    std::fs::write(&path, &fixture.bytes).unwrap();
    let request = SourceRequest::new(
        MediaId::from_raw(0x66),
        SourceLocation::Path(PathBuf::from(&path)),
    );
    let backend = MxfBackend::new().unwrap();
    let mut source = backend.open_source(&request, &operation()).unwrap();
    assert_eq!(next_packet(source.as_mut()).unwrap().data(), b"V0");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn unrelated_bytes_do_not_probe_as_mxf() {
    let request = memory_request(0x77, "not-really.mxf", b"not an MXF file");
    let backend = Arc::new(MxfBackend::new().unwrap());
    let mut registry = BackendRegistry::new();
    registry
        .register(
            BackendRegistration::new(
                backend,
                BackendCapabilities::new([BackendCapability::Source]),
                100,
                BackendTier::Primary,
            )
            .unwrap(),
        )
        .unwrap();
    let error = match registry.probe_source(
        request,
        SourceProbeLimits::default(),
        FallbackPolicy::Disallow,
        &operation(),
    ) {
        Ok(_) => panic!("unrelated bytes must not select MXF"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::Unsupported);
}

#[test]
fn every_truncated_klv_prefix_fails_without_panicking() {
    let fixture = fixture(0);
    let backend = MxfBackend::new().unwrap();
    for length in 0..fixture.bytes.len() {
        let bytes = &fixture.bytes[..length];
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            backend.open_source(&memory_request(1, "prefix.mxf", bytes), &operation())
        }));
        assert!(outcome.is_ok(), "opening a {length}-byte prefix panicked");
        assert!(
            outcome.unwrap().is_err(),
            "a truncated {length}-byte prefix unexpectedly opened"
        );
    }
}

#[test]
fn cancelled_operations_do_not_open_consume_or_seek_media() {
    let fixture = fixture(0);
    let request = memory_request(0x88, "cancelled.mxf", &fixture.bytes);
    let backend = MxfBackend::new().unwrap();

    let cancelled_open = operation();
    cancelled_open.cancellation_token().cancel();
    let error = match backend.open_source(&request, &cancelled_open) {
        Ok(_) => panic!("cancelled source open must fail"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::Cancelled);

    let mut source = backend.open_source(&request, &operation()).unwrap();
    let cancelled_read = operation();
    cancelled_read.cancellation_token().cancel();
    assert_eq!(
        source.read_packet(&cancelled_read).unwrap_err().category(),
        ErrorCategory::Cancelled
    );
    assert_eq!(next_packet(source.as_mut()).unwrap().data(), b"V0");

    let mut source = backend.open_source(&request, &operation()).unwrap();
    let cancelled_seek = operation();
    cancelled_seek.cancellation_token().cancel();
    let error = source
        .seek(
            SeekRequest::new(
                RationalTime::new(1, Timebase::integer(24).unwrap()),
                SeekMode::Exact,
            ),
            &cancelled_seek,
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(next_packet(source.as_mut()).unwrap().data(), b"V0");
}
