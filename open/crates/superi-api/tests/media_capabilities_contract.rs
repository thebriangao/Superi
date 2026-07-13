use std::sync::Arc;

use superi_api::api::{
    CapabilityConstraint, HardwareAcceleration as PublicHardwareAcceleration, MediaCapabilitiesApi,
    MediaOperation,
};
use superi_api::commands::{ApiCommand, GetMediaCapabilities, GetMediaCapabilitiesResult};
use superi_api::events::{ApiEvent, MediaCapabilitiesChanged};
use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use superi_engine::introspection::MediaCapabilities;
use superi_engine::media::media_backend_registry;
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendDescriptor, BackendRegistration,
    BackendRegistry, BackendTier, ChromaSampling, CodecCapability, CodecOperation,
    HardwareAcceleration, MediaBackend,
};
use superi_media_io::decode::{Decoder, DecoderConfig};
use superi_media_io::demux::{
    BackendId, CodecId, MediaSource, SourceProbe, SourceProbeResult, SourceRequest,
};
use superi_media_io::encode::{Encoder, EncoderConfig};
use superi_media_io::operation::OperationContext;

struct DeclaredBackend {
    descriptor: BackendDescriptor,
}

impl DeclaredBackend {
    fn new(id: &str) -> Self {
        Self {
            descriptor: BackendDescriptor::new(
                BackendId::new(id).unwrap(),
                format!("{id} test backend"),
            )
            .unwrap(),
        }
    }

    fn unexpected_factory_call() -> Error {
        Error::new(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "capability introspection must not instantiate media resources",
        )
    }
}

impl MediaBackend for DeclaredBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        _probe: &SourceProbe<'_>,
        _operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        Ok(SourceProbeResult::NoMatch)
    }

    fn open_source(
        &self,
        _request: &SourceRequest,
        _operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        Err(Self::unexpected_factory_call())
    }

    fn create_decoder(
        &self,
        _config: &DecoderConfig,
        _operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>> {
        Err(Self::unexpected_factory_call())
    }

    fn create_encoder(
        &self,
        _config: &EncoderConfig,
        _operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        Err(Self::unexpected_factory_call())
    }
}

fn capability_decode(codec: &str) -> BackendCapability {
    BackendCapability::Decode(CodecId::new(codec).unwrap())
}

fn register(
    registry: &mut BackendRegistry,
    id: &str,
    capabilities: impl IntoIterator<Item = BackendCapability>,
    priority: u16,
    tier: BackendTier,
) {
    registry
        .register(
            BackendRegistration::new(
                Arc::new(DeclaredBackend::new(id)),
                BackendCapabilities::new(capabilities),
                priority,
                tier,
            )
            .unwrap(),
        )
        .unwrap();
}

#[test]
fn query_is_deterministic_strict_and_exposes_effective_fallback_policy() {
    let mut registry = BackendRegistry::new();
    register(
        &mut registry,
        "z-primary",
        [capability_decode("av1")],
        10,
        BackendTier::Primary,
    );
    register(
        &mut registry,
        "platform",
        [capability_decode("h264"), capability_decode("av1")],
        100,
        BackendTier::Fallback,
    );
    register(
        &mut registry,
        "a-primary",
        [BackendCapability::Source, capability_decode("av1")],
        10,
        BackendTier::Primary,
    );

    let api = MediaCapabilitiesApi::new(&MediaCapabilities::from_registry(&registry).unwrap());
    let result = api.execute(GetMediaCapabilities::new());
    let snapshot = result.snapshot();

    assert_eq!(
        GetMediaCapabilities::METHOD,
        "superi.media.capabilities.get"
    );
    assert_eq!(snapshot.schema_version().to_string(), "2.0.0");
    assert_eq!(snapshot.revision(), 0);
    assert_eq!(
        snapshot
            .backends()
            .iter()
            .map(|backend| backend.id())
            .collect::<Vec<_>>(),
        ["a-primary", "platform", "z-primary"]
    );

    let source = snapshot
        .operations()
        .iter()
        .find(|support| support.operation() == &MediaOperation::Source)
        .unwrap();
    assert_eq!(source.primary_backends(), ["a-primary"]);
    assert!(source.fallback_backends().is_empty());

    let av1 = snapshot
        .operations()
        .iter()
        .find(|support| {
            support.operation()
                == &MediaOperation::Decode {
                    codec: "av1".to_owned(),
                }
        })
        .unwrap();
    assert_eq!(av1.primary_backends(), ["a-primary", "z-primary"]);
    assert_eq!(av1.fallback_backends(), ["platform"]);

    let h264 = snapshot
        .operations()
        .iter()
        .find(|support| {
            support.operation()
                == &MediaOperation::Decode {
                    codec: "h264".to_owned(),
                }
        })
        .unwrap();
    assert!(h264.primary_backends().is_empty());
    assert_eq!(h264.fallback_backends(), ["platform"]);

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(
        json["snapshot"]["operations"][0]["operation"]["kind"],
        "source"
    );
    let decoded: GetMediaCapabilitiesResult = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, result);
    assert!(
        serde_json::from_value::<GetMediaCapabilities>(serde_json::json!({
            "unexpected": true
        }))
        .is_err()
    );
}

#[test]
fn state_changes_emit_one_typed_event_for_ui_and_automation_clients() {
    let mut registry = BackendRegistry::new();
    register(
        &mut registry,
        "software",
        [BackendCapability::Source, capability_decode("av1")],
        10,
        BackendTier::Primary,
    );
    let initial = MediaCapabilities::from_registry(&registry).unwrap();
    let mut api = MediaCapabilitiesApi::new(&initial);

    assert!(api.synchronize(&initial).unwrap().is_none());
    register(
        &mut registry,
        "platform",
        [capability_decode("h264")],
        20,
        BackendTier::Fallback,
    );

    let changed = MediaCapabilities::from_registry(&registry).unwrap();
    let event = api.synchronize(&changed).unwrap().unwrap();
    assert_eq!(
        MediaCapabilitiesChanged::NAME,
        "superi.media.capabilities.changed"
    );
    assert_eq!(event.snapshot().revision(), 1);
    assert_eq!(
        api.execute(GetMediaCapabilities::new())
            .snapshot()
            .revision(),
        1
    );
    assert!(api.synchronize(&changed).unwrap().is_none());

    let event_json = serde_json::to_string(&event).unwrap();
    let decoded: MediaCapabilitiesChanged = serde_json::from_str(&event_json).unwrap();
    assert_eq!(decoded, event);
}

#[test]
fn detailed_capability_changes_emit_the_same_full_replacement_event() {
    let codec = CodecId::new("av1").unwrap();
    let mut initial_registry = BackendRegistry::new();
    register(
        &mut initial_registry,
        "software",
        [BackendCapability::Decode(codec.clone())],
        10,
        BackendTier::Primary,
    );
    let initial = MediaCapabilities::from_registry(&initial_registry).unwrap();
    let mut api = MediaCapabilitiesApi::new(&initial);

    let detail = CodecCapability::new(CodecOperation::Decode, codec.clone())
        .with_profiles(["main"])
        .unwrap()
        .with_levels_runtime()
        .with_bit_depths([8, 10])
        .unwrap()
        .with_chroma_sampling([ChromaSampling::Monochrome, ChromaSampling::Cs420])
        .unwrap();
    let capabilities = BackendCapabilities::new([BackendCapability::Decode(codec)])
        .with_hardware_acceleration(HardwareAcceleration::Software)
        .with_codec_capabilities([detail])
        .unwrap();
    let mut changed_registry = BackendRegistry::new();
    changed_registry
        .register(
            BackendRegistration::new(
                Arc::new(DeclaredBackend::new("software")),
                capabilities,
                10,
                BackendTier::Primary,
            )
            .unwrap(),
        )
        .unwrap();

    let changed = MediaCapabilities::from_registry(&changed_registry).unwrap();
    let event = api.synchronize(&changed).unwrap().unwrap();
    assert_eq!(event.snapshot().revision(), 1);
    assert_eq!(
        event.snapshot().backends()[0].hardware_acceleration(),
        PublicHardwareAcceleration::Software
    );
    assert!(matches!(
        event.snapshot().backends()[0].codec_capabilities()[0].profiles(),
        CapabilityConstraint::Values { values } if values.as_slice() == ["main"]
    ));
    assert!(api.synchronize(&changed).unwrap().is_none());
}

#[test]
fn detailed_codec_rows_expose_valid_tuples_hardware_and_encode_support() {
    let codec = CodecId::new("av1").unwrap();
    let decode = CodecCapability::new(CodecOperation::Decode, codec.clone())
        .with_profiles(["main"])
        .unwrap()
        .with_levels_runtime()
        .with_bit_depths([8, 10])
        .unwrap()
        .with_chroma_sampling([ChromaSampling::Monochrome, ChromaSampling::Cs420])
        .unwrap();
    let encode = CodecCapability::new(CodecOperation::Encode, codec.clone())
        .with_profiles(["main"])
        .unwrap()
        .with_levels_runtime()
        .with_bit_depths([8, 10])
        .unwrap()
        .with_chroma_sampling([ChromaSampling::Monochrome, ChromaSampling::Cs420])
        .unwrap();
    let capabilities = BackendCapabilities::new([
        BackendCapability::Decode(codec.clone()),
        BackendCapability::Encode(codec),
    ])
    .with_hardware_acceleration(HardwareAcceleration::Software)
    .with_codec_capabilities([decode, encode])
    .unwrap();
    let mut registry = BackendRegistry::new();
    registry
        .register(
            BackendRegistration::new(
                Arc::new(DeclaredBackend::new("rust-av1")),
                capabilities,
                100,
                BackendTier::Primary,
            )
            .unwrap(),
        )
        .unwrap();

    let api = MediaCapabilitiesApi::new(&MediaCapabilities::from_registry(&registry).unwrap());
    let result = api.execute(GetMediaCapabilities::new());
    let snapshot = result.snapshot();
    assert_eq!(snapshot.schema_version().to_string(), "2.0.0");
    let backend = &snapshot.backends()[0];
    assert_eq!(
        backend.hardware_acceleration(),
        PublicHardwareAcceleration::Software
    );
    assert_eq!(backend.codec_capabilities().len(), 2);
    let encode = backend
        .codec_capabilities()
        .iter()
        .find(|capability| {
            capability.operation()
                == &MediaOperation::Encode {
                    codec: "av1".to_owned(),
                }
        })
        .unwrap();
    assert!(matches!(
        encode.profiles(),
        CapabilityConstraint::Values { values } if values.as_slice() == ["main"]
    ));
    assert!(matches!(
        encode.levels(),
        CapabilityConstraint::RuntimeNegotiated
    ));
    assert!(matches!(
        encode.bit_depths(),
        CapabilityConstraint::Values { values } if values.as_slice() == [8, 10]
    ));
    assert!(matches!(
        encode.chroma_sampling(),
        CapabilityConstraint::Values { values }
            if values.as_slice() == [superi_api::api::ChromaSampling::Monochrome,
                superi_api::api::ChromaSampling::Cs420]
    ));

    let json = serde_json::to_value(&result).unwrap();
    assert_eq!(
        json["snapshot"]["backends"][0]["hardware_acceleration"],
        "software"
    );
    assert_eq!(
        json["snapshot"]["backends"][0]["codec_capabilities"][1]["operation"]["kind"],
        "encode"
    );
    assert_eq!(
        json["snapshot"]["backends"][0]["codec_capabilities"][1]["levels"]["kind"],
        "runtime_negotiated"
    );
    let mut unknown_detail = json.clone();
    unknown_detail["snapshot"]["backends"][0]["codec_capabilities"][0]["unexpected"] =
        serde_json::json!(true);
    assert!(serde_json::from_value::<GetMediaCapabilitiesResult>(unknown_detail).is_err());
    let decoded: GetMediaCapabilitiesResult = serde_json::from_value(json).unwrap();
    assert_eq!(decoded, result);
}

#[test]
fn assembled_default_registry_reaches_the_strict_public_capability_snapshot() {
    let registry = media_backend_registry().unwrap();
    let api = MediaCapabilitiesApi::new(&MediaCapabilities::from_registry(&registry).unwrap());
    let result = api.execute(GetMediaCapabilities::new());
    let av1 = result
        .snapshot()
        .backends()
        .iter()
        .find(|backend| backend.id() == "rust-av1")
        .unwrap();
    assert_eq!(
        av1.hardware_acceleration(),
        PublicHardwareAcceleration::Software
    );
    let av1_encode = av1
        .codec_capabilities()
        .iter()
        .filter(|detail| {
            detail.operation()
                == &MediaOperation::Encode {
                    codec: "av1".to_owned(),
                }
        })
        .collect::<Vec<_>>();
    assert_eq!(av1_encode.len(), 3);
    assert!(av1_encode.iter().any(|detail| {
        matches!(
            detail.profiles(),
            CapabilityConstraint::Values { values } if values.as_slice() == ["main"]
        ) && matches!(
            detail.bit_depths(),
            CapabilityConstraint::Values { values } if values.as_slice() == [8, 10]
        )
    }));

    let json = serde_json::to_value(result).unwrap();
    assert_eq!(json["snapshot"]["schema_version"], "2.0.0");
}
