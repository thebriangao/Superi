---
name: superi-execution
description: Implement one planned Superi checkpoint inline with proof-first slices, complete integration, review, and evidence.
---

# Superi Execution

## Entry gate

Proceed only when:

- the exact live claim is still owned by this task
- remote synchronization is current
- `planning.md` and `execution.md` exist and have been re-read in full
- affected source, interfaces, callers, consumers, manifests, schemas, tests, fixtures, maps, and
  governing documents have been read through EOF
- the architecture, migration, error behavior, map work, and proof plan are fixed

## Execute

1. Write the smallest real failing proof for the first planned behavior.
2. Run it and record the expected failure.
3. Implement the smallest complete slice through its actual consumer.
4. Include errors, degradation, cancellation, migration, permissions, feature configuration,
   diagnostics, and compatibility needed by that slice.
5. Rerun the focused proof.
6. Record command, result, files, decision, and remaining risk in `execution.md`.
7. Capture privately whenever a visual or interaction decision becomes real.
8. Continue in planned order.

No placeholders, mocked success, dead interfaces, hidden manual steps, duplicate state, or unrelated
rewrites.

## Native UI checkpoints

Use `superi-wgpu-construction`, `superi-visual-design`, `superi-icon-system`, and
`superi-visual-capture` for their scopes. Every UI action must use a canonical transaction owner.
Every drawn, hit-tested, focused, and semantic element must derive from the same retained scene.

## Migrations

- preserve source until destination validation succeeds
- record source hash and schema
- preserve identity, exact values, unknown data, and provenance
- fail closed with a recoverable diagnostic
- test successful, repeated, partial, corrupt, and interrupted migration

## Widening verification

After each implementation unit:

1. focused unit or fixture proof
2. affected crate or subsystem suite
3. caller and consumer suite
4. boundary, dependency, API, migration, and capture proof as applicable

Run final repository verification only after the implementation and maps are current.

## Review

Read every changed file and the complete diff line by line. Re-read critical interfaces and tests in
full. Check the result against:

- live checkpoint and non-goals
- canonical ownership
- dependency direction
- failure and degraded behavior
- accessibility and interaction
- product and visual plan
- map accuracy
- real-consumer proof

Correct every defect and rerun affected proofs.

## Exit gate

Hand off to `superi-testing` only when implementation is complete, no planned slice remains, focused
and subsystem proofs pass, captures are inspectable, and `execution.md` accurately records the live
state.
