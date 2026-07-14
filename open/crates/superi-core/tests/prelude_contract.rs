use std::str::FromStr;

use serde::{Deserialize, Serialize};
use superi_core::prelude::{
    AlphaMode, AspectRatio, CacheId, CapabilityId, CapabilitySet, CaptionId, ChannelLayout,
    ChannelPosition, ChromaSubsampling, ClipId, ColorPrimaries, ColorRange, ColorSpace,
    ComponentId, CounterSnapshot, CounterUnit, DeviceId, DiagnosticEvent, DiagnosticSeverity,
    Duration, EdgeId, Error, ErrorCategory, ErrorContext, FeatureAvailability, FeatureDescriptor,
    FeatureDiscovery, FeatureId, FieldVisibility, FiniteF64, FrameRate, GapId, GeneratorId,
    GraphId, IdentifierKind, JobId, Matrix3, MatrixCoefficients, MediaId, NodeId, ParameterId,
    PerformanceCounter, PixelBounds, PixelFormat, PixelModel, PixelNumeric, PixelPacking, Point2,
    PortId, ProjectId, RationalTime, Recoverability, Rect, ResourceId, Result, ResultExt,
    SampleFormat, SampleNumeric, SampleTime, SemanticVersion, SettingKey, SettingValue,
    SettingValueKind, SettingsSnapshot, TimeRange, TimeRounding, Timebase, Timecode,
    TimecodeFormat, TimecodeMode, TimelineId, TraceField, TraceValue, TrackId, TransferFunction,
    TransitionId, TypedId, UserSafeError, Vector2, VersionIdentifier,
    STABLE_PRIMITIVE_SCHEMA_REVISION,
};

fn assert_stable_wire_type<T>()
where
    T: Serialize + for<'de> Deserialize<'de>,
{
}

fn raw_id<T: TypedId>(id: T) -> u128 {
    id.raw()
}

fn add_context<T>(result: Result<T>) -> Result<T> {
    result.with_error_context(ErrorContext::new("prelude.consumer", "add_context"))
}

#[test]
fn prelude_exposes_the_complete_curated_contract() {
    assert_stable_wire_type::<ErrorCategory>();
    assert_stable_wire_type::<Recoverability>();
    assert_stable_wire_type::<ErrorContext>();
    assert_stable_wire_type::<IdentifierKind>();
    assert_stable_wire_type::<ProjectId>();
    assert_stable_wire_type::<MediaId>();
    assert_stable_wire_type::<TrackId>();
    assert_stable_wire_type::<ClipId>();
    assert_stable_wire_type::<TimelineId>();
    assert_stable_wire_type::<GapId>();
    assert_stable_wire_type::<TransitionId>();
    assert_stable_wire_type::<GeneratorId>();
    assert_stable_wire_type::<CaptionId>();
    assert_stable_wire_type::<NodeId>();
    assert_stable_wire_type::<ParameterId>();
    assert_stable_wire_type::<JobId>();
    assert_stable_wire_type::<CacheId>();
    assert_stable_wire_type::<DeviceId>();
    assert_stable_wire_type::<GraphId>();
    assert_stable_wire_type::<PortId>();
    assert_stable_wire_type::<EdgeId>();
    assert_stable_wire_type::<ResourceId>();
    assert_stable_wire_type::<Timebase>();
    assert_stable_wire_type::<FrameRate>();
    assert_stable_wire_type::<TimeRounding>();
    assert_stable_wire_type::<RationalTime>();
    assert_stable_wire_type::<SampleTime>();
    assert_stable_wire_type::<Duration>();
    assert_stable_wire_type::<TimeRange>();
    assert_stable_wire_type::<TimecodeMode>();
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
    assert_stable_wire_type::<UserSafeError>();
    assert_stable_wire_type::<DiagnosticEvent>();
    assert_stable_wire_type::<CounterUnit>();
    assert_stable_wire_type::<CounterSnapshot>();

    let id = ProjectId::from_raw(7);
    assert_eq!(raw_id(id), 7);
    assert_eq!(STABLE_PRIMITIVE_SCHEMA_REVISION, 1);

    let error = Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "invalid prelude consumer input",
    );
    assert_eq!(
        add_context::<()>(Err(error)).unwrap_err().contexts().len(),
        1
    );

    let counter = PerformanceCounter::new("prelude.contracts", CounterUnit::Count).unwrap();
    assert_eq!(counter.increment(), 1);
}

#[test]
fn prelude_only_consumer_round_trips_cross_domain_state() {
    let schema_version = SemanticVersion::from_str("1.0.0").unwrap();
    let producer = VersionIdentifier::new(
        ComponentId::from_str("superi.engine").unwrap(),
        SemanticVersion::from_str("2.4.0").unwrap(),
    );
    let render_capability = CapabilityId::from_str("superi.render.gpu").unwrap();
    let capabilities = CapabilitySet::new([render_capability.clone()]);
    let feature_id = FeatureId::from_str("superi.render.preview").unwrap();
    let discovery = FeatureDiscovery::new(
        schema_version.clone(),
        producer,
        capabilities,
        [FeatureDescriptor::new(
            feature_id.clone(),
            SemanticVersion::from_str("1.2.0").unwrap(),
            FeatureAvailability::Available,
            CapabilitySet::new([render_capability]),
        )],
    )
    .unwrap();

    let settings = SettingsSnapshot::new(
        schema_version,
        [(
            SettingKey::from_str("superi.playback.loop").unwrap(),
            SettingValue::Boolean(true),
        )],
    )
    .unwrap();
    let project = ProjectId::from_raw(0x1234);
    let time = RationalTime::from_frames(48, FrameRate::FPS_24000_1001);
    let bounds = PixelBounds::from_origin_size(-16, 8, 3840, 2160).unwrap();
    let transform = Matrix3::translation(Vector2::new(12.5, -4.0).unwrap());
    let timecode = Timecode::from_frames(
        1_798,
        TimecodeFormat::drop_frame(FrameRate::FPS_30000_1001).unwrap(),
    );
    let channels = ChannelLayout::stereo();

    let error = Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Degraded,
        "preview device unavailable",
    )
    .with_context(
        ErrorContext::new("superi-engine.preview", "select_device")
            .with_field("device", DeviceId::from_raw(9).to_string()),
    );
    let event = DiagnosticEvent::from_error(
        "preview.device.unavailable",
        "superi-engine.preview",
        &error,
    )
    .unwrap()
    .with_field("project", TraceField::user_safe(project.to_string()))
    .unwrap();

    let encoded = serde_json::to_string(&(
        project,
        settings,
        discovery,
        time,
        bounds,
        transform,
        timecode,
        channels,
        PixelFormat::Rgba16Float,
        AlphaMode::Premultiplied,
        ColorSpace::ACESCG,
        event,
    ))
    .unwrap();
    let decoded: (
        ProjectId,
        SettingsSnapshot,
        FeatureDiscovery,
        RationalTime,
        PixelBounds,
        Matrix3,
        Timecode,
        ChannelLayout,
        PixelFormat,
        AlphaMode,
        ColorSpace,
        DiagnosticEvent,
    ) = serde_json::from_str(&encoded).unwrap();

    assert_eq!(decoded.0, project);
    assert_eq!(decoded.3, time);
    assert_eq!(decoded.4, bounds);
    assert_eq!(decoded.5, transform);
    assert_eq!(decoded.6, timecode);
    assert_eq!(decoded.7, ChannelLayout::stereo());
    assert_eq!(decoded.8, PixelFormat::Rgba16Float);
    assert_eq!(decoded.9, AlphaMode::Premultiplied);
    assert_eq!(decoded.10, ColorSpace::ACESCG);
    assert_eq!(decoded.1.len(), 1);
    assert!(decoded.2.is_available(&feature_id));
    assert_eq!(decoded.11.severity(), DiagnosticSeverity::Warning);
    assert_eq!(
        decoded.11.user_safe_error().unwrap().recoverability(),
        Recoverability::Degraded
    );
    assert_eq!(
        decoded
            .11
            .user_safe_fields()
            .map(|(name, _)| name)
            .collect::<Vec<_>>(),
        ["project"]
    );
}
