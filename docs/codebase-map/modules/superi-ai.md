---
module_id: superi-ai
source_paths:
  - open/crates/superi-ai
source_hash: 2ca549731614bc2dbc745fb5945782ab1acf898fd501ae900b7c712be5d1c338
source_files: 6
mapped_at_commit: a11cecdbf19ae1de90d94324abe844db49ed0c85
---

## Purpose and ownership

`superi-ai` reserves the open-tree crate boundary for local, offline inference and AI transformations whose results become ordinary editable graph artifacts. The crate currently owns only the intended namespace and dependency direction. It does not implement an inference runtime, model loading, auditing, pipelines, or artifact conversion.

## Source inventory

- `open/crates/superi-ai/Cargo.toml`: Declares the crate and its dependencies on `superi-core`, `superi-image`, and `superi-graph`.
- `open/crates/superi-ai/src/artifacts.rs`: Placeholder for representing AI outputs as editable graph artifacts.
- `open/crates/superi-ai/src/audit.rs`: Placeholder for bundled-model licensing audits.
- `open/crates/superi-ai/src/lib.rs`: Documents the intended offline AI boundary and publicly exposes the four placeholder modules.
- `open/crates/superi-ai/src/pipelines.rs`: Placeholder naming the intended transcription, cleanup, analysis, tracking, reframing, matching, search, and transcript-edit pipelines.
- `open/crates/superi-ai/src/runtime.rs`: Placeholder for a local inference runtime with permissively licensed bundled models.

## Public surface

The library exports the module names `artifacts`, `audit`, `pipelines`, and `runtime`. Each exported module contains documentation and a TODO only, so there are no public types, functions, traits, constants, or executable operations.

## Architecture and data flow

There is no implemented data flow. The manifest establishes a future downward path from AI code to core types, image primitives, and graph artifacts. No Rust source currently imports any of those crates, loads a model, accepts media, produces graph state, or performs local or remote inference.

## Dependencies and consumers

- Declared dependencies are `superi-core`, `superi-image`, and `superi-graph`. They are manifest-only dependencies today.
- `superi-engine` declares `superi-ai` as a dependency, but no engine source references a `superi_ai` item.
- No other workspace Rust code consumes this crate's module surface.

## Invariants and operational boundaries

- The Cargo dependency direction is downward into core, image, and graph. The crate does not depend on engine or API.
- The source documentation states that inference must be local and offline and that outputs must be editable graph artifacts. These are intended boundaries only, because no runtime or enforcement exists yet.
- No networking, model weights, native runtime, file format, or license-audit mechanism is present.

## Tests and verification

The crate owns no unit tests, integration tests, examples, or benchmarks. Compilation can prove only that the empty public module structure and manifest dependency graph are valid.

## Current status and risks

This entire crate is an explicit skeleton. All five Rust files are documentation-only placeholders, and the Cargo manifest is the only operative artifact. The largest risk is mistaking named modules and pipeline lists for implemented offline, licensing, artifact, or inference guarantees.

## Maintenance notes

When any AI path becomes real, replace the applicable placeholder description with the concrete model lifecycle, inputs, outputs, graph mutation path, offline enforcement, licensing proof, failure behavior, and tests. Recheck both the direct dependencies and the engine consumer because neither currently exercises this crate.
