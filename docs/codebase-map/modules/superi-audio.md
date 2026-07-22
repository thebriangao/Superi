---
module_id: superi-audio
source_paths:
  - open/crates/superi-audio
source_hash: 97f1257f7950de13328a53613ed802a65e647bafdc50923015330f79dd499076
source_files: 36
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
fades, pan, mute, solo, phase, and semantic channel mapping with immutable prepared snapshots. A
strict canonical codec preserves that authored state with exact f32 bit patterns, an explicit
format revision, and a SHA-256 payload digest while rejecting unknown or noncanonical input.
Revisioned clip-gain automation owns exact keyframes plus Read, Write, Touch, and Latch behavior,
then prepares immutable curves for absolute-sample evaluation through the existing clip processor.
Its prepared sinc converter maps fixed device-output blocks to exact variable source consumption while
retaining each independent sample clock and smoothly correcting bounded device-rate error.
Common mono, stereo, quad, 5.1, and 7.1 layouts compose the core semantic order, and explicit
prepared speaker or discrete conversion nodes adapt channels without implicit graph routing.
Prepared core effects add cascaded biquad equalization, linked-channel compression and peak
limiting, fixed channel-preserving delay, and normalized soft saturation through the existing
single-input graph processor boundary.
Prepared macOS Audio Unit effect hosting now enters that same processor boundary through exact
FourCC component identity, bounded background-domain preparation, verified process isolation,
explicit stream and channel-layout negotiation, stable pull callbacks, and preallocated planar
render storage. Native failures poison the affected instance, while prevalidation and final output
checks preserve graph continuity and caller-owned output on rejection.
Its transparent prepared meter preserves graph samples while publishing coherent peak, RMS,
true-peak, phase, spectrum, momentary, short-term, and integrated loudness snapshots through
bounded lock-free storage.

Its VST3 worker host loads one explicit audio-effect class on macOS, Windows, or Linux, negotiates
one exact canonical main input and output, and exposes the prepared plugin as the existing graph's
single-input `AudioProcessor`. Preallocated planar f32 buffers preserve channel order, exact
`SampleTime` drives the VST3 process context, bounded sample-offset automation crosses into the
callback, and bounded output-parameter monitoring crosses back to control-side readings. Audio Unit
class-info property lists and VST3 component and controller streams now round-trip through one
format-neutral, versioned, digest-checked state envelope that also records the exact sample clock and
native plus transport latency evidence.

Graph preparation now computes cumulative processor latency and inserts one preallocated delay for
each faster incoming direct, send, or auxiliary-return route. The format-neutral isolated-process
bridge advances the same prepared dry delay on every block, publishes wet output only after complete
finite validation, and falls back to timing-matched dry audio when a worker is missing or faults.

Its capture slice discovers input devices, validates exact stream configurations, and publishes
channel-indexed recorded samples at exact physical input coordinates through one preallocated
ring. An independent preallocated monitoring ring preserves normalized interleaved samples for the
existing output path. Atomic arming and monitoring controls take effect at callback boundaries.

All processing paths enforce the platform-owned audio execution domain, but their control-path owners
remain separate. Graph editing and preparation allocate outside callbacks. Schedule construction and
transport reanchoring also occur outside callbacks. Resampler and meter construction and scratch
allocation remain outside callbacks. Device discovery and stream setup remain on control threads.
Decoded sample binding to the real prepared graph, automation persistence, broader effect and
plugin automation, concrete platform worker transport, Audio Unit instruments, and complete
timeline audio-graph orchestration remain separate concerns. Engine foreground playback
feeds the existing
bounded output producer and paces video from its paired presentation clock. Engine transport
requests queued-audio discard through an atomic generation handshake without moving queue ownership
or device callback work into the engine. Engine render-export now invokes a caller-owned audio
processing stage, records its `AudioGraphId`, and validates exact returned block semantics before
encoding, but it does not yet adapt decoded blocks into `PreparedAudioGraph`.

## Source inventory

- `open/crates/superi-audio/Cargo.toml`: Declares dependencies on `superi-core`,
  `superi-concurrency`, exact CPAL 0.17.3, ringbuf 0.4.8, default-feature-free rubato 0.16.2,
  serde, serde_json, sha2, exact VST3 0.3.0, libloading 0.8.9, plus macOS-only `block2`,
  AudioToolbox, Core Audio type, and Core Foundation bindings for the private native hosts.
- `open/crates/superi-audio/src/automation.rs`: Implements bounded revisioned clip-gain lanes,
  exact keyframes, Read, Write, Touch, and Latch state, half-open region replacement, immutable
  snapshots, and prepared allocation-free absolute-sample curve evaluation.
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
- `open/crates/superi-audio/src/hosting.rs`: Defines exact Audio Unit effect identity, isolation
  policy, bounded immutable host configuration, background-domain preparation, native diagnostics,
  and the safe prepared graph-processor surface, and exports the worker-side VST3 host.
- `open/crates/superi-audio/src/hosting/audio_unit_macos.rs`: Owns the private audited macOS
  AudioToolbox lifecycle, asynchronous instance transfer, exact property and channel negotiation,
  class-info property-list state restoration and capture, latency reporting, pull callback,
  preallocated planar rendering, output validation, poisoning, and teardown.
- `open/crates/superi-audio/src/hosting/vst3.rs`: Implements strict class identity, exact canonical
  speaker mapping, bounded configuration, metadata, automation and monitoring handoffs, telemetry,
  initial-state restoration, exact captured component and controller state, lease-gated worker
  session ownership, and the prepared graph processor.
- `open/crates/superi-audio/src/hosting/vst3/native.rs`: Contains the private audited platform
  loader, VST3 COM ownership, bounded host messages and attributes, fixed parameter queues, bus
  negotiation, bounded seekable `IBStream` state transfer, planar process bridge, and reverse
  lifecycle.
- `open/crates/superi-audio/src/lib.rs`: Documents the implemented automation, graph, channel,
  routing, scheduling, capture, playback, conversion, effects, hosting, plugin-state, isolated
  bridge, and metering boundaries and exports the fourteen audio concern modules.
- `open/crates/superi-audio/src/metering.rs`: Implements transparent prepared graph metering,
  per-channel sample peak, RMS and true peak, stereo phase correlation, bounded spectrum analysis,
  K-weighted EBU windows, ITU programme gating, coherent lock-free snapshots, and explicit history
  saturation.
- `open/crates/superi-audio/src/mixing.rs`: Implements validated clip controls, semantic channel
  matrices, revisioned identity mutations, immutable solo-aware snapshots, bounded clip
  preparation, and allocation-free gain, fade, pan, mute, solo, phase, and routing DSP.
- `open/crates/superi-audio/src/playback.rs`: Implements stable device identity, capability
  discovery, exact configuration validation, bounded producer and callback endpoints, sample
  conversion, producer-requested and callback-applied queue discontinuities, audio-clock
  publication, atomic telemetry, and production stream lifecycle.
- `open/crates/superi-audio/src/plugins.rs`: Defines bounded format-neutral plugin identity,
  versioned digest-checked component and controller state, sample-rate and latency evidence, the
  isolated process bridge contract, lock-free runtime telemetry, and a prepared timing-matched dry
  fallback processor.
- `open/crates/superi-audio/src/resample.rs`: Implements prepared fixed-output band-limited
  conversion, ordered interleaved and planar transfer, exact dual-clock accounting, sinc delay
  reporting, and bounded ramped device-clock correction.
- `open/crates/superi-audio/src/serialize.rs`: Implements the canonical revisioned clip-mix codec,
  exact float-bit representation, payload digest verification, strict structural bounds, and
  byte-canonical decoding.
- `open/crates/superi-audio/src/routing.rs`: Implements allocation-free unity summing for prepared
  submix, auxiliary, and master buses in stable route-identity order with non-finite rejection.
- `open/crates/superi-audio/src/sync.rs`: Implements exact timeline placements, immutable schedule
  validation, transport epochs, callback-window mapping, lazy source slices, and audio-master
  publication.
- `open/crates/superi-audio/tests/audio_graph_contract.rs`: Proves graph topology, validation,
  preparation, exact bounded processing, continuity, channel order, and domain ownership.
- `open/crates/superi-audio/tests/audio_delay_compensation_contract.rs`: Proves fixed processor
  latency propagation, dry and auxiliary route alignment, partition independence, exact graph
  latency diagnostics, and fallible compensation storage preparation.
- `open/crates/superi-audio/tests/audio_plugin_runtime_contract.rs`: Proves exact native-state
  round trips, corruption and bound rejection, worker fault telemetry, and timing-matched dry
  fallback through the prepared isolated bridge processor.
- `open/crates/superi-audio/tests/audio_automation_contract.rs`: Proves revisioned exact keyframes,
  Read, Write, Touch, and Latch region semantics, invalid and overflow atomicity, and
  partition-independent automated gain through a real source, clip, submix, and master graph.
- `open/crates/superi-audio/tests/channel_layout_contract.rs`: Proves common semantic order,
  documented speaker coefficients, discrete copy, drop, and zero-fill behavior, fail-closed
  validation, and exact consecutive sample time through an explicit prepared graph node.
- `open/crates/superi-audio/tests/timeline_sync_contract.rs`: Proves canonical schedule order,
  callback ownership, exact clipping and silence gaps, seek epochs, overflow and clock rejection,
  long-duration timing, audio-master publication, and zero video drift.
- `open/crates/superi-audio/tests/device_output_contract.rs`: Proves capability containment, bounded
  whole-frame admission, allocation ceilings, timed silence, clock progression, persisted device
  identities, domain-conflict behavior, atomic discard coalescing and recovery, telemetry, and real
  host discovery.
- `open/crates/superi-audio/tests/device_input_contract.rs`: Proves capability containment, atomic
  arming and monitoring, exact timing through gaps, independent whole-frame backpressure, malformed
  callback rejection, domain-conflict behavior, stable locators, real discovery, long-session
  drift freedom, and a real monitoring bridge into bounded output playback.
- `open/crates/superi-audio/tests/clip_mixing_contract.rs`: Public consumer proof for every clip
  control, exact multi-block envelopes, snapshot solo behavior, atomic identity mutations, invalid
  layouts and values, clip bounds, and failure atomicity.
- `open/crates/superi-audio/tests/clip_mix_serialization_contract.rs`: Proves exact authored-state
  round trips, deterministic re-encoding, exact route order, corruption and unknown-field
  rejection, and noncanonical input rejection.
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
- `open/crates/superi-audio/tests/audio_unit_host_contract.rs`: Proves bounded safe configuration,
  background ownership, real Apple Peak Limiter execution through source, effect, and master nodes,
  adjacent-partition continuity, exact timing failure atomicity, and verified out-of-process loading.
- `open/crates/superi-audio/tests/fixtures/vst3_gain.rs`: Builds as a real temporary VST3 dynamic
  module with gain automation, output parameter changes, process-context observation, callback
  allocation counting, canonical layout negotiation, and lifecycle evidence.
- `open/crates/superi-audio/tests/vst3_host_contract.rs`: Proves strict safe values, failed loading,
  parent-side native isolation, every supported layout, exact automation and monitoring, real-time
  and offline process context, source-to-master routing, and explicit reverse shutdown.

## Public surface

The crate root exports `automation`, `capture`, `channels`, `effects`, `graph`, `hosting`,
`metering`, `mixing`, `playback`, `plugins`, `resample`, `routing`, `serialize`, and `sync`. Every exported
module contains substantive behavior.

`automation` exposes typed `AudioAutomationTarget` addresses, validated exact keyframes,
professional `AudioAutomationMode` values, bounded ordered mutations and transactions,
revisioned `AudioAutomationState`, immutable complete snapshots, and
`PreparedAudioAutomationCurve`. State supports no-op suppression and atomic candidate validation.
Write replaces the full played pass, Touch replaces only physical-touch regions, and Latch holds
the most recent touched value through the exact pass end. Prepared curves interpolate finite linear
gain from absolute signed sample coordinates without callback mutation or allocation.

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

`hosting` exposes safe `AudioUnitComponentId`, `AudioUnitExecutionPolicy`, `AudioUnitHostConfig`,
and `PreparedAudioUnit` values. Effects bind one exact component, integral sample clock, maximum
slice, and equal ordered input and output layout. Out-of-process loading is the default and must be
confirmed from the native instance; in-process loading requires explicit caller audit. A prepared
instance exposes component version, actual process location, poison state, native latency, bounded
state capture, and ordinary graph processor behavior without exposing native handles or callback
storage. Optional initial state is restored before initialization through the native class-info
property.

`hosting::vst3` exposes `Vst3ClassId`, `Vst3EffectConfig`, `Vst3ProcessMode`, immutable effect and
parameter metadata, exact `Vst3AutomationPoint`, `Vst3AutomationWriter`, output parameter points,
control-side readings and telemetry, `Vst3WorkerSession`, and `PreparedVst3WorkerEffect`. Loading
accepts one explicit module and class inside a dedicated plugin worker. The prepared effect supports
only one canonical mono, stereo, quad, 5.1, or 7.1 main input and output with f32 processing.
Only automatable writable parameters enter the input handoff. Latency and tail are reported without
applying compensation inside the host, because the graph applies route compensation. Optional
component and controller state is restored in VST3-defined order before activation and captured
through bounded seekable streams off the audio callback. Raw VST3 and platform types remain private.

Configuration rejects more than 1,048,576 planar samples or points in either bounded handoff,
per-block automation larger than its total queue, more than 4,096 controller parameters, or more
than 1,048,576 combined fixed parameter-point cells before allocating the prepared graph node.

`plugins` exposes `AudioPluginFormat`, exact bounded `AudioPluginIdentity`, `AudioPluginState`,
`IsolatedAudioPluginProcessBridge`, `PreparedIsolatedAudioPlugin`, and atomic runtime readings. The
binary state envelope binds exact native bytes to identity, schema, sample rate, fixed native and
transport latency, and a SHA-256 digest. The prepared bridge processor requires wet output to arrive
already aligned to its declared transport delay and otherwise advances and publishes the same
timing-matched dry path without blocking or allocating.

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
resulting `PreparedAudioGraph` exposes stable prepared input routes, per-node and destination
latency diagnostics, and processes exact `SampleTime`, bounded frames, and caller-owned output.
Preparation computes every cumulative processor latency and preallocates route delay rings plus
scratch for faster branches. `AudioProcessInputs` lazily yields borrowed current-block samples,
source identities, route identities, layouts, and compensation-adjusted buffers without allocation.

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
`OutputDiscardStatus`, clonable telemetry, and an owning `DeviceOutput`. The producer requests an
exact discard generation and exposes pending status; sample admission remains blocked until the
consumer applies that generation. `discover_output_devices` performs real host enumeration,
`create_output_buffer` preallocates the engine-to-device handoff, and
`start_device_output` revalidates and starts the selected stream.

`mixing` exposes `ChannelMap`, complete inspectable `ClipMixControls`, revisioned `ClipMixState`,
identity-preserving `ClipMixMutation` values, immutable `ClipMixSnapshot`, and prepared
`ClipMixProcessor`. State mutations set, inherit, transfer, or remove complete intent atomically.
Preparation binds one clip identity to an exact `SampleTime` interval and fixed layouts.
`prepare_processor_with_automation` explicitly binds a matching prepared clip-gain curve and leaves
the existing fixed-gain preparation method unchanged when automation is absent.

`serialize` exposes `serialize_clip_mix_state` and `deserialize_clip_mix_state`. The codec emits one
canonical `superi.clip-mix` revision 1 JSON envelope, stores every f32 as its exact bit pattern,
binds the ordered payload to a lowercase SHA-256 digest, reconstructs state through checked public
invariants, applies matching bounds to encoding and decoding, and rejects duplicate identities,
unknown fields, over-limit payloads, and alternate encodings of otherwise equivalent content.

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
fallibly reserves one maximum-sized interleaved f32 buffer per node. The same pass adds each
processor's fixed `latency_samples`, derives cumulative output arrival per node, and preallocates a
ring plus maximum-block scratch for the exact difference between every incoming route and its
destination's slowest branch.

Prepared processing requires `ExecutionDomain::Audio`, then validates rate, positive bounded frame
count, exact output length, coordinate overflow, and continuity with the prior successful block.
Each processor reads earlier current-block buffers through a borrowed input view and writes its own
prepared buffer. A compensated route returns the delayed prepared scratch instead of the earlier
node buffer, while a zero-delay route remains borrowed directly. `SummingBus` performs deterministic
unity addition without callback allocation. The destination is copied into caller-owned output,
and continuity advances only after full success.

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
interleaved frame submissions or rejects them whole. A control-side discontinuity increments one
atomic requested generation and blocks new admission. At the next valid callback, the sole consumer
clears its ring, publishes the applied generation, and then renders only post-acknowledgement samples
or silence. Multiple pending requests coalesce at the latest observed generation, and no producer
can admit new-epoch samples that the same acknowledgement would clear. The platform callback enters
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

Automation mutation stays on a control owner. One ordered transaction applies to a cloned bounded
candidate, validates complete lane and keyframe capacity, and advances one revision only for a
semantic change. Active passes retain their baseline curve and exact half-open write regions;
completed publication inserts boundary points that preserve untouched interpolation immediately
before and after every replacement. Snapshot preparation validates the lane clock once and copies
effective points into immutable storage. `ClipMixProcessor` then chooses the prepared automated gain
or its existing fixed gain for each absolute frame before applying unchanged fades, phase, pan,
mute, solo, channel mapping, and graph routing.

Clip-mix serialization is a control-path operation over authored state only. Encoding walks stable
clip identity order, represents all float values as exact bits, hashes the canonical payload, and
then emits the strict envelope. Decoding validates byte and entry ceilings, the envelope and
payload revisions, digest, uniqueness, semantic controls, and exact canonical re-encoding before
publishing a reconstructed `ClipMixState`; prepared processors and callback state never cross this
boundary.

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

Audio Unit preparation discovers one exact effect component, transfers asynchronous instantiation
ownership through a bounded completion wait, verifies the component and actual process location,
sets and reads back maximum frames, planar native-float stream formats, and semantic channel
layouts, restores any bounded class-info property list, registers one stable pull callback, reads
native latency, then initializes the unit. Control-side state capture copies and serializes the
current class-info property list without entering the audio callback. Processing validates the
complete block and exactly representable sample window before deinterleaving into prepared planes.
The callback serves only bounded integral subranges from those planes. After synchronous native
rendering, the host checks callback status, output buffer identity and shape, silence signaling, and
finite samples before interleaving into caller output. Destruction uninitializes and disposes the
instance before releasing callback or plane storage.

Meter preparation validates the exact sample clock, ordered channel layout, callback bound,
spectrum dimensions, and integrated-history ceiling before fallibly allocating all DSP and atomic
publication storage. The audio callback copies its sole input unchanged, then updates K-weighting,
four-phase true-peak interpolation, rolling loudness and spectrum windows, and one seqlock-protected
atomic snapshot. The control-side reader retries concurrent publication, constructs owned channel
and spectrum values, and performs the two-stage integrated loudness gate without blocking audio.

VST3 preparation retains the native module, factory, component, processor, optional controller,
host callbacks, parameter queues, planar sample storage, and immutable metadata before publishing a
graph node. Before activation it restores bounded component state, forwards that component state to
the controller, and restores bounded controller state. Control-side capture writes both streams into
owned bounded seekable buffers. It rejects unrepresentable bus topology, event buses, unsupported
semantic speaker arrangements, and non-f32 processing. The callback validates domain, rate, layout, extent,
continuity, and finite input, translates exact absolute points in the current half-open block to
sample offsets, performs one native process call, and maps finite planar output plus output points
back to graph order and absolute time. Bounded host messages and attributes serve control-side
communication but reject audio-domain access, while unsupported optional process-context demands
fail setup instead of receiving guessed values. Output monitoring visits each bounded fixed queue
and point once after processing. Shutdown succeeds only after the graph lease is retired and
reverses processing, component, each bus and connection direction, controller, factory,
module-entry, and loader ownership. A failed reverse call leaves unresolved owners and mapped code
retained for retry or worker-process exit.

Format-neutral plugin state encoding stays off the callback and rejects oversized identity fields,
individual state streams above 32 MiB, combined native state above the bounded total, invalid sample
rates, structural truncation or trailing bytes, and digest mismatch before constructing state. The
isolated process processor preallocates one interleaved dry-delay ring and wet output buffer. Each
audio block advances dry timing regardless of worker status, accepts only complete finite aligned
wet output from the bridge, and records missing, faulted, or malformed worker output before copying
the matching delayed dry block to caller output.

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
- serde and serde_json provide the strict bounded clip-mix wire shape, and sha2 provides the clip-mix
  and binary plugin-state payload digests. Exact-float and canonical-byte policy remains owned by
  `superi-audio`.
- macOS-only `block2`, `objc2-audio-toolbox`, and `objc2-core-audio-types` provide generated block,
  AudioToolbox, and Core Audio type bindings. `objc2-core-foundation` also supplies bounded property
  list, data, and error ownership for Audio Unit state. The private host retains lifecycle, pointer,
  callback, process-isolation, state, latency, and semantic-layout policy under the repository unsafe
  inventory.
- `vst3` 0.3.0 and its `com-scrape-types` 0.1.1 support crate provide low-level generated COM
  bindings under `MIT OR Apache-2.0`. libloading retains Windows and Linux modules, while
  objc2-core-foundation retains macOS VST3 bundles. These dependencies remain behind the private
  worker-native boundary.
- `superi-engine::audio_mix` owns production timeline edit and clip-mix identity reconciliation.
  No production adapter yet binds decoded media into the schedule or prepared graph.
- `superi-engine::audio_plugins` owns deterministic VST3 bundle and caller-supplied Audio Unit
  candidate discovery, strict worker contracts, lifecycle supervision, recovery and quarantine,
  per-instance project records, and prepared isolated bridge construction. Concrete platform worker
  launchers remain adapters outside this crate.
- `superi-engine::dispatcher` owns optional lifecycle-scoped automation state, serialized
  inspection and transaction execution, dynamic no-op event reservation, and complete replacement
  events. `superi-api` projects that engine boundary without a direct dependency on this crate.
- `app/src-tauri/src/capabilities.rs` directly consumes only input and output device enumeration for
  a strict System-panel observation. It preserves default configurations, channel counts, exact
  sample-rate ranges, sample formats, buffer constraints, skipped-device evidence, and explicit
  channel-layout knowledge without creating, starting, playing, pausing, discarding, routing, or
  reconfiguring a stream.
- The production editing workspace consumes the attached public automation replacement as read-only
  clip detail. It correlates only exact `clip_gain` targets, signed sample positions, sample rates,
  finite values, mode, and active-pass state, and adds no automation mutation or prepared curve.
- `superi-project` owns clip-mix state inside the durable project aggregate and stores the canonical
  codec bytes as the singleton schema-4 audio component. Engine project command history restores
  authored clip-mix snapshots, while audio device, callback, meter, resampler, and prepared graph
  state remain operational and absent from persistence and history.
- `superi-project` extension records also store one exact bounded plugin-state envelope per audio
  node instance. Runtime readiness, native handles, delay rings, worker telemetry, and quarantine
  remain operational engine or audio state and are never serialized into those records.
- `superi-engine::project_transaction` composes timeline edits, graph mutations, media commands,
  root selection, and clip-mix mutations into one bounded project transaction and one history entry.
- `superi-engine::playback` wraps the existing `OutputProducer`, accepts only whole borrowed sample
  submissions, and passes the paired `AudioMasterClock` into its engine-owned A/V coordinator as
  the authoritative video pacing source. That coordinator returns bounded wait, correction, drop,
  and discontinuity recovery evidence and preserves continuity through monotonic fallback. The
  device callback and `OutputConsumer` remain audio-owned, and engine never changes audio samples or
  the physical counter.
- `superi-engine::export_queue` exposes an `ExportAudioGraph` seam whose result retains the
  audio-owned `AudioGraphId`. It sends each decoded block through that caller-owned stage, validates
  exact timestamp, duration, metadata, sample precision, rate, and channel layout, and then encodes
  the returned block. The current production PCM lane proves source, codec, and stage orchestration,
  but the stage implementation in that contract is not a `PreparedAudioGraph` adapter.
- `superi-engine::transport` requests producer-side discard generations for seek, scrub, step,
  rate, direction, resume, and loop discontinuities, and exposes pending acknowledgement. It admits
  no inactive samples and mutes non-normal or reverse samples because this queue does not own
  timestamped rate conversion.
- `superi-timeline` remains upstream through future engine composition rather than a direct Rust
  dependency. Its sample-exact placements, track order, channel layout, and routing intent are
  adapter inputs.
- `superi-media-io` remains the decoded sample owner and is not a direct dependency. No production
  decoder currently feeds a prepared graph from scheduled slices.
- The public integration contracts, engine foreground playback contract, and engine
  render-export audio-stage contract are current real consumers. They process exact adjacent
  blocks through clip DSP, dry, auxiliary, submix, and master paths, publish scheduled presentation
  through actual concurrency clocks, exercise bounded device output, coordinate real foreground
  video against that clock under normal, late, discontinuous, and recovered conditions, and prove clip identity
  inheritance while converting between independent source and device clocks and applying core
  effects through the prepared graph. The metering contract places a real meter between a source
  and master bus. The Audio Unit contract places Apple's Peak Limiter between a deterministic source
  and the terminal master. Input proof
  additionally exercises exact channel-indexed capture and routes monitoring samples through the
  production bounded output handoff. The VST3 contract adds isolated real-module proof through a
  source, hosted effect, submix, and master without exposing an editor-process load path. New delay,
  runtime bridge, and engine supervision contracts prove aligned routing, fallback timing, restart,
  quarantine, compatible-version state restoration, and save-reopen identity preservation.

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
- Every processor declares one fixed nonnegative latency at preparation. Each node's cumulative
  output latency is the slowest incoming arrival plus that processor latency, and every faster route
  receives exactly the difference. Compensation storage is fully allocated before publication, and
  impossible capacity fails preparation instead of weakening alignment.
- Every block has one exact integral sample clock and must follow the prior successful block
  without a gap or overlap. Failed prevalidation does not advance continuity.
- The graph-owned successful process path takes no lock, allocates no memory, and frees no memory.
  `AudioProcessor` implementers receive the same explicit contract, but the graph cannot prove the
  internals of caller-supplied processor code.
- Processor failure may retain processor-internal partial state; callers must treat processor error
  recovery according to that processor's contract. Graph continuity advances only on full success.
- Audio Unit native code is confined to one macOS-only private module with a narrow unsafe allowance.
  Public configuration and processor surfaces remain safe Rust values, no raw handle escapes, and
  required out-of-process loading is verified rather than inferred. VST3 remains in its separate
  private worker-native module.
- Audio Unit preparation requires the background domain and processing requires the audio domain.
  The maximum slice, sample rate, ordered channel layout, component identity, and process-location
  policy are immutable after preparation.
- Audio Unit state capture and restoration use only bounded Core Foundation property-list bytes on
  the background domain. Native latency is converted to integral samples once during preparation
  and becomes the processor's fixed graph latency.
- The successful Audio Unit process path takes no host lock, allocation, or free. It supports
  repeated bounded native pulls, commits only complete finite output, and poisons an instance after
  native entry, callback, buffer-contract, or output failure. Prevalidation does not poison or
  mutate graph continuity.
- VST3 native code exists only in the inventoried private worker boundary. Module handles outlive
  copied symbols and all COM objects; the session cannot unload while a prepared effect lease
  remains. An unretired lease intentionally keeps the module mapped until worker exit.
- VST3 accepts exactly one main input and output with equal canonical channel meaning, f32 samples,
  and no event buses. Unsupported topology fails instead of dropping buses or changing layout.
- VST3 state restoration orders component `setState`, controller `setComponentState`, then
  controller `setState`. Bounded `IBStream` reads and writes preserve exact bytes and report short
  transfers. State capture and restore remain outside the audio callback, and native latency becomes
  the processor's fixed graph latency.
- VST3 automation is admitted as one validated nondecreasing slice and maps exact absolute time to
  block-local offsets. Only automatable writable parameter IDs are accepted. One future point
  remains queued for the next block. Monitoring retains exact absolute time, including read-only
  output parameters, and reports bounded overflow.
- The successful VST3 callback uses only prepared buffers, fixed queues, atomics, and one native
  process call. It performs no allocation, free, lock, wait, I/O, controller call, lifecycle call,
  or module lookup. Failed or nonfinite plugin output is cleared before graph publication.
- Plugin identity fields are bounded, and compatibility across an installed upgrade requires the
  same format and component identifier. The durable state envelope retains the saved vendor and
  version as evidence, while its digest, schema, clock, native latency, transport latency, and exact
  component and controller bytes must all validate before use.
- The isolated bridge contract requires a worker process, not an in-process adapter. Its successful
  audio callback never blocks or allocates, always advances dry delay, and replaces wet output with
  the timing-matched delayed dry block on worker absence, fault, malformed extent, or nonfinite data.
- The output queue is nonzero, checked for overflow, capped at 1,048,576 samples, and admits only
  finite normalized complete frames. The callback takes no blocking lock and grows no storage.
- Output discard is requested only by the producer and applied only by the consumer. Admission is
  rejected while requested and applied generations differ. The callback clears preallocated ring
  state and publishes acknowledgement without allocation or locking; an already presented hardware
  buffer cannot be recalled.
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
- Current audio remains offline and open-tree only. Native device I/O is isolated behind CPAL, and
  native VST3 execution is restricted to the explicit plugin-worker boundary.
- Clip mix publication is revision checked and atomic. Complete controls follow a right fragment,
  transfer to a replacement, and disappear with a removed clip through engine-owned reconciliation.
- Project undo and redo must not snapshot callback queues, physical clocks, device handles,
  telemetry, prepared DSP state, or other operational audio state. Later compound authored edits
  may include durable clip-mix intent only through an engine transaction that preserves both
  timeline and audio revision fences.
- Nonzero pan requires canonical stereo output. Gain and route coefficients are finite and bounded;
  fades use exact integer sample lengths and must fit the prepared clip interval.
- Automation transactions contain one to 64 ordered mutations, retain at most 4096 lanes and
  1,048,576 keyframes, use one exact revision fence, and publish atomically. Gains are finite in
  `0..=64`, lane clocks are positive and fixed, pass time never moves backward, and writable
  `i64::MAX` coordinates fail because no exclusive region end can be represented.
- Read accepts direct authored keyframes but no write pass. Write replaces the complete half-open
  pass, Touch replaces only touched intervals and resumes the baseline at release, and Latch holds
  after release through the pass end. Untouched regions retain the original curve exactly.
- Prepared automation is immutable and clock checked. Callback evaluation uses absolute sample
  coordinates, performs no allocation, locking, dispatch, or state mutation, and is invariant to
  block partitioning. A missing lane preserves existing fixed clip gain exactly.
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

`audio_delay_compensation_contract.rs` has three public tests. They prove fixed-latency parallel
impulses align at the same destination sample, whole and partitioned execution agree, dry plus typed
auxiliary-return routing stays aligned through submix and master, cumulative latency diagnostics are
exact, and an unrepresentable compensation allocation fails preparation.

`audio_plugin_runtime_contract.rs` has two public tests. They prove exact component and controller
state bytes, clock and latency evidence, deterministic re-encoding, corruption and oversize
rejection, and timing-matched delayed dry fallback with atomic worker-fault telemetry.

Together these contracts prove the graph and scheduler coexist without changing exact timing or
channel meaning. Dependent concurrency clock and A/V tests, the engine coordinator and foreground
playback contracts, and timeline track-semantics tests guard the composed contracts. The engine
consumer proves actual sample-clock video decisions and exact timing preservation, but deterministic
local proof does not claim physical hardware latency, hot-plug behavior, prepared graph delivery, or
hardware A/V behavior. Engine export adds real acquired PCM decode and encode around an explicit test
audio stage, not a binding into this crate's prepared graph.

Two playback unit tests and eleven public output contracts prove typed conversion, backend-default
buffer semantics, capacity and normalized-sample validation, whole-frame backpressure, silence and
telemetry, two-generation discard coalescing, pending producer rejection, callback-owned clearing,
post-acknowledgement recovery, exact clock progression, persisted locators, domain-conflict
degradation, production host discovery, endpoint thread transfer, and 5,120,000 simulated frames
without accumulated drift.

Nine public input contracts prove capability containment, atomic arming and monitoring, exact
sample coordinates across gaps, independent whole-frame pressure, malformed and non-finite input
rejection, domain-conflict timing, stable locators, real host discovery, endpoint thread transfer,
a real monitoring-to-output bridge, and 512,000 simulated frames without accumulated drift.

`clip_mixing_contract` has four public integration tests. It proves swapped channel routing, phase
inversion, bounded gain, exact three-sample fade endpoints across adjacent callback blocks,
hard-pan endpoint exactness, mute, snapshot-wide solo, transactional set/inherit/transfer/remove,
stale revision and partial-batch rejection, invalid semantic routes and numeric controls, fade
duration bounds, and out-of-clip processing rejection through the actual prepared graph processor.

`clip_mix_serialization_contract` has two public integration tests. It proves exact float and
revision round trips, deterministic re-encoding, exact route order, digest and strict-schema
rejection, and byte-canonical enforcement. Project migration contracts separately exercise the
canonical empty state used by schema upgrades.

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

Four automation contracts prove exact negative and positive keyframes, interpolation, semantic
no-ops, stale and invalid rollback, wrong clocks, maximum-coordinate rejection, exact Write, Touch,
and Latch boundaries, active-pass snapshots, and whole-versus-split equivalence. The routed proof
renders finite audible stereo samples through source, automated clip, submix, and master nodes.

Four private unit tests cover typed playback conversion plus Audio Unit callback storage copying,
host-plane publication, repeated bounded pulls, malformed native requests, and failure reporting.
Seven public Audio Unit host contracts cover component identity and bounds, background-domain
preparation, a real Apple Peak Limiter through the prepared graph and terminal master, adjacent
partition continuity, finite audible stereo output, exact sample-time rejection without graph or
output commit, required out-of-process verification from the instantiated component, class-info
state capture and restore, and native latency publication.

The VST3 contract has six ordinary tests plus one ignored child entry invoked only by its parent.
It compiles a standards-shaped temporary dynamic module, proves the parent never loads it, and runs
five successful isolated layout cases plus unsupported-context, partial-activation, and failed-stop
cases. The child proves exact
source, hosted effect, submix, and master output; in-block and block-boundary automation; output
monitoring and overflow; metadata, process context, real-time and offline modes; host message and
attribute support; read-only automation and optional-context rejection; zero fixture callback
allocations; load failure; lease conflict; partial unwind; failed-stop module retention; reverse
lifecycle order; exact component and controller state traffic; restored initial state; captured
state; fixed latency publication; and module exit.

## Current status and risks

The production desktop shell now exposes a read-only observation of current input and output
declarations through the strict four-domain capability snapshot. This improves operational
visibility only. Enumeration does not prove future stream creation, semantic channel layout,
physical latency, routing, synchronization, or audible output, and retained observations may be
stale after hotplug until refresh.

The independent audio graph, channel conversion, bus routing, sample-accurate schedule, production
device-input and device-output substrates, durable clip-mix codec, clip mix processor, prepared sample-rate converter,
revisioned clip-gain automation, core effects, graph-native meter, macOS Audio Unit effect host, and
worker-side VST3 effect host are substantive and publicly test-backed. Fixed graph delay
compensation, exact native plugin state envelopes, Audio Unit class-info persistence, VST3 component
and controller persistence, and a timing-matched isolated bridge fallback are also substantive. The
VST3 subset is canonical single-main-bus processing exercised through the graph master. Audio Unit
instruments, MIDI, broader effect automation, preset browsing, plug-in UI, dynamic latency rebuild,
and concrete cross-process transport remain absent.
The production timeline surface now presents attached clip-gain modes, exact keyframe samples, and
clip-relative keyframe diamond positions, but this remains observation only and does not make
automation durable project state.
Engine consumes timeline edit outcomes for atomic clip identity reconciliation and foreground
playback feeds the bounded device producer while coordinating video from the actual audio clock.
That foreground path now exposes bounded video correction and applied discontinuity recovery without
mutating audio timing, and transport uses the callback-owned discard handshake for control
discontinuities. Engine export now fetches decoded audio, invokes an explicit graph-stage seam,
preserves its graph identity, and completes an exact in-tree PCM encode. There is still no
scheduled-slice `PreparedAudioGraph` binding, concrete platform plugin worker launcher, production
shared-memory transport, heartbeat and kill integration, or end-to-end decoded source playback and
final-mix delivery path. Engine now owns format-neutral discovery, validation, supervision, restart,
quarantine, state checkpointing, and per-node project records, while audio owns the prepared bridge
and fixed graph compensation. Audio Unit hosting performs exact macOS channel-layout
negotiation for a configured effect, but device-level semantic layout remains separate. Microphone
permission, physical input latency, semantic input layout, and hot-plug recovery remain
platform-owned boundaries.

Engine now owns bounded project-level history and a compound transaction that includes authored
audio intent. Undo and redo restore the exact clip-mix state through project snapshots, and save and
reopen preserve it through the canonical codec without capturing operational audio resources.

Multi-input routing is deliberately unity-only to avoid claiming later control semantics. Prepared
input views retain immutable routes and earlier buffers without self-referential storage or callback
allocation. The schedule iterator is deterministic and allocation-free but scans placements linearly;
a future index must be prepared outside callbacks and preserve exact render order. Caller processors
remain a trust boundary for real-time safety and error atomicity. Physical latency, semantic
channel routing, hot-plug, constrained-device, and soak evidence remain
owned by the platform audio and physical test lanes. Current gain is linear rather than
decibel-addressed, fades are linear only, and pan is the canonical stereo equal-power model.
Effects intentionally omit parameter automation, lookahead, tempo sync, and convolution; those
require separate prepared control and latency contracts. Current automation addresses clip linear
gain only and is not yet stored in project snapshots.
Third-party Audio Units remain native code with vendor-defined behavior. Default out-of-process
loading contains process failure, and engine supervision defines restart and quarantine policy, but
Audio Unit registry enumeration, concrete process launch and heartbeat control, parameter metadata,
UI, and broad compatibility policy remain external. Deterministic proof currently uses Apple's Peak
Limiter rather than a physical third-party test matrix.

## Maintenance notes

Preserve the edit-versus-prepare split, fixed schedule epochs, stable identity ordering, exact
sample and channel meanings, fallible preallocation, whole-frame queue admission, explicit capacity
ceiling, timed-silence clock behavior, callback-only atomic telemetry, and failure-only diagnostic
allocation. Preserve sole-writer discard ownership, pending admission rejection, acquire and release
generation ordering, and callback-only queue clearing. Preserve direct, send, return, and
single-master role validation and stable edge-ordered
summing. Preserve fixed converter lookahead, explicit filter delay, bounded ramped clock correction,
exact dual-clock reports, effect configuration bounds, linked dynamics, channel-local filter and
delay state, and adjacent-block continuity. Preserve transparent meter placement, fixed analysis windows,
bounded atomic publication, explicit programme-history saturation, and control-side integrated
gating. Any indexed extension must define ordering explicitly and prove
callback safety. Keep discovery and stream setup on control threads, and revalidate capabilities
before stream creation.
Keep the desktop capability projection synchronized with public device declarations. Preserve exact
sample-rate ranges, sample formats, buffer constraints, defaults, skipped-device evidence, and
explicit unknown channel meaning, and never let its refresh enter stream or routing ownership.

Preserve Audio Unit background preparation, default verified isolation, exact identity and property
readback, stable callback lifetime, fixed planar storage, bounded repeated pulls, caller-output
commit after complete validation, poison-on-native-failure behavior, and teardown before callback
storage release. New native calls, properties, plug-in classes, or callback forms require matching
unsafe inventory and real macOS lifecycle proof.

Preserve capture's independent whole-frame rings, atomic callback-boundary controls, exact physical
sample continuation through dropped intervals, and channel-index meaning. Bridge monitoring into
the existing output producer rather than adding a competing playback path.

Preserve VST3's worker-only load boundary, retained module and COM ordering, exact speaker masks,
single-main-bus rejection policy, preallocated planar and parameter storage, half-open automation
window, finite fail-closed output, bounded monitoring, and lease-gated explicit shutdown. Add
new state calls only through the bounded seekable stream boundary and preserve component,
controller-component, then controller restore order. Keep discovery, lifecycle supervision,
recovery, quarantine, and project-record policy in engine. Add concrete platform worker transport
through an adapter that satisfies the existing isolated bridge contract instead of expanding the
audio callback contract.

Preserve fixed-latency declarations, cumulative prepare-time propagation, exact route difference,
fallible preallocation, and zero callback allocation for delay compensation. Preserve exact
format-neutral state bytes, digest and bound checks, per-instance project identity, timing-matched
dry fallback, and worker-fault telemetry. Dynamic latency changes require a control-side graph
rebuild and must never mutate compensation rings inside a callback.

Extend the existing engine output, clock, and A/V coordinator consumer by adapting immutable
timeline and decoded audio owners into the existing schedule and graph types instead of adding
upward dependencies. Keep channel layout and routing intent attached through that adapter, publish
only completed audible windows, and replace the export stage seam with a real prepared-graph adapter
before claiming timeline-owned source playback, mixing, or final delivery through this crate.

Keep authored clip-mix changes inside the engine compound transaction and retain complete failure
atomicity across timeline, project, and audio revisions. Never serialize callback-owned or prepared
runtime state into project snapshots or add an audio-local undo stack.

Preserve automation's ordered candidate transaction, exact revision fence, finite bounds, baseline
curve, half-open overwrite regions, and separate edit-versus-prepare boundary. Extend targets only
through typed schema changes and real consumers. Never dispatch, allocate, lock, or mutate authored
state from `ClipMixProcessor::process`, and keep the fixed-gain preparation path behavior compatible.
Keep read-only desktop keyframes correlated to one attached automation snapshot and exact clip ID.
Never infer visual effect curves from audio keys or persist UI preview state into this owner.

After source changes, refresh this map's inventory, architecture, invariants, tests, hash, and file
count from resulting behavior, then update consumer maps and validate the global map closure.
