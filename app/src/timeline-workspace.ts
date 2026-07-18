import type { EditorCanonicalDocument } from "./api.ts";

const TIMELINE_FORMAT = "superi.timeline";
const TIMELINE_FORMAT_REVISION = 1;
const MIN_EMPTY_DURATION_SECONDS = 10;
const MAX_RULER_TICKS = 4_000;
const SAFE_SIGNED_DECIMAL = /^(?:0|-[1-9][0-9]*|[1-9][0-9]*)$/;
const SAFE_UNSIGNED_DECIMAL = /^(?:0|[1-9][0-9]*)$/;
const SHA256_HEX = /^[0-9a-f]{64}$/;

export interface TimelineRate {
  readonly numerator: number;
  readonly denominator: number;
}

export interface TimelineExactPoint {
  readonly value: string;
  readonly timebase: TimelineRate;
}

export interface TimelineExactDuration {
  readonly value: string;
  readonly timebase: TimelineRate;
}

export interface TimelineExactRange {
  readonly start: TimelineExactPoint;
  readonly duration: TimelineExactDuration;
}

export interface TimelineObjectReference {
  readonly kind: TimelineItemKind;
  readonly id: string;
}

export interface TimelineSourceReference {
  readonly kind: "media" | "timeline";
  readonly id: string;
}

export type TimelineItemKind =
  | "clip"
  | "gap"
  | "transition"
  | "generator"
  | "caption";

export type TimelineTrackKind = "video" | "audio" | "caption" | "data";

export interface TimelineCanvasItem {
  readonly kind: TimelineItemKind;
  readonly id: string;
  readonly name: string;
  readonly startSeconds: number;
  readonly endSeconds: number;
  readonly recordRange: TimelineExactRange;
  readonly source: TimelineSourceReference | null;
  readonly sourceRange: TimelineExactRange | null;
  readonly transition: {
    readonly from: TimelineObjectReference;
    readonly to: TimelineObjectReference;
  } | null;
  readonly selected: boolean;
  readonly group: readonly string[] | null;
  readonly link: readonly string[] | null;
}

export interface TimelineCanvasTrack {
  readonly id: string;
  readonly name: string;
  readonly kind: TimelineTrackKind;
  readonly targeted: boolean;
  readonly syncLocked: boolean;
  readonly items: readonly TimelineCanvasItem[];
}

export interface TimelineCanvasModel {
  readonly projectId: string;
  readonly projectName: string;
  readonly projectRevision: string;
  readonly documentSha256: string;
  readonly id: string;
  readonly name: string;
  readonly editRate: TimelineRate;
  readonly globalStart: TimelineExactPoint;
  readonly globalStartSeconds: number;
  readonly startSeconds: number;
  readonly endSeconds: number;
  readonly durationSeconds: number;
  readonly linkedSelectionEnabled: boolean;
  readonly snappingEnabled: boolean;
  readonly tracks: readonly TimelineCanvasTrack[];
}

export interface TimelineRulerTick {
  readonly seconds: number;
  readonly major: boolean;
  readonly label: string | null;
}

export interface TimelineRulerOptions {
  readonly startSeconds: number;
  readonly endSeconds: number;
  readonly visibleStartSeconds: number;
  readonly visibleEndSeconds: number;
  readonly pixelsPerSecond: number;
  readonly editRate: TimelineRate;
}

interface ParsedPoint {
  readonly exact: TimelineExactPoint;
  readonly units: bigint;
  readonly seconds: number;
}

interface ParsedDuration {
  readonly exact: TimelineExactDuration;
  readonly units: bigint;
  readonly seconds: number;
}

interface ParsedRange {
  readonly exact: TimelineExactRange;
  readonly startUnits: bigint;
  readonly durationUnits: bigint;
  readonly startSeconds: number;
  readonly endSeconds: number;
}

interface DirectItem extends Omit<TimelineCanvasItem, "kind"> {
  readonly kind: Exclude<TimelineItemKind, "transition">;
  readonly parsedRecordRange: ParsedRange;
}

interface PendingTransition {
  readonly kind: "transition";
  readonly id: string;
  readonly name: string;
  readonly from: TimelineObjectReference;
  readonly to: TimelineObjectReference;
  readonly fromOffset: ParsedDuration;
  readonly toOffset: ParsedDuration;
}

interface PendingTrack {
  readonly id: string;
  readonly name: string;
  readonly kind: TimelineTrackKind;
  readonly targeted: boolean;
  readonly syncLocked: boolean;
  readonly items: readonly (DirectItem | PendingTransition)[];
}

export class TimelineProjectionError extends Error {
  public readonly path: string;

  public constructor(path: string, message: string) {
    super(`${message} (${path})`);
    this.name = "TimelineProjectionError";
    this.path = path;
  }
}

export function projectTimelineDocument(
  document: EditorCanonicalDocument,
  rootTimelineId: string,
): TimelineCanvasModel {
  if (
    document.format !== TIMELINE_FORMAT ||
    document.format_revision !== TIMELINE_FORMAT_REVISION
  ) {
    throw projectionError(
      "document",
      `unsupported timeline document format ${document.format} ` +
        `revision ${document.format_revision}`,
    );
  }
  asString(document.resource, "document.resource");
  if (!Number.isSafeInteger(document.byte_length) || document.byte_length < 0) {
    throw projectionError("document.byte_length", "expected a nonnegative safe integer");
  }
  const documentSha256 = asSha256(document.sha256, "document.sha256");

  const envelope = asObject(document.content, "document.content");
  if (
    asString(envelope.format, "document.content.format") !== TIMELINE_FORMAT ||
    asInteger(
      envelope.format_revision,
      "document.content.format_revision",
    ) !== TIMELINE_FORMAT_REVISION ||
    asInteger(
      envelope.primitive_schema_revision,
      "document.content.primitive_schema_revision",
    ) !== 1
  ) {
    throw projectionError(
      "document.content",
      "unsupported embedded timeline document format",
    );
  }
  asSha256(envelope.payload_sha256, "document.content.payload_sha256");

  const payload = asObject(envelope.payload, "document.content.payload");
  const timelines = asArray(
    payload.timelines,
    "document.content.payload.timelines",
  );
  const matching = timelines.filter((value, index) => {
    const timeline = asObject(
      value,
      `document.content.payload.timelines[${index}]`,
    );
    return timeline.id === rootTimelineId;
  });
  if (matching.length !== 1) {
    throw projectionError(
      "document.content.payload.timelines",
      `root timeline ${rootTimelineId} must exist exactly once`,
    );
  }

  const timelinePath = `document.content.payload.timelines[root=${rootTimelineId}]`;
  const timeline = asObject(matching[0], timelinePath);
  const editRate = parseRate(timeline.edit_rate, `${timelinePath}.edit_rate`);
  const globalStart = parsePoint(
    timeline.global_start,
    `${timelinePath}.global_start`,
  );
  if (!sameRate(editRate, globalStart.exact.timebase)) {
    throw projectionError(
      `${timelinePath}.global_start`,
      "timeline global start must use the edit rate",
    );
  }
  const editState = asObject(timeline.edit_state, `${timelinePath}.edit_state`);
  const selected = new Set(
    asArray(editState.selected_objects, `${timelinePath}.edit_state.selected_objects`).map(
      (value, index) => {
        const reference = parseObjectReference(
          value,
          `${timelinePath}.edit_state.selected_objects[${index}]`,
        );
        return objectKey(reference);
      },
    ),
  );
  const linkRelations = parseRelations(
    editState.links,
    `${timelinePath}.edit_state.links`,
  );
  const groupRelations = parseRelations(
    editState.groups,
    `${timelinePath}.edit_state.groups`,
  );
  const links = indexRelations(linkRelations);
  const groups = indexRelations(groupRelations);
  const trackStates = parseTrackStates(
    editState.track_states,
    `${timelinePath}.edit_state.track_states`,
  );

  const pendingTracks = asArray(timeline.tracks, `${timelinePath}.tracks`).map(
    (value, trackIndex): PendingTrack => {
      const path = `${timelinePath}.tracks[${trackIndex}]`;
      const track = asObject(value, path);
      const id = asString(track.id, `${path}.id`);
      const state = trackStates.get(id);
      if (!state) {
        throw projectionError(
          `${timelinePath}.edit_state.track_states`,
          `track ${id} has no matching edit state`,
        );
      }
      const semantics = asObject(track.semantics, `${path}.semantics`);
      const kind = parseTrackKind(semantics.kind, `${path}.semantics.kind`);
      const items = asArray(track.items, `${path}.items`).map((item, itemIndex) =>
        parseItem(
          item,
          `${path}.items[${itemIndex}]`,
          globalStart.seconds,
          selected,
          groups,
          links,
        ),
      );
      const pending = {
        id,
        name: asString(track.name, `${path}.name`),
        kind,
        targeted: state.targeted,
        syncLocked: state.syncLocked,
        items,
      };
      validateTrackSequence(pending, path);
      return pending;
    },
  );
  const trackIds = new Set<string>();
  for (const track of pendingTracks) {
    if (trackIds.has(track.id)) {
      throw projectionError(`${timelinePath}.tracks`, `duplicate track identity ${track.id}`);
    }
    trackIds.add(track.id);
  }
  if (trackStates.size !== trackIds.size) {
    throw projectionError(
      `${timelinePath}.edit_state.track_states`,
      "track edit state must correspond exactly to the timeline tracks",
    );
  }

  const directItems = new Map<string, DirectItem>();
  for (const track of pendingTracks) {
    for (const item of track.items) {
      if (item.kind === "transition") continue;
      const key = objectKey(item);
      if (directItems.has(key)) {
        throw projectionError(timelinePath, `duplicate editorial identity ${key}`);
      }
      directItems.set(key, item);
    }
  }

  const tracks = pendingTracks.map((track): TimelineCanvasTrack => ({
    id: track.id,
    name: track.name,
    kind: track.kind,
    targeted: track.targeted,
    syncLocked: track.syncLocked,
    items: track.items.map((item) =>
      item.kind === "transition"
        ? resolveTransition(
            item,
            directItems,
            globalStart.seconds,
            selected,
            groups,
            links,
          )
        : omitParsedRange(item),
    ),
  }));

  const allItems = tracks.flatMap((track) => track.items);
  const itemKeys = new Set<string>();
  const clipIds = new Set<string>();
  for (const item of allItems) {
    const key = objectKey(item);
    if (itemKeys.has(key)) {
      throw projectionError(timelinePath, `duplicate editorial identity ${key}`);
    }
    itemKeys.add(key);
    if (item.kind === "clip") clipIds.add(item.id);
  }
  for (const key of selected) {
    if (!itemKeys.has(key)) {
      throw projectionError(
        `${timelinePath}.edit_state.selected_objects`,
        `selected editorial identity ${key} does not exist`,
      );
    }
  }
  validateRelationMembers(
    groupRelations,
    clipIds,
    `${timelinePath}.edit_state.groups`,
  );
  validateRelationMembers(
    linkRelations,
    clipIds,
    `${timelinePath}.edit_state.links`,
  );
  let startSeconds = globalStart.seconds;
  let endSeconds = globalStart.seconds;
  for (const item of allItems) {
    startSeconds = Math.min(startSeconds, item.startSeconds);
    endSeconds = Math.max(endSeconds, item.endSeconds);
  }
  if (endSeconds <= startSeconds) {
    endSeconds = startSeconds + MIN_EMPTY_DURATION_SECONDS;
  }

  return deepFreeze({
    projectId: asString(payload.project_id, "document.content.payload.project_id"),
    projectName: asString(payload.name, "document.content.payload.name"),
    projectRevision: asCanonicalUnsigned(
      payload.revision,
      "document.content.payload.revision",
    ),
    documentSha256,
    id: asString(timeline.id, `${timelinePath}.id`),
    name: asString(timeline.name, `${timelinePath}.name`),
    editRate,
    globalStart: globalStart.exact,
    globalStartSeconds: globalStart.seconds,
    startSeconds,
    endSeconds,
    durationSeconds: endSeconds - startSeconds,
    linkedSelectionEnabled: asBoolean(
      editState.linked_selection_enabled,
      `${timelinePath}.edit_state.linked_selection_enabled`,
    ),
    snappingEnabled: asBoolean(
      timeline.snapping_enabled,
      `${timelinePath}.snapping_enabled`,
    ),
    tracks,
  });
}

export function buildTimelineRulerTicks(
  options: TimelineRulerOptions,
): readonly TimelineRulerTick[] {
  const pixelsPerSecond = finitePositive(
    options.pixelsPerSecond,
    "pixelsPerSecond",
  );
  const frameSeconds = timelineFrameDuration(options.editRate);
  const requestedMajor = 88 / pixelsPerSecond;
  const majorStep =
    requestedMajor <= frameSeconds * 8
      ? niceFrameStep(requestedMajor, frameSeconds)
      : niceDecimalStep(requestedMajor);
  const majorFrames = majorStep / frameSeconds;
  const minorDivisions =
    requestedMajor <= frameSeconds * 8
      ? frameAlignedMinorDivisions(majorFrames)
      : 4;
  const minorStep = majorStep / minorDivisions;
  const visibleStart = clampNumber(
    options.visibleStartSeconds,
    options.startSeconds,
    options.endSeconds,
  );
  const visibleEnd = clampNumber(
    options.visibleEndSeconds,
    visibleStart,
    options.endSeconds,
  );
  const firstIndex = Math.ceil((visibleStart - Number.EPSILON) / minorStep);
  const lastIndex = Math.floor((visibleEnd + Number.EPSILON) / minorStep);
  if (lastIndex - firstIndex + 1 > MAX_RULER_TICKS) {
    throw projectionError("ruler", "visible scale would produce too many ruler ticks");
  }

  const ticks: TimelineRulerTick[] = [];
  for (let index = firstIndex; index <= lastIndex; index += 1) {
    const seconds = normalizeFloat(index * minorStep);
    const majorRatio = seconds / majorStep;
    const major = nearlyInteger(majorRatio);
    ticks.push({
      seconds,
      major,
      label: major ? formatTimelineTime(seconds, options.editRate) : null,
    });
  }
  return Object.freeze(ticks.map((tick) => Object.freeze(tick)));
}

export function formatTimelineTime(
  seconds: number,
  editRate: TimelineRate,
): string {
  if (!Number.isFinite(seconds)) {
    throw projectionError("time", "timeline time must be finite");
  }
  const frameSeconds = timelineFrameDuration(editRate);
  const signedFrames = Math.round(seconds / frameSeconds);
  const negative = signedFrames < 0;
  const frames = Math.abs(signedFrames);
  const nominalFramesPerSecond = Math.max(
    1,
    Math.round(editRate.numerator / editRate.denominator),
  );
  const frame = frames % nominalFramesPerSecond;
  const totalSeconds = Math.floor(frames / nominalFramesPerSecond);
  const second = totalSeconds % 60;
  const totalMinutes = Math.floor(totalSeconds / 60);
  const minute = totalMinutes % 60;
  const hour = Math.floor(totalMinutes / 60);
  const label = `${pad(hour, 2)}:${pad(minute, 2)}:${pad(second, 2)}:${pad(frame, 2)}`;
  return negative ? `-${label}` : label;
}

export function timelineFrameDuration(editRate: TimelineRate): number {
  const rate = parseRate(editRate, "editRate");
  return rate.denominator / rate.numerator;
}

export function snapTimelineTime(
  seconds: number,
  editRate: TimelineRate,
  anchorSeconds = 0,
): number {
  const frameSeconds = timelineFrameDuration(editRate);
  return normalizeFloat(
    anchorSeconds + Math.round((seconds - anchorSeconds) / frameSeconds) * frameSeconds,
  );
}

export function clampTimelineRange(
  first: number,
  second: number,
  minimum: number,
  maximum: number,
): { readonly inPoint: number; readonly outPoint: number } {
  if (![first, second, minimum, maximum].every(Number.isFinite) || maximum < minimum) {
    throw projectionError("range", "timeline range bounds must be finite and ordered");
  }
  const inPoint = clampNumber(Math.min(first, second), minimum, maximum);
  const outPoint = clampNumber(Math.max(first, second), inPoint, maximum);
  return Object.freeze({ inPoint, outPoint });
}

export function timelineItemsInWindow(
  items: readonly TimelineCanvasItem[],
  visibleStartSeconds: number,
  visibleEndSeconds: number,
  overscanSeconds = 0,
): readonly TimelineCanvasItem[] {
  if (
    ![visibleStartSeconds, visibleEndSeconds, overscanSeconds].every(Number.isFinite) ||
    visibleEndSeconds < visibleStartSeconds ||
    overscanSeconds < 0
  ) {
    throw projectionError("visibleWindow", "timeline item window must be finite and ordered");
  }
  const minimum = visibleStartSeconds - overscanSeconds;
  const maximum = visibleEndSeconds + overscanSeconds;
  return Object.freeze(
    items.filter(
      (item) => item.endSeconds >= minimum && item.startSeconds <= maximum,
    ),
  );
}

export function clampNumber(value: number, minimum: number, maximum: number): number {
  return Math.min(maximum, Math.max(minimum, value));
}

function parseItem(
  value: unknown,
  path: string,
  globalStartSeconds: number,
  selected: ReadonlySet<string>,
  groups: ReadonlyMap<string, readonly string[]>,
  links: ReadonlyMap<string, readonly string[]>,
): DirectItem | PendingTransition {
  const item = asObject(value, path);
  const kind = parseItemKind(item.kind, `${path}.kind`);
  const id = asString(item.id, `${path}.id`);
  const name = asString(item.name, `${path}.name`);
  if (kind === "transition") {
    return {
      kind,
      id,
      name,
      from: parseObjectReference(item.from, `${path}.from`),
      to: parseObjectReference(item.to, `${path}.to`),
      fromOffset: parseDuration(item.from_offset, `${path}.from_offset`),
      toOffset: parseDuration(item.to_offset, `${path}.to_offset`),
    };
  }

  const recordRange = parseRange(item.record_range, `${path}.record_range`);
  const reference = { kind, id } satisfies TimelineObjectReference;
  let source: TimelineSourceReference | null = null;
  let sourceRange: TimelineExactRange | null = null;
  if (kind === "clip") {
    source = parseSource(item.source, `${path}.source`);
    sourceRange = parseRange(item.source_range, `${path}.source_range`).exact;
  }
  const startSeconds = offsetDisplaySeconds(
    globalStartSeconds,
    recordRange.startSeconds,
    `${path}.record_range.start`,
  );
  const endSeconds = offsetDisplaySeconds(
    globalStartSeconds,
    recordRange.endSeconds,
    `${path}.record_range.end`,
  );
  return {
    kind,
    id,
    name,
    startSeconds,
    endSeconds,
    recordRange: recordRange.exact,
    source,
    sourceRange,
    transition: null,
    selected: selected.has(objectKey(reference)),
    group: groups.get(id) ?? null,
    link: links.get(id) ?? null,
    parsedRecordRange: recordRange,
  };
}

function resolveTransition(
  value: PendingTransition,
  directItems: ReadonlyMap<string, DirectItem>,
  globalStartSeconds: number,
  selected: ReadonlySet<string>,
  groups: ReadonlyMap<string, readonly string[]>,
  links: ReadonlyMap<string, readonly string[]>,
): TimelineCanvasItem {
  const from = directItems.get(objectKey(value.from));
  const to = directItems.get(objectKey(value.to));
  if (!from || !to) {
    throw projectionError(
      `transition.${value.id}`,
      "transition references must resolve to timed editorial objects",
    );
  }
  const timebase = from.recordRange.duration.timebase;
  if (
    !sameRate(timebase, value.fromOffset.exact.timebase) ||
    !sameRate(timebase, value.toOffset.exact.timebase)
  ) {
    throw projectionError(
      `transition.${value.id}`,
      "transition offsets must use the outgoing record timebase",
    );
  }
  const startUnits =
    from.parsedRecordRange.startUnits +
    from.parsedRecordRange.durationUnits -
    value.fromOffset.units;
  const durationUnits = value.fromOffset.units + value.toOffset.units;
  const startValue = startUnits.toString();
  const durationValue = durationUnits.toString();
  const startSeconds = exactUnitsToSeconds(
    startUnits,
    timebase,
    `transition.${value.id}.record_range.start`,
  );
  const durationSeconds = exactUnitsToSeconds(
    durationUnits,
    timebase,
    `transition.${value.id}.record_range.duration`,
  );
  const reference = { kind: value.kind, id: value.id } satisfies TimelineObjectReference;
  const displayStartSeconds = offsetDisplaySeconds(
    globalStartSeconds,
    startSeconds,
    `transition.${value.id}.record_range.start`,
  );
  const displayEndSeconds = offsetDisplaySeconds(
    displayStartSeconds,
    durationSeconds,
    `transition.${value.id}.record_range.end`,
  );
  return {
    kind: value.kind,
    id: value.id,
    name: value.name,
    startSeconds: displayStartSeconds,
    endSeconds: displayEndSeconds,
    recordRange: {
      start: { value: startValue, timebase },
      duration: { value: durationValue, timebase },
    },
    source: null,
    sourceRange: null,
    transition: { from: value.from, to: value.to },
    selected: selected.has(objectKey(reference)),
    group: groups.get(value.id) ?? null,
    link: links.get(value.id) ?? null,
  };
}

function validateTrackSequence(track: PendingTrack, path: string): void {
  let recordRate: TimelineRate | null = null;
  let priorEndUnits = 0n;
  for (const [index, item] of track.items.entries()) {
    if (item.kind === "transition") {
      const previous = track.items[index - 1];
      const next = track.items[index + 1];
      if (
        !previous ||
        previous.kind === "transition" ||
        !next ||
        next.kind === "transition"
      ) {
        throw projectionError(
          `${path}.items[${index}]`,
          "transition must sit between two timed editorial objects",
        );
      }
      if (
        objectKey(item.from) !== objectKey(previous) ||
        objectKey(item.to) !== objectKey(next)
      ) {
        throw projectionError(
          `${path}.items[${index}]`,
          "transition endpoints must match adjacent editorial objects",
        );
      }
      if (
        !sameRate(item.fromOffset.exact.timebase, previous.recordRange.duration.timebase) ||
        !sameRate(item.toOffset.exact.timebase, next.recordRange.duration.timebase) ||
        item.fromOffset.units > previous.parsedRecordRange.durationUnits ||
        item.toOffset.units > next.parsedRecordRange.durationUnits ||
        (item.fromOffset.units === 0n && item.toOffset.units === 0n)
      ) {
        throw projectionError(
          `${path}.items[${index}]`,
          "transition offsets must fit adjacent objects on their record clock",
        );
      }
      continue;
    }

    const range = item.parsedRecordRange;
    if (recordRate === null) {
      recordRate = range.exact.start.timebase;
    } else if (!sameRate(recordRate, range.exact.start.timebase)) {
      throw projectionError(path, "track items must use one exact record clock");
    }
    if (range.startUnits !== priorEndUnits) {
      throw projectionError(
        `${path}.items[${index}].record_range`,
        "timed track items must be contiguous from timeline zero",
      );
    }
    priorEndUnits = range.startUnits + range.durationUnits;

    const incoming = track.items[index - 1];
    const outgoing = track.items[index + 1];
    const consumed =
      (incoming?.kind === "transition" ? incoming.toOffset.units : 0n) +
      (outgoing?.kind === "transition" ? outgoing.fromOffset.units : 0n);
    if (consumed > range.durationUnits) {
      throw projectionError(
        `${path}.items[${index}]`,
        "transition overlap exceeds the adjacent item duration",
      );
    }
  }
}

function omitParsedRange(item: DirectItem): TimelineCanvasItem {
  const { parsedRecordRange: _, ...value } = item;
  return value;
}

function parsePoint(value: unknown, path: string): ParsedPoint {
  const point = asObject(value, path);
  const raw = asCanonicalSigned(point.value, `${path}.value`);
  const timebase = parseRate(point.timebase, `${path}.timebase`);
  const units = BigInt(raw);
  return {
    exact: { value: raw, timebase },
    units,
    seconds: exactUnitsToSeconds(units, timebase, path),
  };
}

function parseDuration(value: unknown, path: string): ParsedDuration {
  const duration = asObject(value, path);
  const raw = asCanonicalUnsigned(duration.value, `${path}.value`);
  const timebase = parseRate(duration.timebase, `${path}.timebase`);
  const units = BigInt(raw);
  return {
    exact: { value: raw, timebase },
    units,
    seconds: exactUnitsToSeconds(units, timebase, path),
  };
}

function parseRange(value: unknown, path: string): ParsedRange {
  const range = asObject(value, path);
  const start = parsePoint(range.start, `${path}.start`);
  const duration = parseDuration(range.duration, `${path}.duration`);
  if (!sameRate(start.exact.timebase, duration.exact.timebase)) {
    throw projectionError(path, "range start and duration must use one timebase");
  }
  const endUnits = start.units + duration.units;
  const endSeconds = exactUnitsToSeconds(
    endUnits,
    start.exact.timebase,
    `${path}.end`,
  );
  return {
    exact: { start: start.exact, duration: duration.exact },
    startUnits: start.units,
    durationUnits: duration.units,
    startSeconds: start.seconds,
    endSeconds,
  };
}

function parseRate(value: unknown, path: string): TimelineRate {
  const rate = asObject(value, path);
  const numerator = asInteger(rate.numerator, `${path}.numerator`);
  const denominator = asInteger(rate.denominator, `${path}.denominator`);
  if (numerator <= 0 || denominator <= 0) {
    throw projectionError(path, "timebase terms must be positive");
  }
  if (greatestCommonDivisor(numerator, denominator) !== 1) {
    throw projectionError(path, "timebase terms must already be reduced");
  }
  return Object.freeze({ numerator, denominator });
}

function parseSource(value: unknown, path: string): TimelineSourceReference {
  const source = asObject(value, path);
  const kind = asString(source.kind, `${path}.kind`);
  if (kind !== "media" && kind !== "timeline") {
    throw projectionError(`${path}.kind`, `unsupported clip source ${kind}`);
  }
  return { kind, id: asString(source.id, `${path}.id`) };
}

function parseObjectReference(value: unknown, path: string): TimelineObjectReference {
  const reference = asObject(value, path);
  return {
    kind: parseItemKind(reference.kind, `${path}.kind`),
    id: asString(reference.id, `${path}.id`),
  };
}

function parseItemKind(value: unknown, path: string): TimelineItemKind {
  const kind = asString(value, path);
  if (
    kind !== "clip" &&
    kind !== "gap" &&
    kind !== "transition" &&
    kind !== "generator" &&
    kind !== "caption"
  ) {
    throw projectionError(path, `unsupported timeline item kind ${kind}`);
  }
  return kind;
}

function parseTrackKind(value: unknown, path: string): TimelineTrackKind {
  const kind = asString(value, path);
  if (kind !== "video" && kind !== "audio" && kind !== "caption" && kind !== "data") {
    throw projectionError(path, `unsupported timeline track kind ${kind}`);
  }
  return kind;
}

function parseTrackStates(
  value: unknown,
  path: string,
): ReadonlyMap<string, { readonly targeted: boolean; readonly syncLocked: boolean }> {
  const result = new Map<
    string,
    { readonly targeted: boolean; readonly syncLocked: boolean }
  >();
  for (const [index, entry] of asArray(value, path).entries()) {
    const statePath = `${path}[${index}]`;
    const state = asObject(entry, statePath);
    const id = asString(state.track_id, `${statePath}.track_id`);
    if (result.has(id)) {
      throw projectionError(statePath, `duplicate track edit state ${id}`);
    }
    result.set(id, {
      targeted: asBoolean(state.targeted, `${statePath}.targeted`),
      syncLocked: asBoolean(state.sync_locked, `${statePath}.sync_locked`),
    });
  }
  return result;
}

function parseRelations(
  value: unknown,
  path: string,
): readonly (readonly string[])[] {
  const members = new Set<string>();
  return asArray(value, path).map((relation, relationIndex) => {
    const relationPath = `${path}[${relationIndex}]`;
    const ids = asArray(relation, relationPath).map((id, memberIndex) =>
      asString(id, `${relationPath}[${memberIndex}]`),
    );
    if (ids.length < 2) {
      throw projectionError(relationPath, "timeline relation must contain at least two clips");
    }
    for (const id of ids) {
      if (members.has(id)) {
        throw projectionError(relationPath, `clip ${id} appears in multiple relation components`);
      }
      members.add(id);
    }
    return Object.freeze(ids.slice());
  });
}

function indexRelations(
  relations: readonly (readonly string[])[],
): ReadonlyMap<string, readonly string[]> {
  const result = new Map<string, readonly string[]>();
  for (const relation of relations) {
    for (const id of relation) result.set(id, relation);
  }
  return result;
}

function validateRelationMembers(
  relations: readonly (readonly string[])[],
  clipIds: ReadonlySet<string>,
  path: string,
): void {
  for (const [relationIndex, relation] of relations.entries()) {
    for (const id of relation) {
      if (!clipIds.has(id)) {
        throw projectionError(
          `${path}[${relationIndex}]`,
          `related clip ${id} does not exist`,
        );
      }
    }
  }
}

function objectKey(value: TimelineObjectReference): string {
  return `${value.kind}:${value.id}`;
}

function exactUnitsToSeconds(
  units: bigint,
  timebase: TimelineRate,
  path: string,
): number {
  const numericUnits = Number(units);
  if (!Number.isSafeInteger(numericUnits)) {
    throw projectionError(path, "timeline coordinate exceeds the safe display range");
  }
  const numerator = BigInt(timebase.numerator);
  const whole = units / numerator;
  const remainder = units % numerator;
  const seconds =
    Number(whole) * timebase.denominator +
    (Number(remainder) * timebase.denominator) / timebase.numerator;
  if (!Number.isFinite(seconds) || Math.abs(seconds) > Number.MAX_SAFE_INTEGER) {
    throw projectionError(path, "timeline coordinate exceeds the safe display range");
  }
  return seconds;
}

function offsetDisplaySeconds(base: number, offset: number, path: string): number {
  const result = base + offset;
  if (!Number.isFinite(result) || Math.abs(result) > Number.MAX_SAFE_INTEGER) {
    throw projectionError(path, "timeline coordinate exceeds the safe display range");
  }
  return result;
}

function asObject(value: unknown, path: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw projectionError(path, "expected an object");
  }
  return value as Record<string, unknown>;
}

function asArray(value: unknown, path: string): readonly unknown[] {
  if (!Array.isArray(value)) {
    throw projectionError(path, "expected an array");
  }
  return value;
}

function asString(value: unknown, path: string): string {
  if (typeof value !== "string" || value.length === 0) {
    throw projectionError(path, "expected a nonempty string");
  }
  return value;
}

function asBoolean(value: unknown, path: string): boolean {
  if (typeof value !== "boolean") {
    throw projectionError(path, "expected a boolean");
  }
  return value;
}

function asInteger(value: unknown, path: string): number {
  if (typeof value !== "number" || !Number.isSafeInteger(value)) {
    throw projectionError(path, "expected a safe integer");
  }
  return value;
}

function asCanonicalSigned(value: unknown, path: string): string {
  if (typeof value !== "string" || !SAFE_SIGNED_DECIMAL.test(value)) {
    throw projectionError(path, "expected a canonical signed decimal string");
  }
  return value;
}

function asCanonicalUnsigned(value: unknown, path: string): string {
  if (typeof value !== "string" || !SAFE_UNSIGNED_DECIMAL.test(value)) {
    throw projectionError(path, "expected a canonical unsigned decimal string");
  }
  return value;
}

function asSha256(value: unknown, path: string): string {
  if (typeof value !== "string" || !SHA256_HEX.test(value)) {
    throw projectionError(path, "expected a lowercase SHA-256 digest");
  }
  return value;
}

function finitePositive(value: number, path: string): number {
  if (!Number.isFinite(value) || value <= 0) {
    throw projectionError(path, "expected a finite positive number");
  }
  return value;
}

function niceDecimalStep(requested: number): number {
  const exponent = Math.floor(Math.log10(requested));
  const magnitude = 10 ** exponent;
  const normalized = requested / magnitude;
  const multiplier = normalized <= 1 ? 1 : normalized <= 2 ? 2 : normalized <= 5 ? 5 : 10;
  return multiplier * magnitude;
}

function niceFrameStep(requested: number, frameSeconds: number): number {
  const frames = Math.max(1, requested / frameSeconds);
  return niceDecimalStep(frames) * frameSeconds;
}

function frameAlignedMinorDivisions(majorFrames: number): number {
  const frames = Math.max(1, Math.round(majorFrames));
  if (frames % 5 === 0) return 5;
  if (frames % 4 === 0) return 4;
  if (frames % 2 === 0) return 2;
  return 1;
}

function sameRate(left: TimelineRate, right: TimelineRate): boolean {
  return left.numerator === right.numerator && left.denominator === right.denominator;
}

function greatestCommonDivisor(left: number, right: number): number {
  let a = left;
  let b = right;
  while (b !== 0) {
    const remainder = a % b;
    a = b;
    b = remainder;
  }
  return a;
}

function nearlyInteger(value: number): boolean {
  return Math.abs(value - Math.round(value)) < 1e-7;
}

function normalizeFloat(value: number): number {
  return Number(value.toFixed(12));
}

function pad(value: number, width: number): string {
  return String(value).padStart(width, "0");
}

function projectionError(path: string, message: string): TimelineProjectionError {
  return new TimelineProjectionError(path, message);
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
