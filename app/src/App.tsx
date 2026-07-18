import { useCallback, useEffect, useState, type ComponentType } from "react";
import { convertFileSrc } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { open } from "@tauri-apps/plugin-dialog";

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
import {
  executeDesktopProject,
  getDesktopProjectSnapshot,
  getDesktopProjectSettings,
  importDesktopMedia,
  inspectProjectMediaSource,
  mutateProjectMediaAnnotations,
  mutateProjectMediaIdentity,
  mutateProjectDerivedMedia,
  mutateProjectOfflineMedia,
  localSearchMedia,
  mutateProjectMediaMetadata,
  mutateProjectMediaLibrary,
  readProjectMediaLibrary,
  updateDesktopProjectSettings,
  type DesktopProjectCommand,
  type DesktopProjectFailure,
  type DesktopProjectSettings,
  type DesktopProjectSettingsUpdate,
  type DesktopProjectSnapshot,
  type DesktopMediaImportOrigin,
  type DesktopMediaImportResult,
  type MediaLibraryMutation,
  type MediaLibrarySnapshot,
  type MediaEditorialAnnotations,
  type MediaSelection,
  type DerivedMediaMutation,
  type OfflineMediaMutation,
  type UserMetadataMutation,
} from "./project-lifecycle";
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
  const [projectSnapshot, setProjectSnapshot] =
    useState<DesktopProjectSnapshot | null>(null);
  const [projectFailure, setProjectFailure] =
    useState<DesktopProjectFailure | null>(null);
  const [projectPending, setProjectPending] = useState(false);
  const [projectPath, setProjectPath] = useState("");
  const [projectName, setProjectName] = useState("Untitled Project");
  const [saveAsPath, setSaveAsPath] = useState("");
  const [projectSettings, setProjectSettings] =
    useState<DesktopProjectSettings | null>(null);
  const [projectSettingsPending, setProjectSettingsPending] = useState(false);
  const [mediaImportPending, setMediaImportPending] = useState(false);
  const [mediaImportResult, setMediaImportResult] =
    useState<DesktopMediaImportResult | null>(null);
  const [mediaLibrary, setMediaLibrary] =
    useState<MediaLibrarySnapshot | null>(null);
  const [mediaViewMode, setMediaViewMode] = useState<"list" | "grid">("grid");
  const [activeBinId, setActiveBinId] = useState<string | null>(null);
  const [activeCollectionId, setActiveCollectionId] = useState<string | null>(null);
  const [selectedMediaId, setSelectedMediaId] = useState<string | null>(null);
  const [newBinName, setNewBinName] = useState("");
  const [newBinParent, setNewBinParent] = useState<string | null>(null);
  const [smartName, setSmartName] = useState("");
  const [smartNeedle, setSmartNeedle] = useState("");
  const [mediaSearch, setMediaSearch] = useState("");
  const [offlineSourcePath, setOfflineSourcePath] = useState("");
  const [replacementFingerprint, setReplacementFingerprint] = useState("");
  const [userMetadataKey, setUserMetadataKey] = useState("");
  const [userMetadataValue, setUserMetadataValue] = useState("");
  const [thumbnailFailures, setThumbnailFailures] =
    useState<ReadonlySet<string>>(new Set());

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
    let active = true;
    void getDesktopProjectSnapshot()
      .then((project) => {
        if (active) {
          setProjectSnapshot(project);
          setProjectFailure(project.failure);
        }
      })
      .catch((error: unknown) => {
        if (active) {
          setProjectFailure(projectFailureFrom(error));
        }
      });
    return () => {
      active = false;
    };
  }, []);

  useEffect(() => {
    let active = true;
    if (!projectSnapshot?.active) {
      setProjectSettings(null);
      return () => {
        active = false;
      };
    }
    void getDesktopProjectSettings()
      .then((settings) => {
        if (active) {
          setProjectSettings(settings);
        }
      })
      .catch((error: unknown) => {
        if (active) {
          setProjectFailure(projectFailureFrom(error));
        }
      });
    return () => {
      active = false;
    };
  }, [
    projectSnapshot?.active?.path,
    projectSnapshot?.active?.identity.project_revision,
  ]);

  useEffect(() => {
    let active = true;
    if (!projectSnapshot?.active) {
      setMediaLibrary(null);
      return () => {
        active = false;
      };
    }
    void readProjectMediaLibrary()
      .then((library) => {
        if (active) {
          setMediaLibrary(library);
        }
      })
      .catch((error: unknown) => {
        if (active) {
          setProjectFailure(projectFailureFrom(error));
        }
      });
    return () => {
      active = false;
    };
  }, [
    projectSnapshot?.active?.path,
    projectSnapshot?.active?.identity.project_revision,
  ]);

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

  const executeProject = async (command: DesktopProjectCommand) => {
    setProjectPending(true);
    try {
      const project = await executeDesktopProject(command);
      setProjectSnapshot(project);
      setProjectFailure(project.failure);
    } catch (error: unknown) {
      setProjectFailure(projectFailureFrom(error));
    } finally {
      setProjectPending(false);
    }
  };

  const saveProjectSettings = async () => {
    if (projectSettings === null) {
      return;
    }
    const { project_revision, ...values } = projectSettings;
    const update: DesktopProjectSettingsUpdate = {
      expected_project_revision: project_revision,
      ...values,
    };
    setProjectSettingsPending(true);
    try {
      const settings = await updateDesktopProjectSettings(update);
      setProjectSettings(settings);
      const project = await getDesktopProjectSnapshot();
      setProjectSnapshot(project);
      setProjectFailure(project.failure);
    } catch (error: unknown) {
      setProjectFailure(projectFailureFrom(error));
    } finally {
      setProjectSettingsPending(false);
    }
  };

  const importMediaPaths = useCallback(
    async (origin: DesktopMediaImportOrigin, paths: readonly string[]) => {
      const active = projectSnapshot?.active;
      if (!active || paths.length === 0) {
        return;
      }
      setMediaImportPending(true);
      try {
        const result = await importDesktopMedia({
          expected_project_revision: active.identity.project_revision,
          origin,
          paths,
          recursive: true,
          detect_image_sequences: true,
        });
        setMediaImportResult(result);
        const project = await getDesktopProjectSnapshot();
        setProjectSnapshot(project);
        setProjectFailure(project.failure);
        setMediaLibrary(await readProjectMediaLibrary());
      } catch (error: unknown) {
        setProjectFailure(projectFailureFrom(error));
      } finally {
        setMediaImportPending(false);
      }
    },
    [
      projectSnapshot?.active?.path,
      projectSnapshot?.active?.identity.project_revision,
    ],
  );

  useEffect(() => {
    if (!projectSnapshot?.active) {
      return;
    }
    const unlisten = getCurrentWebviewWindow().onDragDropEvent((event) => {
      if (event.payload.type === "drop") {
        void importMediaPaths("drag_drop", event.payload.paths);
      }
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, [importMediaPaths, projectSnapshot?.active?.path]);

  const pickMedia = async () => {
    const selected = await open({ multiple: true, directory: false });
    if (selected !== null) {
      await importMediaPaths("picker", Array.isArray(selected) ? selected : [selected]);
    }
  };

  const scanFolder = async () => {
    const selected = await open({ multiple: false, directory: true });
    if (selected !== null) {
      await importMediaPaths("folder_scan", [selected]);
    }
  };

  const mutateMediaLibrary = async (mutation: MediaLibraryMutation) => {
    if (mediaLibrary === null) {
      return;
    }
    try {
      setMediaLibrary(await mutateProjectMediaLibrary(mediaLibrary, mutation));
      setProjectFailure(null);
    } catch (error: unknown) {
      setProjectFailure(projectFailureFrom(error));
    }
  };

  const inspectMediaSource = async () => {
    if (selectedMedia === undefined) {
      return;
    }
    try {
      setMediaLibrary(await inspectProjectMediaSource(selectedMedia));
      setProjectFailure(null);
    } catch (error: unknown) {
      setProjectFailure(projectFailureFrom(error));
    }
  };

  const mutateMediaMetadata = async (mutation: UserMetadataMutation) => {
    if (mediaLibrary === null || selectedMedia === undefined) {
      return;
    }
    try {
      setMediaLibrary(
        await mutateProjectMediaMetadata(
          mediaLibrary,
          selectedMedia.media_id,
          mutation,
        ),
      );
      setProjectFailure(null);
    } catch (error: unknown) {
      setProjectFailure(projectFailureFrom(error));
    }
  };

  const mutateMediaAnnotations = async (
    annotations: MediaEditorialAnnotations,
  ) => {
    if (mediaLibrary === null || selectedMedia === undefined) {
      return;
    }
    try {
      setMediaLibrary(
        await mutateProjectMediaAnnotations(
          mediaLibrary,
          selectedMedia.media_id,
          annotations,
        ),
      );
      setProjectFailure(null);
    } catch (error: unknown) {
      setProjectFailure(projectFailureFrom(error));
    }
  };

  const mutateMediaIdentity = async (selections: readonly MediaSelection[]) => {
    if (mediaLibrary === null || selectedMedia === undefined) {
      return;
    }
    try {
      setMediaLibrary(
        await mutateProjectMediaIdentity(
          mediaLibrary,
          selectedMedia.media_id,
          selections,
        ),
      );
      setProjectFailure(null);
    } catch (error: unknown) {
      setProjectFailure(projectFailureFrom(error));
    }
  };

  const mutateDerivedMedia = async (mutation: DerivedMediaMutation) => {
    if (mediaLibrary === null || selectedMedia === undefined) {
      return;
    }
    try {
      setMediaLibrary(
        await mutateProjectDerivedMedia(
          mediaLibrary,
          selectedMedia.media_id,
          mutation,
        ),
      );
      setProjectFailure(null);
    } catch (error: unknown) {
      setProjectFailure(projectFailureFrom(error));
    }
  };

  const mutateOfflineMedia = async (mutation: OfflineMediaMutation) => {
    if (mediaLibrary === null || selectedMedia === undefined) return;
    try {
      setMediaLibrary(
        await mutateProjectOfflineMedia(mediaLibrary, selectedMedia.media_id, mutation),
      );
      setProjectFailure(null);
    } catch (error: unknown) {
      setProjectFailure(projectFailureFrom(error));
    }
  };

  const createProject = () => {
    const identity = crypto.randomUUID();
    void executeProject({
      kind: "create",
      path: projectPath,
      project: {
        project_id: identity,
        project_name: projectName.trim() || "Untitled Project",
        root_timeline_id: crypto.randomUUID(),
        root_timeline_name: `${projectName.trim() || "Untitled Project"} Timeline`,
        edit_rate_numerator: 24,
        edit_rate_denominator: 1,
      },
    });
  };

  const failure = snapshot?.failure ?? clientFailure;
  const phase = snapshot
    ? APPLICATION_LABELS[snapshot.application_phase]
    : "Connecting";
  const activeCollection = mediaLibrary?.smart_collections.find(
    (collection) => collection.collection_id === activeCollectionId,
  );
  const visibleMedia = localSearchMedia(
    mediaLibrary?.items.filter((item) => {
      if (activeCollection) {
        return activeCollection.media_ids.includes(item.media_id);
      }
      return activeBinId === null || item.bin_id === activeBinId;
    }) ?? [],
    mediaSearch,
  );
  const selectedMedia = mediaLibrary?.items.find(
    (item) => item.media_id === selectedMediaId,
  );

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

      <section aria-labelledby="project-lifecycle-title">
        <h4 id="project-lifecycle-title">Project lifecycle</h4>
        <p className="explanation">
          {projectSnapshot?.active
            ? `${projectSnapshot.active.identity.project_id} at ${projectSnapshot.active.path}`
            : "No project is open."}
        </p>

        {projectFailure ? (
          <div className="failure" role="alert">
            <p>{projectFailure.title}</p>
            <p>{projectFailure.action}</p>
            <p className="failure-code">
              {projectFailure.code} / {projectFailure.class}
            </p>
          </div>
        ) : null}

        <label>
          Project path
          <input
            value={projectPath}
            onChange={(event) => setProjectPath(event.currentTarget.value)}
          />
        </label>
        <label>
          Project name
          <input
            value={projectName}
            onChange={(event) => setProjectName(event.currentTarget.value)}
          />
        </label>
        <div className="actions" aria-label="Project open actions">
          <button
            type="button"
            disabled={projectPending || projectPath.trim().length === 0}
            onClick={createProject}
          >
            Create
          </button>
          <button
            type="button"
            disabled={projectPending || projectPath.trim().length === 0}
            onClick={() =>
              void executeProject({ kind: "open", path: projectPath })
            }
          >
            Open
          </button>
          <button
            type="button"
            disabled={projectPending || projectSnapshot?.active === null}
            onClick={() => void executeProject({ kind: "save" })}
          >
            Save
          </button>
          <button
            className="secondary"
            type="button"
            disabled={projectPending || projectSnapshot?.active === null}
            onClick={() => void executeProject({ kind: "close" })}
          >
            Close
          </button>
        </div>

        {projectSnapshot?.active ? (
          <div className="actions" aria-label="Media import actions">
            <button
              type="button"
              disabled={projectPending || mediaImportPending}
              onClick={() => void pickMedia()}
            >
              Import media
            </button>
            <button
              type="button"
              className="secondary"
              disabled={projectPending || mediaImportPending}
              onClick={() => void scanFolder()}
            >
              Scan folder
            </button>
            <span className="explanation">
              Drop files or folders anywhere in the window.
            </span>
          </div>
        ) : null}
        {mediaImportResult ? (
          <p className="explanation" role="status">
            Imported {mediaImportResult.imported.length} source
            {mediaImportResult.imported.length === 1 ? "" : "s"}; skipped{" "}
            {mediaImportResult.skipped.length}. Project revision{" "}
            {mediaImportResult.project_revision}.
          </p>
        ) : null}

        {projectSnapshot?.active && mediaLibrary ? (
          <section className="media-browser" data-testid="media-browser">
            <header className="media-browser-header">
              <div>
                <p className="eyebrow">Project media</p>
                <h4>Media library</h4>
              </div>
              <div className="media-view-switch" aria-label="Media view">
                <button
                  type="button"
                  aria-pressed={mediaViewMode === "list"}
                  onClick={() => setMediaViewMode("list")}
                >List</button>
                <button
                  type="button"
                  aria-pressed={mediaViewMode === "grid"}
                  onClick={() => setMediaViewMode("grid")}
                >Grid</button>
              </div>
            </header>
            <label>
              Search local media
              <input
                type="search"
                value={mediaSearch}
                onChange={(event) => setMediaSearch(event.currentTarget.value)}
                placeholder="Name, path, metadata, label, or offline state"
              />
            </label>

            <div className="media-browser-layout">
              <aside className="media-browser-navigation">
                <h5>Bins</h5>
                <button
                  type="button"
                  className="media-scope"
                  aria-pressed={activeBinId === null && activeCollectionId === null}
                  onClick={() => {
                    setActiveBinId(null);
                    setActiveCollectionId(null);
                  }}
                >All media</button>
                {mediaLibrary.bins.map((bin) => (
                  <button
                    type="button"
                    className="media-scope"
                    style={{ paddingInlineStart: bin.parent_id ? "1.5rem" : undefined }}
                    aria-pressed={activeBinId === bin.bin_id}
                    key={bin.bin_id}
                    onClick={() => {
                      setActiveBinId(bin.bin_id);
                      setActiveCollectionId(null);
                    }}
                  >{bin.name}</button>
                ))}
                <label>
                  Bin name
                  <input
                    value={newBinName}
                    onChange={(event) => setNewBinName(event.currentTarget.value)}
                  />
                </label>
                <label>
                  Parent bin
                  <select
                    value={newBinParent ?? ""}
                    onChange={(event) =>
                      setNewBinParent(event.currentTarget.value || null)
                    }
                  >
                    <option value="">Root</option>
                    {mediaLibrary.bins.map((bin) => (
                      <option value={bin.bin_id} key={bin.bin_id}>{bin.name}</option>
                    ))}
                  </select>
                </label>
                <button
                  type="button"
                  disabled={newBinName.trim().length === 0}
                  onClick={() => {
                    void mutateMediaLibrary({
                      kind: "create_bin",
                      bin_id: crypto.randomUUID(),
                      name: newBinName.trim(),
                      parent_id: newBinParent,
                    });
                    setNewBinName("");
                  }}
                >Add bin</button>

                <h5>Smart collections</h5>
                {mediaLibrary.smart_collections.map((collection) => (
                  <button
                    type="button"
                    className="media-scope"
                    aria-pressed={activeCollectionId === collection.collection_id}
                    key={collection.collection_id}
                    onClick={() => {
                      setActiveCollectionId(collection.collection_id);
                      setActiveBinId(null);
                    }}
                  >{collection.name}</button>
                ))}
                <label>
                  Collection name
                  <input
                    value={smartName}
                    onChange={(event) => setSmartName(event.currentTarget.value)}
                  />
                </label>
                <label>
                  Name contains
                  <input
                    value={smartNeedle}
                    onChange={(event) => setSmartNeedle(event.currentTarget.value)}
                  />
                </label>
                <button
                  type="button"
                  disabled={
                    smartName.trim().length === 0 || smartNeedle.trim().length === 0
                  }
                  onClick={() => {
                    void mutateMediaLibrary({
                      kind: "upsert_smart_collection",
                      collection_id: crypto.randomUUID(),
                      name: smartName.trim(),
                      name_contains: smartNeedle.trim(),
                    });
                    setSmartName("");
                    setSmartNeedle("");
                  }}
                >Save collection</button>
              </aside>

              <div className={`media-items media-items-${mediaViewMode}`}>
                {visibleMedia.map((item) => {
                  const showSource =
                    item.thumbnail.kind === "source" &&
                    !thumbnailFailures.has(item.media_id);
                  return (
                    <button
                      type="button"
                      className="media-item"
                      aria-pressed={selectedMediaId === item.media_id}
                      key={item.media_id}
                      onClick={() => setSelectedMediaId(item.media_id)}
                    >
                      <span className="media-thumbnail">
                        {showSource && item.thumbnail.kind === "source" ? (
                          <img
                            alt=""
                            src={convertFileSrc(item.thumbnail.source_path)}
                            onError={() =>
                              setThumbnailFailures((current) =>
                                new Set(current).add(item.media_id),
                              )
                            }
                          />
                        ) : (
                          <span className="thumbnail_fallback" aria-label="Thumbnail unavailable">
                            {item.kind === "image_sequence" ? "SEQ" : "MEDIA"}
                          </span>
                        )}
                      </span>
                      <span className="media-item-copy">
                        <strong>{item.name}</strong>
                        <small>{item.kind.replace("_", " ")}</small>
                      </span>
                    </button>
                  );
                })}
                {visibleMedia.length === 0 ? (
                  <p className="explanation">No media in this view.</p>
                ) : null}
              </div>

              <aside className="media-metadata">
                <h5>Metadata</h5>
                {selectedMedia ? (
                  <>
                    <strong>{selectedMedia.name}</strong>
                    <section aria-label="Offline media">
                      <h5>Offline media</h5>
                      <p>{selectedMedia.offline.status}</p>
                      <p>
                        {selectedMedia.offline.available_paths.length} available / {selectedMedia.offline.missing_paths.length} missing
                        {selectedMedia.offline.derived_fallback_available ? " / derived fallback ready" : ""}
                      </p>
                      <label>
                        Local source path
                        <input
                          value={offlineSourcePath}
                          onChange={(event) => setOfflineSourcePath(event.currentTarget.value)}
                        />
                      </label>
                      <label>
                        Replacement fingerprint
                        <input
                          value={replacementFingerprint}
                          onChange={(event) => setReplacementFingerprint(event.currentTarget.value)}
                        />
                      </label>
                      <div className="actions">
                        <button type="button" disabled={!offlineSourcePath.trim()} onClick={() => void mutateOfflineMedia({
                          kind: "relink",
                          source_paths: [offlineSourcePath.trim()],
                          candidate_fingerprint: selectedMedia.content_fingerprint,
                        })}>Relink source</button>
                        <button type="button" disabled={!offlineSourcePath.trim() || !replacementFingerprint.trim()} onClick={() => void mutateOfflineMedia({
                          kind: "replace",
                          source_paths: [offlineSourcePath.trim()],
                          replacement_fingerprint: replacementFingerprint.trim(),
                        })}>Replace source</button>
                        <button type="button" onClick={() => void mutateOfflineMedia({
                          kind: "conform",
                          frame_rate_numerator: selectedMedia.frame_rate_numerator ?? 24,
                          frame_rate_denominator: selectedMedia.frame_rate_denominator ?? 1,
                        })}>Conform source</button>
                      </div>
                    </section>
                    <dl>
                      {Object.entries(selectedMedia.metadata).map(([key, value]) => (
                        <div key={key}>
                          <dt>{key.replaceAll("_", " ")}</dt>
                          <dd>{value}</dd>
                        </div>
                      ))}
                    </dl>
                    <h5>Source metadata</h5>
                    <p className="source-metadata-status">
                      {selectedMedia.source_metadata.status} inspection {selectedMedia.source_metadata.inspection_generation}
                    </p>
                    <dl>
                      {Object.entries(selectedMedia.source_metadata.fields).map(([key, value]) => (
                        <div key={key}>
                          <dt>{key.replaceAll("_", " ")}</dt>
                          <dd>{value}</dd>
                        </div>
                      ))}
                    </dl>
                    <button type="button" onClick={() => void inspectMediaSource()}>
                      Inspect source
                    </button>
                    <form
                      className="media-annotation-editor"
                      key={`${selectedMedia.media_id}:${mediaLibrary?.revision ?? 0}`}
                      onSubmit={(event) => {
                        event.preventDefault();
                        const fields = new FormData(event.currentTarget);
                        const terms = (name: string) =>
                          String(fields.get(name) ?? "")
                            .split(",")
                            .map((value) => value.trim())
                            .filter((value) => value.length > 0);
                        const optional = (name: string) => {
                          const value = String(fields.get(name) ?? "").trim();
                          return value.length === 0 ? null : value;
                        };
                        const rating = String(fields.get("rating") ?? "");
                        void mutateMediaAnnotations({
                          clip_name: optional("clip_name"),
                          labels: terms("labels"),
                          rating: rating.length === 0 ? null : Number(rating),
                          keywords: terms("keywords"),
                          comment: optional("comment"),
                          favorite: fields.get("favorite") === "on",
                        });
                      }}
                    >
                      <h5>Editorial annotations</h5>
                      <label>
                        Clip name
                        <input
                          name="clip_name"
                          defaultValue={selectedMedia.annotations.clip_name ?? ""}
                          maxLength={256}
                        />
                      </label>
                      <label>
                        Labels, comma separated
                        <input
                          name="labels"
                          defaultValue={selectedMedia.annotations.labels.join(", ")}
                        />
                      </label>
                      <label>
                        Rating
                        <input
                          name="rating"
                          type="number"
                          min={1}
                          max={5}
                          defaultValue={selectedMedia.annotations.rating ?? ""}
                        />
                      </label>
                      <label>
                        Keywords, comma separated
                        <input
                          name="keywords"
                          defaultValue={selectedMedia.annotations.keywords.join(", ")}
                        />
                      </label>
                      <label>
                        Comment
                        <textarea
                          name="comment"
                          defaultValue={selectedMedia.annotations.comment ?? ""}
                          maxLength={4096}
                        />
                      </label>
                      <label>
                        <input
                          name="favorite"
                          type="checkbox"
                          defaultChecked={selectedMedia.annotations.favorite}
                        />
                        Favorite
                      </label>
                      <button type="submit">Save annotations</button>
                    </form>
                    <section aria-label="Media usage">
                      <h5>Usage</h5>
                      <p>
                        {selectedMedia.usage.clip_count} clips across {selectedMedia.usage.timeline_count} timelines
                      </p>
                      {selectedMedia.usage.timeline_ids.length > 0 ? (
                        <p>{selectedMedia.usage.timeline_ids.join(", ")}</p>
                      ) : null}
                    </section>
                    <section aria-label="Media identity and selections">
                      <h5>Identity and selections</h5>
                      <p>
                        Canonical media: {selectedMedia.identity_tracking.canonical_media_id}
                      </p>
                      <p>
                        Fingerprint: {selectedMedia.identity_tracking.content_fingerprint}
                      </p>
                      <p>
                        Exact duplicates: {selectedMedia.identity_tracking.duplicate_media_ids.length}
                      </p>
                      {selectedMedia.identity_tracking.duplicate_media_ids.length > 0 ? (
                        <p>{selectedMedia.identity_tracking.duplicate_media_ids.join(", ")}</p>
                      ) : null}
                      <form
                        onSubmit={(event) => {
                          event.preventDefault();
                          const fields = new FormData(event.currentTarget);
                          const start = Number(fields.get("selection_start"));
                          const end = Number(fields.get("selection_end"));
                          const selectionId = crypto.randomUUID();
                          const regionId = crypto.randomUUID();
                          void mutateMediaIdentity([
                            ...selectedMedia.selections,
                            {
                              selection_id: selectionId,
                              name: String(fields.get("selection_name") ?? "").trim(),
                              start_frame: start,
                              end_frame: end,
                              rate_numerator: 24,
                              rate_denominator: 1,
                              tracked_regions: [
                                {
                                  region_id: regionId,
                                  observations: [
                                    {
                                      frame: start,
                                      x_millionths: 0,
                                      y_millionths: 0,
                                      width_millionths: 1_000_000,
                                      height_millionths: 1_000_000,
                                    },
                                  ],
                                },
                              ],
                            },
                          ]);
                        }}
                      >
                        <label>
                          Selection name
                          <input name="selection_name" required maxLength={128} />
                        </label>
                        <label>
                          Start frame
                          <input name="selection_start" type="number" required defaultValue={0} />
                        </label>
                        <label>
                          End frame
                          <input name="selection_end" type="number" required defaultValue={1} />
                        </label>
                        <button type="submit">Add reusable selection</button>
                      </form>
                      {selectedMedia.selections.map((selection) => (
                        <article key={selection.selection_id}>
                          <strong>{selection.name}</strong>
                          <p>
                            Frames {selection.start_frame} to {selection.end_frame} at {selection.rate_numerator}/{selection.rate_denominator}
                          </p>
                          {selection.tracked_regions.map((region) => (
                            <div key={region.region_id}>
                              {region.observations.map((observation) => (
                                <form
                                  key={observation.frame}
                                  onSubmit={(event) => {
                                    event.preventDefault();
                                    const fields = new FormData(event.currentTarget);
                                    const replacement = {
                                      ...observation,
                                      x_millionths: Number(fields.get("x_millionths")),
                                      y_millionths: Number(fields.get("y_millionths")),
                                      width_millionths: Number(fields.get("width_millionths")),
                                      height_millionths: Number(fields.get("height_millionths")),
                                    };
                                    void mutateMediaIdentity(
                                      selectedMedia.selections.map((candidate) =>
                                        candidate.selection_id !== selection.selection_id
                                          ? candidate
                                          : {
                                              ...candidate,
                                              tracked_regions: candidate.tracked_regions.map((tracked) =>
                                                tracked.region_id !== region.region_id
                                                  ? tracked
                                                  : {
                                                      ...tracked,
                                                      observations: tracked.observations.map((sample) =>
                                                        sample.frame === observation.frame
                                                          ? replacement
                                                          : sample,
                                                      ),
                                                    },
                                              ),
                                            },
                                      ),
                                    );
                                  }}
                                >
                                  <span>Frame {observation.frame}</span>
                                  <input name="x_millionths" type="number" min={0} max={1_000_000} defaultValue={observation.x_millionths} />
                                  <input name="y_millionths" type="number" min={0} max={1_000_000} defaultValue={observation.y_millionths} />
                                  <input name="width_millionths" type="number" min={1} max={1_000_000} defaultValue={observation.width_millionths} />
                                  <input name="height_millionths" type="number" min={1} max={1_000_000} defaultValue={observation.height_millionths} />
                                  <button type="submit">Refine tracked region</button>
                                </form>
                              ))}
                            </div>
                          ))}
                        </article>
                      ))}
                    </section>
                    <section aria-label="Proxy and optimized media">
                      <h5>Proxy and optimized media</h5>
                      <p>
                        Active: {selectedMedia.resolved_representation.representation}
                        {selectedMedia.resolved_representation.fallback_to_original
                          ? " (deterministic original fallback)"
                          : ""}
                      </p>
                      <div className="actions">
                        <button
                          type="button"
                          onClick={() =>
                            void mutateDerivedMedia({
                              kind: "set_choice",
                              choice: { kind: "original" },
                            })
                          }
                        >
                          Use original
                        </button>
                        <button
                          type="button"
                          onClick={() =>
                            void mutateDerivedMedia({
                              kind: "create_or_replace",
                              artifact_id: crypto.randomUUID(),
                              purpose: "proxy",
                              quality: "quarter",
                              status: "ready",
                              source_fingerprint: selectedMedia.content_fingerprint,
                              source_revision: mediaLibrary.project_revision,
                              byte_len: 1,
                            })
                          }
                        >
                          Create or replace proxy
                        </button>
                        <button
                          type="button"
                          onClick={() =>
                            void mutateDerivedMedia({
                              kind: "set_choice",
                              choice: { kind: "proxy", quality: "quarter" },
                            })
                          }
                        >
                          Use quarter proxy
                        </button>
                        <button
                          type="button"
                          onClick={() =>
                            void mutateDerivedMedia({
                              kind: "create_or_replace",
                              artifact_id: crypto.randomUUID(),
                              purpose: "optimized",
                              quality: "full",
                              status: "ready",
                              source_fingerprint: selectedMedia.content_fingerprint,
                              source_revision: mediaLibrary.project_revision,
                              byte_len: 1,
                            })
                          }
                        >
                          Create or replace optimized media
                        </button>
                      </div>
                      <ul>
                        {selectedMedia.derived_media.map((attachment) => (
                          <li key={`${attachment.purpose}:${attachment.quality}`}>
                            {attachment.purpose} / {attachment.quality} / {attachment.status}
                          </li>
                        ))}
                      </ul>
                    </section>
                    <div className="user-metadata-editor">
                      <h5>User metadata</h5>
                      <dl>
                        {Object.entries(selectedMedia.user_metadata).map(([key, value]) => (
                          <div key={key}>
                            <dt>{key}</dt>
                            <dd>{value}</dd>
                            <button
                              type="button"
                              onClick={() => void mutateMediaMetadata({ kind: "remove", key })}
                            >Remove</button>
                          </div>
                        ))}
                      </dl>
                      <label>
                        Metadata key
                        <input
                          value={userMetadataKey}
                          onChange={(event) => setUserMetadataKey(event.currentTarget.value)}
                        />
                      </label>
                      <label>
                        Metadata value
                        <input
                          value={userMetadataValue}
                          onChange={(event) => setUserMetadataValue(event.currentTarget.value)}
                        />
                      </label>
                      <button
                        type="button"
                        disabled={userMetadataKey.trim().length === 0}
                        onClick={() => {
                          void mutateMediaMetadata({
                            kind: "upsert",
                            key: userMetadataKey.trim(),
                            value: userMetadataValue,
                          });
                          setUserMetadataKey("");
                          setUserMetadataValue("");
                        }}
                      >Save metadata</button>
                    </div>
                    <button
                      type="button"
                      disabled={activeBinId === null}
                      onClick={() =>
                        void mutateMediaLibrary({
                          kind: "move_media",
                          media_id: selectedMedia.media_id,
                          bin_id: activeBinId,
                        })
                      }
                    >Move to active bin</button>
                  </>
                ) : (
                  <p className="explanation">Select media to inspect its current identity.</p>
                )}
              </aside>
            </div>
          </section>
        ) : null}

        <label>
          Save-as path
          <input
            value={saveAsPath}
            onChange={(event) => setSaveAsPath(event.currentTarget.value)}
          />
        </label>
        <div className="actions" aria-label="Project save and recovery actions">
          <button
            type="button"
            disabled={
              projectPending ||
              projectSnapshot?.active === null ||
              saveAsPath.trim().length === 0
            }
            onClick={() =>
              void executeProject({
                kind: "save_as",
                destination: saveAsPath,
                replace_existing: false,
              })
            }
          >
            Save as
          </button>
          <button
            type="button"
            disabled={projectPending || projectSnapshot?.active === null}
            onClick={() =>
              void executeProject({ kind: "discover_recovery" })
            }
          >
            Find recovery
          </button>
        </div>

        {projectSettings ? (
          <fieldset disabled={projectSettingsPending || projectPending}>
            <legend>Project settings</legend>
            <p className="explanation">
              Revision {projectSettings.project_revision}. Audio sample timing and
              channel layout are stored as project authority.
            </p>
            <label>
              Frame-rate numerator
              <input
                type="number"
                min="1"
                value={projectSettings.frame_rate_numerator}
                onChange={(event) =>
                  setProjectSettings({
                    ...projectSettings,
                    frame_rate_numerator: Number(event.currentTarget.value),
                  })
                }
              />
            </label>
            <label>
              Frame-rate denominator
              <input
                type="number"
                min="1"
                value={projectSettings.frame_rate_denominator}
                onChange={(event) =>
                  setProjectSettings({
                    ...projectSettings,
                    frame_rate_denominator: Number(event.currentTarget.value),
                  })
                }
              />
            </label>
            <label>
              Timecode
              <select
                value={projectSettings.timecode_mode}
                onChange={(event) =>
                  setProjectSettings({
                    ...projectSettings,
                    timecode_mode: event.currentTarget.value,
                  })
                }
              >
                <option value="non_drop_frame">Non-drop frame</option>
                <option value="drop_frame">Drop frame</option>
              </select>
            </label>
            <label>
              Resolution width
              <input
                type="number"
                min="1"
                value={projectSettings.resolution_width ?? ""}
                onChange={(event) =>
                  setProjectSettings({
                    ...projectSettings,
                    resolution_width: optionalNumber(event.currentTarget.value),
                  })
                }
              />
            </label>
            <label>
              Resolution height
              <input
                type="number"
                min="1"
                value={projectSettings.resolution_height ?? ""}
                onChange={(event) =>
                  setProjectSettings({
                    ...projectSettings,
                    resolution_height: optionalNumber(event.currentTarget.value),
                  })
                }
              />
            </label>
            <label>
              Color mode
              <select
                value={projectSettings.color_mode}
                onChange={(event) => {
                  const colorMode = event.currentTarget.value;
                  setProjectSettings({
                    ...projectSettings,
                    color_mode: colorMode,
                    color_working_space:
                      colorMode === "built_in_acescg"
                        ? "acescg"
                        : projectSettings.color_working_space,
                    color_config_id:
                      colorMode === "built_in_acescg"
                        ? null
                        : projectSettings.color_config_id,
                    color_config_fingerprint:
                      colorMode === "built_in_acescg"
                        ? null
                        : projectSettings.color_config_fingerprint,
                  });
                }}
              >
                <option value="built_in_acescg">Built-in ACEScg</option>
                <option value="pinned_config">Pinned config</option>
              </select>
            </label>
            <label>
              Working color space
              <input
                value={projectSettings.color_working_space}
                onChange={(event) =>
                  setProjectSettings({
                    ...projectSettings,
                    color_working_space: event.currentTarget.value,
                  })
                }
              />
            </label>
            {projectSettings.color_mode === "pinned_config" ? (
              <>
                <label>
                  Color config ID
                  <input
                    value={projectSettings.color_config_id ?? ""}
                    onChange={(event) =>
                      setProjectSettings({
                        ...projectSettings,
                        color_config_id: optionalText(event.currentTarget.value),
                      })
                    }
                  />
                </label>
                <label>
                  Color config fingerprint
                  <input
                    value={projectSettings.color_config_fingerprint ?? ""}
                    onChange={(event) =>
                      setProjectSettings({
                        ...projectSettings,
                        color_config_fingerprint: optionalText(
                          event.currentTarget.value,
                        ),
                      })
                    }
                  />
                </label>
              </>
            ) : null}
            <label>
              Audio sample rate (Hz)
              <input
                type="number"
                min="1"
                value={projectSettings.audio_sample_rate_hz}
                onChange={(event) =>
                  setProjectSettings({
                    ...projectSettings,
                    audio_sample_rate_hz: Number(event.currentTarget.value),
                  })
                }
              />
            </label>
            <label>
              Audio channel layout
              <select
                value={projectSettings.audio_output_layout}
                onChange={(event) =>
                  setProjectSettings({
                    ...projectSettings,
                    audio_output_layout: event.currentTarget.value,
                  })
                }
              >
                <option value="mono">Mono</option>
                <option value="stereo">Stereo</option>
                <option value="quad">Quad</option>
                <option value="surround_5_1">5.1 surround</option>
                <option value="surround_7_1">7.1 surround</option>
              </select>
            </label>
            <label>
              Cache policy
              <select
                value={projectSettings.cache_mode}
                onChange={(event) => {
                  const cacheMode = event.currentTarget.value;
                  setProjectSettings({
                    ...projectSettings,
                    cache_mode: cacheMode,
                    cache_max_bytes:
                      cacheMode === "bounded"
                        ? (projectSettings.cache_max_bytes ?? 8 * 1_024 * 1_024)
                        : null,
                    cache_max_frames:
                      cacheMode === "bounded"
                        ? (projectSettings.cache_max_frames ?? 96)
                        : null,
                  });
                }}
              >
                <option value="automatic">Automatic</option>
                <option value="bounded">Bounded</option>
                <option value="disabled">Disabled</option>
              </select>
            </label>
            {projectSettings.cache_mode === "bounded" ? (
              <>
                <label>
                  Cache bytes
                  <input
                    type="number"
                    min="1"
                    value={projectSettings.cache_max_bytes ?? ""}
                    onChange={(event) =>
                      setProjectSettings({
                        ...projectSettings,
                        cache_max_bytes: optionalNumber(
                          event.currentTarget.value,
                        ),
                      })
                    }
                  />
                </label>
                <label>
                  Cache frames
                  <input
                    type="number"
                    min="1"
                    value={projectSettings.cache_max_frames ?? ""}
                    onChange={(event) =>
                      setProjectSettings({
                        ...projectSettings,
                        cache_max_frames: optionalNumber(
                          event.currentTarget.value,
                        ),
                      })
                    }
                  />
                </label>
              </>
            ) : null}
            <label>
              Proxy policy
              <select
                value={projectSettings.proxy_mode}
                onChange={(event) =>
                  setProjectSettings({
                    ...projectSettings,
                    proxy_mode: event.currentTarget.value,
                  })
                }
              >
                <option value="disabled">Disabled</option>
                <option value="on_demand">On demand</option>
                <option value="prefer">Prefer proxies</option>
              </select>
            </label>
            <label>
              Proxy quality
              <select
                value={projectSettings.proxy_quality}
                onChange={(event) =>
                  setProjectSettings({
                    ...projectSettings,
                    proxy_quality: event.currentTarget.value,
                  })
                }
              >
                <option value="eighth">Eighth</option>
                <option value="quarter">Quarter</option>
                <option value="half">Half</option>
                <option value="full">Full</option>
              </select>
            </label>
            <label>
              Working folder
              <input
                value={projectSettings.working_folder ?? ""}
                onChange={(event) =>
                  setProjectSettings({
                    ...projectSettings,
                    working_folder: optionalText(event.currentTarget.value),
                  })
                }
              />
            </label>
            <label>
              Cache folder
              <input
                value={projectSettings.cache_folder ?? ""}
                onChange={(event) =>
                  setProjectSettings({
                    ...projectSettings,
                    cache_folder: optionalText(event.currentTarget.value),
                  })
                }
              />
            </label>
            <label>
              Proxy folder
              <input
                value={projectSettings.proxy_folder ?? ""}
                onChange={(event) =>
                  setProjectSettings({
                    ...projectSettings,
                    proxy_folder: optionalText(event.currentTarget.value),
                  })
                }
              />
            </label>
            <div className="actions">
              <button type="button" onClick={() => void saveProjectSettings()}>
                Save project settings
              </button>
            </div>
          </fieldset>
        ) : null}

        {projectSnapshot?.recent.length ? (
          <div className="actions" aria-label="Recent projects">
            {projectSnapshot.recent.map((recent) => (
              <button
                className="secondary"
                type="button"
                key={recent.path}
                disabled={projectPending}
                onClick={() =>
                  void executeProject({
                    kind: "open_recent",
                    path: recent.path,
                  })
                }
              >
                {recent.path}
              </button>
            ))}
          </div>
        ) : null}

        {projectSnapshot?.recovery?.candidates.map((candidate) => (
          <button
            type="button"
            key={candidate.candidate_id}
            disabled={projectPending}
            onClick={() =>
              void executeProject({
                kind: "restore_recovery",
                catalog_revision:
                  projectSnapshot.recovery?.catalog_revision ?? 0,
                candidate_id: candidate.candidate_id,
              })
            }
          >
            {candidate.action}
          </button>
        ))}
      </section>
    </div>
  );
}

function optionalNumber(value: string): number | null {
  return value.length === 0 ? null : Number(value);
}

function optionalText(value: string): string | null {
  return value.length === 0 ? null : value;
}

function projectFailureFrom(error: unknown): DesktopProjectFailure {
  if (
    typeof error === "object" &&
    error !== null &&
    "class" in error &&
    "code" in error &&
    "title" in error &&
    "action" in error
  ) {
    return error as DesktopProjectFailure;
  }
  return {
    class: "terminal",
    code: "project_transport_unavailable",
    title: "Project service is unavailable",
    action: "Restart Superi before continuing.",
    context: {},
  };
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
