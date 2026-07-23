import type {
  EditorCanonicalDocument,
  EditorMulticamAudioPolicy,
  EditorMulticamSyncMethod,
  ExactTime,
  ProjectAction,
} from "./api.ts";
import {
  timelineExactPointAtDisplaySeconds,
  type TimelineCanvasItem,
  type TimelineCanvasModel,
  type TimelineExactPoint,
  type TimelineExactRange,
  type TimelineRate,
} from "./timeline-workspace.ts";

type MulticamAction = Extract<
  ProjectAction,
  { readonly action: "mutate_multicam" }
>;
type JsonRecord = Record<string, unknown>;

export interface TimelineMulticamAnglePresentation {
  readonly id: string;
  readonly name: string;
  readonly cameraLabel: string;
  readonly enabled: boolean;
  readonly available: boolean;
  readonly sourceClipIds: readonly string[];
  readonly active: boolean;
}

export interface TimelineMulticamSwitchPresentation {
  readonly angleId: string;
  readonly sourceRange: TimelineExactRange;
}

export interface TimelineMulticamCutPresentation {
  readonly switchIndex: number;
  readonly recordTime: ExactTime;
  readonly recordSeconds: number;
  readonly fromAngleId: string;
  readonly toAngleId: string;
}

export interface TimelineMulticamUnavailable {
  readonly status: "unavailable";
  readonly reason: string;
}

export interface TimelineMulticamSetup {
  readonly status: "setup";
  readonly targetTimelineId: string;
  readonly clipId: string;
  readonly sourceTimelineId: string;
  readonly sourceName: string;
  readonly sourceAuthored: boolean;
  readonly syncMethod: EditorMulticamSyncMethod | null;
  readonly canCreate: boolean;
  readonly candidateAngleCount: number;
  readonly angles: readonly TimelineMulticamAnglePresentation[];
  readonly reason: string;
}

export interface TimelineMulticamReady {
  readonly status: "ready";
  readonly targetTimelineId: string;
  readonly clipId: string;
  readonly clipName: string;
  readonly clipRecordRange: TimelineExactRange;
  readonly sourceTimelineId: string;
  readonly sourceName: string;
  readonly syncMethod: EditorMulticamSyncMethod;
  readonly audioPolicy: EditorMulticamAudioPolicy;
  readonly angles: readonly TimelineMulticamAnglePresentation[];
  readonly switches: readonly TimelineMulticamSwitchPresentation[];
  readonly cuts: readonly TimelineMulticamCutPresentation[];
  readonly activeAngleId: string | null;
}

export type TimelineMulticamProjection =
  | TimelineMulticamUnavailable
  | TimelineMulticamSetup
  | TimelineMulticamReady;

export interface BuildMulticamCreationActionInput {
  readonly targetTimelineId: string;
  readonly selectedClip: TimelineCanvasItem;
  readonly sourceModel: TimelineCanvasModel;
  readonly syncMethod: EditorMulticamSyncMethod;
  readonly createAngleId: () => string;
}

export class TimelineMulticamError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "TimelineMulticamError";
  }
}

export function projectTimelineMulticam(
  document: EditorCanonicalDocument,
  model: TimelineCanvasModel,
  selectedClip: TimelineCanvasItem | null,
  playheadSeconds: number,
): TimelineMulticamProjection {
  try {
    return freezeProjection(
      projectTimelineMulticamChecked(document, model, selectedClip, playheadSeconds),
    );
  } catch (error: unknown) {
    return Object.freeze({
      status: "unavailable",
      reason:
        error instanceof Error && error.message.length > 0
          ? error.message
          : "Multicam state could not be projected from the canonical timeline.",
    });
  }
}

export function buildMulticamCreationAction({
  targetTimelineId,
  selectedClip,
  sourceModel,
  syncMethod,
  createAngleId,
}: BuildMulticamCreationActionInput): MulticamAction {
  requireNestedTarget(targetTimelineId, selectedClip, sourceModel.id);
  const sourceTracks = sourceModel.tracks
    .filter((track) => track.kind === "video")
    .map((track) => ({
      track,
      sourceClipIds: track.items
        .filter((item) => item.kind === "clip")
        .map((item) => item.id),
    }))
    .filter((candidate) => candidate.sourceClipIds.length > 0);
  if (sourceTracks.length < 2) {
    throw new TimelineMulticamError(
      "Multicam creation requires at least two nonempty video tracks in the nested source timeline.",
    );
  }
  const ids = new Set<string>();
  const angles = sourceTracks.map(({ track, sourceClipIds }, index) => {
    const angleId = createAngleId();
    if (!isTypedId(angleId, "multicam-angle") || ids.has(angleId)) {
      throw new TimelineMulticamError(
        "Multicam creation produced an invalid or duplicate angle identity.",
      );
    }
    ids.add(angleId);
    const label = track.name.trim() || `Camera ${index + 1}`;
    return {
      angle_id: angleId,
      name: label,
      camera_label: label,
      enabled: true,
      metadata: {},
      source_clip_ids: sourceClipIds,
    };
  });
  const initial = angles[0];
  if (!initial) {
    throw new TimelineMulticamError("Multicam creation has no initial camera angle.");
  }
  return {
    action: "mutate_multicam",
    mutations: [
      {
        operation: "set_source",
        timeline_id: sourceModel.id,
        source: { sync_method: syncMethod, angles },
      },
      {
        operation: "attach_clip",
        timeline_id: targetTimelineId,
        clip_id: selectedClip.id,
        initial_angle_id: initial.angle_id,
        audio_policy: { kind: "follow_video" },
      },
    ],
  };
}

export function buildMulticamAttachAction(
  projection: TimelineMulticamSetup,
): MulticamAction {
  if (!projection.sourceAuthored) {
    throw new TimelineMulticamError(
      "Create synchronized source angles before attaching this nested clip.",
    );
  }
  const initial =
    projection.angles.find((angle) => angle.enabled) ?? projection.angles[0];
  if (!initial) {
    throw new TimelineMulticamError(
      "The synchronized source has no angle available for attachment.",
    );
  }
  return oneMutation({
    operation: "attach_clip",
    timeline_id: projection.targetTimelineId,
    clip_id: projection.clipId,
    initial_angle_id: initial.id,
    audio_policy: { kind: "follow_video" },
  });
}

export function buildMulticamSyncAction(
  projection: Pick<TimelineMulticamReady, "sourceTimelineId">,
  syncMethod: EditorMulticamSyncMethod,
): MulticamAction {
  return oneMutation({
    operation: "set_sync_method",
    timeline_id: projection.sourceTimelineId,
    sync_method: syncMethod,
  });
}

export function buildMulticamSwitchAction(
  projection: TimelineMulticamReady,
  model: TimelineCanvasModel,
  playheadSeconds: number,
  angleId: string,
): MulticamAction {
  const angle = projection.angles.find((candidate) => candidate.id === angleId);
  if (!angle?.enabled) {
    throw new TimelineMulticamError("Choose an enabled multicam angle before switching.");
  }
  const canonicalRecordTime = timelineExactPointAtDisplaySeconds(
    model,
    playheadSeconds,
  );
  if (!contains(projection.clipRecordRange, canonicalRecordTime)) {
    throw new TimelineMulticamError(
      "Move the playhead inside the selected multicam clip before switching angles.",
    );
  }
  if (!angle.available) {
    throw new TimelineMulticamError(
      "Choose a multicam angle with source media at the playhead before switching.",
    );
  }
  return oneMutation({
    operation: "switch_at",
    timeline_id: projection.targetTimelineId,
    clip_id: projection.clipId,
    record_time: publicExactTime(canonicalRecordTime),
    angle_id: angleId,
  });
}

export function buildMulticamMoveCutAction(
  projection: TimelineMulticamReady,
  cutIndex: number,
  deltaFrames: number,
): MulticamAction {
  const cut = projection.cuts[cutIndex];
  if (!cut || !Number.isSafeInteger(deltaFrames) || deltaFrames === 0) {
    throw new TimelineMulticamError("Choose one multicam cut and a nonzero frame nudge.");
  }
  const current = BigInt(cut.recordTime.value);
  const next = current + BigInt(deltaFrames);
  const clipStart = integer(
    projection.clipRecordRange.start.value,
    "multicam clip start",
  );
  const clipEnd =
    clipStart +
    integer(projection.clipRecordRange.duration.value, "multicam clip duration");
  if (next <= clipStart || next >= clipEnd) {
    throw new TimelineMulticamError(
      "A refined multicam cut must remain strictly inside the selected clip.",
    );
  }
  return oneMutation({
    operation: "move_cut",
    timeline_id: projection.targetTimelineId,
    clip_id: projection.clipId,
    at_record_time: cut.recordTime,
    to_record_time: { ...cut.recordTime, value: safeInteger(next, "refined multicam cut") },
  });
}

export function buildMulticamAudioAction(
  projection: TimelineMulticamReady,
  audioPolicy: EditorMulticamAudioPolicy,
): MulticamAction {
  if (
    audioPolicy.kind === "fixed" &&
    !projection.angles.some(
      (angle) => angle.id === audioPolicy.angle_id && angle.enabled,
    )
  ) {
    throw new TimelineMulticamError(
      "Fixed multicam audio requires an enabled source angle.",
    );
  }
  return oneMutation({
    operation: "set_audio_policy",
    timeline_id: projection.targetTimelineId,
    clip_id: projection.clipId,
    audio_policy: audioPolicy,
  });
}

export function buildMulticamDetachAction(
  projection: TimelineMulticamReady,
): MulticamAction {
  return oneMutation({
    operation: "detach_clip",
    timeline_id: projection.targetTimelineId,
    clip_id: projection.clipId,
  });
}

function projectTimelineMulticamChecked(
  document: EditorCanonicalDocument,
  model: TimelineCanvasModel,
  selectedClip: TimelineCanvasItem | null,
  playheadSeconds: number,
): TimelineMulticamSetup | TimelineMulticamReady {
  if (
    selectedClip === null ||
    selectedClip.kind !== "clip" ||
    selectedClip.source?.kind !== "timeline"
  ) {
    throw new TimelineMulticamError(
      "Select one nested timeline clip to create or edit multicam state.",
    );
  }
  if (!Number.isFinite(playheadSeconds)) {
    throw new TimelineMulticamError("The timeline playhead is not finite.");
  }
  const timelines = canonicalTimelines(document);
  const target = uniqueTimeline(timelines, model.id);
  const source = uniqueTimeline(timelines, selectedClip.source.id);
  const rawTargetClip = uniqueRawClip(target, selectedClip.id);
  const sourceName = nonemptyString(source.name, "multicam source name");
  const rawSource = source.multicam_source;
  const rawClipMatches = array(target.multicam_clips, "multicam clips").filter(
    (candidate) => record(candidate, "multicam clip").clip_id === selectedClip.id,
  );
  if (rawClipMatches.length > 1) {
    throw new TimelineMulticamError(
      `Clip ${selectedClip.id} has duplicate multicam state.`,
    );
  }
  const rawClip = rawClipMatches[0];
  if (rawSource === null && rawClip !== undefined) {
    throw new TimelineMulticamError(
      "The selected clip has switch state but its synchronized source is absent.",
    );
  }
  const candidateAngleCount = candidateSourceAngleCount(source);
  if (rawSource === null || rawClip === undefined) {
    const parsedSource =
      rawSource === null ? null : record(rawSource, "multicam source");
    const parsedAngles = parsedSource === null ? [] : parseAngles(parsedSource.angles);
    const syncMethod =
      parsedSource === null ? null : parseSyncMethod(parsedSource.sync_method);
    return {
      status: "setup",
      targetTimelineId: model.id,
      clipId: selectedClip.id,
      sourceTimelineId: selectedClip.source.id,
      sourceName,
      sourceAuthored: rawSource !== null,
      syncMethod,
      canCreate:
        rawSource === null
          ? candidateAngleCount >= 2
          : parsedAngles.some((angle) => angle.enabled),
      candidateAngleCount,
      angles: parsedAngles.map((angle) => ({
        ...angle,
        available: false,
        active: false,
      })),
      reason:
        rawSource === null
          ? candidateAngleCount >= 2
            ? "Create synchronized angles from the nested source video tracks."
            : "The nested source needs at least two nonempty video tracks."
          : "Attach the selected nested clip to its existing synchronized source.",
    };
  }

  const sourceState = record(rawSource, "multicam source");
  const syncMethod = parseSyncMethod(sourceState.sync_method);
  const parsedAngles = parseAngles(sourceState.angles);
  const clipState = record(rawClip, "multicam clip");
  const switches = parseSwitches(clipState.switches);
  if (switches.length === 0) {
    throw new TimelineMulticamError("The selected multicam clip has no switch program.");
  }
  const audioPolicy = parseAudioPolicy(clipState.audio_policy);
  const clipRecordRange = exactRange(rawTargetClip.record_range, "clip record range");
  const timeMap = parseTimeMap(rawTargetClip.time_map);
  const recordTime = timelineExactPointAtDisplaySeconds(model, playheadSeconds);
  const sourceClipRanges = sourceClipRecordRanges(source);
  let playheadSourceTime: TimelineExactPoint | null = null;
  let activeAngleId: string | null = null;
  if (contains(clipRecordRange, recordTime)) {
    const localRecord = subtractPoints(recordTime, clipRecordRange.start);
    const sourceTime = sourceAtLocalRecord(timeMap, localRecord);
    playheadSourceTime = sourceTime;
    activeAngleId =
      switches.find((entry) => contains(entry.sourceRange, sourceTime))?.angleId ??
      null;
  }
  const cuts = switches.slice(1).map((entry, index) => {
    const localRecord = localRecordAtSource(timeMap, entry.sourceRange.start);
    const absoluteRecord = addPoints(clipRecordRange.start, localRecord);
    return {
      switchIndex: index + 1,
      recordTime: publicExactTime(absoluteRecord),
      recordSeconds: pointSeconds(absoluteRecord),
      fromAngleId: switches[index]?.angleId ?? entry.angleId,
      toAngleId: entry.angleId,
    };
  });
  const angleIds = new Set(parsedAngles.map((angle) => angle.id));
  for (const entry of switches) {
    if (!angleIds.has(entry.angleId)) {
      throw new TimelineMulticamError(
        `Multicam switch references missing angle ${entry.angleId}.`,
      );
    }
  }
  return {
    status: "ready",
    targetTimelineId: model.id,
    clipId: selectedClip.id,
    clipName: selectedClip.name,
    clipRecordRange,
    sourceTimelineId: selectedClip.source.id,
    sourceName,
    syncMethod,
    audioPolicy,
    angles: parsedAngles.map((angle) => ({
      ...angle,
      available:
        angle.enabled &&
        playheadSourceTime !== null &&
        angle.sourceClipIds.some((clipId) =>
          contains(
            requiredSourceClipRange(sourceClipRanges, clipId),
            playheadSourceTime,
          ),
        ),
      active: angle.id === activeAngleId,
    })),
    switches,
    cuts,
    activeAngleId,
  };
}

function canonicalTimelines(document: EditorCanonicalDocument): readonly JsonRecord[] {
  if (document.format !== "superi.timeline" || document.format_revision !== 2) {
    throw new TimelineMulticamError("Multicam requires timeline format revision 2.");
  }
  const content = record(document.content, "timeline document content");
  if (content.format !== "superi.timeline" || content.format_revision !== 2) {
    throw new TimelineMulticamError("The canonical timeline envelope is unsupported.");
  }
  const payload = record(content.payload, "timeline payload");
  return array(payload.timelines, "timelines").map((value) =>
    record(value, "timeline"),
  );
}

function uniqueTimeline(
  timelines: readonly JsonRecord[],
  timelineId: string,
): JsonRecord {
  const matches = timelines.filter((timeline) => timeline.id === timelineId);
  if (matches.length !== 1) {
    throw new TimelineMulticamError(
      `Timeline ${timelineId} must appear exactly once in the canonical document.`,
    );
  }
  return matches[0]!;
}

function uniqueRawClip(timeline: JsonRecord, clipId: string): JsonRecord {
  const matches: JsonRecord[] = [];
  for (const trackValue of array(timeline.tracks, "timeline tracks")) {
    const track = record(trackValue, "timeline track");
    for (const itemValue of array(track.items, "timeline track items")) {
      const item = record(itemValue, "timeline item");
      if (item.kind === "clip" && item.id === clipId) matches.push(item);
    }
  }
  if (matches.length !== 1) {
    throw new TimelineMulticamError(
      `Clip ${clipId} must appear exactly once in the active timeline.`,
    );
  }
  return matches[0]!;
}

function candidateSourceAngleCount(timeline: JsonRecord): number {
  return array(timeline.tracks, "source tracks").filter((trackValue) => {
    const track = record(trackValue, "source track");
    const semantics = record(track.semantics, "track semantics");
    return (
      semantics.kind === "video" &&
      array(track.items, "source track items").some(
        (item) => record(item, "source item").kind === "clip",
      )
    );
  }).length;
}

function sourceClipRecordRanges(timeline: JsonRecord): ReadonlyMap<string, TimelineExactRange> {
  const ranges = new Map<string, TimelineExactRange>();
  for (const trackValue of array(timeline.tracks, "source tracks")) {
    const track = record(trackValue, "source track");
    for (const itemValue of array(track.items, "source track items")) {
      const item = record(itemValue, "source item");
      if (item.kind !== "clip") continue;
      const id = nonemptyString(item.id, "source clip id");
      if (ranges.has(id)) {
        throw new TimelineMulticamError(`Duplicate source clip ${id}.`);
      }
      ranges.set(id, exactRange(item.record_range, "source clip record range"));
    }
  }
  return ranges;
}

function requiredSourceClipRange(
  ranges: ReadonlyMap<string, TimelineExactRange>,
  clipId: string,
): TimelineExactRange {
  const rangeValue = ranges.get(clipId);
  if (!rangeValue) {
    throw new TimelineMulticamError(
      `Multicam angle references missing source clip ${clipId}.`,
    );
  }
  return rangeValue;
}

function parseAngles(value: unknown) {
  const seen = new Set<string>();
  return array(value, "multicam angles").map((angleValue) => {
    const angle = record(angleValue, "multicam angle");
    const id = nonemptyString(angle.id, "multicam angle id");
    if (seen.has(id)) {
      throw new TimelineMulticamError(`Duplicate multicam angle ${id}.`);
    }
    seen.add(id);
    return {
      id,
      name: nonemptyString(angle.name, "multicam angle name"),
      cameraLabel: nonemptyString(angle.camera_label, "multicam camera label"),
      enabled: boolean(angle.enabled, "multicam angle enabled state"),
      sourceClipIds: array(angle.source_clips, "multicam source clips").map(
        (clipId) => nonemptyString(clipId, "multicam source clip id"),
      ),
    };
  });
}

function parseSwitches(value: unknown): TimelineMulticamSwitchPresentation[] {
  return array(value, "multicam switches").map((switchValue) => {
    const entry = record(switchValue, "multicam switch");
    return {
      angleId: nonemptyString(entry.angle_id, "multicam switch angle"),
      sourceRange: exactRange(entry.source_range, "multicam switch range"),
    };
  });
}

function parseSyncMethod(value: unknown): EditorMulticamSyncMethod {
  const method = record(value, "multicam sync method");
  switch (method.kind) {
    case "manual":
    case "timecode":
    case "in_points":
    case "out_points":
    case "audio":
      return { kind: method.kind };
    case "clip_marker":
      return {
        kind: "clip_marker",
        name: nonemptyString(method.name, "multicam sync marker name"),
      };
    default:
      throw new TimelineMulticamError("The multicam sync method is unsupported.");
  }
}

function parseAudioPolicy(value: unknown): EditorMulticamAudioPolicy {
  const policy = record(value, "multicam audio policy");
  switch (policy.kind) {
    case "follow_video":
    case "all_angles":
      return { kind: policy.kind };
    case "fixed":
      return {
        kind: "fixed",
        angle_id: nonemptyString(policy.angle_id, "fixed multicam audio angle"),
      };
    default:
      throw new TimelineMulticamError("The multicam audio policy is unsupported.");
  }
}

interface ParsedTimeMap {
  readonly sourceTimebase: TimelineRate;
  readonly recordTimebase: TimelineRate;
  readonly segments: readonly {
    readonly recordRange: TimelineExactRange;
    readonly sourceStart: TimelineExactPoint;
    readonly rateNumerator: bigint;
    readonly rateDenominator: bigint;
  }[];
}

function parseTimeMap(value: unknown): ParsedTimeMap {
  const map = record(value, "clip time map");
  const duration = exactDuration(map.record_duration, "clip time map duration");
  const sourceTimebase = rate(map.source_timebase, "clip source timebase");
  const segments = array(map.segments, "clip time map segments").map(
    (segmentValue) => {
      const segment = record(segmentValue, "clip time map segment");
      const rateNumerator = integer(segment.rate_numerator, "playback rate numerator");
      const rateDenominator = integer(
        segment.rate_denominator,
        "playback rate denominator",
      );
      if (rateDenominator <= 0n) {
        throw new TimelineMulticamError(
          "A multicam clip time-map rate denominator must be positive.",
        );
      }
      return {
        recordRange: exactRange(segment.record_range, "retime segment range"),
        sourceStart: exactPoint(segment.source_start, "retime source start"),
        rateNumerator,
        rateDenominator,
      };
    },
  );
  if (segments.length === 0) {
    throw new TimelineMulticamError("A multicam clip time map cannot be empty.");
  }
  return {
    sourceTimebase,
    recordTimebase: duration.timebase,
    segments,
  };
}

function sourceAtLocalRecord(
  map: ParsedTimeMap,
  recordTime: TimelineExactPoint,
): TimelineExactPoint {
  requireSameRate(recordTime.timebase, map.recordTimebase, "record time map");
  const value = integer(recordTime.value, "local record time");
  const segment = map.segments.find((candidate) =>
    contains(candidate.recordRange, recordTime),
  );
  if (!segment) {
    throw new TimelineMulticamError(
      "The playhead lies outside complete multicam time-map coverage.",
    );
  }
  const offset = value - integer(segment.recordRange.start.value, "segment start");
  const sourceDelta = scaleRecordOffset(map, segment, offset);
  return {
    value: (
      integer(segment.sourceStart.value, "segment source start") + sourceDelta
    ).toString(),
    timebase: map.sourceTimebase,
  };
}

function localRecordAtSource(
  map: ParsedTimeMap,
  sourceTime: TimelineExactPoint,
): TimelineExactPoint {
  requireSameRate(sourceTime.timebase, map.sourceTimebase, "source time map");
  const target = integer(sourceTime.value, "multicam source cut");
  for (const segment of map.segments) {
    if (segment.rateNumerator === 0n) {
      if (target === integer(segment.sourceStart.value, "held source start")) {
        return segment.recordRange.start;
      }
      continue;
    }
    const recordDuration = integer(
      segment.recordRange.duration.value,
      "segment record duration",
    );
    const sourceStart = integer(segment.sourceStart.value, "segment source start");
    const sourceEnd = sourceStart + scaleRecordOffset(map, segment, recordDuration);
    const minimum = sourceStart < sourceEnd ? sourceStart : sourceEnd;
    const maximum = sourceStart > sourceEnd ? sourceStart : sourceEnd;
    if (target < minimum || target > maximum) continue;
    const recordNumerator =
      (target - sourceStart) *
      BigInt(map.recordTimebase.numerator) *
      segment.rateDenominator *
      BigInt(map.sourceTimebase.denominator);
    const recordDenominator =
      BigInt(map.recordTimebase.denominator) *
      segment.rateNumerator *
      BigInt(map.sourceTimebase.numerator);
    const offset = divideExact(
      recordNumerator,
      recordDenominator,
      "multicam source cut to record mapping",
    );
    return {
      value: (
        integer(segment.recordRange.start.value, "segment record start") + offset
      ).toString(),
      timebase: map.recordTimebase,
    };
  }
  throw new TimelineMulticamError(
    "A multicam source cut does not map exactly into the selected clip.",
  );
}

function scaleRecordOffset(
  map: ParsedTimeMap,
  segment: ParsedTimeMap["segments"][number],
  offset: bigint,
): bigint {
  const numerator =
    offset *
    BigInt(map.recordTimebase.denominator) *
    segment.rateNumerator *
    BigInt(map.sourceTimebase.numerator);
  const denominator =
    BigInt(map.recordTimebase.numerator) *
    segment.rateDenominator *
    BigInt(map.sourceTimebase.denominator);
  return divideExact(numerator, denominator, "multicam record to source mapping");
}

function divideExact(numerator: bigint, denominator: bigint, label: string): bigint {
  if (denominator === 0n || numerator % denominator !== 0n) {
    throw new TimelineMulticamError(`${label} is not exact on the authored clocks.`);
  }
  return numerator / denominator;
}

function requireNestedTarget(
  targetTimelineId: string,
  selectedClip: TimelineCanvasItem,
  sourceTimelineId: string,
) {
  if (
    targetTimelineId.length === 0 ||
    selectedClip.kind !== "clip" ||
    selectedClip.source?.kind !== "timeline" ||
    selectedClip.source.id !== sourceTimelineId
  ) {
    throw new TimelineMulticamError(
      "Multicam creation requires one nested clip and its exact source timeline.",
    );
  }
}

function oneMutation(
  mutation: MulticamAction["mutations"][number],
): MulticamAction {
  return { action: "mutate_multicam", mutations: [mutation] };
}

function exactRange(value: unknown, label: string): TimelineExactRange {
  const rangeValue = record(value, label);
  const start = exactPoint(rangeValue.start, `${label} start`);
  const duration = exactDuration(rangeValue.duration, `${label} duration`);
  requireSameRate(start.timebase, duration.timebase, label);
  if (integer(duration.value, label) <= 0n) {
    throw new TimelineMulticamError(`${label} must be nonempty.`);
  }
  return { start, duration };
}

function exactPoint(value: unknown, label: string): TimelineExactPoint {
  const point = record(value, label);
  return {
    value: integer(point.value, label).toString(),
    timebase: rate(point.timebase, `${label} timebase`),
  };
}

function exactDuration(value: unknown, label: string) {
  const duration = record(value, label);
  const parsed = integer(duration.value, label);
  if (parsed < 0n) {
    throw new TimelineMulticamError(`${label} cannot be negative.`);
  }
  return {
    value: parsed.toString(),
    timebase: rate(duration.timebase, `${label} timebase`),
  };
}

function rate(value: unknown, label: string): TimelineRate {
  const parsed = record(value, label);
  if (
    !Number.isSafeInteger(parsed.numerator) ||
    !Number.isSafeInteger(parsed.denominator) ||
    Number(parsed.numerator) <= 0 ||
    Number(parsed.denominator) <= 0
  ) {
    throw new TimelineMulticamError(`${label} is invalid.`);
  }
  return {
    numerator: Number(parsed.numerator),
    denominator: Number(parsed.denominator),
  };
}

function contains(
  rangeValue: TimelineExactRange,
  point: TimelineExactPoint,
): boolean {
  requireSameRate(rangeValue.start.timebase, point.timebase, "exact range query");
  const start = integer(rangeValue.start.value, "exact range start");
  const end = start + integer(rangeValue.duration.value, "exact range duration");
  const target = integer(point.value, "exact range point");
  return target >= start && target < end;
}

function subtractPoints(
  value: TimelineExactPoint,
  start: TimelineExactPoint,
): TimelineExactPoint {
  requireSameRate(value.timebase, start.timebase, "exact point subtraction");
  return {
    value: (
      integer(value.value, "exact point") - integer(start.value, "exact point start")
    ).toString(),
    timebase: value.timebase,
  };
}

function addPoints(
  value: TimelineExactPoint,
  offset: TimelineExactPoint,
): TimelineExactPoint {
  requireSameRate(value.timebase, offset.timebase, "exact point addition");
  return {
    value: (
      integer(value.value, "exact point") + integer(offset.value, "exact offset")
    ).toString(),
    timebase: value.timebase,
  };
}

function pointSeconds(value: TimelineExactPoint): number {
  return (
    (Number(value.value) * value.timebase.denominator) /
    value.timebase.numerator
  );
}

function publicExactTime(value: TimelineExactPoint): ExactTime {
  return {
    value: safeInteger(integer(value.value, "public exact time"), "public exact time"),
    timebase: value.timebase,
  };
}

function safeInteger(value: bigint, label: string): number {
  const parsed = Number(value);
  if (!Number.isSafeInteger(parsed)) {
    throw new TimelineMulticamError(`${label} exceeds the public safe integer range.`);
  }
  return parsed;
}

function requireSameRate(left: TimelineRate, right: TimelineRate, label: string) {
  if (
    left.numerator !== right.numerator ||
    left.denominator !== right.denominator
  ) {
    throw new TimelineMulticamError(`${label} uses incompatible exact clocks.`);
  }
}

function integer(value: unknown, label: string): bigint {
  if (typeof value !== "string" || !/^(?:0|-[1-9][0-9]*|[1-9][0-9]*)$/.test(value)) {
    throw new TimelineMulticamError(`${label} is not a canonical signed integer.`);
  }
  return BigInt(value);
}

function record(value: unknown, label: string): JsonRecord {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new TimelineMulticamError(`${label} is not an object.`);
  }
  return value as JsonRecord;
}

function array(value: unknown, label: string): readonly unknown[] {
  if (!Array.isArray(value)) {
    throw new TimelineMulticamError(`${label} is not an array.`);
  }
  return value;
}

function nonemptyString(value: unknown, label: string): string {
  if (typeof value !== "string" || value.length === 0) {
    throw new TimelineMulticamError(`${label} is empty or invalid.`);
  }
  return value;
}

function boolean(value: unknown, label: string): boolean {
  if (typeof value !== "boolean") {
    throw new TimelineMulticamError(`${label} is invalid.`);
  }
  return value;
}

function isTypedId(value: string, kind: string): boolean {
  return new RegExp(`^${kind}:[0-9a-f]{32}$`).test(value);
}

function freezeProjection<T extends TimelineMulticamSetup | TimelineMulticamReady>(
  projection: T,
): T {
  if (projection.status === "setup") {
    Object.freeze(projection.angles);
  } else {
    Object.freeze(projection.angles);
    Object.freeze(projection.switches);
    Object.freeze(projection.cuts);
  }
  return Object.freeze(projection);
}
