import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = resolve(appRoot, "..");
const tauriRoot = resolve(appRoot, "src-tauri");

function read(path) {
  return readFileSync(path, "utf8");
}

function readJson(path) {
  return JSON.parse(read(path));
}

test("production app pins the approved React, Vite, TypeScript, and Tauri toolchains", () => {
  const packageJson = readJson(resolve(appRoot, "package.json"));
  const cargo = read(resolve(tauriRoot, "Cargo.toml"));

  assert.equal(packageJson.engines.node, "24.13.0");
  assert.equal(packageJson.dependencies.react, "19.2.7");
  assert.equal(packageJson.dependencies["react-dom"], "19.2.7");
  assert.equal(packageJson.dependencies["@tauri-apps/api"], "2.11.1");
  assert.equal(packageJson.devDependencies.typescript, "5.9.3");
  assert.equal(packageJson.devDependencies.vite, "7.3.6");
  assert.equal(packageJson.devDependencies["@vitejs/plugin-react"], "5.2.0");
  assert.match(cargo, /^tauri = \{ version = "=2\.11\.5",/m);
  assert.match(cargo, /^tauri-build = \{ version = "=2\.6\.3"/m);
});

test("lifecycle seam is explicit without claiming adjacent process or API binding work", () => {
  const frontend = read(resolve(appRoot, "src/lifecycle.ts"));
  const lifecycle = read(resolve(tauriRoot, "src/lifecycle.rs"));
  const host = read(resolve(tauriRoot, "src/lib.rs"));

  assert.match(frontend, /desktop_lifecycle_snapshot/);
  assert.match(frontend, /desktop_lifecycle_request/);
  assert.match(frontend, /application_phase/);
  assert.match(frontend, /engine_phase/);
  assert.match(lifecycle, /LifecycleCoordinator/);
  assert.match(lifecycle, /headless-engine/);
  assert.match(lifecycle, /request_restart/);
  assert.match(lifecycle, /request_recovery/);
  assert.match(host, /RunEvent::ExitRequested/);
  assert.doesNotMatch(frontend, /open\/bindings\/typescript\/superi-api/);
  assert.doesNotMatch(lifecycle, /Command::new|process::Command|TcpStream/);
});

test("blocking workflows exercise the production app rather than CI-only smoke packages", () => {
  const frontendWorkflow = read(
    resolve(repositoryRoot, ".github/workflows/frontend.yml"),
  );
  const tauriWorkflow = read(
    resolve(repositoryRoot, ".github/workflows/tauri.yml"),
  );

  assert.match(frontendWorkflow, /working-directory: app/);
  assert.match(frontendWorkflow, /node-version-file: app\/.node-version/);
  assert.match(frontendWorkflow, /cache-dependency-path: app\/package-lock\.json/);
  assert.doesNotMatch(frontendWorkflow, /working-directory: ci\/frontend-smoke/);
  assert.match(tauriWorkflow, /working-directory: app\/src-tauri/);
  assert.match(tauriWorkflow, /run: cargo build --locked --bin superi-desktop/);
  assert.doesNotMatch(tauriWorkflow, /working-directory: ci\/tauri-smoke\/src-tauri/);
});

test("production build contains a generated hashed React entry", () => {
  const distRoot = resolve(appRoot, "dist");
  const html = read(resolve(distRoot, "index.html"));
  const assets = readdirSync(resolve(distRoot, "assets"));
  const script = assets.find((name) => /^index-[a-zA-Z0-9_-]+\.js$/.test(name));

  assert.ok(script, `missing hashed JavaScript entry in ${assets.join(", ")}`);
  assert.match(html, new RegExp(`/assets/${script.replace(".", "\\.")}`));
  assert.match(html, /<title>Superi<\/title>/);
});
