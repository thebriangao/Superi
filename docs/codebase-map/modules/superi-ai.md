---
module_id: superi-ai
source_paths:
  - open/crates/superi-ai
source_hash: ac7aa927cdd049e11c800d617046d15f70497a3a39db8da2caa92693840998a4
source_files: 8
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-ai` owns the open-tree boundary for local, offline inference and AI transformations whose results must become ordinary editable graph artifacts. It now owns one truthful process-local capability snapshot that separates enforced architectural requirements from executable availability. It still does not implement an inference runtime, model loading, auditing, pipelines, or artifact conversion.

## Source inventory

- `open/crates/superi-ai/Cargo.toml`: Declares the crate and its dependencies on `superi-core`, `superi-image`, and `superi-graph`.
- `open/crates/superi-ai/src/artifacts.rs`: Placeholder for representing AI outputs as editable graph artifacts.
- `open/crates/superi-ai/src/audit.rs`: Placeholder for bundled-model licensing audits.
- `open/crates/superi-ai/src/capabilities.rs`: Defines schema revision 1, the explicit unavailable runtime state, an empty executable-pipeline catalog, local-only and editable-artifact requirements, getters, and side-effect-free discovery.
- `open/crates/superi-ai/src/lib.rs`: Documents the intended offline AI boundary and publicly exposes the capability owner plus the four placeholder modules.
- `open/crates/superi-ai/src/pipelines.rs`: Placeholder naming the intended transcription, cleanup, analysis, tracking, reframing, matching, search, and transcript-edit pipelines.
- `open/crates/superi-ai/src/runtime.rs`: Placeholder for a local inference runtime with permissively licensed bundled models.
- `open/crates/superi-ai/tests/capabilities_contract.rs`: Proves that discovery is schema 1, local-only, editable-artifact constrained, runtime unavailable, and empty of executable pipelines.

## Public surface

The library exports `artifacts`, `audit`, `capabilities`, `pipelines`, and `runtime`. `capabilities` exposes `AI_CAPABILITY_SCHEMA_VERSION`, `AiRuntimeAvailability`, `AiPipelineCapability`, `AiCapabilities`, and `discover_local_capabilities`. The returned immutable snapshot uses getters and reports only implemented executable availability. The remaining modules contain documentation and TODO text only.

## Architecture and data flow

The only implemented data flow is read-only discovery. `discover_local_capabilities` constructs a schema-1 value with `Unavailable` runtime state, required local-only execution, required editable artifacts, and an empty pipeline list. The desktop shell converts that value into its strict four-domain hardware snapshot and renders it in the System panel. No AI call loads a model, accepts media, produces graph state, starts work, or performs local or remote inference.

## Dependencies and consumers

- Declared dependencies are `superi-core`, `superi-image`, and `superi-graph`. They are manifest-only dependencies today.
- `superi-engine` declares `superi-ai` as a dependency, but no engine source references a `superi_ai` item.
- `superi-desktop` directly consumes `discover_local_capabilities` for honest shell-level operational visibility. It does not use the crate as an execution path.

## Invariants and operational boundaries

- The Cargo dependency direction is downward into core, image, and graph. The crate does not depend on engine or API.
- Discovery never equates a required architectural boundary with runtime availability. Local-only and editable-artifact requirements are true while runtime availability remains explicitly unavailable.
- Available pipelines stay empty until an audited executable owner exists. Capability discovery performs no I/O, network access, model loading, media mutation, graph mutation, or hidden fallback.
- No networking, model weights, native runtime, file format, or license-audit mechanism is present.

## Tests and verification

`capabilities_contract.rs` proves the exact absent-runtime contract and prevents named future pipeline intent from being advertised as executable capability. The desktop Rust and frontend contracts separately prove the consumer projection, persistence, strict parsing, and visible unavailable state. There are no model, pipeline, artifact-production, or license-audit tests because those systems remain absent.

## Current status and risks

The capability owner is implemented, while the runtime, audit, pipeline, and artifact modules remain explicit skeletons. The main risk is overstating the capability value as an implemented AI engine. The explicit unavailable runtime, empty available-pipeline list, separate requirement flags, and focused contract prevent that misrepresentation.

## Maintenance notes

When any AI path becomes real, update capability discovery only after the concrete runtime, model lifecycle, inputs, outputs, graph mutation path, offline enforcement, licensing proof, failure behavior, and tests exist. Recheck the desktop projection, engine consumer, and public artifact boundary together so availability, user control, and editable output remain coherent.
