---
module_id: superi-effects
source_paths:
  - open/crates/superi-effects
source_hash: 797aad844fd82b11bb646d153d8e6632f6d749165cafff994e33f02f8d7e672c
source_files: 22
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-effects` owns the higher-tier open visual effect authoring, animation, reusable control,
mask, rotoscoping, transition, built-in operation, and bounded reference-evaluation layer above the
generic graph. It provides inspectable typed definitions, ordinary editable graph-node
instantiation, deterministic discovery, exact-schema runtime factory translation, exact keyframe
animation, graph-native links and parent controls, bounded animated closed cubic masks with complete
controls and ordered soft-alpha composition, editable exact-frame rotoscope artifacts with
solver-independent propagation hooks, and reusable cross-dissolve and directional-wipe schemas with
exact handle-to-progress conversion. It also provides concrete schemas plus real reference pixels
for transform, crop, opacity, blend, composite, Gaussian blur, sharpen, radial distortion, chroma
key, invert, grade, cross dissolves, and directional wipes.

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
persistence, UI, mask rasterization, feather and expansion filtering, propagation solvers,
production transition attachment and GPU execution, text, tracking, and OFX hosting remain absent
or staged in their owning modules.

## Source inventory

- `open/crates/superi-effects/Cargo.toml`: Declares approved downward dependencies on
  `superi-core`, `superi-gpu`, `superi-image`, and `superi-graph`. It uses workspace Serde for the
  animation, mask, and rotoscope wires, `half` for checked binary16 reference conversion, and JSON
  only in tests.
- `open/crates/superi-effects/src/authoring.rs`: Implements presentation metadata, typed effect
  definitions, graph-native instance construction, atomic generic catalog snapshots, classified
  validation, runtime factories, the shared presentation-text validator, and the graph
  `NodeCompiler` adapter.
- `open/crates/superi-effects/src/catalog.rs`: Implements the stable built-in kind and family enums,
  exact versioned authoring definitions and schemas, typed ports and animatable parameters,
  inspectable controls and defaults, deterministic discovery, GPU capability declarations, atomic
  graph registration, and caller-owned instance creation.
- `open/crates/superi-effects/src/control.rs`: Implements the host animation-value projection seam,
  exact-time parameter evaluation, scalar animation expression conversion, inspectable reusable
  controls, checked parent expressions, canonical control relationships, and revision-bound rig
  compilation into ordinary graph parameter-driver mutations.
- `open/crates/superi-effects/src/keyframe.rs`: Implements checked animation values, independent
  value tangents, fixed and roving timing, linear, cubic, and hold interpolation, cubic Bezier
  easing, bounded time expressions, immutable curve editing, exact uniform retiming, deterministic
  evaluation, and the revisioned standalone wire.
- `open/crates/superi-effects/src/lib.rs`: Documents the implemented authoring, animation,
  control, mask, rotoscope, and transition foundations and publicly exports them with the built-in
  catalog, reference evaluator, and staged visual feature modules.
- `open/crates/superi-effects/src/mask.rs`: Implements animated closed cubic mask paths, fill rules,
  complete checked controls, immutable topology, control, and stack edits, exact-time sampling,
  deterministic soft-coverage boolean composition, and the strict revisioned mask-stack wire.
- `open/crates/superi-effects/src/ofx.rs`: Placeholder for an additive OFX-compatible plugin
  surface.
- `open/crates/superi-effects/src/reference.rs`: Implements immutable effect and transition state,
  conservative ROI mapping, canonical image and shared transition-window validation, bounded
  binary16 and binary32 CPU pixel operations, editable-snapshot runtime compilation, graph
  evaluation, deterministic introspection, and cache fingerprints.
- `open/crates/superi-effects/src/rotoscope.rs`: Implements bounded exact-frame span identities,
  generic authored base masks and corrections, inspectable derived propagation, directional request
  construction, revision-fenced solver hooks, immutable editing and invalidation, strict versioned
  persistence, and checked reconstruction.
- `open/crates/superi-effects/src/text.rs`: Placeholder for additive text and motion-design
  primitives.
- `open/crates/superi-effects/src/tracking.rs`: Placeholder for motion-tracking data and solving.
- `open/crates/superi-effects/src/transition.rs`: Implements stable cross-dissolve and
  directional-wipe kinds, exact versioned definitions and graph schemas, caller-owned instance
  construction, animatable progress, direction, and softness parameters, atomic registration,
  stable wipe direction choices, and exact handle timing mapped to clamped progress without taking
  timeline editorial ownership.
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
- `open/crates/superi-effects/tests/control_contract.rs`: Proves exact-time animated parameter
  resolution, chained parent controls, shared targets, lossless multi-component links, canonical
  inspection, scalar failure boundaries, cross-workflow and editor-script-headless parity, graph
  document reload, editable driver clearing, classified rig validation, atomic cycle rollback, and
  compilation of one reusable parent rig through real built-in visual state across host payloads.
- `open/crates/superi-effects/tests/keyframe_contract.rs`: Proves exact evaluation, interpolation,
  easing, tangents, holds, roving allocation, expressions, immutable edits, retiming, invalid state,
  strict standalone persistence, authoring integration, and real generic graph reload.
- `open/crates/superi-effects/tests/mask_contract.rs`: Proves cubic path inspection, exact-time
  control sampling, immutable topology, mask-control, and stack edits, every boolean alpha
  operation, invalid and hostile-state rejection, strict standalone persistence, reusable control
  linking, and ordinary timeline-role and node-graph-role mutation plus canonical graph reload.
- `open/crates/superi-effects/tests/reference_contract.rs`: Exercises real pixels for every
  operation category, binary16 and binary32 retention, extended RGB, metadata, premultiplied
  algebra, ROI, monotonic distortion, unsupported image meaning, invalid state, and final plus
  temporary resource ceilings.
- `open/crates/superi-effects/tests/rotoscope_contract.rs`: Proves forward and backward propagation
  targets, directional anchors, injected hook execution, source provenance, correction precedence,
  immutable span and base editing, exact invalidation, stale and malformed result rejection, bounded
  state, strict persistence, `GraphValue::Domain`, and real editable graph reload.
- `open/crates/superi-effects/tests/transition_contract.rs`: Proves exact handle timing, stable
  transition discovery, typed animatable parameters, atomic registration, caller-owned bindings,
  premultiplied dissolve and four-direction wipe pixels, soft edges, common display windows, ROI and
  tile stability, real graph evaluation, introspection, cache identity, immutable revisions,
  cross-workflow reuse, and invalid choice rejection.

## Public surface

The library exports `authoring`, `catalog`, `control`, `keyframe`, `mask`, `ofx`, `reference`,
`rotoscope`, `text`, `tracking`, and `transition`.

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

`control` exposes the following implemented animation and rigging contracts:

- `ParameterAnimationValue` lets a host-owned parameter payload produce one checked
  `AnimationValue` at exact `RationalTime`. `AnimationCurve` samples its authored curve, while an
  `AnimationValue` is a time-invariant implementation suitable for host value enums.
- `evaluate_animated_parameter` checks the requested schema is animatable and uses
  `GraphSnapshot::evaluate_parameter_with` to sample only reached undriven literals before the
  graph resolves links and expressions. Its `ParameterEvaluation<AnimationValue>` retains the
  graph-owned deterministic dependency-completion order.
- `AnimationValue` implements graph `ExpressionParameterValue`. Exactly one component may cross
  the numeric expression boundary; direct links preserve every component without conversion, and
  multi-component expression input fails with classified context.
- `ReusableControl` combines one rig-local `ParameterName`, required label and summary,
  `ParameterControl` presentation hint, and exact typed `ParameterReference` to an ordinary graph
  parameter.
- `ParentExpression` compiles bounded editable source through `ScalarExpression` and requires both
  explicit `parent` and `local` variables. `ControlRelationship` stores either a lossless link or a
  parent relationship targeting one exact `ParameterAddress`.
- `ParameterControlRig` canonicalizes controls by name and relationships by target, rejects
  duplicate or missing control intent, inspects source and target schemas for exact type and
  animatable declarations, and creates one revision-bound `GraphTransaction` containing only
  `SetParameterDriver` mutations. Applying that transaction leaves cycle, stale-revision, and
  failure atomicity with the canonical graph owner.

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

`mask` exposes the following implemented authoring and composition contracts:

- `MaskFillRule` retains nonzero or evenodd winding, and `MaskBooleanOperation` selects replace,
  union, subtract, intersect, or exclude in explicit stack order.
- `MaskVertex` stores a core-owned finite `Point2` anchor and relative `Vector2` handles.
  `MaskCubicSegment` exposes absolute start, controls, and end for caller-owned rasterization.
- `MaskVertexAnimation` checks one six-component `AnimationCurve` for anchor and handle coordinates.
  `MaskPathAnimation` retains one closed contour with a fixed fill rule and common clock, samples
  explicit closing cubic segments, and provides checked immutable fill-rule replacement plus vertex
  insertion, replacement, and removal. Paths are bounded from 3 through 4,096 vertices.
- `Mask` owns one animated path plus scalar feather radius, signed expansion, normalized opacity,
  hold-interpolated inversion, and one boolean operation. Construction validates authored values,
  component widths, and clocks; sampling rechecks expression and interpolation output before
  publishing `MaskSample`. Immutable replacement methods rebuild every path and control edit through
  that same checked constructor.
- `MaskStack` bounds canonical order to 256 masks, requires one clock for nonempty state, provides
  immutable mask edits, and samples to `MaskStackSample`. Empty state means full unmasked coverage.
- `MaskStackSample::compose_rasterized_coverages` accepts one normalized caller-rasterized path
  coverage per sample after fill, expansion, and feather. It applies inversion and opacity, then
  deterministic Porter-Duff replace, source-over union, destination-out subtraction, source-in
  intersection, or XOR exclusion without claiming rasterization or pixels.
- `MASK_STACK_SCHEMA_REVISION` identifies the strict standalone stack wire. Private wire records
  deny unknown fields and reconstruct every nested curve, vertex animation, path, mask, and stack
  through checked public constructors.

`rotoscope` exposes editable exact-frame mask propagation state:

- `RotoscopeSpanId`, `RotoscopeFrame<T>`, and `RotoscopeSpan<T>` retain stable identities, one
  half-open exact-time range, a complete generic authored base payload, strictly ordered corrections,
  and separately inspectable derived samples without owning mask geometry.
- `RotoscopeArtifact<T>` canonicalizes non-overlapping spans on one core-owned `Timebase`, advances a
  monotonic content revision, resolves base and corrections above propagation, and provides immutable
  add, replace, remove, base, correction, and derived-result clearing operations.
- `PropagationRequest<T>` exposes the base plus directional correction anchors and every exact
  non-authored target in traversal order. `RotoscopePropagator<T>` is the engine-neutral hook, while
  `PropagationResult<T>` requires complete ordered coverage and retains the source revision, span,
  direction, range, anchors, and targets for atomic application.
- `RotoscopeFrameSource` and `ResolvedRotoscopeFrame<T>` preserve visible provenance. The strict
  `ROTOSCOPE_ARTIFACT_SCHEMA_REVISION` wire denies unknown fields, bounds span and frame collection
  decoding, and reconstructs all state through checked ranges, clocks, ordering, overlap, and size
  validation.

`transition` exposes reusable graph-native visual transitions without duplicating editorial state:

- `TransitionKind` discovers exact `1.0.0` cross-dissolve and directional-wipe schemas in stable
  presentation order. `TransitionCatalog` returns their authoring definitions, registers both
  graph schemas atomically, and instantiates ordinary `EditableNode<GraphValue<T>>` values from
  caller-owned `TransitionNodeBindings` and `TransitionParameterBinding` identities.
- Every transition has required `from` and `to` image inputs, one `result` image output, and an
  animatable normalized `progress` parameter. Directional wipes additionally expose the stable
  `WipeDirection` choice vocabulary and animatable normalized `softness`.
- Transition definitions declare exact ACEScg, current-frame, deterministic input-bounds,
  per-region semantics and require `superi.render.gpu`, while the bounded CPU reference remains an
  oracle rather than a production fallback.
- `TransitionTiming` checks one exact core-owned edit clock, nonempty combined handles, and bounded
  coordinates. It retains the cut plus from and to offsets, exposes the exact half-open range from
  `cut - from_offset` through `cut + to_offset`, and maps caller time to clamped progress.

`reference` exposes bounded executable proof:

- `ReferenceEffectState` plus sampling, blend, and Porter-Duff enums retain exact compiled effect
  and transition state; directional-wipe state uses the transition module's stable
  `WipeDirection`. `required_input_regions` calculates state-dependent conservative dependencies in
  semantic input order.
- `evaluate_reference` accepts premultiplied, unqualified RGBA ACEScg in `Rgba16Float` or
  `Rgba32Float`. Results retain color tags, channel identity, metadata, representation, and display
  window without clamping extended scene-linear RGB.
- `ReferenceEffectNode`, `compile_reference_node`, and the limits-aware compiler translate an exact
  immutable editable effect or transition snapshot into graph `EvaluateNode<Image>` and
  `IntrospectNode` behavior. Transitions require a shared display window, fingerprint resolved
  progress and discrete choices, and blend premultiplied channels with exact endpoint behavior.

The three remaining feature modules expose no substantive public types or behavior.

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

The rotoscope flow is:

1. A caller creates generic complete mask payloads at one exact frame clock, assigns each independent
   non-overlapping span a stable ID and base frame, then stores corrections as canonical authored
   state separate from derived propagation.
2. Forward requests traverse increasing coordinates and backward requests traverse decreasing
   coordinates. Both expose the base followed by same-direction corrections as anchors and omit
   authored frames from the exact target sequence.
3. A tracking or local inference implementation receives only the immutable bounded
   `PropagationRequest<T>` and returns a complete `PropagationResult<T>`. Application rejects stale
   revisions, absent spans, changed ranges or anchors, partial, duplicate, reordered, or wrong-clock
   output before publishing any change.
4. Correction edits invalidate only their directional tail. Repropagation replaces only derived
   samples on that side, while the base, all corrections, opposite-direction samples, generic mask
   layering, and composition payload remain intact and inspectable.
5. The strict standalone artifact wire and ordinary generic graph persistence preserve authored and
   derived state. The real consumer test stores the artifact in an animatable effect parameter and
   `GraphValue::Domain`, reloads the graph document, and obtains canonical bytes and equal resolved
   frames.

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

The transition flow reuses that same authoring and evaluation path while preserving timeline law:

1. `superi-timeline` remains the owner of transition identity, adjacency, source handles, record
   placement, grouping, synchronization, edit reconciliation, serialization, and graph compilation.
   It has no dependency on effects.
2. A higher integration owner selects one `TransitionKind`, allocates stable graph identities, and
   projects timeline-owned endpoint and timing intent into the catalog's ordinary `from`, `to`,
   `result`, and animatable parameter bindings. No effect-owned timeline list or topology is created.
3. `TransitionTiming` converts the exact timeline cut and handles into a half-open visual range and
   host-owned progress without rescaling or rounding clocks. Graph drivers may animate the same
   declared parameters because they remain ordinary typed graph state.
4. Reference compilation requires exact transition schema equality, resolves all graph drivers from
   one immutable snapshot, parses stable wipe choices, validates normalized domains, and includes
   schema, semantic port identities, progress, direction, and softness in node fingerprints.
5. Cross dissolve linearly interpolates every premultiplied RGBA channel. Directional wipe derives
   tile-independent pixel-center coordinates from the shared display window, supports four stable
   directions and a normalized smooth band, and guarantees exact all-from and all-to endpoints.
6. Both semantic inputs request the exact output region. The real `GraphEvaluationSnapshot`
   contract proves evaluation, diagnostics, cache changes, old-snapshot isolation, and identical
   behavior in independent ordinary graphs representing timeline and node-graph workflows.

The reusable-control flow composes animation and built-in state with existing graph drivers:

1. A host stores animatable literals in ordinary typed graph parameters and implements
   `ParameterAnimationValue` for any payload that needs exact-time sampling. `AnimationCurve` is
   the built-in concrete proof.
2. A `ReusableControl` names and presents one exact animatable `ParameterReference`. A
   `ParameterControlRig` validates every source and target against one immutable graph revision.
3. Link relationships become lossless `ParameterDriver::link` state. Parent relationships bind the
   editable `parent` plus `local` source to exact references and become
   `ParameterDriver::expression` state. Canonical target order makes the emitted transaction
   deterministic.
4. `EditableGraph::apply` remains authoritative for exact dependencies, replacement, dependency
   cycles, rollback, and publication. Graph documents persist the resulting link and expression
   source without serializing a second rig hierarchy.
5. At one exact time, `evaluate_animated_parameter` asks graph to project only reached literal
   payloads into sampled `AnimationValue` values. Graph performs the same request-local dependency
   traversal for editor, script, headless, timeline-role, and node-graph-role consumers.
6. The same rig can target concrete catalog nodes stored in `GraphValue<T>`. Reference compilation
   resolves those ordinary graph drivers into built-in visual state without a control-specific
   runtime branch or a dependency on the host domain payload.

The implemented foundations compose without another ownership layer. The animation integration
builds an animatable `EffectParameterDefinition<AnimationCurve>`, stores the resulting node in an
`EditableGraph`, and preserves it through strict graph reload. Graph remains unaware of effect
metadata, keyframes, interpolation, control presentation, and time-expression variable meaning.

The mask authoring and composition flow is:

1. A caller builds each stable vertex slot from one six-component animation curve. Core `Point2`
   and `Vector2` values are reconstructed at sample time, while `MaskPathAnimation` owns contour
   order, closure, fill rule, vertex bounds, immutable topology edits, and one shared exact clock.
2. `Mask::new` composes that path with scalar animation curves for nonnegative feather pixels,
   signed expansion pixels, normalized opacity, and a hold-interpolated zero-or-one inversion
   toggle. It rejects mixed clocks, illegal authored ranges, and wrong component widths.
3. `Mask::sample` evaluates every curve at the same exact physical time, rejects expression or
   easing overshoot, converts relative handles to explicit closed cubic segments, and publishes
   inspectable geometry and controls without allocating an image or GPU resource.
4. A future runtime rasterizer applies the sampled path, winding, expansion, and feather and returns
   normalized coverage. `MaskStackSample` applies inversion and opacity, then combines ordered soft
   coverage through explicit Porter-Duff equations. The API cannot be mistaken for a pixel renderer.
5. Immutable path, mask-control, and stack edits reconstruct the complete checked artifact. The
   strict revisioned stack wire stores authored curves and contour order only, denies unknown or
   future state, and rebuilds every nested owner through its constructor.
6. The mask integration declares `GraphValue<MaskStack>` as one animatable effect parameter, wraps
   each stack as exact domain state, and mutates a source in two independent ordinary graphs
   representing timeline and node-graph roles. A `ParameterControlRig` links the complete stack to a
   reusable target through ordinary driver state before both graphs serialize and reload. Equal
   exact-time samples and canonical bytes prove workflow reuse without adding mask knowledge to
   graph.

## Dependencies and consumers

- `superi-core` supplies errors, diagnostics context, finite geometry, color and alpha semantics,
  capability sets, semantic versions, `Point2`, `Vector2`, `RationalTime`, `Timebase`, exact
  rescaling, and stable primitive serialization.
- `superi-graph` supplies schemas, registries, neutral `GraphValue<T>`, typed editable state,
  mutation, parameter evaluation and projected literal evaluation, typed parameter drivers,
  bounded scalar expressions, immutable runtime compilation, lazy evaluation, diagnostics, cache
  identity, and generic graph persistence. Graph never depends on effects.
- `superi-image` supplies immutable image artifacts, metadata, exact crop and transform operations,
  sample representations, and finite limits.
- `superi-gpu` is a declared production capability dependency. Current effects source uploads,
  owns, and evaluates no GPU resource.
- Serde owns strict animation, mask-stack, and rotoscope records. JSON is test-only. `half` performs
  checked reference conversion to and from binary16.
- `superi-timeline` does not depend on effects. It compiles native state into graph-owned
  `GraphValue<TimelineGraphValue>`, including stable transition endpoint, identity, and handle
  parameters. A higher integration owner can pair that editorial projection with the effects-owned
  transition definitions without reversing the dependency or copying timeline mutation policy.
- `superi-engine` declares `superi-effects` but has no production catalog, animation, evaluator,
  playback, viewport, or export call site. Current real consumers are the role-neutral authoring,
  generic graph reload, reusable controls over shared processing payloads, strict animation and
  mask payloads, strict rotoscope artifacts, transition authoring and timing, and bounded headless
  graph-evaluation contracts.
  Mask and transition tests label independent ordinary graphs as timeline and node-graph roles
  without claiming production timeline attachment.

## Invariants and operational boundaries

- Effects depends down on graph, image, GPU, and core. None of those modules depends on effects.
- Effect authoring composes the canonical graph. It owns no competing schema, DAG, parameter driver,
  transaction, snapshot, identity, evaluator, scheduler, or serialization envelope.
- Definitions and animation curves are immutable after construction. Exact schema identity includes
  node type and semantic version, and full schema equality is checked before runtime projection.
- Labels, summaries, and categories cannot be blank. Defaults and overrides must match their exact
  graph `ValueTypeId`; unknown and duplicate authoring state is rejected with classified context.
- Every instance identity belongs to the caller and is validated against every schema declaration.
  Defaults become ordinary editable parameter payloads, and runtime factories own no hidden state.
- Discovery, definition, port, parameter, catalog, and schema iteration is deterministic. Batch
  registration is atomic, earlier snapshots remain immutable, and failures cannot publish partial
  state.
- Every result-affecting built-in parameter is typed, inspectable, editable, and animatable.
  Discrete choices remain bounded choice variants rather than numeric coercions.
- Transition visual state is graph-native and workflow-neutral. Timeline retains endpoint identity,
  adjacency, source and record timing, grouping, synchronization, persistence, and mutation policy;
  effects retains only reusable schemas, parameter meaning, exact handle-to-progress conversion,
  and bounded visual semantics.
- Transition timing uses one exact core clock, requires a nonzero combined handle range, checks both
  range endpoints, and clamps progress only after exact integer-coordinate comparison. No implicit
  timebase rescale or timeline-owned identity is hidden in `TransitionTiming`.
- Cross-dissolve and wipe progress plus wipe softness are finite and normalized. Wipe direction is
  one stable choice, both inputs share canonical pixel, channel, color, alpha, and display-window
  meaning, and spatial coordinates derive from the display window rather than the requested tile.
- Transition endpoints are exact even with a soft wipe. Every premultiplied RGBA channel uses the
  same interpolation weight, and both semantic input bindings participate in deterministic
  fingerprints and same-region dependencies.
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
- Fixed animation time is exact and increasing. Curves require fixed endpoints, values and tangents
  are finite and bounded, roving times are derived and distinct, and exact retiming rejects inexact
  maps. Expression source is editable and bounded to `time` and `value` without I/O, mutation,
  loops, recursion, calls, or dynamic lookup.
- Rotoscope spans are nonempty, bounded, uniquely identified, non-overlapping, and use one exact
  artifact timebase. Base, correction, and propagation coordinates are in range; correction and
  propagation sequences are unique and increasing; authored frames never overlap derived samples.
- Propagation is replaceable derived state. Requests are bounded and directionally ordered, results
  cover the target sequence exactly, revision and request identity are checked atomically, and manual
  corrections always resolve above propagated samples and survive repropagation.
- Workflow parity is structural. Timeline and node-graph roles receive no role flag or hidden state
  branch, and old immutable graph revisions cannot observe later direct edits.
- Reusable controls are typed references to ordinary animatable parameters. Control and relationship
  iteration is canonical, every relationship target is unique, and rig compilation emits only
  ordinary graph-driver mutations against one exact revision.
- Parent composition uses only explicit `parent` and `local` scalar variables. Direct links retain
  complete multi-component payloads; numeric expressions reject multi-component inputs instead of
  coercing or discarding components.
- Sampled animation values are request-local results. The rig retains no graph revision, evaluated
  value, dependency cache, workflow branch, or parallel hierarchy, and graph mutation remains the
  authority for type checks, cycles, replacement, and rollback.
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
- Mask path topology is explicit and bounded. Every vertex uses one six-component curve on one
  clock, sampled handles remain relative until checked conversion, and every contour is closed with
  at least three vertices.
- Mask feather, expansion, opacity, and inversion are inspectable curves. Authored and sampled
  values are checked, inversion uses hold interpolation, and every nonempty stack uses one clock.
- Mask ordering is canonical and bounded. Boolean composition accepts only finite normalized
  caller-rasterized coverage and uses explicit soft-alpha equations. Empty state means no mask and
  therefore full coverage.
- Mask serialization records authored state only, denies unknown fields, checks its schema
  revision, and rebuilds all nested state through checked constructors before publication.
- Current code performs bounded reference pixel processing and ROI calculation, but no production
  GPU submission, cache integration, mask path rasterization, feather or expansion filtering,
  production timeline sampling or transition attachment, engine playback, project autosave,
  propagation solver, plugin containment, or text rendering. The reference oracle is not a
  production route.

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

Five transition tests prove exact cut and handle timing, clamped progress, mixed-clock and overflow
rejection, stable kind and direction discovery, exact schemas, metadata, ports, animatable controls,
typed defaults, GPU capability, caller-owned identities, atomic registration, invalid binding and
choice rejection, cross-dissolve endpoints and midpoint, four wipe directions, soft bands, shared
display-window validation, same-region dependencies, tile stability, and real immutable graph
evaluation with introspection, semantic cache changes, old-revision isolation, and independent
timeline-role and node-graph-role reuse.

Five control integration tests prove inspectable canonical controls and relationships, exact-time
curve projection, chained scalar parenting, one child control reused by multiple targets, lossless
two-component links, explicit nonscalar expression rejection, equal timeline-role and node-graph
state, equal editor-script-headless samples, canonical graph reload, driver clearing, duplicate and
missing intent rejection, animatable and exact-type enforcement, graph-owned cycle rollback, and
parent-control compilation through real built-in opacity state across two host payload domains.

The animation consumer proof creates the payload through a real animatable authoring definition,
stores the resulting node in `EditableGraph`, serializes the complete graph document, reloads it
through graph validation, compares canonical bytes, and obtains identical samples. A separate graph
integration test proves projected literal evaluation without copying driver traversal.

Seven rotoscope tests prove exact forward and backward requests, ordered anchors, real hook
execution, provenance, correction precedence and directional invalidation, immutable span and base
editing, propagation clearing, stale and malformed output rejection, bounded construction, strict
wire reconstruction, generic mask payload retention, animatable effect authoring, `GraphValue`
reuse, and canonical graph reload.

Focused tests, crate-wide tests, warnings-denied Clippy, rustdoc, formatting, dependency and offline
boundary checks, map validation, complete workspace tests, fixtures, platform codec consumers,
frontend gates, and the full repository checkpoint verifier form the delivery floor.

Six mask integration tests prove sampled cubic anchors and controls including closure, nonzero and
evenodd state, animated feather, expansion, opacity, and inversion, immutable fill-rule, vertex,
mask-control, operation, and stack edits, bounds, every boolean soft-coverage equation, empty-stack
meaning, invalid raster coverage, authored control rejection, sampled expression overshoot
rejection, and hold-only inversion. They also prove the strict standalone wire rejects future,
unknown, and invalid nested state, and that
the same animatable `GraphValue<MaskStack>` domain payload survives ordinary mutation, reusable
control linking, and canonical generic graph reload in independent timeline-role and node-graph-role
graphs.

## Current status and risks

The authoring SDK, exact keyframe animation, reusable graph-native control rigs, animated mask
authoring and composition, editable rotoscope artifacts and propagation hooks, built-in definitions,
generic editable instantiation, deterministic CPU reference pixels, ROI mapping, immutable graph
compilation, introspection, animated mask authoring and composition, reusable transition definitions,
exact transition timing, bounded transition pixels, and role-neutral graph proofs are substantive and
test-backed. Strict curve, mask-stack, and rotoscope payloads retain authored state across generic
graph reload.
The reference implementation is scalar and allocation-bounded, not performance production code,
and masks have no rasterizer or rendered consumer.

There is no GPU shader parity, engine registry, production runtime catalog, timeline attachment,
playback, viewport, export, project persistence, UI, spatial motion path beyond mask contours, mask
rasterization, propagation solver, text, tracking solver, production transition attachment, or OFX
host. Rotoscope mask payloads are generic and have no production mask-type consumer yet. Authoring
metadata is in memory and has no independent wire. Control hints do not yet encode enforceable
numeric bounds, choice option vocabularies, grouping, conditional visibility, or accessibility
policy; transition domains and wipe choices are therefore validated by the reference compiler.
Animation has no stable project-level property identity or production caller-time context.

Reusable control presentation and rig definitions remain in-memory authoring descriptions, while
their applied driver meaning is persisted by graph. Parent expressions are scalar only; transform
matrix composition and spatial paths remain later domain work. Runtime factories are exact-version
bound but have no plugin discovery, GPU device, cache, or lifecycle integration.

The CPU evaluator proves implementation semantics and graph integration but does not close a
production import-to-render path. The `superi.render.gpu` requirement deliberately prevents it from
being mistaken for production execution.
Mask stack edits currently use canonical vector indexes rather than future project-stable mask IDs.
Contour topology changes are discrete rather than interpolated. Fill, feather, and expansion are
sampled authoring inputs, but a later runtime still owns rasterization, ROI, filtering, image and GPU
values, caching, and pixels. The timeline-role mask proof uses an ordinary graph because production
effect attachment does not exist. Generic graph reload proves persistence and editability, not
project autosave, rendered pixels, or engine playback.
Transition definitions likewise have no standalone wire or production timeline binder. Timeline
already preserves editorial transition state and compiles neutral graph parameters, while the
effects contract proves reusable visual nodes in ordinary graphs; the higher integration that maps
between those contracts is intentionally still absent.

## Maintenance notes

Preserve the one-way effects-to-graph dependency and keep authored values in ordinary graph state.
Keep animation property meaning with node schemas and exact time ownership in core. Preserve checked
immutable editing, the authored-versus-derived timing split, bounded expressions, exact schema
matching, atomic catalog publication, workflow-neutral instances, request-local literal sampling,
canonical rig ordering, graph-owned driver state, canonical image meaning, and bounded reference
allocation. Keep rotoscope bases and corrections canonical, propagation derived, every request and
wire collection bounded, revisions fenced, exact clocks unchanged, and generic mask payloads
uninterpreted by the temporal layer. Never store a second effects-only dependency graph, evaluated
control cache, or solver-owned rotoscope state.

Add built-in kinds only through one versioned authoring definition with typed defaults, presentation,
complete caller-owned binding validation, state compilation, ROI mapping, reference pixels,
fingerprint coverage, and real graph consumer tests. Keep formulas and image rules aligned across
future GPU implementations, and compare shaders against the oracle without moving GPU ownership
into effects or adding an implicit CPU fallback.

Keep mask geometry in core points and vectors, retain relative handles and explicit closed contour
order, and reconstruct authored curves and every immutable control replacement through their
existing checked owners. Preserve the clear boundary between sampled mask state and caller-owned
rasterization. Future rasterizers must consume fill rule, expansion, feather, inversion, opacity,
and boolean ordering without hiding edits or claiming a new persistence or graph owner.

Keep transition schemas versioned, workflow-neutral, and limited to visual parameter meaning. Do
not move timeline identity, adjacency, handles, record placement, grouping, synchronization,
serialization, or edit reconciliation into effects. Preserve exact-clock handle conversion,
closed progress domains, stable wipe choice codes, common display-window coordinates, exact
endpoints, premultiplied interpolation, same-region dependencies, semantic fingerprints, and tiled
parity. Future GPU implementations must match the bounded oracle and use the same graph parameters.

When production consumers arrive, record property identities, caller-time flow, GPU resource
ownership, cache behavior, serialization and migration ownership, timeline attachment, project
reload, engine registration, viewport, headless, and export consumers. Update the graph, timeline,
engine, workspace, and global maps whenever those contracts or relationships change. Never report
registered schemas, factory translation, mask composition, or graph reload as production pixel
execution without an exercised implementation and real output.
