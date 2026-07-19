import type {
  EditorialObjectId,
  EditorCanonicalDocument,
  ExactDuration,
  ExactTimeRange,
  ProjectAction,
} from "./api.ts";
import {
  projectTimelineDocument,
  type TimelineCanvasModel,
  type TimelineCanvasTrack,
  type TimelineRate,
  type TimelineSelectionTarget,
} from "./timeline-workspace.ts";

type NestedSequenceAction = Extract<
  ProjectAction,
  { readonly action: "place_nested_sequence" }
>;
type CompoundClipAction = Extract<
  ProjectAction,
  { readonly action: "create_compound_clip" }
>;

export interface TimelineCatalogEntry {
  readonly id: string;
  readonly name: string;
  readonly model: TimelineCanvasModel;
  readonly duration: ExactDuration;
  readonly childTimelineIds: readonly string[];
}

export interface TimelineCatalog {
  readonly entries: readonly TimelineCatalogEntry[];
  readonly byId: ReadonlyMap<string, TimelineCatalogEntry>;
}

export type NestedSequencePlacementIntent =
  | { readonly placement: "append" }
  | {
      readonly placement: "replace";
      readonly target: TimelineSelectionTarget;
    };

export interface BuildNestedSequenceActionInput {
  readonly catalog: TimelineCatalog;
  readonly parentTimelineId: string;
  readonly parentTrackId: string;
  readonly sourceTimelineId: string;
  readonly clipId: string;
  readonly name: string;
  readonly placement: NestedSequencePlacementIntent;
}

export interface BuildCompoundClipActionInput {
  readonly model: TimelineCanvasModel;
  readonly selectedTargets: readonly TimelineSelectionTarget[];
  readonly compoundTimelineId: string;
  readonly name: string;
  readonly createTrackId: (track: TimelineCanvasTrack) => string;
  readonly createClipId: (track: TimelineCanvasTrack) => string;
}

export function projectTimelineCatalog(
  document: EditorCanonicalDocument,
): TimelineCatalog {
  const timelineIds = canonicalTimelineIds(document);
  const entries = timelineIds.map((timelineId): TimelineCatalogEntry => {
    const model = projectTimelineDocument(document, timelineId);
    const childTimelineIds: string[] = [];
    const seen = new Set<string>();
    for (const track of model.tracks) {
      for (const item of track.items) {
        if (
          item.kind !== "clip" ||
          item.source?.kind !== "timeline" ||
          seen.has(item.source.id)
        ) {
          continue;
        }
        seen.add(item.source.id);
        childTimelineIds.push(item.source.id);
      }
    }
    return Object.freeze({
      id: model.id,
      name: model.name,
      model,
      duration: timelineDuration(model),
      childTimelineIds: Object.freeze(childTimelineIds),
    });
  });
  const byId = new Map(entries.map((entry) => [entry.id, entry]));
  for (const entry of entries) {
    for (const childTimelineId of entry.childTimelineIds) {
      if (!byId.has(childTimelineId)) {
        throw new Error(
          `${entry.name} references missing child timeline ${childTimelineId}.`,
        );
      }
    }
  }
  return Object.freeze({
    entries: Object.freeze(entries),
    byId,
  });
}

export function nestedTimelineCandidates(
  catalog: TimelineCatalog,
  parentTimelineId: string,
): readonly TimelineCatalogEntry[] {
  requireTimeline(catalog, parentTimelineId, "parent");
  return Object.freeze(
    catalog.entries.filter(
      (entry) =>
        entry.id !== parentTimelineId &&
        !timelineReaches(catalog, entry.id, parentTimelineId),
    ),
  );
}

export function openNestedTimelinePath(
  catalog: TimelineCatalog,
  path: readonly string[],
  clipId: string,
): readonly string[] {
  if (path.length === 0) {
    throw new Error("Open-in-timeline requires an active timeline path.");
  }
  const rootTimelineId = path[0];
  const reconciled = reconcileTimelinePath(catalog, rootTimelineId, path);
  if (!sameStrings(reconciled, path)) {
    throw new Error("The active timeline path is stale and must be reconciled.");
  }
  const parent = requireTimeline(catalog, path[path.length - 1], "active");
  const matching = parent.model.tracks
    .flatMap((track) => track.items)
    .filter((item) => item.kind === "clip" && item.id === clipId);
  if (matching.length !== 1) {
    throw new Error(`Clip ${clipId} must exist exactly once on ${parent.name}.`);
  }
  const source = matching[0].source;
  if (source?.kind !== "timeline") {
    throw new Error(`Clip ${clipId} does not reference a child timeline.`);
  }
  requireTimeline(catalog, source.id, "child");
  if (path.includes(source.id)) {
    throw new Error(`Opening ${source.id} would repeat a timeline in the active path.`);
  }
  return Object.freeze([...path, source.id]);
}

export function reconcileTimelinePath(
  catalog: TimelineCatalog,
  rootTimelineId: string,
  requestedPath: readonly string[],
): readonly string[] {
  requireTimeline(catalog, rootTimelineId, "root");
  const reconciled = [rootTimelineId];
  if (requestedPath[0] !== rootTimelineId) return Object.freeze(reconciled);
  for (const childTimelineId of requestedPath.slice(1)) {
    const parent = requireTimeline(
      catalog,
      reconciled[reconciled.length - 1],
      "path parent",
    );
    if (
      !parent.childTimelineIds.includes(childTimelineId) ||
      !catalog.byId.has(childTimelineId) ||
      reconciled.includes(childTimelineId)
    ) {
      break;
    }
    reconciled.push(childTimelineId);
  }
  return Object.freeze(reconciled);
}

export function buildNestedSequenceAction(
  input: BuildNestedSequenceActionInput,
): NestedSequenceAction {
  const parent = requireTimeline(input.catalog, input.parentTimelineId, "parent");
  const source = requireTimeline(input.catalog, input.sourceTimelineId, "source");
  if (timelineReaches(input.catalog, source.id, parent.id)) {
    throw new Error(
      `Placing ${source.name} in ${parent.name} would create a timeline cycle.`,
    );
  }
  if (source.duration.value <= 0) {
    throw new Error(`${source.name} is empty and cannot be placed as a nested sequence.`);
  }
  const track = parent.model.tracks.find(
    (candidate) => candidate.id === input.parentTrackId,
  );
  if (!track) {
    throw new Error(`Target track ${input.parentTrackId} was not found on ${parent.name}.`);
  }
  if (track.locked) {
    throw new Error(`${track.name} is locked and cannot receive a nested sequence.`);
  }
  requireTypedId(input.clipId, "clip");
  const name = requireName(input.name, "Nested sequence");
  const sourceDurationOnTrack = rescaleExact(
    BigInt(source.duration.value),
    source.model.editRate,
    track.timebase,
    `${source.name} duration on ${track.name}`,
  );

  let placement: NestedSequenceAction["request"]["placement"];
  if (input.placement.placement === "append") {
    placement = { placement: "append" };
  } else {
    const target = input.placement.target;
    if (target.trackId !== track.id) {
      throw new Error("Replace requires one selected object on the exact target track.");
    }
    if (target.item.kind === "transition") {
      throw new Error("A transition cannot be replaced by a nested sequence.");
    }
    const targetDuration = exactUnits(
      target.item.recordRange.duration.value,
      `${target.item.name} record duration`,
    );
    requireSameRate(
      target.item.recordRange.duration.timebase,
      track.timebase,
      `${target.item.name} record duration`,
    );
    if (targetDuration !== sourceDurationOnTrack) {
      throw new Error(
        `Replace requires ${source.name} to match the exact duration of ${target.item.name}.`,
      );
    }
    placement = {
      placement: "replace",
      target_id: publicObjectId(target),
    };
  }

  return {
    action: "place_nested_sequence",
    source_timeline_id: source.id,
    request: {
      parent_timeline_id: parent.id,
      parent_track_id: track.id,
      clip_id: input.clipId,
      name,
      source_range: {
        start: { value: 0, timebase: publicRate(source.model.editRate) },
        duration: {
          value: source.duration.value,
          timebase: publicRate(source.model.editRate),
        },
      },
      placement,
    },
  };
}

export function buildCompoundClipAction(
  input: BuildCompoundClipActionInput,
): CompoundClipAction {
  requireTypedId(input.compoundTimelineId, "timeline");
  const name = requireName(input.name, "Compound clip");
  if (input.selectedTargets.length === 0) {
    throw new Error("Select at least one complete timeline object to create a compound clip.");
  }

  const targetsByKey = new Map<string, TimelineSelectionTarget>();
  for (const target of input.selectedTargets) {
    if (target.item.kind === "transition") {
      throw new Error(
        "Transitions are retained from their complete endpoints and cannot be selected directly for a compound clip.",
      );
    }
    const track = input.model.tracks.find((candidate) => candidate.id === target.trackId);
    const currentItem = track?.items.find(
      (item) => item.kind === target.item.kind && item.id === target.item.id,
    );
    if (!currentItem) {
      throw new Error("The compound selection is stale for the active timeline revision.");
    }
    const key = `${target.item.kind}:${target.item.id}`;
    if (!targetsByKey.has(key)) targetsByKey.set(key, target);
  }
  const orderedTargets = input.model.tracks.flatMap((track) =>
    track.items.flatMap((item) => {
      const target = targetsByKey.get(`${item.kind}:${item.id}`);
      return target ? [target] : [];
    }),
  );

  const affectedTrackIds = new Set(
    orderedTargets.map((target) => target.trackId),
  );
  const tracks = input.model.tracks
    .filter((track) => affectedTrackIds.has(track.id))
    .map((track) => {
      if (track.locked) {
        throw new Error(`${track.name} is locked and cannot be compounded.`);
      }
      const compoundTrackId = input.createTrackId(track);
      const clipId = input.createClipId(track);
      requireTypedId(compoundTrackId, "track");
      requireTypedId(clipId, "clip");
      return {
        parent_track_id: track.id,
        compound_track_id: compoundTrackId,
        clip_id: clipId,
      };
    });
  requireDistinct(
    tracks.flatMap((track) => [track.compound_track_id, track.clip_id]),
    "compound track and clip identities",
  );

  return {
    action: "create_compound_clip",
    request: {
      parent_timeline_id: input.model.id,
      compound_timeline_id: input.compoundTimelineId,
      name,
      selected_objects: orderedTargets.map(publicObjectId),
      tracks,
    },
  };
}

function canonicalTimelineIds(document: EditorCanonicalDocument): readonly string[] {
  const content = requireRecord(document.content, "document.content");
  const payload = requireRecord(content.payload, "document.content.payload");
  if (!Array.isArray(payload.timelines)) {
    throw new Error("The canonical document timeline catalog is unavailable.");
  }
  const ids = payload.timelines.map((value, index) => {
    const timeline = requireRecord(value, `document.content.payload.timelines[${index}]`);
    if (typeof timeline.id !== "string" || timeline.id.length === 0) {
      throw new Error(`Timeline ${index} has no stable identity.`);
    }
    return timeline.id;
  });
  requireDistinct(ids, "timeline identities");
  return Object.freeze(ids);
}

function timelineDuration(model: TimelineCanvasModel): ExactDuration {
  let longest = 0n;
  for (const track of model.tracks) {
    let end = 0n;
    for (const item of track.items) {
      if (item.kind === "transition") continue;
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
      const candidate =
        exactUnits(item.recordRange.start.value, `${item.name} record start`) +
        exactUnits(item.recordRange.duration.value, `${item.name} record duration`);
      if (candidate > end) end = candidate;
    }
    const editEnd = rescaleExact(
      end,
      track.timebase,
      model.editRate,
      `${track.name} duration`,
    );
    if (editEnd > longest) longest = editEnd;
  }
  return Object.freeze({
    value: safeInteger(longest, `${model.name} duration`),
    timebase: publicRate(model.editRate),
  });
}

function timelineReaches(
  catalog: TimelineCatalog,
  fromTimelineId: string,
  targetTimelineId: string,
): boolean {
  const pending = [fromTimelineId];
  const visited = new Set<string>();
  while (pending.length > 0) {
    const timelineId = pending.pop()!;
    if (timelineId === targetTimelineId) return true;
    if (visited.has(timelineId)) continue;
    visited.add(timelineId);
    const entry = catalog.byId.get(timelineId);
    if (entry) pending.push(...entry.childTimelineIds);
  }
  return false;
}

function publicObjectId(target: TimelineSelectionTarget): EditorialObjectId {
  return { kind: target.item.kind, id: target.item.id };
}

function publicRate(rate: TimelineRate): TimelineRate {
  validateRate(rate, "timeline rate");
  return { numerator: rate.numerator, denominator: rate.denominator };
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
    throw new Error(`${label} is not exactly representable on the destination clock.`);
  }
  return numerator / denominator;
}

function exactUnits(value: string, label: string): bigint {
  if (!/^(?:0|-[1-9][0-9]*|[1-9][0-9]*)$/.test(value)) {
    throw new Error(`${label} is not a canonical integer.`);
  }
  return BigInt(value);
}

function safeInteger(value: bigint, label: string): number {
  const result = Number(value);
  if (!Number.isSafeInteger(result)) {
    throw new Error(`${label} exceeds the exact desktop API range.`);
  }
  return result;
}

function requireSameRate(left: TimelineRate, right: TimelineRate, label: string): void {
  validateRate(left, label);
  validateRate(right, label);
  if (
    left.numerator !== right.numerator ||
    left.denominator !== right.denominator
  ) {
    throw new Error(`${label} does not use the owning track clock.`);
  }
}

function validateRate(rate: TimelineRate, label: string): void {
  if (
    !Number.isSafeInteger(rate.numerator) ||
    !Number.isSafeInteger(rate.denominator) ||
    rate.numerator <= 0 ||
    rate.denominator <= 0
  ) {
    throw new Error(`${label} must contain positive safe integers.`);
  }
}

function requireTypedId(value: string, kind: "timeline" | "track" | "clip"): void {
  if (!new RegExp(`^${kind}:[0-9a-f]{32}$`).test(value)) {
    throw new Error(`The generated ${kind} identity is invalid.`);
  }
}

function requireName(value: string, label: string): string {
  const name = value.trim();
  if (name.length === 0) throw new Error(`${label} name must not be empty.`);
  return name;
}

function requireTimeline(
  catalog: TimelineCatalog,
  timelineId: string,
  label: string,
): TimelineCatalogEntry {
  const entry = catalog.byId.get(timelineId);
  if (!entry) throw new Error(`The ${label} timeline ${timelineId} was not found.`);
  return entry;
}

function requireRecord(value: unknown, label: string): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${label} must be an object.`);
  }
  return value as Record<string, unknown>;
}

function requireDistinct(values: readonly string[], label: string): void {
  if (new Set(values).size !== values.length) {
    throw new Error(`${label} must be unique.`);
  }
}

function sameStrings(left: readonly string[], right: readonly string[]): boolean {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}
