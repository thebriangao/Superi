//! `superi-engine`, orchestration, the binder that wires all subsystems into one coherent engine.
//!
//! § 5.13 in `docs/architecture.md`. Depends on: all T0-T4 crates (+ superi-codecs-platform via os-codecs). Status: skeleton.

pub mod av_sync;
pub mod command;
pub mod error;
pub mod export_queue;
pub mod introspection;
pub mod lifecycle;
pub mod nodes;
pub mod playback;
pub mod plugins;
pub mod render;
pub mod resources;
pub mod validation;
