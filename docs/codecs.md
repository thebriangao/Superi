# Superi: Codec Support Policy & Matrix

**Status:** Foundational policy. Living document, versioned and dated.
**Version:** 0.6
**Date:** 2026-07-13
**Audience:** Engineers, contributors. Operationalizes the codec/licensing boundary from
`architecture.md §2`, `§4.6`, and open item #1 into a concrete, per-format support plan.

---

## 0. The governing rule

> **Royalty-free -> permissive in our tree, using pure Rust where mature. Patent-encumbered -> the user's OS (opt-in). Vendor RAW -> optional user-installed SDK plugin.**

Two facts make this the only coherent policy:

1. **Patents, not licenses, encumber H.264/H.265/H.266/ProRes/AAC.** A patent covers the *act of
   decoding*, regardless of the code's license. No license choice, MIT or otherwise, makes a
   codec patent go away. You cannot write your way to a legal in-tree H.264 decoder.
2. **"100% MIT, zero exceptions" is a property of _our source tree and our distribution_, not of the
   user's whole machine.** Every program calls a proprietary OS, GPU driver, and firmware; the locked
   stack already does (wgpu → Metal/D3D12/Vulkan). So encumbered decode performed by the *user's OS*
  , which licensed it, keeps our tree 100% clean while still opening real footage.

The **default build is royalty-free-only**: 100% permissive, zero copyleft, zero proprietary even at
runtime. The encumbered OS path is an explicit opt-in (`os-codecs` cargo feature). The offline +
license CI runs the default configuration, so the clean guarantee is machine-proven.

## 1. Launch set (v1 target)

A genuinely usable professional editor, every row legal, MIT tree provably clean:

- **In-tree, all users (permissive, using pure Rust where mature):**
  AV1, VP9, VP8 · MP3, FLAC, Vorbis, Opus, PCM · EXR, DPX, PNG, JPEG, TIFF · MP4/MOV/MKV/MXF demux.
- **OS opt-in (`os-codecs`), Mac + Windows + supported Linux VA drivers:**
  H.264, H.265, ProRes, AAC. Linux currently covers H.264 and HEVC Main 8-bit decode plus H.264
  opaque NV12 export when the installed VA driver exposes those operations.
- **Later (post-launch):**
  H.266 (VVC), DNxHR, camera RAW (ARRIRAW / R3D / BRAW via vendor plugins).

This opens essentially everything a working editor sees day-to-day.

## 2. Full matrix

### Video

| codec | role | patent | acquisition path | user coverage |
|---|---|---|---|---|
| **AV1** | delivery, modern web | royalty-free | in-tree: `rav1d` 1.0.0 decode + `rav1e` 0.7.1 encode (BSD-2-Clause, pure Rust) | all platforms |
| **VP9 / VP8** | web / legacy | royalty-free | in-tree backend over official `libvpx` 1.16.0 (BSD-3-Clause) through a checked local FFI shim | all platforms when the pinned runtime is bundled |
| **H.264 / H.265** | acquisition + delivery (bulk of footage) | encumbered | **OS**: VideoToolbox / Media Foundation / Linux VA-API | Mac and Windows system support; Linux H.264 and HEVC Main 8-bit support is driver-dependent |
| **H.266 (VVC)** | emerging | encumbered | OS where present (installed-base support thin in 2026) | limited today |
| **ProRes** | pro mezzanine / acquisition | Apple proprietary | **OS**: VideoToolbox on Mac; runtime-discovered Media Foundation transforms on Windows | Mac ✓ · Windows depends on installed MFTs · Linux gap |
| **DNxHD / DNxHR** | broadcast intermediate | VC-3 is a SMPTE standard, cleaner than ProRes | TBD: possibly in-tree | `[VERIFY]` patent status |
| **ARRIRAW / R3D / BRAW** | high-end camera RAW | vendor proprietary SDKs | optional **user-installed vendor worker** through the MIT host adapter; SDK code never enters the MIT tree | explicit opt-in, worker availability and platform support are reported at runtime |

### Audio

| codec | patent | acquisition path | note |
|---|---|---|---|
| **PCM** | free | in-tree `superi-codecs-rs` backend (MIT, no external dependency) | codec complete; WAV and AIFF containers tracked separately |
| **FLAC** | free | in-tree backend using `claxon` 0.4.3 decode plus `flacenc` 0.4.0 encode (Apache-2.0, pure Rust) | 8, 12, 16, 20, and 24 bit precision; one to eight channels |
| **Vorbis** | free | in-tree `lewton` 0.10.2 decode (MIT OR Apache-2.0) plus `vorbis_rs` 0.5.4, `aotuv_lancer_vorbis_sys` 0.1.4, and `ogg_next_sys` 0.1.3 encode (BSD-3-Clause, bundled C) | codec complete; raw packet transport, exact sample timing, semantic channel mapping, metadata, reset, and deterministic output are covered by public contracts; encoder state is isolated on its worker thread; exact versions preserve Rust 1.80 |
| **Opus** | free | in-tree audited wrapper over `libopus_sys` 0.3.3 (MIT) with statically bundled `libopus` 1.5 (BSD-3-Clause) | codec complete; decode and encode cover 8, 12, 16, 24, and 48 kHz, signed 16-bit and float packed or planar audio, and standard one through eight channel mappings |
| **MP3** | **expired (2017)** | `oxideav-mp3` at immutable revision `f37901b5d9c691b113e96a3bb95645c67af1a046` (MIT, pure Rust, Rust 1.80) | decode and CBR encode through the default backend |
| **AAC** | AAC-LC core largely expired ~2017; HE-AAC murkier | route via **OS** (rides in the same MP4s as H.264) | `[VERIFY]` before claiming "free" |
| **AC-3 / Dolby** | proprietary | OS only | low priority |

### Image / VFX sequences

All royalty-free, all in-tree, all platforms, handled in `superi-image::io`, **not** the codec crates.
This is the entire VFX/color mezzanine, with no caveats:

`OpenEXR` (`exr`, BSD-3) · `DPX` (open spec) · `PNG` (`png`, MIT/Apache) · `JPEG` baseline (expired,
MIT/Apache) · `TIFF` · `WebP` (royalty-free) · `TGA` / `BMP`.

### Containers (demux)

Parsing a container has **no** patent issue (distinct from decoding the codec inside it).

`MP4 / MOV` and `MXF` use bounds-checked pure-Rust parsers in `superi-media-io`; `MKV / WebM` will
use an audited permissive Rust parser. The MXF path preserves partition, package, track, descriptor,
index, edit-rate, and generic-container essence relationships without claiming codec decode.

## 3. Where each lives (crate mapping)

| crate | responsibility |
|---|---|
| `superi-media-io` | the decode/encode **interface** + pure-Rust container demux + image-sequence IO |
| `superi-codecs-rs` | **default backend**, in-tree permissive royalty-free video/audio codecs (PCM, AV1, VP9, Opus, Vorbis, FLAC, MP3), using pure Rust where mature and documented BSD C boundaries where needed |
| `superi-codecs-platform` | **opt-in backend** (`os-codecs` feature), OS decode for H.264/H.265/H.266/ProRes/AAC (MIT binding code; `unsafe` FFI boundary) |
| `superi-codecs-vendor` | **opt-in host adapter** (`vendor-codecs` feature), revisioned process protocol for explicitly selected ARRIRAW, R3D, and BRAW worker executables; contains no vendor SDK or vendor code |
| `superi-image::io` | still/sequence image formats (EXR, DPX, PNG, JPEG, TIFF, WebP, …) |

Backends register behind the `superi-media-io` interface; the engine core only ever knows the
interface, never a concrete codec.

Every Superi-owned native codec call is isolated behind that safe interface. The complete unsafe
operation, ownership, callback, buffer, threading, failure, and target inventory is maintained in
[`unsafe-ffi.md`](unsafe-ffi.md), together with the compiler and target-specific audit commands.

### macOS VideoToolbox implementation contract

The opt-in macOS backend uses the generated `objc2` 0.3.2 bindings for Core Foundation, Core
Media, Core Video, VideoToolbox, Core Audio Types, and AudioToolbox. These dependencies are enabled
only for the macOS target and are licensed under permissive alternatives. The backend registers the
stable identity `apple-videotoolbox` as one primary candidate. It never probes or opens containers,
and it never silently substitutes another codec backend after a native failure.

Video decode and encode support `h264`, `hevc`, `prores-422-proxy`, `prores-422-lt`, `prores-422`,
`prores-422-hq`, and `prores-4444`. MP4 and MOV sample entries `apco`, `apcs`, `apcn`, `apch`, and
`ap4h` normalize to those profile-specific identifiers while retaining the original RFC 6381
metadata. H.264 and HEVC ingest requires a bounds-checked `codec.configuration` byte record. ProRes
ingest requires explicit `video.width` and `video.height` metadata.

Decoded video retains its `CVPixelBuffer` and crosses the media interface as External storage, so a
native frame can be passed directly into the matching encoder without a pixel copy. CPU encoder
input accepts BGRA8 and RGBA16F. CoreMedia timestamps are converted only when the exact rational
value fits; malformed configuration, incompatible storage, host rejection, cancellation, and
post-flush input each return an explicit typed error. Flush drains delayed native output, and reset
recreates the session for seeking or a new stream lifetime.

AAC decode and encode use AudioConverter with the stable `aac` identifier. Decode requires an
explicit packed PCM output format plus checked `codec.configuration` metadata. Both raw
AudioSpecificConfig and ESDS magic-cookie forms are accepted. Packed U8, I16, I24, I32, F32, and
F64 PCM with one to eight semantic channels is supported. Packet timing remains on the exact sample
clock, the configured channel layout is retained, flush drains pending packets, and reset clears
converter state. Planar PCM and unavailable host operations fail explicitly instead of being
reported as supported work.

### Windows Media Foundation implementation contract

The opt-in Windows backend discovers synchronous Media Foundation transforms at runtime for H.264,
HEVC, AAC, and the four 4:2:2 ProRes sample-entry identities. It advertises only operations returned
by the current host. H.264, HEVC, and AAC availability varies with the Windows edition, version,
and installed media components. ProRes is exposed only when an installed transform declares
lossless v210 input or output for the requested profile and direction. ProRes 4444 remains
unadvertised on Windows because the public frame contract cannot retain its alpha plane without
loss. No inbox ProRes implementation is assumed.

COM initialization, Media Foundation startup, activation objects, transforms, samples, and shutdown
remain on dedicated worker threads. The safe media interface preserves packet or frame metadata and
exact Superi timing through a transform provenance ledger. H.264 and HEVC MP4 configuration and
length-prefixed samples are checked and converted to the Annex B input required by Media Foundation.
AAC AudioSpecificConfig or `esds` metadata is validated before constructing native media types.
Decode produces validated NV12, P010, planar 10-bit 4:2:2, or packed I16 CPU storage. Encode accepts
NV12, P010, BGRA8, planar 10-bit 4:2:2, or packed I16 as permitted by the selected codec and
publishes codec configuration on the first output packet when the native type or stable AAC
configuration provides it.

The backend supports flush and drain, reset after seeking, cancellation checks between bounded
native calls, deterministic shutdown, and typed unsupported, corrupt-data, conflict, unavailable,
and resource errors. Hardware and asynchronous transforms are not advertised because their D3D
manager and event ownership contracts are not yet projected through the public GPU boundary. This
keeps the current capability set truthful instead of copying GPU media into an undisclosed fallback.

### Vendor RAW worker implementation contract

ARRIRAW, R3D, and BRAW support is absent from the ordinary engine registry. Enabling the
`vendor-codecs` feature compiles only the MIT host adapter. A caller must provide each worker
executable through `VendorPluginConfig`; Superi does not search for, download, bundle, or load a
vendor SDK. The engine starts each selected executable in a separate process with an empty inherited
environment, validates its handshake completely, and publishes capabilities only after every new
worker can be registered atomically. Duplicate backend identifiers, protocol mismatches, empty or
duplicate format declarations, missing executables, and failed handshakes leave the registry
unchanged.

Protocol revision 1 uses one strict, bounded, newline-delimited JSON request and response at a time.
It covers content-based probing, source open and close, source fingerprints, packet reads, exact
seeking, decoder creation, packet submission, receive, flush, reset, and classified failure. Every
identifier, timebase, duration, metadata key, pixel format, color tag, alpha mode, plane geometry,
and hexadecimal payload is rebuilt through checked public constructors before it reaches an engine
consumer. Operation cancellation and deadlines remain active while waiting on process locks, writes,
and reads. A timed out, oversized, invalid-JSON, unterminated, mismatched-identifier, closed, or
unresponsive worker is terminated. Other invalid protocol values return a terminal typed failure
instead of silently falling back.

The worker may expose `arriraw`, `r3d`, `braw`, or any nonempty subset, and the host advertises only
`Source` plus decode for those declared formats. Vendor RAW encode is not advertised. Revision 1
returns validated CPU frames and can preserve any pixel, color, alpha, timing, and metadata semantics
already represented by `superi-media-io`. Cross-process shared memory and GPU handles require a
future negotiated protocol revision after their ownership model is added to the public GPU boundary.
General discovery, signature scanning, operating-system sandbox policy, permissions UI, and
quarantine remain owned by their later extension checkpoints; this media checkpoint provides the
explicit worker boundary and crash containment they will coordinate.

Current vendor facts are tracked from primary sources. The
[ARRI Image SDK](https://www.arri.com/en/learn-help/learn-help-camera-system/pre-postproduction/file-formats-data-handling/arriraw)
is available to ARRI Partner Program developers for ARRIRAW and MXF/ARRIRAW processing. The
[RED R3D SDK](https://www.red.com/developers) supports R3D loading and decoding on Windows, macOS,
and Linux. [Blackmagic RAW SDK 5.1](https://www.blackmagicdesign.com/developer/products/braw/sdk-and-software)
provides macOS, Windows x86, and Linux packages plus CPU or GPU decode, metadata, and sidecar APIs.
Those SDKs remain the responsibility of the separately installed worker and its distributor.

### MP3 implementation contract

`superi-codecs-rs` adapts the MIT `oxideav-mp3` implementation at the exact Git revision above,
with `oxideav-core` pinned to 0.1.29. The published `oxideav-mp3` 0.1.3 package does not yet expose
the completed decoder and audio encoder, so the immutable upstream commit is required until a
complete permissive release is published.

The default backend accepts canonical mono or left-right stereo at 8, 11.025, 12, 16, 22.05, 24,
32, 44.1, or 48 kHz. Decoded and encoded audio uses signed 16-bit packed or planar storage. It
preserves exact sample timestamps and packet metadata, rejects fractional-sample timing and
unsupported layouts, resets state for seeking, and emits encoded packets after flush because the
implementation schedules its bit reservoir across the complete input stream.

### VP8 and VP9 implementation contract

`superi-codecs-rs` compiles its own narrow C shim against the official libvpx 1.16.0 public
headers at commit `1024874c5919305883187e2953de8fcb4c3d7fa6`. The headers, library license, and
patent grant are retained under `vendor/libvpx`; no MPL Rust binding crate is used. The shim owns
the concrete C structs and accepts only function pointers loaded by the Rust backend, so a normal
Superi build does not link a host development library.

At runtime the backend requires the ABI-matched libvpx 1.16 shared library. Release packaging must
place it beside the executable, while `SUPERI_LIBVPX_PATH` provides an explicit development and
deployment override. The backend also searches the platform library names and standard Homebrew
locations on macOS. A missing or incompatible runtime is an unavailable backend error, which keeps
fallback explicit.

VP8 accepts opaque 8-bit planar YUV 4:2:0. VP9 accepts opaque planar YUV 4:2:0, 4:2:2, and 4:4:4
at 8 or 10 bits. Frames and packets retain exact timestamps, duration, keyframe state, color tags,
and namespaced metadata. The codec copies all planes through libvpx-owned images, including odd
subsampled dimensions, and carries Superi's complete primaries, transfer, matrix, and range tags in
packet metadata when the VPx bitstream cannot express every axis. WebM alpha uses a distinct
payload path that the current Matroska reader does not expose, so the primary color decoder rejects
a declared alpha stream instead of silently discarding it.

### Opus implementation contract

The default Opus backend statically builds the permissive `libopus` source bundled by
`libopus_sys`, so ingest, playback, and export do not require a system codec or a network
connection. A small audited wrapper in `opus.rs` uniquely owns each native decoder and encoder,
checks every pointer, length, status, and variadic control call, and destroys the matching state on
drop. `libopus_sys` 0.3.3 retains Rust 1.80 and uses a CMake 3.16 policy floor compatible with CMake
4.

Decode and encode support the five native Opus sample rates and signed 16-bit or 32-bit float
packed or planar audio. OpusHead parsing preserves pre-skip, input sample-rate metadata, output
gain, mapping family, and standard one through eight channel meaning. Packet timing compensates
encoder lookahead, final padding is explicit, Matroska discard padding trims the decoded tail, and
packet or block metadata crosses the codec boundary. Reset clears buffered codec state for seeking
and stream replay, while unsupported rates, layouts, malformed headers, corrupt packets, timeline
gaps, and cancelled operations fail through typed media errors.

### AV1 implementation contract

The default `rust-av1` backend decodes raw AV1 temporal units through `rav1d` 1.0.0 and encodes
them through `rav1e` 0.7.1. Both dependencies are pinned to BSD-2-Clause releases compatible with
the workspace Rust 1.80 floor. Optional assembly, command-line binaries, signal handling, and
global threading features are disabled. The private rav1d ownership wrapper contains its unsafe
dav1d-compatible API boundary, and decoded pictures are copied into validated Superi storage.
`av1-grain` 0.2.4 and `jobserver` 0.1.34 are pinned because their next patch releases raise the
minimum compiler beyond Rust 1.80.

Decode and encode preserve exact frame timestamps, durations, packet metadata, and semantic color
signaling. Raw H.273 color identifiers are also retained in namespaced frame metadata. The encoder
accepts CPU-addressable opaque monochrome and planar or semiplanar YUV at 8 or 10 bits, including
NV12 and P010 conversion. Unsupported precision, GPU-only storage, and non-opaque alpha fail
explicitly instead of changing media silently. Reset reconstructs codec state for predictable seek
and relink replay, while flush drains the stream to a deterministic end-of-stream result.

### Linux VA-API implementation contract

The opt-in `linux-vaapi` backend uses `cros-codecs` 0.0.6 and `cros-libva` 0.0.12, both
BSD-3-Clause, to parse compressed syntax and submit VA parameter buffers. The pixel decode and
encode transform remains inside the user's installed VA driver. Superi does not ship a VA driver,
software H.264 or HEVC transform, FFmpeg, GStreamer, or a native codec object through this path.
The dependencies are Linux-targeted and enter the build only through the platform codec crate.

Building the opt-in path requires the system development files for `libva`, `libva-drm`, DRM, GBM,
Clang, and `pkg-config`. A Debian-family development environment can provide them with `libva-dev`,
`libdrm-dev`, `libgbm-dev`, `clang`, and `pkg-config`. Runtime operation requires a readable and
writable DRM render node plus a VA driver that exposes the requested profile and entrypoint.
`SUPERI_VAAPI_RENDER_NODE` can select one absolute render-node path; otherwise nodes under
`/dev/dri` are considered in stable lexical order.

Registration is capability-based. A machine with no usable render node, GBM device, or supported
profile registers no Linux platform backend, leaving normal registry selection and fallback intact.
The current truthful surface is H.264 Baseline, Main, or High 8-bit decode, HEVC Main 8-bit decode,
and H.264 opaque NV12 encode. HEVC Main10 is not advertised because the current public frame path
does not describe the native high-bit-depth layout exactly. H.264 and HEVC alpha payloads are not
supported and are rejected instead of discarded. H.264 export accepts CPU NV12 and frames returned
by this VA backend. Declared color signaling on export is rejected until the native encoder can
write the exact requested VUI fields.

Native display, decoder, and encoder objects stay on dedicated worker threads. Decoded DMA-BUF
owners cross the public media boundary without CPU readback. MP4 and Matroska AVC or HEVC decoder
configuration records are converted to checked Annex B access units, while existing Annex B input
passes through. Opaque tokens restore signed presentation timestamps, decode timestamps, exact
durations, timebases, keyframe state, and metadata after native reordering. Flush drains delayed
output, and reset clears timing, configuration, references, and queued output for predictable seek
and relink replay. Native dependency panics are contained at the worker boundary and reported as
typed unavailable errors instead of unwinding through editor code.

## 4. License policy

Permissive-class allowlist, **zero copyleft**: MIT, BSD-2-Clause, BSD-3-Clause, Apache-2.0,
Apache-2.0 with LLVM exception, ISC, NCSA, CC0-1.0, Zlib, Unicode. Copyleft is denied,
GPL/LGPL/AGPL **and MPL** (weak copyleft still counts).

- **Consequence:** `Symphonia` (the popular all-in-one pure-Rust media crate) is **MPL-2.0 → excluded**.
  Compressed audio is assembled from **per-codec permissive crates**
  (`claxon`/`lewton`/`vorbis_rs`/`libopus_sys`) instead, while PCM is implemented in-tree
  without an external dependency. This is an accepted, recurring cost of the zero-exception rule.
- Enforced by `cargo-deny` (`open/deny.toml`); wired into CI in a later pass.

## 5. Caveats & accepted tradeoffs

1. **Linux system-codec coverage remains driver-dependent.** Linux has no universal licensed OS
   codec layer. The VA-API path leans on system-installed libraries and drivers the user supplies,
   still their machine and not our tree. Supported H.264 and HEVC operations register only when the
   active render node exposes them. "All users" is true for royalty-free codecs; "most users" is
   the honest word for encumbered codecs.
2. **C-via-FFI creeps back** for the Vorbis encoder, `libvpx`, and `libopus`. These are license-clean
   (BSD) and patent-clean (royalty-free), so they pass the rule, but they are C at an `unsafe`
   boundary, against the Rust-native *spirit*. Choose per codec: pure-Rust when mature (AV1 uses
   `rav1d`/`rav1e`), BSD-C binding when not. Vorbis keeps the non-`Send` bundled encoder state on a
   dedicated worker thread behind the safe `vorbis_rs` API. VP8 and VP9 keep every concrete libvpx
   struct inside a checked local C shim and copy decoded planes into Superi-owned immutable storage.
3. **ProRes is Mac-centric.** Decode + encode uses VideoToolbox on Mac. Windows advertises only
   profile and direction pairs reported by installed Media Foundation transforms; no inbox ProRes
   transform is assumed. Linux remains unsupported without user-supplied libraries.
4. **H.266/VVC OS support is thin** in 2026; treat as forward-looking, not a launch guarantee.
5. **FLAC stays on the Rust 1.80 floor.** `flacenc` 0.4.0 is built without optional default
   features and its `built` helper is pinned to 0.7.1. Newer `flacenc` releases require Rust 1.83,
   while newer `built` 0.7 releases generate code that does not compile on Rust 1.80.
6. **Opus keeps one audited native boundary.** `libopus_sys` 0.3.3 is pinned, `libopus` 1.5 is
   built statically from the dependency's bundled source, and no system library is loaded at
   runtime. Native state has unique Rust ownership and is exposed only through the codec-neutral
   backend.
7. **AV1 inherits the completed `paste` 1.0.15 macro.** Both pinned AV1 implementations use this
   compile-time proc macro. It has no known vulnerability, but its archived status produces the
   informational `RUSTSEC-2024-0436` unmaintained advisory. The advisory remains visible and is not
   ignored. Replace it through future codec releases when that is compatible with the Rust floor.

## 6. Open items to verify (before these harden)

Architecture-level confirmation belongs to the codec legal review (`architecture.md` open item #1).
The specific facts to nail down:

- `[VERIFY]` **AAC-LC patent status**, confirm whether AAC-LC can be treated as patent-free, or must
  stay strictly on the OS path.
- `[VERIFY]` **DNxHR / VC-3 patent status**, determines whether it can be an in-tree codec.

## 7. The hard line

The MIT tree **never** links GPL/LGPL/MPL or patent-encumbered code. Encumbered decode happens only
on the user's OS, behind the opt-in feature. This is enforced, not promised: the **network-isolated
offline CI test** proves the default build needs no server, and **`cargo-deny`** proves every bundled
crate is permissively licensed. Both are sibling guarantees to the offline law (`architecture.md §2`).
If a task seems to require linking encumbered or copyleft code into the core, **stop and flag it**;
it does not belong in this tree.
