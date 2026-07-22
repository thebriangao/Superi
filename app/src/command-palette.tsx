import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
} from "react";

import {
  type CommandPaletteAction,
  type CommandPaletteCatalog,
  type CommandPaletteExecutionResult,
} from "./command-palette.ts";
import {
  containTabFocus,
  focusFirstInScope,
} from "./focus-management.ts";
import { restoreShellFocus } from "./shell-input.ts";
import "./command-palette.css";

export interface CommandPaletteProps {
  readonly catalog: CommandPaletteCatalog;
  readonly onDismiss: () => void;
  readonly onExecute: (
    action: CommandPaletteAction,
  ) => Promise<CommandPaletteExecutionResult>;
}

export function CommandPalette({
  catalog,
  onDismiss,
  onExecute,
}: CommandPaletteProps) {
  const dialogRef = useRef<HTMLDialogElement | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  const priorFocus = useRef<HTMLElement | null>(null);
  const runningActionRef = useRef<string | null>(null);
  const [query, setQuery] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const [runningActionId, setRunningActionId] = useState<string | null>(null);
  const [feedback, setFeedback] = useState<string | null>(null);
  const results = useMemo(() => catalog.search(query), [catalog, query]);
  const boundedSelectedIndex = Math.min(
    selectedIndex,
    Math.max(results.length - 1, 0),
  );
  const selectedAction = results[boundedSelectedIndex] ?? null;

  useEffect(() => {
    const dialog = dialogRef.current;
    priorFocus.current =
      document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
    if (dialog !== null && !dialog.open) {
      dialog.showModal();
    }
    if (dialog !== null) focusFirstInScope(dialog, inputRef.current);
    return () => {
      if (dialog?.open) dialog.close();
      restoreShellFocus(priorFocus.current);
    };
  }, []);

  useEffect(() => {
    setSelectedIndex(0);
    setFeedback(null);
  }, [query]);

  const runAction = async (action: CommandPaletteAction) => {
    if (runningActionRef.current !== null) return;
    if (!action.availability.enabled) {
      setFeedback(
        action.availability.reason ?? "This action is unavailable.",
      );
      return;
    }
    runningActionRef.current = action.id;
    setRunningActionId(action.id);
    setFeedback(null);
    const result = await onExecute(action).catch((error: unknown) => ({
      status: "failed" as const,
      message:
        error instanceof Error
          ? error.message
          : "The selected action could not be completed.",
    }));
    if (result.status === "completed") {
      onDismiss();
      return;
    }
    runningActionRef.current = null;
    setRunningActionId(null);
    setFeedback(result.message);
  };

  const handleKeyDown = (event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (results.length === 0) return;
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setSelectedIndex((current) => (current + 1) % results.length);
      return;
    }
    if (event.key === "ArrowUp") {
      event.preventDefault();
      setSelectedIndex(
        (current) => (current - 1 + results.length) % results.length,
      );
      return;
    }
    if (event.key === "Home") {
      event.preventDefault();
      setSelectedIndex(0);
      return;
    }
    if (event.key === "End") {
      event.preventDefault();
      setSelectedIndex(results.length - 1);
      return;
    }
    if (event.key === "Enter" && selectedAction !== null) {
      event.preventDefault();
      void runAction(selectedAction);
    }
  };

  const handleBackdrop = (event: ReactMouseEvent<HTMLDialogElement>) => {
    if (event.target === dialogRef.current) onDismiss();
  };

  const handleDialogKeyDown = (
    event: ReactKeyboardEvent<HTMLDialogElement>,
  ) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      onDismiss();
      return;
    }
    if (event.key === "Tab") {
      event.preventDefault();
      containTabFocus(
        event.currentTarget,
        document.activeElement,
        event.shiftKey,
      );
    }
  };

  return (
    <dialog
      className="command-palette-dialog"
      ref={dialogRef}
      aria-modal="true"
      aria-labelledby="command-palette-title"
      aria-describedby="command-palette-description"
      onKeyDown={handleDialogKeyDown}
      onCancel={(event) => {
        event.preventDefault();
        onDismiss();
      }}
      onClick={handleBackdrop}
      tabIndex={-1}
    >
      <section className="command-palette-surface">
        <header className="command-palette-header">
          <div>
            <p className="command-palette-eyebrow">Application commands</p>
            <h2 id="command-palette-title">Find Command</h2>
          </div>
          <button
            className="command-palette-close"
            type="button"
            aria-label="Close command palette"
            onClick={onDismiss}
          >
            Esc
          </button>
        </header>
        <p id="command-palette-description" className="command-palette-help">
          Search every registered workspace and desktop action by name,
          category, keyword, or stable identity.
        </p>
        <input
          className="command-palette-search"
          ref={inputRef}
          type="search"
          value={query}
          placeholder="Search commands"
          aria-label="Search commands"
          aria-controls="command-palette-results"
          aria-activedescendant={
            selectedAction === null
              ? undefined
              : commandPaletteOptionId(selectedAction.id)
          }
          autoComplete="off"
          spellCheck={false}
          onChange={(event) => setQuery(event.currentTarget.value)}
          onKeyDown={handleKeyDown}
        />
        <div className="command-palette-status" aria-live="polite">
          <span>
            {results.length} {results.length === 1 ? "action" : "actions"}
          </span>
          {feedback === null ? null : <strong>{feedback}</strong>}
        </div>
        <div
          id="command-palette-results"
          className="command-palette-results"
          role="listbox"
          aria-label="Matching commands"
        >
          {results.length === 0 ? (
            <p className="command-palette-empty">No matching command.</p>
          ) : (
            results.map((action, index) => {
              const selected = index === boundedSelectedIndex;
              const running = runningActionId === action.id;
              return (
                <button
                  id={commandPaletteOptionId(action.id)}
                  className="command-palette-option"
                  type="button"
                  role="option"
                  key={action.id}
                  aria-selected={selected}
                  aria-disabled={!action.availability.enabled || running}
                  data-selected={selected || undefined}
                  data-disabled={!action.availability.enabled || undefined}
                  onMouseMove={() => setSelectedIndex(index)}
                  onClick={() => void runAction(action)}
                >
                  <span className="command-palette-option-copy">
                    <span className="command-palette-option-heading">
                      <strong>{action.title}</strong>
                      <span>{action.category}</span>
                    </span>
                    <span className="command-palette-option-detail">
                      {action.availability.enabled
                        ? action.detail
                        : action.availability.reason}
                    </span>
                    <code>{action.id}</code>
                  </span>
                  {action.shortcut === null ? null : (
                    <kbd>{formatShortcut(action.shortcut)}</kbd>
                  )}
                </button>
              );
            })
          )}
        </div>
      </section>
    </dialog>
  );
}

function commandPaletteOptionId(actionId: string): string {
  return `command-palette-option-${actionId.replace(/[^a-z0-9]+/giu, "-")}`;
}

function formatShortcut(shortcut: string): string {
  return shortcut
    .split("+")
    .map((part) => {
      switch (part.toLowerCase()) {
        case "mod":
          return navigator.platform.includes("Mac") ? "Command" : "Ctrl";
        case "shift":
          return "Shift";
        case "alt":
          return "Alt";
        default:
          return part.toUpperCase();
      }
    })
    .join(" + ");
}
