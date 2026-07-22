import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  APPLICATION_THEME,
  applyApplicationTheme,
} from "../src/theme.ts";

const appRoot = new URL("../", import.meta.url);

function read(path: string): string {
  return readFileSync(new URL(path, appRoot), "utf8");
}

test("color-critical dark theme publishes one frozen application contract", () => {
  assert.deepEqual(APPLICATION_THEME, {
    id: "color-critical-dark",
    schemaVersion: 1,
    colorScheme: "dark",
    browserThemeColor: "#111315",
    sceneAppearanceOwner: "native-color-pipeline",
    workspaceStatePolicy: "untouched",
  });
  assert.ok(Object.isFrozen(APPLICATION_THEME));
});

test("theme activation repairs document declarations without application state", () => {
  const attributes = new Map<string, string>([
    ["data-superi-theme", "unsupported"],
    ["data-superi-theme-schema", "0"],
    ["data-superi-scene-owner", "webview"],
  ]);
  let themeMeta: { name: string; content: string } | null = null;
  const target = {
    documentElement: {
      getAttribute: (name: string) => attributes.get(name) ?? null,
      setAttribute: (name: string, value: string) => attributes.set(name, value),
    },
    head: {
      append: (element: { name: string; content: string }) => {
        themeMeta = element;
      },
    },
    createElement: () => ({ name: "", content: "" }),
    querySelector: () => themeMeta,
  } as unknown as Document;

  const activation = applyApplicationTheme(target);

  assert.equal(attributes.get("data-superi-theme"), APPLICATION_THEME.id);
  assert.equal(attributes.get("data-superi-theme-schema"), "1");
  assert.equal(
    attributes.get("data-superi-scene-owner"),
    APPLICATION_THEME.sceneAppearanceOwner,
  );
  assert.equal(attributes.get("data-superi-theme-status"), "recovered");
  assert.deepEqual(themeMeta, {
    name: "theme-color",
    content: APPLICATION_THEME.browserThemeColor,
  });
  assert.equal(activation.recovered, true);
  assert.deepEqual(activation.repairs, [
    "theme identity",
    "theme schema",
    "scene appearance owner",
    "browser theme metadata",
  ]);
  assert.ok(Object.isFrozen(activation));
  assert.ok(Object.isFrozen(activation.repairs));
});

test("production bootstrap declares and activates the theme before the application", () => {
  const index = read("index.html");
  const main = read("src/main.tsx");

  assert.match(index, /data-superi-theme="color-critical-dark"/);
  assert.match(index, /data-superi-theme-schema="1"/);
  assert.match(index, /data-superi-scene-owner="native-color-pipeline"/);
  assert.match(index, /<meta name="theme-color" content="#111315"/);
  assert.ok(main.indexOf('import "./theme.css"') < main.indexOf('import "./styles.css"'));
  assert.ok(
    main.indexOf("applyApplicationTheme(document)") <
      main.indexOf("new DesktopSuperiTransport()"),
  );
});

test("theme tokens separate application chrome from immutable color data", () => {
  const theme = read("src/theme.css");
  const styles = read("src/styles.css");
  const palette = read("src/command-palette.css");
  const shortcuts = read("src/keyboard-shortcuts.css");
  const feedbackStart = styles.indexOf(".application-tooltip-host {");
  const feedbackStyles = styles.slice(feedbackStart);

  for (const token of [
    "--theme-canvas",
    "--theme-surface-panel",
    "--theme-text-primary",
    "--theme-focus-ring",
    "--theme-status-warning-text",
    "--theme-status-correctable-border",
    "--theme-status-error-text",
    "--viewer-surround",
    "--viewer-cinema-surround",
    "--viewer-overlay-guide",
    "--viewer-overlay-safe-area",
    "--marker-red",
    "--marker-white",
  ]) {
    assert.match(theme, new RegExp(`${token}:`));
  }
  assert.match(theme, /color-scheme:\s*dark/);
  assert.doesNotMatch(theme, /prefers-color-scheme|color-scheme:\s*light/i);
  assert.doesNotMatch(palette, /#[0-9a-f]{3,8}|rgba?\(/i);
  assert.doesNotMatch(shortcuts, /#[0-9a-f]{3,8}|rgba?\(/i);
  assert.notEqual(feedbackStart, -1);
  assert.doesNotMatch(feedbackStyles, /#[0-9a-f]{3,8}|rgba?\(/i);
  assert.match(palette, /var\(--theme-surface-overlay\)/);
  assert.match(shortcuts, /var\(--theme-focus-ring\)/);
  assert.match(feedbackStyles, /var\(--theme-status-correctable-border\)/);
  assert.match(styles, /background:\s*var\(--viewer-surround\)/);
  assert.match(styles, /background:\s*var\(--viewer-cinema-surround\)/);
  assert.match(styles, /background:\s*var\(--marker-red\)/);
  assert.match(styles, /background:\s*var\(--marker-white\)/);
});

test("native viewer CSS isolates theme chrome without transforming pixels", () => {
  const styles = read("src/styles.css");
  const nativeViewport = styles.match(/\.native-viewport \{([\s\S]*?)\n\}/)?.[1];
  const nativeFrame = styles.match(/\.native-viewport__frame \{([\s\S]*?)\n\}/)?.[1];

  assert.ok(nativeViewport, "native viewer block must exist");
  assert.ok(nativeFrame, "native viewer frame block must exist");
  assert.match(nativeViewport, /background:\s*var\(--viewer-surround\)/);
  assert.match(nativeViewport, /filter:\s*none/);
  assert.match(nativeViewport, /mix-blend-mode:\s*normal/);
  assert.match(nativeViewport, /opacity:\s*1/);
  assert.match(nativeFrame, /forced-color-adjust:\s*none/);
  assert.match(nativeFrame, /isolation:\s*isolate/);
});
