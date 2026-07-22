import assert from "node:assert/strict";
import test from "node:test";

import {
  DESKTOP_PLATFORMS,
  PLATFORM_SEMANTIC_CONTRACTS,
  platformSemanticSnapshot,
} from "../src/platform-parity.ts";

test("desktop parity covers every supported platform and semantic domain exactly once", () => {
  assert.deepEqual(DESKTOP_PLATFORMS, ["macos", "windows", "linux"]);
  assert.deepEqual(
    PLATFORM_SEMANTIC_CONTRACTS.map((contract) => contract.domain),
    [
      "project",
      "engine",
      "ui",
      "shortcut",
      "media",
      "color",
      "audio",
      "ai",
      "plugin",
      "export",
    ],
  );
  assert.equal(new Set(PLATFORM_SEMANTIC_CONTRACTS.map(({ domain }) => domain)).size, 10);
});

test("every platform projects the same frozen semantic contract identity", () => {
  const snapshots = DESKTOP_PLATFORMS.map(platformSemanticSnapshot);
  assert.deepEqual(
    snapshots.map(({ contract_id }) => contract_id),
    [
      "superi.desktop.semantic-parity.v1",
      "superi.desktop.semantic-parity.v1",
      "superi.desktop.semantic-parity.v1",
    ],
  );
  assert.ok(snapshots.every(({ domains }) => domains === PLATFORM_SEMANTIC_CONTRACTS));
  assert.ok(snapshots.every(Object.isFrozen));
  assert.ok(PLATFORM_SEMANTIC_CONTRACTS.every(Object.isFrozen));
  assert.match(
    PLATFORM_SEMANTIC_CONTRACTS.find(({ domain }) => domain === "audio")?.invariant ?? "",
    /sample timing.*channel meaning.*synchronization.*routing.*audible continuity/iu,
  );
});

test("unsupported platform labels cannot invent a semantic branch", () => {
  assert.throws(
    () => platformSemanticSnapshot("android"),
    /unsupported desktop platform/iu,
  );
});
