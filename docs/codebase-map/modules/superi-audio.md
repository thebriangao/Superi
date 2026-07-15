---
module_id: superi-audio
source_paths:
  - open/crates/superi-audio
source_hash: baa487e85e619276390cb26b41634ae83891b62b8e0061c898d667e02acd9987
source_files: 10
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-audio` owns the independent audio processing subsystem. Its foundational graph now has an
editable deterministic DAG and a separately prepared runtime plan for bounded interleaved f32
processing at exact sample coordinates. Playback, device output, mixing, resampling, metering,
sample-accurate A/V coordination, and plugin hosting remain reserved sibling concerns.

The audio graph is intentionally separate from the image/GPU-oriented `superi-graph` engine. It
uses audio-owned topology identities while reusing core-owned `SampleTime`, `ChannelLayout`, and
shared errors, plus the concurrency-owned real-time audio execution domain.

## Source inventory

- `open/crates/superi-audio/Cargo.toml`: Declares dependencies on `superi-core` and
  `superi-concurrency`, both now used by the graph implementation.
- `open/crates/superi-audio/src/graph.rs`: Implements typed audio graph, node, and edge identities;
  editable node and edge storage; deterministic cycle-safe topology; exact channel-layout
  validation; destination-scoped preparation; processor contracts; preallocated intermediate
  buffers; and exact consecutive block processing on the audio domain.
- `open/crates/superi-audio/src/hosting.rs`: Placeholder for additive VST3 and Audio Unit hosting.
- `open/crates/superi-audio/src/lib.rs`: Documents the implemented graph boundary and publicly
  exposes the seven audio concern modules.
- `open/crates/superi-audio/src/metering.rs`: Placeholder for metering and audio analysis.
- `open/crates/superi-audio/src/mixing.rs`: Placeholder for buses, levels, fades, and mixing.
- `open/crates/superi-audio/src/playback.rs`: Placeholder for low-latency device playback.
- `open/crates/superi-audio/src/resample.rs`: Placeholder for sample-rate conversion.
- `open/crates/superi-audio/src/sync.rs`: Placeholder for audio/video synchronization.
- `open/crates/superi-audio/tests/audio_graph_contract.rs`: Public consumer proof for topology,
  validation, preparation, exact processing, bounded blocks, continuity, and domain ownership.

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

## Dependencies and consumers

- `superi-core` supplies ordered `ChannelLayout`, exact `SampleTime`, and the shared classified
  error model. The audio graph composes these meanings instead of duplicating them.
- `superi-concurrency` supplies `ExecutionDomain::Audio` and its platform-owned, nonblocking,
  allocation-free policy. The prepared graph enforces this domain at its process boundary.
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
- All implementation is safe Rust. Plugin ABI, worker isolation, native code, and device callbacks
  are not part of this module.

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

## Current status and risks

The independent audio graph substrate is substantive and publicly test-backed. Six sibling modules
remain documentation-only placeholders. There is no production engine or decoder consumer, no
device backend, and no end-to-end source-playback-final-mix path.

The single-input shape is deliberately narrow to avoid inventing later bus semantics. Extending it
to multi-input processing will require precomputed input views that preserve allocation-free callback
behavior. Caller processors remain a trust boundary for real-time safety and error atomicity.

## Maintenance notes

Preserve the edit-versus-prepare split, stable identity ordering, exact sample and channel meanings,
fallible preallocation, and failure-only diagnostic allocation. Any multi-input extension must
define summing order and routing semantics explicitly and prove no callback allocation. Device,
playback, sync, resample, metering, or hosting implementations must update their actual consumers
and replace only the corresponding placeholder claims.

After owned source changes, update this map's inventory, architecture, invariants, tests, hash, and
file count from the resulting behavior, then validate the global map closure.
