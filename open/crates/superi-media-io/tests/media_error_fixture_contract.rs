use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use superi_core::ids::MediaId;
use superi_media_io::demux::{MediaSource, SourceLocation, SourceRequest};
use superi_media_io::operation::{MediaPriority, OperationContext};
use superi_media_io::pcm::PcmContainerSource;
use superi_media_io::read::ReadOutcome;

const HEADER: &str = "case_id,payload,container,trigger,error_category,recoverability,corruption_kind,mutation_offset,truncate_to,data_offset,expected_bytes,actual_bytes,usable_bytes,usable_frames";
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug)]
struct MediaErrorRow<'a> {
    case_id: &'a str,
    payload: &'a str,
    container: &'a str,
    trigger: &'a str,
    error_category: &'a str,
    recoverability: &'a str,
    corruption_kind: Option<&'a str>,
    mutation_offset: Option<u64>,
    truncate_to: Option<u64>,
    data_offset: Option<u64>,
    expected_bytes: Option<usize>,
    actual_bytes: Option<usize>,
    usable_bytes: Option<usize>,
    usable_frames: Option<u64>,
}

struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

fn canonical_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/media/error-cases/v1")
}

fn operation() -> OperationContext {
    OperationContext::new(MediaPriority::Interactive)
}

fn optional<T>(value: &str, row: usize, field: usize) -> Option<T>
where
    T: std::str::FromStr,
    T::Err: std::fmt::Debug,
{
    (!value.is_empty()).then(|| {
        value
            .parse()
            .unwrap_or_else(|_| panic!("catalog row {row} field {field}"))
    })
}

fn parse_catalog(catalog: &str) -> Vec<MediaErrorRow<'_>> {
    let content = catalog
        .strip_suffix("\r\n")
        .expect("canonical catalog must end with CRLF");
    let mut lines = content.split("\r\n");
    assert_eq!(lines.next(), Some(HEADER));

    let rows: Vec<_> = lines
        .enumerate()
        .map(|(index, line)| {
            let row = index + 1;
            assert!(!line.is_empty(), "catalog row {row} must not be empty");
            let fields: Vec<_> = line.split(',').collect();
            assert_eq!(fields.len(), 14, "catalog row {row} field count");
            assert!(matches!(fields[2], "wave" | "aiff" | "aifc"));
            assert!(matches!(fields[3], "open" | "read"));
            assert!(matches!(fields[4], "corrupt_data" | "unsupported"));
            assert!(matches!(fields[5], "user_correctable" | "degraded"));
            assert!(fields[6].is_empty() || fields[6] == "truncated");
            MediaErrorRow {
                case_id: fields[0],
                payload: fields[1],
                container: fields[2],
                trigger: fields[3],
                error_category: fields[4],
                recoverability: fields[5],
                corruption_kind: (!fields[6].is_empty()).then_some(fields[6]),
                mutation_offset: optional(fields[7], row, 7),
                truncate_to: optional(fields[8], row, 8),
                data_offset: optional(fields[9], row, 9),
                expected_bytes: optional(fields[10], row, 10),
                actual_bytes: optional(fields[11], row, 11),
                usable_bytes: optional(fields[12], row, 12),
                usable_frames: optional(fields[13], row, 13),
            }
        })
        .collect();

    assert_eq!(
        rows.iter().map(|row| row.case_id).collect::<Vec<_>>(),
        [
            "malformed-wave",
            "truncated-aiff",
            "unsupported-aifc",
            "partial-readable-wave",
        ]
    );
    rows
}

fn read_catalog() -> String {
    fs::read_to_string(canonical_fixture().join("media-error-cases.csv"))
        .expect("canonical media error catalog must exist")
}

fn case<'a>(rows: &'a [MediaErrorRow<'a>], case_id: &str) -> MediaErrorRow<'a> {
    *rows
        .iter()
        .find(|row| row.case_id == case_id)
        .unwrap_or_else(|| panic!("missing case {case_id}"))
}

fn open_memory(row: MediaErrorRow<'_>, media_id: u128) -> superi_core::error::Error {
    let bytes =
        fs::read(canonical_fixture().join(row.payload)).expect("fixture payload must exist");
    let request = SourceRequest::new(
        MediaId::from_raw(media_id),
        SourceLocation::Memory {
            name: row.payload.to_owned(),
            data: Arc::from(bytes),
        },
    );
    PcmContainerSource::open(&request, &operation()).expect_err("fixture open must fail")
}

#[test]
fn catalog_is_strict_and_payload_mutations_are_independently_visible() {
    let catalog = read_catalog();
    assert_eq!(
        catalog.matches('\n').count(),
        catalog.matches("\r\n").count()
    );
    let rows = parse_catalog(&catalog);

    let malformed = case(&rows, "malformed-wave");
    assert_eq!(malformed.container, "wave");
    assert_eq!(malformed.trigger, "open");
    let malformed_bytes =
        fs::read(canonical_fixture().join(malformed.payload)).expect("malformed WAVE must exist");
    assert_eq!(&malformed_bytes[..12], b"RIFF4\0\0\0WAVE");
    let offset = malformed
        .mutation_offset
        .expect("mutation offset must exist") as usize;
    assert_eq!(offset, 32);
    assert_eq!(&malformed_bytes[offset..offset + 2], &2_u16.to_le_bytes());

    let truncated = case(&rows, "truncated-aiff");
    assert_eq!(truncated.container, "aiff");
    assert_eq!(truncated.trigger, "open");
    let truncated_bytes =
        fs::read(canonical_fixture().join(truncated.payload)).expect("truncated AIFF must exist");
    assert_eq!(&truncated_bytes[..4], b"FORM");
    assert_eq!(&truncated_bytes[8..12], b"AIFF");
    assert_eq!(
        truncated_bytes.len() as u64,
        truncated.truncate_to.expect("truncation length must exist")
    );
    let declared_end = u64::from(u32::from_be_bytes(
        truncated_bytes[4..8].try_into().expect("FORM size bytes"),
    )) + 8;
    assert_eq!(declared_end, truncated_bytes.len() as u64 + 1);

    let unsupported = case(&rows, "unsupported-aifc");
    assert_eq!(unsupported.container, "aifc");
    assert_eq!(unsupported.trigger, "open");
    let unsupported_bytes = fs::read(canonical_fixture().join(unsupported.payload))
        .expect("unsupported AIFC must exist");
    assert_eq!(&unsupported_bytes[..4], b"FORM");
    let offset = unsupported
        .mutation_offset
        .expect("mutation offset must exist") as usize;
    assert_eq!(offset, 8);
    assert_eq!(&unsupported_bytes[offset..offset + 4], b"AIFC");

    let partial = case(&rows, "partial-readable-wave");
    assert_eq!(partial.container, "wave");
    assert_eq!(partial.trigger, "read");
    let partial_bytes =
        fs::read(canonical_fixture().join(partial.payload)).expect("partial seed must exist");
    assert_eq!(&partial_bytes[..12], b"RIFF4\0\0\0WAVE");
    assert_eq!(partial_bytes.len(), 60);
    assert_eq!(partial.data_offset, Some(44));
}

#[test]
fn malformed_truncated_and_unsupported_fixtures_drive_production_open_errors() {
    let catalog = read_catalog();
    let rows = parse_catalog(&catalog);

    for (media_id, case_id) in [
        (301, "malformed-wave"),
        (302, "truncated-aiff"),
        (303, "unsupported-aifc"),
    ] {
        let row = case(&rows, case_id);
        let error = open_memory(row, media_id);
        assert_eq!(error.category().code(), row.error_category, "{case_id}");
        assert_eq!(
            error.recoverability().code(),
            row.recoverability,
            "{case_id}"
        );
    }
}

#[test]
fn partial_fixture_drives_aligned_packet_and_exact_corruption_evidence() {
    let catalog = read_catalog();
    let rows = parse_catalog(&catalog);
    let row = case(&rows, "partial-readable-wave");
    let bytes = fs::read(canonical_fixture().join(row.payload)).expect("partial seed must exist");
    let suffix = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "superi-media-error-partial-{}-{suffix}.wav",
        std::process::id()
    ));
    let _guard = TempFile(path.clone());
    fs::write(&path, bytes).expect("temporary partial source must be written");

    let request = SourceRequest::new(MediaId::from_raw(304), SourceLocation::Path(path.clone()));
    let mut source = PcmContainerSource::open(&request, &operation())
        .expect("complete seed must open before truncation");
    assert_eq!(source.audio_data_offset(), row.data_offset.unwrap());
    OpenOptions::new()
        .write(true)
        .open(&path)
        .expect("temporary source must reopen")
        .set_len(row.truncate_to.expect("post-open length must exist"))
        .expect("temporary source must truncate");

    let ReadOutcome::Partial { value, report } = source
        .read_packet(&operation())
        .expect("post-open truncation must remain partially readable")
    else {
        panic!("fixture must return an explicit partial packet")
    };

    assert_eq!(value.data().len(), row.usable_bytes.unwrap());
    assert_eq!(
        value.timing().duration().unwrap().value(),
        row.usable_frames.unwrap()
    );
    assert_eq!(report.kind().code(), row.corruption_kind.unwrap());
    assert_eq!(report.recoverability().code(), row.recoverability);
    assert_eq!(report.stream_id().unwrap().value(), 0);
    assert_eq!(report.byte_offset(), row.data_offset);
    assert_eq!(report.expected_bytes(), row.expected_bytes);
    assert_eq!(report.actual_bytes(), row.actual_bytes);
    assert_eq!(
        report.to_error("fixture_contract").category().code(),
        row.error_category
    );
}
