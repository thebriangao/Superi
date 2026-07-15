---
name: superi-execution
description: Use when an assigned Superi checkpoint has a research-backed internal plan and is ready to be built autonomously.
---

# Superi Execution

## Principle

Finish the real checkpoint, not an approximation. Working behavior through its actual consumer,
fresh verification, remote delivery, and checklist evidence define completion.

## Execute

1. **Synchronize safely.** Reconfirm the exact timestamped claim suffix on the configured
   descriptions tab, re-read the matching immutable main-tab specification, inspect the worktree,
   fetch current remote state, and integrate without discarding any existing work. Never delete code
   you did not write.
2. **Load the plan.** Read the mandatory `plans/<checkpoint-id>/planning.md`, then create and maintain
   the live task list and execution evidence in `plans/<checkpoint-id>/execution.md`. These are the
   only two plan files permitted. Keep one slice active, but retain ownership of the entire checkpoint
   and its necessary dependencies. Refuse execution if the map validator did not pass, the global
   index or any mandatory map was not read in full, an omitted map lacks recorded raw-code
   substitutes, or the selected raw-file inventory was not read through EOF and recorded.
3. **Prove before changing.** For each behavior, write the smallest real test first and run it to
   confirm the expected failure. For bugs, reproduce the defect. For existing untested behavior,
   capture it before modifying it. Then implement the smallest complete change and run the proof
   again.
4. **Integrate completely.** Follow current interfaces and dependency direction. Include real error
   paths, migrations, feature configurations, diagnostics, and documentation required by the
   checkpoint. No placeholders, mocked success, dead interfaces, or hidden manual steps.
5. **Verify in widening rings.** Run focused tests after every slice and relevant subsystem suites
   after integration. Then run
   `python3 .agents/skills/superi-execution/scripts/verify_checkpoint.py --base <base-revision>` from
   the root, using `--full` for broad infrastructure changes or uncertain selection. Record every
   selected command and result in `execution.md`. The verifier is a floor, so also run every
   checkpoint-specific proof from `planning.md`. Headless proof is mandatory. Use computer control
   only for behavior that cannot be proven headlessly, and verify resulting engine or project state
   instead of trusting appearance alone.
6. **Refresh the maps.** Run `superi-mapping` after source edits and tests. Use the Git diff to find
   every directly affected module, then update each module map from the changed raw files and related
   interfaces read through EOF. Update consumer maps and the global index when contracts, ownership,
   layering, public flow, or status changed. Recompute hashes and file counts, and never change only
   metadata while leaving inaccurate prose.
7. **Review the whole result.** Inspect the source and map diff line by line against the checkpoint,
   research, plan, architectural boundaries, failure modes, and neighboring work. Remove only defects
   introduced by this checkpoint. Never weaken a test, requirement, diagnostic, safety boundary, or
   map validation rule to pass.
8. **Deliver and record.** Follow the root `AGENTS.md` Git delivery and Google Docs paired-tab
   lifecycle exactly. Reverify after the final rebase and prove the commit exists on the required
   remote branch. Replace the exact descriptions-tab claim suffix with the required three-sentence
   `Implemented` description, then highlight the main-tab specification from its first non-whitespace
   character through its final character without highlighting the separator space or changing its ID
   tag. Explicitly count the sentences and verify both tabs through fresh readback.

## Document completion gate

Do not return `Done.` unless all of these are true:

- this worker's descriptions-tab claim suffix no longer exists;
- exactly one completion description follows the matching immutable ID on the descriptions tab;
- the description contains exactly three concise and truthful sentences, never fewer or more;
- the first description sentence begins with `Implemented`;
- the three sentences accurately cover the outcome, principal implementation or integration, and
  fresh verification with the delivered `origin/main` commit;
- the description suffix is black, nonbold, nonitalic, unhighlighted Urbanist at 11 pt and has no
  list marker, heading, link, or background color;
- the main-tab checkpoint text is byte-for-byte unchanged;
- the complete main specification from its first non-whitespace character through its final
  character uses the configured completion background highlight, while the separator space and
  entire ID tag remain unhighlighted;
- every affected module map and required global relationship is current in the delivered commit;
- the codebase-map validator passes after the final rebase and immediately before push;
- the deterministic verifier and every checkpoint-specific proof pass on the final rebased source,
  with the exact commands and results recorded in `execution.md`;
- fresh readback of both configured tabs confirms the IDs, texts, sentence count, positions,
  formatting, removed claim, immutable main text, complete specification highlight, unhighlighted
  separator space, and unhighlighted main ID.

## Autonomy loop

When evidence breaks an assumption, inspect the root cause, research the missing fact, revise the
internal plan, and continue. Retry transient tool, test, rebase, push, and document failures with a
bounded corrective change. Do not pause for routine choices, progress reports, or approval between
slices.

Stop only for a blocking condition defined by root `AGENTS.md`. Otherwise continue until every item
in its completion gate passes, then return exactly `Done.`
