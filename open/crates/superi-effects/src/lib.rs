//! `superi-effects`, graph-native visual effect authoring, exact editable animation, and built-in
//! visual operations.
//!
//! Section 5.10 in `docs/architecture.md`. The authoring SDK provides typed inspectable definitions,
//! editable graph-node instantiation, deterministic discovery, and exact-schema runtime factory
//! compilation. The keyframe module provides exact editable animation. The built-in catalog and
//! bounded CPU reference cover common visual operations, while masks, transitions, text, tracking,
//! OFX hosting, and production GPU integration remain staged in their owning modules.

pub mod authoring;
pub mod catalog;
pub mod keyframe;
pub mod mask;
pub mod ofx;
pub mod reference;
pub mod text;
pub mod tracking;
pub mod transition;
