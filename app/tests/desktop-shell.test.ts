import assert from "node:assert/strict";
import test from "node:test";

import {
  decideDesktopClose,
  desktopDocumentTitle,
  desktopShellIntentAutomationId,
  getDesktopShellSnapshot,
  partitionDesktopDrop,
  syncDesktopShell,
} from "../src/desktop-shell.ts";

test("desktop shell intents retain stable automation identities", () => {
  assert.equal(
    desktopShellIntentAutomationId({ kind: "open_command_palette" }),
    "application.command_palette.open",
  );
  assert.equal(
    desktopShellIntentAutomationId({
      kind: "open_recent",
      path: "/projects/alpha.superi",
    }),
    "desktop.file.open_recent:%2Fprojects%2Falpha.superi",
  );
  assert.equal(
    desktopShellIntentAutomationId({
      kind: "open_workspace",
      route_id: "color",
    }),
    "application.route.color",
  );
  assert.equal(
    desktopShellIntentAutomationId({ kind: "request_close", reason: "quit" }),
    "desktop.application.quit",
  );
});

test("desktop shell separates one project document from native media drops", () => {
  assert.deepEqual(partitionDesktopDrop(["/projects/alpha.superi"]), {
    kind: "project",
    path: "/projects/alpha.superi",
  });
  assert.deepEqual(
    partitionDesktopDrop(["/media/a.mov", "/media/stills"]),
    { kind: "media", paths: ["/media/a.mov", "/media/stills"] },
  );
  assert.deepEqual(
    partitionDesktopDrop(["/projects/alpha.superi", "/media/a.mov"]),
    { kind: "ambiguous" },
  );
  assert.deepEqual(
    partitionDesktopDrop(["/projects/a.superi", "/projects/b.SUPERI"]),
    { kind: "ambiguous" },
  );
});

test("safe close blocks live operations and exposes session history loss", () => {
  assert.equal(
    decideDesktopClose({ busy: true, active: true, undoDepth: 4, redoDepth: 0 }),
    "block_busy",
  );
  assert.equal(
    decideDesktopClose({ busy: false, active: true, undoDepth: 4, redoDepth: 0 }),
    "confirm_history",
  );
  assert.equal(
    decideDesktopClose({ busy: false, active: true, undoDepth: 0, redoDepth: 3 }),
    "confirm_history",
  );
  assert.equal(
    decideDesktopClose({ busy: false, active: true, undoDepth: 0, redoDepth: 0 }),
    "save_and_close",
  );
  assert.equal(
    decideDesktopClose({ busy: false, active: false, undoDepth: 0, redoDepth: 0 }),
    "close",
  );
});

test("document title exposes native document identity without leaking paths", () => {
  assert.equal(desktopDocumentTitle(null), "Superi");
  assert.equal(
    desktopDocumentTitle({
      path: "/projects/feature/alpha.superi",
      project_id: "project-alpha",
      project_revision: 17,
    }),
    "alpha.superi [r17] - Superi",
  );
  assert.equal(
    desktopDocumentTitle({
      path: "C:\\Projects\\beta.superi",
      project_id: "project-beta",
      project_revision: 2,
    }),
    "beta.superi [r2] - Superi",
  );
});

test("native sequence resumes after a webview reload before the next sync", async () => {
  const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
  const previousWindow = Object.getOwnPropertyDescriptor(globalThis, "window");
  Object.defineProperty(globalThis, "window", {
    configurable: true,
    value: {
      __TAURI_INTERNALS__: {
        invoke: async (command: string, args: Record<string, unknown> = {}) => {
          calls.push({ command, args });
          if (command === "desktop_shell_snapshot") {
            return {
              revision: 7,
              client_sequence: 41,
              active: null,
              recent_paths: [],
              undo_depth: 0,
              redo_depth: 0,
              next_undo: null,
              next_redo: null,
              busy: false,
              workspace: {
                active_route_id: "editing",
                hidden_panel_ids: [],
                focused_panel_id: null,
                panel_layouts: [],
              },
              keyboard_shortcuts: {
                schema_version: 1,
                overrides: [
                  {
                    command_id: "application.route.editing",
                    shortcut: "mod+e",
                  },
                ],
              },
              failure: null,
            };
          }
          return {
            revision: 8,
            ...(args.sync as Record<string, unknown>),
            failure: null,
          };
        },
      },
    },
  });

  try {
    await getDesktopShellSnapshot();
    await syncDesktopShell({
      active: null,
      recent_paths: [],
      undo_depth: 0,
      redo_depth: 0,
      next_undo: null,
      next_redo: null,
      busy: false,
      workspace: {
        active_route_id: "editing",
        hidden_panel_ids: [],
        focused_panel_id: null,
        panel_layouts: [],
      },
      keyboard_shortcuts: {
        schema_version: 1,
        overrides: [
          {
            command_id: "application.route.editing",
            shortcut: "mod+e",
          },
        ],
      },
    });
    assert.equal(
      (calls.at(-1)?.args.sync as { client_sequence: number }).client_sequence,
      42,
    );
    assert.deepEqual(
      (calls.at(-1)?.args.sync as { keyboard_shortcuts: unknown })
        .keyboard_shortcuts,
      {
        schema_version: 1,
        overrides: [
          {
            command_id: "application.route.editing",
            shortcut: "mod+e",
          },
        ],
      },
    );
    assert.deepEqual(
      calls.at(-1)?.args.sync,
      {
        active: null,
        recent_paths: [],
        undo_depth: 0,
        redo_depth: 0,
        next_undo: null,
        next_redo: null,
        busy: false,
        workspace: {
          active_route_id: "editing",
          hidden_panel_ids: [],
          focused_panel_id: null,
          panel_layouts: [],
        },
        keyboard_shortcuts: {
          schema_version: 1,
          overrides: [
            {
              command_id: "application.route.editing",
              shortcut: "mod+e",
            },
          ],
        },
        client_sequence: 42,
      },
    );
  } finally {
    if (previousWindow === undefined) {
      Reflect.deleteProperty(globalThis, "window");
    } else {
      Object.defineProperty(globalThis, "window", previousWindow);
    }
  }
});
