export const ACCESSIBILITY_SEMANTICS_SCHEMA_VERSION = 1;

export type ApplicationSemanticRole =
  | "navigation"
  | "toolbar"
  | "region"
  | "status"
  | "log";

export interface ApplicationSemanticSurface {
  readonly id: string;
  readonly role: ApplicationSemanticRole;
  readonly label?: string;
  readonly labelledBy?: string;
  readonly describedBy?: string;
  readonly controls: readonly string[];
  readonly live: "off" | "polite" | "assertive";
  readonly atomic: boolean;
}

function surface(
  value: Omit<ApplicationSemanticSurface, "controls"> & {
    readonly controls?: readonly string[];
  },
): ApplicationSemanticSurface {
  return Object.freeze({
    ...value,
    controls: Object.freeze([...(value.controls ?? [])]),
  });
}

export const APPLICATION_SEMANTIC_SURFACES = Object.freeze({
  routes: surface({
    id: "application-routes",
    role: "navigation",
    label: "Application routes",
    controls: ["active-workflow"],
    live: "off",
    atomic: false,
  }),
  workspaceControls: surface({
    id: "workspace-controls",
    role: "toolbar",
    label: "Workspace controls",
    controls: ["active-workflow"],
    live: "off",
    atomic: false,
  }),
  activeWorkflow: surface({
    id: "active-workflow",
    role: "region",
    labelledBy: "route-title",
    describedBy: "active-workflow-status",
    live: "off",
    atomic: false,
  }),
  activeWorkflowStatus: surface({
    id: "active-workflow-status",
    role: "status",
    label: "Active workflow focus",
    live: "polite",
    atomic: true,
  }),
  notifications: surface({
    id: "application-notifications",
    role: "log",
    label: "Recent application notifications",
    live: "polite",
    atomic: false,
  }),
  applicationStatus: surface({
    id: "application-status",
    role: "status",
    label: "Application status",
    live: "polite",
    atomic: true,
  }),
  intelligentResults: surface({
    id: "media-content-analysis",
    role: "region",
    labelledBy: "media-content-analysis-title",
    describedBy:
      "media-content-analysis-description media-content-analysis-status",
    live: "off",
    atomic: false,
  }),
  intelligentResultsStatus: surface({
    id: "media-content-analysis-status",
    role: "status",
    label: "Content analysis state",
    live: "polite",
    atomic: true,
  }),
});

export type ApplicationSemanticSurfaceId =
  keyof typeof APPLICATION_SEMANTIC_SURFACES;

export function applicationSemanticSurface(
  id: string,
): ApplicationSemanticSurface {
  const candidate = APPLICATION_SEMANTIC_SURFACES[
    id as ApplicationSemanticSurfaceId
  ];
  if (candidate === undefined) {
    throw new Error(`unknown application semantic surface: ${id}`);
  }
  return candidate;
}
