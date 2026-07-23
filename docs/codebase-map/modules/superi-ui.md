---
module_id: superi-ui
source_paths:
  - open/crates/superi-ui
source_hash: 15b3a66373043136cf4c4142c47a1b7577ae0e2daa0589636e3ace150a11ee2d
source_files: 13
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-ui` is the retained native presentation foundation. It owns scene identity, layout output,
deterministic paint order, Inter text shaping, original icon geometry, normalized input, focus,
semantics, the wgpu compositor, and private inspection artifacts. It does not own authored project
state, engine transactions, media decoding, playback policy, or the GPU submission queue.

One retained `Scene` is the source for pixels, hit testing, focus, and accessibility. This prevents
visual and semantic surfaces from drifting into independent trees.

## Source inventory

- `open/crates/superi-ui/Cargo.toml`: Declares the retained UI crate and its rendering, text, image, and accessibility dependencies.
- `open/crates/superi-ui/assets/INTER-LICENSE.txt`: Carries the bundled Inter font license.
- `open/crates/superi-ui/assets/InterVariable.ttf`: Bundles the exact offline Inter variable font used by deterministic painting.
- `open/crates/superi-ui/src/capture.rs`: Writes deterministic PNG, semantic tree, transcript, manifest, and hash artifacts.
- `open/crates/superi-ui/src/fixture.rs`: Builds the responsive scaffold diagnostic without composing a later product surface.
- `open/crates/superi-ui/src/icons.rs`: Owns the versioned neutral foundation icon registry and geometry validation.
- `open/crates/superi-ui/src/input.rs`: Normalizes pointer, keyboard, text, and focus input into foundation-probe state.
- `open/crates/superi-ui/src/lib.rs`: Defines crate ownership and exports the public retained UI modules.
- `open/crates/superi-ui/src/paint.rs`: Shapes Inter text and rasterizes scene nodes, icons, focus, and colors into exact RGBA.
- `open/crates/superi-ui/src/renderer.rs`: Encodes retained pixels through wgpu and the sole managed GPU submission owner.
- `open/crates/superi-ui/src/scene.rs`: Defines stable nodes, geometry, colors, semantics, validation, focus, and hit testing.
- `open/crates/superi-ui/src/semantics.rs`: Projects the retained scene into deterministic semantic and AccessKit trees.
- `open/crates/superi-ui/tests/foundation_contract.rs`: Proves deterministic painting, coupled semantics, input, GPU output, and captures.

## Public surface

The crate exposes retained scene values, the foundation fixture, normalized input and interaction
controllers, semantic projection, deterministic icon lookup, CPU painting, native and headless wgpu
encoding, and inspection capture records. `encode_scene_to_view` is the shared compositor entry used
by both the native host and private inspection tool.

## Architecture and data flow

The fixture produces a validated neutral capability ledger for an exact logical extent. It proves
the retained scene, normalized input, semantic projection, and private capture paths while labeling
all later product surfaces as deferred. Input resolves through scene hit testing and focus order,
then mutates only the selected foundation probe and text sample. Painting traverses the same scene
in stable order, shapes bundled Inter glyphs through Swash, and produces an RGBA image. The renderer
uploads that image with a staging buffer, records a wgpu pass, and submits it through
`superi-gpu::GpuSubmissionQueue`. Inspection uses the same path and an explicitly classified
inspection readback.

Semantics walk the same node order and derive both a serializable tree and one complete AccessKit
`TreeUpdate`. Deterministic AccessKit IDs map back to retained `NodeId` values so operating-system
actions reenter the same focus and activation controller.

## Dependencies and consumers

`superi-ui` depends on `superi-core` for classified errors and `superi-gpu` for device, surface,
submission, and readback ownership. It directly uses wgpu-compatible types, PNG encoding, Swash,
Serde, hashing, and AccessKit. `superi-desktop` is the production window consumer.
`tool-superi-ui-inspect` is the private deterministic capture and control consumer.

## Invariants and operational boundaries

- One retained scene supplies paint, hit testing, focus, and semantics.
- Node and icon identities are stable and duplicate identities fail validation.
- Bundled Inter bytes make text output independent of installed fonts.
- UI rendering never acquires or exposes a second raw GPU queue.
- Inspection readback is explicitly classified and is not a playback pixel path.
- Captures include content hashes, semantic output, and input transcripts.
- Layout adapts to the requested extent without browser or webview geometry.
- The C001 fixture contains no project, workspace, media, viewer, transport, timeline, inspector,
  graph, color, audio, or delivery composition.

## Tests and verification

`foundation_contract.rs` verifies scene validity, deterministic pixels, semantic parity, focus and
activation behavior, serializable interaction transcripts, AccessKit projection, neutral icon
validation, and the absence of later product node families. Strict all-target Clippy covers every
source and test target. The private inspector additionally renders the same scene twice and compares
exact PNG bytes.

## Current status and risks

The foundation is implemented and exercised through both the native host and headless inspection
path. Its only representative surface is a neutral scaffold diagnostic, not an editor workspace.
Future checkpoints own retained widgets, shell, docking, product surfaces, viewport texture
composition, text editing, internationalization, and platform accessibility without introducing a
parallel scene.

## Maintenance notes

Update this map whenever scene ownership, paint order, semantic projection, font or icon assets,
input routing, compositor submission, capture artifacts, or consumers change. Recompute the module
hash only after reconciling the inventory and all behavioral sections.
