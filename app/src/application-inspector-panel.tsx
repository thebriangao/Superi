import { useMemo, useState } from "react";
import { useApplication } from "./application-context.tsx";
import { createApplicationInspectorModel } from "./application-inspector.ts";
import { useApplicationPresentation } from "./application-presentation.tsx";
import "./application-inspector.css";

export function ApplicationInspectorPanel() {
  const {
    commandFailure,
    dispatch,
    editorProject,
    executeCommand,
    refreshEditorProject,
    registry,
    state,
  } = useApplication();
  const { notificationState, publishNotification } =
    useApplicationPresentation();
  const [pending, setPending] = useState(false);
  const [actionStatus, setActionStatus] = useState(
    "Inspector actions use existing application owners.",
  );
  const route = registry.route(state.activeRouteId);
  const model = useMemo(
    () =>
      createApplicationInspectorModel({
        routeTitle: route.title,
        focusedPanelTitle:
          state.focusedPanelId === null
            ? null
            : registry.panel(state.focusedPanelId).title,
        visiblePanelCount: state.visiblePanelIds.length,
        hiddenPanelCount: state.hiddenPanelIds.length,
        workspaceRevision: state.revision,
        selectionSummary: state.selection.items.map(
          (item) =>
            `${item.resource} / ${item.identity} / revision ${item.revision}`,
        ),
        editorProject,
        notificationState,
        commandFailure,
      }),
    [
      commandFailure,
      editorProject,
      notificationState,
      registry,
      route.title,
      state.focusedPanelId,
      state.hiddenPanelIds.length,
      state.revision,
      state.selection.items,
      state.visiblePanelIds.length,
    ],
  );

  const refresh = async () => {
    setPending(true);
    setActionStatus("Refreshing the public editor snapshot.");
    try {
      await refreshEditorProject();
      setActionStatus("Editor refresh request completed. Current state is shown above.");
      publishNotification({
        id: "application-inspector-refresh",
        title: "Inspector refresh requested",
        message: "The shared panel retained its last-valid view and shows the current editor status.",
        tone: "information",
      });
    } finally {
      setPending(false);
    }
  };

  const openSystem = async () => {
    setPending(true);
    setActionStatus("Opening authoritative System recovery and engine controls.");
    try {
      await executeCommand("application.route.system");
    } finally {
      setPending(false);
    }
  };

  const clearSelection = () => {
    if (state.selection.items.length === 0) return;
    dispatch({ type: "clear_selection" });
    setActionStatus("Shared selection cleared. The exact prior selection can be restored below.");
  };

  const restoreSelection = () => {
    if (state.selectionRestore === null || state.selection.items.length > 0) return;
    dispatch({ type: "restore_cleared_selection" });
    setActionStatus("The exact cleared selection and anchor were restored.");
  };

  return (
    <div className="panel-content application-inspector" data-testid="application-inspector">
      <header className="application-inspector__header">
        <div>
          <p className="eyebrow">Shared operational view</p>
          <h3>Inspector</h3>
        </div>
        <span
          className="application-inspector__engine"
          data-condition={model.engine.condition}
          role="status"
        >
          <strong>{model.engine.label}</strong>
          <small>{model.engine.detail}</small>
        </span>
      </header>

      <div className="application-inspector__actions" aria-label="Inspector actions">
        <article>
          <strong>Refresh editor state</strong>
          <p>Read the existing public snapshot without discarding the last-valid view.</p>
          <button type="button" disabled={pending} onClick={() => void refresh()}>
            Refresh
          </button>
        </article>
        <article>
          <strong>Review engine recovery</strong>
          <p>Open System, the authoritative lifecycle and diagnostic owner.</p>
          <button type="button" disabled={pending} onClick={() => void openSystem()}>
            Open System
          </button>
        </article>
        <article>
          <strong>Shared selection</strong>
          <p>Clear only transient selection intent; the exact selection remains restorable.</p>
          <div>
            <button
              type="button"
              disabled={pending || state.selection.items.length === 0}
              onClick={clearSelection}
            >
              Clear selection
            </button>
            <button
              className="secondary"
              type="button"
              disabled={
                pending ||
                state.selectionRestore === null ||
                state.selection.items.length > 0
              }
              onClick={restoreSelection}
            >
              Restore cleared selection
            </button>
          </div>
        </article>
      </div>

      <p className="application-inspector__action-status" role="status">
        {actionStatus}
      </p>

      <div className="application-inspector__groups">
        {model.groups.map((group) => (
          <section
            aria-labelledby={`application-inspector-${group.id}`}
            data-inspector-group={group.id}
            key={group.id}
          >
            <header>
              <h4 id={`application-inspector-${group.id}`}>{group.title}</h4>
              <p>{group.summary}</p>
            </header>
            <dl>
              {group.rows.map((row) => (
                <div data-tone={row.tone} key={row.id}>
                  <dt>{row.label}</dt>
                  <dd>{row.value}</dd>
                </div>
              ))}
            </dl>
          </section>
        ))}
      </div>
    </div>
  );
}
