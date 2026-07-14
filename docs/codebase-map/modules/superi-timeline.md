---
module_id: superi-timeline
source_paths:
  - open/crates/superi-timeline
source_hash: 480e49a4e44c88ddd83dc83dc9e6eb51782f117f1ebb204c1498ed27f9abc46f
source_files: 10
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-timeline` reserves the Rust-native editorial model, OTIO-compatible interchange, editing
operations, markers, multicam, nesting, and compilation to the generic graph. It currently has no
timeline types, serializer, editor, or compiler. It does own the semantic consumer contract for the
canonical OTIO 0.18.1 fixture baseline so future production work starts from executable interchange
evidence.

## Source inventory

- `open/crates/superi-timeline/Cargo.toml`: Declares runtime dependencies on `superi-core` and
  `superi-graph`, plus development-only `serde_json` for the canonical OTIO fixture contract.
- `open/crates/superi-timeline/src/compile.rs`: Placeholder for timeline-to-graph compilation.
- `open/crates/superi-timeline/src/edit_ops.rs`: Placeholder for ripple, roll, slip, slide, razor, and three-point or four-point edit primitives.
- `open/crates/superi-timeline/src/lib.rs`: Documents the intended editorial subsystem and exports seven placeholder modules.
- `open/crates/superi-timeline/src/markers.rs`: Placeholder for markers, metadata, bins, and media management.
- `open/crates/superi-timeline/src/model.rs`: Placeholder for tracks, clips, transitions, and edit decisions.
- `open/crates/superi-timeline/src/multicam.rs`: Placeholder for a multicam data model.
- `open/crates/superi-timeline/src/nested.rs`: Placeholder for nested sequences and compound clips.
- `open/crates/superi-timeline/src/otio.rs`: Reserves the ratified OTIO-compatible serialization
  boundary, points to the shared 0.18.1 fixtures, and leaves the production reader, writer, and
  model to P2.W02.
- `open/crates/superi-timeline/tests/otio_fixture_contract.rs`: Consumes both canonical OTIO
  timelines and their expectation record. It proves exact schema labels, hierarchy, stable IDs,
  rational timing, media linkage, transition relationships, gaps, owner-relative markers, nesting,
  two linear rate changes, opaque JSON retention, and explicit unsupported diagnostics.

## Public surface

The library exports `compile`, `edit_ops`, `markers`, `model`, `multicam`, `nested`, and `otio`. The
modules contain no types, functions, serializers, schemas, or graph compiler. The OTIO fixture test
is a development consumer and adds no public runtime surface.

## Architecture and data flow

There is no implemented editorial runtime data flow. The manifest establishes a future path from
core time and identity types into a timeline model and from timeline snapshots into the generic
graph. No source currently imports core or graph, and no timeline data can reach project persistence
or engine orchestration. Test flow runs from the shared `timeline/otio-interchange/v1` payloads
through `serde_json::Value` assertions, preserving complete opaque JSON while checking semantic
contracts that the future native importer and exporter must satisfy.

## Dependencies and consumers

- Declared dependencies are `superi-core` and `superi-graph`. They are unused in source.
- `serde_json` is development-only and reads checked-in canonical JSON. No OTIO library, Python
  package, network path, or fixture-tool runtime dependency enters the crate.
- `superi-project` and `superi-engine` declare `superi-timeline` as a dependency.
- Neither consumer references a `superi_timeline` Rust item, and no API or CLI code exposes timeline behavior.

## Invariants and operational boundaries

- Cargo places timeline above the node-agnostic graph and below project and engine.
- Rust-native ownership, production OTIO reading and writing, deterministic incremental compilation,
  edit semantics, and undoability remain intended contracts only.
- The OTIO mechanism is ratified as a native Rust JSON reader and writer with opaque unknown-field
  preservation. The fixture targets OTIO_CORE:0.18.1 and records exact preservation diagnostics,
  but the production implementation remains absent.
- LinearTimeWarp values preserve clip duration while changing sampled source speed. Transition
  offsets preserve adjacent relationships without increasing track duration. Marker ranges remain
  relative to their owning item.

## Tests and verification

Three integration tests consume the shared canonical OTIO fixture. They prove the exact first
editorial slice, comprehensive interchange structure and metadata, two rate changes, stable
unsupported-object pointers, preserve-opaque policy, and the warning code
`timeline.otio.unsupported_construct`. Official OpenTimelineIO 0.18.1 separately loads both files,
reports exact 48-frame and 120-frame durations at 24 fps, and preserves semantic equivalence through
targeted write and read. Compilation of the library itself still proves only placeholder namespace
and manifest layering.

## Current status and risks

All production Rust files remain skeletons. Canonical fixture compatibility and future importer
expectations are now test-backed, but the absent model, reader, writer, and compiler mean runtime
editorial compatibility, graph compilation, and edit correctness cannot be claimed.

## Maintenance notes

When implementation begins, map the timeline object model, time semantics, stable identifiers,
validation, edit transformations, nesting, multicam, marker metadata, OTIO version behavior, graph
derivation, and consumers. Extend the versioned fixture and expectations before changing importer or
exporter semantics. Treat timeline-to-graph determinism and production interchange preservation as
test-backed properties only.
