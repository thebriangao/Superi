//! `superi-effects`, graph-native visual effect authoring, exact editable animation, and built-in
//! visual operations.
//!
//! Section 5.10 in `docs/architecture.md`. The authoring SDK provides typed inspectable definitions,
//! editable graph-node instantiation, deterministic discovery, and exact-schema runtime factory
//! compilation. The keyframe and control modules provide exact editable animation, reusable links,
//! and parent expressions through ordinary graph driver state. The mask module provides animated
//! cubic paths, complete controls, strict persistence, and soft-coverage composition. The rotoscope
//! module keeps exact authored corrections separate from revision-fenced propagation. The built-in
//! catalog and bounded CPU reference cover common visual operations, while mask rasterization,
//! propagation solvers, transitions, text, tracking, OFX hosting, and production GPU integration
//! remain staged in their owning modules.

pub mod authoring;
pub mod catalog;
pub mod control;
pub mod keyframe;
pub mod mask;
pub mod ofx;
pub mod reference;
pub mod rotoscope;
pub mod text;
pub mod tracking;
pub mod transition;
