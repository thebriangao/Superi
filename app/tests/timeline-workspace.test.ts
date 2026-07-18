import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import type { EditorCanonicalDocument } from "../src/api.ts";
import {
  TIMELINE_DEFAULT_SNAP_RULES,
  TimelineProjectionError,
  buildTimelineRulerTicks,
  clampTimelineRange,
  expandTimelineSelection,
  formatTimelineTime,
  parseTimelineSelectionIdentity,
  projectTimelineDocument,
  resolveTimelineSnap,
  timelineRectanglesIntersect,
  timelineItemsInWindow,
  timelineSelectionIdentity,
  timelineSelectionNeighbor,
  timelineSelectionRange,
  timelineSelectionTargets,
} from "../src/timeline-workspace.ts";

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const rate = Object.freeze({ numerator: 24, denominator: 1 });

function time(value: string) {
  return { value, timebase: rate };
}

function duration(value: string) {
  return { value, timebase: rate };
}

function range(start: string, length: string) {
  return { start: time(start), duration: duration(length) };
}

function rangeAt(
  start: string,
  length: string,
  timebase: { readonly numerator: number; readonly denominator: number },
) {
  return {
    start: { value: start, timebase },
    duration: { value: length, timebase },
  };
}

function rootTimeline(document: EditorCanonicalDocument): Record<string, unknown> {
  const content = document.content as Record<string, unknown>;
  const payload = content.payload as Record<string, unknown>;
  const timelines = payload.timelines as Array<Record<string, unknown>>;
  const timeline = timelines[0];
  assert.ok(timeline);
  return timeline;
}

function clip(
  id: string,
  name: string,
  sourceId: string,
  sourceStart: string,
  recordStart: string,
) {
  return {
    kind: "clip",
    id,
    name,
    source: { kind: "media", id: sourceId },
    source_range: range(sourceStart, "48"),
    record_range: range(recordStart, "48"),
    time_map: {
      record_duration: duration("48"),
      source_timebase: rate,
      segments: [
        {
          record_range: range("0", "48"),
          source_start: time(sourceStart),
          rate_numerator: "1",
          rate_denominator: "1",
        },
      ],
    },
  };
}

function canonicalDocument(): EditorCanonicalDocument {
  return {
    resource: "superi.editor.state.timeline",
    format: "superi.timeline",
    format_revision: 1,
    byte_length: 1_024,
    sha256: "a".repeat(64),
    content: {
      format: "superi.timeline",
      format_revision: 1,
      primitive_schema_revision: 1,
      payload_sha256: "b".repeat(64),
      payload: {
        project_id: "project.test",
        name: "Editorial test",
        revision: "9",
        media: [],
        media_library: { bins: [], smart_collections: [] },
        timelines: [
          {
            id: "timeline.main",
            name: "Main sequence",
            edit_rate: rate,
            global_start: time("0"),
            tracks: [
              {
                id: "track.video.1",
                name: "V1",
                semantics: {
                  kind: "video",
                  frame_rate: rate,
                  compositing: "over",
                },
                items: [
                  clip("clip.a", "Opening", "media.a", "48", "0"),
                  {
                    kind: "transition",
                    id: "transition.ab",
                    name: "Dissolve",
                    from: { kind: "clip", id: "clip.a" },
                    to: { kind: "clip", id: "clip.b" },
                    from_offset: duration("12"),
                    to_offset: duration("12"),
                  },
                  clip("clip.b", "Reaction", "media.b", "240", "48"),
                ],
              },
              {
                id: "track.audio.1",
                name: "A1",
                semantics: {
                  kind: "audio",
                  sample_rate: 48_000,
                  channel_layout: ["front_left", "front_right"],
                  routing: {
                    destination: { kind: "main" },
                    destination_layout: ["front_left", "front_right"],
                    routes: [],
                  },
                },
                items: [],
              },
            ],
            edit_state: {
              selected_objects: [{ kind: "clip", id: "clip.a" }],
              track_states: [
                {
                  track_id: "track.video.1",
                  targeted: true,
                  sync_locked: true,
                },
                {
                  track_id: "track.audio.1",
                  targeted: false,
                  sync_locked: false,
                },
              ],
              linked_selection_enabled: true,
              links: [["clip.a", "clip.b"]],
              groups: [["clip.a", "clip.b"]],
            },
            snapping_enabled: true,
            markers: [],
            metadata: [],
            multicam_source: null,
            multicam_clips: [],
          },
        ],
      },
    },
  };
}

test("canonical projection preserves exact editorial timing and relationships", () => {
  const model = projectTimelineDocument(
    canonicalDocument(),
    "timeline.main",
  );

  assert.equal(model.id, "timeline.main");
  assert.equal(model.name, "Main sequence");
  assert.deepEqual(model.editRate, rate);
  assert.equal(model.startSeconds, 0);
  assert.equal(model.endSeconds, 4);
  assert.equal(model.tracks.length, 2);

  const video = model.tracks[0];
  assert.equal(video.id, "track.video.1");
  assert.equal(video.kind, "video");
  assert.equal(video.targeted, true);
  assert.equal(video.syncLocked, true);

  const opening = video.items.find((item) => item.id === "clip.a");
  assert.ok(opening);
  assert.equal(opening.kind, "clip");
  assert.equal(opening.recordRange.start.value, "0");
  assert.equal(opening.recordRange.duration.value, "48");
  assert.equal(opening.startSeconds, 0);
  assert.equal(opening.endSeconds, 2);
  assert.deepEqual(opening.source, { kind: "media", id: "media.a" });
  assert.equal(opening.sourceRange?.start.value, "48");
  assert.equal(opening.selected, true);
  assert.deepEqual(opening.group, ["clip.a", "clip.b"]);
  assert.deepEqual(opening.link, ["clip.a", "clip.b"]);

  const transition = video.items.find(
    (item) => item.id === "transition.ab",
  );
  assert.ok(transition);
  assert.equal(transition.kind, "transition");
  assert.equal(transition.startSeconds, 1.5);
  assert.equal(transition.endSeconds, 2.5);
  assert.deepEqual(transition.transition, {
    from: { kind: "clip", id: "clip.a" },
    to: { kind: "clip", id: "clip.b" },
  });
  assert.ok(Object.isFrozen(model));
  assert.ok(Object.isFrozen(model.tracks));
  assert.ok(Object.isFrozen(video.items));
});

test("projects owner-clock targets and resolves exact configurable snap rules", () => {
  const document = canonicalDocument();
  const timeline = rootTimeline(document);
  const audioRate = Object.freeze({ numerator: 48_000, denominator: 1 });
  timeline.markers = [
    {
      id: "marker.timeline",
      owner: { kind: "timeline" },
      marked_range: range("84", "1"),
      label: "Timeline note",
      flag: null,
      note: null,
      metadata: [],
    },
    {
      id: "marker.track",
      owner: { kind: "track", id: "track.video.1" },
      marked_range: range("72", "1"),
      label: "Track note",
      flag: null,
      note: null,
      metadata: [],
    },
    {
      id: "marker.object",
      owner: { kind: "object", id: { kind: "clip", id: "clip.a" } },
      marked_range: range("12", "1"),
      label: "Preferred cut",
      flag: "cyan",
      note: null,
      metadata: [],
    },
    {
      id: "marker.inexact",
      owner: { kind: "track", id: "track.audio.1" },
      marked_range: rangeAt("1", "1", audioRate),
      label: "Subframe audio note",
      flag: null,
      note: null,
      metadata: [],
    },
    {
      id: "marker.overscan",
      owner: { kind: "object", id: { kind: "clip", id: "clip.a" } },
      marked_range: range("48", "1"),
      label: "Outside clip",
      flag: null,
      note: null,
      metadata: [],
    },
  ];

  const model = projectTimelineDocument(document, "timeline.main");
  assert.deepEqual(
    model.snapTargets
      .filter((target) => target.id.startsWith("marker."))
      .map((target) => [target.kind, target.id, target.time.value]),
    [
      ["marker_start", "marker.object", "12"],
      ["marker_end", "marker.object", "13"],
      ["marker_start", "marker.track", "72"],
      ["marker_end", "marker.track", "73"],
      ["marker_start", "marker.timeline", "84"],
      ["marker_end", "marker.timeline", "85"],
    ],
  );
  assert.ok(
    model.snapTargets.every(
      (target) =>
        target.id !== "marker.inexact" && target.id !== "marker.overscan",
    ),
  );
  assert.ok(Object.isFrozen(model.snapTargets));
  assert.ok(model.snapTargets.every(Object.isFrozen));

  const tied = resolveTimelineSnap(model, {
    atSeconds: 47 / 24,
    toleranceFrames: 1,
    playheadSeconds: null,
    rules: TIMELINE_DEFAULT_SNAP_RULES,
    sessionEnabled: true,
  });
  assert.equal(tied?.target.kind, "item_start");
  assert.equal(tied?.target.id, "clip.b");
  assert.equal(tied?.timeSeconds, 2);
  assert.equal(tied?.distanceFrames, 1);

  const withoutItemStarts = resolveTimelineSnap(model, {
    atSeconds: 47 / 24,
    toleranceFrames: 1,
    playheadSeconds: null,
    rules: { ...TIMELINE_DEFAULT_SNAP_RULES, itemStart: false },
    sessionEnabled: true,
  });
  assert.equal(withoutItemStarts?.target.kind, "item_end");
  assert.equal(withoutItemStarts?.target.id, "clip.a");

  const marker = resolveTimelineSnap(model, {
    atSeconds: 11 / 24,
    toleranceFrames: 1,
    playheadSeconds: null,
    rules: TIMELINE_DEFAULT_SNAP_RULES,
    sessionEnabled: true,
  });
  assert.equal(marker?.target.kind, "marker_start");
  assert.equal(marker?.target.id, "marker.object");

  const playhead = resolveTimelineSnap(model, {
    atSeconds: 23 / 24,
    toleranceFrames: 1,
    playheadSeconds: 1,
    rules: TIMELINE_DEFAULT_SNAP_RULES,
    sessionEnabled: true,
  });
  assert.equal(playhead?.target.kind, "playhead");
  assert.equal(playhead?.timeSeconds, 1);

  const canonicalPlayhead = resolveTimelineSnap(model, {
    atSeconds: 1,
    toleranceFrames: 1,
    playheadSeconds: 1 + Number.EPSILON,
    rules: TIMELINE_DEFAULT_SNAP_RULES,
    sessionEnabled: true,
  });
  assert.equal(canonicalPlayhead?.target.kind, "playhead");
  assert.equal(canonicalPlayhead?.timeSeconds, 1);

  const incompleteRules = { ...TIMELINE_DEFAULT_SNAP_RULES } as Record<
    string,
    boolean
  >;
  delete incompleteRules.markerEnd;
  assert.throws(
    () =>
      resolveTimelineSnap(model, {
        atSeconds: 1,
        toleranceFrames: 1,
        playheadSeconds: null,
        rules: incompleteRules as never,
        sessionEnabled: true,
      }),
    (error) =>
      error instanceof TimelineProjectionError &&
      /snap rule must be boolean \(snap\.rules\.markerEnd\)/i.test(error.message),
  );

  assert.equal(
    resolveTimelineSnap(model, {
      atSeconds: 23 / 24,
      toleranceFrames: 1,
      playheadSeconds: 1,
      rules: TIMELINE_DEFAULT_SNAP_RULES,
      sessionEnabled: false,
    }),
    null,
  );

  const disabledDocument = canonicalDocument();
  rootTimeline(disabledDocument).snapping_enabled = false;
  const disabled = projectTimelineDocument(disabledDocument, "timeline.main");
  assert.equal(
    resolveTimelineSnap(disabled, {
      atSeconds: 47 / 24,
      toleranceFrames: 1,
      playheadSeconds: null,
      rules: TIMELINE_DEFAULT_SNAP_RULES,
      sessionEnabled: true,
    }),
    null,
  );
});

test("ruler, time labels, and ranges remain deterministic across scale changes", () => {
  const ticks = buildTimelineRulerTicks({
    startSeconds: 0,
    endSeconds: 4,
    visibleStartSeconds: 0,
    visibleEndSeconds: 4,
    pixelsPerSecond: 120,
    editRate: rate,
  });

  assert.ok(ticks.length >= 5);
  assert.equal(ticks[0]?.seconds, 0);
  assert.ok(ticks.every((tick, index) => index === 0 || tick.seconds > ticks[index - 1].seconds));
  assert.ok(ticks.some((tick) => tick.major && tick.label === "00:00:02:00"));
  const frameTicks = buildTimelineRulerTicks({
    startSeconds: 0,
    endSeconds: 1,
    visibleStartSeconds: 0,
    visibleEndSeconds: 1,
    pixelsPerSecond: 800,
    editRate: rate,
  });
  assert.ok(
    frameTicks.every((tick) =>
      Math.abs(tick.seconds * 24 - Math.round(tick.seconds * 24)) < 1e-8,
    ),
  );
  assert.equal(formatTimelineTime(3.5, rate), "00:00:03:12");
  assert.deepEqual(clampTimelineRange(3.5, 1, 0, 4), {
    inPoint: 1,
    outPoint: 3.5,
  });
  assert.deepEqual(clampTimelineRange(-3, 9, 0, 4), {
    inPoint: 0,
    outPoint: 4,
  });

  const hourStart = canonicalDocument();
  const content = hourStart.content as Record<string, unknown>;
  const payload = content.payload as Record<string, unknown>;
  const timelines = payload.timelines as Array<Record<string, unknown>>;
  timelines[0].global_start = time("86400");
  const offsetModel = projectTimelineDocument(hourStart, "timeline.main");
  assert.equal(offsetModel.startSeconds, 3_600);
  assert.equal(offsetModel.endSeconds, 3_604);
  assert.equal(offsetModel.tracks[0].items[0].startSeconds, 3_600);
  assert.equal(offsetModel.tracks[0].items[1].startSeconds, 3_601.5);
  assert.equal(offsetModel.tracks[0].items[0].recordRange.start.value, "0");
  assert.equal(formatTimelineTime(offsetModel.startSeconds, rate), "01:00:00:00");

  const baseModel = projectTimelineDocument(canonicalDocument(), "timeline.main");
  assert.deepEqual(
    timelineItemsInWindow(baseModel.tracks[0].items, 0, 1).map((item) => item.id),
    ["clip.a"],
  );
  assert.deepEqual(
    timelineItemsInWindow(baseModel.tracks[0].items, 3.5, 4).map((item) => item.id),
    ["clip.b"],
  );
});

test("unsupported canonical state fails visibly instead of inventing timeline state", () => {
  const wrongFormat = canonicalDocument();
  wrongFormat.format = "superi.timeline.future";
  assert.throws(
    () => projectTimelineDocument(wrongFormat, "timeline.main"),
    (error) =>
      error instanceof TimelineProjectionError &&
      /unsupported timeline document format/i.test(error.message),
  );

  assert.throws(
    () => projectTimelineDocument(canonicalDocument(), "timeline.missing"),
    (error) =>
      error instanceof TimelineProjectionError &&
      /root timeline/i.test(error.message),
  );

  const unsafeTime = canonicalDocument();
  const content = unsafeTime.content as Record<string, unknown>;
  const payload = content.payload as Record<string, unknown>;
  const timelines = payload.timelines as Array<Record<string, unknown>>;
  const globalStart = timelines[0].global_start as Record<string, unknown>;
  globalStart.value = "9007199254740992";
  assert.throws(
    () => projectTimelineDocument(unsafeTime, "timeline.main"),
    (error) =>
      error instanceof TimelineProjectionError &&
      /safe display range/i.test(error.message),
  );

  const misplacedTransition = canonicalDocument();
  const misplacedContent = misplacedTransition.content as Record<string, unknown>;
  const misplacedPayload = misplacedContent.payload as Record<string, unknown>;
  const misplacedTimelines = misplacedPayload.timelines as Array<Record<string, unknown>>;
  const misplacedTracks = misplacedTimelines[0].tracks as Array<Record<string, unknown>>;
  const misplacedItems = misplacedTracks[0].items as unknown[];
  misplacedItems.push(misplacedItems.splice(1, 1)[0]);
  assert.throws(
    () => projectTimelineDocument(misplacedTransition, "timeline.main"),
    (error) =>
      error instanceof TimelineProjectionError &&
      /transition must sit between/i.test(error.message),
  );

  const absentMarkerOwner = canonicalDocument();
  rootTimeline(absentMarkerOwner).markers = [
    {
      id: "marker.absent",
      owner: { kind: "track", id: "track.missing" },
      marked_range: range("1", "1"),
      label: null,
      flag: null,
      note: null,
      metadata: [],
    },
  ];
  assert.throws(
    () => projectTimelineDocument(absentMarkerOwner, "timeline.main"),
    (error) =>
      error instanceof TimelineProjectionError &&
      /marker owner track track\.missing does not exist/i.test(error.message),
  );

  const mismatchedObjectMarker = canonicalDocument();
  rootTimeline(mismatchedObjectMarker).markers = [
    {
      id: "marker.wrong-clock",
      owner: { kind: "object", id: { kind: "clip", id: "clip.a" } },
      marked_range: rangeAt(
        "48",
        "1",
        Object.freeze({ numerator: 48_000, denominator: 1 }),
      ),
      label: null,
      flag: null,
      note: null,
      metadata: [],
    },
  ];
  assert.throws(
    () => projectTimelineDocument(mismatchedObjectMarker, "timeline.main"),
    (error) =>
      error instanceof TimelineProjectionError &&
      /marker range must use its owner's exact record clock/i.test(error.message),
  );
});

test("item target ties retain lower editorial object identity order", () => {
  const document = canonicalDocument();
  const timeline = rootTimeline(document);
  const tracks = timeline.tracks as Array<Record<string, unknown>>;
  tracks.unshift({
    id: "track.video.gap",
    name: "V0",
    semantics: {
      kind: "video",
      frame_rate: rate,
      compositing: "over",
    },
    items: [
      {
        kind: "gap",
        id: "clip.a",
        name: "Same ID gap",
        record_range: range("0", "48"),
      },
    ],
  });
  const editState = timeline.edit_state as Record<string, unknown>;
  const trackStates = editState.track_states as Array<Record<string, unknown>>;
  trackStates.push({
    track_id: "track.video.gap",
    targeted: false,
    sync_locked: false,
  });

  const model = projectTimelineDocument(document, "timeline.main");
  const match = resolveTimelineSnap(model, {
    atSeconds: 0,
    toleranceFrames: 0,
    playheadSeconds: null,
    rules: {
      timelineStart: false,
      playhead: false,
      itemStart: true,
      itemEnd: false,
      markerStart: false,
      markerEnd: false,
    },
    sessionEnabled: true,
  });
  assert.equal(match?.target.kind, "item_start");
  assert.deepEqual(match?.target.editorialObject, {
    kind: "clip",
    id: "clip.a",
  });
  assert.equal(match?.target.label, "Opening start");
});

test("selection helpers preserve identity, relationship, range, lasso, and navigation intent", () => {
  const document = canonicalDocument();
  const content = document.content as Record<string, unknown>;
  const payload = content.payload as Record<string, unknown>;
  const timelines = payload.timelines as Array<Record<string, unknown>>;
  const timeline = timelines[0];
  const tracks = timeline.tracks as Array<Record<string, unknown>>;
  const audioItems = tracks[1].items as unknown[];
  const audioRate = Object.freeze({ numerator: 48_000, denominator: 1 });
  audioItems.push({
    kind: "clip",
    id: "clip.c",
    name: "Room tone",
    source: { kind: "media", id: "media.c" },
    source_range: rangeAt("0", "48000", audioRate),
    record_range: rangeAt("0", "48000", audioRate),
    time_map: {
      record_duration: {
        value: "48000",
        timebase: audioRate,
      },
      source_timebase: audioRate,
      segments: [
        {
          record_range: rangeAt("0", "48000", audioRate),
          source_start: { value: "0", timebase: audioRate },
          rate_numerator: "1",
          rate_denominator: "1",
        },
      ],
    },
  });
  const editState = timeline.edit_state as Record<string, unknown>;
  editState.groups = [["clip.a", "clip.b"]];
  editState.links = [["clip.b", "clip.c"]];

  const model = projectTimelineDocument(document, "timeline.main");
  const targets = timelineSelectionTargets(model);
  const keyA = targets.find((target) => target.item.id === "clip.a")?.key;
  const keyB = targets.find((target) => target.item.id === "clip.b")?.key;
  const keyC = targets.find((target) => target.item.id === "clip.c")?.key;
  const transitionKey = targets.find(
    (target) => target.item.id === "transition.ab",
  )?.key;
  assert.ok(keyA);
  assert.ok(keyB);
  assert.ok(keyC);
  assert.ok(transitionKey);

  assert.deepEqual(expandTimelineSelection(model, [keyA]), [
    keyC,
    keyA,
    keyB,
  ]);
  assert.deepEqual(expandTimelineSelection(model, [keyA], true), [keyA]);
  assert.deepEqual(timelineSelectionRange(model, keyA, keyB), [
    keyA,
    transitionKey,
    keyB,
    keyC,
  ]);
  editState.linked_selection_enabled = false;
  const unlinkedModel = projectTimelineDocument(document, "timeline.main");
  assert.deepEqual(expandTimelineSelection(unlinkedModel, [keyA]), [keyA, keyB]);
  assert.equal(timelineSelectionNeighbor(model, keyA, "up"), keyC);
  assert.equal(timelineSelectionNeighbor(model, keyA, "right"), transitionKey);
  assert.equal(timelineSelectionNeighbor(model, keyB, "home"), keyA);
  assert.equal(timelineSelectionNeighbor(model, keyA, "end"), keyB);

  const identity = timelineSelectionIdentity("timeline/main", {
    kind: "clip",
    id: "clip / exact",
  });
  assert.deepEqual(parseTimelineSelectionIdentity(identity), {
    timelineId: "timeline/main",
    object: { kind: "clip", id: "clip / exact" },
  });
  assert.equal(parseTimelineSelectionIdentity("project:unrelated"), null);
  assert.throws(
    () =>
      timelineSelectionIdentity("timeline.main", {
        kind: "clip",
        id: "x".repeat(4_096),
      }),
    /identity is too long/i,
  );
  assert.equal(
    parseTimelineSelectionIdentity(`superi.timeline.object/${"x".repeat(4_096)}`),
    null,
  );

  assert.equal(
    timelineRectanglesIntersect(
      { left: 10, top: 10, right: 30, bottom: 30 },
      { left: 29, top: 29, right: 40, bottom: 40 },
    ),
    true,
  );
  assert.equal(
    timelineRectanglesIntersect(
      { left: 10, top: 10, right: 30, bottom: 30 },
      { left: 31, top: 31, right: 40, bottom: 40 },
    ),
    false,
  );
});

test("timeline surface is integrated without a second authored mutation owner", () => {
  const workspaces = readFileSync(
    resolve(appRoot, "src/editor-workspaces.tsx"),
    "utf8",
  );
  const timeline = readFileSync(
    resolve(appRoot, "src/timeline-workspace.tsx"),
    "utf8",
  );
  const styles = readFileSync(resolve(appRoot, "src/styles.css"), "utf8");

  assert.match(workspaces, /<TimelineWorkspace\b/);
  assert.match(workspaces, /document=\{snapshot\.timeline\.document\}/);
  assert.match(timeline, /data-timeline-canvas/);
  assert.match(timeline, /aria-label="Timeline playhead"/);
  assert.match(timeline, /aria-label="Timeline in point"/);
  assert.match(timeline, /aria-label="Timeline out point"/);
  assert.match(timeline, /Zoom out/);
  assert.match(timeline, /Fit timeline/);
  assert.match(timeline, /Command or Control/);
  assert.match(timeline, /Linked selection/);
  assert.match(timeline, /resolveTimelineSnap/);
  assert.match(timeline, /aria-label="Timeline snap target rules"/);
  assert.match(timeline, /Session target snap/);
  assert.match(timeline, /aria-live="polite"/);
  assert.match(timeline, /event\.key !== "Escape"/);
  assert.match(timeline, /timeline-snap-guide/);
  assert.match(workspaces, /selection=\{state\.selection\}/);
  assert.match(workspaces, /dispatchSelection=\{dispatch\}/);
  assert.match(timeline, /role="listbox"/);
  assert.match(timeline, /aria-multiselectable="true"/);
  assert.match(timeline, /role="option"/);
  assert.match(timeline, /aria-selected=\{interactionSelected\}/);
  assert.match(timeline, /aria-live="polite"/);
  assert.match(timeline, /timelineRectanglesIntersect/);
  assert.match(timeline, /beginSelection/);
  assert.match(timeline, /commitLasso/);
  assert.match(timeline, /timelineSelectionRange/);
  assert.match(timeline, /timelineSelectionNeighbor/);
  assert.match(timeline, /event\.key === "ArrowLeft"/);
  assert.match(timeline, /event\.key === "ArrowRight"/);
  assert.match(timeline, /event\.key === "ArrowUp"/);
  assert.match(timeline, /event\.key === "ArrowDown"/);
  assert.match(timeline, /event\.key === "Home"/);
  assert.match(timeline, /event\.key === "End"/);
  assert.match(timeline, /event\.key === "Escape"/);
  assert.match(timeline, /event\.key\.toLowerCase\(\) === "a"/);
  assert.match(timeline, /Shift-click/);
  assert.match(timeline, /Option-click/);
  assert.match(timeline, /drag empty track space/);
  assert.match(timeline, /model\?\.tracks\.slice\(\)\.reverse\(\)/);
  assert.match(timeline, /timelineItemsInWindow/);
  assert.match(styles, /\.timeline-ruler \{[\s\S]*?z-index: 9;/);
  assert.match(styles, /\.timeline-range \{[\s\S]*?z-index: 10;/);
  assert.match(styles, /\.timeline-playhead \{[\s\S]*?z-index: 11;/);
  assert.match(styles, /\.timeline-lasso \{/);
  assert.match(styles, /\.timeline-selection-status \{/);
  assert.match(styles, /\.timeline-item-authored-selected \{/);
  assert.match(styles, /\.timeline-item:focus-visible \{/);
  assert.match(styles, /button\.timeline-range-handle \{[\s\S]*?pointer-events: auto;/);
  assert.match(styles, /\.timeline-snap-controls/);
  assert.match(styles, /\.timeline-snap-guide/);
  assert.match(styles, /\.timeline-snap-status/);
  assert.doesNotMatch(
    timeline,
    /superi\.project\.command\.execute|superi\.slice|useSuperiApi|DesktopSuperiTransport|@tauri-apps/,
  );
});
