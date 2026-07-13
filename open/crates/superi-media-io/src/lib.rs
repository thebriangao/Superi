//! Codec-neutral media input, decode, encode, and container contracts.
//!
//! Sources are selected through bounded content probes rather than concrete codec types. Project
//! identity remains separate from replaceable local locations, packets retain exact presentation
//! and decode timing, and decoded video or audio retains its complete representation.
//! Decoder and encoder traits use explicit input, drain, and end-of-stream states so concrete
//! software and platform backends can share one predictable editor-facing lifecycle.

pub mod audio_io;
pub mod backend;
pub mod decode;
pub mod demux;
pub mod encode;
pub mod image_seq;
pub mod timecode;
