---
module_id: superi-effects
source_paths:
  - open/crates/superi-effects
source_hash: c54ef5bf6af803a69396286d7c07e33b5c4f71d08d70fee97104f3f02a0eb069
source_files: 11
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-effects` owns the higher-tier open visual effect authoring and animation layer above the
generic graph. Its graph-native authoring SDK provides inspectable presentation and defaults,
ordinary editable node instantiation, deterministic definition discovery, and exact-schema runtime
factory translation. Its animation substrate provides exact directly editable keyframes, fixed and
roving timing, linear, cubic, and hold interpolation, inspectable easing and value tangents, bounded
time expressions, immutable edits, exact retiming, and strict persistence.

The generic graph remains authoritative for schema identity, instance identities, typed editable
values, transactions, parameter drivers, immutable snapshots, topology, serialization, and
evaluation. Core remains authoritative for exact time and errors. Effects adds domain meaning around
those contracts and never creates a competing graph, time, expression, or persistence model.

Mask and rotoscoping behavior, transitions, text and motion-design primitives, tracking, and OFX
compatibility remain explicit placeholders. No built-in visual node, pixel algorithm, GPU kernel,
mask renderer, transition implementation, tracker, text engine, plugin host, engine playback path,
or rendered effect output is implemented here yet.

## Source inventory

- `open/crates/superi-effects/Cargo.toml`: Declares approved downward dependencies on `superi-core`,
  `superi-gpu`, `superi-image`, and `superi-graph`, adds workspace Serde for the animation wire, and
  uses workspace JSON only in tests.
- `open/crates/superi-effects/src/authoring.rs`: Implements typed inspectable definitions,
  graph-native instance construction, atomic catalog snapshots, classified validation, runtime
  factories, and the graph `NodeCompiler` adapter.
- `open/crates/superi-effects/src/keyframe.rs`: Implements checked animation values, independent
  value tangents, fixed and roving timing, segment interpolation, cubic Bezier easing, bounded time
  expressions, immutable curve editing, exact uniform retiming, deterministic evaluation, and the
  revisioned standalone wire.
- `open/crates/superi-effects/src/lib.rs`: Documents both implemented foundations and publicly
  exports authoring, animation, and five staged visual feature modules.
- `open/crates/superi-effects/src/mask.rs`: Placeholder for mask and rotoscoping data and rendering.
- `open/crates/superi-effects/src/ofx.rs`: Placeholder for an additive OFX-compatible plugin surface.
- `open/crates/superi-effects/src/text.rs`: Placeholder for additive text and motion-design
  primitives.
- `open/crates/superi-effects/src/tracking.rs`: Placeholder for motion-tracking data and solving.
- `open/crates/superi-effects/src/transition.rs`: Placeholder for transition definitions and
  execution.
- `open/crates/superi-effects/tests/authoring_contract.rs`: Proves typed discovery, immutable
  snapshots, workflow-neutral editable instances, graph mutation, exact runtime compilation, atomic
  failures, schema drift rejection, and thread-safe sharing.
- `open/crates/superi-effects/tests/keyframe_contract.rs`: Proves exact evaluation, interpolation,
  easing, tangents, holds, roving allocation, expressions, immutable edits, retiming, invalid state,
  strict standalone persistence, authoring-SDK instantiation, and real generic graph reload.

## Public surface

The library exports `authoring`, `keyframe`, `mask`, `ofx`, `text`, `tracking`, and `transition`.
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
  `EditableNode<T>` for one runtime translation. Closures with the same contract implement the
  trait.
- `EffectNodeCompiler<T, N>` owns exact factories over one immutable catalog snapshot, rejects
  unknown or duplicate registration, implements graph `NodeCompiler`, rejects unregistered nodes,
  unavailable implementations, and same-ID schema drift, and preserves factory errors with effect
  context.

`keyframe` exposes the following implemented animation contracts:

- `AnimationValue` is a finite, nonempty, bounded multi-component property value. `new`, `scalar`,
  `components`, and `component_count` expose checked construction and stable inspection.
- `ValueTangent` stores finite component slopes in value units per second and exposes them in
  stable property order.
- `KeyframeTiming` distinguishes exact fixed `RationalTime` from authored interior `Roving` intent.
  `Interpolation` selects `Linear`, cubic Hermite `Cubic`, or outgoing `Hold` behavior. `Easing`
  selects direct segment progress or one inspectable `CubicEasing`.
- `CubicEasing::new`, `control_points`, and `map` expose a finite cubic Bezier time map. The x
  controls are restricted to zero through one, y overshoot remains legal, endpoints are exact, and
  interior inversion uses a fixed iteration count.
- `Keyframe::new` binds timing, value, outgoing interpolation and easing, and independent incoming
  and outgoing tangents. Read methods expose every authored field without exposing unchecked
  mutation.
- `TimeExpression::compile`, `source`, and `variables` expose editable bounded arithmetic over only
  `time` and interpolated `value`. The implementation reuses `superi-graph::expr::ScalarExpression`
  instead of embedding a second parser or host scripting runtime.
- `AnimationCurve::new` checks complete authored state and derives roving times. `timebase`,
  `keyframes`, `resolved_times`, and `expression` expose authored intent and computed timing.
  `evaluate` samples at an exact caller-provided physical time. `with_inserted`, `with_replaced`,
  `without`, and `with_expression` return revalidated immutable edits, while `retimed` maps the
  complete curve onto exact new endpoints.
- `ANIMATION_CURVE_SCHEMA_REVISION` identifies the current incompatible standalone curve wire.
  `AnimationCurve` and `TimeExpression` implement Serde through private checked wire types.

The five remaining feature modules expose no substantive public types or behavior.

## Architecture and data flow

The graph-native authoring flow is:

1. A caller supplies existing graph value types, port and parameter names, an exact `NodeSchemaId`,
   `NodeBehavior`, required capabilities, inspectable presentation, and exactly typed defaults.
2. `EffectNodeDefinition::new` canonicalizes each description namespace, rejects duplicates, and
   constructs the authoritative immutable `NodeSchema` without instance or workflow-specific state.
3. A timeline, node editor, script, or other owner supplies stable instance identities.
   `instantiate` validates override names and types, fills omitted values from typed defaults, and
   delegates complete binding validation to `EditableNode::new`. The node enters `EditableGraph`
   through ordinary atomic transactions.
4. Catalog registration stages definitions in exact schema-ID order, updates a cloned graph
   registry, and publishes both maps atomically. Immutable snapshots preserve earlier revisions.
5. Runtime preparation binds exact factories to one catalog snapshot. During
   `GraphEvaluationSnapshot::compile`, `EffectNodeCompiler` confirms the authored node's complete
   schema, resolves its exact factory, and passes through the original graph snapshot and node.
   This crate currently supplies no concrete visual runtime factory.

The authored animation and evaluation flow is:

1. The caller creates finite property values, optional component tangents, inspectable easing, and
   fixed or roving keyframes on one core-owned `Timebase`.
2. `AnimationCurve::new` validates complete component and timing state. It requires fixed endpoints,
   strictly increasing fixed anchors, one component width, matching tangent dimensions, and enough
   integer ticks to place every interior key distinctly.
3. Fixed times remain authored state. Each roving group derives integer-tick positions between fixed
   anchors from cumulative L1 component distance, with even index spacing when all adjacent value
   distances are zero. Derived times are inspectable but never serialized as a second source of
   truth.
4. `evaluate` compares one exact caller `RationalTime` against resolved key times. Exact keys and
   clamped endpoints retain authored values; an interior segment uses outgoing linear, hold, or
   cubic Hermite interpolation after its outgoing easing map. Missing cubic tangents default to the
   segment secant.
5. An optional time expression runs component by component after base interpolation. `time` is the
   exact evaluation instant converted to physical seconds, while `value` is the component's base
   sample. The shared bounded graph evaluator rejects division by zero and nonfinite results.
6. Immutable edits reconstruct the complete curve. Uniform retiming exactly maps every fixed key,
   retains roving intent, recomputes derived times, and inversely scales value-per-second tangents so
   the normalized value graph remains stable.
7. Standalone serialization records an explicit schema revision, timebase, authored keyframes, and
   optional expression source. Deserialization rejects unknown fields and unsupported revisions,
   recompiles expressions, rebuilds value and tangent types, and reuses `AnimationCurve::new`.

The two foundations compose without another ownership layer. The animation integration builds an
animatable `EffectParameterDefinition<AnimationCurve>`, inspects that declaration, instantiates an
ordinary effect-authored `EditableNode`, and stores it in `EditableGraph`. Generic graph
serialization preserves the curve payload, while checked graph reload reconstructs it through the
strict effects wire and produces identical samples and canonical graph bytes. Graph remains unaware
of effect metadata, keyframes, interpolation, and time-expression variable meaning.

## Dependencies and consumers

- `superi-core` supplies classified errors, deterministic context, recoverability, capability sets,
  semantic versions, `RationalTime`, `Timebase`, exact rescaling, and stable time serialization.
- `superi-graph` supplies schemas, registries, names, behavior declarations, typed parameter values,
  instance bindings, editable nodes and snapshots, the `NodeCompiler` seam, generic graph
  serialization, and the bounded scalar-expression language. Effects depends downward on graph;
  graph never depends on effects.
- Serde owns the strict curve and expression records. JSON is test-only and proves standalone and
  generic graph document behavior. Both packages were already resolved in the workspace lock graph.
- `superi-image` and `superi-gpu` remain direct manifest dependencies for later visual execution,
  but current effect source imports neither and owns no image or GPU resource.
- `superi-engine` declares `superi-effects` as a dependency, but current engine source has no effect
  catalog, compiler, animation evaluator, playback, or rendering call site.
- `superi-timeline` has no dependency on effects. Its existing graph compiler demonstrates the
  host-owned editable model this SDK is designed to join, but no production timeline object attaches
  an effect definition or evaluates an animation curve yet.
- Public integration contracts are the current direct consumers. Authoring tests label independent
  editable graphs as timeline and node-graph roles; the animation test composes the authoring SDK,
  strict curve payload, and graph document reload without introducing a runtime module.

## Invariants and operational boundaries

- Effect authoring composes the canonical graph. It does not own a competing schema, DAG, parameter
  driver, transaction, snapshot, identity, evaluator, scheduler, or serialization envelope.
- Definitions are immutable after construction. Exact schema identity includes node type and
  semantic version, and full schema equality is checked again before runtime factory use.
- Labels, summaries, and categories cannot be blank. Defaults and overrides must match their exact
  graph `ValueTypeId`; unknown and duplicate authoring state is rejected with classified context.
- Every instance identity belongs to the caller and is validated against every schema declaration.
  Defaults become ordinary editable parameter payloads, and runtime factories own no hidden state.
- Definition, port, parameter, catalog, and schema iteration is deterministic. Batch registration is
  atomic, immutable snapshots cannot observe partial state, and exact missing factories degrade
  explicitly rather than pretending execution occurred.
- Workflow parity is structural: timeline and node-graph roles receive no role flag or private state
  branch. A parameter's animatable schema declaration does not alter its value type or storage.
- Authored animation time is exact. Fixed keys use one curve clock, fixed anchors increase strictly,
  and the first and last keys cannot rove.
- Values and tangents are finite and bounded to 64 components. Curves are nonempty and bounded to
  100,000 keys. Every key uses one component width and every tangent matches it.
- Roving keys retain authored roving identity, receive distinct deterministic integer ticks, and
  recompute after every edit, retime, and reload. Resolved times are derived data only.
- Interpolation belongs to the key leaving a segment. Hold retains the left value until the exact
  right key, cubic tangents are independent and measured per second, and easing changes normalized
  segment time separately from value-graph slopes.
- Expression source is editable and bounded, and only `time` and `value` may be referenced. There is
  no I/O, mutation, function call, loop, recursion, dynamic lookup, or host script escape.
- Public animation edits never mutate an existing curve or publish partially checked state. Exact
  retiming rejects inexact fixed-key maps, unrepresentable endpoints, overflow, and invalid ranges.
- Curve serialization records authored state only, denies unknown fields, checks its schema
  revision, and routes every decoded value through public validation before publication.
- Current code performs no pixel processing, GPU submission, ROI execution, cache integration,
  spatial path geometry, timeline sampling, engine playback, project autosave, mask behavior,
  plugin containment, or text rendering.

## Tests and verification

Six authoring integration tests construct and inspect one typed animatable definition, prove
canonical discovery and immutable earlier snapshots, instantiate the same definition in independent
timeline-role and node-graph-role `EditableGraph` values, mutate ordinary parameters, and observe
equal results. They compile both snapshots through one exact `Send + Sync` factory, reject missing
factories and same-ID schema drift, and prove metadata, type, binding, override, registration, and
factory failures leave authoritative state unchanged.

Eight animation integration tests prove exact scalar and vector linear sampling, outgoing holds,
cubic Hermite tangents, cubic Bezier mapping and y overshoot, endpoint clamping, interior roving
allocation, stable zero-distance fallback, narrow-span rejection, bounded time expressions, source
and variable inspection, syntax and runtime failures, and host-escape rejection. They also prove
immutable insert, replace, remove, and expression edits, exact retiming and tangent scaling,
inexact-map rejection, invalid authored state, strict standalone schema behavior, unknown-field
rejection, and checked reconstruction.

The animation consumer proof creates the payload through a real animatable authoring definition,
stores the resulting node in `EditableGraph`, serializes the complete graph document, reloads it
through graph validation, compares canonical bytes, and obtains identical samples. A separate graph
integration test proves `ScalarExpression` independently of graph parameter bindings. Focused and
crate-wide tests, warnings-denied Clippy, rustdoc, complete workspace tests, the full repository
verifier, map validation, dependency checks, fixtures, platform codec consumers, and frontend gates
are the delivery floor.

## Current status and risks

The authoring SDK and keyframe animation module are substantive and test-backed. Effects can define,
discover, instantiate, and translate caller-supplied graph-native definitions, and an animatable
parameter can retain a strict curve payload across generic graph reload. There are still no built-in
effect definitions or production runtime factories, so this work does not render a visual result.

Animation curves lack stable project-level property identities, spatial motion paths, a graph-editor
UI, a production timeline consumer, and an engine evaluation context. Authoring metadata remains
in-memory and has no independent serialization contract. Control hints do not yet carry bounds,
choice options, units, grouping, conditional visibility, or accessibility policy. Runtime factories
are exact-version bound but have no plugin discovery, GPU device, cache, or lifecycle integration.

Mask, transition, text, tracking, and OFX modules remain skeletons. The timeline-role authoring proof
uses an ordinary graph because production effect attachment does not exist. The generic graph reload
proves persistence and editability, not project autosave, rendered pixels, or engine playback.

## Maintenance notes

Preserve the one-way effects-to-graph dependency and keep authored values in ordinary graph state.
Keep animation property meaning with node schemas and exact time ownership in core. Preserve checked
immutable editing, the authored-versus-derived timing split, bounded expressions, exact schema
matching, atomic catalog publication, and workflow-neutral instances.

When concrete nodes or production consumers arrive, record their schemas, types, presentation,
property identities, caller-time path, ROI and color behavior, factory implementations, GPU or CPU
resource ownership, cache identity, serialization owner, migration and reload behavior, and real
timeline, engine, headless, UI, project, and rendered consumers. Update the graph consumer map and
global index whenever effects uses another graph surface. Never report registered schemas, factory
translation, or graph reload as pixel execution without an exercised implementation and real output.
