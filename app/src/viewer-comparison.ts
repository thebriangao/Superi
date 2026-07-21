const VIEWER_COMPARISON_CATALOG = [
  { mode: "single", label: "Single" },
  { mode: "compare", label: "Compare" },
  { mode: "split", label: "Split" },
  { mode: "wipe", label: "Wipe" },
  { mode: "difference", label: "Difference" },
  { mode: "reference", label: "Reference" },
  { mode: "snapshot", label: "Snapshot" },
] as const;

export type ViewerComparisonMode =
  (typeof VIEWER_COMPARISON_CATALOG)[number]["mode"];
export type ViewerComparisonOrientation = "vertical" | "horizontal";
export type ViewerComparisonRole = "source" | "program" | "composite" | "color";

export interface ViewerComparisonDefinition {
  readonly mode: ViewerComparisonMode;
  readonly label: string;
}

export const VIEWER_COMPARISON_DEFINITIONS: readonly ViewerComparisonDefinition[] =
  Object.freeze(
    VIEWER_COMPARISON_CATALOG.map((definition) =>
      Object.freeze({ ...definition }),
    ),
  );

export interface ViewerTemporalContext {
  readonly owner: "source" | "playback";
  readonly value: number;
  readonly timebaseNumerator: number;
  readonly timebaseDenominator: number;
}

export interface ViewerVisualIdentity {
  readonly phase: "presenting";
  readonly physicalWidth: number;
  readonly physicalHeight: number;
  readonly surfaceGeneration: number;
  readonly frameSequence: number;
  readonly displayIntent: string;
}

export interface ViewerFrameIdentity {
  readonly role: ViewerComparisonRole;
  readonly visual: ViewerVisualIdentity | null;
  readonly temporal: ViewerTemporalContext | null;
}

export interface ViewerVisualSnapshot {
  readonly role: ViewerComparisonRole;
  readonly phase: string;
  readonly physicalWidth: number;
  readonly physicalHeight: number;
  readonly surfaceGeneration: number;
  readonly frameSequence: number;
  readonly displayIntent: string;
}

export interface ViewerComparisonState {
  readonly mode: ViewerComparisonMode;
  readonly orientation: ViewerComparisonOrientation;
  readonly position: number;
  readonly reference: ViewerFrameIdentity | null;
  readonly snapshot: ViewerFrameIdentity | null;
}

export type ViewerComparisonAction =
  | { readonly action: "mode"; readonly mode: ViewerComparisonMode }
  | { readonly action: "capture_reference" }
  | { readonly action: "capture_snapshot" }
  | { readonly action: "position"; readonly position: number }
  | {
      readonly action: "orientation";
      readonly orientation: ViewerComparisonOrientation;
    };

const INITIAL_VIEWER_COMPARISON: ViewerComparisonState = Object.freeze({
  mode: "single",
  orientation: "vertical",
  position: 0.5,
  reference: null,
  snapshot: null,
});

export function initialViewerComparison(): ViewerComparisonState {
  return INITIAL_VIEWER_COMPARISON;
}

export function createViewerFrameIdentity(
  role: ViewerComparisonRole,
  snapshot: ViewerVisualSnapshot | null,
  temporal: ViewerTemporalContext | null,
): ViewerFrameIdentity {
  const displayIntent = snapshot?.displayIntent.trim() ?? "";
  const visual =
    snapshot?.role === role &&
    snapshot.phase === "presenting" &&
    Number.isSafeInteger(snapshot.physicalWidth) &&
    snapshot.physicalWidth > 0 &&
    Number.isSafeInteger(snapshot.physicalHeight) &&
    snapshot.physicalHeight > 0 &&
    Number.isSafeInteger(snapshot.surfaceGeneration) &&
    snapshot.surfaceGeneration > 0 &&
    Number.isSafeInteger(snapshot.frameSequence) &&
    snapshot.frameSequence > 0 &&
    displayIntent.length > 0
      ? Object.freeze({
          phase: "presenting" as const,
          physicalWidth: snapshot.physicalWidth,
          physicalHeight: snapshot.physicalHeight,
          surfaceGeneration: snapshot.surfaceGeneration,
          frameSequence: snapshot.frameSequence,
          displayIntent,
        })
      : null;
  const exactTemporal = validTemporalContext(temporal)
    ? Object.freeze({ ...temporal })
    : null;
  return Object.freeze({ role, visual, temporal: exactTemporal });
}

export function viewerComparisonAvailable(
  state: ViewerComparisonState,
  current: ViewerFrameIdentity,
  mode: ViewerComparisonMode,
): boolean {
  switch (mode) {
    case "single":
      return true;
    case "reference":
      return (
        state.reference !== null &&
        state.reference.role === current.role &&
        state.reference.visual !== null
      );
    case "snapshot":
      return (
        state.snapshot !== null &&
        state.snapshot.role === current.role &&
        state.snapshot.visual !== null
      );
    case "compare":
    case "split":
    case "wipe":
    case "difference":
      return (
        current.visual !== null &&
        state.reference !== null &&
        state.reference.role === current.role &&
        state.reference.visual !== null
      );
  }
}

export function applyViewerComparison(
  state: ViewerComparisonState,
  action: ViewerComparisonAction,
  current: ViewerFrameIdentity,
): ViewerComparisonState {
  switch (action.action) {
    case "mode":
      return viewerComparisonAvailable(state, current, action.mode)
        ? freezeState({ ...state, mode: action.mode })
        : state;
    case "capture_reference":
      return current.visual === null
        ? state
        : freezeState({ ...state, reference: cloneFrame(current) });
    case "capture_snapshot":
      return current.visual === null
        ? state
        : freezeState({ ...state, snapshot: cloneFrame(current) });
    case "position": {
      if (!Number.isFinite(action.position)) return state;
      const position = Math.min(0.95, Math.max(0.05, action.position));
      return freezeState({ ...state, position });
    }
    case "orientation":
      return freezeState({ ...state, orientation: action.orientation });
  }
}

export function comparisonUsesPosition(mode: ViewerComparisonMode): boolean {
  return mode === "split" || mode === "wipe";
}

export function formatViewerComparisonState(
  state: ViewerComparisonState,
  current: ViewerFrameIdentity,
): string {
  const label =
    VIEWER_COMPARISON_DEFINITIONS.find(({ mode }) => mode === state.mode)?.label ??
    "Single";
  if (state.mode === "single") {
    return `${label}: ${formatFrame("current", current)}.`;
  }
  if (state.mode === "reference") {
    return `${label}: ${formatOptionalFrame("reference", state.reference)}.`;
  }
  if (state.mode === "snapshot") {
    return `${label}: ${formatOptionalFrame("snapshot", state.snapshot)}.`;
  }
  const divider = comparisonUsesPosition(state.mode)
    ? `; ${state.orientation} boundary ${Math.round(state.position * 100)}%`
    : "";
  const availability = viewerComparisonAvailable(state, current, state.mode)
    ? ""
    : " unavailable";
  return `${label}${availability}: ${formatFrame("current", current)}; ${formatOptionalFrame("reference", state.reference)}${divider}.`;
}

function validTemporalContext(
  temporal: ViewerTemporalContext | null,
): temporal is ViewerTemporalContext {
  return (
    temporal !== null &&
    Number.isSafeInteger(temporal.value) &&
    Number.isSafeInteger(temporal.timebaseNumerator) &&
    temporal.timebaseNumerator > 0 &&
    Number.isSafeInteger(temporal.timebaseDenominator) &&
    temporal.timebaseDenominator > 0
  );
}

function freezeState(state: ViewerComparisonState): ViewerComparisonState {
  return Object.freeze(state);
}

function cloneFrame(frame: ViewerFrameIdentity): ViewerFrameIdentity {
  return createViewerFrameIdentity(
    frame.role,
    frame.visual === null ? null : { role: frame.role, ...frame.visual },
    frame.temporal,
  );
}

function formatOptionalFrame(
  label: "reference" | "snapshot",
  frame: ViewerFrameIdentity | null,
): string {
  return frame === null ? `${label} not captured` : formatFrame(label, frame);
}

function formatFrame(label: string, frame: ViewerFrameIdentity): string {
  const visual = frame.visual;
  const visualState = visual
    ? `${label} surface ${visual.surfaceGeneration} frame ${visual.frameSequence}, ${visual.physicalWidth}x${visual.physicalHeight}, ${visual.displayIntent}`
    : `${label} native frame unavailable`;
  const temporalState = frame.temporal
    ? `${frame.temporal.owner} context ${frame.temporal.value} @ ${frame.temporal.timebaseNumerator}/${frame.temporal.timebaseDenominator}, native frame binding unavailable`
    : "exact temporal coordinate unavailable";
  return `${visualState}, ${temporalState}`;
}
