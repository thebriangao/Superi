---
name: superi-mapping
description: Use to create, validate, or refresh Superi's comprehensive codebase maps before planning or after source changes.
---

# Superi Mapping

## Purpose

Maintain `docs/codebase-map/` as a current, comprehensive navigation layer over the repository. A
map accelerates orientation, but never replaces reading the raw files that a worker will change or
the interfaces and tests needed to prove the change.

## Mapping contract

Every discovered repository module owns one map at `docs/codebase-map/modules/<module-id>.md`. The
mapping script defines module membership, source hashes, complete file inventories, and required map
sections. Run it from the repository root:

```text
python3 .agents/skills/superi-mapping/scripts/codebase_maps.py inventory
python3 .agents/skills/superi-mapping/scripts/codebase_maps.py files <module-id>
python3 .agents/skills/superi-mapping/scripts/codebase_maps.py shards <module-id> --max-lines 4000
python3 .agents/skills/superi-mapping/scripts/codebase_maps.py validate
```

The `workspace` module owns repository files outside `open/crates/*` and `open/tools/*`. Generated
output, dependency caches, ignored files, plan files, and the generated map tree are excluded.
Tracked binary artifacts remain in the inventory, but their bytes are not treated as readable prose.

For ordinary checkpoint work, the one tier 2 research planner performs initial map validation and
reads the global index plus the complete affected caller, consumer, contract, and runtime-path map
closure. This work is read-only. The planner records stale or missing maps and replaces their
authority with deeper raw-code evidence. After implementation, the checkpoint owner alone refreshes
affected maps and validates them. The owner does not need to repeat the planner's initial map
ingestion.

## Create all maps

This workflow applies only when the user explicitly assigns a full-map creation or rebuild outside a
normal checkpoint. The current agent performs it sequentially and may not create reader, writer,
synthesizer, or reviewer agents:

1. Synchronize the repository safely, run `inventory`, and establish one ordered module list.
2. Partition large modules with `shards`. A shard contains whole files only and may exceed the line
   target when one file is larger than the target.
3. Read every assigned text file from its first line through EOF. Reading in chunks is allowed only
   when every chunk is consumed. Search may locate symbols, but search output is never a substitute
   for the full read.
4. Maintain a structured local note at
   `plans/codebase-mapping/<module-id>/shards/<shard-id>.md` for each shard. Cover every assigned file,
   public and internal surfaces, data flow, dependencies, consumers, invariants, tests, incomplete
   behavior, risks, and relationships that another module map must mention.
5. Read every shard note in full, then read all module manifests, public entry points, and
   cross-module interfaces needed to reconcile the evidence. Write the final module map with the
   exact hash and file count reported by the mapping script.
6. After every module exists, synthesize `docs/codebase-map/index.md` from all maps. Explain global
   layering, dependency direction, major runtime flows, shared invariants, and where each concern is
   owned.
7. Run `validate`, repair every failure, inspect the complete map diff, and rerun validation.

## Module map format

Each module map starts with this metadata, using values from the script:

```text
---
module_id: <module-id>
source_paths:
  - <owned-path>
source_hash: <sha256>
source_files: <count>
mapped_at_commit: <git-revision>
---
```

It then contains all of these sections exactly once:

```text
## Purpose and ownership
## Source inventory
## Public surface
## Architecture and data flow
## Dependencies and consumers
## Invariants and operational boundaries
## Tests and verification
## Current status and risks
## Maintenance notes
```

`Source inventory` lists every owned path in backticks and explains its concrete role. The rest of
the map describes implemented reality, not intended future architecture. Clearly label placeholders,
unfinished paths, inferred relationships, and behavior that is defined only by tests or docs.

## Refresh maps after a change

Map maintenance is part of implementation, not a later documentation task:

1. During planning, the one tier 2 research planner runs `validate`, reads the required map closure,
   and records missing or stale maps. It never edits a map and uses deeper raw-code evidence whenever
   map authority is unavailable.
2. Before editing source, the checkpoint owner reads both plans and every raw file selected for
   modification, along with its relevant callers, consumers, public interfaces, tests, schemas, and
   governing documents, in full. It may rely on the planner for initial map orientation.
3. After editing and testing, run `changed --base <revision>` to identify directly affected modules.
4. For each affected module, read its existing map, every changed file, and every related interface
   or test in full. Update the source inventory and all architectural statements affected by the
   change, including removals, renames, new consumers, changed invariants, and new proof.
5. Recompute the module hash and file count after all source edits. Update `mapped_at_commit` to the
   revision the work is based on, or `working-tree` when source edits are not committed yet.
6. Update maps for other modules when their consumer relationship or contract changed even if their
   own source hash did not. Update the global index whenever ownership, layering, public flow, or
   module status changed.
7. After the final rebase and before delivery, rerun `validate`. If a map conflicts during rebase,
   regenerate it from the rebased source and reconciled behavior instead of choosing one side.

No source-changing commit is complete while an affected map is stale. Do not update only the hash;
the prose and inventory must truthfully describe the resulting code.

The checkpoint owner is the only map writer. It personally reviews the final map diff, reconciles
cross-module statements, and runs the final validator after integration. Never spawn a map reader,
writer, synthesizer, or reviewer for checkpoint execution.
