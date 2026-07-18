import { invoke, isTauri } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";

type ViewportSnapshot = {
  phase: string;
  physicalWidth: number;
  physicalHeight: number;
  surfaceGeneration: number;
  frameSequence: number;
  summary: string | null;
};

export function NativeViewport() {
  const host = useRef<HTMLElement>(null);
  const [snapshot, setSnapshot] = useState<ViewportSnapshot | null>(null);
  const [summary, setSummary] = useState<string | null>(null);

  useEffect(() => {
    const element = host.current;
    if (!element || !isTauri()) {
      setSummary("Native GPU output is available in the desktop application.");
      return;
    }

    let animationFrame = 0;
    let disposed = false;
    const publish = () => {
      cancelAnimationFrame(animationFrame);
      animationFrame = requestAnimationFrame(() => {
        const bounds = element.getBoundingClientRect();
        void invoke<ViewportSnapshot>("desktop_viewport_update", {
          placement: {
            x: bounds.x,
            y: bounds.y,
            width: bounds.width,
            height: bounds.height,
            scaleFactor: window.devicePixelRatio,
            visible:
              document.visibilityState === "visible" &&
              bounds.width > 0 &&
              bounds.height > 0,
          },
        })
          .then((next) => {
            if (!disposed) {
              setSnapshot(next);
              setSummary(null);
            }
          })
          .catch((error: unknown) => {
            if (!disposed) {
              setSummary(error instanceof Error ? error.message : String(error));
            }
          });
      });
    };

    const observer = new ResizeObserver(publish);
    observer.observe(element);
    window.addEventListener("resize", publish);
    document.addEventListener("visibilitychange", publish);
    publish();

    return () => {
      disposed = true;
      cancelAnimationFrame(animationFrame);
      observer.disconnect();
      window.removeEventListener("resize", publish);
      document.removeEventListener("visibilitychange", publish);
      const bounds = element.getBoundingClientRect();
      void invoke("desktop_viewport_update", {
        placement: {
          x: Math.max(0, bounds.x),
          y: Math.max(0, bounds.y),
          width: 0,
          height: 0,
          scaleFactor: window.devicePixelRatio,
          visible: false,
        },
      });
    };
  }, []);

  const status = summary
    ? summary
    : snapshot
      ? `${snapshot.phase} · ${snapshot.physicalWidth}×${snapshot.physicalHeight} · frame ${snapshot.frameSequence}`
      : "Starting native GPU output";

  return (
    <div className="native-viewport-shell">
      <section className="native-viewport" ref={host} aria-label="Native GPU media viewport" />
      <span className="native-viewport__status" role="status" aria-live="polite">
        {status}
      </span>
    </div>
  );
}
