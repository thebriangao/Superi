import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import { shortcutFromKeyboardEvent } from "../src/keyboard-shortcuts.ts";
import {
  isInputMethodKeyboardEvent,
  keyboardInputDisposition,
} from "../src/shell-input.ts";

test("IME, dead-key, and legacy composition events fail closed", () => {
  for (const event of [
    { key: "a", isComposing: true },
    { key: "Dead" },
    { key: "Process" },
    { key: "Unidentified" },
    { key: "Enter", keyCode: 229 },
  ]) {
    assert.equal(isInputMethodKeyboardEvent(event), true);
    assert.equal(keyboardInputDisposition(event, false, false), "composing");
  }
  assert.equal(isInputMethodKeyboardEvent({ key: "Enter", keyCode: 13 }), false);
  assert.equal(
    shortcutFromKeyboardEvent(
      { key: "K", keyCode: 229, metaKey: false, ctrlKey: true, altKey: false, shiftKey: false },
      "other",
    ),
    null,
  );
});

test("settled non-Latin shortcuts retain NFC identity", () => {
  const event = { metaKey: true, ctrlKey: false, altKey: false, shiftKey: false };
  assert.equal(shortcutFromKeyboardEvent({ ...event, key: "A\u030A" }, "apple"), "mod+å");
  assert.equal(shortcutFromKeyboardEvent({ ...event, key: "Ж" }, "apple"), "mod+ж");
  assert.equal(shortcutFromKeyboardEvent({ ...event, key: "界" }, "apple"), "mod+界");
});

test("editable command palette and track rename consumers guard composition", () => {
  const palette = readFileSync(new URL("../src/command-palette.tsx", import.meta.url), "utf8");
  const timeline = readFileSync(new URL("../src/timeline-workspace.tsx", import.meta.url), "utf8");
  assert.ok((palette.match(/isInputMethodKeyboardEvent\(event\)/g) ?? []).length >= 2);
  assert.match(timeline, /onKeyDown=\{\(event\) => \{\s*if \(isInputMethodKeyboardEvent\(event\)\) return;/);
});
