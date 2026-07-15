---
module_id: superi-project
source_paths:
  - open/crates/superi-project
source_hash: 52aee870902e9505b12355befe2715fa6debc35dc4315bed7d687672bf615dad
source_files: 8
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-project` owns the coherent whole-project aggregate and the stable schema-1 SQLite
serialization boundary. One `ProjectDocument` combines the validated editorial project, selected
root timeline, retained compiled timeline graphs, optional named standalone editable graphs, and
one optimistic document revision. Immutable `ProjectSnapshot` values give editor, script,
headless, persistence, and engine consumers one equal published state.

`ProjectDatabase` is the only public whole-project database authority. It creates a new
nonoverwriting `.superi` database or secured in-memory database, opens an existing database
read-only, replaces all semantic rows in one checked transaction, and reconstructs one complete
`ProjectDocument`. The schema persists canonical timeline and graph component documents instead of
copying their domain models into competing SQL fields.

This module does not yet own migrations between project schema revisions, temporary save
publication, filesystem synchronization, atomic destination replacement, save-as, copy, backup,
settings, history, autosave, recovery journals, unknown extension preservation, modified-since-open
policy, or public API and CLI commands. Those remain assigned to later project checkpoints.

## Source inventory

- `open/crates/superi-project/Cargo.toml`: Declares core, graph, timeline, exact workspace
  `rusqlite`, and workspace SHA-256 dependencies.
- `open/crates/superi-project/src/autosave.rs`: Placeholder for autosave policy and execution.
- `open/crates/superi-project/src/document.rs`: Implements `ProjectDocument`, immutable snapshots,
  private edit candidates, retained timeline compilations, named standalone graphs, revision
  fencing, checked reconstruction, and complete relationship validation.
- `open/crates/superi-project/src/lib.rs`: Documents the implemented aggregate and schema-1
  persistence boundary, exports project modules, and re-exports `ProjectDatabase` plus stable format
  constants.
- `open/crates/superi-project/src/persist.rs`: Implements secured SQLite connections, schema 1,
  deterministic component records and manifest evidence, transactional replacement, strict
  interpretation, bounded decoding, and checked aggregate reconstruction.
- `open/crates/superi-project/src/recovery.rs`: Placeholder for crash recovery.
- `open/crates/superi-project/tests/document_contract.rs`: Proves coherent construction, immutable
  concurrent snapshots, ordinary graph editing, atomic failure behavior, compilation freshness,
  standalone graph identity, explicit revision restoration, and graph identity checks.
- `open/crates/superi-project/tests/persistence_contract.rs`: Proves durable create and read-only
  reopen, exact schema identity, deterministic semantic rows, complete timeline, media, relink,
  graph, and revision preservation, rollback, read-only enforcement, and corruption rejection.

## Public surface

The crate root exports `autosave`, `document`, `persist`, and `recovery`, and re-exports the stable
persistence authority and constants.

- `ProjectDocument::new` accepts one `EditorialProject` and selected `TimelineId`, compiles that
  root through `superi_timeline::compile_timeline`, validates the aggregate, and starts document
  revision zero.
- `ProjectDocument::from_parts` restores an explicit document revision and complete graph
  collection after an outer decoder has validated its format. It rejects duplicate identities and
  validates every relationship.
- `ProjectDocument::snapshot` returns a cloneable immutable `ProjectSnapshot`. Document and
  snapshot accessors expose project identity, revision, selected root, editorial state, stable graph
  iteration, graph lookup, and timeline compilation lookup.
- `ProjectDocument::edit` requires the exact current revision, changes one private candidate,
  validates the complete result, and publishes once only when semantic state changed.
- `ProjectDraft` exposes candidate editorial and graph mutation, root selection, graph membership,
  and explicit timeline recompilation or replacement.
- `ProjectGraph::restore_timeline` deterministically regenerates trusted compilation provenance
  around a decoded editable graph only when the graph identity matches the same project and root.
- `StandaloneProjectGraph` owns one nonblank name and one ordinary
  `EditableGraph<CompiledTimelineGraphValue>`.
- `ProjectDatabase::create` reserves a new path without overwriting an existing file, secures the
  connection, creates exact schema 1, and records the Superi application and schema identities.
- `ProjectDatabase::memory` creates the same secured schema without filesystem state.
- `ProjectDatabase::open_read_only` opens existing state without write authority and validates
  database integrity, application identity, schema revision, and exact schema objects.
- `ProjectDatabase::replace` pre-encodes one immutable snapshot, bounds all content, writes every
  row in one immediate transaction, reloads the candidate through public component decoders,
  compares the exact snapshot, and commits only after equality.
- `ProjectDatabase::load` verifies the database, metadata, component lengths and SHA-256 values,
  project manifest, canonical component bytes, graph ownership, and revisions inside one read
  transaction before returning one checked document.
- `PROJECT_APPLICATION_ID`, `PROJECT_SCHEMA_REVISION`, `PROJECT_FORMAT`, and
  `PROJECT_FORMAT_VERSION` identify application `SUPR`, schema `1`, `superi.project`, and `1.0.0`.

## Architecture and data flow

In-memory publication remains one aggregate transaction:

1. A caller captures the current document revision or immutable snapshot.
2. `ProjectDocument::edit` rejects stale input before cloning one unpublished candidate.
3. The closure changes editorial state, retained graph state, root selection, or graph membership
   through `ProjectDraft`.
4. Validation checks the selected root, current compilation revision and project identity, map and
   graph identities, unique compiled roots, and standalone names.
5. Failure discards the candidate, a no-op returns the existing snapshot, and a real change advances
   once and replaces the shared state.

Schema-1 replacement preserves that exact published state:

1. Timeline serializes the complete editorial project into canonical `superi.timeline` bytes.
2. Graph serializes every retained graph snapshot in stable `GraphId` order into canonical
   `superi.graph` bytes. Timeline and standalone ownership, root or name, graph revision, byte
   length, and SHA-256 remain explicit row evidence.
3. A domain-separated manifest covers project and component format revisions, primitive revision,
   project identity, document revision, selected root, timeline evidence, and ordered graph
   evidence.
4. One immediate SQLite transaction replaces metadata, the singleton timeline component, and graph
   rows. A preflight or transaction failure preserves the prior complete database.
5. Before commit, the candidate database passes `quick_check`, exact application and schema checks,
   exact schema-object inspection, row and size bounds, component and manifest integrity, canonical
   component decoding, checked graph reconstruction, and exact `ProjectSnapshot` equality.

Load follows the same path without partial publication. One deferred read transaction pins a
coherent SQLite snapshot across every identity, schema, metadata, component, and manifest query.
Timeline bytes reconstruct the validated `EditorialProject`. Each timeline graph uses
`ProjectGraph::restore_timeline`, each standalone graph uses `StandaloneProjectGraph::new`, and
`ProjectDocument::from_parts` joins the complete state at the stored revision. Direct graph edits
are never replaced by recompilation; compilation supplies only trusted provenance around the
decoded graph.

Connections enable SQLite defensive mode, foreign keys, cell-size checks, and a finite busy
timeout. They disable triggers, views, trusted schema, double-quoted string literals, and memory
mapping. Read-only connections also enable query-only mode. Schema 1 contains no trigger, view,
extension, network, process, device, or GPU behavior.

`superi-engine::resources::acquire_project_resources` remains the real downstream consumer. It
clones the exact selected compilation from `ProjectSnapshot`, including reloaded direct graph
edits, then acquires the exact reachable source and decoder set before one resources publication.

## Dependencies and consumers

- `superi-core` supplies typed identities, stable primitive revision, classified errors,
  recoverability, diagnostic context, and the shared `Result`.
- `superi-graph` supplies editable graph snapshots and canonical graph encoding and checked
  decoding.
- `superi-timeline` supplies the editorial model, compiler, canonical timeline component codec, and
  strict Serde support for `TimelineGraphValue` inside graph component documents.
- Exact `rusqlite` 0.32.1 supplies safe SQLite access with bundled SQLite 3.46.0 and no public SQL or
  connection type leakage. Its bundled feature also exposes modern defensive configuration.
- Exact `sha2` 0.10.9 supplies component and project manifest integrity without defining the later
  public dirty-state hash.
- `superi-engine` consumes immutable snapshots for transactional resource acquisition.
- API and CLI do not yet expose database or document commands. Later public commands must wrap this
  owner instead of creating another project or database authority.

## Invariants and operational boundaries

- The selected root exists and has exactly one retained timeline compilation at the current
  editorial revision. Every retained graph identity is unique and equals its ordered map key.
- Document edits and database replacement are all-or-nothing. Stale revisions, failed closures,
  invalid candidates, preflight bounds, SQL failures, failed reload, or snapshot inequality publish
  nothing.
- Schema identity is explicit at three levels: SQLite application ID, monotonic schema revision,
  and semantic format plus version. The stable primitive and component revisions are recorded too.
- Schema 1 has exactly three strict tables, one metadata singleton, one timeline singleton, and
  bounded graph rows. Extra user tables, indexes, views, or triggers are corruption.
- Project and graph revisions use canonical decimal text so every `u64` revision is preserved.
  Typed IDs use fixed 16-byte big-endian blobs.
- Canonical component bytes remain owned by timeline and graph. Project stores them with exact
  length and SHA-256 evidence and does not duplicate their semantic fields.
- The project manifest is private integrity evidence. It is not C014's public dirty-state hash.
- Wrong application identity and future schema or format versions are unsupported. Malformed,
  noncanonical, tampered, missing, extra, or inconsistent stored state is corrupt data.
- Semantic row order, component bytes, and manifest digest are deterministic. SQLite page layout is
  not a public deterministic contract.
- `create` does not overwrite an existing path. Atomic destination replacement, save-as, copy,
  backup, migration, autosave, and recovery remain outside C002.

## Tests and verification

`document_contract.rs` contains six public tests over coherent construction, immutable equal
snapshots shared across three roles, ordinary typed graph edits, stale and failed edit rollback,
no-op revision stability, editorial recompilation coherence, standalone graph state, and checked
parts restoration.

`persistence_contract.rs` contains five real database tests. They prove nonoverwriting creation and
read-only reopen, exact schema objects and version identities, equal semantic rows across independent
databases, complete editorial, media fingerprint mismatch, multicam, direct graph edit, standalone,
and revision preservation, post-load conflict fencing and editing, preflight rollback, read-only
denial, and rejection of wrong application identity, future revisions, corrupt components, altered
manifest evidence, missing rows, and extra views.

Timeline's serialization contract independently round trips a real compiled multicam graph through
the public graph codec and rejects unknown `TimelineGraphValue` fields and tags. The engine resource
contract proves that exact project-retained graph state reaches a real media preparation consumer.
Rust 1.80.1 check, strict Clippy, dependency direction, policy, formatting, map validation, and the
repository verifier remain required delivery gates.

## Current status and risks

The coherent in-memory document owner, schema-1 SQLite application format, deterministic component
records, integrity manifest, transactional replacement, checked reconstruction, durable create and
read-only reopen, and engine snapshot consumer are substantive and test-backed.

Project schema migrations, synchronized temporary save publication, atomic destination replacement,
save-as, copy, backup, settings, history, autosave, recovery, unknown extension preservation,
modified-since-open policy, dirty-state hashing, public API adaptation, CLI, and scripting remain
absent. Schema 1 intentionally rejects older, future, or extended layouts until their owning
checkpoints provide an explicit migration or preservation contract.

Bundled SQLite increases the locked build graph and must remain pinned to the Rust 1.80-compatible
rusqlite 0.32.1 path unless a separately verified upgrade changes that decision. Integrity digests
detect changes but do not authenticate malicious files. Defensive configuration, strict schema
inspection, bounds, and checked domain reconstruction remain mandatory for untrusted projects.

## Maintenance notes

Keep one project aggregate, one database authority, and one publication revision. New semantic
state must enter `ProjectState`, immutable snapshot access, complete validation, schema evidence,
round-trip comparison, tests, and maps together. Do not add hidden intelligent-result storage, a
second editorial owner, unchecked graph mutation, or a second persistence model.

Preserve timeline and graph component ownership. Incompatible project layout changes require a new
monotonic schema revision, semantic version decision, and explicit C003 migration. Do not change
schema 1 in place after release. Keep filesystem publication, replacement, backups, autosave, and
recovery behavior in their assigned modules and checkpoints.

Refresh this map after any project source, manifest, public consumer, schema, or test change. Reread
every changed file and relevant component interface through EOF, reconcile prose before recomputing
the hash, and validate all maps after integration and immediately before delivery.
