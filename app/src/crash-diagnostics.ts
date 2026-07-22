import { invoke } from "@tauri-apps/api/core";

import type { ApplicationRoutePanelLayoutPresentation } from "./application.ts";

export type DesktopCrashFailureClass =
  | "retryable"
  | "degraded"
  | "user_correctable"
  | "terminal";

export type DesktopCrashRecoveryEntryPoint =
  | "restore_workspace"
  | "retry_engine"
  | "continue_degraded"
  | "review_project_recovery"
  | "restart_engine";

export interface DesktopCrashContext {
  readonly component: string;
  readonly operation: string;
}

export interface DesktopWorkspaceContinuity {
  readonly route_id: string;
  readonly hidden_panel_ids: readonly string[];
  readonly focused_panel_id: string | null;
  readonly panel_layouts: readonly ApplicationRoutePanelLayoutPresentation[];
}

export interface DesktopProjectContinuity {
  readonly path: string;
  readonly project_revision: number;
}

export interface DesktopLifecycleContinuity {
  readonly revision: number;
  readonly engine_generation: number;
  readonly application_phase: string;
}

export interface DesktopSessionContinuity {
  readonly workspace: DesktopWorkspaceContinuity;
  readonly project: DesktopProjectContinuity | null;
  readonly lifecycle: DesktopLifecycleContinuity | null;
}

export interface DesktopCrashDiagnostic {
  readonly diagnostic_id: string;
  readonly captured_unix_millis: number;
  readonly failure_class: DesktopCrashFailureClass;
  readonly code: string;
  readonly title: string;
  readonly action: string;
  readonly contexts: readonly DesktopCrashContext[];
  readonly continuity: DesktopSessionContinuity;
  readonly recovery_entry_point: DesktopCrashRecoveryEntryPoint;
}

export interface DesktopCrashDiagnosticsSnapshot {
  readonly revision: number;
  readonly current_session_id: string;
  readonly persistence_available: boolean;
  readonly continuity: DesktopSessionContinuity;
  readonly diagnostics: readonly DesktopCrashDiagnostic[];
}

const SNAPSHOT_COMMAND = "desktop_crash_diagnostics_snapshot";
const WORKSPACE_UPDATE_COMMAND = "desktop_crash_workspace_update";
const PROJECT_UPDATE_COMMAND = "desktop_crash_project_update";
const DISMISS_COMMAND = "desktop_crash_diagnostic_dismiss";

let workspaceUpdateTail: Promise<void> = Promise.resolve();
let projectUpdateTail: Promise<void> = Promise.resolve();

export async function getDesktopCrashDiagnostics(): Promise<DesktopCrashDiagnosticsSnapshot> {
  return invoke<DesktopCrashDiagnosticsSnapshot>(SNAPSHOT_COMMAND);
}

export function updateDesktopCrashWorkspace(
  workspace: DesktopWorkspaceContinuity,
): Promise<DesktopCrashDiagnosticsSnapshot> {
  const update = workspaceUpdateTail.then(() =>
    invoke<DesktopCrashDiagnosticsSnapshot>(WORKSPACE_UPDATE_COMMAND, {
      workspace,
    }),
  );
  workspaceUpdateTail = update.then(
    () => undefined,
    () => undefined,
  );
  return update;
}

export function updateDesktopCrashProject(
  project: DesktopProjectContinuity | null,
): Promise<DesktopCrashDiagnosticsSnapshot> {
  const update = projectUpdateTail.then(() =>
    invoke<DesktopCrashDiagnosticsSnapshot>(PROJECT_UPDATE_COMMAND, {
      project,
    }),
  );
  projectUpdateTail = update.then(
    () => undefined,
    () => undefined,
  );
  return update;
}

export async function dismissDesktopCrashDiagnostic(
  diagnosticId: string,
): Promise<DesktopCrashDiagnosticsSnapshot> {
  return invoke<DesktopCrashDiagnosticsSnapshot>(DISMISS_COMMAND, {
    diagnosticId,
  });
}
