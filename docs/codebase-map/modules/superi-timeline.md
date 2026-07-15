---
module_id: superi-timeline
source_paths:
  - open/crates/superi-timeline
source_hash: 01533bbc374dda81d25d3a6999f964234eab4fdefd83ff7de8ef47277fe4f6f4
source_files: 18
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-timeline` owns the foundational Rust-native editorial project model and typed track
semantics. It represents linked media, timelines, ordered tracks, clips, explicit gaps,
transitions, generators, captions, and nested timeline sources with core-owned identities and
exact rational timing. It also owns authoritative timeline selection, track targeting, sync locks,
linked selection, and clip grouping. Video, audio, caption, and timed-data tracks carry their
explicit clock and media behavior. Clip range maps keep source and record clocks synchronized,
while resolved range contexts expose known media availability or derived nested-timeline
availability without destroying overscan. Timeline, track, and object markers preserve permanent
identity, explicit ownership, owner-relative exact ranges, visible labels, flags, notes, and nested
deterministic metadata. Persistent snapping resolves exact timeline, playhead, item, and visible
marker boundaries with stable filters, exclusions, and tie ordering. Foundational insert,
overwrite, append, replace, lift, and extract commands reshape those objects while reporting every
inserted, removed, modified, split, or invalidated relationship. Whole-project validation and
revision-checked atomic batches keep linked objects, annotations, user intent, timing,
synchronization, nesting, and direct edits valid at publication boundaries.

The model also owns a narrow immutable color metadata seam that retains graph color state through
the future compilation boundary without changing source meaning.

The crate continues to reserve advanced trim operations, multicam behavior, OTIO-compatible
interchange, and deterministic timeline-to-graph compilation. Those surfaces are not implemented.
The canonical OTIO 0.18.1 fixture remains executable evidence for future interchange work rather
than a production reader or writer.

## Source inventory

- `open/crates/superi-timeline/Cargo.toml`: Declares runtime dependencies on `superi-core` and
  `superi-graph`, plus development-only `serde_json` for canonical OTIO fixture contracts.
- `open/crates/superi-timeline/src/compile.rs`: Placeholder for timeline-to-graph compilation.
- `open/crates/superi-timeline/src/edit_ops.rs`: Implements directly inspectable insert, overwrite,
  append, replace, lift, and extract commands, exact source-aware splitting, deterministic fragment
  identities, transition reconciliation, result reports, and atomic multi-track batches.
- `open/crates/superi-timeline/src/edit_state.rs`: Implements exact and relationship-expanded
  selection, per-track targeting and sync-lock intent, canonical clip links and groups, stable
  introspection, and structural reconciliation.
- `open/crates/superi-timeline/src/ids.rs`: Re-exports the canonical project and editorial object
  identities owned by `superi-core`.
- `open/crates/superi-timeline/src/lib.rs`: Exports the implemented identity, edit-state, edit
  operation, and model modules plus the staged editorial namespaces.
- `open/crates/superi-timeline/src/markers.rs`: Implements stable timeline, track, and object marker
  ownership, visible labels, flags, notes, recursively nested ordered metadata, owner-relative range
  resolution, dangling-owner reconciliation, persistent snapping state, exact candidate projection,
  target filters, exclusions, and deterministic tie resolution.
- `open/crates/superi-timeline/src/model.rs`: Implements four track kinds, track-specific timing and
  media semantics, exact clip range maps, linked availability context, every foundational
  editorial object, ordered tracks, timelines, annotation integration, validated project snapshots,
  atomic revision-checked editing, and `TimelineColorMetadata`, which retains exact graph color
  metadata through compilation.
- `open/crates/superi-timeline/src/multicam.rs`: Placeholder for a multicam data model.
- `open/crates/superi-timeline/src/nested.rs`: Placeholder for higher-level compound clip and
  nested sequence operations. The foundational model already supports clips sourced from another
  timeline and rejects nesting cycles.
- `open/crates/superi-timeline/src/otio.rs`: Reserves the ratified OTIO-compatible serialization
  boundary and points to the shared 0.18.1 fixtures. The production reader and writer remain
  staged.
- `open/crates/superi-timeline/tests/edit_state_contract.rs`: Proves linked and grouped selection,
  direct member control, target and sync-lock ordering, link and group independence, state
  reconciliation, identity and timing retention, revision conflicts, and atomic rollback.
- `open/crates/superi-timeline/tests/model_contract.rs`: Proves every foundational object,
  cross-rate and cross-track synchronization, linked media and nesting, direct edits, revision
  conflicts, atomic rollback, transition bounds, continuity, missing links, and nesting cycles.
- `open/crates/superi-timeline/tests/edit_ops_contract.rs`: Proves all six foundational operations,
  exact cross-rate source slicing, nested source preservation, typed fragment identities, explicit
  transition removal, lift gaps, synchronized multi-track publication, and failed-batch rollback.
- `open/crates/superi-timeline/tests/markers_contract.rs`: Proves stable marker identity, timeline,
  track, and object ownership, visible semantics, nested metadata, direct mutation, owner-relative
  resolution, preserved overscan, structural-edit survival, dangling-owner cleanup, exact snapping,
  filters, exclusions, persistent disablement, stable ties, and atomic rollback.
- `open/crates/superi-timeline/tests/otio_fixture_contract.rs`: Proves canonical OTIO schema,
  hierarchy, identity, timing, relationships, opaque retention, and unsupported diagnostics.
- `open/crates/superi-timeline/tests/range_contract.rs`: Proves exact cross-clock point and subrange
  mapping, fallible atomic range replacement, media overscan classification, unknown availability,
  and derived nested-timeline availability.
- `open/crates/superi-timeline/tests/track_semantics_contract.rs`: Proves all four track kinds,
  exact clocks, channel routing, linked audio reshaping, continuity, and bounded validation.

## Public surface

The `ids` module re-exports `ProjectId`, `MediaId`, `TimelineId`, `TrackId`, `ClipId`, `GapId`,
`TransitionId`, `GeneratorId`, `CaptionId`, and `MarkerId`. These are the same sealed core identifier
types used by every other subsystem.

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

- `LinkedMediaReference`, including stable media identity, display name, target locator, and an
  optional available source range.
- `ClipSource`, which links a clip to either media or another timeline.
- `ClipRangeMap` for nonempty equal-duration source and record ranges plus checked exact point and
  subrange translation in both directions.
- `ClipRangeContext` and `RangeAvailability` for resolving a clip's typed source, synchronized
  ranges, optional availability, and unknown, full, partial, or unavailable status.
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
- `TrackEditState` for one stable track's targeted and sync-locked flags.
- `ClipRelation`, a deterministic set of stable `ClipId` members directly addressable through any
  member, without inventing a second clip or group identity domain.
- `TimelineEditState` for selected objects, track controls, the linked-selection toggle, clip link
  components, and clip groups.
- `Timeline` operations to select objects, link and unlink clips, group and ungroup clips, set track
  intent, enumerate targeted tracks by timeline order and media kind, and resolve sync-affected
  tracks for later insert and ripple commands.

The annotation and snapping surface includes:

- `MarkerOwner` for explicit timeline, track, or stable editorial-object ownership, plus `Marker`
  with core-owned `MarkerId`, an exact owner-relative `TimeRange`, and directly replaceable
  `MarkerLabel`, `MarkerFlag`, `MarkerNote`, and marker metadata.
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

The foundational operation surface includes:

- `EditOperation` and `EditKind` for insert, overwrite, append, replace, lift, and extract commands
  targeted by stable timeline and track identity.
- Caller-supplied typed right-fragment identities whenever one existing timed object must survive on
  both sides of an edit boundary. The left fragment retains the original object identity.
- `apply_edit_batch`, which applies one or more commands through one existing
  `EditorialProject::edit` publication and returns `EditBatchResult` at the new project revision.
- `EditOutcome`, `EditFragment`, and `TrackDurationChange`, which expose affected ranges,
  inserted and removed objects, changed retained objects, created right fragments, removed
  transitions, and exact duration effects without reconstructing a diff from the final track.

`compile`, `multicam`, `nested`, and `otio` remain public namespace reservations without production
operations. `markers` and `edit_ops` are substantive public operation surfaces.

`TimelineColorMetadata::from_graph` retains exact graph-owned color state, `graph` exposes it, and
`compile` returns an unchanged clone for a later graph compiler.

## Architecture and data flow

Callers first construct complete track semantics. Video carries a frame rate and compositing mode.
Audio validates one sample rate, reuses the ordered core `ChannelLayout`, and requires one explicit
routing or mute decision per source channel. Caption and data semantics retain their exact clocks
and bounded type identifiers. `AudioSpan` preserves a linked clip identity and derives record and
source samples with checked exact conversion, so split and trim operations cannot silently drift.

Editorial construction and validation then proceed as follows:

1. Callers construct media references and timeline objects using canonical identities, exact
   `TimeRange` values, and `TrackSemantics` embedded in each track.
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
6. A timeline compares track endpoints in physical rational time and exactly rescales the longest
   endpoint to its primary edit rate. This preserves synchronization across clocks such as frames,
   milliseconds, and audio samples without implicit rounding.
7. Read-only accessors expose the published project, while timeline, track, and object lookup keeps
   each relationship understandable by identity and order.

Direct edits use a copy-validate-publish transaction. `EditorialProject::edit` checks the expected
revision and clones current state into `ProjectDraft`. The closure mutates fields or inserts and
removes linked media and timelines. The entire candidate is revalidated and its revision advances
only after the closure succeeds; every failure discards the draft.

Each `Timeline` constructs one edit state beside its ordered tracks. New tracks begin untargeted
with sync lock enabled, linked selection begins enabled, and selection and relationships begin
empty. State mutations resolve only IDs present in that timeline. Related selection walks clip
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

Foundational operation flow extends that transaction without creating another state model:

1. `apply_edit_batch` resolves each target timeline and track inside the unpublished
   `ProjectDraft`, preserving command order across related tracks.
2. Each operation validates that record points and ranges use the target track clock. Material
   duration is rescaled only when exact, and transitions are rejected as timed material.
3. A boundary inside a clip, gap, generator, or caption slices the object. Clip slicing adjusts the
   source start and duration with exact rational conversion, while other object payloads remain
   unchanged. Caller-supplied typed IDs identify right fragments deterministically.
4. Insert shifts later items right, overwrite changes an equal-duration interval in place, append
   uses the exact current end, replace preserves target placement and duration, lift creates an
   explicit caller-owned gap, and extract shifts later items left.
5. Existing transitions are restored only when their original endpoints remain adjacent and their
   offsets still fit without overlap. Every invalidated transition is returned in the operation
   result rather than retargeted implicitly.
6. The existing whole-project validator resolves media and nested timeline sources, global object
   identity, track continuity, synchronization, and nesting cycles before one new revision is
   published. A failure in any command or final invariant discards every command in the batch.

The separate fixture path reads checked-in OTIO JSON through development-only `serde_json::Value`
assertions. It does not enter the native model yet.

## Dependencies and consumers

- `superi-core` supplies shared errors, exact rational and sample time, channel layouts, project and
  media identity, and all typed editorial identities used by production source.
- `superi-graph` supplies `GraphColorMetadata` to the narrow color propagation seam and remains the
  future compilation target for the substantive editorial model.
- `serde_json` is development-only and reads checked-in canonical JSON. No OTIO library, Python
  package, network path, or fixture-tool runtime dependency enters the crate.
- `superi-project` and `superi-engine` declare `superi-timeline` as a dependency. Engine integration
  tests consume the color metadata seam; neither source tree consumes the editorial model yet.
- Public integration tests are the current real consumers. No API or CLI surface exposes the
  general editorial model.

## Invariants and operational boundaries

- Project, media, timeline, track, clip, gap, transition, generator, caption, and marker identities
  are permanent typed domains. Track, editorial object, and marker identities are unique across one
  project.
- Marker ranges use their explicit owner's exact clock and never start before owner zero. Timeline
  and track markers use record coordinates. Object markers remain relative to a stable timed object,
  preserve overscan across trims, resolve through current record placement, and disappear only when
  their owner disappears.
- Metadata keys are canonical nonblank ASCII without whitespace and maps retain stable key order.
  Marker label, flag, and note semantics are explicit public fields rather than hidden key conventions.
- Snapping never rounds a candidate. The request coordinate and tolerance use one clock, inexact
  cross-clock candidates are skipped, exclusions are explicit, and stable target order breaks ties.
- Timeline edit state references only tracks and objects owned by that timeline. Surviving stable
  identities retain their selection, targeting, synchronization, link, and group intent through
  structural project edits.
- Link components and group components are each disjoint canonical sets with at least two clips.
  They intentionally share core `ClipId` members rather than introducing relationship IDs. Groups
  include the complete linked components they contain, while unlinking does not silently ungroup.
- Related selection follows groups and the enabled link policy to a fixed point. Direct selection
  bypasses both relationships so one linked or grouped clip remains individually controllable.
- Track targeting is independent of sync lock. Target iteration follows timeline layer order, and
  an explicitly edited track participates in sync-sensitive work even when its own sync lock is off.
- Names, linked-media locators, caption text, generator kinds, and parameter keys must be nonblank.
- Every track owns one exact edit clock. Record ranges use that clock, remain nonnegative, and form
  contiguous content. Empty time is represented by an explicit gap.
- A timeline duration is the longest physical track endpoint exactly represented at the timeline's
  primary edit rate. Unrepresentable synchronization is rejected rather than rounded.
- Clips preserve physical duration between source and record ranges even when their timebases
  differ. Construction and direct replacement validate before mutation, so a clip cannot publish a
  desynchronized range map. Point and subrange mapping rejects out-of-range and inexact conversion.
- Media source ranges may exceed an optional available range so overscan and relink intent are not
  destroyed. Availability remains inspectable as unknown, fully available, partially available, or
  unavailable.
- Nested clip source ranges use the target timeline's primary edit rate, stay within its duration,
  and may not form a direct or indirect cycle.
- A transition names the timed item immediately before and after it. Its offsets use the track edit
  clock, fit adjacent durations, do not overlap another transition on the same item, and do not add
  to track duration. Adjacent transitions are invalid.
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
- Insert and extract ripple only the addressed track per command. Related track changes remain
  synchronized when callers submit them in one atomic batch at one expected project revision.
- Overwrite and replace preserve track duration. Lift makes empty time explicit with a named gap.
  Append and insert report exact extension, while extract reports exact shortening.
- A transition is never silently redirected. It survives only with its original adjacent endpoints
  and valid nonoverlapping handles; otherwise its typed identity appears in the outcome.
- Advanced retiming, ripple and roll trims, slip, slide, razor, three-point and four-point edits,
  production OTIO preservation, deterministic graph compilation, undo-history ownership,
  multicam, and higher-level editorial commands remain outside this state.
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

Five foundational edit-operation tests prove insert, overwrite, append, replace, lift, and extract
through the public API. They cover source and record clocks at 48 and 24 units per second, arbitrary
boundaries within clips and gaps, nested timeline material, exact right-fragment identities,
transition invalidation, unchanged and changed duration reports, synchronized two-track batches,
stale revisions, wrong clocks, transition material, overwrite bounds, and complete rollback after a
later command fails.

Four range tests prove exact cross-clock point and subrange translation, half-open and inexact
failure paths, atomic direct replacement, all four availability classifications, editable media
overscan, and nested source resolution.

Six marker tests prove all three owner classes, project-wide marker identity uniqueness, stable
marker iteration, label, flag, note, nested
metadata, direct mutation, object-relative placement, overscan suppression, exact cross-clock snap
projection, target filters, object and marker exclusions, playhead candidates, stable ties,
persistent disablement, atomic invalid-clock rollback, survival through a real insert, and selective
cleanup through a real extract.

Workspace tests, warnings-denied Clippy, formatting, dependency direction, the offline boundary
scan, and codebase-map validation are required delivery gates.

## Current status and risks

The foundational project model, rational range mapping, linked availability context, typed track
semantics, authoritative timeline edit state, markers, deterministic metadata, exact snapping, and
six primary editorial operations are substantive and test-backed. Production OTIO reading and
writing, graph compilation, advanced trim transforms, undo ownership, multicam, persistence, and
engine or API integration remain absent.

The model requires equal physical source and record duration for clips. Future time-warp support
must introduce explicit retime state rather than weakening that invariant. Clip range setters
validate before mutation, while callers use `EditorialProject::edit` or `apply_edit_batch` for
atomic publication of broader project changes. Selection is authoritative command intent, not
hover, focus, marquee geometry, or optimistic UI presentation state. Sync resolution identifies
participating tracks but performs no transform on its own. Links and groups are timeline-local and
have no independent durable ID. Edit material is currently one timed object per command;
multi-object source sequences and link-group targeting belong to later command and orchestration
layers. Audio continuity is structural evidence rather than signal analysis or playback. The model
has no stable Serde schema, hostile-input collection bounds, or production consumer outside its
contract tests. The engine color propagation contract consumes only the narrow metadata
seam and does not make timeline-to-graph compilation operational.

## Maintenance notes

Treat track clocks and semantics, object identity, continuity, physical-time equality, source-aware
fragmentation, explicit transition invalidation, result reporting, marker ownership, exact snapping,
metadata ordering, source-link resolution,
selection expansion, track intent, clip relationship partitions, reconciliation, transition
adjacency, nesting acyclicity, and atomic publication as public contracts. Extend tests before
changing them. Later higher-level operations must consume `tracks_affected_by_sync` and exact
selection state instead of recreating those policies. Add advanced edit commands, interchange, and
graph compilation only through their owning modules, and update project, engine, API, CLI,
persistence, and fixture maps when those paths begin consuming native timeline state. Preserve the
OTIO fixture's versioned semantics rather than inferring interchange behavior from the native model
alone.
