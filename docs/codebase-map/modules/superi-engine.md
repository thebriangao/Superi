---
module_id: superi-engine
source_paths:
  - open/crates/superi-engine
source_hash: 92b2175ace8d0e0aabf95dfb6dd9e8731afa514cbab25a896f9cccbf16e86d90
source_files: 24
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-engine` is the open orchestration layer. Four paths are currently substantive: canonical
editorial command state, media backend registry assembly, declaration-based capability
introspection, CPU-decoded frame upload into GPU allocations, and exact viewport and export color
metadata branching. Playback, rendered color execution, export encoding, broad
transactions, lifecycle, resources, plugins, nodes, A/V sync, and validation remain incomplete.

The command path is a bounded reference owner for contract conformance. It does not claim to replace
the production project, timeline, graph, media, color, render, or muxing owners.

## Source inventory

- `open/crates/superi-engine/Cargo.toml`: Declares subsystem dependencies, optional codec features,
  production `sha2`, and test-only `pollster`.
- `open/crates/superi-engine/src/av_sync.rs`: Placeholder for A/V synchronization.
- `open/crates/superi-engine/src/command.rs`: Implements canonical fixture identity, named timeline
  and trim state, complete mirror graph control state, typed operation evidence, bounded source
  validation, monotonic revisions, and full-state undo plus redo.
- `open/crates/superi-engine/src/error.rs`: Placeholder for cross-subsystem recovery.
- `open/crates/superi-engine/src/export_queue.rs`: Placeholder for render and export queues.
- `open/crates/superi-engine/src/frame_upload.rs`: Implements the media-I/O-to-GPU upload boundary
  for CPU-addressable decoded video and retains the frame's complete color pipeline beside the GPU owner.
- `open/crates/superi-engine/src/introspection.rs`: Implements deterministic API-neutral backend and
  codec capability snapshots.
- `open/crates/superi-engine/src/lib.rs`: Exposes fourteen engine modules.
- `open/crates/superi-engine/src/lifecycle.rs`: Placeholder for subsystem lifecycle.
- `open/crates/superi-engine/src/media.rs`: Builds default and feature-gated media registries.
- `open/crates/superi-engine/src/nodes.rs`: Placeholder for media and graph nodes.
- `open/crates/superi-engine/src/playback.rs`: Placeholder for playback orchestration.
- `open/crates/superi-engine/src/plugins.rs`: Placeholder for plugins and extensions.
- `open/crates/superi-engine/src/render.rs`: Defines independent viewport and export color metadata branches from cached scene state, requiring correctly classified terminal display or output stages.
- `open/crates/superi-engine/src/resources.rs`: Placeholder for resource arbitration.
- `open/crates/superi-engine/src/validation.rs`: Placeholder for real-condition validation.
- `open/crates/superi-engine/tests/av1_capability_contract.rs`: Default AV1 selection proof.
- `open/crates/superi-engine/tests/color_metadata_propagation_contract.rs`: Proves exact source interpretation, ordered transform history, cache identity, independent viewport and export intent, and invalid-order rejection across media, graph, timeline, cache, and engine boundaries.
- `open/crates/superi-engine/tests/frame_upload_contract.rs`: Upload, ownership, storage, and budget
  proof.
- `open/crates/superi-engine/tests/opus_capability_contract.rs`: Default Opus selection proof.
- `open/crates/superi-engine/tests/os_codec_registry_contract.rs`: Feature-gated host registry proof.
- `open/crates/superi-engine/tests/scenario_contract.rs`: Exact canonical state, atomicity, bounds,
  operation log, and reversal proof.
- `open/crates/superi-engine/tests/vendor_codec_registry_contract.rs`: Explicit vendor registry
  proof.
- `open/crates/superi-engine/tests/vorbis_capability_contract.rs`: Default Vorbis selection proof.

## Public surface

`command` exposes the fixed canonical values, `ScenarioEngine`, `ScenarioAction`,
`ScenarioSnapshot`, phases, fixture state, timeline state, graph nodes and edges, mirror parameters,
implementation identity, typed operation arguments, and operation records. Supported mutations are
import, insert, trim, and horizontal mirror. Undo and redo are history actions. Export is
intentionally absent from engine mutations.

`frame_upload` exposes `UploadedVideoFrame` and `VideoFrameUploader` with explicit configuration,
shared pool construction, and exact color-pipeline access. `render` exposes `ViewportColorMetadata`
and `ExportColorMetadata`, which clone cached scene metadata and append a correctly typed terminal
stage without transforming pixels. `introspection` exposes engine-owned media backend, operation, codec,
constraint, and hardware records through `MediaCapabilities::from_registry`. `media` exposes the
default registry and the feature-gated explicitly configured vendor constructor.

The other public modules currently contain documentation only.

## Architecture and data flow

### Canonical editorial command state

The scenario engine begins with stable project identity and empty state. Import requires exact
`slice/video-cfr` version 1 metadata, lowercase manifest and payload digests, 24 fps, 96 frames, and
96 by 54 extent. It reads at most 64 MiB and independently verifies the payload digest. Placement
accepts only timeline frame zero. Trim accepts only source `[24, 72)`. The mirror action constructs
the exact source, transform, and output graph, two image edges, binary64 matrix, nearest sampling,
transparent black edges, and derived timeline identity.

Every successful mutation stores a complete before and after content snapshot plus one stable typed
operation record. The active log uses `slice.op.import`, `slice.op.insert`, `slice.op.trim`, and
`slice.op.effect` with original resulting revisions. Undo and redo restore complete content without
filesystem side effects or reimport and advance the global revision monotonically. Rejected actions
leave state and both history stacks unchanged.

### Registry, introspection, and upload

Default registry construction atomically registers permissive Rust codecs. `os-codecs` may add
host-discovered operations, and `vendor-codecs` requires explicit worker configuration.
Introspection reads declarations only, orders stable records, separates primary and fallback tiers,
and never constructs a source or codec.

`VideoFrameUploader` accepts CPU storage, validates and borrows exact planes, asks the GPU uploader
for pooled textures, and preserves format, time, duration, metadata, complete color pipeline, and
allocation lifetime. It rejects GPU and external storage with a classified degraded error.

The color metadata integration path preserves source tags and ordered transforms through media,
graph, timeline, and cache wrappers, then derives independent display and delivery branches. Each
branch validates its terminal stage kind and leaves the cached scene state unchanged. No production
path yet executes those transforms, connects uploaded textures to graph evaluation, monitors a
viewport, or encodes an export.

## Dependencies and consumers

- `superi-core` supplies errors, identifiers, and exact time used directly by canonical commands,
  introspection, and upload.
- `sha2` supplies bounded fixture payload identity.
- `superi-media-io`, `superi-gpu`, and `superi-codecs-rs` support registry, declaration, and upload
  paths.
- Platform and vendor codec crates are feature-gated.
- Image, graph, cache, and timeline now support the color metadata integration contract. Concurrency,
  color execution, effects, audio, AI, and project remain declared dependencies without production command integration.
- `superi-api` consumes command snapshots and capability snapshots, preserving the public seam.
- `superi-cli` reaches this module only through `superi-api`.

## Invariants and operational boundaries

- Canonical arguments are exact; the reference engine is not a general editor model.
- Source validation is bounded, digest checked, and atomic.
- Four mutations have stable typed IDs and complete internal prior state.
- Export is not a project mutation and is not represented in engine history.
- Undo and redo restore complete semantic state without reimport or filesystem effects.
- Default registry construction is vendor free; host and vendor behavior remains opt-in.
- Introspection is declaration-only and has deterministic ordering.
- Upload preserves source representation and supports CPU-addressable buffers only.
- Upload preserves exact color-pipeline metadata, and viewport or export intent branches cannot mutate cached scene state.
- A viewport terminal stage must be `Display`; an export terminal stage must be `Output`.
- GPU ownership and pool lifetime remain tied to the originating device.
- Placeholder modules do not imply whole-engine transaction, render, or lifecycle behavior.

## Tests and verification

Canonical scenario contracts prove the exact fixture metadata, names, half-open ranges, complete
graph topology and ports, mirror matrix, four operation IDs and original revisions, two undo plus
two redo recovery at revision 8, rejected-action atomicity, payload digest failure, and the 64 MiB
bound.

Codec contracts prove default AV1, Opus, and Vorbis declarations, optional host exposure, and
explicit vendor registration. Upload contracts prove semantic preservation and cloned allocation
ownership when an adapter is available, classified unsupported storage, and retryable shared budget
failure. Color metadata contracts prove exact ICC and named-space retention, transform order,
source continuity, cache mismatch rejection, independent display and delivery intent, and invalid
stage ordering. GPU tests may skip without an adapter; capability tests prove declarations, not codec
execution.

## Current status and risks

Canonical command state is substantive and test-backed, but it is a reference boundary whose
implementation identity is disclosed as such. It validates fixture bytes without opening their
container or decoding frames. Timeline and graph state are exact control models but do not use the
production timeline owner or the generic `superi-graph` DAG store.

Nine orchestration files remain documentation-only placeholders. There is no coherent source
registry integration, playback clock, audio flow, cache storage, rendered color execution,
encoder-to-mux path, project persistence, lifecycle, plugin host, or real-condition validator.

## Maintenance notes

Keep fixed canonical state synchronized with `docs/vertical-slice.md`, the strict API projection,
CLI runner, and operation contracts. A new production owner should replace the corresponding stub
through its real crate rather than growing this reference model into a competing system. Registry
or upload changes require updating their actual consumers and tests independently. Remove a
placeholder label only after substantive behavior and consumer proof exist.
