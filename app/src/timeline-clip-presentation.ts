import type { EditorStateSnapshot } from "./api.ts";
import {
  projectTimelineDocument,
  type TimelineCanvasItem,
  type TimelineCanvasModel,
  type TimelineExactPoint,
  type TimelineExactRange,
  type TimelineRate,
  type TimelineTrackKind,
} from "./timeline-workspace.ts";

export interface TimelineMediaSourcePresentation {
  readonly kind: "media";
  readonly id: string;
  readonly name: string;
  readonly target: string | null;
  readonly relinkStatus:
    | "online"
    | "missing"
    | "unverified"
    | "fingerprint_mismatch"
    | "unavailable";
}

export interface TimelineNestedSourcePresentation {
  readonly kind: "timeline";
  readonly id: string;
  readonly name: string;
}

export type TimelineClipSourcePresentation =
  | TimelineMediaSourcePresentation
  | TimelineNestedSourcePresentation;

export interface TimelineClipTimeMapSegment {
  readonly recordRange: TimelineExactRange;
  readonly sourceStart: TimelineExactPoint;
  readonly rateNumerator: string;
  readonly rateDenominator: string;
}

export interface TimelineClipTimeMap {
  readonly recordDuration: {
    readonly value: string;
    readonly timebase: TimelineRate;
  };
  readonly sourceTimebase: TimelineRate;
  readonly segments: readonly TimelineClipTimeMapSegment[];
}

export interface TimelineClipEffectPresentation {
  readonly nodeId: string;
  readonly nodeType: string;
  readonly label: string;
  readonly driverCount: number;
}

export interface TimelineClipMarkerPresentation {
  readonly id: string;
  readonly label: string | null;
  readonly flag: string | null;
  readonly note: string | null;
}

export interface TimelineClipMulticamPresentation {
  readonly syncMethod: string;
  readonly switchCount: number;
  readonly audioPolicy: string;
}

export interface TimelineClipAutomationKeyframe {
  readonly sample: number;
  readonly sampleRate: number;
  readonly value: number;
}

export interface TimelineClipAutomationPass {
  readonly startSample: number;
  readonly sampleRate: number;
  readonly currentValue: number;
  readonly touchActive: boolean;
  readonly latchActive: boolean;
}

export interface TimelineClipAutomationPresentation {
  readonly sampleRate: number;
  readonly defaultGain: number;
  readonly mode: "read" | "write" | "touch" | "latch";
  readonly keyframes: readonly TimelineClipAutomationKeyframe[];
  readonly activePass: TimelineClipAutomationPass | null;
}

export interface TimelineClipPresentation {
  readonly id: string;
  readonly name: string;
  readonly timelineId: string;
  readonly trackId: string;
  readonly trackName: string;
  readonly trackKind: TimelineTrackKind;
  readonly targeted: boolean;
  readonly syncLocked: boolean;
  readonly source: TimelineClipSourcePresentation;
  readonly sourceRange: TimelineExactRange;
  readonly recordRange: TimelineExactRange;
  readonly timeMap: TimelineClipTimeMap;
  readonly startSeconds: number;
  readonly endSeconds: number;
  readonly geometry: {
    readonly leftPercent: number;
    readonly widthPercent: number;
  };
  readonly canonicalSelected: boolean;
  readonly retimed: boolean;
  readonly linkedClipIds: readonly string[];
  readonly groupedClipIds: readonly string[];
  readonly markers: readonly TimelineClipMarkerPresentation[];
  readonly metadataKeys: readonly string[];
  readonly multicam: TimelineClipMulticamPresentation | null;
  readonly effects: readonly TimelineClipEffectPresentation[];
  readonly automation: TimelineClipAutomationPresentation | null;
}

export interface TimelineClipProjectionReady {
  readonly status: "ready";
  readonly projectRevision: number;
  readonly timelineId: string;
  readonly clips: readonly TimelineClipPresentation[];
}

export interface TimelineClipProjectionUnavailable {
  readonly status: "unavailable";
  readonly reason: string;
}

export type TimelineClipProjection =
  | TimelineClipProjectionReady
  | TimelineClipProjectionUnavailable;

type JsonRecord = Record<string, unknown>;

const TIMELINE_FORMAT = "superi.timeline";
const GRAPH_FORMAT = "superi.graph";
const TIMELINE_FORMAT_REVISION = 2;
const GRAPH_FORMAT_REVISION = 1;
const SIGNED_DECIMAL = /^(?:0|-[1-9][0-9]*|[1-9][0-9]*)$/;
const UNSIGNED_DECIMAL = /^(?:0|[1-9][0-9]*)$/;
const UNAVAILABLE_REASON =
  "Timeline clip detail is malformed or uses an unsupported contract.";

export function projectTimelineClips(
  snapshot: EditorStateSnapshot,
): TimelineClipProjection {
  try {
    const model = projectTimelineDocument(
      snapshot.timeline.document,
      snapshot.project.root_timeline_id,
    );
    return projectTimelineClipDetails(snapshot, model);
  } catch {
    return deepFreeze({ status: "unavailable", reason: UNAVAILABLE_REASON });
  }
}

export function projectTimelineClipDetails(
  snapshotValue: EditorStateSnapshot,
  model: TimelineCanvasModel,
): TimelineClipProjection {
  try {
    return deepFreeze(projectTimelineClipDetailsChecked(snapshotValue, model));
  } catch {
    return deepFreeze({ status: "unavailable", reason: UNAVAILABLE_REASON });
  }
}

export function formatTimelineClipTiming(
  clip: TimelineClipPresentation,
): string {
  return [
    `source ${formatExactRange(clip.sourceRange)}`,
    `record ${formatExactRange(clip.recordRange)}`,
  ].join("; ");
}

export function timelineClipAutomationKeyPercent(
  clip: Pick<TimelineClipPresentation, "recordRange">,
  keyframe: TimelineClipAutomationKeyframe,
): number | null {
  const recordStartValue = Number(clip.recordRange.start.value);
  const recordDurationValue = Number(clip.recordRange.duration.value);
  const startRate = clip.recordRange.start.timebase;
  const durationRate = clip.recordRange.duration.timebase;
  if (
    !Number.isSafeInteger(recordStartValue) ||
    !Number.isSafeInteger(recordDurationValue) ||
    recordDurationValue <= 0 ||
    !Number.isSafeInteger(keyframe.sample) ||
    !Number.isSafeInteger(keyframe.sampleRate) ||
    keyframe.sampleRate <= 0
  ) {
    return null;
  }
  const recordStartSeconds =
    (recordStartValue * startRate.denominator) / startRate.numerator;
  const recordDurationSeconds =
    (recordDurationValue * durationRate.denominator) /
    durationRate.numerator;
  const keyframeSeconds = keyframe.sample / keyframe.sampleRate;
  const ratio =
    (keyframeSeconds - recordStartSeconds) / recordDurationSeconds;
  if (!Number.isFinite(ratio) || ratio < 0 || ratio > 1) return null;
  return roundedPercent(ratio);
}

function projectTimelineClipDetailsChecked(
  snapshotValue: EditorStateSnapshot,
  model: TimelineCanvasModel,
): TimelineClipProjectionReady {
  const snapshot = record(snapshotValue);
  const project = record(snapshot.project);
  const projectRevision = nonnegativeSafeInteger(project.project_revision);
  const rootTimelineId = nonemptyString(project.root_timeline_id);
  if (
    rootTimelineId !== model.id ||
    nonemptyString(project.project_id) !== model.projectId ||
    canonicalUnsigned(model.projectRevision) !== BigInt(projectRevision)
  ) {
    throw new Error("timeline model does not match the editor snapshot");
  }

  const document = record(record(snapshot.timeline).document);
  const envelope = currentEnvelope(
    document.content,
    TIMELINE_FORMAT,
    TIMELINE_FORMAT_REVISION,
  );
  const payload = record(envelope.payload);
  const timelines = array(payload.timelines).map(record);
  const rootMatches = timelines.filter(
    (timeline) => timeline.id === rootTimelineId,
  );
  if (rootMatches.length !== 1) throw new Error("root timeline is not unique");
  const root = rootMatches[0];
  if (root === undefined) throw new Error("root timeline is absent");

  const media = mediaMap(payload.media);
  const timelineNames = new Map<string, string>();
  for (const timeline of timelines) {
    const id = nonemptyString(timeline.id);
    if (timelineNames.has(id)) throw new Error("duplicate timeline identity");
    timelineNames.set(id, nonemptyString(timeline.name));
  }
  const rawClips = rawClipMap(root.tracks);
  const markers = markerMap(root.markers);
  const metadata = metadataMap(root.metadata);
  const multicam = multicamMap(root);
  const effects = effectMap(snapshot, rootTimelineId);
  const automation = automationMap(snapshot);

  const clips: TimelineClipPresentation[] = [];
  for (const track of model.tracks) {
    for (const item of track.items) {
      if (item.kind !== "clip") continue;
      const raw = rawClips.get(item.id);
      if (raw === undefined) throw new Error("projected clip has no raw owner");
      const source = clipSource(item, media, timelineNames);
      const sourceRange = item.sourceRange;
      if (sourceRange === null) throw new Error("clip source range is absent");
      const timeMap = parseTimeMap(raw.time_map);
      clips.push({
        id: item.id,
        name: item.name,
        timelineId: model.id,
        trackId: track.id,
        trackName: track.name,
        trackKind: track.kind,
        targeted: track.targeted,
        syncLocked: track.syncLocked,
        source,
        sourceRange,
        recordRange: item.recordRange,
        timeMap,
        startSeconds: item.startSeconds,
        endSeconds: item.endSeconds,
        geometry: clipGeometry(item, model),
        canonicalSelected: item.selected,
        retimed: isRetimed(timeMap),
        linkedClipIds: peers(item.id, item.link),
        groupedClipIds: peers(item.id, item.group),
        markers: markers.get(item.id) ?? [],
        metadataKeys: metadata.get(item.id) ?? [],
        multicam: multicam.get(item.id) ?? null,
        effects: effects.get(item.id) ?? [],
        automation: automation.get(item.id) ?? null,
      });
    }
  }
  if (clips.length !== rawClips.size) {
    throw new Error("raw and projected clip identities differ");
  }

  return {
    status: "ready",
    projectRevision,
    timelineId: model.id,
    clips,
  };
}

function rawClipMap(value: unknown): Map<string, JsonRecord> {
  const result = new Map<string, JsonRecord>();
  for (const trackValue of array(value)) {
    const track = record(trackValue);
    for (const itemValue of array(track.items)) {
      const item = record(itemValue);
      if (item.kind !== "clip") continue;
      const id = nonemptyString(item.id);
      if (result.has(id)) throw new Error("duplicate raw clip identity");
      result.set(id, item);
    }
  }
  return result;
}

function mediaMap(value: unknown): Map<string, TimelineMediaSourcePresentation> {
  const result = new Map<string, TimelineMediaSourcePresentation>();
  for (const mediaValue of array(value)) {
    const media = record(mediaValue);
    const id = nonemptyString(media.id);
    const relink = record(media.relink_state);
    if (result.has(id)) throw new Error("duplicate media identity");
    result.set(id, {
      kind: "media",
      id,
      name: nonemptyString(media.name),
      target: string(media.target),
      relinkStatus: relinkStatus(relink.status),
    });
  }
  return result;
}

function clipSource(
  item: TimelineCanvasItem,
  media: ReadonlyMap<string, TimelineMediaSourcePresentation>,
  timelineNames: ReadonlyMap<string, string>,
): TimelineClipSourcePresentation {
  const source = item.source;
  if (source === null) throw new Error("clip source is absent");
  if (source.kind === "media") {
    return (
      media.get(source.id) ?? {
        kind: "media",
        id: source.id,
        name: source.id,
        target: null,
        relinkStatus: "unavailable",
      }
    );
  }
  const name = timelineNames.get(source.id);
  if (name === undefined) throw new Error("nested timeline is absent");
  return { kind: "timeline", id: source.id, name };
}

function parseTimeMap(value: unknown): TimelineClipTimeMap {
  const map = record(value);
  const recordDuration = exactDuration(map.record_duration);
  const sourceTimebase = rate(map.source_timebase);
  const segments = array(map.segments).map((segmentValue) => {
    const segment = record(segmentValue);
    const rateNumerator = canonicalSigned(segment.rate_numerator);
    const rateDenominator = canonicalSigned(segment.rate_denominator);
    if (BigInt(rateDenominator) === 0n) {
      throw new Error("time map rate denominator is zero");
    }
    return {
      recordRange: exactRange(segment.record_range),
      sourceStart: exactPoint(segment.source_start),
      rateNumerator,
      rateDenominator,
    };
  });
  return { recordDuration, sourceTimebase, segments };
}

function isRetimed(map: TimelineClipTimeMap): boolean {
  return (
    map.segments.length !== 1 ||
    map.segments.some(
      (segment) =>
        BigInt(segment.rateNumerator) !== BigInt(segment.rateDenominator),
    )
  );
}

function clipGeometry(
  item: TimelineCanvasItem,
  model: TimelineCanvasModel,
): { readonly leftPercent: number; readonly widthPercent: number } {
  if (model.durationSeconds <= 0) {
    return { leftPercent: 0, widthPercent: 0 };
  }
  return {
    leftPercent: roundedPercent(
      (item.startSeconds - model.startSeconds) / model.durationSeconds,
    ),
    widthPercent: roundedPercent(
      (item.endSeconds - item.startSeconds) / model.durationSeconds,
    ),
  };
}

function roundedPercent(ratio: number): number {
  return Math.min(100, Math.max(0, Number((ratio * 100).toFixed(6))));
}

function peers(id: string, relation: readonly string[] | null): readonly string[] {
  return relation?.filter((member) => member !== id) ?? [];
}

function markerMap(
  value: unknown,
): Map<string, readonly TimelineClipMarkerPresentation[]> {
  const result = new Map<string, TimelineClipMarkerPresentation[]>();
  for (const markerValue of array(value)) {
    const marker = record(markerValue);
    exactRange(marker.marked_range);
    array(marker.metadata);
    const owner = record(marker.owner);
    if (owner.kind !== "object") continue;
    const identity = record(owner.id);
    if (identity.kind !== "clip") continue;
    const clipId = nonemptyString(identity.id);
    const current = result.get(clipId) ?? [];
    current.push({
      id: nonemptyString(marker.id),
      label: nullableString(marker.label),
      flag: nullableString(marker.flag),
      note: nullableString(marker.note),
    });
    result.set(clipId, current);
  }
  return result;
}

function metadataMap(value: unknown): Map<string, readonly string[]> {
  const result = new Map<string, string[]>();
  for (const ownedValue of array(value)) {
    const owned = record(ownedValue);
    const owner = record(owned.owner);
    const keys = array(owned.entries).map((entryValue) => {
      const entry = record(entryValue);
      record(entry.value);
      return nonemptyString(entry.key);
    });
    if (owner.kind !== "object") continue;
    const identity = record(owner.id);
    if (identity.kind !== "clip") continue;
    const clipId = nonemptyString(identity.id);
    result.set(clipId, [...new Set([...(result.get(clipId) ?? []), ...keys])]);
  }
  return result;
}

function multicamMap(
  timeline: JsonRecord,
): Map<string, TimelineClipMulticamPresentation> {
  const result = new Map<string, TimelineClipMulticamPresentation>();
  const sourceValue = timeline.multicam_source;
  const source = sourceValue === null ? null : record(sourceValue);
  const syncMethod =
    source === null
      ? "unavailable"
      : nonemptyString(record(source.sync_method).kind);
  if (source !== null) array(source.angles);
  for (const clipValue of array(timeline.multicam_clips)) {
    const clip = record(clipValue);
    const id = nonemptyString(clip.clip_id);
    const switches = array(clip.switches);
    for (const switchValue of switches) {
      const multicamSwitch = record(switchValue);
      exactRange(multicamSwitch.source_range);
      nonemptyString(multicamSwitch.angle_id);
    }
    if (result.has(id)) throw new Error("duplicate multicam clip");
    result.set(id, {
      syncMethod,
      switchCount: switches.length,
      audioPolicy: nonemptyString(record(clip.audio_policy).kind),
    });
  }
  return result;
}

function effectMap(
  snapshot: JsonRecord,
  rootTimelineId: string,
): Map<string, readonly TimelineClipEffectPresentation[]> {
  try {
    const documents = array(record(snapshot.graph).documents).map(record);
    const matching = documents.filter((document) => {
      const scope = record(document.scope);
      return (
        scope.kind === "timeline" && scope.root_timeline_id === rootTimelineId
      );
    });
    if (matching.length !== 1) return new Map();
    const graphDocument = record(matching[0]?.document);
    if (
      graphDocument.format !== GRAPH_FORMAT ||
      graphDocument.format_revision !== GRAPH_FORMAT_REVISION
    ) {
      return new Map();
    }
    const payload = record(
      currentEnvelope(
        graphDocument.content,
        GRAPH_FORMAT,
        GRAPH_FORMAT_REVISION,
      ).payload,
    );
    const nodes = array(payload.nodes).map(record);
    const nodeTypeById = new Map<string, string>();
    const clipNodeById = new Map<string, string>();
    for (const node of nodes) {
      const id = nonemptyString(node.id);
      if (nodeTypeById.has(id)) throw new Error("duplicate graph node");
      nodeTypeById.set(id, nonemptyString(record(node.schema).node_type));
      const clipId = clipIdFromNode(node);
      if (clipId !== null) clipNodeById.set(clipId, id);
    }

    const outgoing = new Map<string, string[]>();
    for (const edgeValue of array(payload.edges)) {
      const edge = record(edgeValue);
      const source = nonemptyString(record(edge.source).node_id);
      const destination = nonemptyString(record(edge.destination).node_id);
      if (!nodeTypeById.has(source) || !nodeTypeById.has(destination)) {
        throw new Error("graph edge references an absent node");
      }
      const current = outgoing.get(source) ?? [];
      current.push(destination);
      outgoing.set(source, current);
    }

    const drivers = new Map<string, number>();
    for (const driverValue of array(payload.parameter_drivers ?? [])) {
      const target = record(record(driverValue).target);
      const nodeId = nonemptyString(target.node_id);
      drivers.set(nodeId, (drivers.get(nodeId) ?? 0) + 1);
    }

    const result = new Map<
      string,
      readonly TimelineClipEffectPresentation[]
    >();
    for (const [clipId, clipNodeId] of clipNodeById) {
      const visited = new Set<string>([clipNodeId]);
      const pending = [...(outgoing.get(clipNodeId) ?? [])];
      const clipEffects: TimelineClipEffectPresentation[] = [];
      while (pending.length > 0) {
        const nodeId = pending.shift();
        if (nodeId === undefined || visited.has(nodeId)) continue;
        visited.add(nodeId);
        const nodeType = nodeTypeById.get(nodeId);
        if (nodeType === undefined || nodeType.startsWith("superi.timeline.")) {
          continue;
        }
        clipEffects.push({
          nodeId,
          nodeType,
          label: effectLabel(nodeType),
          driverCount: drivers.get(nodeId) ?? 0,
        });
        pending.push(...(outgoing.get(nodeId) ?? []));
      }
      result.set(clipId, clipEffects);
    }
    return result;
  } catch {
    return new Map();
  }
}

function clipIdFromNode(node: JsonRecord): string | null {
  const nodeType = nonemptyString(record(node.schema).node_type);
  if (!/^superi\.timeline\.[^.]+\.clip$/.test(nodeType)) return null;
  for (const parameterValue of array(node.parameters)) {
    const parameter = record(parameterValue);
    if (parameter.name !== "object-id") continue;
    const payload = record(parameter.payload);
    const domain =
      "domain" in payload
        ? record(payload.domain)
        : payload.kind === "domain"
          ? record(payload.value)
          : null;
    if (domain === null || domain.kind !== "editorial_object_id") return null;
    const identity = record(domain.value);
    if (identity.kind !== "clip") return null;
    return nonemptyString(identity.id);
  }
  return null;
}

function effectLabel(nodeType: string): string {
  const segments = nodeType.split(".");
  const index = segments.indexOf("effects");
  const relevant = index >= 0 ? segments.slice(index + 1) : segments.slice(-2);
  return relevant
    .flatMap((segment) => segment.split(/[_-]/))
    .filter(Boolean)
    .map((word) => `${word.charAt(0).toUpperCase()}${word.slice(1)}`)
    .join(" ");
}

function automationMap(
  snapshot: JsonRecord,
): Map<string, TimelineClipAutomationPresentation> {
  const result = new Map<string, TimelineClipAutomationPresentation>();
  try {
    const automation = record(record(snapshot.audio).automation);
    if (automation.status !== "attached") return result;
    for (const laneValue of array(record(automation.state).lanes)) {
      try {
        const lane = record(laneValue);
        const target = record(lane.target);
        if (target.kind !== "clip_gain") continue;
        const clipId = nonemptyString(target.clip_id);
        const keyframes = array(lane.keyframes).map((keyframeValue) => {
          const keyframe = record(keyframeValue);
          const at = record(keyframe.at);
          return {
            sample: safeInteger(at.sample),
            sampleRate: positiveSafeInteger(at.sample_rate),
            value: finiteNumber(keyframe.value),
          };
        });
        const activePass =
          lane.active_pass === null ? null : automationPass(lane.active_pass);
        if (result.has(clipId)) throw new Error("duplicate automation lane");
        result.set(clipId, {
          sampleRate: positiveSafeInteger(lane.sample_rate),
          defaultGain: finiteNumber(lane.default_gain),
          mode: automationMode(record(lane.mode).kind),
          keyframes,
          activePass,
        });
      } catch {
        continue;
      }
    }
  } catch {
    return new Map();
  }
  return result;
}

function automationPass(value: unknown): TimelineClipAutomationPass {
  const pass = record(value);
  const start = record(pass.start);
  return {
    startSample: safeInteger(start.sample),
    sampleRate: positiveSafeInteger(start.sample_rate),
    currentValue: finiteNumber(pass.current_value),
    touchActive: boolean(pass.touch_active),
    latchActive: boolean(pass.latch_active),
  };
}

function automationMode(
  value: unknown,
): TimelineClipAutomationPresentation["mode"] {
  if (
    value === "read" ||
    value === "write" ||
    value === "touch" ||
    value === "latch"
  ) {
    return value;
  }
  throw new Error("unsupported automation mode");
}

function currentEnvelope(
  value: unknown,
  format: string,
  formatRevision: number,
): JsonRecord {
  const envelope = record(value);
  if (
    envelope.format !== format ||
    envelope.format_revision !== formatRevision ||
    envelope.primitive_schema_revision !== 1 ||
    typeof envelope.payload_sha256 !== "string" ||
    !/^[0-9a-f]{64}$/.test(envelope.payload_sha256)
  ) {
    throw new Error("unsupported canonical envelope");
  }
  record(envelope.payload);
  return envelope;
}

function exactPoint(value: unknown): TimelineExactPoint {
  const point = record(value);
  return { value: canonicalSigned(point.value), timebase: rate(point.timebase) };
}

function exactDuration(value: unknown): TimelineClipTimeMap["recordDuration"] {
  const duration = record(value);
  return {
    value: canonicalUnsignedString(duration.value),
    timebase: rate(duration.timebase),
  };
}

function exactRange(value: unknown): TimelineExactRange {
  const rangeValue = record(value);
  const start = exactPoint(rangeValue.start);
  const duration = exactDuration(rangeValue.duration);
  if (!sameRate(start.timebase, duration.timebase)) {
    throw new Error("range clocks differ");
  }
  return { start, duration };
}

function rate(value: unknown): TimelineRate {
  const rateValue = record(value);
  const numerator = positiveSafeInteger(rateValue.numerator);
  const denominator = positiveSafeInteger(rateValue.denominator);
  return { numerator, denominator };
}

function sameRate(left: TimelineRate, right: TimelineRate): boolean {
  return (
    left.numerator === right.numerator && left.denominator === right.denominator
  );
}

function formatExactRange(range: TimelineExactRange): string {
  return (
    `${range.start.value}+${range.duration.value} @ ` +
    `${range.start.timebase.numerator}/${range.start.timebase.denominator}`
  );
}

function relinkStatus(
  value: unknown,
): TimelineMediaSourcePresentation["relinkStatus"] {
  if (
    value === "online" ||
    value === "missing" ||
    value === "unverified" ||
    value === "fingerprint_mismatch"
  ) {
    return value;
  }
  throw new Error("unsupported relink status");
}

function record(value: unknown): JsonRecord {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error("expected object");
  }
  return value as JsonRecord;
}

function array(value: unknown): unknown[] {
  if (!Array.isArray(value)) throw new Error("expected array");
  return value;
}

function string(value: unknown): string {
  if (typeof value !== "string") throw new Error("expected string");
  return value;
}

function nonemptyString(value: unknown): string {
  const result = string(value);
  if (result.length === 0) throw new Error("expected nonempty string");
  return result;
}

function nullableString(value: unknown): string | null {
  return value === null ? null : string(value);
}

function boolean(value: unknown): boolean {
  if (typeof value !== "boolean") throw new Error("expected boolean");
  return value;
}

function finiteNumber(value: unknown): number {
  if (typeof value !== "number" || !Number.isFinite(value)) {
    throw new Error("expected finite number");
  }
  return value;
}

function safeInteger(value: unknown): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value)) {
    throw new Error("expected safe integer");
  }
  return value;
}

function positiveSafeInteger(value: unknown): number {
  const result = safeInteger(value);
  if (result <= 0) throw new Error("expected positive safe integer");
  return result;
}

function nonnegativeSafeInteger(value: unknown): number {
  const result = safeInteger(value);
  if (result < 0) throw new Error("expected nonnegative safe integer");
  return result;
}

function canonicalSigned(value: unknown): string {
  const result = string(value);
  if (!SIGNED_DECIMAL.test(result)) {
    throw new Error("expected canonical signed decimal");
  }
  return result;
}

function canonicalUnsignedString(value: unknown): string {
  const result = string(value);
  if (!UNSIGNED_DECIMAL.test(result)) {
    throw new Error("expected canonical unsigned decimal");
  }
  return result;
}

function canonicalUnsigned(value: unknown): bigint {
  return BigInt(canonicalUnsignedString(value));
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
