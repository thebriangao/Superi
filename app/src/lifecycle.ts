import { invoke } from "@tauri-apps/api/core";

export type ApplicationLifecyclePhase =
  | "starting"
  | "running"
  | "suspending"
  | "suspended"
  | "resuming"
  | "stopping"
  | "restarting"
  | "recovering"
  | "failed"
  | "stopped";

export type EngineLifecyclePhase =
  | "starting"
  | "ready"
  | "suspending"
  | "suspended"
  | "resuming"
  | "stopping"
  | "failed"
  | "stopped";

export type LifecycleIntent =
  | "start"
  | "run"
  | "shutdown"
  | "restart"
  | "recover";

export type ApplicationLifecycleRequest = "recover" | "restart" | "shutdown";

export interface DesktopLifecycleFailureContext {
  readonly component: string;
  readonly operation: string;
}

export interface DesktopLifecycleFailure {
  readonly category: string;
  readonly recoverability: string;
  readonly summary: string;
  readonly contexts: readonly DesktopLifecycleFailureContext[];
}

export interface DesktopLifecycleSnapshot {
  readonly revision: number;
  readonly engine_generation: number;
  readonly application_phase: ApplicationLifecyclePhase;
  readonly engine_phase: EngineLifecyclePhase;
  readonly intent: LifecycleIntent;
  readonly engine_acknowledgement_pending: boolean;
  readonly failure: DesktopLifecycleFailure | null;
  readonly can_retry: boolean;
  readonly can_restart: boolean;
  readonly can_shutdown: boolean;
}

export type DesktopProcessPhase =
  | "starting"
  | "running"
  | "stopping"
  | "stopped"
  | "failed";

export type DesktopProcessServiceId =
  | "application_exit"
  | "file_association_tasks"
  | "engine_control"
  | "playback"
  | "background_workers"
  | "gpu_submission"
  | "window_persistence";

export type DesktopProcessServiceKind =
  | "monitor"
  | "task_group"
  | "execution_domain"
  | "worker_pool"
  | "persistence";

export type DesktopProcessServicePhase =
  | "pending"
  | "starting"
  | "running"
  | "stopping"
  | "stopped"
  | "failed";

export interface DesktopProcessServiceSnapshot {
  readonly id: DesktopProcessServiceId;
  readonly label: string;
  readonly kind: DesktopProcessServiceKind;
  readonly phase: DesktopProcessServicePhase;
  readonly owned_units: number;
  readonly active_units: number;
  readonly join_pending: boolean;
  readonly thread_names: readonly string[];
  readonly summary: string;
}

export interface DesktopProcessSnapshot {
  readonly revision: number;
  readonly phase: DesktopProcessPhase;
  readonly accepting_background_tasks: boolean;
  readonly services: readonly DesktopProcessServiceSnapshot[];
}

const SNAPSHOT_COMMAND = "desktop_lifecycle_snapshot";
const REQUEST_COMMAND = "desktop_lifecycle_request";
const PROCESS_SNAPSHOT_COMMAND = "desktop_process_snapshot";

export async function getDesktopLifecycle(): Promise<DesktopLifecycleSnapshot> {
  return invoke<DesktopLifecycleSnapshot>(SNAPSHOT_COMMAND);
}

export async function requestDesktopLifecycle(
  request: ApplicationLifecycleRequest,
): Promise<DesktopLifecycleSnapshot> {
  return invoke<DesktopLifecycleSnapshot>(REQUEST_COMMAND, { request });
}

export async function getDesktopProcess(): Promise<DesktopProcessSnapshot> {
  return invoke<DesktopProcessSnapshot>(PROCESS_SNAPSHOT_COMMAND);
}
