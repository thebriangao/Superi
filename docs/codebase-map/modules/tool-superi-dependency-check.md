---
module_id: tool-superi-dependency-check
source_paths:
  - open/tools/superi-dependency-check
source_hash: acff2f3a226af5eceb9083f7812c71d175a002032a42666172e0b877b20411f1
source_files: 4
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-dependency-check` owns the executable open-workspace dependency-direction policy. It is a
repository utility, not a runtime crate, and verifies that the runtime Cargo graph remains within
the reviewed architecture documented in `open/docs/STRUCTURE.md`.

## Source inventory

- `open/tools/superi-dependency-check/Cargo.toml`: Declares the workspace package, Serde dependencies,
  shared lint policy, library target, and command target.
- `open/tools/superi-dependency-check/src/lib.rs`: Runs locked offline Cargo metadata, parses the
  workspace package graph, applies exact runtime and dev policies including cache consumption of
  concurrency, project consumption of authored audio, plus the API contract's test-only
  EngineControl edge, and reports deterministic errors.
- `open/tools/superi-dependency-check/src/main.rs`: Runs the library against the containing workspace,
  prints a successful package and edge summary, and returns a failing process status on violations.
- `open/tools/superi-dependency-check/tests/dependency_direction_contract.rs`: Covers the checked-in
  workspace, forbidden runtime and build edges, the reviewed project-to-audio edge and forbidden
  reverse edge, both reviewed API test edges, separation of dev and production policy, and
  fail-closed behavior for new runtime crates.

## Public surface

The library exports `check_workspace`, `validate_metadata`, `CheckReport`, and `CheckError`.
`check_workspace` accepts a workspace path and executes Cargo metadata with `--no-deps`, `--locked`,
and `--offline`. `validate_metadata` accepts Cargo metadata format 1 JSON, which provides a focused
contract-test seam without invoking Cargo. The binary takes no arguments and reports either the
number of checked runtime crates and internal edges or the complete ordered violation list.

## Architecture and data flow

The checker asks Cargo for the live workspace packages without resolving dependency source graphs.
It classifies packages whose manifest path contains `crates` as runtime crates, then selects an
explicit policy for each runtime package. Internal path dependencies use the normal and build
allowlist unless Cargo marks them as dev-only. Dev dependencies use a separate allowlist, so a test
relationship cannot authorize the same production edge. Unknown runtime crates and every
unapproved edge fail closed.

The API dev policy reviews `superi-media-io` for registry contracts and `superi-concurrency` only
for entering EngineControl around a real dispatcher introspection contract. Both crates remain
unauthorized as direct production API dependencies.

The project runtime policy reviews the downward edge to `superi-audio` for authored clip-mix state
and its canonical codec. The reverse audio-to-project edge remains forbidden, keeping prepared DSP,
devices, and callback ownership below project policy and persistence.

Violations are collected in a `BTreeSet`, making diagnostics stable for identical metadata. A clean
graph returns counts, and the command prints the summary for contributor and CI use.

## Dependencies and consumers

The implementation uses the Rust standard library for process execution, paths, collections, and
errors. Serde and Serde JSON parse Cargo metadata. It has no dependency on any Superi runtime crate.
The binary and contract tests consume the library. Cargo workspace tests discover its live-workspace
contract automatically because `open/Cargo.toml` includes `tools/*` members. Contributors consume
the direct command documented by `open/docs/STRUCTURE.md`.

## Invariants and operational boundaries

- Metadata collection is locked and offline and cannot update the dependency resolution.
- Runtime crates require an explicit policy entry even when they currently have no internal edges.
- Normal and build dependencies share the production policy; dev dependencies never widen it.
- The reviewed API media and EngineControl dev edges remain separately rejected when synthetic
  metadata presents either edge as normal or build scope.
- The reviewed project-to-audio production edge is accepted, while synthetic metadata proves the
  reverse edge remains rejected.
- Only internal path dependencies between workspace packages are checked. Registry dependency
  licensing and source policy remain owned by cargo-deny and its workflow.
- Tools are outside the runtime crate DAG and are not treated as runtime policy subjects.
- Policy changes must update the executable table and the human-readable structure document
  together through architecture review.

## Tests and verification

Five integration contracts exercise the current workspace and synthetic metadata failures. Fresh
checkpoint proof passed the focused package and documentation tests plus the direct locked command.
The direct command validated 19 runtime crates and 67 internal edges, including the reviewed
`superi-cache` to `superi-concurrency` production edge for bounded background rendering and the
test-only API to concurrency EngineControl edge. Synthetic metadata proves both reviewed API dev
edges remain forbidden in production and the authored-audio edge cannot reverse direction.

## Current status and risks

The checker covers every current runtime crate and exact internal normal, build, and reviewed dev
edges. Its policy is intentionally explicit rather than inferred from tier names. This makes drift
visible but requires a deliberate policy and documentation update whenever architecture changes.
The path-based runtime classification assumes repository runtime crates remain under `open/crates`.
It does not replace license checks, source checks, network-client scans, or the open-to-closed source
boundary scan.

## Maintenance notes

When a runtime crate or internal dependency changes, update `policy_for`, its negative and live graph
contracts, and `open/docs/STRUCTURE.md` in the same review. Preserve the separate dev policy and
fail-closed unknown-crate behavior. Recompute this map hash and file count after every owned source,
test, or manifest change, and rerun the direct locked command plus focused contracts.
