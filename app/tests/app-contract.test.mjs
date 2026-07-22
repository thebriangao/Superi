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

test("command palette discovers stable application and native shell actions", () => {
  const application = read(resolve(appRoot, "src/application.ts"));
  const context = read(resolve(appRoot, "src/application-context.tsx"));
  const catalog = read(resolve(appRoot, "src/command-palette.ts"));
  const palette = read(resolve(appRoot, "src/command-palette.tsx"));
  const styles = read(resolve(appRoot, "src/command-palette.css"));
  const app = read(resolve(appRoot, "src/App.tsx"));
  const shell = read(resolve(appRoot, "src/desktop-shell.ts"));
  const nativeShell = read(resolve(tauriRoot, "src/desktop_shell.rs"));
  const packageJson = readJson(resolve(appRoot, "package.json"));

  assert.match(application, /commandPaletteOpen/);
  assert.match(application, /applicationCommandAvailability/);
  assert.match(context, /allowInEditableContext/);
  assert.match(catalog, /export class CommandPaletteCatalog/);
  assert.match(catalog, /desktopShellCommandPaletteActions/);
  assert.match(catalog, /executeCommandPaletteAction/);
  assert.match(palette, /role="listbox"/);
  assert.match(palette, /showModal\(\)/);
  assert.match(styles, /\.command-palette-dialog::backdrop/);
  assert.match(app, /application\.command_palette\.open/);
  assert.match(app, /application\.workspace_layout\.reset_all/);
  assert.match(app, /application\.workspace_layout\.undo_reset/);
  assert.match(app, /<CommandPalette/);
  assert.match(
    app,
    /intent\.kind === "request_close"[\s\S]*requestDesktopClose\(\)/,
  );
  assert.match(shell, /desktopShellIntentAutomationId/);
  assert.match(shell, /open_command_palette/);
  assert.match(nativeShell, /superi\.edit\.command_palette/);
  assert.match(nativeShell, /OpenCommandPalette/);
  assert.match(packageJson.scripts.test, /command-palette\.test\.ts/);
  assert.doesNotMatch(
    catalog + palette,
    /SuperiApiBindings|superi\.project\.command\.execute|@tauri-apps/,
  );
});

test("panel workspace exposes real dock, tab, resize, hide, and continuity behavior", () => {
  const application = read(resolve(appRoot, "src/application.ts"));
  const panelWorkspace = read(resolve(appRoot, "src/panel-workspace.tsx"));
  const app = read(resolve(appRoot, "src/App.tsx"));
  const shell = read(resolve(appRoot, "src/desktop-shell.ts"));
  const crash = read(resolve(appRoot, "src/crash-diagnostics.ts"));

  assert.match(app, /<PanelWorkspace\s*\/>/);
  assert.match(app, /applicationWorkspacePresentation\(state\)/);
  assert.match(app, /panel_layouts: workspace\.panel_layouts/);
  assert.match(application, /type: "dock_panel"/);
  assert.match(application, /type: "activate_panel"/);
  assert.match(application, /type: "resize_panel_dock"/);
  assert.match(application, /createPanelLayouts/);
  assert.match(application, /applicationWorkspacePresentation/);
  assert.match(panelWorkspace, /role="tablist"/);
  assert.match(panelWorkspace, /role="tabpanel"/);
  assert.match(panelWorkspace, /role="separator"/);
  assert.match(panelWorkspace, /draggable/);
  assert.match(panelWorkspace, /setPointerCapture/);
  assert.match(panelWorkspace, /hidden=\{!selected\}/);
  assert.match(panelWorkspace, /aria-label=\{`Dock \$\{activePanel\.title\}`\}/);
  assert.match(panelWorkspace, /type: "toggle_panel"/);
  assert.match(shell, /ApplicationWorkspacePresentation/);
  assert.match(crash, /ApplicationRoutePanelLayoutPresentation/);
  assert.doesNotMatch(
    panelWorkspace,
    /useSuperiApi|executeDesktopProject|undo_depth|redo_depth|resolveDesktopClose/,
  );
});

test("workspace header exposes saved layout recovery and authoritative engine state", () => {
  const application = read(resolve(appRoot, "src/application.ts"));
  const app = read(resolve(appRoot, "src/App.tsx"));
  const styles = read(resolve(appRoot, "src/styles.css"));

  assert.match(application, /applicationWorkspaceLayoutStatus/);
  assert.match(application, /type: "reset_workspace_layouts"/);
  assert.match(application, /type: "undo_workspace_layout_reset"/);
  assert.match(application, /workspaceLayoutResetUndo/);
  assert.match(app, /Reset all layouts/);
  assert.match(app, /Undo reset/);
  assert.match(app, /Default, saved/);
  assert.match(app, /Custom, saved/);
  assert.match(app, /Default, session only/);
  assert.match(app, /latestHeaderLifecycleRevision/);
  assert.match(app, /await getDesktopLifecycle\(\)/);
  assert.match(app, /window\.setTimeout\(refresh, 1_000\)/);
  assert.match(app, /data-engine-state/);
  assert.match(
    app,
    /executeCommand\("application\.route\.system"\)/,
  );
  assert.match(styles, /\.workspace-layout-controls/);
  assert.match(styles, /\.workspace-layout-state/);
  assert.match(styles, /\.engine-state-control/);
  assert.doesNotMatch(
    application,
    /@tauri-apps|desktop_lifecycle_snapshot|requestDesktopLifecycle/,
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
  assert.match(associations, /runtime\.spawn_background_task/);
  assert.doesNotMatch(associations, /tauri::async_runtime::spawn_blocking/);
  assert.match(associations, /DesktopProjectCommand::Open/);
  assert.match(associations, /superi:\/\/project-opened/);
  assert.match(projectAdapter, /listenForDesktopProjectOpen/);
  assert.match(projectAdapter, /superi:\/\/project-opened/);
  assert.match(app, /await listenForDesktopProjectOpen/);
  assert.match(app, /snapshot\.revision <= latestProjectRevision\.current/);
  assert.match(app, /acceptProjectSnapshot\(event\.snapshot\)/);
  assert.match(app, /void refreshEditorProject\(\)/);
});

test("desktop process retains every long-lived shell owner and exposes cleanup state", () => {
  const bridge = read(resolve(appRoot, "src/lifecycle.ts"));
  const app = read(resolve(appRoot, "src/App.tsx"));
  const host = read(resolve(tauriRoot, "src/lib.rs"));
  const runtime = read(resolve(tauriRoot, "src/process_runtime.rs"));
  const engine = read(resolve(tauriRoot, "src/engine.rs"));
  const viewport = read(resolve(tauriRoot, "src/viewport.rs"));
  const windows = read(resolve(tauriRoot, "src/window_session.rs"));

  assert.match(runtime, /DesktopProcessServiceId::ALL/);
  assert.match(runtime, /exit_monitor: Option<JoinHandle/);
  assert.match(runtime, /background_tasks: Vec<ShellTaskHandle>/);
  assert.match(runtime, /join_application_exit/);
  assert.match(runtime, /join_background_tasks/);
  assert.match(runtime, /accepting_background_tasks = false/);
  assert.match(engine, /launch_with_runtime/);
  assert.match(engine, /match worker\.join\(\)/);
  assert.match(engine, /match playback_worker\.join\(\)/);
  assert.match(engine, /worker_pool\.shutdown/);
  assert.match(viewport, /pub fn shutdown_and_join\(&self\) -> Result<\(\)>/);
  assert.match(windows, /pub fn shutdown_and_join\(&self\) -> Result<\(\)>/);
  assert.match(host, /desktop_process_snapshot/);
  assert.match(host, /join_process_owners/);
  assert.match(host, /runtime\.join_application_exit\(\)/);
  assert.match(host, /runtime\.join_background_tasks\(\)/);
  assert.match(host, /viewport\.shutdown_and_join\(\)/);
  assert.match(bridge, /desktop_process_snapshot/);
  assert.match(bridge, /DesktopProcessServiceSnapshot/);
  assert.match(app, /Process ownership/);
  assert.match(app, /service\.join_pending/);
  assert.doesNotMatch(
    host + runtime + engine + viewport + windows,
    /std::process::Command|use std::process/,
  );
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

test("system capability discovery composes authoritative providers without changing media state", () => {
  const cargo = read(resolve(tauriRoot, "Cargo.toml"));
  const host = read(resolve(tauriRoot, "src/lib.rs"));
  const capabilities = read(resolve(tauriRoot, "src/capabilities.rs"));
  const app = read(resolve(appRoot, "src/App.tsx"));
  const adapter = read(resolve(appRoot, "src/system-capabilities.ts"));
  const packageJson = readJson(resolve(appRoot, "package.json"));

  assert.match(cargo, /^superi-ai = \{ path = "\.\.\/\.\.\/open\/crates\/superi-ai" \}/m);
  assert.match(cargo, /^superi-audio = \{ path = "\.\.\/\.\.\/open\/crates\/superi-audio" \}/m);
  assert.match(host, /pub mod capabilities/);
  assert.match(host, /DesktopCapabilityState::default/);
  assert.match(host, /desktop_capabilities_discover/);
  assert.match(capabilities, /tauri::async_runtime::spawn_blocking/);
  assert.match(capabilities, /GpuInstance::new/);
  assert.match(capabilities, /enumerate_adapters/);
  assert.match(capabilities, /discover_output_devices/);
  assert.match(capabilities, /discover_input_devices/);
  assert.match(capabilities, /media_backend_registry/);
  assert.match(capabilities, /MediaCapabilities::from_registry/);
  assert.match(capabilities, /MediaCapabilitiesApi::new/);
  assert.match(capabilities, /discover_local_capabilities/);
  assert.doesNotMatch(
    capabilities,
    /start_device_output|start_device_capture|create_output_buffer|create_capture_buffer|request_discard|\.play\(|\.pause\(/,
  );
  assert.match(adapter, /desktop_capabilities_discover/);
  assert.match(app, /discoverDesktopCapabilities/);
  assert.match(app, /Hardware capabilities/);
  assert.match(app, /Channel meaning is never inferred/);
  assert.match(packageJson.scripts.test, /system-capabilities\.test\.ts/);
});
