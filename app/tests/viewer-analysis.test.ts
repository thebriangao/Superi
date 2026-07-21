import assert from "node:assert/strict";
import test from "node:test";

import {
  DEFAULT_VIEWER_ANALYSIS_VIEW,
  VIEWER_ANALYSIS_DEFINITIONS,
  viewerAnalysisDefinition,
} from "../src/viewer-analysis.ts";

test("viewer analysis modes have one frozen deterministic catalog", () => {
  assert.equal(DEFAULT_VIEWER_ANALYSIS_VIEW, "image");
  assert.deepEqual(
    VIEWER_ANALYSIS_DEFINITIONS.map(({ view }) => view),
    [
      "image",
      "alpha",
      "red",
      "green",
      "blue",
      "luminance",
      "false_color",
      "clipping",
    ],
  );
  assert.deepEqual(
    VIEWER_ANALYSIS_DEFINITIONS.map(({ label }) => label),
    [
      "Image",
      "Alpha",
      "Red",
      "Green",
      "Blue",
      "Luminance",
      "False color",
      "Clipping",
    ],
  );
  assert.ok(Object.isFrozen(VIEWER_ANALYSIS_DEFINITIONS));
  for (const definition of VIEWER_ANALYSIS_DEFINITIONS) {
    assert.ok(Object.isFrozen(definition));
    assert.equal(viewerAnalysisDefinition(definition.view), definition);
  }
});

test("unknown analysis modes cannot silently fall back to image", () => {
  assert.throws(
    () => viewerAnalysisDefinition("waveform" as never),
    /unknown viewer analysis view/i,
  );
});
