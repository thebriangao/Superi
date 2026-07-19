import assert from "node:assert/strict";
import test from "node:test";

import type { EditorCanonicalDocument } from "../src/api.ts";
import {
  buildMulticamAudioAction,
  buildMulticamCreationAction,
  buildMulticamDetachAction,
  buildMulticamMoveCutAction,
  buildMulticamSwitchAction,
  buildMulticamSyncAction,
  projectTimelineMulticam,
} from "../src/timeline-multicam.ts";
import { projectTimelineCatalog } from "../src/timeline-nesting.ts";

const RATE = Object.freeze({ numerator: 24, denominator: 1 });
const TARGET = "timeline:00000000000000000000000000000001";
const SOURCE = "timeline:00000000000000000000000000000002";
const TARGET_CLIP = "clip:00000000000000000000000000000011";
const SOURCE_CLIP_A = "clip:00000000000000000000000000000012";
const SOURCE_CLIP_B = "clip:00000000000000000000000000000013";
const ANGLE_A = "multicam-angle:00000000000000000000000000000021";
const ANGLE_B = "multicam-angle:00000000000000000000000000000022";

function point(value: string) {
  return { value, timebase: RATE };
}

function duration(value: string) {
  return { value, timebase: RATE };
}

function range(start: string, length: string) {
  return { start: point(start), duration: duration(length) };
}

function clip(
  id: string,
  name: string,
  source: { readonly kind: "media" | "timeline"; readonly id: string },
  recordLength = "24",
) {
  return {
    kind: "clip",
    id,
    name,
    source,
    source_range: range("0", "24"),
    record_range: range("0", recordLength),
    time_map: {
      record_duration: duration("24"),
      source_timebase: RATE,
      segments: [
        {
          record_range: range("0", "24"),
          source_start: point("0"),
          rate_numerator: "1",
          rate_denominator: "1",
        },
      ],
    },
  };
}

function videoTrack(id: string, name: string, items: readonly unknown[]) {
  return {
    id,
    name,
    semantics: { kind: "video", frame_rate: RATE, compositing: "over" },
    items,
  };
}

function timeline(
  id: string,
  name: string,
  tracks: readonly Record<string, unknown>[],
  selected: readonly { readonly kind: "clip"; readonly id: string }[] = [],
) {
  return {
    id,
    name,
    edit_rate: RATE,
    global_start: point("0"),
    tracks,
    edit_state: {
      selected_objects: selected,
      track_states: tracks.map((track) => ({
        track_id: track.id,
        height: 72,
        targeted: true,
        locked: false,
        sync_locked: true,
        muted: false,
        solo: false,
        enabled: true,
      })),
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

function canonicalDocument(
  authored: boolean,
  closeAngleRecordLength = "24",
): EditorCanonicalDocument {
  const target = timeline(
    TARGET,
    "Program",
    [
      videoTrack("track:00000000000000000000000000000031", "V1", [
        clip(TARGET_CLIP, "Interview", { kind: "timeline", id: SOURCE }),
      ]),
    ],
    [{ kind: "clip", id: TARGET_CLIP }],
  );
  const source = timeline(SOURCE, "Synchronized cameras", [
    videoTrack("track:00000000000000000000000000000032", "Wide", [
      clip(SOURCE_CLIP_A, "Wide source", {
        kind: "media",
        id: "media:00000000000000000000000000000041",
      }),
    ]),
    videoTrack("track:00000000000000000000000000000033", "Close", [
      clip(SOURCE_CLIP_B, "Close source", {
        kind: "media",
        id: "media:00000000000000000000000000000042",
      }, closeAngleRecordLength),
    ]),
  ]);
  if (authored) {
    source.multicam_source = {
      sync_method: { kind: "timecode" },
      angles: [
        {
          id: ANGLE_A,
          name: "Wide",
          camera_label: "Wide",
          enabled: true,
          metadata: [],
          source_clips: [SOURCE_CLIP_A],
        },
        {
          id: ANGLE_B,
          name: "Close",
          camera_label: "Close",
          enabled: true,
          metadata: [],
          source_clips: [SOURCE_CLIP_B],
        },
      ],
    };
    target.multicam_clips = [
      {
        clip_id: TARGET_CLIP,
        switches: [
          { source_range: range("0", "12"), angle_id: ANGLE_A },
          { source_range: range("12", "12"), angle_id: ANGLE_B },
        ],
        audio_policy: { kind: "follow_video" },
      },
    ];
  }
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
        name: "Multicam test",
        revision: "4",
        media: [],
        media_library: { bins: [], smart_collections: [] },
        timelines: [target, source],
      },
    },
  };
}

function selectedModel(document: EditorCanonicalDocument) {
  const catalog = projectTimelineCatalog(document);
  const model = catalog.byId.get(TARGET)?.model;
  const sourceModel = catalog.byId.get(SOURCE)?.model;
  assert.ok(model);
  assert.ok(sourceModel);
  const selected = model.tracks[0]?.items[0];
  assert.ok(selected);
  return { model, sourceModel, selected };
}

test("creation derives synchronized angles from source video tracks atomically", () => {
  const document = canonicalDocument(false);
  const { model, sourceModel, selected } = selectedModel(document);
  const projection = projectTimelineMulticam(document, model, selected, 0.25);
  assert.equal(projection.status, "setup");
  if (projection.status !== "setup") return;
  assert.equal(projection.canCreate, true);
  assert.equal(projection.sourceAuthored, false);

  const angleIds = [ANGLE_A, ANGLE_B];
  const action = buildMulticamCreationAction({
    targetTimelineId: model.id,
    selectedClip: selected,
    sourceModel,
    syncMethod: { kind: "timecode" },
    createAngleId: () => angleIds.shift()!,
  });
  assert.equal(action.action, "mutate_multicam");
  assert.deepEqual(action.mutations.map((mutation) => mutation.operation), [
    "set_source",
    "attach_clip",
  ]);
  assert.deepEqual(action.mutations[0], {
    operation: "set_source",
    timeline_id: SOURCE,
    source: {
      sync_method: { kind: "timecode" },
      angles: [
        {
          angle_id: ANGLE_A,
          name: "Wide",
          camera_label: "Wide",
          enabled: true,
          metadata: {},
          source_clip_ids: [SOURCE_CLIP_A],
        },
        {
          angle_id: ANGLE_B,
          name: "Close",
          camera_label: "Close",
          enabled: true,
          metadata: {},
          source_clip_ids: [SOURCE_CLIP_B],
        },
      ],
    },
  });
});

test("viewer resolves the active angle and authors exact switch and cut actions", () => {
  const document = canonicalDocument(true);
  const { model, selected } = selectedModel(document);
  const projection = projectTimelineMulticam(document, model, selected, 0.75);
  assert.equal(projection.status, "ready");
  if (projection.status !== "ready") return;
  assert.equal(projection.activeAngleId, ANGLE_B);
  assert.deepEqual(
    projection.angles.map((angle) => [angle.name, angle.available, angle.active]),
    [
      ["Wide", true, false],
      ["Close", true, true],
    ],
  );
  assert.equal(projection.cuts.length, 1);
  assert.equal(projection.cuts[0]?.recordTime.value, 12);

  assert.deepEqual(buildMulticamSwitchAction(projection, model, 0.75, ANGLE_A), {
    action: "mutate_multicam",
    mutations: [
      {
        operation: "switch_at",
        timeline_id: TARGET,
        clip_id: TARGET_CLIP,
        record_time: { value: 18, timebase: RATE },
        angle_id: ANGLE_A,
      },
    ],
  });
  assert.deepEqual(buildMulticamMoveCutAction(projection, 0, -2), {
    action: "mutate_multicam",
    mutations: [
      {
        operation: "move_cut",
        timeline_id: TARGET,
        clip_id: TARGET_CLIP,
        at_record_time: { value: 12, timebase: RATE },
        to_record_time: { value: 10, timebase: RATE },
      },
    ],
  });
});

test("sync, audio, and detach planners retain explicit reversible intent", () => {
  const document = canonicalDocument(true);
  const { model, selected } = selectedModel(document);
  const projection = projectTimelineMulticam(document, model, selected, 0.25);
  assert.equal(projection.status, "ready");
  if (projection.status !== "ready") return;

  assert.equal(
    buildMulticamSyncAction(projection, { kind: "audio" }).mutations[0]?.operation,
    "set_sync_method",
  );
  assert.deepEqual(
    buildMulticamSyncAction(projection, {
      kind: "clip_marker",
      name: "Interview sync",
    }).mutations[0],
    {
      operation: "set_sync_method",
      timeline_id: SOURCE,
      sync_method: { kind: "clip_marker", name: "Interview sync" },
    },
  );
  assert.deepEqual(
    buildMulticamAudioAction(projection, { kind: "fixed", angle_id: ANGLE_A })
      .mutations[0],
    {
      operation: "set_audio_policy",
      timeline_id: TARGET,
      clip_id: TARGET_CLIP,
      audio_policy: { kind: "fixed", angle_id: ANGLE_A },
    },
  );
  assert.equal(buildMulticamDetachAction(projection).mutations[0]?.operation, "detach_clip");
});

test("viewer distinguishes authored program state from source availability", () => {
  const document = canonicalDocument(true, "12");
  const { model, selected } = selectedModel(document);
  const projection = projectTimelineMulticam(document, model, selected, 0.75);
  assert.equal(projection.status, "ready");
  if (projection.status !== "ready") return;

  const close = projection.angles.find((angle) => angle.id === ANGLE_B);
  assert.equal(close?.active, true);
  assert.equal(close?.available, false);
  assert.throws(
    () => buildMulticamSwitchAction(projection, model, 0.75, ANGLE_B),
    /source media at the playhead/,
  );
});
