//! Construction of the complete media backend registry used by engine consumers.

use superi_core::error::Result;
use superi_media_io::backend::BackendRegistry;
#[cfg(feature = "vendor-codecs")]
use superi_media_io::operation::OperationContext;

/// Builds the complete registry for this engine configuration and host.
///
/// The default permissive backends are always present. When `os-codecs` is enabled, platform
/// backends add only operations discovered from the current operating system.
pub fn media_backend_registry() -> Result<BackendRegistry> {
    let mut registry = BackendRegistry::new();
    superi_codecs_rs::register::register_default_backends(&mut registry)?;
    #[cfg(feature = "os-codecs")]
    superi_codecs_platform::register::register_platform_backends(&mut registry)?;
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
