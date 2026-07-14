---
module_id: workspace
source_paths:
  - repository files outside open/crates/* and open/tools/*
source_hash: a41707e0e3f927ed4253f4fba5363491ce955fa02c4003cc76945ce95a5b439c
source_files: 57
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
checkpoint assignments, fixes the exact Google Docs claim, blocked-note, and three-sentence
completion protocol, requires safe synchronization with `origin/main`, and makes current maps plus
full selected raw-file reads a prerequisite for implementation. It routes a single checkpoint
through mapping, planning, and execution, and routes multiple checkpoints into separate
Codex-managed worktree tasks. It is ignored by Git and copied into managed worktrees through
`.worktreeinclude`, so the mapping script does not include it in this module's 57-file inventory or
source hash. It must still be reread independently before repository work.

The workspace is both policy and live build configuration. The documents define the intended and
ratified architecture, while `open/Cargo.toml` and `open/Cargo.lock` expose the dependency graph
that Cargo actually resolves. When those disagree, current manifests, crate source, tests, and
fresh tool output are implementation evidence; aspirational or stale prose is not.

## Source inventory

### Repository workflows and mapping

- `.agents/skills/superi-execution/SKILL.md`: Defines the checkpoint execution loop after planning.
  It refuses execution without validated maps and complete reading evidence, requires test-first
  proof, full integration, widening verification, affected and consumer map refreshes, whole-result
  review, remote delivery, and an explicit Google Docs completion gate before `Done.` is allowed.
- `.agents/skills/superi-execution/agents/openai.yaml`: Supplies the display name, short description,
  and default invocation prompt for the execution skill.
- `.agents/skills/superi-mapping/SKILL.md`: Defines module discovery, shard reading, synthesis,
  map frontmatter and required sections, stale-map refresh, and whole-map validation. It explicitly
  requires every assigned text file to be read from first line through EOF.
- `.agents/skills/superi-mapping/scripts/codebase_maps.py`: Implements repository discovery, module
  assignment, UTF-8 and binary classification, deterministic source hashing, whole-file sharding,
  changed-module reporting, and strict map validation. It reads tracked plus nonignored untracked
  files, excludes generated maps, plans, Git internals, and build output, assigns crate and tool
  roots to their own modules, and assigns everything else to `workspace`. Validation checks anchored
  frontmatter, exact source ownership, revision syntax, inventory-section entries, resolved index
  links, unexpected module maps, required headings, current hashes, and forbidden Unicode dashes.
- `.agents/skills/superi-planning/SKILL.md`: Defines evidence-based planning for one checkpoint. It
  requires the live canonical assignment, verified visible claim, validated global and module maps,
  full implementation-path reading through EOF, uncertainty research, proof design, and an ordered
  change map before execution.
- `.agents/skills/superi-planning/agents/openai.yaml`: Supplies the display name, short description,
  and default invocation prompt for the planning skill.
- `.github/scripts/check-dependency-policy.sh`: Executable Bash contract check for the dependency
  policy workflow. It requires exact workflow name, permissions, checker invocation, cargo-deny
  action inputs, unknown-Git denial, revision-pinned Git policy, and the approved OxideAV source;
  any missing required line fails before cargo-deny runs.
- `.github/workflows/ci.yml`: Defines cross-platform locked-workspace quality jobs. Pull requests and
  pushes to `main` run five macOS, Windows, and Ubuntu lanes, with Ubuntu 26.04 marked experimental;
  a separate Ubuntu 22.04 job runs weekly or by manual dispatch. Both jobs install stable Rust with
  rustfmt and Clippy, record build identity, enforce the open-tree boundary with the locked
  repository scanner, and run formatting, locked build and test commands, strict all-target Clippy,
  and locked documentation tests from `open/`. Hosted macOS excludes only three named native codec
  lifecycle tests that require the physical hardware lane. Linux jobs install `libva-dev` and
  `nasm` so the locked media dependency graph can discover `libva.pc` and the approved runtime can
  retain its optimized x86 code. Intel macOS jobs install `nasm` with Homebrew. Linux and macOS jobs
  build the approved libvpx 1.16.0 archive after verifying its pinned checksum and expose that exact
  shared runtime to capability and codec tests.
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
  Rust dependencies, libva headers, nasm, checksum-pinned libvpx 1.16, and test artifacts online,
  then enters a distinct Linux network namespace and runs workspace tests, fixture validation, and
  the CLI consumer with Cargo forced offline.
- `.gitignore`: Excludes Rust and JavaScript build output, editor and macOS files, local agent law,
  checkpoint plans, Python bytecode and cache directories, browser artifacts, and the frontend CI
  contract's generated `dist/`. In particular, `AGENTS.md`, `BASE_INSTRUCTIONS.md`, and `/plans/`
  remain local working inputs rather than normal tracked sources.
- `.worktreeinclude`: Requests that the otherwise ignored `AGENTS.md` be copied into Codex-managed
  worktrees so repository law is present in isolated checkpoint tasks.

### Product, architecture, and policy documents

- `LICENSE`: Applies the MIT license to the repository, with copyright held by Brian Gao and Justin
  Chen, and includes the standard permission, notice-retention, and warranty-disclaimer terms.
- `README.md`: Gives the public project orientation, product split, graph and GPU model, subsystem
  hierarchy, build commands, vertical slice, phases, invariants, open questions, and claimed current
  status. Its first executable thread links to the exact canonical slice contract. Several other
  scaffold-status statements are stale relative to current manifests and policy documents, as
  recorded below.
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
- `docs/checkpoints/P1.W05.C005.md`: Durable implementation evidence for deterministic display,
  view, look, and delivery rules. It records source-role selection, ordered LUT processing,
  authoritative output-transform integration, critical-tier verification, delivery context, and
  intentionally separate persistence, ICC, GPU, storage, viewport, and export stages.
- `docs/checkpoints/P1.W07.C002.md`: Durable implementation evidence for the complete Rust CI quality
  suite. It records the low-risk configuration boundary, both-job command coverage, the explicit
  hosted macOS native codec exception, focused local proof, hosted proof requirement, delivery
  context, and deferred feature and frontend coverage.
- `docs/checkpoints/P1.W07.C004.md`: Durable implementation evidence for frontend CI. It records the
  isolated contract boundary, exact Node.js, TypeScript, and Vite versions, advisory-driven Vite
  update, red-to-green and negative controls, clean locked npm verification, locked Rust tests,
  delivery
  context, and the explicit absence of the real React and Tauri application.
- `docs/checkpoints/P1.W07.C008.md`: Durable implementation evidence for the open-tree boundary
  scanner. It records the dependency-free tool, canonical and malformed-tree contracts, locked
  workflow integration, isolated Rust verification, delivery context, and remaining static-policy
  limitations.
- `docs/checkpoints/P1.W07.C009.md`: Durable implementation evidence for the network-isolated core
  workflow, namespace and offline contracts, focused verification, hosted proof requirement,
  delivery context, and intentionally unimplemented editorial slice.
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
- `docs/platform-testing.md`: Defines revision 1 of required automated and physical test lanes for
  macOS, Windows, and Ubuntu, stable suite identifiers, cadence and blocking rules, deterministic
  cross-platform expectations, capability-based codec testing, and the structured evidence every
  result must retain.
- `docs/unsafe-ffi.md`: Defines the deny-by-default unsafe policy and inventories audited macOS
  CoreGraphics, AV1, Opus, VPx, VideoToolbox, AudioConverter, Windows Media Foundation, and Linux
  VVC VA-API boundaries. It records ownership, buffer, thread, failure, and target proof for each
  boundary plus required source scans, Clippy runs, and focused tests.
- `docs/vertical-slice.md`: Defines revision 1 of `superi.slice.canonical.v1`. It pins the immutable
  video fixture role, exact one-track edit and trim, one typed horizontal-mirror graph effect,
  explicit delivery, eight stable replacement stages, runner report, conformance levels,
  reproducibility proof, and the boundaries owned by P1.W07.C017 through P1.W07.C026.

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

### Cargo workspace and repository configuration

- `open/Cargo.lock`: Cargo lockfile format 3 for the resolved workspace. It records 22 local
  workspace packages, registry dependencies, target-support dependency trees, and the exact
  `oxideav-mp3` Git revision. It is generated resolution evidence and is not hand-edited policy.
- `open/Cargo.toml`: Root Cargo workspace manifest using resolver 2 and glob members under
  `crates/*` and `tools/*`. It centralizes version `0.0.0`, Rust 2021, MIT, Rust 1.80, repository
  metadata, deny-by-default unsafe lints, and shared dependencies for error handling, serialization,
  images, GPU, codecs, hashes, platform APIs, and native build support.
- `open/README.md`: Compact open-tree orientation and build commands. It describes an offline engine,
  codec features, downward dependency tiers, and an architectural skeleton, but its 18-crate and
  implementation-status claims lag the current 19 crate packages plus three repository tools.
- `open/ci/network-isolated-contract.sh`: Executable contract binding the dedicated workflow to
  immutable checkout, least privilege, locked artifact preparation, namespace isolation, fixture
  validation, and the headless CLI consumer.
- `open/ci/run-network-isolated.sh`: Linux harness that verifies a distinct namespace,
  loopback-only interfaces, no IPv4 route, and a failed numeric outbound connection before running
  the current core commands with locked offline Cargo.
- `open/deny.toml`: Cargo-deny policy allowing a bounded permissive license set, warning on duplicate
  versions and yanked advisories, rejecting unknown Git sources, requiring pinned Git revisions, and
  permitting only the pinned OxideAV MP3 repository as a Git source.
- `open/docs/STRUCTURE.md`: Compact dependency-tier map, codec placement, suggested human ownership,
  crate-boundary working rules, repository-tool placement, fixture-tool responsibility, and a list
  of deferred work. Its 18-crate wording is also behind the current workspace membership.
- `open/rust-toolchain.toml`: Selects the floating stable Rust channel with `rustfmt` and Clippy.
  Package metadata separately declares Rust 1.80 as the minimum supported version.
- `open/rustfmt.toml`: Sets Rust 2021 formatting and a 100-column maximum width.

### Shared test fixtures

- `open/test-fixtures/README.md`: Defines the immutable versioned fixture layout, strict schema 1
  manifest, file inventory, provenance and parent-lineage rules, redistribution restrictions,
  contributor workflow, offline validation command, hard-failure conditions, and the deterministic
  video baseline's exact reproduction and consumer contract.
- `open/test-fixtures/policy/utf8/v1/fixture.json`: Schema 1 manifest for fixture identity
  `policy/utf8`, version 1. It declares a synthetic CC0 payload generated by POSIX `printf`, records
  no parents, and inventories `hello.txt` as 6 bytes with its SHA-256 digest.
- `open/test-fixtures/policy/utf8/v1/hello.txt`: The six-byte UTF-8 payload `hello` followed by a
  newline. It is the fixture validator's deterministic self-test input.
- `open/test-fixtures/video/pixel-formats/v1/fixture.json`: Schema 1 CC0 provenance and exact
  inventory for the generated catalog and raw-frame payload.
- `open/test-fixtures/video/pixel-formats/v1/video-cases.csv`: Fixed CRLF catalog with one record per
  plane across 207 format-and-rate cases, including geometry, offsets, sizes, and plane digests.
- `open/test-fixtures/video/pixel-formats/v1/video-frames.bin`: A 13,419-byte binary containing every
  catalog plane contiguously. `superi-fixture-tool` produces it, its manifest binds its exact hash,
  and `superi-media-io` consumes and validates every plane through the public frame path.

The mapping inventory contains 56 UTF-8 text artifacts and the one 13,419-byte binary payload
described above. The binary is intentionally read through metadata, producer, provenance,
manifest, and consumer evidence rather than interpreted as prose.

## Public surface

This module has no runtime Rust API of its own. Its public surfaces are configuration and contract
surfaces consumed by people, Cargo, repository agents, tests, and downstream modules:

- The root README, north star, architecture, Phase 0 contracts, codec policy, phase plan, platform
  matrix, unsafe inventory, and MIT license define the repository's public technical and product
  commitments.
- `docs/vertical-slice.md` is the normative integration contract for the first editorial thread.
  It distinguishes disclosed-stub contract conformance from all-runtime conformance and reserves
  concrete fixture, runner, expectation, instrumentation, and replacement work for their owning
  checkpoints.
- `open/Cargo.toml` exports inherited workspace package metadata, lints, and dependency declarations
  to every member manifest. The current glob expansion is 19 crate packages plus
  `superi-fixture-tool`, `superi-dependency-check`, and `superi-boundary-tool`, for 22 members total.
- `open/Cargo.lock` is the reproducible dependency-resolution surface for builds and audit tools.
- `open/deny.toml`, `open/rust-toolchain.toml`, and `open/rustfmt.toml` are entry points for license
  audit, toolchain installation, and formatting.
- The shared fixture root is a repository-wide data interface. Tests identify a fixture by stable
  path and version, consume only manifest-listed payloads, and validate them through
  `superi-fixture-tool` rather than selecting an implicit latest version.
- The version 1 video fixture is the current deterministic format-and-rate baseline. Its fixed
  catalog and raw bytes are generated by `superi-fixture-tool` and consumed by the
  `superi-media-io` integration contract without adding a runtime dependency between them.
- The three repository skills expose checkpoint planning, checkpoint execution, and codebase map
  maintenance workflows. Their `agents/openai.yaml` files are presentation metadata, not alternate
  behavior specifications.
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
  boundary.
- `.github/workflows/dependency-policy.yml` is the separate dependency license and source policy
  surface.
- `.github/workflows/frontend.yml` and `ci/frontend-smoke/` form a third CI surface for locked npm
  installation, strict TypeScript checking, Vite production bundling, and bundle-contract proof.
  This surface is intentionally not the absent React application or Tauri desktop shell.
- `.github/workflows/network-isolated.yml` and `open/ci/` form a fourth CI surface. It prepares
  artifacts before isolation, then proves current workspace tests, fixture validation, and the CLI
  consumer run with no non-loopback interface, no IPv4 route, and Cargo offline mode.

Together the four workflows enforce the open-tree boundary, locked hosted Rust builds, dependency
policy, a locked frontend toolchain contract, and one network-isolated core path. They do not yet
implement the complete documented feature, malformed-input, GPU, audio, shell, UI, or slice suites.

The stable public automation protocol described by Phase 0 is owned in `superi-api`, not here.
Likewise, codec, graph, image, engine, project, timeline, and CLI Rust interfaces live in their crate
modules even when workspace documents define constraints on them.

## Architecture and data flow

Repository work flows through two control planes.

The operational control plane begins with `AGENTS.md`. A single checkpoint synchronizes with the
remote, claims the exact live Google Docs entry, validates and reads the complete codebase-map layer,
researches under `superi-planning`, builds under `superi-execution`, refreshes every affected or
contract-dependent map, and completes only after repository, remote, and document readback. A
multi-checkpoint request is dispatched into separate Codex-managed worktrees, where
`.worktreeinclude` supplies the otherwise ignored root law.

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
`open/Cargo.lock`, and writes generated build output under the ignored `open/target/`. Runtime
dependency direction is downward through the crate tiers: core and representation types support
GPU, concurrency, media, graph, and codecs; feature catalogs and timeline build on those; engine
orchestration assembles them; the API is the stable facade; and CLI is a headless consumer. The
fixture, dependency-check, and boundary tools are workspace members for common build, test, Clippy,
and MSRV coverage, but none is part of the runtime DAG.

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

The frontend CI path begins on pull requests, pushes to `main`, or manual dispatch. Its isolated
Ubuntu 24.04 job installs the exact Node.js 24.13.0 declaration, performs a lockfile-only `npm ci`,
runs strict no-emit TypeScript checking, builds the minimal browser entry with Vite 7.3.6, and then
tests both workflow wiring and the generated hashed bundle. It proves the locked frontend toolchain
and independent gates without creating a second application architecture or claiming React, Tauri,
editorial behavior, native viewport integration, or product UI coverage.

The network-isolated path begins on pull requests, pushes to `main`, or manual dispatch. It pins
checkout, disables persisted credentials, installs stable Rust, libva headers, and nasm, builds the
checksum-pinned approved libvpx 1.16 runtime, fetches locked dependencies, and builds the workspace
and test executables while online. It records the host namespace and uses privileged `unshare
--net` to enter a new namespace, carrying only the required Rust environment and approved libvpx
path. The harness rejects the host namespace, any non-loopback interface, any IPv4 route, or a
successful numeric outbound connection before forcing Cargo offline and running workspace tests,
fixture validation, and the CLI. This proves current core commands operate without outbound access
after setup, not that dependency or media-runtime acquisition is offline.

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
production owner replaces it, and any reported stub prevents runtime conformance. This contract
defines integration intent only; no current engine or CLI path executes the complete scenario.

Shared fixture data flows from a versioned directory to `fixture.json`, whose byte counts and hashes
bind every payload. `superi-fixture-tool` validates path safety, schema, provenance, lineage,
inventory completeness, size, and digest before crate tests, golden tests, fuzzing, benchmarks, or
end-to-end workflows consume the data. It also generates the video baseline into a new absent
directory from stable pixel-format, rate, geometry, sample, and serialization rules. The media-I/O
contract then validates the canonical catalog's complete matrix, exact plane bytes, and public
frame construction. The UTF-8 fixture remains the validator's smallest policy self-test.

The product boundary is physical and one way. The open workspace must build and perform core work
without `closed/`, accounts, remote services, or a network. Superi Max may call the open public API
and may produce normal editable artifacts, but no open crate may consume proprietary implementation.

## Dependencies and consumers

The workspace module depends on Git for source discovery and revision identity, Python 3 for map
generation, Cargo and stable Rust for the open workspace, Bash and `grep` for the executable policy
contract, cargo-deny plus GitHub Actions for dependency policy, GitHub-hosted macOS, Windows, and
Ubuntu runners for build portability, Node.js 24.13.0 with npm for the frontend contract, and the
Google Docs plus Codex environment described by repository law for checkpoint coordination. The
mapping script invokes Git directly and uses only the Python standard library.

Every crate and repository tool consumes `open/Cargo.toml` package defaults and may opt into its
central dependency declarations. Cargo, CI, developers, and audit tooling consume the lockfile,
toolchain, formatter, ignore rules, and deny policy. Crate tests and end-to-end workflows consume the
shared fixture contract and fixture versions. Contributors, planners, reviewers, UI and engine
teams, and release operators consume the architecture and verification documents. The future slice
runner and each production subsystem consume the stable scenario state, stages, and report boundary.

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
- Repository checkpoint claims and completion evidence use exact configured formatting; native
  checkboxes remain human-owned. Work synchronizes with `origin/main`, preserves all existing work,
  and never force pushes or uses destructive conflict handling.
- Every module map reflects implemented reality, contains every discovered path, and becomes stale
  when its deterministic source hash changes. Generated maps and local plan files do not contribute
  to source hashes.
- Map freshness is both structural and semantic. Every assigned text file must be read through EOF,
  every inventory path and required section must remain present, and a hash-only update is invalid
  when surfaces, flows, relationships, invariants, tests, status, or risks changed.

## Tests and verification

The workspace documents define several proof layers. Four implemented workflows now cover the
open-tree boundary, hosted locked-workspace builds, dependency policy, a locked frontend toolchain
contract, and network-isolated execution of current core commands; every broader suite or physical
matrix remains a contract until a current workflow or fresh result demonstrates execution.

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
  Ubuntu 22.04 on weekly or manual runs. YAML parsing and all six lane-ID presence checks, preview
  policy, disabled credentials, one locked boundary command per job, complete two-job command
  coverage, exact Linux `libva-dev` and cross-platform x86 `nasm` provisioning, checksum-pinned
  libvpx 1.16.0 provisioning, and the hosted macOS native-test condition passed during this refresh.
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
- `.github/workflows/network-isolated.yml` prepares locked inputs and test executables on Ubuntu
  24.04 after installing libva headers and nasm and building checksum-pinned libvpx 1.16, then uses
  a distinct empty network namespace and Cargo offline mode for workspace tests, canonical fixture
  validation, and the CLI consumer. The delivered hosted run is the authoritative namespace proof
  because the local macOS host cannot execute Linux `unshare --net`.
- `docs/checkpoints/P1.W07.C004.md` records a fresh clean npm installation, typecheck, production
  build, three passing contract tests, zero reported vulnerabilities, negative TypeScript and
  missing-bundle controls, YAML parsing, and a complete locked Rust test run. These are delivery
  results for the isolated contract, not proof of a React application or Tauri shell.
- Root and open-tree guidance call for workspace build, test, strict Clippy, documentation tests,
  default and optional codec feature coverage, and the real CLI or vertical-slice consumer.
- `docs/platform-testing.md` defines stable `toolchain`, `features`, `fixtures`, `malformed`, `gpu`,
  `codecs`, `audio`, `slice`, `performance`, and `soak` suites. Hosted lanes prove source and CPU
  portability, while named physical lanes are required for real GPU, display, audio, hardware
  codec, performance, and long-session evidence.
- `docs/unsafe-ffi.md` requires a repository unsafe scan, all-feature strict Clippy, Windows-target
  Clippy for Media Foundation, codec tests, and all-feature engine tests after native-boundary
  changes. Real lifecycle tests still run on the owning operating system.
- `open/test-fixtures/README.md` defines
  `cargo run -p superi-fixture-tool -- check test-fixtures` as the shared validation path and
  `cargo run -p superi-fixture-tool -- generate-video <OUTPUT_DIRECTORY>` as the video reproduction
  path. The UTF-8 fixture's payload is 6 bytes and its SHA-256 is
  `5891b5b522d5df086d0ff0b110fbd9d21bb4fc7163af34d08286a2e846f6be03`, matching its manifest.
- The canonical slice contract passed a focused semantic check for its scenario identity, exact
  fixture and edit, one effect, eight stages, conformance levels, default offline boundary,
  adjacent checkpoint ownership, and root README discovery. These checks validate the contract and
  existing repository policy, not a runtime slice.
- Fresh fixture validation reports two valid fixture versions and three payloads. The video fixture
  contains 207 cases in a 13,419-byte binary and is also protected by catalog, generator, CLI,
  media-consumer, and canonical-root contracts.
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
updated consistently. Fresh Cargo metadata expands the member globs to 22 packages: 19 crates under
`open/crates/` plus the `superi-fixture-tool`, `superi-dependency-check`, and
`superi-boundary-tool` repository utilities. The
lockfile includes a substantial
GPU, image, codec, serialization, platform, and native-build dependency graph, and current codec,
image, platform, and unsafe documents describe implemented contracts rather than empty placeholders.
The synchronized remote revision now ends at `de1770ece3b7430a7b2509c838346d94aa2619d7`.
Commit `217e9d48703bcfd4736d949aea510c94505071bc` added the dependency-policy workflow and aligned the
root README, deny policy, and structure guide with license-audit CI. Commit
`e0b3af9f099f527a8544d1b0317896640969903b` added the executable dependency-policy contract and its
durable record. Commit `68c007309c3c548d28c2001c1673c61c57da3ac0` added the cross-platform hosted
build workflow and durable `P1.W07.C001` checkpoint record. Commit
`cb1fe287c5ca3d9f5fd91d25c1a4b90b70594867` added the locked frontend CI contract and durable
`P1.W07.C004` record. Later mapping and dependency-direction work reached the current remote base.

The largest current risk is cross-document drift:

- The canonical vertical slice is now exact, but its named fixture, runner, expectation record,
  project and graph state, muxer, and all-runtime execution remain future checkpoint work. A
  contract-conformant stub run must not be reported as product or runtime conformance.

- `README.md` and `open/README.md` say the tree is an architectural skeleton, count 18 crates, claim
  that only the CLI scaffold has executable behavior, and say no external workspace dependency has
  been activated. Those statements conflict with the current 22-member Cargo graph, resolved
  external dependencies, shared fixture tool, and detailed implementation contracts.
- `open/docs/STRUCTURE.md` also says 18 crates and still labels offline CI and the vertical slice as
  deferred. The four workflows now cover dependency policy, locked hosted compilation with the
  open-tree boundary scan, the frontend toolchain contract, and one network-isolated core path, but
  that path prepares dependencies online and must not be mistaken for product behavior, a complete
  offline build, feature, malformed-input, UI, shell, slice, or physical-platform enforcement.
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
  Clippy, and documentation-test suite. Feature coverage, fixture behavior, and malformed-input
  checks remain intentionally owned by later checkpoints.
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

This map is based on the synchronized `origin/main` revision plus this uncommitted checkpoint, so
`mapped_at_commit` is `working-tree`. The remote base was
`de1770ece3b7430a7b2509c838346d94aa2619d7` when the map was refreshed. Its hash describes the exact
57 discovered source files, including the generated binary payload, layered on that revision.

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

Always reread `AGENTS.md` even if the workspace hash is current. It is operational law outside the
generated inventory. Also inspect any future binary assigned to this module by file type, size,
producer, provenance, manifest, and consumers rather than treating its bytes as prose.

The root and open READMEs, compact structure guide, Phase 0 contracts, codec policy, unsafe audit,
and platform matrix overlap intentionally but currently disagree in status. When implementation
changes, update the most authoritative contract and every public status summary that would otherwise
mislead a contributor. Keep planned requirements clearly separate from code paths and test evidence
that exist and have run.
