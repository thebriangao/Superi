import assert from "node:assert/strict";
import test from "node:test";

import type {
  EditorAudioState,
  EditorCanonicalDocument,
  EditorPlaybackSnapshot,
  EditorStateSnapshot,
} from "../src/api.ts";
import type { SourceMonitorSnapshot } from "../src/project-lifecycle.ts";
import {
  projectViewerStatusDisplay,
  VIEWER_STATUS_FIELDS,
} from "../src/viewer-status.ts";

const DROP_RATE = Object.freeze({ numerator: 30_000, denominator: 1_001 });

function exactTime(value: string, rate = DROP_RATE) {
  return { value, timebase: rate };
}

function exactRange(start: string, duration: string, rate = DROP_RATE) {
  return {
    start: exactTime(start, rate),
    duration: { value: duration, timebase: rate },
  };
}

function canonicalDocument(
  rate = DROP_RATE,
  globalStart = "0",
  trackRate = rate,
): EditorCanonicalDocument {
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
        project_id: "project.viewer-status",
        name: "Viewer status contract",
        revision: "12",
        media: [],
        media_library: { bins: [], smart_collections: [] },
        timelines: [
          {
            id: "timeline.main",
            name: "Main sequence",
            edit_rate: rate,
            global_start: exactTime(globalStart, rate),
            tracks: [
              {
                id: "track.video.1",
                name: "V1",
                semantics: {
                  kind: "video",
                  frame_rate: trackRate,
                  compositing: "over",
                },
                items: [
                  {
                    kind: "gap",
                    id: "gap.leader",
                    name: "Leader",
                    record_range: exactRange("0", "1790", trackRate),
                  },
                  {
                    kind: "clip",
                    id: "clip.opening",
                    name: "Opening",
                    source: { kind: "media", id: "media.camera-a" },
                    source_range: exactRange("900", "100", trackRate),
                    record_range: exactRange("1790", "100", trackRate),
                    time_map: {
                      record_duration: { value: "100", timebase: trackRate },
                      source_timebase: trackRate,
                      segments: [
                        {
                          record_range: exactRange("0", "100", trackRate),
                          source_start: exactTime("900", trackRate),
                          rate_numerator: "1",
                          rate_denominator: "1",
                        },
                      ],
                    },
                  },
                  {
                    kind: "clip",
                    id: "clip.audio",
                    name: "Tail",
                    source: { kind: "media", id: "media.camera-b" },
                    source_range: exactRange("1000", "100", trackRate),
                    record_range: exactRange("1890", "100", trackRate),
                    time_map: {
                      record_duration: { value: "100", timebase: trackRate },
                      source_timebase: trackRate,
                      segments: [
                        {
                          record_range: exactRange("0", "100", trackRate),
                          source_start: exactTime("1000", trackRate),
                          rate_numerator: "1",
                          rate_denominator: "1",
                        },
                      ],
                    },
                  },
                ],
              },
            ],
            edit_state: {
              selected_objects: [{ kind: "clip", id: "clip.opening" }],
              track_states: [
                {
                  track_id: "track.video.1",
                  height: 88,
                  targeted: true,
                  locked: false,
                  sync_locked: true,
                  muted: false,
                  solo: false,
                  enabled: true,
                },
              ],
              linked_selection_enabled: true,
              links: [["clip.opening", "clip.audio"]],
              groups: [["clip.opening", "clip.audio"]],
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

const PLAYBACK = {
  mode: "playing",
  bounds: {
    start: { value: 0, timebase: DROP_RATE },
    duration: 4_000,
  },
  playhead: { value: 1_800, timebase: DROP_RATE },
  scheduled_frame: { value: 1_801, timebase: DROP_RATE },
  scheduled_due_clock: {
    value: 60_060,
    timebase: { numerator: 1_000, denominator: 1 },
  },
  rate_numerator: 1,
  rate_denominator: 1,
  direction: "forward",
  loop_range: {
    start: { value: 1_790, timebase: DROP_RATE },
    duration: 100,
  },
  epoch: 7,
  total_dropped: 9,
  consecutive_dropped: 2,
  forced_presentations: 1,
  audio_state: "discard_pending",
  discard_requested_generation: 5,
  discard_applied_generation: 4,
  degradation: ["viewport_backpressure", "audio_discard_pending"],
  failure: null,
} satisfies EditorPlaybackSnapshot;

const AUDIO_STATE = {
  audio_track_count: 1,
  tracks: [
    {
      timeline_id: "timeline.main",
      track_id: "track.audio.1",
      sample_rate: 48_000,
      source_channels: ["front_left", "front_right"],
      destination: { kind: "main" },
      destination_channels: ["front_left", "front_right"],
      routes: [
        {
          source: "front_left",
          target: { kind: "channel", channel: "front_left" },
        },
        { source: "front_right", target: { kind: "muted" } },
      ],
      clip_count: 2,
      continuity: {
        status: "audited",
        uninterrupted_record_coverage: false,
        seams: [
          {
            left_clip_id: "clip.audio.left",
            right_clip_id: "clip.audio.right",
            record: { kind: "gap", sample_count: 240 },
            source: {
              kind: "discontinuous",
              expected: 96_000,
              actual: 96_032,
            },
          },
        ],
      },
    },
  ],
  timeline_resource: "superi.editor.state.timeline",
  clip_mix: canonicalDocument(),
  automation: { status: "detached" },
} satisfies EditorAudioState;

function editorSnapshot(
  rate = DROP_RATE,
  globalStart = "0",
  timecodeMode: "drop_frame" | "non_drop_frame" = "drop_frame",
  playback: EditorPlaybackSnapshot = PLAYBACK,
): EditorStateSnapshot {
  return {
    project: {
      root_timeline_id: "timeline.main",
      settings: {
        values: {
          "superi.project.timeline.default_rate_numerator": {
            kind: "integer",
            value: rate.numerator,
          },
          "superi.project.timeline.default_rate_denominator": {
            kind: "integer",
            value: rate.denominator,
          },
          "superi.project.timeline.timecode_mode": {
            kind: "text",
            value: timecodeMode,
          },
        },
      },
    },
    timeline: { document: canonicalDocument(rate, globalStart) },
    audio: AUDIO_STATE,
    playback: { status: "attached", pending_command: true, latest: playback },
  } as unknown as EditorStateSnapshot;
}

const SOURCE_MONITOR = {
  monitor_revision: 6,
  engine_state: "ready",
  project_id: "project.viewer-status",
  project_revision: 12,
  library_revision: 4,
  media_id: "media.camera-a",
  media_name: "Camera A",
  source_fingerprint: "source-sha",
  opened_fingerprint: "source-sha",
  backend_id: "ffmpeg",
  container_id: "mov",
  stream: {
    stream_id: 2,
    kind: "video",
    codec: "prores",
    timebase_numerator: 30_000,
    timebase_denominator: 1_001,
  },
  current: {
    value: 900,
    timebase_numerator: 30_000,
    timebase_denominator: 1_001,
  },
  duration: null,
  range_start: null,
  range_end: null,
  marks: { source_fingerprint: "source-sha", in_mark: null, out_mark: null },
  marks_fresh: true,
  presentation_note: "Exact source monitor frame is ready.",
} satisfies SourceMonitorSnapshot;

test("program display separates drop-frame labels from physical playback drops", () => {
  const snapshot = editorSnapshot();
  const before = structuredClone(snapshot);
  const display = projectViewerStatusDisplay("program", snapshot, SOURCE_MONITOR);

  assert.equal(display.timecode, "00:01:00;02 (drop-frame labels)");
  assert.equal(
    display.frame,
    "record 1800; display 1800 @ 30000/1001 frames/s",
  );
  assert.match(display.source, /Opening \(clip\.opening\)/);
  assert.match(display.source, /media media\.camera-a/);
  assert.match(display.source, /source \[900, 1000\)/);
  assert.match(display.source, /record \[1790, 1890\)/);
  assert.equal(
    display.droppedFrames,
    "9 physical playback drops total; 2 consecutive; 1 forced presentation; independent of drop-frame labels.",
  );
  assert.match(display.playbackStatus, /attached; command pending; playing/);
  assert.match(display.playbackStatus, /\+1\/1x forward/);
  assert.match(display.playbackStatus, /epoch 7/);
  assert.match(display.frameCache, /mode playing/);
  assert.match(display.frameCache, /foreground frame 1801 @ 30000\/1001/);
  assert.match(display.frameCache, /due 60060 @ 1000\/1/);
  assert.match(display.frameCache, /interaction does not wait for cache work/);
  assert.match(display.frameCache, /fill, hit, and occupancy telemetry unavailable/);
  assert.match(display.visualState, /backpressured/);
  assert.match(display.audioState, /discard_pending/);
  assert.match(display.audioState, /generation 5 requested, 4 applied/);
  assert.match(
    display.audioCache,
    /transport synchronization pending discard acknowledgement/,
  );
  assert.match(display.audioCache, /discard generation 5 requested, 4 applied/);
  assert.match(display.audioCache, /sample clock 48000 Hz/);
  assert.match(display.audioCache, /source channels \[front_left, front_right\]/);
  assert.match(
    display.audioCache,
    /destination main \[front_left, front_right\]/,
  );
  assert.match(display.audioCache, /front_left -> front_left/);
  assert.match(display.audioCache, /front_right -> muted/);
  assert.match(display.audioCache, /record gap 240 samples/);
  assert.match(
    display.audioCache,
    /source discontinuity expected 96000 actual 96032/,
  );
  assert.match(display.audioCache, /audible output state not inferred/);
  assert.equal(display.comparisonState, "Single program source: clip clip.opening.");
  const liveComparison = projectViewerStatusDisplay(
    "program",
    snapshot,
    SOURCE_MONITOR,
    "Wipe vertical at 43%; current frame 18 against reference frame 7.",
  );
  assert.equal(
    liveComparison.comparisonState,
    "Wipe vertical at 43%; current frame 18 against reference frame 7.",
  );
  assert.match(display.editorialIntent, /V1 targeted, sync locked, enabled/);
  assert.match(display.editorialIntent, /clip selected/);
  assert.match(display.editorialIntent, /group clip\.opening \+ clip\.audio/);
  assert.match(display.editorialIntent, /link clip\.opening \+ clip\.audio/);
  assert.equal(Object.isFrozen(display), true);
  assert.deepEqual(snapshot, before);
});

test("non-drop display includes the canonical global start and preserves long form", () => {
  const rate = Object.freeze({ numerator: 24, denominator: 1 });
  const playback = {
    ...PLAYBACK,
    bounds: { start: { value: 0, timebase: rate }, duration: 240 },
    playhead: { value: 0, timebase: rate },
    scheduled_frame: null,
    scheduled_due_clock: null,
    loop_range: null,
  } satisfies EditorPlaybackSnapshot;
  const display = projectViewerStatusDisplay(
    "color",
    editorSnapshot(rate, "86400", "non_drop_frame", playback),
    null,
  );

  assert.equal(display.timecode, "01:00:00:00 (non-drop-frame labels)");
  assert.equal(
    display.frame,
    "record 0; display 86400 @ 24/1 frames/s",
  );

  const longForm = projectViewerStatusDisplay(
    "color",
    editorSnapshot(rate, "2073600", "non_drop_frame", playback),
    null,
  );
  assert.equal(longForm.timecode, "24:00:00:00 (non-drop-frame labels)");
});

test("source role reports retained monitor identity and exact coordinate", () => {
  const display = projectViewerStatusDisplay(
    "source",
    editorSnapshot(),
    SOURCE_MONITOR,
  );

  assert.equal(
    display.source,
    "Camera A (media.camera-a); video stream 2 prores; source 900 @ 30000/1001; ready; fingerprint current.",
  );
  assert.equal(display.comparisonState, "Source monitor view; no program comparison.");

  const sourceOnly = projectViewerStatusDisplay("source", null, SOURCE_MONITOR);
  assert.equal(sourceOnly.source, display.source);
  assert.equal(sourceOnly.comparisonState, display.comparisonState);
  assert.match(sourceOnly.editorialIntent, /monitor revision 6/);
  assert.match(sourceOnly.timecode, /^Unavailable:/);
});

test("unobserved owners remain explicit and never become guessed success", () => {
  const display = projectViewerStatusDisplay("composite", null, null);

  assert.equal(display.timecode, "Unavailable: editor state has not been observed.");
  assert.equal(display.frame, "Unavailable: editor state has not been observed.");
  assert.equal(display.source, "Unavailable: editor state has not been observed.");
  assert.equal(display.droppedFrames, "Unavailable: playback has not been observed.");
  assert.equal(display.playbackStatus, "Detached: editor state has not been observed.");
  assert.equal(display.frameCache, "Unavailable: playback has not been observed.");
  assert.equal(display.visualState, "Unavailable: playback has not been observed.");
  assert.equal(display.audioState, "Unavailable: playback has not been observed.");
  assert.equal(display.audioCache, "Unavailable: playback has not been observed.");
  assert.equal(display.comparisonState, "Unavailable: editor state has not been observed.");
  assert.equal(display.editorialIntent, "Unavailable: editorial state has not been observed.");
});

test("cache indicators preserve exact viewer state through every transport mode", () => {
  assert.deepEqual(
    VIEWER_STATUS_FIELDS.map((field) => field.key),
    [
      "timecode",
      "frame",
      "source",
      "droppedFrames",
      "playbackStatus",
      "frameCache",
      "visualState",
      "audioState",
      "audioCache",
      "comparisonState",
      "editorialIntent",
    ],
  );

  for (const mode of ["paused", "playing", "scrubbing", "ended"]) {
    const playback = {
      ...PLAYBACK,
      mode,
      audio_state: "synchronized",
      discard_applied_generation: 5,
      degradation: ["viewport_backpressure"],
    } satisfies EditorPlaybackSnapshot;
    const display = projectViewerStatusDisplay(
      "program",
      editorSnapshot(DROP_RATE, "0", "drop_frame", playback),
      SOURCE_MONITOR,
      `Compare current and reference while ${mode}.`,
    );

    assert.match(display.timecode, /00:01:00;02/);
    assert.match(display.frame, /record 1800/);
    assert.match(display.playbackStatus, new RegExp(`; ${mode};`));
    assert.match(display.frameCache, new RegExp(`mode ${mode}`));
    assert.match(display.visualState, /backpressured/);
    assert.match(display.audioState, /synchronized/);
    assert.match(display.audioCache, /transport synchronization current/);
    assert.match(display.audioCache, /sample clock 48000 Hz/);
    assert.equal(
      display.comparisonState,
      `Compare current and reference while ${mode}.`,
    );
  }
});

test("timing-only output and malformed audio evidence stay explicit", () => {
  const timingOnly = {
    ...PLAYBACK,
    audio_state: "synchronized",
    discard_applied_generation: 5,
    degradation: [
      "prefetch_failure",
      "viewport_output_unavailable",
      "audio_output_unavailable",
    ],
  } satisfies EditorPlaybackSnapshot;
  const timingOnlyDisplay = projectViewerStatusDisplay(
    "program",
    editorSnapshot(DROP_RATE, "0", "drop_frame", timingOnly),
    SOURCE_MONITOR,
  );
  assert.match(timingOnlyDisplay.frameCache, /predictive cache failed/);
  assert.match(timingOnlyDisplay.frameCache, /decoded viewport output unavailable/);
  assert.match(timingOnlyDisplay.audioCache, /device audio output unavailable/);
  assert.match(timingOnlyDisplay.audioCache, /audible continuity not claimed/);

  const malformed = structuredClone(editorSnapshot());
  malformed.audio.tracks[0].sample_rate = 0;
  const malformedDisplay = projectViewerStatusDisplay(
    "program",
    malformed,
    SOURCE_MONITOR,
  );
  assert.match(malformedDisplay.audioCache, /^Unavailable:/);
  assert.match(malformedDisplay.audioState, /discard_pending/);
  assert.match(malformedDisplay.frameCache, /mode playing/);
  assert.equal(Object.isFrozen(malformedDisplay), true);
});

test("invalid settings and inexact track clocks fail closed without hiding independent state", () => {
  const malformed = structuredClone(editorSnapshot());
  malformed.project.settings.values[
    "superi.project.timeline.timecode_mode"
  ] = { kind: "text", value: "guess" };
  const malformedDisplay = projectViewerStatusDisplay(
    "program",
    malformed,
    SOURCE_MONITOR,
  );
  assert.match(malformedDisplay.timecode, /^Unavailable:/);
  assert.match(malformedDisplay.frame, /^Unavailable:/);
  assert.match(malformedDisplay.droppedFrames, /^9 physical playback drops/);
  assert.match(malformedDisplay.playbackStatus, /^attached; command pending; playing/);

  const rootRate = Object.freeze({ numerator: 24, denominator: 1 });
  const trackRate = Object.freeze({ numerator: 30, denominator: 1 });
  const playback = {
    ...PLAYBACK,
    bounds: { start: { value: 0, timebase: rootRate }, duration: 240 },
    playhead: { value: 1, timebase: rootRate },
    scheduled_frame: null,
    scheduled_due_clock: null,
    loop_range: null,
  } satisfies EditorPlaybackSnapshot;
  const exactRoot = editorSnapshot(
    rootRate,
    "0",
    "non_drop_frame",
    playback,
  );
  const inexactTrack = {
    ...exactRoot,
    timeline: {
      ...exactRoot.timeline,
      document: canonicalDocument(rootRate, "0", trackRate),
    },
  } satisfies EditorStateSnapshot;
  const inexactDisplay = projectViewerStatusDisplay(
    "program",
    inexactTrack,
    SOURCE_MONITOR,
  );
  assert.match(inexactDisplay.source, /^Unavailable:/);
  assert.match(inexactDisplay.source, /cannot be represented exactly/);
  assert.match(inexactDisplay.comparisonState, /^Unavailable:/);
  assert.match(inexactDisplay.editorialIntent, /^Unavailable:/);
});
