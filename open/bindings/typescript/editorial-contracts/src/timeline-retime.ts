import type {
  EditorClipTimeMap,
  EditorRetimeSegment,
  ExecuteProjectCommand,
  ExactTimebase,
  TimelineEditOperation,
} from "./api.ts";
import type {
  TimelineClipPresentation,
  TimelineClipTimeMap,
} from "./timeline-clip-presentation.ts";

const SIGNED_DECIMAL = /^(?:0|-[1-9][0-9]*|[1-9][0-9]*)$/;
const UNSIGNED_DECIMAL = /^(?:0|[1-9][0-9]*)$/;
const MAX_SAFE = BigInt(Number.MAX_SAFE_INTEGER);
const MIN_SAFE = -MAX_SAFE;

export type TimelineRetimeMode =
  | "identity"
  | "speed"
  | "reverse"
  | "freeze"
  | "time_remap";

export interface TimelineRetimeDraftSegment {
  readonly recordDuration: string;
  readonly rateNumerator: string;
  readonly rateDenominator: string;
}

export interface TimelineRetimeDraft {
  readonly sourceStart: string;
  readonly mode: TimelineRetimeMode;
  readonly segments: readonly TimelineRetimeDraftSegment[];
}

export type TimelineRetimeClip = Pick<
  TimelineClipPresentation,
  | "id"
  | "name"
  | "timelineId"
  | "trackId"
  | "trackName"
  | "recordRange"
  | "sourceRange"
  | "timeMap"
  | "startSeconds"
  | "endSeconds"
>;

export interface TimelineRetimeCurvePoint {
  readonly recordOffset: string;
  readonly sourceValue: string;
  readonly xPercent: number;
  readonly yPercent: number;
}

export interface TimelineRetimeTarget {
  readonly timelineId: string;
  readonly trackId: string;
  readonly trackName: string;
  readonly clipId: string;
  readonly clipName: string;
}

interface TimelineRetimePlanInput {
  readonly clip: TimelineRetimeClip;
  readonly draft: TimelineRetimeDraft;
  readonly projectRevision: number;
  readonly transactionId: string;
}

interface TimelineRetimePlanBase {
  readonly currentMode: TimelineRetimeMode;
  readonly target: TimelineRetimeTarget;
}

export interface TimelineRetimeReadyPlan extends TimelineRetimePlanBase {
  readonly status: "ready";
  readonly mode: TimelineRetimeMode;
  readonly consequence: string;
  readonly sourceTraversal: {
    readonly start: string;
    readonly end: string;
  };
  readonly curvePoints: readonly TimelineRetimeCurvePoint[];
  readonly timeMap: EditorClipTimeMap;
  readonly operation: TimelineEditOperation;
  readonly request: ExecuteProjectCommand;
}

export interface TimelineRetimeDisabledPlan extends TimelineRetimePlanBase {
  readonly status: "disabled";
  readonly reason: string;
}

export type TimelineRetimePlan =
  | TimelineRetimeReadyPlan
  | TimelineRetimeDisabledPlan;

interface NormalizedRate {
  readonly numerator: bigint;
  readonly denominator: bigint;
}

interface BuiltMap {
  readonly timeMap: EditorClipTimeMap;
  readonly sourceValues: readonly bigint[];
  readonly recordOffsets: readonly bigint[];
}

export function timelineRetimeMode(
  timeMap: TimelineClipTimeMap,
): TimelineRetimeMode {
  if (timeMap.segments.length !== 1) return "time_remap";
  const segment = timeMap.segments[0];
  if (segment === undefined) return "time_remap";
  const rate = reduceRate(
    parseSigned(segment.rateNumerator, "current rate numerator"),
    parsePositive(segment.rateDenominator, "current rate denominator"),
  );
  if (rate.numerator === 0n) return "freeze";
  if (rate.numerator < 0n) return "reverse";
  if (rate.numerator === rate.denominator) return "identity";
  return "speed";
}

export function timelineRetimeDraftFromClip(
  clip: TimelineRetimeClip,
): TimelineRetimeDraft {
  const first = clip.timeMap.segments[0];
  if (first === undefined) {
    throw new Error("Current clip time map has no segments.");
  }
  return freezeDraft({
    sourceStart: first.sourceStart.value,
    mode: timelineRetimeMode(clip.timeMap),
    segments: clip.timeMap.segments.map((segment) => ({
      recordDuration: segment.recordRange.duration.value,
      rateNumerator: segment.rateNumerator,
      rateDenominator: segment.rateDenominator,
    })),
  });
}

export function timelineRetimeDraftForMode(
  clip: TimelineRetimeClip,
  current: TimelineRetimeDraft,
  mode: TimelineRetimeMode,
  splitOffset?: string,
): TimelineRetimeDraft {
  const duration = clip.timeMap.recordDuration.value;
  const currentMode = current.mode;
  const firstRate = current.segments[0] ?? {
    recordDuration: duration,
    rateNumerator: "1",
    rateDenominator: "1",
  };

  switch (mode) {
    case "identity":
      return freezeDraft({
        sourceStart: clip.sourceRange.start.value,
        mode,
        segments: [segment(duration, "1", "1")],
      });
    case "speed": {
      let numerator = "2";
      let denominator = "1";
      if (currentMode === "speed") {
        numerator = firstRate.rateNumerator;
        denominator = firstRate.rateDenominator;
      } else if (currentMode === "reverse") {
        numerator = absoluteDecimal(firstRate.rateNumerator);
        denominator = firstRate.rateDenominator;
      }
      return freezeDraft({
        sourceStart: current.sourceStart,
        mode,
        segments: [segment(duration, numerator, denominator)],
      });
    }
    case "reverse": {
      let numerator = "-1";
      let denominator = "1";
      let sourceStart = sourceRangeLastSample(clip);
      if (currentMode === "reverse") {
        numerator = firstRate.rateNumerator;
        denominator = firstRate.rateDenominator;
        sourceStart = current.sourceStart;
      } else if (currentMode === "speed") {
        numerator = negatePositiveDecimal(firstRate.rateNumerator);
        denominator = firstRate.rateDenominator;
      }
      return freezeDraft({
        sourceStart,
        mode,
        segments: [segment(duration, numerator, denominator)],
      });
    }
    case "freeze":
      return freezeDraft({
        sourceStart: current.sourceStart,
        mode,
        segments: [segment(duration, "0", "1")],
      });
    case "time_remap": {
      if (current.segments.length > 1) {
        return freezeDraft({ ...current, mode });
      }
      const total = parsePositive(duration, "record duration");
      const offset =
        splitOffset ??
        (() => {
          const midpoint = total / 2n;
          if (midpoint <= 0n || midpoint >= total) {
            throw new Error(
              "Time remap requires a clip with an interior record point.",
            );
          }
          return midpoint.toString();
        })();
      return freezeDraft({
        ...splitTimelineRetimeDraft(current, offset),
        mode,
      });
    }
  }
}

export function splitTimelineRetimeDraft(
  draft: TimelineRetimeDraft,
  recordOffset: string,
): TimelineRetimeDraft {
  const offset = parseUnsigned(recordOffset, "retime point");
  const durations = draft.segments.map((value, index) =>
    parsePositive(value.recordDuration, `segment ${index + 1} duration`),
  );
  const total = durations.reduce((sum, value) => sum + value, 0n);
  if (offset <= 0n || offset >= total) {
    throw new Error("Retime point must be an interior record offset.");
  }

  let cursor = 0n;
  for (let index = 0; index < durations.length; index += 1) {
    const duration = durations[index]!;
    const end = cursor + duration;
    if (offset === cursor || offset === end) {
      throw new Error("Retime point already exists at that record boundary.");
    }
    if (offset > cursor && offset < end) {
      const original = draft.segments[index]!;
      const left = offset - cursor;
      const right = end - offset;
      return freezeDraft({
        ...draft,
        mode: "time_remap",
        segments: [
          ...draft.segments.slice(0, index),
          segment(
            left.toString(),
            original.rateNumerator,
            original.rateDenominator,
          ),
          segment(
            right.toString(),
            original.rateNumerator,
            original.rateDenominator,
          ),
          ...draft.segments.slice(index + 1),
        ],
      });
    }
    cursor = end;
  }
  throw new Error("Retime point does not fall inside the clip.");
}

export function removeTimelineRetimePoint(
  draft: TimelineRetimeDraft,
  boundaryIndex: number,
): TimelineRetimeDraft {
  if (
    !Number.isSafeInteger(boundaryIndex) ||
    boundaryIndex <= 0 ||
    boundaryIndex >= draft.segments.length
  ) {
    throw new Error("Retime point must identify an existing interior boundary.");
  }
  const previous = draft.segments[boundaryIndex - 1]!;
  const next = draft.segments[boundaryIndex]!;
  const duration =
    parsePositive(previous.recordDuration, "preceding segment duration") +
    parsePositive(next.recordDuration, "following segment duration");
  const segments = [
    ...draft.segments.slice(0, boundaryIndex - 1),
    segment(
      duration.toString(),
      previous.rateNumerator,
      previous.rateDenominator,
    ),
    ...draft.segments.slice(boundaryIndex + 1),
  ];
  return freezeDraft({
    ...draft,
    mode:
      segments.length > 1
        ? "time_remap"
        : modeForDraftRate(
            segments[0]!.rateNumerator,
            segments[0]!.rateDenominator,
          ),
    segments,
  });
}

export function planTimelineRetime(
  input: TimelineRetimePlanInput,
): TimelineRetimePlan {
  const target = Object.freeze({
    timelineId: input.clip.timelineId,
    trackId: input.clip.trackId,
    trackName: input.clip.trackName,
    clipId: input.clip.id,
    clipName: input.clip.name,
  });
  let currentMode: TimelineRetimeMode;
  try {
    currentMode = timelineRetimeMode(input.clip.timeMap);
  } catch {
    return Object.freeze({
      status: "disabled",
      currentMode: "time_remap",
      target,
      reason: "Current retime state is unavailable.",
    });
  }

  try {
    if (
      !Number.isSafeInteger(input.projectRevision) ||
      input.projectRevision < 0
    ) {
      throw new Error("Project revision must be a nonnegative safe integer.");
    }
    if (input.transactionId.trim().length === 0) {
      throw new Error("Retime transaction identity is required.");
    }
    const built = buildTimeMap(input.clip, input.draft);
    if (
      timeMapKey(built.timeMap) ===
      presentationTimeMapKey(input.clip.timeMap)
    ) {
      return Object.freeze({
        status: "disabled",
        currentMode,
        target,
        reason: "The proposed retime already matches the clip's authored map.",
      });
    }

    const operation: TimelineEditOperation = {
      operation: "retime",
      timeline_id: input.clip.timelineId,
      track_id: input.clip.trackId,
      clip_id: input.clip.id,
      time_map: built.timeMap,
    };
    const request: ExecuteProjectCommand = {
      transaction_id: input.transactionId,
      expected_project_revision: input.projectRevision,
      command: {
        command: "apply",
        actions: [
          {
            action: "edit_timeline",
            operations: [operation],
          },
        ],
      },
    };
    const curvePoints = curvePointsFor(built);
    const sourceStart = built.sourceValues[0]!.toString();
    const sourceEnd = built.sourceValues.at(-1)!.toString();
    const consequence =
      `${modeLabel(input.draft.mode)} ${input.clip.name} on ${input.clip.trackName} ` +
      `(${input.clip.id} on ${input.clip.trackId}). Retains the exact ` +
      `${input.clip.timeMap.recordDuration.value}-unit record duration while traversing ` +
      `source ${sourceStart} to ${sourceEnd} across ${input.draft.segments.length} ` +
      `${input.draft.segments.length === 1 ? "segment" : "segments"} in one undoable edit.`;

    return Object.freeze({
      status: "ready",
      currentMode,
      target,
      mode: input.draft.mode,
      consequence,
      sourceTraversal: Object.freeze({ start: sourceStart, end: sourceEnd }),
      curvePoints,
      timeMap: built.timeMap,
      operation,
      request,
    });
  } catch (error) {
    return Object.freeze({
      status: "disabled",
      currentMode,
      target,
      reason:
        error instanceof Error ? error.message : "The retime draft is invalid.",
    });
  }
}

export function timelineRetimePlayheadOffset(
  clip: TimelineRetimeClip,
  playheadSeconds: number,
): string | null {
  if (
    !Number.isFinite(playheadSeconds) ||
    playheadSeconds <= clip.startSeconds ||
    playheadSeconds >= clip.endSeconds
  ) {
    return null;
  }
  const duration = parsePositive(
    clip.timeMap.recordDuration.value,
    "record duration",
  );
  requireSafeUnsigned(duration, "record duration");
  const ratio =
    (playheadSeconds - clip.startSeconds) /
    (clip.endSeconds - clip.startSeconds);
  const offset = BigInt(Math.round(Number(duration) * ratio));
  return offset > 0n && offset < duration ? offset.toString() : null;
}

function buildTimeMap(
  clip: TimelineRetimeClip,
  draft: TimelineRetimeDraft,
): BuiltMap {
  if (draft.segments.length === 0) {
    throw new Error("Retime map must contain at least one segment.");
  }
  const recordRate = validateTimebase(
    clip.timeMap.recordDuration.timebase,
    "record timebase",
  );
  const sourceRate = validateTimebase(
    clip.timeMap.sourceTimebase,
    "source timebase",
  );
  const recordDuration = parsePositive(
    clip.timeMap.recordDuration.value,
    "clip record duration",
  );
  requireSafeUnsigned(recordDuration, "clip record duration");
  const sourceStart = parseSigned(draft.sourceStart, "source anchor");
  requireSafeSigned(sourceStart, "source anchor");

  let recordCursor = 0n;
  let sourceCursor = sourceStart;
  const recordOffsets = [0n];
  const sourceValues = [sourceStart];
  const segments: EditorRetimeSegment[] = [];

  for (let index = 0; index < draft.segments.length; index += 1) {
    const value = draft.segments[index]!;
    const duration = parsePositive(
      value.recordDuration,
      `segment ${index + 1} record duration`,
    );
    requireSafeUnsigned(
      duration,
      `segment ${index + 1} record duration`,
    );
    const rate = reduceRate(
      parseSigned(
        value.rateNumerator,
        `segment ${index + 1} rate numerator`,
      ),
      parsePositive(
        value.rateDenominator,
        `segment ${index + 1} rate denominator`,
      ),
    );
    requireSafeSigned(
      rate.numerator,
      `segment ${index + 1} rate numerator`,
    );
    requireSafeUnsigned(
      rate.denominator,
      `segment ${index + 1} rate denominator`,
    );

    const numerator =
      duration *
      BigInt(recordRate.denominator) *
      rate.numerator *
      BigInt(sourceRate.numerator);
    const denominator =
      BigInt(recordRate.numerator) *
      rate.denominator *
      BigInt(sourceRate.denominator);
    if (numerator % denominator !== 0n) {
      throw new Error(
        `Segment ${index + 1} cannot produce an exact source seam in these clocks.`,
      );
    }

    segments.push({
      record_range: {
        start: {
          value: toSafeSignedNumber(recordCursor, "segment record start"),
          timebase: recordRate,
        },
        duration: {
          value: toSafeUnsignedNumber(duration, "segment record duration"),
          timebase: recordRate,
        },
      },
      source_start: {
        value: toSafeSignedNumber(sourceCursor, "segment source start"),
        timebase: sourceRate,
      },
      rate_numerator: toSafeSignedNumber(
        rate.numerator,
        "segment rate numerator",
      ),
      rate_denominator: toSafeUnsignedNumber(
        rate.denominator,
        "segment rate denominator",
      ),
    });

    recordCursor += duration;
    sourceCursor += numerator / denominator;
    requireSafeSigned(
      sourceCursor,
      `segment ${index + 1} source end`,
    );
    recordOffsets.push(recordCursor);
    sourceValues.push(sourceCursor);
  }

  if (recordCursor !== recordDuration) {
    throw new Error(
      `Segment record durations must sum exactly to ${recordDuration} units.`,
    );
  }

  return {
    timeMap: {
      record_duration: {
        value: toSafeUnsignedNumber(recordDuration, "clip record duration"),
        timebase: recordRate,
      },
      source_timebase: sourceRate,
      segments,
    },
    sourceValues,
    recordOffsets,
  };
}

function curvePointsFor(built: BuiltMap): readonly TimelineRetimeCurvePoint[] {
  const total = built.recordOffsets.at(-1)!;
  const minSource = built.sourceValues.reduce(
    (minimum, value) => (value < minimum ? value : minimum),
    built.sourceValues[0]!,
  );
  const maxSource = built.sourceValues.reduce(
    (maximum, value) => (value > maximum ? value : maximum),
    built.sourceValues[0]!,
  );
  const sourceSpan = maxSource - minSource;
  return Object.freeze(
    built.recordOffsets.map((offset, index) =>
      Object.freeze({
        recordOffset: offset.toString(),
        sourceValue: built.sourceValues[index]!.toString(),
        xPercent: percent(offset, total),
        yPercent:
          sourceSpan === 0n
            ? 50
            : roundPercent(
                100 -
                  (Number(built.sourceValues[index]! - minSource) /
                    Number(sourceSpan)) *
                    100,
              ),
      }),
    ),
  );
}

function timeMapKey(timeMap: EditorClipTimeMap): string {
  return JSON.stringify({
    recordDuration: [
      timeMap.record_duration.value.toString(),
      rateKey(timeMap.record_duration.timebase),
    ],
    sourceTimebase: rateKey(timeMap.source_timebase),
    segments: timeMap.segments.map((value) => ({
      recordStart: value.record_range.start.value.toString(),
      recordDuration: value.record_range.duration.value.toString(),
      sourceStart: value.source_start.value.toString(),
      rate: [
        value.rate_numerator.toString(),
        value.rate_denominator.toString(),
      ],
    })),
  });
}

function presentationTimeMapKey(timeMap: TimelineClipTimeMap): string {
  return JSON.stringify({
    recordDuration: [
      parsePositive(
        timeMap.recordDuration.value,
        "current record duration",
      ).toString(),
      rateKey(timeMap.recordDuration.timebase),
    ],
    sourceTimebase: rateKey(timeMap.sourceTimebase),
    segments: timeMap.segments.map((value, index) => {
      const rate = reduceRate(
        parseSigned(
          value.rateNumerator,
          `current segment ${index + 1} numerator`,
        ),
        parsePositive(
          value.rateDenominator,
          `current segment ${index + 1} denominator`,
        ),
      );
      return {
        recordStart: parseSigned(
          value.recordRange.start.value,
          `current segment ${index + 1} record start`,
        ).toString(),
        recordDuration: parsePositive(
          value.recordRange.duration.value,
          `current segment ${index + 1} duration`,
        ).toString(),
        sourceStart: parseSigned(
          value.sourceStart.value,
          `current segment ${index + 1} source start`,
        ).toString(),
        rate: [rate.numerator.toString(), rate.denominator.toString()],
      };
    }),
  });
}

function validateTimebase(
  value: ExactTimebase,
  label: string,
): ExactTimebase {
  if (
    !Number.isSafeInteger(value.numerator) ||
    value.numerator <= 0 ||
    !Number.isSafeInteger(value.denominator) ||
    value.denominator <= 0
  ) {
    throw new Error(`${label} must use positive safe integer terms.`);
  }
  return Object.freeze({
    numerator: value.numerator,
    denominator: value.denominator,
  });
}

function parseSigned(value: string, label: string): bigint {
  if (!SIGNED_DECIMAL.test(value)) {
    throw new Error(`${label} must be a canonical signed decimal.`);
  }
  return BigInt(value);
}

function parsePositive(value: string, label: string): bigint {
  if (!UNSIGNED_DECIMAL.test(value) || BigInt(value) <= 0n) {
    throw new Error(`${label} must be a positive canonical decimal.`);
  }
  return BigInt(value);
}

function parseUnsigned(value: string, label: string): bigint {
  if (!UNSIGNED_DECIMAL.test(value)) {
    throw new Error(`${label} must be a canonical unsigned decimal.`);
  }
  return BigInt(value);
}

function reduceRate(numerator: bigint, denominator: bigint): NormalizedRate {
  if (numerator === 0n) {
    return { numerator: 0n, denominator: 1n };
  }
  const divisor = greatestCommonDivisor(absolute(numerator), denominator);
  return {
    numerator: numerator / divisor,
    denominator: denominator / divisor,
  };
}

function greatestCommonDivisor(left: bigint, right: bigint): bigint {
  let a = left;
  let b = right;
  while (b !== 0n) {
    const remainder = a % b;
    a = b;
    b = remainder;
  }
  return a;
}

function requireSafeSigned(value: bigint, label: string): void {
  if (value < MIN_SAFE || value > MAX_SAFE) {
    throw new Error(
      `${label} must fit the public safe integer wire range.`,
    );
  }
}

function requireSafeUnsigned(value: bigint, label: string): void {
  if (value < 0n || value > MAX_SAFE) {
    throw new Error(
      `${label} must fit the public safe integer wire range.`,
    );
  }
}

function toSafeSignedNumber(value: bigint, label: string): number {
  requireSafeSigned(value, label);
  return Number(value);
}

function toSafeUnsignedNumber(value: bigint, label: string): number {
  requireSafeUnsigned(value, label);
  return Number(value);
}

function segment(
  recordDuration: string,
  rateNumerator: string,
  rateDenominator: string,
): TimelineRetimeDraftSegment {
  return Object.freeze({
    recordDuration,
    rateNumerator,
    rateDenominator,
  });
}

function freezeDraft(value: TimelineRetimeDraft): TimelineRetimeDraft {
  return Object.freeze({
    sourceStart: value.sourceStart,
    mode: value.mode,
    segments: Object.freeze(
      value.segments.map((entry) => Object.freeze({ ...entry })),
    ),
  });
}

function sourceRangeLastSample(clip: TimelineRetimeClip): string {
  if (
    rateKey(clip.sourceRange.start.timebase) !==
    rateKey(clip.sourceRange.duration.timebase)
  ) {
    throw new Error("Source range clocks must match before reversing.");
  }
  return (
    parseSigned(clip.sourceRange.start.value, "source range start") +
    parsePositive(clip.sourceRange.duration.value, "source range duration") -
    1n
  ).toString();
}

function absoluteDecimal(value: string): string {
  return absolute(parseSigned(value, "playback rate numerator")).toString();
}

function negatePositiveDecimal(value: string): string {
  const parsed = parseSigned(value, "playback rate numerator");
  return (-absolute(parsed === 0n ? 1n : parsed)).toString();
}

function absolute(value: bigint): bigint {
  return value < 0n ? -value : value;
}

function modeForDraftRate(
  numeratorValue: string,
  denominatorValue: string,
): TimelineRetimeMode {
  const rate = reduceRate(
    parseSigned(numeratorValue, "rate numerator"),
    parsePositive(denominatorValue, "rate denominator"),
  );
  if (rate.numerator === 0n) return "freeze";
  if (rate.numerator < 0n) return "reverse";
  if (rate.numerator === rate.denominator) return "identity";
  return "speed";
}

function modeLabel(mode: TimelineRetimeMode): string {
  switch (mode) {
    case "identity":
      return "Reset timing for";
    case "speed":
      return "Change speed for";
    case "reverse":
      return "Reverse";
    case "freeze":
      return "Freeze";
    case "time_remap":
      return "Apply a time-remap curve to";
  }
}

function rateKey(value: ExactTimebase): string {
  return `${value.numerator}/${value.denominator}`;
}

function percent(value: bigint, total: bigint): number {
  return roundPercent((Number(value) / Number(total)) * 100);
}

function roundPercent(value: number): number {
  return Math.round(value * 1_000) / 1_000;
}
