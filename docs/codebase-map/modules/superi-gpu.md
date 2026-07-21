---
module_id: superi-gpu
source_paths:
  - open/crates/superi-gpu
source_hash: 3ccd5b7e2cf881c46a02f4a6878ee0cfcb72f6cd0e7342d05b6178179ed21d44
source_files: 34
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-gpu` is the open engine's wgpu substrate. It owns native adapter discovery and selection,
logical device lifetimes, managed GPU resources, shader validation and caching, exact texture and
buffer metadata, decoded-frame upload, storage conversion, managed compute and render passes,
exclusive submission and fence retirement, native viewport surfaces, explicit readback, aggregate
diagnostics, memory pressure cooperation, and device-loss reconstruction.

The crate's central boundary is one device identity per logical device generation. Managed handles,
pass batches, readbacks, fences, surface frames, and recovery results are accepted only by the
device lifetime that created them. Multiple adapters produce independent devices and queues. The
crate does not provide implicit cross-device copies, shared resources, synchronization, or failover.

Media payloads stay GPU resident across upload, conversion, render work, and presentation. CPU
movement is explicit at decoded-frame upload and export or thumbnail readback. The crate exposes raw
wgpu resource borrows and re-exports `wgpu` for descriptor compatibility, but keeps the raw queue
private. Normal command submission, completion polling, resource retirement, and presentation go
through the single managed submission owner. Queue texture writes used by upload and recovery are
internal ordered writes rather than alternate public submission queues.

Pixel conversion is a storage conversion boundary, not a color-management engine. It can change
packing, numeric representation, RGB or YUV matrix and range, chroma subsampling, and alpha
association while preserving extent. It rejects changes to color primaries or transfer functions,
which must be performed by `superi-color`.

## Source inventory

### Package and example

- `open/crates/superi-gpu/Cargo.toml`: Declares the crate, workspace metadata, direct wgpu,
  raw-window-handle, SHA-256, core, and image dependencies, plus native smoke-test dependencies.
- `open/crates/superi-gpu/examples/native_viewport_smoke.rs`: Runs a manual one-frame native window,
  surface, clear, managed submit, present, fence wait, and exit path.

### Device, ownership, and resources

- `open/crates/superi-gpu/src/lib.rs`: States crate-wide ownership, exports every public module, and
  re-exports the workspace wgpu version.
- `open/crates/superi-gpu/src/device.rs`: Owns instances, deterministic adapter catalogs and
  selection, multi-adapter creation, device identity, capabilities, loss state, the private queue,
  error-scope serialization, and recreation of a lost device generation.
- `open/crates/superi-gpu/src/resource.rs`: Implements device-scoped resource managers, process-local
  resource IDs, live counts, ownership checks, the RAII lease used by managed handles, and a
  borrowed device accessor that keeps render encoding inside the same managed lifetime.
- `open/crates/superi-gpu/src/buffer.rs`: Wraps raw buffers with immutable creation metadata,
  resource leases, optional memory reservations, and portable creation validation.
- `open/crates/superi-gpu/src/texture.rs`: Wraps textures and views, retains parent allocations,
  snapshots descriptors, carries optional memory reservations, and validates basic view ranges.
- `open/crates/superi-gpu/src/binding.rs`: Owns samplers, bind-group layouts, buffer binding ranges,
  retained binding resources, bind groups, and device-identity validation.
- `open/crates/superi-gpu/src/pool.rs`: Provides process-local managed payload budgets, RAII byte
  reservations, coalesced pressure subscriptions, and ordered cooperative eviction.
- `open/crates/superi-gpu/src/texture_pool.rs`: Computes aligned physical texture allocations,
  accounts payload bytes, reuses exact-compatible idle textures, and prevents reuse while owners
  escape a checkout.
- `open/crates/superi-gpu/src/resource_contract.rs`: Exercises the crate-private managed resource
  graph with real buffers, textures, bindings, shaders, pipelines, compute, render, and readback,
  including transitive lifetime and classified-error checks.

### Shaders, pipelines, passes, and conversion

- `open/crates/superi-gpu/src/shader.rs`: Parses and validates WGSL, computes exact SHA-256 source
  identities, reflects entry points and bindings, serializes backend error scopes, and maintains a
  bounded exact-key LRU cache.
- `open/crates/superi-gpu/src/pipeline.rs`: Owns managed pipeline layouts and asynchronous compute
  and render pipeline creation, retains modules and layouts, and records validation metadata for
  pass preflight.
- `open/crates/superi-gpu/src/pass.rs`: Defines owned compute and render plans, performs complete CPU
  preflight, records accepted passes in order, retains transitive dependencies, and submits batches
  through the exclusive queue owner.
- `open/crates/superi-gpu/src/pass_contract.rs`: Exercises one real managed compute pass followed by
  one render pass and verifies exact GPU results through readback.
- `open/crates/superi-gpu/src/convert.rs`: Maps logical pixel formats to portable plane textures,
  validates exact storage-conversion plans, generates WGSL, builds per-plane render pipelines, and
  encodes GPU-resident conversions with an explicit retention lease.
- `open/crates/superi-gpu/src/conversion_contract.rs`: Tests conversion layouts, plans, shader and
  pipeline compilation, GPU execution, high-bit-depth and packed formats, upload-pool lifetimes, and
  a CPU alpha-reference comparison.

### Transfer, submission, presentation, diagnostics, and recovery

- `open/crates/superi-gpu/src/upload.rs`: Validates decoder planes, selects lossless texture storage,
  acquires pooled planes, repacks only copy-incompatible rows, writes textures, and returns immutable
  clone-owned GPU frames.
- `open/crates/superi-gpu/src/upload_contract.rs`: Proves upload layout, direct and repacked writes,
  planar and semiplanar formats, aligned allocation reuse, and final-clone pool return.
- `open/crates/superi-gpu/src/submission.rs`: Owns the non-Send exclusive queue, scoped retained-owner
  bundles, monotonic fences, callback polling, ordered retirement, loss aborts, and blocking waits.
- `open/crates/superi-gpu/src/readback.rs`: Implements named export and thumbnail boundaries,
  texture-copy validation, budgeted staging, fence-gated mapping, row-padding removal, and result
  ownership.
- `open/crates/superi-gpu/src/diagnostics.rs`: Produces privacy-safe aggregate snapshots and optional
  timestamp-query timing for managed pass batches.
- `open/crates/superi-gpu/src/surface.rs`: Classifies desktop native handles, retains shell hosts,
  configures native surfaces, acquires frames, and submits before presentation through the managed
  queue.
- `open/crates/superi-gpu/src/recovery.rs`: Recreates a lost device on the same adapter, runs ordered
  typed reconstruction recipes, permits validated replacement-device initialization writes, and
  publishes reviewed progress notices and all-or-nothing recovered outputs.

### Public contract tests

- `open/crates/superi-gpu/tests/device_contract.rs`: Proves public adapter selection, capability
  rejection, device ownership, private queue policy, and ordered multi-adapter device creation.
- `open/crates/superi-gpu/tests/diagnostics_contract.rs`: Proves device-scoped aggregate snapshots,
  user-safe fields, timed passes, timing capacity, and timing-resource retirement.
- `open/crates/superi-gpu/tests/memory_pool_contract.rs`: Proves budgets, pressure delivery, ordered
  eviction, concurrent reservation safety, payload sizing, and shared texture-pool accounting.
- `open/crates/superi-gpu/tests/native_surface_contract.rs`: Proves desktop handle classification,
  target support, viewport validation, and retryable unavailable shell handles.
- `open/crates/superi-gpu/tests/pass_contract.rs`: Proves managed pass preflight, ordering, ownership,
  retained pipeline lifetime, and stale-device submission rejection through the public API.
- `open/crates/superi-gpu/tests/readback_contract.rs`: Proves readback layout and budgets, exact export
  and thumbnail bytes, cancellation, fence ordering, and recovered-device rejection.
- `open/crates/superi-gpu/tests/recovery_contract.rs`: Proves native loss detection, generation
  replacement, ordered dependent reconstruction, initialization writes, notices, stale rejection,
  resumed work, and partial-failure cleanup.
- `open/crates/superi-gpu/tests/shader_contract.rs`: Proves reflection, exact caching, diagnostics,
  LRU behavior, pipeline compatibility, device scope, and concurrent error-scope ordering.
- `open/crates/superi-gpu/tests/submission_contract.rs`: Proves exclusive ownership, monotonic fences,
  stale queue-state rejection, and pooled-owner retention through retirement.
- `open/crates/superi-gpu/tests/texture_pool_contract.rs`: Proves alignment, exact compatibility,
  escaped-handle safety, draining, and validation before native allocation.

## Public surface

### Device and adapter lifetime

- `InstanceOptions` and `GpuInstance` select compiled native backends and enumerate adapters on
  non-wasm targets.
- `AdapterId`, `AdapterCapabilities`, `AdapterSnapshot`, and `AdapterCatalog` provide deterministic
  process-local adapter identity and capability snapshots. `AdapterSelection` selects one exact or
  policy-ranked adapter. `MultiAdapterSelection`, `SelectedAdapters`, and `GpuDeviceSet` create
  distinct devices in primary-first order.
- `DeviceRequest` declares required features, limits, label, and memory hints. `GpuDevice` exposes
  the selected adapter, enabled capabilities, current generation and status, availability checks,
  and a borrowed raw device, but not its queue.
- `GpuDeviceStatus`, `GpuDeviceLoss`, and `GpuDeviceLossReason` expose stable loss state. Device
  recreation is consumed through the recovery layer rather than making obsolete handles valid.

### Managed resources and memory

- `GpuResources` is the cloneable factory and accounting scope for buffers, textures, views,
  samplers, bind-group layouts, bind groups, shader modules, pipeline layouts, and pipelines.
  Separate managers for one device may interoperate because ownership checks use device identity,
  while manager scope remains diagnostic.
- `GpuResourceId`, `GpuResourceKind`, and `GpuResourceStats` expose process-local identities and live
  counts. IDs are diagnostic and not durable serialization keys.
- `GpuBuffer`, `GpuTexture`, `GpuTextureView`, `GpuSampler`, `GpuBindGroupLayout`, `GpuBindGroup`,
  `GpuPipelineLayout`, `GpuComputePipeline`, and `GpuRenderPipeline` are managed owners. Cloneable
  wrappers share one underlying allocation and lease. Views, bindings, and pipelines retain their
  parent dependencies.
- `MemoryBudget`, `GpuMemoryPool`, `MemoryReservation`, `MemoryClass`, pressure and subscription
  types, `MemoryEvictor`, and `MemoryEvictionOutcome` define deterministic managed-payload
  accounting and cooperative reclamation.
- `TextureAlignment`, `TextureRequest`, `TexturePoolConfig`, `TexturePool`, and `PooledTexture`
  expose aligned, budgeted, exact-key texture reuse. Logical extent, physical extent, allocation
  identity, and full-initialization responsibility remain explicit.

### Shaders, bindings, pipelines, and passes

- `GpuShaderModuleDescriptor`, `GpuShaderModule`, `GpuShaderModuleInfo`, reflection types,
  diagnostics, `ShaderCache`, and `ShaderCacheStats` expose bounded canonical WGSL compilation and
  reflection without retaining source text in diagnostic values.
- Binding descriptors use managed buffers, samplers, texture views, and explicit layouts. The
  completed bind group owns clones of every dependency, including resource arrays.
- Pipeline descriptors consume managed modules and optional managed layouts. Resulting pipelines
  retain those owners and expose cached metadata used to validate later pass state.
- `GpuComputePassPlan` and `GpuRenderPassPlan` own their exact commands and resources.
  `GpuPassEncoder` validates whole plans before mutating the raw encoder, assigns stable sequence
  numbers to accepted plans, and produces a single-use `GpuPassBatch`. `GpuSubmissionQueue` consumes
  the batch and returns `GpuPassSubmission` with a fence and optional timing handle.

### Upload, conversion, readback, and presentation

- `DecodedPlane`, `DecodedFrameUpload`, `UploadConfig`, and `DecodedFrameUploader` form the CPU
  decoded-frame boundary. `UploadedPlane`, `UploadedFrame`, `PlaneUploadLayout`, `PlaneUploadPath`,
  and `UploadReport` describe the lossless GPU-resident result and its pooled ownership.
- `GpuFrameDescriptor` resolves logical `PixelFormat`, color, alpha, and chroma metadata into exact
  portable plane layouts. `GpuPixelFrame` binds that descriptor to managed views or adapts an
  `UploadedFrame` without copying. `GpuConversionPlan` validates storage-only compatibility.
  `GpuPixelConverter` records one render pass per destination plane and returns a must-use
  `GpuConversionLease` for submission retention.
- `TextureReadbackRequest`, `ReadbackBoundary`, `TextureReadbackManager`,
  `EncodedTextureReadback`, `SubmittedTextureReadback`, `TextureReadbackLayout`, and
  `TextureReadbackResult` make export and thumbnail readback explicit and single-use.
- `ViewportHost`, `NativeViewportKind`, `ViewportExtent`, `NativeViewportSurface`, and
  `ViewportFrame` own the native surface lifecycle. `ViewportFrame::submit_and_present` consumes the
  frame only after managed submission succeeds.

### Submission, diagnostics, and recovery

- `GpuSubmissionQueue` is thread-affine and claims the sole managed submission-owner slot on its
  device. `GpuSubmissionResources` is a queue-scoped heterogeneous owner bundle. `GpuFence`,
  `GpuFenceStatus`, and `GpuSubmissionProgress` expose queue-local completion without giving fence
  observers the ability to poll the device.
- `GpuDiagnosticSnapshot` combines adapter class, feature state, resource counts, submission
  progress, and optional memory totals. `GpuTimingConfig`, `GpuTimingHandle`, `GpuPassTiming`, and
  `GpuTimingReport` expose label-free optional timestamp timing.
- `GpuRecoveryPlan` registers labelled typed recipes in dependency order. `ReconstructionKey`,
  `ReconstructedResources`, and `GpuRecoveryContext` permit typed dependencies and validated writes
  on the replacement device. `RecoveredGpu`, `GpuRecoveryReport`, `GpuRecoveryNotice`, and
  `GpuRecoveryPhase` publish only after complete success.

## Architecture and data flow

### Device and resource bootstrap

1. `GpuInstance` intersects requested and compiled backends. Native enumeration snapshots adapter
   features, limits, downlevel support, and driver identity, then sorts deterministically and assigns
   process-local ordinals.
2. Selection filters exact capability requirements before ranking. Multi-adapter selection removes
   each chosen record so one adapter cannot satisfy two slots.
3. Device creation rechecks requirements and creates a private wgpu queue, a new identity token,
   generation one, loss state, a FIFO error-scope gate, and an exclusive submission-owner flag.
4. `GpuResources` creates managed objects for that identity. Each object owns a lease and immutable
   creation metadata. Buffers and textures created through budgeted internal paths also own one
   non-cloneable `MemoryReservation` behind their shared allocation owner.
5. Bind groups retain layouts and all bound objects. Texture views retain their texture. Pipelines
   retain modules and explicit layouts. These edges turn wgpu's implicit command dependencies into
   explicit Rust ownership.

### Memory and texture flow

1. `TextureRequest` combines caller alignment with format block requirements, validates device
   capabilities, and computes checked payload bytes across mips, layers or depth, samples,
   compressed blocks, and supported multiplanar formats.
2. `TexturePool` first takes an exact-compatible idle allocation. A miss reserves bytes from the
   shared `GpuMemoryPool`, which emits coalesced pressure and calls evictors in caller-provided order
   before enforcing the hard limit.
3. A checkout exposes distinct logical and physical extents and requires the caller to fully
   initialize the requested logical region. Idle allocations remain charged to the budget.
4. On checkout drop, the texture returns to idle only when no cloned texture or dependent view has
   escaped. Otherwise it is discarded from the pool and its reservation remains live until the last
   owner drops.

### Decoded bytes to GPU planes

1. `DecodedFrameUpload` validates nonzero dimensions, canonical plane count, row count, stride, and
   byte length from `superi-core` pixel metadata.
2. The uploader maps each format to lossless storage. Packed RGB and BGR use three `R8Unorm` texels
   per logical pixel. Higher-bit unorm and planar formats use integer textures when required to
   preserve exact bits. Planar data stays in luma, first chroma, second chroma order. NV12 and P010
   use interleaved two-channel chroma.
3. All row-copy plans are computed before allocation. The uploader acquires all pooled planes with
   `COPY_DST`, `COPY_SRC`, and `TEXTURE_BINDING` usage.
4. Block-compatible source strides are written directly with the private device queue. Other rows
   are copied into a tight temporary containing only each logical row prefix. One queue write occurs
   per plane.
5. `UploadedFrame` retains every checkout through an `Arc`. Upload returns no managed fence. Queue
   ordering makes later work on the same device observe the writes, while callers must keep the
   frame alive or retain its textures in the consuming submission.
6. Only the logical texture extent is initialized. Alignment padding in a larger pooled allocation
   is not cleared and must never be sampled or read as valid media.

### GPU-resident pixel conversion

1. `GpuFrameDescriptor` resolves logical format and chroma geometry into one or more physical plane
   layouts. Odd subsampled dimensions use ceiling division. P010 declares six stored low zero bits.
2. `GpuPixelFrame::from_uploaded` validates exact metadata and creates managed views over existing
   upload textures without a CPU repack or GPU copy. It retains the uploaded frame.
3. `GpuConversionPlan` requires equal extents, explicit supported range and matrix metadata,
   unchanged primaries and transfer functions, and no unrepresentable alpha loss.
4. Converter creation checks source sampling and destination render capabilities, compiles generated
   WGSL, creates one source bind group and one render pipeline per destination plane, and may reuse a
   caller-owned `ShaderCache`.
5. Encoding checks exact descriptor equality and usages, then records one full-screen-triangle
   render pass per output plane. Viewports and scissors restrict writes to initialized logical
   physical extents, and subsampled loads clamp to logical chroma bounds.
6. The returned `GpuConversionLease` owns the bind group, destination views, and any uploaded source
   or destination owners. A caller can place that lease in `GpuSubmissionResources` so lifetime
   transfers from command recording to fence retirement.

### Managed pass and submission flow

1. Managed shaders and pipeline layouts are compiled under the per-device FIFO error-scope lock.
   CPU reflection and pipeline metadata become deterministic inputs to pass validation.
2. Compute and render plans own pipelines, bind groups, buffers, texture views, dynamic offsets,
   push constants, attachments, and commands. `GpuPassEncoder` preflights the complete plan before a
   raw pass begins. Rejection does not increment the accepted-pass sequence.
3. Accepted plans encode in call order into one command encoder. The batch retains the original
   plans and therefore their transitive resource graph.
4. The one `GpuSubmissionQueue` for the device consumes command buffers and a queue-scoped owner
   bundle. It assigns a monotonic fence, submits in iterator order, registers completion, and stores
   all retained owners in an in-flight record.
5. Only queue polling or waiting drives callbacks and retires the completed consecutive prefix.
   Retirement releases owners and permits pooled reuse. Fence clones are thread-safe observers, but
   the queue itself is deliberately neither `Send` nor `Sync`.
6. Timed pass batches resolve two timestamps per accepted pass into managed buffers. Mapping may
   complete before the fence, but a timing report is published only after both mapping and queue
   retirement. Labels and media-derived values never enter timing reports.

### Readback and surface sinks

1. A readback request names export or thumbnail intent and validates a device-owned, single-sample,
   two-dimensional, uncompressed color texture with `COPY_SRC`, an in-bounds mip and region, and a
   supported storage format.
2. The manager reserves a padded staging buffer, records one texture-to-buffer copy, and retains
   source plus staging. Submission joins the same queue order as prior render work and starts async
   mapping.
3. Poll or wait requires the originating queue and both callback readiness and fence retirement.
   The result strips wgpu's 256-byte row padding and returns tightly packed storage bytes without
   color conversion, encoding, or swizzling.
4. A native surface retains an owned stable handle provider, filters compatible adapters,
   configures FIFO presentation, and acquires a frame tied to an exclusive surface borrow and the
   configuring device. `submit_and_present` validates queue identity, submits first, then presents
   without waiting for the fence.
5. The production desktop owns four role-addressed surfaces on the sole GPU submission thread.
   `superi-color` builds the selected sRGB or Display P3 presenter above each surface, validates an
   exact active-monitor profile binding before acquisition and again before submission, and retains
   canonical RGBA16F frame ownership through the managed fence. A desktop selection change keeps the
   native child hidden until this owner successfully presents the replacement transform, and the
   desktop rejects queued commands whose revision is no longer current. The GPU crate receives no
   ICC bytes, display-discovery policy, React state, or alternate submission path.

### Device loss and recovery

1. The device-loss callback records the first meaningful loss. New resources and submissions reject
   the obsolete identity, and the submission owner aborts work according to completion-prefix and
   loss-reason rules.
2. Recovery is legal only after confirmed loss. It recreates a device on the same selected adapter
   with the same request, a new identity and queue, and the next generation.
3. Caller-registered recipes execute synchronously in exact order against fresh `GpuResources`.
   Typed keys can access only prior outputs from the same plan. Recovery writes validate
   replacement ownership, `COPY_DST`, ranges, and alignment before using ordered queue writes.
4. Any recipe failure drops all partial GPU owners and withholds the replacement result. Success
   publishes `RecoveredGpu` and ordered labels only after every recipe completes.
5. Recovery does not replay pending work or discover resource graphs. Callers must rebuild texture
   pools, upload frames, caches, shaders, pipelines, readbacks, timing state, and application GPU
   state from surviving CPU data. Every old-generation object remains invalid.

## Dependencies and consumers

### Direct dependencies

- `wgpu` is the backend for native instances, adapters, logical devices, private queues, resources,
  Naga WGSL parsing, pipelines, command encoding, error scopes, mapping, timestamp queries, native
  surfaces, and presentation. The crate publicly re-exports this exact workspace version.
- `superi-core` owns categorized errors, recoverability, contextual diagnostics, field visibility,
  `PixelFormat`, `PixelPacking`, `ChromaSubsampling`, `AlphaMode`, and color-space metadata. These
  types are authoritative for upload geometry, storage conversion, error classification, and safe
  diagnostics.
- `raw-window-handle` defines stable display and window handle provider contracts and desktop handle
  families used by `surface`.
- `sha2` supplies exact WGSL SHA-256 source identities used by shader caching and diagnostics.
- `superi-image` is declared as a direct dependency but current crate source uses it only in the
  cfg(test) conversion contract for a CPU premultiplication reference and image comparison.
- `pollster` is a dev dependency used to block on async device, shader, and pipeline setup in tests
  and the smoke example. `winit` is a target-specific dev dependency for the native smoke example.

### Implemented workspace consumers

- `superi-engine/src/frame_upload.rs` is the production decoder integration. It accepts only
  `FrameStorageKind::Cpu`, preserves timestamp, duration, format, and metadata, maps decoder planes
  into `DecodedFrameUpload`, and returns `UploadedVideoFrame` owning the `UploadedFrame`. GPU and
  external decoder storage currently return a degraded unsupported result instead of causing an
  implicit download.
- `superi-color/src/working_space.rs` uses `GpuFrameDescriptor` to define canonical GPU working
  storage as one premultiplied `Rgba16Float` plane with explicit scene-linear wide-gamut metadata.
  It validates the descriptor but does not ask `superi-gpu` to change primaries or transfer.
- `superi-color/src/gpu_transform.rs` uses managed textures, shader caching, explicit binding and
  pipeline layouts, compute-pass batches, the exclusive submission queue, and fence-scoped retained
  owners to execute wide-gamut color transforms without an ordinary CPU pixel path.
- `superi-color/src/gpu_display.rs` uses the same managed texture, device, native frame, queue, and
  fence-retention contracts to render a canonical working texture directly into a display
  attachment without readback.
- `superi-color/src/view.rs` wraps `NativeViewportSurface` with immutable monitor and ICC evidence.
  It delegates adapter filtering, configuration, acquisition, and submit-before-present to this
  crate, but rejects presentation if monitor/profile evidence changed after acquisition.
- `app/src-tauri/src/viewport.rs` creates four production native hosts on the UI thread, moves their
  surfaces into the sole GPU submission domain, composes exact per-role monitor freshness guards and
  explicit sRGB or Display P3 `GpuDisplayPresenter` instances through `superi-color`, and waits for
  retained presentation work before source or host teardown. Geometry updates and color selection
  remain separate strict shell commands, and neither transports pixels, ICC bytes, or GPU handles.
- `superi-concurrency` defines a blocking-capable `GpuSubmission` execution domain. Its integration
  contract constructs the real non-Send `GpuSubmissionQueue`, submits, and waits inside that owned
  thread. The GPU crate documents this placement but does not itself enforce the execution-domain
  enum at construction time.
- `superi-cache` uses `GpuMemoryPool`, `MemoryClass::Cache`, `MemoryEvictor`, and the non-cloneable
  memory reservation inside its device-resident frame-cache entries. It admits the same exact
  managed payload bytes through cache total, project, and device limits before GPU cooperation,
  rolls local scopes back after GPU refusal, and releases both owners with the retained entry. A
  retryable GPU-only refusal can now make the cache release an eligible LRU entry from the matching
  device before retrying the unchanged reservation path without a tier lock held.

### Declared and prospective consumers

- `superi-effects` and `superi-graph` declare `superi-gpu` dependencies and name it in their crate
  ownership docs, but their current public modules contain no concrete GPU call sites.
- `TextureReadbackManager`, GPU timing, and aggregate snapshots have strong public contract tests,
  but workspace search finds no non-test export, thumbnail, or telemetry integration yet.
- Application render and conversion coordinators must retain every owner referenced by arbitrary
  raw command buffers. Managed pass, readback, timing, and surface paths construct the retention
  chain, while raw wgpu encoding cannot infer it automatically.

## Invariants and operational boundaries

### Identity, ownership, and scheduling

- Device identity, not manager scope or adapter metadata, is the interoperability key. Recovered,
  foreign, or additional-adapter resources produce a user-correctable conflict before use.
- One `GpuSubmissionQueue` may claim one device at a time. Queue-scoped fences and retention bundles
  become stale when that owner drops, even if a new owner wraps the same healthy device.
- The submission queue is thread-affine. Polling, waiting, retirement, presentation, and queue drop
  belong on the dedicated blocking-capable GPU submission thread. Fences and immutable managed
  owners may be observed or moved across threads where their types permit it.
- In-flight resource safety is explicit. Arbitrary command buffers are safe only when every pooled
  checkout or compound managed owner they reference is retained until the fence retires.
- Resource wrappers expose raw borrows for wgpu integration. This does not expose the queue and does
  not make immutable creation snapshots reflect later raw-handle state such as buffer mapping.

### Validation and memory

- Empty usages, malformed binding ranges, foreign owners, invalid views, unsupported format
  capabilities, and malformed pass state are rejected before native work wherever the crate has
  enough information. Detailed wgpu descriptor and hazard validation remains the backend boundary.
- The pass layer validates declared state and attachment aliasing, not arbitrary read/write hazards
  across bind-group resources, command buffers, passes, or submissions. Queue order and wgpu
  validation remain authoritative below it.
- `GpuMemoryPool` measures deterministic Superi-managed payload bytes. It does not measure driver
  metadata, heap granularity, backend suballocation, migration, or total process GPU memory.
- Memory reservations serialize pending accounting so concurrent allocations cannot cross the hard
  limit. Evictors run synchronously in caller order and must not reserve from the same pool, which
  would deadlock. Participant-reported releases are diagnostic; observed resident-byte changes
  decide allocation acceptance.
- Slow subscribers receive the newest coalesced pressure edge, not a complete history. External
  platform pressure must be translated explicitly through `apply_external_pressure`.
- Pool reuse requires exact descriptor-key compatibility. The ordered `view_formats` vector is part
  of the key, and equivalent sets in another order do not reuse one allocation.

### Pixel and transfer boundaries

- Upload preserves decoded bits and plane order. Integer texture storage used for exact preservation
  must be interpreted through retained `PixelFormat` metadata rather than sampled as normalized data
  by assumption.
- Logical texture extent can be smaller than physical allocation extent. Only initialized logical
  rows and columns may affect conversion, rendering, readback, or diagnostics.
- Storage conversion never resizes, changes primaries, or changes transfer functions. It requires
  explicit color range and matrix signaling, refuses silent alpha loss, and applies no dithering.
- Conversion uses generated bilinear chroma reconstruction and location-dependent point or box
  downsampling. Floating-point outputs preserve negative and above-one values where the destination
  format permits them.
- Upload and recovery initialization use internal queue writes and return no managed fence. Their
  completion guarantee is same-queue ordering. Readback, passes, timing, and presentation use the
  managed fence stream.
- Readback supports only explicit export or thumbnail of single-sampled, uncompressed 2D color
  storage. Multisample resolve, depth/stencil, compression, 1D or 3D textures, color conversion,
  swizzling, encoding, and streaming rows belong above or before this boundary.

### Platform and failure boundaries

- Native surfaces support AppKit on macOS, Win32 and WinRT on Windows, and Xlib, XCB, and Wayland on
  Linux. Display and window handle families must match and match the compiled target.
- Adapter enumeration and surface-compatible adapter listing are absent on `wasm32`; this crate has
  no browser adapter-request path.
- FIFO presentation is required. The implementation prefers sRGB output, opaque alpha, and maximum
  frame latency two. macOS surface creation on the operating-system main thread is a caller contract,
  not a local runtime assertion.
- The Linux smoke dev dependency enables X11, while the surface implementation itself recognizes
  Xlib, XCB, and Wayland handles. The smoke example is not proof of every Linux window system.
- Invalid caller input is generally `InvalidInput` or `Unsupported` and user-correctable. Foreign or
  stale lifetimes are `Conflict`. Device loss, unavailable native handles, and memory pressure may be
  retryable. Identity or arithmetic exhaustion and poisoned critical ownership state are terminal.
- No owned Rust source file introduces an explicit unsafe block. Native handles, backend drivers,
  shader execution, and wgpu validation remain external safety and platform boundaries.

## Tests and verification

The crate has four internal contract modules and ten public integration-test files. Internal tests
can exercise crate-private helpers; files under `tests/` import only the exported `superi_gpu`
surface. Together they cover the following:

- Adapter policy, feature and limit rejection, deterministic identity, independent multi-adapter
  devices, exclusive submission ownership, monotonic fences, ordered completion-prefix retirement,
  stale queue state, and device-loss abort behavior.
- Managed resource creation, process-local IDs, live counts, ownership conflicts, transitive
  binding and pipeline retention, raw compute and render execution, and classified validation
  errors.
- WGSL parsing, reflection, exact cache hits, failure non-caching, LRU eviction, concurrent
  error-scope ordering, pipeline stage and layout compatibility, and escaped shader lifetime.
- Compute and render plan preflight, stable accepted-pass order, deferred-state rejection, dynamic
  offset and buffer ranges, attachment validation, pipeline retention, real exact compute output
  `0x12345678`, and exact rendered RGBA output.
- Format-to-plane mappings, odd chroma geometry, packed RGB storage, exact conversion plans,
  per-format shader compilation, representative YUV, alpha, packed, high-bit-depth, and limited
  range execution, upload-to-conversion zero-copy ownership, and one CPU alpha reference.
- Direct and repacked decoded upload, byte preservation, planar YUV420, NV12, P010, aligned reuse,
  and final-clone checkout return.
- Budget validation, RAII totals, pressure coalescing, ordered cooperative eviction, concurrent hard
  limit safety, mip/sample/compressed/NV12 payload accounting, aligned pool reuse, escaped-owner
  discard, and cross-pool idle reclamation.
- Readback preflight, padded staging, exact RGBA8 and RGBA16F storage bytes, queue ordering,
  thumbnail subregions, cancellation, and old-generation rejection.
- Privacy-safe snapshots and events, timestamp feature and capacity checks, ordered compute/render
  timings, foreign-queue rejection, and retirement of unread timing resources.
- Typed ordered recovery, replacement initialization writes, new generation and scope, safe observer
  notices, resumed submission and readback, obsolete resource rejection, and all-or-nothing cleanup.
- Desktop handle classification, target reporting, viewport extent rules, retryable handle
  unavailability, and one manual native winit clear, present, and wait smoke path.

Most real GPU tests return early when no acceptable adapter is available. Timestamp tests also skip
their native section when `TIMESTAMP_QUERY` is unavailable. A green adapterless or feature-limited
CI run therefore proves CPU validation and public type contracts but not shader compilation, GPU
execution, device loss, timing, or presentation. The manual viewport smoke proves only one native
frame and does not cover resize, occlusion, repeated acquisition, surface loss, monitor migration,
or recovery.

Numerical conversion proof is representative rather than exhaustive. All public formats compile,
but the suite does not independently reference every YUV equation, chroma kernel, range transform,
or pixel format. Round trips can hide symmetric encode and decode defects. A hardware-backed lane
and broader one-way reference vectors remain important verification needs.

## Current status and risks

The module is substantive across every owned source file. It implements device and resource
ownership, decoded upload, conversion, passes, queue retirement, memory pressure, readback,
diagnostics, native presentation, and explicit recovery. It contains no scaffold-only production
path. The desktop now integrates four native presentation surfaces through `superi-color`, while
several other boundaries intentionally stop short of application policy or full integration.

- Cross-adapter transfer and synchronization are absent. Multi-adapter support is independent
  selection and device ownership only.
- GPU and external decoder surfaces are not imported. The engine consumer rejects them with degraded
  unsupported status and relies on decode selection to provide CPU frames rather than downloading
  implicitly.
- Queue writes from upload have no per-upload fence or progress entry. Prematurely dropping an
  uploaded owner before consuming submission retention could permit pool reuse while GPU work still
  references the texture.
- Aligned upload allocations contain uninitialized padding. A consumer that uses physical allocation
  extent instead of logical initialized extent can read stale pooled contents and create correctness
  or privacy defects.
- Raw wgpu command encoding can bypass managed pass preflight and cannot be inspected for retained
  dependencies. Correct manual `GpuSubmissionResources` construction remains a caller obligation.
- Pipeline constructors allow automatic wgpu layouts, but managed passes with reflected bindings
  require explicit managed layouts so bind-group identity can be proven.
- The memory budget is portable accounting, not physical residency. Slow synchronous evictors block
  allocation, and faulty participant release counts can skew diagnostics even though they cannot
  bypass hard-limit decisions.
- Shader cache capacity is by entry count, while keys retain full label and source text. Concurrent
  misses may duplicate backend compilation before the second insertion resolves to one canonical
  cached module.
- Wait and queue drop have no overall deadline. A backend that never services callbacks and never
  reports loss can block the GPU submission thread indefinitely. Some test poll loops are likewise
  unbounded.
- Recovery is same-adapter, explicit, process-local reconstruction. It is not transparent replay,
  durable serialization, adapter failover, or rollback of external callback side effects. Tests do
  not cover spontaneous driver loss, adapter disappearance, repeated loss during reconstruction,
  or out-of-memory recovery.
- Surface automation is mostly handle-level. Real AppKit, Win32, WinRT, X11, XCB, and Wayland
  configure, acquire, resize, present, and loss behavior needs platform hardware coverage.
- The desktop color consumer currently selects only built-in SDR sRGB and Display P3 transforms.
  Exact monitor and ICC freshness are enforced above this crate, but arbitrary ICC tag evaluation,
  HDR display modes, cross-adapter presentation, and non-macOS system-profile discovery remain
  outside the GPU substrate and are not implied by successful native submission.
- Public readback, timing, and snapshot APIs are contract-tested but not yet connected to non-test
  workspace export, thumbnail, or telemetry consumers. Cache now consumes portable memory
  accounting for budgeted retained values, while declared GPU dependencies in effects and graph
  remain skeleton relationships.
- Diagnostic snapshots and timing reports are explicitly user-safe, but shader and general error
  context can contain caller labels. Those errors require a separate redaction decision before
  entering user-safe telemetry.

## Maintenance notes

- Recompute the hash and file count with
  `python3 .agents/skills/superi-mapping/scripts/codebase_maps.py hash superi-gpu` after any owned
  source change. Refresh this map's prose, inventory, and `mapped_at_commit`; never update only the
  hash.
- Preserve the device-identity rule when adding managers or consumers. Manager scope is for
  accounting, queue identity scopes fences and retention builders, and generation replacement must
  invalidate old GPU owners.
- Any new raw command path must document how every referenced allocation is retained through the
  matching fence. Any new pooled upload or conversion path must distinguish logical initialized
  extent from physical allocation extent.
- New upload formats require coordinated changes to lossless plane layout, `superi-core` pixel
  metadata interpretation, uploader row planning, conversion sampling and packing, payload sizing,
  and one-way GPU reference tests.
- New public readback formats must define tight storage bytes, wgpu copy alignment, row stripping,
  and whether higher layers perform color or alpha transforms. Do not turn readback into an implicit
  conversion boundary.
- Changes to submission, loss, polling, timing, readback, upload, or recovery must preserve the
  distinction between internal queue writes, managed command submission, callback completion,
  consecutive retirement, and user-visible result readiness.
- Surface changes must be reconciled with `superi-color/src/view.rs`; upload changes with
  `superi-engine/src/frame_upload.rs`; queue placement and blocking guidance with
  `superi-concurrency/src/threads.rs` and its GPU integration contract.
- Changes to native presentation must also be reconciled with the four-role desktop owner, its
  separate geometry and color commands, the exact monitor-binding freshness checks, both built-in
  output transforms, and the rule that ICC bytes and pixels never cross React IPC.
- Memory-pool and reservation changes must be reconciled with `superi-cache::eviction` and
  `CacheMemoryPlacement::Device`, preserving exact managed-byte equality, rollback after refusal,
  matching-device LRU release before retry, and lock-free pressure cooperation across the cache tier
  boundary.
- Run the crate's contract suite on a real adapter and the manual native viewport smoke when changes
  touch backend execution or presentation. Record skipped GPU sections separately from passing CPU
  validation so adapterless runs are not overstated.
