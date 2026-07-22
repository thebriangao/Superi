import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const styles = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");
const theme = readFileSync(new URL("../src/theme.css", import.meta.url), "utf8");

test("typography scales with browser zoom while controls inherit readable text", () => {
  assert.match(styles, /font-size:\s*clamp\(/);
  assert.match(styles, /text-size-adjust:\s*100%/);
  assert.match(styles, /button,[\s\S]*input,[\s\S]*font:\s*inherit/);
  assert.match(theme, /--theme-font-size-min:/);
  assert.match(theme, /--theme-font-size-max:/);
});

test("focus, contrast, reduced motion, and status never depend on color alone", () => {
  assert.match(styles, /:where\([\s\S]*\):focus-visible/);
  assert.match(styles, /--theme-focus-width/);
  assert.match(theme, /@media \(prefers-contrast: more\)/);
  assert.match(styles, /@media \(prefers-reduced-motion: reduce\)/);
  assert.match(styles, /animation-duration:\s*0\.01ms !important/);
  assert.match(styles, /data-status-condition="attention"[\s\S]*content:\s*"!"/);
  assert.match(styles, /data-status-condition="degraded"[\s\S]*border-style:\s*dashed/);
  assert.doesNotMatch(theme, /--viewer-surround:\s*var\(--theme/);
});
