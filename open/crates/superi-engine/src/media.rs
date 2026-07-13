//! Construction of the complete media backend registry used by engine consumers.

use superi_core::error::Result;
use superi_media_io::backend::BackendRegistry;

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
