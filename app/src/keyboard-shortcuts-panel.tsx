import { useMemo, useRef, useState, type KeyboardEvent } from "react";

import { useApplication } from "./application-context.tsx";
import {
  KEYBOARD_SHORTCUT_RESERVED_BINDINGS,
  detectKeyboardShortcutPlatform,
  formatKeyboardShortcut,
  shortcutFromKeyboardEvent,
} from "./keyboard-shortcuts.ts";
import "./keyboard-shortcuts.css";

const MAX_IMPORT_BYTES = 256 * 1024;

export function KeyboardShortcutsPanel() {
  const {
    registry,
    keyboardShortcutProfile,
    keyboardShortcutsHydrated,
    inactiveKeyboardShortcutCommandIds,
    keyboardShortcutNotice,
    keyboardShortcutFailure,
    keyboardShortcutForCommand,
    setKeyboardShortcut,
    resetKeyboardShortcut,
    resetKeyboardShortcuts,
    importKeyboardShortcuts,
    exportKeyboardShortcuts,
  } = useApplication();
  const platform = useMemo(detectKeyboardShortcutPlatform, []);
  const [localStatus, setLocalStatus] = useState<string | null>(null);
  const [resetArmed, setResetArmed] = useState(false);
  const [transferFailure, setTransferFailure] = useState<string | null>(null);
  const importInput = useRef<HTMLInputElement>(null);
  const overriddenCommandIds = new Set(
    keyboardShortcutProfile.overrides.map((override) => override.command_id),
  );

  const captureShortcut = (
    event: KeyboardEvent<HTMLInputElement>,
    commandId: string,
  ) => {
    if (event.nativeEvent.isComposing) return;
    if (event.key === "Escape") {
      event.preventDefault();
      event.currentTarget.blur();
      setLocalStatus("Shortcut capture was cancelled without changes.");
      return;
    }
    if (
      ["Backspace", "Delete"].includes(event.key) &&
      !event.metaKey &&
      !event.ctrlKey &&
      !event.altKey &&
      !event.shiftKey
    ) {
      event.preventDefault();
      const result = setKeyboardShortcut(commandId, null);
      setLocalStatus(result.status === "completed" ? result.message : null);
      setTransferFailure(null);
      return;
    }
    if (event.key === "Tab") return;
    const shortcut = shortcutFromKeyboardEvent(event.nativeEvent, platform);
    if (shortcut === null) return;
    event.preventDefault();
    const result = setKeyboardShortcut(commandId, shortcut);
    setLocalStatus(result.status === "completed" ? result.message : null);
    setTransferFailure(null);
  };

  const importProfile = async (file: File | undefined) => {
    if (file === undefined) return;
    if (file.size > MAX_IMPORT_BYTES) {
      setTransferFailure("Keyboard shortcut imports must be 256 KB or smaller.");
      if (importInput.current !== null) importInput.current.value = "";
      return;
    }
    try {
      const result = importKeyboardShortcuts(await file.text());
      setLocalStatus(result.status === "completed" ? result.message : null);
      setTransferFailure(
        result.status === "failed" ? result.message : null,
      );
    } catch {
      setTransferFailure(
        "The selected shortcut profile could not be read. Choose a local JSON file and try again.",
      );
    } finally {
      if (importInput.current !== null) importInput.current.value = "";
    }
  };

  const exportProfile = () => {
    try {
      const blob = new Blob([exportKeyboardShortcuts()], {
        type: "application/json;charset=utf-8",
      });
      const url = URL.createObjectURL(blob);
      const link = document.createElement("a");
      link.href = url;
      link.download = "superi-keyboard-shortcuts.json";
      link.style.display = "none";
      document.body.append(link);
      link.click();
      link.remove();
      URL.revokeObjectURL(url);
      setTransferFailure(null);
      setLocalStatus("The active keyboard shortcut profile was exported.");
    } catch {
      setTransferFailure(
        "The browser could not export the shortcut profile. Keep this panel open and try again.",
      );
    }
  };

  const resetAll = () => {
    if (!resetArmed) {
      setResetArmed(true);
      setLocalStatus(
        "Reset is ready. Activate Confirm reset all to remove every override.",
      );
      return;
    }
    const result = resetKeyboardShortcuts();
    setResetArmed(false);
    setLocalStatus(result.message);
    setTransferFailure(null);
  };

  return (
    <div className="panel-content keyboard-shortcuts-panel">
      <header className="keyboard-shortcuts-heading">
        <div>
          <p className="eyebrow">Application preferences</p>
          <h3>Keyboard shortcuts</h3>
        </div>
        <span data-ready={keyboardShortcutsHydrated}>
          {keyboardShortcutsHydrated ? "Session restored" : "Restoring session"}
        </span>
      </header>

      <p className="keyboard-shortcuts-introduction">
        Focus a capture field, then press the complete shortcut. Backspace or
        Delete clears a binding, Escape cancels capture, and Tab continues
        navigation. Command on Apple platforms and Control elsewhere are stored
        as the portable primary modifier.
      </p>

      {keyboardShortcutFailure || transferFailure ? (
        <p className="keyboard-shortcuts-message keyboard-shortcuts-error" role="alert">
          {transferFailure ?? keyboardShortcutFailure}
        </p>
      ) : null}
      {keyboardShortcutNotice || localStatus ? (
        <p className="keyboard-shortcuts-message" role="status">
          {localStatus ?? keyboardShortcutNotice}
        </p>
      ) : null}
      {inactiveKeyboardShortcutCommandIds.length > 0 ? (
        <details className="keyboard-shortcuts-inactive">
          <summary>
            {inactiveKeyboardShortcutCommandIds.length} unavailable command
            binding{inactiveKeyboardShortcutCommandIds.length === 1 ? "" : "s"}
            {" "}retained
          </summary>
          <ul>
            {inactiveKeyboardShortcutCommandIds.map((commandId) => (
              <li key={commandId}>{commandId}</li>
            ))}
          </ul>
        </details>
      ) : null}

      <div className="keyboard-shortcuts-list" role="list">
        {registry.commandDefinitions.map((command) => {
          const shortcut = keyboardShortcutForCommand(command.id);
          const defaultShortcut = command.shortcut ?? null;
          const overridden = overriddenCommandIds.has(command.id);
          const fieldDescriptionId = `${command.id.replace(/[^a-zA-Z0-9_-]/gu, "-")}-shortcut-help`;
          return (
            <section
              className="keyboard-shortcut-row"
              data-overridden={overridden}
              key={command.id}
              role="listitem"
            >
              <div className="keyboard-shortcut-command">
                <strong>{command.title}</strong>
                <code>{command.id}</code>
                <small id={fieldDescriptionId}>
                  Default: {defaultShortcut === null
                    ? "Unassigned"
                    : formatKeyboardShortcut(defaultShortcut, platform)}
                </small>
              </div>
              <label className="keyboard-shortcut-capture">
                <span>Current shortcut</span>
                <input
                  aria-describedby={fieldDescriptionId}
                  aria-label={`Capture shortcut for ${command.title}`}
                  onKeyDown={(event) => captureShortcut(event, command.id)}
                  placeholder="Press a shortcut"
                  readOnly
                  value={
                    shortcut === null
                      ? "Unassigned"
                      : formatKeyboardShortcut(shortcut, platform)
                  }
                />
              </label>
              <div className="keyboard-shortcut-row-actions">
                <button
                  className="secondary"
                  type="button"
                  disabled={shortcut === null}
                  onClick={() => {
                    const result = setKeyboardShortcut(command.id, null);
                    setLocalStatus(
                      result.status === "completed" ? result.message : null,
                    );
                    setTransferFailure(null);
                  }}
                >
                  Clear
                </button>
                <button
                  className="secondary"
                  type="button"
                  disabled={!overridden}
                  onClick={() => {
                    const result = resetKeyboardShortcut(command.id);
                    setLocalStatus(
                      result.status === "completed" ? result.message : null,
                    );
                    setTransferFailure(null);
                  }}
                >
                  Reset
                </button>
              </div>
            </section>
          );
        })}
      </div>

      <section className="keyboard-shortcuts-transfer" aria-labelledby="shortcut-transfer-title">
        <div>
          <h4 id="shortcut-transfer-title">Import and export</h4>
          <p>
            JSON import replaces the active profile only after the complete file
            passes schema, command, conflict, and native reservation checks.
          </p>
        </div>
        <label>
          <span>Import shortcut profile</span>
          <input
            accept="application/json,.json"
            ref={importInput}
            type="file"
            onChange={(event) => void importProfile(event.currentTarget.files?.[0])}
          />
        </label>
        <div className="keyboard-shortcuts-transfer-actions">
          <button type="button" onClick={exportProfile}>
            Export JSON
          </button>
          <button
            className={resetArmed ? "" : "secondary"}
            type="button"
            onClick={resetAll}
            onBlur={() => setResetArmed(false)}
          >
            {resetArmed ? "Confirm reset all" : "Reset all"}
          </button>
        </div>
      </section>

      <details className="keyboard-shortcuts-reserved">
        <summary>Reserved native shortcuts</summary>
        <p>
          File, Edit, and application accelerators remain owned by the native
          menu and cannot be assigned to application commands.
        </p>
        <ul>
          {KEYBOARD_SHORTCUT_RESERVED_BINDINGS.map((binding) => (
            <li key={binding.shortcut}>
              <kbd>{formatKeyboardShortcut(binding.shortcut, platform)}</kbd>
              <span>{binding.title}</span>
            </li>
          ))}
        </ul>
      </details>
    </div>
  );
}
