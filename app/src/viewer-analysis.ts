export type ViewerAnalysisView =
  | "image"
  | "alpha"
  | "red"
  | "green"
  | "blue"
  | "luminance"
  | "false_color"
  | "clipping";

export interface ViewerAnalysisDefinition {
  readonly view: ViewerAnalysisView;
  readonly label: string;
  readonly description: string;
}

export const VIEWER_ANALYSIS_DEFINITIONS: readonly ViewerAnalysisDefinition[] =
  Object.freeze([
    definition("image", "Image", "Canonical image through the active display transform"),
    definition("alpha", "Alpha", "Straight alpha as an opaque neutral image"),
    definition("red", "Red", "Unassociated scene-linear red as a neutral image"),
    definition("green", "Green", "Unassociated scene-linear green as a neutral image"),
    definition("blue", "Blue", "Unassociated scene-linear blue as a neutral image"),
    definition("luminance", "Luminance", "Scene-linear CIE Y as a neutral image"),
    definition("false_color", "False color", "Fixed scene-linear exposure bands"),
    definition("clipping", "Clipping", "Under and over range in display-linear RGB"),
  ]);

export const DEFAULT_VIEWER_ANALYSIS_VIEW: ViewerAnalysisView = "image";

export function viewerAnalysisDefinition(
  view: ViewerAnalysisView,
): ViewerAnalysisDefinition {
  const definition = VIEWER_ANALYSIS_DEFINITIONS.find(
    (candidate) => candidate.view === view,
  );
  if (!definition) throw new Error(`Unknown viewer analysis view: ${view}`);
  return definition;
}

function definition(
  view: ViewerAnalysisView,
  label: string,
  description: string,
): ViewerAnalysisDefinition {
  return Object.freeze({ view, label, description });
}
