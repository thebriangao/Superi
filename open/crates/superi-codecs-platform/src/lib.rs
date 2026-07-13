//! `superi-codecs-platform`, opt-in OS backends for encumbered codecs, enabled through the
//! `os-codecs` feature in `superi-engine`. The macOS and Windows backends provide H.264, HEVC,
//! ProRes, and AAC through native media frameworks. See `docs/codecs.md`.
//!
//! Section 5.1 in `docs/architecture.md`. Depends on: superi-core, superi-image, superi-media-io.

pub mod media_foundation;
pub mod register;
pub mod vaapi;
pub mod videotoolbox;
