import assert from "node:assert/strict";
import test from "node:test";

import {
  VIEWER_COMPARISON_DEFINITIONS,
  applyViewerComparison,
  createViewerFrameIdentity,
  formatViewerComparisonState,
  initialViewerComparison,
  viewerComparisonAvailable,
} from "../src/viewer-comparison.ts";

const REFERENCE_FRAME = createViewerFrameIdentity(
  "program",
  {
    role: "program",
    phase: "presenting",
    physicalWidth: 3840,
    physicalHeight: 2160,
    surfaceGeneration: 12,
    frameSequence: 48,
    displayIntent: "scene-linear ACEScg to sRGB display",
  },
  {
    owner: "playback",
    value: 47,
    timebaseNumerator: 24,
    timebaseDenominator: 1,
  },
);

const CURRENT_FRAME = createViewerFrameIdentity(
  "program",
  {
    role: "program",
    phase: "presenting",
    physicalWidth: 3840,
    physicalHeight: 2160,
    surfaceGeneration: 12,
    frameSequence: 49,
    displayIntent: "scene-linear ACEScg to sRGB display",
  },
  {
    owner: "playback",
    value: 48,
    timebaseNumerator: 24,
    timebaseDenominator: 1,
  },
);

test("comparison catalog and captures preserve exact immutable frame identity", () => {
  assert.deepEqual(
    VIEWER_COMPARISON_DEFINITIONS.map(({ mode }) => mode),
    ["single", "compare", "split", "wipe", "difference", "reference", "snapshot"],
  );
  assert.ok(Object.isFrozen(VIEWER_COMPARISON_DEFINITIONS));
  assert.ok(VIEWER_COMPARISON_DEFINITIONS.every(Object.isFrozen));

  const initial = initialViewerComparison();
  assert.ok(Object.isFrozen(initial));
  assert.equal(
    applyViewerComparison(initial, { action: "mode", mode: "difference" }, CURRENT_FRAME),
    initial,
  );

  const captured = applyViewerComparison(
    initial,
    { action: "capture_reference" },
    REFERENCE_FRAME,
  );
  assert.ok(Object.isFrozen(captured));
  assert.ok(Object.isFrozen(captured.reference));
  assert.ok(Object.isFrozen(captured.reference?.visual));
  assert.ok(Object.isFrozen(captured.reference?.temporal));
  assert.deepEqual(captured.reference, REFERENCE_FRAME);

  const split = applyViewerComparison(
    captured,
    { action: "mode", mode: "split" },
    CURRENT_FRAME,
  );
  const bounded = applyViewerComparison(
    split,
    { action: "position", position: 2 },
    CURRENT_FRAME,
  );
  const horizontal = applyViewerComparison(
    bounded,
    { action: "orientation", orientation: "horizontal" },
    CURRENT_FRAME,
  );
  assert.deepEqual(
    {
      mode: horizontal.mode,
      orientation: horizontal.orientation,
      position: horizontal.position,
      referenceCoordinate: horizontal.reference?.temporal?.value,
    },
    {
      mode: "split",
      orientation: "horizontal",
      position: 0.95,
      referenceCoordinate: 47,
    },
  );
});

test("comparison availability gates missing pixels and reports exact current and captured context", () => {
  const mismatchedRole = createViewerFrameIdentity(
    "source",
    {
      role: "program",
      phase: "presenting",
      physicalWidth: 3840,
      physicalHeight: 2160,
      surfaceGeneration: 12,
      frameSequence: 49,
      displayIntent: "scene-linear ACEScg to sRGB display",
    },
    null,
  );
  assert.equal(mismatchedRole.visual, null);

  const unavailable = createViewerFrameIdentity(
    "program",
    null,
    {
      owner: "playback",
      value: 48,
      timebaseNumerator: 24,
      timebaseDenominator: 1,
    },
  );
  const initial = initialViewerComparison();
  assert.equal(viewerComparisonAvailable(initial, unavailable, "compare"), false);
  assert.equal(
    applyViewerComparison(initial, { action: "capture_reference" }, unavailable),
    initial,
  );
  assert.match(
    formatViewerComparisonState(initial, unavailable),
    /native frame unavailable.*playback context 48 @ 24\/1.*native frame binding unavailable/,
  );

  const reference = applyViewerComparison(
    initial,
    { action: "capture_reference" },
    REFERENCE_FRAME,
  );
  const sourceFrame = createViewerFrameIdentity(
    "source",
    {
      role: "source",
      phase: "presenting",
      physicalWidth: 1920,
      physicalHeight: 1080,
      surfaceGeneration: 4,
      frameSequence: 8,
      displayIntent: "scene-linear ACEScg to sRGB display",
    },
    null,
  );
  assert.equal(viewerComparisonAvailable(reference, sourceFrame, "reference"), false);
  assert.equal(viewerComparisonAvailable(reference, CURRENT_FRAME, "difference"), true);
  const difference = applyViewerComparison(
    reference,
    { action: "mode", mode: "difference" },
    CURRENT_FRAME,
  );
  assert.match(
    formatViewerComparisonState(difference, CURRENT_FRAME),
    /Difference.*current surface 12 frame 49.*playback context 48 @ 24\/1.*reference surface 12 frame 48.*playback context 47 @ 24\/1/,
  );

  const snapshotted = applyViewerComparison(
    difference,
    { action: "capture_snapshot" },
    CURRENT_FRAME,
  );
  const snapshot = applyViewerComparison(
    snapshotted,
    { action: "mode", mode: "snapshot" },
    CURRENT_FRAME,
  );
  assert.match(
    formatViewerComparisonState(snapshot, CURRENT_FRAME),
    /Snapshot.*surface 12 frame 49.*playback context 48 @ 24\/1/,
  );
});
