import type {
  CSSProperties,
  DragEvent,
  KeyboardEvent,
  MouseEvent,
  PointerEvent,
} from "react";

import {
  APPLICATION_PANEL_DOCKS,
  APPLICATION_PANEL_DOCK_SIZE_BOUNDS,
  applicationRoutePanelLayout,
  type ApplicationPanelDockId,
  type ApplicationPanelDockLayout,
} from "./application.ts";
import { useApplication } from "./application-context.tsx";
import { useApplicationPresentation } from "./application-presentation.tsx";
import { APPLICATION_SEMANTIC_SURFACES } from "./accessibility-semantics.ts";
import {
  latestPointerSample,
  releasePointerCapture,
  restoreShellFocus,
} from "./shell-input.ts";

const PANEL_DRAG_TYPE = "application/x-superi-panel-id";
const DOCK_LABELS: Readonly<Record<ApplicationPanelDockId, string>> = {
  left: "Left",
  center: "Center",
  right: "Right",
  bottom: "Bottom",
};

type PanelWorkspaceStyle = CSSProperties & {
  readonly "--panel-left-size": string;
  readonly "--panel-left-separator": string;
  readonly "--panel-right-size": string;
  readonly "--panel-right-separator": string;
  readonly "--panel-bottom-size": string;
  readonly "--panel-bottom-separator": string;
};

export function PanelWorkspace() {
  const { dispatch, registry, state } = useApplication();
  const layout = applicationRoutePanelLayout(state);
  const visible = new Set(state.visiblePanelIds);
  const docks = new Map(layout.docks.map((dock) => [dock.dockId, dock]));
  const hasVisiblePanels = (dockId: ApplicationPanelDockId) =>
    docks.get(dockId)!.panelIds.some((panelId) => visible.has(panelId));
  const leftVisible = hasVisiblePanels("left");
  const rightVisible = hasVisiblePanels("right");
  const bottomVisible = hasVisiblePanels("bottom");
  const style: PanelWorkspaceStyle = {
    "--panel-left-size": leftVisible
      ? `${docks.get("left")!.sizeBasisPoints / 100}%`
      : "0px",
    "--panel-left-separator": leftVisible ? "5px" : "0px",
    "--panel-right-size": rightVisible
      ? `${docks.get("right")!.sizeBasisPoints / 100}%`
      : "0px",
    "--panel-right-separator": rightVisible ? "5px" : "0px",
    "--panel-bottom-size": bottomVisible
      ? `${docks.get("bottom")!.sizeBasisPoints / 100}%`
      : "0px",
    "--panel-bottom-separator": bottomVisible ? "5px" : "0px",
  };

  return (
    <div
      className="workspace-panel-layout"
      aria-describedby={
        APPLICATION_SEMANTIC_SURFACES.activeWorkflow.describedBy
      }
      aria-labelledby={APPLICATION_SEMANTIC_SURFACES.activeWorkflow.labelledBy}
      data-route-panel-layout={layout.routeId}
      data-keyboard-landmark="active-workflow"
      id="active-workflow"
      role={APPLICATION_SEMANTIC_SURFACES.activeWorkflow.role}
      style={style}
      tabIndex={-1}
    >
      {APPLICATION_PANEL_DOCKS.map((dockId) => (
        <PanelDock
          dock={docks.get(dockId)!}
          key={dockId}
          visiblePanelIds={state.visiblePanelIds}
        />
      ))}
      {leftVisible ? <PanelDockSeparator dockId="left" /> : null}
      {rightVisible ? <PanelDockSeparator dockId="right" /> : null}
      {bottomVisible ? <PanelDockSeparator dockId="bottom" /> : null}
      {state.visiblePanelIds.length === 0 ? (
        <div className="empty-route">
          <p>No panels are visible on this route.</p>
          <p>Use the panel controls above to restore one.</p>
        </div>
      ) : null}
      <p
        className="panel-layout-status"
        aria-atomic={APPLICATION_SEMANTIC_SURFACES.activeWorkflowStatus.atomic}
        aria-label={APPLICATION_SEMANTIC_SURFACES.activeWorkflowStatus.label}
        aria-live={APPLICATION_SEMANTIC_SURFACES.activeWorkflowStatus.live}
        id={APPLICATION_SEMANTIC_SURFACES.activeWorkflowStatus.id}
        role={APPLICATION_SEMANTIC_SURFACES.activeWorkflowStatus.role}
      >
        {state.focusedPanelId === null
          ? "No panel is focused"
          : `${registry.panel(state.focusedPanelId).title} is focused`}
      </p>
    </div>
  );
}

function PanelDock({
  dock,
  visiblePanelIds,
}: {
  readonly dock: ApplicationPanelDockLayout;
  readonly visiblePanelIds: readonly string[];
}) {
  const { dispatch, registry } = useApplication();
  const { openContextMenu, publishNotification } =
    useApplicationPresentation();
  const visible = new Set(visiblePanelIds);
  const panelIds = dock.panelIds.filter((panelId) => visible.has(panelId));
  if (panelIds.length === 0) return null;
  const activePanelId =
    dock.activePanelId !== null && panelIds.includes(dock.activePanelId)
      ? dock.activePanelId
      : panelIds[0];
  const activePanel = registry.panel(activePanelId);

  const dockPanel = (event: DragEvent, index: number) => {
    event.preventDefault();
    event.stopPropagation();
    const panelId = event.dataTransfer.getData(PANEL_DRAG_TYPE);
    if (!visiblePanelIds.includes(panelId)) return;
    dispatch({
      type: "dock_panel",
      panelId,
      dockId: dock.dockId,
      index,
    });
  };

  const hidePanel = (panelId: string, panelIndex: number) => {
    const panel = registry.panel(panelId);
    const remainingPanelIds = panelIds.filter(
      (candidatePanelId) => candidatePanelId !== panelId,
    );
    const nextPanelId =
      remainingPanelIds[
        Math.min(panelIndex, Math.max(remainingPanelIds.length - 1, 0))
      ] ?? null;
    dispatch({ type: "toggle_panel", panelId });
    publishNotification({
      id: `panel-hidden:${panelId}`,
      title: `${panel.title} hidden`,
      message: "The panel remains recoverable from the workspace header.",
      tone: "information",
    });
    if (nextPanelId !== null) {
      window.requestAnimationFrame(() => {
        restoreShellFocus(document.getElementById(panelTabId(nextPanelId)));
      });
    }
  };

  const showPanelContextMenu = (
    event: MouseEvent<HTMLButtonElement> | KeyboardEvent<HTMLButtonElement>,
    panelId: string,
    panelIndex: number,
  ) => {
    event.preventDefault();
    event.stopPropagation();
    const panel = registry.panel(panelId);
    const rectangle = event.currentTarget.getBoundingClientRect();
    const x = "clientX" in event && event.clientX > 0
      ? event.clientX
      : rectangle.left + 16;
    const y = "clientY" in event && event.clientY > 0
      ? event.clientY
      : rectangle.bottom + 4;
    openContextMenu({
      label: `${panel.title} panel actions`,
      x,
      y,
      returnFocus: event.currentTarget,
      items: [
        {
          id: "activate",
          label: "Activate panel",
          detail: "Move keyboard and workspace focus to this panel.",
          disabled: panelId === activePanelId,
          onSelect: () => {
            dispatch({ type: "activate_panel", panelId });
            publishNotification({
              id: `panel-activated:${panelId}`,
              title: `${panel.title} activated`,
              message: "Panel focus is included in workspace continuity.",
              tone: "information",
            });
          },
        },
        ...APPLICATION_PANEL_DOCKS.map((dockId) => ({
          id: `dock:${dockId}`,
          label: `Move to ${DOCK_LABELS[dockId].toLowerCase()} dock`,
          detail: `Keep the panel visible in the ${DOCK_LABELS[dockId].toLowerCase()} workspace region.`,
          disabled: dockId === dock.dockId,
          onSelect: () => {
            dispatch({
              type: "dock_panel",
              panelId,
              dockId,
              index: panelIndex,
            });
            publishNotification({
              id: `panel-docked:${panelId}`,
              title: `${panel.title} moved`,
              message: `The panel is now in the ${DOCK_LABELS[dockId].toLowerCase()} dock.`,
              tone: "success",
            });
          },
        })),
        {
          id: "hide",
          label: "Hide panel",
          detail: "Keep the panel available from the visible panel controls.",
          tone: "danger",
          onSelect: () => hidePanel(panelId, panelIndex),
        },
      ],
    });
  };

  return (
    <section
      className={`panel-dock panel-dock-${dock.dockId}`}
      data-panel-dock={dock.dockId}
      aria-label={`${DOCK_LABELS[dock.dockId]} panel dock`}
      onDragOver={(event) => {
        if (event.dataTransfer.types.includes(PANEL_DRAG_TYPE)) {
          event.preventDefault();
          event.dataTransfer.dropEffect = "move";
        }
      }}
      onDrop={(event) => dockPanel(event, panelIds.length)}
    >
      <header className="panel-dock-header">
        <div
          className="panel-tab-list"
          role="tablist"
          aria-label={`${DOCK_LABELS[dock.dockId]} dock panels`}
        >
          {panelIds.map((panelId, index) => {
            const panel = registry.panel(panelId);
            const selected = panelId === activePanelId;
            return (
              <span
                className="panel-tab-slot"
                key={panelId}
                role="presentation"
                onDragOver={(event) => {
                  if (event.dataTransfer.types.includes(PANEL_DRAG_TYPE)) {
                    event.preventDefault();
                    event.dataTransfer.dropEffect = "move";
                  }
                }}
                onDrop={(event) => dockPanel(event, index)}
              >
                <button
                  className="panel-tab"
                  type="button"
                  id={panelTabId(panelId)}
                  role="tab"
                  aria-controls={panelBodyId(panelId)}
                  aria-keyshortcuts="Shift+F10"
                  aria-selected={selected}
                  tabIndex={selected ? 0 : -1}
                  draggable
                  onClick={() =>
                    dispatch({ type: "activate_panel", panelId })
                  }
                  onContextMenu={(event) =>
                    showPanelContextMenu(event, panelId, index)
                  }
                  onDragStart={(event) => {
                    event.dataTransfer.effectAllowed = "move";
                    event.dataTransfer.setData(PANEL_DRAG_TYPE, panelId);
                    event.dataTransfer.setData("text/plain", panel.title);
                  }}
                  onKeyDown={(event) => {
                    if (
                      event.key === "ContextMenu" ||
                      (event.shiftKey && event.key === "F10")
                    ) {
                      showPanelContextMenu(event, panelId, index);
                      return;
                    }
                    handleTabKey(event, panelIds, index, dispatch);
                  }}
                >
                  {panel.title}
                </button>
                <button
                  className="panel-tab-hide"
                  type="button"
                  aria-label={`Hide ${panel.title}`}
                  onClick={() => hidePanel(panelId, index)}
                >
                  x
                </button>
              </span>
            );
          })}
        </div>
        <label className="panel-dock-selector">
          <span>Dock</span>
          <select
            aria-label={`Dock ${activePanel.title}`}
            value={dock.dockId}
            onChange={(event) => {
              const dockId = event.currentTarget.value as ApplicationPanelDockId;
              dispatch({
                type: "dock_panel",
                panelId: activePanelId,
                dockId,
              });
              publishNotification({
                id: `panel-docked:${activePanelId}`,
                title: `${activePanel.title} moved`,
                message: `The panel is now in the ${DOCK_LABELS[dockId].toLowerCase()} dock.`,
                tone: "success",
              });
            }}
          >
            {APPLICATION_PANEL_DOCKS.map((dockId) => (
              <option value={dockId} key={dockId}>
                {DOCK_LABELS[dockId]}
              </option>
            ))}
          </select>
        </label>
      </header>
      <div className="panel-dock-content">
        {panelIds.map((panelId) => {
          const panel = registry.panel(panelId);
          const Panel = panel.renderer;
          const selected = panelId === activePanelId;
          return (
            <section
              className={`workspace-panel panel-${panel.region}`}
              data-panel-id={panel.id}
              id={panelBodyId(panel.id)}
              key={panel.id}
              role="tabpanel"
              aria-labelledby={panelTabId(panel.id)}
              hidden={!selected}
              tabIndex={-1}
              onFocus={() =>
                dispatch({ type: "focus_panel", panelId: panel.id })
              }
            >
              <Panel />
            </section>
          );
        })}
      </div>
    </section>
  );
}

function PanelDockSeparator({
  dockId,
}: {
  readonly dockId: Exclude<ApplicationPanelDockId, "center">;
}) {
  const { dispatch, state } = useApplication();
  const dock = applicationRoutePanelLayout(state).docks.find(
    (candidate) => candidate.dockId === dockId,
  )!;
  const bounds = APPLICATION_PANEL_DOCK_SIZE_BOUNDS[dockId];
  const orientation = dockId === "bottom" ? "horizontal" : "vertical";
  const resizeFromPointer = (event: PointerEvent<HTMLDivElement>) => {
    const container = event.currentTarget.parentElement;
    if (container === null) return;
    const pointer = latestPointerSample(event.nativeEvent);
    const rectangle = container.getBoundingClientRect();
    const ratio =
      dockId === "left"
        ? (pointer.clientX - rectangle.left) / rectangle.width
        : dockId === "right"
          ? (rectangle.right - pointer.clientX) / rectangle.width
          : (rectangle.bottom - pointer.clientY) / rectangle.height;
    dispatch({
      type: "resize_panel_dock",
      dockId,
      sizeBasisPoints: Math.round(ratio * 10_000),
    });
  };

  return (
    <div
      className={`panel-dock-separator panel-dock-separator-${dockId}`}
      role="separator"
      aria-label={`Resize ${DOCK_LABELS[dockId]} panel dock`}
      aria-orientation={orientation}
      aria-valuemin={Math.round(bounds.minimum / 100)}
      aria-valuemax={Math.round(bounds.maximum / 100)}
      aria-valuenow={Math.round(dock.sizeBasisPoints / 100)}
      tabIndex={0}
      onPointerDown={(event) => {
        try {
          event.currentTarget.setPointerCapture(event.pointerId);
        } catch {
          return;
        }
        resizeFromPointer(event);
      }}
      onPointerMove={(event) => {
        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
          resizeFromPointer(event);
        }
      }}
      onPointerUp={(event) =>
        releasePointerCapture(event.currentTarget, event.pointerId)
      }
      onPointerCancel={(event) =>
        releasePointerCapture(event.currentTarget, event.pointerId)
      }
      onKeyDown={(event) => {
        const increment = 200;
        const delta =
          event.key === "ArrowRight" || event.key === "ArrowUp"
            ? increment
            : event.key === "ArrowLeft" || event.key === "ArrowDown"
              ? -increment
              : 0;
        if (delta === 0) return;
        event.preventDefault();
        dispatch({
          type: "resize_panel_dock",
          dockId,
          sizeBasisPoints: dock.sizeBasisPoints + delta,
        });
      }}
    />
  );
}

function handleTabKey(
  event: KeyboardEvent<HTMLButtonElement>,
  panelIds: readonly string[],
  index: number,
  dispatch: ReturnType<typeof useApplication>["dispatch"],
) {
  if (event.key === "Delete") {
    event.preventDefault();
    dispatch({ type: "toggle_panel", panelId: panelIds[index] });
    return;
  }
  const nextIndex =
    event.key === "ArrowRight"
      ? (index + 1) % panelIds.length
      : event.key === "ArrowLeft"
        ? (index - 1 + panelIds.length) % panelIds.length
        : event.key === "Home"
          ? 0
          : event.key === "End"
            ? panelIds.length - 1
            : null;
  if (nextIndex === null) return;
  event.preventDefault();
  const panelId = panelIds[nextIndex];
  dispatch({ type: "activate_panel", panelId });
  restoreShellFocus(document.getElementById(panelTabId(panelId)));
}

function panelTabId(panelId: string): string {
  return `panel-tab-${panelId.replace(/[^a-zA-Z0-9_-]/gu, "-")}`;
}

export function panelBodyId(panelId: string): string {
  return `panel-body-${panelId.replace(/[^a-zA-Z0-9_-]/gu, "-")}`;
}
