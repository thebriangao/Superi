import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import {
  EDITOR_WORKSPACE_IDS,
  createEditorStateRequest,
  projectAudioTrack,
} from "../src/editor-project.ts";

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

function read(path: string): string {
  return readFileSync(path, "utf8");
}

test("five professional workspaces are exact views over the existing application owner", () => {
  assert.deepEqual(EDITOR_WORKSPACE_IDS, [
    "editing",
    "compositing",
    "color",
    "audio",
    "delivery",
  ]);

  const app = read(resolve(appRoot, "src/App.tsx"));
  const applicationContext = read(
    resolve(appRoot, "src/application-context.tsx"),
  );
  const workspaces = read(resolve(appRoot, "src/editor-workspaces.tsx"));
  const packageJson = JSON.parse(read(resolve(appRoot, "package.json")));

  for (const workspace of EDITOR_WORKSPACE_IDS) {
    assert.match(app, new RegExp(`id: "${workspace}"`));
    assert.match(app, new RegExp(`application\\.route\\.${workspace}`));
  }
  assert.equal((app.match(/<ApplicationProvider\b/g) ?? []).length, 1);
  assert.doesNotMatch(app, /EditorProjectProvider|ProjectStateProvider/);
  assert.match(applicationContext, /superi\.editor\.state\.get/);
  assert.match(applicationContext, /superi\.project\.state\.changed/);
  assert.match(applicationContext, /classifyDesktopTransportError/);
  assert.doesNotMatch(
    workspaces,
    /createContext|useReducer|useState|useSuperiApi|DesktopSuperiTransport|@tauri-apps/,
  );
  assert.equal((workspaces.match(/<NativeViewport\b/g) ?? []).length, 3);
  assert.match(workspaces, /<SourceMonitor \/>/);
  assert.match(workspaces, /<NativeViewport role="program" label="Program" \/>/);
  assert.match(workspaces, /<NativeViewport role="composite" label="Composite" \/>/);
  assert.match(workspaces, /<NativeViewport role="color" label="Color" \/>/);
  assert.match(packageJson.scripts.test, /editor-workspaces\.test\.ts/);
});

test("editor requests use one explicit public transaction identity", () => {
  assert.deepEqual(createEditorStateRequest("desktop-project-17"), {
    transaction_id: "desktop-project-17",
  });
  assert.throws(() => createEditorStateRequest("  "), /transaction identity/i);
});

test("timeline track gestures route through the application command owner", () => {
  const applicationContext = read(
    resolve(appRoot, "src/application-context.tsx"),
  );
  const workspaces = read(resolve(appRoot, "src/editor-workspaces.tsx"));
  const timeline = read(resolve(appRoot, "src/timeline-workspace.tsx"));

  assert.match(applicationContext, /executeProjectActions/);
  assert.match(applicationContext, /superi\.project\.command\.execute/);
  assert.match(applicationContext, /expected_project_revision/);
  assert.match(workspaces, /action: "mutate_tracks"/);
  assert.match(workspaces, /mutateTracks=\{mutateTracks\}/);
  for (const operation of [
    "create",
    "delete",
    "rename",
    "set_height",
    "reorder",
    "set_targeted",
    "set_locked",
    "set_sync_locked",
    "set_muted",
    "set_solo",
    "set_enabled",
  ]) {
    assert.match(timeline, new RegExp(`operation: "${operation}"`));
  }
  assert.doesNotMatch(
    timeline,
    /useSuperiApi|DesktopSuperiTransport|@tauri-apps\/api/,
  );
});

test("audio projection preserves sample timing, channel order, routing, and continuity", () => {
  const track = {
    timeline_id: "timeline.main",
    track_id: "audio.dialogue",
    sample_rate: 48_000,
    source_channels: ["dialogue.left", "dialogue.right"],
    destination: { kind: "track" as const, track_id: "bus.dialogue" },
    destination_channels: ["bus.left", "bus.right"],
    routes: [
      {
        source: "dialogue.left",
        target: { kind: "channel" as const, channel: "bus.left" },
      },
      {
        source: "dialogue.right",
        target: { kind: "muted" as const },
      },
    ],
    clip_count: 3,
    continuity: {
      status: "audited" as const,
      uninterrupted_record_coverage: false,
      seams: [
        {
          left_clip_id: "clip.1",
          right_clip_id: "clip.2",
          record: { kind: "gap" as const, sample_count: 240 },
          source: {
            kind: "discontinuous" as const,
            expected: 96_000,
            actual: 96_240,
          },
        },
      ],
    },
  };

  const projection = projectAudioTrack(track);
  assert.deepEqual(projection, track);
  assert.equal(projection.sample_rate, 48_000);
  assert.deepEqual(projection.source_channels, [
    "dialogue.left",
    "dialogue.right",
  ]);
  assert.deepEqual(projection.destination_channels, ["bus.left", "bus.right"]);
  assert.deepEqual(projection.routes[1].target, { kind: "muted" });
  assert.deepEqual(projection.continuity.seams[0].record, {
    kind: "gap",
    sample_count: 240,
  });
  assert.ok(Object.isFrozen(projection));
  assert.ok(Object.isFrozen(projection.source_channels));
  assert.ok(Object.isFrozen(projection.routes));
});
