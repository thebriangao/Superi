import type {
  EditorAudioContinuity,
  EditorAudioState,
  EditorAudioTrackState,
  EditorStateAudioChannelTarget,
  ExactTime,
  TimelineEditOperation,
} from "./api.ts";
import type {
  TimelineClipMulticamPresentation,
  TimelineClipPresentation,
} from "./timeline-clip-presentation.ts";
import type {
  TimelineEditingTool,
  TimelineEditPlan,
} from "./timeline-editing.ts";
import {
  formatTimelineTime,
  type TimelineCanvasModel,
  type TimelineCanvasTrack,
  type TimelineExactRange,
} from "./timeline-workspace.ts";

export type TimelineEditorialFeedbackPhase =
  | "unavailable"
  | "idle"
  | "preview"
  | "applying"
  | "applied"
  | "failed";

export interface TimelineEditorialTarget {
  readonly trackId: string;
  readonly itemId: string;
}

export interface TimelineViewerFeedback {
  readonly phase: TimelineEditorialFeedbackPhase;
  readonly title: string;
  readonly coordinate: string | null;
  readonly detail: string;
  readonly clipId: string | null;
  readonly multicam: TimelineClipMulticamPresentation | null;
}

export type TimelineAudioAudibility =
  | "audible"
  | "disabled"
  | "muted"
  | "solo_suppressed"
  | "unavailable";

export type TimelineAudioRouteState =
  | "routed"
  | "muted"
  | "unrouted"
  | "disabled"
  | "solo_suppressed"
  | "unavailable";

export interface TimelineAudioRouteFeedback {
  readonly source: string;
  readonly target: string | null;
  readonly state: TimelineAudioRouteState;
}

export interface TimelineAudioSeamFeedback {
  readonly leftClipId: string;
  readonly rightClipId: string;
  readonly recordKind: "seamless" | "gap" | "overlap";
  readonly recordSampleCount: number | null;
  readonly sourceKind: "continuous" | "discontinuous" | "different_clip";
  readonly sourceExpected: number | null;
  readonly sourceActual: number | null;
  readonly sourceLeft: string | null;
  readonly sourceRight: string | null;
}

export type TimelineAudioContinuityFeedback =
  | {
      readonly status: "audited";
      readonly uninterruptedRecordCoverage: boolean;
      readonly seams: readonly TimelineAudioSeamFeedback[];
    }
  | {
      readonly status: "unsupported";
      readonly reason: string;
      readonly seams: readonly [];
    };

export interface TimelineAudioTrackFeedback {
  readonly timelineId: string;
  readonly trackId: string;
  readonly sampleRate: number;
  readonly sourceChannels: readonly string[];
  readonly destination: string;
  readonly destinationChannels: readonly string[];
  readonly routes: readonly TimelineAudioRouteFeedback[];
  readonly clipCount: number;
  readonly audibility: TimelineAudioAudibility;
  readonly signalStatus: "unobserved";
  readonly continuity: TimelineAudioContinuityFeedback;
}

export interface TimelineAudioFeedback {
  readonly signalStatus: "unobserved";
  readonly message: string;
  readonly tracks: readonly TimelineAudioTrackFeedback[];
}

export interface TimelineEditorialFeedback {
  readonly phase: TimelineEditorialFeedbackPhase;
  readonly tool: TimelineEditingTool;
  readonly message: string;
  readonly source: TimelineViewerFeedback;
  readonly program: TimelineViewerFeedback;
  readonly audio: TimelineAudioFeedback;
}

export interface TimelineEditorialFeedbackOptions {
  readonly model: TimelineCanvasModel;
  readonly clips: readonly TimelineClipPresentation[];
  readonly audio: EditorAudioState;
  readonly tool: TimelineEditingTool;
  readonly target: TimelineEditorialTarget | null;
  readonly plan: TimelineEditPlan | null;
  readonly playheadSeconds: number;
  readonly phase: TimelineEditorialFeedbackPhase;
  readonly message: string;
}

export function projectTimelineEditorialFeedback(
  options: TimelineEditorialFeedbackOptions,
): TimelineEditorialFeedback {
  const clip = activeClip(options);
  const operation = activeOperation(options.plan, clip?.id ?? options.target?.itemId ?? null);
  const viewer = viewerFeedback(options, clip, operation);
  return deepFreeze({
    phase: options.phase,
    tool: options.tool,
    message: options.message,
    source: { ...viewer.source, phase: options.phase },
    program: { ...viewer.program, phase: options.phase },
    audio: projectAudioFeedback(options.model, options.audio),
  });
}

function activeClip(
  options: TimelineEditorialFeedbackOptions,
): TimelineClipPresentation | null {
  const targetId = options.target?.itemId;
  if (targetId) {
    const target = options.clips.find((clip) => clip.id === targetId);
    if (target) return target;
  }
  const selected = options.clips.find((clip) => clip.canonicalSelected);
  if (selected) return selected;
  return (
    options.clips.find(
      (clip) =>
        options.playheadSeconds >= clip.startSeconds &&
        options.playheadSeconds < clip.endSeconds,
    ) ?? null
  );
}

function activeOperation(
  plan: TimelineEditPlan | null,
  clipId: string | null,
): TimelineEditOperation | null {
  if (!plan) return null;
  if (!clipId) return plan.operations[0] ?? null;
  return plan.operations.find((operation) => operationTargetsClip(operation, clipId)) ?? null;
}

function operationTargetsClip(
  operation: TimelineEditOperation,
  clipId: string,
): boolean {
  switch (operation.operation) {
    case "slip":
    case "slide":
    case "retime":
      return operation.clip_id === clipId;
    case "trim":
    case "ripple":
    case "extend":
      return operation.target_id.kind === "clip" && operation.target_id.id === clipId;
    case "roll":
      return (
        (operation.left_id.kind === "clip" && operation.left_id.id === clipId) ||
        (operation.right_id.kind === "clip" && operation.right_id.id === clipId)
      );
    case "razor":
      return operation.target_id.kind === "clip" && operation.target_id.id === clipId;
    case "replace":
      return operation.target_id.kind === "clip" && operation.target_id.id === clipId;
    default:
      return false;
  }
}

function viewerFeedback(
  options: TimelineEditorialFeedbackOptions,
  clip: TimelineClipPresentation | null,
  operation: TimelineEditOperation | null,
): {
  readonly source: Omit<TimelineViewerFeedback, "phase">;
  readonly program: Omit<TimelineViewerFeedback, "phase">;
} {
  if (!clip) {
    return {
      source: {
        title: "Source unavailable",
        coordinate: null,
        detail: "Select a canonical clip to inspect source consequences.",
        clipId: null,
        multicam: null,
      },
      program: {
        title: "Program playhead",
        coordinate: formatTimelineTime(options.playheadSeconds, options.model.editRate),
        detail: options.message,
        clipId: null,
        multicam: null,
      },
    };
  }

  const common = { clipId: clip.id, multicam: cloneMulticam(clip.multicam) };
  if (operation?.operation === "slip") {
    return {
      source: {
        ...common,
        title: "Slip source",
        coordinate: formatApiPoint(operation.source_start),
        detail: `${clip.name} proposes a new source start while its record placement stays fixed.`,
      },
      program: {
        ...common,
        title: "Record range held",
        coordinate: formatExactRange(clip.recordRange),
        detail: `${clip.trackName} retains the complete canonical record range.`,
      },
    };
  }
  if (operation?.operation === "slide") {
    return {
      source: {
        ...common,
        title: "Source range held",
        coordinate: formatExactRange(clip.sourceRange),
        detail: `${clip.name} retains its complete source range during the slide.`,
      },
      program: {
        ...common,
        title: "Slide record start",
        coordinate: formatApiPoint(operation.to),
        detail: `${options.plan?.affectedItemIds.length ?? 1} canonical objects participate in this exact placement preview.`,
      },
    };
  }
  if (operation?.operation === "trim") {
    return {
      source: {
        ...common,
        title: `Trim ${operation.side} source`,
        coordinate: formatExactRange(clip.sourceRange),
        detail: "The canonical source range is retained as pre-edit evidence until project history commits the new boundary.",
      },
      program: {
        ...common,
        title: `Trim ${operation.side} boundary`,
        coordinate: formatApiPoint(operation.to),
        detail: `${clip.name} previews the exact ${operation.side} record boundary.`,
      },
    };
  }

  return {
    source: {
      ...common,
      title: clip.multicam ? "Multicam source context" : "Source context",
      coordinate: formatExactRange(clip.sourceRange),
      detail: sourceDescription(clip),
    },
    program: {
      ...common,
      title: operation ? options.plan?.label ?? "Edit preview" : "Program range",
      coordinate: formatExactRange(clip.recordRange),
      detail: options.message,
    },
  };
}

function sourceDescription(clip: TimelineClipPresentation): string {
  const source = clip.source;
  if (source.kind === "timeline") {
    return `${clip.name} reads nested timeline ${source.name}.`;
  }
  return `${clip.name} reads ${source.name} with ${source.relinkStatus} source identity.`;
}

function cloneMulticam(
  multicam: TimelineClipMulticamPresentation | null,
): TimelineClipMulticamPresentation | null {
  if (!multicam) return null;
  return {
    syncMethod: multicam.syncMethod,
    switchCount: multicam.switchCount,
    audioPolicy: multicam.audioPolicy,
    angles: multicam.angles.map((angle) => ({
      id: angle.id,
      name: angle.name,
      cameraLabel: angle.cameraLabel,
      enabled: angle.enabled,
      sourceClipIds: [...angle.sourceClipIds],
    })),
    switches: multicam.switches.map((multicamSwitch) => ({
      sourceRange: cloneExactRange(multicamSwitch.sourceRange),
      angleId: multicamSwitch.angleId,
    })),
    audioPolicyDetail:
      multicam.audioPolicyDetail.kind === "fixed"
        ? {
            kind: "fixed",
            angleId: multicam.audioPolicyDetail.angleId,
          }
        : { kind: multicam.audioPolicyDetail.kind },
  };
}

function cloneExactRange(range: TimelineExactRange): TimelineExactRange {
  return {
    start: {
      value: range.start.value,
      timebase: { ...range.start.timebase },
    },
    duration: {
      value: range.duration.value,
      timebase: { ...range.duration.timebase },
    },
  };
}

function projectAudioFeedback(
  model: TimelineCanvasModel,
  audio: EditorAudioState,
): TimelineAudioFeedback {
  const timelineAudioTracks = model.tracks.filter((track) => track.kind === "audio");
  const trackById = new Map(timelineAudioTracks.map((track) => [track.id, track]));
  const soloActive = timelineAudioTracks.some(
    (track) => track.enabled && track.solo,
  );
  const tracks = audio.tracks
    .filter((track) => track.timeline_id === model.id)
    .map((track) => projectAudioTrack(track, trackById.get(track.track_id), soloActive));
  return {
    signalStatus: "unobserved",
    message:
      "Routing, audibility, sample timing, and continuity are canonical. Live signal level is not observed by the editor snapshot.",
    tracks,
  };
}

function projectAudioTrack(
  track: EditorAudioTrackState,
  canvasTrack: TimelineCanvasTrack | undefined,
  soloActive: boolean,
): TimelineAudioTrackFeedback {
  const audibility = trackAudibility(canvasTrack, soloActive);
  const routeBySource = new Map(track.routes.map((route) => [route.source, route.target]));
  const routes = track.source_channels.map((source) =>
    projectAudioRoute(source, routeBySource.get(source), audibility),
  );
  return {
    timelineId: track.timeline_id,
    trackId: track.track_id,
    sampleRate: track.sample_rate,
    sourceChannels: [...track.source_channels],
    destination:
      track.destination.kind === "main"
        ? "main"
        : `track:${track.destination.track_id}`,
    destinationChannels: [...track.destination_channels],
    routes,
    clipCount: track.clip_count,
    audibility,
    signalStatus: "unobserved",
    continuity: projectContinuity(track.continuity),
  };
}

function trackAudibility(
  track: TimelineCanvasTrack | undefined,
  soloActive: boolean,
): TimelineAudioAudibility {
  if (!track) return "unavailable";
  if (!track.enabled) return "disabled";
  if (track.muted) return "muted";
  if (soloActive && !track.solo) return "solo_suppressed";
  return "audible";
}

function projectAudioRoute(
  source: string,
  target: EditorStateAudioChannelTarget | undefined,
  audibility: TimelineAudioAudibility,
): TimelineAudioRouteFeedback {
  if (audibility === "unavailable") {
    return {
      source,
      target: targetLabel(target),
      state: "unavailable",
    };
  }
  if (audibility === "disabled" || audibility === "muted") {
    return {
      source,
      target: targetLabel(target),
      state: "disabled",
    };
  }
  if (audibility === "solo_suppressed") {
    return {
      source,
      target: targetLabel(target),
      state: "solo_suppressed",
    };
  }
  if (!target) return { source, target: null, state: "unrouted" };
  if (target.kind === "muted") return { source, target: null, state: "muted" };
  return { source, target: target.channel, state: "routed" };
}

function targetLabel(target: EditorStateAudioChannelTarget | undefined): string | null {
  return target?.kind === "channel" ? target.channel : null;
}

function projectContinuity(
  continuity: EditorAudioContinuity,
): TimelineAudioContinuityFeedback {
  if (continuity.status === "unsupported") {
    return { status: "unsupported", reason: continuity.reason, seams: [] };
  }
  return {
    status: "audited",
    uninterruptedRecordCoverage: continuity.uninterrupted_record_coverage,
    seams: continuity.seams.map((seam) => ({
      leftClipId: seam.left_clip_id,
      rightClipId: seam.right_clip_id,
      recordKind: seam.record.kind,
      recordSampleCount:
        seam.record.kind === "seamless" ? null : seam.record.sample_count,
      sourceKind: seam.source.kind,
      sourceExpected:
        seam.source.kind === "discontinuous" ? seam.source.expected : null,
      sourceActual:
        seam.source.kind === "discontinuous" ? seam.source.actual : null,
      sourceLeft:
        seam.source.kind === "different_clip" ? seam.source.left : null,
      sourceRight:
        seam.source.kind === "different_clip" ? seam.source.right : null,
    })),
  };
}

function formatApiPoint(point: ExactTime): string {
  return `${point.value} @ ${point.timebase.numerator}/${point.timebase.denominator}`;
}

function formatExactRange(range: TimelineExactRange): string {
  return `${range.start.value}+${range.duration.value} @ ${range.start.timebase.numerator}/${range.start.timebase.denominator}`;
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
