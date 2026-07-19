export type ViewerScaleMode = "fit" | "zoom" | "pixel";
export type ViewerPresentationMode = "normal" | "fullscreen" | "cinema";
export type ViewerNavigationRole = "source" | "program" | "composite" | "color";

export interface ViewerNavigationState {
  readonly scaleMode: ViewerScaleMode;
  readonly scale: number;
  readonly panX: number;
  readonly panY: number;
  readonly presentation: ViewerPresentationMode;
  readonly externalDisplayIntent: string;
}

export type ViewerNavigationAction =
  | { readonly action: "fit" }
  | { readonly action: "pixel" }
  | { readonly action: "zoom"; readonly factor: number }
  | { readonly action: "pan"; readonly deltaX: number; readonly deltaY: number }
  | { readonly action: "presentation"; readonly mode: ViewerPresentationMode };

const MIN_SCALE = 0.0625;
const MAX_SCALE = 16;

export function initialViewerNavigation(
  role: ViewerNavigationRole,
): ViewerNavigationState {
  return freezeNavigation({
    scaleMode: "fit",
    scale: 1,
    panX: 0,
    panY: 0,
    presentation: "normal",
    externalDisplayIntent: `${role}-managed-display`,
  });
}

export function applyViewerNavigation(
  state: ViewerNavigationState,
  action: ViewerNavigationAction,
): ViewerNavigationState {
  if (action.action === "fit") {
    return freezeNavigation({ ...state, scaleMode: "fit", scale: 1, panX: 0, panY: 0 });
  }
  if (action.action === "pixel") {
    return freezeNavigation({ ...state, scaleMode: "pixel", scale: 1, panX: 0, panY: 0 });
  }
  if (action.action === "zoom") {
    const factor = finite(action.factor, "zoom factor");
    const scale = Math.min(MAX_SCALE, Math.max(MIN_SCALE, state.scale * factor));
    return freezeNavigation({ ...state, scaleMode: "zoom", scale });
  }
  if (action.action === "pan") {
    return freezeNavigation({
      ...state,
      panX: state.panX + finite(action.deltaX, "horizontal pan"),
      panY: state.panY + finite(action.deltaY, "vertical pan"),
    });
  }
  return freezeNavigation({ ...state, presentation: action.mode });
}

export function viewerTransform(state: ViewerNavigationState): {
  readonly transform: string;
  readonly imageRendering: "auto" | "pixelated";
} {
  return Object.freeze({
    transform: `translate3d(${state.panX}px, ${state.panY}px, 0) scale(${state.scale})`,
    imageRendering: state.scaleMode === "pixel" ? "pixelated" : "auto",
  });
}

function finite(value: number, label: string): number {
  if (!Number.isFinite(value)) throw new Error(`${label} must be finite`);
  return value;
}

function freezeNavigation(state: ViewerNavigationState): ViewerNavigationState {
  return Object.freeze(state);
}
