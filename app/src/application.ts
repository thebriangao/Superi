import type {
  PublicResourceReference,
  SuperiApiBindings,
  SuperiApiResource,
} from "./api.ts";

export type ApplicationPanelRegion = "primary" | "secondary" | "utility";

export type ApplicationSelectionReference = Omit<
  PublicResourceReference,
  "resource"
> & {
  readonly resource: SuperiApiResource;
};

export interface ApplicationPanelDefinition<Renderer = unknown> {
  readonly id: string;
  readonly title: string;
  readonly region: ApplicationPanelRegion;
  readonly renderer: Renderer;
}

export interface ApplicationRouteDefinition {
  readonly id: string;
  readonly title: string;
  readonly panelIds: readonly string[];
  readonly defaultPanelId?: string;
}

export interface ApplicationSelection {
  readonly items: readonly ApplicationSelectionReference[];
  readonly anchor: ApplicationSelectionReference | null;
}

export interface ApplicationState {
  readonly revision: number;
  readonly activeRouteId: string;
  readonly hiddenPanelIds: readonly string[];
  readonly visiblePanelIds: readonly string[];
  readonly focusedPanelId: string | null;
  readonly selection: ApplicationSelection;
}

export type ApplicationAction =
  | { readonly type: "navigate"; readonly routeId: string }
  | { readonly type: "toggle_panel"; readonly panelId: string }
  | { readonly type: "focus_panel"; readonly panelId: string }
  | {
      readonly type: "replace_selection";
      readonly items: readonly ApplicationSelectionReference[];
      readonly anchor?: ApplicationSelectionReference | null;
    }
  | {
      readonly type: "extend_selection";
      readonly item: ApplicationSelectionReference;
    }
  | {
      readonly type: "remove_selection";
      readonly resource: SuperiApiResource;
      readonly identity: string;
    }
  | { readonly type: "clear_selection" };

export interface ApplicationCommandContext {
  readonly state: ApplicationState;
  readonly api: SuperiApiBindings | null;
  readonly dispatch: (action: ApplicationAction) => void;
}

export interface ApplicationCommandDefinition {
  readonly id: string;
  readonly title: string;
  readonly shortcut?: string;
  readonly isEnabled?: (context: ApplicationCommandContext) => boolean;
  readonly execute: (
    context: ApplicationCommandContext,
  ) => void | Promise<unknown>;
}

export interface ApplicationRegistryDefinitions<Renderer = unknown> {
  readonly defaultRouteId: string;
  readonly panels: readonly ApplicationPanelDefinition<Renderer>[];
  readonly routes: readonly ApplicationRouteDefinition[];
  readonly commands: readonly ApplicationCommandDefinition[];
}

export class ApplicationRegistry<Renderer = unknown> {
  readonly defaultRouteId: string;
  readonly panelDefinitions: readonly ApplicationPanelDefinition<Renderer>[];
  readonly routeDefinitions: readonly ApplicationRouteDefinition[];
  readonly commandDefinitions: readonly ApplicationCommandDefinition[];

  private readonly panelsById = new Map<
    string,
    ApplicationPanelDefinition<Renderer>
  >();
  private readonly routesById = new Map<string, ApplicationRouteDefinition>();
  private readonly commandsById = new Map<
    string,
    ApplicationCommandDefinition
  >();
  private readonly commandsByShortcut = new Map<
    string,
    ApplicationCommandDefinition
  >();

  constructor(definitions: ApplicationRegistryDefinitions<Renderer>) {
    this.defaultRouteId = requireIdentity(
      definitions.defaultRouteId,
      "default route",
    );
    this.panelDefinitions = Object.freeze(
      definitions.panels.map((panel) => {
        const id = requireIdentity(panel.id, "panel");
        if (this.panelsById.has(id)) {
          throw new Error(`duplicate panel identity: ${id}`);
        }
        const definition = Object.freeze({
          ...panel,
          id,
          title: requireLabel(panel.title, `panel ${id}`),
        });
        this.panelsById.set(id, definition);
        return definition;
      }),
    );
    this.routeDefinitions = Object.freeze(
      definitions.routes.map((route) => {
        const id = requireIdentity(route.id, "route");
        if (this.routesById.has(id)) {
          throw new Error(`duplicate route identity: ${id}`);
        }
        const panelIds = route.panelIds.map((panelId) =>
          requireIdentity(panelId, `route ${id} panel`),
        );
        if (new Set(panelIds).size !== panelIds.length) {
          throw new Error(`route ${id} contains a duplicate panel identity`);
        }
        for (const panelId of panelIds) {
          if (!this.panelsById.has(panelId)) {
            throw new Error(`route ${id} references unknown panel: ${panelId}`);
          }
        }
        const defaultPanelId = route.defaultPanelId ?? panelIds[0];
        if (
          defaultPanelId !== undefined &&
          !panelIds.includes(defaultPanelId)
        ) {
          throw new Error(
            `route ${id} default panel is not registered on the route: ${defaultPanelId}`,
          );
        }
        const definition = Object.freeze({
          ...route,
          id,
          title: requireLabel(route.title, `route ${id}`),
          panelIds: Object.freeze(panelIds),
          defaultPanelId,
        });
        this.routesById.set(id, definition);
        return definition;
      }),
    );
    if (!this.routesById.has(this.defaultRouteId)) {
      throw new Error(`unknown default route: ${this.defaultRouteId}`);
    }
    this.commandDefinitions = Object.freeze(
      definitions.commands.map((command) => {
        const id = requireIdentity(command.id, "command");
        if (this.commandsById.has(id)) {
          throw new Error(`duplicate command identity: ${id}`);
        }
        const shortcut =
          command.shortcut === undefined
            ? undefined
            : normalizeShortcut(command.shortcut);
        if (
          shortcut !== undefined &&
          this.commandsByShortcut.has(shortcut)
        ) {
          throw new Error(`duplicate shortcut: ${shortcut}`);
        }
        const definition = Object.freeze({
          ...command,
          id,
          title: requireLabel(command.title, `command ${id}`),
          shortcut,
        });
        this.commandsById.set(id, definition);
        if (shortcut !== undefined) {
          this.commandsByShortcut.set(shortcut, definition);
        }
        return definition;
      }),
    );
    Object.freeze(this);
  }

  public panel(id: string): ApplicationPanelDefinition<Renderer> {
    const panel = this.panelsById.get(id);
    if (panel === undefined) {
      throw new Error(`unknown panel: ${id}`);
    }
    return panel;
  }

  public route(id: string): ApplicationRouteDefinition {
    const route = this.routesById.get(id);
    if (route === undefined) {
      throw new Error(`unknown route: ${id}`);
    }
    return route;
  }

  public command(id: string): ApplicationCommandDefinition {
    const command = this.commandsById.get(id);
    if (command === undefined) {
      throw new Error(`unknown command: ${id}`);
    }
    return command;
  }

  public commandForShortcut(
    shortcut: string,
  ): ApplicationCommandDefinition | null {
    return this.commandsByShortcut.get(normalizeShortcut(shortcut)) ?? null;
  }
}

export function createApplicationState<Renderer>(
  registry: ApplicationRegistry<Renderer>,
): ApplicationState {
  const route = registry.route(registry.defaultRouteId);
  return freezeState({
    revision: 0,
    activeRouteId: route.id,
    hiddenPanelIds: [],
    visiblePanelIds: route.panelIds,
    focusedPanelId: preferredPanel(route, route.panelIds),
    selection: emptySelection(),
  });
}

export function reduceApplicationState<Renderer>(
  registry: ApplicationRegistry<Renderer>,
  state: ApplicationState,
  action: ApplicationAction,
): ApplicationState {
  switch (action.type) {
    case "navigate": {
      const route = registry.route(action.routeId);
      if (route.id === state.activeRouteId) {
        return state;
      }
      const visiblePanelIds = visiblePanels(route, state.hiddenPanelIds);
      return nextState(state, {
        activeRouteId: route.id,
        visiblePanelIds,
        focusedPanelId: preferredPanel(route, visiblePanelIds),
      });
    }
    case "toggle_panel": {
      const route = registry.route(state.activeRouteId);
      registry.panel(action.panelId);
      if (!route.panelIds.includes(action.panelId)) {
        throw new Error(
          `panel ${action.panelId} is not registered on route ${route.id}`,
        );
      }
      const hidden = new Set(state.hiddenPanelIds);
      if (hidden.has(action.panelId)) {
        hidden.delete(action.panelId);
      } else {
        hidden.add(action.panelId);
      }
      const hiddenPanelIds = registry.panelDefinitions
        .map((panel) => panel.id)
        .filter((panelId) => hidden.has(panelId));
      const visiblePanelIds = visiblePanels(route, hiddenPanelIds);
      const focusedPanelId =
        state.focusedPanelId !== null &&
        visiblePanelIds.includes(state.focusedPanelId)
          ? state.focusedPanelId
          : preferredPanel(route, visiblePanelIds);
      return nextState(state, {
        hiddenPanelIds,
        visiblePanelIds,
        focusedPanelId,
      });
    }
    case "focus_panel": {
      registry.panel(action.panelId);
      if (!state.visiblePanelIds.includes(action.panelId)) {
        throw new Error(`panel is not visible: ${action.panelId}`);
      }
      if (state.focusedPanelId === action.panelId) {
        return state;
      }
      return nextState(state, { focusedPanelId: action.panelId });
    }
    case "replace_selection": {
      const selection = selectionFrom(action.items, action.anchor);
      if (sameSelection(state.selection, selection)) {
        return state;
      }
      return nextState(state, { selection });
    }
    case "extend_selection": {
      const item = freezeSelectionReference(action.item);
      const key = selectionKey(item);
      const items = [...state.selection.items];
      const existing = items.findIndex(
        (candidate) => selectionKey(candidate) === key,
      );
      if (existing === -1) {
        items.push(item);
      } else {
        items[existing] = item;
      }
      return nextState(state, {
        selection: selectionFrom(items, item),
      });
    }
    case "remove_selection": {
      const identity = requireIdentity(action.identity, "selection");
      const items = state.selection.items.filter(
        (item) =>
          item.resource !== action.resource || item.identity !== identity,
      );
      if (items.length === state.selection.items.length) {
        return state;
      }
      const anchor =
        state.selection.anchor !== null &&
        items.some(
          (item) =>
            selectionKey(item) === selectionKey(state.selection.anchor!),
        )
          ? state.selection.anchor
          : items.at(-1) ?? null;
      return nextState(state, { selection: selectionFrom(items, anchor) });
    }
    case "clear_selection":
      return state.selection.items.length === 0
        ? state
        : nextState(state, { selection: emptySelection() });
  }
}

export async function executeApplicationCommand<Renderer>(options: {
  readonly registry: ApplicationRegistry<Renderer>;
  readonly state: () => ApplicationState;
  readonly api: SuperiApiBindings | null;
  readonly dispatch: (action: ApplicationAction) => void;
  readonly commandId: string;
}): Promise<{ readonly status: "completed" | "disabled" }> {
  const command = options.registry.command(options.commandId);
  const context: ApplicationCommandContext = {
    state: options.state(),
    api: options.api,
    dispatch: options.dispatch,
  };
  if (command.isEnabled?.(context) === false) {
    return { status: "disabled" };
  }
  await command.execute(context);
  return { status: "completed" };
}

export function normalizeShortcut(shortcut: string): string {
  const aliases: Readonly<Record<string, string>> = {
    command: "mod",
    cmd: "mod",
    meta: "mod",
    control: "ctrl",
    option: "alt",
  };
  const parts = shortcut
    .split("+")
    .map((part) => aliases[part.trim().toLowerCase()] ?? part.trim().toLowerCase())
    .filter(Boolean);
  if (parts.length === 0 || new Set(parts).size !== parts.length) {
    throw new Error(`invalid shortcut: ${shortcut}`);
  }
  const modifiers = ["mod", "ctrl", "alt", "shift"];
  const keys = parts.filter((part) => !modifiers.includes(part));
  if (keys.length !== 1) {
    throw new Error(`shortcut must contain exactly one key: ${shortcut}`);
  }
  return [...modifiers.filter((modifier) => parts.includes(modifier)), keys[0]].join(
    "+",
  );
}

export function isEditableCommandTarget(target: unknown): boolean {
  if (typeof target !== "object" || target === null) {
    return false;
  }
  const candidate = target as {
    readonly tagName?: unknown;
    readonly isContentEditable?: unknown;
    readonly closest?: unknown;
  };
  if (candidate.isContentEditable === true) {
    return true;
  }
  const tagName =
    typeof candidate.tagName === "string"
      ? candidate.tagName.toLowerCase()
      : "";
  if (["input", "textarea", "select"].includes(tagName)) {
    return true;
  }
  return (
    typeof candidate.closest === "function" &&
    candidate.closest("input, textarea, select, [contenteditable='true']") !==
      null
  );
}

function visiblePanels(
  route: ApplicationRouteDefinition,
  hiddenPanelIds: readonly string[],
): readonly string[] {
  const hidden = new Set(hiddenPanelIds);
  return route.panelIds.filter((panelId) => !hidden.has(panelId));
}

function preferredPanel(
  route: ApplicationRouteDefinition,
  visiblePanelIds: readonly string[],
): string | null {
  if (
    route.defaultPanelId !== undefined &&
    visiblePanelIds.includes(route.defaultPanelId)
  ) {
    return route.defaultPanelId;
  }
  return visiblePanelIds[0] ?? null;
}

function nextState(
  state: ApplicationState,
  replacement: Partial<Omit<ApplicationState, "revision">>,
): ApplicationState {
  return freezeState({
    ...state,
    ...replacement,
    revision: state.revision + 1,
  });
}

function freezeState(state: ApplicationState): ApplicationState {
  return Object.freeze({
    ...state,
    hiddenPanelIds: Object.freeze([...state.hiddenPanelIds]),
    visiblePanelIds: Object.freeze([...state.visiblePanelIds]),
    selection: state.selection,
  });
}

function selectionFrom(
  references: readonly ApplicationSelectionReference[],
  requestedAnchor?: ApplicationSelectionReference | null,
): ApplicationSelection {
  const items: ApplicationSelectionReference[] = [];
  const positions = new Map<string, number>();
  for (const reference of references) {
    const item = freezeSelectionReference(reference);
    const key = selectionKey(item);
    const position = positions.get(key);
    if (position === undefined) {
      positions.set(key, items.length);
      items.push(item);
    } else {
      items[position] = item;
    }
  }
  const requested =
    requestedAnchor === undefined
      ? items.at(-1) ?? null
      : requestedAnchor === null
        ? null
        : freezeSelectionReference(requestedAnchor);
  const anchor =
    requested === null
      ? null
      : items.find((item) => selectionKey(item) === selectionKey(requested)) ??
        null;
  if (requested !== null && anchor === null) {
    throw new Error("selection anchor must identify one selected resource");
  }
  return Object.freeze({
    items: Object.freeze(items),
    anchor,
  });
}

function emptySelection(): ApplicationSelection {
  return Object.freeze({ items: Object.freeze([]), anchor: null });
}

function freezeSelectionReference(
  reference: ApplicationSelectionReference,
): ApplicationSelectionReference {
  const resource = requireIdentity(reference.resource, "selection resource");
  const identity = requireIdentity(reference.identity, "selection identity");
  if (!Number.isSafeInteger(reference.revision) || reference.revision < 0) {
    throw new Error("selection revision must be a nonnegative safe integer");
  }
  const version = reference.schema_version;
  if (!/^\d+\.\d+\.\d+$/.test(version)) {
    throw new Error("selection schema version must be semantic major.minor.patch text");
  }
  return Object.freeze({
    resource: resource as SuperiApiResource,
    schema_version: version,
    identity,
    revision: reference.revision,
  });
}

function selectionKey(reference: ApplicationSelectionReference): string {
  return `${reference.resource}\u0000${reference.identity}`;
}

function sameSelection(
  left: ApplicationSelection,
  right: ApplicationSelection,
): boolean {
  return JSON.stringify(left) === JSON.stringify(right);
}

function requireIdentity(value: string, kind: string): string {
  const identity = value.trim();
  if (identity.length === 0) {
    throw new Error(`${kind} identity must not be empty`);
  }
  return identity;
}

function requireLabel(value: string, kind: string): string {
  const label = value.trim();
  if (label.length === 0) {
    throw new Error(`${kind} title must not be empty`);
  }
  return label;
}
