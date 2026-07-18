//! Persisted removable-volume and changed-source evidence for desktop media identities.

use std::collections::BTreeMap;
use std::fs::{File, Metadata};
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{media_library_invalid, DesktopImportedMedia, DesktopProjectFailure, MediaBrowserItem};

/// Conservative source-volume classification exposed to the media inspector.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaVolumeKind {
    System,
    Removable,
    Unknown,
}

/// Current availability of the volume root that owns one source path.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaVolumeStatus {
    Mounted,
    Offline,
}

/// Exact volume evidence retained with each source path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaSourceVolume {
    volume_id: String,
    root_path: String,
    kind: MediaVolumeKind,
    status: MediaVolumeStatus,
}

/// File bytes and metadata observed in one stable scan.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaSourceFingerprint {
    pub(super) size_bytes: u64,
    pub(super) modified_unix_seconds: Option<u64>,
    pub(super) modified_subsec_nanos: Option<u32>,
    pub(super) content_fingerprint: String,
}

impl MediaSourceFingerprint {
    fn metadata_matches(&self, other: &Self) -> bool {
        self.size_bytes == other.size_bytes
            && self.modified_unix_seconds.is_some()
            && self.modified_unix_seconds == other.modified_unix_seconds
            && self.modified_subsec_nanos == other.modified_subsec_nanos
    }
}

/// Current relationship between a source path and its accepted byte baseline.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaSourcePathStatus {
    Unchecked,
    Unchanged,
    Changed,
    Missing,
    VolumeOffline,
    Unavailable,
}

/// Explicit editor action retained while source availability or bytes need attention.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaRelinkIntent {
    #[default]
    None,
    WaitForVolume,
    LocateSource,
    ReviewChangedSource,
    RetryInspection,
}

/// Overall state of one imported identity's current source evidence.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaSourceMonitoringStatus {
    #[default]
    Unchecked,
    Ready,
    Attention,
}

/// Persisted baseline and latest observation for one authoritative source path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaSourcePathState {
    path: String,
    volume: MediaSourceVolume,
    status: MediaSourcePathStatus,
    baseline: Option<MediaSourceFingerprint>,
    observed: Option<MediaSourceFingerprint>,
    detail: Option<String>,
}

/// Persisted source-change state attached to one stable media identity.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaSourceMonitoring {
    status: MediaSourceMonitoringStatus,
    scan_generation: u64,
    expected_content_fingerprint: String,
    relink_intent: MediaRelinkIntent,
    paths: Vec<MediaSourcePathState>,
}

/// Revision-fenced request for selected media IDs, or every item when IDs are empty.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MediaSourceScanRequest {
    pub expected_project_revision: u64,
    pub expected_library_revision: u64,
    pub media_ids: Vec<String>,
    pub verify_content: bool,
}

impl MediaSourceMonitoring {
    pub(super) fn from_imported(media: &DesktopImportedMedia) -> Self {
        if media.source_fingerprints.len() != media.source_paths.len() {
            return Self::unscanned(&media.source_paths, &media.content_fingerprint);
        }
        let paths = media
            .source_paths
            .iter()
            .cloned()
            .zip(media.source_fingerprints.iter().cloned())
            .map(|(path, fingerprint)| MediaSourcePathState {
                volume: volume_for_path(&path),
                path,
                status: MediaSourcePathStatus::Unchanged,
                baseline: Some(fingerprint.clone()),
                observed: Some(fingerprint),
                detail: None,
            })
            .collect();
        Self {
            status: MediaSourceMonitoringStatus::Ready,
            scan_generation: 0,
            expected_content_fingerprint: media.content_fingerprint.clone(),
            relink_intent: MediaRelinkIntent::None,
            paths,
        }
    }

    pub(super) fn unscanned(paths: &[String], expected_content_fingerprint: &str) -> Self {
        Self {
            status: MediaSourceMonitoringStatus::Unchecked,
            scan_generation: 0,
            expected_content_fingerprint: expected_content_fingerprint.to_owned(),
            relink_intent: MediaRelinkIntent::None,
            paths: paths
                .iter()
                .cloned()
                .map(|path| MediaSourcePathState {
                    volume: volume_for_path(&path),
                    path,
                    status: MediaSourcePathStatus::Unchecked,
                    baseline: None,
                    observed: None,
                    detail: None,
                })
                .collect(),
        }
    }

    pub(super) fn reconcile(&mut self, paths: &[String], expected_content_fingerprint: &str) {
        let same_paths = self
            .paths
            .iter()
            .map(|state| state.path.as_str())
            .eq(paths.iter().map(String::as_str));
        if self.expected_content_fingerprint != expected_content_fingerprint || !same_paths {
            *self = Self::unscanned(paths, expected_content_fingerprint);
        }
    }

    pub(super) fn has_changed_source(&self) -> bool {
        self.paths
            .iter()
            .any(|path| path.status == MediaSourcePathStatus::Changed)
    }
}

pub(super) fn scan_item(
    item: &mut MediaBrowserItem,
    verify_content: bool,
) -> Result<(), DesktopProjectFailure> {
    item.source_monitoring
        .reconcile(&item.source_paths, &item.content_fingerprint);
    let generation = item
        .source_monitoring
        .scan_generation
        .checked_add(1)
        .ok_or_else(|| media_library_invalid("Source scan generation is exhausted"))?;
    let previous = item
        .source_monitoring
        .paths
        .iter()
        .map(|state| (state.path.clone(), state.clone()))
        .collect::<BTreeMap<_, _>>();
    let mut probed = item
        .source_paths
        .iter()
        .map(|path| probe_path(path, previous.get(path)))
        .collect::<Vec<_>>();

    let all_files = probed
        .iter()
        .all(|probe| matches!(probe, PathProbe::File { .. }));
    let needs_baseline = probed.iter().any(|probe| match probe {
        PathProbe::File { previous, .. } => previous
            .as_ref()
            .and_then(|state| state.baseline.as_ref())
            .is_none(),
        PathProbe::State(_) => false,
    });

    let paths = if all_files && needs_baseline {
        let native_paths = item
            .source_paths
            .iter()
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        match fingerprint_sources(&native_paths) {
            Ok((aggregate, fingerprints)) => probed
                .drain(..)
                .zip(fingerprints)
                .map(|(probe, observed)| {
                    let PathProbe::File {
                        path,
                        volume,
                        previous,
                        ..
                    } = probe
                    else {
                        unreachable!("all source probes were files")
                    };
                    let baseline = previous.and_then(|state| state.baseline);
                    let accepted = aggregate == item.content_fingerprint;
                    let unchanged = accepted
                        || baseline.as_ref().is_some_and(|value| {
                            value.content_fingerprint == observed.content_fingerprint
                        });
                    MediaSourcePathState {
                        path,
                        volume,
                        status: if unchanged {
                            MediaSourcePathStatus::Unchanged
                        } else {
                            MediaSourcePathStatus::Changed
                        },
                        baseline: if accepted {
                            Some(observed.clone())
                        } else {
                            baseline
                        },
                        observed: Some(observed),
                        detail: (!unchanged).then(|| {
                            "Source bytes differ from the accepted media identity.".to_owned()
                        }),
                    }
                })
                .collect(),
            Err(_) => probed.drain(..).map(unavailable_after_read).collect(),
        }
    } else {
        probed
            .drain(..)
            .map(|probe| scan_probe(probe, verify_content))
            .collect()
    };

    item.source_monitoring = MediaSourceMonitoring {
        status: MediaSourceMonitoringStatus::Unchecked,
        scan_generation: generation,
        expected_content_fingerprint: item.content_fingerprint.clone(),
        relink_intent: MediaRelinkIntent::None,
        paths,
    };
    refresh_summary(&mut item.source_monitoring);
    Ok(())
}

pub(super) fn validate(
    monitoring: &MediaSourceMonitoring,
    source_paths: &[String],
    content_fingerprint: &str,
) -> Result<(), DesktopProjectFailure> {
    if monitoring.expected_content_fingerprint != content_fingerprint
        || monitoring.paths.len() != source_paths.len()
        || !monitoring
            .paths
            .iter()
            .zip(source_paths)
            .all(|(state, path)| state.path == *path)
    {
        return Err(media_library_invalid(
            "Source monitoring does not match media identity",
        ));
    }
    for state in &monitoring.paths {
        if state.volume.volume_id.trim().is_empty() || state.volume.root_path.trim().is_empty() {
            return Err(media_library_invalid("Source volume identity is invalid"));
        }
        for fingerprint in [state.baseline.as_ref(), state.observed.as_ref()]
            .into_iter()
            .flatten()
        {
            if !valid_fingerprint(&fingerprint.content_fingerprint) {
                return Err(media_library_invalid("Source fingerprint is invalid"));
            }
        }
        let fingerprints_match = state
            .baseline
            .as_ref()
            .zip(state.observed.as_ref())
            .is_some_and(|(baseline, observed)| {
                baseline.content_fingerprint == observed.content_fingerprint
            });
        if (state.status == MediaSourcePathStatus::Unchanged && !fingerprints_match)
            || (state.status == MediaSourcePathStatus::Changed
                && (fingerprints_match || state.observed.is_none()))
            || (matches!(
                state.status,
                MediaSourcePathStatus::Missing
                    | MediaSourcePathStatus::VolumeOffline
                    | MediaSourcePathStatus::Unavailable
            ) && state.observed.is_some())
            || (state.status == MediaSourcePathStatus::VolumeOffline
                && (state.volume.kind != MediaVolumeKind::Removable
                    || state.volume.status != MediaVolumeStatus::Offline))
        {
            return Err(media_library_invalid(
                "Source path monitoring is inconsistent",
            ));
        }
    }
    let mut canonical = monitoring.clone();
    refresh_summary(&mut canonical);
    if canonical.status != monitoring.status || canonical.relink_intent != monitoring.relink_intent
    {
        return Err(media_library_invalid(
            "Source monitoring summary is inconsistent",
        ));
    }
    Ok(())
}

pub(super) fn fingerprint_sources(
    paths: &[PathBuf],
) -> io::Result<(String, Vec<MediaSourceFingerprint>)> {
    let mut aggregate = Sha256::new();
    let mut fingerprints = Vec::with_capacity(paths.len());
    for path in paths {
        fingerprints.push(fingerprint_path_with_aggregate(path, Some(&mut aggregate))?);
    }
    Ok((
        format!("sha256:{}", hex_bytes(&aggregate.finalize())),
        fingerprints,
    ))
}

enum PathProbe {
    File {
        path: String,
        volume: MediaSourceVolume,
        metadata: MediaSourceFingerprint,
        previous: Option<MediaSourcePathState>,
    },
    State(MediaSourcePathState),
}

fn probe_path(path: &str, previous: Option<&MediaSourcePathState>) -> PathProbe {
    let volume = volume_for_path(path);
    let baseline = previous.and_then(|state| state.baseline.clone());
    if volume.status == MediaVolumeStatus::Offline {
        return PathProbe::State(MediaSourcePathState {
            path: path.to_owned(),
            volume,
            status: MediaSourcePathStatus::VolumeOffline,
            baseline,
            observed: None,
            detail: Some("The removable source volume is not mounted.".to_owned()),
        });
    }
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => PathProbe::File {
            path: path.to_owned(),
            volume,
            metadata: metadata_fingerprint(&metadata, String::new()),
            previous: previous.cloned(),
        },
        Ok(_) => PathProbe::State(MediaSourcePathState {
            path: path.to_owned(),
            volume,
            status: MediaSourcePathStatus::Unavailable,
            baseline,
            observed: None,
            detail: Some("The source path is not a regular file.".to_owned()),
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            PathProbe::State(MediaSourcePathState {
                path: path.to_owned(),
                volume,
                status: MediaSourcePathStatus::Missing,
                baseline,
                observed: None,
                detail: Some("The source file is missing from its mounted volume.".to_owned()),
            })
        }
        Err(_) => PathProbe::State(MediaSourcePathState {
            path: path.to_owned(),
            volume,
            status: MediaSourcePathStatus::Unavailable,
            baseline,
            observed: None,
            detail: Some("The source file could not be inspected.".to_owned()),
        }),
    }
}

fn scan_probe(probe: PathProbe, verify_content: bool) -> MediaSourcePathState {
    let PathProbe::File {
        path,
        volume,
        metadata,
        previous,
    } = probe
    else {
        let PathProbe::State(state) = probe else {
            unreachable!()
        };
        return state;
    };
    let baseline = previous.and_then(|state| state.baseline);
    let has_baseline = baseline.is_some();
    if !verify_content
        && baseline
            .as_ref()
            .is_some_and(|accepted| accepted.metadata_matches(&metadata))
    {
        return MediaSourcePathState {
            path,
            volume,
            status: MediaSourcePathStatus::Unchanged,
            observed: baseline.clone(),
            baseline,
            detail: None,
        };
    }
    match fingerprint_path_with_aggregate(Path::new(&path), None) {
        Ok(observed) => {
            let unchanged = baseline.as_ref().is_some_and(|accepted| {
                accepted.content_fingerprint == observed.content_fingerprint
            });
            MediaSourcePathState {
                path,
                volume,
                status: if unchanged {
                    MediaSourcePathStatus::Unchanged
                } else if has_baseline {
                    MediaSourcePathStatus::Changed
                } else {
                    MediaSourcePathStatus::Unchecked
                },
                baseline: if unchanged {
                    Some(observed.clone())
                } else {
                    baseline
                },
                observed: Some(observed),
                detail: if unchanged || !has_baseline {
                    None
                } else {
                    Some("Source bytes differ from the accepted media identity.".to_owned())
                },
            }
        }
        Err(_) => MediaSourcePathState {
            path,
            volume,
            status: MediaSourcePathStatus::Unavailable,
            baseline,
            observed: None,
            detail: Some("The source changed or became unreadable during inspection.".to_owned()),
        },
    }
}

fn unavailable_after_read(probe: PathProbe) -> MediaSourcePathState {
    match probe {
        PathProbe::State(state) => state,
        PathProbe::File {
            path,
            volume,
            previous,
            ..
        } => MediaSourcePathState {
            path,
            volume,
            status: MediaSourcePathStatus::Unavailable,
            baseline: previous.and_then(|state| state.baseline),
            observed: None,
            detail: Some("The source changed or became unreadable during inspection.".to_owned()),
        },
    }
}

fn refresh_summary(monitoring: &mut MediaSourceMonitoring) {
    let statuses = monitoring
        .paths
        .iter()
        .map(|path| path.status)
        .collect::<Vec<_>>();
    if statuses.contains(&MediaSourcePathStatus::Changed) {
        monitoring.status = MediaSourceMonitoringStatus::Attention;
        monitoring.relink_intent = MediaRelinkIntent::ReviewChangedSource;
    } else if statuses.contains(&MediaSourcePathStatus::VolumeOffline) {
        monitoring.status = MediaSourceMonitoringStatus::Attention;
        monitoring.relink_intent = MediaRelinkIntent::WaitForVolume;
    } else if statuses.contains(&MediaSourcePathStatus::Missing) {
        monitoring.status = MediaSourceMonitoringStatus::Attention;
        monitoring.relink_intent = MediaRelinkIntent::LocateSource;
    } else if statuses.contains(&MediaSourcePathStatus::Unavailable) {
        monitoring.status = MediaSourceMonitoringStatus::Attention;
        monitoring.relink_intent = MediaRelinkIntent::RetryInspection;
    } else if !statuses.is_empty()
        && statuses
            .iter()
            .all(|status| *status == MediaSourcePathStatus::Unchanged)
    {
        monitoring.status = MediaSourceMonitoringStatus::Ready;
        monitoring.relink_intent = MediaRelinkIntent::None;
    } else {
        monitoring.status = MediaSourceMonitoringStatus::Unchecked;
        monitoring.relink_intent = MediaRelinkIntent::None;
    }
}

fn volume_for_path(path: &str) -> MediaSourceVolume {
    let normalized = path.replace('\\', "/");
    let components = normalized
        .trim_start_matches('/')
        .split('/')
        .filter(|component| !component.is_empty())
        .collect::<Vec<_>>();
    let (root_path, kind) = if normalized.starts_with('/')
        && components.first() == Some(&"Volumes")
        && components.len() >= 2
    {
        (
            format!("/Volumes/{}", components[1]),
            MediaVolumeKind::Removable,
        )
    } else if normalized.starts_with('/')
        && components.first() == Some(&"media")
        && components.len() >= 3
    {
        (
            format!("/media/{}/{}", components[1], components[2]),
            MediaVolumeKind::Removable,
        )
    } else if normalized.starts_with("/run/media/") && components.len() >= 4 {
        (
            format!("/run/media/{}/{}", components[2], components[3]),
            MediaVolumeKind::Removable,
        )
    } else if normalized.starts_with('/')
        && components.first() == Some(&"mnt")
        && components.len() >= 2
    {
        (
            format!("/mnt/{}", components[1]),
            MediaVolumeKind::Removable,
        )
    } else if normalized.as_bytes().get(1) == Some(&b':') {
        (format!("{}:\\", &normalized[..1]), MediaVolumeKind::Unknown)
    } else if normalized.starts_with('/') {
        ("/".to_owned(), MediaVolumeKind::System)
    } else {
        ("relative".to_owned(), MediaVolumeKind::Unknown)
    };
    let status = if kind == MediaVolumeKind::Removable && !Path::new(&root_path).is_dir() {
        MediaVolumeStatus::Offline
    } else {
        MediaVolumeStatus::Mounted
    };
    MediaSourceVolume {
        volume_id: root_path.clone(),
        root_path,
        kind,
        status,
    }
}

fn fingerprint_path_with_aggregate(
    path: &Path,
    mut aggregate: Option<&mut Sha256>,
) -> io::Result<MediaSourceFingerprint> {
    let mut file = File::open(path)?;
    let before = file.metadata()?;
    if !before.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "source is not a regular file",
        ));
    }
    let mut individual = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        individual.update(&buffer[..count]);
        if let Some(hasher) = aggregate.as_deref_mut() {
            hasher.update(&buffer[..count]);
        }
    }
    individual.update([0]);
    if let Some(hasher) = aggregate {
        hasher.update([0]);
    }
    let after = file.metadata()?;
    let before_signature = metadata_fingerprint(&before, String::new());
    let after_signature = metadata_fingerprint(&after, String::new());
    if !before_signature.metadata_matches(&after_signature) {
        return Err(io::Error::other("source changed while it was read"));
    }
    Ok(metadata_fingerprint(
        &after,
        format!("sha256:{}", hex_bytes(&individual.finalize())),
    ))
}

fn metadata_fingerprint(
    metadata: &Metadata,
    content_fingerprint: String,
) -> MediaSourceFingerprint {
    let (modified_unix_seconds, modified_subsec_nanos) = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map_or((None, None), |duration| {
            (Some(duration.as_secs()), Some(duration.subsec_nanos()))
        });
    MediaSourceFingerprint {
        size_bytes: metadata.len(),
        modified_unix_seconds,
        modified_subsec_nanos,
        content_fingerprint,
    }
}

fn valid_fingerprint(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..].bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut output, "{byte:02x}").expect("writing to String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{volume_for_path, MediaVolumeKind, MediaVolumeStatus};

    #[test]
    fn conventional_removable_roots_are_conservative_and_stable() {
        let mac = volume_for_path("/Volumes/Camera A/DCIM/clip.mov");
        assert_eq!(mac.kind, MediaVolumeKind::Removable);
        assert_eq!(mac.volume_id, "/Volumes/Camera A");
        assert_eq!(mac.status, MediaVolumeStatus::Offline);

        let linux = volume_for_path("/run/media/editor/RAID/clip.mov");
        assert_eq!(linux.kind, MediaVolumeKind::Removable);
        assert_eq!(linux.volume_id, "/run/media/editor/RAID");

        let system = volume_for_path("/Users/editor/Media/clip.mov");
        assert_eq!(system.kind, MediaVolumeKind::System);
        assert_eq!(system.volume_id, "/");

        let windows = volume_for_path("D:\\Media\\clip.mov");
        assert_eq!(windows.kind, MediaVolumeKind::Unknown);
        assert_eq!(windows.volume_id, "D:\\");
    }
}
