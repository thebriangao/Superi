---
name: superi-execution
description: Use when an assigned Superi checkpoint has a research-backed internal plan and is ready to be built autonomously.
---

# Superi Execution

## Principle

Finish the real checkpoint, not an approximation. Working behavior through its actual consumer,
fresh verification, remote delivery, and checklist evidence define completion.

## Execute

1. **Synchronize safely.** Reconfirm the checkpoint claim, inspect the worktree, fetch current remote
   state, and integrate without discarding any existing work. Never delete code you did not write.
2. **Load the plan.** Turn the `superi-planning` output into a live task list. Keep one slice active,
   but retain ownership of the entire checkpoint and its necessary dependencies.
3. **Prove before changing.** For each behavior, write the smallest real test first and run it to
   confirm the expected failure. For bugs, reproduce the defect. For existing untested behavior,
   capture it before modifying it. Then implement the smallest complete change and run the proof
   again.
4. **Integrate completely.** Follow current interfaces and dependency direction. Include real error
   paths, migrations, feature configurations, diagnostics, and documentation required by the
   checkpoint. No placeholders, mocked success, dead interfaces, or hidden manual steps.
5. **Verify in widening rings.** Run focused tests after every slice, relevant subsystem suites after
   integration, then every final command required by root `AGENTS.md`. Headless proof is mandatory.
   Use computer control only for behavior that cannot be proven headlessly, and verify resulting
   engine or project state instead of trusting appearance alone.
6. **Review the whole result.** Inspect the diff line by line against the checkpoint, research, plan,
   architectural boundaries, failure modes, and neighboring work. Remove only defects introduced by
   this checkpoint. Never weaken a test, requirement, diagnostic, or safety boundary to pass.
7. **Deliver and record.** Follow the root `AGENTS.md` Git delivery and Google Docs completion steps
   exactly. Reverify after the final rebase, prove the commit exists on the required remote branch,
   then check and annotate only the assigned checkpoint.

## Autonomy loop

When evidence breaks an assumption, inspect the root cause, research the missing fact, revise the
internal plan, and continue. Retry transient tool, test, rebase, push, and document failures with a
bounded corrective change. Do not pause for routine choices, progress reports, or approval between
slices.

Stop only for a blocking condition defined by root `AGENTS.md`. Otherwise continue until every item
in its completion gate passes, then return exactly `Done.`
