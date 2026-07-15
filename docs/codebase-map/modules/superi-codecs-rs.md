---
module_id: superi-codecs-rs
source_paths:
  - open/crates/superi-codecs-rs
source_hash: 4f7cb542c0ae20961aa7f05b9c3c54d005ea5835f91176451aad02d1edeb6e16
source_files: 36
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-codecs-rs` is Superi's default T1b backend for in-tree, permissively licensed video and
audio codecs. It implements software decode and encode for AV1, FLAC, MP3, Opus, linear PCM,
Vorbis, VP8, and VP9 behind the codec-neutral contracts owned by `superi-media-io`. It owns codec
registration, elementary-bitstream and canonical-sample adaptation, native codec lifetimes,
codec-specific timing and metadata translation, and the private libvpx ABI shim. It does not own
container probing or demux, still-image codecs, GPU upload, graph evaluation, OS codecs, vendor RAW
workers, or engine orchestration.

The crate is in the ordinary engine configuration. `superi-engine` constructs a registry and always
calls `register_default_backends` before it optionally adds OS or vendor backends. The dependency
direction remains `superi-engine` -> `superi-codecs-rs` -> `superi-media-io` and `superi-core`; this
crate never depends upward on the engine. Its declared `superi-image` dependency is not used by any
current crate source.

The implementation is codec-only. Every backend returns `NoMatch` from `probe_source` and rejects
`open_source`; container backends identify codec IDs, provide packet timing and configuration, and
feed elementary packets into decoders selected from the registry. Encoders return elementary
packets for a separate mux or export path.

## Source inventory

- `open/crates/superi-codecs-rs/Cargo.toml`: crate manifest for the three internal media
  dependencies, all Rust and native codec libraries, dynamic loading, and the pinned build helpers
  required by the Rust 1.80 workspace floor.
- `open/crates/superi-codecs-rs/build.rs`: unconditionally compiles the private libvpx C shim and
  declares rebuild tracking for the shim and vendored headers.
- `open/crates/superi-codecs-rs/src/av1.rs`: `rust-av1` backend, rav1d decoder ownership and picture
  conversion, rav1e encoder configuration, AV1 capability rows, timing correlation, color metadata,
  semiplanar conversion, and AV1 error translation.
- `open/crates/superi-codecs-rs/src/flac.rs`: Claxon decode, flacenc whole-stream encode, exact FLAC
  precision and channel assignment handling, canonical integer sample conversion, and reversible
  Vorbis-comment metadata encoding.
- `open/crates/superi-codecs-rs/src/lib.rs`: public crate entry point exposing the eight codec and
  registry modules while keeping `vpx_ffi` private.
- `open/crates/superi-codecs-rs/src/mp3.rs`: OxideAV MPEG Audio Layer III decode and fixed 128
  kbit/s CBR encode, MP3 header and bit-reservoir inspection, sample timing, packed or planar I16
  conversion, delayed metadata attribution, and lifecycle adaptation.
- `open/crates/superi-codecs-rs/src/opus.rs`: audited libopus FFI ownership, OpusHead parsing and
  serialization, channel mapping, pre-skip and gain, discard-padding decode, fixed-frame encode,
  lookahead compensation, and tail-padding metadata.
- `open/crates/superi-codecs-rs/src/pcm.rs`: public PCM identity table plus byte-exact PCM decode and
  encode, explicit endian conversion, packed or planar repacking, canonical passthrough, timing, and
  queue lifecycle.
- `open/crates/superi-codecs-rs/src/register.rs`: construction and duplicate-ID preflight for the
  complete default backend batch, plus deterministic insertion into caller-owned registries.
- `open/crates/superi-codecs-rs/src/vorbis.rs`: lewton packet decode, threaded `vorbis_rs` encode,
  internal Ogg packet recovery, Xiph-laced header handling, sample normalization, channel-order
  translation, granule timing, metadata range attribution, and worker cleanup.
- `open/crates/superi-codecs-rs/src/vp9.rs`: safe media-facing VP8 and VP9 backend, capability rows,
  libvpx runtime use, video format and color mapping, timing and metadata policy, and FFI error
  classification.
- `open/crates/superi-codecs-rs/src/vpx_ffi.rs`: private unsafe Rust boundary that loads the official
  libvpx 1.16 runtime, pins function addresses and library lifetime, owns native handles, checks C
  results, and copies native frames and packets into Rust-owned values.
- `open/crates/superi-codecs-rs/src/vpx_shim.c`: checked C implementation over the loaded libvpx
  symbol table, including context creation, configuration, image allocation, plane copy, decode,
  encode, drain, and destruction.
- `open/crates/superi-codecs-rs/src/vpx_shim.h`: shared Rust/C ABI with fixed codec and format values,
  function-table layout, owned transfer records, opaque handles, and shim function declarations.
- `open/crates/superi-codecs-rs/tests/av1_contract.rs`: AV1 registry, deterministic encode/decode,
  timing, metadata, CPU formats, ten-bit and HDR color, Matroska ingest, seek, relink, reset,
  cancellation, corruption, stream mismatch, and alpha-rejection contracts.
- `open/crates/superi-codecs-rs/tests/flac_contract.rs`: FLAC registry, native decode, lossless
  flush-time encode, all supported precisions and channel mappings, planar input, Matroska codec
  private data, lifecycle, corruption, mismatch, and cancellation contracts.
- `open/crates/superi-codecs-rs/tests/mp3_contract.rs`: MP3 registry, MPEG rate families, packed and
  planar I16 round trips, delayed metadata assignment, exact timebase conversion, reset,
  cancellation, unsupported input, and corrupt-data contracts.
- `open/crates/superi-codecs-rs/tests/opus_contract.rs`: Opus registry, OpusHead, rates and storage
  representations, one-through-eight-channel mappings, output gain, exact logical duration,
  pre-skip and tail padding, Matroska trimming, lifecycle, corruption, and cancellation contracts.
- `open/crates/superi-codecs-rs/tests/pcm_contract.rs`: every PCM identity, endian and planar
  conversion, canonical storage, inferred and exact timing, zero-frame behavior, lifecycle,
  cancellation, malformed packets, and metadata contracts.
- `open/crates/superi-codecs-rs/tests/register_contract.rs`: default-registry order, atomic duplicate
  rejection, primary selection, and detailed AV1 and VP9 capability tuple contracts.
- `open/crates/superi-codecs-rs/tests/vorbis_contract.rs`: deterministic Vorbis headers and audio,
  Xiph-laced configuration, all public sample formats, rounded container timing, 5.1 channel
  remapping, lifecycle, cancellation, stream mismatch, and malformed input contracts.
- `open/crates/superi-codecs-rs/tests/vpx_contract.rs`: real libvpx VP8 and VP9 round trips, odd-sized
  planes, the supported VP9 bit-depth/subsampling matrix, color and metadata, alpha rejection,
  lifecycle, corruption, cancellation, and WebM-to-decoder integration.
- `open/crates/superi-codecs-rs/vendor/libvpx/.gitattributes`: preserves the vendored license file's
  intentional trailing blank line from broad whitespace normalization.
- `open/crates/superi-codecs-rs/vendor/libvpx/LICENSE`: retained WebM Project BSD-style
  redistribution, warranty, and liability terms for the vendored libvpx material.
- `open/crates/superi-codecs-rs/vendor/libvpx/PATENTS`: retained additional Google patent grant,
  modification exclusion, and patent-enforcement termination terms for distributed WebM code.
- `open/crates/superi-codecs-rs/vendor/libvpx/include/vpx/vp8.h`: shared VP8/VP9 controls,
  postprocessing declarations, reference-frame identities, image carriers, and typed wrappers.
- `open/crates/superi-codecs-rs/vendor/libvpx/include/vpx/vp8cx.h`: VP8/VP9 encoder entry points,
  frame flags, codec controls, ROI, active-map, scalability, tuning, and typed control declarations.
- `open/crates/superi-codecs-rs/vendor/libvpx/include/vpx/vp8dx.h`: VP8/VP9 decoder entry points,
  decoder controls, decrypt callback state, and typed control declarations.
- `open/crates/superi-codecs-rs/vendor/libvpx/include/vpx/vpx_codec.h`: common libvpx context,
  version, error, capability, control, destroy, and ABI contracts.
- `open/crates/superi-codecs-rs/vendor/libvpx/include/vpx/vpx_decoder.h`: generic decoder
  initialization, stream inspection, input ordering, frame iteration, callbacks, and external
  frame-buffer contracts.
- `open/crates/superi-codecs-rs/vendor/libvpx/include/vpx/vpx_encoder.h`: generic encoder
  configuration, initialization, reconfiguration, encode and flush protocol, packet iteration,
  caller output buffers, global headers, preview images, and scalability declarations.
- `open/crates/superi-codecs-rs/vendor/libvpx/include/vpx/vpx_ext_ratectrl.h`: optional VP9 external
  rate-control model, first-pass and TPL inputs, frame, QP, GOP, reference, and rdmult decisions, and
  callback lifecycle.
- `open/crates/superi-codecs-rs/vendor/libvpx/include/vpx/vpx_frame_buffer.h`: external decoder frame
  buffer record, get and release callbacks, return rules, and work/reference buffer limits.
- `open/crates/superi-codecs-rs/vendor/libvpx/include/vpx/vpx_image.h`: libvpx planar image formats,
  color/range tags, signed strides, plane layout, ownership state, allocation, wrapping, cropping,
  flipping, and release contracts.
- `open/crates/superi-codecs-rs/vendor/libvpx/include/vpx/vpx_integer.h`: C integer, size, format,
  limit, and compiler-inline portability definitions used by the vendored ABI.
- `open/crates/superi-codecs-rs/vendor/libvpx/include/vpx/vpx_tpl.h`: borrowed temporal dependency
  model block, frame, and GOP statistics and their ABI version contribution.

## Public surface

The crate root publicly exposes `av1`, `flac`, `mp3`, `opus`, `pcm`, `register`, `vorbis`, and `vp9`.
Concrete decoder and encoder structs remain private; consumers receive `Box<dyn Decoder>` and
`Box<dyn Encoder>` from each `MediaBackend` implementation.

- `av1` exposes `AV1_CODEC_ID`, `Av1Backend::new`, and `Av1Backend::registration`. The stable backend
  ID is `rust-av1`.
- `flac` exposes `FlacBackend::new`, `FlacBackend::codec_id`, and
  `FlacBackend::registration`. The stable backend ID is `rust-flac`.
- `mp3` exposes `Mp3Backend::new`, `Mp3Backend::codec_id`, and `Mp3Backend::registration`. The
  stable backend ID is `rust-mp3`.
- `opus` exposes `OPUS_CODEC_ID`, `OpusBackend::new`, `OpusBackend::codec_id`, and
  `OpusBackend::registration`. The stable backend ID is `rust-opus`.
- `pcm` exposes the non-exhaustive `PcmEncoding` enum, its complete `ALL` list, stable codec IDs,
  sample-format mapping, and `PcmBackend`. The stable backend ID is `rust-pcm`; encodings cover
  canonical PCM, U8, signed I16/I24/I32 in both byte orders, and F32/F64 in both byte orders.
- `vorbis` exposes `VORBIS_CODEC_ID` and `VorbisBackend`. The stable backend ID is `rust-vorbis`.
- `vp9` exposes the non-exhaustive `VpxCodec` enum for VP8 and VP9 plus `VpxBackend::new`,
  `runtime_version`, and registration. The stable backend ID is `libvpx`.
- `register` exposes `default_backend_registry`, which returns a new populated registry, and
  `register_default_backends`, which atomically preflights and adds the ordinary codec set to an
  existing registry.

Every default backend is registered at priority 100, tier `Primary`, and
`HardwareAcceleration::Software`. All advertise decode and encode only, not `Source`. Registry order
is `rust-pcm`, `rust-av1`, `rust-mp3`, `rust-flac`, `rust-vorbis`, `rust-opus`, then `libvpx`.
Selection is performed with codec IDs and `BackendRequirement`; callers should not construct private
codec state directly.

The detailed capability rows preserve profile, level, bit-depth, and chroma relationships. AV1
publishes main, high, and professional rows; VP8 publishes 8-bit 4:2:0; VP9 publishes profiles 0
through 3 for the supported 8/10-bit 4:2:0/4:2:2/4:4:4 combinations. Audio rows mark profiles,
levels, and chroma not applicable and publish their supported sample precision. PCM publishes all
twelve stable representations. VP9 and AV1 levels are runtime negotiated.

`vpx_ffi` and the C shim are intentionally private. Their layouts and numeric values are a crate
internal ABI, not a supported Rust API. The vendored libvpx headers declare a much larger C surface
than Superi wraps, including SVC, multipass, external frame buffers, TPL, and external rate control;
those declarations are compatibility input and must not be presented as implemented Superi
capabilities.

## Architecture and data flow

### Registration and packet boundary

`register_default_backends` first constructs all seven registrations, including the runtime-loaded
VPx backend, and checks both existing and intra-batch backend IDs before mutating the caller's
registry. This makes duplicate-ID failure atomic, but also makes a compatible libvpx runtime a
construction dependency of the whole default registry. `superi-engine::media_backend_registry` is
the direct runtime consumer. Engine timeline resource preparation now selects these registrations
from opened stream codec IDs and constructs the selected decoder exactly once. Optional OS and
vendor layers are added only after this default set.

Upstream container implementations in `superi-media-io` map Matroska/WebM identities `V_AV1`,
`V_VP8`, `V_VP9`, `A_MPEG/L3`, `A_FLAC`, `A_VORBIS`, and `A_OPUS`, and MP4/MOV `av01`, to the stable
codec IDs. They attach `codec.configuration`, codec delay, seek pre-roll, discard padding, color,
provenance, and exact packet timing as applicable. Codec decoders consume that elementary packet
contract and emit `VideoFrame` or `AudioBlock`; encoders consume those canonical values and emit
`Packet`. No codec in this crate parses a container.

### AV1 path

Decode validates one AV1 video stream, queues at most 64 temporal units, and owns one single-thread
rav1d context. Each packet is copied into rav1d storage and assigned a monotonic sequence offset;
Superi timing and metadata remain in a parallel queue. Decoded picture offsets, PTS, and duration
must match the retained record. Sequence offsets are checked when narrowed to rav1d's target C ABI
width, including the 32-bit offset field exposed on Windows MSVC, then widened losslessly for
correlation with Superi state. The wrapper validates dimensions, signed strides, plane geometry,
bit depth, and allocation arithmetic before copying visible rows into owned CPU planes. It emits
8-bit monochrome or planar 4:2:0/4:2:2/4:4:4 and 10-bit planar 4:2:0/4:2:2/4:4:4 in little-endian
16-bit storage. Ten-bit monochrome and 12-bit pictures are rejected because no exact public format
exists. H.273 color fields and raw AV1 values are translated into `VideoFormat` and namespaced
metadata.

Encode creates a single-thread low-latency rav1e context at speed 10 and quantizer 80. It accepts
opaque CPU `R8Unorm`, planar 8/10-bit YUV, NV12, and P010, deinterleaving semiplanar input when
needed. Exact format and timebase agreement are required; GPU or other external storage is rejected.
Rav1e's opaque frame context carries source timing and metadata through reordering. Output uses
equal PTS and DTS, retains duration and metadata, reports keyframe and encoder facts, and attaches
the AV1 sequence header as `codec.configuration` on the first packet.

### Lossless and packet-audio paths

FLAC decode accepts either a complete native `fLaC` stream or a headerless FLAC frame combined with
byte-valued `codec.configuration`. Claxon stream info determines or verifies the requested format.
It supports 8/12/16-bit values in canonical I16 storage and 20/24-bit values in canonical three-byte
I24 storage, packed or planar, for standard one-through-eight-channel FLAC assignments. Packet
duration must equal decoded samples and packet PTS must map exactly to a sample boundary. Claxon is
contained with `catch_unwind`, and block-relative timestamps are checked before output. FLAC encode
holds canonical `i32` samples and one stable metadata/precision contract for the complete stream,
then flacenc emits one keyframed native stream packet at flush. Its private `SUPERI_` Vorbis-comment
encoding round-trips supported metadata values and preserves foreign comments under namespaced keys.

MP3 decode requires one complete MPEG Audio Layer III frame per packet, a supported MPEG sample
rate, canonical mono or stereo, and packed or planar I16. It parses and validates the header before
OxideAV, requires exact frame length and duration when declared, and rejects fractional-sample
timing. OxideAV planar output is either wrapped into canonical planes or explicitly interleaved.
MP3 encode uses OxideAV's fixed 128 kbit/s CBR path, interleaves planar input, and submits sample
positions relative to the first block. Per-input metadata spans survive encoder delay. Encoded
headers and side information determine packet duration and whether `main_data_begin == 0`, which
marks independent decode.

Opus decode parses or synthesizes OpusHead, creates a single-stream or multistream libopus state,
applies Q8 gain, verifies packet sample count, removes pre-skip, and applies packet duration and
signed `container.discard-padding-ns` trimming. Standard family 0 and 1 mappings are translated to
canonical editor channel order; other mapping families decode as discrete positions. Output can be
packed or planar I16/F32 at 8, 12, 16, 24, or 48 kHz. Opus encode accepts only canonical layouts
through eight channels, converts input to interleaved F32, and emits 20 ms frames. It queues an
untimed OpusHead first, separates raw codec frames from logical post-lookahead frames, zero-pads on
flush, and records tail padding so aggregate packet duration equals the submitted media duration.

Vorbis decode obtains identification, comment, and setup headers from Xiph-laced
`codec.configuration` or three in-band packets. Lewton retains overlap state and emits canonical
F32 planar audio; codec channel order is remapped for standard one-through-eight-channel layouts.
The first packet establishes the cursor and later output is sample-contiguous. Duration may trim
tail output but cannot exceed decoded samples. Vorbis encode moves the non-`Send` encoder onto a
named worker reached through zero-capacity channels. It converts every supported packed or planar
integer/float input to finite normalized F32, remaps channels, and lets the worker write Ogg pages
into a shared grow-only byte buffer. An Ogg packet reader extracts three raw headers and subsequent
audio packets. Granule positions determine exact tail timing; the public output remains raw Vorbis
packets rather than an Ogg container.

### PCM path

Explicit PCM packets are interleaved. Decode validates whole sample frames, exact duration, stream
identity, and timebase, then byte-swaps big-endian samples and either leaves packed storage or
deinterleaves it into canonical little-endian planes. Canonical PCM packets already use Superi byte
order; a planar canonical packet concatenates complete channel planes. Encode performs the inverse:
canonical blocks concatenate existing planes without numerical conversion, while explicit forms
walk canonical packed or planar input and produce interleaved bytes in the selected byte order.
Every block becomes one keyframed packet. PCM never normalizes, clips, quantizes, or changes sample
values.

### VP8 and VP9 path

`Runtime::load` uses `SUPERI_LIBVPX_PATH` exclusively when set; otherwise it searches beside the
executable, platform library names, and standard Homebrew paths on macOS. It accepts only version
strings beginning with `v1.16.` and resolves all required symbols before exposing a runtime.
`VpxBackend` shares that runtime through `Arc`; each decoder or encoder owns a separate one-thread
native context.

Decode synchronously passes a compressed packet to the C shim, iterates native images, and copies
visible I420/I422/I444 rows into Rust-owned planes before the next libvpx call. Packet timing must
include a timestamp and aggregate duration. Multiple output frames divide duration evenly and
receive sequential timestamps. VP8 supports opaque YUV420p8. VP9 supports opaque planar 4:2:0,
4:2:2, and 4:4:4 at 8 or 10 bits, including odd dimensions by ceiling chroma geometry. Complete
container or packet color metadata overrides the coarser native tags.

Encode accepts the same CPU planar matrix, with VP8 restricted to YUV420p8. Rust validates and
removes row padding; the C shim validates the contiguous byte count again, allocates a temporary
libvpx image, copies planes, sets color/range controls, forces the first keyframe, and encodes with
one-pass VBR, zero lag, a fixed estimated bitrate, and the good-quality deadline. Native compressed
packets are copied before any later codec call. Null-image submission flushes. The FFI wrapper owns
all native handles and retains the loaded library until the last context is destroyed.

## Dependencies and consumers

Internal dependencies are `superi-core` for classified errors, exact time, color, alpha, pixel and
sample formats, and channel layouts; `superi-media-io` for backend registration and selection,
packets, streams, metadata, operation contexts, decoder/encoder lifecycles, CPU video buffers, and
audio blocks; and the currently unused manifest dependency `superi-image`.

Pinned codec dependencies are `rav1d` 1.1.0, `rav1e` 0.7.1, and `av1-grain` 0.2.4 for AV1;
`claxon` 0.4.3 and `flacenc` 0.4.0 for FLAC; `oxideav-core` 0.1.29 and `oxideav-mp3` at immutable
revision `f37901b5d9c691b113e96a3bb95645c67af1a046` for MP3; `lewton` 0.10.2,
`vorbis_rs` 0.5.4, `aotuv_lancer_vorbis_sys` 0.1.4, `ogg_next_sys` 0.1.3, and `ogg` 0.8.0 for
Vorbis; and bundled static `libopus_sys` 0.3.3 for Opus. `libloading` 0.8.9 and `libc` support the
native boundaries. Build dependencies pin `built` 0.7.1 and `jobserver` 0.1.34 for Rust 1.80
compatibility and use `cc` to compile the VPx shim.

Direct consumers and producers are:

- `superi-engine` always calls `register_default_backends`; its optional codec features add other
  layers afterward. Resource preparation consumes decoder selection and factory lifecycles, and the
  canonical WebM contract proves a live `rust-av1` decoder beside the retained timeline graph.
- `superi-media-io` Matroska/WebM and MP4/MOV readers produce codec IDs, elementary packets,
  configuration, timing, trimming, color, and provenance metadata consumed here.
- Registry and engine capability consumers inspect the detailed codec rows without invoking codec
  construction. Engine resource preparation separately selects factories from coarse codec
  capability and retains the exact `DecoderConfig` and backend evidence.
- Playback, ingest, seek, relink, graph upload, and audio pipelines consume decoded `VideoFrame` and
  `AudioBlock` values through the neutral interface.
- Mux and export paths consume encoded elementary `Packet` values and their configuration,
  keyframe, timing, and metadata fields.

The crate does not depend on a container, engine, graph, GPU, color-processing, or audio-engine
module. It therefore preserves the downward-only T1b boundary, although its CPU-only video outputs
require a later explicit GPU upload for the repository's GPU-resident render pipeline.

## Invariants and operational boundaries

- Every stateful codec checks `OperationContext` at its public boundary. AV1, FLAC, Opus, and Vorbis
  also poll selected conversion loops. A native call already in progress is not preemptible.
- Decoder receive states are `NeedInput`, `Frame` or `Audio`, and drained `EndOfStream`; encoder
  receive states are `NeedInput`, `Packet`, and drained `EndOfStream`. Sending after flush is a
  conflict until reset. Reset clears queues and timing and reconstructs native state where needed.
- Codec, stream kind, stream ID, timebase, and configured media format must agree. Encoder inputs
  are format-identical and sample/frame-contiguous. Codecs do not perform arbitrary format or sample
  conversion beyond their explicit packing, endian, channel-order, or semiplanar adaptations.
- Packet and media timestamps are signed and may be negative. PCM, FLAC, and MP3 require exact
  sample-boundary rescaling. Vorbis and Opus use explicit nearest-ties-even behavior where container
  timing and codec delay require it. Packet duration describes logical media, not hidden lookahead or
  padding.
- Public multi-byte audio is little-endian. Packed audio is interleaved in semantic channel order;
  planar audio has one complete plane per channel. Video planes are immutable, owned, and validated
  for count, row count, and stride before public exposure.
- Metadata follows the temporal region that produced output. Direct paths copy it, while delayed
  MP3, Opus, Vorbis, and AV1 paths retain timing-correlated metadata records. Codec configuration,
  delay, padding, color, precision, and provenance use namespaced keys.
- Alpha is never silently discarded. AV1 and VPx require opaque input and reject declared alpha.
  GPU-only or external video storage is rejected by current AV1 and VPx encoders instead of being
  downloaded implicitly.
- Unsafe code is confined to AV1 rav1d ownership, Opus libopus ownership, and the VPx Rust/C/runtime
  boundary. Native contexts are uniquely owned, manually `Send` only under exclusive access, and
  destroyed exactly once. Borrowed native pictures, packet buffers, error detail, and images are
  copied or fully consumed before their owning call or context can invalidate them.
- Allocation arithmetic is checked before public storage is built. AV1 bounds its input queue at 64;
  codec packet allocations are owned by `Arc<[u8]>` or vectors. FLAC whole-stream, Vorbis Ogg, and
  delayed metadata storage remain stream-lifetime allocations and do not have a global byte budget.
- Codec errors are translated to `superi-core::Error` with stable components
  `superi-codecs-rs.av1`, `.flac`, `.mp3`, `.opus`, `.pcm`, `.vorbis`, `.vpx`, or `.register` and
  operation context. State/configuration mistakes are generally user-correctable, malformed
  bitstreams degraded or corrupt, resource failures retryable where identified, and impossible
  lifecycle/ABI/timing states terminal.
- The Rust and C VPx layers share exact numeric enum values, field order, ABI version arguments, and
  symbol-table order. Input dimensions, plane sizes, sample ranges, pointer validity, iterator state,
  signed strides, and platform integer limits must be checked before or immediately after the C
  call. High-bit-depth libvpx images use 16-bit storage even for 10-bit meaningful samples.
- The vendored libvpx headers expose borrowed and callback-owned APIs not wrapped by Superi. Packet
  iterators start null; packet/image storage expires on later codec calls; external buffers must be
  fully zeroed and retained until release; fixed layer/GOP/reference counts are ABI bounds. These
  rules remain vendor compatibility constraints even where current product code does not exercise
  the feature.
- The module is inside the default royalty-free codec boundary. It must not gain GPL, LGPL, MPL,
  proprietary, or patent-encumbered codec code. H.264, HEVC, VVC, ProRes, and AAC remain in opt-in OS
  backends, while vendor RAW remains in separate user-selected workers.
- The vendored libvpx BSD terms and patent grant must remain with redistribution. Binary
  distributions must reproduce the required notices, project names cannot imply endorsement, and
  specified patent enforcement can terminate the additional patent license. Repository-wide codec
  legal review remains an explicitly open architecture item.

## Tests and verification

The module owns eight integration suites. They select real registrations and exercise actual codec
implementations rather than mocked success:

- `av1_contract.rs` proves primary selection, deterministic packets, timing and metadata round trip,
  the advertised CPU input matrix, 10-bit/HDR/color behavior, alpha rejection, lifecycle and
  asynchronous corrupt-data reporting, plus generated Matroska ingest, exact seek, and relink.
- `flac_contract.rs` proves lossless values, 8/12/16/20/24-bit precision, one-through-eight-channel
  assignments, planar 20-bit input, flush-only output, reset, typed failures, cancellation, and a
  generated Matroska codec-private path.
- `mp3_contract.rs` proves mono/stereo packed and planar flow, MPEG-2.5/MPEG-2/MPEG-1 frame durations,
  delayed metadata attribution, exact or rejected timing rescale, deterministic reset, corruption,
  unsupported input, and cancellation.
- `opus_contract.rs` proves deterministic OpusHead and audio, all five sample rates and four storage
  forms, one-through-eight-channel identity, gain, pre-skip, exact aggregate duration, tail padding,
  lifecycle, corruption, cancellation, and Matroska codec-delay/discard-padding integration.
- `pcm_contract.rs` proves every explicit encoding, byte swapping and interleaving, canonical planar
  preservation, zero frames, inferred timing, lifecycle, cancellation, exact duration, stream
  identity, and malformed-frame rejection.
- `register_contract.rs` proves stable complete assembly, selection, duplicate-ID atomicity, and the
  detailed AV1 and VP9 rows.
- `vorbis_contract.rs` proves deterministic headers and audio, all public sample formats, Xiph-laced
  configuration, exact and rounded timing, 5.1 remapping, lifecycle, wrong-stream and malformed-data
  handling, cancellation, and unsupported layouts.
- `vpx_contract.rs` proves real VP8 and VP9 bitstreams, odd 17x15 geometry, the full supported VP9
  planar matrix without downconversion, color and metadata, alpha and format rejection, lifecycle,
  corruption, cancellation, and generated WebM ingest through the real container backend.

Fixtures are constructed inside the contract files: raw audio/video planes, codec headers and
packets, and minimal Matroska/WebM byte streams. There is no separate fixture directory owned by
this module. Lossless paths compare bytes exactly; lossy paths use timing, metadata, signal energy,
channel identity, or bounded plane statistics as appropriate. The vendored headers themselves are
declaration evidence, not proof that advanced libvpx controls are linked, wrapped, or tested.

The mapping synthesis did not execute these codec suites. Their source records the enforced
contracts, while current runtime success still depends on the pinned native build inputs and, for
VP8/VP9, an ABI-compatible libvpx 1.16 shared library.

## Current status and risks

All eight advertised codec families have substantive decode and encode implementations and public
contract suites. No source shard reported an empty placeholder, `todo!`, `unimplemented!`, `TODO`,
or `FIXME` in the implementation surface. Current limitations and contradictions are explicit:

- The AV1 main capability row combines 8/10-bit depth with monochrome/4:2:0 as sets, which the
  registry contract defines as supported cross-products. Decode rejects 10-bit monochrome because
  no exact public pixel format exists. The capability therefore over-advertises that combination;
  the registration test checks the row but does not prove the rejected combination.
- Default registration eagerly constructs `VpxBackend`. A missing, wrong-version, or incomplete
  libvpx runtime prevents the entire default registry from being created, including unrelated
  PCM/AV1/audio backends. A bad explicit `SUPERI_LIBVPX_PATH` suppresses fallback search.
- The checked VPx shim uses vendored 1.16 headers with a dynamically loaded implementation. Version
  prefix and symbol checks reduce but do not eliminate ABI-layout drift. Function pointers cross
  `void *`, and ten-bit plane bytes assume the supported little-endian targets.
- The vendored libvpx surface is much broader than the wrapped path. SVC, multipass, caller output
  buffers, preview and PSNR packets, decrypt callbacks, external frame buffers, TPL, external rate
  control, ROI, active maps, and advanced threading are unexercised compatibility declarations.
- VPx decode owns timing per input packet. Flush rejects delayed frames because it has no retained
  packet timing for them, and one packet producing multiple frames requires evenly divisible
  duration. Decoded keyframe state is inherited from the packet for every frame.
- VPx encode policy is fixed: software CPU input, one thread, one-pass VBR, zero lag, estimated
  bitrate, automatic keyframes, and no user rate-control or quality surface. It has no GPU-resident,
  alpha, 12-bit, RGB, or non-planar path.
- AV1 decoder flush does not submit a distinct rav1d end marker; it reports EOS after immediate
  output and queued input are exhausted. Reordered-stream coverage must continue to guard delayed
  picture behavior. The Windows offset range is finite and now fails explicitly before submission
  rather than truncating sequence identity. AV1 encode is CPU-only.
- FLAC encode buffers the complete interleaved stream and emits only at flush. Long recordings can
  consume unbounded memory. Headerless packet decode assumes independently decodable FLAC frames
  combined with retained configuration.
- Vorbis retains a grow-only Ogg byte vector and input metadata intervals for an encoder lifetime.
  Encoder quality and bitrate remain library defaults. Decode and encode support only standard
  semantic layouts through eight channels, and later decoder packet timestamps do not introduce
  discontinuities after the initial cursor.
- MP3 is fixed to 128 kbit/s CBR, mono/stereo I16, and one complete frame per packet. Interior
  OxideAV work has only boundary cancellation checks, and delayed output plus metadata spans remain
  allocated until drain/reset.
- Opus decoder reset clears remaining pre-skip rather than restoring the retained header's initial
  pre-skip, so reset-after-seek differs from a fresh stream replay unless the header is resent.
  Later packet PTS values do not override the inferred contiguous cursor.
- PCM changes only storage and byte order; it does not resample, normalize, or convert numeric sample
  types. Raw PCM is not self-describing and always needs an external codec ID and audio format.
- Error recoverability is not perfectly uniform across codec modules. Consumers should honor the
  full typed category and context rather than infer policy from codec class.
- The crate requires a C compiler even for consumers interested only in Rust codec paths because
  the VPx shim build is unconditional. Vorbis and Opus also retain BSD C boundaries despite the
  preference for pure Rust where mature.
- `superi-image` is an unused manifest dependency. Current video output is CPU-owned, so this module
  does not itself satisfy the architecture's later GPU-residency goal; upload ownership lies above
  the codec interface.
- The repository's codec policy treats these formats as royalty-free and dependencies as
  permissive, but the architecture still marks final codec-boundary legal review as open. The
  libvpx notice and patent-grant obligations are distribution requirements, not optional comments.

## Maintenance notes

After any source change under `open/crates/superi-codecs-rs`, rerun the mapping script's `files` and
`hash` commands, update the inventory, metadata, and every affected behavior statement, then run the
map validator. A new source, test, vendored header, license, or fixture file must appear explicitly
in this inventory.

Changes to a backend registration must update its codec operations, detailed correlated capability
rows, stable ID, registry ordering, selection tests, engine capability consumers, and this map. Do
not flatten profile/depth/chroma rows into independent lists, and resolve the AV1 10-bit monochrome
advertisement before relying on it for automated format selection.

Changes to packets, stream metadata, audio/video storage, decoder/encoder lifecycle, or operation
context must be reconciled with the owning `superi-media-io` map and raw interfaces. Changes to
container codec IDs or `codec.configuration`, delay, padding, or color keys must update both the
container and codec contract tests.

Treat `vp9.rs`, `vpx_ffi.rs`, `vpx_shim.c`, `vpx_shim.h`, the vendored headers, runtime version
filter, and VPx tests as one compatibility unit. Any enum value, field order, symbol list, ABI
version, plane geometry, ownership, or lifetime change requires end-to-end verification against the
packaged libvpx runtime. Keep vendored license and patent files intact and avoid broad formatting of
vendor material.

Native ownership changes must preserve unique handles, retained library lifetime, copy-before-next-
call rules, exact destructor pairing, checked arithmetic, and typed error context. Buffering changes
must preserve temporal metadata attribution and exact logical duration across codec delay, lookahead,
overlap, and padding.

Do not add encumbered, proprietary, or copyleft codec implementations to this crate. OS-licensed
codecs belong behind `superi-codecs-platform`; vendor RAW belongs behind the separate process host.
Re-run license and offline verification whenever a dependency, bundled native artifact, runtime
search path, or redistribution package changes.
