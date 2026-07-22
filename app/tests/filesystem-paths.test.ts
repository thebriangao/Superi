import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

import {
  filesystemPathBasename,
  filesystemPathKey,
  filesystemPathUnderRoot,
  removableVolumeBehavior,
} from "../src/filesystem-paths.ts";

test("path construction preserves original Unicode filenames and separator style", () => {
  const decomposed = "Cafe\u0301 片段.mov";
  assert.equal(filesystemPathBasename(`/Media/${decomposed}`), decomposed);
  assert.equal(filesystemPathUnderRoot("/Volumes/RAID/", `C:\\Camera\\${decomposed}`), `/Volumes/RAID/${decomposed}`);
  assert.equal(filesystemPathUnderRoot("D:\\Media\\", `/Camera/${decomposed}`), `D:\\Media\\${decomposed}`);
  assert.equal(filesystemPathUnderRoot("", `/Camera/${decomposed}`), decomposed);
});

test("comparison keys normalize syntax and Unicode only under explicit case policy", () => {
  const composed = "/Media/Café/片段.MOV";
  const decomposed = "\\Media\\Cafe\u0301\\片段.MOV";
  assert.equal(filesystemPathKey(composed, "sensitive"), filesystemPathKey(decomposed, "sensitive"));
  assert.notEqual(filesystemPathKey(composed, "sensitive"), filesystemPathKey(composed.toLowerCase(), "sensitive"));
  assert.equal(filesystemPathKey(composed, "insensitive"), filesystemPathKey(composed.toLowerCase(), "insensitive"));
});

test("removable volume behavior is deterministic and consumed by the media UI", () => {
  assert.equal(removableVolumeBehavior({ kind: "removable", status: "offline" }, "volume_offline"), "wait_for_volume");
  assert.equal(removableVolumeBehavior({ kind: "system", status: "mounted" }, "missing"), "locate_source");
  assert.equal(removableVolumeBehavior({ kind: "system", status: "mounted" }, "changed"), "review_changed_source");
  assert.equal(removableVolumeBehavior({ kind: "unknown", status: "mounted" }, "unchanged"), "ready");
  const app = readFileSync(new URL("../src/App.tsx", import.meta.url), "utf8");
  assert.match(app, /filesystemPathUnderRoot\(batchRelinkRoot\.trim\(\), path\)/);
  assert.match(app, /removableVolumeBehavior\(sourcePath\.volume, sourcePath\.status\)/);
});
