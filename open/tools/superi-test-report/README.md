# Superi structured test reports

`superi-test-report` converts explicit lane evidence into deterministic, schema-versioned JSON. It
runs offline, belongs to repository tooling, and never participates in the Superi runtime crate DAG.
The tool derives status, summary counts, and findings so a producer cannot submit a contradictory
pass summary.

## Build a report

From `open/`:

```bash
cargo run -p superi-test-report --locked --offline -- \
  build path/to/lane-input.json path/to/lane-report.json
```

The destination must not exist. A valid passing report returns exit code `0`. A valid report with
blocking failures, regressions, flaky outcomes, or gaps is written first and then returns exit code
`1`, preserving the evidence needed to diagnose the gate. Usage, malformed input, invalid evidence,
and file errors return exit code `2` and do not replace an existing report.

## Input contract

Schema version `1` requires the complete result context defined by
[`docs/platform-testing.md`](../../../docs/platform-testing.md):

- matrix revision, lane ID, declared suite IDs, and whether the lane is blocking;
- commit SHA, dirty state, build profile, Rust version, Cargo lockfile digest, and enabled features;
- fixture manifest and expected-output revisions plus reference project and media IDs;
- exact operating-system, CPU, memory, hardware-tier, optional GPU, optional audio, codec-backend,
  and cache evidence;
- explicit run timing and durable artifact identifiers or links;
- one or more contiguous attempts for every test, including command, status, duration, random seed,
  exact non-pass reason, and attempt artifacts;
- optional typed performance, golden-comparison, and platform-gap evidence attached to a test.

Identifiers use stable lowercase codes. Commit and lockfile digests use lowercase hexadecimal.
Unknown fields, duplicate identifiers, non-finite or negative measurements, noncontiguous attempts,
undeclared suites, and incomplete non-pass reasons are rejected.

Performance evidence states the metric, unit, baseline, observation, allowed regression fraction,
and whether lower or higher is better. The report retains the computed failure threshold and delta.
Golden evidence retains expected and actual revisions, tolerance, aggregate mismatch counts, maximum
error, first mismatch, and links to bounded raw diffs. Large logs, images, and dumps remain workflow
artifacts and are referenced by stable IDs instead of being embedded.

## Derived findings and status

The generator produces these finding categories:

- `performance_regression` when the observation crosses its explicit baseline threshold;
- `golden_mismatch` when samples exceed the supplied comparison tolerance;
- `flaky_test` when retained attempts disagree with the final attempt;
- `platform_gap` for explicit capability gaps, explained skips, and every declared suite that
  produced no test evidence.

Retries never erase prior attempts. A final pass can therefore remain a flaky failure on a blocking
lane. A skipped test remains a platform gap, even when the skip has a valid capability reason. A
missing or unimplemented suite is also a gap and can never count as planned coverage or mocked
success.

Arrays whose order has no execution meaning are canonicalized, tests are ordered by suite and ID,
and findings are ordered by test ID and category. Identical validated input therefore produces
byte-identical pretty JSON with one trailing newline. Producers must retain the referenced raw
artifacts according to their CI policy because hosted artifact links can expire independently of the
report.
