---
module_id: superi-timeline
source_paths:
  - open/crates/superi-timeline
source_hash: 36c3a19546f9085bc6bfc01176072ad06dae79ea8350c44c266934f58cac6764
source_files: 9
mapped_at_commit: a11cecdbf19ae1de90d94324abe844db49ed0c85
---

## Purpose and ownership

`superi-timeline` reserves the Rust-native editorial model, OTIO-compatible interchange, editing operations, markers, multicam, nesting, and compilation to the generic graph. It currently has no timeline types, serializer, editor, or compiler.

## Source inventory

- `open/crates/superi-timeline/Cargo.toml`: Declares dependencies on `superi-core` and `superi-graph`.
- `open/crates/superi-timeline/src/compile.rs`: Placeholder for timeline-to-graph compilation.
- `open/crates/superi-timeline/src/edit_ops.rs`: Placeholder for ripple, roll, slip, slide, razor, and three-point or four-point edit primitives.
- `open/crates/superi-timeline/src/lib.rs`: Documents the intended editorial subsystem and exports seven placeholder modules.
- `open/crates/superi-timeline/src/markers.rs`: Placeholder for markers, metadata, bins, and media management.
- `open/crates/superi-timeline/src/model.rs`: Placeholder for tracks, clips, transitions, and edit decisions.
- `open/crates/superi-timeline/src/multicam.rs`: Placeholder for a multicam data model.
- `open/crates/superi-timeline/src/nested.rs`: Placeholder for nested sequences and compound clips.
- `open/crates/superi-timeline/src/otio.rs`: Placeholder for OTIO-compatible serialization, with the mechanism explicitly unresolved in the source.

## Public surface

The library exports `compile`, `edit_ops`, `markers`, `model`, `multicam`, `nested`, and `otio`. The modules contain no types, functions, serializers, schemas, or graph compiler.

## Architecture and data flow

There is no implemented editorial data flow. The manifest establishes a future path from core time and identity types into a timeline model and from timeline snapshots into the generic graph. No source currently imports core or graph, and no timeline data can reach project persistence or engine orchestration.

## Dependencies and consumers

- Declared dependencies are `superi-core` and `superi-graph`. They are unused in source.
- `superi-project` and `superi-engine` declare `superi-timeline` as a dependency.
- Neither consumer references a `superi_timeline` Rust item, and no API or CLI code exposes timeline behavior.

## Invariants and operational boundaries

- Cargo places timeline above the node-agnostic graph and below project and engine.
- Rust-native ownership, OTIO compatibility, opaque unknown-field preservation, deterministic incremental compilation, stable derived identifiers, edit semantics, and undoability are intended contracts only.
- The OTIO mechanism is explicitly unresolved in `otio.rs`, and the crate has no OTIO dependency, schema set, or compatibility fixture.

## Tests and verification

The crate owns no tests, examples, benchmarks, or OTIO fixtures. Compilation proves only the placeholder namespace and manifest layering.

## Current status and risks

All Rust files are explicit skeletons. The unresolved OTIO path and absent model mean no editorial compatibility, graph compilation, or edit correctness can be claimed.

## Maintenance notes

When implementation begins, map the timeline object model, time semantics, stable identifiers, validation, edit transformations, nesting, multicam, marker metadata, OTIO version behavior, graph derivation, and consumers. Treat timeline-to-graph determinism and interchange preservation as test-backed properties only.
