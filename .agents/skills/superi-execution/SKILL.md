---
name: superi-execution
description: Use when an assigned Superi checkpoint has a research-backed internal plan and is ready to be built autonomously.
---

# Superi Execution

## Principle

Finish the real checkpoint, not an approximation. Working behavior through its actual consumer,
fresh verification, remote delivery, and checklist evidence define completion.

## Execute

1. **Reconfirm owner state.** The checkpoint owner reconfirms its exact timestamped claim suffix,
   re-reads the immutable main specification, inspects the worktree, fetches current remote state, and
   integrates without discarding existing work. The owner reads `planning.md` and `execution.md`,
   confirms the recorded reading evidence and base revision, and refuses execution when the planning
   gate is incomplete. Never delete code another worker wrote.
2. **Staff the execution plan.** The owner launches project-scoped `superi-tier2` specialists from
   the approved execution decomposition. Give each one a detailed assignment with exact path
   ownership, mandatory reads, interfaces, proof, peers, and handoff. Use one writer per path at a
   time. Run disjoint slices concurrently, serialize dependent or overlapping slices, and route
   interface decisions directly between affected peers. Tier 2 specialists never pull, rebase,
   commit, push, or touch Google Docs.
3. **Prove before changing.** Each implementation specialist writes the smallest real test first and
   runs it to confirm the expected failure. For bugs, reproduce the defect. For existing untested
   behavior, capture it before modifying it. Then implement the smallest complete slice and rerun the
   proof. Record commands, results, files, and remaining risks in its handoff and the assigned section
   of `execution.md` when authorized.
4. **Integrate completely.** Specialists follow current interfaces and dependency direction and
   include real error paths, migrations, feature configurations, diagnostics, and documentation.
   They coordinate directly before changing shared contracts. No placeholders, mocked success, dead
   interfaces, hidden manual steps, or concurrent edits to one path.
5. **Verify in widening rings.** Focused test specialists verify individual slices, then an
   integration specialist runs relevant subsystem and consumer suites. The checkpoint owner runs the
   final
   `python3 .agents/skills/superi-execution/scripts/verify_checkpoint.py --base <base-revision>` from
   the root, using `--full` for broad infrastructure changes or uncertain selection, and confirms
   every checkpoint-specific proof from `planning.md`. Keep bulky logs inside specialist threads and
   record exact commands, exit status, concise results, and artifact locations in `execution.md`.
   Headless proof is mandatory. Use computer control only when headless proof is impossible, and
   verify resulting engine or project state instead of trusting appearance alone.
6. **Refresh the maps.** Assign tier 2 map specialists after source edits and tests. Use the Git diff
   to identify every directly affected module, update each map from changed raw files and related
   interfaces read through EOF, and update consumers and the global index when contracts, ownership,
   layering, public flow, or status changed. The owner reviews the complete map diff, reconciles
   cross-module statements, recomputes hashes and file counts, and runs the final validator.
7. **Review independently.** Assign at least one fresh tier 2 integration reviewer that did not own
   the implementation. It reads every changed file through EOF and inspects the complete source,
   test, and map diff against the checkpoint, research, plan, architectural boundaries, failure modes,
   and neighboring work, then reports evidence and defects without editing unless explicitly assigned
   a correction. The owner personally reads the final diff line by line, every critical interface and
   relevant test through EOF, all disputed or high-risk regions, and both plan files. Delegate
   substantive corrections, then repeat focused and final verification. The owner may make only small
   integration corrections that do not require new architecture or broad context.
8. **Deliver and record as owner.** Only the checkpoint owner performs the final fetch and safe
   integration, commit, rebase, push, remote verification, and Google Docs paired-tab lifecycle.
   Reverify the final rebased source and prove the commit exists on the required remote branch.
   Replace the exact descriptions-tab claim with the required three-sentence `Implemented`
   description, highlight only the main specification text, explicitly count the sentences, and
   verify both tabs through fresh readback.

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

When evidence breaks an assumption, the owner routes root-cause inspection and missing research to
tier 2 specialists, has the lead planner revise `planning.md`, has the execution decomposer reconcile
`execution.md`, and continues. Retry transient tool, test, rebase, push, and document failures with a
bounded corrective change. Do not pause for routine choices, progress reports, or approval between
slices. Reuse idle specialists with follow-up assignments and queue additional specialists whenever
more parallel work would materially reduce owner context or improve proof.

Stop only for a blocking condition defined by root `AGENTS.md`. Otherwise continue until every item
in its completion gate passes, then return exactly `Done.`
