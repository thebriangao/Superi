import {
  Fragment,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent,
  type WheelEvent,
} from "react";

import type {
  EditorCanonicalDocument,
  EditorPlaybackState,
  EditorRationalTime,
  EditorStateSnapshot,
  EditorTimeRange,
} from "./api.ts";
import {
  generateProjectMediaPreview,
  readProjectMediaLibrary,
  type MediaPreviewBundle,
} from "./project-lifecycle.ts";
import {
  formatTimelineClipTiming,
  projectTimelineClipDetails,
  timelineClipAutomationKeyPercent,
  type TimelineClipPresentation,
  type TimelineClipProjection,
} from "./timeline-clip-presentation.ts";
import {
  TimelineProjectionError,
  buildTimelineRulerTicks,
  clampNumber,
  clampTimelineRange,
  formatTimelineTime,
  projectTimelineDocument,
  snapTimelineTime,
  timelineItemsInWindow,
  timelineFrameDuration,
  type TimelineCanvasItem,
  type TimelineCanvasModel,
} from "./timeline-workspace.ts";

const HEADER_WIDTH = 184;
const MIN_PIXELS_PER_SECOND = 0.2;
const MAX_PIXELS_PER_SECOND = 1_600;
const DEFAULT_PIXELS_PER_SECOND = 96;

type TimelineGesture = "playhead" | "in" | "out";

type TimelineClipPreviewState =
  | { readonly status: "loading" }
  | { readonly status: "ready"; readonly bundle: MediaPreviewBundle }
  | { readonly status: "unavailable"; readonly reason: string };

export interface TimelineWorkspaceProps {
  readonly document: EditorCanonicalDocument;
  readonly rootTimelineId: string;
  readonly playback: EditorPlaybackState;
  readonly snapshot: EditorStateSnapshot;
  readonly sharedSelectedClipIds: readonly string[];
  readonly onSelectClip: (clipId: string, extend: boolean) => void;
}

export function TimelineWorkspace({
  document,
  rootTimelineId,
  playback,
  snapshot,
  sharedSelectedClipIds,
  onSelectClip,
}: TimelineWorkspaceProps) {
  const projection = useMemo(() => {
    try {
      return {
        model: projectTimelineDocument(document, rootTimelineId),
        failure: null,
      };
    } catch (error) {
      return {
        model: null,
        failure:
          error instanceof TimelineProjectionError
            ? error.message
            : "The canonical timeline document could not be projected.",
      };
    }
  }, [document, rootTimelineId]);
  const model = projection.model;
  const clipProjection = useMemo(
    () => (model ? projectTimelineClipDetails(snapshot, model) : null),
    [model, snapshot],
  );
  const clipById = useMemo(() => {
    const result = new Map<string, TimelineClipPresentation>();
    if (clipProjection?.status === "ready") {
      for (const clip of clipProjection.clips) result.set(clip.id, clip);
    }
    return result;
  }, [clipProjection]);
  const sharedSelection = useMemo(
    () => new Set(sharedSelectedClipIds),
    [sharedSelectedClipIds],
  );
  const clipPreviews = useTimelineClipPreviews(
    clipProjection,
    snapshot.project.project_revision,
  );
  const initial = initialView(model, playback);
  const [playhead, setPlayhead] = useState(initial.playhead);
  const [inPoint, setInPoint] = useState(initial.inPoint);
  const [outPoint, setOutPoint] = useState(initial.outPoint);
  const [pixelsPerSecond, setPixelsPerSecond] = useState(
    DEFAULT_PIXELS_PER_SECOND,
  );
  const [viewportWidth, setViewportWidth] = useState(0);
  const [scrollLeft, setScrollLeft] = useState(0);
  const [gesture, setGesture] = useState<TimelineGesture | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const pendingScrollRef = useRef<number | null>(null);
  const autoFitIdentityRef = useRef<string | null>(null);
  const viewIdentityRef = useRef(
    model ? `${model.projectId}:${model.id}` : null,
  );

  useLayoutEffect(() => {
    const viewport = scrollRef.current;
    if (!viewport) return;
    const measure = () => setViewportWidth(viewport.clientWidth);
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(viewport);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    if (!model) return;
    const identity = `${model.projectId}:${model.id}`;
    if (viewIdentityRef.current !== identity) {
      const next = initialView(model, playback);
      viewIdentityRef.current = identity;
      setPlayhead(next.playhead);
      setInPoint(next.inPoint);
      setOutPoint(next.outPoint);
      return;
    }
    setPlayhead((value) =>
      clampNumber(value, model.startSeconds, model.endSeconds),
    );
    setInPoint((value) =>
      clampNumber(value, model.startSeconds, model.endSeconds),
    );
    setOutPoint((value) =>
      clampNumber(value, model.startSeconds, model.endSeconds),
    );
  }, [model, playback]);

  const visibleContentWidth = Math.max(1, viewportWidth - HEADER_WIDTH);
  const contentWidth = model
    ? Math.max(model.durationSeconds * pixelsPerSecond, visibleContentWidth)
    : visibleContentWidth;
  const maxScrollLeft = Math.max(0, contentWidth - visibleContentWidth);

  useLayoutEffect(() => {
    const viewport = scrollRef.current;
    const requested = pendingScrollRef.current;
    if (!viewport || requested === null) return;
    const next = clampNumber(requested, 0, maxScrollLeft);
    viewport.scrollLeft = next;
    setScrollLeft(next);
    pendingScrollRef.current = null;
  }, [pixelsPerSecond, contentWidth, maxScrollLeft]);

  const fitTimeline = useCallback(() => {
    if (!model) return;
    const fit = clampNumber(
      visibleContentWidth / model.durationSeconds,
      MIN_PIXELS_PER_SECOND,
      MAX_PIXELS_PER_SECOND,
    );
    pendingScrollRef.current = 0;
    if (scrollRef.current) {
      scrollRef.current.scrollLeft = 0;
      setScrollLeft(0);
    }
    setPixelsPerSecond(fit);
  }, [model, visibleContentWidth]);

  useLayoutEffect(() => {
    if (!model || viewportWidth === 0) return;
    const identity = `${model.projectId}:${model.id}`;
    if (autoFitIdentityRef.current === identity) return;
    autoFitIdentityRef.current = identity;
    fitTimeline();
  }, [fitTimeline, model, viewportWidth]);

  const zoomAt = useCallback(
    (factor: number, anchorViewportX = visibleContentWidth / 2) => {
      const viewport = scrollRef.current;
      if (!model || !viewport) return;
      const previous = pixelsPerSecond;
      const next = clampNumber(
        previous * factor,
        MIN_PIXELS_PER_SECOND,
        MAX_PIXELS_PER_SECOND,
      );
      if (next === previous) return;
      const anchor = clampNumber(anchorViewportX, 0, visibleContentWidth);
      const contentAtAnchor = viewport.scrollLeft + anchor;
      pendingScrollRef.current =
        (contentAtAnchor / previous) * next - anchor;
      setPixelsPerSecond(next);
    },
    [model, pixelsPerSecond, visibleContentWidth],
  );

  const visibleStartSeconds = model
    ? clampNumber(
        model.startSeconds + scrollLeft / pixelsPerSecond,
        model.startSeconds,
        model.endSeconds,
      )
    : 0;
  const visibleEndSeconds = model
    ? clampNumber(
        visibleStartSeconds + visibleContentWidth / pixelsPerSecond,
        visibleStartSeconds,
        model.endSeconds,
      )
    : 0;
  const rulerTicks = useMemo(
    () =>
      model
        ? buildTimelineRulerTicks({
            startSeconds: model.startSeconds,
            endSeconds: model.endSeconds,
            visibleStartSeconds,
            visibleEndSeconds,
            pixelsPerSecond,
            editRate: model.editRate,
          })
        : [],
    [model, pixelsPerSecond, visibleEndSeconds, visibleStartSeconds],
  );
  const displayTracks = useMemo(
    () => model?.tracks.slice().reverse() ?? [],
    [model],
  );
  const visibleSpanSeconds = Math.max(
    model ? timelineFrameDuration(model.editRate) : 0,
    visibleEndSeconds - visibleStartSeconds,
  );
  const renderedTracks = useMemo(
    () =>
      displayTracks.map((track) => ({
        track,
        visibleItems: timelineItemsInWindow(
          track.items,
          visibleStartSeconds,
          visibleEndSeconds,
          visibleSpanSeconds,
        ),
      })),
    [
      displayTracks,
      visibleEndSeconds,
      visibleSpanSeconds,
      visibleStartSeconds,
    ],
  );

  const eventTime = useCallback(
    (clientX: number) => {
      const viewport = scrollRef.current;
      if (!viewport || !model) return 0;
      const bounds = viewport.getBoundingClientRect();
      const contentX =
        clientX - bounds.left + viewport.scrollLeft - HEADER_WIDTH;
      const raw =
        model.startSeconds +
        clampNumber(contentX, 0, contentWidth) / pixelsPerSecond;
      return clampNumber(
        snapTimelineTime(raw, model.editRate, model.globalStartSeconds),
        model.startSeconds,
        model.endSeconds,
      );
    },
    [contentWidth, model, pixelsPerSecond],
  );

  const applyGesture = useCallback(
    (kind: TimelineGesture, value: number) => {
      if (!model) return;
      if (kind === "playhead") {
        setPlayhead(value);
        return;
      }
      if (kind === "in") {
        setInPoint(Math.min(value, outPoint));
        return;
      }
      setOutPoint(Math.max(value, inPoint));
    },
    [inPoint, model, outPoint],
  );

  const beginGesture = useCallback(
    (event: PointerEvent<HTMLElement>, kind: TimelineGesture) => {
      if (!model || event.button !== 0) return;
      event.preventDefault();
      event.stopPropagation();
      scrollRef.current?.setPointerCapture(event.pointerId);
      setGesture(kind);
      applyGesture(kind, eventTime(event.clientX));
    },
    [applyGesture, eventTime, model],
  );

  const moveGesture = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      if (!gesture) return;
      event.preventDefault();
      applyGesture(gesture, eventTime(event.clientX));
    },
    [applyGesture, eventTime, gesture],
  );

  const endGesture = useCallback((event: PointerEvent<HTMLDivElement>) => {
    if (scrollRef.current?.hasPointerCapture(event.pointerId)) {
      scrollRef.current.releasePointerCapture(event.pointerId);
    }
    setGesture(null);
  }, []);

  const handleWheel = useCallback(
    (event: WheelEvent<HTMLDivElement>) => {
      const viewport = scrollRef.current;
      if (!viewport || !model) return;
      if (event.metaKey || event.ctrlKey) {
        event.preventDefault();
        const bounds = viewport.getBoundingClientRect();
        const anchor = event.clientX - bounds.left - HEADER_WIDTH;
        zoomAt(Math.exp(-event.deltaY * 0.002), anchor);
        return;
      }
      if (event.shiftKey && Math.abs(event.deltaY) > Math.abs(event.deltaX)) {
        event.preventDefault();
        viewport.scrollLeft = clampNumber(
          viewport.scrollLeft + event.deltaY,
          0,
          maxScrollLeft,
        );
      }
    },
    [maxScrollLeft, model, zoomAt],
  );

  const sliderKey = useCallback(
    (event: KeyboardEvent<HTMLButtonElement>, kind: TimelineGesture) => {
      if (!model) return;
      const frame = timelineFrameDuration(model.editRate);
      const multiplier = event.shiftKey ? 10 : 1;
      const pageFrames = Math.max(
        1,
        Math.round(model.editRate.numerator / model.editRate.denominator),
      );
      const current =
        kind === "playhead" ? playhead : kind === "in" ? inPoint : outPoint;
      let next: number | null = null;
      if (event.key === "ArrowLeft" || event.key === "ArrowDown") {
        next = current - frame * multiplier;
      } else if (event.key === "ArrowRight" || event.key === "ArrowUp") {
        next = current + frame * multiplier;
      } else if (event.key === "PageDown") {
        next = current - frame * pageFrames;
      } else if (event.key === "PageUp") {
        next = current + frame * pageFrames;
      } else if (event.key === "Home") {
        next = model.startSeconds;
      } else if (event.key === "End") {
        next = model.endSeconds;
      }
      if (next === null) return;
      event.preventDefault();
      const shouldSnap = event.key !== "Home" && event.key !== "End";
      applyGesture(
        kind,
        clampNumber(
          shouldSnap
            ? snapTimelineTime(next, model.editRate, model.globalStartSeconds)
            : next,
          model.startSeconds,
          model.endSeconds,
        ),
      );
    },
    [applyGesture, inPoint, model, outPoint, playhead],
  );

  if (!model) {
    return (
      <section className="timeline-workspace timeline-workspace-failed">
        <header>
          <div>
            <p className="section-kicker">Timeline canvas</p>
            <h4>{rootTimelineId}</h4>
          </div>
          <span>Unavailable</span>
        </header>
        <p role="alert">{projection.failure}</p>
      </section>
    );
  }

  const range = clampTimelineRange(
    inPoint,
    outPoint,
    model.startSeconds,
    model.endSeconds,
  );
  const playheadX = (playhead - model.startSeconds) * pixelsPerSecond;
  const rangeStartX = (range.inPoint - model.startSeconds) * pixelsPerSecond;
  const rangeWidth = Math.max(
    1,
    (range.outPoint - range.inPoint) * pixelsPerSecond,
  );
  const stageStyle = {
    "--timeline-header-width": `${HEADER_WIDTH}px`,
    "--timeline-content-width": `${contentWidth}px`,
  } as CSSProperties;

  return (
    <section className="timeline-workspace" data-timeline-canvas>
      <header className="timeline-toolbar">
        <div className="timeline-toolbar-title">
          <p className="section-kicker">Timeline canvas</p>
          <h4>{model.name}</h4>
          <span>{model.id}</span>
          <div className="timeline-intent-badges">
            <b data-enabled={model.snappingEnabled}>
              Snap {model.snappingEnabled ? "on" : "off"}
            </b>
            <b data-enabled={model.linkedSelectionEnabled}>
              Linked selection {model.linkedSelectionEnabled ? "on" : "off"}
            </b>
          </div>
        </div>
        <div className="timeline-readouts">
          <TimelineReadout
            label="Playhead"
            value={formatTimelineTime(playhead, model.editRate)}
          />
          <TimelineReadout
            label="Range"
            value={
              `${formatTimelineTime(range.inPoint, model.editRate)} to ` +
              formatTimelineTime(range.outPoint, model.editRate)
            }
          />
          <TimelineReadout
            label="Visible"
            value={formatTimelineTime(visibleStartSeconds, model.editRate)}
          />
        </div>
        <div className="timeline-toolbar-actions">
          <button
            className="secondary timeline-compact-button"
            type="button"
            onClick={() => {
              setInPoint(Math.min(playhead, range.outPoint));
            }}
          >
            Set in
          </button>
          <button
            className="secondary timeline-compact-button"
            type="button"
            onClick={() => {
              setOutPoint(Math.max(playhead, range.inPoint));
            }}
          >
            Set out
          </button>
          <button
            className="secondary timeline-compact-button"
            type="button"
            onClick={() => {
              setInPoint(model.startSeconds);
              setOutPoint(model.endSeconds);
            }}
          >
            Full range
          </button>
          <span className="timeline-toolbar-divider" aria-hidden="true" />
          <button
            className="secondary timeline-icon-button"
            type="button"
            aria-label="Zoom out"
            title="Zoom out"
            onClick={() => zoomAt(0.72)}
          >
            -
          </button>
          <output className="timeline-scale-output">
            {formatScale(pixelsPerSecond)}
          </output>
          <button
            className="secondary timeline-icon-button"
            type="button"
            aria-label="Zoom in"
            title="Zoom in"
            onClick={() => zoomAt(1.38)}
          >
            +
          </button>
          <button
            className="secondary timeline-compact-button"
            type="button"
            aria-label="Fit timeline"
            onClick={fitTimeline}
          >
            Fit
          </button>
        </div>
      </header>
      <p className="timeline-gesture-hint">
        Scroll to navigate, Shift-scroll to move horizontally, Command or Control
        scroll to zoom, and drag the ruler or range handles for frame-precise timing.
      </p>
      {clipProjection?.status === "unavailable" ? (
        <p className="timeline-clip-detail-failure" role="alert">
          {clipProjection.reason}
        </p>
      ) : null}
      <div
        className={`timeline-scroll${gesture ? " timeline-scroll-gesturing" : ""}`}
        ref={scrollRef}
        onScroll={(event) => setScrollLeft(event.currentTarget.scrollLeft)}
        onPointerMove={moveGesture}
        onPointerUp={endGesture}
        onPointerCancel={endGesture}
        onWheel={handleWheel}
      >
        <div className="timeline-stage" style={stageStyle}>
          <div className="timeline-corner">
            <span>{displayTracks.length} tracks</span>
            <strong>
              {model.editRate.numerator}/{model.editRate.denominator} fps
            </strong>
          </div>
          <div
            className="timeline-ruler"
            aria-label="Timeline ruler"
            onPointerDown={(event) => beginGesture(event, "playhead")}
          >
            {rulerTicks.map((tick) => {
              const left = (tick.seconds - model.startSeconds) * pixelsPerSecond;
              return (
                <span
                  className={
                    tick.major
                      ? "timeline-ruler-tick timeline-ruler-tick-major"
                      : "timeline-ruler-tick"
                  }
                  key={`${tick.seconds}:${tick.major ? "major" : "minor"}`}
                  style={{ left }}
                >
                  {tick.label ? <b>{tick.label}</b> : null}
                </span>
              );
            })}
            <button
              type="button"
              className="timeline-range-handle timeline-range-handle-in"
              role="slider"
              aria-label="Timeline in point"
              aria-valuemin={model.startSeconds}
              aria-valuemax={range.outPoint}
              aria-valuenow={range.inPoint}
              aria-valuetext={formatTimelineTime(range.inPoint, model.editRate)}
              style={{ left: rangeStartX }}
              onPointerDown={(event) => beginGesture(event, "in")}
              onKeyDown={(event) => sliderKey(event, "in")}
            >
              I
            </button>
            <button
              type="button"
              className="timeline-range-handle timeline-range-handle-out"
              role="slider"
              aria-label="Timeline out point"
              aria-valuemin={range.inPoint}
              aria-valuemax={model.endSeconds}
              aria-valuenow={range.outPoint}
              aria-valuetext={formatTimelineTime(range.outPoint, model.editRate)}
              style={{ left: rangeStartX + rangeWidth }}
              onPointerDown={(event) => beginGesture(event, "out")}
              onKeyDown={(event) => sliderKey(event, "out")}
            >
              O
            </button>
            <button
              type="button"
              className="timeline-playhead-handle"
              role="slider"
              aria-label="Timeline playhead"
              aria-valuemin={model.startSeconds}
              aria-valuemax={model.endSeconds}
              aria-valuenow={playhead}
              aria-valuetext={formatTimelineTime(playhead, model.editRate)}
              style={{ left: playheadX }}
              onPointerDown={(event) => beginGesture(event, "playhead")}
              onKeyDown={(event) => sliderKey(event, "playhead")}
            >
              <span aria-hidden="true" />
            </button>
          </div>
          {renderedTracks.map(({ track, visibleItems }) => (
            <Fragment key={track.id}>
              <header className="timeline-track-header">
                <div>
                  <span>{track.kind}</span>
                  <strong title={track.name}>{track.name}</strong>
                  <code title={track.id}>{track.id}</code>
                </div>
                <div className="timeline-track-state">
                  {track.targeted ? <span>Target</span> : null}
                  {track.syncLocked ? <span>Sync</span> : null}
                </div>
              </header>
              <div
                className={`timeline-lane timeline-lane-${track.kind}`}
                data-track-id={track.id}
                onPointerDown={(event) => beginGesture(event, "playhead")}
              >
                {track.items.length === 0 ? (
                  <span className="timeline-empty-lane">No timed items</span>
                ) : (
                  visibleItems.map((item) => {
                    const detail = clipById.get(item.id) ?? null;
                    return (
                      <TimelineItem
                        detail={detail}
                        item={item}
                        key={`${item.kind}:${item.id}`}
                        model={model}
                        onSelectClip={onSelectClip}
                        pixelsPerSecond={pixelsPerSecond}
                        previews={clipPreviews}
                        sharedSelected={sharedSelection.has(item.id)}
                      />
                    );
                  })
                )}
              </div>
            </Fragment>
          ))}
          <div
            className="timeline-range"
            style={{
              left: HEADER_WIDTH + rangeStartX,
              width: rangeWidth,
            }}
            aria-hidden="true"
          />
          <div
            className="timeline-playhead"
            style={{ left: HEADER_WIDTH + playheadX }}
            aria-hidden="true"
          />
        </div>
      </div>
    </section>
  );
}

function TimelineItem({
  detail,
  item,
  model,
  onSelectClip,
  pixelsPerSecond,
  previews,
  sharedSelected,
}: {
  readonly detail: TimelineClipPresentation | null;
  readonly item: TimelineCanvasItem;
  readonly model: TimelineCanvasModel;
  readonly onSelectClip: (clipId: string, extend: boolean) => void;
  readonly pixelsPerSecond: number;
  readonly previews: ReadonlyMap<string, TimelineClipPreviewState>;
  readonly sharedSelected: boolean;
}) {
  const left = (item.startSeconds - model.startSeconds) * pixelsPerSecond;
  const width = Math.max(2, (item.endSeconds - item.startSeconds) * pixelsPerSecond);
  const sourceLabel = item.source
    ? `${item.source.kind}:${item.source.id}`
    : item.kind;
  const preview =
    detail?.source.kind === "media"
      ? previews.get(detail.source.id) ?? null
      : null;
  const evidence = [
    detail?.source.kind === "media" ? detail.source.relinkStatus : null,
    preview?.status === "loading"
      ? "preview loading"
      : preview?.status === "unavailable"
        ? "preview unavailable"
        : null,
    detail?.retimed ? "retimed" : null,
    detail?.multicam ? `multi ${detail.multicam.switchCount}` : null,
    detail && detail.effects.length > 0
      ? detail.effects.length === 1
        ? detail.effects[0]?.label
        : `fx ${detail.effects.length}`
      : null,
    detail?.automation && detail.automation.keyframes.length > 0
      ? `keys ${detail.automation.keyframes.length}`
      : null,
    detail?.automation ? `gain ${detail.automation.mode}` : null,
    detail?.automation?.activePass?.touchActive ? "touch active" : null,
    detail?.automation?.activePass?.latchActive ? "latch active" : null,
    detail && detail.markers.length > 0
      ? `marks ${detail.markers.length}`
      : null,
    detail && detail.metadataKeys.length > 0
      ? `meta ${detail.metadataKeys.length}`
      : null,
    detail && detail.groupedClipIds.length > 0
      ? `group ${detail.groupedClipIds.length + 1}`
      : item.group
        ? `group ${item.group.length}`
        : null,
    detail && detail.linkedClipIds.length > 0
      ? `link ${detail.linkedClipIds.length + 1}`
      : item.link
        ? `link ${item.link.length}`
        : null,
    detail?.targeted ? "target" : null,
    detail?.syncLocked ? "sync" : null,
    item.selected ? "timeline selected" : null,
    sharedSelected ? "workspace selected" : null,
  ].filter((value): value is string => value !== null);
  const title = [
    `${item.name} (${item.kind}:${item.id})`,
    detail ? formatTimelineClipTiming(detail) : `record ${formatExactRange(item.recordRange)}`,
    detail
      ? `source identity ${detail.source.kind}:${detail.source.id} (${detail.source.name})`
      : item.source
        ? `source identity ${sourceLabel}`
        : null,
    detail?.source.kind === "media"
      ? `source state ${detail.source.relinkStatus}`
      : null,
    preview?.status === "loading"
      ? "preview loading"
      : preview?.status === "unavailable"
        ? `preview unavailable: ${preview.reason}`
        : null,
    detail?.retimed
      ? `retime ${detail.timeMap.segments
          .map((segment) => `${segment.rateNumerator}/${segment.rateDenominator}`)
          .join(", ")}`
      : null,
    detail && detail.effects.length > 0
      ? `effects ${detail.effects
          .map(
            (effect) =>
              `${effect.label} (${effect.nodeType}, ${effect.driverCount} drivers)`,
          )
          .join(", ")}`
      : null,
    detail?.automation
      ? `clip gain ${detail.automation.mode}, keyframes ${detail.automation.keyframes
          .map(
            (keyframe) =>
              `${keyframe.sample}@${keyframe.sampleRate}=${keyframe.value}`,
          )
          .join(", ") || "none"}, active pass ${
          detail.automation.activePass
            ? `${detail.automation.activePass.currentValue}, touch ${detail.automation.activePass.touchActive}, latch ${detail.automation.activePass.latchActive}`
            : "none"
        }`
      : null,
    detail && detail.markers.length > 0
      ? `markers ${detail.markers
          .map((marker) => marker.label ?? marker.id)
          .join(", ")}`
      : null,
    detail && detail.metadataKeys.length > 0
      ? `metadata ${detail.metadataKeys.join(", ")}`
      : null,
    item.transition
      ? `transition ${item.transition.from.kind}:${item.transition.from.id} ` +
        `to ${item.transition.to.kind}:${item.transition.to.id}`
      : null,
  ]
    .filter((value): value is string => value !== null)
    .join("\n");
  const className =
    `timeline-item timeline-item-${item.kind}` +
    (item.selected ? " timeline-item-selected" : "") +
    (sharedSelected ? " timeline-item-shared-selected" : "");
  const data = {
    "data-item-id": item.id,
    "data-item-kind": item.kind,
    "data-record-start": item.recordRange.start.value,
    "data-record-duration": item.recordRange.duration.value,
    "data-source-id": item.source?.id,
    "data-grouped": item.group ? "true" : "false",
    "data-linked": item.link ? "true" : "false",
  };
  const label = [
    item.name,
    item.kind,
    `${formatTimelineTime(item.startSeconds, model.editRate)} to ${formatTimelineTime(
      item.endSeconds,
      model.editRate,
    )}`,
    detail ? `source ${detail.source.name}` : sourceLabel,
    ...evidence,
  ].join(", ");
  const contents = (
    <>
      {item.kind === "clip" ? (
        <TimelineClipVisual detail={detail} preview={preview} />
      ) : null}
      {detail?.automation ? (
        <TimelineClipAutomationKeys detail={detail} />
      ) : null}
      <span className="timeline-item-copy">
        <span className="timeline-item-kind">{item.kind}</span>
        <strong>{item.name}</strong>
        <small>{detail?.source.name ?? sourceLabel}</small>
        {evidence.length > 0 ? (
          <span className="timeline-item-evidence">
            {evidence.map((value) => (
              <b key={value}>{value}</b>
            ))}
          </span>
        ) : null}
      </span>
    </>
  );

  if (item.kind === "clip") {
    return (
      <button
        {...data}
        aria-label={label}
        aria-pressed={sharedSelected}
        className={className}
        onClick={(event: MouseEvent<HTMLButtonElement>) =>
          onSelectClip(
            item.id,
            event.shiftKey || event.metaKey || event.ctrlKey,
          )
        }
        onPointerDown={(event) => event.stopPropagation()}
        style={{ left, width }}
        title={title}
        type="button"
      >
        {contents}
      </button>
    );
  }
  return (
    <div
      {...data}
      aria-label={label}
      className={className}
      role="group"
      style={{ left, width }}
      title={title}
    >
      {contents}
    </div>
  );
}

function TimelineClipAutomationKeys({
  detail,
}: {
  readonly detail: TimelineClipPresentation;
}) {
  const positioned =
    detail.automation?.keyframes.flatMap((keyframe, index) => {
      const percent = timelineClipAutomationKeyPercent(detail, keyframe);
      return percent === null ? [] : [{ keyframe, index, percent }];
    }) ?? [];
  if (positioned.length === 0) return null;
  return (
    <span className="timeline-item-keyframes" aria-hidden="true">
      {positioned.map(({ keyframe, index, percent }) => (
        <span
          className="timeline-item-keyframe"
          data-keyframe-sample={keyframe.sample}
          key={`${keyframe.sample}:${keyframe.sampleRate}:${index}`}
          style={{ left: `${percent}%` }}
          title={`${keyframe.sample}@${keyframe.sampleRate}=${keyframe.value}`}
        />
      ))}
    </span>
  );
}

function TimelineClipVisual({
  detail,
  preview,
}: {
  readonly detail: TimelineClipPresentation | null;
  readonly preview: TimelineClipPreviewState | null;
}) {
  if (preview?.status === "ready" && detail?.trackKind === "audio") {
    const waveform = preview.bundle.waveform;
    if (waveform.status === "ready") {
      return (
        <span className="timeline-item-preview timeline-item-waveform" aria-hidden="true">
          <img alt="" draggable={false} src={waveform.artifact.image.data_url} />
        </span>
      );
    }
  }
  if (preview?.status === "ready" && detail?.trackKind === "video") {
    const filmstrip = preview.bundle.filmstrip;
    if (filmstrip.status === "ready" && filmstrip.artifact.frames.length > 0) {
      return (
        <span className="timeline-item-preview timeline-item-filmstrip" aria-hidden="true">
          {filmstrip.artifact.frames.slice(0, 8).map((frame, index) => (
            <img
              alt=""
              draggable={false}
              key={`${frame.source_index ?? index}:${index}`}
              src={frame.data_url}
            />
          ))}
        </span>
      );
    }
    const thumbnail = preview.bundle.thumbnail;
    if (thumbnail.status === "ready") {
      return (
        <span className="timeline-item-preview" aria-hidden="true">
          <img alt="" draggable={false} src={thumbnail.artifact.data_url} />
        </span>
      );
    }
    const still = preview.bundle.preview;
    if (still.status === "ready") {
      return (
        <span className="timeline-item-preview" aria-hidden="true">
          <img alt="" draggable={false} src={still.artifact.data_url} />
        </span>
      );
    }
  }
  const fallback =
    preview?.status === "loading"
      ? "Loading preview"
      : preview?.status === "unavailable"
        ? preview.reason
        : detail?.source.kind === "timeline"
          ? "Nested timeline"
          : "Preview unavailable";
  return (
    <span className="timeline-item-preview-fallback" aria-hidden="true" title={fallback}>
      {detail?.trackKind === "audio" ? "wave" : "clip"}
    </span>
  );
}

function useTimelineClipPreviews(
  projection: TimelineClipProjection | null,
  projectRevision: number,
): ReadonlyMap<string, TimelineClipPreviewState> {
  const [previews, setPreviews] = useState<
    ReadonlyMap<string, TimelineClipPreviewState>
  >(() => new Map());

  useEffect(() => {
    let cancelled = false;
    if (projection?.status !== "ready") {
      setPreviews(new Map());
      return () => {
        cancelled = true;
      };
    }

    const sources = new Map<
      string,
      Extract<TimelineClipPresentation["source"], { readonly kind: "media" }>
    >();
    for (const clip of projection.clips) {
      if (clip.source.kind === "media") sources.set(clip.source.id, clip.source);
    }
    const initial = new Map<string, TimelineClipPreviewState>();
    for (const source of sources.values()) {
      initial.set(
        source.id,
        source.relinkStatus === "missing" ||
          source.relinkStatus === "fingerprint_mismatch"
          ? {
              status: "unavailable",
              reason: `Source is ${source.relinkStatus.replaceAll("_", " ")}.`,
            }
          : { status: "loading" },
      );
    }
    setPreviews(initial);

    const update = (mediaId: string, state: TimelineClipPreviewState) => {
      if (cancelled) return;
      setPreviews((current) => {
        const next = new Map(current);
        next.set(mediaId, state);
        return next;
      });
    };

    const hydrate = async () => {
      const eligible = [...sources.values()].filter(
        (source) =>
          source.relinkStatus !== "missing" &&
          source.relinkStatus !== "fingerprint_mismatch",
      );
      if (eligible.length === 0) return;
      try {
        const library = await readProjectMediaLibrary();
        if (cancelled) return;
        if (
          library.project_revision !== projectRevision ||
          projection.projectRevision !== projectRevision
        ) {
          for (const source of eligible) {
            update(source.id, {
              status: "unavailable",
              reason: "Media library project revision changed.",
            });
          }
          return;
        }
        const itemById = new Map(
          library.items.map((item) => [item.media_id, item] as const),
        );
        for (const source of eligible) {
          if (cancelled) return;
          const item = itemById.get(source.id);
          if (item === undefined) {
            update(source.id, {
              status: "unavailable",
              reason: "Media library record is unavailable.",
            });
            continue;
          }
          if (
            item.offline.status === "offline" &&
            !item.offline.derived_fallback_available
          ) {
            update(source.id, {
              status: "unavailable",
              reason: "Source media is offline without a derived preview.",
            });
            continue;
          }
          try {
            const bundle = await generateProjectMediaPreview(library, item);
            if (cancelled) return;
            if (
              bundle.media_id !== source.id ||
              bundle.freshness !== item.content_fingerprint
            ) {
              update(source.id, {
                status: "unavailable",
                reason: "Generated preview freshness changed.",
              });
              continue;
            }
            update(source.id, { status: "ready", bundle });
          } catch {
            update(source.id, {
              status: "unavailable",
              reason: "Preview generation failed.",
            });
          }
        }
      } catch {
        for (const source of eligible) {
          update(source.id, {
            status: "unavailable",
            reason: "Media library could not be read.",
          });
        }
      }
    };
    void hydrate();
    return () => {
      cancelled = true;
    };
  }, [projectRevision, projection]);

  return previews;
}

function TimelineReadout({
  label,
  value,
}: {
  readonly label: string;
  readonly value: string;
}) {
  return (
    <span>
      <small>{label}</small>
      <strong>{value}</strong>
    </span>
  );
}

function initialView(
  model: TimelineCanvasModel | null,
  playback: EditorPlaybackState,
): { readonly playhead: number; readonly inPoint: number; readonly outPoint: number } {
  if (!model) return { playhead: 0, inPoint: 0, outPoint: 0 };
  let playhead = model.startSeconds;
  let inPoint = model.startSeconds;
  let outPoint = model.endSeconds;
  if (playback.status === "attached" && playback.latest) {
    const observedPlayhead = publicTimeSeconds(playback.latest.playhead);
    if (Number.isFinite(observedPlayhead)) {
      playhead = clampNumber(
        observedPlayhead,
        model.startSeconds,
        model.endSeconds,
      );
    }
    if (playback.latest.loop_range) {
      const loop = publicRangeSeconds(playback.latest.loop_range);
      if (Number.isFinite(loop.start) && Number.isFinite(loop.end)) {
        const range = clampTimelineRange(
          loop.start,
          loop.end,
          model.startSeconds,
          model.endSeconds,
        );
        inPoint = range.inPoint;
        outPoint = range.outPoint;
      }
    }
  }
  return { playhead, inPoint, outPoint };
}

function publicTimeSeconds(value: EditorRationalTime): number {
  return (
    (value.value * value.timebase.denominator) / value.timebase.numerator
  );
}

function publicRangeSeconds(value: EditorTimeRange): {
  readonly start: number;
  readonly end: number;
} {
  const start = publicTimeSeconds(value.start);
  const duration =
    (value.duration * value.start.timebase.denominator) /
    value.start.timebase.numerator;
  return { start, end: start + duration };
}

function formatExactRange(range: TimelineCanvasItem["recordRange"]): string {
  const rate = range.start.timebase;
  return `${range.start.value}+${range.duration.value} @ ${rate.numerator}/${rate.denominator}`;
}

function formatScale(value: number): string {
  if (value >= 100) return `${Math.round(value)} px/s`;
  if (value >= 10) return `${value.toFixed(1)} px/s`;
  return `${value.toFixed(2)} px/s`;
}
