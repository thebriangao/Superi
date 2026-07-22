export const SCREEN_READER_SUPPORT_SCHEMA_VERSION = 1;

export const SCREEN_READER_SURFACE_ORDER = Object.freeze([
  "project",
  "media",
  "timeline",
  "inspector",
  "mixer",
  "graph",
  "scopes",
  "jobs",
  "dialogs",
] as const);

export type ScreenReaderSurfaceId =
  (typeof SCREEN_READER_SURFACE_ORDER)[number];

export interface ScreenReaderSurfaceSupport {
  readonly id: ScreenReaderSurfaceId;
  readonly descriptionId: string;
  readonly label: string;
  readonly interaction: "browse" | "edit" | "monitor" | "modal";
  readonly description: string;
}

function support(
  value: ScreenReaderSurfaceSupport,
): ScreenReaderSurfaceSupport {
  return Object.freeze(value);
}

export const SCREEN_READER_SURFACES = Object.freeze({
  project: support({
    id: "project",
    descriptionId: "screen-reader-project-help",
    label: "Project state and controls",
    interaction: "edit",
    description:
      "Project identity, revision, lifecycle state, failures, and available create, open, save, close, refresh, and selection actions are announced from the current project owner.",
  }),
  media: support({
    id: "media",
    descriptionId: "screen-reader-media-help",
    label: "Project media library",
    interaction: "edit",
    description:
      "Media search, view state, selection, source availability, metadata, transcript, and local intelligent results remain labelled and editable through ordinary project actions.",
  }),
  timeline: support({
    id: "timeline",
    descriptionId: "screen-reader-timeline-help",
    label: "Timeline editor",
    interaction: "edit",
    description:
      "Timeline items expose selection, track, timing, relationship, edit-target, and command status. Arrow navigation, range extension, selection, and exact edit controls remain available from the keyboard.",
  }),
  inspector: support({
    id: "inspector",
    descriptionId: "screen-reader-inspector-help",
    label: "Shared inspector",
    interaction: "browse",
    description:
      "Inspector groups expose labelled workspace, metadata, history, and diagnostic terms with current engine and action status from existing owners.",
  }),
  mixer: support({
    id: "mixer",
    descriptionId: "screen-reader-mixer-help",
    label: "Audio mixer state",
    interaction: "browse",
    description:
      "Audio tracks announce sample rate, ordered channels, destination, route mapping, clip count, continuity, and current project state without inferring live meter values.",
  }),
  graph: support({
    id: "graph",
    descriptionId: "screen-reader-graph-help",
    label: "Typed graph documents",
    interaction: "edit",
    description:
      "Graph documents expose stable identity, typed scope, revision, format, and content fingerprint. Graph edits remain deterministic typed project actions rather than local visual state.",
  }),
  scopes: support({
    id: "scopes",
    descriptionId: "screen-reader-scopes-help",
    label: "Color scopes and viewer analysis",
    interaction: "monitor",
    description:
      "Color analysis modes and active overlays are named controls with selected state, while viewer status describes exact monitor, transform, timing, and presentation evidence.",
  }),
  jobs: support({
    id: "jobs",
    descriptionId: "screen-reader-jobs-help",
    label: "Background jobs",
    interaction: "monitor",
    description:
      "Jobs expose category, status, determinate or indeterminate progress, detail, failure recovery, retry, and dismissal without replacing the authoritative operation result.",
  }),
  dialogs: support({
    id: "dialogs",
    descriptionId: "screen-reader-dialogs-help",
    label: "Application dialogs",
    interaction: "modal",
    description:
      "Dialogs announce their title and purpose, move focus inside, contain keyboard traversal when modal, close with Escape, and restore focus to the invoking control.",
  }),
});

export function screenReaderSurface(
  id: string,
): ScreenReaderSurfaceSupport {
  const candidate = SCREEN_READER_SURFACES[id as ScreenReaderSurfaceId];
  if (candidate === undefined) {
    throw new Error(`unknown screen-reader surface: ${id}`);
  }
  return candidate;
}
