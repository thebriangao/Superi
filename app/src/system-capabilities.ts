import { invoke } from "@tauri-apps/api/core";

export type CapabilityCondition = "available" | "degraded" | "unavailable";
export type CapabilityFreshness = "live" | "retained";
export type CapabilityCacheStatus = "current" | "retained" | "unavailable";

export interface CapabilityFailure {
  readonly code: string;
  readonly title: string;
  readonly action: string;
}

export interface CapabilityDomain<T> {
  readonly condition: CapabilityCondition;
  readonly freshness: CapabilityFreshness;
  readonly data: T | null;
  readonly failure: CapabilityFailure | null;
}

export interface GpuAdapterCapability {
  readonly id: string;
  readonly name: string;
  readonly backend: string;
  readonly device_type: string;
  readonly vendor_id: number;
  readonly device_id: number;
  readonly driver: string;
  readonly driver_info: string;
  readonly feature_bits: string;
  readonly webgpu_compliant: boolean;
  readonly max_texture_dimension_2d: number;
  readonly max_bind_groups: number;
  readonly max_buffer_size: number;
}

export interface GpuCapabilities {
  readonly adapters: readonly GpuAdapterCapability[];
  readonly skipped_adapters: number;
}

export type AudioBufferSize =
  | { readonly kind: "unknown" }
  | { readonly kind: "range"; readonly min: number; readonly max: number };

export interface AudioStreamConfig {
  readonly channels: number;
  readonly sample_rate: number;
  readonly sample_format: string;
  readonly buffer_frames: number | null;
}

export interface AudioCapabilityRange {
  readonly channels: number;
  readonly min_sample_rate: number;
  readonly max_sample_rate: number;
  readonly sample_format: string;
  readonly buffer_size: AudioBufferSize;
}

export interface AudioDeviceCapability {
  readonly id: string;
  readonly name: string;
  readonly is_default: boolean;
  readonly default_config: AudioStreamConfig | null;
  readonly capabilities: readonly AudioCapabilityRange[];
  readonly channel_layout_known: boolean;
}

export interface AudioCapabilities {
  readonly outputs: readonly AudioDeviceCapability[];
  readonly inputs: readonly AudioDeviceCapability[];
  readonly skipped_output_devices: number;
  readonly skipped_input_devices: number;
}

export interface CodecBackendCapability {
  readonly id: string;
  readonly display_name: string;
  readonly priority: number;
  readonly tier: string;
  readonly hardware_acceleration: string;
  readonly operations: readonly string[];
  readonly codec_capability_count: number;
}

export interface CodecOperationSupport {
  readonly operation: string;
  readonly primary_backends: readonly string[];
  readonly fallback_backends: readonly string[];
}

export interface CodecCapabilities {
  readonly schema_version: string;
  readonly revision: number;
  readonly backends: readonly CodecBackendCapability[];
  readonly operations: readonly CodecOperationSupport[];
}

export interface AiCapabilities {
  readonly schema_version: number;
  readonly runtime: "unavailable";
  readonly local_only: boolean;
  readonly requires_editable_artifacts: boolean;
  readonly available_pipelines: readonly string[];
}

export interface DesktopCapabilitySnapshot {
  readonly schema_version: number;
  readonly revision: number;
  readonly observed_at_unix_ms: number;
  readonly cache_status: CapabilityCacheStatus;
  readonly persistence_failure: CapabilityFailure | null;
  readonly gpu: CapabilityDomain<GpuCapabilities>;
  readonly audio: CapabilityDomain<AudioCapabilities>;
  readonly codecs: CapabilityDomain<CodecCapabilities>;
  readonly ai: CapabilityDomain<AiCapabilities>;
}

export interface DesktopCapabilityHost {
  readonly invoke: (
    command: string,
    args?: Record<string, unknown>,
  ) => Promise<unknown>;
}

const COMMAND = "desktop_capabilities_discover";
const DEFAULT_HOST: DesktopCapabilityHost = { invoke };

export async function discoverDesktopCapabilities(
  host: DesktopCapabilityHost = DEFAULT_HOST,
): Promise<DesktopCapabilitySnapshot> {
  return deepFreeze(parseSnapshot(await host.invoke(COMMAND)));
}

export function capabilityFailureText(
  failure: CapabilityFailure | null,
): string | null {
  return failure === null ? null : `${failure.title} ${failure.action}`;
}

function parseSnapshot(value: unknown): DesktopCapabilitySnapshot {
  const root = exactRecord(
    value,
    [
      "schema_version",
      "revision",
      "observed_at_unix_ms",
      "cache_status",
      "persistence_failure",
      "gpu",
      "audio",
      "codecs",
      "ai",
    ],
    "capabilities",
  );
  const schemaVersion = integer(root.schema_version, "schema_version", 1);
  if (schemaVersion !== 1) {
    throw new Error("capabilities.schema_version is unsupported");
  }
  const snapshot: DesktopCapabilitySnapshot = {
    schema_version: schemaVersion,
    revision: integer(root.revision, "revision", 1),
    observed_at_unix_ms: integer(
      root.observed_at_unix_ms,
      "observed_at_unix_ms",
      0,
    ),
    cache_status: oneOf(
      root.cache_status,
      ["current", "retained", "unavailable"],
      "cache_status",
    ),
    persistence_failure: nullable(root.persistence_failure, parseFailure),
    gpu: parseDomain(root.gpu, "gpu", parseGpu),
    audio: parseDomain(root.audio, "audio", parseAudio),
    codecs: parseDomain(root.codecs, "codecs", parseCodecs),
    ai: parseDomain(root.ai, "ai", parseAi),
  };
  const retained = [
    snapshot.gpu,
    snapshot.audio,
    snapshot.codecs,
    snapshot.ai,
  ].some((domain) => domain.freshness === "retained");
  if (
    (snapshot.cache_status === "current" &&
      (retained || snapshot.persistence_failure !== null)) ||
    (snapshot.cache_status === "retained" &&
      (!retained || snapshot.persistence_failure !== null)) ||
    (snapshot.cache_status === "unavailable" &&
      snapshot.persistence_failure === null)
  ) {
    throw new Error("capabilities cache state is inconsistent");
  }
  return snapshot;
}

function parseDomain<T>(
  value: unknown,
  path: string,
  parseData: (value: unknown, path: string) => T,
): CapabilityDomain<T> {
  const domain = exactRecord(
    value,
    ["condition", "freshness", "data", "failure"],
    path,
  );
  const condition = oneOf(
    domain.condition,
    ["available", "degraded", "unavailable"],
    `${path}.condition`,
  );
  const freshness = oneOf(
    domain.freshness,
    ["live", "retained"],
    `${path}.freshness`,
  );
  const data = nullable(domain.data, (candidate) =>
    parseData(candidate, `${path}.data`),
  );
  const failure = nullable(domain.failure, parseFailure);
  const valid =
    ((condition === "available" && data !== null && failure === null) ||
      (condition === "degraded" && data !== null && failure !== null) ||
      (condition === "unavailable" && failure !== null)) &&
    (freshness !== "retained" || data !== null);
  if (!valid) {
    throw new Error(`${path} has inconsistent capability state`);
  }
  return {
    condition,
    freshness,
    data,
    failure,
  };
}

function parseFailure(value: unknown): CapabilityFailure {
  const failure = exactRecord(value, ["code", "title", "action"], "failure");
  return {
    code: text(failure.code, "failure.code", 128),
    title: text(failure.title, "failure.title", 256),
    action: text(failure.action, "failure.action", 512),
  };
}

function parseGpu(value: unknown, path: string): GpuCapabilities {
  const gpu = exactRecord(value, ["adapters", "skipped_adapters"], path);
  return {
    adapters: array(gpu.adapters, `${path}.adapters`, 32).map(
      (adapter, index) => parseGpuAdapter(adapter, `${path}.adapters[${index}]`),
    ),
    skipped_adapters: integer(
      gpu.skipped_adapters,
      `${path}.skipped_adapters`,
      0,
    ),
  };
}

function parseGpuAdapter(value: unknown, path: string): GpuAdapterCapability {
  const adapter = exactRecord(
    value,
    [
      "id",
      "name",
      "backend",
      "device_type",
      "vendor_id",
      "device_id",
      "driver",
      "driver_info",
      "feature_bits",
      "webgpu_compliant",
      "max_texture_dimension_2d",
      "max_bind_groups",
      "max_buffer_size",
    ],
    path,
  );
  return {
    id: text(adapter.id, `${path}.id`, 512),
    name: text(adapter.name, `${path}.name`, 512),
    backend: text(adapter.backend, `${path}.backend`, 64),
    device_type: text(adapter.device_type, `${path}.device_type`, 64),
    vendor_id: integer(adapter.vendor_id, `${path}.vendor_id`, 0),
    device_id: integer(adapter.device_id, `${path}.device_id`, 0),
    driver: text(adapter.driver, `${path}.driver`, 512, true),
    driver_info: text(adapter.driver_info, `${path}.driver_info`, 1_024, true),
    feature_bits: hexadecimal(adapter.feature_bits, `${path}.feature_bits`),
    webgpu_compliant: boolean(
      adapter.webgpu_compliant,
      `${path}.webgpu_compliant`,
    ),
    max_texture_dimension_2d: integer(
      adapter.max_texture_dimension_2d,
      `${path}.max_texture_dimension_2d`,
      0,
    ),
    max_bind_groups: integer(
      adapter.max_bind_groups,
      `${path}.max_bind_groups`,
      0,
    ),
    max_buffer_size: integer(
      adapter.max_buffer_size,
      `${path}.max_buffer_size`,
      0,
    ),
  };
}

function parseAudio(value: unknown, path: string): AudioCapabilities {
  const audio = exactRecord(
    value,
    [
      "outputs",
      "inputs",
      "skipped_output_devices",
      "skipped_input_devices",
    ],
    path,
  );
  return {
    outputs: array(audio.outputs, `${path}.outputs`, 128).map((device, index) =>
      parseAudioDevice(device, `${path}.outputs[${index}]`),
    ),
    inputs: array(audio.inputs, `${path}.inputs`, 128).map((device, index) =>
      parseAudioDevice(device, `${path}.inputs[${index}]`),
    ),
    skipped_output_devices: integer(
      audio.skipped_output_devices,
      `${path}.skipped_output_devices`,
      0,
    ),
    skipped_input_devices: integer(
      audio.skipped_input_devices,
      `${path}.skipped_input_devices`,
      0,
    ),
  };
}

function parseAudioDevice(value: unknown, path: string): AudioDeviceCapability {
  const device = exactRecord(
    value,
    [
      "id",
      "name",
      "is_default",
      "default_config",
      "capabilities",
      "channel_layout_known",
    ],
    path,
  );
  return {
    id: text(device.id, `${path}.id`, 32_768),
    name: text(device.name, `${path}.name`, 512),
    is_default: boolean(device.is_default, `${path}.is_default`),
    default_config: nullable(device.default_config, (config) =>
      parseAudioConfig(config, `${path}.default_config`),
    ),
    capabilities: array(
      device.capabilities,
      `${path}.capabilities`,
      1_024,
    ).map((capability, index) =>
      parseAudioRange(capability, `${path}.capabilities[${index}]`),
    ),
    channel_layout_known: boolean(
      device.channel_layout_known,
      `${path}.channel_layout_known`,
    ),
  };
}

function parseAudioConfig(value: unknown, path: string): AudioStreamConfig {
  const config = exactRecord(
    value,
    ["channels", "sample_rate", "sample_format", "buffer_frames"],
    path,
  );
  return {
    channels: integer(config.channels, `${path}.channels`, 1),
    sample_rate: integer(config.sample_rate, `${path}.sample_rate`, 1),
    sample_format: text(config.sample_format, `${path}.sample_format`, 32),
    buffer_frames: nullable(config.buffer_frames, (frames) =>
      integer(frames, `${path}.buffer_frames`, 1),
    ),
  };
}

function parseAudioRange(value: unknown, path: string): AudioCapabilityRange {
  const capability = exactRecord(
    value,
    [
      "channels",
      "min_sample_rate",
      "max_sample_rate",
      "sample_format",
      "buffer_size",
    ],
    path,
  );
  const minSampleRate = integer(
    capability.min_sample_rate,
    `${path}.min_sample_rate`,
    1,
  );
  const maxSampleRate = integer(
    capability.max_sample_rate,
    `${path}.max_sample_rate`,
    minSampleRate,
  );
  return {
    channels: integer(capability.channels, `${path}.channels`, 1),
    min_sample_rate: minSampleRate,
    max_sample_rate: maxSampleRate,
    sample_format: text(
      capability.sample_format,
      `${path}.sample_format`,
      32,
    ),
    buffer_size: parseBufferSize(capability.buffer_size, `${path}.buffer_size`),
  };
}

function parseBufferSize(value: unknown, path: string): AudioBufferSize {
  const candidate = record(value, path);
  const kind = oneOf(candidate.kind, ["unknown", "range"], `${path}.kind`);
  if (kind === "unknown") {
    exactKeys(candidate, ["kind"], path);
    return { kind };
  }
  exactKeys(candidate, ["kind", "min", "max"], path);
  const min = integer(candidate.min, `${path}.min`, 1);
  return {
    kind,
    min,
    max: integer(candidate.max, `${path}.max`, min),
  };
}

function parseCodecs(value: unknown, path: string): CodecCapabilities {
  const codecs = exactRecord(
    value,
    ["schema_version", "revision", "backends", "operations"],
    path,
  );
  return {
    schema_version: semanticVersion(
      codecs.schema_version,
      `${path}.schema_version`,
    ),
    revision: integer(codecs.revision, `${path}.revision`, 0),
    backends: array(codecs.backends, `${path}.backends`, 1_024).map(
      (backend, index) => parseCodecBackend(backend, `${path}.backends[${index}]`),
    ),
    operations: array(codecs.operations, `${path}.operations`, 4_096).map(
      (operation, index) =>
        parseCodecOperation(operation, `${path}.operations[${index}]`),
    ),
  };
}

function parseCodecBackend(value: unknown, path: string): CodecBackendCapability {
  const backend = exactRecord(
    value,
    [
      "id",
      "display_name",
      "priority",
      "tier",
      "hardware_acceleration",
      "operations",
      "codec_capability_count",
    ],
    path,
  );
  return {
    id: text(backend.id, `${path}.id`, 512),
    display_name: text(backend.display_name, `${path}.display_name`, 512),
    priority: integer(backend.priority, `${path}.priority`, 0),
    tier: text(backend.tier, `${path}.tier`, 32),
    hardware_acceleration: text(
      backend.hardware_acceleration,
      `${path}.hardware_acceleration`,
      64,
    ),
    operations: stringArray(backend.operations, `${path}.operations`, 4_096),
    codec_capability_count: integer(
      backend.codec_capability_count,
      `${path}.codec_capability_count`,
      0,
    ),
  };
}

function parseCodecOperation(value: unknown, path: string): CodecOperationSupport {
  const operation = exactRecord(
    value,
    ["operation", "primary_backends", "fallback_backends"],
    path,
  );
  return {
    operation: text(operation.operation, `${path}.operation`, 512),
    primary_backends: stringArray(
      operation.primary_backends,
      `${path}.primary_backends`,
      1_024,
    ),
    fallback_backends: stringArray(
      operation.fallback_backends,
      `${path}.fallback_backends`,
      1_024,
    ),
  };
}

function parseAi(value: unknown, path: string): AiCapabilities {
  const ai = exactRecord(
    value,
    [
      "schema_version",
      "runtime",
      "local_only",
      "requires_editable_artifacts",
      "available_pipelines",
    ],
    path,
  );
  const schemaVersion = integer(ai.schema_version, `${path}.schema_version`, 1);
  const localOnly = boolean(ai.local_only, `${path}.local_only`);
  const requiresEditableArtifacts = boolean(
    ai.requires_editable_artifacts,
    `${path}.requires_editable_artifacts`,
  );
  const availablePipelines = stringArray(
    ai.available_pipelines,
    `${path}.available_pipelines`,
    64,
  );
  if (
    schemaVersion !== 1 ||
    !localOnly ||
    !requiresEditableArtifacts ||
    availablePipelines.length !== 0
  ) {
    throw new Error(`${path} has inconsistent unavailable AI state`);
  }
  return {
    schema_version: schemaVersion,
    runtime: oneOf(ai.runtime, ["unavailable"], `${path}.runtime`),
    local_only: localOnly,
    requires_editable_artifacts: requiresEditableArtifacts,
    available_pipelines: availablePipelines,
  };
}

function exactRecord(
  value: unknown,
  keys: readonly string[],
  path: string,
): Record<string, unknown> {
  const candidate = record(value, path);
  exactKeys(candidate, keys, path);
  return candidate;
}

function record(value: unknown, path: string): Record<string, unknown> {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    throw new Error(`${path} must be an object`);
  }
  return value as Record<string, unknown>;
}

function exactKeys(
  value: Record<string, unknown>,
  keys: readonly string[],
  path: string,
): void {
  const expected = new Set(keys);
  for (const key of Object.keys(value)) {
    if (!expected.has(key)) {
      throw new Error(`${path} has unexpected field ${key}`);
    }
  }
  for (const key of keys) {
    if (!(key in value)) {
      throw new Error(`${path} is missing field ${key}`);
    }
  }
}

function array(
  value: unknown,
  path: string,
  maximum: number,
): readonly unknown[] {
  if (!Array.isArray(value) || value.length > maximum) {
    throw new Error(`${path} must be an array of at most ${maximum} items`);
  }
  return value;
}

function stringArray(
  value: unknown,
  path: string,
  maximum: number,
): readonly string[] {
  return array(value, path, maximum).map((item, index) =>
    text(item, `${path}[${index}]`, 512),
  );
}

function text(
  value: unknown,
  path: string,
  maximum: number,
  allowEmpty = false,
): string {
  if (
    typeof value !== "string" ||
    (!allowEmpty && value.length === 0) ||
    value.length > maximum ||
    /[\u0000-\u001f\u007f]/u.test(value)
  ) {
    throw new Error(`${path} must be bounded display text`);
  }
  return value;
}

function integer(
  value: unknown,
  path: string,
  minimum: number,
): number {
  if (!Number.isSafeInteger(value) || (value as number) < minimum) {
    throw new Error(`${path} must be a safe integer at least ${minimum}`);
  }
  return value as number;
}

function boolean(value: unknown, path: string): boolean {
  if (typeof value !== "boolean") {
    throw new Error(`${path} must be a Boolean`);
  }
  return value;
}

function nullable<T>(
  value: unknown,
  parse: (candidate: unknown) => T,
): T | null {
  return value === null ? null : parse(value);
}

function oneOf<const T extends string>(
  value: unknown,
  choices: readonly T[],
  path: string,
): T {
  if (typeof value !== "string" || !choices.includes(value as T)) {
    throw new Error(`${path} has an unsupported value`);
  }
  return value as T;
}

function hexadecimal(value: unknown, path: string): string {
  const result = text(value, path, 128);
  if (!/^0x[0-9a-f]+$/u.test(result)) {
    throw new Error(`${path} must be lowercase hexadecimal text`);
  }
  return result;
}

function semanticVersion(value: unknown, path: string): string {
  const result = text(value, path, 64);
  if (!/^\d+\.\d+\.\d+$/u.test(result)) {
    throw new Error(`${path} must be semantic version text`);
  }
  return result;
}

function deepFreeze<T>(value: T): T {
  if (typeof value !== "object" || value === null || Object.isFrozen(value)) {
    return value;
  }
  for (const nested of Object.values(value)) {
    deepFreeze(nested);
  }
  return Object.freeze(value);
}
