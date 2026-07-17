---
name: superi-execution
description: Use when an assigned Superi checkpoint has a research-backed internal plan and is ready to be built autonomously.
---

# Superi Execution

## Principle

Finish the real checkpoint, not an approximation. Working behavior through its actual consumer,
fresh verification, remote delivery, and checklist evidence define completion.

## Execute

The one permitted tier 2 research planner has already produced both plans. From this point through
delivery, the checkpoint owner works alone and must not spawn any agent.

1. **Reconfirm owner state.** Reconfirm the exact timestamped claim suffix, re-read the immutable
   main specification, inspect the worktree, fetch current remote state, and integrate without
   discarding existing work. Read `planning.md` and `execution.md` in full and confirm the base
   revision, evidence record, architecture, and proof plan. Never delete code another worker wrote.
2. **Read the implementation yourself.** Use the plans to select scope, then personally read every
   file that may change plus relevant public interfaces, callers, consumers, manifests, schemas,
   tests, fixtures, and governing documents through EOF. Do not rely on the planner's summary as a
   substitute for understanding the current code. Initial codebase-map ingestion belongs to the
   planner and does not need to be repeated.
3. **Prove before changing.** Write the smallest real test first and run it to confirm the expected
   failure. For bugs, reproduce the defect. For existing untested behavior, capture it before
   modifying it. Implement the smallest complete slice, rerun the proof, and maintain exact commands,
   results, changed files, decisions, and remaining risks in `execution.md`.
4. **Integrate completely.** Follow current interfaces and dependency direction. Include real error
   paths, migrations, feature configurations, diagnostics, and documentation required by the
   checkpoint. No placeholders, mocked success, dead interfaces, hidden manual steps, or unrelated
   rewrites.
5. **Verify in widening rings.** Run focused tests after every slice, then relevant subsystem and
   real-consumer suites. Run
   `python3 .agents/skills/superi-execution/scripts/verify_checkpoint.py --base <base-revision>` from
   the root, using `--full` for broad infrastructure changes or uncertain selection, and run every
   checkpoint-specific proof from `planning.md`. Record concise command outcomes and artifact paths
   in `execution.md`. Headless proof is mandatory. Use computer control only when headless proof is
   impossible, then verify resulting engine or project state instead of trusting appearance alone.
6. **Refresh the maps yourself.** Use the Git diff and `superi-mapping` commands to identify affected
   modules. Update each affected map from the changed code and interfaces, update consumers and the
   global index when contracts, ownership, layering, public flow, or status changed, recompute hashes
   and counts, and run the validator. The planner never edits maps.
7. **Review the whole result yourself.** Read every changed file and the complete source, test, and
   map diff line by line against the checkpoint, plans, architectural boundaries, failure modes, and
   neighboring work. Re-read every critical interface and relevant test through EOF. Correct defects,
   rerun focused proof, rerun final verification, and never delegate review or debugging.
8. **Deliver and record.** Perform the final fetch, safe integration, commit, rebase, push, remote
   verification, and Google Docs paired-tab lifecycle. Reverify the final rebased source and prove
   the commit exists on the required remote branch. Replace the exact descriptions-tab claim with the
   required three-sentence `Implemented` description, highlight only the main specification text,
   explicitly count the sentences, and verify both tabs through fresh readback.

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

When evidence breaks an assumption, inspect the root cause, perform the missing research yourself,
update the execution evidence, revise the approach, and continue. Never spawn another agent. Retry
transient tool, test, rebase, push, and document failures with a bounded corrective change. Do not
pause for routine choices, progress reports, or approval between slices.

Stop only for a blocking condition defined by root `AGENTS.md`. Otherwise continue until every item
in its completion gate passes, then return exactly `Done.`
