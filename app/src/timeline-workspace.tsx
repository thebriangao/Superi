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
  type PointerEvent,
  type RefCallback,
  type WheelEvent,
} from "react";

import type {
  ApplicationAction,
  ApplicationSelection,
} from "./application.ts";
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
  TIMELINE_DEFAULT_SNAP_RULES,
  TimelineProjectionError,
  buildTimelineRulerTicks,
  clampNumber,
  clampTimelineRange,
  expandTimelineSelection,
  formatTimelineTime,
  parseTimelineSelectionIdentity,
  projectTimelineDocument,
  resolveTimelineSnap,
  snapTimelineTime,
  timelineObjectKey,
  timelineRectanglesIntersect,
  timelineItemsInWindow,
  timelineFrameDuration,
  timelineSelectionIdentity,
  timelineSelectionNeighbor,
  timelineSelectionRange,
  timelineSelectionTargets,
  type TimelineCanvasItem,
  type TimelineCanvasModel,
  type TimelineSnapMatch,
  type TimelineSnapRules,
  type TimelineRectangle,
  type TimelineSelectionDirection,
} from "./timeline-workspace.ts";

const HEADER_WIDTH = 184;
const MIN_PIXELS_PER_SECOND = 0.2;
const MAX_PIXELS_PER_SECOND = 1_600;
const DEFAULT_PIXELS_PER_SECOND = 96;
const SNAP_TOLERANCE_PIXELS = 10;
const MAX_SNAP_TOLERANCE_FRAMES = 12;
const LASSO_DRAG_THRESHOLD = 4;
const TIMELINE_SELECTION_HELP_ID = "timeline-selection-help";

const TIMELINE_SNAP_RULES = [
  { key: "timelineStart", label: "Timeline start" },
  { key: "playhead", label: "Playhead" },
  { key: "itemStart", label: "Item starts" },
  { key: "itemEnd", label: "Item ends" },
  { key: "markerStart", label: "Marker starts" },
  { key: "markerEnd", label: "Marker ends" },
] as const satisfies readonly {
  readonly key: keyof TimelineSnapRules;
  readonly label: string;
}[];

type TimelineGesture = "playhead" | "in" | "out";

type TimelineClipPreviewState =
  | { readonly status: "loading" }
  | { readonly status: "ready"; readonly bundle: MediaPreviewBundle }
  | { readonly status: "unavailable"; readonly reason: string };

interface TimelineLasso {
  readonly pointerId: number;
  readonly startX: number;
  readonly startY: number;
  readonly currentX: number;
  readonly currentY: number;
  readonly additive: boolean;
  readonly toggle: boolean;
  readonly direct: boolean;
  readonly dragged: boolean;
  readonly baseKeys: readonly string[];
}

export interface TimelineWorkspaceProps {
  readonly document: EditorCanonicalDocument;
  readonly rootTimelineId: string;
  readonly playback: EditorPlaybackState;
  readonly snapshot: EditorStateSnapshot;
  readonly selection: ApplicationSelection;
  readonly dispatchSelection: (action: ApplicationAction) => void;
  readonly selectionSchemaVersion: string;
  readonly selectionRevision: number;
}

export function TimelineWorkspace({
  document,
  rootTimelineId,
  playback,
  snapshot,
  selection,
  dispatchSelection,
  selectionSchemaVersion,
  selectionRevision,
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
  const [sessionSnappingEnabled, setSessionSnappingEnabled] = useState(true);
  const [snapRules, setSnapRules] = useState<TimelineSnapRules>(() => ({
    ...TIMELINE_DEFAULT_SNAP_RULES,
  }));
  const [snapMatch, setSnapMatch] = useState<TimelineSnapMatch | null>(null);
  const gestureOriginRef = useRef<{
    readonly kind: TimelineGesture;
    readonly value: number;
  } | null>(null);
  const gesturePointerRef = useRef<number | null>(null);
  const [lasso, setLasso] = useState<TimelineLasso | null>(null);
  const [lassoPreviewKeys, setLassoPreviewKeys] = useState<readonly string[] | null>(
    null,
  );
  const [focusedKey, setFocusedKey] = useState<string | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const stageRef = useRef<HTMLDivElement>(null);
  const itemRefs = useRef(new Map<string, HTMLElement>());
  const lassoRef = useRef<TimelineLasso | null>(null);
  const lassoPreviewKeysRef = useRef<readonly string[] | null>(null);
  const pendingScrollRef = useRef<number | null>(null);
  const autoFitIdentityRef = useRef<string | null>(null);
  const viewIdentityRef = useRef(
    model ? `${model.projectId}:${model.id}` : null,
  );
  const selectionTargets = useMemo(
    () => (model ? timelineSelectionTargets(model) : []),
    [model],
  );
  const selectionTargetsByKey = useMemo(
    () => new Map(selectionTargets.map((target) => [target.key, target])),
    [selectionTargets],
  );
  const selectedKeys = useMemo(() => {
    if (!model) return Object.freeze([]) as readonly string[];
    const keys: string[] = [];
    const seen = new Set<string>();
    for (const reference of selection.items) {
      if (
        reference.resource !== "superi.editor.state" ||
        reference.schema_version !== selectionSchemaVersion ||
        reference.revision !== selectionRevision
      ) {
        continue;
      }
      const parsed = parseTimelineSelectionIdentity(reference.identity);
      if (parsed?.timelineId !== model.id) continue;
      const key = timelineObjectKey(parsed.object);
      if (!selectionTargetsByKey.has(key) || seen.has(key)) continue;
      seen.add(key);
      keys.push(key);
    }
    return Object.freeze(keys);
  }, [
    model,
    selection.items,
    selectionRevision,
    selectionSchemaVersion,
    selectionTargetsByKey,
  ]);
  const selectionAnchorKey = useMemo(() => {
    if (!model || selection.anchor === null) return null;
    const reference = selection.anchor;
    if (
      reference.resource !== "superi.editor.state" ||
      reference.schema_version !== selectionSchemaVersion ||
      reference.revision !== selectionRevision
    ) {
      return null;
    }
    const parsed = parseTimelineSelectionIdentity(reference.identity);
    if (parsed?.timelineId !== model.id) return null;
    const key = timelineObjectKey(parsed.object);
    return selectionTargetsByKey.has(key) ? key : null;
  }, [
    model,
    selection.anchor,
    selectionRevision,
    selectionSchemaVersion,
    selectionTargetsByKey,
  ]);
  const visibleSelectionKeys = lassoPreviewKeys ?? selectedKeys;
  const visibleSelection = useMemo(
    () => new Set(visibleSelectionKeys),
    [visibleSelectionKeys],
  );
  const authoredSelection = useMemo(
    () =>
      new Set(
        selectionTargets
          .filter((target) => target.item.selected)
          .map((target) => target.key),
      ),
    [selectionTargets],
  );
  const rovingFocusKey =
    (focusedKey !== null && selectionTargetsByKey.has(focusedKey)
      ? focusedKey
      : null) ??
    selectionAnchorKey ??
    selectedKeys[0] ??
    authoredSelection.values().next().value ??
    selectionTargets[0]?.key ??
    null;

  useEffect(() => {
    if (focusedKey !== null && !selectionTargetsByKey.has(focusedKey)) {
      setFocusedKey(rovingFocusKey);
    }
  }, [focusedKey, rovingFocusKey, selectionTargetsByKey]);

  useEffect(() => {
    const pointerId = lassoRef.current?.pointerId ?? gesturePointerRef.current;
    lassoRef.current = null;
    lassoPreviewKeysRef.current = null;
    gestureOriginRef.current = null;
    gesturePointerRef.current = null;
    setLasso(null);
    setLassoPreviewKeys(null);
    setGesture(null);
    setSnapMatch(null);
    if (
      pointerId !== null &&
      scrollRef.current?.hasPointerCapture(pointerId)
    ) {
      scrollRef.current.releasePointerCapture(pointerId);
    }
  }, [model?.id, model?.projectId, model?.projectRevision, selectionRevision]);

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
      setSessionSnappingEnabled(true);
      setSnapRules({ ...TIMELINE_DEFAULT_SNAP_RULES });
      setSnapMatch(null);
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

  useEffect(() => {
    setSnapMatch(null);
  }, [model?.documentSha256]);

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

  const pointerSnapToleranceFrames = model
    ? clampNumber(
        Math.ceil(
          SNAP_TOLERANCE_PIXELS /
            (pixelsPerSecond * timelineFrameDuration(model.editRate)),
        ),
        1,
        MAX_SNAP_TOLERANCE_FRAMES,
      )
    : 1;

  const eventTime = useCallback(
    (
      kind: TimelineGesture,
      clientX: number,
    ): {
      readonly value: number;
      readonly match: TimelineSnapMatch | null;
    } => {
      const viewport = scrollRef.current;
      if (!viewport || !model) return { value: 0, match: null };
      const bounds = viewport.getBoundingClientRect();
      const contentX =
        clientX - bounds.left + viewport.scrollLeft - HEADER_WIDTH;
      const raw =
        model.startSeconds +
        clampNumber(contentX, 0, contentWidth) / pixelsPerSecond;
      const minimum = kind === "out" ? inPoint : model.startSeconds;
      const maximum = kind === "in" ? outPoint : model.endSeconds;
      const frameAligned = clampNumber(
        snapTimelineTime(raw, model.editRate, model.globalStartSeconds),
        minimum,
        maximum,
      );
      const candidate = resolveTimelineSnap(model, {
        atSeconds: frameAligned,
        toleranceFrames: pointerSnapToleranceFrames,
        playheadSeconds: kind === "playhead" ? null : playhead,
        rules: snapRules,
        sessionEnabled: sessionSnappingEnabled,
      });
      const match =
        candidate &&
        candidate.timeSeconds >= minimum &&
        candidate.timeSeconds <= maximum
          ? candidate
          : null;
      return { value: match?.timeSeconds ?? frameAligned, match };
    },
    [
      contentWidth,
      inPoint,
      model,
      outPoint,
      pixelsPerSecond,
      playhead,
      pointerSnapToleranceFrames,
      sessionSnappingEnabled,
      snapRules,
    ],
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

  const commitSelection = useCallback(
    (keys: readonly string[], requestedAnchor: string | null) => {
      if (!model) return;
      const normalized: string[] = [];
      const seen = new Set<string>();
      for (const key of keys) {
        if (!selectionTargetsByKey.has(key) || seen.has(key)) continue;
        seen.add(key);
        normalized.push(key);
      }
      if (normalized.length === 0) {
        dispatchSelection({ type: "clear_selection" });
        return;
      }
      const references = normalized.map((key) => {
        const target = selectionTargetsByKey.get(key)!;
        return Object.freeze({
          resource: "superi.editor.state" as const,
          schema_version: selectionSchemaVersion,
          identity: timelineSelectionIdentity(model.id, target.item),
          revision: selectionRevision,
        });
      });
      const anchorIndex =
        requestedAnchor === null ? -1 : normalized.indexOf(requestedAnchor);
      dispatchSelection({
        type: "replace_selection",
        items: references,
        anchor: anchorIndex === -1 ? references.at(-1) ?? null : references[anchorIndex],
      });
    },
    [
      dispatchSelection,
      model,
      selectionRevision,
      selectionSchemaVersion,
      selectionTargetsByKey,
    ],
  );

  const selectTarget = useCallback(
    (
      targetKey: string,
      modifiers: {
        readonly shiftKey: boolean;
        readonly altKey: boolean;
        readonly metaKey: boolean;
        readonly ctrlKey: boolean;
      },
      fallbackAnchor: string | null = null,
    ) => {
      if (!model || !selectionTargetsByKey.has(targetKey)) return;
      const direct = modifiers.altKey;
      const toggle = modifiers.metaKey || modifiers.ctrlKey;
      if (modifiers.shiftKey) {
        const range = timelineSelectionRange(
          model,
          selectionAnchorKey ?? fallbackAnchor ?? targetKey,
          targetKey,
          direct,
        );
        commitSelection(
          toggle ? unionSelection(selectedKeys, range) : range,
          targetKey,
        );
        return;
      }
      const related = expandTimelineSelection(model, [targetKey], direct);
      if (!toggle) {
        commitSelection(related, targetKey);
        return;
      }
      const current = new Set(selectedKeys);
      const remove = related.every((key) => current.has(key));
      for (const key of related) {
        if (remove) current.delete(key);
        else current.add(key);
      }
      commitSelection(
        selectionTargets
          .filter((target) => current.has(target.key))
          .map((target) => target.key),
        remove ? selectionAnchorKey : targetKey,
      );
    },
    [
      commitSelection,
      model,
      selectedKeys,
      selectionAnchorKey,
      selectionTargets,
      selectionTargetsByKey,
    ],
  );

  const focusItem = useCallback(
    (key: string) => {
      setFocusedKey(key);
      const target = selectionTargetsByKey.get(key);
      const viewport = scrollRef.current;
      if (model && target && viewport) {
        const itemStart =
          (target.item.startSeconds - model.startSeconds) * pixelsPerSecond;
        const itemEnd =
          (target.item.endSeconds - model.startSeconds) * pixelsPerSecond;
        let nextScroll = viewport.scrollLeft;
        if (itemStart < viewport.scrollLeft) nextScroll = itemStart;
        else if (itemEnd > viewport.scrollLeft + visibleContentWidth) {
          nextScroll = itemEnd - visibleContentWidth;
        }
        nextScroll = clampNumber(nextScroll, 0, maxScrollLeft);
        if (nextScroll !== viewport.scrollLeft) {
          viewport.scrollLeft = nextScroll;
          setScrollLeft(nextScroll);
        }
      }
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          const item = itemRefs.current.get(key);
          item?.focus({ preventScroll: true });
          item?.scrollIntoView({ block: "nearest", inline: "nearest" });
        });
      });
    },
    [
      maxScrollLeft,
      model,
      pixelsPerSecond,
      selectionTargetsByKey,
      visibleContentWidth,
    ],
  );

  const itemRef = useCallback(
    (key: string): RefCallback<HTMLElement> =>
      (node) => {
        if (node === null) itemRefs.current.delete(key);
        else itemRefs.current.set(key, node);
      },
    [],
  );

  const beginSelection = useCallback(
    (event: PointerEvent<HTMLElement>, key: string) => {
      if (event.button !== 0) return;
      event.preventDefault();
      event.stopPropagation();
      setFocusedKey(key);
      event.currentTarget.focus({ preventScroll: true });
      selectTarget(key, event, key);
    },
    [selectTarget],
  );

  const itemKeyDown = useCallback(
    (event: KeyboardEvent<HTMLElement>, key: string) => {
      if (!model) return;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "a") {
        event.preventDefault();
        commitSelection(
          selectionTargets.map((target) => target.key),
          key,
        );
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        commitSelection([], null);
        return;
      }
      if (event.key === "Enter" || event.key === " ") {
        event.preventDefault();
        selectTarget(key, event, key);
        return;
      }

      let direction: TimelineSelectionDirection | null = null;
      if (event.key === "ArrowLeft") direction = "left";
      else if (event.key === "ArrowRight") direction = "right";
      else if (event.key === "ArrowUp") direction = "up";
      else if (event.key === "ArrowDown") direction = "down";
      else if (event.key === "Home") direction = "home";
      else if (event.key === "End") direction = "end";
      if (direction === null) return;
      event.preventDefault();
      const next = timelineSelectionNeighbor(model, key, direction);
      if (next === null) return;
      focusItem(next);
      if (event.shiftKey) {
        commitSelection(
          timelineSelectionRange(
            model,
            selectionAnchorKey ?? key,
            next,
            event.altKey,
          ),
          next,
        );
      } else {
        commitSelection(expandTimelineSelection(model, [next], event.altKey), next);
      }
    },
    [
      commitSelection,
      focusItem,
      model,
      selectTarget,
      selectionAnchorKey,
      selectionTargets,
    ],
  );

  const beginGesture = useCallback(
    (event: PointerEvent<HTMLElement>, kind: TimelineGesture) => {
      if (!model || event.button !== 0) return;
      event.preventDefault();
      event.stopPropagation();
      scrollRef.current?.setPointerCapture(event.pointerId);
      gestureOriginRef.current = {
        kind,
        value: kind === "playhead" ? playhead : kind === "in" ? inPoint : outPoint,
      };
      gesturePointerRef.current = event.pointerId;
      setGesture(kind);
      const resolved = eventTime(kind, event.clientX);
      setSnapMatch(resolved.match);
      applyGesture(kind, resolved.value);
    },
    [applyGesture, eventTime, inPoint, model, outPoint, playhead],
  );

  const beginLasso = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      if (!model || event.button !== 0) return;
      event.preventDefault();
      event.stopPropagation();
      scrollRef.current?.setPointerCapture(event.pointerId);
      gestureOriginRef.current = { kind: "playhead", value: playhead };
      gesturePointerRef.current = event.pointerId;
      const resolved = eventTime("playhead", event.clientX);
      setSnapMatch(resolved.match);
      applyGesture("playhead", resolved.value);
      const toggle = event.metaKey || event.ctrlKey;
      const additive = event.shiftKey;
      const nextLasso = Object.freeze({
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY,
        currentX: event.clientX,
        currentY: event.clientY,
        additive,
        toggle,
        direct: event.altKey,
        dragged: false,
        baseKeys: selectedKeys,
      });
      const preview = toggle || additive ? selectedKeys : Object.freeze([]);
      lassoRef.current = nextLasso;
      lassoPreviewKeysRef.current = preview;
      setLasso(nextLasso);
      setLassoPreviewKeys(preview);
    },
    [applyGesture, eventTime, model, playhead, selectedKeys],
  );

  const moveGesture = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      const activeLasso = lassoRef.current;
      if (activeLasso) {
        if (event.pointerId !== activeLasso.pointerId) return;
        if (!model) {
          lassoRef.current = null;
          lassoPreviewKeysRef.current = null;
          setLasso(null);
          setLassoPreviewKeys(null);
          return;
        }
        event.preventDefault();
        const distance = Math.hypot(
          event.clientX - activeLasso.startX,
          event.clientY - activeLasso.startY,
        );
        const dragged = activeLasso.dragged || distance >= LASSO_DRAG_THRESHOLD;
        const rectangle = pointerRectangle(
          activeLasso.startX,
          activeLasso.startY,
          event.clientX,
          event.clientY,
        );
        const hitKeys: string[] = [];
        for (const item of scrollRef.current?.querySelectorAll<HTMLElement>(
          "[data-selection-key]",
        ) ?? []) {
          const key = item.dataset.selectionKey;
          if (
            key !== undefined &&
            timelineRectanglesIntersect(rectangle, item.getBoundingClientRect())
          ) {
            hitKeys.push(key);
          }
        }
        const related = dragged
          ? expandTimelineSelection(model, hitKeys, activeLasso.direct)
          : [];
        let preview: readonly string[];
        if (activeLasso.toggle) {
          const toggled = new Set(activeLasso.baseKeys);
          for (const key of related) {
            if (toggled.has(key)) toggled.delete(key);
            else toggled.add(key);
          }
          preview = selectionTargets
            .filter((target) => toggled.has(target.key))
            .map((target) => target.key);
        } else if (activeLasso.additive) {
          preview = unionSelection(activeLasso.baseKeys, related);
        } else {
          preview = related;
        }
        const nextPreview = Object.freeze([...preview]);
        const nextLasso = Object.freeze({
          ...activeLasso,
          currentX: event.clientX,
          currentY: event.clientY,
          dragged,
        });
        lassoPreviewKeysRef.current = nextPreview;
        lassoRef.current = nextLasso;
        setLassoPreviewKeys(nextPreview);
        setLasso(nextLasso);
        return;
      }
      if (gesture) {
        event.preventDefault();
        const resolved = eventTime(gesture, event.clientX);
        setSnapMatch(resolved.match);
        applyGesture(gesture, resolved.value);
      }
    },
    [applyGesture, eventTime, gesture, model, selectionTargets],
  );

  const commitLasso = useCallback(() => {
    const activeLasso = lassoRef.current;
    if (!activeLasso) return;
    const preview = lassoPreviewKeysRef.current ?? activeLasso.baseKeys;
    if (activeLasso.dragged) {
      commitSelection(preview, preview.at(-1) ?? null);
    } else if (!activeLasso.additive && !activeLasso.toggle) {
      commitSelection([], null);
    }
    lassoRef.current = null;
    lassoPreviewKeysRef.current = null;
    setLasso(null);
    setLassoPreviewKeys(null);
  }, [commitSelection]);

  const endGesture = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      const activeLasso = lassoRef.current;
      const pointerId = activeLasso?.pointerId ?? gesturePointerRef.current;
      if (pointerId !== null && event.pointerId !== pointerId) return;
      if (activeLasso) commitLasso();
      gestureOriginRef.current = null;
      gesturePointerRef.current = null;
      setGesture(null);
      if (scrollRef.current?.hasPointerCapture(event.pointerId)) {
        scrollRef.current.releasePointerCapture(event.pointerId);
      }
    },
    [commitLasso],
  );

  const cancelGesture = useCallback((event?: PointerEvent<HTMLDivElement>) => {
    const activeLasso = lassoRef.current;
    const origin = gestureOriginRef.current;
    const pointerId = activeLasso?.pointerId ?? gesturePointerRef.current;
    if (origin === null && activeLasso === null) return;
    if (event && pointerId !== null && event.pointerId !== pointerId) return;
    if (origin) applyGesture(origin.kind, origin.value);
    gestureOriginRef.current = null;
    gesturePointerRef.current = null;
    lassoRef.current = null;
    lassoPreviewKeysRef.current = null;
    setGesture(null);
    setLasso(null);
    setLassoPreviewKeys(null);
    setSnapMatch(null);
    if (
      pointerId !== null &&
      scrollRef.current?.hasPointerCapture(pointerId)
    ) {
      scrollRef.current.releasePointerCapture(pointerId);
    }
  }, [applyGesture]);

  useEffect(() => {
    if (!gesture && !lasso) return;
    const reverseOnEscape = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      cancelGesture();
    };
    window.addEventListener("keydown", reverseOnEscape);
    return () => window.removeEventListener("keydown", reverseOnEscape);
  }, [cancelGesture, gesture, lasso]);

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
      setSnapMatch(null);
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

  const toggleSessionSnapping = useCallback(() => {
    setSessionSnappingEnabled((enabled) => !enabled);
    setSnapMatch(null);
  }, []);

  const toggleSnapRule = useCallback((rule: keyof TimelineSnapRules) => {
    setSnapRules((current) => ({
      ...current,
      [rule]: !current[rule],
    }));
    setSnapMatch(null);
  }, []);

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
  const snapTargetX = snapMatch
    ? (snapMatch.timeSeconds - model.startSeconds) * pixelsPerSecond
    : null;
  const rangeStartX = (range.inPoint - model.startSeconds) * pixelsPerSecond;
  const rangeWidth = Math.max(
    1,
    (range.outPoint - range.inPoint) * pixelsPerSecond,
  );
  const stageStyle = {
    "--timeline-header-width": `${HEADER_WIDTH}px`,
    "--timeline-content-width": `${contentWidth}px`,
  } as CSSProperties;
  const targetSnappingActive =
    model.snappingEnabled && sessionSnappingEnabled;
  const gestureName =
    gesture === "playhead"
      ? "Playhead"
      : gesture === "in"
        ? "In point"
        : gesture === "out"
          ? "Out point"
          : "Last gesture";
  const snapStatus = snapMatch
    ? `${gestureName} snaps to ${snapMatch.target.label} at ${formatTimelineTime(
        snapMatch.timeSeconds,
        model.editRate,
      )}, ${snapMatch.distanceFrames} ${
        snapMatch.distanceFrames === 1 ? "frame" : "frames"
      } away.`
    : !model.snappingEnabled
      ? "Project target snapping is off. Gestures remain frame precise."
      : !sessionSnappingEnabled
        ? "Session target snapping is paused. Gestures remain frame precise."
        : gesture
          ? `${gestureName} remains frame aligned, with no enabled target in range.`
          : `${model.snapTargets.length} exact targets ready. Drag a timing tool to preview its consequence.`;
  const lassoStyle =
    lasso?.dragged && stageRef.current
      ? lassoStageRectangle(lasso, stageRef.current.getBoundingClientRect())
      : null;

  return (
    <section className="timeline-workspace" data-timeline-canvas>
      <header className="timeline-toolbar">
        <div className="timeline-toolbar-title">
          <p className="section-kicker">Timeline canvas</p>
          <h4>{model.name}</h4>
          <span>{model.id}</span>
          <div className="timeline-intent-badges">
            <b data-enabled={model.snappingEnabled}>
              Project snap {model.snappingEnabled ? "on" : "off"}
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
          <TimelineReadout
            label="Selected"
            value={String(visibleSelectionKeys.length)}
          />
        </div>
        <div className="timeline-toolbar-actions">
          <button
            className="secondary timeline-compact-button"
            type="button"
            onClick={() => {
              setSnapMatch(null);
              setInPoint(Math.min(playhead, range.outPoint));
            }}
          >
            Set in
          </button>
          <button
            className="secondary timeline-compact-button"
            type="button"
            onClick={() => {
              setSnapMatch(null);
              setOutPoint(Math.max(playhead, range.inPoint));
            }}
          >
            Set out
          </button>
          <button
            className="secondary timeline-compact-button"
            type="button"
            onClick={() => {
              setSnapMatch(null);
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
      <section
        className="timeline-snap-controls"
        aria-label="Timeline snap target rules"
        data-enabled={targetSnappingActive}
      >
        <button
          className="secondary timeline-snap-master"
          type="button"
          aria-pressed={targetSnappingActive}
          disabled={!model.snappingEnabled}
          onClick={toggleSessionSnapping}
        >
          Session target snap {targetSnappingActive ? "on" : "off"}
        </button>
        <div className="timeline-snap-rules" role="group" aria-label="Included targets">
          {TIMELINE_SNAP_RULES.map((rule) => (
            <button
              className="secondary timeline-snap-rule"
              type="button"
              aria-pressed={snapRules[rule.key]}
              disabled={!targetSnappingActive}
              key={rule.key}
              onClick={() => toggleSnapRule(rule.key)}
              title={`Include ${rule.label.toLowerCase()}`}
            >
              {rule.label}
            </button>
          ))}
        </div>
        <output
          className="timeline-snap-status"
          aria-live="polite"
          data-matched={snapMatch !== null}
        >
          {snapStatus}
        </output>
      </section>
      <div className="timeline-gesture-hint" id={TIMELINE_SELECTION_HELP_ID}>
        <span>
          Click to select, Command or Control-click toggles, Shift-click selects a
          range, Option-click selects directly, and drag empty track space for a
          lasso. Arrow keys navigate, Shift extends, Command or Control-A selects
          all, and Escape clears. Escape during a timing or lasso drag reverses
          it immediately. Scroll to navigate, Shift-scroll pans, and Command or
          Control-scroll zooms around the pointer.
        </span>
        <output className="timeline-selection-status" aria-live="polite">
          {visibleSelectionKeys.length === 0
            ? "No timeline items selected"
            : `${visibleSelectionKeys.length} timeline item${
                visibleSelectionKeys.length === 1 ? "" : "s"
              } selected`}
        </output>
      </div>
      {clipProjection?.status === "unavailable" ? (
        <p className="timeline-clip-detail-failure" role="alert">
          {clipProjection.reason}
        </p>
      ) : null}
      <div
        className={
          `timeline-scroll${gesture ? " timeline-scroll-gesturing" : ""}` +
          (lasso ? " timeline-scroll-selecting" : "")
        }
        ref={scrollRef}
        onScroll={(event) => setScrollLeft(event.currentTarget.scrollLeft)}
        onPointerMove={moveGesture}
        onPointerUp={endGesture}
        onPointerCancel={cancelGesture}
        onLostPointerCapture={cancelGesture}
        onWheel={handleWheel}
      >
        <div
          aria-describedby={TIMELINE_SELECTION_HELP_ID}
          aria-label={`${model.name} timeline items`}
          aria-multiselectable="true"
          className="timeline-stage"
          ref={stageRef}
          role="listbox"
          style={stageStyle}
        >
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
                onPointerDown={beginLasso}
              >
                {track.items.length === 0 ? (
                  <span className="timeline-empty-lane">No timed items</span>
                ) : (
                  visibleItems.map((item) => {
                    const detail = clipById.get(item.id) ?? null;
                    const key = timelineObjectKey(item);
                    return (
                      <TimelineItem
                        authoredSelected={authoredSelection.has(key)}
                        detail={detail}
                        focused={rovingFocusKey === key}
                        interactionSelected={visibleSelection.has(key)}
                        item={item}
                        itemRef={itemRef(key)}
                        key={key}
                        model={model}
                        onFocus={() => setFocusedKey(key)}
                        onKeyDown={(event) => itemKeyDown(event, key)}
                        onPointerDown={(event) => beginSelection(event, key)}
                        pixelsPerSecond={pixelsPerSecond}
                        previews={clipPreviews}
                        selectionKey={key}
                      />
                    );
                  })
                )}
              </div>
            </Fragment>
          ))}
          {lassoStyle ? (
            <div className="timeline-lasso" style={lassoStyle} aria-hidden="true" />
          ) : null}
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
          {snapMatch && snapTargetX !== null ? (
            <div
              className="timeline-snap-guide"
              data-snap-kind={snapMatch.target.kind}
              style={{ left: HEADER_WIDTH + snapTargetX }}
              aria-hidden="true"
            >
              <span>
                {snapMatch.target.label}
                <b>{formatTimelineTime(snapMatch.timeSeconds, model.editRate)}</b>
              </span>
            </div>
          ) : null}
        </div>
      </div>
    </section>
  );
}

function TimelineItem({
  authoredSelected,
  detail,
  focused,
  interactionSelected,
  item,
  itemRef,
  model,
  onFocus,
  onKeyDown,
  onPointerDown,
  pixelsPerSecond,
  previews,
  selectionKey,
}: {
  readonly authoredSelected: boolean;
  readonly detail: TimelineClipPresentation | null;
  readonly focused: boolean;
  readonly interactionSelected: boolean;
  readonly item: TimelineCanvasItem;
  readonly itemRef: RefCallback<HTMLElement>;
  readonly model: TimelineCanvasModel;
  readonly onFocus: () => void;
  readonly onKeyDown: (event: KeyboardEvent<HTMLElement>) => void;
  readonly onPointerDown: (event: PointerEvent<HTMLElement>) => void;
  readonly pixelsPerSecond: number;
  readonly previews: ReadonlyMap<string, TimelineClipPreviewState>;
  readonly selectionKey: string;
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
    authoredSelected ? "authored selected" : null,
    interactionSelected ? "workspace selected" : null,
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
    (authoredSelected ? " timeline-item-authored-selected" : "") +
    (interactionSelected ? " timeline-item-selected" : "");
  const data = {
    "data-item-id": item.id,
    "data-item-kind": item.kind,
    "data-record-start": item.recordRange.start.value,
    "data-record-duration": item.recordRange.duration.value,
    "data-source-id": item.source?.id,
    "data-grouped": item.group ? "true" : "false",
    "data-linked": item.link ? "true" : "false",
    "data-selection-key": selectionKey,
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
      {detail?.automation ? <TimelineClipAutomationKeys detail={detail} /> : null}
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
  const keyShortcuts =
    "ArrowLeft ArrowRight ArrowUp ArrowDown Home End Shift+ArrowLeft " +
    "Shift+ArrowRight Shift+ArrowUp Shift+ArrowDown Shift+Home Shift+End " +
    "Control+A Meta+A Enter Space Escape";

  if (item.kind === "clip") {
    return (
      <button
        {...data}
        aria-describedby={TIMELINE_SELECTION_HELP_ID}
        aria-keyshortcuts={keyShortcuts}
        aria-label={label}
        aria-selected={interactionSelected}
        className={className}
        onFocus={onFocus}
        onKeyDown={onKeyDown}
        onPointerDown={onPointerDown}
        ref={itemRef}
        role="option"
        style={{ left, width }}
        tabIndex={focused ? 0 : -1}
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
      aria-describedby={TIMELINE_SELECTION_HELP_ID}
      aria-keyshortcuts={keyShortcuts}
      aria-label={label}
      aria-selected={interactionSelected}
      className={className}
      onFocus={onFocus}
      onKeyDown={onKeyDown}
      onPointerDown={onPointerDown}
      ref={itemRef}
      role="option"
      style={{ left, width }}
      tabIndex={focused ? 0 : -1}
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

function unionSelection(
  left: readonly string[],
  right: readonly string[],
): readonly string[] {
  const values: string[] = [];
  const seen = new Set<string>();
  for (const key of [...left, ...right]) {
    if (seen.has(key)) continue;
    seen.add(key);
    values.push(key);
  }
  return Object.freeze(values);
}

function pointerRectangle(
  firstX: number,
  firstY: number,
  secondX: number,
  secondY: number,
): TimelineRectangle {
  return {
    left: Math.min(firstX, secondX),
    top: Math.min(firstY, secondY),
    right: Math.max(firstX, secondX),
    bottom: Math.max(firstY, secondY),
  };
}

function lassoStageRectangle(
  lasso: TimelineLasso,
  stage: TimelineRectangle,
): CSSProperties {
  const rectangle = pointerRectangle(
    lasso.startX,
    lasso.startY,
    lasso.currentX,
    lasso.currentY,
  );
  return {
    left: rectangle.left - stage.left,
    top: rectangle.top - stage.top,
    width: Math.max(1, rectangle.right - rectangle.left),
    height: Math.max(1, rectangle.bottom - rectangle.top),
  };
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
