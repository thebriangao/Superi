---
module_id: superi-engine
source_paths:
  - open/crates/superi-engine
source_hash: 696b763dba983a76b0b11015eb374a38ca4140acbc4356076989a54c4e4eae46
source_files: 22
mapped_at_commit: a11cecdbf19ae1de90d94324abe844db49ed0c85
---

## Purpose and ownership

`superi-engine` is the orchestration layer intended to bind the open subsystems into playback, rendering, export, transaction, resource, lifecycle, plugin, validation, and public-introspection flows. Three paths are currently implemented: media backend registry assembly, declaration-based capability introspection, and upload of CPU-decoded video planes into GPU-resident allocations. The other orchestration modules are explicit placeholders.

## Source inventory

- `open/crates/superi-engine/Cargo.toml`: Declares the full subsystem dependency set, optional `os-codecs` and `vendor-codecs` features, and the test-only `pollster` dependency.
- `open/crates/superi-engine/src/av_sync.rs`: Placeholder for audio and video synchronization coordination.
- `open/crates/superi-engine/src/command.rs`: Placeholder for engine-wide commands, transactions, undo, persistence, and API routing.
- `open/crates/superi-engine/src/error.rs`: Placeholder for cross-subsystem error propagation and recovery.
- `open/crates/superi-engine/src/export_queue.rs`: Placeholder for render and export queue orchestration.
- `open/crates/superi-engine/src/frame_upload.rs`: Implements the media-I/O-to-GPU upload boundary for CPU-addressable decoded video frames.
- `open/crates/superi-engine/src/introspection.rs`: Implements deterministic, API-neutral media backend and codec capability snapshots from registry declarations.
- `open/crates/superi-engine/src/lib.rs`: Publicly exposes all fourteen engine modules and distinguishes integrated media and upload paths from remaining work.
- `open/crates/superi-engine/src/lifecycle.rs`: Placeholder for subsystem lifecycle and shared state.
- `open/crates/superi-engine/src/media.rs`: Builds the default media backend registry and feature-gated platform or explicitly configured vendor variants.
- `open/crates/superi-engine/src/nodes.rs`: Placeholder for source and output nodes connecting media I/O to the graph.
- `open/crates/superi-engine/src/playback.rs`: Placeholder for decoder, graph, cache, audio, and clock playback orchestration.
- `open/crates/superi-engine/src/plugins.rs`: Placeholder for plugin and extension loading and sandboxing.
- `open/crates/superi-engine/src/render.rs`: Placeholder for decode, graph, color, and encode orchestration.
- `open/crates/superi-engine/src/resources.rs`: Placeholder for cross-subsystem resource arbitration.
- `open/crates/superi-engine/src/validation.rs`: Placeholder for real-condition integration validation.
- `open/crates/superi-engine/tests/av1_capability_contract.rs`: Verifies default AV1 decode and encode selection through engine introspection.
- `open/crates/superi-engine/tests/frame_upload_contract.rs`: Verifies semantic preservation, GPU allocation lifetime, unsupported storage fallback, and shared GPU memory-budget errors.
- `open/crates/superi-engine/tests/opus_capability_contract.rs`: Verifies default Opus decode and encode selection.
- `open/crates/superi-engine/tests/os_codec_registry_contract.rs`: Feature-gated host checks for default and discovered operating-system backends and their hardware declarations.
- `open/crates/superi-engine/tests/vendor_codec_registry_contract.rs`: Feature-gated proof that default registries exclude vendor RAW and explicit worker configuration reaches introspection.
- `open/crates/superi-engine/tests/vorbis_capability_contract.rs`: Verifies default Vorbis decode and encode selection.

## Public surface

The library exposes the modules `av_sync`, `command`, `error`, `export_queue`, `frame_upload`, `introspection`, `lifecycle`, `media`, `nodes`, `playback`, `plugins`, `render`, `resources`, and `validation`. Only `frame_upload`, `introspection`, and `media` contain public items.

`frame_upload` exposes:

- `UploadedVideoFrame<'device>`, a cloneable owner of `VideoFormat`, timestamp, duration, cloned media metadata, and an `UploadedFrame<'device>`, with read-only getters.
- `VideoFrameUploader<'device>`, constructed with defaults, an explicit `UploadConfig`, or an explicit config plus shared `GpuMemoryPool`.
- `VideoFrameUploader::upload`, which accepts a `VideoFrame` and returns a GPU-resident frame or a classified error.

`introspection` exposes engine-owned mirrors of media declarations: `MediaBackendTier`, `MediaCapabilityConstraint<T>`, `MediaChromaSampling`, `MediaHardwareAcceleration`, `MediaOperation`, `MediaCodecCapability`, `MediaBackendCapabilities`, `MediaOperationSupport`, and `MediaCapabilities`. Record fields remain private and are available through getters. `MediaCapabilities::from_registry` is the construction path, and `is_empty` reports whether any operation is declared.

`media` exposes `media_backend_registry` in every build. With `vendor-codecs`, it also exposes `media_backend_registry_with_vendor_plugins`, which requires caller-supplied `VendorPluginConfig` values and an `OperationContext`.

## Architecture and data flow

### Media registry and introspection

`media_backend_registry` starts with an empty `superi-media-io` registry and asks `superi-codecs-rs` to atomically register the implemented permissive PCM, AV1, MP3, FLAC, Vorbis, Opus, and VP9 backends. With `os-codecs`, it then asks `superi-codecs-platform` to discover and register operations available on the current host. Platform registration may add VideoToolbox on macOS, Media Foundation on Windows, VA-API on Linux, or nothing when the host exposes no supported operation.

The default constructor never includes vendor workers. With `vendor-codecs`, `media_backend_registry_with_vendor_plugins` first builds the ordinary registry and then passes only the caller-selected worker configurations to `superi-codecs-vendor`. That adapter starts and handshakes explicit external processes before their registrations become visible; this engine crate neither discovers nor downloads them.

`MediaCapabilities::from_registry` reads immutable backend descriptors and capability declarations without opening a source or creating a decoder or encoder. Backend records are sorted by stable identifier. Declared source, decode, and encode operations are deduplicated through a `BTreeSet`. For each operation, the engine asks the registry for primary and fallback registrations in actual selection order: descending priority, then ascending backend identifier. Detailed codec tuples are converted and sorted; a declared decode or encode operation without a detailed row receives an `Unreported` row for each dimension.

Unsupported future variants from non-exhaustive media I/O enums become `Unsupported` and `Degraded` errors with `superi-engine.introspection/build_media_capabilities` context instead of being silently discarded. `superi-api` converts this engine-owned snapshot into the stable serialized public schema.

### Decoded-frame upload

`VideoFrameUploader::upload` accepts only a buffer reporting `FrameStorageKind::Cpu` and downcasts it to `CpuVideoBuffer`. It borrows every source plane with its exact byte slice, stride, and row count, then builds `superi_gpu::upload::DecodedFrameUpload` using the frame width, height, and unchanged pixel format. The GPU uploader validates plane layout, acquires pooled textures under the configured memory policy, directly stages compatible rows or repacks incompatible strides, and returns retained GPU allocations.

The engine wrapper preserves the source `VideoFormat`, rational timestamp, duration, and cloned `MediaMetadata` beside the GPU frame. It does not retain the CPU pixel allocation. GPU and external decoder storage return an `Unsupported` and `Degraded` error so a higher decode-selection layer can choose CPU output; there is no implicit download or external-texture import. Validation and upload errors receive frame context with storage kind, dimensions, and pixel-format code.

No implemented path connects the resulting `UploadedVideoFrame` to graph nodes, playback, color processing, rendering, cache, display, or encode.

## Dependencies and consumers

- Implemented source directly uses `superi-core` for errors and time, `superi-media-io` for frames, metadata, registries, and declarations, `superi-gpu` for device-bound upload and memory pools, and `superi-codecs-rs` for default backend registration.
- `superi-codecs-platform` is used only under `os-codecs`. `superi-codecs-vendor` is used only under `vendor-codecs`.
- `superi-image`, `superi-concurrency`, `superi-graph`, `superi-cache`, `superi-color`, `superi-effects`, `superi-timeline`, `superi-audio`, `superi-ai`, and `superi-project` are declared dependencies but are not referenced by implemented engine source. Most exist for the planned orchestration layer.
- `superi-api` consumes `introspection` types and the default registry constructor for its public capability snapshot and integration contracts.
- Engine integration tests are the only consumers of `frame_upload` in repository Rust code. No playback or render path invokes it.
- `superi-cli` reaches the engine only as a transitive dependency through `superi-api` and does not initialize it.
- `pollster` is used only by frame-upload tests to synchronously create a wgpu device.

## Invariants and operational boundaries

- Default registry construction is vendor-free. Vendor workers require both the feature and an explicit caller-provided list.
- Operating-system codecs require `os-codecs` and are limited to operations discovered on the current host.
- Introspection is declaration-only and never calls source, decoder, or encoder factories.
- Backend presentation is stable by identifier. Operation presentation follows enum and codec ordering, while candidate lists preserve registry selection order and keep primary and fallback tiers separate.
- Unknown media declaration variants fail with a classified degraded error rather than disappearing from the snapshot.
- Decoded upload preserves source format, timing, metadata, plane structure, and pixel representation. It supports CPU-addressable buffers only.
- Uploaded allocations are tied to the borrowed GPU device lifetime, are cloneable through the underlying shared frame owner, and return textures to the originating pool after the final clone is dropped.
- A caller-provided `GpuMemoryPool` participates in allocation admission. Budget exhaustion is retryable and resource-classified.
- The crate does not yet enforce whole-engine transactions, A/V sync, render determinism, cache coordination, lifecycle, plugin containment, or cross-subsystem resource policy because those modules are placeholders.

## Tests and verification

Eight integration tests cover the implemented engine paths:

- Three default-codec contracts verify that AV1, Opus, and Vorbis each expose one primary Rust backend for decode and encode and no fallback backend.
- Three frame-upload contracts verify GPU residency and cloned allocation ownership while preserving format, time, duration, and metadata; degraded rejection of external storage with engine context; and propagation of retryable shared-memory budget exhaustion.
- The `os-codecs` feature contract verifies the Rust AV1 backend plus platform-specific host declarations and hardware modes when available.
- The `vendor-codecs` contract verifies that ordinary construction contains no ARRIRAW, R3D, or BRAW support, then compiles a mock worker, explicitly registers it, and observes its declared RAW decode operations through introspection.

Frame-upload tests skip successfully when no wgpu adapter can be created, so a passing suite on such a host does not prove the GPU copy path. The default codec tests exercise registry declarations and selection, not actual decode or encode. Placeholder orchestration modules have no tests.

## Current status and risks

Registry assembly, capability introspection, and CPU-decoded frame upload are substantive and test-backed. The eleven files `av_sync.rs`, `command.rs`, `error.rs`, `export_queue.rs`, `lifecycle.rs`, `nodes.rs`, `playback.rs`, `plugins.rs`, `render.rs`, `resources.rs`, and `validation.rs` are explicit documentation-only placeholders.

The broad Cargo dependency list and public module tree therefore overstate system integration. There is no coherent engine lifecycle, transaction owner, source-to-output node flow, playback clock, audio path, cache use, render queue, export, plugin host, or real-condition integration validation. External and backend-GPU frame import are deliberately incomplete, and the current fallback comment depends on a higher decode-selection path that is not implemented here.

## Maintenance notes

When adding an engine path, distinguish manifest-only dependencies from crates actually wired into runtime flow. Update registry and introspection maps together when backend declarations or ranking change, and update `superi-api` when the public projection changes. For upload changes, document supported storage kinds, ownership and device lifetimes, format conversions, memory accounting, fallback behavior, and a real consumer. Remove placeholder labels only when a module contains substantive behavior and proof.
