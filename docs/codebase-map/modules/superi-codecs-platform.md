---
module_id: superi-codecs-platform
source_paths:
  - open/crates/superi-codecs-platform
source_hash: e7ed76ba90d9f515eb3920d79a11fa2528f416f8e7cfb8ab4f93890fefb82acf
source_files: 15
mapped_at_commit: 217e9d48703bcfd4736d949aea510c94505071bc
---

## Purpose and ownership

`superi-codecs-platform` is the opt-in adapter layer between the safe, codec-neutral
`superi-media-io` contracts and codec implementations supplied by the host operating system or its
installed driver stack. It owns native H.264, HEVC, VVC, ProRes, and AAC registration, codec
configuration normalization, session construction, packet and frame translation, native resource
lifetimes, and platform-specific error conversion. It does not own containers, demux, source
probing, backend selection policy, or the royalty-free default codec implementations.

The crate is reached through the `superi-engine` feature `os-codecs`. With that feature disabled,
the engine has no dependency on this crate and its default permissive codecs remain the complete
registry. With it enabled, one target-specific adapter may add operations supplied by the current
host:

- macOS uses VideoToolbox for video and AudioConverter for AAC under backend ID
  `apple-videotoolbox`.
- Windows uses runtime-discovered synchronous Media Foundation transforms under backend ID
  `windows-media-foundation`.
- Linux uses installed VA-API drivers for H.264 and HEVC decode, H.264 encode, and a bounded VVC
  Main 10 decode path under backend ID `linux-vaapi`.
- Other targets contribute no registration.

This ownership is also a legal boundary. The crate and workspace source are MIT, while
patent-encumbered transforms execute in the user's OS framework or installed VA driver. The
default build remains free of this dependency. The crate must not acquire a bundled software
H.264, HEVC, VVC, ProRes, or AAC implementation, nor link a GPL, LGPL, MPL, or other disallowed
dependency. `docs/codecs.md` records the governing codec policy and still marks codec-boundary
legal review and AAC patent status as unresolved legal verification, so this map describes the
implemented isolation but does not claim that review is complete.

## Source inventory

- `open/crates/superi-codecs-platform/Cargo.toml` defines the MIT workspace package, safe Superi
  dependencies, target-specific native binding dependencies, Linux codec/parser dependencies, and
  the pinned unconditional `bindgen` build dependency.
- `open/crates/superi-codecs-platform/build.rs` is the Linux-only libva binding generator. It finds
  headers through `CROS_LIBVA_H_PATH` or `pkg-config`, requires libva package 2.22 or newer and
  `va/va_dec_vvc.h`, allowlists the VVC-facing VA ABI, and writes `OUT_DIR/libva_vvc.rs`. It returns
  without generation on non-Linux targets.
- `open/crates/superi-codecs-platform/src/lib.rs` is the public crate root. It documents the
  opt-in boundary and exposes the `media_foundation`, `register`, `vaapi`, and `videotoolbox`
  modules.
- `open/crates/superi-codecs-platform/src/media_foundation.rs` owns cross-platform Media
  Foundation codec and operation identities, ProRes profiles and FourCCs, AVC and HEVC
  configuration parsing, length-prefixed to Annex B conversion, AAC AudioSpecificConfig and ESDS
  parsing, and the Windows-only backend re-export.
- `open/crates/superi-codecs-platform/src/media_foundation_windows.rs` is the private Windows COM
  and Media Foundation implementation. It discovers transforms, implements the codec-only
  `MediaBackend`, runs decoder and encoder workers, negotiates native types, translates safe media
  values, and owns all COM allocations and native lifecycles.
- `open/crates/superi-codecs-platform/src/register.rs` selects the one applicable host adapter,
  preflights backend IDs, installs registrations into a caller-owned registry, and constructs a
  platform-only registry.
- `open/crates/superi-codecs-platform/src/vaapi.rs` is the shared VA-API facade. It owns stable
  codec IDs, capability projection, AVC/HEVC/VVC Annex B normalization, timing provenance,
  lifecycle state, alpha validation, common errors, and the Linux-only public re-exports.
- `open/crates/superi-codecs-platform/src/vaapi_linux.rs` probes Linux render nodes and driver
  capabilities, implements H.264/HEVC/VVC decode and H.264 encode, confines cros-codecs and GBM
  objects to workers, and exposes external DMA-backed frame storage.
- `open/crates/superi-codecs-platform/src/videotoolbox.rs` is the safe Apple facade. It owns stable
  codec IDs and deterministic capability declarations, implements the codec-only backend, and
  conditionally re-exports the macOS configuration parsers and external frame type.
- `open/crates/superi-codecs-platform/src/videotoolbox/macos.rs` is the private VideoToolbox,
  CoreMedia, and CoreVideo FFI boundary. It creates video sessions, parses AVC/HEVC records,
  translates callbacks, retains external image buffers, and copies safe CPU or packet data at the
  native boundary.
- `open/crates/superi-codecs-platform/src/videotoolbox/macos/aac.rs` is the private AudioConverter
  adapter. It translates AAC and packed PCM, manages synchronous input callbacks and magic
  cookies, and owns converter lifetime and packet timing.
- `open/crates/superi-codecs-platform/src/vvc.rs` is the stateful crate-private VVC Annex B parser.
  It retains parameter sets and picture-order state, assembles one picture, applies bounded
  `oxideav-h266` 0.0.8 workarounds, and produces checked parser state for VA submission.
- `open/crates/superi-codecs-platform/src/vvc_vaapi_linux.rs` is the raw Linux VA-API 1.22 VVC
  boundary. It maps parsed Main 10 pictures into generated VA structures, submits buffers,
  synchronizes P010 surfaces, exports checked DMA-BUF layouts, and owns raw display and descriptor
  cleanup.
- `open/crates/superi-codecs-platform/tests/media_foundation_contract.rs` fixes the public Media
  Foundation identities and configuration behavior and conditionally exercises real Windows AAC
  encode/decode when both transforms are discovered.
- `open/crates/superi-codecs-platform/tests/videotoolbox_contract.rs` is a macOS-only public
  contract suite for registration, configuration parsing, video and AAC lifecycles, timing,
  cancellation, error categories, and external-frame reuse.

## Public surface

### Registration

- `register::platform_backend_registry() -> Result<BackendRegistry>` creates a registry containing
  only platform operations available or declared on the current target.
- `register::register_platform_backends(&mut BackendRegistry) -> Result<()>` discovers or builds
  the target registration, checks every incoming backend ID against the destination and its peers,
  then registers the complete set. With the current `BackendRegistry::register` contract, duplicate
  identity is its only later failure, so the preflight makes this operation effectively
  all-or-nothing. The mutable registry borrow also prevents concurrent registry mutation during the
  call.

### Media Foundation

- `media_foundation::MediaFoundationCodec` and `MediaFoundationOperation` expose stable codec and
  decode/encode identities. H.264, HEVC, and AAC use `h264`, `hevc`, and `aac`; ProRes uses
  `prores-422-proxy`, `prores-422-lt`, `prores-422`, `prores-422-hq`, and `prores-4444`.
- `media_foundation::ProResProfile` maps those identities to `apco`, `apcs`, `apcn`, `apch`, and
  `ap4h` sample-entry FourCCs.
- `media_foundation::AnnexBConfiguration` parses AVCDecoderConfigurationRecord or
  HEVCDecoderConfigurationRecord bytes, exposes the 1 through 4 byte NAL length size and Annex B
  parameter sets, and converts checked length-prefixed samples with optional parameter-set prefix.
- `media_foundation::AacConfiguration` accepts a raw AudioSpecificConfig or extracts descriptor
  tag `0x05` from an `esds` payload. It exposes checked AAC-LC object type, sample rate, channel
  count, original config bytes, and the Media Foundation user-data tail.
- `media_foundation::MediaFoundationBackend` is public only on Windows. It implements
  `MediaBackend` but always reports `NoMatch` for source probes and `Unsupported` for source open.

### VideoToolbox

- `videotoolbox::VideoToolboxBackend` is public on all targets. Its native decoder and encoder
  factories work only on macOS; non-macOS calls fail as unsupported. Registration consumes it only
  on macOS.
- On macOS, `videotoolbox` also re-exports `parse_avcc`, `parse_hvcc`,
  `VideoCodecConfiguration`, and `VideoToolboxFrameBuffer`. The buffer retains a CoreVideo image,
  implements `VideoFrameBuffer`, reports `External` storage, and provides immutable shared access.

### VA-API

- `vaapi::registration()` returns `Ok(None)` on non-Linux targets. On Linux it may return no
  registration when no usable render node or supported capability survives probing.
- Linux publicly re-exports `VaapiBackend` and `VaapiFrameBuffer`. The frame buffer implements
  `VideoFrameBuffer`, reports `External` storage, and retains either an NV12 pooled DMA frame or a
  P010 VVC DMA-BUF owner. H.264 encode recognizes the NV12 form by downcast for same-backend native
  surface reuse.
- VVC parser and raw VA types remain crate-private. No `VADisplay`, context, surface, buffer ID,
  pointer, or raw file descriptor enters the public safe API.

All three backends implement the same `superi-media-io` `MediaBackend`, `Decoder`, and `Encoder`
contracts. They expose owned `Packet`, `VideoFrame`, `AudioBlock`, metadata, exact rational timing,
typed errors, and explicit `NeedInput` and `EndOfStream` states. None exposes a container source or
silently substitutes a different backend after native construction fails.

## Architecture and data flow

### Registration and capability publication

1. `superi-engine::media::media_backend_registry` first registers the default permissive backends.
   When `os-codecs` is enabled, it calls `register_platform_backends` on the same registry, then
   appends the engine-owned in-tree container source registrations.
2. macOS deterministically creates one primary registration at priority 200 with
   `PlatformManaged` acceleration. It declares decode and encode for H.264, HEVC, AAC, and five
   ProRes profiles. H.264, HEVC, and AAC dimensions are runtime-negotiated where appropriate;
   ProRes rows carry fixed profile, depth, and chroma values. This is declaration, not a host probe:
   native session creation can still fail.
3. Windows discovers transforms on a dedicated COM MTA. It enumerates synchronous, local,
   transcode-only MFT activations for each codec and direction and creates a primary priority-200
   registration only when at least one exact pair exists. It reports `Software` acceleration.
   H.264, HEVC, AAC, and four alpha-free ProRes profiles may appear; ProRes 4444 is never published.
4. Linux enumerates an absolute `SUPERI_VAAPI_RENDER_NODE` override or sorted `/dev/dri/renderD*`
   nodes. It opens a display, checks GBM where required, queries profile and entrypoint attributes,
   and attempts native construction. Failed H.264/HEVC decoders, H.264 encoders, or VVC decoders
   are removed before one primary priority-200 `Hardware` registration is built. An empty result
   means no Linux platform registration.
5. Capability declarations drive selection, but concrete creation still validates codec, stream
   kind, timing, pixel/audio format, alpha, color, driver or transform availability, and the native
   media type. A capability is not a guarantee that every profile, dimension, bitrate, or syntax
   variation will configure successfully.

### macOS video and audio flow

H.264 and HEVC decode require complete checked AVC or HEVC configuration records. ProRes decode
uses explicit positive `video.width` and `video.height` metadata. Compressed input is copied into
CoreMedia-owned storage and submitted asynchronously to VideoToolbox. Callback state translates a
retained `CVImageBuffer` into a `VideoToolboxFrameBuffer`; the normal requested output is RGBA16F
with unspecified color, straight alpha for ProRes 4444, and opaque alpha otherwise. Timing and
metadata are carried into safe frames through the queued callback result.

Video encode requires a frame whose complete `VideoFormat` equals the encoder configuration. A
matching `VideoToolboxFrameBuffer` is retained and passed to VideoToolbox without an adapter CPU
copy. A CPU buffer is accepted only for packed BGRA8 or RGBA16F and is copied while the destination
CoreVideo buffer is locked. Encoded sample bytes are copied into owned packets. H.264 and HEVC
callbacks attach `avcC` or `hvcC` configuration metadata when available. The implementation marks
only the first H.264 or HEVC output packet as keyframe; ProRes is treated as all-intra.

AAC decode validates raw AudioSpecificConfig or ESDS metadata, requires an explicit packed output
format, and performs one synchronous `AudioConverterFillComplexBuffer` into a fixed 1 MiB output
allocation per compressed packet. AAC encode supplies packed PCM through a synchronous callback,
loops over at most 256 packet descriptions per fill, publishes the compression magic cookie when
available, and advances packet timestamps by the reported frame count or 1024-frame AAC default.

Video callbacks queue output behind a mutex and public receive returns `NeedInput` while the queue
is empty. Video flush waits for asynchronous work; AAC decode currently only changes lifecycle
state, while AAC encode performs one empty-input fill. Reset clears output and timing state and
resets or recreates native state. New input after flush is rejected until reset.

### Windows Media Foundation flow

Public decoder and encoder methods synchronously send commands, cloned `OperationContext` values,
and bounded reply channels to named worker threads. Each worker enters COM MTA and Media Foundation,
activates the first discovered transform for the exact codec and direction, negotiates stream zero,
and keeps all native state on that thread.

For decode, H.264 and HEVC packets with configuration metadata are converted from length-prefixed
NAL units to Annex B and parameter sets are prefixed to keyframes. AAC and ProRes bytes pass
unchanged. PTS is preferred, DTS is the fallback, and otherwise a synthetic position is assigned in
the 10,000,000-tick HNS clock. A provenance queue travels with submitted samples. Backpressure from
`MF_E_NOTACCEPTING` pulls output before retry, and stream-change results renegotiate output.

Decoded NV12 and P010 become checked two-plane `CpuVideoBuffer` values. v210 is unpacked into three
little-endian 10-bit planar 4:2:2 planes. AAC becomes packed I16 `AudioBlock` data. Native HNS is
matched to provenance exactly when possible, with oldest-entry fallback, so Superi timing and
metadata survive ordinary reordering.

Video encode accepts CPU NV12, P010, BGRA8, or planar 10-bit 4:2:2 as appropriate. Row padding is
removed from NV12, P010, and BGRA, while planar 4:2:2 is packed to aligned v210. AAC encode accepts
packed I16 at 44.1 or 48 kHz with 1, 2, or 6 channels and uses fixed bitrates. Output samples become
owned packets with restored timing, metadata, and native clean-point keyframe state. Native
sequence headers or stable AAC config are published on the first packet after construction or
reset.

Flush sends end-of-stream and drain messages, receive eventually returns `EndOfStream`, and reset
sends a native flush/start sequence and clears provenance, output, drain, sequence-header, and
synthetic-time state.

### Linux VA-API flow

H.264 and HEVC use cros-codecs stateless decoders and output NV12. VVC normalizes Annex B data,
passes it through `VvcBitstreamParser`, submits one checked Main 10 picture through the raw VA-API
path, and outputs P010. AVC, HEVC, and VVC normalizers accept leading Annex B directly or parse
configuration metadata and convert length-prefixed packets with four-byte start codes. Parameter
sets are prefixed on first input and keyframes.

Each input receives a monotonic token whose ledger entry owns exact packet timing, keyframe state,
and cloned metadata. Native output returns the token, allowing the adapter to recover provenance
after reordering. H.264 and HEVC frames add platform, vendor, and keyframe metadata. H.264 encoded
packets also publish the most recently derived AVC configuration record.

H.264 is the only Linux encode path. It accepts opaque NV12 with unspecified color, tries Main,
High, then Baseline through normal and low-power entrypoints, and uses a deterministic geometry
based CBR target with a 256 kbps floor. Same-backend NV12 frames can reuse their native surface;
CPU NV12 is copied row by row into a GBM frame.

The VVC path is deliberately narrower than its Main 10 registration label. It accepts layer-zero,
4:2:0, one-picture access units and currently submits only one intra slice in one untiled,
unpartitioned picture. It rejects inter prediction, multiple slices, subpictures, entry points,
scaling lists, and partition overrides. The raw decoder synchronizes every surface before exporting
one composed P010 layer. It validates 1 through 4 unique DMA-BUF objects, two planes, object indexes,
offsets, pitches, sizes, common modifier, and exact dimensions before transferring each descriptor
into one `File` owner. Its native `flush` returns no delayed frames.

### FFI, ownership, and threading

- Workspace lints deny unsafe code by default. Unsafe is allowed only inside the private macOS
  VideoToolbox/AudioConverter modules and the private Linux VVC VA module, plus the audited Windows
  implementation. Raw values remain behind safe media interfaces and each unsafe operation is
  required to carry a local `SAFETY:` justification.
- VideoToolbox session callback state is boxed at a stable address and outlives the retained
  session. Drop invalidates the session before releasing callback state. Successful create results
  become one `CFRetained` owner, callback images are retained before escape, and callback sample
  bytes are copied before borrowed native storage expires.
- `AudioConverter` uniquely owns one nonnull converter and disposes it exactly once. Callback byte
  slices, packet descriptions, buffer lists, and counts remain stack-owned for the complete
  synchronous fill and do not escape it.
- Windows discovery, decoders, and encoders own separate threads. `ComMfRuntime` balances successful
  `CoInitializeEx`/`CoUninitialize` and `MFStartup`/`MFShutdown`; dependent COM values drop before
  shutdown. MFT activation arrays are consumed entry by entry and freed once. Every successful
  buffer lock is paired with unlock, and output `ManuallyDrop` fields are taken exactly once.
- Linux cros-codecs displays, GBM devices, codec instances, and pooled surfaces remain on bounded
  worker threads. Commands use capacity-eight channels and readiness handshakes. Dependency panics
  are caught at command boundaries. Drop sends stop and joins; reset reconstructs native state on
  its owning worker.
- `VvcVaapiDecoder` retains the DRM render-node file for the `VADisplay` lifetime and drops context,
  configuration, and display in dependency order. VA parameter buffers are destroyed after
  submission. Exported DMA-BUF `File` owners outlive the destroyed VA surface and close only after
  the last `Arc` frame owner drops.
- Linux NV12 pooled frames remain checked out while consumers retain them. The current decoder
  reports a native unavailable error if consumer ownership exhausts the pool rather than exposing
  a high-level retryable backpressure state.

### Timing, lifecycle, cancellation, and errors

Every adapter validates stream kind, stream ID, packet or frame timebase, exact duration where
required, and format compatibility before crossing the native boundary. Output storage is owned or
retained, so no callback pointer, locked buffer, borrowed packet range, COM allocation, or raw file
descriptor escapes as a borrow.

Flush is terminal for input until reset across all adapters. The public lifecycle remains
`send`/`receive`/`flush`/drain/`EndOfStream`/`reset`, although native drain depth differs by adapter.
Public operations call `OperationContext::check` before work and at bounded loops. A synchronous
foreign call itself cannot be preempted by the cooperative token, so cancellation and deadlines
take effect at the next check unless the native API returns earlier.

Errors preserve `superi_core` category, recoverability, component, operation, and contextual fields:

- Contract mismatches are generally `InvalidInput` and `UserCorrectable`.
- Input after flush and duplicate registration are `Conflict` and `UserCorrectable`.
- Malformed configuration, packets, or native output are `CorruptData`; Media Foundation parsing
  generally marks these `Degraded`, while VA shared helpers often mark malformed input
  `UserCorrectable`.
- Unsupported codec variations are `Unsupported` and usually `Degraded`.
- Worker creation, resource bounds, and allocation failures use `ResourceExhausted` or
  `Unavailable` with retryability appropriate to the adapter.
- Windows HRESULT and Apple OSStatus values are retained as error fields when available. Linux VVC
  returns internal strings that the parent VA adapter converts to structured unavailable errors.
- A VA timing-token overflow is an `Internal` terminal error. Caught dependency panics become
  degraded unavailable errors rather than unwinding through the public codec boundary.

## Dependencies and consumers

### Superi dependencies

- `superi-core` supplies structured errors, exact rational time and duration, color space, alpha,
  pixel formats, and audio sample formats.
- `superi-media-io` supplies the backend registry and correlated capability model, stable media
  identities, source stubs, packets and metadata, video and audio ownership types, codec lifecycle
  traits, and cooperative operation context.
- `superi-image` is declared in the crate manifest and documented by the crate root, but no owned
  source currently references `superi_image`. It is therefore an apparently unused direct
  dependency, not an implemented architectural relationship.

### Platform and build dependencies

- macOS uses pinned `objc2` bindings for AudioToolbox, Core Audio Types, Core Foundation, CoreMedia,
  CoreVideo, and VideoToolbox. These dependencies are target-gated and permissively licensed.
- Windows uses the pinned `windows` bindings for Media Foundation, COM, structured storage, and
  variants. The implementation assumes synchronous MFT behavior and stream ID zero.
- Linux uses `cros-codecs` 0.0.6 with VA-API support for H.264/HEVC/H.264 encode and pinned
  `oxideav-h266` 0.0.8 for VVC syntax. Runtime behavior also depends on system libva, libva-drm,
  DRM, GBM, and a compatible installed VA driver.
- `bindgen` 0.70.1 is unconditional in the build graph, although code generation is Linux-only.
  Linux builds need Clang/libclang and either `CROS_LIBVA_H_PATH` or working `pkg-config` metadata
  for libva 2.22 or newer and the VVC header.

### Consumers

- `superi-engine` is the only non-test crate consumer. Its optional `os-codecs` feature enables
  this crate, and `superi-engine/src/media.rs` registers platform backends after the default
  permissive set.
- Engine capability introspection consumes the resulting `BackendRegistration` rows and preserves
  the explicit `Software`, `Hardware`, or `PlatformManaged` acceleration state. Engine selection
  uses the shared registry's priority, tier, and explicit fallback policy. Timeline resource
  preparation now creates a selected platform decoder when one advertises the opened stream codec,
  while native factory failure returns directly without switching implementations.
- The crate's integration tests consume public configuration, registration, decoder, encoder,
  timing, metadata, and buffer contracts. `superi-engine/tests/os_codec_registry_contract.rs`
  checks that host-discovered platform registrations compose with the default registry under
  `os-codecs`.
- Container readers are upstream producers of stable codec IDs, packet bytes, and
  `codec.configuration` metadata, but they do not depend on concrete backend types. The module
  consumes that codec-neutral handoff and never demuxes the container itself.

## Invariants and operational boundaries

- The crate is opt-in and offline. It performs no codec download, server request, SDK discovery, or
  network operation. Linux may inspect local render nodes and system development/runtime files.
- The open tree contains only MIT adapter source and allowlisted dependencies. Encumbered transform
  execution stays in the installed OS framework or VA driver. A task requiring bundled encumbered
  codec code or copyleft linkage must stop at the legal boundary.
- Codec backends never advertise `Source`, never claim a container match, and never open a source.
  Demux remains in `superi-media-io`.
- Backend identity is stable and unique. All platform registrations are primary with priority 200;
  fallback remains a caller policy and no adapter silently changes backend after selection.
- macOS registration is deterministic and may overstate session availability only in the explicit
  sense that details are runtime-negotiated. Windows and Linux registration are host-probed and may
  be absent. Windows acceleration remains `Software`; Linux remains `Hardware`; macOS remains
  `PlatformManaged`.
- H.264 and HEVC length-prefixed samples require valid configuration metadata for conversion.
  Without configuration, Media Foundation and VA paths treat input as already Annex B. NAL length
  sizes are limited to 1 through 4 and all lengths and offsets are checked.
- Windows H.264 decode is 8-bit NV12. HEVC decode is NV12 or P010. ProRes is 10-bit 4:2:2 without
  alpha. AAC decode/encode storage is packed I16 and backend channel layouts are mono, stereo, or
  5.1 even though the parser recognizes more configuration values.
- VideoToolbox video decode requests RGBA16F, reports unspecified color, and preserves straight
  alpha only for ProRes 4444. CPU encode input is BGRA8 or RGBA16F; external CoreVideo input can be
  reused. AAC is packed PCM with at most eight channels.
- Linux H.264 and HEVC output is NV12; VVC output is P010. H.264 encode accepts opaque NV12 and
  rejects declared color signaling. Stream identity, timebase, timestamp presence, exact duration,
  and complete color metadata axes are checked.
- The current VVC parser is layer-zero Main 10 4:2:0, bounded to 64 stored values per parameter-set
  map, at most 600 slices in parsing, one picture per packet, dimensions within native domains, and
  a 4 KiB temporary slice-header parse. The raw submission layer is narrower still and accepts one
  intra slice.
- Every output owns or shares its storage. `VideoFrameBuffer` is `Send + Sync`; codec objects are
  `Send`; backend factories are `Send + Sync`. Platform-specific native mutability remains on the
  owning worker or behind synchronized callback state.
- Packet timing and metadata are never inferred from output order when a native token or timestamp
  match is available. Reset clears pending provenance but does not reuse VA token values.
- ProRes 4444 is a stable identity on Apple only. Windows deliberately omits it rather than dropping
  alpha, and Linux has no ProRes path.
- Linux compilation currently requires the VVC libva header and minimum libva version even if the
  eventual driver exposes only H.264 or HEVC. This is a build-time restriction, not a capability
  declaration.

## Tests and verification

- `media_foundation_contract.rs` verifies stable codec IDs, ProRes FourCC mapping, AVC and HEVC
  parsing and conversion, malformed-data classification, AAC raw/ESDS parsing, unsupported AAC
  cases, target-gated registration, and Windows capability metadata. Its real AAC lifecycle runs
  only when both AAC directions are discovered and checks production, timing, configuration, drain,
  decode, and selected metadata propagation.
- `videotoolbox_contract.rs` runs only on macOS. It verifies the exact eight-codec decode and encode
  registration, capability detail, duplicate-ID atomicity, AVC/HEVC parsers, cancellation and
  missing-configuration errors, and native H.264, HEVC, five ProRes profile, and AAC lifecycles.
  The H.264 test also proves an external decoded frame can be passed to another encoder through the
  public API. It does not prove pixel/sample fidelity or native retain counts.
- Inline Media Foundation coverage checks odd-width planar 10-bit to v210 pack/unpack behavior.
- Inline shared VA coverage exercises AVC, HEVC, and VVC normalization, malformed records,
  parameter-set ordering, timing and metadata restoration after reordering, alpha rejection,
  capability projection, and flush/reset lifecycle.
- Inline Linux VA coverage verifies IDR detection, AVC configuration generation, exact frame-rate
  conversion, padded CPU-row removal, deterministic bitrate, and render-node filtering/sorting
  without requiring a live driver.
- Inline VVC parser coverage checks bit copying, embedded picture-header replacement, removed-range
  handling, emulation-prevention-aware offsets, and empty access-unit rejection.
- Inline raw VVC coverage checks numeric narrowing. Its real parser-to-VA mapper test silently skips
  unless `SUPERI_VVC_FIXTURE` names a fixture, and it does not create a VA display or decode pixels.
- Linux registration unit tests live-probe the host and require zero or one nonempty truthful
  `linux-vaapi` registration. The engine `os_codec_registry_contract` verifies composition and
  acceleration projection for any host registration.
- Target proof remains distributed: macOS native lifecycle tests require Apple frameworks, Windows
  lifecycle proof requires installed MFTs, and Linux driver behavior requires real render-node and
  driver testing. Host-independent unit tests cannot substitute for these runs.

## Current status and risks

The platform registry and all three target adapters are substantive implementations, not
placeholders. macOS has native video and AAC lifecycle contracts, Windows has configuration and
conditional AAC contracts, and Linux has normalization, probing, worker, and VVC mapper tests.
Proof strength is not equal across targets, and capability publication intentionally describes
operation-level availability rather than complete codec conformance.

Key incomplete behavior and risks are:

- Apple advertises all operations without probing OS version, native session availability, profile,
  or hardware. Creation may fail after selection. H.264/HEVC keyframe metadata marks only the first
  output packet, decoded color is unspecified, and encoder profile, level, bitrate, GOP, and color
  controls are absent.
- VideoToolbox callback conversion collapses detailed Rust errors to synthetic status `-12909`, and
  poisoned output mutexes can panic through `expect`. CPU-to-CoreVideo row copies rely on upstream
  geometry for visible row coverage.
- AAC decode has a fixed 1 MiB output buffer, performs one fill per packet, maps untimed packets to
  sample zero, and does not explicitly drain delayed decoder output at flush. AAC encode timestamp
  assumptions rely on the surrounding audio timebase contract.
- Windows activates only the first discovered transform and does not try later candidates after a
  configuration failure. Discovery proves broad type compatibility, not every requested profile,
  rate, dimension, color, bitrate, or installed component behavior.
- Windows packet input without configuration is assumed to be Annex B. HNS provenance falls back to
  oldest pending input when timestamps do not match, which can misassociate metadata for unusual
  reorder or multi-output behavior. Extreme native time conversion falls back to zero.
- Windows stride is inferred from total output size, not an explicit stride attribute. Zero-length
  locked buffers, future relaxation of plane validation, partial color mappings, fractional-rate
  bitrate estimation, and missing user controls remain unsafe or semantic review points.
- Linux H.264 encode uses a geometry-derived fixed CBR target, rejects all declared color, and keeps
  only the first SPS/PPS for generated AVC config. Its single-buffer NV12 layout assumption must stay
  aligned with GBM and cros-codecs behavior.
- Consumer retention of Linux pooled frames can exhaust native output and currently surfaces as an
  unavailable error. A GBM allocation failure after successful probe can panic inside the worker;
  the panic is contained but loses structured allocator detail.
- VA normalizers may duplicate parameter sets on Annex B keyframes, accept an empty access unit when
  prepended configuration made the combined buffer nonempty, and do not require complete AVC/HEVC
  configuration consumption. VVC parsing does require complete consumption.
- VVC capability metadata cannot express the implemented intra-only, single-slice syntax subset.
  Real hardware submission, pixel correctness, context resize, DMA-BUF interoperability and
  lifetime, driver failures, and cleanup failures lack default automated proof. Raw flush emits no
  delayed pictures.
- VVC workarounds depend on `oxideav-h266` 0.0.8 inference and consumed-bit behavior. Any parser or
  libva ABI upgrade requires a field-by-field re-audit, not only a compile fix.
- Linux build reproducibility depends on system libva headers, `pkg-config`, Clang/libclang, and
  installed driver behavior. `bindgen` is paid on the build graph for all targets, and the declared
  `superi-image` dependency appears unused.
- Cooperative cancellation cannot interrupt a native call that blocks internally. Adapter checks
  bound work around calls, but release qualification must still test hangs and teardown against
  real platform implementations.
- The codec policy's legal review and AAC verification remain open. Implementation and tests do not
  close those policy decisions.

## Maintenance notes

- Re-run `python3 .agents/skills/superi-mapping/scripts/codebase_maps.py files
  superi-codecs-platform` after any crate change. Every returned path must remain listed here in
  backticks with a concrete role, including target-gated tests and build scripts.
- Recompute the module hash after source changes and update the hash, file count, inventory,
  platform behavior, dependencies, consumers, invariants, tests, status, and risks together. Do not
  refresh metadata alone.
- Any change to stable codec IDs, backend IDs, priority, tier, acceleration, or correlated
  capability rows must be reconciled with engine introspection and selection tests.
- Keep configuration parsing and packet normalization aligned with container-produced
  `codec.configuration` metadata. Changes to MP4/MOV or Matroska codec records require paired parser
  and adapter contract review.
- New native handles, callbacks, unsafe implementations, platform dependencies, or ownership models
  require a matching update to `docs/unsafe-ffi.md`, local `SAFETY:` evidence, strict Clippy, and a
  native lifecycle test on the owning target.
- Audit `oxideav-h266`, generated libva structures, cros-codecs/GBM layout, `windows` bindings, and
  `objc2` APIs at every dependency upgrade. Preserve the Rust 1.80 floor and the permissive license
  allowlist.
- Test native adapters on their real operating systems. In particular, keep a macOS lifecycle run,
  a Windows MFT discovery and lifecycle run, and a Linux render-node decode/encode run with explicit
  VVC fixture and DMA-BUF lifetime coverage in release evidence.
- Preserve the opt-in and container-separation boundaries. A new source capability, bundled codec,
  hidden CPU readback, silent fallback, or network dependency is an architectural and legal change,
  not a local adapter detail.
