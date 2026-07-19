import assert from "node:assert/strict";
import test from "node:test";

import type { EditorCanonicalDocument } from "../src/api.ts";
import {
  buildCompoundClipAction,
  buildNestedSequenceAction,
  nestedTimelineCandidates,
  openNestedTimelinePath,
  projectTimelineCatalog,
  reconcileTimelinePath,
} from "../src/timeline-nesting.ts";
import { timelineSelectionTargets } from "../src/timeline-workspace.ts";

const videoRate = Object.freeze({ numerator: 24, denominator: 1 });
const audioRate = Object.freeze({ numerator: 48_000, denominator: 1 });

const ROOT = "timeline:00000000000000000000000000000001";
const CHILD = "timeline:00000000000000000000000000000002";
const GRANDCHILD = "timeline:00000000000000000000000000000003";

function point(value: string, timebase = videoRate) {
  return { value, timebase };
}

function duration(value: string, timebase = videoRate) {
  return { value, timebase };
}

function range(start: string, length: string, timebase = videoRate) {
  return {
    start: point(start, timebase),
    duration: duration(length, timebase),
  };
}

function clip({
  id,
  name,
  source,
  sourceRange,
  recordRange,
}: {
  readonly id: string;
  readonly name: string;
  readonly source: { readonly kind: "media" | "timeline"; readonly id: string };
  readonly sourceRange: ReturnType<typeof range>;
  readonly recordRange: ReturnType<typeof range>;
}) {
  return {
    kind: "clip",
    id,
    name,
    source,
    source_range: sourceRange,
    record_range: recordRange,
    time_map: {
      record_duration: recordRange.duration,
      source_timebase: sourceRange.start.timebase,
      segments: [
        {
          record_range: range(
            "0",
            recordRange.duration.value,
            recordRange.start.timebase,
          ),
          source_start: sourceRange.start,
          rate_numerator: "1",
          rate_denominator: "1",
        },
      ],
    },
  };
}

function trackState(trackId: string, selected = false) {
  return {
    track_id: trackId,
    height: 72,
    targeted: selected,
    locked: false,
    sync_locked: true,
    muted: false,
    solo: false,
    enabled: true,
  };
}

function timeline({
  id,
  name,
  tracks,
  selected = [],
}: {
  readonly id: string;
  readonly name: string;
  readonly tracks: readonly Record<string, unknown>[];
  readonly selected?: readonly { readonly kind: "clip"; readonly id: string }[];
}) {
  return {
    id,
    name,
    edit_rate: videoRate,
    global_start: point("0"),
    tracks,
    edit_state: {
      selected_objects: selected,
      track_states: tracks.map((track) => trackState(String(track.id), true)),
      linked_selection_enabled: true,
      links: [],
      groups: [],
    },
    snapping_enabled: true,
    markers: [],
    metadata: [],
    multicam_source: null,
    multicam_clips: [],
  };
}

function videoTrack(id: string, items: readonly Record<string, unknown>[]) {
  return {
    id,
    name: id,
    semantics: { kind: "video", frame_rate: videoRate, compositing: "over" },
    items,
  };
}

function audioTrack(id: string, items: readonly Record<string, unknown>[]) {
  return {
    id,
    name: id,
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
    items,
  };
}

function canonicalDocument(): EditorCanonicalDocument {
  const rootNested = clip({
    id: "clip:00000000000000000000000000000101",
    name: "Child sequence",
    source: { kind: "timeline", id: CHILD },
    sourceRange: range("0", "72"),
    recordRange: range("0", "72"),
  });
  const rootAudio = clip({
    id: "clip:00000000000000000000000000000102",
    name: "Guide audio",
    source: { kind: "media", id: "media:00000000000000000000000000000101" },
    sourceRange: range("0", "144000", audioRate),
    recordRange: range("0", "144000", audioRate),
  });
  const childMedia = clip({
    id: "clip:00000000000000000000000000000201",
    name: "Child picture",
    source: { kind: "media", id: "media:00000000000000000000000000000201" },
    sourceRange: range("0", "72"),
    recordRange: range("0", "72"),
  });
  const childNested = clip({
    id: "clip:00000000000000000000000000000202",
    name: "Grandchild sequence",
    source: { kind: "timeline", id: GRANDCHILD },
    sourceRange: range("0", "24"),
    recordRange: range("0", "24"),
  });
  const grandchildMedia = clip({
    id: "clip:00000000000000000000000000000301",
    name: "Grandchild picture",
    source: { kind: "media", id: "media:00000000000000000000000000000301" },
    sourceRange: range("0", "24"),
    recordRange: range("0", "24"),
  });

  return {
    resource: "superi.editor.state.timeline",
    format: "superi.timeline",
    format_revision: 2,
    byte_length: 1_024,
    sha256: "a".repeat(64),
    content: {
      format: "superi.timeline",
      format_revision: 2,
      primitive_schema_revision: 1,
      payload_sha256: "b".repeat(64),
      payload: {
        project_id: "project:00000000000000000000000000000001",
        name: "Nested editorial test",
        revision: "12",
        media: [],
        media_library: { bins: [], smart_collections: [] },
        timelines: [
          timeline({
            id: ROOT,
            name: "Root",
            tracks: [
              videoTrack("track:00000000000000000000000000000101", [rootNested]),
              audioTrack("track:00000000000000000000000000000102", [rootAudio]),
            ],
            selected: [
              { kind: "clip", id: rootNested.id },
              { kind: "clip", id: rootAudio.id },
            ],
          }),
          timeline({
            id: CHILD,
            name: "Child",
            tracks: [
              videoTrack("track:00000000000000000000000000000201", [
                childMedia,
              ]),
              videoTrack("track:00000000000000000000000000000202", [
                childNested,
              ]),
            ],
          }),
          timeline({
            id: GRANDCHILD,
            name: "Grandchild",
            tracks: [
              videoTrack("track:00000000000000000000000000000301", [
                grandchildMedia,
              ]),
            ],
          }),
        ],
      },
    },
  };
}

test("catalog projects every timeline, exact durations, dependencies, and safe candidates", () => {
  const catalog = projectTimelineCatalog(canonicalDocument());

  assert.deepEqual(
    catalog.entries.map((entry) => [entry.id, entry.name, entry.duration.value]),
    [
      [ROOT, "Root", 72],
      [CHILD, "Child", 72],
      [GRANDCHILD, "Grandchild", 24],
    ],
  );
  assert.deepEqual(catalog.byId.get(ROOT)?.childTimelineIds, [CHILD]);
  assert.deepEqual(catalog.byId.get(CHILD)?.childTimelineIds, [GRANDCHILD]);
  assert.deepEqual(
    nestedTimelineCandidates(catalog, CHILD).map((entry) => entry.id),
    [GRANDCHILD],
  );
});

test("open and reconciliation retain only real nested edges", () => {
  const catalog = projectTimelineCatalog(canonicalDocument());
  const childPath = openNestedTimelinePath(
    catalog,
    [ROOT],
    "clip:00000000000000000000000000000101",
  );
  const grandchildPath = openNestedTimelinePath(
    catalog,
    childPath,
    "clip:00000000000000000000000000000202",
  );

  assert.deepEqual(childPath, [ROOT, CHILD]);
  assert.deepEqual(grandchildPath, [ROOT, CHILD, GRANDCHILD]);
  assert.deepEqual(reconcileTimelinePath(catalog, ROOT, grandchildPath), grandchildPath);
  assert.deepEqual(reconcileTimelinePath(catalog, ROOT, [ROOT, GRANDCHILD]), [ROOT]);
  assert.throws(
    () => openNestedTimelinePath(catalog, childPath, "clip:00000000000000000000000000000201"),
    /does not reference a child timeline/,
  );
});

test("nested actions preserve exact source duration and explicit placement intent", () => {
  const catalog = projectTimelineCatalog(canonicalDocument());
  const append = buildNestedSequenceAction({
    catalog,
    parentTimelineId: ROOT,
    parentTrackId: "track:00000000000000000000000000000101",
    sourceTimelineId: GRANDCHILD,
    clipId: "clip:00000000000000000000000000000401",
    name: "Placed grandchild",
    placement: { placement: "append" },
  });
  const root = catalog.byId.get(ROOT)!;
  const target = timelineSelectionTargets(root.model).find(
    (candidate) => candidate.item.id === "clip:00000000000000000000000000000101",
  )!;
  const replace = buildNestedSequenceAction({
    catalog,
    parentTimelineId: ROOT,
    parentTrackId: target.trackId,
    sourceTimelineId: CHILD,
    clipId: "clip:00000000000000000000000000000402",
    name: "Replacement child",
    placement: { placement: "replace", target },
  });

  assert.deepEqual(append, {
    action: "place_nested_sequence",
    source_timeline_id: GRANDCHILD,
    request: {
      parent_timeline_id: ROOT,
      parent_track_id: "track:00000000000000000000000000000101",
      clip_id: "clip:00000000000000000000000000000401",
      name: "Placed grandchild",
      source_range: {
        start: { value: 0, timebase: videoRate },
        duration: { value: 24, timebase: videoRate },
      },
      placement: { placement: "append" },
    },
  });
  assert.deepEqual(replace.request.placement, {
    placement: "replace",
    target_id: { kind: "clip", id: target.item.id },
  });
  assert.equal(replace.request.source_range.duration.value, 72);
  assert.throws(
    () =>
      buildNestedSequenceAction({
        catalog,
        parentTimelineId: CHILD,
        parentTrackId: "track:00000000000000000000000000000201",
        sourceTimelineId: ROOT,
        clipId: "clip:00000000000000000000000000000403",
        name: "Cycle",
        placement: { placement: "append" },
      }),
    /would create a timeline cycle/,
  );
});

test("compound action maps selected objects and affected tracks in canonical order", () => {
  const catalog = projectTimelineCatalog(canonicalDocument());
  const root = catalog.byId.get(ROOT)!;
  const targets = timelineSelectionTargets(root.model).filter((target) =>
    target.item.id.endsWith("101") || target.item.id.endsWith("102"),
  );
  let sequence = 0;
  const next = (kind: "timeline" | "track" | "clip") => {
    sequence += 1;
    return `${kind}:${sequence.toString(16).padStart(32, "0")}`;
  };

  const action = buildCompoundClipAction({
    model: root.model,
    selectedTargets: targets,
    compoundTimelineId: next("timeline"),
    name: "Dialogue beat",
    createTrackId: () => next("track"),
    createClipId: () => next("clip"),
  });

  assert.equal(action.action, "create_compound_clip");
  assert.equal(action.request.parent_timeline_id, ROOT);
  assert.deepEqual(action.request.selected_objects, [
    { kind: "clip", id: "clip:00000000000000000000000000000101" },
    { kind: "clip", id: "clip:00000000000000000000000000000102" },
  ]);
  assert.deepEqual(
    action.request.tracks.map((mapping) => mapping.parent_track_id),
    [
      "track:00000000000000000000000000000101",
      "track:00000000000000000000000000000102",
    ],
  );
  assert.equal(new Set(action.request.tracks.map((mapping) => mapping.clip_id)).size, 2);
});
