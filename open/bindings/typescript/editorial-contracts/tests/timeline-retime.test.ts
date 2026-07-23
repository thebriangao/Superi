import assert from "node:assert/strict";
import test from "node:test";

import {
  planTimelineRetime,
  removeTimelineRetimePoint,
  splitTimelineRetimeDraft,
  timelineRetimeDraftForMode,
  timelineRetimeDraftFromClip,
  timelineRetimeMode,
} from "../src/timeline-retime.ts";


const recordRate = Object.freeze({ numerator: 24, denominator: 1 });
const sourceRate = Object.freeze({ numerator: 48, denominator: 1 });

const clip = Object.freeze({
  id: "clip:00000000000000000000000000000004",
  name: "Arrival",
  timelineId: "timeline:00000000000000000000000000000002",
  trackId: "track:00000000000000000000000000000003",
  trackName: "V1",
  startSeconds: 0,
  endSeconds: 2,
  recordRange: {
    start: { value: "0", timebase: recordRate },
    duration: { value: "48", timebase: recordRate },
  },
  sourceRange: {
    start: { value: "48", timebase: sourceRate },
    duration: { value: "96", timebase: sourceRate },
  },
  timeMap: {
    recordDuration: { value: "48", timebase: recordRate },
    sourceTimebase: sourceRate,
    segments: [
      {
        recordRange: {
          start: { value: "0", timebase: recordRate },
          duration: { value: "48", timebase: recordRate },
        },
        sourceStart: { value: "48", timebase: sourceRate },
        rateNumerator: "1",
        rateDenominator: "1",
      },
    ],
  },
});

test("mode drafts expose exact speed reverse freeze and identity consequences", () => {
  const current = timelineRetimeDraftFromClip(clip);
  assert.equal(timelineRetimeMode(clip.timeMap), "identity");

  const speed = planTimelineRetime({
    clip,
    draft: timelineRetimeDraftForMode(clip, current, "speed"),
    projectRevision: 17,
    transactionId: "retime-speed",
  });
  assert.equal(speed.status, "ready");
  if (speed.status !== "ready") return;
  assert.equal(speed.mode, "speed");
  assert.deepEqual(speed.sourceTraversal, { start: "48", end: "240" });
  assert.equal(speed.timeMap.segments[0]?.rate_numerator, 2);
  assert.match(speed.consequence, /Arrival/);
  assert.match(speed.consequence, /48-unit record duration/);
  assert.deepEqual(speed.request, {
    transaction_id: "retime-speed",
    expected_project_revision: 17,
    command: {
      command: "apply",
      actions: [
        {
          action: "edit_timeline",
          operations: [speed.operation],
        },
      ],
    },
  });

  const reverse = planTimelineRetime({
    clip,
    draft: timelineRetimeDraftForMode(clip, current, "reverse"),
    projectRevision: 17,
    transactionId: "retime-reverse",
  });
  assert.equal(reverse.status, "ready");
  if (reverse.status !== "ready") return;
  assert.deepEqual(reverse.sourceTraversal, { start: "143", end: "47" });
  assert.equal(reverse.timeMap.segments[0]?.rate_numerator, -1);

  const freeze = planTimelineRetime({
    clip,
    draft: timelineRetimeDraftForMode(clip, current, "freeze"),
    projectRevision: 17,
    transactionId: "retime-freeze",
  });
  assert.equal(freeze.status, "ready");
  if (freeze.status !== "ready") return;
  assert.deepEqual(freeze.sourceTraversal, { start: "48", end: "48" });
  assert.equal(freeze.timeMap.segments[0]?.rate_numerator, 0);

  const identity = planTimelineRetime({
    clip,
    draft: timelineRetimeDraftForMode(clip, current, "identity"),
    projectRevision: 17,
    transactionId: "retime-identity",
  });
  assert.equal(identity.status, "disabled");
  if (identity.status !== "disabled") return;
  assert.match(identity.reason, /already matches/);
});

test("time-remap points preserve exact duration and derive continuous source seams", () => {
  const current = timelineRetimeDraftFromClip(clip);
  const split = splitTimelineRetimeDraft(current, "24");
  const remapDraft = {
    ...split,
    mode: "time_remap" as const,
    segments: [
      { ...split.segments[0]!, rateNumerator: "2" },
      {
        ...split.segments[1]!,
        rateNumerator: "0",
        rateDenominator: "7",
      },
    ],
  };
  const plan = planTimelineRetime({
    clip,
    draft: remapDraft,
    projectRevision: 17,
    transactionId: "retime-curve",
  });
  assert.equal(plan.status, "ready");
  if (plan.status !== "ready") return;
  assert.deepEqual(
    plan.timeMap.segments.map((segment) => ({
      recordStart: segment.record_range.start.value,
      recordDuration: segment.record_range.duration.value,
      sourceStart: segment.source_start.value,
      numerator: segment.rate_numerator,
      denominator: segment.rate_denominator,
    })),
    [
      {
        recordStart: 0,
        recordDuration: 24,
        sourceStart: 48,
        numerator: 2,
        denominator: 1,
      },
      {
        recordStart: 24,
        recordDuration: 24,
        sourceStart: 144,
        numerator: 0,
        denominator: 1,
      },
    ],
  );
  assert.deepEqual(
    plan.curvePoints.map((point) => [
      point.recordOffset,
      point.sourceValue,
    ]),
    [
      ["0", "48"],
      ["24", "144"],
      ["48", "144"],
    ],
  );

  const merged = removeTimelineRetimePoint(remapDraft, 1);
  assert.deepEqual(merged.segments, [
    {
      recordDuration: "48",
      rateNumerator: "2",
      rateDenominator: "1",
    },
  ]);
});

test("invalid sums inexact clocks unsafe values and no-op maps stay disabled", () => {
  const current = timelineRetimeDraftFromClip(clip);

  const wrongSum = planTimelineRetime({
    clip,
    draft: {
      ...current,
      segments: [
        {
          recordDuration: "47",
          rateNumerator: "2",
          rateDenominator: "1",
        },
      ],
    },
    projectRevision: 17,
    transactionId: "wrong-sum",
  });
  assert.equal(wrongSum.status, "disabled");
  if (wrongSum.status === "disabled") {
    assert.match(wrongSum.reason, /sum exactly/);
  }

  const inexact = planTimelineRetime({
    clip,
    draft: {
      sourceStart: "48",
      mode: "time_remap",
      segments: [
        {
          recordDuration: "1",
          rateNumerator: "1",
          rateDenominator: "3",
        },
        {
          recordDuration: "47",
          rateNumerator: "1",
          rateDenominator: "1",
        },
      ],
    },
    projectRevision: 17,
    transactionId: "inexact",
  });
  assert.equal(inexact.status, "disabled");
  if (inexact.status === "disabled") {
    assert.match(inexact.reason, /exact source seam/);
  }

  const unsafe = planTimelineRetime({
    clip,
    draft: {
      ...current,
      sourceStart: "9007199254740992",
    },
    projectRevision: 17,
    transactionId: "unsafe",
  });
  assert.equal(unsafe.status, "disabled");
  if (unsafe.status === "disabled") {
    assert.match(unsafe.reason, /safe integer/);
  }

  assert.throws(() => splitTimelineRetimeDraft(current, "0"), /interior/);
  assert.throws(() => removeTimelineRetimePoint(current, 0), /boundary/);
});
