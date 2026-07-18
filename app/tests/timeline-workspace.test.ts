import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import type { EditorCanonicalDocument } from "../src/api.ts";
import {
  TimelineProjectionError,
  buildTimelineRulerTicks,
  clampTimelineRange,
  formatTimelineTime,
  projectTimelineDocument,
  timelineItemsInWindow,
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
  assert.match(timeline, /model\?\.tracks\.slice\(\)\.reverse\(\)/);
  assert.match(timeline, /timelineItemsInWindow/);
  assert.match(styles, /\.timeline-ruler \{[\s\S]*?z-index: 9;/);
  assert.match(styles, /\.timeline-range \{[\s\S]*?z-index: 10;/);
  assert.match(styles, /\.timeline-playhead \{[\s\S]*?z-index: 11;/);
  assert.match(styles, /button\.timeline-range-handle \{[\s\S]*?pointer-events: auto;/);
  assert.doesNotMatch(
    timeline,
    /superi\.project\.command\.execute|superi\.slice|useSuperiApi|DesktopSuperiTransport|@tauri-apps/,
  );
});
