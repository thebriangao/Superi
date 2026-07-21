import assert from "node:assert/strict";
import test from "node:test";

import {
  VIEWER_DISPLAY_TRANSFORMS,
  createViewerColorSelection,
  formatViewerColorState,
  projectViewerColorState,
  type ViewerColorSnapshot,
} from "../src/viewer-color-management.ts";

const SNAPSHOT = {
  profileGeneration: 7,
  monitorProfiles: [
    {
      id: "macos-cgdisplay:1",
      name: "Built-in Display",
      primary: true,
      builtIn: true,
      profileState: "profiled",
      profileId: "a".repeat(64),
      profileModel: "matrix_trc",
      renderingIntent: "perceptual",
    },
    {
      id: "macos-cgdisplay:9",
      name: "Reference Display",
      primary: false,
      builtIn: false,
      profileState: "unprofiled",
      profileId: null,
      profileModel: null,
      renderingIntent: null,
    },
  ],
  selectedMonitorId: "macos-cgdisplay:1",
  displayTransform: "display_p3",
  displayIntent: "scene-linear ACEScg to Display P3 display",
  displayTransformId: "superi.viewport.acescg-to-display-p3.v1",
  transformOrder: [
    "alpha_unassociate",
    "scene_to_display_primaries",
    "gamut_mapping",
    "tone_mapping",
    "transfer_encoding",
    "alpha_reassociate",
  ],
  profileNote:
    "Profile identity and freshness verified; built-in display transform selected; arbitrary ICC tag evaluation is unavailable.",
} as const satisfies ViewerColorSnapshot;

test("viewer color state freezes exact monitor profile and display transform evidence", () => {
  assert.deepEqual(
    VIEWER_DISPLAY_TRANSFORMS.map(({ code }) => code),
    ["srgb", "display_p3"],
  );
  assert.ok(Object.isFrozen(VIEWER_DISPLAY_TRANSFORMS));
  assert.ok(VIEWER_DISPLAY_TRANSFORMS.every(Object.isFrozen));

  const state = projectViewerColorState(SNAPSHOT);
  assert.ok(Object.isFrozen(state));
  assert.ok(Object.isFrozen(state.monitorProfiles));
  assert.ok(state.monitorProfiles.every(Object.isFrozen));
  assert.ok(Object.isFrozen(state.transformOrder));
  assert.equal(state.selectedMonitor?.id, "macos-cgdisplay:1");
  assert.equal(state.selectedMonitor?.profileId, "a".repeat(64));
  assert.equal(state.displayTransform, "display_p3");
  assert.equal(
    formatViewerColorState(state),
    "Built-in Display (macos-cgdisplay:1), profile aaaaaaaaaaaa, matrix trc, perceptual; Display P3 via superi.viewport.acescg-to-display-p3.v1; profile catalog generation 7; Profile identity and freshness verified; built-in display transform selected; arbitrary ICC tag evaluation is unavailable.",
  );

  assert.deepEqual(
    createViewerColorSelection("program", state, {
      monitorId: "macos-cgdisplay:9",
      displayTransform: "srgb",
    }),
    {
      role: "program",
      monitorId: "macos-cgdisplay:9",
      displayTransform: "srgb",
    },
  );
});

test("viewer color state rejects ambiguous or dishonest native evidence", () => {
  assert.throws(
    () =>
      projectViewerColorState({
        ...SNAPSHOT,
        monitorProfiles: [SNAPSHOT.monitorProfiles[0], SNAPSHOT.monitorProfiles[0]],
      }),
    /duplicate monitor identity/i,
  );
  assert.throws(
    () =>
      projectViewerColorState({
        ...SNAPSHOT,
        displayIntent: "scene-linear ACEScg to sRGB display",
      }),
    /display intent/i,
  );
  assert.throws(
    () =>
      projectViewerColorState({
        ...SNAPSHOT,
        selectedMonitorId: "macos-cgdisplay:missing",
      }),
    /selected monitor/i,
  );
  assert.throws(
    () =>
      projectViewerColorState({
        ...SNAPSHOT,
        monitorProfiles: [
          {
            ...SNAPSHOT.monitorProfiles[0],
            profileState: "unprofiled",
          },
        ],
      }),
    /unprofiled monitor/i,
  );
  assert.throws(
    () =>
      projectViewerColorState({
        ...SNAPSHOT,
        monitorProfiles: [
          {
            ...SNAPSHOT.monitorProfiles[0],
            profileModel: "future_model" as "matrix_trc",
          },
        ],
      }),
    /ICC identity evidence/i,
  );
  assert.throws(
    () =>
      projectViewerColorState({
        ...SNAPSHOT,
        monitorProfiles: SNAPSHOT.monitorProfiles.map((profile) => ({
          ...profile,
          primary: true,
        })),
      }),
    /multiple primary/i,
  );
});

test("unavailable profile discovery remains explicit and selectable transforms stay bounded", () => {
  const state = projectViewerColorState({
    ...SNAPSHOT,
    profileGeneration: 0,
    monitorProfiles: [],
    selectedMonitorId: null,
    displayTransform: "srgb",
    displayIntent: "scene-linear ACEScg to sRGB display",
    displayTransformId: "superi.viewport.acescg-to-srgb.v1",
    profileNote: "Monitor profile discovery is unavailable on this desktop target.",
  });
  assert.equal(state.selectedMonitor, null);
  assert.match(formatViewerColorState(state), /profile unavailable/i);
  assert.throws(
    () =>
      createViewerColorSelection("color", state, {
        monitorId: "macos-cgdisplay:1",
        displayTransform: "srgb",
      }),
    /active monitor/i,
  );
});
