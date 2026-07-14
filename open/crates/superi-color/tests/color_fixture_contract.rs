use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use superi_color::config::ColorManagementConfig;
use superi_color::hdr::Nits;
use superi_color::transform_in::{InputColorTransform, InputSourceKind, InputTransformOptions};
use superi_color::transform_out::{OutputColorTransform, OutputTargetKind, OutputTransformOptions};
use superi_color::working_space::WorkingSpace;
use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::geometry::PixelBounds;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_image::value::{Image, ImageDescriptor, ImageSamples};

const HEADER: &str = "image_id,source_kind,source_primaries,source_transfer,source_matrix,source_range,pixel_format,alpha_mode,width,height,offset,bytes,sha256,output_target,output_primaries,output_transfer,output_matrix,output_range,pq_reference_white_nits";
const CONFIG: &[u8] = br#"{
  "schema": "superi.color-config",
  "version": 1,
  "id": "canonical-fixture",
  "default_working_space": "acescg",
  "roles": { "scene_linear": "acescg" },
  "working_spaces": [{
    "id": "acescg",
    "primaries": "aces_ap1",
    "transfer": "linear",
    "matrix": "rgb",
    "range": "full"
  }]
}"#;

#[derive(Clone, Copy, Debug)]
struct ImageRow<'a> {
    image_id: &'a str,
    source_kind: InputSourceKind,
    source: ColorSpace,
    pixel_format: PixelFormat,
    alpha_mode: AlphaMode,
    width: u32,
    height: u32,
    offset: usize,
    bytes: usize,
    sha256: &'a str,
    output_target: OutputTargetKind,
    output: ColorSpace,
    pq_reference_white_nits: Option<f64>,
}

fn canonical_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test-fixtures/color/image-sequences/v1")
}

fn read_fixture() -> (String, Vec<u8>) {
    let root = canonical_fixture();
    (
        fs::read_to_string(root.join("image-cases.csv"))
            .expect("canonical color catalog must exist"),
        fs::read(root.join("image-samples.bin")).expect("canonical color payload must exist"),
    )
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

fn parse_catalog(catalog: &str) -> Vec<ImageRow<'_>> {
    let content = catalog
        .strip_suffix("\r\n")
        .expect("canonical catalog must end with CRLF");
    let mut lines = content.split("\r\n");
    assert_eq!(lines.next(), Some(HEADER));
    let rows = lines
        .enumerate()
        .map(|(index, line)| {
            let fields: Vec<_> = line.split(',').collect();
            assert_eq!(fields.len(), 19, "catalog row {} field count", index + 1);
            let reference_white =
                (!fields[18].is_empty()).then(|| parse_field(fields[18], index + 1, 18));
            let row = ImageRow {
                image_id: fields[0],
                source_kind: source_kind(fields[1]),
                source: color_space(fields[2], fields[3], fields[4], fields[5]),
                pixel_format: pixel_format(fields[6]),
                alpha_mode: alpha_mode(fields[7]),
                width: parse_field(fields[8], index + 1, 8),
                height: parse_field(fields[9], index + 1, 9),
                offset: parse_field(fields[10], index + 1, 10),
                bytes: parse_field(fields[11], index + 1, 11),
                sha256: fields[12],
                output_target: output_target(fields[13]),
                output: color_space(fields[14], fields[15], fields[16], fields[17]),
                pq_reference_white_nits: reference_white,
            };
            assert_eq!(row.width, 2);
            assert_eq!(row.height, 2);
            assert_eq!(
                row.pq_reference_white_nits.is_some(),
                row.source.transfer() == TransferFunction::Pq
            );
            assert_eq!(
                row.pq_reference_white_nits.is_some(),
                row.output.transfer() == TransferFunction::Pq
            );
            row
        })
        .collect::<Vec<_>>();

    assert_eq!(rows.len(), 8);
    assert_eq!(
        rows.iter().map(|row| row.image_id).collect::<Vec<_>>(),
        [
            "sdr-srgb-premultiplied-f32",
            "wide-display-p3-straight-u16",
            "hdr-bt2020-pq-opaque-f32",
            "hdr-bt2020-hlg-opaque-f32",
            "scene-acescg-premultiplied-f16",
            "sequence-acescg-f32-0",
            "sequence-acescg-f32-1",
            "sequence-acescg-f32-2",
        ]
    );
    rows
}

fn source_kind(value: &str) -> InputSourceKind {
    match value {
        "camera" => InputSourceKind::Camera,
        "display_referred" => InputSourceKind::DisplayReferred,
        "scene_referred" => InputSourceKind::SceneReferred,
        _ => panic!("unknown source kind {value}"),
    }
}

fn output_target(value: &str) -> OutputTargetKind {
    match value {
        "display" => OutputTargetKind::Display,
        "deliverable" => OutputTargetKind::Deliverable,
        _ => panic!("unknown output target {value}"),
    }
}

fn color_space(primaries: &str, transfer: &str, matrix: &str, range: &str) -> ColorSpace {
    ColorSpace::new(
        ColorPrimaries::from_code(primaries).expect("known fixture primaries"),
        TransferFunction::from_code(transfer).expect("known fixture transfer"),
        MatrixCoefficients::from_code(matrix).expect("known fixture matrix"),
        ColorRange::from_code(range).expect("known fixture range"),
    )
}

fn pixel_format(value: &str) -> PixelFormat {
    PixelFormat::from_code(value).expect("known fixture pixel format")
}

fn alpha_mode(value: &str) -> AlphaMode {
    AlphaMode::from_code(value).expect("known fixture alpha mode")
}

fn image(row: ImageRow<'_>, payload: &[u8]) -> Image {
    let bytes = &payload[row.offset..row.offset + row.bytes];
    let samples = match row.pixel_format {
        PixelFormat::Rgba16Unorm => ImageSamples::from_u16(
            bytes
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]])),
        ),
        PixelFormat::Rgba16Float => ImageSamples::from_f16_bits(
            bytes
                .chunks_exact(2)
                .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]])),
        ),
        PixelFormat::Rgba32Float => ImageSamples::from_f32_bits(
            bytes
                .chunks_exact(4)
                .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])),
        ),
        _ => panic!("fixture contains an unsupported dense format"),
    };
    let bounds = PixelBounds::from_origin_size(0, 0, row.width, row.height).unwrap();
    Image::new(
        ImageDescriptor::new(bounds, bounds, row.pixel_format, row.source, row.alpha_mode).unwrap(),
        samples,
    )
    .unwrap()
}

fn input_transform(row: ImageRow<'_>) -> InputColorTransform {
    let mut options = InputTransformOptions::new();
    if let Some(reference_white) = row.pq_reference_white_nits {
        options = options.with_pq_reference_white(Nits::new(reference_white).unwrap());
    }
    InputColorTransform::new(
        row.source_kind,
        row.source,
        configured_working_space(),
        options,
    )
    .unwrap()
}

fn output_transform(row: ImageRow<'_>) -> OutputColorTransform {
    let mut options = OutputTransformOptions::new();
    if let Some(reference_white) = row.pq_reference_white_nits {
        options = options.with_pq_reference_white(Nits::new(reference_white).unwrap());
    }
    OutputColorTransform::new(
        row.output_target,
        configured_working_space(),
        row.output,
        options,
    )
    .unwrap()
}

fn configured_working_space() -> WorkingSpace {
    let config = ColorManagementConfig::from_json(CONFIG).unwrap();
    assert_eq!(
        config.role("scene_linear"),
        Some(config.default_working_space())
    );
    config.default_working_space()
}

fn numeric_samples(image: &Image) -> Vec<f32> {
    match image.samples() {
        ImageSamples::U16(values) => values
            .iter()
            .map(|value| f32::from(*value) / 65_535.0)
            .collect(),
        ImageSamples::F16(_) | ImageSamples::F32(_) => (0..image.samples().len())
            .map(|index| image.samples().float_value(index).unwrap())
            .collect(),
        _ => panic!("fixture image must use u16 or floating samples"),
    }
}

fn row<'a>(rows: &'a [ImageRow<'a>], image_id: &str) -> ImageRow<'a> {
    *rows
        .iter()
        .find(|candidate| candidate.image_id == image_id)
        .expect("fixture image must exist")
}

#[test]
fn canonical_catalog_has_strict_identity_offsets_and_hashes() {
    let (catalog, payload) = read_fixture();
    assert_eq!(
        catalog.matches('\n').count(),
        catalog.matches("\r\n").count()
    );
    let rows = parse_catalog(&catalog);

    let mut expected_offset = 0;
    for row in rows {
        assert_eq!(row.offset, expected_offset);
        let end = row.offset.checked_add(row.bytes).unwrap();
        assert!(end <= payload.len());
        assert_eq!(
            format!("{:x}", Sha256::digest(&payload[row.offset..end])),
            row.sha256
        );
        expected_offset = end;
    }
    assert_eq!(expected_offset, payload.len());
}

#[test]
fn every_case_round_trips_through_explicit_input_and_output_intent() {
    let (catalog, payload) = read_fixture();
    let rows = parse_catalog(&catalog);

    for row in rows {
        let source = image(row, &payload);
        let source_samples = numeric_samples(&source);
        let working = input_transform(row).apply_f32(&source).unwrap();
        let output_transform = output_transform(row);
        assert_eq!(output_transform.target_kind(), row.output_target);
        let output = output_transform.apply_f32(&working).unwrap();

        assert_eq!(output.descriptor().color_space(), row.output);
        assert_eq!(output.descriptor().pixel_format(), PixelFormat::Rgba32Float);
        assert_eq!(output.descriptor().alpha_mode(), AlphaMode::Premultiplied);
        for pixel in 0..4 {
            let base = pixel * 4;
            let alpha = if row.alpha_mode == AlphaMode::Opaque {
                1.0
            } else {
                source_samples[base + 3]
            };
            let expected_rgb = if row.alpha_mode == AlphaMode::Straight {
                [
                    source_samples[base] * alpha,
                    source_samples[base + 1] * alpha,
                    source_samples[base + 2] * alpha,
                ]
            } else {
                source_samples[base..base + 3].try_into().unwrap()
            };
            for (channel, expected) in expected_rgb.iter().enumerate() {
                assert_close(
                    output.samples().float_value(base + channel).unwrap(),
                    *expected,
                    8.0e-4,
                );
            }
            assert_close(
                output.samples().float_value(base + 3).unwrap(),
                alpha,
                2.0e-6,
            );
        }
    }
}

#[test]
fn fixtures_prove_transfer_order_hdr_meaning_alpha_and_precision() {
    let (catalog, payload) = read_fixture();
    let rows = parse_catalog(&catalog);

    let sdr = row(&rows, "sdr-srgb-premultiplied-f32");
    let sdr_working = input_transform(sdr)
        .apply_f32(&image(sdr, &payload))
        .unwrap();
    for channel in 0..3 {
        assert_close(
            sdr_working.image().samples().float_value(channel).unwrap(),
            0.25,
            4.0e-6,
        );
    }
    assert_eq!(sdr_working.image().samples().float_value(3), Some(0.5));
    for channel in 4..8 {
        assert_eq!(
            sdr_working.image().samples().float_value(channel),
            Some(0.0)
        );
    }

    let pq = row(&rows, "hdr-bt2020-pq-opaque-f32");
    let pq_working = input_transform(pq).apply_f32(&image(pq, &payload)).unwrap();
    for channel in 0..3 {
        assert_close(
            pq_working.image().samples().float_value(channel).unwrap(),
            1.0,
            8.0e-6,
        );
    }

    let hlg = row(&rows, "hdr-bt2020-hlg-opaque-f32");
    let hlg_working = input_transform(hlg)
        .apply_f32(&image(hlg, &payload))
        .unwrap();
    for channel in 0..3 {
        assert_close(
            hlg_working.image().samples().float_value(channel).unwrap(),
            1.0 / 12.0,
            8.0e-7,
        );
    }

    let f16 = row(&rows, "scene-acescg-premultiplied-f16");
    let f16_image = image(f16, &payload);
    assert_eq!(
        &f16_image.samples().f16_bits().unwrap()[..4],
        &[0xb400, 0x3800, 0x4000, 0x3800]
    );
    let f16_working = input_transform(f16).apply_f32(&f16_image).unwrap();
    assert_eq!(
        (0..4)
            .map(|index| f16_working.image().samples().float_value(index).unwrap())
            .collect::<Vec<_>>(),
        [-0.25, 0.5, 2.0, 0.5]
    );

    let f32 = row(&rows, "sequence-acescg-f32-0");
    let f32_image = image(f32, &payload);
    assert_eq!(f32_image.samples().f32_bits().unwrap()[0], 0x3eaa_aaab);
    let f32_working = input_transform(f32).apply_f32(&f32_image).unwrap();
    assert_eq!(
        f32_working
            .image()
            .samples()
            .float_value(0)
            .unwrap()
            .to_bits(),
        0x3eaa_aaab
    );
}

fn assert_close(actual: f32, expected: f32, tolerance: f32) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "expected {expected:.8}, got {actual:.8}, tolerance {tolerance:.3e}"
    );
}
