---
module_id: superi-audio
source_paths:
  - open/crates/superi-audio
source_hash: 8e2a688077643149f17970ecd9c22bff10ce432940295dc6952fe22e59900458
source_files: 11
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-audio` owns the independent audio processing subsystem. Its foundational graph now has an
editable deterministic DAG and a separately prepared runtime plan for bounded interleaved f32
processing at exact sample coordinates. Its playback slice discovers operating-system output
devices and moves normalized interleaved samples through a fixed-capacity lock-free queue into typed
platform callbacks. Mixing, resampling, metering, sample-accurate A/V coordination, and plugin
hosting remain reserved sibling concerns.

The audio graph is intentionally separate from the image/GPU-oriented `superi-graph` engine. It
uses audio-owned topology identities while reusing core-owned `SampleTime`, `ChannelLayout`, and
shared errors, plus the concurrency-owned real-time audio execution domain.

## Source inventory

- `open/crates/superi-audio/Cargo.toml`: Declares dependencies on `superi-core`,
  `superi-concurrency`, exact CPAL 0.17.3, and ringbuf 0.4.8.
- `open/crates/superi-audio/src/graph.rs`: Implements typed audio graph, node, and edge identities;
  editable node and edge storage; deterministic cycle-safe topology; exact channel-layout
  validation; destination-scoped preparation; processor contracts; preallocated intermediate
  buffers; and exact consecutive block processing on the audio domain.
- `open/crates/superi-audio/src/hosting.rs`: Placeholder for additive VST3 and Audio Unit hosting.
- `open/crates/superi-audio/src/lib.rs`: Documents the implemented graph boundary and publicly
  exposes the seven audio concern modules.
- `open/crates/superi-audio/src/metering.rs`: Placeholder for metering and audio analysis.
- `open/crates/superi-audio/src/mixing.rs`: Placeholder for buses, levels, fades, and mixing.
- `open/crates/superi-audio/src/playback.rs`: Implements stable device identity, capability
  discovery, exact configuration validation, bounded producer and callback endpoints, sample
  conversion, audio-clock publication, atomic telemetry, and production stream lifecycle.
- `open/crates/superi-audio/src/resample.rs`: Placeholder for sample-rate conversion.
- `open/crates/superi-audio/src/sync.rs`: Placeholder for audio/video synchronization.
- `open/crates/superi-audio/tests/audio_graph_contract.rs`: Public consumer proof for topology,
  validation, preparation, exact processing, bounded blocks, continuity, and domain ownership.
- `open/crates/superi-audio/tests/device_output_contract.rs`: Public consumer proof for capability
  containment, bounded whole-frame admission, allocation ceilings, timed silence, clock progression,
  persisted device identities, domain-conflict behavior, telemetry, and real host discovery.

## Public surface

`graph` exposes `AudioGraphId`, `AudioNodeId`, and `AudioEdgeId` as ordered audio-owned u128
identities with permanent diagnostic prefixes. `AudioNode` declares one optional input layout and
one output layout. `AudioEdge` identifies one directed node route. `AudioGraph` owns a fixed sample
rate and positive maximum frame count, exposes stable maps and topological inspection, and supports
checked node and edge insertion and removal.

`AudioProcessor` accepts one `AudioProcessBlock` containing exact `SampleTime`, frame count,
optional connected input, mutable output, and explicit input and output channel layouts.
`AudioGraph::prepare` consumes node processors for the selected destination and its ancestors,
then returns `PreparedAudioGraph`. The prepared value exposes its graph identity, fixed node order,
destination layout, next required sample, and `process`.

`playback` exposes validated opaque `OutputDeviceId` values, exact capability ranges and stream
configurations, partial discovery failures, bounded `OutputProducer` and `OutputConsumer` endpoints,
clonable telemetry, and an owning `DeviceOutput`. `discover_output_devices` performs real host
enumeration, `create_output_buffer` preallocates the engine-to-device handoff, and
`start_device_output` revalidates and starts the selected stream.

## Architecture and data flow

Graph editing occurs outside the audio callback. Nodes and edges live in ordered maps, adjacency is
kept in ordered sets, edge insertion validates both endpoints, direct or transitive cycles, one
incoming route per processing node, and exact ordered layout equality before mutation. Stable Kahn
ordering chooses the smallest ready node identity.

Preparation walks backward from one destination, rejects unconnected required inputs and missing
processors, filters the complete topological order to required ancestors, resolves each input to an
earlier runtime index, and fallibly reserves one maximum-sized interleaved f32 buffer per node. The
editable graph remains independent of processor state and cannot mutate the prepared topology.

Processing first requires `ExecutionDomain::Audio`, then validates sample rate, positive bounded
frame count, exact output length, coordinate overflow, and continuity with the prior successful
block. Each processor reads only an earlier node's current block and writes its own preallocated
buffer. The destination buffer is copied into caller-owned output, and the next sample advances
only after every node succeeds. Graph-owned diagnostics allocate only on failure paths.

Playback discovery and stream setup stay on control threads. The sole producer admits complete
interleaved frame submissions or rejects them whole. The platform callback enters
`ExecutionDomain::Audio`, converts samples directly into the device-owned typed slice, substitutes
silence on starvation or a domain conflict, advances `AudioMasterClock` by every complete presented
frame, and updates relaxed atomics. Device capabilities are re-read before stream construction, and
portable speaker positions remain explicitly unknown when CPAL reports only channel count.

## Dependencies and consumers

- `superi-core` supplies ordered `ChannelLayout`, exact `SampleTime`, and the shared classified
  error model. The audio graph composes these meanings instead of duplicating them.
- `superi-concurrency` supplies `ExecutionDomain::Audio` and its platform-owned, nonblocking,
  allocation-free policy plus `AudioMasterClock`. The prepared graph and output callback enforce
  the audio domain at their process boundaries.
- CPAL 0.17.3 supplies CoreAudio, WASAPI, and ALSA discovery and output adapters while remaining
  compatible with the repository Rust declaration. ringbuf 0.4.8 supplies the preallocated SPSC
  sample queue. Linux CI installs ALSA development headers.
- `superi-engine` declares `superi-audio` but still has no production Rust call site. Engine
  playback orchestration is outside this graph checkpoint.
- `audio_graph_contract` is the first real in-repository consumer. It builds and processes a
  source-to-gain-to-gain chain through the public API on the audio domain.
- No media decoder or `superi-media-io::AudioBlock` currently feeds this graph. The graph uses
  interleaved f32 callback buffers and explicit core layouts rather than introducing an upward
  media-I/O dependency.

## Invariants and operational boundaries

- Graph sample rate and maximum frames are positive and fixed for one graph and prepared lifetime.
- Nodes and edges are deterministically inspectable. Rejected mutations leave all primary and
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

## Tests and verification

`audio_graph_contract` has three public integration tests. It proves stable identity ordering and
topological order; atomic cycle, duplicate-input, layout, and missing-endpoint rejection;
unconnected-input preparation failure; exact source and two-stage gain processing over consecutive
48 kHz stereo blocks; audio-domain enforcement; and nonadvancing failure for rate, bound, output
length, and continuity errors.

The real processing fixture emits channel-distinct samples from exact `SampleTime`, passes them
through two processors, and verifies every interleaved result in two adjacent blocks. This proves
the public substrate and callback boundary, not decoded media, hardware output, physical latency,
multi-input mixing, resampling quality, metering, A/V sync, or plugin hosting.

Two playback unit tests and ten public output contracts prove typed conversion, backend-default
buffer semantics, capacity and normalized-sample validation, whole-frame backpressure, silence and
telemetry, exact clock progression, persisted locators, domain-conflict degradation, production host
discovery, endpoint thread transfer, and 5,120,000 simulated frames without accumulated drift.

## Current status and risks

The independent audio graph and production device-output substrate are substantive and publicly
test-backed. Five sibling modules remain documentation-only placeholders. There is no production
engine or decoder composition and no end-to-end source-playback-final-mix path.

The single-input shape is deliberately narrow to avoid inventing later bus semantics. Extending it
to multi-input processing will require precomputed input views that preserve allocation-free callback
behavior. Caller processors remain a trust boundary for real-time safety and error atomicity.
Physical latency, semantic channel routing, hot-plug, constrained-device, and soak evidence remain
owned by the platform audio and physical test lanes.

## Maintenance notes

Preserve the edit-versus-prepare split, stable identity ordering, exact sample and channel meanings,
fallible preallocation, whole-frame queue admission, explicit capacity ceiling, timed-silence clock
behavior, and callback-only atomic telemetry. Any multi-input extension must define summing order
and routing semantics explicitly and prove no callback allocation. Keep discovery and stream setup
on control threads, and revalidate capabilities before stream creation.

After owned source changes, update this map's inventory, architecture, invariants, tests, hash, and
file count from the resulting behavior, then validate the global map closure.
