---
module_id: superi-project
source_paths:
  - open/crates/superi-project
source_hash: 14a1cec3c4761a1320961c5358baa8556ef1c12d61ca354578e9daafd6b0410c
source_files: 28
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-project` owns the coherent whole-project aggregate, authoritative versioned project
settings, authored clip-mix state, durable opaque extension records, a bounded durable command log,
stable schema-5 SQLite
serialization, ordered forward migration from supported older project schemas, and read-only
project integrity validation with deterministic repair reporting. It also owns versioned semantic
project hashing and ordered component diagnostics over the same canonical prepared evidence. Its
single released-format table binds application identity, format identity, stable primitive revision,
and every schema 0 through 5 semantic version pair to deterministic compatibility outcomes and
forward migration paths. One
`ProjectDocument` combines
the validated
editorial project, selected root timeline, retained compiled timeline graphs, optional named
standalone editable graphs, complete timeline, color, audio, cache, proxy, render, and optional
working-folder settings,
authored clip-mix state, bounded plugin, effect, AI artifact metadata, future namespaced extension
records, one optimistic authored document revision, and one independent monotonic command-log
sequence. Immutable `ProjectSnapshot` values
give editor, script, headless, persistence, API, and engine consumers one equal published state.

`ProjectDatabase` is the only public whole-project database and file-publication authority. It
creates a new nonoverwriting `.superi` database or secured in-memory database, opens exact current
state read-only, opens current or supported legacy state with write authority, and reconstructs one
complete `ProjectDocument`. One typed command surface atomically saves the active file, publishes
save-as and copy destinations under explicit collision policy, and creates no-clobber backups. A
file-backed database retains its absolute active path and one validated content generation between
operations, while an in-memory database retains its secured connection until save-as establishes
active file identity and generation. Writable
open reports its source schema and migrates exact schema 0 through 4 to schema 5 through contiguous
immediate transactions. The schema persists canonical timeline, graph, settings, and audio
component documents beside one strict extension-record table instead of copying domain models into
competing SQL fields. Frozen schema 2 adds canonical settings, frozen schema 3 adds canonical
authored audio, schema 4 adds integrity-protected canonical extension metadata plus exact opaque
payload bytes, and current schema 5 adds typed command metadata and bounded retained request bytes
under the same manifest and transactional replacement boundary.

The project media boundary interprets timeline-owned opaque targets as versioned filesystem
references when their syntax is known. It owns portable relative path validation, project-file
resolution, explicit host-absolute platform evidence, stable `MediaId` queries, and atomic path,
missing, and fingerprint-checked relink commands without creating a second media state model. The
document also exposes one checked whole-snapshot restore seam for the engine-owned command-history
policy. Restoration preserves project identity and active command-log lineage, validates the
complete authored aggregate, and publishes a fresh monotonic document revision instead of reviving
an old revision number or importing a recovery candidate's command records.

The project autosave boundary owns one clockless and threadless controller per `ProjectId`. A host
supplies monotonic elapsed time and immutable selected snapshots through one typed command surface.
The controller publishes complete current-schema recovery points through the existing atomic
no-clobber `Backup` authority, assigns strictly parsed numeric generations, retains a bounded
user-selected count, and prunes only regular files in its project-owned namespace. Policy and
schedule state remain session-local, while every completed artifact is an ordinary independently
openable `.superi` database containing the complete editable project meaning.

The project recovery boundary consumes that exact autosave namespace. It discovers and fully loads
published generations, retains classified raw diagnostics for unusable entries, compares typed
project meaning without hashes, revalidates exact regular-file identity before every action, and
durably dismisses one opaque candidate through a synchronized tombstone transition. Candidate paths
and source-chain details remain private to this crate, and interrupted dismissal cleanup is safely
resumed during later discovery.

The project integrity boundary exposes one role-neutral read-only command for current and supported
legacy state. It reports bounded deterministic status, stage, finding, evidence, verified identity,
and repair disposition values after complete SQLite, foreign-key, application, schema, component,
manifest, semantic reconstruction, aggregate, and source-stability checks. It performs no repair,
migration, file creation, salvage, or authority change. A restore recommendation points callers to
the independently validated recovery path rather than opening another mutation surface.

The project diagnostics boundary computes one domain-separated SHA-256 content identity from
canonical timeline, settings, clip-mix, extension, and retained graph evidence. It includes stable
project, root, component codec, primitive schema, extension identity and payload-schema, and graph
scope meaning. It deliberately excludes the outer `ProjectDocument` revision, database schema and
manifest, file path, active save identity, autosave generation, and SQLite page layout. The report
retains the observed outer revision only for correlation and exposes immutable components in fixed
family order followed by stable extension and graph identity order.

This module does not own command-history branching or selection policy. It owns only the durable
bounded record store and its persistence, not engine restoration transactions, runtime readiness,
plugin process state, or API and CLI adaptation. Engine retains execution history and dispatch,
while the API-owned local
host and CLI now compose this crate's existing database and file commands without creating another
project authority.

## Source inventory

- `open/crates/superi-project/Cargo.toml`: Declares audio, core, graph, timeline, exact workspace
  `rusqlite`, synchronization-only `fs4`, workspace Serde, SHA-256, and JSON component dependencies.
- `open/crates/superi-project/src/autosave.rs`: Implements validated runtime policy, host-driven
  monotonic scheduling, typed configure, tick, save-now, prune, and inspect commands, deterministic
  project and artifact naming, atomic Backup publication, bounded collision retry, strict managed
  namespace inspection, count-based retention, safe regular-file pruning, and classified failure
  or postpublication cleanup evidence.
- `open/crates/superi-project/src/command_log.rs`: Owns validated request evidence, monotonic typed
  command records, exact request SHA-256, retained or digest-only payload disposition, 4,096-record
  and 64-MiB oldest-first retention, cursor metadata, strict decoding, and deterministic equality.
- `open/crates/superi-project/src/compatibility.rs`: Owns the authoritative schema 0 through 5
  project format release table, complete observed format identity, current support descriptor,
  typed compatibility dispositions and reasons, and deterministic forward migration paths.
- `open/crates/superi-project/src/document.rs`: Implements `ProjectDocument`, immutable snapshots,
  private edit candidates, authoritative settings, retained timeline compilations, named standalone
  graphs, authored clip-mix state, ordered extension records, project-owned command-log snapshots,
  independent append without authored revision, revision fencing, checked reconstruction and
  restoration, fresh monotonic restore publication, and complete relationship validation.
- `open/crates/superi-project/src/diagnostics.rs`: Implements versioned semantic project hashing,
  immutable SHA-256 digest and component-evidence values, stable graph scope and component
  vocabularies, canonical ordering, explicit hash framing, and snapshot diagnostics built from the
  persistence preparation path without depending on database bytes or outer document revision.
- `open/crates/superi-project/src/extensions.rs`: Owns bounded compound extension identities, open
  namespaced plugin, effect, AI artifact metadata, and future kinds, opaque payload envelopes,
  requested and granted capabilities, user-controlled lifecycle, structured failure evidence, and
  one typed revision-fenced upsert, remove, lifecycle, grant, failure, and clear command surface for
  documents and caller-owned drafts.
- `open/crates/superi-project/src/integrity.rs`: Implements the role-neutral read-only integrity
  command, bounded deterministic report vocabulary, complete SQLite and foreign-key evidence,
  application and schema identity checks, registered current and legacy reconstruction, component
  finding classification, verified identity, source-stability fencing, and safe repair dispositions.
- `open/crates/superi-project/src/lib.rs`: Documents the implemented aggregate, schema-5
  persistence, migration, settings, extension state, command log, semantic diagnostics, atomic save,
  referenced-media, and integrity
  boundaries, exports public project modules, keeps migration and save mechanics private, and
  re-exports the database, save, autosave, integrity, diagnostics, command-log, and stable format surfaces.
- `open/crates/superi-project/src/migrate.rs`: Owns exact schema-0, schema-1, and frozen schema-2
  and schema-3 contracts, frozen schema-4 persistence, the contiguous 0-to-1-to-2-to-3-to-4-to-5
  migration registry, secured
  compatibility decoding, root-rate-derived settings defaults, canonical empty-audio and empty
  extension and empty command-log migrations, checked aggregate reconstruction, registered read-only revision inspection,
  transactional canonical rewrites, shared complete integrity checks, and precommit rollback proof.
- `open/crates/superi-project/src/media.rs`: Implements versioned referenced-media target encoding,
  portable relative path normalization, deterministic project-file resolution, host-platform
  evidence, stable media lookup, bounded atomic fingerprinted-media insertion, and reusable draft
  plus revision-fenced document commands that retain editable timeline graphs and suppress false
  document revisions for exact duplicate imports and other semantic no-ops.
- `open/crates/superi-project/src/persist.rs`: Implements secured short-lived file connections and
  retained in-memory storage, active path identity and validated file generations, stable
  before-and-after generation observation, schema 5 plus frozen schema-1 through schema-4
  migration helpers, one typed per-schema component layout, deterministic timeline, graph, settings, audio, and extension component
  records and manifest evidence, canonical strict extension metadata, separately hashed opaque
  payload bytes, typed command-log metadata and replay bytes, checked in-memory replacement, strict
  interpretation, bounded decoding, complete
  shared SQLite and foreign-key collection, and checked aggregate reconstruction.
- `open/crates/superi-project/src/recovery.rs`: Implements opaque autosave generation identities,
  deterministic restart discovery, complete current-schema database loading, semantic comparison
  across editorial, settings, authored clip-mix, extension, root, and graph state, internal raw
  failure diagnostics with stable next actions, per-action file identity revalidation, durable
  exact tombstone dismissal, degraded cleanup evidence, and restart tombstone cleanup.
- `open/crates/superi-project/src/save.rs`: Implements the typed save, save-as, copy, and backup
  commands, explicit collision policy, stable sibling writer-lock ownership for replacement saves,
  active and destination generation conflict checks, complete same-parent SQLite candidates,
  semantic and integrity validation, handle closure and platform-correct file synchronization,
  atomic replacement or no-clobber publication, active-path rebinding, classified publication
  state, and owned-candidate cleanup.
- `open/crates/superi-project/src/settings.rs`: Owns schema `1.0.0`, the exact timeline, color,
  audio, cache, proxy, render, and optional working-folder key vocabulary, deterministic
  root-derived defaults, bounded path validation, strict cross-field validation, and bounded
  ordered set or remove transactions.
- `open/crates/superi-project/tests/document_contract.rs`: Proves coherent construction, immutable
  concurrent snapshots, ordinary graph editing, atomic failure behavior, compilation freshness,
  standalone graph identity, checked reconstruction, revision-fenced whole-snapshot restoration,
  monotonic restore publication, exhaustion atomicity, and graph identity checks.
- `open/crates/superi-project/tests/command_log_contract.rs`: Proves exact retained request and
  encoded record round trips, digest-only oversized requests, strict evidence, independent authored
  revision, monotonic sequence identity, and deterministic count and byte retention.
- `open/crates/superi-project/tests/diagnostics_contract.rs`: Proves version constants, equivalent
  construction-order identity, media ID and fingerprint sensitivity, rejected relink evidence,
  exact first changed component families, outer-revision exclusion, monotonic restore equality,
  and identical semantics after reload from byte-different SQLite page layouts and paths.
- `open/crates/superi-project/tests/integrity_contract.rs`: Proves one deterministic command surface
  across editor, script, and headless roles, valid current and supported legacy reconstruction,
  migration reporting, complete component corruption classification, stable bounded evidence,
  source failure handling, byte nonmutation, and continued authority-project use after inspection.
- `open/crates/superi-project/tests/migration_contract.rs`: Proves public supported legacy open,
  legacy timeline and graph component migration, exact editable-state preservation, canonical
  current reopen, continued editing and replacement, save, save-as, copy, and backup after migration,
  source-revision preservation, current byte stability, read-only legacy refusal, future
  nonmutation, and malformed legacy rollback.
- `open/crates/superi-project/tests/media_reference_contract.rs`: Proves portable path grammar,
  versioned target round trips, relative and host-absolute resolution, stable identity commands,
  retained direct graph edits, relink conflicts, database round trips, and unknown target handling.
- `open/crates/superi-project/tests/persistence_contract.rs`: Proves durable create and read-only
  reopen, exact schema identity, deterministic semantic rows, complete timeline, media, relink,
  graph, authored audio, known and unknown extension state, exact opaque payload, command-log
  evidence, and revision preservation, rollback, read-only enforcement, and corruption rejection.
- `open/crates/superi-project/tests/save_contract.rs`: Proves the public save surface through real
  file-backed and in-memory projects, exact active-path changes, collision policy, read-only
  publications, stale-authority detection with explicit save-as and copy escape paths, missing and
  malformed active-file protection, alias and invalid-destination rejection, bounds before
  mutation, current-schema integrity, non-UTF-8 destinations, permissions, and later save behavior
  after copy or save-as.
- `open/crates/superi-project/tests/project_settings_contract.rs`: Proves default and configured
  settings, strict atomic revision-fenced transactions, no-op behavior, complete domain validation,
  schema-5 durability, manifest coverage, and migration defaults.
- `open/crates/superi-project/tests/autosave_contract.rs`: Proves deterministic scheduling, enable
  and disable control, manual publication, unchanged suppression, missing-artifact republication,
  large forward jumps, monotonic-time rejection, exact current-schema snapshot recovery points,
  unchanged active-project bytes, strict generation ownership, count retention independent of
  filesystem timestamps, explicit pruning, foreign and save-candidate preservation, symlink
  rejection before deletion, policy and deadline bounds, no-clobber generation selection,
  generation exhaustion, state-preserving failure, and same-time retry.
- `open/crates/superi-project/tests/recovery_contract.rs`: Proves exact C009 namespace discovery,
  typed comparison including authored clip-mix and extension state, degraded corrupt-entry
  coexistence, wrong-project and symlink classification,
  retryable stale-file identity, exact durable dismissal, foreign-file preservation, restart
  rediscovery, and tombstone cleanup.
- `open/crates/superi-project/tests/extension_state_contract.rs`: Proves role-neutral plugin,
  auxiliary effect, AI artifact provenance, and unknown-kind records, exact opaque bytes, bounded
  construction, capability narrowing, lifecycle and quarantine control, structured failure clear,
  stale fencing, semantic no-op behavior, removal, and immutable snapshot access through one
  command surface.
- `open/crates/superi-project/tests/version_negotiation_contract.rs`: Proves the exact authoritative
  release table and typed current, migration-required, future-application, unsupported, and invalid
  outcomes, including the complete registered schema successor path.

## Public surface

The crate root exports `autosave`, `command_log`, `compatibility`, `diagnostics`, `document`, `extensions`, `integrity`, `media`, `persist`,
`recovery`, and `settings`, keeps save mechanics private, and re-exports the stable persistence,
save, autosave, integrity, diagnostics, and recovery authorities, project format and semantic hash
constants, and the media path target format identifier.

- `project_format_support` returns the one authoritative application, text format, primitive
  revision, and released schema-version table. `ProjectFormatIdentity` carries one complete
  observation, and `negotiate_project_format` returns a typed `ProjectVersionDisposition`, ordered
  reasons, current target, and exact successor migration path without opening or mutating a file.

- `ProjectDocument::new` accepts one `EditorialProject` and selected `TimelineId`, compiles that
  root through `superi_timeline::compile_timeline`, derives deterministic settings from its edit
  rate, validates the aggregate, and starts document revision zero.
- `ProjectDocument::from_parts` restores an explicit document revision and complete graph
  collection with deterministic settings and empty clip-mix compatibility defaults after an outer
  decoder has validated its format. `from_parts_with_settings` restores explicit settings with
  empty audio, `from_complete_parts` restores explicit audio with deterministic settings, and
  `from_complete_parts_with_settings` restores both. The schema-4
  `from_complete_parts_with_settings_and_extensions` constructor also restores exact extension
  records, while the persistence-only complete constructor restores the independently validated
  command log. All reject duplicate identities and validate every relationship.
- `ProjectDocument::snapshot` returns a cloneable immutable `ProjectSnapshot`. Document and
  snapshot accessors expose project identity, revision, selected root, editorial state, settings,
  authored clip-mix state, stable graph and extension-record iteration and lookup, command-log
  sequence and retained records, and timeline compilation lookup.
- `ProjectCommandRecordDraft` captures the exact compact request length and SHA-256 before dispatch,
  retaining exact bytes only within the one-MiB replay bound. `ProjectDocument::append_command_record`
  appends one successful command without changing authored revision or authored semantic state.
- `ProjectDocument::edit` requires the exact current revision, changes one private candidate,
  validates the complete result, and publishes once only when semantic state changed.
- `ProjectDocument::restore_snapshot` requires the exact current revision and matching project
  identity, validates the complete target aggregate, returns the existing snapshot for equal state,
  and otherwise publishes the target contents at one fresh monotonic document revision.
- `ProjectDocument::execute_settings_transaction` applies one bounded ordered settings transaction
  through that same whole-project edit boundary. Document, snapshot, and draft accessors expose the
  authoritative immutable settings; a semantic no-op preserves the revision.
- `ProjectDraft` exposes candidate editorial, settings, clip-mix, graph, and extension mutation,
  paired mutable editorial and audio access for atomic identity reconciliation, root selection,
  graph membership, and explicit timeline recompilation or replacement.
- `ProjectExtensionRecord` retains one compound extension and record identity, extension version,
  open namespaced kind, payload schema, requested and user-granted `CapabilitySet`, lifecycle,
  optional structured failure, and exact opaque bytes. Construction enforces payload, identity,
  capability, context, message, and counter bounds and requires grants to remain a subset of
  requests and quarantined state to retain failure evidence.
- `ProjectExtensionCommand` is the one revisioned public control surface for upsert, remove,
  lifecycle changes, grant changes, failure recording, and failure clearing.
  `ProjectExtensionOperation`, `ProjectExtensionCommandResult`, and
  `ProjectExtensionCommandOutcome` expose stable operation codes and typed complete outcomes.
  Document execution uses the aggregate revision fence; draft execution composes the same behavior
  inside a compound caller-owned edit. Equal commands are successful no-ops.
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
  `MediaId`. `ProjectDraft::execute_media_command` applies `SetPath`, `MarkMissing`, or
  `ConsiderRelink` inside a caller-owned aggregate edit. `ProjectDocument::execute_media_command`
  wraps the same operation behind the document revision fence. Both return the semantic result, and
  the document wrapper also returns the exact published snapshot. Accepted commands that do not
  change semantic state keep the existing editorial and document revisions.
- `ProjectDatabase::create` reserves a new path without overwriting an existing file, secures the
  connection, creates exact schema 5, and records the Superi application and schema identities.
- `ProjectDatabase::memory` creates the same secured schema without filesystem state.
- `ProjectDatabase::open` opens an existing database with write authority. Current schema 5 is
  validated without mutation, while exact supported schema 0 through schema 4 is
  upgraded
  transactionally through the contiguous chain before the database is returned.
- `ProjectDatabase::open_read_only` opens existing state without write authority and validates
  database integrity, application identity, current schema revision, and exact schema objects.
  Supported legacy state is refused with a classified requirement for writable migration.
- `ProjectDatabase::source_schema_revision` reports the revision observed at open, and
  `ProjectDatabase::was_migrated` distinguishes a completed upgrade from current-schema open.
- `ProjectDatabase::active_path` reports the absolute file identity used for project-relative state,
  or none for an in-memory database before save-as.
- `ProjectDestinationCollision` requires an absent name or explicitly permits replacement of an
  existing validated Superi project. `ProjectSaveCommand` supplies `Save`, `SaveAs`, `SaveCopy`, and
  `Backup` through one role-neutral command surface. `ProjectSaveOperation` and
  `ProjectSaveOutcome` report the completed operation, absolute destination, resulting active path,
  and whether an existing destination was replaced.
- `ProjectDatabase::execute_save_command` pre-encodes and bounds one immutable snapshot before
  filesystem mutation, builds and reloads one complete current-schema candidate, closes and
  synchronizes it, then atomically publishes it. Save-as rebinds active identity at the publication
  commit point; copy and backup retain the prior active identity.
- `ProjectAutosavePolicy` validates enabled state, a nonzero interval, one explicit existing
  canonical recovery root, and retention from one through
  `MAX_PROJECT_AUTOSAVE_RETENTION` recovery points. It contains runtime policy only and never enters
  project schema or authored meaning.
- `ProjectAutosaveCommand` supplies `Configure`, `Tick`, `SaveNow`, `Prune`, and `Inspect` through
  one public controller entrypoint. `ProjectAutosaveOperation` and
  `ProjectAutosaveDisposition` distinguish typed work and successful no-work outcomes without
  exposing a direct publisher or deletion bypass.
- `ProjectAutosaveController` binds permanently to one `ProjectId`, accepts caller-supplied
  monotonic `Duration` values, validates immutable snapshot identity, publishes through
  `ProjectSaveCommand::Backup`, schedules at most one recovery point per observed tick, and retains
  no thread, clock, or global process state.
- `ProjectAutosaveArtifact`, `ProjectAutosaveState`, and `ProjectAutosaveOutcome` expose completed
  generation, absolute path, project revision, active policy, accepted elapsed time, next deadline,
  latest successful publication, managed count, and per-command prune progress. A completed managed
  name is `autosave-g<20 zero-padded decimal generation>.superi` inside
  `project-<32 lowercase raw ProjectId hex>`.
- `ProjectRecoveryController` binds one `ProjectId` to the same canonical recovery root and
  publishes a monotonic `ProjectRecoveryCatalog` containing valid candidates and classified
  findings. Discovery ignores foreign names and save sidecars, fully opens exact managed regular
  files, and cleans recognized dismissal tombstones without following symlinks.
- `ProjectRecoveryCandidateId` is an opaque stable generation identity. Compare, restore loading,
  and dismissal accept only that identity plus an exact catalog revision and revalidate the
  cataloged file identity before use.
- `ProjectRecoveryComparison` reports current and candidate revisions plus editorial, settings,
  authored clip-mix, root timeline, and graph changes using typed equality. `ProjectRecoveryFinding`
  retains a complete
  internal `FailureDiagnostic` and a stable `ProjectRecoveryNextAction` for retryable, degraded,
  user-correctable, and terminal conditions.
- `ProjectRecoveryController::dismiss` renames one exact managed entry to a recognized same-directory
  tombstone and synchronizes the directory before the dismissal becomes authoritative. Cleanup
  trouble remains a degraded finding, and future discovery completes recognized cleanup.
- `ProjectDatabase::replace` delegates file-backed state to `Save` and keeps the existing checked
  immediate transaction for in-memory state. Both paths pre-encode one immutable snapshot, bound
  all content, write every row including settings, audio, and extensions, reload through public
  component decoders, and require exact snapshot equality before commit or publication.
- `ProjectDatabase::load` verifies the database, metadata, component lengths and SHA-256 values,
  project manifest, canonical timeline, settings, audio, graph, and extension metadata and payload
  bytes, graph and extension ownership, and revisions inside one read transaction before returning
  one checked document.
- `ProjectIntegrityCommand::Validate` accepts one project path through the public
  `execute_project_integrity_command` entrypoint. `ProjectIntegrityReport` returns one stable
  `ProjectIntegrityStatus`, overall `ProjectRepairDisposition`, observed and current schema
  revisions, bounded ordered `ProjectIntegrityFinding` values, completion evidence, and verified
  `ProjectIntegrityIdentity` with media, graph, and extension counts only after complete semantic
  reconstruction.
- `ProjectIntegrityStage` and `ProjectIntegrityFindingCode` provide permanent semantic codes for
  open, SQLite structure, foreign keys, application identity, schema, component evidence, semantic
  reconstruction, aggregate validation, and source stability. Evidence values are UTF-8 bounded,
  finding count is capped by `MAX_PROJECT_INTEGRITY_FINDINGS`, and overflow retains an explicit
  truncation finding instead of claiming complete interpretation.
- `ProjectRepairDisposition` distinguishes no action, checked forward migration, validated recovery,
  inspection retry, access correction, newer application use, and manual recovery. It reports a safe
  next action only and grants no repair or publication authority.
- `ProjectDigest` is one immutable 32-byte SHA-256 value with exact byte access and canonical
  lowercase hexadecimal display. `ProjectComponentEvidence` pairs a canonical encoded byte length
  with one such digest.
- `ProjectGraphScope` distinguishes one timeline-root compilation from one named standalone graph.
  `ProjectDiagnosticComponent` exposes permanent family codes and typed timeline, settings,
  clip-mix, extension metadata plus payload, and graph evidence. Extension records retain typed
  compound identity and payload schema; graph records retain identity, scope, graph revision, codec
  revision, and canonical evidence.
- `ProjectDiagnostics::from_snapshot` invokes the same bounded canonical preparation used by
  persistence, then publishes project and root identity, observed outer revision, stable primitive
  revision, hash algorithm, hash format revision, aggregate content hash, and immutable ordered
  components. `PROJECT_HASH_ALGORITHM` is `sha256` and `PROJECT_HASH_FORMAT_REVISION` is `1`.
- `PROJECT_APPLICATION_ID`, `PROJECT_OLDEST_SUPPORTED_SCHEMA_REVISION`,
  `PROJECT_SCHEMA_REVISION`, `PROJECT_FORMAT`, and `PROJECT_FORMAT_VERSION` identify application
  `SUPR`, supported source schema `0`, current schema `4`, `superi.project`, and `1.3.0`.
- `ProjectSettings`, `ProjectSettingsTransaction`, and `ProjectSettingMutation` expose exact schema
  `1.0.0`, canonical key iteration and lookup, strict checked construction, and bounded set or
  remove operations. Permanent keys cover timeline edit rate and timecode, color identity, audio
  sample rate and ordered channel layout, cache limits, proxy policy, and render rate, extent,
  pixel, alpha, and color intent.

## Architecture and data flow

In-memory publication remains one aggregate transaction:

1. A caller captures the current document revision or immutable snapshot.
2. `ProjectDocument::edit` rejects stale input before cloning one unpublished candidate.
3. The closure changes editorial state, settings, authored clip-mix state, retained graph state,
   extension records, root selection, or graph membership through `ProjectDraft`.
4. Validation checks the selected root, current compilation revision and project identity, map and
   graph identities, unique compiled roots, standalone names, clip-mix controls, extension identity,
   count and envelope bounds, and that every mixed clip still exists in the editorial project.
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

Prepared schema-5 serialization preserves that exact published state:

1. Timeline serializes the complete editorial project into canonical `superi.timeline` bytes.
2. Graph serializes every retained graph snapshot in stable `GraphId` order into canonical
   `superi.graph` bytes. Timeline and standalone ownership, root or name, graph revision, byte
   length, and SHA-256 remain explicit row evidence.
3. Project serializes its validated shared settings snapshot into canonical JSON, bounded to one
   MiB, with an explicit settings format revision, byte length, and SHA-256 evidence.
4. Audio serializes authored `ClipMixState` into canonical `superi.clip-mix` bytes with exact float
   meaning. The singleton audio row records its format revision, length, and SHA-256 digest.
5. Every extension record becomes one stable ordered row. Canonical strict JSON retains identity,
   extension version, kind, payload schema, requested and granted capabilities, lifecycle, and
   optional structured failure under metadata revision 1. Metadata and exact opaque payload bytes
   have independent lengths and SHA-256 values, so an unknown future kind and non-UTF-8 payload can
   survive load, edit, history, save, and recovery without interpretation.
6. Command-log metadata stores the next sequence, and each retained row stores typed correlation,
   request length and digest, optional bounded replay bytes, before and after authored revisions and
   semantic hashes, and authored-change disposition. Record ordering and retained byte totals are
   reconstructed and validated before publication.
7. A domain-separated manifest covers project and component format revisions, primitive revision,
   project identity, document revision, selected root, timeline evidence, settings evidence, ordered
   graph evidence, audio evidence, and ordered extension identity, metadata, and payload evidence.
8. In-memory replacement writes metadata, singleton timeline, settings, and audio components,
   graph rows, extension rows, and command-log rows in one immediate transaction. A preflight or transaction failure
   preserves the prior complete database.
9. Before an in-memory commit or file candidate publication, the candidate passes exact application
   and schema checks, exact schema-object inspection, row and size bounds, component and manifest
   integrity, canonical component and extension metadata decoding, exact payload validation,
   checked aggregate reconstruction, and exact `ProjectSnapshot` equality. File candidates also
   pass a full SQLite integrity check after commit.

Semantic diagnostics reuse preparation without reusing the private manifest identity:

1. `ProjectDiagnostics::from_snapshot` calls `PreparedProject::from_snapshot`, so timeline,
   settings, clip-mix, extension metadata, opaque payload, and graph bytes and digests come from the
   exact canonical encoders and bounds used by persistence.
2. The public component list always begins with timeline, settings, and clip mix. Extensions follow
   in compound key order and graphs follow in `GraphId` order, preserving typed extension payload
   schema and timeline or named standalone graph scope beside byte evidence.
3. A revision-1 domain-separated framing length-prefixes the SHA-256 algorithm code, hash format
   revision, stable primitive revision, project and selected-root identities, component count, each
   family tag, every relevant codec revision and semantic identity, exact byte length, and canonical
   digest.
4. The outer document revision is copied into the report for optimistic-command correlation but is
   never framed into the content hash. Private manifest, SQLite schema, file path, active database
   identity, save destination, autosave generation, and page layout are also excluded.
5. A content mutation therefore changes the aggregate hash and exposes the exact changed component
   family, while undo or recovery of equal authored meaning restores the prior hash even though the
   new outer revision remains monotonic.

File publication adds one explicit commit boundary:

1. The command resolves an absolute destination through its canonical parent, rejects canonical
   active-path aliases for copy and backup plus Unix hard-link aliases, and applies its explicit
   collision rule. Replace-existing accepts only a regular file that opens and loads as a valid
   Superi project.
2. The complete prepared snapshot enters a uniquely and exclusively reserved same-parent candidate.
   SQLite uses single-file rollback-journal mode and extra synchronization, then writes schema 5 in
   one immediate transaction and proves exact semantic equality before and after candidate commit.
3. The SQLite connection closes before the candidate file is synchronized through a handle with the
   access required by its platform, including write access on Windows. Sidecar absence is required,
   and Unix parent directories are synchronized.
4. Replace-existing opens a deterministic hidden sibling lock, rejects symlink, directory, or
   identity-substituted lock entries, and acquires one nonblocking exclusive operating-system lock.
   A held lock is a retryable conflict; unsupported locking fails closed as user-correctable.
5. While that writer lock remains held, the command rechecks the destination generation captured at
   command start and, for the active path, the content generation captured by create or open. A
   changed destination is retryable; a stale, missing, replaced, corrupt, or nonregular active file
   is a user-correctable conflict and is never recreated or overwritten.
6. Replace-existing uses same-parent atomic rename. Require-absent uses an atomic hard-link claim and
   removes the private candidate only after the destination name is owned. A race becomes a
   classified conflict instead of clobbering the competing entry.
7. Save and save-as install the freshly observed active generation at the publication commit point;
   save-as also rebinds active identity before any fallible finalization. Prepublication failures
   preserve the prior destination and active identity; postpublication failures report
   `publication_state=published`, unconfirmed durability, exact destination and candidate context,
   and a recovery action without claiming rollback.

Autosave composes that publication authority without another persistence model:

1. Configuration validates caller policy, rejects backward elapsed time, anchors one checked next
   deadline, creates only the bound project's direct child under an existing canonical root, and
   applies a lower retention limit before committing controller state.
2. A periodic tick returns disabled or not due without publication. At a due boundary it suppresses
   a duplicate only when the last successful periodic revision still has its exact managed file,
   advances a new deadline from the observed time, and never performs catch-up bursts.
3. `SaveNow` always publishes, including while periodic scheduling is disabled or the immutable
   revision is unchanged. Every successful publish chooses one checked generation and delegates the
   complete snapshot to no-clobber `Backup`; a destination collision causes a bounded rescan and
   retry, never replacement.
4. Scanning recognizes only the exact project directory and strict 20-digit artifact syntax,
   revalidates the directory without following a final symlink, inspects every managed name with
   `symlink_metadata`, sorts by parsed generation, and ignores all foreign, malformed, and
   `.superi-save-candidate-` entries.
5. Automatic and explicit pruning preflight every managed entry, retain the newest configured
   count, remove older regular files individually, never recurse, and synchronize a completed
   deletion batch where supported. A cleanup error after publication preserves the new artifact and
   reports its exact generation, path, project revision, and partial prune progress.
6. Policy, elapsed anchors, and last-publication state live only in the controller. A restart reads
   no timer journal, while each completed artifact remains a complete current-schema database for
   recovery discovery and validation.

Recovery consumes the same durable namespace without another persistence model:

1. Construction canonicalizes one existing recovery root and derives the exact C009 project child.
   Discovery accepts only strict published and recognized tombstone names, rejects a symlinked
   project directory, and sorts candidates and findings by numeric generation.
2. Every published regular file opens through `ProjectDatabase::open_read_only` and `load`. A valid
   matching project becomes a candidate; corruption, another project identity, unsafe file type,
   or classified storage failure becomes an internal finding while other candidates remain usable.
3. Compare requires an exact catalog revision and current project identity, revalidates file
   metadata identity, reloads the candidate, and compares editorial state, settings, selected root,
   authored clip-mix state, and every retained graph using typed semantic equality.
4. Restore selection returns only a fully revalidated `ProjectSnapshot`; the engine remains the sole
   authority for monotonic document restoration, persistent active-project replacement, and history
   publication.
5. Dismissal revalidates the exact candidate, reserves one generation tombstone, atomically renames
   and synchronizes it, then removes only that tombstone. A cleanup failure cannot resurrect the
   candidate and is retained as degraded evidence for later discovery cleanup.

Load follows the same path without partial publication. A file-backed load first requires the
current active generation to match the generation captured by create, open, save, or save-as. One
deferred read transaction then pins a coherent SQLite snapshot across every identity, schema,
metadata, component, and manifest query, and a second generation check after handle closure rejects
a collaborator change during the read.
Timeline bytes reconstruct the validated `EditorialProject`, and settings bytes reconstruct the
strict project schema without coercion. Each timeline graph uses
`ProjectGraph::restore_timeline`, each standalone graph uses `StandaloneProjectGraph::new`, and
`ProjectDocument::from_complete_parts_with_settings_and_extensions` joins the complete settings,
decoded audio intent, and exact extension records at the stored revision. Direct graph edits
are never replaced by recompilation; compilation supplies only trusted provenance around the
decoded graph.

Integrity inspection composes the same checked owners without a write path:

1. The command first reads and verifies the SQLite file header, then opens the existing path through
   the hardened read-only connection configuration and starts one deferred snapshot transaction.
2. One shared collector consumes complete bounded `PRAGMA integrity_check` and
   `PRAGMA foreign_key_check` results. Ordinary open, save validation, migration validation, and the
   integrity command all use the same collector instead of one-row or quick-check shortcuts.
3. Application identity and schema revision dispatch select exactly one registered schema 0, 1, 2,
   3, 4, or 5 reader. Each path performs its exact schema-object, row, component, digest, manifest,
   codec, relationship, and aggregate checks without migration.
4. Only complete reconstruction returns verified project identity, document revision, selected root,
   media count, graph count, and extension count. Supported legacy state reports migration required
   while preserving identity; future or wrong-application state remains unsupported.
5. Findings are classified into deterministic stages and codes, sorted canonically, and bounded with
   explicit truncation. Missing, inaccessible, busy, changed, or resource-exhausted sources remain
   indeterminate instead of being misreported as durable corruption.
6. A same-connection `data_version` comparison rejects an inspection whose visible source changed.
   Checked close completes the read, and no branch creates, writes, migrates, salvages, or recomputes
   authority evidence.

Writable open applies one contiguous compatibility path:

1. Connection-level application identity and `user_version` dispatch before mutation. Persistence,
   migration registry validation, and integrity interpretation share the authoritative released
   format table instead of maintaining independent schema-to-semantic-version matches. Current
   schema 5 runs the existing exact validator only; a future schema, wrong application, or
   unrepresentable revision fails without a write transaction.
2. Exact schema 0 is the supported `superi.project` version `0.9.0` predecessor. Its three strict
   tables retain project, document, graph ownership, graph revision, and component-document meaning
   but predate schema 1's component lengths, component digests, and project manifest.
3. One immediate transaction repeats the identity and exact schema check, runs full SQLite and
   foreign-key integrity checks, bounds every row, and decodes declared timeline and graph component
   revisions 0 or 1 through their existing checked owners.
4. The 0-to-1 step reconstructs the complete legacy document through existing checked owners before
   dropping any legacy table, writes the immutable schema-1 representation, reloads it, and requires
   exact snapshot equality.
5. The 1-to-2 step loads schema-1 meaning, derives deterministic project settings from the selected
   root timeline rate, and rewrites the complete snapshot through frozen schema 2.
6. The 2-to-3 step loads frozen schema-2 meaning, adds canonical empty clip-mix state, and writes
   the frozen schema-3 representation.
7. The 3-to-4 step loads and verifies frozen schema-3 meaning, adds an empty extension-record set,
   and writes the frozen schema-4 representation.
8. The 4-to-5 step loads and verifies frozen schema-4 meaning, adds an empty command log, and writes
   schema 5 through the current serializer. Direct legacy open enters at its matching step, while
   schema 0 runs all five steps in order.
9. The migration registry contains exactly the contiguous 0-to-1, 1-to-2, 2-to-3, 3-to-4, and 4-to-5 steps
   and ends at the current schema constant. A failure at any point drops the borrowed transaction
   and restores the complete source schema.

Settings transactions reuse whole-project publication. The complete candidate key map is rebuilt
in canonical order, then exact key membership, value types, ranges, modes, drop-frame compatibility,
pinned color identity, bounded cache pairs, render override pairs, and conditional key presence are
validated together. A successful semantic change advances once beside every other project owner.
Invalid, stale, duplicate-key, empty, oversized, or no-op transactions cannot partially publish.

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
mapping. Read-only connections also enable query-only mode. Schema 4 contains no trigger, view,
network, process, device, or GPU behavior and adds one strict extension-record table to the frozen
schema-3 layout. Persisted lifecycle describes user-controlled project state, not live runtime
readiness, worker identity, PID, registry availability, or process supervision. Migration never
opens a second connection and never owns commit
authority outside the one outer transaction.

`superi-engine::resources::MediaResourceRequest::from_project_media` is the real target consumer.
It resolves one stored project filesystem path, retains `MediaId` and expected fingerprint, rejects
explicit missing state, and feeds the ordinary media-I/O request. `acquire_project_resources` then
clones the exact selected compilation from `ProjectSnapshot`, including reloaded direct graph edits,
and acquires the exact reachable source and decoder set before one resources publication.

## Dependencies and consumers

- `superi-core` supplies typed identities, stable primitive revision, classified errors,
  recoverability, diagnostic context, exact time and timecode validation, and the shared versioned
  setting key, value, and snapshot contracts.
- `superi-audio` supplies the authored `ClipMixState` aggregate and its strict canonical component
  codec. Prepared processors, devices, queues, and callback state do not enter the project layer.
- `superi-graph` supplies editable graph snapshots and canonical graph encoding and checked
  decoding.
- `superi-timeline` supplies the editorial model, compiler, canonical timeline component codec, and
  strict Serde support for `TimelineGraphValue` inside graph component documents.
- Exact `rusqlite` 0.32.1 supplies safe SQLite access with bundled SQLite 3.46.0 and no public SQL or
  connection type leakage. Its bundled feature also exposes modern defensive configuration.
- Exact `sha2` 0.10.9 supplies component and private manifest integrity plus the separately
  domain-separated public semantic project hash. The two digest domains have different inputs and
  authority.
- Exact `fs4` 1.1.0 supplies synchronization-only nonblocking operating-system file locks for the
  persistent sibling writer-lock entry without adding networking, process discovery, or public API
  types.
- `serde` and `serde_json` supply strict canonical settings and extension metadata encoding and
  decoding, while tests also use JSON to build exact legacy revision-0 component fixtures from
  current canonical payloads.
- `superi-engine` consumes immutable snapshots for transactional resource acquisition, adapts
  project-owned referenced-media paths into media-I/O source requests, resolves settings into
  existing subsystem types, dispatches authoritative settings transactions, exclusively owns
  bounded command history and compound transactions over the checked whole-snapshot restore seam,
  routes extension commands and typed results through the same dispatcher and event owner,
  supplies the real selected history snapshot used by the autosave consumer contract after apply,
  undo, and redo, and exposes eventless `InspectProjectDiagnostics` results from that same attached
  history snapshot.
- `superi-api` consumes project settings and recovery only through engine-owned re-exports and
  dispatcher commands. Its local scripting contract uses this crate as a test-only downstream
  proof for SQLite reload, semantic integrity, media identity, autosave discovery, comparison, and
  recovery restoration; production script mutation still reaches project only through engine and
  `ProjectEditorApi`. Its local project host reaches the curated engine re-export seam for database
  open, creation, publication, copy, backup, recovery, and validation, then supplies durable file
  commands to `superi-cli` without a direct API-to-project dependency. Future adapters must continue
  to wrap this owner instead of creating another project or database authority.
- Editor, script, and headless callers can consume the same project-owned integrity report directly.
  No API, CLI, engine, transport, or GUI adapter is added by this checkpoint.

## Invariants and operational boundaries

- The selected root exists and has exactly one retained timeline compilation at the current
  editorial revision. Every retained graph identity is unique and equals its ordered map key.
- Every project owns one exact schema-`1.0.0` settings snapshot. Defaults derive only from the
  selected root edit rate; all other defaults are deterministic and platform neutral.
- Setting transactions contain one to 64 unique known keys and validate the complete candidate.
  Values are never coerced, conditional pairs remain complete, and no-op publication preserves the
  document revision.
- Optional working, cache, and proxy folder values are platform-neutral project policy strings.
  Each is nonempty, NUL-free, and bounded to 4,096 bytes; this module does not perform relocation.
- Extension identity is the ordered compound extension and record key. A project contains at most
  4096 records, each record ID is at most 128 bytes, each opaque payload is at most 64 MiB, each
  capability set is at most 1024 entries, and structured failure context and message counts are
  bounded before publication.
- Extension kinds are open namespaced component IDs. Plugin, effect, and AI artifact helpers are
  conveniences, not a closed enum. Unknown kinds, future extension versions, future payload schemas,
  and non-UTF-8 payload bytes remain exact opaque data as long as the project envelope is valid.
- Granted capabilities are always a user-controlled subset of requested capabilities. Disabled and
  quarantined lifecycle states remain durable and scriptable; quarantine requires structured
  failure evidence. Persisted state never claims that a runtime, worker, schema factory, or plugin
  registry is currently available.
- Authored graph effect parameters remain graph-owned. The effect extension kind stores only
  auxiliary extension-owned state. Generated AI output remains an ordinary editable artifact; the
  extension record stores only supplementary provenance, lifecycle, capability, or failure meaning.
- Document edits, in-memory database replacement, schema migration, and prepublication file saves
  are all-or-nothing. Stale revisions, failed closures, invalid candidates, preflight bounds, SQL
  failures, failed reload, snapshot inequality, or precommit interruption publish nothing.
- Whole-snapshot restoration is also all-or-nothing. It requires matching project identity and the
  exact current revision, preserves the current aggregate on validation or revision exhaustion,
  returns equal state without a revision, and gives changed state one fresh monotonic revision.
- Schema identity is explicit at three levels: SQLite application ID, monotonic schema revision,
  and semantic format plus version. The stable primitive and component revisions are recorded too.
- Schema 5 has exactly eight strict tables, adding command-log metadata and bounded typed command
  rows to the six authored-state tables. Extra user tables, indexes, views, or triggers are corruption.
- Frozen schema 4 has exactly six strict tables, one metadata singleton, one timeline singleton, one
  settings singleton, one audio singleton, bounded graph rows, and bounded extension rows. Extra
  user tables, indexes, views, or triggers are corruption; the layout remains readable only through
  migration and is not silently reinterpreted as schema 5.
- Frozen schema 3 has exactly five strict tables with audio but no extension table and remains
  readable only through migration. Its manifest and component semantics are not silently
  reinterpreted as schema 4.
- Frozen schema 2 has exactly four strict tables with settings but no audio, and remains readable
  only through migration. Its manifest and component semantics are not silently reinterpreted as
  schema 3.
- Frozen schema 1 has exactly three strict tables and remains readable only through migration. Its
  manifest and component semantics are not silently reinterpreted as schema 2.
- Supported schema 0 also has exactly three strict tables and retains every semantic field needed
  for lossless reconstruction. It may carry declared timeline or graph component revision 0 or 1,
  both of which are checked and canonicalized by their owning codec during migration.
- Project and graph revisions use canonical decimal text so every `u64` revision is preserved.
  Typed IDs use fixed 16-byte big-endian blobs.
- Canonical timeline, graph, and audio component bytes remain owned by those crates. Project owns
  canonical settings and extension metadata bytes and stores every component plus extension payload
  with exact length and SHA-256 evidence without duplicating timeline, graph, audio, authored effect,
  or generated artifact semantic fields.
- Timeline remains the owner of media identity, opaque target text, and relink evidence. Project
  interprets only recognized filesystem target syntax, preserves unknown locators, and never derives
  identity from a path. Relative resolution is lexical and requires an absolute project file path;
  foreign-platform absolute targets and future target versions fail explicitly.
- The project manifest remains private storage-integrity evidence and includes the outer document
  revision plus schema and format identity. It is never exposed or reused as the public semantic
  project hash.
- The public semantic hash is explicitly revisioned and domain separated. It covers stable
  primitive and component codec revisions, project and selected-root identity, canonical component
  lengths and digests, extension compound identity and payload schema, and graph identity, scope,
  revision, and canonical evidence.
- The observed outer document revision is correlation evidence only. Equal authored meaning after
  undo, recovery, save, copy, backup, migration to current canonical bytes, or reopen must retain the
  same content hash even when the document revision, destination path, or SQLite layout differs.
- Components are immutable and ordered by fixed family, then extension compound identity, then
  graph identity. Callers can identify the first changed component without parsing private SQLite
  rows or opaque payload bytes.
- Wrong application identity, future schema or format versions, and read-only legacy open are
  unsupported. Malformed, noncanonical current state, tampered, missing, extra, or inconsistent
  stored state is corrupt data.
- Integrity validation is read-only and role neutral. It never creates a missing path, writes or
  migrates a database, repairs a digest, salvages partial state, changes active identity, or selects
  recovery authority.
- A valid or migration-required integrity report carries verified identity only after complete
  semantic reconstruction. Invalid, unsupported, indeterminate, truncated, or source-changed reports
  carry no verified identity.
- Integrity findings and evidence are deterministic and bounded. Truncation or source mutation makes
  the report indeterminate, and a recovery disposition is guidance to the separately validated
  recovery controller rather than direct mutation permission.
- Semantic row order, canonical component bytes, private manifest digest, and public semantic hash
  framing are deterministic. SQLite page layout is not a public deterministic contract and never
  enters semantic diagnostics.
- `create`, backup, and require-absent publication do not overwrite an existing path. Migration
  changes only the already opened source database after the complete legacy document is checked and
  only at commit. Replace-existing accepts only a validated project and preserves destination
  permissions on Unix. Copy and backup never rebind active identity; save-as does so only after
  publication.
- Every file save publishes one complete current-schema snapshot. It never exposes the candidate as
  active state, retains no live SQLite handle across ordinary calls, and never reports an unconfirmed
  postpublication error as if the old path were restored. Directory synchronization is explicit on
  Unix; other platforms do not receive an unsupported physical-durability guarantee.
- A file-backed database retains one validated active generation containing complete-byte SHA-256,
  length, modification time, and available local file identity. Load, Save, and same-path SaveAs
  compare that generation before authority changes; a conflict preserves the caller snapshot and
  collaborator bytes. Permission-only changes remain compatible and are preserved by the next save.
- Replacement publication is serialized by one deterministic persistent sibling lock file. Lock
  acquisition is nonblocking, the lock is held through generation validation and the publication
  commit point, unsafe lock entries fail closed, and the file remains as coordination state so
  unlinking cannot split cooperating writers into different lock generations.
- Autosave is bound to exactly one project identity and accepts only caller-supplied monotonic time.
  It starts no timer or thread, stores no process clock in project meaning, publishes at most once
  for each observed tick, and rejects backward time without changing schedule or files.
- Periodic duplicate suppression requires both the same document revision and the still-present
  exact managed recovery point. Manual save-now always publishes. Failed publication marks neither
  a revision nor a deadline complete and may retry at the same elapsed value.
- Managed autosave ownership is defined only by the exact project child name and exact 20-digit
  generation syntax. Retention is count based and generation ordered, never wall-clock, mtime, or
  directory order based. Unknown names and save candidates remain untouched.
- A managed-name symlink, directory, or nonregular entry blocks every prune before deletion.
  Pruning uses individual `remove_file` calls only, never recursion or glob expansion, and retains
  honest partial progress plus publication evidence when cleanup fails.
- Autosave does not open retained artifact contents during scan or prune. Recovery owns database
  validation, semantic comparison, and exact dismissal, while the engine owns restoration of the
  active project and session history.
- Recovery never accepts an arbitrary path. It resolves one opaque generation under the exact
  project namespace, revalidates regular-file identity before compare, restore load, or dismissal,
  and keeps all paths and source chains inside internal diagnostics.
- A durable tombstone transition is the dismissal commit point. Cleanup errors produce degraded
  evidence rather than failed dismissal or candidate resurrection, and restart discovery cleans
  recognized regular tombstones only.

## Tests and verification

`document_contract.rs` contains seven public tests over coherent construction, immutable equal
snapshots shared across three roles, ordinary typed graph edits, stale and failed edit rollback,
no-op revision stability, editorial recompilation coherence, standalone graph state, checked parts
reconstruction, revision-fenced whole-snapshot restoration, monotonic undo-style publication,
identity rejection, and exhaustion atomicity.

`persistence_contract.rs` contains six real database tests. They prove nonoverwriting creation and
read-only reopen, exact schema objects and version identities, equal semantic rows across independent
  databases, complete editorial, media fingerprint mismatch, multicam, direct graph edit, standalone,
  exact authored audio, known and future extension kinds, lifecycle, capabilities, structured
  failure, non-UTF-8 opaque payload, and revision preservation, post-load conflict fencing and
  editing, preflight rollback, read-only denial, and rejection of wrong application identity,
  future revisions, corrupt components, altered extension metadata or payload evidence, invalid
  command-log evidence, missing rows, and extra views. The focused command-log persistence test
  proves exact reopen while authored revision remains unchanged.

`migration_contract.rs` contains three public database tests. They prove exact schema-0 and frozen
  schema-1 component compatibility, the contiguous 0-to-1-to-2-to-3-to-4-to-5 chain, deterministic settings
defaults, canonical audio initialization, current canonical rewrite, source-revision reporting,
complete snapshot equality, post-migration edit and resave, current-schema save, save-as, copy, and
backup without changing the reported source revision, read-only legacy refusal, byte-stable current
open, future schema nonmutation, and malformed legacy logical rollback. Private migration contracts
prove that frozen schema-2 projects preserve settings while gaining canonical empty audio, frozen
schema-3 projects gain an empty extension set without semantic drift, schema-4 projects gain an
empty command log without authored semantic drift, and a classified failure after rewrite restores
the exact source before the production path succeeds.

`project_settings_contract.rs` contains three public tests. They prove exact defaults, atomic
invalid-candidate rollback, stale fencing, semantic no-op stability, complete timeline, color,
audio, cache, proxy, and render validation, bounded transaction construction, schema-5 persistence,
manifest coverage, and migration defaults.

`extension_state_contract.rs` contains three public command contracts. They prove one role-neutral
surface for plugin, auxiliary effect, AI artifact provenance, and unknown future state, exact opaque
bytes, deterministic iteration, semantic no-op behavior, capability subset enforcement, user
disable, quarantine and structured failure state, explicit recovery control, stale revision
fencing, removal, record and payload bounds, and atomic invalid-command rejection.

`media_reference_contract.rs` contains five public tests. They prove canonical portable relative
paths, versioned and compatible target decoding, deterministic project-relative resolution,
foreign-platform visibility, stable `MediaId` commands, revision conflicts, preserved direct graph
edits, explicit missing and fingerprint-mismatch state, exact database round trips, accepted relinks,
and safe rejection of opaque or future syntax.

`save_contract.rs` contains ten public file contracts. They prove atomic active replacement,
save-as rebinding from memory and read-only sources, copy and backup identity preservation, both
collision policies, supplied live-state capture, invalid destination and canonical alias rejection,
bounded failure before filesystem mutation, non-UTF-8 paths and hard-link alias detection on Unix,
dangling symbolic-link rejection, permission preservation, and exact current-schema reload plus
integrity for every published artifact. They also prove that stale authorities cannot overwrite a
collaborator, that stale loads fail visibly, that SaveCopy and distinct SaveAs remain explicit
recovery paths, and that missing, corrupt, directory-substituted, or symlink-substituted active
files are never clobbered. Private save tests inject every prepublication and postpublication stage,
exercise destination and same-baseline writer races in real subprocesses, prove one winner and one
visible conflict, separately prove that an actively held sibling lock is retryable, reject unsafe
lock entries, distinguish same-length content generations, classify representative storage
exhaustion, and abort subprocesses at
candidate-close, candidate-sync, publication, and completion stages for both rename and no-clobber
publication paths.

`autosave_contract.rs` contains five public controller contracts. They prove exact due boundaries,
disable and re-enable control, forced manual recovery points, unchanged periodic suppression,
republication after external removal, one-save forward jumps, backward-time atomicity, exact
current-schema reopen equality, unchanged active-project bytes, deterministic generation order
despite reversed mtimes, bounded retention, explicit pruning, strict lookalike exclusion, candidate
and foreign-file preservation, managed-name symlink rejection before any delete, invalid policy and
deadline rejection, no-clobber next-generation selection, generation exhaustion without state
advance, and same-time retry. The engine consumer separately autosaves selected immutable history
state after apply, undo, and redo and requires exact snapshot equality on every reopen.

`recovery_contract.rs` contains four public contracts. They prove deterministic discovery from the
exact C009 namespace, complete valid loading beside degraded corruption, stable catalog revisions,
typed semantic comparison including authored clip-mix state, wrong-project and symlink correction
actions, retryable stale file identity without mutation, exact durable dismissal, foreign-file
preservation, restart discovery,
and recognized tombstone cleanup. Private classification coverage proves retryable and terminal
source evidence is retained rather than downgraded.

`diagnostics_contract.rs` contains three public contracts. They prove fixed SHA-256 algorithm and
hash-format revision identity, construction-order-independent reports, stable lowercase digest
display, media ID, fingerprint, target, and rejected-relink sensitivity, and exact timeline,
settings, clip-mix, extension, and graph component difference visibility. They also prove that
monotonic snapshot restoration recovers the prior semantic hash under a newer observed document
revision and that two different file paths with byte-different SQLite page layouts reload to one
equal diagnostics report.

`integrity_contract.rs` contains six public tests. They prove deterministic editor, script, and
headless interpretation; full current-schema identity; complete linked-media, retained-graph,
settings, authored-audio, and opaque-extension reconstruction; schema-0 migration reporting without mutation;
continued writable migration through the existing authority; wrong-application, future-schema, and
non-SQLite classification; missing-source noncreation; exact timeline, graph, settings, audio,
extension metadata, extension payload, manifest, singleton, and extra-schema-object findings; stable ordering; bounded evidence; and byte
nonmutation of every inspected source. Six unit tests prove permanent code strings, UTF-8 bounds,
complete foreign-key collection, exact finding-limit behavior, truncation, source-change precedence,
and source versus semantic not-found classification.

The downstream `superi-api` scripting contract persists the selected scripted snapshot through the
real database, reloads exact equality, validates full integrity, discovers and compares an autosave
candidate, restores it through the existing recovery owner, and proves stable media ID, target, and
fingerprint meaning. This adds no interpreter or file authority to the project crate.

`version_negotiation_contract.rs` contains two public tests. They prove the exact six released
schema and semantic format pairs, stable application, text, and primitive identities, current and
registered migration outcomes, the exact schema 1 to 5 successor path, future schema, semantic, and
primitive reasons, inconsistent schema-format rejection, and foreign identity classification.

The downstream local project host and CLI contracts use the same database and save authority for
no-clobber creation, complete reopen, mutation publication, copy, backup, recovery, validation, and
bounded automation. They prove successful authored changes reopen exactly, rejected or rolled-back
commands leave project bytes and meaning unchanged, and media availability changes preserve source,
record, timing, identity, grouping, linking, targeting, and synchronization intent.

Timeline's serialization contract independently round trips a real compiled multicam graph through
the public graph codec and rejects unknown `TimelineGraphValue` fields and tags. The engine resource
contract opens an exact schema-0 fixture through the public database owner and proves that the
migrated retained graph and real media stream reach resource acquisition unchanged.
Rust 1.80.0 check, strict Clippy, dependency direction, policy, formatting, map validation, and the
repository verifier remain required delivery gates.

## Current status and risks

The coherent in-memory document owner, authoritative versioned settings, authored clip-mix state,
bounded opaque extension records, bounded durable command records, schema-5 SQLite application format, deterministic timeline,
graph, settings, audio, and extension component records, integrity manifest, transactional in-memory
replacement, exact schema-0, schema-1, frozen schema-2, and frozen schema-3 compatibility, ordered
forward migration, checked reconstruction, durable create
and read-only reopen, writable current or legacy open, atomic save, save-as, copy, and backup
publication, explicit collision policy, active path identity, validated modified-since-open
generation fencing, cooperative writer serialization, versioned referenced-media paths,
stable identity commands, and the real engine and public API consumers are substantive and
test-backed. The same authoritative compatibility table now drives persistence checks, migration
registry alignment, integrity interpretation, and the public API projection. Its checked
whole-snapshot restore seam supports the engine-owned session command
history without moving branching policy or retained entries into the project crate. The same
history, compound transaction, dispatcher, save, and autosave consumers preserve plugin, effect,
AI artifact metadata, and unknown extension state through one typed command surface without
requiring a runtime implementation. Its typed
autosave controller adds deterministic host-driven scheduling, complete current-schema recovery
points, bounded count retention, strict managed naming, safe pruning, and user control while reusing
the same atomic Backup authority and leaving active project identity unchanged. Its recovery owner
now discovers, validates, compares, classifies, and durably dismisses those exact recovery points
without exposing filesystem identity or creating a second store. The public integrity command adds
complete current and supported legacy reconstruction, deterministic bounded evidence, verified
identity, and repair reporting without creating another database or recovery authority.
The public semantic diagnostics surface now adds a versioned SHA-256 content identity and ordered
typed component evidence over canonical prepared state, and the engine dispatcher consumes it
without events or history mutation.

Additional schema revisions beyond 5, persisted undo and redo branch history, authenticated integrity,
and transport-catalog database adaptation remain absent. The API-owned local host and CLI now
provide durable process workflows through this crate's existing authority. This crate has no script
interpreter or source loader; the supported API-local runtime above it preserves project meaning
through the existing engine command, snapshot, persistence, integrity, autosave, and recovery
owners.
Autosave policy is process-local and recovery roots are caller selected; no background timer,
persistent scheduler, wire adapter, runtime registry, plugin worker, or automatic recovery choice
is claimed. Exact schemas 0, 1, 2, and 3 are the supported predecessors. Future, older unknown, or
extended layouts remain rejected until an explicit migration or preservation contract exists.
Current atomicity proofs cover ordinary local filesystem semantics, including cooperating
cross-process replacement through the persistent sibling lock. They do not claim interoperability
with writers that ignore the lock, reliable advisory-lock behavior on every network filesystem, or
physical power-loss proof on every platform. Rust 1.80 exposes no stable portable Windows file
identity, so a pre-existing Windows hard-link alias is not recognized as the active path; explicit
collision policy still prevents unintended no-clobber publication, and replace-existing changes
only the named destination entry without mutating the original active link.

Bundled SQLite increases the locked build graph and must remain pinned to the Rust 1.80-compatible
rusqlite 0.32.1 path unless a separately verified upgrade changes that decision. Integrity digests
and semantic project hashes detect changes but do not authenticate malicious files. Hash equality
means equal canonical authored evidence under one declared framing revision, not file identity,
runtime readiness, safe mergeability, or storage integrity. The `data_version` fence detects changes
visible to the open connection but does not claim a cross-process filesystem replacement lock.
Defensive configuration, strict schema inspection, bounds, and checked domain reconstruction remain
mandatory for untrusted projects.

## Maintenance notes

Keep one project aggregate, one database and file-publication authority, one authoritative settings
vocabulary, and one publication revision. New semantic state must enter `ProjectState`, immutable
snapshot access, complete validation, schema evidence, round-trip comparison, tests, and maps
together. Do not add
hidden intelligent-result storage, a second editorial or settings owner, unchecked graph mutation,
or a second persistence model.

Preserve timeline, graph, and audio component ownership and project ownership of settings,
extension envelopes, opaque payload bytes, lifecycle, capabilities, and structured failure evidence.
Incompatible project layout changes require a new monotonic schema revision, semantic version
decision, and one exact successor step appended to the contiguous migration registry. Do not change
schema 0, schema 1, schema 2, schema 3, schema 4, or schema 5 in place after release. Keep every compatibility
consumer synchronized with the one released-format table; never add a second schema-to-format match
in persistence, integrity, engine, or API code. Keep every file-backed save operation on
`ProjectSaveCommand` and the existing complete-candidate publication path. Preserve
active-path rebinding at the publication commit point, explicit collision policy, precommit cleanup
ownership, the persistent sibling lock, active and destination generation fencing, and honest
published-error context together. Keep autosave clockless, host-driven,
count-bounded, and routed through `ProjectSaveCommand::Backup`. Preserve its strict filenames,
foreign-entry exclusion, regular-file-only pruning, and postpublication evidence
together with recovery's opaque identity, complete load, file revalidation, classified diagnostic,
and tombstone dismissal contracts. Never add another project or file authority.

Keep integrity validation on the shared hardened read-only connection, complete SQLite collector,
registered schema readers, canonical component codecs, and checked aggregate reconstruction. Add new
finding or disposition codes only as stable semantic contracts, preserve bounded deterministic
evidence and explicit indeterminate state, and route any actual repair through the existing migration,
save, or validated recovery authority instead of adding mutation to the integrity command.

Keep semantic diagnostics on `PreparedProject` canonical evidence and separate from the private
manifest. Any input, ordering, tag, field framing, algorithm, or interpretation change requires an
explicit new `PROJECT_HASH_FORMAT_REVISION` and compatibility decision. Never add outer document
revision, paths, save identity, autosave generations, SQLite schema or page layout, runtime state,
or process state to the semantic content hash. Preserve typed component evidence and the eventless
engine consumer together.

Keep scripted project consumers on the same public command, immutable snapshot, canonical codec,
database, integrity, autosave, and recovery paths. Never add a script-specific project schema,
media identity, conflict policy, file format, hash, or recovery store.
Keep local API and CLI consumers on the same database, save, recovery, integrity, and immutable
snapshot authorities. Do not expose a second database handle, publication algorithm, path identity,
or collision policy above this crate.

Refresh this map after any project source, manifest, public consumer, schema, or test change. Reread
every changed file and relevant component interface through EOF, reconcile prose before recomputing
the hash, and validate all maps after integration and immediately before delivery.
