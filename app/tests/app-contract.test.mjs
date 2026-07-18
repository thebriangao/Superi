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

test("engine process link remains routing-free beneath the desktop transport", () => {
  const frontend = read(resolve(appRoot, "src/lifecycle.ts"));
  const cargo = read(resolve(tauriRoot, "Cargo.toml"));
  const engine = read(resolve(tauriRoot, "src/engine.rs"));
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
  assert.match(cargo, /^superi-api = \{ path = "\.\.\/\.\.\/open\/crates\/superi-api" \}/m);
  assert.match(cargo, /^superi-engine = \{ path = "\.\.\/\.\.\/open\/crates\/superi-engine" \}/m);
  assert.match(engine, /ExecutionDomain::EngineControl/);
  assert.match(engine, /EngineCommandDispatcher::new/);
  assert.match(engine, /sync_channel\(REQUEST_CAPACITY\)/);
  assert.match(engine, /\.try_send\(EngineRequest::IntegrationValidation/);
  assert.match(engine, /IntegrationValidationApi::new/);
  assert.match(host, /LinkedEngineProcess::launch/);
  assert.match(host, /\.manage\(engine\.connection\(\)\)/);
  assert.match(host, /RunEvent::ExitRequested/);
  assert.doesNotMatch(frontend, /open\/bindings\/typescript\/superi-api/);
  assert.doesNotMatch(engine, /LocalProjectHost|JsonRpc|tauri::command|reconnect|cancel/);
  assert.doesNotMatch(lifecycle, /Command::new|process::Command|TcpStream/);
});

test("application framework composes shared UI state above the delivered API client", () => {
  const application = read(resolve(appRoot, "src/application.ts"));
  const context = read(resolve(appRoot, "src/application-context.tsx"));
  const app = read(resolve(appRoot, "src/App.tsx"));
  const main = read(resolve(appRoot, "src/main.tsx"));
  const transport = read(resolve(appRoot, "src/transport.ts"));
  const packageJson = readJson(resolve(appRoot, "package.json"));

  assert.match(application, /export class ApplicationRegistry/);
  assert.match(application, /export function reduceApplicationState/);
  assert.match(application, /export async function executeApplicationCommand/);
  assert.match(application, /PublicResourceReference/);
  assert.match(context, /export function ApplicationProvider/);
  assert.match(context, /export function useApplication/);
  assert.match(app, /new ApplicationRegistry/);
  assert.match(app, /<ApplicationProvider registry=\{APPLICATION_REGISTRY\}>/);
  assert.match(app, /superi\.engine\.introspection/);
  assert.match(main, /new DesktopSuperiTransport\(\)/);
  assert.match(transport, /implements SuperiTransport/);
  assert.match(packageJson.scripts.test, /application-framework\.test\.ts/);
  assert.doesNotMatch(
    application,
    /@tauri-apps|desktop_api_dispatch|DesktopSuperiTransport|reconnect|cancelRequest/,
  );
  assert.doesNotMatch(
    context,
    /@tauri-apps|desktop_api_dispatch|DesktopSuperiTransport|reconnect|cancelRequest/,
  );
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
