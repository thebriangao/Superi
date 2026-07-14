---
module_id: superi-graph
source_paths:
  - open/crates/superi-graph
source_hash: 7e40f02c5b314252e88cc80c04ece6543875f9fd47b3cf789ecda77728225f2b
source_files: 13
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-graph` owns the node-type-neutral graph boundary for official graph-facing identifiers,
versioned node schemas, DAG storage, typed connections, lazy evaluation, mutation, serialization,
ROI propagation, expressions, and deterministic headless execution. Official instance identifiers,
node registration, and schema discovery are implemented. Graph instances, edges, mutation,
evaluation, ROI execution, expressions, serialization, and headless rendering remain explicit
placeholders.

The crate does not own identifier value representation. `superi-core` remains the single identity
owner, while future graph state owns allocation and deterministic derivation policy. Schema type
identities and schema-local names are definition metadata, separate from the core object identifiers
that address editable graph instances.

## Source inventory

- `open/crates/superi-graph/Cargo.toml`: Declares dependencies on `superi-core`, `superi-gpu`,
  `superi-image`, and `superi-concurrency`.
- `open/crates/superi-graph/src/dag.rs`: Placeholder for nodes as GPU operations and edges as pixel
  flow.
- `open/crates/superi-graph/src/eval.rs`: Placeholder for lazy per-frame and per-region evaluation.
- `open/crates/superi-graph/src/expr.rs`: Placeholder for expressions and parameter links.
- `open/crates/superi-graph/src/headless.rs`: Placeholder for deterministic CLI and CI evaluation
  parity.
- `open/crates/superi-graph/src/ids.rs`: Re-exports the six official graph-facing core identifier
  types and documents graph ownership of future allocation and derivation policy.
- `open/crates/superi-graph/src/lib.rs`: Documents the partial implementation and exports `ids`,
  `node`, and the remaining graph module tree.
- `open/crates/superi-graph/src/mutate.rs`: Placeholder for mutations compiled from timeline and UI
  operations.
- `open/crates/superi-graph/src/node.rs`: Implements typed versioned schemas, complete node behavior
  declarations, atomic registration, and immutable deterministic discovery snapshots.
- `open/crates/superi-graph/src/roi.rs`: Placeholder for region-of-interest and dirty-region
  propagation.
- `open/crates/superi-graph/src/serialize.rs`: Placeholder for graph serialization and
  deserialization.
- `open/crates/superi-graph/tests/identifier_contract.rs`: Proves the public six-type identifier
  surface, domain distinction, canonical text round trips, and exact type identity with core.
- `open/crates/superi-graph/tests/node_registry_contract.rs`: Exercises the public node-schema and
  registry contract, including reader parity and failure atomicity.

## Public surface

`superi_graph::ids` publicly exposes `GraphId`, `NodeId`, `PortId`, `EdgeId`, `ParameterId`, and
`ResourceId`. Each is the same sealed 128-bit type exported by `superi_core::ids`, with canonical
lowercase `kind:32hex` text, platform-independent big-endian bytes, strict parsing, and core-owned
Serde behavior. Graph does not wrap or alias those values into a second runtime identity system.

`superi_graph::node` exposes the schema-discovery contract:

- `NodeTypeId` and `ValueTypeId` reuse the strict core namespaced-name contract. `PortName` and
  `ParameterName` are distinct schema-local identifier types with strict lowercase canonical
  spelling. `NodeSchemaId` combines a node type and exact `SemanticVersion`.
- `PortSchema` declares a typed field and `Single`, `Optional`, or `Variadic` cardinality.
  `ParameterSchema` declares a typed parameter and whether it is animatable.
- `NodeBehavior` requires explicit `TimeBehavior`, `RoiBehavior`, `ColorRequirements`,
  `Determinism`, and `CachePolicy`. `NodeSchema` adds typed input, output, and parameter maps plus
  symbolic required capabilities.
- `NodeRegistry` registers one schema or one atomic batch. `NodeRegistrySnapshot` exposes revision,
  length, exact lookup, deterministic iteration, all versions of a node type, and latest-version
  discovery.
- Construction and registration failures use `superi_core::error::Error` with stable category,
  recoverability, component, operation, schema identity, collection, and field context where
  applicable.

The crate also exports `dag`, `eval`, `expr`, `headless`, `mutate`, `roi`, and `serialize`. Those
modules remain documentation-only. No graph instance, node instance, port instance, edge,
evaluator, mutation, persistence, plugin loader, or catalog-specific node API is implemented.

## Architecture and data flow

The instance identity flow is:

1. `superi-core` defines and serializes every official identifier domain.
2. `superi-graph` re-exports the six domains required by graph state and graph-facing interfaces.
3. Future graph editors, scripts, timeline compilation, inspection, preview, and headless rendering
   can share one concrete identity type without conversion or duplicated state.

The schema discovery flow is:

1. Node catalogs construct an immutable `NodeSchema` from validated definition identities, typed
   port and parameter declarations, complete behavior metadata, and symbolic capabilities.
2. Registration preflights the entire batch against existing and pending exact schema identities.
   A successful nonempty transaction extends one canonical `BTreeMap` and advances the registry
   revision once. A conflict or exhausted revision changes neither contents nor revision.
3. `NodeRegistry::snapshot` clones an `Arc` to the canonical map. Later registration uses
   `Arc::make_mut`, so existing snapshots retain their exact revision and contents while the
   registry copies only when a snapshot is shared.
4. Editor, script, and headless callers can clone the same `Send + Sync` snapshot and observe
   identical ordered definitions without hidden process state.

Schema discovery orders node families by canonical namespaced identity. Versions within a family
use SemVer precedence, followed by canonical version text so build-metadata variants remain distinct
and totally ordered. Input ports, output ports, and parameters each use an independent `BTreeMap`,
which preserves direction-specific namespaces and canonical field ordering.

No graph instance currently allocates, stores, connects, mutates, evaluates, or serializes the
official identifiers or registered definitions. The disclosed canonical reference graph in
`superi-engine` uses core `NodeId` but is not a consumer of `superi-graph` and retains string ports
and edges.

## Dependencies and consumers

- Implemented source uses `superi-core` for official object IDs, color-space tags, shared errors,
  semantic versions, canonical namespaced validation, and capability sets.
- `superi-gpu`, `superi-image`, and `superi-concurrency` remain declared for the future evaluator and
  are not imported by current graph source.
- Direct manifest consumers are `superi-ai`, `superi-cache`, `superi-color`, `superi-effects`,
  `superi-timeline`, `superi-project`, and `superi-engine`.
- None of those consumers currently imports a `superi_graph` Rust item. The two public integration
  test targets are the real consumers of the identifier and schema-discovery APIs.

## Invariants and operational boundaries

- Graph never defines a competing node, parameter, graph, port, edge, or resource object ID type.
  The core type and canonical wire identity remain authoritative across every consumer.
- Identifier values are opaque. Future graph state owns allocation, deterministic derivation,
  uniqueness scope, and any meaning assigned to zero.
- Graph remains below color, effects, timeline, cache, AI, project, and engine catalogs. The neutral
  identifier and schema APIs import no domain catalog and introduce no new dependency edge.
- Node type and value type definition identities are strict namespaced values. Port and parameter
  schema names are distinct types and are never normalized. Exact schema identity includes full
  SemVer build metadata.
- A schema cannot contain duplicate inputs, duplicate outputs, or duplicate parameters. An input and
  output may share the same local name because direction is represented by separate typed maps.
- Every constructible schema includes all Phase 0 metadata: schema version, typed inputs and outputs,
  typed parameters, time behavior, ROI behavior, color requirements, determinism, cache policy, and
  required capabilities.
- Registration never replaces an exact schema identity. Batch conflict checks are complete before
  mutation, empty batches are idempotent, and each successful nonempty batch advances one revision.
- Snapshots are immutable and isolated from later registrations. Discovery order cannot depend on
  hash iteration, thread timing, registration order, locale, or platform.
- Behavior declarations are metadata only. The crate still has no graph algorithm, persistence
  format, evaluator, scheduler connection, GPU operation ownership, plugin loading, or runtime
  parameter state.

## Tests and verification

`identifier_contract.rs` has two tests proving all six object-ID domains are distinct, each
canonical text value parses back exactly, and every graph export has the same Rust `TypeId` as its
official core owner.

`node_registry_contract.rs` has five tests proving strict typed definition names, complete and
inspectable schema fields, canonical port and parameter ordering, SemVer and build-metadata
discovery order, exact and latest lookup, one-revision batch registration, immutable copy-on-write
snapshots, `Send + Sync` reader sharing, editor-script-headless observation parity, actionable
duplicate errors, and failure atomicity for existing and intra-batch conflicts. It also proves
duplicate schema fields fail before registration.

Focused verification runs both integration targets through the crate's public API. Crate-wide tests,
Clippy, and rustdoc cover the library and integration targets. The complete workspace all-targets
suite exercises downstream compatibility. The repository map validator checks the source inventory
and hash, while dependency and boundary tools enforce the one-way open architecture.

## Current status and risks

Official graph-facing identifiers, node registration, and schema discovery are implemented and
test-backed. The rest of the crate is a central skeleton, so registered definitions cannot yet be
instantiated, connected, serialized, evaluated, cached, or rendered. No downstream production
catalog registers a schema yet. Capability, color, ROI, time, determinism, and cache declarations
are inspectable facts whose enforcement belongs to later graph and evaluator checkpoints.

The latest-version rule deterministically selects the lexically highest build-metadata variant when
SemVer precedence ties. Consumers that require one deployment-specific build must request its exact
`NodeSchemaId` rather than treating build metadata as environment selection.

The main integration risks are future code inventing local object-ID wrappers, confusing schema
definition identity with instance identity, attaching nondeterministic allocation policy to value
types, or treating the registered metadata as sufficient graph-state or evaluation proof.

## Maintenance notes

Keep schema identity separate from graph-instance identifiers and runtime state. New object ID
domains must be added through core and proved at both the core wire boundary and graph-facing
surface. Extend schema types only when a later checkpoint has a real consumer and proof; do not
attach evaluator factories or domain catalog behavior to the neutral registry by convenience.

Update this map when graph storage, typed connection validation, mutation, evaluation, ROI,
serialization, diagnostics, missing-node handling, or a downstream catalog becomes real. Recheck
direct consumer maps whenever they begin importing either public graph contract.
