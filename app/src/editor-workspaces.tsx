import { NativeViewport, SourceMonitor } from "./native-viewport.tsx";
import { useCallback, type ReactNode } from "react";
import type { TimelineTrackMutation } from "./api.ts";

import { useApplication } from "./application-context.tsx";
import {
  projectAudioTrack,
  type EditorProjectPresentation,
} from "./editor-project.ts";
import { TimelineWorkspace } from "./timeline-workspace.tsx";

export function EditingWorkspacePanel() {
  const {
    editorProject,
    refreshEditorProject,
    executeProjectActions,
    dispatch,
    state,
  } = useApplication();
  const snapshot = editorProject.snapshot;
  const mutateTracks = useCallback(
    async (mutations: readonly TimelineTrackMutation[]) => {
      await executeProjectActions([
        { action: "mutate_tracks", mutations: [...mutations] },
      ]);
    },
    [executeProjectActions],
  );
  return (
    <WorkspaceSurface
      label="Editing"
      project={editorProject}
      refresh={refreshEditorProject}
    >
      <div className="native-viewer-grid native-viewer-grid-dual">
        <SourceMonitor />
        <NativeViewport role="program" label="Program" />
      </div>
      {snapshot ? (
        <>
          <TimelineWorkspace
            document={snapshot.timeline.document}
            rootTimelineId={snapshot.project.root_timeline_id}
            snapshot={snapshot}
            playback={snapshot.playback}
            selection={state.selection}
            dispatchSelection={dispatch}
            selectionSchemaVersion={snapshot.schema_version}
            selectionRevision={snapshot.project.project_revision}
            mutateTracks={mutateTracks}
          />
          <div className="editor-summary-grid">
            <Metric label="Project" value={snapshot.project.project_id} />
            <Metric
              label="Project revision"
              value={snapshot.project.project_revision}
            />
            <Metric label="Timelines" value={snapshot.timeline.timeline_count} />
            <Metric label="Media records" value={snapshot.media.media_count} />
            <Metric label="Undo depth" value={snapshot.project.undo_depth} />
            <Metric label="Redo depth" value={snapshot.project.redo_depth} />
          </div>
          <section className="editor-section">
            <div className="editor-section-heading">
              <div>
                <p className="section-kicker">Canonical editorial state</p>
                <h4>{snapshot.project.root_timeline_id}</h4>
              </div>
              <button
                type="button"
                className="secondary"
                onClick={() =>
                  dispatch({
                    type: "replace_selection",
                    items: [
                      {
                        resource: "superi.editor.state",
                        schema_version: snapshot.schema_version,
                        identity: snapshot.project.project_id,
                        revision: snapshot.project.project_revision,
                      },
                    ],
                  })
                }
              >
                Select project
              </button>
            </div>
            <dl className="editor-detail-list">
              <Detail label="Timeline resource" value={snapshot.project.timeline_resource} />
              <Detail label="Timeline format" value={snapshot.timeline.document.format} />
              <Detail
                label="Timeline document"
                value={snapshot.timeline.document.sha256}
                code
              />
              <Detail label="Playback" value={snapshot.playback.status} />
              <Detail
                label="Playback mode"
                value={
                  snapshot.playback.status === "attached" &&
                  snapshot.playback.latest
                    ? snapshot.playback.latest.mode
                    : "not observed"
                }
              />
              <Detail
                label="Audio playback"
                value={
                  snapshot.playback.status === "attached" &&
                  snapshot.playback.latest
                    ? snapshot.playback.latest.audio_state
                    : "not observed"
                }
              />
            </dl>
          </section>
        </>
      ) : null}
    </WorkspaceSurface>
  );
}

export function CompositingWorkspacePanel() {
  const { editorProject, refreshEditorProject } = useApplication();
  const snapshot = editorProject.snapshot;
  return (
    <WorkspaceSurface
      label="Compositing"
      project={editorProject}
      refresh={refreshEditorProject}
    >
      <NativeViewport role="composite" label="Composite" />
      {snapshot ? (
        <>
          <div className="editor-summary-grid">
            <Metric label="Graphs" value={snapshot.graph.documents.length} />
            <Metric
              label="Effect resources"
              value={snapshot.effect.graph_resources.length}
            />
            <Metric
              label="Effect extensions"
              value={snapshot.effect.extension_records.length}
            />
          </div>
          <section className="editor-section">
            <p className="section-kicker">Editable graph documents</p>
            {snapshot.graph.documents.length === 0 ? (
              <EmptyState message="No graph documents are attached to this project." />
            ) : (
              <ul className="editor-card-list">
                {snapshot.graph.documents.map((graph) => (
                  <li key={graph.graph_id}>
                    <div>
                      <strong>{graph.graph_id}</strong>
                      <span>{formatGraphScope(graph.scope)}</span>
                    </div>
                    <dl>
                      <Detail label="Revision" value={graph.graph_revision} compact />
                      <Detail label="Format" value={graph.document.format} compact />
                      <Detail label="Document" value={graph.document.sha256} code compact />
                    </dl>
                  </li>
                ))}
              </ul>
            )}
          </section>
        </>
      ) : null}
    </WorkspaceSurface>
  );
}

export function ColorWorkspacePanel() {
  const { editorProject, refreshEditorProject } = useApplication();
  const snapshot = editorProject.snapshot;
  return (
    <WorkspaceSurface
      label="Color"
      project={editorProject}
      refresh={refreshEditorProject}
    >
      <NativeViewport role="color" label="Color" />
      {snapshot ? (
        <>
          <div className="editor-summary-grid">
            <Metric
              label="Working space"
              value={snapshot.color.management.working_space}
            />
            <Metric label="Management" value={snapshot.color.management.kind} />
            <Metric label="Render target" value={snapshot.color.render_target} />
            <Metric
              label="Color graph resources"
              value={snapshot.color.graph_resources.length}
            />
          </div>
          <section className="editor-section">
            <p className="section-kicker">Engine-owned color state</p>
            <dl className="editor-detail-list">
              <Detail label="Settings resource" value={snapshot.color.settings_resource} />
              <Detail label="Project revision" value={snapshot.color.project_revision} />
              <Detail
                label="Render fingerprint"
                value={snapshot.color.render_settings_fingerprint}
                code
              />
              {snapshot.color.management.kind === "pinned_config" ? (
                <>
                  <Detail label="Config" value={snapshot.color.management.config_id} />
                  <Detail
                    label="Config fingerprint"
                    value={snapshot.color.management.config_fingerprint}
                    code
                  />
                </>
              ) : null}
            </dl>
          </section>
        </>
      ) : null}
    </WorkspaceSurface>
  );
}

export function AudioWorkspacePanel() {
  const { editorProject, refreshEditorProject } = useApplication();
  const snapshot = editorProject.snapshot;
  const tracks = snapshot?.audio.tracks.map(projectAudioTrack) ?? [];
  return (
    <WorkspaceSurface
      label="Audio"
      project={editorProject}
      refresh={refreshEditorProject}
    >
      {snapshot ? (
        <>
          <div className="editor-summary-grid">
            <Metric label="Tracks" value={snapshot.audio.audio_track_count} />
            <Metric label="Automation" value={snapshot.audio.automation.status} />
            <Metric
              label="Playback sync"
              value={
                snapshot.playback.status === "attached" &&
                snapshot.playback.latest
                  ? snapshot.playback.latest.audio_state
                  : "not observed"
              }
            />
            <Metric label="Timeline" value={snapshot.audio.timeline_resource} />
            <Metric label="Mix document" value={snapshot.audio.clip_mix.sha256} />
          </div>
          <section className="editor-section">
            <p className="section-kicker">Exact authored audio state</p>
            {tracks.length === 0 ? (
              <EmptyState message="No authored audio tracks are present." />
            ) : (
              <div className="audio-track-list">
                {tracks.map((track) => (
                  <article className="audio-track" key={track.track_id}>
                    <header>
                      <div>
                        <span>{track.timeline_id}</span>
                        <h4>{track.track_id}</h4>
                      </div>
                      <strong>{track.sample_rate.toLocaleString()} samples/s</strong>
                    </header>
                    <div className="audio-route-grid">
                      <ChannelList label="Source order" channels={track.source_channels} />
                      <ChannelList
                        label="Destination order"
                        channels={track.destination_channels}
                      />
                    </div>
                    <dl className="editor-detail-list compact-details">
                      <Detail label="Destination" value={formatDestination(track.destination)} />
                      <Detail label="Clip count" value={track.clip_count} />
                    </dl>
                    <div className="audio-routes">
                      <span className="section-kicker">Channel routes</span>
                      {track.routes.length === 0 ? (
                        <p>No channel routes are authored.</p>
                      ) : (
                        <ul>
                          {track.routes.map((route, index) => (
                            <li key={`${route.source}:${index}`}>
                              <code>{route.source}</code>
                              <span aria-hidden="true">-&gt;</span>
                              <code>{formatRouteTarget(route.target)}</code>
                            </li>
                          ))}
                        </ul>
                      )}
                    </div>
                    <ContinuityState continuity={track.continuity} />
                  </article>
                ))}
              </div>
            )}
          </section>
        </>
      ) : null}
    </WorkspaceSurface>
  );
}

export function DeliveryWorkspacePanel() {
  const { editorProject, refreshEditorProject } = useApplication();
  const snapshot = editorProject.snapshot;
  const latest =
    snapshot?.export.status === "attached" ? snapshot.export.latest : null;
  return (
    <WorkspaceSurface
      label="Delivery"
      project={editorProject}
      refresh={refreshEditorProject}
    >
      {snapshot ? (
        <>
          <div className="editor-summary-grid">
            <Metric label="Queue" value={snapshot.export.status} />
            <Metric label="Queue revision" value={latest?.revision ?? "not observed"} />
            <Metric label="Jobs" value={latest?.jobs.length ?? 0} />
          </div>
          <section className="editor-section">
            <p className="section-kicker">Public export queue state</p>
            {snapshot.export.status === "detached" ? (
              <EmptyState message="The export queue is detached." />
            ) : latest === null ? (
              <EmptyState message="No export queue replacement has been observed." />
            ) : latest.jobs.length === 0 ? (
              <EmptyState message="The export queue contains no jobs." />
            ) : (
              <ul className="delivery-job-list">
                {latest.jobs.map((job) => (
                  <li key={job.job_id}>
                    <header>
                      <div>
                        <span>{job.status}</span>
                        <strong>{job.job_id}</strong>
                      </div>
                      <span>attempt {job.attempt}</span>
                    </header>
                    <dl className="editor-detail-list compact-details">
                      <Detail
                        label="Progress"
                        value={`${job.completed_units} / ${job.total_units ?? "unknown"}`}
                      />
                      <Detail label="Progress revision" value={job.progress_revision} />
                      <Detail label="Dependencies" value={job.dependencies.length} />
                      <Detail label="Retry allowed" value={yesNo(job.retry_allowed)} />
                      <Detail label="Final" value={yesNo(job.is_final)} />
                      <Detail label="Result" value={yesNo(job.has_result)} />
                    </dl>
                    {job.failure ? (
                      <p className="inline-warning">A structured export failure is attached.</p>
                    ) : null}
                  </li>
                ))}
              </ul>
            )}
          </section>
        </>
      ) : null}
    </WorkspaceSurface>
  );
}

export function SharedSelectionPanel() {
  const { state, executeCommand } = useApplication();
  return (
    <div className="panel-content selection-panel">
      {state.selection.items.length === 0 ? (
        <p className="empty-selection">
          Nothing is selected. Every workspace shares exact public resource
          references without copying engine state.
        </p>
      ) : (
        <ul className="selection-list">
          {state.selection.items.map((item) => (
            <li key={`${item.resource}:${item.identity}`}>
              <span>{item.resource}</span>
              <strong>{item.identity}</strong>
              <small>revision {item.revision}</small>
            </li>
          ))}
        </ul>
      )}
      <button
        className="secondary"
        type="button"
        disabled={state.selection.items.length === 0}
        onClick={() => void executeCommand("application.selection.clear")}
      >
        Clear selection
      </button>
    </div>
  );
}

function WorkspaceSurface({
  label,
  project,
  refresh,
  children,
}: {
  readonly label: string;
  readonly project: EditorProjectPresentation;
  readonly refresh: () => Promise<void>;
  readonly children: ReactNode;
}) {
  const hasSnapshot = project.snapshot !== null;
  return (
    <div className="panel-content editor-workspace-content">
      <div
        className={`project-state-banner project-state-${project.status}`}
        role={project.failure ? "alert" : "status"}
      >
        <div>
          <span>{label} project state</span>
          <strong>{projectStatusLabel(project)}</strong>
          {project.failure ? (
            <p>{project.failure.action}</p>
          ) : hasSnapshot ? (
            <p>
              Public API snapshot at project revision {project.snapshot?.project.project_revision}.
            </p>
          ) : (
            <p>Waiting for the public editor state surface.</p>
          )}
        </div>
        <button
          className="secondary"
          type="button"
          disabled={project.status === "loading" || project.status === "refreshing"}
          onClick={() => void refresh()}
        >
          Refresh state
        </button>
      </div>
      {project.failure ? (
        <p className="project-failure-code">
          {project.failure.code} / {project.failure.condition}
        </p>
      ) : null}
      {children}
    </div>
  );
}

function Metric({ label, value }: { readonly label: string; readonly value: string | number }) {
  return (
    <div>
      <span>{label}</span>
      <strong title={String(value)}>{value}</strong>
    </div>
  );
}

function Detail({
  label,
  value,
  code = false,
  compact = false,
}: {
  readonly label: string;
  readonly value: string | number;
  readonly code?: boolean;
  readonly compact?: boolean;
}) {
  return (
    <div className={compact ? "compact" : undefined}>
      <dt>{label}</dt>
      <dd className={code ? "code-value" : undefined} title={String(value)}>
        {value}
      </dd>
    </div>
  );
}

function ChannelList({ label, channels }: { readonly label: string; readonly channels: readonly string[] }) {
  return (
    <div>
      <span className="section-kicker">{label}</span>
      {channels.length === 0 ? (
        <p>None</p>
      ) : (
        <ol>
          {channels.map((channel, index) => (
            <li key={`${channel}:${index}`}>
              <span>{index + 1}</span>
              <code>{channel}</code>
            </li>
          ))}
        </ol>
      )}
    </div>
  );
}

function ContinuityState({
  continuity,
}: {
  readonly continuity: ReturnType<typeof projectAudioTrack>["continuity"];
}) {
  if (continuity.status === "unsupported") {
    return (
      <div className="continuity-state continuity-unsupported">
        <span>Continuity unsupported</span>
        <p>{continuity.reason}</p>
      </div>
    );
  }
  return (
    <div className="continuity-state">
      <span>
        Record coverage: {continuity.uninterrupted_record_coverage ? "uninterrupted" : "not uninterrupted"}
      </span>
      {continuity.seams.length === 0 ? (
        <p>No adjacent seams are present.</p>
      ) : (
        <ul>
          {continuity.seams.map((seam, index) => (
            <li key={`${seam.left_clip_id}:${seam.right_clip_id}:${index}`}>
              <strong>{seam.left_clip_id} -&gt; {seam.right_clip_id}</strong>
              <span>{formatRecordContinuity(seam.record)}</span>
              <span>{formatSourceContinuity(seam.source)}</span>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

function EmptyState({ message }: { readonly message: string }) {
  return <p className="editor-empty-state">{message}</p>;
}

function projectStatusLabel(project: EditorProjectPresentation): string {
  if (project.status === "ready") return "Live public snapshot";
  if (project.status === "refreshing") return "Refreshing last-valid snapshot";
  if (project.status === "loading") return "Loading public snapshot";
  if (project.status === "degraded") return "Degraded public API";
  if (project.status === "failed") return "Project state unavailable";
  return "API client unavailable";
}

function formatGraphScope(scope: unknown): string {
  return typeof scope === "string" ? scope : JSON.stringify(scope);
}

function formatDestination(destination: { readonly kind: "main" } | { readonly kind: "track"; readonly track_id: string }): string {
  return destination.kind === "main" ? "main" : `track:${destination.track_id}`;
}

function formatRouteTarget(target: { readonly kind: "muted" } | { readonly kind: "channel"; readonly channel: string }): string {
  return target.kind === "muted" ? "muted" : target.channel;
}

function formatRecordContinuity(record: { readonly kind: "seamless" } | { readonly kind: "gap" | "overlap"; readonly sample_count: number }): string {
  return record.kind === "seamless" ? "record seamless" : `record ${record.kind}: ${record.sample_count} samples`;
}

function formatSourceContinuity(source: { readonly kind: "continuous" } | { readonly kind: "discontinuous"; readonly expected: number; readonly actual: number } | { readonly kind: "different_clip"; readonly left: string; readonly right: string }): string {
  if (source.kind === "continuous") return "source continuous";
  if (source.kind === "different_clip") return `source clips differ: ${source.left} / ${source.right}`;
  return `source discontinuous: expected ${source.expected}, actual ${source.actual}`;
}

function yesNo(value: boolean): string {
  return value ? "yes" : "no";
}
