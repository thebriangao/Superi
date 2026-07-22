import assert from "node:assert/strict";
import test from "node:test";

import { nextContainedFocusIndex } from "../src/focus-management.ts";

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
