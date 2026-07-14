---
module_id: tool-superi-fixture-tool
source_paths:
  - open/tools/superi-fixture-tool
source_hash: 7b59f5a4f238843f4c737a8d95210171151c28b003b672dcc0769658a4ba8b1e
source_files: 16
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-fixture-tool` owns offline validation for shared canonical fixtures and deterministic
generation for the version 1 raw-video, synchronized multichannel audio, timing, color and
image-sequence, media-error, and OTIO interchange baselines. It validates layout, manifest schema,
provenance, lineage, payload inventory, byte counts, and SHA-256
digests without fetching data or executing documentary generator commands. All six generators
create approved synthetic evidence directly and never replace an existing output path.

The package is a repository utility, not a runtime crate. The canonical fixture store and policy
remain under `open/test-fixtures`; this tool creates a requested new video, audio, timing, color,
media-error, or OTIO version directory and validates any fixture root, but it does not select
versions for consumers or mutate released data.

## Source inventory

- `open/tools/superi-fixture-tool/Cargo.toml`: Declares the workspace library and binary package and
  opts into workspace `serde`, `serde_json`, and `sha2` dependencies.
- `open/tools/superi-fixture-tool/src/lib.rs`: Implements strict fixture validation plus
  dependency-free video, audio, timing, color, media-error, and OTIO baseline generators. The video
  path owns stable format
  and rate tables, exact plane geometry, finite sample synthesis, CSV serialization, and manifest
  creation. The audio path owns WAVEFORMATEXTENSIBLE serialization, common sample rates, speaker
  masks, synchronized integer waveforms, and PCM interleaving. The timing path owns stable cadence,
  continuity, and timecode tables. The color path owns fixed SDR, wide-gamut, HDR, alpha,
  high-bit-depth, and image-sequence cases, exact little-endian samples, two catalogs, and sequence
  timing. The media-error path owns tiny PCM container serialization,
  controlled mutations and truncations, a fixed outcome catalog, and the exact partial-read recipe.
  The OTIO path owns fixed native JSON object construction, stable editorial identities, exact
  rational timing, explicit unsupported expectations, and the first slice projection. All six own
  exact manifests, reports, and no-overwrite guards.
- `open/tools/superi-fixture-tool/src/main.rs`: Implements `check`, `generate-video`,
  `generate-audio`, `generate-timing`, `generate-color`, `generate-media-errors`, and
  `generate-otio`, exact usage, summaries, diagnostics, and process exit statuses.
- `open/tools/superi-fixture-tool/tests/audio_cli_contract.rs`: Proves process-level audio
  generation, exact summary, manifest creation, complete usage, and no-overwrite failure.
- `open/tools/superi-fixture-tool/tests/audio_generator_contract.rs`: Compares every generated audio
  artifact byte for byte with the canonical version and proves report bounds and overwrite refusal.
- `open/tools/superi-fixture-tool/tests/color_cli_contract.rs`: Proves process-level color
  generation, exact image and sequence summary, manifest creation, complete usage, and no-overwrite
  failure.
- `open/tools/superi-fixture-tool/tests/color_generator_contract.rs`: Generates all color artifacts
  twice, compares them byte for byte with the canonical version, checks report and payload bounds,
  and proves preservation of an existing directory.
- `open/tools/superi-fixture-tool/tests/media_error_cli_contract.rs`: Proves process-level
  media-error generation, exact four-case summary, manifest creation, complete usage, and
  no-overwrite failure.
- `open/tools/superi-fixture-tool/tests/media_error_generator_contract.rs`: Generates media-error
  artifacts twice, compares every output with the canonical catalog, four media payloads, and
  manifest, checks exact count and tiny size bounds, and preserves an existing directory.
- `open/tools/superi-fixture-tool/tests/timing_cli_contract.rs`: Proves process-level timing
  generation, exact case and sample summary, manifest creation, and no-overwrite failure.
- `open/tools/superi-fixture-tool/tests/timing_generator_contract.rs`: Generates timing artifacts
  twice into temporary directories, compares both outputs byte for byte with the canonical version,
  checks report and size bounds, and proves preservation of an existing directory.
- `open/tools/superi-fixture-tool/tests/otio_cli_contract.rs`: Proves process-level OTIO generation,
  exact timeline summary, manifest creation, and no-overwrite failure.
- `open/tools/superi-fixture-tool/tests/otio_generator_contract.rs`: Generates OTIO artifacts twice,
  compares all timelines, expectations, and manifest bytes with the canonical version, checks the
  two-timeline report and payload bound, and proves preservation of an existing directory.
- `open/tools/superi-fixture-tool/tests/validator_contract.rs`: Exercises canonical and temporary
  fixture validation, drift, inventory, identity, provenance, path, and symlink failures.
- `open/tools/superi-fixture-tool/tests/video_cli_contract.rs`: Proves process-level generation,
  summary output, no-overwrite failure, exact usage, and exit statuses.
- `open/tools/superi-fixture-tool/tests/video_generator_contract.rs`: Regenerates all video artifacts
  into a temporary directory, compares them byte for byte with the canonical version, checks the
  207-case report and tiny payload bound, and proves preservation of an existing directory.

## Public surface

The library exports `validate_root(&Path) -> Result<ValidationReport, ValidationErrors>`.
`ValidationReport` exposes fixture and payload counts. `ValidationErrors` exposes stable structured
entries and deterministic display output with code, path, and message.

The library also exports `generate_video_baseline(&Path) -> io::Result<VideoBaselineReport>`, the
stable artifact names, and `VIDEO_BASELINE_CASE_COUNT`. A successful report exposes the case count
and payload byte count. The generator accepts only an absent output path, creates its parent if
needed, and emits `video-cases.csv`, `video-frames.bin`, and `fixture.json`.

The audio surface exports `generate_audio_baseline(&Path) -> io::Result<AudioBaselineReport>`, the
three stable WAVE artifact names, `AUDIO_MANIFEST_NAME`, and `AUDIO_BASELINE_CASE_COUNT`. A successful
report exposes the three-case count and total WAVE bytes. The generator accepts only an absent output
path, creates its parent if needed, and emits three WAVEFORMATEXTENSIBLE PCM16 files plus
`fixture.json`.

The media-error surface exports
`generate_media_error_baseline(&Path) -> io::Result<MediaErrorBaselineReport>`, the catalog, four
media artifact, and manifest names, plus `MEDIA_ERROR_BASELINE_CASE_COUNT`. Its report exposes four
cases, aggregate media payload bytes, and catalog bytes. It accepts only an absent output path and
emits `media-error-cases.csv`, malformed WAVE, truncated AIFF, unsupported AIFC, a complete
partial-readable WAVE seed, and `fixture.json`.

The timing surface exports
`generate_timing_baseline(&Path) -> io::Result<TimingBaselineReport>`, stable artifact names,
`TIMING_BASELINE_CASE_COUNT`, and `TIMING_BASELINE_SAMPLE_COUNT`. Its report exposes five cases, 18
samples, and catalog bytes. It accepts only an absent output path and emits `timing-cases.csv` plus
`fixture.json`.

The color surface exports
`generate_color_baseline(&Path) -> io::Result<ColorBaselineReport>`, stable artifact names,
`COLOR_BASELINE_IMAGE_COUNT`, and `COLOR_BASELINE_SEQUENCE_FRAME_COUNT`. Its report exposes eight
images, three sequence frames, and 448 payload bytes. It accepts only an absent output path and
emits `image-cases.csv`, `sequence-cases.csv`, `image-samples.bin`, and `fixture.json`.

The OTIO surface exports `generate_otio_baseline(&Path) -> io::Result<OtioBaselineReport>`, stable
names for two `.otio` payloads, `expectations.json`, and the manifest, plus
`OTIO_BASELINE_TIMELINE_COUNT`. Its report exposes two timelines and total payload bytes. It accepts
only an absent output path and emits the final canonical slice, comprehensive interchange coverage,
explicit preserve plus diagnose contracts, and exact `fixture.json`.

The executable accepts exactly these forms:

```text
superi-fixture-tool check [FIXTURE_ROOT]
superi-fixture-tool generate-video <OUTPUT_DIRECTORY>
superi-fixture-tool generate-audio <OUTPUT_DIRECTORY>
superi-fixture-tool generate-timing <OUTPUT_DIRECTORY>
superi-fixture-tool generate-color <OUTPUT_DIRECTORY>
superi-fixture-tool generate-media-errors <OUTPUT_DIRECTORY>
superi-fixture-tool generate-otio <OUTPUT_DIRECTORY>
```

Validation defaults to `test-fixtures`, prints fixture and payload counts on success, and exits 1
for policy failure. Video generation prints `generated 207 video cases`; audio generation prints
`generated 3 audio cases`; timing generation prints `generated 5 timing cases and 18 samples`;
color generation prints `generated 8 color images and 3 sequence frames`, and media-error
generation prints `generated 4 media error cases`. OTIO generation prints
`generated 2 OTIO timelines`. Every generator exits 1 for filesystem or overwrite failure. Invalid command shapes print the
complete seven-command usage and exit 2.

The accepted manifest format remains strict schema version 1. Its manifest, provenance, generator,
parent, and payload objects reject unknown fields. Generator records remain documentary for general
fixtures; the video, audio, timing, color, media-error, and OTIO commands are separate executable
implementations whose
canonical byte identities are proved by their integration tests.

## Architecture and data flow

Validation recursively discovers manifests, parses and hashes exact bytes, checks path-derived
identity and version, validates provenance rules, verifies every payload size and streamed digest,
resolves local parent manifests, rejects unlisted files, sorts errors, and returns counts only for a
complete successful pass.

Video generation uses 23 stable pixel-format codes and nine exact rational frame rates for 207
format-and-rate cases. Each case contains one 5 by 3 frame. Packed, planar, semiplanar, 4:2:0,
4:2:2, and 4:4:4 layouts use exact little-endian geometry, including ceiling chroma dimensions.
Stable patterns cover 8-bit and 16-bit unorm, bounded 10-bit, high-aligned P010, finite binary16,
and finite binary32 samples.

Every plane is appended contiguously to `video-frames.bin`. One fixed CSV row records the case,
format, exact rate, dimensions, plane index, offset, size, stride, rows, and digest. The generated
schema 1 manifest records CC0 provenance, generator identity and seed, artifact sizes, and hashes.
All bytes are computed before the new output directory is created. An explicit metadata check and
the atomic directory-create boundary prevent replacement, including ordinary races.

Audio generation uses three stable specifications: 44,100 Hz stereo with mask `0x0003`, 48,000 Hz
5.1 with mask `0x003f`, and 96,000 Hz 7.1 with mask `0x063f`. Each file is 100 ms of interleaved
PCM16 in a 40-byte WAVEFORMATEXTENSIBLE format chunk. A ten millisecond silent lead-in precedes an
integer-only 1 kHz triangle signal, which returns to zero at 90 ms before a ten millisecond silent
tail. Every channel shares exact signal boundaries and uses a distinct integer gain. The generated
schema 1 manifest records CC0 provenance, generator identity and seed, WAVE sizes, and hashes. All
bytes are computed before the new output directory is created.

Timing generation serializes one fixed 11-field CRLF catalog with five cases and 18 rows. CFR uses
four contiguous 24 fps samples. VFR stores three packets in decode order with distinct presentation
order and durations. The 29.97 drop-frame case keeps physical frames 1799 through 1801 continuous
while labels skip from `00:00:59;29` to `00:01:00;02`. Forward-gap and reset cases retain explicit
continuity segments. The generated schema 1 manifest records CC0 provenance, exact size, digest,
command, and stable seed before the absent output directory is created.

Color generation serializes a fixed 19-field image catalog and a fixed 7-field sequence catalog.
Eight 2 by 2 images cover premultiplied sRGB, straight Display P3 u16, BT.2020 PQ and HLG, ACEScg
f16, and three ACEScg f32 sequence frames. Exact little-endian values include zero alpha, negative
and above-one scene values, a 100 nit PQ reference white, and stable f16 and f32 bit patterns. The
sequence maps logical images 0 through 2 to file frames -2, 0, and 2 and presentation timestamps 48
through 50 at 24000/1001 fps. All 448 payload bytes and both catalogs are computed before the absent
output directory is created.

Media-error generation serializes fixed 60-byte stereo PCM16 WAVE and 70-byte AIFF sources. It
mutates the WAVE block-align field for the malformed case, removes the final AIFF byte for the
truncated case, changes the AIFF form type to AIFC for the unsupported case, and retains one complete
WAVE as the seed for cataloged post-open truncation to 53 bytes. A fixed 14-field CRLF catalog binds
each case to its production trigger, shared error and recovery codes, mutation or truncation values,
and partial packet evidence. The exact schema 1 manifest, CC0 provenance, sizes, and hashes are
computed before the absent output directory is created.

OTIO generation constructs two complete native JSON timelines at 24 fps. The canonical timeline
binds the immutable AV1 WebM identity, exact 48-frame edit, and editable mirror effect. The coverage
timeline adds adjacent clips and transition metadata, gaps, owner-relative markers, nested
composition, 2.0 and 0.5 linear time warps, and stable object IDs. A separate expectation record
pins OpenTimelineIO 0.18.1 and OTIO_CORE:0.18.1, records exact durations, and requires opaque
preservation plus a stable warning for FreezeFrame and a named generic effect. All JSON and manifest
bytes are computed before the absent output directory is created.

## Dependencies and consumers

The standard library supplies filesystem, path, collection, formatting, byte, and process support.
`serde` and `serde_json` parse manifests, while `sha2` computes payload, catalog, and manifest
digests. No external media tool, platform encoder, network service, or random source participates in
generation.

The binary and 13 integration-test files consume the library. `open/test-fixtures/README.md`
documents all six generation commands. The canonical-root validator consumes the complete fixture
store.
That store now includes the separately generated encoded canonical slice source, which this tool
validates as an ordinary strict manifest and opaque payload but does not reproduce.
Runtime crates do not depend on this tool; separate integration tests consume the emitted canonical
video, audio, timing, color, image-sequence, media-error, and OTIO artifacts. The video test checks
generator tables
indirectly against live core definitions. The audio test opens every WAVE through the production PCM
source and checks exact sample clocks, masks, routing, synchronization, and continuity. The timing
test exercises packet, presentation-map, timestamp, and source-timecode behavior. The color test
uses public input and output transforms, while the image-sequence test uses public random-access and
seek interfaces. The media-error
test checks the catalog and mutations independently, then drives all four cases through production
PCM open or packet-read behavior. `superi-timeline` adds no runtime tool dependency; its
development-only JSON contract consumes the
two OTIO timelines and expectations to prove hierarchy, identity, timing, relationships, metadata,
nesting, rate changes, canonical slice linkage, and unsupported handling.

## Invariants and operational boundaries

- Validation is offline and read-only. Generation is offline and writes only a newly created output
  directory.
- Fixture identity, version, provenance, paths, inventory, byte counts, and digests remain strict
  schema 1 contracts with deterministic error order.
- Released versions remain immutable by repository policy. The tool refuses overwrite but does not
  prove Git history or prevent a contributor from deleting a directory outside the tool.
- Video output is deterministic across supported hosts because sample values, byte order, geometry,
  catalog order, CRLF records, manifest text, and seed are fixed in Rust.
- Audio output is deterministic across supported hosts because WAVE fields, little-endian PCM16
  samples, integer-only waveform math, sample and channel order, manifest text, and seed are fixed in
  Rust.
- Timing output is deterministic across supported hosts because case order, exact rational clocks,
  source timestamps, continuity segments, labels, CRLF records, manifest text, and seed are fixed.
- Color output is deterministic across supported hosts because case order, exact numeric bits,
  little-endian byte order, color and timing fields, CRLF records, manifest text, and seed are fixed.
- Media-error output is deterministic across supported hosts because container fields, sample bytes,
  mutations, truncation lengths, catalog rows, manifest text, and seed are fixed.
- OTIO output is deterministic across supported hosts because object order, JSON serialization,
  schema labels, rational values, identities, relationship metadata, expectations, manifest text,
  and seed are fixed in Rust.
- Odd dimensions use ceiling division for chroma. Ten-bit planar values stay in 10 bits, P010 values
  occupy the high 10 bits of 16-bit containers, and floating-point samples are finite.
- The generator table is intentionally local to this repository tool. The media consumer contract
  fails if its format or rate matrix diverges from `superi-core`.
- Generator command and source fields in arbitrary manifests remain documentary and are never
  executed during validation.

## Tests and verification

Seven validator contracts cover the canonical root, success counts, content and inventory drift,
identity, versions, provenance, unsafe paths, and Unix symlinks. Six generator contract files prove
video, audio, timing, color, media-error, and OTIO artifacts reproduce byte for byte, report exact
case, sample, image, and timeline counts, stay within their payload bounds, and leave existing
directories unchanged. Six CLI contract files prove all six generation summaries, no-overwrite
diagnostics, complete usage,
manifest creation, and exit statuses.

Separate runtime contracts validate the generated media baselines through real consumers.
The video contract proves the full 23 by 9 matrix, exact rates and geometry, contiguous offsets,
per-plane hashes, numeric representation rules, and construction through public video frame types.
The audio contract proves all three common rates, exact WAVE channel masks and canonical layouts,
sample-aligned timing, synchronized signal boundaries, distinct channel routing, complete
sample identity, and bounded adjacent-sample continuity through `PcmContainerSource`. The canonical
timing contract proves its strict schema, CFR and VFR maps, decode and presentation order,
continuous drop-frame samples, unsegmented discontinuity rejection, and reversible segment
normalization. The color contract proves transfer order, HDR meaning, alpha association, output
intent, and exact high-bit-depth sample bits. The image-sequence contract proves exact catalog
references, timing, random access, seeking, and unmodified frame bytes. The media-error contract
proves fixed mutations, strict parser classifications, and the exact aligned partial packet plus
corruption evidence after a cataloged post-open truncation. The OTIO contracts add two timelines,
exact schemas and timing, stable editorial relationships, opaque preservation
expectations, and official OpenTimelineIO 0.18.1 semantic read plus write plus read equivalence.
The combined canonical validator reports eight fixture versions and 19 payloads.

## Current status and risks

Validation, deterministic video, audio, timing, color, media-error, and OTIO generation, all seven
CLI commands, canonical
artifacts, and real consumer proof are implemented. The video baseline is raw single-frame evidence,
the audio baseline is PCM-container evidence, the timing baseline is metadata evidence, and the
color baseline is raw color-transform and sequence evidence, and the media-error baseline is focused
WAVE, AIFF, and AIFC failure evidence. Together they still do not
prove encoded codec corruption, malformed Matroska, MP4, or MXF, HDR, playback, physical devices,
hardware clocks, A/V synchronization, scheduling, real-time behavior, or a production editorial
runtime. The OTIO baseline proves interchange data and expectations, not a reader or writer. The
encoded slice fixture participates in strict generic validation but has a separate documentary
FFmpeg generator and is not reproduced by this tool.

Validation still checks SPDX, media type, source, author, rights, and semantic quality only to the
degree documented by schema rules. It validates a filesystem snapshot rather than history. Lineage
does not reject cycles or duplicate parents. Concurrent external mutation can still race validation,
and a write failure after creating a new generation directory can leave a partial new directory for
the caller to inspect and remove.

## Maintenance notes

Keep all generator tables, WAVE and AIFF schemas, waveform math, mutations, truncation recipes, and
color sample bits, OTIO objects, expectations, and serializations intentionally stable for
version 1. Add a new fixture version when bytes or schema change. Any new core pixel format,
standard video rate, canonical audio rate, channel layout, timing case, cadence, discontinuity,
color case, sequence representation, or OTIO target schema representation must first make the
corresponding consumer contract fail, then receive
deliberate generator, fixture-version, documentation, and proof updates.

Keep this map, fixture policy, command usage, validator behavior, and tests synchronized. Extend
red contracts before changing schema, generation layouts, overwrite rules, errors, or output. After
owned source changes, refresh the exact inventory, hash, behavioral prose, consumer relationships,
and global fixture flow before delivery.
