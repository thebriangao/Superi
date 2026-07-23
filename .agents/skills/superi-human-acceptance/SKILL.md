---
name: superi-human-acceptance
description: Run the exact Phase Infinity screenshot judgment, feedback, approval, and delivery state machine.
---

# Superi Human Acceptance

## Entry gate

Enter only when:

- implementation is complete
- every technical and real-consumer proof passes
- maps are current
- private captures are deterministic
- full screenshots and useful crops have been inspected
- pixels and semantic JSON agree
- the complete diff has been reviewed
- `execution.md` records all evidence

## Ready for judgment

Show final screenshots in the task. Include enough visual evidence for the human to judge the whole
surface, important detail, and interaction result.

Do not:

- commit
- push
- replace the claim
- highlight the main specification
- call the checkpoint complete

End the turn with exactly:

`Is this good?`

This state is `awaiting_judgment`. It releases an orchestrator execution slot while keeping the task
available for direct human response.

## Awaiting judgment

Accept only exact case-sensitive `Yes` or `No`.

- For any other message reply exactly `Instruction unclear. Try again with a Yes or No.`
- For exact `No` reply exactly `What should change?` and enter `collecting_feedback`.
- For exact `Yes` enter `approved_delivery`.

## Collecting feedback

Accept the next human message as feedback regardless of content.

1. Reinspect the relevant source, captures, and semantics.
2. Research again if the feedback exposes an unresolved product or platform question.
3. Brainstorm deliberate alternatives.
4. Update `planning.md` and `execution.md`.
5. Implement the selected revision.
6. Rerun focused and complete verification.
7. Render and inspect new final screenshots.
8. Return to `awaiting_judgment` and end with exactly `Is this good?`.

Do not apply a superficial patch without re-evaluating the affected design.

## Approved delivery

Exact `Yes` authorizes `superi-integration` to perform final cleanup, fresh synchronization, safe
rebase, complete verification, commit, push, paired document completion, highlight, and readback.

If approved state changes materially during integration, return to judgment with new screenshots.
When every delivery gate passes, return exactly `Done.`
