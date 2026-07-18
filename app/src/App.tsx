import { useCallback, useEffect, useState } from "react";

import {
  getDesktopLifecycle,
  requestDesktopLifecycle,
  type ApplicationLifecycleRequest,
  type DesktopLifecycleSnapshot,
} from "./lifecycle";

interface ClientFailure {
  readonly summary: string;
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

export function App() {
  const [snapshot, setSnapshot] = useState<DesktopLifecycleSnapshot | null>(null);
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
  const phase = snapshot ? APPLICATION_LABELS[snapshot.application_phase] : "Connecting";

  return (
    <main className="shell" aria-labelledby="product-title">
      <section className="status-card" aria-live="polite">
        <header className="product-lockup">
          <p className="eyebrow">Desktop editor</p>
          <h1 id="product-title">Superi</h1>
        </header>

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
        </dl>

        {failure ? (
          <div className="failure" role="alert">
            <p>{failure.summary}</p>
            {snapshot?.failure ? (
              <p className="failure-code">
                {snapshot.failure.category} / {snapshot.failure.recoverability}
              </p>
            ) : null}
          </div>
        ) : (
          <p className="explanation">
            The native shell remains responsive while the headless engine completes lifecycle
            work outside the application thread.
          </p>
        )}

        <div className="actions" aria-label="Lifecycle actions">
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
      </section>
    </main>
  );
}
