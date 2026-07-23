import type {
  EditorGraphValue,
  EditorStateSnapshot,
  ProjectAction,
} from "./api.ts";
import type {
  TimelineCanvasItem,
  TimelineCanvasModel,
  TimelineExactDuration,
  TimelineRate,
} from "./timeline-workspace.ts";

const GRAPH_FORMAT = "superi.graph";
const GRAPH_FORMAT_REVISION = 1;
const TIMELINE_NODE_PREFIX = "superi.timeline.";
const BUILT_IN_WIPE_DIRECTIONS = [
  "left-to-right",
  "right-to-left",
  "top-to-bottom",
  "bottom-to-top",
] as const;

type JsonRecord = Record<string, unknown>;

export type TimelineTransitionAlignment =
  | "start"
  | "center"
  | "end"
  | "custom";

export type TimelineTransitionParameterKind =
  | "scalar"
  | "boolean"
  | "choice"
  | "unsupported";

export type TimelineTransitionParameterRestriction =
  | "host_owned"
  | "driven"
  | "unsupported"
  | null;

export interface TimelineTransitionParameterPresentation {
  readonly graphId: string;
  readonly nodeId: string;
  readonly nodeType: string;
  readonly parameterId: string;
  readonly name: string;
  readonly label: string;
  readonly valueType: string;
  readonly kind: TimelineTransitionParameterKind;
  readonly value: number | boolean | string | null;
  readonly animatable: boolean;
  readonly driven: boolean;
  readonly editable: boolean;
  readonly restriction: TimelineTransitionParameterRestriction;
  readonly choices: readonly string[];
}

export interface TimelineTransitionEffectPresentation {
  readonly nodeId: string;
  readonly nodeType: string;
  readonly label: string;
  readonly parameters: readonly TimelineTransitionParameterPresentation[];
}

export type TimelineTransitionGraphPresentation =
  | {
      readonly status: "ready";
      readonly graphId: string;
      readonly graphRevision: number;
      readonly effects: readonly TimelineTransitionEffectPresentation[];
    }
  | {
      readonly status: "unavailable";
      readonly reason: string;
      readonly effects: readonly [];
    };

export interface TimelineTransitionPresentation {
  readonly projectId: string;
  readonly projectRevision: number;
  readonly timelineId: string;
  readonly trackId: string;
  readonly id: string;
  readonly name: string;
  readonly from: { readonly kind: string; readonly id: string };
  readonly to: { readonly kind: string; readonly id: string };
  readonly fromOffset: TimelineExactDuration;
  readonly toOffset: TimelineExactDuration;
  readonly duration: TimelineExactDuration;
  readonly maximumFromOffset: TimelineExactDuration;
  readonly maximumToOffset: TimelineExactDuration;
  readonly alignment: TimelineTransitionAlignment;
  readonly graph: TimelineTransitionGraphPresentation;
}

export interface TimelineTransitionProjection {
  readonly transitions: readonly TimelineTransitionPresentation[];
}

export interface TimelineTransitionHandleValues {
  readonly fromOffsetValue: string;
  readonly toOffsetValue: string;
}

interface TransitionTimingProjection
  extends Omit<TimelineTransitionPresentation, "graph"> {}

interface GraphProjection {
  readonly byTransitionId: ReadonlyMap<
    string,
    Extract<TimelineTransitionGraphPresentation, { status: "ready" }>
  >;
  readonly failure: string | null;
}

export function projectTimelineTransitionDetails(
  snapshot: EditorStateSnapshot,
  model: TimelineCanvasModel,
): TimelineTransitionProjection {
  const timing = projectTransitionTiming(snapshot, model);
  const graph = projectTransitionGraphs(snapshot, snapshot.project.root_timeline_id);
  return deepFreeze({
    transitions: timing.map((transition) => ({
      ...transition,
      graph:
        graph.byTransitionId.get(transition.id) ??
        unavailableGraph(
          graph.failure ??
            `Compiled graph state for transition ${transition.id} is unavailable.`,
        ),
    })),
  });
}

export function transitionHandlesForAlignment(
  transition: TimelineTransitionPresentation,
  alignment: Exclude<TimelineTransitionAlignment, "custom">,
  durationValue: string,
): TimelineTransitionHandleValues | null {
  const duration = exactNonnegativeInteger(durationValue, "transition duration");
  if (duration === 0n) return null;
  let from: bigint;
  let to: bigint;
  switch (alignment) {
    case "start":
      from = 0n;
      to = duration;
      break;
    case "center":
      from = duration / 2n;
      to = duration - from;
      break;
    case "end":
      from = duration;
      to = 0n;
      break;
  }
  return handlesWithinLimits(transition, from, to);
}

export function transitionHandlesForDuration(
  transition: TimelineTransitionPresentation,
  durationValue: string,
): TimelineTransitionHandleValues | null {
  if (transition.alignment !== "custom") {
    return transitionHandlesForAlignment(
      transition,
      transition.alignment,
      durationValue,
    );
  }
  const duration = exactNonnegativeInteger(durationValue, "transition duration");
  if (duration === 0n) return null;
  const maximumFrom = BigInt(transition.maximumFromOffset.value);
  const maximumTo = BigInt(transition.maximumToOffset.value);
  if (duration > maximumFrom + maximumTo) return null;
  const currentFrom = BigInt(transition.fromOffset.value);
  const currentTo = BigInt(transition.toOffset.value);
  const currentDuration = currentFrom + currentTo;
  let from = (duration * currentFrom) / currentDuration;
  const minimumFrom = duration > maximumTo ? duration - maximumTo : 0n;
  const maximumAllowedFrom = minimumBigInt(maximumFrom, duration);
  from = maximumBigInt(minimumFrom, minimumBigInt(from, maximumAllowedFrom));
  return handlesWithinLimits(transition, from, duration - from);
}

export function buildSetTransitionAction(
  transition: TimelineTransitionPresentation,
  fromOffsetValue: string,
  toOffsetValue: string,
): ProjectAction {
  const from = exactNonnegativeInteger(fromOffsetValue, "from handle");
  const to = exactNonnegativeInteger(toOffsetValue, "to handle");
  if (from === 0n && to === 0n) {
    throw new Error("Transition duration must be at least one exact track unit.");
  }
  const handles = handlesWithinLimits(transition, from, to);
  if (handles === null) {
    throw new Error("Transition handles exceed the available adjacent media.");
  }
  if (
    handles.fromOffsetValue === transition.fromOffset.value &&
    handles.toOffsetValue === transition.toOffset.value
  ) {
    throw new Error("Transition timing already matches the canonical state.");
  }
  const fromNumber = safeWireInteger(from, "from handle");
  const toNumber = safeWireInteger(to, "to handle");
  return {
    action: "edit_timeline",
    operations: [
      {
        operation: "set_transition",
        timeline_id: transition.timelineId,
        track_id: transition.trackId,
        transition_id: transition.id,
        from_offset: {
          value: fromNumber,
          timebase: transition.fromOffset.timebase,
        },
        to_offset: {
          value: toNumber,
          timebase: transition.toOffset.timebase,
        },
      },
    ],
  };
}

export function buildTransitionParameterAction(
  parameter: TimelineTransitionParameterPresentation,
  value: number | boolean | string,
): ProjectAction {
  if (!parameter.editable) {
    throw new Error(`${parameter.label} is inspectable but not directly editable.`);
  }
  let graphValue: EditorGraphValue;
  switch (parameter.kind) {
    case "scalar": {
      const scalar = typeof value === "number" ? value : Number(value);
      if (!Number.isFinite(scalar)) {
        throw new Error(`${parameter.label} must be a finite number.`);
      }
      graphValue = { kind: "scalar", value: scalar };
      break;
    }
    case "boolean":
      if (typeof value !== "boolean") {
        throw new Error(`${parameter.label} must be true or false.`);
      }
      graphValue = { kind: "boolean", value };
      break;
    case "choice": {
      if (typeof value !== "string" || value.trim().length === 0) {
        throw new Error(`${parameter.label} must be a nonempty choice.`);
      }
      if (parameter.choices.length > 0 && !parameter.choices.includes(value)) {
        throw new Error(`${parameter.label} is not one of the supported choices.`);
      }
      graphValue = { kind: "choice", value };
      break;
    }
    case "unsupported":
      throw new Error(`${parameter.label} uses an unsupported parameter type.`);
  }
  return {
    action: "mutate_graph",
    graph_id: parameter.graphId,
    mutations: [
      {
        operation: "set_parameter",
        node_id: parameter.nodeId,
        parameter_id: parameter.parameterId,
        value: { value_type: parameter.valueType, value: graphValue },
      },
    ],
  };
}

function projectTransitionTiming(
  snapshot: EditorStateSnapshot,
  model: TimelineCanvasModel,
): TransitionTimingProjection[] {
  const result: TransitionTimingProjection[] = [];
  for (const track of model.tracks) {
    for (const [index, item] of track.items.entries()) {
      if (item.kind !== "transition" || item.transition === null) continue;
      const previous = track.items[index - 1];
      const next = track.items[index + 1];
      if (!previous || previous.transition || !next || next.transition) {
        throw new Error(`Transition ${item.id} has invalid adjacent timing state.`);
      }
      const from = exactDurationUnits(item.transition.fromOffset, model.editRate);
      const to = exactDurationUnits(item.transition.toOffset, model.editRate);
      const previousDuration = exactDurationUnits(
        previous.recordRange.duration,
        model.editRate,
      );
      const nextDuration = exactDurationUnits(
        next.recordRange.duration,
        model.editRate,
      );
      const incoming = track.items[index - 2];
      const outgoing = track.items[index + 2];
      const incomingTo =
        incoming?.kind === "transition" && incoming.transition
          ? exactDurationUnits(incoming.transition.toOffset, model.editRate)
          : 0n;
      const outgoingFrom =
        outgoing?.kind === "transition" && outgoing.transition
          ? exactDurationUnits(outgoing.transition.fromOffset, model.editRate)
          : 0n;
      if (incomingTo > previousDuration || outgoingFrom > nextDuration) {
        throw new Error(`Transition ${item.id} overlaps invalid adjacent handles.`);
      }
      const maximumFrom = previousDuration - incomingTo;
      const maximumTo = nextDuration - outgoingFrom;
      if (from > maximumFrom || to > maximumTo || from + to === 0n) {
        throw new Error(`Transition ${item.id} exceeds available adjacent handles.`);
      }
      result.push({
        projectId: model.projectId,
        projectRevision: snapshot.project.project_revision,
        timelineId: model.id,
        trackId: track.id,
        id: item.id,
        name: item.name,
        from: item.transition.from,
        to: item.transition.to,
        fromOffset: item.transition.fromOffset,
        toOffset: item.transition.toOffset,
        duration: exactDuration(from + to, model.editRate),
        maximumFromOffset: exactDuration(maximumFrom, model.editRate),
        maximumToOffset: exactDuration(maximumTo, model.editRate),
        alignment: transitionAlignment(from, to),
      });
    }
  }
  return result;
}

function projectTransitionGraphs(
  snapshot: EditorStateSnapshot,
  rootTimelineId: string,
): GraphProjection {
  try {
    const documents = array(record(snapshot.graph).documents).map(record);
    const matching = documents.filter((document) => {
      const scope = record(document.scope);
      return scope.kind === "timeline" && scope.root_timeline_id === rootTimelineId;
    });
    if (matching.length !== 1) {
      throw new Error("The root timeline does not have one exact compiled graph document.");
    }
    const matchingDocument = matching[0]!;
    const graphId = nonemptyString(matchingDocument.graph_id, "graph identity");
    const graphRevision = safeNonnegativeNumber(
      matchingDocument.graph_revision,
      "graph revision",
    );
    const document = record(matchingDocument.document);
    if (
      document.format !== GRAPH_FORMAT ||
      document.format_revision !== GRAPH_FORMAT_REVISION
    ) {
      throw new Error("The compiled graph document format is unsupported.");
    }
    const envelope = record(document.content);
    if (
      envelope.format !== GRAPH_FORMAT ||
      envelope.format_revision !== GRAPH_FORMAT_REVISION
    ) {
      throw new Error("The compiled graph envelope format is unsupported.");
    }
    const payload = record(envelope.payload);
    if (nonemptyString(payload.graph_id, "payload graph identity") !== graphId) {
      throw new Error("The compiled graph identity changed inside its envelope.");
    }

    const schemaByKey = new Map<string, JsonRecord>();
    for (const schemaValue of array(payload.schemas)) {
      const schema = record(schemaValue);
      const key = schemaKey(record(schema.id));
      if (schemaByKey.has(key)) throw new Error("The graph repeats a node schema.");
      schemaByKey.set(key, schema);
    }

    const nodeById = new Map<string, JsonRecord>();
    const nodeTypeById = new Map<string, string>();
    const transitionNodeById = new Map<string, string>();
    for (const nodeValue of array(payload.nodes)) {
      const node = record(nodeValue);
      const nodeId = nonemptyString(node.id, "node identity");
      const schemaId = record(node.schema);
      const nodeType = nonemptyString(schemaId.node_type, "node type");
      if (!schemaByKey.has(schemaKey(schemaId))) {
        throw new Error("A graph node references an absent schema.");
      }
      if (nodeById.has(nodeId)) throw new Error("The graph repeats a node identity.");
      nodeById.set(nodeId, node);
      nodeTypeById.set(nodeId, nodeType);
      if (/^superi\.timeline\.[^.]+\.transition$/.test(nodeType)) {
        const transitionId = transitionIdFromNode(node);
        if (transitionId !== null) {
          if (transitionNodeById.has(transitionId)) {
            throw new Error("The graph repeats a transition identity.");
          }
          transitionNodeById.set(transitionId, nodeId);
        }
      }
    }

    const outgoing = new Map<string, string[]>();
    for (const edgeValue of array(payload.edges)) {
      const edge = record(edgeValue);
      const source = nonemptyString(record(edge.source).node_id, "edge source");
      const destination = nonemptyString(
        record(edge.destination).node_id,
        "edge destination",
      );
      if (!nodeById.has(source) || !nodeById.has(destination)) {
        throw new Error("A graph edge references an absent node.");
      }
      const destinations = outgoing.get(source) ?? [];
      destinations.push(destination);
      outgoing.set(source, destinations);
    }

    const driven = new Set<string>();
    for (const driverValue of array(payload.parameter_drivers ?? [])) {
      const target = record(record(driverValue).target);
      driven.add(
        parameterKey(
          nonemptyString(target.node_id, "driver node identity"),
          nonemptyString(target.parameter_id, "driver parameter identity"),
        ),
      );
    }

    const byTransitionId = new Map<
      string,
      Extract<TimelineTransitionGraphPresentation, { status: "ready" }>
    >();
    for (const [transitionId, transitionNodeId] of transitionNodeById) {
      const visited = new Set<string>([transitionNodeId]);
      const pending = [...(outgoing.get(transitionNodeId) ?? [])];
      const effects: TimelineTransitionEffectPresentation[] = [];
      while (pending.length > 0) {
        const nodeId = pending.shift();
        if (nodeId === undefined || visited.has(nodeId)) continue;
        visited.add(nodeId);
        const nodeType = nodeTypeById.get(nodeId);
        const node = nodeById.get(nodeId);
        if (!nodeType || !node) throw new Error("Graph traversal lost a node.");
        if (nodeType.startsWith(TIMELINE_NODE_PREFIX)) continue;
        const schema = schemaByKey.get(schemaKey(record(node.schema)));
        if (!schema) throw new Error("Graph traversal lost a node schema.");
        effects.push({
          nodeId,
          nodeType,
          label: effectLabel(nodeType),
          parameters: projectParameters(graphId, nodeId, nodeType, node, schema, driven),
        });
        pending.push(...(outgoing.get(nodeId) ?? []));
      }
      byTransitionId.set(transitionId, {
        status: "ready",
        graphId,
        graphRevision,
        effects,
      });
    }
    return { byTransitionId, failure: null };
  } catch (error: unknown) {
    return {
      byTransitionId: new Map(),
      failure:
        error instanceof Error
          ? error.message
          : "Compiled transition graph detail is unavailable.",
    };
  }
}

function projectParameters(
  graphId: string,
  nodeId: string,
  nodeType: string,
  node: JsonRecord,
  schema: JsonRecord,
  driven: ReadonlySet<string>,
): TimelineTransitionParameterPresentation[] {
  const parameterSchemaByName = new Map<string, JsonRecord>();
  for (const value of array(schema.parameters)) {
    const parameter = record(value);
    parameterSchemaByName.set(
      nonemptyString(parameter.name, "schema parameter name"),
      parameter,
    );
  }
  return array(node.parameters).map((value) => {
    const parameter = record(value);
    const parameterId = nonemptyString(parameter.id, "parameter identity");
    const name = nonemptyString(parameter.name, "parameter name");
    const valueType = nonemptyString(parameter.value_type, "parameter value type");
    const parameterSchema = parameterSchemaByName.get(name);
    if (
      !parameterSchema ||
      parameterSchema.value_type !== valueType ||
      typeof parameterSchema.animatable !== "boolean"
    ) {
      throw new Error("A graph parameter does not match its schema.");
    }
    const parsed = parseParameterPayload(record(parameter.payload));
    const hostOwned = nodeType.startsWith("superi.transition.") && name === "progress";
    const isDriven = driven.has(parameterKey(nodeId, parameterId));
    const restriction: TimelineTransitionParameterRestriction = hostOwned
      ? "host_owned"
      : isDriven
        ? "driven"
        : parsed.kind === "unsupported"
          ? "unsupported"
          : null;
    return {
      graphId,
      nodeId,
      nodeType,
      parameterId,
      name,
      label: parameterLabel(name),
      valueType,
      kind: parsed.kind,
      value: parsed.value,
      animatable: parameterSchema.animatable,
      driven: isDriven,
      editable: restriction === null,
      restriction,
      choices:
        nodeType === "superi.transition.directional-wipe" && name === "direction"
          ? [...BUILT_IN_WIPE_DIRECTIONS]
          : [],
    };
  });
}

function parseParameterPayload(payload: JsonRecord): {
  readonly kind: TimelineTransitionParameterKind;
  readonly value: number | boolean | string | null;
} {
  const scalar = scalarFromCanonicalBits(payload.scalar);
  if (scalar !== null) {
    return { kind: "scalar", value: scalar };
  }
  if (typeof payload.boolean === "boolean") {
    return { kind: "boolean", value: payload.boolean };
  }
  if (typeof payload.choice === "string" && payload.choice.length > 0) {
    return { kind: "choice", value: payload.choice };
  }
  return { kind: "unsupported", value: null };
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

function transitionIdFromNode(node: JsonRecord): string | null {
  for (const value of array(node.parameters)) {
    const parameter = record(value);
    if (parameter.name !== "object-id") continue;
    const payload = record(parameter.payload);
    const domain = "domain" in payload ? record(payload.domain) : null;
    if (domain?.kind !== "editorial_object_id") return null;
    const identity = record(domain.value);
    return identity.kind === "transition"
      ? nonemptyString(identity.id, "transition identity")
      : null;
  }
  return null;
}

function handlesWithinLimits(
  transition: TimelineTransitionPresentation,
  from: bigint,
  to: bigint,
): TimelineTransitionHandleValues | null {
  if (
    from < 0n ||
    to < 0n ||
    from > BigInt(transition.maximumFromOffset.value) ||
    to > BigInt(transition.maximumToOffset.value) ||
    from + to === 0n
  ) {
    return null;
  }
  return {
    fromOffsetValue: from.toString(),
    toOffsetValue: to.toString(),
  };
}

function exactDuration(value: bigint, timebase: TimelineRate): TimelineExactDuration {
  return { value: value.toString(), timebase };
}

function exactDurationUnits(
  duration: TimelineExactDuration,
  timebase: TimelineRate,
): bigint {
  if (
    duration.timebase.numerator !== timebase.numerator ||
    duration.timebase.denominator !== timebase.denominator
  ) {
    throw new Error("Transition timing does not use the exact timeline clock.");
  }
  return exactNonnegativeInteger(duration.value, "transition handle");
}

function transitionAlignment(
  from: bigint,
  to: bigint,
): TimelineTransitionAlignment {
  if (from === 0n) return "start";
  if (to === 0n) return "end";
  if (from === to) return "center";
  return "custom";
}

function exactNonnegativeInteger(value: string, label: string): bigint {
  if (!/^(0|[1-9][0-9]*)$/.test(value)) {
    throw new Error(`${label} must be an exact nonnegative integer.`);
  }
  return BigInt(value);
}

function safeWireInteger(value: bigint, label: string): number {
  if (value > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error(`${label} exceeds the desktop's safe public integer range.`);
  }
  return Number(value);
}

function unavailableGraph(reason: string): TimelineTransitionGraphPresentation {
  return { status: "unavailable", reason, effects: [] };
}

function schemaKey(schemaId: JsonRecord): string {
  return `${nonemptyString(schemaId.node_type, "schema node type")}:${JSON.stringify(
    schemaId.schema_version,
  )}`;
}

function parameterKey(nodeId: string, parameterId: string): string {
  return `${nodeId}\u0000${parameterId}`;
}

function effectLabel(nodeType: string): string {
  return nodeType
    .split(".")
    .slice(-2)
    .flatMap((segment) => segment.split(/[_-]/))
    .filter(Boolean)
    .map((word) => `${word.charAt(0).toUpperCase()}${word.slice(1)}`)
    .join(" ");
}

function parameterLabel(name: string): string {
  return name
    .split(/[_-]/)
    .filter(Boolean)
    .map((word) => `${word.charAt(0).toUpperCase()}${word.slice(1)}`)
    .join(" ");
}

function record(value: unknown): JsonRecord {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error("Expected structured graph state.");
  }
  return value as JsonRecord;
}

function array(value: unknown): unknown[] {
  if (!Array.isArray(value)) throw new Error("Expected an ordered graph list.");
  return value;
}

function nonemptyString(value: unknown, label: string): string {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new Error(`Expected ${label}.`);
  }
  return value;
}

function safeNonnegativeNumber(value: unknown, label: string): number {
  if (
    typeof value !== "number" ||
    !Number.isSafeInteger(value) ||
    value < 0
  ) {
    throw new Error(`Expected exact ${label}.`);
  }
  return value;
}

function minimumBigInt(left: bigint, right: bigint): bigint {
  return left < right ? left : right;
}

function maximumBigInt(left: bigint, right: bigint): bigint {
  return left > right ? left : right;
}

function deepFreeze<T>(value: T): T {
  if (typeof value !== "object" || value === null || Object.isFrozen(value)) {
    return value;
  }
  for (const child of Object.values(value as Record<string, unknown>)) {
    deepFreeze(child);
  }
  return Object.freeze(value);
}
