---
name: superi-integration
description: Reconcile, map, review, verify, and deliver a completed Superi checkpoint without discarding concurrent work.
---

# Superi Integration

## Pre-delivery integration

Before any human gate or Git delivery:

1. Review `git status`, staged state, untracked state, and complete diff.
2. Identify preexisting changes and preserve their ownership.
3. Update all affected module maps and the global index where relationships changed.
4. Run the map validator.
5. Run focused, subsystem, real-consumer, boundary, dependency, API, capture, and final verifier
   commands from `execution.md`.
6. Read every changed file and diff line.
7. Confirm no placeholder, dead route, stale instruction, duplicate owner, old presentation
   dependency, or hidden manual step remains.

For Phase Infinity interface checkpoints, stop here and enter `superi-human-acceptance`. Do not
commit or push.

## Approved delivery

After exact human `Yes`, and only then:

1. Perform final cleanup without changing the accepted product direction.
2. Run `git fetch origin`.
3. Inspect divergence and incoming file diff.
4. Fast-forward or safely rebase owned commits as needed.
5. Preserve both sides of every conflict and stop if safe resolution is not provable.
6. Re-read affected instructions, interfaces, and maps after integration.
7. Rerun the complete verification matrix on final source.
8. Run the map validator immediately before push.
9. Review the final staged diff and commit message.
10. Commit with the root message convention.
11. Push without force.
12. Prove the delivered commit exists on `origin/main`.
13. Complete the paired Google Docs lifecycle with revision control.
14. Read both tabs back and verify every completion condition.

## Non-interface delivery

For a checkpoint that does not require human visual acceptance, perform the approved-delivery steps
after every technical and integration gate passes.

## Completion description

Write exactly three concise sentences:

1. begins `Implemented` and states the outcome
2. names principal systems, files, interfaces, or integration
3. states fresh verification and delivered `origin/main` commit

Apply black nonbold nonitalic unhighlighted Urbanist at 11 pt. Keep the description ID bold. On the
main tab, highlight only the immutable specification text with the fixed completion color.

## Exit gate

Delivery passes only when final source is verified, maps are current, remote delivery is proven,
both document tabs pass fresh readback, the active claim is gone, and no completion formatting
boundary is wrong. Then return exactly `Done.`
