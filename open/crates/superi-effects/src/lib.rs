//! `superi-effects`, graph-native visual effect authoring, exact editable animation, and built-in
//! visual operations.
//!
//! Section 5.10 in `docs/architecture.md`. The authoring SDK provides typed inspectable definitions,
//! editable graph-node instantiation, deterministic discovery, and exact-schema runtime factory
//! compilation. The keyframe and control modules provide exact editable animation, reusable links,
//! and parent expressions through ordinary graph driver state. The mask module provides animated
//! cubic paths, complete controls, strict persistence, and soft-coverage composition. The rotoscope
//! module keeps exact authored corrections separate from revision-fenced propagation. The transition
//! module provides reusable cross-dissolve and directional-wipe schemas, exact handle-to-progress
//! timing, and graph-native parameterization. The text module provides editable typography,
//! paragraph controls, exact animation, offline OpenType shaping, Unicode line breaking and bidi
//! layout, strict persistence, and inspectable positioned glyphs. The built-in catalog and bounded
//! CPU reference cover common visual operations and transition semantics, while mask rasterization,
//! propagation solvers, text rasterization and GPU atlases, tracking, OFX hosting, and production GPU
//! integration remain staged in their owning modules.

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
