---
name: superi-visual-capture
description: Render, control, semantically inspect, crop, compare, and review Superi privately through the product wgpu path.
---

# Superi Visual Capture

## Required path

Use `tools/superi-capture`, which invokes the Rust `superi-ui-inspect` tool. It must use the same
retained scene, layout, text, icon, input, semantics, and wgpu renderer as `superi-desktop`.

Never use:

- a mock or second renderer
- source-only inspection as visual proof
- third-party application capture
- the user's foreground desktop when private capture can prove the state
- image editing to repair rendered defects

## Deterministic setup

Pin:

- named fixture and state
- logical width and height
- scale factor
- target color format
- font and icon registry version
- locale and text direction
- clock and animation point
- random seed
- GPU adapter policy

Use only synthetic or repository fixtures free of user media, secrets, and private project data.

## Constructive loop

1. Render as soon as the first composition is meaningful.
2. Inspect the semantic tree and full-resolution PNG.
3. Capture useful crops for text, icon, focus, selection, controls, seams, and failure state.
4. Control by stable node ID when possible.
5. Dispatch pointer, key, text, IME, focus, or clock input.
6. Render the next state.
7. Compare pixels, semantics, bounds, focus, actions, and input transcript.
8. Correct defects in source and repeat.

Quick captures during construction are encouraged. They are evidence, not completion by themselves.

## Commands

The wrapper provides:

- `render`
- `inspect`
- `click`
- `key`
- `type`
- `crop`
- `compare`

Use `--help` for the current exact arguments. Never invent an unsupported flag.

## Inspect

Review:

- information hierarchy
- pure-black plane and meaningful depth
- aligned seams and baselines
- compact text clarity
- icon silhouette and optical alignment
- clipping and atlas bleed
- active, selected, focused, hovered, pressed, disabled, busy, warning, and failure state
- generous hit bounds behind compact visuals
- semantic names, roles, values, actions, relationships, and focus order
- deterministic hash on repeated render

## Final evidence

Produce:

- one complete representative surface
- one useful component or typography crop
- one interaction result
- semantic JSON
- input transcript
- manifest with dimensions, scale, format, font, icons, clock, adapter, and hashes
- exact comparison result from a repeated render

Record all paths and hashes in `execution.md`.

## Exit gate

Capture passes when repeated output is deterministic, pixels and semantics describe the same state,
the interaction changes both correctly, all visual defects are resolved in source, and the final
screenshots are ready for human judgment.
