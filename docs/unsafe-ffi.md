# Unsafe FFI boundaries

Superi denies Rust `unsafe` code by default. The open tree currently permits it only inside the
audited target boundary modules listed here. Every unsafe block and unsafe trait implementation
must have a local `SAFETY:` comment that states the concrete pointer, length, lifetime, ownership,
or threading invariant that makes the operation valid.

The public boundary remains `superi-media-io`. Sources, packets, video frames, audio blocks,
decoders, encoders, backend registrations, operation cancellation, and typed errors cross that
interface as safe Rust values. Raw handles, callback pointers, native allocation addresses, and
foreign object lifetimes do not cross it.

## Enforced policy

`open/Cargo.toml` applies these workspace rules:

- `unsafe_code = "deny"` rejects unsafe code unless an audited boundary grants a narrower local
  allowance.
- `unsafe_op_in_unsafe_fn = "deny"` requires each unsafe operation to remain visible in its own
  unsafe block, even inside an unsafe function.
- `clippy::undocumented_unsafe_blocks = "deny"` rejects an unsafe block or unsafe implementation
  without a directly associated safety justification.

An unsafe allowance is not proof by itself. It identifies the source scope that must be reviewed
against this inventory. New native dependencies, modules, callbacks, raw handles, or unsafe trait
implementations require an inventory update and target-specific Clippy proof in the same change.

## Boundary inventory

### macOS CoreGraphics display profile discovery

- Source: `open/crates/superi-color/src/icc/macos.rs`
- Dependency and target: pinned `objc2-core-graphics` framework bindings on macOS only
- Safe entry: `SystemDisplayProfileDiscovery` through `DisplayProfileDiscovery`, then
  `DisplayProfileCatalog` and the monitor-aware viewport presentation owner
- Unsafe surface: `CGGetActiveDisplayList` count queries and the bounded active-display ID fill
- Pointer and count rules: the count query passes a null display-list pointer only with a zero
  maximum. The fill allocates exactly the previously validated count, passes that exact length,
  and keeps the count output live for the complete call. A confirmation query rejects display-set
  changes instead of accepting a truncated or stale list.
- Retained ownership: `CGDisplayCopyColorSpace` returns a retained color-space owner through the
  generated binding. `CGColorSpace::icc_data` returns retained Core Foundation data, and Superi
  copies those bytes into an `Arc<[u8]>` before the native owners are released.
- Threading: the application shell invokes discovery from its display-event owner. Published
  profiles and snapshots are immutable safe Rust values and contain no raw CoreGraphics handles.
- Failure and fallback: CoreGraphics status codes, zero displays, resource limits, and display-set
  races become typed errors. A display with no exported ICC bytes remains explicitly unprofiled;
  no sRGB profile or profile from another monitor is guessed.
- Target proof: macOS focused tests exercise the count limit and real active-display query when a
  display server is available. Strict Clippy with undocumented unsafe blocks denied checks both
  `CGGetActiveDisplayList` calls and their local `SAFETY:` invariants.

### AV1 through rav1d

- Source: `open/crates/superi-codecs-rs/src/av1.rs`
- Dependency and target: pinned `rav1d` 1.0.0 on every supported target
- Safe entry: `Av1Backend` through `MediaBackend`, `Decoder`, and codec registration
- Unsafe surface: dav1d-compatible context creation, packet allocation and copy, send and receive,
  picture references, signed-stride plane reads, flush, and destruction
- Ownership: `Rav1dHandle` uniquely owns one optional context. `OwnedDav1dPicture` releases one
  returned picture exactly once. Decoded planes are copied into validated Superi-owned storage
  before the native picture is released.
- Threading: the unique handle may move between threads but is never accessed concurrently.
- Failure and fallback: native status codes become typed Superi errors. The backend keeps the stable
  `rust-av1` identity and never silently changes codec or precision.

The allowance is scoped to the ownership implementations, pointer read, and plane-copy function,
not to the complete public module.

### Opus through libopus

- Source: `open/crates/superi-codecs-rs/src/opus.rs`
- Dependency and target: pinned statically bundled `libopus_sys` 0.3.3 on every supported target
- Safe entry: `OpusBackend` through `MediaBackend`, `Decoder`, and `Encoder`
- Unsafe surface: packet sample queries, decoder and encoder creation, variadic control requests,
  packed sample decode and encode, reset, and matching destruction for single-stream and
  multistream state
- Ownership: each `NativeDecoder`, `NativeMsDecoder`, `NativeEncoder`, or `NativeMsEncoder` owns one
  nonnull native state and destroys it with the matching function exactly once.
- Buffer rules: every packet, mapping, input sample, and output slice is checked against the exact
  byte or per-channel frame count passed to libopus.
- Threading: owned states may move between threads, but safe methods require exclusive mutable
  access and never call one state concurrently.
- Failure and fallback: all negative libopus status values become typed media failures. Timing,
  channel layout, metadata, padding, and the stable `rust-opus` identity remain in safe code.

The former module-wide allowance is intentionally split across only the packet query and native
ownership implementations.

### VP8 and VP9 through the libvpx shim

- Rust source: `open/crates/superi-codecs-rs/src/vpx_ffi.rs`
- C source: `open/crates/superi-codecs-rs/src/vpx_shim.c` and `vpx_shim.h`
- Dependency and target: official libvpx 1.16 ABI loaded at runtime on every supported target
- Safe entry: `VpxBackend` through `MediaBackend`, `Decoder`, and `Encoder`
- Unsafe surface: dynamic-library loading, version and function-symbol lookup, opaque decoder and
  encoder handles, shim calls, native error strings, and borrowed packet bytes
- ABI isolation: Rust transports erased function addresses only to the local C shim. The shim owns
  concrete libvpx structs, restores exact public C signatures, validates arguments and geometry,
  and exposes fixed-width Superi result structs.
- Ownership: `Runtime` retains the library for every copied symbol address. Decoder and encoder
  handles each own one opaque shim context and destroy it exactly once.
- Buffer rules: decoded planes copy into Rust vectors whose stride and row count match a checked
  layout. Encoded packet bytes are copied before the next shim call can invalidate libvpx storage.
- Threading: the immutable function table is shared with the retained library. Each context remains
  uniquely owned and is never used concurrently.
- Failure and fallback: missing or incompatible libraries produce an unavailable backend error.
  The stable `libvpx` identity and explicit registry fallback rules remain unchanged.

`vpx_ffi.rs` is a private module dedicated to this boundary, so its module allowance cannot expose
raw FFI values through the public crate API.

### macOS VideoToolbox, CoreMedia, and CoreVideo

- Source: `open/crates/superi-codecs-platform/src/videotoolbox/macos.rs`
- Dependency and target: pinned `objc2` framework bindings on macOS only
- Safe entry: `VideoToolboxBackend` through the safe platform registry and media interfaces
- Unsafe surface: session creation and invalidation, decoder and encoder submission, callbacks,
  retained Core Foundation object conversion, format descriptions, sample and block buffers,
  CoreVideo pixel-buffer locks, bounded CPU copies, and CoreMedia time values
- Callback lifetime: decoder and encoder state is boxed before session creation. The stable pointer
  remains registered until the owner drains or invalidates the session. The owner's `Drop`
  implementation invalidates the session while both the session and boxed state are still live, so
  no callback may observe the later field teardown.
- Retained ownership: every successful create result becomes one `CFRetained` owner. Callback image
  buffers are retained before leaving the callback. Borrowed sample and format objects never outlive
  the callback or retained parent.
- Buffer rules: compressed bytes and parameter sets remain live for synchronous create calls.
  CoreVideo CPU writes occur only between a matching lock and unlock and copy no more than the
  checked source and destination row span.
- Threading: session mutation is serialized by exclusive decoder or encoder access. Callback queues
  use a mutex and first-packet state uses an atomic. The frame wrapper exposes immutable shared
  access to a retained CoreVideo image.
- Failure and fallback: every OS status becomes a typed error. Native frame ownership, exact timing,
  metadata, alpha behavior, the `apple-videotoolbox` identity, and explicit fallback remain visible
  through safe media contracts.

The `macos` module is private and compiled only for macOS. Its module allowance contains the native
framework boundary while `videotoolbox.rs` remains a safe cross-platform adapter.

### macOS AudioConverter

- Source: `open/crates/superi-codecs-platform/src/videotoolbox/macos/aac.rs`
- Dependency and target: pinned AudioToolbox and Core Audio bindings on macOS only
- Safe entry: AAC creation delegated by `VideoToolboxBackend`
- Unsafe surface: converter creation, properties, synchronous fill calls, input callback pointers,
  buffer-list mutation, reset, and disposal
- Callback lifetime: `InputContext`, packet descriptions, byte slices, packet counts, and output
  lists are live local storage for the complete synchronous `AudioConverterFillComplexBuffer` call.
- Ownership: `AudioConverter` owns one nonnull converter reference and disposes it exactly once.
- Buffer rules: callback offsets and packet counts are bounded by the remaining input slice. Output
  capacity and packet-description counts are allocated before the native call and checked before
  constructing safe packets or audio blocks.
- Threading and failure: converter calls use exclusive mutable ownership. OS status codes become
  typed failures without changing timing, channel layout, metadata, or backend selection.

This private child of the macOS boundary has its own module documentation because its callback and
buffer ownership differ from the VideoToolbox video sessions.

### macOS Audio Unit effect hosting

- Source: `open/crates/superi-audio/src/hosting/audio_unit_macos.rs`
- Dependency and target: pinned `objc2-audio-toolbox`, `objc2-core-audio-types`, and `block2`
  bindings on macOS only
- Safe entry: `AudioUnitHostConfig` and `PreparedAudioUnit` through the existing
  `AudioProcessor` graph boundary
- Unsafe surface: component discovery and identity reads, asynchronous instantiation, property
  negotiation, stream and channel-layout configuration, render callback registration, bounded
  planar buffer-list access, synchronous rendering, uninitialization, and disposal
- Instantiation ownership: the escaping completion block retains shared state until one result is
  delivered or the bounded waiter abandons it. One send wrapper transfers the unique instance
  through the completion mutex without dereferencing it; late, duplicate, failed, or abandoned
  completions dispose every nonnull instance exactly once.
- Callback lifetime: the callback context is boxed before registration and remains stable until
  `Drop` first deactivates and uninitializes the unit, then disposes the instance. Input plane
  allocations and the fixed output list remain live through every synchronous `AudioUnitRender`.
- Buffer and timing rules: preparation fixes the maximum slice, planar native float format, exact
  sample rate, and ordered channel meaning. The callback accepts only bounded subranges inside the
  published integral sample window, copies or publishes one preallocated plane per channel, and
  rejects malformed buffer lists. Processing commits caller output only after exact buffer shape,
  finite samples, callback status, and an exactly representable `AudioTimeStamp` are verified.
- Process isolation: out-of-process loading is the default policy and is verified from
  `kAudioUnitProperty_LoadedOutOfProcess`. In-process execution requires an explicit audited policy;
  no silent fallback satisfies a required isolation request.
- Threading: discovery, instantiation, configuration, and initialization require the blocking
  background domain. The prepared instance moves once into graph ownership, and exclusive mutable
  processing runs on the audio domain without host allocation or locks on the successful path.
- Failure and fallback: native render, callback, malformed output, or nonfinite output failure
  poisons the instance and prevents reuse. Prevalidation leaves native state, graph continuity, and
  caller output untouched. Drop tears down the native unit while all registered storage is live.

The public hosting module contains only safe values and a safe processor implementation. The
macOS-only private child contains the complete native allowance and exposes no raw handle, pointer,
buffer list, or callback type.

### Windows Media Foundation and COM

- Source: `open/crates/superi-codecs-platform/src/media_foundation_windows.rs`
- Dependency and target: pinned `windows` 0.61.3 bindings on Windows only
- Safe entry: `MediaFoundationBackend` through the safe platform registry and media interfaces
- Unsafe surface: COM and Media Foundation startup, transform enumeration and activation, media
  type attributes, transform lifecycle and processing, samples, activation arrays, output
  descriptors, locked byte buffers, and shutdown
- Thread ownership: discovery, each decoder, and each encoder run on dedicated worker threads. A
  `ComMfRuntime` guard initializes COM and Media Foundation on that thread and shuts both down on
  the same thread after dependent objects are released.
- Allocation ownership: the activation array returned by `MFTEnumEx` is converted entry by entry,
  each COM reference is taken once, and the original array is freed once with `CoTaskMemFree`.
  `ManuallyDrop` output fields are initialized before `ProcessOutput` and extracted exactly once
  afterward.
- Buffer rules: input bytes copy only into a successfully locked allocation of the requested size.
  Output bytes copy only from the current length reported by a successfully locked contiguous
  buffer. Every lock has a matching unlock.
- Capability and fallback: only discovered synchronous transforms are advertised. Native failures,
  unsupported formats, stream changes, timing, color, alpha limits, and cancellation become typed
  safe results under the stable `windows-media-foundation` identity. No hardware or asynchronous
  transform is claimed by this CPU boundary.

The `windows` module is private and compiled only on Windows. `media_foundation.rs` retains the safe
configuration parsers and cross-platform identifiers outside the allowance.

### Linux VVC through VA-API 1.22

- Source: `open/crates/superi-codecs-platform/src/vvc_vaapi_linux.rs`
- Dependency and target: generated bindings for system `libva` 2.22 or newer on Linux only
- Safe entry: `VaapiBackend` through the safe platform registry, `Decoder`, and frame interfaces
- Unsafe surface: DRM display creation, VA initialization, configuration, contexts, P010 surfaces,
  picture, slice, ALF, and LMCS buffers, submission, synchronization, DMA-BUF export, and matching
  destruction
- Ownership: `VvcVaapiDecoder` keeps the render-node file alive for the display lifetime and drops
  context, configuration, and display in dependency order. Parameter buffers are destroyed after
  submission. Every exported file descriptor becomes exactly one `File` owner or is closed on a
  rejected descriptor.
- Buffer rules: all syntax reaches FFI through fixed-layout generated structs or immutable slice
  bytes whose exact length is supplied. Exported object, layer, plane, object-index, format, pitch,
  offset, and modifier values are bounded before safe frame construction.
- Threading: the display and every dependent VA object remain on one decoder worker thread. Only
  owned DMA-BUF files plus plain layout values cross into the safe immutable frame wrapper.
- Capability and fallback: VVC is advertised only after a Main 10 VLD configuration with P010
  render format can be created. Unsupported syntax is rejected before submission, and every native
  status becomes an explicit failure without changing backend identity or falling back silently.

The raw generated bindings remain private to this Linux-only module. No raw display, context,
surface, buffer identifier, pointer, or exported descriptor crosses the safe module boundary.

### VST3 worker module hosting

- Source: `open/crates/superi-audio/src/hosting/vst3/native.rs`
- Dependencies and targets: exact `vst3` 0.3.0 and `com-scrape-types` 0.1.1 on macOS, Windows, and
  Linux; pinned `objc2-core-foundation` 0.3.2 on macOS; pinned `libloading` 0.8.9 on Windows and
  Linux
- Safe entry: `Vst3WorkerSession::load` creates one worker-local session, prepared graph effect,
  bounded automation writer, and control-side readings for an explicit module path and class ID
- Unsafe surface: platform module entry, exit, and factory symbols; retained dynamic-library or
  Core Foundation bundle handles; VST3 COM factory, component, processor, and controller calls;
  host callback objects; planar audio pointers; process context; parameter queues; and explicit
  COM ownership transfer
- Module ownership: the platform loader retains executable code through every copied symbol and
  COM object. macOS pairs `bundleEntry` and `bundleExit` around one retained `CFBundle`; Windows
  pairs optional `InitDll` and `ExitDll` around one retained library; Linux passes the actual
  `dlopen` handle to `ModuleEntry`, reconstructs its unique library owner, and later calls
  `ModuleExit`. Module unload occurs only after reverse processor, component, controller, factory,
  and callback teardown. Each bus and connection direction has independent unwind state. Any failed
  reverse lifecycle call retains the unresolved owners and module for retry or worker-process exit.
- Interface ownership: `ComPtr` owns every acquired factory or plugin reference. The host and
  bounded message, attribute, and parameter-change objects are retained `ComWrapper` values.
  Host-created binary buffers and message identifiers retain stable backing storage under explicit
  count and byte caps. The session refuses shutdown while a prepared processor lease exists, and an
  unretired lease intentionally leaks until worker process exit instead of unloading code reachable
  by an audio callback.
- Buffer rules: configuration bounds frame, channel, automation, and monitoring storage before
  load. The process bridge exposes exactly one main input and output with an equal canonical mono,
  stereo, quad, 5.1, or 7.1 layout. It builds channel pointers only inside checked preallocated
  planar slices, passes one exact `ProcessData`, and copies finite f32 output back into the existing
  interleaved graph buffer. Parameter queues use fixed prepared point arrays and exact block-local
  sample offsets. Planar and handoff storage each cap at 1,048,576 elements, parameter metadata caps
  at 4,096 entries, and combined parameter-point cells cap at 1,048,576.
- Threading: construction, controller interaction, bus negotiation, activation, and teardown run
  on the plugin worker control path. One prepared effect alone performs process calls on
  `ExecutionDomain::Audio`. Host message and attribute operations fail on that domain before
  allocating or taking a mutex. The explicit `Send` and `Sync` implementations are justified by
  that unique process lease and the control-side strong-count shutdown proof, not by concurrent
  plugin access.
- Capability and failure: the host rejects instruments, event buses, auxiliary or sidechain buses,
  multiple main buses, unsupported speaker arrangements, f64-only processing, invalid metadata,
  nonfinite samples, and failed lifecycle calls. Latency and tail are reported but delay
  compensation, module discovery, validation databases, state persistence, supervision, restart,
  quarantine, and recovery remain outside this boundary. Parameters must be both automatable and
  writable for input automation, and any requested optional process-context field fails setup
  rather than receiving guessed timing or transport data.
- Runtime proof: `vst3_host_contract` compiles a standards-shaped VST3 fixture into a temporary
  bundle, loads it only in isolated child processes, and proves every supported layout through a
  source, hosted effect, submix, and master. It also proves exact sample-offset automation,
  block-boundary retention, output monitoring and overflow, process context, real-time and offline
  modes, bounded host message and attribute behavior, read-only automation and optional-context
  rejection, partial-activation unwind, failed-stop mapped-code retention, callback allocation
  counts, lease-gated shutdown, reverse lifecycle order, and module exit evidence.

The public `hosting::vst3` module exports only safe owned values. Raw VST3 identifiers, interfaces,
callbacks, pointers, and platform loader types remain private to this audited file.

## Native dependencies without Superi unsafe blocks

Vorbis uses safe `vorbis_rs` ownership on a dedicated worker. Its sys crates are linked but Superi
does not call their raw functions. Linux H.264, HEVC, and H.264 encode use safe `cros-codecs` and
`libva` ownership in Superi source; Linux VVC is inventoried separately above. wgpu and the current
image, container, and color paths use safe Rust APIs. macOS Audio Unit hosting is inventoried
separately above. Third-party crate internals and vendored libvpx headers are governed by dependency
pinning, licensing, and upstream review, but they are not Superi-owned Rust unsafe boundaries.

## Required audit

Run from `open/` after any change to a native boundary:

```text
rg -n --glob '*.rs' '\bunsafe\b' crates
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo clippy -p superi-codecs-platform --target x86_64-pc-windows-msvc --lib -- -D warnings
cargo clippy -p superi-audio --all-targets -- -D warnings
cargo test -p superi-codecs-rs -p superi-codecs-platform --all-targets
cargo test -p superi-audio --test vst3_host_contract
cargo test -p superi-engine --all-features
```

The source scan must match this inventory. Host Clippy audits macOS in a macOS checkout. The Windows
target command audits the cfg-gated Media Foundation module without requiring a Windows runtime.
Real native lifecycle tests must still run on their owning operating system before release.
