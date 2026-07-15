---
module_id: superi-audio
source_paths:
  - open/crates/superi-audio
source_hash: 6aec10950469f952187476ae2e0a5ddf28c3c9c245074baae3dfa78da36a74d4
source_files: 12
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-audio` owns independent audio processing and sample-accurate playback scheduling. Its
foundational graph provides an editable deterministic DAG and a separately prepared plan for
bounded interleaved f32 processing at exact sample coordinates. Its timeline scheduler maps
immutable editorial placements into exact callback source slices and publishes completed device
presentation to the shared audio master clock. Its playback slice discovers operating-system output
devices and moves normalized interleaved samples through a fixed-capacity lock-free queue into typed
platform callbacks.

All three paths enforce the platform-owned audio execution domain, but their control-path owners
remain separate. Graph editing and preparation allocate outside callbacks. Schedule construction and
transport reanchoring also occur outside callbacks. Device discovery and stream setup remain on
control threads. Decoded sample binding, buses, mixing, routing execution, resampling, metering,
plugins, and engine orchestration remain separate concerns.

## Source inventory

- `open/crates/superi-audio/Cargo.toml`: Declares dependencies on `superi-core`,
  `superi-concurrency`, exact CPAL 0.17.3, and ringbuf 0.4.8.
- `open/crates/superi-audio/src/graph.rs`: Implements typed audio graph, node, and edge identities;
  editable node and edge storage; deterministic cycle-safe topology; exact channel-layout
  validation; destination-scoped preparation; processor contracts; preallocated intermediate
  buffers; and exact consecutive block processing on the audio domain.
- `open/crates/superi-audio/src/hosting.rs`: Placeholder for additive VST3 and Audio Unit hosting.
- `open/crates/superi-audio/src/lib.rs`: Documents the implemented graph and scheduling boundaries
  and exports the seven audio concern modules.
- `open/crates/superi-audio/src/metering.rs`: Placeholder for metering and audio analysis.
- `open/crates/superi-audio/src/mixing.rs`: Placeholder for buses, levels, fades, and mixing.
- `open/crates/superi-audio/src/playback.rs`: Implements stable device identity, capability
  discovery, exact configuration validation, bounded producer and callback endpoints, sample
  conversion, audio-clock publication, atomic telemetry, and production stream lifecycle.
- `open/crates/superi-audio/src/resample.rs`: Placeholder for sample-rate conversion.
- `open/crates/superi-audio/src/sync.rs`: Implements exact timeline placements, immutable schedule
  validation, transport epochs, callback-window mapping, lazy source slices, and audio-master
  publication.
- `open/crates/superi-audio/tests/audio_graph_contract.rs`: Proves graph topology, validation,
  preparation, exact bounded processing, continuity, channel order, and domain ownership.
- `open/crates/superi-audio/tests/timeline_sync_contract.rs`: Proves canonical schedule order,
  callback ownership, exact clipping and silence gaps, seek epochs, overflow and clock rejection,
  long-duration timing, audio-master publication, and zero video drift.
- `open/crates/superi-audio/tests/device_output_contract.rs`: Proves capability containment, bounded
  whole-frame admission, allocation ceilings, timed silence, clock progression, persisted device
  identities, domain-conflict behavior, telemetry, and real host discovery.

## Public surface

The crate root exports `graph`, `hosting`, `metering`, `mixing`, `playback`, `resample`, and `sync`.
`graph`, `sync`, and `playback` contain substantive behavior.

`graph` exposes ordered audio-owned `AudioGraphId`, `AudioNodeId`, and `AudioEdgeId` values.
`AudioNode` declares one optional input layout and one output layout. `AudioEdge` identifies one
directed route. `AudioGraph` owns a fixed sample rate and positive maximum frame count, supports
checked node and edge mutation, and exposes deterministic topology. `AudioGraph::prepare` consumes
processor implementations for one destination and returns `PreparedAudioGraph`, whose `process`
method accepts exact `SampleTime`, bounded frames, and caller-owned output.

`sync` exposes these scheduling values:

- `AudioTimelinePlacement` retains track and clip identity, authored track order, exact record and
  source starts, and a positive frame count on one sample clock.
- `AudioTimelineSchedule` tags a canonical immutable placement snapshot with timeline identity,
  revision, and sample rate. It rejects conflicting order bindings, duplicate clip identities,
  rate mismatches, and within-track overlap while preserving cross-track overlap and silence gaps.
- `AudioScheduleEpoch` maps one device anchor to one timeline anchor at the same sample rate.
- `AudioTimelineScheduler` binds a schedule to the current epoch and maps device callback windows
  through fixed-anchor checked integer arithmetic.
- `AudioCallbackPlan` exposes exact device and timeline bounds, yields borrowed
  `ScheduledAudioSlice` intersections, and publishes its exclusive presented device end to
  `AudioMasterClock`.

`playback` exposes validated opaque `OutputDeviceId` values, exact capability ranges and stream
configurations, partial discovery failures, bounded `OutputProducer` and `OutputConsumer` endpoints,
clonable telemetry, and an owning `DeviceOutput`. `discover_output_devices` performs real host
enumeration, `create_output_buffer` preallocates the engine-to-device handoff, and
`start_device_output` revalidates and starts the selected stream.

## Architecture and data flow

Graph editing occurs outside the audio callback. Nodes and edges live in ordered maps, adjacency is
kept in ordered sets, and edge insertion validates endpoints, cycles, one incoming route per
processing node, and exact ordered layout equality before mutation. Preparation walks backward from
one destination, validates connectivity and processor coverage, filters stable topological order,
resolves runtime input indices, and fallibly reserves one maximum-sized interleaved f32 buffer per
node.

Prepared processing requires `ExecutionDomain::Audio`, then validates rate, positive bounded frame
count, exact output length, coordinate overflow, and continuity with the prior successful block.
Each processor reads an earlier node's current buffer and writes its own prepared buffer. The
destination is copied into caller-owned output, and continuity advances only after full success.

Schedule construction is also a control-path operation. It validates half-open source and record
windows, one common rate, stable track-order ownership, unique clip identities, and no overlap
within a track, then sorts once into boxed storage. A transport owner binds the schedule to a device
and timeline epoch and explicitly reanchors after seeks or discontinuities.

The callback asks `plan_callback` for an exact device window. The scheduler derives timeline bounds
from fixed anchors without polling or integrating prior rounded positions. A lazy borrowed iterator
yields source intersections in authored track and record order, leaving uncovered intervals as
silence. A future renderer can feed those exact source windows through a prepared graph while
retaining channel and routing ownership. Only after the complete device window is audible does the
caller publish its exclusive end to `AudioMasterClock`; existing playback policy then paces video.

Playback discovery and stream setup stay on control threads. The sole producer admits complete
interleaved frame submissions or rejects them whole. The platform callback enters
`ExecutionDomain::Audio`, converts samples directly into the device-owned typed slice, substitutes
silence on starvation or a domain conflict, advances `AudioMasterClock` by every complete presented
frame, and updates relaxed atomics. Device capabilities are re-read before stream construction, and
portable speaker positions remain explicitly unknown when CPAL reports only channel count.

## Dependencies and consumers

- `superi-core` supplies ordered `ChannelLayout`, exact `SampleTime`, and the shared classified
  error model plus timeline, track, and clip identities. Audio composes these meanings instead of
  duplicating them.
- `superi-concurrency` supplies `ExecutionDomain::Audio` and its platform-owned, nonblocking,
  allocation-free policy plus `AudioMasterClock`, `PlaybackClock`, and downstream A/V policy. The
  prepared graph, scheduler, and output callback enforce the audio domain at their boundaries.
- CPAL 0.17.3 supplies CoreAudio, WASAPI, and ALSA discovery and output adapters while remaining
  compatible with the repository Rust declaration. ringbuf 0.4.8 supplies the preallocated SPSC
  sample queue. Linux CI installs ALSA development headers.
- `superi-engine` declares `superi-audio` but has no production adapter from timeline and decoded
  media state into the schedule or prepared graph.
- `superi-timeline` remains upstream through future engine composition rather than a direct Rust
  dependency. Its sample-exact placements, track order, channel layout, and routing intent are
  adapter inputs.
- `superi-media-io` remains the decoded sample owner and is not a direct dependency. No production
  decoder currently feeds a prepared graph from scheduled slices.
- The three public integration contracts are the current real consumers. They process exact adjacent
  blocks, publish scheduled presentation through actual concurrency clocks, and exercise bounded
  device output.

## Invariants and operational boundaries

- Graph and schedule sample rates are positive integral clocks. Resampling and rounding are outside
  both paths.
- Graph nodes and edges are deterministically inspectable. Rejected mutations leave primary and
  adjacency collections unchanged.
- A source has no input. A processing node has exactly one connected input in this foundational
  graph. Multi-input summing, buses, channel maps, and automation belong to later mixing work.
- Edges require exact ordered channel-layout equality. The graph performs no implicit upmix,
  downmix, remap, pan, or resample.
- Prepared execution includes only the chosen destination and its connected ancestors. Its order
  and buffers do not change during processing.
- Every block has one exact integral sample clock and must follow the prior successful block
  without a gap or overlap. Failed prevalidation does not advance continuity.
- The graph-owned successful process path takes no lock, allocates no memory, and frees no memory.
  `AudioProcessor` implementers receive the same explicit contract, but the graph cannot prove the
  internals of caller-supplied processor code.
- Processor failure may retain processor-internal partial state; callers must treat processor error
  recovery according to that processor's contract. Graph continuity advances only on full success.
- All implementation is safe Rust. Plugin ABI, worker isolation, and native code outside CPAL
  remain outside this module.
- The output queue is nonzero, checked for overflow, capped at 1,048,576 samples, and admits only
  finite normalized complete frames. The callback takes no blocking lock and grows no storage.
- Starvation and conflicting domain ownership produce timed digital silence rather than clock
  stalls. Device reset, removal, or format changes require control-side stream reconstruction.
- Timeline placements are nonnegative, nonempty, checked half-open record windows. Source
  coordinates may be negative. One track has one order, one order identifies one track, and only
  different tracks may overlap.
- Callback planning rejects windows before the active epoch, rate mismatch, zero length, and
  coordinate overflow. Fixed anchors prevent accumulated drift.
- Successful graph processing, callback planning, schedule iteration, and master publication
  require the audio domain and use no graph-owned or scheduler-owned lock, allocation, or free.
  Caller-provided processors retain responsibility for their own real-time behavior.
- `publish_presented` reports only a complete audible window. Planning alone never advances the
  master and audio never corrects itself to video.
- Scheduled slices intentionally contain no sample buffer, channel layout, route, gain, or
  processing state. Those meanings remain attached to their owners until an engine adapter binds
  them to the graph.
- All current implementation is safe Rust, offline, and open-tree only; native output is isolated
  behind CPAL.

## Tests and verification

`audio_graph_contract.rs` has three public tests covering stable identity and topology, atomic
route rejection, connectivity and processor validation, exact channel-distinct source and gain
processing over adjacent 48 kHz stereo blocks, audio-domain enforcement, and nonadvancing failures
for rate, bound, output length, and continuity errors.

`timeline_sync_contract.rs` has five public tests covering canonical placement order, allowed
cross-track overlap, rejected same-track overlap and invalid identities or clocks, exact callback
clipping and silence gaps, audio-domain ownership, seek epochs, coordinate limits, a one-hour exact
mapping, real `AudioMasterClock` publication, and zero observed video drift.

Together these contracts prove the graph and scheduler coexist without changing exact timing or
channel meaning. Dependent concurrency clock and A/V tests and timeline track-semantics tests guard
the composed contracts. Deterministic local proof does not claim physical hardware latency,
hot-plug behavior, decoded sample binding, routed mixing, or hardware A/V behavior.

Two playback unit tests and ten public output contracts prove typed conversion, backend-default
buffer semantics, capacity and normalized-sample validation, whole-frame backpressure, silence and
telemetry, exact clock progression, persisted locators, domain-conflict degradation, production host
discovery, endpoint thread transfer, and 5,120,000 simulated frames without accumulated drift.

## Current status and risks

The independent audio graph, sample-accurate schedule, and production device-output substrate are
substantive and publicly test-backed. Four sibling modules remain documentation-only placeholders.
There is no production timeline adapter, decoded-audio fetch, scheduled-slice graph binding, mixing,
routing execution, resampling, metering, plugin host, engine playback composition, or end-to-end
source-playback-final-mix path.

The single-input shape is deliberately narrow to avoid inventing later bus semantics. Extending it
to multi-input processing will require precomputed input views that preserve allocation-free callback
behavior. The schedule iterator is deterministic and allocation-free but scans placements linearly;
a future index must be prepared outside callbacks and preserve exact render order. Caller processors
remain a trust boundary for real-time safety and error atomicity. Physical latency, semantic
channel routing, hot-plug, constrained-device, and soak evidence remain
owned by the platform audio and physical test lanes.

## Maintenance notes

Preserve the edit-versus-prepare split, fixed schedule epochs, stable identity ordering, exact
sample and channel meanings, fallible preallocation, whole-frame queue admission, explicit capacity
ceiling, timed-silence clock behavior, callback-only atomic telemetry, and failure-only diagnostic
allocation. Any multi-input or indexed extension must define ordering explicitly and prove callback
safety. Keep discovery and stream setup on control threads, and revalidate capabilities before
stream creation.

When engine integration arrives, adapt immutable timeline and decoded audio owners into the
existing schedule and graph types instead of adding upward dependencies. Keep channel layout and
routing intent attached through that adapter, publish only completed audible windows, and add a
real engine consumer before claiming source playback, mixing, or final delivery.

After source changes, refresh this map's inventory, architecture, invariants, tests, hash, and file
count from resulting behavior, then update consumer maps and validate the global map closure.
