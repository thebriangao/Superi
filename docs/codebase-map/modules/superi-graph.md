---
module_id: superi-graph
source_paths:
  - open/crates/superi-graph
source_hash: 9736827ae67807819f2266ec6867e0dbefb82f364be5a236939d973a4bc45857
source_files: 10
mapped_at_commit: a11cecdbf19ae1de90d94324abe844db49ed0c85
---

## Purpose and ownership

`superi-graph` reserves the node-type-neutral graph boundary for DAG storage, typed node contracts, lazy evaluation, mutation, serialization, ROI propagation, expressions, and deterministic headless execution. The crate currently defines only this intended namespace and contains no graph data structure or evaluator.

## Source inventory

- `open/crates/superi-graph/Cargo.toml`: Declares dependencies on `superi-core`, `superi-gpu`, `superi-image`, and `superi-concurrency`.
- `open/crates/superi-graph/src/dag.rs`: Placeholder for nodes as GPU operations and edges as pixel flow.
- `open/crates/superi-graph/src/eval.rs`: Placeholder for lazy per-frame and per-region evaluation.
- `open/crates/superi-graph/src/expr.rs`: Placeholder for expressions and parameter links.
- `open/crates/superi-graph/src/headless.rs`: Placeholder for deterministic CLI and CI evaluation parity.
- `open/crates/superi-graph/src/lib.rs`: Documents the node-agnostic crate and publicly exports eight placeholder modules.
- `open/crates/superi-graph/src/mutate.rs`: Placeholder for mutations compiled from timeline and UI operations.
- `open/crates/superi-graph/src/node.rs`: Placeholder for node input, output, and type contracts.
- `open/crates/superi-graph/src/roi.rs`: Placeholder for region-of-interest and dirty-region propagation.
- `open/crates/superi-graph/src/serialize.rs`: Placeholder for graph serialization and deserialization.

## Public surface

The library exports `dag`, `eval`, `expr`, `headless`, `mutate`, `node`, `roi`, and `serialize`. Each module is documentation-only, so no graph, node, edge, identifier, evaluator, context, mutation, or serialization API exists.

## Architecture and data flow

There is no implemented graph data flow. The dependency graph places this crate above shared core, GPU, image, and concurrency facilities and below domain catalogs and orchestration. No source imports its declared dependencies, and downstream crates cannot construct or evaluate a graph.

## Dependencies and consumers

- Declared dependencies are `superi-core`, `superi-gpu`, `superi-image`, and `superi-concurrency`. They are unused in source.
- Direct manifest consumers are `superi-ai`, `superi-cache`, `superi-color`, `superi-effects`, `superi-timeline`, `superi-project`, and `superi-engine`.
- None of those consumers currently references a `superi_graph` Rust item. The only engine source mention is a placeholder description in its unimplemented node-wiring module.

## Invariants and operational boundaries

- Cargo enforces the generic direction: graph does not depend on `superi-color` or `superi-effects`, while those catalogs depend on graph.
- Acyclicity, typed ports, immutable evaluation, stable identifiers, determinism, ROI behavior, GPU operation ownership, and serialization compatibility are intended contracts only.
- The crate has no algorithm, persistence format, locking model, scheduler connection, or error surface.

## Tests and verification

The crate owns no tests, examples, or benchmarks. Compilation validates only the placeholder module tree and acyclic manifest relationships.

## Current status and risks

All Rust files are explicit skeletons. This is a central dependency with many declared consumers, but it currently provides no public item, so the wider manifest graph must not be interpreted as a working render graph.

## Maintenance notes

Any implementation should update this map with concrete ownership of identifiers, graph validation, mutation, evaluation, scheduling, resource lifetimes, serialization, and domain-node registration. Recheck every direct consumer because this crate's neutral public contract is intended to anchor them.
