//! `superi-graph`, the node DAG + lazy evaluator (node-type-agnostic).
//!
//! Section 5.5 in `docs/architecture.md`. Depends on: superi-core, superi-gpu,
//! superi-image, superi-concurrency. Status: typed identifiers, node schema registration, schema
//! discovery, deterministic DAG storage, typed port validation, atomic editable graph transactions,
//! and dependency invalidation are implemented; evaluation remains pending.

pub mod dag;
pub mod eval;
pub mod expr;
pub mod headless;
pub mod ids;
pub mod invalidation;
pub mod mutate;
pub mod node;
pub mod roi;
pub mod serialize;
pub mod validation;
