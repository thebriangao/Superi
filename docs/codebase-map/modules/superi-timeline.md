---
module_id: superi-timeline
source_paths:
  - open/crates/superi-timeline
source_hash: 03d7bc55ba651427a6cb5f19e1fb8bde5cd6e1e09bc53e976d8e53ce5d9461a7
source_files: 15
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-timeline` owns the foundational Rust-native editorial project model and typed track
semantics. It represents linked media, timelines, ordered tracks, clips, explicit gaps,
transitions, generators, captions, and nested timeline sources with core-owned identities and
exact rational timing. It also owns authoritative timeline selection, track targeting, sync locks,
linked selection, and clip grouping. Video, audio, caption, and timed-data tracks carry their
explicit clock and media behavior. Whole-project validation and revision-checked atomic drafts keep
linked objects, user intent, timing, synchronization, nesting, and direct edits valid at publication
boundaries.

The crate continues to reserve advanced edit operations, markers, multicam behavior,
OTIO-compatible interchange, and deterministic timeline-to-graph compilation. Those surfaces are
not implemented. The canonical OTIO 0.18.1 fixture remains executable evidence for future
interchange work rather than a production reader or writer.

## Source inventory

- `open/crates/superi-timeline/Cargo.toml`: Declares runtime dependencies on `superi-core` and
  `superi-graph`, plus development-only `serde_json` for canonical OTIO fixture contracts.
- `open/crates/superi-timeline/src/compile.rs`: Placeholder for timeline-to-graph compilation.
- `open/crates/superi-timeline/src/edit_ops.rs`: Placeholder for general editorial edit primitives.
- `open/crates/superi-timeline/src/edit_state.rs`: Implements exact and relationship-expanded
  selection, per-track targeting and sync-lock intent, canonical clip links and groups, stable
  introspection, and structural reconciliation.
- `open/crates/superi-timeline/src/ids.rs`: Re-exports the canonical project and editorial object
  identities owned by `superi-core`.
- `open/crates/superi-timeline/src/lib.rs`: Exports the implemented identity, edit-state, and model
  modules plus the staged editorial namespaces.
- `open/crates/superi-timeline/src/markers.rs`: Placeholder for markers, metadata, bins, and media
  management.
- `open/crates/superi-timeline/src/model.rs`: Implements four track kinds, track-specific timing and
  media semantics, linked media, every foundational editorial object, ordered tracks, timelines,
  validated project snapshots, and atomic revision-checked editing.
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
- `open/crates/superi-timeline/tests/otio_fixture_contract.rs`: Proves canonical OTIO schema,
  hierarchy, identity, timing, relationships, opaque retention, and unsupported diagnostics.
- `open/crates/superi-timeline/tests/track_semantics_contract.rs`: Proves all four track kinds,
  exact clocks, channel routing, linked audio reshaping, continuity, and bounded validation.

## Public surface

The `ids` module re-exports `ProjectId`, `MediaId`, `TimelineId`, `TrackId`, `ClipId`, `GapId`,
`TransitionId`, `GeneratorId`, and `CaptionId`. These are the same sealed core identifier types used
by every other subsystem.

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

`compile`, `edit_ops`, `markers`, `multicam`, `nested`, and `otio` remain public namespace
reservations without production operations.

## Architecture and data flow

Callers first construct complete track semantics. Video carries a frame rate and compositing mode.
Audio validates one sample rate, reuses the ordered core `ChannelLayout`, and requires one explicit
routing or mute decision per source channel. Caption and data semantics retain their exact clocks
and bounded type identifiers. `AudioSpan` preserves a linked clip identity and derives record and
source samples with checked exact conversion, so split and trim operations cannot silently drift.

Editorial construction and validation then proceed as follows:

1. Callers construct media references and timeline objects using canonical identities, exact
   `TimeRange` values, and `TrackSemantics` embedded in each track.
2. `EditorialProject::new` indexes media and timelines, rejects duplicate identities, and validates
   the complete candidate graph before publishing it.
3. Validation walks every timeline and ordered track using that track's edit clock. It verifies
   local timing and object uniqueness, resolves clip sources, validates transitions against
   adjacent timed items, and follows nested timeline links to reject cycles.
4. A timeline compares track endpoints in physical rational time and exactly rescales the longest
   endpoint to its primary edit rate. This preserves synchronization across clocks such as frames,
   milliseconds, and audio samples without implicit rounding.
5. Read-only accessors expose the published project, while timeline, track, and object lookup keeps
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

The separate fixture path reads checked-in OTIO JSON through development-only `serde_json::Value`
assertions. It does not enter the native model yet.

## Dependencies and consumers

- `superi-core` supplies shared errors, exact rational and sample time, channel layouts, project and
  media identity, and all typed editorial identities used by production source.
- `superi-graph` remains a declared dependency for future compilation but is not imported by
  production timeline source.
- `serde_json` is development-only and reads checked-in canonical JSON. No OTIO library, Python
  package, network path, or fixture-tool runtime dependency enters the crate.
- `superi-project` and `superi-engine` declare `superi-timeline` as a dependency, but neither source
  tree imports a production timeline item yet.
- Public integration tests are the current real consumers. No API or CLI surface exposes the
  general editorial model.

## Invariants and operational boundaries

- Project, media, timeline, track, clip, gap, transition, generator, and caption identities are
  permanent typed domains. Track and editorial object identities are unique across one project.
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
  differ. Media source ranges may exceed an optional available range so overscan is not destroyed.
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
- Advanced retiming, production OTIO preservation, deterministic graph compilation, undo-history
  ownership, markers, multicam, and higher-level editorial commands remain outside this state.

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

Workspace tests, warnings-denied Clippy, formatting, dependency direction, the offline boundary
scan, and codebase-map validation are required delivery gates.

## Current status and risks

The foundational project model, typed track semantics, and authoritative timeline edit state are
substantive and test-backed. Production OTIO reading and writing, graph compilation, advanced edit
transforms, undo ownership, markers, multicam, persistence, and engine or API integration remain
absent.

The model requires equal physical source and record duration for clips. Future time-warp support
must introduce explicit retime state rather than weakening that invariant. Direct setters permit
temporarily invalid unpublished state; callers must use `EditorialProject::edit` for atomic
publication. Audio continuity is structural evidence rather than signal analysis or playback.
Selection is authoritative command intent, not hover, focus, marquee geometry, or optimistic UI
presentation state. Sync resolution identifies participating tracks but performs no insert, ripple,
or trim transform yet. Links and groups are timeline-local and have no independent durable ID.
The model has no stable Serde schema, hostile-input collection bounds, or consumer outside its
contract tests.

## Maintenance notes

Treat track clocks and semantics, object identity, continuity, physical-time equality, source-link
resolution, selection expansion, track intent, clip relationship partitions, reconciliation,
transition adjacency, nesting acyclicity, and atomic publication as public contracts. Extend tests
before changing them. Later edit operations must consume `tracks_affected_by_sync` and exact
selection state instead of recreating those policies. Add interchange and graph compilation only
through their owning modules, and update project, engine, API, CLI, persistence, and fixture maps
when those paths begin consuming native timeline state. Preserve the OTIO fixture's versioned
semantics rather than inferring interchange behavior from the native model alone.
