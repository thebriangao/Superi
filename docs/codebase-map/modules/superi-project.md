---
module_id: superi-project
source_paths:
  - open/crates/superi-project
source_hash: c5617090fa9c26e08136ecd4548607469ec8e57137c094cfa65cf057269ee338
source_files: 7
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-project` owns the in-memory whole-project aggregate. One `ProjectDocument` combines the
validated editorial project, selected root timeline, retained compiled timeline graphs, optional
named standalone editable graphs, and one optimistic document revision. It publishes immutable
`ProjectSnapshot` values that editor, script, headless, and engine consumers can share without
copying or observing partial mutation.

The same crate reserves the higher project boundary for stable serialization, migration, atomic
save, autosave, and crash recovery. Those durable paths remain placeholders and are not implied by
the implemented in-memory model.

## Source inventory

- `open/crates/superi-project/Cargo.toml`: Declares the core identity and error contract, generic
  editable graph state, and editorial timeline model dependencies.
- `open/crates/superi-project/src/autosave.rs`: Placeholder for autosave policy and execution.
- `open/crates/superi-project/src/document.rs`: Implements `ProjectDocument`, immutable snapshots,
  private edit candidates, retained timeline compilations, named standalone graphs, exact revision
  fencing, and cross-object validation.
- `open/crates/superi-project/src/lib.rs`: Documents the implemented in-memory boundary and exports
  the document plus staged durability modules.
- `open/crates/superi-project/src/persist.rs`: Placeholder for whole-project serialization.
- `open/crates/superi-project/src/recovery.rs`: Placeholder for crash recovery.
- `open/crates/superi-project/tests/document_contract.rs`: Proves coherent construction, immutable
  concurrent snapshots, ordinary graph editing, atomic failure behavior, compilation freshness,
  standalone graph identity, and root validation.

## Public surface

The crate root exports `autosave`, `document`, `persist`, and `recovery`. Only `document` is
substantive.

- `ProjectDocument::new` accepts one `EditorialProject` and selected `TimelineId`, compiles that
  root through `superi_timeline::compile_timeline`, validates the aggregate, and starts document
  revision zero.
- `ProjectDocument::from_parts` is the checked reconstruction seam for later persistence work. It
  accepts an explicit document revision and complete graph collection, rejects duplicate graph
  identities, and performs all relationship validation without decoding or migration policy.
- `ProjectDocument::snapshot` returns a cloneable `ProjectSnapshot` backed by shared immutable
  state. Document and snapshot accessors expose project identity, document revision, selected root,
  editorial state, stable graph iteration, graph lookup, and timeline compilation lookup.
- `ProjectDocument::edit` requires the exact current document revision, clones one unpublished
  candidate, invokes a caller closure with `ProjectDraft`, validates the complete result, and
  publishes one new shared state only when semantic state changed.
- `ProjectDraft` exposes candidate editorial access, graph lookup and mutation, root selection,
  graph insertion, replacement, and non-root removal, plus explicit timeline recompilation or
  compilation replacement.
- `ProjectGraph` distinguishes retained `TimelineGraphCompilation` values from
  `StandaloneProjectGraph` values while exposing their common editable graph and immutable graph
  snapshot surfaces. `ProjectGraph::restore_timeline` deterministically regenerates compilation
  provenance from a validated editorial project and installs externally reconstructed editable
  graph state only when its stable graph identity matches the same project and root.
- `StandaloneProjectGraph` owns one nonblank editor-facing name and one
  `EditableGraph<CompiledTimelineGraphValue>`. Its name and graph remain ordinary checked mutable
  project state during a document edit.

## Architecture and data flow

Initial construction compiles the selected editorial root and retains the complete
`TimelineGraphCompilation`, including project identity, root identity, editorial revision,
bidirectional provenance index, editable graph, and graph revision. It does not flatten compiled
state into a weaker project record.

The publication flow is:

1. A caller captures the current document revision or immutable snapshot.
2. `ProjectDocument::edit` rejects a stale expected revision before cloning state.
3. The closure changes editorial state, retained graph state, root selection, or graph membership
   only inside `ProjectDraft`.
4. Complete validation requires the selected editorial timeline and its retained compilation,
   checks every graph map identity, rejects duplicate compiled roots, validates every standalone
   name, and requires every compilation to match the same project and current editorial revision.
5. A failed closure or validation error discards the complete candidate. A successful no-op returns
   the existing snapshot without advancing. A changed candidate advances the document revision once
   and atomically replaces the shared `Arc` state.
6. Prior snapshots retain the old immutable state. New editor, script, headless, persistence, or
   engine consumers receive only one coherent published revision.

Editorial edits intentionally make retained compilations stale until the same candidate explicitly
recompiles or replaces them. Direct graph transactions do not change the editorial revision and
remain part of the retained project graph. This makes generated or intelligent results ordinary
inspectable, controllable, and reusable graph state instead of hidden execution output.

`superi-engine::resources::acquire_project_resources` is the first real downstream consumer. It
clones the exact selected compilation from `ProjectSnapshot`, including direct graph edits, then
uses the snapshot editorial state to acquire the exact reachable source and decoder set before one
`TimelineResources` publication.

## Dependencies and consumers

- `superi-core` supplies `ProjectId`, `TimelineId`, `GraphId`, classified errors, recoverability,
  diagnostic contexts, and the shared `Result` type.
- `superi-graph` supplies `EditableGraph` and immutable `GraphSnapshot` state for standalone and
  timeline-owned graphs.
- `superi-timeline` supplies the validated `EditorialProject`,
  `CompiledTimelineGraphValue`, `TimelineGraphCompilation`, and deterministic compiler.
- `superi-engine` directly consumes immutable project snapshots for transactional resource
  acquisition and preserves the legacy editorial-only acquisition entry point.
- `superi-api` and `superi-cli` do not yet consume this model. Their later stable command, query,
  event, and scripting surfaces must wrap this owner rather than introduce another project state.
- Future persistence code can enumerate and clone the public aggregate, then call `from_parts` only
  after format validation and migration. The document model itself performs no bytes or file I/O.

## Invariants and operational boundaries

- The selected root timeline exists in the editorial project and has exactly one retained
  compilation.
- Every retained timeline compilation belongs to the same `ProjectId`, names an existing timeline,
  and captures the current editorial revision. One timeline root cannot have multiple retained
  compilations.
- Every ordered map key equals the actual `GraphId`, and duplicate graph identities are rejected.
- Standalone graphs have nonblank names and use the same ordinary checked graph transaction surface
  as timeline graphs.
- Stale document revisions, closure failures, invalid candidates, and revision exhaustion publish
  nothing. Successful no-op edits do not advance the document revision.
- Immutable snapshots are `Send + Sync`, and later document edits cannot mutate prior snapshots.
- Timeline recompilation is explicit because it can replace direct processing graph edits. This
  layer does not claim a merge policy, command history, undo, redo, or cross-resource transaction.
- The aggregate owns no file path, extension, schema version, migration graph, SQLite layout,
  locking, checksum, atomic replacement, backup, autosave cadence, recovery journal, or unknown-data
  preservation policy.

## Tests and verification

`document_contract.rs` contains six public contract tests. They prove selected-root construction
and retained compilation identity, immutable equal snapshots shared across three thread roles,
ordinary typed graph parameter edits that preserve prior snapshots, stale and failed edit rollback,
no-op revision stability, editorial revision and recompilation coherence, named standalone graph
insertion, duplicate identity rejection, blank-name rejection, missing-root classification, exact
edited graph restoration at an explicit document revision, and mismatched graph identity rejection.

The engine media acquisition contract adds a real WebM and AV1 consumer proof. It mutates one
compiled clip parameter through `ProjectDocument`, acquires resources from `ProjectSnapshot`, and
asserts that the returned compilation exactly equals the published graph at revision two while the
real media stream is opened and identified.

## Current status and risks

The in-memory document owner, immutable snapshot surface, atomic revision-fenced edits, retained
timeline compilation model, standalone graph model, and engine resource consumer are substantive
and test-backed. Serialization, migrations, durable save, backup, settings, history, autosave,
recovery, extension preservation, public API adaptation, CLI, and scripting remain absent.

Recompiling after an editorial change deterministically reconstructs the timeline graph and can
replace direct graph edits. C001 exposes that operation explicitly and rejects stale publication,
but it does not define semantic graph merge behavior. `ProjectDocument::from_parts` is a checked
model seam, not proof that any future decoder, schema, migration, or file transaction is safe.

## Maintenance notes

Keep one project aggregate and one publication revision. New state must enter `ProjectState`, its
public snapshot accessors, complete validation, focused tests, and the later persistence schema
together. Do not add hidden intelligent-result storage, a second editorial owner, unchecked graph
mutation, or engine-private project state.

Preserve complete `TimelineGraphCompilation` values and their provenance. If recompilation gains a
merge policy, define it at the command or history layer with explicit tests rather than silently
changing document publication. Keep persistence, schema migration, save replacement, autosave, and
recovery behavior in their dedicated modules and checkpoints.
