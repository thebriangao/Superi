---
module_id: superi-graph
source_paths:
  - open/crates/superi-graph
source_hash: 75d8de710d4f7bccdbc8318fb53a0d6736f24a7e480e234696f1aae94adf068a
source_files: 12
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-graph` owns the node-type-neutral graph boundary for DAG storage, typed node contracts,
lazy evaluation, mutation, serialization, ROI propagation, expressions, and deterministic headless
execution. Its first implemented public contract is the set of official graph-facing identifiers.
The crate does not own their value representation: `superi-core` remains the single identity owner,
while future graph state owns allocation and deterministic derivation policy.

Graph storage, validation, mutation, evaluation, serialization, and headless rendering remain
placeholders. The typed identifier surface must not be interpreted as an implemented graph or a
working render path.

## Source inventory

The module owns 12 text files:

- `open/crates/superi-graph/Cargo.toml` declares dependencies on `superi-core`, `superi-gpu`,
  `superi-image`, and `superi-concurrency`.
- `open/crates/superi-graph/src/dag.rs` is a placeholder for nodes as GPU operations and edges as
  pixel flow.
- `open/crates/superi-graph/src/eval.rs` is a placeholder for lazy per-frame and per-region
  evaluation.
- `open/crates/superi-graph/src/expr.rs` is a placeholder for expressions and parameter links.
- `open/crates/superi-graph/src/headless.rs` is a placeholder for deterministic CLI and CI
  evaluation parity.
- `open/crates/superi-graph/src/ids.rs` re-exports the six official graph-facing core identifier
  types and documents graph ownership of future allocation and derivation policy.
- `open/crates/superi-graph/src/lib.rs` documents the partial status and exports `ids` beside the
  eight placeholder modules.
- `open/crates/superi-graph/src/mutate.rs` is a placeholder for mutations compiled from timeline and
  UI operations.
- `open/crates/superi-graph/src/node.rs` is a placeholder for node input, output, and type contracts.
- `open/crates/superi-graph/src/roi.rs` is a placeholder for region-of-interest and dirty-region
  propagation.
- `open/crates/superi-graph/src/serialize.rs` is a placeholder for graph serialization and
  deserialization.
- `open/crates/superi-graph/tests/identifier_contract.rs` proves the public six-type surface,
  domain distinction, canonical text round trips, and exact type identity with `superi-core`.

## Public surface

`superi_graph::ids` publicly exposes `GraphId`, `NodeId`, `PortId`, `EdgeId`, `ParameterId`, and
`ResourceId`. Each is the same sealed 128-bit type exported by `superi_core::ids`, with canonical
lowercase `kind:32hex` text, platform-independent big-endian bytes, strict parsing, and core-owned
Serde behavior. Graph does not wrap or alias values into a second runtime identity system.

The crate also exports `dag`, `eval`, `expr`, `headless`, `mutate`, `node`, `roi`, and `serialize`.
Those modules remain documentation-only and expose no graph state, node schema, edge model,
evaluator, context, mutation, or serialization API.

## Architecture and data flow

The implemented flow is a type-level contract only:

1. `superi-core` defines and serializes every official identifier domain.
2. `superi-graph` re-exports the six domains required by graph state and graph-facing interfaces.
3. Future graph editors, scripts, timeline compilation, inspection, preview, and headless rendering
   can share one concrete identity type without conversion or duplicated state.

No graph instance currently allocates, stores, connects, mutates, evaluates, or serializes these
identifiers. The disclosed canonical reference graph in `superi-engine` uses core `NodeId` but is
not a consumer of `superi-graph` and retains string ports and edges. It remains reference behavior,
not a production graph implementation.

## Dependencies and consumers

- The implemented `ids` module uses `superi-core` directly. `superi-gpu`, `superi-image`, and
  `superi-concurrency` remain declared but unused by graph source.
- Direct manifest consumers are `superi-ai`, `superi-cache`, `superi-color`, `superi-effects`,
  `superi-timeline`, `superi-project`, and `superi-engine`.
- None of those consumers currently imports a `superi_graph` Rust item. The identifier seam is
  available and test-backed for later graph state but is not yet exercised by a graph owner.

## Invariants and operational boundaries

- Graph never defines a competing node, parameter, graph, port, edge, or resource ID type. The
  `superi-core` type and canonical wire identity remain authoritative across every consumer.
- Existing core identifier codes and relative code order remain immutable. The graph, port, edge,
  and resource domains were appended within primitive schema revision 1 without changing an
  existing representation.
- Identifier values are opaque. Future graph state owns allocation, deterministic derivation,
  uniqueness scope, and any meaning assigned to zero.
- Cargo enforces the generic dependency direction: graph does not depend on `superi-color` or
  `superi-effects`, while those catalogs depend on graph.
- Acyclicity, typed ports, immutable evaluation, deterministic state ordering, ROI behavior, GPU
  operation ownership, and serialization compatibility remain intended contracts only.
- The crate has no graph algorithm, persistence format, locking model, scheduler connection, or
  error surface yet.

## Tests and verification

The graph crate owns two integration tests in `tests/identifier_contract.rs`. They prove all six
public domains are distinct, each canonical text value parses back exactly, and every graph export
has the same Rust `TypeId` as its official core owner.

Fresh verification at mapping time ran:

```text
cargo test --manifest-path open/Cargo.toml -p superi-graph --locked
```

Both integration tests and the empty library and doc-test targets passed. Cargo reported only the
existing future-incompatibility warning for transitive crate `block v0.1.6`. No test exercises graph
state or evaluation because those behaviors do not exist.

## Current status and risks

The identifier contract is implemented and test-backed. The rest of the crate remains a central
placeholder with many declared consumers. Wider manifests must not be read as proof of a working
render graph, shared editable graph state, or preview and headless parity.

The main risks are future code inventing local identifier wrappers, attaching nondeterministic
allocation policy to the value types, or treating the re-export surface as sufficient graph-state
proof. Subsequent checkpoints must build storage and behavior on these exact IDs and exercise the
real consumer path before changing the partial status.

## Maintenance notes

Any graph implementation must update this map with concrete ownership of allocation, identifiers,
graph validation, mutation, evaluation, scheduling, resource lifetimes, serialization, and
domain-node registration. Recheck every direct consumer when this neutral public contract becomes
part of runtime state. New identifier domains must be added through `superi-core` and proved at both
the core wire boundary and graph-facing surface.
