//! `galileo-codecs-rs`, default backend, pure-Rust royalty-free decoders. See `docs/codecs.md`.
//!
//! § 5.1 in `docs/architecture.md`. Depends on: galileo-core, galileo-image, galileo-media-io. Status: skeleton.

pub mod av1;
pub mod flac;
pub mod mp3;
pub mod opus;
pub mod register;
pub mod vorbis;
pub mod vp9;
