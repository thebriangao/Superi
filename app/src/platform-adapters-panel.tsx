import { useCallback, useEffect, useState } from "react";

import {
  discoverPlatformAdapters,
  type PlatformAdapterSnapshot,
} from "./platform-adapters.ts";
import { desktopPlatformLabel } from "./platform-parity.ts";

export function PlatformAdaptersPanel() {
  const [snapshot, setSnapshot] = useState<PlatformAdapterSnapshot | null>(null);
  const [pending, setPending] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  const refresh = useCallback(async () => {
    setPending(true);
    try {
      setSnapshot(await discoverPlatformAdapters());
      setFailure(null);
    } catch {
      setFailure(
        "Native adapter declarations are unavailable. Restart Superi, then refresh adapters.",
      );
    } finally {
      setPending(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return (
    <section aria-labelledby="platform-adapters-title">
      <header>
        <p className="eyebrow">Current host</p>
        <h4 id="platform-adapters-title">Native adapter contracts</h4>
      </header>
      {snapshot === null ? (
        <p className={failure === null ? "explanation" : "failure"} role="status">
          {failure ?? "Reading native adapter declarations."}
        </p>
      ) : (
        <>
          <p className="explanation">
            {desktopPlatformLabel(snapshot.platform)} declares six native adapter
            families behind the same shared contracts. Availability remains in
            hardware capability discovery.
          </p>
          <dl className="lifecycle-details">
            {snapshot.adapters.map((adapter) => (
              <div key={adapter.domain}>
                <dt>{adapter.domain}</dt>
                <dd>
                  {adapter.implementation} / {adapter.contract_id}
                </dd>
              </div>
            ))}
          </dl>
          <p className="explanation">
            Media guarantees: {snapshot.media_guarantees.join(", ")}.
          </p>
        </>
      )}
      <div className="actions" aria-label="Platform adapter actions">
        <button
          className="secondary"
          disabled={pending}
          onClick={() => void refresh()}
          type="button"
        >
          {pending ? "Reading adapters" : "Refresh adapters"}
        </button>
      </div>
    </section>
  );
}
