---
module_id: superi-graph
source_paths:
  - open/crates/superi-graph
source_hash: 7ce0387e9421a4e80051a3440d58252a2897fc1ed20c5e9edbc4d995412580da
source_files: 21
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-graph` owns the node-type-neutral graph boundary for official graph-facing identifiers,
versioned node schemas, deterministic DAG storage, typed connections, lazy evaluation, mutation,
serialization, ROI propagation, expressions, and deterministic headless execution. Official
instance identifiers, node registration, schema discovery, graph membership, typed port endpoints,
cycle prevention, stable inspection, topological ordering, typed input and output binding
validation, schema-level connection compatibility, editable node instances, runtime parameter
state, atomic revisioned mutation transactions, exact dirty-region algebra, deterministic
dependency invalidation planning, lazy request-scoped evaluation, snapshot-bound
region-of-interest propagation, and exact requested-versus-dirty work intersection are
implemented.

The crate does not own identifier value representation. `superi-core` remains the single identity
owner, while graph state owns payload and connection membership. Schema type identities and
schema-local names are definition metadata, separate from the core object identifiers that address
editable graph instances.

Cache generation integration, editable-snapshot-to-evaluator binding, scheduling, expressions,
serialization, and explicit headless integration remain absent or placeholders. The implemented
storage, schema, validation, mutation, invalidation, generic evaluator, and ROI planning surfaces
must not be interpreted as a working production render path.

## Source inventory

The module owns 21 text files:

- `open/crates/superi-graph/Cargo.toml`: Declares dependencies on `superi-core`, `superi-gpu`,
  `superi-image`, and `superi-concurrency`.
- `open/crates/superi-graph/src/dag.rs`: Owns `GraphEndpoint`, `GraphEdge`, and generic
  `DirectedAcyclicGraph<N>` storage. Ordered primary and adjacency collections support checked node
  and edge insertion and removal, stable immutable inspection, narrow payload mutation,
  deterministic topological order, and atomic cycle prevention.
- `open/crates/superi-graph/src/eval.rs`: Implements exact endpoint, rational-frame, and pixel-region
  requests, node-declared incoming dependencies, request-local reuse, canonical dependency pulls,
  one shared stateless evaluator, and structured failure context.
- `open/crates/superi-graph/src/expr.rs`: Placeholder for expressions and parameter links.
- `open/crates/superi-graph/src/headless.rs`: Placeholder for deterministic CLI and CI evaluation
  parity.
- `open/crates/superi-graph/src/ids.rs`: Re-exports the six official graph-facing core identifier
  types and documents graph ownership of future allocation and derivation policy.
- `open/crates/superi-graph/src/invalidation.rs`: Owns exact normalized dirty-region sets,
  requested-work clipping, immutable invalidation seeds and plans, stable topological dependency
  propagation, identity-region convenience, edge-aware mapping, and structured failure context.
- `open/crates/superi-graph/src/lib.rs`: Documents the partial implementation and exports the
  identifier, node-schema, DAG, validation, mutation, invalidation, evaluator, and ROI surfaces
  beside the remaining module tree.
- `open/crates/superi-graph/src/mutate.rs`: Implements complete schema-bound editable node
  instances, opaque typed parameters, immutable graph snapshots, optimistic revisions, and ordered
  atomic add, remove, connect, disconnect, reorder, and parameter transactions.
- `open/crates/superi-graph/src/node.rs`: Implements typed versioned schemas, complete node behavior
  declarations, atomic registration, and immutable deterministic discovery snapshots.
- `open/crates/superi-graph/src/roi.rs`: Owns exact requested output regions, per-output regions of
  definition, built-in and custom node mapping, dependency-only upstream propagation, immutable
  snapshot stamping, stable evaluation order, and invalidation intersection.
- `open/crates/superi-graph/src/serialize.rs`: Placeholder for graph serialization and
  deserialization.
- `open/crates/superi-graph/src/validation.rs`: Implements pure typed input and output binding
  validation, canonical binding snapshots, structured diagnostics, and exact output-to-input schema
  compatibility without inspecting evaluator-owned payloads.
- `open/crates/superi-graph/tests/dag_contract.rs`: Proves typed deterministic storage, shared
  routing, stable topological order, direct and transitive cycle rejection, atomic failures, and
  consistent removal.
- `open/crates/superi-graph/tests/evaluation_contract.rs`: Proves demand-only pulls, physical-time
  and exact-region request identity, at-most-once request-local work, canonical dependency order,
  temporal requests, caller parity, and structured evaluator failures.
- `open/crates/superi-graph/tests/identifier_contract.rs`: Proves the public six-type identifier
  surface, domain distinction, canonical text round trips, and exact type identity with core.
- `open/crates/superi-graph/tests/invalidation_contract.rs`: Proves exact dirty-region unions and
  clipping, stable dependency propagation, edge-specific mapping and branch stopping, actionable
  errors, clean-node exclusion, mutation-snapshot integration, and editor-script-headless parity
  across insertion histories.
- `open/crates/superi-graph/tests/mutation_contract.rs`: Proves all six mutation forms, exact
  instance binding, typed parameters and connections, input cardinality, immutable snapshots,
  deterministic state, revision conflicts, explicit removal, cycle safety, and full rollback.
- `open/crates/superi-graph/tests/node_registry_contract.rs`: Exercises the public node-schema and
  registry contract, including reader parity and failure atomicity.
- `open/crates/superi-graph/tests/port_validation_contract.rs`: Exercises successful binding
  normalization, every input failure class, terminal output failures, connection compatibility,
  opaque payload preservation, stable variadic order, and editor-script-headless parity.
- `open/crates/superi-graph/tests/roi_contract.rs`: Proves pass-through pruning, exact repeated
  region union, per-source full-frame domains, checked expansion and clipping, custom per-input
  mapping, structured failures, invalidation intersection, immutable snapshot stamping, stable
  dependency order, and editor-script-headless parity across insertion histories.

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

The generic payload type keeps topology independent of node representation. The mutation owner now
stores its complete `EditableNode<T>` payload through this interface, while other callers may still
use a schema identity or a separate payload without coupling the DAG algorithm to a catalog.

`superi_graph::eval` exposes the node-neutral lazy evaluation contract:

- `EvaluationKey` and `EvaluationRequest` identify one stored output endpoint at an exact
  `RationalTime` and signed half-open `PixelBounds`. Physical-time equality makes equivalent
  timebase representations the same request-local work.
- `EvaluateNode<V>` declares only the incoming edge, frame, and region dependencies required for
  one output, then evaluates from immutable `EvaluationContext` inputs. Its default requests every
  stored incoming edge at the current frame and region.
- `LazyEvaluator` validates each declared edge against the authoritative DAG, canonicalizes and
  deduplicates declarations, recursively resolves source endpoints, and evaluates each identical
  key at most once during one call.
- `EvaluationResult<V>` owns only values reached by the pull and exposes the requested value,
  stable dependency-completion keys, and request-local lookup without requiring `V: Clone`.
- Every call starts with an empty value set. No persistent cache, graph revision, dirty-region
  propagation, scheduler, catalog lookup, or caller mode participates.

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

`superi_graph::mutate` exposes the editable state boundary:

- `InstancePort` binds one stable `PortId` to one exact input or output `PortName`.
  `EditableParameter<T>` binds `ParameterId` and `ParameterName` to one
  `TypedParameterValue<T>`. `EditableNode<T>` requires a complete one-to-one binding against an
  immutable `NodeSchema` and rejects unknown, missing, duplicate, cross-direction, or mistyped
  state before graph insertion.
- `GraphMutation<T>` represents add, remove, connect, disconnect, presentation reorder, and typed
  parameter replacement. `GraphTransaction<T>` retains ordered mutations and the exact revision
  they expect.
- `EditableGraph<T>` applies nonempty transactions to a cloned candidate, publishes one new
  revision only after every mutation succeeds, and rejects stale revisions. Empty current-revision
  transactions are idempotent.
- `GraphSnapshot<T>` shares one immutable `Arc` state containing the checked DAG and explicit node
  presentation order. Processing order remains the DAG's deterministic topological order.
- Connect resolves stored instance ports to schema names, reuses `validate_connection`, enforces
  target `Single` and `Optional` cardinality, and then enters the checked DAG boundary. Remove stays
  explicit: incident edges must be disconnected earlier in the same transaction or a prior one.
- Mutation failures preserve their original shared error classification and add stable graph,
  expected revision, mutation index, and mutation code context.

`superi_graph::invalidation` exposes the derived invalidation boundary:

- `DirtyRegion` identifies full-frame or exact half-open `PixelBounds` work. `DirtyRegionSet`
  stores a canonical exact union as deterministic, nonoverlapping finite rectangles, with full-frame
  subsumption and clipping to requested output work.
- `InvalidationSeed` identifies changed output on one stored node. `InvalidatedNode` and
  `InvalidationPlan` expose only affected nodes, once each, in the DAG's stable topological order.
- `propagate_dependency_invalidation` preserves finite region identity across dependencies that
  share one coordinate space. `propagate_invalidation_with` supplies exact graph and edge identity
  to a caller-owned deterministic mapper, so transformed or custom ROI behavior can map or stop a
  branch without entering the neutral DAG algorithm.
- Missing seed nodes fail before mapping with shared not-found diagnostics. Mapper failures retain
  their category and recoverability while gaining graph, edge, source, and destination context.
- Plans are immutable derived values over a borrowed graph snapshot. They own no project mutation,
  cache state, evaluator state, or scheduler state.

`superi_graph::roi` exposes the derived required-work boundary:

- `RoiDomains` records one finite region of definition for every output endpoint reached in an
  evaluation context. Duplicate endpoint declarations fail instead of silently replacing meaning.
- `RoiRequest` identifies one exact output endpoint and a `DirtyRegionSet` of requested work.
  Reusing the invalidation algebra preserves irregular coverage and full-frame meaning without a
  competing region type.
- `propagate_roi` handles `FullFrame`, `InputBounds`, and checked `Expanded` node behavior.
  `propagate_roi_with` also invokes a deterministic `CustomRoiMapper` with the exact immutable node
  and requested output-port map, then validates every returned input identity.
- `RoiPlan` stamps graph identity and editable revision, exposes required input and output endpoint
  regions in stable order, and lists only required nodes in dependency-first topological order.
- `RoiPlan::invalidated_output_work` intersects requested endpoint work with an existing
  `InvalidationPlan`, excluding clean nodes and preserving clean gaps.
- Missing nodes, wrong-direction requests, absent domains, overflow, missing custom mapping, and
  invalid custom output use shared actionable diagnostics without mutating graph state.

The crate also exports placeholder `expr`, `headless`, and `serialize` modules. They expose no
expression, serialization, or explicit headless API.

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

The lazy evaluation flow is:

1. A caller passes the same immutable `DirectedAcyclicGraph<N>` used for graph inspection plus one
   output endpoint, rational frame, and pixel region request.
2. The target payload receives its incoming stored edges in stable `EdgeId` order and declares only
   the input work needed for that output. A declaration may select a branch or request another
   source frame or region, but it cannot name routing outside an incoming stored edge.
3. The evaluator sorts declarations by edge, physical frame, region, and stable time
   representation, removes equal requests, then recursively pulls each source endpoint.
4. A request-local value list reuses equal endpoint, physical-frame, and exact-region keys. Resolved
   inputs retain their declaration, stored edge, source key, and borrowed opaque value.
5. The node evaluates once after all declared inputs complete. Errors retain their classification
   and gain graph, endpoint, frame, region, operation, and route context.
6. The returned result owns the requested value and every reached request-local value in stable
   completion order. A later call starts empty, so it cannot reuse stale data after an edit.

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

1. The evaluator resolves only payloads required by its request and leaves truthful `ValueTypeId`
   tagging to the concrete value owner that integrates schema validation.
2. Input or output binding groups enter the same pure validator. Groups are indexed by canonical
   `PortName`, unknown and duplicate names fail, and each declared port is checked for cardinality
   and exact type identity.
3. A successful result retains opaque payloads untouched, preserves per-port graph order, and
   normalizes declared ports into one immutable canonical map. It does not run a node, resolve an
   absent optional input, inspect a GPU resource, or mutate project state.
4. Editor, script, preview, and headless callers can use the same `Send + Sync` value contract and
   observe identical results and diagnostics without a second validation model.
5. Graph construction can call the schema-level connection check before storing an edge. Instance
   existence, edge cardinality, and cycle prevention now belong to the mutation and DAG owners;
   invalidation planning is derived after published changes, while schema integration and
   scheduling remain future owners.

The mutation transaction flow is:

1. A caller constructs each `EditableNode<T>` against one immutable exact schema. Complete ordered
   maps bind instance port and parameter IDs to schema-local names, and initial parameters retain
   opaque payloads behind exact `ValueTypeId` tags.
2. The caller captures the latest immutable graph snapshot and sends one ordered transaction with
   that expected revision. Add and reorder use explicit presentation positions; processing order
   continues to come from topology.
3. `EditableGraph::apply` rejects a stale revision, checks revision capacity, and clones the shared
   state into a private candidate. Every mutation then sees prior mutations from the same batch.
4. Connect resolves source and target instance ports, calls the pure schema validator, checks the
   candidate target connection count, and calls checked DAG insertion. Parameter replacement uses a
   narrow mutable payload lookup on the candidate DAG and rechecks its schema type.
5. Any failure adds the ordered mutation index and code, then discards the candidate. A successful
   nonempty batch publishes one new `Arc` state and advances exactly one revision, while every older
   snapshot keeps its exact state.
6. Editor, script, and headless callers clone the same `GraphSnapshot<T>` and observe identical
   typed nodes, parameters, edges, visual order, and topological order without a second model.

The dependency invalidation flow is:

1. A caller supplies one or more `InvalidationSeed` values against one immutable
   `DirectedAcyclicGraph` snapshot. Every seed node is validated before edge mapping begins, and
   empty regions become no work.
2. Finite dirty rectangles are normalized by exact coordinate strips. Overlap is removed without
   replacing irregular unions with bounding boxes, and full-frame dirtiness subsumes finite bounds.
3. The planner walks the DAG's stable topological order. Each affected source is presented once to
   its outgoing edges in stable `EdgeId` order, converging branches merge exactly, and clean or
   disconnected nodes never enter the plan.
4. The identity convenience copies dirty coverage only when dependencies share one coordinate
   space. The edge-aware path gives a caller the immutable graph and typed edge so node transforms
   can return exact mapped work or stop a branch.
5. Evaluators can call `requested_work` to intersect a node's invalidated output with one requested
   `PixelBounds`, preserving only required work. Editor, script, and headless callers receive the
   same public plan for equal snapshots, seeds, and deterministic edge mapping.

The region-of-interest flow is:

1. A caller supplies one immutable `GraphSnapshot<T>`, current `RoiDomains`, and one or more exact
   output `RoiRequest` values. Every request is validated as an output on that snapshot before any
   custom node mapping begins.
2. Requests are clipped to their output regions of definition and merged through `DirtyRegionSet`,
   preserving exact nonrectangular coverage. Empty requests become no work.
3. The planner walks reverse stable topological order. `InputBounds` passes requested coverage,
   `Expanded` applies checked symmetric pixel growth, and `FullFrame` resolves each connected
   source's own region of definition rather than inventing one global frame.
4. `Custom` behavior receives the exact immutable node and requested work by output `PortId`. Its
   returned input map is validated against the node instance before any dependency work is added.
5. Each connected input maps through its exact stored edge to an upstream output and is clipped to
   that output's region of definition. Repeated and converging work merges exactly, while unrelated,
   unconnected, and empty branches remain absent.
6. The plan filters forward topological order to required nodes, stamps graph ID and revision, and
   can intersect each required output with a node-level `InvalidationPlan` without filling clean
   gaps or taking cache ownership.

The mutation layer is the integration contract across the DAG, registry, and validator. It binds
stored `PortId` endpoints to `PortName`, exact schemas, and `ValueTypeId` compatibility without
adding catalog knowledge to topology. The invalidation planner derives work directly from the same
checked DAG exposed by each immutable `GraphSnapshot`. The ROI planner consumes the same snapshot,
schema behavior, typed edges, and exact region algebra to derive upstream work. The generic
evaluator resolves caller-owned DAG payloads but has no production binding from `EditableNode<T>`.
Production evaluation integration, persistence, cache generations, undo history, and engine
transaction coordination remain separate later owners.

The disclosed canonical reference graph in `superi-engine` uses core `NodeId` but is not a consumer
of this store and retains string ports and edges. It remains reference behavior, not production
graph evaluation or runtime integration.

## Dependencies and consumers

- Implemented source uses `superi-core` for official object IDs, color-space tags, shared errors,
  semantic versions, canonical namespaced validation, and capability sets.
- `superi-gpu`, `superi-image`, and `superi-concurrency` remain declared for later concrete
  evaluation integration and are not imported by current graph source. The implemented generic
  evaluator uses only core values plus graph-owned storage and payload behavior.
- Direct manifest consumers are `superi-ai`, `superi-cache`, `superi-color`, `superi-effects`,
  `superi-timeline`, `superi-project`, and `superi-engine`.
- None of those consumers currently imports a `superi_graph` Rust item. The eight public
  integration test targets are the real consumers of identifier, schema-discovery, DAG,
  validation, mutation, invalidation, evaluation, and ROI APIs.

## Invariants and operational boundaries

- Graph never defines a competing node, parameter, graph, port, edge, or resource object ID type.
  The core type and canonical wire identity remain authoritative across every consumer.
- Identifier values are opaque. Callers own allocation, deterministic derivation, uniqueness scope,
  and any meaning assigned to zero; each graph enforces node and edge uniqueness within itself.
- Graph remains below color, effects, timeline, cache, AI, project, and engine catalogs. The neutral
  identifier, schema, DAG, validation, mutation, invalidation, evaluator, and ROI APIs import no
  domain catalog and introduce no new dependency edge.
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
- Every editable node binds all schema inputs, outputs, and parameters exactly once. Input and output
  IDs cannot overlap within one node, and every initial or replacement parameter retains the exact
  declared `ValueTypeId` without exposing its payload representation.
- Stored connections resolve source outputs and target inputs through those exact bindings. Single
  and optional inputs accept at most one stored edge; variadic inputs retain stable edge identity
  order through the DAG adjacency set.
- Connected nodes cannot be removed implicitly. A transaction must disconnect incident edges before
  remove, which keeps the full ordered edit explicit without claiming undo ownership.
- Every transaction compares one expected graph revision. Empty current-revision batches are
  idempotent, successful nonempty batches advance once, stale or exhausted revisions publish
  nothing, and any mutation failure discards all earlier candidate edits.
- Presentation order is explicit and independent of deterministic topological processing order.
  Equivalent explicit transactions produce equal snapshots regardless of insertion history.
- Graph snapshots are immutable `Arc` views. A later transaction cannot change a prior reader's
  nodes, parameters, edges, presentation order, topology, or revision.
- Input validation never merges duplicate binding groups. Each declared port appears exactly once
  after validation, variadic value order is preserved, and absent optional or variadic ports do not
  become evaluator work.
- Input and connection errors are user-correctable. Output schema violations are internal terminal
  failures so invalid values cannot enter caches or downstream nodes.
- Graph-level type validation compares exact `ValueTypeId` values and never inspects, coerces, or
  copies the opaque payload. The evaluator value owner remains responsible for truthful type tags.
- Dirty-region normalization never replaces an irregular union with a bounding rectangle. Finite
  regions remain exact half-open `PixelBounds`, clean gaps remain clean, and full-frame state is
  represented explicitly rather than guessed from finite coordinates.
- Invalidation validates every seed before mapping, includes each affected node once in stable
  topological order, maps outgoing dependencies in stable edge order, merges converging coverage
  exactly, and excludes clean and disconnected nodes.
- Identity-region propagation is valid only for dependencies in one coordinate space. Node-specific
  transforms and custom ROI behavior must use the edge-aware mapping seam and a deterministic
  mapper to retain editor, script, and headless parity.
- An invalidation plan is derived from one immutable graph snapshot and contains no authoritative
  editable state, cache generations, scheduler state, or hidden process state.
- Evaluation resolves only node-declared incoming routes and never scans the whole DAG by default.
  Declarations are canonicalized before work, equal request keys execute once per call, and the
  immutable graph borrow is the evaluation snapshot boundary.
- Request-local reuse is not persistent caching and does not consume an invalidation plan
  automatically. A new call starts empty and no dirty region, graph revision, timing record,
  scheduler decision, or caller-specific path is hidden in evaluator state.
- ROI validates all authored requests before custom mapping, walks nodes and edges deterministically,
  and records only nonempty connected work. Unrelated graph branches cannot enter the plan.
- Full-frame ROI resolves each connected source's declared output domain. Input-bound and expanded
  work is clipped to the same per-endpoint domain, and expansion never saturates coordinate
  overflow.
- Custom ROI output is implementation-owned and must name exact input `PortId` values on the
  immutable node. Invalid implementation output is terminal and cannot enter the derived plan.
- Every ROI plan retains its source graph ID and editable revision, contains no mutable graph,
  invalidation, cache, scheduler, or payload state, and is identical across reader roles for equal
  snapshots, domains, requests, and deterministic custom mapping.
- The crate has no persistence format, locking owner, scheduler connection, editable-snapshot
  evaluator binding, GPU resource ownership, plugin loading, undo history, cache generation owner,
  or engine transaction coordinator yet.

## Tests and verification

The graph crate owns 47 integration tests across eight files. The two identifier tests prove all six
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

Six mutation tests prove complete schema instance bindings, all six ordered operations, typed
parameters and connections, target cardinality, explicit disconnect plus remove, presentation and
topological order separation, stale revision handling, immutable old snapshots, identical editor,
script, and headless sharing, equivalent deterministic state, cycle safety, and full rollback after
failures in the middle of a candidate batch.

Nine invalidation tests prove exact dirty-region union decomposition, clean-gap preservation,
full-frame subsumption, empty-region handling, requested-work clipping, stable topological
dependency order, clean-node exclusion, exact converging merges, edge-specific transforms, branch
stopping, unknown-node and mapper diagnostics, and identical editor-script-headless plans across
different node and edge insertion histories. The integration proof derives those plans from the
same immutable editable graph snapshot published by the mutation owner.

Eight evaluation tests prove default incoming pulls, fresh observation after an editable payload
change, lazy branch selection, skipped failure branches, exact frame and region keys, reuse across
physically equal timebase representations, distinct temporal and spatial work, stable declaration
normalization, insertion-independent values and traces, editor-script-headless caller parity through
one evaluator, missing targets, invalid node-declared routes, and preserved node failure
classification with request context.

Eight ROI tests prove pass-through dependency pruning, exact repeated region union, per-source
full-frame domains, checked expansion and clipping, coordinate-overflow rejection, custom per-input
mapping, invalid mapper diagnostics, wrong-direction request rejection, invalidation intersection,
snapshot revision stamping, stable dependency order, and identical editor-script-headless plans
across different insertion histories.

Focused verification runs all eight integration targets through the crate's public API. Crate-wide
tests, strict Clippy, and rustdoc cover the library and integration targets. The complete workspace
suite exercises downstream compatibility. The repository map validator checks the source inventory
and hash, while dependency and boundary tools enforce the one-way open architecture. No test yet
connects evaluation to a production node catalog, GPU value, engine, CLI, or rendered artifact.

## Current status and risks

Official graph-facing identifiers, node registration, schema discovery, deterministic DAG storage,
typed binding validation, schema-level output-to-input compatibility, complete schema-bound node
instances, editable parameters, immutable snapshots, and revisioned atomic mutation transactions
are implemented and test-backed beside exact dirty-region sets, deterministic dependency
invalidation planning, and snapshot-bound ROI propagation. Registered definitions can be
instantiated, topology and visual order can be edited, exact state can be shared across reader
roles, and callers can derive both dirty and requested work from the same published DAG snapshot.
Lazy request-scoped evaluation is also implemented and test-backed, so caller-owned evaluator
payloads can resolve stored topology. No production binding makes `EditableNode<T>` an evaluator
node or connects ROI and invalidation plans to evaluator requests yet. The crate cannot serialize,
persistently cache, schedule, or render production values, and no downstream production catalog
consumes the mutation, invalidation, ROI, or evaluation owner.

The latest-version rule deterministically selects the lexically highest build-metadata variant when
SemVer precedence ties. Consumers that require one deployment-specific build must request its exact
`NodeSchemaId` rather than treating build metadata as environment selection.

Linear reachability and topological ordering are chosen for auditable correctness and may need
measured optimization for very large interactive graphs. Transactions currently clone the editable
state before applying a batch, which favors atomic auditability over large-graph edit throughput and
must be benchmarked before replacement. Subsequent checkpoints must extend the single checked
storage and mutation boundary, neutral registry, pure validator, and shared evaluator rather than
creating competing topology, identity, schema, validation, revision, or caller-specific execution
systems.

Request-local value lookup is linear to preserve physical-time equality without inventing a second
time key. It is deterministic and bounded by reached work, but must be measured before large graph
or temporal-window optimization. Persistent cache keys and cache generation integration belong to
later owners.

Other integration risks are attaching nondeterministic allocation policy to value types, claiming a
type tag proves its concrete payload representation, treating mutable editor order as evaluation
order, or treating validated editable state as sufficient evaluation proof. Invalidation-specific
risks are using identity mapping across a transform, providing a nondeterministic edge mapper, or
treating a derived plan as cache generation state.
Evaluation-specific risks are treating node-declared regions as completed ROI propagation or
treating generic evaluation as production graph, GPU, headless, or render proof.
ROI-specific risks are supplying stale regions
of definition, implementing nondeterministic custom mapping, or reusing a plan after its stamped
graph revision has changed.

## Maintenance notes

Preserve the transaction as the public editable-state boundary and the DAG as its checked topology
owner. New validation must run on the private candidate before publication, every error must retain
its ordered mutation context, and failed batches must leave both state and revision unchanged. Keep
schema and catalog knowledge out of the DAG algorithm, retain deterministic collections and tie
breaks, and benchmark before replacing reachability checks or full-state candidate cloning.

Keep schema identity separate from graph-instance identifiers and runtime state. New object ID
domains must be added through core and proved at both the core wire boundary and graph-facing
surface. Extend schema types only when a later checkpoint has a real consumer and proof; do not
attach evaluator factories or domain catalog behavior to the neutral registry by convenience.

Keep invalidation pure over an immutable DAG snapshot. Preserve exact half-open dirty coverage,
stable topological and edge ordering, preflight seed validation, and the caller-owned mapping seam.
Do not move node-specific ROI policy into the generic DAG or treat full frame as an invented finite
extent.

Keep dependency declaration and execution in the same shared evaluator for every caller. Add
persistent reuse only with revisioned cache keys and invalidation proof, and add scheduling without
changing the semantic completion order.

Keep ROI pure over one immutable editable snapshot. Preserve per-output regions of definition,
exact region-set union, checked expansion, strict custom input validation, dependency-only reverse
traversal, forward topological result order, and graph revision stamping. Do not create an
editor-specific, script-specific, or headless-specific propagation path.

Update this map when mutation, invalidation, ROI, and evaluation integrate, cache generations,
scheduling, serialization, expressions, missing-node handling, undo ownership, engine coordination,
or a downstream catalog becomes real. Recheck direct consumer maps whenever they begin importing
any public graph contract.
