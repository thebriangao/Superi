import assert from "node:assert/strict";
import test from "node:test";

import type { ApplicationCommandDefinition } from "../src/application.ts";
import {
  commandForKeyboardShortcut,
  createKeyboardShortcutProfile,
  effectiveKeyboardShortcut,
  exportKeyboardShortcutProfile,
  formatKeyboardShortcut,
  importKeyboardShortcutProfile,
  resetKeyboardShortcut,
  resetKeyboardShortcuts,
  resolveKeyboardShortcutProfile,
  setKeyboardShortcut,
  shortcutFromKeyboardEvent,
} from "../src/keyboard-shortcuts.ts";

const commands: readonly ApplicationCommandDefinition[] = [
  {
    id: "application.route.editing",
    title: "Open editing workspace",
    shortcut: "Mod+1",
    execute() {},
  },
  {
    id: "application.route.color",
    title: "Open color workspace",
    shortcut: "Mod+3",
    execute() {},
  },
  {
    id: "application.selection.clear",
    title: "Clear shared selection",
    shortcut: "Mod+Shift+A",
    execute() {},
  },
  {
    id: "application.inspector.toggle",
    title: "Toggle inspector",
    execute() {},
  },
];

test("shortcut capture preserves portable modifiers, named keys, Unicode, and IME input", () => {
  assert.equal(
    shortcutFromKeyboardEvent(
      { key: "K", metaKey: true, ctrlKey: false, altKey: false, shiftKey: true },
      "apple",
    ),
    "mod+shift+k",
  );
  assert.equal(
    shortcutFromKeyboardEvent(
      { key: "k", metaKey: false, ctrlKey: true, altKey: false, shiftKey: false },
      "other",
    ),
    "mod+k",
  );
  assert.equal(
    shortcutFromKeyboardEvent(
      { key: " ", metaKey: true, ctrlKey: false, altKey: false, shiftKey: false },
      "apple",
    ),
    "mod+space",
  );
  assert.equal(
    shortcutFromKeyboardEvent(
      { key: "+", metaKey: true, ctrlKey: false, altKey: false, shiftKey: true },
      "apple",
    ),
    "mod+shift+plus",
  );
  assert.equal(
    shortcutFromKeyboardEvent(
      { key: "Å", metaKey: true, ctrlKey: false, altKey: false, shiftKey: false },
      "apple",
    ),
    "mod+å",
  );
  assert.equal(
    shortcutFromKeyboardEvent(
      {
        key: "Process",
        metaKey: false,
        ctrlKey: false,
        altKey: false,
        shiftKey: false,
        isComposing: true,
      },
      "other",
    ),
    null,
  );
  assert.equal(
    shortcutFromKeyboardEvent(
      { key: "Shift", metaKey: false, ctrlKey: false, altKey: false, shiftKey: true },
      "other",
    ),
    null,
  );
  assert.equal(formatKeyboardShortcut("mod+alt+arrowup", "apple"), "Command + Option + Arrow Up");
  assert.equal(formatKeyboardShortcut("mod+alt+arrowup", "other"), "Control + Alt + Arrow Up");
});

test("profile mutations reject application and native conflicts before replacing state", () => {
  const initial = createKeyboardShortcutProfile();
  const changed = setKeyboardShortcut(
    commands,
    initial,
    "application.inspector.toggle",
    "Mod+K",
  );
  assert.equal(
    effectiveKeyboardShortcut(commands, changed, "application.inspector.toggle"),
    "mod+k",
  );
  assert.equal(
    commandForKeyboardShortcut(commands, changed, " K + Mod ")?.id,
    "application.inspector.toggle",
  );
  assert.throws(
    () =>
      setKeyboardShortcut(
        commands,
        changed,
        "application.route.color",
        "Mod+K",
      ),
    /already assigned.*Toggle inspector/i,
  );
  assert.throws(
    () =>
      setKeyboardShortcut(
        commands,
        changed,
        "application.route.color",
        "Mod+S",
      ),
    /reserved.*Save/i,
  );
  assert.throws(
    () =>
      setKeyboardShortcut(
        commands,
        changed,
        "application.route.color",
        "K",
      ),
    /modifier/i,
  );
  assert.equal(
    effectiveKeyboardShortcut(commands, changed, "application.route.color"),
    "mod+3",
  );
  assert.equal(initial.overrides.length, 0);
  assert.ok(Object.isFrozen(changed));
  assert.ok(Object.isFrozen(changed.overrides));
  assert.ok(Object.isFrozen(changed.overrides[0]));
});

test("explicit unbinding, command reset, and reset all preserve default intent", () => {
  const unbound = setKeyboardShortcut(
    commands,
    createKeyboardShortcutProfile(),
    "application.route.editing",
    null,
  );
  assert.equal(
    effectiveKeyboardShortcut(commands, unbound, "application.route.editing"),
    null,
  );
  assert.equal(commandForKeyboardShortcut(commands, unbound, "Mod+1"), null);

  const restored = resetKeyboardShortcut(
    commands,
    unbound,
    "application.route.editing",
  );
  assert.equal(
    effectiveKeyboardShortcut(commands, restored, "application.route.editing"),
    "mod+1",
  );
  assert.deepEqual(resetKeyboardShortcuts().overrides, []);
});

test("import is transactional and deterministic while retaining unavailable commands", () => {
  const source = JSON.stringify({
    schema_version: 1,
    overrides: [
      { command_id: "future.command", shortcut: "Mod+Y" },
      { command_id: "application.route.editing", shortcut: "Mod+E" },
    ],
  });
  const resolved = importKeyboardShortcutProfile(commands, source);
  assert.deepEqual(resolved.inactive_command_ids, ["future.command"]);
  assert.equal(
    effectiveKeyboardShortcut(commands, resolved.profile, "application.route.editing"),
    "mod+e",
  );
  assert.equal(
    exportKeyboardShortcutProfile(resolved.profile),
    `${JSON.stringify(
      {
        schema_version: 1,
        overrides: [
          { command_id: "application.route.editing", shortcut: "mod+e" },
          { command_id: "future.command", shortcut: "mod+y" },
        ],
      },
      null,
      2,
    )}\n`,
  );

  assert.throws(
    () =>
      importKeyboardShortcutProfile(
        commands,
        JSON.stringify({
          schema_version: 1,
          overrides: [
            { command_id: "application.route.editing", shortcut: "Mod+K" },
            { command_id: "application.route.color", shortcut: "Mod+K" },
          ],
        }),
      ),
    /conflicts.*Open editing workspace/i,
  );
  assert.throws(
    () =>
      importKeyboardShortcutProfile(
        commands,
        JSON.stringify({ schema_version: 2, overrides: [] }),
      ),
    /schema version/i,
  );
  assert.throws(
    () =>
      resolveKeyboardShortcutProfile(commands, {
        schema_version: 1,
        overrides: [
          { command_id: "application.route.editing", shortcut: "Mod+E" },
          { command_id: "application.route.editing", shortcut: null },
        ],
      }),
    /duplicate command/i,
  );
  assert.throws(
    () =>
      resolveKeyboardShortcutProfile(commands, {
        schema_version: 1,
        overrides: [{ command_id: " future.command ", shortcut: "Mod+Y" }],
      }),
    /command identity is invalid/i,
  );
  assert.throws(
    () =>
      resolveKeyboardShortcutProfile(commands, {
        schema_version: 1,
        overrides: [
          { command_id: "é".repeat(300), shortcut: "Mod+Y" },
        ],
      }),
    /command identity is invalid/i,
  );
});
