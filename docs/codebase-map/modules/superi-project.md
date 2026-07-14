---
module_id: superi-project
source_paths:
  - open/crates/superi-project
source_hash: e78a1f151734496003936b6f153474fee87d19947ea97665c925a4fb5be26b48
source_files: 6
mapped_at_commit: a11cecdbf19ae1de90d94324abe844db49ed0c85
---

## Purpose and ownership

`superi-project` reserves ownership for project documents, whole-engine-state persistence, autosave, and crash recovery. It currently has no project model, schema, database, save operation, migration, or recovery implementation.

## Source inventory

- `open/crates/superi-project/Cargo.toml`: Declares dependencies on `superi-core`, `superi-graph`, and `superi-timeline`.
- `open/crates/superi-project/src/autosave.rs`: Placeholder for autosave.
- `open/crates/superi-project/src/document.rs`: Placeholder for the project and document model.
- `open/crates/superi-project/src/lib.rs`: Documents the intended persistence subsystem and exports four placeholder modules.
- `open/crates/superi-project/src/persist.rs`: Placeholder for serialization of whole engine state.
- `open/crates/superi-project/src/recovery.rs`: Placeholder for crash recovery.

## Public surface

The library publicly exports `autosave`, `document`, `persist`, and `recovery`. All four modules contain only documentation and TODO markers, with no usable project or persistence items.

## Architecture and data flow

There is no implemented project lifecycle or persistence flow. The manifest establishes intended access to core, graph, and timeline state. No source imports those crates, and no code creates, loads, saves, migrates, autosaves, validates, or recovers a project.

## Dependencies and consumers

- Declared dependencies are `superi-core`, `superi-graph`, and `superi-timeline`. They are unused in source.
- `superi-engine` declares `superi-project` as a dependency, but no engine source references a `superi_project` item.
- No API or CLI source currently reaches this crate.

## Invariants and operational boundaries

- The dependency direction places projects above graph and timeline and below engine.
- Atomic save, SQLite layout, schema versioning, migrations, external media policy, unknown-data preservation, autosave separation, and recovery behavior are documented repository goals, not implemented crate behavior.
- The crate owns no file extension handling, paths, locking, checksums, transaction IDs, or revision types.

## Tests and verification

The crate owns no tests, examples, fixtures, or benchmarks. Compilation verifies only the empty module surface.

## Current status and risks

Every Rust file is an explicit skeleton. Error-context strings referring to `superi-project` in core tests are generic diagnostics examples and do not constitute a project implementation or consumer.

## Maintenance notes

When project behavior is added, map the exact storage schema, durable-save protocol, schema and format versions, migration graph, validation, autosave cadence and ownership, recovery selection, external artifact policy, and engine or API callers. Do not infer those details from repository design documents once code exists.
