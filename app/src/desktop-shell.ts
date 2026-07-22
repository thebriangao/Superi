import { invoke } from "@tauri-apps/api/core";

import type { ApplicationWorkspacePresentation } from "./application.ts";

export type DesktopCloseReason = "window" | "quit";

export interface DesktopShellDocument {
  readonly path: string;
  readonly project_id: string;
  readonly project_revision: number;
}

export type DesktopWorkspacePresentation = ApplicationWorkspacePresentation;

export interface DesktopShellFailure {
  readonly code: string;
  readonly title: string;
  readonly action: string;
}

export interface DesktopShellSnapshot {
  readonly revision: number;
  readonly client_sequence: number;
  readonly active: DesktopShellDocument | null;
  readonly recent_paths: readonly string[];
  readonly undo_depth: number;
  readonly redo_depth: number;
  readonly busy: boolean;
  readonly workspace: DesktopWorkspacePresentation;
  readonly failure: DesktopShellFailure | null;
}

export interface DesktopShellPresentation {
  readonly active: DesktopShellDocument | null;
  readonly recent_paths: readonly string[];
  readonly undo_depth: number;
  readonly redo_depth: number;
  readonly busy: boolean;
  readonly workspace: DesktopWorkspacePresentation;
}

export type DesktopShellIntent =
  | { readonly kind: "new_project" }
  | { readonly kind: "open_project" }
  | { readonly kind: "open_recent"; readonly path: string }
  | { readonly kind: "save_project" }
  | { readonly kind: "save_project_as" }
  | { readonly kind: "close_project" }
  | { readonly kind: "import_media" }
  | { readonly kind: "scan_folder" }
  | { readonly kind: "undo" }
  | { readonly kind: "redo" }
  | { readonly kind: "open_workspace"; readonly route_id: string }
  | { readonly kind: "request_close"; readonly reason: DesktopCloseReason };

export type DesktopDrop =
  | { readonly kind: "project"; readonly path: string }
  | { readonly kind: "media"; readonly paths: readonly string[] }
  | { readonly kind: "ambiguous" };

export type DesktopCloseDecision =
  | "block_busy"
  | "confirm_history"
  | "save_and_close"
  | "close";

const DESKTOP_SHELL_EVENT = "superi://desktop-shell-intent";
const SNAPSHOT_COMMAND = "desktop_shell_snapshot";
const SYNC_COMMAND = "desktop_shell_sync";
const RESOLVE_CLOSE_COMMAND = "desktop_shell_resolve_close";
const REQUEST_CLOSE_COMMAND = "desktop_shell_request_close";

let clientSequence = 0;
let synchronization = Promise.resolve<DesktopShellSnapshot | null>(null);

export function partitionDesktopDrop(paths: readonly string[]): DesktopDrop {
  const unique = [...new Set(paths.filter((path) => path.trim().length > 0))];
  const projectPaths = unique.filter((path) => /\.superi$/iu.test(path));
  if (projectPaths.length === 1 && unique.length === 1) {
    return { kind: "project", path: projectPaths[0] };
  }
  if (projectPaths.length > 0) {
    return { kind: "ambiguous" };
  }
  return { kind: "media", paths: unique };
}

export function decideDesktopClose(input: {
  readonly busy: boolean;
  readonly active: boolean;
  readonly undoDepth: number;
  readonly redoDepth: number;
}): DesktopCloseDecision {
  if (input.busy) return "block_busy";
  if (input.active && (input.undoDepth > 0 || input.redoDepth > 0)) {
    return "confirm_history";
  }
  if (input.active) return "save_and_close";
  return "close";
}

export function desktopDocumentTitle(
  document: DesktopShellDocument | null,
): string {
  if (document === null) return "Superi";
  const name = document.path.split(/[\\/]/u).filter(Boolean).at(-1) ?? document.path;
  return `${name} [r${document.project_revision}] - Superi`;
}

export async function getDesktopShellSnapshot(): Promise<DesktopShellSnapshot> {
  const snapshot = await invoke<DesktopShellSnapshot>(SNAPSHOT_COMMAND);
  clientSequence = Math.max(clientSequence, snapshot.client_sequence);
  return snapshot;
}

export function syncDesktopShell(
  presentation: DesktopShellPresentation,
): Promise<DesktopShellSnapshot> {
  const pending = synchronization.then(async () => {
    clientSequence += 1;
    const snapshot = await invoke<DesktopShellSnapshot>(SYNC_COMMAND, {
      sync: {
        ...presentation,
        client_sequence: clientSequence,
      },
    });
    clientSequence = Math.max(clientSequence, snapshot.client_sequence);
    return snapshot;
  });
  synchronization = pending.catch(() => null);
  return pending;
}

export async function resolveDesktopClose(allow: boolean): Promise<boolean> {
  return invoke<boolean>(RESOLVE_CLOSE_COMMAND, { allow });
}

export async function requestDesktopClose(): Promise<boolean> {
  return invoke<boolean>(REQUEST_CLOSE_COMMAND);
}

export async function listenDesktopShellIntents(
  listener: (intent: DesktopShellIntent) => void,
): Promise<() => void> {
  const { listen } = await import("@tauri-apps/api/event");
  return listen<DesktopShellIntent>(DESKTOP_SHELL_EVENT, (event) => {
    listener(event.payload);
  });
}
