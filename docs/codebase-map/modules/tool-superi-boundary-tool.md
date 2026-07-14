---
module_id: tool-superi-boundary-tool
source_paths:
  - open/tools/superi-boundary-tool
source_hash: a9fa4d961d30ce77085c4000990adfd6be81374822c232d4553e9425e8be9bc2
source_files: 4
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-boundary-tool` owns the automated static boundary check for the open Rust tree. It rejects
known network client dependencies and direct socket APIs, supported Cargo or Rust routes into the
closed product, and symlinks that could escape the inspected tree. It is an offline repository
utility, not an engine runtime dependency.

## Source inventory

- `open/tools/superi-boundary-tool/Cargo.toml`: Declares a dependency-free workspace library and
  binary that inherit repository metadata and lint policy.
- `open/tools/superi-boundary-tool/src/lib.rs`: Implements deterministic traversal, Cargo dependency
  checks, Rust lexical scanning, stable violations, and reports without following symlinks.
- `open/tools/superi-boundary-tool/src/main.rs`: Implements `check [OPEN_TREE_ROOT]`, stable summary
  output, validation errors, usage handling, and process exit statuses.
- `open/tools/superi-boundary-tool/tests/scanner_contract.rs`: Proves the canonical open tree,
  workflow integration, dependency aliases, direct APIs, closed paths, inert prose, symlinks, and
  CLI success and failure behavior.

## Public surface

The library exports `scan_open_tree(&Path) -> Result<ScanReport, ScanError>`. A successful report
exposes scanned file and manifest counts. A violation exposes a stable code, repository-relative
path, one-based line, and explanatory message. Errors and violations implement display formatting
for deterministic contributor diagnostics.

The executable accepts exactly `superi-boundary-tool check [OPEN_TREE_ROOT]`. With no explicit root
it scans the current directory. Success prints `validated <N> files across <M> Cargo manifests` and
exits 0. Policy violations print one stable diagnostic per line and exit 1. Invalid arguments or a
filesystem scan failure print an error and exit 2.

## Architecture and data flow

The scanner recursively walks the supplied root using symlink-aware metadata, rejects every
symlink, skips generated or repository-control directories, sorts paths, and inspects Cargo
manifests plus Rust source and build scripts. Cargo parsing recognizes dependency, development,
build, and target tables, including renamed packages and package identities. It rejects the named
network-client set and path or package references whose components identify the closed product.

Rust scanning removes comments and inert string contents while preserving source line positions.
It detects standard-library and common runtime network namespaces, raw socket entry points, nested
imports, path attributes, and include macros whose paths cross into `closed`. Violations are sorted
by path, line, code, and message before reporting. The command runs before each locked
cross-platform workspace build, and the canonical-tree integration test includes the same scan in
`cargo test --workspace`.

## Dependencies and consumers

The package uses only the Rust standard library and inherits Rust 1.80 compatibility through the
workspace. The CLI directly consumes the library. The focused integration test is the other direct
consumer and reads `.github/workflows/ci.yml` to prevent hosted build jobs from drifting away from
the locked scanner command. No runtime crate depends on this tool.

`open/README.md` documents the policy and contributor command. `open/Cargo.lock` records the local
workspace package without adding an external dependency. GitHub Actions invokes the command from
`open/` before building the workspace.

## Invariants and operational boundaries

- Scanning is deterministic, offline, read-only, and dependency-free.
- Symlinks are rejected and never followed.
- Cargo aliases do not hide actual package identity from policy.
- Comments and inert string prose do not create policy violations.
- Executable Rust paths, build scripts, include macros, and path attributes remain in scope.
- Open code cannot reference the closed product through Cargo paths, package names, or Rust paths.
- Core open code has no network-client or direct-socket exception.
- Stable diagnostics retain code, path, line, and message for reproducible CI failures.

## Tests and verification

Eight integration contracts cover canonical-tree gating, exact cross-platform workflow wiring,
direct and renamed network dependencies, direct and nested network APIs, build scripts, closed
manifest paths and package identities, source includes and attributes, ignored comments and
strings, symlink rejection, and process behavior. Fresh Rust 1.80 formatting, focused tests,
warnings-denied focused Clippy, the canonical scan of 304 files and 23 manifests, workflow YAML
parsing, a locked full workspace build, and the complete workspace test and documentation suite
passed using `CARGO_TARGET_DIR=/private/tmp/superi-targets/P1.W07.C008`.

Full strict workspace Clippy was also executed. It reached seven pre-existing missing safety
comments in AV1, Opus, and VideoToolbox files outside this module; strict focused Clippy for this
tool passed. A sandboxed native codec run was denied host framework access, while the same contract
and complete workspace suite passed with native system-service access.

## Current status and risks

The scanner, CLI, workspace gate, and hosted-build invocation are implemented. The policy is a
static check over current Cargo and Rust surfaces. It does not provide runtime network isolation,
inspect future frontend or scripting languages, prove that arbitrary transitive implementation
code never uses a network internally, or define a future user-installed plugin capability model.

The named client and namespace policy must evolve when the workspace adds a new approved language,
dependency mechanism, or explicit plugin sandbox. Cargo and Rust syntax outside the covered forms
can require focused parser extensions. Runtime network isolation remains separately owned.

## Maintenance notes

Keep the finite client list, Cargo table handling, Rust lexical rules, stable codes, README policy,
workflow command, and contract fixtures synchronized. Add failing fixtures before expanding syntax
or exceptions. Any future exception must remain narrow, documented, and unable to create a core
network path or an open-to-closed dependency.

After source changes, recompute this module's source hash and file count, update its behavioral
claims from the raw files, update workspace or consumer maps when workflow integration changes, and
run the map validator immediately before delivery.
