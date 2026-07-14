---
module_id: superi-audio
source_paths:
  - open/crates/superi-audio
source_hash: 51a95c71a73080ccf542b8abc20f1c51423f0f68b0542f87554f78d87e10eeb2
source_files: 9
mapped_at_commit: a11cecdbf19ae1de90d94324abe844db49ed0c85
---

## Purpose and ownership

`superi-audio` reserves the subsystem boundary for an audio processing graph, sample-accurate synchronization, playback, mixing, resampling, metering, and plugin hosting. It currently contains no audio model, processor, clock, buffer, plugin host, or runtime behavior.

## Source inventory

- `open/crates/superi-audio/Cargo.toml`: Declares the crate and dependencies on `superi-core` and `superi-concurrency`.
- `open/crates/superi-audio/src/graph.rs`: Placeholder for a separate audio processing graph.
- `open/crates/superi-audio/src/hosting.rs`: Placeholder for additive VST3 and Audio Unit hosting.
- `open/crates/superi-audio/src/lib.rs`: Documents the intended subsystem and publicly exposes seven placeholder modules.
- `open/crates/superi-audio/src/metering.rs`: Placeholder for metering and audio analysis.
- `open/crates/superi-audio/src/mixing.rs`: Placeholder for buses, levels, fades, and mixing behavior.
- `open/crates/superi-audio/src/playback.rs`: Placeholder for the low-latency playback path.
- `open/crates/superi-audio/src/resample.rs`: Placeholder for sample-rate conversion and resampling.
- `open/crates/superi-audio/src/sync.rs`: Placeholder for sample-accurate audio and video synchronization.

## Public surface

The library publicly exports `graph`, `hosting`, `metering`, `mixing`, `playback`, `resample`, and `sync`. The modules contain documentation and TODO markers only, so the crate exposes no usable audio types or operations.

## Architecture and data flow

There is no implemented audio data flow. The manifest places the future audio subsystem above shared core and concurrency facilities, but no source imports either dependency. No media I/O, decoder, engine playback, graph evaluation, clock, or output device is connected to this crate.

## Dependencies and consumers

- Declared dependencies are `superi-core` and `superi-concurrency`. Both are unused in source.
- `superi-engine` declares `superi-audio` as a dependency, but no engine source references a `superi_audio` item.
- `superi-concurrency` contains an audio thread-role label string, not a Rust dependency on this crate and not a consumer of its surface.

## Invariants and operational boundaries

- The manifest keeps audio below engine and API and above only core and concurrency.
- The intended subsystem separation is visible in module names, but sample accuracy, real-time safety, thread ownership, latency, and plugin isolation are not implemented or enforced.
- Plugin hosting has no native bindings, discovery, sandbox, ABI, or license boundary.

## Tests and verification

The crate owns no tests, examples, or benchmarks. Compilation verifies only the placeholder module graph.

## Current status and risks

Every Rust module is an explicit documentation-only skeleton. The crate name and public modules can compile while providing no audio functionality, so downstream manifest dependency does not demonstrate integration.

## Maintenance notes

Update this map when concrete audio buffers, graph contracts, clocks, devices, plugin boundaries, or engine connections appear. Record real-time and thread invariants from code and tests rather than promoting the current TODO descriptions into guarantees.
