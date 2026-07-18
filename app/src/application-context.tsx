import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useReducer,
  useRef,
  useState,
  type ComponentType,
  type ReactNode,
} from "react";

import {
  ApplicationRegistry,
  createApplicationState,
  executeApplicationCommand,
  isEditableCommandTarget,
  normalizeShortcut,
  reduceApplicationState,
  type ApplicationAction,
  type ApplicationState,
} from "./application.ts";
import { useSuperiApi } from "./api-context";
import {
  INITIAL_EDITOR_PROJECT,
  createEditorStateRequest,
  type EditorProjectPresentation,
} from "./editor-project.ts";
import { classifyDesktopTransportError } from "./transport.ts";

export type ApplicationPanelRenderer = ComponentType;

export type ApplicationCommandResult =
  | { readonly status: "completed" | "disabled" }
  | { readonly status: "failed"; readonly message: string };

export interface ApplicationContextValue {
  readonly registry: ApplicationRegistry<ApplicationPanelRenderer>;
  readonly state: ApplicationState;
  readonly dispatch: (action: ApplicationAction) => void;
  readonly executeCommand: (
    commandId: string,
  ) => Promise<ApplicationCommandResult>;
  readonly commandFailure: string | null;
  readonly editorProject: EditorProjectPresentation;
  readonly refreshEditorProject: () => Promise<void>;
}

const ApplicationContext = createContext<ApplicationContextValue | null>(null);

export interface ApplicationProviderProps {
  readonly registry: ApplicationRegistry<ApplicationPanelRenderer>;
  readonly children: ReactNode;
}

export function ApplicationProvider({
  registry,
  children,
}: ApplicationProviderProps) {
  const api = useSuperiApi();
  const [state, dispatch] = useReducer(
    (current: ApplicationState, action: ApplicationAction) =>
      reduceApplicationState(registry, current, action),
    registry,
    createApplicationState,
  );
  const stateRef = useRef(state);
  const [commandFailure, setCommandFailure] = useState<string | null>(null);
  const [editorProject, setEditorProject] = useState<EditorProjectPresentation>(
    INITIAL_EDITOR_PROJECT,
  );
  const editorRequestRevision = useRef(0);
  const editorTransactionRevision = useRef(0);
  stateRef.current = state;

  const refreshEditorProject = useCallback(async (): Promise<void> => {
    const requestRevision = editorRequestRevision.current + 1;
    editorRequestRevision.current = requestRevision;
    if (api === null) {
      setEditorProject((current) => ({
        ...current,
        status: "unavailable",
        failure: null,
      }));
      return;
    }

    const transactionRevision = editorTransactionRevision.current + 1;
    editorTransactionRevision.current = transactionRevision;
    const transactionId = `superi.desktop.project-state.${transactionRevision}`;
    setEditorProject((current) => ({
      ...current,
      status: current.snapshot === null ? "loading" : "refreshing",
      transactionId,
      failure: null,
    }));

    try {
      const result = await api.request(
        "superi.editor.state.get",
        createEditorStateRequest(transactionId),
      );
      if (editorRequestRevision.current !== requestRevision) {
        return;
      }
      if (result.transaction_id !== transactionId) {
        throw new Error("editor state response transaction identity changed");
      }
      setEditorProject({
        status: "ready",
        transactionId,
        commandSequence: result.command_sequence,
        snapshot: result.snapshot,
        failure: null,
      });
    } catch (error: unknown) {
      if (editorRequestRevision.current !== requestRevision) {
        return;
      }
      const failure = classifyDesktopTransportError(error);
      setEditorProject((current) => ({
        ...current,
        status: failure.condition === "terminal" ? "failed" : "degraded",
        transactionId,
        failure,
      }));
    }
  }, [api]);

  const executeCommand = useCallback(
    async (commandId: string): Promise<ApplicationCommandResult> => {
      try {
        const result = await executeApplicationCommand({
          registry,
          state: () => stateRef.current,
          api,
          dispatch,
          commandId,
        });
        setCommandFailure(null);
        return result;
      } catch (error: unknown) {
        const message =
          error instanceof Error
            ? error.message
            : "The application command could not be completed.";
        setCommandFailure(message);
        return { status: "failed", message };
      }
    },
    [api, registry],
  );

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || isEditableCommandTarget(event.target)) {
        return;
      }
      const shortcut = shortcutFromKeyboardEvent(event);
      if (shortcut === null) {
        return;
      }
      const command = registry.commandForShortcut(shortcut);
      if (command === null) {
        return;
      }
      event.preventDefault();
      void executeCommand(command.id);
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [executeCommand, registry]);

  useEffect(() => {
    void refreshEditorProject();
    if (api === null) {
      return;
    }
    const refresh = () => void refreshEditorProject();
    const unsubscribers = [
      api.subscribe("superi.project.state.changed", refresh),
      api.subscribe("superi.audio.automation.changed", refresh),
      api.subscribe("superi.jobs.changed", refresh),
    ];
    return () => {
      editorRequestRevision.current += 1;
      for (const unsubscribe of unsubscribers) {
        unsubscribe();
      }
    };
  }, [api, refreshEditorProject]);

  return (
    <ApplicationContext.Provider
      value={{
        registry,
        state,
        dispatch,
        executeCommand,
        commandFailure,
        editorProject,
        refreshEditorProject,
      }}
    >
      {children}
    </ApplicationContext.Provider>
  );
}

export function useApplication(): ApplicationContextValue {
  const application = useContext(ApplicationContext);
  if (application === null) {
    throw new Error("ApplicationProvider is missing from the React tree");
  }
  return application;
}

function shortcutFromKeyboardEvent(event: KeyboardEvent): string | null {
  const key = event.key.trim().toLowerCase();
  if (
    key.length === 0 ||
    ["meta", "control", "alt", "shift"].includes(key)
  ) {
    return null;
  }
  const parts: string[] = [];
  if (event.metaKey || event.ctrlKey) {
    parts.push("mod");
  }
  if (event.altKey) {
    parts.push("alt");
  }
  if (event.shiftKey) {
    parts.push("shift");
  }
  parts.push(key === " " ? "space" : key);
  return normalizeShortcut(parts.join("+"));
}
