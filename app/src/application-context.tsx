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
import type {
  ExecutePlaybackTransportResult,
  ExecuteProjectCommand,
  ExecuteProjectCommandResult,
  PlaybackTransportAction,
  ProjectAction,
} from "./api.ts";
import type { SourceMonitorSnapshot } from "./project-lifecycle.ts";
import type { TimelineEditorialFeedback } from "./timeline-editorial-feedback.ts";
import { classifyDesktopTransportError } from "./transport.ts";
import {
  createViewerFrameIdentity,
  formatViewerComparisonState,
  initialViewerComparison,
} from "./viewer-comparison.ts";

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
  readonly executeProjectActions: (
    actions: readonly ProjectAction[],
  ) => Promise<ExecuteProjectCommandResult>;
  readonly executeProjectCommand: (
    request: ExecuteProjectCommand,
  ) => Promise<ExecuteProjectCommandResult>;
  readonly executePlaybackTransport: (
    action: PlaybackTransportAction,
  ) => Promise<ExecutePlaybackTransportResult>;
  readonly sourceMonitor: SourceMonitorSnapshot | null;
  readonly setSourceMonitor: (snapshot: SourceMonitorSnapshot | null) => void;
  readonly editorialFeedback: TimelineEditorialFeedback | null;
  readonly setEditorialFeedback: (
    feedback: TimelineEditorialFeedback | null,
  ) => void;
  readonly programComparisonSummary: string;
  readonly setProgramComparisonSummary: (summary: string) => void;
}

const ApplicationContext = createContext<ApplicationContextValue | null>(null);
const INITIAL_PROGRAM_COMPARISON_SUMMARY = formatViewerComparisonState(
  initialViewerComparison(),
  createViewerFrameIdentity("program", null, null),
);

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
  const [sourceMonitor, setSourceMonitor] =
    useState<SourceMonitorSnapshot | null>(null);
  const [editorialFeedback, setEditorialFeedback] =
    useState<TimelineEditorialFeedback | null>(null);
  const [programComparisonSummary, setProgramComparisonSummary] = useState(
    INITIAL_PROGRAM_COMPARISON_SUMMARY,
  );
  const editorProjectRef = useRef(editorProject);
  const editorRequestRevision = useRef(0);
  const editorTransactionRevision = useRef(0);
  const projectCommandRevision = useRef(0);
  const playbackCommandRevision = useRef(0);
  stateRef.current = state;
  editorProjectRef.current = editorProject;

  const refreshEditorProject = useCallback(async (): Promise<void> => {
    const requestRevision = editorRequestRevision.current + 1;
    editorRequestRevision.current = requestRevision;
    if (api === null) {
      setEditorProject((current) => {
        const next: EditorProjectPresentation = {
          ...current,
          status: "unavailable",
          failure: null,
        };
        editorProjectRef.current = next;
        return next;
      });
      return;
    }

    const transactionRevision = editorTransactionRevision.current + 1;
    editorTransactionRevision.current = transactionRevision;
    const transactionId = `superi.desktop.project-state.${transactionRevision}`;
    setEditorProject((current) => {
      const next: EditorProjectPresentation = {
        ...current,
        status: current.snapshot === null ? "loading" : "refreshing",
        transactionId,
        failure: null,
      };
      editorProjectRef.current = next;
      return next;
    });

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
      const next: EditorProjectPresentation = {
        status: "ready",
        transactionId,
        commandSequence: result.command_sequence,
        snapshot: result.snapshot,
        failure: null,
      };
      editorProjectRef.current = next;
      setEditorProject(next);
    } catch (error: unknown) {
      if (editorRequestRevision.current !== requestRevision) {
        return;
      }
      const failure = classifyDesktopTransportError(error);
      setEditorProject((current) => {
        const next: EditorProjectPresentation = {
          ...current,
          status: failure.condition === "terminal" ? "failed" : "degraded",
          transactionId,
          failure,
        };
        editorProjectRef.current = next;
        return next;
      });
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

  const executeProjectCommand = useCallback(
    async (
      request: ExecuteProjectCommand,
    ): Promise<ExecuteProjectCommandResult> => {
      if (api === null) {
        throw new Error(
          "Timeline editing is available only through the desktop project owner.",
        );
      }
      let result: ExecuteProjectCommandResult;
      try {
        result = await api.request(
          "superi.project.command.execute",
          request,
        );
      } catch (error: unknown) {
        try {
          await refreshEditorProject();
        } catch {
          // The original command failure remains the actionable result.
        }
        throw error;
      }
      if (result.transaction_id !== request.transaction_id) {
        await refreshEditorProject();
        throw new Error("project command response transaction identity changed");
      }
      await refreshEditorProject();
      return result;
    },
    [api, refreshEditorProject],
  );

  const executeProjectActions = useCallback(
    async (
      actions: readonly ProjectAction[],
    ): Promise<ExecuteProjectCommandResult> => {
      const snapshot = editorProjectRef.current.snapshot;
      if (snapshot === null) {
        throw new Error("A durable project must be open before editing project state.");
      }
      if (actions.length === 0) {
        throw new Error("A project command must contain at least one action.");
      }
      const commandRevision = projectCommandRevision.current + 1;
      projectCommandRevision.current = commandRevision;
      const transactionId = `superi.desktop.project-command.${commandRevision}`;
      try {
        return await executeProjectCommand({
          transaction_id: transactionId,
          expected_project_revision: snapshot.project.project_revision,
          command: {
            command: "apply",
            actions: [...actions],
          },
        });
      } catch (error: unknown) {
        const failure = classifyDesktopTransportError(error);
        throw new Error(`${failure.title} ${failure.action}`);
      }
    },
    [executeProjectCommand],
  );

  const executePlaybackTransport = useCallback(
    async (
      action: PlaybackTransportAction,
    ): Promise<ExecutePlaybackTransportResult> => {
      if (api === null) {
        throw new Error(
          "Playback transport is available only through the desktop playback owner.",
        );
      }
      const commandRevision = playbackCommandRevision.current + 1;
      playbackCommandRevision.current = commandRevision;
      const transactionId = `superi.desktop.playback.${commandRevision}`;
      try {
        const result = await api.request("superi.playback.transport.execute", {
          transaction_id: transactionId,
          command: action,
        });
        if (result.transaction_id !== transactionId) {
          throw new Error(
            "playback command response transaction identity changed",
          );
        }
        if (!result.accepted || !result.pending_command) {
          throw new Error(
            "playback owner did not acknowledge bounded asynchronous execution",
          );
        }

        for (let attempt = 0; attempt < 50; attempt += 1) {
          if (attempt > 0) {
            await waitForPlaybackOwner(4);
          }
          await refreshEditorProject();
          const presentation = editorProjectRef.current;
          if (presentation.failure !== null) {
            throw new Error(
              `${presentation.failure.title} ${presentation.failure.action}`,
            );
          }
          const playback = presentation.snapshot?.playback;
          if (playback?.status === "detached") {
            throw new Error("The desktop playback owner is detached.");
          }
          if (playback?.status === "attached" && !playback.pending_command) {
            if (playback.latest?.failure !== null && playback.latest?.failure !== undefined) {
              throw new Error(
                `Playback command failed with ${playback.latest.failure.category} (${playback.latest.failure.recoverability}).`,
              );
            }
            return result;
          }
        }
        throw new Error(
          "The desktop playback owner did not complete the accepted command in time.",
        );
      } catch (error: unknown) {
        const failure = classifyDesktopTransportError(error);
        throw new Error(`${failure.title} ${failure.action}`);
      }
    },
    [api, refreshEditorProject],
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
        executeProjectActions,
        commandFailure,
        editorProject,
        refreshEditorProject,
        executeProjectCommand,
        executePlaybackTransport,
        sourceMonitor,
        setSourceMonitor,
        editorialFeedback,
        setEditorialFeedback,
        programComparisonSummary,
        setProgramComparisonSummary,
      }}
    >
      {children}
    </ApplicationContext.Provider>
  );
}

function waitForPlaybackOwner(milliseconds: number): Promise<void> {
  return new Promise((resolve) => window.setTimeout(resolve, milliseconds));
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
