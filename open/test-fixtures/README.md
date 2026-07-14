# Superi test fixture contract

This directory is the canonical home for fixtures shared across crates, golden tests, fuzzing,
benchmarks, and end-to-end workflows. The contract makes a test input independently identifiable,
reviewable, reproducible, and safe to redistribute. Crate-private executable helpers may remain
under a crate's `tests/fixtures` directory, but shared media or data must use this root.

## Immutable layout

Every fixture version has this layout:

```text
test-fixtures/<suite>/<fixture-id>/v<positive-integer>/
  fixture.json
  <payload files>
```

`fixture_id` is the path between `test-fixtures/` and the version directory, joined with `/`. It has
at least two lowercase components and uses only ASCII letters, digits, `.`, `_`, and `-`. The
manifest's `fixture_version` must match the `vN` directory.

A fixture version merged to the canonical branch is immutable. Do not replace a payload, rewrite
its manifest, or reuse its version. Correct or regenerate it in `vN+1`, update consumers explicitly,
and retain the old version while any compatibility or regression test needs it. Removing a version
requires proof that no code, historical compatibility test, benchmark baseline, fuzz seed, or
published report still references it.

## Manifest schema version 1

Each `fixture.json` contains exactly these top-level fields:

- `schema_version`: currently `1`.
- `fixture_id`: stable path-derived identity.
- `fixture_version`: positive content revision.
- `description`: what behavior and edge case the fixture represents.
- `provenance`: the origin, rights, reproduction, and lineage record described below.
- `files`: a complete payload inventory. Every entry records a normalized relative `path`, IANA
  `media_type`, byte count in `bytes`, and lowercase `sha256` digest.

Unknown fields are rejected. Payload paths cannot be absolute, contain `..`, use a backslash, name
the manifest, or resolve through a symlink. Every regular file in a version directory must appear
exactly once in `files`; listed files must exist and match both size and digest.

## Provenance

`provenance` contains exactly these fields:

- `kind`: `synthetic`, `generated`, `recorded`, `third_party`, or `derived`.
- `source`: a durable origin description. For third-party material, include the canonical source
  URL or publication identifier and acquisition context. Validation never fetches it.
- `author`: the person, organization, device, dataset, or generator responsible for the source.
- `created_on`: a real `YYYY-MM-DD` date for creation or acquisition.
- `license`: the SPDX identifier or expression governing redistribution and test use.
- `rights`: concrete evidence or rationale that Superi may store, modify, and redistribute it.
- `generator`: either `null` or an object with nonempty `name`, `version`, `command`, and `seed`.
- `parents`: an array of exact in-repository parent fixture references. Each reference contains
  `fixture_id`, `fixture_version`, and the parent's `manifest_sha256`.

Synthetic, generated, and derived fixtures require a generator record. Use an explicit value such
as `not-applicable` when a deterministic process has no random seed. The command is documentary and
must be sufficient to reproduce the payload from approved local inputs; the validator never runs
it. Derived fixtures require at least one parent, and each parent manifest digest must match a
fixture present in this root. Other provenance kinds cannot declare parents.

Do not commit credentials, personal data, user project data, undisclosed training data, unclear
copyright, nonredistributable samples, or content whose license conflicts with repository policy.
Record transformations as new derived versions rather than obscuring origin. Large binaries require
the same review and manifest; storage mechanism does not weaken this contract.

## Deterministic video baseline

`video/pixel-formats/v1` contains one 5 by 3 raw frame for every combination of the 23 pixel
formats in `superi-core::PixelFormat::ALL` and the nine standard `FrameRate` constants. The odd
dimensions exercise exact packed, planar, semiplanar, and chroma-subsampled geometry. Integer,
10-bit, P010, half-float, and float payloads use stable little-endian sample patterns. The complete
207-case binary payload remains below 64 KiB.

`video-cases.csv` uses a fixed 12-field, CRLF-delimited catalog. Each record binds one plane to its
case identity, exact rational frame rate, dimensions, plane index, payload offset, byte count,
stride, row count, and SHA-256. `video-frames.bin` stores catalog planes contiguously with no gaps.
The `superi-media-io` fixture contract proves the catalog is the complete Cartesian product of the
live core definitions, verifies every plane digest and representation, and constructs every frame
through the public media-I/O CPU buffer and timing path.

Reproduce the version into a new absent directory from `open/`:

```text
cargo run -p superi-fixture-tool -- generate-video <OUTPUT_DIRECTORY>
```

The generator refuses to overwrite any existing output path. Compare all three generated artifacts
byte for byte with the canonical version. Do not regenerate into the checked-in `v1` directory.

## Deterministic synchronized audio baseline

`audio/synchronized-multichannel/v1` contains three 100 ms WAVEFORMATEXTENSIBLE PCM16 files: 44,100
Hz stereo, 48,000 Hz 5.1, and 96,000 Hz 7.1. Their channel masks use canonical speaker order, and
their interleaved samples use distinct per-channel gains so a routing swap is observable. Every file
has a ten millisecond silent lead-in, an integer-only 1 kHz triangle signal through 90 ms, and a ten
millisecond silent tail. The shared onset and tail are exact sample boundaries at every rate, and the
zero-valued boundaries keep adjacent-sample changes bounded.

The `superi-media-io` fixture contract opens all three files through `PcmContainerSource`. It proves
their exact sample clocks, frame counts, WAVE channel masks, canonical channel layouts, complete
sample sequences, synchronized signal boundaries, channel-specific routing signatures, and audible
continuity bound.

Reproduce the version into a new absent directory from `open/`:

```text
cargo run -p superi-fixture-tool -- generate-audio <OUTPUT_DIRECTORY>
```

The generator refuses to overwrite any existing output path. Compare the manifest and all three
WAVE files byte for byte with the canonical version. Do not regenerate into the checked-in `v1`
directory.

## Contributor workflow

1. Prefer the smallest synthetic fixture that exposes the behavior. Use representative recorded or
   third-party material only when synthetic data cannot exercise the path.
2. Create a new stable fixture identity or the next version. Never modify a released version.
3. Generate or copy payloads with the network disconnected. Pin tool versions and record the exact
   command, seed, source, lineage, license, and rights evidence.
4. Inventory every payload with byte count and SHA-256. Keep expected outputs separate from inputs
   when their version lifecycles differ.
5. Point tests at the exact identity and version. Tests must not select `latest`, download missing
   data, overwrite fixtures, or accept regenerated output automatically.
6. Run `cargo run -p superi-fixture-tool -- check test-fixtures` from `open/`. Review manifest and
   payload changes as product code. A golden change needs an explanation of the intentional semantic
   change, not only updated hashes.

CI and local verification run entirely offline. A missing fixture, unsupported manifest schema,
unmanaged file, incomplete provenance, unsafe path, lineage mismatch, size drift, or digest drift is
a hard failure.
