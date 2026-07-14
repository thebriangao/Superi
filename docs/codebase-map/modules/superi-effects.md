---
module_id: superi-effects
source_paths:
  - open/crates/superi-effects
source_hash: eed1a74aa8e8002ad247add0a37647ea4f845de7c6107ddeae51992bcbfeacd1
source_files: 9
mapped_at_commit: a11cecdbf19ae1de90d94324abe844db49ed0c85
---

## Purpose and ownership

`superi-effects` reserves the effect-node catalog boundary for authoring conventions, animation, masks, transitions, tracking, text, and future OFX compatibility. It has no implemented node types, parameter model, renderer, solver, or plugin interface.

## Source inventory

- `open/crates/superi-effects/Cargo.toml`: Declares dependencies on `superi-core`, `superi-gpu`, `superi-image`, and `superi-graph`.
- `open/crates/superi-effects/src/authoring.rs`: Placeholder for internal effect and node authoring conventions.
- `open/crates/superi-effects/src/keyframe.rs`: Placeholder for parameters that vary over time.
- `open/crates/superi-effects/src/lib.rs`: Documents the intended effect subsystem and publicly exports seven placeholder modules.
- `open/crates/superi-effects/src/mask.rs`: Placeholder for mask and rotoscoping data and rendering.
- `open/crates/superi-effects/src/ofx.rs`: Placeholder for an additive OFX-compatible plugin surface.
- `open/crates/superi-effects/src/text.rs`: Placeholder for additive text and motion-design primitives.
- `open/crates/superi-effects/src/tracking.rs`: Placeholder for motion-tracking data and solving.
- `open/crates/superi-effects/src/transition.rs`: Placeholder for transitions.

## Public surface

The library exports the module names `authoring`, `keyframe`, `mask`, `ofx`, `text`, `tracking`, and `transition`. The exported modules contain no types, functions, traits, constants, node registrations, or shaders.

## Architecture and data flow

There is no effect evaluation or authoring flow. The manifest points downward to generic graph, GPU, image, and core crates, matching the intended catalog-over-graph layering. No source currently imports those crates or creates graph nodes.

## Dependencies and consumers

- Declared dependencies are `superi-core`, `superi-gpu`, `superi-image`, and `superi-graph`. They are manifest-only today.
- `superi-engine` declares `superi-effects` as a dependency, but no implemented engine source references a `superi_effects` item.
- `superi-graph` does not depend on this crate, preserving the catalog-to-generic-graph direction.

## Invariants and operational boundaries

- Cargo enforces that effects can depend on graph while graph cannot depend on effects.
- Determinism, serialization, keyframe interpolation, ROI behavior, GPU residency, mask semantics, plugin safety, and text fidelity are not implemented.
- OFX, text, and tracking are explicitly additive placeholders and have no external dependencies or ABI.

## Tests and verification

The crate owns no tests, examples, benchmarks, fixtures, or shaders. Compilation validates only its namespace and dependency declarations.

## Current status and risks

Every Rust file is an explicit skeleton. Public module names may suggest a broad effect system, but no effect artifact can be authored, serialized, evaluated, rendered, or consumed.

## Maintenance notes

When effect types appear, record their graph registration, schemas, time and ROI semantics, GPU or CPU execution, serialization, error behavior, and real engine consumers. Keep generic graph ownership separate from this catalog.
