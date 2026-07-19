import assert from "node:assert/strict";
import test from "node:test";

import {
  applyViewerNavigation,
  initialViewerNavigation,
  viewerTransform,
} from "../src/viewer-navigation.ts";

test("viewer navigation is frozen, bounded, and keeps fit, pixel, and pan intent exact", () => {
  const initial = initialViewerNavigation("program");
  assert.ok(Object.isFrozen(initial));
  assert.equal(initial.scaleMode, "fit");
  assert.equal(initial.presentation, "normal");
  assert.equal(initial.externalDisplayIntent, "program-managed-display");

  const pixel = applyViewerNavigation(initial, { action: "pixel" });
  assert.deepEqual(pixel, {
    scaleMode: "pixel",
    scale: 1,
    panX: 0,
    panY: 0,
    presentation: "normal",
    externalDisplayIntent: "program-managed-display",
  });
  assert.deepEqual(viewerTransform(pixel), {
    transform: "translate3d(0px, 0px, 0) scale(1)",
    imageRendering: "pixelated",
  });

  const zoomed = applyViewerNavigation(pixel, { action: "zoom", factor: 99 });
  assert.equal(zoomed.scale, 16);
  const panned = applyViewerNavigation(zoomed, {
    action: "pan",
    deltaX: 24,
    deltaY: -12,
  });
  assert.deepEqual([panned.panX, panned.panY], [24, -12]);
  assert.ok(Object.isFrozen(panned));

  const fit = applyViewerNavigation(panned, { action: "fit" });
  assert.deepEqual([fit.scaleMode, fit.scale, fit.panX, fit.panY], ["fit", 1, 0, 0]);
});

test("fullscreen and cinema are exclusive presentation modes without changing navigation", () => {
  const zoomed = applyViewerNavigation(initialViewerNavigation("source"), {
    action: "zoom",
    factor: 2,
  });
  const cinema = applyViewerNavigation(zoomed, {
    action: "presentation",
    mode: "cinema",
  });
  const fullscreen = applyViewerNavigation(cinema, {
    action: "presentation",
    mode: "fullscreen",
  });

  assert.equal(cinema.presentation, "cinema");
  assert.equal(fullscreen.presentation, "fullscreen");
  assert.equal(fullscreen.scale, zoomed.scale);
  assert.equal(fullscreen.externalDisplayIntent, "source-managed-display");
});
