//! Deterministic, read-only golden verification for editor outputs.

use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

const GOLDEN_SCHEMA_VERSION: u32 = 1;

/// A stable validation, parsing, I/O, or comparison failure.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GoldenError {
    code: &'static str,
    path: Option<PathBuf>,
    message: String,
    expected_sha256: Option<String>,
    actual_sha256: Option<String>,
}

impl GoldenError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    #[must_use]
    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    #[must_use]
    pub fn expected_sha256(&self) -> Option<&str> {
        self.expected_sha256.as_deref()
    }

    #[must_use]
    pub fn actual_sha256(&self) -> Option<&str> {
        self.actual_sha256.as_deref()
    }
}

impl fmt::Display for GoldenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}", self.code)?;
        if let Some(path) = &self.path {
            write!(formatter, ": {}", path.display())?;
        }
        write!(formatter, ": {}", self.message)?;
        if let (Some(expected), Some(actual)) = (&self.expected_sha256, &self.actual_sha256) {
            write!(
                formatter,
                ": expected sha256 {expected}, actual sha256 {actual}"
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for GoldenError {}

/// Exact CPU-visible frame output and its interpretation.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FrameGolden {
    schema_version: u32,
    kind: GoldenKind,
    width: u32,
    height: u32,
    row_stride_bytes: u64,
    pixel_format: String,
    channel_names: Vec<String>,
    color_space: String,
    alpha_mode: String,
    payload: Vec<u8>,
}

impl FrameGolden {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        width: u32,
        height: u32,
        row_stride_bytes: u64,
        pixel_format: impl Into<String>,
        channel_names: Vec<impl Into<String>>,
        color_space: impl Into<String>,
        alpha_mode: impl Into<String>,
        payload: Vec<u8>,
    ) -> Result<Self, GoldenError> {
        let value = Self {
            schema_version: GOLDEN_SCHEMA_VERSION,
            kind: GoldenKind::Frame,
            width,
            height,
            row_stride_bytes,
            pixel_format: pixel_format.into(),
            channel_names: channel_names.into_iter().map(Into::into).collect(),
            color_space: color_space.into(),
            alpha_mode: alpha_mode.into(),
            payload,
        };
        value.validate()?;
        Ok(value)
    }

    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn to_golden_bytes(&self) -> Result<Vec<u8>, GoldenError> {
        self.validate()?;
        canonical_bytes(self)
    }

    fn validate(&self) -> Result<(), GoldenError> {
        validate_envelope(self.schema_version, self.kind, GoldenKind::Frame)?;
        if self.width == 0 || self.height == 0 {
            return Err(error(
                "frame.dimensions",
                "frame dimensions must be nonzero",
            ));
        }
        validate_labels("frame.channels", &self.channel_names)?;
        validate_text("frame.color_space", &self.color_space)?;
        validate_text("frame.alpha_mode", &self.alpha_mode)?;

        let bytes_per_pixel = match self.pixel_format.as_str() {
            "rgba8" | "bgra8" => 4_u64,
            "rgb16f-le" => 6,
            "rgba16f-le" => 8,
            "rgb32f-le" => 12,
            "rgba32f-le" => 16,
            _ => {
                return Err(error(
                    "frame.pixel_format",
                    "pixel format must be one of the stable packed golden formats",
                ));
            }
        };
        let minimum_stride = u64::from(self.width)
            .checked_mul(bytes_per_pixel)
            .ok_or_else(|| error("frame.row_stride", "frame row size overflows"))?;
        if self.row_stride_bytes < minimum_stride {
            return Err(error(
                "frame.row_stride",
                "row stride does not contain one complete packed row",
            ));
        }
        let expected = self
            .row_stride_bytes
            .checked_mul(u64::from(self.height))
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| error("frame.payload_length", "frame payload size overflows"))?;
        if self.payload.len() != expected {
            return Err(error(
                "frame.payload_length",
                format!(
                    "frame payload has {} bytes, expected {expected}",
                    self.payload.len()
                ),
            ));
        }
        Ok(())
    }
}

/// Exact audio output with sample-clock, channel, layout, and encoding metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AudioGolden {
    schema_version: u32,
    kind: GoldenKind,
    sample_rate: u32,
    start_sample: i64,
    frame_count: u64,
    channel_names: Vec<String>,
    sample_format: String,
    interleaved: bool,
    payload: Vec<u8>,
}

impl AudioGolden {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sample_rate: u32,
        start_sample: i64,
        frame_count: u64,
        channel_names: Vec<impl Into<String>>,
        sample_format: impl Into<String>,
        interleaved: bool,
        payload: Vec<u8>,
    ) -> Result<Self, GoldenError> {
        let value = Self {
            schema_version: GOLDEN_SCHEMA_VERSION,
            kind: GoldenKind::Audio,
            sample_rate,
            start_sample,
            frame_count,
            channel_names: channel_names.into_iter().map(Into::into).collect(),
            sample_format: sample_format.into(),
            interleaved,
            payload,
        };
        value.validate()?;
        Ok(value)
    }

    #[must_use]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub fn to_golden_bytes(&self) -> Result<Vec<u8>, GoldenError> {
        self.validate()?;
        canonical_bytes(self)
    }

    fn validate(&self) -> Result<(), GoldenError> {
        validate_envelope(self.schema_version, self.kind, GoldenKind::Audio)?;
        if self.sample_rate == 0 {
            return Err(error(
                "audio.sample_rate",
                "audio sample rate must be nonzero",
            ));
        }
        validate_labels("audio.channels", &self.channel_names)?;
        let sample_bytes = match self.sample_format.as_str() {
            "i16-le" => 2_u64,
            "i24-le" => 3,
            "i32-le" | "f32-le" => 4,
            "f64-le" => 8,
            _ => {
                return Err(error(
                    "audio.sample_format",
                    "audio sample format must be a stable little-endian golden format",
                ));
            }
        };
        let expected = self
            .frame_count
            .checked_mul(self.channel_names.len() as u64)
            .and_then(|value| value.checked_mul(sample_bytes))
            .and_then(|value| usize::try_from(value).ok())
            .ok_or_else(|| error("audio.payload_length", "audio payload size overflows"))?;
        if self.payload.len() != expected {
            return Err(error(
                "audio.payload_length",
                format!(
                    "audio payload has {} bytes, expected {expected}",
                    self.payload.len()
                ),
            ));
        }
        Ok(())
    }
}

/// Canonical timeline document output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimelineGolden(StructuredGolden);

impl TimelineGolden {
    pub fn new(schema: impl Into<String>, document: Value) -> Result<Self, GoldenError> {
        StructuredGolden::new(GoldenKind::Timeline, schema, document).map(Self)
    }

    pub fn to_golden_bytes(&self) -> Result<Vec<u8>, GoldenError> {
        self.0.to_golden_bytes(GoldenKind::Timeline)
    }
}

/// Canonical whole-project document output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectGolden(StructuredGolden);

impl ProjectGolden {
    pub fn new(schema: impl Into<String>, document: Value) -> Result<Self, GoldenError> {
        StructuredGolden::new(GoldenKind::Project, schema, document).map(Self)
    }

    pub fn to_golden_bytes(&self) -> Result<Vec<u8>, GoldenError> {
        self.0.to_golden_bytes(GoldenKind::Project)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StructuredGolden {
    schema_version: u32,
    kind: GoldenKind,
    document_schema: String,
    document: Value,
}

impl StructuredGolden {
    fn new(
        kind: GoldenKind,
        document_schema: impl Into<String>,
        document: Value,
    ) -> Result<Self, GoldenError> {
        let value = Self {
            schema_version: GOLDEN_SCHEMA_VERSION,
            kind,
            document_schema: document_schema.into(),
            document: sort_json(document),
        };
        value.validate(kind)?;
        Ok(value)
    }

    fn validate(&self, expected_kind: GoldenKind) -> Result<(), GoldenError> {
        validate_envelope(self.schema_version, self.kind, expected_kind)?;
        validate_text("structured.document_schema", &self.document_schema)?;
        if !self.document.is_object() {
            return Err(error(
                "structured.document",
                "timeline and project golden documents must be JSON objects",
            ));
        }
        Ok(())
    }

    fn to_golden_bytes(&self, expected_kind: GoldenKind) -> Result<Vec<u8>, GoldenError> {
        self.validate(expected_kind)?;
        let mut canonical = self.clone();
        canonical.document = sort_json(canonical.document);
        canonical_bytes(&canonical)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum GoldenKind {
    Frame,
    Audio,
    Timeline,
    Project,
}

/// Verifies an exact frame against an existing golden file without modifying it.
pub fn verify_frame(path: &Path, actual: &FrameGolden) -> Result<(), GoldenError> {
    actual.validate()?;
    let expected: FrameGolden = read_golden(path)?;
    expected.validate()?;
    compare(path, expected.to_golden_bytes()?, actual.to_golden_bytes()?)
}

/// Verifies exact audio samples and sample-clock metadata without modifying the golden.
pub fn verify_audio(path: &Path, actual: &AudioGolden) -> Result<(), GoldenError> {
    actual.validate()?;
    let expected: AudioGolden = read_golden(path)?;
    expected.validate()?;
    compare(path, expected.to_golden_bytes()?, actual.to_golden_bytes()?)
}

/// Verifies canonical timeline JSON without modifying the golden.
pub fn verify_timeline(path: &Path, actual: &TimelineGolden) -> Result<(), GoldenError> {
    let expected: TimelineGolden = read_golden(path)?;
    compare(path, expected.to_golden_bytes()?, actual.to_golden_bytes()?)
}

/// Verifies canonical project JSON without modifying the golden.
pub fn verify_project(path: &Path, actual: &ProjectGolden) -> Result<(), GoldenError> {
    let expected: ProjectGolden = read_golden(path)?;
    compare(path, expected.to_golden_bytes()?, actual.to_golden_bytes()?)
}

fn read_golden<T>(path: &Path) -> Result<T, GoldenError>
where
    T: for<'de> Deserialize<'de>,
{
    let bytes = fs::read(path).map_err(|source| GoldenError {
        code: "golden.read",
        path: Some(path.to_path_buf()),
        message: source.to_string(),
        expected_sha256: None,
        actual_sha256: None,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| GoldenError {
        code: "golden.parse",
        path: Some(path.to_path_buf()),
        message: source.to_string(),
        expected_sha256: None,
        actual_sha256: None,
    })
}

fn compare(path: &Path, expected: Vec<u8>, actual: Vec<u8>) -> Result<(), GoldenError> {
    if expected == actual {
        return Ok(());
    }
    let first_difference = expected
        .iter()
        .zip(&actual)
        .position(|(left, right)| left != right)
        .unwrap_or_else(|| expected.len().min(actual.len()));
    Err(GoldenError {
        code: "golden.mismatch",
        path: Some(path.to_path_buf()),
        message: format!(
            "canonical output differs at byte {first_difference}, expected {} bytes, actual {} bytes",
            expected.len(),
            actual.len()
        ),
        expected_sha256: Some(digest(&expected)),
        actual_sha256: Some(digest(&actual)),
    })
}

fn canonical_bytes(value: &impl Serialize) -> Result<Vec<u8>, GoldenError> {
    let value = serde_json::to_value(value)
        .map(sort_json)
        .map_err(|source| error("golden.serialize", source.to_string()))?;
    let mut bytes = serde_json::to_vec_pretty(&value)
        .map_err(|source| error("golden.serialize", source.to_string()))?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn sort_json(value: Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.into_iter().map(sort_json).collect()),
        Value::Object(values) => {
            let sorted = values
                .into_iter()
                .map(|(key, value)| (key, sort_json(value)))
                .collect::<BTreeMap<_, _>>();
            Value::Object(sorted.into_iter().collect())
        }
        scalar => scalar,
    }
}

fn validate_envelope(
    schema_version: u32,
    kind: GoldenKind,
    expected_kind: GoldenKind,
) -> Result<(), GoldenError> {
    if schema_version != GOLDEN_SCHEMA_VERSION {
        return Err(error(
            "golden.schema",
            format!("golden schema must be {GOLDEN_SCHEMA_VERSION}, got {schema_version}"),
        ));
    }
    if kind != expected_kind {
        return Err(error("golden.kind", "golden kind does not match harness"));
    }
    Ok(())
}

fn validate_labels(code: &'static str, labels: &[String]) -> Result<(), GoldenError> {
    if labels.is_empty() || labels.iter().any(|label| label.trim().is_empty()) {
        return Err(error(code, "semantic channel labels must be nonempty"));
    }
    let unique = labels.iter().collect::<std::collections::BTreeSet<_>>();
    if unique.len() != labels.len() {
        return Err(error(code, "semantic channel labels must be unique"));
    }
    Ok(())
}

fn validate_text(code: &'static str, value: &str) -> Result<(), GoldenError> {
    if value.trim().is_empty() {
        return Err(error(code, "value must be nonempty"));
    }
    Ok(())
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn error(code: &'static str, message: impl Into<String>) -> GoldenError {
    GoldenError {
        code,
        path: None,
        message: message.into(),
        expected_sha256: None,
        actual_sha256: None,
    }
}
