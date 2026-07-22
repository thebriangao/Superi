import assert from "node:assert/strict";
import test from "node:test";

import {
  projectHistoryPresentation,
  projectMutationLabel,
} from "../src/project-history.ts";

const active = {
  path: "/projects/alpha.superi",
  project_id: "project-alpha",
  project_revision: 17,
} as const;

const editorProject = {
  project_id: "project-alpha",
  project_revision: 17,
  undo_depth: 3,
  redo_depth: 1,
  next_undo: "consider_media_relink",
  next_redo: "upsert_extension",
} as const;

test("global history names the exact next engine transactions for one document", () => {
  const presentation = projectHistoryPresentation({
    active,
    editorProject,
    busy: false,
  });

  assert.equal(presentation.condition, "ready");
  assert.equal(presentation.documentLabel, "alpha.superi");
  assert.equal(presentation.projectId, "project-alpha");
  assert.equal(presentation.projectRevision, 17);
  assert.equal(presentation.sessionOnly, true);
  assert.deepEqual(presentation.undo, {
    command: "undo",
    depth: 3,
    mutationKind: "consider_media_relink",
    title: "Undo Media Relink",
    detail:
      "3 transactions are available to undo in alpha.superi. History ends when this project session closes.",
    enabled: true,
    disabledReason: null,
  });
  assert.deepEqual(presentation.redo, {
    command: "redo",
    depth: 1,
    mutationKind: "upsert_extension",
    title: "Redo Extension Update",
    detail:
      "1 transaction is available to redo in alpha.superi. History ends when this project session closes.",
    enabled: true,
    disabledReason: null,
  });
  assert.equal(presentation.closeUndoDepth, 3);
  assert.equal(presentation.closeRedoDepth, 1);
  assert.match(presentation.status, /alpha\.superi.*revision 17/i);
  assert.match(presentation.status, /session-only/i);
  assert.ok(Object.isFrozen(presentation));
  assert.ok(Object.isFrozen(presentation.undo));
  assert.ok(Object.isFrozen(presentation.redo));
});

test("busy and revision-lagged projects retain visibility but fail closed", () => {
  const busy = projectHistoryPresentation({
    active,
    editorProject,
    busy: true,
  });
  assert.equal(busy.condition, "busy");
  assert.equal(busy.undo.title, "Undo Media Relink");
  assert.equal(busy.undo.enabled, false);
  assert.equal(
    busy.undo.disabledReason,
    "Wait for the current project operation to finish.",
  );

  const synchronizing = projectHistoryPresentation({
    active,
    editorProject: { ...editorProject, project_revision: 16 },
    busy: false,
  });
  assert.equal(synchronizing.condition, "synchronizing");
  assert.equal(synchronizing.undo.depth, 3);
  assert.equal(synchronizing.closeUndoDepth, 3);
  assert.equal(synchronizing.undo.enabled, false);
  assert.equal(
    synchronizing.undo.disabledReason,
    "Wait for project transaction history to synchronize.",
  );
  assert.match(synchronizing.status, /revision 17.*synchroniz/i);
});

test("history coherence rejects missing next actions without hiding safe-close counts", () => {
  const presentation = projectHistoryPresentation({
    active,
    editorProject: { ...editorProject, next_undo: null },
    busy: false,
  });
  assert.equal(presentation.condition, "synchronizing");
  assert.equal(presentation.undo.depth, 3);
  assert.equal(presentation.closeUndoDepth, 3);
  assert.equal(presentation.undo.enabled, false);
  assert.equal(presentation.undo.mutationKind, null);

  const unexpectedEmptyAction = projectHistoryPresentation({
    active,
    editorProject: {
      ...editorProject,
      undo_depth: 0,
      next_undo: "compound",
    },
    busy: false,
  });
  assert.equal(unexpectedEmptyAction.condition, "synchronizing");
  assert.equal(unexpectedEmptyAction.undo.enabled, false);
});

test("future mutation values remain visible as safe generic project changes", () => {
  const presentation = projectHistoryPresentation({
    active,
    editorProject: { ...editorProject, next_undo: "future_mutation" },
    busy: false,
  });
  assert.equal(presentation.condition, "ready");
  assert.equal(presentation.undo.mutationKind, "unknown");
  assert.equal(presentation.undo.title, "Undo Project Change");
  assert.equal(presentation.undo.enabled, true);
  assert.equal(projectMutationLabel("project_settings"), "Project Settings");
  assert.equal(projectMutationLabel("future_mutation"), "Project Change");
});

test("no active document never exposes stale editor history", () => {
  const presentation = projectHistoryPresentation({
    active: null,
    editorProject,
    busy: false,
  });
  assert.equal(presentation.condition, "no_project");
  assert.equal(presentation.documentLabel, null);
  assert.equal(presentation.undo.depth, 0);
  assert.equal(presentation.redo.depth, 0);
  assert.equal(presentation.closeUndoDepth, 0);
  assert.equal(presentation.closeRedoDepth, 0);
  assert.equal(presentation.undo.enabled, false);
  assert.equal(presentation.undo.disabledReason, "Open a project first.");
});
