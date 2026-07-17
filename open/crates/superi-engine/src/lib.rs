//! `superi-engine`, orchestration, the binder that wires all subsystems into one coherent engine.
//!
//! Media backend construction, transactional project snapshot or timeline graph resource
//! preparation,
//! decoded-frame GPU upload, complete proxy or optimized-media packet generation, transparent proxy
//! or original-source resolution, predictive cache population, foreground playback graph and color
//! execution, audio-device admission, audio-master A/V coordination and recovery, bounded viewport
//! handoff, exact interactive transport, classified cross-subsystem failure propagation and
//! recovery, shared finite-resource arbitration, capability and health introspection, coherent
//! decode, graph, delivery color, audio, and elementary-stream export orchestration, bounded
//! logical export jobs, deterministic project and device lifecycle across sleep, wake, shutdown,
//! and restart, bounded typed command dispatch, contained OpenFX bundle discovery and worker
//! supervision, shared plugin availability for playback, rendering, and export, and atomic timeline
//! plus clip-mix edits are integrated while the remaining orchestration modules advance through
//! their focused checkpoints.

pub mod audio_mix;
pub mod av_sync;
pub mod command;
pub mod derived_media;
pub mod dispatcher;
pub mod error;
pub mod export_dispatch;
pub mod export_jobs;
pub mod export_queue;
pub mod frame_upload;
pub mod history;
pub mod introspection;
pub mod lifecycle;
pub mod media;
pub mod nodes;
pub mod playback;
pub mod plugins;
pub mod project_settings;
pub mod proxy_substitution;
pub mod render;
pub mod resource_arbitration;
pub mod resources;
pub mod transport;
pub mod validation;

#[cfg(feature = "test-support")]
#[doc(hidden)]
pub mod test_support;
