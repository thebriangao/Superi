---
name: superi-planning
description: Use when an agent receives one Superi checklist checkpoint or must revise its implementation plan after discovering changed code, requirements, standards, dependencies, or research.
---

# Superi Planning

## Principle

Plan from evidence, not memory. Find the smallest complete path from the assigned checkpoint to
working, testable behavior through Superi's real architecture.

## Build the plan

1. **Pin and claim as owner.** The tier 0 or tier 1 checkpoint owner reads the exact checkpoint and
   surrounding workstream from both configured tabs, extracts the behavior and boundaries, places the
   exact timestamped `CLAIMED |` suffix, and verifies both tabs through fresh readback. The owner also
   synchronizes the repository and records the base revision before delegating expensive work.
2. **Create the planning team.** Spawn project-scoped `superi-tier2` specialists with detailed
   assignments under the checkpoint team model in root `AGENTS.md`. Use parallel map, raw-file,
   standards, dependency, license, security, platform, or algorithm researchers where the work is
   independent. Name peers and require direct coordination when their evidence intersects.
3. **Assign one lead planner.** Make one tier 2 lead planner the sole writer of
   `plans/<checkpoint-id>/planning.md`. Give it the live checkpoint specification, repository law,
   base revision, required output schema, and every evidence handoff. Other specialists send concise
   findings to the lead planner and owner rather than creating extra plan documents.
4. **Load the codebase maps.** Assigned specialists run the map validator, refresh any missing or
   stale relevant map with `superi-mapping`, then read the global index, every directly affected map,
   and every caller, consumer, contract, and runtime-path map in full. Omit another map only through
   the root `AGENTS.md` raw-code substitution rule. Record each reader, complete inventory, ownership,
   dependency direction, public boundary, runtime flow, current status, and likely raw-file scope.
   Never use a stale map or search result as source evidence.
5. **Read the implementation path.** Assigned specialists read every raw file that may change from
   first line through EOF, plus all relevant callers, consumers, public interfaces, manifests, tests,
   schemas, fixtures, and governing docs needed to trace and prove the checkpoint. Apply the same rule
   to every directly changed module. Continue after truncation until EOF and report the exact reading
   inventory, omitted-map substitutes, consumer path, recent history, material findings, and open
   questions to the planner and owner.
6. **Research uncertainty.** Research whenever specifications, standards, licenses, security
   guidance, platform behavior, algorithms, dependencies, or current best practice matter. Prefer
   primary sources and current official documentation. Cross-check consequential claims, record exact
   versions and license implications, and separate verified facts from inference.
7. **Synthesize architecture and proof.** The lead planner reconciles all evidence into one coherent
   design with exact files and interfaces, data flow, error paths, compatibility constraints, and
   tests. Include feature configurations, migrations, concurrency, GPU, codec, offline, and open or
   closed boundary effects when applicable. Every checkpoint sentence must map to implementation and
   proof without unresolved design choices.
8. **Decompose execution.** After the owner reviews `planning.md` for evidence completeness and
   architectural compatibility, assign one tier 2 execution decomposer as the initial sole writer of
   `plans/<checkpoint-id>/execution.md`. It divides the plan into ordered, dependency-aware slices,
   assigns disjoint file ownership, identifies required failing proofs and widening verification, and
   marks which slices may run concurrently. End with integration review, map refresh, deterministic
   verification, delivery, and checklist readback owned by the checkpoint owner.

## Internal output

Write the concise internal plan to `plans/<checkpoint-id>/planning.md`. This file is mandatory and
is the only planning document permitted for the checkpoint. Structure it as:

```text
outcome: one measurable sentence
reading record: global index, required maps, omitted-map substitutions, and selected raw files
evidence: repository findings and research conclusions
change map: ordered files, interfaces, and implementation slices
proof: focused failures, success tests, applicable local suites, and real consumer
risks: only live risks with a concrete mitigation or blocking condition
```

Every checkpoint requirement must map to an implementation slice and proof. The plan is invalid
until the map validator passes, every mandatory map was read in full by a named specialist, every
omitted map has recorded raw-code substitutes, and every selected raw file was read through EOF and
recorded. The owner must verify the evidence record and resolve conflicts between specialist reports,
but should not duplicate the complete initial ingestion. Leave no placeholders, unresolved design
choices, or invented architecture. Do not create any planning document other than the mandatory
`planning.md`, and do not ask for routine approval unless the checkpoint explicitly requires it.

If legal approval, credentials, external authority, or an absent architectural decision blocks safe
work, follow the root `AGENTS.md` blocked path. Otherwise invoke `superi-execution` immediately and
adapt the plan whenever repository evidence changes.
