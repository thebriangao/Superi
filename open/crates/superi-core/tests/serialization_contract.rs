use std::fmt::Debug;
use std::str::FromStr;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::diagnostics::{
    CounterSnapshot, CounterUnit, DiagnosticEvent, DiagnosticSeverity, FailureDiagnostic,
    FieldVisibility, FiniteF64, PerformanceCounter, TraceField, TraceValue, UserSafeError,
};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability};
use superi_core::geometry::{AspectRatio, Matrix3, PixelBounds, Point2, Rect, Vector2};
use superi_core::ids::{
    CacheId, ClipId, DeviceId, IdentifierKind, JobId, MediaId, NodeId, ParameterId, ProjectId,
    TrackId,
};
use superi_core::pixel::{
    AlphaMode, ChannelLayout, ChannelPosition, ChromaSubsampling, PixelFormat, PixelModel,
    PixelNumeric, PixelPacking, SampleFormat, SampleNumeric,
};
use superi_core::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION;
use superi_core::settings::{
    CapabilityId, CapabilitySet, ComponentId, FeatureAvailability, FeatureDescriptor,
    FeatureDiscovery, FeatureId, SemanticVersion, SettingKey, SettingValue, SettingValueKind,
    SettingsSnapshot, VersionIdentifier, VersionSection,
};
use superi_core::time::{
    Duration, FrameRate, RationalTime, SampleTime, TimeRange, TimeRounding, Timebase,
};
use superi_core::timecode::{Timecode, TimecodeComponent, TimecodeFormat, TimecodeMode};

fn assert_stable_wire_type<T>()
where
    T: Serialize + for<'de> Deserialize<'de>,
{
}

fn assert_round_trip<T>(value: &T)
where
    T: Serialize + DeserializeOwned + Debug + PartialEq,
{
    let encoded = serde_json::to_string(value).expect("serialize stable primitive");
    let decoded = serde_json::from_str(&encoded).expect("deserialize stable primitive");
    assert_eq!(*value, decoded);
    assert_eq!(encoded, serde_json::to_string(&decoded).unwrap());
}

fn assert_string_code<T>(value: &T, code: &str)
where
    T: Serialize + DeserializeOwned + Debug + PartialEq,
{
    assert_eq!(serde_json::to_string(value).unwrap(), format!("\"{code}\""));
    assert_round_trip(value);
}

#[test]
fn stable_public_types_implement_serde() {
    assert_stable_wire_type::<ErrorCategory>();
    assert_stable_wire_type::<Recoverability>();
    assert_stable_wire_type::<ErrorContext>();
    assert_stable_wire_type::<IdentifierKind>();
    assert_stable_wire_type::<ProjectId>();
    assert_stable_wire_type::<MediaId>();
    assert_stable_wire_type::<TrackId>();
    assert_stable_wire_type::<ClipId>();
    assert_stable_wire_type::<NodeId>();
    assert_stable_wire_type::<ParameterId>();
    assert_stable_wire_type::<JobId>();
    assert_stable_wire_type::<CacheId>();
    assert_stable_wire_type::<DeviceId>();
    assert_stable_wire_type::<Timebase>();
    assert_stable_wire_type::<FrameRate>();
    assert_stable_wire_type::<TimeRounding>();
    assert_stable_wire_type::<RationalTime>();
    assert_stable_wire_type::<SampleTime>();
    assert_stable_wire_type::<Duration>();
    assert_stable_wire_type::<TimeRange>();
    assert_stable_wire_type::<TimecodeMode>();
    assert_stable_wire_type::<TimecodeComponent>();
    assert_stable_wire_type::<TimecodeFormat>();
    assert_stable_wire_type::<Timecode>();
    assert_stable_wire_type::<Point2>();
    assert_stable_wire_type::<Vector2>();
    assert_stable_wire_type::<Matrix3>();
    assert_stable_wire_type::<Rect>();
    assert_stable_wire_type::<AspectRatio>();
    assert_stable_wire_type::<PixelBounds>();
    assert_stable_wire_type::<PixelModel>();
    assert_stable_wire_type::<PixelNumeric>();
    assert_stable_wire_type::<PixelPacking>();
    assert_stable_wire_type::<ChromaSubsampling>();
    assert_stable_wire_type::<PixelFormat>();
    assert_stable_wire_type::<AlphaMode>();
    assert_stable_wire_type::<SampleNumeric>();
    assert_stable_wire_type::<SampleFormat>();
    assert_stable_wire_type::<ChannelPosition>();
    assert_stable_wire_type::<ChannelLayout>();
    assert_stable_wire_type::<ColorPrimaries>();
    assert_stable_wire_type::<TransferFunction>();
    assert_stable_wire_type::<MatrixCoefficients>();
    assert_stable_wire_type::<ColorRange>();
    assert_stable_wire_type::<ColorSpace>();
    assert_stable_wire_type::<SettingKey>();
    assert_stable_wire_type::<CapabilityId>();
    assert_stable_wire_type::<FeatureId>();
    assert_stable_wire_type::<ComponentId>();
    assert_stable_wire_type::<VersionSection>();
    assert_stable_wire_type::<SemanticVersion>();
    assert_stable_wire_type::<VersionIdentifier>();
    assert_stable_wire_type::<SettingValueKind>();
    assert_stable_wire_type::<SettingValue>();
    assert_stable_wire_type::<SettingsSnapshot>();
    assert_stable_wire_type::<CapabilitySet>();
    assert_stable_wire_type::<FeatureAvailability>();
    assert_stable_wire_type::<FeatureDescriptor>();
    assert_stable_wire_type::<FeatureDiscovery>();
    assert_stable_wire_type::<DiagnosticSeverity>();
    assert_stable_wire_type::<FieldVisibility>();
    assert_stable_wire_type::<FiniteF64>();
    assert_stable_wire_type::<TraceValue>();
    assert_stable_wire_type::<TraceField>();
    assert_stable_wire_type::<FailureDiagnostic>();
    assert_stable_wire_type::<UserSafeError>();
    assert_stable_wire_type::<DiagnosticEvent>();
    assert_stable_wire_type::<CounterUnit>();
    assert_stable_wire_type::<CounterSnapshot>();
}

#[test]
fn permanent_codes_and_identifier_text_are_the_wire_identity() {
    assert_eq!(STABLE_PRIMITIVE_SCHEMA_REVISION, 1);

    for value in ErrorCategory::ALL {
        assert_string_code(value, value.code());
    }
    for value in Recoverability::ALL {
        assert_string_code(value, value.code());
    }
    for value in PixelFormat::ALL {
        assert_string_code(value, value.code());
    }
    for value in AlphaMode::ALL {
        assert_string_code(value, value.code());
    }
    for value in SampleFormat::ALL {
        assert_string_code(value, value.code());
    }
    for value in ColorPrimaries::ALL {
        assert_string_code(value, value.code());
    }
    for value in TransferFunction::ALL {
        assert_string_code(value, value.code());
    }
    for value in MatrixCoefficients::ALL {
        assert_string_code(value, value.code());
    }
    for value in ColorRange::ALL {
        assert_string_code(value, value.code());
    }
    for value in SettingValueKind::ALL {
        assert_string_code(value, value.code());
    }
    for value in FeatureAvailability::ALL {
        assert_string_code(value, value.code());
    }
    for value in DiagnosticSeverity::ALL {
        assert_string_code(value, value.code());
    }
    for value in FieldVisibility::ALL {
        assert_string_code(value, value.code());
    }
    for value in CounterUnit::ALL {
        assert_string_code(value, value.code());
    }

    assert_string_code(&TimeRounding::NearestTiesEven, "nearest_ties_even");
    assert_string_code(&TimecodeMode::DropFrame, "drop_frame");
    assert_string_code(&TimecodeComponent::Minutes, "minutes");
    assert_string_code(&PixelModel::Rgba, "rgba");
    assert_string_code(&PixelNumeric::Float, "float");
    assert_string_code(&PixelPacking::Semiplanar, "semiplanar");
    assert_string_code(&ChromaSubsampling::Cs420, "4:2:0");
    assert_string_code(&SampleNumeric::SignedInteger, "signed_integer");
    assert_string_code(&VersionSection::PreRelease, "pre_release");

    let raw = 0x0011_2233_4455_6677_8899_aabb_ccdd_eeff_u128;
    macro_rules! assert_id {
        ($type:ty, $prefix:literal) => {{
            let value = <$type>::from_raw(raw);
            assert_string_code(
                &value,
                concat!($prefix, ":00112233445566778899aabbccddeeff"),
            );
        }};
    }
    assert_id!(ProjectId, "project");
    assert_id!(MediaId, "media");
    assert_id!(TrackId, "track");
    assert_id!(ClipId, "clip");
    assert_id!(NodeId, "node");
    assert_id!(ParameterId, "parameter");
    assert_id!(JobId, "job");
    assert_id!(CacheId, "cache");
    assert_id!(DeviceId, "device");
}

#[test]
fn wide_integers_and_version_numbers_remain_exact_across_json() {
    let timebase = Timebase::new(24_000, 1_001).unwrap();
    let time = RationalTime::new(i64::MIN, timebase);
    assert_eq!(
        serde_json::to_string(&time).unwrap(),
        r#"{"value":"-9223372036854775808","timebase":{"numerator":24000,"denominator":1001}}"#
    );
    assert_round_trip(&time);

    let sample = SampleTime::new(i64::MAX, 192_000).unwrap();
    assert_eq!(
        serde_json::to_string(&sample).unwrap(),
        r#"{"sample":"9223372036854775807","sample_rate":192000}"#
    );
    assert_round_trip(&sample);

    let duration = Duration::new(i64::MAX as u64, Timebase::SECONDS).unwrap();
    assert_eq!(
        serde_json::to_string(&duration).unwrap(),
        r#"{"value":"9223372036854775807","timebase":{"numerator":1,"denominator":1}}"#
    );
    assert_round_trip(&duration);

    let timecode = Timecode::from_frames(
        i64::MAX,
        TimecodeFormat::drop_frame(FrameRate::FPS_60000_1001).unwrap(),
    );
    assert!(serde_json::to_string(&timecode)
        .unwrap()
        .contains(r#""frames":"9223372036854775807""#));
    assert_round_trip(&timecode);

    let integer = SettingValue::Integer(i64::MIN);
    assert_eq!(
        serde_json::to_string(&integer).unwrap(),
        r#"{"kind":"integer","value":"-9223372036854775808"}"#
    );
    assert_round_trip(&integer);

    let trace = TraceValue::Unsigned(u64::MAX);
    assert_eq!(
        serde_json::to_string(&trace).unwrap(),
        r#"{"kind":"u64","value":"18446744073709551615"}"#
    );
    assert_round_trip(&trace);

    let counter =
        PerformanceCounter::with_initial_value("render.frames", CounterUnit::Frames, u64::MAX)
            .unwrap()
            .snapshot();
    assert_eq!(
        serde_json::to_string(&counter).unwrap(),
        r#"{"name":"render.frames","unit":"frames","value":"18446744073709551615"}"#
    );
    assert_round_trip(&counter);

    let version = SemanticVersion::from_str(
        "18446744073709551615.18446744073709551615.18446744073709551615-rc.1+macos.arm64",
    )
    .unwrap();
    assert_string_code(
        &version,
        "18446744073709551615.18446744073709551615.18446744073709551615-rc.1+macos.arm64",
    );
    let qualified = VersionIdentifier::new(ComponentId::new("superi.engine").unwrap(), version);
    assert_string_code(
        &qualified,
        "superi.engine@18446744073709551615.18446744073709551615.18446744073709551615-rc.1+macos.arm64",
    );
}

#[test]
fn composite_shapes_are_explicit_ordered_and_deterministic() {
    let context = ErrorContext::new("superi-engine", "render")
        .with_field("z_frame", "42")
        .with_field("a_device", "gpu-0");
    assert_eq!(
        serde_json::to_string(&context).unwrap(),
        r#"{"component":"superi-engine","operation":"render","fields":{"a_device":"gpu-0","z_frame":"42"}}"#
    );
    assert_round_trip(&context);

    let point = Point2::new(12.5, -3.25).unwrap();
    let vector = Vector2::new(-2.0, 8.0).unwrap();
    assert_eq!(
        serde_json::to_string(&point).unwrap(),
        r#"{"x":12.5,"y":-3.25}"#
    );
    assert_eq!(
        serde_json::to_string(&vector).unwrap(),
        r#"{"x":-2.0,"y":8.0}"#
    );
    assert_round_trip(&point);
    assert_round_trip(&vector);

    let matrix = Matrix3::from_rows([[1.0, 0.0, 2.0], [0.0, 1.0, 3.0], [0.0, 0.0, 1.0]]).unwrap();
    assert_round_trip(&matrix);
    let rect = Rect::new(
        Point2::new(-1.0, 2.0).unwrap(),
        Point2::new(8.0, 9.0).unwrap(),
    )
    .unwrap();
    assert_round_trip(&rect);
    assert_round_trip(&AspectRatio::new(16, 9).unwrap());
    assert_round_trip(&PixelBounds::new(-10, -20, 30, 40).unwrap());

    let layout = ChannelLayout::new([
        ChannelPosition::FrontRight,
        ChannelPosition::FrontLeft,
        ChannelPosition::Discrete(12),
    ])
    .unwrap();
    assert_eq!(
        serde_json::to_string(&layout).unwrap(),
        r#"["front_right","front_left","discrete:12"]"#
    );
    assert_round_trip(&layout);

    assert_eq!(
        serde_json::to_string(&ColorSpace::BT2100_PQ).unwrap(),
        r#"{"primaries":"bt2020","transfer":"pq","matrix":"bt2020_non_constant","range":"limited"}"#
    );
    assert_round_trip(&ColorSpace::BT2100_PQ);

    let range = TimeRange::new(
        RationalTime::new(-12, FrameRate::FPS_24.timebase()),
        Duration::from_frames(48, FrameRate::FPS_24).unwrap(),
    )
    .unwrap();
    assert_round_trip(&range);
}

#[test]
fn settings_features_and_diagnostics_use_validated_public_state() {
    let snapshot = SettingsSnapshot::new(
        SemanticVersion::from_str("1.0.0").unwrap(),
        [
            (
                SettingKey::new("ui.theme").unwrap(),
                SettingValue::Text("dark".to_owned()),
            ),
            (
                SettingKey::new("audio.latency_samples").unwrap(),
                SettingValue::Integer(256),
            ),
        ],
    )
    .unwrap();
    let snapshot_json = serde_json::to_string(&snapshot).unwrap();
    assert_eq!(
        snapshot_json,
        r#"{"schema_version":"1.0.0","values":[{"key":"audio.latency_samples","value":{"kind":"integer","value":"256"}},{"key":"ui.theme","value":{"kind":"text","value":"dark"}}]}"#
    );
    assert_round_trip(&snapshot);

    let gpu = CapabilityId::new("render.gpu").unwrap();
    let capabilities = CapabilitySet::new([gpu.clone()]);
    let feature = FeatureDescriptor::new(
        FeatureId::new("render.preview").unwrap(),
        SemanticVersion::from_str("2.1.0").unwrap(),
        FeatureAvailability::Available,
        CapabilitySet::new([gpu]),
    );
    let discovery = FeatureDiscovery::new(
        SemanticVersion::from_str("1.0.0").unwrap(),
        VersionIdentifier::from_str("superi.engine@0.2.0").unwrap(),
        capabilities,
        [feature],
    )
    .unwrap();
    assert_round_trip(&discovery);

    let event = DiagnosticEvent::new(
        "render.frame.completed",
        "superi-engine.render",
        DiagnosticSeverity::Info,
        "frame completed",
    )
    .unwrap()
    .with_field("z.private", TraceField::sensitive("/private/media.mov"))
    .unwrap()
    .with_field("a.frame", TraceField::user_safe(u64::MAX))
    .unwrap();
    let event_json = serde_json::to_string(&event).unwrap();
    assert!(event_json.find("a.frame").unwrap() < event_json.find("z.private").unwrap());
    assert_round_trip(&event);

    let error = Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "device detail",
    )
    .with_context(ErrorContext::new("superi-gpu", "submit"));
    let error_event =
        DiagnosticEvent::from_error("render.submit.failed", "superi-engine.render", &error)
            .unwrap();
    assert_round_trip(&error_event);
}

#[test]
fn invalid_or_noncanonical_wire_values_are_rejected() {
    assert!(serde_json::from_str::<PixelFormat>(r#""Rgba16Float""#).is_err());
    assert!(
        serde_json::from_str::<ProjectId>(r#""project:00112233445566778899AABBCCDDEEFF""#).is_err()
    );
    assert!(serde_json::from_str::<RationalTime>(
        r#"{"value":9007199254740992,"timebase":{"numerator":1,"denominator":1}}"#
    )
    .is_err());
    for text in ["+1", "01", "-0", " 1", "1 "] {
        let input = format!(r#"{{"value":"{text}","timebase":{{"numerator":1,"denominator":1}}}}"#);
        assert!(
            serde_json::from_str::<RationalTime>(&input).is_err(),
            "{text}"
        );
    }
    assert!(serde_json::from_str::<Duration>(
        r#"{"value":"18446744073709551615","timebase":{"numerator":1,"denominator":1}}"#
    )
    .is_err());
    assert!(serde_json::from_str::<Timebase>(r#"{"numerator":24,"denominator":0}"#).is_err());
    assert!(serde_json::from_str::<Timebase>(r#"{"numerator":24,"denominator":2}"#).is_err());
    assert!(serde_json::from_str::<AspectRatio>(r#"{"numerator":32,"denominator":18}"#).is_err());
    assert!(
        serde_json::from_str::<Timebase>(r#"{"numerator":24,"denominator":1,"future":true}"#)
            .is_err()
    );
    assert!(serde_json::from_str::<Point2>(r#"{"x":1e400,"y":0.0}"#).is_err());
    assert!(
        serde_json::from_str::<Rect>(r#"{"min":{"x":2.0,"y":0.0},"max":{"x":1.0,"y":1.0}}"#)
            .is_err()
    );
    assert!(serde_json::from_str::<ChannelLayout>(r#"["front_left","front_left"]"#).is_err());
    assert!(serde_json::from_str::<ChannelPosition>(r#""discrete:01""#).is_err());
    assert!(serde_json::from_str::<TimecodeFormat>(
        r#"{"rate":{"numerator":24,"denominator":1},"mode":"drop_frame"}"#
    )
    .is_err());

    let duplicate_settings = r#"{"schema_version":"1.0.0","values":[{"key":"ui.theme","value":{"kind":"text","value":"dark"}},{"key":"ui.theme","value":{"kind":"text","value":"light"}}]}"#;
    assert!(serde_json::from_str::<SettingsSnapshot>(duplicate_settings).is_err());

    let inconsistent_feature = r#"{"schema_version":"1.0.0","producer":"superi.engine@1.0.0","capabilities":[],"features":[{"id":"render.preview","version":"1.0.0","availability":"available","required_capabilities":["render.gpu"]}]}"#;
    assert!(serde_json::from_str::<FeatureDiscovery>(inconsistent_feature).is_err());

    let safe = UserSafeError::from_error(&Error::new(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "internal",
    ));
    let mut safe_json = serde_json::to_value(&safe).unwrap();
    safe_json["title"] = Value::String("tampered title".to_owned());
    assert!(serde_json::from_value::<UserSafeError>(safe_json).is_err());

    let regular = DiagnosticEvent::new(
        "render.completed",
        "superi-engine",
        DiagnosticSeverity::Info,
        "done",
    )
    .unwrap();
    let mut regular_json = serde_json::to_value(&regular).unwrap();
    regular_json["user_safe_error"] = serde_json::to_value(&safe).unwrap();
    assert!(serde_json::from_value::<DiagnosticEvent>(regular_json).is_err());
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PublicConsumerState {
    project_id: ProjectId,
    position: RationalTime,
    bounds: PixelBounds,
    pixel_format: PixelFormat,
    color_space: ColorSpace,
    settings: SettingsSnapshot,
    event: DiagnosticEvent,
}

#[test]
fn nested_public_consumer_state_round_trips_without_private_shortcuts() {
    let state = PublicConsumerState {
        project_id: ProjectId::from_raw(u128::MAX),
        position: RationalTime::new(i64::MAX, FrameRate::FPS_24000_1001.timebase()),
        bounds: PixelBounds::new(-2_048, -1_080, 2_048, 1_080).unwrap(),
        pixel_format: PixelFormat::Rgba16Float,
        color_space: ColorSpace::ACESCG,
        settings: SettingsSnapshot::new(
            SemanticVersion::from_str("1.0.0").unwrap(),
            [(
                SettingKey::new("render.quality").unwrap(),
                SettingValue::Text("full".to_owned()),
            )],
        )
        .unwrap(),
        event: DiagnosticEvent::new(
            "render.started",
            "superi-engine.render",
            DiagnosticSeverity::Info,
            "render started",
        )
        .unwrap(),
    };

    let encoded = serde_json::to_vec(&state).unwrap();
    let decoded: PublicConsumerState = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(state, decoded);

    let value: Value = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(
        value["project_id"],
        json!("project:ffffffffffffffffffffffffffffffff")
    );
    assert_eq!(value["position"]["value"], json!(i64::MAX.to_string()));
    assert_eq!(value["pixel_format"], json!("rgba16_float"));
}
