import type { EditorProjectState, ProjectMutationKind } from "./api.ts";

const PROJECT_MUTATION_KINDS = new Set<ProjectMutationKind>([
  "project_settings",
  "compound",
  "set_media_path",
  "mark_media_missing",
  "consider_media_relink",
  "upsert_extension",
  "remove_extension",
  "set_extension_lifecycle",
  "set_extension_capabilities",
  "record_extension_failure",
  "clear_extension_failure",
  "extension_state",
  "unknown",
]);

export type ProjectHistoryCondition =
  | "no_project"
  | "synchronizing"
  | "busy"
  | "ready";

export interface ProjectHistoryDocument {
  readonly path: string;
  readonly project_id: string;
  readonly root_timeline_id: string;
  readonly project_revision: number;
}

export type ProjectHistoryEditorState = Pick<
  EditorProjectState,
  | "project_id"
  | "root_timeline_id"
  | "project_revision"
  | "undo_depth"
  | "redo_depth"
  | "next_undo"
  | "next_redo"
>;

export interface ProjectHistoryActionPresentation {
  readonly command: "undo" | "redo";
  readonly depth: number;
  readonly mutationKind: ProjectMutationKind | null;
  readonly title: string;
  readonly detail: string;
  readonly enabled: boolean;
  readonly disabledReason: string | null;
}

export interface ProjectHistoryPresentation {
  readonly condition: ProjectHistoryCondition;
  readonly documentLabel: string | null;
  readonly projectId: string | null;
  readonly projectRevision: number | null;
  readonly sessionOnly: true;
  readonly undo: ProjectHistoryActionPresentation;
  readonly redo: ProjectHistoryActionPresentation;
  readonly closeUndoDepth: number;
  readonly closeRedoDepth: number;
  readonly status: string;
}

export function projectMutationLabel(kind: string): string {
  switch (normalizeProjectMutationKind(kind)) {
    case "project_settings":
      return "Project Settings";
    case "compound":
      return "Compound Edit";
    case "set_media_path":
      return "Media Path Change";
    case "mark_media_missing":
      return "Missing Media Change";
    case "consider_media_relink":
      return "Media Relink";
    case "upsert_extension":
      return "Extension Update";
    case "remove_extension":
      return "Extension Removal";
    case "set_extension_lifecycle":
      return "Extension Lifecycle Change";
    case "set_extension_capabilities":
      return "Extension Permission Change";
    case "record_extension_failure":
      return "Extension Failure Record";
    case "clear_extension_failure":
      return "Extension Failure Clear";
    case "extension_state":
      return "Extension State Change";
    case "unknown":
      return "Project Change";
  }
}

export function projectHistoryPresentation(input: {
  readonly active: ProjectHistoryDocument | null;
  readonly editorProject: ProjectHistoryEditorState | null;
  readonly busy: boolean;
}): ProjectHistoryPresentation {
  const { active, editorProject } = input;
  const projectMatches =
    active !== null &&
    editorProject !== null &&
    active.project_id === editorProject.project_id &&
    active.root_timeline_id === editorProject.root_timeline_id;
  const closeUndoDepth = projectMatches
    ? historyDepth(editorProject.undo_depth)
    : 0;
  const closeRedoDepth = projectMatches
    ? historyDepth(editorProject.redo_depth)
    : 0;
  const nextUndo = projectMatches
    ? normalizeOptionalProjectMutationKind(editorProject.next_undo)
    : null;
  const nextRedo = projectMatches
    ? normalizeOptionalProjectMutationKind(editorProject.next_redo)
    : null;
  const revisionMatches =
    projectMatches &&
    active.project_revision === editorProject.project_revision;
  const historyCoherent =
    projectMatches &&
    coherentAction(closeUndoDepth, editorProject.next_undo) &&
    coherentAction(closeRedoDepth, editorProject.next_redo);
  const condition: ProjectHistoryCondition =
    active === null
      ? "no_project"
      : !revisionMatches || !historyCoherent
        ? "synchronizing"
        : input.busy
          ? "busy"
          : "ready";
  const documentLabel = active === null ? null : pathBasename(active.path);
  const undo = historyAction({
    command: "undo",
    condition,
    depth: closeUndoDepth,
    mutationKind: nextUndo,
    documentLabel,
  });
  const redo = historyAction({
    command: "redo",
    condition,
    depth: closeRedoDepth,
    mutationKind: nextRedo,
    documentLabel,
  });
  const status = historyStatus({
    condition,
    documentLabel,
    projectRevision: active?.project_revision ?? null,
    undoDepth: closeUndoDepth,
    redoDepth: closeRedoDepth,
  });

  return Object.freeze({
    condition,
    documentLabel,
    projectId: active?.project_id ?? null,
    projectRevision: active?.project_revision ?? null,
    sessionOnly: true,
    undo,
    redo,
    closeUndoDepth,
    closeRedoDepth,
    status,
  });
}

function historyAction(input: {
  readonly command: "undo" | "redo";
  readonly condition: ProjectHistoryCondition;
  readonly depth: number;
  readonly mutationKind: ProjectMutationKind | null;
  readonly documentLabel: string | null;
}): ProjectHistoryActionPresentation {
  const verb = input.command === "undo" ? "Undo" : "Redo";
  const direction = input.command === "undo" ? "undo" : "redo";
  const mutationLabel =
    input.mutationKind === null
      ? "Project Change"
      : projectMutationLabel(input.mutationKind);
  const countLabel = input.depth === 1 ? "transaction is" : "transactions are";
  const projectLabel = input.documentLabel ?? "the active project";
  const disabledReason = actionDisabledReason(
    input.condition,
    input.command,
    input.depth,
  );

  return Object.freeze({
    command: input.command,
    depth: input.depth,
    mutationKind: input.mutationKind,
    title: `${verb} ${mutationLabel}`,
    detail: `${input.depth} ${countLabel} available to ${direction} in ${projectLabel}. History ends when this project session closes.`,
    enabled: disabledReason === null,
    disabledReason,
  });
}

function actionDisabledReason(
  condition: ProjectHistoryCondition,
  command: "undo" | "redo",
  depth: number,
): string | null {
  switch (condition) {
    case "no_project":
      return "Open a project first.";
    case "synchronizing":
      return "Wait for project transaction history to synchronize.";
    case "busy":
      return "Wait for the current project operation to finish.";
    case "ready":
      return depth > 0
        ? null
        : `No project change is available to ${command}.`;
  }
}

function historyStatus(input: {
  readonly condition: ProjectHistoryCondition;
  readonly documentLabel: string | null;
  readonly projectRevision: number | null;
  readonly undoDepth: number;
  readonly redoDepth: number;
}): string {
  if (input.condition === "no_project") {
    return "No project is open. Transaction history is session-only.";
  }
  const identity = `${input.documentLabel ?? "Active project"} revision ${input.projectRevision ?? 0}`;
  if (input.condition === "synchronizing") {
    return `${identity}. Transaction history is synchronizing; last-known session-only counts are ${input.undoDepth} undo and ${input.redoDepth} redo.`;
  }
  if (input.condition === "busy") {
    return `${identity}. A project operation is active; ${input.undoDepth} undo and ${input.redoDepth} redo transactions remain session-only.`;
  }
  return `${identity}. ${input.undoDepth} undo and ${input.redoDepth} redo transactions are available in session-only history.`;
}

function coherentAction(depth: number, mutation: string | null): boolean {
  return depth === 0 ? mutation === null : mutation !== null;
}

function historyDepth(value: number): number {
  return Number.isSafeInteger(value) && value >= 0 ? value : 0;
}

function normalizeOptionalProjectMutationKind(
  kind: string | null,
): ProjectMutationKind | null {
  return kind === null ? null : normalizeProjectMutationKind(kind);
}

function normalizeProjectMutationKind(kind: string): ProjectMutationKind {
  return PROJECT_MUTATION_KINDS.has(kind as ProjectMutationKind)
    ? (kind as ProjectMutationKind)
    : "unknown";
}

function pathBasename(path: string): string {
  return path.split(/[\\/]/u).filter(Boolean).at(-1) ?? path;
}
