export type ViewerOverlayKind =
  | "safe-area"
  | "guide"
  | "grid"
  | "ruler"
  | "center"
  | "aspect"
  | "custom";

export interface ViewerOverlayDefinition {
  readonly kind: ViewerOverlayKind;
  readonly label: string;
  readonly geometry?: Readonly<{
    readonly insetTop: number;
    readonly insetRight: number;
    readonly insetBottom: number;
    readonly insetLeft: number;
  }>;
}

export type ViewerOverlayVisibility = Readonly<Record<ViewerOverlayKind, boolean>>;

const customGeometry = Object.freeze({
  insetTop: 12.5,
  insetRight: 8,
  insetBottom: 12.5,
  insetLeft: 8,
});

export const OVERLAY_DEFINITIONS: readonly ViewerOverlayDefinition[] = Object.freeze([
  Object.freeze({ kind: "safe-area", label: "Safe area" }),
  Object.freeze({ kind: "guide", label: "Guides" }),
  Object.freeze({ kind: "grid", label: "Grid" }),
  Object.freeze({ kind: "ruler", label: "Rulers" }),
  Object.freeze({ kind: "center", label: "Center" }),
  Object.freeze({ kind: "aspect", label: "Aspect" }),
  Object.freeze({ kind: "custom", label: "Custom", geometry: customGeometry }),
]);

export function initialViewerOverlays(): ViewerOverlayVisibility {
  return Object.freeze({
    "safe-area": false,
    guide: false,
    grid: false,
    ruler: false,
    center: false,
    aspect: false,
    custom: false,
  });
}

export function toggleViewerOverlay(
  visibility: ViewerOverlayVisibility,
  kind: ViewerOverlayKind,
): ViewerOverlayVisibility {
  return Object.freeze({ ...visibility, [kind]: !visibility[kind] });
}

export function visibleViewerOverlays(
  visibility: ViewerOverlayVisibility,
): readonly ViewerOverlayDefinition[] {
  return Object.freeze(OVERLAY_DEFINITIONS.filter(({ kind }) => visibility[kind]));
}
