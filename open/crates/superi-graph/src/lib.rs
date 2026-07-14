//! `superi-graph`, the node DAG + lazy evaluator (node-type-agnostic).
//!
//! Section 5.5 in `docs/architecture.md`. Depends on: superi-core, superi-gpu,
//! superi-image, superi-concurrency. Status: typed identifiers, node schema registration, and
//! schema discovery, and deterministic DAG storage are implemented; evaluation remains pending.

pub mod dag;
pub mod eval;
pub mod expr;
pub mod headless;
pub mod ids;
pub mod mutate;
pub mod node;
pub mod roi;
pub mod serialize;
