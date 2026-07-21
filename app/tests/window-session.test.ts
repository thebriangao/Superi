import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  closeDesktopWindow,
  createDesktopWindow,
  focusDesktopWindow,
  listenDesktopWindowSession,
  moveDesktopWindowToMonitor,
  parseDesktopWindowSnapshot,
  reopenDesktopWindow,
  setDesktopWindowFullscreen,
  undoDesktopWindowPlacement,
  updateDesktopWindowWorkspace,
} from "../src/window-session.ts";

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const snapshot = {
  schemaVersion: 1,
  revision: 7,
  phase: "ready",
  persistencePhase: "ready",
  nativeViewportOwner: "main",
  lastFocusedLabel: "workspace-1",
  recoveryNote: null,
  failure: null,
  recentlyClosedCount: 1,
  monitors: [
    {
      id: "monitor:primary",
      name: "Primary",
      positionX: 0,
      positionY: 0,
      physicalWidth: 2560,
      physicalHeight: 1440,
      scaleFactor: 2,
      primary: true,
    },
  ],
  windows: [
    {
      label: "main",
      title: "Superi",
      workspace: "editing",
      primary: true,
      focused: false,
      fullscreen: false,
      monitorId: "monitor:primary",
      x: 20,
      y: 30,
      width: 1100,
      height: 720,
      canUndoPlacement: false,
      canClose: false,
    },
    {
      label: "workspace-1",
      title: "Superi Workspace 1",
      workspace: "color",
      primary: false,
      focused: true,
      fullscreen: true,
      monitorId: "monitor:primary",
      x: 120,
      y: 130,
      width: 900,
      height: 700,
      canUndoPlacement: true,
      canClose: true,
    },
  ],
};

test("window snapshots validate exact recoverable state", () => {
  assert.deepEqual(parseDesktopWindowSnapshot(snapshot), snapshot);
  assert.throws(
    () => parseDesktopWindowSnapshot({ ...snapshot, phase: "invented" }),
    /window session snapshot/i,
  );
  assert.throws(
    () => parseDesktopWindowSnapshot({ ...snapshot, invented: true }),
    /window session snapshot/i,
  );
  assert.throws(
    () =>
      parseDesktopWindowSnapshot({
        ...snapshot,
        windows: [{ ...snapshot.windows[0], width: 0 }],
      }),
    /window session snapshot/i,
  );
});

test("window actions use one strict native command surface", async () => {
  const calls: Array<{ command: string; args: Record<string, unknown> }> = [];
  const host = {
    async invoke(command: string, args: Record<string, unknown>) {
      calls.push({ command, args });
      return snapshot;
    },
    async listen() {
      return () => {};
    },
  };

  await createDesktopWindow("color", host);
  await focusDesktopWindow("workspace-1", host);
  await setDesktopWindowFullscreen("workspace-1", false, host);
  await moveDesktopWindowToMonitor("workspace-1", "monitor:primary", host);
  await undoDesktopWindowPlacement("workspace-1", host);
  await updateDesktopWindowWorkspace("workspace-1", "delivery", host);
  await closeDesktopWindow("workspace-1", host);
  await reopenDesktopWindow(host);

  assert.deepEqual(calls, [
    { command: "desktop_window_create", args: { request: { workspace: "color" } } },
    { command: "desktop_window_focus", args: { request: { label: "workspace-1" } } },
    {
      command: "desktop_window_fullscreen",
      args: { request: { label: "workspace-1", fullscreen: false } },
    },
    {
      command: "desktop_window_move_to_monitor",
      args: { request: { label: "workspace-1", monitorId: "monitor:primary" } },
    },
    {
      command: "desktop_window_undo_placement",
      args: { request: { label: "workspace-1" } },
    },
    {
      command: "desktop_window_workspace_update",
      args: { request: { label: "workspace-1", workspace: "delivery" } },
    },
    { command: "desktop_window_close", args: { request: { label: "workspace-1" } } },
    { command: "desktop_window_reopen", args: {} },
  ]);
});

test("window events validate payloads and unsubscribe exactly once", async () => {
  let listener: ((event: { payload: unknown }) => void) | null = null;
  let unlistenCount = 0;
  const host = {
    async invoke() {
      return snapshot;
    },
    async listen(_event: string, next: (event: { payload: unknown }) => void) {
      listener = next;
      return () => {
        unlistenCount += 1;
      };
    },
  };
  const revisions: number[] = [];
  const unlisten = await listenDesktopWindowSession(
    (next) => revisions.push(next.revision),
    host,
  );
  listener?.({ payload: snapshot });
  unlisten();
  assert.deepEqual(revisions, [7]);
  assert.equal(unlistenCount, 1);
});

test("the real shell exposes window operations and auxiliary viewers fail closed", () => {
  const app = readFileSync(resolve(appRoot, "src/App.tsx"), "utf8");
  const panel = readFileSync(
    resolve(appRoot, "src/window-session-panel.tsx"),
    "utf8",
  );
  const viewport = readFileSync(
    resolve(appRoot, "src/native-viewport.tsx"),
    "utf8",
  );
  const tauriConfig = readFileSync(
    resolve(appRoot, "src-tauri/tauri.conf.json"),
    "utf8",
  );

  assert.match(app, /<WindowSessionPanel\s*\/>/);
  for (const action of [
    "createDesktopWindow",
    "focusDesktopWindow",
    "setDesktopWindowFullscreen",
    "moveDesktopWindowToMonitor",
    "undoDesktopWindowPlacement",
    "closeDesktopWindow",
    "reopenDesktopWindow",
  ]) {
    assert.match(panel, new RegExp(action));
  }
  assert.match(viewport, /getCurrentWebviewWindow\(\)\.label/);
  assert.match(viewport, /Native GPU presentation remains owned by the primary window/);
  assert.match(app, /waiting for its restored workspace identity/i);
  assert.match(tauriConfig, /"visible": false/);
});
