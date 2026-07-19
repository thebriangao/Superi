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
  const playbackControls = read(
    resolve(appRoot, "src/playback-controls.tsx"),
  );
  const packageJson = JSON.parse(read(resolve(appRoot, "package.json")));

  for (const workspace of EDITOR_WORKSPACE_IDS) {
    assert.match(app, new RegExp(`id: "${workspace}"`));
    assert.match(app, new RegExp(`application\\.route\\.${workspace}`));
  }
  assert.equal((app.match(/<ApplicationProvider\b/g) ?? []).length, 1);
  assert.doesNotMatch(app, /EditorProjectProvider|ProjectStateProvider/);
  assert.match(applicationContext, /superi\.editor\.state\.get/);
  assert.match(applicationContext, /executeProjectActions/);
  assert.match(applicationContext, /superi\.project\.command\.execute/);
  assert.match(applicationContext, /superi\.playback\.transport\.execute/);
  assert.match(applicationContext, /executePlaybackTransport/);
  assert.match(applicationContext, /expected_project_revision/);
  assert.match(applicationContext, /superi\.project\.state\.changed/);
  assert.match(applicationContext, /classifyDesktopTransportError/);
  assert.doesNotMatch(
    workspaces,
    /createContext|useReducer|useState|useSuperiApi|DesktopSuperiTransport|@tauri-apps/,
  );
  assert.equal((workspaces.match(/<NativeViewport\b/g) ?? []).length, 3);
  assert.match(workspaces, /<SourceMonitor\b/);
  assert.match(workspaces, /onSnapshotChange=\{setSourceMonitor\}/);
  assert.match(workspaces, /onExecuteProjectCommand=\{executeProjectCommand\}/);
  assert.match(applicationContext, /superi\.project\.command\.execute/);
  assert.match(workspaces, /role="program"[\s\S]*?label="Program"/);
  assert.match(workspaces, /<PlaybackControls \/>/);
  assert.match(workspaces, /<NativeViewport role="composite" label="Composite" \/>/);
  assert.match(workspaces, /<NativeViewport role="color" label="Color" \/>/);
  assert.match(workspaces, /executeProjectActions={executeProjectActions}/);
  for (const action of [
    "play",
    "pause",
    "stop",
    "set_loop",
    "set_rate",
    "set_direction",
    "step_frames",
    "inspect",
  ]) {
    assert.match(playbackControls, new RegExp(`action: "${action}"`));
  }
  assert.match(playbackControls, /playbackActionForKey\("j"/);
  assert.match(playbackControls, /playbackActionForKey\("l"/);
  assert.match(playbackControls, /Comparison state/);
  assert.match(playbackControls, /Audio synchronization/);
  assert.match(playbackControls, /Degraded behavior/);
  assert.doesNotMatch(
    playbackControls,
    /useSuperiApi|DesktopSuperiTransport|@tauri-apps\/api|\binvoke\b/,
  );
  assert.match(packageJson.scripts.test, /editor-workspaces\.test\.ts/);
  assert.match(packageJson.scripts.test, /playback-transport\.test\.ts/);
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

test("timeline marker gestures share the revision-fenced application command owner", () => {
  const applicationContext = read(
    resolve(appRoot, "src/application-context.tsx"),
  );
  const workspaces = read(resolve(appRoot, "src/editor-workspaces.tsx"));
  const timeline = read(resolve(appRoot, "src/timeline-workspace.tsx"));

  assert.match(applicationContext, /executeProjectActions/);
  assert.match(applicationContext, /expected_project_revision/);
  assert.match(workspaces, /action: "mutate_markers"/);
  assert.match(workspaces, /mutateMarkers=\{mutateMarkers\}/);
  for (const operation of [
    "create",
    "set_range",
    "set_label",
    "set_flag",
    "set_note",
    "remove",
  ]) {
    assert.match(timeline, new RegExp(`operation: "${operation}"`));
  }
  assert.match(timeline, /Reverse marker change/);
  assert.match(timeline, /availableAtRevision/);
  assert.match(timeline, /markerCreateMutation/);
  assert.doesNotMatch(
    timeline,
    /superi\.project\.command\.execute|useSuperiApi|DesktopSuperiTransport|@tauri-apps\/api/,
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

test("editorial feedback crosses the existing application owner into viewers and meters", () => {
  const applicationContext = read(
    resolve(appRoot, "src/application-context.tsx"),
  );
  const workspaces = read(resolve(appRoot, "src/editor-workspaces.tsx"));
  const timeline = read(resolve(appRoot, "src/timeline-workspace.tsx"));
  const nativeViewport = read(resolve(appRoot, "src/native-viewport.tsx"));
  const styles = read(resolve(appRoot, "src/styles.css"));

  assert.match(applicationContext, /editorialFeedback/);
  assert.match(applicationContext, /setEditorialFeedback/);
  assert.match(timeline, /projectTimelineEditorialFeedback/);
  assert.match(timeline, /onEditorialFeedback/);
  assert.match(workspaces, /feedback=\{editorialFeedback\?\.source \?\? null\}/);
  assert.match(workspaces, /feedback=\{editorialFeedback\?\.program \?\? null\}/);
  assert.match(workspaces, /<EditorialAudioMeters/);
  assert.match(workspaces, /onEditorialFeedback=\{setEditorialFeedback\}/);
  assert.match(nativeViewport, /TimelineViewerFeedback/);
  assert.match(nativeViewport, /initialViewerNavigation/);
  assert.match(nativeViewport, /requestFullscreen/);
  assert.match(nativeViewport, />Fit</);
  assert.match(nativeViewport, />1:1</);
  assert.match(nativeViewport, />Cinema</);
  assert.match(nativeViewport, />Fullscreen</);
  assert.match(nativeViewport, /data-external-display-intent/);
  assert.doesNotMatch(nativeViewport, /playbackNavigationTarget|scrub_to|begin_scrub/);
  assert.match(nativeViewport, /EditorialAudioMeters/);
  assert.match(nativeViewport, /data-signal-status=\{feedback\.signalStatus\}/);
  assert.match(nativeViewport, /data-route-state=\{route\.state\}/);
  assert.match(styles, /\.viewer-editorial-feedback/);
  assert.match(styles, /\.native-viewport__toolbar/);
  assert.match(styles, /data-presentation="cinema"/);
  assert.match(styles, /\.editorial-audio-meters/);
  assert.match(styles, /\.editorial-audio-route\[data-route-state="routed"\]/);
  assert.doesNotMatch(
    workspaces,
    /createContext|useReducer|useState|useSuperiApi|DesktopSuperiTransport|@tauri-apps/,
  );
});
