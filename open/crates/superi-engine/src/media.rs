//! Construction of the complete media backend registry used by engine consumers.

use std::collections::BTreeSet;
use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendRegistration, BackendRegistry, BackendTier,
    MediaBackend,
};
use superi_media_io::mkv_webm::MkvWebmBackend;
use superi_media_io::mp4_mov::Mp4MovBackend;
use superi_media_io::mxf::MxfBackend;
#[cfg(feature = "vendor-codecs")]
use superi_media_io::operation::OperationContext;
use superi_media_io::pcm::PcmContainerBackend;

const SOURCE_PRIORITY: u16 = 100;

/// Builds the complete registry for this engine configuration and host.
///
/// The default permissive codec backends and all in-tree container sources are always present.
/// When `os-codecs` is enabled, platform backends add only operations discovered from the current
/// operating system. Construction is local and source identifiers are preflighted before any source
/// registration, so callers never observe a partially initialized registry.
pub fn media_backend_registry() -> Result<BackendRegistry> {
    let mut registry = BackendRegistry::new();
    superi_codecs_rs::register::register_default_backends(&mut registry)?;
    #[cfg(feature = "os-codecs")]
    superi_codecs_platform::register::register_platform_backends(&mut registry)?;
    register_source_backends(&mut registry)?;
    Ok(registry)
}

/// Builds the complete registry plus caller-selected vendor RAW worker plugins.
///
/// This constructor exists only with `vendor-codecs`. It never discovers or downloads workers,
/// and registration remains atomic if any explicit worker fails its handshake.
#[cfg(feature = "vendor-codecs")]
pub fn media_backend_registry_with_vendor_plugins(
    plugins: &[superi_codecs_vendor::VendorPluginConfig],
    operation: &OperationContext,
) -> Result<BackendRegistry> {
    let mut registry = media_backend_registry()?;
    superi_codecs_vendor::register_vendor_plugins(&mut registry, plugins, operation)?;
    Ok(registry)
}

fn register_source_backends(registry: &mut BackendRegistry) -> Result<()> {
    let registrations = vec![
        source_registration(Arc::new(MkvWebmBackend::new()?))?,
        source_registration(Arc::new(Mp4MovBackend::new()?))?,
        source_registration(Arc::new(MxfBackend::new()?))?,
        source_registration(Arc::new(PcmContainerBackend::new()?))?,
    ];
    let mut identifiers = registry
        .registrations()
        .map(|registration| registration.backend().descriptor().id().as_str().to_owned())
        .collect::<BTreeSet<_>>();
    for registration in &registrations {
        let id = registration.backend().descriptor().id().as_str();
        if !identifiers.insert(id.to_owned()) {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "source backend identifier conflicts with the engine registry",
            )
            .with_context(
                ErrorContext::new("superi-engine.media", "preflight_source_backends")
                    .with_field("backend_id", id),
            ));
        }
    }
    for registration in registrations {
        registry.register(registration)?;
    }
    Ok(())
}

fn source_registration(backend: Arc<dyn MediaBackend>) -> Result<BackendRegistration> {
    BackendRegistration::new(
        backend,
        BackendCapabilities::new([BackendCapability::Source]),
        SOURCE_PRIORITY,
        BackendTier::Primary,
    )
}
