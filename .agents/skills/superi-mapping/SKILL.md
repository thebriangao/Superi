---
name: superi-mapping
description: Validate, read, reconcile, and refresh Superi codebase maps before planning and after implementation.
---

# Superi Mapping

## Purpose

Use repository maps as an orientation and maintenance contract, never as a substitute for source.
The checkpoint owner performs all mapping inline.

## Before planning

1. Run:

   `python3 .agents/skills/superi-mapping/scripts/codebase_maps.py validate`

2. Read `docs/codebase-map/index.md` from first line through EOF.
3. Identify every affected module, caller, consumer, public contract, runtime path, test surface, and
   governing document.
4. Read every corresponding module map in full.
5. Compare map claims with manifests and raw source. Record every stale, incomplete, or missing map
   in `planning.md`.
6. If a map is stale, use source as authority and include map repair in the checkpoint.

Search is for discovery only. A search result does not count as reading a map or source file.

## During implementation

- Track every source, manifest, schema, fixture, and ownership change.
- Update a module map when its inventory, public surface, behavior, flow, dependency, invariant,
  test, status, risk, or maintenance note changes.
- Update every consumer map made inaccurate by the change.
- Update the global index when ownership, layering, public flow, dependency direction, runtime
  relationship, module status, or product boundary changes.
- Never update only a hash or count.
- Preserve preexisting map edits and reconcile them line by line.

## Refresh

Use the repository script for mechanical counts and hashes, then review the complete generated diff.
Do not accept a generated map until its prose matches the resulting source.

Run the validator:

1. before planning
2. after each map refresh
3. after final integration or rebase
4. immediately before push

## Proof

Record the exact validator command and result in `execution.md`. Completion requires every changed
source path to be represented by a current module map, every affected relationship to be current in
the global index, and zero validator errors.
