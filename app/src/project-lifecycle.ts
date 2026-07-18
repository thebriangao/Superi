import { invoke } from "@tauri-apps/api/core";

export type DesktopProjectFailureClass =
  | "retryable"
  | "degraded"
  | "user_correctable"
  | "terminal";

export interface DesktopProjectFailure {
  readonly class: DesktopProjectFailureClass;
  readonly code: string;
  readonly title: string;
  readonly action: string;
  readonly context: Readonly<Record<string, string>>;
}

export interface DesktopProjectIdentity {
  readonly project_id: string;
  readonly project_revision: number;
  readonly root_timeline_id: string;
}

export interface DesktopProjectRecord {
  readonly path: string;
  readonly identity: DesktopProjectIdentity;
}

export interface DesktopRecoveryCandidate {
  readonly candidate_id: string;
  readonly project_revision: number;
  readonly action: string;
}

export interface DesktopRecoveryCatalog {
  readonly catalog_revision: number;
  readonly candidates: readonly DesktopRecoveryCandidate[];
}

export interface DesktopProjectSnapshot {
  readonly revision: number;
  readonly active: DesktopProjectRecord | null;
  readonly recent: readonly DesktopProjectRecord[];
  readonly recovery: DesktopRecoveryCatalog | null;
  readonly failure: DesktopProjectFailure | null;
}

export interface DesktopProjectCreateRequest {
  readonly project_id: string;
  readonly project_name: string;
  readonly root_timeline_id: string;
  readonly root_timeline_name: string;
  readonly edit_rate_numerator: number;
  readonly edit_rate_denominator: number;
}

export type DesktopProjectCommand =
  | {
      readonly kind: "create";
      readonly path: string;
      readonly project: DesktopProjectCreateRequest;
    }
  | { readonly kind: "open"; readonly path: string }
  | { readonly kind: "open_recent"; readonly path: string }
  | { readonly kind: "save" }
  | {
      readonly kind: "save_as";
      readonly destination: string;
      readonly replace_existing: boolean;
    }
  | { readonly kind: "close" }
  | { readonly kind: "discover_recovery" }
  | {
      readonly kind: "restore_recovery";
      readonly catalog_revision: number;
      readonly candidate_id: string;
    };

const SNAPSHOT_COMMAND = "desktop_project_snapshot";
const EXECUTE_COMMAND = "desktop_project_execute";

export async function getDesktopProjectSnapshot(): Promise<DesktopProjectSnapshot> {
  return invoke<DesktopProjectSnapshot>(SNAPSHOT_COMMAND);
}

export async function executeDesktopProject(
  command: DesktopProjectCommand,
): Promise<DesktopProjectSnapshot> {
  return invoke<DesktopProjectSnapshot>(EXECUTE_COMMAND, { command });
}
