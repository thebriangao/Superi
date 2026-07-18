use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration as StdDuration;

use serde::{Deserialize, Serialize};
use superi_core::ids::MediaId;
use superi_core::time::{RationalTime, Timebase};
use superi_media_io::backend::FallbackPolicy;
use superi_media_io::demux::{
    MediaSource, SeekMode, SeekRequest, SourceLocation, SourceProbeLimits, SourceRequest,
    StreamInfo, StreamKind,
};
use superi_media_io::operation::{MediaPriority, OperationContext};
use tauri::State;

use super::*;

const SOURCE_LOAD_TIMEOUT: StdDuration = StdDuration::from_secs(30);
const SOURCE_SEEK_TIMEOUT: StdDuration = StdDuration::from_secs(10);

/// Exact source-monitor coordinate without floating point or assumed frame rate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMonitorTime {
    pub value: i64,
    pub timebase_numerator: u32,
    pub timebase_denominator: u32,
}

impl SourceMonitorTime {
    fn from_rational(value: RationalTime) -> Self {
        Self {
            value: value.value(),
            timebase_numerator: value.timebase().numerator(),
            timebase_denominator: value.timebase().denominator(),
        }
    }

    fn rational(self) -> Result<RationalTime, DesktopProjectFailure> {
        let timebase = Timebase::new(self.timebase_numerator, self.timebase_denominator)
            .map_err(|_| source_monitor_time_invalid())?;
        if timebase.numerator() != self.timebase_numerator
            || timebase.denominator() != self.timebase_denominator
        {
            return Err(source_monitor_time_invalid());
        }
        Ok(RationalTime::new(self.value, timebase))
    }
}

/// Durable in and out mark intent bound to the source identity on which it was authored.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMonitorMarks {
    pub source_fingerprint: Option<String>,
    pub in_mark: Option<SourceMonitorTime>,
    pub out_mark: Option<SourceMonitorTime>,
}

/// One selected source stream projected without packet or frame payloads.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMonitorStream {
    stream_id: u32,
    kind: String,
    codec: String,
    timebase_numerator: u32,
    timebase_denominator: u32,
}

/// Honest retained source-session state.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceMonitorEngineState {
    Empty,
    Ready,
    Stale,
}

/// Complete source-monitor replacement snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMonitorSnapshot {
    monitor_revision: u64,
    engine_state: SourceMonitorEngineState,
    project_id: Option<String>,
    project_revision: Option<u64>,
    library_revision: Option<u64>,
    media_id: Option<String>,
    media_name: Option<String>,
    source_fingerprint: Option<String>,
    opened_fingerprint: Option<String>,
    backend_id: Option<String>,
    container_id: Option<String>,
    stream: Option<SourceMonitorStream>,
    current: Option<SourceMonitorTime>,
    duration: Option<SourceMonitorTime>,
    range_start: Option<SourceMonitorTime>,
    range_end: Option<SourceMonitorTime>,
    marks: SourceMonitorMarks,
    marks_fresh: bool,
    presentation_note: String,
}

impl SourceMonitorSnapshot {
    #[must_use]
    pub const fn monitor_revision(&self) -> u64 {
        self.monitor_revision
    }

    #[must_use]
    pub const fn engine_state(&self) -> SourceMonitorEngineState {
        self.engine_state
    }

    #[must_use]
    pub const fn current(&self) -> Option<SourceMonitorTime> {
        self.current
    }

    #[must_use]
    pub const fn marks(&self) -> &SourceMonitorMarks {
        &self.marks
    }

    #[must_use]
    pub const fn marks_fresh(&self) -> bool {
        self.marks_fresh
    }
}

/// Revision and fingerprint fences for one source load.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMonitorLoadRequest {
    pub expected_project_revision: u64,
    pub expected_library_revision: u64,
    pub media_id: String,
    pub expected_source_fingerprint: String,
}

/// Exact seek against one retained monitor revision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMonitorSeekRequest {
    pub expected_project_revision: u64,
    pub expected_library_revision: u64,
    pub expected_monitor_revision: u64,
    pub target: SourceMonitorTime,
}

/// Reversible mark operation at the current exact source coordinate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceMonitorMarkMutation {
    SetIn,
    SetOut,
    ClearIn,
    ClearOut,
}

/// Optimistic mark update across the project, media library, and source monitor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMonitorMarkUpdate {
    pub expected_project_revision: u64,
    pub expected_library_revision: u64,
    pub expected_monitor_revision: u64,
    pub mutation: SourceMonitorMarkMutation,
}

/// Optimistic unload of one retained monitor session.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMonitorUnloadRequest {
    pub expected_monitor_revision: u64,
}

/// Atomic durable mark publication plus the resulting source-monitor state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceMonitorUpdateResult {
    monitor: SourceMonitorSnapshot,
    media_library: MediaLibrarySnapshot,
}

impl SourceMonitorUpdateResult {
    #[must_use]
    pub const fn monitor(&self) -> &SourceMonitorSnapshot {
        &self.monitor
    }

    #[must_use]
    pub const fn media_library(&self) -> &MediaLibrarySnapshot {
        &self.media_library
    }
}

#[derive(Default)]
pub(super) struct SourceMonitorRuntime {
    revision: u64,
    loaded: Option<LoadedSource>,
}

struct LoadedSource {
    project_id: String,
    project_revision: u64,
    library_revision: u64,
    media_id: String,
    media_name: String,
    source_fingerprint: String,
    opened_fingerprint: String,
    backend_id: String,
    container_id: String,
    stream: SourceMonitorStream,
    current: RationalTime,
    duration: Option<RationalTime>,
    range_start: Option<RationalTime>,
    range_end: Option<RationalTime>,
    session: SourceMonitorSession,
}

enum SourceMonitorSession {
    Container(Box<dyn MediaSource>),
    ImageRange {
        start: RationalTime,
        end: RationalTime,
    },
}

struct LiveMonitorContext {
    project_id: String,
    project_revision: u64,
    library: MediaLibrarySnapshot,
}

impl DesktopProjectState {
    /// Returns the current retained source session without opening media.
    pub fn source_monitor_snapshot(&self) -> Result<SourceMonitorSnapshot, DesktopProjectFailure> {
        let live = self.source_monitor_live_context("source_monitor_snapshot")?;
        let runtime = self.source_monitor_lock("source_monitor_snapshot")?;
        Ok(snapshot_from_runtime(&runtime, live.as_ref()))
    }

    /// Opens and retains one exact source session after optimistic identity checks.
    pub fn source_monitor_load(
        &self,
        request: SourceMonitorLoadRequest,
    ) -> Result<SourceMonitorSnapshot, DesktopProjectFailure> {
        let (project_id, project_revision) = self.active_project_identity("source_monitor_load")?;
        let item = {
            let store = self.media_library_lock("source_monitor_load")?;
            let library = store.projects.get(&project_id).ok_or_else(|| {
                source_monitor_invalid(
                    "source_monitor_library_missing",
                    "Source monitor media library is unavailable",
                    "Refresh the project media library and try again.",
                )
            })?;
            validate_load_fences(&request, project_revision, library)?;
            library
                .items
                .iter()
                .find(|item| item.media_id == request.media_id)
                .cloned()
                .ok_or_else(|| {
                    source_monitor_invalid(
                        "source_monitor_media_missing",
                        "Source monitor media is unavailable",
                        "Choose media that is still present in the project.",
                    )
                })?
        };
        if item.content_fingerprint != request.expected_source_fingerprint {
            return Err(source_monitor_stale(
                "Source monitor source identity changed",
            ));
        }
        if item.source_monitoring.has_changed_source() {
            return Err(source_monitor_stale(
                "Source monitor source bytes require review",
            ));
        }
        validate_marks(&item.source_monitor_marks)?;

        let prepared = prepare_source(&item)?;

        let (current_project_id, current_project_revision) =
            self.active_project_identity("source_monitor_load_publish")?;
        if current_project_id != project_id || current_project_revision != project_revision {
            return Err(source_monitor_stale(
                "Project changed while source monitor loaded",
            ));
        }
        let store = self.media_library_lock("source_monitor_load_publish")?;
        let library = store.projects.get(&project_id).ok_or_else(|| {
            source_monitor_stale("Media library changed while source monitor loaded")
        })?;
        validate_load_fences(&request, current_project_revision, library)?;
        let current_item = library
            .items
            .iter()
            .find(|candidate| candidate.media_id == item.media_id)
            .ok_or_else(|| source_monitor_stale("Source monitor media was removed"))?;
        if current_item.content_fingerprint != item.content_fingerprint {
            return Err(source_monitor_stale(
                "Source monitor source identity changed",
            ));
        }
        let live = LiveMonitorContext {
            project_id: project_id.clone(),
            project_revision: current_project_revision,
            library: library.clone(),
        };

        let mut runtime = self.source_monitor_lock("source_monitor_load_publish")?;
        runtime.revision = next_monitor_revision(runtime.revision)?;
        runtime.loaded = Some(prepared.finish(
            project_id,
            project_revision,
            request.expected_library_revision,
            item,
        ));
        Ok(snapshot_from_runtime(&runtime, Some(&live)))
    }

    /// Performs one exact seek through the retained source owner.
    pub fn source_monitor_seek(
        &self,
        request: SourceMonitorSeekRequest,
    ) -> Result<SourceMonitorSnapshot, DesktopProjectFailure> {
        let live = self
            .source_monitor_live_context("source_monitor_seek")?
            .ok_or_else(|| source_monitor_stale("No project is open for source monitor seek"))?;
        validate_runtime_fences(
            request.expected_project_revision,
            request.expected_library_revision,
            &live,
        )?;
        let target = request.target.rational()?;
        let operation = interactive_operation(SOURCE_SEEK_TIMEOUT, "source_monitor_seek")?;
        let mut runtime = self.source_monitor_lock("source_monitor_seek")?;
        require_monitor_revision(&runtime, request.expected_monitor_revision)?;
        require_loaded_fresh(&runtime, &live)?;
        {
            let loaded = runtime
                .loaded
                .as_mut()
                .expect("fresh source monitor is loaded");
            if target.timebase().numerator() != loaded.stream.timebase_numerator
                || target.timebase().denominator() != loaded.stream.timebase_denominator
            {
                return Err(source_monitor_time_invalid());
            }
            if target.is_negative() {
                return Err(source_monitor_invalid(
                    "source_monitor_seek_invalid",
                    "Source monitor seek is outside the source range",
                    "Choose an exact nonnegative source position.",
                ));
            }
            loaded.current = match &mut loaded.session {
                SourceMonitorSession::Container(source) => source
                    .seek(SeekRequest::new(target, SeekMode::Exact), &operation)
                    .map_err(|error| safe_failure("source_monitor_seek", error))?,
                SourceMonitorSession::ImageRange { start, end } => {
                    if target < *start || target > *end {
                        return Err(source_monitor_invalid(
                            "source_monitor_seek_invalid",
                            "Source monitor seek is outside the source range",
                            "Choose a position within the displayed source range.",
                        ));
                    }
                    target
                }
            };
        }
        runtime.revision = next_monitor_revision(runtime.revision)?;
        Ok(snapshot_from_runtime(&runtime, Some(&live)))
    }

    /// Publishes one reversible mark edit at the retained exact source coordinate.
    pub fn source_monitor_update_marks(
        &self,
        update: SourceMonitorMarkUpdate,
    ) -> Result<SourceMonitorUpdateResult, DesktopProjectFailure> {
        let live = self
            .source_monitor_live_context("source_monitor_update_marks")?
            .ok_or_else(|| source_monitor_stale("No project is open for source monitor marks"))?;
        validate_runtime_fences(
            update.expected_project_revision,
            update.expected_library_revision,
            &live,
        )?;
        let path = self
            .media_library_path_lock("source_monitor_update_marks")?
            .clone()
            .ok_or_else(not_initialized)?;

        let mut store = self.media_library_lock("source_monitor_update_marks")?;
        let mut runtime = self.source_monitor_lock("source_monitor_update_marks")?;
        require_monitor_revision(&runtime, update.expected_monitor_revision)?;
        require_loaded_fresh(&runtime, &live)?;
        let loaded = runtime
            .loaded
            .as_ref()
            .expect("fresh source monitor is loaded");
        let media_id = loaded.media_id.clone();
        let source_fingerprint = loaded.source_fingerprint.clone();
        let current = SourceMonitorTime::from_rational(loaded.current);

        let mut candidate_store = store.clone();
        let candidate = candidate_store
            .projects
            .get_mut(&live.project_id)
            .ok_or_else(|| source_monitor_stale("Media library changed before mark publication"))?;
        if candidate.revision != update.expected_library_revision
            || candidate.project_revision != update.expected_project_revision
        {
            return Err(source_monitor_stale(
                "Media library changed before mark publication",
            ));
        }
        let item = candidate
            .items
            .iter_mut()
            .find(|item| item.media_id == media_id)
            .ok_or_else(|| source_monitor_stale("Source monitor media was removed"))?;
        if item.content_fingerprint != source_fingerprint {
            return Err(source_monitor_stale(
                "Source monitor source identity changed",
            ));
        }

        let mut marks = item.source_monitor_marks.clone();
        match update.mutation {
            SourceMonitorMarkMutation::SetIn => {
                marks.source_fingerprint = Some(source_fingerprint.clone());
                marks.in_mark = Some(current);
            }
            SourceMonitorMarkMutation::SetOut => {
                marks.source_fingerprint = Some(source_fingerprint.clone());
                marks.out_mark = Some(current);
            }
            SourceMonitorMarkMutation::ClearIn => marks.in_mark = None,
            SourceMonitorMarkMutation::ClearOut => marks.out_mark = None,
        }
        if marks.in_mark.is_none() && marks.out_mark.is_none() {
            marks.source_fingerprint = None;
        }
        validate_marks(&marks)?;
        item.source_monitor_marks = marks;
        candidate.validate()?;
        candidate.revision = candidate.revision.checked_add(1).ok_or_else(|| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "media_library_revision_exhausted",
                "Media library cannot continue",
                "Restart Superi before continuing.",
            )
        })?;
        candidate.refresh_derived();
        let library = candidate.clone();
        publish_media_library_store(&path, &candidate_store)?;
        *store = candidate_store;

        runtime.revision = next_monitor_revision(runtime.revision)?;
        runtime
            .loaded
            .as_mut()
            .expect("fresh source monitor is loaded")
            .library_revision = library.revision;
        let next_live = LiveMonitorContext {
            project_id: live.project_id,
            project_revision: live.project_revision,
            library: library.clone(),
        };
        let monitor = snapshot_from_runtime(&runtime, Some(&next_live));
        Ok(SourceMonitorUpdateResult {
            monitor,
            media_library: library,
        })
    }

    /// Drops one retained source immediately after an optimistic monitor revision check.
    pub fn source_monitor_unload(
        &self,
        request: SourceMonitorUnloadRequest,
    ) -> Result<SourceMonitorSnapshot, DesktopProjectFailure> {
        {
            let mut runtime = self.source_monitor_lock("source_monitor_unload")?;
            require_monitor_revision(&runtime, request.expected_monitor_revision)?;
            runtime.loaded = None;
            runtime.revision = next_monitor_revision(runtime.revision)?;
        }
        self.source_monitor_snapshot()
    }

    pub(super) fn reset_source_monitor(
        &self,
        operation: &'static str,
    ) -> Result<(), DesktopProjectFailure> {
        let mut runtime = self.source_monitor_lock(operation)?;
        if runtime.loaded.take().is_some() {
            runtime.revision = next_monitor_revision(runtime.revision)?;
        }
        Ok(())
    }

    fn source_monitor_live_context(
        &self,
        operation: &'static str,
    ) -> Result<Option<LiveMonitorContext>, DesktopProjectFailure> {
        let (project_id, project_revision) = {
            let lifecycle = self.lock(operation)?;
            let Some(active) = lifecycle
                .as_ref()
                .ok_or_else(not_initialized)?
                .snapshot()
                .active()
            else {
                return Ok(None);
            };
            (active.project_id().to_owned(), active.project_revision())
        };
        let store = self.media_library_lock(operation)?;
        let library = store
            .projects
            .get(&project_id)
            .cloned()
            .unwrap_or_else(|| MediaLibrarySnapshot::empty(project_revision));
        Ok(Some(LiveMonitorContext {
            project_id,
            project_revision,
            library,
        }))
    }

    fn source_monitor_lock(
        &self,
        operation: &'static str,
    ) -> Result<std::sync::MutexGuard<'_, SourceMonitorRuntime>, DesktopProjectFailure> {
        self.source_monitor.lock().map_err(|_| {
            DesktopProjectFailure::new(
                DesktopProjectFailureClass::Terminal,
                "source_monitor_poisoned",
                "Source monitor cannot continue",
                "Restart Superi before continuing.",
            )
            .with_context("operation", operation)
        })
    }
}

struct PreparedSource {
    opened_fingerprint: String,
    backend_id: String,
    container_id: String,
    stream: SourceMonitorStream,
    current: RationalTime,
    duration: Option<RationalTime>,
    range_start: Option<RationalTime>,
    range_end: Option<RationalTime>,
    session: SourceMonitorSession,
}

impl PreparedSource {
    fn finish(
        self,
        project_id: String,
        project_revision: u64,
        library_revision: u64,
        item: MediaBrowserItem,
    ) -> LoadedSource {
        LoadedSource {
            project_id,
            project_revision,
            library_revision,
            media_id: item.media_id,
            media_name: item.name,
            source_fingerprint: item.content_fingerprint,
            opened_fingerprint: self.opened_fingerprint,
            backend_id: self.backend_id,
            container_id: self.container_id,
            stream: self.stream,
            current: self.current,
            duration: self.duration,
            range_start: self.range_start,
            range_end: self.range_end,
            session: self.session,
        }
    }
}

fn prepare_source(item: &MediaBrowserItem) -> Result<PreparedSource, DesktopProjectFailure> {
    let paths = item
        .source_paths
        .iter()
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    if paths.is_empty() || paths.iter().any(|path| !path.is_file()) {
        return Err(source_monitor_invalid(
            "source_monitor_offline",
            "Source monitor media is offline",
            "Relink the source media and try again.",
        ));
    }
    verify_project_fingerprint(&paths, &item.content_fingerprint)?;
    if item.kind == DesktopImportedMediaKind::ImageSequence
        || paths.first().is_some_and(|path| is_still_source(path))
    {
        return prepare_image_range(item, &paths);
    }

    let media_id = MediaId::from_str(&item.media_id).map_err(|_| {
        source_monitor_invalid(
            "source_monitor_media_id_invalid",
            "Source monitor media identity is invalid",
            "Refresh the media library and import the source again.",
        )
    })?;
    let operation = interactive_operation(SOURCE_LOAD_TIMEOUT, "source_monitor_load")?;
    let registry = superi_engine::media::source_backend_registry()
        .map_err(|error| safe_failure("source_monitor_registry", error))?;
    let request = SourceRequest::new(media_id, SourceLocation::Path(paths[0].clone()));
    let selection = registry
        .probe_source(
            request,
            SourceProbeLimits::default(),
            FallbackPolicy::Disallow,
            &operation,
        )
        .map_err(|error| safe_failure("source_monitor_probe", error))?;
    let backend_id = selection
        .primary()
        .backend()
        .descriptor()
        .id()
        .as_str()
        .to_owned();
    let container_id = selection.primary().container().as_str().to_owned();
    let source = selection
        .open(&operation)
        .map_err(|error| safe_failure("source_monitor_open", error))?;
    verify_project_fingerprint(&paths, &item.content_fingerprint)?;
    let source_info = source.info();
    let selected_stream = select_stream(source_info.streams()).ok_or_else(|| {
        source_monitor_invalid(
            "source_monitor_stream_missing",
            "Source monitor found no inspectable stream",
            "Choose a source with a video or audio stream.",
        )
    })?;
    let timebase = selected_stream.timebase();
    let current = RationalTime::zero(timebase);
    let duration = selected_stream
        .duration()
        .or_else(|| source_info.duration())
        .and_then(|duration| {
            i64::try_from(duration.value())
                .ok()
                .map(|value| RationalTime::new(value, duration.timebase()))
        });
    let stream = source_monitor_stream(selected_stream);
    let opened_fingerprint = source_info.identity().fingerprint().to_owned();
    Ok(PreparedSource {
        opened_fingerprint,
        backend_id,
        container_id,
        stream,
        current,
        duration,
        range_start: Some(RationalTime::zero(timebase)),
        range_end: None,
        session: SourceMonitorSession::Container(source),
    })
}

fn prepare_image_range(
    item: &MediaBrowserItem,
    paths: &[PathBuf],
) -> Result<PreparedSource, DesktopProjectFailure> {
    let timebase = match (item.frame_rate_numerator, item.frame_rate_denominator) {
        (Some(numerator), Some(denominator)) => {
            Timebase::new(numerator, denominator).map_err(|_| source_monitor_time_invalid())?
        }
        _ => Timebase::SECONDS,
    };
    let first = item.first_frame.unwrap_or(0);
    let last = item.last_frame.unwrap_or(first);
    if last < first {
        return Err(source_monitor_time_invalid());
    }
    let count = last
        .checked_sub(first)
        .and_then(|value| value.checked_add(1))
        .ok_or_else(source_monitor_time_invalid)?;
    verify_project_fingerprint(paths, &item.content_fingerprint)?;
    let start = RationalTime::new(first, timebase);
    let end = RationalTime::new(last, timebase);
    let extension = paths
        .first()
        .and_then(|path| path.extension())
        .and_then(|extension| extension.to_str())
        .unwrap_or("image")
        .to_ascii_lowercase();
    Ok(PreparedSource {
        opened_fingerprint: item.content_fingerprint.clone(),
        backend_id: "superi-image".to_owned(),
        container_id: if item.kind == DesktopImportedMediaKind::ImageSequence {
            "image-sequence".to_owned()
        } else {
            "still-image".to_owned()
        },
        stream: SourceMonitorStream {
            stream_id: 0,
            kind: "video".to_owned(),
            codec: extension,
            timebase_numerator: timebase.numerator(),
            timebase_denominator: timebase.denominator(),
        },
        current: start,
        duration: Some(RationalTime::new(count, timebase)),
        range_start: Some(start),
        range_end: Some(end),
        session: SourceMonitorSession::ImageRange { start, end },
    })
}

fn select_stream(streams: &[StreamInfo]) -> Option<&StreamInfo> {
    streams
        .iter()
        .find(|stream| stream.kind() == StreamKind::Video)
        .or_else(|| {
            streams
                .iter()
                .find(|stream| stream.kind() == StreamKind::Audio)
        })
        .or_else(|| streams.first())
}

fn source_monitor_stream(stream: &StreamInfo) -> SourceMonitorStream {
    let kind = match stream.kind() {
        StreamKind::Video => "video",
        StreamKind::Audio => "audio",
        StreamKind::Subtitle => "subtitle",
        StreamKind::Data => "data",
        _ => "unknown",
    };
    SourceMonitorStream {
        stream_id: stream.id().value(),
        kind: kind.to_owned(),
        codec: stream.codec().as_str().to_owned(),
        timebase_numerator: stream.timebase().numerator(),
        timebase_denominator: stream.timebase().denominator(),
    }
}

fn snapshot_from_runtime(
    runtime: &SourceMonitorRuntime,
    live: Option<&LiveMonitorContext>,
) -> SourceMonitorSnapshot {
    let Some(loaded) = runtime.loaded.as_ref() else {
        return SourceMonitorSnapshot {
            monitor_revision: runtime.revision,
            engine_state: SourceMonitorEngineState::Empty,
            project_id: live.map(|live| live.project_id.clone()),
            project_revision: live.map(|live| live.project_revision),
            library_revision: live.map(|live| live.library.revision),
            media_id: None,
            media_name: None,
            source_fingerprint: None,
            opened_fingerprint: None,
            backend_id: None,
            container_id: None,
            stream: None,
            current: None,
            duration: None,
            range_start: None,
            range_end: None,
            marks: SourceMonitorMarks::default(),
            marks_fresh: false,
            presentation_note: presentation_note(),
        };
    };
    let live_item = live
        .filter(|live| live.project_id == loaded.project_id)
        .and_then(|live| {
            live.library
                .items
                .iter()
                .find(|item| item.media_id == loaded.media_id)
        });
    let ready = live_item.is_some_and(|item| {
        item.content_fingerprint == loaded.source_fingerprint
            && !item.source_monitoring.has_changed_source()
    });
    let marks = live_item
        .map(|item| item.source_monitor_marks.clone())
        .unwrap_or_default();
    let marks_fresh = ready
        && (marks.in_mark.is_some() || marks.out_mark.is_some())
        && marks.source_fingerprint.as_ref() == Some(&loaded.source_fingerprint);
    SourceMonitorSnapshot {
        monitor_revision: runtime.revision,
        engine_state: if ready {
            SourceMonitorEngineState::Ready
        } else {
            SourceMonitorEngineState::Stale
        },
        project_id: Some(loaded.project_id.clone()),
        project_revision: live.map_or(Some(loaded.project_revision), |live| {
            Some(live.project_revision)
        }),
        library_revision: live.map_or(Some(loaded.library_revision), |live| {
            Some(live.library.revision)
        }),
        media_id: Some(loaded.media_id.clone()),
        media_name: Some(loaded.media_name.clone()),
        source_fingerprint: Some(loaded.source_fingerprint.clone()),
        opened_fingerprint: Some(loaded.opened_fingerprint.clone()),
        backend_id: Some(loaded.backend_id.clone()),
        container_id: Some(loaded.container_id.clone()),
        stream: Some(loaded.stream.clone()),
        current: Some(SourceMonitorTime::from_rational(loaded.current)),
        duration: loaded.duration.map(SourceMonitorTime::from_rational),
        range_start: loaded.range_start.map(SourceMonitorTime::from_rational),
        range_end: loaded.range_end.map(SourceMonitorTime::from_rational),
        marks,
        marks_fresh,
        presentation_note: presentation_note(),
    }
}

fn presentation_note() -> String {
    "Source session state only. Decode and native GPU viewer presentation remain separate."
        .to_owned()
}

fn validate_load_fences(
    request: &SourceMonitorLoadRequest,
    project_revision: u64,
    library: &MediaLibrarySnapshot,
) -> Result<(), DesktopProjectFailure> {
    if request.expected_project_revision != project_revision
        || request.expected_project_revision != library.project_revision
        || request.expected_library_revision != library.revision
    {
        return Err(source_monitor_stale("Source monitor load request is stale"));
    }
    Ok(())
}

fn validate_runtime_fences(
    expected_project_revision: u64,
    expected_library_revision: u64,
    live: &LiveMonitorContext,
) -> Result<(), DesktopProjectFailure> {
    if expected_project_revision != live.project_revision
        || expected_project_revision != live.library.project_revision
        || expected_library_revision != live.library.revision
    {
        return Err(source_monitor_stale("Source monitor request is stale"));
    }
    Ok(())
}

fn require_monitor_revision(
    runtime: &SourceMonitorRuntime,
    expected: u64,
) -> Result<(), DesktopProjectFailure> {
    if runtime.revision != expected {
        return Err(source_monitor_stale("Source monitor state changed"));
    }
    Ok(())
}

fn require_loaded_fresh(
    runtime: &SourceMonitorRuntime,
    live: &LiveMonitorContext,
) -> Result<(), DesktopProjectFailure> {
    let loaded = runtime.loaded.as_ref().ok_or_else(|| {
        source_monitor_invalid(
            "source_monitor_empty",
            "No source is loaded",
            "Load project media in the source monitor first.",
        )
    })?;
    let item = live
        .library
        .items
        .iter()
        .find(|item| item.media_id == loaded.media_id)
        .ok_or_else(|| source_monitor_stale("Source monitor media was removed"))?;
    if loaded.project_id != live.project_id
        || item.content_fingerprint != loaded.source_fingerprint
        || item.source_monitoring.has_changed_source()
    {
        return Err(source_monitor_stale(
            "Source monitor source identity changed",
        ));
    }
    Ok(())
}

pub(super) fn validate_marks(marks: &SourceMonitorMarks) -> Result<(), DesktopProjectFailure> {
    let has_marks = marks.in_mark.is_some() || marks.out_mark.is_some();
    if has_marks != marks.source_fingerprint.is_some()
        || marks
            .source_fingerprint
            .as_ref()
            .is_some_and(|fingerprint| fingerprint.trim().is_empty())
    {
        return Err(source_monitor_invalid(
            "source_monitor_marks_invalid",
            "Source monitor marks are invalid",
            "Clear the marks and set them again on the current source.",
        ));
    }
    let in_mark = marks.in_mark.map(SourceMonitorTime::rational).transpose()?;
    let out_mark = marks
        .out_mark
        .map(SourceMonitorTime::rational)
        .transpose()?;
    if in_mark
        .zip(out_mark)
        .is_some_and(|(in_mark, out_mark)| in_mark > out_mark)
    {
        return Err(source_monitor_invalid(
            "source_monitor_marks_reversed",
            "Source monitor in mark is after the out mark",
            "Move or clear one mark before setting this position.",
        ));
    }
    Ok(())
}

fn verify_project_fingerprint(
    paths: &[PathBuf],
    expected: &str,
) -> Result<(), DesktopProjectFailure> {
    let (actual, _) = fingerprint_sources(paths)?;
    if actual != expected {
        return Err(source_monitor_stale(
            "Source monitor source identity changed",
        ));
    }
    Ok(())
}

fn is_still_source(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "exr" | "tif" | "tiff" | "dpx"
            )
        })
        .unwrap_or(false)
}

fn interactive_operation(
    timeout: StdDuration,
    operation: &'static str,
) -> Result<OperationContext, DesktopProjectFailure> {
    OperationContext::new(MediaPriority::Interactive)
        .with_timeout(timeout)
        .map_err(|error| safe_failure(operation, error))
}

fn next_monitor_revision(revision: u64) -> Result<u64, DesktopProjectFailure> {
    revision.checked_add(1).ok_or_else(|| {
        DesktopProjectFailure::new(
            DesktopProjectFailureClass::Terminal,
            "source_monitor_revision_exhausted",
            "Source monitor cannot continue",
            "Restart Superi before continuing.",
        )
    })
}

fn source_monitor_time_invalid() -> DesktopProjectFailure {
    source_monitor_invalid(
        "source_monitor_time_invalid",
        "Source monitor time is invalid",
        "Use the exact timebase and coordinate shown by the loaded source.",
    )
}

fn source_monitor_stale(title: &'static str) -> DesktopProjectFailure {
    source_monitor_invalid(
        "source_monitor_stale",
        title,
        "Refresh the media library and source monitor, then try again.",
    )
}

fn source_monitor_invalid(
    code: &'static str,
    title: &'static str,
    action: &'static str,
) -> DesktopProjectFailure {
    user_correctable(code, title, action)
}

#[tauri::command]
pub async fn desktop_source_monitor_snapshot(
    state: State<'_, DesktopProjectState>,
) -> Result<SourceMonitorSnapshot, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.source_monitor_snapshot())
        .await
        .map_err(|_| project_task_failed("source_monitor_snapshot"))?
}

#[tauri::command]
pub async fn desktop_source_monitor_load(
    request: SourceMonitorLoadRequest,
    state: State<'_, DesktopProjectState>,
) -> Result<SourceMonitorSnapshot, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.source_monitor_load(request))
        .await
        .map_err(|_| project_task_failed("source_monitor_load"))?
}

#[tauri::command]
pub async fn desktop_source_monitor_seek(
    request: SourceMonitorSeekRequest,
    state: State<'_, DesktopProjectState>,
) -> Result<SourceMonitorSnapshot, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.source_monitor_seek(request))
        .await
        .map_err(|_| project_task_failed("source_monitor_seek"))?
}

#[tauri::command]
pub async fn desktop_source_monitor_update_marks(
    update: SourceMonitorMarkUpdate,
    state: State<'_, DesktopProjectState>,
) -> Result<SourceMonitorUpdateResult, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.source_monitor_update_marks(update))
        .await
        .map_err(|_| project_task_failed("source_monitor_update_marks"))?
}

#[tauri::command]
pub async fn desktop_source_monitor_unload(
    request: SourceMonitorUnloadRequest,
    state: State<'_, DesktopProjectState>,
) -> Result<SourceMonitorSnapshot, DesktopProjectFailure> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || state.source_monitor_unload(request))
        .await
        .map_err(|_| project_task_failed("source_monitor_unload"))?
}
