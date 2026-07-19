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
  type FocusEvent,
} from "react";

import type {
  ApplicationAction,
  ApplicationSelection,
} from "./application.ts";
import type {
  EditorCanonicalDocument,
  EditorPlaybackState,
  ProjectAction,
  EditorRationalTime,
  EditorStateSnapshot,
  EditorTimeRange,
  TimelineTrackMutation,
  ExecuteProjectCommand,
  ExecuteProjectCommandResult,
} from "./api.ts";
import type { SourceMonitorSnapshot } from "./project-lifecycle.ts";
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
  TimelineEditingError,
  compileGapClose,
  compileGapInsert,
  compileRippleDelete,
  compileTimelineGesture,
  timelineEditingTools,
  type TimelineEditPlan,
  type TimelineEditingSide,
  type TimelineEditingTool,
  type TimelineExtendMode,
} from "./timeline-editing.ts";
import {
  buildSetTransitionAction,
  buildTransitionParameterAction,
  projectTimelineTransitionDetails,
  transitionHandlesForAlignment,
  transitionHandlesForDuration,
  type TimelineTransitionAlignment,
  type TimelineTransitionParameterPresentation,
  type TimelineTransitionPresentation,
} from "./timeline-transition-presentation.ts";
import {
  TIMELINE_DEFAULT_SNAP_RULES,
  TimelineProjectionError,
  buildTimelineEditCommand,
  buildTimelineHistoryCommand,
  buildTimelineRulerTicks,
  clampNumber,
  clampTimelineRange,
  expandTimelineSelection,
  formatTimelineTime,
  parseTimelineSelectionIdentity,
  projectTimelineDocument,
  resolveTimelineSnap,
  projectSourceMonitorForTimelineEdit,
  snapTimelineTime,
  timelineObjectKey,
  timelineRectanglesIntersect,
  timelineItemsInWindow,
  timelineFrameDuration,
  timelineSelectionIdentity,
  timelineSelectionNeighbor,
  timelineSelectionRange,
  timelineSelectionTargets,
  MAX_TRACK_HEIGHT,
  MIN_TRACK_HEIGHT,
  type TimelineCanvasItem,
  type TimelineCanvasModel,
  type TimelineCanvasTrack,
  type TimelineSnapMatch,
  type TimelineSnapRules,
  type TimelineRectangle,
  type TimelineSelectionDirection,
  type TimelineTrackKind,
  type TimelineEditCommandResult,
  type TimelineEditGesture,
  type TimelineEditSource,
  type TimelineThreePointMode,
} from "./timeline-workspace.ts";

const HEADER_WIDTH = 268;
const MIN_PIXELS_PER_SECOND = 0.2;
const MAX_PIXELS_PER_SECOND = 1_600;
const DEFAULT_PIXELS_PER_SECOND = 96;
const SNAP_TOLERANCE_PIXELS = 10;
const MAX_SNAP_TOLERANCE_FRAMES = 12;
const LASSO_DRAG_THRESHOLD = 4;
const EDIT_DRAG_THRESHOLD = 4;
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
const TIMELINE_EDIT_GESTURES: readonly TimelineEditGesture[] = [
  "insert",
  "overwrite",
  "append",
  "replace",
  "three_point",
  "four_point",
  "lift",
  "extract",
  "backspace",
];

const TIMELINE_THREE_POINT_MODES = [
  {
    value: "source_range_at_record_start",
    label: "Source range at record start",
    detail: "Source in and out plus the playhead as record start",
  },
  {
    value: "source_start_over_record_range",
    label: "Source start over record range",
    detail: "Source in plus the timeline in and out range",
  },
  {
    value: "source_range_backtimed_to_record_end",
    label: "Source range backtimed to record end",
    detail: "Source in and out plus the playhead as record end",
  },
  {
    value: "source_end_backtimed_over_record_range",
    label: "Source end backtimed over record range",
    detail: "Source out plus the timeline in and out range",
  },
] as const satisfies readonly {
  readonly value: TimelineThreePointMode;
  readonly label: string;
  readonly detail: string;
}[];

type TimelineGesture = "playhead" | "in" | "out";

interface TimelineEditTarget {
  readonly trackId: string;
  readonly itemId: string;
}

interface TimelineEditGestureState extends TimelineEditTarget {
  readonly pointerId: number;
  readonly side: TimelineEditingSide;
  readonly grabOffsetSeconds: number;
  readonly pointerStartClientX: number;
  readonly dragged: boolean;
  readonly plan: TimelineEditPlan | null;
}

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
  readonly mutateTracks: (
    mutations: readonly TimelineTrackMutation[],
  ) => Promise<void>;
  readonly executeProjectActions: (
    actions: readonly ProjectAction[],
  ) => Promise<ExecuteProjectCommandResult>;
  readonly sourceMonitor: SourceMonitorSnapshot | null;
  readonly onExecuteProjectCommand: (
    request: ExecuteProjectCommand,
  ) => Promise<ExecuteProjectCommandResult>;
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
  mutateTracks,
  executeProjectActions,
  sourceMonitor,
  onExecuteProjectCommand,
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
  const transitionProjection = useMemo(
    () => (model ? projectTimelineTransitionDetails(snapshot, model) : null),
    [model, snapshot],
  );
  const initial = initialView(model, playback);
  const [playhead, setPlayhead] = useState(initial.playhead);
  const [inPoint, setInPoint] = useState(initial.inPoint);
  const [outPoint, setOutPoint] = useState(initial.outPoint);
  const [rangeExplicit, setRangeExplicit] = useState(false);
  const [pixelsPerSecond, setPixelsPerSecond] = useState(
    DEFAULT_PIXELS_PER_SECOND,
  );
  const [viewportWidth, setViewportWidth] = useState(0);
  const [scrollLeft, setScrollLeft] = useState(0);
  const [gesture, setGesture] = useState<TimelineGesture | null>(null);
  const [editingTool, setEditingTool] = useState<TimelineEditingTool>("trim");
  const [editingSide, setEditingSide] =
    useState<TimelineEditingSide>("end");
  const [extendMode, setExtendMode] =
    useState<TimelineExtendMode>("ripple");
  const [activeEditTarget, setActiveEditTarget] =
    useState<TimelineEditTarget | null>(null);
  const [editGesture, setEditGesture] =
    useState<TimelineEditGestureState | null>(null);
  const [editPlan, setEditPlan] = useState<TimelineEditPlan | null>(null);
  const [editFailure, setEditFailure] = useState<string | null>(null);
  const [editMessage, setEditMessage] = useState(
    "Select a timed object, choose a tool, then drag or apply at the playhead.",
  );
  const [gapFrameCount, setGapFrameCount] = useState(24);
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
  const [pendingTrackAction, setPendingTrackAction] = useState<string | null>(null);
  const [trackFailure, setTrackFailure] = useState<string | null>(null);
  const trackActionPendingRef = useRef(false);
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
  const selectedEditItemIds = useMemo(
    () =>
      selectedKeys.flatMap((key) => {
        const target = selectionTargetsByKey.get(key);
        return target ? [target.item.id] : [];
      }),
    [selectedKeys, selectionTargetsByKey],
  );
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
  const selectedTransition = useMemo(() => {
    if (visibleSelectionKeys.length !== 1 || transitionProjection === null) {
      return null;
    }
    const target = selectionTargetsByKey.get(visibleSelectionKeys[0]);
    if (target?.item.kind !== "transition") return null;
    return (
      transitionProjection.transitions.find(
        (transition) => transition.id === target.item.id,
      ) ?? null
    );
  }, [selectionTargetsByKey, transitionProjection, visibleSelectionKeys]);
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

  const executeTrackMutations = useCallback(
    async (identity: string, mutations: readonly TimelineTrackMutation[]) => {
      if (trackActionPendingRef.current) return;
      trackActionPendingRef.current = true;
      setPendingTrackAction(identity);
      setTrackFailure(null);
      try {
        await mutateTracks(mutations);
      } catch (error: unknown) {
        setTrackFailure(
          error instanceof Error
            ? error.message
            : "The track command could not be completed.",
        );
      } finally {
        trackActionPendingRef.current = false;
        setPendingTrackAction(null);
      }
    },
    [mutateTracks],
  );
  const [targetTrackId, setTargetTrackId] = useState(
    () => preferredTargetTrackId(model),
  );
  const [selectedEdit, setSelectedEdit] =
    useState<TimelineEditGesture>("insert");
  const [threePointMode, setThreePointMode] =
    useState<TimelineThreePointMode>("source_range_at_record_start");
  const [commandPending, setCommandPending] = useState(false);
  const commandPendingRef = useRef(false);
  const [commandStatus, setCommandStatus] = useState(
    "Choose an exact target and editorial gesture.",
  );
  const transactionSequenceRef = useRef(0);

  useEffect(() => {
    setTargetTrackId((current) =>
      model?.tracks.some((track) => track.id === current)
        ? current
        : preferredTargetTrackId(model),
    );
  }, [model]);

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
      setRangeExplicit(false);
      setActiveEditTarget(null);
      setEditGesture(null);
      setEditPlan(null);
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
    setEditGesture(null);
    setEditPlan(null);
  }, [model?.documentSha256]);

  const sourceProjection = useMemo(
    () => (model ? projectSourceMonitorForTimelineEdit(sourceMonitor, model) : null),
    [model, sourceMonitor],
  );
  const gestureCommandPlan = useMemo<TimelineEditCommandResult | null>(() => {
    if (!model) return null;
    let previewSequence = 0;
    const plan = buildTimelineEditCommand({
      gesture: selectedEdit,
      model,
      targetTrackId,
      playheadSeconds: playhead,
      inPointSeconds: inPoint,
      outPointSeconds: outPoint,
      rangeExplicit,
      source:
        sourceProjection?.status === "ready" ? sourceProjection.source : null,
      threePointMode,
      selectedItemIds: selectedEditItemIds,
      transactionId: "superi.desktop.timeline.preview",
      createId: (kind) => {
        previewSequence += 1;
        return `${kind}:${previewSequence.toString(16).padStart(32, "0")}`;
      },
    });
    if (
      plan.status === "disabled" &&
      editGestureUsesSource(selectedEdit) &&
      sourceProjection?.status === "disabled"
    ) {
      return { ...plan, reason: sourceProjection.reason };
    }
    return plan;
  }, [
    inPoint,
    model,
    outPoint,
    playhead,
    rangeExplicit,
    selectedEdit,
    selectedEditItemIds,
    sourceProjection,
    targetTrackId,
    threePointMode,
  ]);

  const nextTransactionId = useCallback((kind: string) => {
    transactionSequenceRef.current += 1;
    return `superi.desktop.timeline.${kind}.${transactionSequenceRef.current}.${randomHex(8)}`;
  }, []);

  const executeEdit = useCallback(
    async (edit: TimelineEditGesture) => {
      setSelectedEdit(edit);
      if (!model || commandPendingRef.current) return;
      if (
        editGestureUsesSource(edit) &&
        sourceProjection?.status !== "ready"
      ) {
        setCommandStatus(
          sourceProjection?.reason ?? "Load a compatible source before editing.",
        );
        return;
      }
      let transactionId: string;
      try {
        transactionId = nextTransactionId(edit);
      } catch (error: unknown) {
        setCommandStatus(timelineCommandFailure(error));
        return;
      }
      const plan = buildTimelineEditCommand({
        gesture: edit,
        model,
        targetTrackId,
        playheadSeconds: playhead,
        inPointSeconds: inPoint,
        outPointSeconds: outPoint,
        rangeExplicit,
        source:
          sourceProjection?.status === "ready" ? sourceProjection.source : null,
        threePointMode,
        selectedItemIds: selectedEditItemIds,
        transactionId,
        createId: randomEditorialId,
      });
      if (plan.status === "disabled") {
        setCommandStatus(plan.reason);
        return;
      }
      commandPendingRef.current = true;
      setCommandPending(true);
      setCommandStatus(`Applying ${timelineEditGestureLabel(edit)} to ${plan.target}.`);
      try {
        const result = await onExecuteProjectCommand(plan.request);
        setCommandStatus(
          `${timelineEditGestureLabel(edit)} completed at project revision ${result.state.project_revision}. Undo is available immediately.`,
        );
      } catch (error: unknown) {
        setCommandStatus(timelineCommandFailure(error));
      } finally {
        commandPendingRef.current = false;
        setCommandPending(false);
      }
    },
    [
      inPoint,
      model,
      nextTransactionId,
      onExecuteProjectCommand,
      outPoint,
      playhead,
      rangeExplicit,
      selectedEditItemIds,
      sourceProjection,
      targetTrackId,
      threePointMode,
    ],
  );

  const executeHistory = useCallback(
    async (command: "undo" | "redo") => {
      if (commandPendingRef.current) return;
      const available = command === "undo"
        ? snapshot.project.undo_depth
        : snapshot.project.redo_depth;
      if (available === 0) {
        setCommandStatus(`There is no ${command} step available.`);
        return;
      }
      commandPendingRef.current = true;
      setCommandPending(true);
      setCommandStatus(`${capitalize(command)} is in progress.`);
      try {
        const result = await onExecuteProjectCommand(
          buildTimelineHistoryCommand(
            command,
            snapshot.project.project_revision,
            nextTransactionId(command),
          ),
        );
        setCommandStatus(
          `${capitalize(command)} completed at project revision ${result.state.project_revision}.`,
        );
      } catch (error: unknown) {
        setCommandStatus(timelineCommandFailure(error));
      } finally {
        commandPendingRef.current = false;
        setCommandPending(false);
      }
    },
    [
      nextTransactionId,
      onExecuteProjectCommand,
      snapshot.project.project_revision,
      snapshot.project.redo_depth,
      snapshot.project.undo_depth,
    ],
  );

  useEffect(() => {
    const handleEditShortcut = (event: globalThis.KeyboardEvent) => {
      if (
        event.defaultPrevented ||
        isEditableTimelineTarget(event.target)
      ) {
        return;
      }
      if (event.key === "Backspace" && !event.metaKey && !event.ctrlKey) {
        event.preventDefault();
        void executeEdit("backspace");
        return;
      }
      if (
        event.key.toLowerCase() === "z" &&
        (event.metaKey || event.ctrlKey) &&
        !event.altKey
      ) {
        event.preventDefault();
        void executeHistory(event.shiftKey ? "redo" : "undo");
      }
    };
    window.addEventListener("keydown", handleEditShortcut, true);
    return () => window.removeEventListener("keydown", handleEditShortcut, true);
  }, [executeEdit, executeHistory]);

  const activeEditTrack = useMemo(
    () =>
      model && activeEditTarget
        ? model.tracks.find((track) => track.id === activeEditTarget.trackId) ?? null
        : null,
    [activeEditTarget, model],
  );
  const activeEditItem = useMemo(
    () =>
      activeEditTrack && activeEditTarget
        ? activeEditTrack.items.find(
            (item) =>
              item.id === activeEditTarget.itemId && item.kind !== "transition",
          ) ?? null
        : null,
    [activeEditTarget, activeEditTrack],
  );
  const operationTrack =
    activeEditTrack ??
    model?.tracks.find((track) => track.targeted) ??
    model?.tracks[0] ??
    null;
  const activeEditLocked = activeEditTrack?.locked ?? false;
  const operationTrackLocked = operationTrack?.locked ?? false;

  useEffect(() => {
    if (activeEditTarget && (!activeEditTrack || !activeEditItem)) {
      setActiveEditTarget(null);
      setEditMessage("The prior edit target no longer exists in this revision.");
    }
  }, [activeEditItem, activeEditTarget, activeEditTrack]);

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

  const rawEditPointerTime = useCallback(
    (clientX: number): number => {
      const viewport = scrollRef.current;
      if (!viewport || !model) return 0;
      const bounds = viewport.getBoundingClientRect();
      const contentX =
        clientX - bounds.left + viewport.scrollLeft - HEADER_WIDTH;
      return clampNumber(
        model.startSeconds +
          clampNumber(contentX, 0, contentWidth) / pixelsPerSecond,
        model.startSeconds,
        model.endSeconds,
      );
    },
    [contentWidth, model, pixelsPerSecond],
  );

  const resolveEditTime = useCallback(
    (
      raw: number,
    ): { readonly value: number; readonly match: TimelineSnapMatch | null } => {
      if (!model) return { value: 0, match: null };
      const frameAligned = clampNumber(
        snapTimelineTime(raw, model.editRate, model.globalStartSeconds),
        model.startSeconds,
        model.endSeconds,
      );
      const match = resolveTimelineSnap(model, {
        atSeconds: frameAligned,
        toleranceFrames: pointerSnapToleranceFrames,
        playheadSeconds: playhead,
        rules: snapRules,
        sessionEnabled: sessionSnappingEnabled,
      });
      return { value: match?.timeSeconds ?? frameAligned, match };
    },
    [
      model,
      playhead,
      pointerSnapToleranceFrames,
      sessionSnappingEnabled,
      snapRules,
    ],
  );

  const compileEditGesturePlan = useCallback(
    (
      target: TimelineEditTarget,
      side: TimelineEditingSide,
      toSeconds: number,
    ): TimelineEditPlan => {
      if (!model) throw new TimelineEditingError("Timeline state is unavailable.");
      return compileTimelineGesture({
        model,
        tool: editingTool,
        trackId: target.trackId,
        itemId: target.itemId,
        side,
        toSeconds,
        extendMode,
      });
    },
    [editingTool, extendMode, model],
  );

  const showEditPlan = useCallback((plan: TimelineEditPlan) => {
    setEditPlan(plan);
    setEditFailure(null);
    setEditMessage(
      `${plan.label}. ${plan.operations.length} atomic ${
        plan.operations.length === 1 ? "operation" : "operations"
      }, ${plan.affectedItemIds.length} affected ${
        plan.affectedItemIds.length === 1 ? "object" : "objects"
      }.`,
    );
  }, []);

  const reportEditFailure = useCallback((error: unknown) => {
    const message = timelineEditErrorMessage(error);
    setEditPlan(null);
    setEditFailure(message);
    setEditMessage(message);
  }, []);

  const executeEditPlan = useCallback(
    async (plan: TimelineEditPlan) => {
      if (commandPendingRef.current) return;
      commandPendingRef.current = true;
      setCommandPending(true);
      setEditFailure(null);
      setEditMessage(`Applying ${plan.label.toLowerCase()} through project history.`);
      try {
        const result = await executeProjectActions([
          {
            action: "edit_timeline",
            operations: [...plan.operations],
          },
        ]);
        setEditPlan(null);
        setSnapMatch(null);
        setEditMessage(
          `${plan.label} applied at project revision ${result.state.project_revision} and refreshed from canonical state. Undo is available immediately.`,
        );
      } catch (error) {
        reportEditFailure(error);
      } finally {
        commandPendingRef.current = false;
        setCommandPending(false);
      }
    },
    [executeProjectActions, reportEditFailure],
  );

  const beginEditGesture = useCallback(
    (
      event: PointerEvent<HTMLElement>,
      trackId: string,
      item: TimelineCanvasItem,
    ) => {
      if (!model || commandPending || event.button !== 0 || item.kind === "transition") {
        return;
      }
      const track = model.tracks.find((candidate) => candidate.id === trackId);
      if (track?.locked) {
        reportEditFailure(
          new TimelineEditingError(
            `${track.name} is locked. Selection remains available, but timing edits are disabled.`,
          ),
        );
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      const target = { trackId, itemId: item.id };
      const edgeSide = timelineToolUsesEdge(editingTool)
        ? event.clientX <=
          event.currentTarget.getBoundingClientRect().left +
            event.currentTarget.getBoundingClientRect().width / 2
          ? "start"
          : "end"
        : editingSide;
      setActiveEditTarget(target);
      setEditingSide(edgeSide);
      setEditFailure(null);
      scrollRef.current?.setPointerCapture(event.pointerId);

      const rawPointer = rawEditPointerTime(event.clientX);
      const targetStart =
        editingTool === "razor" || editingTool === "slip" || editingTool === "slide"
          ? item.startSeconds
          : edgeSide === "start"
            ? item.startSeconds
            : item.endSeconds;
      const resolved = resolveEditTime(
        editingTool === "razor" ? rawPointer : targetStart,
      );
      let plan: TimelineEditPlan | null = null;
      if (editingTool === "razor") {
        try {
          plan = compileEditGesturePlan(target, edgeSide, resolved.value);
          showEditPlan(plan);
        } catch (error) {
          reportEditFailure(error);
        }
      } else {
        setEditPlan(null);
        setEditMessage(
          `Drag ${item.name} with ${timelineToolLabel(editingTool).toLowerCase()}, then release to apply.`,
        );
      }
      setSnapMatch(resolved.match);
      setEditGesture({
        ...target,
        pointerId: event.pointerId,
        side: edgeSide,
        grabOffsetSeconds:
          editingTool === "razor" ? 0 : rawPointer - targetStart,
        pointerStartClientX: event.clientX,
        dragged: editingTool === "razor",
        plan,
      });
    },
    [
      compileEditGesturePlan,
      commandPending,
      editingSide,
      editingTool,
      model,
      rawEditPointerTime,
      reportEditFailure,
      resolveEditTime,
      showEditPlan,
    ],
  );

  const moveEditGesture = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      if (!editGesture || event.pointerId !== editGesture.pointerId) return;
      event.preventDefault();
      const dragged =
        editGesture.dragged ||
        Math.abs(event.clientX - editGesture.pointerStartClientX) >=
          EDIT_DRAG_THRESHOLD;
      if (!dragged) return;
      const raw = rawEditPointerTime(event.clientX) - editGesture.grabOffsetSeconds;
      const resolved = resolveEditTime(raw);
      try {
        const plan = compileEditGesturePlan(
          editGesture,
          editGesture.side,
          resolved.value,
        );
        setEditGesture((current) =>
          current && current.pointerId === event.pointerId
            ? { ...current, dragged, plan }
            : current,
        );
        showEditPlan(plan);
      } catch (error) {
        setEditGesture((current) =>
          current && current.pointerId === event.pointerId
            ? { ...current, dragged, plan: null }
            : current,
        );
        reportEditFailure(error);
      }
      setSnapMatch(resolved.match);
    },
    [
      compileEditGesturePlan,
      editGesture,
      rawEditPointerTime,
      reportEditFailure,
      resolveEditTime,
      showEditPlan,
    ],
  );

  const endEditGesture = useCallback(
    (event: PointerEvent<HTMLDivElement>) => {
      if (!editGesture || event.pointerId !== editGesture.pointerId) return;
      const dragged =
        editGesture.dragged ||
        Math.abs(event.clientX - editGesture.pointerStartClientX) >=
          EDIT_DRAG_THRESHOLD;
      let plan: TimelineEditPlan | null = null;
      if (dragged) {
        const raw =
          rawEditPointerTime(event.clientX) - editGesture.grabOffsetSeconds;
        const resolved = resolveEditTime(raw);
        try {
          plan = compileEditGesturePlan(
            editGesture,
            editGesture.side,
            resolved.value,
          );
          showEditPlan(plan);
          setSnapMatch(resolved.match);
        } catch (error) {
          reportEditFailure(error);
        }
      }
      if (scrollRef.current?.hasPointerCapture(event.pointerId)) {
        scrollRef.current.releasePointerCapture(event.pointerId);
      }
      setEditGesture(null);
      if (plan) void executeEditPlan(plan);
    },
    [
      compileEditGesturePlan,
      editGesture,
      executeEditPlan,
      rawEditPointerTime,
      reportEditFailure,
      resolveEditTime,
      showEditPlan,
    ],
  );

  const cancelEditGesture = useCallback(() => {
    if (!editGesture) return;
    if (scrollRef.current?.hasPointerCapture(editGesture.pointerId)) {
      scrollRef.current.releasePointerCapture(editGesture.pointerId);
    }
    setEditGesture(null);
    setEditPlan(null);
    setEditFailure(null);
    setSnapMatch(null);
    setEditMessage("Edit gesture cancelled before publication.");
  }, [editGesture]);

  const applyEditAtPlayhead = useCallback(() => {
    if (!activeEditTarget || !activeEditItem || commandPending) return;
    try {
      const plan = compileEditGesturePlan(
        activeEditTarget,
        editingSide,
        playhead,
      );
      showEditPlan(plan);
      void executeEditPlan(plan);
    } catch (error) {
      reportEditFailure(error);
    }
  }, [
    activeEditItem,
    activeEditTarget,
    compileEditGesturePlan,
    commandPending,
    editingSide,
    executeEditPlan,
    playhead,
    reportEditFailure,
    showEditPlan,
  ]);

  const nudgeActiveEdit = useCallback(
    (direction: -1 | 1) => {
      if (!model || !activeEditTarget || !activeEditItem || commandPending) return;
      const frame = timelineFrameDuration(model.editRate);
      const base =
        editingTool === "razor"
          ? playhead
          : editingTool === "slip" || editingTool === "slide"
            ? activeEditItem.startSeconds
            : editingSide === "start"
              ? activeEditItem.startSeconds
              : activeEditItem.endSeconds;
      try {
        const plan = compileEditGesturePlan(
          activeEditTarget,
          editingSide,
          base + direction * frame,
        );
        showEditPlan(plan);
        void executeEditPlan(plan);
      } catch (error) {
        reportEditFailure(error);
      }
    },
    [
      activeEditItem,
      activeEditTarget,
      compileEditGesturePlan,
      commandPending,
      editingSide,
      editingTool,
      executeEditPlan,
      model,
      playhead,
      reportEditFailure,
      showEditPlan,
    ],
  );

  const rippleDeleteRange = useCallback(() => {
    if (!model || !operationTrack || commandPending) return;
    try {
      const plan = compileRippleDelete({
        model,
        trackId: operationTrack.id,
        startSeconds: Math.min(inPoint, outPoint),
        endSeconds: Math.max(inPoint, outPoint),
      });
      showEditPlan(plan);
      void executeEditPlan(plan);
    } catch (error) {
      reportEditFailure(error);
    }
  }, [
    commandPending,
    executeEditPlan,
    inPoint,
    model,
    operationTrack,
    outPoint,
    reportEditFailure,
    showEditPlan,
  ]);

  const insertGapAtPlayhead = useCallback(() => {
    if (!model || !operationTrack || commandPending) return;
    try {
      const plan = compileGapInsert({
        model,
        trackId: operationTrack.id,
        atSeconds: playhead,
        frameCount: gapFrameCount,
      });
      showEditPlan(plan);
      void executeEditPlan(plan);
    } catch (error) {
      reportEditFailure(error);
    }
  }, [
    commandPending,
    executeEditPlan,
    gapFrameCount,
    model,
    operationTrack,
    playhead,
    reportEditFailure,
    showEditPlan,
  ]);

  const closeActiveGap = useCallback(() => {
    if (
      !model ||
      !activeEditTrack ||
      !activeEditItem ||
      activeEditItem.kind !== "gap" ||
      commandPending
    ) {
      return;
    }
    try {
      const plan = compileGapClose({
        model,
        trackId: activeEditTrack.id,
        gapId: activeEditItem.id,
      });
      showEditPlan(plan);
      void executeEditPlan(plan);
    } catch (error) {
      reportEditFailure(error);
    }
  }, [
    activeEditItem,
    activeEditTrack,
    commandPending,
    executeEditPlan,
    model,
    reportEditFailure,
    showEditPlan,
  ]);

  const activateEditTarget = useCallback(
    (trackId: string, item: TimelineCanvasItem) => {
      if (item.kind === "transition") {
        setActiveEditTarget(null);
        setEditPlan(null);
        setEditFailure(null);
        setEditMessage(
          "Transitions are edited through their adjacent timed objects.",
        );
        return;
      }
      setActiveEditTarget({ trackId, itemId: item.id });
      setEditPlan(null);
      setEditFailure(null);
      setEditMessage(
        `${item.name} selected for ${timelineToolLabel(editingTool).toLowerCase()}.`,
      );
    },
    [editingTool],
  );

  const applyGesture = useCallback(
    (kind: TimelineGesture, value: number) => {
      if (!model) return;
      if (kind === "playhead") {
        setPlayhead(value);
        return;
      }
      if (kind === "in") {
        setRangeExplicit(true);
        setInPoint(Math.min(value, outPoint));
        return;
      }
      setRangeExplicit(true);
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
    if (!gesture && !lasso && !editGesture) return;
    const reverseOnEscape = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.preventDefault();
      if (editGesture) cancelEditGesture();
      else cancelGesture();
    };
    window.addEventListener("keydown", reverseOnEscape);
    return () => window.removeEventListener("keydown", reverseOnEscape);
  }, [cancelEditGesture, cancelGesture, editGesture, gesture, lasso]);

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

  const addTrack = useCallback(
    (kind: TimelineTrackKind) => {
      if (!model) return;
      const count = model.tracks.filter((track) => track.kind === kind).length + 1;
      const prefix =
        kind === "video"
          ? "V"
          : kind === "audio"
            ? "A"
            : kind === "caption"
              ? "C"
              : "D";
      void executeTrackMutations(`create:${kind}`, [
        {
          operation: "create",
          timeline_id: model.id,
          track_id: createTrackId(),
          name: `${prefix}${count}`,
          kind,
          position: model.tracks.length,
          height: 72,
        },
      ]);
    },
    [executeTrackMutations, model],
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
  const snapTargetX = snapMatch
    ? (snapMatch.timeSeconds - model.startSeconds) * pixelsPerSecond
    : null;
  const editPreviewX =
    editPlan?.previewSeconds === null || editPlan === null
      ? null
      : (editPlan.previewSeconds - model.startSeconds) * pixelsPerSecond;
  const editAffectedIds = new Set(editPlan?.affectedItemIds ?? []);
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
    editGesture
      ? timelineToolLabel(editingTool)
      : gesture === "playhead"
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
        : gesture || editGesture
          ? `${gestureName} remains frame aligned, with no enabled target in range.`
          : `${model.snapTargets.length} exact targets ready. Drag a timing tool to preview its consequence.`;
  const lassoStyle =
    lasso?.dragged && stageRef.current
      ? lassoStageRectangle(lasso, stageRef.current.getBoundingClientRect())
      : null;
  const activeTargetLabel = activeEditItem
    ? `${activeEditItem.name} on ${activeEditTrack?.name ?? activeEditTarget?.trackId}`
    : operationTrack
      ? `No object selected, operations target ${operationTrack.name}`
      : "No editable track is available";

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
              formatTimelineTime(range.outPoint, model.editRate) +
              (rangeExplicit ? "" : " (not set)")
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
          <div className="timeline-add-track" aria-label="Add timeline track">
            {(["video", "audio", "caption", "data"] as const).map((kind) => (
              <button
                className="secondary timeline-compact-button"
                type="button"
                key={kind}
                disabled={pendingTrackAction !== null}
                title={`Add ${kind} track`}
                onClick={() => addTrack(kind)}
              >
                +{kind[0]?.toUpperCase()}
              </button>
            ))}
          </div>
          <span className="timeline-toolbar-divider" aria-hidden="true" />
          <button
            className="secondary timeline-compact-button"
            type="button"
            onClick={() => {
              setSnapMatch(null);
              setRangeExplicit(true);
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
              setRangeExplicit(true);
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
              setRangeExplicit(true);
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
        className="timeline-edit-controls"
        aria-label="Timeline editing tools"
        data-ready={true}
      >
        <div className="timeline-edit-tool-catalog" role="toolbar">
          {timelineEditingTools.map((tool) => (
            <button
              className="secondary timeline-edit-tool"
              type="button"
              aria-pressed={editingTool === tool.id}
              data-timeline-editing-tool={tool.id}
              disabled={commandPending}
              key={tool.id}
              title={tool.description}
              onClick={() => {
                setEditingTool(tool.id);
                setEditPlan(null);
                setEditFailure(null);
                setEditMessage(tool.description);
              }}
            >
              {tool.label}
            </button>
          ))}
        </div>
        <div className="timeline-edit-modes">
          <span role="group" aria-label="Editing edge">
            {(["start", "end"] as const).map((side) => (
              <button
                className="secondary"
                type="button"
                aria-pressed={editingSide === side}
                disabled={commandPending}
                key={side}
                onClick={() => setEditingSide(side)}
              >
                {side === "start" ? "Start edge" : "End edge"}
              </button>
            ))}
          </span>
          <span role="group" aria-label="Extend behavior">
            {(["ripple", "roll"] as const).map((mode) => (
              <button
                className="secondary"
                type="button"
                aria-pressed={extendMode === mode}
                disabled={editingTool !== "extend" || commandPending}
                key={mode}
                onClick={() => setExtendMode(mode)}
              >
                Extend {mode}
              </button>
            ))}
          </span>
        </div>
        <div className="timeline-edit-actions">
          <button
            className="secondary"
            type="button"
            disabled={!activeEditItem || activeEditLocked || commandPending}
            onClick={applyEditAtPlayhead}
          >
            Apply at playhead
          </button>
          <button
            className="secondary"
            type="button"
            aria-label="Nudge edit backward one frame"
            disabled={!activeEditItem || activeEditLocked || commandPending}
            onClick={() => nudgeActiveEdit(-1)}
          >
            Nudge -1f
          </button>
          <button
            className="secondary"
            type="button"
            aria-label="Nudge edit forward one frame"
            disabled={!activeEditItem || activeEditLocked || commandPending}
            onClick={() => nudgeActiveEdit(1)}
          >
            Nudge +1f
          </button>
          <button
            className="secondary"
            type="button"
            disabled={
              !operationTrack ||
              operationTrackLocked ||
              range.inPoint === range.outPoint ||
              commandPending
            }
            onClick={rippleDeleteRange}
          >
            Ripple delete range
          </button>
          <label>
            <span>Gap frames</span>
            <input
              type="number"
              min="1"
              max="100000"
              step="1"
              value={gapFrameCount}
              disabled={commandPending}
              onChange={(event) => {
                const value = Number(event.currentTarget.value);
                if (Number.isSafeInteger(value) && value > 0) {
                  setGapFrameCount(value);
                }
              }}
            />
          </label>
          <button
            className="secondary"
            type="button"
            disabled={!operationTrack || operationTrackLocked || commandPending}
            onClick={insertGapAtPlayhead}
          >
            Insert gap at playhead
          </button>
          <button
            className="secondary"
            type="button"
            disabled={
              activeEditItem?.kind !== "gap" || activeEditLocked || commandPending
            }
            onClick={closeActiveGap}
          >
            Close gap
          </button>
        </div>
        <div className="timeline-edit-consequence">
          <span>
            <small>Target</small>
            <strong>{activeTargetLabel}</strong>
          </span>
          <output
            className="timeline-edit-status"
            aria-live="polite"
            data-failed={editFailure !== null}
            data-pending={commandPending}
          >
            {commandPending ? "Publishing one atomic edit transaction. " : ""}
            {editMessage}
          </output>
        </div>
      </section>
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
          Control-scroll zooms around the pointer. Choose an edit tool and drag a
          timed item, or use the exact playhead and frame-nudge actions. Edit
          previews publish only when the gesture is released.
        </span>
        <output className="timeline-selection-status" aria-live="polite">
          {visibleSelectionKeys.length === 0
            ? "No timeline items selected"
            : `${visibleSelectionKeys.length} timeline item${
                visibleSelectionKeys.length === 1 ? "" : "s"
              } selected`}
        </output>
      </div>
      <section className="timeline-edit-console" aria-label="Timeline edit controls">
        <header>
          <div>
            <p className="section-kicker">Editorial gestures</p>
            <h5>Exact target, visible consequence</h5>
          </div>
          <div className="timeline-history-actions">
            <button
              type="button"
              className="secondary timeline-compact-button"
              disabled={commandPending || snapshot.project.undo_depth === 0}
              title="Command or Control Z"
              onClick={() => void executeHistory("undo")}
            >
              Undo {snapshot.project.undo_depth}
            </button>
            <button
              type="button"
              className="secondary timeline-compact-button"
              disabled={commandPending || snapshot.project.redo_depth === 0}
              title="Command or Control Shift Z"
              onClick={() => void executeHistory("redo")}
            >
              Redo {snapshot.project.redo_depth}
            </button>
          </div>
        </header>
        <div className="timeline-edit-actions">
          {TIMELINE_EDIT_GESTURES.map((edit) => (
            <button
              type="button"
              key={edit}
              disabled={commandPending}
              aria-pressed={selectedEdit === edit}
              data-ready={
                selectedEdit === edit && gestureCommandPlan?.status === "ready"
              }
              onFocus={() => setSelectedEdit(edit)}
              onPointerEnter={() => setSelectedEdit(edit)}
              onClick={() => void executeEdit(edit)}
            >
              {timelineEditGestureLabel(edit)}
            </button>
          ))}
        </div>
        <label
          className="timeline-point-edit-mode"
          data-active={selectedEdit === "three_point"}
        >
          <span>
            <strong>Three-point rule</strong>
            <small>
              {TIMELINE_THREE_POINT_MODES.find(
                (candidate) => candidate.value === threePointMode,
              )?.detail}
            </small>
          </span>
          <select
            aria-label="Three-point edit rule"
            value={threePointMode}
            disabled={commandPending}
            onChange={(event) =>
              setThreePointMode(event.target.value as TimelineThreePointMode)
            }
          >
            {TIMELINE_THREE_POINT_MODES.map((mode) => (
              <option key={mode.value} value={mode.value}>
                {mode.label}
              </option>
            ))}
          </select>
        </label>
        <dl className="timeline-edit-state">
          <div>
            <dt>Target</dt>
            <dd>{gestureCommandPlan?.target ?? "No target track"}</dd>
          </div>
          <div>
            <dt>Source</dt>
            <dd>
              {sourceProjection?.status === "ready"
                ? formatTimelineEditSource(sourceProjection.source)
                : sourceProjection?.reason ?? "No source state"}
            </dd>
          </div>
          <div>
            <dt>Source engine</dt>
            <dd>{sourceMonitor?.engine_state ?? "empty"}</dd>
          </div>
          <div>
            <dt>Consequence</dt>
            <dd>
              {gestureCommandPlan?.status === "ready"
                ? gestureCommandPlan.consequence
                : gestureCommandPlan?.reason ?? "Choose an edit gesture."}
            </dd>
          </div>
        </dl>
        <output
          className="timeline-command-status"
          data-pending={commandPending}
          aria-live="polite"
        >
          {commandStatus}
        </output>
        <p className="timeline-edit-shortcuts">
          Three-point uses source marks with the playhead or timeline range. Four-point
          uses source in and out plus timeline in and out. Backspace extracts the
          selected item or active range. Command or Control Z reverses immediately,
          and Shift adds redo.
        </p>
      </section>
      {selectedTransition ? (
        <TimelineTransitionInspector
          executeProjectActions={executeProjectActions}
          key={selectedTransition.id}
          transition={selectedTransition}
        />
      ) : null}
      {clipProjection?.status === "unavailable" ? (
        <p className="timeline-clip-detail-failure" role="alert">
          {clipProjection.reason}
        </p>
      ) : null}
      {trackFailure ? (
        <p className="timeline-command-failure" role="alert">
          {trackFailure}
        </p>
      ) : null}
      <div
        className={
          `timeline-scroll${gesture ? " timeline-scroll-gesturing" : ""}` +
          (lasso ? " timeline-scroll-selecting" : "") +
          (editGesture ? " timeline-scroll-editing" : "")
        }
        ref={scrollRef}
        onScroll={(event) => setScrollLeft(event.currentTarget.scrollLeft)}
        onPointerMove={(event) => {
          if (editGesture) moveEditGesture(event);
          else moveGesture(event);
        }}
        onPointerUp={(event) => {
          if (editGesture) endEditGesture(event);
          else endGesture(event);
        }}
        onPointerCancel={(event) => {
          if (editGesture) cancelEditGesture();
          else cancelGesture(event);
        }}
        onLostPointerCapture={(event) => {
          if (!editGesture) cancelGesture(event);
        }}
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
              <TimelineTrackHeader
                track={track}
                timelineId={model.id}
                canonicalPosition={model.tracks.findIndex(
                  (candidate) => candidate.id === track.id,
                )}
                trackCount={model.tracks.length}
                pending={pendingTrackAction !== null || commandPending}
                execute={executeTrackMutations}
                editTarget={targetTrackId === track.id}
                onSelectEditTarget={() => setTargetTrackId(track.id)}
              />
              <div
                className={
                  `timeline-lane timeline-lane-${track.kind}` +
                  (track.enabled ? "" : " timeline-lane-disabled")
                }
                data-track-id={track.id}
                data-targeted={track.targeted}
                data-locked={track.locked}
                data-sync-locked={track.syncLocked}
                data-muted={track.muted}
                data-solo={track.solo}
                data-enabled={track.enabled}
                style={{ height: track.height }}
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
                        editingActive={
                          activeEditTarget?.trackId === track.id &&
                          activeEditTarget.itemId === item.id
                        }
                        editingAffected={editAffectedIds.has(item.id)}
                        item={item}
                        itemRef={itemRef(key)}
                        key={key}
                        model={model}
                        onFocus={() => {
                          setFocusedKey(key);
                          activateEditTarget(track.id, item);
                        }}
                        onKeyDown={(event) => itemKeyDown(event, key)}
                        onPointerDown={(event) => {
                          setTargetTrackId(track.id);
                          beginSelection(event, key);
                          if (
                            !event.shiftKey &&
                            !event.altKey &&
                            !event.metaKey &&
                            !event.ctrlKey
                          ) {
                            beginEditGesture(event, track.id, item);
                          }
                        }}
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
          {editPlan && editPreviewX !== null ? (
            <div
              className="timeline-edit-preview"
              style={{ left: HEADER_WIDTH + editPreviewX }}
              aria-hidden="true"
            >
              <span>
                {editPlan.label}
                <b>
                  {editPlan.operations.length} atomic {editPlan.operations.length === 1
                    ? "operation"
                    : "operations"}
                </b>
              </span>
            </div>
          ) : null}
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

function TimelineTrackHeader({
  track,
  timelineId,
  canonicalPosition,
  trackCount,
  pending,
  execute,
  editTarget,
  onSelectEditTarget,
}: {
  readonly track: TimelineCanvasTrack;
  readonly timelineId: string;
  readonly canonicalPosition: number;
  readonly trackCount: number;
  readonly pending: boolean;
  readonly execute: (
    identity: string,
    mutations: readonly TimelineTrackMutation[],
  ) => Promise<void>;
  readonly editTarget: boolean;
  readonly onSelectEditTarget: () => void;
}) {
  const [name, setName] = useState(track.name);
  const [deleteArmed, setDeleteArmed] = useState(false);
  const cancelNameCommitRef = useRef(false);
  useEffect(() => setName(track.name), [track.name]);

  const mutate = (suffix: string, mutation: TimelineTrackMutation) => {
    void execute(`${track.id}:${suffix}`, [mutation]);
  };
  const commitName = (_event?: FocusEvent<HTMLInputElement>) => {
    if (cancelNameCommitRef.current) {
      cancelNameCommitRef.current = false;
      setName(track.name);
      return;
    }
    const next = name.trim();
    if (next.length === 0) {
      setName(track.name);
      return;
    }
    if (next !== track.name) {
      mutate("rename", {
        operation: "rename",
        timeline_id: timelineId,
        track_id: track.id,
        name: next,
      });
      setName(track.name);
    }
  };
  const resize = (delta: number) => {
    const height = clampNumber(
      track.height + delta,
      MIN_TRACK_HEIGHT,
      MAX_TRACK_HEIGHT,
    );
    if (height !== track.height) {
      mutate("height", {
        operation: "set_height",
        timeline_id: timelineId,
        track_id: track.id,
        height,
      });
    }
  };

  return (
    <header
      className="timeline-track-header"
      style={{ height: track.height }}
      data-track-id={track.id}
    >
      <div className="timeline-track-identity">
        <span>{track.kind}</span>
        <input
          aria-label={`Track name ${track.name}`}
          value={name}
          disabled={pending}
          onChange={(event) => {
            cancelNameCommitRef.current = false;
            setName(event.currentTarget.value);
          }}
          onBlur={commitName}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              event.preventDefault();
              event.currentTarget.blur();
            } else if (event.key === "Escape") {
              event.preventDefault();
              cancelNameCommitRef.current = true;
              setName(track.name);
              event.currentTarget.blur();
            }
          }}
        />
        <code title={track.id}>{track.id}</code>
      </div>
      <div className="timeline-track-controls">
        <button
          type="button"
          aria-label={`Use ${track.name} as the edit target`}
          aria-pressed={editTarget}
          title="Use as edit target"
          disabled={pending}
          data-active={editTarget}
          onClick={onSelectEditTarget}
        >
          Edit
        </button>
        <button
          type="button"
          aria-label={`Move ${track.name} up`}
          title="Move track up"
          disabled={pending || canonicalPosition >= trackCount - 1}
          onClick={() =>
            mutate("up", {
              operation: "reorder",
              timeline_id: timelineId,
              track_id: track.id,
              position: canonicalPosition + 1,
            })
          }
        >
          U
        </button>
        <button
          type="button"
          aria-label={`Move ${track.name} down`}
          title="Move track down"
          disabled={pending || canonicalPosition <= 0}
          onClick={() =>
            mutate("down", {
              operation: "reorder",
              timeline_id: timelineId,
              track_id: track.id,
              position: canonicalPosition - 1,
            })
          }
        >
          D
        </button>
        <button
          type="button"
          aria-label={`Decrease ${track.name} height`}
          title="Decrease track height"
          disabled={pending || track.height <= MIN_TRACK_HEIGHT}
          onClick={() => resize(-8)}
        >
          -
        </button>
        <button
          type="button"
          aria-label={`Increase ${track.name} height`}
          title="Increase track height"
          disabled={pending || track.height >= MAX_TRACK_HEIGHT}
          onClick={() => resize(8)}
        >
          +
        </button>
        <TrackToggle
          label="T"
          title="Target track"
          pressed={track.targeted}
          disabled={pending}
          onClick={() =>
            mutate("target", {
              operation: "set_targeted",
              timeline_id: timelineId,
              track_id: track.id,
              targeted: !track.targeted,
            })
          }
        />
        <TrackToggle
          label="L"
          title="Lock track edits"
          pressed={track.locked}
          disabled={pending}
          onClick={() =>
            mutate("lock", {
              operation: "set_locked",
              timeline_id: timelineId,
              track_id: track.id,
              locked: !track.locked,
            })
          }
        />
        <TrackToggle
          label="Y"
          title="Sync lock track"
          pressed={track.syncLocked}
          disabled={pending}
          onClick={() =>
            mutate("sync", {
              operation: "set_sync_locked",
              timeline_id: timelineId,
              track_id: track.id,
              sync_locked: !track.syncLocked,
            })
          }
        />
        {track.kind === "audio" ? (
          <>
            <TrackToggle
              label="M"
              title="Mute track"
              pressed={track.muted}
              disabled={pending}
              onClick={() =>
                mutate("mute", {
                  operation: "set_muted",
                  timeline_id: timelineId,
                  track_id: track.id,
                  muted: !track.muted,
                })
              }
            />
            <TrackToggle
              label="S"
              title="Solo track"
              pressed={track.solo}
              disabled={pending}
              onClick={() =>
                mutate("solo", {
                  operation: "set_solo",
                  timeline_id: timelineId,
                  track_id: track.id,
                  solo: !track.solo,
                })
              }
            />
          </>
        ) : null}
        <TrackToggle
          label="E"
          title="Enable track output"
          pressed={track.enabled}
          disabled={pending}
          onClick={() =>
            mutate("enable", {
              operation: "set_enabled",
              timeline_id: timelineId,
              track_id: track.id,
              enabled: !track.enabled,
            })
          }
        />
        <button
          type="button"
          className={deleteArmed ? "timeline-track-delete-armed" : ""}
          aria-label={
            deleteArmed
              ? `Confirm delete ${track.name}`
              : `Delete ${track.name}`
          }
          title={deleteArmed ? "Confirm track deletion" : "Delete track"}
          disabled={pending || track.locked}
          onBlur={() => setDeleteArmed(false)}
          onClick={() => {
            if (!deleteArmed) {
              setDeleteArmed(true);
              return;
            }
            setDeleteArmed(false);
            mutate("delete", {
              operation: "delete",
              timeline_id: timelineId,
              track_id: track.id,
            });
          }}
        >
          {deleteArmed ? "?" : "X"}
        </button>
      </div>
    </header>
  );
}

function TrackToggle({
  label,
  title,
  pressed,
  disabled,
  onClick,
}: {
  readonly label: string;
  readonly title: string;
  readonly pressed: boolean;
  readonly disabled: boolean;
  readonly onClick: () => void;
}) {
  return (
    <button
      type="button"
      aria-label={title}
      aria-pressed={pressed}
      title={title}
      disabled={disabled}
      data-active={pressed}
      onClick={onClick}
    >
      {label}
    </button>
  );
}

function preferredTargetTrackId(model: TimelineCanvasModel | null): string {
  if (!model) return "";
  return (
    model.tracks.find(
      (track) =>
        track.targeted && (track.kind === "video" || track.kind === "audio"),
    ) ??
    model.tracks.find(
      (track) => track.kind === "video" || track.kind === "audio",
    ) ??
    model.tracks[0]
  )?.id ?? "";
}

function editGestureUsesSource(edit: TimelineEditGesture): boolean {
  return (
    edit === "insert" ||
    edit === "overwrite" ||
    edit === "append" ||
    edit === "replace" ||
    edit === "three_point" ||
    edit === "four_point"
  );
}

function timelineEditGestureLabel(edit: TimelineEditGesture): string {
  if (edit === "three_point") return "Three-point";
  if (edit === "four_point") return "Four-point";
  return capitalize(edit);
}

function randomEditorialId(
  kind: "clip" | "gap" | "generator" | "caption",
): string {
  let identity = randomHex(16);
  while (/^0+$/.test(identity)) identity = randomHex(16);
  return `${kind}:${identity}`;
}

function randomHex(byteLength: number): string {
  if (!globalThis.crypto?.getRandomValues) {
    throw new Error("Secure editorial identity generation is unavailable.");
  }
  const bytes = new Uint8Array(byteLength);
  globalThis.crypto.getRandomValues(bytes);
  return Array.from(bytes, (value) => value.toString(16).padStart(2, "0")).join("");
}

function formatTimelineEditSource(
  source: TimelineEditSource,
): string {
  const range = source.sourceRange;
  const marks =
    source.sourceIn && source.sourceOutExclusive
      ? `, marks ${source.sourceIn.value} to ${source.sourceOutExclusive.value - 1}`
      : source.sourceIn
        ? `, source in ${source.sourceIn.value}`
        : source.sourceOutExclusive
          ? `, source out ${source.sourceOutExclusive.value - 1}`
          : ", no explicit source marks";
  return (
    `${source.mediaName} (${source.streamKind}) from ${range.start.value} for ` +
    `${range.duration.value} units at ${range.start.timebase.numerator}/` +
    `${range.start.timebase.denominator}${marks}`
  );
}

function timelineCommandFailure(error: unknown): string {
  if (error instanceof Error) return error.message;
  if (typeof error === "object" && error !== null) {
    if ("title" in error) return String(error.title);
    if ("message" in error) return String(error.message);
  }
  return "The timeline command could not be completed.";
}

function isEditableTimelineTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.isContentEditable ||
    target.tagName === "INPUT" ||
    target.tagName === "TEXTAREA" ||
    target.tagName === "SELECT"
  );
}

function capitalize(value: string): string {
  return value.length === 0 ? value : value[0].toUpperCase() + value.slice(1);
}

function TimelineItem({
  authoredSelected,
  detail,
  focused,
  interactionSelected,
  editingActive,
  editingAffected,
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
  readonly editingActive: boolean;
  readonly editingAffected: boolean;
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
    editingActive ? "editing target" : null,
    editingAffected ? "edit consequence" : null,
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
    (interactionSelected ? " timeline-item-selected" : "") +
    (editingActive ? " timeline-item-edit-active" : "") +
    (editingAffected ? " timeline-item-edit-affected" : "");
  const data = {
    "data-item-id": item.id,
    "data-item-kind": item.kind,
    "data-record-start": item.recordRange.start.value,
    "data-record-duration": item.recordRange.duration.value,
    "data-source-id": item.source?.id,
    "data-grouped": item.group ? "true" : "false",
    "data-linked": item.link ? "true" : "false",
    "data-selection-key": selectionKey,
    "data-editing-active": editingActive ? "true" : "false",
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
      {item.transition ? (
        <span className="timeline-transition-handles" aria-hidden="true">
          <i />
          <span>
            {item.transition.fromOffset.value}/{item.transition.toOffset.value}
          </span>
          <i />
        </span>
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

type ExecuteProjectActions = TimelineWorkspaceProps["executeProjectActions"];

function TimelineTransitionInspector({
  executeProjectActions,
  transition,
}: {
  readonly executeProjectActions: ExecuteProjectActions;
  readonly transition: TimelineTransitionPresentation;
}) {
  const [fromValue, setFromValue] = useState(transition.fromOffset.value);
  const [toValue, setToValue] = useState(transition.toOffset.value);
  const [durationValue, setDurationValue] = useState(transition.duration.value);
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    setFromValue(transition.fromOffset.value);
    setToValue(transition.toOffset.value);
    setDurationValue(transition.duration.value);
    setPending(false);
    setMessage(null);
  }, [
    transition.id,
    transition.projectRevision,
    transition.fromOffset.value,
    transition.toOffset.value,
    transition.duration.value,
  ]);

  const commitTiming = useCallback(
    async (nextFrom: string, nextTo: string) => {
      let action: ProjectAction;
      try {
        action = buildSetTransitionAction(transition, nextFrom, nextTo);
      } catch (error: unknown) {
        setMessage(
          error instanceof Error
            ? error.message
            : "Transition timing is not valid.",
        );
        return;
      }
      setPending(true);
      setMessage("Applying exact transition timing...");
      try {
        const result = await executeProjectActions([action]);
        setMessage(
          `Transition timing published at project revision ${result.state.project_revision}.`,
        );
      } catch (error: unknown) {
        setMessage(
          error instanceof Error
            ? error.message
            : "Transition timing could not be published.",
        );
      } finally {
        setPending(false);
      }
    },
    [executeProjectActions, transition],
  );

  const updateHandle = (side: "from" | "to", value: string) => {
    const nextFrom = side === "from" ? value : fromValue;
    const nextTo = side === "to" ? value : toValue;
    if (side === "from") setFromValue(value);
    else setToValue(value);
    const total = sumExactHandleValues(nextFrom, nextTo);
    if (total !== null) setDurationValue(total);
    setMessage(null);
  };

  const updateDuration = (value: string) => {
    setDurationValue(value);
    if (!/^(0|[1-9][0-9]*)$/.test(value)) {
      setMessage(null);
      return;
    }
    try {
      const handles = transitionHandlesForDuration(transition, value);
      if (handles === null) {
        setMessage("That duration does not fit the available adjacent handles.");
        return;
      }
      setFromValue(handles.fromOffsetValue);
      setToValue(handles.toOffsetValue);
      setMessage(null);
    } catch (error: unknown) {
      setMessage(error instanceof Error ? error.message : "Duration is not valid.");
    }
  };

  const applyAlignment = async (
    alignment: Exclude<TimelineTransitionAlignment, "custom">,
  ) => {
    try {
      const handles = transitionHandlesForAlignment(
        transition,
        alignment,
        durationValue,
      );
      if (handles === null) {
        setMessage(`The current duration cannot use ${alignment} alignment.`);
        return;
      }
      setFromValue(handles.fromOffsetValue);
      setToValue(handles.toOffsetValue);
      await commitTiming(handles.fromOffsetValue, handles.toOffsetValue);
    } catch (error: unknown) {
      setMessage(error instanceof Error ? error.message : "Alignment is not valid.");
    }
  };

  const alignmentAvailable = (
    alignment: Exclude<TimelineTransitionAlignment, "custom">,
  ): boolean => {
    try {
      return (
        transitionHandlesForAlignment(transition, alignment, durationValue) !==
        null
      );
    } catch {
      return false;
    }
  };

  return (
    <section
      className="timeline-transition-inspector"
      aria-label={`Transition editor for ${transition.name}`}
    >
      <header>
        <div>
          <p className="section-kicker">Selected transition</p>
          <h5>{transition.name}</h5>
          <span>
            {transition.from.kind}:{transition.from.id} to {transition.to.kind}:
            {transition.to.id}
          </span>
        </div>
        <div className="timeline-transition-summary">
          <b>{transition.alignment} aligned</b>
          <code>{transition.id}</code>
        </div>
      </header>
      <form
        className="timeline-transition-timing"
        onSubmit={(event) => {
          event.preventDefault();
          void commitTiming(fromValue, toValue);
        }}
      >
        <label>
          <span>From handle</span>
          <input
            aria-label="From handle"
            disabled={pending}
            inputMode="numeric"
            maxLength={20}
            onChange={(event) => updateHandle("from", event.currentTarget.value)}
            value={fromValue}
          />
          <small>max {transition.maximumFromOffset.value}</small>
        </label>
        <label>
          <span>To handle</span>
          <input
            aria-label="To handle"
            disabled={pending}
            inputMode="numeric"
            maxLength={20}
            onChange={(event) => updateHandle("to", event.currentTarget.value)}
            value={toValue}
          />
          <small>max {transition.maximumToOffset.value}</small>
        </label>
        <label>
          <span>Transition duration</span>
          <input
            aria-label="Transition duration"
            disabled={pending}
            inputMode="numeric"
            maxLength={20}
            onChange={(event) => updateDuration(event.currentTarget.value)}
            value={durationValue}
          />
          <small>
            units at {transition.duration.timebase.numerator}/
            {transition.duration.timebase.denominator}
          </small>
        </label>
        <div
          className="timeline-transition-alignment"
          role="group"
          aria-label="Transition alignment"
        >
          {(["start", "center", "end"] as const).map((alignment) => (
            <button
              className="secondary"
              type="button"
              aria-pressed={transition.alignment === alignment}
              disabled={pending || !alignmentAvailable(alignment)}
              key={alignment}
              onClick={() => void applyAlignment(alignment)}
            >
              {capitalize(alignment)}
            </button>
          ))}
        </div>
        <button className="primary" disabled={pending} type="submit">
          {pending ? "Applying..." : "Apply timing"}
        </button>
      </form>
      <output className="timeline-transition-status" aria-live="polite">
        {message ??
          "Exact handles are canonical timeline state. Alignment applies immediately."}
      </output>
      <section className="timeline-transition-parameters">
        <header>
          <div>
            <p className="section-kicker">Editable parameters</p>
            <h5>Visual and processing intent</h5>
          </div>
          {transition.graph.status === "ready" ? (
            <code>
              {transition.graph.graphId} r{transition.graph.graphRevision}
            </code>
          ) : null}
        </header>
        {transition.graph.status === "unavailable" ? (
          <p className="timeline-transition-parameter-warning" role="status">
            {transition.graph.reason}
          </p>
        ) : transition.graph.effects.length === 0 ? (
          <p className="timeline-transition-parameter-warning" role="status">
            No downstream visual transition node is attached. Exact timing remains
            editable.
          </p>
        ) : (
          transition.graph.effects.map((effect) => (
            <article className="timeline-transition-effect" key={effect.nodeId}>
              <header>
                <strong>{effect.label}</strong>
                <code>{effect.nodeType}</code>
              </header>
              <div className="timeline-transition-parameter-grid">
                {effect.parameters.map((parameter) => (
                  <TimelineTransitionParameterControl
                    executeProjectActions={executeProjectActions}
                    key={parameter.parameterId}
                    parameter={parameter}
                  />
                ))}
              </div>
            </article>
          ))
        )}
      </section>
    </section>
  );
}

function TimelineTransitionParameterControl({
  executeProjectActions,
  parameter,
}: {
  readonly executeProjectActions: ExecuteProjectActions;
  readonly parameter: TimelineTransitionParameterPresentation;
}) {
  const [value, setValue] = useState(String(parameter.value ?? ""));
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  useEffect(() => {
    setValue(String(parameter.value ?? ""));
    setPending(false);
    setMessage(null);
  }, [parameter.parameterId, parameter.value, parameter.driven]);

  const commit = async (nextValue: string | boolean) => {
    let action: ProjectAction;
    try {
      action = buildTransitionParameterAction(parameter, nextValue);
    } catch (error: unknown) {
      setMessage(error instanceof Error ? error.message : "Parameter is not valid.");
      return;
    }
    setPending(true);
    setMessage("Applying...");
    try {
      const result = await executeProjectActions([action]);
      setMessage(`Published at revision ${result.state.project_revision}.`);
    } catch (error: unknown) {
      setMessage(
        error instanceof Error
          ? error.message
          : "The parameter could not be published.",
      );
    } finally {
      setPending(false);
    }
  };

  const restriction = parameter.restriction
    ? parameter.restriction.replace("_", " ")
    : parameter.animatable
      ? "animatable"
      : "static";
  if (!parameter.editable) {
    return (
      <div className="timeline-transition-parameter" data-editable="false">
        <span>{parameter.label}</span>
        <output>{String(parameter.value ?? "unavailable")}</output>
        <small>{restriction}</small>
      </div>
    );
  }

  if (parameter.kind === "boolean") {
    const checked = value === "true";
    return (
      <label className="timeline-transition-parameter" data-editable="true">
        <span>{parameter.label}</span>
        <input
          checked={checked}
          disabled={pending}
          onChange={(event) => {
            const next = event.currentTarget.checked;
            setValue(String(next));
            void commit(next);
          }}
          type="checkbox"
        />
        <small>{message ?? restriction}</small>
      </label>
    );
  }

  if (parameter.kind === "choice" && parameter.choices.length > 0) {
    return (
      <label className="timeline-transition-parameter" data-editable="true">
        <span>{parameter.label}</span>
        <select
          disabled={pending}
          onChange={(event) => {
            const next = event.currentTarget.value;
            setValue(next);
            void commit(next);
          }}
          value={value}
        >
          {parameter.choices.map((choice) => (
            <option key={choice} value={choice}>
              {choice}
            </option>
          ))}
        </select>
        <small>{message ?? restriction}</small>
      </label>
    );
  }

  return (
    <form
      className="timeline-transition-parameter"
      data-editable="true"
      onSubmit={(event) => {
        event.preventDefault();
        void commit(value);
      }}
    >
      <label>
        <span>{parameter.label}</span>
        <input
          disabled={pending}
          onChange={(event) => setValue(event.currentTarget.value)}
          step={parameter.kind === "scalar" ? "any" : undefined}
          type={parameter.kind === "scalar" ? "number" : "text"}
          value={value}
        />
      </label>
      <button className="secondary" disabled={pending} type="submit">
        Apply
      </button>
      <small>{message ?? restriction}</small>
    </form>
  );
}

function sumExactHandleValues(from: string, to: string): string | null {
  if (!/^(0|[1-9][0-9]*)$/.test(from) || !/^(0|[1-9][0-9]*)$/.test(to)) {
    return null;
  }
  return (BigInt(from) + BigInt(to)).toString();
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

function timelineToolUsesEdge(tool: TimelineEditingTool): boolean {
  return tool === "ripple" || tool === "roll" || tool === "trim" || tool === "extend";
}

function timelineToolLabel(tool: TimelineEditingTool): string {
  return timelineEditingTools.find((candidate) => candidate.id === tool)?.label ?? tool;
}

function timelineEditErrorMessage(error: unknown): string {
  if (error instanceof TimelineEditingError) return error.message;
  if (error instanceof Error && error.message.length > 0) return error.message;
  return "The timeline edit could not be completed.";
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

function createTrackId(): string {
  const value = globalThis.crypto.randomUUID().replaceAll("-", "");
  return `track:${value}`;
}
