---
module_id: workspace
source_paths:
  - repository files outside open/crates/* and open/tools/*
source_hash: fc5f84579e4caafd4708f7d1ab7a027b05d2b77be13030d1830dee0d3f565b38
source_files: 134
mapped_at_commit: working-tree
---

## Purpose and ownership

The `workspace` module owns the repository-level product definition, architectural contracts,
license and codec policy, build sequencing, operating-system test policy, unsafe-boundary audit,
Cargo workspace configuration, dependency lock, shared test-fixture contract, and repository-owned
agent workflows. Runtime implementation under `open/crates/*` and repository utilities under
`open/tools/*` belong to their own module maps. This map therefore explains the constraints and
coordination layer around those modules rather than duplicating their internal APIs.

The root `AGENTS.md` is the highest-authority operational law for work in this checkout. It routes
checkpoint assignments, fixes the paired-tab Google Docs claim, blocked-note, highlight, and
three-sentence completion protocol, requires safe synchronization with `origin/main`, and makes
current mandatory maps plus complete selected raw-file reads a prerequisite for implementation.
Optional maps may be replaced only by the recorded deeper raw-code reading defined there. It routes
a single checkpoint through mapping, planning, and execution, and routes multiple checkpoints into
separate Codex-managed worktree tasks. Multi-checkpoint dispatch defaults to three active workers
but obeys an explicit positive user concurrency value. The file is ignored by Git and copied into
managed worktrees through `.worktreeinclude`, so the mapping script does not include it in this
module's 133-file inventory or source hash. It must still be reread independently before repository
work.

The workspace is both policy and live build configuration. The documents define the intended and
ratified architecture, while `open/Cargo.toml` and `open/Cargo.lock` expose the dependency graph
that Cargo actually resolves. When those disagree, current manifests, crate source, tests, and
fresh tool output are implementation evidence; aspirational or stale prose is not.

## Source inventory

### Repository workflows and mapping

- `.agents/skills/superi-execution/SKILL.md`: Defines the checkpoint execution loop after planning.
  It requires the sole `planning.md` input and maintains the sole `execution.md` evidence file,
  refuses execution without validated mandatory maps and complete reading evidence, requires
  test-first proof, full integration, deterministic local verification, affected and consumer map
  refreshes, whole-result review, remote delivery, and paired-tab Google Docs completion before
  `Done.` is allowed.
- `.agents/skills/superi-execution/scripts/verify_checkpoint.py`: Selects deterministic local quality
  gates from the Git change set relative to a required base revision, validates changed Python and
  JSON syntax, always validates the codebase maps, and conditionally runs the applicable workflow,
  Rust, dependency, fixture, codec-feature, canonical-slice, shell, and frontend checks. `--full`
  selects every supported gate, while `--dry-run` exposes the exact selection without executing it.
- `.agents/skills/superi-execution/agents/openai.yaml`: Supplies the display name, short description,
  and default invocation prompt for the execution skill.
- `.agents/skills/superi-mapping/SKILL.md`: Defines module discovery, shard reading, synthesis,
  map frontmatter and required sections, stale-map refresh, and whole-map validation. Normal
  checkpoint workers read their mandatory map closure and may replace only optional maps with the
  prescribed deeper raw-code evidence. Mapping delegation is reserved for an explicitly assigned
  full-map coordinator, while each checkpoint worker refreshes its own affected maps.
- `.agents/skills/superi-mapping/scripts/codebase_maps.py`: Implements repository discovery, module
  assignment, UTF-8 and binary classification, deterministic source hashing, whole-file sharding,
  changed-module reporting, and strict map validation. It reads tracked plus nonignored untracked
  files, excludes generated maps, plans, Git internals, and build output, assigns crate and tool
  roots to their own modules, and assigns everything else to `workspace`. Validation checks anchored
  frontmatter, exact source ownership, revision syntax, inventory-section entries, resolved index
  links, unexpected module maps, required headings, current hashes, and forbidden Unicode dashes.
- `.agents/skills/superi-planning/SKILL.md`: Defines evidence-based planning for one checkpoint. It
  requires the live canonical assignment, verified descriptions-tab claim suffix, immutable
  main-tab specification, validated mandatory maps or recorded optional-map substitutions, full
  implementation-path reading through EOF, uncertainty research, proof design, and an ordered
  change map in the mandatory `plans/<checkpoint-id>/planning.md` before execution. No other
  planning document is permitted.
- `.agents/skills/superi-planning/agents/openai.yaml`: Supplies the display name, short description,
  and default invocation prompt for the planning skill.
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
  generated-bundle contract tests from `ci/frontend-smoke/`.
- `.github/workflows/network-isolated.yml`: Defines a blocking Ubuntu 24.04 job that prepares locked
  Rust dependencies, checksum-pinned libva 2.22 and libvpx 1.16, nasm, and test artifacts online,
  then enters a distinct Linux network namespace and runs workspace tests, fixture validation, and
  the CLI consumer with Cargo forced offline.
- `.github/workflows/tauri.yml`: Defines the blocking native Tauri Rust matrix for macOS 26 arm64,
  macOS 15 Intel, Windows 2025, and Ubuntu 24.04. It installs Linux WebKitGTK 4.1 prerequisites and
  runs locked formatting, mock-runtime tests, strict all-target Clippy, and native wry compilation.
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
  status. It now distinguishes the canonical contract runner from missing production behavior and
  gives the normalized invocation, locked fixture and slice contributor baseline, same-change stub
  replacement rule, and physical hardware proof boundary, while broader architecture remains
  aspirational.
- `closed/README.md`: Defines `closed/` as a notice for the separately maintained proprietary
  Superi Max tier and states the one-way dependency rule: Max may consume open Superi, while open
  Superi must never import, link, or depend on Max.
- `docs/architecture.md`: Records the foundational product boundary, locked Rust, wgpu, native OTIO,
  Tauri, React, TypeScript, and public API directions, the graph/GPU/color/caching/concurrency model,
  subsystem inventory, continuous-integration phasing, open and closed product separation, and open
  legal or product decisions.
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
  CoreGraphics, AV1, Opus, VPx, VideoToolbox, AudioConverter, Windows Media Foundation, and Linux
  VVC VA-API boundaries. It records ownership, buffer, thread, failure, and target proof for each
  boundary plus required source scans, Clippy runs, and focused tests.
- `docs/vertical-slice.md`: Defines revision 1 of `superi.slice.canonical.v1`. It pins the immutable
  video fixture role, exact one-track edit and trim, one typed horizontal-mirror graph effect,
  explicit delivery, eight stable replacement stages, schema 1.1.0 runner report, bounded stage
  timing and resident-memory records, conformance levels, portable project-state proof, shared
  hosted baseline, same-change production replacement rule, and the boundaries owned by
  P1.W07.C017 through P1.W07.C026.

### Frontend CI contract

- `ci/frontend-smoke/.node-version`: Pins Node.js 24.13.0 for local and hosted frontend gates.
- `ci/frontend-smoke/README.md`: Defines the CI-only boundary, exact local commands, build-before-test
  ordering, and migration requirement when the real Phase 3 application enters the repository.
- `ci/frontend-smoke/index.html`: Supplies the minimal browser document and module entry consumed by
  the Vite production build.
- `ci/frontend-smoke/package-lock.json`: Lockfile version 3 resolution for exact TypeScript 5.9.3,
  Vite 7.3.6, their build dependencies, and platform-optional esbuild and Rollup packages.
- `ci/frontend-smoke/package.json`: Declares a private CI package, Node.js 24.13.0, independent
  typecheck, build, and test commands, and exact TypeScript and Vite development dependencies.
- `ci/frontend-smoke/src/main.ts`: Implements a strict typed browser entry that verifies the contract
  root and renders the declared product, readiness, and independent frontend gates.
- `ci/frontend-smoke/tests/contract.test.mjs`: Verifies exact scripts and versions, strict compiler
  settings, immutable and least-privilege workflow wiring, locked installation, mandatory gates, and
  the generated hashed JavaScript entry in the production bundle.
- `ci/frontend-smoke/tsconfig.json`: Defines strict no-emit TypeScript checking for the browser entry
  with ES2022, DOM, bundler-resolution, isolated-module, and forced-module semantics.

### Tauri Rust CI contract

- `ci/tauri-smoke/README.md`: Defines the CI-only desktop-host boundary, mock and native runtime
  proof split, exact local commands, and migration to the future Phase 3 application.
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
  native runners, Linux prerequisites, and all four mandatory Rust gates.

### Cargo workspace and repository configuration

- `open/Cargo.lock`: Cargo lockfile format 3 for the resolved workspace. It records 24 local
  workspace packages, registry dependencies, target-support dependency trees, scenario digest
  and process-instrumentation dependency edges, graph and timeline document serialization and
  integrity edges, cache-key hashing, and the exact `oxideav-mp3` Git revision. Timeline state
  directly consumes the already-resolved `serde`, `serde_json`, and `sha2` packages, while
  `superi-cache` directly consumes the same pinned `sha2` package and now records its reviewed
  internal dependency on `superi-concurrency` for bounded background rendering. `superi-effects`
  now directly consumes the already-resolved `serde` package for its animation wire and records
  `serde_json` only for strict persistence and graph-reload tests. Audio output adds exact CPAL
  0.17.3 and ringbuf 0.4.8 plus their target-specific backend dependency trees. The lockfile is
  generated resolution evidence and is not hand-edited policy.
- `open/Cargo.toml`: Root Cargo workspace manifest using resolver 2 and glob members under
  `crates/*` and `tools/*`. It centralizes version `0.0.0`, Rust 2021, MIT, Rust 1.80, repository
  metadata, deny-by-default unsafe lints, and shared dependencies for error handling, serialization,
  images, GPU, codecs, hashes, process instrumentation, platform APIs, native build support, and
  reviewed audio device and ring-buffer primitives.
- `open/README.md`: Compact open-tree orientation and build commands. It records the 19 runtime
  crates plus repository tools, the exact canonical runner command, contract-only status, and
  the remaining production integration boundary.
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
  OTIO baseline generation, structured test-report responsibility, and deferred production work.
  Its cache tier now records the reviewed downward dependency on concurrency used by render jobs.
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
- `open/Cargo.toml` exports inherited workspace package metadata, lints, and dependency declarations
  to every member manifest. The current glob expansion is 19 crate packages plus
  `superi-fixture-tool`, `superi-dependency-check`, `superi-boundary-tool`, `superi-bench`, and
  `superi-test-report`, for 24 members total.
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
- `.github/workflows/frontend.yml` and `ci/frontend-smoke/` form a third CI surface for locked npm
  installation, strict TypeScript checking, Vite production bundling, and bundle-contract proof.
  This surface is intentionally not the absent React application or Tauri desktop shell.
- `.github/workflows/network-isolated.yml` and `open/ci/` form a fourth CI surface. It prepares
  artifacts before isolation, then proves current workspace tests, fixture validation, and the CLI
  consumer run with no non-loopback interface, no IPv4 route, and Cargo offline mode.

Together the five workflows enforce the open-tree boundary, locked hosted Rust builds, dependency
policy, a locked frontend toolchain contract, and one network-isolated core path. They do not yet
implement the complete documented feature, malformed-input, GPU, audio, shell, UI, or slice suites.

The stable public automation protocol described by Phase 0 is owned in `superi-api`, not here.
Likewise, codec, graph, image, engine, project, timeline, and CLI Rust interfaces live in their crate
modules even when workspace documents define constraints on them.

## Architecture and data flow

Repository work flows through two control planes.

The operational control plane begins with `AGENTS.md`. A single checkpoint synchronizes with the
remote, claims its exact descriptions-tab ID suffix, rereads the immutable main-tab specification,
validates and reads the global index plus mandatory affected and contract-path maps, and records
deeper raw-code substitutes for any permitted optional-map omission. Planning writes exactly
`plans/<id>/planning.md`; execution writes exactly `plans/<id>/execution.md`, implements and tests
the result, runs the changed-path local verifier plus checkpoint-specific proof, refreshes affected
maps, and completes only after repository, remote, and paired-tab document readback. A
multi-checkpoint request is dispatched in first-seen order into separate Codex-managed worktrees.
The rolling queue defaults to three active workers unless the user supplies another positive
concurrency value, and `.worktreeinclude` supplies the otherwise ignored root law.

The codebase-map flow is a repository navigation and freshness control plane. The Python script
discovers tracked files plus nonignored untracked files, excludes Git internals, generated maps,
plans, dependency output, and build output, then assigns `open/crates/*` and `open/tools/*` roots to
their own modules and everything else to `workspace`. A mapper runs `files` for the authoritative
owned-path list, reads every assigned text file from first line through EOF, and may use `shards` to
partition large inventories only at whole-file boundaries. Readers record surfaces, flows,
relationships, invariants, tests, gaps, and risks; synthesis reconciles those notes with manifests,
public entry points, and cross-module contracts before writing the required map sections. The global
index then captures repository-wide layering and runtime flow.

Maintenance follows the same evidence rule. Validate before relying on maps, use `changed` and the
actual diff after source work, reread each changed file and relevant interface or test through EOF,
update inventory and every affected architectural statement, and refresh consumer maps or the global
index when contracts, ownership, layering, flow, or status changes. Only after prose is reconciled
may the exact `hash` and file count be recorded. Validation must pass after updates, after final
integration or rebase, and before delivery. A passing hash never excuses stale prose.

The build control plane begins at `open/Cargo.toml`. Cargo expands `crates/*` and `tools/*`, applies
shared package metadata and lint defaults, resolves member and external dependencies into
`open/Cargo.lock`, including the pinned MIT rubato 0.16.2 sample-rate converter, and writes
generated build output under the ignored `open/target/`. Runtime
dependency direction is downward through the crate tiers: core and representation types support
GPU, concurrency, media, graph, and codecs; feature catalogs and timeline build on those; engine
orchestration assembles them; the API is the stable facade; and CLI is a headless consumer. The
fixture, dependency-check, boundary, and structured-report tools are workspace members for common
build, test, Clippy, and MSRV coverage, but none is part of the runtime DAG.

The timeline component document reuses workspace `serde`, `serde_json`, and `sha2` pins already
present for core and graph contracts. This changes the direct package edges recorded for
`superi-timeline` but does not change crate-tier direction, introduce a network path, or transfer
SQLite and autosave ownership away from `superi-project`.

The effects animation wire reuses the same workspace Serde and JSON pins. The lockfile change only
records direct dependencies on packages already resolved elsewhere in the workspace; it introduces
no registry package, network behavior, runtime tier reversal, or persistence ownership transfer.

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

The frontend CI path begins on pull requests, pushes to `main`, or manual dispatch. Its isolated
Ubuntu 24.04 job installs the exact Node.js 24.13.0 declaration, performs a lockfile-only `npm ci`,
runs strict no-emit TypeScript checking, builds the minimal browser entry with Vite 7.3.6, and then
tests both workflow wiring and the generated hashed bundle. It proves the locked frontend toolchain
and independent gates without creating a second application architecture or claiming React, Tauri,
editorial behavior, native viewport integration, or product UI coverage.

The Tauri Rust CI path begins on pull requests, pushes to `main`, or manual dispatch. Its blocking
matrix compiles the pinned CI-only host on macOS 26 arm64, macOS 15 Intel, Windows 2025, and Ubuntu
24.04. Every lane checks formatting, runs the mock-runtime unit test, denies Clippy warnings, and
compiles the real native wry binary from the lockfile. The binary constructs the command builder
without launching a window, so the gate proves native SDK and linker compatibility without claiming
the absent Phase 3 application.

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
state. Engine transactions coordinate project, timeline, graph, caches, persistence, undo, events,
playback, and export. The API presents the same command surface to UI, CLI, scripts, extensions, and
Superi Max, with no privileged closed-tier route.

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
24.13.0 with npm for the frontend contract, Tauri 2 with native desktop SDKs for the shell contract,
and the Google Docs plus Codex environment described by repository law for checkpoint coordination.
The mapping and verification scripts use only the Python standard library; the verifier
conditionally invokes the repository's Git, Python, Bash, Cargo, cargo-deny, and npm commands.

Every crate and repository tool consumes `open/Cargo.toml` package defaults and may opt into its
central dependency declarations. Cargo, CI, developers, and audit tooling consume the lockfile,
toolchain, formatter, ignore rules, and deny policy. Crate tests and end-to-end workflows consume the
shared fixture contract and fixture versions. Contributors, planners, reviewers, UI and engine
teams, and release operators consume the architecture and verification documents. The current slice
runner and each future production subsystem consume the stable scenario state, stages, report
boundary, and stage instrumentation contract. `superi-cli` consumes the pinned, system-only
`sysinfo` dependency for portable current-process resident-memory samples.

The documents deliberately point into other modules:

- `superi-core` owns shared identifiers, time, geometry, errors, diagnostics, and serializable base
  types.
- `superi-image`, `superi-gpu`, `superi-concurrency`, and `superi-media-io` own representation,
  resource, scheduling, and codec-neutral media foundations.
- `superi-codecs-rs`, `superi-codecs-platform`, and `superi-codecs-vendor` implement the three codec
  acquisition classes behind media interfaces.
- `superi-graph`, `superi-cache`, `superi-color`, `superi-effects`, `superi-timeline`, `superi-audio`,
  and `superi-ai` own evaluation and capability layers.
- `superi-project` owns persistence, `superi-engine` owns integration, `superi-api` owns the stable
  seam, and `superi-cli` is the headless consumer.
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
- The graph is the render primitive, and timeline compilation is deterministic. UI state is not a
  hidden render input. Local AI and automation produce normal editable, undoable artifacts.
- The canonical slice keeps one typed editable graph state across timeline inspection, preview, CLI,
  scripting, and export. Stub stages remain visible and can never satisfy runtime conformance.
- Canonical working images are tagged, scene-linear, premultiplied RGBA 16-bit float, with ACEScg as
  the default space and explicit transforms at input, display, and output boundaries.
- GPU residency, bounded resource use, immutable render snapshots, explicit thread ownership,
  bounded channels, cancellation, backpressure, and device-loss recovery are architecture
  contracts, not optional optimizations.
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

- `.github/workflows/dependency-policy.yml` runs on pushes, pull requests, and manual dispatch. Its
  Ubuntu 24.04 job first runs `.github/scripts/check-dependency-policy.sh`, then checks approved
  licenses and sources with cargo-deny against `open/Cargo.toml` using all features. This is
  automated policy enforcement, not evidence that the workspace builds, tests, works offline, or
  passes any physical platform lane.
- Fresh local verification for this refresh ran `bash -n` on the policy checker and executed it
  successfully. cargo-deny 0.19.9 then reported `licenses ok, sources ok` for the all-feature open
  workspace. The unused `Unicode-DFS-2016` allowance produced the documented non-failing warning.
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
  no-emit checking, immutable actions, read-only credentials, every independent gate, and a hashed
  JavaScript entry in the generated production bundle.
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
  Clippy for Media Foundation, codec tests, and all-feature engine tests after native-boundary
  changes. Real lifecycle tests still run on the owning operating system.
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
  public projection, checkout-independent project identity, eight-stage reports, expectation
  evidence, collision safety, two-run reproducibility, hosted command coverage, honest stub
  disclosure, all-stage timing and resident-memory records, and an exact observed-boundary maximum.
- Phase 0 completion additionally requires written codec counsel, a Tauri, React, and native wgpu
  viewport demonstration on all three operating systems, an API command and ordered event round
  trip, reference-validated OTIO interchange, color reference proof, and named subsystem owners.
- The mapping script is the structural proof for this map. Its focused requirements are the exact
  source hash and file count, one occurrence of every required heading, every owned path in the
  source inventory, a global-index link for every discovered module, and no Unicode em dash or en
  dash.

No test result is inferred from a documented command. A virtual adapter cannot satisfy a physical
lane, an unimplemented suite is a gap, and a retry retains its original failure evidence.

## Current status and risks

The workspace is beyond the original empty scaffold even though the public orientation has not been
updated consistently. Fresh Cargo metadata expands the member globs to 24 packages: 19 crates under
`open/crates/` plus the `superi-fixture-tool`, `superi-dependency-check`,
`superi-boundary-tool`, `superi-bench`, and `superi-test-report` repository utilities. The
lockfile includes a substantial
GPU, image, codec, serialization, platform, and native-build dependency graph, and current codec,
image, platform, and unsafe documents describe implemented contracts rather than empty placeholders.
The effects crate now owns a substantive graph-native authoring SDK plus an exact animation curve
and strict Serde wire. Its animation lockfile change records only direct use of already-resolved
serialization packages, while built-in visual nodes, the remaining feature modules, and engine
integration remain incomplete.
The synchronized remote revision before this checkpoint is
`5649d9075b29eef79b181caa880a650a59786ae1`.
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
single-master routing. Borrowed prepared input views and stable route-ID summing preserve channel
meaning and avoid callback allocation. Public consumers prove both a source-to-gain chain and a
dry-submix plus auxiliary-return path, including atomic topology rejection and order-sensitive
floating-point behavior. Explicit prepared channel nodes now convert canonical mono through 7.1
layouts using documented speaker rules or caller-selected discrete order without changing sample
time. Gain, pan, fader, metering, resampling, plugin hosting, decoded-sample
binding, and engine composition remain absent; production device output and sample-accurate
scheduling are implemented in the same audio crate but are not yet assembled by the engine.

The effects crate now provides a substantive graph-native authoring SDK. It composes canonical graph
schemas, editable nodes, deterministic registry snapshots, and snapshot-bound compiler translation,
and its public contract proves the same definition and typed edit path in timeline-role and
node-graph-role graphs. Built-in effects, image or GPU execution, timeline attachment, engine
integration, persistence, and rendered output remain absent.

The largest current risk is cross-document drift:

- The canonical fixture, independent expected contract, reference project and graph state, strict
  API projection, and contract runner now exist, and every stage reports bounded timing and
  resident-memory boundary evidence. The hosted workflow directly runs fixture validation and the
  portable eight-stage contract, and expectation version 2 removes checkout location from project
  identity. Generic typed DAG storage, cycle prevention, schema-bound editable nodes, and atomic
  graph mutation transactions and native timeline-to-graph compilation now exist, while graph
  evaluation and orchestration, rendered comparison, color and render integration, muxing,
  playable output, and all-runtime execution remain future work. The current contract-conformant
  run must not be reported as product or runtime conformance.
  Boundary samples are not continuous intra-stage peaks, constrained-device thresholds, or
  long-session soak proof.

- `open/docs/STRUCTURE.md` also says 18 crates and still labels offline CI and the vertical slice as
  deferred. The four workflows now cover dependency policy, locked hosted compilation with the
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
  Rust 1.80 floor. The hosted workflow also installs floating stable, and the recorded local build
  used Rust 1.97.0, so neither proves the minimum compiler until a Rust 1.80 lane runs.
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
- The frontend workflow validates a deliberately minimal TypeScript and Vite contract. It has no
  React dependency, no Tauri host, no native viewport, no public API consumer, and no editorial
  behavior, so a passing frontend lane must not be reported as product UI or desktop-shell proof.
- The frontend lockfile includes many platform-optional esbuild and Rollup packages. Their presence
  describes npm's portable dependency resolution and does not make those platforms supported Superi
  targets or prove a native frontend build outside the Ubuntu workflow.
- The governing `AGENTS.md` is ignored and absent from the mapping script's repository inventory.
  Changes to repository law therefore do not change this module's source hash, which makes manual
  rereading mandatory even when this map validates as current.
- Hosted workflows are not the routine agent completion gate. The local deterministic verifier plus
  checkpoint-specific proof is authoritative for the agent workflow unless a checkpoint explicitly
  owns hosted CI behavior; this does not convert a failing hosted lane into passing product evidence.

This map is based on the synchronized `origin/main` revision plus this uncommitted checkpoint, so
`mapped_at_commit` is `working-tree`. The remote base was
`41ad83bf61b2cecb5690bd7df792b585d2f6f24c` when the map was refreshed. Its hash describes the exact
134 discovered source files, including twelve generated binary payloads, layered on that revision.

## Maintenance notes

Run the mapping script from the repository root. Use `files workspace` to establish the complete
owned-path reading assignment, read every listed text file through EOF, use `hash workspace` only
after source changes settle, and run `validate` after map synthesis. The script hashes each owned
path, a null separator, its exact bytes, and another null separator in sorted path order, so path
changes and content changes both invalidate the hash.

Refresh this map whenever a root document, repository skill, workspace manifest, lockfile, toolchain
policy, deny policy, shared fixture, or other non-crate and non-tool source changes. Reconcile prose,
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
