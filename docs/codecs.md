# Superi: Codec Support Policy & Matrix

**Status:** Foundational policy. Living document, versioned and dated.
**Version:** 0.3
**Date:** 2026-07-12
**Audience:** Engineers, contributors. Operationalizes the codec/licensing boundary from
`architecture.md §2`, `§4.6`, and open item #1 into a concrete, per-format support plan.

---

## 0. The governing rule

> **Royalty-free → pure-Rust, in our tree. Patent-encumbered → the user's OS (opt-in). Vendor RAW → optional user-installed SDK plugin.**

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

- **In-tree, all users (pure-Rust / permissive):**
  AV1, VP9 · MP3, FLAC, Vorbis, Opus, PCM · EXR, DPX, PNG, JPEG, TIFF · MP4/MOV/MKV/MXF demux.
- **OS opt-in (`os-codecs`), Mac + Windows:**
  H.264, H.265, ProRes, AAC.
- **Later (post-launch):**
  H.266 (VVC), DNxHR, camera RAW (ARRIRAW / R3D / BRAW via vendor plugins).

This opens essentially everything a working editor sees day-to-day.

## 2. Full matrix

### Video

| codec | role | patent | acquisition path | user coverage |
|---|---|---|---|---|
| **AV1** | delivery, modern web | royalty-free | in-tree: `rav1d` decode + `rav1e` encode (BSD-2, ~pure Rust) | all platforms |
| **VP9 / VP8** | web / legacy | royalty-free | in-tree; pure-Rust decode thin → likely `libvpx` (BSD-3) via FFI | all platforms |
| **H.264 / H.265** | acquisition + delivery (bulk of footage) | encumbered | **OS**: VideoToolbox / Media Foundation | Mac + Windows ✓ · Linux gap |
| **H.266 (VVC)** | emerging | encumbered | OS where present (installed-base support thin in 2026) | limited today |
| **ProRes** | pro mezzanine / acquisition | Apple proprietary | **OS**: VideoToolbox decode **and** encode (Mac) | Mac ✓ · Win decode limited · Linux gap |
| **DNxHD / DNxHR** | broadcast intermediate | VC-3 is a SMPTE standard, cleaner than ProRes | TBD: possibly in-tree | `[VERIFY]` patent status |
| **ARRIRAW / R3D / BRAW** | high-end camera RAW | vendor proprietary SDKs | optional **user-installed vendor plugin**; never in MIT tree | opt-in, post-launch |

### Audio

| codec | patent | acquisition path | note |
|---|---|---|---|
| **PCM** | free | in-tree `superi-codecs-rs` backend (MIT, no external dependency) | codec complete; WAV and AIFF containers tracked separately |
| **FLAC** | free | in-tree backend using `claxon` 0.4.3 decode plus `flacenc` 0.4.0 encode (Apache-2.0, pure Rust) | 8, 12, 16, 20, and 24 bit precision; one to eight channels |
| **Vorbis** | free | in-tree `lewton` (MIT/ISC, pure Rust) | clean |
| **Opus** | free | `libopus` (BSD-3) via FFI; pure-Rust decode thin | license-clean |
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
| `superi-codecs-rs` | **default backend**, in-tree royalty-free video/audio codecs (PCM, AV1, VP9, Opus, Vorbis, FLAC, MP3) |
| `superi-codecs-platform` | **opt-in backend** (`os-codecs` feature), OS decode for H.264/H.265/H.266/ProRes/AAC (MIT binding code; `unsafe` FFI boundary) |
| `superi-image::io` | still/sequence image formats (EXR, DPX, PNG, JPEG, TIFF, WebP, …) |

Backends register behind the `superi-media-io` interface; the engine core only ever knows the
interface, never a concrete codec.

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

## 4. License policy

Permissive-class allowlist, **zero copyleft**: MIT, BSD-2-Clause, BSD-3-Clause, Apache-2.0, ISC,
Zlib, Unicode. Copyleft is denied, GPL/LGPL/AGPL **and MPL** (weak copyleft still counts).

- **Consequence:** `Symphonia` (the popular all-in-one pure-Rust media crate) is **MPL-2.0 → excluded**.
  Compressed audio is assembled from **per-codec permissive crates**
  (`claxon`/`lewton`/`libopus`) instead, while PCM is implemented in-tree without an external
  dependency. This is an accepted, recurring cost of the zero-exception rule.
- Enforced by `cargo-deny` (`open/deny.toml`); wired into CI in a later pass.

## 5. Caveats & accepted tradeoffs

1. **Linux is the encumbered-codec gap.** The OS path covers Mac + Windows (the large majority of
   pros). Linux has no universal licensed OS codec layer (VA-API is hardware-accel, driver-dependent),
   so H.264/H.265 there leans on *system-installed* libraries the user supplies, still their machine,
   not our tree, but not automatic. "All users" is true for royalty-free; "most users" is the honest
   word for encumbered.
2. **C-via-FFI creeps back** for `libvpx` / `libopus` / possibly `dav1d`. These are license-clean
   (BSD) and patent-clean (royalty-free), so they pass the rule, but they are C at an `unsafe`
   boundary, against the Rust-native *spirit*. Choose per codec: pure-Rust when mature (AV1 has
   `rav1d`/`rav1e`), BSD-C binding when not.
3. **ProRes is Mac-centric.** Decode + encode via VideoToolbox on Mac; Windows OS decode is limited;
   Linux unsupported without user-supplied libs.
4. **H.266/VVC OS support is thin** in 2026; treat as forward-looking, not a launch guarantee.
5. **FLAC stays on the Rust 1.80 floor.** `flacenc` 0.4.0 is built without optional default
   features and its `built` helper is pinned to 0.7.1. Newer `flacenc` releases require Rust 1.83,
   while newer `built` 0.7 releases generate code that does not compile on Rust 1.80.

## 6. Open items to verify (before these harden)

Architecture-level confirmation belongs to the codec legal review (`architecture.md` open item #1).
The specific facts to nail down:

- `[VERIFY]` **AAC-LC patent status**, confirm whether AAC-LC can be treated as patent-free, or must
  stay strictly on the OS path.
- `[VERIFY]` **`rav1d` / `lewton` licenses + maturity**, confirm permissive license and
  production-readiness for decode. The FLAC backend dependency licenses and Rust floor are pinned
  and enforced by the workspace gates.
- `[VERIFY]` **DNxHR / VC-3 patent status**, determines whether it can be an in-tree codec.
- `[VERIFY]` **ProRes-on-Windows** decode path (Media Foundation coverage vs. none).

## 7. The hard line

The MIT tree **never** links GPL/LGPL/MPL or patent-encumbered code. Encumbered decode happens only
on the user's OS, behind the opt-in feature. This is enforced, not promised: the **network-isolated
offline CI test** proves the default build needs no server, and **`cargo-deny`** proves every bundled
crate is permissively licensed. Both are sibling guarantees to the offline law (`architecture.md §2`).
If a task seems to require linking encumbered or copyleft code into the core, **stop and flag it**;
it does not belong in this tree.
