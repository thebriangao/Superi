//! `superi-api`, the unified public automation API, the single stable surface UI/scripting/extensions/agent drive (the crate the closed tree depends on).
//!
//! § 3.5 in `docs/architecture.md`. Depends on: superi-core, superi-engine. Status: skeleton.

pub mod api;
pub mod commands;
pub mod events;
pub mod project;
pub mod recovery;
pub mod scenario;
pub mod scripting;
pub mod validation;
pub mod version;
