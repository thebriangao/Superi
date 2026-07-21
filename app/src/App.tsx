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
  generateProjectMediaPreview,
  getDesktopProjectSnapshot,
  getDesktopProjectSettings,
  importDesktopMedia,
  inspectProjectMediaSource,
  listenForDesktopProjectOpen,
  mutateProjectMediaAnnotations,
  mutateProjectMediaContentAnalysis,
  mutateProjectMediaBatch,
  mutateProjectMediaIdentity,
  mutateProjectDerivedMedia,
  mutateProjectOfflineMedia,
  searchProjectMediaContent,
  mutateProjectMediaMetadata,
  mutateProjectMediaLibrary,
  readProjectMediaLibrary,
  scanProjectMediaSources,
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
  type MediaBatchOperation,
  type MediaPreviewBundle,
  type MediaEditorialAnnotations,
  type MediaBrowserItem,
  type MediaContentAnalysis,
  type MediaContentSearchSnapshot,
  type MediaLocalAiContent,
  type MediaSelection,
  type MediaTimelineRelationship,
  type MediaTranscriptSegment,
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

function sourceBasename(path: string): string {
  return path.split(/[\\/]/u).filter(Boolean).at(-1) ?? path;
}

function sourcePathUnderRoot(root: string, path: string): string {
  const trimmedRoot = root.replace(/[\\/]+$/u, "");
  const separator = root.includes("\\") && !root.includes("/") ? "\\" : "/";
  return `${trimmedRoot}${separator}${sourceBasename(path)}`;
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
  const [contentSearch, setContentSearch] =
    useState<MediaContentSearchSnapshot | null>(null);
  const [contentSearchPending, setContentSearchPending] = useState(false);
  const [offlineSourcePath, setOfflineSourcePath] = useState("");
  const [replacementFingerprint, setReplacementFingerprint] = useState("");
  const [userMetadataKey, setUserMetadataKey] = useState("");
  const [userMetadataValue, setUserMetadataValue] = useState("");
  const [batchMediaIds, setBatchMediaIds] =
    useState<ReadonlySet<string>>(new Set());
  const [batchNamePrefix, setBatchNamePrefix] = useState("");
  const [batchRelinkRoot, setBatchRelinkRoot] = useState("");
  const [batchMetadataKey, setBatchMetadataKey] = useState("");
  const [batchMetadataValue, setBatchMetadataValue] = useState("");
  const [batchPending, setBatchPending] = useState(false);
  const [batchResult, setBatchResult] = useState<string | null>(null);
  const [sourceScanPending, setSourceScanPending] = useState(false);
  const [thumbnailFailures, setThumbnailFailures] =
    useState<ReadonlySet<string>>(new Set());
  const [mediaPreview, setMediaPreview] = useState<MediaPreviewBundle | null>(null);
  const [mediaPreviewPending, setMediaPreviewPending] = useState(false);
  const [mediaPreviewFailure, setMediaPreviewFailure] = useState<string | null>(null);

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
    let unlisten: (() => void) | null = null;
    let latestProjectRevision = -1;
    const initialize = async () => {
      try {
        unlisten = await listenForDesktopProjectOpen((event) => {
          if (!active) {
            return;
          }
          if (event.snapshot !== null) {
            if (event.snapshot.revision < latestProjectRevision) {
              return;
            }
            latestProjectRevision = event.snapshot.revision;
            setProjectPath(event.path);
            setProjectSnapshot(event.snapshot);
            setProjectFailure(event.snapshot.failure ?? event.failure);
          } else if (event.failure !== null) {
            setProjectPath(event.path);
            setProjectFailure(event.failure);
          }
        });
        if (!active) {
          unlisten();
          unlisten = null;
          return;
        }
      } catch (error: unknown) {
        if (active) {
          setProjectFailure(projectFailureFrom(error));
        }
      }
      try {
        const project = await getDesktopProjectSnapshot();
        if (active && project.revision >= latestProjectRevision) {
          latestProjectRevision = project.revision;
          setProjectSnapshot(project);
          setProjectFailure(project.failure);
        }
      } catch (error: unknown) {
        if (active) {
          setProjectFailure(projectFailureFrom(error));
        }
      }
    };
    void initialize();
    return () => {
      active = false;
      unlisten?.();
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
    const query = mediaSearch.trim();
    if (mediaLibrary === null || query.length === 0) {
      setContentSearch(null);
      setContentSearchPending(false);
      return () => {
        active = false;
      };
    }
    setContentSearch(null);
    setContentSearchPending(true);
    const timer = window.setTimeout(() => {
      void searchProjectMediaContent(mediaLibrary, query)
        .then((result) => {
          if (
            active &&
            result.project_revision === mediaLibrary.project_revision &&
            result.library_revision === mediaLibrary.revision
          ) {
            setContentSearch(result);
            setProjectFailure(null);
          }
        })
        .catch((error: unknown) => {
          if (active) {
            setContentSearch(null);
            setProjectFailure(projectFailureFrom(error));
          }
        })
        .finally(() => {
          if (active) {
            setContentSearchPending(false);
          }
        });
    }, 120);
    return () => {
      active = false;
      window.clearTimeout(timer);
    };
  }, [mediaLibrary, mediaSearch]);

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

  const mutateMediaContentAnalysis = async (
    analysis: MediaContentAnalysis,
  ) => {
    if (mediaLibrary === null || selectedMedia === undefined) {
      return;
    }
    try {
      setMediaLibrary(
        await mutateProjectMediaContentAnalysis(
          mediaLibrary,
          selectedMedia,
          analysis,
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

  const scanMediaSources = async (
    mediaIds: readonly string[],
    verifyContent: boolean,
  ) => {
    if (mediaLibrary === null) return;
    setSourceScanPending(true);
    try {
      setMediaLibrary(
        await scanProjectMediaSources(mediaLibrary, mediaIds, verifyContent),
      );
      setProjectFailure(null);
    } catch (error: unknown) {
      setProjectFailure(projectFailureFrom(error));
    } finally {
      setSourceScanPending(false);
    }
  };

  const runMediaBatch = async (
    operations: readonly MediaBatchOperation[],
  ) => {
    if (mediaLibrary === null || operations.length === 0) return;
    setBatchPending(true);
    setBatchResult(null);
    try {
      const result = await mutateProjectMediaBatch(mediaLibrary, operations);
      setMediaLibrary(result.snapshot);
      setBatchResult(
        `${result.operation_count} operations committed for ${result.affected_media_ids.length} media at library revision ${result.snapshot.revision}.`,
      );
      setProjectFailure(null);
    } catch (error: unknown) {
      setProjectFailure(projectFailureFrom(error));
    } finally {
      setBatchPending(false);
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
  const scopedMedia =
    mediaLibrary?.items.filter((item) => {
      if (activeCollection) {
        return activeCollection.media_ids.includes(item.media_id);
      }
      return activeBinId === null || item.bin_id === activeBinId;
    }) ?? [];
  const normalizedMediaSearch = mediaSearch.trim().toLocaleLowerCase();
  const contentSearchReady =
    contentSearch?.query.toLocaleLowerCase() === normalizedMediaSearch;
  const contentSearchByMediaId = new Map(
    contentSearchReady
      ? contentSearch.results.map((result) => [result.media_id, result] as const)
      : [],
  );
  const visibleMedia =
    normalizedMediaSearch.length === 0
      ? scopedMedia
      : scopedMedia.filter((item) => contentSearchByMediaId.has(item.media_id));
  const selectedMedia = mediaLibrary?.items.find(
    (item) => item.media_id === selectedMediaId,
  );
  const batchSelectedMedia =
    mediaLibrary?.items.filter((item) => batchMediaIds.has(item.media_id)) ?? [];

  useEffect(() => {
    let active = true;
    if (mediaLibrary === null || selectedMedia === undefined) {
      setMediaPreview(null);
      setMediaPreviewFailure(null);
      setMediaPreviewPending(false);
      return () => {
        active = false;
      };
    }
    const requestedMediaId = selectedMedia.media_id;
    const requestedFreshness = selectedMedia.content_fingerprint;
    setMediaPreview(null);
    setMediaPreviewFailure(null);
    setMediaPreviewPending(true);
    setThumbnailFailures((current) => {
      const next = new Set(current);
      next.delete(requestedMediaId);
      return next;
    });
    void generateProjectMediaPreview(mediaLibrary, selectedMedia)
      .then((preview) => {
        if (
          active &&
          preview.media_id === requestedMediaId &&
          preview.freshness === requestedFreshness
        ) {
          setMediaPreview(preview);
        }
      })
      .catch((error: unknown) => {
        if (active) {
          setMediaPreviewFailure(projectFailureFrom(error).title);
        }
      })
      .finally(() => {
        if (active) {
          setMediaPreviewPending(false);
        }
      });
    return () => {
      active = false;
    };
  }, [
    mediaLibrary?.project_revision,
    mediaLibrary?.revision,
    selectedMedia?.media_id,
    selectedMedia?.content_fingerprint,
  ]);

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
                  className="secondary"
                  disabled={sourceScanPending || mediaLibrary.items.length === 0}
                  onClick={() => void scanMediaSources([], false)}
                >Check all sources</button>
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
              Search local media content
              <input
                type="search"
                value={mediaSearch}
                onChange={(event) => setMediaSearch(event.currentTarget.value)}
                placeholder="Search metadata, transcript, or local AI content"
              />
            </label>
            {normalizedMediaSearch.length > 0 ? (
              <p className="media-search-status" role="status">
                {contentSearchPending
                  ? "Searching the current local media revision"
                  : `${contentSearch?.results.length ?? 0} ranked result${
                      contentSearch?.results.length === 1 ? "" : "s"
                    } with explainable match evidence`}
              </p>
            ) : null}

            <section
              className="media-batch-operations"
              data-testid="media-batch-operations"
              aria-label="Batch media operations"
            >
              <header className="media-batch-header">
                <div>
                  <p className="eyebrow">Large project tools</p>
                  <h5>Batch operations</h5>
                </div>
                <strong>{batchSelectedMedia.length} selected</strong>
              </header>
              <label className="media-batch-selection">
                Media in the current view
                <select
                  multiple
                  value={[...batchMediaIds]}
                  size={Math.max(3, Math.min(visibleMedia.length, 7))}
                  onChange={(event) =>
                    setBatchMediaIds(
                      new Set(
                        Array.from(
                          event.currentTarget.selectedOptions,
                          (option) => option.value,
                        ),
                      ),
                    )
                  }
                >
                  {visibleMedia.map((item) => (
                    <option value={item.media_id} key={item.media_id}>
                      {item.name} ({item.offline.status})
                    </option>
                  ))}
                </select>
              </label>
              <div className="actions media-batch-selection-actions">
                <button
                  type="button"
                  className="secondary"
                  disabled={visibleMedia.length === 0 || batchPending}
                  onClick={() =>
                    setBatchMediaIds(
                      new Set(visibleMedia.map((item) => item.media_id)),
                    )
                  }
                >Select visible</button>
                <button
                  type="button"
                  className="secondary"
                  disabled={batchSelectedMedia.length === 0 || batchPending}
                  onClick={() => setBatchMediaIds(new Set())}
                >Clear selection</button>
              </div>
              <div className="media-batch-fields">
                <label>
                  Rename prefix
                  <input
                    value={batchNamePrefix}
                    maxLength={480}
                    onChange={(event) => setBatchNamePrefix(event.currentTarget.value)}
                    placeholder="Interview"
                  />
                </label>
                <button
                  type="button"
                  disabled={
                    batchPending ||
                    batchSelectedMedia.length === 0 ||
                    batchNamePrefix.trim().length === 0
                  }
                  onClick={() =>
                    void runMediaBatch(
                      batchSelectedMedia.map((item, index) => ({
                        kind: "rename",
                        media_id: item.media_id,
                        name: `${batchNamePrefix.trim()} ${String(index + 1).padStart(3, "0")}`,
                      })),
                    )
                  }
                >Batch rename</button>
                <button
                  type="button"
                  disabled={batchPending || batchSelectedMedia.length === 0}
                  onClick={() =>
                    void runMediaBatch(
                      batchSelectedMedia.map((item) => ({
                        kind: "organize",
                        media_id: item.media_id,
                        bin_id: activeBinId,
                      })),
                    )
                  }
                >Organize selected</button>
                <button
                  type="button"
                  disabled={batchPending || batchSelectedMedia.length === 0}
                  onClick={() =>
                    void runMediaBatch(
                      batchSelectedMedia.map((item) => ({
                        kind: "transcode",
                        media_id: item.media_id,
                        artifact_id: `optimized:${item.media_id}:${mediaLibrary.project_revision}:${mediaLibrary.revision + 1}`,
                        quality: "full",
                        status: "generating",
                        source_fingerprint: item.content_fingerprint,
                        source_revision: mediaLibrary.project_revision,
                        byte_len: 0,
                        select: true,
                      })),
                    )
                  }
                >Queue optimized transcode</button>
                <button
                  type="button"
                  disabled={batchPending || batchSelectedMedia.length === 0}
                  onClick={() =>
                    void runMediaBatch(
                      batchSelectedMedia.map((item) => ({
                        kind: "proxy",
                        media_id: item.media_id,
                        artifact_id: `proxy:${item.media_id}:${mediaLibrary.project_revision}:${mediaLibrary.revision + 1}`,
                        quality: "quarter",
                        status: "generating",
                        source_fingerprint: item.content_fingerprint,
                        source_revision: mediaLibrary.project_revision,
                        byte_len: 0,
                        select: true,
                      })),
                    )
                  }
                >Queue proxy</button>
                <label>
                  Relink root
                  <input
                    value={batchRelinkRoot}
                    onChange={(event) => setBatchRelinkRoot(event.currentTarget.value)}
                    placeholder="/Volumes/Project/Media"
                  />
                </label>
                <button
                  type="button"
                  disabled={
                    batchPending ||
                    batchSelectedMedia.length === 0 ||
                    batchRelinkRoot.trim().length === 0
                  }
                  onClick={() =>
                    void runMediaBatch(
                      batchSelectedMedia.map((item) => ({
                        kind: "relink",
                        media_id: item.media_id,
                        source_paths: item.source_paths.map((path) =>
                          sourcePathUnderRoot(batchRelinkRoot.trim(), path),
                        ),
                        candidate_fingerprint: item.content_fingerprint,
                      })),
                    )
                  }
                >Batch relink</button>
                <label>
                  Metadata key
                  <input
                    value={batchMetadataKey}
                    onChange={(event) => setBatchMetadataKey(event.currentTarget.value)}
                    placeholder="production.unit"
                  />
                </label>
                <label>
                  Metadata value
                  <input
                    value={batchMetadataValue}
                    onChange={(event) => setBatchMetadataValue(event.currentTarget.value)}
                  />
                </label>
                <button
                  type="button"
                  disabled={
                    batchPending ||
                    batchSelectedMedia.length === 0 ||
                    batchMetadataKey.trim().length === 0
                  }
                  onClick={() =>
                    void runMediaBatch(
                      batchSelectedMedia.map((item) => ({
                        kind: "metadata_upsert",
                        media_id: item.media_id,
                        key: batchMetadataKey.trim(),
                        value: batchMetadataValue,
                      })),
                    )
                  }
                >Update metadata</button>
                <button
                  type="button"
                  className="secondary"
                  disabled={
                    batchPending ||
                    batchSelectedMedia.length === 0 ||
                    batchMetadataKey.trim().length === 0
                  }
                  onClick={() =>
                    void runMediaBatch(
                      batchSelectedMedia.map((item) => ({
                        kind: "metadata_remove",
                        media_id: item.media_id,
                        key: batchMetadataKey.trim(),
                      })),
                    )
                  }
                >Remove metadata</button>
              </div>
              <p className="media-batch-note">
                Proxy and optimized work stays replaceable and uses the original source
                until matching ready artifact evidence is attached.
              </p>
              {batchResult ? <p role="status">{batchResult}</p> : null}
            </section>

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
                  const generatedThumbnail =
                    item.media_id === selectedMediaId &&
                    mediaPreview?.media_id === item.media_id &&
                    mediaPreview.freshness === item.content_fingerprint &&
                    mediaPreview.thumbnail.status === "ready" &&
                    !thumbnailFailures.has(item.media_id)
                      ? mediaPreview.thumbnail.artifact
                      : null;
                  const showSource =
                    generatedThumbnail === null &&
                    item.thumbnail.kind === "source" &&
                    !thumbnailFailures.has(item.media_id);
                  const searchResult = contentSearchByMediaId.get(item.media_id);
                  const strongestMatch = searchResult?.matches[0];
                  return (
                    <button
                      type="button"
                      className="media-item"
                      aria-pressed={selectedMediaId === item.media_id}
                      key={item.media_id}
                      onClick={() => setSelectedMediaId(item.media_id)}
                    >
                      <span className="media-thumbnail">
                        {generatedThumbnail !== null ? (
                          <img
                            alt=""
                            src={generatedThumbnail.data_url}
                            onError={() =>
                              setThumbnailFailures((current) =>
                                new Set(current).add(item.media_id),
                              )
                            }
                          />
                        ) : showSource && item.thumbnail.kind === "source" ? (
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
                        <small>
                          source {item.source_monitoring.status}
                          {item.source_monitoring.relink_intent === "none"
                            ? ""
                            : ` / ${item.source_monitoring.relink_intent.replaceAll("_", " ")}`}
                        </small>
                        {strongestMatch ? (
                          <span className="media-search-evidence">
                            <b>{strongestMatch.signal.replace("_", " ")}</b>
                            {strongestMatch.evidence}
                            {strongestMatch.signal !== "metadata" &&
                            searchResult &&
                            !searchResult.analysis_fresh ? (
                              <em>Retained analysis from a replaced source</em>
                            ) : null}
                            {strongestMatch.start_frame !== null &&
                            strongestMatch.end_frame !== null ? (
                              <em>
                                Frames {strongestMatch.start_frame} to{" "}
                                {strongestMatch.end_frame}
                                {strongestMatch.rate_numerator !== null &&
                                strongestMatch.rate_denominator !== null
                                  ? ` at ${strongestMatch.rate_numerator}/${strongestMatch.rate_denominator}`
                                  : ""}
                                {strongestMatch.timeline_relationships.length > 0
                                  ? ` in ${strongestMatch.timeline_relationships
                                      .map((relationship) =>
                                        relationship.clip_id
                                          ? `${relationship.timeline_id}/${relationship.clip_id}`
                                          : relationship.timeline_id,
                                      )
                                      .join(", ")}`
                                  : ""}
                              </em>
                            ) : null}
                          </span>
                        ) : null}
                      </span>
                    </button>
                  );
                })}
                {visibleMedia.length === 0 && !contentSearchPending ? (
                  <p className="explanation">No media in this view.</p>
                ) : null}
              </div>

              <aside className="media-metadata">
                <h5>Metadata</h5>
                {selectedMedia ? (
                  <>
                    <strong>{selectedMedia.name}</strong>
                    <section
                      data-testid="source-monitoring"
                      aria-label="Source monitoring"
                    >
                      <h5>Source monitoring</h5>
                      <p>
                        {selectedMedia.source_monitoring.status} / {selectedMedia.source_monitoring.relink_intent.replaceAll("_", " ")} / scan {selectedMedia.source_monitoring.scan_generation}
                      </p>
                      <button
                        type="button"
                        disabled={sourceScanPending}
                        onClick={() =>
                          void scanMediaSources([selectedMedia.media_id], true)
                        }
                      >Verify selected source bytes</button>
                      {selectedMedia.source_monitoring.paths.map((sourcePath) => (
                        <article key={sourcePath.path}>
                          <strong>{sourcePath.status.replaceAll("_", " ")}</strong>
                          <p>{sourcePath.path}</p>
                          <dl>
                            <div>
                              <dt>Volume</dt>
                              <dd>{sourcePath.volume.volume_id}</dd>
                            </div>
                            <div>
                              <dt>Volume kind</dt>
                              <dd>{sourcePath.volume.kind}</dd>
                            </div>
                            <div>
                              <dt>Volume status</dt>
                              <dd>{sourcePath.volume.status}</dd>
                            </div>
                            <div>
                              <dt>Accepted bytes</dt>
                              <dd>
                                {sourcePath.baseline?.content_fingerprint ?? "not established"}
                              </dd>
                            </div>
                            <div>
                              <dt>Observed bytes</dt>
                              <dd>
                                {sourcePath.observed?.content_fingerprint ?? "not available"}
                              </dd>
                            </div>
                            <div>
                              <dt>Observed size</dt>
                              <dd>{sourcePath.observed?.size_bytes ?? "not available"}</dd>
                            </div>
                          </dl>
                          {sourcePath.detail ? <p>{sourcePath.detail}</p> : null}
                        </article>
                      ))}
                    </section>
                    <section className="media-preview" aria-label="Generated media preview">
                      <h5>Preview</h5>
                      {mediaPreviewPending ? (
                        <p className="source-metadata-status">Generating preview</p>
                      ) : null}
                      {mediaPreviewFailure ? (
                        <p className="media-preview-unavailable">{mediaPreviewFailure}</p>
                      ) : null}
                      {mediaPreview?.media_id === selectedMedia.media_id &&
                      mediaPreview.freshness === selectedMedia.content_fingerprint ? (
                        <>
                          {mediaPreview.preview.status === "ready" ? (
                            <img
                              className="media-preview-image"
                              src={mediaPreview.preview.artifact.data_url}
                              width={mediaPreview.preview.artifact.width}
                              height={mediaPreview.preview.artifact.height}
                              alt={`Generated preview for ${selectedMedia.name}`}
                            />
                          ) : (
                            <p className="media-preview-unavailable">
                              {mediaPreview.preview.reason}
                            </p>
                          )}

                          <p className="media-preview-status">
                            Thumbnail {mediaPreview.thumbnail.status}
                          </p>

                          {mediaPreview.filmstrip.status === "ready" ? (
                            <div
                              className="media-preview-filmstrip"
                              aria-label={`${mediaPreview.filmstrip.artifact.frames.length} representative frames`}
                            >
                              {mediaPreview.filmstrip.artifact.frames.map((frame, index) => (
                                <img
                                  key={`${frame.source_index ?? index}:${frame.data_url.length}`}
                                  src={frame.data_url}
                                  width={frame.width}
                                  height={frame.height}
                                  alt={`Representative frame ${
                                    frame.source_index === null ? index + 1 : frame.source_index + 1
                                  } of ${frame.source_count}`}
                                />
                              ))}
                            </div>
                          ) : (
                            <p className="media-preview-unavailable">
                              {mediaPreview.filmstrip.reason}
                            </p>
                          )}

                          {mediaPreview.waveform.status === "ready" ? (
                            <div className="media-preview-waveform">
                              <img
                                src={mediaPreview.waveform.artifact.image.data_url}
                                width={mediaPreview.waveform.artifact.image.width}
                                height={mediaPreview.waveform.artifact.image.height}
                                alt={`Channel-separated waveform for ${selectedMedia.name}`}
                              />
                              <dl>
                                <div>
                                  <dt>Sample range</dt>
                                  <dd>
                                    {mediaPreview.waveform.artifact.start_sample} to {mediaPreview.waveform.artifact.start_sample + mediaPreview.waveform.artifact.frame_count}
                                  </dd>
                                </div>
                                <div>
                                  <dt>Sample rate</dt>
                                  <dd>{mediaPreview.waveform.artifact.sample_rate} Hz</dd>
                                </div>
                                <div>
                                  <dt>Frames</dt>
                                  <dd>{mediaPreview.waveform.artifact.frame_count}</dd>
                                </div>
                                <div>
                                  <dt>Channels</dt>
                                  <dd>{mediaPreview.waveform.artifact.channel_layout.join(", ")}</dd>
                                </div>
                              </dl>
                            </div>
                          ) : (
                            <p className="media-preview-unavailable">
                              {mediaPreview.waveform.reason}
                            </p>
                          )}
                        </>
                      ) : null}
                    </section>
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
                    <MediaContentAnalysisEditor
                      key={`${selectedMedia.media_id}:content:${mediaLibrary.revision}`}
                      item={selectedMedia}
                      onSave={mutateMediaContentAnalysis}
                    />
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

interface MediaContentAnalysisEditorProps {
  readonly item: MediaBrowserItem;
  readonly onSave: (analysis: MediaContentAnalysis) => Promise<void>;
}

function MediaContentAnalysisEditor({
  item,
  onSave,
}: MediaContentAnalysisEditorProps) {
  const analysis = item.content_analysis;
  const hasAnalysis = analysis.source_fingerprint.length > 0;
  const analysisFresh =
    hasAnalysis && analysis.source_fingerprint === item.content_fingerprint;
  const editable = !hasAnalysis || analysisFresh;
  const save = (replacement: MediaContentAnalysis) => {
    const hasArtifacts =
      replacement.provenance !== null ||
      replacement.transcript_segments.length > 0 ||
      replacement.local_ai_content.length > 0;
    void onSave({
      ...replacement,
      source_fingerprint: hasArtifacts
        ? replacement.source_fingerprint || item.content_fingerprint
        : "",
    });
  };

  return (
    <section
      className="media-content-analysis"
      aria-label="Editable language analysis"
    >
      <header>
        <div>
          <h5>Editable language analysis</h5>
          <p>
            Analysis is ordinary project state and stays searchable without the
            model.
          </p>
        </div>
        <strong className={analysisFresh ? "content-fresh" : "content-stale"}>
          {!hasAnalysis ? "not analyzed" : analysisFresh ? "current" : "stale source"}
        </strong>
      </header>
      {hasAnalysis && analysisFresh ? (
        <button
          type="button"
          className="secondary"
          onClick={() =>
            void onSave({
              source_fingerprint: "",
              provenance: null,
              transcript_segments: [],
              local_ai_content: [],
            })
          }
        >
          Clear content analysis
        </button>
      ) : null}
      {hasAnalysis && !analysisFresh ? (
        <div className="content-analysis-warning">
          <p>
            Retained from {analysis.source_fingerprint}. Review it before binding
            it to {item.content_fingerprint}.
          </p>
          <button
            type="button"
            onClick={() =>
              void onSave({
                ...analysis,
                source_fingerprint: item.content_fingerprint,
              })
            }
          >
            Confirm analysis for current source
          </button>
        </div>
      ) : null}

      <fieldset disabled={!editable}>
        <form
          className="content-analysis-provenance"
          onSubmit={(event) => {
            event.preventDefault();
            const fields = new FormData(event.currentTarget);
            save({
              ...analysis,
              provenance: optionalFormText(fields, "analysis_provenance"),
            });
          }}
        >
          <label>
            Analysis provenance
            <input
              name="analysis_provenance"
              defaultValue={analysis.provenance ?? ""}
              maxLength={512}
              placeholder="Manual edit or audited local tool"
            />
          </label>
          <button type="submit">Save provenance</button>
        </form>

        <div className="content-analysis-group">
          <div className="content-analysis-heading">
            <h5>Transcript segments</h5>
            <span>{analysis.transcript_segments.length}</span>
          </div>
          {analysis.transcript_segments.map((segment) => (
            <form
              className="content-artifact-card"
              key={segment.segment_id}
              onSubmit={(event) => {
                event.preventDefault();
                const replacement = transcriptSegmentFromForm(
                  new FormData(event.currentTarget),
                  segment.segment_id,
                  item,
                );
                save({
                  ...analysis,
                  transcript_segments: analysis.transcript_segments.map(
                    (candidate) =>
                      candidate.segment_id === segment.segment_id
                        ? replacement
                        : candidate,
                  ),
                });
              }}
            >
              <label>
                Segment ID
                <input value={segment.segment_id} readOnly />
              </label>
              <label>
                Editable text
                <textarea
                  name="segment_text"
                  defaultValue={segment.text}
                  maxLength={16_384}
                  required
                />
              </label>
              <div className="content-timing-grid">
                <label>
                  Start frame
                  <input
                    name="segment_start"
                    type="number"
                    defaultValue={segment.start_frame}
                    required
                  />
                </label>
                <label>
                  End frame
                  <input
                    name="segment_end"
                    type="number"
                    defaultValue={segment.end_frame}
                    required
                  />
                </label>
                <label>
                  Rate numerator
                  <input
                    name="segment_rate_numerator"
                    type="number"
                    min={1}
                    defaultValue={segment.rate_numerator}
                    required
                  />
                </label>
                <label>
                  Rate denominator
                  <input
                    name="segment_rate_denominator"
                    type="number"
                    min={1}
                    defaultValue={segment.rate_denominator}
                    required
                  />
                </label>
              </div>
              <label>
                Speaker
                <input
                  name="segment_speaker"
                  defaultValue={segment.speaker ?? ""}
                  maxLength={256}
                />
              </label>
              <label>
                Timeline relationships, one timeline_id | clip_id per line
                <textarea
                  name="segment_relationships"
                  defaultValue={timelineRelationshipsText(
                    segment.timeline_relationships,
                  )}
                />
              </label>
              <div className="actions">
                <button type="submit">Save transcript segment</button>
                <button
                  type="button"
                  className="secondary"
                  onClick={() =>
                    save({
                      ...analysis,
                      transcript_segments: analysis.transcript_segments.filter(
                        (candidate) => candidate.segment_id !== segment.segment_id,
                      ),
                      local_ai_content: analysis.local_ai_content.map((content) => ({
                        ...content,
                        segment_ids: content.segment_ids.filter(
                          (segmentId) => segmentId !== segment.segment_id,
                        ),
                      })),
                    })
                  }
                >
                  Remove segment
                </button>
              </div>
            </form>
          ))}
          <form
            className="content-artifact-card content-artifact-new"
            onSubmit={(event) => {
              event.preventDefault();
              const fields = new FormData(event.currentTarget);
              const segmentId =
                optionalFormText(fields, "segment_id") ?? crypto.randomUUID();
              save({
                ...analysis,
                transcript_segments: [
                  ...analysis.transcript_segments,
                  transcriptSegmentFromForm(fields, segmentId, item),
                ],
              });
            }}
          >
            <strong>Add transcript segment</strong>
            <label>
              Segment ID, optional
              <input name="segment_id" maxLength={256} />
            </label>
            <label>
              Editable text
              <textarea name="segment_text" maxLength={16_384} required />
            </label>
            <div className="content-timing-grid">
              <label>
                Start frame
                <input
                  name="segment_start"
                  type="number"
                  defaultValue={item.first_frame ?? 0}
                  required
                />
              </label>
              <label>
                End frame
                <input
                  name="segment_end"
                  type="number"
                  defaultValue={(item.first_frame ?? 0) + 1}
                  required
                />
              </label>
              <label>
                Rate numerator
                <input
                  name="segment_rate_numerator"
                  type="number"
                  min={1}
                  defaultValue={item.frame_rate_numerator ?? 24}
                  required
                />
              </label>
              <label>
                Rate denominator
                <input
                  name="segment_rate_denominator"
                  type="number"
                  min={1}
                  defaultValue={item.frame_rate_denominator ?? 1}
                  required
                />
              </label>
            </div>
            <label>
              Speaker
              <input name="segment_speaker" maxLength={256} />
            </label>
            <label>
              Timeline relationships, one timeline_id | clip_id per line
              <textarea name="segment_relationships" />
            </label>
            <button type="submit">Add transcript segment</button>
          </form>
        </div>

        <div className="content-analysis-group">
          <div className="content-analysis-heading">
            <h5>Local AI content</h5>
            <span>{analysis.local_ai_content.length}</span>
          </div>
          {analysis.local_ai_content.map((content) => (
            <form
              className="content-artifact-card"
              key={content.content_id}
              onSubmit={(event) => {
                event.preventDefault();
                const replacement = localAiContentFromForm(
                  new FormData(event.currentTarget),
                  content.content_id,
                );
                save({
                  ...analysis,
                  local_ai_content: analysis.local_ai_content.map((candidate) =>
                    candidate.content_id === content.content_id
                      ? replacement
                      : candidate,
                  ),
                });
              }}
            >
              <label>
                Content ID
                <input value={content.content_id} readOnly />
              </label>
              <label>
                Editable label
                <input
                  name="content_label"
                  defaultValue={content.label}
                  maxLength={256}
                  required
                />
              </label>
              <label>
                Search terms, comma separated
                <input
                  name="content_terms"
                  defaultValue={content.terms.join(", ")}
                />
              </label>
              <label>
                Transcript segment IDs, comma separated
                <input
                  name="content_segment_ids"
                  defaultValue={content.segment_ids.join(", ")}
                />
              </label>
              <div className="actions">
                <button type="submit">Save local AI content</button>
                <button
                  type="button"
                  className="secondary"
                  onClick={() =>
                    save({
                      ...analysis,
                      local_ai_content: analysis.local_ai_content.filter(
                        (candidate) => candidate.content_id !== content.content_id,
                      ),
                    })
                  }
                >
                  Remove local content
                </button>
              </div>
            </form>
          ))}
          <form
            className="content-artifact-card content-artifact-new"
            onSubmit={(event) => {
              event.preventDefault();
              const fields = new FormData(event.currentTarget);
              const contentId =
                optionalFormText(fields, "content_id") ?? crypto.randomUUID();
              save({
                ...analysis,
                local_ai_content: [
                  ...analysis.local_ai_content,
                  localAiContentFromForm(fields, contentId),
                ],
              });
            }}
          >
            <strong>Add local AI content</strong>
            <label>
              Content ID, optional
              <input name="content_id" maxLength={256} />
            </label>
            <label>
              Editable label
              <input name="content_label" maxLength={256} required />
            </label>
            <label>
              Search terms, comma separated
              <input name="content_terms" />
            </label>
            <label>
              Transcript segment IDs, comma separated
              <input name="content_segment_ids" />
            </label>
            <button type="submit">Add local AI content</button>
          </form>
        </div>
      </fieldset>
    </section>
  );
}

function transcriptSegmentFromForm(
  fields: FormData,
  segmentId: string,
  item: MediaBrowserItem,
): MediaTranscriptSegment {
  return {
    segment_id: segmentId,
    text: String(fields.get("segment_text") ?? "").trim(),
    start_frame: Number(fields.get("segment_start")),
    end_frame: Number(fields.get("segment_end")),
    rate_numerator: Number(
      fields.get("segment_rate_numerator") ?? item.frame_rate_numerator ?? 24,
    ),
    rate_denominator: Number(
      fields.get("segment_rate_denominator") ?? item.frame_rate_denominator ?? 1,
    ),
    speaker: optionalFormText(fields, "segment_speaker"),
    timeline_relationships: timelineRelationshipsFromForm(
      fields,
      "segment_relationships",
    ),
  };
}

function localAiContentFromForm(
  fields: FormData,
  contentId: string,
): MediaLocalAiContent {
  return {
    content_id: contentId,
    label: String(fields.get("content_label") ?? "").trim(),
    terms: commaSeparatedTerms(fields, "content_terms"),
    segment_ids: commaSeparatedTerms(fields, "content_segment_ids"),
  };
}

function optionalFormText(fields: FormData, name: string): string | null {
  const value = String(fields.get(name) ?? "").trim();
  return value.length === 0 ? null : value;
}

function commaSeparatedTerms(fields: FormData, name: string): readonly string[] {
  return String(fields.get(name) ?? "")
    .split(",")
    .map((value) => value.trim())
    .filter((value) => value.length > 0);
}

function timelineRelationshipsFromForm(
  fields: FormData,
  name: string,
): readonly MediaTimelineRelationship[] {
  return String(fields.get(name) ?? "")
    .split("\n")
    .map((value) => value.trim())
    .filter((value) => value.length > 0)
    .map((value) => {
      const separator = value.indexOf("|");
      if (separator < 0) {
        return { timeline_id: value, clip_id: null };
      }
      const timelineId = value.slice(0, separator).trim();
      const clipId = value.slice(separator + 1).trim();
      return {
        timeline_id: timelineId,
        clip_id: clipId.length === 0 ? null : clipId,
      };
    });
}

function timelineRelationshipsText(
  relationships: readonly MediaTimelineRelationship[],
): string {
  return relationships
    .map((relationship) =>
      relationship.clip_id
        ? `${relationship.timeline_id} | ${relationship.clip_id}`
        : relationship.timeline_id,
    )
    .join("\n");
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
