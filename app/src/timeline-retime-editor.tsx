import {
  useEffect,
  useMemo,
  useState,
  type KeyboardEvent,
} from "react";

import type { TimelineClipPresentation } from "./timeline-clip-presentation.ts";
import {
  planTimelineRetime,
  removeTimelineRetimePoint,
  splitTimelineRetimeDraft,
  timelineRetimeDraftForMode,
  timelineRetimeDraftFromClip,
  timelineRetimePlayheadOffset,
  type TimelineRetimeDraft,
  type TimelineRetimeMode,
  type TimelineRetimeReadyPlan,
} from "./timeline-retime.ts";

const MODES: readonly {
  readonly mode: TimelineRetimeMode;
  readonly label: string;
}[] = [
  { mode: "speed", label: "Speed" },
  { mode: "reverse", label: "Reverse" },
  { mode: "freeze", label: "Freeze" },
  { mode: "time_remap", label: "Time remap" },
  { mode: "identity", label: "Normal" },
];

export interface TimelineRetimeEditorProps {
  readonly clip: TimelineClipPresentation | null;
  readonly selectionCount: number;
  readonly playheadSeconds: number;
  readonly projectRevision: number;
  readonly busy: boolean;
  readonly undoDepth: number;
  readonly status: string;
  readonly makeTransactionId: () => string;
  readonly onApply: (plan: TimelineRetimeReadyPlan) => void;
  readonly onUndo: () => void;
}

export function TimelineRetimeEditor({
  clip,
  selectionCount,
  playheadSeconds,
  projectRevision,
  busy,
  undoDepth,
  status,
  makeTransactionId,
  onApply,
  onUndo,
}: TimelineRetimeEditorProps) {
  const [draft, setDraft] = useState<TimelineRetimeDraft | null>(() =>
    clip ? timelineRetimeDraftFromClip(clip) : null,
  );
  const [draftStatus, setDraftStatus] = useState(
    "Choose a timing mode or edit an exact rate.",
  );

  useEffect(() => {
    setDraft(clip ? timelineRetimeDraftFromClip(clip) : null);
    setDraftStatus(
      clip
        ? "Authored timing loaded. Escape resets every unsubmitted change."
        : "Select one clip directly to author its timing.",
    );
  }, [clip, projectRevision]);

  const preview = useMemo(
    () =>
      clip && draft
        ? planTimelineRetime({
            clip,
            draft,
            projectRevision,
            transactionId: "superi.desktop.timeline.retime.preview",
          })
        : null,
    [clip, draft, projectRevision],
  );
  const playheadOffset = useMemo(
    () =>
      clip
        ? timelineRetimePlayheadOffset(clip, playheadSeconds)
        : null,
    [clip, playheadSeconds],
  );

  if (!clip || !draft || !preview) {
    const reason =
      selectionCount === 0
        ? "Select one clip to expose its exact retime target."
        : selectionCount === 1
          ? "The selected timeline object is not a source-bearing clip."
          : "Select one clip directly. Linked or range selections have more than one target.";
    return (
      <section
        className="timeline-retime-editor timeline-retime-editor-disabled"
        aria-label="Clip retime controls"
      >
        <header>
          <div>
            <p className="section-kicker">Time remapping</p>
            <h5>Speed, reverse, freeze, and curves</h5>
          </div>
          <span data-state="disabled">No exact clip target</span>
        </header>
        <p>{reason}</p>
      </section>
    );
  }

  const chooseMode = (mode: TimelineRetimeMode) => {
    try {
      setDraft(
        timelineRetimeDraftForMode(
          clip,
          draft,
          mode,
          mode === "time_remap" ? playheadOffset ?? undefined : undefined,
        ),
      );
      setDraftStatus(
        `${modeLabel(mode)} draft ready. Review the exact consequence before applying.`,
      );
    } catch (error: unknown) {
      setDraftStatus(retimeFailure(error));
    }
  };

  const updateSegment = (
    index: number,
    field: "recordDuration" | "rateNumerator" | "rateDenominator",
    value: string,
  ) => {
    setDraft({
      ...draft,
      segments: draft.segments.map((segment, candidate) =>
        candidate === index ? { ...segment, [field]: value } : segment,
      ),
    });
    setDraftStatus("Draft changed. Validation and curve preview update immediately.");
  };

  const addPoint = () => {
    if (playheadOffset === null) {
      setDraftStatus(
        "Move the playhead inside the selected clip before adding a curve point.",
      );
      return;
    }
    try {
      setDraft({
        ...splitTimelineRetimeDraft(draft, playheadOffset),
        mode: "time_remap",
      });
      setDraftStatus(
        `Added an exact curve point at local record offset ${playheadOffset}.`,
      );
    } catch (error: unknown) {
      setDraftStatus(retimeFailure(error));
    }
  };

  const removePoint = (boundaryIndex: number) => {
    try {
      setDraft(removeTimelineRetimePoint(draft, boundaryIndex));
      setDraftStatus(
        "Removed the curve point and retained the preceding segment rate.",
      );
    } catch (error: unknown) {
      setDraftStatus(retimeFailure(error));
    }
  };

  const resetDraft = () => {
    setDraft(timelineRetimeDraftFromClip(clip));
    setDraftStatus("Unsubmitted retime changes were reversed.");
  };

  const handleKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.key !== "Escape" || busy) return;
    event.preventDefault();
    event.stopPropagation();
    resetDraft();
  };

  const apply = () => {
    let transactionId: string;
    try {
      transactionId = makeTransactionId();
    } catch (error: unknown) {
      setDraftStatus(retimeFailure(error));
      return;
    }
    const plan = planTimelineRetime({
      clip,
      draft,
      projectRevision,
      transactionId,
    });
    if (plan.status === "disabled") {
      setDraftStatus(plan.reason);
      return;
    }
    onApply(plan);
  };

  const points = preview.status === "ready" ? preview.curvePoints : [];
  const polyline = points
    .map((point) => `${point.xPercent},${point.yPercent}`)
    .join(" ");

  return (
    <section
      className="timeline-retime-editor"
      aria-label="Clip retime controls"
      data-ready={preview.status === "ready"}
      onKeyDown={handleKeyDown}
    >
      <header>
        <div>
          <p className="section-kicker">Time remapping</p>
          <h5>Speed, reverse, freeze, and curves</h5>
        </div>
        <span data-state={preview.status}>
          {preview.status === "ready" ? "Ready to apply" : "Draft needs attention"}
        </span>
      </header>

      <dl className="timeline-retime-target">
        <div>
          <dt>Exact clip</dt>
          <dd>{clip.name} ({clip.id})</dd>
        </div>
        <div>
          <dt>Exact track</dt>
          <dd>{clip.trackName} ({clip.trackId})</dd>
        </div>
        <div>
          <dt>Record</dt>
          <dd>
            {clip.recordRange.start.value} + {clip.recordRange.duration.value} at{" "}
            {clip.recordRange.duration.timebase.numerator}/
            {clip.recordRange.duration.timebase.denominator}
          </dd>
        </div>
        <div>
          <dt>Authored state</dt>
          <dd>
            {modeLabel(preview.currentMode)}, {clip.timeMap.segments.length}{" "}
            {clip.timeMap.segments.length === 1 ? "segment" : "segments"}
          </dd>
        </div>
      </dl>

      <div
        className="timeline-retime-modes"
        role="group"
        aria-label="Retime mode"
      >
        {MODES.map(({ mode, label }) => (
          <button
            className="secondary"
            type="button"
            aria-pressed={draft.mode === mode}
            disabled={busy}
            key={mode}
            onClick={() => chooseMode(mode)}
          >
            {label}
          </button>
        ))}
      </div>

      <label className="timeline-retime-anchor">
        <span>Source anchor</span>
        <input
          aria-label="Exact retime source anchor"
          disabled={busy}
          inputMode="numeric"
          spellCheck={false}
          value={draft.sourceStart}
          onChange={(event) => {
            setDraft({ ...draft, sourceStart: event.currentTarget.value });
            setDraftStatus("Source anchor changed. Review the derived traversal.");
          }}
        />
        <small>
          Exact units at {clip.timeMap.sourceTimebase.numerator}/
          {clip.timeMap.sourceTimebase.denominator}
        </small>
      </label>

      <div className="timeline-retime-curve-layout">
        <div className="timeline-retime-segments">
          <div className="timeline-retime-segment-heading">
            <span>Exact segments</span>
            <button
              className="secondary timeline-compact-button"
              type="button"
              disabled={busy}
              onClick={addPoint}
            >
              Add point at playhead
            </button>
          </div>
          {draft.segments.map((segment, index) => (
            <div className="timeline-retime-segment" key={index}>
              {index > 0 ? (
                <button
                  className="secondary timeline-retime-remove-point"
                  type="button"
                  disabled={busy}
                  aria-label={`Remove retime point ${index}`}
                  title="Merge with the preceding segment and retain its rate"
                  onClick={() => removePoint(index)}
                >
                  Remove point
                </button>
              ) : null}
              <strong>Segment {index + 1}</strong>
              <label>
                <span>Record units</span>
                <input
                  aria-label={`Segment ${index + 1} record duration`}
                  disabled={busy}
                  inputMode="numeric"
                  spellCheck={false}
                  value={segment.recordDuration}
                  onChange={(event) =>
                    updateSegment(
                      index,
                      "recordDuration",
                      event.currentTarget.value,
                    )
                  }
                />
              </label>
              <label>
                <span>Rate numerator</span>
                <input
                  aria-label={`Segment ${index + 1} rate numerator`}
                  disabled={busy}
                  inputMode="numeric"
                  spellCheck={false}
                  value={segment.rateNumerator}
                  onChange={(event) =>
                    updateSegment(
                      index,
                      "rateNumerator",
                      event.currentTarget.value,
                    )
                  }
                />
              </label>
              <label>
                <span>Rate denominator</span>
                <input
                  aria-label={`Segment ${index + 1} rate denominator`}
                  disabled={busy}
                  inputMode="numeric"
                  spellCheck={false}
                  value={segment.rateDenominator}
                  onChange={(event) =>
                    updateSegment(
                      index,
                      "rateDenominator",
                      event.currentTarget.value,
                    )
                  }
                />
              </label>
            </div>
          ))}
        </div>

        <figure className="timeline-retime-curve">
          <figcaption>Record to source curve</figcaption>
          {preview.status === "ready" ? (
            <>
              <svg
                aria-label="Exact retime curve"
                role="img"
                viewBox="0 0 100 100"
                preserveAspectRatio="none"
              >
                <polyline points={polyline} />
                {points.map((point) => (
                  <circle
                    cx={point.xPercent}
                    cy={point.yPercent}
                    key={point.recordOffset}
                    r="2.4"
                  />
                ))}
              </svg>
              <ol>
                {points.map((point) => (
                  <li key={point.recordOffset}>
                    Record {point.recordOffset}, source {point.sourceValue}
                  </li>
                ))}
              </ol>
            </>
          ) : (
            <p role="alert">{preview.reason}</p>
          )}
        </figure>
      </div>

      <dl className="timeline-retime-consequence">
        <div>
          <dt>Proposed state</dt>
          <dd>
            {modeLabel(draft.mode)}, {draft.segments.length}{" "}
            {draft.segments.length === 1 ? "segment" : "segments"}
          </dd>
        </div>
        <div>
          <dt>Consequence</dt>
          <dd>
            {preview.status === "ready" ? preview.consequence : preview.reason}
          </dd>
        </div>
      </dl>

      <div className="timeline-retime-actions">
        <button
          type="button"
          disabled={busy || preview.status !== "ready"}
          onClick={apply}
        >
          Apply exact retime
        </button>
        <button
          className="secondary"
          type="button"
          disabled={busy}
          onClick={resetDraft}
        >
          Reset draft
        </button>
        <button
          className="secondary"
          type="button"
          disabled={busy || undoDepth === 0}
          title="Command or Control Z"
          onClick={onUndo}
        >
          Undo latest edit
        </button>
      </div>
      <output className="timeline-retime-draft-status" aria-live="polite">
        {draftStatus}
      </output>
      <output
        className="timeline-command-status"
        data-pending={busy}
        aria-live="polite"
      >
        {status}
      </output>
    </section>
  );
}

function modeLabel(mode: TimelineRetimeMode): string {
  switch (mode) {
    case "identity":
      return "Normal speed";
    case "speed":
      return "Speed change";
    case "reverse":
      return "Reverse";
    case "freeze":
      return "Freeze";
    case "time_remap":
      return "Time remap";
  }
}

function retimeFailure(error: unknown): string {
  return error instanceof Error
    ? error.message
    : "The retime draft could not be updated.";
}
