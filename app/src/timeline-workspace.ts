import type { EditorCanonicalDocument } from "./api.ts";

const TIMELINE_FORMAT = "superi.timeline";
const TIMELINE_FORMAT_REVISION = 1;
const MIN_EMPTY_DURATION_SECONDS = 10;
const MAX_RULER_TICKS = 4_000;
const SAFE_SIGNED_DECIMAL = /^(?:0|-[1-9][0-9]*|[1-9][0-9]*)$/;
const SAFE_UNSIGNED_DECIMAL = /^(?:0|[1-9][0-9]*)$/;
const SHA256_HEX = /^[0-9a-f]{64}$/;
const TIMELINE_SELECTION_IDENTITY_PREFIX = "superi.timeline.object/";
const MAX_TIMELINE_SELECTION_IDENTITY_LENGTH = 4_096;

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

export type TimelineSnapTargetKind =
  | "timeline_start"
  | "playhead"
  | "item_start"
  | "item_end"
  | "marker_start"
  | "marker_end";

export interface TimelineSnapRules {
  readonly timelineStart: boolean;
  readonly playhead: boolean;
  readonly itemStart: boolean;
  readonly itemEnd: boolean;
  readonly markerStart: boolean;
  readonly markerEnd: boolean;
}

const TIMELINE_SNAP_RULE_KEYS = Object.freeze([
  "timelineStart",
  "playhead",
  "itemStart",
  "itemEnd",
  "markerStart",
  "markerEnd",
] as const satisfies readonly (keyof TimelineSnapRules)[]);

export const TIMELINE_DEFAULT_SNAP_RULES: Readonly<TimelineSnapRules> =
  Object.freeze({
    timelineStart: true,
    playhead: true,
    itemStart: true,
    itemEnd: true,
    markerStart: true,
    markerEnd: true,
  });

export interface TimelineSnapTarget {
  readonly kind: Exclude<TimelineSnapTargetKind, "playhead">;
  readonly id: string;
  readonly label: string;
  readonly editorialObject: TimelineObjectReference | null;
  readonly time: TimelineExactPoint;
  readonly timeSeconds: number;
}

interface TimelinePlayheadSnapTarget {
  readonly kind: "playhead";
  readonly id: "playhead";
  readonly label: "Playhead";
  readonly editorialObject: null;
  readonly time: TimelineExactPoint;
  readonly timeSeconds: number;
}

export interface TimelineSnapRequest {
  readonly atSeconds: number;
  readonly toleranceFrames: number;
  readonly playheadSeconds: number | null;
  readonly rules: TimelineSnapRules;
  readonly sessionEnabled: boolean;
}

export interface TimelineSnapMatch {
  readonly target: TimelineSnapTarget | TimelinePlayheadSnapTarget;
  readonly timeSeconds: number;
  readonly distanceFrames: number;
}

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
  readonly snapTargets: readonly TimelineSnapTarget[];
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

export interface TimelineSelectionTarget {
  readonly key: string;
  readonly trackId: string;
  readonly trackIndex: number;
  readonly itemIndex: number;
  readonly item: TimelineCanvasItem;
}

export type TimelineSelectionDirection =
  | "left"
  | "right"
  | "up"
  | "down"
  | "home"
  | "end";

export interface TimelineRectangle {
  readonly left: number;
  readonly top: number;
  readonly right: number;
  readonly bottom: number;
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
  readonly recordRate: TimelineRate;
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
      const recordRate = parseTrackRecordRate(
        semantics,
        kind,
        `${path}.semantics`,
      );
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
        recordRate,
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

  const snapTargets = buildTimelineSnapTargets({
    timeline,
    timelinePath,
    editRate,
    globalStartSeconds: globalStart.seconds,
    tracks: pendingTracks,
    directItems,
  });

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
  for (const target of snapTargets) {
    startSeconds = Math.min(startSeconds, target.timeSeconds);
    endSeconds = Math.max(endSeconds, target.timeSeconds);
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
    snapTargets,
    tracks,
  });
}

function buildTimelineSnapTargets({
  timeline,
  timelinePath,
  editRate,
  globalStartSeconds,
  tracks,
  directItems,
}: {
  readonly timeline: Record<string, unknown>;
  readonly timelinePath: string;
  readonly editRate: TimelineRate;
  readonly globalStartSeconds: number;
  readonly tracks: readonly PendingTrack[];
  readonly directItems: ReadonlyMap<string, DirectItem>;
}): readonly TimelineSnapTarget[] {
  const targets: TimelineSnapTarget[] = [];
  const addTarget = (
    kind: TimelineSnapTarget["kind"],
    id: string,
    label: string,
    editorialObject: TimelineObjectReference | null,
    units: bigint,
    sourceRate: TimelineRate,
    path: string,
  ) => {
    const editUnits = rescaleExactUnits(units, sourceRate, editRate);
    if (editUnits === null) return;
    const offsetSeconds = exactUnitsToSeconds(editUnits, editRate, path);
    targets.push({
      kind,
      id,
      label,
      editorialObject,
      time: { value: editUnits.toString(), timebase: editRate },
      timeSeconds: offsetDisplaySeconds(globalStartSeconds, offsetSeconds, path),
    });
  };

  addTarget(
    "timeline_start",
    asString(timeline.id, `${timelinePath}.id`),
    "Timeline start",
    null,
    0n,
    editRate,
    `${timelinePath}.snap_targets.timeline_start`,
  );

  for (const item of directItems.values()) {
    const range = item.parsedRecordRange;
    addTarget(
      "item_start",
      item.id,
      `${item.name} start`,
      { kind: item.kind, id: item.id },
      range.startUnits,
      range.exact.start.timebase,
      `${timelinePath}.snap_targets.item_start.${item.id}`,
    );
    addTarget(
      "item_end",
      item.id,
      `${item.name} end`,
      { kind: item.kind, id: item.id },
      range.startUnits + range.durationUnits,
      range.exact.start.timebase,
      `${timelinePath}.snap_targets.item_end.${item.id}`,
    );
  }

  const tracksById = new Map(tracks.map((track) => [track.id, track]));
  const markerIds = new Set<string>();
  for (const [index, value] of asArray(
    timeline.markers,
    `${timelinePath}.markers`,
  ).entries()) {
    const path = `${timelinePath}.markers[${index}]`;
    const marker = asObject(value, path);
    const id = asString(marker.id, `${path}.id`);
    if (markerIds.has(id)) {
      throw projectionError(path, `duplicate marker identity ${id}`);
    }
    markerIds.add(id);
    const label = asNullableString(marker.label, `${path}.label`) ?? id;
    asNullableString(marker.flag, `${path}.flag`);
    asNullableString(marker.note, `${path}.note`);
    asArray(marker.metadata, `${path}.metadata`);
    const markedRange = parseRange(marker.marked_range, `${path}.marked_range`);
    if (markedRange.startUnits < 0n) {
      throw projectionError(
        `${path}.marked_range.start`,
        "marker range must not start before its owner zero",
      );
    }

    const owner = asObject(marker.owner, `${path}.owner`);
    const ownerKind = asString(owner.kind, `${path}.owner.kind`);
    let ownerRate: TimelineRate;
    let ownerStartUnits = 0n;
    let ownerDurationUnits: bigint | null = null;
    if (ownerKind === "timeline") {
      ownerRate = editRate;
    } else if (ownerKind === "track") {
      const trackId = asString(owner.id, `${path}.owner.id`);
      const track = tracksById.get(trackId);
      if (!track) {
        throw projectionError(
          `${path}.owner.id`,
          `marker owner track ${trackId} does not exist`,
        );
      }
      ownerRate = track.recordRate;
    } else if (ownerKind === "object") {
      const object = parseObjectReference(owner.id, `${path}.owner.id`);
      const item = directItems.get(objectKey(object));
      if (!item) {
        throw projectionError(
          `${path}.owner.id`,
          `marker owner object ${objectKey(object)} has no timed record range`,
        );
      }
      ownerRate = item.parsedRecordRange.exact.start.timebase;
      ownerStartUnits = item.parsedRecordRange.startUnits;
      ownerDurationUnits = item.parsedRecordRange.durationUnits;
    } else {
      throw projectionError(
        `${path}.owner.kind`,
        `unsupported marker owner kind ${ownerKind}`,
      );
    }

    if (!sameRate(markedRange.exact.start.timebase, ownerRate)) {
      throw projectionError(
        `${path}.marked_range`,
        "marker range must use its owner's exact record clock",
      );
    }
    if (
      ownerDurationUnits !== null &&
      markedRange.startUnits + markedRange.durationUnits > ownerDurationUnits
    ) {
      continue;
    }
    const startUnits = ownerStartUnits + markedRange.startUnits;
    const endUnits = startUnits + markedRange.durationUnits;
    addTarget(
      "marker_start",
      id,
      `${label} start`,
      null,
      startUnits,
      ownerRate,
      `${path}.snap_target.start`,
    );
    addTarget(
      "marker_end",
      id,
      `${label} end`,
      null,
      endUnits,
      ownerRate,
      `${path}.snap_target.end`,
    );
  }

  targets.sort((left, right) => {
    const leftUnits = BigInt(left.time.value);
    const rightUnits = BigInt(right.time.value);
    if (leftUnits !== rightUnits) return leftUnits < rightUnits ? -1 : 1;
    const kind = snapTargetKindOrder(left.kind) - snapTargetKindOrder(right.kind);
    if (kind !== 0) return kind;
    return compareSnapTargetIdentity(left, right);
  });
  return deepFreeze(targets);
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

export function resolveTimelineSnap(
  model: TimelineCanvasModel,
  request: TimelineSnapRequest,
): TimelineSnapMatch | null {
  if (!request.sessionEnabled || !model.snappingEnabled) return null;
  if (
    !Number.isSafeInteger(request.toleranceFrames) ||
    request.toleranceFrames < 0
  ) {
    throw projectionError(
      "snap.toleranceFrames",
      "snap tolerance must be a nonnegative frame count",
    );
  }
  validateSnapRules(request.rules);
  const atUnits = displaySecondsToEditUnits(
    request.atSeconds,
    model,
    "snap.atSeconds",
  );
  const tolerance = BigInt(request.toleranceFrames);
  const candidates: Array<{
    readonly target: TimelineSnapMatch["target"];
    readonly units: bigint;
    readonly distance: bigint;
  }> = [];

  for (const target of model.snapTargets) {
    if (!snapRuleEnabled(target.kind, request.rules)) continue;
    const units = BigInt(target.time.value);
    const distance = absoluteBigInt(units - atUnits);
    if (distance <= tolerance) candidates.push({ target, units, distance });
  }

  if (request.playheadSeconds !== null && request.rules.playhead) {
    const units = tryDisplaySecondsToEditUnits(request.playheadSeconds, model);
    if (units !== null) {
      const distance = absoluteBigInt(units - atUnits);
      if (distance <= tolerance) {
        candidates.push({
          target: deepFreeze({
            kind: "playhead",
            id: "playhead",
            label: "Playhead",
            editorialObject: null,
            time: { value: units.toString(), timebase: model.editRate },
            timeSeconds: offsetDisplaySeconds(
              model.globalStartSeconds,
              exactUnitsToSeconds(
                units,
                model.editRate,
                "snap.playheadSeconds",
              ),
              "snap.playheadSeconds",
            ),
          }),
          units,
          distance,
        });
      }
    }
  }

  candidates.sort((left, right) => {
    if (left.distance !== right.distance) {
      return left.distance < right.distance ? -1 : 1;
    }
    const kind =
      snapTargetKindOrder(left.target.kind) -
      snapTargetKindOrder(right.target.kind);
    if (kind !== 0) return kind;
    const identity = compareSnapTargetIdentity(left.target, right.target);
    if (identity !== 0) return identity;
    if (left.units === right.units) return 0;
    return left.units < right.units ? -1 : 1;
  });
  const best = candidates[0];
  if (!best) return null;
  return deepFreeze({
    target: best.target,
    timeSeconds: best.target.timeSeconds,
    distanceFrames: Number(best.distance),
  });
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

export function timelineSelectionTargets(
  model: TimelineCanvasModel,
): readonly TimelineSelectionTarget[] {
  const targets: TimelineSelectionTarget[] = [];
  const tracks = model.tracks.slice().reverse();
  for (const [trackIndex, track] of tracks.entries()) {
    for (const [itemIndex, item] of track.items.entries()) {
      targets.push(
        Object.freeze({
          key: objectKey(item),
          trackId: track.id,
          trackIndex,
          itemIndex,
          item,
        }),
      );
    }
  }
  return Object.freeze(targets);
}

export function timelineObjectKey(value: TimelineObjectReference): string {
  return objectKey(value);
}

export function expandTimelineSelection(
  model: TimelineCanvasModel,
  requestedKeys: readonly string[],
  direct = false,
): readonly string[] {
  const targets = timelineSelectionTargets(model);
  const targetsByKey = new Map(targets.map((target) => [target.key, target]));
  const clipKeys = new Map(
    targets
      .filter((target) => target.item.kind === "clip")
      .map((target) => [target.item.id, target.key]),
  );
  const expanded = new Set(
    requestedKeys.filter((key) => targetsByKey.has(key)),
  );
  if (direct) {
    return Object.freeze(
      targets.filter((target) => expanded.has(target.key)).map((target) => target.key),
    );
  }

  const pending = [...expanded]
    .map((key) => targetsByKey.get(key))
    .filter(
      (target): target is TimelineSelectionTarget =>
        target !== undefined && target.item.kind === "clip",
    )
    .map((target) => target.item.id);
  const visited = new Set<string>();
  while (pending.length > 0) {
    const clipId = pending.pop();
    if (clipId === undefined || visited.has(clipId)) continue;
    visited.add(clipId);
    const key = clipKeys.get(clipId);
    if (key === undefined) continue;
    expanded.add(key);
    const target = targetsByKey.get(key);
    if (target === undefined) continue;
    const related = [
      ...(target.item.group ?? []),
      ...(model.linkedSelectionEnabled ? target.item.link ?? [] : []),
    ];
    for (const member of related) {
      if (!visited.has(member) && clipKeys.has(member)) pending.push(member);
    }
  }

  return Object.freeze(
    targets.filter((target) => expanded.has(target.key)).map((target) => target.key),
  );
}

export function timelineSelectionRange(
  model: TimelineCanvasModel,
  anchorKey: string,
  focusKey: string,
  direct = false,
): readonly string[] {
  const targets = timelineSelectionTargets(model);
  const anchorIndex = targets.findIndex((target) => target.key === anchorKey);
  const focusIndex = targets.findIndex((target) => target.key === focusKey);
  if (focusIndex === -1) return Object.freeze([]);
  if (anchorIndex === -1) {
    return expandTimelineSelection(model, [focusKey], direct);
  }
  const start = Math.min(anchorIndex, focusIndex);
  const end = Math.max(anchorIndex, focusIndex);
  const directKeys = targets.slice(start, end + 1).map((target) => target.key);
  const directKeySet = new Set(directKeys);
  const expanded = expandTimelineSelection(model, directKeys, direct);
  return Object.freeze([
    ...directKeys,
    ...expanded.filter((key) => !directKeySet.has(key)),
  ]);
}

export function timelineSelectionNeighbor(
  model: TimelineCanvasModel,
  currentKey: string,
  direction: TimelineSelectionDirection,
): string | null {
  const targets = timelineSelectionTargets(model);
  const current = targets.find((target) => target.key === currentKey);
  if (current === undefined) return null;
  const trackTargets = targets.filter(
    (target) => target.trackIndex === current.trackIndex,
  );
  const localIndex = trackTargets.findIndex((target) => target.key === currentKey);
  if (direction === "left") return trackTargets[localIndex - 1]?.key ?? null;
  if (direction === "right") return trackTargets[localIndex + 1]?.key ?? null;
  if (direction === "home") return trackTargets[0]?.key ?? null;
  if (direction === "end") return trackTargets.at(-1)?.key ?? null;

  const step = direction === "up" ? -1 : 1;
  const trackCount = Math.max(0, ...targets.map((target) => target.trackIndex)) + 1;
  const currentCenter = (current.item.startSeconds + current.item.endSeconds) / 2;
  for (
    let trackIndex = current.trackIndex + step;
    trackIndex >= 0 && trackIndex < trackCount;
    trackIndex += step
  ) {
    const candidates = targets.filter((target) => target.trackIndex === trackIndex);
    if (candidates.length === 0) continue;
    return candidates.reduce((nearest, candidate) => {
      const nearestCenter =
        (nearest.item.startSeconds + nearest.item.endSeconds) / 2;
      const candidateCenter =
        (candidate.item.startSeconds + candidate.item.endSeconds) / 2;
      return Math.abs(candidateCenter - currentCenter) <
        Math.abs(nearestCenter - currentCenter)
        ? candidate
        : nearest;
    }).key;
  }
  return null;
}

export function timelineSelectionIdentity(
  timelineId: string,
  object: TimelineObjectReference,
): string {
  if (timelineId.length === 0 || object.id.length === 0) {
    throw projectionError("selection.identity", "timeline selection identity is incomplete");
  }
  const identity =
    TIMELINE_SELECTION_IDENTITY_PREFIX +
    `${encodeURIComponent(timelineId)}/${object.kind}/${encodeURIComponent(object.id)}`;
  if (identity.length > MAX_TIMELINE_SELECTION_IDENTITY_LENGTH) {
    throw projectionError("selection.identity", "timeline selection identity is too long");
  }
  return identity;
}

export function parseTimelineSelectionIdentity(identity: string): {
  readonly timelineId: string;
  readonly object: TimelineObjectReference;
} | null {
  if (
    identity.length > MAX_TIMELINE_SELECTION_IDENTITY_LENGTH ||
    !identity.startsWith(TIMELINE_SELECTION_IDENTITY_PREFIX)
  ) {
    return null;
  }
  const encoded = identity.slice(TIMELINE_SELECTION_IDENTITY_PREFIX.length).split("/");
  if (encoded.length !== 3) return null;
  const [timelineId, kind, id] = encoded;
  if (kind === undefined || !isItemKind(kind)) return null;
  try {
    const decodedTimelineId = decodeURIComponent(timelineId ?? "");
    const decodedId = decodeURIComponent(id ?? "");
    if (decodedTimelineId.length === 0 || decodedId.length === 0) return null;
    return Object.freeze({
      timelineId: decodedTimelineId,
      object: Object.freeze({ kind, id: decodedId }),
    });
  } catch {
    return null;
  }
}

export function timelineRectanglesIntersect(
  left: TimelineRectangle,
  right: TimelineRectangle,
): boolean {
  const values = [
    left.left,
    left.top,
    left.right,
    left.bottom,
    right.left,
    right.top,
    right.right,
    right.bottom,
  ];
  if (!values.every(Number.isFinite)) return false;
  const leftBox = normalizeRectangle(left);
  const rightBox = normalizeRectangle(right);
  return !(
    leftBox.right < rightBox.left ||
    leftBox.left > rightBox.right ||
    leftBox.bottom < rightBox.top ||
    leftBox.top > rightBox.bottom
  );
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
    if (!sameRate(track.recordRate, range.exact.start.timebase)) {
      throw projectionError(
        `${path}.items[${index}].record_range`,
        "timed item must use its track's exact record clock",
      );
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
  if (!isItemKind(kind)) {
    throw projectionError(path, `unsupported timeline item kind ${kind}`);
  }
  return kind;
}

function isItemKind(value: string): value is TimelineItemKind {
  return (
    value === "clip" ||
    value === "gap" ||
    value === "transition" ||
    value === "generator" ||
    value === "caption"
  );
}

function parseTrackKind(value: unknown, path: string): TimelineTrackKind {
  const kind = asString(value, path);
  if (kind !== "video" && kind !== "audio" && kind !== "caption" && kind !== "data") {
    throw projectionError(path, `unsupported timeline track kind ${kind}`);
  }
  return kind;
}

function parseTrackRecordRate(
  semantics: Record<string, unknown>,
  kind: TimelineTrackKind,
  path: string,
): TimelineRate {
  if (kind === "video") {
    return parseRate(semantics.frame_rate, `${path}.frame_rate`);
  }
  if (kind === "audio") {
    const sampleRate = asInteger(semantics.sample_rate, `${path}.sample_rate`);
    if (sampleRate <= 0) {
      throw projectionError(`${path}.sample_rate`, "sample rate must be positive");
    }
    return Object.freeze({ numerator: sampleRate, denominator: 1 });
  }
  return parseRate(semantics.timebase, `${path}.timebase`);
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

function rescaleExactUnits(
  units: bigint,
  sourceRate: TimelineRate,
  targetRate: TimelineRate,
): bigint | null {
  const numerator =
    units * BigInt(sourceRate.denominator) * BigInt(targetRate.numerator);
  const denominator =
    BigInt(sourceRate.numerator) * BigInt(targetRate.denominator);
  if (numerator % denominator !== 0n) return null;
  return numerator / denominator;
}

function displaySecondsToEditUnits(
  seconds: number,
  model: TimelineCanvasModel,
  path: string,
): bigint {
  if (!Number.isFinite(seconds)) {
    throw projectionError(path, "snap coordinate must be finite");
  }
  const units =
    ((seconds - model.globalStartSeconds) * model.editRate.numerator) /
    model.editRate.denominator;
  if (!Number.isSafeInteger(Math.round(units)) || !nearlyInteger(units)) {
    throw projectionError(path, "snap coordinate must use the timeline edit clock");
  }
  return BigInt(Math.round(units));
}

function tryDisplaySecondsToEditUnits(
  seconds: number,
  model: TimelineCanvasModel,
): bigint | null {
  try {
    return displaySecondsToEditUnits(seconds, model, "snap.playheadSeconds");
  } catch {
    return null;
  }
}

function validateSnapRules(rules: TimelineSnapRules): void {
  if (typeof rules !== "object" || rules === null) {
    throw projectionError("snap.rules", "snap rules must be an object");
  }
  for (const name of TIMELINE_SNAP_RULE_KEYS) {
    const enabled = rules[name];
    if (typeof enabled !== "boolean") {
      throw projectionError(`snap.rules.${name}`, "snap rule must be boolean");
    }
  }
}

function snapRuleEnabled(
  kind: TimelineSnapTargetKind,
  rules: TimelineSnapRules,
): boolean {
  switch (kind) {
    case "timeline_start":
      return rules.timelineStart;
    case "playhead":
      return rules.playhead;
    case "item_start":
      return rules.itemStart;
    case "item_end":
      return rules.itemEnd;
    case "marker_start":
      return rules.markerStart;
    case "marker_end":
      return rules.markerEnd;
  }
}

function snapTargetKindOrder(kind: TimelineSnapTargetKind): number {
  switch (kind) {
    case "timeline_start":
      return 0;
    case "playhead":
      return 1;
    case "item_start":
      return 2;
    case "item_end":
      return 3;
    case "marker_start":
      return 4;
    case "marker_end":
      return 5;
  }
}

function absoluteBigInt(value: bigint): bigint {
  return value < 0n ? -value : value;
}

function compareStrings(left: string, right: string): number {
  if (left === right) return 0;
  return left < right ? -1 : 1;
}

function compareSnapTargetIdentity(
  left: TimelineSnapMatch["target"],
  right: TimelineSnapMatch["target"],
): number {
  if (left.editorialObject && right.editorialObject) {
    const objectKind =
      timelineItemKindOrder(left.editorialObject.kind) -
      timelineItemKindOrder(right.editorialObject.kind);
    if (objectKind !== 0) return objectKind;
    return compareStrings(left.editorialObject.id, right.editorialObject.id);
  }
  return compareStrings(left.id, right.id);
}

function timelineItemKindOrder(kind: TimelineItemKind): number {
  switch (kind) {
    case "clip":
      return 0;
    case "gap":
      return 1;
    case "transition":
      return 2;
    case "generator":
      return 3;
    case "caption":
      return 4;
  }
}

function normalizeRectangle(value: TimelineRectangle): TimelineRectangle {
  return {
    left: Math.min(value.left, value.right),
    top: Math.min(value.top, value.bottom),
    right: Math.max(value.left, value.right),
    bottom: Math.max(value.top, value.bottom),
  };
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

function asNullableString(value: unknown, path: string): string | null {
  return value === null ? null : asString(value, path);
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
