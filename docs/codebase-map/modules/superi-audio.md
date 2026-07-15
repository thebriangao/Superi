---
module_id: superi-audio
source_paths:
  - open/crates/superi-audio
source_hash: 6aa5c29b2fc50fa7c16f90b9017b982fd6ea3a34baa6221f4e82247771af5601
source_files: 23
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-audio` owns independent audio processing, sample-accurate playback scheduling, and bounded
operating-system input capture. Its
foundational graph provides an editable deterministic DAG, typed direct, send, and auxiliary-return
routes, one terminal master, and a separately prepared plan for bounded interleaved f32 processing
at exact sample coordinates. Exact-layout submix, auxiliary, and master buses consume borrowed
multi-input views and sum in stable route-identity order. Its timeline scheduler maps
immutable editorial placements into exact callback source slices and publishes completed device
presentation to the shared audio master clock. Its playback slice discovers operating-system output
devices and moves normalized interleaved samples through a fixed-capacity lock-free queue into typed
platform callbacks. Audio-owned clip mix state adds transactional gain,
fades, pan, mute, solo, phase, and semantic channel mapping with immutable prepared snapshots. Its
prepared sinc converter maps fixed device-output blocks to exact variable source consumption while
retaining each independent sample clock and smoothly correcting bounded device-rate error.
Common mono, stereo, quad, 5.1, and 7.1 layouts compose the core semantic order, and explicit
prepared speaker or discrete conversion nodes adapt channels without implicit graph routing.
Prepared core effects add cascaded biquad equalization, linked-channel compression and peak
limiting, fixed channel-preserving delay, and normalized soft saturation through the existing
single-input graph processor boundary.
Its transparent prepared meter preserves graph samples while publishing coherent peak, RMS,
true-peak, phase, spectrum, momentary, short-term, and integrated loudness snapshots through
bounded lock-free storage.

Its capture slice discovers input devices, validates exact stream configurations, and publishes
channel-indexed recorded samples at exact physical input coordinates through one preallocated
ring. An independent preallocated monitoring ring preserves normalized interleaved samples for the
existing output path. Atomic arming and monitoring controls take effect at callback boundaries.

All processing paths enforce the platform-owned audio execution domain, but their control-path owners
remain separate. Graph editing and preparation allocate outside callbacks. Schedule construction and
transport reanchoring also occur outside callbacks. Resampler and meter construction and scratch
allocation remain outside callbacks. Device discovery and stream setup remain on control threads. Decoded
sample binding, automation, plugins, and complete engine orchestration remain separate
concerns.

## Source inventory

- `open/crates/superi-audio/Cargo.toml`: Declares dependencies on `superi-core`,
  `superi-concurrency`, exact CPAL 0.17.3, ringbuf 0.4.8, and default-feature-free rubato 0.16.2.
- `open/crates/superi-audio/src/channels.rs`: Defines the common-layout catalog, explicit speaker
  and discrete interpretations, control-side conversion-matrix preparation, and allocation-free
  exact-layout processing with fail-closed validation.
- `open/crates/superi-audio/src/capture.rs`: Implements stable input-device identity, capability
  discovery, exact configuration validation, atomic arming and monitoring, dual bounded capture
  and monitor rings, exact channel-indexed sample coordinates, telemetry, and production stream
  lifecycle.
- `open/crates/superi-audio/src/effects.rs`: Implements validated prepared equalization,
  linked-channel compression and limiting, channel-preserving fixed delay, and normalized soft
  saturation with finite input and output enforcement.
- `open/crates/superi-audio/src/graph.rs`: Implements typed audio graph, node, and edge identities;
  source, processor, submix, auxiliary, and master roles; direct, send, and return routes; one-master
  and deterministic cycle-safe topology; exact channel-layout validation; destination-scoped and
  master preparation; borrowed ordered multi-input views; preallocated intermediate buffers; and
  exact consecutive block processing on the audio domain.
- `open/crates/superi-audio/src/hosting.rs`: Placeholder for additive VST3 and Audio Unit hosting.
- `open/crates/superi-audio/src/lib.rs`: Documents the implemented graph, channel, routing,
  scheduling, capture, playback, conversion, effects, and metering boundaries and exports the
  eleven audio concern modules.
- `open/crates/superi-audio/src/metering.rs`: Implements transparent prepared graph metering,
  per-channel sample peak, RMS and true peak, stereo phase correlation, bounded spectrum analysis,
  K-weighted EBU windows, ITU programme gating, coherent lock-free snapshots, and explicit history
  saturation.
- `open/crates/superi-audio/src/mixing.rs`: Implements validated clip controls, semantic channel
  matrices, revisioned identity mutations, immutable solo-aware snapshots, bounded clip
  preparation, and allocation-free gain, fade, pan, mute, solo, phase, and routing DSP.
- `open/crates/superi-audio/src/playback.rs`: Implements stable device identity, capability
  discovery, exact configuration validation, bounded producer and callback endpoints, sample
  conversion, audio-clock publication, atomic telemetry, and production stream lifecycle.
- `open/crates/superi-audio/src/resample.rs`: Implements prepared fixed-output band-limited
  conversion, ordered interleaved and planar transfer, exact dual-clock accounting, sinc delay
  reporting, and bounded ramped device-clock correction.
- `open/crates/superi-audio/src/routing.rs`: Implements allocation-free unity summing for prepared
  submix, auxiliary, and master buses in stable route-identity order with non-finite rejection.
- `open/crates/superi-audio/src/sync.rs`: Implements exact timeline placements, immutable schedule
  validation, transport epochs, callback-window mapping, lazy source slices, and audio-master
  publication.
- `open/crates/superi-audio/tests/audio_graph_contract.rs`: Proves graph topology, validation,
  preparation, exact bounded processing, continuity, channel order, and domain ownership.
- `open/crates/superi-audio/tests/channel_layout_contract.rs`: Proves common semantic order,
  documented speaker coefficients, discrete copy, drop, and zero-fill behavior, fail-closed
  validation, and exact consecutive sample time through an explicit prepared graph node.
- `open/crates/superi-audio/tests/timeline_sync_contract.rs`: Proves canonical schedule order,
  callback ownership, exact clipping and silence gaps, seek epochs, overflow and clock rejection,
  long-duration timing, audio-master publication, and zero video drift.
- `open/crates/superi-audio/tests/device_output_contract.rs`: Proves capability containment, bounded
  whole-frame admission, allocation ceilings, timed silence, clock progression, persisted device
  identities, domain-conflict behavior, telemetry, and real host discovery.
- `open/crates/superi-audio/tests/device_input_contract.rs`: Proves capability containment, atomic
  arming and monitoring, exact timing through gaps, independent whole-frame backpressure, malformed
  callback rejection, domain-conflict behavior, stable locators, real discovery, long-session
  drift freedom, and a real monitoring bridge into bounded output playback.
- `open/crates/superi-audio/tests/clip_mixing_contract.rs`: Public consumer proof for every clip
  control, exact multi-block envelopes, snapshot solo behavior, atomic identity mutations, invalid
  layouts and values, clip bounds, and failure atomicity.
- `open/crates/superi-audio/tests/routing_contract.rs`: Proves dry submix, auxiliary send and return,
  single-master delivery, stable route order independent of edit history, exact consecutive blocks,
  and atomic role, layout, master, and cycle rejection.
- `open/crates/superi-audio/tests/resample_contract.rs`: Proves channel-preserving conversion,
  anti-aliasing, exact source and device clocks, continuity, domain and input rejection, and signed
  device-clock correction.
- `open/crates/superi-audio/tests/audio_effects_contract.rs`: Proves all public effects through the
  real prepared graph, including adjacent-block state, channel linking, exact delay timing,
  response and ceiling behavior, bounds, and non-finite rejection.
- `open/crates/superi-audio/tests/metering_contract.rs`: Proves sample-transparent graph execution,
  exact channel semantics and continuity, peak, RMS, inter-sample true peak, phase, spectrum,
  calibrated EBU loudness windows, ITU gating, and bounded preparation.

## Public surface

The crate root exports `capture`, `channels`, `effects`, `graph`, `hosting`, `metering`, `mixing`,
`playback`, `resample`, `routing`, and `sync`. Every module except the hosting placeholder contains substantive
behavior.

`capture` exposes validated opaque `InputDeviceId` values, exact capability ranges and stream
configurations, partial discovery failures, atomic `CaptureControl`, bounded `CaptureReader` and
`MonitorReader` endpoints, clonable telemetry, and an owning `DeviceCapture`. Recording preserves
exact `SampleTime`, channel index, and normalized value; monitoring preserves complete interleaved
frames independently. `discover_input_devices` performs real host enumeration,
`create_capture_buffer` preallocates both handoffs, and `start_device_capture` revalidates and
starts the selected stream.

`channels` exposes `CommonChannelLayout`, `ChannelInterpretation`, and `PreparedChannelMixer`.
Canonical layouts preserve core speaker order. Speaker conversion implements documented mono,
stereo, quad, and 5.1 matrices; exact 7.1 identity is supported while undefined 7.1 speaker
conversion fails closed. Discrete conversion copies by stream index and explicitly drops or
zero-fills unmatched channels.

`effects` exposes immutable validated configurations and prepared processors for low-pass,
high-pass, peaking, low-shelf, and high-shelf equalization; feed-forward compression;
zero-lookahead peak limiting; fixed feedback delay; and normalized soft saturation. Equalizer,
compressor, and limiter preparation bind one positive sample rate. Every processor binds one
unchanged ordered layout and implements the graph's public single-input `AudioProcessor` contract.

`metering` exposes validated `MeterConfig`, transparent `PreparedMeter`, independently retained
`MeterReadings`, and owned `MeterSnapshot` values. Snapshots retain exact sample coordinates and
semantic channel order while reporting per-channel instantaneous and held levels, phase,
frequency bins, EBU loudness windows, bounded integrated programme history, and saturation.

`graph` exposes ordered audio-owned `AudioGraphId`, `AudioNodeId`, and `AudioEdgeId` values.
`AudioNode` declares a source, single-input processor, or typed multi-input bus with exact input and
output layout. `AudioEdge` retains direct, send, or auxiliary-return intent. `AudioGraph` owns a
fixed sample rate, positive maximum frame count, and at most one master, supports checked node and
edge mutation, and exposes deterministic topology. `AudioGraph::prepare` consumes processor
implementations for one destination, while `prepare_master` selects the authored master. The
resulting `PreparedAudioGraph` exposes stable prepared input routes and processes exact
`SampleTime`, bounded frames, and caller-owned output. `AudioProcessInputs` lazily yields borrowed
current-block samples, source identities, route identities, and layouts without allocation.

`routing::SummingBus` is a unity processor for submix, auxiliary, and master nodes. It clears the
prepared output and adds exact-layout inputs in ascending route identity, rejecting non-finite
results. It does not own gain, pan, effects, automation, or channel conversion.

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

`mixing` exposes `ChannelMap`, complete inspectable `ClipMixControls`, revisioned `ClipMixState`,
identity-preserving `ClipMixMutation` values, immutable `ClipMixSnapshot`, and prepared
`ClipMixProcessor`. State mutations set, inherit, transfer, or remove complete intent atomically.
Preparation binds one clip identity to an exact `SampleTime` interval and fixed layouts.

`resample` exposes validated `DeviceClockErrorPpm` and `SampleRateConverterConfig` values,
`PreparedSampleRateConverter`, and exact `ResampleBlockReport` accounting. The prepared converter
reports fixed device output, maximum source lookahead, exact next source demand, sinc filter delay,
and current native-clock positions before converting one interleaved block on the audio domain.

## Architecture and data flow

Graph editing occurs outside the audio callback. Nodes and edges live in ordered maps, adjacency is
kept in ordered sets, and edge insertion validates endpoints, cycles, route roles, one incoming
route per ordinary processor, variadic bus inputs, one master, and exact ordered layout equality
before mutation. Direct routes carry dry or submix flow, sends terminate at auxiliaries, returns
leave auxiliaries for a submix or master, and the master has no output route. Preparation walks
backward from one destination, validates connectivity and processor coverage, filters stable
topological order, resolves every input to an earlier runtime index in edge identity order, and
fallibly reserves one maximum-sized interleaved f32 buffer per node.

Prepared processing requires `ExecutionDomain::Audio`, then validates rate, positive bounded frame
count, exact output length, coordinate overflow, and continuity with the prior successful block.
Each processor reads earlier current-block buffers through a borrowed input view and writes its own
prepared buffer. `SummingBus` performs deterministic unity addition without callback allocation.
The destination is copied into caller-owned output, and continuity advances only after full success.

Channel conversion is authored as an ordinary processor with distinct exact input and output
layouts. Its dense matrix is allocated and filled during control-side preparation. Callback
processing validates both layouts, exact sample counts, and finite input before writing output,
then performs only bounded matrix arithmetic without allocation, locks, or timing mutation.

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

Capture discovery, buffer construction, and stream setup also stay on control threads. The input
callback enters `ExecutionDomain::Audio`, validates complete finite normalized frames, advances its
physical sample position for every accepted platform frame, and independently admits whole frames
to recording and monitoring rings according to atomic controls. Disarmed, pressured, and
domain-conflicted intervals remain physical timing gaps instead of collapsing later sample
coordinates. A monitor reader copies into caller-owned storage for submission through the existing
output producer.

Clip preparation validates that fades fit the exact clip duration and precomputes a dense
destination-by-source routing matrix plus per-output phase coefficients. The processor validates
the prepared clock, layouts, and half-open clip interval before touching output. It maps semantic
channels, applies the W3C equal-power stereo equations, multiplies bounded linear gain and
endpoint-inclusive sample fades, applies per-destination phase, and honors one snapshot-wide solo
decision. Its successful callback path allocates nothing and takes no lock.

Sample-rate conversion is prepared on a control path with a fixed output block, ordered layout,
source and device anchors, and bounded device error. The callback validates ownership, exact
positions, fixed lookahead and output storage, finite samples, correction bounds, and coordinate
capacity before changing DSP state. It deinterleaves only the exact source prefix required after
the ramped ratio update, runs the preallocated sinc converter, reinterleaves the fixed output, and
advances both exact clocks only after success. A positive observed device-rate error applies
`1 / (1 + ppm / 1_000_000)` to the nominal output-to-input ratio.

Effect preparation validates every finite bounded parameter, computes coefficients and time
constants, and allocates per-channel or delay state before callback execution. Processing validates
the exact input and output layout, buffer shape, sample rate where applicable, and all input samples
before state mutation. Equalizer state is independent per band and channel; dynamics detection and
gain are linked once per frame; the delay ring advances in interleaved channel order; and all state
continues identically across arbitrary adjacent block partitions.

Meter preparation validates the exact sample clock, ordered channel layout, callback bound,
spectrum dimensions, and integrated-history ceiling before fallibly allocating all DSP and atomic
publication storage. The audio callback copies its sole input unchanged, then updates K-weighting,
four-phase true-peak interpolation, rolling loudness and spectrum windows, and one seqlock-protected
atomic snapshot. The control-side reader retries concurrent publication, constructs owned channel
and spectrum values, and performs the two-stage integrated loudness gate without blocking audio.

## Dependencies and consumers

- `superi-core` supplies ordered `ChannelLayout`, exact `SampleTime`, and the shared classified
  error model plus timeline, track, and clip identities. Audio composes these meanings instead of
  duplicating them.
- `superi-concurrency` supplies `ExecutionDomain::Audio` and its platform-owned, nonblocking,
  allocation-free policy plus `AudioMasterClock`, `PlaybackClock`, and downstream A/V policy. The
  prepared graph, scheduler, and output callback enforce the audio domain at their boundaries.
- CPAL 0.17.3 supplies CoreAudio, WASAPI, and ALSA input and output adapters while remaining
  compatible with the repository Rust declaration. ringbuf 0.4.8 supplies the preallocated SPSC
  sample queue. Linux CI installs ALSA development headers.
- rubato 0.16.2 supplies the MIT-licensed adjustable asynchronous sinc implementation with a Rust
  1.61 declaration. Audio uses no default FFT feature and calls only its prepared
  `process_into_buffer` path.
- `superi-engine::audio_mix` owns production timeline edit and clip-mix identity reconciliation.
  No production adapter yet binds decoded media into the schedule or prepared graph.
- `superi-timeline` remains upstream through future engine composition rather than a direct Rust
  dependency. Its sample-exact placements, track order, channel layout, and routing intent are
  adapter inputs.
- `superi-media-io` remains the decoded sample owner and is not a direct dependency. No production
  decoder currently feeds a prepared graph from scheduled slices.
- The ten public integration contracts are the current real consumers. They process exact adjacent
  blocks through clip DSP, dry, auxiliary, submix, and master paths, publish scheduled presentation
  through actual concurrency clocks, exercise bounded device output, and prove clip identity
  inheritance while converting between independent source and device clocks and applying core
  effects through the prepared graph. The metering contract places a real meter between a source
  and master bus. Input proof
  additionally exercises exact channel-indexed capture and routes monitoring samples through the
  production bounded output handoff.

## Invariants and operational boundaries

- Graph and schedule sample rates are positive integral clocks. Explicit conversion is a separate
  prepared boundary and neither path rounds implicitly.
- Graph nodes and edges are deterministically inspectable. Rejected mutations leave primary and
  adjacency collections unchanged.
- A source has no input and an ordinary processor has exactly one direct input. Submix, auxiliary,
  and master buses accept one or more routes and sum them in immutable edge identity order.
- One graph has at most one master. The master is terminal, send destinations are auxiliary buses,
  and auxiliary returns terminate only at submix or master buses. Failed route edits are atomic.
- Edges require exact ordered channel-layout equality. The graph performs no implicit upmix,
  downmix, remap, or pan. Callers insert an explicit prepared channel processor when a layout
  transition is intended. Explicit rate conversion is a separate prepared boundary.
- A converter owns one unchanged ordered layout and two exact native sample clocks. Each call emits
  one fixed device block, consumes a reported leading prefix of fixed maximum source lookahead,
  and exposes sinc latency separately from sample-coordinate advancement.
- Converter rejection before DSP leaves both positions unchanged. Its successful path uses only
  preparation-owned buffers, takes no lock, and performs no heap allocation or free.
- Effects require one exact connected layout and finite input. Equalizer state is per channel;
  compression and limiting deliberately apply one linked gain across the frame; delay never crosses
  channel positions; and successful processing allocates and locks nothing.
- The limiter is zero-lookahead: it adds no latency and constrains the current frame immediately.
  Compressor and limiter release state, equalizer history, and delay contents persist across exact
  adjacent graph blocks.
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
- Capture rings are nonzero, checked for overflow, capped at 1,048,576 samples each, and admit only
  finite normalized complete frames. Recording and monitoring pressure are independent and never
  admit a partial frame.
- Capture callback processing takes no blocking lock, grows no storage, and preserves the physical
  sample clock across disarmed, pressured, and domain-conflicted intervals. CPAL channel count is
  retained without inventing semantic speaker positions.
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
- Clip mix publication is revision checked and atomic. Complete controls follow a right fragment,
  transfer to a replacement, and disappear with a removed clip through engine-owned reconciliation.
- Nonzero pan requires canonical stereo output. Gain and route coefficients are finite and bounded;
  fades use exact integer sample lengths and must fit the prepared clip interval.
- Meter processing requires one exact-layout input, copies it before analysis, and never changes
  sample timing, routing, channel order, or continuity. Callback state and publication storage are
  preallocated, lock-free, and bounded; programme history reports saturation instead of growing.
- True peak uses the ITU four-phase FIR at 48 kHz, loudness applies K weighting with LFE exclusion
  and surround weighting, momentary and short-term windows are 400 ms and 3 s, and integrated
  loudness applies absolute and relative gates. Spectrum resolution is explicitly configured and
  bounded rather than inferred as a mastering-grade FFT contract.

## Tests and verification

`audio_graph_contract.rs` has three public tests covering stable identity and topology, atomic
route rejection, connectivity and processor validation, exact channel-distinct source and gain
processing over adjacent 48 kHz stereo blocks, audio-domain enforcement, and nonadvancing failures
for rate, bound, output length, and continuity errors.

`channel_layout_contract.rs` has five public tests covering every common layout through 7.1,
canonical 5.1 order, standardized 5.1-to-stereo coefficients with excluded LFE, explicit discrete
drop and zero-fill behavior, unsupported speaker conversion, validation failure atomicity, and
consecutive prepared-graph sample time.

`timeline_sync_contract.rs` has five public tests covering canonical placement order, allowed
cross-track overlap, rejected same-track overlap and invalid identities or clocks, exact callback
clipping and silence gaps, audio-domain ownership, seek epochs, coordinate limits, a one-hour exact
mapping, real `AudioMasterClock` publication, and zero observed video drift.

`routing_contract.rs` has three public tests. One renders two sources through a dry submix and a
parallel auxiliary send and return into the single master over adjacent 48 kHz stereo blocks. One
uses order-sensitive floating-point values and different edit orders to prove summing order
comes from stable edge identity. One proves duplicate masters, master outputs, wrong direct, send,
and return roles, layout mismatch, and cycles fail without mutating topology.

Together these contracts prove the graph and scheduler coexist without changing exact timing or
channel meaning. Dependent concurrency clock and A/V tests and timeline track-semantics tests guard
the composed contracts. Deterministic local proof does not claim physical hardware latency,
hot-plug behavior, decoded sample binding, engine-composed delivery, or hardware A/V behavior.

Two playback unit tests and ten public output contracts prove typed conversion, backend-default
buffer semantics, capacity and normalized-sample validation, whole-frame backpressure, silence and
telemetry, exact clock progression, persisted locators, domain-conflict degradation, production host
discovery, endpoint thread transfer, and 5,120,000 simulated frames without accumulated drift.

Nine public input contracts prove capability containment, atomic arming and monitoring, exact
sample coordinates across gaps, independent whole-frame pressure, malformed and non-finite input
rejection, domain-conflict timing, stable locators, real host discovery, endpoint thread transfer,
a real monitoring-to-output bridge, and 512,000 simulated frames without accumulated drift.

`clip_mixing_contract` has four public integration tests. It proves swapped channel routing, phase
inversion, bounded gain, exact three-sample fade endpoints across adjacent callback blocks,
hard-pan endpoint exactness, mute, snapshot-wide solo, transactional set/inherit/transfer/remove,
stale revision and partial-batch rejection, invalid semantic routes and numeric controls, fade
duration bounds, and out-of-clip processing rejection through the actual prepared graph processor.

Four resampling contracts prove distinct stereo channel order and settled continuity at 44.1 to 48
kHz, stop-band attenuation at 96 to 48 kHz, exact source and device reports across blocks, rejected
domain, position, and drift inputs without position advance, and positive and negative clock-error
consumption over sustained output.

Four effects contracts prove three-band equalizer response and partition-independent state, linked
compression, an exact peak ceiling, fixed stereo delay coordinates without cross-channel leakage,
bounded odd saturation, finite output, rejected invalid parameters, and non-finite input failure
without graph-clock advancement.

Four metering contracts prove transparent placement in the prepared audio graph, exact stereo
channel identity and sample continuity, sample peak and RMS, coherent phase and spectrum output,
inter-sample true peak, calibrated 400 ms, 3 s, and integrated loudness, history saturation policy,
and rejection of unbounded or nonsensical preparation.

## Current status and risks

The independent audio graph, channel conversion, bus routing, sample-accurate schedule, production
device-input and device-output substrates, clip mix processor, prepared sample-rate converter,
core effects, and graph-native meter are
substantive and publicly test-backed. Plugin hosting remains a documentation-only placeholder.
Engine consumes timeline edit outcomes for atomic clip identity reconciliation, but there is no
decoded-audio fetch, scheduled-slice graph binding, plugin host, platform channel-layout negotiation,
engine playback composition, or end-to-end
source-playback-final-mix path. Microphone permission, physical input latency, semantic input
layout, and hot-plug recovery remain platform-owned boundaries.

Multi-input routing is deliberately unity-only to avoid claiming later control semantics. Prepared
input views retain immutable routes and earlier buffers without self-referential storage or callback
allocation. The schedule iterator is deterministic and allocation-free but scans placements linearly;
a future index must be prepared outside callbacks and preserve exact render order. Caller processors
remain a trust boundary for real-time safety and error atomicity. Physical latency, semantic
channel routing, hot-plug, constrained-device, and soak evidence remain
owned by the platform audio and physical test lanes. Current gain is linear rather than
decibel-addressed, fades are linear only, and pan is the canonical stereo equal-power model.
Effects intentionally omit automation, lookahead, tempo sync, and convolution; those require
separate prepared control and latency contracts.

## Maintenance notes

Preserve the edit-versus-prepare split, fixed schedule epochs, stable identity ordering, exact
sample and channel meanings, fallible preallocation, whole-frame queue admission, explicit capacity
ceiling, timed-silence clock behavior, callback-only atomic telemetry, and failure-only diagnostic
allocation. Preserve direct, send, return, and single-master role validation and stable edge-ordered
summing. Preserve fixed converter lookahead, explicit filter delay, bounded ramped clock correction,
exact dual-clock reports, effect configuration bounds, linked dynamics, channel-local filter and
delay state, and adjacent-block continuity. Preserve transparent meter placement, fixed analysis windows,
bounded atomic publication, explicit programme-history saturation, and control-side integrated
gating. Any indexed extension must define ordering explicitly and prove
callback safety. Keep discovery and stream setup on control threads, and revalidate capabilities
before stream creation.

Preserve capture's independent whole-frame rings, atomic callback-boundary controls, exact physical
sample continuation through dropped intervals, and channel-index meaning. Bridge monitoring into
the existing output producer rather than adding a competing playback path.

When engine integration arrives, adapt immutable timeline and decoded audio owners into the
existing schedule and graph types instead of adding upward dependencies. Keep channel layout and
routing intent attached through that adapter, publish only completed audible windows, and add a
real engine consumer before claiming source playback, mixing, or final delivery.

After source changes, refresh this map's inventory, architecture, invariants, tests, hash, and file
count from resulting behavior, then update consumer maps and validate the global map closure.
