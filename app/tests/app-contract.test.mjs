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

test("local crash diagnostics retain private evidence and recover through existing owners", () => {
  const bridge = read(resolve(appRoot, "src/crash-diagnostics.ts"));
  const app = read(resolve(appRoot, "src/App.tsx"));
  const diagnostics = read(
    resolve(tauriRoot, "src/crash_diagnostics.rs"),
  );
  const host = read(resolve(tauriRoot, "src/lib.rs"));

  assert.match(diagnostics, /active-session\.json/);
  assert.match(diagnostics, /MAX_RETAINED_DIAGNOSTICS/);
  assert.match(diagnostics, /DesktopCrashFailureClass/);
  assert.match(diagnostics, /Retryable/);
  assert.match(diagnostics, /Degraded/);
  assert.match(diagnostics, /UserCorrectable/);
  assert.match(diagnostics, /Terminal/);
  assert.match(diagnostics, /private_detail/);
  assert.match(diagnostics, /record_panic_best_effort/);
  assert.match(diagnostics, /finish_session/);
  assert.match(host, /DesktopCrashDiagnostics::default/);
  assert.match(host, /desktop_crash_diagnostics_snapshot/);
  assert.match(host, /desktop_crash_workspace_update/);
  assert.match(host, /desktop_crash_project_update/);
  assert.match(host, /desktop_crash_diagnostic_dismiss/);
  assert.match(host, /diagnostics\.install_panic_hook\(\)/);
  assert.match(host, /diagnostics\.observe_lifecycle\(&snapshot\)/);
  assert.match(host, /diagnostics\.finish_session\(\)/);
  assert.match(bridge, /DesktopCrashFailureClass/);
  assert.match(bridge, /desktop_crash_diagnostics_snapshot/);
  assert.match(bridge, /desktop_crash_workspace_update/);
  assert.match(bridge, /desktop_crash_project_update/);
  assert.match(bridge, /desktop_crash_diagnostic_dismiss/);
  assert.match(bridge, /workspaceUpdateTail\.then/);
  assert.match(bridge, /projectUpdateTail\.then/);
  assert.doesNotMatch(bridge, /private_detail|superi\.api|superi\.project\.recovery/);
  assert.match(app, /updateDesktopCrashWorkspace/);
  assert.match(app, /updateDesktopCrashProject/);
  assert.match(app, /!windowSessionHydrated/);
  assert.match(app, /getCurrentWebviewWindow\(\)\.label !== "main"/);
  assert.match(app, /Restore workspace/);
  assert.match(app, /Review project recovery/);
  assert.match(app, /executeDesktopProject/);
  assert.match(app, /requestDesktopLifecycle/);
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

test("development shell owns exact Superi project file association ingress", () => {
  const config = readJson(resolve(tauriRoot, "tauri.conf.json"));
  const host = read(resolve(tauriRoot, "src/lib.rs"));
  const associations = read(resolve(tauriRoot, "src/file_associations.rs"));
  const projectAdapter = read(resolve(appRoot, "src/project-lifecycle.ts"));
  const app = read(resolve(appRoot, "src/App.tsx"));

  assert.deepEqual(config.bundle.fileAssociations, [
    {
      ext: ["superi"],
      contentTypes: ["com.superi.project"],
      name: "Superi Project",
      description: "Superi project",
      role: "Editor",
      mimeType: "application/x-superi-project",
      rank: "Owner",
      exportedType: {
        identifier: "com.superi.project",
        conformsTo: ["public.database", "public.data"],
      },
    },
  ]);
  assert.match(host, /file_associations::route_startup_project_files/);
  assert.match(host, /RunEvent::Opened \{ urls \}/);
  assert.match(host, /file_associations::route_opened_project_urls/);
  assert.match(associations, /tauri::async_runtime::spawn_blocking/);
  assert.match(associations, /DesktopProjectCommand::Open/);
  assert.match(associations, /superi:\/\/project-opened/);
  assert.match(projectAdapter, /listenForDesktopProjectOpen/);
  assert.match(projectAdapter, /superi:\/\/project-opened/);
  assert.match(app, /await listenForDesktopProjectOpen/);
  assert.match(app, /snapshot\.revision <= latestProjectRevision\.current/);
  assert.match(app, /acceptProjectSnapshot\(event\.snapshot\)/);
  assert.match(app, /void refreshEditorProject\(\)/);
});

test("native desktop commands compose with persistent multi-window ownership", () => {
  const host = read(resolve(tauriRoot, "src/lib.rs"));
  const shell = read(resolve(tauriRoot, "src/desktop_shell.rs"));
  const windows = read(resolve(tauriRoot, "src/window_session.rs"));
  const app = read(resolve(appRoot, "src/App.tsx"));
  const application = read(resolve(appRoot, "src/application.ts"));

  assert.match(host, /desktop_shell::handle_window_event\(window, event\)/);
  assert.match(host, /state\.handle_window_event\(window, event, transport\.inner\(\)\)/);
  assert.match(shell, /window\.label\(\) != "main"/);
  assert.match(shell, /emit_to\(&target, DESKTOP_SHELL_EVENT, intent\)/);
  assert.match(shell, /lifecycle\.0\.request_shutdown\(\)/);
  assert.match(windows, /pub\(crate\) fn command_target/);
  assert.doesNotMatch(windows, /lifecycle\.request_shutdown/);
  assert.match(app, /currentWindowLabel !== "main"/);
  assert.match(app, /type: "restore_workspace_presentation"/);
  assert.match(application, /active_route_id: state\.activeRouteId/);
});
