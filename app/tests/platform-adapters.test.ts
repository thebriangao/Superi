import assert from "node:assert/strict";
import test from "node:test";

import {
  discoverPlatformAdapters,
  type PlatformAdapterHost,
} from "../src/platform-adapters.ts";

function snapshot() {
  return {
    schema_version: 1,
    platform: "macos",
    media_guarantees: [
      "timing",
      "precision",
      "metadata",
      "alpha",
      "predictable_fallback",
    ],
    adapters: [
      { domain: "gpu", contract_id: "superi.adapter.gpu.v1", implementation: "wgpu-metal" },
      { domain: "audio", contract_id: "superi.adapter.audio.v1", implementation: "cpal-coreaudio" },
      { domain: "filesystem", contract_id: "superi.adapter.filesystem.v1", implementation: "std-filesystem-macos" },
      { domain: "font", contract_id: "superi.adapter.font.v1", implementation: "webview-coretext" },
      { domain: "monitor", contract_id: "superi.adapter.monitor.v1", implementation: "tauri-cocoa-monitor" },
      { domain: "codec", contract_id: "superi.adapter.codec.v1", implementation: "engine-codec-registry-macos" },
    ],
  };
}

test("native adapter discovery invokes one read and freezes all six shared contracts", async () => {
  const calls: string[] = [];
  const host: PlatformAdapterHost = {
    invoke: async (command) => {
      calls.push(command);
      return snapshot();
    },
  };
  const result = await discoverPlatformAdapters(host);
  assert.deepEqual(calls, ["desktop_platform_adapters"]);
  assert.equal(result.platform, "macos");
  assert.deepEqual(result.adapters.map(({ domain }) => domain), [
    "gpu",
    "audio",
    "filesystem",
    "font",
    "monitor",
    "codec",
  ]);
  assert.ok(Object.isFrozen(result));
  assert.ok(Object.isFrozen(result.adapters));
  assert.ok(result.adapters.every(Object.isFrozen));
});

test("strict parsing rejects target, contract, duplicate, and shape drift", async () => {
  await assert.rejects(
    discoverPlatformAdapters({ invoke: async () => ({ ...snapshot(), platform: "android" }) }),
    /platform/iu,
  );
  const wrongContract = structuredClone(snapshot());
  wrongContract.adapters[0].contract_id = "superi.adapter.audio.v1";
  await assert.rejects(
    discoverPlatformAdapters({ invoke: async () => wrongContract }),
    /contract_id/iu,
  );
  const duplicate = structuredClone(snapshot());
  duplicate.adapters[5] = { ...duplicate.adapters[0] };
  await assert.rejects(
    discoverPlatformAdapters({ invoke: async () => duplicate }),
    /adapter domains/iu,
  );
  await assert.rejects(
    discoverPlatformAdapters({ invoke: async () => ({ ...snapshot(), unknown: true }) }),
    /unexpected field/iu,
  );
});
