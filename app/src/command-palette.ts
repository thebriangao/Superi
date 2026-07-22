import type {
  ApplicationCommandAvailability,
  ApplicationCommandDefinition,
} from "./application.ts";
import {
  desktopShellIntentAutomationId,
  type DesktopShellIntent,
} from "./desktop-shell.ts";

const MAX_ACTIONS = 512;
const MAX_QUERY_LENGTH = 4_096;

export interface CommandPaletteAvailability {
  readonly enabled: boolean;
  readonly reason: string | null;
}

export type CommandPaletteInvocation =
  | {
      readonly kind: "application_command";
      readonly command_id: string;
    }
  | {
      readonly kind: "desktop_shell_intent";
      readonly intent: DesktopShellIntent;
    };

export interface CommandPaletteAction {
  readonly id: string;
  readonly title: string;
  readonly category: string;
  readonly keywords: readonly string[];
  readonly shortcut: string | null;
  readonly detail: string;
  readonly availability: CommandPaletteAvailability;
  readonly invocation: CommandPaletteInvocation;
}

export interface DesktopShellCommandPaletteInput {
  readonly active: boolean;
  readonly busy: boolean;
  readonly undoDepth: number;
  readonly redoDepth: number;
  readonly recentPaths: readonly string[];
}

export type CommandPaletteExecutionResult =
  | { readonly status: "completed" }
  | { readonly status: "disabled"; readonly message: string }
  | { readonly status: "failed"; readonly message: string };

export interface CommandPaletteExecutionHost {
  readonly executeApplicationCommand: (
    commandId: string,
  ) => Promise<
    | { readonly status: "completed" | "disabled" }
    | { readonly status: "failed"; readonly message: string }
  >;
  readonly executeDesktopShellIntent: (
    intent: DesktopShellIntent,
  ) => Promise<void>;
}

export class CommandPaletteCatalog {
  readonly actions: readonly CommandPaletteAction[];

  private readonly actionsById = new Map<string, CommandPaletteAction>();

  constructor(actions: readonly CommandPaletteAction[]) {
    if (actions.length > MAX_ACTIONS) {
      throw new Error(`command palette exceeds ${MAX_ACTIONS} actions`);
    }
    this.actions = Object.freeze(
      actions.map((action) => {
        const frozen = freezeAction(action);
        if (this.actionsById.has(frozen.id)) {
          throw new Error(`duplicate command palette action: ${frozen.id}`);
        }
        this.actionsById.set(frozen.id, frozen);
        return frozen;
      }),
    );
    Object.freeze(this);
  }

  public action(actionId: string): CommandPaletteAction {
    const action = this.actionsById.get(actionId);
    if (action === undefined) {
      throw new Error(`unknown command palette action: ${actionId}`);
    }
    return action;
  }

  public search(query: string): readonly CommandPaletteAction[] {
    const normalizedQuery = normalizeSearchText(
      query.slice(0, MAX_QUERY_LENGTH),
    );
    if (normalizedQuery.length === 0) {
      return this.actions;
    }
    const tokens = normalizedQuery.split(" ").filter(Boolean);
    return Object.freeze(
      this.actions
        .map((action) => ({ action, score: searchScore(action, normalizedQuery, tokens) }))
        .filter(
          (candidate): candidate is { action: CommandPaletteAction; score: number } =>
            candidate.score !== null,
        )
        .sort(
          (left, right) =>
            left.score - right.score ||
            Number(right.action.availability.enabled) -
              Number(left.action.availability.enabled) ||
            compareText(left.action.category, right.action.category) ||
            compareText(left.action.title, right.action.title) ||
            compareText(left.action.id, right.action.id),
        )
        .map(({ action }) => action),
    );
  }
}

export function applicationCommandPaletteActions(
  commands: readonly ApplicationCommandDefinition[],
  availability: (commandId: string) => ApplicationCommandAvailability,
): readonly CommandPaletteAction[] {
  return Object.freeze(
    commands.map((command) =>
      freezeAction({
        id: command.id,
        title: command.title,
        category: command.category,
        keywords: command.keywords,
        shortcut: command.shortcut ?? null,
        detail: command.id,
        availability: availability(command.id),
        invocation: {
          kind: "application_command",
          command_id: command.id,
        },
      }),
    ),
  );
}

export function desktopShellCommandPaletteActions(
  input: DesktopShellCommandPaletteInput,
): readonly CommandPaletteAction[] {
  const idle = input.busy
    ? disabled("Wait for the current operation to finish.")
    : enabled();
  const project = input.busy
    ? disabled("Wait for the current operation to finish.")
    : input.active
      ? enabled()
      : disabled("Open a project first.");
  const undo = input.busy
    ? disabled("Wait for the current operation to finish.")
    : input.undoDepth > 0
      ? enabled()
      : disabled("No project change is available to undo.");
  const redo = input.busy
    ? disabled("Wait for the current operation to finish.")
    : input.redoDepth > 0
      ? enabled()
      : disabled("No project change is available to redo.");

  const actions = [
    desktopAction(
      "New Project",
      "File",
      ["create", "document"],
      "mod+n",
      "Create a new durable Superi project.",
      idle,
      { kind: "new_project" },
    ),
    desktopAction(
      "Open Project...",
      "File",
      ["document", "browse"],
      "mod+o",
      "Open a durable Superi project from disk.",
      idle,
      { kind: "open_project" },
    ),
    ...input.recentPaths.map((path) =>
      desktopAction(
        `Open ${pathBasename(path)}`,
        "File",
        ["recent", "project", path],
        null,
        path,
        idle,
        { kind: "open_recent", path },
      ),
    ),
    desktopAction(
      "Save Project",
      "File",
      ["write", "document"],
      "mod+s",
      "Save the active project through its durable owner.",
      project,
      { kind: "save_project" },
    ),
    desktopAction(
      "Save Project As...",
      "File",
      ["copy", "destination", "document"],
      "mod+shift+s",
      "Save the active project to a selected destination.",
      project,
      { kind: "save_project_as" },
    ),
    desktopAction(
      "Close Project",
      "File",
      ["document", "safe close"],
      "mod+w",
      "Preserve and close the active project.",
      project,
      { kind: "close_project" },
    ),
    desktopAction(
      "Import Media...",
      "File",
      ["add", "files", "source"],
      "mod+i",
      "Import selected media into the active project.",
      project,
      { kind: "import_media" },
    ),
    desktopAction(
      "Scan Folder...",
      "File",
      ["import", "directory", "media"],
      null,
      "Scan one selected folder for project media.",
      project,
      { kind: "scan_folder" },
    ),
    desktopAction(
      "Undo Project Change",
      "Edit",
      ["history", "reverse"],
      "mod+z",
      `${input.undoDepth} project changes available to undo.`,
      undo,
      { kind: "undo" },
    ),
    desktopAction(
      "Redo Project Change",
      "Edit",
      ["history", "restore"],
      "mod+shift+z",
      `${input.redoDepth} project changes available to redo.`,
      redo,
      { kind: "redo" },
    ),
    desktopAction(
      "Quit Superi",
      "Application",
      ["exit", "safe close", "shutdown"],
      "mod+q",
      "Enter the existing safe application close path.",
      enabled(),
      { kind: "request_close", reason: "quit" },
    ),
  ];
  return Object.freeze(actions);
}

export async function executeCommandPaletteAction(
  action: CommandPaletteAction,
  host: CommandPaletteExecutionHost,
): Promise<CommandPaletteExecutionResult> {
  if (!action.availability.enabled) {
    return Object.freeze({
      status: "disabled",
      message: action.availability.reason ?? "This action is unavailable.",
    });
  }
  try {
    if (action.invocation.kind === "application_command") {
      const result = await host.executeApplicationCommand(
        action.invocation.command_id,
      );
      if (result.status === "failed") {
        return Object.freeze(result);
      }
      if (result.status === "disabled") {
        return Object.freeze({
          status: "disabled",
          message:
            action.availability.reason ??
            "The application state changed before this action could run.",
        });
      }
      return Object.freeze({ status: "completed" });
    }
    await host.executeDesktopShellIntent(action.invocation.intent);
    return Object.freeze({ status: "completed" });
  } catch (error: unknown) {
    return Object.freeze({
      status: "failed",
      message:
        error instanceof Error
          ? error.message
          : "The selected action could not be completed.",
    });
  }
}

function desktopAction(
  title: string,
  category: string,
  keywords: readonly string[],
  shortcut: string | null,
  detail: string,
  availability: CommandPaletteAvailability,
  intent: DesktopShellIntent,
): CommandPaletteAction {
  return freezeAction({
    id: desktopShellIntentAutomationId(intent),
    title,
    category,
    keywords,
    shortcut,
    detail,
    availability,
    invocation: { kind: "desktop_shell_intent", intent },
  });
}

function freezeAction(action: CommandPaletteAction): CommandPaletteAction {
  const invocation =
    action.invocation.kind === "application_command"
      ? Object.freeze({
          kind: "application_command" as const,
          command_id: requireText(
            action.invocation.command_id,
            "application command identity",
          ),
        })
      : Object.freeze({
          kind: "desktop_shell_intent" as const,
          intent: Object.freeze({ ...action.invocation.intent }),
        });
  return Object.freeze({
    id: requireText(action.id, "command palette action identity"),
    title: requireText(action.title, "command palette action title"),
    category: requireText(action.category, "command palette action category"),
    keywords: Object.freeze(
      action.keywords.map((keyword) =>
        requireText(keyword, "command palette search keyword"),
      ),
    ),
    shortcut:
      action.shortcut === null
        ? null
        : requireText(action.shortcut, "command palette shortcut"),
    detail: requireText(action.detail, "command palette action detail"),
    availability: Object.freeze({
      enabled: action.availability.enabled,
      reason:
        action.availability.reason === null
          ? null
          : requireText(
              action.availability.reason,
              "command palette availability reason",
            ),
    }),
    invocation,
  });
}

function searchScore(
  action: CommandPaletteAction,
  query: string,
  tokens: readonly string[],
): number | null {
  const title = normalizeSearchText(action.title);
  const id = normalizeSearchText(action.id);
  const category = normalizeSearchText(action.category);
  const keywords = action.keywords.map(normalizeSearchText);
  const detail = normalizeSearchText(action.detail);
  const shortcut = normalizeSearchText(action.shortcut ?? "");
  const searchable = [title, id, category, ...keywords, detail, shortcut].join(
    " ",
  );
  if (!tokens.every((token) => searchable.includes(token))) {
    return null;
  }
  if (title === query) return 0;
  if (id === query) return 1;
  if (title.startsWith(query)) return 5;
  if (title.includes(query)) return 10;
  let score = action.availability.enabled ? 20 : 40;
  for (const token of tokens) {
    if (title.startsWith(token)) score += 1;
    else if (title.includes(token)) score += 3;
    else if (keywords.some((keyword) => keyword.includes(token))) score += 5;
    else if (category.includes(token)) score += 7;
    else if (detail.includes(token)) score += 9;
    else score += 11;
  }
  return score;
}

function normalizeSearchText(value: string): string {
  return value
    .normalize("NFKD")
    .replace(/\p{Mark}+/gu, "")
    .toLowerCase()
    .replace(/[^\p{Letter}\p{Number}]+/gu, " ")
    .trim()
    .replace(/\s+/gu, " ");
}

function compareText(left: string, right: string): number {
  return left < right ? -1 : left > right ? 1 : 0;
}

function enabled(): CommandPaletteAvailability {
  return Object.freeze({ enabled: true, reason: null });
}

function disabled(reason: string): CommandPaletteAvailability {
  return Object.freeze({ enabled: false, reason });
}

function pathBasename(path: string): string {
  return path.split(/[\\/]/u).filter(Boolean).at(-1) ?? path;
}

function requireText(value: string, kind: string): string {
  const text = value.trim();
  if (text.length === 0) {
    throw new Error(`${kind} must not be empty`);
  }
  return text;
}
