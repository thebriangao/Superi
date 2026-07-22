---
module_id: workspace
source_paths:
  - repository files outside open/crates/* and open/tools/*
source_hash: 5b8e779b8ec7e6d2df58b88383d7028de9dcaa5d9e95c5a35ab78e3e00f6943f
source_files: 341
mapped_at_commit: working-tree
---

## Purpose and ownership

The `workspace` module owns the repository-level product definition, architectural contracts,
license and codec policy, build sequencing, operating-system test policy, unsafe-boundary audit,
the production React and Tauri desktop shell including persistent multi-window session ownership,
customizable registry-backed layout reset and recovery, always-visible engine lifecycle state,
bounded crash continuity, configurable cross-session keyboard shortcuts, retained GPU, audio,
codec, and AI capability visibility, explicit process-lifetime execution ownership, and a
searchable typed command and action catalog, fixed color-critical dark theme, semantic application
feedback chrome, and separate native viewer and authored marker color-data meaning,
Cargo workspace configuration, dependency lock,
shared test-fixture contract, and repository-owned agent workflows. Runtime implementation under
`open/crates/*` and repository utilities under
`open/tools/*` belong to their own module maps. This map therefore explains the constraints and
coordination layer around those modules rather than duplicating their internal APIs.

The root `AGENTS.md` is the highest-authority operational law for work in this checkout. It routes
checkpoint assignments, fixes the paired-tab Google Docs claim, blocked-note, highlight, and
three-sentence completion protocol, requires safe synchronization with `origin/main`, and makes
current mandatory maps plus complete selected raw-file reads a prerequisite for implementation.
Optional maps may be replaced only by the recorded deeper raw-code reading defined there. It routes
a single checkpoint to a tier 0 or tier 1 owner that performs live Google Docs inspection, map and
current implementation reading, planning, implementation, testing, mapping, review, and delivery
inline without any subagent. External research is optional and begins only when repository evidence
cannot resolve a material question. Multiple checkpoints still use separate
Codex-managed worktree tasks. Multi-checkpoint dispatch defaults to three active workers but obeys an
explicit positive user concurrency value. The file is ignored by Git and copied into managed
worktrees through `.worktreeinclude`, so the mapping script does not include it in this module's
current file inventory or source hash. It must still be reread independently before repository work.

The workspace is both policy and live build configuration. The documents define the intended and
ratified architecture, while `open/Cargo.toml` and `open/Cargo.lock` expose the dependency graph
that Cargo actually resolves. When those disagree, current manifests, crate source, tests, and
fresh tool output are implementation evidence; aspirational or stale prose is not.

## Source inventory

### Repository workflows and mapping

- `.agents/skills/superi-execution/SKILL.md`: Defines the checkpoint execution loop after planning.
  It prohibits every further agent after planning and requires the owner to read the raw
  implementation, write code and tests, refresh maps, review the full result, run deterministic
  local verification, deliver remotely, and complete paired-tab Google Docs before `Done.`.
- `.agents/skills/superi-execution/scripts/verify_checkpoint.py`: Selects deterministic local quality
  gates from the Git change set relative to a required base revision, validates changed Python and
  JSON syntax, always validates the codebase maps, and conditionally runs the applicable workflow,
  Rust, dependency, fixture, codec-feature, canonical-slice, shell, and frontend checks. `--full`
  selects every supported gate, while `--dry-run` exposes the exact selection without executing it.
- `.agents/skills/superi-execution/agents/openai.yaml`: Supplies the display name, short description,
  and default invocation prompt for the execution skill.
- `.agents/skills/superi-mapping/SKILL.md`: Defines module discovery, shard reading, synthesis,
  map frontmatter and required sections, stale-map refresh, and whole-map validation. The checkpoint
  owner reads the initial map closure and performs every required map refresh inline. Optional stale
  maps are replaced with prescribed deeper raw-code evidence during planning.
- `.agents/skills/superi-mapping/scripts/codebase_maps.py`: Implements repository discovery, module
  assignment, UTF-8 and binary classification, deterministic source hashing, whole-file sharding,
  changed-module reporting, and strict map validation. It reads tracked plus nonignored untracked
  files, excludes generated maps, plans, Git internals, and build output, assigns crate and tool
  roots to their own modules, and assigns everything else to `workspace`. Validation checks anchored
  frontmatter, exact source ownership, revision syntax, inventory-section entries, resolved index
  links, unexpected module maps, required headings, current hashes, and forbidden Unicode dashes.
- `.agents/skills/superi-planning/SKILL.md`: Defines evidence-based inline planning for one
  checkpoint. The owner synchronizes and claims, reads live Google Docs, maps, and current code,
  performs external research only when a material question remains unresolved, and writes both
  `planning.md` and `execution.md`. No other planning document or checkpoint subagent is permitted.
- `.agents/skills/superi-planning/agents/openai.yaml`: Supplies the display name, short description,
  and default invocation prompt for the planning skill.
- `.codex/config.toml`: Pins every Superi project session to `gpt-5.6-sol` with `max` reasoning. The
  repository defines no custom agent profile or project-level agent concurrency settings because
  each checkpoint owner completes its checkpoint inline.
- `.github/scripts/check-dependency-policy.sh`: Executable Bash contract check for the dependency
  policy workflow. It requires exact workflow name, permissions, checker invocation, cargo-deny
  action inputs, unknown-Git denial, revision-pinned Git policy, and the approved OxideAV source;
  any missing required line fails before cargo-deny runs.
- `.github/scripts/provision-linux-libva.sh`: Shared Linux media and audio provisioner for both Rust
  workflows. It installs ALSA development headers and exact source-build tools, verifies the
  official libva 2.22.0 archive against a pinned
  SHA-256, builds a private prefix, verifies the VVC header and API version, installs the GBM
  development link target, and publishes header, pkg-config, native-linker, and runtime library
  paths to subsequent hosted steps.
- `.github/scripts/libvpx-windows.def`: Reviewed Windows DLL export surface containing the official
  libvpx 1.16 symbols consumed by the production runtime loader.
- `.github/scripts/provision-windows-libvpx.sh`: Windows hosted runtime provisioner that pins the
  reviewed vcpkg registry baseline, builds libvpx 1.16.0 with VP9 high-bit-depth support through the
  supported static MSVC triplet without binary-cache reuse, relinks the archive into one DLL using
  the reviewed exports, verifies the exports with CRLF-safe exact symbol checks and actionable
  diagnostics, verifies the runtime version, and publishes its native path.
- `.github/scripts/check-ci-features.py`: Standard-library contract that binds supported matrix
  lanes to explicit default or `os-codecs` policy, the real CLI feature build, engine and API
  consumer tests, and a default-only Ubuntu 22.04 job.
- `.github/workflows/ci.yml`: Defines cross-platform locked-workspace quality jobs. Pull requests and
  pushes to `main` run five macOS, Windows, and Ubuntu lanes, with Ubuntu 26.04 marked experimental;
  a separate Ubuntu 22.04 job runs weekly or by manual dispatch. Both jobs install stable Rust with
  rustfmt and Clippy, record build identity, enforce the open-tree boundary with the locked
  repository scanner, and run formatting, locked build and test commands, strict all-target Clippy,
  locked documentation tests, the supported `os-codecs` CLI build and tests, canonical fixture
  validation, and the normalized eight-stage slice contract from `open/`. Hosted macOS excludes
  only three named native codec lifecycle tests that require the physical hardware lane. Linux jobs
  run the shared provisioner to build checksum-pinned libva 2.22.0, install the GBM development link
  target and `nasm`, and publish both native-linker and runtime paths, so the locked media dependency
  graph sees the required VVC API and the approved runtime retains optimized x86 code. Intel macOS
  jobs install `nasm` with Homebrew. Linux and macOS jobs build the approved
  libvpx 1.16.0 archive after verifying its pinned checksum and expose that exact shared runtime to
  capability and codec tests. Windows builds the same approved runtime from a pinned vcpkg registry
  revision as a static MSVC archive with VP9 high-bit-depth support, then relinks it into a DLL with
  only the production loader's reviewed symbols. Both macOS lanes, Windows 2025,
  and Ubuntu 26.04
  also build the CLI with `os-codecs` and test the engine and API consumers; Ubuntu 24.04 and the
  Ubuntu 22.04 job remain default-only.
- `.github/workflows/dependency-policy.yml`: Defines the current GitHub Actions dependency-policy
  workflow. Pushes, pull requests, and manual dispatch run a read-only Ubuntu 24.04 job. After
  `actions/checkout@v4`, the job runs the repository contract checker, then uses
  `EmbarkStudios/cargo-deny-action@v2` to check all-feature licenses and sources against
  `open/Cargo.toml`.
- `.github/workflows/frontend.yml`: Defines the locked frontend typecheck and production-build gate.
  A read-only Ubuntu 24.04 job installs Node.js 24.13.0 from the repository declaration, restores
  only npm's cache, runs `npm ci`, strict TypeScript checking, a Vite production build, and the
  production application contract tests from `app/`.
- `.github/workflows/network-isolated.yml`: Defines a blocking Ubuntu 24.04 job that prepares locked
  Rust dependencies, checksum-pinned libva 2.22 and libvpx 1.16, nasm, and test artifacts online,
  then enters a distinct Linux network namespace and runs workspace tests, fixture validation, and
  the CLI consumer with Cargo forced offline.
- `.github/workflows/tauri.yml`: Defines the blocking native Tauri Rust matrix for macOS 26 arm64,
  macOS 15 Intel, Windows 2025, and Ubuntu 24.04. It installs Linux WebKitGTK 4.1 prerequisites and
  builds the production frontend before running locked formatting, mock-runtime and lifecycle tests,
  strict all-target Clippy, and native `superi-desktop` wry compilation from `app/src-tauri/`.
- `.gitignore`: Excludes Rust and JavaScript build output, editor and macOS files, local agent law,
  checkpoint plans, Python bytecode and cache directories, browser artifacts, frontend `dist/`, and
  Tauri-generated ACL schemas. In particular, `AGENTS.md`, `BASE_INSTRUCTIONS.md`, and `/plans/`
  remain local working inputs rather than normal tracked sources.
- `.worktreeinclude`: Requests that the otherwise ignored `AGENTS.md` be copied into Codex-managed
  worktrees so repository law is present in isolated checkpoint tasks.

### Product, architecture, and policy documents

- `LICENSE`: Applies the MIT license to the repository, with copyright held by Brian Gao and Justin
  Chen, and includes the standard permission, notice-retention, and warranty-disclaimer terms.
- `README.md`: Gives the public project orientation, product split, graph and GPU model, subsystem
  hierarchy, build commands, vertical slice, phases, invariants, open questions, and claimed current
  status. It identifies the production React and Tauri shell, its explicit headless-engine lifecycle,
  reliable generated transport, single application/project presentation owner, five professional
  workspace views, persistent customizable layout status and one-step reset recovery, an
  always-visible native engine lifecycle projection, and one native GPU editing viewport while retaining honest runtime
  method-routing and viewer-binding limits.
- `closed/README.md`: Defines `closed/` as a notice for the separately maintained proprietary
  Superi Max tier and states the one-way dependency rule: Max may consume open Superi, while open
  Superi must never import, link, or depend on Max.
- `docs/architecture.md`: Records the foundational product boundary, locked Rust, wgpu, native OTIO,
  Tauri, React, TypeScript, and public API directions, the graph/GPU/color/caching/concurrency model,
  subsystem inventory, continuous-integration phasing, open and closed product separation, the
  engine-owned plugin supervision boundary, and open legal or product decisions.
- `docs/checkpoints/P1.W07.C001.md`: Durable implementation evidence for cross-platform hosted build
  CI. It records the six documented lane mappings, workflow security choices, corrected Ubuntu
  22.04 cadence design, local YAML and contract proof, locked workspace build, fixture-tool tests,
  documentation tests, delivery context, and explicitly deferred CI coverage.
- `docs/checkpoints/P1.W05.C003.md`: Durable implementation evidence for explicit display and
  deliverable output color transforms. It records integration with working images, gamut and HDR
  contracts, focused and widening verification, delivery context, and intentionally separate ICC,
  look, YUV, legal-range, quantization, and GPU stages.
- `docs/checkpoints/P1.W05.C004.md`: Durable implementation evidence for bounded versioned color
  configuration, named scene-linear spaces and roles, semantic identity, drift-safe project
  working-space persistence, canonical fixture integration, critical proof, and delivery context.
- `docs/checkpoints/P1.W05.C005.md`: Durable implementation evidence for deterministic display,
  view, look, and delivery rules. It records source-role selection, ordered LUT processing,
  authoritative output-transform integration, critical-tier verification, delivery context, and
  intentionally separate persistence, ICC, GPU, storage, viewport, and export stages.
- `docs/checkpoints/P1.W05.C010.md`: Durable implementation evidence for explicit luminance-preserving
  tone mapping and the separate legal-range RGB storage encoder. It records stage ordering, public
  rule consumption, normative code anchors, alpha and artifact behavior, critical verification,
  delivery context, and the intentionally separate YUV, packed storage, ICC, GPU, and engine work.
- `docs/checkpoints/P1.W05.C011.md`: Durable implementation evidence for exact color metadata
  propagation across decoded media, graph, timeline, cache, GPU upload, viewport intent, and export
  intent. It records ordered transform history, exact source payload retention, cache identity,
  branch independence, verification, delivery context, and intentionally separate pixel execution.
- `docs/checkpoints/P1.W05.C012.md`: Durable implementation evidence for managed GPU-resident
  wide-gamut transforms, binary64 CPU reference parity, explicit pass and fence ownership, native
  export-readback proof, delivery context, and intentionally separate engine integration.
- `docs/checkpoints/P1.W07.C002.md`: Durable implementation evidence for the complete Rust CI quality
  suite. It records the low-risk configuration boundary, both-job command coverage, the explicit
  hosted macOS native codec exception, focused local proof, hosted proof requirement, delivery
  context, and deferred feature and frontend coverage.
- `docs/checkpoints/P1.W07.C003.md`: Durable implementation evidence for explicit default and
  `os-codecs` consumer coverage, the reviewed rav1d dependency correction, critical local proof,
  hosted Windows requirements, delivery context, and physical-lane limitations.
- `docs/checkpoints/P1.W07.C004.md`: Durable implementation evidence for frontend CI. It records the
  isolated contract boundary, exact Node.js, TypeScript, and Vite versions, advisory-driven Vite
  update, red-to-green and negative controls, clean locked npm verification, locked Rust tests,
  delivery
  context, and the explicit absence of the real React and Tauri application.
- `docs/checkpoints/P1.W07.C005.md`: Durable implementation evidence for native Tauri Rust CI. It
  records the CI-only host boundary, pinned Tauri versions, red-to-green corrections, focused and
  widening proof, delivery context, and the explicitly deferred Phase 3 application.
- `docs/checkpoints/P3.W01.C001.md`: Durable implementation evidence for the production React and
  Tauri shell, explicit application and headless-engine lifecycle ownership, exact participant seam,
  classified failure and recovery behavior, focused red-to-green proof, delivery context, and
  adjacent process, generated binding, transport, and editor constraints.
- `docs/checkpoints/P3.W01.C002.md`: Durable implementation evidence for the lifecycle-attached
  EngineControl process owner, bounded transport-neutral application connection, existing
  integration-validation projection, focused red-to-green proof, delivery context, and explicit
  generated-binding and command/event transport exclusions.
- `docs/checkpoints/P3.W01.C003.md`: Durable implementation evidence for the production generated
  TypeScript client adapter, complete map-derived request/event/resource surface, injected React
  provider, focused runtime forwarding proof, and explicit concrete transport exclusions.
- `docs/checkpoints/P3.W01.C004.md`: Durable implementation evidence for the thin native command and
  ordered event bridge, concrete generated frontend transport, reconnect and cancellation state,
  classified public errors, real React consumer, focused proof, and remaining method-routing limits.
- `docs/checkpoints/P3.W01.C005.md`: Durable implementation evidence for deterministic application
  routing, transient workspace layout, explicit panel and command registries, immutable shared
  public-resource selection, React composition, focused proof, and professional workspace limits.
- `docs/checkpoints/P3.W01.C006.md`: Durable implementation evidence for five professional
  registry-backed workspaces over one public editor snapshot, exact audio timing and routing
  projection, classified unavailable states, focused proof, and remaining native routing limits.
- `docs/checkpoints/P1.W07.C008.md`: Durable implementation evidence for the open-tree boundary
  scanner. It records the dependency-free tool, canonical and malformed-tree contracts, locked
  workflow integration, isolated Rust verification, delivery context, and remaining static-policy
  limitations.
- `docs/checkpoints/P1.W07.C009.md`: Durable implementation evidence for the network-isolated core
  workflow, namespace and offline contracts, focused verification, hosted proof requirement,
  delivery context, and intentionally unimplemented editorial slice.
- `docs/checkpoints/P1.W07.C010.md`: Durable implementation evidence for typed read-only frame,
  audio, timeline, and project golden harnesses. It records exact semantic envelopes, canonical
  JSON comparison, immutable fixture integration, red-to-green proof, and runtime consumer limits.
- `docs/checkpoints/P1.W07.C011.md`: Durable implementation evidence for the seven-stage benchmark
  harness, real graph evaluator workload, reproducible context, explicit gap policy, verification,
  delivery, and intentionally unregistered consumer paths.
- `docs/checkpoints/P1.W07.C015.md`: Durable implementation evidence for schema-versioned
  platform-lane reports, deterministic performance, golden, flaky, and gap findings, collision-safe
  publication, focused contracts, delivery context, and intentionally external raw artifacts.
- `docs/checkpoints/P1.W07.C017.md`: Durable implementation evidence for the canonical headless
  runner. It records exact editorial state, API and CLI integration, honest stub publication,
  red-to-green contracts, fixture dependency, verification boundaries, and delivery context.
- `docs/checkpoints/P1.W07.C006.md`: Durable implementation evidence for the dependency-policy
  checkpoint. It records the outcome, integration boundary, red-to-green process, local checks,
  initial successful GitHub Actions run `29302533491`, delivery commits, and remaining limitations.
  It is evidence of the checkpoint rather than the canonical policy source.
- `docs/checkpoints/P1.W07.C007.md`: Durable implementation evidence for the automated open-tree
  dependency-direction gate. It records the executable policy, focused and widening proof, delivery
  boundary, and the host codec limitation observed during the full workspace test attempt.
- `docs/checkpoints/P1.W07.C018.md`: Durable implementation evidence for the deterministic raw-video
  baseline. It records the 207-case scope, generator and media-I/O integration, red-to-green proof,
  verification boundary, delivery context, and explicitly deferred media behavior.
- `docs/checkpoints/P1.W07.C019.md`: Durable implementation evidence for the synchronized
  multichannel audio baseline. It records the three common sample-rate and layout cases, dependency-free
  WAVE generation, media-I/O consumer, red-to-green proof, continuity boundary, delivery context,
  and intentionally deferred playback and physical-device behavior.
- `docs/checkpoints/P1.W07.C020.md`: Durable implementation evidence for the deterministic timing
  baseline. It records the five-case scope, generator and media-I/O integration, red-to-green proof,
  verification boundary, delivery context, and explicitly deferred container and hardware behavior.
- `docs/checkpoints/P1.W07.C021.md`: Durable implementation evidence for the deterministic color and
  image-sequence baseline. It records the eight-image scope, generator, color-transform and
  media-I/O consumers, red-to-green proof, verification boundary, delivery context, and explicitly
  separate rendered golden outputs.
- `docs/checkpoints/P1.W07.C022.md`: Durable implementation evidence for deterministic malformed,
  truncated, unsupported, and partially readable PCM fixtures. It records the generator, catalog,
  production PCM consumer, red-to-green proof, post-open truncation lifecycle, delivery context, and
  intentionally focused format boundary.
- `docs/checkpoints/P1.W07.C023.md`: Durable implementation evidence for the canonical OTIO 0.18.1
  interchange baseline. It records two generated timelines, stable editorial and media identity,
  official reference semantics, unsupported preservation expectations, red-to-green proof, and the
  explicitly deferred production timeline model and reader and writer.
- `docs/checkpoints/P2.W02.C013.md`: Durable implementation evidence for production OTIO 0.18.1
  import and export. It records the native mapping, opaque preservation, stable diagnostics,
  exact-clock policy, public headless consumer, Rust contract proof, and official reference
  validation of Rust-produced outputs.
- `docs/checkpoints/P2.W04.C001.md`: Durable implementation evidence for the independent audio
  processing graph. It records deterministic editable topology, separately prepared bounded block
  execution, exact sample and ordered-channel contracts, audio-domain integration, red-to-green
  proof, delivery context, and the intentionally separate mixing, device, sync, and hosting work.
- `docs/checkpoints/P2.W04.C002.md`: Durable implementation evidence for output-device discovery and
  bounded low-latency playback. It records stable backend identity, exact capability ranges,
  lock-free whole-frame handoff, timed silence, sample-clock publication, dependency and Linux CI
  integration, critical verification, delivery context, and physical-platform constraints.
- `docs/checkpoints/P2.W04.C003.md`: Durable implementation evidence for sample-accurate audio
  timeline scheduling. It records immutable placement snapshots, callback-safe exact mapping,
  audio-master publication, dependent A/V proof, delivery context, and intentionally separate
  decoded-sample binding, routing, mixing, and engine work.
- `docs/checkpoints/P2.W04.C004.md`: Durable implementation evidence for clip gain, fades, pan,
  mute, solo, phase, and semantic channel mapping. It records transactional audio intent,
  allocation-free prepared DSP, atomic timeline identity reconciliation, real razor-edit proof,
  delivery context, and the intentionally separate bus, device, automation, and export work.
- `docs/checkpoints/P2.W04.C005.md`: Durable implementation evidence for typed submix, auxiliary,
  send, return, and master routing. It records deterministic exact-layout summing, borrowed
  prepared inputs, real-time ownership, red-to-green and dependent proof, delivery context, and
  intentionally separate metering, resampling, plugins, and engine-composition work.
- `docs/checkpoints/P2.W04.C006.md`: Durable implementation evidence for prepared band-limited
  sample-rate conversion. It records exact source and device clocks, ordered channels, fixed
  lookahead and output blocks, bounded drift correction, dependency evidence, focused proof,
  delivery context, and remaining engine and physical-device integration.
- `docs/checkpoints/P2.W04.C009.md`: Durable implementation evidence for transparent graph-native
  peak, RMS, true-peak, phase, spectrum, and loudness metering. It records bounded preparation,
  lock-free coherent snapshots, standards research, real graph-consumer proof, delivery context,
  and remaining engine, hardware, and export integration.
- `docs/checkpoints/P2.W05.C001.md`: Durable implementation evidence for the graph-native visual
  effect authoring SDK. It records typed inspectable definitions and defaults, workflow-neutral
  editable graph instances, deterministic catalog discovery, exact-schema runtime factories,
  OpenFX-informed design, red-to-green proof, delivery context, and deferred visual execution.
- `docs/checkpoints/P2.W04.C007.md`: Durable implementation evidence for canonical common channel
  layouts and explicit speaker or discrete conversion. It records prepared callback-safe matrices,
  fail-closed undefined speaker conversions, graph timing proof, and physical device limitations.
- `docs/checkpoints/P2.W04.C008.md`: Durable implementation evidence for prepared equalization,
  linked compression and limiting, fixed delay, and saturation. It records graph integration,
  exact timing and channel behavior, adjacent-block continuity, finite failure boundaries, focused
  proof, delivery context, and later automation and engine composition ownership.
- `docs/checkpoints/P2.W05.C002.md`: Durable implementation evidence for the typed transform, crop,
  opacity, blend, composite, blur, sharpen, distortion, keying, and utility node catalog. It records
  the neutral shared graph payload, timeline coexistence, schema and authoring contracts, bounded
  CPU reference semantics, research basis, real pixel and immutable graph proof, and intentionally
  separate GPU, engine, UI, persistence, playback, and export integration.
- `docs/checkpoints/P2.W06.C003.md`: Durable implementation evidence for foreground playback
  orchestration across decoded provenance, immutable graph evaluation, complete cache identity,
  CPU display color execution, bounded audio admission, audio-master timing with monotonic fallback,
  lossless viewport backpressure, degraded scene rejection, and recovery.
- `docs/checkpoints/P2.W06.C006.md`: Durable implementation evidence for engine-owned A/V
  coordination over the actual audio clock, explicit nonblocking hold, correction, protected and
  eligible-drop behavior, applied discontinuity recovery, immutable media timing, foreground
  playback integration, research basis, deterministic proof, and physical-lane limitations.
- `docs/checkpoints/P2.W06.C005.md`: Durable implementation evidence for coherent prepared-source
  render and export orchestration through decode, immutable graph evaluation, delivery color, audio
  processing, deterministic encoder selection, complete elementary packet validation, lifecycle
  degradation, reset recovery, exact PCM completion, and rejected VP9 timing drift.
- `docs/checkpoints/P2.W06.C004.md`: Durable implementation evidence for exact seek, superseding
  scrub, pause and resume, frame stepping, reduced signed speeds, direction, half-open looping,
  bounded dropped-frame policy, callback-owned audio discontinuities, explicit degradation, and
  recovery over the existing playback engine.
- `docs/checkpoints/P2.W06.C007.md`: Durable implementation evidence for the engine-wide typed
  command dispatcher, atomic revision-fenced canonical transactions, bounded ordered replacement
  events, coherent lifecycle and workflow admission, dispatcher-owned classified error and exact
  recovery state, bounded cross-domain playback control, dispatcher-owned logical export commands
  and automated state observation, public API and headless consumer integration, research basis,
  deterministic proof, and remaining wire and production transaction boundaries.
- `docs/checkpoints/P2.W06.C011.md`: Durable implementation evidence for deterministic OpenFX
  discovery, isolated worker-launch coordination, exact permission narrowing, per-plugin failure
  containment, quarantine and recovery, and one coherent availability path across playback,
  rendering, and export.
- `docs/checkpoints/P2.W07.C016.md`: Durable implementation evidence for the strict generic project
  command, every current authored operation family, complete pre-dispatch conversion, one mixed
  atomic engine transaction, correlated public events, database reload, and public undo plus redo.
- `docs/checkpoints/P2.W07.C022.md`: Defines the supported local `superi-json` language, exact-source
  digest contract, strict bounds, closed method vocabulary, deterministic interpretation,
  conflict and committed-prefix behavior, permission preflight, event preservation, durable project
  meaning, recovery compatibility, and versioning policy.
- `docs/checkpoints/P2.W07.C025.md`: Defines atomic durable generic project command recording,
  bounded retention, schema-5 persistence, cursor-safe public inspection, permission-checked replay,
  event correlation, CLI and scripting access, and recovery-lineage preservation.
- `docs/checkpoints/P2.W04.C010.md`: Durable implementation evidence for production input-device
  discovery, atomic record arming and monitoring, bounded exact-timestamp capture, and the real
  monitoring bridge into existing output playback.
- `docs/checkpoints/P2.W04.C011.md`: Durable implementation evidence for exact clip-gain keyframes,
  Read, Write, Touch, and Latch automation, immutable callback curves, serialized engine ownership,
  strict public transactions and events, routed source-to-master proof, verification, delivery, and
  intentionally deferred persistence, broader targets, and hardware control input.
- `docs/checkpoints/P2.W04.C014.md`: Durable implementation evidence for deterministic native audio
  plugin discovery and validation, exact Audio Unit and VST3 state persistence, graph delay
  compensation, isolated timing-matched fallback, checkpoint recovery and quarantine, per-node
  project save and reopen, verification, and remaining platform transport boundaries.
- `docs/checkpoints/P2.W04.C013.md`: Durable implementation evidence for macOS Audio Unit effect
  hosting. It records exact identity and configuration, verified process isolation, private native
  lifecycle and callback ownership, real Apple Peak Limiter graph proof, delivery gates, and
  intentionally deferred engine, parameter, UI, latency, instrument, MIDI, VST3, and physical work.
- `docs/checkpoints/P2.W04.C012.md`: Durable implementation evidence for worker-side VST3 effect
  hosting on macOS, Windows, and Linux, including the supported bus and layout subset, retained
  module lifecycle, exact timing and automation, bounded monitoring, isolated fixture proof,
  dependency and legal result, and later lifecycle exclusions.
- `docs/checkpoints/P1.W07.C025.md`: Durable implementation evidence for bounded timing and process
  resident-memory instrumentation across all eight canonical slice stages. It records the private
  sampler boundary, schema 1.1.0 report contract, dependency decision, red-to-green proof,
  delivery context, and limits of stage-boundary sampling.
- `docs/checkpoints/P1.W07.C024.md`: Durable implementation evidence for the canonical editorial
  expectation fixture. It records reference-frame derivation, strict CLI consumption, audio
  timing and routing preservation, red-to-green proof, delivery context, and disclosed runtime
  limits.
- `docs/checkpoints/P1.W07.C026.md`: Durable implementation evidence for the hosted fixture and
  slice baseline plus portable expectation version 2. It records both red-to-green failures,
  checkout-independent state identity, workflow and platform integration, contributor replacement
  rules, verification, delivery context, and honest hardware and runtime limits.
- `docs/codecs.md`: Version 0.6 of the codec and licensing policy. It separates permissive in-tree
  codecs, opt-in operating-system codecs, vendor RAW workers, still-image handling, containers,
  capability introspection, platform backend contracts, and currently documented MP3, VPx, Opus,
  AV1, and VA-API behavior. It also records the zero-copyleft allowlist and unresolved AAC and DNxHR
  patent questions.
- `docs/north-star.md`: Defines the product destination, the complete offline open editor, the
  additive hosted Superi Max tier, the transform-existing versus generate-new boundary, the four
  professional disciplines, local bounded AI, and the asymptotic definition of success.
- `docs/phase-0-build-contracts.md`: The founder-approved canonical Phase 0 specification. It locks
  offline behavior, Rust and wgpu, Tauri and React ownership, JSON-RPC transport, native GPU
  viewport, OTIO preservation, legal review, dependency and model licensing, SQLite project files,
  node and artifact semantics, ACEScg color, thread ownership, plugin containment, quantitative
  performance and quality targets, and explicit change control.
- `docs/phases.md`: Defines the canonical build sequence from decisions through engine parts,
  orchestration, UI, hardening, and launch. It repeatedly requires a real import, timeline, trim,
  effect, and export vertical slice to pull subsystem design and public API integration forward.
- `docs/platform-testing.md`: Defines revision 2 of required automated and physical test lanes for
  macOS, Windows, and Ubuntu, stable suite identifiers, cadence and blocking rules, deterministic
  cross-platform expectations, the portable `slice-contract` versus physical all-runtime `slice`
  distinction, capability-based codec testing, and the structured evidence every result must retain.
- `docs/unsafe-ffi.md`: Defines the deny-by-default unsafe policy and inventories audited macOS
  CoreGraphics, AV1, Opus, VPx, VideoToolbox, AudioConverter, Audio Unit hosting, Windows Media
  Foundation, Linux VVC VA-API, and cross-platform VST3 worker-host boundaries. It records
  ownership, buffer, thread, state, latency, failure, and target proof for each boundary plus required
  source scans, Clippy runs, and focused tests. Audio Unit class-info property lists and VST3 bounded
  seekable `IBStream` transfers are explicit native-state boundaries.
- `docs/vertical-slice.md`: Defines revision 1 of `superi.slice.canonical.v1`. It pins the immutable
  video fixture role, exact one-track edit and trim, one typed horizontal-mirror graph effect,
  explicit delivery, eight stable replacement stages, schema 1.1.0 runner report, bounded stage
  timing and resident-memory records, conformance levels, portable project-state proof, shared
  hosted baseline, same-change production replacement rule, and the boundaries owned by
  P1.W07.C017 through P1.W07.C026.

- `docs/checkpoints/P3.W02.C001.md`: Durable implementation evidence for the managed GPU display
  presenter, native Tauri viewport owner, and persistent editing-panel geometry and status bridge.
- `docs/checkpoints/P3.W02.C002.md`: Durable implementation evidence for the strict control-only
  viewport placement payload, direct native presentation boundary, focused command-seam contract,
  and explicit exclusion of media handles and webview image fallbacks.
- `docs/checkpoints/P3.W02.C003.md`: Durable implementation evidence for the four native viewer
  roles, canonical scene-to-display intent, role-aware workspace consumers, and focused proof.
- `docs/checkpoints/P3.W03.C001.md`: Durable implementation evidence for create, open, close, save,
  save-as, bounded recents, revision-fenced recovery, four actionable failure classes, the Tauri
  session owner, and the production React consumer.
- `docs/checkpoints/P3.W03.C002.md`: Durable implementation evidence for project-owned frame rate,
  resolution, color, audio, cache, proxy, and working-folder settings attached to that lifecycle.
- `docs/checkpoints/P3.W03.C003.md`: Durable implementation evidence for atomic media import,
  deterministic folder and image-sequence discovery, picker and drag/drop consumers, stable public
  command/event/automation parity, durable reopen, and duplicate no-op behavior.
- `docs/checkpoints/P3.W03.C004.md`: Durable implementation evidence for project-identity bins,
  sub-bins, list and grid browsing, transparent derived thumbnails, read-only metadata, and saved
  smart collections without absorbing later relink, proxy, metadata-editing, or search ownership.
- `docs/checkpoints/P3.W03.C005.md`: Durable implementation evidence for freshness-fenced source
  metadata inspection, bounded editable user metadata, stable media identity and bin intent,
  missing-source availability, and explicit C006 and C007 ownership exclusions.
- `docs/checkpoints/P3.W03.C006.md`: Durable implementation evidence for typed editorial
  annotations, revision-fenced atomic replacement, derived-only timeline usage indicators, C005
  metadata preservation, and explicit C007 and later-work exclusions.
- `docs/checkpoints/P3.W03.C007.md`: Durable implementation evidence for exact fingerprint duplicate
  identity, persisted exact-time selections, fixed-point manually refinable tracked regions, C006
  annotation preservation, and explicit C008 and later-work exclusions.
- `docs/checkpoints/P3.W03.C008.md`: Durable implementation evidence for source-fresh replaceable
  proxy and optimized-media attachments, explicit quality choice, inspectable status, deterministic
  original fallback, C007 state preservation, and explicit C009+ exclusions.
- `docs/checkpoints/P3.W03.C009.md`: Durable implementation evidence for local offline state and
  search, revision-fenced relink, intentional source replacement, frame-rate conform, preserved C008
  freshness and fallback, and the production media-browser consumer.
- `docs/checkpoints/P3.W03.C010.md`: Durable implementation evidence for revision-fenced generated
  thumbnails, canonical-order filmstrips, exact channel-separated PCM waveforms, selected-media
  previews, bounded ephemeral artifacts, typed unavailable states, and the production inspector.
- `docs/checkpoints/P3.W03.C011.md`: Durable implementation evidence for source-bound editable
  language artifacts, exact transcript timing, speaker and timeline relationships, local AI content
  entries, deterministic metadata plus transcript plus local-content search, and the production
  inspector consumer without model or network dependence.
- `docs/checkpoints/P3.W03.C012.md`: Durable implementation evidence for one ordered atomic batch
  transaction spanning rename, organization, transparent optimized transcode and proxy state,
  fingerprint-guarded relink, metadata upsert and removal, real multi-selection UI consumption,
  complete rollback, derived-only persistence exclusion, and deterministic original fallback.
- `docs/checkpoints/P3.W03.C013.md`: Durable implementation evidence for persisted source
  baselines, conservative removable-volume state, exact changed-byte detection, explicit relink
  intent, stable editorial identity, revision-fenced scans, strict bridge and inspector consumption,
  and adjacent preview, batch, search, proxy, and offline compatibility.
- `docs/checkpoints/P3.W03.C014.md`: Durable implementation evidence for retained source-session
  loading, exact seeking, fingerprint-bound in and out marks, optimistic revision fencing, the
  engine source-only registry consumer, honest native viewer separation, and focused real-source
  proof.
- `docs/checkpoints/P3.W04.C001.md`: Durable implementation evidence for the canonical timeline
  canvas projection, identity-preserving tracks and items, exact rulers, playhead and range intent,
  anchored scroll and zoom behavior, real editing-workspace consumption, and authored-state
  ownership exclusions.
- `docs/checkpoints/P3.W04.C002.md`: Durable implementation evidence for clip names, real generated
  filmstrips, thumbnails, and waveforms, source and editorial badges, graph effects and drivers,
  exact clip-gain keyframes with clip-relative diamond positions, canonical and shared selection
  state, strict freshness rejection, and reuse of the existing canonical timeline projection
  without another authored owner.
- `docs/checkpoints/P3.W04.C003.md`: Durable implementation evidence for all eleven canonical track
  gestures, strict state migration, compiled output intent, project history, removed-clip audio
  reconciliation, generated public contracts, the real durable native route, and the transport-free
  professional timeline consumer.
- `docs/checkpoints/P3.W04.C004.md`: Durable implementation evidence for application-owned timeline
  selection references, canonical group and link expansion, direct and range selection, geometric
  lasso, roving keyboard navigation, accessibility state, and authored-state ownership exclusions.
- `docs/checkpoints/P3.W04.C005.md`: Durable implementation evidence for exact owner-clock timeline,
  playhead, item-edge, and marker-edge targets, stable tie ordering, configurable transient rules,
  visible consequence feedback, Escape reversal, strict malformed-owner failure, and preservation
  of the lower authored-state boundary.
- `docs/checkpoints/P3.W04.C007.md`: Durable implementation evidence for exact ripple, roll, slip,
  slide, razor, trim, extend, ripple-delete, and gap plans, mixed-clock synchronization, typed
  identity allocation, immutable consequence previews, the shared atomic project executor, and
  lower-owned durable semantics and history.
- `docs/checkpoints/P3.W04.C009.md`: Durable implementation evidence for atomic exact transition
  handles, deterministic duration and alignment, adjacent-media limits, typed graph parameter
  controls, application-owned command execution, strict failure behavior, and remaining value-kind
  and safe-integer limits.
- `docs/checkpoints/P3.W04.C010.md`: Durable implementation evidence for all six atomic marker
  gestures, complete visible marker state, exact and non-navigable navigation behavior, project
  history, strict public contracts, selected-project persistence, native routing, and immediate
  revision-fenced typed inverse reversal.
- `docs/checkpoints/P3.W04.C016.md`: Durable implementation evidence for exact trim, slip, slide,
  multicam, and audio consequences in source and program viewers plus the structural meter rack,
  including canonical sample clocks, ordered channel routes, continuity seams, explicit unobserved
  signal telemetry, native placement isolation, and application-owned transient state.
- `docs/checkpoints/P3.W04.C015.md`: Durable implementation evidence for audio-video link, exact
  sample-clock synchronization, detach, replacement intent transfer, complete channel routing,
  strict public schema releases, generated TypeScript, accessible production controls, and
  application-owned durable command publication.
- `docs/checkpoints/P3.W05.C002.md`: Durable implementation evidence for exact seek, superseding
  scrub, explicit audio-scrub behavior, canonical playhead follow, focused proof, and delivery.
- `docs/checkpoints/P3.W05.C003.md`: Durable implementation evidence for native viewer fit, zoom,
  pan, pixel, fullscreen, and cinema presentation modes with preserved playback and overlay state.
- `docs/checkpoints/P3.W05.C004.md`: Durable implementation evidence for presentation-only safe
  area, guide, grid, ruler, center, aspect, and custom viewer overlays.
- `docs/checkpoints/P3.W05.C005.md`: Durable implementation evidence for exact role-aware viewer
  timecode, frame, source, physical dropped-frame, playback, visual, audio, comparison, and
  editorial-intent displays over existing canonical owners.
- `docs/checkpoints/P3.W05.C006.md`: Durable implementation evidence for shell-local compare,
  split, wipe, difference, reference, and snapshot views with exact native frame identity, separate
  temporal navigation context, capture gating, live playback communication, and preserved native
  IPC ownership.
- `docs/checkpoints/P3.W05.C007.md`: Durable implementation evidence for GPU-resident alpha,
  individual-channel, luminance, false-color, and display-linear clipping inspection, exact
  selected-versus-presented state, and preserved canonical scene and display meaning.
- `docs/checkpoints/P3.W05.C008.md`: Durable implementation evidence for exact selected-clip
  built-in transform projection, ordinary graph matrix and sampling controls, driver restrictions,
  application-owned typed mutations, and authored-state separation from viewer navigation.
- `docs/checkpoints/P3.W05.C009.md`: Durable implementation evidence for exact active-monitor ICC
  identity and freshness, reversible per-viewer monitor selection, explicit sRGB and Display P3 GPU
  output transforms, strict control-only IPC, preserved viewer state, and the intentionally absent
  arbitrary ICC tag evaluator.
- `docs/checkpoints/P3.W05.C010.md`: Durable implementation evidence for selectable external
  display output from all four native viewer roles, connection-local monitor routing, exact target
  and presentation diagnostics, shared canonical GPU textures, external failure isolation, and
  preserved navigation, overlay, comparison, analysis, temporal, visual, and audio ownership.
- `docs/checkpoints/P3.W05.C011.md`: Durable implementation evidence for nonblocking frame and
  audio cache indicators over exact scheduled timing, predictive and output degradation, discard
  acknowledgement, sample clocks, ordered channel meaning, routing intent, continuity seams, and
  explicit unavailable telemetry across every transport mode.
- `docs/checkpoints/P3.W06.C001.md`: Durable implementation evidence for persistent native window
  creation, restoration, bounded multi-window behavior, fullscreen, monitor movement, reversible
  placement, per-webview transport generations, one shared engine and project authority, and
  primary-window-only native GPU viewer ownership.
- `docs/checkpoints/P3.W06.C002.md`: Durable implementation evidence for native desktop menus,
  dialogs, clipboard roles, project and media drag and drop, recent documents, path-safe document
  titles, workspace continuity, active project restoration, and one-shot safe close resolution.
- `docs/checkpoints/P3.W06.C003.md`: Durable implementation evidence for exact `.superi` association
  metadata, startup-argument and macOS resource-open ingress, nonblocking reuse of the sole desktop
  project owner, complete replacement snapshots, visible failure retention, and workspace-preserving
  React consumption.
- `docs/checkpoints/P3.W06.C005.md`: Durable implementation evidence for read-only shell capability
  discovery across GPU, audio, codecs, and AI, strict retained-state behavior, exact audio meaning,
  production System-panel visibility, and the absence of runtime or authored-state mutation.
- `docs/checkpoints/P3.W06.C009.md`: Durable implementation evidence for the searchable application
  and native action catalog, stable automation identities, typed availability, accessible transient
  palette, focused native menu intent, and preserved project and workspace ownership.
- `docs/checkpoints/P3.W06.C010.md`: Durable implementation evidence for configurable application
  command shortcuts, transactional conflict detection, native accelerator reservation, deterministic
  import and export, accessible capture, and private cross-session continuity.
- `docs/checkpoints/P3.W06.C012.md`: Durable implementation evidence for the fixed versioned
  color-critical dark environment, semantic chrome tokens, separate viewer and marker color-data
  tokens, deterministic bootstrap recovery, and preserved native color-pipeline ownership.

### Production desktop application

- `app/.node-version`: Pins Node.js 24.13.0 for local and hosted production application gates.
- `app/index.html`: Supplies the production webview document, static color-critical dark theme
  identity, schema, scene-owner declaration, browser chrome color, and React module entry before
  JavaScript startup.
- `app/package-lock.json`: Locks React 19.2.7, Tauri API 2.11.1, Tauri dialog 2.7.1, Tauri CLI
  2.11.4, TypeScript 5.9.3, Vite 7.3.6, the React Vite plug-in 5.2.0, and their transitive frontend
  dependencies.
- `app/package.json`: Declares the private production application package, exact toolchain and
  runtime pins, strict typecheck, Vite build, lifecycle, binding, transport, and application
  framework, command-palette, configurable shortcuts, shared application-presentation, theme,
  native desktop-shell, editor-workspace, timeline-canvas, timeline-nesting,
  timeline-multicam, timeline-clip,
  timeline-transition, caption projection, exchange, and authoring, editorial-feedback, exact
  playback transport, viewer navigation, overlay, comparison, viewer-status, analysis, and
  viewer-transform, viewer color-management, external-display, and persistent window-session
  helper contracts, configurable keyboard-shortcut contracts, strict system-capability projection,
  and Tauri commands.
- `app/src/api.ts`: Re-exports the complete canonical generated TypeScript contract and constructs
  one frozen `SuperiApiBindings` surface around an injected `SuperiTransport` and `SuperiClient`.
- `app/src/api-context.tsx`: Provides the nullable, transport-injected React API context and hook
  without owning project state or concrete reliability behavior.
- `app/src/application.ts`: Defines immutable panel, route, and command registries, canonical left,
  center, right, and bottom dock contracts, route-local ordered tab placement, bounded dock sizing,
  active-tab, visibility, and focus reducers, structural default versus custom layout status,
  deterministic all-route default reset, one exact transient reset undo, complete private workspace
  projection, live-registry reconciliation for restored presentation, panel-only restoration that
  preserves the window-session-owned route, typed immutable shared public resource selection,
  required frozen
  command discoverability metadata, transient palette visibility, current command availability,
  complete modifier, named, function, and Unicode shortcut token normalization, and generated-client
  command delegation without transport behavior.
- `app/src/application-context.tsx`: Provides the sole React application/project presentation owner,
  current keyboard-shortcut profile, configurable keyboard-to-command registry adapter,
  explicitly global editable-control shortcuts, asynchronous command execution and availability
  lookup, and one last-valid public editor
  snapshot, stale-response rejection, generated project, audio, and job refresh subscriptions, and
  classified failure retention above the existing injected generated API. It also owns unique
  project transaction identity, the visible project revision fence, durable authored action
  execution, the current source-monitor snapshot, exact generated project-command submission,
  response correlation, failure classification, a typed generic project-action callback, one
  application-owned cross-sibling editorial feedback replacement, one formatted transient program
  comparison summary for the sibling playback display, and complete replacement-state refresh for
  timeline consumers. It additionally owns unique playback
  transaction identities, submits the generated playback command, verifies immediate bounded
  acceptance, and polls the same editor-state replacement until the playback-domain owner has
  completed without creating a React transport model.
- `app/src/command-palette.ts`: Owns the bounded pure action catalog, frozen stable metadata,
  token-complete locale-independent search ranking, current availability projection, encoded recent
  path identities, effective configurable shortcut projection, and typed delegation to either an
  application command or desktop shell intent. It imports no generated API binding and owns no
  transport or mutation authority.
- `app/src/command-palette.tsx`: Renders the transient accessible modal palette with search,
  listbox navigation, disabled reasons, stable identity visibility, pending and failure state,
  dismissal, and prior-focus restoration. It invokes only the catalog host supplied by the
  always-mounted application consumer.
- `app/src/command-palette.css`: Defines the isolated modal surface, native backdrop, search,
  result, disabled, focus, and responsive presentation entirely through the application theme
  tokens without changing workspace or project state.
- `app/src/keyboard-shortcuts.ts`: Defines the immutable schema-1 shortcut profile, canonical
  effective binding resolution, platform-aware event capture, deterministic import and export,
  transactional update and reset behavior, conflict detection, native accelerator reservations,
  bounded inactive-command retention, and accessible platform labels without React or Tauri state.
- `app/src/keyboard-shortcuts-panel.tsx`: Renders the System route's accessible shortcut editor with
  labeled read-only capture fields, IME-safe keyboard capture, current and default values, clear and
  reset controls, conflict alerts, inactive-command visibility, bounded JSON import, deterministic
  export, native reservation disclosure, and two-step reset-all confirmation.
- `app/src/keyboard-shortcuts.css`: Isolates keyboard-shortcut panel layout, responsive row wrapping,
  status and alert presentation, keyboard focus visibility, and narrow-window behavior through the
  fixed application's semantic theme tokens.
- `app/src/application-presentation.ts`: Defines the framework-neutral application feedback model.
  It validates all four recoverability classes, preserves safe source context and optional
  last-valid resource identity, projects retained crash, lifecycle, project, transport, and public
  export-job evidence, derives truthful operational status and progress, bounds immutable
  notification history, and clamps context-menu placement without importing native commands or
  taking business-state ownership.
- `app/src/application-presentation.tsx`: Provides the single React presentation context and real
  consumers for described tooltips, focus-returning keyboard and pointer context menus clamped from
  their measured size, time-limited notification toasts with retained bounded history, classified
  failure cards, semantic progress, and the always-visible application status bar. Its callbacks
  route only to existing application, project, lifecycle, workspace, and recovery owners.
- `app/src/theme.ts`: Owns the frozen schema-1 `color-critical-dark` application contract, browser
  theme color, native color-pipeline scene owner, untouched workspace-state policy, and deterministic
  document declaration repair with frozen operational evidence. It has no storage, generated API,
  Tauri, project-action, viewer-state, or network dependency.
- `app/src/theme.css`: Owns semantic application surface, border, text, focus, selection, status,
  recovery, feedback, and shadow values plus compatibility aliases for existing presentation-only
  media browser styles. Separately named viewer and marker color-data tokens preserve exact
  surround, overlay, comparison, and authored flag meaning without making them interchangeable
  chrome.
- `app/src/panel-workspace.tsx`: Renders the real route panel consumer as four stable dock targets
  with ordered tablists, mounted inactive tabpanels, labeled hide and dock controls, keyboard tab
  navigation, HTML drag reordering and cross-dock movement, pointer and keyboard separators, a
  live focus announcement, and a shared pointer or keyboard context menu for activate, dock, and
  hide intent. It dispatches only application presentation actions and owns no project, history,
  transport, engine, or close behavior.
- `app/src/window-session.ts`: Defines strict immutable window-session, monitor, placement,
  persistence, recovery, and recently closed DTOs; validates every native replacement; wraps the
  exact snapshot, create, focus, fullscreen, monitor move, placement undo, workspace update, close,
  and reopen commands; and exposes the one replacement event without owning persistence or native
  placement.
- `app/src/window-session-panel.tsx`: Renders the System panel's real persistent window consumer.
  It reports exact focus, route, fullscreen, placement, monitor, persistence, and recovery state,
  exposes bounded create, focus, fullscreen, monitor movement, undo, close, and reopen controls,
  serializes pending actions, and retains the last valid replacement alongside actionable errors.
- `app/src/editor-project.ts`: Defines the exact five workspace identities, public editor-state
  request construction, immutable presentation contract, and sample-preserving audio projection
  without React, transport, or mutable state ownership.
- `app/src/editor-workspaces.tsx`: Renders editing, compositing, color, audio, delivery, and shared
  selection panels from the one application-owned public snapshot, including exact sample rates,
  ordered channels, routes, synchronization observation, continuity evidence, source and program
  editing viewers, one composite viewer, one color viewer, and the canonical timeline canvas. It
  passes the existing shared selection, dispatch, public schema, project revision, and the
  application-owned project action and generic command callbacks into that canvas for track,
  marker, and timeline edit batches. The editing panel consumes application-owned source and
  program consequence feedback plus one ordered audio routing and continuity rack, and retains the
  stateful `SourceMonitor` in the editing source slot with the exact playback control consumer below
  the dual viewer. Source viewer state receives the monitor's exact rational navigation coordinate,
  while program, composite, and color viewer state receives the attached playback playhead as
  separate navigation context. The program viewer publishes its formatted comparison state through
  the existing application owner, without moving a new context, reducer, API client, or Tauri
  access into this workspace file. Shared timeline selection can become an exact
  replace or backspace target without locally mutating canonical timeline state. Delivery projects
  each public export job through a semantic determinate or indeterminate progress element and shows
  its attached failure category and recoverability without inventing scheduling state.
- `app/src/playback-controls.tsx`: Renders play, pause, stop, loop, JKL shuttle, variable exact
  speed, direction, single-frame, exact seek, and bounded latest-target scrub controls through the
  application-owned generated command. It serializes interactions, follows only canonical returned
  playhead state, inspects exact playback state at a bounded cadence only while playing,
  ignores editable keyboard targets, and communicates exact playhead and scheduling clocks, rate,
  direction, loop, continuity, drop, visual, audio, synchronization, live application-owned program
  comparison, failure, pending, and explicit degraded-output state without importing Tauri or
  claiming rendered pixels or audio.
- `app/src/playback-transport.ts`: Defines transport-neutral pure JKL and Space command derivation,
  exact half-open-range navigation mapping, rational time and rate formatting, fixed variable-rate options, and complete user-facing
  labels for every engine playback degradation code.
- `app/src/viewer-navigation.ts`: Defines the frozen shell-local fit, bounded zoom, pan, exact 1:1
  pixel, fullscreen, and cinema state contract plus deterministic display transforms and explicit
  role-addressed external-display intent without importing playback or overlay ownership.
- `app/src/viewer-overlays.ts`: Defines the frozen seven-kind overlay catalog, immutable visibility
  state, deterministic custom inset geometry, and ordered visible projection without importing
  navigation, playback, comparison, or status-display ownership.
- `app/src/viewer-comparison.ts`: Defines the frozen single, compare, split, wipe, difference,
  reference, and snapshot catalog; immutable exact native frame identities and optional separate
  rational navigation contexts; capture and availability gates; bounded split or wipe position; orientation; and
  deterministic live state formatting without importing React, Tauri, playback, project, pixel, or
  transport ownership.
- `app/src/viewer-status.ts`: Defines the frozen role-aware viewer display contract over existing
  editor and source-monitor snapshots. It validates project rate and timecode settings, rescales
  exact clocks with checked integer arithmetic, matches core non-drop and supported NTSC drop-frame
  label projection, selects the exact active program item, and reports source, record, identity,
  relationships, playback, physical drops, visual, audio, comparison, degradation, failure, and
  editorial intent. Its additive frame and audio cache indicators preserve exact foreground
  scheduling, predictive failure, decoded-output availability, callback discard acknowledgement,
  sample clocks, ordered channels, complete routes, and continuity seams while explicitly refusing
  to invent occupancy, hit, prediction-completion, buffer-fill, device-output, or audible-sample
  evidence. The projector accepts no command, setter, transport, timer, poll, or authored state.
- `app/src/viewer-analysis.ts`: Defines the frozen eight-view image and diagnostic catalog, stable
  cross-boundary codes, exact labels and semantics, image default, and fail-closed lookup without
  owning authored, navigation, overlay, playback, comparison, or display-transform state.
- `app/src/viewer-transform-controls.ts`: Defines the frozen selected-clip transform projection and
  typed action builder over the existing editor snapshot and shared application selection. It
  validates one timeline-scoped canonical graph envelope, schemas, node and port topology, order,
  drivers, clip identity, and every exact built-in `superi.effect.transform` 1.0.0 parameter,
  traverses reachable processing nodes in canonical order, decodes all nine finite matrix scalars
  plus nearest or bilinear sampling, and emits one ordered `mutate_graph` action containing only
  changed ordinary parameters. Missing, ambiguous, stale, malformed, unsupported, and driver-owned
  states fail closed without React, transport, history, persistence, rendering, or CSS ownership.
- `app/src/viewer-color-management.ts`: Defines the deeply frozen viewer color-management contract
  for an exact bounded native monitor catalog, exact ICC profile identity and freshness evidence,
  canonical ACEScg scene meaning and RGBA16F precision, explicit sRGB and Display P3 presentation
  transforms, immutable per-role selection payloads, and honest diagnostic formatting. It validates
  all native values and does not accept pixels, ICC bytes, project mutations, playback commands, or
  arbitrary ICC transform claims.
- `app/src/viewer-external-display.ts`: Defines frozen transient external target selection,
  stale-target reconciliation, exact target and native presentation diagnostics, and deterministic
  external output formatting without importing React, Tauri, project, playback, pixel, or authored
  state ownership.
- `app/src/timeline-workspace.ts`: Strictly projects the embedded canonical revision 2 timeline
  document into a deeply frozen canvas model with exact rational source and record ranges, stable
  identities and relationships, complete caption text, language, speaker, style, timeline
  relationships, object metadata, track language and purpose, exact transition from and to offsets,
  bounded track height, lock,
  mute, solo, enable state, exact audio sample rate, ordered source and destination channel meaning,
  explicit main or audio-track routing destination, and one validated target per source channel,
  external-global-start display placement, complete visible marker state, exact owner-clock snap
  targets, stable exact marker navigation, deterministic extent,
  ruler, time-label, visible-window, and range math, and explicit malformed-document rejection. Its
  pure snap resolver honors the canonical switch plus transient target rules, exact cross-clock
  representability, integer frame tolerance, and the lower stable target order without authoring
  timeline state. Pure selection helpers add reversible timeline-object identity, topmost-first
  order, canonical fixed-point group and enabled-link expansion, direct selection, contiguous
  ranges, same-track and nearest-temporal cross-track navigation, and normalized lasso intersection.
  Its pure edit planner validates source-monitor identity, freshness, stream kind, retained bounds,
  inclusive source marks, exact source and target clocks, selection, legal bounds, and minimum
  fragment IDs. It builds generated insert, overwrite, append, replace, lift, extract, backspace,
  undo, and redo project requests plus all four exact three-point placements and equal-duration
  four-point edits with a visible consequence description. It rejects missing marks, inexact clock
  conversion, out-of-bounds derivation, and unsupported fit-to-fill before submitting a project
  command. Pure audio-video planners resolve exactly one video clip and one audio clip by stable
  track identity, build link, exact synchronization, and detach operations, and build one complete
  routing replacement without inventing missing channel decisions.
- `app/src/timeline-captions.ts`: Implements bounded deterministic SRT and WebVTT parsing and
  serialization with strict cue-text escaping and unsupported-markup rejection, canonical
  lowercase color and whole-point style field conversion, exact millisecond caption-track action
  planning with explicit gaps and authored language and purpose, canonical track export, and freshness-fenced transcript conversion
  that preserves editable text, timing, speaker, and timeline relationships without retaining model ownership.
- `app/src/timeline-captions.tsx`: Renders selected caption text, language, speaker, style, and
  relationship editing plus discoverable SRT or WebVTT import, fresh language-analysis import, and
  canonical track export. It retains only transient form state and submits authored changes through
  the injected application project action executor.
- `app/src/timeline-editing.ts`: Compiles ripple, roll, slip, slide, razor, trim, ripple or roll
  extend, ripple delete, synchronized gap insertion, and gap closure from the frozen canonical
  canvas model into existing public `TimelineEditOperation` batches. It preserves exact rational
  clocks with checked integer conversion, validates primary and synchronized track locks before
  typed identity allocation, derives the lower modified-object boundary for immutable previews,
  and leaves atomic mutation, history, grouping, synchronization, and final validation with the
  native project command owner.
- `app/src/timeline-editorial-feedback.ts`: Purely projects the active canonical canvas, strict clip
  detail, exact transient edit plan, and editor audio state into one deeply frozen source-viewer,
  program-viewer, multicam, and audio-meter replacement. It distinguishes trim, slip, and slide
  consequences, preserves sample clocks, ordered channel identities, routes, audibility, and seam
  evidence, and marks live signal telemetry `unobserved` rather than inventing levels.
- `app/src/timeline-clip-presentation.ts`: Supplements the existing frozen canvas model with strict
  read-only clip media names and relink state, exact time maps, markers, metadata, complete multicam
  angle identities, enabled state, exact switch ranges, and audio policy,
  clip-scoped graph effects and parameter drivers, and attached clip-gain sample keyframes with
  exact clip-relative positions for any active timeline in the same canonical project revision. It
  requires the root exactly once, scopes raw clip, marker, metadata, multicam, effect, and automation
  lookup to the active model, leaves geometry with `projectTimelineDocument`, stops effect traversal
  at timeline-owned nodes, rejects malformed detail, and never infers unsupported visual animation
  curves.
- `app/src/timeline-nesting.ts`: Strictly projects every canonical timeline into an immutable
  catalog, derives exact mixed-clock timeline duration and direct child dependencies, filters
  cycle-safe placement candidates, reconciles root-anchored transient open paths, and builds exact
  append or equal-duration replace nested actions plus selection-derived compound actions in
  canonical track and object order without owning mutation state.
- `app/src/timeline-multicam.ts`: Strictly projects a selected nested clip, synchronized source,
  angle catalog, exact-playhead source availability, switch partition, sync provenance, and audio
  intent from canonical timeline state.
  It uses checked BigInt arithmetic to map exact record and source clocks in both directions and
  builds only generated create, attach, sync, switch, cut-move, audio, and detach actions without
  owning project state or transport.
- `app/src/timeline-transition-presentation.ts`: Joins exact canvas transition timing with the one
  project-root compiled graph document, including when the active canvas is a child timeline. It
  derives stable endpoint and graph identity, adjacent-handle limits, duration and alignment,
  downstream processing nodes, animatability, drivers, host restrictions, canonical floating-point
  bit values, and editable scalar, Boolean, and choice intent. It builds strict `set_transition`
  and `set_parameter` project actions while keeping malformed optional graph detail separate from
  proven canonical timing.
- `app/src/timeline-retime.ts`: Owns presentation-only exact retime drafts and command planning over
  one canonical clip. It classifies identity, speed, reverse, freeze, and multi-segment maps; uses
  BigInt to reduce rational rates and derive exact source seams; splits and removes record-curve
  boundaries predictably; rejects no-ops, unsafe wire integers, duration mismatches, and inexact
  clocks; and emits only the generated public `retime` project command.
- `app/src/timeline-retime-editor.tsx`: Renders one exact selected clip and track target, authored
  and proposed timing state, source anchor, rational segment controls, playhead point insertion,
  point removal, an accessible record-to-source curve, visible consequences, draft reset, apply,
  and immediate history undo. It owns only reversible local draft state and receives transaction,
  pending, command, and history behavior from `TimelineWorkspace`.
- `app/src/timeline-workspace.tsx`: Renders the editing timeline with sticky track headers and ruler,
  exact record-positioned items, transient playhead and in and out range, native scrolling,
  pointer-anchored zoom, topmost-first track presentation, bounded visible-item overscan, fit
  controls, frame stepping, accessible clip buttons, authored and interaction selection indicators,
  authored state badges, graph effects and automation keys, six session snap rules, exact target
  consequence status, a visible target guide, and Escape or pointer-cancel restoration without
  taking authored mutation ownership. It adds four-kind creation, inline naming, bounded height,
  order, target, lock, sync lock, audio mute and solo, enable, guarded delete, pending and failure
  state, and honest unavailable state. It also exposes explicit per-track edit targeting, all nine
  editorial gestures, a discoverable four-mode three-point rule, exact source engine, source,
  target, consequence, pending, and failure state, Backspace extraction, and immediate undo and redo
  through the application command callback. Ripple, roll, slip, slide, razor, trim, ripple or roll
  extend, ripple delete, gap insert, and gap closure use the exact compiler, snap-aware pointer
  drafts, one-frame nudges, visible affected-object previews, and one shared pending exclusion before
  publishing the entire operation batch through the injected application action executor. Exact
  speed, reverse, freeze, and time-remap authoring use that same application-owned command and
  history path. The same component projects root-anchored nested breadcrumbs, back and explicit
  selected-clip open controls, nested-clip double-click navigation, cycle-safe child and
  target-track selectors, append or equal-duration replace placement, and selection-derived
  compound creation with visible pending, success, and failure evidence. Open paths remain
  transient while authored placement and compound actions publish through the same injected
  application action executor. A multicam panel uses that same selection, exact playhead, pending
  exclusion, project action callback, refreshed revision, and history owner to create or attach a
  synchronized source, show engine-authored angle state, take an angle at the playhead, refine cuts
  one frame at a time, change sync and audio intent, detach, and undo. It does not claim decoded
  angle frames or mixed multicam audio. It also provides a dedicated marker panel with complete marker listing,
  exact previous and next navigation, timeline-owned create at playhead, range, label, flag, and note
  editing, removal, pending and error state, and revision-fenced typed inverse reversal. It also
  publishes one immutable idle, preview, applying, applied, failed, or unavailable editorial
  feedback replacement through an injected application callback. The projection consumes current
  edit plans and canonical audio or clip detail without becoming an authored owner. It
  progressively reads one revision-matched media library,
  deduplicates sources, generates previews sequentially, and accepts only matching media and
  freshness identities before displaying filmstrips, thumbnails, or waveforms.
  A dedicated Audio and video panel exposes exact Link, Sync by source time, Detach audio, and
  Replace audio gestures plus an audio-track selector, explicit main or audio-track destination,
  and one channel or mute choice per source channel. It publishes through the same injected project
  action executor and reports disabled reasons, pending state, success revision, and failures.
  It also projects the existing application selection into labeled multiselect options, group and
  link aware click selection, exact-object Option selection, Command or Control toggle, Shift range,
  mounted-rectangle lasso preview and commit, roving keyboard focus, offscreen focus scrolling, and
  a polite live count. Exactly one selected transition adds visible handle evidence and an accessible
  inspector for exact from, to, duration, start, center, and end timing plus typed visual parameters.
  One selected caption mounts the caption editor, while import and export remain visible without a
  selection. Exact timing edits continue through the existing trim, razor, nudge, ripple, and
  overwrite gestures instead of a second timing owner.
  The component submits only through its injected application callback and takes no transport,
  project-history, timeline, or graph mutation ownership.
- `app/src/native-viewport.tsx`: Reserves role-addressed native output rectangles and publishes only
  role, analysis code, geometry, scale, visibility, connection-local external display selection,
  and returned status to the shell-local viewport command. It never constructs an encoded image,
  blob URL, pixel readback, or webview frame path. A separate
  strict control-only command selects one exact active monitor and sRGB or Display P3 transform per
  role, then replaces the frozen native color snapshot without moving ICC bytes or frame data
  through React. Its composed
  viewer shell consumes one frozen local contract for fit, bounded zoom, directional pan, exact
  1:1 pixel intent, browser-synchronized fullscreen, cinema layout, and role-addressed external
  display intent without mutating playback or editorial feedback. A
  separate frozen local overlay contract renders pointer-transparent safe-area, guide, grid, ruler,
  center, aspect, and custom geometry above the same transformed frame while preserving navigation
  and excluding timecode and status ownership. A third frozen local catalog selects image, alpha,
  red, green, blue, luminance, false-color, or clipping presentation, republishes the current code
  through one resize and visibility observer, and reports requested, native-selected, and
  last-presented state without moving pixel math into React. A fourth frozen local comparison contract captures
  only exact native surface, frame, extent, display-intent, and optional rational navigation context,
  gates modes on available captures, and renders accessible controls plus pointer-transparent mode,
  identity, orientation, and divider evidence through the same transform. It never retains or
  fabricates GPU textures, comparison pixels, decoded media, or project state. The program viewer
  publishes only its deterministic formatted comparison summary through an optional callback. A
  memoized presentation-only viewer-status
  projection consumes the existing application snapshot and retained source monitor, then renders
  exact role-aware temporal, source, drop, playback, frame-cache, visual, audio, audio-cache,
  comparison, and editorial evidence outside the transformed frame so every navigation, overlay,
  comparison, analysis, and external-display mode preserves it. The Program role additionally
  projects every exact built-in transform reachable
  from the one current selected clip, renders complete matrix and sampling state outside the
  transformed frame, keeps driver-owned values inspectable and read-only, and submits Apply or
  identity reset as one typed graph action through the existing application command owner. Authored
  matrices never become shell-local CSS transforms. A separate frozen external display contract
  drives one accessible target selector, clears stale hotplug identities, and reports exact
  physical extent, scale, selected and presented
  analysis view, display intent, surface generation, frame sequence, and failure state without
  resetting navigation, overlays, comparison captures, analysis, or temporal context. The same
  viewer renders exact profile identity,
  catalog generation, scene contract,
  precision, transform order, display intent, selected monitor, and refresh failures without
  changing its analysis, navigation, playback, overlay, comparison, status, or editorial owners. Its
  composed `SourceMonitor` owns shell-local media-library and monitor state, exposes load, exact seek, mark,
  clear, refresh, and unload controls, publishes every replacement monitor snapshot to its caller,
  refreshes after project revision changes, and labels the retained source session as separate from
  decode and native GPU presentation. Optional editorial consequence strips render outside the
  native child placement host, expose the current feedback phase, retain complete multicam angle
  and switch detail, and never enter the placement payload. `EditorialAudioMeters` renders
  canonical route, audibility, sample clock, destination, and seam evidence, marks routes
  unavailable when their canvas track is absent, and explicitly labels live signal level
  unobserved. Auxiliary webviews stop before native viewport command ownership and render an exact
  primary-window ownership notice, preserving shared engine and workspace state without creating a
  second role-addressed surface set.
- `app/src/App.tsx`: Registers the five professional workspace routes and panels above the delivered
  application framework while retaining the system shell, shared selection, lifecycle controls,
  generated validation request, engine-introspection state, and the production project lifecycle
  consumer for create, open, close, save, save-as, recent, recovery, compact project-settings
  editing, native media picking, recursive folder selection, native drag/drop import, hierarchical
  bins, smart collections, list and grid browsing, deterministic thumbnail fallback, read-only
  source metadata inspection, bounded generic user metadata editing, typed clip annotations, and
  read-only current-project usage indicators. The same detail shows exact duplicate identity and
  edits reusable source-time selections plus fixed-point tracked observations. It also creates or
  replaces proxy and optimized-media attachment records, shows status, and switches explicit
  quality intent with transparent original-source fallback. It derives local availability and owns
  relink, intentional source replacement, and conform controls, then consumes revision-fenced native
  content search with ranked signal evidence. Its structured language-analysis editor preserves
  exact frame timing, rational rate, speaker, timeline plus clip relationships, and local AI labels,
  terms, and transcript links as ordinary state, including explicit stale-source confirmation. The
  same production media browser owns presentation-only multi-selection and invokes one native
  revision-fenced batch for numbered
  rename, active-bin organization, generating optimized transcode or proxy records, root-based
  relink, and metadata upsert or removal, then replaces its view from returned commit evidence. The
  selected-media path also requests
  one freshness-fenced generated bundle, rejects late identity mismatches, replaces only the
  selected card thumbnail, and displays bounded preview, canonical filmstrip, waveform, exact
  sample-range, sample-rate, frame-count, ordered-channel, and unavailable-state evidence. The same
  browser checks every source through metadata-efficient monitoring or forces exact bytes for one
  selected identity, then displays scan generation, relink intent, path state, volume identity and
  kind, mount state, accepted fingerprint, current observation, size, and actionable detail. The
  root shell hydrates its restored workspace route from the native window session before publishing
  subsequent route changes, mounts the real `WindowSessionPanel` in System, and listens for exact
  native replacement events without taking persistence or placement ownership. The always-mounted
  native-shell consumer restores registry-reconciled dock placement, tab order, sizes, visibility,
  active tabs, and focus while preserving the window session's per-webview route, executes typed native menu intents, uses native project
  and media dialogs, separates project from media drops, routes undo and redo through the existing
  project command owner, and resolves project or application close only after busy-state checks,
  durable save, and any required session-history warning for either undo or redo state. Main-window
  state alone projects the process-wide menu, while each native command is delivered only to the
  focused editor webview. The same consumer registers the project association listener before its
  initial snapshot read, rejects stale replacement revisions, updates only project presentation
  after an operating-system open, and preserves route, panel, selection, workspace, and other
  transient application state. It publishes the primary webview's complete route-layout,
  hidden-panel, active-tab, size, and focused-panel continuity only after native window restoration
  hydrates its route, plus
  active-project continuity, into the shell-local crash owner. The workspace header distinguishes
  default and custom structures, reports restoring, saving, saved, or failed native continuity for
  the primary window and explicit session-only state for auxiliary windows,
  resets every route to registry defaults, and exposes one exact undo until later workspace intent
  supersedes it. A separate bounded header poll projects the authoritative native engine lifecycle
  and opens the existing System route for detailed control or recovery without creating another
  lifecycle owner. The root shell also projects that lifecycle, public editor failures, project and
  window continuity, desktop shell failures, public export jobs, workspace saves, and retained
  cross-session crash diagnostics through the shared application-presentation provider. The global
  status and notification center preserve actionable source, code, safe context, last-valid
  resource, continuity, and distinct retryable, degraded, user-correctable, or terminal intent;
  their actions reuse existing refresh, project, System, and lifecycle owners. The retained crash
  surface continues to render classified diagnostics with exact retry, degraded, user-correction, restart, project-recovery,
  workspace-restoration, and dismissal actions. Those actions reuse the existing application,
  lifecycle, and project owners rather than mutating their state inside the diagnostic view. Its
  System panel also consumes the strict four-domain capability adapter, exposes live and retained
  GPU, audio, codec, and AI observations plus actionable failures, and refreshes without changing
  routes, streams, devices, projects, workspaces, codec sessions, or model state.
  The System panel also polls the shell-local process snapshot and renders every retained service
  phase, active and owned unit count, join obligation, and exact thread name without becoming a job
  owner. The root shell additionally composes every registered route, panel, and selection command
  with current file, recent-project, history, import, and quit intents in one transient searchable
  palette. Visible panel toggles, native workspace events, the fixed global opener, and palette
  execution use the same stable typed identities while project mutation remains with its existing
  owner. The palette projects the same effective bindings as global command dispatch, and palette
  quit enters the existing native close request before the one-shot resolution, so it cannot bypass
  save, busy, or history handling. The root also registers the configurable Shortcuts panel,
  hydrates its profile before shell
  synchronization, persists profile changes beside workspace presentation, and derives sidebar
  hints from the same effective binding used by global command dispatch.
- `app/src/desktop-shell.ts`: Reuses the application workspace presentation contract in the strict
  native shell snapshot and intent bridge, carries the versioned keyboard-shortcut profile through
  serialized sequence-fenced synchronization, close request resolution, typed event listening,
  stable automation identity derivation including the palette opener and encoded recent paths, and
  deterministic
  project-versus-media drop partitioning, safe-close decisions, and document titles that expose only
  the basename and durable revision.
- `app/src/crash-diagnostics.ts`: Defines the strict shell-local crash snapshot, complete route and
  panel-layout intent,
  project continuity, four recoverability classes, and five recovery-entry DTOs plus typed wrappers
  for snapshot, continuity update, and dismissal commands. Separate process-lifetime promise tails
  preserve call order for workspace and project updates even when blocking native writes complete at
  different speeds. It intentionally exposes no private panic detail and imports no generated public
  API contract.
- `app/src/lifecycle.ts`: Defines the exact shell-local serialized lifecycle contract, the separate
  process phase and seven-service ownership snapshot, and typed asynchronous wrappers for lifecycle
  and process Tauri commands without importing engine bindings.
- `app/src/project-lifecycle.ts`: Defines strict shell-local project lifecycle, settings, media
  import, media-library snapshot, derived-thumbnail, bin, smart-collection, source-inspection,
  user-metadata, editorial-annotation, derived-usage, duplicate-identity, selection, and tracked
  region DTOs plus derived-media purpose, quality, lifecycle, attachment, selection-evidence DTOs
  and typed wrappers for the lifecycle and media-library Tauri commands. It also mirrors offline
  recovery plus persisted content-analysis artifacts and exposes revision-fenced native replacement
  and content-search wrappers with exact explainable match evidence. The same bridge exposes exact
  C010 thumbnail, filmstrip, waveform, and preview product DTOs plus C012 batch operation and result
  DTOs. It also mirrors the complete C013 source-monitoring, fingerprint, volume, path, request, and
  relink-intent records plus the revision-fenced scan wrapper. Local fallback search includes names,
  paths, source facts, built-in and user metadata, annotations, offline state, source-monitoring
  state, relink intent, and volume evidence without introducing another persisted search index.
  C014 adds exact rational source time, stream, engine-state, fingerprint-bound mark, replacement
  snapshot, and atomic mark-result DTOs plus five optimistic Tauri wrappers. Snapshot reads and
  lifecycle mutations publish replacement project presentation to always-mounted shell subscribers,
  and the same adapter exposes the exact typed `superi://project-opened` replacement event without
  adding project authority to React.
- `app/src/system-capabilities.ts`: Defines and deeply validates the exact schema-1 desktop
  capability snapshot, including condition, freshness, failures, GPU adapter facts, audio stream
  ranges and explicit channel-layout knowledge, codec declarations, AI availability, and retained
  cache state. It exposes one injectable read-only discovery command and rejects unknown, malformed,
  unsafe, or future native data before React receives an immutable replacement.
- `app/src/main.tsx`: Activates the static application theme contract before constructing one
  process-lifetime `DesktopSuperiTransport`, injects that transport through the generated API
  provider, disposes it at unload, and mounts the React application under strict mode.
- `app/src/styles.css`: Defines the responsive, accessible application frame, route rail, four-dock
  grid, tab strips, mounted panel surfaces, resize separators, collapsed empty docks, mobile dock
  stacking, responsive layout status, reset and undo controls, explicit engine lifecycle states,
  complete configurable route-shortcut hints with narrow-window scrolling,
  professional workspace data views, exact audio route and continuity presentation,
  shared selection, lifecycle controls, process-service detail, media-browser list and grid layouts,
  thumbnail fallbacks,
  source and user metadata details, editorial annotation controls, usage summaries, engine API
  status presentation, ranked content-search evidence, structured language and local-content
  artifact editing, stale-analysis warnings, bounded preview raster, horizontally scrollable
  filmstrip, channel-separated waveform, responsive multi-selection batch controls, the sticky
  timeline grid, ruler, tracks, accessible clip buttons, layered filmstrip and waveform visuals,
  dense state badges, range, playhead, controls, interaction selection, authored selection
  evidence, lasso, visible focus, live status, snap rule strip, exact target status and guide,
  transition handle marks, timing and alignment forms, typed effect parameter controls, marker list
  and editor controls, and
  semantic theme-token adoption for shared shell chrome and responsive 16:9 native viewer
  reservations plus bounded navigation controls, clipped transforms,
  pixel-mode fidelity intent, fullscreen, cinema presentation, compact comparison capture and mode
  controls, transformed exact-identity badges, bounded split or wipe controls, difference intent,
  pointer-transparent dividers beneath the existing overlay layer, and compact scrollable Program
  transform cards with a complete 3 by 3 matrix, sampling, driver state, action feedback, and
  responsive controls. It also defines shared tooltip placement, viewport-bounded context menus,
  bounded notification toasts and history, the fixed operational status bar, semantic progress,
  and visually distinct retryable, degraded, user-correctable, and terminal failure cards. Native
  viewer frames preserve exact surround, overlay, comparison, and authored marker color-data
  meaning, disable forced-color substitution within the color-critical frame, retain full opacity
  and normal blending, and apply no CSS filter. Timeline rows use canonical variable height, compact
  two-row track controls,
  disabled output presentation, and visible command failures. It also defines compact ready, stale,
  and empty source-monitor
  controls with exact state details and responsive action groups. The timeline edit console adds
  compact source-placement and exact timing-tool controls, history, target, consequence, affected
  object, shortcut, pending, and result states. The retime
  panel adds responsive exact-target fields, rational segment inputs, curve state, validation,
  consequences, and apply, reset, and undo actions. Source and program consequence strips,
  responsive multicam angle and switch grids, and route-state audio meter cards keep exact evidence
  readable without presenting structural bars as amplitude.
- `app/src/transport.ts`: Implements the concrete generated `SuperiTransport` through one injected
  or Tauri-backed invoke/listen host, generation-scoped request identities, ordered event replay,
  stale and duplicate rejection, reconnect, cooperative cancellation, and exact
  `SuperiTransportError` projection with actionable public context.
- `app/tests/app-contract.test.mjs`: Verifies exact production pins, lifecycle and engine-process
  ownership seams, application framework composition, transport isolation, production workflow
  routing, the hashed React bundle, exact `.superi` association metadata, startup and macOS native
  routing, retained blocking-task dispatch, lifecycle Open reuse, typed React event consumption, focused
  webview menu targeting, primary-only shell synchronization, safe-close composition with the
  persistent window owner, shell-local crash-journal ownership, all four recoverability classes,
  private panic-detail exclusion, mounted dock workspace, accessible tab, hide, drag, dock, and
  separator behavior, default and custom layout recovery controls, always-visible authoritative
  engine status routing, presentation-only ownership, explicit process-service command registration,
  retained exit and association handles, join-all cleanup, child-process exclusion, native command
  registration, and reuse of the
  existing lifecycle and project recovery consumers. It freezes the fixed theme declaration,
  pre-transport activation, semantic and color-data token ownership, native viewer isolation, and
  exclusion of theme code from storage, generated API, Tauri, and project mutation. It freezes the
  command palette's pure catalog,
  accessible modal, native menu intent, production registration, and exclusion from generated API
  and Tauri ownership. It also freezes the shared application
  presentation provider, real tooltip, context-menu, status, notification, progress, and classified
  failure consumers, retained crash evidence, panel keyboard invocation, and pure-model transport
  isolation. It also freezes the authoritative four-provider
  capability composition, strict frontend adapter, real System-panel consumer, and absence of audio
  stream mutation.
- `app/tests/theme.test.ts`: Proves the frozen theme identity, deterministic declaration repair,
  ready and recovered evidence, static pre-JavaScript document declaration, pre-transport
  activation, semantic and color-data token namespaces, complete command-palette token adoption,
  and native viewer isolation from CSS color transforms.
- `app/tests/system-capabilities.test.ts`: Proves exact command invocation, strict all-field parsing,
  immutable output, preservation of audio sample and channel meaning, rejection of unknown and
  malformed data, and actionable failure presentation.
- `app/tests/native-viewport-ipc-contract.test.mjs`: Freezes the shell-local viewport command as a
  placement, analysis, and display-selection control-only Tauri payload, verifies both React
  invocations use that command, and freezes the distinct color command as a control-only selection
  payload with both native registrations. It excludes webview image conversion, ICC bytes, and
  pixel-readback mechanisms from either consumer. It also proves viewer and meter feedback remain
  outside the placement payload,
  selected versus presented diagnostics remain explicit, external windows use the destination-aware
  native path, and an external presentation failure does not terminate inline GPU presentation.
- `app/tests/application-framework.test.ts`: Verifies duplicate and reference validation, immutable
  routing, canonical dock defaults, cross-dock ordered tabs, bounded resize, active-tab focus,
  retained hidden placement, deeply frozen layout state, complete workspace projection, exact
  public-resource selection, structural default and custom detection, all-route registry reset,
  exact deeply frozen one-step undo, superseding-intent invalidation, local-first asynchronous API
  command delegation, shortcut normalization, editable-target safety, frozen discoverability
  metadata, explicit availability reasons, transient
  palette state, and persisted workspace reconciliation against removed routes or panels.
- `app/tests/command-palette.test.ts`: Proves duplicate rejection, deep descriptor freezing, stable
  recent-path automation identities, token-complete deterministic search, current disabled reasons,
  effective configured shortcut projection, typed application and desktop invocation, and actionable
  failure retention.
- `app/tests/application-presentation.test.ts`: Proves exact and distinct policy for all four
  recovery classes, preservation of reviewed transport, project, crash, lifecycle, and last-valid
  context, fail-closed unknown classes, immutable bounded and deduplicated notification history,
  truthful determinate and indeterminate export progress, operational-status priority, and
  viewport-bounded context-menu geometry.
- `app/tests/desktop-shell.test.ts`: Proves deterministic project-versus-media drop classification,
  busy and history-aware close decisions, path-safe document titles for POSIX and Windows paths,
  complete layout and keyboard-shortcut profile transport, and sequence resumption after a webview
  reload, plus stable automation identities for palette, workspace, recent, and close paths.
- `app/tests/keyboard-shortcuts.test.ts`: Proves deterministic schema-1 profile resolution and
  export, transactional collision and native-reservation rejection, inactive-command retention,
  canonical Unicode and platform-aware event capture, IME exclusion, unbinding, and reset behavior.
- `app/tests/editor-workspaces.test.ts`: Verifies exactly five registry-backed professional routes,
  one existing application/project owner, exact source, program, composite, and color viewer
  consumers including the composed source monitor, explicit public editor request identity,
  state-free workspace projection, and immutable preservation of sample timing, ordered channels,
  routes, continuity evidence, one application-owned project transaction path, all twelve track
  operation tags, all six marker operation tags, typed reversal, source snapshot and generated
  command wiring, and the absence of direct transport ownership in the workspace. It also verifies
  transition command wiring through the same application-owned callback plus application-owned
  editorial feedback publication, viewer consumption, audio meter rendering, route-state styling,
  native IPC isolation, accessible analysis selection, selected-versus-presented status, semantic
  public export progress, and visible export failure category plus recoverability. It also freezes the
  complete fit, zoom, pan, 1:1, fullscreen, cinema, and external-display-intent viewer control
  consumer while excluding C002 playback navigation ownership. It freezes all eleven viewer-status
  fields, application-owned projection, transformed-frame separation, exact role coverage, and
  absence of direct status transport or mutation authority. It also freezes the
  editing playback consumer, application-owned generated route, complete required action set,
  exact state categories, and absence of direct Tauri or API-client ownership in the component. It
  also freezes comparison catalog consumption, exact source and playback temporal wiring, program
  summary publication through the existing application owner, live playback summary consumption,
  comparison presentation styles, and continued state-free workspace projection.
  It also proves that the Program viewer consumes shared selection, exact ordinary transform state,
  and the application-owned revision-fenced action executor while keeping authored matrices out of
  shell-local navigation styling.
- `app/tests/viewer-color-management.test.ts`: Proves strict bounded catalog validation, exact ICC
  profile and unprofiled evidence, immutable monitor and transform selection, canonical transform
  order and scene contract, deterministic diagnostics, stale or malformed native rejection, and
  input nonmutation.
- `app/tests/playback-transport.test.ts`: Proves exact signed JKL rate cycling, K pause, Space
  play-or-pause intent, half-open exact seek and scrub coordinate mapping, rational time and rate
  formatting, and explicit unavailable-output labels.
- `app/tests/viewer-navigation.test.ts`: Proves immutable bounded navigation, exact fit and pixel
  resets, deterministic pan, role-addressed display intent, and exclusive fullscreen and cinema
  presentation without changing navigation state.
- `app/tests/viewer-overlays.test.ts`: Proves the complete frozen overlay catalog, deterministic
  custom geometry, immutable visibility changes, ordered projection, and absence of navigation,
  playback, timecode, and status state.
- `app/tests/viewer-comparison.test.ts`: Proves the complete seven-mode catalog, immutable native
  frame and separate rational navigation-context captures, missing-frame and missing-reference
  gating, bounded divider position, orientation, snapshot identity, and deterministic exact current
  and captured summaries.
- `app/tests/viewer-status.test.ts`: Proves core-compatible drop-frame and non-drop timecode labels,
  continuous display frames, physical-drop separation, exact active source and ranges, grouping,
  linking, targeting, synchronization, selection, playback, visual, audio, comparison,
  source-monitor identity, explicit unavailable owners, frozen output, and input nonmutation. It
  additionally proves nonblocking frame scheduling, predictive and decoded-output degradation,
  exact audio discard acknowledgement, sample clocks, ordered source and destination channels,
  explicit channel or mute routing, structural continuity seams, timing-only output honesty,
  malformed-evidence rejection, and preserved exact status through paused, playing, scrubbing, and
  ended modes.
- `app/tests/viewer-analysis.test.ts`: Proves exact ordered view codes and labels, complete catalog
  freezing, the image default, deterministic lookup, and fail-closed unknown selection.
- `app/tests/viewer-transform-controls.test.ts`: Proves exact current selected-clip resolution,
  canonical graph and transform order, all nine scalar values, nearest and bilinear sampling,
  immutable projection, ordered changed-only typed mutations, identity reset, no-op and nonfinite
  rejection, driver locks, stale selection, absent transform, malformed scalar failure, and input
  nonmutation.
- `app/tests/viewer-external-display.test.ts`: Proves immutable exact target selection, stale target
  reconciliation, unavailable-target rejection, inactive output, and exact external target,
  physical extent, scale, analysis view, surface generation, frame sequence, and display-intent
  formatting.
- `app/tests/timeline-workspace.test.ts`: Verifies strict canonical revision handling, exact track,
  item, source and record range, group, link, selection, target, lock, output, synchronization, and
  transition preservation, variable height, external global-start placement, deterministic
  frame-aligned ruler, visible-window, and range math, exact timeline, track, object, item, and
  playhead snap targets, inexact-clock and object-overscan omission, stable tie ordering, per-kind
  filtering, persistent and session disablement, invalid-document and marker-owner failures, all
  thirteen durable track gestures, complete visible marker semantics, exact marker navigation,
  non-navigable state, visible consequence and reversal wiring, selection identity round
  trips, fixed-point related and direct selection, display-order ranges, directional neighbors,
  lasso geometry, real React integration, multiselect accessibility, shared selection wiring,
  transient navigation controls, all nine generated edit requests, minimal fragments, exact
  inclusive source-mark conversion, all four three-point placements, equal-duration four-point
  execution, explicit fit-to-fill rejection, selection override, replace conformance, history
  requests, visible consequences, failure fences, exact transition offsets, production
  transition-inspector wiring, nested catalog and path consumption, visible open, placement, and
  compound controls, double-click nesting, publication through the shared callback, and the absence
  of a second frontend authored mutation owner. It also freezes accessible multicam panel mounting,
  angle-state viewing, creation, synchronization, switching, exact cut refinement, and history undo
  wiring. It also proves strict complete audio routing projection, exact audio-video selection by
  immutable track identity, link, synchronize, detach, and routing action payloads,
  incomplete-route rejection, visible production controls, complete caption projection, caption
  panel integration, publication through the shared callback, and the absence of a second frontend
  authored mutation owner.
- `app/tests/timeline-captions.test.ts`: Proves SRT import produces one canonical millisecond track
  with explicit gaps and authored language and subtitles purpose, strict style fields become the canonical API representation, supported
  WebVTT voice, escaped cue text, and presentation settings round trip deterministically,
  unsupported markup fails closed, and fresh transcript text, deterministic nearest-millisecond
  timing, speaker, and timeline relationships become ordinary editable cues while stale revision or
  fingerprint evidence fails before action construction.
- `app/tests/timeline-nesting.test.ts`: Verifies exact mixed-clock duration projection, direct child
  dependencies, cycle-safe candidates, root-anchored path opening and stale-path reconciliation,
  append and equal-duration replace action payloads, strict locked and incompatible rejection, and
  deterministic compound object and affected-track identity ordering, with complete valid stereo
  routing retained by canonical fixtures.
- `app/tests/timeline-multicam.test.ts`: Verifies strict setup and authored-state projection,
  source-track angle derivation, active-angle and source-availability resolution at the exact
  playhead, unavailable-take rejection, generated atomic creation, live switch, frame cut-move,
  sync, audio, and detach actions.
- `app/tests/timeline-editing.test.ts`: Verifies the complete timing-tool catalog, exact 24 fps,
  24000/1001, 48 kHz, and inexact 44.1 kHz behavior, typed fragment and gap identities, canonical
  sync-track ordering, direct and ripple extend modes, ripple-delete and gap batches, lower-matched
  affected-object previews, deep immutability, lock rejection before allocation, and publication
  through the shared project action executor without direct transport ownership.
- `app/tests/timeline-clip-presentation.test.ts`: Verifies supplemental reuse of the exact canvas
  projection, external global-start placement, mixed record clocks, source names and relink state,
  retime, linking, grouping, targeting, synchronization, markers, metadata, complete multicam angle,
  switch, and audio-policy intent plus missing-angle rejection,
  clip-scoped effects and drivers, exact clip-gain sample keys and positions, deep immutability,
  malformed-state rejection, complete canonical stereo routing, exact child-timeline detail scoping, real preview-command composition,
  stale freshness rejection, and application-owned selection integration.
- `app/tests/timeline-editorial-feedback.test.ts`: Verifies distinct exact trim, slip, and slide
  viewer consequences, multicam angle, switch, and fixed-audio fidelity, 48 kHz and 96 kHz sample
  clocks, ordered channel and destination meaning, route and solo behavior, exact gap and source
  discontinuity seams, deep immutability, explicit unobserved signal telemetry, and missing clip
  behavior.
- `app/tests/timeline-transition-presentation.test.ts`: Verifies exact handles and opposite-edge
  limits, duration and alignment derivation, strict no-op and unsafe input rejection, canonical
  scalar-bit decoding, downstream graph traversal, host and driver restrictions, immutable output,
  exact public timing and graph command payloads, and timing survival under malformed graph detail.
- `app/tests/timeline-retime.test.ts`: Verifies exact mode drafts, continuous BigInt seam derivation,
  curve point insertion and removal, generated target, revision, operation, and request shape,
  visible source traversal, no-op suppression, unsafe integer, wrong-duration, and inexact-clock
  failures, plus production mounting, accessibility, shared pending and history ownership, styling,
  and exclusion of direct Tauri mutation.
- `app/tests/api-bindings.test.mjs`: Verifies the canonical generated re-export, complete typed map
  boundary including nested placement, compound, and multicam DTO, action, operation, and result discriminants, concrete
  provider/bootstrap injection, and real request/subscription forwarding without duplicating
  generated client policy.
- `app/tests/transport.test.mjs`: Verifies the one native dispatcher call, generated request
  identity, ordered replay, stale and duplicate event rejection, reconnect cursor, abort-driven
  cancellation, and exact classified public error preservation through an injected headless host.
- `app/tests/window-session.test.ts`: Proves strict native replacement validation, exact command
  names and payloads, listener cleanup, route hydration and update wiring, real System panel
  mounting, complete user-facing action coverage, and the auxiliary-window exclusion from native
  GPU viewport ownership.
- `app/tsconfig.json`: Enables strict no-emit TypeScript, isolated modules, and bundler resolution.
- `app/vite.config.ts`: Configures the React Vite build and fixed Tauri development port.
- `app/src-tauri/Cargo.lock`: Locks the standalone desktop host together with its path dependencies
  on the public API, engine, shared concurrency, core, image, media-I/O, and Rust codec contracts,
  plus the pinned PNG decoder and data-URL encoder.
- `app/src-tauri/src/viewport.rs`: Owns four role-addressed native child windows plus four hidden
  borderless top-level external windows, checked CSS-to-physical placement, strict analysis and
  connection-local display selection, deterministic Tauri monitor projection, current-editor
  display exclusion, target conflict rejection, exact inline and external selected and
  last-presented status, per-role color binding, and explicit idempotent join-before-host-drop
  lifetime. One dedicated
  GPU submission thread presents both destination kinds from the same managed canonical RGBA16F
  role results through explicit ACEScg-to-sRGB or ACEScg-to-Display-P3 transforms and the direct
  analysis-aware presenter. It owns exact macOS system profile discovery, atomic catalog refresh,
  and immutable freshness-checked monitor bindings before acquire and submit. Unsupported external
  surface presentation records an explicit external failure and continues inline output. The Tauri
  placement DTO and separate color-selection DTO both deny unknown fields, so frame payloads, image
  data, ICC bytes, and texture handles cannot enter either native presenter destination.
- `app/src-tauri/tests/viewer_presentation_contract.rs`: Freezes source, program, composite, and
  color role order plus canonical precision, alpha, scene meaning, terminal display meaning,
  both terminal display meanings, exact transform order and IDs, deterministic intent, arbitrary
  8K extent, invalid zero-extent behavior, reversible exact-monitor selection, stale binding
  rejection after catalog change, absent-selection projection, and the
  exhaustive shell-to-color mapping and linear-light stage for all eight analysis codes. It also
  freezes inline and external as the only two destinations on the shared native presentation owner.
- `app/src-tauri/Cargo.toml`: Declares the `superi-desktop` library and binary, exact Tauri,
  serialization, image, and base64 pins, stable Rust edition, and downward-only lifecycle, engine,
  public API, GPU, audio, AI, media-I/O, image, and in-tree codec dependencies.
- `app/src-tauri/build.rs`: Runs the standard Tauri build integration.
- `app/src-tauri/rust-toolchain.toml`: Selects stable Rust with rustfmt and Clippy.
- `app/src-tauri/tauri.conf.json`: Declares the Superi identity, production frontend, bounded main
  window hidden until persistent restoration applies its safe placement, disabled packaging,
  exact `.superi` editor and owner association metadata with exported `com.superi.project` database
  type and MIME identity, and explicit cross-platform desktop icons.
- `app/src-tauri/icons/app-icon.svg`: Retains the editable source for the initial application icon.
- `app/src-tauri/icons/32x32.png`: Supplies the small desktop PNG icon.
- `app/src-tauri/icons/128x128.png`: Supplies the standard desktop PNG icon.
- `app/src-tauri/icons/128x128@2x.png`: Supplies the high-density desktop PNG icon.
- `app/src-tauri/icons/icon.png`: Supplies Tauri's default Unix desktop icon.
- `app/src-tauri/icons/icon.ico`: Supplies the Windows desktop icon bundle.
- `app/src-tauri/icons/icon.icns`: Supplies the macOS desktop icon bundle.
- `app/src-tauri/src/desktop_shell.rs`: Owns bounded native menu and title projection, stable typed
  menu intents including the focused `Find Command...` action, frontend sequence fencing, private
  schema 3 presentation persistence with schema 1 and 2 migration, strict route, dock,
  ordered-panel, active-tab, size, hidden-state, and focus validation, bounded schema-1
  keyboard-shortcut validation,
  recent-project menu mapping, native clipboard roles, and duplicate-suppressed one-shot window or
  quit resolution into orderly application shutdown. It carries document identity, history depth,
  busy state, layout, and shortcut preferences only as presentation, emits no authored mutation,
  and never serializes project history or project bytes.
- `app/src-tauri/src/crash_diagnostics.rs`: Owns one application-data crash-diagnostics directory,
  exact active-session marker, and 32-record replacement-safe diagnostic journal. It detects an unclean prior
  session, chains a panic hook that retains private panic detail only in the native journal, observes
  classified lifecycle failures, validates bounded workspace, complete dock layout, and active-project continuity, and
  projects only reviewed safe context plus retry, degraded, user-correction, or terminal recovery
  entry points. Corrupt storage is archived and storage failure degrades to an in-memory diagnostic
  without preventing application startup.
- `app/src-tauri/src/capabilities.rs`: Owns bounded shell-level discovery and retained advisory
  cache state for the current GPU adapters, audio input and output declarations, engine codec
  registry, and local AI runtime declaration. It projects authoritative lower owners without
  creating a GPU device, audio stream, codec session, AI pipeline, project mutation, or workspace
  mutation, performs discovery on a blocking worker, retains per-domain fallback with current
  failures, and atomically publishes only a strict validated schema-1 private snapshot.
- `app/src-tauri/src/lifecycle.rs`: Owns explicit application intent, serialized application and
  engine phases, generation changes, classified safe failures, recovery, restart, shutdown, exact
  acknowledgement tokens, retry-visible failed shutdown, and the stable headless-engine participant seam.
- `app/src-tauri/src/process_runtime.rs`: Owns the shell-local desktop process phase, a stable ordered
  inventory for application exit, project association tasks, EngineControl, Playback, background
  workers, GPU submission, and window persistence, plus retained exit and bounded Tauri blocking-task
  handles. An admission handshake records each task handle and active count before execution can
  finish. It closes admission before shutdown, catches task panics into safe failure state, joins
  every retained handle even after an earlier failure, and publishes only operational ownership.
- `app/src-tauri/src/engine.rs`: Owns one lifecycle-attached `EngineCommandDispatcher` on a dedicated
  EngineControl thread, one dedicated Playback thread, one shared bounded worker pool, a
  fixed-capacity nonblocking request connection, and projection through the existing
  integration-validation API contract. The playback thread retains the timing-only production
  runtime and one capacity-bounded control connection; it attaches the active project's exact root
  range, reconfigures that range before durable authored replacement, advances transport, and joins
  before the shared pool shuts down. Startup reports and cleans partial ownership, and shutdown
  attempts both domain joins plus the complete worker-pool join before returning the first failure.
  The EngineControl thread retains one route-fenced
  `ProjectEditorApi` session for generated editor-state and project-command requests, persists every
  successful selected snapshot through `ProjectDatabase`, preserves session undo and redo, and
  invalidates any in-memory state that could not be published durably. Its editor API and playback
  runtime share only the existing bounded dispatcher bridge.
- `app/src-tauri/src/file_associations.rs`: Owns shell-local operating-system project association
  ingress. It filters case-insensitive `.superi` startup arguments and local file URLs, resolves
  relative arguments, canonicalizes existing paths, deduplicates first-seen inputs, runs every open
  through a process-retained bounded Tauri blocking task and `DesktopProjectState::execute(Open)`, emits a complete
  replacement snapshot or retained actionable failure, and restores focus without creating a
  second document or project owner.
- `app/src-tauri/src/window_session.rs`: Owns the native application window session above Tauri.
  Its strict schema-versioned JSON contains exactly one primary window, at most seven auxiliary
  workspace windows, stable labels, routes, normal placement, fullscreen and monitor intent,
  previous placement, focus order, and a bounded recently closed stack. Startup loads before setup,
  preserves malformed input under a rejected filename, restores a safe primary with visible
  recovery evidence, reconciles placements against the current monitor catalog, creates and
  restores auxiliary webviews, and publishes one immutable snapshot event. A capacity-one worker
  coalesces changes and atomically replaces the session file off the event loop. The shared process
  registry reports its named thread, and explicit idempotent shutdown returns signal or join failure.
  Exact commands
  expose snapshot, create, focus, fullscreen, monitor movement, placement undo, route update, close,
  and reopen. It exposes the focused or last-active editor webview as the process-wide native menu
  target, captures primary placement without authorizing closure, and leaves primary close to the
  desktop shell's project-preserving handshake before orderly lifecycle shutdown.
- `app/src-tauri/src/transport.rs`: Owns the thin bounded desktop command dispatcher above the
  managed `EngineConnection`, per-webview generation-scoped request and cancellation state, exact
  public error conversion with reviewed canonical contexts, and a 64-record globally ordered
  replacement-event replay window without engine semantics. It routes integration validation, complete editor state, and
  generic project commands plus immediately accepted playback transport through exact generated request and response types on the managed
  EngineControl connection, publishes every correlated project event in order, and advances desktop
  project identity only after durable command completion. Cancellation wins before durable work
  starts, while a completed durable commit and its replacement event win a late cancellation so
  committed state cannot be hidden from reconciliation. The same late-cancellation rule preserves
  an already accepted playback command. Each connected webview receives the same authored sequence
  in its own current generation, and one webview disconnect cannot cancel or reset another. Focused
  routing proof observes playback completion through editor state before a durable edit.
- `app/src-tauri/src/project_lifecycle.rs`: Owns the sole serialized desktop project session above
  `LocalProjectHost`, including commit-only active identity, bounded deduplicated recent projects,
  revision-fenced recovery presentation, last-valid state retention, and retryable, degraded,
  user-correctable, or terminal actionable failures. A private versioned atomic session record now
  retains only the active path and last-known recent records, validates the active document through
  the local host at launch, and degrades to no active document without discarding recents when that
  path cannot reopen. It republishes the record after lifecycle and editor identity transitions.
  Its editor lease retains the exact active
  path, project, revision, and root through one engine request, admits only same-or-next durable
  identity, permits a root change only with the next revision, synchronizes media presentation, and
  releases the route only after acceptance completes. It projects and atomically updates the
  project-owned settings snapshot, discovers bounded regular media without following symlinks,
  groups numbered still sequences deterministically at the project frame rate, and delegates one
  permission-checked import transaction without taking project persistence authority. A separate
  revision-fenced desktop presentation store follows the active project identity, refreshes items
  from imported media IDs and freshness, persists bin hierarchy and smart definitions atomically,
  regenerates transparent thumbnail and smart-membership derivations on read, caches bounded
  source-derived facts and availability by exact freshness, and persists bounded generic user
  metadata plus bounded typed clip names, labels, ratings, keywords, comments, and favorite intent
  without changing imported identity, source paths, or bin placement. Usage indicators are rebuilt
  from the current read-only project database by counting exact `ClipSource::Media` references and
  are omitted from persisted presentation state. C007 derives canonical and duplicate media IDs
  from exact content fingerprints, persists bounded rational-frame selections beside C006 state,
  and validates ordered fixed-point tracked observations before atomic replacement. C008 persists
  bounded replaceable proxy and optimized attachments beside that state, binds them to exact source
  freshness, and deterministically resolves explicit quality intent or original-source fallback.
  C009 derives online, partial, or offline source state and owns revision-fenced relink, intentional
  replacement, and exact conform changes. C010 resolves an immutable selected-media item under
  revision and fingerprint fences, then performs bounded still, sequence, or WAVE generation without
  persisting artifacts. C011 persists bounded model-independent content analysis
  against one source fingerprint, canonicalizes editable transcript text, exact rational-frame
  timing, speakers, timeline plus clip relationships, and linked local AI content, and atomically
  replaces it without changing other media state. Its read-only revision-fenced query composes
  imported, source, user, editorial, availability, transcript, relationship, and local-content
  fields, returns bounded weighted evidence with stable media-ID tie breaking, keeps stale analysis
  inspectable after source replacement, and requires no network or model runtime. C012 adds a
  bounded ordered batch that clones the snapshot,
  applies rename, organization, fixed-purpose derived-media, relink, and metadata operations, rejects
  the complete batch with its operation index on any failure, and advances one revision on success.
  C013 persists per-path accepted fingerprints, current observations, conservative volume identity,
  path status, scan generation, and relink intent beside the same stable item. Import creates exact
  baselines during its existing byte pass. Revision-fenced selected or all-media scans skip stable
  bytes by metadata signature unless exact verification is requested, preserve every editorial and
  derived field, reject changed sources from old-fingerprint preview generation, and never bind new
  bytes automatically.
  Stable presentation names survive reimport. Replacement responses serialize refreshed usage,
  identity, smart membership, thumbnail, offline, and resolved-representation state for the actual
  consumer, while sidecar publication removes those derived-only fields before atomic replacement
  and restores them from authoritative inputs on read. C014 adds backward-compatible durable exact
  in and out marks bound to source fingerprint, validates them with every complete library
  candidate, preserves them across stable reimport, owns one retained monitor runtime, and releases
  it whenever the active project identity or path changes. The same lifecycle now exposes one exact
  active editor route and accepts only the same or next durable editor revision when path, project,
  root, and expected prior revision still match, then synchronizes dependent media presentation
  state.
- `app/src-tauri/src/project_lifecycle/media_preview.rs`: Implements one bounded nonauthoritative
  preview generator over an immutable media-browser item. It decodes supported still containers,
  adapts explicit RGBA8 sRGB straight-alpha images into `superi-image`, selects at most six
  deterministic sequence frames, scales every raster through the lower thumbnail operation,
  encodes bounded PNG data URLs, opens WAVE through the media-I/O PCM source, decodes exact blocks
  through the in-tree PCM backend, and retains validated start sample, sample rate, frame count, and
  ordered channel labels beside a channel-separated waveform. Every product is independently ready
  or unavailable, and no artifact enters project, media-library, derived-media, playback, routing,
  or export state.
- `app/src-tauri/src/project_lifecycle/source_monitoring.rs`: Defines persisted source fingerprints,
  conservative system, removable, and unknown volume identity, mounted and offline state, exact path
  observations, overall monitoring status, and explicit relink intent. It hashes import bytes once,
  supports metadata-efficient or forced-exact scans, adopts legacy baselines only after aggregate
  fingerprint proof, retains accepted evidence across loss and change, and classifies filesystem
  failures without network, watcher, platform service, or automatic source replacement.
- `app/src-tauri/src/project_lifecycle/source_monitor.rs`: Owns exact rational monitor DTOs,
  optimistic project, library, and monitor revision fences, one retained boxed media source or
  verified image range, real engine source-only registry probe and open, exact seek, ready or stale
  reconciliation, fingerprint-bound mark candidate publication, and immediate unload. Source I/O
  has explicit interactive deadlines and runs only through blocking Tauri workers. Snapshots expose
  metadata and state only, never packets, decoded frames, audio blocks, textures, or presentation.
- `app/src-tauri/src/lib.rs`: Configures the mock and native Tauri builders, retains the linked
  engine process and one shared process runtime, manages its bounded connection and transport state alongside application
  lifecycle, project-session, native desktop-shell, persistent window-session, and
  crash-diagnostics and capability state, registers lifecycle, shell, project, window snapshot and
  mutation, crash continuity and dismissal, capability discovery, viewport placement, viewport
  color selection, and API commands,
  including media-library snapshot, organization mutation, source inspection, user metadata, and
  editorial annotation, C007 identity-selection, C008 derived-media mutation, C009 offline recovery,
  C010 generated-preview, C011 content-analysis mutation plus content-search, and C012 atomic batch
  commands, plus the C013 revision-fenced source-scan command and the C014 source monitor snapshot,
  load, seek, mark, and unload commands,
  initializes the crash-diagnostics, project recovery, native shell, window, and advisory capability
  persistence roots, installs the chained panic hook and native menu on the main-thread setup path, collects portable
  startup association arguments after process launch, routes macOS `RunEvent::Opened` file URLs,
  scopes API dispatch by invoking webview label, passes project state into every blocking generated
  request, targets every returned ordered Tauri event to its client webview, records window focus,
  move, resize, auxiliary close, and route state, intercepts primary close and direct
  operating-system exit through the same one-shot safe-close handshake, observes lifecycle failures,
  retains the application-exit monitor, closes association-task admission, and joins the exit,
  association, window persistence, GPU submission, engine-domain, and worker-pool owners after the
  native application stops. Setup failure follows the same rollback path. It removes only the
  matching crash-session marker after every owner joined and lifecycle shutdown was acknowledged.
- `app/src-tauri/src/main.rs`: Starts the native production desktop host.
- `app/src-tauri/tests/engine_connection_contract.rs`: Proves dedicated EngineControl ownership,
  truthful public validation projection, bounded nonblocking admission, stable connection reuse
  across restart, fresh generation ownership, and orderly stop and join.
- `app/src-tauri/tests/crash_diagnostics_contract.rs`: Proves prior-session unclean-exit detection,
  exact route, dock placement, tab, size, hidden panel, focus, project, revision, and lifecycle
  continuity retention, layout-free marker compatibility, orderly marker removal,
  bounded classification across all four recoverability classes, matching recovery entry points,
  private panic-detail exclusion from the public snapshot, journal retention, corrupt-store archival,
  and degraded startup continuity.
- `app/src-tauri/tests/lifecycle_contract.rs`: Proves exact startup and shutdown acknowledgement,
  stale-token rejection, ordered restart, classified recovery, terminal failure, and blocking-safe
  change observation, including a failed shutdown whose explicit retry remains available.
- `app/src-tauri/tests/process_runtime_contract.rs`: Proves a real linked engine reports exact
  EngineControl, Playback, and bounded worker ownership, joins all three into stopped state, and
  keeps native GPU and persistence cleanup explicit and idempotent before initialization.
- `app/src-tauri/tests/file_associations_contract.rs`: Proves unique first-seen selection of
  case-insensitive `.superi` startup arguments and local file URLs, exclusion of unrelated and
  nonfile inputs, real durable project activation through the existing desktop owner, exact identity
  and recent state, and last-valid state plus structured failure retention after a missing file.
- `app/src-tauri/tests/project_lifecycle_contract.rs`: Proves create, open, close, save, save-as,
  bounded recent ordering, revision-fenced recovery restore, commit-only state changes, and all four
  actionable failure classes through a deterministic backend. It also proves exact active editor
  route matching, accepted revision publication, stale editor refresh rejection, active and recent
  restoration through the real local host, and missing-document degradation that retains recents.
- `app/src-tauri/tests/desktop_shell_contract.rs`: Proves sequence-fenced document, recent, history,
  busy, and complete panel-layout synchronization, stable recent, workspace, and palette menu intents,
  duplicate close suppression, one-shot resolution, schema 1 migration, schema 2 round trip,
  duplicate placement rejection before live mutation, malformed persisted presentation recovery,
  nonblocking persistence failure, and private workspace restoration across native state instances.
- `app/src-tauri/tests/media_import_contract.rs`: Proves picker, drag/drop, recursive folder scan,
  deterministic image-sequence grouping, direct API and automation parity, correlated event
  evidence, durable reopen, and duplicate no-op semantics through the real local project host.
- `app/src-tauri/tests/media_library_views_contract.rs`: Freezes the authoritative snapshot,
  revision-fenced mutation, typed bridge, production bins and smart-collection consumer, list and
  grid controls, transparent freshness-aware thumbnails, deterministic fallback, and exclusion of
  later proxy and search ownership.
- `app/src-tauri/tests/media_metadata_contract.rs`: Freezes source status and inspection DTOs,
  generic user metadata mutation, the two Tauri commands, typed bridge, production metadata
  consumer, stable C004 identity attachment, and exclusion of C006 annotations and C007 duplicate
  grouping.
- `app/src-tauri/tests/media_annotations_contract.rs`: Freezes the C006 typed annotation and
  derived-usage owner, revision fences, stable C005 metadata attachment, registered native command,
  typed bridge, production media-detail consumer, and C007 duplicate-detection exclusion.
- `app/src-tauri/tests/media_identity_contract.rs`: Freezes the C007 exact fingerprint identity
  projection, reusable exact-time selections, manually refinable fixed-point tracked regions,
  registered native command, typed bridge, production detail consumer, and C008+ exclusions.
- `app/src-tauri/tests/derived_media_lifecycle_contract.rs`: Freezes the C008 source-bound proxy and
  optimized-media lifecycle, explicit status and quality selection, deterministic original fallback,
  registered native command, typed bridge, production detail consumer, and C009+ exclusions.
- `app/src-tauri/tests/offline_media_contract.rs`: Freezes C009 offline availability, local search,
  relink, replace, and conform integration while preserving C008 and keeping C010 artifacts out of
  persisted offline state.
- `app/src-tauri/tests/media_preview_generation_contract.rs`: Writes and imports one real PNG still,
  three real PNG sequence frames, one stereo 48 kHz 16-bit WAVE whose 262,145 frames cross the
  media-I/O packet boundary, and one unsupported video source. It proves bounded data-URL thumbnail,
  preview, canonical-order filmstrip, and exact continuous waveform products, stale revision and
  fingerprint rejection, explicit unavailable states, native command registration, the strict
  TypeScript bridge, and the production React inspector.
- `app/src-tauri/tests/media_content_search_contract.rs`: Freezes C011 persisted language and local
  content artifacts, exact timing, speaker and timeline relationships, the two native commands,
  strict TypeScript bridge, production search and editing consumer, and offline model independence.
- `app/src-tauri/tests/media_batch_operations_contract.rs`: Freezes every C012 native, command,
  typed-bridge, and production-consumer marker and executes a real two-source project through one
  nine-operation revision. It proves stable IDs and fingerprints, generating optimized and proxy
  fallback, one revision advance, complete mixed-operation rollback, durable reopen, and exclusion
  of runtime-only presentation derivations from the sidecar.
- `app/src-tauri/tests/removable_media_contract.rs`: Freezes the C013 native owner, Tauri command,
  strict bridge, and production inspector. Real filesystem proof covers import baselines, exact byte
  changes, stable editor state, changed-preview rejection, stale rollback, accepted-byte return,
  missing-file loss and restoration, durable sidecar reload, and absent conventional removable
  volumes with retained `wait_for_volume` intent.
- `app/src-tauri/tests/source_monitor_contract.rs`: Opens one real mono 48 kHz WAVE through the
  engine source registry, proves retained exact seek, atomic and durable fingerprint-bound marks,
  reversed-mark rollback, and unload. It also opens a real three-frame PNG sequence, proves the
  exact imported 24 fps inclusive range and overrun rollback, and sends all four three-point
  placements plus equal-duration four-point editing through the retained generated project route.
  The contract proves undo, redo, revision 8 persistence, source-monitor freshness, Tauri
  registration, bridge coverage, workspace purity, and honest no-presentation wording.
- `app/src-tauri/tests/project_settings_contract.rs`: Proves default inspection, complete atomic
  settings update, lifecycle revision coherence, durable reopen, and stale-revision rejection
  through the real local project host.
- `app/src-tauri/tests/transport_contract.rs`: Proves the bounded transport owner opens exactly one
  ordered connection generation with the stable desktop stream identity and no false replay or
  resync state. Native unit proof additionally opens a real project database, routes complete editor
  state, a track command, and a complete marker create through the linked EngineControl process, verifies
  exact generated response, revision, and event correlation, refreshes desktop lifecycle identity,
  reloads the resulting revision 2 timeline state, and reopens the durable edited track and marker.

### Frontend CI contract

- `ci/frontend-smoke/.node-version`: Pins Node.js 24.13.0 for local and hosted frontend gates.
- `ci/frontend-smoke/README.md`: Defines the retained generated-binding compatibility boundary,
  exact local commands, build-before-test ordering, and the production application's ownership of
  blocking frontend CI.
- `ci/frontend-smoke/index.html`: Supplies the minimal browser document and module entry consumed by
  the Vite production build.
- `ci/frontend-smoke/package-lock.json`: Lockfile version 3 resolution for exact TypeScript 5.9.3,
  Vite 7.3.6, their build dependencies, and platform-optional esbuild and Rollup packages.
- `ci/frontend-smoke/package.json`: Declares a private CI package, Node.js 24.13.0, independent
  typecheck, build, and test commands, and exact TypeScript and Vite development dependencies.
- `ci/frontend-smoke/src/api-contract.ts`: Imports the committed generated API artifact, constructs
  exact typed project and playback commands, a command-log query, unavailable AI state, and API
  version negotiation request through current catalog release `1.9.0`, consumes playback,
  command-log, and negotiation
  responses plus the extension query,
  lifecycle, event, resource, stable public control reference, typed maps, and transport-neutral
  client constructor used by the browser build.
- `ci/frontend-smoke/src/main.ts`: Implements a strict typed browser entry that verifies the contract
  root, consumes generated command, command-log, negotiation, and AI state examples, and renders the declared
  product, readiness, and independent frontend gates.
- `ci/frontend-smoke/tests/contract.test.mjs`: Verifies exact scripts and versions, strict compiler
  settings, production application workflow routing, locked installation, mandatory gates,
  generated API import, command-log, negotiation, and extension discovery contracts, typed maps,
  client surface, and the hashed JavaScript entry in the retained bundle.
- `ci/frontend-smoke/tsconfig.json`: Defines strict no-emit TypeScript checking for the browser entry
  with ES2022, DOM, bundler-resolution, isolated-module, and forced-module semantics.

### Tauri Rust CI contract

- `ci/tauri-smoke/README.md`: Defines the retained native toolchain compatibility boundary, mock and
  native runtime proof split, exact local commands, and the production application's ownership of
  blocking native CI.
- `ci/tauri-smoke/frontend/index.html`: Supplies the static asset consumed by Tauri build metadata.
- `ci/tauri-smoke/src-tauri/Cargo.toml`: Declares exact Tauri 2.11.5 and Tauri Build 2.6.3 versions
  with only the required `test` and `wry` features.
- `ci/tauri-smoke/src-tauri/Cargo.lock`: Locks the standalone Tauri host dependency graph.
- `ci/tauri-smoke/src-tauri/build.rs`: Runs the standard Tauri build integration.
- `ci/tauri-smoke/src-tauri/rust-toolchain.toml`: Selects stable Rust with rustfmt and Clippy.
- `ci/tauri-smoke/src-tauri/src/lib.rs`: Registers one command through a generic builder, exposes
  the native wry builder, and tests the same configuration with Tauri's mock runtime.
- `ci/tauri-smoke/src-tauri/src/main.rs`: Constructs the native builder without launching a window.
- `ci/tauri-smoke/src-tauri/tauri.conf.json`: Declares bounded CI identity, assets, window metadata,
  and disabled bundle generation.
- `ci/tauri-smoke/tests/contract.test.mjs`: Verifies pins, builder surfaces, workflow security,
  native runners, Linux prerequisites, production application routing, frontend prerequisite, and
  all four mandatory Rust gates.

### Cargo workspace and repository configuration

- `open/bindings/typescript/superi-api.ts`: Deterministic committed TypeScript representation of the
  public API. It contains all named DTOs including version negotiation, exact method, event, and
  resource maps, recursive wire primitives, the bounded local scripting request, program, trace,
  and response types, strict exact playback transport actions and acceptance, extension identity, lifecycle, capability, safe failure, feature,
  control, query, event, and resource declarations, generic project import-media request and result
  evidence, and a transport-neutral typed client without owning runtime IPC. The committed artifact
  is freshly regenerated from the canonical schema `1.7.0` project surface and includes strict
  track, caption, marker, and multicam mutation, complete audio routing, audio-video link, exact
  synchronization, audio detach, exact retime map and evidence, metadata, track-output graph DTOs,
  the additive exact `set_transition` timeline operation, complete nested placement and
  selection-derived compound request, action, and evidence unions, and all seven multicam
  operations with ordered mutation evidence.
- `open/Cargo.lock`: Cargo lockfile format 3 for the resolved workspace. It records 25 local
  workspace packages, registry dependencies, target-support dependency trees, scenario digest
  and process-instrumentation dependency edges, the API introspection and validation contracts'
  test edge to `superi-concurrency`, graph and timeline document serialization and integrity edges,
  cache-key hashing, and the exact `oxideav-mp3` Git revision. Timeline state
  directly consumes the already-resolved `serde`, `serde_json`, and `sha2` packages, while
  `superi-audio` now directly consumes the already resolved serde, serde_json, and SHA-256 packages
  for its strict authored clip-mix component codec. Its macOS target also directly consumes the
  already resolved `block2`, `objc2-audio-toolbox`, and `objc2-core-audio-types` packages for the
  private Audio Unit host. `superi-project` directly consumes
  `superi-audio`, exact `rusqlite` 0.32.1, exact synchronization-only `fs4` 1.1.0, and the existing
  Serde, JSON, and SHA-256 packages for canonical extension metadata, opaque-payload evidence,
  validated active-file generations, and legacy component fixtures.
  `superi-engine` records a test-only
  direct rusqlite edge for its real migrated-project resource consumer.
  The bundled SQLite edge resolves `libsqlite3-sys` 0.30.1, `ahash` 0.8.12, `hashbrown` 0.14.5,
  `hashlink` 0.9.1, fallible iterator adapters, and native build discovery packages. This exact
  path compiles on the declared Rust 1.80 floor and keeps project databases offline and
  self-contained. Meanwhile,
  `superi-cache` directly consumes the same pinned `sha2` package and now records its reviewed
  internal dependency on `superi-concurrency` for bounded background rendering. `superi-effects`
  directly consumes Serde for its strict authored wires and now directly consumes the existing
  `serde_json` and `sha2` resolutions at runtime for canonical integrity-protected effect preset
  documents. The `superi-api` package record now consumes the already resolved `serde_json` and
  `sha2` packages at runtime for strict local script interpretation and exact-source identity. It
  also includes `superi-concurrency` as a test-only internal edge for real EngineControl proof and
  enables the existing engine test-support seam for persistence, integrity, media, autosave, and
  recovery proof without adding a direct API-to-project edge. Text layout adds exact Swash 0.2.9, Skrifa
  0.31.1, Unicode Bidi 0.3.18, and Unicode Linebreak 0.1.5 runtime packages plus test-only Font Test
  Data 0.5.0. The locked Indexmap resolution is 2.11.4 so the declared Rust 1.80 compiler can parse
  and build the affected graph and GPU dependency path. Audio output adds exact CPAL
  0.17.3 and ringbuf 0.4.8 plus their target-specific backend dependency trees. Audio Unit hosting
  reuses already locked permissive Objective-C framework bindings and adds no network or internal
  crate edge. VST3 hosting adds
  exact `vst3` 0.3.0 and `com-scrape-types` 0.1.1, both licensed `MIT OR Apache-2.0`, and reuses
  existing libloading 0.8.9 plus macOS Core Foundation bindings. TypeScript generation adds exact
  optional Specta 1.0.5 plus its permissively licensed generator dependencies; none enters the
  default runtime feature graph. The lockfile is generated resolution evidence and is not
  hand-edited policy.
- `open/Cargo.toml`: Root Cargo workspace manifest using resolver 2 and glob members under
  `crates/*` and `tools/*`. It centralizes version `0.0.0`, Rust 2021, MIT, Rust 1.80, repository
  metadata, deny-by-default unsafe lints, and shared dependencies for error handling, serialization,
  images, GPU, codecs, hashes, process instrumentation, platform APIs, native build support,
  reviewed audio device and ring-buffer primitives, a pinned block binding for macOS native
  completion handlers, exact bundled SQLite through rusqlite 0.32.1, exact low-level VST3 bindings,
  exact synchronization-only filesystem locking through `fs4` 1.1.0, offline font shaping plus
  Unicode layout, and exact Specta 1.0.5 for opt-in TypeScript generation.
- `open/README.md`: Compact open-tree orientation and build commands. It records the 19 runtime
  crates plus six repository tools, the committed TypeScript API artifact and freshness command,
  the documented local scripting runtime, the exact canonical runner command, contract-only status,
  and the remaining production integration boundary.
- `open/ci/network-isolated-contract.sh`: Executable contract binding the dedicated workflow to
  immutable checkout, least privilege, locked artifact preparation, namespace isolation, fixture
  validation, namespace-aware interface inspection, and the exact canonical headless CLI invocation
  and output locations.
- `open/ci/run-network-isolated.sh`: Linux harness that verifies a distinct namespace,
  reads its current interface inventory from `/proc/net/dev`, requires loopback-only interfaces and
  no IPv4 route, and proves a numeric outbound connection fails before running locked workspace
  tests, fixture validation, and the canonical runner with temporary outputs under locked offline
  Cargo.
- `open/deny.toml`: Cargo-deny policy allowing a bounded permissive license set, warning on duplicate
  versions and yanked advisories, rejecting unknown Git sources, requiring pinned Git revisions, and
  permitting only the pinned OxideAV MP3 repository as a Git source.
- `open/docs/STRUCTURE.md`: Compact dependency-tier map, codec placement, suggested human ownership,
  crate-boundary working rules, repository-tool placement, fixture-tool responsibility including
  OTIO baseline generation, structured test-report responsibility, TypeScript binding generation,
  and deferred production work.
  Its cache tier records the reviewed downward dependency on concurrency used by render jobs, and
  its project tier records the downward authored-audio dependency used by aggregate persistence.
- `open/rust-toolchain.toml`: Selects the floating stable Rust channel with `rustfmt` and Clippy.
  Package metadata separately declares Rust 1.80 as the minimum supported version.
- `open/rustfmt.toml`: Sets Rust 2021 formatting and a 100-column maximum width.

### Shared test fixtures

- `open/test-fixtures/README.md`: Defines the immutable versioned fixture layout, strict schema 1
  manifest, file inventory, provenance and parent-lineage rules, redistribution restrictions,
  contributor workflow, offline validation command, hard-failure conditions, the deterministic
  video, synchronized multichannel audio, timing, color and image-sequence, media-error, OTIO, and
  golden harness baselines plus the encoded canonical slice source and immutable expectation
  versions 1 and 2.
- `open/test-fixtures/golden/harness/v1/fixture.json`: Schema 1 CC0 manifest for the typed golden
  harness baseline, binding the four canonical JSON envelopes by exact byte count and SHA-256.
- `open/test-fixtures/golden/harness/v1/frame.json`: Canonical frame golden with exact two by one
  RGBA16F layout, channel, color, alpha, stride, and 16-byte payload semantics.
- `open/test-fixtures/golden/harness/v1/audio.json`: Canonical audio golden with exact negative
  sample-clock origin, 48 kHz stereo F32 layout, channel order, frame count, and sample bytes.
- `open/test-fixtures/golden/harness/v1/timeline.json`: Canonical nested timeline document used to
  prove recursively sorted object keys while retaining array order and exact values.
- `open/test-fixtures/golden/harness/v1/project.json`: Canonical nested project document used to
  prove schema identity and deterministic structural comparison.
- `open/test-fixtures/policy/utf8/v1/fixture.json`: Schema 1 manifest for fixture identity
  `policy/utf8`, version 1. It declares a synthetic CC0 payload generated by POSIX `printf`, records
  no parents, and inventories `hello.txt` as 6 bytes with its SHA-256 digest.
- `open/test-fixtures/policy/utf8/v1/hello.txt`: The six-byte UTF-8 payload `hello` followed by a
  newline. It is the fixture validator's deterministic self-test input.
- `open/test-fixtures/slice/video-cfr/v1/fixture.json`: Strict schema 1 manifest for canonical slice
  source `slice/video-cfr` version 1. It records generated CC0 provenance and binds one AV1 WebM
  payload by byte count and SHA-256.
- `open/test-fixtures/slice/video-cfr/v1/input.webm`: The 28,178-byte canonical 96 by 54, 24 fps,
  96-frame AV1 WebM input. It is generated from a fixed FFmpeg test source, validated as an opaque
  fixture payload, and consumed by the CLI's bounded identity path.
- `open/test-fixtures/slice/expectations/v1/fixture.json`: Strict schema 1 manifest for the derived
  `slice/expectations` version 1 fixture. It records the exact source and synchronized-audio parent
  manifests and binds one JSON record plus one RGBA payload.
- `open/test-fixtures/slice/expectations/v1/expectations.json`: Strict canonical scenario record
  containing source identity, 48 per-frame hashes, pixel tolerance, exact multichannel PCM probes,
  48 timestamps, four project-state digests, and complete target export metadata.
- `open/test-fixtures/slice/expectations/v1/expected-frames.rgba`: A 995,328-byte payload containing
  48 tightly packed 96 by 54 RGBA8 mirrored reference frames. Its whole-payload and per-frame
  SHA-256 identities are consumed by the private CLI verifier.
- `open/test-fixtures/slice/expectations/v2/fixture.json`: Strict schema 1 manifest for current
  `slice/expectations` version 2. It retains the exact source and synchronized-audio parents and
  binds the portable JSON record plus unchanged RGBA reference payload.
- `open/test-fixtures/slice/expectations/v2/expectations.json`: Current canonical record with the
  checkout-independent project-state digest and the unchanged frame, audio, timing, tolerance,
  and export contracts.
- `open/test-fixtures/slice/expectations/v2/expected-frames.rgba`: The same 995,328-byte 48-frame
  RGBA8 reference payload retained under immutable version 2 for complete version-local identity.
- `open/test-fixtures/video/pixel-formats/v1/fixture.json`: Schema 1 CC0 provenance and exact
  inventory for the generated catalog and raw-frame payload.
- `open/test-fixtures/video/pixel-formats/v1/video-cases.csv`: Fixed CRLF catalog with one record per
  plane across 207 format-and-rate cases, including geometry, offsets, sizes, and plane digests.
- `open/test-fixtures/video/pixel-formats/v1/video-frames.bin`: A 13,419-byte binary containing every
  catalog plane contiguously. `superi-fixture-tool` produces it, its manifest binds its exact hash,
  and `superi-media-io` consumes and validates every plane through the public frame path.
- `open/test-fixtures/audio/synchronized-multichannel/v1/fixture.json`: Schema 1 CC0 provenance and
  exact inventory for the generated 44,100 Hz stereo, 48,000 Hz 5.1, and 96,000 Hz 7.1 WAVE files.
- `open/test-fixtures/audio/synchronized-multichannel/v1/stereo-44100.wav`: A 17,708-byte
  WAVEFORMATEXTENSIBLE PCM16 stereo fixture with mask `0x0003`, 4,410 frames, synchronized signal
  boundaries, and distinct channel gains.
- `open/test-fixtures/audio/synchronized-multichannel/v1/surround-5-1-48000.wav`: A 57,668-byte
  WAVEFORMATEXTENSIBLE PCM16 5.1 fixture with mask `0x003f`, 4,800 frames, synchronized signal
  boundaries, and distinct channel gains.
- `open/test-fixtures/audio/synchronized-multichannel/v1/surround-7-1-96000.wav`: A 153,668-byte
  WAVEFORMATEXTENSIBLE PCM16 7.1 fixture with mask `0x063f`, 9,600 frames, synchronized signal
  boundaries, and distinct channel gains.
- `open/test-fixtures/color/image-sequences/v1/fixture.json`: Schema 1 CC0 provenance and exact
  inventory for the two generated catalogs and 448-byte image payload.
- `open/test-fixtures/color/image-sequences/v1/image-cases.csv`: Fixed 19-field CRLF catalog with
  eight 2 by 2 images covering SDR, Display P3, PQ, HLG, alpha, u16, f16, and f32 meaning.
- `open/test-fixtures/color/image-sequences/v1/image-samples.bin`: A 448-byte little-endian binary
  containing every catalog image contiguously with exact per-image digests.
- `open/test-fixtures/color/image-sequences/v1/sequence-cases.csv`: Fixed 7-field CRLF catalog that
  maps three logical ACEScg images to file frames -2, 0, and 2 and exact 24000/1001 presentation
  timestamps.
- `open/test-fixtures/timing/cadences/v1/fixture.json`: Schema 1 CC0 provenance and exact inventory
  for the generated timing catalog, including its stable generator command, seed, byte count, and
  digest.
- `open/test-fixtures/timing/cadences/v1/timing-cases.csv`: Fixed 11-field CRLF catalog with five
  cases and 18 samples covering CFR, decode-order VFR, 29.97 drop-frame labels, a forward timestamp
  gap, a reset, and explicit continuity segments.
- `open/test-fixtures/media/error-cases/v1/fixture.json`: Schema 1 CC0 provenance and exact inventory
  for the deterministic error catalog and four compact PCM container payloads.
- `open/test-fixtures/media/error-cases/v1/media-error-cases.csv`: Fixed 14-field CRLF catalog binding
  malformed, truncated, unsupported, and partial-readable cases to trigger stages, shared error
  codes, mutations, truncation lengths, and exact partial packet evidence.
- `open/test-fixtures/media/error-cases/v1/malformed.wav`: A 60-byte stereo PCM16 WAVE whose block
  alignment is deliberately inconsistent with its channel and sample width.
- `open/test-fixtures/media/error-cases/v1/truncated.aiff`: A 69-byte AIFF missing its final declared
  sample byte while retaining the complete container size declaration.
- `open/test-fixtures/media/error-cases/v1/unsupported.aifc`: A 70-byte AIFC form that exercises the
  production PCM parser's explicit unsupported boundary.
- `open/test-fixtures/media/error-cases/v1/partial-readable.wav`: A complete 60-byte stereo PCM16
  WAVE seed whose cataloged post-open truncation produces an aligned usable partial packet.
- `open/test-fixtures/timeline/otio-interchange/v1/fixture.json`: Schema 1 CC0 provenance and exact
  inventory for two generated OTIO timelines plus their expectation record, including the stable
  Rust generator command, target reference version, byte counts, and digests.
- `open/test-fixtures/timeline/otio-interchange/v1/canonical-slice.otio`: Native OTIO JSON for the
  exact 48-frame first editorial slice with one track, one trimmed clip, immutable WebM identity,
  and editable mirror effect metadata.
- `open/test-fixtures/timeline/otio-interchange/v1/interchange-coverage.otio`: Native OTIO JSON with
  clips, a gap, transition adjacency, owner-relative markers, a trimmed nested Stack, two linear
  rate changes, stable IDs, metadata, and two intentionally unsupported effect cases.
- `open/test-fixtures/timeline/otio-interchange/v1/expectations.json`: Pins OpenTimelineIO 0.18.1
  and OTIO_CORE:0.18.1, exact timeline durations, identity and opaque data policy, JSON pointers,
  and preserve plus diagnose behavior for unsupported constructs.

The mapping inventory contains authored UTF-8 contracts plus twelve binary payloads. Binary media is
intentionally read through metadata, producers, provenance, manifests, and consumers rather than
interpreted as prose.

## Public surface

This module has no runtime Rust API of its own. Its public surfaces are configuration and contract
surfaces consumed by people, Cargo, repository agents, tests, and downstream modules:

- The root README, north star, architecture, Phase 0 contracts, codec policy, phase plan, platform
  matrix, unsafe inventory, and MIT license define the repository's public technical and product
  commitments.
- `docs/vertical-slice.md` is the normative integration contract for the first editorial thread.
  It distinguishes disclosed-stub contract conformance from all-runtime conformance and reserves
  expectation and replacement work for their owning checkpoints. The concrete source fixture,
  contract-conformance runner, independent expectation fixture, and bounded stage instrumentation
  now exist under C017, C024, and C025 ownership.
- The shell-local `desktop_process_snapshot` command exposes only explicit operational ownership:
  one process phase, background-task admission, and seven ordered service records with phases,
  counts, pending joins, thread names, and safe summaries. It is independent from the generated
  engine API, authored project state, and the canonical application lifecycle contract.
- The application command registry and schema-1 keyboard-shortcut profile form one private desktop
  presentation surface. The registry owns defaults and executable command IDs, the active profile
  owns only bounded overrides, and the native desktop shell persists that profile without exposing
  a generated public API or entering project state.
- `open/Cargo.toml` exports inherited workspace package metadata, lints, and dependency declarations
  to every member manifest. The current glob expansion is 19 crate packages plus
  `superi-fixture-tool`, `superi-dependency-check`, `superi-boundary-tool`, `superi-bench`, and
  `superi-test-report`, and `superi-api-bindings`, for 25 members total.
- `open/Cargo.lock` is the reproducible dependency-resolution surface for builds and audit tools.
- `open/deny.toml`, `open/rust-toolchain.toml`, and `open/rustfmt.toml` are entry points for license
  audit, toolchain installation, and formatting.
- The shared fixture root is a repository-wide data interface. Tests identify a fixture by stable
  path and version, consume only manifest-listed payloads, and validate them through
  `superi-fixture-tool` rather than selecting an implicit latest version.
- The version 1 video fixture is the current deterministic format-and-rate baseline. Its fixed
  catalog and raw bytes are generated by `superi-fixture-tool` and consumed by the
  `superi-media-io` integration contract without adding a runtime dependency between them.
- The version 1 audio fixture is the current deterministic sample-rate, multichannel routing, and
  synchronization baseline. Its three WAVE files are generated by `superi-fixture-tool` and
  consumed through `superi-media-io::PcmContainerSource` without adding a runtime dependency between
  the packages.
- The version 1 timing fixture is the current deterministic cadence and discontinuity baseline. Its
  fixed catalog is generated by `superi-fixture-tool` and consumed by the media-I/O packet,
  presentation map, timestamp normalizer, and source timecode contracts without a runtime tool edge.
- The version 1 media-error fixture is the current deterministic malformed, truncated, unsupported,
  and partially readable PCM baseline. Its catalog and four payloads are generated by
  `superi-fixture-tool` and consumed by production `PcmContainerSource` open and packet-read behavior
  without a runtime tool edge.
- The version 1 OTIO fixture is the current deterministic interchange baseline. Its two timelines
  and expectation record are generated by `superi-fixture-tool`, consumed by production
  `superi-timeline` import and export through the native editorial model, and proven through both
  Rust contracts and an official OpenTimelineIO 0.18.1 reference oracle. No runtime tool edge or
  OTIO library dependency is introduced.
- The separate `slice/video-cfr` version 1 fixture is the canonical encoded source for the fixed
  editorial scenario. The CLI validates its manifest and payload identity before modeling import;
  current decoded traits remain expected contract values until the media stub is replaced.
- The derived `slice/expectations` version 1 fixture is immutable historical data whose project
  digest captured its authoring checkout path. Current version 2 normalizes that one source
  location to its stable repository-relative identity. The generic fixture tool validates both
  versions, while the CLI consumes version 2 and verifies frame hashes, tolerances, audio timing and
  routing, timestamps, modeled state, and export metadata without treating absent rendered pixels
  as a pass.
- The version 1 color fixture is the current deterministic SDR, wide-gamut, HDR, alpha,
  high-bit-depth, and image-sequence baseline. Its catalogs and raw samples are generated by
  `superi-fixture-tool` and consumed by `superi-color` transforms and `superi-media-io` sequence
  interfaces without runtime tool edges.
- The three repository skills expose checkpoint planning, checkpoint execution, and codebase map
  maintenance workflows. Their `agents/openai.yaml` files are presentation metadata, not alternate
  behavior specifications.
- `.codex/config.toml` exposes only the repository model contract: GPT-5.6 Sol with max reasoning.
  Checkpoint law, planning, execution, and mapping skills prohibit checkpoint subagents and keep the
  complete workflow in the owning task.
- The execution verifier accepts a required Git base revision and optional `--full` or `--dry-run`
  flags. It turns the final changed-path set into an explicit local command plan, validates changed
  Python and JSON inputs, always validates maps, and executes applicable repository gates without
  treating hosted CI status as a general checkpoint completion requirement.
- The mapping script exposes `inventory`, `files`, `hash`, `shards`, `changed`, and `validate`
  commands. Map validation checks anchored metadata, module ID, source ownership, revision, hash,
  file count, required headings, every source-inventory entry, actual index links, unexpected maps,
  and forbidden Unicode dash characters.
- `.github/scripts/check-dependency-policy.sh` is the executable repository contract surface that
  binds the workflow's security-sensitive inputs to `open/deny.toml`. Contributors and CI can run
  it directly with Bash before invoking cargo-deny.
- `.github/workflows/ci.yml` is the cross-platform hosted build surface. Its stable lane identifiers
  map the platform contract to GitHub runner labels, while immutable checkout, disabled credential
  persistence, per-branch concurrency, independent matrix reporting, and timeouts define the job
  boundary. Both build jobs directly validate all fixtures and execute the normalized
  `slice-contract` command after the complete Rust quality suite, and compile and test the supported
  `os-codecs` CLI configuration.
- `.github/workflows/dependency-policy.yml` is the separate dependency license and source policy
  surface.
- `.github/workflows/frontend.yml` and `app/` form a third CI surface for locked npm installation,
  strict TypeScript checking of the production lifecycle and generated public API clients, Vite
  production bundling, and application-contract proof. `ci/frontend-smoke/` remains an independent
  focused generated-binding compatibility consumer rather than application coverage.
- The application-presentation surface is a read-only composition boundary over existing public and
  shell-local evidence. Its pure model exposes normalized failure, progress, notification, status,
  and menu-placement values; its React provider exposes only transient presentation callbacks and
  routes recovery or workspace intent back to existing owners. It is not a new public engine API,
  persistence schema, project document, job scheduler, or native diagnostic owner.
- The application theme surface is the frozen schema-1 `color-critical-dark` identity plus its
  semantic CSS token contract. Root declaration repair is limited to document metadata, and
  `--viewer-*` plus `--marker-*` color-data tokens remain separate from replaceable `--theme-*`
  chrome. The theme surface is not a public color transform, project setting, persisted workspace
  field, or alternate automation protocol.
- The shell-local media-management surface now includes one `mutate_project_media_batch` Tauri
  command. Its strict tagged operation union, expected project and library revisions, bounded
  ordered list, deterministic affected-ID evidence, and complete replacement snapshot are mirrored
  exactly in TypeScript and consumed by the production React media browser. It remains application
  presentation state rather than a new engine API, codec executor, project document, or search
  service.
- The shell-local source-monitor surface exposes snapshot, load, exact seek, mark update, and unload
  commands. Project, library, monitor, media, and fingerprint fences are mirrored exactly in the
  TypeScript bridge; packet and frame payloads remain below the Tauri seam. Mark updates return one
  atomic durable library replacement plus monitor snapshot, while load, seek, and unload mutate
  only retained application runtime state. Scanner-confirmed changed bytes make an existing session
  stale and fence further source operations until the accepted source identity is reviewed.
- `.github/workflows/network-isolated.yml` and `open/ci/` form a fourth CI surface. It prepares
  artifacts before isolation, then proves current workspace tests, fixture validation, and the CLI
  consumer run with no non-loopback interface, no IPv4 route, and Cargo offline mode.

Together the five workflows enforce the open-tree boundary, locked hosted Rust builds, dependency
policy, the production frontend and native shell gates, and one network-isolated core path. They do
not yet implement the complete documented feature, malformed-input, GPU, audio, editor UI, or slice
suites.

The stable public automation protocol described by Phase 0 is owned in `superi-api`, not here. The
workspace owns the desktop lifecycle shell, its shell-local Tauri commands, the committed generated
TypeScript projection, and a retained frontend contract consumer. Engine methods and events remain
public API contracts rather than application-owned behavior.
Likewise, codec, graph, image, engine, project, timeline, and CLI Rust interfaces live in their crate
modules even when workspace documents define constraints on them.

## Architecture and data flow

Repository work flows through two control planes.

The operational control plane begins with `AGENTS.md`. A single checkpoint owner synchronizes with
the remote, claims its exact descriptions-tab ID suffix, rereads the immutable main-tab
specification, and records the base revision. It reads the relevant map closure and current
implementation, performs external research only if those sources leave a material question
unresolved, writes `planning.md` and `execution.md`, then implements, tests, updates maps, reviews the
final diff, runs deterministic proof, controls Git delivery, and completes paired-tab Google Docs.
The owner performs this complete lifecycle inline and may not create a checkpoint subagent. A multi-checkpoint
request is dispatched in first-seen order into separate Codex-managed worktrees. The rolling queue
defaults to three active checkpoint owners unless the user supplies another positive concurrency
value, and `.worktreeinclude` supplies the otherwise ignored root law.

The codebase-map flow is a repository navigation and freshness control plane. The Python script
discovers tracked files plus nonignored untracked files, excludes Git internals, generated maps,
plans, dependency output, and build output, then assigns `open/crates/*` and `open/tools/*` roots to
their own modules and everything else to `workspace`. The current mapper runs `files` for the
authoritative owned-path list, reads every assigned text file from first line through EOF, and may
use `shards` to partition large inventories only at whole-file boundaries. The same mapper records
surfaces, flows, relationships, invariants, tests, gaps, and risks, then reconciles those notes with
manifests, public entry points, and cross-module contracts before writing the required map sections.
The global index then captures repository-wide layering and runtime flow.

Maintenance follows the same evidence rule. Validate before relying on maps, use `changed` and the
actual diff after source work, reread each changed file and relevant interface or test through EOF,
update inventory and every affected architectural statement, and refresh consumer maps or the global
index when contracts, ownership, layering, flow, or status changes. Only after prose is reconciled
may the exact `hash` and file count be recorded. Validation must pass after updates, after final
integration or rebase, and before delivery. A passing hash never excuses stale prose.

The desktop window-session flow begins before Tauri setup finishes. The process-owned state starts
one bounded persistence worker, loads the strict session file, preserves rejected bytes on decode or
normalization failure, enumerates current monitors, reconciles every normal placement into visible
bounds, applies the primary placement, and restores auxiliary workspace webviews in stable session
order. Focus, move, resize, fullscreen, route, close, and reopen mutations replace one revisioned
snapshot, enqueue coalesced atomic persistence, and emit the same exact state to all webviews.
Monitor movement computes relative placement from the source work area into the selected target and
stores the previous placement; undo swaps those records so the action is reversible. Main-window
close requests the existing orderly application shutdown, while auxiliary close removes only that
window and its transport client. Each webview opens an independent generated transport generation
above the one shared engine and project lifecycle, and every authored replacement retains one global
event sequence across those client-local generations.

The keyboard-shortcut flow begins with immutable command registry defaults. The schema-1 profile
retains only explicit overrides, including deliberate unbindings and bounded inactive command IDs,
then resolves one conflict-free effective binding table. `ApplicationProvider` owns that table,
translates noneditable, noncomposing keyboard events into canonical platform-aware tokens, and
executes the matching existing registry command. The System panel edits the same profile through
transactional set, clear, reset, import, and reset-all operations; deterministic export serializes
only canonical overrides. `ApplicationShell` hydrates the profile before its first desktop sync and
persists it beside workspace presentation through native schema 3. Schema 1 and 2 records migrate
with default shortcuts, while an invalid schema-3 shortcut profile falls back independently without
discarding a valid workspace.

The desktop process-ownership flow composes those existing owners without replacing them. Native
startup creates one shared `DesktopProcessRuntime`, then supplies it to the engine, GPU viewport,
window persistence, file-association ingress, exit monitor, Tauri managed state, and React process
snapshot. EngineControl, Playback, and the bounded background pool report their exact named units;
GPU and persistence report explicit handles; the exit monitor and association group retain their
handles directly. Shutdown first closes association admission, then joins the exit monitor, all
admitted association tasks, window persistence, GPU submission, both engine domains, and every
bounded worker. Every join is attempted even after an earlier failure. Setup rollback uses the same
path, the process becomes Failed on any cleanup error, and the crash marker is cleared only after
the complete join set and canonical lifecycle both confirm orderly shutdown.

The desktop C012 media flow extends the existing C004 through C009 presentation owner without a new
authority:

1. React retains only a set of selected stable media IDs and constructs one explicit ordered batch
   from visible editor intent.
2. The typed bridge adds the current project and library revision fences and invokes one shell-local
   Tauri command.
3. `DesktopProjectState` reads current timeline usage, resolves the active project identity, and
   asks its existing `MediaLibrarySnapshot` to apply the complete list to a clone.
4. Rename changes only presentation name, organize reuses bin movement, proxy and transcode reuse
   the C008 source-fresh attachment normalizer, relink reuses the C009 fingerprint guard, and
   metadata reuses the C005 validator.
5. Any failure returns its zero-based operation index and discards the candidate. Success validates
   the complete candidate, advances one library revision, refreshes thumbnails, duplicate identity,
   smart membership, availability, and representation resolution, then replaces authoritative
   state once.
6. Sidecar serialization removes current-project usage, derived identity, smart membership,
   thumbnail, availability, and resolved-representation projections before temporary-file rename.
   Durable presentation names, bins, metadata, annotations, selections, source paths, attachments,
   and explicit representation choice remain stored.
7. The returned replacement snapshot includes every refreshed projection. Generating derived media
   remains inspectable but resolves to the original source until matching ready evidence exists.

The build control plane begins at `open/Cargo.toml`. Cargo expands `crates/*` and `tools/*`, applies
shared package metadata and lint defaults, resolves member and external dependencies into
`open/Cargo.lock`, including the pinned MIT rubato 0.16.2 sample-rate converter and the exact
Rust-1.80-compatible text shaping stack plus exact low-level VST3 0.3.0 bindings, and writes
generated build output under the ignored `open/target/`. Runtime
dependency direction is downward through the crate tiers: core and representation types support
GPU, concurrency, media, graph, and codecs; feature catalogs and timeline build on those; engine
orchestration assembles them; the API is the stable facade; and CLI is a headless consumer. The
  fixture, dependency-check, boundary, structured-report, benchmark, and API-binding tools are
  workspace members for common build, test, Clippy, and MSRV coverage, but none is part of the
  runtime DAG.

Cargo records test-only member dependencies in the same package dependency arrays as runtime
dependencies. The `superi-api` lock entry names `superi-concurrency` so public contracts can
exercise real EngineControl ownership. Production `serde_json` and `sha2` support strict local
scripts, while the engine's existing feature-gated test-support seam exercises durable persistence,
integrity, media, autosave, and recovery without a direct API-to-project edge.

The API's optional `typescript-bindings` feature uses exact Specta 1.0.5 only to reflect its
serializable DTO declarations. The `superi-api-bindings` tool consumes that feature, combines the
derived declarations with the API-owned canonical method, event, and resource registry, and writes
the committed artifact outside the runtime graph. The generated client remains transport-neutral,
and the default API feature set does not include Specta.

The timeline component document reuses workspace `serde`, `serde_json`, and `sha2` pins already
present for core and graph contracts. This changes the direct package edges recorded for
`superi-timeline` but does not change crate-tier direction, introduce a network path, or transfer
SQLite and autosave ownership away from `superi-project`.

The project schema owner consumes exact rusqlite 0.32.1 with default features disabled and bundled
SQLite enabled. It also consumes the existing exact Serde, JSON, and SHA-256 pins for strict
extension metadata and opaque-payload integrity. Rusqlite and libsqlite3-sys are
MIT-licensed, SQLite is public domain, and the bundled path performs no runtime discovery or
network operation. The dependency remains below project and does not change any internal Superi
crate edge. Fresh `cargo +1.80.0 check -p superi-project --locked` proves the selected resolution on
the declared compiler floor. Project's test-only JSON edge builds supported schema-0 component
fixtures, while engine's test-only direct rusqlite edge builds an exact legacy database around the
existing real resource-acquisition consumer. Both packages were already locked and neither edge
enters a runtime dependency tier.

Collaborative replacement publication additionally consumes exact `fs4` 1.1.0 with default
features disabled and only its synchronous lock API enabled. The crate is MIT OR Apache-2.0,
declares Rust 1.75, performs no networking, and resolves only through already present rustix 1.1.4
and Windows Sys 0.61.2 target support. `superi-project` uses it privately for nonblocking exclusive
operating-system locks on one persistent sibling entry; no public type, internal Superi dependency
edge, process discovery, or persistence-format meaning changes.

The authored clip-mix document reuses the workspace serde, serde_json, and SHA-256 pins already used
by other strict component codecs. `superi-project` now depends directly on `superi-audio` so authored
clip-mix state can enter the aggregate and schema-4 database while prepared processors, devices, and
callback state remain below the persistence boundary. This is a downward runtime edge and preserves
the declared crate tiers and offline dependency policy.

The macOS Audio Unit host reuses the workspace's existing pinned AudioToolbox and Core Audio type
bindings and adds `block2` as a direct workspace declaration for the asynchronous native completion
handler. All three packages were already present in the resolved permissive platform graph. The
target-gated edge remains inside `superi-audio`, adds no internal dependency direction, and keeps
discovery, preparation, process-location verification, callbacks, and teardown inside the audited
private native module.

The worker-side VST3 host uses exact `vst3` 0.3.0 and its exact `com-scrape-types` 0.1.1 support
crate, both under the existing permissive license allowlist. It reuses the pinned retained-library
loader on Windows and Linux and the pinned Core Foundation bundle owner on macOS. No host framework,
remote service, plugin binary, SDK source, or runtime discovery database enters the repository;
tests compile their own temporary dynamic module and load it only inside isolated child processes.

The native plugin resilience work adds no new resolved package. Audio Unit state reuses the existing
target-gated Core Foundation binding with data, error, and property-list features, VST3 state reuses
the existing low-level binding, and the format-neutral binary envelope reuses the audio crate's
existing SHA-256 dependency. Engine continues to depend downward on audio and project for the state,
processor, and extension contracts, with no new internal tier edge.

The effects animation, mask, rotoscope, text, and preset wires reuse the same workspace Serde pin.
Effect presets additionally reuse the workspace JSON and SHA-256 pins at runtime for strict
canonical documents, legacy upgrade, and corruption detection, while the built-in visual catalog
reuses the workspace `half` pin for bounded binary16 reference
pixels. Text adds Swash and a direct exact Skrifa constraint for real OpenType shaping, Unicode Bidi
and Unicode Linebreak for deterministic paragraph layout, and reviewed test-only font bytes. Every
package is permissively licensed, caller font resolution remains offline, and the direct Skrifa and
locked Indexmap versions keep the affected crate graph buildable on Rust 1.80. Effects and timeline
consume the neutral graph payload independently, timeline does not depend on effects, and no network
path, runtime tier reversal, or persistence ownership transfer is introduced.

The deterministic cache-key contract reuses the same resolved `sha2` pin. Its lockfile change adds
one direct external package edge to `superi-cache` without changing the reviewed internal runtime
dependency graph or introducing another registry package.

The cache render path adds one reviewed downward edge from `superi-cache` to
`superi-concurrency`. The dependency-direction policy and `open/docs/STRUCTURE.md` authorize that
edge explicitly, while worker priority, cancellation, deadlines, and pool ownership remain outside
the graph crate.

The dependency-direction path is a separate local architecture gate. `superi-dependency-check`
reads locked offline Cargo metadata, classifies all 19 runtime crates, and checks internal normal,
build, and dev-only edges against explicit reviewed policies. Its live-workspace contract runs in
ordinary workspace tests, while the direct command gives contributors a deterministic failure
before review.

The structure guide and executable policy review `superi-project` to `superi-audio` as a downward
runtime edge for authored clip-mix state and its canonical codec. A focused synthetic contract
accepts that edge and rejects the reverse direction, so audio processing ownership cannot acquire
project or persistence policy.

The structure guide and executable policy now review the API's test-only concurrency edge alongside
its existing media-I/O test edge. The former enters EngineControl to exercise the real dispatcher
introspection seam, while synthetic policy contracts prove neither test relationship can become a
normal or build dependency without a separate architecture change.

The dependency-policy CI path begins on a push, pull request, or manual dispatch. The read-only job
checks out the tree, runs `.github/scripts/check-dependency-policy.sh` to verify the expected
workflow and policy coupling, then runs cargo-deny against the virtual workspace manifest with all
features and the `licenses` and `sources` checks. `open/deny.toml` remains the policy authority; the
shell checker guards integration drift, and cargo-deny evaluates the resolved crate graph. Neither
step adds a runtime dependency or outbound path to the open editor.

The cross-platform Rust quality path begins on every pull request, push to `main`, weekly schedule, or
manual dispatch. A five-lane matrix builds on macOS 26 arm64, macOS 15 Intel, Windows Server 2025,
Ubuntu 26.04, and Ubuntu 24.04; only the preview Ubuntu 26.04 lane continues on error. Ubuntu 22.04
is a separate weekly or manual job because matrix values are unavailable to a job-level cadence
condition. Both jobs use read-only permissions, immutable `actions/checkout` with persisted
credentials disabled, stable Rust with rustfmt and Clippy, recorded tool and commit identity,
formatting, a locked full-workspace build, locked workspace tests, strict all-target Clippy, and
locked documentation tests. Linux jobs provision `libva-dev` for the locked media dependency's
pkg-config discovery plus `nasm` for the x86 libvpx build; Intel macOS jobs provision `nasm` with
Homebrew. Linux and macOS jobs build checksum-pinned libvpx 1.16.0 and set its explicit runtime
path. Hosted macOS skips only the three named native VideoToolbox or
AudioConverter lifecycles whose physical evidence belongs to the documented hardware lane. Linux
and Windows run the exact full workspace test command. Concurrency cancels superseded work for the
same pull request or ref, while matrix fail-fast is disabled so platform results remain independent.
The matrix also enables `os-codecs` on both macOS lanes, Windows 2025, and Ubuntu 26.04. Those lanes
build the CLI feature path and test the engine and API consumers after the complete default suite.
Ubuntu 24.04 and Ubuntu 22.04 remain default-only because their distribution libva APIs do not meet
the platform crate's required version.

The application feedback flow remains presentation-only. `ApplicationShell` reads current
workspace continuity, lifecycle, project, editor, window, shell, retained crash, and public export
replacement evidence. Pure adapters preserve reviewed context and classify unknown recovery text as
terminal, then one React provider renders status, progress, notifications, tooltips, context menus,
and recovery actions without clearing the last-valid workspace or editor snapshot. Pointer and
keyboard panel menu commands enter the existing immutable application reducer, while recovery
commands return to the existing lifecycle, project, editor refresh, and System owners. No new Tauri
schema, polling scheduler, project document, job executor, or native persistence path is introduced.

The frontend CI path begins on pull requests, pushes to `main`, or manual dispatch. Its isolated
Ubuntu 24.04 job installs the exact Node.js 24.13.0 declaration, performs a lockfile-only `npm ci`,
runs strict no-emit TypeScript checking over the React application and generated public API adapter,
builds the production entry with Vite 7.3.6, and then tests workflow wiring, lifecycle, generated
client and transport ownership boundaries, deterministic application framework behavior, five
professional workspace projections, immutable dock and tab behavior, exact audio semantics, canonical timeline projection,
navigation including nested open paths, exact nested placement and compound action construction,
exact snap-target resolution, visible rule and reversal wiring, exact pins, runtime transport
forwarding, and the generated hashed bundle. The retained frontend fixture
separately proves checkout-independent generated contract compatibility without standing in for the
production application.

The Tauri Rust CI path begins on pull requests, pushes to `main`, or manual dispatch. Its blocking
matrix builds the production frontend and compiles the pinned application host on macOS 26 arm64,
macOS 15 Intel, Windows 2025, and Ubuntu 24.04. Every lane checks formatting, runs the mock-runtime
and lifecycle tests, denies Clippy warnings, and compiles the real `superi-desktop` native wry binary
from the lockfile. The shell owns explicit application and engine lifecycle state, linked process
ownership, a concrete generated client transport, and the application framework with five public
editor-snapshot workspaces while broader generated method routing remains separate.

The network-isolated path begins on pull requests, pushes to `main`, or manual dispatch. It pins
checkout, disables persisted credentials, installs stable Rust, runs the shared checksum-pinned
libva 2.22 provisioner, builds the approved libvpx 1.16 runtime, fetches locked dependencies, and
builds the workspace and test executables while online. It records the host namespace and uses
privileged `unshare --net` to enter a new namespace, carrying only the required Rust environment,
the private libva header, pkg-config, native linker, and runtime linker paths, and the approved
libvpx path. The harness rejects the host namespace, any non-loopback interface,
any IPv4 route, or a successful numeric outbound connection before forcing Cargo offline and
running workspace tests, fixture validation, and the CLI. Interface discovery uses the current
namespace's procfs network view rather than a sysfs mount that can retain the host namespace view.
This proves current core commands operate without outbound access after setup, not that dependency
or media-runtime acquisition is offline.

The intended media path is source and container handling through `superi-media-io`, explicit backend
selection for permissive, platform, or vendor codecs, validated image and audio representations,
GPU upload and resident graph evaluation, color processing, cache participation, and explicit
readback only at delivery boundaries. The timeline deterministically compiles edits into graph
state. The engine now dispatches canonical scenario transactions and lifecycle commands and emits
ordered replacement state events. It also owns a bounded typed command history and atomic recorded
command path around the real
project document and every currently authored project media and extension command. Apply, undo, and
redo share one revision-fenced dispatcher path, restore complete immutable snapshots at fresh
monotonic revisions, append one durable record and event for every successful generic command, and
preserve failure and no-op branches. Compound transactions include
extension actions beside timeline, graph, media, audio, and root state. The project layer can now
consume any selected immutable
snapshot through one clockless typed autosave controller, publish a complete current-schema recovery
point through the existing atomic Backup path, and deterministically retain only the newest
user-selected generation count. The engine consumer proves apply, undo, and redo state reach those
artifacts exactly. The API now projects every current authored project action through one strict
generic command, typed evidence, minimum history replacement state, durable cursor-safe command-log
query, and correlated event. Its bounded digest-bound `superi-json` runtime interprets a closed
command, command-log, and editor-state step
vocabulary through that same facade, preserves nested permission checks and ordinary events, and
returns deterministic revision, semantic hash, conflict, and committed-prefix evidence. The CLI now
composes durable project, command-log, media, timeline, render settings, recovery, validation, and bounded
JSON-RPC automation through the API-owned local host, including exact-source script execution.
Subscription hosting, dedicated script path loading, and autosave scheduling remain later work.

The production nesting UI starts from the one canonical editor-state timeline document. A pure
projection builds an immutable catalog for every timeline, computes exact duration in each primary
edit clock, and records direct child dependencies. The React owner reconciles one transient path
from the selected root through currently valid nested clip edges, scopes the canvas and supplemental
clip detail to that active timeline, and exposes breadcrumbs, back, explicit open, and double-click
open behavior without authoring state. Placement filters the catalog against the target's recursive
dependency closure and publishes only append or physically equal replace requests; compound
creation maps the visible complete selection into deterministic per-track identities. Both authored
flows use the existing application action callback, engine compound transaction, timeline domain,
history, persistence, evidence, and snapshot refresh path.

Durable extension lifecycle remains user-controlled project state, while one bounded engine-owned
declarative registry now synchronizes exact OpenFX and native audio supervisor status into canonical
identity, capability, lifecycle, safe failure, and control discovery. The API projects that state
through a permission-free query and replacement event, generated TypeScript exposes it to the
frontend consumer, and every mutation still points to the existing permission-checked durable
project command. Worker handles, callbacks, factories, paths, and privileged engine entry points
remain private to runtime owners. The documented broader
target coordinates project, timeline, graph, caches,
persistence, undo, events, playback, and export and presents the same command surface to UI, CLI,
scripts, extensions, and Superi Max, with no privileged closed-tier route.

The canonical slice makes that target executable in stable increments. It fixes one default-build
WebM and AV1 fixture role, exact 24 fps half-open trim, one video track, one typed transform effect,
an independent sRGB deliverable, and eight ordered stage records. A stage reports `stub` until its
production owner replaces it, and any reported stub prevents runtime conformance. The current CLI
executes the complete control sequence at contract conformance, proves exact reversal, and publishes
a non-playable stub artifact. Report schema 1.1.0 records monotonic elapsed microseconds and process
resident bytes before and after every stage, using exactly 16 current-process boundary samples per
run. Six stages remain stubs, so no runtime slice exists.

Shared fixture data flows from a versioned directory to `fixture.json`, whose byte counts and hashes
bind every payload. `superi-fixture-tool` validates path safety, schema, provenance, lineage,
inventory completeness, size, and digest before crate tests, golden tests, fuzzing, benchmarks, or
end-to-end workflows consume the data. It generates the video baseline into a new absent directory
from stable pixel-format, rate, geometry, sample, and serialization rules. It separately generates
the audio baseline from stable sample-rate, speaker-mask, timing, integer-waveform, and WAVE rules,
and generates the timing baseline from fixed cadence, decode-order, label, and continuity-segment
rules. It also generates the media-error baseline from fixed PCM container layouts, mutations,
truncation lengths, shared error expectations, and a post-open partial-read recipe. Media-I/O
contracts validate the video catalog's complete matrix and public frame
construction, open each audio file through the production PCM source to prove exact timing, routing,
synchronization, samples, and continuity, and prove the timing catalog's CFR, VFR, drop-frame,
discontinuity, and reversible segment behavior. The error contract independently checks the critical
bytes, opens malformed, truncated, and unsupported inputs through production parsing, and proves the
cataloged aligned partial packet plus corruption evidence. The UTF-8 fixture remains the validator's
smallest policy self-test. The tool generates the OTIO baseline from fixed native schema objects,
rational edit values, identities, relationships, and unsupported-preservation expectations. The
timeline fixture contract proves the OTIO hierarchy, exact timing, identity,
relationships, nested composition, rate changes, metadata, opaque JSON retention, and explicit
unsupported diagnostics. The production timeline interchange contract imports the same fixtures
into typed native project state, proves direct name and retime edits, deterministic export,
reimport, explicit audio defaults, exact-clock rejection, and stable warning pointers. The public
headless example emits files that official OpenTimelineIO 0.18.1 accepts and target-writes as
equivalent. The encoded slice source is generated separately with the exact manifest command and
consumed through bounded CLI identity validation.
The derived expectation fixture binds that source plus the synchronized audio baseline. The CLI
performs strict bounded reads, validates every reference frame and WAVE sample probe, compares all
currently produced timestamps, state digests, and target traits, and reports rendered pixels as
not evaluated until production stages produce them.

The product boundary is physical and one way. The open workspace must build and perform core work
without `closed/`, accounts, remote services, or a network. Superi Max may call the open public API
and may produce normal editable artifacts, but no open crate may consume proprietary implementation.

## Dependencies and consumers

The workspace module depends on Git for source discovery, change selection, and revision identity,
Python 3 for maps and deterministic checkpoint verification, Cargo and stable Rust for the open
workspace, Bash and `grep` for the executable policy contract, cargo-deny plus GitHub Actions for
dependency policy, GitHub-hosted macOS, Windows, and Ubuntu runners for build portability, Node.js
24.13.0 with npm for the production React application, Tauri 2 with native desktop SDKs for the
production shell,
and the Google Docs plus Codex environment described by repository law for checkpoint coordination.
Project Codex configuration requires GPT-5.6 Sol with max reasoning and defines no custom agent
profile or project-level agent concurrency settings. The mapping and verification scripts use only the Python standard library; the
verifier
conditionally invokes the repository's Git, Python, Bash, Cargo, cargo-deny, and npm commands.

Every crate and repository tool consumes `open/Cargo.toml` package defaults and may opt into its
central dependency declarations. Cargo, CI, developers, and audit tooling consume the lockfile,
toolchain, formatter, ignore rules, and deny policy. Crate tests and end-to-end workflows consume the
shared fixture contract and fixture versions. Contributors, planners, reviewers, UI and engine
teams, and release operators consume the architecture and verification documents. The current slice
runner and each future production subsystem consume the stable scenario state, stages, report
boundary, and stage instrumentation contract. `superi-cli` consumes the pinned, system-only
`sysinfo` dependency for portable current-process resident-memory samples.

The effects preset codec remains a direct runtime consumer recorded by the lockfile. It uses only
already-resolved permissive serialization and hashing packages, remains offline, preserves the
effects-to-graph dependency direction, and does not move atomic project storage or plugin hosting
into the workspace layer.

The API engine-introspection and integration-validation tests consume the test-only concurrency
edge and enter the EngineControl domain to prove the real dispatcher seam. The scripting contract
uses the engine's narrow feature-gated helper to prove real persistence, integrity, media, autosave,
and recovery behavior without a direct project dependency. Production API code does not import the
project owner directly, and no runtime ownership moves into the workspace layer.

The production Tauri host consumes `superi-concurrency::LifecycleCoordinator`, the full
`EngineCommandDispatcher`, the transport-neutral integration-validation API, and core-owned error
classification through explicit downward path dependencies. `LinkedEngineProcess` retains one
dispatcher per application generation on a dedicated EngineControl thread and consumes the exact
headless-engine participant seam. Tauri manages the fixed-capacity `EngineConnection` and the
shell-local `DesktopProcessRuntime`; its React client consumes two lifecycle commands plus a separate
read-only process snapshot. The sibling
`DesktopWindowState` consumes Tauri window and monitor APIs, the application data directory, and the
existing lifecycle shutdown seam. Its typed React client and System panel consume only strict
shell-local snapshot and mutation commands plus one replacement event. Above the unchanged engine
connection, `transport.rs` registers one async Tauri API dispatcher for connect, request, cancel,
and disconnect control, scopes connection and pending identities by invoking webview label, routes
integration validation, complete editor state, and generic project commands into the existing
EngineControl owner, and converts failures through `PublicApiError`. The retained editor session
durably replaces the exact active project before its lifecycle revision advances, then transport
targets every generated project event to all connected webviews with one shared order and each
client's current generation. Generated engine-introspection replacements continue through the same
bounded ordered envelope.
The sibling `DesktopCapabilityState` consumes `superi-gpu` adapter enumeration, `superi-audio`
device declarations, the engine media registry through `MediaCapabilitiesApi`, and `superi-ai`
runtime availability as read-only lower-owner observations. Its strict TypeScript adapter and
System-panel consumer receive metadata only. The private retained cache is operational evidence,
not project, workspace, route, device, codec, model, or editable-artifact authority.
`app/src/transport.ts` implements the generated `SuperiTransport`, and `app/src/api.ts` remains the
sole `SuperiClient` factory. React consumes the injected binding for validation, health, complete
editor replacement state, and exact project commands without owning engine or project behavior.
`application-presentation.ts` receives only already classified public or shell-reviewed evidence and
normalizes it into immutable failure, progress, status, notification, and geometry values. The
React presentation provider composes those values above the existing `ApplicationProvider`; the
shell continues rendering the last-valid editor and workspace state while a fixed status surface
and expandable notification center retain actionable context. Panel menus dispatch the existing
application reducer, delivery progress reads only public replacement jobs, and recovery callbacks
return to the existing lifecycle, project refresh, editor refresh, or System routes. Notification
dismissal changes only bounded process-local presentation history and cannot dismiss a native crash
diagnostic, cancel a job, clear a project failure, or mutate authored state.
Beside those runtime owners, `DesktopCrashDiagnostics` retains one shell-local active-session marker
and a bounded replacement-safe journal under application data. Startup turns a surviving marker into an
unexpected-exit diagnostic before replacing it with the new session. The chained panic hook records
private panic detail in the native journal, while the serialized React snapshot contains only a safe
code, classification, reviewed context, captured continuity, and explicit recovery entries. React
publishes the route, every route-local dock, ordered panel tab, active tab, bounded size, hidden
panel, focused panel, active project, and revision intent through ordered typed blocking-safe
commands. The System panel validates current registry and active-project state before restoring that
intent in one immutable application action, then routes engine retries and restarts through `ApplicationLifecycle` and
project recovery through the existing `DesktopProjectLifecycle`. A clean acknowledged shutdown
removes only its matching marker, so a crash cannot erase evidence for a different session.
The native viewer path remains a separate shell-local control boundary. Each `NativeViewport`
retains transient navigation, overlay, comparison, analysis, and external display selection, then
publishes only role, stable analysis code, CSS geometry, device scale, visibility, and an optional
connection-local display identity. `DesktopViewportState` converts inline geometry to physical
pixels, projects the active Tauri monitor catalog, excludes the editor window's current display,
rejects cross-role target conflicts, positions one hidden borderless external window to the exact
selected physical bounds, and records selected analysis separately from nullable last-presented
state for both destinations. It queues inline and external commands to the existing GPU submission
thread. That owner maps the strict shell enum exhaustively to `superi-color::GpuDisplayView` and
presents the same unchanged managed RGBA16F role texture through the same ACEScg-to-sRGB intent on
both surfaces. Image, alpha, channel, luminance, and false-color analysis enter before primary and
transfer conversion; clipping observes display-linear RGB before transfer and attachment clamping.
External surface failures remain explicit and do not stop inline presentation. No pixels, texture
handles, encoded images, authored state, playback commands, comparison state, or second GPU owner
cross the Tauri seam.
The editing workspace consumes the canonical timeline document already contained by the public
editor snapshot. Its strict projection preserves exact authored identity, timing, grouping,
targeting, locks, synchronization, and output intent, while local playhead, range, scroll, zoom,
source and target choice, pending state, and consequence preview remain transient presentation
intent. Track, caption, and marker gestures plus insert, overwrite, append, replace, lift, extract, backspace, undo, and
redo return only through the generated project command and fresh complete snapshot paths. The
projection resolves timeline, item, and owner-clock marker boundaries into exact edit-rate targets,
skips inexact cross-clock points and valid object-marker overscan, exposes explicit session rules,
guides, and live consequences, and restores captured transient origin on Escape or pointer
cancellation.
Marker create, field edits, and removal retain complete typed inverse batches that are enabled only
at the exact refreshed revision produced by their preceding gesture.
The audio-video panel consumes that same frozen projection and resolves the selected pair by stable
track and clip identity rather than presentation order. Link, exact source-time synchronization,
and detach compile directly to the generated timeline operations. Replace reuses the existing
source-monitor replace planner and exact command executor with an audio-track target override.
Channel controls always submit one complete `set_audio_routing` mutation that retains the canonical
source layout, names an explicit destination, and supplies one channel or mute target per source
channel. The application-owned action and exact command executors retain transaction identity,
revision fencing, native validation, history, persistence, events, and canonical refresh below
React.
Caption projection retains canonical text, language, speaker, style, timeline relationships, and
object metadata beside exact record timing. The caption panel submits selected-field changes as one
generated `mutate_captions` action. Bounded SRT and WebVTT files and fresh source-fingerprint-bound
transcripts become explicit-gap millisecond caption tracks through the same shared executor, while
export reads only the latest frozen canonical track. Parsed input and analysis output never become a
second authored state owner.
The C007 timing compiler consumes that same frozen projection and emits only existing public edit
operations for ripple, roll, slip, slide, razor, trim, extend, ripple delete, and gap work. It
converts every affected track clock exactly, allocates typed identities only after validation, and
publishes the complete immutable batch through `ApplicationProvider.executeProjectActions` as one
`edit_timeline` action. That shared executor supplies the unique transaction identity, revision
fence, native retained-editor route, history, event delivery, and canonical snapshot refresh, so
React owns gesture intent and preview but never becomes an authored-state owner.
The editorial-feedback projector consumes that same transient plan and the latest canonical clip and
audio projections. It publishes one deeply frozen source, program, multicam, and meter replacement
through `ApplicationProvider`, so sibling viewers update immediately without receiving transport or
authored state. Slip preserves record placement while showing the proposed source start, slide
preserves source range while showing the proposed record start, and trim shows its exact proposed
record boundary beside pre-edit canonical source evidence. An operation is shown only when it
targets the active clip, and copied multicam detail is frozen without freezing the canonical input
projection. The audio rack retains sample clocks,
ordered channel identities, route targets, effective enable, mute, and solo state, and exact seam
evidence. A canonical audio track missing from the canvas has unavailable audibility and route
state instead of appearing routed. Because no editor or playback bridge publishes meter readings,
signal status remains explicitly unobserved and no amplitude is inferred. Viewer consequence DOM
remains outside the native child placement host and does not alter the strict geometry-only Tauri
payload.
Viewer comparison follows the same application boundary. Each `NativeViewport` combines the real
native diagnostic snapshot with a separate exact source-monitor or attached-playback navigation
coordinate to build one immutable shell identity. Capture actions retain that identity only, and
mode admission requires
the matching current and captured visual evidence, and split or wipe adjustment stays bounded
presentation state. Navigation and overlays remain independent, the native child stays mounted,
and the transformed pointer-transparent comparison layer communicates mode and identity without
copying or synthesizing pixels. The program viewer publishes its deterministic formatted summary
through `ApplicationProvider`, and `PlaybackControls` displays that live comparison beside the
canonical playback visual, audio, synchronization, timing, and degradation fields.
Program viewer transform controls begin from the same shared application selection and latest editor
snapshot. The pure projector accepts exactly one current clip reference, validates one matching
timeline graph plus its canonical envelope, schemas, nodes, ports, edges, order, drivers, and typed
clip identity, then walks only downstream processing nodes until the next timeline boundary. Every
exact built-in transform becomes one immutable ordered matrix and sampling presentation. Apply or
identity reset builds one changed-only `mutate_graph` action and hands it to
`ApplicationProvider.executeProjectActions`, which retains transaction identity, project revision
fencing, history, persistence, failure classification, and canonical refresh. React retains only
temporary input text, pending state, and result text, and never applies authored matrix values to
the viewer-local navigation transform or native placement geometry.
The same canvas supplements clip items from the snapshot's canonical graph and attached audio
automation, and it hydrates real media previews through the existing freshness-fenced Tauri owner.
Shared clip activation remains an application selection reference, not an authored timeline edit.
Selected transitions retain their exact canonical handle pair, derive adjacent-media limits and
deterministic alignment, and join the root graph to inspect downstream processing intent. Timing
submits one public `set_transition` action and scalar, Boolean, or choice values submit typed graph
mutations through `ApplicationProvider`; the canvas owns only transient form and pending state.
When exactly one source-bearing clip is directly selected, the retime planner projects its current
canonical map into exact decimal draft fields. Mode changes, playhead curve points, and segment edits
remain local until BigInt validation derives a gapless exact map and one generated public project
command. The existing application command owner then supplies the revision fence, pending state,
history unit, durable refresh, undo, and redo; React never patches canonical clip timing locally.

Tauri's `.superi` association ingress selects only local project paths from process arguments or
macOS resource-open URLs, normalizes and deduplicates them, and leaves filesystem validation to the
existing project lifecycle. Blocking opens execute away from the operating-system main thread.
Each result emits one complete replacement snapshot and restores the main window, while the React
listener rejects stale initial state and changes only project presentation, so it does not reset
workspace, panel, route, or selection state.

The native desktop shell accepts a complete bounded presentation from the always-mounted
application consumer and rejects stale client sequences. It rebuilds native File, Edit, and
Workspace menus from active document, recent path, history depth, busy, and route state, then emits
stable typed intents back to that consumer. Project dialogs and drops still call the existing
desktop project and media owners; undo and redo still call the generated project command; native
clipboard roles stay with the operating system. A close request is prevented once, duplicate
requests are suppressed while confirmation is active, and only a successful save plus explicit
frontend resolution enters the existing orderly application lifecycle. Window close, menu quit,
and direct operating-system exit share this path, including when auxiliary webviews remain open.
Workspace route, route-local dock placement, ordered tabs, active tabs, bounded dock sizes, hidden
panels, and focus persist privately in schema 2 and are reconciled against the live registry before
use. The application reducer compares structural placement and sizing against those registry
defaults, resets every route and global hidden state atomically, and retains the exact prior
presentation for one transient undo that never enters project history or native persistence. Schema
1 records migrate with empty layout intent so React can supply current registry defaults without the
native owner inventing application identities.

Beside the engine lifecycle, Tauri manages one serialized `DesktopProjectLifecycle` initialized
with the application recovery root. Its concrete backend calls only `LocalProjectHost` creation,
validation, save, save-as, recovery, settings inspection, and atomic settings transaction methods;
it also uses the same host for complete editor inspection and timeline command execution. Successful
durable results alone replace active identity and bounded recent state. A separate private session
record stores those paths and last-known identities atomically, revalidates the active project on
launch, retains recents when the active path is missing, and never serializes project contents or
the engine's in-memory undo and redo stacks. The typed React
adapter invokes complete project commands and the System panel renders lifecycle state, reviewed
failure actions, and an editable projection of project settings. The project-identity media store
owns bins, smart collections, freshness-fenced source inspection, and revision-fenced generic user
metadata; the React consumer only submits typed commands and replacement snapshots. The same owner
now resolves a preview request against exact project, library, media, and fingerprint fences, then
releases its lock before bounded source work. The child generator composes `superi-image` scaling,
`superi-media-io` WAVE parsing and continuity-validated waveform generation, and the in-tree PCM
decoder into ephemeral PNG data URLs. React requests only the selected item, rejects a late media or
fingerprint mismatch, and never persists generated bytes. Video, compressed-audio, EXR, and DPX
preview products remain explicitly unavailable until a bounded application decoder session exists.
Beside that ephemeral generator, the C014 monitor validates the same project media identity and
current source bytes, then opens a supported container through the engine source-only registry or
retains one verified still or image-sequence range. Probe, open, fingerprinting, and exact seek run
on blocking Tauri workers with interactive deadlines. One monitor mutex retains the boxed source,
current rational coordinate, stream metadata, and monotonic revision. The media sidecar separately
owns fingerprint-bound in and out marks, and only a complete validated candidate publication
advances its library revision. C013 exact changed-byte evidence also participates in monitor
readiness and operation admission, so a retained session becomes stale when its source requires
review. The React editing view consumes metadata and command state while the existing native source
viewport continues to own GPU presentation independently.

The documents deliberately point into other modules:

- `superi-core` owns shared identifiers, time, geometry, errors, diagnostics, and serializable base
  types.
- `superi-image`, `superi-gpu`, `superi-concurrency`, and `superi-media-io` own representation,
  resource, scheduling, and codec-neutral media foundations. The desktop preview consumer uses
  image-owned aspect-fit scaling plus media-I/O-owned PCM source, exact block, and waveform
  continuity contracts without moving those algorithms or semantic owners into the shell. The
  source monitor retains media-I/O `MediaSource` and exact-seek behavior but never owns demux or
  packet semantics.
- `superi-codecs-rs`, `superi-codecs-platform`, and `superi-codecs-vendor` implement the three codec
  acquisition classes behind media interfaces. The desktop preview path directly consumes only the
  in-tree PCM decoder for a WAVE source whose container metadata supplies the exact explicit format;
  it does not construct a platform, vendor, or general playback registry. The C014 monitor instead
  consumes `superi-engine::media::source_backend_registry`, whose source-only construction cannot
  initialize these decoder or encoder runtimes.
- `superi-graph`, `superi-cache`, `superi-color`, `superi-effects`, `superi-timeline`, `superi-audio`,
  and `superi-ai` own evaluation and capability layers. In particular, the workspace theme owns
  application chrome only; `superi-color` and the native GPU path retain scene meaning, precision,
  monitor binding, display intent, transform execution, and transform order.
- `superi-project` owns aggregate validation, authored clip-mix and opaque extension durability,
  checked snapshot restoration, schema-4 database persistence, atomic file publication,
  active-generation conflict detection, cooperative replacement locking, deterministic autosave
  scheduling, managed recovery-point retention, and pruning;
  `superi-engine` owns bounded compound project commands, session command history, extension
  dispatch, canonical asynchronous export-job scheduling, and integration;
  `superi-api` owns the stable public seam, including host-injected filesystem, plugin, and
  destructive authorization, bounded local scripting, durable local project hosting, plus strict
  nonblocking job inspection and cooperative control; and `superi-cli` is the headless durable
  project, JSON-RPC automation, schema, and exact-fixture scenario consumer.
- `superi-fixture-tool` validates repository fixture policy but does not enter runtime engine flow.
- `superi-dependency-check` validates the runtime Cargo graph but does not enter runtime engine flow.
- `superi-boundary-tool` validates source boundaries but does not enter runtime engine flow.
- `superi-test-report` validates and normalizes platform-lane evidence but does not enter runtime
  engine flow.

The closed tier is only a consumer of the open API. It is never a workspace dependency or a source
of open runtime behavior.

## Invariants and operational boundaries

- Open Superi remains MIT, account-free, identity-free, and fully functional with the network
  disconnected. Core code does not initiate outbound traffic or depend on hosted fallback.
- Dependency direction is one way across both major boundaries: higher crate tiers depend downward,
  and Superi Max depends on open Superi rather than the reverse.
- The public API is transport-neutral, versioned, typed, and shared by every client. Bulk media does
  not cross JSON-RPC or webview IPC.
- Crash diagnostics are shell-local operational state, not authored project state or a generated
  public API. The native journal may retain private panic detail, but the React snapshot may expose
  only reviewed bounded context. Session-marker removal requires exact session identity and orderly
  lifecycle acknowledgement; recovery actions must reuse the existing application, lifecycle, and
  project owners and must not replace a different active project.
- Application feedback is presentation-only. It preserves source, code, action, reviewed context,
  optional last-valid resource, and continuity evidence, maps only the four existing recovery
  classes, and fails unknown classes closed to terminal presentation. Notification dismissal and
  menu closure mutate only bounded process-local presentation state; they cannot clear native
  diagnostics, cancel jobs, change projects, or erase last-valid editor and workspace state.
- Every long-lived desktop execution unit has one retained native owner and an explicit join path.
  Project association task admission is bounded and closes before teardown; partial startup joins
  every owner already created; one cleanup failure cannot skip later joins; setup rollback and normal
  exit share the same cleanup order. The process snapshot is operational visibility only and never
  substitutes for application lifecycle, engine, project, GPU, or persistence authority.
- Local scripts use exact digest-bound `superi-json`, a closed versioned step vocabulary, complete
  nested permission preflight, and the same stable public project command surface. They do not gain
  ambient file, process, network, or hidden mutation authority, and a stopped result preserves both
  committed-prefix visibility and the last valid project evidence.
- Public asynchronous job state is a strict projection of the engine-owned queue. Handles,
  progress, dependencies, cooperative controls, and ordered full replacement events may cross the
  public seam, while executors, host polling, waits, typed artifacts, and process-local queue
  ownership do not. Application progress must remain indeterminate when a public total is absent and
  must never create a local percentage, scheduler, retry flag, or completion claim.
- Authored project changes use one typed engine command-history surface. Retained before and after
  snapshots are bounded session state, the selected project snapshot is the only durable state, and
  domain crates do not own competing undo stacks.
- Native shell state is presentation-only. It may retain workspace route, route-local docks,
  ordered tabs, active tabs, bounded dock sizes, hidden panels, focus, active document identity,
  recent paths, busy state, history depth, and a bounded versioned keyboard-shortcut profile, but
  project bytes and undo
  or redo snapshots remain with their existing durable project and engine-session owners. Close
  resolution is one-shot, active operations block closing, and any accepted close saves the active
  project before document replacement or process exit.
- Command discovery is a projection over existing owners, not a second automation or mutation
  protocol. Stable catalog identities must delegate only to registered application commands or
  typed desktop shell intents, disabled actions must expose current reasons, and transient query,
  selection, pending, and modal state must remain outside workspace persistence. The fixed palette
  opener may run from editable controls, while other shortcuts retain editable-target protection.
  Application actions must project their effective configured shortcut rather than stale registry
  defaults.
- Configurable shortcuts may target only registered application commands. All effective bindings are
  canonical and unique, global overrides require a portable primary modifier, native menu and
  clipboard accelerators remain reserved, and import is an all-or-nothing schema validation. Unknown
  command IDs may survive a bounded round trip but cannot dispatch until the registry defines them;
  editable targets, composing input, and unrecognized keys never execute a command.
- Application theme state is fixed presentation metadata, not project, workspace, engine, viewer,
  or generated API authority. Only `color-critical-dark` is supported; activation may reconcile
  document attributes and browser theme-color metadata but must not read or write persistence,
  select a system color preference, mutate application state, or add a second automation path.
  Replaceable `--theme-*` chrome tokens must remain separate from exact `--viewer-*` and
  `--marker-*` color-data meaning, and native scene pixels must retain full opacity, normal blending,
  no CSS filter, canonical precision, monitor binding, display intent, and transform order.
- Desktop point editing converts the source monitor's inclusive out mark to an exclusive edit
  boundary exactly once, derives missing source or record boundaries only when rational clocks are
  exactly representable, and stays within retained source and target track bounds. Four-point edits
  require physically equal source and record durations until timeline-owned fit-to-fill retiming is
  available. Every successful operation uses the retained generated project-command route and the
  single engine history owner.
- Editorial viewer and audio-meter feedback is presentation-only. It derives from exact transient
  plans and canonical replacement state, keeps multicam angle and switch references strict, retains
  sample clocks, channel order, route intent, audibility, and continuity, and labels live signal
  telemetry unobserved until a real runtime owner publishes readings. Feedback DOM may not enter or
  expand the native viewport placement payload.
- Viewer comparison is presentation-only and captures immutable shell diagnostics rather than GPU
  textures. Reference-dependent modes require an exact native reference identity, snapshot view
  requires an exact native snapshot identity, and unavailable current pixels or temporal context
  remain explicit. Comparison controls, summaries, and DOM may not author project state, drive
  playback, alter display intent, copy pixels, retain native textures, or expand the strict
  control-only viewport payload.
- Viewer analysis is transient presentation-only state. The shell sends exactly one stable code,
  native diagnostics distinguish selected from last-presented state, and pixel interpretation stays
  on the production GPU display presenter. Navigation, overlays, playback, audio, comparison,
  authored project state, canonical scene pixels, and frame IPC remain independent.
- Viewer transform controls are an editor for ordinary canonical graph parameters, not a second
  transform, render, graph, history, or persistence owner. They must resolve one current selected
  clip, validate exact graph topology and the built-in transform schema, preserve canonical node
  and parameter order, reject nonfinite or unsupported state, keep driver-owned values read-only,
  and publish only through the revision-fenced application project action executor. Authored
  matrices may never be copied into the viewer navigation CSS transform or native placement payload.
- Viewer color selection is presentation-only and independent for source, program, composite, and
  color roles. The native owner must refresh the bounded display catalog, bind an exact monitor ID
  and profile identity or explicit unprofiled state, verify that binding before frame acquisition and
  again before presentation, and execute only the selected sRGB or Display P3 GPU transform. React
  may send role, monitor ID, and transform ID through the separate strict command and may display
  returned diagnostics, but it may not receive ICC bytes or frame data, reorder the canonical
  transform stages, infer a profile, or claim arbitrary ICC matrix, TRC, or LUT execution. A profile
  change hides the native child until the GPU owner successfully presents the first frame through
  the replacement binding and transform, and both native commands and shell replies are revision
  fenced, so a stale or differently transformed frame cannot flash or replace current diagnostics.
- Nested open paths are root-anchored transient presentation state and may advance only through a
  currently visible clip whose source is the next timeline. Candidate placement rejects self or
  recursive dependency cycles, exact duration conversion gates replacement, and compound action
  construction preserves canonical object and track order. React never moves, rebases, links,
  groups, validates, or persists editorial objects; those policies remain in the timeline domain
  behind the existing project command and history owner.
- Project extension records use open namespaced kinds, bounded strict metadata, and exact opaque
  payload bytes. Capability grants remain a user-controlled subset, lifecycle and structured
  failure state remain durable and scriptable, unknown kinds survive unchanged, and engine runtime
  readiness remains derived rather than persisted.
- Runtime extension discovery is one bounded process-lifetime declarative registry. Exact versions
  may coexist, requested and granted capabilities remain distinct, only Ready features are
  Available, safe failures exclude raw diagnostics, and canonical snapshots change revision only
  when semantic state changes.
- Every discovered extension exposes the existing project command and editor-state resource as its
  sole user-control route. Discovery grants no authority and exposes no worker, callback, launcher,
  factory, path, dispatcher, permission token, or closed-tier engine backdoor.
- Autosave policy and monotonic elapsed state remain session-local and project-owned. Completed
  recovery points are complete current-schema `.superi` databases, count retention is ordered only
  by strict numeric generation, unknown and candidate files are preserved, and recovery discovery,
  restore, and dismissal remain separate owners. The project database owns active-file generation
  conflicts and cooperative replacement locks so autosave and engine recovery cannot overwrite a
  collaborator through a competing file authority.
- The graph is the render primitive, and timeline compilation is deterministic. UI state is not a
  hidden render input. Local AI and automation produce normal editable, undoable artifacts.
- The canonical slice keeps one typed editable graph state across timeline inspection, preview, CLI,
  scripting, and export. Stub stages remain visible and can never satisfy runtime conformance.
- Canonical working images are tagged, scene-linear, premultiplied RGBA 16-bit float, with ACEScg as
  the default space and explicit transforms at input, display, and output boundaries.
- GPU residency, bounded resource use, immutable render snapshots, explicit thread ownership,
  bounded channels, cancellation, backpressure, and device-loss recovery are architecture
  contracts, not optional optimizations.
- Source-derived metadata is read-only, bounded, and tied to one imported content fingerprint.
  Generic user metadata is revision-fenced, bounded, and stored beside the same media identity,
  while media IDs, content fingerprints, source paths, availability, and bin intent remain intact.
  C006 annotations occupy an adjacent typed field with bounded normalized content and atomic
  replacement; usage is a nonserialized projection of current timeline clip references. C007 adds
  exact fingerprint duplicate identity plus persisted bounded selections and manual tracked-region
  observations without changing that authority. C008 adds replaceable source-fresh derived-media
  attachments and explicit quality selection with deterministic original fallback. C009 adds local
  availability, relink, intentional replacement, and conform without changing stable media identity.
  C010 adds nonserialized bounded generated products behind exact project, library, media, and
  fingerprint fences; it cannot change any C008 or C009 authority.
  C011 adds source-bound ordinary transcript and local-content artifacts plus deterministic native
  search, retaining stale analysis for inspection while requiring explicit current-source confirmation
  before further editing. C012 applies only its named fields through one ordered bounded candidate,
  retains stable identity and source freshness, reports the failing operation index without partial
  commit, and advances one revision on success. C014 stores only exact in and out mark intent in the
  sidecar, binds it to one source fingerprint, rejects reversed marks atomically, and exposes stale
  runtime state instead of rebinding marks after identity changes. The retained monitor owns no
  project content, decoded frames, playback transport, cache, or GPU presentation. Runtime-only usage, duplicate grouping, smart
  membership, thumbnail, availability, and resolved-representation projections reach the consumer
  but remain absent from sidecar authority. C013 adds persisted accepted source baselines, current
  observations, volume state, scan generation, and relink intent without changing identity,
  editorial state, or derived choices. Changed bytes remain unaccepted until an explicit C009
  Relink or Replace path succeeds, and a changed source cannot be previewed, loaded, sought, or
  marked under its old fingerprint.
- The MIT tree rejects GPL, LGPL, AGPL, MPL, patent-encumbered in-tree codecs, and dependencies or
  models without adequate redistribution and provenance rights. Operating-system and vendor codec
  paths remain isolated and explicit.
- Workspace Rust denies unsafe code and undocumented unsafe blocks by default. Narrow native
  boundary allowances require local `SAFETY:` reasoning, inventory updates, and target-specific
  audit proof.
- Released fixture versions are immutable. Manifests exactly inventory payloads, derived fixtures
  retain parent lineage, and tests never download, overwrite, or silently regenerate missing data.
- Repository checkpoint coordination uses immutable main-tab specification text and a matching
  descriptions-tab ID. Active work is represented only by the exact timestamped claim suffix;
  completion replaces it with exactly three concise sentences beginning with `Implemented`, then
  highlights the complete main specification while leaving the ID and separator space untouched.
- Checkpoint planning produces exactly `plans/<id>/planning.md`, and execution evidence produces
  exactly `plans/<id>/execution.md`; additional planning documents are prohibited. Work synchronizes
  with `origin/main`, preserves all existing work, and never force pushes or uses destructive
  conflict handling.
- A tier 0 or tier 1 owner alone claims and completes Google Docs, integrates remote state, approves
  compatibility, reads source, plans, implements, tests, updates maps, reviews, commits, rebases,
  pushes, and closes the checkpoint inline. It never creates or delegates to a checkpoint subagent.
- External research is not a mandatory checkpoint stage. The owner uses it only when the live
  checklist, repository law, maps, current code, tests, local documents, and tool output cannot
  resolve a material question confidently.
- Mandatory map closure cannot be skipped. An optional map may be omitted only when the worker
  records and fully reads its module manifest, public entry points, cross-module interfaces, and
  relevant implementation and tests as the deeper substitute.
- Every module map reflects implemented reality, contains every discovered path, and becomes stale
  when its deterministic source hash changes. Generated maps and local plan files do not contribute
  to source hashes.
- Map freshness is both structural and semantic. Every assigned text file must be read through EOF,
  every inventory path and required section must remain present, and a hash-only update is invalid
  when surfaces, flows, relationships, invariants, tests, status, or risks changed.

## Tests and verification

The workspace documents define several proof layers. Five implemented workflows now cover the
open-tree boundary, hosted locked-workspace builds, dependency policy, a locked frontend toolchain
contract, and network-isolated execution of current core commands; every broader suite or physical
matrix remains a contract until a current workflow or fresh result demonstrates execution.

- `.agents/skills/superi-execution/scripts/verify_checkpoint.py` provides the deterministic local
  checkpoint floor. It derives changed files from the merge-base diff, staged and unstaged changes,
  and untracked files, validates changed Python and JSON syntax, always validates maps, and selects
  applicable repository contracts from paths and file kinds. Broad or uncertain work uses `--full`.
  Hosted CI status is not a general completion gate unless the assigned checkpoint explicitly
  requires hosted CI behavior.

- The production application gate installs the exact npm lockfile, reports dependency audit state,
  runs strict TypeScript checking, builds the hashed React bundle, and verifies pins, workflow
  routing, and ownership exclusions. The native gate checks Rust formatting, builds the Tauri mock
  host, and proves startup, exact acknowledgement, shutdown, restart, generation fencing, classified
  failure, immediate placeholder recovery, terminal behavior, and blocking-safe observation.
  Retained frontend and Tauri contracts continue to prove generated binding and toolchain
  compatibility without standing in for the product application.

- The focused process-runtime contracts prove failed-shutdown retry visibility, retained exit and
  blocking-task handles, registration-before-execution counts for fast tasks, closed admission, real
  EngineControl, Playback, and bounded-worker service counts, join-all stopped state, and idempotent
  GPU and persistence cleanup. The static frontend
  contract freezes the seven-service command and System panel projection, explicit cleanup calls,
  and absence of an operating-system child-process launch.

- The focused window-session proof validates strict session normalization and unknown-field
  rejection, missing-monitor relative placement, exact placement swap reversal, corrupt-file
  preservation and safe-primary recovery, atomic JSON round trip, close and reopen continuity, one
  real auxiliary webview creation through Tauri's mock runtime, independent webview connection
  generations, shared ordered authored events, and client-local disconnect. Its frontend companion validates exact
  command payloads, strict replacement admission, listener cleanup, real System panel and route
  hydration wiring, complete action visibility, and the primary-window-only native viewport
  boundary. The composed proof also freezes focused or last-active native menu routing and confirms
  the window owner cannot bypass project-preserving shutdown. The complete production frontend and
  desktop Rust suites provide widening coverage over the same owners.

- The focused desktop-shell contracts prove deterministic drop partitioning, path-safe document
  titles, busy and history-aware close decisions, sequence-fenced native state, exact recent and
  workspace intents, the focused palette menu intent, duplicate close suppression, one-shot resolution, reload-safe sequencing,
  complete schema 3 panel-layout and keyboard-shortcut persistence, schema 1 and 2 migration,
  duplicate placement and shortcut-ID rejection, independent corrupt-shortcut recovery, recoverable
  workspace persistence, active project restoration, and missing-document degradation with retained
  recents. Frontend shortcut contracts prove canonical key handling, transactional conflicts, native
  accelerator reservations, deterministic import and export, inactive-command retention, IME-safe
  dispatch, and reset behavior. Integration contracts keep shell synchronization on the primary webview,
  preserve window-owned routes while restoring panel presentation, and target commands to the
  focused webview. Strict TypeScript, the production build, the complete frontend contract set,
  focused native contracts, and the Tauri library suite provide the current integration floor;
  native menu appearance and operating-system interaction remain physical-lane evidence.

- The focused command-palette contracts prove deeply frozen application and desktop descriptors,
  duplicate rejection, path-derived recent identity, token-complete locale-independent ordering,
  current busy, project, undo, redo, and selection availability, typed delegation, actionable
  failures, and transient reducer state. The application architecture contract freezes the real
  modal, native menu mapping, production registration, and absence of generated API or Tauri imports
  from the pure catalog and view.

- The focused theme contracts prove one frozen schema-1 identity, deterministic repair of missing
  or drifted document declarations, frozen repair evidence, static pre-JavaScript declaration,
  activation before transport construction, semantic chrome and exact color-data namespaces,
  complete command-palette token adoption, and native viewer isolation. The application contract,
  strict TypeScript, production Vite build, complete frontend suite, viewer color-management tests,
  viewer analysis tests, and native viewport IPC contract widen the same boundary.

- The focused crash-diagnostics contracts exercise a real temporary application-data directory and
  prove unclean prior-session detection, exact dock, tab, size, visibility, focus, workspace, and
  project continuity, layout-free marker compatibility, orderly marker
  removal, all four recoverability classifications and their recovery entries, bounded journal
  retention, private panic-detail filtering at the public seam, corrupt-store archival, and
  nonblocking degraded startup. The frontend source contract separately freezes command wiring and
  reuse of the existing lifecycle and project recovery owners.

- The focused application-presentation proof exercises exact policy for retryable, degraded,
  user-correctable, and terminal conditions, fail-closed unknown classes, reviewed transport,
  project, lifecycle, crash, and last-valid context, immutable bounded notification replacement and
  dismissal, truthful public export progress, operational-status priority, and viewport-bounded
  menu placement. Source contracts freeze the real provider, tooltip, menu, notification, status,
  progress, failure, retained crash, panel pointer and keyboard, and delivery job consumers. Strict
  TypeScript checking plus the production Vite build exercise the integrated React path.

- The focused shell-capability proof composes real current-host GPU, audio, engine codec, public
  codec projection, and AI provider calls, validates exact audio sample ranges and explicit unknown
  channel meaning, proves monotonic observations and per-domain retained fallback, restores a valid
  private snapshot, replaces corrupt cache bytes, and keeps discovery outside every stream, route,
  session, model, project, and workspace mutation path. Strict TypeScript tests reject malformed or
  unknown bridge data and freeze exact meaning, while the app contract proves the production System
  panel consumes the authoritative owner. The complete frontend and desktop Rust suites widen the
  same boundary without claiming physical hotplug or native visual proof.

- The focused timeline-canvas proof freezes strict revision 2 parsing, exact source and record
  ranges, stable grouping, linking, selection, complete track control state, two-pass transition
  placement, deterministic ruler and range math, real editing-workspace composition, the complete
  general track controls plus audio routing, reversible shared selection identities, canonical fixed-point group and
  link expansion, direct and range selection, directional neighbors, lasso geometry, multiselect
  semantics, roving focus, keyboard coverage, live status, exact target snapping, transient
  navigation controls, all nine exact generated edit requests, source freshness and half-open
  conversion, all four three-point placement modes, equal-duration four-point editing, fit-to-fill
  rejection, exact cross-clock derivation, minimum fragment identities, shared target selection,
  replace conformance, immediate history requests, visible consequences, and the exclusion of any
  frontend authored mutation owner. It also proves complete visible timeline, track, object,
  inexact, and overscan
  marker projection, stable exact navigation, non-navigable state, all six command tags, and typed
  inverse reversal through the application owner. It freezes exact transition offset retention and
  production inspector wiring without introducing a second command path. The multicam proof adds
  exact active-angle projection, source-track creation, atomic action shapes, live switching,
  frame cut refinement, sync and audio intent, detach, accessible production mounting, and reuse of
  the same pending, command, revision, and history owners.

- The focused audio-video proof freezes exact selection by stable track identity, explicit video
  and audio role admission, generated link, synchronize, detach, replace, and complete routing
  actions, ordered named and discrete channel meaning, explicit mute decisions, incomplete-route
  rejection, lock and relationship disabled reasons, accessible production controls, shared pending
  exclusion, and publication only through the existing application project executor.
- The focused caption proof freezes complete canonical cue projection, millisecond SRT action
  planning with explicit gaps, supported WebVTT voice and style round trips, model-independent
  transcript text, timing, speaker, and relationship retention, stale-analysis rejection, selected
  caption editing, import and export discoverability, generated mutation ownership, TypeScript
  checking, the complete frontend suite, and the production Vite build.

- The focused timing-tool proof freezes all seven direct tools plus ripple delete and gap work,
  exact mixed-clock conversion, synchronized track ordering, typed allocation, immutable affected
  previews, preallocation lock and inexact-clock rejection, and the production shared-executor
  wiring. Lower timeline, engine transaction, API editor, and native transport contracts remain the
  semantic, atomicity, persistence, event, and history proof beneath that frontend compiler.

- The focused timeline-clip proof freezes reuse of that canvas model, exact source and record
  evidence, mixed clocks, stable media and relationship identity, relink, retime, marker, metadata,
  complete multicam angle, switch, and audio-policy detail, missing-angle rejection, graph effect
  and driver state, real audio automation samples, deeply frozen detail, preview freshness fences,
  accessible shared selection, and explicit malformed-state behavior.

- The focused editorial-feedback proof freezes distinct trim, slip, and slide viewer consequences,
  complete multicam context, exact 48 kHz and 96 kHz sample clocks, ordered source and destination
  channels, route targets, solo suppression, gap and source-discontinuity seams, explicit
  unobserved signal telemetry, deep immutability, application-owned cross-sibling publication, real
  viewer and meter consumption, route-state styling, and native placement-payload isolation.

- The focused viewer-cache proof freezes all eleven status fields, exact foreground frame and due
  clocks, predictive failure, decoded-output availability, nonblocking interaction, discard
  generation acknowledgement, transport synchronization, sample rates, ordered channel meaning,
  complete channel or mute routing, structural continuity seams, and explicit absent fill, hit,
  occupancy, device-output, and audible-sample evidence. Paused, playing, scrubbing, and ended
  modes retain exact temporal, visual, audio, cache, and comparison state through the real native
  viewer consumer, while malformed audio evidence fails closed without hiding independent state.

- The focused project-association proof freezes exact `.superi` Tauri metadata, portable startup
  arguments, macOS resource-open routing, nonblocking dispatch, the stable replacement event, and
  listener-first React consumption. Its native integration contract creates and reopens a real
  project, proves ordered filtering and deduplication, and retains the last valid identity, recents,
  and structured failure after an invalid associated path.

- The focused timeline-transition proof freezes exact from and to offsets, adjacent and
  opposite-edge capacity, deterministic start, center, end, and custom alignment, duration fitting,
  graph identity and downstream effect traversal, canonical scalar-bit decoding, host-owned and
  driven restrictions, editable scalar and choice commands, deep immutability, no-op rejection, and
  explicit graph-unavailable fallback. The editor-workspace contract proves the generated command
  remains owned by `ApplicationProvider`, while TypeScript, the complete frontend suite, and the
  production build exercise the real inspector consumer.
- The focused timeline-retime proof freezes identity, speed, reverse, freeze, and multi-segment
  draft behavior, exact rational reduction and cross-clock source seams, deterministic curve point
  insertion and removal, exact clip and track command targets, visible record and source
  consequences, no-op and unsafe-value suppression, duration and clock failures, real workspace
  mounting, accessible controls, shared pending state, generated command routing, Escape reset, and
  immediate use of the existing history undo owner.

- The focused media metadata proof freezes the C005 native, typed bridge, and React consumer
  contract. Native behavior tests cover identity and bin preservation, persistent generic metadata,
  missing-source inspection, stale freshness rejection, and C006 annotation-key rejection; the
  retained C004 contract protects bins, smart collections, list and grid views, and thumbnails.

- The focused media annotation proof freezes the C006 native owner, Tauri registration, typed
  bridge, and existing React media-detail consumer. Strict TypeScript checking and the production
  application build exercise the real consumer while C005 behavior continues to protect source
  facts, generic metadata, identity, freshness, and bin intent.

- The focused media identity proof freezes the C007 native owner, Tauri registration, typed bridge,
  and existing React media-detail consumer. It protects C006 annotations and derived usage while
  proving exact duplicate projection and ordinary editable selection and tracked-region state.

- The focused derived-media proof freezes the C008 native owner, Tauri registration, typed bridge,
  and existing React media-detail consumer. It protects C007 identity and tracked selection state
  while proving replaceable source-fresh attachments, explicit status and quality choice, and
  deterministic original fallback.

- The focused offline-media proof freezes the C009 native owner, Tauri registration, typed bridge,
  and production detail and search consumer. It protects the C008 attachment and fallback contract
  while proving online, partial, and offline state plus guarded relink, deliberate replacement, and
  exact conform behavior.

- The focused generated-preview proof freezes the C010 read-only state fence, native command,
  strict bridge, and selected-media inspector. Real still, sequence, WAVE, and unsupported-video
  inputs prove aspect-fit output, canonical sequence order, exact sample count across two decoded
  blocks, ordered stereo meaning, bounded PNG data URLs, stale-request rejection, and explicit
  unavailable products while the C003, C004, C008, and C009 contracts protect adjacent state.

- The focused media-batch proof freezes the C012 tagged operation union, native candidate owner,
  command registration, typed bridge, and real multi-selection consumer. Its integration test
  imports two real local sources, commits nine mixed operations at one revision, proves stable IDs
  and fingerprints plus original fallback for generating proxy and optimized choices, rejects a
  later mixed batch without partial state, checks derived-only sidecar exclusion, and reopens the
  committed state. The retained C004 through C009 contract ring and production frontend typecheck,
  tests, and build protect every adjacent consumer.

- The focused source-monitoring proof freezes the C013 persisted model, scan request, native command,
  strict bridge, and real inspector. Its integration tests use actual files to prove import-time
  baselines, forced exact byte-change detection, stable identity and editorial state, stale-request
  rollback, old-fingerprint preview rejection, accepted-byte return, mounted-volume file loss and
  restoration, durable reload, and absent conventional removable-volume intent. A private unit test
  fixes conservative macOS, Linux, system, and Windows volume classification.
- The focused source-monitor proof freezes the C014 engine source-only registry consumer, five
  Tauri commands, exact bridge DTOs, state-free workspace projection, and honest separation from
  decode and native presentation. One real WAVE proves content probe and open, retained 48 kHz exact
  seek, atomic fingerprint-bound marks, reversed-mark rollback, scanner-driven changed-byte stale
  state, operation fencing, sidecar reopen, and unload. A real three-frame PNG sequence proves its
  imported 24 fps inclusive range and overrun rollback. The same real sequence drives all four
  three-point placements and an equal-duration four-point edit through the retained generated
  project route, proves undo and redo, preserves fresh source-monitor state, and reopens the final
  revision 8 SQLite project. The
  focused engine unit proof requires the four stable source backends without codec runtime
  initialization, while media import, identity, preview, viewer, TypeScript, frontend, and Tauri
  gates protect adjacent consumers.

- Fresh local configuration proof parses `.codex/config.toml` with Python `tomllib`, confirms the
  exact Sol and max values plus the absence of an agent stanza, verifies that no project agent
  profile remains, and runs `codex --strict-config doctor --summary --ascii --no-color`. A
  noninteractive terminal capability error is an environment note rather than a configuration
  parse failure.

- The project autosave contract proves host-driven due boundaries, enable and disable control,
  forced manual recovery points, exact current-schema snapshot reopen, unchanged active-project
  bytes, strict generation ownership, timestamp-independent retention, foreign and candidate
  preservation, safe tamper rejection, generation exhaustion without schedule success, and retry.
  Its real engine consumer autosaves the selected history snapshot, including unknown extension
  state, after apply, undo, and redo.
  This headless proof does not claim an engine worker, wire adapter, UI, recovery choice, network
  filesystem semantics, or physical power-loss testing.

- The project save contracts prove stable active generations, stale-authority and stale-load
  conflicts, missing and malformed active-file preservation, explicit SaveAs and SaveCopy escape
  paths, and a real two-process same-generation race with exactly one winner and one visible
  conflict. A separate held-lock contract proves retryable classification. The engine recovery
  consumer proves a collaborator replacement after coordinator
  attachment preserves exact disk bytes, selected history, the recovery candidate, sequences, and
  events under a user-correctable conflict.

- The project extension contracts prove bounded plugin, auxiliary effect, AI artifact provenance,
  and unknown-kind envelopes, capability narrowing, user lifecycle and failure control, exact
  non-UTF-8 payload preservation, semantic no-ops, stale fencing, schema-4 persistence, lossless
  schema-3 migration, atomic save and autosave, engine history, compound transactions, dispatcher
  events, public mutation DTOs, and undo plus redo. They do not claim plugin process availability,
  graph factory registration, AI execution, or UI.

- The extension registration contracts prove one bounded engine registry, exact version coexistence,
  canonical change-only snapshots, requested and granted capability separation, ready-only feature
  availability, safe failure evidence, and read-only adapters over the real OpenFX and native audio
  supervisors. API contracts prove strict permission-free discovery, rejection of privileged wire
  fields, full replacement event and reconnect metadata, real permission-checked project control,
  and database reopen. Generated TypeScript, CLI schema, and frontend contracts prove consumer
  visibility without claiming a plugin transport, mutable browser registry, or product UI.

- `docs/checkpoints/P2.W05.C002.md` records the shared typed graph payload, concrete built-in effect
  schemas, caller-owned graph authoring, bounded CPU reference behavior, exact ROI and pixel
  semantics, timeline coexistence, research sources, and focused through full verification. It is
  evidence for a deterministic headless reference path, not production GPU, engine, viewport,
  playback, persistence, or export integration.

- `docs/checkpoints/P2.W06.C003.md` records the single-flight engine playback path through real
  graph, cache, color, audio, clock, worker, and viewport contracts. Its deterministic integration
  proof covers normal, degraded, backpressured, and recovered operation, but does not claim source
  session preparation, transport controls, native GPU presentation, physical hardware, or export.

- `docs/checkpoints/P2.W06.C006.md` records the concrete engine consumer of the shared A/V scheduler
  and actual audio clock. Focused and foreground integration contracts prove bounded holds,
  corrections, protected and eligible drops, applied rebases, exact timing preservation,
  backpressure without duplicate decisions, and continuous clock recovery. This deterministic
  evidence does not replace the platform matrix's physical audio, performance, or soak lanes.
- `docs/checkpoints/P2.W06.C005.md` records the engine transaction from prepared source reads through
  decode, shared graph evaluation, delivery and audio stages, deterministic encode, strict semantic
  validation, and complete elementary stream publication. Its contract covers normal, degraded, and
  recovery behavior, real acquired WebM and WAVE paths, exact PCM completion, and rejection of VP9
  duration drift without claiming muxing, persistence, native GPU readback, or public API control.
- `docs/checkpoints/P2.W06.C004.md` records the transport owner layered over that foreground path.
  It proves exact discontinuity supersession, rational cross-timebase cadence, protected intent,
  bounded ordinary dropping, prediction cancellation, queued-audio discard, backpressure, and
  recovery while preserving C006 live synchronization evidence. Decoded source binding, native GPU
  presentation, public dispatch, and physical hardware remain with their owners.
- `docs/checkpoints/P2.W06.C011.md` records deterministic recursive OpenFX discovery, strict bundle
  validation, the platform worker-launch contract, classified containment, exact permission
  narrowing, restart and quarantine recovery, and shared graph availability for playback,
  rendering, and export. Its real child-process fixture proves process boundaries without claiming
  a concrete XPC, AppContainer, Linux sandbox, native OFX ABI, or GPU-handle transport.

- `docs/checkpoints/P2.W06.C007.md` records the first engine-wide typed dispatcher and public
  transaction plus ordered event seam. Focused engine, API, and CLI contracts prove atomic rollback
  and one-unit commit, revision fencing, coherent playback, rendering, and export admission through
  degradation and recovery, exact event agreement, legacy compatibility, and headless consumption.
  A fifth real transport contract proves the capacity-one nonblocking EngineControl-to-Playback
  command bridge, overtaking prevention, degraded denial, recovery, complete replacement state, and
  structured command-failure evidence. Three export dispatcher contracts prove stable submit,
  automated progress and completion events, inspection, pause, fresh resume and retry, dependency
  release, cancellation, degraded denial, recovery permits, typed result retention, removal, and
  blocking-safe shutdown over the canonical logical queue. This in-process proof does not imply
  wire transport or a broad production project transaction.

- `docs/checkpoints/P2.W07.C016.md` records the generic project editor adapter over that dispatcher.
  Its strict parity proof covers apply, inspect, undo, redo, six action groups, and every current
  timeline, graph, media, clip-mix, and extension mutation. A real mixed fixture proves one revision,
  one history unit, one correlated event, exact database reload, and fresh undo plus redo revisions
  without claiming C017 full snapshots, CLI routing, scripting, subscriptions, or wire transport.
- `docs/checkpoints/P2.W07.C022.md` records the bounded local script language and stable runtime
  contract. Focused API, catalog, generated binding, CLI discovery, persistence, integrity, media,
  autosave, and recovery proofs cover exact-source validation, deterministic interpretation,
  initial and later conflicts, committed-prefix visibility, and nested permission denial without
  claiming arbitrary code, filesystem loading, operating-system sandboxing, or whole-script
  atomicity.
- The asynchronous job API contracts build on that canonical queue rather than introducing another
  scheduler. Focused API, schema, and CLI consumer proof covers strict handles, stable weighted
  priorities, unit progress, every cooperative control, deterministic dependency and handle order,
  user-safe failures, ordered completion events, and typed-result non-exposure. Public submission,
  host polling as a wire method, typed artifact retrieval, persistent queue recovery, muxing, and
  publication remain outside this checkpoint.

- The focused effects preset contract verifies the runtime JSON and SHA-256 edges through canonical
  current documents, legacy migration, integrity rejection, and exact graph reload. Crate-wide
  effects and graph suites prove downstream compatibility; dependency and boundary gates remain the
  repository proof for approved direction and offline behavior.

- `.github/workflows/dependency-policy.yml` runs on pushes, pull requests, and manual dispatch. Its
  Ubuntu 24.04 job first runs `.github/scripts/check-dependency-policy.sh`, then checks approved
  licenses and sources with cargo-deny against `open/Cargo.toml` using all features. This is
  automated policy enforcement, not evidence that the workspace builds, tests, works offline, or
  passes any physical platform lane.
- Fresh local verification for this refresh executed the dependency-policy contract successfully.
  cargo-deny then reported `licenses ok, sources ok` for the open workspace with the new rusqlite
  and bundled SQLite resolution. The unused `Unicode-DFS-2016` allowance produced the documented
  non-failing warning.
- `docs/checkpoints/P1.W07.C006.md` records prior YAML parsing, formatting, diff, prose-dash, focused
  shell, license, and source checks plus successful initial GitHub Actions run `29302533491`. Those
  are durable checkpoint claims; only the shell and cargo-deny checks above were rerun during this
  map refresh.
- `.github/workflows/ci.yml` enforces the locked open-tree boundary, then formats, builds, tests,
  strictly lints, and documentation-tests the workspace on five pull-request and `main` lanes, plus
  Ubuntu 22.04 on weekly or manual runs. Each job also builds and tests the supported `os-codecs`
  CLI configuration, validates canonical fixtures, and runs the normalized eight-stage slice
  contract into fresh runner paths. YAML parsing and all six lane-ID presence checks, preview
  policy, disabled credentials, one locked boundary command per job, complete two-job command
  coverage, exact Linux `libva-dev` and cross-platform x86 `nasm` provisioning, checksum-pinned
  libvpx 1.16.0 provisioning, and the hosted macOS native-test condition passed during this refresh.
- `.github/scripts/check-ci-features.py` verifies the explicit feature policy for all five matrix
  lanes, the default-only extended job, the exact CLI build and engine/API test commands, and the
  absence of accidental all-feature expansion. Fresh local execution and YAML parsing passed.
- `docs/checkpoints/P1.W07.C003.md` records the rav1d 1.1.0 checksum and policy review, focused AV1
  and registry contracts, default and `os-codecs` consumer builds and tests, critical workspace
  verification, and the required unchanged hosted Windows proof.
- `docs/checkpoints/P1.W07.C008.md` records fresh Rust 1.80 formatting, eight focused boundary
  contracts, the canonical scan of 304 files and 23 manifests, warnings-denied focused Clippy,
  workflow syntax, a locked full workspace build, and the complete workspace test and documentation
  suite from the checkpoint-owned target. Full strict workspace Clippy reached only pre-existing
  missing safety comments outside the boundary tool.
- `docs/checkpoints/P1.W07.C002.md` records the focused red-to-green workflow contract, low-risk
  verification scope, isolated local proof, hosted workflow requirement, and delivery context for
  the complete Rust quality suite.
- `docs/checkpoints/P1.W07.C001.md` records that the workflow's red-to-green contract, YAML parsing,
  immutable checkout, lane mappings, locked workspace build, diff hygiene, and prose-dash checks
  passed. It also records a local `cargo build --workspace --locked` on stable Rust 1.97.0 and all
  seven fixture-tool policy tests plus its documentation tests offline. These are durable local
  checkpoint results, not hosted results for every configured lane.
- `.github/workflows/frontend.yml` performs `npm ci`, `npm run typecheck`, `npm run build`, and
  `npm test` under exact Node.js, TypeScript, and Vite versions. The contract tests require strict
  no-emit checking, immutable actions, read-only credentials, every independent gate, the committed
  generated API import, typed negotiation request and response, typed extension discovery snapshot
  and control reference, typed maps, the transport-neutral client surface, and a hashed JavaScript
  entry in the generated production bundle.
- `cargo test -p superi-api --features typescript-bindings --test typescript_bindings_contract`
  proves deterministic API rendering, complete canonical registry coverage, required typed maps,
  extension query, event, resource, lifecycle, capability, safe failure, and control declarations,
  and absence of timestamps or checkout paths. `cargo test -p superi-api-bindings` proves
  idempotent generation plus nonmutating missing and stale checks, and `cargo run --locked -p
  superi-api-bindings -- check` proves the committed artifact is current.
- The production application contracts now include four pure nesting tests for exact mixed-clock
  catalog projection, cycle-safe path and candidate behavior, strict placement, and deterministic
  compound action construction. The expanded workspace, clip-detail, and binding contracts prove
  visible nested navigation and command wiring, child-timeline detail scoping, and generated DTO
  parity. Fresh strict typecheck, production build, and the complete 54-test frontend command passed
  for this checkpoint.
- `.github/workflows/tauri.yml` runs the four blocking Rust gates across two macOS architectures,
  Windows, and Ubuntu. Fresh local proof passed both workflow contracts, formatting, the Tauri mock
  runtime test, strict all-target Clippy, and native macOS wry compilation from the checkpoint target.
- `.github/workflows/network-isolated.yml` prepares locked inputs and test executables on Ubuntu
  24.04 after building checksum-pinned libva 2.22 and libvpx 1.16 and installing nasm, then uses a
  distinct empty network namespace and Cargo offline mode for workspace tests, canonical fixture
  validation, and the CLI consumer. Hosted run `29308007012` stopped before isolation because the
  former distribution libva API 1.20 could not satisfy the H.266 API 1.22 boundary. Run
  `29382902840` reached the distinct namespace after all artifact preparation passed, then exposed
  that host-mounted sysfs did not represent that namespace's interface inventory. The harness now
  uses `/proc/net/dev`; the final delivered run is authoritative because the local macOS host cannot
  execute Linux `unshare --net`.
- `docs/checkpoints/P1.W07.C004.md` records a fresh clean npm installation, typecheck, production
  build, three passing contract tests, zero reported vulnerabilities, negative TypeScript and
  missing-bundle controls, YAML parsing, and a complete locked Rust test run. These are delivery
  results for the isolated contract, not proof of a React application or Tauri shell.
- Root and open-tree guidance call for workspace build, test, strict Clippy, documentation tests,
  default and optional codec feature coverage, and the real CLI or vertical-slice consumer.
- `docs/platform-testing.md` defines stable `toolchain`, `features`, `fixtures`, `malformed`, `gpu`,
  `codecs`, `audio`, `slice-contract`, `slice`, `performance`, and `soak` suites. Hosted lanes prove
  source, CPU, fixture, and contract-slice portability, while named physical lanes are required for
  real GPU, display, audio, hardware codec, all-runtime slice, performance, and long-session evidence.
- `docs/unsafe-ffi.md` requires a repository unsafe scan, all-feature strict Clippy, Windows-target
  Clippy for Media Foundation, strict audio Clippy, codec and VST3 contracts, and all-feature engine
  tests after native-boundary changes. Real lifecycle tests still run on the owning operating
  system; the VST3 contract supplies real macOS lifecycle proof in an isolated child.
- `docs/checkpoints/P2.W04.C013.md` records the macOS Audio Unit host's safe configuration,
  background preparation, default verified isolation, callback and teardown ownership, real Apple
  Peak Limiter graph consumer, exact timing and channel proof, dependency audit, and deferred
  engine, plug-in-management, and physical-lane boundaries.
- `docs/checkpoints/P2.W04.C014.md` records format-neutral state persistence, native Audio Unit and
  VST3 state transfer, fixed graph delay compensation, isolated timing-matched dry fallback,
  deterministic discovery and worker validation, checkpoint recovery and quarantine, per-node
  database reopen proof, and the remaining concrete IPC and physical-lane boundaries.
- `docs/checkpoints/P2.W04.C012.md` records the VST3 host's exact configuration, retained module and
  COM lifecycle, canonical channel and timing behavior, bounded automation and monitoring, isolated
  real-module contracts, dependency audit, and deferred orchestration boundaries.
- `open/test-fixtures/README.md` defines
  `cargo run -p superi-fixture-tool -- check test-fixtures` as the shared validation path and
  `cargo run -p superi-fixture-tool -- generate-video <OUTPUT_DIRECTORY>` as the video reproduction
  path. It defines `cargo run -p superi-fixture-tool -- generate-audio <OUTPUT_DIRECTORY>` as the
  synchronized multichannel audio reproduction path. The timing reproduction path is
  `cargo run -p superi-fixture-tool -- generate-timing <OUTPUT_DIRECTORY>`. The color and sequence
  reproduction path is `cargo run -p superi-fixture-tool -- generate-color <OUTPUT_DIRECTORY>`.
  The media-error reproduction path is `cargo run -p superi-fixture-tool -- generate-media-errors
  <OUTPUT_DIRECTORY>`. The OTIO reproduction
  path is `cargo run -p superi-fixture-tool -- generate-otio <OUTPUT_DIRECTORY>`. The UTF-8 fixture's
  payload is 6 bytes and its SHA-256 is
  `5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03`, matching its manifest.
- The canonical slice contract passed a focused semantic check for its scenario identity, exact
  fixture and edit, one effect, eight stages, conformance levels, default offline boundary,
  adjacent checkpoint ownership, and root README discovery. These checks validate the contract and
  existing repository policy, not a runtime slice.
- Fresh fixture validation reports 11 valid fixture versions and 27 payloads. The video fixture
  contains 207 cases in a 13,419-byte binary. The audio fixture contains three 100 ms PCM16 WAVE
  cases. The timing fixture contains five cases and 18 samples in a 1,041-byte catalog. The color
  fixture contains eight images, three sequence frames, and 448 raw sample bytes. All six generated
  baselines are protected by generator, CLI, consumer, and canonical-root contracts.
  The media-error fixture contains one strict catalog and four tiny PCM containers whose production
  consumer proves three open failures and one explicit partial packet with exact corruption evidence.
  OTIO fixture adds two timelines with exact 48-frame and 120-frame durations plus explicit
  unsupported-object expectations. Production `superi-timeline` maps them to ordinary native
  project state and emits deterministic JSON; official OpenTimelineIO 0.18.1 proves semantic read,
  target write, and reread equivalence for both Rust-produced outputs.
  The encoded slice fixture is a digest-bound 28,178-byte AV1 WebM. Current expectation version 2
  contains a 995,328-byte 48-frame RGBA8 reference payload plus exact audio, timing, portable state,
  and export records. Focused engine, API, and CLI contracts prove exact canonical state, strict
  public projection, atomic revision-fenced transactions, ordered replacement event agreement,
  checkout-independent project identity, eight-stage reports, expectation
  evidence, collision safety, two-run reproducibility, hosted command coverage, honest stub
  disclosure, all-stage timing and resident-memory records, and an exact observed-boundary maximum.
- Phase 0 completion additionally requires written codec counsel, a Tauri, React, and native wgpu
  viewport demonstration on all three operating systems, reference-validated OTIO interchange,
  color reference proof, and named subsystem owners. The in-process API command and ordered event
  round trip now has focused engine, API, and CLI proof, but no wire transport proof.
- The mapping script is the structural proof for this map. Its focused requirements are the exact
  source hash and file count, one occurrence of every required heading, every owned path in the
  source inventory, a global-index link for every discovered module, and no Unicode em dash or en
  dash.

No test result is inferred from a documented command. A virtual adapter cannot satisfy a physical
lane, an unimplemented suite is a gap, and a retry retains its original failure evidence.

## Current status and risks

The workspace is beyond the original empty scaffold and the public orientation now identifies the
first production desktop shell. `app/` contains a locked React 19 and Tauri 2 application whose
single lifecycle owner projects explicit application and headless-engine phases, intent, revision,
generation, pending acknowledgement, classified safe failure, and failed-shutdown retry. A linked native process owner now
retains one real dispatcher per generation on a dedicated EngineControl thread plus one exact
transport runtime on a dedicated Playback thread, exposes the existing
integration-validation result over a bounded nonblocking Rust connection, and performs real orderly
dispatcher shutdown before lifecycle acknowledgement. One separate shell-local process runtime now
lists the exit monitor, retained project association tasks, both engine domains, bounded workers,
GPU submission, and window persistence with exact phases, counts, pending joins, and thread names.
It closes task admission before teardown, joins all owners on normal exit and setup rollback, retains
the first cleanup failure only after later joins run, and lets the System panel show that state without
creating a second lifecycle or job owner. Internal subsystem readiness remains truthful;
the production React bootstrap now consumes the complete generated TypeScript contract through a
transport-injected `SuperiClient` provider with identical request and automation behavior. One thin
native dispatcher forwards integration validation through the retained EngineControl owner and
routes generated editor reads, exact playback commands, and durable project commands through the active project lifecycle. It
emits ordered generated introspection and project replacement events and exposes bounded replay,
reconnect, cancellation, and all four public recoverability conditions through the concrete
frontend transport. The transport now isolates generation, pending, cancellation, replay cursor,
and disconnect state by webview label while retaining one authored event sequence, so every
restored auxiliary window observes the same engine and project replacements without invalidating a
sibling connection.
The process also owns one strict persistent window session. It restores one primary plus bounded
auxiliary workspace windows, reconciles unavailable monitor identities into visible placements,
retains exact fullscreen and prior placement state, preserves rejected session bytes, exposes
recovery and persistence phase visibly, and atomically coalesces updates off the event loop. The
System panel provides precise create, focus, fullscreen, monitor move, undo, close, and reopen
actions. Its persistence worker and the GPU submission domain now expose explicit idempotent join
results to the shared process owner. Workspace routes restore before ordinary route persistence, main close remains orderly,
and auxiliary native GPU viewports remain intentionally unavailable because the existing presenter
owns one primary-window role surface set.
Application command shortcuts are now configurable through one accessible System panel and one
immutable schema-1 profile. The live dispatcher, sidebar hints, editor rows, import and export, and
reset behavior all consume the same effective table; collision and native-reservation failures are
visible without partial mutation. Native schema 3 persists the profile beside workspace presentation,
migrates schema 1 and 2 records to defaults, and recovers an invalid profile independently from a
valid workspace. Physical platform keyboard-layout and assistive-technology coverage remains a
separate lane beyond deterministic model, built-consumer, and headless contract proof.
The same production shell now owns a bounded local crash journal and exact session marker independent
of public engine and authored project state. It retains safe actionable context across sessions,
classifies retryable, degraded, user-correctable, and terminal conditions, and exposes workspace,
engine, and project recovery entry points in the System panel. Raw panic detail remains native-only,
corrupt or unavailable storage becomes visible degraded state instead of preventing startup, and
orderly acknowledged shutdown clears only the current session marker after complete process cleanup.
The native shell now also owns one schema-1 capability observation across GPU adapters, audio input
and output declarations, codec backends and operations, and honest local AI availability. It runs
provider discovery off the main thread, retains bounded validated last-known evidence across
sessions, reports current per-domain failures, and exposes manual refresh through the real System
panel. The snapshot is operational metadata only: it never creates a GPU device or codec session,
starts or reconfigures audio, loads a model, or changes project, route, workspace, and editable
artifact authority.
Above that unchanged transport, `application.ts` owns deterministic route, panel, and command
registries plus immutable per-route four-dock layout, ordered tabs, bounded sizing, hidden placement,
active-tab and focus reconciliation, structural default and custom status, all-route default reset,
one exact transient reset undo, complete private continuity projection, and one immutable typed
public-resource selection. `panel-workspace.tsx` is the real consumer with labeled dock and hide
controls, pointer and keyboard separators, ordered drag docking, accessible tablists, mounted
inactive tabpanels that retain component-local state, and one shared pointer or keyboard context
menu that routes activate, dock, and hide intent back to the same reducer.
`ApplicationProvider` composes the framework above `SuperiApiProvider` and remains the single owner
of transient application state plus one last-valid public editor snapshot. It correlates playback
transactions and observes bounded completion through that same snapshot while leaving timing and
command execution on the native Playback owner. One outer application-presentation provider now
normalizes already classified shell and public evidence into an always-visible status bar,
expandable notification and recovery center, bounded notifications, semantic progress, accessible
tooltips, and a keyboard-operated context menu. The four recovery classes remain distinct, safe
context and last-valid references remain visible, retained crash diagnostics provide cross-session
operational evidence, and recovery callbacks reuse the existing owners. The React shell registers
editing, compositing, color, audio, delivery, and system routes; the professional views project the
same snapshot, preserve exact audio sample, channel, route, synchronization, and continuity fields,
and retain classified degraded state when the native bridge reports a failed generated request. The
native bridge now routes complete editor-state and generic project-command methods through a retained
EngineControl editor session, publishes successful snapshots durably, refreshes desktop project
identity, and emits exact correlated project events. The editing viewer adds play, pause, stop,
loop, JKL shuttle, exact variable speed, direction, and frame-step controls, plus complete exact
temporal, visual, audio, synchronization, comparison, pending, failure, and degradation readouts.
Every source, program, composite, and color native viewer now also memoizes one frozen status
projection from those existing owners. The visible display uses checked exact rescaling, the
canonical global start, project-owned non-drop or supported NTSC drop-frame label mode, a separate
continuous frame number, topmost enabled source identity and exact ranges, physical drop counters,
transport scheduling, nonblocking predictive and foreground frame-cache evidence, visual
degradation, audio discard acknowledgement, canonical sample clocks, ordered channels, routes,
continuity, explicit audio-cache output limits, comparison, and current selection, group, link,
target, and sync-lock intent. Invalid or unobserved owners remain explicitly unavailable, and the
status list stays outside the transformed frame across paused, playing, scrubbing, ended, and every
shell navigation mode. Cache fill, hit, occupancy, prediction completion, buffer fill, device
output, and audible samples remain unavailable rather than inferred.
The Program viewer now also resolves exactly one current selected clip into its timeline-scoped
canonical graph and exposes every reachable exact built-in transform in node order. All nine finite
matrix scalars and nearest or bilinear sampling remain ordinary typed graph parameters; drivers make
their owned inputs inspectable but read-only, while Apply and identity reset publish one ordered
changed-only graph action through the application-owned revision fence. Missing transforms and
stale, ambiguous, malformed, unsupported, or changed graph state remain explicit, authored matrices
never enter the shell-local CSS navigation transform, and refreshed project state remains the only
successful result authority.
The same production bootstrap now owns one fixed versioned application theme without adding
application state. Static HTML declares the dark identity and browser color before startup;
`theme.ts` validates or repairs only that metadata and publishes frozen recovery evidence before the
desktop transport is constructed. `theme.css` centralizes shared chrome values, while viewer and
marker color-data namespaces preserve their exact prior values and native viewer isolation prevents
CSS filters, blending, opacity changes, or forced-color substitution from changing presentation.
The theme never reads system preference or persistence and does not receive ICC bytes, frames,
scene values, display selection, transform stages, project state, or workspace continuity records.
The current production runtime is intentionally timing-only and reports unavailable viewport pixels
and audio output instead of claiming decoded presentation or samples.
Every native viewer role now also owns one frozen comparison presentation with single, compare,
split, wipe, difference, reference, and snapshot modes. Capture controls preserve exact native
surface generation, frame sequence, physical extent, display intent, and available source or
playback rational navigation time, explicitly labeled as unbound from the native frame; missing
native evidence keeps dependent modes disabled. The program summary crosses the existing
application owner into playback controls, while navigation, overlays,
external-display intent, canonical project state, and playback commands remain unchanged. The
control-only Tauri payload remains free of pixels and texture handles. The shell does not retain reference textures or perform scene-linear
difference rendering, and it labels that native render-result binder gap rather than simulating
pixels in React.
Independently, the four native viewer roles present their existing managed canonical role textures
through one GPU owner. They now select image, alpha, red, green, blue, luminance, fixed false-color,
or display-linear clipping on the production `GpuDisplayPresenter`, retain navigation and overlays,
and report requested, selected, queued, and last-presented state without frame IPC. Those initial
textures remain placeholders until a later decoded timeline renderer binds canonical results, so
this analysis path does not turn timing-only playback into decoded presentation.
Each role can now also select one external monitor other than the display containing the editor
window. The native owner keeps the inline child active, routes the same canonical role texture to a
second borderless wgpu surface on the one GPU thread, rejects a target already used by another role,
and reports the exact connection-local target, extent, scale, analysis selection, presentation
identity, and unavailable or failed state. The React selector owns only transient routing intent;
navigation, overlays, comparison captures, analysis, exact temporal context, visual and audio
status, and canonical project state remain unchanged. Tauri monitor identities are intentionally
connection-local routing keys and do not claim the exact platform ICC or HDR identity owned by the
color profile path.

Every native viewer role now also exposes the exact current system display catalog and one
independent color selection. On macOS the native GPU owner refreshes CoreGraphics observations into
the existing transactional ICC catalog, records exact profile content identity or explicit
unprofiled state, binds the role to a selected monitor, and rejects removal or freshness drift before
acquire and before submit. The selected sRGB or Display P3 intent rebuilds the real
`GpuDisplayPresenter`, preserving canonical ACEScg scene-linear RGBA16F input and the six-stage
transform order. A changed selection hides that role's native child and carries a boxed reveal token
with the GPU command, so only a successful current-revision presentation restores visibility. React
also rejects out-of-order command replies before replacing its frozen diagnostics. It receives only
bounded immutable profile metadata through a separate strict control command, so navigation,
playback, overlays, comparison, status, and native placement remain unchanged. Non-macOS system
discovery remains explicitly unavailable, and neither the shell nor `superi-color` evaluates
arbitrary ICC matrix, TRC, or LUT tags yet.
The editing view now also parses the snapshot's canonical timeline document into a frozen
identity-preserving canvas with sticky headers, an adaptive ruler, record-positioned tracks and
items, a frame-snapped playhead, an explicit range, native scrolling, pointer-anchored zoom, and
canonical variable lane height. Track creation, deletion, inline naming, height, order, target,
lock, sync lock, mute, solo, and enable gestures enter the application-owned generated command and
refresh from the durable replacement snapshot.
Caption tracks now expose exact language, purpose, cue text, speaker, bounded style, relationships,
and metadata. Selected cues edit those values through one generated caption mutation batch, while
their timing remains on the existing exact editorial gestures. SRT, WebVTT, and fresh language
analysis create ordinary editable millisecond tracks with explicit gaps, and export derives only
from the current canonical snapshot.
Marker creation, exact range, label, flag, note, and removal gestures use that same command owner,
persist through the selected project, and retain one revision-scoped complete inverse for immediate
reversal. The same canvas projects every authored marker plus exact timeline, item-edge, and
timeline, track, or object marker-edge targets. It exposes a session master plus six target-kind
rules, reports every accepted coordinate, draws the active or last guide, keeps inexact or overscan
markers visibly non-navigable, and reverses transient pointer gestures through their captured origin.
Existing clip items now add real generated filmstrips, thumbnails, and waveforms when available,
plus exact source, relationship, effect, driver, marker, metadata, multicam, retime, and clip-gain
automation evidence with positioned keyframe diamonds.
Trim, slip, and slide plans now publish exact source and program consequences through the existing
application owner. Multicam detail retains complete angle identity, enabled state, switch ranges,
and audio policy. An ordered audio rack shows canonical sample clocks, channel routes, destinations,
audibility, and seam evidence beside an explicit unobserved signal state. The DOM feedback remains
outside native child placement, and no live RMS, peak, true-peak, or loudness path exists in the
current editor snapshot.
Selected transition items now add exact handle, duration, alignment, endpoint, graph, driver, and
parameter evidence. Start, center, and end placement preserves the entered total, impossible
placements are disabled, and all authored changes use the application-owned revision-fenced project
command path before a complete snapshot refresh.
The canvas now projects exact current-revision timeline-object references through the shared
application selection, follows canonical groups and enabled links to a fixed point, preserves
Option direct-object intent, and supports click, toggle, contiguous range, mounted-rectangle lasso,
and roving keyboard selection with visible focus and live status. Those controls retain only
transient viewing and interaction intent. The same timeline exposes exact track targeting,
source-monitor range and engine state, insert, overwrite, append, replace, all four three-point
placements, equal-duration four-point editing, lift, extract, backspace, undo, redo, visible
consequences, pending results, and actionable failures. Inclusive source out marks become half-open
operation bounds, cross-clock derivation must be exact, and unsupported fit-to-fill is reported
before submission. Canonical authored selection
and shared application selection remain distinct until an explicit replace or backspace uses the
shared target. All authored changes still pass through the existing generated command, engine
history, timeline operation, and durable project owners.
Exactly one selected video clip and one selected audio clip now expose durable link, exact
source-time synchronization, detach, and audio replacement controls. The same panel projects every
audio track's sample rate, ordered semantic source channels, destination, and complete route, then
publishes one full channel map with explicit per-source mute choices. Disabled, pending, completed,
and failed intent remains visible, while successful state arrives only from the refreshed canonical
snapshot.
One directly selected clip also exposes exact speed, reverse, freeze, and multi-segment time-map
controls with an inspectable curve, source traversal, target, and consequence. Applying a retime
uses the same generated command and history path, while reset, undo, and redo preserve the existing
application ownership boundary.

The timeline now projects every canonical sequence into one cycle-aware catalog and reconciles a
root-anchored transient open path through actual nested clip edges. Breadcrumb, back, selected-clip,
and double-click navigation retarget the same canvas and supplemental inspector to child timelines.
Cycle-safe source and target selectors publish exact append or equal-duration replace nested actions,
while selected objects publish one deterministic selection-derived compound action with fresh child
timeline, child-track, and parent-instance identities. Pending, success, and failure intent remains
visible, authored results refresh from the canonical snapshot, and neither navigation nor React
creates a competing editorial or history owner.
One selected nested clip now also exposes a strict multicam setup and editing panel. It derives
eligible source angles from nonempty child video tracks, projects canonical sync provenance,
switches, active angle, cuts, and audio intent, and publishes create, attach, take, frame refine,
sync, audio, and detach gestures through the same generated project action callback. Immediate undo
uses the existing history command. The angle viewer reports engine-authored program state separately
from source-media availability at the exact playhead and does not claim decoded tiles or runtime
multicam mixing.
No view takes engine or transport ownership, and unavailable runtime behavior remains honest.
The always-mounted application shell now restores workspace presentation through the live registry,
including route-local dock placement, tab order, active tabs, sizes, visibility, and focus. Its
header reports structural layout condition plus acknowledged continuity progress, performs a
deterministic all-route reset with one-step undo, and polls the authoritative native engine lifecycle
for an always-visible state that routes detailed control to System. It keeps
native menus synchronized with active project and session-history availability, opens project
and media paths through native dialogs or unambiguous drops, and resolves close requests through
busy checks, durable save, and an explicit history-loss warning. Tauri owns the stable menu surface,
path-safe document title, recent mapping, native clipboard roles, private workspace record, and
one-shot close resolution into the existing application lifecycle. The schema 2 workspace record
migrates schema 1 state without inventing removed panel identities, and the crash marker retains the
same complete layout with additive layout-free compatibility. The desktop project lifecycle
separately restores its active path and bounded recent records through the durable local host. Undo
and redo remain intentionally
session-local, file associations remain assigned to P3.W06.C003, and menu fidelity remains subject
to physical macOS, Windows, and Linux verification.
The same application shell now exposes one bounded searchable palette over registered route, panel,
selection, file, recent-project, import, history, and quit actions. Every descriptor has a stable
automation identity, frozen discovery metadata, current availability and disabled reason, and one
typed invocation route. The fixed global shortcut and focused native Edit menu open the same
transient modal; query and highlighted-result state are never persisted, and project mutations
continue through the existing generated command or desktop project owners.
The production document now declares one schema-1 `color-critical-dark` theme before JavaScript,
and `main.tsx` reconciles that declaration before transport construction and React rendering. The
pure theme owner repairs only the root identity, schema, scene-owner metadata, and browser color,
then returns frozen ready or recovered evidence. Shared shell and palette chrome consume semantic
tokens, while viewer surrounds, analysis overlays, comparison colors, and authored marker flags use
separate exact color-data tokens. The native viewer frame rejects forced-color substitution and
keeps full opacity, normal blending, and no CSS filter, so React styling cannot reinterpret native
GPU pixels or reorder the existing display transform.
The System panel also consumes one Tauri-owned project lifecycle that durably creates, validates,
saves, rebinds through save-as, closes, reopens recent paths, and restores opaque recovery
candidates while retaining actionable classified failure context beside the last valid state. It
also observes development file-association opens through one listener-first replacement event.
Portable startup arguments and macOS resource-open URLs enter the same native Open command on a
blocking worker, unrelated resources are ignored, and the main window is restored without reloading
the active workspace. It now inspects and atomically updates frame-rate, resolution, color, audio
sample-rate and channel, cache, proxy, and working-folder authority through that same lifecycle. The
same direct consumer now organizes C003 media identities into durable hierarchical bins and
predicate smart collections,
switches between list and grid presentation, derives freshness-aware source thumbnails or
deterministic fallbacks without persisting derived media, refreshes bounded source facts with
explicit ready, missing, or unavailable state, and edits bounded generic user metadata without
changing source identity or organization. The same selected-media detail atomically replaces typed
clip names, labels, ratings, keywords, comments, and favorite intent, then shows nonserialized usage
counts derived from current timeline clip references. It now also derives canonical and duplicate
identity from exact fingerprints and atomically persists reusable rational-frame selections with
manually refinable fixed-point tracked observations. The same detail now owns the C008 derived-media
lifecycle and transparent switching, then derives local availability and provides bounded local
search plus explicit relink, source replacement, and frame-rate conform flows. The same native owner
now persists model-independent transcript and local AI content entries, validates exact source timing,
speaker, timeline and clip relationships, and provides deterministic revision-fenced metadata,
transcript, and local-content search with explainable evidence. React consumes that query under bin
and smart-collection scope and edits the ordinary artifacts through atomic replacement. The selected
media path also generates thumbnail, filmstrip, waveform, and preview products on demand under exact
revision and fingerprint fences. Supported stills and image sequences produce bounded pixels and
canonical representative frames; WAVE produces channel-separated exact sample and routing evidence.
Generated data remains ephemeral, and unsupported video, compressed-audio, EXR, and DPX surfaces
remain honest unavailable states. The same browser also selects
many visible identities and commits ordered numbered rename, active-bin organization, generating
optimized transcode or proxy records, root-based relink, and metadata edits through one atomic
revision-fenced batch. Responses carry refreshed runtime projections, while the sidecar retains only
durable authority and derived choices. The same stable identities now retain import-time accepted
source baselines and expose metadata-efficient all-source checks plus exact selected-source byte
verification. The inspector distinguishes changed bytes, missing files, unavailable paths, and
offline conventional removable volumes, retains explicit relink intent, and blocks changed sources
from preview generation under stale freshness. Actual transcode bytes, automatic local-AI analysis,
native filesystem notifications, and automatic background scan scheduling remain separately owned.
The editing workspace now also retains one
source-monitor session with explicit empty, ready, or stale engine state, source-only container
loading, exact rational seek, and reversible fingerprint-bound in and out marks. Its native GPU
source viewport remains the independent presentation owner; the monitor does not decode or present
frames. The timeline consumes that state without adding a second editor session and sends every
point edit through the existing generated project command, engine history, and durable replacement
flow.
Fresh Cargo metadata expands the member globs to 25
packages: 19 crates under
`open/crates/` plus the `superi-fixture-tool`, `superi-dependency-check`,
`superi-boundary-tool`, `superi-bench`, `superi-test-report`, and `superi-api-bindings` repository
utilities. The
lockfile includes a substantial
GPU, image, codec, serialization, platform, and native-build dependency graph, and current codec,
image, platform, and unsafe documents describe implemented contracts rather than empty placeholders.
Its `superi-api` package record now includes one test-only concurrency edge for the real engine
introspection ownership contract without changing the production runtime graph.
The API now owns a nonserializable host permission context, typed lexical filesystem and plugin
scopes, explicit destructive operations, deny precedence, payload-derived requirements, and schema
`1.9.0` discovery metadata. It also projects one bounded engine-owned extension registry through
strict exact identity, lifecycle, capability, feature, safe failure, stable control, query, event,
and replacement resource contracts. Its bounded `superi-json` runtime uses already resolved JSON and
packages, adds the typed command-log query step, and preserves the same nested authorization and
project command owner. The CLI exercises that boundary with one
exact canonical fixture-read grant for the scenario path and a separate deny-by-default local
policy context for durable project workflows; authentication, final symlink confinement, and
operating-system sandboxing remain host and I/O-owner responsibilities.
The effects crate now owns a substantive graph-native authoring SDK, exact animation curves,
complete reusable presets, explicit checked schema migration, and strict authored wires. Its preset
lockfile change records direct runtime use of already-resolved JSON and SHA-256 packages for
canonical integrity-protected documents, while effects-to-project integration and production native
plugin execution remain incomplete. The effects-side isolated OpenFX contract and engine-side
bundle discovery, launch coordination, containment, and graph availability are implemented, while
concrete platform transport, native OFX ABI adapters, and GPU-handle IPC remain absent.
The project crate now owns a stable schema-5 SQLite application database with deterministic
timeline, graph, settings, authored audio, and extension component rows, separate metadata and
opaque-payload SHA-256 evidence, checked in-memory replacement, checked reload, durable
nonoverwriting create, read-only reopen, and an ordered exact
schema-0-to-schema-1-to-schema-2-to-schema-3-to-schema-4-to-schema-5 migration inside one immediate transaction.
It also
owns authoritative versioned settings plus one typed save, save-as, copy, and backup surface that
builds, validates, closes, synchronizes, and atomically publishes complete same-parent current-schema
candidates under explicit collision policy, with active-path rebinding, validated active-file
generation fencing, a persistent sibling operating-system writer lock, and honest postpublication
state. Its clockless autosave controller adds host-driven monotonic scheduling, strict managed
generations, complete Backup recovery points, bounded count retention, explicit pruning, and typed
user control without another persistence model. Recovery discovery, comparison, exact dismissal,
and engine-coordinated restoration are implemented, and a changed active generation now blocks
recovery before history mutation. The lockfile records exact rusqlite 0.32.1 and libsqlite3-sys
0.30.1 with bundled SQLite, exact `fs4` 1.1.0, project Serde and JSON, plus engine rusqlite test
edges. Bounded typed generic command records now persist outside authored semantic state, with exact
request digests, bounded replay bytes, cursor-safe queries, and active recovery lineage. Additional
project schema revisions beyond 5, persisted undo and redo branches, public dirty-state hashing,
and transport-catalog database adaptation remain incomplete. The API-owned local host and
CLI now compose existing database open, publication, recovery, and validation authorities without a
direct CLI dependency on the project crate.
The engine now owns a production Rust compound project command and history boundary around that
aggregate. It applies bounded ordered timeline item, track, graph, media, authored audio, extension,
root, existing-child nested placement, and selection-derived compound actions inside
one outer project edit, preserves nonconflicting retained graph work through a three-way recompile,
records one immutable before-and-after unit, restores undo and redo targets with fresh monotonic
revisions, persists only the selected snapshot through the existing database, and exposes one
correlated dispatcher event. Plugin, effect, AI artifact provenance, and unknown future extension
records preserve exact payloads and user-controlled lifecycle without duplicating runtime plugin
readiness. The generic public project command, typed evidence, correlated history event, complete
stable editor snapshot, and local scripting runtime are implemented. CLI project or script
execution now routes through the durable local API host and bounded JSON-RPC automation. Logging,
subscription hosting, and autosave hosting remain incomplete.
The synchronized remote revision before this checkpoint is
`33ff1f7b542a9412ecb90fd70a97855ec31c718e`.
Commit `217e9d48703bcfd4736d949aea510c94505071bc` added the dependency-policy workflow and aligned the
root README, deny policy, and structure guide with license-audit CI. Commit
`e0b3af9f099f527a8544d1b0317896640969903b` added the executable dependency-policy contract and its
durable record. Commit `68c007309c3c548d28c2001c1673c61c57da3ac0` added the cross-platform hosted
build workflow and durable `P1.W07.C001` checkpoint record. Commit
`cb1fe287c5ca3d9f5fd91d25c1a4b90b70594867` added the locked frontend CI contract and durable
`P1.W07.C004` record. Commit `036149c0a5df6901553b7bce5e34f4c323e1c240` added deterministic raw-video
generation, canonical video artifacts, a real media-I/O consumer, and its durable checkpoint record.
Commit `b47ff18f2072075d46cb61ca86c7e71123bda9e2` added deterministic synchronized audio generation,
three canonical WAVE fixtures, production PCM-source consumer proof, and its durable checkpoint record.
Commit `19453e5d946ff16f8d5e5c1fa084ab201f0621b4` added deterministic timing generation, the canonical
cadence catalog, media-I/O timing consumer proof, and its durable checkpoint record.
Commit `b06751610ca9e4ca2d4030c79cf43f1f6c3a533f` added deterministic color and image-sequence
generation, canonical artifacts, production color and media-I/O consumer proof, and its durable
checkpoint record.
Commit `218e110c9cecc2ab9fa1304fceeb116a52ff93f3` added deterministic media-error generation,
canonical malformed and partial-read fixtures, production PCM consumer proof, and its durable
checkpoint record.

Commit `6e0d1d36ec30ee04de81d4ad01b8a7748785619b` added deterministic OTIO 0.18.1 generation, the canonical slice and
coverage timelines, explicit preserve plus diagnose expectations, a timeline-owned semantic
consumer, official reference proof, and its durable checkpoint record. It deliberately does not
implement the later production model, reader, writer, or graph compiler.
Commit `892ecfeba17e8bd12a1fe746d9e7b271d6e5cfae` added bounded stage timing and resident-memory
instrumentation, schema 1.1.0 report evidence, focused process proof, and its durable checkpoint
record.
Commit `5649d9075b29eef79b181caa880a650a59786ae1` added the independent canonical expectation fixture,
strict CLI consumption, reference frames, audio and timing proof, and its durable checkpoint record.

The independent audio processing graph now provides deterministic audio-owned topology,
destination-scoped preparation with fixed intermediate buffers, exact consecutive block
processing on the concurrency-owned audio domain, and typed submix, auxiliary, send, return, and
single-master routing. Graph preparation now propagates every processor's fixed latency and
preallocates exact per-route delay compensation before callback publication. Borrowed prepared input
views and stable route-ID summing preserve channel meaning and avoid callback allocation. Public consumers prove both a source-to-gain chain and a
dry-submix plus auxiliary-return path, including atomic topology rejection and order-sensitive
floating-point behavior. Explicit prepared channel nodes now convert canonical mono through 7.1
layouts using documented speaker rules or caller-selected discrete order without changing sample
time. macOS effect Audio Units now enter the same graph processor boundary through exact component
identity, bounded background preparation, process-location verification, semantic channel
negotiation, preallocated pull callbacks, and poison-on-native-failure ownership. A real Apple Peak
Limiter consumer proves adjacent partition continuity through the terminal master. Audio Unit
class-info property-list state and native latency now round-trip through the host. The worker-side
VST3 host restores and captures exact component and controller streams and reports fixed latency.
One format-neutral digest-checked envelope preserves native state plus sample-clock and latency
evidence, while the prepared isolated bridge always advances timing-matched dry fallback. Engine now
owns deterministic candidate discovery, strict separate-process worker validation, activation,
checkpoint capture, restart, quarantine, and one state record per audio node through real project
save and reopen. Audio Unit instruments, MIDI, broader parameter automation, preset browsing, UI,
concrete platform IPC and sandbox launchers, dynamic latency rebuild, and decoded-sample binding
remain absent. Production device output and
sample-accurate scheduling are implemented in the same audio crate, and engine foreground playback
now feeds its bounded producer and coordinates video from its actual presentation clock with
explicit hold, correction, drop, rebase, and recovery evidence. Engine transport requests
callback-owned discard generations across control discontinuities and explicitly mutes inactive or
non-normal sample pacing, but no engine owner yet renders prepared timeline audio through routing,
effects, resampling, and device delivery.

Engine render-export consumes an acquired media owner through exact seek, complete packet reads,
decode drain and flush, immutable graph evaluation, caller-owned delivery color or audio processing,
one-shot encoder selection, encode drain and flush, exact provenance and packet validation, and
fresh-context reset recovery. It returns complete in-memory elementary packet streams only after a
current lifecycle permit is rechecked. The stable API now inspects and cooperatively controls jobs
already attached to the canonical export queue, but it does not submit prepared executors, expose
host polling or typed results, mux, or publish artifacts. Container muxing and publication,
arbitrary stream counts, native GPU readback, and application submission remain separate gaps.

The effects crate now provides a substantive graph-native authoring SDK. It composes canonical graph
schemas, editable nodes, deterministic registry snapshots, and snapshot-bound compiler translation,
and its public contract proves the same definition and typed edit path in timeline-role and
node-graph-role graphs. Built-in effects, image or GPU execution, timeline attachment, engine
integration, persistence, and rendered output remain absent.

The effects crate now provides the first concrete built-in visual node catalog, generic graph
authoring, bounded CPU reference evaluation, and real immutable graph workflow proof. The timeline
compiler admits the same neutral processing payload while retaining every editorial value exactly.
Production GPU implementations, engine registration, playback, viewport, persistence, UI, and
export remain absent and cannot be inferred from the reference path.

The largest current risk is cross-document drift:

- Application notification history and menu state are intentionally process-local presentation;
  cross-session operational evidence comes only from the native crash, workspace, project, and
  lifecycle owners. Future feedback work must not imply that dismissing a notice dismisses native
  evidence, infer progress when a public total is absent, expose private panic detail, or move retry
  and restart authority into React. Pointer, keyboard, focus, screen-reader, high-contrast, and
  reduced-motion behavior still require physical application-lane validation beyond source,
  TypeScript, unit, and production-build proof.
- A future light theme or broad token replacement could accidentally reinterpret analysis overlays,
  comparison evidence, authored marker flags, or native viewer presentation. Any additional theme
  must preserve the separate semantic and color-data namespaces, exact viewer isolation, static
  recovery contract, monitor binding, display intent, canonical precision, and transform order.
  The current implementation intentionally has no system-preference branch or persisted theme
  choice.

- Persistent window state is application-shell intent, not project authority. Monitor routing IDs
  are derived from local Tauri monitor facts and may change with operating-system topology; restore
  must continue to reconcile them into visible bounds and preserve one reversible prior placement.
  Auxiliary windows share generated engine and project state but do not own the current singleton
  role-addressed native GPU surfaces, so future presenter work must establish an explicit
  multi-webview surface ownership model before removing that honest boundary.

- Viewer timecode labels and physical playback drops are separate domains. Future status work must
  preserve the continuous physical frame index, apply drop-frame rules only to labels at supported
  rates, retain exact clock representability, and never convert missing or malformed observations
  into zero or success. The active program display currently reports the canonical clip source
  range because the shared canvas projection does not expose the current point through a retime map.

- Viewer cache presentation has only transport scheduling, degradation, discard-generation, and
  canonical authored audio evidence. It must not turn absence of `prefetch_failure` into a cache
  hit, derive occupancy or fill from a scheduled frame, infer device-backed audio from routing, or
  describe structural continuity as audible output while the timing-only runtime reports viewport
  and audio output unavailable.

- Editorial audio feedback currently has canonical structural authority but no runtime meter-reading
  owner. Route-state bars must remain labeled as routing and audibility evidence, and signal status
  must remain unobserved until playback publishes real per-channel readings with exact sample and
  channel identity. UI code must not infer silence or level from authored routing state.

- The canonical fixture, independent expected contract, reference project and graph state, strict
  API projection, and contract runner now exist, and every stage reports bounded timing and
  resident-memory boundary evidence. The hosted workflow directly runs fixture validation and the
  portable eight-stage contract, and expectation version 2 removes checkout location from project
  identity. Generic typed DAG storage, cycle prevention, schema-bound editable nodes, atomic graph
  mutation transactions, native timeline-to-graph compilation, a shared typed processing payload,
  and a deterministic CPU reference effect catalog now exist. Production GPU effect evaluation,
  source and timeline session preparation, native viewport integration, rendered comparison,
  muxing, playable output, and all-runtime execution remain future work. Foreground engine playback
  now composes prepared graph, cache, CPU display color, bounded audio, audio-master A/V
  coordination, worker, and viewport contracts, including deterministic late correction and
  discontinuity recovery, and exact interactive transport now controls that prepared path. Decoded
  source and audio binding, native presentation, application export submission, artifact
  publication, and broad public dispatch remain open. Public asynchronous job inspection and
  cooperative control alone do not close those broader paths. The current contract-conformant
  run must not be reported as product or runtime conformance.
  Boundary samples are not continuous intra-stage peaks, constrained-device thresholds, or
  long-session soak proof.

- `open/docs/STRUCTURE.md` still labels offline CI and the vertical slice as deferred. The four
  workflows now cover dependency policy, locked hosted compilation with the
  open-tree boundary scan, the frontend toolchain contract, and one network-isolated core path, but
  that path prepares dependencies online and must not be mistaken for product behavior, a complete
  offline build, full feature or malformed-input coverage, UI, shell, slice, or physical-platform
  enforcement.
- `docs/codecs.md` still says cargo-deny will be wired into CI in a later pass even though
  `.github/workflows/dependency-policy.yml` and `open/deny.toml` now define that enforcement. The
  codec policy's status sentence is stale, although its narrower claim that offline CI is required
  remains accurate.
- `docs/phase-0-build-contracts.md` says encumbered codec implementation may not begin before written
  intellectual-property counsel review and still lists that review as outstanding completion
  evidence. `docs/codecs.md` and `docs/unsafe-ffi.md` describe concrete H.264, HEVC, ProRes, AAC, and
  VVC platform implementations. This map contains no counsel artifact that resolves the apparent
  policy-versus-implementation conflict.
- The lockfile captures platform and transitive packages beyond the three declared desktop targets,
  including dependencies pulled by wgpu and winit. Lockfile presence does not imply that Android,
  WebAssembly, or other targets are supported products.
- `open/rust-toolchain.toml` follows floating `stable`, while workspace package metadata promises a
  Rust 1.80 floor. The hosted workflow still installs floating stable and there is no recurring
  Rust 1.80 lane. The text checkpoint freshly checks the affected `superi-effects` all-target graph
  and the project persistence and autosave checkpoints freshly check `superi-project` with Cargo and
  Rust 1.80.0 against the locked compatible dependency resolution, but those focused local proofs
  are not a recurring whole-workspace hosted guarantee. An engine-wide Rust 1.80 all-target check
  reaches the unchanged rav1d 1.1.0 use of `std::ptr::fn_addr_eq`, which is unavailable on that
  compiler, before it can establish an engine MSRV lane; current hosted and local engine gates use
  the repository's stable toolchain.
- The dependency-policy workflow uses third-party actions by major version tags rather than commit
  digests. It grants only read access, but action-version immutability is not enforced by this file.
- The shell contract check is intentionally exact-line based. It catches deletion or textual drift
  in the required coupling, but it is not a general YAML parser, shell security audit, or proof of
  action behavior. The following cargo-deny step remains the semantic dependency-policy check.
- The checkpoint and fresh local run intentionally check only licenses and sources. Advisory and
  duplicate-version policy checks remain outside this CI contract, cargo-deny cannot replace human
  or legal review, and the configured unused Unicode license allowance still warns.
- The checkpoint record embeds the initial workflow commit and run, but refers readers to the
  canonical checklist for the exact follow-up SHA instead of naming
  `e0b3af9f099f527a8544d1b0317896640969903b` in the record itself.
- The initial GitHub Actions run is recorded as successful, but this refresh did not rerun the
  hosted workflow at the follow-up commit. Fresh local Bash and cargo-deny checks prove the current
  checked-out contract, not hosted-run containment.
- The cross-platform workflow now runs the complete Rust formatting, locked build and test, strict
  Clippy, and documentation-test suite. Default workspace tests include the focused PCM media-error
  contract, and supported lanes also run the explicit `os-codecs` CLI, engine, and API consumers.
  Broader malformed-input matrices remain intentionally separate.
- The first C009 hosted run after nasm provisioning, `29308007012`, failed before namespace entry
  because Ubuntu 24.04's libva API 1.20 was below the unchanged H.266 API 1.22 requirement. Both Rust
  workflows now use one checksum-pinned libva 2.22 source helper; hosted reruns remain required.
- Hosted macOS omits only three native VideoToolbox or AudioConverter lifecycle tests because the
  documented physical codec lane owns that evidence. The workflow keeps their names and rationale
  explicit; it does not weaken or remove their source contracts.
- Ubuntu 22.04 has weekly and manual triggers but no distinct release-event trigger. Manual dispatch
  can supply release evidence, but this workflow does not automatically enforce that release cadence.
- Ubuntu 26.04 is explicitly experimental and nonblocking while its hosted image remains preview.
  Its configured run is useful evidence but cannot yet satisfy the matrix's future blocking state.
- The CI checkpoint record delegates the exact delivered SHA to the canonical checklist and does
  not record a completed hosted run across the six configured lanes. The current workflow and local
  proof are implementation evidence, not proof that every hosted runner completed at this revision.
- The frontend workflow validates a deliberately minimal TypeScript and Vite contract. It now has a
  real compile-time consumer of the generated public API including playback and command-log method
  and resource maps, but no live transport, React dependency,
  Tauri host, native viewport, or editorial behavior, so a passing frontend lane must not be
  reported as product UI or desktop-shell proof.
- The frontend lockfile includes many platform-optional esbuild and Rollup packages. Their presence
  describes npm's portable dependency resolution and does not make those platforms supported Superi
  targets or prove a native frontend build outside the Ubuntu workflow.
- The governing `AGENTS.md` is ignored and absent from the mapping script's repository inventory.
  Changes to repository law therefore do not change this module's source hash, which makes manual
  rereading mandatory even when this map validates as current. The tracked `.codex` project
  configuration contributes to the hash and travels through ordinary Git worktrees.
- Hosted workflows are not the routine agent completion gate. The local deterministic verifier plus
  checkpoint-specific proof is authoritative for the agent workflow unless a checkpoint explicitly
  owns hosted CI behavior; this does not convert a failing hosted lane into passing product evidence.

This map is based on the synchronized `origin/main` revision plus this uncommitted checkpoint, so
`mapped_at_commit` is `working-tree`. The remote base was
`17de32b778420f90d4d82b3ea7d03f1baaf370a6` when this checkpoint began. The later C009
command-palette delivery at `d7fd4a19afa0a3de76d1aa07a813e7e1471b89cb` and C010 configurable
shortcut delivery at `9a31f7fdd88ba4a05f1108ca74c1024498015d0a` were integrated before final verification. Its hash
describes the resulting workspace sources, contracts, durable checkpoint records, command palette,
configurable shortcuts, and shared application-presentation model and consumers.

## Maintenance notes

Run the mapping script from the repository root. Use `files workspace` to establish the complete
owned-path reading assignment, read every listed text file through EOF, use `hash workspace` only
after source changes settle, and run `validate` after map synthesis. The script hashes each owned
path, a null separator, its exact bytes, and another null separator in sorted path order, so path
changes and content changes both invalidate the hash.

Refresh this map whenever a root document, repository skill, project Codex configuration,
workspace manifest, lockfile, toolchain policy, deny policy, shared fixture, or other
non-crate and non-tool source changes. Reconcile prose,
not only frontmatter: update membership counts, dependency and feature descriptions, proof commands,
status claims, cross-module relationships, and identified conflicts. Use `working-tree` while mapped
sources are uncommitted; otherwise record the revision whose source bytes were mapped.

For implementation maintenance, begin with `validate`, use `changed --base <revision>` and the
actual source diff to locate affected ownership, reread every changed file plus related contracts and
tests through EOF, then update affected maps, contract-dependent consumer maps, and the global index
before recomputing metadata. Rerun `validate` after the final integration or rebase and immediately
before delivery. Regenerate and reconcile a conflicted map from the resulting source rather than
choosing one side or preserving a stale hash.

For checkpoint delivery, record the synchronized base revision before edits and run
`python3 .agents/skills/superi-execution/scripts/verify_checkpoint.py --base <base-revision>` after
integration. Use `--full` when path selection may not cover the change and record the selected local
commands plus results in `plans/<id>/execution.md`. Run every additional proof required by the
checkpoint because the verifier is a minimum, not an exhaustive semantic test plan.

Always reread `AGENTS.md` even if the workspace hash is current. It is operational law outside the
generated inventory. Also inspect any future binary assigned to this module by file type, size,
producer, provenance, manifest, and consumers rather than treating its bytes as prose.

The root and open READMEs, compact structure guide, Phase 0 contracts, codec policy, unsafe audit,
and platform matrix overlap intentionally but currently disagree in status. When implementation
changes, update the most authoritative contract and every public status summary that would otherwise
mislead a contributor. Keep planned requirements clearly separate from code paths and test evidence
that exist and have run.
Keep the committed TypeScript artifact, CLI schema consumer, production app adapter, frontend smoke
consumer, API map, engine map, and global index synchronized whenever extension identity, lifecycle,
capability, feature, failure, control, query, event, resource, or reconnect behavior changes.
Discovery must remain a declarative projection of authoritative runtime owners and must never imply
a privileged frontend, CLI, closed-tier, or plugin execution route.
Keep shell capability provider calls, Rust DTOs, strict TypeScript parsing, the System-panel
projection, retained-cache validation, focused contracts, provider maps, and the global index in
sync. Preserve exact audio sample ranges and explicit channel-layout knowledge, retain current
failure beside fallback data, and never let discovery start a stream, select a route, create a GPU
device or codec session, load a model, or mutate authored and workspace state.
Keep file associations limited to the canonical `.superi` document type. Startup arguments and
platform resource URLs must normalize into local paths, remain off the main thread during durable
validation, and enter the sole `DesktopProjectState` Open transition. Emit replacement state for
both success and failure, preserve the prior active project on failure, and never turn the React
listener into a second document, recent-history, persistence, or workspace owner.
Keep native desktop menu IDs and typed frontend intents synchronized. Workspace restoration must
filter removed registry identities, place newly registered panels by their declared region, retain
each hidden panel's last dock, keep every visible dock's active tab explicit, clamp interactive
sizes to the native bounds, and preserve window-owned routes during ordinary shell restoration.
The native schema must reject duplicate routes, docks, or routed panels, hidden active tabs, invalid
focus, and out-of-range sizes before live mutation. Keep layout condition derived from structural
hidden, placement, order, and size intent, keep reset scoped explicitly to every route, and keep its
exact undo transient and invalidated by later workspace intent. The header engine projection must
remain a read-only poll of the native lifecycle owner and route detailed control or recovery to
System. Project-session restoration must revalidate the active path,
recent records must stay bounded and deduplicated, title projection must never expose parent paths,
and exactly one approved close resolution may enter orderly lifecycle shutdown. Never serialize
engine history into either private shell record or let dialogs, drops, or menus bypass the existing
project, media, application, or generated command owners.
Keep command registry defaults, canonical token normalization, the schema-1 shortcut model, provider
dispatch, System panel, sidebar hints, native accelerator reservations, desktop schema migration,
and focused contracts synchronized. Profile changes and imports must remain transactional, unknown
command IDs must remain bounded and inactive, export must remain deterministic, and corrupt shortcut
state must never discard a valid workspace or enter authored project persistence.
Keep the application theme fixed, versioned, and presentation-only. Declare it before JavaScript,
activate it before transport or React construction, restrict recovery to document attributes and
browser theme metadata, and retain frozen ready or recovered evidence. Keep shared chrome on
semantic `--theme-*` tokens, exact viewer and authored marker meaning on separate `--viewer-*` and
`--marker-*` tokens, and native viewer presentation at full opacity with normal blending, no CSS
filter, and protected color-data rendering. Adding a theme requires a product decision plus focused
recovery, token, viewer, color-management, build, and full-frontend proof.
Keep crash diagnostics shell-local and bounded. Preserve private panic details only in the native
journal, project only reviewed context through Tauri, validate every retained route, dock, ordered
panel, active tab, size, hidden state, focus, and
project before restoration, and route retries, restarts, and project recovery through their existing
owners. Never clear an active-session marker without matching its exact session identity and
confirmed orderly shutdown.
Keep application presentation adapters exhaustive over the four existing recovery classes and
fail unknown values closed. Preserve safe source, code, action, contexts, last-valid identity, and
continuity while the underlying snapshot remains rendered. Keep menus, tooltips, notifications,
status, and progress on the React presentation side, route commands to existing owners, and never
let dismissal, menu selection, or progress rendering become project, job, lifecycle, crash, or
workspace persistence authority.
Keep desktop process ownership exhaustive and shell-local. Any new long-lived thread or detached
task must enter the stable inventory, retain its handle, close admission before teardown when
applicable, and join on both setup rollback and normal exit. Register task ownership before allowing
execution to finish so active counts never exceed retained handles. Keep the process snapshot
read-only, attempt all joins after a failure, and never clear crash evidence before every owner is
stopped.
Keep the timeline canvas projection synchronized with the canonical timeline document revision,
exact rational clocks, stable identities, and relationship fields. Keep application selection
references revision-fenced and reversible, mirror the lower fixed-point rule exactly, keep group
expansion unconditional, keep link expansion behind the canonical flag, and retain direct-object
selection. View navigation, lasso geometry, focus, and interaction selection may remain local, but
authored selection, relationships, and edits must route through the existing project, engine, and
public command owners.
Keep viewer status presentation-only and role-aware. Project settings own the display rate and
timecode mode, timeline owns the global start, source and record ranges, identities, relationships,
and editorial intent, engine playback owns physical drops and transport, visual, audio, degradation,
and failure observations, and the retained source monitor owns its current source coordinate and
fingerprint. Exact rescaling must fail closed, omitted drop-frame labels must never alter continuous
frame identity, active-item lookup must preserve half-open range and topmost enabled-track rules,
and status rendering must remain outside native placement and transformed-frame ownership. Frame
cache display may report only exact foreground scheduling, due clocks, predictive failure, and
output availability. Audio cache display must preserve discard acknowledgement, exact sample clocks,
ordered source and destination channels, complete routes, and audited continuity while keeping
occupancy, hit, prediction completion, buffer fill, device output, signal level, and audible samples
explicitly unobserved when no authoritative owner publishes them.
Keep viewer transform projection strict and selected-clip scoped. Validate canonical envelopes,
schema references, graph revision, node and port topology, driver targets, clip identity, exact
`superi.effect.transform` 1.0.0 parameters, and finite scalar bits before exposing controls. Preserve
canonical graph order, keep every driver-owned value inspectable and read-only, emit only changed
ordinary parameters through the application command owner, and never use authored matrices as
viewer-local CSS navigation state.
Keep viewer color state presentation-only and role-aware. The frontend catalog must mirror exact
native monitor and profile identity, catalog generation, selected transform, display intent, and
canonical stage order without carrying ICC bytes or pixels. Preserve the separate strict color
command, refresh and bind through `superi-color`, recheck freshness around native presentation,
rebuild the real presenter for sRGB or Display P3, and keep arbitrary ICC evaluation explicitly
unimplemented until a tested evaluator and real consumer exist.
Keep nested catalog projection complete across every timeline in the canonical document. Open paths
must remain root-anchored, clip-edge validated, transient, and self-healing after authored changes.
Keep candidate filtering cycle-safe and duration conversion exact; keep compound mapping canonical
and caller-identified. Every placement or compound mutation must use generated DTOs and the existing
application command callback, never a React-owned mutation, history, or persistence path.
Keep snap candidates exact in the timeline edit clock, preserve the lower target class and stable tie
order, skip inexact cross-clock coordinates, resolve object markers relative to their owner, and keep
session switches, rule filters, visible consequences, and gesture origins transient. Later authored
clip gestures must call the lower snapping and edit owners rather than treating the React resolver as
an authored operation.
Keep durable marker controls behind the application project command callback. Preserve complete
create metadata and owner state, retain every authored marker in the visible model, omit inexact
coordinates only from navigation and snapping, and clear typed inverse reversal when its exact
refreshed revision is superseded.
Keep caption projection strict and complete. Import must remain byte and cue bounded, exact in
milliseconds, nonoverlapping, and fail closed on unsupported syntax. Transcript conversion must
retain project revision and source fingerprint fences, and every resulting text, timing, speaker,
style, and timeline relationship must become ordinary canonical editable state. Export must read
only the refreshed model, while React form and parsed-file state remain transient.
Keep clip detail supplemental to that exact projection. Graph badges must follow real clip-scoped
topology, keyframes must come from a legal attached owner, previews must retain project, library,
media, and freshness fences, and authored selection must remain distinct from shared UI selection.
Keep editorial feedback derived from the exact active plan and canonical replacement snapshot.
Preserve full multicam angle, switch, and policy identity, keep audio track and channel order stable,
and retain route, audibility, and continuity meanings. Do not add numerical signal fields until a
real runtime observation owner exists, and keep all feedback outside native placement IPC.
Keep viewer analysis codes synchronized across the frozen TypeScript catalog, strict Tauri enum,
and `GpuDisplayView`. Preserve selected versus last-presented diagnostics, scene-linear inspection
before the display transform, clipping after display-linear conversion, the image compatibility
constructor, canonical RGBA16F residency, and the absence of pixels or texture handles in IPC.
Keep external viewer routing connection-local and control-only. Exclude the editor window's current
display, preserve signed virtual-desktop positions plus exact physical extent and scale, reject
cross-role target conflicts, and deactivate stale hotplug identities instead of guessing. Inline
and external surfaces must sample the same managed canonical role texture through the one GPU
submission owner, while an external surface failure remains isolated from inline presentation.
Navigation, overlays, comparison, analysis, temporal, visual, and audio state must retain their
existing owners, and external routing must not claim monitor ICC or HDR policy it does not apply.
Keep transition timing on canonical timeline offsets and processing intent on typed graph values.
Handle inputs must retain exact decimal strings until safe public conversion, alignments must
preserve total duration, opposite-edge transitions must reduce available capacity, and driven,
host-owned, or unsupported parameters must remain noneditable. The React inspector may retain only
transient form state and must submit through the application-owned generic project command callback.
Keep retime drafts presentation-only and exact. Require one direct clip target, canonical decimal
input, positive record durations and denominators, reduced signed rates, safe wire integers,
complete record coverage, and exact source seams before producing the generated retime operation.
Curve controls must state their target and consequence, while apply, undo, redo, and durable refresh
remain with the existing command owner.
Keep audio-video selection tied to immutable track and clip IDs, never reversed display indices.
Preserve exact record timing in link, synchronization, and detach previews, reuse the existing
replace owner, and require a complete ordered route for every source channel. React may own only
draft channel choices, disabled reasons, and pending presentation; native timeline and engine owners
remain authoritative for timing, relationships, channel validation, history, and publication.
