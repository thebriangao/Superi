//! `superi-graph`, the node DAG + lazy evaluator (node-type-agnostic).
//!
//! Section 5.5 in `docs/architecture.md`. Depends on: superi-core, superi-gpu,
//! superi-image, superi-concurrency. Status: typed identifiers, node schema registration, schema
//! discovery, deterministic DAG storage, typed port validation, atomic editable graph transactions,
//! exact dependency invalidation, region-of-interest propagation, deterministic request-scoped
//! scheduling and evaluation, typed parameter links and bounded pure expressions, deterministic
//! node introspection and cache identity, run-local evaluation timing, retained-value adapter
//! evaluation, shared interactive and headless evaluation snapshots, derived missing-node
//! placeholders, and versioned integrity-checked graph documents with migration are implemented;
//! concrete cache storage remains in `superi-cache`, while production node-catalog and render
//! integration remain pending.

pub mod dag;
pub mod diagnostics;
pub mod eval;
pub mod expr;
pub mod headless;
pub mod ids;
pub mod invalidation;
pub mod missing;
pub mod mutate;
pub mod node;
pub mod roi;
pub mod serialize;
pub mod validation;
pub mod value;
