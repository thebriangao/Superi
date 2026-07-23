# Superi codebase map

This index is the current navigation layer for Superi. The detailed module maps own source
inventories, public surfaces, runtime flows, tests, risks, and maintenance rules. Current code,
manifests, tests, and fresh tool output remain authoritative when historical checkpoint documents
describe a retired implementation.

## System shape

Superi is an offline-first Rust media engine with a retained native wgpu interface. Authored state
and transaction semantics remain below the presentation layer. `superi-desktop` owns the thin winit
host, `superi-ui` owns one scene for pixels, input, focus, and accessibility, and `superi-session`
owns portable application services. All three reuse the existing engine, public API, concurrency,
GPU, media, timeline, graph, audio, color, image, cache, effect, project, codec, and AI owners.

The production path contains no React, Tauri, Vite, webview, or browser runtime. The preserved
TypeScript packages are transport and editorial contract consumers, not a presentation surface.
Private interface control and capture use the same retained scene and wgpu compositor through
`superi-ui-inspect`. C001 presents only a neutral platform diagnostic; shell, workspace, media,
viewer, timeline, inspector, composite, color, audio, and delivery surfaces begin in later Phase
Infinity checkpoints.

```text
winit + AccessKit
       |
superi-desktop
       |
superi-ui -------- superi-ui-inspect
       |                    |
superi-session              +-- PNG + semantics + transcript + manifest
       |
superi-api -> superi-engine -> timeline, graph, project, media, audio, color, effects
       |
superi-concurrency + superi-gpu
```

## Module inventory

| Module | Owned path | Current responsibility |
| --- | --- | --- |
| [`superi-ai`](modules/superi-ai.md) | `open/crates/superi-ai` | Local inference capability and editable-artifact boundary |
| [`superi-api`](modules/superi-api.md) | `open/crates/superi-api` | Transport-neutral public schemas, commands, events, permissions, scripting, and generated contracts |
| [`superi-audio`](modules/superi-audio.md) | `open/crates/superi-audio` | Prepared audio graph, devices, plugins, routing, automation, scheduling, and metering |
| [`superi-cache`](modules/superi-cache.md) | `open/crates/superi-cache` | Memory, disk, derived-media, prediction, warming, and render-result reuse |
| [`superi-cli`](modules/superi-cli.md) | `open/crates/superi-cli` | Headless project, media, validation, JSON-RPC, schema, and canonical-slice consumer |
| [`superi-codecs-platform`](modules/superi-codecs-platform.md) | `open/crates/superi-codecs-platform` | Opt-in native codec adapters |
| [`superi-codecs-rs`](modules/superi-codecs-rs.md) | `open/crates/superi-codecs-rs` | Default permissive software codecs |
| [`superi-codecs-vendor`](modules/superi-codecs-vendor.md) | `open/crates/superi-codecs-vendor` | Isolated separately installed vendor RAW worker protocol |
| [`superi-color`](modules/superi-color.md) | `open/crates/superi-color` | Working spaces, transforms, display intent, analysis, tone mapping, LUTs, and ICC discovery |
| [`superi-concurrency`](modules/superi-concurrency.md) | `open/crates/superi-concurrency` | Execution domains, jobs, clocks, handoffs, lifecycle, snapshots, and liveness |
| [`superi-core`](modules/superi-core.md) | `open/crates/superi-core` | Tier-zero identities, exact time, errors, diagnostics, validation, and serialization |
| [`superi-desktop`](modules/superi-desktop.md) | `open/crates/superi-desktop` | Thin native winit host, surface lifecycle, OS input, and AccessKit adapter |
| [`superi-effects`](modules/superi-effects.md) | `open/crates/superi-effects` | Visual definitions, animation, masks, tracking, text, transitions, plugins, and CPU references |
| [`superi-engine`](modules/superi-engine.md) | `open/crates/superi-engine` | Open subsystem assembly, command dispatch, history, lifecycle, playback, and export orchestration |
| [`superi-gpu`](modules/superi-gpu.md) | `open/crates/superi-gpu` | Managed wgpu devices, resources, passes, submission, surfaces, readback, and recovery |
| [`superi-graph`](modules/superi-graph.md) | `open/crates/superi-graph` | Typed graph authoring, validation, evaluation planning, drivers, and diagnostics |
| [`superi-image`](modules/superi-image.md) | `open/crates/superi-image` | CPU image operations and deterministic reference behavior |
| [`superi-media-io`](modules/superi-media-io.md) | `open/crates/superi-media-io` | Media identity, probing, demux, decode, encode, mux, streams, and source coordination |
| [`superi-project`](modules/superi-project.md) | `open/crates/superi-project` | Durable project schema, compatibility, storage, copy, backup, recovery, and locking |
| [`superi-session`](modules/superi-session.md) | `open/crates/superi-session` | Portable app lifecycle, engine connection, project services, transport, diagnostics, and migration |
| [`superi-timeline`](modules/superi-timeline.md) | `open/crates/superi-timeline` | Canonical editorial model, mutations, snapping, selection, interchange, and timing |
| [`superi-ui`](modules/superi-ui.md) | `open/crates/superi-ui` | Retained scene, layout, paint, input, focus, semantics, icons, wgpu composition, and capture |
| [`superi-api-bindings`](modules/tool-superi-api-bindings.md) | `open/tools/superi-api-bindings` | Deterministic generated binding writer and drift checker |
| [`superi-bench`](modules/tool-superi-bench.md) | `open/tools/superi-bench` | Reproducible performance benchmark harness |
| [`superi-boundary-tool`](modules/tool-superi-boundary-tool.md) | `open/tools/superi-boundary-tool` | Open-tree and dependency-direction enforcement |
| [`superi-dependency-check`](modules/tool-superi-dependency-check.md) | `open/tools/superi-dependency-check` | Cargo dependency policy inspection |
| [`superi-fixture-tool`](modules/tool-superi-fixture-tool.md) | `open/tools/superi-fixture-tool` | Fixture validation and canonical vertical-slice execution |
| [`superi-test-report`](modules/tool-superi-test-report.md) | `open/tools/superi-test-report` | Normalized test report collection and comparison |
| [`superi-ui-inspect`](modules/tool-superi-ui-inspect.md) | `open/tools/superi-ui-inspect` | Private deterministic retained UI control, capture, crop, inspect, and compare |
| [`workspace`](modules/workspace.md) | Repository files outside crate and tool roots | Product law, docs, workflows, manifests, bindings, assets, fixtures, and agent operating system |

## Ownership rules

- `superi-engine` and lower subsystems remain the authorities for authored state and execution.
- `superi-session` may coordinate those owners but must remain independent of the window toolkit.
- `superi-ui` owns transient retained presentation and never becomes a project or engine owner.
- `superi-desktop` translates operating-system events and owns no duplicate UI model.
- `superi-gpu::GpuSubmissionQueue` remains the sole normal command submission owner.
- Private capture uses an explicit inspection readback and is not a playback or public pixel route.
- Generated TypeScript and editorial contracts remain valid offline API consumers.
- Historical checkpoint records are evidence about earlier revisions, not current runtime routes.

## Verification boundary

The map validator checks exact module discovery, source hashes, complete per-file inventories,
required sections, index links, and forbidden Unicode dash characters. Checkpoint verification adds
formatting, strict Clippy, tests, dependency policy, boundary checks, generated binding drift,
fixtures, and native UI proofs selected from the complete diff.
