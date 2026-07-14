---
module_id: superi-timeline
source_paths:
  - open/crates/superi-timeline
source_hash: 7fca0a018e05be40f92c40587911e944dbe2b89bc40908bb3792ea424826e135
source_files: 11
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-timeline` owns embeddable editorial track semantics and reserves the Rust-native timeline,
OTIO-compatible interchange, editing operations, markers, multicam, nesting, and compilation to the
generic graph. Its implemented model distinguishes video, audio, caption, and timed-data tracks.
Audio semantics own sample-exact placement, ordered channel routing intent, linked-clip-preserving
reshaping, and structural continuity inspection. The crate does not yet own production project,
timeline, track, or clip containers.

## Source inventory

- `open/crates/superi-timeline/Cargo.toml`: Declares runtime dependencies on `superi-core` and
  `superi-graph`, plus development-only `serde_json` for canonical OTIO fixture contracts.
- `open/crates/superi-timeline/src/compile.rs`: Placeholder for timeline-to-graph compilation.
- `open/crates/superi-timeline/src/edit_ops.rs`: Placeholder for general editorial edit primitives.
- `open/crates/superi-timeline/src/lib.rs`: Documents the partial implementation state and exports
  the seven timeline namespaces.
- `open/crates/superi-timeline/src/markers.rs`: Placeholder for markers, metadata, bins, and media
  management.
- `open/crates/superi-timeline/src/model.rs`: Implements track-kind values, video compositing,
  exact caption and data clocks, bounded semantic identifiers, complete audio channel routing,
  sample-exact linked audio spans, checked split and trim operations, and continuity reports.
- `open/crates/superi-timeline/src/multicam.rs`: Placeholder for a multicam data model.
- `open/crates/superi-timeline/src/nested.rs`: Placeholder for nested sequences and compound clips.
- `open/crates/superi-timeline/src/otio.rs`: Reserves the ratified OTIO-compatible serialization
  boundary and points to the shared 0.18.1 fixtures.
- `open/crates/superi-timeline/tests/otio_fixture_contract.rs`: Proves canonical OTIO schema,
  hierarchy, identity, timing, relationships, opaque retention, and unsupported diagnostics.
- `open/crates/superi-timeline/tests/track_semantics_contract.rs`: Public consumer proof for all four
  track kinds, exact clocks, channel routing, linked audio reshaping, continuity, and validation.

## Public surface

The library exports `compile`, `edit_ops`, `markers`, `model`, `multicam`, `nested`, and `otio`.
`model` exposes:

- `TrackKind` and `TrackSemantics` as the embeddable media-class boundary with one exact edit clock.
- `VideoTrackSemantics` and `VideoCompositing` for frame-rate and visual contribution intent.
- `AudioTrackSemantics`, `AudioRouting`, `AudioRouteDestination`, `AudioChannelRoute`, and
  `AudioChannelTarget` for one integral sample clock, ordered source meaning, typed track or main
  destination, explicit output channel meaning, and explicit mute decisions.
- `AudioSpan` for a `ClipId`-linked record-to-source sample mapping. Checked split, leading trim, and
  trailing trim preserve the link, duration, and exact synchronization.
- `AudioContinuityReport`, `AudioSeam`, `AudioRecordContinuity`, and `AudioSourceContinuity` for
  caller-ordered inspection of seamless coverage, gaps, overlaps, continuous source samples, source
  jumps, and linked-clip changes.
- `CaptionTrackSemantics`, `CaptionPurpose`, and `LanguageTag` for exact cue clocks, explicit timed
  text purpose, and normalized bounded language-tag syntax.
- `DataTrackSemantics` and `DataSchema` for exact timed-event clocks and a scheme identifier URI plus
  optional scheme-specific value.

The other modules remain placeholders. There is no native OTIO reader or writer, general timeline
container, editor, marker model, nesting model, multicam model, or graph compiler.

## Architecture and data flow

Future editorial tracks can embed one `TrackSemantics` value without copying identity, time, or
channel meanings. Video, caption, and data values carry their exact core-owned clock directly.
Audio construction validates one nonzero sample rate, reuses the ordered core `ChannelLayout`, and
requires one routing decision for every source channel in stream order. Non-muted channel targets
must occur in the declared destination layout, while duplicate source decisions and implicit source
loss are rejected.

`AudioSpan::new` converts an editorial record position to the linked source sample clock with exact
rounding only. It retains the core `ClipId`, record sample, source sample, and sample-frame count.
Split and trim operations derive both sample starts with checked integer arithmetic, so reshaping
cannot drift record-to-source synchronization or erase the linked object. Continuity audit consumes
spans in nondecreasing record order, rejects sample-rate mismatch, and emits every adjacent record
and source relationship. Its uninterrupted-coverage predicate reports structural gaps only and does
not claim that waveform samples are audible.

There is still no production path from these semantic values into project persistence, engine
orchestration, the public API, playback, or graph compilation. The public integration test is the
current real consumer.

## Dependencies and consumers

- Production model code uses `superi-core` `TrackId`, `ClipId`, exact time values, channel layout
  values, and the shared actionable error vocabulary.
- `superi-graph` remains a declared but unused dependency reserved for timeline compilation.
- `serde_json` remains development-only and reads checked-in canonical OTIO JSON.
- `superi-project` and `superi-engine` declare `superi-timeline` as a dependency but import no
  production timeline item.
- Public integration tests consume the semantic model directly. No API or CLI surface exposes it.

## Invariants and operational boundaries

- Cargo places timeline above the node-agnostic graph and below project and engine.
- Track semantics reuse core identity, rational time, sample time, and ordered channel meanings.
- Audio record starts must map exactly to source sample boundaries. Implicit rounding is rejected.
- Audio routes cover every ordered source channel exactly once. A channel is explicitly routed or
  explicitly muted; no channel disappears implicitly.
- Audio span construction, endpoint calculation, splitting, trimming, and seam distance calculation
  fail on coordinate overflow instead of wrapping or panicking.
- Continuity audit separates record coverage from source continuity. Overlap is uninterrupted record
  coverage, a gap is explicit absent coverage, and a source change remains linked to typed clip IDs.
- Language tags normalize case because RFC 5646 case carries no meaning. Construction validates
  BCP 47 language, extlang, script, region, variant, extension, private-use, and grandfathered
  grammar with duplicate variant and singleton rejection, but does not claim current IANA registry
  membership.
- Data schema identities are bounded ASCII values without whitespace or control characters. Data
  payloads and cue ranges remain outside this checkpoint.
- The OTIO fixture policy remains opaque unknown-field preservation with explicit diagnostics.

## Tests and verification

Six track-semantics integration tests prove four distinct track kinds, exact edit clocks, direct
routing replacement, typed route destinations, ordered channel coverage, explicit mute behavior,
invalid routing rejection, exact record-to-source sample alignment, linked split and trim behavior,
half-open record and source ranges, fractional-boundary rejection, gap and overlap reports, source
continuity, linked-clip changes, bounded caption and data validation, and checked extreme seam
distances. Three existing OTIO integration tests continue to prove the first editorial fixture,
coverage fixture, and preserve-opaque unsupported-object contract.

## Current status and risks

Typed track semantics are implemented and publicly test-backed. General editorial containers,
rational range ownership, selection, targeting, grouping, general edit operations, native OTIO
serde, nesting, multicam, and graph compilation remain absent. Audio continuity is structural
timeline evidence, not signal analysis, loudness measurement, device routing, playback, or an audio
engine. Caption tag validation is intentionally syntax-only, and data tracks define payload type
identity without defining payload representation.

## Maintenance notes

Embed these semantic values in the future canonical `Track` rather than creating another kind,
clock, channel, or routing model. Keep `AudioSpan` as the sample-exact bridge when general source and
record range ownership lands. Global routing validation must later reject track cycles and invalid
destination kinds when a complete timeline container exists. Extend versioned OTIO fixtures before
claiming preservation of native audio, caption, or data semantics through interchange.
