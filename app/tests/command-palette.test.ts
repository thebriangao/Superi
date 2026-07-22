import assert from "node:assert/strict";
import test from "node:test";

import type { ApplicationCommandDefinition } from "../src/application.ts";
import {
  CommandPaletteCatalog,
  applicationCommandPaletteActions,
  desktopShellCommandPaletteActions,
  executeCommandPaletteAction,
} from "../src/command-palette.ts";

const commands: readonly ApplicationCommandDefinition[] = [
  {
    id: "application.route.color",
    title: "Open color workspace",
    category: "Workspace",
    keywords: ["page", "grade"],
    shortcut: "mod+3",
    execute() {},
  },
  {
    id: "application.selection.clear",
    title: "Clear shared selection",
    category: "Selection",
    keywords: ["deselect"],
    execute() {},
  },
];

function actions() {
  return [
    ...applicationCommandPaletteActions(
      commands,
      (commandId) => ({
        enabled: commandId !== "application.selection.clear",
        reason:
          commandId === "application.selection.clear"
            ? "No shared selection is active."
            : null,
      }),
      (commandId) =>
        commandId === "application.route.color" ? "mod+shift+c" : null,
    ),
    ...desktopShellCommandPaletteActions({
      active: true,
      busy: false,
      undoDepth: 0,
      redoDepth: 2,
      recentPaths: [
        "/projects/alpha.superi",
        "/projects/archive/beta.superi",
      ],
    }),
  ];
}

test("catalog rejects duplicate identities and deeply freezes discoverable actions", () => {
  const catalog = new CommandPaletteCatalog(actions());
  assert.equal(
    catalog.action("application.route.color").shortcut,
    "mod+shift+c",
  );
  assert.ok(Object.isFrozen(catalog.actions));
  assert.ok(Object.isFrozen(catalog.actions[0]));
  assert.ok(Object.isFrozen(catalog.actions[0].keywords));
  assert.ok(Object.isFrozen(catalog.actions[0].availability));
  assert.ok(Object.isFrozen(catalog.actions[0].invocation));
  assert.throws(
    () =>
      new CommandPaletteCatalog([
        catalog.actions[0],
        catalog.actions[0],
      ]),
    /duplicate command palette action.*application\.route\.color/i,
  );
});

test("search is token-complete, deterministic, and biased toward exact action meaning", () => {
  const catalog = new CommandPaletteCatalog(actions());
  assert.equal(catalog.search("color work")[0]?.id, "application.route.color");
  assert.equal(catalog.search("grade page")[0]?.id, "application.route.color");
  assert.equal(catalog.search("save as")[0]?.id, "desktop.file.save_as");
  assert.equal(
    catalog.search("archive beta")[0]?.id,
    "desktop.file.open_recent:%2Fprojects%2Farchive%2Fbeta.superi",
  );
  assert.deepEqual(catalog.search("missing action"), []);
  assert.deepEqual(
    catalog.search("save").map((action) => action.id),
    catalog.search("save").map((action) => action.id),
  );
});

test("desktop actions expose current availability and stable recent identity", () => {
  const unavailable = desktopShellCommandPaletteActions({
    active: false,
    busy: true,
    undoDepth: 0,
    redoDepth: 0,
    recentPaths: ["/projects/alpha.superi"],
  });
  const save = unavailable.find((action) => action.id === "desktop.file.save");
  const recent = unavailable.find((action) =>
    action.id.startsWith("desktop.file.open_recent:"),
  );
  const quit = unavailable.find(
    (action) => action.id === "desktop.application.quit",
  );
  assert.deepEqual(save?.availability, {
    enabled: false,
    reason: "Wait for the current operation to finish.",
  });
  assert.equal(
    recent?.id,
    "desktop.file.open_recent:%2Fprojects%2Falpha.superi",
  );
  assert.equal(recent?.availability.enabled, false);
  assert.equal(quit?.availability.enabled, true);
});

test("execution delegates only typed application commands or desktop intents", async () => {
  const catalog = new CommandPaletteCatalog(actions());
  const calls: unknown[] = [];
  const host = {
    async executeApplicationCommand(commandId: string) {
      calls.push({ commandId });
      return { status: "completed" as const };
    },
    async executeDesktopShellIntent(intent: unknown) {
      calls.push({ intent });
    },
  };

  assert.deepEqual(
    await executeCommandPaletteAction(
      catalog.action("application.route.color"),
      host,
    ),
    { status: "completed" },
  );
  assert.deepEqual(
    await executeCommandPaletteAction(
      catalog.action("desktop.file.save_as"),
      host,
    ),
    { status: "completed" },
  );
  assert.deepEqual(calls, [
    { commandId: "application.route.color" },
    { intent: { kind: "save_project_as" } },
  ]);

  assert.deepEqual(
    await executeCommandPaletteAction(
      catalog.action("application.selection.clear"),
      host,
    ),
    { status: "disabled", message: "No shared selection is active." },
  );
  assert.equal(calls.length, 2);
});

test("execution retains actionable failures from either owner", async () => {
  const catalog = new CommandPaletteCatalog(actions());
  const result = await executeCommandPaletteAction(
    catalog.action("desktop.file.save"),
    {
      async executeApplicationCommand() {
        return { status: "completed" };
      },
      async executeDesktopShellIntent() {
        throw new Error("The project could not be saved. Retry after storage recovers.");
      },
    },
  );
  assert.deepEqual(result, {
    status: "failed",
    message: "The project could not be saved. Retry after storage recovers.",
  });
});
