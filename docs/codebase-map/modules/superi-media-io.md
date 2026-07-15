---
module_id: superi-media-io
source_paths:
  - open/crates/superi-media-io
source_hash: 62bc4d621705bdcaa15e41595578b52754a4489873940b55ae8327549bcffc66
source_files: 39
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-media-io` owns Superi's codec-neutral media boundary. It defines stable source identity and location contracts, content-driven backend probing and deterministic selection, exact packet and decoded-media values, decode and encode lifecycle traits, cooperative operation controls, corruption-aware reads, stream selection, presentation timing, source timecode metadata, image-sequence I/O, waveform-preview adaptation, and four in-tree container source backends.

The implemented container paths are Matroska/WebM, MP4/MOV, MXF, and RIFF/WAVE, RF64/WAVE, or AIFF PCM. Each opens a local file or immutable memory source, binds the caller's project `MediaId` to a SHA-256 content fingerprint, publishes container-neutral `SourceInfo`, and returns timed packets through `MediaSource`. Matroska, MP4, and MXF use private structural parsers; PCM parsing is integrated into its public module.

This crate owns contracts and demuxing, not a complete editor media pipeline. It has no concrete image-sequence filesystem backend, no muxer, no real codec implementation in this module, no backend discovery or dynamic loading, and no scheduler. Concrete software, platform, and vendor codec implementations live in downstream crates. Waveform raster ownership remains in `superi-image`, while shared identities, errors, time, timecode, color, pixel, and audio-layout primitives remain in `superi-core`.

## Source inventory

- `open/crates/superi-media-io/Cargo.toml`: Declares the crate, workspace policy, SHA-256 implementation dependency, and direct `superi-core` and `superi-image` dependencies.
- `open/crates/superi-media-io/src/audio_io.rs`: Defines validated decoded `AudioFormat`, immutable `AudioPlane`, and timed packed or planar `AudioBlock` values with exact byte-layout and metadata checks.
- `open/crates/superi-media-io/src/backend.rs`: Defines backend descriptors, coarse and detailed capability declarations, registrations, tiers, fallback policy, deterministic selection, bounded content probing, source-probe selections, and the `MediaBackend` factory trait.
- `open/crates/superi-media-io/src/decode.rs`: Defines decoded video descriptors, CPU/GPU/external buffer ownership, plane-layout validation, `VideoFrame` with complete color pipeline metadata, `DecoderConfig`, decoder outputs, and the decoder lifecycle trait.
- `open/crates/superi-media-io/src/demux.rs`: Defines stable backend, codec, container, and stream identifiers; typed metadata; source identity and location; streams and edits; packet values and timing; source probing requests; seek modes; and the `MediaSource` trait.
- `open/crates/superi-media-io/src/encode.rs`: Defines decoded encoder inputs, output stream configuration, packet outputs, and the encoder input, drain, reset, and end-of-stream lifecycle trait.
- `open/crates/superi-media-io/src/image_seq.rs`: Defines logical image-sequence timing, stable file-frame labels, random-access frame reads, sequential validated output, retryable finalization, and image-sequence backend traits.
- `open/crates/superi-media-io/src/lib.rs`: Documents the codec-neutral ownership model and exposes sixteen public modules while keeping the three container parsers private.
- `open/crates/superi-media-io/src/matroska_parser.rs`: Privately performs bounded EBML header inspection and complete in-memory Matroska/WebM structure, track, cluster, block, lacing, cue-count, and frame-range parsing.
- `open/crates/superi-media-io/src/mkv_webm.rs`: Implements `MkvWebmBackend`, complete-source residency with a 512 MiB bound, SHA-256 relinking, parser adaptation, metadata projection, deterministic packet interleaving, and seek behavior.
- `open/crates/superi-media-io/src/mp4_mov.rs`: Implements `Mp4MovBackend`, ISO BMFF and QuickTime probing, full-source adaptation, timestamp normalization, edit and VFR presentation mapping, packet interleaving, and presentation-aware seeking.
- `open/crates/superi-media-io/src/mp4_parser.rs`: Privately parses bounded classic and focused fragmented MP4/MOV atom structures, sample tables, edit lists, selected metadata, codec configuration, and sample byte ranges.
- `open/crates/superi-media-io/src/mxf.rs`: Implements `MxfBackend`, semantic metadata-graph resolution, essence and index association, material edits, public metadata projection, packet interleaving, and seek behavior.
- `open/crates/superi-media-io/src/mxf_parser.rs`: Privately parses MXF run-in, KLV records, partitions, primers, local sets, index segments, generic-container essence, and random index packs with exact source offsets and structural validation.
- `open/crates/superi-media-io/src/operation.rs`: Defines ordered media priority, shareable one-way cancellation, process-local monotonic deadlines, remaining-time queries, and cooperative interruption checks.
- `open/crates/superi-media-io/src/pcm.rs`: Implements PCM container probing and demuxing for WAVE, RF64, and AIFF, including format validation, Broadcast Wave and ancillary metadata, frame-aligned packets, partial truncation recovery, fingerprinting, and exact seeks.
- `open/crates/superi-media-io/src/preview.rs`: Converts contiguous decoded audio blocks into channel-ordered waveform envelopes and delegates raster construction to `superi-image`.
- `open/crates/superi-media-io/src/read.rs`: Defines corruption kinds and reports, complete/partial/EOS read outcomes, and exact synchronous reads with interruption checkpoints and byte-progress evidence.
- `open/crates/superi-media-io/src/selection.rs`: Defines explicit or unambiguous video/audio stream pairing, unchanged packet routing, decoder-config projection, and identity-checked selection rebinding after relink.
- `open/crates/superi-media-io/src/timecode.rs`: Implements timestamp-origin normalization, exact edit-list mapping, QuickTime-style timecode sample descriptions and flags, byte-exact timecode sample coding, and segmented source-timecode projection.
- `open/crates/superi-media-io/src/vfr.rs`: Builds bounded, interruptible, exact contiguous presentation maps from packet timing or explicit frames and provides half-open frame lookup and VFR classification.
- `open/crates/superi-media-io/tests/contracts.rs`: Exercises public media values, CPU and opaque GPU frame storage, audio layout validation, capability declarations and selection, thread-safety markers, interruption propagation, and a fake packet-to-decode-to-encode lifecycle.
- `open/crates/superi-media-io/tests/audio_fixture_contract.rs`: Consumes the three canonical synchronized multichannel WAVE fixtures through `PcmContainerSource` and proves exact common sample rates, frame timing, channel masks and order, full interleaved sample identity, shared signal boundaries, distinct routing gains, and bounded audible continuity.
- `open/crates/superi-media-io/tests/color_image_sequence_fixture_contract.rs`: Consumes the
  canonical color and image-sequence catalogs, verifies exact references, per-image hashes, rational
  timing, and noncontiguous file numbering, and reads and seeks exact f32 frame bytes through the
  public `ImageSequenceSource` path.
- `open/crates/superi-media-io/tests/image_sequence_contract.rs`: Exercises logical, file-frame, and presentation addressing, authoritative frame validation, relinking, retryable writes and finalization, and idempotent completion using memory fixtures.
- `open/crates/superi-media-io/tests/interruption_contract.rs`: Exercises priority ordering, cancellation, deadlines, exact reads, retry after `Interrupted`, partial truncation evidence, impossible-reader defense, and send/sync contracts.
- `open/crates/superi-media-io/tests/media_error_fixture_contract.rs`: Consumes the canonical
  malformed WAVE, truncated AIFF, unsupported AIFC, and partial-readable WAVE cases. It verifies
  their strict catalog and byte mutations, exercises production open classifications, applies the
  declared post-open truncation, and proves the aligned partial packet plus exact corruption
  evidence.
- `open/crates/superi-media-io/tests/mkv_webm_contract.rs`: Exercises content probing, EBML defaults, metadata, all lacing modes, absent timing, unknown-size elements, packet ordering, seeking, relinking, source bounds, and malformed Matroska/WebM fixtures.
- `open/crates/superi-media-io/tests/mp4_mov_contract.rs`: Exercises MP4/MOV probing, classic and fragmented samples, ProRes and VVC identity, edit normalization, VFR timing, packet ordering, seeking and preroll, relinking, truncation, and non-advancing cancellation.
- `open/crates/superi-media-io/tests/mxf_contract.rs`: Exercises a synthetic OP1a metadata graph, run-in probing, tracks, material edits, index-driven packets and seeking, relinking, path parity, truncation, and non-advancing cancellation.
- `open/crates/superi-media-io/tests/pcm_containers.rs`: Exercises WAVE extensible, BWF, RF64, AIFF, ancillary preservation, sample formats and channel layouts, exact seeking, relinking, malformed schemas, interruption, and sample-aligned partial recovery.
- `open/crates/superi-media-io/tests/probe_contract.rs`: Exercises incremental bounded probing, content authority over extensions, deterministic candidate ordering, tiered fallback exposure, path and memory parity, selected open behavior, and probe/open errors.
- `open/crates/superi-media-io/tests/selection_contract.rs`: Exercises explicit and unambiguous stream pairing, descriptor-preserving decoder configs, lossless packet classification, relink rebinding, structured failures, and thread safety.
- `open/crates/superi-media-io/tests/timecode_contract.rs`: Exercises timestamp normalization and forward or reverse edit-list mapping across empty, repeated, dwell, non-unit-rate, mixed-timebase, and half-open cases.
- `open/crates/superi-media-io/tests/timecode_metadata_contract.rs`: Exercises timecode descriptions, raw flags, 32-bit and 64-bit big-endian samples, drop-frame labels, counters, physical rates, source segments, 24-hour wrapping, and schema failures.
- `open/crates/superi-media-io/tests/timing_fixture_contract.rs`: Consumes the canonical versioned
  timing fixture, enforces its exact 11-field schema and five-case inventory, and exercises CFR,
  decode-order VFR, drop-frame, gap, reset, and per-segment normalization behavior through public
  packet, presentation-map, timestamp, and source-timecode interfaces.
- `open/crates/superi-media-io/tests/vfr_contract.rs`: Exercises decode-order input sorting, inferred presentation durations, half-open lookup, compatible timebase conversion, negative coordinates, validation failures, overflow, resource bounds, and cancellation.
- `open/crates/superi-media-io/tests/video_fixture_contract.rs`: Consumes the canonical versioned raw-video fixture, proves the complete 23 pixel format by 9 standard rate matrix, verifies exact plane layout, offsets, hashes, and numeric representation, and constructs all 207 cases through the public CPU video-frame path.
- `open/crates/superi-media-io/tests/waveform_preview_contract.rs`: Exercises packed and planar sample normalization, channel ordering, exact peak buckets, width capping, source preservation, continuity and format validation, and nonfinite audio rejection.

## Public surface

The crate root exposes `audio_io`, `backend`, `decode`, `demux`, `encode`, `image_seq`, `mkv_webm`, `mp4_mov`, `mxf`, `operation`, `pcm`, `preview`, `read`, `selection`, `timecode`, and `vfr`. `matroska_parser`, `mp4_parser`, and `mxf_parser` are crate-private implementation layers.

### Common media values and lifecycles

- `demux` owns ordered string identifiers with a restricted lowercase grammar, source-local `StreamId`, deterministic typed `MediaMetadata`, `StreamInfo`, exact `StreamEdit`, optional-field `PacketTiming`, immutable packet bytes, source identity and location, probe requests, seek requests, and `MediaSource`.
- `audio_io` owns exact decoded audio representation. Packed data is one interleaved plane; planar data is one plane per ordered channel; decoded multi-byte samples are little-endian. Construction checks frame counts, sample clocks, plane count, byte count, and overflow.
- `decode` owns nonzero `VideoFormat`, validated `CpuVideoBuffer`, opaque object-safe `VideoFrameBuffer` for CPU, GPU, or external owners, immutable timed `VideoFrame` with exact image-owned color pipeline metadata, `DecoderConfig`, `DecodeOutput`, and `Decoder`. New frames default that pipeline to the format's authoritative color space; `with_color_pipeline` accepts richer source payloads and ordered transform history only when source interpretation matches the format.
- `encode` owns audio or video `EncoderMediaFormat`, stream and codec `EncoderConfig`, `EncodeInput`, `EncodeOutput`, and `Encoder`.
- Decoder and encoder contracts use explicit `NeedInput` and `EndOfStream` states. `flush` preserves delayed output and `reset` discards buffered state after a discontinuity. The traits carry `OperationContext`, but lifecycle legality and config/input consistency remain concrete-backend responsibilities.

### Backends, sources, and selection

- `MediaBackend` reports one stable descriptor, probes a bounded source prefix, opens sources, and creates decoders or encoders. `BackendCapabilities` keeps coarse source/decode/encode declarations plus correlated detailed codec rows and hardware mode.
- `BackendRegistry` rejects duplicate IDs, ranks primary and fallback tiers by descending priority then ascending backend ID, and performs bounded content probing. Profile, level, bit depth, chroma, and hardware detail are descriptive; coarse selection does not filter on them.
- `SourceProbeSelection` preserves the exact request, selected match, permitted fallback matches, bytes examined, and source length. `open` invokes only the selected backend. The registry does not retry an open or codec creation against fallbacks.
- `MkvWebmBackend`, `Mp4MovBackend`, `MxfBackend`, and `PcmContainerBackend` are concrete source backends. They intentionally reject decoder and encoder creation.
- `PairedStreamSelection` supports exactly one selected video stream and one selected audio stream, either explicitly or when the pair is unambiguous. It preserves complete stream descriptors, classifies packets without mutation, and rebinds only when both media ID and content fingerprint match.

### Timing, metadata, image sequences, and previews

- `TimestampNormalizer` shifts one exact stream origin without changing clock rate or PTS/DTS offset. `EditTimeline` maps between movie presentation and media time using integer rational arithmetic, fixed-point rates, empty edits, repeated media, dwell, half-open ranges, and caller-selected rounding.
- `VariableFrameRateMap` maps presentation-order frames only. It sorts packet samples by PTS, derives unknown durations from the next PTS or explicit final end, rejects duplicate/gapped/overlapping timelines, caps maps at 2,000,000 frames, and uses half-open lookup.
- `TimecodeDescription`, `TimecodeFlags`, `SourceTimecode`, `TimecodeSegment`, and `SourceTimecodeTrack` preserve storage width, raw flags, physical media rate, canonical label-counting rate, counter mode, source reference bytes, associations, and segmented timecode projection. These public contracts are not wired to a production container parser or exporter in this crate.
- `ImageSequenceTiming` keeps stable logical positions separate from signed replaceable file-frame labels. `ImageSequenceSource` validates backend frames against authoritative constant format and timing. `ImageSequenceOutput` enforces sequential addresses, exact frame properties, retryable writes and finish, and idempotence after publication.
- `WaveformRequest` and `generate_audio_waveform_image` convert validated contiguous `AudioBlock` slices into per-channel normalized peak envelopes, then call the image module's waveform renderer. They do not decode, mix, resample, play, or own raster semantics.

### Operations and corruption

- `MediaPriority` has stable ascending ranks from background through interactive. `CancellationToken` is a shareable one-way atomic flag. `OperationContext` adds priority and an optional monotonic deadline; cancellation wins when cancellation and expiration are both observable.
- `CorruptionReport` preserves corruption kind, recovery, optional stream and byte offset, and expected/actual byte progress. `ReadOutcome<T>` keeps complete values, usable partial values with evidence, and clean end-of-stream distinct.
- `read_exact_interruptible` accumulates ordinary short reads, retries `io::ErrorKind::Interrupted`, polls between calls, and returns exact progress. It cannot preempt one blocking `Read::read` call.

## Architecture and data flow

### Registry-driven ingest

1. A caller builds a `SourceRequest` from a persistent project `MediaId`, a replaceable path or immutable memory location, and optionally an expected prior fingerprint.
2. `BackendRegistry::probe_source` selects source-capable registrations in allowed tiers, reads an initial bounded prefix, and shows every eligible backend the same `SourceProbe`. Name and extension are untrusted hints.
3. `NeedMoreData` requests are combined, the prefix grows geometrically up to the configured maximum, and all candidates probe again. Matches are ordered by confidence, registration priority, and stable ID within policy. A primary-tier match beats every fallback-tier match; fallback is promoted only when permitted and no primary matches.
4. The selection opens only its chosen backend. Each in-tree container adapter reads or shares the complete source, computes canonical `sha256:<hex>`, checks any expected fingerprint, parses under panic containment, and constructs source and stream metadata.
5. `MediaSource::read_packet` returns complete packets or EOS for MKV/WebM, MP4/MOV, and MXF. PCM may also return a block-aligned usable partial packet with a truncation report. Packets retain their own exact PTS, DTS, duration, keyframe status, stream ID, bytes, and format metadata.
6. Seeks use the edited presentation timeline. Exact and keyframe modes are format-specific, so consumers must use the returned resolved time and treat packets before the target as possible decoder preroll.

Filesystem probing and source opening are synchronous. Repository policy requires them to run on an I/O or background worker. The operation contract adds cooperative checks, not asynchronous I/O or forced preemption.

### Container adapters

| Format path | Parse and packet flow | Timing and seek behavior | Resource and interruption boundary |
| --- | --- | --- | --- |
| Matroska/WebM | `MkvWebmBackend` uses bounded EBML inspection, reads at most 512 MiB, parses tracks and block or lace ranges, then interleaves one frontier item per track by presentation-derived sort keys. | Public timebase is nanoseconds. Laced packets retain absent timestamps or durations when they cannot be derived. Exact seek can begin at a non-keyframe; previous/nearest modes require keyframes. | Parser and long copies receive `OperationContext`; track sorting and seek scans are not fully polled. Packets are capped at 64 MiB and parser metadata has explicit limits. |
| MP4/MOV | `Mp4MovBackend` walks brands and atoms, reads the entire source, maps classic or focused fragmented samples, and interleaves tracks by normalized DTS then stream ID. | Minimum composition time becomes a normalized origin for PTS and DTS. `EditTimeline` handles empty, repeated, and dwell edits; `VariableFrameRateMap` owns frame boundaries. Exact seek returns the requested presentation point but queues from a preceding sync sample. | Adapter loops poll, but `mp4_parser` does not receive an operation. There is no whole-source or packet-byte cap, and fragment support is selective. |
| MXF | `MxfBackend` parses KLV structure, resolves structural sets and references, groups GC essence, selects descriptors, associates index entries, and interleaves tracks by decode time. | Timebase follows track, descriptor, or index rates. Material source clips become rate-1 edits; index temporal offsets produce PTS. Exact seek need not choose random access; keyframe modes use index evidence. | `mxf_parser` has high count limits but no operation context. The adapter has no source, packet, or copied-metadata byte cap and associates index entries to essence by ordinal. |
| WAVE/RF64/AIFF PCM | `PcmContainerSource` parses container structure directly, preserves stored bytes and ancillary chunks, and emits roughly 1 MiB frame-aligned packets from file or memory storage. | The integer sample rate is the timebase. BWF time reference becomes presentation origin. Every packet is a keyframe, and seek requires an exact sample boundary within the declared range. | Parsing, hashing, reading, and memory copies poll. Ancillary chunks are bounded at 64 MiB each, 4,096 chunks, and 256 MiB total. Post-open path truncation can yield a usable aligned partial packet. |

The Matroska parser supports no, Xiph, fixed, and EBML lacing, but it does not apply content transforms, track operations, WebM codec restrictions, rich cue seeking, chapters, tags, or attachments. The MP4 parser supports one sample description per track, selected codec config and metadata, classic sample tables, and a focused fragment subset; it does not model encryption or the full fragmented standard. The MXF parser preserves structural byte ranges and raw local-set values, while the adapter implements a focused metadata graph and material-edit model rather than broad operational-pattern or codec-label coverage.

### Decode, encode, and decoded representation

After source and stream selection, callers pass packets in decode order to a backend-created `Decoder`. Output can be immutable video with CPU, GPU, or external storage or exact packed/planar audio. A decoder may request more input, emit delayed output during drain, reach EOS, and reset after seek. Frames keep precise timing, format, buffer ownership, and metadata without an obligatory pixel copy.

Callers pass decoded `VideoFrame` or `AudioBlock` values to an `Encoder`, which returns timed packets under the same nonblocking input, drain, EOS, and reset model. The media-I/O types do not enforce that every submitted variant matches the config or that emitted packets match configured stream ID and timebase. Concrete backend implementations and tests must enforce those relationships.

No concrete decoder or encoder is implemented in this crate. Software codecs in `superi-codecs-rs`, operating-system adapters in `superi-codecs-platform`, and explicit external workers in `superi-codecs-vendor` implement these interfaces. Container adapters stop at packet delivery, and no muxer consumes encoder packets here.

### Selection, relinking, and presentation metadata

`PairedStreamSelection` operates after `SourceInfo` discovery and before decoder creation. It refuses ambiguity rather than selecting by source order, retains the exact descriptors used to build decoder configs, and routes all other streams as `Unselected`. Rebinding checks both project identity and content fingerprint before resolving the same stream IDs in the reopened source.

Container metadata is kept in deterministic `BTreeMap` values and retains typed text, signed or unsigned integers, booleans, and shared byte arrays. Container adapters project their supported fields without claiming a unified schema: MP4/MOV retains brands, selected `ilst` metadata, edits, codec config, and raw timing; Matroska retains document, track, lacing, and block metadata; MXF retains graph and raw local-set fields; PCM retains format, BWF, SSND, and ancillary information. Metadata-key validation enforces syntax, not namespace ownership.

Source timecode metadata is separate from edit mapping. The byte-exact QuickTime-style timecode contracts and tests preserve sample width, flags, counters, physical rate, label rate, associations, and display wrapping, but no current production container path creates a `SourceTimecodeTrack` or exports one.

### Image sequences and waveform previews

Image sequences are parallel to packetized `MediaSource`, not an encoded container special case. Timing maps logical image numbers to file-frame labels and exact presentation frames. Reads are random access and validated against authoritative info. Outputs accept frames sequentially, retry the same logical address after a writer failure, and publish a fingerprint only after every expected frame. The traits do not carry `OperationContext`, and this crate contains no concrete naming, file I/O, image codec, collision, atomic-directory, or rollback policy.

Waveform preview begins after audio decode. The adapter checks one stable audio format and a gap-free sample clock, normalizes U8, I16, I24, I32, F32, and F64 packed or planar samples, partitions every source frame into at most the requested number of columns, and produces one min/max peak per ordered channel. `superi-image` then enforces image limits and owns the sRGB raster. The sample scan is synchronous and has no operation context.

## Dependencies and consumers

### Direct dependencies

- `superi-core` supplies the canonical error taxonomy and context, `MediaId`, exact timebases, rational time, duration, sample time, frame rate, time ranges and rounding, timecode labels, pixel formats, color spaces, alpha modes, sample formats, and ordered channel layouts. This crate validates and composes those types rather than duplicating them.
- `superi-image` supplies waveform envelope, peak, style, raster result, rendering contracts, and the canonical color pipeline metadata used by decoded video. Dependency direction is one way: media I/O retains image-owned metadata and interprets decoded PCM, while image remains independent of media lifecycles.
- `sha2` computes canonical source fingerprints for all four in-tree container adapters. Identity hashing is interruptible in adapter-sized chunks and is distinct from project identity.

### Verified consumers and implementers

- `superi-codecs-rs` implements permissive AV1, linear PCM, MP3, FLAC, Vorbis, Opus, VP8, and VP9 backends against the registry, capability, packet, frame, audio, decode, encode, and operation contracts. Its default registration function populates an external `BackendRegistry` atomically.
- `superi-codecs-platform` implements host-dependent VideoToolbox, Media Foundation, and VA-API registrations behind the same public contracts. Registration is opt-in through the engine's `os-codecs` feature and includes only host-discovered operations.
- `superi-codecs-vendor` adapts explicitly configured external RAW workers into `MediaBackend` implementations and uses operation, packet, decoder, encoder, corruption, and frame-conversion contracts. It does not alter this crate's discovery policy.
- `superi-engine` constructs registries from Rust codecs and optional platform or vendor backends,
  registers all four in-tree source backends, converts declarations into deterministic capability
  snapshots, adapts project-owned referenced-media paths into local `SourceRequest` values,
  compiles reachable timeline media requests into opened sources and selected decoders, consumes
  `VideoFrame`, `MediaMetadata`, and the complete color pipeline at its CPU-frame-to-GPU upload
  boundary, and adapts complete generated proxy packets or a verified original source behind one
  `MediaSource`. Foreground playback snapshots exact `VideoFrame` format, timestamp, duration,
  metadata, color history, and alpha meaning as graph-result provenance without copying decoded
  pixel storage. Render-export orchestration consumes an acquired source and selected decoders
  through exact seek, complete and partial read handling, packet routing, decode drain and flush,
  audio and video provenance validation, one-shot encoder selection, encode drain and flush, packet
  validation, and reset-based recovery.
- `superi-api` has a test-only direct dependency for public capability fixtures; its production path consumes engine-owned projections rather than media-I/O types directly.
- `superi-concurrency` has a test-only direct dependency used to prove backpressure with decoded frame, audio block, and media metadata payloads.
- Codec integration tests in `superi-codecs-rs` directly connect `MkvWebmBackend` packets to AV1, VP8/VP9, Opus, and FLAC codec implementations, proving selected cross-crate compositions beyond the media-I/O fake backend tests.
- The canonical audio-fixture integration test directly consumes `superi-fixture-tool` output without adding a runtime dependency. It exercises the real PCM container source and shared channel and time contracts for 44,100 Hz stereo, 48,000 Hz 5.1, and 96,000 Hz 7.1.
- The canonical image-sequence integration test likewise consumes generated repository artifacts
  without a runtime tool dependency. It binds three ACEScg f32 payloads to logical images, signed
  file labels, and presentation timestamps through the public sequence reader contract.
- The canonical media-error integration test directly consumes the same repository fixture boundary
  without a runtime tool dependency. It drives malformed, truncated, unsupported, and post-open
  partially readable cases through `PcmContainerSource` and the shared error and corruption
  vocabulary.

`superi-engine::media` is the production registry owner for `MkvWebmBackend`, `Mp4MovBackend`,
`MxfBackend`, and `PcmContainerBackend`. Its resource preparation path uses bounded probing, opens
the selected source once, maps explicit stream IDs into exact decoder configurations, selects one
decoder, and retains source and decoder policy evidence with one timeline compilation. Its project
request adapter obtains the path, `MediaId`, and expected fingerprint from `superi-project`; this
crate receives only the resolved location and integrity evidence and owns no project path syntax.
Engine proxy
substitution separately provides a `MediaSource` adapter over generated packets and a verified lazy
original-source seam. Foreground playback consumes caller-prepared decoded provenance and graph
values, but it does not yet bind prepared sources and decoders to scheduled graph requests. Export
now binds explicit acquired source routes through decode, graph or audio processing, and encode into
complete in-memory elementary packet streams. `nodes`, transport, native presentation, container
muxing, and output publication remain incomplete, so there is still no scheduled
source-to-decode-to-playback or encode-to-container flow. Repository search likewise finds no
production consumer for paired selection, source timecode tracks, image-sequence traits, or
waveform generation.

## Invariants and operational boundaries

- Project identity is independent of location. Relinking may change a path or memory label, but a caller-supplied prior fingerprint must match the newly opened bytes before the source is accepted.
- `SourceLocation::Path` is already resolved runtime input. Portable relative path grammar,
  project-file context, explicit missing state, and persistent target versions belong to
  `superi-project`, not this crate or a container backend.
- Content bytes are authoritative for container selection. File names and extensions can inform probes but cannot create a match.
- Identifiers and metadata keys use deterministic syntax, maps and sets preserve stable order, and backend ranking has explicit tie-breakers. The registry is intended to be shared after registration; concurrent mutation is not synchronized.
- `SourceInfo` requires at least one stream and unique stream IDs. Packet PTS, DTS, and duration are optional and remain absent when a container cannot derive them safely.
- Exact timebases remain attached to every timing value. Edit and presentation intervals are half-open. Cross-timebase conversions are checked and apply an explicit rounding rule where exact representation is unavailable.
- Decoded video format must match buffer dimensions and pixel format. CPU plane counts, rows, minimum strides, and exact allocation lengths are validated. The pipeline's source color interpretation must equal the frame format, while named-space and ICC payloads remain exact. Opaque GPU/external storage keeps backend ownership and synchronization responsibilities.
- Decoded audio timestamp rate must equal format rate. Packed and planar byte counts are overflow-checked and channel ordering follows the shared `ChannelLayout`.
- Decoder and encoder lifecycle ordering is documented at trait boundaries, not encoded as a type-state machine. Concrete implementations must reject illegal send, receive, flush, and reset sequences and preserve state across interruption correctly.
- Operation cancellation and deadlines are cooperative. Most adapters poll around bounded chunks, but a single platform read cannot be preempted, MP4 and MXF private parsers do not receive the operation, image-sequence and waveform APIs have no operation argument, and several seek or sort loops have weaker polling.
- Partial recovery is explicit, never inferred from a short packet. PCM can expose whole sample frames from a truncated read with a `CorruptionReport`; the other current container sources deliver atomic packets or fail.
- All owned Rust source avoids `unsafe`. Structural parsers and adapters check lengths, offsets, additions, multiplications, slice endpoints, and platform conversions before indexing. The remaining dominant safety risks are resource use, selective format semantics, and backend-owned external resource lifetimes.
- Resource policy is format-specific. Matroska has strong source and packet caps, VFR has a frame-count cap, and PCM bounds ancillary retention. MP4/MOV and MXF fully materialize sources without equivalent byte caps, and packet or metadata copies can be large.
- Probe and open are separate operations with no source snapshot lock. A path may change between them; expected fingerprint validation is the integrity boundary, while an open without an expected fingerprint accepts the bytes observed at open time.
- Image-sequence output assumes that a successful frame write means durable acceptance and that retries after errors are safe. Publication followed by an empty fingerprint can leave finalization ambiguous because the generic layer learns the invalid identity only after writer publication.

## Tests and verification

The eighteen integration-test files exercise each public concern with deterministic in-memory builders, temporary path fixtures, and the canonical shared video, audio, timing, image-sequence, and media-error baselines:

- Shared values and fake decode/encode composition: `contracts.rs`.
- Canonical synchronized multichannel PCM coverage: `audio_fixture_contract.rs`.
- Canonical color image-sequence addressing and payload coverage:
  `color_image_sequence_fixture_contract.rs`.
- Image-sequence input/output: `image_sequence_contract.rs`.
- Priority, cancellation, deadlines, exact reads, and corruption reports: `interruption_contract.rs`.
- Canonical malformed, truncated, unsupported, and partial-read PCM containers:
  `media_error_fixture_contract.rs`.
- Content probing, ranking, and fallback exposure: `probe_contract.rs`.
- Container adaptation: `mkv_webm_contract.rs`, `mp4_mov_contract.rs`, `mxf_contract.rs`, and `pcm_containers.rs`.
- Paired selection and packet routing: `selection_contract.rs`.
- Timestamp/edit mapping and source timecode metadata: `timecode_contract.rs` and `timecode_metadata_contract.rs`.
- Canonical cadence, decode-order, drop-frame, and discontinuity coverage:
  `timing_fixture_contract.rs`.
- VFR construction and lookup: `vfr_contract.rs`.
- Canonical pixel-format and standard-frame-rate coverage: `video_fixture_contract.rs`.
- Decoded-audio waveform generation: `waveform_preview_contract.rs`.

Implementation-local unit tests add focused bounds and mapping proof: Matroska packet, copied-metadata, and duration limits; MP4 table bounds, VVC configuration, SHA-256, edit expansion, dwell seek, and VVC mapping; PCM ancillary count and aggregate budgets; and platform-independent parser helpers. The MXF parser and adapter have no embedded unit tests and rely on the MXF integration contract.

The strongest end-to-end format proofs are synthetic: all Matroska lacing modes, classic and one-fragment MP4/MOV, one OP1a MXF graph, and detailed WAVE/RF64/AIFF schemas. MP4/MOV and MXF tests open every proper fixture prefix under panic containment and verify no prefix succeeds. Container and PCM tests verify memory/path parity and relink conflicts. MP4/MOV, MXF, and PCM verify non-advancing cancellation for read and seek; Matroska's contract proves cancelled probing but not the same complete runtime matrix.

The canonical raw-video contract constructs every current `PixelFormat::ALL` value at all nine standard `FrameRate` constants. It proves exact odd-dimension packed, planar, semiplanar, and chroma geometry; contiguous catalog ranges and hashes; finite floats; 10-bit bounds; P010 alignment; exact rational timing; and public `VideoPlane`, `CpuVideoBuffer`, `VideoFormat`, and `VideoFrame` integration. The data is one synthetic raw frame per case, so it does not prove encoded codecs, CFR or VFR sequences, HDR, malformed media, native hardware, scheduling, muxing, or real-time performance.

The canonical audio contract opens three WAVEFORMATEXTENSIBLE PCM16 files through `PcmContainerSource`. It proves exact sample-rate timebases and 100 ms frame counts, stereo, 5.1, and 7.1 mask projection into canonical routing order, complete interleaved sample identity, synchronized ten millisecond onset and 90 ms tail boundaries, channel-specific gains, and a maximum adjacent-sample delta of 600. It does not decode, resample, play audio, route a device, measure a physical clock, prove A/V synchronization, or claim real-time performance.

The canonical timing contract reads five cases and 18 samples from one fixed CRLF catalog. It proves
24 fps CFR continuity, decode-order VFR sorting and duration classification, continuous physical
29.97 frames across skipped drop-frame labels, rejection of unsegmented timestamp gaps and resets,
and reversible normalization for every declared continuity segment. This is public timing-contract
proof over synthetic metadata, not container parsing, codec output, scheduler behavior, or hardware.

The canonical image-sequence contract reads three ACEScg f32 image references from fixed CRLF
catalogs. It proves logical images 0 through 2, file labels -2, 0, and 2, presentation timestamps 48
through 50 at 24000/1001 fps, contiguous payload hashes, random access, seek resolution, and exact
public `VideoFrame` bytes. The test uses an in-memory fixture reader, so concrete filesystem naming,
still-image decoding, publication, and operation cancellation remain unproved.

The canonical media-error contract reads four cases from one fixed CRLF catalog and checks the
critical header mutations independently. Production `PcmContainerSource` open proves malformed WAVE
and truncated AIFF as `corrupt_data`, unsupported AIFC as `unsupported`, and their exact recovery
classifications. The partial case opens a complete WAVE seed, truncates its temporary path to 53
bytes, and proves an 8-byte, two-frame partial packet with stream 0, byte offset 44, expected 16,
actual 9, `truncated`, and `degraded` evidence. This focused PCM baseline does not claim exhaustive
malformed coverage for Matroska/WebM, MP4/MOV, MXF, codecs, hardware, or playback recovery.

The other fixtures prove implemented contracts, not broad real-world compatibility, native codec behavior, encrypted media, muxing, scheduling, export atomicity, or real-time performance. The fake decode/encode pipeline proves trait composition and lifecycle only. Waveform tests assert peak data but not complete raster pixels. Image-sequence tests use memory backends and do not prove filesystem naming or publication. Timeout behavior is tested at shared boundaries, not inside every format parser.

## Current status and risks

The module is substantive and test-rich at the value, probing, demux, timing, selection,
interruption, PCM, and waveform-adapter layers. It provides four real source backends and exact
contracts consumed by three codec families plus engine source and decoder preparation,
introspection, upload, generation, transparent proxy or original-source resolution, and exact
render-export lifecycle orchestration. It is not merely a scaffold.

Its canonical fixture consumers now cover the complete current raw pixel-format and
standard-video-rate matrix, synchronized multichannel PCM at three common audio rates, exact timing
cadences, noncontiguous image-sequence addressing, and four deterministic PCM error or degradation
paths. These are format, container, timing, representation, and focused failure contracts. Engine
registry, editorial preparation, and explicit render-export integration are now real, but broad
compatibility, scheduled playback, muxing, and output publication are not.

Principal incomplete behavior and risks are:

- Engine preparation requires callers to choose every decoder stream explicitly. It does not yet
  consume paired selection, language or role metadata, multiple stems, or output-storage constraints.
- Decode and encode remain boundary contracts in this crate. In-tree codecs implement them and the
  engine composes an explicit complete elementary-stream export transaction, but there is no
  in-module codec, muxer, container output, or publication owner.
- Image-sequence input/output has no concrete backend and no operation context. File naming, missing-frame policy, codec choice, collision handling, rollback, and atomic publication are undefined here.
- Source timecode metadata is byte-exact and test-proven but not produced by current container adapters or consumed by an exporter.
- Paired selection models one video and one audio stream only. It does not choose by language or role, select multiple stems, or own subtitle/data routing beyond `Unselected`.
- Detailed codec capability tuples are introspection metadata. Registry selection considers only coarse source/decode/encode capability, tier, priority, and backend identity.
- Matroska parsing omits content transforms, track relationships, cue indexing, tags, chapters, attachments, and WebM restrictions. Its exact seek can begin at a non-keyframe, unlike MP4 decoder-preroll behavior.
- MP4/MOV uses only the first sample description, has focused fragment support, does not parse encryption or most auxiliary structures, and does not globally recheck total samples accumulated across individually bounded fragment runs.
- MXF semantic projection uses a small fallback UL vocabulary, focused material components, ordinal index-to-essence association, generic codec IDs, and first/preferred-track duration rules. Valid complex files may expose ambiguous candidates or timing not represented by the synthetic OP1a test.
- MP4/MOV and MXF have full-source residency and packet-copy paths without explicit byte ceilings. Matroska has tighter bounds but still materializes the full source. Opening cost and memory scale with file size.
- Cancellation guarantees differ across formats and phases. Blocking reads, uninstrumented parser loops, sort/seek scans, waveform generation, and image-sequence traits can exceed expected response latency.
- `ImageSequenceTiming::with_presentation_start` narrows `u64` frame count to `i64` with `as` for one end-coordinate check, while address lookup uses checked conversion. Extremely large counts can therefore be validated inconsistently.
- Parser panic containment converts unexpected parser panics into corrupt-media errors, but it is not a substitute for resource budgets or a representative compatibility corpus.

## Maintenance notes

When adding or changing media behavior, update the common contract and its actual implementers together. A new pixel or sample representation may require changes in decoded buffer validation, software/platform codec adapters, engine upload, waveform normalization, and tests. A new codec capability must keep coarse registration and correlated detail consistent.

Container changes should keep probe, full parser, public metadata projection, packet ordering, seek behavior, fingerprint checks, resource bounds, operation polling, and contract fixtures aligned. Do not generalize one format's cancellation, error category, packet atomicity, source cap, or seek behavior to another.

Keep project target decoding outside media I/O. New persistent path forms must be interpreted by
`superi-project`, adapted by engine, and arrive here only as an explicit `SourceLocation` with stable
caller-owned media identity and optional expected fingerprint.

Keep the synchronized audio fixture contract aligned with shared channel layouts, exact sample clocks, the PCM parser's WAVE mask projection, and the repository fixture generator. Changing waveform bytes, timing boundaries, rates, masks, or channel order requires a new immutable fixture version and matching reproduction evidence.

Keep the canonical image-sequence fixture aligned with `ImageSequenceTiming`, signed file-frame
labels, presentation-start semantics, constant `VideoFormat`, and exact referenced sample bytes.
Changing catalog identity, numbering, timing, or payloads requires a new immutable fixture version
and matching color and sequence consumer evidence.

Keep the media-error fixture contract aligned with strict PCM open behavior, shared error codes,
`ReadOutcome::Partial`, block alignment, and `CorruptionReport` byte progress. Changing parser
classifications, seed bytes, mutation offsets, or the post-open truncation lifecycle requires a new
immutable fixture version and matching generator and consumer evidence.

Preserve the distinction among decode order, presentation order, edited presentation time, source timecode, and sample-clock coordinates. Changes to `StreamEdit`, timestamp normalization, VFR mapping, or timecode metadata require reviewing MP4/MOV seek and duration behavior plus selection and downstream codec expectations.

Keep all four source backends under the explicit `superi-engine` registry owner with primary tier,
priority 100, preflighted stable IDs, and real integration proof. Do not make the registry
auto-discover implementations or silently execute fallback candidates. Preserve complete
`SourceIdentity` equality during resource preparation and at the proxy or original seam. Add real
consumers before treating paired selection, image sequences, timecode metadata, waveform generation,
or muxing as integrated engine flows. Keep the engine export consumer aligned with complete-read
admission, codec drain and reset lifecycles, exact packet timing and metadata, and no exception retry.

After source edits, regenerate this module's file inventory, source count, and hash, then reconcile every affected statement rather than updating metadata alone. Update maps for `superi-core`, `superi-image`, codec implementers, `superi-engine`, or public API consumers whenever their interface or dependency relationship changes.
