import { invoke, isTauri } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";

import {
  loadProjectSourceMonitor,
  readProjectMediaLibrary,
  readSourceMonitorSnapshot,
  seekProjectSourceMonitor,
  unloadProjectSourceMonitor,
  updateProjectSourceMonitorMarks,
  type MediaLibrarySnapshot,
  type SourceMonitorMarkMutation,
  type SourceMonitorSnapshot,
  type SourceMonitorTime,
} from "./project-lifecycle.ts";
import type {
  TimelineAudioFeedback,
  TimelineAudioSeamFeedback,
  TimelineAudioTrackFeedback,
  TimelineViewerFeedback,
} from "./timeline-editorial-feedback.ts";

type ViewportSnapshot = {
  role: NativeViewerRole;
  phase: string;
  physicalWidth: number;
  physicalHeight: number;
  surfaceGeneration: number;
  frameSequence: number;
  displayIntent: string;
  summary: string | null;
};

export type NativeViewerRole = "source" | "program" | "composite" | "color";

export interface SourceMonitorProps {
  readonly projectRevision: number | null;
  readonly feedback?: TimelineViewerFeedback | null;
  readonly onSnapshotChange: (
    snapshot: SourceMonitorSnapshot | null,
  ) => void;
}

export function SourceMonitor({
  projectRevision,
  feedback = null,
  onSnapshotChange,
}: SourceMonitorProps) {
  const [library, setLibrary] = useState<MediaLibrarySnapshot | null>(null);
  const [monitor, setMonitor] = useState<SourceMonitorSnapshot | null>(null);
  const [selectedMediaId, setSelectedMediaId] = useState("");
  const [seekValue, setSeekValue] = useState("");
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!isTauri()) {
      setMonitor(null);
      setFailure("Source monitor controls are available in the desktop application.");
      return;
    }
    setBusy(true);
    setFailure(null);
    try {
      const [nextLibrary, nextMonitor] = await Promise.all([
        readProjectMediaLibrary(),
        readSourceMonitorSnapshot(),
      ]);
      setLibrary(nextLibrary);
      setMonitor(nextMonitor);
      setSelectedMediaId((current) =>
        nextLibrary.items.some((item) => item.media_id === current)
          ? current
          : nextMonitor.media_id ?? nextLibrary.items[0]?.media_id ?? "",
      );
      setSeekValue(nextMonitor.current?.value.toString() ?? "");
    } catch (error: unknown) {
      setFailure(sourceMonitorFailure(error));
    } finally {
      setBusy(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [projectRevision, refresh]);

  useEffect(() => {
    onSnapshotChange(monitor);
  }, [monitor, onSnapshotChange]);

  const selectedItem = library?.items.find(
    (item) => item.media_id === selectedMediaId,
  );
  const ready = monitor?.engine_state === "ready";

  const load = async () => {
    if (!library || !selectedItem) return;
    await runSourceMonitorAction(async () => {
      const next = await loadProjectSourceMonitor(library, selectedItem);
      setMonitor(next);
      setSeekValue(next.current?.value.toString() ?? "");
    }, setBusy, setFailure);
  };

  const seek = async () => {
    if (!library || !monitor?.current) return;
    const value = Number(seekValue);
    if (!Number.isSafeInteger(value)) {
      setFailure("Enter an exact integer source coordinate.");
      return;
    }
    const target: SourceMonitorTime = { ...monitor.current, value };
    await runSourceMonitorAction(async () => {
      const next = await seekProjectSourceMonitor(library, monitor, target);
      setMonitor(next);
      setSeekValue(next.current?.value.toString() ?? "");
    }, setBusy, setFailure);
  };

  const updateMark = async (mutation: SourceMonitorMarkMutation) => {
    if (!library || !monitor) return;
    await runSourceMonitorAction(async () => {
      const next = await updateProjectSourceMonitorMarks(
        library,
        monitor,
        mutation,
      );
      setLibrary(next.media_library);
      setMonitor(next.monitor);
    }, setBusy, setFailure);
  };

  const unload = async () => {
    if (!monitor) return;
    await runSourceMonitorAction(async () => {
      const next = await unloadProjectSourceMonitor(monitor);
      setMonitor(next);
      setSeekValue("");
    }, setBusy, setFailure);
  };

  return (
    <div className="source-monitor" data-engine-state={monitor?.engine_state ?? "empty"}>
      <NativeViewport role="source" label="Source" feedback={feedback} />
      <section className="source-monitor__controls" aria-label="Source monitor controls">
        <div className="source-monitor__heading">
          <div>
            <span className="source-monitor__eyebrow">Source session</span>
            <strong>{monitor?.engine_state ?? "empty"}</strong>
          </div>
          <button type="button" className="secondary" disabled={busy} onClick={() => void refresh()}>
            Refresh
          </button>
        </div>
        <label className="source-monitor__field">
          <span>Project media</span>
          <select
            value={selectedMediaId}
            disabled={busy || !library?.items.length}
            onChange={(event) => setSelectedMediaId(event.target.value)}
          >
            {library?.items.map((item) => (
              <option key={item.media_id} value={item.media_id}>
                {item.name}
              </option>
            ))}
          </select>
        </label>
        <div className="source-monitor__actions">
          <button type="button" disabled={busy || !selectedItem} onClick={() => void load()}>
            Load source
          </button>
          <button type="button" className="secondary" disabled={busy || !monitor || monitor.engine_state === "empty"} onClick={() => void unload()}>
            Unload
          </button>
        </div>
        <dl className="source-monitor__state">
          <SourceMonitorDetail label="Media" value={monitor?.media_name ?? "none"} />
          <SourceMonitorDetail label="Backend" value={monitor?.backend_id ?? "not loaded"} />
          <SourceMonitorDetail label="Container" value={monitor?.container_id ?? "not loaded"} />
          <SourceMonitorDetail
            label="Stream"
            value={monitor?.stream ? `${monitor.stream.kind} · ${monitor.stream.codec}` : "not loaded"}
          />
          <SourceMonitorDetail
            label="Coordinate"
            value={monitor?.current ? formatSourceMonitorTime(monitor.current) : "not loaded"}
          />
          <SourceMonitorDetail
            label="Duration"
            value={monitor?.duration ? formatSourceMonitorTime(monitor.duration) : "unknown"}
          />
          <SourceMonitorDetail
            label="Marks"
            value={monitor ? `${formatSourceMonitorTime(monitor.marks.in_mark)} / ${formatSourceMonitorTime(monitor.marks.out_mark)}` : "none"}
          />
          <SourceMonitorDetail
            label="Mark identity"
            value={monitor?.marks_fresh ? "current" : "unset or stale"}
          />
        </dl>
        <div className="source-monitor__seek">
          <label className="source-monitor__field">
            <span>Exact coordinate</span>
            <input
              type="number"
              step="1"
              value={seekValue}
              disabled={busy || !ready}
              onChange={(event) => setSeekValue(event.target.value)}
            />
          </label>
          <button type="button" disabled={busy || !ready || seekValue.length === 0} onClick={() => void seek()}>
            Seek
          </button>
        </div>
        <div className="source-monitor__marks">
          <button type="button" disabled={busy || !ready} onClick={() => void updateMark("set_in")}>
            Set in
          </button>
          <button type="button" disabled={busy || !ready} onClick={() => void updateMark("set_out")}>
            Set out
          </button>
          <button type="button" className="secondary" disabled={busy || !ready || !monitor?.marks.in_mark} onClick={() => void updateMark("clear_in")}>
            Clear in
          </button>
          <button type="button" className="secondary" disabled={busy || !ready || !monitor?.marks.out_mark} onClick={() => void updateMark("clear_out")}>
            Clear out
          </button>
        </div>
        <p className="source-monitor__note">
          {monitor?.presentation_note ?? "Source session state is separate from decode and native GPU viewer presentation."}
        </p>
        {failure ? <p className="source-monitor__failure" role="alert">{failure}</p> : null}
      </section>
    </div>
  );
}

async function runSourceMonitorAction(
  action: () => Promise<void>,
  setBusy: (busy: boolean) => void,
  setFailure: (failure: string | null) => void,
) {
  setBusy(true);
  setFailure(null);
  try {
    await action();
  } catch (error: unknown) {
    setFailure(sourceMonitorFailure(error));
  } finally {
    setBusy(false);
  }
}

function sourceMonitorFailure(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error !== null && "title" in error) {
    return String(error.title);
  }
  return String(error);
}

function formatSourceMonitorTime(time: SourceMonitorTime | null): string {
  if (!time) return "unset";
  return `${time.value} @ ${time.timebase_numerator}/${time.timebase_denominator}`;
}

function SourceMonitorDetail({ label, value }: { readonly label: string; readonly value: string }) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

export interface NativeViewportProps {
  readonly role: NativeViewerRole;
  readonly label: string;
  readonly feedback?: TimelineViewerFeedback | null;
}

export function NativeViewport({
  role,
  label,
  feedback = null,
}: NativeViewportProps) {
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
            role,
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
          role,
          x: Math.max(0, bounds.x),
          y: Math.max(0, bounds.y),
          width: 0,
          height: 0,
          scaleFactor: window.devicePixelRatio,
          visible: false,
        },
      });
    };
  }, [role]);

  const status = summary
    ? summary
    : snapshot
      ? `${label} · ${snapshot.displayIntent} · ${snapshot.phase} · ${snapshot.physicalWidth}×${snapshot.physicalHeight} · frame ${snapshot.frameSequence}`
      : "Starting native GPU output";

  return (
    <div className="native-viewport-shell">
      <section
        className="native-viewport"
        ref={host}
        aria-label={`${label} native GPU media viewer`}
        data-viewer-role={role}
      />
      <span className="native-viewport__status" role="status" aria-live="polite">
        {status}
      </span>
      {feedback ? <ViewerEditorialFeedback feedback={feedback} label={label} /> : null}
    </div>
  );
}

function ViewerEditorialFeedback({
  feedback,
  label,
}: {
  readonly feedback: TimelineViewerFeedback;
  readonly label: string;
}) {
  return (
    <section
      className="viewer-editorial-feedback"
      aria-label={`${label} editorial feedback`}
      data-has-multicam={feedback.multicam !== null}
      data-phase={feedback.phase}
    >
      <div className="viewer-editorial-feedback__summary">
        <div>
          <span>Editorial consequence</span>
          <strong>{feedback.title}</strong>
        </div>
        <div className="viewer-editorial-feedback__coordinate">
          <span>{feedback.phase}</span>
          <code>{feedback.coordinate ?? "exact coordinate unavailable"}</code>
        </div>
      </div>
      <p>{feedback.detail}</p>
      {feedback.multicam ? (
        <div className="viewer-multicam-feedback">
          <div className="viewer-multicam-feedback__header">
            <span>{feedback.multicam.syncMethod} sync</span>
            <span>{feedback.multicam.switchCount} switches</span>
            <span>{multicamAudioPolicy(feedback.multicam)}</span>
          </div>
          <div
            className="viewer-multicam-feedback__angles"
            aria-label="Multicam angles"
          >
            {feedback.multicam.angles.map((angle) => (
              <span
                key={angle.id}
                data-enabled={angle.enabled}
                title={`${angle.name}: ${angle.sourceClipIds.join(", ")}`}
              >
                <b>{angle.cameraLabel}</b>
                {angle.name}
              </span>
            ))}
          </div>
          <ol className="viewer-multicam-feedback__switches">
            {feedback.multicam.switches.map((multicamSwitch, index) => (
              <li key={`${multicamSwitch.angleId}:${index}`}>
                <code>{formatMulticamRange(multicamSwitch.sourceRange)}</code>
                <span>{multicamSwitch.angleId}</span>
              </li>
            ))}
          </ol>
        </div>
      ) : null}
    </section>
  );
}

export function EditorialAudioMeters({
  feedback,
}: {
  readonly feedback: TimelineAudioFeedback | null;
}) {
  if (!feedback) {
    return (
      <section
        className="editorial-audio-meters editorial-audio-meters--empty"
        aria-label="Editorial audio feedback meters"
        data-signal-status="unobserved"
      >
        <span>Audio feedback</span>
        <p>Select a timeline to inspect canonical routing and continuity.</p>
      </section>
    );
  }
  return (
    <section
      className="editorial-audio-meters"
      aria-label="Editorial audio feedback meters"
      data-signal-status={feedback.signalStatus}
    >
      <header>
        <div>
          <span>Audio feedback</span>
          <strong>Routing and continuity meters</strong>
        </div>
        <em>Signal level unobserved</em>
      </header>
      <p>{feedback.message}</p>
      {feedback.tracks.length === 0 ? (
        <div className="editorial-audio-meters__empty">No audio tracks in this timeline.</div>
      ) : (
        <div className="editorial-audio-meters__tracks">
          {feedback.tracks.map((track) => (
            <EditorialAudioTrackMeter key={track.trackId} track={track} />
          ))}
        </div>
      )}
    </section>
  );
}

function EditorialAudioTrackMeter({
  track,
}: {
  readonly track: TimelineAudioTrackFeedback;
}) {
  return (
    <article
      className="editorial-audio-track"
      data-audibility={track.audibility}
      data-signal-status={track.signalStatus}
    >
      <header>
        <div>
          <strong>{track.trackId}</strong>
          <span>{track.sampleRate} Hz sample clock</span>
        </div>
        <span>{track.audibility.replace("_", " ")}</span>
      </header>
      <div className="editorial-audio-track__routing">
        <span>Destination {track.destination}</span>
        <span>{track.clipCount} clips</span>
      </div>
      <div className="editorial-audio-track__channels">
        {track.routes.map((route, index) => (
          <div
            className="editorial-audio-route"
            data-route-state={route.state}
            key={`${route.source}:${index}`}
          >
            <code>{route.source}</code>
            <span className="editorial-audio-route__meter" aria-hidden="true">
              <i />
            </span>
            <code>
              {route.target ??
                (route.state === "unavailable" ? "unavailable" : "muted or unrouted")}
            </code>
            <b>{route.state.replace("_", " ")}</b>
          </div>
        ))}
      </div>
      <div className="editorial-audio-track__destination-channels">
        <span>Ordered destination channels</span>
        <code>{track.destinationChannels.join(" | ") || "none"}</code>
      </div>
      <AudioContinuityFeedback track={track} />
    </article>
  );
}

function AudioContinuityFeedback({
  track,
}: {
  readonly track: TimelineAudioTrackFeedback;
}) {
  if (track.continuity.status === "unsupported") {
    return (
      <p className="editorial-audio-continuity" data-continuity="unsupported">
        Continuity unsupported: {track.continuity.reason}
      </p>
    );
  }
  return (
    <div
      className="editorial-audio-continuity"
      data-continuity={
        track.continuity.uninterruptedRecordCoverage ? "continuous" : "attention"
      }
    >
      <span>
        {track.continuity.uninterruptedRecordCoverage
          ? "Uninterrupted record coverage"
          : "Record continuity needs attention"}
      </span>
      {track.continuity.seams.length > 0 ? (
        <ul>
          {track.continuity.seams.map((seam, index) => (
            <li key={`${seam.leftClipId}:${seam.rightClipId}:${index}`}>
              {formatAudioSeam(seam)}
            </li>
          ))}
        </ul>
      ) : null}
    </div>
  );
}

function formatAudioSeam(seam: TimelineAudioSeamFeedback): string {
  const record =
    seam.recordSampleCount === null
      ? seam.recordKind
      : `${seam.recordKind} ${seam.recordSampleCount} samples`;
  let source: string = seam.sourceKind;
  if (seam.sourceKind === "discontinuous") {
    source = `discontinuous ${seam.sourceExpected} to ${seam.sourceActual}`;
  } else if (seam.sourceKind === "different_clip") {
    source = `different clip ${seam.sourceLeft} to ${seam.sourceRight}`;
  }
  return `${seam.leftClipId} to ${seam.rightClipId}: ${record}, ${source}`;
}

function multicamAudioPolicy(
  multicam: NonNullable<TimelineViewerFeedback["multicam"]>,
): string {
  const policy = multicam.audioPolicyDetail;
  return policy.kind === "fixed"
    ? `fixed audio ${policy.angleId}`
    : `${policy.kind.replace("_", " ")} audio`;
}

function formatMulticamRange(
  range: NonNullable<TimelineViewerFeedback["multicam"]>["switches"][number]["sourceRange"],
): string {
  return `${range.start.value}+${range.duration.value} @ ${range.start.timebase.numerator}/${range.start.timebase.denominator}`;
}
