//! `superi-engine`, orchestration, the binder that wires all subsystems into one coherent engine.
//!
//! Media backend construction, decoded-frame GPU upload, complete proxy or optimized-media packet
//! generation, transparent proxy or original-source resolution, and playback-domain predictive
//! cache population are integrated while the remaining orchestration modules advance through their
//! focused checkpoints.

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
pub mod proxy_substitution;
pub mod render;
pub mod resources;
pub mod validation;
