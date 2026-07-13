use std::collections::BTreeSet;

use superi_codecs_rs::flac::FlacBackend;
use superi_codecs_rs::register::{default_backend_registry, register_default_backends};
use superi_core::error::{ErrorCategory, Recoverability};
use superi_media_io::backend::{
    BackendCapability, BackendRegistry, BackendRequirement, BackendTier, FallbackPolicy,
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
        }

        assert!(!decoded_codecs.is_empty());
        assert_eq!(decoded_codecs, encoded_codecs);
    }
}
