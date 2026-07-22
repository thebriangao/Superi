import type {
  EditorExportJobSnapshot,
  PublicResourceReference,
  Recoverability,
  TraceValue,
} from "./api.ts";
import type { DesktopCrashDiagnostic } from "./crash-diagnostics.ts";
import type { DesktopLifecycleSnapshot } from "./lifecycle.ts";
import type { DesktopProjectFailure } from "./project-lifecycle.ts";
import type { DesktopTransportFailure } from "./transport.ts";

export type ApplicationFailureCondition = Recoverability;

export type ApplicationRecoveryIntent =
  | "retry"
  | "continue_degraded"
  | "correct"
  | "restart";

export interface ApplicationRecoveryPolicy {
  readonly intent: ApplicationRecoveryIntent;
  readonly label: string;
  readonly summary: string;
}

export interface ApplicationFeedbackContext {
  readonly label: string;
  readonly value: string;
}

export interface ApplicationFailurePresentation {
  readonly id: string;
  readonly source: string;
  readonly revision: number;
  readonly condition: ApplicationFailureCondition;
  readonly category: string;
  readonly code: string;
  readonly title: string;
  readonly action: string;
  readonly contexts: readonly ApplicationFeedbackContext[];
  readonly lastValidResource: PublicResourceReference | null;
  readonly primaryAction: ApplicationRecoveryPolicy;
}

export interface ApplicationFailureMessage {
  readonly id: string;
  readonly source: string;
  readonly revision: number;
  readonly condition: string;
  readonly category?: string;
  readonly code: string;
  readonly title: string;
  readonly action: string;
  readonly contexts?: readonly ApplicationFeedbackContext[];
  readonly lastValidResource?: PublicResourceReference | null;
}

export function applicationRecoveryPolicy(
  condition: string,
): ApplicationRecoveryPolicy {
  switch (condition) {
    case "retryable":
      return {
        intent: "retry",
        label: "Retry",
        summary:
          "The operation can be attempted again without discarding the last-valid state.",
      };
    case "degraded":
      return {
        intent: "continue_degraded",
        label: "Continue",
        summary:
          "Unrelated work can continue while the affected capability remains visible.",
      };
    case "user_correctable":
      return {
        intent: "correct",
        label: "Review",
        summary: "User input or project state must be corrected before retrying.",
      };
    case "terminal":
    default:
      return {
        intent: "restart",
        label: "Restart",
        summary: "Preserve available work and begin a fresh application lifetime.",
      };
  }
}

function normalizeFailureCondition(condition: string): ApplicationFailureCondition {
  switch (condition) {
    case "retryable":
      return "retryable";
    case "degraded":
      return "degraded";
    case "user_correctable":
      return "user_correctable";
    case "terminal":
    default:
      return "terminal";
  }
}

export function applicationFailureFromMessage(
  message: ApplicationFailureMessage,
): ApplicationFailurePresentation {
  const condition = normalizeFailureCondition(message.condition);
  return {
    id: message.id,
    source: message.source,
    revision: message.revision,
    condition,
    category: message.category ?? "application",
    code: message.code,
    title: message.title,
    action: message.action,
    contexts: message.contexts ?? [],
    lastValidResource: message.lastValidResource ?? null,
    primaryAction: applicationRecoveryPolicy(condition),
  };
}

export function applicationFailureFromTransport(
  id: string,
  source: string,
  revision: number,
  failure: DesktopTransportFailure,
): ApplicationFailurePresentation {
  const contexts: ApplicationFeedbackContext[] = [];
  for (const context of failure.contexts) {
    contexts.push({
      label: "Context",
      value: `${context.component}.${context.operation}`,
    });
    for (const [field, value] of Object.entries(context.fields).sort(
      ([left], [right]) => left.localeCompare(right),
    )) {
      contexts.push({
        label: field,
        value: formatTraceValue(value),
      });
    }
  }
  return applicationFailureFromMessage({
    id,
    source,
    revision,
    condition: failure.condition,
    category: failure.category,
    code: failure.code,
    title: failure.title,
    action: failure.action,
    contexts,
    lastValidResource: failure.lastValidResource,
  });
}

export function applicationFailureFromProject(
  id: string,
  source: string,
  revision: number,
  failure: DesktopProjectFailure,
): ApplicationFailurePresentation {
  return applicationFailureFromMessage({
    id,
    source,
    revision,
    condition: failure.class,
    category: "project",
    code: failure.code,
    title: failure.title,
    action: failure.action,
    contexts: Object.entries(failure.context)
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([label, value]) => ({ label, value })),
  });
}

export function applicationFailureFromCrashDiagnostic(
  diagnostic: DesktopCrashDiagnostic,
): ApplicationFailurePresentation {
  const contexts: ApplicationFeedbackContext[] = diagnostic.contexts.map(
    (context) => ({
      label: "Context",
      value: `${context.component}.${context.operation}`,
    }),
  );
  contexts.push({
    label: "Workspace",
    value: diagnostic.continuity.workspace.route_id,
  });
  contexts.push({
    label: "Recovery entry",
    value: diagnostic.recovery_entry_point,
  });
  if (diagnostic.continuity.workspace.focused_panel_id !== null) {
    contexts.push({
      label: "Focused panel",
      value: diagnostic.continuity.workspace.focused_panel_id,
    });
  }
  if (diagnostic.continuity.project !== null) {
    contexts.push({
      label: "Project",
      value: `${diagnostic.continuity.project.path} at revision ${diagnostic.continuity.project.project_revision}`,
    });
  }
  if (diagnostic.continuity.lifecycle !== null) {
    contexts.push({
      label: "Engine generation",
      value: String(diagnostic.continuity.lifecycle.engine_generation),
    });
  }
  const presentation = applicationFailureFromMessage({
    id: `crash:${diagnostic.diagnostic_id}`,
    source: "Retained crash recovery",
    revision: diagnostic.captured_unix_millis,
    condition: diagnostic.failure_class,
    category: "crash_recovery",
    code: diagnostic.code,
    title: diagnostic.title,
    action: diagnostic.action,
    contexts,
  });
  return {
    ...presentation,
    primaryAction: {
      ...presentation.primaryAction,
      label: "Review recovery",
    },
  };
}

export function applicationFailureFromLifecycle(
  snapshot: DesktopLifecycleSnapshot,
): ApplicationFailurePresentation | null {
  if (snapshot.failure === null) {
    return null;
  }
  return applicationFailureFromMessage({
    id: "engine-lifecycle",
    source: "Engine lifecycle",
    revision: snapshot.revision,
    condition: snapshot.failure.recoverability,
    category: snapshot.failure.category,
    code: `engine.lifecycle.${snapshot.failure.category}`,
    title: snapshot.failure.summary,
    action:
      normalizeFailureCondition(snapshot.failure.recoverability) === "terminal"
        ? "Restart Superi while preserving the latest workspace and project continuity."
        : "Use the available lifecycle recovery action, then verify the engine state.",
    contexts: [
      {
        label: "Engine generation",
        value: String(snapshot.engine_generation),
      },
      {
        label: "Application phase",
        value: snapshot.application_phase,
      },
      {
        label: "Engine phase",
        value: snapshot.engine_phase,
      },
      ...snapshot.failure.contexts.map((context) => ({
        label: "Context",
        value: `${context.component}.${context.operation}`,
      })),
    ],
  });
}

function formatTraceValue(value: TraceValue): string {
  switch (value.kind) {
    case "bool":
      return value.value ? "true" : "false";
    case "i64":
    case "u64":
    case "text":
      return value.value;
    case "f64":
      return String(value.value);
  }
}

export type ApplicationNotificationTone =
  | "information"
  | "success"
  | "warning"
  | "error";

export interface ApplicationNotificationInput {
  readonly id: string;
  readonly title: string;
  readonly message: string;
  readonly tone: ApplicationNotificationTone;
  readonly actionLabel?: string;
  readonly onAction?: () => void;
}

export interface ApplicationNotification extends ApplicationNotificationInput {
  readonly sequence: number;
}

export interface ApplicationNotificationState {
  readonly notifications: readonly ApplicationNotification[];
  readonly nextSequence: number;
}

export type ApplicationNotificationAction =
  | {
      readonly type: "publish";
      readonly notification: ApplicationNotificationInput;
    }
  | { readonly type: "dismiss"; readonly id: string }
  | { readonly type: "clear" };

export const MAX_APPLICATION_NOTIFICATIONS = 24;

function freezeNotifications(
  notifications: readonly ApplicationNotification[],
  nextSequence: number,
): ApplicationNotificationState {
  const frozenNotifications = Object.freeze(
    notifications.map((notification) => Object.freeze(notification)),
  );
  return Object.freeze({
    notifications: frozenNotifications,
    nextSequence,
  });
}

export function createApplicationNotificationState(): ApplicationNotificationState {
  return freezeNotifications([], 1);
}

export function reduceApplicationNotificationState(
  state: ApplicationNotificationState,
  action: ApplicationNotificationAction,
): ApplicationNotificationState {
  if (action.type === "clear") {
    return freezeNotifications([], state.nextSequence);
  }
  if (action.type === "dismiss") {
    return freezeNotifications(
      state.notifications.filter((notification) => notification.id !== action.id),
      state.nextSequence,
    );
  }
  const notification: ApplicationNotification = {
    ...action.notification,
    sequence: state.nextSequence,
  };
  const notifications = [
    ...state.notifications.filter(
      (existing) => existing.id !== action.notification.id,
    ),
    notification,
  ].slice(-MAX_APPLICATION_NOTIFICATIONS);
  return freezeNotifications(notifications, state.nextSequence + 1);
}

export interface ApplicationProgressPresentation {
  readonly id: string;
  readonly label: string;
  readonly status: string;
  readonly detail: string;
  readonly completed: number;
  readonly total: number | null;
  readonly percent: number | null;
  readonly active: boolean;
  readonly failureCondition: ApplicationFailureCondition | null;
}

export function applicationProgressFromEditorJob(
  job: EditorExportJobSnapshot,
): ApplicationProgressPresentation {
  const total = job.total_units;
  const completed =
    total === null
      ? Math.max(0, job.completed_units)
      : Math.min(Math.max(0, job.completed_units), Math.max(0, total));
  const percent =
    total === null || total <= 0
      ? null
      : (completed / total) * 100;
  return {
    id: `export:${job.job_id}`,
    label: `Export ${job.job_id}`,
    status: job.status,
    detail: `Attempt ${job.attempt}, progress revision ${job.progress_revision}`,
    completed,
    total,
    percent,
    active: !job.is_final,
    failureCondition:
      job.failure === null
        ? null
        : normalizeFailureCondition(job.failure.recoverability),
  };
}

export type ApplicationOperationalCondition =
  | "ready"
  | "working"
  | "degraded"
  | "attention";

export interface ApplicationOperationalStatus {
  readonly condition: ApplicationOperationalCondition;
  readonly label: string;
  readonly detail: string;
}

export function applicationOperationalStatus(input: {
  readonly failures: readonly ApplicationFailurePresentation[];
  readonly progress: readonly ApplicationProgressPresentation[];
  readonly workspaceContinuity: string;
  readonly engineLabel: string;
}): ApplicationOperationalStatus {
  if (input.failures.some((failure) => failure.condition === "terminal")) {
    return {
      condition: "attention",
      label: "Needs attention",
      detail: `${input.engineLabel}. Workspace continuity: ${input.workspaceContinuity}.`,
    };
  }
  if (input.failures.length > 0) {
    return {
      condition: "degraded",
      label: "Working with limitations",
      detail: `${input.failures.length} recoverable condition${input.failures.length === 1 ? "" : "s"} visible.`,
    };
  }
  const activeProgress = input.progress.filter((progress) => progress.active);
  if (activeProgress.length > 0) {
    return {
      condition: "working",
      label: "Work in progress",
      detail: `${activeProgress.length} operation${activeProgress.length === 1 ? "" : "s"} active.`,
    };
  }
  return {
    condition: "ready",
    label: "Ready",
    detail: `${input.engineLabel}. Workspace continuity: ${input.workspaceContinuity}.`,
  };
}

export function placeApplicationContextMenu(input: {
  readonly x: number;
  readonly y: number;
  readonly menuWidth: number;
  readonly menuHeight: number;
  readonly viewportWidth: number;
  readonly viewportHeight: number;
}): { readonly left: number; readonly top: number } {
  const margin = 8;
  const maximumLeft = Math.max(margin, input.viewportWidth - input.menuWidth - margin);
  const maximumTop = Math.max(margin, input.viewportHeight - input.menuHeight - margin);
  return {
    left: Math.min(maximumLeft, Math.max(margin, input.x)),
    top: Math.min(maximumTop, Math.max(margin, input.y)),
  };
}
