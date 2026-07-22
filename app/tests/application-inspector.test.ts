import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

import {
  createApplicationInspectorModel,
  type ApplicationInspectorInput,
} from "../src/application-inspector.ts";

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function input(
  overrides: Partial<ApplicationInspectorInput> = {},
): ApplicationInspectorInput {
  return {
    routeTitle: "Editing",
    focusedPanelTitle: "Editing workspace",
    visiblePanelCount: 3,
    hiddenPanelCount: 1,
    workspaceRevision: 17,
    selectionSummary: ["timeline / clip-7 / revision 4"],
    editorProject: {
      status: "ready",
      transactionId: "editor-state-9",
      commandSequence: 22,
      failure: null,
      snapshot: {
        schema_version: "1.9.0",
        project: {
          project_id: "project-1",
          root_timeline_id: "timeline-1",
          project_revision: 8,
          history_capacity: 64,
          undo_depth: 3,
          redo_depth: 1,
          next_undo: "Trim clip",
          next_redo: "Move marker",
          semantic_hash_algorithm: "sha256",
          semantic_hash_format_revision: 1,
          semantic_hash: "abcdef0123456789",
        },
        playback: {
          status: "attached",
          pending_command: false,
          latest: {
            mode: "paused",
            epoch: 6,
            degradation: [],
            failure: null,
          },
        },
      },
    },
    notificationState: {
      nextSequence: 4,
      notifications: [
        {
          id: "notice-3",
          sequence: 3,
          title: "Workspace restored",
          message: "The saved panel arrangement is active.",
          tone: "success",
        },
      ],
    },
    commandFailure: null,
    ...overrides,
  } as ApplicationInspectorInput;
}

test("shared inspector projects exact engine, metadata, history, and diagnostic groups", () => {
  const source = input();
  const model = createApplicationInspectorModel(source);

  assert.equal(model.engine.condition, "ready");
  assert.equal(model.engine.label, "Playback paused");
  assert.deepEqual(
    model.groups.map((group) => group.id),
    ["inspector", "metadata", "history", "diagnostics"],
  );
  assert.deepEqual(
    model.groups[0]?.rows.map((row) => [row.label, row.value]),
    [
      ["Route", "Editing"],
      ["Focused panel", "Editing workspace"],
      ["Visible panels", "3"],
      ["Hidden panels", "1"],
      ["Selection", "timeline / clip-7 / revision 4"],
    ],
  );
  assert.match(model.groups[1]?.rows[3]?.value ?? "", /sha256/);
  assert.deepEqual(
    model.groups[2]?.rows.slice(0, 4).map((row) => row.value),
    ["3", "1", "Trim clip", "Move marker"],
  );
  assert.equal(model.groups[3]?.rows.at(-1)?.value, "Workspace restored");
  assert.ok(Object.isFrozen(model));
  assert.ok(Object.isFrozen(model.groups));
  assert.ok(Object.isFrozen(model.groups[0]?.rows));
  assert.equal(source.editorProject.snapshot?.project.project_revision, 8);
});

test("engine state stays honest for detached, pending, degraded, and failed observations", () => {
  const detached = createApplicationInspectorModel(
    input({
      editorProject: {
        ...input().editorProject,
        snapshot: {
          ...input().editorProject.snapshot!,
          playback: { status: "detached" },
        },
      },
    }),
  );
  assert.deepEqual(detached.engine, {
    condition: "degraded",
    label: "Playback detached",
    detail: "The editor snapshot is valid, but no playback owner is attached.",
  });

  const pending = createApplicationInspectorModel(
    input({
      editorProject: {
        ...input().editorProject,
        snapshot: {
          ...input().editorProject.snapshot!,
          playback: { status: "attached", pending_command: true, latest: null },
        },
      },
    }),
  );
  assert.equal(pending.engine.condition, "working");
  assert.equal(pending.engine.label, "Playback command pending");

  const degraded = createApplicationInspectorModel(
    input({
      editorProject: {
        ...input().editorProject,
        snapshot: {
          ...input().editorProject.snapshot!,
          playback: {
            status: "attached",
            pending_command: false,
            latest: {
              mode: "playing",
              epoch: 7,
              degradation: ["viewport_unavailable", "audio_unavailable"],
              failure: null,
            },
          },
        },
      },
    }),
  );
  assert.equal(degraded.engine.condition, "degraded");
  assert.match(degraded.engine.detail, /viewport_unavailable/);

  const failed = createApplicationInspectorModel(
    input({
      editorProject: {
        ...input().editorProject,
        status: "failed",
        snapshot: null,
      },
      commandFailure: "The selected panel is unavailable.",
    }),
  );
  assert.equal(failed.engine.condition, "attention");
  assert.equal(failed.engine.label, "Editor state failed");
  assert.equal(
    failed.groups[3]?.rows.some((row) => row.value.includes("selected panel")),
    true,
  );
});

test("production panel is dockable on every route and delegates to existing owners", () => {
  const app = readFileSync(resolve(appRoot, "src/App.tsx"), "utf8");
  const panel = readFileSync(
    resolve(appRoot, "src/application-inspector-panel.tsx"),
    "utf8",
  );

  assert.match(app, /id: "application\.inspector"/);
  assert.equal(
    [...app.matchAll(/panelIds: \[[^\]]*"application\.inspector"[^\]]*\]/gs)].length,
    6,
  );
  assert.match(app, /application\.panel\.application\.inspector\.toggle/);
  assert.match(panel, /createApplicationInspectorModel/);
  assert.match(panel, /refreshEditorProject/);
  assert.match(panel, /application\.route\.system/);
  assert.match(panel, /type: "clear_selection"/);
  assert.match(panel, /type: "restore_cleared_selection"/);
  assert.match(panel, /Restore cleared selection/);
  assert.doesNotMatch(panel, /@tauri-apps|useSuperiApi|\.request\(/);
});
