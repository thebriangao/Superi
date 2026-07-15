//! `superi-effects`, graph-native visual effect authoring and staged compositing foundations.
//!
//! Section 5.10 in `docs/architecture.md`. The internal authoring SDK provides typed inspectable
//! definitions, editable graph-node instantiation, deterministic discovery, and exact-schema runtime
//! factory compilation shared by timeline and node-graph callers. Concrete visual nodes, animation,
//! masks, transitions, text, tracking, and OFX hosting remain staged in their owning modules.

pub mod authoring;
pub mod keyframe;
pub mod mask;
pub mod ofx;
pub mod text;
pub mod tracking;
pub mod transition;
