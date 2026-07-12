//! `superi-codecs-platform`, opt-in OS backend for encumbered codecs (H.264/H.265/H.266/ProRes/AAC), MIT binding code only; enabled via the `os-codecs` feature in `superi-engine`. See `docs/codecs.md`.
//!
//! § 5.1 in `docs/architecture.md`. Depends on: superi-core, superi-image, superi-media-io. Status: skeleton.

pub mod media_foundation;
pub mod register;
pub mod vaapi;
pub mod videotoolbox;
