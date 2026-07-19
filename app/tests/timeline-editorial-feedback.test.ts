import assert from "node:assert/strict";
import test from "node:test";

import type { EditorAudioState } from "../src/api.ts";
import type { TimelineClipPresentation } from "../src/timeline-clip-presentation.ts";
import {
  projectTimelineEditorialFeedback,
  type TimelineEditorialFeedbackOptions,
} from "../src/timeline-editorial-feedback.ts";
import type { TimelineEditPlan } from "../src/timeline-editing.ts";
import type { TimelineCanvasModel } from "../src/timeline-workspace.ts";

const EDIT_RATE = Object.freeze({ numerator: 24, denominator: 1 });
const SOURCE_RATE = Object.freeze({ numerator: 48_000, denominator: 1 });

function baseOptions(): TimelineEditorialFeedbackOptions {
  const model = {
    projectId: "project.feedback",
    projectName: "Feedback",
    projectRevision: "12",
    documentSha256: "timeline-sha",
    id: "timeline.main",
    name: "Main",
    editRate: EDIT_RATE,
    globalStart: { value: "0", timebase: EDIT_RATE },
    globalStartSeconds: 0,
    startSeconds: 0,
    endSeconds: 8,
    durationSeconds: 8,
    linkedSelectionEnabled: true,
    snappingEnabled: true,
    markers: [],
    snapTargets: [],
    tracks: [
      {
        id: "track.video.1",
        name: "V1",
        kind: "video",
        timebase: EDIT_RATE,
        targeted: true,
        height: 80,
        locked: false,
        syncLocked: true,
        muted: false,
        solo: false,
        enabled: true,
        items: [
          {
            kind: "clip",
            id: "clip.hero",
            name: "Hero",
            startSeconds: 2,
            endSeconds: 4,
            recordRange: {
              start: { value: "48", timebase: EDIT_RATE },
              duration: { value: "48", timebase: EDIT_RATE },
            },
            source: { kind: "media", id: "media.hero" },
            sourceRange: {
              start: { value: "96000", timebase: SOURCE_RATE },
              duration: { value: "96000", timebase: SOURCE_RATE },
            },
            transition: null,
            selected: true,
            group: null,
            link: null,
          },
        ],
      },
    ],
  } as TimelineCanvasModel;
  const clip = {
    id: "clip.hero",
    name: "Hero",
    timelineId: "timeline.main",
    trackId: "track.video.1",
    trackName: "V1",
    trackKind: "video",
    targeted: true,
    syncLocked: true,
    source: {
      kind: "media",
      id: "media.hero",
      name: "Hero source",
      target: "hero.mov",
      relinkStatus: "online",
    },
    sourceRange: {
      start: { value: "96000", timebase: SOURCE_RATE },
      duration: { value: "96000", timebase: SOURCE_RATE },
    },
    recordRange: {
      start: { value: "48", timebase: EDIT_RATE },
      duration: { value: "48", timebase: EDIT_RATE },
    },
    timeMap: {
      recordDuration: { value: "48", timebase: EDIT_RATE },
      sourceTimebase: SOURCE_RATE,
      segments: [],
    },
    startSeconds: 2,
    endSeconds: 4,
    geometry: { leftPercent: 25, widthPercent: 25 },
    canonicalSelected: true,
    retimed: false,
    linkedClipIds: [],
    groupedClipIds: [],
    markers: [],
    metadataKeys: [],
    multicam: null,
    effects: [],
    automation: null,
  } as TimelineClipPresentation;
  const audio = {
    audio_track_count: 0,
    tracks: [],
    timeline_resource: "superi.timeline",
    clip_mix: {
      resource: "superi.clip-mix",
      format: "superi.clip-mix",
      format_revision: 1,
      byte_length: 2,
      sha256: "clip-mix-sha",
      content: {},
    },
    automation: { status: "detached" },
  } as EditorAudioState;
  return {
    model,
    clips: [clip],
    audio,
    tool: "slip",
    target: { trackId: "track.video.1", itemId: "clip.hero" },
    plan: null,
    playheadSeconds: 3,
    phase: "idle",
    message: "Ready for an exact edit.",
  };
}

test("slip feedback exposes the proposed source start while holding the record range", () => {
  const plan = {
    label: "Slip to source 120000",
    operations: [
      {
        operation: "slip",
        timeline_id: "timeline.main",
        track_id: "track.video.1",
        clip_id: "clip.hero",
        source_start: {
          value: 120_000,
          timebase: { numerator: 48_000, denominator: 1 },
        },
      },
    ],
    previewSeconds: 3,
    affectedItemIds: ["clip.hero"],
  } as TimelineEditPlan;
  const feedback = projectTimelineEditorialFeedback({
    ...baseOptions(),
    plan,
    phase: "preview",
    message: "Slip preview is active.",
  });

  assert.equal(feedback.source.title, "Slip source");
  assert.equal(feedback.source.coordinate, "120000 @ 48000/1");
  assert.equal(feedback.program.title, "Record range held");
  assert.equal(feedback.program.coordinate, "48+48 @ 24/1");
  assert.equal(feedback.phase, "preview");
  assert.equal(feedback.source.phase, "preview");
  assert.equal(feedback.program.phase, "preview");
  assert.equal(Object.isFrozen(feedback), true);
  assert.equal(Object.isFrozen(feedback.source), true);
});

test("trim and slide feedback retain distinct source and record consequences", () => {
  const trim = projectTimelineEditorialFeedback({
    ...baseOptions(),
    tool: "trim",
    phase: "preview",
    plan: {
      label: "Trim end",
      operations: [
        {
          operation: "trim",
          timeline_id: "timeline.main",
          track_id: "track.video.1",
          target_id: { kind: "clip", id: "clip.hero" },
          side: "end",
          to: { value: 84, timebase: EDIT_RATE },
          gap_id: "gap.preview",
        },
      ],
      previewSeconds: 3.5,
      affectedItemIds: ["clip.hero", "gap.preview"],
    },
  });
  assert.equal(trim.source.title, "Trim end source");
  assert.equal(trim.source.coordinate, "96000+96000 @ 48000/1");
  assert.equal(trim.program.title, "Trim end boundary");
  assert.equal(trim.program.coordinate, "84 @ 24/1");

  const slide = projectTimelineEditorialFeedback({
    ...baseOptions(),
    tool: "slide",
    phase: "preview",
    plan: {
      label: "Slide",
      operations: [
        {
          operation: "slide",
          timeline_id: "timeline.main",
          track_id: "track.video.1",
          clip_id: "clip.hero",
          to: { value: 24, timebase: EDIT_RATE },
        },
      ],
      previewSeconds: 1,
      affectedItemIds: ["clip.before", "clip.hero", "clip.after"],
    },
  });
  assert.equal(slide.source.title, "Source range held");
  assert.equal(slide.source.coordinate, "96000+96000 @ 48000/1");
  assert.equal(slide.program.title, "Slide record start");
  assert.equal(slide.program.coordinate, "24 @ 24/1");
  assert.match(slide.program.detail, /3 canonical objects/);

  const unrelated = projectTimelineEditorialFeedback({
    ...baseOptions(),
    plan: {
      label: "Unrelated slip",
      operations: [
        {
          operation: "slip",
          timeline_id: "timeline.main",
          track_id: "track.video.2",
          clip_id: "clip.other",
          source_start: { value: 7, timebase: EDIT_RATE },
        },
      ],
      previewSeconds: 3,
      affectedItemIds: ["clip.other"],
    },
  });
  assert.equal(unrelated.source.title, "Source context");
  assert.equal(unrelated.program.title, "Program range");
});

test("multicam feedback retains angle identities, switch ranges, and audio policy", () => {
  const options = baseOptions();
  const clip = options.clips[0];
  assert.ok(clip);
  const multicam: NonNullable<TimelineClipPresentation["multicam"]> = {
    syncMethod: "timecode",
    switchCount: 2,
    audioPolicy: "fixed",
    angles: [
      {
        id: "angle.a",
        name: "Camera A",
        cameraLabel: "A",
        enabled: true,
        sourceClipIds: ["clip.camera.a"],
      },
      {
        id: "angle.b",
        name: "Camera B",
        cameraLabel: "B",
        enabled: false,
        sourceClipIds: ["clip.camera.b"],
      },
    ],
    switches: [
      {
        sourceRange: {
          start: { value: "96000", timebase: SOURCE_RATE },
          duration: { value: "48000", timebase: SOURCE_RATE },
        },
        angleId: "angle.a",
      },
      {
        sourceRange: {
          start: { value: "144000", timebase: SOURCE_RATE },
          duration: { value: "48000", timebase: SOURCE_RATE },
        },
        angleId: "angle.b",
      },
    ],
    audioPolicyDetail: { kind: "fixed", angleId: "angle.a" },
  };
  const feedback = projectTimelineEditorialFeedback({
    ...options,
    clips: [
      {
        ...clip,
        multicam,
      },
    ],
  });

  assert.equal(feedback.source.title, "Multicam source context");
  assert.deepEqual(feedback.source.multicam?.angles.map((angle) => angle.id), [
    "angle.a",
    "angle.b",
  ]);
  assert.deepEqual(feedback.source.multicam?.switches, [
    {
      sourceRange: {
        start: { value: "96000", timebase: SOURCE_RATE },
        duration: { value: "48000", timebase: SOURCE_RATE },
      },
      angleId: "angle.a",
    },
    {
      sourceRange: {
        start: { value: "144000", timebase: SOURCE_RATE },
        duration: { value: "48000", timebase: SOURCE_RATE },
      },
      angleId: "angle.b",
    },
  ]);
  assert.deepEqual(feedback.source.multicam?.audioPolicyDetail, {
    kind: "fixed",
    angleId: "angle.a",
  });
  assert.equal(Object.isFrozen(multicam), false);
  assert.equal(Object.isFrozen(multicam.angles), false);
});

test("audio feedback preserves sample clocks, channel order, routing, solo state, and seams", () => {
  const options = baseOptions();
  const audioTrack = (id: string, solo: boolean) => ({
    id,
    name: id,
    kind: "audio" as const,
    timebase: SOURCE_RATE,
    targeted: false,
    height: 64,
    locked: false,
    syncLocked: true,
    muted: false,
    solo,
    enabled: true,
    items: [],
  });
  const audio = {
    ...options.audio,
    audio_track_count: 2,
    tracks: [
      {
        timeline_id: "timeline.main",
        track_id: "track.audio.dialogue",
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
        clip_count: 3,
        continuity: {
          status: "audited",
          uninterrupted_record_coverage: false,
          seams: [
            {
              left_clip_id: "clip.dialogue.1",
              right_clip_id: "clip.dialogue.2",
              record: { kind: "gap", sample_count: 128 },
              source: {
                kind: "discontinuous",
                expected: 96_000,
                actual: 96_256,
              },
            },
          ],
        },
      },
      {
        timeline_id: "timeline.main",
        track_id: "track.audio.music",
        sample_rate: 96_000,
        source_channels: ["front_left"],
        destination: { kind: "track", track_id: "bus.music" },
        destination_channels: ["front_center"],
        routes: [
          {
            source: "front_left",
            target: { kind: "channel", channel: "front_center" },
          },
        ],
        clip_count: 1,
        continuity: {
          status: "audited",
          uninterrupted_record_coverage: true,
          seams: [],
        },
      },
    ],
  } as EditorAudioState;
  const feedback = projectTimelineEditorialFeedback({
    ...options,
    model: {
      ...options.model,
      tracks: [
        ...options.model.tracks,
        audioTrack("track.audio.dialogue", false),
        audioTrack("track.audio.music", true),
      ],
    },
    audio,
  });

  assert.equal(feedback.audio.signalStatus, "unobserved");
  assert.doesNotMatch(
    JSON.stringify(feedback.audio),
    /"(?:samplePeak|truePeak|rms|level)":/,
  );
  assert.deepEqual(feedback.audio.tracks.map((track) => track.trackId), [
    "track.audio.dialogue",
    "track.audio.music",
  ]);
  const dialogue = feedback.audio.tracks[0];
  assert.ok(dialogue);
  assert.equal(dialogue.sampleRate, 48_000);
  assert.deepEqual(dialogue.sourceChannels, ["front_left", "front_right"]);
  assert.equal(dialogue.audibility, "solo_suppressed");
  assert.deepEqual(dialogue.routes, [
    {
      source: "front_left",
      target: "front_left",
      state: "solo_suppressed",
    },
    { source: "front_right", target: null, state: "solo_suppressed" },
  ]);
  assert.deepEqual(dialogue.continuity, {
    status: "audited",
    uninterruptedRecordCoverage: false,
    seams: [
      {
        leftClipId: "clip.dialogue.1",
        rightClipId: "clip.dialogue.2",
        recordKind: "gap",
        recordSampleCount: 128,
        sourceKind: "discontinuous",
        sourceExpected: 96_000,
        sourceActual: 96_256,
        sourceLeft: null,
        sourceRight: null,
      },
    ],
  });
  const music = feedback.audio.tracks[1];
  assert.ok(music);
  assert.equal(music.sampleRate, 96_000);
  assert.equal(music.destination, "track:bus.music");
  assert.equal(music.audibility, "audible");
  assert.equal(music.routes[0]?.state, "routed");
  assert.equal(Object.isFrozen(dialogue.routes), true);
  assert.equal(Object.isFrozen(dialogue.continuity), true);

  const missingCanvasTrack = projectTimelineEditorialFeedback({
    ...options,
    audio: {
      ...audio,
      audio_track_count: 1,
      tracks: [audio.tracks[0]!],
    },
  });
  assert.equal(missingCanvasTrack.audio.tracks[0]?.audibility, "unavailable");
  assert.equal(missingCanvasTrack.audio.tracks[0]?.routes[0]?.state, "unavailable");
});

test("missing clip state remains explicit without hiding canonical audio feedback", () => {
  const options = baseOptions();
  const feedback = projectTimelineEditorialFeedback({
    ...options,
    clips: [],
    target: null,
    phase: "failed",
    message: "Canonical clip detail is unavailable.",
  });
  assert.equal(feedback.source.title, "Source unavailable");
  assert.equal(feedback.source.coordinate, null);
  assert.equal(feedback.program.title, "Program playhead");
  assert.equal(feedback.phase, "failed");
  assert.equal(feedback.audio.signalStatus, "unobserved");
});
