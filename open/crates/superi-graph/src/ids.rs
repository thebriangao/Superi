//! Official identifiers used by graph state and graph-facing contracts.
//!
//! The canonical value types live in `superi-core`, so every graph consumer uses
//! the same domain-distinct identity across editing, scripting, inspection, and
//! headless rendering. Graph state owns allocation and deterministic derivation
//! policies when those behaviors are introduced.

pub use superi_core::ids::{EdgeId, GraphId, NodeId, ParameterId, PortId, ResourceId};
