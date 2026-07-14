---
name: superi-planning
description: Use when an agent receives one Superi checklist checkpoint or must revise its implementation plan after discovering changed code, requirements, standards, dependencies, or research.
---

# Superi Planning

## Principle

Plan from evidence, not memory. Find the smallest complete path from the assigned checkpoint to
working, testable behavior through Superi's real architecture.

## Build the plan

1. **Pin the assignment.** Read the exact checkpoint and surrounding workstream from the master
   checklist. Extract the required behavior, dependencies, exclusions, and completion evidence.
2. **Claim visibly.** Follow the root `AGENTS.md` claim lifecycle before expensive research. Place
   the timestamped claim paragraph directly below the checkpoint, apply the exact configured font,
   size, and color, and verify it through a fresh document readback before touching the repository.
3. **Load the codebase maps.** Run the map validator, refresh any missing or stale relevant map with
   `superi-mapping`, then read the global index and every module map in full. Use the maps to define
   affected ownership, dependency direction, public boundaries, runtime flow, current status, and
   likely raw-file scope. Never use a stale map or a search result as source evidence.
4. **Read the implementation path.** Read every raw file that may change from first line through EOF,
   plus all relevant callers, consumers, public interfaces, manifests, tests, schemas, fixtures, and
   governing docs needed to trace and prove the checkpoint. Apply the same rule to every directly
   changed module. Chunk only when every chunk is consumed, and continue after tool truncation until
   EOF. Record the complete map inventory, selected raw-file inventory, traced consumer path, recent
   history, and material findings as planning evidence.
5. **Research uncertainty.** Research the topic whenever specifications, standards, licenses,
   security guidance, platform behavior, algorithms, dependencies, or current best practice matter.
   Prefer primary sources and current official documentation. Cross-check consequential claims,
   record exact versions and license implications, and separate verified facts from inference.
6. **Design the proof.** Name exact files and interfaces, data flow, error paths, compatibility
   constraints, and tests. Include feature configurations, migrations, concurrency, GPU, codec,
   offline, and open or closed boundary effects when applicable.
7. **Sequence the work.** Order small implementation slices so each has a failing proof first, a
   minimal implementation, and focused verification. End with the full relevant suite, real workflow
   validation, delivery, and checklist readback required by root `AGENTS.md`.

## Internal output

Keep the plan concise and internal. Structure it as:

```text
outcome: one measurable sentence
reading record: every map plus all selected raw files read through EOF
evidence: repository findings and research conclusions
change map: ordered files, interfaces, and implementation slices
proof: focused failures, success tests, full suites, and real workflow
risks: only live risks with a concrete mitigation or blocking condition
```

Every checkpoint requirement must map to an implementation slice and proof. The plan is invalid
until the map validator passes, every codebase map was read in full, and every selected raw file was
read through EOF and recorded. Leave no placeholders, unresolved design choices, or invented
architecture. Do not create a plan document or ask for routine approval unless the checkpoint
explicitly requires one.

If legal approval, credentials, external authority, or an absent architectural decision blocks safe
work, follow the root `AGENTS.md` blocked path. Otherwise invoke `superi-execution` immediately and
adapt the plan whenever repository evidence changes.
