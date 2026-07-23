---
module_id: tool-superi-ui-inspect
source_paths:
  - open/tools/superi-ui-inspect
source_hash: f893633f80ed9f5e39eb64e02b1686b92f0914b5b2c2c0410abd79988c91744b
source_files: 3
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-ui-inspect` is the private deterministic control and capture utility for the retained native
interface. It renders through the production scene and wgpu compositor without controlling the
user's desktop or introducing a browser-only presentation path.

## Source inventory

- `open/tools/superi-ui-inspect/Cargo.toml`: Declares the private CLI and retained UI dependencies.
- `open/tools/superi-ui-inspect/src/main.rs`: Implements render, inspect, click, key, type, crop, and compare commands.
- `open/tools/superi-ui-inspect/tests/cli_contract.rs`: Proves artifact generation, deterministic comparison, and interaction replay.

## Public surface

The command-line surface accepts explicit output and state paths and provides `render`, `inspect`,
`click`, `key`, `type`, `crop`, and `compare`. A repository-root wrapper at
`tools/superi-capture` exposes the binary consistently.

## Architecture and data flow

Each command loads or creates retained interaction state, builds the same neutral scaffold scene used
by the native host, applies normalized foundation-probe input when requested, and captures through
the shared wgpu renderer. The tool writes `surface.png`, `semantics.json`, `transcript.json`,
`manifest.json`, and `state.json`. Compare checks exact artifacts; crop derives a bounded PNG region
from a capture.

## Dependencies and consumers

The tool consumes `superi-ui` and its transitive managed GPU boundary. Repository checkpoint
verification and human visual review consume its artifacts. It has no production runtime consumer.

## Invariants and operational boundaries

- The tool uses the production retained scene and compositor.
- It never controls global pointer, keyboard, focus, or third-party applications.
- Every interaction is recorded in the transcript.
- Output paths are explicit and deterministic.
- Pixel inspection uses only the classified private readback boundary.

## Tests and verification

`cli_contract.rs` runs the compiled binary to verify complete artifacts, command discovery, and a
serialized and reloadable semantic-probe interaction. Checkpoint visual verification additionally
compares independent baseline and interaction renders and inspects the scaffold and handoff crops.

## Current status and risks

The tool supports the neutral Phase Infinity scaffold and is suitable for deterministic human
review without implying a completed product workspace. Later widget and workspace checkpoints must
expand its semantic queries and action vocabulary in lockstep with retained UI behavior.

## Maintenance notes

Keep commands, output schema, root wrapper behavior, retained state compatibility, and capture
manifest fields synchronized with `superi-ui`.
