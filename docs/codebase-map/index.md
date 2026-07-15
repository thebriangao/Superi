# Superi codebase map

This index is the repository-wide navigation layer for Superi. It summarizes ownership, dependency
direction, implemented runtime paths, incomplete architecture, and verification boundaries across
all discovered modules. Each linked module map is the detailed source of truth for one owned path.

The index describes implemented reality first. Statements labeled "documented target" come from
repository contracts but are not yet implemented end to end. Statements labeled "synthesis" are
cross-module conclusions derived from current manifests and module maps, and should be rechecked
against raw source before changing code.

## Module inventory

| Module ID | Map | Owned path | Current role | Status |
| --- | --- | --- | --- | --- |
| `superi-ai` | [module map](modules/superi-ai.md) | `open/crates/superi-ai` | Reserved local inference and editable-artifact boundary | Skeleton: public module names only |
| `superi-api` | [module map](modules/superi-api.md) | `open/crates/superi-api` | Transport-neutral public facade for capabilities and canonical editorial state | Partial: capability and canonical scenario controls implemented; transport, general API, and scripting absent |
| `superi-audio` | [module map](modules/superi-audio.md) | `open/crates/superi-audio` | Independent prepared audio graph with explicit channel conversion, typed bus routing, transactional clip DSP, core effects, sample-accurate scheduling, bounded device playback, callback-owned discontinuity discard, dual-clock sample-rate conversion, and graph-native metering | Partial: graph processing, channel-layout conversion, bus routing, clip controls, equalization, compression, limiting, delay, saturation, callback scheduling, audio-master publication, device output and discard acknowledgement, band-limited resampling, and peak, RMS, true-peak, phase, spectrum, and loudness metering implemented; engine export invokes an explicit audio stage and proves exact PCM completion, while decoded-sample binding to the prepared graph, automation, variable-rate playback audio, hosting, and complete timeline composition remain absent |
| `superi-cache` | [module map](modules/superi-cache.md) | `open/crates/superi-cache` | Composite reusable-result identity, budgeted final-frame and intermediate-node memory retention, priority-aware strict LRU eviction, precise graph edit invalidation, versioned corruption-recovering disk persistence, replaceable derived-media publication, layered render reuse, bounded background population, bounded playback prediction, bounded edit and scrub warming, and deterministic lifecycle management | Complete identity feeds independent memory and disk tiers with exact admission, revision fencing, bounded envelopes, atomic publication, schema isolation, and corruption quarantine; memory, persistent, and derived owners expose inspection and exact clearing, persistent namespaces relocate through rename or synchronized staged copy, render jobs add cancellation-safe layered reuse, prediction supplies finite signed frame plans and an owned host adapter, and warming is deterministic and hard bounded; engine and scheduler own quality substitution and lifecycle policy remains caller-owned |
| `superi-cli` | [module map](modules/superi-cli.md) | `open/crates/superi-cli` | Headless canonical editorial scenario consumer | Implemented portable expectation verifier and eight instrumented contract stages; rendered media flow absent |
| `superi-codecs-platform` | [module map](modules/superi-codecs-platform.md) | `open/crates/superi-codecs-platform` | Opt-in host codec adapters for Apple, Windows, and Linux | Implemented, host-dependent: native proof depth varies and legal review remains open |
| `superi-codecs-rs` | [module map](modules/superi-codecs-rs.md) | `open/crates/superi-codecs-rs` | Default permissive software codec implementations | Implemented: AV1, FLAC, MP3, Opus, PCM, Vorbis, VP8, and VP9 decode and encode |
| `superi-codecs-vendor` | [module map](modules/superi-codecs-vendor.md) | `open/crates/superi-codecs-vendor` | Explicit process adapter for separately installed vendor RAW workers | Implemented first revision: decode-only, CPU-only, JSON and hexadecimal IPC |
| `superi-color` | [module map](modules/superi-color.md) | `open/crates/superi-color` | Versioned configuration, project working spaces, color math, CPU input and output transforms, GPU wide-gamut transforms, tone mapping, legal-range RGB encoding, LUTs, ICC discovery, and presentation profile guards | Substantial but partial: project-pinned configuration, CPU transforms, managed GPU wide-gamut transforms, and an engine CPU display consumer are implemented; engine export validates a caller-owned delivery stage but does not execute this crate, while ICC evaluation, native GPU display conversion, concrete export conversion, and shell integration remain absent |
| `superi-concurrency` | [module map](modules/superi-concurrency.md) | `open/crates/superi-concurrency` | Execution domains, jobs, clocks, handoffs, shared snapshots, lifecycle, liveness, and derived-media selection | Substantial; audio enforces its domain, engine proxy resolution consumes selection, engine foreground playback and transport consume bounded workers, cancellation, anchor-based clocks, the A/V scheduler, and handoffs, engine lifecycle composes acknowledged phases, EngineControl ownership, immutable publication, and lock-free signals, and render-export enforces lifecycle admission at transaction start and publication; broader liveness and GPU submission composition remain incomplete |
| `superi-core` | [module map](modules/superi-core.md) | `open/crates/superi-core` | Tier-zero values, validation, exact time, identifiers, errors, diagnostics, and stable serialization | Implemented and broadly consumed; crate-level skeleton wording is stale |
| `superi-effects` | [module map](modules/superi-effects.md) | `open/crates/superi-effects` | Graph-native visual definitions, editable defaults and instances, complete reusable effect presets with explicit schema migration and missing-plugin recovery, exact editable keyframe animation, reusable typed control rigs, strict visual composition artifacts with layer parenting, reusable precompositions, collapse boundaries and time remapping, editable 2D and 3D spatial layers with cameras, lights, depth ordering and exact motion sampling, editable cubic vector shapes with fills, strokes, gradients, repeaters, and path animation, animated cubic mask paths with ordered boolean alpha composition, editable rotoscope artifacts and propagation hooks, editable point, planar, object, and calibrated camera tracking with manual corrections and bounded CPU reference solvers, styled text authoring, offline OpenType shaping, Unicode paragraph layout, versioned built-in visual nodes and transitions, exact handle-to-progress timing, isolated OpenFX effect hosting, and bounded CPU reference evaluation | Substantive but partial: typed authoring, presets, deterministic integrity-protected preset documents, revision-zero document migration, explicit transactional schema migration, graph-native missing-plugin placeholder editing and recovery, animation, graph-native links and parent controls, strict visual and spatial composition state, local and nested DAG validation, exact time maps, reusable collapsed or isolated precompositions, complete resolved layer paths, binary64 transform composition, perspective and orthographic cameras, ambient, directional and point lights, stable depth order, exact shutter sampling, vector shape documents, mask controls, rotoscope spans and corrections, revision-fenced propagation, stable tracking selections, observations, transformed regions, camera poses, revision-fenced tracking results, typography, paragraph controls, glyph layout, transform, crop, opacity, blend, composite, blur, sharpen, distortion, keying, invert, grade, cross dissolve, directional wipe, isolated OFX adapter validation, graph projection, permissions, exact-time sampling, lifecycle, recovery, quarantine, workflow parity, strict reload, and real pixel proof are implemented; project persistence, production plugin binding, production spatial GPU execution, engine, UI, vector, mask, and text rasterization, glyph atlases, propagation solvers, pyramid and GPU tracking acceleration, production tracking attachment, native OFX discovery and worker transport, production transition attachment, and complete timeline attachment remain absent |
| `superi-engine` | [module map](modules/superi-engine.md) | `open/crates/superi-engine` | Open subsystem assembly and orchestration | Partial: canonical command state, complete source and codec registry, transactional timeline graph plus source and decoder preparation, capability introspection, CPU-frame GPU upload, color metadata branching, derived-media generation, transparent proxy resolution, predictive cache population, foreground graph and CPU display execution, bounded audio admission, audio-master A/V coordination with correction and discontinuity recovery, lossless viewport handoff, exact interactive transport control, coherent decode, graph, delivery, audio, and elementary-stream export execution, atomic timeline plus clip-mix edits, and deterministic subsystem lifecycle are implemented; decoded source and prepared-audio binding, native GPU presentation and export readback, container muxing and publication, public dispatch, plugin transport, and worker supervision remain absent |
| `superi-gpu` | [module map](modules/superi-gpu.md) | `open/crates/superi-gpu` | wgpu device, resource, upload, conversion, pass, submission, presentation, and recovery substrate | Implemented substrate with explicit application-level integration gaps |
| `superi-graph` | [module map](modules/superi-graph.md) | `open/crates/superi-graph` | Node-neutral identifiers and shared typed values, versioned schema discovery, deterministic DAG storage, typed port validation, editable mutation transactions, canonical graph documents, reusable scalar expressions, typed parameter links and expressions, caller-projected literal evaluation, derived missing-node resolution, dependency and semantic edit invalidation, region-of-interest propagation, request-scoped scheduling and evaluation, node introspection, graph and revision cache lineage, timing, and shared interactive and headless evaluation snapshots | Partial: graph-facing IDs, exact neutral domain and processing values, node schemas, immutable discovery, typed DAG state, atomic mutations, deterministic integrity-checked serialization, checked deserialization, legacy migration, shared bounded scalar programs, typed driver state, parameter-cycle protection, literal-only projected evaluation, fail-closed missing-node placeholders, exact region and edit invalidation, snapshot-bound ROI planning, generic demand-only evaluation, deterministic graph cache inspection, final and intermediate retained-work pruning, run-local timing, and role-neutral editable-to-runtime evaluation implemented; effects consumes broad authoring and reference evaluation, timeline compiles editable graphs, and engine consumes externally prepared snapshots for playback and render-export, while production engine catalog and plugin binding, project persistence, and complete application rendering remain absent |
| `superi-image` | [module map](modules/superi-image.md) | `open/crates/superi-image` | Host image values, still interchange, CPU operations, sequences, previews, and reference validation | Implemented host-side subsystem with explicit representation limits |
| `superi-media-io` | [module map](modules/superi-media-io.md) | `open/crates/superi-media-io` | Codec-neutral source, demux, packet, frame, audio, selection, timing, and operation contracts | Implemented contracts and four demuxers; engine source registration, preparation, and complete elementary-stream export lifecycle orchestration are integrated, while muxing and publication are absent |
| `superi-project` | [module map](modules/superi-project.md) | `open/crates/superi-project` | Reserved project document, persistence, autosave, and recovery boundary | Skeleton: no project model or storage format |
| `superi-timeline` | [module map](modules/superi-timeline.md) | `open/crates/superi-timeline` | Native editorial project state, media bins and saved queries, metadata and relink state, rational range maps and availability, exact clip retiming, typed tracks, authoritative edit intent, markers, exact snapping, clip relationships, atomic foundational, advanced, nested, and multicam operations, OTIO 0.18.1 interchange, versioned integrity-checked state documents, color metadata propagation, and deterministic typed graph compilation | Foundational model, bins, sub-bins, metadata smart collections, explicit relink evidence, exact range and retime resolution, speed changes, reverse, freeze, piecewise time remapping, track clocks, linked sample reshaping, selection, targeting, synchronization, clip relationships, three-class marker ownership, snapping, six primary operations, nine advanced edit families, nested placement, compound creation, shared child editing, recursive inspection, native multicam angle metadata, switching, structural inheritance and resolution, dependency-light OTIO import and export, opaque preservation, stable interchange diagnostics, a headless consumer, strict canonical timeline documents, revision 0 migration, checked recovery, graph color metadata, stable editable timeline-to-graph compilation, and engine preparation retention are test-backed; broader interchange interpretation, fit-to-fill, grouped-source compound synthesis, the owning project container and autosave policy, graph evaluation, multicam mixing, playback, and render integration are absent |
| `tool-superi-dependency-check` | [module map](modules/tool-superi-dependency-check.md) | `open/tools/superi-dependency-check` | Offline executable policy for the open runtime dependency graph | Implemented exact runtime, build, dev, and new-crate checks |
| `tool-superi-boundary-tool` | [module map](modules/tool-superi-boundary-tool.md) | `open/tools/superi-boundary-tool` | Offline scanner for network-client and open-to-closed policy | Implemented library, CLI, workspace gate, and hosted-build command |
| `tool-superi-bench` | [module map](modules/tool-superi-bench.md) | `open/tools/superi-bench` | Stable benchmark harnesses and reproducible stage reporting | Implemented seven-stage runner with real graph evaluation and explicit gaps |
| `tool-superi-fixture-tool` | [module map](modules/tool-superi-fixture-tool.md) | `open/tools/superi-fixture-tool` | Offline fixture validation, generation, and typed golden verification | Implemented validation library, six generators, seven-command CLI, four golden harnesses, and focused contracts |
| `tool-superi-test-report` | [module map](modules/tool-superi-test-report.md) | `open/tools/superi-test-report` | Offline structured platform-lane evidence generator | Implemented strict schema, deterministic findings, collision-safe CLI, and focused contracts |
| `workspace` | [module map](modules/workspace.md) | Repository files outside `open/crates/*` and `open/tools/*` | Product law, architecture, policy, workspace configuration, fixtures, and agent workflows | Active control layer: deterministic checkpoint workflow and contract slice delivered; focused playback and elementary-stream export runtime paths exist, while the canonical application slice remains absent |

## Ownership and repository boundaries

The mapping script assigns each `open/crates/<name>` package to one crate module and each
`open/tools/<name>` package to one tool module. The `workspace` module owns the remaining discovered
repository files, including product and architecture documents, Cargo workspace configuration,
license and codec policy, platform test policy, unsafe-boundary inventory, shared fixture data, CI,
and repository-owned agent skills. Generated maps, local plans, Git internals, build output, and
ignored files do not contribute to module hashes.

`AGENTS.md` is the repository's operational law but is intentionally ignored by Git and therefore
outside the generated workspace inventory and hash. It must be read separately for every task even
when all maps validate. `.worktreeinclude` copies it into Codex-managed worktrees.

Checkpoint work reads this global index plus every directly affected, caller, consumer, contract,
and runtime-path map in full. Another map may be omitted only through the recorded deeper raw-code
substitution defined by root law; directly affected and contract-path maps remain mandatory.
Planning and execution write only `plans/<id>/planning.md` and `plans/<id>/execution.md`, then the
changed-path verifier establishes the deterministic local gate floor. Hosted CI status is not a
general checkpoint completion gate unless the checkpoint explicitly owns hosted CI behavior.
Multi-checkpoint dispatch defaults to three active worktrees but follows any explicit positive
concurrency value from the user.

The open runtime and tool workspace lives under `open/`. Current Cargo membership is 19 runtime
crates plus `superi-fixture-tool`, `superi-dependency-check`, `superi-boundary-tool`, and
`superi-bench`, and `superi-test-report`. All five tools are built with the workspace but remain
outside the runtime dependency graph. The root
`closed/README.md` is only a boundary notice for the separately maintained proprietary tier. Open
Superi must never import, link, or depend on closed code. Closed
Superi may consume the same open public API as any other client and must produce ordinary editable
artifacts through that public seam.

Open operation is required to remain offline, account-free, and independent of hosted fallback.
The platform codec module may inspect installed operating-system frameworks, local drivers, render
nodes, and build headers. The vendor codec module may start only executables explicitly selected by
the caller. Neither boundary discovers or downloads remote codec implementations.

## Dependency direction

Dependency arrows below point from a consumer to a dependency. This is a synthesis of the current
Cargo graph and module maps. Several manifest edges are scaffold declarations only and have no
current Rust call site.

```text
superi-cli
  -> superi-api
    -> superi-engine

superi-engine
  -> superi-codecs-rs
  -> superi-codecs-platform        optional through os-codecs
  -> superi-codecs-vendor          optional through vendor-codecs
  -> superi-media-io
  -> superi-gpu
  -> superi-core
  -> superi-cache                  derived publication, substitution, color, playback retention, prediction
  -> superi-concurrency            selection, playback jobs, cancellation, clocks, handoffs, lifecycle
  -> superi-timeline               graph compilation, reachable media preparation, signed playback rate
  -> superi-graph                  foreground evaluation and retained timeline compilation
  -> image, color, audio           active playback and display contracts
  -> effects, ai, and project      partial or manifest-only integration

superi-project -> superi-timeline -> superi-graph -> superi-image
superi-color, superi-effects, superi-cache, superi-ai -> lower graph/image/GPU/core layers
superi-cache -> superi-concurrency        bounded background render jobs
superi-audio -> superi-concurrency -> superi-core
superi-graph -> superi-gpu, superi-image, superi-concurrency, superi-core

superi-codecs-rs, superi-codecs-platform, superi-codecs-vendor
  -> superi-media-io
  -> superi-core

superi-media-io -> superi-image -> superi-core
superi-gpu -> superi-core

superi-bench -> superi-graph -> superi-core
```

`superi-core` is the tier-zero semantic contract and has no Superi dependency. Higher modules must
not copy its identifier, time, geometry, color-tag, pixel, audio-layout, error, diagnostic, or
stable serialization meanings into competing local types without an explicit boundary conversion.

The generic graph direction is deliberately one way. Graph may depend on representation and
execution substrates, while color, effects, cache, timeline, AI, project, and engine may depend on
graph. Graph must not depend upward on a domain catalog. Its implemented surface uses core-owned
object identifiers and core semantic contracts for schema definitions, and it owns generic
deterministic DAG storage, opaque typed bindings, deterministic request-scoped scheduling and
evaluation, typed parameter drivers and bounded expressions, derived exact-schema availability, and
structured validation errors. Graph also owns a catalog-neutral value payload that preserves exact
domain state beside finite processing values without naming an effect. Effects is the first
higher-tier authoring, animation, concrete schema, expression, diagnostics, compiler, generic
persistence, and reference-evaluation consumer above that boundary. Its strict keyframe payload and
built-in visual definitions remain ordinary editable graph state. Effects presets retain one
complete saved schema and every typed literal, instantiate fresh ordinary graph nodes, and use
graph-derived missing availability plus canonical documents without adding preset or plugin state to
graph. Explicit effects-owned schema migrations transform complete preset parameter sets through
checked forward steps rather than changing graph's schema resolver. Effects also reuses the bounded scalar
program for time and parent expressions, compiles reusable controls into ordinary typed drivers,
and projects literal curves into exact-time samples through the graph-owned evaluator without adding
effect types to graph. The visual composition, spatial composition, vector-shape, mask, rotoscope,
tracking, and text contracts likewise persist strict visual layers, animated transforms, cameras,
lights, depth and shutter state, editable shape documents, animated mask stacks, exact-frame
artifacts, tracked selections and observations, and styled text domain state through the neutral
value payload and generic graph documents without adding composition, spatial, shape, path,
propagation, tracking, solver, camera-pose, font, shaping, or paragraph meaning to graph.
Effects-owned compositions retain visual parenting, precomposition collapse, and layer-to-source
time maps, while timeline remains authoritative for editorial nested sequences, clip retiming, and
edit policy.
Effects transition definitions also remain ordinary graph schemas with typed parameters and a
bounded reference evaluator. Timeline compilation consumes the same value payload, schemas,
editable storage, atomic mutations, and
immutable snapshots without importing effects; it remains authoritative for transition identity,
adjacency, handles, record placement, grouping, synchronization, persistence, and mutation. A later
integration owner may pair its neutral transition projection with the effects schemas without
reversing this dependency. Timeline and cache also consume the graph-owned color metadata wrapper,
but no timeline path consumes graph evaluation, documents, animation curves, the effects catalog,
or a production runtime factory.

Codec implementations depend down on the codec-neutral `superi-media-io` interface. Media I/O does
not depend on a concrete codec, engine, or registry assembler. The engine owns the current assembly
choice. The API depends on engine-owned projections rather than leaking media-I/O implementation
types. The CLI depends only on the API for editorial control and never imports engine scenario
state directly.

## Public control flow

### Implemented today

Media registry construction and capability introspection are implemented as follows:

1. `superi-engine::media` creates a `BackendRegistry` and registers the default Rust codecs.
2. The `os-codecs` feature may append host-discovered platform codecs.
3. Engine construction creates and preflights primary priority-100 registrations for the in-tree
   Matroska or WebM, MP4 or MOV, MXF, and PCM container sources before inserting any of them.
4. The separate vendor constructor may append only explicitly configured vendor workers.
5. `superi-engine::introspection::MediaCapabilities::from_registry` reads declarations without
   opening sources or constructing codecs, then produces deterministic engine-owned records.
6. `superi-api::MediaCapabilitiesApi` projects those records into strict serializable API types.
7. `GetMediaCapabilities` clones the current full snapshot. `synchronize` emits one full-replacement
   `MediaCapabilitiesChanged` event only when semantic capability state changes.

Timeline graph and media preparation are implemented at an engine-private shared-resource boundary:

1. The caller supplies one editorial project, root timeline, registry, operation context, fallback
   policy, and exactly one explicit source plus decoder-stream request for each reachable media ID.
2. `superi-timeline::compile_timeline` produces the single editable graph compilation for the root
   and every reachable nested timeline. Engine traversal resolves the same closure to media IDs and
   rejects missing, duplicate, extra, empty, or duplicate-stream requests.
3. The engine binds persistent project fingerprints, performs one bounded content probe, opens only
   the selected source, verifies its returned identity, resolves complete stream descriptors, and
   constructs only the selected decoder backend.
4. Source and decoder selection evidence retains stable IDs, content confidence, probe bounds,
   fallback candidates, and fallback-use state. A selected backend failure is returned directly and
   never retried through those candidates.
5. The graph compilation, all opened sources, and all live decoders publish together as one
   `TimelineResources` value only after a final operation check. Playback, render, export, A/V sync,
   and arbitration consumers remain later owners.

Proxy and optimized-media generation is implemented at an engine-private bulk-media boundary:

1. The caller binds authoritative media identity and revision to explicit purpose, quality, and an
   engine-derived fingerprint of the complete encoder configuration.
2. `superi-cache::DerivedMediaCatalog` returns an exact immutable artifact when one already exists.
3. On a miss, `superi-engine::generate_derived_media` rederives and verifies the settings, selects
   one primary registered encoder with fallback disabled, and drives caller-prepared decoded inputs
   through the codec-neutral lifecycle.
4. The engine retains every packet privately through flush and end of stream, hashes complete bytes,
   timing, keyframe state, and typed metadata, then asks the cache catalog to publish one immutable
   artifact.
5. Failure or cancellation publishes nothing. A prior complete artifact remains unchanged, while an
   exact miss exposes the authoritative original source instead of choosing stale or different-quality
   media.

The real default AV1 backend consumes this path in engine integration proof. The path does not
render or resize input, mux or persist output, choose playback proxies, mutate projects, or enter the
transport-neutral API.

Transparent proxy substitution is implemented at the same private bulk-media boundary:

1. The caller supplies the complete authoritative `SourceIdentity`, exact source revision,
   requested scheduler quality, fallback policy, immutable derived artifacts, and a lazy original
   opener.
2. `superi-engine::resolve_proxy_source` admits only proxy-purpose artifacts with exact source ID,
   source fingerprint, revision, known quality, complete packet stream and timebase, timing, and
   keyframe access.
3. The engine translates cache qualities one for one and delegates exact, lower-quality, stable
   cache-ID tie, unavailable, and source-only decisions to
   `superi-concurrency::DerivedMediaRequest::select`.
4. A selected artifact becomes a packet-backed `MediaSource` that retains the immutable artifact,
   preserves generated packet bytes and timing, supports deterministic keyframe-bounded seek, and
   exposes the authoritative original identity.
5. Source selection opens the original only then and rejects any identity mismatch. Source-only
   delivery therefore ignores even a valid proxy, while stale, mismatched, malformed, optimized,
   missing, or higher-only candidates fall back without changing authored meaning.

The real default AV1 generation path feeds this substitution contract in engine integration proof.
No production playback clock, export queue, project owner, or public API invokes the resolver yet.

Bounded edit and scrub warming is implemented at a cache-owned planning boundary:

1. A caller supplies one checked half-open timeline frame interval and one hard-limit policy.
2. Edit planning canonicalizes exact editorial boundaries, then ranks nearest valid frames across
   all boundaries independently of input order and duplicates.
3. Scrub planning compares two exact observations, caps the observed stride, favors predicted
   frames in the forward or backward direction, retains a smaller opposite tail, and alternates
   nearest neighbors when the scrub is stationary.
4. Every plan is clipped and hard bounded before work begins. Targets contain only exact timeline
   frame, reason, and rank, so the caller must map them to its ordinary graph request and complete
   cache scope.
5. The real graph and memory-cache integration proof shows warmed demand hits without node
   execution, changed media fingerprints recompute, and ordinary budget pressure preserves exact
   fresh results.

The planner owns no cache key, retained value, project mutation, source opening, proxy quality,
fallback, job dispatch, cancellation, or hidden history. No production editor, engine, playback,
API, or UI path consumes it yet.

The API-local revision begins at zero and increments only on a changed snapshot. The public schema
version is `2.0.0`; the permanent method and event names are
`superi.media.capabilities.get` and `superi.media.capabilities.changed`.

Canonical editorial control is also implemented at a bounded reference boundary:

1. `superi-engine::command::ScenarioEngine` validates fixed fixture identity and owns exact import,
   insert, trim, mirror, operation-log, undo, and redo state.
2. `superi-api::ScenarioApi` accepts one strict typed action command and projects complete public
   timeline, graph, implementation, operation, and failure state.
3. `superi-cli` resolves and digest-validates the repository source and derived expectation
   fixtures, executes the normalized fixed scenario through the API, proves undo plus redo recovery,
   and emits eight stage records with bounded monotonic timing and current-process resident-memory
   boundary samples.
4. The CLI verifies 48 frame identities, exact synchronized PCM evidence, timestamps, state digests,
   and target metadata. It distinguishes applicable expectation success from rendered pixels that
   are not evaluated and rendered audio that is not applicable to the video-only slice.
5. Six missing production owners are reported as stubs, and the CLI publishes a non-playable
   contract artifact instead of claiming `canonical.webm`.
6. Project expectation version 2 normalizes the canonical absolute source path to its stable
   repository-relative identity before hashing, and both hosted Rust build jobs validate every
   fixture, compile and test the supported `os-codecs` configuration, and execute this same
   normalized consumer path with accurate active-feature identity.

The independent audio processing graph is implemented below engine orchestration:

1. `superi-audio::graph::AudioGraph` owns audio-specific graph, node, and edge identities, source,
   processor, submix, auxiliary, and master roles, one exact sample rate, one positive process-block
   bound, and ordered editable DAG storage.
2. Edge insertion rejects missing endpoints, cycles, ambiguous ordinary inputs, duplicate masters,
   illegal direct, send, return, or master routes, and unequal ordered `ChannelLayout` values before
   mutation. No implicit channel conversion or resampling occurs.
3. Preparation selects one destination or the single master and its ancestors, computes stable
   processing order, resolves every input in edge identity order to an earlier node, and fallibly
   preallocates every interleaved f32 buffer. Borrowed input views expose current-block samples
   without callback allocation or copying.
4. `PreparedAudioGraph::process` requires `ExecutionDomain::Audio`, rejects rate, size, output,
   overflow, and continuity mismatches before running processors, then advances the next exact
   sample only after complete success.
5. `PreparedChannelMixer` converts explicit canonical layouts with a precomputed speaker or
   discrete matrix. Direct graph edges remain exact-layout only, and consecutive block timing is
   unchanged through the explicit converter node.
6. `ClipMixState` publishes complete controls and identity changes transactionally. Preparation
   resolves snapshot-wide solo and precomputes semantic routing and phase coefficients before the
   callback applies gain, exact linear fades, equal-power stereo pan, mute, solo, and phase.
7. Public crate integration tests use unity `SummingBus` processors to prove dry submix, parallel
   auxiliary send and return, stable identity-ordered summing, and one terminal master over
   consecutive 48 kHz stereo blocks.
8. `superi-engine::audio_mix` consumes real timeline edit outcomes against cloned project and mix
   state. It inherits right-fragment intent, transfers replacements, removes deleted identities,
   and publishes both revisions only after both validate.
9. Public audio and engine contracts consume exact consecutive blocks and a real razor edit. No
   decoder, resampler, meter, plugin host, or engine playback owner feeds the complete routing path.

Generic graph storage is implemented independently of that reference path:

1. `superi-graph::ids` exposes the official core-owned graph identity types.
   `superi-graph::value::GraphValue<T>` preserves exact domain payloads beside finite scalar,
   vector, color, matrix, Boolean, and bounded choice values. Expressions accept only the explicit
   scalar variant without coercion.
2. `superi-graph::dag::DirectedAcyclicGraph` stores caller-owned node payloads and typed port edges
   in ordered primary and adjacency collections.
3. Checked edge insertion rejects missing nodes, duplicates, self-loops, and transitive cycles
   before mutation. Stable Kahn ordering uses the smallest ready node identity.
4. Exact dirty-region sets preserve half-open coverage without bounding-box over-invalidation. A
   pure planner validates changed nodes, walks stable topological and edge order, maps dependency
   regions through a caller-owned edge seam, merges converging work, and excludes clean nodes. A
   semantic edit planner compares immutable revisions, expands parameter-driver dependencies, and
   propagates roots through both prior and current topology in stable identity order.
5. ROI requests are validated against one immutable editable snapshot, clipped to per-output
   regions of definition, and propagated only through connected ancestors in reverse stable
   topological order. Built-in full-frame, pass-through, and checked expansion behavior shares one
   path with validated custom per-input mappings.
6. ROI plans stamp graph identity and revision, return required nodes in dependency-first order,
   preserve exact irregular region sets, and intersect with invalidation plans without scheduling
   clean work.
7. Public integration tests consume the store, invalidation plan, ROI plan, and shared evaluation
   snapshot directly. Timeline consumes neutral values and editable storage, while effects consumes
   schemas, mutation, parameter evaluation, diagnostics, and headless compilation for its bounded
   CPU reference. No production engine, API, CLI, project, GPU catalog, or rendered path consumes
   them, so no runtime stage label changes.

Typed port validation is implemented beside storage and reused by editable graph transactions:

1. Registered `NodeSchema` definitions declare canonical names, exact `ValueTypeId` tags, and
   `Single`, `Optional`, or `Variadic` cardinality independently for inputs and outputs.
2. One pure validator canonicalizes typed binding groups, preserves variadic graph order, retains
   opaque evaluator-owned payloads, and returns structured input or output diagnostics.
3. A schema-level connection check accepts only an existing output to an existing input with exact
   type identity. Input and connection failures are user-correctable, while node implementation
   output violations are internal terminal failures.
4. The validation API remains evaluator-neutral and never inspects or coerces an opaque payload.

Editable graph transactions now integrate those neutral contracts without integrating a production
evaluator:

1. `EditableNode<T>` binds stable port and parameter instance IDs to every declaration in one exact
   immutable schema. Initial and replacement parameter payloads retain exact `ValueTypeId` tags.
2. `GraphTransaction<T>` carries one expected revision plus ordered add, remove, connect,
   disconnect, presentation reorder, parameter replacement, driver set, and driver clear mutations.
3. `EditableGraph<T>` applies the batch to a cloned candidate. Connect resolves `PortId` to
   `PortName`, reuses schema compatibility, enforces target cardinality, and then uses checked DAG
   insertion. Remove requires explicit prior disconnection.
4. Driver mutation resolves existing typed targets and explicit dependencies against the private
   candidate, stores direct links or bounded pure expressions in canonical target order, and rejects
   direct or transitive parameter cycles before publication. Referenced nodes require explicit
   driver cleanup before removal.
5. Any failure discards the candidate and preserves the revision. A successful nonempty batch
   publishes one immutable `Arc` snapshot and advances once; editor, script, and headless readers
   can share the exact same typed state and deterministic orders. Snapshot parameter evaluation
   resolves literals, lossless links, and finite expressions once per request in stable dependency
   order, with domain-owned numeric conversion for opaque payloads. Higher domains may project only
   reached undriven literals into another result domain while graph retains exact types, driver
   traversal, cycle invariants, and completion ordering.
6. Public graph integration tests, the native timeline compiler, and the effects authoring and
   control contracts are real consumers. Effects instantiates ordinary typed nodes, compiles
   reusable controls into normal parameter-driver mutations, and samples linked or parented curves
   in two role-labeled graphs. Engine, API, CLI, project, and product runtime paths do not yet import
   this effect transaction path, so no runtime stage label changes.

Editable property animation now composes with that neutral state boundary:

1. `superi-effects` authors one checked `AnimationCurve` over core-owned exact time. Fixed and
   roving keys retain inspectable timing intent beside outgoing linear, cubic, or hold interpolation,
   cubic Bezier easing, independent value tangents, and optional bounded expression source.
2. Curve construction derives distinct integer-tick roving times between fixed anchors. Exact
   evaluation clamps endpoints, preserves exact key values, interpolates the active segment, and
   then applies a shared scalar expression over only `time` and base `value`.
3. Immutable insert, replace, remove, expression, and uniform retime operations reconstruct checked
   authored state. Retime requires exact fixed-key mappings, recomputes roving positions, and
   inversely rescales per-second tangents.
4. The strict revisioned curve wire serializes authored state, not derived roving times, and routes
   reload through public validation. A real integration test declares the curve as an animatable
   effect parameter, instantiates the effect-authored `EditableNode`, serializes and reloads its
   graph, and proves canonical bytes plus identical samples.
5. `ParameterControlRig` names ordinary animatable parameters as reusable controls and compiles
   lossless links or bounded `parent` plus `local` expressions into one revision-bound graph
   transaction. Exact-time evaluation samples reached literal curves before the canonical graph
   driver traversal, preserving vector links while rejecting nonscalar expression inputs.
6. Cross-workflow tests apply one rig to timeline-role and node-graph-role graphs, obtain equal
   editor-script-headless samples, reload complete driver state through the graph document, and
   prove graph-owned cycle rollback. The same rig also compiles through a real built-in opacity
   state across two shared host payloads. These are substantive editable artifacts and graph
   persistence consumers, not a production timeline attachment, production spatial path, UI, engine playback
   path, project document, GPU execution, or rendered output.

Editable visual compositions now preserve layering and nested relationships on that neutral state
boundary without copying editorial timeline ownership:

1. `superi-effects` stores bottom-to-top generic visual layers with exact half-open active ranges,
   complete mask and effect payloads, optional same-composition parents, and exact layer-to-source
   time maps supporting forward, speed, reverse, freeze, endpoint hold, and explicit rounding.
2. `Composition` validates one local parent DAG and exposes immutable layer naming, range, source,
   payload, parent, time-map, collapse, isolation, insertion, replacement, removal, and reorder
   controls. `CompositionArtifact` canonicalizes reusable compositions and validates its root,
   nested references, source clocks and ranges, and the complete precomposition DAG.
3. `resolve_frame` emits deterministic bottom-to-top structural outputs. Pass-through collapse
   expands reusable child layers, while authored boundary preservation or a mask or effect
   `RequiresIntermediateSurface` rule retains one explicit nested boundary. Every output preserves
   its complete ancestor payload path, owning-composition times, mapped source times, and
   root-to-direct parent chains.
4. Immutable artifact edits advance a checked content revision. Its standalone revisioned wire
   rejects unknown fields, unsupported revisions, unbounded composition, layer, and remap sequences,
   invalid parents, and recursive nesting before publication.
5. A real animatable effect parameter and `GraphValue::Domain` consumer persist the same artifact in
   independent ordinary graphs labeled as timeline-role and node-graph-role workflows, reload
   canonical bytes, and produce equal structural frames. This is reusable visual composition state,
   not editorial nested-sequence mutation, a production graph compiler, GPU execution, or rendered
   pixels.

Editable spatial compositions now consume that structural owner without duplicating it:

1. Each visual composition layer carries one strict `SpatialLayer` payload with stable source image
   identity, 2D or 3D interpretation, animated anchor, position, XYZ Euler rotation, scale, opacity,
   and reconstruction choice. Composition remains authoritative for IDs, order, parenting, ranges,
   precompositions, collapse, and exact time remapping.
2. One `SpatialScene` per reusable composition retains an animated right-handed perspective or
   orthographic camera, bounded ambient, directional, and point lights, layer-stack or camera-depth
   ordering, and exact shutter endpoints plus sample count. Construction and reload reject missing
   scene coverage, mixed clocks, wrong component widths, invalid domains, and excessive collections.
3. Exact sampling starts from `resolve_frame`, samples every same-composition parent and nested step
   at its owning composition coordinate, multiplies private binary64 matrices root to leaf, and
   publishes inspectable world transforms, anchors, normals, camera depth, camera, lights, opacity,
   and shutter offsets. Camera-depth order is far to near with stable authored ties.
4. One graph-native definition exposes variadic image sources, one image result, and the complete
   artifact as one animatable domain parameter. Its unbounded time declaration is honest about
   authored shutter endpoints. Independent timeline-role and node-graph-role graphs reload canonical
   bytes and produce identical sampled and rendered results without graph learning spatial meaning.
5. A bounded CPU oracle projects source planes through perspective or orthographic homographies,
   applies deterministic diffuse lighting and opacity, composites premultiplied ACEScg pixels, and
   incrementally averages exact shutter samples. Fixed layer and total-evaluation ceilings plus
   `ImageLimits` bound work before pixels; production GPU playback, viewport, and export remain absent.

Editable animated masks now compose with the same effect and graph foundations:

1. `superi-effects` authors each stable closed contour from bounded six-component vertex curves over
   core-owned points, vectors, and exact time. Nonzero and evenodd fill rules, relative cubic
   handles, explicit closure, and immutable vertex topology remain directly inspectable.
2. Each mask carries animated feather radius, signed expansion, normalized opacity, and a
   hold-interpolated inversion toggle beside replace, union, subtract, intersect, or exclude stack
   behavior. Construction checks clocks and authored ranges, and sampling rejects expression or
   interpolation overshoot before publishing state.
3. Exact-time sampling exposes cubic segments and every control to a future rasterizer. The caller
   applies fill, expansion, and feather, then returns one normalized coverage per mask; the sampled
   stack applies inversion, opacity, and deterministic Porter-Duff soft-alpha equations in canonical
   order. No image, GPU resource, ROI, or pixel result is manufactured by this authoring layer.
4. Immutable fill-rule, vertex, mask-control, operation, and stack edits reconstruct bounded state.
   The strict revisioned mask-stack wire denies unknown and future fields and routes every nested
   curve, path, mask, and stack through checked constructors.
5. A real integration test declares `GraphValue<MaskStack>` as an animatable effect parameter,
   mutates it in independent timeline-role and node-graph-role `EditableGraph` values, links the
   complete stack through a reusable control rig, serializes and reloads both, and proves equal
   samples plus canonical bytes. This is workflow-neutral editable mask state, not a built-in node,
   rasterizer, production timeline attachment, or rendered effect.

Editable vector shapes compose through the same neutral state boundary:

1. `superi-effects` authors stable open or closed cubic paths from bounded six-component vertex
   curves over core-owned points, vectors, and exact time. Immutable vertex edits and exact retiming
   preserve topology and interpolation across every nested operation.
2. Optional fills retain nonzero or evenodd winding, scene-linear solid color or ordered linear and
   radial gradients, explicit spread, and opacity. Strokes retain paint, opacity, width, cap, join,
   miter, animated dash pattern, and dash offset.
3. Bounded repeaters retain held integer copy count, fractional offset, affine transform components,
   opacity endpoints, and above or below composition. Exact-time sampling publishes deterministic
   virtual copies and renderer-ready geometry without allocating pixels or GPU resources.
4. A strict revisioned `VectorShapeDocument` wire denies unknown or future state and reconstructs all
   nested curves and operations through checked constructors. Whole-document edits and retiming
   remain directly inspectable after reload.
5. A real integration test stores the complete document in `GraphValue::Domain`, mutates and links it
   through ordinary effect-authored nodes in separate timeline-role and node-graph-role graphs, and
   proves equal samples plus canonical bytes after reload. This is editable reusable state, not a
   rasterizer, production timeline attachment, or rendered effect.

Editable rotoscope propagation uses the same state boundary without giving a solver ownership:

1. `superi-effects` stores bounded non-overlapping spans on one exact core timebase. Each span keeps a
   stable ID, complete generic base mask, strictly ordered per-frame corrections, and separately
   inspectable derived propagation.
2. Forward and backward requests expose the base plus same-direction correction anchors and every
   non-authored target in exact traversal order. A tracking or local inference engine implements the
   public propagation hook and returns complete typed samples without hidden graph or mask state.
3. Result construction rejects partial, extra, reordered, or wrong-clock output. Atomic application
   also checks the source revision, span, direction, range, anchors, and targets, then replaces only
   derived samples on that side while corrections remain authoritative.
4. Immutable operations add, replace, and remove spans, bases, corrections, and derived results.
   Directional correction edits invalidate only affected propagation, preserving opposite-side work
   and the generic payload's mask layers and composition relationships.
5. A strict versioned bounded wire and ordinary effect parameter plus `GraphValue::Domain` consumer
   survive canonical graph reload. Mask geometry, rendering, a propagation solver, production
   timeline attachment, UI, and engine execution remain unimplemented.

Editable motion tracking uses the same ordinary state boundary:

1. `superi-effects` retains stable point, planar region, object region, and calibrated camera
   selections on one exact core timebase. Feature IDs, selected geometry, known noncoplanar world
   landmarks, camera intrinsics, reference models, observations, transformed regions, confidence,
   and camera poses remain inspectable typed state.
2. Each track keeps its authored reference and manual corrections separate from replaceable derived
   samples. Immutable edits advance one content revision, corrections remain authoritative, only the
   affected derived segment is invalidated, and each request starts from the nearest coherent sample.
3. The bounded CPU reference solver consumes explicit finite dense luma frames. It performs iterative
   local point registration, normalized homography fitting with deterministic residual consensus,
   two-dimensional object similarity fitting, or calibrated known-landmark camera-pose refinement.
4. Solver results retain the complete request provenance and are rejected when stale, partial,
   reordered, wrong-clock, nonfinite, or inconsistent with the requested selection. Atomic
   application never publishes a partially checked result.
5. A strict versioned bounded wire plus ordinary animatable effect parameter and
   `GraphValue::Domain` consumer survive canonical graph reload in independent timeline-role and
   node-graph-role graphs with equal complete artifacts.
6. This is editable reusable motion state with deterministic reference solving, not production frame
   decode, pyramid tracking, GPU acceleration, camera calibration, structure from motion, project
   persistence, timeline attachment, UI, cache, engine execution, or rendered pixels.

Editable transitions now reuse the same one-way timeline, effects, and graph boundaries:

1. `superi-timeline` owns transition IDs, adjacent timed endpoints, source handles, record intent,
   grouping and synchronization consequences, atomic invalidation, persistence, and neutral graph
   compilation. It does not import effects.
2. `superi-effects::transition` supplies stable `1.0.0` cross-dissolve and directional-wipe schemas
   with required `from` and `to` image ports, one `result`, animatable progress, and wipe direction
   plus softness. Catalog registration and instance binding reuse the ordinary authoring SDK.
3. `TransitionTiming` validates one exact edit clock and maps cut-centered handles to a half-open
   range plus clamped host progress without taking identity or edit ownership from timeline.
4. The effects reference compiler requires exact schemas, resolves graph-owned drivers, fingerprints
   semantic ports and every resolved transition parameter, requests the same region from both
   inputs, and evaluates premultiplied cross dissolves or four-direction display-window-based wipes.
5. Focused tests execute these nodes through `GraphEvaluationSnapshot`, prove exact endpoints, soft
   edges, tiled parity, introspection, cache-key changes, immutable old revisions, and equal output
   in independent timeline-role and node-graph-role graphs. Production timeline binding, GPU parity,
   engine registration, project persistence, viewport, playback, and export remain absent.

Editable text layout uses the same state boundary without claiming a rasterizer:

1. `superi-effects` stores bounded UTF-8 content under complete style and logical paragraph spans.
   Persistent font asset references, OpenType features and animated axes, fill, opacity, tracking,
   baseline shift, measure, line height, indents, spacing, alignment, direction, and wrap controls
   remain inspectable and share one exact animation clock and interval.
2. Immutable text, style, paragraph, and whole-layer retime operations reconstruct checked state.
   Continuous and hold-discrete controls are checked at authoring and again after exact-time sampling,
   and the strict revisioned wire rebuilds every nested owner.
3. A caller resolves stable font IDs to exact local bytes. Pinned Skrifa and Swash validate and shape
   the selected collection face; no host font enumeration, fallback substitution, account, or
   network service enters the contract.
4. Style, script, and Unicode bidi itemization produces logical glyph clusters. Unicode Linebreak
   opportunities are accepted only at cluster boundaries, then bounded wrapping, visual bidi
   cluster ordering, paragraph geometry, alignment, and justification produce owned inspectable
   lines, runs, metrics, glyph IDs, source ranges, positions, and advances.
5. One full `GraphValue<TextLayer>` payload links losslessly through ordinary reusable controls in
   independent timeline-role and node-graph-role graphs, survives canonical graph reload, and lays
   out identically from the same font bytes. Text rasterization, glyph atlases, GPU resources,
   production timeline attachment, engine registration, UI, viewport, and export remain absent.

Versioned graph documents preserve that same editable state without claiming project persistence:

1. `serialize_graph` emits one deterministic `superi.graph` JSON envelope containing a canonical
   payload, explicit format and primitive revisions, and SHA-256 integrity.
2. The payload preserves complete schemas, typed node and parameter state, presentation order,
   edges, graph identity, and optimistic revision. Equivalent semantic state normalizes to the same
   bytes independently of insertion and JSON object-key order.
3. `deserialize_graph` rejects corrupt, truncated, unknown, or future documents, migrates the one
   supported legacy revision, and reconstructs state through existing schema, node, transaction,
   connection, cardinality, and cycle checks.
4. Independent editor, script, and headless-style tests load the document, observe equal snapshots,
   and evaluate equal results through the shared evaluator. No project file, SQLite, autosave,
   recovery journal, engine, API, CLI, or product runtime path consumes the codec yet.

Missing plugin schemas now degrade that same editable state without creating a second model:

1. `superi_graph::missing::resolve_graph` compares each node's complete embedded schema with one
   immutable `NodeRegistrySnapshot` and retains the original `GraphSnapshot<T>` unchanged.
2. Exact identity and schema equality is available. An absent identity produces an unregistered
   placeholder, while different fields under the same identity produce an incompatible placeholder
   and fail closed rather than reinterpreting typed state.
3. Resolved nodes expose original schemas, ports, parameters, drivers, edges, and order beside the
   derived availability. Missing state never enters transactions, document bytes, or migrations, so
   users may inspect, edit, and resave the graph while a plugin is absent.
4. The shared evaluation gate returns one canonical `Unavailable` and `Degraded` result containing
   graph and registry revisions plus every blocker in stable node order. Registering exact saved
   schemas later restores evaluation without changing graph revision or authored meaning.
5. The effects OpenFX host is now a real consumer. Its discovered catalog retains every scanned
   exact schema for editing, while its active catalog publishes schemas only when the isolated
   plugin is Ready; disabled, faulted, and quarantined operations therefore use this unchanged
   missing-node path.
6. Public graph integration tests and the effects preset contract are also current consumers. Presets
   instantiate and resave the unchanged editable state while implementations are absent or
   incompatible, then recover when the exact saved schema returns.
7. A future engine adapter still owns native discovery, worker containment, bounded IPC, deadlines,
   GPU-handle transport, version compatibility policy, implementation factories, and production
   compilation above the safe effects-side `IsolatedOfxAdapter` and node-neutral graph contracts.

Deterministic graph scheduling and lazy evaluation are implemented over caller-owned DAG payloads
with a bounded effects reference consumer but without a production engine catalog:

1. One request names a stored output endpoint, exact rational frame, and signed half-open pixel
   region. A node payload declares only the incoming stored edge requests needed for that output.
2. Discovery validates those routes, canonicalizes their order, and records only reached source
   endpoint, physical-frame, and exact-region keys. Equivalent physical times are one work key,
   while each distinct declared edge remains a semantic node input.
3. The planner publishes unique prerequisites and deterministic readiness batches. Dependencies
   occur in earlier batches, and equal-ready keys are ordered independently of graph insertion
   history or thread timing.
4. Pre-execution inspection binds each reached payload's exact schema, behavior, and canonical
   editable-state fingerprint to a versioned graph cache decision. Available keys include stable graph,
   endpoint, route, policy-scoped time and region, and complete upstream key lineage; disabled,
   nondeterministic, and dependency-blocked work stays explicitly non-cacheable.
5. Evaluation walks that exact schedule, evaluates identical work once per call, and returns opaque
   values, the schedule, and stable semantic completion keys. Diagnostic evaluation uses the same
   executor and pairs the unchanged result with monotonic planning, execution, and per-node timing.
   Timings do not participate in semantic inspection, result equality, or cache identity.
6. Cached evaluation accepts one caller-owned `EvaluationValueCache<V>`, checks the final key before
   node execution, and recursively stops at retained intermediate keys. Every adapter call receives
   stable graph identity, optional published revision, semantic graph lineage, and exact work
   identity. `superi-cache` memory and disk contexts add authoritative project, media, parameter,
   color, and render context, with host or device placement added for memory admission. Both scoped
   adapters derive a complete `FrameCacheKey` before either final or intermediate tier is consulted.
   Only successful cacheable work is offered for retention. Each admitted memory entry owns exact
   total and project byte and frame reservations, device entries also hold the shared GPU cache
   reservation, and a refusal selects an eligible scoped victim without changing the fresh evaluator
   result. Successful hits and
   insertions promote per-tier logical recency. Automatic pressure reclaims intermediate values
   before final frames and chooses oldest access within a tier; explicit management can also remove
   exact per-tier LRU victims. One graph edit invalidation atomically
   removes affected older entries and releases their reservations from both tiers, then fences late
   old-snapshot work while leaving unrelated semantic keys reusable. Later requests recompute exact
   evicted or invalidated results through the unchanged evaluator. Disk entries validate bounded
   versioned envelopes, caller-owned
   schema revisions, and payload digests before decode, publish through synchronized same-directory
   renames, and quarantine invalid bytes. A budget or persistence failure stores nothing or becomes
   a miss without changing the fresh evaluator result. Cache render orchestration derives the exact
   target frame key before dispatch, checks memory before persistent storage, promotes disk hits,
   and uses the same graph path for fresh work. Its bounded worker queue single-flights active exact
   frames and stages newly evaluated values until final cooperative cancellation, deadline, and
   progress checks pass.
   Memory inspection reports deterministic tier keys plus budget evidence, and exact clearing
   preserves graph revision fences. Persistent inspection classifies managed, quarantined,
   temporary, and unknown files without following links; clearing publishes an empty namespace
   before cleanup, and relocation publishes a complete renamed or destination-local staged copy
   before source removal. Derived inspection and clearing retain source, revision, purpose, quality,
   content, and byte evidence, after which exact lookup returns the authoritative original source.
7. `GraphEvaluationSnapshot<T, N>` retains one exact editable snapshot and uses a higher-tier
   `NodeCompiler<T, N>` to replace only node payloads while preserving graph identity, node IDs,
   edge routes, and checked topology. Each compilation receives the full snapshot, so authored
   parameter drivers remain visible. Scheduling, inspection, ordinary evaluation, cached
   evaluation, and diagnostic evaluation delegate to the same `LazyEvaluator` for editor, script,
   interactive, playback, export, and headless callers.
8. Public graph and effects tests prove linked and expression-driven parameter state enters
   runtime-node compilation, real effect and transition pixels evaluate through immutable old and
   new revisions, exact visual state changes alter diagnostics and cache identity, exact invalidated lazy
   work, equal role schedules, semantic inspection, cache keys, exact retained results, final-hit
   short circuiting, intermediate subtree pruning, immutable old revisions, fresh edited results,
   driver-expanded edit roots, selective two-tier cleanup, stale revision fencing, and contextual
   atomic compiler failure. Cache render orchestration, engine playback, and engine render-export
   are the first production role consumers of externally prepared evaluation snapshots, while effects is a deterministic
   headless CPU reference consumer with exact authoring, animation, and transition foundations. No production
   engine node catalog, API, CLI, or native GPU stage builds the canonical evaluation path, so
   `graph.evaluate` remains a disclosed stub in the canonical application slice.
9. Playback prediction validates an exact playhead, half-open bounds, and nonzero signed step, then
   emits at most 512 nearest-first critical, farther predictive, and trailing frame requests.
   `superi-engine::PlaybackPrefetcher` requires the playback domain, cancels the prior generation,
   and submits one playback-priority cache job without waiting. Each bounded frame evaluation uses
   the same snapshot and owned host cache identity. Polling exposes success, cancellation, or
   degraded evaluator failure without changing transport or final output.
10. Foreground playback accepts one prepared immutable graph, exact output request, complete scene
    cache identity, display transform, bounded audio producer and its device clock, lifecycle-owned
    worker pool, and bounded viewport sender. One playback-priority frame job evaluates through the
    same graph and cache, rejects time, scene-color, or alpha mismatch before retention, performs
    display color execution, and passes immutable media timing plus a distinct live deadline and
    duration to an engine coordinator over the audio-master or monotonic clock. The coordinator
    returns bounded hold, nominal or corrected presentation, explicit drop, and applied
    discontinuity recovery evidence without changing media timing. Viewport backpressure retains the
    exact payload and resolved presentation, so retry does not repeat a scheduler observation. The
    path remains nonblocking and leaves prepared source binding and native GPU submission to their
    separate owners.
11. `superi-engine::PlaybackTransport` composes foreground and prediction owners on the playback
    domain. Seek, scrub, pause, resume, frame step, signed rate, direction, and loop changes cancel
    stale generations, request callback-owned audio discard, and submit one protected exact frame.
    Checked frame-to-clock cadence uses fixed anchors and distinct deadlines. Optional late-frame
    policy skips only ordinary playing frames, protects user and loop intent, and forces visible
    progress at its positive ceiling. Immutable snapshots expose frame, prediction, viewport, and
    audio degradation. Render-export remains a separate exact-time consumer, and public dispatch
    remains absent.
12. Render-export accepts exact acquired-source routes, a current lifecycle permit, one immutable
    graph snapshot and decoded-frame binder, explicit delivery and audio stage owners, the ordinary
    backend registry, and an export-priority operation. It seeks and reads complete packets, drains
    selected decoders, evaluates exact graph scene envelopes, invokes delivery color or audio
    processing, selects each encoder once, drains every encoder, validates timing interval unions,
    metadata, formats, precision, color, alpha, and graph identity, and resets all completed state.
    It publishes complete in-memory elementary packet streams only after every codec reaches end of
    stream and the permit remains current. Partial reads, semantic drift, cancellation, degradation,
    or codec failure publish nothing and trigger fresh-context reset recovery. Container muxing,
    persistence, native GPU readback, progress streaming, arbitrary stream counts, API, and
    application integration remain separate owners.
13. Invalidation-to-render orchestration, ROI-plan-to-evaluator binding, cache invalidation
    invocation, automatic capacity policy, external directory coordination, and production engine
    catalog wiring remain separate later checkpoints. Cache owns bounded outer job dispatch for background
    population without moving priority or worker ownership into graph.

No public transport request envelope, dispatcher, event channel, subscription, broad public
transaction, script runtime, or UI is implemented. The engine-layer playback transport has no shell,
extension, automation, or closed-tier runtime consumer in this repository.

### Documented target, incomplete

Repository contracts describe one stable public command and event seam shared by UI, CLI, scripts,
extensions, automation, and closed-tier clients. Engine transactions are intended to coordinate
project, timeline, graph, caches, undo, persistence, lifecycle, playback, render, export, and event
publication. Bulk frames, audio, packets, and GPU resources are intended to stay behind that seam.

That target must not be read as current behavior. Timeline now owns foundational validated
editorial state plus selection, targeting, sync locks, linked selection, clip groups, exact clip
retiming, six primary operations, and ripple, roll, slip, slide, razor, trim, extend, three-point,
and four-point edits.
Timeline compilation now publishes native editorial state into the generic editable graph, and the
engine retains that compilation with exact opened sources and decoders as one preparation bundle.
No production engine catalog evaluates that compiled state. Effects can instantiate compatible shared
processing nodes, attach strict exact keyframe payloads, and evaluate them through its bounded CPU
reference, but no production timeline attachment or project owner closes the flow. Engine
foreground playback evaluates caller-prepared graph values, performs CPU display conversion, admits
prepared audio, and coordinates viewport delivery with the device clock under normal, late,
discontinuous, and recovered conditions. Engine transport owns exact interactive controls and
bounded ordinary frame dropping over that prepared path, but it does not bind the prepared source
bundle or timeline audio renderer. Engine render-export separately binds explicit acquired source
routes through decode, immutable graph evaluation, caller-owned delivery color or audio processing,
and encode with exact elementary-stream publication and recovery. It does not consume the
timeline-compiled runtime graph, mux or publish a container, or expose API control. No complete
edit-to-source-backed-playback or edit-to-export control flow exists.

`docs/vertical-slice.md` now defines the exact first control flow as scenario
`superi.slice.canonical.v1`: one immutable WebM and AV1 fixture role, one 24 fps video track, a
half-open middle trim, one typed horizontal-mirror transform node, explicit sRGB delivery, and eight
ordered stage records. It permits disclosed stubs only for contract conformance. Runtime
conformance requires every stage to use its production owner. The CLI now executes the complete
control sequence at contract conformance, with six stages explicitly reported as stubs. Report
schema 1.1.0 names timing and memory units, records before and after resident bytes for every stage,
and reports the largest resident value observed at those boundaries.

## Media ingest and codec flow

### Implemented components

`superi-media-io` owns the neutral values and lifecycle:

```text
SourceRequest
  -> bounded content probe and deterministic backend selection
  -> MediaSource and SourceInfo
  -> optional paired stream selection
  -> timed Packet values in decode order
  -> selected Decoder implementation
  -> VideoFrame or AudioBlock
```

Content bytes, not file extensions, are authoritative for source selection. A persistent project
`MediaId` is separate from source location. Container open computes a SHA-256 content fingerprint,
and relink accepts replacement location only when an expected fingerprint matches. Backend
selection orders candidates by tier, priority, and stable backend ID. Fallback candidates remain
explicit and are not silently tried after a selected open or codec creation fails.

The implemented source backends are Matroska/WebM, MP4/MOV, MXF, and WAVE/RF64/AIFF PCM. They
produce container-neutral streams and packets but do not decode. `superi-codecs-rs` provides the
ordinary priority-100 software decoders and encoders for AV1, FLAC, MP3, Opus, PCM, Vorbis, VP8,
and VP9. `superi-codecs-platform`, when enabled, contributes priority-200 host operations. Apple
uses VideoToolbox and AudioConverter, Windows uses discovered Media Foundation transforms, and
Linux uses VA-API. `superi-codecs-vendor`, when explicitly configured, contributes priority-500
decode-only source and codec adapters backed by separate worker processes.

Codec decoders receive elementary packets with exact timing, configuration, metadata, and stream
identity. They return safe CPU, external, or backend-owned frames through media-I/O interfaces.
Codec encoders receive decoded frames or audio blocks and return elementary packets. Containers do
not decode, and codecs do not demux.

The engine now composes caller-prepared decoded inputs with one selected encoder for proxy or
optimized-media generation. It validates exact purpose, quality, codec, timebase, representation,
color or channel meaning against the cache request, reaches real default AV1 encoding, and publishes
only complete elementary packets after end of stream. Cache identity preserves authoritative
source ID, content, and revision, and a missing or failed generation leaves that source as fallback.
The engine also adapts a selected complete proxy artifact to `MediaSource`, preserving packets and
authoritative source identity, or lazily returns a verified original source under deterministic
scheduler policy.

The default engine registry now includes all four in-tree container source adapters. Its resource
preparation path compiles one timeline graph, discovers the exact reachable linked-media set, binds
project fingerprints, probes and opens each source, maps explicitly requested stream descriptors to
decoder configurations, and constructs each selected decoder once. One returned owner retains the
compiled graph, live sources and decoders, and complete selection evidence for later playback,
render, and export consumers.

### Integration gaps

There is no muxer, export container writer, production image-sequence backend, multiple-stem stream
selector, source-to-generation decoder, or source-to-playback coordinator. Resource preparation
creates source and decoder owners but does not itself schedule packet flow, choose paired streams,
upload decoded output, evaluate graph nodes, or synchronize audio and video. Render-export now
consumes explicit acquired routes and schedules their packet, decode, graph or audio, and encode
lifecycles, but it does not choose paired streams from editorial intent or mux output. Derived generation does not
render or resize its caller-prepared inputs or persist its packets. Proxy resolution is caller-owned
and is not wired to playback or export orchestration.
Platform and vendor frames can be external or
backend-owned, but the engine upload path currently accepts CPU frames only. Higher-level decode
selection is expected to request a CPU fallback, but preparation currently selects by codec, tier,
priority, and stable ID without an output-storage constraint.

## Image, color, and GPU flow

The host image and decoded-video representations are intentionally distinct.

`superi-image` owns dense CPU `Image` values, native `ImageStorage` and `ImageAccess`, multipart
`StillImage`, eight still formats, numbered sequences, deterministic CPU operations, alpha
operations, thumbnails, waveforms, and reference comparison. Native access does not silently
repack into dense processing storage. Dense processing does not silently upload to a GPU.

`superi-color` consumes dense images for explicit input and output transforms. The input path validates
source family, transfer, primaries, range, matrix expectations, and alpha; decodes transfer;
converts primaries in binary64; applies an explicit gamut policy; and produces canonical
premultiplied scene-linear working storage. Canonical storage is RGBA binary16 with ACEScg as the
default working space, while numerically sensitive computation uses a separate RGBA binary32
value. LUT parsing and evaluation, HDR transfer functions, ICC profile validation and discovery,
and monitor-profile freshness checks are also implemented. Validated rule sets retain source roles,
select the first applicable ordered display view or an independent delivery output, and apply named
LUT looks in declared order. The output path validates one display or
deliverable target, converts linear working primaries, encodes SDR, HLG, or absolute PQ, preserves
premultiplied alpha and image identity, and emits authoritative full-range RGB binary32 output for
later storage conversion. An explicit luminance shoulder can preserve RGB ratios while mapping a
declared source peak before transfer encoding. A separate legal-range encoder unassociates the
output, rounds RGB to exact 8 through 16-bit legal codes, preserves alpha, and emits normalized
limited-range straight-alpha storage without choosing a YUV matrix.
Strict versioned JSON configuration resolves bounded named scene-linear working spaces, aliases, and
roles through the same `WorkingSpace` API. Serializable project settings pin one canonical space to
the config ID and normalized semantic SHA-256, rejecting semantic drift instead of silently changing
scene meaning.

The managed GPU wide-gamut path derives its WGSL matrix and gamut constants from the same binary64
reference transform, validates a canonical single-plane `Rgba16Float` source, allocates a managed
`Rgba16Float` destination, and returns an owned compute-pass batch or submitted fence. Source,
destination, bindings, pipeline, and command dependencies remain retained through submission. CPU
pixels are exposed only by an explicit readback owner used by export or reference verification.

The versioned color baseline now exercises that public CPU transform path with eight compact SDR,
Display P3, PQ, HLG, alpha, f16, and f32 images. It separately maps three ACEScg f32 payloads through
the public media-I/O image-sequence source with noncontiguous file numbers and exact 24000/1001
presentation timestamps. These are real consumer contracts over canonical raw fixtures, not an
engine, GPU, viewer, still-image decoder, or rendered golden-output path.

`superi-gpu` owns device identities, resources, memory budgets, pooled textures, decoded upload,
storage conversion, shaders, passes, the exclusive submission queue, fences, readback, native
surfaces, diagnostics, and device-loss reconstruction. Its storage converter may change packing,
numeric representation, matrix, range, subsampling, and alpha association. It refuses to change
primaries or transfer functions because those are color-management operations.

The implemented decoded-frame bridge is:

```text
superi-media-io VideoFrame with CPU storage and exact color pipeline
  -> superi-engine VideoFrameUploader
  -> superi-gpu DecodedFrameUploader
  -> pooled GPU plane textures
  -> UploadedVideoFrame retaining format, timing, metadata, color pipeline, and GPU ownership
```

The uploader preserves decoded bits, plane order, timestamps, duration, format, metadata, and the
complete image-owned color pipeline. It
uses direct row writes when compatible and a tight CPU repack otherwise. Logical initialized
texture extent remains distinct from aligned physical allocation extent. Pooled allocations and
all command dependencies must remain retained until the matching fence retires.

An adjacent metadata-only path carries exact source color tags, ICC bytes, named-space identity,
working and current spaces, and ordered input or creative transforms from media into graph,
timeline, and complete cache identity. Engine render metadata derives independent display and
delivery branches by appending a correctly typed terminal stage without mutating cached scene state.
This path does not execute transforms or render pixels.

No implemented engine path sends `UploadedVideoFrame` into graph evaluation, pixel color processing,
cache storage, playback, display, or encode. Official graph identifiers, schema registration, immutable
discovery, generic graph topology storage, typed input and output bindings, and schema-level
connection compatibility, a schema-bound editable graph, atomic mutation, and a caller-owned lazy
evaluator plus snapshot-owned typed parameter links and bounded expressions exist. Effects also
owns exact editable animation curves, strict visual composition artifacts, animated mask stacks,
exact-frame rotoscope artifacts, complete point, planar, object, and camera tracking artifacts, and
styled text layers with inspectable glyph layout that survive generic graph parameter serialization
through animatable authoring definitions, but none is
connected to a production runtime node. Visual composition resolution retains nested paths and time
maps structurally without applying transforms or pixels. A role-neutral
evaluation snapshot compiles editable instances into caller-owned evaluator payloads without
changing topology. Effects implements that seam for versioned visual schemas, bounded CPU reference
images, and CPU text layout metadata, but no production engine catalog connects it to a GPU value.
Color input, output,
LUT, and rule transforms remain CPU implementations and
have no graph-visible node catalog. A GPU wide-gamut transform exists as a direct public surface,
but no engine or graph consumer composes it with the complete display, delivery, ICC, viewport, or
export pipeline.
`MonitorAwareViewport` prevents stale-profile presentation but does not color-convert a frame.

GPU readback is explicit and limited to export or thumbnail storage bytes. It performs no color
conversion, swizzle, encoding, or resize. Image preview and CPU reference validation require an
explicit readback by a GPU owner before they can consume the result.

## Concurrency and operation flow

`superi-concurrency` supplies reusable execution and liveness mechanisms:

- Seven domains identify UI, engine control, playback, render, audio, background jobs, and GPU
  submission, with explicit blocking and allocation policy.
- A bounded worker pool combines local queues, stealing, deterministic 8:4:2:1 priority service,
  cooperative cancellation, deadlines, progress, typed completion, and panic containment.
- Fixed-capacity handoffs never drop a saturated payload. They return ownership to the producer for
  explicit retry and keep route capacity independent.
- `DomainOwned<T>` keeps mutable state in one execution domain. `SnapshotPublisher<T>` publishes
  immutable generation-tagged `Arc` snapshots for cross-thread readers.
- Playback clocks recompute from monotonic or audio-sample anchors. The A/V scheduler produces
  wait, present, drop, or rebase instructions but never performs them. Engine foreground playback
  is the current concrete consumer that applies a requested rebase and retains resolved presentation
  evidence across viewport backpressure.
- Lifecycle coordination uses revisioned requests and exact participant acknowledgements.
  Liveness probes and explicit wait-resource ownership produce starvation and deadlock findings.

Media and codec operations use `superi-media-io::OperationContext`, which carries priority,
cancellation, and an optional monotonic deadline. The vendor adapter keeps that context active
while waiting on process I/O. Platform and Rust codecs check it at public boundaries and selected
loops. Concurrency jobs use their own `JobControl` and require the job closure to call checkpoints.
Both models are cooperative. Neither can preempt a blocking operating-system call, native codec
call, or closure that omits checkpoints.

These mechanisms are not yet a complete composed runtime. Engine proxy resolution consumes the
derived-media selection policy, and engine playback consumes playback domain ownership, bounded
workers, playback priority, cooperative cancellation, progress, and nonblocking completion for
predictive cache population. Foreground playback adds one exact frame job, audio-master A/V
coordination with bounded correction and discontinuity recovery, monotonic fallback, and a bounded
lossless viewport handoff. Engine transport composes cancellation, clock reanchoring, distinct frame
deadlines and live durations, prediction, and handoff retry into exact interactive control. Engine
lifecycle keeps authoritative state on EngineControl,
composes one acknowledged lifecycle participant, publishes immutable generation-tagged snapshots,
and exposes one lock-free signal. Its exact action tokens sequence shared-state, playback,
rendering, and export initialization, recovery, reverse teardown, and restart without performing
subsystem work inline. Render-export consumes that control plane by requiring the same current
export permit before codec creation and artifact publication, but it does not use concurrency jobs
or make lifecycle acquire codec resources. Audio enforces the platform-owned audio domain for fixed prepared
graph processing and exact callback scheduling. Device input uses independent preallocated capture
and monitoring queues with exact physical sample coordinates. Device output uses a preallocated
lock-free queue and publishes the audio master clock consumed by foreground playback. Cache's
background render queue
constructs a bounded pool, submits cache-kind work at background priority, carries `JobControl`,
exposes typed completion and snapshots, and layers exact-frame single-flight over dispatch. Graph
has no direct production concurrency consumer. Playback and transport accept an externally
lifecycle-owned pool, a caller-created handoff, and a caller-paired audio producer and device clock.
Lifecycle does not yet construct or drive those owners, and no engine owner binds scheduled source
slices to the prepared audio graph or a native device setup. The `submit` module is a placeholder. A
contract test hosts the real non-Send `GpuSubmissionQueue` inside the GPU submission domain, but no
engine owner wires that pattern into playback or render.

## Engine, API, CLI, and tool roles

`superi-engine` is the intended integration owner. It implements fixed canonical command state,
full-state undo plus redo, codec registry assembly, deterministic capability introspection, and
CPU-decoded frame upload, plus codec-neutral proxy and optimized-media packet generation and
transparent proxy or original-source resolution, playback-domain predictive cache population, and
transactional timeline graph plus source and decoder preparation. It evaluates exact foreground
graph values through shared cache retention, performs CPU display color execution, admits bounded
audio, coordinates viewport delivery with the shared clock through explicit wait, correction, drop,
and rebase outcomes, retains frames plus resolved presentation through backpressure, and owns exact
seek, scrub, pause, resume, frame step, signed-rate, direction, loop, and bounded drop policy. It
also composes exact acquired-source reads, decode, shared graph evaluation, caller-owned delivery
color and audio stages, deterministic encoder selection, complete codec drains, strict semantic
validation, reset recovery, and all-or-nothing elementary packet publication. It atomically
coordinates production timeline edit batches with audio-owned clip mix intent.
It owns a canonical lifecycle control plane that sequences shared-state, playback, rendering, and
export subsystem actions, publishes one immutable health and admission snapshot, isolates
recoverable degradation, rolls back failed startup, preserves dependency-safe reverse teardown,
and restarts with a fresh lifetime.
The command model is a reference boundary, not production project, timeline, or graph ownership.
Prepared resources, foreground playback, interactive transport, and render-export do not form a
source-backed playback or public control flow. Variable-rate decoded audio, native GPU presentation
and readback, container muxing, and file publication remain absent. Errors, nodes, plugins, and
validation remain explicit placeholders.

`superi-api` is the stable public facade. It keeps implementation types private and exposes strict
versioned capability records plus the fixed canonical scenario action and complete state projection.
It has no transport or broad editor command set.

`superi-cli` is a binary boundary, not a library. It accepts only the normalized slice command plus
help and version, validates repository fixture authority, drives `ScenarioApi`, writes the strict
schema 1.1.0 report with all-stage timing, resident-memory, and versioned expectation evidence, and
publishes a non-playable contract artifact through collision-safe paths. Its project expectation
digest is portable across checkout roots, while strict undo and redo comparison and reported media
paths remain unchanged.

`superi-fixture-tool` is a repository utility, not an engine component. It validates schema,
identity, provenance, lineage, payload ownership, byte counts, hashes, path safety, and unmanaged
files under `open/test-fixtures`. Validation is offline and read-only. Its deterministic video
command creates an absent output directory containing all 207 current pixel-format and
standard-frame-rate cases, a fixed catalog, raw payload, and exact manifest. Its audio command
creates three WAVEFORMATEXTENSIBLE PCM16 files covering 44,100 Hz stereo, 48,000 Hz 5.1, and 96,000
Hz 7.1 with exact sample timing, channel masks, synchronized signal boundaries, and integer-only
waveforms. Its timing command emits five fixed CFR, VFR, drop-frame, gap, and reset cases with 18
samples and explicit continuity segments. Its color command emits eight deterministic SDR,
wide-gamut, HDR, alpha, high-bit-depth, and sequence images with two strict catalogs and one 448-byte
sample payload. Its media-error command emits four fixed PCM container
cases for malformed, truncated, unsupported, and post-open partially readable behavior, with one
strict outcome catalog. Its OTIO command creates two native JSON
timelines and an expectation record from fixed Rust values, including the first slice projection,
clips, gaps, transition relationships, owner-relative markers, nesting, two linear rate changes,
stable metadata IDs, and preserve plus diagnose contracts for unsupported effects. All generators
refuse overwrite. The tool does not execute recorded commands, prove repository-history
immutability, or verify legal and semantic
claims inside arbitrary free-form provenance fields.

The same utility exposes schema-versioned, read-only golden verification for frame, audio, timeline,
and project outputs. Frame and audio envelopes compare exact payload bytes together with layout and
meaning metadata, while timeline and project envelopes canonicalize object-key order without
changing array order or scalar values. Verification reports expected and actual SHA-256 evidence,
never writes expected files, and provides no automatic update or bless path.

`superi-dependency-check` is also a repository utility. It reads the locked workspace graph offline
and fails when a runtime crate adds an unreviewed normal, build, or dev-only internal edge, or when a
new runtime crate has no explicit policy. The structure guide and executable policy are reviewed as
one architecture contract.

`superi-boundary-tool` is a dependency-free repository utility, not an engine component. It scans
Cargo and Rust source deterministically, rejects forbidden network clients and direct socket APIs,
rejects supported open-to-closed import routes and symlinks, and runs before each locked hosted
workspace build as well as through the canonical workspace test gate.

`superi-test-report` is an offline repository utility that validates explicit schema-versioned lane
evidence and derives canonical status, summary counts, performance regressions, golden mismatches,
flaky tests, and platform gaps. It retains retry and artifact evidence, creates missing-suite gaps,
writes valid blocking reports before returning failure, and refuses to replace an existing report.

## Shared invariants

The following constraints cross multiple modules and should be preserved together:

- Open and closed dependency direction is one way. Open behavior cannot require closed code,
  accounts, remote services, or a network.
- Shared identifiers, exact timebases, half-open ranges, stable codes, color and pixel tags, channel
  order, error categories, recoverability, and primitive serialization are owned by `superi-core`.
- Project identity is separate from replaceable media location. Content fingerprints protect
  relinking, while metadata and source timing remain attached to the artifact that produced them.
- Timeline media organization retains stable bin and smart collection identities. Manual bin
  membership and dynamic query results never replace clip `MediaId` links, and mismatched relink
  candidates retain evidence without replacing the active target.
- Deterministic ordering is explicit. Stable backend IDs break selection ties; ordered maps and
  sets stabilize public snapshots, fixtures, diagnostics, and validator output.
- Editable graph state has one optimistic revision and immutable shared snapshots. A nonempty
  transaction publishes exactly once only after every ordered mutation passes, while stale or
  failed batches preserve the prior state and revision. Presentation order never replaces DAG
  processing order.
- Parameter drivers are ordinary typed editable graph state in canonical target order. Every
  dependency is explicit, direct links preserve exact types and payloads, expressions are bounded
  and pure, parameter cycles fail before publication, and all caller roles evaluate one immutable
  snapshot through the same request-local result path.
- Timeline selection, per-track targeting and sync locks, linked selection, and clip groups publish
  inside the same revision-checked project transaction as clip source and record state. Stable
  surviving identities retain intent, direct selection bypasses relationships, and sync-sensitive
  track resolution preserves timeline layer order.
- Multicam source timelines own ordered stable angle identity, synchronization provenance,
  metadata, and local source membership. Ordinary nested target clips own independent gapless
  switch and audio intent, resolve through both clip time maps exactly, and inherit state through
  structural fragment and replacement edits inside the same project transaction.
- Plugin availability is derived from immutable graph and registry snapshots, never persisted into
  authored state. Only an exact saved schema definition is available; absent or conflicting schemas
  retain typed placeholders and produce one stable degraded evaluation result for every caller.
- Graph ROI derives exact connected work from one immutable snapshot, current per-output domains,
  schema behavior, and stable typed edges. Plans retain graph identity and revision, never mutate
  state, and cannot be treated as cache-generation ownership.
- Runtime-node compilation receives that complete immutable graph snapshot, including parameter
  drivers, then preserves checked topology and delegates every caller role to one lazy evaluator.
  Retained evaluation uses one explicit caller-owned adapter and only complete semantic cache keys;
  final and intermediate hits cannot change authored state or result meaning. Cache-local LRU
  eviction changes only retained availability, so victims deterministically recompute without
  changing project or output meaning.
- Cache render jobs derive the complete requested frame identity before bounded dispatch. Memory
  and persistent hits retain exact meaning, persistent promotion changes only availability, one
  queue single-flights each active exact frame, and cancelled or timed-out fresh work publishes no
  staged final or intermediate values.
- Semantic edit invalidation compares immutable revisions, expands changed parameters through both
  driver graphs, and propagates roots through both pixel topologies. The cache applies that plan to
  both tiers under one lock, preserves unaffected semantic keys, and rejects affected work older
  than the graph and node revision fence.
- Capability declarations are metadata, not proof that a factory or every declared format can run.
  Introspection must not instantiate codecs or sources.
- Backend fallback is explicit. The registry, platform adapters, and vendor workers do not silently
  switch implementation after the chosen operation fails.
- Engine media preparation binds one retained timeline compilation to the exact reachable source
  request set and explicit decoder streams. Project fingerprint conflicts, opened identity drift,
  missing streams, cancellation, and selected backend failures abort the whole unpublished bundle;
  recorded fallback candidates are evidence rather than exception retries.
- Derived media binds exact source identity and revision to purpose, quality, and complete canonical
  encoder settings. Generation publishes only after end of stream and complete packet hashing;
  failure retains prior complete media or the original source fallback. It cannot change project,
  graph, source, or final-render meaning.
- Cache warming ranks only exact in-bounds timeline frames under a hard policy limit. It does not
  derive a cache key, store a value, choose source or proxy quality, or alter fallback. Each caller
  must use the ordinary revisioned graph request and complete cache identity, so stale, evicted, or
  skipped speculative work remains a transparent demand miss.
- Pixel storage, alpha association, color interpretation, dimensions, timing, and buffer ownership
  are separate contracts. Constructing valid metadata does not prove a codec, color transform, GPU
  operation, or output supports it.
- Color tags do not execute transforms. Input, working, display, and delivery transforms require
  explicit owners. GPU storage conversion must not silently change primaries or transfer.
- GPU device identity and generation scope every managed object. Old, foreign, or recovered-device
  resources cannot be mixed. Submission retention must outlive fence retirement.
- Bounded allocation, queue capacity, pressure, cancellation, and backpressure are explicit at each
  implemented boundary. A local bound must not be generalized into a global process-memory claim.
- Canonical slice instrumentation refreshes only the current process exactly twice per stage. Its
  largest observed boundary value is not a continuously sampled intra-stage peak or soak result.
- Cancellation and deadlines are cooperative. A single blocking foreign call remains a latency
  boundary even when surrounding loops poll correctly.
- Alpha is not silently discarded. Codec and conversion paths reject unsupported alpha, and image
  operations distinguish color channels from auxiliary channels.
- Errors retain category, recoverability, component, operation, and contextual detail. Raw
  diagnostic content is not user-safe by default; presentation must use the safe projection.
- Unsafe Rust is denied by default. Narrow native boundaries require local safety reasoning,
  ownership proof, audit inventory updates, and target-specific verification.
- Public bulk media stays out of the transport-neutral API. The API exposes control and metadata,
  while packets, frames, audio, textures, and command buffers remain behind engine boundaries.

## Test and verification strategy

Implemented modules primarily use public integration contract files rather than broad end-to-end
application tests. `superi-core`, `superi-graph`, `superi-image`, `superi-media-io`, `superi-codecs-rs`,
`superi-codecs-vendor`, `superi-color`, `superi-concurrency`, `superi-gpu`, and the implemented
engine and API slices all have focused contracts around their public values and lifecycles.
Container-to-codec tests and engine capability tests provide selected cross-crate composition. The
derived-media engine contracts additionally drive the real default AV1 encoder through complete
packet publication and deterministic reuse, then exercise exact or lower-quality transparent
substitution, replacement, generated packet reads, keyframe seek, strict freshness, and verified
original or source-only fallback. They do not claim rendering, muxing, persistence, playback-clock
integration, or container delivery.
The cache warming contract maps deterministic edit and scrub targets through the real graph
evaluator and bounded memory cache, then proves demand reuse, source-fingerprint freshness, and
unchanged recomputation after pressure. It does not claim a production editor, playback, engine,
API, UI, decoder, or proxy consumer.

The effects preset contract exercises the public effects and graph seams together. It proves
complete exact-schema capture, fresh workflow-neutral instances, deterministic current and legacy
documents, transactional explicit schema migration, and edit plus resave continuity through real
unregistered and incompatible graph placeholders before exact-schema recovery. It does not claim a
project container, atomic project save, autosave journal, plugin host, or production engine binder.

Repository checkpoint execution also has a deterministic local selector at
`.agents/skills/superi-execution/scripts/verify_checkpoint.py`. Given a recorded base revision, it
collects committed, staged, unstaged, and untracked changes, validates changed Python and JSON,
always validates codebase maps, and conditionally runs applicable workflow, dependency, Rust,
fixture, codec-feature, slice, shell, and frontend gates. `--full` selects the complete supported
local set, while checkpoint-specific proof remains mandatory beyond this floor.

Test source documents an intended or previously exercised contract, but its presence is not a fresh
passing result. Individual module maps state whether synthesis ran a suite. At mapping time, the
core and fixture-tool maps record fresh successful runs; several other maps explicitly state that
their synthesis did not execute the tests.

Native and GPU proof is environment-dependent. Many GPU tests return successfully without running
device work when no adapter is available. Timestamp paths may skip when features are absent.
Platform codec proof must run on macOS, Windows, or Linux with the actual framework, transform, or
driver. A host-independent parser test does not prove native lifecycle, pixel fidelity, teardown,
or hardware behavior.

Repository-level CI now has five implemented workflow surfaces. The dependency-policy workflow
checks licenses and sources with cargo-deny on Ubuntu 24.04. The cross-platform workflow runs the
locked Rust quality suite on GitHub-hosted macOS, Windows, and Ubuntu runners. Pull requests and
pushes to `main` run five matrix lanes: blocking `ci-macos-26-arm64` on `macos-26`, blocking
`ci-macos-15-x64` on `macos-15-intel`, blocking `ci-windows-2025-x64` on `windows-2025`, blocking
`ci-ubuntu-24-x64` on `ubuntu-24.04`, and nonblocking preview `ci-ubuntu-26-x64` on
`ubuntu-26.04`. The same matrix runs on the weekly Monday 07:00 UTC schedule and manual dispatch;
those two triggers also add the blocking `ci-ubuntu-22-x64` job on `ubuntu-22.04`.

Every hosted Rust lane checks out with read-only permissions, an immutable pinned checkout action,
and no persisted credentials. It installs the current stable toolchain with rustfmt and Clippy,
records Rust, Cargo, toolchain, and commit identity, then runs the locked open-tree boundary command,
formatting, a locked workspace build, locked workspace tests, strict all-target Clippy, and locked
documentation tests from `open/`.
Linux lanes use one repository-owned helper to install exact build tools, verify and build the
official libva 2.22.0 source at a pinned SHA-256, and publish its header, pkg-config, and runtime
paths. The helper installs the GBM development target, publishes the private native-linker path,
and installs `nasm`, while Intel macOS lanes install `nasm` with Homebrew. Linux
and macOS lanes build the approved libvpx 1.16.0 source after verifying its pinned archive checksum,
then expose the exact shared runtime to capability and codec tests. Windows builds libvpx 1.16.0
with VP9 high-bit-depth support from a pinned vcpkg registry revision as a static MSVC archive,
relinks it into a DLL with the reviewed production export surface, verifies the exact runtime, and
publishes it to those same strict tests.
Hosted macOS skips only three named VideoToolbox or AudioConverter lifecycle tests whose physical
codec evidence belongs to the documented hardware lane; Linux and Windows run the exact full
workspace test command. Matrix fail-fast is disabled, superseded branch runs are cancelled, and
each build has a 90-minute timeout. Ubuntu 26.04 remains experimental, so its failure does not fail
the workflow.

The matrix records an explicit `os_codecs` policy. Both macOS lanes, Windows 2025, and Ubuntu 26.04
build the real CLI consumer with `os-codecs` and run the engine and API consumer suites with their
forwarded features. Ubuntu 24.04 and the separate Ubuntu 22.04 job remain default-only because
their distribution libva APIs are below the platform crate's required version. The default AV1
path pins crates.io rav1d 1.1.0, whose packaged build script removes the broken rav1d 1.0.0 MSVC
reference to an excluded compatibility source.

The durable CI checkpoint record also reports focused workflow-contract verification, one local
locked workspace build with stable Rust 1.97.0, and successful offline fixture-tool policy tests.
The boundary scan is now a recurring workflow step; the other local verification remains delivery
evidence rather than hosted workflow behavior.

The frontend workflow runs on pull requests, pushes to `main`, and manual dispatch using a read-only
Ubuntu 24.04 job. It installs exact Node.js 24.13.0, uses `npm ci` against the committed lockfile,
runs strict no-emit TypeScript 5.9.3 checking, creates a Vite 7.3.6 production bundle, and verifies
the workflow contract plus generated hashed JavaScript entry. Its `ci/frontend-smoke/` consumer is
an isolated toolchain contract, not the deferred React application or Tauri desktop shell.

The Tauri Rust workflow runs on pull requests, pushes to `main`, and manual dispatch across macOS 26
arm64, macOS 15 Intel, Windows 2025, and Ubuntu 24.04. Its pinned CI-only Tauri 2 host uses one
generic command configuration for a mock-runtime unit test and the real native wry builder. Every
blocking lane checks formatting, locked tests, strict all-target Clippy, and locked native binary
compilation; Linux installs WebKitGTK 4.1 and the documented desktop integration prerequisites.
This proves the native host toolchain boundary, not the deferred Phase 3 application or hardware
behavior.

The dedicated network-isolated workflow prepares locked Cargo artifacts, checksum-pinned libva 2.22
and libvpx 1.16 runtimes, and nasm on Ubuntu 24.04 while online. It transfers the private libva
header, pkg-config, native linker, and runtime linker paths plus the approved libvpx path through
the privileged namespace boundary, then runs workspace tests, canonical fixture validation, and the
canonical headless slice inside a distinct Linux network namespace with only
loopback as verified through the namespace-aware procfs network view, no IPv4 route, a failed
numeric outbound probe, and Cargo offline mode. Hosted run
`29308007012` stopped before namespace entry because distribution libva API 1.20 could not satisfy
the unchanged H.266 API 1.22 requirement; both Rust workflows now use the shared source provisioner.
The final hosted run remains the offline execution proof, not an offline acquisition proof or a
runtime import-to-export slice.

The cross-platform Rust workflow does not run an all-container malformed-input matrix beyond
default workspace contracts, frontend or Tauri checks, golden comparisons, benchmarks, soak, or
the vertical slice. The
separate frontend workflow does not prove React, Tauri, the native viewport, API integration,
editorial behavior, or product UI. Neither is an MSRV lane,
and neither is an end-to-end offline build proof because hosted setup and installation may use the network. Hosted
virtual machines also do not satisfy the
physical GPU, display, audio-device, hardware-codec, performance, and long-session lanes in
`docs/platform-testing.md`. Real platform codec lifecycle, pixel or sample fidelity, driver
behavior, device loss, presentation, and teardown still require their owning operating systems and
physical hardware.

Shared fixtures are versioned, immutable by repository policy, and validated offline with
`superi-fixture-tool`. The deterministic video baseline adds 207 one-frame cases across all 23
current pixel formats and nine standard frame rates. The tool proves byte reproduction, while the
`superi-media-io` consumer proves exact geometry, numeric representation, timing, hashes, and public
frame construction. The synchronized audio baseline adds three 100 ms WAVE files across common
sample rates and canonical stereo, 5.1, and 7.1 layouts. Byte reproduction and production PCM-source
consumption prove sample clocks, masks, routing order, exact samples, shared signal boundaries, and
bounded continuity. The deterministic timing baseline adds five cases and 18 samples for CFR,
decode-order VFR, 29.97 drop-frame labels, a forward gap, and a reset. Its media-I/O consumer proves
real packet and presentation maps, unsegmented discontinuity rejection, and reversible explicit
segments. The deterministic color baseline adds eight SDR, wide-gamut, PQ, HLG, alpha,
f16, and f32 images plus three ACEScg sequence frames. Public color transforms prove transfer order,
HDR scene meaning, reference-white handling, output intent, alpha association, and exact high-depth
bits, while public media sequence access proves logical, file, and presentation identity. It does
not prove encoded HDR, still-image decoding, display rendering, perceptual golden tolerances, or GPU
color execution. The deterministic media-error baseline adds malformed WAVE, truncated AIFF, unsupported
AIFC, and post-open partial-read coverage. Its production PCM consumer proves shared error and
recovery classifications plus exact aligned packet and corruption evidence. These remain synthetic
raw-frame, PCM-container, timing-metadata, color, and focused PCM-failure proofs, not encoded codec
corruption, malformed Matroska, MP4, or MXF, hardware, playback, device, A/V synchronization,
scheduling, or editorial runtime proof. Snapshot validation still does not prove Git-history immutability,
provenance truth, legal clearance, or semantic quality beyond focused contracts. The separate
golden harness baseline commits exact typed examples for frame, audio, timeline, and project
comparison. It proves the reusable verifier and canonical format, while real renderer, mixer,
timeline-engine, and project-runtime outputs remain consumer-owned future cases. The separate
`slice/video-cfr` fixture provides one digest-bound 96-frame AV1 WebM for the canonical runner. Its
canonical runner still treats decoded traits as expectations, while engine resource and render-export
contracts now open it with the production WebM source and decode all 96 frames through the in-tree
AV1 backend. The render-export lane evaluates those frames through one shared graph and delivery
stage, then rejects the selected VP9 encoder's duration rounding before publication. The derived
`slice/expectations` version 1 remains immutable historical data. Current version 2 adds a portable
project-state digest while retaining the same 48 mirrored RGBA8 frame identities, explicit pixel
and PCM tolerances, synchronized audio probes, timestamps, and target export metadata. The CLI
validates those applicable values but cannot compare rendered pixels that no current stage produces.

The deterministic OTIO baseline adds a 48-frame native JSON projection of the canonical slice plus
a 120-frame coverage timeline. The coverage payload includes clips, gap, transition adjacency,
owner-relative markers, a trimmed nested Stack, 2.0 and 0.5 LinearTimeWarp effects, stable object
identity, and explicit unsupported-effect pointers. `superi-timeline` now imports those documents
into the ordinary typed native project, preserves complete source templates, and exports the
current edited hierarchy through an explicit OTIO_CORE 0.18.1 target. Supported names, ranges,
media links, transition handles, marker values, metadata, and linear retime scalars remain directly
editable; unsupported fields and effects remain opaque with stable warning pointers. The public
headless example emits deterministic JSON, and official OpenTimelineIO 0.18.1 loads, target-writes,
rereads, and finds both Rust-produced outputs equivalent at their exact 48-frame and 120-frame
durations.

The native timeline model now exposes checked source-to-record range maps, media and nested source
availability context, and embeddable video, audio, caption, and data semantics using core-owned
clocks, identifiers, and ordered channel meanings. Range mapping is exact across clocks, media
overscan remains editable, and nested availability derives from the linked timeline. Audio routes
require one explicit decision per source channel, sample placements retain typed clip links through
split and trim, and continuity reports expose every record gap, overlap, source jump, or linked-clip
change. Tracks embed those semantics in the validated native timeline container. Timeline-local
project state also owns stable manual bins and sub-bins, saved metadata and relink queries, and
explicit online, missing, unverified, or fingerprint-mismatch evidence. Those values publish in the
same atomic project revision without replacing clip media identity or flattening nested sequences.
Timeline-local
edit state adds exact or relationship-expanded selection, stable per-track target and sync-lock
intent, canonical clip links and groups, direct member control, deterministic target and sync
projection, and structural reconciliation inside the same project transaction. Atomic foundational
edit batches now add ripple,
roll, slip, slide, razor, trim, extend, and exact three-point and four-point placement to insert,
overwrite, append, replace, lift, and extract. They preserve clip source and nested-timeline
relationships, inherit fragment intent, require explicit sync-locked ripple adjustments, report
typed fragments and invalidated transitions, reject implicit fit-to-fill retiming, and publish at
one project revision. Nested operations place
existing child timelines, create compound timelines and their parent clips atomically, edit shared
children through stable instance identities, and expose every direct or recursive relationship
without flattening. Deterministic compilation now converts a selected root and every reachable
nested timeline into one typed editable graph revision with stable domain-separated addresses,
explicit transition and nesting edges, complete object parameters including multicam intent, and
bidirectional provenance.
Project persistence, engine, API, CLI, playback, audio-engine, graph evaluation, and rendering do
not consume that compiled state yet.

Native multicam state composes those same timeline and clip owners. One synchronized source
timeline stores ordered `MulticamAngleId` metadata and clip membership, while each ordinary nested
target clip stores an independent source-clock switch partition and explicit follow-video,
fixed-angle, or all-angle audio intent. Resolution follows the target clip time map, active angle
membership, and selected source clip time map without flattening the direct source relationship.
Structural fragments and replacements inherit source membership and target switch intent through
the shared atomic edit path. Graph compilation retains that intent as typed parameters. Runtime
graph evaluation, playback, mixing, the owning project container, engine, and API consumers remain
absent.

Versioned timeline state documents preserve the complete editable owner graph without claiming the
project file boundary. `serialize_timeline_state` emits canonical `superi.timeline` revision 1 JSON
with the stable primitive revision and SHA-256 payload integrity. `deserialize_timeline_state`
strictly rejects corrupt, interrupted, unknown, oversized, or future state, migrates revision 0 in
memory, reconstructs through checked media, timeline, annotation, relationship, retime, nesting,
and multicam APIs, and exposes canonical current bytes only after whole-project validation. Runtime
playback, mixing, the SQLite container, autosave and journal orchestration, engine, and API consumers
remain absent.

The largest verification gap is the absence of a production import-to-export slice. Its canonical
contract, source fixture, reference project state, graph control state, public action flow, and
contract runner now exist. Process contracts run the CLI twice and prove all eight timing and
resident-memory records, the observed-boundary maximum, and deterministic content after normalizing
run-specific measurements and paths. Both hosted Rust build jobs now validate the fixture root and
run the same portable eight-stage contract as a first-class development baseline. Independent
expected data now exists, and native timeline compilation produces generic editable graph state,
and effects provides typed catalog state plus real headless reference pixels. Effects now also
preserves reusable complete preset schemas, typed literals, explicit migrations, and editable
missing-plugin placeholders through strict standalone documents and ordinary graph reload. Atomic
project save, autosave, interruption
selection, and recovery journals remain with the unimplemented project owner. Engine graph
evaluation, GPU effect and transition execution, production timeline-to-transition binding,
rendered comparison, color delivery, and muxing are not
integrated. The new encoder path begins from caller-prepared frames and ends at elementary derived
packets, so it does not close that slice. There is no
current test or runtime that imports through the engine, selects and decodes original media,
edits a timeline, evaluates a graph, applies input and output color, renders through the GPU,
encodes and muxes output, persists a project, and drives the flow through the public API.

## Placeholders and incomplete integration

Entire crate skeletons are `superi-ai` and `superi-project`. Their manifests establish intended
dependency direction, but their public modules expose no substantive types or operations.
`superi-effects` now has substantive graph-native authoring, exact keyframe animation, strict visual
effect presets with complete schemas and typed literals, deterministic integrity-protected preset
documents, explicit schema migration, missing-plugin editing and exact-schema recovery,
composition artifacts with layer parenting, reusable precompositions, collapse boundaries, exact
time remapping and complete resolved paths, strict editable 2D and 3D spatial layers with cameras,
lights, stable depth ordering, exact motion sampling, graph reload, and bounded real pixels, editable cubic vector shapes with fills, strokes,
gradients, repeaters, and path animation, animated cubic mask authoring plus soft-coverage
composition, editable rotoscope spans, corrections, and propagation hooks, styled text authoring
plus editable point, planar, object, and calibrated camera tracking artifacts with manual corrections
and bounded CPU reference solvers, offline OpenType shaping and Unicode paragraph layout, reusable typed control rigs, built-in
visual nodes, and bounded CPU reference execution. It also has reusable cross-dissolve and
directional-wipe schemas, exact handle timing, animatable transition parameters, and bounded
reference pixels, while production spatial GPU execution, production GPU factories, vector, mask, and
text rasterization, glyph atlases, propagation solvers, pyramid and GPU tracking acceleration,
production tracking attachment, native OpenFX bundle discovery,
worker IPC and process supervision, production transition and timeline attachment, and engine
execution remain absent. Its OpenFX 1.5.1 effect-side host now validates isolated adapters, projects
graph-native definitions, samples exact-time values, and owns permissions, lifecycle, recovery, and
quarantine without loading native code.
`superi-audio` now has a substantive
independent processing graph, typed bus routing, sample-accurate scheduler, production device input and output,
clip-mix processor, prepared sample-rate converter, explicit channel conversion, and prepared core
effects, plus a graph-native meter, while automation, hosting, and decoded-sample binding remain
absent. Engine foreground playback feeds the bounded output producer and consumes its actual
presentation clock through explicit video synchronization and recovery outcomes. Transport uses
callback-owned discard acknowledgement and explicitly mutes inactive or unsupported signed-rate
audio. Engine render-export invokes an explicit audio stage, retains its graph identity, and proves
real acquired PCM decode and encode completion, but no adapter yet binds decoded or scheduled
timeline blocks into the actual prepared graph, routing, effects, resampling, and device delivery
path.
`superi-cache` now has substantive composite identity, budgeted memory retention, hierarchical
memory policy, priority-aware LRU eviction, precise edit invalidation, persistent storage, color
metadata, replaceable proxy or optimized-media publication, deterministic inspection and clearing,
safe persistent relocation, layered render reuse, bounded background population, bounded playback
prediction, owned asynchronous host retention, and bounded edit and scrub warming, while automatic
capacity and external-directory policy remain caller-owned. Engine performs substitution using
concurrency-owned selection and consumes predictive plus foreground retained graph evaluation.
Transport replans the bounded playback predictor, but no editor invokes warming and no export owner
invokes the render queue. The render queue still lacks a
production engine node catalog, ROI and
invalidation orchestration, and a rendered frame consumer, and no production editor or playback
owner invokes warming.

Partial modules contain these explicit placeholder areas:

- `superi-api`: scripting and every general command, dispatcher, transport, subscription, and
  transaction path beyond capabilities and the fixed canonical scenario.
- `superi-audio`: binding decoded samples into the real prepared graph, plugin hosting, effect
  automation, and engine composition across schedule, conversion, clip and effects processing,
  graph routing, and device execution. Export currently owns only an explicit stage seam.
- `superi-color`: broader config-persisted rule graphs, ICC transform evaluation, GPU output
  conversion, and production viewport or export integration.
- `superi-concurrency`: GPU submission coordination module and production composition beyond the
  audio domain, derived-media selection, playback workers, clocks, bounded handoffs, and lifecycle
  control consumers.
- `superi-engine`: four placeholder orchestration modules covering errors, nodes, plugins, and
  validation. Playback and transport compose prepared foreground graph, cache, CPU display,
  audio discontinuity, A/V coordination, clock, worker, prediction, and viewport owners, but
  prepared source and decoded-audio binding and native GPU presentation remain absent.
  Render-export composes explicit acquired source, decoder, shared graph, delivery, audio, encoder,
  lifecycle, and recovery owners into complete elementary streams. Native GPU readback, container
  muxing, publication, and application integration remain absent. Lifecycle, resource preparation,
  and clip-mix edit orchestration are implemented separately.
- `superi-effects`: production GPU node implementations, engine registration, playback, viewport,
  export, project persistence, autosave and recovery orchestration, production native plugin binding, UI,
  production spatial transform, camera, light, and motion-blur
  execution, vector shape and mask rasterization, reusable rigging beyond bounded time and scalar parent
  expressions, persistent rig presentation, feather and expansion filtering, propagation solvers,
  production transition binding and GPU parity, text rasterization, glyph atlases, production
  tracking attachment plus pyramid and GPU acceleration, native OpenFX bundle discovery, isolated
  worker IPC, and adapter implementation, beyond its implemented safe OpenFX host contract,
  graph-native authoring, complete reusable presets, strict current
  and legacy preset documents, explicit transactional schema migrations, graph-native missing-plugin
  recovery, exact keyframe curves, versioned built-in effect and transition schemas, exact transition
  timing, reusable control drivers, strict visual
  composition, strict spatial composition, vector shape, and animated mask-stack payloads, editable
  rotoscope artifacts and hooks, editable point, planar, object, and calibrated camera tracking artifacts with bounded CPU
  reference solvers, strict styled text, real shaping and paragraph layout, bounded CPU reference
  including spatial real pixels,
  ROI, diagnostics, strict reload, and immutable real-pixel graph contracts.
- `superi-graph`: invalidation and ROI render orchestration, outer job dispatch, project persistence,
  undo ownership, engine coordination, cache invalidation invocation and resource policy,
  production engine node catalogs, and runtime consumers beyond its implemented identifier,
  neutral typed value, node-schema, typed DAG storage and validation, schema-bound instances,
  immutable snapshots, atomic editable
  transactions, canonical versioned graph documents, typed parameter drivers and expressions,
  caller-projected literal evaluation,
  derived fail-closed plugin availability, exact dirty regions, dependency and semantic edit
  invalidation, snapshot-bound ROI planning, deterministic request-scoped scheduling, node
  introspection, graph and revision cache lineage, retained value adapter and pruning, run-local
  timing, and shared interactive and headless evaluation surfaces.
- `superi-timeline`: broader OTIO schema and vendor-effect interpretation beyond its pinned 0.18.1
  native subset, graph evaluation, fit-to-fill, grouped-source compound synthesis and higher-level
  edit orchestration, undo ownership, multicam playback and mixing, the owning SQLite project
  container, autosave and recovery-journal orchestration, and application consumers beyond its
  native model, authoritative edit state, marker and metadata state, exact snapping, foundational,
  advanced, nested, and multicam edit operations, deterministic graph compilation, versioned state
  documents, shared processing-payload compatibility, OTIO headless consumer, and contract tests.

Substantive modules also have intentionally incomplete boundaries. Media I/O has no muxer or
production registry owner for its source backends. GPU has no cross-adapter transfer or external
decoder import. Color has no ICC transform evaluation, project-configured rule persistence, or
production output-transform consumer. Vendor IPC has no
shared memory, GPU transport, sandbox, or encode. Platform capability and proof depth differ by
host. Repository status documents disagree about package count, implementation maturity, CI, and
legal completion.

A declared dependency, public module name, documented architecture, or passing compile is not proof
that one of these flows exists. Remove a placeholder label only after substantive behavior has a
real consumer and proportionate verification.

## How to navigate the maps

1. Start with this index to identify the owning module and its direct producers and consumers.
2. Read the owning module map from first line through EOF. For a cross-module change, also read each
   affected producer, consumer, and public contract map in full.
3. Use the module's `Source inventory` to locate complete raw-file ownership. An optional map may be
   omitted only when root law permits it and the module manifest, public entry points, cross-module
   interfaces, and relevant implementation and tests are read through EOF and recorded instead.
   Search is only for discovery and never replaces the required complete reads.
4. Follow `Public surface` for supported entry points, `Architecture and data flow` for actual
   execution, `Dependencies and consumers` for direction, `Invariants and operational boundaries`
   for constraints, and `Current status and risks` for incomplete behavior.
5. Treat mapped status as a navigation aid, not current runtime proof. Revalidate facts that may
   have changed and run fresh tests appropriate to the affected boundary.

For common concerns, begin at these owners:

- Shared values, exact time, identifiers, errors, and wire primitives: `superi-core`.
- Files, packets, decoded frames, decoded audio, and backend selection: `superi-media-io`.
- Default, host, and vendor codec behavior: the three `superi-codecs-*` modules.
- CPU still images and reference operations: `superi-image`.
- Color interpretation and transforms: `superi-color`.
- GPU resources, residency, conversion, submission, and recovery: `superi-gpu`.
- Jobs, domains, clocks, handoffs, lifecycle, and liveness: `superi-concurrency`.
- Audio graph topology, input and output device identity, capability discovery, record arming,
  input monitoring, bounded sample handoff, typed platform callbacks, stream lifecycle,
  audio-clock publication, capture and output telemetry, and graph-native
  peak, RMS, true-peak, phase, spectrum, and loudness analysis: `superi-audio`.
- Graph-facing identifiers, node schemas, deterministic DAG state, typed binding validation,
  schema-bound instances, editable transactions, canonical graph documents, typed parameter links
  and expressions, missing-node resolution, exact dirty regions, dependency and semantic edit
  invalidation, snapshot-bound ROI propagation, deterministic request-scoped scheduling and
  evaluation, pre-execution node introspection, graph and revision lineage plus retained-value
  adapter, run-local timing, and shared interactive and headless evaluation:
  `superi-graph`, with value identity, rational time, and pixel bounds owned by `superi-core`.
- Visual definitions, complete reusable effect presets, explicit preset schema migration, editable
  animation, controls, visual composition, tracking, text, transitions, and bounded reference
  evaluation: `superi-effects`. Presets use graph-owned documents and derived missing-node
  availability while project persistence and plugin hosting remain separate owners.
- Complete reusable-result identity, budgeted final-frame and intermediate-node memory retention,
  exact total, project, and device admission, priority-aware LRU eviction, precise revision-safe
  graph edit invalidation, versioned bounded disk persistence with corruption recovery, cache color
  identity, complete derived-media publication, deterministic inspection and clearing, safe
  persistent relocation, layered render reuse, bounded background population, and bounded
  exact-frame edit and scrub warming: `superi-cache`, followed by `superi-engine` for codec
  generation and transparent substitution and `superi-concurrency` for quality choice and
  background job execution.
- Native editorial objects, typed track semantics, exact timing and clip retiming, selection, track
  targeting, sync locks, linked selection, clip grouping, markers, deterministic metadata, exact
  snapping, and foundational insert, overwrite, append, replace, lift, and extract operations plus
  advanced ripple, roll, slip, slide, razor, trim, extend, three-point, and four-point operations,
  plus nested placement, compound creation, shared child editing, recursive nesting inspection,
  multicam angle metadata, synchronization provenance, switching, audio intent, exact resolution,
  and deterministic typed editable graph compilation:
  `superi-timeline`.
- Current assembly and public capability flow: `superi-engine` then `superi-api`.
- Product law, open and closed boundaries, CI, fixtures, and maintenance workflow: `workspace`.
- Canonical first editorial slice, typed scenario state, replacement stages, and proof: `workspace`.
- Reviewed internal runtime dependency direction: `tool-superi-dependency-check`.
- Static network-client and open-to-closed enforcement: `tool-superi-boundary-tool`.
- Deterministic structured platform-lane evidence: `tool-superi-test-report`.

## Map maintenance

Run the mapping commands from the repository root:

```text
python3 .agents/skills/superi-mapping/scripts/codebase_maps.py inventory
python3 .agents/skills/superi-mapping/scripts/codebase_maps.py files <module-id>
python3 .agents/skills/superi-mapping/scripts/codebase_maps.py hash <module-id>
python3 .agents/skills/superi-mapping/scripts/codebase_maps.py changed --base <revision>
python3 .agents/skills/superi-mapping/scripts/codebase_maps.py validate
```

After source changes, use `changed` and the actual diff to find directly affected ownership. Read
every changed raw file and relevant public interface, caller, consumer, test, schema, fixture, and
governing document through EOF. Update source inventory, surfaces, flows, dependencies, invariants,
tests, status, risks, and maintenance guidance before recording the new exact hash and file count.

Update consumer maps even when their own source hash did not change if a relationship or contract
they describe changed. Update this index whenever ownership, dependency layering, public control
flow, major runtime flow, shared invariant, module status, open and closed boundary, or navigation
advice changes.

Do not perform a hash-only refresh. Generated maps are excluded from module source hashes, so
validation cannot detect stale prose by hash alone. Rerun validation after map edits, after final
integration or rebase, and immediately before delivery. Validation must confirm the complete
module inventory, exact frontmatter hashes and file counts, required module headings, every source
inventory path, every index link, and absence of forbidden Unicode dash characters.
