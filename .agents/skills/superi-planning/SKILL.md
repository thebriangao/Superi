---
name: superi-planning
description: Use when one Superi checkpoint needs a single plan-only tier 2 researcher to inspect Google Docs, maps, current code, and external evidence before the checkpoint owner implements it alone.
---

# Superi Planning

## Principle

Plan from evidence, not memory. Find the smallest complete path from the assigned checkpoint to
working, testable behavior through Superi's real architecture.

## Roles

The checkpoint owner synchronizes and claims the work, creates one tier 2 research planner, then
implements and delivers the checkpoint alone. The research planner performs broad ingestion and hard
architectural reasoning, but its only writes are the two mandatory files under
`plans/<checkpoint-id>/`. No checkpoint may create a second tier 2 agent for any reason.

## Build the plan

1. **Prepare as owner.** Read and claim the exact checkpoint on both configured tabs, synchronize the
   repository, record the base revision, and establish the product and repository boundary.
2. **Create one planner.** Spawn exactly one project-scoped `superi-tier2` agent with the checkpoint
   ID, configured document coordinates, base revision, repository law, and required output schema.
   Never spawn another agent. If the planner becomes unavailable, the owner finishes the missing
   planning itself.
3. **Read live product state.** The planner independently reads the checkpoint on both Google Docs
   tabs, its phase, workstream, subsystem, neighboring requirements, active claim, and relevant prior
   completion descriptions. It changes nothing in the document.
4. **Load maps read-only.** The planner runs the map validator and reads the global index plus every
   affected, caller, consumer, contract, and runtime-path map through EOF. It records stale or missing
   maps and compensates with deeper raw-code evidence instead of editing the map or trusting stale
   prose.
5. **Read current implementation.** The planner reads every likely changed file, public interface,
   caller, consumer, manifest, schema, test, fixture, and governing document needed to trace the real
   behavior. Every selected text file is read through EOF, including continuation after truncation.
6. **Research uncertainty.** Research current specifications, standards, licenses, security
   guidance, platform behavior, algorithms, and dependencies whenever they matter. Prefer primary
   sources, cross-check consequential claims, record exact versions and license implications, and
   separate verified facts from inference.
7. **Synthesize both plans.** Think through architecture, failure paths, compatibility, implementation
   order, test-first proof, map maintenance, deterministic verification, delivery, and Google Docs
   completion. Write `planning.md` and `execution.md`, then return one concise evidence handoff.
8. **Transfer to the owner.** The owner reads both plan files in full, then independently reads all
   relevant raw code and tests through EOF before editing. The planner's map-backed orientation
   removes the owner's initial map-reading requirement, but never replaces the owner's direct
   understanding of the implementation it will change.

## Internal output

Write the research and architecture plan to `plans/<checkpoint-id>/planning.md`:

```text
outcome: one measurable sentence
document record: exact live Google Docs paragraphs and surrounding completion context
reading record: global index, required maps, stale-map substitutions, and selected raw files
research: primary sources, versions, verified facts, and explicit inferences
architecture: ownership, interfaces, data flow, failure paths, and compatibility boundaries
change map: ordered files, interfaces, implementation slices, and map maintenance
proof: failing tests, success tests, applicable local suites, real consumer, and final verifier
risks: only live risks with a concrete mitigation or blocking condition
```

Write the ordered build plan to `plans/<checkpoint-id>/execution.md`:

```text
base revision: synchronized commit before implementation
preconditions: claim, evidence, legal, dependency, and architecture gates
slices: one ordered owner-executed sequence with exact files and failing proof first
verification: focused, subsystem, consumer, map, deterministic, and UI proof as applicable
delivery: final fetch, integration, commit, rebase, push, remote verification, and document readback
evidence log: initially empty area for the owner to maintain during execution
```

Every checkpoint sentence must map to implementation and proof. The plan is invalid until the
planner has read the mandatory map closure, recorded any stale-map raw-code substitutes, read every
selected raw file through EOF, and resolved architecture choices. Leave no placeholders or invented
interfaces. Create no other plan documents and write no repository or Google Docs file outside the
two permitted plan files.

If legal approval, credentials, external authority, or an absent architectural decision blocks safe
work, report the exact blocker to the owner. Otherwise the owner invokes `superi-execution`, reads
the implementation itself, and executes without spawning another agent.
