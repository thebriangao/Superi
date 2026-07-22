import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const styles = readFileSync(new URL("../src/styles.css", import.meta.url), "utf8");

test("RTL mirrors application chrome with logical reading alignment", () => {
  assert.match(styles, /\[dir="rtl"\] \.application-shell[\s\S]*grid-template-columns:\s*minmax\(0, 1fr\) 208px/);
  assert.match(styles, /\[dir="rtl"\] \.application-sidebar[\s\S]*border-left:/);
  assert.match(styles, /\[dir="rtl"\] \.application-workspace[\s\S]*grid-column:\s*1/);
  assert.match(styles, /\[dir="rtl"\] \.application-toast-region[\s\S]*left:\s*14px/);
  assert.match(styles, /text-align:\s*start/);
});

test("paths, shortcuts, timecode, timeline, and viewer coordinates remain stable", () => {
  for (const token of ["code", "kbd", "data-path", "data-timecode", "timeline-stage", "native-viewport__frame"]) {
    assert.match(styles, new RegExp(token));
  }
  assert.match(styles, /direction:\s*ltr/);
  assert.match(styles, /unicode-bidi:\s*isolate/);
});
