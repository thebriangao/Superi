//! Stable public command vocabulary.

use serde::{Deserialize, Serialize};

use crate::api::MediaCapabilitiesSnapshot;
use crate::version::GET_MEDIA_CAPABILITIES_METHOD;

/// One typed public API command and its response contract.
pub trait ApiCommand {
    /// Successful response returned by this command.
    type Response;

    /// Permanent namespaced JSON-RPC method name.
    const METHOD: &'static str;
}

/// Structured parameters for a media capability query.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetMediaCapabilities {}

impl GetMediaCapabilities {
    /// Creates an unfiltered media capability query.
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }
}

impl ApiCommand for GetMediaCapabilities {
    type Response = GetMediaCapabilitiesResult;

    const METHOD: &'static str = GET_MEDIA_CAPABILITIES_METHOD;
}

/// Successful response to [`GetMediaCapabilities`].
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GetMediaCapabilitiesResult {
    snapshot: MediaCapabilitiesSnapshot,
}

impl GetMediaCapabilitiesResult {
    pub(crate) const fn new(snapshot: MediaCapabilitiesSnapshot) -> Self {
        Self { snapshot }
    }

    /// Returns the complete point-in-time capability state.
    #[must_use]
    pub const fn snapshot(&self) -> &MediaCapabilitiesSnapshot {
        &self.snapshot
    }
}
