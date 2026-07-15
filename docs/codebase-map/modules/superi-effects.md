---
module_id: superi-effects
source_paths:
  - open/crates/superi-effects
source_hash: 45f4db66d49319466b514275eb626bd3b0896e0c27b0bea5d00cc350a3d15756
source_files: 31
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-effects` owns the higher-tier open visual effect authoring, animation, reusable control,
visual and spatial composition, vector shape, mask, rotoscoping, motion tracking, text, transition, built-in
operation, and bounded reference-evaluation layer above the generic graph. It provides inspectable typed definitions,
ordinary editable graph-node instantiation, deterministic discovery, exact-schema runtime factory
translation, exact keyframe animation, graph-native links and parent controls, bounded animated
open and closed cubic vector paths with fills, strokes, gradients, and repeaters, bounded animated
closed cubic masks with complete controls and ordered soft-alpha composition, strict reusable
composition artifacts with same-composition layer parenting, exact time remapping, reusable
precompositions, explicit collapse boundaries, and complete resolved visual paths. It composes
those owners into strict editable 2D and 3D spatial layers with animated transforms, cameras,
ambient, directional, and point lights, deterministic depth ordering, exact shutter sampling, and
a bounded real-pixel spatial oracle. It also owns editable exact-frame rotoscope artifacts with
solver-independent propagation hooks and strict point,
planar, object, and calibrated known-landmark camera tracking artifacts with manual corrections,
tracked observations, transformed regions, revision-fenced solver results, and deterministic bounded
CPU reference solving. It provides strict
styled UTF-8 authoring, animated typography and paragraph controls, caller-resolved offline fonts,
OpenType shaping, Unicode line breaking and bidi reordering, and inspectable positioned glyphs for
a later raster owner. It also provides reusable cross-dissolve and directional-wipe schemas with
exact handle-to-progress conversion, concrete schemas, and real reference pixels
for transform, crop, opacity, blend, composite, Gaussian blur, sharpen, radial distortion, chroma
key, invert, grade, cross dissolves, directional wipes, and complete planar spatial compositions. It
also owns the safe OpenFX 1.5.1
effect-side host contract: isolated worker adapter validation, plugin and context description,
graph-native schema projection, exact-time parameter sampling, explicit permissions and activation,
instance lifecycle, structured failures, restart, and quarantine.

The generic graph remains authoritative for schema identity, instance identities, typed editable
values, transactions, parameter drivers, immutable snapshots, topology, serialization, evaluation,
diagnostics, and cache identity. Core remains authoritative for exact time, finite geometry, color
meaning, capabilities, and classified errors. Image remains authoritative for canonical image
artifacts and allocation limits. Timeline remains authoritative for editorial nested sequences,
clip time maps, and mutation policy. Effects adds visual meaning around those contracts and never
creates a competing graph, editorial nesting model, timeline effect list, time system, expression
language, or project persistence envelope.

The built-in schemas require the production `superi.render.gpu` capability, but this crate currently
implements only a deterministic bounded CPU oracle and headless proof. Production GPU kernels,
engine catalog registration, timeline effect attachment, playback, viewport, export, project
persistence, UI, production spatial GPU execution, vector shape rasterization, mask rasterization,
feather and expansion
filtering, propagation solvers, production transition attachment and GPU execution, text
rasterization and GPU atlases, pyramidal or GPU tracking acceleration, production tracking
attachment, native OpenFX bundle discovery, worker IPC, and native OFX entry-point execution remain
absent or staged in their owning modules.

## Source inventory

- `open/crates/superi-effects/Cargo.toml`: Declares approved downward dependencies on
  `superi-core`, `superi-gpu`, `superi-image`, and `superi-graph`. It uses workspace Serde for the
  animation, composition, spatial, vector shape, mask, rotoscope, tracking, and text wires, `half`
  for checked binary16 reference
  conversion, Swash and pinned Skrifa for offline OpenType shaping, Unicode Bidi and Unicode
  Linebreak for layout, and JSON only in tests.
- `open/crates/superi-effects/src/authoring.rs`: Implements presentation metadata, typed effect
  definitions, graph-native instance construction, atomic generic catalog snapshots, classified
  validation, runtime factories, the shared presentation-text validator, and the graph
  `NodeCompiler` adapter.
- `open/crates/superi-effects/src/catalog.rs`: Implements the stable built-in kind and family enums,
  exact versioned authoring definitions and schemas, typed ports and animatable parameters,
  inspectable controls and defaults, deterministic discovery, GPU capability declarations, atomic
  graph registration, and caller-owned instance creation.
- `open/crates/superi-effects/src/composition.rs`: Implements exact layer-to-source time maps,
  same-composition parent DAGs, generic visual and precomposition layers, explicit collapse and
  isolation controls, immutable composition and artifact editing, reusable nesting validation,
  complete structural frame resolution with both owning-composition and mapped-source time, and a
  strict bounded revisioned wire.
- `open/crates/superi-effects/src/control.rs`: Implements the host animation-value projection seam,
  exact-time parameter evaluation, scalar animation expression conversion, inspectable reusable
  controls, checked parent expressions, canonical control relationships, and revision-bound rig
  compilation into ordinary graph parameter-driver mutations.
- `open/crates/superi-effects/src/keyframe.rs`: Implements checked animation values, independent
  value tangents, fixed and roving timing, linear, cubic, and hold interpolation, cubic Bezier
  easing, bounded time expressions, immutable curve editing, exact uniform retiming, deterministic
  evaluation, and the revisioned standalone wire.
- `open/crates/superi-effects/src/lib.rs`: Documents the implemented authoring, animation,
  composition, spatial, control, vector shape, mask, rotoscope, tracking, text, transition, and
  OpenFX foundations and publicly exports them with the built-in catalog and reference evaluator.
- `open/crates/superi-effects/src/mask.rs`: Implements animated closed cubic mask paths, fill rules,
  complete checked controls, immutable topology, control, and stack edits, exact-time sampling,
  deterministic soft-coverage boolean composition, and the strict revisioned mask-stack wire.
- `open/crates/superi-effects/src/ofx.rs`: Implements the OpenFX 1.5.1 compatible effect-side host
  contract, validated isolated-worker guarantees, exact lifecycle and standard-context descriptors,
  graph-native clip and finite parameter projection, discovered and active catalogs, explicit-time
  timeline projection before graph expression evaluation, bounded opaque image resources, explicit
  permission grants, instance state, structured adapter failures, restart, and quarantine.
- `open/crates/superi-effects/src/reference.rs`: Implements immutable effect and transition state,
  conservative ROI mapping, canonical image and shared transition-window validation, bounded
  binary16 and binary32 CPU pixel operations, editable-snapshot runtime compilation, graph
  evaluation, deterministic introspection, cache fingerprints, and serializable reconstruction
  choices reused by spatial layer state.
- `open/crates/superi-effects/src/rotoscope.rs`: Implements bounded exact-frame span identities,
  generic authored base masks and corrections, inspectable derived propagation, directional request
  construction, revision-fenced solver hooks, immutable editing and invalidation, strict versioned
  persistence, and checked reconstruction.
- `open/crates/superi-effects/src/shape.rs`: Implements stable animated open and closed cubic paths,
  scene-linear solid and gradient fills, complete stroke geometry and dash state, bounded virtual
  repeaters, immutable edits, exact retiming, renderer-ready sampling, and strict revisioned vector
  shape document persistence.
- `open/crates/superi-effects/src/spatial.rs`: Implements strict reusable spatial composition state
  over `CompositionArtifact<SpatialLayer>`, animated 2D and 3D transforms, right-handed perspective
  and orthographic cameras, ambient, directional, and point lights, layer-stack or camera-depth
  ordering, exact bounded shutter samples, parent and precomposition matrix composition on each
  owning clock, immutable retiming, strict reload, one graph-native definition, and a bounded
  real-pixel CPU oracle built from existing reference image operations.
- `open/crates/superi-effects/src/text.rs`: Implements bounded styled UTF-8 ranges, persistent font
  asset references, OpenType features and animated axes, continuous and hold-discrete typography
  and paragraph controls, immutable text and style edits, exact whole-layer retiming, strict
  versioned persistence, caller-owned offline font resolution, Swash shaping, Unicode line breaks,
  bidi visual ordering, wrapping, indents, alignment, justification, and owned positioned glyphs.
- `open/crates/superi-effects/src/tracking.rs`: Implements exact-bit persisted tracking geometry,
  stable track and feature identities, point, planar-region, object-region, and calibrated
  known-landmark camera selections, authored corrections, revision-fenced derived samples,
  strict bounded persistence, checked transient luma frames, iterative local point registration,
  normalized residual-consensus homography fitting, 2D similarity fitting, and iterative camera
  pose refinement.
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
- `open/crates/superi-effects/tests/composition_contract.rs`: Proves exact speed, reverse, freeze,
  endpoint hold, and explicit-rounding time maps, extreme coordinates, local parent inspection and
  cycle rejection, shared nested instances, both collapse boundary rules, complete mask and effect
  payload paths, immutable edits, strict bounded persistence, and canonical timeline-role plus
  node-graph-role graph reload.
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
- `open/crates/superi-effects/tests/ofx_contract.rs`: Proves scan and activation lifecycle order,
  isolated adapter rejection, exact standard context validation, deterministic graph definitions,
  permission denial, discovered versus active catalogs, timeline projection before graph
  expressions, bounded clip access, canonical graph reload, retained missing-node state, structured
  worker failure, panic containment, recovery, and repeated-failure quarantine.
- `open/crates/superi-effects/tests/reference_contract.rs`: Exercises real pixels for every
  operation category, binary16 and binary32 retention, extended RGB, metadata, premultiplied
  algebra, ROI, monotonic distortion, unsupported image meaning, invalid state, and final plus
  temporary resource ceilings.
- `open/crates/superi-effects/tests/rotoscope_contract.rs`: Proves forward and backward propagation
  targets, directional anchors, injected hook execution, source provenance, correction precedence,
  immutable span and base editing, exact invalidation, stale and malformed result rejection, bounded
  state, strict persistence, `GraphValue::Domain`, and real editable graph reload.
- `open/crates/superi-effects/tests/tracking_contract.rs`: Proves real luma-driven point, planar,
  object, and calibrated camera solving, dominant planar residual consensus, transformed regions,
  authored correction precedence and invalidation, nearest coherent temporal sources, stale and
  malformed result rejection, shared core geometry conversion, strict bounded persistence,
  animatable authoring, `GraphValue::Domain`, and canonical reuse across independent workflow-role
  graphs.
- `open/crates/superi-effects/tests/transition_contract.rs`: Proves exact handle timing, stable
  transition discovery, typed animatable parameters, atomic registration, caller-owned bindings,
  premultiplied dissolve and four-direction wipe pixels, soft edges, common display windows, ROI and
  tile stability, real graph evaluation, introspection, cache identity, immutable revisions,
  cross-workflow reuse, and invalid choice rejection.
- `open/crates/superi-effects/tests/text_contract.rs`: Proves real deterministic shaping from
  reviewed font bytes, mixed bidi runs, animated wrapping, alignment and typography, exact retiming,
  immutable UTF-8 and style edits, strict bounded reload, classified failures, reusable complete
  payload links, two workflow-role graphs, and canonical graph reload.
- `open/crates/superi-effects/tests/shape_contract.rs`: Proves path topology, fills, gradients,
  strokes, dash phase, virtual repeater transforms, direct immutable edits, exact retiming, strict
  standalone persistence, and reusable timeline-role plus node-graph-role graph reload.
- `open/crates/superi-effects/tests/spatial_contract.rs`: Proves strict editable state, exact
  sampling and retiming, 2D and 3D transform composition, parent and precomposition clocks,
  perspective and orthographic cameras, all three light families, deterministic depth overlap,
  exact motion pixels, bounded failures, strict wire validation, and identical sampled plus rendered
  results through independent ordinary graph reloads.

## Public surface

The library exports `authoring`, `catalog`, `composition`, `control`, `keyframe`, `mask`, `ofx`,
`reference`, `rotoscope`, `shape`, `spatial`, `text`, `tracking`, and `transition`.

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

`ofx` exposes the isolated OpenFX effect-side contract without loading native code:

- `OfxHostCapabilities`, `OfxAdapterContract`, and `IsolatedOfxAdapter` make the supported OpenFX
  1.5.1 contexts and finite parameter types, worker-process isolation, protocol revision, bounded
  messages, render deadline, restart guarantee, and typed lifecycle actions inspectable.
- `OfxPluginDescriptor`, `OfxContextDescriptor`, clip and parameter descriptors, plugin
  capabilities, exact identities, and render-thread declarations validate standard mandatory
  clips and host-managed parameters. Exact OpenFX names are retained while deterministic lower-kebab
  graph names are collision checked.
- `OfxPluginHost<A>` scans through load, describe, describe-in-context, and unload with no granted
  permissions. `definition`, `discovered_catalog`, and `active_catalog` project contexts into
  ordinary `EffectNodeDefinition<GraphValue<T>>` values and publish runtime schemas only while the
  plugin is ready.
- `OfxParameterSampler<T>` projects timeline-owned literals at one finite `OfxTime` before
  `GraphSnapshot::evaluate_parameter_with` resolves graph links and expressions. Create and render
  requests retain the exact graph revision, instance key, OpenFX parameter names, render window,
  and read-only input or write-only Output resource tokens.
- Explicit enable, disable, destroy, recover, and quarantine acknowledgement operations enforce
  permissions and lifecycle order. Adapter errors and panics become structured failure records;
  repeated failures quarantine the plugin and failed schemas leave active discovery without
  changing authored graph state.

`composition` exposes reusable visual layering without taking editorial timeline ownership:

- `CompositionId` and `CompositionLayerId` are canonical persisted integer identities.
  `CompositionLayer<T>` retains a required editable name, half-open active range, optional
  same-composition parent, generic complete visual payload, exact `TimeRemap`, and explicit
  `PrecompositionCollapse` plus `LayerIsolation` controls.
- `TimeRemapKeyframe` maps exact layer time to exact source time with outgoing linear or hold
  interpolation. `TimeRemap` requires one clock on each side and strictly increasing layer keys,
  supports forward, reverse, speed, freeze, and endpoint holds, and reports the selected segment
  plus caller-controlled exact, floor, ceil, toward-zero, or nearest-ties-even rounding.
- `Composition<T>` preserves bottom-to-top order, validates unique layer identities and one acyclic
  same-composition parent chain, exposes root-to-direct parent inspection, and provides immutable
  name, range, insertion, replacement, removal, and reorder operations.
- `CompositionArtifact<T>` canonicalizes reusable compositions by identity, validates the root,
  precomposition targets and clocks, nested ranges, and the full composition DAG, then advances one
  checked content revision for immutable add, replace, remove, and root edits.
- `resolve_frame` returns `ResolvedCompositionFrame`, `ResolvedLayer`, and `ResolvedLayerStep`
  values in deterministic bottom-to-top order. Every output retains its complete root-to-leaf
  layer path, generic payloads, exact owning-composition coordinates, mapped source coordinates,
  and local parent chains. A preserved or isolation-forced precomposition remains an explicit
  boundary; a pass-through collapsed instance expands the referenced composition without
  discarding ancestor state.
- `COMPOSITION_ARTIFACT_SCHEMA_REVISION` identifies the strict standalone wire. All records deny
  unknown fields, bound compositions, layers, and remap keys before allocation, use canonical
  integer identities, and reconstruct every local plus nested relationship through checked owners.

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

`tracking` exposes editable motion state and bounded deterministic reference solvers:

- `TrackId`, `FeatureId`, `TrackedFeature`, and `CameraLandmark` retain stable artifact-local
  identities. `TrackingPoint`, `TrackingMatrix3`, and `TrackingRect` persist exact finite binary64
  bits and convert explicitly to and from core-owned `Point2`, `Matrix3`, and `Rect`; `TrackingPoint3`
  represents the camera solver's known world landmarks.
- `TrackingSelection` stores point, planar region, object region, or calibrated camera intent.
  Planar selections require at least four unique image features, object selections require at least
  two, and camera selections require at least six unique noncoplanar known landmarks, positive
  `CameraIntrinsics`, and an inspectable prior `CameraPose`.
- `TrackingModel`, `TrackingObservation`, and `TrackingSample` expose the solved point, homography,
  similarity transform, transformed region, camera pose, feature positions, confidence, and exact
  integer frame coordinate. `TrackingTrack` keeps its authored reference and manual corrections
  separate from replaceable derived samples.
- `TrackingArtifact` canonicalizes tracks on one core-owned `Timebase`, advances immutable content
  revisions, supports complete selection replacement and manual correction edits, resolves authored
  state above derived samples, invalidates only the affected authored segment, and creates requests
  from the nearest coherent available sample.
- `TrackingRequest` and public checked `TrackingResult::new` form the engine-neutral solver seam.
  Application rechecks artifact revision, track, source sample, target frame, model kind, feature
  identity, transformed region, and observation residual before publishing atomically.
- `TrackingFrame` accepts bounded explicit dense finite luma with no hidden image conversion.
  `CpuTrackingSolver` applies iterative Lucas-Kanade point registration with a minimum-eigenvalue
  texture gate, normalized bounded residual-consensus homography fitting, least-squares 2D
  similarity fitting, and bounded Gauss-Newton calibrated pose refinement from known 3D landmarks.
- `TRACKING_ARTIFACT_SCHEMA_REVISION` identifies the strict standalone wire. Unknown and future
  state fail, every nested collection is bounded during decoding, finite bits are rechecked, and
  complete state reconstructs through artifact validation before publication.

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

`spatial` exposes editable planar 2D and 3D composition state plus bounded executable proof:

- `SpatialSourceId`, `LayerSpace`, `LayerTransform`, and `SpatialLayer` retain stable source
  references, 2D or 3D interpretation, independently inspectable anchor, position, XYZ Euler
  rotation, scale, opacity curves, and nearest or bilinear sampling. Every curve uses the owning
  composition clock and is revalidated after interpolation.
- `SpatialCamera` composes animated position, XYZ Euler rotation, near and far clipping, and a
  `CameraProjection` with animated perspective vertical field of view or orthographic vertical
  extent. The camera is right handed, looks down local negative Z, and publishes its sampled view
  matrix and projection without hiding authored state.
- `SpatialLight` supports animated ambient, directional, and point color plus linear intensity.
  Directional orientation and point position and attenuation range remain animated and inspectable.
- `SpatialScene` assigns exactly one camera, bounded light list, `DepthOrdering`, and `MotionBlur`
  record to each reusable composition. Shutter endpoints and samples use exact project-clock ticks,
  include both endpoints, and require an exactly divisible interval.
- `SpatialCompositionArtifact` wraps the canonical `CompositionArtifact<SpatialLayer>`, requires
  exactly one scene per composition, samples every same-composition parent and collapsed nested path
  step on its owning composition coordinate, composes binary64 matrices root to leaf, sorts camera
  depth far to near with stable ties, retimes every ranged spatial curve, and reconstructs the strict
  revisioned wire through checked owners.
- `spatial_node_definition` exposes one variadic image input, one image result, and one animatable
  `GraphValue::Domain` spatial parameter under the production GPU capability. It declares unbounded
  time because authored shutter endpoints can request times beyond the current frame.
- `render_spatial_reference` projects planar layers through perspective or orthographic homographies,
  applies deterministic diffuse light gain and opacity, composites in layer-stack or camera-depth
  order, and averages exact shutter samples in premultiplied space. Fixed layer, light, shutter, and
  total-evaluation ceilings combine with `ImageLimits`; this CPU path is an oracle, not playback.

`text` exposes editable typography and real layout without claiming rasterization:

- `TextRange`, `FontFace`, `OpenTypeFeature`, `VariationAxis`, `TextStyle`, `TextStyleSpan`,
  `ParagraphStyle`, and `ParagraphSpan` retain exact UTF-8 byte coverage, caller asset identity,
  collection face, features, animated axes, RGBA fill, opacity, tracking, baseline shift, measure,
  line height, indents, spacing, alignment, direction, and wrapping.
- `TextLayer` requires complete canonical style and paragraph coverage, one exact animation clock and
  authored interval, valid paragraph boundaries, bounded collections, and checked visual domains.
  Immutable text, style, paragraph, and whole-layer retime operations reconstruct through the same
  boundary. `TEXT_LAYER_SCHEMA_REVISION` identifies the strict bounded wire.
- `FontResolver` supplies exact bytes for a stable `FontFace` without host discovery or network
  access. `TextLayoutEngine` validates the persisted face with pinned Skrifa, itemizes by style,
  script, and Unicode bidi level, shapes with Swash, accepts only cluster-safe Unicode line breaks,
  wraps and visually reorders clusters, then applies paragraph geometry.
- `TextLayout`, `TextLayoutLine`, `TextLayoutRun`, and `PositionedGlyph` own inspectable source
  ranges, paragraph and style identity, direction, sampled appearance, metrics, glyph IDs, positions,
  and advances for a later raster and GPU owner.

`shape` exposes editable vector shape authoring and renderer-ready sampling:

- `PathVertexAnimation` stores anchor and relative cubic handles in one six-component
  `AnimationCurve`. `PathAnimation` retains stable vertex slots, explicit open or closed topology,
  bounded immutable vertex edits, exact sampling, and exact retiming.
- `ShapeColorAnimation`, `GradientStopAnimation`, `GradientGeometry`, `GradientPaint`, `Paint`, and
  `FillStyle` retain straight scene-linear ACEScg color, animated ordered stops, linear or radial
  geometry, pad, repeat, or reflect spread, nonzero or evenodd fill, and animated opacity.
- `StrokeStyle` retains animated paint, opacity, width, miter limit, dash values, and dash offset
  beside explicit cap and join choices. Sampling normalizes odd dash arrays and exposes executable
  phase inspection without rasterizing a path.
- `ShapeRepeater` retains a held bounded integer copy count, fractional offset, animated anchor,
  position, positive scale, rotation, start and end opacity, and above or below composition. Sampling
  emits deterministic virtual copies with explicit affine transforms.
- `VectorShape` and `VectorShapeDocument` combine these operation families on one exact clock,
  provide immutable replacement and whole-document retiming, and publish complete sampled artifacts
  without allocating an image or GPU resource.
- `VECTOR_SHAPE_DOCUMENT_SCHEMA_REVISION` identifies the strict standalone wire. Unknown fields and
  future revisions fail, and deserialization rebuilds every nested operation through checked public
  constructors.
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

No exported feature module remains a scaffold placeholder.

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

The OpenFX flow reuses that graph authority behind a native isolation seam:

1. `OfxPluginHost::scan` rejects in-process or unbounded adapters, loads the permission-free worker,
   reads plugin-wide and per-context descriptions, validates exact OpenFX mandatory state and every
   derived graph name, then unloads and publishes only disabled metadata.
2. A discovered context becomes one exact `ofx.<plugin>.<context>` schema at the plugin semantic
   version. Clips become typed image ports, supported finite parameters become ordinary
   `GraphValue<T>` defaults, and plugin permission requests remain required schema capabilities.
3. Explicit enable checks the complete caller grant before loading. Only Ready contributes an
   active catalog; Disabled, Faulted, and Quarantined keep editor discovery but rely on graph-owned
   missing-node resolution for runtime availability.
4. Create and render validate the authored node against the complete scanned schema. A caller
   projects reached literals at one `OfxTime`, then graph links and expressions resolve and exact
   OpenFX names are reconstructed for the adapter request.
5. Render binds bounded opaque resources against every required clip, grants read-only inputs and a
   write-only Output, and validates the adapter receipt. Graph or binding failures occur before the
   adapter and do not count as plugin failures.
6. An adapter error or panic marks all worker instances failed and records action, category,
   recovery, and diagnostic context. Explicit restart retains the consecutive count; the configured
   threshold quarantines repeated failures until the user acknowledges and restarts the worker.

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

The visual composition flow retains reusable layer state without copying timeline policy:

1. A caller creates generic visual or precomposition layers on one composition clock. Every layer
   retains a half-open active range, complete generic mask and effect payload, exact editable time
   map, and at most one same-composition parent identity.
2. `Composition::new` validates bottom-to-top identity uniqueness, clock agreement, parent
   existence, self-parent rejection, and the local parent DAG. Immutable edits reconstruct the same
   boundary before publishing a replacement.
3. `CompositionArtifact::new` canonicalizes reusable compositions by ID, validates every nested
   reference, requires each precomposition source clock to match its target composition, checks
   mapped source coordinates, and rejects recursive nesting.
4. At one exact root time, each active layer maps to source time using explicit rounding. Visual
   layers become structural outputs. A `PreserveBoundary` precomposition remains one boundary, and
   `RequiresIntermediateSurface` overrides collapse for layer-level masks or effects.
5. A collapsed pass-through precomposition resolves child layers in their authored bottom-to-top
   order. Each output retains the complete nested `ResolvedLayerStep` path, including every generic
   payload, owning-composition time, mapped source time, and root-to-direct local parent chain, so no
   editable state is lost.
6. The strict standalone wire and ordinary generic graph document both preserve authored state.
   Independent ordinary graphs labeled as timeline-role and node-graph-role consumers reload
   canonical bytes and produce equal structural resolution without a role flag or timeline import.

The spatial composition flow executes that structural state without creating another layer owner:

1. Each existing `CompositionLayerId` carries one `SpatialLayer` payload. The payload adds only a
   source image identity, 2D or 3D transform curves, opacity, and sampling; composition remains the
   owner of order, parenting, active range, nesting, collapse, and time remapping.
2. `SpatialCompositionArtifact::new` validates every payload against its composition clock and
   requires one canonical `SpatialScene` per composition. Strict reload routes nested composition,
   layer, curve, camera, light, depth, and shutter state through the same checked boundaries.
3. Sampling first asks `CompositionArtifact::resolve_frame` for structural outputs. For each step it
   samples the root-to-direct same-composition parent chain and the step transform at that step's
   owning composition time, then multiplies every retained precomposition and leaf transform root to
   leaf. Camera, lights, world anchors, normals, opacity, and camera depth remain inspectable.
4. Stack mode preserves authored bottom-to-top order. Camera-depth mode sorts far to near, then uses
   authored stack position and source identity for stable ties. Exact shutter offsets resample the
   entire artifact, including animated camera and light state.
5. The graph-native definition stores the complete artifact as one animatable domain parameter with
   variadic source images and one result. Two independent ordinary graphs serialize, reload, sample,
   and render equal state without workflow branches.
6. The CPU oracle validates fixed work ceilings before pixel loops, projects each source plane with a
   private binary64 view and model homography, reuses the canonical reference transform, grade,
   opacity, and Porter-Duff operations, and incrementally averages shutter frames. The declared GPU
   capability preserves production playback and export ownership outside this path.

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

The motion-tracking flow is:

1. A caller defines one stable track as a point, planar selected region, object region, or calibrated
   camera with known noncoplanar world landmarks. Every feature, exact frame coordinate, reference
   model, manual correction, solved model, observation, confidence, and transformed region remains
   directly inspectable ordinary state on one artifact clock.
2. `TrackingArtifact::solve_request` selects the nearest exact authored or derived sample and stamps
   the current content revision, complete selection, source sample, and target frame. Manual
   corrections resolve above solver output and invalidate derived samples only between adjacent
   authored anchors.
3. A solver receives that immutable request plus explicit checked source and target luma frames.
   The public result constructor rejects a wrong frame, model kind, or feature identity, and atomic
   application rejects stale revision, changed source state, authored target replacement, invalid
   region geometry, and unexplained observations.
4. The CPU reference uses bounded local gradient registration for each selected feature. Point
   tracks publish direct positions; planar tracks normalize coordinates, score a bounded deterministic
   set of homography candidates, and refit the dominant residual consensus; object tracks fit one
   least-squares rotation, translation, and uniform scale; calibrated camera tracks minimize
   landmark reprojection error from the prior world-to-camera pose.
5. The strict wire stores no luma frame or solver cache. It bounds tracks, features, landmarks,
   observations, and samples during decode and reconstructs the complete artifact through checked
   selection, model, ordering, revision, and residual validation.
6. The real consumer declares the complete artifact as one animatable effect parameter, wraps it as
   `GraphValue::Domain`, reloads canonical graph documents in two independent workflow-role graphs,
   and obtains equal inspectable state. This proves reusable editable persistence, not production
   timeline attachment, project autosave, image decode, color conversion, pyramid tracking, GPU
   acceleration, camera calibration, structure from motion, bundle adjustment, or rendered pixels.

The text layout flow is:

1. A caller builds one `TextLayer` whose adjacent style and paragraph spans cover the complete UTF-8
   text exactly once. Every continuous and discrete visual control composes the existing
   `AnimationCurve`, shares one exact clock and interval, and remains directly inspectable.
2. At one exact time, layout samples every reached style and paragraph control and rejects
   interpolation or expression output outside its supported finite domain before publishing output.
3. The caller resolves stable font asset IDs to exact local bytes. Skrifa and Swash reject a missing
   collection face; the module never enumerates host fonts or reaches a network.
4. Unicode bidi levels, style boundaries, and scripts produce logical shaping items. Swash emits
   glyph clusters and metrics, Unicode Linebreak supplies opportunities, and wrapping accepts only
   shaped cluster boundaries. Visual ordering reverses bidi clusters without reversing glyphs inside
   a cluster.
5. Paragraph measure, first, start, and end indents, spacing, line height, start, center, end, and
   justify alignment position owned runs and glyphs. The result remains CPU layout metadata for a
   later rasterizer, not a pixel or GPU resource.
6. The strict text wire and ordinary graph document retain only authored state. Complete text
   payloads link losslessly across independent timeline-role and node-graph-role graphs, reload
   canonically, and lay out identically from the same font bytes.

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

The vector shape authoring flow is:

1. A caller creates stable open or closed cubic path slots from six-component curves and composes
   them with optional fill, stroke, and repeater state on one exact `Timebase`.
2. Paint sampling resolves straight scene-linear solid color or ordered linear and radial gradient
   stops with explicit spread. Stroke sampling retains cap, join, miter, normalized dash pattern,
   and phase, while repeater sampling produces bounded virtual affine copies and opacity.
3. Every path and visual operation provides immutable checked replacement. Whole-document retiming
   maps every nested curve exactly and preserves stable topology, interpolation, and editable state.
4. The strict revisioned document wire reconstructs all nested curves and operations through checked
   constructors. Sampling returns renderer-ready geometry and style state, not an image, GPU resource,
   ROI, cache entry, or production render result.
5. A real integration test stores `GraphValue<VectorShapeDocument>` in ordinary effect-authored
   nodes, mutates and links one complete document in separate timeline-role and node-graph-role
   graphs, then proves equal exact-time samples and canonical graph bytes after reload.

## Dependencies and consumers

- `superi-core` supplies errors, diagnostics context, finite geometry, color and alpha semantics,
  capability sets, semantic versions, `Point2`, `Vector2`, `Matrix3`, `Rect`, `RationalTime`, `Timebase`, exact
  rescaling, and stable primitive serialization.
- `superi-graph` supplies schemas, registries, neutral `GraphValue<T>`, typed editable state,
  mutation, parameter evaluation and projected literal evaluation, typed parameter drivers,
  bounded scalar expressions, immutable runtime compilation, lazy evaluation, diagnostics, cache
  identity, and generic graph persistence. Graph never depends on effects.
- `superi-image` supplies immutable image artifacts, metadata, exact crop and transform operations,
  sample representations, and finite limits. Tracking deliberately accepts explicit checked luma
  instead of inventing an implicit image color conversion or residency route.
- `superi-gpu` is a declared production capability dependency. Current effects source uploads,
  owns, and evaluates no GPU resource.
- Serde owns strict animation, visual-composition, spatial-composition, vector-shape, mask-stack,
  rotoscope, tracking, and text
  records. JSON and
  the reviewed Tinos subset are test-only. `half` performs checked reference conversion to and from
  binary16. Swash 0.2.9 and Skrifa 0.31.1 parse and shape caller-resolved local font bytes; Unicode
  Bidi 0.3.18 and Unicode Linebreak 0.1.5 provide deterministic Unicode layout data without a host
  or network API.
- `superi-timeline` does not depend on effects. It retains editorial nested sequences, clip time
  maps, transition identity, and mutation policy, while the effects composition artifact retains
  visual layering and reusable compositing relationships. Timeline compiles native state into
  graph-owned `GraphValue<TimelineGraphValue>`, including stable transition endpoint, identity, and
  handle
  parameters. A higher integration owner can pair that editorial projection with the effects-owned
  transition definitions without reversing the dependency or copying timeline mutation policy.
- `superi-engine` declares `superi-effects` but has no production catalog, native plugin discovery,
  worker adapter, IPC transport, animation, evaluator, playback, viewport, or export call site. Its
  future plugin supervisor can implement `IsolatedOfxAdapter` without moving native code or worker
  lifecycle into effects. Current real consumers are the role-neutral authoring,
  generic graph reload, reusable controls over shared processing payloads, strict animation,
  visual-composition, spatial-composition, vector-shape, mask, rotoscope, tracking, and text payloads,
  inspectable glyph layout, transition authoring and timing, and bounded headless graph-evaluation
  contracts. Composition, spatial, shape, mask, tracking, text, and transition tests label independent
  ordinary graphs as timeline and
  node-graph roles without claiming production timeline attachment.

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
- OpenFX native code is never loaded or invoked by this crate. Adapters must report worker-process
  isolation, protocol revision 1, a positive message bound and render deadline, and restart support;
  scanning grants no permissions and activation denies every ungranted requested capability.
- OpenFX exact names and plugin versions remain stable at the adapter boundary. Portable graph names
  are deterministic and collision checked, standard context clips and host-managed parameters are
  validated exactly, and unsupported or lossy parameter state fails before schema publication.
- OpenFX parameter state belongs to the graph. Timeline projection runs only for reached literals at
  explicit finite time, graph links and expressions retain dependency authority, and native requests
  contain no hidden editable value. Disabled and failed plugins retain discovered definitions while
  active catalogs fail closed for runtime evaluation.
- OpenFX resource tokens are bounded and opaque. Required clips must bind exactly once, input access
  is read-only, Output access is write-only, and graph, sampling, or binding errors do not increment
  plugin failure state. Adapter errors and panics fault every instance, repeated failures quarantine,
  and a worker restart is required before native execution resumes.
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
- Visual compositions are effects-owned structural artifacts, not editorial nested sequences.
  They import no timeline type, own no timeline mutation policy, and remain ordinary generic domain
  payloads that any workflow role may persist through graph.
- Composition and layer identities are unique and canonical. Every layer has at most one parent in
  the same composition, self-parenting and missing parents are rejected, and both local parenting
  and cross-composition nesting must remain acyclic at construction, edit, and reload boundaries.
- Layer and composition clocks are core-owned exact timebases. Remap layer keys increase strictly,
  source keys use one explicit clock, linear interpolation uses checked integer arithmetic with a
  caller-selected rounding policy, and hold interpolation plus endpoint holds never invent
  floating point authored time.
- Precomposition instances share one referenced composition, so replacing shared content affects
  every instance without copying it. Collapse expands only a `PassThrough` instance; authored
  boundary preservation or `RequiresIntermediateSurface` retains an inspectable nested boundary.
- Structural resolution preserves deterministic bottom-to-top order and every nested ancestor
  payload, owning-composition coordinate, mapped source coordinate, and root-to-direct parent chain.
  The generic composition owner performs no matrix
  composition, camera or light evaluation, mask rasterization, pixel processing, GPU work, or
  editorial sequence compilation.
- Composition edits are immutable and revisioned. The standalone wire denies unknown fields,
  checks its schema revision, bounds compositions, layers, and remap keys before publication, and
  reconstructs every local plus nested relationship through the checked artifact boundary.
- Spatial state never duplicates composition identity, order, parenting, nesting, collapse, or time
  remapping. Every parent and nested transform samples on the exact owning-composition coordinate;
  mapped source time remains separately inspectable.
- Spatial curves share their composition clock and expected component width. Authored values and
  interpolated samples must retain finite nonzero scales, normalized opacity, valid camera field of
  view and clipping, nonnegative light color and intensity, and positive point-light range.
- Spatial matrix evaluation is private binary64 state. A 2D layer ignores X and Y rotation plus Z
  anchor and scale while retaining explicit Z depth; a 3D layer evaluates every component. The
  right-handed camera looks down negative Z, and image Y is inverted only by viewport projection.
- Spatial stack ordering is deterministic. Camera-depth mode is far to near with authored stack and
  source identity tie breakers. Motion sampling includes exact endpoints at an integer interval and
  rejects zero, excessive, reversed, single-sample spans, inexact intervals, and coordinate overflow.
- The spatial CPU oracle requires every resolved source, enforces fixed per-frame and total layer
  evaluation ceilings before pixel work, reuses canonical ACEScg premultiplied image operations, and
  applies `ImageLimits` to every intermediate and final image. It is not an engine fallback.
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
- Tracking selections, tracks, features, landmarks, observations, corrections, and derived samples
  are bounded and canonical. Every artifact uses one exact core timebase, integer frame coordinates,
  exact finite geometry bits, stable identities, and immutable revisioned edits.
- Authored reference state and manual corrections always resolve above solver output. Correction
  changes invalidate only the segment between adjacent authored anchors, requests select the nearest
  coherent exact sample, and result application rechecks revision, source state, track, target,
  model kind, feature identity, region geometry, and observation residual atomically.
- Tracking frame luma is explicit transient input and never enters the artifact wire. Dimensions,
  sample count, finite values, patch radius, iterations, displacement, tracks, features, landmarks,
  observations, samples, and deterministic homography candidates are all hard bounded before their
  work or allocation boundary.
- Point registration rejects border, texture, displacement, and residual failures. Planar fitting
  normalizes coordinates and uses bounded deterministic residual consensus; object fitting rejects
  insufficient spatial spread; camera fitting requires positive calibrated intrinsics, noncoplanar
  known landmarks, positive depth, a prior pose, a nonsingular system, and bounded reprojection
  residual.
- Camera tracking owns only calibrated known-landmark pose refinement. It does not calibrate lenses,
  infer scene structure, run bundle adjustment, model rolling shutter, or create a camera, project,
  image, timeline, GPU, or render owner.
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
- Text style and paragraph spans cover complete UTF-8 state exactly once and stay aligned to scalar
  and logical paragraph boundaries. All continuous and discrete controls share one exact timebase
  and authored interval; discrete changes use hold interpolation, and all sampled values are checked
  again before layout publication.
- Font identity is persistent caller-owned asset identity plus a collection index. Resolution is an
  explicit offline byte seam; system font enumeration, fallback substitution, and network lookup are
  absent. Features and animated axes are unique, bounded, and use exact printable OpenType tags.
- Shaping items preserve logical source ranges across style, script, and bidi boundaries. Line
  breaking never splits a shaped cluster. Visual bidi ordering reverses clusters, never glyphs
  within one cluster, and output remains bounded owned metadata rather than pixels or GPU resources.
- Vector path topology is explicit and bounded. Open paths have at least two vertices, closed paths
  have at least three, every vertex retains one six-component curve, and all nested state shares one
  exact clock.
- Vector fills, strokes, gradients, and repeaters retain complete inspectable animation. Gradient
  geometry and stops, alpha, stroke widths and dashes, repeater scales and copy counts, and every
  sampled result are checked before publication.
- Vector document edits are immutable, exact retiming reaches every nested curve, strict persistence
  rejects unknown or future state, and sampled geometry remains allocation-free authoring output for
  a caller-owned rasterizer.
- Current code performs bounded reference pixel processing, spatial composition proof, and ROI
  calculation, but no production GPU submission, cache integration, mask path rasterization, feather or expansion filtering,
  production timeline sampling or transition attachment, engine playback, project autosave, propagation solver, plugin
  containment, text rasterization, glyph atlas, production tracking attachment or acceleration, or
  rendered text composition. The reference oracle, tracking solver, and text layout engine are not
  production render routes.

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

Seven OpenFX tests prove the permission-free scan lifecycle, exact descriptor and standard-context
validation, unsafe adapter and graph-name collision rejection, inspectable graph-native definitions,
explicit permission grants, discovered and active catalogs, canonical graph persistence, retained
missing-node state, timeline projection before graph expressions, exact clip access, authored-error
separation, host-managed transition animation, structured adapter failures, panic containment,
recovery, and quarantine acknowledgement.

Five control integration tests prove inspectable canonical controls and relationships, exact-time
curve projection, chained scalar parenting, one child control reused by multiple targets, lossless
two-component links, explicit nonscalar expression rejection, equal timeline-role and node-graph
state, equal editor-script-headless samples, canonical graph reload, driver clearing, duplicate and
missing intent rejection, animatable and exact-type enforcement, graph-owned cycle rollback, and
parent-control compilation through real built-in opacity state across two host payload domains.

Seven visual composition tests prove exact forward, speed, reverse, freeze, endpoint-hold, and
explicit-rounding time maps including full signed-coordinate spans; complete bottom-to-top nested
paths; same-composition root-to-direct parents; generic mask and effect payload retention; shared
precomposition reuse; pass-through collapse; isolation-forced and authored boundaries; immutable
payload and order edits; local and nested cycle rejection; strict schema, unknown-field, hostile
parent, revision, and sequence-bound handling; animatable `GraphValue::Domain` storage; canonical
graph reload; and equal resolution in independent timeline-role and node-graph-role consumers.

Eight spatial composition tests prove directly inspectable and retimable 2D and 3D transform state,
strict standalone reconstruction, perspective and orthographic cameras, ambient, directional, and
point lights, far-to-near depth overlap on real pixels, exact three-sample motion pixels, complete
same-composition parent and collapsed precomposition transform paths on each owning clock, one
unbounded-time graph-native definition, canonical reload in independent workflow-role graphs,
identical sampled and rendered results after reload, invalid curve widths and clocks, missing scene
coverage and sources, oversized scene and light wire arrays, inexact or excessive shutter samples,
and image resource ceilings.

The animation consumer proof creates the payload through a real animatable authoring definition,
stores the resulting node in `EditableGraph`, serializes the complete graph document, reloads it
through graph validation, compares canonical bytes, and obtains identical samples. A separate graph
integration test proves projected literal evaluation without copying driver traversal.

Seven rotoscope tests prove exact forward and backward requests, ordered anchors, real hook
execution, provenance, correction precedence and directional invalidation, immutable span and base
editing, propagation clearing, stale and malformed output rejection, bounded construction, strict
wire reconstruction, generic mask payload retention, animatable effect authoring, `GraphValue`
reuse, and canonical graph reload.

Nine tracking tests prove shared core geometry conversion, real point registration, dominant-motion
planar homography recovery with a coherent outlier, object similarity motion and transformed bounds,
calibrated known-landmark camera pose refinement, exact temporal source selection, correction
precedence and segment invalidation, stale and malformed external result rejection, strict bounded
wire reconstruction, all four selection kinds in ordinary state, animatable effect authoring,
`GraphValue::Domain`, and canonical reload in independent timeline-role and node-graph-role graphs.

Seven text tests prove deterministic real OpenType shaping from reviewed local bytes, Unicode LTR
and RTL run ordering, animated wrapping, paragraph alignment and typography, exact whole-layer
retiming, immutable UTF-8, style and paragraph edits, strict bounded wire reconstruction, missing
font and invalid-state failure before publication, lossless reusable control links, equal independent
workflow-role graphs, canonical graph reload, and equal layout after reload. The focused contract,
complete crate suite, warnings-denied all-target Clippy, rustdoc, and a Rust 1.80 check and focused
test pass freshly on the locked dependency graph.
Seven vector shape tests prove stable open and closed cubic topology, exact sampling and retiming,
scene-linear solid and gradient fills, spread and duplicate-stop behavior, complete stroke and dash
semantics, bounded fractional repeaters, direct immutable edits across every visual operation,
strict standalone persistence, and one reusable complete document through canonical timeline-role
and node-graph-role graph reload.

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

The authoring SDK, exact keyframe animation, reusable graph-native control rigs, strict visual
composition artifacts, same-composition parenting, reusable precompositions, explicit collapse
boundaries, exact time remapping, strict spatial composition artifacts, editable 2D and 3D
transforms, cameras, lights, depth ordering, exact motion sampling, editable vector shape documents, animated mask authoring and
composition, editable rotoscope artifacts and propagation hooks, styled text authoring and real
glyph layout, editable point, planar, object, and calibrated camera tracking with bounded CPU
reference solvers, built-in definitions, generic editable instantiation, deterministic CPU reference
pixels including bounded spatial composition, ROI mapping, immutable graph compilation,
introspection, reusable transition definitions,
exact transition timing, bounded transition pixels, and role-neutral graph proofs are substantive
and test-backed. The OpenFX 1.5.1 effect-side host, isolated adapter contract, graph projection,
explicit-time sampling, permissions, lifecycle, structured failure, recovery, and quarantine are
also substantive and test-backed. Strict curve, visual-composition, spatial-composition,
vector-shape, mask-stack, rotoscope, tracking, and text
payloads retain authored state across generic graph reload. The reference and text layout
implementations are scalar, allocation-bounded CPU proofs, not performance production render code,
and vector shapes, masks, and text have no rasterizer or rendered consumer.

There is no GPU shader parity, engine registry, production runtime catalog, timeline attachment,
playback, viewport, export, project persistence, UI, production spatial transform, camera, light, or
motion-blur execution, vector shape rasterization, mask rasterization,
propagation solver, text rasterization or glyph atlas, production tracking attachment, pyramid or GPU
tracking acceleration, production transition
attachment, native OpenFX bundle discovery, worker transport, process supervisor, or production OFX
adapter. Rotoscope mask payloads are generic and have no production mask-type
consumer yet. Authoring metadata is in memory and has no independent wire. Control hints do not yet
encode enforceable numeric bounds, choice option vocabularies, grouping, conditional visibility, or
accessibility policy; transition domains and wipe choices are therefore validated by the reference
compiler. Animation has no stable project-level property identity or production caller-time context.
Visual composition resolution is structural, and the spatial module is its effects-owned reference
consumer. Neither has a production engine compiler, GPU renderer, timeline attachment, or
project-level document owner yet.

Tracking is a bounded scalar CPU reference over caller-supplied luma, not a production optical-flow
engine. Local registration has no pyramid and intentionally rejects large displacement, border,
texture-degenerate, or high-residual patches. Camera tracking assumes caller-calibrated intrinsics,
known noncoplanar landmarks, and a close prior pose; it does not perform calibration, structure from
motion, rolling-shutter estimation, scene reconstruction, or bundle adjustment. The timeline-role
tracking proof uses ordinary graphs because no production effect attachment, project owner, frame
provider, engine scheduler, UI, viewport, cache, or GPU consumer exists.

Reusable control presentation and rig definitions remain in-memory authoring descriptions, while
their applied driver meaning is persisted by graph. Parent expressions are scalar only. Spatial
matrix composition is planar and effects-local, with no material, shadow, volumetric, mesh, or GPU
runtime contract. Runtime factories are exact-version bound but have no plugin discovery, GPU
device, cache, or lifecycle integration.

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

Vector shape documents likewise use canonical shape and vertex indexes rather than future
project-stable property identities. Topology changes are discrete, gradient interpolation is linear
in the stored scene-linear components, and a later runtime still owns tessellation, rasterization,
ROI, GPU resources, caching, and pixels. The timeline-role shape proof uses an ordinary graph because
production effect attachment does not exist.

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

Keep tracking selections, authored references, and corrections canonical, derived samples
replaceable, exact clocks and identities unchanged, external solver results request-bound, and every
wire plus work collection bounded. Preserve explicit luma ownership and core geometry conversions.
Future pyramid, GPU, frame-provider, cache, timeline, UI, project, and engine integration must
consume the same artifact without hiding observations, overwriting corrections, weakening revision
fences, changing deterministic CPU meaning, or treating known-landmark pose refinement as camera
calibration or scene reconstruction.

Keep OpenFX native loading, bundle discovery, IPC, deadlines, GPU-handle transport, and worker
supervision in the engine adapter. Preserve permission-free scanning, explicit activation grants,
exact context and name validation, ordinary graph-owned values, literal-only timeline projection,
fail-closed active catalogs, bounded opaque resources, structured failures, restart, and quarantine.

Keep text fonts caller-resolved and offline, authored spans canonical, curve clocks and intervals
identical, discrete changes hold-interpolated, nested wires reconstructed through checked owners,
and layout results derived and bounded. Preserve the split between logical shaping clusters and
visual bidi ordering, accept line breaks only at cluster boundaries, and leave rasterization, atlas,
GPU residency, timeline attachment, engine registration, viewport, and export ownership outside
this module.

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

Keep visual composition identity, parenting, collapse intent, layer isolation, time remaps, and
generic visual payloads in the checked effects artifact. Preserve bottom-to-top order, exact clocks,
local and nested DAG validation, immutable revisions, bounded strict persistence, and complete
resolved ancestor paths with separate owning-composition and mapped-source coordinates. Do not move
editorial nested-sequence identity or clip retiming out of timeline, add a timeline dependency to
effects, flatten payload state during collapse, or make the generic structural resolver own matrix,
mask, pixel, GPU, or production graph execution.

Keep spatial layer payloads inside the generic composition owner, sample every same-composition
parent and nested step at its owning coordinate, and preserve root-to-leaf matrix order. Retain strict
scene coverage, exact shutter ticks, stable depth ties, finite sampled-domain checks, fixed CPU work
ceilings, canonical premultiplied image semantics, and the unbounded graph time declaration. Future
GPU implementations must consume the same artifact and match the bounded oracle without turning the
oracle into a fallback or adding workflow-specific state.

Keep transition schemas versioned, workflow-neutral, and limited to visual parameter meaning. Do
not move timeline identity, adjacency, handles, record placement, grouping, synchronization,
serialization, or edit reconciliation into effects. Preserve exact-clock handle conversion,
closed progress domains, stable wipe choice codes, common display-window coordinates, exact
endpoints, premultiplied interpolation, same-region dependencies, semantic fingerprints, and tiled
parity. Future GPU implementations must match the bounded oracle and use the same graph parameters.

Keep vector paths in core points and vectors, retain relative handles and explicit open or closed
topology, and reconstruct every immutable style, repeater, retime, and wire operation through checked
owners. Future rasterizers must consume the sampled fill rule, paint, stroke geometry, dash phase,
and virtual copy transforms without moving pixels, GPU allocation, cache policy, or persistence into
the authoring artifact.

When production consumers arrive, record property identities, caller-time flow, GPU resource
ownership, cache behavior, serialization and migration ownership, timeline attachment, project
reload, engine registration, viewport, headless, and export consumers. Update the graph, timeline,
engine, workspace, and global maps whenever those contracts or relationships change. Never report
registered schemas, factory translation, mask composition, or graph reload as production pixel
execution without an exercised implementation and real output.
