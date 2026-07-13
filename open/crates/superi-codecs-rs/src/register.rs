//! Register permissive codecs as default `superi-media-io` backends.

use superi_core::error::Result;
use superi_media_io::backend::BackendRegistry;

use crate::flac::FlacBackend;
use crate::mp3::Mp3Backend;
use crate::pcm::PcmBackend;
use crate::vorbis::VorbisBackend;

/// Registers every implemented in-tree codec backend.
pub fn register_default_backends(registry: &mut BackendRegistry) -> Result<()> {
    registry.register(PcmBackend::registration()?)?;
    registry.register(Mp3Backend::registration()?)?;
    registry.register(FlacBackend::registration()?)?;
    registry.register(VorbisBackend::registration()?)
}
