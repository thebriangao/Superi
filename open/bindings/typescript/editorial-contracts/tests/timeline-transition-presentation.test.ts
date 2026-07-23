import assert from "node:assert/strict";
import test from "node:test";

import type { EditorStateSnapshot } from "../src/api.ts";
import {
  buildSetTransitionAction,
  buildTransitionParameterAction,
  projectTimelineTransitionDetails,
  transitionHandlesForAlignment,
} from "../src/timeline-transition-presentation.ts";
import type {
  TimelineCanvasItem,
  TimelineCanvasModel,
  TimelineRate,
} from "../src/timeline-workspace.ts";

const RATE: TimelineRate = { numerator: 24, denominator: 1 };

function exactRange(start: string, duration: string) {
  return {
    start: { value: start, timebase: RATE },
    duration: { value: duration, timebase: RATE },
  };
}

function clip(id: string, start: string, duration: string): TimelineCanvasItem {
  const end = (BigInt(start) + BigInt(duration)).toString();
  return {
    kind: "clip",
    id,
    name: id,
    startSeconds: Number(start) / 24,
    endSeconds: Number(end) / 24,
    recordRange: exactRange(start, duration),
    source: { kind: "media", id: `media.${id}` },
    sourceRange: exactRange(start, duration),
    transition: null,
    selected: false,
    group: null,
    link: null,
  };
}

function transitionItem(
  id: string,
  from: string,
  to: string,
  cut: string,
  fromOffset: string,
  toOffset: string,
): TimelineCanvasItem {
  const start = BigInt(cut) - BigInt(fromOffset);
  const duration = BigInt(fromOffset) + BigInt(toOffset);
  return {
    kind: "transition",
    id,
    name: id,
    startSeconds: Number(start) / 24,
    endSeconds: Number(start + duration) / 24,
    recordRange: exactRange(start.toString(), duration.toString()),
    source: null,
    sourceRange: null,
    transition: {
      from: { kind: "clip", id: from },
      to: { kind: "clip", id: to },
      fromOffset: { value: fromOffset, timebase: RATE },
      toOffset: { value: toOffset, timebase: RATE },
    },
    selected: false,
    group: null,
    link: null,
  };
}

function model(): TimelineCanvasModel {
  return {
    projectId: "project.cut",
    projectName: "Cut",
    projectRevision: "17",
    documentSha256: "a".repeat(64),
    id: "timeline.main",
    name: "Main",
    editRate: RATE,
    globalStart: { value: "0", timebase: RATE },
    startSeconds: 0,
    endSeconds: 10,
    durationSeconds: 10,
    snappingEnabled: true,
    linkedSelectionEnabled: true,
    snapTargets: [],
    tracks: [
      {
        id: "track.v1",
        name: "V1",
        kind: "video",
        targeted: true,
        syncLocked: true,
        items: [
          clip("clip.a", "0", "100"),
          transitionItem(
            "transition.main",
            "clip.a",
            "clip.b",
            "100",
            "20",
            "10",
          ),
          clip("clip.b", "100", "80"),
          transitionItem(
            "transition.outgoing",
            "clip.b",
            "clip.c",
            "180",
            "15",
            "5",
          ),
          clip("clip.c", "180", "60"),
        ],
      },
    ],
  } as unknown as TimelineCanvasModel;
}

function snapshot(): EditorStateSnapshot {
  const transitionSchema = {
    id: {
      node_type: "superi.timeline.video.transition",
      schema_version: { major: 1, minor: 0, patch: 0 },
    },
    parameters: [
      {
        name: "object-id",
        value_type: "superi.value.timeline.object-id",
        animatable: false,
      },
    ],
  };
  const wipeSchema = {
    id: {
      node_type: "superi.transition.directional-wipe",
      schema_version: { major: 1, minor: 0, patch: 0 },
    },
    parameters: [
      { name: "progress", value_type: "superi.value.scalar", animatable: true },
      { name: "direction", value_type: "superi.value.choice", animatable: false },
      { name: "softness", value_type: "superi.value.scalar", animatable: true },
    ],
  };
  const trackSchema = {
    id: {
      node_type: "superi.timeline.video.track",
      schema_version: { major: 1, minor: 0, patch: 0 },
    },
    parameters: [],
  };
  const envelope = {
    format: "superi.graph",
    format_revision: 1,
    primitive_schema_revision: 1,
    payload_sha256: "b".repeat(64),
    payload: {
      graph_id: "graph.main",
      revision: "9",
      schemas: [transitionSchema, wipeSchema, trackSchema],
      nodes: [
        {
          id: "node.transition.main",
          schema: transitionSchema.id,
          inputs: [],
          outputs: [{ id: "port.transition.out", name: "content" }],
          parameters: [
            {
              id: "parameter.transition.object",
              name: "object-id",
              value_type: "superi.value.timeline.object-id",
              payload: {
                domain: {
                  kind: "editorial_object_id",
                  value: { kind: "transition", id: "transition.main" },
                },
              },
            },
          ],
        },
        {
          id: "node.transition.wipe",
          schema: wipeSchema.id,
          inputs: [{ id: "port.wipe.in", name: "from" }],
          outputs: [{ id: "port.wipe.out", name: "result" }],
          parameters: [
            {
              id: "parameter.wipe.progress",
              name: "progress",
              value_type: "superi.value.scalar",
              payload: { scalar: 4602678819172646912 },
            },
            {
              id: "parameter.wipe.direction",
              name: "direction",
              value_type: "superi.value.choice",
              payload: { choice: "left-to-right" },
            },
            {
              id: "parameter.wipe.softness",
              name: "softness",
              value_type: "superi.value.scalar",
              payload: { scalar: 4598175219545276416 },
            },
          ],
        },
        {
          id: "node.track.v1",
          schema: trackSchema.id,
          inputs: [{ id: "port.track.in", name: "items" }],
          outputs: [],
          parameters: [],
        },
      ],
      edges: [
        {
          id: "edge.transition.wipe",
          source: {
            node_id: "node.transition.main",
            port_id: "port.transition.out",
          },
          destination: {
            node_id: "node.transition.wipe",
            port_id: "port.wipe.in",
          },
        },
        {
          id: "edge.wipe.track",
          source: { node_id: "node.transition.wipe", port_id: "port.wipe.out" },
          destination: { node_id: "node.track.v1", port_id: "port.track.in" },
        },
      ],
      node_order: [
        "node.transition.main",
        "node.transition.wipe",
        "node.track.v1",
      ],
      parameter_drivers: [],
    },
  };

  return {
    schema_version: "1.0.0",
    project: {
      project_id: "project.cut",
      root_timeline_id: "timeline.main",
      project_revision: 17,
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
            sha256: "c".repeat(64),
            content: envelope,
          },
        },
      ],
    },
  } as unknown as EditorStateSnapshot;
}

test("projects exact transition handles, limits, alignment, and typed graph parameters", () => {
  const projection = projectTimelineTransitionDetails(snapshot(), model());
  assert.equal(projection.transitions.length, 2);
  const transition = projection.transitions[0];
  assert.equal(transition.id, "transition.main");
  assert.equal(transition.trackId, "track.v1");
  assert.equal(transition.fromOffset.value, "20");
  assert.equal(transition.toOffset.value, "10");
  assert.equal(transition.duration.value, "30");
  assert.equal(transition.maximumFromOffset.value, "100");
  assert.equal(transition.maximumToOffset.value, "65");
  assert.equal(transition.alignment, "custom");
  assert.equal(transition.graph.status, "ready");
  if (transition.graph.status !== "ready") return;
  assert.equal(transition.graph.graphId, "graph.main");
  assert.equal(transition.graph.effects.length, 1);
  const parameters = transition.graph.effects[0].parameters;
  assert.deepEqual(
    parameters.map((parameter) => [parameter.name, parameter.kind, parameter.editable]),
    [
      ["progress", "scalar", false],
      ["direction", "choice", true],
      ["softness", "scalar", true],
    ],
  );
  assert.deepEqual(parameters[1].choices, [
    "left-to-right",
    "right-to-left",
    "top-to-bottom",
    "bottom-to-top",
  ]);
  assert.equal(parameters[0].value, 0.5);
  assert.equal(parameters[2].value, 0.25);
  assert.equal(Object.isFrozen(projection), true);
  assert.equal(Object.isFrozen(parameters), true);
});

test("derives exact alignments and strict public command payloads", () => {
  const transition = projectTimelineTransitionDetails(snapshot(), model()).transitions[0];
  assert.deepEqual(transitionHandlesForAlignment(transition, "center", "31"), {
    fromOffsetValue: "15",
    toOffsetValue: "16",
  });
  assert.deepEqual(transitionHandlesForAlignment(transition, "end", "30"), {
    fromOffsetValue: "30",
    toOffsetValue: "0",
  });
  assert.equal(transitionHandlesForAlignment(transition, "start", "70"), null);
  assert.throws(
    () => buildSetTransitionAction(transition, "20", "10"),
    /already matches/,
  );

  assert.deepEqual(buildSetTransitionAction(transition, "15", "15"), {
    action: "edit_timeline",
    operations: [
      {
        operation: "set_transition",
        timeline_id: "timeline.main",
        track_id: "track.v1",
        transition_id: "transition.main",
        from_offset: { value: 15, timebase: RATE },
        to_offset: { value: 15, timebase: RATE },
      },
    ],
  });

  assert.equal(transition.graph.status, "ready");
  if (transition.graph.status !== "ready") return;
  const direction = transition.graph.effects[0].parameters[1];
  assert.deepEqual(buildTransitionParameterAction(direction, "bottom-to-top"), {
    action: "mutate_graph",
    graph_id: "graph.main",
    mutations: [
      {
        operation: "set_parameter",
        node_id: "node.transition.wipe",
        parameter_id: "parameter.wipe.direction",
        value: {
          value_type: "superi.value.choice",
          value: { kind: "choice", value: "bottom-to-top" },
        },
      },
    ],
  });
});

test("keeps exact timing available when optional graph detail is malformed", () => {
  const malformed = structuredClone(snapshot()) as unknown as {
    graph: { documents: { document: { format_revision: number } }[] };
  };
  malformed.graph.documents[0].document.format_revision = 2;
  const projection = projectTimelineTransitionDetails(
    malformed as unknown as EditorStateSnapshot,
    model(),
  );
  assert.equal(projection.transitions.length, 2);
  assert.equal(projection.transitions[0].duration.value, "30");
  assert.equal(projection.transitions[0].graph.status, "unavailable");
});
