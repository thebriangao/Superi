import assert from "node:assert/strict";
import test from "node:test";

import {
  INITIAL_VIEWER_EXTERNAL_DISPLAY_SELECTION,
  formatViewerExternalDisplayOutput,
  reconcileViewerExternalDisplaySelection,
  selectViewerExternalDisplay,
  type ViewerExternalDisplayTarget,
} from "../src/viewer-external-display.ts";

const TARGETS: readonly ViewerExternalDisplayTarget[] = Object.freeze([
  Object.freeze({
    id: "tauri-monitor:studio",
    name: "Studio monitor",
    positionX: 2560,
    positionY: -120,
    physicalWidth: 3840,
    physicalHeight: 2160,
    scaleFactor: 2,
    primary: false,
  }),
  Object.freeze({
    id: "tauri-monitor:client",
    name: "Client display",
    positionX: -1920,
    positionY: 0,
    physicalWidth: 1920,
    physicalHeight: 1080,
    scaleFactor: 1,
    primary: true,
  }),
]);

test("external display selection is frozen, exact, and rejects stale targets", () => {
  assert.ok(Object.isFrozen(INITIAL_VIEWER_EXTERNAL_DISPLAY_SELECTION));
  assert.deepEqual(INITIAL_VIEWER_EXTERNAL_DISPLAY_SELECTION, { targetId: null });

  const selected = selectViewerExternalDisplay(
    INITIAL_VIEWER_EXTERNAL_DISPLAY_SELECTION,
    "tauri-monitor:studio",
    TARGETS,
  );
  assert.deepEqual(selected, { targetId: "tauri-monitor:studio" });
  assert.ok(Object.isFrozen(selected));
  assert.equal(
    reconcileViewerExternalDisplaySelection(selected, TARGETS),
    selected,
  );
  assert.equal(
    reconcileViewerExternalDisplaySelection(selected, TARGETS.slice(1)),
    INITIAL_VIEWER_EXTERNAL_DISPLAY_SELECTION,
  );
  assert.throws(
    () => selectViewerExternalDisplay(selected, "tauri-monitor:missing", TARGETS),
    /external display target is not available/i,
  );
  assert.equal(
    selectViewerExternalDisplay(selected, null, TARGETS),
    INITIAL_VIEWER_EXTERNAL_DISPLAY_SELECTION,
  );
});

test("external output reports exact target, visual mode, and native frame identity", () => {
  assert.equal(
    formatViewerExternalDisplayOutput({
      phase: "presenting",
      targetId: "tauri-monitor:studio",
      targetName: "Studio monitor",
      selectedView: "false_color",
      presentedView: "false_color",
      physicalWidth: 3840,
      physicalHeight: 2160,
      scaleFactor: 2,
      surfaceGeneration: 7,
      frameSequence: 91,
      displayIntent: "scene-linear ACEScg to sRGB display",
      summary: null,
    }),
    "External Studio monitor; presenting 3840x2160 @ 2x; selected false_color; presented false_color; surface 7 frame 91; scene-linear ACEScg to sRGB display.",
  );
  assert.equal(
    formatViewerExternalDisplayOutput({
      phase: "inactive",
      targetId: null,
      targetName: null,
      selectedView: "image",
      presentedView: null,
      physicalWidth: 0,
      physicalHeight: 0,
      scaleFactor: 0,
      surfaceGeneration: 0,
      frameSequence: 0,
      displayIntent: "scene-linear ACEScg to sRGB display",
      summary: "No external display selected.",
    }),
    "External display inactive; No external display selected.",
  );
});
