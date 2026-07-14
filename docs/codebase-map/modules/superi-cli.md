---
module_id: superi-cli
source_paths:
  - open/crates/superi-cli
source_hash: 8a3d3c8c4a70a7e4a7f178efaa2135055b3530a2920a4a66d840be6689959f29
source_files: 5
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-cli` is the workspace's headless public API consumer and owns the normalized process
contract for `superi.slice.canonical.v1`. It validates the authoritative repository fixture,
executes canonical editorial actions through `superi-api`, proves exact reversal, writes the strict
eight-stage report, records bounded timing and process resident-memory evidence, and publishes a
clearly labeled non-playable contract artifact.

The current runner satisfies contract conformance only. It does not open or decode media, evaluate
pixels, apply production color, encode AV1, mux WebM, or claim a working editor export. Every absent
production owner is explicit in stage diagnostics and the artifact name.

## Source inventory

- `open/crates/superi-cli/Cargo.toml`: Declares `serde`, `serde_json`, `sha2`, `sysinfo`,
  `superi-core`, and `superi-api`, plus `os-codecs` forwarding to the API.
- `open/crates/superi-cli/src/commands.rs`: Implements exact argument parsing, repository and
  fixture resolution, bounded strict manifest validation, canonical API execution, stage and
  digest reporting, instrumentation integration, undo plus redo proof, collision-safe publication,
  and structured exit errors.
- `open/crates/superi-cli/src/instrumentation.rs`: Implements one reusable current-process sampler,
  monotonic stage probes, resident-set boundary records, and the report instrumentation summary.
- `open/crates/superi-cli/src/main.rs`: Passes process arguments to the private command owner and
  exits with its exact status.
- `open/crates/superi-cli/tests/scenario_runner.rs`: Provides process contracts for two-run
  reproducibility, exact state and schema 1.1.0 report contents, all-stage timing and nonzero
  resident-memory evidence, honest stub evidence, collision preservation, help, version, usage,
  and status 2 invalid input.

## Public surface

This crate produces a binary, not a library. Its normalized scenario invocation is:

```text
superi-cli slice run --scenario superi.slice.canonical.v1 \
  --artifact-dir <EMPTY_DIRECTORY> --report <REPORT_JSON>
```

The artifact directory may be absent or empty, must not be a symlink, and receives
`canonical.webm.contract-stub`. The report path must not exist. Both files use create-only temporary
publication followed by a hard link, so an existing destination is never replaced.

No arguments and `--help` print usage and succeed. `--version` prints `superi 0.0.0`. Invalid input
returns 2, unavailable required capability returns 3, and stage or verification failure returns 4.
Errors are one strict stderr JSON object with category, recoverability, message, and stage ID when a
stage owns the failure. Success prints one stdout JSON summary after both artifact and report exist.

## Architecture and data flow

The runner walks working-directory ancestors to locate the Superi repository. It records Git commit
and dirty state plus Rust toolchain, build target, features, and profile. It then reads the strict
schema 1 manifest for `slice/video-cfr` version 1 with a one MiB bound, rejects symlinks and unknown
fields, validates required provenance, verifies the exact regular payload's 64 MiB bound, byte
count, and SHA-256, and only then creates the artifact directory.

Execution uses `ScenarioApi` exclusively:

```text
fixture.resolve
  -> media.import
  -> timeline.edit
  -> timeline.compile
  -> graph.evaluate
  -> color.deliver
  -> media.export
  -> slice.verify
```

The API receives exact import, placement, trim, and mirror actions. Timeline compilation, pixel
evaluation, color delivery, and media export remain contract stubs. The runner undoes effect and
trim, redoes both, removes only the monotonic revision from comparison, and requires exact final
semantic state recovery without reimport.

One `ProcessMemorySampler` resolves the CLI process ID once and refreshes only that process with
memory enabled and task enumeration disabled. Each stage takes one resident-set sample immediately
before its work and one immediately after, for 16 bounded refreshes in a complete run. The same
probe measures monotonic elapsed microseconds. An unavailable or zero resident-memory sample is an
explicit stage failure, not a fabricated value or omitted field.

The contract artifact is deterministic JSON with `playable: false`, six missing runtime owners,
and the planned WebM, AV1, 96 by 54, 24 fps, 48-frame target. It is not named `canonical.webm`.
Report schema 1.1.0 retains repository and fixture identities, state digests, full public state,
eight stage records, backend expectations, target metadata, artifact identity, 48 modeled
timestamps, unavailable expected-output status, and all stub diagnostics. Every stage retains its
existing `duration_us` and adds resident bytes before and after. The report summary declares the
clock, units, memory metric, boundary sampling, stage count, and maximum resident value observed
across those boundaries. Contract success never becomes runtime success.

## Dependencies and consumers

- `superi-api` supplies the only editorial control boundary used by the runner.
- `serde` and `serde_json` parse strict manifests and serialize state, stages, reports, artifacts,
  summaries, and failures.
- `sha2` computes manifest, payload, semantic state, timeline, graph, operation log, and artifact
  identities.
- `sysinfo` 0.36.1 uses only its `system` feature to refresh resident memory for the current
  process. Default component, disk, network, and user collectors are disabled.
- `superi-core` remains a declared dependency from the original crate topology but is not directly
  imported by current CLI source.
- `open/ci/run-network-isolated.sh` invokes the exact canonical command with temporary output paths
  after workspace tests and fixture validation inside the isolated namespace.
- Root and open-tree READMEs document the command and contract-only result.

No runtime crate consumes this binary. The process contracts, contributor workflow, and isolated CI
harness are its current consumers.

## Invariants and operational boundaries

- The only accepted scenario ID is `superi.slice.canonical.v1` at revision 1.
- Repository fixture bytes are input. The runner never downloads, modifies, regenerates, or accepts
  an arbitrary source path.
- Source and manifest reads are bounded. Fixture identity, inventory, path type, size, and digest
  must pass before editorial state or output is created.
- Output paths are create-only and collision safe. Existing content and symlinks are preserved and
  rejected.
- Export is outside engine mutation history. The four mutation records remain import, insert, trim,
  and effect.
- Contract stubs are never called runtime, and the non-playable artifact is never called WebM
  output.
- Stage order, implementation identity, input and output summaries, diagnostics, state, and artifact
  bytes are deterministic. Durations, resident-memory samples, the observed boundary maximum, and
  chosen output paths are run-specific evidence.
- Instrumentation performs exactly two current-process memory refreshes per stage. It does not scan
  unrelated processes, spawn a sampling thread, retain an unbounded trace, or claim an intra-stage
  memory peak.
- The runner initiates no network operation and executes with default features in the isolated CI
  path.

## Tests and verification

The process contract runs the complete command twice with separate output locations. It proves the
strict report schema and scenario identity, authoritative fixture details, exact eight-stage order,
stub and runtime classifications, canonical timeline, mirror matrix, four-operation log, undo plus
redo recovery, unavailable expected output, non-playable artifact, target stream shape, 48 modeled
timestamps, identical stub bytes, schema 1.1.0 instrumentation metadata, all-stage duration values,
two nonzero resident samples per stage, and an exact summary maximum. It requires report equality
after removing only durations, resident values, the observed boundary maximum, and output paths.

Negative process contracts prove unknown scenario rejection, preservation of a nonempty artifact
directory, preservation of an existing report, exact status 2, and help, version, and usage output.
The focused test does not prove Linux namespace isolation, production media behavior, real output
decoding, or expected pixel comparison. Those remain widening or future-owner evidence.

## Current status and risks

The CLI is now a substantive API consumer and canonical contract runner. Its strongest limitation
is intentional: six stages model typed boundaries without production execution. The fixture payload
is digest-validated but its decoded traits are reported as expected contract values because the
current media stage does not open it.

Boundary samples do not continuously observe allocations inside a stage and are not a peak-memory,
constrained-device, or long-session soak result. They provide a portable stage-local signal for the
continuously working slice while those wider performance suites remain separate owners.

There is no independent expected-output fixture, so the report records expectation status as
unavailable. The runner uses local `git` and `rustc` commands for reproducibility identity and uses
hard links for atomic create-only publication, which assumes a normal contributor filesystem with
hard-link support inside each destination directory.

## Maintenance notes

Keep argument order, scenario identity, exit statuses, artifact name, report fields, stage IDs, and
stub disclosure synchronized with `docs/vertical-slice.md`, process contracts, isolated CI, and
public guidance. Keep stage probes around each stage when its stub is replaced so the fixed
instrumentation contract is inherited by the production owner. When a production owner replaces a
stub, route through that real subsystem, add consumer proof, update implementation identity and
diagnostics, and raise conformance only after all runtime gates pass. Never rename a contract stub
to `canonical.webm` merely to satisfy a filename.
