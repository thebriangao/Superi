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
2. **Map reality.** Read every applicable `AGENTS.md`, current code, tests, public interfaces,
   relevant docs, recent history, and unfinished neighboring work. Trace the real consumer path
   before proposing changes.
3. **Research uncertainty.** Research the topic whenever specifications, standards, licenses,
   security guidance, platform behavior, algorithms, dependencies, or current best practice matter.
   Prefer primary sources and current official documentation. Cross-check consequential claims,
   record exact versions and license implications, and separate verified facts from inference.
4. **Design the proof.** Name exact files and interfaces, data flow, error paths, compatibility
   constraints, and tests. Include feature configurations, migrations, concurrency, GPU, codec,
   offline, and open or closed boundary effects when applicable.
5. **Sequence the work.** Order small implementation slices so each has a failing proof first, a
   minimal implementation, and focused verification. End with the full relevant suite, real workflow
   validation, delivery, and checklist readback required by root `AGENTS.md`.

## Internal output

Keep the plan concise and internal. Structure it as:

```text
outcome: one measurable sentence
evidence: repository findings and research conclusions
change map: ordered files, interfaces, and implementation slices
proof: focused failures, success tests, full suites, and real workflow
risks: only live risks with a concrete mitigation or blocking condition
```

Every checkpoint requirement must map to an implementation slice and proof. Leave no placeholders,
unresolved design choices, or invented architecture. Do not create a plan document or ask for routine
approval unless the checkpoint explicitly requires one.

If legal approval, credentials, external authority, or an absent architectural decision blocks safe
work, follow the root `AGENTS.md` blocked path. Otherwise invoke `superi-execution` immediately and
adapt the plan whenever repository evidence changes.
