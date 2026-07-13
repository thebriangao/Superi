//! Register installed operating-system codec backends behind `superi-media-io`.

use std::collections::BTreeSet;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_media_io::backend::{BackendRegistration, BackendRegistry};

#[cfg(target_os = "windows")]
use crate::media_foundation::MediaFoundationBackend;
#[cfg(target_os = "macos")]
use crate::videotoolbox::VideoToolboxBackend;

/// Builds a registry containing only platform operations available on this host.
pub fn platform_backend_registry() -> Result<BackendRegistry> {
    let mut registry = BackendRegistry::new();
    register_platform_backends(&mut registry)?;
    Ok(registry)
}

/// Discovers and registers this operating system's native codec backends atomically.
pub fn register_platform_backends(registry: &mut BackendRegistry) -> Result<()> {
    let registrations = platform_registrations()?;
    ensure_backend_ids_available(registry, &registrations)?;
    for registration in registrations {
        registry.register(registration)?;
    }
    Ok(())
}

fn platform_registrations() -> Result<Vec<BackendRegistration>> {
    #[cfg(target_os = "macos")]
    return Ok(vec![VideoToolboxBackend::registration()?]);
    #[cfg(target_os = "windows")]
    return Ok(MediaFoundationBackend::registration()?
        .into_iter()
        .collect());
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    Ok(Vec::new())
}

fn ensure_backend_ids_available(
    registry: &BackendRegistry,
    registrations: &[BackendRegistration],
) -> Result<()> {
    let mut backend_ids = registry
        .registrations()
        .map(|registration| registration.backend().descriptor().id().as_str().to_owned())
        .collect::<BTreeSet<_>>();
    for registration in registrations {
        let backend_id = registration.backend().descriptor().id().as_str();
        if !backend_ids.insert(backend_id.to_owned()) {
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
    }
    Ok(())
}
