# Superi Open Tree: Structure

How the workspace is organized, why, and how to work in it.

## The principle

One cargo crate per `§5` subsystem (`../../docs/architecture.md`). The crate graph **is** the
architecture: dependencies point downward only, so the engine/UI separation, the no-subsystem-bleed
rule, and the codec boundary are enforced by the Rust compiler, not by convention.

## Dependency tiers (downward-only, acyclic)

| tier | crates | depends on |
|---|---|---|
| T0 | `superi-core` | none |
| T1 | `superi-image` | core |
| T1 | `superi-gpu` | core, image |
| T1 | `superi-concurrency` | core, gpu |
| T1 | `superi-media-io` | core, image |
| T1b | `superi-codecs-rs` (default backend) | core, image, media-io |
| T1b | `superi-codecs-platform` (opt-in, `os-codecs`) | core, image, media-io |
| T1b | `superi-codecs-vendor` (opt-in host, `vendor-codecs`) | core, image, media-io |
| T2 | `superi-graph` (node-agnostic) | core, gpu, image, concurrency |
| T2 | `superi-cache` | core, gpu, image, graph, concurrency |
| T3 | `superi-color` | core, gpu, image, graph |
| T3 | `superi-effects` | core, gpu, image, graph |
| T3 | `superi-timeline` | core, graph |
| T3 | `superi-audio` | core, concurrency |
| T3 | `superi-ai` | core, image, graph |
| T4 | `superi-project` | core, graph, timeline, audio |
| T4 | `superi-engine` (orchestration) | all T0-T4 (+ codecs-platform via `os-codecs`, codecs-vendor via `vendor-codecs`) |
| T5 | `superi-api` (the public seam) | core, engine |
| T6 | `superi-cli` (first consumer) | core, api |

**Invariant:** `superi-graph` never depends on `superi-color`/`superi-effects`, node catalogs depend on
the graph, never the reverse. New capability = new node type, not a new dependency on the engine core.

## Codecs

See `../../docs/codecs.md`. Default build = royalty-free, pure-Rust, in-tree. `--features os-codecs`
adds the user's-OS decode path for encumbered formats. `--features vendor-codecs` adds only the MIT
host adapter for explicitly selected external ARRIRAW, R3D, and BRAW worker executables. Nothing
encumbered, proprietary, or copyleft ever links into the MIT core.

## Suggested ownership (6 engineers)

| dev | crates | theme |
|---|---|---|
| 1 | `superi-gpu`, `superi-concurrency` | graphics / systems |
| 2 | `superi-graph`, `superi-cache` | core engine |
| 3 | `superi-media-io`, `superi-codecs-rs`, `superi-codecs-platform`, `superi-codecs-vendor`, `superi-image` | media / IO / codecs |
| 4 | `superi-color`, `superi-effects` | color / comp |
| 5 | `superi-timeline`, `superi-project`, `superi-api` | editorial + public surface |
| 6 | `superi-audio`, `superi-ai` | audio + AI |
| none | `superi-core`, `superi-engine`, `superi-cli` | shared / lead-stewarded |

## Working in this workspace

- A **crate is the natural unit of ownership**, with a focused responsibility and an explicit
  dependency contract.
- Compiler-enforced boundaries support parallel development: work in `superi-color` cannot
  accidentally reach into `superi-gpu` internals or create a cycle.
- Respect the DAG. If a task seems to need an upward dependency, the design is wrong, stop and flag.
- The **offline law** and the **codec/license boundary** are sacred (`../../docs/architecture.md`,
  `../../docs/codecs.md`).

Repository-only utilities live under `tools/` and are workspace members so normal build, test,
Clippy, and minimum-Rust verification cover them. They do not participate in the runtime crate DAG.
`superi-fixture-tool` enforces the immutable layout, provenance, lineage, inventory, and content
integrity rules documented in `../test-fixtures/README.md` without network access. Its deterministic
generators include the versioned OTIO 0.18.1 interchange baseline consumed by timeline development
contracts without creating a runtime dependency.
`superi-test-report` validates explicit lane evidence and emits deterministic structured reports for
performance regressions, golden mismatches, flaky tests, and platform gaps without network access.

### Automated dependency-direction gate

Run the architecture gate from `open/`:

```bash
cargo run -p superi-dependency-check --locked
```

The checker reads locked, offline Cargo metadata and validates every internal normal and build
dependency against the exact runtime edges above. Test-only dependencies use a separate reviewed
allowlist, so a dev edge cannot silently authorize the same production edge. New runtime crates and
new internal edges fail closed until this document and the checker policy are updated together in
the architecture review that introduces them. Current reviewed dev edges let concurrency test media
values from `superi-media-io`, and let API contracts use `superi-media-io` registry fixtures plus the
`superi-concurrency` EngineControl domain needed to exercise the real engine introspection seam.
None of those test edges authorizes a production dependency. Because the checker is a wildcard
workspace member, its live-workspace contract also runs under the ordinary workspace test gate.
Project's reviewed runtime edge to audio carries authored clip-mix state and its canonical codec into
the durable aggregate. It does not carry prepared processors, devices, callbacks, or project policy
down into `superi-audio`, so the T4-to-T3 dependency direction stays one way.

`superi-bench` provides the stable Cargo benchmark boundary for decode, graph evaluation, upload,
playback, cache, render, and project save/load. Run it with `cargo bench -p superi-bench`; use
`SUPERI_BENCH_WARMUP`, `SUPERI_BENCH_SAMPLES`, and comma-separated `SUPERI_BENCH_STAGES` values to
bound or filter a run, and set `SUPERI_BENCH_REPORT` to retain its JSON report. Performance evidence
must also set the `SUPERI_BENCH_BUILD`, `SUPERI_BENCH_CPU`, `SUPERI_BENCH_MEMORY_MIB`,
`SUPERI_BENCH_GPU_BACKEND`, `SUPERI_BENCH_GPU_DRIVER`, `SUPERI_BENCH_CACHE_STATE`,
`SUPERI_BENCH_HARDWARE_TIER`, `SUPERI_BENCH_FIXTURE_REVISION`, and
`SUPERI_BENCH_PROJECT_REVISION` context fields.

Subsystem owners register a workload only when it executes that subsystem's real consumer path and
names its exact versioned fixture. Until then, the permanent stage is emitted as a `gap`. A gap is
successful harness execution but never a passing performance result, and a report with
`context_complete: false` is diagnostic output rather than accepted performance evidence.
The shipped graph stage pulls a deterministic three-node DAG through
`superi_graph::eval::LazyEvaluator`; decode, upload, playback, cache, render, and save/load retain
explicit gaps until their owners provide a bounded consumer workload and versioned fixture.

## Deferred (not in this scaffold)

Network-isolated offline CI test · the vertical slice (`import → trim → effect → export`) · the
`closed/` tree · the web UI · codec legal sign-off (open item #1) · the production OTIO reader,
writer, and timeline model (P2.W02).
