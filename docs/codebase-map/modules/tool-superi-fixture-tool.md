---
module_id: tool-superi-fixture-tool
source_paths:
  - open/tools/superi-fixture-tool
source_hash: e743d443d362e0ee4a63312055b4e6bd83c976e9fa7afe87820060feca0104c8
source_files: 6
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-fixture-tool` owns offline validation for shared canonical fixtures and deterministic
generation for the version 1 raw-video baseline. It validates layout, manifest schema, provenance,
lineage, payload inventory, byte counts, and SHA-256 digests without fetching data or executing
documentary generator commands. Its video generator creates approved synthetic bytes directly and
never replaces an existing output path.

The package is a repository utility, not a runtime crate. The canonical fixture store and policy
remain under `open/test-fixtures`; this tool creates a requested new video version directory and
validates any fixture root, but it does not select versions for consumers or mutate released data.

## Source inventory

- `open/tools/superi-fixture-tool/Cargo.toml`: Declares the workspace library and binary package and
  opts into workspace `serde`, `serde_json`, and `sha2` dependencies.
- `open/tools/superi-fixture-tool/src/lib.rs`: Implements strict fixture validation plus the
  dependency-free video baseline generator, stable format and rate tables, exact plane geometry,
  finite sample synthesis, CSV serialization, manifest creation, reports, and no-overwrite guard.
- `open/tools/superi-fixture-tool/src/main.rs`: Implements `check` and `generate-video`, exact usage,
  summaries, diagnostics, and process exit statuses.
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

The executable accepts exactly these forms:

```text
superi-fixture-tool check [FIXTURE_ROOT]
superi-fixture-tool generate-video <OUTPUT_DIRECTORY>
```

Validation defaults to `test-fixtures`, prints fixture and payload counts on success, and exits 1
for policy failure. Generation prints `generated 207 video cases` and exits 1 for filesystem or
overwrite failure. Invalid command shapes print the complete two-line usage and exit 2.

The accepted manifest format remains strict schema version 1. Its manifest, provenance, generator,
parent, and payload objects reject unknown fields. Generator records remain documentary for general
fixtures; the video command is a separate executable implementation whose canonical byte identity
is proved by its integration test.

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

## Dependencies and consumers

The standard library supplies filesystem, path, collection, formatting, byte, and process support.
`serde` and `serde_json` parse manifests, while `sha2` computes payload, catalog, and manifest
digests. No external media tool, platform encoder, network service, or random source participates in
generation.

The binary and three integration-test files consume the library. `open/test-fixtures/README.md`
documents both commands. The canonical-root validator consumes the complete fixture store.
`superi-media-io` does not depend on this tool at runtime; its integration test consumes the emitted
canonical video artifacts and checks the generator tables indirectly against live core definitions.

## Invariants and operational boundaries

- Validation is offline and read-only. Generation is offline and writes only a newly created output
  directory.
- Fixture identity, version, provenance, paths, inventory, byte counts, and digests remain strict
  schema 1 contracts with deterministic error order.
- Released versions remain immutable by repository policy. The tool refuses overwrite but does not
  prove Git history or prevent a contributor from deleting a directory outside the tool.
- Video output is deterministic across supported hosts because sample values, byte order, geometry,
  catalog order, CRLF records, manifest text, and seed are fixed in Rust.
- Odd dimensions use ceiling division for chroma. Ten-bit planar values stay in 10 bits, P010 values
  occupy the high 10 bits of 16-bit containers, and floating-point samples are finite.
- The generator table is intentionally local to this repository tool. The media consumer contract
  fails if its format or rate matrix diverges from `superi-core`.
- Generator command and source fields in arbitrary manifests remain documentary and are never
  executed during validation.

## Tests and verification

Seven validator contracts cover the canonical root, success counts, content and inventory drift,
identity, versions, provenance, unsafe paths, and Unix symlinks. Two generator contracts prove all
three canonical artifacts reproduce byte for byte, 207 cases are reported, the payload stays below
64 KiB, and an existing directory remains unchanged. Two CLI contracts prove success output,
no-overwrite diagnostics, complete usage, and exit statuses.

The separate `superi-media-io` contract validates the canonical catalog and payload through their
real consumer. It proves the full 23 by 9 matrix, exact rates and geometry, contiguous offsets,
per-plane hashes, numeric representation rules, and construction through public video frame types.
The canonical validator reports two fixture versions and three payloads after this addition.

## Current status and risks

Validation, deterministic video generation, both CLI commands, canonical artifacts, and real
consumer proof are implemented. The generated video baseline is raw single-frame evidence, not an
encoded codec, sequence cadence, HDR, malformed-media, hardware, audio, or editorial-slice proof.

Validation still checks SPDX, media type, source, author, rights, and semantic quality only to the
degree documented by schema rules. It validates a filesystem snapshot rather than history. Lineage
does not reject cycles or duplicate parents. Concurrent external mutation can still race validation,
and a write failure after creating a new generation directory can leave a partial new directory for
the caller to inspect and remove.

## Maintenance notes

Keep the generator table and serialization intentionally stable for version 1. Add a new fixture
version when bytes or schema change. Any new core pixel format or standard rate must first make the
media consumer contract fail, then receive deliberate generator, fixture-version, documentation,
and proof updates.

Keep this map, fixture policy, command usage, validator behavior, and tests synchronized. Extend
red contracts before changing schema, generation layouts, overwrite rules, errors, or output. After
owned source changes, refresh the exact inventory, hash, behavioral prose, consumer relationships,
and global fixture flow before delivery.
