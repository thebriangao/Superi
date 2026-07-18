import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";

import type { EditorStateSnapshot } from "../src/api.ts";
import {
  formatTimelineClipTiming,
  projectTimelineClips,
  timelineClipAutomationKeyPercent,
} from "../src/timeline-clip-presentation.ts";

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

const rate = (numerator: number, denominator = 1) => ({
  numerator,
  denominator,
});

const point = (value: string, numerator: number, denominator = 1) => ({
  value,
  timebase: rate(numerator, denominator),
});

const duration = (value: string, numerator: number, denominator = 1) => ({
  value,
  timebase: rate(numerator, denominator),
});

const range = (
  start: string,
  length: string,
  numerator: number,
  denominator = 1,
) => ({
  start: point(start, numerator, denominator),
  duration: duration(length, numerator, denominator),
});

function completeSnapshot(): EditorStateSnapshot {
  const timelineEnvelope = {
    format: "superi.timeline",
    format_revision: 1,
    primitive_schema_revision: 1,
    payload_sha256: "a".repeat(64),
    payload: {
      project_id: "project.cut",
      name: "Launch cut",
      revision: "17",
      media: [
        {
          id: "media.video",
          name: "Arrival A",
          target: "/media/arrival-a.mov",
          available_range: range("0", "240", 24),
          metadata: [],
          relink_state: {
            status: "online",
            expected_fingerprint: "video-fingerprint",
            observed_fingerprint: "video-fingerprint",
            rejected_target: null,
          },
        },
        {
          id: "media.audio",
          name: "Dialogue A",
          target: "/media/dialogue-a.wav",
          available_range: range("0", "192000", 48_000),
          metadata: [],
          relink_state: {
            status: "unverified",
            expected_fingerprint: null,
            observed_fingerprint: null,
            rejected_target: null,
          },
        },
      ],
      media_library: { bins: [], smart_collections: [] },
      timelines: [
        {
          id: "timeline.main",
          name: "Main timeline",
          edit_rate: rate(24),
          global_start: point("86400", 24),
          tracks: [
            {
              id: "track.video",
              name: "V1",
              semantics: {
                kind: "video",
                frame_rate: rate(24),
                compositing: "over",
              },
              items: [
                {
                  kind: "clip",
                  id: "clip.video",
                  name: "Arrival close",
                  source: { kind: "media", id: "media.video" },
                  source_range: range("24", "48", 24),
                  record_range: range("0", "48", 24),
                  time_map: {
                    record_duration: duration("48", 24),
                    source_timebase: rate(24),
                    segments: [
                      {
                        record_range: range("0", "48", 24),
                        source_start: point("24", 24),
                        rate_numerator: "2",
                        rate_denominator: "1",
                      },
                    ],
                  },
                },
              ],
            },
            {
              id: "track.audio",
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
              items: [
                {
                  kind: "gap",
                  id: "gap.audio.lead",
                  name: "Lead gap",
                  record_range: range("0", "48000", 48_000),
                },
                {
                  kind: "clip",
                  id: "clip.audio",
                  name: "Dialogue take 3",
                  source: { kind: "media", id: "media.audio" },
                  source_range: range("48000", "48000", 48_000),
                  record_range: range("48000", "48000", 48_000),
                  time_map: {
                    record_duration: duration("48000", 48_000),
                    source_timebase: rate(48_000),
                    segments: [
                      {
                        record_range: range("0", "48000", 48_000),
                        source_start: point("48000", 48_000),
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
            selected_objects: [{ kind: "clip", id: "clip.video" }],
            track_states: [
              { track_id: "track.video", targeted: true, sync_locked: true },
              { track_id: "track.audio", targeted: false, sync_locked: false },
            ],
            linked_selection_enabled: true,
            links: [["clip.video", "clip.audio"]],
            groups: [["clip.video", "clip.audio"]],
          },
          snapping_enabled: true,
          markers: [
            {
              id: "marker.video",
              owner: {
                kind: "object",
                id: { kind: "clip", id: "clip.video" },
              },
              marked_range: range("12", "1", 24),
              label: "Preferred cut",
              flag: "cyan",
              note: "Keep this reaction",
              metadata: [],
            },
          ],
          metadata: [
            {
              owner: {
                kind: "object",
                id: { kind: "clip", id: "clip.video" },
              },
              entries: [
                {
                  key: "editorial.status",
                  value: { kind: "text", value: "approved" },
                },
              ],
            },
          ],
          multicam_source: {
            sync_method: { kind: "timecode" },
            angles: [
              {
                id: "angle.a",
                name: "Camera A",
                camera_label: "A",
                enabled: true,
                metadata: [],
                source_clips: ["clip.video"],
              },
            ],
          },
          multicam_clips: [
            {
              clip_id: "clip.video",
              switches: [
                {
                  source_range: range("24", "48", 24),
                  angle_id: "angle.a",
                },
              ],
              audio_policy: { kind: "follow_video" },
            },
          ],
        },
      ],
    },
  };

  const graphEnvelope = {
    format: "superi.graph",
    format_revision: 1,
    primitive_schema_revision: 1,
    payload_sha256: "b".repeat(64),
    payload: {
      graph_id: "graph.main",
      revision: "9",
      schemas: [],
      nodes: [
        {
          id: "node.clip.video",
          schema: {
            node_type: "superi.timeline.video.clip",
            schema_version: { major: 1, minor: 0, patch: 0 },
          },
          inputs: [],
          outputs: [{ id: "port.clip.out", name: "content" }],
          parameters: [
            {
              id: "parameter.clip.object",
              name: "object-id",
              value_type: "superi.value.timeline.object-id",
              payload: {
                domain: {
                  kind: "editorial_object_id",
                  value: { kind: "clip", id: "clip.video" },
                },
              },
            },
          ],
        },
        {
          id: "node.effect.grade",
          schema: {
            node_type: "superi.effects.color.primary",
            schema_version: { major: 1, minor: 0, patch: 0 },
          },
          inputs: [{ id: "port.effect.in", name: "image" }],
          outputs: [{ id: "port.effect.out", name: "image" }],
          parameters: [
            {
              id: "parameter.effect.mix",
              name: "mix",
              value_type: "superi.value.scalar",
              payload: { scalar: 1 },
            },
          ],
        },
        {
          id: "node.track.video",
          schema: {
            node_type: "superi.timeline.video.track",
            schema_version: { major: 1, minor: 0, patch: 0 },
          },
          inputs: [{ id: "port.track.in", name: "items" }],
          outputs: [],
          parameters: [],
        },
        {
          id: "node.effect.track",
          schema: {
            node_type: "superi.effects.color.output",
            schema_version: { major: 1, minor: 0, patch: 0 },
          },
          inputs: [],
          outputs: [],
          parameters: [],
        },
      ],
      edges: [
        {
          id: "edge.clip.effect",
          source: { node_id: "node.clip.video", port_id: "port.clip.out" },
          destination: {
            node_id: "node.effect.grade",
            port_id: "port.effect.in",
          },
        },
        {
          id: "edge.effect.track",
          source: { node_id: "node.effect.grade", port_id: "port.effect.out" },
          destination: {
            node_id: "node.track.video",
            port_id: "port.track.in",
          },
        },
      ],
      node_order: [
        "node.clip.video",
        "node.effect.grade",
        "node.track.video",
        "node.effect.track",
      ],
      parameter_drivers: [
        {
          target: {
            node_id: "node.effect.grade",
            parameter_id: "parameter.effect.mix",
          },
          value_type: "superi.value.scalar",
          driver: {
            kind: "expression",
            source: "mix",
            variables: [],
          },
        },
      ],
    },
  };

  return {
    schema_version: { major: 1, minor: 0, patch: 0 },
    project: {
      project_id: "project.cut",
      root_timeline_id: "timeline.main",
      project_revision: 17,
    },
    timeline: {
      timeline_count: 1,
      document: {
        resource: "superi.editor.state.timeline",
        format: "superi.timeline",
        format_revision: 1,
        byte_length: 1,
        sha256: "c".repeat(64),
        content: timelineEnvelope,
      },
    },
    graph: {
      documents: [
        {
          graph_id: "graph.main",
          graph_revision: 9,
          scope: { kind: "timeline", root_timeline_id: "timeline.main" },
          document: {
            resource: "superi.editor.state.graph.graph.main",
            format: "superi.graph",
            format_revision: 1,
            byte_length: 1,
            sha256: "d".repeat(64),
            content: graphEnvelope,
          },
        },
      ],
    },
    audio: {
      automation: {
        status: "attached",
        state: {
          schema_version: { major: 1, minor: 0, patch: 0 },
          revision: 4,
          lanes: [
            {
              target: { kind: "clip_gain", clip_id: "clip.audio" },
              sample_rate: 48_000,
              default_gain: 1,
              mode: { kind: "touch" },
              keyframes: [
                { at: { sample: 48_000, sample_rate: 48_000 }, value: 0.8 },
                { at: { sample: 96_000, sample_rate: 48_000 }, value: 1 },
              ],
              active_pass: {
                start: { sample: 48_000, sample_rate: 48_000 },
                current_value: 0.9,
                touch_active: true,
                latch_active: false,
              },
            },
          ],
        },
      },
    },
  } as unknown as EditorStateSnapshot;
}

test("enriches the existing exact canvas projection with clip detail", () => {
  const projection = projectTimelineClips(completeSnapshot());

  assert.equal(projection.status, "ready");
  if (projection.status !== "ready") return;
  assert.equal(projection.projectRevision, 17);
  assert.equal(projection.timelineId, "timeline.main");
  assert.equal(projection.clips.length, 2);

  const video = projection.clips[0];
  assert.equal(video.id, "clip.video");
  assert.equal(video.name, "Arrival close");
  assert.equal(video.trackKind, "video");
  assert.equal(video.targeted, true);
  assert.equal(video.syncLocked, true);
  assert.equal(video.canonicalSelected, true);
  assert.equal(video.retimed, true);
  assert.equal(video.startSeconds, 3_600);
  assert.equal(video.endSeconds, 3_602);
  assert.deepEqual(video.geometry, { leftPercent: 0, widthPercent: 100 });
  assert.deepEqual(video.source, {
    kind: "media",
    id: "media.video",
    name: "Arrival A",
    target: "/media/arrival-a.mov",
    relinkStatus: "online",
  });
  assert.deepEqual(video.sourceRange, range("24", "48", 24));
  assert.deepEqual(video.recordRange, range("0", "48", 24));
  assert.deepEqual(video.linkedClipIds, ["clip.audio"]);
  assert.deepEqual(video.groupedClipIds, ["clip.audio"]);
  assert.deepEqual(video.markers, [
    {
      id: "marker.video",
      label: "Preferred cut",
      flag: "cyan",
      note: "Keep this reaction",
    },
  ]);
  assert.deepEqual(video.metadataKeys, ["editorial.status"]);
  assert.deepEqual(video.multicam, {
    syncMethod: "timecode",
    switchCount: 1,
    audioPolicy: "follow_video",
  });
  assert.deepEqual(video.effects, [
    {
      nodeId: "node.effect.grade",
      nodeType: "superi.effects.color.primary",
      label: "Color Primary",
      driverCount: 1,
    },
  ]);
  assert.equal(video.automation, null);
  assert.match(formatTimelineClipTiming(video), /source 24\+48 @ 24\/1/);
  assert.match(formatTimelineClipTiming(video), /record 0\+48 @ 24\/1/);

  const audio = projection.clips[1];
  assert.equal(audio.id, "clip.audio");
  assert.equal(audio.trackKind, "audio");
  assert.equal(audio.targeted, false);
  assert.equal(audio.syncLocked, false);
  assert.deepEqual(audio.geometry, { leftPercent: 50, widthPercent: 50 });
  assert.equal(audio.source.kind, "media");
  if (audio.source.kind === "media") {
    assert.equal(audio.source.relinkStatus, "unverified");
  }
  assert.equal(audio.retimed, false);
  assert.deepEqual(audio.automation, {
    sampleRate: 48_000,
    defaultGain: 1,
    mode: "touch",
    keyframes: [
      { sample: 48_000, sampleRate: 48_000, value: 0.8 },
      { sample: 96_000, sampleRate: 48_000, value: 1 },
    ],
    activePass: {
      startSample: 48_000,
      sampleRate: 48_000,
      currentValue: 0.9,
      touchActive: true,
      latchActive: false,
    },
  });
  if (audio.automation === null) return;
  assert.equal(
    timelineClipAutomationKeyPercent(audio, audio.automation.keyframes[0]),
    0,
  );
  assert.equal(
    timelineClipAutomationKeyPercent(audio, audio.automation.keyframes[1]),
    100,
  );
  assert.equal(
    timelineClipAutomationKeyPercent(audio, {
      sample: 0,
      sampleRate: 48_000,
      value: 1,
    }),
    null,
  );

  assert.ok(Object.isFrozen(projection));
  assert.ok(Object.isFrozen(projection.clips));
  assert.ok(Object.isFrozen(video));
  assert.ok(Object.isFrozen(video.effects));
  assert.throws(() => {
    (video.linkedClipIds as string[]).push("clip.forbidden");
  }, TypeError);
});

test("keeps malformed supplemental or unsafe canonical state unavailable", () => {
  const malformed = completeSnapshot();
  const content = malformed.timeline.document.content as Record<string, unknown>;
  const payload = content.payload as Record<string, unknown>;
  const timelines = payload.timelines as Array<Record<string, unknown>>;
  const tracks = timelines[0]?.tracks as Array<Record<string, unknown>>;
  const items = tracks[0]?.items as Array<Record<string, unknown>>;
  const clipRange = items[0]?.record_range as Record<string, unknown>;
  const start = clipRange.start as Record<string, unknown>;
  start.value = "9007199254740992";

  assert.deepEqual(projectTimelineClips(malformed), {
    status: "unavailable",
    reason: "Timeline clip detail is malformed or uses an unsupported contract.",
  });
});

test("integrates real previews and shared selection into the existing canvas owner", () => {
  const workspaces = readFileSync(
    resolve(appRoot, "src/editor-workspaces.tsx"),
    "utf8",
  );
  const timeline = readFileSync(
    resolve(appRoot, "src/timeline-workspace.tsx"),
    "utf8",
  );
  const styles = readFileSync(resolve(appRoot, "src/styles.css"), "utf8");
  const packageJson = JSON.parse(
    readFileSync(resolve(appRoot, "package.json"), "utf8"),
  );

  assert.match(workspaces, /snapshot=\{snapshot\}/);
  assert.match(workspaces, /sharedSelectedClipIds=\{sharedSelectedClipIds\}/);
  assert.match(workspaces, /type: "extend_selection"/);
  assert.match(workspaces, /type: "replace_selection"/);
  assert.match(workspaces, /resource: "superi\.editor\.state"/);
  assert.match(timeline, /projectTimelineClipDetails/);
  assert.match(timeline, /readProjectMediaLibrary/);
  assert.match(timeline, /generateProjectMediaPreview/);
  assert.match(timeline, /bundle\.freshness !== item\.content_fingerprint/);
  assert.match(timeline, /aria-pressed=\{sharedSelected\}/);
  assert.match(timeline, /\.\.\.evidence,/);
  assert.match(timeline, /formatTimelineClipTiming/);
  assert.match(timeline, /timeline-item-filmstrip/);
  assert.match(timeline, /timeline-item-waveform/);
  assert.match(timeline, /timeline-item-keyframe/);
  assert.match(styles, /button\.timeline-item \{[\s\S]*?pointer-events: auto;/);
  assert.match(styles, /\.timeline-item-shared-selected/);
  assert.match(styles, /\.timeline-item-preview/);
  assert.doesNotMatch(
    timeline,
    /superi\.project\.command\.execute|superi\.timeline\.edit|useSuperiApi/,
  );
  assert.match(packageJson.scripts.test, /timeline-clip-presentation\.test\.ts/);
});
