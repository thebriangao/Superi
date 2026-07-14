# Superi Phase 0 Build Contracts

**Status:** Locked architectural specification
**Approved by:** Founder
**Approval date:** 2026-07-12
**Scope:** Open-source Superi only

This document records the irreversible technical and product contracts that govern Superi's build.
It is the canonical reference for the Phase 0 architecture decisions and must be read before any
subsystem implementation plan is approved.

The repository `AGENTS.md` files remain the highest-authority operational rules. If an
implementation, dependency, prototype, or later proposal conflicts with those rules or with a
contract below, work must stop until the conflict is resolved explicitly.

## Completion boundary

The architectural contracts in this document are approved. That approval does not by itself
complete every Phase 0 test or external-review requirement.

Phase 0 may be marked fully complete only after all of the following evidence exists:

- A written codec and patent review from qualified intellectual-property counsel.
- A Tauri window containing React and a resizable native wgpu viewport has been demonstrated on
  macOS, Windows, and Linux.
- A public API command and ordered event round trip has been demonstrated without transferring
  frame data through webview IPC.
- A representative OTIO sequence has completed a reference-validated semantic round trip.
- The working color model has passed reference-transform validation.
- Every subsystem has a named human owner.

## 1. Open product boundary

### Contract

Superi must remain completely functional when the machine has no network connection, including
editing, rendering, exporting, project persistence, automation, and every bundled local AI
feature. The open tree contains no accounts, authentication, credits, licensing checks,
telemetry, update checks, remote inference, server fallback, or dependency on Superi Max.

Core Superi code never initiates outbound network traffic. A user-installed plugin may receive
explicit network permission only as an optional capability, and that permission can never affect
core functionality or the completeness of the offline editor. CI enforces the boundary through
network-isolated workflows, dependency-direction checks, and scans for network clients and
closed-tree imports.

### Rationale

Enforcing offline operation structurally prevents commercial pressure, convenience, or accidental
dependencies from gradually weakening the central promise of Superi.

## 2. Rust engine and wgpu graphics

### Contract

Rust is the exclusive implementation language for the headless engine, using stable Rust and an
explicit minimum supported Rust version. wgpu is the exclusive GPU abstraction, and WGSL is the
canonical shader language. Production features may not introduce separate Metal, Vulkan, or D3D12
implementations.

Platform-specific APIs are permitted only at narrow surface-creation, hardware-codec,
external-memory, audio, and plugin FFI boundaries. Every unsafe block is isolated, documented, and
justified. The architecture relies on the native backends supported by
[wgpu](https://docs.rs/wgpu/latest/wgpu/) rather than implementing independent renderers.

### Rationale

One memory-safe engine and one graphics abstraction minimize duplicated implementation work while
preserving access to every major desktop GPU platform.

## 3. Tauri, React, and TypeScript boundary

### Contract

Tauri 2 is the desktop host, React is the interface framework, and TypeScript is the application
language. React owns presentation, panel layout, transient interaction state, accessibility,
keyboard routing, and optimistic visual feedback. It never owns authoritative project state or
editing semantics.

Every operation that changes a project, timeline, graph, render, export, or media resource crosses
the generated client for the public Rust API. Tauri owns process startup, native windows,
permissions, lifecycle integration, and the thin bridge into that API, using its supported Rust
command and channel mechanisms described in the
[Tauri documentation](https://v2.tauri.app/develop/calling-rust/).

### Rationale

This boundary provides the productivity of the web ecosystem without trapping critical editing
behavior inside a particular interface implementation.

## 4. Shell-to-engine transport

### Contract

`superi-api` owns a typed, transport-neutral protocol based on JSON-RPC 2.0 semantics. The protocol
defines namespaced methods, string request identifiers, structured errors, cancellation,
transaction identifiers, capability negotiation, and explicit protocol versions.

The Tauri shell exposes one thin command dispatcher and one ordered channel for event envelopes.
External local clients use the same protocol over Unix domain sockets on macOS and Linux and named
pipes on Windows. Events carry sequence numbers, project revision identifiers, and subscription
identifiers so clients can detect gaps, reject stale state, and resynchronize deterministically.

Frames, audio buffers, GPU textures, and other bulk media never enter the command or event
protocol. JSON-RPC provides the transport-neutral request model, and Tauri channels provide ordered
delivery for the shell bridge. See the [JSON-RPC 2.0 specification](https://www.jsonrpc.org/specification)
and [Tauri channel guidance](https://v2.tauri.app/develop/calling-rust/#channels).

### Rationale

A stable protocol shared by the UI, CLI, tests, scripts, plugins, and Superi Max prevents any client
from gaining privileged access or creating a second automation architecture.

## 5. Native wgpu viewport

### Contract

Each editor window contains a dedicated native child view managed through a platform-specific
`ViewportHost` interface. wgpu creates its presentation surface from that child view's native
handle. React communicates only viewport geometry, scale factor, visibility, display selection,
and normalized input events.

Decoded frames remain GPU textures and are presented directly through the native surface. The
unsafe handle conversion is isolated to one small implementation per platform, and the native
view must outlive its wgpu surface, as required by the
[wgpu surface API](https://docs.rs/wgpu/latest/wgpu/enum.SurfaceTargetUnsafe.html). Tauri exposes the
required [native window handles](https://docs.rs/tauri/latest/tauri/webview/struct.WebviewWindow.html),
but the webview never receives serialized frame pixels.

### Rationale

Native presentation preserves GPU residency, HDR potential, low-latency output, and 8K scalability
without binding the renderer to webview performance or IPC bandwidth.

### Display profile handoff

The shell connection that owns a native viewport also owns monitor selection and display-profile
events. It publishes one complete `NativeDisplayProfileProvider` snapshot to `superi-color`, using
`macos-cgdisplay:<CGDirectDisplayID>` on macOS, the Win32 display device name on Windows,
`linux-x11-crtc:<RandR CRTC ID>` on X11, or a connection-local Wayland output ID. Exact ICC bytes
are included when the platform exports them; their absence is an explicit unprofiled state.

`MonitorAwareViewport` owns the resulting binding beside the real `NativeViewportSurface`. The
shell supplies the current monitor identity at frame acquisition and presentation, and the color
owner checks both monitor identity and immutable profile generation at both boundaries. Window
moves and profile notifications therefore require an explicit reversible rebind, while a change
during an acquired frame rejects presentation instead of using a stale transform.

## 6. OpenTimelineIO interchange

### Contract

Superi uses a Rust-native timeline model and a native Rust OTIO JSON reader and writer, with no
runtime dependency on the C++ or Python OTIO implementations. The importer converts supported OTIO
objects into Superi types while retaining unknown schemas, unknown fields, metadata, rational time
values, and extension payloads as opaque versioned data.

The exporter targets an explicit repository-pinned `OTIO_CORE` schema-version set, supports
controlled upgrade and downgrade transforms, and never silently substitutes or discards
unsupported semantics. Compatibility tests compare Superi output against the official OTIO
reference implementation. OTIO is treated strictly as editorial interchange, not as the Superi
project format or a media container. See the [OTIO overview](https://opentimelineio.readthedocs.io/en/latest/)
and [OTIO schema-versioning model](https://opentimelineio.readthedocs.io/en/v0.16.0/tutorials/versioning-schemas.html).

### Rationale

Native Rust keeps the engine coherent, while faithful schema compatibility and opaque-field
preservation prevent Superi from becoming an isolated editorial ecosystem.

## 7. Codec legal review

### Contract

No encumbered codec implementation may begin until qualified intellectual-property counsel
delivers a written matrix covering patents, copyright licenses, distribution, jurisdictions,
commercial use, operating-system APIs, vendor SDKs, and user-installed modules. Until that review
is complete, the technical policy remains pure-Rust implementations for approved royalty-free
formats, operating-system codec adapters behind the optional `os-codecs` feature, and vendor RAW
support through separately installed plugins.

FFmpeg and libav remain outside the MIT core. This is required both because of the repository's
zero-copyleft policy and because software licensing does not settle codec patent exposure. The
review must consider current patent programs and their jurisdictional coverage rather than assuming
that operating-system availability or widespread use provides permission. See
[FFmpeg licensing](https://ffmpeg.org/doxygen/7.0/md_LICENSE.html) and
[Via LA AVC/H.264 licensing](https://www.via-la.com/licensing-programs/avc-h-264/).

### Rationale

A written legal matrix prevents an early implementation shortcut from invalidating the MIT promise
or creating years of architectural and commercial liability.

## 8. Dependency and bundled-model licensing

### Contract

Runtime and build dependencies require an explicit permissive allowlist, automated license
scanning, provenance records, locked versions, and human review. The initial software allowlist is
MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode-3.0, Unicode-DFS-2016, and
individually approved public-domain components. Copyleft, source-available, noncommercial,
research-only, field-of-use, and redistribution-restricted terms are rejected.

A bundled model must include redistributable weights, inference code, training code when
applicable, sufficient training-data provenance, commercial-use rights, modification rights, and
unrestricted local use. The model, tokenizer, vocabulary, configuration, training artifacts, and
notices are audited separately. Downloadable weights alone do not satisfy the policy, which follows
the freedoms and preferred-form expectations in the
[Open Source AI Definition](https://opensource.org/ai/open-source-ai-definition).

### Rationale

Superi cannot be genuinely forkable if its software is permissive but its bundled models contain
restrictions, undisclosed provenance, or irreplaceable binary artifacts.

## 9. Project file and migrations

### Contract

A `.superi` project is a single SQLite database identified by a Superi application ID. It uses
normalized tables for project metadata, media references, timelines, graphs, artifacts, settings,
and extension records. Media, proxies, thumbnails, render caches, models, and generated previews
remain external by default.

Saving writes a complete temporary database, validates checksums and invariants, synchronizes it to
durable storage, and atomically replaces the prior file. Autosaves are separate recovery snapshots,
so an interrupted save never damages the last valid project. SQLite provides a documented
cross-platform format, application identifiers, transaction recovery, and schema version fields
suitable for an [application file format](https://www.sqlite.org/fileformat2.html).

Schema compatibility uses a monotonic integer revision plus a semantic project-format version.
Every released older schema migrates forward transactionally and losslessly, and the original file
is preserved until migration succeeds. Unsupported newer major versions open read-only when safe or
fail clearly without mutation, and Superi never silently downgrades a project, removes unknown
extension data, or overwrites a file that failed validation.

### Rationale

A transactional single-file format offers better corruption resistance, migration control, and
long-session persistence than compressed JSON archives or loosely coordinated project directories.

## 10. Node, artifact, and timeline-compilation contracts

### Contract

Every node type has a stable namespaced identifier, schema version, typed input and output ports,
parameter schema, time behavior, region-of-interest behavior, color requirements, determinism
declaration, cache policy, and capability requirements. A node instance contains only ordinary
serializable state, including parameters, expressions, keyframes, connections, and stable object
identifiers. Evaluation receives an immutable context and cannot mutate the project.

Every AI or automated result is represented as normal clips, masks, nodes, keyframes, markers,
captions, edits, or metadata created through an undoable engine transaction. Provenance is
supplementary metadata rather than a hidden execution dependency. The timeline compiler is a pure,
deterministic, incremental transformation from a validated timeline snapshot and evaluation context
into a DAG with stable derived identifiers, allowing unchanged subgraphs and caches to survive
edits.

### Rationale

These contracts make manual editing, automation, headless rendering, caching, undo, local AI, and
Superi Max operate on one inspectable source of truth.

## 11. Color pipeline and configuration

### Contract

Canonical image storage is premultiplied RGBA 16-bit float in a tagged scene-linear working space,
with computations promoted to 32-bit float whenever numerical stability requires it. The built-in
default working space is ACEScg. Projects may select another explicitly declared scene-linear
wide-gamut space through an immutable, versioned, content-hashed `ColorConfig`.

Every input receives an explicit input transform, and every viewer and export receives an explicit
display or output transform. Grading nodes may operate in log or perceptual spaces only through
visible graph conversions. The configuration model recreates the relevant OCIO 2 concepts,
including roles, named spaces, aliases, file rules, looks, displays, views, context variables, and
transform graphs, and unsupported operators are rejected rather than approximated. See the
[OCIO configuration for ACES](https://opencolorio.readthedocs.io/en/v2.5.2/configurations/aces_cg.html).

Reference transforms, LUTs, and configuration versions are pinned inside each project so an update
cannot silently change its appearance. Negative values, values above one, HDR luminance, alpha
association, chromatic adaptation, and legal-range handling remain explicit throughout the graph.

### Rationale

A scene-linear ACEScg default provides a durable professional baseline, while explicit
configuration and transform pinning prevent hidden color assumptions from corrupting interchange or
archived projects.

## 12. Thread ownership

### Contract

The operating-system main thread owns Tauri, the webview, window lifecycle, native viewport
geometry, and input dispatch. It never performs engine work or waits synchronously for rendering. A
single engine-control thread owns authoritative mutable project state and serializes commands into
transactions, while render coordinators consume immutable snapshots and schedule bounded work onto
a CPU job pool.

A dedicated playback scheduler owns the master playback clock. The audio callback owns only
lock-free real-time audio buffers. Background I/O workers own blocking filesystem and codec work,
and one GPU-submission thread owns queue submission, presentation order, resource retirement, and
device-loss recovery.

Communication uses bounded channels, immutable snapshots, generation identifiers, cancellation
tokens, and documented backpressure. Blocking locks and memory allocation are prohibited in the
audio callback. Offline rendering uses the same graph evaluator and GPU-submission path as playback
under a different scheduler and quality policy.

### Rationale

Explicit ownership removes entire classes of deadlocks, priority inversions, race conditions, A/V
drift, and nondeterministic rendering before implementation complexity makes them difficult to
eliminate.

## 13. Plugin trust, permissions, and containment

### Contract

Superi supports three execution classes: audited built-in code compiled into the application,
capability-sandboxed WebAssembly components, and third-party native plugins running in supervised
worker processes. No third-party native code runs in the main editor process in the supported
configuration, regardless of signature. A signature establishes identity rather than safety.

WebAssembly plugins declare typed WIT imports and exports and receive only explicitly granted host
capabilities. Native workers use versioned IPC, shared memory, bounded audio rings, and platform GPU
sharing handles where required. This aligns with the
[WebAssembly Component Model](https://component-model.bytecodealliance.org/design/components.html),
where components interact through declared interfaces rather than shared component memory.

Capabilities include specific filesystem roots, project read or mutation access, media decode,
export destinations, GPU access, microphone access, network access, process spawning, UI panels,
and persistent storage. Sensitive capabilities are denied by default. Native workers are sandboxed
with platform facilities, scanned before activation, watched through heartbeats and deadlines, and
automatically bypassed or restarted after failure without corrupting project state.

Plugin state is serializable and checkpointed. Crashes become structured diagnostics, and repeated
failures quarantine the plugin for that project.

### Rationale

Treating plugins as potentially hostile and failure-prone preserves professional stability without
closing Superi to third-party effects, codecs, audio processors, and automation.

## 14. Measurable completion targets

### Measurement contract

Every measurement names a versioned reference project, media set, application build, operating
system, GPU backend, driver, CPU, memory capacity, cache state, and hardware tier. Results without
that context are not accepted as evidence.

### Playback and interaction

- Non-rendering edit commands acknowledge within 16 ms at p95.
- Warm playback begins within 150 ms.
- Cold playback begins within 750 ms.
- Cached scrubbing displays the requested frame within 100 ms at p95.
- Reference playback sustains 1080p60 on baseline hardware with no dropped frames after a two-second
  warmup.
- Reference playback sustains 4K60 on recommended hardware with no dropped frames after a two-second
  warmup.
- Reference playback sustains 8K30 through the documented high-end or proxy profile with no dropped
  frames after a two-second warmup.

### Render correctness

- Headless and interactive rendering produce identical command graphs.
- Same-build, same-backend renders are deterministic.
- Cross-backend image comparisons use documented per-node tolerances.
- The general normalized absolute-error ceiling is `0.001` where a stricter node-specific tolerance
  is not defined.
- Reference color transforms remain below Delta E 2000 of `0.5`.

### Audio and video synchronization

- Playback A/V offset remains below 10 ms.
- Accumulated playback drift remains below 1 ms per hour.
- Export timestamps are sample-accurate.
- The reference workload completes without audio underruns.

### Memory and stability

- Configured memory caches exceed their limit by no more than 5 percent.
- An eight-hour warmed session grows by no more than 100 MB or 1 percent of working-set memory,
  whichever is larger.
- Superi completes an eight-hour interactive soak without crashes, deadlocks, lost edits,
  device-loss corruption, or failed recovery.
- Superi completes a twenty-four-hour headless render under the same failure constraints.

### Local AI quality

- Transcription word error rate is at or below 10 percent on the licensed clean-speech benchmark.
- Transcription word error rate is at or below 20 percent on the licensed noisy-speech benchmark.
- Diarization error rate is at or below 15 percent.
- Detection tasks achieve at least `0.90` F1.
- Segmentation and masking achieve at least `0.85` mean intersection over union.
- Content search achieves at least `0.80` recall at 10.
- Audio denoising improves SI-SDR by at least 6 dB on the reference corpus.
- Transcript-driven edit boundaries align within one video frame for at least 99 percent of
  reference edits.
- No accepted model update regresses a primary metric by more than 2 percent without explicit human
  approval.

### Rationale

Fixed measurements tied to reproducible workloads turn fast, accurate, stable, synchronized, and
high-quality into engineering gates rather than subjective aspirations.

## Change control

These contracts may change only through an explicit architecture decision that records the proposed
change, affected systems, migration cost, rejected alternatives, evidence, and founder approval.
Implementation convenience, dependency availability, or schedule pressure is not sufficient reason
to violate a contract silently.
