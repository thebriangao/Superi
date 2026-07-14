---
module_id: tool-superi-fixture-tool
source_paths:
  - open/tools/superi-fixture-tool
source_hash: 9e1e4eea19eca4b9a9a3d315c20fd70f2298557e65b6f863df0b570ebdd12879
source_files: 4
mapped_at_commit: 217e9d48703bcfd4736d949aea510c94505071bc
---

## Purpose and ownership

`superi-fixture-tool` owns the offline validation boundary for Superi's shared canonical test fixtures. It checks fixture layout, manifest schema, provenance metadata, parent lineage, payload inventory, file types, byte counts, and SHA-256 content digests without fetching data or executing generator commands.

The module is a repository utility, not a runtime engine crate. It owns a reusable validation library and the `superi-fixture-tool` binary. It does not own the canonical fixture store or policy, which live under `open/test-fixtures`, and it does not generate, copy, update, or delete fixtures despite the package name.

## Source inventory

- `open/tools/superi-fixture-tool/Cargo.toml`: Declares the workspace library and binary package, inherits repository package metadata and lint policy, and opts into workspace `serde`, `serde_json`, and `sha2` dependencies.
- `open/tools/superi-fixture-tool/src/lib.rs`: Implements manifest deserialization, recursive fixture discovery, identity and provenance checks, payload hashing, lineage resolution, unmanaged-file detection, deterministic error ordering, public reports, and structured validation errors.
- `open/tools/superi-fixture-tool/src/main.rs`: Implements the `check [FIXTURE_ROOT]` command, default root selection, summary output, error rendering, usage handling, and process exit statuses.
- `open/tools/superi-fixture-tool/tests/validator_contract.rs`: Exercises the public validator against the canonical fixture root and temporary valid, drifted, unlisted, misidentified, incomplete, unsafe-path, and symlinked fixtures.

## Public surface

The library exports `validate_root(&Path) -> Result<ValidationReport, ValidationErrors>`. Validation is read-only. A successful `ValidationReport` exposes copied counts through `fixture_count()` and `payload_count()`. `ValidationErrors` exposes borrowed entries through `iter()`, implements `Display` and `std::error::Error`, and owns one or more `ValidationError` values. Each error exposes a stable string `code()`, a filesystem `path()`, and a human-readable `message()`; the fields and backing collections remain private.

The executable accepts exactly `superi-fixture-tool check [FIXTURE_ROOT]`. With no explicit root it uses the relative path `test-fixtures`, which is intended for invocation from `open/`. Success prints `validated <N> fixture versions and <M> payloads` to stdout and exits 0. Validation failure prints one line per error as `<code>: <path>: <message>` to stderr and exits 1. A missing or incorrect command, or more than one root argument, prints `usage: superi-fixture-tool check [FIXTURE_ROOT]` to stderr and exits 2.

The accepted JSON format is schema version 1. `Manifest`, `Provenance`, `Generator`, `Parent`, and `Payload` reject unknown fields. A manifest contains `schema_version`, `fixture_id`, `fixture_version`, `description`, `provenance`, and `files`. Provenance kinds deserialize from `synthetic`, `generated`, `recorded`, `third_party`, or `derived`. Generator records contain `name`, `version`, `command`, and `seed`; parent references contain `fixture_id`, `fixture_version`, and `manifest_sha256`; payload records contain `path`, `media_type`, `bytes`, and `sha256`. Because `generator` deserializes as `Option<Generator>`, its field may be omitted or set to `null`; later validation rejects the absent value only for synthetic, generated, and derived fixtures.

## Architecture and data flow

The CLI parses the command and optional root, then delegates all behavior to `validate_root`. The validator first rejects a non-directory root, recursively walks the root with symlink-aware metadata, records regular files and symlinks, and gathers regular files named `fixture.json`. It sorts manifest paths before parsing so traversal order does not determine fixture processing order.

Each manifest is read as bytes, parsed with strict Serde structures, and hashed exactly as stored. Validation checks schema version 1, a positive `fixture_version` matching its `vN` directory, and a lowercase path-derived `fixture_id` with at least two components. It also requires a nonempty description and payload list, validates required provenance text and calendar-date shape, and applies provenance-kind rules. Synthetic, generated, and derived fixtures require complete generator details. Derived fixtures require parents, while all other kinds prohibit parents.

Parsed fixtures enter an identity index keyed by `(fixture_id, fixture_version)`. The payload pass requires safe normalized relative paths, unique manifest entries, nonempty media types, lowercase 64-character SHA-256 strings, regular non-symlink files, exact byte counts, and exact streamed content digests. The validator then resolves every declared parent against the in-memory identity index and compares the declared parent digest with the SHA-256 of the referenced manifest's exact bytes.

Finally, the validator compares every discovered file with the handled manifest and payload set. The root policy file `README.md` is the sole explicit exception. An extra file inside any parsed version directory is `payload.unlisted`; any other file below the root that is not part of a parsed version is `fixture.unmanaged`. All collected errors are sorted by path, then code, then message. Counts are returned only when the complete pass has no errors.

Fixture creation remains an external contributor flow. A contributor creates or copies a payload offline, records the pinned command, seed, source, rights, license, and optional parent lineage in `fixture.json`, computes size and digest fields, and invokes this tool as the final check. Generator commands and source strings are documentary inputs: this module never executes a command, regenerates a payload, contacts a source, or verifies reproduction from approved inputs.

## Dependencies and consumers

- The Rust standard library provides recursive filesystem access, symlink metadata, path normalization, buffered file reads, collections, formatting, and process exit codes.
- `serde` derives the strict manifest data model, and `serde_json` deserializes manifest bytes.
- `sha2` computes payload and raw-manifest SHA-256 digests. Payloads are streamed in 64 KiB chunks, while manifests are already held in memory for JSON parsing.
- The package inherits workspace metadata, Rust 1.80 compatibility, and lint settings through `open/Cargo.toml`. The workspace's `tools/*` membership includes it in normal workspace build, test, Clippy, and minimum-Rust checks.
- The binary is the direct consumer of the library. `validator_contract.rs` is the only in-repository Rust caller of `validate_root` outside that binary.
- `open/test-fixtures/README.md` documents the policy this module enforces and instructs contributors to run `cargo run -p superi-fixture-tool -- check test-fixtures` from `open/`. `open/docs/STRUCTURE.md` places the utility outside the runtime crate DAG.
- The integration test consumes the whole canonical `open/test-fixtures` tree and therefore makes its validity and nonempty inventory part of the package's test gate. No engine, API, CLI, media, or codec runtime crate depends on this utility, and the currently stored `policy/utf8/v1` fixture has no consumer beyond the validator contract test.

## Invariants and operational boundaries

- Validation is offline and read-only. There is no network client, subprocess execution, fixture mutation, or automatic acceptance of regenerated output.
- A fixture identity is its lowercase repository-relative path above `vN`; it must contain at least two slash-separated components, and component edges must be lowercase ASCII letters or digits. Interior `.`, `_`, and `-` characters are allowed.
- Fixture versions are positive and must match the version directory exactly. Duplicate parsed `(fixture_id, fixture_version)` identities are rejected.
- Schema objects reject unknown fields. Required strings are trimmed only for emptiness checks; stored values are otherwise preserved and not normalized.
- Dates must be ten ASCII characters in `YYYY-MM-DD` form and have a month-appropriate day, including Gregorian leap-year handling.
- Payload paths must be nonempty UTF-8 manifest strings composed only of normalized relative `Normal` path components. Absolute paths, backslashes, `.` or `..`, repeated separators, trailing separators, and the manifest name are rejected.
- Manifests and payloads must be regular files. Payload symlinks are rejected, directory symlinks are not traversed, and other special filesystem types produce errors.
- Every listed payload is unique, exists beneath its version directory, and matches the declared byte length and lowercase SHA-256 digest. Every discovered regular file or symlink except the root `README.md` must be owned by a parsed fixture.
- Lineage is local to one validation root. Every parent identity and version must exist in that pass, and its declaration must pin the exact raw manifest digest.
- `BTreeMap` and `BTreeSet` make identity, duplicate, and handled-file bookkeeping ordered. Sorting manifests and final errors makes results stable for an unchanged filesystem, apart from platform-specific path rendering and I/O messages.
- Any validation error suppresses the success report. The tool does not return a partial inventory alongside errors.

The validator enforces the present filesystem snapshot, not repository history. The documented rule that a version merged to the canonical branch is immutable still depends on review and version-control comparison; changing both a payload and its manifest digest can pass this validator as a self-consistent snapshot.

## Tests and verification

`validator_contract.rs` owns seven integration tests. The canonical-root test validates `open/test-fixtures` and requires at least one fixture version and one payload. Temporary-root tests prove successful counting, size drift rejection, unlisted-file rejection, path-derived identity and directory-version checks, nonempty license enforcement, derived-parent requirements, rejection of payload path traversal and repeated separators, and Unix payload-symlink rejection.

Fresh verification at mapped commit `217e9d48703bcfd4736d949aea510c94505071bc` passed `cargo test -p superi-fixture-tool`: all seven integration tests, both empty unit-test targets, and doc tests succeeded. `cargo run -p superi-fixture-tool -- check test-fixtures` also succeeded and reported one fixture version and one payload.

There are no direct CLI contract tests for argument cardinality, default-root behavior, stdout and stderr formatting, or exit codes. Tests also do not directly cover a missing or empty root, malformed or unknown-field JSON, unsupported schemas, all provenance kinds and generator fields, calendar edge cases, duplicate identities or payloads, media types, same-length digest drift, parent existence and hash matching, unmanaged files outside versions, manifest symlinks, special files, read failures, or final error ordering.

## Current status and risks

The validator and CLI are implemented and exercised against a minimal canonical synthetic fixture. The package name can suggest fixture generation, but no generation API or command exists. The supported command is validation-only.

Several policy statements are deliberately documentary rather than machine-proven. `license` and `media_type` need only be nonempty, so SPDX and IANA correctness are not parsed. `source`, `author`, and `rights` are checked only for nonempty text. Recorded and third-party manifests may omit the policy's nominally present `generator` field because the Serde field is optional and those kinds do not require a generator record. The tool does not inspect credentials, personal data, copyright status, redistribution compatibility, semantic fixture quality, generator reproducibility, or whether a golden-output change is intentional.

Lineage proves local presence and exact parent-manifest bytes, but it does not reject duplicate parent entries, cycles, self-reference, references to newer versions, or semantically inappropriate parents. The root `README.md` is exempt when present but is not required or validated. Empty directories are not inventoried. Validation is not a filesystem snapshot, so concurrent mutation can produce race-dependent metadata or read errors.

The walker rejects a manifest symlink as unmanaged rather than normally reaching the dedicated `manifest.symlink` branch, because symlinks are not added to the manifest list. The outcome is still rejection, but that specific error code is effectively defensive against a file changing into a symlink between discovery and parsing. Some malformed manifests can produce secondary unmanaged-file errors because only successfully parsed manifests establish version-directory ownership.

## Maintenance notes

Keep this map, `open/test-fixtures/README.md`, and validator behavior synchronized whenever the manifest schema, provenance rules, path policy, lineage semantics, output format, error codes, or command surface changes. Treat new subcommands as observable CLI contracts and add process-level tests for their arguments, streams, and exit statuses.

If fixture generation is added, map the trust boundary separately from validation: approved inputs, offline enforcement, tool and seed pinning, deterministic reproduction, overwrite protection, provenance emission, and review workflow all need explicit ownership and end-to-end proof. Do not describe documentary generator metadata as executed provenance until the module actually runs and verifies it.

When immutability must become machine-enforced, add a comparison against an explicit trusted baseline rather than weakening snapshot validation. Expand lineage tests before adding real derived fixtures, and preserve stable error ordering so CI failures remain reviewable. Recompute the module inventory and source hash after any owned-file change, even when only tests or package metadata change.
