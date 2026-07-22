import assert from "node:assert/strict";
import test from "node:test";

import {
  SCREEN_READER_SUPPORT_SCHEMA_VERSION,
  SCREEN_READER_SURFACE_ORDER,
  SCREEN_READER_SURFACES,
  screenReaderSurface,
} from "../src/screen-reader-support.ts";

test("screen-reader support covers every required professional surface exactly once", () => {
  assert.equal(SCREEN_READER_SUPPORT_SCHEMA_VERSION, 1);
  assert.deepEqual(SCREEN_READER_SURFACE_ORDER, [
    "project",
    "media",
    "timeline",
    "inspector",
    "mixer",
    "graph",
    "scopes",
    "jobs",
    "dialogs",
  ]);
  const surfaces = Object.values(SCREEN_READER_SURFACES);
  assert.equal(new Set(surfaces.map((surface) => surface.id)).size, 9);
  assert.equal(new Set(surfaces.map((surface) => surface.descriptionId)).size, 9);
});

test("surface guidance is labelled, actionable, immutable, and fail closed", () => {
  for (const id of SCREEN_READER_SURFACE_ORDER) {
    const surface = screenReaderSurface(id);
    assert.ok(surface.label.length > 0);
    assert.ok(surface.description.length > 60);
    assert.ok(Object.isFrozen(surface));
  }
  assert.equal(SCREEN_READER_SURFACES.graph.interaction, "edit");
  assert.match(SCREEN_READER_SURFACES.graph.description, /typed project actions/);
  assert.equal(SCREEN_READER_SURFACES.dialogs.interaction, "modal");
  assert.throws(
    () => screenReaderSurface("invented"),
    /unknown screen-reader surface/,
  );
});
