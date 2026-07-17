---
module_id: superi-graph
source_paths:
  - open/crates/superi-graph
source_hash: 68e83e97dc61e16f7ef7fc1fe33b9fcd156e05b0436f2c3809aeac590c9c7523
source_files: 32
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-graph` owns the node-type-neutral graph boundary for official graph-facing identifiers,
versioned node schemas, deterministic DAG storage, typed connections, lazy evaluation, mutation,
serialization, ROI propagation, expressions, and deterministic headless execution. Official
graph values retain exact domain-owned payloads beside finite scalar, vector, color, matrix,
Boolean, and bounded choice processing state without importing a concrete catalog. Official
instance identifiers, node registration, schema discovery, graph membership, typed port endpoints,
cycle prevention, stable inspection, topological ordering, typed input and output binding
validation, schema-level connection compatibility, editable node instances, runtime parameter
state, atomic revisioned mutation transactions, exact dirty-region algebra, deterministic
dependency invalidation planning, snapshot-bound region-of-interest propagation, exact
requested-versus-dirty work intersection, lazy request-scoped evaluation, deterministic work
scheduling, reusable bounded scalar expression programs, typed parameter links, graph-bound pure
expressions, parameter dependency-cycle
protection, deterministic parameter evaluation with caller-projected literal payloads, and derived
missing-node resolution against an immutable schema registry are implemented. Deterministic
pre-execution node introspection,
policy-scoped cache keys, actionable non-cacheable decisions, run-local evaluator timing, and a
node-neutral retained-value adapter are also implemented without changing semantic result equality.
Cached evaluation checks a final target before node execution, prunes complete prerequisite
subtrees at intermediate hits, and exposes graph lineage plus exact work identity to caller-owned
storage. One immutable role-neutral evaluation snapshot retains the exact editable graph revision,
compiles caller-owned runtime node payloads without changing topology, and delegates both
interactive and headless work to the shared stateless evaluator with or without a caller-owned
cache. Semantic graph edit invalidation compares two immutable revisions, expands parameter-driver
dependencies, and propagates roots through both prior and current topology. Cached work from an
evaluation snapshot carries stable graph and revision lineage so its external owner can invalidate
precisely without adding a revision to semantic result keys. Versioned deterministic graph
documents preserve exact schemas,
typed editable nodes, literal parameters, parameter drivers, presentation order, edges, graph
identity, and optimistic revision across strict serialization, checked deserialization, integrity
validation, and explicit legacy migration.

The crate does not own identifier value representation. `superi-core` remains the single identity
owner, while graph state owns payload and connection membership. Schema type identities and
schema-local names are definition metadata, separate from the core object identifiers that address
editable graph instances.

Concrete cache storage, exact byte and frame admission, precise edit cleanup, and bounded
background render dispatch remain owned by `superi-cache`; invalidation invocation,
ROI-plan evaluator orchestration, project storage, and a production runtime node catalog remain
absent or placeholders. The
implemented storage, schema, validation, mutation, invalidation, ROI planning, scheduling, cache
identity, diagnostics, document, parameter evaluator, compiler seam, and shared interactive and
headless evaluator surfaces must not be interpreted as a working production render path or atomic
project save system.

## Source inventory

The module owns 32 text files:

- `open/crates/superi-graph/Cargo.toml`: Declares dependencies on `superi-core`, `superi-gpu`,
  `superi-image`, `superi-concurrency`, Serde, JSON, and SHA-256 hashing.
- `open/crates/superi-graph/src/dag.rs`: Owns `GraphEndpoint`, `GraphEdge`, and generic
  `DirectedAcyclicGraph<N>` storage. Ordered primary and adjacency collections support checked node
  and edge insertion and removal, stable immutable inspection, narrow payload mutation,
  deterministic topological order, and atomic cycle prevention.
- `open/crates/superi-graph/src/diagnostics.rs`: Owns canonical node-state fingerprints, exact
  schema and behavior introspection, versioned evaluation cache keys, explicit cacheability
  decisions, deterministic plan inspection, monotonic node timings, and diagnostic evaluation
  reports that retain the ordinary evaluator result.
- `open/crates/superi-graph/src/eval.rs`: Implements exact endpoint, rational-frame, and pixel-region
  requests, node-declared incoming dependencies, request-local reuse, canonical dependency pulls,
  inspectable deterministic ready batches, pre-execution diagnostic inspection, scheduled and timed
  evaluation through one shared stateless executor, graph-owned `EvaluationCacheIdentity` values
  with stable graph and optional published-revision lineage,
  the `EvaluationValueCache<V>` adapter, final-frame short circuiting, intermediate-node dependency
  pruning, cacheable insertion, and structured failure context.
- `open/crates/superi-graph/src/expr.rs`: Owns the reusable `ScalarExpression` bounded ASCII program,
  graph-local parameter addresses and exact typed references, direct and expression drivers,
  editable source plus checked postfix instructions, finite deterministic arithmetic, and the
  domain-owned payload conversion seam. The scalar layer accepts an explicit variable allowlist;
  `ParameterExpression` adds complete graph binding validation without duplicating the parser.
- `open/crates/superi-graph/src/headless.rs`: Owns the caller-supplied runtime node compiler seam
  and immutable `GraphEvaluationSnapshot<T, N>`. Compilation retains the exact editable snapshot,
  exposes that full snapshot to every node compilation so authored parameter drivers remain
  visible, preserves checked node and edge identity, adds exact revision and schema failure context,
  and delegates scheduling, inspection, ordinary evaluation, cached evaluation with the exact
  editable revision, and diagnostic evaluation to the shared lazy evaluator.
- `open/crates/superi-graph/src/ids.rs`: Re-exports the six official graph-facing core identifier
  types and documents graph ownership of future allocation and derivation policy.
- `open/crates/superi-graph/src/invalidation.rs`: Owns exact normalized dirty-region sets,
  requested-work clipping, immutable invalidation seeds and plans, stable topological dependency
  propagation, semantic revision-to-revision `GraphEditInvalidation` derivation, parameter-driver
  expansion, prior and current topology union, identity-region convenience, edge-aware mapping, and
  structured failure context.
- `open/crates/superi-graph/src/lib.rs`: Documents the partial implementation and exports the
  identifier, node-schema, DAG, diagnostics, validation, mutation, invalidation, evaluator, ROI,
  parameter link, expression, neutral graph value, shared evaluation snapshot, missing-node
  resolution, and graph document surfaces beside the remaining module tree.
- `open/crates/superi-graph/src/missing.rs`: Derives exact schema availability from one immutable
  editable graph snapshot and one registry snapshot, retains unavailable nodes as inspectable
  placeholders over their original typed state, rejects incompatible same-identity definitions,
  and provides a deterministic degraded evaluation gate without changing authored state.
- `open/crates/superi-graph/src/mutate.rs`: Implements complete schema-bound editable node
  instances, opaque typed parameters, canonical authored driver state, immutable graph snapshots,
  optimistic revisions, ordered atomic add, remove, connect, disconnect, reorder, parameter, set
  driver, and clear driver transactions, dependency validation and cycle rejection, and shared
  request-local parameter evaluation with an optional higher-domain literal projection callback.
- `open/crates/superi-graph/src/node.rs`: Implements typed versioned schemas, complete node behavior
  declarations, atomic registration, immutable deterministic discovery snapshots, and
  `GraphColorMetadata`, which preserves the image-owned pipeline across graph boundaries.
- `open/crates/superi-graph/src/roi.rs`: Owns exact requested output regions, per-output regions of
  definition, built-in and custom node mapping, dependency-only upstream propagation, immutable
  snapshot stamping, stable evaluation order, and invalidation intersection.
- `open/crates/superi-graph/src/serialize.rs`: Owns the strict versioned graph document envelope,
  canonical JSON encoding, SHA-256 payload integrity, complete schema and editable-state records,
  checked reconstruction, explicit legacy migration, and canonical upgraded bytes.
- `open/crates/superi-graph/src/validation.rs`: Implements pure typed input and output binding
  validation, canonical binding snapshots, structured diagnostics, and exact output-to-input schema
  compatibility without inspecting evaluator-owned payloads.
- `open/crates/superi-graph/src/value.rs`: Owns exact finite binary64 values, bounded discrete
  choices, and the neutral generic `GraphValue<T>` payload. Domain values remain lossless, typed
  processing variants remain inspectable and serializable, and expressions accept only explicit
  scalar values without coercion.
- `open/crates/superi-graph/tests/dag_contract.rs`: Proves typed deterministic storage, shared
  routing, stable topological order, direct and transitive cycle rejection, atomic failures, and
  consistent removal.
- `open/crates/superi-graph/tests/cache_evaluation_contract.rs`: Proves final-frame hits stop the
  whole pull, intermediate-node hits prune complete prerequisite work, semantic results equal the
  uncached evaluator, separate retention roles remain distinct, disabled work bypasses storage, and
  failed outputs are retried instead of retained as successful frames.
- `open/crates/superi-graph/tests/diagnostics_contract.rs`: Proves deterministic pre-execution
  inspection, static, per-frame, and per-region cache-key scope, physical-time normalization,
  state-lineage invalidation, unreachable-edit isolation, explicit non-cacheable propagation,
  shared editor-script-headless results, run-local timing, failure context, and thread-safe public
  values.
- `open/crates/superi-graph/tests/edit_invalidation_contract.rs`: Proves parameter edits expand
  through driver targets and pixel descendants, disconnections and removals retain old-topology
  descendants, presentation-only and skipped forward revisions are deterministic, and invalid graph
  or revision pairs fail without guessing.
- `open/crates/superi-graph/tests/evaluation_contract.rs`: Proves demand-only pulls, physical-time
  and exact-region request identity, at-most-once request-local work, canonical dependency order,
  deterministic ready batches, distinct-edge input meaning, exact invalidated request clipping,
  temporal requests, caller parity, and structured evaluator failures.
- `open/crates/superi-graph/tests/expression_contract.rs`: Proves the reusable scalar language
  independently of graph bindings, checked inspectable expression source, canonical instructions
  and variables, language bounds, finite arithmetic, typed links and expressions, deterministic
  transitive results, driver replacement and clearing, immutable snapshots, cycle and dependency
  rejection, explicit node-removal cleanup, transaction rollback, and editor-script-headless
  parameter parity. It also proves projected evaluation visits only reached undriven literals,
  preserves graph dependency order and driver semantics, retains identity evaluation, and adds
  exact projection failure context.
- `open/crates/superi-graph/tests/headless_contract.rs`: Proves one `Send + Sync` role-neutral
  evaluation snapshot, snapshot-owned linked parameter compilation, exact invalidated lazy work,
  skipped unused failure branches, equal editor, script, and headless schedules, inspections,
  cache decisions, diagnostic results, caller-owned retained evaluation, immutable old revision
  behavior, fresh results after editable mutation, and classified compiler errors with exact graph,
  revision, node, and schema context.
- `open/crates/superi-graph/tests/identifier_contract.rs`: Proves the public six-type identifier
  surface, domain distinction, canonical text round trips, and exact type identity with core.
- `open/crates/superi-graph/tests/invalidation_contract.rs`: Proves exact dirty-region unions and
  clipping, stable dependency propagation, edge-specific mapping and branch stopping, actionable
  errors, clean-node exclusion, mutation-snapshot integration, and editor-script-headless parity
  across insertion histories.
- `open/crates/superi-graph/tests/missing_node_contract.rs`: Proves graph documents load without
  plugin discovery, unavailable nodes retain exact schemas, bindings, parameters, edges, and
  canonical bytes, edits remain possible while unavailable, incompatible registrations fail
  closed, exact schemas restore evaluation, and editor-script-headless callers observe identical
  state, blocker diagnostics, and results.
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
- `open/crates/superi-graph/tests/serialization_contract.rs`: Proves deterministic current-format
  round trips, canonical normalization, lossless legacy migration, integrity and compatibility
  rejection, checked semantic reconstruction, interrupted-document rejection, and equal editor,
  script, and headless evaluation from independently loaded graph documents through the public
  graph evaluation snapshot rather than a private executable DAG bridge.
- `open/crates/superi-graph/tests/value_contract.rs`: Proves exact finite binary64 retention,
  validated serialization for every neutral variant, bounded choices, lossless domain payloads,
  and scalar-only expression conversion without implicit coercion.

## Public surface

`superi_graph::ids` publicly exposes `GraphId`, `NodeId`, `PortId`, `EdgeId`, `ParameterId`, and
`ResourceId`. Each is the same sealed 128-bit type exported by `superi_core::ids`, with canonical
lowercase `kind:32hex` text, platform-independent big-endian bytes, strict parsing, and core-owned
Serde behavior. Graph does not wrap or alias those values into a second runtime identity system.

`superi_graph::value` exposes the catalog-neutral editable payload contract:

- `FiniteF64` rejects nonfinite state and retains exact binary64 bits, including signed zero,
  through equality and serialization.
- `ChoiceValue` retains one nonempty UTF-8 choice bounded to 256 bytes and validates decoded state.
- `GraphValue<T>` preserves exact domain-owned state in `Domain(T)` beside scalar, vector, color,
  matrix, Boolean, and choice processing variants. Checked constructors, accessors, and stable
  value-kind inspection keep variants explicit.
- `ExpressionParameterValue` is implemented only for the scalar variant. Boolean, choice, color,
  vector, matrix, and domain values never acquire numeric meaning through implicit conversion.

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
- `GraphColorMetadata` owns one exact `ColorPipelineMetadata` snapshot for graph state, can append a
  validated transform stage, and exposes or returns the unchanged image-owned pipeline.

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
  timebase representations the same request-local work. `EvaluationKey` also provides a total order
  consistent with that equality for deterministic planning and completion indexes.
- `EvaluateNode<V>` declares only the incoming edge, frame, and region dependencies required for
  one output, then evaluates from immutable `EvaluationContext` inputs. Its default requests every
  stored incoming edge at the current frame and region.
- `LazyEvaluator` validates each declared edge against the authoritative DAG, canonicalizes and
  deduplicates declarations, discovers only reached source work, and builds one request-local work
  graph before value evaluation.
- `EvaluationSchedule` publishes stable `EvaluationBatch` readiness waves and unique prerequisite
  keys. Equal-ready work uses the exact work-key order, while distinct input edges remain separate
  `EvaluationInput` values even when they reuse one completed source key.
- `LazyEvaluator::schedule` exposes the current diagnostic schedule without evaluating values.
  `LazyEvaluator::evaluate` always builds and executes one private current schedule so a separately
  inspected schedule cannot become hidden reusable state.
- `EvaluationCacheEntryKind` distinguishes the top-level final frame from prerequisite intermediate
  nodes. `EvaluationCacheIdentity` pairs stable graph identity, an optional published editable
  revision, the available graph lineage key, and the exact evaluator work key. This allows a higher
  owner to fence stale graph and node lineage and compose physical time plus outer result context
  without moving those concerns into graph. Direct immutable DAG evaluation is conservatively
  unversioned, while `GraphEvaluationSnapshot` supplies its exact revision. `EvaluationValueCache<V>`
  is the graph-owned, storage-neutral lookup and insertion seam; concrete key composition,
  synchronization, memory placement, and policy remain above graph.
- `LazyEvaluator::evaluate_with_cache` builds the same private plan and deterministic inspection as
  the uncached path. It checks the target final key before selecting work, recursively stops at
  retained intermediate keys, executes only remaining dependencies, and stores clones only for
  `Available` identities. Cacheable evaluation requires `V: Clone`; callers are expected to use
  cloneable resource handles rather than duplicate resource contents.
- `LazyEvaluator::inspect` builds deterministic node metadata and cache-key decisions from that
  same private plan before values execute. `LazyEvaluator::evaluate_with_diagnostics` executes the
  same plan through the ordinary value loop and returns its unchanged result beside run-local
  timing.
- `EvaluationResult<V>` owns only values reached by the pull and exposes the requested value, the
  exact semantic schedule, stable value completion keys, and request-local lookup. Ordinary
  evaluation requires no clone; cached evaluation includes only retained or freshly executed
  values actually needed after pruning.
- Ordinary evaluation starts with an empty value set. Cached evaluation receives its store
  explicitly from the caller, and neither path hides graph revision, dirty-region propagation,
  outer job priority, worker pool, catalog lookup, or caller mode in evaluator state.

`superi_graph::headless` exposes the shared editable-to-executable evaluation boundary:

- `NodeCompiler<T, N>` receives the complete immutable `GraphSnapshot<T>`, one stable `NodeId`, and
  its `EditableNode<T>`. Higher-tier catalogs use the snapshot to resolve authored parameter drivers
  and produce caller-owned runtime payloads without adding any catalog dependency to graph.
- Closures with the same signature implement the compiler trait directly. Compilation failures
  retain their classification and gain graph ID, editable revision, node ID, and exact schema ID
  context.
- `GraphEvaluationSnapshot<T, N>` owns the exact source `GraphSnapshot<T>` beside a runtime
  `DirectedAcyclicGraph<N>`. Compilation visits nodes and edges in stable identity order and
  replaces only node payloads, so topology and dependency meaning cannot diverge.
- `editable_snapshot`, `graph_id`, and `graph_revision` keep authored state inspectable.
  `schedule`, `inspect`, `evaluate`, `evaluate_with_cache`, and `evaluate_with_diagnostics`
  delegate directly to `LazyEvaluator`, giving editor, script, interactive, playback, and headless
  roles one request-scoped path rather than caller-specific execution algorithms.
- A later edit requires a newly compiled snapshot. Older evaluation snapshots retain their exact
  immutable revision, and semantic cache keys reflect the newly compiled node state. The compiler
  seam does not itself provide a production catalog, concrete cache, worker dispatch, or rendered
  value.

`superi_graph::diagnostics` exposes the graph-specific inspection and timing contract:

- `NodeStateFingerprint` hashes caller-supplied canonical editable state under a versioned domain.
  `NodeIntrospection` binds that fingerprint to one exact `NodeSchemaId` and `NodeBehavior`, and
  `IntrospectNode` supplies the current value to diagnostic planning.
- `EvaluationCacheKey` is a versioned SHA-256 graph identity component over graph and output identity, exact schema,
  node state, declared behavior, policy-relevant canonical physical time and region, and every
  ordered incoming route plus upstream cache key. It intentionally excludes the whole graph
  revision so unrelated edits do not invalidate unchanged reached work.
- `CacheKeyStatus` distinguishes an available key from policy-disabled, nondeterministic, and
  dependency-blocked work. A dependent without a complete upstream lineage names the first
  blocking `EdgeId` in canonical dependency order instead of publishing a partial key.
- `NodeInspection` and `EvaluationInspection` are deterministic pre-execution values. `NodeTiming`
  records one monotonic implementation-call duration, while `EvaluationDiagnostics` keeps planning,
  execution, and node timing outside semantic inspection.
- `EvaluationReport<V>` pairs those diagnostics with the exact ordinary `EvaluationResult<V>` from
  the shared executor. A diagnostic-path node failure preserves shared error classification and
  request context, then adds schema, state fingerprint, cache decision, key or blocking edge, and
  elapsed nanoseconds.
- Graph identity does not own media content, evaluated parameter, color-pipeline, render-setting,
  concrete stored value, eviction, resource budget, persistence, or invalidation-generation state.
  `superi-cache` composes the graph adapter inputs with its complete outer identity before memory
  or versioned disk retention. Its memory adapter owns exact hierarchical budgets, automatic
  priority-aware pressure handling, strict per-tier LRU victim selection, and precise edit
  invalidation. Generation cleanup and persistent directory lifecycle stay with later cache and
  orchestration checkpoints.

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

`superi_graph::expr` exposes the node-neutral parameter dependency contract:

- `ParameterAddress` combines one stable `NodeId` and its node-local `ParameterId`.
  `ParameterReference` adds the exact source `ValueTypeId` expected by the author.
- `ParameterDriver` stores either one lossless typed direct link or one `ParameterExpression`, and
  exposes dependencies in canonical parameter-address and type order.
- `ScalarExpression::compile` accepts editable ASCII source plus an explicit allowed-variable set.
  It retains only referenced variables in canonical order and exposes checked postfix instructions
  and resolver-based finite evaluation for higher domain owners that do not need graph addresses.
- `ParameterExpression::compile` adds explicit named typed graph bindings around that scalar
  program. Its bounded pure language supports finite decimal constants, parentheses, unary
  negation, and addition, subtraction, multiplication, and division, with no I/O, mutation,
  functions, loops, recursion, or host script escape.
- A successful expression retains trimmed editable source, variables in canonical name order, and
  checked postfix `ExpressionInstruction` values. Missing, duplicate, and unused bindings, invalid
  syntax, excessive source, instruction count, or nesting, and nonfinite constants fail before
  authored graph state exists.
- `ExpressionParameterValue` is the catalog-owned conversion seam between opaque typed payloads and
  finite scalar expression arithmetic. Direct links never convert their payload.

`superi_graph::mutate` exposes the editable state boundary:

- `InstancePort` binds one stable `PortId` to one exact input or output `PortName`.
  `EditableParameter<T>` binds `ParameterId` and `ParameterName` to one
  `TypedParameterValue<T>`. `EditableNode<T>` requires a complete one-to-one binding against an
  immutable `NodeSchema` and rejects unknown, missing, duplicate, cross-direction, or mistyped
  state before graph insertion.
- `GraphMutation<T>` represents add, remove, connect, disconnect, presentation reorder, typed
  parameter replacement, driver set or replacement, and driver clearing. `GraphTransaction<T>`
  retains ordered mutations and the exact revision they expect.
- `EditableGraph<T>` applies nonempty transactions to a cloned candidate, publishes one new
  revision only after every mutation succeeds, and rejects stale revisions. Empty current-revision
  transactions are idempotent.
- `GraphSnapshot<T>` shares one immutable `Arc` state containing the checked DAG and explicit node
  presentation order plus authored parameter drivers in canonical target order. Processing order
  remains the DAG's deterministic topological order.
- Connect resolves stored instance ports to schema names, reuses `validate_connection`, enforces
  target `Single` and `Optional` cardinality, and then enters the checked DAG boundary. Remove stays
  explicit: incident edges must be disconnected earlier in the same transaction or a prior one.
- Mutation failures preserve their original shared error classification and add stable graph,
  expected revision, mutation index, and mutation code context.
- `GraphSnapshot::evaluate_parameter` resolves literals, direct links, and expressions from that
  exact immutable state. `GraphSnapshot::evaluate_parameter_with` keeps the same graph-owned
  traversal but lets a higher domain project only reached undriven literals into another
  `ExpressionParameterValue` result domain. The graph preserves the literal `ValueTypeId`, exact
  link and expression meaning, cycle invariant, request-local memo, and dependency-completion order.
- `ParameterEvaluation<T>` returns the typed result and unique parameters in deterministic
  dependency-completion order, with no caller mode or persistent cache.

`superi_graph::missing` exposes the derived plugin-availability boundary:

- `resolve_graph` compares every authored node's exact embedded `NodeSchema` with one immutable
  `NodeRegistrySnapshot`. It returns a `GraphResolution<T>` containing the unchanged
  `GraphSnapshot<T>`, registry revision, and canonical `NodeId`-ordered availability state.
- `NodeAvailability::Available` requires both exact `NodeSchemaId` identity and structural schema
  equality. An absent identity produces `MissingNodeReason::UnregisteredSchema`, while a
  same-identity definition with different fields produces `IncompatibleSchema` and fails closed.
- `MissingNodePlaceholder` records the stable node identity, saved schema identity, and reason.
  `ResolvedNode` pairs that derived state with the original `EditableNode<T>`, so callers can still
  inspect exact ports, parameters, behavior, capabilities, drivers, and connected graph state.
- `GraphResolution::require_evaluable` returns the exact authored snapshot when every node is
  available. Otherwise it returns one `Unavailable` and `Degraded` shared error with graph and
  registry revisions plus every blocker in canonical order. Editing, serialization, and later
  registry resolution remain possible after that evaluation result.
- Availability is never serialized, migrated, or written into the graph. Registering the exact
  saved schema in a later registry snapshot restores availability without a graph transaction or
  document rewrite.

`superi_graph::serialize` exposes the editable graph document boundary:

- `GRAPH_DOCUMENT_FORMAT_REVISION` identifies the current strict envelope revision.
  `serialize_graph` accepts one immutable `GraphSnapshot<T>` and emits deterministic canonical JSON
  when `T` is serializable.
- The envelope identifies `superi.graph`, records its format and core primitive schema revisions,
  and includes a SHA-256 digest of the canonical payload. Unknown fields, unsupported future
  revisions, malformed checksums, truncated bytes, and noncanonical identities fail explicitly.
- The payload preserves the graph identity and optimistic revision, every complete `NodeSchema`,
  typed node, instance port and parameter binding, opaque JSON parameter payload, presentation
  order, typed edge, and authored parameter driver. Semantically ordered collections are
  canonicalized independently of input insertion and JSON object-key order.
- `deserialize_graph` verifies the envelope before reconstructing all schemas and editable nodes
  through their checked constructors. It publishes nodes and edges through one `GraphTransaction`,
  then restores the persisted revision only after the complete state validates.
- `GraphLoad<T>` returns the checked graph, source revision, migration state, and canonical current
  document. Revision zero has one explicit legacy envelope and migrates losslessly to revision one;
  unknown revisions are never guessed.
- The codec owns editable graph meaning, not files, SQLite, locking, autosave, backup selection, or
  crash-safe project replacement. Those durability mechanisms remain `superi-project` concerns.

`superi_graph::invalidation` exposes the derived invalidation boundary:

- `DirtyRegion` identifies full-frame or exact half-open `PixelBounds` work. `DirtyRegionSet`
  stores a canonical exact union as deterministic, nonoverlapping finite rectangles, with full-frame
  subsumption and clipping to requested output work.
- `InvalidationSeed` identifies changed output on one stored node. `InvalidatedNode` and
  `InvalidationPlan` expose only affected nodes, once each, in the DAG's stable topological order.
- `GraphEditInvalidation` identifies one graph and revision interval plus direct semantic roots and
  every affected node in stable identity order. `derive_edit_invalidation` compares node schemas,
  ports, literal parameters, edges, and parameter drivers, expands changed parameter addresses
  through both revisions' driver dependency graphs, and propagates roots through both prior and
  current pixel topology. Presentation order is excluded, equal or skipped-forward revisions are
  deterministic, and mismatched, reversed, or same-revision divergent pairs fail explicitly.
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

The shared editable-value flow is:

1. A graph-owning domain selects `GraphValue<T>` as its one editable payload and wraps all existing
   domain state in `Domain(T)` without translation.
2. Higher-tier processing catalogs instantiate schema parameters with explicit scalar, vector,
   color, matrix, Boolean, or bounded choice variants while topology and mutation remain generic.
3. Finite numeric constructors preserve exact binary64 bits and reject NaN or infinity before state
   can enter a snapshot or serialized document. Choice construction applies its byte bound at the
   same boundary.
4. Parameter links retain the exact variant. Expression evaluation accepts and produces only
   `Scalar`, so no other processing or domain value is silently coerced.
5. Timeline compilation uses this neutral wrapper to retain native editorial values and admit
   shared processing nodes in the same graph. Effects remains the catalog owner above graph.

The lazy evaluation flow is:

1. A caller passes the same immutable `DirectedAcyclicGraph<N>` used for graph inspection plus one
   output endpoint, rational frame, and pixel region request.
2. The target payload receives its incoming stored edges in stable `EdgeId` order and declares only
   the input work needed for that output. A declaration may select a branch or request another
   source frame or region, but it cannot name routing outside an incoming stored edge.
3. Discovery sorts declarations by edge, physical frame, region, and stable time representation,
   removes equal declarations, validates every route, and recursively records only reached source
   work. No node value is evaluated during this planning pass.
4. Equal endpoint, physical-frame, and exact-region keys identify one planned work unit. A separate
   canonical input list retains every distinct declared edge and source key, so two edges may reuse
   one source value without losing their input meaning.
5. The planner derives unique prerequisite sets and repeatedly publishes every currently ready key
   as one ordered `EvaluationBatch`. Dependencies always occur in earlier batches, and equal-ready
   work is ordered independently of insertion history or thread timing.
6. Inspection walks readiness batches before value execution. Every reached payload supplies its
   exact schema, behavior, and canonical editable-state fingerprint. Cache keys propagate through
   complete upstream key lineage, while disabled, nondeterministic, or blocked work remains
   explicitly non-cacheable.
7. Key encoding reduces physical rational time so equal coordinates in different timebases remain
   equal, includes exact region only for per-region policy, and excludes the whole graph revision.
   Stable graph, node, port, edge, schema, state, behavior, and upstream identities still invalidate
   every materially changed lineage.
8. Evaluation walks those published batches and uses a request-local completion index. Each node
   evaluates once after all declared inputs complete. Ordinary evaluation records no timing;
   diagnostic evaluation times the same implementation calls with a monotonic clock. Errors retain
   classification and request context, then gain introspection and elapsed-time context only on the
   diagnostic path.
9. Cached evaluation gives the same plan and inspection to one caller-owned
   `EvaluationValueCache<V>`. Each available graph key is paired with stable graph identity,
   optional published revision, and its exact work key before the adapter is called. An available
   final identity is checked first. After a final miss, recursive work selection stops at available
   intermediate hits, so those values and their complete prerequisite subtrees do not execute.
   Remaining values use the ordinary node contract and are inserted into the final or intermediate
   role only after successful evaluation.
10. The returned result owns the requested value, exact semantic schedule, and every retained or
    freshly executed value actually needed in stable completion order. A diagnostic report retains
    the ordinary result unchanged and keeps semantic inspection separate from run-local planning,
    execution, and node durations. Outer render coordinators may map readiness onto bounded workers
    under their own policy without changing this schedule's dependency meaning.

The shared interactive and headless evaluation flow is:

1. An editor, script, playback owner, export owner, or headless owner captures one immutable
   `GraphSnapshot<T>` from the editable graph transaction boundary.
2. A higher-tier `NodeCompiler<T, N>` receives the complete immutable graph snapshot beside every
   schema-bound editable node in stable identity order and produces its runtime `EvaluateNode<V>`
   payload. This keeps snapshot-owned parameter drivers visible during compilation. A failure
   discards the local projection and gains exact source revision and schema context.
3. `GraphEvaluationSnapshot<T, N>` retains the source snapshot and inserts compiled payloads under
   the same node IDs, then copies the already checked edges in stable order. No alternate routing,
   presentation state, or revision is invented.
4. Every caller inspects the same editable snapshot and invokes `schedule`, `evaluate`, or
   `evaluate_with_cache` on this role-neutral value. All methods call `LazyEvaluator` directly, so
   dependency discovery, work identity, readiness, cache identity, and semantic results are
   identical across roles.
5. ROI and invalidation remain derived from the retained editable snapshot. Their exact requested
   regions can enter evaluation unchanged. A new edit compiles a new evaluation snapshot, while
   old readers retain the old immutable result boundary and cannot observe partial mutation.
6. `superi-cache` implements memory and persistent retained-value adapters for separate final and
   intermediate tiers, plus strict memory LRU reclamation and precise edit invalidation. Eviction,
   invalidation, and persistence failures remain transparent to graph: removed or unavailable
   values become ordinary misses and recompute through this same evaluator. Cache render
   orchestration now inspects target identity before bounded dispatch, drives this snapshot through
   layered memory and disk adapters, and stages background inserts until cooperative completion.
   `superi-effects` now implements a higher-tier authoring catalog, exact-schema `NodeCompiler`
   adapter, strict animation, visual and spatial composition, vector shape, animated mask-stack,
   rotoscope, motion-tracking, text-layer payloads, complete reusable effect presets, and an isolated
   OpenFX host over this snapshot, plus versioned built-in effect and transition definitions, exact
   transition timing, and bounded CPU reference factories. Preset tests instantiate ordinary
   editable nodes from complete saved schemas and exercise this module's canonical serialization and
   missing-node resolver without adding preset state to graph. The OpenFX host uses projected
   parameter evaluation for explicit-time samples and exact registry snapshots for discovered versus
   active plugin availability. Transition tests use the same snapshot for two semantic image inputs,
   animatable parameters, diagnostics, cache identity, and old-revision isolation. Effects supplies
   no production GPU runtime factory, spatial GPU renderer, vector, mask, or text rasterizer,
   production tracking accelerator, native OpenFX transport, propagation solver, timeline transition
   binder, or production GPU-rendered application value. Engine foreground playback supplies a
   caller-prepared scene-value runtime and CPU display consumer. Engine render-export binds each
   prepared decoded frame to the same immutable snapshot, evaluates an exact scene envelope, and
   sends the caller-owned delivery result to an encoder. No API, CLI, or native GPU owner closes the
   application path, so the canonical `graph.evaluate` stage remains an honest stub even though the
   generic interactive, playback, export, headless, and cache boundaries are explicit and
   test-backed. Engine playback accepts an externally prepared snapshot for exact predicted and
   foreground frames, while export evaluates explicit acquired-source routes without adding
   role-specific behavior to graph.

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

The missing-node resolution flow is:

1. A caller loads or edits a normal `GraphSnapshot<T>` whose nodes retain complete embedded schemas
   independently of current plugin discovery.
2. `resolve_graph` walks authored nodes in stable identity order against one exact
   `NodeRegistrySnapshot`. Equal identity and definition is available, an absent identity is
   unregistered, and a differing definition under the same identity is incompatible.
3. `GraphResolution<T>` retains the exact graph snapshot and stores only derived availability.
   `ResolvedNode` exposes each original typed node beside `NodeAvailability`, while
   `MissingNodePlaceholder` exposes stable blocker identity and reason.
4. Editing and graph serialization continue through the original checked graph owners. Availability
   never enters a transaction or document, so absent plugins cannot erase ports, parameter payloads,
   expressions, drivers, edges, presentation order, or schema behavior.
5. A compiler or render caller invokes `require_evaluable` before binding implementations. Missing
   nodes produce one canonical degraded unavailable result shared by editor, script, and headless
   callers. A later registry containing the exact saved definitions returns the same authored graph
   as evaluable without migration.

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
   invalidation planning is derived after published changes, while schema-to-evaluator integration
   and outer worker dispatch remain future owners.

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
   narrow mutable payload lookup on the candidate DAG and rechecks its schema type. Driver mutation
   resolves every target and dependency against the candidate, validates exact declared types,
   inserts the authored driver in canonical state, and rejects any dependency path that returns to
   the target.
5. Any failure adds the ordered mutation index and code, then discards the candidate. A successful
   nonempty batch publishes one new `Arc` state and advances exactly one revision, while every older
   snapshot keeps its exact state.
6. Editor, script, and headless callers clone the same `GraphSnapshot<T>` and observe identical
   typed nodes, parameters, drivers, edges, visual order, and topological order without a second
   model. Node removal remains explicit and is rejected until every driver targeting or depending
   on that node is cleared.

The parameter evaluation flow is:

1. A caller requests one `ParameterAddress` from an immutable `GraphSnapshot<T>`. Missing authored
   targets fail as user-correctable input before dependency traversal. Identity evaluation uses the
   stored payload domain, while projected evaluation supplies one caller-owned literal callback and
   a separate expression-capable result domain.
2. An undriven literal returns its exact stored value or calls the projection once and retains the
   literal's exact `ValueTypeId`. Driven placeholder literals are not projected. A direct link
   resolves its source first and clones that lossless result payload.
3. An expression resolves unique referenced addresses in canonical address and type order. A
   request-local ordered memo evaluates shared transitive dependencies once while retaining stable
   dependency-completion order.
4. The concrete payload owner converts each explicit expression variable to a finite scalar through
   `ExpressionParameterValue`. The checked postfix program performs only basic arithmetic, rejects
   division by zero and nonfinite values or results, and converts the finite result back to the
   driver's exact target type.
5. Every call starts empty and reads only the immutable snapshot. Editor, script, headless, and
   higher-domain projected callers therefore share one dependency traversal, authored driver state,
   typed result path, and completion order without a parallel expression store, caller-specific
   interpreter, or persistent cache.
6. Mutation-time cycle rejection is authoritative. Evaluation retains an active parameter set as a
   defensive terminal invariant check for corrupted future deserialization rather than permitting
   recursive execution.

Higher domain owners may reuse the same checked scalar program without manufacturing graph
addresses. They provide one explicit allowed-variable set at compile time and one resolver at
evaluation time. `superi-effects::keyframe::TimeExpression` permits only `time` and `value`.
`superi-effects::control` uses projected evaluation to sample exact-time animation literals before
ordinary graph links and parent expressions resolve. Both preserve the downward effects-to-graph
dependency and add no effects concept to the graph crate.

The graph document flow is:

1. A caller snapshots one checked `EditableGraph<T>` and serializes its complete definition and
   instance state into the current `superi.graph` envelope.
2. The encoder canonicalizes schemas, fields, capabilities, nodes, bindings, parameters, parameter
   drivers, edges, presentation order, and JSON object keys, then hashes the canonical payload and
   emits canonical envelope bytes.
3. The decoder rejects malformed, truncated, unknown, future, or integrity-mismatched documents
   before graph publication. A supported legacy envelope is migrated explicitly in memory.
4. Schema and instance constructors recheck names, versions, fields, cardinality, and parameter
   types. One transaction rechecks node identity, edge endpoints, connection compatibility,
   cardinality, and acyclicity before the persisted revision becomes visible.
5. The returned `GraphLoad<T>` supplies the exact checked graph and canonical current document, so
   an editor, script, or headless caller can save an upgrade without maintaining a second model.
6. Project persistence may place those bytes in a crash-safe container later. The graph codec does
   not claim filesystem interruption handling beyond rejecting incomplete document bytes.

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

The semantic edit invalidation flow is:

1. A caller supplies two immutable `GraphSnapshot<T>` values. Graph identity must match, revisions
   must be ordered, and differing state at one revision fails instead of being guessed.
2. Node schema, port, and parameter differences create direct node roots and changed parameter
   addresses. Edge differences root their old and new destinations, while driver differences root
   their target addresses.
3. Changed addresses expand through the union of old and new parameter-driver dependency graphs,
   so indirect driven targets become direct processing roots.
4. Full-frame roots propagate through both prior and current DAGs. This preserves descendants after
   a disconnection or removal without invalidating unrelated branches.
5. `GraphEditInvalidation` publishes stable graph and revision lineage plus sorted direct roots and
   affected node IDs. Presentation-only changes publish an empty processing plan.

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

The mutation layer is the integration contract across the DAG, registry, validator, and parameter
driver owner. It binds
stored `PortId` endpoints to `PortName`, exact schemas, and `ValueTypeId` compatibility without
adding catalog knowledge to topology. The invalidation planner derives work directly from the same
checked DAG exposed by each immutable `GraphSnapshot`. The ROI planner consumes the same snapshot,
schema behavior, typed edges, and exact region algebra to derive upstream work. Parameter
evaluation consumes the same snapshot's opaque literals and authored drivers. Missing-node
resolution compares that snapshot's exact schemas with immutable current discovery without changing
either owner. The generic evaluator resolves caller-owned DAG payloads, while the evaluation
snapshot binds one complete
`GraphSnapshot<T>` to those payloads through a caller-owned compiler without adding catalog
knowledge. Runtime payloads can expose exact schema, behavior, and canonical state identity by
opting into `IntrospectNode`. The document codec preserves and reconstructs that same checked
snapshot without assuming a project container. `superi-effects` now supplies the first production
higher-tier definition catalog and compiler adapter while retaining all authored values in these
graph owners. Its strict curve, visual composition, spatial composition, animated mask-stack,
rotoscope, motion-tracking, and text-layer payloads survive canonical graph reload through generic parameter
serialization without adding animation, composition, transform, camera, light, depth, motion,
mask, propagation, tracking, typography, shaping, or paragraph meaning to graph. Its cross-dissolve and
directional-wipe schemas, parameters, port bindings, immutable compilation, and reference evaluation
likewise use neutral graph contracts without adding transition meaning or timeline edit policy to
graph. Concrete runtime factories, rasterization, propagation solving, cache value integration and
invalidation invocation, project persistence, and direct graph-command adaptation into the engine
transaction owner remain separate later work. Engine now owns project-level history above this
crate, while graph intentionally owns no undo stack.

The disclosed canonical reference graph in `superi-engine` uses core `NodeId` but is not a consumer
of this store and retains string ports and edges. It remains reference behavior, not production
graph evaluation or runtime integration. Separately, engine resource preparation retains the real
`TimelineGraphCompilation` produced by `superi-timeline`; it does not translate that graph into the
reference model. `superi-project` now owns those compilations as ordinary whole-project state and
also admits named standalone `EditableGraph<CompiledTimelineGraphValue>` documents. Its immutable
snapshots preserve graph revisions and prior graph state without moving graph validation or
mutation policy into the project crate.
Engine project history may select those complete immutable project snapshots through the
project-owned restoration seam. This reverses retained timeline and standalone graph state
atomically with the rest of the project, but no `ProjectMutation` variant directly wraps
`GraphTransaction`, and graph remains unaware of session history.

## Dependencies and consumers

- Implemented source uses `superi-core` for official object IDs, color-space tags, shared errors,
  semantic versions, canonical namespaced validation, and capability sets. The document codec uses
  Serde and JSON for generic strict records. SHA-256 owns canonical document integrity, node-state
  fingerprints, and versioned evaluation cache identity.
- `superi-image` supplies the exact color pipeline and transform-stage contracts used by
  `GraphColorMetadata`. `superi-gpu` and `superi-concurrency` remain declared for later concrete
  evaluation integration and are not imported by current graph source. The implemented generic
  evaluator and semantic scheduler use only core values plus graph-owned storage, ordered standard
  collections, and payload behavior. Outer job priority and bounded worker dispatch remain with
  render coordinators and `superi-concurrency`.
- Direct manifest consumers are `superi-ai`, `superi-cache`, `superi-color`, `superi-effects`,
  `superi-timeline`, `superi-project`, and `superi-engine`.
- Project consumes `EditableGraph` and `GraphSnapshot` directly for retained timeline and named
  standalone graph state. It validates project-level identity and revision relationships while all
  node, port, parameter, edge, transaction, and immutable graph semantics remain owned here.
- Effects consumes `ScalarExpression` for bounded animation time and parent expressions, stores its
  strict curve, visual composition, spatial composition, vector shape document, animated mask-stack,
  rotoscope, motion-tracking, text-layer payloads, and complete reusable preset state through
  animatable authoring definitions,
  compiles reusable controls including lossless complete-mask links into ordinary typed drivers, and
  uses `evaluate_parameter_with` to sample only literal curves before graph driver resolution.
  Its OpenFX host also projects explicit-time literals through that same evaluator, publishes scanned
  definitions through `NodeRegistry`, and removes non-ready schemas from active discovery so
  `resolve_graph` retains authored unavailable operations without a graph rewrite.
  It also declares transition nodes with ordinary typed image ports and animatable scalar or choice
  parameters, registers them through `NodeRegistry`, mutates them through `EditableGraph`, and
  compiles them through `GraphEvaluationSnapshot`. Preset instances use fresh graph-owned IDs and
  retain exact schemas plus typed literals, while `resolve_graph` derives absent or incompatible
  availability without changing the editable node or document. Graph remains unaware of effect presentation,
  control rigs, curve, composition, spatial layer, camera, light, depth, motion, vector shape, mask,
  rotoscope, tracking, and text types, interpolation,
  keyframes, path geometry, paints, strokes, repeaters, mask controls, boolean operations,
  propagation state, tracked observations, transforms, camera poses, fonts, shaping, paragraph
  layout, transition visual semantics, timeline adjacency, and animation variable meaning.
- Cache consumes `EvaluationValueCache`, `EvaluationCacheEntryKind`, `EvaluationCacheIdentity`,
  `EvaluationCacheKey`, `GraphEditInvalidation`, and `GraphColorMetadata`; its scoped adapter
  composes graph lineage and work time with outer result identity before concrete retention, applies
  edit plans atomically to both tiers, and participates in precise revision fencing. Successful hits
  promote cache-local recency, and automatic or explicit cache-local victim removal does not change
  this graph contract. Cache render orchestration composes immutable evaluation snapshots with
  layered reuse and bounded background jobs without moving worker policy into graph.
  `superi-engine::playback` consumes `GraphEvaluationSnapshot` and
  `EvaluationRequest` to populate predicted frames and evaluate exact foreground scene values
  through the cache-owned host adapter, without adding mode-specific evaluation behavior.
  `superi-engine::export_queue` consumes the same snapshot and request contracts, binds the current
  decoded frame through a caller-owned runtime node seam, retains the evaluated graph revision, and
  validates the existing playback scene envelope before delivery and encoding. The
  `superi-engine::plugins::PluginSupervisor` rebuilds an active `NodeRegistrySnapshot` from ready
  OpenFX hosts and resolves the same immutable graph for playback, rendering, and export, preserving
  graph-owned blocker order and revision semantics. The
  project resource preparation path clones the exact published timeline compilation and editable
  graph beside exact opened media owners, without recompiling, copying payloads, or evaluating graph
  state. The engine color
  propagation contract exercises metadata. Effects consumes versioned schemas, schema registration
  snapshots, typed editable
  nodes and values, instance bindings, immutable snapshots, bounded scalar expressions,
  `NodeCompiler`, parameter evaluation, diagnostics, and evaluator seams. It uses those neutral
  contracts for workflow-neutral authoring, exact keyframe, visual composition, spatial composition,
  vector shape, mask-stack, rotoscope, motion-tracking, text-layer, and preset
  payload reload, a versioned built-in effect and transition catalog, and bounded reference pixels
  without reversing the dependency. The composition, spatial, shape, mask, and text contracts
  mutate, link, and reload
  strict domain payloads in independent timeline-role and node-graph-role graphs without adding
  composition, transform, camera, light, depth, motion, shape, mask, font, shaping, or paragraph
  behavior to graph. The rotoscope contract reloads generic authored and derived per-frame state
  without adding propagation behavior. The tracking contract
  reloads complete point, planar, object, and camera artifacts in independent timeline-role and
  node-graph-role graphs without adding selection, solver, correction, or pose behavior. Timeline
  consumes versioned schemas, typed ports, complete editable nodes, atomic graph transactions, DAG
  validation, immutable snapshots, and `GraphValue` to publish native editorial state beside shared
  processing intent without importing effects. Engine consumes that compiled result at its
  preparation boundary. Other declared domain consumers still have no production graph call site.
- The sixteen graph integration test targets remain the direct consumers of identifier,
  schema-discovery, DAG, validation, mutation, invalidation, ROI, serialization, expression,
  diagnostics, ordinary and cached evaluation, shared evaluation-snapshot, missing-node, and
  neutral value APIs. The effects authoring contract composes schema discovery, editable instances,
  graph mutation, and evaluation-snapshot compilation into one higher-tier SDK.
  Its animation, composition, spatial, shape, mask, rotoscope, tracking, text, and preset contracts
  are direct consumers of generic parameter serialization and checked graph reload. The preset
  contract also consumes exact-schema missing-node resolution and recovery.

## Invariants and operational boundaries

- Graph never defines a competing node, parameter, graph, port, edge, or resource object ID type.
  The core type and canonical wire identity remain authoritative across every consumer.
- Identifier values are opaque. Callers own allocation, deterministic derivation, uniqueness scope,
  and any meaning assigned to zero; each graph enforces node and edge uniqueness within itself.
- Graph remains below color, effects, timeline, cache, AI, project, and engine catalogs. The neutral
  identifier, value, schema, DAG, validation, mutation, invalidation, evaluator, diagnostics, and
  ROI APIs import no domain catalog and introduce no new dependency edge.
- Neutral numeric values are always finite and preserve their exact binary64 bits. Choices are
  nonempty and bounded to 256 UTF-8 bytes, and validated deserialization cannot bypass either rule.
- `GraphValue<T>` never changes or interprets `Domain(T)`. Processing variants remain distinct, and
  expression evaluation accepts only `Scalar` without Boolean, choice, color, vector, matrix, or
  domain coercion.
- Graph color metadata preserves the complete image-owned pipeline exactly and does not execute,
  infer, normalize, or reorder color transforms.
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
- Every parameter driver has one existing typed target and only existing explicitly typed
  dependencies. Direct links require exact source and target type identity. Expressions retain
  explicit typed variables, while the catalog remains responsible for numeric conversion of each
  value type.
- Parameter dependencies form a separate checked graph from pixel-flow edges. Driver mutation
  rejects direct and transitive cycles before snapshot publication, and a referenced node cannot be
  removed until every affected driver is explicitly cleared.
- Expression source, bindings, and checked instructions are ordinary immutable snapshot state.
  Compilation is bounded and pure, arithmetic accepts and produces only finite values, and no
  editor, script, or headless caller can attach hidden host code or external state.
- Stored connections resolve source outputs and target inputs through those exact bindings. Single
  and optional inputs accept at most one stored edge; variadic inputs retain stable edge identity
  order through the DAG adjacency set.
- Connected nodes cannot be removed implicitly. A transaction must disconnect incident edges before
  remove, which keeps the full ordered edit explicit without claiming local undo ownership. A
  higher project history owner may restore a complete retained snapshot without changing this
  transaction law.
- Every transaction compares one expected graph revision. Empty current-revision batches are
  idempotent, successful nonempty batches advance once, stale or exhausted revisions publish
  nothing, and any mutation failure discards all earlier candidate edits.
- Presentation order is explicit and independent of deterministic topological processing order.
  Equivalent explicit transactions produce equal snapshots regardless of insertion history.
- Graph snapshots are immutable `Arc` views. A later transaction cannot change a prior reader's
  nodes, parameters, drivers, edges, presentation order, topology, or revision.
- Plugin availability is derived from one graph snapshot and one registry snapshot. It is never
  authored, serialized, migrated, or cached into node state, so changing discovery cannot change
  the graph revision or editable meaning.
- `superi-engine::plugins::PluginSupervisor` is a production consumer of this boundary. It rebuilds
  one active `NodeRegistrySnapshot` after lifecycle changes and calls `resolve_graph` through the
  same `EngineWorkKind` path for playback, rendering, and export.
- An available node requires exact schema identity and structural equality. Missing identities and
  same-identity definition conflicts retain the original typed node as a stable placeholder and
  never substitute a latest version, coerce bindings, or rewrite saved schema fields.
- Missing-node iteration and degraded evaluation diagnostics use stable `NodeId` order and record
  both graph and registry revisions. Editor, script, and headless callers receive the same state and
  result for equal snapshots.
- Current graph documents are deterministic for equal editable meaning. Canonical collection and
  JSON object order, exact identifier text, explicit format and primitive revisions, and payload
  integrity cannot depend on insertion history, caller role, locale, platform, or hash iteration.
- Deserialization never bypasses schema, node, transaction, connection, cardinality, parameter
  driver, type, or cycle validation. The persisted graph revision becomes visible only after one
  complete candidate state is accepted, and a nonempty graph may not claim revision zero.
- Legacy migration is explicit and bounded to known source revisions. Unknown future format or
  primitive revisions, unknown fields, corrupt digests, duplicate presentation entries, invalid
  typed parameters, cycles, and interrupted documents fail instead of being repaired or guessed.
- Parameter evaluation reads one immutable snapshot, resolves unique dependencies in deterministic
  completion order, evaluates each address once per call, and owns no persistent cache or caller
  mode. Direct links preserve the exact source payload; expression conversion is explicit. A
  projected call may transform only reached undriven literals, cannot change their `ValueTypeId`,
  and reuses the same graph-owned driver traversal and invariant checks.
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
- An edit invalidation is derived only from two immutable snapshots with one graph identity and
  ordered revisions. Direct roots reflect semantic node, parameter, edge, and driver differences;
  affected nodes include driver dependents plus descendants from both old and new topology. Visual
  reorder alone never becomes processing work.
- Evaluation resolves only node-declared incoming routes and never scans the whole DAG by default.
  Declarations are canonicalized before work, equal request keys execute once per call, and the
  immutable graph borrow is the evaluation snapshot boundary.
- Evaluation-key ordering is consistent with physical-time equality. Equivalent timebase
  representations are one planned and completed work key, while distinct endpoint, physical frame,
  or exact region values remain distinct.
- Scheduling separates unique prerequisite work from full canonical node inputs. Equal source work
  is counted once for readiness, but every distinct declared edge remains an input with its exact
  dependency and stored route.
- Every scheduled prerequisite occurs in an earlier readiness batch. Equal-ready work uses stable
  key order. Ordinary completion follows the full schedule; cached completion preserves schedule
  order while omitting work below retained hits.
- Request-local reuse is not persistent caching and does not consume an invalidation plan
  automatically. Ordinary calls start empty, while cached calls receive one explicit external
  adapter. No dirty region, graph revision, retained value, outer job policy, or caller-specific
  path is hidden in evaluator state.
- Runtime compilation changes only node payloads. Graph ID, node IDs, exact edge routes, and checked
  acyclicity come from the retained editable snapshot, and every compiler failure discards the
  unpublished projection while preserving its classification and naming exact source state.
- `GraphEvaluationSnapshot<T, N>` is role-neutral. Editor, script, interactive, playback, and
  headless callers inspect one immutable authored revision and delegate scheduling, ordinary
  evaluation, and cached evaluation to the same `LazyEvaluator`; no caller mode can alter
  dependency discovery, identity, or result meaning.
- Evaluation snapshots do not observe later mutations. Cross-revision reuse is permitted only when
  the complete semantic cache identity remains equal; a higher-tier catalog must faithfully compile
  every evaluation-affecting editable value for each newly published graph revision. Cached adapter
  calls from a snapshot carry that exact revision so the cache owner can fence affected old work
  while preserving equal unaffected semantic keys.
- Semantic inspection is available before value execution and never contains measured timing.
  Ordinary and diagnostic evaluation consume the same private plan and executor, and timing cannot
  change `EvaluationResult` or cache-key equality.
- A cache key exists only for a deterministic or explicitly seeded node with enabled policy and a
  complete upstream key lineage. Node providers must include every result-affecting state byte and
  seed in the canonical state fingerprint.
- Static keys omit frame and region, per-frame keys include canonical physical time, and per-region
  keys include canonical physical time plus exact half-open bounds. Upstream keys and exact stored
  routes may conservatively make a dependent identity more specific, never less specific.
- Cached evaluation consults only `Available` graph identities and passes stable graph identity,
  optional published revision, semantic lineage, and exact work identity to the caller-owned
  adapter. A final hit stops the whole pull; an intermediate hit stops its complete prerequisite
  subtree. Failed evaluation never inserts the failing output, and disabled, nondeterministic, or
  dependency-blocked work always executes.
- Graph owns its lineage component and the storage-neutral adapter, not complete outer result
  identity, concrete storage, synchronization, invalidation cleanup, eviction, budgets, or
  persistence. `superi-cache` owns complete composite identity, exact hierarchical admission, and
  budgeted memory plus versioned corruption-recovering disk implementations for both retention
  tiers. Memory includes priority-aware strict per-tier LRU victim selection and precise graph edit
  cleanup. Later checkpoints own generation cleanup and persistent directory lifecycle policy.
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
- The crate has no project container, atomic file replacement, locking owner, autosave, recovery
  journal, outer scheduler connection, production runtime node catalog, GPU resource ownership,
  plugin loading, local undo history, cache generation owner, or direct engine command adapter.

## Tests and verification

The graph crate owns 88 integration tests across sixteen files. The two identifier tests prove all
six public domains are distinct, each canonical text value parses back exactly, and every graph
export has the same Rust `TypeId` as its official core owner.

Five node-registry tests prove strict typed definition names, complete and inspectable schema fields,
canonical port and parameter ordering, SemVer and build-metadata discovery order, exact and latest
lookup, one-revision batch registration, immutable copy-on-write snapshots, `Send + Sync` reader
sharing, editor-script-headless observation parity, actionable duplicate errors, and failure
atomicity for existing and intra-batch conflicts. They also prove duplicate schema fields fail
before registration.

Four DAG tests prove deterministic equality across insertion orders, typed endpoints, shared
routing, stable topological order, duplicate and missing-identity errors, direct and transitive
cycle prevention, failure atomicity, and consistent explicit removal.

Six diagnostics tests prove deterministic inspection across insertion histories and physically
equal timebase representations, static, per-frame, and per-region cache-key scope, exact upstream
lineage, state-edit invalidation, unchanged reached work after an unrelated unreachable edit,
policy-disabled and nondeterministic decisions, exact downstream blocking edges, equal ordinary and
diagnostic results, editor-script-headless semantic parity, real monotonic node timing, preserved
failure classification, added schema and cache context, and `Send + Sync` public values.

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

Ten expression tests prove the shared allowed-variable scalar program, editable source and checked
postfix inspection, canonical named variables, syntax and resource bounds, duplicate, missing, and
unused binding rejection, finite arithmetic, direct links, transitive expressions, exact driver
typing, deterministic dependency completion, replacement and clearing, immutable old snapshots,
missing references, direct and multi-hop cycle rejection, full candidate rollback, explicit driver
cleanup before node removal, lossless versioned document round trips, and equal
editor-script-headless parameter results. Projected evaluation additionally proves only reached
undriven literals enter the caller domain, driver semantics and completion order remain unchanged,
identity evaluation stays compatible, and projection failures gain exact graph and parameter
context.

Nine invalidation tests prove exact dirty-region union decomposition, clean-gap preservation,
full-frame subsumption, empty-region handling, requested-work clipping, stable topological
dependency order, clean-node exclusion, exact converging merges, edge-specific transforms, branch
stopping, unknown-node and mapper diagnostics, and identical editor-script-headless plans across
different node and edge insertion histories. The integration proof derives those plans from the
same immutable editable graph snapshot published by the mutation owner.

Four edit-invalidation tests prove parameter-driver expansion, exact affected descendants,
old-topology retention after edge or node removal, unrelated branch exclusion, presentation-order
exclusion, skipped-forward revision determinism, and explicit failure for mismatched, same-revision
divergent, or reversed snapshot pairs.

Eleven evaluation tests prove default incoming pulls, fresh observation after an editable payload
change, lazy branch selection, skipped failure branches, exact frame and region keys, reuse across
physically equal timebase representations, distinct temporal and spatial work, stable declaration
normalization, insertion-independent values and traces, deterministic readiness batches, planning
without value execution, distinct input edges that reuse one source work unit, exact invalidated
request clipping, editor-script-headless caller parity through one evaluator, missing targets,
invalid node-declared routes, and preserved node failure classification with request context.

Three cached-evaluation tests prove final-frame hits stop all node execution, intermediate-node hits
prune their complete upstream dependency work, retained and uncached results remain equal, final and
intermediate roles remain distinct, disabled nodes bypass storage, and failed outputs retain shared
classification without entering the cache.

Four headless integration tests prove the public compiler consumes complete immutable editable
state, resolves a snapshot-owned linked parameter during runtime-node compilation, retains graph ID
and revision beside executable state, preserves exact invalidated request regions, schedules only
the selected branch, and produces equal editor, script, and headless results through one
`Send + Sync` evaluation snapshot. They also prove old snapshots remain stable, new revisions
observe edited driver sources without stale reuse, and compiler failures keep their classification
while naming exact graph, revision, node, and schema state. The retained-path proof shows the same
snapshot delegates to caller-owned storage and returns an identical final value on a hit.

Eight ROI tests prove pass-through dependency pruning, exact repeated region union, per-source
full-frame domains, checked expansion and clipping, coordinate-overflow rejection, custom per-input
mapping, invalid mapper diagnostics, wrong-direction request rejection, invalidation intersection,
snapshot revision stamping, stable dependency order, and identical editor-script-headless plans
across different insertion histories.

Five serialization tests prove deterministic canonical current bytes, complete snapshot round
trips, semantic normalization, stable reserialization, lossless revision-zero migration, canonical
upgraded bytes, integrity and future-revision rejection, unknown-field and interruption rejection,
duplicate-order, mistyped-parameter, and cycle rejection through checked contracts, and equal
editor, script, and headless evaluation after independent loads.

Three missing-node tests prove independent graph loading before plugin discovery, exact preservation
of typed schemas, instance bindings, parameters, edges, and canonical bytes while unavailable,
continued checked editing, stable placeholder order and diagnostics, fail-closed incompatible
same-identity registration, recovery when the exact saved schema returns, and identical editor,
script, and headless state plus evaluation results through the shared evaluation snapshot.

Four downstream effects preset tests now provide a second public consumer of these contracts. They
capture complete schemas and typed literals, instantiate fresh ordinary graph nodes in independent
workflow-role graphs, serialize and reload them canonically, edit and resave them while unregistered
or incompatible, and recover the same authored meaning when the exact saved schema returns.

Four neutral-value tests prove exact finite binary64 retention including signed zero, checked
construction and deserialization, lossless serialization of every processing variant and an owned
domain payload, bounded discrete choices, and scalar-only expression conversion without coercion.

Focused verification runs all sixteen integration targets through the crate's public API. Crate-wide
tests, strict Clippy, and rustdoc cover the library and integration targets. The complete workspace
suite exercises downstream compatibility. The repository map validator checks the source inventory
and hash, while dependency and boundary tools enforce the one-way open architecture. Effects tests
now connect the neutral value and evaluation contracts to concrete effect and transition catalogs,
strict visual composition, spatial composition, and vector shape payloads, and bounded CPU reference
implementations. Spatial coverage proves one animatable domain parameter survives independent graph
reloads with identical sampled and rendered results. Transition coverage
proves two semantic image inputs, ordinary animatable parameter mutation, exact-schema compilation,
immutable revision isolation, diagnostic and cache identity changes, same-region dependency requests,
and tiled evaluation through the public `GraphEvaluationSnapshot`. No test yet connects those
catalogs to the production engine, GPU value, timeline transition binder, CLI, or import-to-export
rendered artifact.

## Current status and risks

Official graph-facing identifiers, node registration, schema discovery, deterministic DAG storage,
typed binding validation, schema-level output-to-input compatibility, complete schema-bound node
instances, editable parameters, typed parameter drivers, immutable snapshots, and revisioned atomic
mutation transactions
are implemented and test-backed beside exact dirty-region sets, deterministic dependency
invalidation planning, semantic revision-to-revision edit invalidation, and snapshot-bound ROI
propagation. Neutral graph values now retain exact domain state and finite typed processing state in
one payload without catalog dependencies. Registered definitions can be
instantiated, topology, visual order, literal parameters, typed links, and expressions can be
edited, exact state can be shared across reader roles, and callers can derive both dirty and
requested work plus deterministic parameter results from the same published DAG snapshot.
Lazy request-scoped evaluation and deterministic semantic scheduling are also implemented and
test-backed, so caller-owned evaluator payloads can resolve stored topology through inspectable
readiness batches and stable completion order. Opt-in node introspection now exposes deterministic
pre-execution schema, behavior, state, and cache-lineage decisions, while diagnostic evaluation
returns the same result beside run-local planning, execution, and node timing. The role-neutral
evaluation snapshot compiles one exact editable revision into caller-owned evaluator payloads,
retains the source state for every reader, and delegates scheduling and execution to that same
evaluator. Cached evaluation accepts caller-owned storage, stops at exact final and intermediate
hits, inserts only successful cacheable values, and carries snapshot graph and revision lineage.
`superi-cache` supplies scoped composite-key memory and disk consumers that bind graph identity to
authoritative outer result context, plus a bounded background render consumer over immutable
evaluation snapshots. Memory retains each admitted value with exact total, project,
and optional device accounting, applies graph edit invalidation atomically, promotes hits and
insertions, and reclaims eligible LRU values under budget or GPU pressure. Disk validates bounded
versioned envelopes, isolates invalid entries, and records classified failures. Reclamation,
budget refusal, or persistence failure remains an ordinary miss or skipped insertion and cannot
change the evaluator result. Cache can dispatch caller-compiled snapshots. Effects now implements a
production definition catalog, exact-schema compiler adapter, graph-native transition catalog, and
bounded CPU reference factory, but it has no production GPU runtime factory and does not connect
complete ROI and invalidation plans or timeline-owned transitions to a rendered-frame application
flow. Engine playback is the first production role consumer of an externally prepared evaluation
snapshot. Its prediction contract proves cached warming does not change foreground meaning, and its
foreground contract evaluates one exact scene value through validated retention and CPU display
conversion. Engine render-export is a second production role consumer: it binds each prepared
decoded frame to one immutable snapshot, evaluates the exact requested time and region, retains the
source graph revision, and validates scene timing, color, and alpha before delivery and encode.
Project is the first whole-project owner to retain timeline's editable compilation and named
standalone graphs. Engine resource preparation consumes one immutable project snapshot and keeps
its exact editable compilation beside source and decoder lifetimes, but it does not compile runtime
nodes or evaluate output.
Engine project history now also reverses complete retained graph state by restoring whole project
snapshots at fresh document revisions. Graph transactions are not yet directly exposed as typed
project mutations, and this crate does not observe or store history metadata.
The versioned graph document codec now preserves and validates that complete editable state,
migrates the supported legacy envelope, returns canonical upgraded bytes, and retains typed links
and editable expression source through save and load. Missing-node resolution now derives exact
current schema availability without changing those bytes or that editable state, retains absent or
incompatible nodes as typed placeholders, and gives every caller one deterministic degraded
evaluation result until exact schemas return. Exact schemas then enable the shared interactive and
headless evaluation snapshot without a graph rewrite. The crate cannot store a project atomically,
own concrete cached values, persist cache data, bind
plugin implementations, or render production values. Timeline compilation, project document
retention, engine preparation retention, and memory cache retention are now real downstream
consumers. Effects is a concrete downstream schema, expression,
diagnostics, evaluator, immutable compiler, generic serialization, authoring, and animation consumer
with strict visual composition, spatial composition, vector shape, mask, rotoscope, motion-tracking,
and text payloads, inspectable glyph layout, ordinary transition nodes, isolated OpenFX definitions
and lifecycle catalogs, complete reusable preset instances, and bounded reference pixels. Effects
also consumes canonical graph documents, exact-schema missing-node resolution, projected parameter
evaluation, and typed drivers for exact-time links, reusable controls, and parent expressions,
including ordinary `GraphValue<T>` built-in state. Its preset and OpenFX contracts consume
missing-node resolution for absent, incompatible, disabled, faulted, and quarantined operations, but
engine plugin supervision now connects active plugin lifecycle to this derived availability, but it
does not yet connect native plugin transport, production spatial GPU execution, vector, mask, or
text rasterization, glyph atlases, production tracking pyramids and GPU acceleration, propagation
solvers, timeline tracking or transition attachment, invalidation orchestration, production plugin
binding, GPU execution, or production engine orchestration into a complete render path.

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

The expression language is intentionally a bounded numeric foundation rather than a general script
runtime. Its reusable scalar layer supports an explicit variable vocabulary and basic finite
arithmetic only. Graph parameter expressions add exact typed bindings, while effects time and
parent expressions prove that higher domains can reuse the program without inventing graph
identities. Node catalogs must implement `ExpressionParameterValue` for their result representation
and may project stored literals through `evaluate_parameter_with` when exact time or another domain
context must be resolved first. Projection must remain request-local and cannot become a second
driver evaluator or stored topology. Persistence must serialize editable source, exact typed
bindings when present, and checked meaning or deterministically recompile and validate source during
migration.

Public request-local value and inspection lookup remains linear and bounded by reached work.
Planning and execution indexes use `BTreeMap` keys whose total order matches endpoint,
physical-time, and exact-region equality. The complete request-local planner remains deterministic
but must be measured before large graph or temporal-window optimization. Cached work selection is
also recursive and may need measured nonrecursive optimization for very deep graphs. Concrete
storage, invalidation invocation, and resource policy belong to cache and orchestration owners.

Other integration risks are attaching nondeterministic allocation policy to value types, claiming a
type tag proves its concrete payload representation, treating mutable editor order as evaluation
order, or treating validated editable state as sufficient evaluation proof. Invalidation-specific
risks are using identity mapping across a transform, providing a nondeterministic edge mapper,
omitting driver dependency expansion, propagating edits through only one of the old and new
topologies, or treating a derived plan as cache generation state.
Evaluation-specific risks are treating node-declared regions as completed ROI propagation, treating
semantic readiness batches as a worker-pool or GPU-submission guarantee, implementing a compiler
that omits evaluation-affecting editable state, or treating the generic shared headless boundary as
production catalog, GPU, CLI, or rendered artifact proof.
Expression-specific risks are adding implicit variables, type coercion, platform math functions,
unbounded evaluation, host script escape, or a caller-specific formula store. Reusing a changed
parameter result across graph revisions without semantic identity and a revision fence is invalid.
Diagnostics-specific risks are omitting result-affecting state or a seed from a node fingerprint,
treating run-local durations as semantic equality, treating identity alone as proof that a concrete
store contains a value, or reusing a blocked dependency lineage. Cache-specific risks are returning
a value under the wrong role or key, omitting snapshot revision lineage, executing pruned
prerequisites, inserting a failed output, or using deep-copy payloads where cloneable resource
handles are required.
ROI-specific risks are supplying stale regions of definition, implementing nondeterministic custom
mapping, or reusing a plan after its stamped graph revision has changed.
Serialization-specific risks are extending the wire format without a migration, accepting partial
state outside checked constructors, mistaking a payload digest for durable file replacement, or
letting a caller-specific wrapper become a competing editable graph model.
Missing-node-specific risks are persisting derived availability, selecting a newer schema without an
approved compatibility contract, accepting a same-identity definition conflict, or letting a
placeholder become a second editable model. The current boundary avoids all four by preserving the
original graph and requiring exact registry equality before evaluation.

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
For edit derivation, compare complete semantic node and driver state and propagate through both
immutable topologies. Do not move node-specific ROI policy into the generic DAG or treat full frame
as an invented finite extent.

Keep dependency declaration, inspection, and execution in the same shared evaluator for every
caller. Keep retained reuse behind `EvaluationValueCache<V>` and expose stable graph identity,
optional published revision, semantic graph lineage, and exact work identity without claiming
complete outer result identity; composite keys, concrete storage, and policy remain outside graph.
Map ready batches onto bounded workers only
through the outer render coordinator. Preserve work-key equality, physical-time cache normalization,
policy scope, complete upstream lineage, unique prerequisite counting, full distinct-edge inputs,
batch order, pruned completion order, and result meaning together. Keep run-local timing outside
semantic inspection and result equality.

Keep parameter drivers inside the same editable snapshot and transaction boundary. Preserve exact
target and dependency types, explicit named variables, bounded pure compilation, deterministic
address order, mutation-time cycle rejection, request-local evaluation, and literal-only projection
together. Higher domains may adapt literal payloads, but graph must retain dependency traversal,
driver meaning, exact type tags, cycle checks, memoization, and completion order. Do not add implicit
catalog lookup, host scripting, caller-specific formulas, or cached results without graph revision
ownership and invalidation proof.

Keep `GraphValue<T>` neutral and lossless. Add variants only for shared representation needs with
checked construction, validated serialization, exact equality, and explicit expression behavior.
Do not move effect names, option vocabularies, defaults, parameter schemas, or pixel algorithms into
graph, and do not coerce a non-scalar variant merely to make an expression compile.

Keep interactive and headless evaluation on `GraphEvaluationSnapshot<T, N>`. Higher-tier catalogs
must compile every runtime node from the complete retained editable state, preserve compiler failure
classification, and create a new evaluation snapshot for each new editable revision. Do not expose
the private runtime projection as a competing topology or add catalog knowledge to graph.

Keep ROI pure over one immutable editable snapshot. Preserve per-output regions of definition,
exact region-set union, checked expansion, strict custom input validation, dependency-only reverse
traversal, forward topological result order, and graph revision stamping. Do not create an
editor-specific, script-specific, or headless-specific propagation path.

Keep graph documents strict, versioned, canonical, and reconstructed through the existing checked
state owners. Additive fields must remain canonically omittable for old current-format documents;
incompatible fields require an explicit format revision and migration. Preserve unknown-future
rejection, and leave atomic project storage, recovery selection, and locking in `superi-project`.

Keep plugin availability derived from the immutable graph and registry snapshots. Preserve exact
saved schemas, typed instance state, stable blocker order, fail-closed definition conflicts, and the
shared degraded evaluation gate together. The effects OpenFX host now registers only scanned exact
schemas and derives active availability from plugin lifecycle. The engine supervisor rebuilds that
active snapshot and resolves every workflow role through this same gate. Platform adapters may add
explicitly supported historical schemas and implementation factories above this crate, but no
layer may teach the neutral resolver to guess compatibility or persist discovery state.

Recheck the effects preset consumer whenever preset capture, schema migration, document recovery, or
fresh instance binding changes. Presets must remain higher-tier users of ordinary editable nodes and
derived availability, never another graph document, identity system, or persisted plugin-state owner.

Update this map when mutation, invalidation, ROI, serialization, expressions, diagnostics, and
evaluation integrate further, concrete retention or eviction behavior changes, or cache revision
and resource policies change the adapter contract,
ROI-to-evaluator binding, project storage, direct graph command adaptation, project-history coordination,
plugin implementation lookup, the effects catalog gains production executable nodes, or another
downstream catalog becomes real. Recheck direct consumer maps whenever they begin importing any
public graph contract, and recheck value consumers whenever the neutral payload or scalar expression
boundary changes. Recheck the effects tracking consumer whenever its persisted artifact, revision
fence, correction, or graph-role reload contract changes.
