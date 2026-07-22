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
| `superi-ai` | [module map](modules/superi-ai.md) | `open/crates/superi-ai` | Local-only inference and editable-artifact boundary with honest executable capability discovery | Partial: schema-1 unavailable-runtime discovery is implemented and consumed by the desktop shell; model audit, loading, pipelines, inference, and editable artifact production remain skeletons |
| `superi-api` | [module map](modules/superi-api.md) | `open/crates/superi-api` | Transport-neutral public schema catalog, stateless API and project version negotiation, host-injected permission boundary, current state and control facades, bounded deterministic local scripting, bounded ordered event delivery, declarative extension discovery, durable command-log inspection, generated TypeScript contracts, and durable local project hosting | Partial: deterministic versioned discovery, strict API and project compatibility negotiation, JSON-RPC data contracts, fail-closed authorization, immutable extension identity, lifecycle, capability, safe failure, and control discovery, current public DTOs and controls including exact playback transport, all thirteen track mutations, all six caption mutations, all six marker mutations, all seven multicam mutations, and all 20 timeline edits with atomic audio-video link, source-time synchronization, detach, transition-handle and retime replacement, existing-child nested placement, selection-derived compound creation, and complete channel routing under editor schema `1.7.0`, digest-bound `superi-json` interpretation, bounded replay with explicit reconnect recovery, atomic recording of every successful stable project command, permission-checked command-log replay inspection, deterministic TypeScript output, an API-owned local host for no-clobber create, open, mutation, copy, backup, recovery, validation, render settings, command-log inspection, and narrow JSON-RPC automation, and a production desktop consumer of canonical editor-state timeline, graph, attached automation and playback state, shared selection, exact transport control, and durable track, caption, marker, multicam, audio-video, routing, transition, retime, nesting, and compound commands with typed reversal are implemented; authentication, network transport, general dynamic routing, push delivery, persisted event replay, public job submission and typed results, general-purpose language evaluation, and full-catalog automation remain absent |
| `superi-audio` | [module map](modules/superi-audio.md) | `open/crates/superi-audio` | Independent prepared audio graph with explicit channel conversion, typed bus routing, fixed route delay compensation, transactional clip DSP, revisioned clip-gain automation, canonical authored-state serialization, bounded native plugin state, timing-matched isolated bridge fallback, core effects, macOS Audio Unit effects, worker-side VST3 effects, sample-accurate scheduling, bounded device I/O, dual-clock sample-rate conversion, and graph-native metering | Partial: core graph, routing, fixed delay compensation, clip, device, conversion, metering, clip-gain automation, exact Audio Unit and VST3 state persistence, verified Audio Unit isolation, canonical single-main-bus VST3 processing, isolated bridge fallback, and read-only desktop clip-key presentation are implemented; engine owns lifecycle supervision and per-node project state, while decoded-sample binding, automation persistence, concrete platform worker transport, broader effect automation, variable-rate playback audio, Audio Unit instruments, dynamic latency rebuild, and complete timeline composition remain absent |
| `superi-cache` | [module map](modules/superi-cache.md) | `open/crates/superi-cache` | Composite reusable-result identity, budgeted final-frame and intermediate-node memory retention, priority-aware strict LRU eviction, precise graph edit invalidation, versioned corruption-recovering disk persistence, replaceable derived-media publication, layered render reuse, bounded background population, bounded playback prediction, bounded edit and scrub warming, and deterministic lifecycle management | Complete identity feeds independent memory and disk tiers with exact admission, revision fencing, bounded envelopes, atomic publication, schema isolation, and corruption quarantine; memory, persistent, and derived owners expose inspection and exact clearing, persistent namespaces relocate through rename or synchronized staged copy, render jobs add cancellation-safe layered reuse, prediction supplies finite signed frame plans and an owned host adapter, and warming is deterministic and hard bounded; engine and scheduler own quality substitution and lifecycle policy remains caller-owned |
| `superi-cli` | [module map](modules/superi-cli.md) | `open/crates/superi-cli` | Headless durable project, media, timeline, render settings, inspect, command-log query, validate, recovery, JSON-RPC automation, public schema, canonical scenario, and engine validation consumer | Implemented no-clobber creation, complete open and inspect, revision-fenced durable mutation, copy, backup, recovery, render configuration, metadata and permission-checked replayable command-log query, strict policy input, bounded path-redacted errors, per-request flushed JSON-RPC automation, deterministic `api schema` including permission, version-negotiation, and extension-discovery metadata, exact fixture scenario transactions, portable expectation verification, eight instrumented contract stages, and strict `engine validate`; version-negotiation command execution, public job hosting, event polling, render submission, muxed publication, persisted undo and redo branch journals, and live application attachment remain absent |
| `superi-codecs-platform` | [module map](modules/superi-codecs-platform.md) | `open/crates/superi-codecs-platform` | Opt-in host codec adapters for Apple, Windows, and Linux | Implemented, host-dependent: native proof depth varies and legal review remains open |
| `superi-codecs-rs` | [module map](modules/superi-codecs-rs.md) | `open/crates/superi-codecs-rs` | Default permissive software codec implementations | Implemented: AV1, FLAC, MP3, Opus, PCM, Vorbis, VP8, and VP9 decode and encode; the desktop directly consumes only the in-tree PCM backend for bounded exact WAVE previews |
| `superi-codecs-vendor` | [module map](modules/superi-codecs-vendor.md) | `open/crates/superi-codecs-vendor` | Explicit process adapter for separately installed vendor RAW workers | Implemented first revision: decode-only, CPU-only, JSON and hexadecimal IPC |
| `superi-color` | [module map](modules/superi-color.md) | `open/crates/superi-color` | Versioned configuration, project working spaces, color math, CPU input and output transforms, GPU wide-gamut, native display transforms, deterministic viewer analysis, tone mapping, legal-range RGB encoding, LUTs, ICC discovery, and presentation profile guards | Substantial but partial: project-pinned configuration, CPU transforms, managed GPU wide-gamut plus eight-mode sRGB and Display P3 native display transforms, an engine CPU display consumer, and four exact monitor-bound desktop GPU display consumers are implemented; engine export validates a caller-owned delivery stage but does not execute this crate, while arbitrary ICC evaluation and concrete export conversion remain absent |
| `superi-concurrency` | [module map](modules/superi-concurrency.md) | `open/crates/superi-concurrency` | Execution domains, jobs, clocks, handoffs, shared snapshots, lifecycle, liveness, and derived-media selection | Substantial; audio enforces its domain, engine proxy resolution consumes selection, engine foreground playback and transport consume bounded workers, cancellation, anchor-based clocks, the A/V scheduler, and handoffs, engine lifecycle composes acknowledged startup, sleep, wake, shutdown, and restart phases with EngineControl ownership, immutable publication, and lock-free signals, engine error propagation keeps bounded bookkeeping in a separate EngineControl `DomainOwned`, render-export enforces lifecycle admission, and the engine export queue composes bounded workers, progress, dependency history, typed completion, and recovery attempts; broader liveness and GPU submission composition remain incomplete |
| `superi-core` | [module map](modules/superi-core.md) | `open/crates/superi-core` | Tier-zero values, validation, exact time, identifiers, errors, diagnostics, and stable serialization | Implemented and broadly consumed; crate-level skeleton wording is stale |
| `superi-effects` | [module map](modules/superi-effects.md) | `open/crates/superi-effects` | Graph-native visual definitions, editable defaults and instances, reusable presets, animation, controls, composition, spatial, shape, mask, rotoscope, tracking, text, transitions, isolated OpenFX hosting, and bounded CPU reference evaluation | Substantive but partial: graph-native authoring, strict documents and migrations, missing-plugin recovery, visual contracts, isolated OFX adapter validation, boxed dynamic forwarding, permissions, lifecycle, recovery, quarantine, workflow parity, real pixel proof, topology-backed desktop effect badges, and typed scalar, Boolean, and choice editing for attached transition nodes are implemented; effects-to-project persistence integration, native plugin binding, production GPU execution, concrete worker transport, general effect editing UI, and complete timeline attachment remain absent |
| `superi-engine` | [module map](modules/superi-engine.md) | `open/crates/superi-engine` | Open subsystem assembly and orchestration | Partial: complete and source-only media registry assembly, engine-wide typed command dispatch, scoped standalone EngineControl hosting, bounded revision-fenced project history, atomic durable command recording, curated behavior-free public editor, project compatibility, and save construction seams, deterministic semantic project diagnostics and coherent editor-state inspection, compound timeline item, track, caption, marker, multicam, nested placement, selection-derived compound, graph, media, authored audio, extension, and root transactions, removed-track clip-mix reconciliation, retained-graph recompilation, undo and redo, settings, audio automation, crash recovery, lifecycle, validation, playback and export dispatch, a production timing-only exact playback runtime, project preparation, autosave consumer proof, render-export orchestration, arbitration, OpenFX containment, native audio plugin lifecycle, and one bounded declarative runtime extension registry are implemented; production autosave hosting, persisted automation and undo and redo branch journals, live subsystem reconfiguration, concrete lifecycle workers and platform callbacks, automatic arbiter binding, decoded source and prepared-audio binding, native GPU presentation, muxing and publication, concrete plugin transport, public job submission and typed result access, persistent job recovery, and production plugin factories remain absent |
| `superi-gpu` | [module map](modules/superi-gpu.md) | `open/crates/superi-gpu` | wgpu device, resource, upload, conversion, pass, submission, presentation, and recovery substrate | Implemented substrate with four production desktop surface consumers and explicit remaining integration gaps |
| `superi-graph` | [module map](modules/superi-graph.md) | `open/crates/superi-graph` | Node-neutral identifiers and shared typed values, versioned schema discovery, deterministic DAG storage, typed port validation, editable mutation transactions, canonical graph documents, reusable scalar expressions, typed parameter links and expressions, caller-projected literal evaluation, derived missing-node resolution, dependency and semantic edit invalidation, region-of-interest propagation, request-scoped scheduling and evaluation, node introspection, graph and revision cache lineage, timing, and shared interactive and headless evaluation snapshots | Partial: graph-facing IDs, exact neutral domain and processing values, node schemas, immutable discovery, typed DAG state, atomic mutations, deterministic integrity-checked serialization, checked deserialization, legacy migration, shared bounded scalar programs, typed driver state, parameter-cycle protection, literal-only projected evaluation, fail-closed missing-node placeholders, exact region and edit invalidation, snapshot-bound ROI planning, generic demand-only evaluation, deterministic graph cache inspection, final and intermediate retained-work pruning, run-local timing, role-neutral editable-to-runtime evaluation, desktop clip topology and driver inspection, and typed desktop scalar, Boolean, and choice mutation for attached transition nodes are implemented; effects consumes broad authoring and reference evaluation, timeline compiles editable graphs, project retains and atomically publishes timeline and named standalone graph documents, and engine consumes externally prepared snapshots for playback and render-export, while production engine catalog and plugin binding and complete application rendering remain absent |
| `superi-image` | [module map](modules/superi-image.md) | `open/crates/superi-image` | Host image values, still interchange, CPU operations, sequences, previews, and reference validation | Implemented host-side subsystem with explicit representation limits; desktop selected-media previews consume its dense RGBA and aspect-fit scaling contracts |
| `superi-media-io` | [module map](modules/superi-media-io.md) | `open/crates/superi-media-io` | Codec-neutral source, demux, packet, frame, audio, selection, timing, and operation contracts | Implemented contracts and four demuxers; engine source registration, project-path request adaptation, preparation, and complete elementary-stream export lifecycle orchestration are integrated, desktop selected-media inspection consumes exact WAVE and waveform contracts, and the desktop source monitor retains real source open and exact seek behavior through the engine source-only registry, while persistent path syntax, muxing, and publication remain outside this crate |
| `superi-project` | [module map](modules/superi-project.md) | `open/crates/superi-project` | Whole-project in-memory document, authoritative released-format compatibility negotiation, semantic hashing and component diagnostics, durable settings, authored audio, opaque extension records, bounded durable command log, stable SQLite serialization and migration, collaborative-safe atomic save, save-as, copy, backup, and autosave publication, read-only integrity validation and repair reporting, referenced-media paths, and recovery boundary | Partial: coherent revisioned aggregate, immutable snapshots, checked monotonic whole-snapshot restoration, one authoritative schema 0 through 5 format table with typed compatibility outcomes and migration paths, versioned SHA-256 semantic content identity and ordered typed component evidence, authoritative versioned settings, retained timeline and named standalone graphs, authored clip-mix state, bounded plugin, effect, AI artifact metadata, unknown extension state, and a project-owned 4096-record, 64 MiB command log, schema-5 SQLite identity and eight strict tables, deterministic canonical components, exact opaque payload preservation, command request evidence, and integrity manifest, exact frozen schema-0 through schema-4 compatibility, contiguous forward migration, checked reconstruction, validated active-file generation fencing, cooperative cross-process writer locking, durable publication, stable media and extension commands, deterministic host-driven autosave scheduling, complete Backup recovery points, bounded generation retention, safe pruning, complete current and legacy integrity interpretation, bounded deterministic repair reports, restart discovery, complete typed comparison including extension state, classified findings, exact durable dismissal, real engine history, diagnostics inspection and restore, settings resolution, stable public settings and recovery control, downstream script compatibility, and API-owned local host plus CLI file consumers are implemented; transport-catalog database adaptation remains absent, while the crate intentionally owns no interpreter or source loader |
| `superi-timeline` | [module map](modules/superi-timeline.md) | `open/crates/superi-timeline` | Native editorial project state, media bins and saved queries, metadata and relink state, rational range maps and availability, exact clip retiming, typed tracks, authoritative edit intent, atomic track, caption, marker, and multicam management, exact snapping, clip relationships, atomic foundational, advanced, nested, and multicam operations, OTIO 0.18.1 interchange, versioned integrity-checked state documents, color metadata propagation, and deterministic typed graph compilation | Foundational model, media and metadata state, exact retiming, direct authored time-map and atomic dual-handle transition replacement, typed tracks, all thirteen track, six caption, six marker, and seven multicam gestures, durable caption-track language and purpose, speaker, style, and timeline relationship attributes, caption fragment inheritance, audio-video link, exact sample-clock synchronization, detach, replacement intent transfer, complete channel routing, existing-child nested placement, prepared and selection-derived multi-track compound creation, shared child editing, recursive inspection, multicam source creation, sync provenance, attachment, switching, cut movement, audio intent, and detachment, OTIO interchange, strict revision 2 timeline documents with revision 1 and revision 0 migration, typed output-state graph values, stable editable timeline-to-graph compilation, three-way preservation of nonconflicting direct graph edits, downstream schema-5 project retention and atomic file publication, engine compound history restoration, engine preparation retention, strict public edit-operation, caption-mutation, marker-mutation, and multicam-mutation translation, and canonical-document exposure are test-backed; the production canvas adds durable track, caption, marker, multicam, audio-video, and routing controls, SRT and WebVTT exchange, fresh transcript conversion, complete visible marker and engine-authored angle state, typed inverse marker reversal and history undo, child-timeline clip presentation, transient root-anchored nested navigation, exact placement and compound commands, exact transition inspection and typed timing and graph-parameter commands, application-owned interaction selection, exact transient snapping, visible session rules and consequences, reversible source placement, exact ripple, roll, slip, slide, razor, trim, extend, ripple-delete, and gap plans that publish through lower owners, exact speed, reverse, freeze, and multi-segment time-map authoring, and exact multicam take plus frame cut refinement; broader interchange interpretation, fit-to-fill synthesis, timeline-driven autosave scheduling, graph evaluation, multicam mixing, decoded angle playback, and render integration are absent |
| `tool-superi-dependency-check` | [module map](modules/tool-superi-dependency-check.md) | `open/tools/superi-dependency-check` | Offline executable policy for the open runtime dependency graph | Implemented exact runtime, build, dev, and new-crate checks |
| `tool-superi-boundary-tool` | [module map](modules/tool-superi-boundary-tool.md) | `open/tools/superi-boundary-tool` | Offline scanner for network-client and open-to-closed policy | Implemented library, CLI, workspace gate, and hosted-build command |
| `tool-superi-bench` | [module map](modules/tool-superi-bench.md) | `open/tools/superi-bench` | Stable benchmark harnesses and reproducible stage reporting | Implemented seven-stage runner with real graph evaluation and explicit gaps |
| `tool-superi-fixture-tool` | [module map](modules/tool-superi-fixture-tool.md) | `open/tools/superi-fixture-tool` | Offline fixture validation, generation, and typed golden verification | Implemented validation library, six generators, seven-command CLI, four golden harnesses, and focused contracts |
| `tool-superi-test-report` | [module map](modules/tool-superi-test-report.md) | `open/tools/superi-test-report` | Offline structured platform-lane evidence generator | Implemented strict schema, deterministic findings, collision-safe CLI, and focused contracts |
| `tool-superi-api-bindings` | [module map](modules/tool-superi-api-bindings.md) | `open/tools/superi-api-bindings` | Deterministic committed TypeScript API artifact generator and freshness checker | Implemented pure rendering, idempotent generation, nonmutating freshness checks, complete playback, track, caption, marker, multicam, audio-video, routing, retime, nested placement, and compound contracts, and focused tests |
| `workspace` | [module map](modules/workspace.md) | Repository files outside `open/crates/*` and `open/tools/*` | Product law, architecture, policy, production React and Tauri shell, workspace configuration, fixtures, generated TypeScript artifact, retained frontend consumer, and agent workflows | Active control layer: deterministic checkpoint workflow, explicit revisioned application and headless-engine lifecycle, one durable project lifecycle with versioned active-path and bounded-recent restoration plus native `.superi` startup and macOS association ingress, revision-fenced recovery, atomic project-settings inspection and editing, real media import, durable organization, metadata, annotations, identity, transparent derived-media state, offline recovery, persisted source baselines, removable-volume and changed-file state, explicit relink intent, source-bound editable transcript plus local-content artifacts, deterministic explainable native content search, one ordered atomic batch for rename, organization, generating transcode or proxy evidence, relink, and metadata, bounded on-demand thumbnail, filmstrip, WAVE waveform, and selected-media preview generation, one retained source monitor with exact seek plus fingerprint-bound in and out marks, linked EngineControl and Playback domain owners with one retained durable project editor session and one timing-only exact transport runtime, explicit shell-local seven-service process ownership with retained exit and association tasks plus join-all cleanup, bounded native validation, editor-state, playback, and project-command routing, shell-local lifecycle, native menu, dialog, drag and drop, clipboard-role, recent-document, schema 3 route-local dock, ordered-tab, bounded-size, visibility, focus, versioned configurable keyboard-shortcut, crash, and safe-close continuity, a bounded private native crash journal with cross-session workspace and project continuity plus all four safe recoverability classes, plus one strict geometry and analysis placement command and one separate strict color-selection command, injected generated `SuperiClient`, thin command and event transport, ordered replay and reconnect, cooperative cancellation, deterministic application routing, registry-reconciled dock and tab presentation with accessible drag, keyboard, hide, and separator consumers, shared pointer and keyboard context-menu consumers, described tooltips, bounded notifications, semantic progress, and one operational status and classified recovery center, panel and command registries, conflict-free configurable shortcuts with deterministic import and export, a bounded searchable typed action catalog with stable automation identities and effective shortcut projection into one transient accessible palette, one fixed versioned color-critical dark theme with semantic chrome tokens and separate viewer and marker color-data tokens, immutable shared selection, one public editor-snapshot presentation, generic project-command owner, serialized playback observer, five professional workspaces, source, program, composite, and color native GPU viewers with shell-local single, compare, split, wipe, difference, reference, and snapshot presentation, exact native frame identity and source or playback time capture, live program comparison reporting, and role-aware exact timecode, continuous frame, source, physical dropped-frame, playback-status, nonblocking frame-cache and audio-cache, and editorial-intent displays over existing canonical owners, plus image, alpha, individual-channel, luminance, false-color, and display-linear clipping inspection with distinct selected and last-presented state, exact active-monitor ICC identity and freshness, reversible per-role sRGB or Display P3 GPU presentation, and Program-only exact selected-clip built-in graph transform matrix and sampling controls through the existing revision-fenced project owner, exact play, pause, stop, loop, JKL shuttle, variable speed, reverse, frame-step, seek, and scrub controls with complete temporal, visual, audio, synchronization, comparison, and degradation presentation, and a strict canonical timeline canvas with all thirteen track controls, complete selected caption controls, bounded SRT and WebVTT import and export, fresh transcript conversion, all six durable marker controls, complete visible marker state, exact tracks, ruler, playhead, range, scrolling, zoom, real clip visuals, topology-backed effects, exact transition timing, alignment, duration, and typed parameter controls, positioned clip-gain keys, badges, group and link aware interaction selection, range and lasso selection, roving keyboard navigation, root-anchored nested breadcrumbs and open behavior, cycle-safe append or equal-duration replace placement, deterministic selection-derived compound creation, exact target snapping, six visible session rules, all nine exact edit gestures including four three-point placements and equal-duration four-point editing, exact advanced ripple, roll, slip, slide, razor, trim, extend, ripple-delete, synchronized gap insertion, gap closure, audio-video link, source-time synchronization, detach, replacement intent transfer, complete channel mapping, atomic multicam source setup, synchronization provenance, exact-playhead angle availability, live switching, one-frame cut refinement, authored audio intent, detachment, application-owned exact viewer consequences, complete multicam angle and switch presentation, an actionable audio and video panel, exact speed, reverse, freeze, and multi-segment time-map controls, visible source engine, source, target, affected-object, consequence, pending, and result state, backspace, undo, redo, guides, typed immediate marker reversal, and immediate retime reversal are delivered; point edits, advanced timing batches, caption changes, transition changes, marker mutations, multicam mutations, audio-video edits, routing, retime changes, nested placement, and compound creation enter the retained native editor only through application-owned public command callbacks; crash recovery actions reuse the existing application, lifecycle, and project owners, while private panic detail never crosses the Tauri seam; one GPU submission owner presents role-addressed canonical RGBA16F results through exact monitor bindings and immutable built-in sRGB or Display P3 intent with deterministic analysis views, while arbitrary ICC tag execution remains absent, comparison captures remain exact shell diagnostics rather than retained textures, generated preview artifacts remain ephemeral and CPU-owned, source-monitor state remains separate from decode and presentation, no runtime meter reading crosses the editor snapshot, and the timing-only transport explicitly reports unavailable pixels and audio |

The workspace desktop shell now owns one persistent revisioned window session above Tauri. It
restores one primary and bounded auxiliary workspace webviews, reconciles saved placements against
current monitors, preserves rejected state, coalesces atomic background persistence, and exposes
fullscreen, monitor movement, placement reversal, close, and reopen through a strict System panel
consumer. Each webview retains its own generated transport generation over the one shared engine
and project owner while authored replacement events preserve one global order. Native menus target
the focused editor webview, the main window projects shared project and panel state, and primary
close or process quit uses the one-shot safe-close handshake. Native GPU viewer surfaces remain
attached only to the primary webview until the presenter owns an explicit multi-webview surface
model.

The same application shell owns one schema-1 configurable keyboard-shortcut profile over immutable
command registry defaults. It resolves a unique effective table for live dispatch, sidebar hints,
and the accessible System editor; detects conflicts and reserved native accelerators before mutation;
and round-trips canonical overrides through deterministic JSON. Native desktop schema 3 persists the
profile beside workspace presentation, migrates schema 1 and 2 records to defaults, and recovers an
invalid shortcut profile independently from a valid workspace. The profile remains private
presentation state and never enters authored project, engine history, or generated API ownership.

The desktop shell also owns one strict schema-1 operational capability snapshot. It composes
read-only GPU adapter enumeration, audio input and output declarations, the engine codec registry
through `MediaCapabilitiesApi`, and honest `superi-ai` availability, then exposes live, degraded,
unavailable, and retained observations in the System panel. A bounded private cache preserves
last-known visibility across sessions without becoming device, route, stream, codec, model,
project, workspace, or editable-artifact authority.

The application panel shell now owns one immutable route-local layout above the same registry. Four
stable left, center, right, and bottom docks retain ordered panel identities, active visible tabs,
and bounded sizes while the global hidden set remembers each panel's placement. The real React
consumer exposes labeled tab, hide, dock, drag, pointer or keyboard context-menu, and pointer or keyboard resize behavior, keeps
inactive tabpanels mounted, and projects the complete reconciled layout into both normal desktop
continuity and crash recovery without entering project history, document identity, or safe closing.
Structural placement, order, size, and hidden intent produce an explicit default or custom state.
One all-route reset restores registry defaults and keeps an exact transient undo until later
workspace intent supersedes it. The header reports restoring, saving, saved, or failed primary
continuity, explicit session-only auxiliary state, and polls the native engine lifecycle for an
always-visible status whose detailed controls remain in the existing System owner.
One shared presentation provider now composes that workspace status with lifecycle, public editor,
project, window, desktop shell, retained crash, and public export-job evidence. It preserves safe
actionable context and last-valid state through one always-visible status bar, expandable recovery
and notification center, semantic progress, bounded notifications, and accessible tooltips and
menus. Its actions return to existing application, project, editor, lifecycle, and System owners.

The application shell now also projects registered application commands and current native file,
recent-project, import, history, and quit intents into one bounded searchable catalog. Stable
automation identities, frozen discovery metadata, current availability, and typed delegation keep
the palette, fixed global opener, and focused native Edit menu on existing application and desktop
owners. Query, highlighted result, pending state, and the modal itself are transient and never enter
workspace or project persistence.

The production application now declares one fixed schema-1 `color-critical-dark` theme before
JavaScript and reconciles only its document identity, schema, scene-owner metadata, and browser
color before transport construction. Semantic `--theme-*` tokens own shared chrome, while exact
`--viewer-*` and `--marker-*` color-data tokens preserve surrounds, overlays, comparisons, and
authored flag meaning. Native viewer reservations retain full opacity, normal blending, no CSS
filter, and forced-color protection; scene meaning, RGBA16F precision, monitor binding, display
intent, and transform order remain with the native GPU and `superi-color` owners.

The same shell now owns one read-only seven-service process snapshot above its existing lifecycle.
It retains the application-exit and bounded project-association task handles directly, composes the
existing EngineControl, Playback, worker-pool, GPU, and persistence owners, closes task admission
before teardown, and attempts every join on normal exit and setup rollback. The System panel shows
phases, active and owned counts, pending joins, and thread names without becoming an execution owner.

The workspace viewer shell now owns frozen fit, bounded zoom, directional pan, exact 1:1 pixel,
fullscreen, cinema, and role-addressed external-display presentation intent while preserving the
existing exact playback, caption, editorial feedback, comparison, and overlay boundaries.

The same shell owns a frozen transient image and analysis catalog with `image`, `alpha`, `red`,
`green`, `blue`, `luminance`, `false_color`, and `clipping` codes. React publishes only the selected
code beside placement control, native diagnostics distinguish selected from last-presented state,
and `superi-color` performs every pixel operation on the unchanged canonical GPU texture. Source
inspection precedes the display transform, while clipping intentionally observes display-linear RGB
before transfer encoding and attachment clamping.

The same shell composes a separate frozen presentation-only overlay contract for safe area, guides,
grid, rulers, center, aspect, and custom geometry. Overlay visibility follows the C003 transform
without mutating navigation, pixels, temporal context, comparison intent, external-display intent,
or status ownership.

The same shell now composes a third frozen role-aware viewer-status projection. `superi-project`
owns display rate and timecode mode, `superi-timeline` owns global start, source and record ranges,
identity, grouping, linking, targeting, synchronization, and selection, `superi-engine` owns exact
playback, physical drops, visual, audio, degradation, and failure observations, and the workspace
source monitor owns its exact source coordinate and fingerprint. The workspace converts those
observations with checked integer arithmetic, applies drop-frame rules only to labels, selects the
topmost enabled active video item at the half-open record coordinate, and renders the resulting
display outside both authored state and native placement ownership. Additive cache indicators use
only exact foreground scheduling, due clocks, predictive and output degradation, callback discard
acknowledgement, canonical sample clocks, ordered channels, complete routes, and continuity seams.
They keep fill, hit, occupancy, prediction completion, device output, and audible samples explicitly
unobserved when the public snapshot does not expose them.

The Program viewer also composes one frozen selected-clip transform projection over the existing
typed timeline graph. It validates canonical graph identity, revision, topology, order, drivers,
clip ownership, and exact built-in transform schema before presenting all nine finite matrix values
and nearest or bilinear sampling. Apply and identity reset emit one changed-only typed graph action
through the existing application project owner, while driver-owned values remain inspectable and
read-only and authored matrices never enter viewer-local CSS navigation or native placement state.

Viewer comparison is a third frozen shell-local contract. It admits compare, split, wipe,
difference, and reference only after an exact native reference identity is captured, admits snapshot
only after an exact native snapshot identity is captured, preserves available source or playback
rational navigation time, explicitly labels that context as unbound from the native frame, and keeps
divider position bounded. Every mode follows the same navigation transform and leaves overlays,
display intent, project state, playback commands, and the geometry-only Tauri payload unchanged. The
program viewer publishes one formatted live summary through the existing application owner, while
time-bound native frames, retained GPU reference pixels, and scene-linear native difference
rendering remain an explicit render-result binder gap.

External display routing is a separate frozen shell-local control contract. React selects only an
active connection-local monitor identity, while Tauri excludes the editor window's current display,
rejects a target already owned by another viewer, and routes the same managed canonical role texture
to one borderless external surface on the sole GPU submission owner. The shell reports exact target
geometry, scale, selected and presented analysis, surface generation, frame sequence, display
intent, and unavailable or failed state without resetting navigation, overlays, comparison,
analysis, temporal, visual, or audio context. External presentation failure remains isolated from
inline output, and routing identities do not claim monitor ICC or HDR policy.

Viewer color management is another frozen shell-local contract layered over the same native owner. Each
role selects one exact active monitor and one built-in sRGB or Display P3 transform; the native owner
refreshes the bounded ICC catalog, preserves exact profile content identity or explicit unprofiled
state, rejects stale bindings before acquire and present, hides a profile-changing native child until
the first current-revision replacement frame presents successfully, rejects out-of-order native
replies, and returns immutable diagnostics without ICC bytes or pixels. The canonical ACEScg
scene-linear RGBA16F meaning and transform order remain fixed, while arbitrary ICC tag evaluation is
explicitly not implemented.

Native desktop interaction is a separate presentation boundary over the same application and
project owners. Tauri maps stable File, Edit, and Workspace menu IDs to typed intents, retains
platform clipboard roles, projects a basename-only document title, and persists schema 3 route-local
docks, ordered tabs, bounded sizes, visibility, focus, and configurable shortcuts with schema 1 and
2 migration,
and suppresses duplicate close requests until one resolution grants a single following close event.
The always-mounted React shell uses native dialogs and unambiguous drops, then routes document,
media, workspace, and history intent through existing owners. Its layout header acknowledges native
continuity progress, resets all registry layouts with transient undo, and projects read-only engine
lifecycle state while routing detailed recovery to System. Window close, menu quit, and direct
operating-system exit requests share that safe-close handshake. The desktop project lifecycle
atomically retains its active path and bounded recents between launches, revalidates the active
project through `LocalProjectHost`, and never persists the engine's session-only undo or redo stack.

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
crates plus `superi-fixture-tool`, `superi-dependency-check`, `superi-boundary-tool`,
`superi-bench`, `superi-test-report`, and `superi-api-bindings`. All six tools are built with the workspace but remain
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
  -> superi-project                immutable snapshots, project format compatibility, semantic hash and component diagnostics, checked edit and restoration, authored media and extension commands, durable clip-mix and opaque extension state, autosave, and recovery store contracts
  -> image, color, audio           active playback and display contracts plus native audio plugin state and isolated bridge supervision
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
superi-api-bindings -> superi-api

superi-desktop -> superi-api -> superi-engine
superi-engine -> superi-project -> superi-timeline
superi-desktop -> superi-engine, superi-color, superi-gpu, superi-audio, superi-ai,
                  superi-concurrency, superi-core
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
adjacency, handles, record placement, grouping, synchronization, persistence, and mutation. The
desktop now pairs canonical transition timing with downstream effect schemas through public editor
state, and submits visual values through generic graph mutations without reversing this dependency.
Timeline and cache also consume the graph-owned color metadata wrapper,
but no timeline path consumes graph evaluation, documents, animation curves, the effects catalog,
or a production runtime factory.

Codec implementations depend down on the codec-neutral `superi-media-io` interface. Media I/O does
not depend on a concrete codec, engine, or registry assembler. The engine owns the current assembly
choice. The API depends on engine-owned projections rather than leaking media-I/O implementation
types. The CLI depends only on the API for editorial control and never imports engine scenario
state directly.

## Public control flow

### Implemented today

The production desktop lifecycle is implemented without creating a second engine behavior path:

1. `ApplicationLifecycle` wraps the shared `LifecycleCoordinator` and records one explicit
   application intent plus a monotonic headless-engine generation.
2. `HeadlessEngineLifecycleParticipant` exposes the exact current signal, acknowledgement, and
   classified failure seam. Every completion is token fenced, so stale work cannot advance a newer
   restart or recovery generation.
3. `LinkedEngineProcess` retains one full `EngineCommandDispatcher` per application generation on a
   dedicated EngineControl thread, one exact timing-only `PlaybackControlRuntime` on a dedicated
   Playback thread, and one shared bounded worker pool. The two domains communicate only through
   the existing capacity-bounded dispatcher bridge and consume lifecycle state through signal,
   acknowledgement, and failure operations. Partial startup joins already-created owners, while
   shutdown attempts both domain joins and every bounded worker before returning the first failure.
4. Tauri manages the lifecycle owner, fixed-capacity `EngineConnection`, strict persistent
   `DesktopWindowState`, and one `DesktopProcessRuntime` with seven ordered service records. It
   retains the exit monitor and bounded association tasks, receives explicit engine, GPU, and
   persistence reports, and exposes a read-only process snapshot. It restores
   primary and auxiliary workspace webviews before steady state, and joins the window persistence,
   GPU, engine, worker, association, and exit owners after the native host stops. Setup failure uses
   the same join-all rollback. Primary exit requests record orderly shutdown without blocking the
   main thread.
5. The connection admits typed integration-validation, complete editor-state, playback, and generic project
   command requests with `try_send`; response waiting stays with a blocking-safe caller, restart
   constructs a fresh dispatcher, and outstanding internal subsystem initialization remains
   truthfully visible. One route-fenced `ProjectEditorApi` session stays on EngineControl so command
   history is retained without moving the non-Send dispatcher across domains.
6. The native transport routes those four generated methods through that managed connection and
   scopes generation, pending, cancellation, replay, and disconnect state by invoking webview label.
   Successful authored commands compare and replace the exact active project database, refresh
   desktop lifecycle identity, and emit every correlated project event in monotonic envelopes to
   each connected webview with its current generation.
   Cancellation wins before durable work begins, while committed commands remain visible after late
   cancellation. Transport retains 64 events for cursor replay and converts only reviewed failure
   context to `PublicApiError`.
7. React renders the serialized application and engine phases, pending acknowledgement, safe failure,
   and recovery affordances through the two shell-local lifecycle commands. A bounded read-only
   header poll keeps engine phase visible from every route and opens System for detailed control.
   The System panel separately polls process ownership and renders each service phase, unit count,
   pending join, and exact thread name. It independently renders exact persistent window, monitor,
   focus, fullscreen, previous placement,
   persistence, recovery, and recently closed state through strict window-session commands.
   In parallel, one shell-local crash owner writes an exact active-session marker and a bounded
   replacement-safe journal under application data. Startup converts a surviving marker into an unexpected-exit
   record; a chained panic hook retains private detail only in that journal; lifecycle failures add
   safe classified context; and orderly acknowledged shutdown removes only the matching marker.
   React publishes validated primary-window route, dock, ordered-tab, active-tab, size, visibility,
   and focus continuity only after native window restoration hydrates its route, publishes active-project continuity through a separate ordered
   queue, receives no private panic detail, and routes restoration, retry, degraded continuation, project recovery, and restart through
   the existing application, lifecycle, and project owners. The same panel also
   renders strict read-only GPU, audio, codec, and AI observations through a separate retained
   shell-capability owner without starting runtime work or changing user state.
8. Separately, `app/src/api.ts` re-exports the complete generated public contract and constructs a
   frozen `SuperiClient` binding around the concrete `DesktopSuperiTransport`. Every webview owns
   its local client instance and generation while the native transport preserves one authored event
   order. Invoke/listen delivery, fencing, replay, reconnect, cancellation, and classified failure
   projection remain transport-owned; the provider and React consumer own no engine behavior.
9. `app/src/application.ts` defines deterministic routes, panels, commands, immutable four-dock
   route layouts, ordered and active tabs, bounded sizes, hidden placement, registry reconciliation,
   structural default or custom status, deterministic all-route reset, one exact transient undo,
   complete private continuity projection, and one immutable typed public-resource selection without
   importing transport or engine owners. `app/src/panel-workspace.tsx` is the accessible real
   consumer, opens the shared pointer or keyboard context menu for activate, dock, and hide intent,
   and dispatches only presentation actions.
10. `ApplicationProvider` composes that model above the injected generated client. The shell routes
    generic workspace and system panels through the registries, and async commands delegate through
    `SuperiApiBindings` while local routing and selection remain responsive and nonauthoritative. It
   also owns project and playback transaction identity, the visible revision fence, response correlation,
    classified failure retention, durable generic project-action execution, and complete editor-state
    refresh after authored track, transition, graph, or marker edits. Accepted playback commands are
    observed to completion through the same replacement-state query without a second React clock.
    One outer `ApplicationPresentationProvider` normalizes only already classified public or
    shell-reviewed lifecycle, editor, project, window, shell, crash, workspace, and job evidence.
    It renders the shared status, notification, progress, tooltip, menu, and recovery surfaces while
    routing commands back to existing owners and retaining the last-valid workspace and editor view.
11. The same `ApplicationProvider` retains one last-valid `EditorStateSnapshot` for presentation,
    refreshes it through generated project, audio, and job events, and projects it across five
    registry-backed professional workspaces without taking engine or transport ownership. The
    delivery workspace exposes each public export job as determinate or indeterminate semantic
    progress and displays attached failure category plus recoverability without adding polling.
    The editing workspace receives typed selection state and the typed project action callback and
    never imports transport. Marker creation, range, label, flag, note, and removal use that callback
    and retain one complete inverse batch only at the exact resulting revision. The editing viewer
    also renders exact play, pause, stop, loop, JKL shuttle, variable speed, direction, and frame-step
    controls plus temporal, visual, audio, synchronization, comparison, and degradation state. Every
    native viewer role additionally projects exact record timecode and continuous frame identity,
    role-aware source state, physical playback drops, scheduling, nonblocking frame-cache and
    audio-cache evidence, failure, and canonical editorial intent from that same snapshot and
    retained source monitor without adding a command, timer, poll, or state owner.
12. The editing timeline maps exact current-revision object identities into that existing shared
    application selection. Click, toggle, range, lasso, and keyboard gestures follow canonical
    groups and enabled links or preserve direct-object intent, while focus and lasso geometry remain
    transient and authored timeline state remains below React. One selected transition projects its
    exact handles, adjacent capacity, duration, alignment, graph effects, drivers, and supported
    typed parameters; timing and parameter gestures enter the existing generic project command
    through the application-owned callback and return through the refreshed canonical snapshot.
13. Editing reserves source and program native rectangles, compositing reserves composite, and color
    reserves color; React publishes only role, stable analysis code, geometry, scale, visibility,
    optional connection-local external display identity, and status through the shell-local command.
    Tauri owns all four child windows plus one hidden borderless external window per role, excludes
    the editor window's current display, rejects target conflicts, and distinguishes selected
    analysis from nullable last-presented analysis for both destinations. The sole GPU submission
    domain configures inline and external surfaces and presents the same managed canonical
    `Rgba16Float` role result through the selected display transform without readback. External
    surface failure remains explicit and does not stop inline output. A separate strict color command carries
    only role, exact monitor ID, and sRGB or Display P3 transform ID. Tauri owns all four child
    and external windows, distinguishes selected analysis from nullable last-presented analysis, and
    owns the shared bounded system-profile catalog. The sole GPU submission domain verifies immutable profile
    bindings before acquire and present, then presents managed canonical `Rgba16Float` results
    through the selected `OutputColorTransform` and `GpuDisplayView` without readback. Image, alpha,
    channel, luminance, and fixed false-color inspection operate in source scene-linear space before
    display conversion; clipping classifies display-linear RGB before transfer and attachment
    clamping. Both DTOs reject every unknown field, and focused contracts keep ICC bytes, frame
    bytes, image conversion, blob URLs, pixel readback, and texture handles outside IPC while fixing
    scene meaning, precision, alpha, transform order, display intent, analysis identity, and actual
    offscreen pixel parity.
    Viewer navigation, overlays, status, and editorial feedback remain DOM presentation outside the
    native placement payload; the status list is also outside the transformed frame so every fit,
    zoom, pan, pixel, fullscreen, cinema, external-display, paused, playing, scrubbing, and ended
    mode retains exact temporal, visual, audio, cache, and comparison evidence. The
    external status additionally reports target geometry, scale, selected and presented analysis,
    surface generation, frame sequence, display intent, and unavailable or failed state.
14. A separate Tauri-owned project lifecycle calls `LocalProjectHost` for durable create, open,
    save, save-as, validation, recovery, settings inspection, editor inspection, atomic settings
    transactions, and timeline command execution. It
    commits active and bounded recent presentation only after success, retains revision-fenced
    recovery candidates and reviewed actionable failure context, and exposes complete lifecycle and
    settings state to one typed System-panel consumer. Exact `.superi` startup arguments and macOS
    resource-open URLs enter that same Open transition on a blocking worker, emit complete
    replacement state, and restore the main window without reloading React workspace state.
15. The same desktop owner projects imported media into one durable presentation store for bins,
    metadata, annotations, identity, selections, source-fresh derived attachments, availability,
    relink, and local search. It also persists source-bound content analysis beside stable imported
    identities. Revision-fenced native search composes metadata, editable transcript timing and
    speaker state, timeline plus clip relationships, and local AI content into bounded deterministic
    evidence without taking model-runtime or network authority. C012 adds one bounded ordered batch:
    React supplies selected stable IDs, the typed bridge attaches project and library revision fences,
    native code evaluates every rename, organization, optimized transcode or proxy record, relink,
    and metadata operation on a clone, and one failure discards the candidate while success advances
    and persists one revision. Runtime-only usage, duplicate grouping, smart membership, thumbnail,
    availability, and resolved fallback reach the consumer but remain absent from sidecar authority.
    C013 attaches persisted accepted source baselines, current observations, conservative volume
    identity, scan generation, path state, and relink intent to those same stable media identities.
    Import captures exact per-file baselines during its existing byte pass. Revision-fenced all-media
    scans skip stable bytes by metadata signature, selected scans can force exact hashing, filesystem
    loss becomes inspectable state, and changed bytes never rewrite identity or bind automatically.
    Actual transcode byte production remains with lower engine and codec owners.
16. That project lifecycle also resolves one selected media identity under exact project, library,
    media, and fingerprint fences before releasing its lock for bounded blocking generation. Still
    and image-sequence products use lower dense-image and aspect-fit scaling contracts; WAVE products
    use exact PCM container, decoder, continuity, sample-clock, and ordered-channel contracts. The
    typed bridge and React inspector discard stale responses, retain only ephemeral data URLs, and
    expose unsupported video, compressed audio, EXR, and DPX products as explicit unavailable states.
17. The lifecycle retains one separate source-monitor runtime behind the same project, library,
    media, and fingerprint fences. Container and PCM media open through the engine source-only
    registry, image sequences use a verified inclusive range, and every seek uses exact rational
    time. Scanner-confirmed changed bytes make the retained session stale and fence later source
    operations. Fingerprint-bound in and out marks publish atomically with the media sidecar, while
    React exposes ready, stale, empty, seek, mark, clear, and unload state without treating source
    open as decoded-frame or native GPU presentation.
18. The editing timeline converts the ready source monitor's inclusive marks into exact half-open
    operation ranges, chooses an explicit track plus playhead, timeline range, or selected item
    target, and plans only the fragment identities required by the existing timeline owner. Insert,
    overwrite, append, replace, lift, extract, backspace, all four three-point placements, and
    equal-duration four-point editing submit one generated project command with visible source
    engine, target, and consequence state. Exact clock conversion and retained bounds are required,
    unsupported fit-to-fill is explicit, undo and redo use the existing history commands, and every
    success refreshes the complete editor snapshot instead of patching React state locally.
19. One directly selected clip exposes an exact local retime draft for normal speed, signed rational
    speed, reverse, freeze, or a contiguous multi-segment time map. The visible curve, source
    traversal, target, and consequence derive from the same validated draft. Apply submits one
    generated `TimelineEditOperation::Retime` through the existing compound project transaction,
    graph recompile, persistence, event, command-log replay, undo, redo, and replacement-snapshot
    path; reset and Escape discard only the presentation draft.
20. The timeline's exact transient plan and strict canonical clip and audio detail feed one deeply
    frozen editorial-feedback replacement owned by `ApplicationProvider`. Source and program
    viewers distinguish trim, slip, and slide consequences and expose complete multicam angles,
    switch ranges, and audio policy outside native child placement. The audio rack retains sample
    clocks, ordered channels, routes, audibility, and exact continuity seams. Because the editor
    snapshot has no runtime meter readings, signal status remains explicitly unobserved and no
    numerical level enters the React or Tauri placement contract.
21. Exactly one selected video clip and one selected audio clip expose link, exact source-time
    synchronization, detach, and audio replacement in one focused panel. Link and detach preserve
    both clips' timing, synchronization translates the complete audio source map at its sample clock
    while preserving record placement, and replace reuses the existing source-monitor path. Per-track
    channel controls retain the canonical sample rate and source layout and submit one complete main
    or audio-track destination map with one channel or mute target per source channel. Every gesture
    uses the application-owned project action or exact command executor and returns only through
    native validation, history, persistence, event, and complete snapshot refresh owners.

Media registry construction and capability introspection are implemented as follows:

1. `superi-engine::media` creates a `BackendRegistry` and registers the default Rust codecs.
2. The `os-codecs` feature may append host-discovered platform codecs.
3. Engine construction creates and preflights primary priority-100 registrations for the in-tree
   Matroska or WebM, MP4 or MOV, MXF, and PCM container sources before inserting any of them.
4. `source_backend_registry` reuses that exact source registration path without constructing codec
   factories, giving the desktop source monitor one complete source-open registry without a second
   backend list or a vendor-codec dependency.
5. The separate vendor constructor may append only explicitly configured vendor workers.
6. `superi-engine::introspection::MediaCapabilities::from_registry` reads declarations without
   opening sources or constructing codecs, then produces deterministic engine-owned records.
7. `superi-api::MediaCapabilitiesApi` projects those records into strict serializable API types.
8. `GetMediaCapabilities` clones the current full snapshot. `synchronize` emits one full-replacement
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

Process-lifetime extension registration and capability discovery are implemented without an engine
backdoor:

1. `superi-engine::extensions::ExtensionRegistry` accepts bounded declarative registrations under
   exact versioned or native-audio identities. Distinct versions coexist, duplicate identities fail,
   and equal full synchronization does not advance the registry revision.
2. Registrations retain requested and granted capabilities separately and validate one core-owned
   `FeatureDiscovery` record. Only Ready registrations may advertise Available features.
3. The existing OpenFX and native audio supervisors expose canonical read-only status lists. Engine
   adapters project those lists into shared registration state without moving workers, launchers,
   callbacks, factories, paths, processors, or dispatchers into the registry.
4. Faulted and quarantined registrations expose only category, recoverability, bounded stage and
   attempt evidence, and one stable recommended user action. Raw diagnostics and source chains stay
   private.
5. `superi-api::ExtensionRegistryApi` exposes permission-free query `superi.extensions.get`, full
   replacement event `superi.extensions.changed`, and resource `superi.extensions` with strict wire
   validation and change-only publication.
6. Every registration points user control to existing method `superi.project.command.execute` and
   resource `superi.editor.state` for upsert, removal, lifecycle, grant, failure, and failure-clear
   operations. Actual mutation remains permission checked, revision fenced, persistent, undoable,
   scriptable, and recoverable through the project owner.

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

Public schema discovery describes every current API-owned contract without inspecting private
engine enums or creating another state owner:

1. Every `ApiCommand` declares its permanent method, command or query kind, semantic schema version,
   permission requirement mode, possible permission kinds, and exact requirement derivation with no
   default classification. Every `ApiEvent` declares its permanent event and payload version, while
   `ApiResource` centrally names the 13 current replacement resources.
2. One explicit registration list builds schema references for all 16 commands, 14 queries,
   nine events, and 13 resources, including discovery method `superi.api.schema.get` itself.
3. `PublicApiSchemaSnapshot` fixes catalog schema `1.9.0`, stable primitive revision 1, and JSON-RPC
   `2.0`; sorts every category by permanent name; and rejects duplicate names, command-query
   overlap, malformed names, or incompatible identity.
4. The catalog includes the complete core-owned error category, recoverability, capability, and
   permission vocabularies plus exact method permission metadata. `PublicApiError` derives safe presentation from `UserSafeError`, copies only
   explicitly user-safe diagnostic fields or reviewed caller context, and may retain one last-valid
   public resource reference without copying mutable state ownership.
5. Strict typed JSON-RPC request, success, and failure values provide data framing only. They do not
   route arbitrary JSON or start a network server.
6. `superi-cli api schema` consumes `PublicApiSchemaApi` directly and prints the exact canonical
   snapshot without importing engine or rebuilding catalog declarations.

Public version negotiation is stateless and does not create a runtime downgrade owner:

1. `superi-project` owns one authoritative application, text format, primitive revision, and schema
   0 through 4 semantic release table plus typed compatibility outcomes and migration paths.
2. `superi-engine::editor` reexports that vocabulary without adding behavior or another table.
3. `superi-api::NegotiateApiVersion` validates bounded nonempty strictly ascending API and primitive
   offers, and `VersionNegotiationApi` selects the highest canonical common values.
4. Optional project evaluation runs independently and returns API-owned strict dispositions,
   reasons, current target, and exact migration successors.
5. `superi.api.version.negotiate` is permission-free schema `1.0.0`, emits no event or resource,
   performs no I/O or state change, appears in the catalog and generated TypeScript method map, and
   is consumed by CLI discovery plus the frontend compile-time contract.

Public event delivery orders every current typed producer without replacing its engine or facade
owner:

1. `PublicApiEvent` is a closed union with exactly the nine cataloged event payloads. Publication
   rejects mismatched event and replacement-state revisions and retains either command correlation
   with source event sequence, command sequence, and caller transaction identity, or observation
   correlation with the authoritative snapshot revision.
2. `EventStreamApi` allocates one independent nonzero public sequence, retains complete immutable
   records in a bounded deque, and registers subscriber identities in a separate bounded set. It
   never changes an engine queue or existing facade drain.
3. `superi.events.subscription.open`, `superi.events.subscription.close`, and
   `superi.events.subscription.poll` form one schema `1.0.0` transport-neutral control surface.
   Subscribers hold their own cursors, and repeated polls are non-destructive and idempotent.
4. One poll is capped by caller and configured batch bounds. A future cursor fails. An evicted
   cursor or changed stream identity returns no partial records and instead returns an explicit
   reset barrier plus all eleven authoritative state resources and their exact query or typed inspect
   command paths.
5. The subscription resource is reestablished through open, close, and poll and is therefore not in
   the state resynchronization manifest. Network hosting, dynamic routing, push delivery,
   authentication, and persisted replay across process lifetimes remain absent.

Host-injected public authorization is one pre-dispatch boundary over the same typed commands:

1. `ApiPermissionContext` is nonserializable host authority. Empty policy denies every protected
   operation, every requirement needs an allow, and any matching deny overrides allows.
2. Filesystem rules use validated project-relative or declared Unix, Windows drive, or Windows UNC
   targets with exact or component-recursive lexical scopes. This does not claim symlink confinement;
   the actual I/O owner still applies handle and operating-system containment.
3. Plugin rules scope durable state, lifecycle, and explicit capability delegation by exact or all
   canonical extension identities. Delegation is allowed only within the rule's canonical ceiling.
4. Destructive rules separately scope job cancellation and removal, recovery restore and dismissal,
   and audio automation removal. Payload-dependent commands union every nested requirement before
   authorization.
5. Project editor, scenario, jobs, recovery, automation, and settings facades authorize before
   conversion or engine dispatch. Denial advances no command sequence, state revision, history,
   durable recovery file, or event stream and exposes no path or plugin identity.
6. `superi-cli` binds only an exact read grant for its resolved canonical scenario fixture. Schema
   discovery reports authorization metadata but creates no authority.

Public asynchronous job control projects the canonical engine export queue without adding another
scheduler:

1. `AsyncJobsApi` owns one dispatcher with the existing engine export queue attached. It maps only
   `InspectAll`, pause, resume, retry, cancel, cancel-all, and finalized removal into schema `1.0.0`
   query and command contracts.
2. Complete replacement snapshots expose canonical handles, stable kind and 8:4:2:1 priority
   vocabulary, every queue state, attempt count, coherent unit progress, deterministic dependencies,
   reviewed safe failure data, result availability, retry eligibility, and finality.
3. Query and control are nonblocking on EngineControl. A host control loop uses the noncataloged
   runtime poll seam so worker progress, cancellation acknowledgement, and completion enter the same
   ordered `ExportJobsStateChanged` dispatcher envelopes.
4. `AsyncJobsApi` verifies matching event and state revisions and publishes `superi.jobs.changed`
   full replacement events in canonical handle order. Raw failures, executor bindings, control
   tokens, and typed artifacts never cross the API boundary.
5. Public submission, runtime polling as a wire method, waiting, typed result retrieval, queue
   persistence, muxing, and file publication remain absent.

Whole-project in-memory publication is implemented as follows:

1. `superi-project::ProjectDocument::new` accepts one validated editorial project and selected root
   timeline, calls the timeline compiler, derives deterministic project settings from the root edit
   rate, and retains the complete compilation with its provenance and editable graph at document
   revision zero.
2. `ProjectDocument::edit` requires the exact current document revision, clones one private
   candidate, and exposes ordinary editorial, settings, clip-mix, graph, and extension mutation only
   through `ProjectDraft`.
3. Publication validates the selected root, graph identities, unique compiled roots, standalone
   names, shared project identity, exact project settings, authored clip-mix validity and clip
   membership, bounded extension envelopes and unique compound identities, and exact
   editorial-to-compilation revision. A stale revision, failed closure, or
   invalid candidate publishes nothing; an unchanged candidate does not advance.
4. Successful changes advance once and replace one shared `Arc` state. Cloneable immutable
   `ProjectSnapshot` values preserve prior revisions for editor, script, headless, persistence, and
   API and engine consumers.
5. Intelligent or generated output has no hidden channel. It remains an ordinary typed parameter
   or node in a retained timeline compilation, ordinary editorial state, or an explicitly named
   standalone editable graph. An AI extension record may preserve supplementary provenance and
   lifecycle metadata but not replace the editable artifact.
6. Plugin, auxiliary effect, AI artifact metadata, and unknown future state use one open namespaced
   `ProjectExtensionRecord` envelope with exact opaque bytes, requested and user-granted
   capabilities, user-controlled lifecycle, and optional structured failure. One revisioned command
   surface performs upsert, remove, lifecycle, grant, failure, and clear operations, and equal
   commands remain semantic no-ops.
7. For later durable loading, `ProjectGraph::restore_timeline` recompiles trusted provenance and
   installs an externally decoded editable graph only when its deterministic graph identity matches
   the same project and root. `ProjectDocument::from_complete_parts_with_settings_and_extensions`
   then joins decoded settings, authored clip-mix state, and exact extension records and validates
   the complete aggregate at the stored document revision without owning bytes, migrations, or file
   I/O.

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
   authored project media commands, every project extension command, project settings transactions,
   and one bounded compound transaction.
2. A successful semantic media, extension, settings, or compound mutation records complete
   immutable before and after snapshots plus stable mutation kind. The default capacity is 64, the
   accepted maximum is 4096, and full capacity evicts the oldest undo entry.
3. Failed commands and semantic no-ops preserve undo and redo. A successful new branch clears redo,
   while undo and redo move an entry only after `ProjectDocument::restore_snapshot` accepts the
   complete target behind the exact current revision fence.
4. Restoration validates project identity and the full aggregate, then publishes selected old
   contents at a fresh monotonic document revision. Stale input, invalid state, empty branches, or
   revision exhaustion changes neither the project nor history.
5. `EngineCommandDispatcher` can attach exactly one project-history owner. Typed history execution
   and inspection expose the selected snapshot and branch metadata. Every successful generic apply,
   undo, redo, inspect, and semantic no-op reserves event capacity, atomically appends one sequenced
   project command record, and publishes a correlated `ProjectStateChanged` event. Extension
   commands retain their typed result through that replacement event, while settings changes also
   publish a correlated `ProjectSettingsChanged` event. Any failure preserves project, history,
   command log, and events together.
6. A compound transaction applies one to 64 ordered root, timeline, track, marker, nested
   placement, selection-derived compound, graph, media, extension, or authored audio actions inside
   one outer project edit. Timeline-owned actions reconcile retained graphs through a three-way
   compile, clip-mix reconciliation preserves moved identities, every action validates the draft,
   and a late failure rolls back everything.
7. Undo and redo stacks plus their capacity remain session-local operational state. Schema-5
   persistence durably stores the selected authored snapshot and a separate bounded command log,
   including exact compact request bytes when safe to retain and SHA-256 evidence otherwise. The
   generic API adapter, bounded local scripting consumer, durable local CLI execution, and cursor
   query are implemented, while persisted undo and redo branches, network hosting, and full-catalog
   automation remain later work. Complete editor replacement state is exposed separately through
   the same public facade.

Generic public project editing composes that history without introducing another owner:

1. `superi-api::ProjectEditorApi` owns one dispatcher with an attached authoritative project
   history and exposes permanent schema `1.7.0` command `superi.project.command.execute`.
2. Strict API-owned DTOs cover apply, inspect, undo, redo, every current project action family,
   all 20 timeline edits, all thirteen track mutations, all six caption mutations, all six marker
   mutations, all seven multicam mutations, all eight graph
   mutations, all three media mutations, all four clip-mix mutations, all six extension mutations,
   existing-child nested placement, and selection-derived compound creation. The retime DTO carries
   the complete exact public clip time map. Unknown fields and operation tags fail closed.
   Audio-video link, synchronization, and detach carry both exact track and clip roles. Complete
   routing carries one main or audio-track destination, an ordered destination layout, and one
   semantic channel or mute decision per source channel.
3. The API converts every identifier, exact time, exact retime segment and seam, material, graph
   value, media path, clip control, and extension record before dispatch. Engine exposes only a
   curated checked construction seam,
   so API retains its exact dependency tier and lower crates do not become wire-schema owners.
4. One public apply becomes exactly one `CompoundProjectTransaction` and one history command.
   Successful authored changes remain one project revision, one undo unit, and one correlated
   `superi.project.state.changed` event; inspect and failures emit no event.
5. Results and events preserve caller transaction identity, command and event sequences, project
   revision, minimum replacement history state, and ordered semantic action evidence. The bounded
   `superi.project.history` replacement remains separate from the complete read-only editor state
   returned through `ProjectEditorApi`.
6. Nested placement carries one existing child identity, exact source range, caller-owned parent
   clip identity, and all four placement modes. Selection-derived compounds carry the complete
   selected object set plus caller-owned child-track and parent-instance identities in canonical
   track order. Both return strict typed action evidence and preserve the same request recording,
   event, persistence, undo, and redo behavior.
7. The in-process persistence consumer receives the selected immutable snapshot. A real mixed API
   contract proves database replacement and exact reload, then public undo and redo at fresh
   revisions. Existing recovery commands continue to operate on the same durable aggregate.

Bounded local project scripting composes that same public facade:

1. `superi-api::RunProjectScript` exposes permanent command `superi.project.script.run` at schema
   `1.0.0`. The request carries exact UTF-8 source and its required lowercase SHA-256 digest.
2. `superi-json` programs declare the exact language and version, one canonical script identity, an
   initial project revision fence, and one to 256 ordered steps. Source is capped at 1,048,576
   bytes, identities at 128 bytes, and JSON nesting at 128 levels; duplicate object members,
   unknown fields, unsupported methods, future versions, and alternate digests fail before dispatch.
3. The closed step union reuses only `superi.project.command.execute` and
   `superi.editor.state.get`. Complete nested filesystem and plugin requirements are aggregated and
   authorized before any step, then each step retains its ordinary checked conversion, dispatcher,
   history, event, and lower-domain behavior.
4. The initial revision conflict rejects all steps. A later failure stops the suffix and preserves
   any earlier independently committed prefix and its ordinary events. The typed trace exposes
   completed records, failed index, safe failure, initial and final revision and semantic hash,
   status, and explicit committed-effect evidence.
5. Equal exact source against an identical complete initial runtime state has deterministic
   interpretation. Source loading, file confinement, module loading, arbitrary code, process or
   network access, hidden retries, and whole-script atomicity are not part of the runtime.
6. Real contracts prove media identity and fingerprint preservation, SQLite reopen, integrity,
   autosave discovery and comparison, recovery restoration, conflict visibility, and permission
   denial through the existing project, engine, and API owners.

Durable local project control composes the API and project owners without changing dependency
direction:

1. `superi-api::LocalProjectHost` owns one operation at a time. It completely loads the project,
   enters scoped EngineControl through the engine helper, attaches the document, and executes the
   existing editor, settings, or recovery facade.
2. A semantic mutation drains its correlated replacement event, detaches the selected immutable
   snapshot, and calls the existing project database publication path before returning success. A
   no-op does not manufacture a save, while permission, stale revision, conversion, dispatch, event,
   or persistence failure emits no success.
3. Creation uses production constructors plus an in-memory database and no-clobber save-as. Explicit
   save and save-as reuse collaborative-safe publication and collision rules; copy, backup, and
   recovery reuse existing project commands and active-database ownership. Validation is a
   read-only complete current-schema reconstruction.
4. The desktop project lifecycle retains active identity, bounded recent paths, recovery
   presentation, actionable failure state, and a projection of the durable project settings owner.
   A private versioned record atomically retains the active path and last-known recent identities,
   revalidates the active document through the local host on launch, and keeps recents plus failure
   evidence when the active path is missing. Its React consumer receives complete replacement
   snapshots, never supplies recovery artifact paths, and edits settings only through
   revision-fenced project transactions.
5. `superi-cli` retains paths as operating-system values and consumes only API DTOs. Media and
   timeline commands enforce exact action partitions; render inspection returns editor and settings
   state from one loaded revision; render configuration accepts only existing render setting keys.
6. Automation accepts one strict JSON-RPC `2.0` request per bounded JSONL line, echoes string or
   integral numeric IDs, publishes each independent request durably, flushes its response, then
   advances. A later failure stops without rolling back earlier acknowledged lines.
7. Session undo and redo stacks still do not enter SQLite or either desktop presentation record.
   The native shell exposes their current depth only for menu enablement and warns before a save and
   close ends either undo or redo state in that session. The CLI and desktop lifecycle control
   selected durable state but do not claim a persisted command journal, render submission,
   container muxing, or network server.

Stable whole-project serialization is implemented at the same owner boundary:

1. `ProjectDatabase::create` reserves a new path without overwriting, or `memory` creates an
   equivalent in-memory database. Both establish SQLite application ID `SUPR`, schema revision 5,
   semantic format `superi.project` version `1.4.0`, and exactly eight strict tables. File-backed
   create and open also capture one stable complete-byte generation for later conflict detection.
2. Preparation serializes the editorial owner through the canonical timeline codec, the validated
   settings snapshot to bounded canonical JSON, every retained graph through the canonical graph
   codec in stable `GraphId` order, and authored clip-mix state through the strict audio codec before
   filesystem mutation. Rows retain component revisions, ownership, exact lengths, SHA-256 values,
   and a domain-separated project manifest over all version, identity, revision, settings, and
   ordered component evidence. Each extension row retains canonical strict revision-1 metadata and
   exact opaque payload bytes under independent lengths and SHA-256 values; the manifest covers
   their stable compound identity and ordered evidence. Separate command-log metadata and record
   tables preserve monotonic sequence, typed command kind and outcome, bounded request disposition,
   exact retained bytes when present, and request SHA-256 evidence without entering authored
   semantic identity.
3. In-memory `replace` writes semantic rows in one immediate transaction and requires exact snapshot
   reload before commit. File-backed `replace` delegates to the same public `Save` command used by
   interactive, script, and headless callers.
4. `ProjectDatabase::execute_save_command` builds one complete schema-5 SQLite candidate in the
   destination directory, requires exact semantic reload and full integrity after candidate commit,
   closes the SQLite handle, and synchronizes the candidate before publication.
5. `Save` replaces the active file, `SaveAs` publishes and rebinds active identity plus generation
   at the commit point, `SaveCopy` publishes without rebinding, and `Backup` always requires an absent
   destination. Save-as and copy expose explicit require-absent or replace-existing collision
   behavior.
6. Replace-existing accepts only a regular validated Superi project, acquires one nonblocking
   operating-system lock through a deterministic persistent hidden sibling entry, revalidates the
   command-start destination generation and active generation while locked, and publishes by
   same-parent rename. A held lock or changed destination is retryable; a stale, missing, corrupt,
   symlinked, or nonregular active file is user-correctable and never overwritten. Unsafe or
   unsupported lock state fails closed.
7. Require-absent atomically claims the destination name without clobbering it. Active-path aliases,
   destination appearance or replacement races, partial candidates, and prepublication faults never
   become a successful save; postpublication faults report that the new file is already authoritative
   instead of claiming rollback. Distinct SaveAs, SaveCopy, and Backup remain explicit escape paths
   after an active-file conflict.
8. `open_read_only` applies defensive, query-only connection policy and validates database identity
   and exact schema objects. File-backed `load` requires the active generation captured by create,
   open, save, or save-as both before and after its short-lived coherent read transaction, so a
   collaborator change is visible and no partial or stale state is returned.
9. Timeline owns `superi.timeline` meaning and strict `TimelineGraphValue` Serde, graph owns
   `superi.graph` meaning, audio owns `superi.clip-mix` meaning, and project owns settings plus the
   extension envelope, normalized container, manifest, aggregate reconstruction, active path, and
   file publication. Direct graph edits, settings, authored audio, opaque extension bytes, document
   revisions, project-relative meaning, and command-log sequence plus replay evidence survive reload
   and save-as.
10. `ProjectDatabase::open` validates current schema 5 without mutation or migrates exact schema 0,
   1, 2, 3, or 4 through the contiguous 0-to-1-to-2-to-3-to-4-to-5 registry. Schema 0 first reconstructs
   and writes
   the frozen schema-1 representation, schema 1 derives deterministic settings from the selected
   root rate and writes frozen schema 2, schema 2 adds canonical empty clip-mix state in frozen
   schema 3, schema 3 adds an empty extension set in frozen schema 4, and schema 4 adds an empty
   command log before writing schema 5 with exact snapshot equality, all inside one immediate
   transaction.
11. A wrong application, future schema, unsupported format, malformed component, or forced failure
   after schema rewrite leaves the source unchanged. Writable open reports its source revision;
   read-only legacy open requires migration instead of partially interpreting old state.
12. History stacks remain session-local, while the project-owned command log is durable and outside
   authored semantic revision and recovery candidate state. The API-owned local host and CLI compose
   public database open, no-clobber create, save-copy, backup, validation, recovery, mutation, and
   command-log query workflows over this authority. The API-local script runtime consumes immutable
   snapshots and existing commands without adding a project-owned interpreter. Recovery discovery
   and restoration compose this database authority through project, engine, and API owners while
   preserving the active log lineage.

Deterministic semantic project hashing reuses canonical preparation without reusing file identity:

1. `superi-project::ProjectDiagnostics::from_snapshot` invokes the same bounded
   `PreparedProject::from_snapshot` path as persistence, so timeline, settings, clip-mix, extension
   metadata, opaque payload, and retained graph evidence comes from each existing canonical codec.
2. Public components are immutable and ordered as timeline, settings, clip mix, compound extension
   identity order, then `GraphId` order. They retain codec revisions, canonical lengths and SHA-256
   digests, extension payload schema, and timeline or named standalone graph scope.
3. Hash format revision 1 length-prefixes one domain-separated SHA-256 framing over the stable
   algorithm and primitive revisions, project and selected-root identities, component count, family
   tags, semantic identities, codec and graph revisions, byte lengths, and canonical digests.
4. The outer project document revision remains in the report only for optimistic-command
   correlation. Database schema, private manifest, active path, save destination, autosave
   generation, and SQLite page layout never enter the semantic content hash.
5. `superi-engine::EngineCommandDispatcher` exposes `InspectProjectDiagnostics` against its exact
   selected history snapshot. The typed result advances one successful command sequence, emits no
   event, reserves no event capacity, and changes no project or history state.
6. Focused contracts prove construction-order equality, media identity, fingerprint, target, and
   rejected-relink sensitivity, exact changed component families, monotonic undo restoration, and
   equal reports after reload from byte-different SQLite layouts and paths. Local script traces now
   retain the selected editor state's project-owned semantic hash, while dedicated diagnostics API,
   CLI, subscription, and database-file adapters remain later work.

Complete editor-state inspection composes canonical authored resources and cached runtime evidence:

1. `superi-engine::EngineCommandDispatcher` handles `InspectEditorState` by borrowing one selected
   `ProjectHistoryState` on EngineControl.
2. `superi-engine::editor_state` derives project diagnostics and settings, canonical timeline,
   graph, and clip-mix documents, bounded extension descriptors, exact audio routing and continuity,
   and explicit recovery, automation, playback, and export availability from that one snapshot and
   already retained runtime observations.
3. The inspector performs no recovery discovery, playback command, export poll, filesystem work,
   authored mutation, event reservation, or event publication. Its successful command sequence is
   the aggregate resynchronization fence.
4. `superi-api::state` projects strict schema `1.0.0` at `superi.editor.state.get` with explicit
   project, timeline, graph, media, audio, color, effect, AI, playback, and export roots and resource
   `superi.editor.state`.
5. Exact canonical JSON bytes, lengths, and SHA-256 identities preserve authored meaning. Audio
   retains integral sample clocks, ordered channel meaning, explicit routing and mute intent, and
   exact continuity evidence, while bulk packets, frames, samples, textures, extension payloads,
   and export results stay private.
6. The production editing workspace strictly projects the canonical timeline document into stable
   track and item identities with exact source and record ranges, grouping, linking, selection,
   height, targeting, locks, synchronization, mute, solo, enable state, complete audio routing, an adaptive ruler, and
   transient playhead, range, scroll, zoom,
   application-owned interaction selection, lasso geometry, and keyboard focus. Relationship-aware
   selection mirrors the canonical fixed-point rule, while direct selection bypasses it. Clip items
   supplement that frozen model with real media previews, source and relationship state,
   clip-scoped graph nodes and drivers, exact audio automation keys, and separate interaction
   selection. Every authored marker remains visible, exact marker targets navigate in stable order,
   and inexact or overscan targets remain explicitly non-navigable. Caption items retain canonical
   text, language, speaker, style, timeline relationships, and metadata beside exact record timing.
   All thirteen track, six caption, and six marker
   gestures return through the application-owned generated command, and marker reversal uses one
   complete typed inverse at the exact resulting revision, so the workspace does not create a
   frontend authored-state owner.
   Exactly one selected video clip and one selected audio clip enable link, exact source-time
   synchronization, detach, and existing-path audio replacement. Per-source channel controls submit
   one complete main or audio-track route with explicit semantic channel or mute intent through the
   same application executor.
   The same frozen model feeds an exact timing compiler for ripple, roll, slip, slide, razor, trim,
   ripple or roll extend, ripple delete, and synchronized gap insertion or closure. It emits one
   immutable public operation batch through the same application executor; lower timeline and
   engine owners retain validation, atomicity, relationship intent, persistence, events, and history.
   The caption panel uses that same executor for selected cue changes and for bounded SRT, WebVTT,
   or fresh transcript conversion into explicit-gap millisecond tracks. Export reads only the
   refreshed canonical model, and parsed files plus form drafts remain transient.
7. Focused engine, API, and frontend contracts prove coherent revisions, strict deterministic round trips,
   explicit detached, unobserved, pending, and observed owners, multichannel routing, muted channels,
   a sample-exact continuity gap, exact timeline projection, revision and freshness fenced clip
   previews, complete caption projection and exchange, stale transcript rejection, complete marker
   state, exact marker navigation, durable typed reversal, and legal
   keyframe ownership without hidden polling.

Read-only project integrity validation composes those same persistence owners:

1. `execute_project_integrity_command` accepts one `ProjectIntegrityCommand::Validate` path and
   returns one bounded deterministic `ProjectIntegrityReport` for editor, script, or headless use.
2. The command verifies the SQLite header, opens only through hardened read-only policy, and runs one
   complete shared SQLite and foreign-key collector. Ordinary open, migration, save candidates, and
   integrity inspection no longer rely on one-row or quick-check shortcuts.
3. Exact application identity and a registered schema 0, 1, 2, 3, or 4 reader lead through schema-object
   evidence, component bounds and digests, manifest checks, canonical timeline, graph, settings, and
   audio codecs, full semantic reconstruction, and aggregate validation without migration.
4. Only complete reconstruction yields verified identity. Supported legacy state reports checked
   forward migration, while invalid, future, wrong-application, inaccessible, busy, changed, or
   truncated state receives a stable status, stage, finding code, and safe repair disposition.
5. Findings and evidence are canonically ordered and hard bounded, a same-connection `data_version`
   fence rejects changed inspection state, and the command never creates, writes, migrates, repairs,
   salvages, or selects project authority.
6. `RestoreValidatedRecovery` directs a caller to the separately validated recovery controller and
   engine coordination. No API, CLI, engine, transport, or GUI integrity adapter exists yet.

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
6. The engine history consumer passes selected snapshots, including opaque unknown extension state,
   after apply, undo, and redo through this
   surface and reopens exact equal artifacts. Production background autosave hosting and CLI remain
   later work.

Crash recovery consumes that exact autosave namespace through one stable public command surface:

1. `superi-project::ProjectRecoveryController` binds the exact project child and discovers only
   strict published generations. It rejects unsafe project-directory shape, ignores foreign and
   save-sidecar names, opens each exact regular candidate through `ProjectDatabase::open_read_only`
   and `load`, and retains internal classified findings beside valid candidates.
2. Candidate identity is one opaque 20-digit generation. Every compare, restore load, and dismiss
   action requires the exact catalog revision, resolves only under the project namespace, and
   revalidates file identity before use. No caller supplies a path.
3. Semantic comparison reports current and candidate revisions plus editorial, settings, authored
   clip-mix, extension, root timeline, and graph differences through typed equality. The new
   diagnostics hash can identify equal canonical authored evidence, but recovery does not treat
   digest equality as file validity, merge authority, or a replacement for complete candidate load.
4. `superi-engine::ProjectRecoveryCoordinator` attaches only when one file-backed active database
   reloads to the selected `ProjectCommandHistory` snapshot. Restore prepares a cloned monotonic
   document first, publishes it through `ProjectDatabase::replace`, then commits empty session
   history through an infallible swap. If a collaborator changed the active database after
   attachment, project generation fencing rejects publication before history changes. The source
   candidate remains until explicit dismissal.
5. Exact dismissal atomically renames one revalidated candidate to a recognized tombstone and
   synchronizes the directory before success. Cleanup trouble becomes a degraded finding, and later
   discovery safely completes recognized tombstone cleanup.
6. `EngineCommandDispatcher` reserves command and event sequence space and queue capacity before
   discover, restore, or dismiss mutation and emits one complete correlated replacement state.
   Compare is read-only and event-free. Every persistence, stale-fence, terminal, or queue failure
   preserves the active database, history, candidate, sequence, and event stream. An external active
   replacement is a user-correctable conflict that preserves collaborator bytes exactly.
7. `superi-api::ProjectRecoveryApi` exposes strict schema `1.0.0` get, compare, restore, dismiss,
   and changed contracts. Public snapshots contain opaque identities and reviewed category,
   recoverability, safe action, and next-action fields, never paths, raw SQLite text, contexts, or
   source-chain details.

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
   processing order, resolves every input in edge identity order to an earlier node, accumulates each
   processor's fixed latency, and fallibly preallocates every interleaved f32 buffer plus the exact
   ring and scratch required to delay faster routes to the slowest arrival. Borrowed input views
   expose direct or compensation-adjusted current-block samples without callback allocation.
4. `PreparedAudioGraph::process` requires `ExecutionDomain::Audio`, rejects rate, size, output,
   overflow, and continuity mismatches before running processors, then advances the next exact
   sample only after complete success.
5. `PreparedChannelMixer` converts explicit canonical layouts with a precomputed speaker or
   discrete matrix. Direct graph edges remain exact-layout only, and consecutive block timing is
   unchanged through the explicit converter node.
6. `ClipMixState` publishes complete controls and identity changes transactionally. Preparation
   resolves snapshot-wide solo and precomputes semantic routing and phase coefficients before the
   callback applies gain, exact linear fades, equal-power stereo pan, mute, solo, and phase.
7. `AudioAutomationState` publishes bounded revision-fenced clip-gain lanes with exact keyframes and
   Read, Write, Touch, or Latch behavior. `prepare_processor_with_automation` binds an immutable
   clock-checked curve, and the existing clip processor evaluates absolute sample coordinates
   without callback allocation, locking, dispatch, or mutation.
8. `superi-audio::serialize` encodes authored clip-mix state as strict canonical revision-1 JSON,
   preserves each f32 by exact bits, binds the ordered payload to SHA-256, and rejects corruption,
   unknown fields, alternate encodings, duplicate identities, and bounded-input violations.
9. `superi-project` retains clip-mix state in every immutable snapshot and schema-5 audio
   component. Prepared processors, device state, queues, and callback resources remain absent from
   the aggregate and persistence.
10. Public crate integration tests use unity `SummingBus` processors to prove dry submix, parallel
   auxiliary send and return, stable identity-ordered summing, and one terminal master over
   consecutive 48 kHz stereo blocks.
11. `superi-engine::audio_mix` consumes real timeline edit outcomes against cloned project and mix
   state. It inherits right-fragment intent, transfers replacements, removes deleted identities,
   and publishes both revisions only after both validate.
12. The engine dispatcher owns optional automation state, exact candidate preflight, dynamic no-op
   event reservation, and complete revision-correlated replacement events. `superi-api` projects
   schema 1.0.0 strict transactions and events through engine re-exports without a direct audio
   dependency.
13. The compound project command routes timeline, graph, media, audio, and root actions through one
   history unit. Public audio and engine contracts prove save and reopen, undo and redo, and audible
   adjacent-block continuity. No decoder or engine playback owner feeds the complete
   routing path.
14. `superi-audio::hosting::PreparedAudioUnit` enters the ordinary processor path on macOS after
   background-domain discovery and initialization. Exact component identity, maximum slice, planar
   format, ordered channel meaning, native latency, and actual process location are read back before publication.
   Optional class-info property-list state is restored before initialization and captured in bounded
   serialized form on the control path.
   The audio-domain path uses preallocated pull-callback and output storage, supports bounded native
   subrange requests, commits only complete finite output, and poisons native failures. A public
   contract runs Apple's Peak Limiter from a deterministic source through the terminal master in
   audited in-process and verified out-of-process modes.
15. `superi-audio::hosting::vst3` prepares one explicit worker-local VST3 audio effect for canonical
   mono, stereo, quad, 5.1, or 7.1 f32 processing. It retains platform module and COM ownership,
   converts the graph's interleaved semantic order through preallocated planar buffers, and maps
   exact `SampleTime` plus bounded automation and output monitoring across one native process call.
   Bounded seekable streams restore component, controller-component, then controller state before
   activation and capture exact state off the callback.
16. A real temporary VST3 fixture runs only in isolated child processes and proves every supported
   layout through source, hosted effect, submix, and master, including exact state traffic and native
   latency.
17. `superi-audio::plugins` owns a bounded versioned digest-checked native-state envelope and an
   isolated process bridge processor. It preserves exact component and controller bytes, sample
   clock, native and transport latency, always advances a preallocated dry delay, and publishes
   timing-matched dry fallback after missing, faulted, malformed, or nonfinite worker output.
18. `superi-engine::audio_plugins` discovers VST3 bundles deterministically, accepts typed Audio Unit
   candidates, rejects in-process or unbounded workers, validates descriptors, captures and restores
   compatible checkpoints, restarts one fault, quarantines repeated activation failure, and stores
   one exact project extension record per audio node. Concrete IPC, sandbox launchers, heartbeats,
   kill integration, and Audio Unit registry enumeration remain external adapters.

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
   documents, including strict timeline-owned graph values, in schema-5 SQLite rows, canonicalizes
   supported legacy component documents during migration, reconstructs exact retained graph
   revisions, and atomically publishes those revisions through save, save-as, copy, backup, and
   deterministic autosave recovery points. Recovery discovery,
   comparison, and monotonic restoration now consume that exact container through project, engine,
   and API owners. The API-owned local host and CLI now consume those database files through the
   project authority, while complete product runtime paths remain absent.

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
    domain. Seek, scrub, pause, resume, stop, frame step, signed shuttle, rate, direction, loop, and
    authored-bounds changes cancel
    stale generations, request callback-owned audio discard, and submit one protected exact frame.
    Checked frame-to-clock cadence uses fixed anchors and distinct deadlines. Optional late-frame
    policy skips only ordinary playing frames, protects user and loop intent, and forces visible
    progress at its positive ceiling. Immutable snapshots expose frame, prediction, viewport, and
    audio degradation. A production timing-only runtime now owns this transport on the desktop
    Playback thread, executes the capacity-bounded engine bridge, and explicitly marks viewport and
    audio output unavailable. The strict public playback command and native desktop route reach that
    owner, while prepared decoded pixels, rendered audio, and native presentation remain absent.
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
    project apply, undo, redo, recovery discovery, comparison, restore, dismissal, history
    inspection, and semantic diagnostics inspection. Project diagnostics read the exact selected
    history snapshot, return ordered project-owned evidence and its versioned semantic hash, advance
    only the successful command sequence, and emit no event or project mutation. The dispatcher also
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
    projects the scenario transaction and event seam, generic project-history command and event
    vocabulary, complete editor replacement-state query, strict recovery command surface, and
    validation schema through one `ProjectEditorApi`. `superi-cli` consumes the scenario seam for
    its canonical runner and the validation facade for exact deterministic `engine validate` JSON.
15. Invalidation-to-render orchestration, ROI-plan-to-evaluator binding, cache invalidation
    invocation, automatic capacity policy, external directory coordination, and production engine
    catalog wiring remain separate later checkpoints. Cache owns bounded outer job dispatch for
    background population without moving priority or worker ownership into graph.

The in-process engine request envelope, dispatcher, bounded event channel, playback-domain command
bridge, logical export controller, engine-owned typed project command history, canonical public
scenario transaction, generic public project transaction, complete editor replacement-state query,
and strict read-only integration validation query are implemented. The production desktop now adds
one generated playback wire route over the same bounded bridge. The bounded API-local script
runtime consumes the same generic project transaction and editor-state query without adding an
engine request model. Subscription delivery, live wire routing, production UI extension control,
CLI editor or script execution, and closed-tier runtime consumers are not implemented.

### Documented target, incomplete

Repository contracts describe one stable public command and event seam shared by UI, CLI, scripts,
extensions, automation, and closed-tier clients. The canonical scenario transaction remains the
fixed public reference slice. The engine production project-history surface and correlated events
now have a strict generic public adapter for every current authored project operation. Later clients
must consume that vocabulary rather than define parallel mutations. Broader engine transactions are
intended to coordinate
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
bounded ordinary frame dropping over that prepared path. The desktop can now control and observe the
same exact transport through the public API, but its production runtime is timing-only and does not
bind the prepared source bundle, timeline audio renderer, or native viewport output. Engine
render-export separately binds explicit acquired source
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

The production native display path separately samples the same canonical `Rgba16Float` texture
through `GpuDisplayPresenter`. Its image compatibility constructor retains the existing output,
while an explicit `GpuDisplayView` compiles alpha, individual-channel, source-luminance, fixed
false-color, or clipping inspection into the presentation shader. Diagnostic views are opaque and
zero-alpha RGB avoids division. Source modes run before primary, gamut, and transfer stages; clipping
runs after display-linear primary and gamut conversion but before transfer encoding and attachment
clamping. The four Tauri viewer roles use this path on the sole GPU submission owner for both inline
and external native surfaces, sampling the same managed canonical role texture on each destination.
Actual offscreen tests compare every mode per pixel and channel to the CPU reference after half
quantization. The production path also composes the existing ICC, viewport, output-transform, and
GPU display surfaces for source, program, composite, and color roles. On macOS it refreshes exact active
CoreGraphics displays and validated profile bytes into one transactional catalog, binds each role to
a selected monitor and profile generation, and rejects stale or removed bindings before acquire and
again before submit. It builds one real `GpuDisplayPresenter` for the selected SDR sRGB or Display P3
intent and one explicit `GpuDisplayView`, then samples the canonical ACEScg scene-linear
`Rgba16Float` texture without CPU readback. Image, alpha, individual-channel, source-luminance, and
fixed false-color modes run before primary, gamut, and transfer stages; clipping runs after
display-linear primary and gamut conversion but before transfer encoding and attachment clamping.
Diagnostic modes are opaque and zero-alpha RGB avoids division. Actual offscreen tests compare all
eight modes per pixel and channel to the CPU reference after half quantization. React receives only
bounded profile metadata and control state. Non-macOS discovery and arbitrary ICC matrix, TRC, or
LUT evaluation remain explicitly unavailable.

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
Color input, output, LUT, and rule transforms remain CPU implementations and have no graph-visible
node catalog. A GPU wide-gamut transform exists as a direct public surface, and the desktop viewer
now composes built-in display transforms with ICC identity and viewport freshness guards. No engine
or graph consumer composes the complete display, delivery, arbitrary ICC evaluation, viewport, or
export pipeline, and `MonitorAwareViewport` itself remains a guard rather than a color evaluator.

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
dispatcher, atomic revision-fenced canonical scenario, project settings, and audio automation
transactions, deterministic semantic project diagnostics inspection, crash recovery discovery,
comparison, durable restore, and exact dismissal,
full-state undo plus redo,
bounded revision-fenced project command history over real project media, extension, and settings
mutations,
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
project settings, authored audio automation, project crash recovery, and integration validation
behavior. Project history, settings, and recovery use one attached authoritative `ProjectDocument`;
audio automation retains a separate audio-owned state attachment. The canonical scenario command
model remains a reference boundary, while project history delegates to real project media,
extension, and settings owners. The generic public project adapter now covers current timeline,
graph, media, clip-mix, extension, and compound control. Prepared resources, foreground playback, interactive
transport, and render-export do not yet form one source-backed broad public control flow.
Variable-rate decoded audio, native GPU presentation and readback, container muxing, file
publication, and persistent export recovery remain absent.
Nodes remain an explicit placeholder. Validation, plugin discovery, supervisor coordination, and
the shared bounded declarative extension registry are substantive, while concrete platform
transports and native OFX adapters remain absent. The registry synchronizes canonical supervisor
status for discovery but never owns a worker, factory, callback, path, launcher, or dispatcher.

`superi-api` is the stable public facade and schema owner. It keeps implementation types private,
publishes one deterministic schema `1.9.0` catalog for all current commands, queries, events,
resources, errors, capabilities, and permissions, and exposes strict versioned media capability, complete engine
introspection, integration validation, project
settings, recovery, audio automation, and asynchronous job records plus the fixed canonical scenario action,
optimistic ordered scenario, generic authored project, settings, and automation transactions,
strict recovery get, compare, restore, and dismiss commands, complete editor state projections,
cursor-safe command-log inspection, and matching replacement events.
It also projects one immutable engine-owned extension registry through exact versioned identity,
lifecycle, requested and granted capabilities, core feature availability, bounded safe failure,
and a stable existing project-control reference. Its permission-free discovery query and full
replacement event expose no worker, callback, launcher, factory, path, dispatcher, permission token,
or mutation authority.
It also exposes exact-source schema `1.0.0` local `superi-json` interpretation through the existing
generic project and editor-state methods, with bounded strict parsing, permission preflight,
deterministic typed traces, and explicit conflict and committed-prefix evidence.
It exposes stateless schema `1.0.0` API and optional project version negotiation with strict ordered
offers, canonical highest-common selection, complete support disclosure, and typed incompatibility
outcomes without runtime downgrade or project mutation.
It also exposes nonblocking job query and cooperative controls with matching full replacement
events, plus bounded ordered event registration and non-destructive polling with explicit restart
and eviction recovery. Its optional generator surface derives serializable declarations from the same Rust DTOs,
registers method, event, and resource pairs from the same canonical registry as catalog discovery,
and emits deterministic TypeScript declarations, exact typed maps, and a transport-neutral client.
Validation nests
canonical introspection, which preserves workflow readiness and only reviewed user-safe failure
data, then adds exact action and endpoint evidence. Project settings retain complete project-owned
scalar meaning, recovery retains opaque identity plus reviewed safe findings, and automation
retains audio-owned typed state through engine re-exports without direct production project or
audio dependencies. Asynchronous jobs project the canonical engine export queue and ordered event
envelopes without exposing host polling, executor submission, waits, typed artifacts, or another
scheduler. It defines strict data-only JSON-RPC 2.0 request, success, and structured safe
failure shapes. Its nonserializable host permission context denies protected filesystem, plugin,
and destructive operations by default and authorizes complete typed payloads before dispatch, but it
does not provide authentication, operating-system sandboxing, network hosting, live wire routing,
general dynamic dispatch, push delivery, persisted replay, public job submission, or a scripting
language runtime beyond bounded `superi-json`. Its local host now composes database file commands and CLI editor execution over
the generic facade, scoped EngineControl, project publication, recovery, settings, validation, and a
narrow typed JSON-RPC method subset without registering a parallel transport catalog. The generic
facade projects engine project history through minimum replacement state and semantic action
evidence and returns complete editor replacement state without exposing engine-owned mutable state.

`superi-cli` is a binary boundary, not a library. It accepts durable project, media, timeline,
render settings, inspect, validate, recovery, and JSON-RPC automation workflows plus exact `api
schema`, the normalized slice command, exact `engine validate`, help, and version. Schema discovery consumes the API-owned catalog
including permission, version negotiation, local scripting, and asynchronous job query and control
metadata without importing engine or
duplicating registry data. It also discovers event subscription open, close, poll, and resource
metadata. It exposes one strict local permission-policy parser but no job-control or event-poll
command and has no live queue or stream attachment. Local project paths, bounded request input,
durable publication, and correlated output remain behind the API host; the CLI imports no engine,
project, timeline, graph, audio, or concurrency type. It validates repository fixture authority,
drives revision-fenced `ScenarioApi`
transactions under one exact canonical fixture-read grant and verifies their events, writes the strict
schema 1.1.0 report with all-stage timing, resident-memory, and versioned expectation evidence, and
publishes a non-playable contract artifact through collision-safe paths. Its validation command
uses the API-owned fresh-engine helper and prints the strict immutable projection without importing
engine or concurrency directly.
Its project expectation
digest is portable across checkout roots, while strict undo and redo comparison and reported media
paths remain unchanged. Local failures preserve category and recoverability with bounded structured
context and redact path-shaped values.

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
- Native desktop shell state remains presentation-only. Workspace layout, bounded recent paths, and
  a bounded versioned keyboard-shortcut profile may persist privately, but project bytes, durable
  document identity, authored state, and engine
  history retain their existing owners. Active operations block close, session-history loss is
  explicit, document titles omit parent paths, and one accepted close resolution enters the
  existing orderly application lifecycle exactly once.
- Command discovery remains a bounded projection over existing typed owners. Catalog actions may
  delegate only to registered application commands or desktop shell intents, disabled actions must
  retain current reasons and effective shortcuts, and palette query or modal state must not enter
  durable workspace state.
- Effective application shortcuts are canonical, unique, and registry-backed. Configurable global
  bindings require portable modifiers, native menu and clipboard accelerators stay reserved, imports
  are transactional, unknown command IDs remain bounded and inactive, and editable or composing
  input never dispatches an application command.
- Shell capability state is operational evidence only. Provider declarations, last-known cache
  data, and current failures must preserve exact sample and channel meaning while remaining outside
  GPU device selection, audio streams and routing, codec sessions, AI execution, project state,
  workspace presentation, and editable artifacts.

- Every long-lived desktop thread or detached blocking task has one retained native owner, stable
  operational identity, bounded admission where applicable, and an explicit join on setup rollback
  and normal exit. Cleanup attempts continue after an earlier failure, and the read-only process
  snapshot never replaces application lifecycle, engine, project, GPU, or persistence authority.
- A desktop source-monitor session is runtime state, while in and out marks are durable intent bound
  to the imported fingerprint. Load and exact seek remain project, library, media, monitor, and
  fingerprint fenced; scanner-confirmed changed bytes also make the session stale and reject later
  source operations. Invalid ranges or failed publication preserve the prior session and sidecar.
  Opening a source never implies that decoded pixels reached the native GPU presentation owner.
- Viewer cache indicators are presentation-only observations over the existing immutable editor
  snapshot. A scheduled frame is not a cache hit, absence of a predictive failure is not completion,
  authored routing is not device output, and structural continuity is not an observed audible
  sample. Exact discard generations, sample clocks, channel order, routing, and continuity must be
  preserved while unavailable fill, occupancy, prediction, output, and signal evidence stays
  explicit across every transport and shell navigation mode.
- Project semantic identity is one versioned, domain-separated SHA-256 framing over explicit
  project identity and ordered canonical component evidence. It includes authored media, settings,
  clip mix, extensions, and graphs, but excludes the outer document revision, database schema and
  layout, active path, save identity, private integrity manifest, and runtime readiness. Ordered
  component diagnostics identify changed families without becoming file-validity, merge, or
  conflict-resolution authority.
- One `ProjectDatabase` owns active file identity and every save publication. A file operation first
  reconstructs and validates the complete supplied immutable snapshot in a private same-parent
  candidate, then publishes through explicit replace-existing or require-absent behavior. Save-as
  rebinds at the publication commit point, copy and backup do not rebind, prepublication failure
  preserves prior authoritative state, and any fallible postpublication result must identify the new
  destination as already published.
- A file-backed project retains one validated active generation over complete bytes and available
  file identity. Load and active replacement reject stale, missing, corrupt, or substituted files
  without mutating them. Cooperative replacement writers serialize through one deterministic
  persistent sibling lock held across destination and active-generation revalidation plus the
  publication commit point; lock contention is visible and no retry is hidden.
- Autosave reuses that database and Backup authority rather than defining another format or active
  file owner. Policy and time anchors remain session-local; completed recovery points contain full
  current-schema editable meaning. Scheduling uses caller monotonic time, retention uses strict
  numeric generations, pruning touches only preflighted regular managed files, and recovery
  discovery and restoration must validate artifact contents through the database owner.
- Project integrity inspection is read-only, role neutral, deterministic, and bounded. Verified
  identity appears only after complete current or supported legacy semantic reconstruction; source
  mutation or truncated evidence remains indeterminate, and repair dispositions never grant mutation
  authority.
- Project settings are authoritative whole-project state under the same optimistic revision as
  editorial and graph meaning. Project owns keys, defaults, candidate validation, and persistence;
  engine resolves existing subsystem types; API exposes only strict shared values and complete
  replacement state.
- Audio project settings preserve an exact sample timebase and ordered channel layout. They do not
  synthesize routing, resample authored media, reinterpret channel meaning, alter synchronization,
  or claim live device reconfiguration merely by being inspected or persisted.
- Timeline audio-video synchronization preserves both record placements and translates the complete
  audio source map only with exact clock conversion. Channel routing remains a complete ordered
  replacement with one explicit target or mute decision per source channel; React and the API may
  project or transport that intent but cannot fill missing routes or round timing.
- Native audio effects preserve one exact component identity, sample clock, maximum slice, and
  ordered channel layout. Preparation and teardown remain outside the audio callback, required
  process isolation is verified from the instance, successful processing uses prepared storage,
  and native or callback failure cannot publish partial caller output or permit instance reuse.
- Native plugin state is bounded, digest checked, and format neutral. Audio Unit property-list bytes
  and VST3 component plus controller streams are captured and restored only off the callback, retain
  exact saved identity evidence, and may cross an installed upgrade only when format and component
  identifier still match.
- Every prepared audio processor declares fixed latency. Graph preparation computes cumulative
  arrival and allocates the exact route differences, so direct, send, and auxiliary-return branches
  remain sample aligned without callback allocation. Dynamic latency requires a control-side graph
  rebuild.
- Native audio supervision requires BackgroundJob for scan and lifecycle work, EngineControl for
  runtime observation, and a declared separate-process worker with bounded messages, deadline, and
  restart support. The first activation fault retains a checkpoint, repeated activation failure
  quarantines, and durable records use per-node instance identity without persisting readiness.
- Timeline media organization retains stable bin and smart collection identities. Manual bin
  membership and dynamic query results never replace clip `MediaId` links, and mismatched relink
  candidates retain evidence without replacing the active target.
- Deterministic ordering is explicit. Stable backend IDs break selection ties; ordered maps and
  sets stabilize public snapshots, fixtures, diagnostics, and validator output.
- Engine capability and health introspection is read-only. Workflow availability comes from the
  canonical lifecycle admission path, independent owner revisions remain visible, and raw failure
  messages, sources, contexts, internal identities, and recovery tokens remain private.
- The public API catalog is one explicit deterministic registration surface. Every method declares
  command or query kind and schema version, every event declares payload version, every replacement
  resource has one stable identity, and all references retain primitive revision 1. Duplicate names,
  cross-category method overlap, incompatible catalog identity, and unknown wire fields fail closed.
- Local script source is exact UTF-8 bound to a required lowercase SHA-256, and `superi-json` has one
  strict versioned closed step vocabulary. Duplicate members, excessive depth, unsupported methods,
  stale initial revisions, and nested permission denial fail without hidden interpretation.
- Script mutation remains the existing public project mutation. A later failure stops only the
  unexecuted suffix, preserves prior committed commands and ordinary events, and returns explicit
  initial and final revision and semantic hash evidence rather than implying whole-script atomicity.
- Public event delivery has one independent nonzero sequence, a bounded whole-record replay buffer,
  and a bounded subscriber registry. Subscriber cursors remain caller-owned, repeated polls are
  non-destructive, and no subscriber can advance or close another.
- The closed public event union matches all nine catalog events exactly. Command events retain
  source event sequence, command sequence, and transaction identity; observation events retain the
  authoritative snapshot revision. JSON-RPC IDs are never event correlation IDs.
- Eviction and changed stream identity return no partial events. They return a reset barrier and the
  complete eleven-resource replacement manifest, including query versus typed inspect command paths.
  Events published after the barrier remain replayable.
- Generated TypeScript is a projection of that same registry and the serializable Rust DTOs. Its
  method, event, and resource names must equal the canonical catalog, wire-specific scalar shadows
  remain explicit, output is deterministic and path-free, and the client owns no transport or
  mutable project state.
- Public asynchronous jobs are one strict projection of the engine-owned logical export queue.
  Canonical handles, weighted priority vocabulary, progress, dependencies, safe failures,
  cooperative controls, result availability, and ordered full replacement events may cross the
  boundary, while host polling, executor submission, waits, typed artifacts, and queue ownership do
  not.
- Public JSON-RPC error data retains category, recoverability, stable safe code, title, action,
  reviewed actionable context, and optional last-valid resource identity. It never copies raw error
  summaries, source chains, raw context values, or internal and sensitive diagnostic fields.
- Editable graph state has one optimistic revision and immutable shared snapshots. A nonempty
  transaction publishes exactly once only after every ordered mutation passes, while stale or
  failed batches preserve the prior state and revision. Presentation order never replaces DAG
  processing order.
- Parameter drivers are ordinary typed editable graph state in canonical target order. Every
  dependency is explicit, direct links preserve exact types and payloads, expressions are bounded
  and pure, parameter cycles fail before publication, and all caller roles evaluate one immutable
  snapshot through the same request-local result path.
- Timeline selection, bounded track height, targeting, authored-item locks, sync locks, mute, solo,
  enable, linked selection, and clip groups publish inside the same revision-checked project
  transaction as clip source and record state. Stable surviving identities retain intent, direct
  selection bypasses relationships, sync-sensitive track resolution preserves timeline layer order,
  and a locked track rejects contained-item changes until explicitly unlocked.
- Timeline interaction selection in React is a revision-fenced projection into the existing shared
  application selection. Group and enabled-link expansion mirrors canonical authored relationships;
  lasso geometry, range anchors, and keyboard focus cannot publish authored timeline state.
- Track batches are ordered, nonempty, and atomic. Canonical positions are bottom-to-top, active
  mute and solo intent is audio-only, retained output intent compiles separately from presentation
  controls, and deleting a populated track releases only mix controls for clips that disappeared.
- Multicam source timelines own ordered stable angle identity, synchronization provenance,
  metadata, and local source membership. Ordinary nested target clips own independent gapless
  switch and audio intent, resolve through both clip time maps exactly, and inherit state through
  structural fragment and replacement edits inside the same project transaction.
- Durable extension identity is one ordered extension and record key. Open namespaced kinds,
  bounded canonical metadata, and exact opaque payload bytes preserve current and unknown future
  state. Requested capabilities do not grant themselves, granted capabilities remain a
  user-controlled subset, quarantine retains structured failure evidence, and no persisted
  lifecycle value may claim that a worker, factory, or registry is currently ready.
- Runtime extension identity is exact and versioned, and one bounded process-lifetime declarative
  registry may represent multiple installed versions independently. Requested and granted
  capabilities remain separate, core feature discovery stays authoritative, only Ready features
  are Available, and equal synchronization does not advance its revision.
- Runtime extension discovery is observation, not authority. Safe failures omit raw diagnostics,
  and registrations expose only the existing durable project command and editor-state resource for
  user control. Workers, launchers, callbacks, factories, paths, dispatchers, permission tokens, and
  privileged closed-tier routes never enter the registry or public DTO.
- Authored graph effect parameters remain graph-owned, and generated AI output remains an ordinary
  editable artifact. Project extension records may retain only auxiliary extension state,
  supplementary provenance, capability, lifecycle, and failure meaning without creating hidden
  content or a competing render model.
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
- Application feedback preserves safe source, code, action, context, last-valid identity, and
  continuity while distinguishing exactly retryable, degraded, user-correctable, and terminal
  conditions. Unknown recovery values fail closed to terminal. Menus, tooltips, notifications,
  progress, and status are presentation-only; dismissal cannot clear native evidence, progress
  cannot invent a missing total, and recovery commands must return to existing owners.
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
The desktop source-monitor contract opens and seeks a real 48 kHz mono WAVE source, persists and
reopens exact fingerprint-bound marks, rejects a reversed range without publication, proves an exact
changed-byte scan makes the retained session stale and fences another seek, unloads the retained
source, and drives a real three-frame PNG sequence at exact 24 fps through its inclusive range and
overrun rollback. That sequence also drives all four three-point placements and equal-duration
four-point editing through the retained generated project route, proves undo and redo, preserves
fresh monitor state, and reopens the final revision 8 project. A focused engine unit contract proves
that the source-only constructor
contains exactly the four source registrations and constructs no codec factories; frontend
contracts keep the editing workspace projection and native source-viewer boundary explicit.
The desktop window-session contracts prove strict schema normalization, safe corrupt-state
preservation, current-monitor reconciliation, relative monitor movement, exact placement reversal,
atomic JSON round trip, close and reopen continuity, real auxiliary webview construction through
Tauri's mock runtime, independent webview transport generations, one authored event order, client-local disconnect,
strict bridge payloads, listener cleanup, real System panel and route hydration wiring, and the
primary-window native GPU surface boundary. The complete desktop Rust and production frontend
suites widen that focused proof without claiming physical multi-monitor automation on every
supported operating system.
The desktop process-runtime contracts prove retained exit and blocking-task handles, closed
admission, failed-shutdown retry visibility, real EngineControl, Playback, and bounded-worker counts,
join-all stopped state, idempotent GPU and persistence cleanup, React process visibility, and no
operating-system child-process launch.
Focused desktop-shell contracts prove project and media drop partitioning, basename-only titles,
workspace reconciliation, sequence-fenced menu presentation, recent and workspace intent mapping,
duplicate close suppression, one-shot resolution, schema 3 full-layout and shortcut persistence,
schema 1 and 2 migration, duplicate placement and shortcut-ID rejection before live mutation,
independent corrupt-shortcut recovery, real-host
active project restoration, and missing-document degradation with retained recents. Strict
TypeScript, the production frontend build and contract set, focused shortcut and native integration tests, and
the Tauri library suite cover the implementation boundary; native menu appearance and interaction
remain operating-system physical-lane evidence.
Their integration keeps per-window routes and transport generations with the window-session owner,
projects main-window shell presentation into the process-wide menu, targets commands to the focused
webview, and routes only primary close or application quit through safe project preservation.
The shell-capability contracts compose one real current-host observation through all four provider
owners, prove exact audio configuration projection and explicit unknown channel meaning, retain
per-domain fallback with current failures, restore valid private cache state, replace corrupt cache
bytes, strictly parse and freeze the frontend replacement, and wire the production System panel.
They do not claim hotplug automation, physical stream success, codec execution, AI runtime, or
native visual proof.
The focused application-presentation contracts prove all four distinct recovery policies,
fail-closed unknown recovery, safe transport, project, lifecycle, crash, and last-valid context,
bounded immutable notification history, truthful public export progress, status priority, and
viewport-bounded menu geometry. Source contracts freeze the real provider, tooltip, menu,
notification center, status bar, failure cards, retained crash projection, panel pointer and
keyboard consumers, and delivery progress. Exact Node 24.13.0 TypeScript checking and the production
Vite build exercise the integrated React path; physical focus, screen-reader, high-contrast, and
reduced-motion behavior remain application-lane evidence.
The focused viewer-cache contract proves the exact eleven-field catalog, foreground frame and due
clocks, predictive and decoded-output degradation, nonblocking interaction, callback discard
acknowledgement, transport synchronization, exact sample rate, ordered source and destination
channels, complete channel or mute routes, continuity seams, malformed-evidence rejection, frozen
output, and unchanged input. Its real workspace consumer retains temporal, visual, audio, cache,
and comparison state across paused, playing, scrubbing, ended, fit, zoom, pan, pixel, fullscreen,
cinema, analysis, overlay, comparison, and external-display behavior without adding an IPC field,
timer, request, or second state owner.
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
The engine and API extension registry contracts prove exact version coexistence, duplicate
rejection, canonical ordering, change-only revisions, requested and granted capability validation,
ready-only availability, bounded safe failure, and immutable adapters over real OpenFX and native
audio supervisor state. Strict JSON rejects injected runtime handles and alternate control routes;
the advertised project control executes through the existing permission boundary, persists and
reopens through the project database, and missing runtime state never erases durable authored data.
The public schema contracts prove the exact 16-command, 14-query, nine-event, and 13-resource
catalog at schema `1.9.0`, current method and resource versions, primitive revision 1, canonical
ordering, strict JSON-RPC success and failure exclusivity, duplicate and incompatible identity
rejection, every recovery class, safe diagnostic field filtering, last-valid resource references,
the complete permission vocabulary, and exact metadata for all 30 methods. The version negotiation
contracts prove strict ordered offers, canonical highest-common API and primitive selection, both
typed incompatibilities, independent project compatibility, and JSON-RPC 2.0 framing. The scripting
contracts prove digest-bound source, bounded interpretation, permission preflight, deterministic
traces, durable project agreement, and query-step access to the same command-log surface. The
command-log contracts prove bounded cursor paging, explicit resynchronization after eviction,
metadata-only inspection, strict typed request reconstruction, and reauthorization of every
replayable command before disclosure. The CLI process contract
invokes `api schema` twice and requires exact deterministic output, category counts, names,
permission metadata, help, and invalid usage behavior.
The event stream contracts prove strict bounded identities and configuration, exact closed-union
catalog parity, independent monotonic public order, real command and observation correlation,
idempotent independent subscriber replay, batch caps, whole-record eviction, explicit restart and
cursor gaps, complete state resynchronization metadata, reset barriers, post-barrier delivery,
future-cursor rejection, close isolation, and terminal sequence exhaustion. Real project editor,
engine introspection, extension registry, and asynchronous job lifecycle events all pass through the
same broker.
The permission contracts prove fail-closed defaults, explicit deny precedence, component-aware
filesystem scopes and traversal resistance, exact plugin identity and delegation ceilings,
destructive-operation scoping, safe errors, unchanged state, files, sequences, and events on denial,
and authorized parity through the existing facades. The canonical CLI scenario proves the same
public path remains available under one exact resolved fixture-read grant.
The TypeScript binding contracts render the API twice, require every canonical method, event, and
resource, reject paths and timestamps, and prove that generation is idempotent while missing or
stale checks never mutate their target. The committed artifact then passes its own freshness check,
strict TypeScript compilation, Vite production bundling, and a browser consumer contract that uses
the typed project command, AI state, extension lifecycle and control state, method response, event,
resource, and client surfaces.
The generic editor contracts lock all four commands and every current authored operation
discriminant, prove pre-dispatch conversion atomicity, and execute one real six-action project
transaction through event correlation, database reload, undo, and redo. They also drive one exact
retime through graph compilation, persistence, command-log replay, undo, and redo, while rejecting
an inexact source seam without changing project or history state.

The scripting contracts lock the `superi-json` language, method, schema, file convention, and every
published bound. They prove strict duplicate and unknown field rejection, exact-source SHA-256,
deterministic equal interpretation, mutation and complete state queries, initial and later conflict
visibility, committed-prefix and ordinary event preservation, nested permission denial before
dispatch, stable media identity and fingerprint, exact SQLite reopen, verified integrity, autosave
comparison, and recovery restoration. They do not claim arbitrary code, modules, filesystem source
loading, operating-system sandboxing, network access, or whole-script atomicity.

The public asynchronous job contracts drive the real dispatcher-owned export queue and prove strict
handles, stable kind and 8:4:2:1 priority vocabulary, nonblocking progress, ordered completion
events, pause, resume, retry, cancel, cancel-all, finalized removal, dependency state, deterministic
handle order, reviewed safe failures, and typed-result non-exposure. They do not claim public
submission, a transport poll or wait method, persistent queue recovery, muxing, or publication.
The project, engine, and API settings contracts compose one real project document, schema-5
database, typed resolver, full dispatcher, and stable public facade. They prove exact defaults and
all six settings domains, atomic optimistic updates, no-op stability, migration-derived defaults,
manifest coverage, strict public JSON, permanent names, full replacement events, audio timebase and
ordered channel preservation, invalid and stale rollback, and dependency direction. They do not
claim live subsystem reconfiguration, project file wire commands, or hardware audio behavior.
The macOS Audio Unit contracts use the real Apple Peak Limiter to prove exact component identity,
background preparation, actual process-location verification, repeated bounded pull callbacks,
adjacent partition continuity, stereo meaning, finite audible output through the terminal master,
prevalidation failure atomicity, bounded class-info state round trips, and fixed native latency. This
host proof does not claim broad third-party plug-in coverage, parameter automation, plug-in UI,
physical device latency, or a decoded timeline adapter.
The audio delay and isolated runtime contracts prove aligned parallel and auxiliary routes, exact
cumulative latency diagnostics, partition independence, fallible compensation preparation, exact
state bytes and digest rejection, worker-fault telemetry, and timing-matched dry fallback. The engine
supervision contracts add stable VST3 discovery, wrong-domain and in-process rejection,
compatible-version checkpoint recovery, restart and quarantine, and distinct per-node project state
through real database save and reopen. Concrete platform IPC and sandbox behavior remain outside
these deterministic adapters.
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
permissions and non-UTF-8 paths on Unix, destination-race preservation, stale-authority and stale-load
conflicts, missing and malformed active-file preservation, explicit save-as and copy escape paths,
deterministic fault injection before and after publication, and subprocess abort behavior across
both rename and no-clobber commit paths. Private subprocess proof starts two authorities from the
same generation and observes exactly one winner plus one visible conflict, while a separate
held-lock contract proves retryable classification and later success through the persistent safe
lock entry. Migration coverage also publishes all four
operations from an exact schema-0 source while preserving its reported origin revision.

The project autosave contracts exercise the public controller through deterministic monotonic
commands. They prove exact deadlines, disable and manual control, unchanged suppression only while
the periodic artifact exists, one-save forward jumps, strict generation ownership, mtime-independent
retention, foreign and candidate preservation, symlink tamper rejection before deletion, policy and
deadline bounds, no-clobber generation choice, state-preserving exhaustion, retry, complete
current-schema reopen equality, and unchanged active-project bytes. The engine consumer autosaves
the selected real history snapshot, including unknown extension state, after apply, undo, and redo
without adding engine filesystem ownership.

The project extension contracts exercise one role-neutral command surface across direct document
edits, caller-owned drafts, schema-5 SQLite, atomic file publication, autosave, engine history,
compound transactions, dispatcher events, and undo plus redo. They prove plugin, auxiliary effect,
AI artifact provenance, and unknown future kinds, requested and granted capabilities, user disable
and quarantine, structured failure recovery, exact non-UTF-8 payloads, deterministic ordering,
semantic no-ops, stale fencing, tamper rejection, and lossless schema-3 migration. They do not prove
plugin process readiness, graph factory availability, AI execution, UI, or public wire adaptation.

The project integrity contracts exercise one public command across editor, script, and headless
roles. They prove complete current identity, schema-0 migration reporting and later writable
migration, linked-media and relink evidence, retained graph, settings, authored-audio, and opaque
extension meaning, component and schema corruption classification including extension metadata and
payload evidence, missing-source noncreation, stable bounded evidence, byte nonmutation, and
continued authority-project use. Unit contracts prove permanent codes,
canonical ordering, UTF-8 bounds, complete foreign-key rows, exact limit and truncation behavior,
source-change precedence, and source versus semantic not-found classification.

The project diagnostics contracts exercise the public semantic report across direct snapshots,
byte-distinct SQLite layouts, reload, engine history, mutation, and undo. They prove stable version
constants, construction-order equality, exact media identity and fingerprint sensitivity, ordered
component-family visibility for timeline, settings, clip mix, extension, and graph changes,
outer-revision exclusion, restored hash identity after monotonic undo, and eventless typed engine
inspection of the selected snapshot. They do not claim file integrity, merge decisions, runtime
availability, a dedicated diagnostics API or CLI method, or database-file command adaptation.
Local script traces consume the project-owned hash through complete editor state, but no dedicated
diagnostics wire method or database-file command is claimed.

The desktop crash-diagnostics contracts exercise a real temporary application-data root and prove
unclean-session detection, exact route, dock placement, tab, size, hidden panel, focus, project,
revision, and lifecycle continuity, layout-free marker compatibility, orderly
marker removal, all four recoverability classes and matching entry points, bounded retention, private
panic-detail filtering at the Tauri seam, corrupt-journal archival, and degraded startup continuity.
The frontend source contract freezes native command wiring and reuse of existing application,
lifecycle, and project recovery owners without adding a generated public API method.

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
runs strict no-emit TypeScript 5.9.3 checking over the production React lifecycle client and
generated API adapter, creates a Vite 7.3.6 production bundle, and verifies exact pins, lifecycle and
generated-client ownership boundaries, runtime transport forwarding, workflow routing, and the
generated hashed JavaScript entry. The retained `ci/frontend-smoke/` fixture separately proves
generated public API compatibility without standing in for the application.

The Tauri Rust workflow runs on pull requests, pushes to `main`, and manual dispatch across macOS 26
arm64, macOS 15 Intel, Windows 2025, and Ubuntu 24.04. It builds the production frontend, then uses
the pinned Tauri 2 application configuration for a mock-runtime host test, explicit lifecycle
contracts, and the real native wry builder. Every blocking lane checks formatting, locked tests,
strict all-target Clippy, and locked `superi-desktop` compilation; Linux installs WebKitGTK 4.1 and
the documented desktop integration prerequisites. This proves the production shell and lifecycle
boundary, not engine process launch, public API transport, editor behavior, or physical hardware.

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
Timeline-local edit state adds exact or relationship-expanded selection, bounded per-track height,
stable target, authored-item lock, sync-lock, mute, solo, and enable intent, canonical clip links and
groups, direct member control, deterministic target and sync projection, and structural
reconciliation inside the same project transaction. One ordered atomic track batch creates,
deletes, names, resizes, reorders, targets, locks, sync-locks, mutes, solos, or enables tracks while
preserving every unaffected identity and relationship. Atomic foundational
edit batches now add ripple,
roll, slip, slide, razor, trim, extend, and exact three-point and four-point placement to insert,
overwrite, append, replace, lift, and extract. They preserve clip source and nested-timeline
relationships, inherit fragment intent, require explicit sync-locked ripple adjustments, report
typed fragments and invalidated transitions, reject implicit fit-to-fill retiming, and publish at
one project revision. A direct retime edit instead replaces one exact clip time map, rejects locked,
missing, invalid, and no-op targets atomically, and retains stable clip identity and record duration.
Nested operations place existing child timelines, add prepared compounds, or
derive multi-track compounds from a complete selection atomically. Selection-derived compounds
move original identities, sources, time maps, annotations, metadata, transitions, links, groups,
track intent, and implicit internal gaps into a zero-rebased child, then replace each affected
parent track with one linked and grouped nested instance. They also edit shared children through
stable instance identities and expose every direct or recursive relationship without flattening.
Deterministic compilation now converts a selected root and every reachable
nested timeline into one typed editable graph revision with stable domain-separated addresses,
explicit transition and nesting edges, complete object parameters including multicam intent, and
bidirectional provenance. Enable, mute, and solo compile as typed output intent, while height,
target, and locks remain nonprocessing editorial state.
Project now retains that compiled state and engine resource acquisition preserves its exact selected
graph. Project also interprets recognized referenced-media paths through one stable target format,
and engine adapts the resolved local path, stored `MediaId`, and fingerprint evidence into the real
source acquisition flow. Engine project history now wraps authored media, settings, and compound
commands and reverses the complete aggregate, including track state, retained graph, and clip-mix
state, through immutable project snapshots. Timeline item and track edits use a three-way recompile
to preserve nonconflicting direct graph work, and populated-track deletion releases only controls
for clips that disappeared. Selected snapshots also reach complete project-owned autosave recovery points
after apply, undo, and redo. API exposes generic authored project control, settings transactions,
and strict recovery control through that owner but does not expose autosave scheduling or database
file commands; CLI,
playback, audio-engine, graph evaluation, and rendering do not execute that history.

Native multicam state composes those same timeline and clip owners. One synchronized source
timeline stores ordered `MulticamAngleId` metadata and clip membership, while each ordinary nested
target clip stores an independent source-clock switch partition and explicit follow-video,
fixed-angle, or all-angle audio intent. Resolution follows the target clip time map, active angle
membership, and selected source clip time map without flattening the direct source relationship.
Structural fragments and replacements inherit source membership and target switch intent through
the shared atomic edit path. A dedicated timeline mutation batch creates or replaces source state,
changes sync provenance, attaches nested targets, maps exact record-time takes and cut moves through
the canonical clip time map, changes audio intent, and detaches targets at one revision. Engine
compound transactions recompile retained graphs and add history restoration. Public schema `1.7.0`
and catalog `1.9.0` expose all seven mutations and ordered evidence through generated TypeScript,
and the desktop timeline uses that route for setup, engine-state angle viewing, switching, frame
refinement, sync, audio, detach, and undo. Graph compilation retains that intent as typed
parameters, and the project document retains the complete compilation. The desktop supplemental
clip projection also validates and displays complete angle identity, enabled state, exact switch
ranges, and full audio policy through source and program consequence strips. Runtime graph
evaluation, decoded angle playback, and multicam audio mixing remain absent.

Versioned timeline state documents preserve the complete editable owner graph without claiming the
project file boundary. `serialize_timeline_state` emits canonical `superi.timeline` revision 2 JSON
with the stable primitive revision and SHA-256 payload integrity. `deserialize_timeline_state`
strictly rejects corrupt, interrupted, unknown, oversized, or future state, migrates revision 1 and
revision 0 in memory with explicit former track defaults, reconstructs through checked media,
timeline, annotation, relationship, retime, nesting, and multicam APIs, and exposes canonical current
bytes only after whole-project validation. Strict
`TimelineGraphValue` Serde also preserves compiled graph payloads through the graph codec.
`superi-project` stores canonical current timeline, settings, graph, audio, extension, and command-log
records inside stable SQLite schema 5. Its exact schema-0, schema-1, schema-2, schema-3, and schema-4
migrations accept declared
timeline and graph component revisions through the owning codecs, derive deterministic settings
from the root rate at the 1-to-2 step, add canonical empty audio at the 2-to-3 step, add an empty
extension set at the 3-to-4 step, add an empty command log at the 4-to-5 step, and write canonical
current rows before commit. One
complete-candidate command surface publishes save,
save-as, copy, and backup files without moving document meaning into timeline. Project autosave
reuses that surface for host-driven recovery points. Project integrity inspection reuses the exact
schema readers and canonical component owners without writing or migrating the source. Project
diagnostics reuse canonical preparation to expose a versioned semantic hash and ordered component
evidence independently of file path, SQLite layout, outer document revision, and runtime state.
Timeline-driven scheduling, runtime playback, and mixing remain absent. Public database file
consumers now reach retained timeline state through the API-owned local host and CLI over the
project authority. Recovery discovery and restoration consume the retained timeline and authored
audio meaning through project, engine, and API.

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
selection, monotonic restoration, and exact dismissal are implemented through project, engine, and
API owners. Persistent command journals remain unimplemented.
Engine graph
evaluation, GPU effect and transition execution, production timeline-to-transition binding,
rendered comparison, color delivery, and muxing are not
integrated. The new encoder path begins from caller-prepared frames and ends at elementary derived
packets, so it does not close that slice. There is no
current test or runtime that imports through the engine, selects and decodes original media,
edits a timeline, evaluates a graph, applies input and output color, renders through the GPU,
encodes and muxes output, persists a project, and drives the flow through the public API.

## Placeholders and incomplete integration

No entire crate remains a pure skeleton. `superi-ai` now owns honest unavailable-runtime capability
discovery, while its model audit, loading, pipeline, inference, and artifact modules remain
placeholders. `superi-project` now has a substantive in-memory
document aggregate, immutable snapshots, checked whole-project edits, retained timeline and named
standalone graphs, authoritative versioned settings, authored clip-mix state, bounded opaque
extension records, a bounded durable command log, stable schema-5 SQLite serialization, exact
schema-0 through schema-4 migration, real engine settings and resource consumers, stable public settings, and
project-internal extension command surfaces, plus versioned portable referenced-media paths and
revision-fenced path and relink commands, semantic no-op suppression, and checked monotonic snapshot
restoration consumed by engine command history. Its one typed save command builds and validates
complete same-parent candidates, atomically replaces or claims destinations, preserves copy and
backup identity, and rebinds save-as at the publication commit point. Its typed autosave controller
provides deterministic scheduling, complete Backup recovery points, bounded generation retention,
safe pruning, and direct user control. Its recovery controller adds exact restart discovery,
complete typed comparison including authored clip-mix and extension state, classified findings,
revalidated opaque identity, durable tombstone dismissal, and engine-coordinated restoration.
One public read-only integrity command performs complete current and supported legacy
reconstruction with bounded deterministic repair reporting. Replacement publication is serialized
through a persistent sibling lock and guarded by validated active and destination file generations.
Persisted undo and redo branches and transport-catalog database adaptation remain absent. The
API-owned local host and CLI now compose the existing project database, file commands, and
permission-checked command-log query, while the project crate
owns no interpreter or source loader. The API-local runtime above it uses existing commands and
preserves this owner through persistence, integrity, autosave, and recovery. Unknown extension
kinds, versions, schemas, and payload bytes are preserved, but runtime factory availability and
public extension adapters remain separate owners.
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
effects, revisioned clip-gain automation, a graph-native meter, fixed route delay compensation, a
bounded native-state envelope, a timing-matched isolated process bridge, a macOS Audio Unit effect
host with default verified process isolation, and a worker-side VST3 audio-effect host. Audio Unit
class-info state and VST3 component plus controller state round-trip exactly, both hosts publish fixed
native latency, and the graph aligns direct, send, and auxiliary-return branches before processing.
Engine now discovers native audio candidates, validates separate worker contracts and descriptors,
captures compatible checkpoints, restarts and quarantines faults, prepares the isolated bridge, and
persists one exact record per audio node through project save and reopen. Concrete platform IPC,
sandbox launchers, heartbeats, kill integration, Audio Unit registry enumeration, dynamic latency
rebuild, automation persistence, broader effect automation, Audio Unit instruments, and decoded-sample
binding remain absent. Engine foreground playback
feeds the bounded output producer and consumes its actual
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

- `superi-api`: network hosting, live wire routing, general dynamic dispatch, push delivery,
  persisted replay, public job submission, host polling and waits, typed job
  results, authentication, operating-system sandboxing, shell source loading, general-purpose code
  execution, and full-catalog automation. Generic authored project
  control, the deterministic catalog, strict JSON-RPC data shapes, and safe structured error
  projection plus bounded exact-source `superi-json` interpretation are implemented, including host-injected filesystem, plugin, and destructive
  authorization plus catalog metadata and generated TypeScript declarations, maps, and client.
  Permission-free extension discovery is implemented as a strict immutable projection of the
  engine registry with exact identity, lifecycle, capabilities, features, safe failures, and the
  existing durable project control reference. It does not expose runtime registration mutation,
  workers, callbacks, factories, paths, dispatchers, or permission authority.
  Stateless API and project version negotiation is implemented without claiming runtime protocol
  downgrade or transport routing.
  The local project host and CLI now implement no-clobber create, open, durable mutation, copy,
  backup, recovery, validation, render settings, and a narrow caller-correlated JSON-RPC adapter.
  Media, complete engine introspection, complete editor replacement
  state, and coherent integration validation remain read-only surfaces;
  project settings and clip-gain automation inspection and mutation plus strict project recovery
  control and asynchronous export-job inspection, progress, cooperative control, and ordered
  replacement events are implemented. Bounded event registration, polling, backpressure, replay,
  and reconnect recovery are also implemented as transport-neutral API state.
- `superi-project`: persisted history and command journals, public database API adaptation, CLI,
  and any crate-local scripting interpreter beyond its implemented document, settings, extension state, database, migration,
  media path, collaborative-safe atomic save, autosave, recovery, read-only integrity command, and
  semantic diagnostics owners.
- `superi-audio`: binding decoded samples into the real prepared graph, concrete platform worker
  transport, shared-memory or IPC adapters, heartbeat and kill control, Audio Unit instruments,
  MIDI, preset browsing, plug-in UI, dynamic latency rebuild, automation persistence, broader effect
  automation modes, and engine composition across schedule, conversion, clip and effects processing,
  graph routing, and device execution. Clip-gain automation, Audio Unit and VST3 effects, exact
  native state, fixed delay compensation, isolated dry fallback, engine lifecycle supervision, and
  per-node project persistence are implemented; export currently owns only an explicit stage seam.
- `superi-color`: broader config-persisted rule graphs, arbitrary ICC transform evaluation, HDR and
  broader GPU output conversion, engine-owned viewport composition, and production export
  integration. The desktop's exact monitor-bound sRGB and Display P3 viewport path, including all
  eight deterministic inspection modes, is implemented; decoded-render binding remains absent.
- `superi-concurrency`: GPU submission coordination module and production composition beyond the
  audio domain, derived-media selection, playback and export workers, export dependency history,
  clocks, bounded handoffs, and lifecycle control consumers.
- `superi-engine`: one placeholder orchestration module covering nodes.
  Classified cross-subsystem error propagation and recovery is implemented beside the canonical
  lifecycle. Project and device power lifecycle is an explicit action boundary, but platform
  callbacks and native device owners
  are not yet bound to those actions. Typed project media and extension command history, full
  snapshot undo and redo, project settings, compound transactions, and selected-snapshot autosave
  compatibility use the same authoritative project state. A curated editor seam supports the
  strict generic public adapter without moving behavior or wire ownership into engine. Project
  recovery discovery, comparison,
  restore, and dismissal use that same dispatcher and active database, but production background
  autosave hosting, live subsystem reconfiguration, callbacks,
  and native device owners remain
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
  typed results remain runtime local. The public API now projects inspection, progress, cooperative
  controls, and ordered replacement events without cataloging submit, poll, wait, or typed-result
  access. Native GPU readback, container muxing, publication,
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
- `superi-graph`: invalidation and ROI render orchestration, outer job dispatch, engine
  coordination, cache invalidation invocation and
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
  native subset, graph evaluation, fit-to-fill and broader higher-level edit orchestration, multicam
  playback and mixing, autosave and recovery-journal orchestration, and application consumers beyond
  the delivered canonical canvas with application-owned interaction selection, exact transient
  target snapping, root-anchored open-in-timeline navigation, existing-child placement, and
  selection-derived compound commands over its native model, authoritative edit state, marker and
  metadata state, exact snapping, foundational, advanced, nested, and multicam edit operations,
  deterministic graph compilation, versioned state documents, shared processing-payload
  compatibility, OTIO headless consumer, and contract tests.

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
  audio-clock publication, capture and output telemetry, revisioned clip-gain keyframes plus Read,
  Write, Touch, and Latch behavior, fixed route delay compensation, format-neutral native state,
  timing-matched isolated bridge fallback, macOS Audio Unit and worker-side VST3 effect hosting, and
  graph-native peak, RMS, true-peak, phase, spectrum, and loudness analysis: `superi-audio`, followed
  by `superi-engine` for discovery, worker validation, checkpoint recovery, quarantine, and per-node
  project records.
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
- Coherent whole-project snapshots, authored clip-mix and opaque extension state, deterministic
  versioned semantic hashing and ordered component diagnostics, schema-5 SQLite storage, supported
  forward migration, deterministic autosave recovery points, read-only whole-project integrity
  validation and bounded repair reporting, active
  project path and generation identity, collaborative-safe locked save, save-as, copy, and backup
  publication, portable media paths,
  revision-fenced relink commands, deterministic autosave scheduling, managed recovery points,
  bounded retention, safe pruning, restart recovery discovery, typed comparison, classified
  findings, exact durable dismissal, and a bounded project-owned command log with monotonic cursors,
  request evidence, and active-lineage recovery: `superi-project`.
- Complete reusable-result identity, budgeted final-frame and intermediate-node memory retention,
  exact total, project, and device admission, priority-aware LRU eviction, precise revision-safe
  graph edit invalidation, versioned bounded disk persistence with corruption recovery, cache color
  identity, complete derived-media publication, deterministic inspection and clearing, safe
  persistent relocation, layered render reuse, bounded background population, and bounded
  exact-frame edit and scrub warming: `superi-cache`, followed by `superi-engine` for codec
  generation and transparent substitution and `superi-concurrency` for quality choice and
  background job execution.
- Native editorial objects, typed track semantics, exact timing and clip retiming, selection, track
  creation, deletion, naming, height, order, targeting, locks, sync locks, mute, solo, enable,
  linked selection, clip grouping, all six atomic marker gestures, deterministic metadata, exact snapping, and
  foundational insert, overwrite, append, replace, lift, and extract operations plus
  advanced ripple, roll, slip, slide, razor, trim, extend, three-point, and four-point operations,
  plus nested placement, compound creation, shared child editing, recursive nesting inspection,
  multicam angle metadata, synchronization provenance, switching, audio intent, exact resolution,
  and deterministic typed editable graph compilation:
  `superi-timeline`.
- Durable project settings, defaults, strict candidate validation, schema-5 persistence, and
  migration: `superi-project`, followed by `superi-engine` for typed subsystem resolution and
  dispatcher control, then `superi-api` for the stable transport-neutral surface.
- Crash recovery discovery, comparison, monotonic active-project restoration, exact dismissal, and
  user-safe public control: `superi-project`, followed by `superi-engine`, then `superi-api`.
- Desktop crash evidence, active-session detection, bounded safe projection, and cross-session route,
  dock, tab, size, panel, focus, and project intent: `workspace`, which routes engine recovery to the existing lifecycle owner
  and project restoration to the existing `superi-project`, `superi-engine`, and `superi-api` path.
- Application context menus, tooltips, notifications, operational status, semantic progress, and
  classified error presentation: `workspace`, which composes safe public and shell-local evidence
  while routing project, editor, lifecycle, crash, and workspace actions back to their existing
  owners.
- Process-lifetime extension registration, exact runtime identity, capability and lifecycle
  discovery, safe failure state, and stable user control: `superi-engine` for the bounded declarative
  registry and supervisor adapters, followed by `superi-api` for the permission-free strict query,
  event, resource, and generated contracts, then `superi-project` for durable mutation authority.
- Public command, query, event, resource, error, capability, and permission schema discovery plus
  strict JSON-RPC data contracts and host-injected pre-dispatch authorization: `superi-api`, followed
  by `superi-cli` for exact-fixture scenario, schema, durable local project, and JSON-RPC process
  consumers.
- Stable typed project command execution, atomic outcome recording, bounded durable retention,
  cursor-safe metadata inspection, and permission-checked replay disclosure: `superi-api` owns the
  public request and query shapes, `superi-engine` owns atomic dispatch, `superi-project` owns log
  durability outside authored semantic state, and `superi-cli` plus the workspace frontend consume
  the same public surface.
- Bounded deterministic local project scripting, exact-source identity, conflict traces, and nested
  permission preflight: `superi-api`, followed by `superi-engine` for the sealed project dispatcher,
  and `superi-project` for durable meaning, integrity, autosave, and recovery ownership.
- Durable project format release identity and compatibility: `superi-project`, reexported without
  behavior by `superi-engine`, projected by `superi-api` through the stateless version query, then
  consumed by CLI schema discovery and the workspace frontend contract.
- Durable project create, inspect, mutation, media and timeline partitioning, render settings,
  validation, copy, backup, recovery, and ordered automation: `superi-project` for persistence,
  `superi-engine` for scoped dispatch, `superi-api` for the local host and typed DTOs, then
  `superi-cli` for the process grammar and acknowledgement boundary.
- Deterministic TypeScript API declarations, exact typed maps, committed artifact freshness, and the
  transport-neutral client: `superi-api`, followed by `tool-superi-api-bindings` for generation and
  `workspace` for the committed artifact and frontend compile-time consumer.
- Asynchronous job handles, progress, cooperative control, and ordered replacement events:
  `superi-engine` for canonical queue ownership, followed by `superi-api` for the stable public
  projection and `superi-cli` for schema discovery only.
- Current assembly, public capability, health, and coherent integration validation flow:
  `superi-engine`, then `superi-api`, then `superi-cli` for the process consumer.
- Desktop source-monitor loading, exact seek, fingerprint-bound in and out marks, sidecar
  publication, and React engine-state projection: `workspace`, with source registry assembly in
  `superi-engine` and source-open plus exact seek contracts in `superi-media-io`.
- Persistent desktop window creation, restoration, fullscreen, monitor movement, reversible
  placement, per-webview transport generations, route continuity, and primary native viewport
  ownership: `workspace`.
- Production editing timeline, exact target snapping, clip and caption presentation, foundational
  gestures, caption exchange, and direct retime authoring:
  `superi-timeline` for
  canonical editorial identity, timing, and edit semantics, `superi-graph` for topology and drivers,
  `superi-audio` for exact clip-gain keys, `superi-project` for durable replacement,
  `superi-engine` for history and events, `superi-api` for generated state and command contracts,
  then `workspace` for strict projection, source and target planning, freshness-fenced media visuals,
  exact owner-clock target projection, complete caption fields, bounded SRT and WebVTT exchange,
  fresh transcript conversion, visible session rules and consequences, all nine gestures,
  all four three-point placements, equal-duration four-point editing, immediate reversal, and shared
  selection, exact audio-video link, source-time synchronization, detach, replacement, complete
  channel routing, exact advanced timing gestures published as atomic action batches, exact speed,
  reverse, freeze, and multi-segment time-map controls, root-anchored nested open paths, cycle-safe
  append or equal-duration replace placement, and deterministic selection-derived compound creation
  through the same authored command owner.
- Shared finite-resource arbitration across decode, GPU, cache, audio, AI, and export workloads:
  `superi-engine`, followed by each lower subsystem owner for its authoritative local allocation and
  release behavior.
- Product law, open and closed boundaries, CI, fixtures, and maintenance workflow: `workspace`.
- Canonical first editorial slice, typed scenario state, replacement stages, and proof: `workspace`.
- Reviewed internal runtime dependency direction: `tool-superi-dependency-check`.
- Static network-client and open-to-closed enforcement: `tool-superi-boundary-tool`.
- Deterministic structured platform-lane evidence: `tool-superi-test-report`.
- Deterministic TypeScript artifact generation and freshness checks: `tool-superi-api-bindings`.

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
