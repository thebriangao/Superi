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
  stateRef.current = state;

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

  return (
    <ApplicationContext.Provider
      value={{
        registry,
        state,
        dispatch,
        executeCommand,
        commandFailure,
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
