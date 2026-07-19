import { useCallback, useEffect, useRef, useState } from "react";

import { isEditableCommandTarget } from "./application.ts";
import { useApplication } from "./application-context.tsx";
import type {
  EditorPlaybackSnapshot,
  PlaybackTransportAction,
} from "./api.ts";
import {
  VARIABLE_PLAYBACK_RATES,
  formatExactPlaybackTime,
  formatExactRate,
  playbackActionForKey,
  playbackDegradationLabel,
  playbackVisualState,
} from "./playback-transport.ts";

export function PlaybackControls() {
  const { editorProject, executePlaybackTransport } = useApplication();
  const playback = editorProject.snapshot?.playback ?? null;
  const snapshot =
    playback?.status === "attached" ? playback.latest : null;
  const snapshotRef = useRef<EditorPlaybackSnapshot | null>(snapshot);
  const commandInFlight = useRef(false);
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<string | null>(null);
  snapshotRef.current = snapshot;

  const submit = useCallback(
    async (
      action: PlaybackTransportAction,
      showBusy = true,
    ): Promise<void> => {
      if (commandInFlight.current) {
        return;
      }
      commandInFlight.current = true;
      if (showBusy) {
        setBusy(true);
      }
      try {
        await executePlaybackTransport(action);
        setFailure(null);
      } catch (error: unknown) {
        setFailure(
          error instanceof Error
            ? error.message
            : "Playback transport could not complete the command.",
        );
      } finally {
        commandInFlight.current = false;
        if (showBusy) {
          setBusy(false);
        }
      }
    },
    [executePlaybackTransport],
  );

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || isEditableCommandTarget(event.target)) {
        return;
      }
      const action = playbackActionForKey(event.key, snapshotRef.current);
      if (action === null) {
        return;
      }
      event.preventDefault();
      void submit(action);
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [submit]);

  useEffect(() => {
    if (snapshot?.mode !== "playing") {
      return;
    }
    const interval = window.setInterval(() => {
      void submit({ action: "inspect" }, false);
    }, 100);
    return () => window.clearInterval(interval);
  }, [snapshot?.mode, submit]);

  const attached = playback?.status === "attached";
  const pending = attached && playback.pending_command;
  const disabled = !attached || pending || busy;
  const direction = snapshot?.direction === "reverse" ? "reverse" : "forward";
  const loopEnabled = snapshot?.loop_range !== null && snapshot?.loop_range !== undefined;
  const rateOption = selectedRateOption(snapshot);

  return (
    <section className="timeline-workspace" aria-label="Playback transport">
      <div className="timeline-toolbar">
        <div className="timeline-toolbar-title">
          <p className="section-kicker">Exact playback transport</p>
          <h4>{snapshot?.mode ?? (attached ? "awaiting observation" : "detached")}</h4>
          <span>JKL shuttle, space play/pause</span>
        </div>
        <div className="timeline-readouts" aria-live="polite">
          <span>
            <small>Playhead</small>
            <strong>{formatOptionalTime(snapshot?.playhead ?? null)}</strong>
          </span>
          <span>
            <small>Rate</small>
            <strong>{snapshot === null ? "not observed" : formatExactRate(snapshot)}</strong>
          </span>
        </div>
        <div className="timeline-toolbar-actions">
          <button
            type="button"
            className="secondary timeline-compact-button"
            aria-keyshortcuts="J"
            disabled={disabled}
            onClick={() => void submit(playbackActionForKey("j", snapshot)!)}
          >
            J Reverse
          </button>
          <button
            type="button"
            className="secondary timeline-compact-button"
            aria-keyshortcuts="K"
            disabled={disabled}
            onClick={() => void submit({ action: "pause" })}
          >
            K Pause
          </button>
          <button
            type="button"
            className="secondary timeline-compact-button"
            aria-keyshortcuts="L"
            disabled={disabled}
            onClick={() => void submit(playbackActionForKey("l", snapshot)!)}
          >
            L Forward
          </button>
          <button
            type="button"
            className="secondary timeline-compact-button"
            disabled={disabled}
            onClick={() =>
              void submit(
                snapshot?.mode === "playing"
                  ? { action: "pause" }
                  : { action: "play" },
              )
            }
          >
            {snapshot?.mode === "playing" ? "Pause" : "Play"}
          </button>
          <button
            type="button"
            className="secondary timeline-compact-button"
            disabled={disabled}
            onClick={() => void submit({ action: "stop" })}
          >
            Stop
          </button>
        </div>
      </div>

      <div className="timeline-edit-controls">
        <div className="timeline-toolbar-actions">
          <button
            type="button"
            className="secondary timeline-compact-button"
            aria-pressed={loopEnabled}
            disabled={disabled}
            onClick={() =>
              void submit({ action: "set_loop", enabled: !loopEnabled })
            }
          >
            Loop {loopEnabled ? "On" : "Off"}
          </button>
          <button
            type="button"
            className="secondary timeline-compact-button"
            disabled={disabled}
            onClick={() =>
              void submit({
                action: "set_direction",
                direction: direction === "forward" ? "reverse" : "forward",
              })
            }
          >
            Direction {direction}
          </button>
        </div>
        <label>
          Variable speed
          <select
            value={rateOption}
            disabled={disabled}
            onChange={(event) => {
              const rate = VARIABLE_PLAYBACK_RATES.find(
                (candidate) =>
                  rateValue(candidate.numerator, candidate.denominator) ===
                  event.target.value,
              );
              if (rate === undefined) {
                return;
              }
              void submit({
                action: "set_rate",
                numerator:
                  direction === "reverse" ? -rate.numerator : rate.numerator,
                denominator: rate.denominator,
              });
            }}
          >
            {rateOption === "custom" ? (
              <option value="custom" disabled>
                Exact custom rate
              </option>
            ) : null}
            {VARIABLE_PLAYBACK_RATES.map((rate) => (
              <option
                key={rateValue(rate.numerator, rate.denominator)}
                value={rateValue(rate.numerator, rate.denominator)}
              >
                {rate.label}
              </option>
            ))}
          </select>
        </label>
        <div className="timeline-toolbar-actions">
          <button
            type="button"
            className="secondary timeline-compact-button"
            disabled={disabled}
            onClick={() => void submit({ action: "step_frames", delta: -1 })}
          >
            Step -1 frame
          </button>
          <button
            type="button"
            className="secondary timeline-compact-button"
            disabled={disabled}
            onClick={() => void submit({ action: "step_frames", delta: 1 })}
          >
            Step +1 frame
          </button>
        </div>
      </div>

      <dl className="editor-detail-list">
        <PlaybackDetail label="Mode" value={snapshot?.mode ?? "not observed"} />
        <PlaybackDetail
          label="Scheduled frame"
          value={formatOptionalTime(snapshot?.scheduled_frame ?? null)}
        />
        <PlaybackDetail
          label="Scheduled clock"
          value={formatOptionalTime(snapshot?.scheduled_due_clock ?? null)}
        />
        <PlaybackDetail label="Loop" value={formatLoop(snapshot)} />
        <PlaybackDetail
          label="Frame delivery"
          value={
            snapshot === null
              ? "not observed"
              : `${snapshot.total_dropped} total dropped, ${snapshot.consecutive_dropped} consecutive, ${snapshot.forced_presentations} forced`
          }
        />
        <PlaybackDetail
          label="Continuity epoch"
          value={snapshot?.epoch ?? "not observed"}
        />
        <PlaybackDetail
          label="Visual state"
          value={snapshot === null ? "not observed" : playbackVisualState(snapshot)}
        />
        <PlaybackDetail
          label="Audio state"
          value={snapshot?.audio_state ?? "not observed"}
        />
        <PlaybackDetail
          label="Audio synchronization"
          value={formatAudioSynchronization(snapshot)}
        />
        <PlaybackDetail
          label="Comparison state"
          value="Single program output; no visual comparison mode is active."
        />
        <PlaybackDetail
          label="Command state"
          value={pending ? "accepted and pending" : busy ? "settling" : "ready"}
        />
        <PlaybackDetail
          label="Degraded behavior"
          value={formatDegradation(snapshot)}
        />
      </dl>
      {snapshot?.failure ? (
        <p className="inline-warning">
          Playback failure: {snapshot.failure.category} ({snapshot.failure.recoverability}).
        </p>
      ) : null}
      {failure ? <p className="inline-warning">{failure}</p> : null}
    </section>
  );
}

function PlaybackDetail({
  label,
  value,
}: {
  readonly label: string;
  readonly value: string | number;
}) {
  return (
    <div>
      <dt>{label}</dt>
      <dd>{value}</dd>
    </div>
  );
}

function formatOptionalTime(
  time: EditorPlaybackSnapshot["playhead"] | null,
): string {
  return time === null ? "not scheduled" : formatExactPlaybackTime(time);
}

function formatLoop(snapshot: EditorPlaybackSnapshot | null): string {
  const range = snapshot?.loop_range;
  if (range === null || range === undefined) {
    return "disabled";
  }
  return `${range.start.value} + ${range.duration} units @ ${range.start.timebase.numerator}/${range.start.timebase.denominator} units/s`;
}

function formatAudioSynchronization(
  snapshot: EditorPlaybackSnapshot | null,
): string {
  if (snapshot === null) {
    return "not observed";
  }
  const status =
    snapshot.discard_requested_generation === snapshot.discard_applied_generation
      ? "acknowledged"
      : "pending";
  return `${status}, requested generation ${snapshot.discard_requested_generation}, applied generation ${snapshot.discard_applied_generation}`;
}

function formatDegradation(snapshot: EditorPlaybackSnapshot | null): string {
  if (snapshot === null) {
    return "not observed";
  }
  if (snapshot.degradation.length === 0) {
    return "none";
  }
  return snapshot.degradation.map(playbackDegradationLabel).join(" ");
}

function selectedRateOption(snapshot: EditorPlaybackSnapshot | null): string {
  if (snapshot === null) {
    return rateValue(1, 1);
  }
  const current = rateValue(
    Math.abs(snapshot.rate_numerator),
    snapshot.rate_denominator,
  );
  return VARIABLE_PLAYBACK_RATES.some(
    (rate) => rateValue(rate.numerator, rate.denominator) === current,
  )
    ? current
    : "custom";
}

function rateValue(numerator: number, denominator: number): string {
  return `${numerator}/${denominator}`;
}
