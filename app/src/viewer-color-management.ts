export type ViewerColorRole = "source" | "program" | "composite" | "color";
export type ViewerDisplayTransform = "srgb" | "display_p3";
export type ViewerProfileState = "profiled" | "unprofiled";
export type ViewerProfileModel = "matrix_trc" | "lut";
export type ViewerProfileRenderingIntent =
  | "perceptual"
  | "media_relative_colorimetric"
  | "saturation"
  | "absolute_colorimetric";

export interface ViewerDisplayTransformDefinition {
  readonly code: ViewerDisplayTransform;
  readonly label: string;
  readonly displayIntent: string;
  readonly transformId: string;
}

export const VIEWER_DISPLAY_TRANSFORMS: readonly ViewerDisplayTransformDefinition[] =
  Object.freeze([
    Object.freeze({
      code: "srgb" as const,
      label: "sRGB",
      displayIntent: "scene-linear ACEScg to sRGB display",
      transformId: "superi.viewport.acescg-to-srgb.v1",
    }),
    Object.freeze({
      code: "display_p3" as const,
      label: "Display P3",
      displayIntent: "scene-linear ACEScg to Display P3 display",
      transformId: "superi.viewport.acescg-to-display-p3.v1",
    }),
  ]);

export const VIEWER_DISPLAY_TRANSFORM_ORDER = Object.freeze([
  "alpha_unassociate",
  "scene_to_display_primaries",
  "gamut_mapping",
  "tone_mapping",
  "transfer_encoding",
  "alpha_reassociate",
] as const);

export interface ViewerMonitorProfileSnapshot {
  readonly id: string;
  readonly name: string;
  readonly primary: boolean;
  readonly builtIn: boolean;
  readonly profileState: ViewerProfileState;
  readonly profileId: string | null;
  readonly profileModel: ViewerProfileModel | null;
  readonly renderingIntent: ViewerProfileRenderingIntent | null;
}

export interface ViewerColorSnapshot {
  readonly profileGeneration: number;
  readonly monitorProfiles: readonly ViewerMonitorProfileSnapshot[];
  readonly selectedMonitorId: string | null;
  readonly displayTransform: ViewerDisplayTransform;
  readonly displayIntent: string;
  readonly displayTransformId: string;
  readonly transformOrder: readonly string[];
  readonly profileNote: string;
}

export interface ViewerColorState {
  readonly profileGeneration: number;
  readonly monitorProfiles: readonly ViewerMonitorProfileSnapshot[];
  readonly selectedMonitorId: string | null;
  readonly selectedMonitor: ViewerMonitorProfileSnapshot | null;
  readonly displayTransform: ViewerDisplayTransform;
  readonly displayIntent: string;
  readonly displayTransformId: string;
  readonly transformOrder: readonly string[];
  readonly profileNote: string;
}

export interface ViewerColorSelection {
  readonly role: ViewerColorRole;
  readonly monitorId: string;
  readonly displayTransform: ViewerDisplayTransform;
}

const MAX_ACTIVE_MONITORS = 64;
const PROFILE_ID = /^[0-9a-f]{64}$/;

export function projectViewerColorState(
  snapshot: ViewerColorSnapshot,
): ViewerColorState {
  if (
    !Number.isSafeInteger(snapshot.profileGeneration) ||
    snapshot.profileGeneration < 0
  ) {
    throw new Error("profile catalog generation must be a nonnegative safe integer");
  }
  if (snapshot.monitorProfiles.length > MAX_ACTIVE_MONITORS) {
    throw new Error("monitor profile catalog exceeds the fixed display limit");
  }

  const ids = new Set<string>();
  let primaryCount = 0;
  const monitorProfiles = snapshot.monitorProfiles.map((profile) => {
    validateText(profile.id, "monitor identity");
    validateText(profile.name, "monitor name");
    if (typeof profile.primary !== "boolean" || typeof profile.builtIn !== "boolean") {
      throw new Error("monitor primary and built-in state must be boolean");
    }
    if (profile.primary) {
      primaryCount += 1;
    }
    if (ids.has(profile.id)) {
      throw new Error(`duplicate monitor identity ${profile.id}`);
    }
    ids.add(profile.id);
    validateProfile(profile);
    return Object.freeze({ ...profile });
  });
  if (primaryCount > 1) {
    throw new Error("monitor profile catalog cannot contain multiple primary displays");
  }

  const selectedMonitor =
    snapshot.selectedMonitorId === null
      ? null
      : monitorProfiles.find(
          (profile) => profile.id === snapshot.selectedMonitorId,
        ) ?? null;
  if (snapshot.selectedMonitorId !== null && selectedMonitor === null) {
    throw new Error("selected monitor is not present in the active profile catalog");
  }

  const transform = displayTransform(snapshot.displayTransform);
  if (snapshot.displayIntent !== transform.displayIntent) {
    throw new Error("display intent does not match the selected display transform");
  }
  if (snapshot.displayTransformId !== transform.transformId) {
    throw new Error("display transform identity does not match the selected transform");
  }
  if (
    snapshot.transformOrder.length !== VIEWER_DISPLAY_TRANSFORM_ORDER.length ||
    !snapshot.transformOrder.every(
      (stage, index) => stage === VIEWER_DISPLAY_TRANSFORM_ORDER[index],
    )
  ) {
    throw new Error("display transform order does not match the canonical viewer pipeline");
  }
  validateText(snapshot.profileNote, "profile note", 512);

  return Object.freeze({
    profileGeneration: snapshot.profileGeneration,
    monitorProfiles: Object.freeze(monitorProfiles),
    selectedMonitorId: snapshot.selectedMonitorId,
    selectedMonitor,
    displayTransform: snapshot.displayTransform,
    displayIntent: snapshot.displayIntent,
    displayTransformId: snapshot.displayTransformId,
    transformOrder: Object.freeze([...snapshot.transformOrder]),
    profileNote: snapshot.profileNote,
  });
}

export function createViewerColorSelection(
  role: ViewerColorRole,
  state: ViewerColorState,
  selection: Readonly<{
    monitorId: string;
    displayTransform: ViewerDisplayTransform;
  }>,
): ViewerColorSelection {
  if (!state.monitorProfiles.some(({ id }) => id === selection.monitorId)) {
    throw new Error("color selection requires an active monitor profile");
  }
  displayTransform(selection.displayTransform);
  return Object.freeze({
    role,
    monitorId: selection.monitorId,
    displayTransform: selection.displayTransform,
  });
}

export function formatViewerColorState(state: ViewerColorState): string {
  const transform = displayTransform(state.displayTransform);
  if (state.selectedMonitor === null) {
    return (
      `Monitor profile unavailable; ${transform.label} via ${state.displayTransformId}; ` +
      `profile catalog generation ${state.profileGeneration}; ${state.profileNote}`
    );
  }
  const monitor = state.selectedMonitor;
  const profile =
    monitor.profileState === "profiled"
      ? `profile ${monitor.profileId!.slice(0, 12)}, ${words(monitor.profileModel!)}, ${words(monitor.renderingIntent!)}`
      : "unprofiled monitor";
  return (
    `${monitor.name} (${monitor.id}), ${profile}; ${transform.label} via ` +
    `${state.displayTransformId}; profile catalog generation ${state.profileGeneration}; ` +
    state.profileNote
  );
}

function displayTransform(
  code: ViewerDisplayTransform,
): ViewerDisplayTransformDefinition {
  const definition = VIEWER_DISPLAY_TRANSFORMS.find(
    (candidate) => candidate.code === code,
  );
  if (definition === undefined) {
    throw new Error(`unsupported viewer display transform ${String(code)}`);
  }
  return definition;
}

function validateProfile(profile: ViewerMonitorProfileSnapshot): void {
  if (profile.profileState !== "profiled" && profile.profileState !== "unprofiled") {
    throw new Error("monitor profile state is unsupported");
  }
  if (profile.profileState === "profiled") {
    if (
      profile.profileId === null ||
      !PROFILE_ID.test(profile.profileId) ||
      profile.profileModel === null ||
      !["matrix_trc", "lut"].includes(profile.profileModel) ||
      profile.renderingIntent === null ||
      ![
        "perceptual",
        "media_relative_colorimetric",
        "saturation",
        "absolute_colorimetric",
      ].includes(profile.renderingIntent)
    ) {
      throw new Error("profiled monitor requires complete ICC identity evidence");
    }
    return;
  }
  if (
    profile.profileId !== null ||
    profile.profileModel !== null ||
    profile.renderingIntent !== null
  ) {
    throw new Error("unprofiled monitor cannot publish ICC profile evidence");
  }
}

function validateText(value: string, label: string, limit = 256): void {
  if (
    value.length === 0 ||
    value.length > limit ||
    [...value].some((character) => /[\u0000-\u001f\u007f]/.test(character))
  ) {
    throw new Error(`${label} must be nonempty, bounded, and free of control characters`);
  }
}

function words(value: string): string {
  return value.replaceAll("_", " ");
}
