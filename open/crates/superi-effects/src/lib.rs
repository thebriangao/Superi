//! `superi-effects`, graph-native visual effect authoring, exact editable animation, and built-in
//! visual operations.
//!
//! Section 5.10 in `docs/architecture.md`. The authoring SDK provides typed inspectable definitions,
//! editable graph-node instantiation, deterministic discovery, and exact-schema runtime factory
//! compilation. The keyframe and control modules provide exact editable animation, reusable links,
//! and parent expressions through ordinary graph driver state. The mask module provides animated
//! cubic paths, complete controls, strict persistence, and soft-coverage composition. The composition
//! module retains layer parenting, reusable precompositions, explicit collapse boundaries, exact time
//! remapping, and complete nested visual paths. The rotoscope module keeps exact authored corrections
//! separate from revision-fenced propagation. The tracking module retains editable point, planar,
//! object, and calibrated camera state with manual corrections and bounded CPU reference solvers.
//! The preset module captures complete graph-native effect schemas and literal parameters, provides
//! strict integrity-checked documents, and applies explicit transactional schema migrations while
//! missing implementations remain ordinary graph placeholders.
//! The shape module provides editable cubic vector paths and exact-time visual-operation sampling.
//! The transition module provides reusable cross-dissolve and directional-wipe schemas, exact
//! handle-to-progress timing, and graph-native parameterization.
//! The text module provides editable typography, paragraph controls, exact animation, offline
//! OpenType shaping, Unicode line breaking and bidi layout, strict persistence, and inspectable
//! positioned glyphs. The spatial module provides editable 2D and 3D layer transforms, cameras,
//! lights, deterministic depth ordering, exact motion sampling, graph-native persistence, and a
//! bounded real-pixel reference renderer. The built-in catalog and bounded CPU reference cover
//! and transition semantics. The OpenFX host validates an isolated worker adapter, projects scanned
//! contexts into graph-native definitions, samples timeline and graph parameter state, and exposes
//! explicit permissions, lifecycle, recovery, and quarantine controls. Vector and mask
//! rasterization, propagation solvers, text rasterization and GPU atlases, production tracking
//! attachment and acceleration, native plugin discovery and transport, and production GPU
//! integration remain staged in their owners.

pub mod authoring;
pub mod catalog;
pub mod composition;
pub mod control;
pub mod keyframe;
pub mod mask;
pub mod ofx;
pub mod preset;
pub mod reference;
pub mod rotoscope;
pub mod shape;
pub mod spatial;
pub mod text;
pub mod tracking;
pub mod transition;
