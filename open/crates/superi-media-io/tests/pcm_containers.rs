use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::sync::Arc;

use superi_core::error::ErrorCategory;
use superi_core::ids::MediaId;
use superi_core::pixel::ChannelLayout;
use superi_core::time::{RationalTime, Timebase};
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendRegistration, BackendRegistry, BackendTier,
    FallbackPolicy,
};
use superi_media_io::demux::{
    MediaSource, MetadataValue, Packet, SeekMode, SeekRequest, SourceLocation, SourceProbeLimits,
    SourceRequest,
};
use superi_media_io::operation::{MediaPriority, OperationContext};
use superi_media_io::pcm::{
    ByteOrder, PcmContainerBackend, PcmContainerKind, PcmContainerSource, PcmEncoding,
};
use superi_media_io::read::{CorruptionKind, ReadOutcome};

fn operation() -> OperationContext {
    OperationContext::new(MediaPriority::Interactive)
}

fn next_packet(source: &mut dyn MediaSource) -> Option<Packet> {
    match source.read_packet(&operation()).unwrap() {
        ReadOutcome::Complete(packet) => Some(packet),
        ReadOutcome::EndOfStream => None,
        ReadOutcome::Partial { .. } => panic!("complete fixtures must yield complete PCM packets"),
        _ => panic!("unknown PCM packet outcome"),
    }
}

fn chunk_le(id: [u8; 4], data: &[u8]) -> Vec<u8> {
    let mut chunk = Vec::with_capacity(8 + data.len() + (data.len() & 1));
    chunk.extend_from_slice(&id);
    chunk.extend_from_slice(&(data.len() as u32).to_le_bytes());
    chunk.extend_from_slice(data);
    if data.len() & 1 == 1 {
        chunk.push(0);
    }
    chunk
}

fn chunk_be(id: [u8; 4], data: &[u8]) -> Vec<u8> {
    let mut chunk = Vec::with_capacity(8 + data.len() + (data.len() & 1));
    chunk.extend_from_slice(&id);
    chunk.extend_from_slice(&(data.len() as u32).to_be_bytes());
    chunk.extend_from_slice(data);
    if data.len() & 1 == 1 {
        chunk.push(0);
    }
    chunk
}

fn riff_wave(chunks: impl IntoIterator<Item = Vec<u8>>) -> Vec<u8> {
    let mut body = b"WAVE".to_vec();
    for chunk in chunks {
        body.extend_from_slice(&chunk);
    }
    let mut file = b"RIFF".to_vec();
    file.extend_from_slice(&(body.len() as u32).to_le_bytes());
    file.extend_from_slice(&body);
    file
}

fn aiff(chunks: impl IntoIterator<Item = Vec<u8>>) -> Vec<u8> {
    let mut body = b"AIFF".to_vec();
    for chunk in chunks {
        body.extend_from_slice(&chunk);
    }
    let mut file = b"FORM".to_vec();
    file.extend_from_slice(&(body.len() as u32).to_be_bytes());
    file.extend_from_slice(&body);
    file
}

fn wave_extensible_format() -> Vec<u8> {
    let mut format = Vec::with_capacity(40);
    format.extend_from_slice(&0xfffe_u16.to_le_bytes());
    format.extend_from_slice(&6_u16.to_le_bytes());
    format.extend_from_slice(&48_000_u32.to_le_bytes());
    format.extend_from_slice(&864_000_u32.to_le_bytes());
    format.extend_from_slice(&18_u16.to_le_bytes());
    format.extend_from_slice(&24_u16.to_le_bytes());
    format.extend_from_slice(&22_u16.to_le_bytes());
    format.extend_from_slice(&20_u16.to_le_bytes());
    format.extend_from_slice(&0x3f_u32.to_le_bytes());
    format.extend_from_slice(&[
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b,
        0x71,
    ]);
    format
}

fn broadcast_extension(time_reference: u64) -> Vec<u8> {
    let mut bext = vec![0_u8; 602];
    bext[..18].copy_from_slice(b"Superi field take\0");
    bext[256..262].copy_from_slice(b"Superi");
    bext[320..330].copy_from_slice(b"2026-07-12");
    bext[330..338].copy_from_slice(b"21-30-00");
    bext[338..346].copy_from_slice(&time_reference.to_le_bytes());
    bext[346..348].copy_from_slice(&2_u16.to_le_bytes());
    bext[412..422].copy_from_slice(&[0x9c, 0xff, 0x2c, 0x01, 0x78, 0xec, 0x64, 0x00, 0xb8, 0x0b]);
    bext
}

fn wave_fixture() -> Vec<u8> {
    let audio: Vec<_> = (0_u8..72).collect();
    let cue = [
        1, 0, 0, 0, // one cue
        7, 0, 0, 0, // identifier
        2, 0, 0, 0, // position
        b'd', b'a', b't', b'a', 0, 0, 0, 0, 0, 0, 0, 0, 2, 0, 0, 0,
    ];
    riff_wave([
        chunk_le(*b"JUNK", b"odd"),
        chunk_le(*b"fmt ", &wave_extensible_format()),
        chunk_le(*b"bext", &broadcast_extension(96_000)),
        chunk_le(*b"cue ", &cue),
        chunk_le(*b"data", &audio),
    ])
}

fn aiff_fixture() -> Vec<u8> {
    let mut common = Vec::with_capacity(18);
    common.extend_from_slice(&2_u16.to_be_bytes());
    common.extend_from_slice(&3_u32.to_be_bytes());
    common.extend_from_slice(&16_u16.to_be_bytes());
    common.extend_from_slice(&[0x40, 0x0e, 0xac, 0x44, 0, 0, 0, 0, 0, 0]);

    let mut sound = Vec::new();
    sound.extend_from_slice(&4_u32.to_be_bytes());
    sound.extend_from_slice(&8_u32.to_be_bytes());
    sound.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    sound.extend_from_slice(&[
        0x00, 0x01, 0xff, 0xff, 0x12, 0x34, 0xed, 0xcb, 0x7f, 0xff, 0x80, 0x00,
    ]);

    aiff([
        chunk_be(*b"NAME", b"Take 1"),
        chunk_be(*b"COMM", &common),
        chunk_be(*b"MARK", &[0, 1, 0, 7, 0, 0, 0, 2, 3, b'h', b'i', b't']),
        chunk_be(*b"SSND", &sound),
    ])
}

fn rf64_float_fixture() -> Vec<u8> {
    let mut format = Vec::with_capacity(18);
    format.extend_from_slice(&3_u16.to_le_bytes());
    format.extend_from_slice(&2_u16.to_le_bytes());
    format.extend_from_slice(&96_000_u32.to_le_bytes());
    format.extend_from_slice(&768_000_u32.to_le_bytes());
    format.extend_from_slice(&8_u16.to_le_bytes());
    format.extend_from_slice(&32_u16.to_le_bytes());
    format.extend_from_slice(&0_u16.to_le_bytes());
    let format_chunk = chunk_le(*b"fmt ", &format);
    let audio = [0_u8, 0, 0, 0x3f, 0, 0, 0, 0xbf];
    let riff_size = 4 + 36 + format_chunk.len() as u64 + 8 + audio.len() as u64;

    let mut ds64 = Vec::with_capacity(28);
    ds64.extend_from_slice(&riff_size.to_le_bytes());
    ds64.extend_from_slice(&(audio.len() as u64).to_le_bytes());
    ds64.extend_from_slice(&1_u64.to_le_bytes());
    ds64.extend_from_slice(&0_u32.to_le_bytes());

    let mut file = b"RF64".to_vec();
    file.extend_from_slice(&u32::MAX.to_le_bytes());
    file.extend_from_slice(b"WAVE");
    file.extend_from_slice(&chunk_le(*b"ds64", &ds64));
    file.extend_from_slice(&format_chunk);
    file.extend_from_slice(b"data");
    file.extend_from_slice(&u32::MAX.to_le_bytes());
    file.extend_from_slice(&audio);
    file
}

fn aiff_20_bit_fixture() -> Vec<u8> {
    let mut common = Vec::with_capacity(18);
    common.extend_from_slice(&1_u16.to_be_bytes());
    common.extend_from_slice(&1_u32.to_be_bytes());
    common.extend_from_slice(&20_u16.to_be_bytes());
    common.extend_from_slice(&[0x40, 0x0e, 0xbb, 0x80, 0, 0, 0, 0, 0, 0]);

    let mut sound = vec![0_u8; 8];
    sound.extend_from_slice(&[0x12, 0x34, 0x50]);
    aiff([chunk_be(*b"COMM", &common), chunk_be(*b"SSND", &sound)])
}

fn memory_request(media_id: u128, name: &str, bytes: Vec<u8>) -> SourceRequest {
    SourceRequest::new(
        MediaId::from_raw(media_id),
        SourceLocation::Memory {
            name: name.into(),
            data: Arc::from(bytes),
        },
    )
}

#[test]
fn wave_extensible_and_bwf_preserve_layout_timing_metadata_and_offsets() {
    let request = memory_request(7, "take.wav", wave_fixture());
    let mut source = PcmContainerSource::open(&request, &operation()).unwrap();

    assert_eq!(source.container_kind(), PcmContainerKind::Wave);
    assert_eq!(source.format().encoding(), PcmEncoding::Integer);
    assert_eq!(source.format().byte_order(), ByteOrder::LittleEndian);
    assert_eq!(source.format().sample_rate(), 48_000);
    assert_eq!(source.format().bits_per_sample(), 24);
    assert_eq!(source.format().valid_bits_per_sample(), 20);
    assert_eq!(source.format().block_align(), 18);
    assert_eq!(
        source.format().channel_layout(),
        &ChannelLayout::surround_5_1()
    );
    assert_eq!(source.frame_count(), 4);
    assert_eq!(source.info().duration().unwrap().value(), 4);
    assert_eq!(source.info().streams()[0].codec().as_str(), "pcm_s24le");
    assert_eq!(
        source.info().metadata().get("container.bwf.time_reference"),
        Some(&MetadataValue::Unsigned(96_000))
    );
    assert_eq!(
        source.info().metadata().get("container.bwf.version"),
        Some(&MetadataValue::Unsigned(2))
    );
    assert_eq!(
        source.info().metadata().get("container.data_offset"),
        Some(&MetadataValue::Unsigned(source.audio_data_offset()))
    );

    let chunks = source.ancillary_chunks();
    assert_eq!(chunks.len(), 3);
    assert_eq!(chunks[0].id(), *b"JUNK");
    assert_eq!(chunks[0].data(), b"odd");
    assert!(chunks[0].payload_offset() < chunks[1].payload_offset());
    assert_eq!(chunks[1].id(), *b"bext");
    assert_eq!(chunks[2].id(), *b"cue ");

    let packet = next_packet(&mut source).unwrap();
    assert_eq!(packet.data(), (0_u8..72).collect::<Vec<_>>());
    assert_eq!(packet.timing().presentation_time().unwrap().value(), 96_000);
    assert_eq!(packet.timing().decode_time().unwrap().value(), 96_000);
    assert_eq!(packet.timing().duration().unwrap().value(), 4);
    assert!(packet.is_keyframe());
    assert_eq!(
        packet.metadata().get("container.offset"),
        Some(&MetadataValue::Unsigned(source.audio_data_offset()))
    );
    assert!(matches!(
        source.read_packet(&operation()).unwrap(),
        ReadOutcome::EndOfStream
    ));

    let actual = source
        .seek(
            SeekRequest::new(
                RationalTime::new(96_002, Timebase::integer(48_000).unwrap()),
                SeekMode::Exact,
            ),
            &operation(),
        )
        .unwrap();
    assert_eq!(actual.value(), 96_002);
    let packet = next_packet(&mut source).unwrap();
    assert_eq!(packet.data(), (36_u8..72).collect::<Vec<_>>());
    assert_eq!(packet.timing().presentation_time().unwrap().value(), 96_002);
    assert_eq!(packet.timing().duration().unwrap().value(), 2);
}

#[test]
fn aiff_preserves_big_endian_pcm_ssnd_offset_and_edit_chunks() {
    let request = memory_request(8, "take.aiff", aiff_fixture());
    let mut source = PcmContainerSource::open(&request, &operation()).unwrap();

    assert_eq!(source.container_kind(), PcmContainerKind::Aiff);
    assert_eq!(source.format().encoding(), PcmEncoding::Integer);
    assert_eq!(source.format().byte_order(), ByteOrder::BigEndian);
    assert_eq!(source.format().sample_rate(), 44_100);
    assert_eq!(source.format().bits_per_sample(), 16);
    assert_eq!(source.format().valid_bits_per_sample(), 16);
    assert_eq!(source.format().block_align(), 4);
    assert_eq!(source.format().channel_layout(), &ChannelLayout::stereo());
    assert_eq!(source.frame_count(), 3);
    assert_eq!(source.info().streams()[0].codec().as_str(), "pcm_s16be");
    assert_eq!(
        source.info().metadata().get("container.aiff.ssnd_offset"),
        Some(&MetadataValue::Unsigned(4))
    );
    assert_eq!(
        source
            .info()
            .metadata()
            .get("container.aiff.ssnd_block_size"),
        Some(&MetadataValue::Unsigned(8))
    );
    assert_eq!(
        source
            .info()
            .metadata()
            .get("container.aiff.ssnd_offset_data"),
        Some(&MetadataValue::Bytes(Arc::from([0xde, 0xad, 0xbe, 0xef,])))
    );
    assert_eq!(source.ancillary_chunks().len(), 2);
    assert_eq!(source.ancillary_chunks()[0].id(), *b"NAME");
    assert_eq!(source.ancillary_chunks()[1].id(), *b"MARK");

    let packet = next_packet(&mut source).unwrap();
    assert_eq!(
        packet.data(),
        [0x00, 0x01, 0xff, 0xff, 0x12, 0x34, 0xed, 0xcb, 0x7f, 0xff, 0x80, 0x00]
    );
    assert_eq!(packet.timing().presentation_time().unwrap().value(), 0);
    assert_eq!(packet.timing().duration().unwrap().value(), 3);

    let actual = source
        .seek(
            SeekRequest::new(
                RationalTime::new(1, Timebase::integer(44_100).unwrap()),
                SeekMode::PreviousKeyframe,
            ),
            &operation(),
        )
        .unwrap();
    assert_eq!(actual.value(), 1);
    assert_eq!(
        next_packet(&mut source).unwrap().data(),
        [0x12, 0x34, 0xed, 0xcb, 0x7f, 0xff, 0x80, 0x00]
    );
}

#[test]
fn rf64_sizes_float_pcm_and_non_byte_aligned_aiff_precision_are_exact() {
    let mut rf64 = PcmContainerSource::open(
        &memory_request(81, "float.wav", rf64_float_fixture()),
        &operation(),
    )
    .unwrap();
    assert_eq!(rf64.container_kind(), PcmContainerKind::Wave);
    assert_eq!(rf64.format().encoding(), PcmEncoding::Float);
    assert_eq!(rf64.format().sample_rate(), 96_000);
    assert_eq!(rf64.format().bits_per_sample(), 32);
    assert_eq!(rf64.info().streams()[0].codec().as_str(), "pcm_f32le");
    assert_eq!(rf64.frame_count(), 1);
    assert_eq!(
        next_packet(&mut rf64).unwrap().data(),
        [0, 0, 0, 0x3f, 0, 0, 0, 0xbf]
    );

    let mut aiff = PcmContainerSource::open(
        &memory_request(82, "twenty-bit.aiff", aiff_20_bit_fixture()),
        &operation(),
    )
    .unwrap();
    assert_eq!(aiff.format().sample_rate(), 48_000);
    assert_eq!(aiff.format().bits_per_sample(), 24);
    assert_eq!(aiff.format().valid_bits_per_sample(), 20);
    assert_eq!(aiff.format().block_align(), 3);
    assert_eq!(aiff.info().streams()[0].codec().as_str(), "pcm_s24be");
    assert_eq!(next_packet(&mut aiff).unwrap().data(), [0x12, 0x34, 0x50]);
}

#[test]
fn file_sources_and_relinks_use_content_identity() {
    let bytes = wave_fixture();
    let mut path = std::env::temp_dir();
    path.push(format!(
        "superi-pcm-container-{}-{}.wav",
        std::process::id(),
        MediaId::from_raw(91)
    ));
    let _guard = TempFile(path.clone());
    fs::write(&path, &bytes).unwrap();

    let request = SourceRequest::new(MediaId::from_raw(91), SourceLocation::Path(path.clone()));
    let source = PcmContainerSource::open(&request, &operation()).unwrap();
    let fingerprint = source.info().identity().fingerprint().to_owned();
    assert!(fingerprint.starts_with("sha256:"));
    assert_eq!(fingerprint.len(), 71);

    let relink = SourceRequest::new(MediaId::from_raw(91), SourceLocation::Path(path))
        .with_expected_fingerprint(fingerprint.clone())
        .unwrap();
    assert_eq!(
        PcmContainerSource::open(&relink, &operation())
            .unwrap()
            .info()
            .identity()
            .fingerprint(),
        fingerprint
    );

    let mut changed = bytes;
    *changed.last_mut().unwrap() ^= 0xff;
    let mismatch = memory_request(91, "replacement.wav", changed)
        .with_expected_fingerprint(fingerprint)
        .unwrap();
    let error = PcmContainerSource::open(&mismatch, &operation()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
}

#[test]
fn malformed_or_unsupported_containers_fail_without_partial_success() {
    let mut invalid_wave = wave_fixture();
    let format = invalid_wave
        .windows(4)
        .position(|window| window == b"fmt ")
        .unwrap();
    invalid_wave[format + 8 + 12..format + 8 + 14].copy_from_slice(&17_u16.to_le_bytes());
    let error = PcmContainerSource::open(&memory_request(1, "bad.wav", invalid_wave), &operation())
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);

    let mut invalid_mask = wave_fixture();
    let format = invalid_mask
        .windows(4)
        .position(|window| window == b"fmt ")
        .unwrap();
    invalid_mask[format + 8 + 20..format + 8 + 24].copy_from_slice(&0x07_u32.to_le_bytes());
    let error = PcmContainerSource::open(
        &memory_request(4, "bad-mask.wav", invalid_mask),
        &operation(),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);

    let mut truncated = aiff_fixture();
    truncated.pop();
    let error = PcmContainerSource::open(&memory_request(2, "short.aiff", truncated), &operation())
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);

    let mut compressed = aiff_fixture();
    compressed[8..12].copy_from_slice(b"AIFC");
    let error = PcmContainerSource::open(
        &memory_request(3, "compressed.aifc", compressed),
        &operation(),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);
}

#[test]
fn pcm_backend_probes_and_opens_wav_and_aiff_through_the_registry() {
    let backend = Arc::new(PcmContainerBackend::new().unwrap());
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

    for (media_id, name, bytes, container) in [
        (101, "registry.wav", wave_fixture(), "wav"),
        (102, "registry.aiff", aiff_fixture(), "aiff"),
    ] {
        let request = memory_request(media_id, name, bytes);
        let selection = registry
            .probe_source(
                request,
                SourceProbeLimits::default(),
                FallbackPolicy::Disallow,
                &operation(),
            )
            .unwrap();
        assert_eq!(
            selection.primary().backend().descriptor().id().as_str(),
            "pcm-containers"
        );
        assert_eq!(selection.primary().container().as_str(), container);
        let mut source = selection.open(&operation()).unwrap();
        assert!(matches!(
            source.read_packet(&operation()).unwrap(),
            ReadOutcome::Complete(_)
        ));
    }
}

#[test]
fn pcm_operations_honor_cancellation_without_advancing_source_state() {
    let request = memory_request(111, "cancel.wav", wave_fixture());
    let cancelled_open = operation();
    cancelled_open.cancellation_token().cancel();
    let error = PcmContainerSource::open(&request, &cancelled_open).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Cancelled);

    let mut source = PcmContainerSource::open(&request, &operation()).unwrap();
    let cancelled_read = operation();
    cancelled_read.cancellation_token().cancel();
    let error = source.read_packet(&cancelled_read).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(next_packet(&mut source).unwrap().data().len(), 72);

    let mut source = PcmContainerSource::open(&request, &operation()).unwrap();
    let cancelled_seek = operation();
    cancelled_seek.cancellation_token().cancel();
    let error = source
        .seek(
            SeekRequest::new(
                RationalTime::new(96_002, Timebase::integer(48_000).unwrap()),
                SeekMode::Exact,
            ),
            &cancelled_seek,
        )
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Cancelled);
    assert_eq!(
        next_packet(&mut source)
            .unwrap()
            .timing()
            .presentation_time()
            .unwrap()
            .value(),
        96_000
    );
}

#[test]
fn truncated_file_packet_returns_aligned_partial_data_and_corruption_evidence() {
    let bytes = wave_fixture();
    let mut path = std::env::temp_dir();
    path.push(format!(
        "superi-pcm-partial-{}-{}.wav",
        std::process::id(),
        MediaId::from_raw(121)
    ));
    let _guard = TempFile(path.clone());
    fs::write(&path, bytes).unwrap();

    let request = SourceRequest::new(MediaId::from_raw(121), SourceLocation::Path(path.clone()));
    let mut source = PcmContainerSource::open(&request, &operation()).unwrap();
    OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_len(source.audio_data_offset() + 37)
        .unwrap();

    let ReadOutcome::Partial { value, report } = source.read_packet(&operation()).unwrap() else {
        panic!("a post-open truncation must return a usable partial PCM packet")
    };
    assert_eq!(value.data().len(), 36);
    assert_eq!(value.timing().duration().unwrap().value(), 2);
    assert_eq!(report.kind(), CorruptionKind::Truncated);
    assert_eq!(report.stream_id().unwrap().value(), 0);
    assert_eq!(report.byte_offset(), Some(source.audio_data_offset()));
    assert_eq!(report.expected_bytes(), Some(72));
    assert_eq!(report.actual_bytes(), Some(37));
}

#[derive(Debug)]
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}
