//! Stable serialization contracts for shared public primitives.
//!
//! Wire representations are part of Superi's public compatibility surface. Unit
//! tags use permanent lowercase codes, identifiers use their canonical text,
//! composite values use explicit snake-case fields, and wide integers use
//! canonical decimal strings so JSON and JavaScript clients retain exact values.
//! Deserialization routes through checked domain constructors and rejects unknown
//! object fields instead of silently creating or truncating state.
//!
//! Project, API, graph, and interchange formats own their containing schema
//! versions. They must record or negotiate [`STABLE_PRIMITIVE_SCHEMA_REVISION`]
//! before decoding values from this module's contract.

use std::collections::BTreeMap;
use std::str::FromStr;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use crate::diagnostics::{
    CounterSnapshot, CounterUnit, DiagnosticEvent, DiagnosticSeverity, FailureDiagnostic,
    FieldVisibility, FiniteF64, PerformanceCounter, TraceField, TraceValue, UserSafeError,
};
use crate::error::{ErrorCategory, ErrorContext, Recoverability};
use crate::geometry::{AspectRatio, Matrix3, PixelBounds, Point2, Rect, Vector2};
use crate::ids::{
    CacheId, ClipId, DeviceId, EdgeId, GraphId, IdentifierKind, JobId, MediaId, NodeId,
    ParameterId, PortId, ProjectId, ResourceId, TrackId,
};
use crate::pixel::{
    AlphaMode, ChannelLayout, ChannelPosition, ChromaSubsampling, PixelFormat, PixelModel,
    PixelNumeric, PixelPacking, SampleFormat, SampleNumeric,
};
use crate::settings::{
    CapabilityId, CapabilitySet, ComponentId, FeatureAvailability, FeatureDescriptor,
    FeatureDiscovery, FeatureId, SemanticVersion, SettingKey, SettingValue, SettingValueKind,
    SettingsSnapshot, VersionIdentifier, VersionSection,
};
use crate::time::{
    Duration, FrameRate, RationalTime, SampleTime, TimeRange, TimeRounding, Timebase,
};
use crate::timecode::{Timecode, TimecodeComponent, TimecodeFormat, TimecodeMode};

/// Current incompatible revision of the shared primitive wire contract.
///
/// Existing codes, fields, and meanings are immutable within one revision. A
/// containing project or protocol format must migrate or negotiate this value
/// before decoding a different revision.
pub const STABLE_PRIMITIVE_SCHEMA_REVISION: u32 = 1;

macro_rules! impl_string_enum {
    (
        $type:ty,
        $expecting:literal,
        { $($variant:path => $code:literal),+ $(,)? }
    ) => {
        impl Serialize for $type {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: Serializer,
            {
                let code = match self {
                    $($variant => $code,)+
                };
                serializer.serialize_str(code)
            }
        }

        impl<'de> Deserialize<'de> for $type {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: Deserializer<'de>,
            {
                let code = String::deserialize(deserializer)?;
                match code.as_str() {
                    $($code => Ok($variant),)+
                    _ => Err(D::Error::unknown_variant(&code, &[$($code),+])),
                }
            }
        }
    };
}

impl_string_enum!(
    ErrorCategory,
    "a stable error category code",
    {
        ErrorCategory::InvalidInput => "invalid_input",
        ErrorCategory::NotFound => "not_found",
        ErrorCategory::Conflict => "conflict",
        ErrorCategory::Unsupported => "unsupported",
        ErrorCategory::PermissionDenied => "permission_denied",
        ErrorCategory::ResourceExhausted => "resource_exhausted",
        ErrorCategory::Unavailable => "unavailable",
        ErrorCategory::Timeout => "timeout",
        ErrorCategory::Cancelled => "cancelled",
        ErrorCategory::CorruptData => "corrupt_data",
        ErrorCategory::Internal => "internal",
    }
);

impl_string_enum!(
    DiagnosticSeverity,
    "a stable diagnostic severity code",
    {
        DiagnosticSeverity::Trace => "trace",
        DiagnosticSeverity::Debug => "debug",
        DiagnosticSeverity::Info => "info",
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Error => "error",
    }
);

impl_string_enum!(
    FieldVisibility,
    "a stable field visibility code",
    {
        FieldVisibility::UserSafe => "user_safe",
        FieldVisibility::Internal => "internal",
        FieldVisibility::Sensitive => "sensitive",
    }
);

impl_string_enum!(
    CounterUnit,
    "a stable counter unit code",
    {
        CounterUnit::Count => "count",
        CounterUnit::Bytes => "bytes",
        CounterUnit::Nanoseconds => "nanoseconds",
        CounterUnit::Frames => "frames",
        CounterUnit::Samples => "samples",
    }
);

impl_string_enum!(
    Recoverability,
    "a stable recoverability code",
    {
        Recoverability::Retryable => "retryable",
        Recoverability::Degraded => "degraded",
        Recoverability::UserCorrectable => "user_correctable",
        Recoverability::Terminal => "terminal",
    }
);

impl_string_enum!(
    VersionSection,
    "a stable semantic version section code",
    {
        VersionSection::Major => "major",
        VersionSection::Minor => "minor",
        VersionSection::Patch => "patch",
        VersionSection::PreRelease => "pre_release",
        VersionSection::BuildMetadata => "build_metadata",
    }
);

impl_string_enum!(
    SettingValueKind,
    "a stable setting value kind code",
    {
        SettingValueKind::Boolean => "boolean",
        SettingValueKind::Integer => "integer",
        SettingValueKind::Text => "text",
    }
);

impl_string_enum!(
    FeatureAvailability,
    "a stable feature availability code",
    {
        FeatureAvailability::Available => "available",
        FeatureAvailability::Disabled => "disabled",
        FeatureAvailability::Unsupported => "unsupported",
        FeatureAvailability::Unavailable => "unavailable",
    }
);

impl_string_enum!(
    IdentifierKind,
    "a stable identifier kind code",
    {
        IdentifierKind::Project => "project",
        IdentifierKind::Media => "media",
        IdentifierKind::Track => "track",
        IdentifierKind::Clip => "clip",
        IdentifierKind::Node => "node",
        IdentifierKind::Parameter => "parameter",
        IdentifierKind::Job => "job",
        IdentifierKind::Cache => "cache",
        IdentifierKind::Device => "device",
        IdentifierKind::Graph => "graph",
        IdentifierKind::Port => "port",
        IdentifierKind::Edge => "edge",
        IdentifierKind::Resource => "resource",
    }
);

impl_string_enum!(
    TimeRounding,
    "a stable time rounding code",
    {
        TimeRounding::Exact => "exact",
        TimeRounding::Floor => "floor",
        TimeRounding::Ceil => "ceil",
        TimeRounding::TowardZero => "toward_zero",
        TimeRounding::NearestTiesEven => "nearest_ties_even",
    }
);

impl_string_enum!(
    TimecodeMode,
    "a stable timecode mode code",
    {
        TimecodeMode::NonDropFrame => "non_drop_frame",
        TimecodeMode::DropFrame => "drop_frame",
    }
);

impl_string_enum!(
    TimecodeComponent,
    "a stable timecode component code",
    {
        TimecodeComponent::Hours => "hours",
        TimecodeComponent::Minutes => "minutes",
        TimecodeComponent::Seconds => "seconds",
        TimecodeComponent::Frames => "frames",
    }
);

impl_string_enum!(
    PixelModel,
    "a stable pixel model code",
    {
        PixelModel::Gray => "gray",
        PixelModel::Rg => "rg",
        PixelModel::Rgb => "rgb",
        PixelModel::Bgr => "bgr",
        PixelModel::Rgba => "rgba",
        PixelModel::Bgra => "bgra",
        PixelModel::Yuv => "yuv",
    }
);

impl_string_enum!(
    PixelNumeric,
    "a stable pixel numeric code",
    {
        PixelNumeric::Unorm => "unorm",
        PixelNumeric::Float => "float",
    }
);

impl_string_enum!(
    PixelPacking,
    "a stable pixel packing code",
    {
        PixelPacking::Packed => "packed",
        PixelPacking::Planar => "planar",
        PixelPacking::Semiplanar => "semiplanar",
    }
);

impl_string_enum!(
    ChromaSubsampling,
    "a stable chroma subsampling code",
    {
        ChromaSubsampling::Cs444 => "4:4:4",
        ChromaSubsampling::Cs422 => "4:2:2",
        ChromaSubsampling::Cs420 => "4:2:0",
    }
);

impl_string_enum!(
    PixelFormat,
    "a stable pixel format code",
    {
        PixelFormat::R8Unorm => "r8_unorm",
        PixelFormat::R16Unorm => "r16_unorm",
        PixelFormat::R16Float => "r16_float",
        PixelFormat::R32Float => "r32_float",
        PixelFormat::Rg8Unorm => "rg8_unorm",
        PixelFormat::Rg16Unorm => "rg16_unorm",
        PixelFormat::Rg16Float => "rg16_float",
        PixelFormat::Rg32Float => "rg32_float",
        PixelFormat::Rgb8Unorm => "rgb8_unorm",
        PixelFormat::Bgr8Unorm => "bgr8_unorm",
        PixelFormat::Rgba8Unorm => "rgba8_unorm",
        PixelFormat::Bgra8Unorm => "bgra8_unorm",
        PixelFormat::Rgba16Unorm => "rgba16_unorm",
        PixelFormat::Rgba16Float => "rgba16_float",
        PixelFormat::Rgba32Float => "rgba32_float",
        PixelFormat::Yuv420p8 => "yuv420p8",
        PixelFormat::Yuv420p10 => "yuv420p10",
        PixelFormat::Yuv422p8 => "yuv422p8",
        PixelFormat::Yuv422p10 => "yuv422p10",
        PixelFormat::Yuv444p8 => "yuv444p8",
        PixelFormat::Yuv444p10 => "yuv444p10",
        PixelFormat::Nv12 => "nv12",
        PixelFormat::P010 => "p010",
    }
);

impl_string_enum!(
    AlphaMode,
    "a stable alpha mode code",
    {
        AlphaMode::Opaque => "opaque",
        AlphaMode::Straight => "straight",
        AlphaMode::Premultiplied => "premultiplied",
    }
);

impl_string_enum!(
    SampleNumeric,
    "a stable sample numeric code",
    {
        SampleNumeric::UnsignedInteger => "unsigned_integer",
        SampleNumeric::SignedInteger => "signed_integer",
        SampleNumeric::Float => "float",
    }
);

impl_string_enum!(
    SampleFormat,
    "a stable sample format code",
    {
        SampleFormat::U8 => "u8",
        SampleFormat::U8Planar => "u8_planar",
        SampleFormat::I16 => "i16",
        SampleFormat::I16Planar => "i16_planar",
        SampleFormat::I24 => "i24",
        SampleFormat::I24Planar => "i24_planar",
        SampleFormat::I32 => "i32",
        SampleFormat::I32Planar => "i32_planar",
        SampleFormat::F32 => "f32",
        SampleFormat::F32Planar => "f32_planar",
        SampleFormat::F64 => "f64",
        SampleFormat::F64Planar => "f64_planar",
    }
);

impl_string_enum!(
    ColorPrimaries,
    "a stable color primaries code",
    {
        ColorPrimaries::Unspecified => "unspecified",
        ColorPrimaries::Bt709 => "bt709",
        ColorPrimaries::Bt2020 => "bt2020",
        ColorPrimaries::DisplayP3 => "display_p3",
        ColorPrimaries::AcesAp0 => "aces_ap0",
        ColorPrimaries::AcesAp1 => "aces_ap1",
    }
);

impl_string_enum!(
    TransferFunction,
    "a stable transfer function code",
    {
        TransferFunction::Unspecified => "unspecified",
        TransferFunction::Linear => "linear",
        TransferFunction::Srgb => "srgb",
        TransferFunction::Bt709 => "bt709",
        TransferFunction::Bt2020TenBit => "bt2020_10bit",
        TransferFunction::Bt2020TwelveBit => "bt2020_12bit",
        TransferFunction::Gamma22 => "gamma22",
        TransferFunction::Gamma24 => "gamma24",
        TransferFunction::Pq => "pq",
        TransferFunction::Hlg => "hlg",
    }
);

impl_string_enum!(
    MatrixCoefficients,
    "a stable matrix coefficients code",
    {
        MatrixCoefficients::Unspecified => "unspecified",
        MatrixCoefficients::Rgb => "rgb",
        MatrixCoefficients::Bt601 => "bt601",
        MatrixCoefficients::Bt709 => "bt709",
        MatrixCoefficients::Bt2020NonConstant => "bt2020_non_constant",
        MatrixCoefficients::Bt2020Constant => "bt2020_constant",
    }
);

impl_string_enum!(
    ColorRange,
    "a stable color range code",
    {
        ColorRange::Unspecified => "unspecified",
        ColorRange::Full => "full",
        ColorRange::Limited => "limited",
    }
);

macro_rules! impl_typed_id {
    ($($type:ty),+ $(,)?) => {
        $(
            impl Serialize for $type {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: Serializer,
                {
                    serializer.collect_str(self)
                }
            }

            impl<'de> Deserialize<'de> for $type {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: Deserializer<'de>,
                {
                    let text = String::deserialize(deserializer)?;
                    Self::from_str(&text).map_err(D::Error::custom)
                }
            }
        )+
    };
}

impl_typed_id!(
    ProjectId,
    MediaId,
    TrackId,
    ClipId,
    NodeId,
    ParameterId,
    JobId,
    CacheId,
    DeviceId,
    GraphId,
    PortId,
    EdgeId,
    ResourceId,
);

macro_rules! impl_canonical_text {
    ($($type:ty),+ $(,)?) => {
        $(
            impl Serialize for $type {
                fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
                where
                    S: Serializer,
                {
                    serializer.collect_str(self)
                }
            }

            impl<'de> Deserialize<'de> for $type {
                fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
                where
                    D: Deserializer<'de>,
                {
                    let text = String::deserialize(deserializer)?;
                    Self::from_str(&text).map_err(D::Error::custom)
                }
            }
        )+
    };
}

impl_canonical_text!(
    SettingKey,
    CapabilityId,
    FeatureId,
    ComponentId,
    SemanticVersion,
    VersionIdentifier,
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CanonicalI64(i64);

impl Serialize for CanonicalI64 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CanonicalI64 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        parse_canonical_i64(&text)
            .map(Self)
            .map_err(D::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct CanonicalU64(u64);

impl Serialize for CanonicalU64 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.collect_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for CanonicalU64 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let text = String::deserialize(deserializer)?;
        parse_canonical_u64(&text)
            .map(Self)
            .map_err(D::Error::custom)
    }
}

fn parse_canonical_i64(text: &str) -> Result<i64, &'static str> {
    let digits = if text == "0" {
        return Ok(0);
    } else if let Some(digits) = text.strip_prefix('-') {
        if digits == "0" {
            return Err("negative zero is not a canonical signed integer");
        }
        digits
    } else {
        text
    };

    if digits.is_empty()
        || digits.starts_with('0')
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err("expected a canonical signed decimal integer string");
    }
    text.parse()
        .map_err(|_| "signed decimal integer is outside the i64 range")
}

fn parse_canonical_u64(text: &str) -> Result<u64, &'static str> {
    if text == "0" {
        return Ok(0);
    }
    if text.is_empty() || text.starts_with('0') || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("expected a canonical unsigned decimal integer string");
    }
    text.parse()
        .map_err(|_| "unsigned decimal integer is outside the u64 range")
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ErrorContextWire {
    component: String,
    operation: String,
    fields: BTreeMap<String, String>,
}

impl Serialize for ErrorContext {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ErrorContextWire {
            component: self.component().to_owned(),
            operation: self.operation().to_owned(),
            fields: self.fields().clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ErrorContext {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ErrorContextWire::deserialize(deserializer)?;
        let mut context = Self::new(wire.component, wire.operation);
        for (key, value) in wire.fields {
            context.insert_field(key, value);
        }
        Ok(context)
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RateWire {
    numerator: u32,
    denominator: u32,
}

impl Serialize for Timebase {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        RateWire {
            numerator: self.numerator(),
            denominator: self.denominator(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Timebase {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RateWire::deserialize(deserializer)?;
        let value = Self::new(wire.numerator, wire.denominator).map_err(D::Error::custom)?;
        if value.numerator() != wire.numerator || value.denominator() != wire.denominator {
            return Err(D::Error::custom("timebase terms must already be reduced"));
        }
        Ok(value)
    }
}

impl Serialize for FrameRate {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        RateWire {
            numerator: self.numerator(),
            denominator: self.denominator(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FrameRate {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RateWire::deserialize(deserializer)?;
        let value = Self::new(wire.numerator, wire.denominator).map_err(D::Error::custom)?;
        if value.numerator() != wire.numerator || value.denominator() != wire.denominator {
            return Err(D::Error::custom("frame-rate terms must already be reduced"));
        }
        Ok(value)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RationalTimeWire {
    value: CanonicalI64,
    timebase: Timebase,
}

impl Serialize for RationalTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        RationalTimeWire {
            value: CanonicalI64(self.value()),
            timebase: self.timebase(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RationalTime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RationalTimeWire::deserialize(deserializer)?;
        Ok(Self::new(wire.value.0, wire.timebase))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SampleTimeWire {
    sample: CanonicalI64,
    sample_rate: u32,
}

impl Serialize for SampleTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        SampleTimeWire {
            sample: CanonicalI64(self.sample()),
            sample_rate: self.sample_rate(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SampleTime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = SampleTimeWire::deserialize(deserializer)?;
        Self::new(wire.sample.0, wire.sample_rate).map_err(D::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DurationWire {
    value: CanonicalU64,
    timebase: Timebase,
}

impl Serialize for Duration {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        DurationWire {
            value: CanonicalU64(self.value()),
            timebase: self.timebase(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Duration {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = DurationWire::deserialize(deserializer)?;
        Self::new(wire.value.0, wire.timebase).map_err(D::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TimeRangeWire {
    start: RationalTime,
    duration: Duration,
}

impl Serialize for TimeRange {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        TimeRangeWire {
            start: self.start(),
            duration: self.duration(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TimeRange {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = TimeRangeWire::deserialize(deserializer)?;
        Self::new(wire.start, wire.duration).map_err(D::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TimecodeFormatWire {
    rate: FrameRate,
    mode: TimecodeMode,
}

impl Serialize for TimecodeFormat {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        TimecodeFormatWire {
            rate: self.rate(),
            mode: self.mode(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TimecodeFormat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = TimecodeFormatWire::deserialize(deserializer)?;
        match wire.mode {
            TimecodeMode::NonDropFrame => Ok(Self::non_drop(wire.rate)),
            TimecodeMode::DropFrame => Self::drop_frame(wire.rate).map_err(D::Error::custom),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TimecodeWire {
    frames: CanonicalI64,
    format: TimecodeFormat,
}

impl Serialize for Timecode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        TimecodeWire {
            frames: CanonicalI64(self.frames()),
            format: self.format(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Timecode {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = TimecodeWire::deserialize(deserializer)?;
        Ok(Self::from_frames(wire.frames.0, wire.format))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PairWire {
    x: f64,
    y: f64,
}

impl Serialize for Point2 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        PairWire {
            x: self.x(),
            y: self.y(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Point2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = PairWire::deserialize(deserializer)?;
        Self::new(wire.x, wire.y).map_err(D::Error::custom)
    }
}

impl Serialize for Vector2 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        PairWire {
            x: self.x(),
            y: self.y(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Vector2 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = PairWire::deserialize(deserializer)?;
        Self::new(wire.x, wire.y).map_err(D::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Matrix3Wire {
    rows: [[f64; 3]; 3],
}

impl Serialize for Matrix3 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        Matrix3Wire { rows: self.rows() }.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Matrix3 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = Matrix3Wire::deserialize(deserializer)?;
        Self::from_rows(wire.rows).map_err(D::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct RectWire {
    min: Point2,
    max: Point2,
}

impl Serialize for Rect {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        RectWire {
            min: self.min(),
            max: self.max(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Rect {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RectWire::deserialize(deserializer)?;
        Self::new(wire.min, wire.max).map_err(D::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AspectRatioWire {
    numerator: u32,
    denominator: u32,
}

impl Serialize for AspectRatio {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        AspectRatioWire {
            numerator: self.numerator(),
            denominator: self.denominator(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AspectRatio {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = AspectRatioWire::deserialize(deserializer)?;
        let value = Self::new(wire.numerator, wire.denominator).map_err(D::Error::custom)?;
        if value.numerator() != wire.numerator || value.denominator() != wire.denominator {
            return Err(D::Error::custom(
                "aspect-ratio terms must already be reduced",
            ));
        }
        Ok(value)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PixelBoundsWire {
    min_x: i32,
    min_y: i32,
    max_x: i32,
    max_y: i32,
}

impl Serialize for PixelBounds {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        PixelBoundsWire {
            min_x: self.min_x(),
            min_y: self.min_y(),
            max_x: self.max_x(),
            max_y: self.max_y(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PixelBounds {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = PixelBoundsWire::deserialize(deserializer)?;
        Self::new(wire.min_x, wire.min_y, wire.max_x, wire.max_y).map_err(D::Error::custom)
    }
}

impl Serialize for ChannelPosition {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let code = match self {
            Self::FrontLeft => "front_left",
            Self::FrontRight => "front_right",
            Self::FrontCenter => "front_center",
            Self::LowFrequency => "low_frequency",
            Self::BackLeft => "back_left",
            Self::BackRight => "back_right",
            Self::FrontLeftOfCenter => "front_left_of_center",
            Self::FrontRightOfCenter => "front_right_of_center",
            Self::BackCenter => "back_center",
            Self::SideLeft => "side_left",
            Self::SideRight => "side_right",
            Self::TopCenter => "top_center",
            Self::TopFrontLeft => "top_front_left",
            Self::TopFrontCenter => "top_front_center",
            Self::TopFrontRight => "top_front_right",
            Self::TopBackLeft => "top_back_left",
            Self::TopBackCenter => "top_back_center",
            Self::TopBackRight => "top_back_right",
            Self::Discrete(index) => return serializer.serialize_str(&format!("discrete:{index}")),
        };
        serializer.serialize_str(code)
    }
}

impl<'de> Deserialize<'de> for ChannelPosition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let code = String::deserialize(deserializer)?;
        let position = match code.as_str() {
            "front_left" => Self::FrontLeft,
            "front_right" => Self::FrontRight,
            "front_center" => Self::FrontCenter,
            "low_frequency" => Self::LowFrequency,
            "back_left" => Self::BackLeft,
            "back_right" => Self::BackRight,
            "front_left_of_center" => Self::FrontLeftOfCenter,
            "front_right_of_center" => Self::FrontRightOfCenter,
            "back_center" => Self::BackCenter,
            "side_left" => Self::SideLeft,
            "side_right" => Self::SideRight,
            "top_center" => Self::TopCenter,
            "top_front_left" => Self::TopFrontLeft,
            "top_front_center" => Self::TopFrontCenter,
            "top_front_right" => Self::TopFrontRight,
            "top_back_left" => Self::TopBackLeft,
            "top_back_center" => Self::TopBackCenter,
            "top_back_right" => Self::TopBackRight,
            _ => {
                let index = code
                    .strip_prefix("discrete:")
                    .ok_or_else(|| D::Error::custom("unknown channel position code"))?;
                let index = parse_canonical_u64(index).map_err(D::Error::custom)?;
                Self::Discrete(
                    u16::try_from(index)
                        .map_err(|_| D::Error::custom("discrete channel index exceeds u16"))?,
                )
            }
        };
        Ok(position)
    }
}

impl Serialize for ChannelLayout {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.positions().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ChannelLayout {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let positions = Vec::<ChannelPosition>::deserialize(deserializer)?;
        Self::new(positions).map_err(D::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ColorSpaceWire {
    primaries: ColorPrimaries,
    transfer: TransferFunction,
    matrix: MatrixCoefficients,
    range: ColorRange,
}

impl Serialize for ColorSpace {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        ColorSpaceWire {
            primaries: self.primaries(),
            transfer: self.transfer(),
            matrix: self.matrix(),
            range: self.range(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ColorSpace {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = ColorSpaceWire::deserialize(deserializer)?;
        Ok(Self::new(
            wire.primaries,
            wire.transfer,
            wire.matrix,
            wire.range,
        ))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", deny_unknown_fields)]
enum SettingValueWire {
    #[serde(rename = "boolean")]
    Boolean(bool),
    #[serde(rename = "integer")]
    Integer(CanonicalI64),
    #[serde(rename = "text")]
    Text(String),
}

impl Serialize for SettingValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let wire = match self {
            Self::Boolean(value) => SettingValueWire::Boolean(*value),
            Self::Integer(value) => SettingValueWire::Integer(CanonicalI64(*value)),
            Self::Text(value) => SettingValueWire::Text(value.clone()),
        };
        wire.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SettingValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match SettingValueWire::deserialize(deserializer)? {
            SettingValueWire::Boolean(value) => Ok(Self::Boolean(value)),
            SettingValueWire::Integer(value) => Ok(Self::Integer(value.0)),
            SettingValueWire::Text(value) => Ok(Self::Text(value)),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingEntryWire {
    key: SettingKey,
    value: SettingValue,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SettingsSnapshotWire {
    schema_version: SemanticVersion,
    values: Vec<SettingEntryWire>,
}

impl Serialize for SettingsSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        SettingsSnapshotWire {
            schema_version: self.schema_version().clone(),
            values: self
                .iter()
                .map(|(key, value)| SettingEntryWire {
                    key: key.clone(),
                    value: value.clone(),
                })
                .collect(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for SettingsSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = SettingsSnapshotWire::deserialize(deserializer)?;
        Self::new(
            wire.schema_version,
            wire.values
                .into_iter()
                .map(|entry| (entry.key, entry.value)),
        )
        .map_err(D::Error::custom)
    }
}

impl Serialize for CapabilitySet {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.iter().collect::<Vec<_>>().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CapabilitySet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Ok(Self::new(Vec::<CapabilityId>::deserialize(deserializer)?))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FeatureDescriptorWire {
    id: FeatureId,
    version: SemanticVersion,
    availability: FeatureAvailability,
    required_capabilities: CapabilitySet,
}

impl Serialize for FeatureDescriptor {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        FeatureDescriptorWire {
            id: self.id().clone(),
            version: self.version().clone(),
            availability: self.availability(),
            required_capabilities: self.required_capabilities().clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FeatureDescriptor {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = FeatureDescriptorWire::deserialize(deserializer)?;
        Ok(Self::new(
            wire.id,
            wire.version,
            wire.availability,
            wire.required_capabilities,
        ))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FeatureDiscoveryWire {
    schema_version: SemanticVersion,
    producer: VersionIdentifier,
    capabilities: CapabilitySet,
    features: Vec<FeatureDescriptor>,
}

impl Serialize for FeatureDiscovery {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        FeatureDiscoveryWire {
            schema_version: self.schema_version().clone(),
            producer: self.producer().clone(),
            capabilities: self.capabilities().clone(),
            features: self.iter().cloned().collect(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FeatureDiscovery {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = FeatureDiscoveryWire::deserialize(deserializer)?;
        Self::new(
            wire.schema_version,
            wire.producer,
            wire.capabilities,
            wire.features,
        )
        .map_err(D::Error::custom)
    }
}

impl Serialize for FiniteF64 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_f64(self.get())
    }
}

impl<'de> Deserialize<'de> for FiniteF64 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(f64::deserialize(deserializer)?).map_err(D::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", deny_unknown_fields)]
enum TraceValueWire {
    #[serde(rename = "bool")]
    Boolean(bool),
    #[serde(rename = "i64")]
    Signed(CanonicalI64),
    #[serde(rename = "u64")]
    Unsigned(CanonicalU64),
    #[serde(rename = "f64")]
    Float(FiniteF64),
    #[serde(rename = "text")]
    Text(String),
}

impl Serialize for TraceValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let wire = match self {
            Self::Boolean(value) => TraceValueWire::Boolean(*value),
            Self::Signed(value) => TraceValueWire::Signed(CanonicalI64(*value)),
            Self::Unsigned(value) => TraceValueWire::Unsigned(CanonicalU64(*value)),
            Self::Float(value) => TraceValueWire::Float(*value),
            Self::Text(value) => TraceValueWire::Text(value.clone()),
        };
        wire.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TraceValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        match TraceValueWire::deserialize(deserializer)? {
            TraceValueWire::Boolean(value) => Ok(Self::Boolean(value)),
            TraceValueWire::Signed(value) => Ok(Self::Signed(value.0)),
            TraceValueWire::Unsigned(value) => Ok(Self::Unsigned(value.0)),
            TraceValueWire::Float(value) => Ok(Self::Float(value)),
            TraceValueWire::Text(value) => Ok(Self::Text(value)),
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct TraceFieldWire {
    value: TraceValue,
    visibility: FieldVisibility,
}

impl Serialize for TraceField {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        TraceFieldWire {
            value: self.value().clone(),
            visibility: self.visibility(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TraceField {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = TraceFieldWire::deserialize(deserializer)?;
        Ok(Self::from_parts(wire.value, wire.visibility))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct FailureDiagnosticWire {
    category: ErrorCategory,
    recoverability: Recoverability,
    summary: String,
    contexts: Vec<ErrorContext>,
    source_summaries: Vec<String>,
}

impl Serialize for FailureDiagnostic {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        FailureDiagnosticWire {
            category: self.category(),
            recoverability: self.recoverability(),
            summary: self.summary().to_owned(),
            contexts: self.contexts().to_vec(),
            source_summaries: self.source_summaries().to_vec(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for FailureDiagnostic {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = FailureDiagnosticWire::deserialize(deserializer)?;
        Ok(Self::from_parts(
            wire.category,
            wire.recoverability,
            wire.summary,
            wire.contexts,
            wire.source_summaries,
        ))
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UserSafeErrorWire {
    category: ErrorCategory,
    recoverability: Recoverability,
    code: String,
    title: String,
    action: String,
}

impl Serialize for UserSafeError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        UserSafeErrorWire {
            category: self.category(),
            recoverability: self.recoverability(),
            code: self.code().to_owned(),
            title: self.title().to_owned(),
            action: self.action().to_owned(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for UserSafeError {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = UserSafeErrorWire::deserialize(deserializer)?;
        let value = Self::from_parts(wire.category, wire.recoverability);
        if value.code() != wire.code || value.title() != wire.title || value.action() != wire.action
        {
            return Err(D::Error::custom(
                "user-safe error text does not match its stable classification",
            ));
        }
        Ok(value)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct DiagnosticEventWire {
    name: String,
    component: String,
    severity: DiagnosticSeverity,
    message: String,
    fields: BTreeMap<String, TraceField>,
    failure: Option<FailureDiagnostic>,
    user_safe_error: Option<UserSafeError>,
}

impl Serialize for DiagnosticEvent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        DiagnosticEventWire {
            name: self.name().to_owned(),
            component: self.component().to_owned(),
            severity: self.severity(),
            message: self.message().to_owned(),
            fields: self.fields().clone(),
            failure: self.failure().cloned(),
            user_safe_error: self.user_safe_error().cloned(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DiagnosticEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = DiagnosticEventWire::deserialize(deserializer)?;
        Self::from_parts(
            wire.name,
            wire.component,
            wire.severity,
            wire.message,
            wire.fields,
            wire.failure,
            wire.user_safe_error,
        )
        .map_err(D::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CounterSnapshotWire {
    name: String,
    unit: CounterUnit,
    value: CanonicalU64,
}

impl Serialize for CounterSnapshot {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        CounterSnapshotWire {
            name: self.name().to_owned(),
            unit: self.unit(),
            value: CanonicalU64(self.value()),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CounterSnapshot {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = CounterSnapshotWire::deserialize(deserializer)?;
        PerformanceCounter::with_initial_value(wire.name, wire.unit, wire.value.0)
            .map(|counter| counter.snapshot())
            .map_err(D::Error::custom)
    }
}
