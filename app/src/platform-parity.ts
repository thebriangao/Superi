export const DESKTOP_PLATFORMS = Object.freeze([
  "macos",
  "windows",
  "linux",
] as const);

export type DesktopPlatform = (typeof DESKTOP_PLATFORMS)[number];

export type PlatformSemanticDomain =
  | "project"
  | "engine"
  | "ui"
  | "shortcut"
  | "media"
  | "color"
  | "audio"
  | "ai"
  | "plugin"
  | "export";

export interface PlatformSemanticContract {
  readonly domain: PlatformSemanticDomain;
  readonly owner: string;
  readonly invariant: string;
}

export interface PlatformSemanticSnapshot {
  readonly schema_version: 1;
  readonly contract_id: "superi.desktop.semantic-parity.v1";
  readonly platform: DesktopPlatform;
  readonly domains: readonly PlatformSemanticContract[];
}

const SEMANTIC_CONTRACTS: PlatformSemanticContract[] = [
  {
    domain: "project",
    owner: "project lifecycle and revision-fenced editor",
    invariant:
      "Stable project identity, revision, commands, recovery, and transaction meaning.",
  },
  {
    domain: "engine",
    owner: "generated client and headless engine lifecycle",
    invariant:
      "Identical typed requests, events, resources, failures, and lifecycle states.",
  },
  {
    domain: "ui",
    owner: "application registry and workspace presentation",
    invariant:
      "Identical routes, panels, actions, state meaning, and reversible presentation intent.",
  },
  {
    domain: "shortcut",
    owner: "portable keyboard shortcut profile",
    invariant:
      "Identical command identities and conflict rules with platform-specific labels only.",
  },
  {
    domain: "media",
    owner: "project media library and engine media contracts",
    invariant:
      "Stable media identity, timing, metadata, alpha, precision, and fallback meaning.",
  },
  {
    domain: "color",
    owner: "managed color and native viewer contracts",
    invariant:
      "Stable scene meaning, precision, transform order, display intent, and appearance.",
  },
  {
    domain: "audio",
    owner: "engine audio and application presentation contracts",
    invariant:
      "Preserve sample timing, channel meaning, synchronization, routing intent, and audible continuity.",
  },
  {
    domain: "ai",
    owner: "local intelligent-feature contracts",
    invariant:
      "Stable controllable results that become ordinary editable project artifacts.",
  },
  {
    domain: "plugin",
    owner: "engine plugin registry and supervision contracts",
    invariant:
      "Stable typed discovery, isolation, capability, failure, and project-state meaning.",
  },
  {
    domain: "export",
    owner: "public export job and delivery contracts",
    invariant:
      "Stable settings, progress, cancellation, failure, color, media, and result meaning.",
  },
];

export const PLATFORM_SEMANTIC_CONTRACTS: readonly PlatformSemanticContract[] =
  Object.freeze(SEMANTIC_CONTRACTS.map((contract) => Object.freeze(contract)));

export function platformSemanticSnapshot(
  platform: string,
): PlatformSemanticSnapshot {
  if (!DESKTOP_PLATFORMS.includes(platform as DesktopPlatform)) {
    throw new Error(`unsupported desktop platform: ${platform}`);
  }
  return Object.freeze({
    schema_version: 1,
    contract_id: "superi.desktop.semantic-parity.v1",
    platform: platform as DesktopPlatform,
    domains: PLATFORM_SEMANTIC_CONTRACTS,
  });
}

export function desktopPlatformLabel(platform: DesktopPlatform): string {
  switch (platform) {
    case "macos":
      return "macOS";
    case "windows":
      return "Windows";
    case "linux":
      return "Linux";
  }
}
