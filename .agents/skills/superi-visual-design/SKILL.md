---
name: superi-visual-design
description: Research, compose, inspect, and iterate original Superi interface surfaces while applying the separate locked interface design authority.
---

# Superi Visual Design

## Required design authority

Read `../superi-interface-design/SKILL.md` completely before using this workflow. That skill owns
Superi's locked appearance and shell decisions. This skill owns research, composition, inspection,
and iteration procedure only. Never restate, override, or reopen the locked decisions.

## Before drawing

1. Read the live checkpoint and relevant section of the Phase Infinity master architecture.
2. Identify the primary task, information hierarchy, canonical selection, focus target, command
   route, persistent state, ephemeral state, empty state, loading state, degraded state, failure
   state, and recovery action.
3. Inspect adjacent Superi surfaces through private captures.
4. Research unresolved task patterns from public primary material only.
5. Brainstorm at least three original directions.
6. Record a compact comparison and choose one direction before source changes.

## Surface specification

Record:

- stable spatial roles
- entry and exit paths
- hierarchy from application to status
- active, selected, focused, hovered, pressed, disabled, busy, warning, and destructive state
- pointer, keyboard, assistive-technology, and reduced-motion behavior
- text hierarchy and tabular-number needs
- icon reuse or creation
- full-surface and crop capture states
- expected appearance at compact, default, large-text, and high-contrast settings

Selection and focus must be visually distinct. No required state may rely on color alone. Compact
targets must provide 24 by 24 logical hit area or equivalent spacing. Visible focus must meet a
strong 2 px perimeter-equivalent area and at least 3:1 change contrast.

## Locked-system checks

- Apply every relevant rule in `superi-interface-design`.
- Keep primary media, timelines, graphs, scopes, and waveforms visually dominant.
- Make empty states offer the next action and useful shortcut.
- Make failures name impact, preserved work, and recovery.
- Keep motion explanatory and repeated work immediate.

## Iteration

Capture early, not only at the end. Inspect the whole surface and useful crops after every material
composition change. Correct hierarchy, alignment, spacing, clipping, baseline, icon, contrast,
focus, density, and state defects in source. Do not edit screenshots.

## Exit gate

The design passes only when real rendered pixels and semantic inspection agree, every required state
is legible, keyboard and assistive routes are complete, adjacent surfaces feel related, and no
third-party visual identity has been copied.
