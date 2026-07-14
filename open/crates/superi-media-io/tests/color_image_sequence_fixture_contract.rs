use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use superi_core::ids::MediaId;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::time::{FrameRate, RationalTime};
use superi_media_io::decode::{CpuVideoBuffer, VideoFormat, VideoFrame, VideoPlane};
use superi_media_io::demux::SourceIdentity;
use superi_media_io::image_seq::{
    ImageSequenceFrameAddress, ImageSequenceFrameReader, ImageSequenceInfo, ImageSequenceSource,
    ImageSequenceTiming,
};

const IMAGE_HEADER: &str = "image_id,source_kind,source_primaries,source_transfer,source_matrix,source_range,pixel_format,alpha_mode,width,height,offset,bytes,sha256,output_target,output_primaries,output_transfer,output_matrix,output_range,pq_reference_white_nits";
const SEQUENCE_HEADER: &str = "sequence_id,image_number,file_frame_number,presentation_timestamp,rate_numerator,rate_denominator,image_id";

#[derive(Clone, Debug)]
struct ImageRecord {
    image_id: String,
    primaries: String,
    transfer: String,
    matrix: String,
    range: String,
    pixel_format: String,
    alpha_mode: String,
    width: u32,
    height: u32,
    offset: usize,
    bytes: usize,
    sha256: String,
}

#[derive(Clone, Debug)]
struct SequenceRecord {
    sequence_id: String,
    image_number: u64,
    file_frame_number: i64,
    presentation_timestamp: i64,
    rate_numerator: u32,
    rate_denominator: u32,
    image_id: String,
}

fn canonical_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/color/image-sequences/v1")
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

fn read_crlf_lines(path: &Path, header: &str, fields: usize) -> Vec<Vec<String>> {
    let text = fs::read_to_string(path).expect("canonical catalog must exist");
    assert_eq!(text.matches('\n').count(), text.matches("\r\n").count());
    let content = text
        .strip_suffix("\r\n")
        .expect("canonical catalog must end with CRLF");
    let mut lines = content.split("\r\n");
    assert_eq!(lines.next(), Some(header));
    lines
        .enumerate()
        .map(|(index, line)| {
            let values = line.split(',').map(str::to_owned).collect::<Vec<_>>();
            assert_eq!(
                values.len(),
                fields,
                "catalog row {} field count",
                index + 1
            );
            values
        })
        .collect()
}

fn image_records() -> Vec<ImageRecord> {
    read_crlf_lines(
        &canonical_fixture().join("image-cases.csv"),
        IMAGE_HEADER,
        19,
    )
    .into_iter()
    .enumerate()
    .map(|(index, fields)| ImageRecord {
        image_id: fields[0].clone(),
        primaries: fields[2].clone(),
        transfer: fields[3].clone(),
        matrix: fields[4].clone(),
        range: fields[5].clone(),
        pixel_format: fields[6].clone(),
        alpha_mode: fields[7].clone(),
        width: parse_field(&fields[8], index + 1, 8),
        height: parse_field(&fields[9], index + 1, 9),
        offset: parse_field(&fields[10], index + 1, 10),
        bytes: parse_field(&fields[11], index + 1, 11),
        sha256: fields[12].clone(),
    })
    .collect()
}

fn sequence_records() -> Vec<SequenceRecord> {
    read_crlf_lines(
        &canonical_fixture().join("sequence-cases.csv"),
        SEQUENCE_HEADER,
        7,
    )
    .into_iter()
    .enumerate()
    .map(|(index, fields)| SequenceRecord {
        sequence_id: fields[0].clone(),
        image_number: parse_field(&fields[1], index + 1, 1),
        file_frame_number: parse_field(&fields[2], index + 1, 2),
        presentation_timestamp: parse_field(&fields[3], index + 1, 3),
        rate_numerator: parse_field(&fields[4], index + 1, 4),
        rate_denominator: parse_field(&fields[5], index + 1, 5),
        image_id: fields[6].clone(),
    })
    .collect()
}

fn sequence_format() -> VideoFormat {
    VideoFormat::new(
        2,
        2,
        PixelFormat::Rgba32Float,
        ColorSpace::ACESCG,
        AlphaMode::Premultiplied,
    )
    .unwrap()
}

struct FixtureReader {
    images: Vec<ImageRecord>,
    sequence: Vec<SequenceRecord>,
    payload: Arc<[u8]>,
}

impl ImageSequenceFrameReader for FixtureReader {
    fn read_frame(&mut self, address: ImageSequenceFrameAddress) -> Result<VideoFrame> {
        let record = self
            .sequence
            .iter()
            .find(|record| record.image_number == address.image_number())
            .ok_or_else(|| fixture_error("logical image is absent from the fixture"))?;
        if record.file_frame_number != address.file_frame_number()
            || record.presentation_timestamp != address.presentation_time().value()
        {
            return Err(fixture_error(
                "sequence address does not match the fixture catalog",
            ));
        }
        let image = self
            .images
            .iter()
            .find(|image| image.image_id == record.image_id)
            .ok_or_else(|| fixture_error("sequence image is absent from the image catalog"))?;
        if image.primaries != "aces_ap1"
            || image.transfer != "linear"
            || image.matrix != "rgb"
            || image.range != "full"
            || image.pixel_format != "rgba32_float"
            || image.alpha_mode != "premultiplied"
            || image.width != 2
            || image.height != 2
            || image.bytes != 64
        {
            return Err(fixture_error(
                "sequence image format changed within the fixture",
            ));
        }
        let end = image
            .offset
            .checked_add(image.bytes)
            .ok_or_else(|| fixture_error("fixture image range overflowed"))?;
        let bytes: Arc<[u8]> = self
            .payload
            .get(image.offset..end)
            .ok_or_else(|| fixture_error("fixture image range is outside the payload"))?
            .into();
        let plane = VideoPlane::new(bytes, 32, 2)?;
        let buffer = Arc::new(CpuVideoBuffer::new(
            2,
            2,
            PixelFormat::Rgba32Float,
            vec![plane],
        )?);
        VideoFrame::new(
            sequence_format(),
            address.presentation_time(),
            address.duration(),
            buffer,
        )
    }
}

fn fixture_error(message: &'static str) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::Degraded,
        message,
    )
}

#[test]
fn canonical_sequence_catalog_has_strict_references_timing_and_payloads() {
    let images = image_records();
    let sequence = sequence_records();
    let payload = fs::read(canonical_fixture().join("image-samples.bin"))
        .expect("canonical image payload must exist");

    assert_eq!(images.len(), 8);
    assert_eq!(sequence.len(), 3);
    assert!(sequence
        .iter()
        .all(|record| record.sequence_id == "acescg-editorial-sequence"));
    assert_eq!(
        sequence
            .iter()
            .map(|record| record.image_number)
            .collect::<Vec<_>>(),
        [0, 1, 2]
    );
    assert_eq!(
        sequence
            .iter()
            .map(|record| record.file_frame_number)
            .collect::<Vec<_>>(),
        [-2, 0, 2]
    );
    assert_eq!(
        sequence
            .iter()
            .map(|record| record.presentation_timestamp)
            .collect::<Vec<_>>(),
        [48, 49, 50]
    );
    assert_eq!(
        sequence
            .iter()
            .map(|record| record.image_id.as_str())
            .collect::<Vec<_>>(),
        [
            "sequence-acescg-f32-0",
            "sequence-acescg-f32-1",
            "sequence-acescg-f32-2",
        ]
    );
    assert!(sequence
        .iter()
        .all(|record| (record.rate_numerator, record.rate_denominator) == (24_000, 1_001)));

    for record in &sequence {
        let image = images
            .iter()
            .find(|image| image.image_id == record.image_id)
            .expect("sequence reference must resolve");
        let end = image.offset + image.bytes;
        assert_eq!(image.primaries, "aces_ap1");
        assert_eq!(image.transfer, "linear");
        assert_eq!(image.matrix, "rgb");
        assert_eq!(image.range, "full");
        assert_eq!(image.pixel_format, "rgba32_float");
        assert_eq!(image.alpha_mode, "premultiplied");
        assert_eq!((image.width, image.height, image.bytes), (2, 2, 64));
        assert_eq!(
            format!("{:x}", Sha256::digest(&payload[image.offset..end])),
            image.sha256
        );
    }
}

#[test]
fn public_image_sequence_source_preserves_logical_file_and_presentation_identity() {
    let images = image_records();
    let sequence = sequence_records();
    let payload: Arc<[u8]> = fs::read(canonical_fixture().join("image-samples.bin"))
        .expect("canonical image payload must exist")
        .into();
    let timing = ImageSequenceTiming::new(-2, 2, 3, FrameRate::FPS_24000_1001)
        .unwrap()
        .with_presentation_start(RationalTime::new(48, FrameRate::FPS_24000_1001.timebase()))
        .unwrap();
    let info = ImageSequenceInfo::new(
        SourceIdentity::new(
            MediaId::from_raw(21),
            format!("sha256:{:x}", Sha256::digest(payload.as_ref())),
        )
        .unwrap(),
        timing,
        sequence_format(),
    );
    let mut source = ImageSequenceSource::new(
        info,
        Box::new(FixtureReader {
            images: images.clone(),
            sequence,
            payload: Arc::clone(&payload),
        }),
    );

    let last = source.read_frame(2).unwrap();
    assert_eq!(last.timestamp().value(), 50);
    assert_eq!(last.format(), sequence_format());
    assert_frame_matches(&last, &images, "sequence-acescg-f32-2", &payload);

    let middle = source
        .seek(RationalTime::new(49, FrameRate::FPS_24000_1001.timebase()))
        .unwrap();
    assert_eq!(middle.timestamp().value(), 49);
    assert_frame_matches(&middle, &images, "sequence-acescg-f32-1", &payload);

    let last_bytes = last
        .buffer()
        .as_any()
        .downcast_ref::<CpuVideoBuffer>()
        .unwrap()
        .planes()[0]
        .bytes();
    let middle_bytes = middle
        .buffer()
        .as_any()
        .downcast_ref::<CpuVideoBuffer>()
        .unwrap()
        .planes()[0]
        .bytes();
    assert_ne!(last_bytes, middle_bytes);
}

fn assert_frame_matches(
    frame: &VideoFrame,
    images: &[ImageRecord],
    image_id: &str,
    payload: &[u8],
) {
    let image = images
        .iter()
        .find(|image| image.image_id == image_id)
        .unwrap();
    let buffer = frame
        .buffer()
        .as_any()
        .downcast_ref::<CpuVideoBuffer>()
        .expect("fixture frame must use CPU storage");
    assert_eq!(
        buffer.planes()[0].bytes(),
        &payload[image.offset..image.offset + image.bytes]
    );
}
