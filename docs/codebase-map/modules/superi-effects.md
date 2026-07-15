---
module_id: superi-effects
source_paths:
  - open/crates/superi-effects
source_hash: 637f8b153a15208c0c2c9e59d1336bca694a5811415110f7d83dbb4bcbc8c1a8
source_files: 16
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-effects` owns the higher-tier open visual effect authoring, animation, built-in operation,
and bounded reference-evaluation layer above the generic graph. It provides inspectable typed
definitions, ordinary editable graph-node instantiation, deterministic definition discovery,
exact-schema runtime factory translation, exact keyframe animation, and concrete schemas plus real
reference pixels for transform, crop, opacity, blend, composite, Gaussian blur, sharpen, radial
distortion, chroma key, invert, and grade.

The generic graph remains authoritative for schema identity, instance identities, typed editable
values, transactions, parameter drivers, immutable snapshots, topology, serialization, evaluation,
diagnostics, and cache identity. Core remains authoritative for exact time, finite geometry, color
meaning, capabilities, and classified errors. Image remains authoritative for canonical image
artifacts and allocation limits. Effects adds visual meaning around those contracts and never
creates a competing graph, timeline effect list, time system, expression language, or persistence
envelope.

The built-in schemas require the production `superi.render.gpu` capability, but this crate currently
implements only a deterministic bounded CPU oracle and headless proof. Production GPU kernels,
engine catalog registration, timeline effect attachment, playback, viewport, export, project
persistence, UI, masks, transitions beyond the explicit composite operation, text, tracking, and
OFX hosting remain absent or staged in their owning modules.

## Source inventory

- `open/crates/superi-effects/Cargo.toml`: Declares approved downward dependencies on
  `superi-core`, `superi-gpu`, `superi-image`, and `superi-graph`. It uses workspace Serde for the
  animation wire, `half` for checked binary16 reference conversion, and JSON only in tests.
- `open/crates/superi-effects/src/authoring.rs`: Implements presentation metadata, typed effect
  definitions, graph-native instance construction, atomic generic catalog snapshots, classified
  validation, runtime factories, and the graph `NodeCompiler` adapter.
- `open/crates/superi-effects/src/catalog.rs`: Implements the stable built-in kind and family enums,
  exact versioned authoring definitions and schemas, typed ports and animatable parameters,
  inspectable controls and defaults, deterministic discovery, GPU capability declarations, atomic
  graph registration, and caller-owned instance creation.
- `open/crates/superi-effects/src/keyframe.rs`: Implements checked animation values, independent
  value tangents, fixed and roving timing, linear, cubic, and hold interpolation, cubic Bezier
  easing, bounded time expressions, immutable curve editing, exact uniform retiming, deterministic
  evaluation, and the revisioned standalone wire.
- `open/crates/superi-effects/src/lib.rs`: Documents the implemented foundations and exports every
  effect module.
- `open/crates/superi-effects/src/mask.rs`: Placeholder for mask and rotoscoping data and rendering.
- `open/crates/superi-effects/src/ofx.rs`: Placeholder for an additive OFX-compatible plugin
  surface.
- `open/crates/superi-effects/src/reference.rs`: Implements immutable operation state, conservative
  ROI mapping, canonical image validation, bounded binary16 and binary32 CPU pixel operations,
  editable-snapshot runtime compilation, graph evaluation, deterministic introspection, and cache
  fingerprints.
- `open/crates/superi-effects/src/text.rs`: Placeholder for additive text and motion-design
  primitives.
- `open/crates/superi-effects/src/tracking.rs`: Placeholder for motion-tracking data and solving.
- `open/crates/superi-effects/src/transition.rs`: Placeholder for higher-level transitions.
- `open/crates/superi-effects/tests/authoring_contract.rs`: Proves typed discovery, immutable
  snapshots, workflow-neutral editable instances, graph mutation, exact runtime compilation,
  atomic failures, schema drift rejection, and thread-safe sharing.
- `open/crates/superi-effects/tests/catalog_contract.rs`: Proves complete stable built-in discovery,
  exact schemas, families, presentation metadata, typed controls and defaults, ports, parameters,
  behavior, GPU capability, caller-owned identities, atomic registration, graph publication, and
  invalid binding rejection.
- `open/crates/superi-effects/tests/graph_workflow_contract.rs`: Compiles a real expression-driven
  editable effect graph through `GraphEvaluationSnapshot`, evaluates pixels, inspects cache identity
  and diagnostics, proves direct-edit revision isolation, and rejects unsupported schema versions.
- `open/crates/superi-effects/tests/keyframe_contract.rs`: Proves exact evaluation, interpolation,
  easing, tangents, holds, roving allocation, expressions, immutable edits, retiming, invalid state,
  strict standalone persistence, authoring integration, and real generic graph reload.
- `open/crates/superi-effects/tests/reference_contract.rs`: Exercises real pixels for every
  operation category, binary16 and binary32 retention, extended RGB, metadata, premultiplied
  algebra, ROI, monotonic distortion, unsupported image meaning, invalid state, and final plus
  temporary resource ceilings.

## Public surface

The library exports `authoring`, `catalog`, `keyframe`, `mask`, `ofx`, `reference`, `text`,
`tracking`, and `transition`.

`authoring` exposes the workflow-neutral authoring foundation:

- `ParameterControl`, `EffectMetadata`, `EffectPortDefinition`, and
  `EffectParameterDefinition<T>` preserve user-facing labels, summaries, categories, control hints,
  exact graph declarations, animation intent, and type-matched defaults without changing stored
  value semantics.
- `EffectNodeDefinition<T>` owns one immutable `NodeSchema` and deterministic typed presentation
  maps. `instantiate` validates caller-owned port and parameter identities, applies typed defaults
  and overrides, and returns an ordinary `EditableNode<T>`.
- `EffectCatalog<T>` and `EffectCatalogSnapshot<T>` atomically publish exact definitions beside a
  graph `NodeRegistry`, retain immutable earlier revisions, and discover definitions in canonical
  schema-ID order.
- `EffectNodeFactory<T, N>` receives the exact immutable `GraphSnapshot<T>`, `NodeId`, and authored
  node. `EffectNodeCompiler<T, N>` binds exact factories to one catalog snapshot, implements graph
  `NodeCompiler`, and rejects absent factories, unregistered nodes, and same-ID schema drift.

`catalog` exposes the concrete built-in layer:

- `EffectNodeKind` lists eleven operations in stable presentation order. `EffectNodeFamily` groups
  them into geometry, compositing, filter, keying, and utility families.
- The concrete `EffectCatalog` owns exact `1.0.0` schemas and can return the full generic
  `EffectNodeDefinition<GraphValue<T>>` for any shared payload. Built-in definition construction
  reuses the authoring SDK, so presentation metadata, parameter controls, defaults, and editable
  instantiation have one implementation path.
- Every schema uses `superi.value.image` ports, typed scalar, color, or bounded choice parameters,
  explicit animatability, current-frame time behavior, deterministic per-region caching, exact
  ACEScg requirements, and the `superi.render.gpu` capability.
- `EffectCatalog::register` publishes every schema in one atomic graph registry revision.
  `instantiate<T>` accepts stable instance identities from the caller and rejects incomplete,
  duplicate, unknown, cross-direction, or mistyped bindings before publication.

`keyframe` exposes exact editable animation:

- `AnimationValue`, `ValueTangent`, `KeyframeTiming`, `Interpolation`, `Easing`, `CubicEasing`, and
  `Keyframe` retain finite bounded property values, independent slopes, fixed or roving intent,
  linear, cubic, or hold segments, and inspectable time easing.
- `TimeExpression` reuses `superi-graph::expr::ScalarExpression` for bounded arithmetic over only
  `time` and interpolated `value`.
- `AnimationCurve` validates complete authored state, derives distinct roving times, evaluates at
  exact `RationalTime`, and provides immutable insert, replace, remove, expression, and exact retime
  edits.
- `ANIMATION_CURVE_SCHEMA_REVISION` identifies the strict standalone wire. Deserialization denies
  unknown fields, recompiles expressions, and reconstructs through the public checked boundary.

`reference` exposes bounded executable proof:

- `ReferenceEffectState`, sampling, blend, and Porter-Duff enums retain exact compiled operation
  state. `required_input_regions` calculates state-dependent conservative dependencies.
- `evaluate_reference` accepts premultiplied, unqualified RGBA ACEScg in `Rgba16Float` or
  `Rgba32Float`. Results retain color tags, channel identity, metadata, representation, and display
  window without clamping extended scene-linear RGB.
- `ReferenceEffectNode`, `compile_reference_node`, and the limits-aware compiler translate an exact
  immutable editable snapshot into graph `EvaluateNode<Image>` and `IntrospectNode` behavior.

The five remaining feature modules expose no substantive public behavior.

## Architecture and data flow

The shared authoring flow is:

1. A definition combines an exact `NodeSchemaId`, typed ports and parameters, node behavior,
   required capabilities, presentation metadata, control hints, and exactly typed defaults.
2. `EffectNodeDefinition::new` validates and canonicalizes each namespace, then constructs the
   authoritative immutable graph schema without workflow or instance state.
3. A timeline, node editor, script, or headless caller supplies stable instance identities.
   `instantiate` validates overrides, fills omitted values from defaults, and delegates complete
   binding validation to `EditableNode::new`.
4. Generic authoring catalog registration stages definitions in exact schema-ID order, updates a
   cloned graph registry, and publishes both maps atomically. Runtime compilation checks complete
   schema equality before invoking an exact caller-supplied factory.
5. The concrete built-in catalog constructs every definition through this same authoring path and
   stores its exact schema for deterministic discovery and graph registration.

The animation flow is:

1. The caller creates finite property values, optional component tangents, inspectable easing, and
   fixed or roving keys on one core-owned `Timebase`.
2. Curve construction requires fixed endpoints, strictly increasing fixed anchors, one component
   width, matching tangents, and enough integer ticks for every interior key.
3. Roving groups derive integer-tick positions from cumulative component distance, with stable index
   spacing when every adjacent distance is zero. Derived times are inspected but never serialized.
4. Evaluation compares exact caller time against resolved keys. Interior sampling uses the outgoing
   linear, hold, or cubic segment after its outgoing easing map, then applies an optional bounded
   time expression component by component.
5. Immutable edits reconstruct complete checked state. Uniform retiming maps fixed keys exactly,
   retains roving intent, recomputes derived timing, and inversely scales value-per-second tangents.
6. Strict standalone and generic graph reload preserve authored intent. Effects tests instantiate an
   animatable definition, store its curve in `EditableGraph`, reload canonical graph bytes, and
   obtain identical samples.

The built-in evaluation flow is:

1. A caller gets one concrete authoring definition, allocates stable IDs, and stores the resulting
   `EditableNode<GraphValue<T>>` in an ordinary graph beside domain-owned payloads and shared scalar,
   vector, color, matrix, Boolean, or choice processing values.
2. Graph transactions publish nodes, typed edges, literal parameters, direct links, or bounded
   expressions. Effects owns no parallel editable state.
3. Reference compilation resolves every parameter driver from the exact immutable graph snapshot,
   requires full equality with the exact built-in schema, validates operation domains, and binds
   semantic inputs by destination `PortId`.
4. ROI planning expands or inverse-maps requested regions for neighborhood and geometric operations.
   Local pointwise operations request the output region from each semantic input.
5. `GraphEvaluationSnapshot` executes the same immutable node for editor, script, diagnostics,
   cache, and headless roles. Introspection fingerprints exact schema, resolved values, discrete
   choices, and semantic bindings while graph topology and upstream lineage remain graph-owned.
6. The CPU oracle validates image meaning, state, dimensions, channels, final storage, temporary
   pixels, kernels, and coordinate arithmetic against `ImageLimits` before loops or allocation.

## Dependencies and consumers

- `superi-core` supplies errors, diagnostics context, finite geometry, color and alpha semantics,
  capability sets, semantic versions, `RationalTime`, `Timebase`, exact rescaling, and stable time
  serialization.
- `superi-graph` supplies schemas, registries, neutral `GraphValue<T>`, typed editable state,
  mutation, parameter evaluation, bounded scalar expressions, immutable runtime compilation, lazy
  evaluation, diagnostics, cache identity, and generic graph persistence. Graph never depends on
  effects.
- `superi-image` supplies immutable image artifacts, metadata, exact crop and transform operations,
  sample representations, and finite limits.
- `superi-gpu` is a declared production capability dependency. Current effects source uploads,
  owns, and evaluates no GPU resource.
- Serde owns strict animation records. JSON is test-only. `half` performs checked reference
  conversion to and from binary16.
- `superi-timeline` does not depend on effects. It compiles native state into graph-owned
  `GraphValue<TimelineGraphValue>`, the neutral payload shape that a higher integration owner can
  share with built-in processing values.
- `superi-engine` declares `superi-effects` but has no production catalog, animation, evaluator,
  playback, viewport, or export call site. Current real consumers are the role-neutral authoring,
  generic graph reload, and bounded headless graph-evaluation contracts.

## Invariants and operational boundaries

- Effects depends down on graph, image, GPU, and core. None of those modules depends on effects.
- Effect authoring composes the canonical graph. It owns no competing schema, DAG, parameter driver,
  transaction, snapshot, identity, evaluator, scheduler, or serialization envelope.
- Definitions and animation curves are immutable after construction. Exact schema identity includes
  node type and semantic version, and full schema equality is checked before runtime projection.
- Labels, summaries, and categories cannot be blank. Defaults and overrides must match their exact
  graph `ValueTypeId`. Every instance identity belongs to the caller.
- Discovery, definition, port, parameter, catalog, and schema iteration is deterministic. Batch
  registration is atomic, earlier snapshots remain immutable, and failures cannot publish partial
  state.
- Every result-affecting built-in parameter is typed, inspectable, editable, and animatable.
  Discrete choices remain bounded choice variants rather than numeric coercions.
- Production schemas require `superi.render.gpu`. CPU evaluation is an oracle and headless proof,
  not an engine fallback or production render route.
- Blend and composite algebra uses premultiplied alpha. Straight-color pointwise operations
  explicitly unassociate and reassociate. RGB is extended scene-linear, and alpha remains finite
  from zero through one.
- Radial mappings must remain monotonic across requested work. Singular transforms, invalid choices,
  negative filter domains, nonfinite state or samples, incompatible inputs, unsupported image
  meaning, and resource overflow fail actionably.
- Reference output and every material temporary allocation are checked before their loops or
  reservation. Conversion must remain finite in the destination representation.
- Fixed animation time is exact and increasing. Curves require fixed endpoints. Values and tangents
  are finite and bounded, roving times are derived and distinct, and expression source is editable
  but cannot perform I/O, mutation, loops, recursion, calls, or dynamic lookup.
- Workflow parity is structural. Timeline and node-graph roles receive no role flag or hidden state
  branch, and old immutable graph revisions cannot observe later direct edits.

## Tests and verification

Six authoring tests prove typed discovery, immutable snapshots, workflow-neutral nodes, exact runtime
factories, atomic failure, schema drift rejection, and thread-safe sharing. Eight animation tests
prove exact scalar and vector sampling, linear, hold, and cubic behavior, easing overshoot, roving
allocation, bounded expressions, immutable editing, exact retiming, strict persistence, and generic
graph reload.

Five catalog tests cover all eleven operations, full schema behavior, authoring metadata and control
integration, caller-owned identities, typed defaults, atomic registration, ordinary graph
publication, and invalid bindings. Five reference tests exercise every operation category through
real canonical images, both sample representations, retained semantics, formulas, spatial work,
invalid domains, and limits. Two graph workflow tests prove expression resolution, immutable real
pixel evaluation, diagnostic parity, cache-key changes, old-reader stability, and fail-closed exact
schema versioning.

Focused tests, crate-wide tests, warnings-denied Clippy, rustdoc, formatting, dependency and offline
boundary checks, map validation, complete workspace tests, fixtures, platform codec consumers,
frontend gates, and the full repository checkpoint verifier form the delivery floor.

## Current status and risks

The authoring SDK, exact keyframe animation, built-in definitions, generic editable instantiation,
deterministic CPU reference pixels, ROI mapping, immutable graph compilation, introspection, and
role-neutral graph proof are substantive and test-backed. The reference implementation is scalar
and allocation-bounded, not performance production code.

There is no GPU shader parity, engine registry, production runtime catalog, timeline attachment,
playback, viewport, export, project persistence, UI, spatial motion path, mask, text, tracking,
higher transition authoring, or OFX host. Authoring metadata is in memory and has no independent
wire. Control hints do not yet contain numeric bounds, choice option vocabularies, grouping,
conditional visibility, or accessibility policy. Animation has no stable project-level property
identity or production caller-time context.

The CPU evaluator proves implementation semantics and graph integration but does not close a
production import-to-render path. The `superi.render.gpu` requirement deliberately prevents it from
being mistaken for production execution.

## Maintenance notes

Preserve the one-way effects-to-graph dependency and keep authored values in ordinary graph state.
Keep animation property meaning with node definitions and exact time ownership in core. Preserve
checked immutable editing, authored versus derived timing, bounded expressions, exact schema
matching, atomic catalog publication, workflow-neutral instances, canonical image meaning, and
bounded reference allocation.

Add built-in kinds only through one versioned authoring definition with typed defaults, presentation,
complete caller-owned binding validation, state compilation, ROI mapping, reference pixels,
fingerprint coverage, and real graph consumer tests. Keep formulas and image rules aligned across
future GPU implementations, and compare shaders against the oracle without moving GPU ownership
into effects or adding an implicit CPU fallback.

When production consumers arrive, record property identities, caller-time flow, GPU resource
ownership, cache behavior, serialization and migration ownership, timeline attachment, project
reload, engine registration, viewport, headless, and export consumers. Update the graph, timeline,
engine, workspace, and global maps whenever those contracts or relationships change.
