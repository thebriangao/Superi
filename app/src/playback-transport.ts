import type {
  EditorPlaybackSnapshot,
  EditorRationalTime,
  PlaybackDirection,
  PlaybackTransportAction,
} from "./api.ts";

const SHUTTLE_MAGNITUDES = Object.freeze([1, 2, 4, 8] as const);

const DEGRADATION_LABELS: Readonly<Record<string, string>> = Object.freeze({
  frame_failure:
    "Frame evaluation failed; the exact transport position remains available.",
  viewport_backpressure:
    "Viewport delivery is backpressured; exact transport timing remains authoritative.",
  prefetch_failure:
    "Frame prefetch failed; foreground transport remains authoritative.",
  audio_discard_pending:
    "Audio discard acknowledgement is pending for the current discontinuity.",
  audio_rate_unsupported:
    "The selected rate cannot preserve audio meaning; audio is intentionally degraded.",
  viewport_output_unavailable:
    "Rendered viewport output is unavailable; timing-only playback remains observable.",
  audio_output_unavailable:
    "Audio output unavailable; timing and synchronization remain observable.",
});

export const VARIABLE_PLAYBACK_RATES = Object.freeze([
  { numerator: 1, denominator: 4, label: "1/4x" },
  { numerator: 1, denominator: 2, label: "1/2x" },
  { numerator: 1, denominator: 1, label: "1x" },
  { numerator: 2, denominator: 1, label: "2x" },
  { numerator: 4, denominator: 1, label: "4x" },
  { numerator: 8, denominator: 1, label: "8x" },
] as const);

export function playbackActionForKey(
  key: string,
  snapshot: EditorPlaybackSnapshot | null,
): PlaybackTransportAction | null {
  const normalized = key === " " ? "space" : key.toLowerCase();
  switch (normalized) {
    case "j":
      return nextShuttleAction("reverse", snapshot);
    case "k":
      return { action: "pause" };
    case "l":
      return nextShuttleAction("forward", snapshot);
    case "space":
      return snapshot?.mode === "playing"
        ? { action: "pause" }
        : { action: "play" };
    default:
      return null;
  }
}

export function nextShuttleAction(
  direction: PlaybackDirection,
  snapshot: EditorPlaybackSnapshot | null,
): PlaybackTransportAction {
  const sameDirection =
    snapshot !== null &&
    snapshot.mode === "playing" &&
    ((direction === "reverse" && snapshot.rate_numerator < 0) ||
      (direction === "forward" && snapshot.rate_numerator > 0));
  let magnitude = 1;
  if (sameDirection && snapshot !== null) {
    const currentMagnitude = Math.abs(snapshot.rate_numerator);
    magnitude =
      SHUTTLE_MAGNITUDES.find(
        (candidate) =>
          candidate * snapshot.rate_denominator > currentMagnitude,
      ) ?? SHUTTLE_MAGNITUDES[0];
  }
  return {
    action: "shuttle",
    numerator: direction === "reverse" ? -magnitude : magnitude,
    denominator: 1,
  };
}

export function formatExactPlaybackTime(time: EditorRationalTime): string {
  return `${time.value} @ ${time.timebase.numerator}/${time.timebase.denominator} units/s`;
}

export function formatExactRate(snapshot: EditorPlaybackSnapshot): string {
  const sign = snapshot.rate_numerator > 0 ? "+" : "";
  return `${sign}${snapshot.rate_numerator}/${snapshot.rate_denominator}x ${snapshot.direction}`;
}

export function playbackDegradationLabel(code: string): string {
  return DEGRADATION_LABELS[code] ?? `Unrecognized playback degradation: ${code}.`;
}

export function playbackVisualState(
  snapshot: EditorPlaybackSnapshot,
): string {
  if (snapshot.degradation.includes("viewport_output_unavailable")) {
    return "Timing-only degraded output; rendered viewport pixels are unavailable.";
  }
  if (snapshot.degradation.includes("viewport_backpressure")) {
    return "Rendered output is temporarily backpressured; transport timing is exact.";
  }
  if (snapshot.degradation.includes("frame_failure")) {
    return "Rendered output failed for the current frame; transport timing is exact.";
  }
  return "Rendered program output is available.";
}
