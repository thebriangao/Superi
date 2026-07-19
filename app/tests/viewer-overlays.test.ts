import assert from "node:assert/strict";
import test from "node:test";

import {
  OVERLAY_DEFINITIONS,
  initialViewerOverlays,
  toggleViewerOverlay,
  visibleViewerOverlays,
} from "../src/viewer-overlays.ts";

test("viewer overlays expose the complete frozen presentation-only catalog", () => {
  assert.deepEqual(
    OVERLAY_DEFINITIONS.map(({ kind }) => kind),
    ["safe-area", "guide", "grid", "ruler", "center", "aspect", "custom"],
  );
  assert.ok(Object.isFrozen(OVERLAY_DEFINITIONS));
  assert.ok(OVERLAY_DEFINITIONS.every(Object.isFrozen));
  assert.deepEqual(OVERLAY_DEFINITIONS.at(-1)?.geometry, {
    insetTop: 12.5,
    insetRight: 8,
    insetBottom: 12.5,
    insetLeft: 8,
  });
});

test("overlay visibility is immutable and independent from viewer navigation and status", () => {
  const initial = initialViewerOverlays();
  const safe = toggleViewerOverlay(initial, "safe-area");
  const custom = toggleViewerOverlay(safe, "custom");

  assert.ok(Object.isFrozen(initial));
  assert.ok(Object.isFrozen(custom));
  assert.deepEqual(visibleViewerOverlays(initial), []);
  assert.deepEqual(
    visibleViewerOverlays(custom).map(({ kind }) => kind),
    ["safe-area", "custom"],
  );
  assert.deepEqual(initial, {
    "safe-area": false,
    guide: false,
    grid: false,
    ruler: false,
    center: false,
    aspect: false,
    custom: false,
  });
  assert.doesNotMatch(JSON.stringify(custom), /timecode|playback|status|zoom|pan|fullscreen|cinema/);
});
