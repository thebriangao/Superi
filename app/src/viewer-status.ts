import type {
  EditorAudioSeam,
  EditorAudioTrackState,
  EditorPlaybackSnapshot,
  EditorRationalTime,
  EditorStateSnapshot,
} from "./api.ts";
import {
  formatExactPlaybackTime,
  formatExactRate,
  playbackDegradationLabel,
  playbackVisualState,
} from "./playback-transport.ts";
import type { SourceMonitorSnapshot, SourceMonitorTime } from "./project-lifecycle.ts";
import {
  projectTimelineDocument,
  type TimelineCanvasItem,
  type TimelineCanvasModel,
  type TimelineCanvasTrack,
  type TimelineExactRange,
  type TimelineRate,
} from "./timeline-workspace.ts";

const RATE_NUMERATOR_KEY = "superi.project.timeline.default_rate_numerator";
const RATE_DENOMINATOR_KEY = "superi.project.timeline.default_rate_denominator";
const TIMECODE_MODE_KEY = "superi.project.timeline.timecode_mode";
const SIGNED_DECIMAL = /^(?:0|-[1-9][0-9]*|[1-9][0-9]*)$/;
const MAX_U32 = 4_294_967_295;

export type ViewerStatusRole = "source" | "program" | "composite" | "color";

export interface ViewerStatusDisplay {
  readonly timecode: string;
  readonly frame: string;
  readonly source: string;
  readonly droppedFrames: string;
  readonly playbackStatus: string;
  readonly frameCache: string;
  readonly visualState: string;
  readonly audioState: string;
  readonly audioCache: string;
  readonly comparisonState: string;
  readonly editorialIntent: string;
}

export const VIEWER_STATUS_FIELDS = Object.freeze([
  Object.freeze({ key: "timecode", label: "Timecode" }),
  Object.freeze({ key: "frame", label: "Frame" }),
  Object.freeze({ key: "source", label: "Source" }),
  Object.freeze({ key: "droppedFrames", label: "Dropped frames" }),
  Object.freeze({ key: "playbackStatus", label: "Playback status" }),
  Object.freeze({ key: "frameCache", label: "Frame cache" }),
  Object.freeze({ key: "visualState", label: "Visual state" }),
  Object.freeze({ key: "audioState", label: "Audio state" }),
  Object.freeze({ key: "audioCache", label: "Audio cache" }),
  Object.freeze({ key: "comparisonState", label: "Comparison state" }),
  Object.freeze({ key: "editorialIntent", label: "Editorial intent" }),
] as const satisfies readonly {
  readonly key: keyof ViewerStatusDisplay;
  readonly label: string;
}[]);

interface TemporalDisplay {
  readonly timecode: string;
  readonly frame: string;
  readonly recordFrame: bigint | null;
}

interface ActiveProgramItem {
  readonly track: TimelineCanvasTrack;
  readonly item: TimelineCanvasItem;
}

type ActiveProgramProjection =
  | { readonly status: "resolved"; readonly active: ActiveProgramItem | null }
  | { readonly status: "unavailable"; readonly reason: string };

export function projectViewerStatusDisplay(
  role: ViewerStatusRole,
  snapshot: EditorStateSnapshot | null,
  sourceMonitor: SourceMonitorSnapshot | null,
  liveComparisonState: string | null = null,
): ViewerStatusDisplay {
  if (snapshot === null) {
    return freezeDisplay({
      timecode: "Unavailable: editor state has not been observed.",
      frame: "Unavailable: editor state has not been observed.",
      source:
        role === "source" && sourceMonitor !== null
          ? formatSourceMonitor(sourceMonitor)
          : "Unavailable: editor state has not been observed.",
      droppedFrames: "Unavailable: playback has not been observed.",
      playbackStatus: "Detached: editor state has not been observed.",
      frameCache: "Unavailable: playback has not been observed.",
      visualState: "Unavailable: playback has not been observed.",
      audioState: "Unavailable: playback has not been observed.",
      audioCache: "Unavailable: playback has not been observed.",
      comparisonState:
        liveComparisonState ??
        (role === "source"
          ? "Source monitor view; no program comparison."
          : "Unavailable: editor state has not been observed."),
      editorialIntent:
        role === "source"
          ? formatSourceIntent(sourceMonitor)
          : "Unavailable: editorial state has not been observed.",
    });
  }

  const playback = observedPlayback(snapshot);
  const model = projectModel(snapshot);
  const temporal = projectTemporalDisplay(snapshot, model, playback);
  const active =
    model !== null && temporal.recordFrame !== null
      ? projectActiveProgramItem(model, temporal.recordFrame)
      : { status: "resolved", active: null } as const;

  return freezeDisplay({
    timecode: temporal.timecode,
    frame: temporal.frame,
    source:
      role === "source"
        ? sourceMonitor === null
          ? "Unavailable: source monitor has not been observed."
          : formatSourceMonitor(sourceMonitor)
        : model === null
          ? "Unavailable: canonical timeline state is invalid."
          : temporal.recordFrame === null
            ? "Unavailable: playback position has not been observed."
            : active.status === "unavailable"
              ? `Unavailable: ${active.reason}.`
              : formatProgramSource(active.active),
    droppedFrames: formatDroppedFrames(playback),
    playbackStatus: formatPlaybackStatus(snapshot, playback),
    frameCache: formatFrameCache(playback),
    visualState:
      playback === null
        ? "Unavailable: playback has not been observed."
        : playbackVisualState(playback),
    audioState: formatAudioState(playback),
    audioCache: formatAudioCache(snapshot, playback),
    comparisonState:
      liveComparisonState ?? formatComparisonState(role, model, temporal, active),
    editorialIntent:
      role === "source"
        ? formatSourceIntent(sourceMonitor)
        : model === null
          ? "Unavailable: canonical editorial state is invalid."
          : temporal.recordFrame === null
            ? "Unavailable: playback position has not been observed."
            : formatEditorialIntent(model, active),
  });
}

function observedPlayback(snapshot: EditorStateSnapshot): EditorPlaybackSnapshot | null {
  return snapshot.playback.status === "attached" ? snapshot.playback.latest : null;
}

function projectModel(snapshot: EditorStateSnapshot): TimelineCanvasModel | null {
  try {
    return projectTimelineDocument(
      snapshot.timeline.document,
      snapshot.project.root_timeline_id,
    );
  } catch {
    return null;
  }
}

function projectTemporalDisplay(
  snapshot: EditorStateSnapshot,
  model: TimelineCanvasModel | null,
  playback: EditorPlaybackSnapshot | null,
): TemporalDisplay {
  if (model === null) {
    return {
      timecode: "Unavailable: canonical timeline state is invalid.",
      frame: "Unavailable: canonical timeline state is invalid.",
      recordFrame: null,
    };
  }
  if (playback === null) {
    return {
      timecode: "Unavailable: playback position has not been observed.",
      frame: "Unavailable: playback position has not been observed.",
      recordFrame: null,
    };
  }

  try {
    const rate = projectTimecodeRate(snapshot, model);
    const mode = projectTimecodeMode(snapshot);
    const recordFrame = rescaleEditorTime(playback.playhead, rate);
    const globalStart = canonicalInteger(
      model.globalStart.value,
      "timeline global-start frame",
    );
    if (!sameRate(model.globalStart.timebase, rate)) {
      throw new Error("timeline global-start clock differs from the display rate");
    }
    const displayFrame = globalStart + recordFrame;
    const modeLabel =
      mode === "drop_frame" ? "drop-frame labels" : "non-drop-frame labels";
    return {
      timecode: `${formatTimecode(displayFrame, rate, mode)} (${modeLabel})`,
      frame:
        `record ${recordFrame}; display ${displayFrame} @ ` +
        `${rate.numerator}/${rate.denominator} frames/s`,
      recordFrame,
    };
  } catch (error: unknown) {
    const reason = error instanceof Error ? error.message : String(error);
    return {
      timecode: `Unavailable: ${reason}.`,
      frame: `Unavailable: ${reason}.`,
      recordFrame: null,
    };
  }
}

function projectTimecodeRate(
  snapshot: EditorStateSnapshot,
  model: TimelineCanvasModel,
): TimelineRate {
  const numerator = integerSetting(snapshot, RATE_NUMERATOR_KEY);
  const denominator = integerSetting(snapshot, RATE_DENOMINATOR_KEY);
  const rate = { numerator, denominator };
  assertRate(rate, "project timeline rate");
  if (!sameRate(rate, model.editRate)) {
    throw new Error("project timeline rate differs from the canonical root edit rate");
  }
  return rate;
}

function projectTimecodeMode(
  snapshot: EditorStateSnapshot,
): "drop_frame" | "non_drop_frame" {
  const setting = snapshot.project.settings.values[TIMECODE_MODE_KEY];
  if (
    setting?.kind !== "text" ||
    (setting.value !== "drop_frame" && setting.value !== "non_drop_frame")
  ) {
    throw new Error("project timecode mode is missing or invalid");
  }
  return setting.value;
}

function integerSetting(snapshot: EditorStateSnapshot, key: string): number {
  const setting = snapshot.project.settings.values[key];
  if (setting?.kind !== "integer" || !Number.isSafeInteger(setting.value)) {
    throw new Error(`project setting ${key} is missing or invalid`);
  }
  return setting.value;
}

function rescaleEditorTime(time: EditorRationalTime, target: TimelineRate): bigint {
  assertRate(time.timebase, "playback timebase");
  assertRate(target, "timeline edit rate");
  if (!Number.isSafeInteger(time.value)) {
    throw new Error("playback position is not an exact safe integer");
  }
  const numerator =
    BigInt(time.value) *
    BigInt(time.timebase.denominator) *
    BigInt(target.numerator);
  const denominator =
    BigInt(time.timebase.numerator) * BigInt(target.denominator);
  if (numerator % denominator !== 0n) {
    throw new Error("playback position cannot be represented exactly at the timeline edit rate");
  }
  return numerator / denominator;
}

function assertRate(rate: TimelineRate, label: string): void {
  if (
    !Number.isSafeInteger(rate.numerator) ||
    !Number.isSafeInteger(rate.denominator) ||
    rate.numerator <= 0 ||
    rate.denominator <= 0 ||
    rate.numerator > MAX_U32 ||
    rate.denominator > MAX_U32
  ) {
    throw new Error(`${label} is not a positive exact rational rate`);
  }
}

function sameRate(left: TimelineRate, right: TimelineRate): boolean {
  return (
    left.numerator === right.numerator &&
    left.denominator === right.denominator
  );
}

function canonicalInteger(value: string, label: string): bigint {
  if (!SIGNED_DECIMAL.test(value)) {
    throw new Error(`${label} is not a canonical signed integer`);
  }
  return BigInt(value);
}

function formatTimecode(
  frame: bigint,
  rate: TimelineRate,
  mode: "drop_frame" | "non_drop_frame",
): string {
  assertRate(rate, "timecode rate");
  const nominalValue =
    (BigInt(rate.numerator) + BigInt(rate.denominator) / 2n) /
    BigInt(rate.denominator);
  if (nominalValue <= 0n || nominalValue > BigInt(Number.MAX_SAFE_INTEGER)) {
    throw new Error("timecode rate has no valid nominal frame rate");
  }
  const nominal = Number(nominalValue);

  const negative = frame < 0n;
  const continuous = negative ? -frame : frame;
  let labelFrame = continuous;
  if (mode === "drop_frame") {
    if (
      rate.denominator !== 1_001 ||
      rate.numerator !== nominal * 1_000 ||
      nominal % 30 !== 0
    ) {
      throw new Error("drop-frame labels are unsupported for the project timeline rate");
    }
    const nominalFrames = BigInt(nominal);
    const droppedPerMinute = nominalFrames / 15n;
    const framesPerMinute = nominalFrames * 60n - droppedPerMinute;
    const framesPerTenMinutes = nominalFrames * 600n - droppedPerMinute * 9n;
    const completeTenMinuteCycles = continuous / framesPerTenMinutes;
    const remainder = continuous % framesPerTenMinutes;
    labelFrame = continuous + completeTenMinuteCycles * droppedPerMinute * 9n;
    if (remainder >= droppedPerMinute) {
      labelFrame +=
        droppedPerMinute * ((remainder - droppedPerMinute) / framesPerMinute);
    }
  }

  const nominalFrames = BigInt(nominal);
  const frameField = labelFrame % nominalFrames;
  const totalSeconds = labelFrame / nominalFrames;
  const seconds = totalSeconds % 60n;
  const totalMinutes = totalSeconds / 60n;
  const minutes = totalMinutes % 60n;
  const hours = totalMinutes / 60n;
  const separator = mode === "drop_frame" ? ";" : ":";
  const prefix = negative ? "-" : "";
  return (
    `${prefix}${padTimecodeField(hours)}:${padTimecodeField(minutes)}:` +
    `${padTimecodeField(seconds)}${separator}${padTimecodeField(frameField)}`
  );
}

function padTimecodeField(value: bigint): string {
  return value.toString().padStart(2, "0");
}

function activeProgramItem(
  model: TimelineCanvasModel,
  recordFrame: bigint,
): ActiveProgramItem | null {
  let gapFallback: ActiveProgramItem | null = null;
  for (const track of [...model.tracks].reverse()) {
    if (track.kind !== "video" || !track.enabled) continue;
    const trackFrame = rescaleFrame(recordFrame, model.editRate, track.timebase);
    const activeItems = track.items.filter((item) =>
      exactRangeContains(item.recordRange, trackFrame),
    );
    const transition = activeItems.find((item) => item.kind === "transition");
    if (transition) return { track, item: transition };
    const visual = [...activeItems].reverse().find((item) => item.kind !== "gap");
    if (visual) return { track, item: visual };
    const gap = activeItems.find((item) => item.kind === "gap");
    if (gap && gapFallback === null) gapFallback = { track, item: gap };
  }
  return gapFallback;
}

function projectActiveProgramItem(
  model: TimelineCanvasModel,
  recordFrame: bigint,
): ActiveProgramProjection {
  try {
    return { status: "resolved", active: activeProgramItem(model, recordFrame) };
  } catch (error: unknown) {
    return {
      status: "unavailable",
      reason: error instanceof Error ? error.message : String(error),
    };
  }
}

function rescaleFrame(
  frame: bigint,
  source: TimelineRate,
  target: TimelineRate,
): bigint {
  assertRate(source, "source frame rate");
  assertRate(target, "target frame rate");
  const numerator =
    frame * BigInt(source.denominator) * BigInt(target.numerator);
  const denominator = BigInt(source.numerator) * BigInt(target.denominator);
  if (numerator % denominator !== 0n) {
    throw new Error("record frame cannot be represented exactly on the track clock");
  }
  return numerator / denominator;
}

function exactRangeContains(range: TimelineExactRange, coordinate: bigint): boolean {
  const start = canonicalInteger(range.start.value, "record range start");
  const duration = canonicalInteger(range.duration.value, "record range duration");
  return duration > 0n && coordinate >= start && coordinate < start + duration;
}

function formatProgramSource(active: ActiveProgramItem | null): string {
  if (active === null) return "No enabled video source at the exact record frame.";
  const { item } = active;
  if (item.kind === "transition" && item.transition !== null) {
    return (
      `${item.name} (${item.id}); transition ` +
      `${objectIdentity(item.transition.from)} to ${objectIdentity(item.transition.to)}; ` +
      `record ${formatExactRange(item.recordRange)}.`
    );
  }
  if (item.kind === "gap") {
    return `${item.name} (${item.id}); gap; record ${formatExactRange(item.recordRange)}.`;
  }
  const source = item.source
    ? `${item.source.kind} ${item.source.id}`
    : `${item.kind} ${item.id}`;
  const sourceRange = item.sourceRange
    ? `; source ${formatExactRange(item.sourceRange)}`
    : "";
  return (
    `${item.name} (${item.id}); ${source}${sourceRange}; ` +
    `record ${formatExactRange(item.recordRange)}.`
  );
}

function formatExactRange(range: TimelineExactRange): string {
  if (!sameRate(range.start.timebase, range.duration.timebase)) {
    return "unavailable due to mismatched clocks";
  }
  const start = canonicalInteger(range.start.value, "range start");
  const duration = canonicalInteger(range.duration.value, "range duration");
  return (
    `[${start}, ${start + duration}) @ ` +
    `${range.start.timebase.numerator}/${range.start.timebase.denominator}`
  );
}

function objectIdentity(object: { readonly kind: string; readonly id: string }): string {
  return `${object.kind} ${object.id}`;
}

function formatSourceMonitor(monitor: SourceMonitorSnapshot): string {
  try {
    return formatValidSourceMonitor(monitor);
  } catch (error: unknown) {
    const reason = error instanceof Error ? error.message : String(error);
    return `Unavailable: ${reason}.`;
  }
}

function formatValidSourceMonitor(monitor: SourceMonitorSnapshot): string {
  if (monitor.media_id === null) {
    return `Source monitor empty; ${monitor.engine_state}; ${monitor.presentation_note}`;
  }
  const name = monitor.media_name ?? "Unnamed media";
  const stream = monitor.stream
    ? `${monitor.stream.kind} stream ${monitor.stream.stream_id} ${monitor.stream.codec}`
    : "stream unavailable";
  const current = monitor.current
    ? `source ${formatSourceMonitorTime(monitor.current)}`
    : "source coordinate unavailable";
  const fingerprint =
    monitor.source_fingerprint !== null &&
    monitor.source_fingerprint === monitor.opened_fingerprint
      ? "fingerprint current"
      : "fingerprint stale";
  return (
    `${name} (${monitor.media_id}); ${stream}; ${current}; ` +
    `${monitor.engine_state}; ${fingerprint}.`
  );
}

function formatSourceMonitorTime(time: SourceMonitorTime): string {
  assertRate(
    {
      numerator: time.timebase_numerator,
      denominator: time.timebase_denominator,
    },
    "source-monitor timebase",
  );
  if (!Number.isSafeInteger(time.value)) {
    throw new Error("source-monitor coordinate is not an exact safe integer");
  }
  return `${time.value} @ ${time.timebase_numerator}/${time.timebase_denominator}`;
}

function formatDroppedFrames(playback: EditorPlaybackSnapshot | null): string {
  if (playback === null) return "Unavailable: playback has not been observed.";
  if (
    !nonnegativeCount(playback.total_dropped) ||
    !nonnegativeCount(playback.consecutive_dropped) ||
    !nonnegativeCount(playback.forced_presentations)
  ) {
    return "Unavailable: playback drop counters are invalid.";
  }
  const forcedLabel =
    playback.forced_presentations === 1 ? "forced presentation" : "forced presentations";
  return (
    `${playback.total_dropped} physical playback drops total; ` +
    `${playback.consecutive_dropped} consecutive; ` +
    `${playback.forced_presentations} ${forcedLabel}; independent of drop-frame labels.`
  );
}

function nonnegativeCount(value: number): boolean {
  return Number.isSafeInteger(value) && value >= 0;
}

function formatPlaybackStatus(
  snapshot: EditorStateSnapshot,
  playback: EditorPlaybackSnapshot | null,
): string {
  if (snapshot.playback.status === "detached") {
    return "detached; playback owner is unavailable.";
  }
  if (playback === null) {
    return snapshot.playback.pending_command
      ? "attached; command pending; no playback observation yet."
      : "attached; no playback observation yet.";
  }
  try {
    validatePlaybackSnapshot(playback);
    const pending = snapshot.playback.pending_command
      ? "command pending"
      : "no command pending";
    const bounds = `bounds ${formatEditorRange(playback.bounds)}`;
    const loop = playback.loop_range
      ? `loop ${formatEditorRange(playback.loop_range)}`
      : "loop off";
    const scheduled = playback.scheduled_frame
      ? `scheduled frame ${formatValidatedEditorTime(playback.scheduled_frame)}`
      : "scheduled frame unavailable";
    const due = playback.scheduled_due_clock
      ? `due ${formatValidatedEditorTime(playback.scheduled_due_clock)}`
      : "due clock unavailable";
    const degradation =
      playback.degradation.length === 0
        ? "degradation none"
        : `degradation ${playback.degradation.map(playbackDegradationLabel).join(" ")}`;
    const failure = playback.failure
      ? `failure ${playback.failure.category}/${playback.failure.recoverability}`
      : "failure none";
    return (
      `attached; ${pending}; ${playback.mode}; ${formatExactRate(playback)}; ` +
      `${bounds}; ${loop}; epoch ${playback.epoch}; ${scheduled}; ${due}; ` +
      `${degradation}; ${failure}.`
    );
  } catch {
    return "attached; playback observation contains an invalid exact value.";
  }
}

function formatEditorRange(range: EditorPlaybackSnapshot["bounds"]): string {
  const start = range.start;
  assertRate(start.timebase, "playback range timebase");
  if (
    !Number.isSafeInteger(start.value) ||
    !Number.isSafeInteger(range.duration) ||
    range.duration <= 0
  ) {
    throw new Error("playback range is not exact");
  }
  return (
    `[${start.value}, ${BigInt(start.value) + BigInt(range.duration)}) @ ` +
    `${start.timebase.numerator}/${start.timebase.denominator}`
  );
}

function validatePlaybackSnapshot(playback: EditorPlaybackSnapshot): void {
  if (
    !Number.isSafeInteger(playback.rate_numerator) ||
    !Number.isSafeInteger(playback.rate_denominator) ||
    playback.rate_numerator === 0 ||
    playback.rate_denominator <= 0 ||
    !nonnegativeCount(playback.epoch)
  ) {
    throw new Error("playback rate or continuity epoch is invalid");
  }
  formatEditorRange(playback.bounds);
  formatValidatedEditorTime(playback.playhead);
}

function formatValidatedEditorTime(time: EditorRationalTime): string {
  assertRate(time.timebase, "playback observation timebase");
  if (!Number.isSafeInteger(time.value)) {
    throw new Error("playback observation is not an exact safe integer");
  }
  return formatExactPlaybackTime(time);
}

function formatAudioState(playback: EditorPlaybackSnapshot | null): string {
  if (playback === null) return "Unavailable: playback has not been observed.";
  if (
    !nonnegativeCount(playback.discard_requested_generation) ||
    !nonnegativeCount(playback.discard_applied_generation)
  ) {
    return "Unavailable: audio discard generations are invalid.";
  }
  const acknowledgement =
    playback.discard_requested_generation === playback.discard_applied_generation
      ? "discard acknowledged"
      : "discard acknowledgement pending";
  return (
    `${playback.audio_state}; generation ` +
    `${playback.discard_requested_generation} requested, ` +
    `${playback.discard_applied_generation} applied; ${acknowledgement}.`
  );
}

function formatFrameCache(playback: EditorPlaybackSnapshot | null): string {
  if (playback === null) return "Unavailable: playback has not been observed.";
  try {
    validatePlaybackSnapshot(playback);
    if (
      playback.mode !== "paused" &&
      playback.mode !== "playing" &&
      playback.mode !== "scrubbing" &&
      playback.mode !== "ended"
    ) {
      throw new Error("playback mode is invalid");
    }
    if (
      !Array.isArray(playback.degradation) ||
      playback.degradation.some(
        (degradation) =>
          typeof degradation !== "string" || degradation.trim().length === 0,
      )
    ) {
      throw new Error("playback degradation state is invalid");
    }
    const scheduled = playback.scheduled_frame
      ? `foreground frame ${formatValidatedEditorTime(playback.scheduled_frame)}`
      : "foreground frame unavailable";
    const due = playback.scheduled_due_clock
      ? `due ${formatValidatedEditorTime(playback.scheduled_due_clock)}`
      : "due clock unavailable";
    const prediction = playback.degradation.includes("prefetch_failure")
      ? "predictive cache failed; foreground scheduling remains authoritative"
      : "predictive cache completion not exposed";
    const output = playback.degradation.includes("viewport_output_unavailable")
      ? "decoded viewport output unavailable"
      : "decoded viewport output availability not inferred";
    return (
      `mode ${playback.mode}; ${scheduled}; ${due}; ${prediction}; ${output}; ` +
      "interaction does not wait for cache work; fill, hit, and occupancy telemetry unavailable."
    );
  } catch (error: unknown) {
    const reason = error instanceof Error ? error.message : String(error);
    return `Unavailable: ${reason}.`;
  }
}

function formatAudioCache(
  snapshot: EditorStateSnapshot,
  playback: EditorPlaybackSnapshot | null,
): string {
  if (playback === null) return "Unavailable: playback has not been observed.";
  try {
    if (
      !nonnegativeCount(playback.discard_requested_generation) ||
      !nonnegativeCount(playback.discard_applied_generation) ||
      playback.discard_applied_generation > playback.discard_requested_generation
    ) {
      throw new Error("audio discard generations are invalid");
    }
    if (
      !Array.isArray(playback.degradation) ||
      playback.degradation.some(
        (degradation) =>
          typeof degradation !== "string" || degradation.trim().length === 0,
      )
    ) {
      throw new Error("playback degradation state is invalid");
    }
    const synchronization = formatAudioSynchronization(playback);
    const tracks = formatAudioTracks(snapshot);
    const output = playback.degradation.includes("audio_output_unavailable")
      ? "device audio output unavailable; audible continuity not claimed"
      : "audible output state not inferred";
    return (
      `${synchronization}; discard generation ` +
      `${playback.discard_requested_generation} requested, ` +
      `${playback.discard_applied_generation} applied; buffer fill telemetry unavailable; ` +
      `${tracks}; ${output}.`
    );
  } catch (error: unknown) {
    const reason = error instanceof Error ? error.message : String(error);
    return `Unavailable: ${reason}.`;
  }
}

function formatAudioSynchronization(playback: EditorPlaybackSnapshot): string {
  const discardPending =
    playback.discard_requested_generation !== playback.discard_applied_generation;
  switch (playback.audio_state) {
    case "synchronized":
      if (discardPending) {
        throw new Error("audio synchronization conflicts with pending discard state");
      }
      return "transport synchronization current";
    case "discard_pending":
      if (!discardPending) {
        throw new Error("audio discard-pending state has no pending generation");
      }
      return (
        "transport synchronization pending discard acknowledgement; " +
        "pre-discontinuity queued audio blocked until callback acknowledgement"
      );
    case "muted_inactive":
      return "transport inactive and muted";
    case "muted_unsupported_rate":
      return "transport rate unsupported and muted";
    default:
      throw new Error("audio transport state is invalid");
  }
}

function formatAudioTracks(snapshot: EditorStateSnapshot): string {
  const audio = snapshot.audio;
  if (
    audio === null ||
    typeof audio !== "object" ||
    !nonnegativeCount(audio.audio_track_count) ||
    !Array.isArray(audio.tracks) ||
    audio.audio_track_count !== audio.tracks.length
  ) {
    throw new Error("canonical audio track state is invalid");
  }
  if (audio.tracks.length === 0) return "canonical audio tracks none";

  const identities = new Set<string>();
  const tracks = audio.tracks.map((track) => {
    const identity = `${track.timeline_id}\u0000${track.track_id}`;
    if (identities.has(identity)) {
      throw new Error("canonical audio track identity is duplicated");
    }
    identities.add(identity);
    return formatAudioTrack(track);
  });
  return `canonical audio tracks ${tracks.join(" | ")}`;
}

function formatAudioTrack(track: EditorAudioTrackState): string {
  requireText(track.timeline_id, "audio timeline identity");
  requireText(track.track_id, "audio track identity");
  if (!Number.isSafeInteger(track.sample_rate) || track.sample_rate <= 0) {
    throw new Error("audio sample clock is invalid");
  }
  if (!nonnegativeCount(track.clip_count)) {
    throw new Error("audio track clip count is invalid");
  }
  const sourceChannels = validateChannelOrder(
    track.source_channels,
    "source channel order",
  );
  const destinationChannels = validateChannelOrder(
    track.destination_channels,
    "destination channel order",
  );
  if (!Array.isArray(track.routes) || track.routes.length !== sourceChannels.length) {
    throw new Error("audio route count does not match source channel order");
  }
  const routes = track.routes.map((route, index) => {
    if (route.source !== sourceChannels[index]) {
      throw new Error("audio route order does not match source channel order");
    }
    if (route.target.kind === "muted") return `${route.source} -> muted`;
    if (
      route.target.kind !== "channel" ||
      !destinationChannels.includes(route.target.channel)
    ) {
      throw new Error("audio route target is absent from destination channel order");
    }
    return `${route.source} -> ${route.target.channel}`;
  });
  const destination =
    track.destination.kind === "main"
      ? "main"
      : `track ${requireText(track.destination.track_id, "audio destination track identity")}`;
  return (
    `${track.track_id}: sample clock ${track.sample_rate} Hz; ` +
    `source channels [${sourceChannels.join(", ")}]; ` +
    `destination ${destination} [${destinationChannels.join(", ")}]; ` +
    `routes [${routes.join(", ")}]; clips ${track.clip_count}; ` +
    formatAudioContinuity(track)
  );
}

function validateChannelOrder(channels: readonly string[], label: string): readonly string[] {
  if (!Array.isArray(channels) || channels.length === 0) {
    throw new Error(`${label} is empty`);
  }
  const seen = new Set<string>();
  for (const channel of channels) {
    const value = requireText(channel, label);
    if (seen.has(value)) throw new Error(`${label} contains a duplicate`);
    seen.add(value);
  }
  return channels;
}

function formatAudioContinuity(track: EditorAudioTrackState): string {
  if (track.continuity.status === "unsupported") {
    return `continuity unsupported: ${requireText(track.continuity.reason, "continuity reason")}`;
  }
  if (
    track.continuity.status !== "audited" ||
    typeof track.continuity.uninterrupted_record_coverage !== "boolean" ||
    !Array.isArray(track.continuity.seams)
  ) {
    throw new Error("audio continuity state is invalid");
  }
  const seams =
    track.continuity.seams.length === 0
      ? "seams none"
      : track.continuity.seams.map(formatAudioSeam).join("; ");
  return (
    "continuity audited; uninterrupted record coverage " +
    `${track.continuity.uninterrupted_record_coverage ? "yes" : "no"}; ${seams}`
  );
}

function formatAudioSeam(seam: EditorAudioSeam): string {
  const left = requireText(seam.left_clip_id, "left audio clip identity");
  const right = requireText(seam.right_clip_id, "right audio clip identity");
  let record: string;
  switch (seam.record.kind) {
    case "seamless":
      record = "record seamless";
      break;
    case "gap":
    case "overlap":
      if (!Number.isSafeInteger(seam.record.sample_count) || seam.record.sample_count <= 0) {
        throw new Error("audio record seam sample count is invalid");
      }
      record = `record ${seam.record.kind} ${seam.record.sample_count} samples`;
      break;
    default:
      throw new Error("audio record seam is invalid");
  }

  let source: string;
  switch (seam.source.kind) {
    case "continuous":
      source = "source continuous";
      break;
    case "discontinuous":
      if (
        !nonnegativeCount(seam.source.expected) ||
        !nonnegativeCount(seam.source.actual)
      ) {
        throw new Error("audio source seam sample timing is invalid");
      }
      source =
        `source discontinuity expected ${seam.source.expected} ` +
        `actual ${seam.source.actual}`;
      break;
    case "different_clip":
      source =
        `source clip change ${requireText(seam.source.left, "left source clip identity")} ` +
        `to ${requireText(seam.source.right, "right source clip identity")}`;
      break;
    default:
      throw new Error("audio source seam is invalid");
  }
  return `seam ${left} -> ${right}, ${record}, ${source}`;
}

function requireText(value: string, label: string): string {
  if (typeof value !== "string" || value.trim().length === 0) {
    throw new Error(`${label} is missing or invalid`);
  }
  return value;
}

function formatComparisonState(
  role: ViewerStatusRole,
  model: TimelineCanvasModel | null,
  temporal: TemporalDisplay,
  active: ActiveProgramProjection,
): string {
  if (role === "source") return "Source monitor view; no program comparison.";
  if (model === null) return "Unavailable: canonical timeline state is invalid.";
  if (temporal.recordFrame === null) {
    return "Unavailable: playback position has not been observed.";
  }
  if (active.status === "unavailable") return `Unavailable: ${active.reason}.`;
  if (active.active?.item.kind === "transition" && active.active.item.transition !== null) {
    return (
      `Transition comparison: ${objectIdentity(active.active.item.transition.from)} to ` +
      `${objectIdentity(active.active.item.transition.to)}.`
    );
  }
  if (active.active === null) return "No program source at the exact record frame.";
  return `Single program source: ${active.active.item.kind} ${active.active.item.id}.`;
}

function formatEditorialIntent(
  model: TimelineCanvasModel,
  projection: ActiveProgramProjection,
): string {
  if (projection.status === "unavailable") {
    return `Unavailable: ${projection.reason}.`;
  }
  const active = projection.active;
  if (active === null) {
    return (
      `No active video item; linked selection ` +
      `${model.linkedSelectionEnabled ? "enabled" : "disabled"}; ` +
      `snapping ${model.snappingEnabled ? "enabled" : "disabled"}.`
    );
  }
  const { track, item } = active;
  const trackState = [
    track.targeted ? "targeted" : "not targeted",
    track.syncLocked ? "sync locked" : "not sync locked",
    track.enabled ? "enabled" : "disabled",
  ].join(", ");
  const selection = item.selected ? `${item.kind} selected` : `${item.kind} not selected`;
  const group = item.group ? `group ${item.group.join(" + ")}` : "no group";
  const link = item.link ? `link ${item.link.join(" + ")}` : "no link";
  return (
    `${track.name} ${trackState}; ${selection}; ${group}; ${link}; ` +
    `linked selection ${model.linkedSelectionEnabled ? "enabled" : "disabled"}; ` +
    `snapping ${model.snappingEnabled ? "enabled" : "disabled"}.`
  );
}

function formatSourceIntent(sourceMonitor: SourceMonitorSnapshot | null): string {
  try {
    return formatValidSourceIntent(sourceMonitor);
  } catch (error: unknown) {
    const reason = error instanceof Error ? error.message : String(error);
    return `Unavailable: ${reason}.`;
  }
}

function formatValidSourceIntent(sourceMonitor: SourceMonitorSnapshot | null): string {
  if (sourceMonitor === null) {
    return "Unavailable: source-monitor intent has not been observed.";
  }
  const inMark = sourceMonitor.marks.in_mark
    ? formatSourceMonitorTime(sourceMonitor.marks.in_mark)
    : "unset";
  const outMark = sourceMonitor.marks.out_mark
    ? formatSourceMonitorTime(sourceMonitor.marks.out_mark)
    : "unset";
  return (
    `Source marks in ${inMark}, out ${outMark}; ` +
    `${sourceMonitor.marks_fresh ? "identity current" : "identity stale"}; ` +
    `monitor revision ${sourceMonitor.monitor_revision}.`
  );
}

function freezeDisplay(display: ViewerStatusDisplay): ViewerStatusDisplay {
  return Object.freeze(display);
}
