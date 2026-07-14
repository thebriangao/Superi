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
| `superi-api` | [module map](modules/superi-api.md) | `open/crates/superi-api` | Transport-neutral public facade, currently for media capability snapshots | Partial: capability query and change event implemented; general API and scripting absent |
| `superi-audio` | [module map](modules/superi-audio.md) | `open/crates/superi-audio` | Reserved audio graph, playback, mixing, resampling, metering, and plugin boundary | Skeleton: public module names only |
| `superi-cache` | [module map](modules/superi-cache.md) | `open/crates/superi-cache` | Reserved frame, proxy, render, prefetch, eviction, and disk cache boundary | Skeleton: public module names only |
| `superi-cli` | [module map](modules/superi-cli.md) | `open/crates/superi-cli` | Headless executable intended to consume the public API | Scaffold: prints one version line and does not call API or engine |
| `superi-codecs-platform` | [module map](modules/superi-codecs-platform.md) | `open/crates/superi-codecs-platform` | Opt-in host codec adapters for Apple, Windows, and Linux | Implemented, host-dependent: native proof depth varies and legal review remains open |
| `superi-codecs-rs` | [module map](modules/superi-codecs-rs.md) | `open/crates/superi-codecs-rs` | Default permissive software codec implementations | Implemented: AV1, FLAC, MP3, Opus, PCM, Vorbis, VP8, and VP9 decode and encode |
| `superi-codecs-vendor` | [module map](modules/superi-codecs-vendor.md) | `open/crates/superi-codecs-vendor` | Explicit process adapter for separately installed vendor RAW workers | Implemented first revision: decode-only, CPU-only, JSON and hexadecimal IPC |
| `superi-color` | [module map](modules/superi-color.md) | `open/crates/superi-color` | Color math, input and output transforms, working images, LUTs, ICC discovery, and presentation profile guards | Substantial but partial: output transforms are implemented; versioned configuration and ICC evaluation remain absent |
| `superi-concurrency` | [module map](modules/superi-concurrency.md) | `open/crates/superi-concurrency` | Execution domains, jobs, clocks, handoffs, shared snapshots, lifecycle, and liveness | Substantial but not engine-integrated; GPU submission module is a placeholder |
| `superi-core` | [module map](modules/superi-core.md) | `open/crates/superi-core` | Tier-zero values, validation, exact time, identifiers, errors, diagnostics, and stable serialization | Implemented and broadly consumed; crate-level skeleton wording is stale |
| `superi-effects` | [module map](modules/superi-effects.md) | `open/crates/superi-effects` | Reserved effect-node catalog, animation, mask, transition, text, tracking, and OFX boundary | Skeleton: public module names only |
| `superi-engine` | [module map](modules/superi-engine.md) | `open/crates/superi-engine` | Open subsystem assembly and orchestration | Partial: registry assembly, capability introspection, and CPU-frame GPU upload implemented |
| `superi-gpu` | [module map](modules/superi-gpu.md) | `open/crates/superi-gpu` | wgpu device, resource, upload, conversion, pass, submission, presentation, and recovery substrate | Implemented substrate with explicit application-level integration gaps |
| `superi-graph` | [module map](modules/superi-graph.md) | `open/crates/superi-graph` | Reserved node-neutral DAG, mutation, evaluation, ROI, expression, and serialization boundary | Skeleton: no graph type or evaluator |
| `superi-image` | [module map](modules/superi-image.md) | `open/crates/superi-image` | Host image values, still interchange, CPU operations, sequences, previews, and reference validation | Implemented host-side subsystem with explicit representation limits |
| `superi-media-io` | [module map](modules/superi-media-io.md) | `open/crates/superi-media-io` | Codec-neutral source, demux, packet, frame, audio, selection, timing, and operation contracts | Implemented contracts and four demuxers; production source registration and muxing absent |
| `superi-project` | [module map](modules/superi-project.md) | `open/crates/superi-project` | Reserved project document, persistence, autosave, and recovery boundary | Skeleton: no project model or storage format |
| `superi-timeline` | [module map](modules/superi-timeline.md) | `open/crates/superi-timeline` | Reserved editorial model, edits, OTIO, nesting, multicam, and graph compilation | Skeleton: no timeline model, OTIO path, or compiler |
| `tool-superi-dependency-check` | [module map](modules/tool-superi-dependency-check.md) | `open/tools/superi-dependency-check` | Offline executable policy for the open runtime dependency graph | Implemented exact runtime, build, dev, and new-crate checks |
| `tool-superi-boundary-tool` | [module map](modules/tool-superi-boundary-tool.md) | `open/tools/superi-boundary-tool` | Offline scanner for network-client and open-to-closed policy | Implemented library, CLI, workspace gate, and hosted-build command |
| `tool-superi-fixture-tool` | [module map](modules/tool-superi-fixture-tool.md) | `open/tools/superi-fixture-tool` | Offline validator for canonical repository fixtures | Implemented validation library and CLI; it does not generate fixtures |
| `workspace` | [module map](modules/workspace.md) | Repository files outside `open/crates/*` and `open/tools/*` | Product law, architecture, policy, workspace configuration, fixtures, and agent workflows | Active policy and build control layer with known cross-document status drift |

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

The open runtime and tool workspace lives under `open/`. Current Cargo membership is 19 runtime
crates plus `superi-fixture-tool`, `superi-dependency-check`, and `superi-boundary-tool`. All three
tools are built with the workspace but remain outside the runtime dependency graph. The root
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
  -> image, concurrency, graph, cache, color, effects, timeline,
     audio, ai, and project        mostly manifest-only today

superi-project -> superi-timeline -> superi-graph
superi-color, superi-effects, superi-cache, superi-ai -> lower graph/image/GPU/core layers
superi-audio -> superi-concurrency -> superi-core
superi-graph -> superi-gpu, superi-image, superi-concurrency, superi-core

superi-codecs-rs, superi-codecs-platform, superi-codecs-vendor
  -> superi-media-io
  -> superi-core

superi-media-io -> superi-image -> superi-core
superi-gpu -> superi-core
```

`superi-core` is the tier-zero semantic contract and has no Superi dependency. Higher modules must
not copy its identifier, time, geometry, color-tag, pixel, audio-layout, error, diagnostic, or
stable serialization meanings into competing local types without an explicit boundary conversion.

The generic graph direction is deliberately one way. Graph may depend on representation and
execution substrates, while color, effects, cache, timeline, AI, project, and engine may depend on
graph. Graph must not depend upward on a domain catalog. This relationship currently exists mainly
in manifests because graph and most of its consumers remain skeletons.

Codec implementations depend down on the codec-neutral `superi-media-io` interface. Media I/O does
not depend on a concrete codec, engine, or registry assembler. The engine owns the current assembly
choice. The API depends on engine-owned projections rather than leaking media-I/O implementation
types. The CLI is intended to depend only on the API, although its current source uses neither API
nor engine.

## Public control flow

### Implemented today

The only implemented public engine-to-API slice is media capability introspection:

1. `superi-engine::media` creates a `BackendRegistry` and registers the default Rust codecs.
2. The `os-codecs` feature may append host-discovered platform codecs.
3. The separate vendor constructor may append only explicitly configured vendor workers.
4. `superi-engine::introspection::MediaCapabilities::from_registry` reads declarations without
   opening sources or constructing codecs, then produces deterministic engine-owned records.
5. `superi-api::MediaCapabilitiesApi` projects those records into strict serializable API types.
6. `GetMediaCapabilities` clones the current full snapshot. `synchronize` emits one full-replacement
   `MediaCapabilitiesChanged` event only when semantic capability state changes.

The API-local revision begins at zero and increments only on a changed snapshot. The public schema
version is `2.0.0`; the permanent method and event names are
`superi.media.capabilities.get` and `superi.media.capabilities.changed`.

No transport, request envelope, dispatcher, event channel, subscription, public transaction,
editor command, script runtime, or structured public error wire is implemented. The CLI does not
construct `MediaCapabilitiesApi`; it prints a fixed scaffold line. There is no UI, shell, extension,
automation, or closed-tier runtime consumer in this repository.

### Documented target, incomplete

Repository contracts describe one stable public command and event seam shared by UI, CLI, scripts,
extensions, automation, and closed-tier clients. Engine transactions are intended to coordinate
project, timeline, graph, caches, undo, persistence, lifecycle, playback, render, export, and event
publication. Bulk frames, audio, packets, and GPU resources are intended to stay behind that seam.

That target must not be read as current behavior. Project, timeline, graph, cache, audio, effects,
and most engine orchestration are still placeholders, so no complete edit, playback, render, save,
or export control flow exists.

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

### Integration gaps

The engine's default registry currently registers codec backends but not the four implemented
container source backends. No production source owner therefore reaches the implemented demuxers
through ordinary engine construction. Container-to-codec composition exists in codec integration
tests, not in an engine playback or import path.

There is no muxer, export container writer, production image-sequence backend, multiple-stem stream
selector, or source-to-playback coordinator. Platform and vendor frames can be external or
backend-owned, but the engine upload path currently accepts CPU frames only. Higher-level decode
selection is expected to request a CPU fallback, yet that selection flow is not implemented.

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
later storage conversion.

`superi-gpu` owns device identities, resources, memory budgets, pooled textures, decoded upload,
storage conversion, shaders, passes, the exclusive submission queue, fences, readback, native
surfaces, diagnostics, and device-loss reconstruction. Its storage converter may change packing,
numeric representation, matrix, range, subsampling, and alpha association. It refuses to change
primaries or transfer functions because those are color-management operations.

The implemented decoded-frame bridge is:

```text
superi-media-io VideoFrame with CPU storage
  -> superi-engine VideoFrameUploader
  -> superi-gpu DecodedFrameUploader
  -> pooled GPU plane textures
  -> UploadedVideoFrame retaining format, timing, metadata, and GPU ownership
```

The uploader preserves decoded bits, plane order, timestamps, duration, format, and metadata. It
uses direct row writes when compatible and a tight CPU repack otherwise. Logical initialized
texture extent remains distinct from aligned physical allocation extent. Pooled allocations and
all command dependencies must remain retained until the matching fence retires.

No implemented engine path sends `UploadedVideoFrame` into graph evaluation, color processing,
cache, playback, display, or encode. The graph is a skeleton. Color input, output, LUT, and rule
transforms are CPU implementations and have no graph-visible node catalog. Output transforms do
not evaluate validated ICC profile state or provide a GPU viewport or
export consumer. `MonitorAwareViewport` prevents stale-profile presentation but does not
color-convert a frame.

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
  wait, present, drop, or rebase instructions but never performs them.
- Lifecycle coordination uses revisioned requests and exact participant acknowledgements.
  Liveness probes and explicit wait-resource ownership produce starvation and deadlock findings.

Media and codec operations use `superi-media-io::OperationContext`, which carries priority,
cancellation, and an optional monotonic deadline. The vendor adapter keeps that context active
while waiting on process I/O. Platform and Rust codecs check it at public boundaries and selected
loops. Concurrency jobs use their own `JobControl` and require the job closure to call checkpoints.
Both models are cooperative. Neither can preempt a blocking operating-system call, native codec
call, or closure that omits checkpoints.

These mechanisms are not yet a composed runtime. Graph, audio, and engine declare concurrency
dependencies but do not construct worker pools, clocks, handoffs, lifecycle participants, or
liveness monitors in production source. The `submit` module is a placeholder. A contract test hosts
the real non-Send `GpuSubmissionQueue` inside the GPU submission domain, but no engine owner wires
that pattern into playback or render.

## Engine, API, CLI, and tool roles

`superi-engine` is the intended integration owner. Its implemented responsibilities are limited to
codec registry assembly, deterministic capability introspection, and CPU-decoded frame upload.
Lifecycle, commands, transactions, undo, playback, A/V sync, render, export queues, resources,
plugins, nodes, validation, and cross-subsystem error recovery remain explicit placeholders.

`superi-api` is the stable public facade. Its capability slice keeps media implementation types
private and exposes strict versioned records, a typed query, and a full-replacement event. It has no
transport or broad editor command set.

`superi-cli` is a binary boundary, not a library. It forwards codec feature flags at build time but
does not initialize the engine or API and accepts no arguments.

`superi-fixture-tool` is a repository utility, not an engine component. It validates schema,
identity, provenance, lineage, payload ownership, byte counts, hashes, path safety, and unmanaged
files under `open/test-fixtures`. It is offline and read-only. It does not generate fixtures,
execute recorded generator commands, prove repository-history immutability, or verify legal and
semantic claims inside free-form provenance fields.

`superi-dependency-check` is also a repository utility. It reads the locked workspace graph offline
and fails when a runtime crate adds an unreviewed normal, build, or dev-only internal edge, or when a
new runtime crate has no explicit policy. The structure guide and executable policy are reviewed as
one architecture contract.

`superi-boundary-tool` is a dependency-free repository utility, not an engine component. It scans
Cargo and Rust source deterministically, rejects forbidden network clients and direct socket APIs,
rejects supported open-to-closed import routes and symlinks, and runs before each locked hosted
workspace build as well as through the canonical workspace test gate.

## Shared invariants

The following constraints cross multiple modules and should be preserved together:

- Open and closed dependency direction is one way. Open behavior cannot require closed code,
  accounts, remote services, or a network.
- Shared identifiers, exact timebases, half-open ranges, stable codes, color and pixel tags, channel
  order, error categories, recoverability, and primitive serialization are owned by `superi-core`.
- Project identity is separate from replaceable media location. Content fingerprints protect
  relinking, while metadata and source timing remain attached to the artifact that produced them.
- Deterministic ordering is explicit. Stable backend IDs break selection ties; ordered maps and
  sets stabilize public snapshots, fixtures, diagnostics, and validator output.
- Capability declarations are metadata, not proof that a factory or every declared format can run.
  Introspection must not instantiate codecs or sources.
- Backend fallback is explicit. The registry, platform adapters, and vendor workers do not silently
  switch implementation after the chosen operation fails.
- Pixel storage, alpha association, color interpretation, dimensions, timing, and buffer ownership
  are separate contracts. Constructing valid metadata does not prove a codec, color transform, GPU
  operation, or output supports it.
- Color tags do not execute transforms. Input, working, display, and delivery transforms require
  explicit owners. GPU storage conversion must not silently change primaries or transfer.
- GPU device identity and generation scope every managed object. Old, foreign, or recovered-device
  resources cannot be mixed. Submission retention must outlive fence retirement.
- Bounded allocation, queue capacity, pressure, cancellation, and backpressure are explicit at each
  implemented boundary. A local bound must not be generalized into a global process-memory claim.
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
application tests. `superi-core`, `superi-image`, `superi-media-io`, `superi-codecs-rs`,
`superi-codecs-vendor`, `superi-color`, `superi-concurrency`, `superi-gpu`, and the implemented
engine and API slices all have focused contracts around their public values and lifecycles.
Container-to-codec tests and engine capability tests provide selected cross-crate composition.

Test source documents an intended or previously exercised contract, but its presence is not a fresh
passing result. Individual module maps state whether synthesis ran a suite. At mapping time, the
core and fixture-tool maps record fresh successful runs; several other maps explicitly state that
their synthesis did not execute the tests.

Native and GPU proof is environment-dependent. Many GPU tests return successfully without running
device work when no adapter is available. Timestamp paths may skip when features are absent.
Platform codec proof must run on macOS, Windows, or Linux with the actual framework, transform, or
driver. A host-independent parser test does not prove native lifecycle, pixel fidelity, teardown,
or hardware behavior.

Repository-level CI now has three implemented workflow surfaces. The dependency-policy workflow
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
Linux lanes first install `libva-dev`, which supplies the `libva.pc` metadata required to compile
the locked media dependency graph on GitHub-hosted Ubuntu images. Linux and macOS lanes build the
approved libvpx 1.16.0 source after verifying its pinned archive checksum, then expose the exact
shared runtime to capability and codec tests.
Hosted macOS skips only three named VideoToolbox or AudioConverter lifecycle tests whose physical
codec evidence belongs to the documented hardware lane; Linux and Windows run the exact full
workspace test command. Matrix fail-fast is disabled, superseded branch runs are cancelled, and
each build has a 90-minute timeout. Ubuntu 26.04 remains experimental, so its failure does not fail
the workflow.

The durable CI checkpoint record also reports focused workflow-contract verification, one local
locked workspace build with stable Rust 1.97.0, and successful offline fixture-tool policy tests.
The boundary scan is now a recurring workflow step; the other local verification remains delivery
evidence rather than hosted workflow behavior.

The frontend workflow runs on pull requests, pushes to `main`, and manual dispatch using a read-only
Ubuntu 24.04 job. It installs exact Node.js 24.13.0, uses `npm ci` against the committed lockfile,
runs strict no-emit TypeScript 5.9.3 checking, creates a Vite 7.3.6 production bundle, and verifies
the workflow contract plus generated hashed JavaScript entry. Its `ci/frontend-smoke/` consumer is
an isolated toolchain contract, not the deferred React application or Tauri desktop shell.

The dedicated network-isolated workflow prepares locked Cargo artifacts, libva headers, and a
checksum-pinned approved libvpx 1.16 runtime on Ubuntu 24.04 while online, then runs workspace tests,
canonical fixture validation, and the headless CLI inside a distinct Linux network namespace with
only loopback, no IPv4 route, a failed numeric outbound probe, and Cargo offline mode. Its pinned
checkout, read-only permissions, disabled credentials, timeout, and cancellation policy match the
repository CI boundary. This is an offline execution proof for current core commands after setup,
not an offline dependency-acquisition proof or a replacement for the absent editorial slice.

The cross-platform Rust workflow does not run the `os-codecs` feature matrix, malformed-input
suites, frontend or Tauri checks, golden comparisons, benchmarks, soak, or the vertical slice. The
separate frontend workflow does not prove React, Tauri, the native viewport, API integration,
editorial behavior, or product UI. Neither is an MSRV lane,
and neither is an end-to-end offline build proof because hosted setup and installation may use the network. Hosted
virtual machines also do not satisfy the
physical GPU, display, audio-device, hardware-codec, performance, and long-session lanes in
`docs/platform-testing.md`. Real platform codec lifecycle, pixel or sample fidelity, driver
behavior, device loss, presentation, and teardown still require their owning operating systems and
physical hardware.

Shared fixtures are versioned, immutable by repository policy, and validated offline with
`superi-fixture-tool`. The validator proves the current filesystem snapshot and manifest digests,
not Git-history immutability, provenance truth, legal clearance, or semantic quality.

The largest verification gap is the absence of a production import-to-export slice. There is no
current test or runtime that imports through the engine, selects and decodes media, edits a
timeline, evaluates a graph, applies input and output color, coordinates audio and playback,
renders through the GPU, encodes and muxes output, persists a project, and drives the flow through
the public API.

## Placeholders and incomplete integration

Entire crate skeletons are `superi-ai`, `superi-audio`, `superi-cache`, `superi-effects`,
`superi-graph`, `superi-project`, and `superi-timeline`. Their manifests establish intended
dependency direction, but their public modules expose no substantive types or operations.

Partial modules contain these explicit placeholder areas:

- `superi-api`: scripting and every general command, dispatcher, transport, subscription, and
  transaction path beyond media capabilities.
- `superi-cli`: private command module and all API or engine behavior.
- `superi-color`: versioned configuration, ICC transform evaluation, tone mapping, GPU output
  conversion, and production viewport or export integration.
- `superi-concurrency`: GPU submission coordination module and all production engine composition.
- `superi-engine`: eleven orchestration modules covering A/V sync, commands, errors, export,
  lifecycle, nodes, playback, plugins, render, resources, and validation.

Substantive modules also have intentionally incomplete boundaries. Media I/O has no muxer or
production registry owner for its source backends. GPU has no cross-adapter transfer or external
decoder import. Color has no ICC transform evaluation, project-configured rule persistence, tone mapper, or
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
3. Use the module's `Source inventory` to locate complete raw-file ownership. Use search only for
   discovery; it does not replace reading selected source, manifests, tests, schemas, fixtures, and
   governing documents through EOF.
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
- Current assembly and public capability flow: `superi-engine` then `superi-api`.
- Product law, open and closed boundaries, CI, fixtures, and maintenance workflow: `workspace`.
- Reviewed internal runtime dependency direction: `tool-superi-dependency-check`.
- Static network-client and open-to-closed enforcement: `tool-superi-boundary-tool`.

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
