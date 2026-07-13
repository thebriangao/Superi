//! Codec backend factory boundary.
//!
//! Registration, ranking, and fallback policy belong to the next architecture
//! layer. This module defines only the stable factories that policy selects.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::decode::{Decoder, DecoderConfig};
use crate::demux::{BackendId, MediaSource, SourceRequest};
use crate::encode::{Encoder, EncoderConfig};

/// Human-readable identity for one backend implementation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendDescriptor {
    id: BackendId,
    display_name: String,
}

impl BackendDescriptor {
    /// Creates a backend descriptor.
    pub fn new(id: BackendId, display_name: impl Into<String>) -> Result<Self> {
        let display_name = display_name.into();
        if display_name.trim().is_empty() {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "backend display name must not be empty",
            )
            .with_context(ErrorContext::new(
                "superi-media-io.backend",
                "create_backend_descriptor",
            )));
        }
        Ok(Self { id, display_name })
    }

    /// Returns stable machine identity.
    #[must_use]
    pub const fn id(&self) -> &BackendId {
        &self.id
    }

    /// Returns a diagnostic display name.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }
}

/// Codec-neutral backend factories selected by higher-level fallback policy.
pub trait MediaBackend: Send + Sync {
    /// Returns stable backend identity.
    fn descriptor(&self) -> &BackendDescriptor;

    /// Opens a source for ingest, playback, or relinking.
    fn open_source(&self, request: &SourceRequest) -> Result<Box<dyn MediaSource>>;

    /// Creates a decoder for one source stream.
    fn create_decoder(&self, config: &DecoderConfig) -> Result<Box<dyn Decoder>>;

    /// Creates an encoder for one output stream.
    fn create_encoder(&self, config: &EncoderConfig) -> Result<Box<dyn Encoder>>;
}
