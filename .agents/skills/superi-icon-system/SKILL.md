---
name: superi-icon-system
description: Reuse or produce original Superi icons through the locked GPT Image concept, visual rejection, cleanup, vectorization, normalization, state, registry, and atlas workflow.
---

# Superi Icon System

## Required design authority

Read `../superi-interface-design/SKILL.md` completely before working with icons. Its icon appearance,
color, and state decisions are locked. This skill supplies the production and registry workflow.

## Registry first

Before adding artwork:

1. Read the icon registry and relevant atlas map in full.
2. Search by semantic action, object, state, and neighboring usage.
3. Reuse an existing icon when its meaning matches.
4. Never fork an icon only to obtain a slightly different visual weight.

## New icon workflow

When no semantic match exists:

1. Define the exact action, object, state, or category meaning without naming a familiar
   third-party glyph.
2. Declare whether and why the icon uses color in default, hover, active, clicked, toggled,
   category, or status states.
3. Generate exactly two original starting concepts with GPT Image 2 on a black background. Use a
   white icon unless the declared color contract requires color.
4. Inspect both concepts visually at useful crops and actual interface sizes.
5. Reject concepts that are weak, derivative, ambiguous, blob-like, generic, or dependent on
   invisible small decoration.
6. Generate exactly one revised concept after each rejection.
7. Repeat inspection and revision until the silhouette is excellent.
8. Remove the black background and background-transition pixels while preserving intentional
   shading and color.
9. Vectorize the accepted raster without replacing it with manually invented vector geometry.
10. Normalize it to the 24 by 24 logical view box.
11. Apply the current line, corner, gap, shading, and optical-correction vocabulary at actual
    interface sizes.
12. Keep the silhouette distinct from adjacent registry entries.
13. Render every declared grayscale and color state against Superi's black interface, including
    selected, disabled, compact, default, and high-DPI proofs.
14. Inspect full pixels and useful crops, then reject or revise failures.
15. Assign a lowercase dotted name, semantic description, category, color contract, version,
    accepted vector source, and review status.
16. Register, hash, atlas-pack, and integrate the asset.

Do not copy, trace, recolor, or mechanically modify third-party icons.
Do not manually construct a new icon from invented vector geometry.

## Technical contract

Every registry item provides:

- stable name and version
- semantic description
- normalized source geometry
- logical bounds and optical inset
- fill or stroke behavior
- category
- grayscale and color-state contract
- atlas identity
- deterministic render hash
- screenshot proof
- duplicate and collision review
- semantic brief and accepted generation lineage

Icon identity must not depend on atlas position. A missing icon fails with an explicit diagnostic and
never silently renders a different meaning.

## Accessibility

Icons do not supply their own accessible name when a surrounding control already does. An icon-only
control must expose an explicit action name. State changes require semantic state and visible
non-color feedback.

## Exit gate

Run registry validation, atlas determinism, duplicate detection, visual capture, and semantic-control
tests. Confirm that at least 10 percent of the complete registry has a meaningful colored version,
without treating 10 percent as a target or cap. Record accepted and rejected concepts in
`planning.md`, and record final hashes and captures in `execution.md`.
