use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use superi_core::time::{RationalTime, Timebase};
use superi_media_io::demux::PacketTiming;
use superi_media_io::timecode::{
    TimecodeDescription, TimecodeFlags, TimecodeSampleEncoding, TimestampNormalizer,
};
use superi_media_io::vfr::VariableFrameRateMap;

const HEADER: &str = "case_id,kind,segment,decode_index,presentation_index,rate_numerator,rate_denominator,presentation_timestamp,decode_timestamp,duration,timecode_label";

#[derive(Clone, Copy, Debug)]
struct TimingRow<'a> {
    case_id: &'a str,
    kind: &'a str,
    segment: u32,
    decode_index: u64,
    presentation_index: u64,
    rate_numerator: u32,
    rate_denominator: u32,
    presentation_timestamp: i64,
    decode_timestamp: i64,
    duration: u64,
    timecode_label: Option<&'a str>,
}

impl TimingRow<'_> {
    fn timebase(self) -> Timebase {
        Timebase::new(self.rate_numerator, self.rate_denominator)
            .expect("fixture rate must be valid")
    }

    fn packet(self) -> PacketTiming {
        PacketTiming::new(
            self.timebase(),
            Some(self.presentation_timestamp),
            Some(self.decode_timestamp),
            Some(self.duration),
        )
        .expect("fixture packet timing must be valid")
    }
}

fn canonical_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/timing/cadences/v1")
}

fn read_catalog() -> String {
    fs::read_to_string(canonical_fixture().join("timing-cases.csv"))
        .expect("canonical timing catalog must exist")
}

fn parse_field<T>(value: &str, row: usize, field: usize) -> T
where
    T: std::str::FromStr,
    T::Err: std::fmt::Debug,
{
    value
        .parse()
        .unwrap_or_else(|_| panic!("catalog row {row} field {field}"))
}

fn parse_catalog(catalog: &str) -> Vec<TimingRow<'_>> {
    let content = catalog
        .strip_suffix("\r\n")
        .expect("canonical catalog must end with CRLF");
    let mut lines = content.split("\r\n");
    assert_eq!(lines.next(), Some(HEADER));

    let rows: Vec<_> = lines
        .enumerate()
        .map(|(index, line)| {
            assert!(
                !line.is_empty(),
                "catalog row {} must not be empty",
                index + 1
            );
            let fields: Vec<_> = line.split(',').collect();
            assert_eq!(fields.len(), 11, "catalog row {} field count", index + 1);
            let kind = fields[1];
            assert!(matches!(
                kind,
                "cfr" | "vfr" | "drop-frame" | "discontinuous"
            ));
            let label = (!fields[10].is_empty()).then_some(fields[10]);
            assert_eq!(label.is_some(), kind == "drop-frame");
            let row = TimingRow {
                case_id: fields[0],
                kind,
                segment: parse_field(fields[2], index + 1, 2),
                decode_index: parse_field(fields[3], index + 1, 3),
                presentation_index: parse_field(fields[4], index + 1, 4),
                rate_numerator: parse_field(fields[5], index + 1, 5),
                rate_denominator: parse_field(fields[6], index + 1, 6),
                presentation_timestamp: parse_field(fields[7], index + 1, 7),
                decode_timestamp: parse_field(fields[8], index + 1, 8),
                duration: parse_field(fields[9], index + 1, 9),
                timecode_label: label,
            };
            assert!(row.duration > 0);
            let _ = row.timebase();
            row
        })
        .collect();

    assert_eq!(rows.len(), 18);
    let mut cases = Vec::new();
    for row in &rows {
        if cases.last().copied() != Some(row.case_id) {
            cases.push(row.case_id);
        }
    }
    assert_eq!(
        cases,
        [
            "cfr-24",
            "vfr-milliseconds",
            "drop-frame-29.97",
            "timestamp-gap",
            "timestamp-reset",
        ]
    );

    for case_id in cases {
        let case: Vec<_> = rows.iter().filter(|row| row.case_id == case_id).collect();
        for (expected, row) in case.iter().enumerate() {
            assert_eq!(row.decode_index, expected as u64);
        }
        let mut segments: BTreeMap<u32, Vec<u64>> = BTreeMap::new();
        for row in case {
            segments
                .entry(row.segment)
                .or_default()
                .push(row.presentation_index);
        }
        for indexes in segments.values_mut() {
            indexes.sort_unstable();
            assert_eq!(
                indexes,
                &(0..indexes.len() as u64).collect::<Vec<_>>(),
                "presentation indexes must be complete within each segment"
            );
        }
    }
    rows
}

fn case<'a>(rows: &'a [TimingRow<'a>], case_id: &str) -> Vec<TimingRow<'a>> {
    rows.iter()
        .copied()
        .filter(|row| row.case_id == case_id)
        .collect()
}

#[test]
fn canonical_catalog_has_a_strict_fixed_schema_and_case_inventory() {
    let catalog = read_catalog();

    assert!(!catalog.as_bytes().contains(&0));
    assert_eq!(
        catalog.matches('\n').count(),
        catalog.matches("\r\n").count()
    );
    let rows = parse_catalog(&catalog);

    assert_eq!(case(&rows, "cfr-24").len(), 4);
    assert_eq!(case(&rows, "vfr-milliseconds").len(), 3);
    assert_eq!(case(&rows, "drop-frame-29.97").len(), 3);
    assert_eq!(case(&rows, "timestamp-gap").len(), 4);
    assert_eq!(case(&rows, "timestamp-reset").len(), 4);
}

#[test]
fn cfr_and_vfr_cases_drive_real_packet_and_presentation_maps() {
    let catalog = read_catalog();
    let rows = parse_catalog(&catalog);

    let cfr = case(&rows, "cfr-24");
    assert!(cfr.iter().all(|row| row.kind == "cfr"));
    let cfr_map = VariableFrameRateMap::from_packet_timings(cfr.iter().map(|row| row.packet()))
        .expect("CFR fixture must map");
    assert_eq!(cfr_map.frame_count(), 4);
    assert!(!cfr_map.is_variable_frame_rate());
    assert_eq!(cfr_map.presentation_start().value(), 0);
    assert_eq!(cfr_map.presentation_end().value(), 4);
    for index in 0..4 {
        let frame = cfr_map.frame(index).expect("CFR frame must exist");
        assert_eq!(frame.presentation_time().value(), index as i64);
        assert_eq!(frame.duration().value(), 1);
    }

    let vfr = case(&rows, "vfr-milliseconds");
    assert!(vfr.iter().all(|row| row.kind == "vfr"));
    let packets: Vec<_> = vfr.iter().map(|row| row.packet()).collect();
    assert_eq!(
        packets
            .iter()
            .map(|packet| packet.presentation_time().unwrap().value())
            .collect::<Vec<_>>(),
        [80, 0, 40]
    );
    assert_eq!(
        packets
            .iter()
            .map(|packet| packet.decode_time().unwrap().value())
            .collect::<Vec<_>>(),
        [0, 40, 80]
    );
    let vfr_map = VariableFrameRateMap::from_packet_timings(packets)
        .expect("decode-order VFR fixture must map");
    assert_eq!(vfr_map.frame_count(), 3);
    assert!(vfr_map.is_variable_frame_rate());
    assert_eq!(
        (0..3)
            .map(|index| vfr_map.frame(index).unwrap().presentation_time().value())
            .collect::<Vec<_>>(),
        [0, 40, 80]
    );
    assert_eq!(
        (0..3)
            .map(|index| vfr_map.frame(index).unwrap().duration().value())
            .collect::<Vec<_>>(),
        [40, 40, 80]
    );
}

#[test]
fn drop_frame_case_skips_labels_without_dropping_physical_frames() {
    let catalog = read_catalog();
    let rows = parse_catalog(&catalog);
    let drop_frame = case(&rows, "drop-frame-29.97");
    assert!(drop_frame.iter().all(|row| row.kind == "drop-frame"));

    let first = drop_frame[0];
    let description = TimecodeDescription::new(
        TimecodeSampleEncoding::Timecode32,
        TimecodeFlags::new(TimecodeFlags::DROP_FRAME | TimecodeFlags::WRAPS_AT_24_HOURS),
        first.rate_numerator,
        first.rate_denominator,
        30,
        Arc::from(&b"canonical-timing-fixture"[..]),
    )
    .expect("Apple-compatible drop-frame description must be valid");

    let mapping =
        VariableFrameRateMap::from_packet_timings(drop_frame.iter().map(|row| row.packet()))
            .expect("physical drop-frame samples must stay contiguous");
    assert!(!mapping.is_variable_frame_rate());
    assert_eq!(mapping.frame_count(), 3);
    assert_eq!(mapping.presentation_start().value(), 1_799);
    assert_eq!(mapping.presentation_end().value(), 1_802);

    for (offset, row) in drop_frame.iter().enumerate() {
        assert_eq!(row.presentation_timestamp, 1_799 + offset as i64);
        let source = description
            .timecode_from_frames(row.presentation_timestamp)
            .expect("fixture frame must produce source timecode");
        assert_eq!(source.frames(), row.presentation_timestamp);
        assert_eq!(source.to_string(), row.timecode_label.unwrap());
        assert_eq!(
            source.to_rational_time().value(),
            row.presentation_timestamp
        );
        assert_eq!(source.to_rational_time().timebase(), row.timebase());
    }
}

#[test]
fn discontinuities_require_explicit_segments_and_each_segment_rebases_reversibly() {
    let catalog = read_catalog();
    let rows = parse_catalog(&catalog);

    for case_id in ["timestamp-gap", "timestamp-reset"] {
        let discontinuous = case(&rows, case_id);
        assert!(discontinuous.iter().all(|row| row.kind == "discontinuous"));

        let error =
            VariableFrameRateMap::from_packet_timings(discontinuous.iter().map(|row| row.packet()))
                .expect_err("an unsegmented discontinuity must be rejected");
        assert!(error.message().contains("gap"));

        let mut segments: BTreeMap<u32, Vec<TimingRow<'_>>> = BTreeMap::new();
        for row in discontinuous {
            segments.entry(row.segment).or_default().push(row);
        }
        assert_eq!(segments.len(), 2);

        for segment in segments.values() {
            let timebase = segment[0].timebase();
            let normalizer = TimestampNormalizer::from_presentation_timestamps(
                timebase,
                segment.iter().map(|row| row.presentation_timestamp),
            );
            let normalized_packets: Vec<_> = segment
                .iter()
                .map(|row| {
                    let source_presentation =
                        RationalTime::new(row.presentation_timestamp, timebase);
                    let source_decode = RationalTime::new(row.decode_timestamp, timebase);
                    let normalized_presentation = normalizer
                        .normalize(source_presentation)
                        .expect("presentation timestamp must normalize");
                    let normalized_decode = normalizer
                        .normalize(source_decode)
                        .expect("decode timestamp must normalize");
                    assert_eq!(
                        normalizer
                            .restore(normalized_presentation)
                            .expect("normalized timestamp must restore"),
                        source_presentation
                    );
                    PacketTiming::new(
                        timebase,
                        Some(normalized_presentation.value()),
                        Some(normalized_decode.value()),
                        Some(row.duration),
                    )
                    .unwrap()
                })
                .collect();
            let mapping = VariableFrameRateMap::from_packet_timings(normalized_packets)
                .expect("one explicit continuity segment must map");
            assert_eq!(mapping.presentation_start().value(), 0);
            assert_eq!(mapping.presentation_end().value(), 80);
            assert!(!mapping.is_variable_frame_rate());
        }
    }

    let gap = case(&rows, "timestamp-gap");
    assert!(gap[2].presentation_timestamp > gap[1].presentation_timestamp + gap[1].duration as i64);
    let reset = case(&rows, "timestamp-reset");
    assert!(reset[2].presentation_timestamp < reset[1].presentation_timestamp);
}
