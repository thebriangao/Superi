use std::collections::BTreeSet;

use superi_codecs_rs::flac::FlacBackend;
use superi_codecs_rs::register::{default_backend_registry, register_default_backends};
use superi_core::error::{ErrorCategory, Recoverability};
use superi_media_io::backend::{
    BackendCapability, BackendRegistry, BackendRequirement, BackendTier, CapabilityConstraint,
    ChromaSampling, CodecOperation, FallbackPolicy, HardwareAcceleration,
};

fn backend_ids(registry: &BackendRegistry) -> Vec<String> {
    registry
        .registrations()
        .map(|registration| registration.backend().descriptor().id().as_str().to_owned())
        .collect()
}

#[test]
fn conflicting_default_registration_does_not_partially_mutate_the_registry() {
    let mut registry = BackendRegistry::new();
    registry
        .register(FlacBackend::registration().unwrap())
        .unwrap();
    let before = backend_ids(&registry);

    let error = register_default_backends(&mut registry).unwrap_err();

    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    let context = error.contexts().last().unwrap();
    assert_eq!(context.component(), "superi-codecs-rs.register");
    assert_eq!(context.operation(), "register_default_backends");
    assert_eq!(context.field("backend_id"), Some("rust-flac"));
    assert_eq!(backend_ids(&registry), before);
}

#[test]
fn assembled_default_registry_exposes_every_implemented_backend_deterministically() {
    let registry = default_backend_registry().unwrap();

    assert_eq!(
        backend_ids(&registry),
        [
            "rust-pcm",
            "rust-av1",
            "rust-mp3",
            "rust-flac",
            "rust-vorbis",
            "rust-opus",
            "libvpx",
        ]
    );

    for registration in registry.registrations() {
        assert_eq!(registration.priority(), 100);
        assert_eq!(registration.tier(), BackendTier::Primary);
        assert_eq!(
            registration.capabilities().hardware_acceleration(),
            HardwareAcceleration::Software
        );
        let backend_id = registration.backend().descriptor().id().as_str();
        let mut decoded_codecs = BTreeSet::new();
        let mut encoded_codecs = BTreeSet::new();

        for capability in registration.capabilities().iter() {
            let requirement = match capability {
                BackendCapability::Decode(codec) => {
                    decoded_codecs.insert(codec.as_str());
                    BackendRequirement::decode(codec.clone())
                }
                BackendCapability::Encode(codec) => {
                    encoded_codecs.insert(codec.as_str());
                    BackendRequirement::encode(codec.clone())
                }
                BackendCapability::Source => panic!("codec backends do not own container probing"),
                _ => panic!("unexpected backend capability"),
            };
            let selection = registry
                .select(&requirement, FallbackPolicy::AllowRegistered)
                .unwrap();
            assert_eq!(selection.primary().descriptor().id().as_str(), backend_id);
            assert!(!selection.fallback_used());
            assert!(selection.fallbacks().is_empty());

            let (operation, codec) = match capability {
                BackendCapability::Decode(codec) => (CodecOperation::Decode, codec),
                BackendCapability::Encode(codec) => (CodecOperation::Encode, codec),
                BackendCapability::Source => unreachable!(),
                _ => unreachable!(),
            };
            assert!(registration
                .capabilities()
                .codec_capabilities()
                .any(|detail| detail.operation() == operation && detail.codec() == codec));
        }

        assert!(!decoded_codecs.is_empty());
        assert_eq!(decoded_codecs, encoded_codecs);
    }
}

#[test]
fn av1_and_vp9_capabilities_preserve_profile_depth_and_chroma_tuples() {
    let registry = default_backend_registry().unwrap();
    let av1 = registry
        .registrations()
        .find(|registration| registration.backend().descriptor().id().as_str() == "rust-av1")
        .unwrap();
    let av1_encode = av1
        .capabilities()
        .codec_capabilities()
        .filter(|detail| detail.operation() == CodecOperation::Encode)
        .collect::<Vec<_>>();
    assert_eq!(av1_encode.len(), 3);
    assert!(av1_encode.iter().any(|detail| {
        matches!(
            detail.profiles(),
            CapabilityConstraint::Values(values) if values.contains("main")
        ) && matches!(
            detail.bit_depths(),
            CapabilityConstraint::Values(values) if values.iter().copied().eq([8, 10])
        ) && matches!(
            detail.chroma_sampling(),
            CapabilityConstraint::Values(values)
                if values.iter().copied().eq([ChromaSampling::Monochrome, ChromaSampling::Cs420])
        )
    }));

    let vpx = registry
        .registrations()
        .find(|registration| registration.backend().descriptor().id().as_str() == "libvpx")
        .unwrap();
    let profile_three = vpx
        .capabilities()
        .codec_capabilities()
        .find(|detail| {
            detail.operation() == CodecOperation::Encode
                && detail.codec().as_str() == "vp9"
                && matches!(
                    detail.profiles(),
                    CapabilityConstraint::Values(values) if values.contains("profile_3")
                )
        })
        .unwrap();
    assert!(matches!(
        profile_three.bit_depths(),
        CapabilityConstraint::Values(values) if values.iter().copied().eq([10])
    ));
    assert!(matches!(
        profile_three.chroma_sampling(),
        CapabilityConstraint::Values(values)
            if values.iter().copied().eq([ChromaSampling::Cs422, ChromaSampling::Cs444])
    ));
}
