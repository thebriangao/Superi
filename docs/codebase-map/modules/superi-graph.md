---
module_id: superi-graph
source_paths:
  - open/crates/superi-graph
source_hash: eb39d4dc25f901aa823f2cb96c4e609d5d2261d827477b0a7d6d7f7bbb0a3b47
source_files: 16
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-graph` owns the node-type-neutral graph boundary for official graph-facing identifiers,
versioned node schemas, deterministic DAG storage, typed connections, lazy evaluation, mutation,
serialization, ROI propagation, expressions, and deterministic headless execution. Official
instance identifiers, node registration, schema discovery, graph membership, typed port endpoints,
cycle prevention, stable inspection, topological ordering, typed input and output binding
validation, and schema-level connection compatibility are implemented.

The crate does not own identifier value representation. `superi-core` remains the single identity
owner, while graph state owns payload and connection membership. Schema type identities and
schema-local names are definition metadata, separate from the core object identifiers that address
editable graph instances.

Mutation transactions, evaluation, ROI execution, expressions, serialization, and headless
rendering remain placeholders. The implemented storage, schema, and validation surfaces must not be
interpreted as a working render path.

## Source inventory

The module owns 16 text files:

- `open/crates/superi-graph/Cargo.toml`: Declares dependencies on `superi-core`, `superi-gpu`,
  `superi-image`, and `superi-concurrency`.
- `open/crates/superi-graph/src/dag.rs`: Owns `GraphEndpoint`, `GraphEdge`, and generic
  `DirectedAcyclicGraph<N>` storage. Ordered primary and adjacency collections support checked node
  and edge insertion and removal, stable inspection, deterministic topological order, and atomic
  cycle prevention.
- `open/crates/superi-graph/src/eval.rs`: Placeholder for lazy per-frame and per-region evaluation.
- `open/crates/superi-graph/src/expr.rs`: Placeholder for expressions and parameter links.
- `open/crates/superi-graph/src/headless.rs`: Placeholder for deterministic CLI and CI evaluation
  parity.
- `open/crates/superi-graph/src/ids.rs`: Re-exports the six official graph-facing core identifier
  types and documents graph ownership of future allocation and derivation policy.
- `open/crates/superi-graph/src/lib.rs`: Documents the partial implementation and exports the
  identifier, node-schema, DAG, and validation surfaces beside the remaining module tree.
- `open/crates/superi-graph/src/mutate.rs`: Placeholder for mutations compiled from timeline and UI
  operations.
- `open/crates/superi-graph/src/node.rs`: Implements typed versioned schemas, complete node behavior
  declarations, atomic registration, and immutable deterministic discovery snapshots.
- `open/crates/superi-graph/src/roi.rs`: Placeholder for region-of-interest and dirty-region
  propagation.
- `open/crates/superi-graph/src/serialize.rs`: Placeholder for graph serialization and
  deserialization.
- `open/crates/superi-graph/src/validation.rs`: Implements pure typed input and output binding
  validation, canonical binding snapshots, structured diagnostics, and exact output-to-input schema
  compatibility without inspecting evaluator-owned payloads.
- `open/crates/superi-graph/tests/dag_contract.rs`: Proves typed deterministic storage, shared
  routing, stable topological order, direct and transitive cycle rejection, atomic failures, and
  consistent removal.
- `open/crates/superi-graph/tests/identifier_contract.rs`: Proves the public six-type identifier
  surface, domain distinction, canonical text round trips, and exact type identity with core.
- `open/crates/superi-graph/tests/node_registry_contract.rs`: Exercises the public node-schema and
  registry contract, including reader parity and failure atomicity.
- `open/crates/superi-graph/tests/port_validation_contract.rs`: Exercises successful binding
  normalization, every input failure class, terminal output failures, connection compatibility,
  opaque payload preservation, stable variadic order, and editor-script-headless parity.

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

`superi_graph::dag` exposes `GraphEndpoint`, `GraphEdge`, and `DirectedAcyclicGraph<N>`. Endpoints
combine official node and port IDs; edges add official edge identity and direction. A graph combines
official graph identity with caller-owned node payloads, typed edges, ordered incoming and outgoing
edge indexes, checked insertion and removal, direct lookup, stable whole-state inspection, and
deterministic topological ordering.

The generic payload type deliberately leaves node instance representation to the later mutation and
serialization contracts. A caller can store a schema identity, an immutable schema reference, or a
richer editable node record without coupling the topology algorithm to a catalog.

`superi_graph::validation` exposes the node-neutral runtime boundary:

- `TypedPortValue<T>` associates an exact `ValueTypeId` with evaluator-owned payload `T` without
  interpreting or constraining its concrete representation. `PortBinding<T>` groups values for one
  named port and preserves their stable graph order.
- `validate_inputs` rejects missing required inputs, invalid cardinality, wrong type tags, unknown
  ports, and duplicate binding groups as user-correctable input. `validate_outputs` applies the same
  schema checks but classifies invalid implementation output as an internal terminal failure.
- `ValidatedPortBindings<T>` contains every declared port in canonical name order. Missing optional
  and variadic ports have empty value slices, and supplied variadic values remain in graph order.
- `validate_connection` accepts only an existing source output and target input with exact
  `ValueTypeId` equality. DAG storage remains responsible for instance endpoints, connection
  counts, edge ordering, and cycle prevention.

The crate also exports placeholder `eval`, `expr`, `headless`, `mutate`, `roi`, and `serialize`
modules. They expose no evaluator, transaction, expression, ROI, serialization, or headless API.

## Architecture and data flow

The instance identity and storage flow is:

1. `superi-core` defines and serializes every official identifier domain.
2. `superi-graph` re-exports the six domains required by graph state and graph-facing interfaces.
3. A caller creates `DirectedAcyclicGraph<N>` with one `GraphId` and inserts node payloads under
   unique `NodeId` values.
4. `insert_edge` validates edge identity, both endpoint nodes, self-loops, and
   destination-to-source reachability before changing primary or adjacency collections. If the
   destination reaches the source, the proposed edge would close a directed cycle and is rejected
   with shared conflict diagnostics.
5. Successful edges enter ordered edge, incoming, and outgoing collections. Removal updates the
   same indexes, and connected nodes must be explicitly disconnected before removal.
6. Inspection reads ordered maps and sets directly. Deterministic Kahn ordering selects the smallest
   ready `NodeId`, independent of insertion order.

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

The typed validation flow is:

1. A future evaluator resolves only payload handles required by its requested subgraph and tags each
   handle with the exact graph `ValueTypeId` owned by its concrete value boundary.
2. Input or output binding groups enter the same pure validator. Groups are indexed by canonical
   `PortName`, unknown and duplicate names fail, and each declared port is checked for cardinality
   and exact type identity.
3. A successful result retains opaque payloads untouched, preserves per-port graph order, and
   normalizes declared ports into one immutable canonical map. It does not run a node, resolve an
   absent optional input, inspect a GPU resource, or mutate project state.
4. Editor, script, preview, and headless callers can use the same `Send + Sync` value contract and
   observe identical results and diagnostics without a second validation model.
5. Graph construction can call the schema-level connection check before storing an edge. Instance
   existence, edge cardinality, cycle prevention, invalidation, and scheduling remain with the DAG
   and future evaluator owners.

The DAG, registry, and validator are adjacent independent contracts. Storage retains typed `PortId`
endpoints and arbitrary node payloads, while schema compatibility uses `PortName` and exact
`ValueTypeId` metadata. No public instance model yet binds a stored `PortId` endpoint to a named
schema port, so the storage insertion boundary cannot call schema validation without inventing that
future ownership contract.

The disclosed canonical reference graph in `superi-engine` uses core `NodeId` but is not a consumer
of this store and retains string ports and edges. It remains reference behavior, not production
graph evaluation or runtime integration.

## Dependencies and consumers

- Implemented source uses `superi-core` for official object IDs, color-space tags, shared errors,
  semantic versions, canonical namespaced validation, and capability sets.
- `superi-gpu`, `superi-image`, and `superi-concurrency` remain declared for the future evaluator and
  are not imported by current graph source.
- Direct manifest consumers are `superi-ai`, `superi-cache`, `superi-color`, `superi-effects`,
  `superi-timeline`, `superi-project`, and `superi-engine`.
- None of those consumers currently imports a `superi_graph` Rust item. The four public integration
  test targets are the real consumers of identifier, schema-discovery, DAG, and validation APIs.

## Invariants and operational boundaries

- Graph never defines a competing node, parameter, graph, port, edge, or resource object ID type.
  The core type and canonical wire identity remain authoritative across every consumer.
- Identifier values are opaque. Callers own allocation, deterministic derivation, uniqueness scope,
  and any meaning assigned to zero; each graph enforces node and edge uniqueness within itself.
- Graph remains below color, effects, timeline, cache, AI, project, and engine catalogs. The neutral
  identifier, schema, DAG, and validation APIs import no domain catalog and introduce no new
  dependency edge.
- Node type and value type definition identities are strict namespaced values. Port and parameter
  schema names are distinct types and are never normalized. Exact schema identity includes full
  SemVer build metadata.
- A schema cannot contain duplicate inputs, duplicate outputs, or duplicate parameters. An input and
  output may share the same local name because direction is represented by separate typed maps.
- Every constructible schema includes all required metadata: schema version, typed inputs and
  outputs, typed parameters, time behavior, ROI behavior, color requirements, determinism, cache
  policy, and required capabilities.
- Registration never replaces an exact schema identity. Batch conflict checks are complete before
  mutation, empty batches are idempotent, and each successful nonempty batch advances one revision.
- Snapshots are immutable and isolated from later registrations. Discovery order cannot depend on
  hash iteration, thread timing, registration order, locale, or platform.
- Every graph mutation preserves acyclicity. Edge insertion rejects a self-loop or an edge whose
  destination already reaches its source, and performs all fallible checks before mutation.
- Node and edge maps plus adjacency sets are `BTreeMap` and `BTreeSet` values. Stable topological
  order uses the smallest ready node identity as its tie break.
- Port identity is retained by storage, but its mapping to schema-local names is not yet represented.
  Schema-level direction and exact type compatibility are implemented independently and await the
  instance binding contract.
- Connected nodes cannot be removed implicitly. Explicit edge removal prevents hidden graph edits
  before transaction and undo ownership exists.
- Input validation never merges duplicate binding groups. Each declared port appears exactly once
  after validation, variadic value order is preserved, and absent optional or variadic ports do not
  become evaluator work.
- Input and connection errors are user-correctable. Output schema violations are internal terminal
  failures so invalid values cannot enter caches or downstream nodes.
- Graph-level type validation compares exact `ValueTypeId` values and never inspects, coerces, or
  copies the opaque payload. The evaluator value owner remains responsible for truthful type tags.
- The crate has no persistence format, revision model, locking model, scheduler connection,
  evaluator, GPU resource ownership, plugin loading, or runtime parameter state yet.

## Tests and verification

The graph crate owns 16 integration tests across four files. The two identifier tests prove all six
public domains are distinct, each canonical text value parses back exactly, and every graph export
has the same Rust `TypeId` as its official core owner.

Five node-registry tests prove strict typed definition names, complete and inspectable schema fields,
canonical port and parameter ordering, SemVer and build-metadata discovery order, exact and latest
lookup, one-revision batch registration, immutable copy-on-write snapshots, `Send + Sync` reader
sharing, editor-script-headless observation parity, actionable duplicate errors, and failure
atomicity for existing and intra-batch conflicts. They also prove duplicate schema fields fail
before registration.

Four DAG tests prove deterministic equality across insertion orders, typed endpoints, shared
routing, stable topological order, duplicate and missing-identity errors, direct and transitive
cycle prevention, failure atomicity, and consistent explicit removal.

Five port-validation tests prove canonical binding snapshots, empty optional and variadic
normalization, stable variadic order, opaque payload retention, missing, unknown, duplicate,
cardinality, and type error diagnostics, caller-correctable input classification, terminal
implementation-output classification, exact connection type compatibility, `Send + Sync` validated
snapshots, and identical editor-script-headless results.

Focused verification runs all four integration targets through the crate's public API. Crate-wide
tests, strict Clippy, and rustdoc cover the library and integration targets. The complete workspace
suite exercises downstream compatibility. The repository map validator checks the source inventory
and hash, while dependency and boundary tools enforce the one-way open architecture. No test
exercises graph evaluation because that behavior does not exist.

## Current status and risks

Official graph-facing identifiers, node registration, schema discovery, deterministic DAG storage,
typed binding validation, and schema-level output-to-input compatibility are implemented and
test-backed. Registered definitions can be selected and topology can be edited and inspected, but
no public instance model currently binds a schema or named schema port to graph payloads and typed
`PortId` endpoints. The crate cannot serialize, evaluate, cache, or render its state, and no
downstream production catalog registers a schema or consumes the DAG.

The latest-version rule deterministically selects the lexically highest build-metadata variant when
SemVer precedence ties. Consumers that require one deployment-specific build must request its exact
`NodeSchemaId` rather than treating build metadata as environment selection.

Linear reachability and topological ordering are chosen for auditable correctness and may need
measured optimization for very large interactive graphs. Subsequent checkpoints must extend the
single checked storage boundary, neutral registry, and pure validator rather than creating
competing topology, identity, schema, or validation systems.

Other integration risks are attaching nondeterministic allocation policy to value types, claiming a
type tag proves its concrete payload representation, or treating validated bindings and registered
metadata as sufficient evaluation proof.

## Maintenance notes

Preserve the single checked mutation boundary for node and edge membership. New validation must run
before collection changes or provide an explicit transaction rollback proof. Keep schema and catalog
knowledge out of the DAG algorithm, retain deterministic collection and tie-break behavior, and
benchmark before replacing the linear reachability check.

Keep schema identity separate from graph-instance identifiers and runtime state. New object ID
domains must be added through core and proved at both the core wire boundary and graph-facing
surface. Extend schema types only when a later checkpoint has a real consumer and proof; do not
attach evaluator factories or domain catalog behavior to the neutral registry by convenience.

Update this map when the instance-to-schema port binding, edge-cardinality validation, mutation,
evaluation, ROI, serialization, missing-node handling, or a downstream catalog becomes real.
Recheck direct consumer maps whenever they begin importing any public graph contract.
