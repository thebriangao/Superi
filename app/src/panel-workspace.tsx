import type {
  CSSProperties,
  DragEvent,
  KeyboardEvent,
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
      data-route-panel-layout={layout.routeId}
      style={style}
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
      <p className="panel-layout-status" aria-live="polite">
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
                  aria-selected={selected}
                  tabIndex={selected ? 0 : -1}
                  draggable
                  onClick={() =>
                    dispatch({ type: "activate_panel", panelId })
                  }
                  onDragStart={(event) => {
                    event.dataTransfer.effectAllowed = "move";
                    event.dataTransfer.setData(PANEL_DRAG_TYPE, panelId);
                    event.dataTransfer.setData("text/plain", panel.title);
                  }}
                  onKeyDown={(event) =>
                    handleTabKey(event, panelIds, index, dispatch)
                  }
                >
                  {panel.title}
                </button>
                <button
                  className="panel-tab-hide"
                  type="button"
                  aria-label={`Hide ${panel.title}`}
                  title={`Hide ${panel.title}`}
                  onClick={() =>
                    dispatch({ type: "toggle_panel", panelId })
                  }
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
            onChange={(event) =>
              dispatch({
                type: "dock_panel",
                panelId: activePanelId,
                dockId: event.currentTarget.value as ApplicationPanelDockId,
              })
            }
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
    const rectangle = container.getBoundingClientRect();
    const ratio =
      dockId === "left"
        ? (event.clientX - rectangle.left) / rectangle.width
        : dockId === "right"
          ? (rectangle.right - event.clientX) / rectangle.width
          : (rectangle.bottom - event.clientY) / rectangle.height;
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
        event.currentTarget.setPointerCapture(event.pointerId);
        resizeFromPointer(event);
      }}
      onPointerMove={(event) => {
        if (event.currentTarget.hasPointerCapture(event.pointerId)) {
          resizeFromPointer(event);
        }
      }}
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
  document.getElementById(panelTabId(panelId))?.focus();
}

function panelTabId(panelId: string): string {
  return `panel-tab-${panelId.replace(/[^a-zA-Z0-9_-]/gu, "-")}`;
}

function panelBodyId(panelId: string): string {
  return `panel-body-${panelId.replace(/[^a-zA-Z0-9_-]/gu, "-")}`;
}
