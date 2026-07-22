export type FilesystemCaseSensitivity = "sensitive" | "insensitive";

export interface FilesystemVolumeEvidence {
  readonly kind: "system" | "removable" | "unknown";
  readonly status: "mounted" | "offline";
}

export type FilesystemSourceStatus =
  | "unchecked"
  | "unchanged"
  | "changed"
  | "missing"
  | "volume_offline"
  | "unavailable";

export type FilesystemVolumeBehavior =
  | "ready"
  | "wait_for_volume"
  | "locate_source"
  | "review_changed_source"
  | "retry_inspection";

export function filesystemPathBasename(path: string): string {
  const withoutTrailingSeparators = path.replace(/[\\/]+$/u, "");
  if (withoutTrailingSeparators.length === 0) return path;
  return withoutTrailingSeparators.split(/[\\/]/u).at(-1) ?? path;
}

export function filesystemPathUnderRoot(root: string, path: string): string {
  const basename = filesystemPathBasename(path);
  if (root.length === 0) return basename;
  const separator = windowsPathSyntax(root) ? "\\" : "/";
  const trimmedRoot = root.replace(/[\\/]+$/u, "");
  if (trimmedRoot.length > 0) return `${trimmedRoot}${separator}${basename}`;
  return `${separator}${basename}`;
}

export function filesystemPathKey(
  path: string,
  caseSensitivity: FilesystemCaseSensitivity,
): string {
  const normalized = normalizePathSyntax(path).normalize("NFC");
  return caseSensitivity === "insensitive"
    ? normalized.toLocaleLowerCase("en-US")
    : normalized;
}

export function removableVolumeBehavior(
  volume: FilesystemVolumeEvidence,
  pathStatus: FilesystemSourceStatus,
): FilesystemVolumeBehavior {
  if (
    pathStatus === "volume_offline" ||
    (volume.kind === "removable" && volume.status === "offline")
  ) {
    return "wait_for_volume";
  }
  if (pathStatus === "missing") return "locate_source";
  if (pathStatus === "changed") return "review_changed_source";
  if (pathStatus === "unavailable") return "retry_inspection";
  return "ready";
}

function windowsPathSyntax(path: string): boolean {
  return /^[A-Za-z]:[\\/]/u.test(path) || (path.includes("\\") && !path.includes("/"));
}

function normalizePathSyntax(path: string): string {
  const slashes = path.replaceAll("\\", "/");
  const prefix = slashes.startsWith("//") ? "//" : slashes.startsWith("/") ? "/" : "";
  const body = slashes
    .slice(prefix.length)
    .replace(/\/{2,}/gu, "/")
    .replace(/\/+$/u, "");
  return `${prefix}${body}`;
}
