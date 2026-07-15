//! `superi-engine`, orchestration, the binder that wires all subsystems into one coherent engine.
//!
//! Media backend construction, decoded-frame GPU upload, and complete proxy or optimized-media
//! packet generation are integrated while the remaining orchestration modules advance through
//! their focused checkpoints.

pub mod av_sync;
pub mod command;
pub mod derived_media;
pub mod error;
pub mod export_queue;
pub mod frame_upload;
pub mod introspection;
pub mod lifecycle;
pub mod media;
pub mod nodes;
pub mod playback;
pub mod plugins;
pub mod render;
pub mod resources;
pub mod validation;
