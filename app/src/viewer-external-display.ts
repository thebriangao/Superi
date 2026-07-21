import type { ViewerAnalysisView } from "./viewer-analysis.ts";

export interface ViewerExternalDisplayTarget {
  readonly id: string;
  readonly name: string;
  readonly positionX: number;
  readonly positionY: number;
  readonly physicalWidth: number;
  readonly physicalHeight: number;
  readonly scaleFactor: number;
  readonly primary: boolean;
}

export interface ViewerExternalDisplaySelection {
  readonly targetId: string | null;
}

export type ViewerExternalDisplayPhase =
  | "inactive"
  | "queued"
  | "presenting"
  | "unavailable"
  | "failed";

export interface ViewerExternalOutputSnapshot {
  readonly phase: ViewerExternalDisplayPhase;
  readonly targetId: string | null;
  readonly targetName: string | null;
  readonly selectedView: ViewerAnalysisView;
  readonly presentedView: ViewerAnalysisView | null;
  readonly physicalWidth: number;
  readonly physicalHeight: number;
  readonly scaleFactor: number;
  readonly surfaceGeneration: number;
  readonly frameSequence: number;
  readonly displayIntent: string;
  readonly summary: string | null;
}

export const INITIAL_VIEWER_EXTERNAL_DISPLAY_SELECTION: ViewerExternalDisplaySelection =
  Object.freeze({ targetId: null });

export function selectViewerExternalDisplay(
  state: ViewerExternalDisplaySelection,
  targetId: string | null,
  targets: readonly ViewerExternalDisplayTarget[],
): ViewerExternalDisplaySelection {
  if (targetId === null) return INITIAL_VIEWER_EXTERNAL_DISPLAY_SELECTION;
  if (!targets.some((target) => target.id === targetId)) {
    throw new Error("External display target is not available.");
  }
  return state.targetId === targetId
    ? state
    : Object.freeze({ targetId });
}

export function reconcileViewerExternalDisplaySelection(
  state: ViewerExternalDisplaySelection,
  targets: readonly ViewerExternalDisplayTarget[],
): ViewerExternalDisplaySelection {
  return state.targetId === null || targets.some(({ id }) => id === state.targetId)
    ? state
    : INITIAL_VIEWER_EXTERNAL_DISPLAY_SELECTION;
}

export function formatViewerExternalDisplayOutput(
  output: ViewerExternalOutputSnapshot,
): string {
  if (output.targetId === null || output.targetName === null) {
    return `External display ${output.phase}; ${sentence(
      output.summary ?? "No external display selected.",
    )}`;
  }
  const presented = output.presentedView ?? "none";
  const details =
    `External ${output.targetName}; ${output.phase} ` +
    `${output.physicalWidth}x${output.physicalHeight} @ ${formatScale(output.scaleFactor)}x; ` +
    `selected ${output.selectedView}; presented ${presented}; ` +
    `surface ${output.surfaceGeneration} frame ${output.frameSequence}; ` +
    `${output.displayIntent}.`;
  return output.summary === null
    ? details
    : `${details} ${sentence(output.summary)}`;
}

function formatScale(scaleFactor: number): string {
  return Number.isInteger(scaleFactor)
    ? scaleFactor.toString()
    : scaleFactor.toFixed(3).replace(/0+$/, "").replace(/\.$/, "");
}

function sentence(value: string): string {
  const trimmed = value.trim();
  return /[.!?]$/.test(trimmed) ? trimmed : `${trimmed}.`;
}
