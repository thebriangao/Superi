---
name: superi-icon-system
description: Reuse, design, normalize, inspect, version, and register original Superi interface icons.
---

# Superi Icon System

## Registry first

Before adding artwork:

1. Read the icon registry and relevant atlas map in full.
2. Search by semantic action, object, state, and neighboring usage.
3. Reuse an existing icon when its meaning matches.
4. Never fork an icon only to obtain a slightly different visual weight.

## New icon workflow

When no semantic match exists:

1. Write the action or object meaning without naming a familiar third-party glyph.
2. Sketch at least two original concepts.
3. Reject concepts that resemble researched product artwork or depend on tiny decoration.
4. Normalize the chosen concept to the 24 by 24 logical view box.
5. Apply the current line weight, cap, join, corner, gap, and optical-correction vocabulary.
6. Keep the silhouette distinct from adjacent registry entries.
7. Render light-on-dark, dark-on-light, selected, disabled, compact, default, and high-DPI proofs.
8. Inspect full pixels and useful crops.
9. Revise or reject failures.
10. Assign a lowercase dotted name, semantic description, category, version, source geometry, and
    review status.
11. Register and atlas-pack the asset.

Do not copy, trace, recolor, or mechanically modify third-party icons.

## Technical contract

Every registry item provides:

- stable name and version
- semantic description
- normalized source geometry
- logical bounds and optical inset
- fill or stroke behavior
- category
- atlas identity
- deterministic render hash
- screenshot proof
- duplicate and collision review

Icon identity must not depend on atlas position. A missing icon fails with an explicit diagnostic and
never silently renders a different meaning.

## Accessibility

Icons do not supply their own accessible name when a surrounding control already does. An icon-only
control must expose an explicit action name. State changes require semantic state and visible
non-color feedback.

## Exit gate

Run registry validation, atlas determinism, duplicate detection, visual capture, and semantic-control
tests. Record accepted and rejected concepts in `planning.md`, and record final hashes and captures
in `execution.md`.
