---
name: superi-planning
description: Use when a Superi checkpoint owner must inspect the live checklist and repository, decide whether external research is actually needed, and create the complete implementation and execution plans inline before building.
---

# Superi Planning

## Principle

Plan from evidence, not memory. Find the smallest complete path from the assigned checkpoint to
working, testable behavior through Superi's real architecture.

## Inline ownership

Perform the entire planning workflow in the checkpoint owner task. Never spawn or delegate to a
subagent. Create both mandatory files under `plans/<checkpoint-id>/` yourself, then execute them in
the same task.

## Build the plan

1. **Prepare as owner.** Read and claim the exact checkpoint on both configured tabs, synchronize the
   repository, record the base revision, and establish the product and repository boundary.
2. **Read live product state.** Read the checkpoint on both Google Docs tabs, its phase, workstream,
   subsystem, neighboring requirements, active claim, and relevant prior completion descriptions.
3. **Load current maps.** Run the map validator and read the global index plus every affected,
   caller, consumer, contract, and runtime-path map through EOF. Record stale or missing maps and
   compensate with deeper raw-code evidence instead of trusting stale prose.
4. **Read current implementation.** Read every likely changed file, public interface,
   caller, consumer, manifest, schema, test, fixture, and governing document needed to trace the real
   behavior. Every selected text file is read through EOF, including continuation after truncation.
5. **Decide whether research is needed.** Default to no external research when the live checklist,
   repository law, maps, code, tests, local documentation, and tool output answer the checkpoint.
   Research only a material unresolved standard, license, security, platform, dependency, algorithm,
   or current API question. Prefer primary sources and distinguish verified facts from inference.
6. **Synthesize both plans.** Think through architecture, failure paths, compatibility, implementation
   order, test-first proof, map maintenance, deterministic verification, delivery, and Google Docs
   completion. Write `planning.md` and `execution.md` yourself.
7. **Begin execution.** Re-read both plans, confirm every checkpoint sentence maps to code and proof,
   then invoke `superi-execution` inline without creating another task or agent.

## Internal output

Write the research and architecture plan to `plans/<checkpoint-id>/planning.md`:

```text
outcome: one measurable sentence
document record: exact live Google Docs paragraphs and surrounding completion context
reading record: global index, required maps, stale-map substitutions, and selected raw files
external evidence: none required, or primary sources, versions, verified facts, and inferences
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

Every checkpoint sentence must map to implementation and proof. The plan is invalid until the owner
has read the mandatory map closure, recorded any stale-map raw-code substitutes, read every selected
raw file through EOF, and resolved architecture choices. Leave no placeholders or invented
interfaces. Create no other plan documents.

If legal approval, credentials, external authority, or an absent architectural decision blocks safe
work, follow the root `AGENTS.md` blocked path. Otherwise invoke `superi-execution` and continue
inline without spawning another agent.
