import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";

const appRoot = new URL("../", import.meta.url);
const rustViewport = readFileSync(
  new URL("src-tauri/src/viewport.rs", appRoot),
  "utf8",
);
const nativeViewport = readFileSync(
  new URL("src/native-viewport.tsx", appRoot),
  "utf8",
);

test("native viewport IPC accepts control placement only", () => {
  const placement = rustViewport.match(
    /#\[serde\(rename_all = "camelCase", deny_unknown_fields\)\]\s*pub struct DesktopViewportPlacement \{[\s\S]*?\n\}/,
  );

  assert.ok(placement, "viewport placement must reject unknown Tauri fields");
  assert.match(placement[0], /\n\s*x: f64,/);
  assert.match(placement[0], /\n\s*y: f64,/);
  assert.match(placement[0], /\n\s*width: f64,/);
  assert.match(placement[0], /\n\s*height: f64,/);
  assert.match(placement[0], /\n\s*scale_factor: f64,/);
  assert.match(placement[0], /\n\s*visible: bool,/);
  assert.doesNotMatch(
    placement[0],
    /frame|image|pixel|texture|handle|bytes|blob|base64/i,
  );

  const invokedCommands = [
    ...nativeViewport.matchAll(/invoke(?:<[^>]+>)?\(\s*"([^"]+)"/g),
  ].map((match) => match[1]);
  assert.deepEqual(invokedCommands, [
    "desktop_viewport_update",
    "desktop_viewport_update",
  ]);
  assert.match(
    nativeViewport,
    /invoke<ViewportSnapshot>\(\s*"desktop_viewport_update",\s*\{\s*placement:\s*\{/,
  );
  assert.doesNotMatch(
    nativeViewport,
    /data:image|createObjectURL|readPixels|toDataURL|transferToImageBitmap/i,
  );
  assert.match(nativeViewport, /viewer-editorial-feedback/);
  assert.match(nativeViewport, /data-phase=\{feedback\.phase\}/);
  assert.match(nativeViewport, /EditorialAudioMeters/);
  assert.doesNotMatch(
    nativeViewport,
    /placement:\s*\{[\s\S]{0,500}(?:feedback|multicam|audio|meter)/i,
  );
});
