import type { EditorStateSnapshot, ProjectAction } from "./api.ts";
import type { ApplicationSelection } from "./application.ts";
import { parseTimelineSelectionIdentity } from "./timeline-workspace.ts";

const GRAPH_FORMAT = "superi.graph";
const GRAPH_FORMAT_REVISION = 1;
const TIMELINE_NODE_PREFIX = "superi.timeline.";
const TRANSFORM_NODE_TYPE = "superi.effect.transform";
const SCALAR_VALUE_TYPE = "superi.value.scalar";
const CHOICE_VALUE_TYPE = "superi.value.choice";
const TIMELINE_OBJECT_VALUE_TYPE = "superi.value.timeline.object-id";

export const VIEWER_TRANSFORM_MATRIX_PARAMETER_NAMES = Object.freeze([
  "m00",
  "m01",
  "m02",
  "m10",
  "m11",
  "m12",
  "m20",
  "m21",
  "m22",
] as const);

export const VIEWER_TRANSFORM_IDENTITY_MATRIX = Object.freeze([
  1, 0, 0,
  0, 1, 0,
  0, 0, 1,
] as const);

const MATRIX_PARAMETER_LABELS = Object.freeze({
  m00: "X basis X",
  m01: "Y basis X",
  m02: "Position X",
  m10: "X basis Y",
  m11: "Y basis Y",
  m12: "Position Y",
  m20: "Perspective X",
  m21: "Perspective Y",
  m22: "Homogeneous scale",
} as const);

export type ViewerTransformMatrixParameterName =
  (typeof VIEWER_TRANSFORM_MATRIX_PARAMETER_NAMES)[number];
export type ViewerTransformSampling = "nearest" | "bilinear";

export interface ViewerTransformMatrixParameter {
  readonly name: ViewerTransformMatrixParameterName;
  readonly label: string;
  readonly parameterId: string;
  readonly value: number;
  readonly driven: boolean;
}

export interface ViewerTransformSamplingParameter {
  readonly parameterId: string;
  readonly value: ViewerTransformSampling;
  readonly driven: boolean;
}

export interface ViewerTransformNodePresentation {
  readonly graphId: string;
  readonly graphRevision: number;
  readonly nodeId: string;
  readonly nodeType: typeof TRANSFORM_NODE_TYPE;
  readonly schemaVersion: "1.0.0";
  readonly matrix: readonly ViewerTransformMatrixParameter[];
  readonly matrixDriven: boolean;
  readonly sampling: ViewerTransformSamplingParameter;
}

export type ViewerTransformProjection =
  | {
      readonly status: "ready";
      readonly projectRevision: number;
      readonly timelineId: string;
      readonly clipId: string;
      readonly graphId: string;
      readonly graphRevision: number;
      readonly transforms: readonly ViewerTransformNodePresentation[];
    }
  | {
      readonly status: "unavailable";
      readonly reason: string;
    };

export interface ViewerTransformDraft {
  readonly matrix: readonly number[];
  readonly sampling: ViewerTransformSampling;
}

type JsonRecord = Record<string, unknown>;

interface GraphNodeRecord {
  readonly id: string;
  readonly nodeType: string;
  readonly schema: JsonRecord;
  readonly value: JsonRecord;
  readonly inputIds: ReadonlySet<string>;
  readonly outputIds: ReadonlySet<string>;
  readonly parameterIds: ReadonlyMap<string, string>;
}

export function projectViewerTransformControls(
  snapshot: EditorStateSnapshot | null,
  selection: ApplicationSelection,
): ViewerTransformProjection {
  if (snapshot === null) {
    return unavailable("Editor state has not been observed.");
  }
  if (selection.items.length === 0) {
    return unavailable("Select one timeline clip to inspect its graph transforms.");
  }
  if (selection.items.length !== 1) {
    return unavailable("Viewer transform controls require exactly one selected clip.");
  }

  const selected = selection.items[0];
  if (selected.resource !== "superi.editor.state") {
    return unavailable("The selected resource is not canonical editor state.");
  }
  if (
    selected.schema_version !== snapshot.schema_version ||
    selected.revision !== snapshot.project.project_revision
  ) {
    return unavailable(
      "The selected clip does not belong to the current editor revision.",
    );
  }
  const identity = parseTimelineSelectionIdentity(selected.identity);
  if (identity === null || identity.object.kind !== "clip") {
    return unavailable("The selected editor identity is not a timeline clip.");
  }

  try {
    return projectSelectedClipTransforms(
      snapshot,
      identity.timelineId,
      identity.object.id,
    );
  } catch (error: unknown) {
    return unavailable(
      error instanceof Error
        ? error.message
        : "Viewer transform state is malformed or unsupported.",
    );
  }
}

export function buildViewerTransformAction(
  transform: ViewerTransformNodePresentation,
  draft: ViewerTransformDraft,
): ProjectAction {
  if (draft.matrix.length !== VIEWER_TRANSFORM_MATRIX_PARAMETER_NAMES.length) {
    throw new Error("Viewer transform matrix must contain exactly nine values.");
  }
  const matrix = draft.matrix.map((value) => {
    if (!Number.isFinite(value)) {
      throw new Error("Viewer transform matrix values must be finite numbers.");
    }
    return Object.is(value, -0) ? 0 : value;
  });
  if (draft.sampling !== "nearest" && draft.sampling !== "bilinear") {
    throw new Error("Viewer transform sampling must be nearest or bilinear.");
  }

  const matrixChanged = matrix.some(
    (value, index) => value !== transform.matrix[index]?.value,
  );
  if (matrixChanged && transform.matrixDriven) {
    throw new Error(
      "Viewer transform matrix is read-only because a graph driver owns at least one value.",
    );
  }
  const samplingChanged = draft.sampling !== transform.sampling.value;
  if (samplingChanged && transform.sampling.driven) {
    throw new Error(
      "Viewer transform sampling is read-only because a graph driver owns it.",
    );
  }

  const mutations: Extract<ProjectAction, { action: "mutate_graph" }>["mutations"] = [];
  if (matrixChanged) {
    for (let index = 0; index < transform.matrix.length; index += 1) {
      const parameter = transform.matrix[index];
      const value = matrix[index];
      if (parameter === undefined || value === undefined || value === parameter.value) {
        continue;
      }
      mutations.push({
        operation: "set_parameter",
        node_id: transform.nodeId,
        parameter_id: parameter.parameterId,
        value: {
          value_type: SCALAR_VALUE_TYPE,
          value: { kind: "scalar", value },
        },
      });
    }
  }
  if (samplingChanged) {
    mutations.push({
      operation: "set_parameter",
      node_id: transform.nodeId,
      parameter_id: transform.sampling.parameterId,
      value: {
        value_type: CHOICE_VALUE_TYPE,
        value: { kind: "choice", value: draft.sampling },
      },
    });
  }
  if (mutations.length === 0) {
    throw new Error("Viewer transform draft already matches canonical graph state.");
  }
  return {
    action: "mutate_graph",
    graph_id: transform.graphId,
    mutations,
  };
}

function projectSelectedClipTransforms(
  snapshot: EditorStateSnapshot,
  timelineId: string,
  clipId: string,
): Extract<ViewerTransformProjection, { status: "ready" }> {
  const documents = snapshot.graph.documents.filter(
    (document) =>
      document.scope.kind === "timeline" &&
      document.scope.root_timeline_id === timelineId,
  );
  if (documents.length !== 1) {
    throw new Error(
      documents.length === 0
        ? "The selected clip has no timeline-scoped graph document."
        : "The selected clip has ambiguous timeline-scoped graph ownership.",
    );
  }
  const graph = documents[0];
  if (!Number.isSafeInteger(graph.graph_revision) || graph.graph_revision < 0) {
    throw new Error("The selected graph revision is invalid.");
  }
  const document = record(graph.document, "graph document");
  if (
    document.format !== GRAPH_FORMAT ||
    document.format_revision !== GRAPH_FORMAT_REVISION
  ) {
    throw new Error("The selected graph document format is unsupported.");
  }
  const envelope = record(document.content, "graph envelope");
  if (
    envelope.format !== GRAPH_FORMAT ||
    envelope.format_revision !== GRAPH_FORMAT_REVISION
  ) {
    throw new Error("The selected graph envelope format is unsupported.");
  }
  const payload = record(envelope.payload, "graph payload");
  if (nonemptyString(payload.graph_id, "payload graph identity") !== graph.graph_id) {
    throw new Error("The selected graph identity changed inside its envelope.");
  }
  if (canonicalUnsigned(payload.revision, "payload graph revision") !== BigInt(graph.graph_revision)) {
    throw new Error("The selected graph revision changed inside its envelope.");
  }

  const schemaByKey = new Map<string, JsonRecord>();
  for (const value of array(payload.schemas, "graph schemas")) {
    const schema = record(value, "node schema");
    const key = schemaKey(record(schema.id, "node schema identity"));
    if (schemaByKey.has(key)) throw new Error("The graph repeats a node schema.");
    schemaByKey.set(key, schema);
  }

  const nodeById = new Map<string, GraphNodeRecord>();
  const parameterAddresses = new Set<string>();
  for (const value of array(payload.nodes, "graph nodes")) {
    const node = record(value, "graph node");
    const id = nonemptyString(node.id, "node identity");
    const nodeSchema = record(node.schema, "node schema identity");
    const nodeType = nonemptyString(nodeSchema.node_type, "node type");
    if (!schemaByKey.has(schemaKey(nodeSchema))) {
      throw new Error("A graph node references an absent schema.");
    }
    if (nodeById.has(id)) throw new Error("The graph repeats a node identity.");
    const inputIds = portIdentities(node.inputs, "node inputs");
    const outputIds = portIdentities(node.outputs, "node outputs");
    const parameterIds = new Map<string, string>();
    for (const parameterValue of array(node.parameters, "node parameters")) {
      const parameter = record(parameterValue, "node parameter");
      const name = nonemptyString(parameter.name, "parameter name");
      const parameterId = nonemptyString(parameter.id, "parameter identity");
      if (parameterIds.has(name)) {
        throw new Error("A graph node repeats a parameter name.");
      }
      const address = parameterKey(id, parameterId);
      if (parameterAddresses.has(address)) {
        throw new Error("The graph repeats a parameter address.");
      }
      parameterAddresses.add(address);
      parameterIds.set(name, parameterId);
    }
    nodeById.set(id, {
      id,
      nodeType,
      schema: schemaByKey.get(schemaKey(nodeSchema))!,
      value: node,
      inputIds,
      outputIds,
      parameterIds,
    });
  }

  const order = array(payload.node_order, "node order").map((value) =>
    nonemptyString(value, "ordered node identity"),
  );
  if (order.length !== nodeById.size || new Set(order).size !== order.length) {
    throw new Error("The graph node order is incomplete or repeats an identity.");
  }
  const orderIndex = new Map<string, number>();
  order.forEach((nodeId, index) => {
    if (!nodeById.has(nodeId)) {
      throw new Error("The graph node order references an absent node.");
    }
    orderIndex.set(nodeId, index);
  });

  const outgoing = new Map<string, string[]>();
  const edgeIds = new Set<string>();
  for (const value of array(payload.edges, "graph edges")) {
    const edge = record(value, "graph edge");
    const edgeId = nonemptyString(edge.id, "edge identity");
    if (edgeIds.has(edgeId)) throw new Error("The graph repeats an edge identity.");
    edgeIds.add(edgeId);
    const source = record(edge.source, "edge source");
    const destination = record(edge.destination, "edge destination");
    const sourceId = nonemptyString(source.node_id, "edge source node");
    const destinationId = nonemptyString(
      destination.node_id,
      "edge destination node",
    );
    const sourceNode = nodeById.get(sourceId);
    const destinationNode = nodeById.get(destinationId);
    if (sourceNode === undefined || destinationNode === undefined) {
      throw new Error("A graph edge references an absent node.");
    }
    const sourcePortId = nonemptyString(source.port_id, "edge source port");
    const destinationPortId = nonemptyString(
      destination.port_id,
      "edge destination port",
    );
    if (
      !sourceNode.outputIds.has(sourcePortId) ||
      !destinationNode.inputIds.has(destinationPortId)
    ) {
      throw new Error("A graph edge references an absent or misdirected port.");
    }
    const destinations = outgoing.get(sourceId) ?? [];
    destinations.push(destinationId);
    outgoing.set(sourceId, destinations);
  }
  for (const destinations of outgoing.values()) {
    destinations.sort(
      (left, right) => orderIndex.get(left)! - orderIndex.get(right)!,
    );
  }

  const driven = new Set<string>();
  for (const value of array(payload.parameter_drivers ?? [], "parameter drivers")) {
    const driver = record(value, "parameter driver");
    const target = record(driver.target, "parameter driver target");
    const address = parameterKey(
      nonemptyString(target.node_id, "driver node identity"),
      nonemptyString(target.parameter_id, "driver parameter identity"),
    );
    if (!parameterAddresses.has(address)) {
      throw new Error("A graph driver references an absent parameter.");
    }
    if (driven.has(address)) {
      throw new Error("The graph repeats a parameter driver target.");
    }
    driven.add(address);
  }

  const clipNodeIds = [...nodeById.values()]
    .filter(
      (node) =>
        /^superi\.timeline\.[^.]+\.clip$/.test(node.nodeType) &&
        clipIdentity(node.value) === clipId,
    )
    .map((node) => node.id);
  if (clipNodeIds.length !== 1) {
    throw new Error(
      clipNodeIds.length === 0
        ? "The selected clip has no canonical graph node."
        : "The selected clip has more than one canonical graph node.",
    );
  }

  const visited = new Set<string>(clipNodeIds);
  const pending = [...(outgoing.get(clipNodeIds[0]) ?? [])];
  const transformIds: string[] = [];
  while (pending.length > 0) {
    const nodeId = pending.shift();
    if (nodeId === undefined || visited.has(nodeId)) continue;
    visited.add(nodeId);
    const node = nodeById.get(nodeId);
    if (node === undefined) throw new Error("Graph traversal lost a node.");
    if (node.nodeType.startsWith(TIMELINE_NODE_PREFIX)) continue;
    if (node.nodeType === TRANSFORM_NODE_TYPE) transformIds.push(nodeId);
    pending.push(...(outgoing.get(nodeId) ?? []));
  }
  transformIds.sort(
    (left, right) => orderIndex.get(left)! - orderIndex.get(right)!,
  );
  if (transformIds.length === 0) {
    throw new Error("The selected clip has no attached built-in transform node.");
  }

  const transforms = transformIds.map((nodeId) =>
    projectTransformNode(graph.graph_id, graph.graph_revision, nodeById.get(nodeId)!, driven),
  );
  return Object.freeze({
    status: "ready" as const,
    projectRevision: snapshot.project.project_revision,
    timelineId,
    clipId,
    graphId: graph.graph_id,
    graphRevision: graph.graph_revision,
    transforms: Object.freeze(transforms),
  });
}

function projectTransformNode(
  graphId: string,
  graphRevision: number,
  node: GraphNodeRecord,
  driven: ReadonlySet<string>,
): ViewerTransformNodePresentation {
  assertTransformSchema(node.schema);
  const parameterByName = new Map<string, JsonRecord>();
  for (const value of array(node.value.parameters, "transform parameters")) {
    const parameter = record(value, "transform parameter");
    const name = nonemptyString(parameter.name, "transform parameter name");
    if (parameterByName.has(name)) {
      throw new Error("The transform node repeats a parameter name.");
    }
    parameterByName.set(name, parameter);
  }
  if (parameterByName.size !== 10) {
    throw new Error("The transform node must contain exactly ten parameters.");
  }

  const matrix = VIEWER_TRANSFORM_MATRIX_PARAMETER_NAMES.map((name) => {
    const parameter = requiredParameter(parameterByName, name);
    const parameterId = nonemptyString(parameter.id, "transform parameter identity");
    if (parameter.value_type !== SCALAR_VALUE_TYPE) {
      throw new Error(`Transform parameter ${name} has the wrong value type.`);
    }
    const value = scalarFromCanonicalBits(record(parameter.payload, "transform payload").scalar);
    if (value === null) {
      throw new Error(
        `Transform parameter ${name} has an invalid canonical scalar payload.`,
      );
    }
    return Object.freeze({
      name,
      label: MATRIX_PARAMETER_LABELS[name],
      parameterId,
      value,
      driven: driven.has(parameterKey(node.id, parameterId)),
    });
  });
  const samplingParameter = requiredParameter(parameterByName, "sampling");
  const samplingId = nonemptyString(
    samplingParameter.id,
    "transform sampling identity",
  );
  if (samplingParameter.value_type !== CHOICE_VALUE_TYPE) {
    throw new Error("Transform sampling has the wrong value type.");
  }
  const samplingValue = record(
    samplingParameter.payload,
    "transform sampling payload",
  ).choice;
  if (samplingValue !== "nearest" && samplingValue !== "bilinear") {
    throw new Error("Transform sampling contains an unsupported value.");
  }
  const sampling = Object.freeze({
    parameterId: samplingId,
    value: samplingValue,
    driven: driven.has(parameterKey(node.id, samplingId)),
  });
  return Object.freeze({
    graphId,
    graphRevision,
    nodeId: node.id,
    nodeType: TRANSFORM_NODE_TYPE,
    schemaVersion: "1.0.0",
    matrix: Object.freeze(matrix),
    matrixDriven: matrix.some((parameter) => parameter.driven),
    sampling,
  });
}

function assertTransformSchema(schema: JsonRecord): void {
  const id = record(schema.id, "transform schema identity");
  if (schemaKey(id) !== `${TRANSFORM_NODE_TYPE}@1.0.0`) {
    throw new Error("The transform node does not use the exact built-in 1.0.0 schema.");
  }
  const parameters = array(schema.parameters, "transform schema parameters");
  const expected = [
    ...VIEWER_TRANSFORM_MATRIX_PARAMETER_NAMES.map((name) => ({
      name,
      valueType: SCALAR_VALUE_TYPE,
    })),
    { name: "sampling", valueType: CHOICE_VALUE_TYPE },
  ];
  if (parameters.length !== expected.length) {
    throw new Error("The transform schema has an unexpected parameter count.");
  }
  for (let index = 0; index < expected.length; index += 1) {
    const parameter = record(parameters[index], "transform schema parameter");
    const required = expected[index]!;
    if (
      parameter.name !== required.name ||
      parameter.value_type !== required.valueType ||
      parameter.animatable !== true
    ) {
      throw new Error("The transform schema parameter contract is unsupported.");
    }
  }
}

function clipIdentity(node: JsonRecord): string | null {
  for (const value of array(node.parameters, "timeline node parameters")) {
    const parameter = record(value, "timeline node parameter");
    if (parameter.name !== "object-id") continue;
    if (parameter.value_type !== TIMELINE_OBJECT_VALUE_TYPE) {
      throw new Error("A timeline clip object identity has the wrong value type.");
    }
    const payload = record(parameter.payload, "timeline object payload");
    const domain = record(payload.domain, "timeline object identity");
    if (domain.kind !== "editorial_object_id") {
      throw new Error("A timeline clip contains an unsupported object identity.");
    }
    const identity = record(domain.value, "timeline editorial identity");
    return identity.kind === "clip"
      ? nonemptyString(identity.id, "timeline clip identity")
      : null;
  }
  throw new Error("A timeline clip graph node is missing its object identity.");
}

function requiredParameter(
  parameters: ReadonlyMap<string, JsonRecord>,
  name: string,
): JsonRecord {
  const parameter = parameters.get(name);
  if (parameter === undefined) {
    throw new Error(`The transform node is missing parameter ${name}.`);
  }
  return parameter;
}

function portIdentities(value: unknown, label: string): ReadonlySet<string> {
  const identities = new Set<string>();
  for (const portValue of array(value, label)) {
    const identity = nonemptyString(record(portValue, "node port").id, "port identity");
    if (identities.has(identity)) {
      throw new Error("A graph node repeats a port identity.");
    }
    identities.add(identity);
  }
  return identities;
}

function scalarFromCanonicalBits(value: unknown): number | null {
  if (
    typeof value !== "number" ||
    !Number.isFinite(value) ||
    !Number.isInteger(value) ||
    value < 0
  ) {
    return null;
  }
  const bits = BigInt(value);
  if (bits > 0xffff_ffff_ffff_ffffn) return null;
  const bytes = new ArrayBuffer(8);
  const view = new DataView(bytes);
  view.setBigUint64(0, bits, false);
  const scalar = view.getFloat64(0, false);
  return Number.isFinite(scalar) ? scalar : null;
}

function schemaKey(schemaId: JsonRecord): string {
  const version = record(schemaId.schema_version, "schema version");
  const parts = [version.major, version.minor, version.patch];
  if (parts.some((part) => !Number.isSafeInteger(part) || Number(part) < 0)) {
    throw new Error("A graph schema has an invalid semantic version.");
  }
  return (
    `${nonemptyString(schemaId.node_type, "schema node type")}@` +
    `${parts[0]}.${parts[1]}.${parts[2]}`
  );
}

function parameterKey(nodeId: string, parameterId: string): string {
  return `${nodeId}\u0000${parameterId}`;
}

function canonicalUnsigned(value: unknown, label: string): bigint {
  if (typeof value !== "string" || !/^(?:0|[1-9][0-9]*)$/.test(value)) {
    throw new Error(`${label} is not a canonical unsigned integer.`);
  }
  return BigInt(value);
}

function record(value: unknown, label: string): JsonRecord {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`${label} is not an object.`);
  }
  return value as JsonRecord;
}

function array(value: unknown, label: string): unknown[] {
  if (!Array.isArray(value)) throw new Error(`${label} is not an array.`);
  return value;
}

function nonemptyString(value: unknown, label: string): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new Error(`${label} is not a nonempty string.`);
  }
  return value;
}

function unavailable(reason: string): Extract<
  ViewerTransformProjection,
  { status: "unavailable" }
> {
  return Object.freeze({ status: "unavailable", reason });
}
