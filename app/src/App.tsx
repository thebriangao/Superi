import { useCallback, useEffect, useState, type ComponentType } from "react";

import {
  ApplicationRegistry,
  type ApplicationSelectionReference,
} from "./application.ts";
import {
  ApplicationProvider,
  useApplication,
} from "./application-context";
import type { EngineIntrospectionSnapshot } from "./api";
import { useSuperiApi } from "./api-context";
import {
  getDesktopLifecycle,
  requestDesktopLifecycle,
  type ApplicationLifecycleRequest,
  type DesktopLifecycleSnapshot,
} from "./lifecycle";
import { classifyDesktopTransportError } from "./transport";
import {
  AudioWorkspacePanel,
  ColorWorkspacePanel,
  CompositingWorkspacePanel,
  DeliveryWorkspacePanel,
  EditingWorkspacePanel,
  SharedSelectionPanel,
} from "./editor-workspaces.tsx";

interface ClientFailure {
  readonly summary: string;
  readonly action?: string;
  readonly code?: string;
  readonly recoverability?: string;
}

interface EngineApiStatus {
  readonly condition: string;
  readonly health: string;
  readonly reference: ApplicationSelectionReference;
}

const APPLICATION_LABELS: Record<
  DesktopLifecycleSnapshot["application_phase"],
  string
> = {
  starting: "Starting",
  running: "Ready",
  suspending: "Suspending",
  suspended: "Suspended",
  resuming: "Resuming",
  stopping: "Stopping",
  restarting: "Restarting",
  recovering: "Recovering",
  failed: "Needs attention",
  stopped: "Stopped",
};

const APPLICATION_REGISTRY = new ApplicationRegistry<ComponentType>({
  defaultRouteId: "editing",
  panels: [
    {
      id: "workspace.editing",
      title: "Editing workspace",
      region: "primary",
      renderer: EditingWorkspacePanel,
    },
    {
      id: "workspace.compositing",
      title: "Compositing workspace",
      region: "primary",
      renderer: CompositingWorkspacePanel,
    },
    {
      id: "workspace.color",
      title: "Color workspace",
      region: "primary",
      renderer: ColorWorkspacePanel,
    },
    {
      id: "workspace.audio",
      title: "Audio workspace",
      region: "primary",
      renderer: AudioWorkspacePanel,
    },
    {
      id: "workspace.delivery",
      title: "Delivery workspace",
      region: "primary",
      renderer: DeliveryWorkspacePanel,
    },
    {
      id: "application.selection",
      title: "Shared selection",
      region: "secondary",
      renderer: SharedSelectionPanel,
    },
    {
      id: "application.system",
      title: "System and engine",
      region: "primary",
      renderer: SystemPanel,
    },
  ],
  routes: [
    {
      id: "editing",
      title: "Editing",
      panelIds: ["workspace.editing", "application.selection"],
      defaultPanelId: "workspace.editing",
    },
    {
      id: "compositing",
      title: "Compositing",
      panelIds: ["workspace.compositing", "application.selection"],
      defaultPanelId: "workspace.compositing",
    },
    {
      id: "color",
      title: "Color",
      panelIds: ["workspace.color", "application.selection"],
      defaultPanelId: "workspace.color",
    },
    {
      id: "audio",
      title: "Audio",
      panelIds: ["workspace.audio", "application.selection"],
      defaultPanelId: "workspace.audio",
    },
    {
      id: "delivery",
      title: "Delivery",
      panelIds: ["workspace.delivery", "application.selection"],
      defaultPanelId: "workspace.delivery",
    },
    {
      id: "system",
      title: "System",
      panelIds: ["application.system"],
      defaultPanelId: "application.system",
    },
  ],
  commands: [
    {
      id: "application.route.editing",
      title: "Open editing workspace",
      shortcut: "Mod+1",
      execute: ({ dispatch }) =>
        dispatch({ type: "navigate", routeId: "editing" }),
    },
    {
      id: "application.route.compositing",
      title: "Open compositing workspace",
      shortcut: "Mod+2",
      execute: ({ dispatch }) =>
        dispatch({ type: "navigate", routeId: "compositing" }),
    },
    {
      id: "application.route.color",
      title: "Open color workspace",
      shortcut: "Mod+3",
      execute: ({ dispatch }) =>
        dispatch({ type: "navigate", routeId: "color" }),
    },
    {
      id: "application.route.audio",
      title: "Open audio workspace",
      shortcut: "Mod+4",
      execute: ({ dispatch }) =>
        dispatch({ type: "navigate", routeId: "audio" }),
    },
    {
      id: "application.route.delivery",
      title: "Open delivery workspace",
      shortcut: "Mod+5",
      execute: ({ dispatch }) =>
        dispatch({ type: "navigate", routeId: "delivery" }),
    },
    {
      id: "application.route.system",
      title: "Open system",
      shortcut: "Mod+0",
      execute: ({ dispatch }) =>
        dispatch({ type: "navigate", routeId: "system" }),
    },
    {
      id: "application.selection.clear",
      title: "Clear shared selection",
      shortcut: "Mod+Shift+A",
      isEnabled: ({ state }) => state.selection.items.length > 0,
      execute: ({ dispatch }) => dispatch({ type: "clear_selection" }),
    },
  ],
});

export function App() {
  return (
    <ApplicationProvider registry={APPLICATION_REGISTRY}>
      <ApplicationShell />
    </ApplicationProvider>
  );
}

function ApplicationShell() {
  const {
    registry,
    state,
    dispatch,
    executeCommand,
    commandFailure,
  } = useApplication();
  const route = registry.route(state.activeRouteId);

  return (
    <main className="application-shell" aria-labelledby="product-title">
      <aside className="application-sidebar" aria-label="Application routes">
        <header className="product-lockup">
          <p className="eyebrow">Desktop editor</p>
          <h1 id="product-title">Superi</h1>
        </header>
        <nav className="route-list">
          {registry.routeDefinitions.map((definition, index) => {
            const shortcut = registry.command(
              `application.route.${definition.id}`,
            ).shortcut;
            return (
              <button
                className="route-button"
                type="button"
                key={definition.id}
                aria-current={
                  definition.id === state.activeRouteId ? "page" : undefined
                }
                onClick={() =>
                  void executeCommand(`application.route.${definition.id}`)
                }
              >
                <span>{definition.title}</span>
                <kbd>{shortcut?.split("+").at(-1) ?? index + 1}</kbd>
              </button>
            );
          })}
        </nav>
        <div className="selection-summary" aria-live="polite">
          <span>Shared selection</span>
          <strong>{state.selection.items.length}</strong>
        </div>
      </aside>

      <section className="application-workspace" aria-labelledby="route-title">
        <header className="workspace-header">
          <div>
            <p className="eyebrow">Application route</p>
            <h2 id="route-title">{route.title}</h2>
          </div>
          <div className="panel-controls" aria-label="Visible panels">
            {route.panelIds.map((panelId) => {
              const panel = registry.panel(panelId);
              const visible = state.visiblePanelIds.includes(panelId);
              return (
                <button
                  className="panel-toggle"
                  type="button"
                  key={panelId}
                  aria-pressed={visible}
                  onClick={() =>
                    dispatch({ type: "toggle_panel", panelId })
                  }
                >
                  {panel.title}
                </button>
              );
            })}
          </div>
        </header>

        {commandFailure ? (
          <p className="command-failure" role="alert">
            {commandFailure}
          </p>
        ) : null}

        <div className="workspace-panels">
          {state.visiblePanelIds.map((panelId) => {
            const panel = registry.panel(panelId);
            const Panel = panel.renderer;
            return (
              <section
                className={`workspace-panel panel-${panel.region}`}
                data-panel-id={panel.id}
                key={panel.id}
                tabIndex={-1}
                onFocus={() =>
                  dispatch({ type: "focus_panel", panelId: panel.id })
                }
              >
                <header className="panel-header">
                  <h3>{panel.title}</h3>
                  <span>{panel.region}</span>
                </header>
                <Panel />
              </section>
            );
          })}
          {state.visiblePanelIds.length === 0 ? (
            <div className="empty-route">
              <p>No panels are visible on this route.</p>
              <p>Use the panel controls above to restore one.</p>
            </div>
          ) : null}
        </div>
      </section>
    </main>
  );
}

function SystemPanel() {
  const api = useSuperiApi();
  const { dispatch } = useApplication();
  const [snapshot, setSnapshot] = useState<DesktopLifecycleSnapshot | null>(null);
  const [engineApi, setEngineApi] = useState<EngineApiStatus | null>(null);
  const [clientFailure, setClientFailure] = useState<ClientFailure | null>(null);
  const [requestPending, setRequestPending] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setSnapshot(await getDesktopLifecycle());
      setClientFailure(null);
    } catch {
      setClientFailure({
        summary: "The native lifecycle service is unavailable.",
      });
    }
  }, []);

  useEffect(() => {
    let active = true;
    const update = async () => {
      if (active) {
        await refresh();
      }
    };
    void update();
    const timer = window.setInterval(() => void update(), 1_000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [refresh]);

  useEffect(() => {
    if (api === null) {
      return;
    }
    let active = true;
    const unsubscribe = api.subscribe(
      "superi.engine.introspection.changed",
      ({ snapshot: engine }) => {
        if (active) {
          setEngineApi((current) => ({
            condition: current?.condition ?? "observed",
            health: engine.health,
            reference: engineSelectionReference(engine),
          }));
        }
      },
    );
    void api
      .request("superi.engine.integration.validation.get", null)
      .then(({ snapshot: validation }) => {
        if (active) {
          setEngineApi({
            condition: validation.condition,
            health: validation.engine.health,
            reference: engineSelectionReference(validation.engine),
          });
          setClientFailure(null);
        }
      })
      .catch((error: unknown) => {
        if (active) {
          const failure = classifyDesktopTransportError(error);
          setClientFailure({
            summary: failure.title,
            action: failure.action,
            code: failure.code,
            recoverability: failure.condition,
          });
        }
      });
    return () => {
      active = false;
      unsubscribe();
    };
  }, [api]);

  const request = async (intent: ApplicationLifecycleRequest) => {
    setRequestPending(true);
    try {
      setSnapshot(await requestDesktopLifecycle(intent));
      setClientFailure(null);
    } catch {
      setClientFailure({
        summary: "The lifecycle request could not be accepted in the current state.",
      });
    } finally {
      setRequestPending(false);
    }
  };

  const failure = snapshot?.failure ?? clientFailure;
  const phase = snapshot
    ? APPLICATION_LABELS[snapshot.application_phase]
    : "Connecting";

  return (
    <div className="panel-content system-panel" aria-live="polite">
      <div className="status-row">
        <span className="status-indicator" aria-hidden="true" />
        <div>
          <p className="status-label">Application</p>
          <p className="status-value">{phase}</p>
        </div>
      </div>

      <dl className="lifecycle-details">
        <div>
          <dt>Engine</dt>
          <dd>{snapshot?.engine_phase ?? "starting"}</dd>
        </div>
        <div>
          <dt>Generation</dt>
          <dd>{snapshot?.engine_generation ?? 1}</dd>
        </div>
        <div>
          <dt>Revision</dt>
          <dd>{snapshot?.revision ?? 0}</dd>
        </div>
        <div>
          <dt>Engine API</dt>
          <dd>
            {engineApi
              ? `${engineApi.condition} / ${engineApi.health}`
              : "connecting"}
          </dd>
        </div>
      </dl>

      {failure ? (
        <div className="failure" role="alert">
          <p>{failure.summary}</p>
          {clientFailure?.action ? <p>{clientFailure.action}</p> : null}
          {snapshot?.failure || clientFailure?.code ? (
            <p className="failure-code">
              {snapshot?.failure
                ? `${snapshot.failure.category} / ${snapshot.failure.recoverability}`
                : `${clientFailure?.code} / ${clientFailure?.recoverability}`}
            </p>
          ) : null}
        </div>
      ) : (
        <p className="explanation">
          The native shell remains responsive while the headless engine completes
          lifecycle work outside the application thread.
        </p>
      )}

      <div className="actions" aria-label="System actions">
        {engineApi ? (
          <button
            type="button"
            onClick={() =>
              dispatch({
                type: "replace_selection",
                items: [engineApi.reference],
              })
            }
          >
            Select engine state
          </button>
        ) : null}
        {snapshot?.can_retry ? (
          <button
            type="button"
            disabled={requestPending}
            onClick={() => void request("recover")}
          >
            Retry engine
          </button>
        ) : null}
        <button
          type="button"
          disabled={requestPending || !snapshot?.can_restart}
          onClick={() => void request("restart")}
        >
          Restart engine
        </button>
        <button
          className="secondary"
          type="button"
          disabled={requestPending || !snapshot?.can_shutdown}
          onClick={() => void request("shutdown")}
        >
          Quit Superi
        </button>
      </div>
    </div>
  );
}

function engineSelectionReference(
  engine: EngineIntrospectionSnapshot,
): ApplicationSelectionReference {
  return {
    resource: "superi.engine.introspection",
    schema_version: engine.schema_version,
    identity: "engine",
    revision: engine.revision,
  };
}
