---
module_id: superi-timeline
source_paths:
  - open/crates/superi-timeline
source_hash: 03cb51d87cea95eb0537f796062671d3678033fd8b0575eeb9b86ec4e0808d56
source_files: 34
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-timeline` owns the foundational Rust-native editorial project model and typed track
semantics. It represents linked media, timelines, ordered tracks, clips, explicit gaps,
transitions, generators, captions, and nested timeline sources with core-owned identities and
exact rational timing. It also owns authoritative timeline selection, bounded track height,
targeting, authored-item locks, sync locks, audio mute and solo, output enable, linked selection,
and clip grouping. Video, audio, caption, and timed-data tracks carry their
explicit clock and media behavior. Clip range maps keep source and record clocks synchronized,
while resolved range contexts expose known media availability or derived nested-timeline
availability without destroying overscan. Timeline, track, and object markers preserve permanent
identity, explicit ownership, owner-relative exact ranges, visible labels, flags, notes, and nested
deterministic metadata, and one atomic marker batch owns create, range, label, flag, note, and remove
gestures. Persistent snapping resolves exact timeline, playhead, item, and visible
marker boundaries with stable filters, exclusions, and tie ordering. Foundational insert,
overwrite, append, replace, lift, and extract commands join ripple, roll, slip, slide, razor, trim,
extend, atomic transition-handle replacement, and exact three-point and four-point commands on one
typed batch surface. Those commands
report every inserted, removed, modified, split, synchronized, or invalidated relationship.
Exact retime replacement uses that same surface to change one clip's complete time map while
preserving its identity and record duration and rejecting semantic no-ops.
Whole-project validation and revision-checked atomic batches keep linked objects, annotations, user
intent, timing, synchronization, nesting, and direct edits valid at publication boundaries.
Clip-owned exact time maps add rational speed changes, reverse playback, freeze frames, and
continuous piecewise-linear time remapping without weakening nominal range invariants. Immutable
segment storage and binary-search record-to-source queries expose exact, held, explicitly rounded,
known, unknown, and unavailable transport behavior directly.

Project media state includes stable manual bins and sub-bins, deterministic metadata, saved smart
collections evaluated from current media state, and explicit online, missing, unverified, or
fingerprint-mismatch relink evidence. The media library shares the project draft transaction, so
organization and relink changes preserve stable `MediaId` source links, exact timing, synchronization,
nested relationships, and subsequent direct edits.

Nested-sequence operations place an existing child timeline, add a prepared compound timeline, or
derive a compound timeline directly from a complete object selection in one project revision. The
selection path preserves original object identity, source, record duration, time maps, annotations,
metadata, links, groups, transitions, track semantics, and edit intent while rebasing the selected
record span to child zero. It leaves implicit internal gaps explicit, replaces each affected parent
track with one linked and grouped nested instance in canonical track order, and rejects relation or
transition boundaries that would make the selection incomplete. Placement reuses foundational
insert, overwrite, append, and replace semantics, exposes exact parent-to-child links and recursive
nesting, and edits a shared child through any stable instance while reporting every current
instance.

Native multicam state keeps ordered camera-angle metadata and synchronization provenance on one
ordinary source timeline while each ordinary nested target clip owns an independent gapless switch
program and explicit audio policy. Exact resolution follows the target clip time map into source
timeline coordinates, selects the active angle, resolves its synchronized source clip, and follows
that clip's own time map to a directly inspectable media or nested-source coordinate. Fragment and
replacement operations inherit target switch intent and source-angle membership through the same
atomic edit path used by selection, links, groups, annotations, nesting, and retiming.

Versioned `superi.timeline` component documents preserve that complete editable project state in
canonical JSON. Revision 2 records the stable core primitive revision, protects the canonical
payload with SHA-256, rejects malformed or unknown state, migrates the supported revision 1 and
revision 0 envelopes in memory, and reconstructs every owner through checked constructors and
timeline APIs. Revision 1 state receives the exact former 72 pixel height plus neutral target,
lock, mute, solo, and enable defaults.
The codec returns canonical current bytes only after whole-project validation. It owns no file I/O,
SQLite schema, autosave policy, replacement protocol, or recovery journal. `superi-project` now
retains this editorial state in memory and stores the canonical component bytes inside its stable
schema-3 SQLite database, while timeline remains the only owner of component interpretation.
Linked media target text remains opaque in this crate. `superi-project` recognizes versioned
filesystem targets, resolves portable project-relative paths, and exposes revision-fenced path and
relink commands while continuing to mutate this single timeline-owned media state.

The model also owns a narrow immutable color metadata seam that retains graph color state through
the future compilation boundary without changing source meaning.

The crate now compiles one selected timeline and every reachable nested timeline into one typed,
editable `superi-graph` document. Stable domain-separated identifiers preserve graph addresses
across semantic edits, while a bidirectional provenance index keeps every timeline, track, and
editorial object understandable and directly controllable. Timeline outputs, ordered tracks,
clips, gaps, transitions, generators, captions, and nested-sequence routing remain ordinary typed
graph state wrapped losslessly in the shared graph payload. Native multicam source catalogs,
synchronization provenance, switching, and audio intent remain ordinary typed graph parameters,
while catalog-owned processing values can coexist in the same editable graph without a timeline
dependency on an effects catalog.

The crate also owns offline OTIO 0.18.1 JSON interchange. Import maps root and nested stacks,
tracks, clips, gaps, transitions, markers, media references, metadata, and linear time warps into
the ordinary native project model. Export rebuilds the current edited hierarchy and merges native
values into complete preserved source templates, retaining unknown fields and unsupported effects
with stable warnings.

## Source inventory

- `open/crates/superi-timeline/Cargo.toml`: Declares runtime dependencies on `superi-core`,
  `superi-graph`, `serde`, `serde_json`, and the workspace-pinned SHA-256 implementation for the
  model, color seam, strict timeline documents, canonical JSON, integrity digests, and offline OTIO
  interchange.
- `open/crates/superi-timeline/src/compile.rs`: Compiles validated root and nested timelines into
  one typed editable graph transaction with stable graph, node, port, parameter, and edge IDs,
  explicit stream routing, authored track and item order, complete object and multicam parameters,
  bidirectional editorial provenance, and a shared graph payload that retains all native values as
  exact domain variants beside catalog-neutral processing values. It can install externally checked
  editable state only when the deterministic graph identity matches the compiled project and root.
  It also retains typed enable, mute, and solo output intent independently from nonprocessing track
  controls and performs a checked three-way recompile that applies new canonical editorial
  structure while preserving nonconflicting retained parameters, custom nodes, and custom edges.
- `open/crates/superi-timeline/examples/otio_roundtrip.rs`: Imports one OTIO document through the
  public native boundary, reports stable diagnostics, and writes deterministic OTIO 0.18.1 JSON.
- `open/crates/superi-timeline/src/edit_ops.rs`: Implements directly inspectable foundational and
  advanced commands, exact source-aware and retime-aware trimming and splitting, direct exact clip
  time-map replacement, semantic no-op rejection, deterministic fragment identities, explicit
  sync-locked ripple plans, transition reconciliation, result reports, locked-track enforcement,
  atomic dual-handle transition timing, and atomic multi-track batches.
- `open/crates/superi-timeline/src/edit_state.rs`: Implements exact and relationship-expanded
  selection, bounded track height, per-track target, lock, sync-lock, mute, solo, and enable intent,
  canonical clip links and groups, stable introspection, and structural reconciliation.
- `open/crates/superi-timeline/src/ids.rs`: Re-exports the canonical project, editorial object, and
  multicam angle identities owned by `superi-core`.
- `open/crates/superi-timeline/src/lib.rs`: Exports the implemented identity, edit-state, edit
  operation, track-operation, marker-operation, model, media, retime, nesting, marker, multicam, serialization, OTIO,
  and graph compilation modules.
- `open/crates/superi-timeline/src/marker_ops.rs`: Implements one revision-checked atomic batch for
  complete marker creation, exact range replacement, label, flag, and note replacement, and removal,
  with strict identity and target checks, field-preserving partial edits, and typed outcomes.
- `open/crates/superi-timeline/src/markers.rs`: Implements stable timeline, track, and object marker
  ownership, visible labels, flags, notes, recursively nested ordered metadata, owner-relative range
  resolution, dangling-owner reconciliation, persistent snapping state, exact candidate projection,
  target filters, exclusions, and deterministic tie resolution.
- `open/crates/superi-timeline/src/media.rs`: Implements metadata-bearing linked media, persistent
  relink evidence, stable manual bins and parent paths, direct membership movement, saved smart
  collection predicates, deterministic query evaluation, and complete media-library validation.
- `open/crates/superi-timeline/src/model.rs`: Implements four track kinds, track-specific timing and
  media semantics, exact clip range maps, clip-owned time maps, linked range and playback
  availability context, every foundational editorial object, ordered tracks, timelines, annotation
  integration, multicam source and clip ownership, project-wide multicam reconciliation and
  validation, checked track insertion, deletion, ordering, naming, and control changes, validated
  project snapshots, atomic revision-checked editing, and
  `TimelineColorMetadata`, which retains exact graph color metadata through compilation.
- `open/crates/superi-timeline/src/multicam.rs`: Implements synchronization provenance, ordered
  angle metadata and source membership, clip-local gapless switching, explicit audio policies,
  movable cuts, exact nested and retimed source resolution, and structured multicam errors.
- `open/crates/superi-timeline/src/nested.rs`: Implements exact nested placement, atomic prepared
  compound creation, selection-derived multi-track compound creation with complete identity and
  relationship preservation, direct child editing by stable instance identity, shared-instance
  inspection, recursive nesting inspection, and typed outcomes over the foundational edit owner.
- `open/crates/superi-timeline/src/otio.rs`: Implements dependency-light OTIO 0.18.1 JSON import
  and export, exact time conversion, deterministic native identity allocation, explicit audio
  defaults, supported object mapping, complete source-template preservation, stable unsupported
  diagnostics, native edit projection, and deterministic target serialization.
- `open/crates/superi-timeline/src/retime.rs`: Implements reduced signed playback rates,
  continuous clip-local retime segments, immutable complete time maps, exact and explicitly
  rounded record-to-source queries, reverse and freeze constructors, direct mode inspection,
  retime slicing, clock replacement, and source translation.
- `open/crates/superi-timeline/src/serialize.rs`: Implements the strict revisioned timeline state
  envelope, complete wire model, stable collection canonicalization, primitive revision check,
  SHA-256 integrity, explicit revision 1 and revision 0 migration, checked reconstruction,
  canonical load output, and classified recovery failures without file I/O. It also implements
  strict lossless Serde for
  every `TimelineGraphValue` variant by reusing checked timeline-owned wire conversions.
- `open/crates/superi-timeline/src/track_ops.rs`: Implements one revision-checked atomic batch for
  track creation, deletion, naming, height, order, targeting, locks, sync locks, audio mute and
  solo, and output enable, with deterministic four-kind creation templates and typed outcomes.
- `open/crates/superi-timeline/tests/edit_state_contract.rs`: Proves linked and grouped selection,
  direct member control, target and sync-lock ordering, link and group independence, state
  reconciliation, identity and timing retention, revision conflicts, and atomic rollback.
- `open/crates/superi-timeline/tests/model_contract.rs`: Proves every foundational object,
  cross-rate and cross-track synchronization, linked media and nesting, direct edits, revision
  conflicts, atomic rollback, transition bounds, continuity, missing links, and nesting cycles.
- `open/crates/superi-timeline/tests/multicam_contract.rs`: Proves ordered angle metadata,
  synchronization provenance, exact retimed nested resolution, switch and cut edits, fixed and
  all-angle audio policy, source and target fragment inheritance, replacement inheritance,
  disabled-angle rejection, stale revisions, and atomic rollback.
- `open/crates/superi-timeline/tests/nested_contract.rs`: Proves exact cross-clock nested placement,
  retained child object and command state, prepared and selection-derived compound creation, exact
  mixed-clock rebasing, track intent, transitions, annotations, metadata, links and groups, shared
  instances, recursive nesting, direct child edits, stale revisions, and atomic boundary, range,
  identity, source, and cycle rejection.
- `open/crates/superi-timeline/tests/edit_ops_contract.rs`: Proves all six foundational operations,
  exact cross-rate source slicing, nested source preservation, typed fragment identities, explicit
  transition removal and dual-handle replacement, lift gaps, synchronized multi-track publication,
  exact outcome evidence, and failed-batch rollback.
- `open/crates/superi-timeline/tests/markers_contract.rs`: Proves stable marker identity, timeline,
  track, and object ownership, visible semantics, nested metadata, all six atomic marker mutations,
  owner and metadata preservation, duplicate and missing-target rejection, direct mutation, owner-relative
  resolution, preserved overscan, structural-edit survival, dangling-owner cleanup, exact snapping,
  filters, exclusions, persistent disablement, stable ties, empty-batch rejection, and late atomic rollback.
- `open/crates/superi-timeline/tests/media_library_contract.rs`: Proves bins, sub-bins, stable paths,
  direct membership, cycle and duplicate rejection, metadata-driven smart collections, missing,
  unverified, mismatched, and accepted relinks, atomic rollback, stable media identity, and
  preservation of retime, links, groups, sync state, nesting, and subsequent direct edits.
- `open/crates/superi-timeline/tests/advanced_edit_ops_contract.rs`: Proves ripple, roll, slip,
  slide, razor, trim, extend, all four three-point forms, exact four-point placement, explicit
  fit-to-fill rejection, sync-locked companion tracks, relationship inheritance, cross-rate
  derivation, transition truth, annotation retention, typed object splits, caller parity, and atomic
  rollback.
- `open/crates/superi-timeline/tests/compile_contract.rs`: Proves deterministic typed graph state,
  exact nested and transition routing, stable addresses across source-range edits, bidirectional
  provenance, unchanged processing state after a selection-only edit, direct graph parameter
  editing, coexistence with a linked shared scalar processing node, typed multicam source, switch,
  and audio intent, three-way retained-edit preservation, overlapping parameter conflict rejection,
  and missing-root failure.
- `open/crates/superi-timeline/tests/otio_fixture_contract.rs`: Proves canonical OTIO schema,
  hierarchy, identity, timing, relationships, opaque retention, and unsupported diagnostics.
- `open/crates/superi-timeline/tests/otio_interchange_contract.rs`: Proves production fixture
  import, native hierarchy and exact timing, media and nested links, markers, linear retiming,
  explicit audio defaults, deterministic export, direct edit reimport, opaque retention, stable
  warning pointers, duplicate identity rejection, and inexact clock rejection.
- `open/crates/superi-timeline/tests/range_contract.rs`: Proves exact cross-clock point and subrange
  mapping, fallible atomic range replacement, media overscan classification, unknown availability,
  and derived nested-timeline availability.
- `open/crates/superi-timeline/tests/retime_contract.rs`: Proves speed changes, reverse, freeze,
  piecewise time remapping, exact seams, explicit quantization, point availability, atomic binding,
  identity resize compatibility, linked intent, and retime preservation through edit splitting.
- `open/crates/superi-timeline/tests/retime_edit_ops_contract.rs`: Proves direct speed, reverse,
  freeze, and multi-segment time-map replacement through the ordinary edit batch, exact outcome
  evidence, stable clip identity and record duration, and atomic rejection of no-ops, locked tracks,
  missing clips, and wrong track bindings.
- `open/crates/superi-timeline/tests/track_semantics_contract.rs`: Proves all four track kinds,
  exact clocks, channel routing, linked audio reshaping, continuity, and bounded validation.
- `open/crates/superi-timeline/tests/track_management_contract.rs`: Proves all eleven track
  operations in one atomic batch, stable survivor state, lock enforcement, explicit unlock and
  delete, audio-only controls, bounded heights, authored-item rejection, and rollback.
- `open/crates/superi-timeline/tests/serialization_contract.rs`: Proves deterministic complete
  state round trips, revision 1 and revision 0 migration, canonical current output, corruption and
  interruption rejection, strict unknown and future state handling, multicam recovery, and
  continued direct editing after load.

## Public surface

The `ids` module re-exports `ProjectId`, `MediaId`, `BinId`, `SmartCollectionId`, `TimelineId`,
`TrackId`, `ClipId`, `GapId`, `TransitionId`, `GeneratorId`, `CaptionId`, `MarkerId`, and
`MulticamAngleId`. These are the same sealed core identifier types used by every other subsystem.

The track semantics surface includes:

- `TrackKind` and `TrackSemantics` for video, audio, caption, and timed-data media classes with one
  exact edit clock.
- `VideoTrackSemantics` and `VideoCompositing` for frame rate and visual contribution intent.
- `AudioTrackSemantics`, `AudioRouting`, `AudioRouteDestination`, `AudioChannelRoute`, and
  `AudioChannelTarget` for integral sample clocks, ordered source meaning, explicit track or main
  destinations, output channel meaning, and explicit mute decisions.
- `AudioSpan` for `ClipId`-linked record-to-source sample mapping, plus
  `AudioContinuityReport`, `AudioSeam`, `AudioRecordContinuity`, and `AudioSourceContinuity` for
  checked splits, trims, record coverage, and source continuity.
- `CaptionTrackSemantics`, `CaptionPurpose`, and `LanguageTag` for exact cue clocks, presentation
  intent, and normalized bounded language-tag syntax.
- `DataTrackSemantics` and `DataSchema` for exact event clocks and bounded payload type identity.

The editorial state surface includes:

- `LinkedMediaReference`, including stable media identity, display name, opaque target locator, optional
  available source range, deterministic `MediaMetadata`, and persistent `MediaRelinkState`.
- `RelinkStatus` and `RelinkDecision` expose online, missing, unverified, and rejected mismatch
  behavior without replacing the active target or stable identity on a failed content check.
- `MediaBin` and `MediaLibrary` expose stable hierarchical organization, root and child iteration,
  root-to-leaf paths, direct membership, and atomic full-library validation.
- `SmartCollection`, `SmartCollectionMatch`, and `MediaPredicate` expose saved all or any queries
  over media names, targets, metadata keys and values, and relink status. Results are derived in
  stable `MediaId` order rather than stored as stale membership.
- `ClipSource`, which links a clip to either media or another timeline.
- `ClipRangeMap` for nonempty equal-duration source and record ranges plus checked exact point and
  subrange translation in both directions.
- `ClipRangeContext` and `RangeAvailability` for resolving a clip's typed source, synchronized
  ranges, optional availability, and unknown, full, partial, or unavailable range status.
- `SampleAvailability` and `ClipPlaybackSample` for transport-ready point resolution with visible
  exact, held, explicitly rounded, known, unknown, or unavailable behavior.
- `Clip`, `Gap`, `Transition`, `Generator`, and `Caption`, each with typed identity and direct
  mutation inside unpublished state.
- `TrackItem` and `Track`, preserving ordered editorial membership, complete `TrackSemantics`, and
  typed lookup.
- `Timeline`, including one primary edit rate, global start, ordered tracks with independent exact
  clocks, exact rational duration, lookup, and direct mutation.
- `EditorialProject`, the validated immutable snapshot, and `ProjectDraft`, the unpublished
  mutable candidate passed to a revision-checked edit closure.

The timeline edit-state surface includes:

- `SelectionUpdate` for replace, add, and remove intent, plus `SelectionExpansion` for ordinary
  relationship following or exact direct-object control.
- `TrackEditState` for one stable track's bounded height, targeted, locked, sync-locked, muted,
  solo, and enabled intent.
- `ClipRelation`, a deterministic set of stable `ClipId` members directly addressable through any
  member, without inventing a second clip or group identity domain.
- `TimelineEditState` for selected objects, track controls, the linked-selection toggle, clip link
  components, and clip groups.
- `Timeline` operations to select objects, link and unlink clips, group and ungroup clips, set track
  intent, insert, delete, reorder, and control tracks, enumerate targeted tracks by timeline order
  and media kind, and resolve sync-affected tracks for later insert and ripple commands.
- `TrackCreationKind`, `TrackMutation`, `TrackMutationKind`, `TrackMutationOutcome`, and
  `TrackMutationBatchResult` for explicit video, audio, caption, and data creation plus all eleven
  track gestures through one atomic project revision.

The annotation and snapping surface includes:

- `MarkerOwner` for explicit timeline, track, or stable editorial-object ownership, plus `Marker`
  with core-owned `MarkerId`, an exact owner-relative `TimeRange`, and directly replaceable
  `MarkerLabel`, `MarkerFlag`, `MarkerNote`, and marker metadata.
- `MarkerMutation`, `MarkerMutationKind`, `MarkerMutationOutcome`, and
  `MarkerMutationBatchResult` for complete creation plus exact range, label, flag, note, and remove
  gestures through one ordered atomic project revision.
- `MetadataKey`, `MetadataValue`, and `TimelineMetadata` for deterministic ordered state with null,
  Boolean, signed and unsigned integer, finite floating-point, text, exact time, exact range, list,
  and nested-map values.
  `MetadataOwner` attaches maps directly to the timeline, a track, an editorial object, or a marker.
- `Timeline` marker lookup, stable iteration, direct unpublished mutation, insert, remove, metadata,
  and visible range resolution. Object-owned marker timing remains relative to the stable object's
  record start, so structural shifts do not rewrite authored intent.
- `SnapRequest`, `SnapTargetKind`, `SnapTarget`, and `SnapMatch` for persistent, exact, nonmutating
  snap queries over timeline zero, a caller playhead, item boundaries, and visible marker boundaries.
  Requests can include target classes and exclude moving objects or markers.

The retime surface includes:

- `PlaybackRate`, a reduced signed rational source-to-record ratio with explicit normal, reverse,
  and freeze constants.
- `RetimeSegment`, a nonempty clip-local record range with one absolute source start and exact rate.
- `ClipTimeMap`, an immutable complete segment set with constant speed, reverse, freeze, custom
  remap construction, direct mode and segment inspection, and allocation-free binary-search lookup.
- `RetimeMode`, `MappedSourceTime`, and `RetimeResolution` for identity, speed, reverse, freeze, and
  remap classification plus exact, held, or caller-selected rounded sample results.
- `Clip::time_map`, `Clip::set_time_map`, and `Clip::source_time_at` for direct checked timing edits
  and absolute record-to-source transport queries without replacing clip identity.

The multicam surface includes:

- `MulticamSyncMethod` for explicit manual, timecode, in-point, out-point, named-marker, or audio
  synchronization provenance.
- `MulticamAngle` and `MulticamSource` for stable angle identity, editor name, camera label,
  enabled state, deterministic metadata, ordered local source clips, and ordered source-level
  inspection and mutation.
- `MulticamSwitch` and `MulticamClip` for one ordinary nested clip's independent gapless source
  partition, range switching, movable cut boundaries, and explicit `MulticamAudioPolicy`.
- `Timeline` methods that attach, inspect, mutate, iterate, and remove source-level or clip-level
  multicam state inside the existing unpublished project draft.
- `resolve_multicam_frame` and `ResolvedMulticamFrame`, which expose the target timeline and clip,
  synchronized source timeline coordinate, selected angle and source clip, direct source
  relationship and time, and active audio angles.

The timeline state document surface includes:

- `TIMELINE_STATE_FORMAT_REVISION`, currently `1`, for the incompatible component document
  contract layered over `STABLE_PRIMITIVE_SCHEMA_REVISION`.
- `serialize_timeline_state`, which emits deterministic `superi.timeline` JSON with complete
  project identity, published revision, media, library, track, item, edit, annotation, retime,
  nested, and multicam state plus a canonical payload SHA-256 digest.
- `deserialize_timeline_state`, which rejects malformed, oversized, corrupt, unknown, or future
  state, migrates revision 0 in memory, reconstructs the model through checked owners, and returns
  no partial project on failure.
- `TimelineStateLoad`, which exposes the validated editable project, source format revision,
  migration status, and canonical current-format bytes for a caller-owned save or recovery policy.

The editorial operation surface includes:

- `EditOperation` and `EditKind` for insert, overwrite, append, replace, lift, extract, ripple,
  roll, slip, slide, razor, trim, extend, `set_transition`, three-point, four-point, and retime
  commands targeted by stable timeline and track identity. `set_transition` changes both exact
  handles of one stable transition together without changing track duration or endpoint identity.
  Retime names one exact clip and replaces only its complete validated `ClipTimeMap`.
- `EditSide` for exact start or end control, `ExtendMode` for explicit ripple or roll delegation,
  and `ThreePointPlacement` for the four forward and backtimed missing-boundary forms.
- `RippleSyncAdjustment` for deterministic per-track gap and fragment identities. A ripple names
  every other sync-locked track in canonical timeline order or fails before mutation.
- Caller-supplied typed right-fragment identities whenever one existing timed object must survive on
  both sides of an edit boundary. The left fragment retains the original object identity.
- `apply_edit_batch`, which applies one or more commands through one existing
  `EditorialProject::edit` publication and returns `EditBatchResult` at the new project revision.
- `EditOutcome`, `EditFragment`, and `TrackDurationChange`, which expose affected ranges,
  inserted and removed objects, changed retained objects, created right fragments, removed
  transitions, synchronized companion tracks, and exact duration effects without reconstructing a
  diff from the final track.

The nested operation surface includes:

- `NestedSequenceRequest` and `NestedSequencePlacement` for an explicit parent timeline, track,
  clip identity, child source range, and insert, overwrite, append, or replace behavior.
- `place_nested_sequence`, which instances an existing child timeline through the foundational
  edit engine, and `create_compound_clip`, which adds a caller-authored child timeline and its
  parent clip atomically without replacing an existing timeline identity.
- `CompoundClipTrackRequest`, `CompoundClipRequest`, and `CompoundClipResult`, plus
  `create_compound_clip_from_selection`, which derive one child track per affected parent track,
  move a complete selected object set and its owned annotations into the child, preserve authored
  relationships and track intent, and replace each selected parent span with one related nested
  instance in a single checked project edit.
- `edit_nested_sequence`, which resolves a child through any stable parent clip, publishes one
  validated child edit, and reports all current parent instances.
- `nested_sequence_instances`, `nested_sequence_tree`, and `NestedSequenceLink`, which expose exact
  parent timeline, track, clip, child timeline, source range, record range, and recursive depth.
- `NestedSequenceResult` and `NestedSequenceEditResult`, which expose the published revision,
  foundational placement outcome, edited child identity, and affected shared instances.

The compilation surface includes `compile_timeline`, `TimelineGraphCompilation`,
`TimelineGraphValue`, `CompiledTimelineGraphValue`, `TimelineGraphOrigin`, and
`TimelineGraphIndex`. `CompiledTimelineGraphValue` is
`GraphValue<TimelineGraphValue>`: every existing editorial value is retained as an exact `Domain`
variant, while shared scalar, vector, color, matrix, Boolean, and choice processing values can be
authored through the same graph. The compilation captures the source project and revision, exposes
the editable graph and immutable snapshots, resolves timeline, track, and object origins in both
directions, and allows later checked graph transactions without inventing a second topology.
`TimelineGraphCompilation::with_graph` lets a higher persistence owner join deterministically
recompiled provenance to a checked decoded graph of the same stable identity while rejecting a
graph derived for another project or root. `TimelineGraphValue` implements strict Serde through an
internally tagged, unknown-field-denying wire that preserves typed identities, exact timing, clip
sources and maps, multicam state, track semantics, authored orders, and deterministic string maps.
This lets `CompiledTimelineGraphValue` pass through the public graph codec without moving timeline
semantics into graph or project.
`recompile_timeline_preserving_edits` accepts the old editorial merge base, its retained editable
compilation, and the next editorial state. It applies canonical additions, removals, parameters, and
edges to a clone of the retained graph, preserves unrelated direct edits and custom graph content,
and rejects ambiguous overlapping changes before returning a new checked compilation.

The OTIO interchange surface includes:

- `OtioDocument`, which owns the ordinary editable `EditorialProject`, root `TimelineId`, stable
  diagnostics, and opaque preservation state without exposing a second timeline model.
- `import_otio` and `export_otio` for offline JSON bytes, with exact native clock conversion and
  deterministic serialization.
- `OtioImportOptions` for explicit sample rate and channel layout policy on generic OTIO audio
  tracks.
- `OtioSchemaTarget::OtioCore0181` and its complete release schema map.
- `OtioDiagnostic`, `OtioDiagnosticSeverity`, and
  `timeline.otio.unsupported_construct` for source schema, identity, severity, and exact JSON
  pointer inspection.

`edit_ops`, `markers`, `multicam`, `nested`, `compile`, and `otio` are substantive public operation
surfaces.

`TimelineColorMetadata::from_graph` retains exact graph-owned color state and `graph` exposes it.
Timeline compilation keeps processing color requirements typed at the graph schema boundary but
does not evaluate or transform color.

## Architecture and data flow

Callers first construct complete track semantics. Video carries a frame rate and compositing mode.
Audio validates one sample rate, reuses the ordered core `ChannelLayout`, and requires one explicit
routing or mute decision per source channel. Caption and data semantics retain their exact clocks
and bounded type identifiers. `AudioSpan` preserves a linked clip identity and derives record and
source samples with checked exact conversion, so split and trim operations cannot silently drift.

Editorial construction and validation then proceed as follows:

1. Callers construct media references and timeline objects using canonical identities, exact
   `TimeRange` values, and `TrackSemantics` embedded in each track. Media references may retain a
   verified content fingerprint and deterministic metadata beside their replaceable locator.
2. `ClipRangeMap::new` requires nonempty source and record ranges with equal physical rational
   duration. Exact point and subrange mapping uses checked core arithmetic and never rounds.
3. `EditorialProject::new` indexes media and timelines, rejects duplicate identities, and validates
   the complete candidate graph before publishing it.
4. Validation walks every timeline and ordered track using that track's edit clock. It verifies
   local timing and object uniqueness, resolves clip sources, validates transitions against
   adjacent timed items, and follows nested timeline links to reject cycles.
5. `EditorialProject::clip_range_context` resolves media availability directly and derives nested
   availability from `[0, nested duration)` at the nested edit rate. Classification reports
   overscan without snapping or rejecting a media-linked clip.
6. Each clip begins with a separate identity `ClipTimeMap`. Direct replacement through either the
   clip method or `EditOperation::Retime` validates complete record coverage and source clock
   binding before mutation, rejects an unchanged map, and reports the unchanged clip record range
   as its exact affected range. Point queries convert absolute record time to clip-local time and
   resolve one immutable segment by binary search.
7. `ClipRangeContext::playback_sample` combines exact map resolution with known source availability
   without changing the nominal selected range. Inexact source samples require an explicit rounding
   policy and report that policy in the result.
8. A timeline compares track endpoints in physical rational time and exactly rescales the longest
   endpoint to its primary edit rate. This preserves synchronization across clocks such as frames,
   milliseconds, and audio samples without implicit rounding.
9. Read-only accessors expose the published project, while timeline, track, and object lookup keeps
   each relationship understandable by identity and order.

Direct edits use a copy-validate-publish transaction. `EditorialProject::edit` checks the expected
revision and clones current state into `ProjectDraft`. The closure mutates fields or inserts and
removes linked media and timelines, reorganizes bins, edits saved queries, or records relink
evidence. The entire candidate is revalidated and its revision advances only after the closure
succeeds; every failure discards the draft. Smart collection results are evaluated on demand from
the published media map and current relink state.
`superi-project` wraps these same media mutations in a whole-document revision fence. Its path and
relink commands retain existing editable timeline graphs, then regenerate only checked compilation
provenance around the new editorial revision because path availability is nonprocessing state.

Each `Timeline` constructs one edit state beside its ordered tracks. New tracks begin at 72 pixels,
untargeted, unlocked, sync locked, unmuted, unsoloed, and enabled. Linked selection begins enabled,
and selection and relationships begin empty. State mutations resolve only IDs present in that
timeline. Related selection walks clip
groups and, when linked selection is enabled, link components to a fixed point; direct selection
addresses the exact requested object even inside a group. Link and group components remain disjoint
within their own relationship class but may overlap each other. Grouping includes complete linked
components, and a new link extends or merges any group that already contains one of its members.

Target projection always follows the timeline's bottom-to-top track order. Sync resolution includes
every explicitly edited track regardless of its flag, then adds every other sync-locked track in
that same stable order. Before a project candidate validates, structural reconciliation retains
selection, track controls, links, and groups for every surviving identity, initializes controls for
new tracks, removes references to deleted objects, and dissolves relationship components with fewer
than two surviving clips. The reconciled state and all clip source and record data publish in the
same project revision or roll back together.

`apply_track_mutation_batch` executes an ordered nonempty batch inside one `EditorialProject::edit`
closure. Creation validates caller identity, canonical position, bounded height, and deterministic
kind semantics. Deletion requires an unlocked target and reconciles contained object identity,
annotations, links, groups, selections, and multicam state. Rename, height, reorder, targeting,
locks, sync locks, mute, solo, and enable preserve every unaffected track and item exactly. Mute and
solo accept active intent only for audio tracks, while neutral false values remain valid for legacy
and generic reconstruction.

`apply_marker_mutation_batch` follows the same single-draft boundary. Creation accepts one complete
marker and rejects any project-wide identity collision. Partial range, label, flag, and note edits
resolve an existing exact target and preserve its owner, metadata, and every unmentioned field.
Removal returns typed evidence for the deleted identity. Empty input, a missing target, or any late
invalid candidate rejects the complete batch without publishing an editorial revision.

Each `Timeline` also constructs annotation state in the same snapshot. Marker insertion validates
that the explicit owner exists and that authored timing uses the owner's exact record clock.
Timeline and track ranges are already record coordinates. Object ranges remain relative to the
stable timed object's record start, resolve through its current placement, and stay authored when
the object shifts. Object-relative ranges beyond the current owner duration remain stored as
editable overscan but do not become visible marker or snap ranges. Reconciliation removes only
markers and object metadata whose stable owner disappeared; timeline and surviving track state is
unchanged.

Snapping is a read-only query over the current validated snapshot. Candidate boundaries are exactly
rescaled into the request clock and skipped when no exact representation exists. The request clock
must match its tolerance clock. Persistent disablement returns no target, filters select target
classes, exclusions prevent a moving object and its markers from snapping to themselves, and equal
distances choose the smallest stable `SnapTarget` identity.

Editorial operation flow extends that transaction without creating another state model:

1. `apply_edit_batch` resolves each target timeline and track inside the unpublished
   `ProjectDraft`, preserving command order across related tracks.
2. Each operation validates that record points and ranges use the target track clock. Material
   duration is rescaled only when exact, and transitions are rejected as timed material.
3. A boundary inside a clip, gap, generator, or caption slices the object. Clip slicing adjusts the
   nominal source range and rebases every intersecting retime segment without changing its exact
   record-to-source behavior, while other object payloads remain unchanged. Caller-supplied typed
   IDs identify right fragments deterministically.
4. Insert shifts later items right, overwrite changes an equal-duration interval in place, append
   uses the exact current end, replace preserves target placement and duration, lift creates an
   explicit caller-owned gap, and extract shifts later items left.
5. Ripple changes one exact edge and shifts downstream material. Every other sync-locked track must
   have one explicit canonical adjustment, which inserts a caller-identified gap on extension or
   extracts the same physical interval on contraction with exact cross-clock conversion.
6. Roll moves one shared cut, slip changes only a clip source window, slide compensates both source
   neighbors around an unchanged center source, and plain trim creates or consumes explicit gaps.
   Extend delegates to the same ripple or roll implementation instead of defining new timing rules.
7. Razor retains the original identity on the left and uses one caller identity on the right.
   Clip fragments inherit direct selection plus link and group components before publication.
   Object annotations remain attached to the original stable identity and reconcile atomically.
8. Three-point placement derives exactly one missing source or record boundary. Four-point
   placement accepts only equal physical durations; fit-to-fill fails as unsupported until explicit
   P2.W02.C008 time remapping exists.
9. `set_transition` resolves one existing transition, requires both handles to use the track clock,
   rejects zero duration and no-op input, changes both offsets together, and reports the union of
   old and new visible ranges. Existing transitions are restored only when their original endpoints
   remain adjacent and their offsets still fit without overlap. Every invalidated transition is
   returned in the operation result rather than retargeted implicitly.
10. The existing whole-project validator resolves media and nested timeline sources, global object
   identity, track continuity, synchronization, and nesting cycles before one new revision is
   published. A failure in any command or final invariant discards every command in the batch.

Nested operation flow composes those same owners:

1. A request names the parent timeline and track, caller-owned compound clip identity, exact child
   source range, and one foundational placement mode.
2. Placement resolves the child and parent inside the unpublished draft, requires the source range
   to use the child edit rate, and converts its duration to the parent track clock only when exact.
3. Existing nested placement calls the shared single-operation executor. Prepared compound creation
   first adds one caller-authored child timeline after rejecting identity collisions, then places
   its clip in the same private draft.
4. Selection-derived compound creation validates complete selected relationship and transition
   boundaries, computes one exact parent span at the timeline edit rate, and converts every affected
   track span exactly into its local clock. It moves selected objects, object annotations, metadata,
   transitions, and relation state into caller-identified child tracks, preserves internal gaps,
   rebases record positions to child zero, and inserts caller-identified parent instances in
   canonical track order. Every parent instance links and groups together when more than one track
   participates.
5. The child timeline retains its own tracks, objects, selection, links, groups, targets, and sync
   locks. Direct child editing resolves the source through a stable parent clip, while final project
   validation checks every shared instance before publication.
6. Instance and recursive-tree inspection walk the validated project in deterministic timeline,
   track, and item order and return every typed source and record relationship without flattening.

Multicam flow composes ordinary timelines, clips, and edit transactions rather than defining a
parallel editorial model:

1. One source `Timeline` owns a `MulticamSource` with at least two ordered `MulticamAngle` values.
   Angle metadata and the authored synchronization method supplement the source clips' exact record
   placement without copying source timing into a second structure.
2. One target `Timeline` attaches `MulticamClip` state to an existing local clip whose source is
   `ClipSource::Timeline`. The switch program covers `[0, source timeline duration)` exactly in the
   source edit clock, while the target clip retains ordinary source range, record range, nesting,
   identity, and retime state.
3. Project validation requires globally unique angle identities, local source membership, no
   overlapping source clips within one angle, complete gapless switches, enabled referenced
   angles, a valid fixed-audio angle, and exact source clock and duration agreement.
4. Resolution maps absolute target record time through `Clip::source_time_at`, selects the active
   switch in synchronized timeline coordinates, finds the angle member covering that time, and
   maps through the selected source clip's `Clip::source_time_at`. No implicit rounding or flattened
   source relationship enters the path.
5. Fragment outcomes clone clip-local switch programs and insert source fragments beside their
   original angle member. Replace outcomes transfer the same state to the caller-supplied identity;
   reconciliation removes references to deleted local clips before the complete candidate validates.
6. Audio policy either follows the video angle, fixes one explicit angle, or exposes all enabled
   angles in source order for a later mixer. This crate stores intent and does not mix samples.

Timeline compilation consumes the validated result without changing authoring state:

1. `compile_timeline` verifies the selected root and walks every reachable nested timeline once in
   deterministic track and item order.
2. One timeline output node, one node per track, and one node per editorial object retain complete
   typed IDs, names, sources, ranges, time maps, semantics, authored order, transition intent,
   output enable, audio mute and solo, generator parameters, caption data, multicam source catalogs,
   synchronization provenance, switch partitions, and audio policy. Every native value enters
   `GraphValue::Domain` without conversion.
3. Typed video, audio, caption, and data ports connect items to tracks and tracks to timeline
   outputs. Transition endpoints and nested child outputs remain explicit graph edges.
4. Graph, node, port, parameter, and edge identities use length-framed domain-separated SHA-256
   derivation over stable editorial identities and semantic roles, truncated to the official
   128-bit graph ID domains. Mutable values and insertion history do not change those addresses.
5. All node additions and connections publish through one `GraphTransaction` at revision one.
   Graph validation rejects duplicate IDs, incompatible ports, invalid cardinality, or cycles
   before any compiled state is returned.
6. The bidirectional index resolves every reachable timeline, track, and object to its node and
   every compiled node back to its editorial owner. Height, selection, targeting, locks, sync
   locks, links, groups, annotations, and caches do not become hidden processing inputs.
7. A higher-tier catalog may add shared processing nodes and typed values through ordinary checked
   graph transactions. The timeline crate neither imports that catalog nor interprets its values,
   and existing native state remains editable under the same stable identities.
8. `superi-project::ProjectDocument` retains the complete compilation beside the matching
   editorial revision. Its immutable snapshots let engine resource acquisition consume the exact
   graph, including later checked graph transactions, without recompiling away editable state.
9. Compound project edits use the old canonical compilation as a merge base. A three-way recompile
   changes canonical graph content only where retained state still matches the base, preserves
   nonconflicting direct parameters and custom topology, and rejects conflicting overlap atomically.

OTIO interchange composes the same native owners:

1. Import validates a root `Timeline.1`, derives an exact timeline clock, and assigns deterministic
   typed project, timeline, track, item, marker, and media identities while retaining source IDs.
2. Root and nested `Stack.1` objects become native timelines. A nested stack also becomes a parent
   clip linked by `ClipSource::Timeline`, so ordinary nesting validation remains authoritative.
3. Track traversal derives contiguous record ranges from ordered OTIO children. Clips, gaps,
   transitions, media, marker ownership, metadata, and `LinearTimeWarp.1` state map into their
   existing native counterparts without another mutation path.
4. Every source object remains an opaque template keyed by native identity. Unsupported effects
   and timed schemas stay present, and warnings retain exact source pointer, schema, and identity.
5. Export traverses the current validated project order, patches supported names, ranges, links,
   handles, marker values, metadata, and linear retime scalars, then merges those values into the
   templates. Removed objects disappear and new objects receive canonical OTIO shapes and IDs.
6. `OtioSchemaTarget::OtioCore0181` makes the pinned target explicit. Runtime behavior uses only
   Rust and `serde_json`; the official OpenTimelineIO package is an external verification oracle.

Timeline document flow preserves those owners without becoming a project container:

1. Serialization projects the immutable `EditorialProject` into an explicit wire model and
   canonicalizes only identity-addressed sets and maps. Authored track, item, angle, source-clip,
   switch, predicate, and retime order remains intact.
2. Revision 2 records the `superi.timeline` format, component revision, stable core primitive
   revision, lowercase SHA-256 payload digest, and complete payload. Wide project revisions,
   playback ratios, and metadata integers use canonical decimal strings.
3. Loading first inspects the minimal header. It strictly decodes revision 2, revision 1, or the
   supported revision 0 envelope, rejects unknown fields and future revisions, and verifies the
   applicable digest before interpreting current payload meaning.
4. Reconstruction creates metadata keys, labels, notes, language tags, data schemas, audio routes,
   clips, time maps, markers, relationships, multicam sources, switch programs, media libraries,
   and the complete project through their checked constructors and mutation APIs. Duplicate set
   members and gapful switch partitions fail instead of being silently normalized.
5. The published project revision and exact relink evidence use narrow crate-private restoration
   seams, followed by whole-project validation. Canonical current bytes are regenerated from the
   validated project, so successful migration never returns stale or unchecked input bytes.
6. The API performs no file I/O and mutates no caller-owned project. `superi-project` stores accepted
   canonical bytes with independent length and SHA-256 evidence inside a complete current-schema
   candidate, then owns atomic save, save-as, copy, and backup publication. Project autosave reuses
   that backup authority, while recovery discovery and restoration remain later project policy.
7. Compiled timeline graph values project through a separate strict tagged wire in this module.
   The graph codec preserves those values and revisions, and project joins the decoded editable
   graph to freshly derived trusted provenance through `TimelineGraphCompilation::with_graph`.

## Dependencies and consumers

- `superi-core` supplies shared errors, exact rational and sample time, channel layouts, project and
  media identity, all typed editorial identities, and the stable `MulticamAngleId` used by
  production source.
- `superi-graph` supplies `GraphColorMetadata` to the narrow color propagation seam and owns the
  neutral `GraphValue` payload, schema, typed DAG, port validation, editable node, snapshot, and
  atomic mutation contracts used by timeline compilation. Timeline depends on no concrete effect
  catalog.
- `superi-effects` now supplies reusable cross-dissolve and directional-wipe schemas, animatable
  visual parameters, exact handle-to-progress conversion, and bounded reference pixels over the
  neutral graph. Timeline does not depend on that crate and remains authoritative for transition
  identity, adjacency, source and record timing, grouping, synchronization, persistence, and edit
  reconciliation. A higher integration owner may bind the existing neutral timeline projection to
  those schemas without moving editorial policy or reversing the dependency.
- The workspace-pinned `sha2` 0.10.9 implementation derives stable graph-facing identifiers from
  domain-separated, length-framed editorial identity inputs and protects canonical timeline payload
  meaning without adding a network path.
- `serde` and `serde_json` encode and strictly decode the stable component wire model and provide
  offline OTIO JSON parsing and serialization. No OTIO library, Python package, network path,
  plugin host, or fixture-tool runtime dependency enters the crate.
- `superi-project` and `superi-engine` consume `superi-timeline`. Project retains one validated
  `EditorialProject` plus complete matching `TimelineGraphCompilation` values in its revisioned
  immutable snapshots, stores canonical timeline bytes in schema 1, and uses timeline-owned strict
  graph-value Serde when storing retained graph documents. Project also interprets recognized media
  targets as typed filesystem paths without changing their timeline serialization meaning. Engine
  integration consumes the color metadata seam, preserves the legacy direct compiler path,
  traverses reachable media and nested timeline relationships, and clones the exact project-retained
  compilation with prepared source and decoder owners. Engine transport separately consumes the
  retime-owned reduced signed `PlaybackRate` without importing editorial mutation policy.
- Engine command history reverses complete project snapshots that contain this editorial and
  compiled graph state. `CompoundProjectAction::EditTimeline` applies an ordered nonempty batch of
  native `EditOperation` values inside the engine transaction boundary, while the public
  `ProjectAction::EditTimeline` and strict `TimelineEditOperation` wire translate into that owner.
  `CompoundProjectAction::MutateMarkers` applies an ordered nonempty batch of native marker
  mutations through the same compound transaction boundary. Timeline remains unaware of history
  and owns no stack; compound timeline, track, marker, graph, audio, and project actions share the
  engine-owned history boundary.
- Public integration tests and the `otio_roundtrip` example are real consumers. The engine's
  complete editor-state inspection and API projection now expose the canonical timeline document,
  and the production editing workspace strictly consumes that document as a read-only canvas while
  retaining exact identity, timing, grouping, targeting, synchronization, and selection evidence.
  Its supplemental clip presentation reuses that frozen projection for names, source state, exact
  time maps, markers, metadata, multicam intent, and separate canonical versus shared selection.
  The canvas now also projects the exact timeline, item, and owner-clock marker candidates into the
  edit clock for transient playhead and range gestures, preserves persistent disablement and stable
  target ordering, skips inexact points and valid object-marker overscan, and exposes session rules,
  visible consequence feedback, and reversal without claiming authored snapping ownership.
  The workspace timing compiler now turns ripple, roll, slip, slide, razor, trim, extend, ripple
  delete, and gap intent into those existing public operation batches with exact mixed-clock and
  typed-identity validation. It publishes one batch through the application-owned project executor;
  this crate and the engine remain authoritative for semantic validation, synchronization,
  relationship preservation, atomic publication, and history. The
  strict public marker DTO and durable application command path now expose all six marker gestures,
  while the workspace projects every authored marker and omits only inexact targets from snapping
  and navigation.

## Invariants and operational boundaries

- Project, media, bin, smart collection, timeline, track, clip, gap, transition, generator,
  caption, marker, and multicam angle identities are permanent typed domains. Track, editorial
  object, marker, and multicam angle identities are unique across one project.
- Manual bin parents must exist and remain acyclic. One linked media identity may belong to at most
  one manual bin, and every member must resolve in the same project. Smart collection membership is
  never persisted and always follows current metadata and relink state in stable identity order.
- A rejected relink retains the active locator and `MediaId`, plus the rejected locator and distinct
  expected and observed fingerprints. Unverified and missing states are explicit and never rewrite
  clip source relationships.
- Target text is persistent schema meaning but not media identity. This crate does not parse paths,
  consult the current directory, or derive `MediaId` from a locator; recognized filesystem syntax
  and project-file-relative resolution belong to `superi-project`.
- Marker ranges use their explicit owner's exact clock and never start before owner zero. Timeline
  and track markers use record coordinates. Object markers remain relative to a stable timed object,
  preserve overscan across trims, resolve through current record placement, and disappear only when
  their owner disappears.
- Metadata keys are canonical nonblank ASCII without whitespace and maps retain stable key order.
  Marker label, flag, and note semantics are explicit public fields rather than hidden key conventions.
- Snapping never rounds a candidate. The request coordinate and tolerance use one clock, inexact
  cross-clock candidates are skipped, exclusions are explicit, and stable target order breaks ties.
- Timeline edit state references only tracks and objects owned by that timeline. Surviving stable
  identities retain their selection, height, targeting, lock, synchronization, output, link, and
  group intent through structural project edits.
- Link components and group components are each disjoint canonical sets with at least two clips.
  They intentionally share core `ClipId` members rather than introducing relationship IDs. Groups
  include the complete linked components they contain, while unlinking does not silently ungroup.
- Related selection follows groups and the enabled link policy to a fixed point. Direct selection
  bypasses both relationships so one linked or grouped clip remains individually controllable.
- Track targeting is independent of sync lock. Target iteration follows timeline layer order, and
  an explicitly edited track participates in sync-sensitive work even when its own sync lock is off.
- Track heights are inclusive integers from 48 through 320 pixels. Active mute and solo intent is
  audio-only. Enable applies to every kind. A locked track must be unlocked before contained items
  change or the track is deleted; control mutations remain available so that unlock is explicit.
- Names, linked-media locators, caption text, generator kinds, and parameter keys must be nonblank.
- Every track owns one exact edit clock. Record ranges use that clock, remain nonnegative, and form
  contiguous content. Empty time is represented by an explicit gap.
- A timeline duration is the longest physical track endpoint exactly represented at the timeline's
  primary edit rate. Unrepresentable synchronization is rejected rather than rounded.
- Clips preserve physical duration between source and record ranges even when their timebases
  differ. Construction and direct replacement validate before mutation, so a clip cannot publish a
  desynchronized range map. Point and subrange mapping rejects out-of-range and inexact conversion.
- Clip time maps remain separate from nominal equal-duration ranges. Their nonempty segments use one
  record clock and one source clock, start at clip-local zero, provide gapless nonoverlapping full
  coverage, and meet at exactly representable continuous source seams.
- Playback rates are reduced signed rational values. Zero holds one source sample, negative values
  run in reverse, and non-unit positive values change speed without changing record duration.
- Transport queries select immutable segments by binary search and never round implicitly. Exact,
  held, and caller-selected rounded resolution remains inspectable beside known, unknown, or
  unavailable source state.
- Record repositioning preserves clip-local time maps. Exact clip splitting rebases intersecting
  segments, source-range replacement translates source anchors, and retimed duration replacement
  requires a new time map instead of discarding timing intent.
- Media source ranges may exceed an optional available range so overscan and relink intent are not
  destroyed. Availability remains inspectable as unknown, fully available, partially available, or
  unavailable.
- Nested clip source ranges use the target timeline's primary edit rate, stay within its duration,
  and may not form a direct or indirect cycle.
- Nested placement derives parent duration only through exact clock conversion and reuses the
  foundational edit outcome, fragment, transition, and rollback rules.
- Compound creation never replaces an existing timeline identity. The caller-authored child and
  parent clip publish together or neither publishes.
- Selection-derived compounds require complete objects and complete link, group, and transition
  boundaries. They preserve every selected object's stable identity, source and time map, rebase
  only record placement, retain internal empty time, and publish all child tracks and parent
  instances in deterministic order or roll back the whole project edit.
- A shared child edit validates every current instance before publication. Instance and recursive
  inspection preserve parent, track, clip, child, source-range, record-range, and depth identity.
- Multicam source state belongs to an ordinary timeline. Every angle retains stable identity,
  nonblank editor and camera labels, deterministic metadata, enabled state, and ordered local clip
  membership. One source clip belongs to at most one angle, and members of one angle never overlap.
- Multicam target state belongs to an ordinary local nested clip, not a competing clip type. Its
  switch intervals are nonempty, gapless, half-open, source-clocked, and cover the complete source
  timeline. Video and fixed-audio references select existing enabled angles.
- Multicam resolution follows both the target and selected source clip time maps with
  `TimeRounding::Exact`, preserving retime, nesting, and direct source identity. A synchronized gap
  returns a typed missing-source error instead of silently choosing another angle or clip.
- Structural fragments inherit source angle membership and target switch intent. Replacement
  transfers those relationships to the new caller-owned clip identity, while deleted identities
  reconcile before publication. Every change remains inside one revision-checked project draft.
- Compilation includes only the selected root and reachable nested timelines. Each reachable
  timeline, track, and editorial object appears exactly once, every object remains independently
  indexed, and nested child outputs feed every parent clip instance through exact typed ports.
- Mutable names, ranges, time maps, order, semantics, output enable, mute, solo, and parameters
  change typed graph payloads without changing stable graph addresses. Height, selection,
  targeting, authored-item locks, synchronization locks, links, groups, annotations, cache state,
  and revision history are not processing inputs.
- Compilation publishes one complete graph transaction or no graph. All identifiers are
  domain-separated, every hash part is length-framed, and any derived-ID collision fails closed.
- Timeline state revision 2 is strict and integrity-protected. Unknown fields, wrong formats,
  unsupported component or primitive revisions, malformed canonical integers, invalid typed IDs,
  oversized documents, and SHA-256 mismatches never produce a partial project.
- Timeline document canonicalization sorts identity-addressed media, timelines, bins, saved
  collections, markers, metadata keys and owners, edit sets, relationship components, and multicam
  target clips. It preserves authored track, item, predicate, angle, source membership, switch, and
  retime order.
- Revision 1 and revision 0 migration is in-memory and lossless. Loaded state must pass the same
  media, timeline, nesting, retime, annotation, edit-state, and multicam validation as newly
  authored state before canonical revision 2 bytes are exposed.
- The codec is a state component rather than a `.superi` file format. It performs no filesystem,
  database, journal, autosave, replacement, network, device, process, or GPU operation.
- A transition names the timed item immediately before and after it. Its offsets use the track edit
  clock, fit adjacent durations, do not overlap another transition on the same item, and do not add
  to track duration. Adjacent transitions are invalid. Effects-owned visual kinds and parameters do
  not replace or reinterpret this editorial owner contract.
- Transition-handle edits replace both offsets in one unpublished draft. The operation rejects an
  incorrect clock, zero total, unchanged values, overflow, insufficient adjacent duration, or
  opposite-edge overlap, and final project validation preserves atomic rollback.
- Audio routes cover every ordered source channel exactly once. Span construction, endpoints,
  splits, trims, and continuity distances use checked sample arithmetic and retain `ClipId` links.
- Language tags normalize case and validate bounded BCP 47 syntax without claiming IANA registry
  membership. Data schema labels are bounded ASCII without whitespace or control characters.
- Atomic drafts publish only after complete validation. Stale expected revisions are conflicts;
  every rejected edit preserves the prior snapshot and revision.
- Foundational edit points and ranges use the target track clock. Cross-rate material duration and
  clip source slices require exact rational conversion, never implicit rounding.
- Splitting an object keeps the original identity on the left and requires a same-domain caller ID
  for the right. Missing, wrong-domain, or unused fragment identities reject the complete batch.
- Insert and extract address only their named tracks. Advanced ripple refuses to leave another
  sync-locked track unchanged and requires one deterministic companion adjustment per affected
  track in stable timeline order.
- A track mutation batch is nonempty, revision-fenced, ordered, and atomic. Positions use canonical
  bottom-to-top order, caller-supplied identities never alias existing tracks, and a late invalid
  control or deletion rolls the complete batch back.
- A marker mutation batch is nonempty, revision-fenced, ordered, and atomic. Create carries complete
  owner, range, visible fields, and metadata, partial edits preserve unmentioned state, identities
  remain project-wide unique, and a missing target or late invalid mutation rolls the batch back.
- Overwrite and replace preserve track duration. Lift makes empty time explicit with a named gap.
  Append and insert report exact extension, while extract reports exact shortening.
- A transition is never silently redirected. It survives only with its original adjacent endpoints
  and valid nonoverlapping handles; otherwise its typed identity appears in the outcome.
- OTIO import rejects duplicate source identity, malformed required structure, and coordinates that
  cannot be represented exactly on the target native clock. It never silently rounds timing.
- Supported OTIO objects are rebuilt from native state while complete source templates preserve
  unknown fields. Unsupported effects remain opaque and produce stable warning code, severity,
  schema, identity, and JSON pointer values.
- Generic OTIO audio lacks complete native semantics, so sample rate and channel layout are explicit
  import options and routing is deterministic. Original audio metadata remains opaque and preserved.
- Compiled editorial parameters remain exact `GraphValue::Domain(TimelineGraphValue)` variants.
  Shared processing variants may coexist but cannot rewrite, coerce, or replace native domain state,
  and their presence does not change stable editorial-to-graph identities.
- The `TimelineGraphValue` wire is internally tagged and denies unknown fields and tags. Decode
  reuses checked timeline constructors for time maps, multicam state, track semantics, and other
  structured variants before a graph can be published.
- Fit-to-fill, arbitrary vendor-effect interpretation, graph evaluation, direct history ownership,
  multicam playback and mixing, and higher-level editorial commands remain outside this state.
  Engine adapts both item edits and track batches to whole-project snapshot history.
- The timeline color seam preserves exact graph metadata and performs no transform, inference,
  normalization, or reordering.

## Tests and verification

Four editorial model tests cover all foundational objects, linked media, nested timelines, lookup,
direct clip and caption edits, rollback, revision conflicts, missing links, discontinuity,
transition bounds, and cycles. Their project combines 48 fps source time, 24 fps timeline and video
time, and millisecond caption time while preserving one exact 3.5 second duration.

Five edit-state tests exercise the public surface through real `EditorialProject::edit`
transactions. They prove linked-selection enable and disable behavior, transitive grouping, exact
member selection, replace, add, and remove selection intent, link and group removal, track target
order, sync-lock inclusion, structural reconciliation, unchanged clip source and record ranges,
missing-ID rollback, and stale-revision rollback.

Six track-semantics tests prove four distinct kinds, direct routing replacement, ordered channel
coverage, explicit mute behavior, exact record-to-source samples, linked split and trim behavior,
fractional-boundary rejection, gap and overlap reports, source continuity, bounded caption and data
validation, and checked extreme seam distances.

Three OTIO tests prove the first editorial fixture, comprehensive coverage fixture, two rate
changes, stable unsupported-object pointers, opaque preservation, and the warning code
`timeline.otio.unsupported_construct`. Official OpenTimelineIO 0.18.1 separately loads both files
and reports exact 48-frame and 120-frame durations at 24 fps.

Nine production interchange tests prove native root and nested structure, exact duration and
record placement, media targets, marker ownership, transition adjacency, 2x and 0.5x playback,
the two required warnings, direct name and retime edits, deterministic bytes, Rust reimport,
preserved unsupported fields, explicit 48 kHz stereo audio policy, duplicate identity rejection,
exact-clock failure pointers, conflicting repeated media rejection, and native hierarchy reshape
with exact reimported duration. The public example emits both fixtures; OpenTimelineIO 0.18.1
under Python 3.12 loads them, target-writes them with its release map, rereads them, and reports
equivalent 48-frame and 120-frame timelines.

Six foundational edit-operation tests prove insert, overwrite, append, replace, lift, extract, and
atomic transition-handle replacement
through the public API. They cover source and record clocks at 48 and 24 units per second, arbitrary
boundaries within clips and gaps, nested timeline material, exact right-fragment identities,
transition invalidation, exact old and new affected-range union, unchanged and changed duration
reports, synchronized two-track batches, stale revisions, wrong clocks, zero and unchanged handles,
overlong handles, transition material, overwrite bounds, and complete rollback after a later
command fails.

Eight nested-operation tests prove existing child placement, prepared and selection-derived atomic
compound creation, preservation of child objects and selection, links, groups, targeting,
sync-lock state, annotations, metadata, transitions, exact mixed-clock span rebasing, internal gaps,
stable source and time-map identity, shared-instance reporting, direct child edits, recursive depth,
caller-owned fragments, incomplete relation and transition boundary rejection, missing and
duplicate identity rejection, stale revisions, source shrink rollback, inexact clock rejection,
and cycle rollback.

Four range tests prove exact cross-clock point and subrange translation, half-open and inexact
failure paths, atomic direct replacement, all four availability classifications, editable media
overscan, and nested source resolution.

Eight marker tests prove all three owner classes, project-wide marker identity uniqueness, stable
marker iteration, label, flag, note, nested
metadata, all six atomic marker gestures, owner and metadata preservation, duplicate and missing
target rejection, empty rejection, late rollback, direct mutation, object-relative placement, overscan suppression, exact cross-clock snap
projection, target filters, object and marker exclusions, playhead candidates, stable ties,
persistent disablement, atomic invalid-clock rollback, survival through a real insert, and selective
cleanup through a real extract.

Nine advanced edit-operation tests prove inward and outward ripple and trim behavior, roll, slip,
slide, clip and non-clip razor splits, extend delegation, all four three-point forms, exact
four-point placement, explicit retime rejection, cross-rate derivation, transition invalidation,
sync-locked two-track contraction, selection and relationship inheritance, stable track intent,
object annotation retention, role-neutral deterministic results, and complete failed-batch rollback.

Six retime tests prove exact 2x speed, rational slow motion, reverse, freeze, custom piecewise maps,
continuous seams, complete coverage, half-open bounds, explicit rounding, point availability,
atomic clip binding, identity resize compatibility, link retention, and retime-preserving split and
record-shift behavior through a real insert operation.

Three retime edit-operation tests prove that speed, reverse, freeze, and continuous multi-segment
maps all use one revision-checked edit path with exact modified-object and affected-range evidence.
They also prove stable clip identity and record duration plus complete rollback for semantic no-ops,
locked tracks, missing clip identities, and wrong track bindings.

Four media-library tests prove stable bin and sub-bin paths, direct media movement, deterministic
metadata smart collections, atomic cycle, duplicate membership, and missing-media rejection,
explicit missing and unverified state, content-mismatch evidence, accepted relinks, preserved
stable identity, and unchanged exact retime, link, group, synchronization, and nested sequence
state through a subsequent real edit batch.

Four multicam tests prove ordered angle identity, labels and deterministic metadata, timecode and
audio synchronization provenance, exact resolution through a 2x target time map and a cross-rate
source clip, range switching, movable cuts, fixed and all-angle audio intent, target and source
fragment inheritance, target and source replacement inheritance, missing coverage and disabled
angle rejection, stale revisions, and complete atomic rollback.

Eleven compilation tests prove identical graph snapshots from identical project state, one atomic
revision with twelve typed nodes and fourteen checked edges, two instances sharing one nested
timeline output, explicit transition routing, retained bottom-to-top track order, caption parameter
inspection, bidirectional provenance, stable graph and node addresses across a source-range edit,
identical graph state after a selection-only project revision, directly editable graph parameters,
one linked shared scalar processing node beside native domain values, typed multicam source,
  switching, audio intent, typed track output state distinct from authoring-only controls,
  preservation of nonconflicting direct parameters plus custom nodes
  and edges across editorial recompilation, overlapping parameter and connection-removal conflict
  rejection, and classified missing-root failure.

The engine media resource contract is the first production crate consumer of compilation. It
retains the canonical fixture's three-node, two-edge graph beside a fingerprint-verified WebM source
and live AV1 decoder, resolves the persistent project target through the project path codec, and
proves exact timing, precision, metadata, color, alpha, missing-state, and identity semantics without
copying timeline graph or media identity into another owner.

Four serialization tests prove deterministic complete project equality, revision 2 envelope and
primitive revision identity, SHA-256 integrity, revision 1 and revision 0 migration, canonical
current output, truncated and tampered recovery rejection, strict unknown and future state
handling, invalid link rejection, exact multicam resolution after load, continued revision-checked
editing, and a complete compiled multicam graph round trip through the public graph codec with
unknown graph-value fields and tags rejected.

Five track-management tests prove all eleven gestures, explicit canonical creation semantics for
video, audio, caption, and data tracks, exact survivor and relationship state, one revision per
batch, locked deletion rollback, explicit unlock and deletion, bounded height, audio-only mute and
solo, and locked authored-item rejection.

Workspace tests, warnings-denied Clippy, formatting, dependency direction, the offline boundary
scan, and codebase-map validation are required delivery gates.

## Current status and risks

The foundational project model, rational range mapping, linked availability context, manual media
organization, saved smart collections, explicit relink state, typed track semantics, atomic track
management and output intent, authoritative timeline edit state, atomic marker management, deterministic metadata,
exact snapping,
exact clip retiming, six primary operations, nine advanced edit families, nested placement,
prepared and selection-derived multi-track compound creation, shared child editing, recursive
inspection, and native multicam angle,
synchronization, switching, audio-intent, structural inheritance, and exact resolution are
substantive and test-backed. Deterministic graph compilation with lossless native domain values and
shared processing-value coexistence, plus production OTIO 0.18.1 reading,
writing, opaque preservation, stable diagnostics, and a headless consumer are also test-backed.
Strict canonical timeline documents, revision 1 and revision 0 migration, checked recovery,
continued editing
after load, and strict compiled graph-value round trips are test-backed. `superi-project` now stores
canonical timeline and compiled graph components in stable SQLite schema 3 and atomically publishes
complete save, save-as, copy, and backup files. Effects has compatible
graph-native transition authoring and a bounded oracle, but the production binder from this
timeline-owned state to those visual schemas is absent. Graph evaluation, fit-to-fill,
multicam mixing and runtime playback, timeline-driven autosave
scheduling, and recovery orchestration remain absent. Generic editor-state inspection and the
public API preserve the canonical timeline document and expose typed track mutation through durable
project commands, including strict marker mutation and evidence. The production editing workspace renders the strict projection with transient
navigation and shared-selection state, exact target snapping, visible session rule and consequence
state, reversible pointer gestures, supplemental clip detail that does not reparse geometry, and
exact advanced timing plans that enter the existing public operation wire as one atomic batch.
Track and marker gestures return through the durable project command owner. Engine
preparation integration now consumes and retains the compiled graph, and engine transport consumes
the standalone signed rate value, but no
owner yet binds that prepared native timeline graph to decoded playback, multicam mixing, or render
output.

Engine now owns bounded project-level undo and redo and a typed compound transaction that applies
timeline edit batches through the checked three-way graph reconciliation seam. It can reverse the
complete retained timeline and graph state through project snapshot restoration without adding a
timeline-local undo owner.

The model retains equal physical source and record duration for nominal clip ranges, while separate
time maps may sample beyond that selection and report known unavailable points. Exact seam and slice
requirements reject a custom remap whose discrete source boundaries cannot be represented. Reverse
construction requires the explicit first sampled source coordinate, avoiding a hidden end-minus-one
assumption across arbitrary clocks. Clip range and time-map setters validate before mutation, while
callers use `EditorialProject::edit` or `apply_edit_batch` for atomic publication of broader project
changes. Selection is authoritative command intent, not hover, focus, marquee geometry, or
optimistic UI presentation state. Advanced ripple and ripple-mode extend consume sync resolution
with explicit per-track identity material; the resolver alone remains a pure projection. Links and
groups are timeline-local and have no independent durable ID. Edit material is currently one timed
object per command; multi-object source sequences and link-group targeting belong to later command
and orchestration layers. Audio continuity is structural evidence rather than signal analysis or
playback. The model now has a strict stable component schema, a 64 MiB document bound, checked
collection and relationship reconstruction, while OTIO collection sizes are not yet bounded
independently of process memory. Nested component collection counts are validated by domain
construction after JSON allocation rather than by a streaming preallocation quota. The
`otio_roundtrip` example is the first production interchange consumer outside contract tests. The
engine color propagation contract consumes the narrow metadata seam. The project document now
retains real generic editable graph state that can admit a higher-tier effects catalog, interprets
recognized referenced-media paths without duplicating media state, and engine resource preparation
consumes its exact selected compilation and resolved local target. The API and editing workspace
inspect canonical editorial state and the workspace may label real clip-scoped graph nodes, but no
API, CLI, playback, application, or production render owner evaluates the compiled graph result
yet.

## Maintenance notes

Treat track clocks, semantics, height, target, lock, sync lock, mute, solo, enable, mutation order,
object identity, media identity, bin hierarchy, smart query
derivation, relink evidence, continuity, physical-time equality, source-aware
and retime-aware fragmentation, exact authored time-map replacement, semantic no-op rejection,
exact time-map seams, explicit transport resolution, explicit
transition invalidation, result reporting, marker ownership, complete create semantics, partial-field
preservation, atomic marker mutation order, exact snapping, metadata ordering, atomic dual-handle
replacement,
source-link resolution, selection expansion, track intent, clip relationship partitions,
reconciliation, transition adjacency, nesting acyclicity, exact nested placement, shared-instance
reporting, multicam angle identity and metadata, source membership, switch coverage, exact
resolution, audio intent, structural inheritance, and atomic publication as public contracts.
Keep linked media targets opaque here, preserve every relink evidence field in canonical state, and
route filesystem interpretation through `superi-project` without moving or duplicating `MediaId`.
Treat the timeline document format, primitive revision gate, field names, enum codes, decimal
integer forms, canonical collection rules, checksum scope, migration behavior, and checked
reconstruction as the same class of public contract. Add a new component revision and explicit
migration for incompatible changes instead of changing revision 2 in place.
Extend tests before changing them. Later
higher-level compound operations must consume `tracks_affected_by_sync`, exact selection state,
and clip-owned time maps instead of recreating those policies. Add higher-level
edit commands and graph evaluation only through their owning modules, and update project, engine,
API, CLI, persistence, and fixture maps when those paths begin consuming native timeline state.
Keep the production workspace projection strict against the canonical document revision and retain
exact ranges, stable identities, relationships, targeting, synchronization, and selection. Local
playhead, range, scroll, and zoom may remain presentation intent, but authored gestures must enter
through project and engine command ownership rather than reconstructing timeline policy in React.
Frontend timing planners may translate direct user coordinates into this module's public operation
forms only when they preserve exact clocks, typed identity domains, synchronized-track order, lock
admission, and one atomic lower-owned publication. Native validation and history remain mandatory;
the preview cannot be treated as canonical state.
The UI may project this module's published exact snap candidates for transient gesture feedback only
when it preserves target classes, persistent disablement, exact cross-clock omission, object-relative
marker resolution, and stable tie order. Session filters, guides, and captured gesture origins must
remain presentation state, and later authored clip moves must enter the native snap and edit query
path rather than treating the UI projection as a mutation authority.
Supplemental clip presentation must reuse that exact projection, keep source and record evidence
unchanged, and preserve canonical timeline selection separately from application selection.
Keep timeline edits in project command history behind the engine compound command owner and preserve
full project snapshot restoration. Preserve the old-canonical, retained, and next-canonical
three-way merge roles, including conflict rejection, instead of recompiling away direct graph edits.
Do not add a timeline-local history stack or bypass the compound transaction boundary.
Keep every compiled editorial value wrapped in `GraphValue::Domain`, preserve
`CompiledTimelineGraphValue` as the shared public payload, and let higher-tier catalogs add only
their own processing nodes and schemas. Timeline must not import effects or translate catalog
choices into a competing native representation.
Keep `PlaybackRate` reduced, signed, and policy-neutral. Engine transport may consume its exact
ratio, but loop, drop, clock, audio, and command behavior must remain above timeline rather than
entering clip retime state.
Extend OTIO only through `OtioDocument` and the pinned schema target, preserve unknown source
templates and stable diagnostics, and prove emitted files through an official compatible reader
before expanding the supported subset. Keep file I/O, SQLite schema, autosave, replacement, and
recovery journals in `superi-project`. Schema 2 must continue consuming
`TimelineStateLoad::canonical_document` only after project-level acceptance, without duplicating or
weakening timeline component meaning.
