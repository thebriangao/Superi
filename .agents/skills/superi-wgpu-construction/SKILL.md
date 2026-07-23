---
name: superi-wgpu-construction
description: Build Superi retained-scene, text, input, semantics, native host, and wgpu rendering changes without violating canonical ownership.
---

# Superi wgpu Construction

## Ownership gate

Before editing:

1. Read the live checkpoint, plans, `superi-ui`, `superi-gpu`, `superi-concurrency`, `superi-api`,
   native host, relevant canonical owners, tests, and maps through EOF.
2. Identify the authored owner, immutable snapshot route, transaction route, event route, viewport
   resource route, and GPU submission route.
3. Stop and redesign if the planned UI stores a shadow project, timeline, graph, playback, export,
   media, color, audio, or effect model.

## Retained scene

- Use stable node identity independent of draw order.
- Derive draw, hit, focus, and semantic output from the same scene.
- Keep scene snapshots immutable during rendering.
- Keep layout deterministic for identical inputs.
- Make clips, transforms, z-order, and hit regions explicit.
- Treat semantic-only nodes as first-class retained nodes.

## Rendering

- Reuse `superi-gpu` device, resource, surface, fence, and recovery owners.
- Submit only through `GpuSubmissionQueue`.
- Preserve explicit target color format, alpha, scale, clip, and compositing behavior.
- Batch without changing deterministic order.
- Rebuild pipelines, buffers, atlases, and views after device loss from retained CPU state.
- Keep viewer pixels GPU-backed and outside public JSON.
- Use the real renderer for headless output.

## Text and icons

- Shape before layout.
- Keep font, variation, size, glyph, subpixel bucket, render mode, and scale in cache identity.
- Preserve bidi order, Unicode line opportunities, fallback, exact metrics, and tabular numbers.
- Separate text editing state from text drawing.
- Resolve icons by stable registry identity, never atlas coordinates.

## Input, focus, and semantics

- Normalize platform input before dispatch.
- Use capture, target, and bubble ordering.
- Make pointer capture acquisition and release explicit.
- Separate physical shortcuts from logical text commands.
- Keep focus separate from selection.
- Trap and restore focus for modal scopes.
- Emit stable semantic roles, names, values, bounds, states, actions, and relationships.
- Never block the UI event loop.

## Failure paths

Exercise:

- surface loss
- device loss
- zero-sized or hidden windows
- scale and display changes
- unavailable font or icon
- invalid scene references
- queue backpressure
- event cursor gaps
- stale revision or permission denial
- inaccessible or unsupported platform adapter

Degradation must remain visible and must not mutate authored data.

## Proof-first slices

Write a failing proof for each material slice:

- scene and layout
- draw output
- text or icon atlas
- hit testing and pointer capture
- focus and semantics
- transaction bridge
- device reconstruction
- native presentation
- private capture

Run focused proof after every slice. Capture when pixels or interaction become inspectable.

## Exit gate

The change passes only when the same scene produces product pixels, private pixels, hit results, and
semantic output; submission ownership is intact; canonical state round-trips through its real owner;
device recovery works; and no placeholder or duplicate path remains.
