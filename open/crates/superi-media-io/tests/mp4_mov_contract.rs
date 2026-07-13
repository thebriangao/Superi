use std::path::PathBuf;
use std::sync::Arc;

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
use superi_media_io::mp4_mov::Mp4MovBackend;
use superi_media_io::operation::{MediaPriority, OperationContext};
use superi_media_io::read::ReadOutcome;

fn operation() -> OperationContext {
    OperationContext::new(MediaPriority::Interactive)
}

fn next_packet(source: &mut dyn MediaSource) -> Option<Packet> {
    match source.read_packet(&operation()).unwrap() {
        ReadOutcome::Complete(packet) => Some(packet),
        ReadOutcome::EndOfStream => None,
        ReadOutcome::Partial { .. } => panic!("MP4 and MOV packets are atomic"),
        _ => panic!("unknown MP4 or MOV packet outcome"),
    }
}

#[derive(Clone, Copy)]
enum FixtureBrand {
    Mp4,
    Mov,
}

struct Fixture {
    bytes: Vec<u8>,
    video_offset: u64,
    audio_offset: u64,
}

fn push_u16(bytes: &mut Vec<u8>, value: u16) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn push_i16(bytes: &mut Vec<u8>, value: i16) {
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

fn atom(kind: &[u8; 4], payload: Vec<u8>) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(payload.len() + 8);
    push_u32(&mut bytes, u32::try_from(payload.len() + 8).unwrap());
    bytes.extend_from_slice(kind);
    bytes.extend(payload);
    bytes
}

fn full_atom(kind: &[u8; 4], version: u8, flags: u32, payload: Vec<u8>) -> Vec<u8> {
    let mut full = Vec::with_capacity(payload.len() + 4);
    full.push(version);
    full.extend_from_slice(&flags.to_be_bytes()[1..]);
    full.extend(payload);
    atom(kind, full)
}

fn identity_matrix(bytes: &mut Vec<u8>) {
    for value in [0x0001_0000_i32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
        push_i32(bytes, value);
    }
}

fn movie_header() -> Vec<u8> {
    let mut payload = Vec::new();
    push_u32(&mut payload, 11);
    push_u32(&mut payload, 12);
    push_u32(&mut payload, 1_000);
    push_u32(&mut payload, 2_000);
    push_u32(&mut payload, 0x0001_0000);
    push_u16(&mut payload, 0x0100);
    push_u16(&mut payload, 0);
    push_u64(&mut payload, 0);
    identity_matrix(&mut payload);
    payload.extend_from_slice(&[0; 24]);
    push_u32(&mut payload, 3);
    full_atom(b"mvhd", 0, 0, payload)
}

fn track_header(track_id: u32, width: u16, height: u16, volume: u16) -> Vec<u8> {
    let mut payload = Vec::new();
    push_u32(&mut payload, 21);
    push_u32(&mut payload, 22);
    push_u32(&mut payload, track_id);
    push_u32(&mut payload, 0);
    push_u32(&mut payload, 2_000);
    push_u64(&mut payload, 0);
    push_u16(&mut payload, if track_id == 1 { 2 } else { 0 });
    push_u16(&mut payload, if track_id == 1 { 7 } else { 0 });
    push_u16(&mut payload, volume);
    push_u16(&mut payload, 0);
    identity_matrix(&mut payload);
    push_u32(&mut payload, u32::from(width) << 16);
    push_u32(&mut payload, u32::from(height) << 16);
    full_atom(b"tkhd", 0, 7, payload)
}

fn edit_list() -> Vec<u8> {
    let mut payload = Vec::new();
    push_u32(&mut payload, 2);
    push_u32(&mut payload, 500);
    push_i32(&mut payload, -1);
    push_i16(&mut payload, 1);
    push_i16(&mut payload, 0);
    push_u32(&mut payload, 1_500);
    push_i32(&mut payload, 100);
    push_i16(&mut payload, 1);
    push_i16(&mut payload, 0);
    atom(b"edts", full_atom(b"elst", 0, 0, payload))
}

fn media_header(timescale: u32) -> Vec<u8> {
    let mut payload = Vec::new();
    push_u32(&mut payload, 31);
    push_u32(&mut payload, 32);
    push_u32(&mut payload, timescale);
    push_u32(&mut payload, timescale * 2);
    let language = (5_u16 << 10) | (14_u16 << 5) | 7_u16;
    push_u16(&mut payload, language);
    push_u16(&mut payload, 0);
    full_atom(b"mdhd", 0, 0, payload)
}

fn handler(kind: &[u8; 4], name: &str) -> Vec<u8> {
    let mut payload = Vec::new();
    push_u32(&mut payload, 0);
    payload.extend_from_slice(kind);
    payload.extend_from_slice(&[0; 12]);
    payload.extend_from_slice(name.as_bytes());
    payload.push(0);
    full_atom(b"hdlr", 0, 0, payload)
}

fn data_information() -> Vec<u8> {
    let url = full_atom(b"url ", 0, 1, Vec::new());
    let mut dref_payload = Vec::new();
    push_u32(&mut dref_payload, 1);
    dref_payload.extend(url);
    atom(b"dinf", full_atom(b"dref", 0, 0, dref_payload))
}

fn sample_description(codec: &[u8; 4]) -> Vec<u8> {
    let mut payload = Vec::new();
    push_u32(&mut payload, 1);
    payload.extend(atom(codec, Vec::new()));
    full_atom(b"stsd", 0, 0, payload)
}

fn sample_table(
    codec: &[u8; 4],
    sample_delta: u32,
    chunk_offset: u32,
    composition_offsets: Option<[i32; 2]>,
) -> Vec<u8> {
    let mut stts_payload = Vec::new();
    push_u32(&mut stts_payload, 1);
    push_u32(&mut stts_payload, 2);
    push_u32(&mut stts_payload, sample_delta);

    let ctts = composition_offsets.map(|offsets| {
        let mut payload = Vec::new();
        push_u32(&mut payload, 2);
        for offset in offsets {
            push_u32(&mut payload, 1);
            push_i32(&mut payload, offset);
        }
        full_atom(b"ctts", 0, 0, payload)
    });

    let mut stss_payload = Vec::new();
    push_u32(&mut stss_payload, 1);
    push_u32(&mut stss_payload, 1);

    let mut stsc_payload = Vec::new();
    push_u32(&mut stsc_payload, 1);
    push_u32(&mut stsc_payload, 1);
    push_u32(&mut stsc_payload, 2);
    push_u32(&mut stsc_payload, 1);

    let mut stsz_payload = Vec::new();
    push_u32(&mut stsz_payload, 0);
    push_u32(&mut stsz_payload, 2);
    push_u32(&mut stsz_payload, 2);
    push_u32(&mut stsz_payload, 2);

    let mut stco_payload = Vec::new();
    push_u32(&mut stco_payload, 1);
    push_u32(&mut stco_payload, chunk_offset);

    let mut payload = Vec::new();
    payload.extend(sample_description(codec));
    payload.extend(full_atom(b"stts", 0, 0, stts_payload));
    if let Some(ctts) = ctts {
        payload.extend(ctts);
    }
    payload.extend(full_atom(b"stss", 0, 0, stss_payload));
    payload.extend(full_atom(b"stsc", 0, 0, stsc_payload));
    payload.extend(full_atom(b"stsz", 0, 0, stsz_payload));
    payload.extend(full_atom(b"stco", 0, 0, stco_payload));
    atom(b"stbl", payload)
}

fn media_box(
    handler_type: &[u8; 4],
    handler_name: &str,
    codec: &[u8; 4],
    timescale: u32,
    offset: u32,
    composition_offsets: Option<[i32; 2]>,
) -> Vec<u8> {
    let media_header_atom = if handler_type == b"vide" {
        full_atom(b"vmhd", 0, 1, vec![0; 8])
    } else {
        full_atom(b"smhd", 0, 0, vec![0; 4])
    };
    let mut minf_payload = media_header_atom;
    minf_payload.extend(data_information());
    minf_payload.extend(sample_table(codec, timescale, offset, composition_offsets));

    let mut payload = media_header(timescale);
    payload.extend(handler(handler_type, handler_name));
    payload.extend(atom(b"minf", minf_payload));
    atom(b"mdia", payload)
}

fn track(
    track_id: u32,
    handler_type: &[u8; 4],
    handler_name: &str,
    codec: &[u8; 4],
    timescale: u32,
    offset: u32,
    composition_offsets: Option<[i32; 2]>,
) -> Vec<u8> {
    let is_video = handler_type == b"vide";
    let mut payload = track_header(
        track_id,
        if is_video { 1_920 } else { 0 },
        if is_video { 1_080 } else { 0 },
        if is_video { 0 } else { 0x0100 },
    );
    if is_video {
        payload.extend(edit_list());
    }
    payload.extend(media_box(
        handler_type,
        handler_name,
        codec,
        timescale,
        offset,
        composition_offsets,
    ));
    atom(b"trak", payload)
}

fn empty_sample_table(codec: &[u8; 4]) -> Vec<u8> {
    let mut count_zero = Vec::new();
    push_u32(&mut count_zero, 0);
    let mut stsz = Vec::new();
    push_u32(&mut stsz, 0);
    push_u32(&mut stsz, 0);
    let mut payload = sample_description(codec);
    payload.extend(full_atom(b"stts", 0, 0, count_zero.clone()));
    payload.extend(full_atom(b"stsc", 0, 0, count_zero.clone()));
    payload.extend(full_atom(b"stsz", 0, 0, stsz));
    payload.extend(full_atom(b"stco", 0, 0, count_zero));
    atom(b"stbl", payload)
}

fn fragmented_track() -> Vec<u8> {
    let mut minf = full_atom(b"vmhd", 0, 1, vec![0; 8]);
    minf.extend(data_information());
    minf.extend(empty_sample_table(b"vxyz"));
    let mut mdia = media_header(1_000);
    mdia.extend(handler(b"vide", "Fragmented Picture Handler"));
    mdia.extend(atom(b"minf", minf));
    let mut payload = track_header(1, 1_920, 1_080, 0);
    payload.extend(atom(b"mdia", mdia));
    atom(b"trak", payload)
}

fn fragmented_fixture() -> Fixture {
    let mut ftyp_payload = Vec::new();
    ftyp_payload.extend_from_slice(b"isom");
    push_u32(&mut ftyp_payload, 512);
    ftyp_payload.extend_from_slice(b"isom");
    let ftyp = atom(b"ftyp", ftyp_payload);

    let mut trex = Vec::new();
    push_u32(&mut trex, 1);
    push_u32(&mut trex, 1);
    push_u32(&mut trex, 1_000);
    push_u32(&mut trex, 2);
    push_u32(&mut trex, 0);
    let mvex = atom(b"mvex", full_atom(b"trex", 0, 0, trex));
    let mut moov_payload = movie_header();
    moov_payload.extend(fragmented_track());
    moov_payload.extend(mvex);
    let moov = atom(b"moov", moov_payload);

    let build_moof = |data_offset: i32| {
        let mut mfhd = Vec::new();
        push_u32(&mut mfhd, 1);
        let mut tfhd = Vec::new();
        push_u32(&mut tfhd, 1);
        push_u32(&mut tfhd, 1_000);
        push_u32(&mut tfhd, 2);
        push_u32(&mut tfhd, 0);
        let mut tfdt = Vec::new();
        push_u32(&mut tfdt, 0);
        let mut trun = Vec::new();
        push_u32(&mut trun, 1);
        push_i32(&mut trun, data_offset);
        let mut traf = full_atom(b"tfhd", 0, 0x020038, tfhd);
        traf.extend(full_atom(b"tfdt", 0, 0, tfdt));
        traf.extend(full_atom(b"trun", 0, 1, trun));
        let mut moof_payload = full_atom(b"mfhd", 0, 0, mfhd);
        moof_payload.extend(atom(b"traf", traf));
        atom(b"moof", moof_payload)
    };
    let placeholder = build_moof(0);
    let moof = build_moof(i32::try_from(placeholder.len() + 8).unwrap());
    let video_offset = u64::try_from(ftyp.len() + moov.len() + moof.len() + 8).unwrap();
    let mdat = atom(b"mdat", b"V0".to_vec());
    let mut bytes = ftyp;
    bytes.extend(moov);
    bytes.extend(moof);
    bytes.extend(mdat);
    Fixture {
        bytes,
        video_offset,
        audio_offset: video_offset,
    }
}

fn fixture(brand: FixtureBrand) -> Fixture {
    fixture_with_file_type(brand, true)
}

fn fixture_with_file_type(brand: FixtureBrand, include_file_type: bool) -> Fixture {
    let major_brand = match brand {
        FixtureBrand::Mp4 => b"isom",
        FixtureBrand::Mov => b"qt  ",
    };
    let mut ftyp_payload = Vec::new();
    ftyp_payload.extend_from_slice(major_brand);
    push_u32(&mut ftyp_payload, 512);
    ftyp_payload.extend_from_slice(major_brand);
    ftyp_payload.extend_from_slice(b"iso2");
    let ftyp = atom(b"ftyp", ftyp_payload);

    let mdat = atom(b"mdat", b"V0V1A0A1".to_vec());
    let prefix_length = if include_file_type { ftyp.len() } else { 0 };
    let video_offset = u64::try_from(prefix_length + 8).unwrap();
    let audio_offset = video_offset + 4;

    let mut moov_payload = movie_header();
    moov_payload.extend(track(
        1,
        b"vide",
        "Picture Handler",
        b"vxyz",
        1_000,
        u32::try_from(video_offset).unwrap(),
        Some([100, 0]),
    ));
    moov_payload.extend(track(
        2,
        b"soun",
        "Sound Handler",
        b"axyz",
        48_000,
        u32::try_from(audio_offset).unwrap(),
        None,
    ));
    let moov = atom(b"moov", moov_payload);

    let mut bytes = if include_file_type { ftyp } else { Vec::new() };
    bytes.extend(mdat);
    bytes.extend(moov);
    Fixture {
        bytes,
        video_offset,
        audio_offset,
    }
}

fn vfr_mov_fixture() -> Fixture {
    let mut ftyp_payload = Vec::new();
    ftyp_payload.extend_from_slice(b"qt  ");
    push_u32(&mut ftyp_payload, 512);
    ftyp_payload.extend_from_slice(b"qt  ");
    let ftyp = atom(b"ftyp", ftyp_payload);
    let mdat = atom(b"mdat", b"V0V1V2".to_vec());
    let video_offset = u64::try_from(ftyp.len() + 8).unwrap();

    let mut stts = Vec::new();
    push_u32(&mut stts, 3);
    for duration in [40, 60, 100] {
        push_u32(&mut stts, 1);
        push_u32(&mut stts, duration);
    }
    let mut stss = Vec::new();
    push_u32(&mut stss, 1);
    push_u32(&mut stss, 1);
    let mut stsc = Vec::new();
    push_u32(&mut stsc, 1);
    push_u32(&mut stsc, 1);
    push_u32(&mut stsc, 3);
    push_u32(&mut stsc, 1);
    let mut stsz = Vec::new();
    push_u32(&mut stsz, 0);
    push_u32(&mut stsz, 3);
    for _ in 0..3 {
        push_u32(&mut stsz, 2);
    }
    let mut stco = Vec::new();
    push_u32(&mut stco, 1);
    push_u32(&mut stco, u32::try_from(video_offset).unwrap());
    let mut stbl = sample_description(b"vxyz");
    stbl.extend(full_atom(b"stts", 0, 0, stts));
    stbl.extend(full_atom(b"stss", 0, 0, stss));
    stbl.extend(full_atom(b"stsc", 0, 0, stsc));
    stbl.extend(full_atom(b"stsz", 0, 0, stsz));
    stbl.extend(full_atom(b"stco", 0, 0, stco));
    let mut minf = full_atom(b"vmhd", 0, 1, vec![0; 8]);
    minf.extend(data_information());
    minf.extend(atom(b"stbl", stbl));
    let mut mdia = media_header(1_000);
    mdia.extend(handler(b"vide", "VFR Picture Handler"));
    mdia.extend(atom(b"minf", minf));
    let mut trak = track_header(1, 1_920, 1_080, 0);
    trak.extend(atom(b"mdia", mdia));
    let mut moov = movie_header();
    moov.extend(atom(b"trak", trak));

    let mut bytes = ftyp;
    bytes.extend(mdat);
    bytes.extend(atom(b"moov", moov));
    Fixture {
        bytes,
        video_offset,
        audio_offset: video_offset,
    }
}

fn memory_request(media_id: u128, name: &str, bytes: &[u8]) -> SourceRequest {
    SourceRequest::new(
        MediaId::from_raw(media_id),
        SourceLocation::Memory {
            name: name.into(),
            data: Arc::from(bytes),
        },
    )
}

fn open_through_registry(
    request: &SourceRequest,
    expected_container: &str,
) -> Box<dyn MediaSource> {
    let backend = Arc::new(Mp4MovBackend::new().unwrap());
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
        "mp4-mov"
    );
    assert_eq!(selection.primary().container().as_str(), expected_container);
    selection.open(&operation()).unwrap()
}

#[test]
fn mp4_demux_preserves_tracks_edits_timestamps_offsets_and_metadata() {
    let fixture = fixture(FixtureBrand::Mp4);
    let request = memory_request(0x44, "camera.mp4", &fixture.bytes);
    let mut source = open_through_registry(&request, "mp4");
    let info = source.info();

    assert_eq!(info.identity().media_id(), MediaId::from_raw(0x44));
    assert!(info.identity().fingerprint().starts_with("sha256:"));
    assert_eq!(info.duration().unwrap().value(), 2_000);
    assert_eq!(info.duration().unwrap().timebase(), Timebase::MILLISECONDS);
    assert_eq!(
        info.metadata().get("container.kind"),
        Some(&MetadataValue::Text("mp4".into()))
    );
    assert_eq!(
        info.metadata().get("container.major-brand"),
        Some(&MetadataValue::Text("isom".into()))
    );
    assert_eq!(
        info.metadata().get("container.compatible-brands"),
        Some(&MetadataValue::Text("isom,iso2".into()))
    );
    assert_eq!(
        info.metadata().get("container.creation-time"),
        Some(&MetadataValue::Unsigned(11))
    );

    assert_eq!(info.streams().len(), 2);
    let video = &info.streams()[0];
    assert_eq!(video.id().value(), 1);
    assert_eq!(video.kind(), StreamKind::Video);
    assert_eq!(video.codec().as_str(), "fourcc-7678797a");
    assert_eq!(video.timebase(), Timebase::integer(1_000).unwrap());
    assert_eq!(video.duration().unwrap().value(), 1_900);
    assert_eq!(
        video.metadata().get("timeline.presentation-frame-count"),
        Some(&MetadataValue::Unsigned(2))
    );
    assert_eq!(
        video.metadata().get("timeline.variable-frame-rate"),
        Some(&MetadataValue::Boolean(true))
    );
    assert_eq!(video.edits().len(), 2);
    assert_eq!(video.edits()[0].segment_duration().value(), 500);
    assert!(video.edits()[0].media_time().is_none());
    assert_eq!(video.edits()[0].rate_integer(), 1);
    assert_eq!(video.edits()[0].rate_fraction(), 0);
    assert_eq!(video.edits()[1].segment_duration().value(), 1_500);
    assert_eq!(video.edits()[1].media_time().unwrap().value(), 0);
    assert_eq!(
        video.metadata().get("timeline.timestamp-origin"),
        Some(&MetadataValue::Signed(100))
    );
    assert_eq!(
        video.metadata().get("container.handler-name"),
        Some(&MetadataValue::Text("Picture Handler".into()))
    );
    assert_eq!(
        video.metadata().get("track.language"),
        Some(&MetadataValue::Text("eng".into()))
    );
    assert_eq!(
        video.metadata().get("video.width"),
        Some(&MetadataValue::Unsigned(1_920))
    );
    assert_eq!(
        video.metadata().get("track.alternate-group"),
        Some(&MetadataValue::Unsigned(7))
    );

    let audio = &info.streams()[1];
    assert_eq!(audio.id().value(), 2);
    assert_eq!(audio.kind(), StreamKind::Audio);
    assert_eq!(audio.timebase(), Timebase::integer(48_000).unwrap());
    assert_eq!(audio.duration().unwrap().value(), 96_000);
    assert!(audio.edits().is_empty());

    let packets = std::iter::from_fn(|| next_packet(source.as_mut())).collect::<Vec<_>>();
    assert_eq!(packets.len(), 4);
    assert_eq!(
        packets
            .iter()
            .map(|packet| packet.stream_id().value())
            .collect::<Vec<_>>(),
        [1, 2, 1, 2]
    );
    assert_eq!(packets[0].data(), b"V0");
    assert_eq!(packets[0].timing().presentation_time().unwrap().value(), 0);
    assert_eq!(packets[0].timing().decode_time().unwrap().value(), -100);
    assert_eq!(packets[0].timing().duration().unwrap().value(), 1_000);
    assert!(packets[0].is_keyframe());
    assert_eq!(
        packets[0].metadata().get("container.offset"),
        Some(&MetadataValue::Unsigned(fixture.video_offset))
    );
    assert_eq!(
        packets[0].metadata().get("container.composition-offset"),
        Some(&MetadataValue::Signed(100))
    );
    assert_eq!(
        packets[0]
            .metadata()
            .get("container.presentation-timestamp"),
        Some(&MetadataValue::Signed(100))
    );
    assert_eq!(
        packets[0].metadata().get("container.decode-timestamp"),
        Some(&MetadataValue::Signed(0))
    );
    assert_eq!(packets[1].data(), b"A0");
    assert_eq!(
        packets[1].metadata().get("container.offset"),
        Some(&MetadataValue::Unsigned(fixture.audio_offset))
    );
    assert_eq!(
        packets[2].timing().presentation_time().unwrap().value(),
        900
    );
    assert_eq!(
        packets[3].timing().presentation_time().unwrap().value(),
        48_000
    );
}

#[test]
fn content_probe_opens_classic_mov_without_file_type_or_extension_hints() {
    let fixture = fixture_with_file_type(FixtureBrand::Mov, false);
    let request = memory_request(0x66, "misleading.bin", &fixture.bytes);
    let mut source = open_through_registry(&request, "mov");
    assert_eq!(
        source.info().metadata().get("container.kind"),
        Some(&MetadataValue::Text("mov".into()))
    );
    assert_eq!(
        source.info().metadata().get("container.major-brand"),
        Some(&MetadataValue::Text("qt  ".into()))
    );
    assert_eq!(next_packet(source.as_mut()).unwrap().data(), b"V0");
}

#[test]
fn content_probe_rejects_unrelated_iso_base_media_brands() {
    let mut ftyp = Vec::new();
    ftyp.extend_from_slice(b"avif");
    push_u32(&mut ftyp, 0);
    ftyp.extend_from_slice(b"mif1");
    let bytes = atom(b"ftyp", ftyp);
    let backend = Arc::new(Mp4MovBackend::new().unwrap());
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
        memory_request(0x88, "still.mp4", &bytes),
        SourceProbeLimits::default(),
        FallbackPolicy::Disallow,
        &operation(),
    ) {
        Ok(_) => panic!("an AVIF file must not probe as MP4 video"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::Unsupported);
}

#[test]
fn fragmented_mp4_preserves_fragment_sample_offsets_and_timing() {
    let fixture = fragmented_fixture();
    let request = memory_request(0x77, "fragmented.mp4", &fixture.bytes);
    let mut source = open_through_registry(&request, "mp4");
    assert_eq!(
        source.info().metadata().get("container.fragmented"),
        Some(&MetadataValue::Boolean(true))
    );
    let packet = next_packet(source.as_mut()).unwrap();
    assert_eq!(packet.data(), b"V0");
    assert_eq!(packet.timing().decode_time().unwrap().value(), 0);
    assert_eq!(packet.timing().duration().unwrap().value(), 1_000);
    assert_eq!(
        packet.metadata().get("container.offset"),
        Some(&MetadataValue::Unsigned(fixture.video_offset))
    );
    assert!(next_packet(source.as_mut()).is_none());
}

#[test]
fn vfr_mov_uses_mapped_duration_and_frame_boundaries_end_to_end() {
    let fixture = vfr_mov_fixture();
    let request = memory_request(0xc9, "variable.mov", &fixture.bytes);
    let mut source = open_through_registry(&request, "mov");
    assert_eq!(source.info().duration().unwrap().value(), 200);
    let video = &source.info().streams()[0];
    assert_eq!(video.duration().unwrap().value(), 200);
    assert_eq!(
        video.metadata().get("timeline.presentation-frame-count"),
        Some(&MetadataValue::Unsigned(3))
    );
    assert_eq!(
        video.metadata().get("timeline.variable-frame-rate"),
        Some(&MetadataValue::Boolean(true))
    );

    let packets = std::iter::from_fn(|| next_packet(source.as_mut())).collect::<Vec<_>>();
    assert_eq!(
        packets
            .iter()
            .map(|packet| packet.timing().duration().unwrap().value())
            .collect::<Vec<_>>(),
        [40, 60, 100]
    );

    let mut source = open_through_registry(&request, "mov");
    let actual = source
        .seek(
            SeekRequest::new(
                RationalTime::new(40, Timebase::MILLISECONDS),
                SeekMode::Exact,
            ),
            &operation(),
        )
        .unwrap();
    assert_eq!(actual.value(), 40);
    assert_eq!(next_packet(source.as_mut()).unwrap().data(), b"V0");
    assert_eq!(next_packet(source.as_mut()).unwrap().data(), b"V1");

    let error = source
        .seek(
            SeekRequest::new(
                RationalTime::new(50, Timebase::MILLISECONDS),
                SeekMode::Exact,
            ),
            &operation(),
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
}

#[test]
fn mov_path_ingest_seek_and_relink_are_predictable() {
    let fixture = fixture(FixtureBrand::Mov);
    let unique = format!("superi-mp4-mov-{}-fixture.mov", std::process::id());
    let path = std::env::temp_dir().join(unique);
    std::fs::write(&path, &fixture.bytes).unwrap();
    let request = SourceRequest::new(
        MediaId::from_raw(0x55),
        SourceLocation::Path(PathBuf::from(&path)),
    );
    let backend = Mp4MovBackend::new().unwrap();
    let mut source = backend.open_source(&request, &operation()).unwrap();
    assert_eq!(
        source.info().metadata().get("container.kind"),
        Some(&MetadataValue::Text("mov".into()))
    );
    assert_eq!(
        source.info().metadata().get("container.major-brand"),
        Some(&MetadataValue::Text("qt  ".into()))
    );

    let actual = source
        .seek(
            SeekRequest::new(
                RationalTime::new(950, Timebase::integer(1_000).unwrap()),
                SeekMode::PreviousKeyframe,
            ),
            &operation(),
        )
        .unwrap();
    assert_eq!(actual.value(), 500);
    let packet = next_packet(source.as_mut()).unwrap();
    assert_eq!(packet.stream_id().value(), 1);
    assert_eq!(packet.data(), b"V0");

    let fingerprint = source.info().identity().fingerprint().to_owned();
    let matching = SourceRequest::new(
        MediaId::from_raw(0x55),
        SourceLocation::Path(PathBuf::from(&path)),
    )
    .with_expected_fingerprint(fingerprint)
    .unwrap();
    assert!(backend.open_source(&matching, &operation()).is_ok());

    let mismatched = SourceRequest::new(
        MediaId::from_raw(0x55),
        SourceLocation::Path(PathBuf::from(&path)),
    )
    .with_expected_fingerprint("sha256:not-the-same-content")
    .unwrap();
    let error = match backend.open_source(&mismatched, &operation()) {
        Ok(_) => panic!("a relink with different content must fail"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(
        error.contexts()[0].field("media_id"),
        Some("00000000000000000000000000000055")
    );
    std::fs::remove_file(path).unwrap();
}

#[test]
fn exact_seek_uses_edited_frame_time_and_returns_decoder_preroll() {
    let fixture = fixture(FixtureBrand::Mp4);
    let request = memory_request(0xaa, "edited.mp4", &fixture.bytes);
    let mut source = open_through_registry(&request, "mp4");

    let actual = source
        .seek(
            SeekRequest::new(
                RationalTime::new(1_400, Timebase::integer(1_000).unwrap()),
                SeekMode::Exact,
            ),
            &operation(),
        )
        .unwrap();
    assert_eq!(actual, RationalTime::new(1_400, Timebase::MILLISECONDS));

    let preroll = next_packet(source.as_mut()).unwrap();
    assert_eq!(preroll.stream_id().value(), 1);
    assert_eq!(preroll.data(), b"V0");
    let target = next_packet(source.as_mut()).unwrap();
    assert_eq!(target.stream_id().value(), 1);
    assert_eq!(target.data(), b"V1");
}

#[test]
fn seeks_report_empty_edits_and_choose_a_predictable_nearest_frame() {
    let fixture = fixture(FixtureBrand::Mp4);
    let request = memory_request(0xbb, "empty-edit.mp4", &fixture.bytes);
    let mut source = open_through_registry(&request, "mp4");
    let target = RationalTime::new(250, Timebase::integer(1_000).unwrap());

    let error = source
        .seek(SeekRequest::new(target, SeekMode::Exact), &operation())
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);

    let actual = source
        .seek(
            SeekRequest::new(target, SeekMode::NearestKeyframe),
            &operation(),
        )
        .unwrap();
    assert_eq!(actual, RationalTime::new(500, Timebase::MILLISECONDS));
    assert_eq!(next_packet(source.as_mut()).unwrap().data(), b"V0");
}

#[test]
fn malformed_sample_ranges_fail_as_corrupt_data_without_panicking() {
    let mut fixture = fixture(FixtureBrand::Mp4);
    let offset = usize::try_from(fixture.video_offset).unwrap();
    fixture.bytes.truncate(offset + 1);
    let backend = Mp4MovBackend::new().unwrap();
    let error = match backend.open_source(
        &memory_request(1, "truncated.mp4", &fixture.bytes),
        &operation(),
    ) {
        Ok(_) => panic!("a truncated container must not open"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::CorruptData);
}

#[test]
fn every_truncated_atom_prefix_fails_without_panicking() {
    let fixture = fixture(FixtureBrand::Mp4);
    let backend = Mp4MovBackend::new().unwrap();
    for length in 0..fixture.bytes.len() {
        let bytes = fixture.bytes[..length].to_vec();
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            backend.open_source(&memory_request(1, "prefix.mp4", &bytes), &operation())
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
    let fixture = fixture(FixtureBrand::Mp4);
    let request = memory_request(0x99, "cancelled.mp4", &fixture.bytes);
    let backend = Mp4MovBackend::new().unwrap();

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
    let error = source.read_packet(&cancelled_read).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(next_packet(source.as_mut()).unwrap().data(), b"V0");

    let mut source = backend.open_source(&request, &operation()).unwrap();
    let cancelled_seek = operation();
    cancelled_seek.cancellation_token().cancel();
    let error = source
        .seek(
            SeekRequest::new(
                RationalTime::new(950, Timebase::integer(1_000).unwrap()),
                SeekMode::PreviousKeyframe,
            ),
            &cancelled_seek,
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(next_packet(source.as_mut()).unwrap().data(), b"V0");
}
