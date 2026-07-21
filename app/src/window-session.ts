const DESKTOP_WINDOW_EVENT = "superi://window-session-changed";

const WINDOW_PHASES = new Set([
  "loading",
  "ready",
  "recovered",
  "failed",
  "shutting_down",
]);
const PERSISTENCE_PHASES = new Set([
  "loading",
  "ready",
  "degraded",
  "stopped",
]);
const WORKSPACES = new Set([
  "editing",
  "compositing",
  "color",
  "audio",
  "delivery",
  "system",
]);

export interface DesktopWindowFailure {
  readonly category: string;
  readonly recoverability: string;
  readonly summary: string;
}

export interface DesktopMonitorTarget {
  readonly id: string;
  readonly name: string;
  readonly positionX: number;
  readonly positionY: number;
  readonly physicalWidth: number;
  readonly physicalHeight: number;
  readonly scaleFactor: number;
  readonly primary: boolean;
}

export interface DesktopWindowRecord {
  readonly label: string;
  readonly title: string;
  readonly workspace: string;
  readonly primary: boolean;
  readonly focused: boolean;
  readonly fullscreen: boolean;
  readonly monitorId: string | null;
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
  readonly canUndoPlacement: boolean;
  readonly canClose: boolean;
}

export interface DesktopWindowSnapshot {
  readonly schemaVersion: 1;
  readonly revision: number;
  readonly phase: string;
  readonly persistencePhase: string;
  readonly nativeViewportOwner: "main";
  readonly lastFocusedLabel: string | null;
  readonly recoveryNote: string | null;
  readonly failure: DesktopWindowFailure | null;
  readonly recentlyClosedCount: number;
  readonly monitors: readonly DesktopMonitorTarget[];
  readonly windows: readonly DesktopWindowRecord[];
}

export interface DesktopWindowHost {
  invoke<T>(command: string, args: Record<string, unknown>): Promise<T>;
  listen<T>(
    event: string,
    listener: (event: { readonly payload: T }) => void,
  ): Promise<() => void>;
}

const TAURI_HOST: DesktopWindowHost = {
  async invoke<T>(command: string, args: Record<string, unknown>): Promise<T> {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(command, args);
  },
  async listen<T>(
    event: string,
    listener: (event: { readonly payload: T }) => void,
  ): Promise<() => void> {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<T>(event, listener);
  },
};

export async function getDesktopWindowSession(
  host: DesktopWindowHost = TAURI_HOST,
): Promise<DesktopWindowSnapshot> {
  return invokeSnapshot(host, "desktop_window_session_snapshot", {});
}

export async function createDesktopWindow(
  workspace: string,
  host: DesktopWindowHost = TAURI_HOST,
): Promise<DesktopWindowSnapshot> {
  requireWorkspace(workspace);
  return invokeSnapshot(host, "desktop_window_create", {
    request: { workspace },
  });
}

export async function focusDesktopWindow(
  label: string,
  host: DesktopWindowHost = TAURI_HOST,
): Promise<DesktopWindowSnapshot> {
  return labelAction(host, "desktop_window_focus", label);
}

export async function setDesktopWindowFullscreen(
  label: string,
  fullscreen: boolean,
  host: DesktopWindowHost = TAURI_HOST,
): Promise<DesktopWindowSnapshot> {
  requireLabel(label);
  return invokeSnapshot(host, "desktop_window_fullscreen", {
    request: { label, fullscreen },
  });
}

export async function moveDesktopWindowToMonitor(
  label: string,
  monitorId: string,
  host: DesktopWindowHost = TAURI_HOST,
): Promise<DesktopWindowSnapshot> {
  requireLabel(label);
  requireIdentity(monitorId, "monitor");
  return invokeSnapshot(host, "desktop_window_move_to_monitor", {
    request: { label, monitorId },
  });
}

export async function undoDesktopWindowPlacement(
  label: string,
  host: DesktopWindowHost = TAURI_HOST,
): Promise<DesktopWindowSnapshot> {
  return labelAction(host, "desktop_window_undo_placement", label);
}

export async function updateDesktopWindowWorkspace(
  label: string,
  workspace: string,
  host: DesktopWindowHost = TAURI_HOST,
): Promise<DesktopWindowSnapshot> {
  requireLabel(label);
  requireWorkspace(workspace);
  return invokeSnapshot(host, "desktop_window_workspace_update", {
    request: { label, workspace },
  });
}

export async function closeDesktopWindow(
  label: string,
  host: DesktopWindowHost = TAURI_HOST,
): Promise<DesktopWindowSnapshot> {
  return labelAction(host, "desktop_window_close", label);
}

export async function reopenDesktopWindow(
  host: DesktopWindowHost = TAURI_HOST,
): Promise<DesktopWindowSnapshot> {
  return invokeSnapshot(host, "desktop_window_reopen", {});
}

export async function listenDesktopWindowSession(
  listener: (snapshot: DesktopWindowSnapshot) => void,
  host: DesktopWindowHost = TAURI_HOST,
): Promise<() => void> {
  return host.listen<unknown>(DESKTOP_WINDOW_EVENT, ({ payload }) => {
    listener(parseDesktopWindowSnapshot(payload));
  });
}

export function parseDesktopWindowSnapshot(
  value: unknown,
): DesktopWindowSnapshot {
  const snapshot = exactObject(value, [
    "schemaVersion",
    "revision",
    "phase",
    "persistencePhase",
    "nativeViewportOwner",
    "lastFocusedLabel",
    "recoveryNote",
    "failure",
    "recentlyClosedCount",
    "monitors",
    "windows",
  ]);
  if (
    snapshot.schemaVersion !== 1 ||
    !safeRevision(snapshot.revision) ||
    typeof snapshot.phase !== "string" ||
    !WINDOW_PHASES.has(snapshot.phase) ||
    typeof snapshot.persistencePhase !== "string" ||
    !PERSISTENCE_PHASES.has(snapshot.persistencePhase) ||
    snapshot.nativeViewportOwner !== "main" ||
    !nullableString(snapshot.lastFocusedLabel) ||
    !nullableString(snapshot.recoveryNote) ||
    !safeRevision(snapshot.recentlyClosedCount) ||
    snapshot.recentlyClosedCount > 8 ||
    !Array.isArray(snapshot.monitors) ||
    !Array.isArray(snapshot.windows) ||
    snapshot.windows.length === 0 ||
    snapshot.windows.length > 8
  ) {
    throw invalidSnapshot();
  }
  if (snapshot.failure !== null) {
    const failure = exactObject(snapshot.failure, [
      "category",
      "recoverability",
      "summary",
    ]);
    if (
      typeof failure.category !== "string" ||
      failure.category.length === 0 ||
      typeof failure.recoverability !== "string" ||
      failure.recoverability.length === 0 ||
      typeof failure.summary !== "string" ||
      failure.summary.length === 0
    ) {
      throw invalidSnapshot();
    }
  }
  const monitorIds = new Set<string>();
  for (const raw of snapshot.monitors) {
    const monitor = exactObject(raw, [
      "id",
      "name",
      "positionX",
      "positionY",
      "physicalWidth",
      "physicalHeight",
      "scaleFactor",
      "primary",
    ]);
    if (
      typeof monitor.id !== "string" ||
      monitor.id.length === 0 ||
      monitorIds.has(monitor.id) ||
      typeof monitor.name !== "string" ||
      !safeInteger(monitor.positionX) ||
      !safeInteger(monitor.positionY) ||
      !positiveInteger(monitor.physicalWidth) ||
      !positiveInteger(monitor.physicalHeight) ||
      typeof monitor.scaleFactor !== "number" ||
      !Number.isFinite(monitor.scaleFactor) ||
      monitor.scaleFactor <= 0 ||
      typeof monitor.primary !== "boolean"
    ) {
      throw invalidSnapshot();
    }
    monitorIds.add(monitor.id);
  }
  const windowLabels = new Set<string>();
  let primaryCount = 0;
  let focusedLabel: string | null = null;
  for (const raw of snapshot.windows) {
    const windowRecord = exactObject(raw, [
      "label",
      "title",
      "workspace",
      "primary",
      "focused",
      "fullscreen",
      "monitorId",
      "x",
      "y",
      "width",
      "height",
      "canUndoPlacement",
      "canClose",
    ]);
    if (
      typeof windowRecord.label !== "string" ||
      !validLabel(windowRecord.label) ||
      windowLabels.has(windowRecord.label) ||
      typeof windowRecord.title !== "string" ||
      windowRecord.title.trim().length === 0 ||
      windowRecord.title.length > 256 ||
      typeof windowRecord.workspace !== "string" ||
      !WORKSPACES.has(windowRecord.workspace) ||
      typeof windowRecord.primary !== "boolean" ||
      windowRecord.primary !== (windowRecord.label === "main") ||
      typeof windowRecord.focused !== "boolean" ||
      typeof windowRecord.fullscreen !== "boolean" ||
      !nullableString(windowRecord.monitorId) ||
      (windowRecord.monitorId !== null &&
        !monitorIds.has(windowRecord.monitorId)) ||
      !safeInteger(windowRecord.x) ||
      !safeInteger(windowRecord.y) ||
      !positiveInteger(windowRecord.width) ||
      !positiveInteger(windowRecord.height) ||
      typeof windowRecord.canUndoPlacement !== "boolean" ||
      typeof windowRecord.canClose !== "boolean" ||
      windowRecord.canClose === windowRecord.primary
    ) {
      throw invalidSnapshot();
    }
    if (windowRecord.primary) primaryCount += 1;
    if (windowRecord.focused) {
      if (focusedLabel !== null) throw invalidSnapshot();
      focusedLabel = windowRecord.label;
    }
    windowLabels.add(windowRecord.label);
  }
  if (
    primaryCount !== 1 ||
    (snapshot.lastFocusedLabel !== null &&
      !windowLabels.has(snapshot.lastFocusedLabel)) ||
    (focusedLabel !== null && focusedLabel !== snapshot.lastFocusedLabel)
  ) {
    throw invalidSnapshot();
  }
  return value as DesktopWindowSnapshot;
}

export function desktopWindowFailure(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error !== null && "summary" in error) {
    return String(error.summary);
  }
  return "The native window session is unavailable.";
}

async function labelAction(
  host: DesktopWindowHost,
  command: string,
  label: string,
): Promise<DesktopWindowSnapshot> {
  requireLabel(label);
  return invokeSnapshot(host, command, { request: { label } });
}

async function invokeSnapshot(
  host: DesktopWindowHost,
  command: string,
  args: Record<string, unknown>,
): Promise<DesktopWindowSnapshot> {
  return parseDesktopWindowSnapshot(await host.invoke<unknown>(command, args));
}

function requireLabel(label: string): void {
  if (!validLabel(label)) {
    throw new Error("The editor window label is invalid.");
  }
}

function validLabel(label: string): boolean {
  return label === "main" || /^workspace-[1-9]\d*$/u.test(label);
}

function requireWorkspace(workspace: string): void {
  if (!WORKSPACES.has(workspace)) {
    throw new Error("The application workspace is not registered.");
  }
}

function requireIdentity(value: string, kind: string): void {
  if (value.trim().length === 0) {
    throw new Error(`The ${kind} identity is required.`);
  }
}

function exactObject(
  value: unknown,
  keys: readonly string[],
): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw invalidSnapshot();
  }
  const record = value as Record<string, unknown>;
  const present = Object.keys(record);
  if (
    present.length !== keys.length ||
    present.some((key) => !keys.includes(key))
  ) {
    throw invalidSnapshot();
  }
  return record;
}

function safeRevision(value: unknown): value is number {
  return safeInteger(value) && value >= 0;
}

function safeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value);
}

function positiveInteger(value: unknown): value is number {
  return safeInteger(value) && value > 0;
}

function nullableString(value: unknown): value is string | null {
  return value === null || typeof value === "string";
}

function invalidSnapshot(): Error {
  return new Error("The native window session snapshot is invalid.");
}
