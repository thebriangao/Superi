import type {
  ApplicationNotificationState,
  ApplicationNotificationTone,
} from "./application-presentation.ts";

export type ApplicationInspectorCondition =
  | "ready"
  | "working"
  | "degraded"
  | "attention";

export type ApplicationInspectorGroupId =
  | "inspector"
  | "metadata"
  | "history"
  | "diagnostics";

export interface ApplicationInspectorRow {
  readonly id: string;
  readonly label: string;
  readonly value: string;
  readonly tone?: ApplicationNotificationTone | "neutral";
}

export interface ApplicationInspectorGroup {
  readonly id: ApplicationInspectorGroupId;
  readonly title: string;
  readonly summary: string;
  readonly rows: readonly ApplicationInspectorRow[];
}

export interface ApplicationInspectorEngineState {
  readonly condition: ApplicationInspectorCondition;
  readonly label: string;
  readonly detail: string;
}

export interface ApplicationInspectorModel {
  readonly engine: ApplicationInspectorEngineState;
  readonly groups: readonly ApplicationInspectorGroup[];
}

interface InspectorFailure {
  readonly category?: string;
  readonly recoverability?: string;
  readonly condition?: string;
  readonly code?: string;
  readonly title?: string;
  readonly action?: string;
}

interface InspectorPlaybackObservation {
  readonly mode: string;
  readonly epoch: number;
  readonly degradation: readonly string[];
  readonly failure: InspectorFailure | null;
}

interface InspectorEditorProject {
  readonly status: string;
  readonly transactionId: string | null;
  readonly commandSequence: number | null;
  readonly failure: InspectorFailure | null;
  readonly snapshot: {
    readonly schema_version: string;
    readonly project: {
      readonly project_id: string;
      readonly root_timeline_id: string;
      readonly project_revision: number;
      readonly history_capacity: number;
      readonly undo_depth: number;
      readonly redo_depth: number;
      readonly next_undo: string | null;
      readonly next_redo: string | null;
      readonly semantic_hash_algorithm: string;
      readonly semantic_hash_format_revision: number;
      readonly semantic_hash: string;
    };
    readonly playback:
      | { readonly status: "detached" }
      | {
          readonly status: "attached";
          readonly pending_command: boolean;
          readonly latest: InspectorPlaybackObservation | null;
        };
  } | null;
}

export interface ApplicationInspectorInput {
  readonly routeTitle: string;
  readonly focusedPanelTitle: string | null;
  readonly visiblePanelCount: number;
  readonly hiddenPanelCount: number;
  readonly workspaceRevision: number;
  readonly selectionSummary: readonly string[];
  readonly editorProject: InspectorEditorProject;
  readonly notificationState: ApplicationNotificationState;
  readonly commandFailure: string | null;
}

const MAX_DIAGNOSTIC_NOTICES = 8;

export function createApplicationInspectorModel(
  input: ApplicationInspectorInput,
): ApplicationInspectorModel {
  const snapshot = input.editorProject.snapshot;
  const project = snapshot?.project ?? null;
  const selection =
    input.selectionSummary.length === 0
      ? [row("selection", "Selection", "Nothing selected")]
      : input.selectionSummary.map((value, index) =>
          row(`selection-${index}`, index === 0 ? "Selection" : "Selected", value),
        );
  const inspector = group(
    "inspector",
    "Inspector",
    "Current workspace and shared selection intent.",
    [
      row("route", "Route", input.routeTitle),
      row("focus", "Focused panel", input.focusedPanelTitle ?? "No panel focused"),
      row("visible", "Visible panels", String(input.visiblePanelCount)),
      row("hidden", "Hidden panels", String(input.hiddenPanelCount)),
      ...selection,
    ],
  );
  const metadata = group(
    "metadata",
    "Metadata",
    "Read-only identity and freshness from the last-valid public editor snapshot.",
    project === null
      ? [row("project", "Project", "No public project snapshot available")]
      : [
          row("project", "Project ID", project.project_id),
          row("revision", "Project revision", String(project.project_revision)),
          row("root", "Root timeline", project.root_timeline_id),
          row(
            "semantic-hash",
            "Semantic identity",
            `${project.semantic_hash_algorithm} r${project.semantic_hash_format_revision} / ${project.semantic_hash}`,
          ),
          row(
            "schema",
            "Editor schema",
            snapshot!.schema_version,
          ),
        ],
  );
  const history = group(
    "history",
    "History",
    "Session history remains owned by the engine and is preserved through existing undo and redo routes.",
    project === null
      ? [row("history", "Project history", "Unavailable without a public project snapshot")]
      : [
          row("undo-depth", "Undo depth", String(project.undo_depth)),
          row("redo-depth", "Redo depth", String(project.redo_depth)),
          row("next-undo", "Next undo", project.next_undo ?? "Nothing to undo"),
          row("next-redo", "Next redo", project.next_redo ?? "Nothing to redo"),
          row("capacity", "History capacity", String(project.history_capacity)),
          row(
            "command-sequence",
            "Command sequence",
            input.editorProject.commandSequence === null
              ? "Not observed"
              : String(input.editorProject.commandSequence),
          ),
        ],
  );
  const diagnosticRows: ApplicationInspectorRow[] = [
    row("editor-status", "Editor state", input.editorProject.status),
    row("workspace-revision", "Workspace revision", String(input.workspaceRevision)),
    row(
      "transaction",
      "Editor transaction",
      input.editorProject.transactionId ?? "Not observed",
    ),
  ];
  if (input.editorProject.failure !== null) {
    diagnosticRows.push(
      row(
        "editor-failure",
        "Editor failure",
        failureText(input.editorProject.failure),
        "error",
      ),
    );
  }
  if (input.commandFailure !== null) {
    diagnosticRows.push(
      row("command-failure", "Command failure", input.commandFailure, "error"),
    );
  }
  for (const notification of input.notificationState.notifications.slice(
    -MAX_DIAGNOSTIC_NOTICES,
  )) {
    diagnosticRows.push(
      row(
        `notice-${notification.sequence}`,
        `Notice ${notification.sequence}`,
        notification.title,
        notification.tone,
      ),
    );
  }
  if (diagnosticRows.length === 3) {
    diagnosticRows.push(
      row("diagnostic-empty", "Operational notices", "No retained application notices"),
    );
  }
  const diagnostics = group(
    "diagnostics",
    "Diagnostics",
    "Bounded safe presentation evidence; authoritative recovery remains in System.",
    diagnosticRows,
  );

  return deepFreeze({
    engine: projectEngineState(input.editorProject),
    groups: [inspector, metadata, history, diagnostics],
  });
}

function projectEngineState(
  editorProject: InspectorEditorProject,
): ApplicationInspectorEngineState {
  const snapshot = editorProject.snapshot;
  if (snapshot === null) {
    if (["loading", "refreshing"].includes(editorProject.status)) {
      return {
        condition: "working",
        label: `Editor state ${editorProject.status}`,
        detail: "The last-valid application state remains visible while the public editor snapshot refreshes.",
      };
    }
    return {
      condition: editorProject.status === "failed" ? "attention" : "degraded",
      label: `Editor state ${editorProject.status}`,
      detail:
        editorProject.failure === null
          ? "No public editor snapshot is available. Open System for authoritative engine recovery."
          : failureText(editorProject.failure),
    };
  }
  const playback = snapshot.playback;
  if (["loading", "refreshing"].includes(editorProject.status)) {
    return {
      condition: "working",
      label: `Editor state ${editorProject.status}`,
      detail: `The last-valid playback state remains ${playbackLabel(playback)} while the public snapshot refreshes.`,
    };
  }
  if (editorProject.failure !== null || ["degraded", "failed"].includes(editorProject.status)) {
    return {
      condition: editorProject.status === "failed" ? "attention" : "degraded",
      label: `Editor state ${editorProject.status}; ${playbackLabel(playback)}`,
      detail:
        editorProject.failure === null
          ? "The last-valid playback observation remains visible while editor state is limited."
          : failureText(editorProject.failure),
    };
  }
  if (playback.status === "detached") {
    return {
      condition: "degraded",
      label: "Playback detached",
      detail: "The editor snapshot is valid, but no playback owner is attached.",
    };
  }
  if (playback.pending_command) {
    return {
      condition: "working",
      label: "Playback command pending",
      detail: "The playback owner has accepted intent and the last-valid observation remains visible.",
    };
  }
  if (playback.latest === null) {
    return {
      condition: "degraded",
      label: "Playback awaiting observation",
      detail: "The playback owner is attached, but no runtime observation has been published.",
    };
  }
  if (playback.latest.failure !== null) {
    return {
      condition: "attention",
      label: "Playback needs attention",
      detail: failureText(playback.latest.failure),
    };
  }
  if (playback.latest.degradation.length > 0) {
    return {
      condition: "degraded",
      label: `Playback ${playback.latest.mode}`,
      detail: `Epoch ${playback.latest.epoch}. Limitations: ${playback.latest.degradation.join(", ")}.`,
    };
  }
  return {
    condition: "ready",
    label: `Playback ${playback.latest.mode}`,
    detail: `Epoch ${playback.latest.epoch}. No playback degradation is reported.`,
  };
}

function playbackLabel(
  playback: InspectorEditorProject["snapshot"] extends infer Snapshot
    ? Snapshot extends { readonly playback: infer Playback }
      ? Playback
      : never
    : never,
): string {
  if (playback.status === "detached") return "playback detached";
  if (playback.pending_command) return "playback command pending";
  return playback.latest === null
    ? "playback awaiting observation"
    : `playback ${playback.latest.mode}`;
}

function failureText(failure: InspectorFailure): string {
  return [
    failure.title,
    failure.code,
    failure.category,
    failure.recoverability ?? failure.condition,
    failure.action,
  ]
    .filter((value): value is string => value !== undefined && value.length > 0)
    .join(" / ") || "A classified failure is present.";
}

function row(
  id: string,
  label: string,
  value: string,
  tone: ApplicationInspectorRow["tone"] = "neutral",
): ApplicationInspectorRow {
  return { id, label, value, tone };
}

function group(
  id: ApplicationInspectorGroupId,
  title: string,
  summary: string,
  rows: readonly ApplicationInspectorRow[],
): ApplicationInspectorGroup {
  return { id, title, summary, rows };
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
