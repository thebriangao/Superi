//! Canonical identifiers used by editorial state.
//!
//! The value types live in `superi-core` so project state, automation, timeline
//! compilation, and persistence share one domain-distinct identity contract.

pub use superi_core::ids::{
    BinId, CaptionId, ClipId, GapId, GeneratorId, MarkerId, MediaId, ProjectId, SmartCollectionId,
    TimelineId, TrackId, TransitionId,
};
