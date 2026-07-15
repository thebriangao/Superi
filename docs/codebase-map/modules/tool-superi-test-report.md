---
module_id: tool-superi-test-report
source_paths:
  - open/tools/superi-test-report
source_hash: a9d53989ba087e0ab9837c158eba6556a873e2eaf727456000c2a6acac46e54c
source_files: 6
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-test-report` is an offline repository utility that converts explicit platform-lane evidence
into deterministic schema-versioned JSON. It owns strict evidence validation, canonical ordering,
derived lane status and summary counts, typed findings for performance regressions, golden
mismatches, flaky tests, and platform gaps, plus collision-safe report publication. It does not run
tests, probe hardware, upload artifacts, parse unstable test-harness output, or participate in the
runtime crate dependency graph.

## Source inventory

- `open/tools/superi-test-report/Cargo.toml`: Declares the workspace library and binary with only
  workspace Serde and Serde JSON dependencies.
- `open/tools/superi-test-report/README.md`: Documents schema 1 input, findings, exit statuses,
  offline use, retry retention, canonical output, and the artifact-link boundary.
- `open/tools/superi-test-report/src/lib.rs`: Implements strict deserialization, validation,
  canonicalization, finding and status derivation, canonical JSON, and no-overwrite publication.
- `open/tools/superi-test-report/src/main.rs`: Implements `build INPUT.json OUTPUT.json` and stable
  passing, blocking, and invalid-input exit behavior.
- `open/tools/superi-test-report/tests/fixtures/passing-lane.json`: Supplies one complete canonical
  passing lane for library and real CLI workflow proof.
- `open/tools/superi-test-report/tests/report_contract.rs`: Proves all finding categories,
  deterministic output, missing-suite and skip gaps, invalid evidence rejection, bounded thresholds,
  passing fixture behavior, blocking report publication, and no overwrite.

## Public surface

The library exports `generate_report`, `build_report_file`, `GeneratedReport`, and `ReportError`.
`generate_report` accepts schema 1 JSON bytes and returns validated derived evidence.
`GeneratedReport::canonical_json` emits pretty JSON with one trailing newline, and
`has_blocking_findings` reports whether a blocking lane contains a non-pass result or finding.
`build_report_file` reads input, derives the report, and publishes it only at an absent destination.

The binary accepts exactly:

```text
superi-test-report build INPUT.json OUTPUT.json
```

A passing report is written and exits 0. A valid blocking report is written before exit 1. Usage,
malformed input, validation failure, and file errors exit 2. Existing output is preserved.

## Architecture and data flow

A producer records the matrix revision, lane and suites, blocking policy, source and fixture
revisions, platform context, timing, raw artifact references, and contiguous attempts for every
test. Deserialization rejects unknown fields. Validation rejects noncanonical identities, malformed
digests, empty required context, duplicates, undeclared suites, incomplete attempts, missing
non-pass reasons, invalid golden counts, nonfinite measurements, and performance regression
fractions outside zero through one.

After validation, unordered collections are sorted. The generator derives final test counts from
last attempts, retains retries, marks disagreeing attempts flaky, evaluates explicit performance
direction and threshold, emits golden mismatches from comparison counts, preserves platform gaps,
converts unexplained skips to gaps, and creates a gap for each suite without test evidence. Findings
are ordered by test ID and category. A lane fails for failed tests, regressions, golden mismatches,
or flakiness; it reports gap when only gaps remain; otherwise it passes.

Publication writes and synchronizes a same-directory temporary file, creates the destination with a
hard link, then removes the temporary. This makes destination creation atomic and prevents a
check-then-replace race from overwriting existing evidence.

## Dependencies and consumers

Serde and Serde JSON are the only external dependencies. The standard library supplies ordered
sets, filesystem I/O, paths, and process exit codes. No Superi runtime crate is a dependency.

`docs/platform-testing.md` is the normative producer contract and directs repository and CI
consumers to this command. `open/docs/STRUCTURE.md` records the tool outside the runtime DAG. The
library contracts and CLI are current direct consumers. Future CI and physical lanes may produce
input and preserve artifacts, but this tool does not invent missing evidence or retain artifacts.

## Invariants and operational boundaries

- Operation is offline, deterministic, and independent of the closed product.
- Schema, source, fixture, platform, timing, attempt, reason, and artifact evidence is explicit.
- Caller-supplied status and summary fields do not exist; the tool derives them.
- Retries never erase attempts, skips never become passes, and missing suite evidence is a gap.
- Metrics and tolerances are finite and nonnegative. Regression fractions are at most one.
- Golden reports retain aggregates and artifact references rather than raw images or logs.
- Identical validated input produces byte-identical canonical JSON.
- Existing output paths are never replaced. Publication requires same-filesystem hard links.

## Tests and verification

The integration suite proves repeated deterministic generation, all four finding categories,
retained flaky attempts, derived counts, automatic missing-suite and unexplained-skip gaps,
attempt-number rejection, regression-fraction bounds, and one passing fixture. Its process contract
proves a blocking report exists before exit 1 and a second invocation exits 2 without changing it.

Focused package tests and strict package Clippy passed before the final synchronized map refresh.
Final workspace, all-feature, documentation, minimum-Rust, fixture, policy, map, and real CLI gates
remain delivery evidence and must be recorded in the checkpoint execution log after they run.

## Current status and risks

The strict library, CLI, example input, and focused contracts are implemented. The tool is a report
normalizer, not a test orchestrator or hardware attestation system. Producers can still provide
false facts inside syntactically valid evidence, artifact URIs can expire, timestamps are retained
as text rather than independently compared, and hard-link publication can fail on some filesystems.

Performance evidence supports one relative threshold and direction per record, not statistical
significance. Golden evidence trusts supplied aggregates and does not read raw diffs. The tool does
not know whether an expected capability list is complete beyond declared suites and records.

## Maintenance notes

Keep schema, validation, ordering, findings, README, fixture, CLI behavior, platform matrix, and
contracts synchronized. Add a failing contract before changing a field, status rule, threshold,
output order, or publication guarantee. Incompatible wire changes require a new schema version.

After owned source changes, rerun the mapping script's `files` and `hash` commands, read every
changed file through EOF, update prose and metadata together, and run the map validator. Update the
workspace and index whenever membership or repository-wide evidence flow changes.
