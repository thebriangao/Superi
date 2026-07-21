import assert from "node:assert/strict";
import test from "node:test";

import type { EditorStateSnapshot } from "../src/api.ts";
import type { ApplicationSelection } from "../src/application.ts";
import { timelineSelectionIdentity } from "../src/timeline-workspace.ts";
import {
  VIEWER_TRANSFORM_IDENTITY_MATRIX,
  buildViewerTransformAction,
  projectViewerTransformControls,
} from "../src/viewer-transform-controls.ts";

const TRANSFORM_TYPE = "superi.effect.transform";
const MATRIX_NAMES = [
  "m00",
  "m01",
  "m02",
  "m10",
  "m11",
  "m12",
  "m20",
  "m21",
  "m22",
] as const;

function scalarBits(value: number): number {
  const bytes = new ArrayBuffer(8);
  const view = new DataView(bytes);
  view.setFloat64(0, value, false);
  return Number(view.getBigUint64(0, false));
}

function transformSchema() {
  return {
    id: {
      node_type: TRANSFORM_TYPE,
      schema_version: { major: 1, minor: 0, patch: 0 },
    },
    parameters: [
      ...MATRIX_NAMES.map((name) => ({
        name,
        value_type: "superi.value.scalar",
        animatable: true,
      })),
      {
        name: "sampling",
        value_type: "superi.value.choice",
        animatable: true,
      },
    ],
  };
}

function transformNode(
  id: string,
  matrix: readonly number[],
  sampling: "nearest" | "bilinear",
) {
  return {
    id,
    schema: transformSchema().id,
    inputs: [{ id: `port.${id}.in`, name: "source" }],
    outputs: [{ id: `port.${id}.out`, name: "result" }],
    parameters: [
      ...MATRIX_NAMES.map((name, index) => ({
        id: `parameter.${id}.${name}`,
        name,
        value_type: "superi.value.scalar",
        payload: { scalar: scalarBits(matrix[index] ?? Number.NaN) },
      })),
      {
        id: `parameter.${id}.sampling`,
        name: "sampling",
        value_type: "superi.value.choice",
        payload: { choice: sampling },
      },
    ],
  };
}

function snapshot(): EditorStateSnapshot {
  const transform = transformSchema();
  const clipSchema = {
    id: {
      node_type: "superi.timeline.video.clip",
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
  const gradeSchema = {
    id: {
      node_type: "superi.effect.grade",
      schema_version: { major: 1, minor: 0, patch: 0 },
    },
    parameters: [],
  };
  const trackSchema = {
    id: {
      node_type: "superi.timeline.video.track",
      schema_version: { major: 1, minor: 0, patch: 0 },
    },
    parameters: [],
  };
  const firstMatrix = [1, 0, 24, 0, 1, -12, 0, 0, 1];
  const secondMatrix = [0, -1, 0, 1, 0, 0, 0, 0, 1];
  const payload = {
    graph_id: "graph.main",
    revision: "9",
    schemas: [clipSchema, transform, gradeSchema, trackSchema],
    nodes: [
      {
        id: "node.clip",
        schema: clipSchema.id,
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
                value: { kind: "clip", id: "clip.hero" },
              },
            },
          },
        ],
      },
      transformNode("node.transform.primary", firstMatrix, "nearest"),
      {
        id: "node.grade",
        schema: gradeSchema.id,
        inputs: [{ id: "port.grade.in", name: "source" }],
        outputs: [{ id: "port.grade.out", name: "result" }],
        parameters: [],
      },
      transformNode("node.transform.secondary", secondMatrix, "bilinear"),
      {
        id: "node.track",
        schema: trackSchema.id,
        inputs: [{ id: "port.track.in", name: "items" }],
        outputs: [],
        parameters: [],
      },
    ],
    edges: [
      {
        id: "edge.clip.primary",
        source: { node_id: "node.clip", port_id: "port.clip.out" },
        destination: {
          node_id: "node.transform.primary",
          port_id: "port.node.transform.primary.in",
        },
      },
      {
        id: "edge.primary.grade",
        source: {
          node_id: "node.transform.primary",
          port_id: "port.node.transform.primary.out",
        },
        destination: { node_id: "node.grade", port_id: "port.grade.in" },
      },
      {
        id: "edge.grade.secondary",
        source: { node_id: "node.grade", port_id: "port.grade.out" },
        destination: {
          node_id: "node.transform.secondary",
          port_id: "port.node.transform.secondary.in",
        },
      },
      {
        id: "edge.secondary.track",
        source: {
          node_id: "node.transform.secondary",
          port_id: "port.node.transform.secondary.out",
        },
        destination: { node_id: "node.track", port_id: "port.track.in" },
      },
    ],
    node_order: [
      "node.clip",
      "node.transform.primary",
      "node.grade",
      "node.transform.secondary",
      "node.track",
    ],
    parameter_drivers: [
      {
        target: {
          node_id: "node.transform.secondary",
          parameter_id: "parameter.node.transform.secondary.m01",
        },
        value_type: "superi.value.scalar",
        driver: { kind: "expression", source: "rotation", variables: [] },
      },
    ],
  };

  return {
    schema_version: "1.0.0",
    project: {
      project_id: "project.viewer",
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
            sha256: "a".repeat(64),
            content: {
              format: "superi.graph",
              format_revision: 1,
              primitive_schema_revision: 1,
              payload_sha256: "b".repeat(64),
              payload,
            },
          },
        },
      ],
    },
  } as unknown as EditorStateSnapshot;
}

function selection(revision = 17): ApplicationSelection {
  const item = {
    resource: "superi.editor.state" as const,
    schema_version: "1.0.0",
    identity: timelineSelectionIdentity("timeline.main", {
      kind: "clip",
      id: "clip.hero",
    }),
    revision,
  };
  return { items: [item], anchor: item };
}

test("projects every selected clip transform in canonical graph order", () => {
  const state = snapshot();
  const before = structuredClone(state);
  const projection = projectViewerTransformControls(state, selection());

  assert.equal(projection.status, "ready");
  if (projection.status !== "ready") return;
  assert.equal(projection.projectRevision, 17);
  assert.equal(projection.timelineId, "timeline.main");
  assert.equal(projection.clipId, "clip.hero");
  assert.equal(projection.graphId, "graph.main");
  assert.equal(projection.graphRevision, 9);
  assert.deepEqual(
    projection.transforms.map((transform) => transform.nodeId),
    ["node.transform.primary", "node.transform.secondary"],
  );
  assert.deepEqual(projection.transforms[0].matrix.map((entry) => entry.value), [
    1, 0, 24, 0, 1, -12, 0, 0, 1,
  ]);
  assert.equal(projection.transforms[0].sampling.value, "nearest");
  assert.equal(projection.transforms[0].matrixDriven, false);
  assert.equal(projection.transforms[1].matrixDriven, true);
  assert.equal(projection.transforms[1].matrix[1].driven, true);
  assert.equal(Object.isFrozen(projection), true);
  assert.equal(Object.isFrozen(projection.transforms), true);
  assert.equal(Object.isFrozen(projection.transforms[0].matrix), true);
  assert.deepEqual(state, before);
});

test("builds one ordered typed graph action for changed ordinary parameters", () => {
  const projection = projectViewerTransformControls(snapshot(), selection());
  assert.equal(projection.status, "ready");
  if (projection.status !== "ready") return;

  assert.deepEqual(
    buildViewerTransformAction(projection.transforms[0], {
      matrix: [1, 0, 30, 0, 1, -12, 0, 0, 1],
      sampling: "bilinear",
    }),
    {
      action: "mutate_graph",
      graph_id: "graph.main",
      mutations: [
        {
          operation: "set_parameter",
          node_id: "node.transform.primary",
          parameter_id: "parameter.node.transform.primary.m02",
          value: {
            value_type: "superi.value.scalar",
            value: { kind: "scalar", value: 30 },
          },
        },
        {
          operation: "set_parameter",
          node_id: "node.transform.primary",
          parameter_id: "parameter.node.transform.primary.sampling",
          value: {
            value_type: "superi.value.choice",
            value: { kind: "choice", value: "bilinear" },
          },
        },
      ],
    },
  );

  assert.deepEqual([...VIEWER_TRANSFORM_IDENTITY_MATRIX], [
    1, 0, 0, 0, 1, 0, 0, 0, 1,
  ]);

  assert.deepEqual(
    buildViewerTransformAction(projection.transforms[0], {
      matrix: VIEWER_TRANSFORM_IDENTITY_MATRIX,
      sampling: "bilinear",
    }),
    {
      action: "mutate_graph",
      graph_id: "graph.main",
      mutations: [
        {
          operation: "set_parameter",
          node_id: "node.transform.primary",
          parameter_id: "parameter.node.transform.primary.m02",
          value: {
            value_type: "superi.value.scalar",
            value: { kind: "scalar", value: 0 },
          },
        },
        {
          operation: "set_parameter",
          node_id: "node.transform.primary",
          parameter_id: "parameter.node.transform.primary.m12",
          value: {
            value_type: "superi.value.scalar",
            value: { kind: "scalar", value: 0 },
          },
        },
        {
          operation: "set_parameter",
          node_id: "node.transform.primary",
          parameter_id: "parameter.node.transform.primary.sampling",
          value: {
            value_type: "superi.value.choice",
            value: { kind: "choice", value: "bilinear" },
          },
        },
      ],
    },
  );
});

test("rejects no-op, nonfinite, unsupported, and driver-owned edits", () => {
  const projection = projectViewerTransformControls(snapshot(), selection());
  assert.equal(projection.status, "ready");
  if (projection.status !== "ready") return;
  const primary = projection.transforms[0];
  const secondary = projection.transforms[1];

  assert.throws(
    () =>
      buildViewerTransformAction(primary, {
        matrix: primary.matrix.map((entry) => entry.value),
        sampling: primary.sampling.value,
      }),
    /already matches/i,
  );
  assert.throws(
    () =>
      buildViewerTransformAction(primary, {
        matrix: [1, 0, Number.NaN, 0, 1, 0, 0, 0, 1],
        sampling: "nearest",
      }),
    /finite/i,
  );
  assert.throws(
    () =>
      buildViewerTransformAction(primary, {
        matrix: [...VIEWER_TRANSFORM_IDENTITY_MATRIX],
        sampling: "bicubic" as "nearest",
      }),
    /sampling/i,
  );
  assert.throws(
    () =>
      buildViewerTransformAction(secondary, {
        matrix: [...VIEWER_TRANSFORM_IDENTITY_MATRIX],
        sampling: secondary.sampling.value,
      }),
    /driver/i,
  );
});

test("fails closed for stale selection, missing transforms, and malformed scalar state", () => {
  const stale = projectViewerTransformControls(snapshot(), selection(16));
  assert.deepEqual(stale, {
    status: "unavailable",
    reason: "The selected clip does not belong to the current editor revision.",
  });

  const missing = structuredClone(snapshot()) as unknown as {
    graph: {
      documents: Array<{
        document: { content: { payload: { edges: unknown[] } } };
      }>;
    };
  };
  missing.graph.documents[0].document.content.payload.edges = [
    {
      id: "edge.clip.track",
      source: { node_id: "node.clip", port_id: "port.clip.out" },
      destination: { node_id: "node.track", port_id: "port.track.in" },
    },
  ];
  assert.deepEqual(
    projectViewerTransformControls(
      missing as unknown as EditorStateSnapshot,
      selection(),
    ),
    {
      status: "unavailable",
      reason: "The selected clip has no attached built-in transform node.",
    },
  );

  const malformed = structuredClone(snapshot()) as unknown as {
    graph: {
      documents: Array<{
        document: {
          content: {
            payload: {
              nodes: Array<{ parameters: Array<{ payload: Record<string, unknown> }> }>;
            };
          };
        };
      }>;
    };
  };
  malformed.graph.documents[0].document.content.payload.nodes[1].parameters[0].payload = {
    scalar: "not-bits",
  };
  const unavailable = projectViewerTransformControls(
    malformed as unknown as EditorStateSnapshot,
    selection(),
  );
  assert.equal(unavailable.status, "unavailable");
  if (unavailable.status === "unavailable") {
    assert.match(unavailable.reason, /scalar/i);
  }

  const absentPort = structuredClone(snapshot()) as unknown as {
    graph: {
      documents: Array<{
        document: {
          content: {
            payload: {
              edges: Array<{ destination: { port_id: string } }>;
            };
          };
        };
      }>;
    };
  };
  absentPort.graph.documents[0].document.content.payload.edges[0].destination.port_id =
    "port.absent";
  const invalidTopology = projectViewerTransformControls(
    absentPort as unknown as EditorStateSnapshot,
    selection(),
  );
  assert.equal(invalidTopology.status, "unavailable");
  if (invalidTopology.status === "unavailable") {
    assert.match(invalidTopology.reason, /port/i);
  }
});
