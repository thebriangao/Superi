import assert from "node:assert/strict";
import test from "node:test";

import {
  capabilityFailureText,
  discoverDesktopCapabilities,
  type DesktopCapabilityHost,
} from "../src/system-capabilities.ts";

function snapshot() {
  return {
    schema_version: 1,
    revision: 7,
    observed_at_unix_ms: 1_721_600_000_000,
    cache_status: "current",
    persistence_failure: null,
    gpu: {
      condition: "available",
      freshness: "live",
      data: {
        adapters: [
          {
            id: "metal:0000106b:00000000:0",
            name: "Apple GPU",
            backend: "metal",
            device_type: "integrated_gpu",
            vendor_id: 4203,
            device_id: 0,
            driver: "Metal",
            driver_info: "system",
            feature_bits: "0x40",
            webgpu_compliant: true,
            max_texture_dimension_2d: 16_384,
            max_bind_groups: 8,
            max_buffer_size: 1_073_741_824,
          },
        ],
        skipped_adapters: 0,
      },
      failure: null,
    },
    audio: {
      condition: "degraded",
      freshness: "live",
      data: {
        outputs: [
          {
            id: "coreaudio:output",
            name: "Built-in Output",
            is_default: true,
            default_config: {
              channels: 2,
              sample_rate: 48_000,
              sample_format: "f32",
              buffer_frames: null,
            },
            capabilities: [
              {
                channels: 2,
                min_sample_rate: 44_100,
                max_sample_rate: 96_000,
                sample_format: "f32",
                buffer_size: { kind: "range", min: 64, max: 1_024 },
              },
            ],
            channel_layout_known: false,
          },
        ],
        inputs: [],
        skipped_output_devices: 0,
        skipped_input_devices: 1,
      },
      failure: {
        code: "audio_input_discovery_partial",
        title: "Some audio inputs could not be inspected",
        action: "Reconnect the input device, then refresh capabilities.",
      },
    },
    codecs: {
      condition: "available",
      freshness: "live",
      data: {
        schema_version: "2.0.0",
        revision: 0,
        backends: [
          {
            id: "pcm",
            display_name: "PCM",
            priority: 100,
            tier: "primary",
            hardware_acceleration: "software",
            operations: ["decode:pcm_s16le", "encode:pcm_s16le"],
            codec_capability_count: 2,
          },
        ],
        operations: [
          {
            operation: "decode:pcm_s16le",
            primary_backends: ["pcm"],
            fallback_backends: [],
          },
        ],
      },
      failure: null,
    },
    ai: {
      condition: "unavailable",
      freshness: "live",
      data: {
        schema_version: 1,
        runtime: "unavailable",
        local_only: true,
        requires_editable_artifacts: true,
        available_pipelines: [],
      },
      failure: {
        code: "ai_runtime_unavailable",
        title: "Local AI runtime is not installed",
        action: "Continue without AI tools.",
      },
    },
  } as const;
}

test("desktop discovery invokes one native read and deeply freezes exact capability meaning", async () => {
  const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
  const host: DesktopCapabilityHost = {
    invoke: async (command, args) => {
      calls.push({ command, args });
      return snapshot();
    },
  };

  const result = await discoverDesktopCapabilities(host);

  assert.deepEqual(calls, [
    { command: "desktop_capabilities_discover", args: undefined },
  ]);
  assert.equal(result.audio.data?.outputs[0]?.capabilities[0]?.min_sample_rate, 44_100);
  assert.equal(result.audio.data?.outputs[0]?.channel_layout_known, false);
  assert.equal(result.codecs.data?.backends[0]?.operations[0], "decode:pcm_s16le");
  assert.equal(result.ai.data?.runtime, "unavailable");
  assert.ok(Object.isFrozen(result));
  assert.ok(Object.isFrozen(result.audio.data?.outputs[0]?.capabilities));
});

test("strict parsing rejects unknown and malformed native capability state", async () => {
  const unknownRoot = { ...snapshot(), unexpected: true };
  await assert.rejects(
    discoverDesktopCapabilities({ invoke: async () => unknownRoot }),
    /unexpected field/i,
  );

  const invalidAudio = structuredClone(snapshot());
  invalidAudio.audio.data.outputs[0].capabilities[0].min_sample_rate = -1;
  await assert.rejects(
    discoverDesktopCapabilities({ invoke: async () => invalidAudio }),
    /min_sample_rate/i,
  );

  const missingAvailableData = structuredClone(snapshot());
  missingAvailableData.gpu.data = null;
  await assert.rejects(
    discoverDesktopCapabilities({ invoke: async () => missingAvailableData }),
    /inconsistent capability state/i,
  );

  const falseAiBoundary = structuredClone(snapshot());
  falseAiBoundary.ai.data.local_only = false;
  await assert.rejects(
    discoverDesktopCapabilities({ invoke: async () => falseAiBoundary }),
    /inconsistent unavailable AI state/i,
  );
});

test("failure presentation retains the safe title and recovery action", () => {
  assert.equal(
    capabilityFailureText(snapshot().audio.failure),
    "Some audio inputs could not be inspected Reconnect the input device, then refresh capabilities.",
  );
  assert.equal(capabilityFailureText(null), null);
});
