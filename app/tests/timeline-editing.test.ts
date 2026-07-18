import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import type { TimelineEditOperation } from "../src/api.ts";
import {
  TimelineEditingError,
  compileGapClose,
  compileGapInsert,
  compileRippleDelete,
  compileTimelineGesture,
  timelineEditingTools,
  type TimelineIdentityAllocator,
} from "../src/timeline-editing.ts";

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
import type {
  TimelineCanvasItem,
  TimelineCanvasModel,
  TimelineCanvasTrack,
  TimelineExactRange,
  TimelineItemKind,
  TimelineRate,
} from "../src/timeline-workspace.ts";

const EDIT_RATE = { numerator: 24, denominator: 1 } as const;
const FRACTIONAL_RATE = { numerator: 24_000, denominator: 1_001 } as const;
const AUDIO_RATE = { numerator: 48_000, denominator: 1 } as const;
const SOURCE_RATE = { numerator: 48, denominator: 1 } as const;
const GLOBAL_START_UNITS = 86_400;
const GLOBAL_START_SECONDS = 3_600;

function exactRange(
  start: number,
  duration: number,
  timebase: TimelineRate,
): TimelineExactRange {
  return {
    start: { value: String(start), timebase },
    duration: { value: String(duration), timebase },
  };
}

function displaySeconds(units: number, timebase: TimelineRate): number {
  return GLOBAL_START_SECONDS +
    (units * timebase.denominator) / timebase.numerator;
}

function clip(
  id: string,
  name: string,
  recordStart: number,
  recordDuration: number,
  recordRate: TimelineRate,
  sourceStart: number,
  sourceDuration: number,
  options: {
    readonly selected?: boolean;
    readonly group?: readonly string[] | null;
    readonly link?: readonly string[] | null;
  } = {},
): TimelineCanvasItem {
  return {
    kind: "clip",
    id,
    name,
    startSeconds: displaySeconds(recordStart, recordRate),
    endSeconds: displaySeconds(recordStart + recordDuration, recordRate),
    recordRange: exactRange(recordStart, recordDuration, recordRate),
    source: { kind: "media", id: `media.${id}` },
    sourceRange: exactRange(sourceStart, sourceDuration, SOURCE_RATE),
    transition: null,
    selected: options.selected ?? false,
    group: options.group ?? null,
    link: options.link ?? null,
  };
}

function gap(
  id: string,
  recordStart: number,
  recordDuration: number,
  recordRate: TimelineRate,
): TimelineCanvasItem {
  return {
    kind: "gap",
    id,
    name: "Intentional gap",
    startSeconds: displaySeconds(recordStart, recordRate),
    endSeconds: displaySeconds(recordStart + recordDuration, recordRate),
    recordRange: exactRange(recordStart, recordDuration, recordRate),
    source: null,
    sourceRange: null,
    transition: null,
    selected: false,
    group: null,
    link: null,
  };
}

function timelineModel(): TimelineCanvasModel {
  const related = ["clip.left", "clip.center", "clip.right"];
  const video: TimelineCanvasTrack = {
    id: "track.video.1",
    name: "V1",
    kind: "video",
    timebase: EDIT_RATE,
    targeted: true,
    height: 72,
    locked: false,
    syncLocked: true,
    muted: false,
    solo: false,
    enabled: true,
    items: [
      clip("clip.left", "Left", 0, 48, EDIT_RATE, 96, 96, {
        group: related,
        link: related,
      }),
      clip("clip.center", "Center", 48, 48, EDIT_RATE, 192, 96, {
        selected: true,
        group: related,
        link: related,
      }),
      clip("clip.right", "Right", 96, 48, EDIT_RATE, 288, 96, {
        group: related,
        link: related,
      }),
      gap("gap.video", 144, 24, EDIT_RATE),
      clip("clip.tail", "Tail", 168, 72, EDIT_RATE, 384, 144),
    ],
  };
  const audio: TimelineCanvasTrack = {
    id: "track.audio.1",
    name: "A1",
    kind: "audio",
    timebase: AUDIO_RATE,
    targeted: false,
    height: 72,
    locked: false,
    syncLocked: true,
    muted: false,
    solo: false,
    enabled: true,
    items: [
      clip(
        "clip.audio.long",
        "Linked production sound",
        0,
        480_000,
        AUDIO_RATE,
        0,
        480,
      ),
    ],
  };
  return {
    projectId: "project.test",
    projectName: "Editorial test",
    projectRevision: "19",
    documentSha256: "a".repeat(64),
    id: "timeline.main",
    name: "Main sequence",
    editRate: EDIT_RATE,
    globalStart: {
      value: String(GLOBAL_START_UNITS),
      timebase: EDIT_RATE,
    },
    globalStartSeconds: GLOBAL_START_SECONDS,
    startSeconds: GLOBAL_START_SECONDS,
    endSeconds: GLOBAL_START_SECONDS + 10,
    durationSeconds: 10,
    linkedSelectionEnabled: true,
    snappingEnabled: true,
    tracks: [video, audio],
  };
}

function sequentialIds(): {
  readonly allocate: TimelineIdentityAllocator;
  readonly issued: () => readonly string[];
} {
  let next = 1;
  const issued: string[] = [];
  return {
    allocate(kind: Exclude<TimelineItemKind, "transition">) {
      const id = `${kind}:${next.toString(16).padStart(32, "0")}`;
      next += 1;
      issued.push(id);
      return id;
    },
    issued: () => issued,
  };
}

function operation(
  operations: readonly TimelineEditOperation[],
  index = 0,
): TimelineEditOperation {
  const value = operations[index];
  assert.ok(value, `missing operation at index ${index}`);
  return value;
}

test("tool catalog exposes every direct professional edit with concise guidance", () => {
  assert.deepEqual(
    timelineEditingTools.map((tool) => tool.id),
    ["ripple", "roll", "slip", "slide", "razor", "trim", "extend"],
  );
  for (const tool of timelineEditingTools) {
    assert.ok(tool.label.length > 0);
    assert.ok(tool.description.length > tool.label.length);
  }
});

test("ripple and ripple extend preserve canonical sync order and exact mixed clocks", () => {
  const model = timelineModel();
  const ids = sequentialIds();
  const ripple = compileTimelineGesture({
    model,
    tool: "ripple",
    trackId: "track.video.1",
    itemId: "clip.center",
    side: "end",
    toSeconds: GLOBAL_START_SECONDS + 4.5,
    allocateId: ids.allocate,
  });
  assert.equal(ripple.label, "Ripple end to 01:00:04:12");
  assert.deepEqual(ripple.operations, [
    {
      operation: "ripple",
      timeline_id: "timeline.main",
      track_id: "track.video.1",
      target_id: { kind: "clip", id: "clip.center" },
      side: "end",
      to: { value: 108, timebase: EDIT_RATE },
      sync_adjustments: [
        {
          track_id: "track.audio.1",
          gap_id: "gap:00000000000000000000000000000001",
          fragment_ids: [
            {
              kind: "clip",
              id: "clip:00000000000000000000000000000002",
            },
          ],
        },
      ],
    },
  ]);
  assert.deepEqual(ripple.affectedItemIds, [
    "clip.center",
    "clip.right",
    "gap.video",
    "clip.tail",
    "clip.audio.long",
  ]);
  assert.equal(Object.isFrozen(ripple), true);
  assert.equal(Object.isFrozen(ripple.operations), true);
  assert.equal(Object.isFrozen(ripple.operations[0]), true);
  assert.equal(Object.isFrozen(ripple.affectedItemIds), true);

  const shrinkIds = sequentialIds();
  const extend = compileTimelineGesture({
    model,
    tool: "extend",
    extendMode: "ripple",
    trackId: "track.video.1",
    itemId: "clip.center",
    side: "end",
    toSeconds: GLOBAL_START_SECONDS + 3.5,
    allocateId: shrinkIds.allocate,
  });
  const value = operation(extend.operations);
  assert.equal(value.operation, "extend");
  if (value.operation !== "extend") return;
  assert.deepEqual(value.to, { value: 84, timebase: EDIT_RATE });
  assert.deepEqual(value.sync_adjustments, [
    {
      track_id: "track.audio.1",
      gap_id: "gap:00000000000000000000000000000001",
      fragment_ids: [
        {
          kind: "clip",
          id: "clip:00000000000000000000000000000002",
        },
      ],
    },
  ]);
});

test("fractional edit clocks retain exact frame units without display rounding drift", () => {
  const base = timelineModel();
  const globalStartSeconds = 1_001;
  const item = {
    ...clip("clip.fractional", "Fractional", 0, 240, FRACTIONAL_RATE, 0, 240),
    startSeconds: globalStartSeconds,
    endSeconds:
      globalStartSeconds + (240 * FRACTIONAL_RATE.denominator) / FRACTIONAL_RATE.numerator,
  };
  const track: TimelineCanvasTrack = {
    id: "track.video.fractional",
    name: "Fractional V1",
    kind: "video",
    timebase: FRACTIONAL_RATE,
    targeted: true,
    syncLocked: true,
    items: [item],
  };
  const model: TimelineCanvasModel = {
    ...base,
    editRate: FRACTIONAL_RATE,
    globalStart: { value: "24000", timebase: FRACTIONAL_RATE },
    globalStartSeconds,
    startSeconds: globalStartSeconds,
    endSeconds:
      globalStartSeconds + (240 * FRACTIONAL_RATE.denominator) / FRACTIONAL_RATE.numerator,
    durationSeconds:
      (240 * FRACTIONAL_RATE.denominator) / FRACTIONAL_RATE.numerator,
    tracks: [track],
  };
  const ids = sequentialIds();
  const toFrame = 241;
  const toSeconds =
    globalStartSeconds +
    (toFrame * FRACTIONAL_RATE.denominator) / FRACTIONAL_RATE.numerator;

  const ripple = compileTimelineGesture({
    model,
    tool: "ripple",
    trackId: track.id,
    itemId: item.id,
    side: "end",
    toSeconds,
    allocateId: ids.allocate,
  });
  assert.deepEqual(operation(ripple.operations), {
    operation: "ripple",
    timeline_id: model.id,
    track_id: track.id,
    target_id: { kind: "clip", id: item.id },
    side: "end",
    to: { value: toFrame, timebase: FRACTIONAL_RATE },
    sync_adjustments: [],
  });

  const insert = compileGapInsert({
    model,
    trackId: track.id,
    atSeconds:
      globalStartSeconds +
      (120 * FRACTIONAL_RATE.denominator) / FRACTIONAL_RATE.numerator,
    frameCount: 1,
    allocateId: ids.allocate,
  });
  const inserted = operation(insert.operations);
  assert.equal(inserted.operation, "insert");
  if (inserted.operation !== "insert") return;
  assert.deepEqual(inserted.material.record_range.duration, {
    value: 1,
    timebase: FRACTIONAL_RATE,
  });
});

test("roll, slip, slide, razor, trim, and roll extend emit strict public operations", () => {
  const model = timelineModel();

  const roll = compileTimelineGesture({
    model,
    tool: "roll",
    trackId: "track.video.1",
    itemId: "clip.center",
    side: "end",
    toSeconds: GLOBAL_START_SECONDS + 4.5,
    allocateId: sequentialIds().allocate,
  });
  assert.deepEqual(roll.operations, [
    {
      operation: "roll",
      timeline_id: "timeline.main",
      track_id: "track.video.1",
      left_id: { kind: "clip", id: "clip.center" },
      right_id: { kind: "clip", id: "clip.right" },
      to: { value: 108, timebase: EDIT_RATE },
    },
  ]);

  const slip = compileTimelineGesture({
    model,
    tool: "slip",
    trackId: "track.video.1",
    itemId: "clip.center",
    side: "end",
    toSeconds: GLOBAL_START_SECONDS + 2.5,
    allocateId: sequentialIds().allocate,
  });
  assert.deepEqual(slip.operations, [
    {
      operation: "slip",
      timeline_id: "timeline.main",
      track_id: "track.video.1",
      clip_id: "clip.center",
      source_start: { value: 216, timebase: SOURCE_RATE },
    },
  ]);

  const slide = compileTimelineGesture({
    model,
    tool: "slide",
    trackId: "track.video.1",
    itemId: "clip.center",
    side: "end",
    toSeconds: GLOBAL_START_SECONDS + 2.5,
    allocateId: sequentialIds().allocate,
  });
  assert.deepEqual(slide.operations, [
    {
      operation: "slide",
      timeline_id: "timeline.main",
      track_id: "track.video.1",
      clip_id: "clip.center",
      to: { value: 60, timebase: EDIT_RATE },
    },
  ]);

  const razorIds = sequentialIds();
  const razor = compileTimelineGesture({
    model,
    tool: "razor",
    trackId: "track.video.1",
    itemId: "clip.center",
    side: "end",
    toSeconds: GLOBAL_START_SECONDS + 3,
    allocateId: razorIds.allocate,
  });
  assert.deepEqual(razor.operations, [
    {
      operation: "razor",
      timeline_id: "timeline.main",
      track_id: "track.video.1",
      target_id: { kind: "clip", id: "clip.center" },
      at: { value: 72, timebase: EDIT_RATE },
      fragment_id: {
        kind: "clip",
        id: "clip:00000000000000000000000000000001",
      },
    },
  ]);

  const trimIds = sequentialIds();
  const trim = compileTimelineGesture({
    model,
    tool: "trim",
    trackId: "track.video.1",
    itemId: "clip.center",
    side: "end",
    toSeconds: GLOBAL_START_SECONDS + 3.5,
    allocateId: trimIds.allocate,
  });
  assert.deepEqual(trim.operations, [
    {
      operation: "trim",
      timeline_id: "timeline.main",
      track_id: "track.video.1",
      target_id: { kind: "clip", id: "clip.center" },
      side: "end",
      to: { value: 84, timebase: EDIT_RATE },
      gap_id: "gap:00000000000000000000000000000001",
    },
  ]);
  assert.deepEqual(trim.affectedItemIds, [
    "clip.center",
    "gap:00000000000000000000000000000001",
  ]);

  const rollExtend = compileTimelineGesture({
    model,
    tool: "extend",
    extendMode: "roll",
    trackId: "track.video.1",
    itemId: "clip.center",
    side: "end",
    toSeconds: GLOBAL_START_SECONDS + 4.5,
    allocateId: sequentialIds().allocate,
  });
  assert.deepEqual(rollExtend.operations, [
    {
      operation: "extend",
      timeline_id: "timeline.main",
      track_id: "track.video.1",
      target_id: { kind: "clip", id: "clip.center" },
      side: "end",
      to: { value: 108, timebase: EDIT_RATE },
      mode: "roll",
      sync_adjustments: [],
    },
  ]);
});

test("gap targets support razor and trim without changing typed identity domains", () => {
  const model = timelineModel();
  const razor = compileTimelineGesture({
    model,
    tool: "razor",
    trackId: "track.video.1",
    itemId: "gap.video",
    side: "end",
    toSeconds: GLOBAL_START_SECONDS + 6.5,
    allocateId: sequentialIds().allocate,
  });
  const razorOperation = operation(razor.operations);
  assert.equal(razorOperation.operation, "razor");
  if (razorOperation.operation !== "razor") return;
  assert.deepEqual(razorOperation.fragment_id, {
    kind: "gap",
    id: "gap:00000000000000000000000000000001",
  });

  const trim = compileTimelineGesture({
    model,
    tool: "trim",
    trackId: "track.video.1",
    itemId: "gap.video",
    side: "end",
    toSeconds: GLOBAL_START_SECONDS + 6.5,
    allocateId: sequentialIds().allocate,
  });
  const trimOperation = operation(trim.operations);
  assert.equal(trimOperation.operation, "trim");
});

test("ripple delete and gap closure extract every sync track atomically with exact fragments", () => {
  const model = timelineModel();
  const deletePlan = compileRippleDelete({
    model,
    trackId: "track.video.1",
    startSeconds: GLOBAL_START_SECONDS + 2.5,
    endSeconds: GLOBAL_START_SECONDS + 3.5,
    allocateId: sequentialIds().allocate,
  });
  assert.deepEqual(deletePlan.operations, [
    {
      operation: "extract",
      timeline_id: "timeline.main",
      track_id: "track.video.1",
      range: {
        start: { value: 60, timebase: EDIT_RATE },
        duration: { value: 24, timebase: EDIT_RATE },
      },
      fragment_ids: [
        {
          kind: "clip",
          id: "clip:00000000000000000000000000000001",
        },
      ],
    },
    {
      operation: "extract",
      timeline_id: "timeline.main",
      track_id: "track.audio.1",
      range: {
        start: { value: 120_000, timebase: AUDIO_RATE },
        duration: { value: 48_000, timebase: AUDIO_RATE },
      },
      fragment_ids: [
        {
          kind: "clip",
          id: "clip:00000000000000000000000000000002",
        },
      ],
    },
  ]);
  assert.deepEqual(deletePlan.affectedItemIds, [
    "clip.center",
    "clip.right",
    "gap.video",
    "clip.tail",
    "clip.audio.long",
  ]);

  const closePlan = compileGapClose({
    model,
    trackId: "track.video.1",
    gapId: "gap.video",
    allocateId: sequentialIds().allocate,
  });
  assert.equal(closePlan.operations.length, 2);
  assert.deepEqual(closePlan.operations[0], {
    operation: "extract",
    timeline_id: "timeline.main",
    track_id: "track.video.1",
    range: {
      start: { value: 144, timebase: EDIT_RATE },
      duration: { value: 24, timebase: EDIT_RATE },
    },
    fragment_ids: [],
  });
  const audioClose = operation(closePlan.operations, 1);
  assert.equal(audioClose.operation, "extract");
  if (audioClose.operation !== "extract") return;
  assert.deepEqual(audioClose.range, {
    start: { value: 288_000, timebase: AUDIO_RATE },
    duration: { value: 48_000, timebase: AUDIO_RATE },
  });
  assert.deepEqual(audioClose.fragment_ids, [
    {
      kind: "clip",
      id: "clip:00000000000000000000000000000001",
    },
  ]);
  assert.deepEqual(closePlan.affectedItemIds, [
    "gap.video",
    "clip.tail",
    "clip.audio.long",
  ]);
});

test("gap insertion uses per-track clocks, typed gap IDs, and split fragments", () => {
  const plan = compileGapInsert({
    model: timelineModel(),
    trackId: "track.video.1",
    atSeconds: GLOBAL_START_SECONDS + 3,
    frameCount: 12,
    allocateId: sequentialIds().allocate,
  });
  assert.deepEqual(plan.operations, [
    {
      operation: "insert",
      timeline_id: "timeline.main",
      track_id: "track.video.1",
      at: { value: 72, timebase: EDIT_RATE },
      material: {
        kind: "gap",
        id: "gap:00000000000000000000000000000001",
        name: "Inserted gap",
        record_range: {
          start: { value: 72, timebase: EDIT_RATE },
          duration: { value: 12, timebase: EDIT_RATE },
        },
      },
      fragment_ids: [
        {
          kind: "clip",
          id: "clip:00000000000000000000000000000002",
        },
      ],
    },
    {
      operation: "insert",
      timeline_id: "timeline.main",
      track_id: "track.audio.1",
      at: { value: 144_000, timebase: AUDIO_RATE },
      material: {
        kind: "gap",
        id: "gap:00000000000000000000000000000003",
        name: "Inserted gap",
        record_range: {
          start: { value: 144_000, timebase: AUDIO_RATE },
          duration: { value: 24_000, timebase: AUDIO_RATE },
        },
      },
      fragment_ids: [
        {
          kind: "clip",
          id: "clip:00000000000000000000000000000004",
        },
      ],
    },
  ]);
  assert.deepEqual(plan.affectedItemIds, [
    "clip.center",
    "clip.right",
    "gap.video",
    "clip.tail",
    "clip.audio.long",
  ]);
});

test("invalid gestures fail before identity allocation or operation publication", () => {
  const model = timelineModel();
  const ids = sequentialIds();
  const invalid = [
    () =>
      compileTimelineGesture({
        model,
        tool: "roll",
        trackId: "track.video.1",
        itemId: "clip.tail",
        side: "end",
        toSeconds: GLOBAL_START_SECONDS + 11,
        allocateId: ids.allocate,
      }),
    () =>
      compileTimelineGesture({
        model,
        tool: "slip",
        trackId: "track.video.1",
        itemId: "gap.video",
        side: "end",
        toSeconds: GLOBAL_START_SECONDS + 6.5,
        allocateId: ids.allocate,
      }),
    () =>
      compileTimelineGesture({
        model,
        tool: "razor",
        trackId: "track.video.1",
        itemId: "clip.center",
        side: "end",
        toSeconds: GLOBAL_START_SECONDS + 2,
        allocateId: ids.allocate,
      }),
    () =>
      compileTimelineGesture({
        model,
        tool: "ripple",
        trackId: "track.video.1",
        itemId: "clip.center",
        side: "end",
        toSeconds: GLOBAL_START_SECONDS + 4,
        allocateId: ids.allocate,
      }),
  ];
  for (const compile of invalid) {
    assert.throws(compile, TimelineEditingError);
  }

  const inexactAudioRate = { numerator: 44_100, denominator: 1 } as const;
  const inexactModel: TimelineCanvasModel = {
    ...model,
    tracks: model.tracks.map((track) =>
      track.id !== "track.audio.1"
        ? track
        : {
            ...track,
            timebase: inexactAudioRate,
            items: [
              clip(
                "clip.audio.inexact",
                "Inexact production sound",
                0,
                441_000,
                inexactAudioRate,
                0,
                480,
              ),
            ],
          },
    ),
  };
  assert.throws(
    () =>
      compileGapInsert({
        model: inexactModel,
        trackId: "track.video.1",
        atSeconds: GLOBAL_START_SECONDS + 3,
        frameCount: 1,
        allocateId: ids.allocate,
      }),
    /cannot be represented exactly/,
  );
  assert.deepEqual(ids.issued(), []);
});

test("locked primary and synchronized tracks reject edits before identity allocation", () => {
  const model = timelineModel();
  const ids = sequentialIds();
  const withTrackLock = (
    trackId: string,
  ): TimelineCanvasModel => ({
    ...model,
    tracks: model.tracks.map((track) =>
      track.id === trackId ? { ...track, locked: true } : track,
    ),
  });

  assert.throws(
    () =>
      compileTimelineGesture({
        model: withTrackLock("track.video.1"),
        tool: "razor",
        trackId: "track.video.1",
        itemId: "clip.center",
        side: "end",
        toSeconds: GLOBAL_START_SECONDS + 3,
        allocateId: ids.allocate,
      }),
    /V1 is locked/,
  );
  assert.throws(
    () =>
      compileTimelineGesture({
        model: withTrackLock("track.audio.1"),
        tool: "ripple",
        trackId: "track.video.1",
        itemId: "clip.center",
        side: "end",
        toSeconds: GLOBAL_START_SECONDS + 4.5,
        allocateId: ids.allocate,
      }),
    /A1 is locked/,
  );
  assert.throws(
    () =>
      compileGapInsert({
        model: withTrackLock("track.audio.1"),
        trackId: "track.video.1",
        atSeconds: GLOBAL_START_SECONDS + 3,
        frameCount: 12,
        allocateId: ids.allocate,
      }),
    /A1 is locked/,
  );
  assert.deepEqual(ids.issued(), []);
});

test("production timeline publishes every editing gesture through the shared project executor", () => {
  const timeline = readFileSync(
    resolve(appRoot, "src/timeline-workspace.tsx"),
    "utf8",
  );
  const editor = readFileSync(
    resolve(appRoot, "src/editor-workspaces.tsx"),
    "utf8",
  );
  const application = readFileSync(
    resolve(appRoot, "src/application-context.tsx"),
    "utf8",
  );

  assert.match(timeline, /timelineEditingTools\.map/);
  assert.match(timeline, /data-timeline-editing-tool/);
  assert.match(timeline, /compileTimelineGesture/);
  assert.match(timeline, /compileRippleDelete/);
  assert.match(timeline, /compileGapInsert/);
  assert.match(timeline, /compileGapClose/);
  assert.match(timeline, /await executeProjectActions\(\[/);
  assert.match(timeline, /action: "edit_timeline"/);
  assert.match(timeline, /operations: \[\.\.\.plan\.operations\]/);
  assert.match(editor, /executeProjectActions=\{executeProjectActions\}/);
  assert.match(
    application,
    /const executeProjectActions = useCallback[\s\S]*return await executeProjectCommand\(/,
  );
  assert.match(
    application,
    /const executeProjectCommand = useCallback[\s\S]*await refreshEditorProject\(\);[\s\S]*return result/,
  );
  assert.match(timeline, /Apply at playhead/);
  assert.match(timeline, /Nudge edit backward one frame/);
  assert.match(timeline, /Nudge edit forward one frame/);
  assert.match(timeline, /Ripple delete/);
  assert.match(timeline, /Insert gap/);
  assert.match(timeline, /Close gap/);
  assert.match(timeline, /aria-label="Timeline editing tools"/);
  assert.match(timeline, /className="timeline-edit-status"/);
  assert.match(timeline, /aria-live="polite"/);
  assert.match(timeline, /timeline-edit-preview/);
  assert.match(timeline, /EDIT_DRAG_THRESHOLD/);
  assert.match(timeline, /activeEditLocked/);
  assert.match(timeline, /operationTrackLocked/);
  assert.match(timeline, /Selection remains available, but timing edits are disabled/);
  assert.doesNotMatch(
    timeline,
    /DesktopSuperiTransport|@tauri-apps|superi\.project\.command\.execute/,
  );
});
