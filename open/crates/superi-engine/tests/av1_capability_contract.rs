use superi_codecs_rs::register::register_default_backends;
use superi_engine::introspection::{MediaCapabilities, MediaOperation};
use superi_media_io::backend::BackendRegistry;

#[test]
fn assembled_engine_capabilities_publish_av1_decode_and_encode() {
    let mut registry = BackendRegistry::new();
    register_default_backends(&mut registry).unwrap();

    let capabilities = MediaCapabilities::from_registry(&registry).unwrap();
    let av1_operations = capabilities
        .operations()
        .iter()
        .filter(|support| {
            matches!(
                support.operation(),
                MediaOperation::Decode { codec } | MediaOperation::Encode { codec }
                    if codec == "av1"
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(av1_operations.len(), 2);
    for support in av1_operations {
        assert_eq!(support.primary_backends(), ["rust-av1"]);
        assert!(support.fallback_backends().is_empty());
    }
}
