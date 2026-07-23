---
name: superi-testing
description: Verify one Superi checkpoint through focused, subsystem, consumer, deterministic, accessibility, and failure proofs.
---

# Superi Testing

## Test truth

Compilation is not behavior. A source-pattern assertion is not interaction. A generated file is not
a playable export. A screenshot is not semantic proof. Test the real consumer and report exactly
which proof ran.

## Build a verification matrix

From the live checkpoint and plans, list:

- primary success behavior
- empty, loading, busy, degraded, failure, cancellation, and recovery states
- permissions and stale revision
- migration and compatibility
- deterministic output
- concurrency and backpressure
- device or process loss
- accessibility and keyboard completion
- private pixel and semantic capture
- real consumer or exported artifact

Every promised outcome needs at least one proof.

## Widening rings

1. unit and property tests for local invariants
2. crate tests for public behavior
3. subsystem tests for ownership and integration
4. caller and consumer tests
5. boundary, dependency, fixture, and generated API checks
6. deterministic private capture and semantic inspection
7. native platform smoke when platform presentation changes
8. full checkpoint verifier

For broad work run:

`python3 .agents/skills/superi-execution/scripts/verify_checkpoint.py --base <base-revision> --full`

## Phase Infinity interface proof

Verify at minimum:

- stable retained IDs
- deterministic layout
- exact clipping and draw order
- text and icon atlas determinism
- pointer hit and capture
- focus order and restoration
- keyboard task completion
- semantic role, name, state, value, bounds, and actions
- visible focus and non-color state
- scale and high-contrast behavior
- screenshot repeatability
- interaction-driven pixel and semantic change
- device and surface reconstruction
- no direct queue submission outside the owner

Use the same product renderer for capture. Never repair a failing golden by accepting unexplained
pixel changes.

## Failure discipline

When a test fails:

1. preserve output
2. identify the earliest broken invariant
3. reproduce with the smallest real case
4. fix source rather than weaken proof
5. rerun the focused test
6. rerun every broader ring made relevant by the fix

## Evidence

Record exact command, exit status, important count, artifact path, hash, and limitation in
`execution.md`. Distinguish tests run now from historical evidence and simulated proof from physical
hardware proof.

## Exit gate

Testing passes only when all matrix rows are proven, the deterministic verifier succeeds, captures
are repeatable, the real consumer succeeds, no diagnostic was weakened, and every limitation is
stated precisely.
