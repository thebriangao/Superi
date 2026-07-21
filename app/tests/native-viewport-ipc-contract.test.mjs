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
  assert.match(placement[0], /\n\s*view: DesktopViewerAnalysisView,/);
  assert.match(placement[0], /\n\s*x: f64,/);
  assert.match(placement[0], /\n\s*y: f64,/);
  assert.match(placement[0], /\n\s*width: f64,/);
  assert.match(placement[0], /\n\s*height: f64,/);
  assert.match(placement[0], /\n\s*scale_factor: f64,/);
  assert.match(placement[0], /\n\s*visible: bool,/);
  assert.match(placement[0], /\n\s*external_display_id: Option<String>,/);
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
  assert.equal(
    (nativeViewport.match(/view: analysisViewRef\.current/g) ?? []).length,
    2,
  );
  assert.equal(
    (nativeViewport.match(/externalDisplayId: externalDisplayIdRef\.current/g) ?? []).length,
    2,
  );
  assert.match(nativeViewport, /viewer-external-display/);
  assert.match(nativeViewport, /aria-label=\{`\$\{label\} external display`\}/);
  assert.match(nativeViewport, /snapshot\.externalDisplays/);
  assert.match(nativeViewport, /snapshot\.externalOutput/);
  assert.match(nativeViewport, /selectedView: ViewerAnalysisView;/);
  assert.match(nativeViewport, /presentedView: ViewerAnalysisView \| null;/);
  assert.match(nativeViewport, /snapshot\.selectedView/);
  assert.match(nativeViewport, /snapshot\.presentedView/);
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
  assert.match(rustViewport, /DesktopViewportSurfaceDestination::External/);
  assert.match(rustViewport, /external_children/);
  assert.match(rustViewport, /create_external_viewport/);
  assert.match(
    rustViewport,
    /destination == DesktopViewportSurfaceDestination::External[\s\S]*?external_output\.phase = "failed"[\s\S]*?continue;/,
  );
});
