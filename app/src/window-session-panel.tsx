import { isTauri } from "@tauri-apps/api/core";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useCallback, useEffect, useMemo, useState } from "react";

import { useApplication } from "./application-context.tsx";
import {
  closeDesktopWindow,
  createDesktopWindow,
  desktopWindowFailure,
  focusDesktopWindow,
  getDesktopWindowSession,
  listenDesktopWindowSession,
  moveDesktopWindowToMonitor,
  reopenDesktopWindow,
  setDesktopWindowFullscreen,
  undoDesktopWindowPlacement,
  type DesktopWindowSnapshot,
} from "./window-session.ts";

export function WindowSessionPanel() {
  const { state } = useApplication();
  const currentLabel = useMemo(
    () => (isTauri() ? getCurrentWebviewWindow().label : "main"),
    [],
  );
  const [snapshot, setSnapshot] = useState<DesktopWindowSnapshot | null>(null);
  const [pending, setPending] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  const accept = useCallback((next: DesktopWindowSnapshot) => {
    setSnapshot((current) =>
      current === null || next.revision >= current.revision ? next : current,
    );
    setFailure(null);
  }, []);

  useEffect(() => {
    if (!isTauri()) {
      setFailure("Native window controls are available in the desktop application.");
      return;
    }
    let active = true;
    let stop: (() => void) | null = null;
    void getDesktopWindowSession()
      .then((next) => {
        if (active) accept(next);
      })
      .catch((error: unknown) => {
        if (active) setFailure(desktopWindowFailure(error));
      });
    void listenDesktopWindowSession((next) => {
      if (active) accept(next);
    })
      .then((unlisten) => {
        if (active) stop = unlisten;
        else unlisten();
      })
      .catch((error: unknown) => {
        if (active) setFailure(desktopWindowFailure(error));
      });
    return () => {
      active = false;
      stop?.();
    };
  }, [accept]);

  const run = async (action: () => Promise<DesktopWindowSnapshot>) => {
    setPending(true);
    setFailure(null);
    try {
      accept(await action());
    } catch (error: unknown) {
      setFailure(desktopWindowFailure(error));
    } finally {
      setPending(false);
    }
  };

  const ready =
    snapshot?.phase === "ready" || snapshot?.phase === "recovered";
  const current = snapshot?.windows.find(
    (windowRecord) => windowRecord.label === currentLabel,
  );

  return (
    <section data-testid="window-session-panel" aria-labelledby="window-session-title">
      <header>
        <p className="eyebrow">Native shell</p>
        <h4 id="window-session-title">Window session</h4>
      </header>
      <p className="explanation">
        {snapshot
          ? `${snapshot.windows.length} active window${snapshot.windows.length === 1 ? "" : "s"}; ${snapshot.persistencePhase} persistence at revision ${snapshot.revision}.`
          : "Reading native window restoration state."}
      </p>
      <dl className="lifecycle-details">
        <div>
          <dt>Restoration</dt>
          <dd>{snapshot?.phase ?? "loading"}</dd>
        </div>
        <div>
          <dt>Current window</dt>
          <dd>{current?.label ?? currentLabel}</dd>
        </div>
        <div>
          <dt>Current workspace</dt>
          <dd>{current?.workspace ?? state.activeRouteId}</dd>
        </div>
        <div>
          <dt>Native viewer owner</dt>
          <dd>{snapshot?.nativeViewportOwner ?? "main"}</dd>
        </div>
      </dl>

      {snapshot?.recoveryNote ? (
        <p className="explanation" role="status">
          {snapshot.recoveryNote}
        </p>
      ) : null}
      {snapshot?.failure || failure ? (
        <div className="failure" role="alert">
          <p>{snapshot?.failure?.summary ?? failure}</p>
          {snapshot?.failure ? (
            <p className="failure-code">
              {snapshot.failure.category} / {snapshot.failure.recoverability}
            </p>
          ) : null}
        </div>
      ) : null}

      <div className="actions" aria-label="Window session actions">
        <button
          type="button"
          disabled={pending || !ready || (snapshot?.windows.length ?? 0) >= 8}
          onClick={() => void run(() => createDesktopWindow(state.activeRouteId))}
        >
          New workspace window
        </button>
        <button
          type="button"
          className="secondary"
          disabled={
            pending || !ready || (snapshot?.recentlyClosedCount ?? 0) === 0
          }
          onClick={() => void run(() => reopenDesktopWindow())}
        >
          Reopen closed window
        </button>
      </div>

      {snapshot?.windows.map((windowRecord) => (
        <article
          className="status-row"
          data-current={windowRecord.label === currentLabel}
          key={windowRecord.label}
        >
          <div>
            <p className="status-label">
              {windowRecord.title} {windowRecord.focused ? "(focused)" : ""}
            </p>
            <p className="status-value">
              {windowRecord.workspace} / {windowRecord.width}x{windowRecord.height}
              {windowRecord.fullscreen ? " / fullscreen" : ""}
            </p>
            <p className="explanation">
              {windowRecord.x},{windowRecord.y} on {windowRecord.monitorId ?? "reconciled monitor"}
            </p>
            <label>
              Move to monitor
              <select
                value={windowRecord.monitorId ?? ""}
                disabled={pending || !ready || (snapshot?.monitors.length ?? 0) === 0}
                onChange={(event) =>
                  void run(() =>
                    moveDesktopWindowToMonitor(
                      windowRecord.label,
                      event.currentTarget.value,
                    ),
                  )
                }
              >
                {windowRecord.monitorId ? null : <option value="">Select monitor</option>}
                {snapshot?.monitors.map((monitor) => (
                  <option value={monitor.id} key={monitor.id}>
                    {monitor.name} {monitor.physicalWidth}x{monitor.physicalHeight}
                    {monitor.primary ? " (primary)" : ""}
                  </option>
                ))}
              </select>
            </label>
          </div>
          <div className="actions" aria-label={`${windowRecord.title} actions`}>
            <button
              type="button"
              disabled={pending || !ready || windowRecord.focused}
              onClick={() =>
                void run(() => focusDesktopWindow(windowRecord.label))
              }
            >
              Focus
            </button>
            <button
              type="button"
              disabled={pending || !ready}
              onClick={() =>
                void run(() =>
                  setDesktopWindowFullscreen(
                    windowRecord.label,
                    !windowRecord.fullscreen,
                  ),
                )
              }
            >
              {windowRecord.fullscreen ? "Leave fullscreen" : "Enter fullscreen"}
            </button>
            <button
              type="button"
              className="secondary"
              disabled={pending || !ready || !windowRecord.canUndoPlacement}
              onClick={() =>
                void run(() => undoDesktopWindowPlacement(windowRecord.label))
              }
            >
              Undo placement
            </button>
            {windowRecord.canClose ? (
              <button
                type="button"
                className="secondary"
                disabled={pending || !ready}
                onClick={() =>
                  void run(() => closeDesktopWindow(windowRecord.label))
                }
              >
                Close window
              </button>
            ) : null}
          </div>
        </article>
      ))}
    </section>
  );
}
