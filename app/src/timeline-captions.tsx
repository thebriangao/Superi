import {
  useCallback,
  useEffect,
  useMemo,
  useState,
  type ChangeEvent,
  type FormEvent,
} from "react";

import type {
  ExecuteProjectCommandResult,
  ProjectAction,
  TimelineCaptionAlignment,
  TimelineCaptionMutation,
  TimelineCaptionPosition,
  TimelineCaptionRelationship,
  TimelineCaptionStyle,
} from "./api.ts";
import {
  readProjectMediaLibrary,
  type MediaBrowserItem,
} from "./project-lifecycle.ts";
import {
  buildCaptionImportActions,
  captionStyleFromFields,
  captionCuesFromTrack,
  captionCuesFromTranscript,
  parseCaptionExchange,
  serializeCaptionExchange,
  type CaptionCue,
  type CaptionExchangeFormat,
} from "./timeline-captions.ts";
import type {
  TimelineCanvasModel,
  TimelineSelectionTarget,
} from "./timeline-workspace.ts";

type ExecuteProjectActions = (
  actions: readonly ProjectAction[],
) => Promise<ExecuteProjectCommandResult>;

interface TimelineCaptionPanelProps {
  readonly model: TimelineCanvasModel;
  readonly selectedCaptionTarget: TimelineSelectionTarget | null;
  readonly executeProjectActions: ExecuteProjectActions;
  readonly projectRevision: number;
}

const DEFAULT_STYLE: TimelineCaptionStyle = Object.freeze({
  font_family: null,
  font_size: null,
  foreground: null,
  background: null,
  bold: false,
  italic: false,
  alignment: "center",
  position: "bottom",
});

export function TimelineCaptionPanel({
  model,
  selectedCaptionTarget,
  executeProjectActions,
  projectRevision,
}: TimelineCaptionPanelProps) {
  const selectedCaption = selectedCaptionTarget?.item.caption ?? null;
  const [name, setName] = useState("");
  const [text, setText] = useState("");
  const [language, setLanguage] = useState("");
  const [speaker, setSpeaker] = useState("");
  const [styleEnabled, setStyleEnabled] = useState(false);
  const [fontFamily, setFontFamily] = useState("");
  const [fontSize, setFontSize] = useState("");
  const [foreground, setForeground] = useState("");
  const [background, setBackground] = useState("");
  const [bold, setBold] = useState(false);
  const [italic, setItalic] = useState(false);
  const [alignment, setAlignment] =
    useState<TimelineCaptionAlignment>("center");
  const [position, setPosition] =
    useState<TimelineCaptionPosition>("bottom");
  const [relationships, setRelationships] = useState("");
  const [pending, setPending] = useState<string | null>(null);
  const [status, setStatus] = useState(
    "Select a caption for text and style editing, or create a caption track from exchange or analysis.",
  );
  const [failure, setFailure] = useState<string | null>(null);
  const [importLanguage, setImportLanguage] = useState("en-US");
  const [importTrackName, setImportTrackName] = useState("Imported subtitles");
  const [analysisItems, setAnalysisItems] = useState<readonly MediaBrowserItem[]>([]);
  const [analysisProjectRevision, setAnalysisProjectRevision] = useState<number | null>(null);
  const [analysisMediaId, setAnalysisMediaId] = useState("");
  const [analysisLanguage, setAnalysisLanguage] = useState("en-US");
  const [analysisTrackName, setAnalysisTrackName] = useState("Analyzed captions");
  const captionTracks = useMemo(
    () => model.tracks.filter((track) => track.kind === "caption"),
    [model.tracks],
  );
  const [exportTrackId, setExportTrackId] = useState("");
  const [exportFormat, setExportFormat] =
    useState<CaptionExchangeFormat>("vtt");

  useEffect(() => {
    if (selectedCaptionTarget === null || selectedCaption === null) {
      setName("");
      setText("");
      setLanguage("");
      setSpeaker("");
      setStyleEnabled(false);
      setFontFamily("");
      setFontSize("");
      setForeground("");
      setBackground("");
      setBold(false);
      setItalic(false);
      setAlignment("center");
      setPosition("bottom");
      setRelationships("");
      return;
    }
    const style = selectedCaption.style ?? DEFAULT_STYLE;
    setName(selectedCaptionTarget.item.name);
    setText(selectedCaption.text);
    setLanguage(selectedCaption.language ?? "");
    setSpeaker(selectedCaption.speaker ?? "");
    setStyleEnabled(selectedCaption.style !== null);
    setFontFamily(style.font_family ?? "");
    setFontSize(style.font_size?.toString() ?? "");
    setForeground(style.foreground ?? "");
    setBackground(style.background ?? "");
    setBold(style.bold);
    setItalic(style.italic);
    setAlignment(style.alignment);
    setPosition(style.position);
    setRelationships(formatRelationships(selectedCaption.timelineRelationships));
    setFailure(null);
    setStatus(
      `${selectedCaptionTarget.item.name || selectedCaptionTarget.item.id} loaded from canonical project state.`,
    );
  }, [model.documentSha256, selectedCaption, selectedCaptionTarget]);

  useEffect(() => {
    setExportTrackId((current) =>
      captionTracks.some((track) => track.id === current)
        ? current
        : captionTracks[0]?.id ?? "",
    );
  }, [captionTracks]);

  useEffect(() => {
    setAnalysisItems([]);
    setAnalysisProjectRevision(null);
    setAnalysisMediaId("");
  }, [model.id, model.projectId]);

  const run = useCallback(
    async (label: string, actions: readonly ProjectAction[]) => {
      if (pending !== null) return null;
      setPending(label);
      setFailure(null);
      setStatus(`${label} is publishing through project history.`);
      try {
        const result = await executeProjectActions(actions);
        setStatus(
          `${label} published at project revision ${result.state.project_revision}. Undo is available immediately.`,
        );
        return result;
      } catch (error: unknown) {
        setFailure(captionFailure(error));
        return null;
      } finally {
        setPending(null);
      }
    },
    [executeProjectActions, pending],
  );

  const saveCaption = useCallback(
    async (event: FormEvent<HTMLFormElement>) => {
      event.preventDefault();
      if (selectedCaptionTarget === null || selectedCaption === null) return;
      try {
        const style = styleEnabled
          ? captionStyleFromFields({
              fontFamily,
              fontSize,
              foreground,
              background,
              bold,
              italic,
              alignment,
              position,
            })
          : null;
        const mutations: TimelineCaptionMutation[] = [
          {
            operation: "set_name",
            timeline_id: model.id,
            caption_id: selectedCaptionTarget.item.id,
            name,
          },
          {
            operation: "set_text",
            timeline_id: model.id,
            caption_id: selectedCaptionTarget.item.id,
            text,
          },
          {
            operation: "set_language",
            timeline_id: model.id,
            caption_id: selectedCaptionTarget.item.id,
            language: optionalText(language),
          },
          {
            operation: "set_speaker",
            timeline_id: model.id,
            caption_id: selectedCaptionTarget.item.id,
            speaker: optionalText(speaker),
          },
          {
            operation: "set_style",
            timeline_id: model.id,
            caption_id: selectedCaptionTarget.item.id,
            style,
          },
          {
            operation: "set_timeline_relationships",
            timeline_id: model.id,
            caption_id: selectedCaptionTarget.item.id,
            relationships: parseRelationships(relationships),
          },
        ];
        await run("Caption edit", [
          { action: "mutate_captions", mutations },
        ]);
      } catch (error: unknown) {
        setFailure(captionFailure(error));
      }
    },
    [
      alignment,
      background,
      bold,
      fontFamily,
      fontSize,
      foreground,
      italic,
      language,
      model.id,
      name,
      position,
      relationships,
      run,
      selectedCaption,
      selectedCaptionTarget,
      speaker,
      styleEnabled,
      text,
    ],
  );

  const importExchange = useCallback(
    async (event: ChangeEvent<HTMLInputElement>) => {
      const input = event.currentTarget;
      const file = input.files?.[0];
      if (!file) return;
      try {
        const format = exchangeFormatForName(file.name);
        const cues = parseCaptionExchange(
          await file.text(),
          format,
          importLanguage.trim(),
        );
        await importCues({
          cues,
          model,
          trackName: importTrackName,
          language: importLanguage,
          purpose: "subtitles",
          run,
          label: `${format.toUpperCase()} caption import`,
        });
      } catch (error: unknown) {
        setFailure(captionFailure(error));
      } finally {
        input.value = "";
      }
    },
    [importLanguage, importTrackName, model, run],
  );

  const loadAnalysisSources = useCallback(async () => {
    if (pending !== null) return;
    setPending("Language analysis discovery");
    setFailure(null);
    try {
      const library = await readProjectMediaLibrary();
      if (library.project_revision !== projectRevision) {
        throw new Error(
          "Media analysis belongs to a different project revision. Refresh before importing captions.",
        );
      }
      const items = library.items.filter(
        (item) =>
          item.content_analysis.transcript_segments.length > 0 &&
          item.content_analysis.source_fingerprint === item.content_fingerprint,
      );
      setAnalysisItems(Object.freeze(items));
      setAnalysisProjectRevision(library.project_revision);
      setAnalysisMediaId((current) =>
        items.some((item) => item.media_id === current)
          ? current
          : items[0]?.media_id ?? "",
      );
      setStatus(
        items.length === 0
          ? "No fresh language analysis with transcript timing is available."
          : `${items.length} fresh language analysis ${items.length === 1 ? "source is" : "sources are"} ready.`,
      );
    } catch (error: unknown) {
      setFailure(captionFailure(error));
    } finally {
      setPending(null);
    }
  }, [pending, projectRevision]);

  const importAnalysis = useCallback(async () => {
    const item = analysisItems.find(
      (candidate) => candidate.media_id === analysisMediaId,
    );
    if (item === undefined || analysisProjectRevision === null) {
      setFailure("Load and select a fresh language analysis source first.");
      return;
    }
    try {
      const cues = captionCuesFromTranscript({
        expectedProjectRevision: analysisProjectRevision,
        projectRevision,
        currentSourceFingerprint: item.content_fingerprint,
        analysisSourceFingerprint: item.content_analysis.source_fingerprint,
        language: analysisLanguage.trim(),
        segments: item.content_analysis.transcript_segments,
      });
      await importCues({
        cues,
        model,
        trackName: analysisTrackName,
        language: analysisLanguage,
        purpose: "captions",
        run,
        label: "Language analysis caption import",
      });
    } catch (error: unknown) {
      setFailure(captionFailure(error));
    }
  }, [
    analysisItems,
    analysisLanguage,
    analysisMediaId,
    analysisProjectRevision,
    analysisTrackName,
    model,
    projectRevision,
    run,
  ]);

  const exportTrack = useCallback(() => {
    try {
      const track = captionTracks.find(
        (candidate) => candidate.id === exportTrackId,
      );
      if (track === undefined) {
        throw new Error("Select a caption track to export.");
      }
      const source = serializeCaptionExchange(
        captionCuesFromTrack(track),
        exportFormat,
      );
      downloadCaptionSource(
        source,
        `${safeFilename(track.name)}.${exportFormat}`,
        exportFormat,
      );
      setFailure(null);
      setStatus(
        `${track.name} exported as deterministic ${exportFormat.toUpperCase()} from canonical project state.`,
      );
    } catch (error: unknown) {
      setFailure(captionFailure(error));
    }
  }, [captionTracks, exportFormat, exportTrackId]);

  const captionCount = captionTracks.reduce(
    (count, track) =>
      count + track.items.filter((item) => item.kind === "caption").length,
    0,
  );

  return (
    <section className="timeline-marker-panel" aria-label="Caption editing and exchange">
      <header className="timeline-marker-header">
        <div>
          <span>Captions and subtitles</span>
          <strong>
            {captionCount} cues on {captionTracks.length} tracks
          </strong>
        </div>
        <div className="timeline-marker-actions">
          <button
            className="secondary timeline-compact-button"
            type="button"
            disabled={pending !== null}
            onClick={() => void loadAnalysisSources()}
          >
            Refresh language analysis
          </button>
        </div>
      </header>
      <div className="timeline-marker-body">
        {selectedCaptionTarget !== null && selectedCaption !== null ? (
          <form className="timeline-marker-editor" onSubmit={saveCaption}>
            <h5>Edit selected caption</h5>
            <label>
              <span>Name</span>
              <input
                value={name}
                disabled={pending !== null}
                onChange={(event) => setName(event.currentTarget.value)}
              />
            </label>
            <label className="timeline-marker-note-field">
              <span>Editable text</span>
              <textarea
                value={text}
                disabled={pending !== null}
                onChange={(event) => setText(event.currentTarget.value)}
              />
            </label>
            <label>
              <span>Language</span>
              <input
                value={language}
                disabled={pending !== null}
                onChange={(event) => setLanguage(event.currentTarget.value)}
                placeholder="en-US"
              />
            </label>
            <label>
              <span>Speaker</span>
              <input
                value={speaker}
                disabled={pending !== null}
                onChange={(event) => setSpeaker(event.currentTarget.value)}
                placeholder="Optional speaker"
              />
            </label>
            <label>
              <span>Presentation styling</span>
              <input
                type="checkbox"
                checked={styleEnabled}
                disabled={pending !== null}
                onChange={(event) => setStyleEnabled(event.currentTarget.checked)}
              />
            </label>
            <label>
              <span>Font family</span>
              <input
                value={fontFamily}
                disabled={pending !== null || !styleEnabled}
                onChange={(event) => setFontFamily(event.currentTarget.value)}
                placeholder="Optional"
              />
            </label>
            <label>
              <span>Font size</span>
              <input
                value={fontSize}
                inputMode="decimal"
                disabled={pending !== null || !styleEnabled}
                onChange={(event) => setFontSize(event.currentTarget.value)}
                placeholder="8 to 256"
              />
            </label>
            <label>
              <span>Foreground RGBA</span>
              <input
                value={foreground}
                disabled={pending !== null || !styleEnabled}
                onChange={(event) => setForeground(event.currentTarget.value)}
                placeholder="#ffffffff"
              />
            </label>
            <label>
              <span>Background RGBA</span>
              <input
                value={background}
                disabled={pending !== null || !styleEnabled}
                onChange={(event) => setBackground(event.currentTarget.value)}
                placeholder="#000000cc"
              />
            </label>
            <label>
              <span>Alignment</span>
              <select
                value={alignment}
                disabled={pending !== null || !styleEnabled}
                onChange={(event) =>
                  setAlignment(event.currentTarget.value as TimelineCaptionAlignment)
                }
              >
                <option value="start">Start</option>
                <option value="center">Center</option>
                <option value="end">End</option>
              </select>
            </label>
            <label>
              <span>Position</span>
              <select
                value={position}
                disabled={pending !== null || !styleEnabled}
                onChange={(event) =>
                  setPosition(event.currentTarget.value as TimelineCaptionPosition)
                }
              >
                <option value="top">Top</option>
                <option value="bottom">Bottom</option>
              </select>
            </label>
            <label>
              <span>Bold</span>
              <input
                type="checkbox"
                checked={bold}
                disabled={pending !== null || !styleEnabled}
                onChange={(event) => setBold(event.currentTarget.checked)}
              />
            </label>
            <label>
              <span>Italic</span>
              <input
                type="checkbox"
                checked={italic}
                disabled={pending !== null || !styleEnabled}
                onChange={(event) => setItalic(event.currentTarget.checked)}
              />
            </label>
            <label className="timeline-marker-note-field">
              <span>Timeline relationships</span>
              <textarea
                value={relationships}
                disabled={pending !== null}
                onChange={(event) => setRelationships(event.currentTarget.value)}
                placeholder="timeline:... | clip:..."
              />
            </label>
            <p>
              Exact timing is {selectedCaptionTarget.item.recordRange.start.value} +{" "}
              {selectedCaptionTarget.item.recordRange.duration.value} at{" "}
              {selectedCaptionTarget.item.recordRange.start.timebase.numerator}/
              {selectedCaptionTarget.item.recordRange.start.timebase.denominator}. Use the
              timeline trim, razor, nudge, ripple, and overwrite gestures to edit timing.
            </p>
            <div className="timeline-marker-editor-actions">
              <button type="submit" disabled={pending !== null}>
                Save caption
              </button>
            </div>
          </form>
        ) : (
          <p className="timeline-marker-empty-editor">
            Select one caption cue to edit its text, language, speaker, presentation,
            and timeline relationships. Timing remains editable through the existing
            exact timeline gestures.
          </p>
        )}
        <div className="timeline-marker-editor">
          <h5>Import SRT or VTT</h5>
          <label>
            <span>Track name</span>
            <input
              value={importTrackName}
              disabled={pending !== null}
              onChange={(event) => setImportTrackName(event.currentTarget.value)}
            />
          </label>
          <label>
            <span>Language</span>
            <input
              value={importLanguage}
              disabled={pending !== null}
              onChange={(event) => setImportLanguage(event.currentTarget.value)}
              placeholder="en-US"
            />
          </label>
          <label>
            <span>Subtitle file</span>
            <input
              type="file"
              accept=".srt,.vtt,text/vtt,application/x-subrip"
              disabled={pending !== null}
              onChange={(event) => void importExchange(event)}
            />
          </label>
          <h5>Language analysis to editable captions</h5>
          <label>
            <span>Fresh analyzed media</span>
            <select
              value={analysisMediaId}
              disabled={pending !== null || analysisItems.length === 0}
              onChange={(event) => setAnalysisMediaId(event.currentTarget.value)}
            >
              {analysisItems.length === 0 ? (
                <option value="">Refresh language analysis first</option>
              ) : null}
              {analysisItems.map((item) => (
                <option value={item.media_id} key={item.media_id}>
                  {item.name} ({item.content_analysis.transcript_segments.length} cues)
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>Caption language</span>
            <input
              value={analysisLanguage}
              disabled={pending !== null}
              onChange={(event) => setAnalysisLanguage(event.currentTarget.value)}
              placeholder="en-US"
            />
          </label>
          <label>
            <span>Analysis track name</span>
            <input
              value={analysisTrackName}
              disabled={pending !== null}
              onChange={(event) => setAnalysisTrackName(event.currentTarget.value)}
            />
          </label>
          <div className="timeline-marker-editor-actions">
            <button
              type="button"
              disabled={pending !== null || analysisMediaId.length === 0}
              onClick={() => void importAnalysis()}
            >
              Create editable caption track
            </button>
          </div>
          <h5>Export caption track</h5>
          <label>
            <span>Canonical track</span>
            <select
              value={exportTrackId}
              disabled={pending !== null || captionTracks.length === 0}
              onChange={(event) => setExportTrackId(event.currentTarget.value)}
            >
              {captionTracks.length === 0 ? (
                <option value="">No caption tracks</option>
              ) : null}
              {captionTracks.map((track) => (
                <option value={track.id} key={track.id}>
                  {track.name}
                </option>
              ))}
            </select>
          </label>
          <label>
            <span>Format</span>
            <select
              value={exportFormat}
              disabled={pending !== null}
              onChange={(event) =>
                setExportFormat(event.currentTarget.value as CaptionExchangeFormat)
              }
            >
              <option value="vtt">WebVTT</option>
              <option value="srt">SRT</option>
            </select>
          </label>
          <div className="timeline-marker-editor-actions">
            <button
              className="secondary"
              type="button"
              disabled={pending !== null || exportTrackId.length === 0}
              onClick={exportTrack}
            >
              Export caption track
            </button>
          </div>
        </div>
      </div>
      <output className="timeline-marker-status" aria-live="polite">
        {pending ?? status}
      </output>
      {failure ? (
        <p className="timeline-command-failure" role="alert">
          {failure}
        </p>
      ) : null}
    </section>
  );
}

async function importCues({
  cues,
  model,
  trackName,
  language,
  purpose,
  run,
  label,
}: {
  readonly cues: readonly CaptionCue[];
  readonly model: TimelineCanvasModel;
  readonly trackName: string;
  readonly language: string;
  readonly purpose: "captions" | "subtitles";
  readonly run: (
    label: string,
    actions: readonly ProjectAction[],
  ) => Promise<ExecuteProjectCommandResult | null>;
  readonly label: string;
}): Promise<void> {
  const plan = buildCaptionImportActions({
    timelineId: model.id,
    trackId: randomTypedId("track"),
    trackName: trackName.trim(),
    trackPosition: model.tracks.length,
    language: language.trim(),
    purpose,
    cues,
    createId: randomTypedId,
  });
  await run(label, plan.actions);
}

function optionalText(value: string): string | null {
  const trimmed = value.trim();
  return trimmed.length === 0 ? null : trimmed;
}

function parseRelationships(source: string): TimelineCaptionRelationship[] {
  const relationships = source
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line): TimelineCaptionRelationship => {
      const parts = line.split("|").map((part) => part.trim());
      if (parts.length > 2 || parts[0]?.length === 0) {
        throw new Error(
          "Each caption relationship must use timeline_id or timeline_id | clip_id.",
        );
      }
      return {
        timeline_id: parts[0]!,
        clip_id: parts[1]?.length ? parts[1] : null,
      };
    });
  return relationships;
}

function formatRelationships(
  relationships: readonly TimelineCaptionRelationship[],
): string {
  return relationships
    .map((relationship) =>
      relationship.clip_id === null
        ? relationship.timeline_id
        : `${relationship.timeline_id} | ${relationship.clip_id}`,
    )
    .join("\n");
}

function exchangeFormatForName(name: string): CaptionExchangeFormat {
  const lowercase = name.toLowerCase();
  if (lowercase.endsWith(".srt")) return "srt";
  if (lowercase.endsWith(".vtt")) return "vtt";
  throw new Error("Caption import accepts only .srt and .vtt files.");
}

function randomTypedId(kind: "track" | "gap" | "caption"): string {
  return `${kind}:${globalThis.crypto.randomUUID().replaceAll("-", "")}`;
}

function downloadCaptionSource(
  source: string,
  filename: string,
  format: CaptionExchangeFormat,
): void {
  const blob = new Blob([source], {
    type: format === "vtt" ? "text/vtt;charset=utf-8" : "application/x-subrip;charset=utf-8",
  });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = filename;
  anchor.click();
  queueMicrotask(() => URL.revokeObjectURL(url));
}

function safeFilename(value: string): string {
  const filename = value.trim().replace(/[^A-Za-z0-9._-]+/g, "-");
  return filename.length === 0 ? "captions" : filename;
}

function captionFailure(error: unknown): string {
  return error instanceof Error
    ? error.message
    : "The caption operation could not be completed.";
}
