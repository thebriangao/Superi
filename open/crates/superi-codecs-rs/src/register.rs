//! Register permissive codecs as default `superi-media-io` backends.

use std::collections::BTreeSet;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_media_io::backend::{BackendRegistration, BackendRegistry};

use crate::av1::Av1Backend;
use crate::flac::FlacBackend;
use crate::mp3::Mp3Backend;
use crate::opus::OpusBackend;
use crate::pcm::PcmBackend;
use crate::vorbis::VorbisBackend;
use crate::vp9::VpxBackend;

/// Builds the ordinary registry for every implemented in-tree codec backend.
///
/// # Errors
///
/// Returns a classified error if a backend cannot construct its stable registration.
pub fn default_backend_registry() -> Result<BackendRegistry> {
    let mut registry = BackendRegistry::new();
    register_default_backends(&mut registry)?;
    Ok(registry)
}

/// Registers every implemented in-tree codec backend into an existing registry.
///
/// All registrations are constructed and their stable identifiers are checked before the caller's
/// registry is modified. A conflict therefore leaves the registry unchanged.
///
/// # Errors
///
/// Returns a classified error if a backend registration cannot be built or if any default backend
/// identifier is already present.
pub fn register_default_backends(registry: &mut BackendRegistry) -> Result<()> {
    let registrations = vec![
        PcmBackend::registration()?,
        Av1Backend::registration()?,
        Mp3Backend::registration()?,
        FlacBackend::registration()?,
        VorbisBackend::registration()?,
        OpusBackend::registration()?,
        VpxBackend::registration()?,
    ];
    ensure_backend_ids_available(registry, &registrations)?;
    for registration in registrations {
        registry.register(registration)?;
    }
    Ok(())
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
                ErrorContext::new("superi-codecs-rs.register", "register_default_backends")
                    .with_field("backend_id", backend_id),
            ));
        }
    }
    Ok(())
}
