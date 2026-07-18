import type {
  EditorialObjectId,
  ExactTime,
  ExactTimeRange,
  TimelineEditOperation,
} from "./api.ts";
import {
  formatTimelineTime,
  type TimelineCanvasItem,
  type TimelineCanvasModel,
  type TimelineCanvasTrack,
  type TimelineItemKind,
  type TimelineRate,
} from "./timeline-workspace.ts";

export type TimelineEditingTool =
  | "ripple"
  | "roll"
  | "slip"
  | "slide"
  | "razor"
  | "trim"
  | "extend";

export type TimelineEditingSide = "start" | "end";
export type TimelineExtendMode = "ripple" | "roll";
export type TimelineEditableItemKind = Exclude<TimelineItemKind, "transition">;

export type TimelineIdentityAllocator = (
  kind: TimelineEditableItemKind,
) => string;

export interface TimelineEditingToolDefinition {
  readonly id: TimelineEditingTool;
  readonly label: string;
  readonly description: string;
}

export interface TimelineEditPlan {
  readonly label: string;
  readonly operations: readonly TimelineEditOperation[];
  readonly previewSeconds: number | null;
  readonly affectedItemIds: readonly string[];
}

export interface CompileTimelineGestureOptions {
  readonly model: TimelineCanvasModel;
  readonly tool: TimelineEditingTool;
  readonly trackId: string;
  readonly itemId: string;
  readonly side: TimelineEditingSide;
  readonly toSeconds: number;
  readonly extendMode?: TimelineExtendMode;
  readonly allocateId?: TimelineIdentityAllocator;
}

export interface CompileRippleDeleteOptions {
  readonly model: TimelineCanvasModel;
  readonly trackId: string;
  readonly startSeconds: number;
  readonly endSeconds: number;
  readonly allocateId?: TimelineIdentityAllocator;
}

export interface CompileGapInsertOptions {
  readonly model: TimelineCanvasModel;
  readonly trackId: string;
  readonly atSeconds: number;
  readonly frameCount: number;
  readonly allocateId?: TimelineIdentityAllocator;
}

export interface CompileGapCloseOptions {
  readonly model: TimelineCanvasModel;
  readonly trackId: string;
  readonly gapId: string;
  readonly allocateId?: TimelineIdentityAllocator;
}

interface ExactTrackRange {
  readonly start: bigint;
  readonly duration: bigint;
  readonly end: bigint;
  readonly timebase: TimelineRate;
}

interface PreparedExtract {
  readonly track: TimelineCanvasTrack;
  readonly range: ExactTrackRange;
  readonly fragmentKinds: readonly TimelineEditableItemKind[];
}

interface PreparedInsert {
  readonly track: TimelineCanvasTrack;
  readonly at: bigint;
  readonly duration: bigint;
  readonly fragmentKinds: readonly TimelineEditableItemKind[];
}

const GENERATED_ID = /^[a-z_]+:[0-9a-f]{32}$/;
const DISPLAY_QUANTIZATION_EPSILON = 1e-6;

export const timelineEditingTools: readonly TimelineEditingToolDefinition[] =
  deepFreeze([
    {
      id: "ripple",
      label: "Ripple",
      description: "Move one edge and keep later synchronized material together.",
    },
    {
      id: "roll",
      label: "Roll",
      description: "Move a shared cut while the combined record span stays fixed.",
    },
    {
      id: "slip",
      label: "Slip",
      description: "Move source content inside a clip without moving its record span.",
    },
    {
      id: "slide",
      label: "Slide",
      description: "Move a clip between two neighbors while preserving total timing.",
    },
    {
      id: "razor",
      label: "Razor",
      description: "Split a timed object at one exact frame-aligned edit point.",
    },
    {
      id: "trim",
      label: "Trim",
      description: "Move one edge in place and create or consume an adjacent gap.",
    },
    {
      id: "extend",
      label: "Extend",
      description: "Extend an edge with explicit ripple or roll behavior.",
    },
  ]);

export class TimelineEditingError extends Error {
  public constructor(message: string) {
    super(message);
    this.name = "TimelineEditingError";
  }
}

export function compileTimelineGesture(
  options: CompileTimelineGestureOptions,
): TimelineEditPlan {
  const track = requireTrack(options.model, options.trackId);
  const item = requireItem(track, options.itemId);
  if (item.kind === "transition") {
    throw editingError("Transitions are changed through their adjacent timed objects.");
  }
  const to = displayPointOnTrack(options.model, track, options.toSeconds);
  const allocateId = options.allocateId ?? allocateTimelineIdentity;
  const target = objectReference(item);
  const side = options.side;
  let operation: TimelineEditOperation;
  let affectedItemIds: readonly string[] = [item.id];

  switch (options.tool) {
    case "ripple": {
      const delta = validateRipple(item, track, side, to);
      const pivot = itemEnd(item, track);
      affectedItemIds = rippleAffectedItemIds(
        options.model,
        track,
        item,
        pivot,
        delta,
      );
      operation = {
        operation: "ripple",
        timeline_id: options.model.id,
        track_id: track.id,
        target_id: target,
        side,
        to: exactTime(to, track.timebase),
        sync_adjustments: compileRippleSyncAdjustments(
          options.model,
          track,
          pivot,
          delta,
          allocateId,
        ),
      };
      break;
    }
    case "roll": {
      const pair = validateRoll(track, item, side, to);
      operation = {
        operation: "roll",
        timeline_id: options.model.id,
        track_id: track.id,
        left_id: objectReference(pair.left),
        right_id: objectReference(pair.right),
        to: exactTime(to, track.timebase),
      };
      affectedItemIds = [pair.left.id, pair.right.id];
      break;
    }
    case "slip": {
      if (item.kind !== "clip" || item.sourceRange === null) {
        throw editingError("Slip requires a source-bearing clip.");
      }
      const delta = to - itemStart(item, track);
      if (delta === 0n) {
        throw editingError("Slip must move the source range.");
      }
      const sourceStart = signedUnits(
        item.sourceRange.start.value,
        "clip source start",
      );
      const sourceDelta = rescaleExact(
        delta,
        track.timebase,
        item.sourceRange.start.timebase,
        "Slip movement",
      );
      operation = {
        operation: "slip",
        timeline_id: options.model.id,
        track_id: track.id,
        clip_id: item.id,
        source_start: exactTime(
          sourceStart + sourceDelta,
          item.sourceRange.start.timebase,
        ),
      };
      break;
    }
    case "slide": {
      if (item.kind !== "clip") {
        throw editingError("Slide requires a source-bearing clip.");
      }
      const neighbors = validateSlide(track, item, to);
      operation = {
        operation: "slide",
        timeline_id: options.model.id,
        track_id: track.id,
        clip_id: item.id,
        to: exactTime(to, track.timebase),
      };
      affectedItemIds = [neighbors.left.id, item.id, neighbors.right.id];
      break;
    }
    case "razor": {
      const range = itemRange(item, track);
      if (to <= range.start || to >= range.end) {
        throw editingError("Razor must fall strictly inside the target object.");
      }
      const fragment = allocateObjectReference(item.kind, allocateId);
      operation = {
        operation: "razor",
        timeline_id: options.model.id,
        track_id: track.id,
        target_id: target,
        at: exactTime(to, track.timebase),
        fragment_id: fragment,
      };
      affectedItemIds = [item.id, fragment.id];
      break;
    }
    case "trim": {
      const trim = validateTrimAndAllocateGap(
        track,
        item,
        side,
        to,
        allocateId,
      );
      operation = {
        operation: "trim",
        timeline_id: options.model.id,
        track_id: track.id,
        target_id: target,
        side,
        to: exactTime(to, track.timebase),
        gap_id: trim.gapId,
      };
      affectedItemIds = trim.affectedItemIds;
      break;
    }
    case "extend": {
      const mode = options.extendMode ?? "ripple";
      let syncAdjustments: Extract<
        TimelineEditOperation,
        { readonly operation: "extend" }
      >["sync_adjustments"];
      if (mode === "ripple") {
        const delta = validateRipple(item, track, side, to);
        const pivot = itemEnd(item, track);
        affectedItemIds = rippleAffectedItemIds(
          options.model,
          track,
          item,
          pivot,
          delta,
        );
        syncAdjustments = compileRippleSyncAdjustments(
          options.model,
          track,
          pivot,
          delta,
          allocateId,
        );
      } else {
        const pair = validateRoll(track, item, side, to);
        affectedItemIds = pair.ids;
        syncAdjustments = [];
      }
      operation = {
        operation: "extend",
        timeline_id: options.model.id,
        track_id: track.id,
        target_id: target,
        side,
        to: exactTime(to, track.timebase),
        mode,
        sync_adjustments: syncAdjustments,
      };
      break;
    }
  }

  const label = `${toolLabel(options.tool)} ${side} to ${formatTimelineTime(
    options.toSeconds,
    options.model.editRate,
  )}`;
  return freezePlan({
    label,
    operations: [operation],
    previewSeconds: options.toSeconds,
    affectedItemIds,
  });
}

export function compileRippleDelete(
  options: CompileRippleDeleteOptions,
): TimelineEditPlan {
  if (
    !Number.isFinite(options.startSeconds) ||
    !Number.isFinite(options.endSeconds) ||
    options.startSeconds >= options.endSeconds
  ) {
    throw editingError("Ripple delete requires a nonempty ordered range.");
  }
  const primary = requireTrack(options.model, options.trackId);
  const startOnPrimary = displayPointOnTrack(
    options.model,
    primary,
    options.startSeconds,
  );
  const endOnPrimary = displayPointOnTrack(
    options.model,
    primary,
    options.endSeconds,
  );
  const prepared = prepareExtractAcrossSync(
    options.model,
    primary,
    exactTrackRange(
      startOnPrimary,
      endOnPrimary - startOnPrimary,
      primary.timebase,
      "Ripple delete",
    ),
  );
  const operations = materializeExtracts(
    options.model.id,
    prepared,
    options.allocateId ?? allocateTimelineIdentity,
  );
  return freezePlan({
    label: `Ripple delete ${formatTimelineTime(
      options.startSeconds,
      options.model.editRate,
    )} to ${formatTimelineTime(options.endSeconds, options.model.editRate)}`,
    operations,
    previewSeconds: options.startSeconds,
    affectedItemIds: affectedIdsForPrepared(prepared),
  });
}

export function compileGapInsert(options: CompileGapInsertOptions): TimelineEditPlan {
  if (!Number.isSafeInteger(options.frameCount) || options.frameCount <= 0) {
    throw editingError("Gap duration must be a positive whole frame count.");
  }
  const primary = requireTrack(options.model, options.trackId);
  const at = displayPointOnTrack(options.model, primary, options.atSeconds);
  const primaryDuration = rescaleExact(
    BigInt(options.frameCount),
    options.model.editRate,
    primary.timebase,
    "Gap duration",
  );
  const prepared = prepareInsertAcrossSync(
    options.model,
    primary,
    at,
    primaryDuration,
  );
  const operations = materializeGapInserts(
    options.model.id,
    prepared,
    options.allocateId ?? allocateTimelineIdentity,
  );
  return freezePlan({
    label: `Insert ${options.frameCount} frame gap at ${formatTimelineTime(
      options.atSeconds,
      options.model.editRate,
    )}`,
    operations,
    previewSeconds: options.atSeconds,
    affectedItemIds: affectedIdsForPrepared(prepared),
  });
}

export function compileGapClose(options: CompileGapCloseOptions): TimelineEditPlan {
  const primary = requireTrack(options.model, options.trackId);
  const gap = requireItem(primary, options.gapId);
  if (gap.kind !== "gap") {
    throw editingError("Close gap requires a gap target.");
  }
  const prepared = prepareExtractAcrossSync(
    options.model,
    primary,
    itemRange(gap, primary),
  );
  const operations = materializeExtracts(
    options.model.id,
    prepared,
    options.allocateId ?? allocateTimelineIdentity,
  );
  return freezePlan({
    label: `Close gap ${gap.name}`,
    operations,
    previewSeconds: gap.startSeconds,
    affectedItemIds: affectedIdsForPrepared(prepared),
  });
}

function validateRipple(
  item: TimelineCanvasItem,
  track: TimelineCanvasTrack,
  side: TimelineEditingSide,
  to: bigint,
): bigint {
  const range = itemRange(item, track);
  const boundary = side === "start" ? range.start : range.end;
  if (to === boundary) {
    throw editingError("Ripple must move the selected edge.");
  }
  if (side === "start" && to >= range.end) {
    throw editingError("Ripple start must remain before the object end.");
  }
  if (side === "end" && to <= range.start) {
    throw editingError("Ripple end must remain after the object start.");
  }
  return side === "end" ? to - range.end : range.start - to;
}

function validateRoll(
  track: TimelineCanvasTrack,
  item: TimelineCanvasItem,
  side: TimelineEditingSide,
  to: bigint,
): {
  readonly left: TimelineCanvasItem;
  readonly right: TimelineCanvasItem;
  readonly ids: readonly string[];
} {
  const items = timedItems(track);
  const index = items.findIndex((candidate) => candidate.id === item.id);
  const left = side === "start" ? items[index - 1] : items[index];
  const right = side === "start" ? items[index] : items[index + 1];
  if (!left || !right) {
    throw editingError("Roll requires an adjacent timed object on the selected edge.");
  }
  const leftRange = itemRange(left, track);
  const rightRange = itemRange(right, track);
  if (leftRange.end !== rightRange.start) {
    throw editingError("Roll targets must share one exact cut.");
  }
  if (to <= leftRange.start || to >= rightRange.end) {
    throw editingError("Roll must leave both adjacent objects with nonzero duration.");
  }
  if (to === rightRange.start) {
    throw editingError("Roll must move the shared cut.");
  }
  return { left, right, ids: [left.id, right.id] };
}

function validateSlide(
  track: TimelineCanvasTrack,
  item: TimelineCanvasItem,
  to: bigint,
): { readonly left: TimelineCanvasItem; readonly right: TimelineCanvasItem } {
  const items = timedItems(track);
  const index = items.findIndex((candidate) => candidate.id === item.id);
  const left = items[index - 1];
  const right = items[index + 1];
  if (!left || !right || left.kind !== "clip" || right.kind !== "clip") {
    throw editingError("Slide requires one adjacent source-bearing clip on each side.");
  }
  const center = itemRange(item, track);
  const leftRange = itemRange(left, track);
  const rightRange = itemRange(right, track);
  const newEnd = to + center.duration;
  if (to <= leftRange.start || newEnd >= rightRange.end) {
    throw editingError("Slide must leave both adjacent clips with nonzero duration.");
  }
  if (to === center.start) {
    throw editingError("Slide must move the center clip.");
  }
  return { left, right };
}

function validateTrimAndAllocateGap(
  track: TimelineCanvasTrack,
  item: TimelineCanvasItem,
  side: TimelineEditingSide,
  to: bigint,
  allocateId: TimelineIdentityAllocator,
): {
  readonly gapId: string | null;
  readonly affectedItemIds: readonly string[];
} {
  const items = timedItems(track);
  const index = items.findIndex((candidate) => candidate.id === item.id);
  const range = itemRange(item, track);
  const boundary = side === "start" ? range.start : range.end;
  if (to === boundary) {
    throw editingError("Trim must move the selected edge.");
  }
  if (side === "start" && to >= range.end) {
    throw editingError("Trim start must remain before the object end.");
  }
  if (side === "end" && to <= range.start) {
    throw editingError("Trim end must remain after the object start.");
  }

  const neighbor = side === "start" ? items[index - 1] : items[index + 1];
  const inward = side === "start" ? to > boundary : to < boundary;
  if (inward) {
    if (neighbor?.kind === "gap") {
      return { gapId: null, affectedItemIds: [item.id, neighbor.id] };
    }
    const gapId = allocateTypedId("gap", allocateId);
    return { gapId, affectedItemIds: [item.id, gapId] };
  }
  if (!neighbor || neighbor.kind !== "gap") {
    throw editingError("Outward trim may consume only an adjacent gap.");
  }
  const gapRange = itemRange(neighbor, track);
  if (
    (side === "start" && to < gapRange.start) ||
    (side === "end" && to > gapRange.end)
  ) {
    throw editingError("Outward trim cannot move beyond the adjacent gap.");
  }
  return { gapId: null, affectedItemIds: [item.id, neighbor.id] };
}

function rippleAffectedItemIds(
  model: TimelineCanvasModel,
  primary: TimelineCanvasTrack,
  target: TimelineCanvasItem,
  pivot: bigint,
  delta: bigint,
): readonly string[] {
  const ids = new Set<string>();
  const primaryItems = timedItems(primary);
  const targetIndex = primaryItems.findIndex((item) => item.id === target.id);
  for (const item of primaryItems.slice(targetIndex)) ids.add(item.id);

  const extending = delta > 0n;
  const magnitude = extending ? delta : -delta;
  for (const track of model.tracks) {
    if (track.id === primary.id || !track.syncLocked) continue;
    const companionPivot = rescaleExact(
      pivot,
      primary.timebase,
      track.timebase,
      `Ripple preview pivot on ${track.name}`,
    );
    const companionMagnitude = rescaleExact(
      magnitude,
      primary.timebase,
      track.timebase,
      `Ripple preview duration on ${track.name}`,
    );
    const affectedStart = extending
      ? companionPivot
      : companionPivot - companionMagnitude;
    for (const item of timedItems(track)) {
      if (itemRange(item, track).end > affectedStart) ids.add(item.id);
    }
  }
  return [...ids];
}

function compileRippleSyncAdjustments(
  model: TimelineCanvasModel,
  primary: TimelineCanvasTrack,
  pivot: bigint,
  delta: bigint,
  allocateId: TimelineIdentityAllocator,
): Extract<
  TimelineEditOperation,
  { readonly operation: "ripple" }
>["sync_adjustments"] {
  const extending = delta > 0n;
  const magnitude = delta < 0n ? -delta : delta;
  const prepared = model.tracks
    .filter((track) => track.id !== primary.id && track.syncLocked)
    .map((track) => {
      requireUnlockedTrack(track);
      const companionPivot = rescaleExact(
        pivot,
        primary.timebase,
        track.timebase,
        `Ripple pivot on ${track.name}`,
      );
      const companionMagnitude = rescaleExact(
        magnitude,
        primary.timebase,
        track.timebase,
        `Ripple duration on ${track.name}`,
      );
      if (extending) {
        validateInsertPoint(track, companionPivot, "Synchronized ripple");
        return {
          track,
          fragmentKinds: fragmentKindsAtPoint(track, companionPivot),
        };
      }
      const start = companionPivot - companionMagnitude;
      const range = exactTrackRange(
        start,
        companionMagnitude,
        track.timebase,
        "Synchronized ripple",
      );
      validateExtractRange(track, range, "Synchronized ripple");
      return { track, fragmentKinds: fragmentKindsInRange(track, range) };
    });

  return prepared.map(({ track, fragmentKinds }) => ({
    track_id: track.id,
    gap_id: allocateTypedId("gap", allocateId),
    fragment_ids: fragmentKinds.map((kind) =>
      allocateObjectReference(kind, allocateId),
    ),
  }));
}

function prepareExtractAcrossSync(
  model: TimelineCanvasModel,
  primary: TimelineCanvasTrack,
  primaryRange: ExactTrackRange,
): readonly PreparedExtract[] {
  return affectedTracks(model, primary).map((track) => {
    const start = rescaleExact(
      primaryRange.start,
      primary.timebase,
      track.timebase,
      `Edit start on ${track.name}`,
    );
    const duration = rescaleExact(
      primaryRange.duration,
      primary.timebase,
      track.timebase,
      `Edit duration on ${track.name}`,
    );
    const range = exactTrackRange(
      start,
      duration,
      track.timebase,
      "Extract range",
    );
    validateExtractRange(track, range, "Extract");
    return {
      track,
      range,
      fragmentKinds: fragmentKindsInRange(track, range),
    };
  });
}

function prepareInsertAcrossSync(
  model: TimelineCanvasModel,
  primary: TimelineCanvasTrack,
  primaryAt: bigint,
  primaryDuration: bigint,
): readonly PreparedInsert[] {
  return affectedTracks(model, primary).map((track) => {
    const at = rescaleExact(
      primaryAt,
      primary.timebase,
      track.timebase,
      `Gap point on ${track.name}`,
    );
    const duration = rescaleExact(
      primaryDuration,
      primary.timebase,
      track.timebase,
      `Gap duration on ${track.name}`,
    );
    if (duration <= 0n) {
      throw editingError("Gap duration must remain positive on every synchronized track.");
    }
    validateInsertPoint(track, at, "Insert gap");
    return {
      track,
      at,
      duration,
      fragmentKinds: fragmentKindsAtPoint(track, at),
    };
  });
}

function materializeExtracts(
  timelineId: string,
  prepared: readonly PreparedExtract[],
  allocateId: TimelineIdentityAllocator,
): readonly TimelineEditOperation[] {
  return prepared.map(({ track, range, fragmentKinds }) => ({
    operation: "extract" as const,
    timeline_id: timelineId,
    track_id: track.id,
    range: exactRange(range),
    fragment_ids: fragmentKinds.map((kind) =>
      allocateObjectReference(kind, allocateId),
    ),
  }));
}

function materializeGapInserts(
  timelineId: string,
  prepared: readonly PreparedInsert[],
  allocateId: TimelineIdentityAllocator,
): readonly TimelineEditOperation[] {
  return prepared.map(({ track, at, duration, fragmentKinds }) => ({
    operation: "insert" as const,
    timeline_id: timelineId,
    track_id: track.id,
    at: exactTime(at, track.timebase),
    material: {
      kind: "gap" as const,
      id: allocateTypedId("gap", allocateId),
      name: "Inserted gap",
      record_range: exactRange(
        exactTrackRange(at, duration, track.timebase, "Inserted gap"),
      ),
    },
    fragment_ids: fragmentKinds.map((kind) =>
      allocateObjectReference(kind, allocateId),
    ),
  }));
}

function affectedTracks(
  model: TimelineCanvasModel,
  primary: TimelineCanvasTrack,
): readonly TimelineCanvasTrack[] {
  const tracks = model.tracks.filter(
    (track) => track.id === primary.id || track.syncLocked,
  );
  for (const track of tracks) requireUnlockedTrack(track);
  return tracks;
}

function affectedIdsForPrepared(
  prepared: readonly (PreparedExtract | PreparedInsert)[],
): readonly string[] {
  const ids = new Set<string>();
  for (const value of prepared) {
    for (const item of timedItems(value.track)) {
      const range = itemRange(item, value.track);
      const affectedStart = "range" in value ? value.range.start : value.at;
      if (range.end > affectedStart) ids.add(item.id);
    }
  }
  return [...ids];
}

function fragmentKindsAtPoint(
  track: TimelineCanvasTrack,
  at: bigint,
): readonly TimelineEditableItemKind[] {
  const split = timedItems(track).find((item) => {
    const range = itemRange(item, track);
    return range.start < at && at < range.end;
  });
  return split ? [editableKind(split)] : [];
}

function fragmentKindsInRange(
  track: TimelineCanvasTrack,
  range: ExactTrackRange,
): readonly TimelineEditableItemKind[] {
  const split = timedItems(track).find((item) => {
    const itemExact = itemRange(item, track);
    return itemExact.start < range.start && itemExact.end > range.end;
  });
  return split ? [editableKind(split)] : [];
}

function validateInsertPoint(
  track: TimelineCanvasTrack,
  at: bigint,
  operation: string,
): void {
  const end = trackEnd(track);
  if (at < 0n || at > end) {
    throw editingError(`${operation} point falls outside ${track.name}.`);
  }
}

function validateExtractRange(
  track: TimelineCanvasTrack,
  range: ExactTrackRange,
  operation: string,
): void {
  if (range.start < 0n || range.duration <= 0n || range.end > trackEnd(track)) {
    throw editingError(`${operation} range is not covered by ${track.name}.`);
  }
}

function displayPointOnTrack(
  model: TimelineCanvasModel,
  track: TimelineCanvasTrack,
  seconds: number,
): bigint {
  if (!Number.isFinite(seconds)) {
    throw editingError("Edit point must be finite.");
  }
  const editUnits =
    ((seconds - model.globalStartSeconds) * model.editRate.numerator) /
    model.editRate.denominator;
  const rounded = Math.round(editUnits);
  if (
    !Number.isSafeInteger(rounded) ||
    Math.abs(editUnits - rounded) > DISPLAY_QUANTIZATION_EPSILON
  ) {
    throw editingError("Edit point must align exactly to the timeline frame clock.");
  }
  return rescaleExact(
    BigInt(rounded),
    model.editRate,
    track.timebase,
    `Edit point on ${track.name}`,
  );
}

function itemRange(
  item: TimelineCanvasItem,
  track: TimelineCanvasTrack,
): ExactTrackRange {
  requireSameRate(
    item.recordRange.start.timebase,
    track.timebase,
    `${item.name} record start`,
  );
  requireSameRate(
    item.recordRange.duration.timebase,
    track.timebase,
    `${item.name} record duration`,
  );
  return exactTrackRange(
    signedUnits(item.recordRange.start.value, `${item.name} record start`),
    unsignedUnits(item.recordRange.duration.value, `${item.name} record duration`),
    track.timebase,
    item.name,
  );
}

function itemStart(item: TimelineCanvasItem, track: TimelineCanvasTrack): bigint {
  return itemRange(item, track).start;
}

function itemEnd(item: TimelineCanvasItem, track: TimelineCanvasTrack): bigint {
  return itemRange(item, track).end;
}

function trackEnd(track: TimelineCanvasTrack): bigint {
  return timedItems(track).reduce(
    (end, item) => {
      const itemEndValue = itemEnd(item, track);
      return itemEndValue > end ? itemEndValue : end;
    },
    0n,
  );
}

function exactTrackRange(
  start: bigint,
  duration: bigint,
  timebase: TimelineRate,
  label: string,
): ExactTrackRange {
  if (duration <= 0n) {
    throw editingError(`${label} duration must be positive.`);
  }
  return { start, duration, end: start + duration, timebase };
}

function exactTime(value: bigint, timebase: TimelineRate): ExactTime {
  return {
    value: safeInteger(value, "edit time"),
    timebase: publicTimebase(timebase),
  };
}

function exactRange(range: ExactTrackRange): ExactTimeRange {
  return {
    start: exactTime(range.start, range.timebase),
    duration: {
      value: safeInteger(range.duration, "edit duration"),
      timebase: publicTimebase(range.timebase),
    },
  };
}

function publicTimebase(timebase: TimelineRate): TimelineRate {
  validateRate(timebase, "edit timebase");
  return {
    numerator: timebase.numerator,
    denominator: timebase.denominator,
  };
}

function rescaleExact(
  value: bigint,
  from: TimelineRate,
  to: TimelineRate,
  label: string,
): bigint {
  validateRate(from, `${label} source clock`);
  validateRate(to, `${label} destination clock`);
  const numerator =
    value * BigInt(from.denominator) * BigInt(to.numerator);
  const denominator = BigInt(from.numerator) * BigInt(to.denominator);
  if (numerator % denominator !== 0n) {
    throw editingError(`${label} cannot be represented exactly on the destination clock.`);
  }
  return numerator / denominator;
}

function signedUnits(value: string, label: string): bigint {
  if (!/^(?:0|-[1-9][0-9]*|[1-9][0-9]*)$/.test(value)) {
    throw editingError(`${label} is not a canonical signed integer.`);
  }
  return BigInt(value);
}

function unsignedUnits(value: string, label: string): bigint {
  if (!/^(?:0|[1-9][0-9]*)$/.test(value)) {
    throw editingError(`${label} is not a canonical unsigned integer.`);
  }
  return BigInt(value);
}

function safeInteger(value: bigint, label: string): number {
  const number = Number(value);
  if (!Number.isSafeInteger(number)) {
    throw editingError(`${label} exceeds the safe desktop API coordinate range.`);
  }
  return number;
}

function validateRate(rate: TimelineRate, label: string): void {
  if (
    !Number.isSafeInteger(rate.numerator) ||
    !Number.isSafeInteger(rate.denominator) ||
    rate.numerator <= 0 ||
    rate.denominator <= 0
  ) {
    throw editingError(`${label} must contain positive safe integers.`);
  }
}

function requireSameRate(
  left: TimelineRate,
  right: TimelineRate,
  label: string,
): void {
  if (
    left.numerator !== right.numerator ||
    left.denominator !== right.denominator
  ) {
    throw editingError(`${label} does not use the track semantics clock.`);
  }
}

function requireTrack(
  model: TimelineCanvasModel,
  trackId: string,
): TimelineCanvasTrack {
  const track = model.tracks.find((candidate) => candidate.id === trackId);
  if (!track) throw editingError("The selected editorial track no longer exists.");
  requireUnlockedTrack(track);
  return track;
}

function requireUnlockedTrack(track: TimelineCanvasTrack): void {
  if (track.locked) {
    throw editingError(`${track.name} is locked and cannot be edited.`);
  }
}

function requireItem(
  track: TimelineCanvasTrack,
  itemId: string,
): TimelineCanvasItem {
  const item = track.items.find((candidate) => candidate.id === itemId);
  if (!item) throw editingError("The selected editorial object no longer exists.");
  return item;
}

function timedItems(track: TimelineCanvasTrack): readonly TimelineCanvasItem[] {
  return track.items.filter((item) => item.kind !== "transition");
}

function editableKind(item: TimelineCanvasItem): TimelineEditableItemKind {
  if (item.kind === "transition") {
    throw editingError("Transitions cannot receive fragment identities.");
  }
  return item.kind;
}

function objectReference(item: TimelineCanvasItem): EditorialObjectId {
  return { kind: editableKind(item), id: item.id } as EditorialObjectId;
}

function allocateObjectReference(
  kind: TimelineEditableItemKind,
  allocateId: TimelineIdentityAllocator,
): EditorialObjectId {
  return { kind, id: allocateTypedId(kind, allocateId) } as EditorialObjectId;
}

function allocateTypedId(
  kind: TimelineEditableItemKind,
  allocateId: TimelineIdentityAllocator,
): string {
  const id = allocateId(kind);
  if (!GENERATED_ID.test(id) || !id.startsWith(`${kind}:`)) {
    throw editingError(`Identity allocator returned an invalid ${kind} identity.`);
  }
  return id;
}

function allocateTimelineIdentity(kind: TimelineEditableItemKind): string {
  if (typeof crypto === "undefined" || typeof crypto.getRandomValues !== "function") {
    throw editingError("Secure editorial identity allocation is unavailable.");
  }
  const bytes = crypto.getRandomValues(new Uint8Array(16));
  if (bytes.every((value) => value === 0)) bytes[15] = 1;
  const hex = [...bytes].map((value) => value.toString(16).padStart(2, "0")).join("");
  return `${kind}:${hex}`;
}

function toolLabel(tool: TimelineEditingTool): string {
  return timelineEditingTools.find((candidate) => candidate.id === tool)?.label ?? tool;
}

function freezePlan(plan: TimelineEditPlan): TimelineEditPlan {
  return deepFreeze({
    ...plan,
    operations: [...plan.operations],
    affectedItemIds: [...plan.affectedItemIds],
  });
}

function deepFreeze<T>(value: T): T {
  if (typeof value !== "object" || value === null || Object.isFrozen(value)) {
    return value;
  }
  for (const child of Object.values(value)) deepFreeze(child);
  return Object.freeze(value);
}

function editingError(message: string): TimelineEditingError {
  return new TimelineEditingError(message);
}
