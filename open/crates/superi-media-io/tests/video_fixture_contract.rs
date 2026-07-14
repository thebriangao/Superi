use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use sha2::{Digest, Sha256};
use superi_core::color_space::ColorSpace;
use superi_core::pixel::{AlphaMode, ChromaSubsampling, PixelFormat, PixelModel, PixelPacking};
use superi_core::time::{Duration, FrameRate, RationalTime};
use superi_media_io::decode::{
    CpuVideoBuffer, FrameStorageKind, VideoFormat, VideoFrame, VideoPlane,
};

const HEADER: &str = "case_id,pixel_format,frame_rate_numerator,frame_rate_denominator,width,height,plane_index,offset,bytes,stride,rows,sha256";
const WIDTH: u32 = 5;
const HEIGHT: u32 = 3;

#[derive(Debug)]
struct CatalogRow {
    case_id: String,
    pixel_format: String,
    rate_numerator: u32,
    rate_denominator: u32,
    width: u32,
    height: u32,
    plane_index: usize,
    offset: usize,
    bytes: usize,
    stride: usize,
    rows: u32,
    sha256: String,
}

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/video/pixel-formats/v1")
}

fn standard_frame_rates() -> [FrameRate; 9] {
    [
        FrameRate::FPS_24,
        FrameRate::FPS_25,
        FrameRate::FPS_30,
        FrameRate::FPS_48,
        FrameRate::FPS_50,
        FrameRate::FPS_60,
        FrameRate::FPS_24000_1001,
        FrameRate::FPS_30000_1001,
        FrameRate::FPS_60000_1001,
    ]
}

fn parse_catalog(text: &str) -> Vec<CatalogRow> {
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some(HEADER));

    lines
        .map(|line| {
            let fields = line.split(',').collect::<Vec<_>>();
            assert_eq!(fields.len(), 12, "catalog records use a fixed schema");
            CatalogRow {
                case_id: fields[0].to_owned(),
                pixel_format: fields[1].to_owned(),
                rate_numerator: fields[2].parse().expect("rate numerator must be numeric"),
                rate_denominator: fields[3].parse().expect("rate denominator must be numeric"),
                width: fields[4].parse().expect("width must be numeric"),
                height: fields[5].parse().expect("height must be numeric"),
                plane_index: fields[6].parse().expect("plane index must be numeric"),
                offset: fields[7].parse().expect("offset must be numeric"),
                bytes: fields[8].parse().expect("byte count must be numeric"),
                stride: fields[9].parse().expect("stride must be numeric"),
                rows: fields[10].parse().expect("rows must be numeric"),
                sha256: fields[11].to_owned(),
            }
        })
        .collect()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut value = String::with_capacity(64);
    for byte in digest {
        use std::fmt::Write as _;
        write!(&mut value, "{byte:02x}").expect("writing to a String cannot fail");
    }
    value
}

fn expected_plane_geometry(pixel_format: PixelFormat) -> Vec<(usize, u32)> {
    let sample_bytes = usize::from(pixel_format.bits_per_component().div_ceil(8));
    match pixel_format.packing() {
        PixelPacking::Packed => vec![(
            WIDTH as usize
                * usize::from(
                    pixel_format
                        .packed_bytes_per_pixel()
                        .expect("packed formats expose bytes per pixel"),
                ),
            HEIGHT,
        )],
        PixelPacking::Planar => {
            let (chroma_width, chroma_height) = match pixel_format
                .chroma_subsampling()
                .expect("planar YUV exposes subsampling")
            {
                ChromaSubsampling::Cs420 => (WIDTH.div_ceil(2), HEIGHT.div_ceil(2)),
                ChromaSubsampling::Cs422 => (WIDTH.div_ceil(2), HEIGHT),
                ChromaSubsampling::Cs444 => (WIDTH, HEIGHT),
                _ => panic!("fixture contract must recognize every chroma subsampling mode"),
            };
            vec![
                (WIDTH as usize * sample_bytes, HEIGHT),
                (chroma_width as usize * sample_bytes, chroma_height),
                (chroma_width as usize * sample_bytes, chroma_height),
            ]
        }
        PixelPacking::Semiplanar => vec![
            (WIDTH as usize * sample_bytes, HEIGHT),
            (
                WIDTH.div_ceil(2) as usize * 2 * sample_bytes,
                HEIGHT.div_ceil(2),
            ),
        ],
        _ => panic!("fixture contract must recognize every pixel packing mode"),
    }
}

fn assert_representative_samples(pixel_format: PixelFormat, bytes: &[u8]) {
    match pixel_format {
        PixelFormat::R16Float | PixelFormat::Rg16Float | PixelFormat::Rgba16Float => {
            for sample in bytes.chunks_exact(2) {
                let bits = u16::from_le_bytes([sample[0], sample[1]]);
                assert_ne!(bits & 0x7c00, 0x7c00, "half samples must be finite");
            }
        }
        PixelFormat::R32Float | PixelFormat::Rg32Float | PixelFormat::Rgba32Float => {
            for sample in bytes.chunks_exact(4) {
                let value = f32::from_le_bytes([sample[0], sample[1], sample[2], sample[3]]);
                assert!(value.is_finite());
                assert!((0.0..=1.0).contains(&value));
            }
        }
        PixelFormat::Yuv420p10 | PixelFormat::Yuv422p10 | PixelFormat::Yuv444p10 => {
            for sample in bytes.chunks_exact(2) {
                assert!(u16::from_le_bytes([sample[0], sample[1]]) <= 1_023);
            }
        }
        PixelFormat::P010 => {
            for sample in bytes.chunks_exact(2) {
                let value = u16::from_le_bytes([sample[0], sample[1]]);
                assert_eq!(value & 0x003f, 0, "P010 stores samples in the high bits");
                assert!(value >> 6 <= 1_023);
            }
        }
        _ => {}
    }
}

#[test]
fn canonical_video_fixture_covers_every_supported_format_and_standard_rate() {
    let root = fixture_root();
    let catalog_bytes = fs::read(root.join("video-cases.csv")).expect("catalog must exist");
    let payload = fs::read(root.join("video-frames.bin")).expect("payload must exist");
    let catalog = std::str::from_utf8(&catalog_bytes).expect("catalog must be UTF-8");

    assert!(catalog.ends_with("\r\n"));
    assert_eq!(catalog.matches("\r\n").count(), catalog.lines().count());
    assert!(
        payload.len() < 64 * 1024,
        "fixture payload must remain tiny"
    );

    let rows = parse_catalog(catalog);
    let rates = standard_frame_rates();
    let mut row_index = 0;
    let mut payload_offset = 0;
    let mut case_count = 0;

    for &pixel_format in PixelFormat::ALL {
        for rate in rates {
            let case_id = format!(
                "{}-{}-{}",
                pixel_format.code(),
                rate.numerator(),
                rate.denominator()
            );
            let first = rows.get(row_index).expect("every expected case must exist");
            assert_eq!(first.case_id, case_id);
            assert_eq!(first.pixel_format, pixel_format.code());
            assert_eq!(first.rate_numerator, rate.numerator());
            assert_eq!(first.rate_denominator, rate.denominator());
            assert_eq!((first.width, first.height), (WIDTH, HEIGHT));

            let expected_geometry = expected_plane_geometry(pixel_format);
            let mut planes = Vec::new();
            while let Some(row) = rows.get(row_index) {
                if row.case_id != case_id {
                    break;
                }
                assert_eq!(row.pixel_format, pixel_format.code());
                assert_eq!(
                    (row.rate_numerator, row.rate_denominator),
                    (rate.numerator(), rate.denominator())
                );
                assert_eq!((row.width, row.height), (WIDTH, HEIGHT));
                assert_eq!(row.plane_index, planes.len());
                assert_eq!(
                    (row.stride, row.rows),
                    expected_geometry[row.plane_index],
                    "catalog plane geometry must be exact"
                );
                assert_eq!(row.offset, payload_offset, "payloads must have no gaps");
                assert_eq!(row.bytes, row.stride * row.rows as usize);
                let end = row
                    .offset
                    .checked_add(row.bytes)
                    .expect("payload range must fit");
                let bytes = payload
                    .get(row.offset..end)
                    .expect("payload range must exist");
                assert_eq!(sha256_hex(bytes), row.sha256);
                assert_representative_samples(pixel_format, bytes);
                planes.push(
                    VideoPlane::new(Arc::from(bytes.to_vec()), row.stride, row.rows)
                        .expect("fixture plane geometry must be valid"),
                );
                payload_offset = end;
                row_index += 1;
            }
            assert!(!planes.is_empty());

            let color_space = if pixel_format.model() == PixelModel::Yuv {
                ColorSpace::BT709
            } else {
                ColorSpace::SRGB
            };
            let format =
                VideoFormat::new(WIDTH, HEIGHT, pixel_format, color_space, AlphaMode::Opaque)
                    .expect("fixture format must be valid");
            let buffer = Arc::new(
                CpuVideoBuffer::new(WIDTH, HEIGHT, pixel_format, planes)
                    .expect("fixture buffer must satisfy media I/O geometry"),
            );
            let frame = VideoFrame::new(
                format,
                RationalTime::zero(rate.timebase()),
                Duration::new(1, rate.timebase()).expect("one frame duration must be valid"),
                buffer,
            )
            .expect("fixture must construct through the public video-frame path");

            assert_eq!(frame.format(), format);
            assert_eq!(frame.buffer().storage_kind(), FrameStorageKind::Cpu);
            assert_eq!(frame.timestamp(), RationalTime::zero(rate.timebase()));
            case_count += 1;
        }
    }

    assert_eq!(case_count, PixelFormat::ALL.len() * rates.len());
    assert_eq!(case_count, 207);
    assert_eq!(row_index, rows.len(), "catalog must contain no extra cases");
    assert_eq!(
        payload_offset,
        payload.len(),
        "payload must contain no extra bytes"
    );
}
