import {
  normalizeShortcut,
  type ApplicationCommandDefinition,
} from "./application.ts";

export const KEYBOARD_SHORTCUT_SCHEMA_VERSION = 1 as const;
const MAX_SHORTCUT_OVERRIDES = 512;
const MAX_COMMAND_IDENTITY_BYTES = 512;
const MAX_SHORTCUT_BYTES = 256;
const UTF8_ENCODER = new TextEncoder();

export type KeyboardShortcutPlatform = "apple" | "other";

export interface KeyboardShortcutOverride {
  readonly command_id: string;
  readonly shortcut: string | null;
}

export interface KeyboardShortcutProfile {
  readonly schema_version: typeof KEYBOARD_SHORTCUT_SCHEMA_VERSION;
  readonly overrides: readonly KeyboardShortcutOverride[];
}

export interface ResolvedKeyboardShortcutProfile {
  readonly profile: KeyboardShortcutProfile;
  readonly inactive_command_ids: readonly string[];
}

export interface KeyboardShortcutEvent {
  readonly key: string;
  readonly metaKey: boolean;
  readonly ctrlKey: boolean;
  readonly altKey: boolean;
  readonly shiftKey: boolean;
  readonly isComposing?: boolean;
}

export interface ReservedKeyboardShortcut {
  readonly shortcut: string;
  readonly title: string;
}

export const KEYBOARD_SHORTCUT_RESERVED_BINDINGS: readonly ReservedKeyboardShortcut[] =
  Object.freeze(
    [
      ["mod+n", "New project"],
      ["mod+o", "Open project"],
      ["mod+s", "Save"],
      ["mod+shift+s", "Save as"],
      ["mod+w", "Close project"],
      ["mod+i", "Import media"],
      ["mod+q", "Quit Superi"],
      ["mod+z", "Undo project change"],
      ["mod+shift+z", "Redo project change"],
      ["mod+x", "Cut"],
      ["mod+c", "Copy"],
      ["mod+v", "Paste"],
      ["mod+a", "Select all"],
    ].map(([shortcut, title]) => Object.freeze({ shortcut, title })),
  );

const RESERVED_BY_SHORTCUT = new Map(
  KEYBOARD_SHORTCUT_RESERVED_BINDINGS.map((binding) => [
    binding.shortcut,
    binding,
  ]),
);

export function createKeyboardShortcutProfile(): KeyboardShortcutProfile {
  return freezeProfile([]);
}

export function resolveKeyboardShortcutProfile(
  commands: readonly ApplicationCommandDefinition[],
  candidate: unknown,
): ResolvedKeyboardShortcutProfile {
  const record = requireRecord(candidate, "keyboard shortcut profile");
  requireExactKeys(record, ["schema_version", "overrides"], "keyboard shortcut profile");
  if (record.schema_version !== KEYBOARD_SHORTCUT_SCHEMA_VERSION) {
    throw new Error(
      `Keyboard shortcut schema version must be ${KEYBOARD_SHORTCUT_SCHEMA_VERSION}.`,
    );
  }
  if (!Array.isArray(record.overrides)) {
    throw new Error("Keyboard shortcut overrides must be an array.");
  }
  if (record.overrides.length > MAX_SHORTCUT_OVERRIDES) {
    throw new Error(
      `Keyboard shortcut overrides exceed the ${MAX_SHORTCUT_OVERRIDES} entry limit.`,
    );
  }

  const overrides: KeyboardShortcutOverride[] = [];
  const commandIds = new Set<string>();
  for (const value of record.overrides) {
    const override = requireRecord(value, "keyboard shortcut override");
    requireExactKeys(
      override,
      ["command_id", "shortcut"],
      "keyboard shortcut override",
    );
    const commandId = requireCommandIdentity(override.command_id);
    if (commandIds.has(commandId)) {
      throw new Error(`Keyboard shortcut profile contains duplicate command ${commandId}.`);
    }
    commandIds.add(commandId);
    const shortcut = requireOptionalShortcut(override.shortcut);
    if (shortcut !== null) {
      requireConfigurableShortcut(shortcut);
      requireUnreservedShortcut(shortcut);
    }
    overrides.push({ command_id: commandId, shortcut });
  }

  const profile = freezeProfile(overrides);
  const commandById = commandMap(commands);
  validateEffectiveShortcuts(commands, profile);
  const inactiveCommandIds = profile.overrides
    .map((override) => override.command_id)
    .filter((commandId) => !commandById.has(commandId));
  return Object.freeze({
    profile,
    inactive_command_ids: Object.freeze(inactiveCommandIds),
  });
}

export function importKeyboardShortcutProfile(
  commands: readonly ApplicationCommandDefinition[],
  source: string,
): ResolvedKeyboardShortcutProfile {
  let candidate: unknown;
  try {
    candidate = JSON.parse(source);
  } catch {
    throw new Error("Keyboard shortcut import must be valid JSON.");
  }
  return resolveKeyboardShortcutProfile(commands, candidate);
}

export function exportKeyboardShortcutProfile(
  profile: KeyboardShortcutProfile,
): string {
  return `${JSON.stringify(profile, null, 2)}\n`;
}

export function setKeyboardShortcut(
  commands: readonly ApplicationCommandDefinition[],
  profile: KeyboardShortcutProfile,
  commandId: string,
  shortcut: string | null,
): KeyboardShortcutProfile {
  const command = commandMap(commands).get(commandId);
  if (command === undefined) {
    throw new Error(`Unknown application command: ${commandId}.`);
  }
  const normalized = shortcut === null ? null : normalizeShortcut(shortcut);
  if (normalized !== null) {
    requireConfigurableShortcut(normalized);
    requireUnreservedShortcut(normalized);
    const assigned = commandForKeyboardShortcut(commands, profile, normalized);
    if (assigned !== null && assigned.id !== commandId) {
      throw new Error(
        `Shortcut ${normalized} is already assigned to ${assigned.title}.`,
      );
    }
  }

  const defaultsTo = command.shortcut === undefined
    ? null
    : normalizeShortcut(command.shortcut);
  const byCommand = new Map(
    profile.overrides.map((override) => [override.command_id, override] as const),
  );
  if (normalized === defaultsTo) {
    byCommand.delete(commandId);
  } else {
    byCommand.set(commandId, { command_id: commandId, shortcut: normalized });
  }
  return resolveKeyboardShortcutProfile(commands, {
    schema_version: KEYBOARD_SHORTCUT_SCHEMA_VERSION,
    overrides: [...byCommand.values()],
  }).profile;
}

export function resetKeyboardShortcut(
  commands: readonly ApplicationCommandDefinition[],
  profile: KeyboardShortcutProfile,
  commandId: string,
): KeyboardShortcutProfile {
  if (!commandMap(commands).has(commandId)) {
    throw new Error(`Unknown application command: ${commandId}.`);
  }
  return resolveKeyboardShortcutProfile(commands, {
    schema_version: KEYBOARD_SHORTCUT_SCHEMA_VERSION,
    overrides: profile.overrides.filter(
      (override) => override.command_id !== commandId,
    ),
  }).profile;
}

export function resetKeyboardShortcuts(): KeyboardShortcutProfile {
  return createKeyboardShortcutProfile();
}

export function effectiveKeyboardShortcut(
  commands: readonly ApplicationCommandDefinition[],
  profile: KeyboardShortcutProfile,
  commandId: string,
): string | null {
  const command = commandMap(commands).get(commandId);
  if (command === undefined) {
    throw new Error(`Unknown application command: ${commandId}.`);
  }
  const override = profile.overrides.find(
    (candidate) => candidate.command_id === commandId,
  );
  if (override !== undefined) {
    return override.shortcut;
  }
  return command.shortcut === undefined ? null : normalizeShortcut(command.shortcut);
}

export function commandForKeyboardShortcut(
  commands: readonly ApplicationCommandDefinition[],
  profile: KeyboardShortcutProfile,
  shortcut: string,
): ApplicationCommandDefinition | null {
  let normalized: string;
  try {
    normalized = normalizeShortcut(shortcut);
  } catch {
    return null;
  }
  return (
    commands.find(
      (command) =>
        effectiveKeyboardShortcut(commands, profile, command.id) === normalized,
    ) ?? null
  );
}

export function shortcutFromKeyboardEvent(
  event: KeyboardShortcutEvent,
  platform: KeyboardShortcutPlatform,
): string | null {
  if (event.isComposing) return null;
  const loweredKey = event.key.normalize("NFC").toLowerCase();
  if (
    loweredKey.length === 0 ||
    ["dead", "process", "unidentified", "meta", "control", "alt", "shift"].includes(
      loweredKey,
    )
  ) {
    return null;
  }
  if (platform === "other" && event.metaKey && !event.ctrlKey) {
    return null;
  }
  const modifiers: string[] = [];
  if (
    (platform === "apple" && event.metaKey) ||
    (platform === "other" && event.ctrlKey)
  ) {
    modifiers.push("mod");
  }
  if (platform === "apple" && event.ctrlKey) modifiers.push("ctrl");
  if (event.altKey) modifiers.push("alt");
  if (event.shiftKey) modifiers.push("shift");
  const key = event.key === " " ? "space" : event.key === "+" ? "plus" : event.key;
  try {
    return normalizeShortcut([...modifiers, key].join("+"));
  } catch {
    return null;
  }
}

export function detectKeyboardShortcutPlatform(): KeyboardShortcutPlatform {
  const platform = globalThis.navigator?.platform ?? "";
  return /mac|iphone|ipad|ipod/iu.test(platform) ? "apple" : "other";
}

export function formatKeyboardShortcut(
  shortcut: string,
  platform: KeyboardShortcutPlatform,
): string {
  const labels: Readonly<Record<string, string>> = {
    mod: platform === "apple" ? "Command" : "Control",
    ctrl: "Control",
    alt: platform === "apple" ? "Option" : "Alt",
    shift: "Shift",
    arrowdown: "Arrow Down",
    arrowleft: "Arrow Left",
    arrowright: "Arrow Right",
    arrowup: "Arrow Up",
    backspace: "Backspace",
    delete: "Delete",
    end: "End",
    enter: "Enter",
    equal: "Equal",
    escape: "Escape",
    home: "Home",
    insert: "Insert",
    minus: "Minus",
    pagedown: "Page Down",
    pageup: "Page Up",
    plus: "Plus",
    space: "Space",
    tab: "Tab",
  };
  return normalizeShortcut(shortcut)
    .split("+")
    .map((part) => labels[part] ?? part.toUpperCase())
    .join(" + ");
}

function validateEffectiveShortcuts(
  commands: readonly ApplicationCommandDefinition[],
  profile: KeyboardShortcutProfile,
): void {
  const commandByShortcut = new Map<string, ApplicationCommandDefinition>();
  for (const command of commands) {
    const shortcut = effectiveKeyboardShortcut(commands, profile, command.id);
    if (shortcut === null) continue;
    requireUnreservedShortcut(shortcut);
    const existing = commandByShortcut.get(shortcut);
    if (existing !== undefined) {
      throw new Error(
        `Shortcut ${shortcut} conflicts with ${existing.title}.`,
      );
    }
    commandByShortcut.set(shortcut, command);
  }
}

function requireConfigurableShortcut(shortcut: string): void {
  const parts = shortcut.split("+");
  if (!parts.includes("mod") && !parts.includes("alt")) {
    throw new Error(
      "A configurable application shortcut must include the portable primary modifier or Alt.",
    );
  }
  if (parts.includes("ctrl")) {
    throw new Error(
      "Use the portable primary modifier instead of a platform-specific Control binding.",
    );
  }
}

function requireUnreservedShortcut(shortcut: string): void {
  const reserved = RESERVED_BY_SHORTCUT.get(shortcut);
  if (reserved !== undefined) {
    throw new Error(
      `Shortcut ${shortcut} is reserved by the native ${reserved.title} action.`,
    );
  }
}

function requireOptionalShortcut(value: unknown): string | null {
  if (value === null) return null;
  if (typeof value !== "string" || utf8ByteLength(value) > MAX_SHORTCUT_BYTES) {
    throw new Error("Keyboard shortcut values must be bounded text or null.");
  }
  const shortcut = normalizeShortcut(value);
  if (utf8ByteLength(shortcut) > MAX_SHORTCUT_BYTES) {
    throw new Error("Keyboard shortcut values must be bounded text or null.");
  }
  return shortcut;
}

function requireCommandIdentity(value: unknown): string {
  if (typeof value !== "string") {
    throw new Error("Keyboard shortcut command identity must be text.");
  }
  const commandId = value;
  if (
    commandId.length === 0 ||
    commandId !== commandId.trim() ||
    utf8ByteLength(commandId) > MAX_COMMAND_IDENTITY_BYTES ||
    Array.from(commandId).some((character) => /\p{Cc}/u.test(character))
  ) {
    throw new Error("Keyboard shortcut command identity is invalid.");
  }
  return commandId;
}

function utf8ByteLength(value: string): number {
  return UTF8_ENCODER.encode(value).byteLength;
}

function commandMap(
  commands: readonly ApplicationCommandDefinition[],
): ReadonlyMap<string, ApplicationCommandDefinition> {
  return new Map(commands.map((command) => [command.id, command]));
}

function freezeProfile(
  overrides: readonly KeyboardShortcutOverride[],
): KeyboardShortcutProfile {
  return Object.freeze({
    schema_version: KEYBOARD_SHORTCUT_SCHEMA_VERSION,
    overrides: Object.freeze(
      [...overrides]
        .sort((left, right) =>
          left.command_id < right.command_id
            ? -1
            : left.command_id > right.command_id
              ? 1
              : 0,
        )
        .map((override) => Object.freeze({ ...override })),
    ),
  });
}

function requireRecord(value: unknown, kind: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`${kind} must be an object.`);
  }
  return value as Record<string, unknown>;
}

function requireExactKeys(
  value: Record<string, unknown>,
  keys: readonly string[],
  kind: string,
): void {
  const actual = Object.keys(value).sort();
  const expected = [...keys].sort();
  if (
    actual.length !== expected.length ||
    actual.some((key, index) => key !== expected[index])
  ) {
    throw new Error(`${kind} contains unsupported or missing fields.`);
  }
}
