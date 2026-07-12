//! `superi-media-io`, decode/encode interface + container demux + image-sequence IO.
//!
//! § 5.1 in `docs/architecture.md`. Depends on: superi-core, superi-image. Status: skeleton.

pub mod audio_io;
pub mod backend;
pub mod decode;
pub mod demux;
pub mod encode;
pub mod image_seq;
pub mod timecode;
