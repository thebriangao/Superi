//! Offline validation for Superi's canonical test fixtures.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use sha2::{Digest, Sha256};

const MANIFEST_NAME: &str = "fixture.json";
const POLICY_NAME: &str = "README.md";
const SUPPORTED_SCHEMA_VERSION: u32 = 1;

pub const VIDEO_CATALOG_NAME: &str = "video-cases.csv";
pub const VIDEO_PAYLOAD_NAME: &str = "video-frames.bin";
pub const VIDEO_MANIFEST_NAME: &str = MANIFEST_NAME;
pub const AUDIO_STEREO_44100_NAME: &str = "stereo-44100.wav";
pub const AUDIO_SURROUND_5_1_48000_NAME: &str = "surround-5-1-48000.wav";
pub const AUDIO_SURROUND_7_1_96000_NAME: &str = "surround-7-1-96000.wav";
pub const AUDIO_MANIFEST_NAME: &str = MANIFEST_NAME;

const VIDEO_WIDTH: usize = 5;
const VIDEO_HEIGHT: u32 = 3;
const VIDEO_CATALOG_HEADER: &str = "case_id,pixel_format,frame_rate_numerator,frame_rate_denominator,width,height,plane_index,offset,bytes,stride,rows,sha256";
const VIDEO_FRAME_RATES: [(u32, u32); 9] = [
    (24, 1),
    (25, 1),
    (30, 1),
    (48, 1),
    (50, 1),
    (60, 1),
    (24_000, 1_001),
    (30_000, 1_001),
    (60_000, 1_001),
];

#[derive(Clone, Copy)]
enum SampleKind {
    U8,
    U10,
    U16,
    P010,
    F16,
    F32,
}

impl SampleKind {
    const fn bytes(self) -> usize {
        match self {
            Self::U8 => 1,
            Self::U10 | Self::U16 | Self::P010 | Self::F16 => 2,
            Self::F32 => 4,
        }
    }
}

#[derive(Clone, Copy)]
enum Subsampling {
    Cs420,
    Cs422,
    Cs444,
}

#[derive(Clone, Copy)]
enum PixelLayout {
    Packed { components: usize },
    Planar { subsampling: Subsampling },
    Semiplanar,
}

#[derive(Clone, Copy)]
struct PixelSpec {
    code: &'static str,
    sample: SampleKind,
    layout: PixelLayout,
}

const VIDEO_PIXEL_SPECS: [PixelSpec; 23] = [
    PixelSpec {
        code: "r8_unorm",
        sample: SampleKind::U8,
        layout: PixelLayout::Packed { components: 1 },
    },
    PixelSpec {
        code: "r16_unorm",
        sample: SampleKind::U16,
        layout: PixelLayout::Packed { components: 1 },
    },
    PixelSpec {
        code: "r16_float",
        sample: SampleKind::F16,
        layout: PixelLayout::Packed { components: 1 },
    },
    PixelSpec {
        code: "r32_float",
        sample: SampleKind::F32,
        layout: PixelLayout::Packed { components: 1 },
    },
    PixelSpec {
        code: "rg8_unorm",
        sample: SampleKind::U8,
        layout: PixelLayout::Packed { components: 2 },
    },
    PixelSpec {
        code: "rg16_unorm",
        sample: SampleKind::U16,
        layout: PixelLayout::Packed { components: 2 },
    },
    PixelSpec {
        code: "rg16_float",
        sample: SampleKind::F16,
        layout: PixelLayout::Packed { components: 2 },
    },
    PixelSpec {
        code: "rg32_float",
        sample: SampleKind::F32,
        layout: PixelLayout::Packed { components: 2 },
    },
    PixelSpec {
        code: "rgb8_unorm",
        sample: SampleKind::U8,
        layout: PixelLayout::Packed { components: 3 },
    },
    PixelSpec {
        code: "bgr8_unorm",
        sample: SampleKind::U8,
        layout: PixelLayout::Packed { components: 3 },
    },
    PixelSpec {
        code: "rgba8_unorm",
        sample: SampleKind::U8,
        layout: PixelLayout::Packed { components: 4 },
    },
    PixelSpec {
        code: "bgra8_unorm",
        sample: SampleKind::U8,
        layout: PixelLayout::Packed { components: 4 },
    },
    PixelSpec {
        code: "rgba16_unorm",
        sample: SampleKind::U16,
        layout: PixelLayout::Packed { components: 4 },
    },
    PixelSpec {
        code: "rgba16_float",
        sample: SampleKind::F16,
        layout: PixelLayout::Packed { components: 4 },
    },
    PixelSpec {
        code: "rgba32_float",
        sample: SampleKind::F32,
        layout: PixelLayout::Packed { components: 4 },
    },
    PixelSpec {
        code: "yuv420p8",
        sample: SampleKind::U8,
        layout: PixelLayout::Planar {
            subsampling: Subsampling::Cs420,
        },
    },
    PixelSpec {
        code: "yuv420p10",
        sample: SampleKind::U10,
        layout: PixelLayout::Planar {
            subsampling: Subsampling::Cs420,
        },
    },
    PixelSpec {
        code: "yuv422p8",
        sample: SampleKind::U8,
        layout: PixelLayout::Planar {
            subsampling: Subsampling::Cs422,
        },
    },
    PixelSpec {
        code: "yuv422p10",
        sample: SampleKind::U10,
        layout: PixelLayout::Planar {
            subsampling: Subsampling::Cs422,
        },
    },
    PixelSpec {
        code: "yuv444p8",
        sample: SampleKind::U8,
        layout: PixelLayout::Planar {
            subsampling: Subsampling::Cs444,
        },
    },
    PixelSpec {
        code: "yuv444p10",
        sample: SampleKind::U10,
        layout: PixelLayout::Planar {
            subsampling: Subsampling::Cs444,
        },
    },
    PixelSpec {
        code: "nv12",
        sample: SampleKind::U8,
        layout: PixelLayout::Semiplanar,
    },
    PixelSpec {
        code: "p010",
        sample: SampleKind::P010,
        layout: PixelLayout::Semiplanar,
    },
];

pub const VIDEO_BASELINE_CASE_COUNT: usize = VIDEO_PIXEL_SPECS.len() * VIDEO_FRAME_RATES.len();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VideoBaselineReport {
    case_count: usize,
    payload_bytes: usize,
}

impl VideoBaselineReport {
    #[must_use]
    pub const fn case_count(self) -> usize {
        self.case_count
    }

    #[must_use]
    pub const fn payload_bytes(self) -> usize {
        self.payload_bytes
    }
}

#[derive(Clone, Copy)]
struct PlaneSpec {
    stride: usize,
    rows: u32,
}

pub fn generate_video_baseline(output_directory: &Path) -> io::Result<VideoBaselineReport> {
    match fs::symlink_metadata(output_directory) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "output directory already exists",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let mut catalog = String::from(VIDEO_CATALOG_HEADER);
    catalog.push_str("\r\n");
    let mut payload = Vec::new();
    let mut case_count = 0;

    for pixel in VIDEO_PIXEL_SPECS {
        let plane_specs = video_plane_specs(pixel);
        for (rate_numerator, rate_denominator) in VIDEO_FRAME_RATES {
            let case_id = format!("{}-{rate_numerator}-{rate_denominator}", pixel.code);
            for (plane_index, plane) in plane_specs.iter().enumerate() {
                let offset = payload.len();
                let sample_count = plane.stride * plane.rows as usize / pixel.sample.bytes();
                for sample_index in 0..sample_count {
                    append_sample(
                        &mut payload,
                        pixel.sample,
                        case_count * 131 + plane_index * 17 + sample_index,
                    );
                }
                let bytes = payload.len() - offset;
                let digest = digest_bytes(&payload[offset..]);
                catalog.push_str(&format!(
                    "{case_id},{},{rate_numerator},{rate_denominator},{VIDEO_WIDTH},{VIDEO_HEIGHT},{plane_index},{offset},{bytes},{},{},{digest}\r\n",
                    pixel.code, plane.stride, plane.rows
                ));
            }
            case_count += 1;
        }
    }

    debug_assert_eq!(case_count, VIDEO_BASELINE_CASE_COUNT);
    let catalog_bytes = catalog.as_bytes();
    let manifest = video_manifest(catalog_bytes, &payload);

    if let Some(parent) = output_directory.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::create_dir(output_directory)?;
    fs::write(output_directory.join(VIDEO_CATALOG_NAME), catalog_bytes)?;
    fs::write(output_directory.join(VIDEO_PAYLOAD_NAME), &payload)?;
    fs::write(output_directory.join(VIDEO_MANIFEST_NAME), manifest)?;

    Ok(VideoBaselineReport {
        case_count,
        payload_bytes: payload.len(),
    })
}

fn video_plane_specs(pixel: PixelSpec) -> Vec<PlaneSpec> {
    let sample_bytes = pixel.sample.bytes();
    match pixel.layout {
        PixelLayout::Packed { components } => vec![PlaneSpec {
            stride: VIDEO_WIDTH * components * sample_bytes,
            rows: VIDEO_HEIGHT,
        }],
        PixelLayout::Planar { subsampling } => {
            let (chroma_width, chroma_height) = match subsampling {
                Subsampling::Cs420 => (VIDEO_WIDTH.div_ceil(2), VIDEO_HEIGHT.div_ceil(2)),
                Subsampling::Cs422 => (VIDEO_WIDTH.div_ceil(2), VIDEO_HEIGHT),
                Subsampling::Cs444 => (VIDEO_WIDTH, VIDEO_HEIGHT),
            };
            vec![
                PlaneSpec {
                    stride: VIDEO_WIDTH * sample_bytes,
                    rows: VIDEO_HEIGHT,
                },
                PlaneSpec {
                    stride: chroma_width * sample_bytes,
                    rows: chroma_height,
                },
                PlaneSpec {
                    stride: chroma_width * sample_bytes,
                    rows: chroma_height,
                },
            ]
        }
        PixelLayout::Semiplanar => vec![
            PlaneSpec {
                stride: VIDEO_WIDTH * sample_bytes,
                rows: VIDEO_HEIGHT,
            },
            PlaneSpec {
                stride: VIDEO_WIDTH.div_ceil(2) * 2 * sample_bytes,
                rows: VIDEO_HEIGHT.div_ceil(2),
            },
        ],
    }
}

fn append_sample(bytes: &mut Vec<u8>, kind: SampleKind, seed: usize) {
    match kind {
        SampleKind::U8 => bytes.push((seed.wrapping_mul(37).wrapping_add(17) & 0xff) as u8),
        SampleKind::U10 => {
            let value = (seed.wrapping_mul(43).wrapping_add(29) & 0x03ff) as u16;
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        SampleKind::U16 => {
            let value = (seed.wrapping_mul(977).wrapping_add(257) & 0xffff) as u16;
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        SampleKind::P010 => {
            let value = ((seed.wrapping_mul(43).wrapping_add(29) & 0x03ff) as u16) << 6;
            bytes.extend_from_slice(&value.to_le_bytes());
        }
        SampleKind::F16 => {
            const FINITE_HALF_BITS: [u16; 6] = [0x0000, 0x3000, 0x3400, 0x3800, 0x3a00, 0x3c00];
            bytes.extend_from_slice(&FINITE_HALF_BITS[seed % FINITE_HALF_BITS.len()].to_le_bytes());
        }
        SampleKind::F32 => {
            let value = (seed % 17) as f32 / 16.0;
            bytes.extend_from_slice(&value.to_le_bytes());
        }
    }
}

fn video_manifest(catalog: &[u8], payload: &[u8]) -> String {
    format!(
        r#"{{
  "schema_version": 1,
  "fixture_id": "video/pixel-formats",
  "fixture_version": 1,
  "description": "Tiny deterministic raw frames for every supported pixel format and standard frame rate.",
  "provenance": {{
    "kind": "generated",
    "source": "Authored and generated in the Superi repository from stable format and frame-rate tables.",
    "author": "Superi contributors",
    "created_on": "2026-07-14",
    "license": "CC0-1.0",
    "rights": "Original synthetic bytes approved for unrestricted redistribution.",
    "generator": {{
      "name": "superi-fixture-tool",
      "version": "0.0.0",
      "command": "cargo run -p superi-fixture-tool -- generate-video <OUTPUT_DIRECTORY>",
      "seed": "superi-video-baseline-v1"
    }},
    "parents": []
  }},
  "files": [
    {{
      "path": "{VIDEO_CATALOG_NAME}",
      "media_type": "text/csv; charset=utf-8",
      "bytes": {},
      "sha256": "{}"
    }},
    {{
      "path": "{VIDEO_PAYLOAD_NAME}",
      "media_type": "application/octet-stream",
      "bytes": {},
      "sha256": "{}"
    }}
  ]
}}
"#,
        catalog.len(),
        digest_bytes(catalog),
        payload.len(),
        digest_bytes(payload)
    )
}

#[derive(Clone, Copy)]
struct AudioSpec {
    name: &'static str,
    sample_rate: u32,
    channels: u16,
    channel_mask: u32,
}

const AUDIO_SPECS: [AudioSpec; 3] = [
    AudioSpec {
        name: AUDIO_STEREO_44100_NAME,
        sample_rate: 44_100,
        channels: 2,
        channel_mask: 0x0003,
    },
    AudioSpec {
        name: AUDIO_SURROUND_5_1_48000_NAME,
        sample_rate: 48_000,
        channels: 6,
        channel_mask: 0x003f,
    },
    AudioSpec {
        name: AUDIO_SURROUND_7_1_96000_NAME,
        sample_rate: 96_000,
        channels: 8,
        channel_mask: 0x063f,
    },
];

pub const AUDIO_BASELINE_CASE_COUNT: usize = AUDIO_SPECS.len();

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AudioBaselineReport {
    case_count: usize,
    payload_bytes: usize,
}

impl AudioBaselineReport {
    #[must_use]
    pub const fn case_count(self) -> usize {
        self.case_count
    }

    #[must_use]
    pub const fn payload_bytes(self) -> usize {
        self.payload_bytes
    }
}

struct AudioArtifact {
    spec: AudioSpec,
    bytes: Vec<u8>,
}

/// Creates the deterministic synchronized multichannel WAVE fixture baseline.
pub fn generate_audio_baseline(output_directory: &Path) -> io::Result<AudioBaselineReport> {
    match fs::symlink_metadata(output_directory) {
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "output directory already exists",
            ));
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }

    let artifacts = AUDIO_SPECS
        .into_iter()
        .map(|spec| AudioArtifact {
            spec,
            bytes: audio_wave(spec),
        })
        .collect::<Vec<_>>();
    let payload_bytes = artifacts.iter().map(|artifact| artifact.bytes.len()).sum();
    let manifest = audio_manifest(&artifacts);

    if let Some(parent) = output_directory.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }
    fs::create_dir(output_directory)?;
    for artifact in &artifacts {
        fs::write(output_directory.join(artifact.spec.name), &artifact.bytes)?;
    }
    fs::write(output_directory.join(AUDIO_MANIFEST_NAME), manifest)?;

    Ok(AudioBaselineReport {
        case_count: artifacts.len(),
        payload_bytes,
    })
}

fn audio_wave(spec: AudioSpec) -> Vec<u8> {
    const BITS_PER_SAMPLE: u16 = 16;
    const PCM_SUBFORMAT_GUID: [u8; 16] = [
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b,
        0x71,
    ];

    let frame_count = spec.sample_rate / 10;
    let block_align = spec.channels * (BITS_PER_SAMPLE / 8);
    let mut samples = Vec::with_capacity(frame_count as usize * usize::from(block_align));
    for frame in 0..frame_count {
        for channel in 0..usize::from(spec.channels) {
            samples
                .extend_from_slice(&audio_sample(frame, spec.sample_rate, channel).to_le_bytes());
        }
    }

    let riff_size = 60_u32 + u32::try_from(samples.len()).expect("audio fixture remains tiny");
    let mut bytes = Vec::with_capacity(68 + samples.len());
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&riff_size.to_le_bytes());
    bytes.extend_from_slice(b"WAVE");
    bytes.extend_from_slice(b"fmt ");
    bytes.extend_from_slice(&40_u32.to_le_bytes());
    bytes.extend_from_slice(&0xfffe_u16.to_le_bytes());
    bytes.extend_from_slice(&spec.channels.to_le_bytes());
    bytes.extend_from_slice(&spec.sample_rate.to_le_bytes());
    bytes.extend_from_slice(&(spec.sample_rate * u32::from(block_align)).to_le_bytes());
    bytes.extend_from_slice(&block_align.to_le_bytes());
    bytes.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    bytes.extend_from_slice(&22_u16.to_le_bytes());
    bytes.extend_from_slice(&BITS_PER_SAMPLE.to_le_bytes());
    bytes.extend_from_slice(&spec.channel_mask.to_le_bytes());
    bytes.extend_from_slice(&PCM_SUBFORMAT_GUID);
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(
        &u32::try_from(samples.len())
            .expect("audio fixture remains tiny")
            .to_le_bytes(),
    );
    bytes.extend_from_slice(&samples);
    bytes
}

fn audio_sample(frame: u32, sample_rate: u32, channel: usize) -> i16 {
    let onset = sample_rate / 100;
    let tail = sample_rate * 9 / 100;
    if frame < onset || frame >= tail {
        return 0;
    }

    let elapsed = frame - onset;
    let phase = i64::from((u64::from(elapsed) * 1_000 % u64::from(sample_rate)) as u32);
    let rate = i64::from(sample_rate);
    let four_phase = phase * 4;
    let triangle = if four_phase < rate {
        four_phase
    } else if four_phase < rate * 3 {
        rate * 2 - four_phase
    } else {
        four_phase - rate * 4
    };
    let gain = 768 * i64::try_from(channel + 1).expect("audio fixture channel index must fit");
    i16::try_from(triangle * gain / rate).expect("audio fixture sample must fit PCM16")
}

fn audio_manifest(artifacts: &[AudioArtifact]) -> String {
    debug_assert_eq!(artifacts.len(), AUDIO_BASELINE_CASE_COUNT);
    format!(
        r#"{{
  "schema_version": 1,
  "fixture_id": "audio/synchronized-multichannel",
  "fixture_version": 1,
  "description": "Deterministic synchronized PCM16 WAVE fixtures at common sample rates and canonical speaker layouts.",
  "provenance": {{
    "kind": "generated",
    "source": "Authored and generated in the Superi repository from stable sample-rate, channel-mask, timing, and integer-waveform rules.",
    "author": "Superi contributors",
    "created_on": "2026-07-14",
    "license": "CC0-1.0",
    "rights": "Original synthetic audio approved for unrestricted redistribution.",
    "generator": {{
      "name": "superi-fixture-tool",
      "version": "0.0.0",
      "command": "cargo run -p superi-fixture-tool -- generate-audio <OUTPUT_DIRECTORY>",
      "seed": "superi-audio-baseline-v1"
    }},
    "parents": []
  }},
  "files": [
    {{
      "path": "{}",
      "media_type": "audio/wav",
      "bytes": {},
      "sha256": "{}"
    }},
    {{
      "path": "{}",
      "media_type": "audio/wav",
      "bytes": {},
      "sha256": "{}"
    }},
    {{
      "path": "{}",
      "media_type": "audio/wav",
      "bytes": {},
      "sha256": "{}"
    }}
  ]
}}
"#,
        artifacts[0].spec.name,
        artifacts[0].bytes.len(),
        digest_bytes(&artifacts[0].bytes),
        artifacts[1].spec.name,
        artifacts[1].bytes.len(),
        digest_bytes(&artifacts[1].bytes),
        artifacts[2].spec.name,
        artifacts[2].bytes.len(),
        digest_bytes(&artifacts[2].bytes),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationReport {
    fixture_count: usize,
    payload_count: usize,
}

impl ValidationReport {
    #[must_use]
    pub const fn fixture_count(self) -> usize {
        self.fixture_count
    }

    #[must_use]
    pub const fn payload_count(self) -> usize {
        self.payload_count
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    code: &'static str,
    path: PathBuf,
    message: String,
}

impl ValidationError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationErrors(Vec<ValidationError>);

impl ValidationErrors {
    pub fn iter(&self) -> impl Iterator<Item = &ValidationError> {
        self.0.iter()
    }
}

impl fmt::Display for ValidationErrors {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for error in &self.0 {
            writeln!(
                formatter,
                "{}: {}: {}",
                error.code,
                error.path.display(),
                error.message
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationErrors {}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
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
    kind: ProvenanceKind,
    source: String,
    author: String,
    created_on: String,
    license: String,
    rights: String,
    generator: Option<Generator>,
    parents: Vec<Parent>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ProvenanceKind {
    Synthetic,
    Generated,
    Recorded,
    ThirdParty,
    Derived,
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

struct ParsedFixture {
    manifest_path: PathBuf,
    manifest_sha256: String,
    version_dir: PathBuf,
    manifest: Manifest,
}

/// Validates every fixture below `root` without fetching or executing anything.
pub fn validate_root(root: &Path) -> Result<ValidationReport, ValidationErrors> {
    let mut errors = Vec::new();
    if !root.is_dir() {
        push_error(
            &mut errors,
            "root.missing",
            root,
            "fixture root is not a directory",
        );
        return Err(ValidationErrors(errors));
    }

    let mut discovered_files = Vec::new();
    let mut manifests = Vec::new();
    walk(root, &mut discovered_files, &mut manifests, &mut errors);
    manifests.sort();

    let mut parsed = Vec::new();
    for manifest_path in manifests {
        if let Some(fixture) = parse_manifest(root, &manifest_path, &mut errors) {
            parsed.push(fixture);
        }
    }

    if parsed.is_empty() {
        push_error(
            &mut errors,
            "fixture.empty",
            root,
            "fixture root contains no fixture.json manifests",
        );
    }

    let mut handled = BTreeSet::new();
    let mut payload_count = 0;
    let mut identities = BTreeMap::new();
    for fixture in &parsed {
        handled.insert(fixture.manifest_path.clone());
        let key = (
            fixture.manifest.fixture_id.clone(),
            fixture.manifest.fixture_version,
        );
        if identities.insert(key, fixture).is_some() {
            push_error(
                &mut errors,
                "fixture.duplicate",
                &fixture.manifest_path,
                "fixture identity and version must be unique",
            );
        }
        validate_fixture(fixture, &mut handled, &mut payload_count, &mut errors);
    }

    validate_lineage(&parsed, &identities, &mut errors);
    validate_unmanaged(root, &discovered_files, &handled, &parsed, &mut errors);

    errors.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then_with(|| left.code.cmp(right.code))
            .then_with(|| left.message.cmp(&right.message))
    });
    if errors.is_empty() {
        Ok(ValidationReport {
            fixture_count: parsed.len(),
            payload_count,
        })
    } else {
        Err(ValidationErrors(errors))
    }
}

fn walk(
    directory: &Path,
    files: &mut Vec<PathBuf>,
    manifests: &mut Vec<PathBuf>,
    errors: &mut Vec<ValidationError>,
) {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            push_io(errors, "path.read", directory, &error);
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                push_io(errors, "path.read", directory, &error);
                continue;
            }
        };
        let path = entry.path();
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                push_io(errors, "path.metadata", &path, &error);
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            files.push(path);
        } else if metadata.is_dir() {
            walk(&path, files, manifests, errors);
        } else if metadata.is_file() {
            if path.file_name().is_some_and(|name| name == MANIFEST_NAME) {
                manifests.push(path.clone());
            }
            files.push(path);
        } else {
            push_error(
                errors,
                "path.type",
                &path,
                "only directories and regular files are allowed",
            );
        }
    }
}

fn parse_manifest(
    root: &Path,
    manifest_path: &Path,
    errors: &mut Vec<ValidationError>,
) -> Option<ParsedFixture> {
    let metadata = match fs::symlink_metadata(manifest_path) {
        Ok(metadata) => metadata,
        Err(error) => {
            push_io(errors, "manifest.metadata", manifest_path, &error);
            return None;
        }
    };
    if metadata.file_type().is_symlink() {
        push_error(
            errors,
            "manifest.symlink",
            manifest_path,
            "manifests must be regular files",
        );
        return None;
    }
    let bytes = match fs::read(manifest_path) {
        Ok(bytes) => bytes,
        Err(error) => {
            push_io(errors, "manifest.read", manifest_path, &error);
            return None;
        }
    };
    let manifest = match serde_json::from_slice::<Manifest>(&bytes) {
        Ok(manifest) => manifest,
        Err(error) => {
            push_error(
                errors,
                "manifest.json",
                manifest_path,
                format!("invalid manifest: {error}"),
            );
            return None;
        }
    };
    let version_dir = match manifest_path.parent() {
        Some(parent) => parent.to_path_buf(),
        None => {
            push_error(
                errors,
                "manifest.path",
                manifest_path,
                "manifest must have a version directory",
            );
            return None;
        }
    };

    validate_manifest_identity(root, &version_dir, &manifest, manifest_path, errors);
    validate_text_fields(&manifest, manifest_path, errors);
    validate_provenance(&manifest.provenance, manifest_path, errors);

    Some(ParsedFixture {
        manifest_path: manifest_path.to_path_buf(),
        manifest_sha256: digest_bytes(&bytes),
        version_dir,
        manifest,
    })
}

fn validate_manifest_identity(
    root: &Path,
    version_dir: &Path,
    manifest: &Manifest,
    manifest_path: &Path,
    errors: &mut Vec<ValidationError>,
) {
    if manifest.schema_version != SUPPORTED_SCHEMA_VERSION {
        push_error(
            errors,
            "manifest.schema",
            manifest_path,
            format!(
                "schema_version must be {SUPPORTED_SCHEMA_VERSION}, got {}",
                manifest.schema_version
            ),
        );
    }
    let Some(version_name) = version_dir.file_name().and_then(|name| name.to_str()) else {
        push_error(
            errors,
            "fixture.version",
            manifest_path,
            "version directory must be valid UTF-8",
        );
        return;
    };
    let expected_version_name = format!("v{}", manifest.fixture_version);
    if manifest.fixture_version == 0 || version_name != expected_version_name {
        push_error(
            errors,
            "fixture.version",
            manifest_path,
            format!(
                "fixture_version {} must match directory {version_name}",
                manifest.fixture_version
            ),
        );
    }

    let expected_id = version_dir
        .parent()
        .and_then(|parent| parent.strip_prefix(root).ok())
        .map(path_as_fixture_id);
    if expected_id.as_deref() != Some(manifest.fixture_id.as_str())
        || !valid_fixture_id(&manifest.fixture_id)
    {
        push_error(
            errors,
            "fixture.id",
            manifest_path,
            format!(
                "fixture_id {:?} must match its lowercase repository path {:?}",
                manifest.fixture_id, expected_id
            ),
        );
    }
}

fn validate_text_fields(
    manifest: &Manifest,
    manifest_path: &Path,
    errors: &mut Vec<ValidationError>,
) {
    require_text(
        errors,
        "fixture.description",
        manifest_path,
        "description",
        &manifest.description,
    );
    if manifest.files.is_empty() {
        push_error(
            errors,
            "payload.empty",
            manifest_path,
            "a fixture must inventory at least one payload",
        );
    }
}

fn validate_provenance(
    provenance: &Provenance,
    manifest_path: &Path,
    errors: &mut Vec<ValidationError>,
) {
    require_text(
        errors,
        "provenance.source",
        manifest_path,
        "source",
        &provenance.source,
    );
    require_text(
        errors,
        "provenance.author",
        manifest_path,
        "author",
        &provenance.author,
    );
    require_text(
        errors,
        "provenance.license",
        manifest_path,
        "license",
        &provenance.license,
    );
    require_text(
        errors,
        "provenance.rights",
        manifest_path,
        "rights",
        &provenance.rights,
    );
    if !valid_date(&provenance.created_on) {
        push_error(
            errors,
            "provenance.date",
            manifest_path,
            "created_on must be a real YYYY-MM-DD date",
        );
    }

    let needs_generator = matches!(
        provenance.kind,
        ProvenanceKind::Synthetic | ProvenanceKind::Generated | ProvenanceKind::Derived
    );
    if needs_generator && provenance.generator.is_none() {
        push_error(
            errors,
            "provenance.generator",
            manifest_path,
            "synthetic, generated, and derived fixtures require generator details",
        );
    }
    if let Some(generator) = &provenance.generator {
        for (field, value) in [
            ("name", generator.name.as_str()),
            ("version", generator.version.as_str()),
            ("command", generator.command.as_str()),
            ("seed", generator.seed.as_str()),
        ] {
            require_text(errors, "provenance.generator", manifest_path, field, value);
        }
    }
    if provenance.kind == ProvenanceKind::Derived && provenance.parents.is_empty() {
        push_error(
            errors,
            "provenance.parents",
            manifest_path,
            "derived fixtures require at least one parent manifest",
        );
    }
    if provenance.kind != ProvenanceKind::Derived && !provenance.parents.is_empty() {
        push_error(
            errors,
            "provenance.parents",
            manifest_path,
            "only derived fixtures may declare parents",
        );
    }
}

fn validate_fixture(
    fixture: &ParsedFixture,
    handled: &mut BTreeSet<PathBuf>,
    payload_count: &mut usize,
    errors: &mut Vec<ValidationError>,
) {
    let mut listed = BTreeSet::new();
    for payload in &fixture.manifest.files {
        if !valid_relative_path(&payload.path) || payload.path == MANIFEST_NAME {
            push_error(
                errors,
                "payload.path",
                &fixture.manifest_path,
                format!(
                    "payload path {:?} is not a safe normalized relative path",
                    payload.path
                ),
            );
            continue;
        }
        if !listed.insert(payload.path.clone()) {
            push_error(
                errors,
                "payload.duplicate",
                &fixture.manifest_path,
                format!("payload path {:?} is listed more than once", payload.path),
            );
            continue;
        }
        if payload.media_type.trim().is_empty() {
            push_error(
                errors,
                "payload.media_type",
                &fixture.manifest_path,
                format!("payload {:?} requires a media_type", payload.path),
            );
        }
        if !valid_sha256(&payload.sha256) {
            push_error(
                errors,
                "payload.sha256",
                &fixture.manifest_path,
                format!(
                    "payload {:?} requires a lowercase SHA-256 digest",
                    payload.path
                ),
            );
        }

        let path = fixture.version_dir.join(&payload.path);
        handled.insert(path.clone());
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) => {
                push_io(errors, "payload.missing", &path, &error);
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            push_error(
                errors,
                "payload.symlink",
                &path,
                "fixture payloads must be regular files, not symlinks",
            );
            continue;
        }
        if !metadata.is_file() {
            push_error(
                errors,
                "payload.type",
                &path,
                "fixture payload must be a regular file",
            );
            continue;
        }
        *payload_count += 1;
        if metadata.len() != payload.bytes {
            push_error(
                errors,
                "payload.size",
                &path,
                format!("expected {} bytes, found {}", payload.bytes, metadata.len()),
            );
        }
        match digest_file(&path) {
            Ok(actual) if actual == payload.sha256 => {}
            Ok(actual) => push_error(
                errors,
                "payload.sha256",
                &path,
                format!("expected {}, found {actual}", payload.sha256),
            ),
            Err(error) => push_io(errors, "payload.read", &path, &error),
        }
    }
}

fn validate_lineage(
    fixtures: &[ParsedFixture],
    identities: &BTreeMap<(String, u32), &ParsedFixture>,
    errors: &mut Vec<ValidationError>,
) {
    for fixture in fixtures {
        for parent in &fixture.manifest.provenance.parents {
            if !valid_fixture_id(&parent.fixture_id)
                || parent.fixture_version == 0
                || !valid_sha256(&parent.manifest_sha256)
            {
                push_error(
                    errors,
                    "provenance.parent",
                    &fixture.manifest_path,
                    "parent identity, version, and manifest_sha256 must be valid",
                );
                continue;
            }
            match identities.get(&(parent.fixture_id.clone(), parent.fixture_version)) {
                Some(referenced) if referenced.manifest_sha256 == parent.manifest_sha256 => {}
                Some(referenced) => push_error(
                    errors,
                    "provenance.parent_hash",
                    &fixture.manifest_path,
                    format!(
                        "parent {}/v{} manifest hash is {}, expected {}",
                        parent.fixture_id,
                        parent.fixture_version,
                        referenced.manifest_sha256,
                        parent.manifest_sha256
                    ),
                ),
                None => push_error(
                    errors,
                    "provenance.parent_missing",
                    &fixture.manifest_path,
                    format!(
                        "parent {}/v{} is not present in the fixture root",
                        parent.fixture_id, parent.fixture_version
                    ),
                ),
            }
        }
    }
}

fn validate_unmanaged(
    root: &Path,
    discovered: &[PathBuf],
    handled: &BTreeSet<PathBuf>,
    fixtures: &[ParsedFixture],
    errors: &mut Vec<ValidationError>,
) {
    let policy = root.join(POLICY_NAME);
    for path in discovered {
        if path == &policy || handled.contains(path) {
            continue;
        }
        let inside_version = fixtures
            .iter()
            .any(|fixture| path.starts_with(&fixture.version_dir));
        push_error(
            errors,
            if inside_version {
                "payload.unlisted"
            } else {
                "fixture.unmanaged"
            },
            path,
            if inside_version {
                "payload is not inventoried by fixture.json"
            } else {
                "files below the fixture root must belong to a versioned fixture"
            },
        );
    }
}

fn require_text(
    errors: &mut Vec<ValidationError>,
    code: &'static str,
    path: &Path,
    field: &str,
    value: &str,
) {
    if value.trim().is_empty() {
        push_error(errors, code, path, format!("{field} must not be empty"));
    }
}

fn valid_relative_path(value: &str) -> bool {
    if value.is_empty() || value.contains('\\') {
        return false;
    }
    let path = Path::new(value);
    if path.is_absolute()
        || !path
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
    {
        return false;
    }
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
        == value
}

fn valid_fixture_id(value: &str) -> bool {
    let mut component_count = 0;
    for component in value.split('/') {
        component_count += 1;
        let bytes = component.as_bytes();
        if bytes.is_empty()
            || !bytes[0].is_ascii_lowercase() && !bytes[0].is_ascii_digit()
            || !bytes[bytes.len() - 1].is_ascii_lowercase()
                && !bytes[bytes.len() - 1].is_ascii_digit()
            || !bytes.iter().all(|byte| {
                byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || matches!(byte, b'.' | b'_' | b'-')
            })
        {
            return false;
        }
    }
    component_count >= 2
}

fn path_as_fixture_id(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            Component::Normal(value) => value.to_str(),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();
    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
    {
        return false;
    }
    let year = value[0..4].parse::<u32>().ok();
    let month = value[5..7].parse::<u32>().ok();
    let day = value[8..10].parse::<u32>().ok();
    let (Some(year), Some(month), Some(day)) = (year, month, day) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let maximum = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=maximum).contains(&day)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn digest_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(format!("{:x}", digest.finalize()))
}

fn digest_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn push_io(errors: &mut Vec<ValidationError>, code: &'static str, path: &Path, error: &io::Error) {
    push_error(errors, code, path, error.to_string());
}

fn push_error(
    errors: &mut Vec<ValidationError>,
    code: &'static str,
    path: &Path,
    message: impl Into<String>,
) {
    errors.push(ValidationError {
        code,
        path: path.to_path_buf(),
        message: message.into(),
    });
}
