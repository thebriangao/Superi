import type {
  EditorCaptionPurpose,
  ExactTimeRange,
  ProjectAction,
  TimelineCaptionMutation,
  TimelineCaptionRelationship,
  TimelineCaptionStyle,
  TimelineEditOperation,
} from "./api.ts";
import type { MediaTranscriptSegment } from "./project-lifecycle.ts";
import type { TimelineCanvasTrack } from "./timeline-workspace.ts";

export type CaptionExchangeFormat = "srt" | "vtt";

export interface CaptionCue {
  readonly id: string;
  readonly name: string;
  readonly text: string;
  readonly startMilliseconds: number;
  readonly endMilliseconds: number;
  readonly language: string;
  readonly speaker: string | null;
  readonly style: TimelineCaptionStyle | null;
  readonly timelineRelationships: readonly TimelineCaptionRelationship[];
}

export interface CaptionImportPlan {
  readonly actions: readonly ProjectAction[];
  readonly captionIds: readonly string[];
}

export function captionStyleFromFields({
  fontFamily,
  fontSize,
  foreground,
  background,
  bold,
  italic,
  alignment,
  position,
}: {
  readonly fontFamily: string;
  readonly fontSize: string;
  readonly foreground: string;
  readonly background: string;
  readonly bold: boolean;
  readonly italic: boolean;
  readonly alignment: TimelineCaptionStyle["alignment"];
  readonly position: TimelineCaptionStyle["position"];
}): TimelineCaptionStyle {
  const parsedFontSize = fontSize.trim().length === 0 ? null : Number(fontSize);
  if (
    parsedFontSize !== null &&
    (!Number.isInteger(parsedFontSize) || parsedFontSize < 8 || parsedFontSize > 256)
  ) {
    throw new Error("Caption font size must be a whole number between 8 and 256 points.");
  }
  return {
    font_family: optionalText(fontFamily),
    font_size: parsedFontSize,
    foreground: optionalColor(foreground),
    background: optionalColor(background),
    bold,
    italic,
    alignment,
    position,
  };
}

const MAX_EXCHANGE_BYTES = 2 * 1024 * 1024;
const MAX_CUES = 5_000;
const MAX_CUE_TEXT_BYTES = 32 * 1024;
const MAX_SPEAKER_BYTES = 512;
const MILLISECOND_TIMEBASE = Object.freeze({ numerator: 1_000, denominator: 1 });
const CANONICAL_ID = /^(?:timeline|track|clip|gap|caption):[0-9a-f]{32}$/;

export function parseCaptionExchange(
  source: string,
  format: CaptionExchangeFormat,
  language: string,
): readonly CaptionCue[] {
  validateLanguage(language);
  if (new TextEncoder().encode(source).byteLength > MAX_EXCHANGE_BYTES) {
    throw new Error("Caption exchange source exceeds the 2 MiB import limit.");
  }
  const normalized = source.replace(/^\uFEFF/, "").replaceAll("\r\n", "\n").replaceAll("\r", "\n");
  const cues = format === "srt"
    ? parseSrt(normalized, language)
    : parseWebVtt(normalized, language);
  validateCues(cues);
  return Object.freeze(cues.map(freezeCue));
}

export function serializeCaptionExchange(
  cues: readonly CaptionCue[],
  format: CaptionExchangeFormat,
): string {
  validateCues(cues);
  if (format === "srt") {
    return `${cues
      .map(
        (cue, index) =>
          `${index + 1}\n${formatTimestamp(cue.startMilliseconds, ",")} --> ${formatTimestamp(cue.endMilliseconds, ",")}\n${cue.text}`,
      )
      .join("\n\n")}\n`;
  }
  const blocks = cues.map((cue) => {
    const settings: string[] = [];
    if (cue.style !== null) {
      if (
        cue.style.font_family !== null ||
        cue.style.font_size !== null ||
        cue.style.foreground !== null ||
        cue.style.background !== null
      ) {
        throw new Error(
          "The bounded WebVTT exporter cannot represent font or color styling without a STYLE block.",
        );
      }
      settings.push(`align:${cue.style.alignment}`);
      settings.push(`line:${cue.style.position === "top" ? "0%" : "100%"}`);
    }
    let payload = encodeWebVttText(cue.text);
    if (cue.style?.italic) payload = `<i>${payload}</i>`;
    if (cue.style?.bold) payload = `<b>${payload}</b>`;
    if (cue.speaker !== null) {
      validateVttAnnotation(cue.speaker, "speaker");
      payload = `<v ${cue.speaker}>${payload}</v>`;
    }
    const timing = `${formatTimestamp(cue.startMilliseconds, ".")} --> ${formatTimestamp(cue.endMilliseconds, ".")}${settings.length > 0 ? ` ${settings.join(" ")}` : ""}`;
    return `${cue.id}\n${timing}\n${payload}`;
  });
  return `WEBVTT\n\n${blocks.join("\n\n")}\n`;
}

export function buildCaptionImportActions({
  timelineId,
  trackId,
  trackName,
  trackPosition,
  language,
  purpose,
  cues,
  createId,
}: {
  readonly timelineId: string;
  readonly trackId: string;
  readonly trackName: string;
  readonly trackPosition: number;
  readonly language: string;
  readonly purpose: EditorCaptionPurpose;
  readonly cues: readonly CaptionCue[];
  readonly createId: (kind: "gap" | "caption") => string;
}): CaptionImportPlan {
  validateCanonicalId(timelineId, "timeline");
  validateCanonicalId(trackId, "track");
  validateLanguage(language);
  if (trackName.trim().length === 0 || trackName.length > 512) {
    throw new Error("Caption track name must be bounded visible text.");
  }
  if (!Number.isSafeInteger(trackPosition) || trackPosition < 0) {
    throw new Error("Caption track position must be a nonnegative safe integer.");
  }
  validateCues(cues);
  const operations: TimelineEditOperation[] = [];
  const mutations: TimelineCaptionMutation[] = [];
  const captionIds: string[] = [];
  let cursor = 0;
  for (const cue of cues) {
    if (cue.startMilliseconds > cursor) {
      const gapId = createId("gap");
      validateCanonicalId(gapId, "gap");
      operations.push({
        operation: "append",
        timeline_id: timelineId,
        track_id: trackId,
        material: {
          kind: "gap",
          id: gapId,
          name: "Caption gap",
          record_range: exactMillisecondRange(
            cursor,
            cue.startMilliseconds - cursor,
          ),
        },
      });
    }
    const captionId = createId("caption");
    validateCanonicalId(captionId, "caption");
    captionIds.push(captionId);
    operations.push({
      operation: "append",
      timeline_id: timelineId,
      track_id: trackId,
      material: {
        kind: "caption",
        id: captionId,
        name: cue.name,
        text: cue.text,
        language: cue.language,
        record_range: exactMillisecondRange(
          cue.startMilliseconds,
          cue.endMilliseconds - cue.startMilliseconds,
        ),
      },
    });
    if (cue.speaker !== null) {
      mutations.push({
        operation: "set_speaker",
        timeline_id: timelineId,
        caption_id: captionId,
        speaker: cue.speaker,
      });
    }
    if (cue.style !== null) {
      mutations.push({
        operation: "set_style",
        timeline_id: timelineId,
        caption_id: captionId,
        style: cue.style,
      });
    }
    if (cue.timelineRelationships.length > 0) {
      mutations.push({
        operation: "set_timeline_relationships",
        timeline_id: timelineId,
        caption_id: captionId,
        relationships: cue.timelineRelationships.slice(),
      });
    }
    cursor = cue.endMilliseconds;
  }

  const actions: ProjectAction[] = [
    {
      action: "mutate_tracks",
      mutations: [
        {
          operation: "create",
          timeline_id: timelineId,
          track_id: trackId,
          name: trackName,
          kind: "caption",
          position: trackPosition,
          height: 72,
        },
        {
          operation: "set_caption_semantics",
          timeline_id: timelineId,
          track_id: trackId,
          language,
          purpose,
        },
      ],
    },
    { action: "edit_timeline", operations },
  ];
  if (mutations.length > 0) {
    actions.push({ action: "mutate_captions", mutations });
  }
  return Object.freeze({
    actions: Object.freeze(actions),
    captionIds: Object.freeze(captionIds),
  });
}

export function captionCuesFromTranscript({
  expectedProjectRevision,
  projectRevision,
  currentSourceFingerprint,
  analysisSourceFingerprint,
  language,
  segments,
}: {
  readonly expectedProjectRevision: number;
  readonly projectRevision: number;
  readonly currentSourceFingerprint: string;
  readonly analysisSourceFingerprint: string;
  readonly language: string;
  readonly segments: readonly MediaTranscriptSegment[];
}): readonly CaptionCue[] {
  if (
    !Number.isSafeInteger(expectedProjectRevision) ||
    !Number.isSafeInteger(projectRevision) ||
    expectedProjectRevision !== projectRevision
  ) {
    throw new Error("Transcript analysis project revision is stale.");
  }
  if (
    currentSourceFingerprint.length === 0 ||
    analysisSourceFingerprint !== currentSourceFingerprint
  ) {
    throw new Error("Transcript analysis is stale for the current media source.");
  }
  validateLanguage(language);
  const cues = segments.map((segment, index): CaptionCue => {
    if (
      !Number.isSafeInteger(segment.start_frame) ||
      !Number.isSafeInteger(segment.end_frame) ||
      segment.start_frame < 0 ||
      segment.end_frame <= segment.start_frame ||
      !Number.isSafeInteger(segment.rate_numerator) ||
      !Number.isSafeInteger(segment.rate_denominator) ||
      segment.rate_numerator <= 0 ||
      segment.rate_denominator <= 0
    ) {
      throw new Error(`Transcript segment ${index + 1} has invalid exact timing.`);
    }
    return {
      id: segment.segment_id,
      name: `Caption ${index + 1}`,
      text: segment.text,
      startMilliseconds: frameToMilliseconds(
        segment.start_frame,
        segment.rate_numerator,
        segment.rate_denominator,
      ),
      endMilliseconds: frameToMilliseconds(
        segment.end_frame,
        segment.rate_numerator,
        segment.rate_denominator,
      ),
      language,
      speaker: segment.speaker,
      style: null,
      timelineRelationships: segment.timeline_relationships.map(
        (relationship) => ({
          timeline_id: relationship.timeline_id,
          clip_id: relationship.clip_id,
        }),
      ),
    };
  });
  validateCues(cues);
  return Object.freeze(cues.map(freezeCue));
}

export function captionCuesFromTrack(
  track: TimelineCanvasTrack,
): readonly CaptionCue[] {
  if (track.kind !== "caption") {
    throw new Error("Caption export requires a caption track.");
  }
  const cues = track.items
    .filter((item) => item.kind === "caption" && item.caption !== null)
    .map((item, index): CaptionCue => {
      const start = rationalUnitsToMilliseconds(
        item.recordRange.start.value,
        item.recordRange.start.timebase.numerator,
        item.recordRange.start.timebase.denominator,
      );
      const duration = rationalUnitsToMilliseconds(
        item.recordRange.duration.value,
        item.recordRange.duration.timebase.numerator,
        item.recordRange.duration.timebase.denominator,
      );
      const caption = item.caption!;
      return {
        id: item.id,
        name: item.name || `Caption ${index + 1}`,
        text: caption.text,
        startMilliseconds: start,
        endMilliseconds: start + duration,
        language: caption.language ?? track.captionLanguage ?? "und",
        speaker: caption.speaker,
        style: caption.style,
        timelineRelationships: caption.timelineRelationships.slice(),
      };
    });
  validateCues(cues);
  return Object.freeze(cues.map(freezeCue));
}

function parseSrt(source: string, language: string): CaptionCue[] {
  const blocks = source.trim().split(/\n{2,}/);
  return blocks.map((block, index) => {
    const lines = block.split("\n");
    const timingIndex = lines[0]?.includes("-->") ? 0 : 1;
    const timing = lines[timingIndex];
    if (timing === undefined) {
      throw new Error(`SRT cue ${index + 1} has no timing line.`);
    }
    const match = timing.match(
      /^(\d{2,}):(\d{2}):(\d{2}),(\d{3})[ \t]+-->[ \t]+(\d{2,}):(\d{2}):(\d{2}),(\d{3})$/,
    );
    if (match === null) {
      throw new Error(`SRT cue ${index + 1} has invalid timing syntax.`);
    }
    const text = lines.slice(timingIndex + 1).join("\n");
    return cueFromParsedBlock({
      id: timingIndex === 1 ? lines[0]! : `cue-${index + 1}`,
      index,
      text,
      startMilliseconds: timestampMatchMilliseconds(match, 1),
      endMilliseconds: timestampMatchMilliseconds(match, 5),
      language,
      speaker: null,
      style: null,
    });
  });
}

function parseWebVtt(source: string, language: string): CaptionCue[] {
  const lines = source.split("\n");
  if (lines[0]?.trim() !== "WEBVTT") {
    throw new Error("WebVTT source must begin with the WEBVTT signature.");
  }
  let bodyStart = 1;
  while (bodyStart < lines.length && lines[bodyStart] !== "") bodyStart += 1;
  const body = lines.slice(bodyStart + 1).join("\n").trim();
  if (body.length === 0) return [];
  const blocks = body.split(/\n{2,}/);
  return blocks.map((block, index) => {
    const cueLines = block.split("\n");
    if (/^(?:NOTE|STYLE|REGION)(?:[ \t]|$)/.test(cueLines[0] ?? "")) {
      throw new Error("The bounded WebVTT importer does not accept NOTE, STYLE, or REGION blocks.");
    }
    const timingIndex = cueLines[0]?.includes("-->") ? 0 : 1;
    const timing = cueLines[timingIndex];
    if (timing === undefined) {
      throw new Error(`WebVTT cue ${index + 1} has no timing line.`);
    }
    const match = timing.match(
      /^(?:(\d{2,}):)?(\d{2}):(\d{2})\.(\d{3})[ \t]+-->[ \t]+(?:(\d{2,}):)?(\d{2}):(\d{2})\.(\d{3})(?:[ \t]+(.*))?$/,
    );
    if (match === null) {
      throw new Error(`WebVTT cue ${index + 1} has invalid timing syntax.`);
    }
    let alignment: TimelineCaptionStyle["alignment"] = "center";
    let position: TimelineCaptionStyle["position"] = "bottom";
    let hasStyle = false;
    for (const setting of (match[9] ?? "").split(/[ \t]+/).filter(Boolean)) {
      if (setting === "align:start" || setting === "align:center" || setting === "align:end") {
        alignment = setting.slice("align:".length) as TimelineCaptionStyle["alignment"];
        hasStyle = true;
      } else if (setting === "line:0%") {
        position = "top";
        hasStyle = true;
      } else if (setting === "line:100%") {
        position = "bottom";
        hasStyle = true;
      } else {
        throw new Error(`WebVTT cue ${index + 1} uses unsupported setting ${setting}.`);
      }
    }
    let text = cueLines.slice(timingIndex + 1).join("\n");
    let speaker: string | null = null;
    const voice = text.match(/^<v[ \t]+([^>]+)>([\s\S]*)<\/v>$/);
    if (voice !== null) {
      speaker = voice[1]!.trim();
      text = voice[2]!;
    }
    let bold = false;
    let italic = false;
    const boldMatch = text.match(/^<b>([\s\S]*)<\/b>$/);
    if (boldMatch !== null) {
      bold = true;
      text = boldMatch[1]!;
      hasStyle = true;
    }
    const italicMatch = text.match(/^<i>([\s\S]*)<\/i>$/);
    if (italicMatch !== null) {
      italic = true;
      text = italicMatch[1]!;
      hasStyle = true;
    }
    text = decodeWebVttText(text, index);
    const startHours = Number(match[1] ?? 0);
    const endHours = Number(match[5] ?? 0);
    return cueFromParsedBlock({
      id: timingIndex === 1 ? cueLines[0]! : `cue-${index + 1}`,
      index,
      text,
      startMilliseconds: timestampPartsMilliseconds(
        startHours,
        Number(match[2]),
        Number(match[3]),
        Number(match[4]),
      ),
      endMilliseconds: timestampPartsMilliseconds(
        endHours,
        Number(match[6]),
        Number(match[7]),
        Number(match[8]),
      ),
      language,
      speaker,
      style: hasStyle
        ? {
            font_family: null,
            font_size: null,
            foreground: null,
            background: null,
            bold,
            italic,
            alignment,
            position,
          }
        : null,
    });
  });
}

function cueFromParsedBlock({
  id,
  index,
  text,
  startMilliseconds,
  endMilliseconds,
  language,
  speaker,
  style,
}: {
  readonly id: string;
  readonly index: number;
  readonly text: string;
  readonly startMilliseconds: number;
  readonly endMilliseconds: number;
  readonly language: string;
  readonly speaker: string | null;
  readonly style: TimelineCaptionStyle | null;
}): CaptionCue {
  return {
    id: id.trim() || `cue-${index + 1}`,
    name: `Caption ${index + 1}`,
    text,
    startMilliseconds,
    endMilliseconds,
    language,
    speaker,
    style,
    timelineRelationships: [],
  };
}

function validateCues(cues: readonly CaptionCue[]): void {
  if (cues.length === 0) throw new Error("Caption import requires at least one cue.");
  if (cues.length > MAX_CUES) throw new Error("Caption import exceeds the 5,000 cue limit.");
  const ids = new Set<string>();
  let priorEnd = 0;
  for (const [index, cue] of cues.entries()) {
    if (
      cue.id.trim().length === 0 ||
      cue.id.includes("\n") ||
      cue.id.includes("-->") ||
      new TextEncoder().encode(cue.id).byteLength > 512 ||
      ids.has(cue.id)
    ) {
      throw new Error(`Caption cue ${index + 1} has an invalid or duplicate identity.`);
    }
    ids.add(cue.id);
    validateCueText(cue.text, index);
    validateLanguage(cue.language);
    if (
      !Number.isSafeInteger(cue.startMilliseconds) ||
      !Number.isSafeInteger(cue.endMilliseconds) ||
      cue.startMilliseconds < 0 ||
      cue.endMilliseconds <= cue.startMilliseconds
    ) {
      throw new Error(`Caption cue ${index + 1} has invalid millisecond timing.`);
    }
    if (cue.startMilliseconds < priorEnd) {
      throw new Error(`Caption cue ${index + 1} overlaps the prior cue.`);
    }
    if (cue.speaker !== null) validateSpeaker(cue.speaker);
    if (cue.timelineRelationships.length > 64) {
      throw new Error(`Caption cue ${index + 1} has too many timeline relationships.`);
    }
    const relationships = new Set<string>();
    for (const relationship of cue.timelineRelationships) {
      validateCanonicalId(relationship.timeline_id, "timeline");
      if (relationship.clip_id !== null) validateCanonicalId(relationship.clip_id, "clip");
      const key = `${relationship.timeline_id}\u0000${relationship.clip_id ?? ""}`;
      if (relationships.has(key)) {
        throw new Error(`Caption cue ${index + 1} has duplicate timeline relationships.`);
      }
      relationships.add(key);
    }
    priorEnd = cue.endMilliseconds;
  }
}

function validateCueText(value: string, index: number): void {
  if (
    value.trim().length === 0 ||
    value.includes("\0") ||
    value.includes("-->") ||
    new TextEncoder().encode(value).byteLength > MAX_CUE_TEXT_BYTES
  ) {
    throw new Error(`Caption cue ${index + 1} has invalid or oversized text.`);
  }
}

function validateLanguage(value: string): void {
  if (!/^[A-Za-z]{2,8}(?:-[A-Za-z0-9]{1,8})*$/.test(value)) {
    throw new Error("Caption language must be a bounded BCP 47 style tag.");
  }
}

function validateSpeaker(value: string): void {
  if (
    value.trim().length === 0 ||
    value.length > MAX_SPEAKER_BYTES ||
    /[\u0000-\u001f\u007f]/.test(value)
  ) {
    throw new Error("Caption speaker must be bounded visible text.");
  }
}

function validateVttAnnotation(value: string, label: string): void {
  validateSpeaker(value);
  if (/[<>&]/.test(value)) {
    throw new Error(`WebVTT ${label} contains unsupported annotation syntax.`);
  }
}

function encodeWebVttText(value: string): string {
  return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}

function decodeWebVttText(value: string, index: number): string {
  if (/[<>]/.test(value) || /&(?!(?:amp|lt|gt);)/.test(value)) {
    throw new Error(`WebVTT cue ${index + 1} uses unsupported cue text markup.`);
  }
  return value
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&amp;", "&");
}

function optionalText(value: string): string | null {
  const trimmed = value.trim();
  return trimmed.length === 0 ? null : trimmed;
}

function optionalColor(value: string): string | null {
  const trimmed = value.trim();
  if (trimmed.length === 0) return null;
  if (!/^#[0-9A-Fa-f]{8}$/.test(trimmed)) {
    throw new Error("Caption colors must use #RRGGBBAA notation.");
  }
  return trimmed.toLowerCase();
}

function validateCanonicalId(value: string, kind: string): void {
  if (!CANONICAL_ID.test(value) || !value.startsWith(`${kind}:`) || /:0{32}$/.test(value)) {
    throw new Error(`Caption action ${kind} identity is invalid.`);
  }
}

function exactMillisecondRange(start: number, duration: number): ExactTimeRange {
  if (!Number.isSafeInteger(start) || !Number.isSafeInteger(duration) || duration <= 0) {
    throw new Error("Caption range must use positive safe millisecond coordinates.");
  }
  return {
    start: { value: start, timebase: MILLISECOND_TIMEBASE },
    duration: { value: duration, timebase: MILLISECOND_TIMEBASE },
  };
}

function frameToMilliseconds(frame: number, rateNumerator: number, rateDenominator: number): number {
  const numerator = BigInt(frame) * BigInt(rateDenominator) * 1_000n;
  const denominator = BigInt(rateNumerator);
  return safeBigIntNumber((numerator * 2n + denominator) / (denominator * 2n));
}

function rationalUnitsToMilliseconds(
  value: string,
  numerator: number,
  denominator: number,
): number {
  if (!/^(?:0|[1-9][0-9]*)$/.test(value)) {
    throw new Error("Caption export timing must be a nonnegative canonical integer.");
  }
  if (
    !Number.isSafeInteger(numerator) ||
    !Number.isSafeInteger(denominator) ||
    numerator <= 0 ||
    denominator <= 0
  ) {
    throw new Error("Caption export timing has an invalid clock.");
  }
  const scaled = BigInt(value) * BigInt(denominator) * 1_000n;
  const divisor = BigInt(numerator);
  return safeBigIntNumber((scaled * 2n + divisor) / (divisor * 2n));
}

function safeBigIntNumber(value: bigint): number {
  const numeric = Number(value);
  if (!Number.isSafeInteger(numeric)) {
    throw new Error("Caption timing exceeds the supported exact range.");
  }
  return numeric;
}

function timestampMatchMilliseconds(match: RegExpMatchArray, offset: number): number {
  return timestampPartsMilliseconds(
    Number(match[offset]),
    Number(match[offset + 1]),
    Number(match[offset + 2]),
    Number(match[offset + 3]),
  );
}

function timestampPartsMilliseconds(
  hours: number,
  minutes: number,
  seconds: number,
  milliseconds: number,
): number {
  if (
    ![hours, minutes, seconds, milliseconds].every(Number.isSafeInteger) ||
    hours < 0 ||
    minutes < 0 ||
    minutes > 59 ||
    seconds < 0 ||
    seconds > 59 ||
    milliseconds < 0 ||
    milliseconds > 999
  ) {
    throw new Error("Caption timestamp is outside the supported clock range.");
  }
  const result = ((hours * 60 + minutes) * 60 + seconds) * 1_000 + milliseconds;
  if (!Number.isSafeInteger(result)) {
    throw new Error("Caption timestamp exceeds the supported exact range.");
  }
  return result;
}

function formatTimestamp(value: number, separator: "," | "."): string {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error("Caption timestamp must be a nonnegative safe millisecond value.");
  }
  const milliseconds = value % 1_000;
  const totalSeconds = Math.floor(value / 1_000);
  const seconds = totalSeconds % 60;
  const totalMinutes = Math.floor(totalSeconds / 60);
  const minutes = totalMinutes % 60;
  const hours = Math.floor(totalMinutes / 60);
  return `${hours.toString().padStart(2, "0")}:${minutes
    .toString()
    .padStart(2, "0")}:${seconds.toString().padStart(2, "0")}${separator}${milliseconds
    .toString()
    .padStart(3, "0")}`;
}

function freezeCue(cue: CaptionCue): CaptionCue {
  return Object.freeze({
    ...cue,
    style: cue.style === null ? null : Object.freeze({ ...cue.style }),
    timelineRelationships: Object.freeze(
      cue.timelineRelationships.map((relationship) => Object.freeze({ ...relationship })),
    ),
  });
}
