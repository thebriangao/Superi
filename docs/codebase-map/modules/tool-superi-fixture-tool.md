---
module_id: tool-superi-fixture-tool
source_paths:
  - open/tools/superi-fixture-tool
source_hash: f252265deb57871adc65cffdcfeb8a4f36cba66793eba46e09c617d199c1d9d8
source_files: 8
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-fixture-tool` owns offline validation for shared canonical fixtures and deterministic
generation for the version 1 raw-video and synchronized multichannel audio baselines. It validates
layout, manifest schema, provenance, lineage, payload inventory, byte counts, and SHA-256 digests
without fetching data or executing documentary generator commands. Both generators create approved
synthetic bytes directly and never replace an existing output path.

The package is a repository utility, not a runtime crate. The canonical fixture store and policy
remain under `open/test-fixtures`; this tool creates a requested new video or audio version directory
and validates any fixture root, but it does not select versions for consumers or mutate released
data.

## Source inventory

- `open/tools/superi-fixture-tool/Cargo.toml`: Declares the workspace library and binary package and
  opts into workspace `serde`, `serde_json`, and `sha2` dependencies.
- `open/tools/superi-fixture-tool/src/lib.rs`: Implements strict fixture validation plus
  dependency-free video and audio baseline generators. The video path owns stable format and rate
  tables, exact plane geometry, finite sample synthesis, CSV serialization, and manifest creation.
  The audio path owns WAVEFORMATEXTENSIBLE serialization, common sample rates, speaker masks,
  synchronized integer waveforms, PCM interleaving, manifest creation, reports, and no-overwrite
  guards.
- `open/tools/superi-fixture-tool/src/main.rs`: Implements `check`, `generate-video`, and
  `generate-audio`, exact usage, summaries, diagnostics, and process exit statuses.
- `open/tools/superi-fixture-tool/tests/validator_contract.rs`: Exercises canonical and temporary
  fixture validation, drift, inventory, identity, provenance, path, and symlink failures.
- `open/tools/superi-fixture-tool/tests/video_cli_contract.rs`: Proves process-level generation,
  summary output, no-overwrite failure, exact usage, and exit statuses.
- `open/tools/superi-fixture-tool/tests/video_generator_contract.rs`: Regenerates all video artifacts
  into a temporary directory, compares them byte for byte with the canonical version, checks the
  207-case report and tiny payload bound, and proves preservation of an existing directory.
- `open/tools/superi-fixture-tool/tests/audio_cli_contract.rs`: Proves process-level audio
  generation, the three-case summary, no-overwrite failure, complete usage, and exit statuses.
- `open/tools/superi-fixture-tool/tests/audio_generator_contract.rs`: Regenerates all audio
  artifacts into a temporary directory, compares them byte for byte with the canonical version,
  checks the three-case report and payload bound, and proves preservation of an existing directory.

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

The executable accepts exactly these forms:

```text
superi-fixture-tool check [FIXTURE_ROOT]
superi-fixture-tool generate-video <OUTPUT_DIRECTORY>
superi-fixture-tool generate-audio <OUTPUT_DIRECTORY>
```

Validation defaults to `test-fixtures`, prints fixture and payload counts on success, and exits 1
for policy failure. Video generation prints `generated 207 video cases`, while audio generation
prints `generated 3 audio cases`; both exit 1 for filesystem or overwrite failure. Invalid command
shapes print the complete three-line usage and exit 2.

The accepted manifest format remains strict schema version 1. Its manifest, provenance, generator,
parent, and payload objects reject unknown fields. Generator records remain documentary for general
fixtures; the video and audio commands are separate executable implementations whose canonical byte
identities are proved by their integration tests.

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

## Dependencies and consumers

The standard library supplies filesystem, path, collection, formatting, byte, and process support.
`serde` and `serde_json` parse manifests, while `sha2` computes payload, catalog, and manifest
digests. No external media tool, platform encoder, network service, or random source participates in
generation.

The binary and five integration-test files consume the library. `open/test-fixtures/README.md`
documents all three commands. The canonical-root validator consumes the complete fixture store.
`superi-media-io` does not depend on this tool at runtime; separate integration tests consume the
emitted canonical video and audio artifacts. The video test checks generator tables indirectly
against live core definitions. The audio test opens every WAVE through the production PCM source and
checks exact sample clocks, masks, routing, synchronization, and continuity.

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
- Odd dimensions use ceiling division for chroma. Ten-bit planar values stay in 10 bits, P010 values
  occupy the high 10 bits of 16-bit containers, and floating-point samples are finite.
- The generator table is intentionally local to this repository tool. The media consumer contract
  fails if its format or rate matrix diverges from `superi-core`.
- Generator command and source fields in arbitrary manifests remain documentary and are never
  executed during validation.

## Tests and verification

Seven validator contracts cover the canonical root, success counts, content and inventory drift,
identity, versions, provenance, unsafe paths, and Unix symlinks. Four generator contracts prove the
video and audio artifacts reproduce byte for byte, report 207 and three cases respectively, stay
within their small payload bounds, and leave existing directories unchanged. Four CLI contracts
prove both generator summaries, no-overwrite diagnostics, complete usage, and exit statuses.

The separate `superi-media-io` contract validates the canonical catalog and payload through their
real consumer. It proves the full 23 by 9 matrix, exact rates and geometry, contiguous offsets,
per-plane hashes, numeric representation rules, and construction through public video frame types.
The separate audio contract proves all three common rates, exact WAVE channel masks and canonical
layouts, sample-aligned timing, synchronized signal boundaries, distinct channel routing, complete
sample identity, and bounded adjacent-sample continuity through `PcmContainerSource`. The canonical
validator reports three fixture versions and six payloads after this addition.

## Current status and risks

Validation, deterministic video and audio generation, all three CLI commands, canonical artifacts,
and real consumer proof are implemented. The generated video baseline is raw single-frame evidence,
not an encoded codec, sequence cadence, HDR, malformed-media, or editorial-slice proof. The audio
baseline proves container-visible timing, routing, synchronization, and continuity, not decoding,
resampling, playback, physical devices, hardware clocks, real-time behavior, or A/V sync.

Validation still checks SPDX, media type, source, author, rights, and semantic quality only to the
degree documented by schema rules. It validates a filesystem snapshot rather than history. Lineage
does not reject cycles or duplicate parents. Concurrent external mutation can still race validation,
and a write failure after creating a new generation directory can leave a partial new directory for
the caller to inspect and remove.

## Maintenance notes

Keep the generator tables, WAVE schema, waveform math, and serialization intentionally stable for
version 1. Add a new fixture version when bytes or schema change. Any new core pixel format,
standard video rate, canonical audio rate, or channel layout must first make the corresponding media
consumer contract fail, then receive deliberate generator, fixture-version, documentation, and
proof updates.

Keep this map, fixture policy, command usage, validator behavior, and tests synchronized. Extend
red contracts before changing schema, generation layouts, overwrite rules, errors, or output. After
owned source changes, refresh the exact inventory, hash, behavioral prose, consumer relationships,
and global fixture flow before delivery.
