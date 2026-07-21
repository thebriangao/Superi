import assert from "node:assert/strict";
import test from "node:test";

import {
  ApplicationRegistry,
  createApplicationState,
  executeApplicationCommand,
  isEditableCommandTarget,
  normalizeShortcut,
  reduceApplicationState,
  restoreApplicationWorkspace,
} from "../src/application.ts";

function definitions() {
  return {
    defaultRouteId: "workspace",
    panels: [
      {
        id: "application.overview",
        title: "Overview",
        region: "primary",
        renderer: "overview",
      },
      {
        id: "application.selection",
        title: "Selection",
        region: "secondary",
        renderer: "selection",
      },
      {
        id: "application.lifecycle",
        title: "Lifecycle",
        region: "primary",
        renderer: "lifecycle",
      },
    ],
    routes: [
      {
        id: "workspace",
        title: "Workspace",
        panelIds: ["application.overview", "application.selection"],
        defaultPanelId: "application.overview",
      },
      {
        id: "system",
        title: "System",
        panelIds: ["application.lifecycle"],
        defaultPanelId: "application.lifecycle",
      },
    ],
    commands: [
      {
        id: "application.route.system",
        title: "Open system route",
        shortcut: "Mod+2",
        execute: ({ dispatch }) => {
          dispatch({ type: "navigate", routeId: "system" });
        },
      },
      {
        id: "application.engine.refresh",
        title: "Refresh engine state",
        shortcut: "Mod+R",
        isEnabled: ({ api }) => api !== null,
        execute: ({ api, dispatch }) => {
          dispatch({ type: "navigate", routeId: "system" });
          return api.request("superi.engine.integration.validation.get", null);
        },
      },
    ],
  };
}

test("registry rejects duplicate identities, shortcuts, and missing panel references", () => {
  const valid = definitions();
  assert.throws(
    () =>
      new ApplicationRegistry({
        ...valid,
        panels: [...valid.panels, valid.panels[0]],
      }),
    /duplicate panel.*application\.overview/i,
  );
  assert.throws(
    () =>
      new ApplicationRegistry({
        ...valid,
        routes: [...valid.routes, valid.routes[0]],
      }),
    /duplicate route.*workspace/i,
  );
  assert.throws(
    () =>
      new ApplicationRegistry({
        ...valid,
        commands: [...valid.commands, valid.commands[0]],
      }),
    /duplicate command.*application\.route\.system/i,
  );
  assert.throws(
    () =>
      new ApplicationRegistry({
        ...valid,
        commands: [
          ...valid.commands,
          {
            id: "application.route.system.alternate",
            title: "Alternate system route",
            shortcut: " mod + 2 ",
            execute() {},
          },
        ],
      }),
    /duplicate shortcut.*mod\+2/i,
  );
  assert.throws(
    () =>
      new ApplicationRegistry({
        ...valid,
        routes: [
          ...valid.routes,
          {
            id: "invalid",
            title: "Invalid",
            panelIds: ["application.missing"],
          },
        ],
      }),
    /unknown panel.*application\.missing/i,
  );
});

test("routing reconciles visible and focused panels without mutating the prior snapshot", () => {
  const registry = new ApplicationRegistry(definitions());
  const initial = createApplicationState(registry);
  assert.equal(initial.activeRouteId, "workspace");
  assert.deepEqual(initial.visiblePanelIds, [
    "application.overview",
    "application.selection",
  ]);
  assert.equal(initial.focusedPanelId, "application.overview");

  const focused = reduceApplicationState(registry, initial, {
    type: "focus_panel",
    panelId: "application.selection",
  });
  const hidden = reduceApplicationState(registry, focused, {
    type: "toggle_panel",
    panelId: "application.selection",
  });
  assert.equal(focused.focusedPanelId, "application.selection");
  assert.deepEqual(hidden.visiblePanelIds, ["application.overview"]);
  assert.equal(hidden.focusedPanelId, "application.overview");
  assert.deepEqual(initial.visiblePanelIds, [
    "application.overview",
    "application.selection",
  ]);

  const system = reduceApplicationState(registry, hidden, {
    type: "navigate",
    routeId: "system",
  });
  assert.deepEqual(system.visiblePanelIds, ["application.lifecycle"]);
  assert.equal(system.focusedPanelId, "application.lifecycle");
  assert.throws(
    () =>
      reduceApplicationState(registry, system, {
        type: "navigate",
        routeId: "missing",
      }),
    /unknown route.*missing/i,
  );
  assert.equal(system.activeRouteId, "system");
});

test("shared selection preserves exact immutable public resource references", () => {
  const registry = new ApplicationRegistry(definitions());
  const engine = {
    resource: "superi.engine.introspection",
    schema_version: "1.0.0",
    identity: "engine",
    revision: 9,
  };
  const audio = {
    resource: "superi.audio.automation",
    schema_version: "1.0.0",
    identity: "clip/dialogue",
    revision: 17,
  };
  const initial = createApplicationState(registry);
  const selected = reduceApplicationState(registry, initial, {
    type: "replace_selection",
    items: [engine, audio],
    anchor: audio,
  });

  assert.deepEqual(selected.selection.items, [engine, audio]);
  assert.deepEqual(selected.selection.anchor, audio);
  assert.ok(Object.isFrozen(selected));
  assert.ok(Object.isFrozen(selected.selection));
  assert.ok(Object.isFrozen(selected.selection.items));
  assert.ok(Object.isFrozen(selected.selection.items[0]));
  assert.equal(selected.selection.items[0].schema_version, "1.0.0");

  const refreshedEngine = { ...engine, revision: 10 };
  const extended = reduceApplicationState(registry, selected, {
    type: "extend_selection",
    item: refreshedEngine,
  });
  assert.deepEqual(extended.selection.items, [refreshedEngine, audio]);
  assert.equal(extended.selection.anchor.revision, 10);

  const cleared = reduceApplicationState(registry, extended, {
    type: "clear_selection",
  });
  assert.deepEqual(cleared.selection.items, []);
  assert.equal(cleared.selection.anchor, null);
});

test("typed commands update local state before awaiting the generated API client", async () => {
  const registry = new ApplicationRegistry(definitions());
  let state = createApplicationState(registry);
  let completeRequest;
  const request = new Promise((resolve) => {
    completeRequest = resolve;
  });
  const calls = [];
  const api = {
    request(method, payload) {
      calls.push({ method, payload });
      return request;
    },
  };
  const pending = executeApplicationCommand({
    registry,
    state: () => state,
    api,
    dispatch(action) {
      state = reduceApplicationState(registry, state, action);
    },
    commandId: "application.engine.refresh",
  });

  assert.equal(state.activeRouteId, "system");
  assert.deepEqual(calls, [
    {
      method: "superi.engine.integration.validation.get",
      payload: null,
    },
  ]);
  completeRequest({ snapshot: { coherent: true } });
  assert.deepEqual(await pending, { status: "completed" });

  const disabled = await executeApplicationCommand({
    registry,
    state: () => state,
    api: null,
    dispatch() {
      assert.fail("disabled command must not dispatch");
    },
    commandId: "application.engine.refresh",
  });
  assert.deepEqual(disabled, { status: "disabled" });
});

test("keyboard helpers canonicalize shortcuts and preserve editable controls", () => {
  assert.equal(normalizeShortcut(" Shift + Mod + K "), "mod+shift+k");
  assert.equal(normalizeShortcut("Control+Alt+R"), "ctrl+alt+r");
  assert.equal(isEditableCommandTarget({ tagName: "INPUT" }), true);
  assert.equal(isEditableCommandTarget({ tagName: "textarea" }), true);
  assert.equal(isEditableCommandTarget({ tagName: "select" }), true);
  assert.equal(isEditableCommandTarget({ tagName: "button" }), false);
  assert.equal(isEditableCommandTarget({ isContentEditable: true }), true);
  assert.equal(isEditableCommandTarget(null), false);
});

test("workspace restoration reconciles persisted routes and panels against the live registry", () => {
  const registry = new ApplicationRegistry(definitions());
  const initial = createApplicationState(registry);
  const restored = restoreApplicationWorkspace(registry, initial, {
    active_route_id: "system",
    hidden_panel_ids: ["application.selection", "removed.panel"],
    focused_panel_id: "removed.panel",
  });
  assert.equal(restored.activeRouteId, "system");
  assert.deepEqual(restored.hiddenPanelIds, ["application.selection"]);
  assert.deepEqual(restored.visiblePanelIds, ["application.lifecycle"]);
  assert.equal(restored.focusedPanelId, "application.lifecycle");

  const windowOwnedRoute = reduceApplicationState(registry, restored, {
    type: "restore_workspace_presentation",
    workspace: {
      active_route_id: "workspace",
      hidden_panel_ids: ["application.selection"],
      focused_panel_id: "application.overview",
    },
  });
  assert.equal(windowOwnedRoute.activeRouteId, "system");
  assert.deepEqual(windowOwnedRoute.visiblePanelIds, ["application.lifecycle"]);
  assert.equal(windowOwnedRoute.focusedPanelId, "application.lifecycle");

  const fallback = restoreApplicationWorkspace(registry, restored, {
    active_route_id: "removed.route",
    hidden_panel_ids: [],
    focused_panel_id: null,
  });
  assert.equal(fallback.activeRouteId, registry.defaultRouteId);
  assert.deepEqual(fallback.visiblePanelIds, [
    "application.overview",
    "application.selection",
  ]);
});
