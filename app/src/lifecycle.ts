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

const SNAPSHOT_COMMAND = "desktop_lifecycle_snapshot";
const REQUEST_COMMAND = "desktop_lifecycle_request";

export async function getDesktopLifecycle(): Promise<DesktopLifecycleSnapshot> {
  return invoke<DesktopLifecycleSnapshot>(SNAPSHOT_COMMAND);
}

export async function requestDesktopLifecycle(
  request: ApplicationLifecycleRequest,
): Promise<DesktopLifecycleSnapshot> {
  return invoke<DesktopLifecycleSnapshot>(REQUEST_COMMAND, { request });
}
