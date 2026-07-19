import assert from "node:assert/strict";
import test from "node:test";

import type { EditorPlaybackSnapshot } from "../src/api.ts";
import {
  formatExactPlaybackTime,
  formatExactRate,
  playbackActionForKey,
  playbackDegradationLabel,
  playbackVisualState,
} from "../src/playback-transport.ts";

const FORWARD_PLAYBACK = {
  mode: "playing",
  playhead: {
    value: 48,
    timebase: { numerator: 24, denominator: 1 },
  },
  scheduled_frame: {
    value: 49,
    timebase: { numerator: 24, denominator: 1 },
  },
  scheduled_due_clock: {
    value: 2_000,
    timebase: { numerator: 1_000, denominator: 1 },
  },
  rate_numerator: 1,
  rate_denominator: 1,
  direction: "forward",
  loop_range: null,
  epoch: 3,
  total_dropped: 0,
  consecutive_dropped: 0,
  forced_presentations: 1,
  audio_state: "discard_acknowledged",
  discard_requested_generation: 4,
  discard_applied_generation: 4,
  degradation: ["viewport_output_unavailable"],
  failure: null,
} satisfies EditorPlaybackSnapshot;

test("JKL shuttle commands cycle exact signed rates and K pauses", () => {
  assert.deepEqual(playbackActionForKey("l", FORWARD_PLAYBACK), {
    action: "shuttle",
    numerator: 2,
    denominator: 1,
  });
  assert.deepEqual(playbackActionForKey("j", FORWARD_PLAYBACK), {
    action: "shuttle",
    numerator: -1,
    denominator: 1,
  });
  assert.deepEqual(
    playbackActionForKey("J", {
      ...FORWARD_PLAYBACK,
      rate_numerator: -2,
      direction: "reverse",
    }),
    { action: "shuttle", numerator: -4, denominator: 1 },
  );
  assert.deepEqual(playbackActionForKey("k", FORWARD_PLAYBACK), {
    action: "pause",
  });
});

test("space toggles exact play and pause intent without inventing UI state", () => {
  assert.deepEqual(playbackActionForKey(" ", FORWARD_PLAYBACK), {
    action: "pause",
  });
  assert.deepEqual(
    playbackActionForKey("space", {
      ...FORWARD_PLAYBACK,
      mode: "paused",
    }),
    { action: "play" },
  );
  assert.equal(playbackActionForKey("x", FORWARD_PLAYBACK), null);
});

test("transport readouts preserve exact rationals and name degraded output", () => {
  assert.equal(
    formatExactPlaybackTime(FORWARD_PLAYBACK.playhead),
    "48 @ 24/1 units/s",
  );
  assert.equal(formatExactRate(FORWARD_PLAYBACK), "+1/1x forward");
  assert.equal(
    playbackDegradationLabel("audio_output_unavailable"),
    "Audio output unavailable; timing and synchronization remain observable.",
  );
  assert.equal(
    playbackVisualState(FORWARD_PLAYBACK),
    "Timing-only degraded output; rendered viewport pixels are unavailable.",
  );
});
