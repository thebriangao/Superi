//! Stable public event vocabulary.

use serde::{Deserialize, Serialize};

use crate::api::MediaCapabilitiesSnapshot;
use crate::version::MEDIA_CAPABILITIES_CHANGED_EVENT;

/// One typed event carried by the ordered public API event channel.
pub trait ApiEvent {
    /// Permanent namespaced event name.
    const NAME: &'static str;
}

/// Full replacement state emitted when media capabilities change.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaCapabilitiesChanged {
    snapshot: MediaCapabilitiesSnapshot,
}

impl MediaCapabilitiesChanged {
    pub(crate) const fn new(snapshot: MediaCapabilitiesSnapshot) -> Self {
        Self { snapshot }
    }

    /// Returns the new complete capability state.
    #[must_use]
    pub const fn snapshot(&self) -> &MediaCapabilitiesSnapshot {
        &self.snapshot
    }
}

impl ApiEvent for MediaCapabilitiesChanged {
    const NAME: &'static str = MEDIA_CAPABILITIES_CHANGED_EVENT;
}
