//! `superi-engine`, orchestration, the binder that wires all subsystems into one coherent engine.
//!
//! Media backend construction, transactional timeline graph and source or decoder preparation,
//! decoded-frame GPU upload, complete proxy or optimized-media packet generation, transparent proxy
//! or original-source resolution, predictive cache population, foreground playback graph and color
//! execution, audio-device admission, audio-master A/V coordination and recovery, bounded viewport
//! handoff, coherent
//! decode, graph, delivery color, audio, and elementary-stream export orchestration, deterministic
//! subsystem lifecycle, and atomic timeline plus clip-mix edits are integrated while the remaining
//! orchestration modules advance through their focused checkpoints.

pub mod audio_mix;
pub mod av_sync;
pub mod command;
pub mod derived_media;
pub mod error;
pub mod export_jobs;
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
pub mod transport;
pub mod validation;
