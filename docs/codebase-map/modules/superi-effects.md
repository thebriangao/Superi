---
module_id: superi-effects
source_paths:
  - open/crates/superi-effects
source_hash: 4fbc860bf7df2ed555448aefe9f2152032a05705d6f745322aa488ab97c60ebb
source_files: 10
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-effects` owns the higher-tier internal visual effect and node authoring SDK above the
generic graph. It defines inspectable effect presentation and defaults, exact graph-native
instantiation, deterministic definition discovery, and exact-schema runtime factory translation.
The generic graph remains authoritative for schema identity, stable instance identities, typed
editable values, transactions, parameter drivers, immutable snapshots, topology, and evaluation.

The crate also reserves animation, mask and rotoscoping, transitions, text, tracking, and future OFX
compatibility modules. Those six feature modules remain explicit skeletons. No built-in visual node,
pixel algorithm, GPU kernel, keyframe model, mask, transition, text object, tracking solver, plugin
host, persistence adapter, or engine playback path is implemented here yet.

## Source inventory

- `open/crates/superi-effects/Cargo.toml`: Declares the approved downward dependencies on
  `superi-core`, `superi-gpu`, `superi-image`, and `superi-graph`. Core and graph are active authoring
  dependencies; image and GPU remain declared boundaries for later visual execution work.
- `open/crates/superi-effects/src/authoring.rs`: Implements typed inspectable definitions,
  graph-native instance construction, atomic catalog snapshots, classified validation, runtime
  factories, and the graph `NodeCompiler` adapter.
- `open/crates/superi-effects/src/keyframe.rs`: Placeholder for parameters that vary over time.
- `open/crates/superi-effects/src/lib.rs`: Documents the implemented authoring boundary, the staged
  compositing features, and publicly exports all seven subsystem modules.
- `open/crates/superi-effects/src/mask.rs`: Placeholder for mask and rotoscoping data and rendering.
- `open/crates/superi-effects/src/ofx.rs`: Placeholder for an additive OFX-compatible plugin surface.
- `open/crates/superi-effects/src/text.rs`: Placeholder for additive text and motion-design
  primitives.
- `open/crates/superi-effects/src/tracking.rs`: Placeholder for motion-tracking data and solving.
- `open/crates/superi-effects/src/transition.rs`: Placeholder for transition definitions and
  execution.
- `open/crates/superi-effects/tests/authoring_contract.rs`: Public integration contract for typed
  discovery, immutable snapshots, workflow-neutral editable instances, graph mutation, exact
  runtime compilation, atomic failures, schema drift rejection, and thread-safe sharing.

## Public surface

`authoring` exposes the following implemented SDK layers:

- `ParameterControl` is a nonexhaustive presentation hint. Automatic, toggle, slider, angle,
  percentage, point, color, choice, and text hints never change the stored graph value type,
  animation declaration, or evaluation semantics.
- `EffectMetadata` requires nonempty label, summary, and category text for one node family.
- `EffectPortDefinition` combines one canonical typed `PortSchema` with a required label and summary.
- `EffectParameterDefinition<T>` combines one canonical typed and optionally animatable
  `ParameterSchema`, required presentation, one control hint, and one exactly type-matched default
  `TypedParameterValue<T>`.
- `EffectNodeDefinition<T>` constructs one immutable `Arc<NodeSchema>` and deterministic input,
  output, and parameter description maps. Its metadata and lookup or iteration methods expose all
  authored intent in canonical schema-local name order.
- `EffectParameterBinding` and `EffectInstanceBindings` retain caller-owned stable parameter and
  port identities. `EffectNodeDefinition::instantiate` validates typed overrides, fills omitted
  overrides from typed defaults, and returns an ordinary `EditableNode<T>`.
- `EffectCatalog<T>` atomically registers exact definitions while synchronizing a graph
  `NodeRegistry`. `EffectCatalogSnapshot<T>` retains immutable definition and schema maps, one exact
  registration revision, exact-ID lookup, and canonical node-type and semantic-version iteration.
- `EffectNodeFactory<T, N>` receives the exact immutable `GraphSnapshot<T>`, `NodeId`, and
  `EditableNode<T>` for one runtime translation. Closures with the same contract implement the trait.
- `EffectNodeCompiler<T, N>` owns exact factories over one immutable catalog snapshot, rejects
  unknown or duplicate registration, implements graph `NodeCompiler`, rejects unregistered nodes,
  unavailable implementations, and same-ID schema drift, and preserves factory errors with effect
  context.

The library also exports `keyframe`, `mask`, `ofx`, `text`, `tracking`, and `transition`, but those
modules still expose no implemented types or behavior.

## Architecture and data flow

Authoring begins with existing graph value types, port and parameter names, an exact
`NodeSchemaId`, `NodeBehavior`, and required `CapabilitySet`. A caller supplies required
presentation plus typed defaults. `EffectNodeDefinition::new` first collects each description
namespace in a `BTreeMap`, rejects duplicates, then constructs the canonical `NodeSchema`. The
result is immutable and contains no instance or workflow-specific state.

For an instance, the timeline, node editor, script, or other caller supplies stable input, output,
and parameter identities. `instantiate` validates every override name and type, copies defaults for
unoverridden parameters, constructs normal `EditableParameter<T>` values, and delegates complete
binding validation to `EditableNode::new`. The returned node enters `EditableGraph<T>` through the
same atomic mutations as every other graph node. No effect-only transaction or hidden parameter
store exists.

Catalog registration stages definitions in canonical exact schema-ID order and rejects any current
or batch duplicate before changing state. It registers cloned schemas into a cloned `NodeRegistry`,
then publishes both definition and graph-schema maps together. Snapshots share immutable maps, so a
later successful registration advances the mutable catalog without changing an earlier reader.

Runtime preparation begins from one catalog snapshot. Higher-tier built-in, extension, or engine
code registers a factory for each exact definition it can execute. During
`GraphEvaluationSnapshot::compile`, the compiler finds the authored node's exact definition,
compares the full schema with the catalog copy, resolves the factory, and passes the original graph
snapshot and node state through. Timeline and node-graph roles therefore use one definition,
editable payload, compiler, and runtime translation path rather than workflow-specific copies.

This checkpoint stops at runtime translation. Factories may later construct executable visual nodes,
but no factory supplied by this crate currently reads or writes image bytes, allocates GPU resources,
executes a shader, schedules graph work, or produces a rendered frame.

## Dependencies and consumers

- `superi-core` supplies classified `Error`, deterministic `ErrorContext`, recoverability, results,
  `CapabilitySet`, and semantic versions embedded by graph schema identities.
- `superi-graph` supplies `NodeSchema`, `NodeRegistry`, names and behavior declarations,
  `TypedParameterValue<T>`, instance bindings, `EditableNode<T>`, `GraphSnapshot<T>`, and the
  `NodeCompiler` seam. Effects depends on graph; graph never depends on effects.
- `superi-image` and `superi-gpu` remain direct manifest dependencies for the later visual runtime,
  but current effect source imports neither crate and owns no image or GPU resource.
- `superi-engine` declares `superi-effects` as a dependency, but current engine source still has no
  effect authoring, catalog, compiler, playback, or rendering call site.
- `superi-timeline` has no dependency on effects. Its existing compile-to-graph path demonstrates the
  host-owned editable graph model that this generic SDK is designed to join, but no production
  timeline object attaches an effect definition yet.
- The public integration contract is the current direct consumer. It labels two independent editable
  graphs as timeline and node-graph roles and proves the same definition, edits, and factory path in
  both without introducing another runtime module.

## Invariants and operational boundaries

- Effect authoring composes the canonical graph. It does not own a competing schema, DAG, parameter
  driver, transaction, snapshot, identity, evaluator, or scheduler.
- Definitions are immutable after construction. Exact schema identity includes node type and semantic
  version, and full schema equality is checked again before runtime factory use.
- Labels, summaries, and categories cannot be blank. Defaults and overrides must match their exact
  graph `ValueTypeId`; unknown and duplicate authoring state is rejected with classified context.
- Every instance identity belongs to the caller and is validated against every schema declaration.
  Defaults become ordinary editable parameter payloads, and runtime factories own no hidden authored
  state.
- Definition, port, parameter, catalog, and schema iteration is deterministic through `BTreeMap` and
  the graph registry's semantic-version ordering.
- Batch catalog registration is atomic. Immutable snapshots cannot observe partial definitions or a
  schema revision that differs from their effect definition revision.
- Runtime factory registration is bound to one immutable catalog snapshot and one exact schema
  version. Missing factories degrade explicitly rather than pretending the node executed.
- Catalogs and compilers are shareable when their caller-owned payload and runtime types are
  `Send + Sync`. A runtime factory itself must be `Send + Sync`.
- Workflow parity is structural: timeline and node-graph roles receive no role flag or private state
  branch. Later timeline attachment code must retain this single authored meaning.
- GPU residency, ROI execution, time sampling, color pixels, caching, serialization, keyframes,
  masks, plugin containment, and text fidelity remain owned by later code and are not implied by
  schema declarations alone.

## Tests and verification

`authoring_contract` contains six public tests:

- It constructs one image-to-image definition with an animatable scalar parameter, inspects exact
  metadata, presentation, graph types, defaults, and synchronized schema discovery, and proves
  canonical definition ordering plus immutable earlier snapshots after later registration.
- It instantiates the same definition in independent timeline-role and node-graph-role
  `EditableGraph` values, reads the same typed default, applies normal `SetParameter` transactions,
  and observes equal typed results.
- It binds one exact `Send + Sync` closure factory and compiles both immutable graph snapshots through
  `GraphEvaluationSnapshot`, recording both graph and node identities.
- It proves an authored node with no runtime factory fails as `Unavailable` and a different full
  schema under the same exact ID fails as invalid input before factory execution.
- It proves blank metadata, mistyped defaults, duplicate batch and later registration, missing
  bindings, unknown overrides, and unknown or duplicate factories fail without changing catalog
  revision or membership.
- Compile-time bounds prove `EffectCatalog<AuthorValue>` and
  `EffectNodeCompiler<AuthorValue, ()>` are `Send + Sync` for a shareable payload.

Fresh focused proof passed `cargo test -p superi-effects`, six public integration tests,
documentation tests, and `cargo clippy -p superi-effects --all-targets -- -D warnings`. Cargo emitted
only the existing future-incompatibility notice for transitive `block v0.1.6`; it was not a test or
lint failure. Broader graph, timeline, engine, formatting, map, and checkpoint verifier results are
recorded in the ignored checkpoint execution log.

## Current status and risks

The internal authoring SDK is substantive and test-backed. The rest of the crate remains skeleton
code. There are no concrete effect definitions or runtime factories in production source, so the
SDK can author and translate caller-supplied definitions but does not yet render a visual result.

The generic payload is intentional and preserves compatibility with the graph host, but a later
shared visual value contract must be chosen by the real built-in and timeline consumer. The control
enum is presentation only and does not yet carry slider bounds, choice options, units, grouping,
conditional visibility, or accessibility policy. Definition metadata is in-memory and has no
serialization or migration contract. Runtime factories are exact-version bound but have no plugin
discovery, missing-node placeholder, GPU device, cache, or lifecycle integration.

The current timeline-role proof uses a normal editable graph rather than a production
`superi-timeline` effect attachment because that model does not exist. The engine still has no
effect catalog or compiler call site. Those absences must remain explicit until later checkpoints
add real callers and rendered behavior.

## Maintenance notes

Refresh this map whenever effect authoring metadata, graph composition, catalog registration,
instance defaults, runtime factory translation, or any staged feature module changes. Preserve the
one-way effect-to-graph dependency and keep all authored values in ordinary graph state.

When concrete nodes arrive, record their exact schemas, types, presentation, time and ROI behavior,
color requirements, capabilities, factory implementations, GPU or CPU resource ownership, cache
identity, and real timeline, engine, headless, and rendered consumers. Update the graph consumer map
and global index whenever this higher-tier catalog uses another graph surface. Do not report a
registered schema or successful runtime translation as pixel execution without an exercised
factory and real output proof.
