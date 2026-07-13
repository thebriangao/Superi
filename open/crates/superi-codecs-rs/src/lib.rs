//! `superi-codecs-rs`, default backend, pure-Rust royalty-free codecs. See `docs/codecs.md`.
//!
//! The linear PCM and MP3 backends are implemented. Other codec modules remain staged for their
//! dedicated checkpoints. Section 5.1 in `docs/architecture.md`. Depends on: superi-core,
//! superi-image, superi-media-io.

pub mod av1;
pub mod flac;
pub mod mp3;
pub mod opus;
pub mod pcm;
pub mod register;
pub mod vorbis;
pub mod vp9;
