# Superi codebase map

This index is the repository-wide navigation layer for Superi. It summarizes ownership, dependency
direction, implemented runtime paths, incomplete architecture, and verification boundaries across
all discovered modules. Each linked module map is the detailed source of truth for one owned path.

The index describes implemented reality first. Statements labeled "documented target" come from
repository contracts but are not yet implemented end to end. Statements labeled "synthesis" are
cross-module conclusions derived from current manifests and module maps, and should be rechecked
against raw source before changing code.

## Module inventory

| Module ID | Map | Owned path | Current role | Status |
| --- | --- | --- | --- | --- |
| `superi-ai` | [module map](modules/superi-ai.md) | `open/crates/superi-ai` | Reserved local inference and editable-artifact boundary | Skeleton: public module names only |
| `superi-api` | [module map](modules/superi-api.md) | `open/crates/superi-api` | Transport-neutral public facade for media capabilities, complete engine health and readiness, coherent integration validation, canonical editorial state, and project settings | Partial: media and engine introspection queries, strict read-only integration validation, revision-fenced scenario and project settings transactions, and ordered replacement events implemented; wire transport, general editorial mutation, database file commands, and scripting absent |
| `superi-audio` | [module map](modules/superi-audio.md) | `open/crates/superi-audio` | Independent prepared audio graph with explicit channel conversion, typed bus routing, transactional clip DSP, canonical authored-state serialization, core effects, sample-accurate scheduling, bounded device playback, callback-owned discontinuity discard, dual-clock sample-rate conversion, and graph-native metering | Partial: graph processing, channel-layout conversion, bus routing, clip controls, strict canonical clip-mix persistence bytes, equalization, compression, limiting, delay, saturation, callback scheduling, audio-master publication, device output and discard acknowledgement, band-limited resampling, and peak, RMS, true-peak, phase, spectrum, and loudness metering implemented; project schema 3 durably stores authored audio, engine compound history reverses it, and engine export invokes an explicit audio stage, while decoded-sample binding to the prepared graph, automation, variable-rate playback audio, hosting, and complete timeline composition remain absent |
| `superi-cache` | [module map](modules/superi-cache.md) | `open/crates/superi-cache` | Composite reusable-result identity, budgeted final-frame and intermediate-node memory retention, priority-aware strict LRU eviction, precise graph edit invalidation, versioned corruption-recovering disk persistence, replaceable derived-media publication, layered render reuse, bounded background population, bounded playback prediction, bounded edit and scrub warming, and deterministic lifecycle management | Complete identity feeds independent memory and disk tiers with exact admission, revision fencing, bounded envelopes, atomic publication, schema isolation, and corruption quarantine; memory, persistent, and derived owners expose inspection and exact clearing, persistent namespaces relocate through rename or synchronized staged copy, render jobs add cancellation-safe layered reuse, prediction supplies finite signed frame plans and an owned host adapter, and warming is deterministic and hard bounded; engine and scheduler own quality substitution and lifecycle policy remains caller-owned |
| `superi-cli` | [module map](modules/superi-cli.md) | `open/crates/superi-cli` | Headless canonical editorial scenario and engine validation consumer | Implemented revision-fenced transaction and event consumer, portable expectation verifier, eight instrumented contract stages, and strict deterministic `engine validate`; rendered media flow and live application attachment absent |
| `superi-codecs-platform` | [module map](modules/superi-codecs-platform.md) | `open/crates/superi-codecs-platform` | Opt-in host codec adapters for Apple, Windows, and Linux | Implemented, host-dependent: native proof depth varies and legal review remains open |
| `superi-codecs-rs` | [module map](modules/superi-codecs-rs.md) | `open/crates/superi-codecs-rs` | Default permissive software codec implementations | Implemented: AV1, FLAC, MP3, Opus, PCM, Vorbis, VP8, and VP9 decode and encode |
| `superi-codecs-vendor` | [module map](modules/superi-codecs-vendor.md) | `open/crates/superi-codecs-vendor` | Explicit process adapter for separately installed vendor RAW workers | Implemented first revision: decode-only, CPU-only, JSON and hexadecimal IPC |
| `superi-color` | [module map](modules/superi-color.md) | `open/crates/superi-color` | Versioned configuration, project working spaces, color math, CPU input and output transforms, GPU wide-gamut transforms, tone mapping, legal-range RGB encoding, LUTs, ICC discovery, and presentation profile guards | Substantial but partial: project-pinned configuration, CPU transforms, managed GPU wide-gamut transforms, and an engine CPU display consumer are implemented; engine export validates a caller-owned delivery stage but does not execute this crate, while ICC evaluation, native GPU display conversion, concrete export conversion, and shell integration remain absent |
| `superi-concurrency` | [module map](modules/superi-concurrency.md) | `open/crates/superi-concurrency` | Execution domains, jobs, clocks, handoffs, shared snapshots, lifecycle, liveness, and derived-media selection | Substantial; audio enforces its domain, engine proxy resolution consumes selection, engine foreground playback and transport consume bounded workers, cancellation, anchor-based clocks, the A/V scheduler, and handoffs, engine lifecycle composes acknowledged startup, sleep, wake, shutdown, and restart phases with EngineControl ownership, immutable publication, and lock-free signals, engine error propagation keeps bounded bookkeeping in a separate EngineControl `DomainOwned`, render-export enforces lifecycle admission, and the engine export queue composes bounded workers, progress, dependency history, typed completion, and recovery attempts; broader liveness and GPU submission composition remain incomplete |
| `superi-core` | [module map](modules/superi-core.md) | `open/crates/superi-core` | Tier-zero values, validation, exact time, identifiers, errors, diagnostics, and stable serialization | Implemented and broadly consumed; crate-level skeleton wording is stale |
| `superi-effects` | [module map](modules/superi-effects.md) | `open/crates/superi-effects` | Graph-native visual definitions, editable defaults and instances, reusable presets, animation, controls, composition, spatial, shape, mask, rotoscope, tracking, text, transitions, isolated OpenFX hosting, and bounded CPU reference evaluation | Substantive but partial: graph-native authoring, strict documents and migrations, missing-plugin recovery, visual contracts, isolated OFX adapter validation, boxed dynamic forwarding, permissions, lifecycle, recovery, quarantine, workflow parity, and real pixel proof are implemented; effects-to-project persistence integration, native plugin binding, production GPU execution, concrete worker transport, UI, and complete timeline attachment remain absent |
| `superi-engine` | [module map](modules/superi-engine.md) | `open/crates/superi-engine` | Open subsystem assembly and orchestration | Partial: engine-wide typed command dispatch, bounded revision-fenced project history, compound timeline, graph, media, authored audio, and root transactions, complete project undo and redo, authoritative project settings resolution and dispatch, coherent project and device lifecycle, integration validation, bounded playback and export dispatch, exact project resource preparation, selected-history autosave consumer proof, render-export orchestration, shared arbitration, and plugin containment are implemented; production autosave hosting, generic public project-history adaptation, live subsystem reconfiguration from settings, platform power callback binding, concrete lifecycle workers, automatic arbiter binding, decoded source and prepared-audio binding, native GPU presentation, muxing and publication, concrete plugin transport, persistent job recovery, and production plugin factories remain absent |
| `superi-gpu` | [module map](modules/superi-gpu.md) | `open/crates/superi-gpu` | wgpu device, resource, upload, conversion, pass, submission, presentation, and recovery substrate | Implemented substrate with explicit application-level integration gaps |
| `superi-graph` | [module map](modules/superi-graph.md) | `open/crates/superi-graph` | Node-neutral identifiers and shared typed values, versioned schema discovery, deterministic DAG storage, typed port validation, editable mutation transactions, canonical graph documents, reusable scalar expressions, typed parameter links and expressions, caller-projected literal evaluation, derived missing-node resolution, dependency and semantic edit invalidation, region-of-interest propagation, request-scoped scheduling and evaluation, node introspection, graph and revision cache lineage, timing, and shared interactive and headless evaluation snapshots | Partial: graph-facing IDs, exact neutral domain and processing values, node schemas, immutable discovery, typed DAG state, atomic mutations, deterministic integrity-checked serialization, checked deserialization, legacy migration, shared bounded scalar programs, typed driver state, parameter-cycle protection, literal-only projected evaluation, fail-closed missing-node placeholders, exact region and edit invalidation, snapshot-bound ROI planning, generic demand-only evaluation, deterministic graph cache inspection, final and intermediate retained-work pruning, run-local timing, and role-neutral editable-to-runtime evaluation implemented; effects consumes broad authoring and reference evaluation, timeline compiles editable graphs, project retains and atomically publishes timeline and named standalone graph documents, and engine consumes externally prepared snapshots for playback and render-export, while production engine catalog and plugin binding and complete application rendering remain absent |
| `superi-image` | [module map](modules/superi-image.md) | `open/crates/superi-image` | Host image values, still interchange, CPU operations, sequences, previews, and reference validation | Implemented host-side subsystem with explicit representation limits |
| `superi-media-io` | [module map](modules/superi-media-io.md) | `open/crates/superi-media-io` | Codec-neutral source, demux, packet, frame, audio, selection, timing, and operation contracts | Implemented contracts and four demuxers; engine source registration, project-path request adaptation, preparation, and complete elementary-stream export lifecycle orchestration are integrated, while persistent path syntax, muxing, and publication remain outside this crate |
| `superi-project` | [module map](modules/superi-project.md) | `open/crates/superi-project` | Whole-project in-memory document, durable settings and authored audio, stable SQLite serialization and migration, atomic save, save-as, copy, backup, and autosave publication, referenced-media paths, and recovery boundary | Partial: coherent revisioned aggregate, immutable snapshots, checked monotonic whole-snapshot restoration, authoritative versioned settings, retained timeline and named standalone graphs, authored clip-mix state, schema-3 SQLite identity and five strict tables, deterministic canonical components and integrity manifest, exact frozen schema-0, schema-1, and schema-2 compatibility, contiguous forward migration, checked reconstruction, durable publication, stable media commands, deterministic host-driven autosave scheduling, complete Backup recovery points, bounded generation retention, safe pruning, typed user control, real engine history and settings resolution, and stable public settings control are implemented; persisted command logs, recovery discovery and restoration, modified-since-open conflict policy, database file API, CLI, and scripting remain absent |
| `superi-timeline` | [module map](modules/superi-timeline.md) | `open/crates/superi-timeline` | Native editorial project state, media bins and saved queries, metadata and relink state, rational range maps and availability, exact clip retiming, typed tracks, authoritative edit intent, markers, exact snapping, clip relationships, atomic foundational, advanced, nested, and multicam operations, OTIO 0.18.1 interchange, versioned integrity-checked state documents, color metadata propagation, and deterministic typed graph compilation | Foundational model, media and metadata state, exact retiming, typed tracks, edit operations, nesting, multicam intent, OTIO interchange, strict timeline documents, compiled graph-value Serde, color metadata, stable editable timeline-to-graph compilation, three-way preservation of nonconflicting direct graph edits, downstream schema-3 project retention and atomic file publication, engine compound history restoration, and engine preparation retention are test-backed; broader interchange interpretation, fit-to-fill, grouped-source compound synthesis, timeline-driven autosave scheduling, graph evaluation, multicam mixing, playback, and render integration are absent |
| `tool-superi-dependency-check` | [module map](modules/tool-superi-dependency-check.md) | `open/tools/superi-dependency-check` | Offline executable policy for the open runtime dependency graph | Implemented exact runtime, build, dev, and new-crate checks |
| `tool-superi-boundary-tool` | [module map](modules/tool-superi-boundary-tool.md) | `open/tools/superi-boundary-tool` | Offline scanner for network-client and open-to-closed policy | Implemented library, CLI, workspace gate, and hosted-build command |
| `tool-superi-bench` | [module map](modules/tool-superi-bench.md) | `open/tools/superi-bench` | Stable benchmark harnesses and reproducible stage reporting | Implemented seven-stage runner with real graph evaluation and explicit gaps |
| `tool-superi-fixture-tool` | [module map](modules/tool-superi-fixture-tool.md) | `open/tools/superi-fixture-tool` | Offline fixture validation, generation, and typed golden verification | Implemented validation library, six generators, seven-command CLI, four golden harnesses, and focused contracts |
| `tool-superi-test-report` | [module map](modules/tool-superi-test-report.md) | `open/tools/superi-test-report` | Offline structured platform-lane evidence generator | Implemented strict schema, deterministic findings, collision-safe CLI, and focused contracts |
| `workspace` | [module map](modules/workspace.md) | Repository files outside `open/crates/*` and `open/tools/*` | Product law, architecture, policy, workspace configuration, fixtures, and agent workflows | Active control layer: deterministic checkpoint workflow and contract slice delivered; focused playback and elementary-stream export runtime paths exist, while the canonical application slice remains absent |

## Ownership and repository boundaries

The mapping script assigns each `open/crates/<name>` package to one crate module and each
`open/tools/<name>` package to one tool module. The `workspace` module owns the remaining discovered
repository files, including product and architecture documents, Cargo workspace configuration,
license and codec policy, platform test policy, unsafe-boundary inventory, shared fixture data, CI,
and repository-owned agent skills. Generated maps, local plans, Git internals, build output, and
ignored files do not contribute to module hashes.

`AGENTS.md` is the repository's operational law but is intentionally ignored by Git and therefore
outside the generated workspace inventory and hash. It must be read separately for every task even
when all maps validate. `.worktreeinclude` copies it into Codex-managed worktrees.

Checkpoint work reads this global index plus every directly affected, caller, consumer, contract,
and runtime-path map in full. Another map may be omitted only through the recorded deeper raw-code
substitution defined by root law; directly affected and contract-path maps remain mandatory.
Planning and execution write only `plans/<id>/planning.md` and `plans/<id>/execution.md`, then the
changed-path verifier establishes the deterministic local gate floor. Hosted CI status is not a
general checkpoint completion gate unless the checkpoint explicitly owns hosted CI behavior.
Multi-checkpoint dispatch defaults to three active worktrees but follows any explicit positive
concurrency value from the user.

The open runtime and tool workspace lives under `open/`. Current Cargo membership is 19 runtime
crates plus `superi-fixture-tool`, `superi-dependency-check`, `superi-boundary-tool`, and
`superi-bench`, and `superi-test-report`. All five tools are built with the workspace but remain
outside the runtime dependency graph. The root
`closed/README.md` is only a boundary notice for the separately maintained proprietary tier. Open
Superi must never import, link, or depend on closed code. Closed
Superi may consume the same open public API as any other client and must produce ordinary editable
artifacts through that public seam.

Open operation is required to remain offline, account-free, and independent of hosted fallback.
The platform codec module may inspect installed operating-system frameworks, local drivers, render
nodes, and build headers. The vendor codec module may start only executables explicitly selected by
the caller. Neither boundary discovers or downloads remote codec implementations.

## Dependency direction

Dependency arrows below point from a consumer to a dependency. This is a synthesis of the current
Cargo graph and module maps. Several manifest edges are scaffold declarations only and have no
current Rust call site.

```text
superi-cli
  -> superi-api
    -> superi-engine

superi-engine
  -> superi-codecs-rs
  -> superi-codecs-platform        optional through os-codecs
  -> superi-codecs-vendor          optional through vendor-codecs
  -> superi-media-io
  -> superi-gpu
  -> superi-core                   errors, diagnostics, safe projection, exact values
  -> superi-cache                  derived publication, substitution, color, playback retention, prediction
  -> superi-concurrency            selection, playback and export jobs, cancellation, progress, dependencies, clocks, handoffs, acknowledged power lifecycle, EngineControl ownership
  -> superi-timeline               graph compilation, three-way retained-edit reconciliation, reachable media preparation, signed playback rate
  -> superi-graph                  foreground evaluation and retained timeline compilation
  -> superi-project                immutable snapshots, checked edit and restoration, authored media commands, durable clip-mix state, and autosave contract
  -> image, color, audio           active playback and display contracts
  -> effects and ai                partial or manifest-only integration

superi-project -> superi-audio
superi-project -> superi-timeline -> superi-graph -> superi-image
superi-color, superi-effects, superi-cache, superi-ai -> lower graph/image/GPU/core layers
superi-cache -> superi-concurrency        bounded background render jobs
superi-audio -> superi-concurrency -> superi-core
superi-graph -> superi-gpu, superi-image, superi-concurrency, superi-core

superi-codecs-rs, superi-codecs-platform, superi-codecs-vendor
  -> superi-media-io
  -> superi-core

superi-media-io -> superi-image -> superi-core
superi-gpu -> superi-core

superi-bench -> superi-graph -> superi-core
```

`superi-core` is the tier-zero semantic contract and has no Superi dependency. Higher modules must
not copy its identifier, time, geometry, color-tag, pixel, audio-layout, error, diagnostic, or
stable serialization meanings into competing local types without an explicit boundary conversion.

The generic graph direction is deliberately one way. Graph may depend on representation and
execution substrates, while color, effects, cache, timeline, AI, project, and engine may depend on
graph. Graph must not depend upward on a domain catalog. Its implemented surface uses core-owned
object identifiers and core semantic contracts for schema definitions, and it owns generic
deterministic DAG storage, opaque typed bindings, deterministic request-scoped scheduling and
evaluation, typed parameter drivers and bounded expressions, derived exact-schema availability, and
structured validation errors. Graph also owns a catalog-neutral value payload that preserves exact
domain state beside finite processing values without naming an effect. Effects is the first
higher-tier authoring, animation, concrete schema, expression, diagnostics, compiler, generic
persistence, and reference-evaluation consumer above that boundary. Its strict keyframe payload and
built-in visual definitions remain ordinary editable graph state. Effects presets retain one
complete saved schema and every typed literal, instantiate fresh ordinary graph nodes, and use
graph-derived missing availability plus canonical documents without adding preset or plugin state to
graph. Explicit effects-owned schema migrations transform complete preset parameter sets through
checked forward steps rather than changing graph's schema resolver. Effects also reuses the bounded scalar
program for time and parent expressions, compiles reusable controls into ordinary typed drivers,
and projects literal curves into exact-time samples through the graph-owned evaluator without adding
effect types to graph. The visual composition, spatial composition, vector-shape, mask, rotoscope,
tracking, and text contracts likewise persist strict visual layers, animated transforms, cameras,
lights, depth and shutter state, editable shape documents, animated mask stacks, exact-frame
artifacts, tracked selections and observations, and styled text domain state through the neutral
value payload and generic graph documents without adding composition, spatial, shape, path,
propagation, tracking, solver, camera-pose, font, shaping, or paragraph meaning to graph.
Effects-owned compositions retain visual parenting, precomposition collapse, and layer-to-source
time maps, while timeline remains authoritative for editorial nested sequences, clip retiming, and
edit policy.
Effects transition definitions also remain ordinary graph schemas with typed parameters and a
bounded reference evaluator. Timeline compilation consumes the same value payload, schemas,
editable storage, atomic mutations, and
immutable snapshots without importing effects; it remains authoritative for transition identity,
adjacency, handles, record placement, grouping, synchronization, persistence, and mutation. A later
integration owner may pair its neutral transition projection with the effects schemas without
reversing this dependency. Timeline and cache also consume the graph-owned color metadata wrapper,
but no timeline path consumes graph evaluation, documents, animation curves, the effects catalog,
or a production runtime factory.

Codec implementations depend down on the codec-neutral `superi-media-io` interface. Media I/O does
not depend on a concrete codec, engine, or registry assembler. The engine owns the current assembly
choice. The API depends on engine-owned projections rather than leaking media-I/O implementation
types. The CLI depends only on the API for editorial control and never imports engine scenario
state directly.

## Public control flow

### Implemented today

Media registry construction and capability introspection are implemented as follows:

1. `superi-engine::media` creates a `BackendRegistry` and registers the default Rust codecs.
2. The `os-codecs` feature may append host-discovered platform codecs.
3. Engine construction creates and preflights primary priority-100 registrations for the in-tree
   Matroska or WebM, MP4 or MOV, MXF, and PCM container sources before inserting any of them.
4. The separate vendor constructor may append only explicitly configured vendor workers.
5. `superi-engine::introspection::MediaCapabilities::from_registry` reads declarations without
   opening sources or constructing codecs, then produces deterministic engine-owned records.
6. `superi-api::MediaCapabilitiesApi` projects those records into strict serializable API types.
7. `GetMediaCapabilities` clones the current full snapshot. `synchronize` emits one full-replacement
   `MediaCapabilitiesChanged` event only when semantic capability state changes.

Complete engine capability and health introspection is implemented as follows:

1. An EngineControl caller asks `EngineCommandDispatcher::introspection_snapshot` for one read-only
   observation, supplying immutable media declarations and optionally one exact resource-arbitration
   snapshot.
2. The dispatcher reads its canonical lifecycle and recovery state without dispatching a command,
   advancing a sequence, publishing an event, or changing project meaning.
3. Workflow readiness is derived through the same lifecycle admission checks used by playback,
   rendering, and export. Active failures retain disposition and recovery progress but expose only
   the core-reviewed `UserSafeError` projection.
4. `superi-api::EngineIntrospectionApi` maps that state into strict public types. The enclosing
   snapshot and nested media capability state have independent API-local revisions.
5. `GetEngineIntrospection` returns complete replacement state, while semantic changes emit one
   `EngineIntrospectionChanged` event and equal observations emit nothing.

Coherent integration validation extends that same read-only state as follows:

1. An EngineControl host asks `EngineCommandDispatcher::integration_validation_snapshot` for one
   observation, supplying the same immutable media declarations and optional resource snapshot used
   by canonical introspection.
2. The dispatcher nests the exact `EngineIntrospectionSnapshot`, then adds scenario reversal state,
   precise lifecycle and recovery actions, revision-scoped workflow permits or denials, endpoint
   attachment, and retained playback and export replacement observations.
3. Deterministic findings compare introspection with lifecycle, recovery, and workflow state and
   reject mismatched recovery actions or endpoint attachment. The read polls no worker and advances
   no command, event, scenario, lifecycle, recovery, capability, or resource revision.
4. `superi-api::IntegrationValidationApi` projects strict schema `1.0.0` without duplicating the
   nested public introspection types. UI and tests can supply live dispatcher observations, while
   the CLI calls the API-owned fresh-engine helper and remains dependent only on `superi-api`.
5. `superi-cli engine validate` emits the complete deterministic JSON result and fails if the
   snapshot reports incoherent ownership state.

Whole-project in-memory publication is implemented as follows:

1. `superi-project::ProjectDocument::new` accepts one validated editorial project and selected root
   timeline, calls the timeline compiler, derives deterministic project settings from the root edit
   rate, and retains the complete compilation with its provenance and editable graph at document
   revision zero.
2. `ProjectDocument::edit` requires the exact current document revision, clones one private
   candidate, and exposes ordinary editorial, settings, clip-mix, and graph mutation only through
   `ProjectDraft`.
3. Publication validates the selected root, graph identities, unique compiled roots, standalone
   names, shared project identity, exact project settings, authored clip-mix validity and clip
   membership, and exact editorial-to-compilation revision. A stale revision, failed closure, or
   invalid candidate publishes nothing; an unchanged candidate does not advance.
4. Successful changes advance once and replace one shared `Arc` state. Cloneable immutable
   `ProjectSnapshot` values preserve prior revisions for editor, script, headless, persistence, and
   API and engine consumers.
5. Intelligent or generated output has no hidden channel. It remains an ordinary typed parameter
   or node in a retained timeline compilation, ordinary editorial state, or an explicitly named
   standalone editable graph.
6. For later durable loading, `ProjectGraph::restore_timeline` recompiles trusted provenance and
   installs an externally decoded editable graph only when its deterministic graph identity matches
   the same project and root. `ProjectDocument::from_complete_parts_with_settings` then joins
   decoded settings and authored clip-mix state and validates the complete aggregate at the stored
   document revision without owning bytes, migrations, or file I/O.

Durable project settings use that same whole-project owner:

1. `superi-project::settings` owns exact schema `1.0.0` and permanent keys for timeline, color,
   audio, cache, proxy, and render behavior. Values use shared Boolean, integer, and text contracts
   without coercion.
2. Bounded ordered transactions rebuild and validate one complete candidate, including drop-frame,
   pinned color identity, cache-limit, proxy-quality, render-override, and conditional-key rules.
3. `ProjectDocument::execute_settings_transaction` reuses optimistic whole-project publication, so
   stale or invalid batches publish nothing and semantic no-ops advance no revision.
4. `superi-engine::project_settings` resolves validated values into existing exact subsystem types
   without rewriting authored state or reconfiguring a live owner merely by inspection.
5. `EngineCommandDispatcher` owns one attached `ProjectCommandHistory` around the project, routes
   inspection and transactions through that shared mutation owner, and emits one ordered full
   replacement event only for a semantic change. `superi-api::ProjectSettingsApi`
   exposes schema `1.0.0` methods `superi.project.settings.get` and
   `superi.project.settings.transaction.execute` plus event `superi.project.settings.changed`
   without depending directly on project.

Referenced-media paths use that same whole-project owner:

1. Timeline retains stable `MediaId`, opaque target text, expected and observed fingerprints,
   rejected candidates, and explicit online, missing, unverified, or mismatch state in canonical
   editorial serialization.
2. Project recognizes `superi.media-path.v1` relative and platform-qualified absolute targets.
   Portable relative paths use canonical slash components and resolve lexically only from an
   absolute owning project file path; unknown locators remain opaque and future versions fail
   explicitly.
3. `ProjectDraft::execute_media_command` addresses one `MediaId` inside an outer aggregate edit.
   `ProjectDocument::execute_media_command` wraps the same operation with a document revision fence.
   Both retain exact editable graph bytes while rebuilding checked compilation provenance for a
   semantic change, and accepted no-ops manufacture no editorial or document revision.
4. Canonical project database replacement already stores that timeline-owned meaning, so accepted
   paths, missing state, mismatch evidence, and stable identity round trip without new SQL fields or
   a competing media model.

Engine-wide project command history composes that owner without duplicating it:

1. `superi-engine::history::ProjectCommandHistory` exclusively owns one `ProjectDocument`, accepts
   revision-fenced typed apply, undo, and redo commands. Its mutation vocabulary wraps all three
   authored project media commands, project settings transactions, and one bounded compound
   transaction.
2. A successful semantic media, settings, or compound mutation records complete immutable before
   and after snapshots plus stable mutation kind. The default capacity is 64, the accepted maximum
   is 4096, and full capacity evicts the oldest undo entry.
3. Failed commands and semantic no-ops preserve undo and redo. A successful new branch clears redo,
   while undo and redo move an entry only after `ProjectDocument::restore_snapshot` accepts the
   complete target behind the exact current revision fence.
4. Restoration validates project identity and the full aggregate, then publishes selected old
   contents at a fresh monotonic document revision. Stale input, invalid state, empty branches, or
   revision exhaustion changes neither the project nor history.
5. `EngineCommandDispatcher` can attach exactly one project-history owner. Typed history execution
   and inspection expose the selected snapshot and branch metadata. Media and compound changes
   publish a correlated `ProjectStateChanged` event, while settings changes publish a correlated
   `ProjectSettingsChanged` event after the same event-capacity preflight. No-ops publish none.
6. A compound transaction applies one to 64 ordered root, timeline, graph, media, or authored audio
   actions inside one outer project edit. Timeline actions reconcile retained graphs through a
   three-way compile, every action validates the draft, and a late failure rolls back everything.
7. History stacks, capacity, and command metadata are session-local operational state. Existing
   schema-3 persistence durably stores only the selected project snapshot, and generic history API,
   CLI, scripting, automation, and logging adapters remain later work.

Stable whole-project serialization is implemented at the same owner boundary:

1. `ProjectDatabase::create` reserves a new path without overwriting, or `memory` creates an
   equivalent in-memory database. Both establish SQLite application ID `SUPR`, schema revision 3,
   semantic format `superi.project` version `1.2.0`, and exactly five strict tables.
2. Preparation serializes the editorial owner through the canonical timeline codec, the validated
   settings snapshot to bounded canonical JSON, every retained graph through the canonical graph
   codec in stable `GraphId` order, and authored clip-mix state through the strict audio codec before
   filesystem mutation. Rows retain component revisions, ownership, exact lengths, SHA-256 values,
   and a domain-separated project manifest over all version, identity, revision, settings, and
   ordered component evidence.
3. In-memory `replace` writes semantic rows in one immediate transaction and requires exact snapshot
   reload before commit. File-backed `replace` delegates to the same public `Save` command used by
   interactive, script, and headless callers.
4. `ProjectDatabase::execute_save_command` builds one complete schema-3 SQLite candidate in the
   destination directory, requires exact semantic reload and full integrity after candidate commit,
   closes the SQLite handle, and synchronizes the candidate before publication.
5. `Save` replaces the active file, `SaveAs` publishes and rebinds active identity at the commit
   point, `SaveCopy` publishes without rebinding, and `Backup` always requires an absent destination.
   Save-as and copy expose explicit require-absent or replace-existing collision behavior.
6. Replace-existing accepts only a regular validated Superi project and publishes by same-parent
   rename. Require-absent atomically claims the destination name without clobbering it. Active-path
   aliases, destination appearance or replacement races, partial candidates, and prepublication
   faults never become a successful save; postpublication faults report that the new file is already
   authoritative instead of claiming rollback.
7. `open_read_only` applies defensive, query-only connection policy and validates database identity
   and exact schema objects. File-backed `load` opens one short-lived read connection, pins a coherent
   transaction across every check, closes the handle, and returns no partial state.
8. Timeline owns `superi.timeline` meaning and strict `TimelineGraphValue` Serde, graph owns
   `superi.graph` meaning, audio owns `superi.clip-mix` meaning, and project owns settings plus the
   normalized container, manifest, aggregate reconstruction, active path, and file publication.
   Direct graph edits, settings, authored audio, document revisions, and project-relative meaning
   survive reload and save-as.
9. `ProjectDatabase::open` validates current schema 3 without mutation or migrates exact schema 0,
   1, or 2 through the contiguous 0-to-1-to-2-to-3 registry. Schema 0 first reconstructs and writes
   the frozen schema-1 representation, schema 1 derives deterministic settings from the selected
   root rate and writes frozen schema 2, and schema 2 adds canonical empty clip-mix state before
   writing schema 3 with exact snapshot equality, all inside one immediate transaction.
10. A wrong application, future schema, unsupported format, malformed component, or forced failure
   after schema rewrite leaves the source unchanged. Writable open reports its source revision;
   read-only legacy open requires migration instead of partially interpreting old state.
11. History stacks, recovery discovery and restoration, modified-since-open conflict policy,
   dirty-state hashing, and public database file API, CLI, and scripting remain later project
   checkpoints.

Deterministic autosave now composes the same project publication authority:

1. `superi-project::ProjectAutosaveController` binds to one `ProjectId` and accepts only typed
   configure, tick, save-now, prune, and inspect commands. A host supplies monotonic elapsed
   `Duration` values and immutable selected snapshots; the controller starts no clock or thread.
2. Configuration anchors a checked periodic deadline beneath one explicit existing canonical
   recovery root. Disabled and not-due ticks are successful no-work outcomes, large forward jumps
   publish at most once, and a backward time value changes neither schedule nor files.
3. A due changed snapshot or forced manual command chooses the next strictly parsed 20-digit
   generation and delegates one complete current-schema recovery point to
   `ProjectSaveCommand::Backup`. Bounded collision retries rescan without overwriting, and a failed
   publication leaves the deadline due for retry.
4. The managed namespace is one `project-<32 lowercase raw ProjectId hex>` child. Scanning
   revalidates that directory, recognizes only exact `autosave-g<generation>.superi` names, rejects
   managed symlinks or nonfiles before deletion, ignores foreign names and save candidates, and
   never opens retained database contents.
5. Automatic and explicit pruning sort only by numeric generation, retain the newest configured
   count, remove older regular files individually, never recurse, and report exact partial progress.
   A cleanup failure after publication preserves and identifies the new recovery point.
6. The engine history consumer passes selected snapshots after apply, undo, and redo through this
   surface and reopens exact equal artifacts. Engine production hosting, API, CLI, recovery choice,
   restore, and dismissal remain later work.

Project snapshot and legacy timeline graph media preparation are implemented at an engine-private
shared-resource boundary:

1. A caller supplies either one immutable `ProjectSnapshot` or one editorial project plus root
   timeline, together with the registry, operation context, fallback policy, and exactly one
   source plus decoder-stream request for each reachable media ID. Persistent project callers can
   construct each request with `MediaResourceRequest::from_project_media` rather than selecting a
   separate path or identity.
2. The project path clones the exact selected `TimelineGraphCompilation`, preserving published
   direct graph edits. The legacy path calls `superi-timeline::compile_timeline`. Engine traversal
   resolves the same nested closure to media IDs and rejects missing, duplicate, extra, empty, or
   duplicate-stream requests.
3. The project request constructor resolves recognized filesystem targets from the absolute
   `.superi` path, binds the stored `MediaId` and expected fingerprint, and rejects explicit missing
   state, opaque targets, and nondeterministic relative project-file context. Media I/O receives the
   resolved local `SourceLocation` but owns no persistent path syntax.
4. The engine binds persistent project fingerprints, performs one bounded content probe, opens only
   the selected source, verifies its returned identity, resolves complete stream descriptors, and
   constructs only the selected decoder backend.
5. Source and decoder selection evidence retains stable IDs, content confidence, probe bounds,
   fallback candidates, and fallback-use state. A selected backend failure is returned directly and
   never retried through those candidates.
6. The retained or newly compiled graph, all opened sources, and all live decoders publish together
   as one `TimelineResources` value only after a final operation check. Playback, render, export,
   A/V sync, and arbitration consumers remain later owners.

Proxy and optimized-media generation is implemented at an engine-private bulk-media boundary:

1. The caller binds authoritative media identity and revision to explicit purpose, quality, and an
   engine-derived fingerprint of the complete encoder configuration.
2. `superi-cache::DerivedMediaCatalog` returns an exact immutable artifact when one already exists.
3. On a miss, `superi-engine::generate_derived_media` rederives and verifies the settings, selects
   one primary registered encoder with fallback disabled, and drives caller-prepared decoded inputs
   through the codec-neutral lifecycle.
4. The engine retains every packet privately through flush and end of stream, hashes complete bytes,
   timing, keyframe state, and typed metadata, then asks the cache catalog to publish one immutable
   artifact.
5. Failure or cancellation publishes nothing. A prior complete artifact remains unchanged, while an
   exact miss exposes the authoritative original source instead of choosing stale or different-quality
   media.

The real default AV1 backend consumes this path in engine integration proof. The path does not
render or resize input, mux or persist output, choose playback proxies, mutate projects, or enter the
transport-neutral API.

Transparent proxy substitution is implemented at the same private bulk-media boundary:

1. The caller supplies the complete authoritative `SourceIdentity`, exact source revision,
   requested scheduler quality, fallback policy, immutable derived artifacts, and a lazy original
   opener.
2. `superi-engine::resolve_proxy_source` admits only proxy-purpose artifacts with exact source ID,
   source fingerprint, revision, known quality, complete packet stream and timebase, timing, and
   keyframe access.
3. The engine translates cache qualities one for one and delegates exact, lower-quality, stable
   cache-ID tie, unavailable, and source-only decisions to
   `superi-concurrency::DerivedMediaRequest::select`.
4. A selected artifact becomes a packet-backed `MediaSource` that retains the immutable artifact,
   preserves generated packet bytes and timing, supports deterministic keyframe-bounded seek, and
   exposes the authoritative original identity.
5. Source selection opens the original only then and rejects any identity mismatch. Source-only
   delivery therefore ignores even a valid proxy, while stale, mismatched, malformed, optimized,
   missing, or higher-only candidates fall back without changing authored meaning.

The real default AV1 generation path feeds this substitution contract in engine integration proof.
No production playback clock, export queue, project owner, or public API invokes the resolver yet.

Bounded edit and scrub warming is implemented at a cache-owned planning boundary:

1. A caller supplies one checked half-open timeline frame interval and one hard-limit policy.
2. Edit planning canonicalizes exact editorial boundaries, then ranks nearest valid frames across
   all boundaries independently of input order and duplicates.
3. Scrub planning compares two exact observations, caps the observed stride, favors predicted
   frames in the forward or backward direction, retains a smaller opposite tail, and alternates
   nearest neighbors when the scrub is stationary.
4. Every plan is clipped and hard bounded before work begins. Targets contain only exact timeline
   frame, reason, and rank, so the caller must map them to its ordinary graph request and complete
   cache scope.
5. The real graph and memory-cache integration proof shows warmed demand hits without node
   execution, changed media fingerprints recompute, and ordinary budget pressure preserves exact
   fresh results.

The planner owns no cache key, retained value, project mutation, source opening, proxy quality,
fallback, job dispatch, cancellation, or hidden history. No production editor, engine, playback,
API, or UI path consumes it yet.

The API-local revision begins at zero and increments only on a changed snapshot. The public schema
version is `2.0.0`; the permanent method and event names are
`superi.media.capabilities.get` and `superi.media.capabilities.changed`.

Canonical editorial control is also implemented at a bounded reference boundary:

1. `superi-engine::command::ScenarioEngine` validates fixed fixture identity and owns exact import,
   insert, trim, mirror, operation-log, undo, and redo state.
2. `superi-api::ScenarioApi` accepts one strict typed action command and projects complete public
   timeline, graph, implementation, operation, and failure state.
3. `superi-cli` resolves and digest-validates the repository source and derived expectation
   fixtures, executes the normalized fixed scenario through the API, proves undo plus redo recovery,
   and emits eight stage records with bounded monotonic timing and current-process resident-memory
   boundary samples.
4. The CLI verifies 48 frame identities, exact synchronized PCM evidence, timestamps, state digests,
   and target metadata. It distinguishes applicable expectation success from rendered pixels that
   are not evaluated and rendered audio that is not applicable to the video-only slice.
5. Six missing production owners are reported as stubs, and the CLI publishes a non-playable
   contract artifact instead of claiming `canonical.webm`.
6. Project expectation version 2 normalizes the canonical absolute source path to its stable
   repository-relative identity before hashing, and both hosted Rust build jobs validate every
   fixture, compile and test the supported `os-codecs` configuration, and execute this same
   normalized consumer path with accurate active-feature identity.

`ScenarioEngine` now reuses the same generic bounded immutable snapshot store as the production
project-history owner. Its fixed four-entry reference model remains separate from real project
state, while both paths share oldest-entry eviction and successful-branch redo semantics.

The independent audio processing graph is implemented below engine orchestration:

1. `superi-audio::graph::AudioGraph` owns audio-specific graph, node, and edge identities, source,
   processor, submix, auxiliary, and master roles, one exact sample rate, one positive process-block
   bound, and ordered editable DAG storage.
2. Edge insertion rejects missing endpoints, cycles, ambiguous ordinary inputs, duplicate masters,
   illegal direct, send, return, or master routes, and unequal ordered `ChannelLayout` values before
   mutation. No implicit channel conversion or resampling occurs.
3. Preparation selects one destination or the single master and its ancestors, computes stable
   processing order, resolves every input in edge identity order to an earlier node, and fallibly
   preallocates every interleaved f32 buffer. Borrowed input views expose current-block samples
   without callback allocation or copying.
4. `PreparedAudioGraph::process` requires `ExecutionDomain::Audio`, rejects rate, size, output,
   overflow, and continuity mismatches before running processors, then advances the next exact
   sample only after complete success.
5. `PreparedChannelMixer` converts explicit canonical layouts with a precomputed speaker or
   discrete matrix. Direct graph edges remain exact-layout only, and consecutive block timing is
   unchanged through the explicit converter node.
6. `ClipMixState` publishes complete controls and identity changes transactionally. Preparation
   resolves snapshot-wide solo and precomputes semantic routing and phase coefficients before the
   callback applies gain, exact linear fades, equal-power stereo pan, mute, solo, and phase.
7. `superi-audio::serialize` encodes authored clip-mix state as strict canonical revision-1 JSON,
   preserves each f32 by exact bits, binds the ordered payload to SHA-256, and rejects corruption,
   unknown fields, alternate encodings, duplicate identities, and bounded-input violations.
8. `superi-project` retains that authored state in every immutable snapshot and schema-3 audio
   component. Prepared processors, device state, queues, and callback resources remain absent from
   the aggregate and persistence.
9. Public crate integration tests use unity `SummingBus` processors to prove dry submix, parallel
   auxiliary send and return, stable identity-ordered summing, and one terminal master over
   consecutive 48 kHz stereo blocks.
10. `superi-engine::audio_mix` consumes real timeline edit outcomes against cloned project and mix
   state. It inherits right-fragment intent, transfers replacements, removes deleted identities,
   and publishes both revisions only after both validate.
11. The compound project command routes timeline, graph, media, audio, and root actions through one
   history unit. Public audio and engine contracts prove save and reopen, undo and redo, and audible
   adjacent-block continuity. No decoder, plugin host, or engine playback owner feeds the complete
   routing path.

Generic graph storage is implemented independently of that reference path:

1. `superi-graph::ids` exposes the official core-owned graph identity types.
   `superi-graph::value::GraphValue<T>` preserves exact domain payloads beside finite scalar,
   vector, color, matrix, Boolean, and bounded choice values. Expressions accept only the explicit
   scalar variant without coercion.
2. `superi-graph::dag::DirectedAcyclicGraph` stores caller-owned node payloads and typed port edges
   in ordered primary and adjacency collections.
3. Checked edge insertion rejects missing nodes, duplicates, self-loops, and transitive cycles
   before mutation. Stable Kahn ordering uses the smallest ready node identity.
4. Exact dirty-region sets preserve half-open coverage without bounding-box over-invalidation. A
   pure planner validates changed nodes, walks stable topological and edge order, maps dependency
   regions through a caller-owned edge seam, merges converging work, and excludes clean nodes. A
   semantic edit planner compares immutable revisions, expands parameter-driver dependencies, and
   propagates roots through both prior and current topology in stable identity order.
5. ROI requests are validated against one immutable editable snapshot, clipped to per-output
   regions of definition, and propagated only through connected ancestors in reverse stable
   topological order. Built-in full-frame, pass-through, and checked expansion behavior shares one
   path with validated custom per-input mappings.
6. ROI plans stamp graph identity and revision, return required nodes in dependency-first order,
   preserve exact irregular region sets, and intersect with invalidation plans without scheduling
   clean work.
7. Public integration tests consume the store, invalidation plan, ROI plan, and shared evaluation
   snapshot directly. Timeline consumes neutral values and editable storage, while effects consumes
   schemas, mutation, parameter evaluation, diagnostics, and headless compilation for its bounded
   CPU reference. No production engine, API, CLI, project, GPU catalog, or rendered path consumes
   them, so no runtime stage label changes.

Typed port validation is implemented beside storage and reused by editable graph transactions:

1. Registered `NodeSchema` definitions declare canonical names, exact `ValueTypeId` tags, and
   `Single`, `Optional`, or `Variadic` cardinality independently for inputs and outputs.
2. One pure validator canonicalizes typed binding groups, preserves variadic graph order, retains
   opaque evaluator-owned payloads, and returns structured input or output diagnostics.
3. A schema-level connection check accepts only an existing output to an existing input with exact
   type identity. Input and connection failures are user-correctable, while node implementation
   output violations are internal terminal failures.
4. The validation API remains evaluator-neutral and never inspects or coerces an opaque payload.

Editable graph transactions now integrate those neutral contracts without integrating a production
evaluator:

1. `EditableNode<T>` binds stable port and parameter instance IDs to every declaration in one exact
   immutable schema. Initial and replacement parameter payloads retain exact `ValueTypeId` tags.
2. `GraphTransaction<T>` carries one expected revision plus ordered add, remove, connect,
   disconnect, presentation reorder, parameter replacement, driver set, and driver clear mutations.
3. `EditableGraph<T>` applies the batch to a cloned candidate. Connect resolves `PortId` to
   `PortName`, reuses schema compatibility, enforces target cardinality, and then uses checked DAG
   insertion. Remove requires explicit prior disconnection.
4. Driver mutation resolves existing typed targets and explicit dependencies against the private
   candidate, stores direct links or bounded pure expressions in canonical target order, and rejects
   direct or transitive parameter cycles before publication. Referenced nodes require explicit
   driver cleanup before removal.
5. Any failure discards the candidate and preserves the revision. A successful nonempty batch
   publishes one immutable `Arc` snapshot and advances once; editor, script, and headless readers
   can share the exact same typed state and deterministic orders. Snapshot parameter evaluation
   resolves literals, lossless links, and finite expressions once per request in stable dependency
   order, with domain-owned numeric conversion for opaque payloads. Higher domains may project only
   reached undriven literals into another result domain while graph retains exact types, driver
   traversal, cycle invariants, and completion ordering.
6. Public graph integration tests, the native timeline compiler, and the effects authoring and
   control contracts are real consumers. Effects instantiates ordinary typed nodes, compiles
   reusable controls into normal parameter-driver mutations, and samples linked or parented curves
   in two role-labeled graphs. Engine, API, CLI, project, and product runtime paths do not yet import
   this effect transaction path, so no runtime stage label changes.

Editable property animation now composes with that neutral state boundary:

1. `superi-effects` authors one checked `AnimationCurve` over core-owned exact time. Fixed and
   roving keys retain inspectable timing intent beside outgoing linear, cubic, or hold interpolation,
   cubic Bezier easing, independent value tangents, and optional bounded expression source.
2. Curve construction derives distinct integer-tick roving times between fixed anchors. Exact
   evaluation clamps endpoints, preserves exact key values, interpolates the active segment, and
   then applies a shared scalar expression over only `time` and base `value`.
3. Immutable insert, replace, remove, expression, and uniform retime operations reconstruct checked
   authored state. Retime requires exact fixed-key mappings, recomputes roving positions, and
   inversely rescales per-second tangents.
4. The strict revisioned curve wire serializes authored state, not derived roving times, and routes
   reload through public validation. A real integration test declares the curve as an animatable
   effect parameter, instantiates the effect-authored `EditableNode`, serializes and reloads its
   graph, and proves canonical bytes plus identical samples.
5. `ParameterControlRig` names ordinary animatable parameters as reusable controls and compiles
   lossless links or bounded `parent` plus `local` expressions into one revision-bound graph
   transaction. Exact-time evaluation samples reached literal curves before the canonical graph
   driver traversal, preserving vector links while rejecting nonscalar expression inputs.
6. Cross-workflow tests apply one rig to timeline-role and node-graph-role graphs, obtain equal
   editor-script-headless samples, reload complete driver state through the graph document, and
   prove graph-owned cycle rollback. The same rig also compiles through a real built-in opacity
   state across two shared host payloads. These are substantive editable artifacts and graph
   persistence consumers, not a production timeline attachment, production spatial path, UI, engine playback
   path, project document, GPU execution, or rendered output.

Editable visual compositions now preserve layering and nested relationships on that neutral state
boundary without copying editorial timeline ownership:

1. `superi-effects` stores bottom-to-top generic visual layers with exact half-open active ranges,
   complete mask and effect payloads, optional same-composition parents, and exact layer-to-source
   time maps supporting forward, speed, reverse, freeze, endpoint hold, and explicit rounding.
2. `Composition` validates one local parent DAG and exposes immutable layer naming, range, source,
   payload, parent, time-map, collapse, isolation, insertion, replacement, removal, and reorder
   controls. `CompositionArtifact` canonicalizes reusable compositions and validates its root,
   nested references, source clocks and ranges, and the complete precomposition DAG.
3. `resolve_frame` emits deterministic bottom-to-top structural outputs. Pass-through collapse
   expands reusable child layers, while authored boundary preservation or a mask or effect
   `RequiresIntermediateSurface` rule retains one explicit nested boundary. Every output preserves
   its complete ancestor payload path, owning-composition times, mapped source times, and
   root-to-direct parent chains.
4. Immutable artifact edits advance a checked content revision. Its standalone revisioned wire
   rejects unknown fields, unsupported revisions, unbounded composition, layer, and remap sequences,
   invalid parents, and recursive nesting before publication.
5. A real animatable effect parameter and `GraphValue::Domain` consumer persist the same artifact in
   independent ordinary graphs labeled as timeline-role and node-graph-role workflows, reload
   canonical bytes, and produce equal structural frames. This is reusable visual composition state,
   not editorial nested-sequence mutation, a production graph compiler, GPU execution, or rendered
   pixels.

Editable spatial compositions now consume that structural owner without duplicating it:

1. Each visual composition layer carries one strict `SpatialLayer` payload with stable source image
   identity, 2D or 3D interpretation, animated anchor, position, XYZ Euler rotation, scale, opacity,
   and reconstruction choice. Composition remains authoritative for IDs, order, parenting, ranges,
   precompositions, collapse, and exact time remapping.
2. One `SpatialScene` per reusable composition retains an animated right-handed perspective or
   orthographic camera, bounded ambient, directional, and point lights, layer-stack or camera-depth
   ordering, and exact shutter endpoints plus sample count. Construction and reload reject missing
   scene coverage, mixed clocks, wrong component widths, invalid domains, and excessive collections.
3. Exact sampling starts from `resolve_frame`, samples every same-composition parent and nested step
   at its owning composition coordinate, multiplies private binary64 matrices root to leaf, and
   publishes inspectable world transforms, anchors, normals, camera depth, camera, lights, opacity,
   and shutter offsets. Camera-depth order is far to near with stable authored ties.
4. One graph-native definition exposes variadic image sources, one image result, and the complete
   artifact as one animatable domain parameter. Its unbounded time declaration is honest about
   authored shutter endpoints. Independent timeline-role and node-graph-role graphs reload canonical
   bytes and produce identical sampled and rendered results without graph learning spatial meaning.
5. A bounded CPU oracle projects source planes through perspective or orthographic homographies,
   applies deterministic diffuse lighting and opacity, composites premultiplied ACEScg pixels, and
   incrementally averages exact shutter samples. Fixed layer and total-evaluation ceilings plus
   `ImageLimits` bound work before pixels; production GPU playback, viewport, and export remain absent.

Editable animated masks now compose with the same effect and graph foundations:

1. `superi-effects` authors each stable closed contour from bounded six-component vertex curves over
   core-owned points, vectors, and exact time. Nonzero and evenodd fill rules, relative cubic
   handles, explicit closure, and immutable vertex topology remain directly inspectable.
2. Each mask carries animated feather radius, signed expansion, normalized opacity, and a
   hold-interpolated inversion toggle beside replace, union, subtract, intersect, or exclude stack
   behavior. Construction checks clocks and authored ranges, and sampling rejects expression or
   interpolation overshoot before publishing state.
3. Exact-time sampling exposes cubic segments and every control to a future rasterizer. The caller
   applies fill, expansion, and feather, then returns one normalized coverage per mask; the sampled
   stack applies inversion, opacity, and deterministic Porter-Duff soft-alpha equations in canonical
   order. No image, GPU resource, ROI, or pixel result is manufactured by this authoring layer.
4. Immutable fill-rule, vertex, mask-control, operation, and stack edits reconstruct bounded state.
   The strict revisioned mask-stack wire denies unknown and future fields and routes every nested
   curve, path, mask, and stack through checked constructors.
5. A real integration test declares `GraphValue<MaskStack>` as an animatable effect parameter,
   mutates it in independent timeline-role and node-graph-role `EditableGraph` values, links the
   complete stack through a reusable control rig, serializes and reloads both, and proves equal
   samples plus canonical bytes. This is workflow-neutral editable mask state, not a built-in node,
   rasterizer, production timeline attachment, or rendered effect.

Editable vector shapes compose through the same neutral state boundary:

1. `superi-effects` authors stable open or closed cubic paths from bounded six-component vertex
   curves over core-owned points, vectors, and exact time. Immutable vertex edits and exact retiming
   preserve topology and interpolation across every nested operation.
2. Optional fills retain nonzero or evenodd winding, scene-linear solid color or ordered linear and
   radial gradients, explicit spread, and opacity. Strokes retain paint, opacity, width, cap, join,
   miter, animated dash pattern, and dash offset.
3. Bounded repeaters retain held integer copy count, fractional offset, affine transform components,
   opacity endpoints, and above or below composition. Exact-time sampling publishes deterministic
   virtual copies and renderer-ready geometry without allocating pixels or GPU resources.
4. A strict revisioned `VectorShapeDocument` wire denies unknown or future state and reconstructs all
   nested curves and operations through checked constructors. Whole-document edits and retiming
   remain directly inspectable after reload.
5. A real integration test stores the complete document in `GraphValue::Domain`, mutates and links it
   through ordinary effect-authored nodes in separate timeline-role and node-graph-role graphs, and
   proves equal samples plus canonical bytes after reload. This is editable reusable state, not a
   rasterizer, production timeline attachment, or rendered effect.

Editable rotoscope propagation uses the same state boundary without giving a solver ownership:

1. `superi-effects` stores bounded non-overlapping spans on one exact core timebase. Each span keeps a
   stable ID, complete generic base mask, strictly ordered per-frame corrections, and separately
   inspectable derived propagation.
2. Forward and backward requests expose the base plus same-direction correction anchors and every
   non-authored target in exact traversal order. A tracking or local inference engine implements the
   public propagation hook and returns complete typed samples without hidden graph or mask state.
3. Result construction rejects partial, extra, reordered, or wrong-clock output. Atomic application
   also checks the source revision, span, direction, range, anchors, and targets, then replaces only
   derived samples on that side while corrections remain authoritative.
4. Immutable operations add, replace, and remove spans, bases, corrections, and derived results.
   Directional correction edits invalidate only affected propagation, preserving opposite-side work
   and the generic payload's mask layers and composition relationships.
5. A strict versioned bounded wire and ordinary effect parameter plus `GraphValue::Domain` consumer
   survive canonical graph reload. Mask geometry, rendering, a propagation solver, production
   timeline attachment, UI, and engine execution remain unimplemented.

Editable motion tracking uses the same ordinary state boundary:

1. `superi-effects` retains stable point, planar region, object region, and calibrated camera
   selections on one exact core timebase. Feature IDs, selected geometry, known noncoplanar world
   landmarks, camera intrinsics, reference models, observations, transformed regions, confidence,
   and camera poses remain inspectable typed state.
2. Each track keeps its authored reference and manual corrections separate from replaceable derived
   samples. Immutable edits advance one content revision, corrections remain authoritative, only the
   affected derived segment is invalidated, and each request starts from the nearest coherent sample.
3. The bounded CPU reference solver consumes explicit finite dense luma frames. It performs iterative
   local point registration, normalized homography fitting with deterministic residual consensus,
   two-dimensional object similarity fitting, or calibrated known-landmark camera-pose refinement.
4. Solver results retain the complete request provenance and are rejected when stale, partial,
   reordered, wrong-clock, nonfinite, or inconsistent with the requested selection. Atomic
   application never publishes a partially checked result.
5. A strict versioned bounded wire plus ordinary animatable effect parameter and
   `GraphValue::Domain` consumer survive canonical graph reload in independent timeline-role and
   node-graph-role graphs with equal complete artifacts.
6. This is editable reusable motion state with deterministic reference solving, not production frame
   decode, pyramid tracking, GPU acceleration, camera calibration, structure from motion, project
   persistence, timeline attachment, UI, cache, engine execution, or rendered pixels.

Editable transitions now reuse the same one-way timeline, effects, and graph boundaries:

1. `superi-timeline` owns transition IDs, adjacent timed endpoints, source handles, record intent,
   grouping and synchronization consequences, atomic invalidation, persistence, and neutral graph
   compilation. It does not import effects.
2. `superi-effects::transition` supplies stable `1.0.0` cross-dissolve and directional-wipe schemas
   with required `from` and `to` image ports, one `result`, animatable progress, and wipe direction
   plus softness. Catalog registration and instance binding reuse the ordinary authoring SDK.
3. `TransitionTiming` validates one exact edit clock and maps cut-centered handles to a half-open
   range plus clamped host progress without taking identity or edit ownership from timeline.
4. The effects reference compiler requires exact schemas, resolves graph-owned drivers, fingerprints
   semantic ports and every resolved transition parameter, requests the same region from both
   inputs, and evaluates premultiplied cross dissolves or four-direction display-window-based wipes.
5. Focused tests execute these nodes through `GraphEvaluationSnapshot`, prove exact endpoints, soft
   edges, tiled parity, introspection, cache-key changes, immutable old revisions, and equal output
   in independent timeline-role and node-graph-role graphs. Production timeline binding, GPU parity,
   engine registration, effects-to-project persistence integration, viewport, playback, and export
   remain absent.

Editable text layout uses the same state boundary without claiming a rasterizer:

1. `superi-effects` stores bounded UTF-8 content under complete style and logical paragraph spans.
   Persistent font asset references, OpenType features and animated axes, fill, opacity, tracking,
   baseline shift, measure, line height, indents, spacing, alignment, direction, and wrap controls
   remain inspectable and share one exact animation clock and interval.
2. Immutable text, style, paragraph, and whole-layer retime operations reconstruct checked state.
   Continuous and hold-discrete controls are checked at authoring and again after exact-time sampling,
   and the strict revisioned wire rebuilds every nested owner.
3. A caller resolves stable font IDs to exact local bytes. Pinned Skrifa and Swash validate and shape
   the selected collection face; no host font enumeration, fallback substitution, account, or
   network service enters the contract.
4. Style, script, and Unicode bidi itemization produces logical glyph clusters. Unicode Linebreak
   opportunities are accepted only at cluster boundaries, then bounded wrapping, visual bidi
   cluster ordering, paragraph geometry, alignment, and justification produce owned inspectable
   lines, runs, metrics, glyph IDs, source ranges, positions, and advances.
5. One full `GraphValue<TextLayer>` payload links losslessly through ordinary reusable controls in
   independent timeline-role and node-graph-role graphs, survives canonical graph reload, and lays
   out identically from the same font bytes. Text rasterization, glyph atlases, GPU resources,
   production timeline attachment, engine registration, UI, viewport, and export remain absent.

Versioned graph documents preserve that same editable state without claiming project persistence:

1. `serialize_graph` emits one deterministic `superi.graph` JSON envelope containing a canonical
   payload, explicit format and primitive revisions, and SHA-256 integrity.
2. The payload preserves complete schemas, typed node and parameter state, presentation order,
   edges, graph identity, and optimistic revision. Equivalent semantic state normalizes to the same
   bytes independently of insertion and JSON object-key order.
3. `deserialize_graph` rejects corrupt, truncated, unknown, or future documents, migrates the one
   supported legacy revision, and reconstructs state through existing schema, node, transaction,
   connection, cardinality, and cycle checks.
4. Independent editor, script, and headless-style tests load the document, observe equal snapshots,
   and evaluate equal results through the shared evaluator. `superi-project` stores canonical graph
   documents, including strict timeline-owned graph values, in schema-3 SQLite rows, canonicalizes
   supported legacy component documents during migration, reconstructs exact retained graph
   revisions, and atomically publishes those revisions through save, save-as, copy, backup, and
   deterministic autosave recovery points. Recovery discovery and restoration, database file API,
   CLI, and complete product runtime paths do not yet consume that container.

Missing plugin schemas now degrade that same editable state without creating a second model:

1. `superi_graph::missing::resolve_graph` compares each node's complete embedded schema with one
   immutable `NodeRegistrySnapshot` and retains the original `GraphSnapshot<T>` unchanged.
2. Exact identity and schema equality is available. An absent identity produces an unregistered
   placeholder, while different fields under the same identity produce an incompatible placeholder
   and fail closed rather than reinterpreting typed state.
3. Resolved nodes expose original schemas, ports, parameters, drivers, edges, and order beside the
   derived availability. Missing state never enters transactions, document bytes, or migrations, so
   users may inspect, edit, and resave the graph while a plugin is absent.
4. The shared evaluation gate returns one canonical `Unavailable` and `Degraded` result containing
   graph and registry revisions plus every blocker in stable node order. Registering exact saved
   schemas later restores evaluation without changing graph revision or authored meaning.
5. The effects OpenFX host is now a real consumer. Its discovered catalog retains every scanned
   exact schema for editing, while its active catalog publishes schemas only when the isolated
   plugin is Ready; disabled, faulted, and quarantined operations therefore use this unchanged
   missing-node path.
6. Public graph integration tests and the effects preset contract are also current consumers. Presets
   instantiate and resave the unchanged editable state while implementations are absent or
   incompatible, then recover when the exact saved schema returns.
7. `superi-engine::plugins::PluginSupervisor` owns native bundle discovery, strict bundle
   validation, launcher coordination, per-plugin containment, exact permission narrowing, active
   registry rebuilding, and one playback, rendering, and export resolution path above the safe
   effects-side `IsolatedOfxAdapter` and node-neutral graph contracts. Concrete platform process
   transport, bounded IPC, GPU handles, native OFX ABI adapters, and production factories remain
   separate launcher and adapter responsibilities.

Deterministic graph scheduling and lazy evaluation are implemented over caller-owned DAG payloads
with a bounded effects reference consumer but without a production engine catalog:

1. One request names a stored output endpoint, exact rational frame, and signed half-open pixel
   region. A node payload declares only the incoming stored edge requests needed for that output.
2. Discovery validates those routes, canonicalizes their order, and records only reached source
   endpoint, physical-frame, and exact-region keys. Equivalent physical times are one work key,
   while each distinct declared edge remains a semantic node input.
3. The planner publishes unique prerequisites and deterministic readiness batches. Dependencies
   occur in earlier batches, and equal-ready keys are ordered independently of graph insertion
   history or thread timing.
4. Pre-execution inspection binds each reached payload's exact schema, behavior, and canonical
   editable-state fingerprint to a versioned graph cache decision. Available keys include stable graph,
   endpoint, route, policy-scoped time and region, and complete upstream key lineage; disabled,
   nondeterministic, and dependency-blocked work stays explicitly non-cacheable.
5. Evaluation walks that exact schedule, evaluates identical work once per call, and returns opaque
   values, the schedule, and stable semantic completion keys. Diagnostic evaluation uses the same
   executor and pairs the unchanged result with monotonic planning, execution, and per-node timing.
   Timings do not participate in semantic inspection, result equality, or cache identity.
6. Cached evaluation accepts one caller-owned `EvaluationValueCache<V>`, checks the final key before
   node execution, and recursively stops at retained intermediate keys. Every adapter call receives
   stable graph identity, optional published revision, semantic graph lineage, and exact work
   identity. `superi-cache` memory and disk contexts add authoritative project, media, parameter,
   color, and render context, with host or device placement added for memory admission. Both scoped
   adapters derive a complete `FrameCacheKey` before either final or intermediate tier is consulted.
   Only successful cacheable work is offered for retention. Each admitted memory entry owns exact
   total and project byte and frame reservations, device entries also hold the shared GPU cache
   reservation, and a refusal selects an eligible scoped victim without changing the fresh evaluator
   result. Successful hits and
   insertions promote per-tier logical recency. Automatic pressure reclaims intermediate values
   before final frames and chooses oldest access within a tier; explicit management can also remove
   exact per-tier LRU victims. One graph edit invalidation atomically
   removes affected older entries and releases their reservations from both tiers, then fences late
   old-snapshot work while leaving unrelated semantic keys reusable. Later requests recompute exact
   evicted or invalidated results through the unchanged evaluator. Disk entries validate bounded
   versioned envelopes, caller-owned
   schema revisions, and payload digests before decode, publish through synchronized same-directory
   renames, and quarantine invalid bytes. A budget or persistence failure stores nothing or becomes
   a miss without changing the fresh evaluator result. Cache render orchestration derives the exact
   target frame key before dispatch, checks memory before persistent storage, promotes disk hits,
   and uses the same graph path for fresh work. Its bounded worker queue single-flights active exact
   frames and stages newly evaluated values until final cooperative cancellation, deadline, and
   progress checks pass.
   Memory inspection reports deterministic tier keys plus budget evidence, and exact clearing
   preserves graph revision fences. Persistent inspection classifies managed, quarantined,
   temporary, and unknown files without following links; clearing publishes an empty namespace
   before cleanup, and relocation publishes a complete renamed or destination-local staged copy
   before source removal. Derived inspection and clearing retain source, revision, purpose, quality,
   content, and byte evidence, after which exact lookup returns the authoritative original source.
7. `GraphEvaluationSnapshot<T, N>` retains one exact editable snapshot and uses a higher-tier
   `NodeCompiler<T, N>` to replace only node payloads while preserving graph identity, node IDs,
   edge routes, and checked topology. Each compilation receives the full snapshot, so authored
   parameter drivers remain visible. Scheduling, inspection, ordinary evaluation, cached
   evaluation, and diagnostic evaluation delegate to the same `LazyEvaluator` for editor, script,
   interactive, playback, export, and headless callers.
8. Public graph and effects tests prove linked and expression-driven parameter state enters
   runtime-node compilation, real effect and transition pixels evaluate through immutable old and
   new revisions, exact visual state changes alter diagnostics and cache identity, exact invalidated lazy
   work, equal role schedules, semantic inspection, cache keys, exact retained results, final-hit
   short circuiting, intermediate subtree pruning, immutable old revisions, fresh edited results,
   driver-expanded edit roots, selective two-tier cleanup, stale revision fencing, and contextual
   atomic compiler failure. Cache render orchestration, engine playback, and engine render-export
   are the first production role consumers of externally prepared evaluation snapshots, while effects is a deterministic
   headless CPU reference consumer with exact authoring, animation, and transition foundations. No production
   engine node catalog, API, CLI, or native GPU stage builds the canonical evaluation path, so
   `graph.evaluate` remains a disclosed stub in the canonical application slice.
9. Playback prediction validates an exact playhead, half-open bounds, and nonzero signed step, then
   emits at most 512 nearest-first critical, farther predictive, and trailing frame requests.
   `superi-engine::PlaybackPrefetcher` requires the playback domain, cancels the prior generation,
   and submits one playback-priority cache job without waiting. Each bounded frame evaluation uses
   the same snapshot and owned host cache identity. Polling exposes success, cancellation, or
   degraded evaluator failure without changing transport or final output.
10. Foreground playback accepts one prepared immutable graph, exact output request, complete scene
    cache identity, display transform, bounded audio producer and its device clock, lifecycle-owned
    worker pool, and bounded viewport sender. One playback-priority frame job evaluates through the
    same graph and cache, rejects time, scene-color, or alpha mismatch before retention, performs
    display color execution, and passes immutable media timing plus a distinct live deadline and
    duration to an engine coordinator over the audio-master or monotonic clock. The coordinator
    returns bounded hold, nominal or corrected presentation, explicit drop, and applied
    discontinuity recovery evidence without changing media timing. Viewport backpressure retains the
    exact payload and resolved presentation, so retry does not repeat a scheduler observation. The
    path remains nonblocking and leaves prepared source binding and native GPU submission to their
    separate owners.
11. `superi-engine::PlaybackTransport` composes foreground and prediction owners on the playback
    domain. Seek, scrub, pause, resume, frame step, signed rate, direction, and loop changes cancel
    stale generations, request callback-owned audio discard, and submit one protected exact frame.
    Checked frame-to-clock cadence uses fixed anchors and distinct deadlines. Optional late-frame
    policy skips only ordinary playing frames, protects user and loop intent, and forces visible
    progress at its positive ceiling. Immutable snapshots expose frame, prediction, viewport, and
    audio degradation. Render-export remains a separate exact-time consumer, and wire dispatch
    remains absent.
12. `superi-engine::EngineResourceArbiter` validates one shared managed-byte hard limit plus a
    protected floor and class hard limit for decode buffers, GPU payloads, caches, prepared audio,
    AI work, and exports. Playback, render, export, background, and recovery callers use the same
    serialized admission path. Exact noncloneable reservations release through RAII, while opaque
    resource bindings preserve the original value's timing, precision, metadata, color, and alpha.
    Under pressure, cooperative owners release their own reservations in fixed cache, AI, export,
    decode, GPU, and audio order without crossing another class's protected floor. Admission either
    commits exact class and shared accounting or returns a consumer-aware scheduling fallback with
    deterministic reclaim and shortage evidence. Lower cache, GPU, audio, media, AI, and export
    owners remain authoritative, and current orchestration paths must opt in by binding their live
    resources and installing reclaimers.
13. Render-export accepts exact acquired-source routes, a current lifecycle permit, one immutable
    graph snapshot and decoded-frame binder, explicit delivery and audio stage owners, the ordinary
    backend registry, and an export-priority operation. It seeks and reads complete packets, drains
    selected decoders, evaluates exact graph scene envelopes, invokes delivery color or audio
    processing, selects each encoder once, drains every encoder, validates timing interval unions,
    metadata, formats, precision, color, alpha, and graph identity, and resets all completed state.
    It publishes complete in-memory elementary packet streams only after every codec reaches end of
    stream and the permit remains current. Partial reads, semantic drift, cancellation, degradation,
    or codec failure publish nothing and trigger fresh-context reset recovery. The tracked entry
    point advances job progress at semantic transaction boundaries without changing publication.
    `ExportJobQueue<R>` retains unique logical jobs and older-only dependencies under explicit
    worker, pending, and retained bounds. EngineControl callers poll state, progress, failure context,
    dependencies, and typed results without waiting; pause and cancel settle cooperatively, while
    resume and permitted retry create fresh attempts. Recoverable failures remain unresolved so
    dependents can continue after successful retry, while terminal failure, cancellation, and
    dependency failure finalize deterministic downstream state. The real paired artifact contract
    prepares its executor through the typed runtime handle and routes submit plus poll exclusively
    through dispatcher commands before reading the retained result. Container muxing, persistence,
    native GPU readback, arbitrary stream counts, API, application integration, and
    crash-recoverable queue persistence remain separate owners.
14. `superi-engine::EngineCommandDispatcher` owns one typed in-process request boundary for
    canonical scenario transactions, lifecycle inspection and mutation, operation-labeled
    classified failure, exact recovery start, completion and reclassification, acknowledged sleep,
    wake, shutdown, restart,
    playback, rendering, or export work admission, exact interactive transport control, and attached
    project apply, undo, redo, and inspection. It also
    owns stable submit, poll, inspect, pause, resume, retry, one-job or all-job cancel, and remove
    commands over the canonical logical export queue. Scenario
    transactions are optimistic, bounded, ordered, atomic, one revision, and one undo unit.
    Transport commands cross a
    capacity-one nonblocking bridge from EngineControl to the real Playback owner, cannot be
    overtaken by another state mutation, and return complete replacement state with structured
    failure evidence when needed. Export submit and recovery attempts receive fresh lifecycle
    permits, while poll turns worker, progress, dependency, failure, and completion changes into
    revisioned full queue replacement events. The full dispatcher owns the canonical error
    coordinator beside lifecycle and emits independently revisioned replacement state containing
    coherent admission, active failures, bounded diagnostics, and the exact pending recovery token.
    Legacy failure and recovery commands route through the same coordinator. Successful state
    changes publish bounded ordered full replacement events. Project changes use the document
    revision for result and event correlation, preflight event capacity before mutation, and suppress
    events for semantic no-ops while retaining their typed result. The dispatcher also retains the latest
    completed playback and export replacement observations and exposes one read-only coherent
    integration validation snapshot that nests canonical introspection beside scenario, exact
    action, workflow admission, playback, and export state. Validation never polls an endpoint,
    waits for progress, advances a state revision, or creates a second mutable owner. `superi-api`
    projects the scenario transaction and event seam and the strict validation schema, but not the
    project-history vocabulary yet. `superi-cli` consumes the scenario
    seam for its canonical runner and the validation facade for exact deterministic `engine
    validate` JSON.
15. Invalidation-to-render orchestration, ROI-plan-to-evaluator binding, cache invalidation
    invocation, automatic capacity policy, external directory coordination, and production engine
    catalog wiring remain separate later checkpoints. Cache owns bounded outer job dispatch for
    background population without moving priority or worker ownership into graph.

The in-process engine request envelope, dispatcher, bounded event channel, playback-domain command
bridge, logical export controller, engine-owned typed project command history, canonical public
scenario transaction, and strict read-only integration validation query are implemented. No
project-history wire or API projection, subscription delivery, broad public editor transaction,
script runtime, production UI, extension host, or closed-tier runtime consumer is implemented.

### Documented target, incomplete

Repository contracts describe one stable public command and event seam shared by UI, CLI, scripts,
extensions, automation, and closed-tier clients. The canonical scenario transaction and ordered
replacement event implement the first public narrow slice. The engine now also provides the
production Rust project-history surface and correlated replacement events for current media and
settings changes. The settings facade consumes that owner, while generic public history adapters do
not yet. Broader engine transactions are intended to coordinate
project, timeline, graph, caches, undo, persistence, lifecycle,
playback, render, export, and event publication. Bulk frames, audio, packets, and GPU resources are
intended to stay behind that seam.

That target must not be read as current behavior. Timeline now owns foundational validated
editorial state plus selection, targeting, sync locks, linked selection, clip groups, exact clip
retiming, six primary operations, and ripple, roll, slip, slide, razor, trim, extend, three-point,
and four-point edits.
Timeline compilation now publishes native editorial state into the generic editable graph. Project
retains that complete compilation as ordinary revisioned document state, and engine can preserve the
exact published graph with opened sources and decoders as one preparation bundle. No production
engine catalog evaluates that compiled state. Effects can instantiate compatible shared processing
nodes, attach strict exact keyframe payloads, and evaluate them through its bounded CPU reference,
but no production timeline attachment closes the flow. Engine
foreground playback evaluates caller-prepared graph values, performs CPU display conversion, admits
prepared audio, and coordinates viewport delivery with the device clock under normal, late,
discontinuous, and recovered conditions. Engine transport owns exact interactive controls and
bounded ordinary frame dropping over that prepared path, but it does not bind the prepared source
bundle or timeline audio renderer. Engine render-export separately binds explicit acquired source
routes through decode, immutable graph evaluation, caller-owned delivery color or audio processing,
and encode with exact elementary-stream publication and recovery. It does not consume the
timeline-compiled runtime graph, mux or publish a container, or expose API control. No complete
edit-to-source-backed-playback or edit-to-export control flow exists.

`docs/vertical-slice.md` now defines the exact first control flow as scenario
`superi.slice.canonical.v1`: one immutable WebM and AV1 fixture role, one 24 fps video track, a
half-open middle trim, one typed horizontal-mirror transform node, explicit sRGB delivery, and eight
ordered stage records. It permits disclosed stubs only for contract conformance. Runtime
conformance requires every stage to use its production owner. The CLI now executes the complete
control sequence at contract conformance, with six stages explicitly reported as stubs. Report
schema 1.1.0 names timing and memory units, records before and after resident bytes for every stage,
and reports the largest resident value observed at those boundaries.

## Media ingest and codec flow

### Implemented components

`superi-media-io` owns the neutral values and lifecycle:

```text
SourceRequest
  -> bounded content probe and deterministic backend selection
  -> MediaSource and SourceInfo
  -> optional paired stream selection
  -> timed Packet values in decode order
  -> selected Decoder implementation
  -> VideoFrame or AudioBlock
```

Content bytes, not file extensions, are authoritative for source selection. A persistent project
`MediaId` is separate from source location. Container open computes a SHA-256 content fingerprint,
and relink accepts replacement location only when an expected fingerprint matches. Backend
selection orders candidates by tier, priority, and stable backend ID. Fallback candidates remain
explicit and are not silently tried after a selected open or codec creation fails.

The implemented source backends are Matroska/WebM, MP4/MOV, MXF, and WAVE/RF64/AIFF PCM. They
produce container-neutral streams and packets but do not decode. `superi-codecs-rs` provides the
ordinary priority-100 software decoders and encoders for AV1, FLAC, MP3, Opus, PCM, Vorbis, VP8,
and VP9. `superi-codecs-platform`, when enabled, contributes priority-200 host operations. Apple
uses VideoToolbox and AudioConverter, Windows uses discovered Media Foundation transforms, and
Linux uses VA-API. `superi-codecs-vendor`, when explicitly configured, contributes priority-500
decode-only source and codec adapters backed by separate worker processes.

Codec decoders receive elementary packets with exact timing, configuration, metadata, and stream
identity. They return safe CPU, external, or backend-owned frames through media-I/O interfaces.
Codec encoders receive decoded frames or audio blocks and return elementary packets. Containers do
not decode, and codecs do not demux.

The engine now composes caller-prepared decoded inputs with one selected encoder for proxy or
optimized-media generation. It validates exact purpose, quality, codec, timebase, representation,
color or channel meaning against the cache request, reaches real default AV1 encoding, and publishes
only complete elementary packets after end of stream. Cache identity preserves authoritative
source ID, content, and revision, and a missing or failed generation leaves that source as fallback.
The engine also adapts a selected complete proxy artifact to `MediaSource`, preserving packets and
authoritative source identity, or lazily returns a verified original source under deterministic
scheduler policy.

The default engine registry now includes all four in-tree container source adapters. Its resource
preparation path compiles one timeline graph, discovers the exact reachable linked-media set, binds
project fingerprints, probes and opens each source, maps explicitly requested stream descriptors to
decoder configurations, and constructs each selected decoder once. One returned owner retains the
compiled graph, live sources and decoders, and complete selection evidence for later playback,
render, and export consumers.

### Integration gaps

There is no muxer, export container writer, production image-sequence backend, multiple-stem stream
selector, source-to-generation decoder, or source-to-playback coordinator. Resource preparation
creates source and decoder owners but does not itself schedule packet flow, choose paired streams,
upload decoded output, evaluate graph nodes, or synchronize audio and video. Render-export now
consumes explicit acquired routes and schedules their packet, decode, graph or audio, and encode
lifecycles, but it does not choose paired streams from editorial intent or mux output. Derived generation does not
render or resize its caller-prepared inputs or persist its packets. Proxy resolution is caller-owned
and is not wired to playback or export orchestration.
Platform and vendor frames can be external or
backend-owned, but the engine upload path currently accepts CPU frames only. Higher-level decode
selection is expected to request a CPU fallback, but preparation currently selects by codec, tier,
priority, and stable ID without an output-storage constraint.

## Image, color, and GPU flow

The host image and decoded-video representations are intentionally distinct.

`superi-image` owns dense CPU `Image` values, native `ImageStorage` and `ImageAccess`, multipart
`StillImage`, eight still formats, numbered sequences, deterministic CPU operations, alpha
operations, thumbnails, waveforms, and reference comparison. Native access does not silently
repack into dense processing storage. Dense processing does not silently upload to a GPU.

`superi-color` consumes dense images for explicit input and output transforms. The input path validates
source family, transfer, primaries, range, matrix expectations, and alpha; decodes transfer;
converts primaries in binary64; applies an explicit gamut policy; and produces canonical
premultiplied scene-linear working storage. Canonical storage is RGBA binary16 with ACEScg as the
default working space, while numerically sensitive computation uses a separate RGBA binary32
value. LUT parsing and evaluation, HDR transfer functions, ICC profile validation and discovery,
and monitor-profile freshness checks are also implemented. Validated rule sets retain source roles,
select the first applicable ordered display view or an independent delivery output, and apply named
LUT looks in declared order. The output path validates one display or
deliverable target, converts linear working primaries, encodes SDR, HLG, or absolute PQ, preserves
premultiplied alpha and image identity, and emits authoritative full-range RGB binary32 output for
later storage conversion. An explicit luminance shoulder can preserve RGB ratios while mapping a
declared source peak before transfer encoding. A separate legal-range encoder unassociates the
output, rounds RGB to exact 8 through 16-bit legal codes, preserves alpha, and emits normalized
limited-range straight-alpha storage without choosing a YUV matrix.
Strict versioned JSON configuration resolves bounded named scene-linear working spaces, aliases, and
roles through the same `WorkingSpace` API. Serializable project settings pin one canonical space to
the config ID and normalized semantic SHA-256, rejecting semantic drift instead of silently changing
scene meaning.

The managed GPU wide-gamut path derives its WGSL matrix and gamut constants from the same binary64
reference transform, validates a canonical single-plane `Rgba16Float` source, allocates a managed
`Rgba16Float` destination, and returns an owned compute-pass batch or submitted fence. Source,
destination, bindings, pipeline, and command dependencies remain retained through submission. CPU
pixels are exposed only by an explicit readback owner used by export or reference verification.

The versioned color baseline now exercises that public CPU transform path with eight compact SDR,
Display P3, PQ, HLG, alpha, f16, and f32 images. It separately maps three ACEScg f32 payloads through
the public media-I/O image-sequence source with noncontiguous file numbers and exact 24000/1001
presentation timestamps. These are real consumer contracts over canonical raw fixtures, not an
engine, GPU, viewer, still-image decoder, or rendered golden-output path.

`superi-gpu` owns device identities, resources, memory budgets, pooled textures, decoded upload,
storage conversion, shaders, passes, the exclusive submission queue, fences, readback, native
surfaces, diagnostics, and device-loss reconstruction. Its storage converter may change packing,
numeric representation, matrix, range, subsampling, and alpha association. It refuses to change
primaries or transfer functions because those are color-management operations.

The implemented decoded-frame bridge is:

```text
superi-media-io VideoFrame with CPU storage and exact color pipeline
  -> superi-engine VideoFrameUploader
  -> superi-gpu DecodedFrameUploader
  -> pooled GPU plane textures
  -> UploadedVideoFrame retaining format, timing, metadata, color pipeline, and GPU ownership
```

The uploader preserves decoded bits, plane order, timestamps, duration, format, metadata, and the
complete image-owned color pipeline. It
uses direct row writes when compatible and a tight CPU repack otherwise. Logical initialized
texture extent remains distinct from aligned physical allocation extent. Pooled allocations and
all command dependencies must remain retained until the matching fence retires.

An adjacent metadata-only path carries exact source color tags, ICC bytes, named-space identity,
working and current spaces, and ordered input or creative transforms from media into graph,
timeline, and complete cache identity. Engine render metadata derives independent display and
delivery branches by appending a correctly typed terminal stage without mutating cached scene state.
This path does not execute transforms or render pixels.

No implemented engine path sends `UploadedVideoFrame` into graph evaluation, pixel color processing,
cache storage, playback, display, or encode. Official graph identifiers, schema registration, immutable
discovery, generic graph topology storage, typed input and output bindings, and schema-level
connection compatibility, a schema-bound editable graph, atomic mutation, and a caller-owned lazy
evaluator plus snapshot-owned typed parameter links and bounded expressions exist. Effects also
owns exact editable animation curves, strict visual composition artifacts, animated mask stacks,
exact-frame rotoscope artifacts, complete point, planar, object, and camera tracking artifacts, and
styled text layers with inspectable glyph layout that survive generic graph parameter serialization
through animatable authoring definitions, but none is
connected to a production runtime node. Visual composition resolution retains nested paths and time
maps structurally without applying transforms or pixels. A role-neutral
evaluation snapshot compiles editable instances into caller-owned evaluator payloads without
changing topology. Effects implements that seam for versioned visual schemas, bounded CPU reference
images, and CPU text layout metadata, but no production engine catalog connects it to a GPU value.
Color input, output,
LUT, and rule transforms remain CPU implementations and
have no graph-visible node catalog. A GPU wide-gamut transform exists as a direct public surface,
but no engine or graph consumer composes it with the complete display, delivery, ICC, viewport, or
export pipeline.
`MonitorAwareViewport` prevents stale-profile presentation but does not color-convert a frame.

GPU readback is explicit and limited to export or thumbnail storage bytes. It performs no color
conversion, swizzle, encoding, or resize. Image preview and CPU reference validation require an
explicit readback by a GPU owner before they can consume the result.

## Concurrency and operation flow

`superi-concurrency` supplies reusable execution and liveness mechanisms:

- Seven domains identify UI, engine control, playback, render, audio, background jobs, and GPU
  submission, with explicit blocking and allocation policy.
- A bounded worker pool combines local queues, stealing, deterministic 8:4:2:1 priority service,
  cooperative cancellation, deadlines, progress, typed completion, and panic containment.
- Fixed-capacity handoffs never drop a saturated payload. They return ownership to the producer for
  explicit retry and keep route capacity independent.
- `DomainOwned<T>` keeps mutable state in one execution domain. `SnapshotPublisher<T>` publishes
  immutable generation-tagged `Arc` snapshots for cross-thread readers.
- Playback clocks recompute from monotonic or audio-sample anchors. The A/V scheduler produces
  wait, present, drop, or rebase instructions but never performs them. Engine foreground playback
  is the current concrete consumer that applies a requested rebase and retains resolved presentation
  evidence across viewport backpressure.
- Lifecycle coordination uses revisioned requests and exact participant acknowledgements.
  Liveness probes and explicit wait-resource ownership produce starvation and deadlock findings.

Media and codec operations use `superi-media-io::OperationContext`, which carries priority,
cancellation, and an optional monotonic deadline. The vendor adapter keeps that context active
while waiting on process I/O. Platform and Rust codecs check it at public boundaries and selected
loops. Concurrency jobs use their own `JobControl` and require the job closure to call checkpoints.
Both models are cooperative. Neither can preempt a blocking operating-system call, native codec
call, or closure that omits checkpoints.

These mechanisms are not yet a complete composed runtime. Engine proxy resolution consumes the
derived-media selection policy, and engine playback consumes playback domain ownership, bounded
workers, playback priority, cooperative cancellation, progress, and nonblocking completion for
predictive cache population. Foreground playback adds one exact frame job, audio-master A/V
coordination with bounded correction and discontinuity recovery, monotonic fallback, and a bounded
lossless viewport handoff. Engine transport composes cancellation, clock reanchoring, distinct frame
deadlines and live durations, prediction, and handoff retry into exact interactive control. Engine
lifecycle keeps authoritative state on EngineControl,
composes one acknowledged lifecycle participant, publishes immutable generation-tagged snapshots,
and exposes one lock-free signal. Its exact action tokens sequence shared-state, project, device,
playback, rendering, and export initialization, recovery, reverse teardown, and restart without
performing subsystem work inline. The lifecycle plan also names project and device owners,
quiesces dependents in reverse order for sleep, retains authoritative project state, releases
volatile device and workflow resources, and revalidates them in forward order after normal or
critical wake. The
dispatcher-owned EngineControl error coordinator captures core-owned
source chains, contexts, safe projections, and stable disposition fields before moving each error
into the lifecycle. It accepts classification-specific recovery intent, rejects stale sequence and
attempt tokens, reclassifies failed recovery, retains fixed-capacity diagnostic history without
copying workflow readiness, and publishes complete independently revisioned recovery state.
The separate engine resource arbiter serializes shared byte admission and
cooperative reclaim without copying lifecycle readiness or lower allocator ownership. Reclaimer
callbacks run outside its accounting lock, direct recursive admission is rejected, and finite
pressure remains a typed scheduling outcome unless a caller explicitly reports a persistent failure
through the error coordinator. Render-export consumes that control plane by requiring the same
current export permit before codec creation and artifact publication. The engine export queue composes that
transaction with export-priority bounded workers, dual cooperative controls, semantic progress,
typed completion, retained logical identity, dependency history, and fresh resume or retry attempts.
The queue remains EngineControl-owned. Its stable actions and automated observations route through
the dispatcher, fresh submit and recovery attempts receive current export permits, generic executor
bindings plus typed results remain runtime local, and explicit draining shutdown must run on a
blocking-safe domain after every logical job is final. The dispatcher rejects export teardown
completion until that condition is visible, so all-job cancellation and polling precede worker
joining. Lifecycle still does not acquire codec resources. Audio enforces the
platform-owned audio domain for fixed prepared
graph processing and exact callback scheduling. Device input uses independent preallocated capture
and monitoring queues with exact physical sample coordinates. Device output uses a preallocated
lock-free queue and publishes the audio master clock consumed by foreground playback. Cache's
background render queue
constructs a bounded pool, submits cache-kind work at background priority, carries `JobControl`,
exposes typed completion and snapshots, and layers exact-frame single-flight over dispatch. Graph
has no direct production concurrency consumer. Playback and transport accept an externally
lifecycle-owned pool, a caller-created handoff, and a caller-paired audio producer and device clock.
Lifecycle does not yet construct or drive those owners, and no engine owner binds scheduled source
slices to the prepared audio graph or a native device setup. The `submit` module is a placeholder. A
contract test hosts the real non-Send `GpuSubmissionQueue` inside the GPU submission domain, but no
engine owner wires that pattern into playback or render.

## Engine, API, CLI, and tool roles

`superi-engine` is the intended integration owner. It implements an engine-wide typed command
dispatcher, atomic revision-fenced canonical scenario and project settings transactions, full-state undo plus redo,
bounded revision-fenced project command history over real project media and settings mutations,
monotonic whole-project undo plus redo, bounded ordered replacement events, dispatcher-owned logical
export commands with revisioned full
state and fresh recovery permits, codec registry assembly, deterministic media capability
introspection, complete read-only engine health, workflow, failure, recovery, and optional resource
introspection, coherent integration validation layered on that same state, authoritative project
attachment and settings resolution,
bounded cross-domain playback command execution, and CPU-decoded frame upload, plus codec-neutral
proxy and optimized-media packet generation and
transparent proxy or original-source resolution, playback-domain predictive cache population, and
transactional source and decoder preparation from either exact immutable project state or a legacy
direct timeline compilation. It evaluates exact foreground
graph values through shared cache retention, performs CPU display color execution, admits bounded
audio, coordinates viewport delivery with the shared clock through explicit wait, correction, drop,
and rebase outcomes, retains frames plus resolved presentation through backpressure, and owns exact
seek, scrub, pause, resume, frame step, signed-rate, direction, loop, and bounded drop policy. It
also composes exact acquired-source reads, decode, shared graph evaluation, caller-owned delivery
color and audio stages, deterministic encoder selection, complete codec drains, strict semantic
validation, reset recovery, and all-or-nothing elementary packet publication. It atomically
coordinates production timeline edit batches with audio-owned clip mix intent. Its bounded logical
export queue adds immutable dependencies, nonblocking progress and result observation, cooperative
pause and cancel, classified retry policy, and fresh recovery attempts around the same real
render-export transaction. Its real paired export contract prepares the typed artifact executor,
submits and polls only through dispatcher commands, observes full replacement events, and reads the
retained artifact from the runtime handle after completion.
It owns a canonical lifecycle control plane that sequences shared-state, project, device, playback,
rendering, and export subsystem actions, publishes one immutable health and admission snapshot,
isolates recoverable degradation, quiesces in reverse order for sleep, revalidates in forward order
after wake, rolls back failed startup, preserves dependency-safe reverse teardown, and restarts with
a fresh lifetime. Its EngineControl error coordinator adds monotonic failure
identity, bounded actionable diagnostics, separate user-safe projection, explicit retry,
continue-degraded, user-correction, and restart dispositions, and exact recovery completion or
reclassification while preserving lifecycle as the only admission authority. The dispatcher owns
that coordinator, exposes classified report and exact recovery commands, and routes legacy failure
and recovery variants through the same validation path.
Its shared finite-resource arbiter adds exact protected and hard class budgets, RAII reservations,
fixed-order cooperative reclaim, opaque resource lifetime binding, deterministic snapshots, and
consumer-aware fallback across decode, GPU, cache, audio, AI, and export workloads without replacing
their subsystem owners.
The dispatcher routes current scenario, lifecycle, sleep, wake, shutdown, restart, classified
recovery, work admission, interactive transport, logical export, attached project history, and
project settings, and integration validation behavior. Project history and settings use one attached
authoritative `ProjectDocument`; the canonical scenario command model remains a reference boundary,
while project history delegates to real project media and settings owners. General timeline, graph,
audio, and compound project commands remain unwired. Prepared resources, foreground playback, interactive
transport, and render-export do not yet form one source-backed broad public control flow.
Variable-rate decoded audio, native GPU presentation and readback, container muxing, file
publication, and persistent export recovery remain absent.
Nodes remain an explicit placeholder. Validation, plugin discovery, and supervisor coordination are
substantive, while concrete platform transports and native OFX adapters remain absent.

`superi-api` is the stable public facade. It keeps implementation types private and exposes strict
versioned media capability, complete engine introspection, integration validation, and project
settings records plus the fixed canonical scenario action, optimistic ordered scenario and settings
transactions, complete state projections, and matching full replacement events. Validation nests
canonical introspection, which preserves workflow readiness and only reviewed user-safe failure
data, then adds exact action and endpoint evidence. Project settings retain complete project-owned
scalar meaning through engine re-exports without a direct project dependency. It has no wire
transport, database file command set, or broad editorial command set.
The engine's broader typed project-history results and replacement events are not yet projected by
this facade.

`superi-cli` is a binary boundary, not a library. It accepts the exact normalized slice command,
exact `engine validate`, help, and version. It validates repository fixture authority, drives revision-fenced `ScenarioApi`
transactions and verifies their events, writes the strict
schema 1.1.0 report with all-stage timing, resident-memory, and versioned expectation evidence, and
publishes a non-playable contract artifact through collision-safe paths. Its validation command
uses the API-owned fresh-engine helper and prints the strict immutable projection without importing
engine or concurrency directly.
Its project expectation
digest is portable across checkout roots, while strict undo and redo comparison and reported media
paths remain unchanged.

`superi-fixture-tool` is a repository utility, not an engine component. It validates schema,
identity, provenance, lineage, payload ownership, byte counts, hashes, path safety, and unmanaged
files under `open/test-fixtures`. Validation is offline and read-only. Its deterministic video
command creates an absent output directory containing all 207 current pixel-format and
standard-frame-rate cases, a fixed catalog, raw payload, and exact manifest. Its audio command
creates three WAVEFORMATEXTENSIBLE PCM16 files covering 44,100 Hz stereo, 48,000 Hz 5.1, and 96,000
Hz 7.1 with exact sample timing, channel masks, synchronized signal boundaries, and integer-only
waveforms. Its timing command emits five fixed CFR, VFR, drop-frame, gap, and reset cases with 18
samples and explicit continuity segments. Its color command emits eight deterministic SDR,
wide-gamut, HDR, alpha, high-bit-depth, and sequence images with two strict catalogs and one 448-byte
sample payload. Its media-error command emits four fixed PCM container
cases for malformed, truncated, unsupported, and post-open partially readable behavior, with one
strict outcome catalog. Its OTIO command creates two native JSON
timelines and an expectation record from fixed Rust values, including the first slice projection,
clips, gaps, transition relationships, owner-relative markers, nesting, two linear rate changes,
stable metadata IDs, and preserve plus diagnose contracts for unsupported effects. All generators
refuse overwrite. The tool does not execute recorded commands, prove repository-history
immutability, or verify legal and semantic
claims inside arbitrary free-form provenance fields.

The same utility exposes schema-versioned, read-only golden verification for frame, audio, timeline,
and project outputs. Frame and audio envelopes compare exact payload bytes together with layout and
meaning metadata, while timeline and project envelopes canonicalize object-key order without
changing array order or scalar values. Verification reports expected and actual SHA-256 evidence,
never writes expected files, and provides no automatic update or bless path.

`superi-dependency-check` is also a repository utility. It reads the locked workspace graph offline
and fails when a runtime crate adds an unreviewed normal, build, or dev-only internal edge, or when a
new runtime crate has no explicit policy. The structure guide and executable policy are reviewed as
one architecture contract. Current reviewed API dev edges support media registry fixtures and the
EngineControl ownership needed by the real introspection contract, while synthetic policy tests
prove neither edge authorizes a production dependency. The reviewed project-to-audio edge carries
authored state downward into persistence, and a focused policy test rejects the reverse direction.

`superi-boundary-tool` is a dependency-free repository utility, not an engine component. It scans
Cargo and Rust source deterministically, rejects forbidden network clients and direct socket APIs,
rejects supported open-to-closed import routes and symlinks, and runs before each locked hosted
workspace build as well as through the canonical workspace test gate.

`superi-test-report` is an offline repository utility that validates explicit schema-versioned lane
evidence and derives canonical status, summary counts, performance regressions, golden mismatches,
flaky tests, and platform gaps. It retains retry and artifact evidence, creates missing-suite gaps,
writes valid blocking reports before returning failure, and refuses to replace an existing report.

## Shared invariants

The following constraints cross multiple modules and should be preserved together:

- Open and closed dependency direction is one way. Open behavior cannot require closed code,
  accounts, remote services, or a network.
- Shared identifiers, exact timebases, half-open ranges, stable codes, color and pixel tags, channel
  order, error categories, recoverability, and primitive serialization are owned by `superi-core`.
- Project identity is separate from replaceable media location. Content fingerprints protect
  relinking, while metadata and source timing remain attached to the artifact that produced them.
- One `ProjectDatabase` owns active file identity and every save publication. A file operation first
  reconstructs and validates the complete supplied immutable snapshot in a private same-parent
  candidate, then publishes through explicit replace-existing or require-absent behavior. Save-as
  rebinds at the publication commit point, copy and backup do not rebind, prepublication failure
  preserves prior authoritative state, and any fallible postpublication result must identify the new
  destination as already published.
- Autosave reuses that database and Backup authority rather than defining another format or active
  file owner. Policy and time anchors remain session-local; completed recovery points contain full
  current-schema editable meaning. Scheduling uses caller monotonic time, retention uses strict
  numeric generations, pruning touches only preflighted regular managed files, and recovery
  discovery and restoration must validate artifact contents through the database owner.
- Project settings are authoritative whole-project state under the same optimistic revision as
  editorial and graph meaning. Project owns keys, defaults, candidate validation, and persistence;
  engine resolves existing subsystem types; API exposes only strict shared values and complete
  replacement state.
- Audio project settings preserve an exact sample timebase and ordered channel layout. They do not
  synthesize routing, resample authored media, reinterpret channel meaning, alter synchronization,
  or claim live device reconfiguration merely by being inspected or persisted.
- Timeline media organization retains stable bin and smart collection identities. Manual bin
  membership and dynamic query results never replace clip `MediaId` links, and mismatched relink
  candidates retain evidence without replacing the active target.
- Deterministic ordering is explicit. Stable backend IDs break selection ties; ordered maps and
  sets stabilize public snapshots, fixtures, diagnostics, and validator output.
- Engine capability and health introspection is read-only. Workflow availability comes from the
  canonical lifecycle admission path, independent owner revisions remain visible, and raw failure
  messages, sources, contexts, internal identities, and recovery tokens remain private.
- Editable graph state has one optimistic revision and immutable shared snapshots. A nonempty
  transaction publishes exactly once only after every ordered mutation passes, while stale or
  failed batches preserve the prior state and revision. Presentation order never replaces DAG
  processing order.
- Parameter drivers are ordinary typed editable graph state in canonical target order. Every
  dependency is explicit, direct links preserve exact types and payloads, expressions are bounded
  and pure, parameter cycles fail before publication, and all caller roles evaluate one immutable
  snapshot through the same request-local result path.
- Timeline selection, per-track targeting and sync locks, linked selection, and clip groups publish
  inside the same revision-checked project transaction as clip source and record state. Stable
  surviving identities retain intent, direct selection bypasses relationships, and sync-sensitive
  track resolution preserves timeline layer order.
- Multicam source timelines own ordered stable angle identity, synchronization provenance,
  metadata, and local source membership. Ordinary nested target clips own independent gapless
  switch and audio intent, resolve through both clip time maps exactly, and inherit state through
  structural fragment and replacement edits inside the same project transaction.
- Plugin availability is derived from immutable graph and registry snapshots, never persisted into
  authored state. Only an exact saved schema definition is available; absent or conflicting schemas
  retain typed placeholders and produce one stable degraded evaluation result for every caller.
- Graph ROI derives exact connected work from one immutable snapshot, current per-output domains,
  schema behavior, and stable typed edges. Plans retain graph identity and revision, never mutate
  state, and cannot be treated as cache-generation ownership.
- Runtime-node compilation receives that complete immutable graph snapshot, including parameter
  drivers, then preserves checked topology and delegates every caller role to one lazy evaluator.
  Retained evaluation uses one explicit caller-owned adapter and only complete semantic cache keys;
  final and intermediate hits cannot change authored state or result meaning. Cache-local LRU
  eviction changes only retained availability, so victims deterministically recompute without
  changing project or output meaning.
- Cache render jobs derive the complete requested frame identity before bounded dispatch. Memory
  and persistent hits retain exact meaning, persistent promotion changes only availability, one
  queue single-flights each active exact frame, and cancelled or timed-out fresh work publishes no
  staged final or intermediate values.
- Semantic edit invalidation compares immutable revisions, expands changed parameters through both
  driver graphs, and propagates roots through both pixel topologies. The cache applies that plan to
  both tiers under one lock, preserves unaffected semantic keys, and rejects affected work older
  than the graph and node revision fence.
- Capability declarations are metadata, not proof that a factory or every declared format can run.
  Introspection must not instantiate codecs or sources.
- Backend fallback is explicit. The registry, platform adapters, and vendor workers do not silently
  switch implementation after the chosen operation fails.
- Logical export jobs reserve identity for their complete retained lifetime and declare only older
  retained dependencies. Their worker, pending, and retention bounds are explicit; EngineControl
  observation never waits; pause and cancel are cooperative; resume and allowed retry use fresh
  controls and progress; and only explicit blocking-safe shutdown joins workers.
- Retryable, degraded, and user-correctable export failures retain actionable context and leave
  dependents waiting for recovery. Terminal failure, cancellation, and dependency failure publish
  final dependency history, and no raced or partial result may become observable.
- Engine media preparation binds one retained timeline compilation to the exact reachable source
  request set and explicit decoder streams. Project fingerprint conflicts, opened identity drift,
  missing streams, cancellation, and selected backend failures abort the whole unpublished bundle;
  recorded fallback candidates are evidence rather than exception retries.
- Derived media binds exact source identity and revision to purpose, quality, and complete canonical
  encoder settings. Generation publishes only after end of stream and complete packet hashing;
  failure retains prior complete media or the original source fallback. It cannot change project,
  graph, source, or final-render meaning.
- Cache warming ranks only exact in-bounds timeline frames under a hard policy limit. It does not
  derive a cache key, store a value, choose source or proxy quality, or alter fallback. Each caller
  must use the ordinary revisioned graph request and complete cache identity, so stale, evicted, or
  skipped speculative work remains a transparent demand miss.
- Pixel storage, alpha association, color interpretation, dimensions, timing, and buffer ownership
  are separate contracts. Constructing valid metadata does not prove a codec, color transform, GPU
  operation, or output supports it.
- Color tags do not execute transforms. Input, working, display, and delivery transforms require
  explicit owners. GPU storage conversion must not silently change primaries or transfer.
- GPU device identity and generation scope every managed object. Old, foreign, or recovered-device
  resources cannot be mixed. Submission retention must outlive fence retirement.
- Bounded allocation, queue capacity, pressure, cancellation, and backpressure are explicit at each
  implemented boundary. A local bound must not be generalized into a global process-memory claim.
- The engine shared resource envelope counts caller-declared managed payload bytes above lower
  subsystem budgets. It never replaces cache or GPU limits, audio callback-safe queues, media value
  ownership, AI ownership, export scheduling, or lifecycle admission. Every class has one protected
  floor and hard limit, unused floors may be borrowed, and exact noncloneable reservations are the
  only accounting authority.
- Shared reclaim is cooperative and deterministic. Other classes retain their protected floors,
  callbacks release their own reservations outside the accounting lock, measured counter change is
  the only progress evidence, and finite pressure selects a semantic scheduling fallback without
  altering timing, precision, metadata, color, alpha, or publication meaning.
- A request larger than its empty shared or class ceiling cannot trigger reclaim because releasing
  valid work cannot make that request admissible.
- Canonical slice instrumentation refreshes only the current process exactly twice per stage. Its
  largest observed boundary value is not a continuously sampled intra-stage peak or soak result.
- Cancellation and deadlines are cooperative. A single blocking foreign call remains a latency
  boundary even when surrounding loops poll correctly.
- Alpha is not silently discarded. Codec and conversion paths reject unsupported alpha, and image
  operations distinguish color channels from auxiliary channels.
- Errors retain category, recoverability, component, operation, and contextual detail. Raw
  diagnostic content is not user-safe by default; presentation must use the safe projection.
- Unsafe Rust is denied by default. Narrow native boundaries require local safety reasoning,
  ownership proof, audit inventory updates, and target-specific verification.
- Public bulk media stays out of the transport-neutral API. The API exposes control and metadata,
  while packets, frames, audio, textures, and command buffers remain behind engine boundaries.

## Test and verification strategy

Implemented modules primarily use public integration contract files rather than broad end-to-end
application tests. `superi-core`, `superi-graph`, `superi-image`, `superi-media-io`, `superi-codecs-rs`,
`superi-codecs-vendor`, `superi-color`, `superi-concurrency`, `superi-gpu`, and the implemented
engine and API slices all have focused contracts around their public values and lifecycles.
Container-to-codec tests and engine capability tests provide selected cross-crate composition. The
derived-media engine contracts additionally drive the real default AV1 encoder through complete
packet publication and deterministic reuse, then exercise exact or lower-quality transparent
substitution, replacement, generated packet reads, keyframe seek, strict freshness, and verified
original or source-only fallback. They do not claim rendering, muxing, persistence, playback-clock
integration, or container delivery.
Engine export-job contracts compose the real bounded pool, progress, control, completion, and
dependency surfaces into one nonblocking logical scheduler. A paired real render-export consumer
proves semantic progress reaches exactly 11 units before its typed elementary-stream artifact is
retained; separate queue contracts prove pause, resume, retry, cancel, all recoverability classes,
terminal propagation, safe removal, domain enforcement, and shutdown.
The engine resource-arbitration contract binds a real high-precision decoded frame without changing
timing, metadata, color, precision, or straight alpha. It also proves complete configuration, exact
RAII release, protected floors, fixed cooperative reclaim, classified callback failure, semantic
fallback for all six resource classes, class-ceiling recovery, hard limits under concurrent callers,
recursive callback rejection, and impossible-request refusal without eviction. Lower subsystem
integration remains an explicit caller action.
The engine and API introspection contracts compose the real registry, dispatcher, lifecycle, error
coordinator, and resource arbiter. They prove canonical normal admission, playback-only,
rendering-plus-export, and export-only degradation, visible recovery progress, restored readiness,
independent public revisions, strict replacement events, unchanged scenario and event state, and
safe JSON that excludes raw diagnostic details.
The project, engine, and API settings contracts compose one real project document, schema-3
database, typed resolver, full dispatcher, and stable public facade. They prove exact defaults and
all six settings domains, atomic optimistic updates, no-op stability, migration-derived defaults,
manifest coverage, strict public JSON, permanent names, full replacement events, audio timebase and
ordered channel preservation, invalid and stale rollback, and dependency direction. They do not
claim live subsystem reconfiguration, project file wire commands, or hardware audio behavior.
The cache warming contract maps deterministic edit and scrub targets through the real graph
evaluator and bounded memory cache, then proves demand reuse, source-fingerprint freshness, and
unchanged recomputation after pressure. It does not claim a production editor, playback, engine,
API, UI, decoder, or proxy consumer.

The effects preset contract exercises the public effects and graph seams together. It proves
complete exact-schema capture, fresh workflow-neutral instances, deterministic current and legacy
documents, transactional explicit schema migration, and edit plus resave continuity through real
unregistered and incompatible graph placeholders before exact-schema recovery. It does not claim a
project container, atomic project save, autosave journal, plugin host, or production engine binder.

The project save contracts exercise public file-backed and in-memory databases through save,
save-as, copy, and backup. They prove exact active-path semantics, collision and alias handling,
read-only source publication, bounded preparation before mutation, current-schema integrity,
permissions and non-UTF-8 paths on Unix, destination-race preservation, deterministic fault
injection before and after publication, and subprocess abort behavior across both rename and
no-clobber commit paths. Migration coverage also publishes all four operations from an exact
schema-0 source while preserving its reported origin revision.

The project autosave contracts exercise the public controller through deterministic monotonic
commands. They prove exact deadlines, disable and manual control, unchanged suppression only while
the periodic artifact exists, one-save forward jumps, strict generation ownership, mtime-independent
retention, foreign and candidate preservation, symlink tamper rejection before deletion, policy and
deadline bounds, no-clobber generation choice, state-preserving exhaustion, retry, complete
current-schema reopen equality, and unchanged active-project bytes. The engine consumer autosaves
the selected real history snapshot after apply, undo, and redo without adding engine filesystem
ownership.

Repository checkpoint execution also has a deterministic local selector at
`.agents/skills/superi-execution/scripts/verify_checkpoint.py`. Given a recorded base revision, it
collects committed, staged, unstaged, and untracked changes, validates changed Python and JSON,
always validates codebase maps, and conditionally runs applicable workflow, dependency, Rust,
fixture, codec-feature, slice, shell, and frontend gates. `--full` selects the complete supported
local set, while checkpoint-specific proof remains mandatory beyond this floor.

Test source documents an intended or previously exercised contract, but its presence is not a fresh
passing result. Individual module maps state whether synthesis ran a suite. At mapping time, the
core and fixture-tool maps record fresh successful runs; several other maps explicitly state that
their synthesis did not execute the tests.

Native and GPU proof is environment-dependent. Many GPU tests return successfully without running
device work when no adapter is available. Timestamp paths may skip when features are absent.
Platform codec proof must run on macOS, Windows, or Linux with the actual framework, transform, or
driver. A host-independent parser test does not prove native lifecycle, pixel fidelity, teardown,
or hardware behavior.

Repository-level CI now has five implemented workflow surfaces. The dependency-policy workflow
checks licenses and sources with cargo-deny on Ubuntu 24.04. The cross-platform workflow runs the
locked Rust quality suite on GitHub-hosted macOS, Windows, and Ubuntu runners. Pull requests and
pushes to `main` run five matrix lanes: blocking `ci-macos-26-arm64` on `macos-26`, blocking
`ci-macos-15-x64` on `macos-15-intel`, blocking `ci-windows-2025-x64` on `windows-2025`, blocking
`ci-ubuntu-24-x64` on `ubuntu-24.04`, and nonblocking preview `ci-ubuntu-26-x64` on
`ubuntu-26.04`. The same matrix runs on the weekly Monday 07:00 UTC schedule and manual dispatch;
those two triggers also add the blocking `ci-ubuntu-22-x64` job on `ubuntu-22.04`.

Every hosted Rust lane checks out with read-only permissions, an immutable pinned checkout action,
and no persisted credentials. It installs the current stable toolchain with rustfmt and Clippy,
records Rust, Cargo, toolchain, and commit identity, then runs the locked open-tree boundary command,
formatting, a locked workspace build, locked workspace tests, strict all-target Clippy, and locked
documentation tests from `open/`.
Linux lanes use one repository-owned helper to install exact build tools, verify and build the
official libva 2.22.0 source at a pinned SHA-256, and publish its header, pkg-config, and runtime
paths. The helper installs the GBM development target, publishes the private native-linker path,
and installs `nasm`, while Intel macOS lanes install `nasm` with Homebrew. Linux
and macOS lanes build the approved libvpx 1.16.0 source after verifying its pinned archive checksum,
then expose the exact shared runtime to capability and codec tests. Windows builds libvpx 1.16.0
with VP9 high-bit-depth support from a pinned vcpkg registry revision as a static MSVC archive,
relinks it into a DLL with the reviewed production export surface, verifies the exact runtime, and
publishes it to those same strict tests.
Hosted macOS skips only three named VideoToolbox or AudioConverter lifecycle tests whose physical
codec evidence belongs to the documented hardware lane; Linux and Windows run the exact full
workspace test command. Matrix fail-fast is disabled, superseded branch runs are cancelled, and
each build has a 90-minute timeout. Ubuntu 26.04 remains experimental, so its failure does not fail
the workflow.

The matrix records an explicit `os_codecs` policy. Both macOS lanes, Windows 2025, and Ubuntu 26.04
build the real CLI consumer with `os-codecs` and run the engine and API consumer suites with their
forwarded features. Ubuntu 24.04 and the separate Ubuntu 22.04 job remain default-only because
their distribution libva APIs are below the platform crate's required version. The default AV1
path pins crates.io rav1d 1.1.0, whose packaged build script removes the broken rav1d 1.0.0 MSVC
reference to an excluded compatibility source.

The durable CI checkpoint record also reports focused workflow-contract verification, one local
locked workspace build with stable Rust 1.97.0, and successful offline fixture-tool policy tests.
The boundary scan is now a recurring workflow step; the other local verification remains delivery
evidence rather than hosted workflow behavior.

The frontend workflow runs on pull requests, pushes to `main`, and manual dispatch using a read-only
Ubuntu 24.04 job. It installs exact Node.js 24.13.0, uses `npm ci` against the committed lockfile,
runs strict no-emit TypeScript 5.9.3 checking, creates a Vite 7.3.6 production bundle, and verifies
the workflow contract plus generated hashed JavaScript entry. Its `ci/frontend-smoke/` consumer is
an isolated toolchain contract, not the deferred React application or Tauri desktop shell.

The Tauri Rust workflow runs on pull requests, pushes to `main`, and manual dispatch across macOS 26
arm64, macOS 15 Intel, Windows 2025, and Ubuntu 24.04. Its pinned CI-only Tauri 2 host uses one
generic command configuration for a mock-runtime unit test and the real native wry builder. Every
blocking lane checks formatting, locked tests, strict all-target Clippy, and locked native binary
compilation; Linux installs WebKitGTK 4.1 and the documented desktop integration prerequisites.
This proves the native host toolchain boundary, not the deferred Phase 3 application or hardware
behavior.

The dedicated network-isolated workflow prepares locked Cargo artifacts, checksum-pinned libva 2.22
and libvpx 1.16 runtimes, and nasm on Ubuntu 24.04 while online. It transfers the private libva
header, pkg-config, native linker, and runtime linker paths plus the approved libvpx path through
the privileged namespace boundary, then runs workspace tests, canonical fixture validation, and the
canonical headless slice inside a distinct Linux network namespace with only
loopback as verified through the namespace-aware procfs network view, no IPv4 route, a failed
numeric outbound probe, and Cargo offline mode. Hosted run
`29308007012` stopped before namespace entry because distribution libva API 1.20 could not satisfy
the unchanged H.266 API 1.22 requirement; both Rust workflows now use the shared source provisioner.
The final hosted run remains the offline execution proof, not an offline acquisition proof or a
runtime import-to-export slice.

The cross-platform Rust workflow does not run an all-container malformed-input matrix beyond
default workspace contracts, frontend or Tauri checks, golden comparisons, benchmarks, soak, or
the vertical slice. The
separate frontend workflow does not prove React, Tauri, the native viewport, API integration,
editorial behavior, or product UI. Neither is an MSRV lane,
and neither is an end-to-end offline build proof because hosted setup and installation may use the network. Hosted
virtual machines also do not satisfy the
physical GPU, display, audio-device, hardware-codec, performance, and long-session lanes in
`docs/platform-testing.md`. Real platform codec lifecycle, pixel or sample fidelity, driver
behavior, device loss, presentation, and teardown still require their owning operating systems and
physical hardware.

Shared fixtures are versioned, immutable by repository policy, and validated offline with
`superi-fixture-tool`. The deterministic video baseline adds 207 one-frame cases across all 23
current pixel formats and nine standard frame rates. The tool proves byte reproduction, while the
`superi-media-io` consumer proves exact geometry, numeric representation, timing, hashes, and public
frame construction. The synchronized audio baseline adds three 100 ms WAVE files across common
sample rates and canonical stereo, 5.1, and 7.1 layouts. Byte reproduction and production PCM-source
consumption prove sample clocks, masks, routing order, exact samples, shared signal boundaries, and
bounded continuity. The deterministic timing baseline adds five cases and 18 samples for CFR,
decode-order VFR, 29.97 drop-frame labels, a forward gap, and a reset. Its media-I/O consumer proves
real packet and presentation maps, unsegmented discontinuity rejection, and reversible explicit
segments. The deterministic color baseline adds eight SDR, wide-gamut, PQ, HLG, alpha,
f16, and f32 images plus three ACEScg sequence frames. Public color transforms prove transfer order,
HDR scene meaning, reference-white handling, output intent, alpha association, and exact high-depth
bits, while public media sequence access proves logical, file, and presentation identity. It does
not prove encoded HDR, still-image decoding, display rendering, perceptual golden tolerances, or GPU
color execution. The deterministic media-error baseline adds malformed WAVE, truncated AIFF, unsupported
AIFC, and post-open partial-read coverage. Its production PCM consumer proves shared error and
recovery classifications plus exact aligned packet and corruption evidence. These remain synthetic
raw-frame, PCM-container, timing-metadata, color, and focused PCM-failure proofs, not encoded codec
corruption, malformed Matroska, MP4, or MXF, hardware, playback, device, A/V synchronization,
scheduling, or editorial runtime proof. Snapshot validation still does not prove Git-history immutability,
provenance truth, legal clearance, or semantic quality beyond focused contracts. The separate
golden harness baseline commits exact typed examples for frame, audio, timeline, and project
comparison. It proves the reusable verifier and canonical format, while real renderer, mixer,
timeline-engine, and project-runtime outputs remain consumer-owned future cases. The separate
`slice/video-cfr` fixture provides one digest-bound 96-frame AV1 WebM for the canonical runner. Its
canonical runner still treats decoded traits as expectations, while engine resource and render-export
contracts now open it with the production WebM source and decode all 96 frames through the in-tree
AV1 backend. The render-export lane evaluates those frames through one shared graph and delivery
stage, then rejects the selected VP9 encoder's duration rounding before publication. The derived
`slice/expectations` version 1 remains immutable historical data. Current version 2 adds a portable
project-state digest while retaining the same 48 mirrored RGBA8 frame identities, explicit pixel
and PCM tolerances, synchronized audio probes, timestamps, and target export metadata. The CLI
validates those applicable values but cannot compare rendered pixels that no current stage produces.

The deterministic OTIO baseline adds a 48-frame native JSON projection of the canonical slice plus
a 120-frame coverage timeline. The coverage payload includes clips, gap, transition adjacency,
owner-relative markers, a trimmed nested Stack, 2.0 and 0.5 LinearTimeWarp effects, stable object
identity, and explicit unsupported-effect pointers. `superi-timeline` now imports those documents
into the ordinary typed native project, preserves complete source templates, and exports the
current edited hierarchy through an explicit OTIO_CORE 0.18.1 target. Supported names, ranges,
media links, transition handles, marker values, metadata, and linear retime scalars remain directly
editable; unsupported fields and effects remain opaque with stable warning pointers. The public
headless example emits deterministic JSON, and official OpenTimelineIO 0.18.1 loads, target-writes,
rereads, and finds both Rust-produced outputs equivalent at their exact 48-frame and 120-frame
durations.

The native timeline model now exposes checked source-to-record range maps, media and nested source
availability context, and embeddable video, audio, caption, and data semantics using core-owned
clocks, identifiers, and ordered channel meanings. Range mapping is exact across clocks, media
overscan remains editable, and nested availability derives from the linked timeline. Audio routes
require one explicit decision per source channel, sample placements retain typed clip links through
split and trim, and continuity reports expose every record gap, overlap, source jump, or linked-clip
change. Tracks embed those semantics in the validated native timeline container. Timeline-local
project state also owns stable manual bins and sub-bins, saved metadata and relink queries, and
explicit online, missing, unverified, or fingerprint-mismatch evidence. Those values publish in the
same atomic project revision without replacing clip media identity or flattening nested sequences.
Timeline-local
edit state adds exact or relationship-expanded selection, stable per-track target and sync-lock
intent, canonical clip links and groups, direct member control, deterministic target and sync
projection, and structural reconciliation inside the same project transaction. Atomic foundational
edit batches now add ripple,
roll, slip, slide, razor, trim, extend, and exact three-point and four-point placement to insert,
overwrite, append, replace, lift, and extract. They preserve clip source and nested-timeline
relationships, inherit fragment intent, require explicit sync-locked ripple adjustments, report
typed fragments and invalidated transitions, reject implicit fit-to-fill retiming, and publish at
one project revision. Nested operations place
existing child timelines, create compound timelines and their parent clips atomically, edit shared
children through stable instance identities, and expose every direct or recursive relationship
without flattening. Deterministic compilation now converts a selected root and every reachable
nested timeline into one typed editable graph revision with stable domain-separated addresses,
explicit transition and nesting edges, complete object parameters including multicam intent, and
bidirectional provenance.
Project now retains that compiled state and engine resource acquisition preserves its exact selected
graph. Project also interprets recognized referenced-media paths through one stable target format,
and engine adapts the resolved local path, stored `MediaId`, and fingerprint evidence into the real
source acquisition flow. Engine project history now wraps authored media, settings, and compound
commands and reverses the complete aggregate, including retained graph and clip-mix state, through
immutable project snapshots. Timeline edits use a three-way recompile to preserve nonconflicting
direct graph work. Selected snapshots also reach complete project-owned autosave recovery points
after apply, undo, and redo. API exposes settings transactions through that owner but does not expose
generic history or autosave commands; CLI, playback, audio-engine, graph evaluation, and rendering
do not execute that history.

Native multicam state composes those same timeline and clip owners. One synchronized source
timeline stores ordered `MulticamAngleId` metadata and clip membership, while each ordinary nested
target clip stores an independent source-clock switch partition and explicit follow-video,
fixed-angle, or all-angle audio intent. Resolution follows the target clip time map, active angle
membership, and selected source clip time map without flattening the direct source relationship.
Structural fragments and replacements inherit source membership and target switch intent through
the shared atomic edit path. Graph compilation retains that intent as typed parameters, and the
project document retains the complete compilation. Runtime graph evaluation, playback, mixing,
engine interpretation of multicam intent, and API consumers remain absent.

Versioned timeline state documents preserve the complete editable owner graph without claiming the
project file boundary. `serialize_timeline_state` emits canonical `superi.timeline` revision 1 JSON
with the stable primitive revision and SHA-256 payload integrity. `deserialize_timeline_state`
strictly rejects corrupt, interrupted, unknown, oversized, or future state, migrates revision 0 in
memory, reconstructs through checked media, timeline, annotation, relationship, retime, nesting,
and multicam APIs, and exposes canonical current bytes only after whole-project validation. Strict
`TimelineGraphValue` Serde also preserves compiled graph payloads through the graph codec.
`superi-project` stores canonical current timeline, settings, graph, and audio component documents
inside stable SQLite schema 3. Its exact schema-0, schema-1, and schema-2 migrations accept declared
timeline and graph component revisions through the owning codecs, derive deterministic settings
from the root rate at the 1-to-2 step, add canonical empty audio at the 2-to-3 step, and write
canonical current rows before commit. One complete-candidate command surface publishes save,
save-as, copy, and backup files without moving document meaning into timeline. Project autosave
reuses that surface for host-driven recovery points. Timeline-driven scheduling, runtime playback,
mixing, recovery discovery and restoration, and public database file consumers remain absent.

The largest verification gap is the absence of a production import-to-export slice. Its canonical
contract, source fixture, reference project state, graph control state, public action flow, and
contract runner now exist. Process contracts run the CLI twice and prove all eight timing and
resident-memory records, the observed-boundary maximum, and deterministic content after normalizing
run-specific measurements and paths. Both hosted Rust build jobs now validate the fixture root and
run the same portable eight-stage contract as a first-class development baseline. Independent
expected data now exists, and native timeline compilation produces generic editable graph state,
and effects provides typed catalog state plus real headless reference pixels. Effects now also
preserves reusable complete preset schemas, typed literals, explicit migrations, and editable
missing-plugin placeholders through strict standalone documents and ordinary graph reload. Atomic
project save and deterministic autosave recovery-point publication are implemented, while recovery
selection, restoration, dismissal, and journals remain unimplemented at the project boundary.
Engine graph
evaluation, GPU effect and transition execution, production timeline-to-transition binding,
rendered comparison, color delivery, and muxing are not
integrated. The new encoder path begins from caller-prepared frames and ends at elementary derived
packets, so it does not close that slice. There is no
current test or runtime that imports through the engine, selects and decodes original media,
edits a timeline, evaluates a graph, applies input and output color, renders through the GPU,
encodes and muxes output, persists a project, and drives the flow through the public API.

## Placeholders and incomplete integration

The only entire crate skeleton is `superi-ai`. `superi-project` now has a substantive in-memory
document aggregate, immutable snapshots, checked whole-project edits, retained timeline and named
standalone graphs, authoritative versioned settings, authored clip-mix state, stable schema-3 SQLite
serialization, exact schema-0, schema-1, and schema-2 migration, real engine settings and resource
consumers, and a stable public settings facade, plus versioned portable referenced-media paths and
revision-fenced path and relink commands, semantic no-op suppression, and checked monotonic snapshot
restoration consumed by engine command history. Its one typed save command builds and validates
complete same-parent candidates, atomically replaces or claims destinations, preserves copy and
backup identity, and rebinds save-as at the publication commit point. Its typed autosave controller
provides deterministic scheduling, complete Backup recovery points, bounded generation retention,
safe pruning, and direct user control. Recovery discovery and restoration remain placeholders,
while persisted command logs, modified-since-open conflict policy, database file API, CLI, and
scripting remain absent.
`superi-effects` now has substantive graph-native authoring, exact keyframe animation, strict visual
effect presets with complete schemas and typed literals, deterministic integrity-protected preset
documents, explicit schema migration, missing-plugin editing and exact-schema recovery,
composition artifacts with layer parenting, reusable precompositions, collapse boundaries, exact
time remapping and complete resolved paths, strict editable 2D and 3D spatial layers with cameras,
lights, stable depth ordering, exact motion sampling, graph reload, and bounded real pixels, editable cubic vector shapes with fills, strokes,
gradients, repeaters, and path animation, animated cubic mask authoring plus soft-coverage
composition, editable rotoscope spans, corrections, and propagation hooks, styled text authoring
plus editable point, planar, object, and calibrated camera tracking artifacts with manual corrections
and bounded CPU reference solvers, offline OpenType shaping and Unicode paragraph layout, reusable typed control rigs, built-in
visual nodes, and bounded CPU reference execution. It also has reusable cross-dissolve and
directional-wipe schemas, exact handle timing, animatable transition parameters, and bounded
reference pixels, while production spatial GPU execution, production GPU factories, vector, mask, and
text rasterization, glyph atlases, propagation solvers, pyramid and GPU tracking acceleration,
production tracking attachment, concrete OpenFX worker IPC, native ABI adapters, production
transition and timeline attachment, and production effect execution remain absent. Its OpenFX 1.5.1
effect-side host now validates isolated adapters, projects
graph-native definitions, samples exact-time values, and owns permissions, lifecycle, recovery, and
quarantine without loading native code. Engine supervision discovers bundles, coordinates launchers,
contains failures, and rebuilds shared workflow availability above that host.
`superi-audio` now has a substantive
independent processing graph, typed bus routing, sample-accurate scheduler, production device input and output,
canonical authored clip-mix codec, clip-mix processor, prepared sample-rate converter, explicit channel conversion, and prepared core
effects, plus a graph-native meter, while automation, hosting, and decoded-sample binding remain
absent. Engine foreground playback feeds the bounded output producer and consumes its actual
presentation clock through explicit video synchronization and recovery outcomes. Transport uses
callback-owned discard acknowledgement and explicitly mutes inactive or unsupported signed-rate
audio. Engine render-export invokes an explicit audio stage, retains its graph identity, and proves
real acquired PCM decode and encode completion, but no adapter yet binds decoded or scheduled
timeline blocks into the actual prepared graph, routing, effects, resampling, and device delivery
path.
`superi-cache` now has substantive composite identity, budgeted memory retention, hierarchical
memory policy, priority-aware LRU eviction, precise edit invalidation, persistent storage, color
metadata, replaceable proxy or optimized-media publication, deterministic inspection and clearing,
safe persistent relocation, layered render reuse, bounded background population, bounded playback
prediction, owned asynchronous host retention, and bounded edit and scrub warming, while automatic
capacity and external-directory policy remain caller-owned. Engine performs substitution using
concurrency-owned selection and consumes predictive plus foreground retained graph evaluation.
Transport replans the bounded playback predictor, but no editor invokes warming and no export owner
invokes the render queue. The render queue still lacks a
production engine node catalog, ROI and
invalidation orchestration, and a rendered frame consumer, and no production editor or playback
owner invokes warming.

Partial modules contain these explicit placeholder areas:

- `superi-api`: scripting, wire transport, subscription delivery, version negotiation, database
  file commands, and general editorial mutation beyond the fixed canonical scenario. Media and
  complete engine introspection plus coherent integration validation remain read-only surfaces;
  project settings inspection and mutation are implemented.
- `superi-project`: persisted history, recovery discovery, comparison, restore, dismissal and
  journals, modified-since-open conflict policy, dirty-state hashing, unknown extension
  preservation, public database API adaptation, CLI, and scripting beyond its implemented document,
  settings, database, migration, media path, atomic save, and autosave command owners.
- `superi-audio`: binding decoded samples into the real prepared graph, plugin hosting, effect
  automation, and engine composition across schedule, conversion, clip and effects processing,
  graph routing, and device execution. Export currently owns only an explicit stage seam.
- `superi-color`: broader config-persisted rule graphs, ICC transform evaluation, GPU output
  conversion, and production viewport or export integration.
- `superi-concurrency`: GPU submission coordination module and production composition beyond the
  audio domain, derived-media selection, playback and export workers, export dependency history,
  clocks, bounded handoffs, and lifecycle control consumers.
- `superi-engine`: one placeholder orchestration module covering nodes.
  Classified cross-subsystem error propagation and recovery is implemented beside the canonical
  lifecycle. Project and device power lifecycle is an explicit action boundary, but platform
  callbacks, additional project command adapters and native device owners
  are not yet bound to those actions. Typed project media command history, full snapshot undo and
  redo, project settings, and selected-snapshot autosave compatibility use the same authoritative
  project state, but production background autosave hosting, additional command adapters, compound
  transactions, live subsystem reconfiguration, callbacks, and native device owners remain
  unbound. Typed dispatch, bounded state events, a capacity-one nonblocking bridge into the real playback
  transport, dispatcher-owned logical export control with full replacement state, and coherent
  read-only integration validation are implemented. Shared finite-resource arbitration is
  implemented as one engine-owned opt-in envelope
  across decode, GPU, cache, audio, AI, and export classes, while current subsystem owners do not yet
  bind every resource or install their reclaimers automatically. Playback and transport compose
  prepared foreground graph, cache, CPU display,
  audio discontinuity, A/V coordination, clock, worker, prediction, and viewport owners, but
  prepared source and decoded-audio binding and native GPU presentation remain absent.
  Render-export composes explicit acquired source, decoder, shared graph, delivery, audio, encoder,
  lifecycle, and recovery owners into complete elementary streams. A bounded logical queue composes
  export-priority attempts, dependencies, progress, pause, resume, retry, cancel, and retained typed
  results around that path. Its stable one-job and all-job actions plus automated state observations route through the
  dispatcher with fresh export permits for submit and recovery attempts; generic executors and
  typed results remain runtime local. Native GPU readback, container muxing, publication,
  persistent queue recovery, and application integration remain absent. Lifecycle, resource
  preparation, and clip-mix edit orchestration are implemented behind the compound project
  transaction and dispatcher. Plugin discovery and supervision
  coordinate boxed effects hosts, active graph availability, classified containment, and recovery
  for all three workflow roles, while concrete platform transports and native OFX adapters remain
  absent.
- `superi-effects`: production GPU node implementations, engine registration, playback, viewport,
  export, effects-to-project persistence integration, autosave and recovery orchestration,
  production native plugin binding, UI,
  production spatial transform, camera, light, and motion-blur
  execution, vector shape and mask rasterization, reusable rigging beyond bounded time and scalar parent
  expressions, persistent rig presentation, feather and expansion filtering, propagation solvers,
  production transition binding and GPU parity, text rasterization, glyph atlases, production
  tracking attachment plus pyramid and GPU acceleration, concrete OpenFX worker IPC and native
  adapter implementation, beyond its implemented safe OpenFX host contract and engine-owned bundle
  discovery and supervision,
  graph-native authoring, complete reusable presets, strict current
  and legacy preset documents, explicit transactional schema migrations, graph-native missing-plugin
  recovery, exact keyframe curves, versioned built-in effect and transition schemas, exact transition
  timing, reusable control drivers, strict visual
  composition, strict spatial composition, vector shape, and animated mask-stack payloads, editable
  rotoscope artifacts and hooks, editable point, planar, object, and calibrated camera tracking artifacts with bounded CPU
  reference solvers, strict styled text, real shaping and paragraph layout, bounded CPU reference
  including spatial real pixels,
  ROI, diagnostics, strict reload, and immutable real-pixel graph contracts.
- `superi-graph`: invalidation and ROI render orchestration, outer job dispatch, direct
  project-history command adaptation, engine coordination, cache invalidation invocation and
  resource policy,
  production engine node catalogs, and runtime consumers beyond its implemented identifier,
  neutral typed value, node-schema, typed DAG storage and validation, schema-bound instances,
  immutable snapshots, atomic editable
  transactions, canonical versioned graph documents, typed parameter drivers and expressions,
  caller-projected literal evaluation,
  derived fail-closed plugin availability, exact dirty regions, dependency and semantic edit
  invalidation, snapshot-bound ROI planning, deterministic request-scoped scheduling, node
  introspection, graph and revision cache lineage, retained value adapter and pruning, run-local
  timing, and shared interactive and headless evaluation surfaces.
- `superi-timeline`: broader OTIO schema and vendor-effect interpretation beyond its pinned 0.18.1
  native subset, graph evaluation, fit-to-fill, grouped-source compound synthesis and higher-level
  edit orchestration, typed project-history command adaptation, multicam playback and mixing,
  autosave and recovery-journal orchestration, and application consumers beyond its
  native model, authoritative edit state, marker and metadata state, exact snapping, foundational,
  advanced, nested, and multicam edit operations, deterministic graph compilation, versioned state
  documents, shared processing-payload compatibility, OTIO headless consumer, and contract tests.

Substantive modules also have intentionally incomplete boundaries. Media I/O has no muxer or
production registry owner for its source backends. GPU has no cross-adapter transfer or external
decoder import. Color has no ICC transform evaluation, project-configured rule persistence, or
production output-transform consumer. Vendor IPC has no
shared memory, GPU transport, sandbox, or encode. Platform capability and proof depth differ by
host. Repository status documents disagree about package count, implementation maturity, CI, and
legal completion.

A declared dependency, public module name, documented architecture, or passing compile is not proof
that one of these flows exists. Remove a placeholder label only after substantive behavior has a
real consumer and proportionate verification.

## How to navigate the maps

1. Start with this index to identify the owning module and its direct producers and consumers.
2. Read the owning module map from first line through EOF. For a cross-module change, also read each
   affected producer, consumer, and public contract map in full.
3. Use the module's `Source inventory` to locate complete raw-file ownership. An optional map may be
   omitted only when root law permits it and the module manifest, public entry points, cross-module
   interfaces, and relevant implementation and tests are read through EOF and recorded instead.
   Search is only for discovery and never replaces the required complete reads.
4. Follow `Public surface` for supported entry points, `Architecture and data flow` for actual
   execution, `Dependencies and consumers` for direction, `Invariants and operational boundaries`
   for constraints, and `Current status and risks` for incomplete behavior.
5. Treat mapped status as a navigation aid, not current runtime proof. Revalidate facts that may
   have changed and run fresh tests appropriate to the affected boundary.

For common concerns, begin at these owners:

- Shared values, exact time, identifiers, errors, and wire primitives: `superi-core`.
- Files, packets, decoded frames, decoded audio, and backend selection: `superi-media-io`.
- Default, host, and vendor codec behavior: the three `superi-codecs-*` modules.
- CPU still images and reference operations: `superi-image`.
- Color interpretation and transforms: `superi-color`.
- GPU resources, residency, conversion, submission, and recovery: `superi-gpu`.
- Jobs, domains, clocks, handoffs, lifecycle, and liveness: `superi-concurrency`.
- Audio graph topology, input and output device identity, capability discovery, record arming,
  input monitoring, bounded sample handoff, typed platform callbacks, stream lifecycle,
  audio-clock publication, capture and output telemetry, and graph-native
  peak, RMS, true-peak, phase, spectrum, and loudness analysis: `superi-audio`.
- Graph-facing identifiers, node schemas, deterministic DAG state, typed binding validation,
  schema-bound instances, editable transactions, canonical graph documents, typed parameter links
  and expressions, missing-node resolution, exact dirty regions, dependency and semantic edit
  invalidation, snapshot-bound ROI propagation, deterministic request-scoped scheduling and
  evaluation, pre-execution node introspection, graph and revision lineage plus retained-value
  adapter, run-local timing, and shared interactive and headless evaluation:
  `superi-graph`, with value identity, rational time, and pixel bounds owned by `superi-core`.
- Visual definitions, complete reusable effect presets, explicit preset schema migration, editable
  animation, controls, visual composition, tracking, text, transitions, and bounded reference
  evaluation: `superi-effects`. Presets use graph-owned documents and derived missing-node
  availability while project persistence and plugin hosting remain separate owners.
- Coherent whole-project snapshots, authored clip-mix state, schema-3 SQLite storage, supported
  forward migration, deterministic autosave recovery points, active
  project path identity, atomic save, save-as, copy, and backup publication, portable media paths,
  revision-fenced relink commands, deterministic autosave scheduling, managed recovery points,
  bounded retention, and safe pruning: `superi-project`.
- Complete reusable-result identity, budgeted final-frame and intermediate-node memory retention,
  exact total, project, and device admission, priority-aware LRU eviction, precise revision-safe
  graph edit invalidation, versioned bounded disk persistence with corruption recovery, cache color
  identity, complete derived-media publication, deterministic inspection and clearing, safe
  persistent relocation, layered render reuse, bounded background population, and bounded
  exact-frame edit and scrub warming: `superi-cache`, followed by `superi-engine` for codec
  generation and transparent substitution and `superi-concurrency` for quality choice and
  background job execution.
- Native editorial objects, typed track semantics, exact timing and clip retiming, selection, track
  targeting, sync locks, linked selection, clip grouping, markers, deterministic metadata, exact
  snapping, and foundational insert, overwrite, append, replace, lift, and extract operations plus
  advanced ripple, roll, slip, slide, razor, trim, extend, three-point, and four-point operations,
  plus nested placement, compound creation, shared child editing, recursive nesting inspection,
  multicam angle metadata, synchronization provenance, switching, audio intent, exact resolution,
  and deterministic typed editable graph compilation:
  `superi-timeline`.
- Durable project settings, defaults, strict candidate validation, schema-3 persistence, and
  migration: `superi-project`, followed by `superi-engine` for typed subsystem resolution and
  dispatcher control, then `superi-api` for the stable transport-neutral surface.
- Current assembly, public capability, health, and coherent integration validation flow:
  `superi-engine`, then `superi-api`, then `superi-cli` for the process consumer.
- Shared finite-resource arbitration across decode, GPU, cache, audio, AI, and export workloads:
  `superi-engine`, followed by each lower subsystem owner for its authoritative local allocation and
  release behavior.
- Product law, open and closed boundaries, CI, fixtures, and maintenance workflow: `workspace`.
- Canonical first editorial slice, typed scenario state, replacement stages, and proof: `workspace`.
- Reviewed internal runtime dependency direction: `tool-superi-dependency-check`.
- Static network-client and open-to-closed enforcement: `tool-superi-boundary-tool`.
- Deterministic structured platform-lane evidence: `tool-superi-test-report`.

## Map maintenance

Run the mapping commands from the repository root:

```text
python3 .agents/skills/superi-mapping/scripts/codebase_maps.py inventory
python3 .agents/skills/superi-mapping/scripts/codebase_maps.py files <module-id>
python3 .agents/skills/superi-mapping/scripts/codebase_maps.py hash <module-id>
python3 .agents/skills/superi-mapping/scripts/codebase_maps.py changed --base <revision>
python3 .agents/skills/superi-mapping/scripts/codebase_maps.py validate
```

After source changes, use `changed` and the actual diff to find directly affected ownership. Read
every changed raw file and relevant public interface, caller, consumer, test, schema, fixture, and
governing document through EOF. Update source inventory, surfaces, flows, dependencies, invariants,
tests, status, risks, and maintenance guidance before recording the new exact hash and file count.

Update consumer maps even when their own source hash did not change if a relationship or contract
they describe changed. Update this index whenever ownership, dependency layering, public control
flow, major runtime flow, shared invariant, module status, open and closed boundary, or navigation
advice changes.

Do not perform a hash-only refresh. Generated maps are excluded from module source hashes, so
validation cannot detect stale prose by hash alone. Rerun validation after map edits, after final
integration or rebase, and immediately before delivery. Validation must confirm the complete
module inventory, exact frontmatter hashes and file counts, required module headings, every source
inventory path, every index link, and absence of forbidden Unicode dash characters.
