import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const appRoot = new URL("../", import.meta.url);
const theme = readFileSync(new URL("src/theme.css", appRoot), "utf8");
const styles = readFileSync(new URL("src/styles.css", appRoot), "utf8");

test("theme publishes offline fallback stacks for supported writing systems", () => {
  for (const token of ["ui", "title", "caption", "metadata", "mono"]) {
    assert.match(theme, new RegExp(`--theme-font-${token}:`));
  }
  for (const family of [
    "Noto Sans Arabic",
    "Noto Sans Hebrew",
    "Noto Sans Devanagari",
    "Noto Sans CJK SC",
    "Noto Sans CJK JP",
    "Noto Sans CJK KR",
    "Noto Color Emoji",
  ]) {
    assert.match(theme, new RegExp(`\\"${family}\\"`));
  }
  assert.doesNotMatch(theme, /@font-face|url\s*\(/i);
});

test("UI, titles, captions, metadata, and exact labels consume role stacks", () => {
  assert.match(styles, /font-family:\s*var\(--theme-font-ui\)/);
  assert.match(styles, /:where\(h1, h2, h3, h4, h5, h6, \.timeline-toolbar-title\)/);
  assert.match(styles, /font-family:\s*var\(--theme-font-title\)/);
  assert.match(styles, /\.timeline-lane-caption,[\s\S]*\.timeline-item-caption,[\s\S]*\.timeline-marker-panel/);
  assert.match(styles, /font-family:\s*var\(--theme-font-caption\)/);
  assert.match(styles, /\.media-metadata,[\s\S]*\.source-metadata-status,[\s\S]*\.user-metadata-editor/);
  assert.match(styles, /font-family:\s*var\(--theme-font-metadata\)/);
  assert.match(styles, /:where\(code, kbd, \.code-value, \[data-path\], \[data-timecode\]\)/);
  assert.match(styles, /font-family:\s*var\(--theme-font-mono\)/);
});
