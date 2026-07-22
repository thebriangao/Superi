import assert from "node:assert/strict";
import test from "node:test";

import {
  keyboardLandmarkDirection,
  nextContainedFocusIndex,
  nextKeyboardLandmarkIndex,
} from "../src/focus-management.ts";

test("contained focus traversal is empty-safe and enters at the requested edge", () => {
  assert.equal(nextContainedFocusIndex(0, -1, false), null);
  assert.equal(nextContainedFocusIndex(-4, 2, true), null);
  assert.equal(nextContainedFocusIndex(4, -1, false), 0);
  assert.equal(nextContainedFocusIndex(4, -1, true), 3);
});

test("contained focus traversal advances and wraps in both directions", () => {
  assert.equal(nextContainedFocusIndex(4, 0, false), 1);
  assert.equal(nextContainedFocusIndex(4, 3, false), 0);
  assert.equal(nextContainedFocusIndex(4, 3, true), 2);
  assert.equal(nextContainedFocusIndex(4, 0, true), 3);
  assert.equal(nextContainedFocusIndex(4, 99, false), 0);
});

test("keyboard landmark routing recognizes only unmodified, settled F6 input", () => {
  assert.equal(keyboardLandmarkDirection({ key: "F6" }), "forward");
  assert.equal(
    keyboardLandmarkDirection({ key: "F6", shiftKey: true }),
    "backward",
  );
  assert.equal(keyboardLandmarkDirection({ key: "F6", ctrlKey: true }), null);
  assert.equal(keyboardLandmarkDirection({ key: "F6", isComposing: true }), null);
  assert.equal(keyboardLandmarkDirection({ key: "F6", repeat: true }), null);
  assert.equal(keyboardLandmarkDirection({ key: "Tab" }), null);
});

test("keyboard landmark traversal enters and wraps every registered region", () => {
  assert.equal(nextKeyboardLandmarkIndex(3, -1, "forward"), 0);
  assert.equal(nextKeyboardLandmarkIndex(3, -1, "backward"), 2);
  assert.equal(nextKeyboardLandmarkIndex(3, 2, "forward"), 0);
  assert.equal(nextKeyboardLandmarkIndex(3, 0, "backward"), 2);
  assert.equal(nextKeyboardLandmarkIndex(0, 0, "forward"), null);
});
