import { invoke } from "@tauri-apps/api/core";

export type DesktopProjectFailureClass =
  | "retryable"
  | "degraded"
  | "user_correctable"
  | "terminal";

export interface DesktopProjectFailure {
  readonly class: DesktopProjectFailureClass;
  readonly code: string;
  readonly title: string;
  readonly action: string;
  readonly context: Readonly<Record<string, string>>;
}

export interface DesktopProjectIdentity {
  readonly project_id: string;
  readonly project_revision: number;
  readonly root_timeline_id: string;
}

export interface DesktopProjectRecord {
  readonly path: string;
  readonly identity: DesktopProjectIdentity;
}

export interface DesktopRecoveryCandidate {
  readonly candidate_id: string;
  readonly project_revision: number;
  readonly action: string;
}

export interface DesktopRecoveryCatalog {
  readonly catalog_revision: number;
  readonly candidates: readonly DesktopRecoveryCandidate[];
}

export interface DesktopProjectSnapshot {
  readonly revision: number;
  readonly active: DesktopProjectRecord | null;
  readonly recent: readonly DesktopProjectRecord[];
  readonly recovery: DesktopRecoveryCatalog | null;
  readonly failure: DesktopProjectFailure | null;
}

export interface DesktopProjectSettings {
  readonly project_revision: number;
  readonly frame_rate_numerator: number;
  readonly frame_rate_denominator: number;
  readonly timecode_mode: string;
  readonly resolution_width: number | null;
  readonly resolution_height: number | null;
  readonly color_mode: string;
  readonly color_working_space: string;
  readonly color_config_id: string | null;
  readonly color_config_fingerprint: string | null;
  readonly audio_sample_rate_hz: number;
  readonly audio_output_layout: string;
  readonly cache_mode: string;
  readonly cache_max_bytes: number | null;
  readonly cache_max_frames: number | null;
  readonly proxy_mode: string;
  readonly proxy_quality: string;
  readonly working_folder: string | null;
  readonly cache_folder: string | null;
  readonly proxy_folder: string | null;
}

export interface DesktopProjectSettingsUpdate {
  readonly expected_project_revision: number;
  readonly frame_rate_numerator: number;
  readonly frame_rate_denominator: number;
  readonly timecode_mode: string;
  readonly resolution_width: number | null;
  readonly resolution_height: number | null;
  readonly color_mode: string;
  readonly color_working_space: string;
  readonly color_config_id: string | null;
  readonly color_config_fingerprint: string | null;
  readonly audio_sample_rate_hz: number;
  readonly audio_output_layout: string;
  readonly cache_mode: string;
  readonly cache_max_bytes: number | null;
  readonly cache_max_frames: number | null;
  readonly proxy_mode: string;
  readonly proxy_quality: string;
  readonly working_folder: string | null;
  readonly cache_folder: string | null;
  readonly proxy_folder: string | null;
}

export interface DesktopProjectCreateRequest {
  readonly project_id: string;
  readonly project_name: string;
  readonly root_timeline_id: string;
  readonly root_timeline_name: string;
  readonly edit_rate_numerator: number;
  readonly edit_rate_denominator: number;
}

export type DesktopMediaImportOrigin =
  | "picker"
  | "drag_drop"
  | "folder_scan"
  | "api"
  | "automation";

export interface DesktopMediaImportRequest {
  readonly expected_project_revision: number;
  readonly origin: DesktopMediaImportOrigin;
  readonly paths: readonly string[];
  readonly recursive: boolean;
  readonly detect_image_sequences: boolean;
}

export interface DesktopImportedMedia {
  readonly media_id: string;
  readonly name: string;
  readonly source_paths: readonly string[];
  readonly content_fingerprint: string;
  readonly kind: "file" | "image_sequence";
  readonly source_count: number;
  readonly first_frame: number | null;
  readonly last_frame: number | null;
  readonly frame_rate_numerator: number | null;
  readonly frame_rate_denominator: number | null;
}

export interface DesktopMediaImportResult {
  readonly project_revision: number;
  readonly imported: readonly DesktopImportedMedia[];
  readonly skipped: readonly string[];
  readonly command_method: "superi.project.command.execute";
  readonly event_name: "superi.project.state.changed";
  readonly event_sequence: number | null;
  readonly automation_method: "superi.project.command.execute" | null;
}

export type ThumbnailPresentation =
  | {
      readonly kind: "source";
      readonly source_path: string;
      readonly freshness: string;
    }
  | {
      readonly kind: "thumbnail_fallback";
      readonly thumbnail_fallback: string;
      readonly freshness: string;
    };

export type SourceMetadataStatus = "ready" | "missing" | "unavailable";

export interface SourceMetadataInspection {
  readonly status: SourceMetadataStatus;
  readonly inspection_generation: number;
  readonly freshness: string;
  readonly fields: Readonly<Record<string, string>>;
}

export interface MediaEditorialAnnotations {
  readonly clip_name: string | null;
  readonly labels: readonly string[];
  readonly rating: number | null;
  readonly keywords: readonly string[];
  readonly comment: string | null;
  readonly favorite: boolean;
}

export interface MediaUsageIndicator {
  readonly clip_count: number;
  readonly timeline_count: number;
  readonly timeline_ids: readonly string[];
}

export interface MediaIdentityTracking {
  readonly canonical_media_id: string;
  readonly content_fingerprint: string;
  readonly duplicate_media_ids: readonly string[];
}

export interface TrackedRegionObservation {
  readonly frame: number;
  readonly x_millionths: number;
  readonly y_millionths: number;
  readonly width_millionths: number;
  readonly height_millionths: number;
}

export interface MediaTrackedRegion {
  readonly region_id: string;
  readonly observations: readonly TrackedRegionObservation[];
}

export interface MediaSelection {
  readonly selection_id: string;
  readonly name: string;
  readonly start_frame: number;
  readonly end_frame: number;
  readonly rate_numerator: number;
  readonly rate_denominator: number;
  readonly tracked_regions: readonly MediaTrackedRegion[];
}

export type DerivedMediaPurpose = "proxy" | "optimized";
export type DerivedMediaQuality = "eighth" | "quarter" | "half" | "full";
export type DerivedMediaStatus = "generating" | "ready" | "stale" | "failed";
export type MediaRepresentationChoice =
  | { readonly kind: "original" }
  | { readonly kind: "proxy"; readonly quality: DerivedMediaQuality }
  | { readonly kind: "optimized"; readonly quality: DerivedMediaQuality };

export interface DerivedMediaAttachment {
  readonly artifact_id: string;
  readonly purpose: DerivedMediaPurpose;
  readonly quality: DerivedMediaQuality;
  readonly status: DerivedMediaStatus;
  readonly source_fingerprint: string;
  readonly source_revision: number;
  readonly byte_len: number;
}

export type OfflineMediaStatus = "online" | "partial" | "offline";

export interface OfflineMediaState {
  readonly status: OfflineMediaStatus;
  readonly available_paths: readonly string[];
  readonly missing_paths: readonly string[];
  readonly derived_fallback_available: boolean;
}

export type OfflineMediaMutation =
  | {
      readonly kind: "relink";
      readonly source_paths: readonly string[];
      readonly candidate_fingerprint: string;
    }
  | {
      readonly kind: "replace";
      readonly source_paths: readonly string[];
      readonly replacement_fingerprint: string;
    }
  | {
      readonly kind: "conform";
      readonly frame_rate_numerator: number;
      readonly frame_rate_denominator: number;
    };

export interface ResolvedMediaRepresentation {
  readonly representation: "original" | DerivedMediaPurpose;
  readonly quality: DerivedMediaQuality | null;
  readonly artifact_id: string | null;
  readonly fallback_to_original: boolean;
}

export type DerivedMediaMutation =
  | ({ readonly kind: "create_or_replace" } & DerivedMediaAttachment)
  | { readonly kind: "set_choice"; readonly choice: MediaRepresentationChoice }
  | {
      readonly kind: "remove";
      readonly purpose: DerivedMediaPurpose;
      readonly quality: DerivedMediaQuality;
    };

export interface MediaBrowserItem {
  readonly media_id: string;
  readonly name: string;
  readonly source_paths: readonly string[];
  readonly content_fingerprint: string;
  readonly kind: "file" | "image_sequence";
  readonly source_count: number;
  readonly first_frame: number | null;
  readonly last_frame: number | null;
  readonly frame_rate_numerator: number | null;
  readonly frame_rate_denominator: number | null;
  readonly bin_id: string | null;
  readonly metadata: Readonly<Record<string, string>>;
  readonly source_metadata: SourceMetadataInspection;
  readonly user_metadata: Readonly<Record<string, string>>;
  readonly annotations: MediaEditorialAnnotations;
  readonly usage: MediaUsageIndicator;
  readonly identity_tracking: MediaIdentityTracking;
  readonly selections: readonly MediaSelection[];
  readonly derived_media: readonly DerivedMediaAttachment[];
  readonly offline: OfflineMediaState;
  readonly representation_choice: MediaRepresentationChoice;
  readonly resolved_representation: ResolvedMediaRepresentation;
  readonly thumbnail: ThumbnailPresentation;
}

export interface MediaBinView {
  readonly bin_id: string;
  readonly name: string;
  readonly parent_id: string | null;
}

export interface SmartCollectionView {
  readonly collection_id: string;
  readonly name: string;
  readonly name_contains: string;
  readonly media_ids: readonly string[];
}

export type MediaLibrarySnapshot = {
  readonly revision: number;
  readonly project_revision: number;
  readonly items: readonly MediaBrowserItem[];
  readonly bins: readonly MediaBinView[];
  readonly smart_collections: readonly SmartCollectionView[];
};

export type MediaLibraryMutation =
  | {
      readonly kind: "create_bin";
      readonly bin_id: string;
      readonly name: string;
      readonly parent_id: string | null;
    }
  | {
      readonly kind: "move_media";
      readonly media_id: string;
      readonly bin_id: string | null;
    }
  | { readonly kind: "remove_bin"; readonly bin_id: string }
  | {
      readonly kind: "upsert_smart_collection";
      readonly collection_id: string;
      readonly name: string;
      readonly name_contains: string;
    }
  | {
      readonly kind: "remove_smart_collection";
      readonly collection_id: string;
    };

export type DesktopProjectCommand =
  | {
      readonly kind: "create";
      readonly path: string;
      readonly project: DesktopProjectCreateRequest;
    }
  | { readonly kind: "open"; readonly path: string }
  | { readonly kind: "open_recent"; readonly path: string }
  | { readonly kind: "save" }
  | {
      readonly kind: "save_as";
      readonly destination: string;
      readonly replace_existing: boolean;
    }
  | { readonly kind: "close" }
  | { readonly kind: "discover_recovery" }
  | {
      readonly kind: "restore_recovery";
      readonly catalog_revision: number;
      readonly candidate_id: string;
    };

export type UserMetadataMutation =
  | {
      readonly kind: "upsert";
      readonly key: string;
      readonly value: string;
    }
  | { readonly kind: "remove"; readonly key: string };

const SNAPSHOT_COMMAND = "desktop_project_snapshot";
const EXECUTE_COMMAND = "desktop_project_execute";
const SETTINGS_COMMAND = "desktop_project_settings";
const UPDATE_SETTINGS_COMMAND = "desktop_project_settings_update";
const IMPORT_MEDIA_COMMAND = "desktop_project_media_import";
const MEDIA_LIBRARY_COMMAND = "project_media_library";
const MUTATE_MEDIA_LIBRARY_COMMAND = "mutate_project_media_library";
const INSPECT_MEDIA_SOURCE_COMMAND = "inspect_project_media_source";
const MUTATE_MEDIA_METADATA_COMMAND = "mutate_project_media_metadata";
const MUTATE_MEDIA_ANNOTATIONS_COMMAND = "mutate_project_media_annotations";
const MUTATE_MEDIA_IDENTITY_COMMAND = "mutate_project_media_identity";
const MUTATE_DERIVED_MEDIA_COMMAND = "mutate_project_derived_media";
const MUTATE_OFFLINE_MEDIA_COMMAND = "mutate_project_offline_media";

export async function getDesktopProjectSnapshot(): Promise<DesktopProjectSnapshot> {
  return invoke<DesktopProjectSnapshot>(SNAPSHOT_COMMAND);
}

export async function executeDesktopProject(
  command: DesktopProjectCommand,
): Promise<DesktopProjectSnapshot> {
  return invoke<DesktopProjectSnapshot>(EXECUTE_COMMAND, { command });
}

export async function getDesktopProjectSettings(): Promise<DesktopProjectSettings> {
  return invoke<DesktopProjectSettings>(SETTINGS_COMMAND);
}

export async function updateDesktopProjectSettings(
  update: DesktopProjectSettingsUpdate,
): Promise<DesktopProjectSettings> {
  return invoke<DesktopProjectSettings>(UPDATE_SETTINGS_COMMAND, { update });
}

export async function importDesktopMedia(
  request: DesktopMediaImportRequest,
): Promise<DesktopMediaImportResult> {
  return invoke<DesktopMediaImportResult>(IMPORT_MEDIA_COMMAND, { request });
}

export async function readProjectMediaLibrary(): Promise<MediaLibrarySnapshot> {
  return invoke<MediaLibrarySnapshot>(MEDIA_LIBRARY_COMMAND);
}

export async function mutateProjectMediaLibrary(
  snapshot: MediaLibrarySnapshot,
  mutation: MediaLibraryMutation,
): Promise<MediaLibrarySnapshot> {
  return invoke<MediaLibrarySnapshot>(MUTATE_MEDIA_LIBRARY_COMMAND, {
    update: {
      expected_project_revision: snapshot.project_revision,
      expected_library_revision: snapshot.revision,
      mutation,
    },
  });
}

export async function inspectProjectMediaSource(
  item: MediaBrowserItem,
): Promise<MediaLibrarySnapshot> {
  return invoke<MediaLibrarySnapshot>(INSPECT_MEDIA_SOURCE_COMMAND, {
    request: {
      media_id: item.media_id,
      expected_freshness: item.content_fingerprint,
    },
  });
}

export async function mutateProjectMediaMetadata(
  snapshot: MediaLibrarySnapshot,
  mediaId: string,
  mutation: UserMetadataMutation,
): Promise<MediaLibrarySnapshot> {
  return invoke<MediaLibrarySnapshot>(MUTATE_MEDIA_METADATA_COMMAND, {
    update: {
      expected_project_revision: snapshot.project_revision,
      expected_library_revision: snapshot.revision,
      media_id: mediaId,
      mutation,
    },
  });
}

export async function mutateProjectMediaAnnotations(
  snapshot: MediaLibrarySnapshot,
  mediaId: string,
  annotations: MediaEditorialAnnotations,
): Promise<MediaLibrarySnapshot> {
  return invoke<MediaLibrarySnapshot>(MUTATE_MEDIA_ANNOTATIONS_COMMAND, {
    update: {
      expected_project_revision: snapshot.project_revision,
      expected_library_revision: snapshot.revision,
      media_id: mediaId,
      annotations,
    },
  });
}

export async function mutateProjectMediaIdentity(
  snapshot: MediaLibrarySnapshot,
  mediaId: string,
  selections: readonly MediaSelection[],
): Promise<MediaLibrarySnapshot> {
  return invoke<MediaLibrarySnapshot>(MUTATE_MEDIA_IDENTITY_COMMAND, {
    update: {
      expected_project_revision: snapshot.project_revision,
      expected_library_revision: snapshot.revision,
      media_id: mediaId,
      selections,
    },
  });
}

export async function mutateProjectDerivedMedia(
  snapshot: MediaLibrarySnapshot,
  mediaId: string,
  mutation: DerivedMediaMutation,
): Promise<MediaLibrarySnapshot> {
  return invoke<MediaLibrarySnapshot>(MUTATE_DERIVED_MEDIA_COMMAND, {
    update: {
      expected_project_revision: snapshot.project_revision,
      expected_library_revision: snapshot.revision,
      media_id: mediaId,
      mutation,
    },
  });
}

export async function mutateProjectOfflineMedia(
  snapshot: MediaLibrarySnapshot,
  mediaId: string,
  mutation: OfflineMediaMutation,
): Promise<MediaLibrarySnapshot> {
  return invoke<MediaLibrarySnapshot>(MUTATE_OFFLINE_MEDIA_COMMAND, {
    update: {
      expected_project_revision: snapshot.project_revision,
      expected_library_revision: snapshot.revision,
      media_id: mediaId,
      mutation,
    },
  });
}

export function localSearchMedia(
  items: readonly MediaBrowserItem[],
  query: string,
): readonly MediaBrowserItem[] {
  const needle = query.trim().toLocaleLowerCase();
  if (needle.length === 0) return items;
  return items.filter((item) =>
    [
      item.name,
      item.media_id,
      item.offline.status,
      ...item.source_paths,
      ...Object.keys(item.metadata),
      ...Object.values(item.metadata),
      ...item.annotations.labels,
      ...item.annotations.keywords,
    ].some((value) => value.toLocaleLowerCase().includes(needle)),
  );
}
