---
name: superi-execution
description: Use when an assigned Superi checkpoint has a research-backed internal plan and is ready to be built autonomously.
---

# Superi Execution

## Principle

Finish the real checkpoint, not an approximation. Working behavior through its actual consumer,
fresh verification, remote delivery, and checklist evidence define completion.

## Execute

1. **Synchronize safely.** Reconfirm the exact timestamped claim paragraph and its formatting,
   inspect the worktree, fetch current remote state, and integrate without discarding any existing
   work. Never delete code you did not write.
2. **Load the plan.** Turn the `superi-planning` output into a live task list. Keep one slice active,
   but retain ownership of the entire checkpoint and its necessary dependencies. Refuse execution
   if the map validator did not pass, the global index and every module map were not read in full, or
   the selected raw-file inventory was not read through EOF and recorded.
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
6. **Refresh the maps.** Run `superi-mapping` after source edits and tests. Use the Git diff to find
   every directly affected module, then update each module map from the changed raw files and related
   interfaces read through EOF. Update consumer maps and the global index when contracts, ownership,
   layering, public flow, or status changed. Recompute hashes and file counts, and never change only
   metadata while leaving inaccurate prose.
7. **Review the whole result.** Inspect the source and map diff line by line against the checkpoint,
   research, plan, architectural boundaries, failure modes, and neighboring work. Remove only defects
   introduced by this checkpoint. Never weaken a test, requirement, diagnostic, safety boundary, or
   map validation rule to pass.
8. **Deliver and record.** Follow the root `AGENTS.md` Git delivery and Google Docs note lifecycle
   exactly. Reverify after the final rebase and prove the commit exists on the required remote
   branch. Leave the native checkbox unchanged, delete the exact active claim paragraph, and replace
   it in the same position with exactly three concise sentences in the exact configured color, font,
   and size. Explicitly count the sentences and verify the final document through a fresh readback.

## Document completion gate

Do not return `Done.` unless all of these are true:

- this worker's claim paragraph no longer exists;
- exactly one completion description appears directly below the assigned checkpoint;
- the description contains exactly three concise and truthful sentences, never fewer or more;
- the three sentences accurately cover the outcome, principal implementation or integration, and
  fresh verification with the delivered `origin/main` commit;
- the entire description uses the exact configured hex color, Urbanist font, and 9 pt size;
- the description has no list marker, checkbox, bold, italics, heading, link, highlight, or
  background color;
- the native checkpoint checkbox remains exactly as found;
- every affected module map and required global relationship is current in the delivered commit;
- the codebase-map validator passes after the final rebase and immediately before push;
- a fresh document readback confirms the text, sentence count, position, formatting, removed claim,
  and unchanged checkbox.

## Autonomy loop

When evidence breaks an assumption, inspect the root cause, research the missing fact, revise the
internal plan, and continue. Retry transient tool, test, rebase, push, and document failures with a
bounded corrective change. Do not pause for routine choices, progress reports, or approval between
slices.

Stop only for a blocking condition defined by root `AGENTS.md`. Otherwise continue until every item
in its completion gate passes, then return exactly `Done.`
