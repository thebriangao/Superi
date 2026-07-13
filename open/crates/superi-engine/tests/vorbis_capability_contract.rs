use superi_codecs_rs::register::register_default_backends;
use superi_engine::introspection::{MediaCapabilities, MediaOperation};
use superi_media_io::backend::BackendRegistry;

#[test]
fn assembled_engine_capabilities_publish_vorbis_decode_and_encode() {
    let mut registry = BackendRegistry::new();
    register_default_backends(&mut registry).unwrap();

    let capabilities = MediaCapabilities::from_registry(&registry).unwrap();
    let vorbis_operations = capabilities
        .operations()
        .iter()
        .filter(|support| {
            matches!(
                support.operation(),
                MediaOperation::Decode { codec } | MediaOperation::Encode { codec }
                    if codec == "vorbis"
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(vorbis_operations.len(), 2);
    for support in vorbis_operations {
        assert_eq!(support.primary_backends(), ["rust-vorbis"]);
        assert!(support.fallback_backends().is_empty());
    }
}
