import { ApplicationTooltip } from "./application-presentation.tsx";
import type { ProjectHistoryPresentation } from "./project-history.ts";

import "./project-history.css";

export function ProjectHistoryControls({
  history,
  onUndo,
  onRedo,
}: {
  readonly history: ProjectHistoryPresentation;
  readonly onUndo: () => void | Promise<void>;
  readonly onRedo: () => void | Promise<void>;
}) {
  return (
    <section
      className="project-history-controls"
      data-history-condition={history.condition}
      aria-label="Project transaction history"
    >
      <div className="project-history-identity">
        <span>Project history</span>
        <strong>{history.documentLabel ?? "No project"}</strong>
        <small aria-live="polite">{history.status}</small>
      </div>
      <div className="project-history-actions">
        <HistoryButton action={history.undo} onExecute={onUndo} />
        <HistoryButton action={history.redo} onExecute={onRedo} />
      </div>
    </section>
  );
}

function HistoryButton({
  action,
  onExecute,
}: {
  readonly action: ProjectHistoryPresentation["undo"];
  readonly onExecute: () => void | Promise<void>;
}) {
  return (
    <ApplicationTooltip
      content={action.disabledReason ?? action.detail}
      placement="bottom"
    >
      <button
        type="button"
        disabled={!action.enabled}
        aria-label={action.title}
        onClick={() => void onExecute()}
      >
        <span>{action.title}</span>
        <strong aria-hidden="true">{action.depth}</strong>
      </button>
    </ApplicationTooltip>
  );
}
