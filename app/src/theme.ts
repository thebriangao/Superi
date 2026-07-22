export const APPLICATION_THEME = Object.freeze({
  id: "color-critical-dark",
  schemaVersion: 1,
  colorScheme: "dark",
  browserThemeColor: "#111315",
  sceneAppearanceOwner: "native-color-pipeline",
  workspaceStatePolicy: "untouched",
} as const);

export interface ApplicationThemeActivation {
  readonly theme: typeof APPLICATION_THEME;
  readonly recovered: boolean;
  readonly repairs: readonly string[];
}

export function applyApplicationTheme(
  target: Document,
): ApplicationThemeActivation {
  const root = target.documentElement;
  const repairs: string[] = [];

  reconcileAttribute(
    root,
    "data-superi-theme",
    APPLICATION_THEME.id,
    "theme identity",
    repairs,
  );
  reconcileAttribute(
    root,
    "data-superi-theme-schema",
    APPLICATION_THEME.schemaVersion.toString(),
    "theme schema",
    repairs,
  );
  reconcileAttribute(
    root,
    "data-superi-scene-owner",
    APPLICATION_THEME.sceneAppearanceOwner,
    "scene appearance owner",
    repairs,
  );

  let themeMeta = target.querySelector<HTMLMetaElement>(
    'meta[name="theme-color"]',
  );
  if (themeMeta === null) {
    themeMeta = target.createElement("meta");
    themeMeta.name = "theme-color";
    target.head.append(themeMeta);
    repairs.push("browser theme metadata");
  } else if (themeMeta.content !== APPLICATION_THEME.browserThemeColor) {
    repairs.push("browser theme color");
  }
  themeMeta.content = APPLICATION_THEME.browserThemeColor;

  const recovered = repairs.length > 0;
  root.setAttribute(
    "data-superi-theme-status",
    recovered ? "recovered" : "ready",
  );

  return Object.freeze({
    theme: APPLICATION_THEME,
    recovered,
    repairs: Object.freeze(repairs),
  });
}

function reconcileAttribute(
  root: HTMLElement,
  name: string,
  expected: string,
  repair: string,
  repairs: string[],
): void {
  if (root.getAttribute(name) !== expected) repairs.push(repair);
  root.setAttribute(name, expected);
}
