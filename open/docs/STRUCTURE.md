# Galileo Open Tree: Structure

How the workspace is organized, why, and how to work in it.

## The principle

One cargo crate per `§5` subsystem (`../../docs/architecture.md`). The crate graph **is** the
architecture: dependencies point downward only, so the engine/UI separation, the no-subsystem-bleed
rule, and the codec boundary are enforced by the Rust compiler, not by convention.

## Dependency tiers (downward-only, acyclic)

| tier | crates | depends on |
|---|---|---|
| T0 | `galileo-core` | none |
| T1 | `galileo-image` | core |
| T1 | `galileo-gpu` | core, image |
| T1 | `galileo-concurrency` | core, gpu |
| T1 | `galileo-media-io` | core, image |
| T1b | `galileo-codecs-rs` (default backend) | core, image, media-io |
| T1b | `galileo-codecs-platform` (opt-in, `os-codecs`) | core, image, media-io |
| T2 | `galileo-graph` (node-agnostic) | core, gpu, image, concurrency |
| T2 | `galileo-cache` | core, gpu, image, graph |
| T3 | `galileo-color` | core, gpu, image, graph |
| T3 | `galileo-effects` | core, gpu, image, graph |
| T3 | `galileo-timeline` | core, graph |
| T3 | `galileo-audio` | core, concurrency |
| T3 | `galileo-ai` | core, image, graph |
| T4 | `galileo-project` | core, graph, timeline |
| T4 | `galileo-engine` (orchestration) | all T0-T4 (+ codecs-platform via `os-codecs`) |
| T5 | `galileo-api` (the public seam) | core, engine |
| T6 | `galileo-cli` (first consumer) | core, api |

**Invariant:** `galileo-graph` never depends on `galileo-color`/`galileo-effects`, node catalogs depend on
the graph, never the reverse. New capability = new node type, not a new dependency on the engine core.

## Codecs

See `../../docs/codecs.md`. Default build = royalty-free, pure-Rust, in-tree. `--features os-codecs`
adds the user's-OS decode path for encumbered formats. Nothing encumbered or copyleft ever links into
the MIT core.

## Suggested ownership (6 engineers)

| dev | crates | theme |
|---|---|---|
| 1 | `galileo-gpu`, `galileo-concurrency` | graphics / systems |
| 2 | `galileo-graph`, `galileo-cache` | core engine |
| 3 | `galileo-media-io`, `galileo-codecs-rs`, `galileo-codecs-platform`, `galileo-image` | media / IO / codecs |
| 4 | `galileo-color`, `galileo-effects` | color / comp |
| 5 | `galileo-timeline`, `galileo-project`, `galileo-api` | editorial + public surface |
| 6 | `galileo-audio`, `galileo-ai` | audio + AI |
| none | `galileo-core`, `galileo-engine`, `galileo-cli` | shared / lead-stewarded |

## Working in this workspace

- A **crate is the natural unit of ownership**, with a focused responsibility and an explicit
  dependency contract.
- Compiler-enforced boundaries support parallel development: work in `galileo-color` cannot
  accidentally reach into `galileo-gpu` internals or create a cycle.
- Respect the DAG. If a task seems to need an upward dependency, the design is wrong, stop and flag.
- The **offline law** and the **codec/license boundary** are sacred (`../../docs/architecture.md`,
  `../../docs/codecs.md`).

## Deferred (not in this scaffold)

Network-isolated offline CI test · license-audit CI (`deny.toml` is present, unwired) · the vertical
slice (`import → trim → effect → export`) · the `closed/` tree · the web UI · codec legal sign-off
(open item #1) · the OTIO mechanism (open item #2).
