//! `galileo-graph`, the node DAG + lazy evaluator (node-type-agnostic).
//!
//! § 5.5 in `docs/architecture.md`. Depends on: galileo-core, galileo-gpu, galileo-image, galileo-concurrency. Status: skeleton.

pub mod dag;
pub mod eval;
pub mod expr;
pub mod headless;
pub mod mutate;
pub mod node;
pub mod roi;
pub mod serialize;
