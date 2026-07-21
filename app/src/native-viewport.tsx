import { invoke, isTauri } from "@tauri-apps/api/core";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { useApplication } from "./application-context.tsx";
import type { ProjectAction } from "./api.ts";
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
import {
  DEFAULT_VIEWER_ANALYSIS_VIEW,
  VIEWER_ANALYSIS_DEFINITIONS,
  type ViewerAnalysisView,
} from "./viewer-analysis.ts";
import {
  applyViewerNavigation,
  initialViewerNavigation,
  viewerTransform,
} from "./viewer-navigation.ts";
import {
  INITIAL_VIEWER_EXTERNAL_DISPLAY_SELECTION,
  formatViewerExternalDisplayOutput,
  reconcileViewerExternalDisplaySelection,
  selectViewerExternalDisplay,
  type ViewerExternalDisplayTarget,
  type ViewerExternalOutputSnapshot,
} from "./viewer-external-display.ts";
import {
  OVERLAY_DEFINITIONS,
  initialViewerOverlays,
  toggleViewerOverlay,
  visibleViewerOverlays,
  type ViewerOverlayDefinition,
} from "./viewer-overlays.ts";
import {
  VIEWER_COMPARISON_DEFINITIONS,
  applyViewerComparison,
  comparisonUsesPosition,
  createViewerFrameIdentity,
  formatViewerComparisonState,
  initialViewerComparison,
  viewerComparisonAvailable,
  type ViewerComparisonAction,
  type ViewerComparisonRole,
  type ViewerComparisonState,
  type ViewerFrameIdentity,
  type ViewerTemporalContext,
} from "./viewer-comparison.ts";
import {
  VIEWER_STATUS_FIELDS,
  projectViewerStatusDisplay,
} from "./viewer-status.ts";
import {
  VIEWER_TRANSFORM_IDENTITY_MATRIX,
  buildViewerTransformAction,
  projectViewerTransformControls,
  type ViewerTransformDraft,
  type ViewerTransformNodePresentation,
  type ViewerTransformProjection,
} from "./viewer-transform-controls.ts";

type ViewportSnapshot = {
  role: NativeViewerRole;
  selectedView: ViewerAnalysisView;
  presentedView: ViewerAnalysisView | null;
  phase: string;
  physicalWidth: number;
  physicalHeight: number;
  surfaceGeneration: number;
  frameSequence: number;
  displayIntent: string;
  summary: string | null;
  externalDisplays: readonly ViewerExternalDisplayTarget[];
  externalOutput: ViewerExternalOutputSnapshot;
};

export type NativeViewerRole = ViewerComparisonRole;

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
      <NativeViewport
        role="source"
        label="Source"
        feedback={feedback}
        temporalContext={sourceViewerTemporalContext(monitor?.current ?? null)}
      />
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

function sourceViewerTemporalContext(
  time: SourceMonitorTime | null,
): ViewerTemporalContext | null {
  return time === null
    ? null
    : {
        owner: "source",
        value: time.value,
        timebaseNumerator: time.timebase_numerator,
        timebaseDenominator: time.timebase_denominator,
      };
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
  readonly temporalContext?: ViewerTemporalContext | null;
  readonly onComparisonStateChange?: (summary: string) => void;
}

export function NativeViewport({
  role,
  label,
  feedback = null,
  temporalContext = null,
  onComparisonStateChange,
}: NativeViewportProps) {
  const { editorProject, sourceMonitor, state, executeProjectActions } =
    useApplication();
  const shell = useRef<HTMLDivElement>(null);
  const host = useRef<HTMLElement>(null);
  const [snapshot, setSnapshot] = useState<ViewportSnapshot | null>(null);
  const [summary, setSummary] = useState<string | null>(null);
  const [navigation, setNavigation] = useState(() => initialViewerNavigation(role));
  const [overlays, setOverlays] = useState(initialViewerOverlays);
  const [comparison, setComparison] = useState(initialViewerComparison);
  const [analysisView, setAnalysisView] = useState<ViewerAnalysisView>(
    DEFAULT_VIEWER_ANALYSIS_VIEW,
  );
  const [externalDisplayId, setExternalDisplayId] = useState<string | null>(
    INITIAL_VIEWER_EXTERNAL_DISPLAY_SELECTION.targetId,
  );
  const analysisViewRef = useRef(analysisView);
  const externalDisplayIdRef = useRef(externalDisplayId);
  const publishViewport = useRef<() => void>(() => {});
  analysisViewRef.current = analysisView;
  externalDisplayIdRef.current = externalDisplayId;

  useEffect(() => {
    setNavigation(initialViewerNavigation(role));
    setComparison(initialViewerComparison());
    setAnalysisView(DEFAULT_VIEWER_ANALYSIS_VIEW);
    setExternalDisplayId(INITIAL_VIEWER_EXTERNAL_DISPLAY_SELECTION.targetId);
  }, [role]);

  useEffect(() => {
    const synchronizeFullscreen = () => {
      if (document.fullscreenElement !== shell.current) {
        setNavigation((current) =>
          current.presentation === "fullscreen"
            ? applyViewerNavigation(current, { action: "presentation", mode: "normal" })
            : current,
        );
      }
    };
    document.addEventListener("fullscreenchange", synchronizeFullscreen);
    return () => document.removeEventListener("fullscreenchange", synchronizeFullscreen);
  }, []);

  useEffect(() => {
    const element = host.current;
    if (!element || !isTauri()) {
      publishViewport.current = () => {};
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
            view: analysisViewRef.current,
            x: bounds.x,
            y: bounds.y,
            width: bounds.width,
            height: bounds.height,
            scaleFactor: window.devicePixelRatio,
            visible:
              document.visibilityState === "visible" &&
              bounds.width > 0 &&
              bounds.height > 0,
            externalDisplayId: externalDisplayIdRef.current,
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
    publishViewport.current = publish;

    const observer = new ResizeObserver(publish);
    observer.observe(element);
    window.addEventListener("resize", publish);
    document.addEventListener("visibilitychange", publish);

    return () => {
      disposed = true;
      publishViewport.current = () => {};
      cancelAnimationFrame(animationFrame);
      observer.disconnect();
      window.removeEventListener("resize", publish);
      document.removeEventListener("visibilitychange", publish);
      externalDisplayIdRef.current = null;
      const bounds = element.getBoundingClientRect();
      void invoke("desktop_viewport_update", {
        placement: {
          role,
          view: analysisViewRef.current,
          x: Math.max(0, bounds.x),
          y: Math.max(0, bounds.y),
          width: 0,
          height: 0,
          scaleFactor: window.devicePixelRatio,
          visible: false,
          externalDisplayId: externalDisplayIdRef.current,
        },
      });
    };
  }, [role]);

  useEffect(() => {
    publishViewport.current();
  }, [analysisView, externalDisplayId, role]);

  useEffect(() => {
    if (!snapshot) return;
    setExternalDisplayId((current) =>
      reconcileViewerExternalDisplaySelection(
        { targetId: current },
        snapshot.externalDisplays,
      ).targetId,
    );
  }, [snapshot]);

  const status = summary
    ? summary
    : snapshot
      ? `${label} · ${snapshot.displayIntent} · selected ${snapshot.selectedView} · presented ${snapshot.presentedView ?? "none"} · ${snapshot.phase} · ${snapshot.physicalWidth}×${snapshot.physicalHeight} · frame ${snapshot.frameSequence}`
      : "Starting native GPU output";
  const externalStatus = snapshot
    ? formatViewerExternalDisplayOutput(snapshot.externalOutput)
    : "External display unavailable; native output has not started.";
  const currentFrame = createViewerFrameIdentity(role, snapshot, temporalContext);
  const comparisonSummary = formatViewerComparisonState(comparison, currentFrame);
  const statusDisplay = useMemo(
    () =>
      projectViewerStatusDisplay(
        role,
        editorProject.snapshot,
        sourceMonitor,
        comparisonSummary,
      ),
    [comparisonSummary, editorProject.snapshot, role, sourceMonitor],
  );
  const transformControls = useMemo(
    () =>
      role === "program"
        ? projectViewerTransformControls(
            editorProject.snapshot,
            state.selection,
          )
        : null,
    [editorProject.snapshot, role, state.selection],
  );
  const applyViewerTransform = useCallback(
    async (action: ProjectAction) => {
      const result = await executeProjectActions([action]);
      return result.state.project_revision;
    },
    [executeProjectActions],
  );
  const transform = viewerTransform(navigation);
  const updateNavigation = (action: Parameters<typeof applyViewerNavigation>[1]) => {
    setNavigation((current) => applyViewerNavigation(current, action));
  };
  const updateComparison = (action: ViewerComparisonAction) => {
    setComparison((current) =>
      applyViewerComparison(current, action, currentFrame),
    );
  };

  useEffect(() => {
    onComparisonStateChange?.(comparisonSummary);
  }, [comparisonSummary, onComparisonStateChange]);

  const toggleFullscreen = async () => {
    try {
      if (document.fullscreenElement === shell.current) {
        await document.exitFullscreen();
      } else if (shell.current) {
        await shell.current.requestFullscreen();
        updateNavigation({ action: "presentation", mode: "fullscreen" });
      }
    } catch (error: unknown) {
      setSummary(error instanceof Error ? error.message : String(error));
    }
  };

  const selectExternalDisplay = (targetId: string | null) => {
    try {
      setExternalDisplayId((current) =>
        selectViewerExternalDisplay(
          { targetId: current },
          targetId,
          snapshot?.externalDisplays ?? [],
        ).targetId,
      );
    } catch (error: unknown) {
      setSummary(error instanceof Error ? error.message : String(error));
    }
  };

  return (
    <div
      className="native-viewport-shell"
      ref={shell}
      data-presentation={navigation.presentation}
      data-scale-mode={navigation.scaleMode}
      data-comparison-mode={comparison.mode}
      data-analysis-view={analysisView}
      data-external-display-phase={snapshot?.externalOutput.phase ?? "unavailable"}
    >
      <div className="native-viewport__toolbar" aria-label={`${label} viewer navigation`}>
        <button type="button" onClick={() => updateNavigation({ action: "fit" })}>Fit</button>
        <button type="button" onClick={() => updateNavigation({ action: "zoom", factor: 0.5 })}>-</button>
        <output aria-label="Viewer zoom">{Math.round(navigation.scale * 100)}%</output>
        <button type="button" onClick={() => updateNavigation({ action: "zoom", factor: 2 })}>+</button>
        <button type="button" onClick={() => updateNavigation({ action: "pixel" })}>1:1</button>
        <button type="button" onClick={() => updateNavigation({ action: "pan", deltaX: -32, deltaY: 0 })}>Left</button>
        <button type="button" onClick={() => updateNavigation({ action: "pan", deltaX: 32, deltaY: 0 })}>Right</button>
        <button type="button" onClick={() => updateNavigation({ action: "pan", deltaX: 0, deltaY: -32 })}>Up</button>
        <button type="button" onClick={() => updateNavigation({ action: "pan", deltaX: 0, deltaY: 32 })}>Down</button>
        <button
          type="button"
          aria-pressed={navigation.presentation === "cinema"}
          onClick={() => updateNavigation({
            action: "presentation",
            mode: navigation.presentation === "cinema" ? "normal" : "cinema",
          })}
        >Cinema</button>
        <button type="button" aria-pressed={navigation.presentation === "fullscreen"} onClick={() => void toggleFullscreen()}>Fullscreen</button>
        <label>
          <span>External</span>
          <select
            aria-label={`${label} external display`}
            value={externalDisplayId ?? ""}
            disabled={!isTauri() || (snapshot?.externalDisplays.length ?? 0) === 0}
            onChange={(event) =>
              selectExternalDisplay(event.target.value || null)
            }
          >
            <option value="">Inline only</option>
            {snapshot?.externalDisplays.map((target) => (
              <option value={target.id} key={target.id}>
                {target.name} {target.physicalWidth}x{target.physicalHeight} @ {target.scaleFactor}x
                {target.primary ? " primary" : ""}
              </option>
            ))}
          </select>
        </label>
      </div>
      <div
        className="native-viewport__comparison-toolbar"
        aria-label={`${label} viewer comparisons`}
      >
        {VIEWER_COMPARISON_DEFINITIONS.map((definition) => (
          <button
            type="button"
            key={definition.mode}
            aria-pressed={comparison.mode === definition.mode}
            disabled={
              !viewerComparisonAvailable(
                comparison,
                currentFrame,
                definition.mode,
              )
            }
            onClick={() =>
              updateComparison({ action: "mode", mode: definition.mode })
            }
          >
            {definition.label}
          </button>
        ))}
        <span className="native-viewport__comparison-captures">
          <button
            type="button"
            disabled={currentFrame.visual === null}
            onClick={() => updateComparison({ action: "capture_reference" })}
          >
            {comparison.reference === null ? "Capture reference" : "Update reference"}
          </button>
          <button
            type="button"
            disabled={currentFrame.visual === null}
            onClick={() => updateComparison({ action: "capture_snapshot" })}
          >
            {comparison.snapshot === null ? "Capture snapshot" : "Update snapshot"}
          </button>
        </span>
        {comparisonUsesPosition(comparison.mode) ? (
          <div className="native-viewport__comparison-position">
            <label className="native-viewport__comparison-slider">
              <span>{comparison.orientation} boundary</span>
              <input
                type="range"
                min="5"
                max="95"
                step="1"
                value={Math.round(comparison.position * 100)}
                aria-label={`${label} comparison boundary`}
                onChange={(event) =>
                  updateComparison({
                    action: "position",
                    position: Number(event.target.value) / 100,
                  })
                }
              />
            </label>
            <output>{Math.round(comparison.position * 100)}%</output>
            <button
              type="button"
              onClick={() =>
                updateComparison({
                  action: "orientation",
                  orientation:
                    comparison.orientation === "vertical"
                      ? "horizontal"
                      : "vertical",
                })
              }
            >
              {comparison.orientation === "vertical" ? "Use horizontal" : "Use vertical"}
            </button>
          </div>
        ) : null}
      </div>
      <div className="native-viewport__overlay-toolbar" aria-label={`${label} viewer analysis`}>
        {VIEWER_ANALYSIS_DEFINITIONS.map((definition) => (
          <button
            type="button"
            key={definition.view}
            title={definition.description}
            aria-pressed={analysisView === definition.view}
            onClick={() => setAnalysisView(definition.view)}
          >
            {definition.label}
          </button>
        ))}
      </div>
      <div className="native-viewport__overlay-toolbar" aria-label={`${label} viewer overlays`}>
        {OVERLAY_DEFINITIONS.map((overlay) => (
          <button
            type="button"
            key={overlay.kind}
            aria-pressed={overlays[overlay.kind]}
            onClick={() => setOverlays((current) => toggleViewerOverlay(current, overlay.kind))}
          >
            {overlay.label}
          </button>
        ))}
      </div>
      <div className="native-viewport__frame">
        <section
          className="native-viewport"
          ref={host}
          aria-label={`${label} native GPU media viewer`}
          data-viewer-role={role}
          data-external-display-intent={navigation.externalDisplayIntent}
          style={{ transform: transform.transform, imageRendering: transform.imageRendering }}
        />
        <div
          className="native-viewport__comparison"
          role="img"
          aria-label={comparisonSummary}
          data-comparison-mode={comparison.mode}
          data-comparison-available={viewerComparisonAvailable(
            comparison,
            currentFrame,
            comparison.mode,
          )}
          style={{ transform: transform.transform }}
        >
          <ViewerComparisonPresentation
            state={comparison}
            current={currentFrame}
          />
        </div>
        <div
          className="native-viewport__overlays"
          aria-label={`${label} active overlays`}
          style={{ transform: transform.transform }}
        >
          {visibleViewerOverlays(overlays).map((overlay) => (
            <ViewerOverlay key={overlay.kind} overlay={overlay} />
          ))}
        </div>
      </div>
      <span className="native-viewport__status" role="status" aria-live="polite">
        {status} · {externalStatus} · requested {analysisView} · {navigation.scaleMode} {Math.round(navigation.scale * 100)}% · pan {navigation.panX},{navigation.panY} · {navigation.presentation} · {navigation.externalDisplayIntent} · {comparisonSummary}
      </span>
      <dl
        className="editor-detail-list compact-details"
        aria-label={`${label} viewer status display`}
      >
        {VIEWER_STATUS_FIELDS.map((field) => (
          <div key={field.key}>
            <dt>{field.label}</dt>
            <dd>{statusDisplay[field.key]}</dd>
          </div>
        ))}
      </dl>
      {transformControls ? (
        <ViewerTransformControls
          projection={transformControls}
          onApply={applyViewerTransform}
        />
      ) : null}
      {feedback ? <ViewerEditorialFeedback feedback={feedback} label={label} /> : null}
    </div>
  );
}

function ViewerTransformControls({
  projection,
  onApply,
}: {
  readonly projection: ViewerTransformProjection;
  readonly onApply: (action: ProjectAction) => Promise<number>;
}) {
  if (projection.status === "unavailable") {
    return (
      <section
        className="viewer-transform-controls viewer-transform-controls--unavailable"
        aria-label="Program viewer transform controls"
        data-transform-state="unavailable"
      >
        <header>
          <div>
            <span>Authored visual state</span>
            <strong>Graph transforms</strong>
          </div>
          <em>Unavailable</em>
        </header>
        <p>{projection.reason}</p>
      </section>
    );
  }
  return (
    <section
      className="viewer-transform-controls"
      aria-label="Program viewer transform controls"
      data-transform-state="ready"
    >
      <header>
        <div>
          <span>Authored visual state</span>
          <strong>Graph transforms</strong>
        </div>
        <div className="viewer-transform-controls__identity">
          <code>{projection.clipId}</code>
          <code>{projection.graphId}</code>
          <span>
            project revision {projection.projectRevision}, graph revision{" "}
            {projection.graphRevision}, {projection.transforms.length}{" "}
            {projection.transforms.length === 1 ? "node" : "nodes"}
          </span>
        </div>
      </header>
      <div className="viewer-transform-controls__nodes">
        {projection.transforms.map((transform, index) => (
          <ViewerTransformNodeControl
            key={`${projection.graphId}:${transform.nodeId}`}
            index={index}
            transform={transform}
            onApply={onApply}
          />
        ))}
      </div>
    </section>
  );
}

function ViewerTransformNodeControl({
  index,
  transform,
  onApply,
}: {
  readonly index: number;
  readonly transform: ViewerTransformNodePresentation;
  readonly onApply: (action: ProjectAction) => Promise<number>;
}) {
  const canonicalMatrix = transform.matrix.map((parameter) => parameter.value);
  const canonicalKey = `${transform.graphRevision}:${transform.nodeId}:${canonicalMatrix.join(",")}:${transform.sampling.value}`;
  const [matrix, setMatrix] = useState(() =>
    canonicalMatrix.map((value) => value.toString()),
  );
  const [sampling, setSampling] = useState(transform.sampling.value);
  const [pending, setPending] = useState(false);
  const [result, setResult] = useState<string | null>(null);

  useEffect(() => {
    setMatrix(canonicalMatrix.map((value) => value.toString()));
    setSampling(transform.sampling.value);
  }, [canonicalKey]);

  const numericMatrix = matrix.map((value) =>
    value.trim().length === 0 ? Number.NaN : Number(value),
  );
  const matrixValid = numericMatrix.every((value) => Number.isFinite(value));
  const matrixChanged =
    matrixValid &&
    numericMatrix.some((value, matrixIndex) => value !== canonicalMatrix[matrixIndex]);
  const samplingChanged = sampling !== transform.sampling.value;
  const canApply =
    !pending &&
    matrixValid &&
    ((matrixChanged && !transform.matrixDriven) ||
      (samplingChanged && !transform.sampling.driven));
  const resetDraft: ViewerTransformDraft = {
    matrix: transform.matrixDriven
      ? canonicalMatrix
      : VIEWER_TRANSFORM_IDENTITY_MATRIX,
    sampling: transform.sampling.driven ? transform.sampling.value : "bilinear",
  };
  const canReset =
    !pending &&
    (resetDraft.matrix.some(
      (value, matrixIndex) => value !== canonicalMatrix[matrixIndex],
    ) || resetDraft.sampling !== transform.sampling.value);

  const publish = async (draft: ViewerTransformDraft, verb: string) => {
    setPending(true);
    setResult(`${verb} pending through the project owner.`);
    try {
      const action = buildViewerTransformAction(transform, draft);
      const revision = await onApply(action);
      setResult(`${verb} committed at project revision ${revision}.`);
    } catch (error: unknown) {
      setResult(
        error instanceof Error
          ? error.message
          : "The viewer transform edit could not be completed.",
      );
    } finally {
      setPending(false);
    }
  };

  return (
    <article
      className="viewer-transform-node"
      data-matrix-driven={transform.matrixDriven}
      data-sampling-driven={transform.sampling.driven}
    >
      <div className="viewer-transform-node__heading">
        <div>
          <span>Transform {index + 1}</span>
          <code>{transform.nodeId}</code>
        </div>
        <span>schema {transform.schemaVersion}</span>
      </div>
      <fieldset disabled={pending || transform.matrixDriven}>
        <legend>
          Exact 3 by 3 matrix
          {transform.matrixDriven ? " (driver owned, inspect only)" : ""}
        </legend>
        <div className="viewer-transform-node__matrix">
          {transform.matrix.map((parameter, matrixIndex) => (
            <label key={parameter.parameterId} data-driven={parameter.driven}>
              <span>
                {parameter.label}
                {parameter.driven ? " (driven)" : ""}
              </span>
              <code>{parameter.name}</code>
              <input
                type="number"
                step="any"
                value={matrix[matrixIndex] ?? ""}
                aria-label={`${parameter.label} ${parameter.name}`}
                onChange={(event) =>
                  setMatrix((current) =>
                    current.map((value, currentIndex) =>
                      currentIndex === matrixIndex ? event.target.value : value,
                    ),
                  )
                }
              />
            </label>
          ))}
        </div>
      </fieldset>
      <label className="viewer-transform-node__sampling">
        <span>
          Sampling
          {transform.sampling.driven ? " (driver owned, inspect only)" : ""}
        </span>
        <select
          value={sampling}
          disabled={pending || transform.sampling.driven}
          onChange={(event) =>
            setSampling(event.target.value as typeof sampling)
          }
        >
          <option value="nearest">Nearest</option>
          <option value="bilinear">Bilinear</option>
        </select>
      </label>
      <div className="viewer-transform-node__actions">
        <button
          type="button"
          disabled={!canApply}
          onClick={() =>
            void publish({ matrix: numericMatrix, sampling }, "Transform edit")
          }
        >
          Apply
        </button>
        <button
          type="button"
          className="secondary"
          disabled={!canReset}
          onClick={() => void publish(resetDraft, "Transform reset")}
        >
          Reset identity
        </button>
      </div>
      <p
        className="viewer-transform-node__result"
        role={result && !result.includes("committed") && !result.includes("pending") ? "alert" : "status"}
        aria-live="polite"
      >
        {result ??
          `Canonical graph revision ${transform.graphRevision}. Matrix ${
            transform.matrixDriven ? "driver owned" : "editable"
          }; sampling ${transform.sampling.driven ? "driver owned" : "editable"}.`}
      </p>
    </article>
  );
}

function ViewerComparisonPresentation({
  state,
  current,
}: {
  readonly state: ViewerComparisonState;
  readonly current: ViewerFrameIdentity;
}) {
  if (state.mode === "single") return null;
  const captured = state.mode === "snapshot" ? state.snapshot : state.reference;
  const showCurrent =
    state.mode === "compare" ||
    state.mode === "split" ||
    state.mode === "wipe" ||
    state.mode === "difference";
  const showDivider =
    state.mode === "compare" || comparisonUsesPosition(state.mode);
  const dividerOrientation =
    state.mode === "compare" ? "vertical" : state.orientation;
  const dividerPosition =
    state.mode === "compare"
      ? "50%"
      : `${Math.round(state.position * 100)}%`;
  const dividerStyle =
    dividerOrientation === "vertical"
      ? { left: dividerPosition }
      : { top: dividerPosition };
  return (
    <>
      <span className="viewer-comparison__mode">{comparisonModeLabel(state.mode)}</span>
      {showCurrent ? (
        <span className="viewer-comparison__identity viewer-comparison__identity--current">
          <b>Current</b>
          <code>{viewerFrameLabel(current)}</code>
        </span>
      ) : null}
      {captured ? (
        <span className="viewer-comparison__identity viewer-comparison__identity--captured">
          <b>{state.mode === "snapshot" ? "Snapshot" : "Reference"}</b>
          <code>{viewerFrameLabel(captured)}</code>
        </span>
      ) : null}
      {showDivider ? (
        <i
          className="viewer-comparison__divider"
          aria-hidden="true"
          data-orientation={dividerOrientation}
          style={dividerStyle}
        />
      ) : null}
    </>
  );
}

function comparisonModeLabel(mode: ViewerComparisonState["mode"]): string {
  return (
    VIEWER_COMPARISON_DEFINITIONS.find(
      (definition) => definition.mode === mode,
    )?.label ?? "Single"
  );
}

function viewerFrameLabel(frame: ViewerFrameIdentity): string {
  const visual = frame.visual;
  const visualLabel = visual
    ? `s${visual.surfaceGeneration} f${visual.frameSequence}`
    : "native unavailable";
  const temporalLabel = frame.temporal
    ? `${frame.temporal.owner} context ${frame.temporal.value} @ ${frame.temporal.timebaseNumerator}/${frame.temporal.timebaseDenominator}`
    : "time unavailable";
  return `${visualLabel} · ${temporalLabel}`;
}

function ViewerOverlay({ overlay }: { readonly overlay: ViewerOverlayDefinition }) {
  const geometry = overlay.geometry;
  return (
    <span
      className={`viewer-overlay viewer-overlay--${overlay.kind}`}
      data-overlay-kind={overlay.kind}
      style={geometry ? {
        inset: `${geometry.insetTop}% ${geometry.insetRight}% ${geometry.insetBottom}% ${geometry.insetLeft}%`,
      } : undefined}
    >
      {overlay.kind === "aspect" ? <i>16:9</i> : null}
    </span>
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
