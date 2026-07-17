---
module_id: superi-project
source_paths:
  - open/crates/superi-project
source_hash: aed4bbea6cbe37ff76eb28a13e67dd2037063ff1171a3385098a3f6dd8a9ee92
source_files: 12
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-project` owns the coherent whole-project aggregate, stable schema-1 SQLite serialization,
and ordered forward migration from supported older project schemas. One `ProjectDocument` combines
the validated editorial project, selected root timeline, retained compiled timeline graphs,
optional named standalone editable graphs, and one optimistic document revision. Immutable
`ProjectSnapshot` values give editor, script, headless, persistence, and engine consumers one equal
published state.

`ProjectDatabase` is the only public whole-project database authority. It creates a new
nonoverwriting `.superi` database or secured in-memory database, opens exact current state
read-only, opens current or supported legacy state with write authority, replaces all semantic rows
in one checked transaction, and reconstructs one complete `ProjectDocument`. Writable open reports
its source schema and migrates exact schema 0 to schema 1 in one immediate transaction. The schema
persists canonical timeline and graph component documents instead of copying their domain models
into competing SQL fields.

The project media boundary interprets timeline-owned opaque targets as versioned filesystem
references when their syntax is known. It owns portable relative path validation, project-file
resolution, explicit host-absolute platform evidence, stable `MediaId` queries, and atomic path,
missing, and fingerprint-checked relink commands without creating a second media state model. The
document also exposes one checked whole-snapshot restore seam for the engine-owned command-history
policy. Restoration preserves project identity, validates the complete aggregate, and publishes a
fresh monotonic document revision instead of reviving an old revision number.

This module does not own command-history storage or policy. It also does not yet own temporary save
publication, filesystem synchronization, atomic destination replacement, save-as, copy, backup,
settings, autosave, recovery journals,
unknown extension preservation, modified-since-open policy, or public API and CLI commands. Those
remain assigned to later project checkpoints.

## Source inventory

- `open/crates/superi-project/Cargo.toml`: Declares core, graph, timeline, exact workspace
  `rusqlite`, workspace SHA-256, and test-only JSON fixture dependencies.
- `open/crates/superi-project/src/autosave.rs`: Placeholder for autosave policy and execution.
- `open/crates/superi-project/src/document.rs`: Implements `ProjectDocument`, immutable snapshots,
  private edit candidates, retained timeline compilations, named standalone graphs, revision
  fencing, checked reconstruction and restoration, fresh monotonic restore publication, and
  complete relationship validation.
- `open/crates/superi-project/src/lib.rs`: Documents the implemented aggregate, schema-1
  persistence, migration, and referenced-media boundaries, exports public project modules, keeps
  migration private, and re-exports `ProjectDatabase` plus stable format constants.
- `open/crates/superi-project/src/migrate.rs`: Owns the exact schema-0 contract, contiguous migration
  registry, secured compatibility decoding, checked aggregate reconstruction, single-transaction
  canonical schema-1 rewrite, full integrity checks, and precommit rollback proof.
- `open/crates/superi-project/src/media.rs`: Implements versioned referenced-media target encoding,
  portable relative path normalization, deterministic project-file resolution, host-platform
  evidence, stable media lookup, and revision-fenced path and relink commands that retain editable
  timeline graphs and suppress false document revisions for semantic no-ops.
- `open/crates/superi-project/src/persist.rs`: Implements secured SQLite connections, schema 1,
  deterministic component records and manifest evidence, transactional replacement, strict
  interpretation, bounded decoding, and checked aggregate reconstruction.
- `open/crates/superi-project/src/recovery.rs`: Placeholder for crash recovery.
- `open/crates/superi-project/tests/document_contract.rs`: Proves coherent construction, immutable
  concurrent snapshots, ordinary graph editing, atomic failure behavior, compilation freshness,
  standalone graph identity, checked reconstruction, revision-fenced whole-snapshot restoration,
  monotonic restore publication, exhaustion atomicity, and graph identity checks.
- `open/crates/superi-project/tests/migration_contract.rs`: Proves public supported legacy open,
  legacy timeline and graph component migration, exact editable-state preservation, canonical
  current reopen, continued editing and replacement, current byte stability, read-only legacy
  refusal, future nonmutation, and malformed legacy rollback.
- `open/crates/superi-project/tests/media_reference_contract.rs`: Proves portable path grammar,
  versioned target round trips, relative and host-absolute resolution, stable identity commands,
  retained direct graph edits, relink conflicts, database round trips, and unknown target handling.
- `open/crates/superi-project/tests/persistence_contract.rs`: Proves durable create and read-only
  reopen, exact schema identity, deterministic semantic rows, complete timeline, media, relink,
  graph, and revision preservation, rollback, read-only enforcement, and corruption rejection.

## Public surface

The crate root exports `autosave`, `document`, `media`, `persist`, and `recovery`, and re-exports the
stable persistence authority, project format constants, and media path target format identifier.

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
- `ProjectDocument::restore_snapshot` requires the exact current revision and matching project
  identity, validates the complete target aggregate, returns the existing snapshot for equal state,
  and otherwise publishes the target contents at one fresh monotonic document revision.
- `ProjectDraft` exposes candidate editorial and graph mutation, root selection, graph membership,
  and explicit timeline recompilation or replacement.
- `ProjectGraph::restore_timeline` deterministically regenerates trusted compilation provenance
  around a decoded editable graph only when the graph identity matches the same project and root.
- `StandaloneProjectGraph` owns one nonblank name and one ordinary
  `EditableGraph<CompiledTimelineGraphValue>`.
- `PortableRelativePath` canonicalizes one UTF-8 slash syntax and rejects host-specific characters,
  reserved device names, and ambiguous trailing forms. `ReferencedMediaPath` encodes
  `superi.media-path.v1` relative or platform-qualified absolute targets, accepts compatible raw
  paths, leaves URI-style locators opaque, and resolves relative paths only from an absolute owning
  project file path.
- `ProjectDocument::media_path` and `ProjectSnapshot::media_path` resolve one typed target by stable
  `MediaId`. `ProjectDocument::execute_media_command` applies `SetPath`, `MarkMissing`, or
  `ConsiderRelink` behind the document revision fence and returns the semantic result with the exact
  published snapshot. It preflights the media mutation before document publication, so accepted
  commands that do not change semantic state keep the existing editorial and document revisions.
- `ProjectDatabase::create` reserves a new path without overwriting an existing file, secures the
  connection, creates exact schema 1, and records the Superi application and schema identities.
- `ProjectDatabase::memory` creates the same secured schema without filesystem state.
- `ProjectDatabase::open` opens an existing database with write authority. Current schema 1 is
  validated without mutation, while exact supported schema 0 is upgraded transactionally to schema
  1 before the database is returned.
- `ProjectDatabase::open_read_only` opens existing state without write authority and validates
  database integrity, application identity, current schema revision, and exact schema objects.
  Supported legacy state is refused with a classified requirement for writable migration.
- `ProjectDatabase::source_schema_revision` reports the revision observed at open, and
  `ProjectDatabase::was_migrated` distinguishes a completed upgrade from current-schema open.
- `ProjectDatabase::replace` pre-encodes one immutable snapshot, bounds all content, writes every
  row in one immediate transaction, reloads the candidate through public component decoders,
  compares the exact snapshot, and commits only after equality.
- `ProjectDatabase::load` verifies the database, metadata, component lengths and SHA-256 values,
  project manifest, canonical component bytes, graph ownership, and revisions inside one read
  transaction before returning one checked document.
- `PROJECT_APPLICATION_ID`, `PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION`,
  `PROJECT_SCHEMA_REVISION`, `PROJECT_FORMAT`, and `PROJECT_FORMAT_VERSION` identify application
  `SUPR`, supported source schema `0`, current schema `1`, `superi.project`, and `1.0.0`.

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

Whole-snapshot restoration is the narrow command-history integration seam:

1. An engine owner supplies its exact current document revision and one previously retained
   immutable `ProjectSnapshot`.
2. Project rejects a stale fence or different `ProjectId` before publication, validates the complete
   target state, and preserves the current aggregate on every failure.
3. Equal target state is idempotent. A real restoration copies the target semantic contents but
   assigns current revision plus one, so undo and redo never make optimistic revision time move
   backward.
4. History capacity, branching, command metadata, and undo or redo selection remain entirely in
   `superi-engine`; the project layer owns only checked aggregate restoration.

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

Writable open adds one explicit compatibility path:

1. Connection-level application identity and `user_version` dispatch before mutation. Current
   schema 1 runs the existing exact validator only; a future schema, wrong application, or
   unrepresentable revision fails without a write transaction.
2. Exact schema 0 is the supported `superi.project` version `0.9.0` predecessor. Its three strict
   tables retain project, document, graph ownership, graph revision, and component-document meaning
   but predate schema 1's component lengths, component digests, and project manifest.
3. One immediate transaction repeats the identity and exact schema check, runs full SQLite and
   foreign-key integrity checks, bounds every row, and decodes declared timeline and graph component
   revisions 0 or 1 through their existing checked owners.
4. The migration reconstructs the complete document through `ProjectGraph::restore_timeline`,
   `StandaloneProjectGraph::new`, and `ProjectDocument::from_parts` before dropping any legacy
   table. It then serializes that snapshot through the current canonical serializers, creates the
   immutable schema-1 tables, writes current integrity evidence, reloads through the schema-1
   loader, and requires exact snapshot equality.
5. The migration registry contains the sole contiguous 0-to-1 step and ends at the current schema
   constant. A failure at any point, including after the schema rewrite but before commit, drops the
   borrowed transaction and restores the complete schema-0 database.

Referenced-media commands reuse the same aggregate publication boundary. A command captures each
retained timeline graph, changes only the timeline-owned media target or relink evidence through an
editorial draft, then rebuilds checked compilation provenance around the unchanged editable graph.
The project document revision and editorial revision advance atomically only for a semantic change,
while stable `MediaId`, direct graph edits, and canonical persistence meaning remain intact. A
missing or otherwise idempotent media update returns its typed result without manufacturing an
editorial or document revision. Portable relative targets are normalized once and resolve lexically
from the owning project file, without process current directory or filesystem-dependent
interpretation.

Connections enable SQLite defensive mode, foreign keys, cell-size checks, and a finite busy
timeout. They disable triggers, views, trusted schema, double-quoted string literals, and memory
mapping. Read-only connections also enable query-only mode. Schema 1 contains no trigger, view,
extension, network, process, device, or GPU behavior. Migration never opens a second connection and
never owns commit authority outside the one outer transaction.

`superi-engine::resources::MediaResourceRequest::from_project_media` is the real target consumer.
It resolves one stored project filesystem path, retains `MediaId` and expected fingerprint, rejects
explicit missing state, and feeds the ordinary media-I/O request. `acquire_project_resources` then
clones the exact selected compilation from `ProjectSnapshot`, including reloaded direct graph edits,
and acquires the exact reachable source and decoder set before one resources publication.

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
- Test-only `serde_json` builds exact legacy revision-0 component fixtures from current canonical
  payloads without entering the runtime dependency surface.
- `superi-engine` consumes immutable snapshots for transactional resource acquisition, adapts
  project-owned referenced-media paths into media-I/O source requests, and exclusively owns bounded
  command history over the checked whole-snapshot restore seam.
- API and CLI do not yet expose database or document commands. Later public commands must wrap this
  owner instead of creating another project or database authority.

## Invariants and operational boundaries

- The selected root exists and has exactly one retained timeline compilation at the current
  editorial revision. Every retained graph identity is unique and equals its ordered map key.
- Document edits, database replacement, and schema migration are all-or-nothing. Stale revisions,
  failed closures, invalid candidates, preflight bounds, SQL failures, failed reload, snapshot
  inequality, or precommit interruption publish nothing.
- Whole-snapshot restoration is also all-or-nothing. It requires matching project identity and the
  exact current revision, preserves the current aggregate on validation or revision exhaustion,
  returns equal state without a revision, and gives changed state one fresh monotonic revision.
- Schema identity is explicit at three levels: SQLite application ID, monotonic schema revision,
  and semantic format plus version. The stable primitive and component revisions are recorded too.
- Schema 1 has exactly three strict tables, one metadata singleton, one timeline singleton, and
  bounded graph rows. Extra user tables, indexes, views, or triggers are corruption.
- Supported schema 0 also has exactly three strict tables and retains every semantic field needed
  for lossless reconstruction. It may carry declared timeline or graph component revision 0 or 1,
  both of which are checked and canonicalized by their owning codec during migration.
- Project and graph revisions use canonical decimal text so every `u64` revision is preserved.
  Typed IDs use fixed 16-byte big-endian blobs.
- Canonical component bytes remain owned by timeline and graph. Project stores them with exact
  length and SHA-256 evidence and does not duplicate their semantic fields.
- Timeline remains the owner of media identity, opaque target text, and relink evidence. Project
  interprets only recognized filesystem target syntax, preserves unknown locators, and never derives
  identity from a path. Relative resolution is lexical and requires an absolute project file path;
  foreign-platform absolute targets and future target versions fail explicitly.
- The project manifest is private integrity evidence. It is not C014's public dirty-state hash.
- Wrong application identity, future schema or format versions, and read-only legacy open are
  unsupported. Malformed, noncanonical current state, tampered, missing, extra, or inconsistent
  stored state is corrupt data.
- Semantic row order, component bytes, and manifest digest are deterministic. SQLite page layout is
  not a public deterministic contract.
- `create` does not overwrite an existing path. Migration changes only the already opened source
  database after the complete legacy document is checked and only at commit. Atomic destination
  replacement, save-as, copy, backup, autosave, and recovery remain later concerns.

## Tests and verification

`document_contract.rs` contains seven public tests over coherent construction, immutable equal
snapshots shared across three roles, ordinary typed graph edits, stale and failed edit rollback,
no-op revision stability, editorial recompilation coherence, standalone graph state, checked parts
reconstruction, revision-fenced whole-snapshot restoration, monotonic undo-style publication,
identity rejection, and exhaustion atomicity.

`persistence_contract.rs` contains five real database tests. They prove nonoverwriting creation and
read-only reopen, exact schema objects and version identities, equal semantic rows across independent
databases, complete editorial, media fingerprint mismatch, multicam, direct graph edit, standalone,
and revision preservation, post-load conflict fencing and editing, preflight rollback, read-only
denial, and rejection of wrong application identity, future revisions, corrupt components, altered
manifest evidence, missing rows, and extra views.

`migration_contract.rs` contains three public database tests. They prove exact schema-0 and legacy
component compatibility, current canonical rewrite, source-revision reporting, complete snapshot
equality, post-migration edit and resave, read-only legacy refusal, byte-stable current open, future
schema nonmutation, and malformed legacy logical rollback. The private migration unit contract
forces a classified failure after the schema-1 rewrite and before commit, verifies exact schema-0
reconstruction after rollback, then runs the production migration successfully on the same state.

`media_reference_contract.rs` contains five public tests. They prove canonical portable relative
paths, versioned and compatible target decoding, deterministic project-relative resolution,
foreign-platform visibility, stable `MediaId` commands, revision conflicts, preserved direct graph
edits, explicit missing and fingerprint-mismatch state, exact database round trips, accepted relinks,
and safe rejection of opaque or future syntax.

Timeline's serialization contract independently round trips a real compiled multicam graph through
the public graph codec and rejects unknown `TimelineGraphValue` fields and tags. The engine resource
contract opens an exact schema-0 fixture through the public database owner and proves that the
migrated retained graph and real media stream reach resource acquisition unchanged.
Rust 1.80.1 check, strict Clippy, dependency direction, policy, formatting, map validation, and the
repository verifier remain required delivery gates.

## Current status and risks

The coherent in-memory document owner, schema-1 SQLite application format, deterministic component
records, integrity manifest, transactional replacement, exact schema-0 compatibility, ordered
forward migration, checked reconstruction, durable create and read-only reopen, writable current or
legacy open, versioned referenced-media paths, stable identity commands, and the real engine target
consumer are substantive and test-backed. Its checked whole-snapshot restore seam now supports the
engine-owned session command history without moving branching policy or retained entries into the
project crate.

Additional schema revisions, synchronized temporary save publication, atomic destination
replacement, save-as, copy, backup, settings, persisted history or command logs, autosave,
recovery, unknown extension
preservation, modified-since-open policy, dirty-state hashing, public API adaptation, CLI, and
scripting remain absent. Exact schema 0 is the only supported predecessor. Future, older unknown,
or extended layouts remain rejected until an explicit migration or preservation contract exists.

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
monotonic schema revision, semantic version decision, and one exact successor step appended to the
contiguous migration registry. Do not change schema 0 or schema 1 in place after release. Keep
filesystem publication, replacement, backups, autosave, and recovery behavior in their assigned
modules and checkpoints.

Refresh this map after any project source, manifest, public consumer, schema, or test change. Reread
every changed file and relevant component interface through EOF, reconcile prose before recomputing
the hash, and validate all maps after integration and immediately before delivery.
