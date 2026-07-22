import { invoke } from "@tauri-apps/api/core";

export type PlatformAdapterDomain =
  | "gpu"
  | "audio"
  | "filesystem"
  | "font"
  | "monitor"
  | "codec";

export interface PlatformAdapterDeclaration {
  readonly domain: PlatformAdapterDomain;
  readonly contract_id: string;
  readonly implementation: string;
}

export interface PlatformAdapterSnapshot {
  readonly schema_version: 1;
  readonly platform: "macos" | "windows" | "linux";
  readonly media_guarantees: readonly [
    "timing",
    "precision",
    "metadata",
    "alpha",
    "predictable_fallback",
  ];
  readonly adapters: readonly PlatformAdapterDeclaration[];
}

export interface PlatformAdapterHost {
  readonly invoke: (command: string) => Promise<unknown>;
}

const COMMAND = "desktop_platform_adapters";
const DEFAULT_HOST: PlatformAdapterHost = { invoke };
const EXPECTED_DOMAINS = [
  "gpu",
  "audio",
  "filesystem",
  "font",
  "monitor",
  "codec",
] as const;
const MEDIA_GUARANTEES = [
  "timing",
  "precision",
  "metadata",
  "alpha",
  "predictable_fallback",
] as const;
const CONTRACT_IDS: Record<PlatformAdapterDomain, string> = {
  gpu: "superi.adapter.gpu.v1",
  audio: "superi.adapter.audio.v1",
  filesystem: "superi.adapter.filesystem.v1",
  font: "superi.adapter.font.v1",
  monitor: "superi.adapter.monitor.v1",
  codec: "superi.adapter.codec.v1",
};

export async function discoverPlatformAdapters(
  host: PlatformAdapterHost = DEFAULT_HOST,
): Promise<PlatformAdapterSnapshot> {
  return parseSnapshot(await host.invoke(COMMAND));
}

function parseSnapshot(value: unknown): PlatformAdapterSnapshot {
  const root = exactRecord(
    value,
    ["schema_version", "platform", "media_guarantees", "adapters"],
    "platform adapters",
  );
  if (root.schema_version !== 1) {
    throw new Error("platform adapters schema_version is unsupported");
  }
  const platform = oneOf(root.platform, ["macos", "windows", "linux"], "platform");
  const mediaGuarantees = root.media_guarantees;
  if (
    !Array.isArray(mediaGuarantees) ||
    mediaGuarantees.length !== MEDIA_GUARANTEES.length ||
    MEDIA_GUARANTEES.some(
      (guarantee, index) => mediaGuarantees[index] !== guarantee,
    )
  ) {
    throw new Error("platform adapter media_guarantees are inconsistent");
  }
  if (!Array.isArray(root.adapters) || root.adapters.length !== EXPECTED_DOMAINS.length) {
    throw new Error("platform adapter domains are incomplete");
  }
  const adapters = root.adapters.map((candidate, index) => {
    const adapter = exactRecord(
      candidate,
      ["domain", "contract_id", "implementation"],
      `adapters[${index}]`,
    );
    const domain = oneOf(adapter.domain, EXPECTED_DOMAINS, `adapters[${index}].domain`);
    if (domain !== EXPECTED_DOMAINS[index]) {
      throw new Error("platform adapter domains are duplicated or out of order");
    }
    if (adapter.contract_id !== CONTRACT_IDS[domain]) {
      throw new Error(`adapters[${index}].contract_id is inconsistent`);
    }
    if (
      typeof adapter.implementation !== "string" ||
      adapter.implementation.trim().length === 0 ||
      adapter.implementation.length > 128
    ) {
      throw new Error(`adapters[${index}].implementation is invalid`);
    }
    return Object.freeze({
      domain,
      contract_id: adapter.contract_id,
      implementation: adapter.implementation,
    });
  });
  return Object.freeze({
    schema_version: 1,
    platform,
    media_guarantees: MEDIA_GUARANTEES,
    adapters: Object.freeze(adapters),
  });
}

function exactRecord(
  value: unknown,
  fields: readonly string[],
  path: string,
): Record<string, unknown> {
  if (value === null || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${path} must be an object`);
  }
  const record = value as Record<string, unknown>;
  const unexpected = Object.keys(record).find((key) => !fields.includes(key));
  if (unexpected !== undefined) {
    throw new Error(`${path} has unexpected field ${unexpected}`);
  }
  const missing = fields.find((field) => !(field in record));
  if (missing !== undefined) throw new Error(`${path}.${missing} is required`);
  return record;
}

function oneOf<const T extends readonly string[]>(
  value: unknown,
  choices: T,
  path: string,
): T[number] {
  if (typeof value !== "string" || !choices.includes(value)) {
    throw new Error(`${path} is invalid`);
  }
  return value as T[number];
}
