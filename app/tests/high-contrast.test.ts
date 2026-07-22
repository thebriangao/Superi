import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const styles = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");

test("forced-colors chrome uses system colors while scene data stays isolated", () => {
  assert.match(styles, /@media \(forced-colors: active\)/);
  for (const color of ["Canvas", "CanvasText", "ButtonText", "GrayText", "Highlight"]) {
    assert.match(styles, new RegExp(`:\\s*${color}`));
  }
  assert.match(styles, /native-viewport__frame,[\s\S]*forced-color-adjust:\s*none/);
  assert.match(styles, /timeline-marker-color[\s\S]*forced-color-adjust:\s*none/);
});

test("state meaning survives color-vision differences through symbols and borders", () => {
  assert.match(styles, /content-fresh::before[\s\S]*content:\s*"✓/);
  assert.match(styles, /content-stale[\s\S]*border:\s*2px dashed/);
  assert.match(styles, /data-tone="warning"[\s\S]*content:\s*"!"/);
  assert.match(styles, /data-failure-condition="degraded"[\s\S]*border-left-style:\s*dashed/);
  assert.match(styles, /data-failure-condition="user_correctable"[\s\S]*border-left-style:\s*double/);
  assert.match(styles, /data-failure-condition="terminal"[\s\S]*border-left-width:\s*6px/);
});
