//! Register the opt-in OS codec backend behind `superi-media-io`.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_media_io::backend::BackendRegistry;

#[cfg(target_os = "macos")]
use crate::videotoolbox::VideoToolboxBackend;

/// Builds the ordinary platform registry for this operating system.
///
/// Platforms whose dedicated checkpoint has not landed return an empty registry rather than
/// claiming unsupported capabilities.
pub fn platform_backend_registry() -> Result<BackendRegistry> {
    let mut registry = BackendRegistry::new();
    register_platform_backends(&mut registry)?;
    Ok(registry)
}

/// Adds this operating system's implemented native codec backend atomically.
pub fn register_platform_backends(registry: &mut BackendRegistry) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let registration = VideoToolboxBackend::registration()?;
        let backend_id = registration.backend().descriptor().id().as_str();
        if registry
            .registrations()
            .any(|existing| existing.backend().descriptor().id().as_str() == backend_id)
        {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "backend identifier is already registered",
            )
            .with_context(
                ErrorContext::new(
                    "superi-codecs-platform.register",
                    "register_platform_backends",
                )
                .with_field("backend_id", backend_id),
            ));
        }
        registry.register(registration)?;
    }

    #[cfg(not(target_os = "macos"))]
    let _ = registry;

    Ok(())
}
