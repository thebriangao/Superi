import assert from "node:assert/strict";
import test from "node:test";

import {
  ACCESSIBILITY_SEMANTICS_SCHEMA_VERSION,
  APPLICATION_SEMANTIC_SURFACES,
  applicationSemanticSurface,
} from "../src/accessibility-semantics.ts";

test("semantic surfaces publish unique stable roles and relationships", () => {
  assert.equal(ACCESSIBILITY_SEMANTICS_SCHEMA_VERSION, 1);
  const surfaces = Object.values(APPLICATION_SEMANTIC_SURFACES);
  assert.equal(new Set(surfaces.map((surface) => surface.id)).size, surfaces.length);
  assert.deepEqual(APPLICATION_SEMANTIC_SURFACES.routes.controls, [
    APPLICATION_SEMANTIC_SURFACES.activeWorkflow.id,
  ]);
  assert.equal(APPLICATION_SEMANTIC_SURFACES.workspaceControls.role, "toolbar");
  assert.equal(APPLICATION_SEMANTIC_SURFACES.activeWorkflow.role, "region");
  assert.equal(
    APPLICATION_SEMANTIC_SURFACES.activeWorkflow.describedBy,
    APPLICATION_SEMANTIC_SURFACES.activeWorkflowStatus.id,
  );
});

test("live surfaces are explicit, atomic where complete state replaces, and frozen", () => {
  assert.equal(APPLICATION_SEMANTIC_SURFACES.notifications.role, "log");
  assert.equal(APPLICATION_SEMANTIC_SURFACES.notifications.atomic, false);
  for (const surface of [
    APPLICATION_SEMANTIC_SURFACES.activeWorkflowStatus,
    APPLICATION_SEMANTIC_SURFACES.applicationStatus,
    APPLICATION_SEMANTIC_SURFACES.intelligentResultsStatus,
  ]) {
    assert.equal(surface.role, "status");
    assert.equal(surface.live, "polite");
    assert.equal(surface.atomic, true);
    assert.ok(Object.isFrozen(surface));
    assert.ok(Object.isFrozen(surface.controls));
  }
  assert.throws(
    () => applicationSemanticSurface("invented-surface"),
    /unknown application semantic surface/,
  );
});
