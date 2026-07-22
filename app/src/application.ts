import type {
  PublicResourceReference,
  SuperiApiBindings,
  SuperiApiResource,
} from "./api.ts";

export type ApplicationPanelRegion = "primary" | "secondary" | "utility";

export const APPLICATION_PANEL_DOCKS = Object.freeze([
  "left",
  "center",
  "right",
  "bottom",
] as const);

export type ApplicationPanelDockId = (typeof APPLICATION_PANEL_DOCKS)[number];

export const APPLICATION_PANEL_DOCK_SIZE_BOUNDS: Readonly<
  Record<
    ApplicationPanelDockId,
    Readonly<{ minimum: number; maximum: number; defaultValue: number }>
  >
> = Object.freeze({
  left: Object.freeze({ minimum: 1_500, maximum: 4_500, defaultValue: 2_400 }),
  center: Object.freeze({
    minimum: 10_000,
    maximum: 10_000,
    defaultValue: 10_000,
  }),
  right: Object.freeze({ minimum: 1_500, maximum: 4_500, defaultValue: 2_800 }),
  bottom: Object.freeze({ minimum: 1_800, maximum: 6_500, defaultValue: 3_000 }),
});

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

export interface ApplicationPanelDockLayout {
  readonly dockId: ApplicationPanelDockId;
  readonly panelIds: readonly string[];
  readonly activePanelId: string | null;
  readonly sizeBasisPoints: number;
}

export interface ApplicationRoutePanelLayout {
  readonly routeId: string;
  readonly docks: readonly ApplicationPanelDockLayout[];
}

export interface ApplicationPanelDockPresentation {
  readonly dock_id: ApplicationPanelDockId;
  readonly panel_ids: readonly string[];
  readonly active_panel_id: string | null;
  readonly size_basis_points: number;
}

export interface ApplicationRoutePanelLayoutPresentation {
  readonly route_id: string;
  readonly docks: readonly ApplicationPanelDockPresentation[];
}

export interface ApplicationState {
  readonly revision: number;
  readonly commandPaletteOpen: boolean;
  readonly activeRouteId: string;
  readonly hiddenPanelIds: readonly string[];
  readonly visiblePanelIds: readonly string[];
  readonly focusedPanelId: string | null;
  readonly panelLayouts: readonly ApplicationRoutePanelLayout[];
  readonly workspaceLayoutResetUndo: ApplicationWorkspacePresentation | null;
  readonly selection: ApplicationSelection;
}

export interface ApplicationWorkspacePresentation {
  readonly active_route_id: string;
  readonly hidden_panel_ids: readonly string[];
  readonly focused_panel_id: string | null;
  readonly panel_layouts: readonly ApplicationRoutePanelLayoutPresentation[];
}

export interface ApplicationWorkspaceLayoutStatus {
  readonly condition: "default" | "custom";
  readonly canReset: boolean;
  readonly canUndoReset: boolean;
}

export type ApplicationAction =
  | { readonly type: "open_command_palette" }
  | { readonly type: "close_command_palette" }
  | { readonly type: "navigate"; readonly routeId: string }
  | {
      readonly type: "restore_workspace";
      readonly workspace: ApplicationWorkspacePresentation;
    }
  | {
      readonly type: "restore_workspace_presentation";
      readonly workspace: ApplicationWorkspacePresentation;
    }
  | { readonly type: "reset_workspace_layouts" }
  | { readonly type: "undo_workspace_layout_reset" }
  | { readonly type: "toggle_panel"; readonly panelId: string }
  | { readonly type: "focus_panel"; readonly panelId: string }
  | { readonly type: "activate_panel"; readonly panelId: string }
  | {
      readonly type: "dock_panel";
      readonly panelId: string;
      readonly dockId: ApplicationPanelDockId;
      readonly index?: number;
    }
  | {
      readonly type: "resize_panel_dock";
      readonly dockId: ApplicationPanelDockId;
      readonly sizeBasisPoints: number;
    }
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
  readonly registry: ApplicationRegistry;
  readonly state: ApplicationState;
  readonly api: SuperiApiBindings | null;
  readonly dispatch: (action: ApplicationAction) => void;
}

export interface ApplicationCommandDefinition {
  readonly id: string;
  readonly title: string;
  readonly category: string;
  readonly keywords: readonly string[];
  readonly shortcut?: string;
  readonly allowInEditableContext?: boolean;
  readonly isEnabled?: (context: ApplicationCommandContext) => boolean;
  readonly disabledReason?: string;
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
          category: requireLabel(command.category, `command ${id} category`),
          keywords: Object.freeze(
            command.keywords.map((keyword) =>
              requireLabel(keyword, `command ${id} keyword`),
            ),
          ),
          shortcut,
          allowInEditableContext: command.allowInEditableContext ?? false,
          disabledReason:
            command.disabledReason === undefined
              ? undefined
              : requireLabel(
                  command.disabledReason,
                  `command ${id} disabled reason`,
                ),
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
    commandPaletteOpen: false,
    activeRouteId: route.id,
    hiddenPanelIds: [],
    visiblePanelIds: route.panelIds,
    focusedPanelId: preferredPanel(route, route.panelIds),
    panelLayouts: createPanelLayouts(registry, [], []),
    workspaceLayoutResetUndo: null,
    selection: emptySelection(),
  });
}

export function reduceApplicationState<Renderer>(
  registry: ApplicationRegistry<Renderer>,
  state: ApplicationState,
  action: ApplicationAction,
): ApplicationState {
  switch (action.type) {
    case "open_command_palette":
      return state.commandPaletteOpen
        ? state
        : nextState(state, { commandPaletteOpen: true });
    case "close_command_palette":
      return state.commandPaletteOpen
        ? nextState(state, { commandPaletteOpen: false })
        : state;
    case "restore_workspace":
      return restoreApplicationWorkspace(registry, state, action.workspace);
    case "restore_workspace_presentation":
      return restoreApplicationWorkspace(registry, state, {
        ...action.workspace,
        active_route_id: state.activeRouteId,
      });
    case "reset_workspace_layouts": {
      if (
        applicationWorkspaceLayoutStatus(registry, state).condition === "default"
      ) {
        return state;
      }
      const route = registry.route(state.activeRouteId);
      return nextState(state, {
        hiddenPanelIds: [],
        visiblePanelIds: route.panelIds,
        focusedPanelId: preferredPanel(route, route.panelIds),
        panelLayouts: createPanelLayouts(registry, [], []),
        workspaceLayoutResetUndo: applicationWorkspacePresentation(state),
      });
    }
    case "undo_workspace_layout_reset":
      return state.workspaceLayoutResetUndo === null
        ? state
        : restoreApplicationWorkspace(
            registry,
            state,
            state.workspaceLayoutResetUndo,
          );
    case "navigate": {
      const route = registry.route(action.routeId);
      if (route.id === state.activeRouteId) {
        return clearWorkspaceLayoutResetUndo(state);
      }
      const visiblePanelIds = visiblePanels(route, state.hiddenPanelIds);
      const layout = applicationRoutePanelLayout(state, route.id);
      return nextState(state, {
        activeRouteId: route.id,
        visiblePanelIds,
        focusedPanelId: preferredPanelFromLayout(
          route,
          layout,
          visiblePanelIds,
        ),
        workspaceLayoutResetUndo: null,
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
      let panelLayouts = reconcileLayoutActivity(
        state.panelLayouts,
        hiddenPanelIds,
      );
      let focusedPanelId: string | null;
      if (!hidden.has(action.panelId)) {
        panelLayouts = replaceRouteLayout(
          panelLayouts,
          activatePanel(
            applicationRoutePanelLayoutFrom(panelLayouts, route.id),
            action.panelId,
          ),
        );
        focusedPanelId = action.panelId;
      } else {
        const layout = applicationRoutePanelLayoutFrom(panelLayouts, route.id);
        focusedPanelId =
          state.focusedPanelId !== null &&
          visiblePanelIds.includes(state.focusedPanelId)
            ? state.focusedPanelId
            : preferredPanelFromLayout(route, layout, visiblePanelIds);
      }
      return nextState(state, {
        hiddenPanelIds,
        visiblePanelIds,
        focusedPanelId,
        panelLayouts,
        workspaceLayoutResetUndo: null,
      });
    }
    case "focus_panel":
    case "activate_panel": {
      const route = registry.route(state.activeRouteId);
      registry.panel(action.panelId);
      if (!state.visiblePanelIds.includes(action.panelId)) {
        throw new Error(`panel is not visible: ${action.panelId}`);
      }
      if (!route.panelIds.includes(action.panelId)) {
        throw new Error(
          `panel ${action.panelId} is not registered on route ${route.id}`,
        );
      }
      const currentLayout = applicationRoutePanelLayout(state, route.id);
      const layout = activatePanel(currentLayout, action.panelId);
      if (
        state.focusedPanelId === action.panelId &&
        layout === currentLayout
      ) {
        return clearWorkspaceLayoutResetUndo(state);
      }
      return nextState(state, {
        focusedPanelId: action.panelId,
        panelLayouts: replaceRouteLayout(state.panelLayouts, layout),
        workspaceLayoutResetUndo: null,
      });
    }
    case "dock_panel": {
      const route = registry.route(state.activeRouteId);
      registry.panel(action.panelId);
      requireDockId(action.dockId);
      if (!route.panelIds.includes(action.panelId)) {
        throw new Error(
          `panel ${action.panelId} is not registered on route ${route.id}`,
        );
      }
      if (!state.visiblePanelIds.includes(action.panelId)) {
        throw new Error(`panel is not visible: ${action.panelId}`);
      }
      if (
        action.index !== undefined &&
        (!Number.isSafeInteger(action.index) || action.index < 0)
      ) {
        throw new Error("panel dock index must be a nonnegative safe integer");
      }
      const layout = dockPanel(
        applicationRoutePanelLayout(state, route.id),
        action.panelId,
        action.dockId,
        action.index,
        state.hiddenPanelIds,
      );
      return nextState(state, {
        focusedPanelId: action.panelId,
        panelLayouts: replaceRouteLayout(state.panelLayouts, layout),
        workspaceLayoutResetUndo: null,
      });
    }
    case "resize_panel_dock": {
      requireDockId(action.dockId);
      if (!Number.isSafeInteger(action.sizeBasisPoints)) {
        throw new Error("panel dock size must be a safe integer");
      }
      const route = registry.route(state.activeRouteId);
      const currentLayout = applicationRoutePanelLayout(state, route.id);
      const sizeBasisPoints = clampDockSize(
        action.dockId,
        action.sizeBasisPoints,
      );
      const currentDock = currentLayout.docks.find(
        (dock) => dock.dockId === action.dockId,
      )!;
      if (currentDock.sizeBasisPoints === sizeBasisPoints) {
        return clearWorkspaceLayoutResetUndo(state);
      }
      const layout: ApplicationRoutePanelLayout = {
        ...currentLayout,
        docks: currentLayout.docks.map((dock) =>
          dock.dockId === action.dockId
            ? { ...dock, sizeBasisPoints }
            : dock,
        ),
      };
      return nextState(state, {
        panelLayouts: replaceRouteLayout(state.panelLayouts, layout),
        workspaceLayoutResetUndo: null,
      });
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

export function restoreApplicationWorkspace<Renderer>(
  registry: ApplicationRegistry<Renderer>,
  state: ApplicationState,
  workspace: ApplicationWorkspacePresentation,
): ApplicationState {
  const route =
    registry.routeDefinitions.find(
      (candidate) => candidate.id === workspace.active_route_id,
    ) ?? registry.route(registry.defaultRouteId);
  const requestedHidden = new Set(workspace.hidden_panel_ids);
  const hiddenPanelIds = registry.panelDefinitions
    .map((panel) => panel.id)
    .filter((panelId) => requestedHidden.has(panelId));
  const visiblePanelIds = visiblePanels(route, hiddenPanelIds);
  let panelLayouts = createPanelLayouts(
    registry,
    workspace.panel_layouts ?? [],
    hiddenPanelIds,
  );
  const requestedFocus =
    workspace.focused_panel_id !== null &&
    visiblePanelIds.includes(workspace.focused_panel_id)
      ? workspace.focused_panel_id
      : null;
  if (requestedFocus !== null) {
    panelLayouts = replaceRouteLayout(
      panelLayouts,
      activatePanel(
        applicationRoutePanelLayoutFrom(panelLayouts, route.id),
        requestedFocus,
      ),
    );
  }
  const focusedPanelId =
    requestedFocus ??
    preferredPanelFromLayout(
      route,
      applicationRoutePanelLayoutFrom(panelLayouts, route.id),
      visiblePanelIds,
    );
  return nextState(state, {
    activeRouteId: route.id,
    hiddenPanelIds,
    visiblePanelIds,
    focusedPanelId,
    panelLayouts,
    workspaceLayoutResetUndo: null,
  });
}

export function applicationRoutePanelLayout(
  state: ApplicationState,
  routeId: string = state.activeRouteId,
): ApplicationRoutePanelLayout {
  return applicationRoutePanelLayoutFrom(state.panelLayouts, routeId);
}

export function applicationWorkspacePresentation(
  state: ApplicationState,
): ApplicationWorkspacePresentation {
  return freezeWorkspacePresentation({
    active_route_id: state.activeRouteId,
    hidden_panel_ids: state.hiddenPanelIds,
    focused_panel_id: state.focusedPanelId,
    panel_layouts: state.panelLayouts.map((layout) => ({
      route_id: layout.routeId,
      docks: layout.docks.map((dock) => ({
        dock_id: dock.dockId,
        panel_ids: dock.panelIds,
        active_panel_id: dock.activePanelId,
        size_basis_points: dock.sizeBasisPoints,
      })),
    })),
  });
}

export function applicationWorkspaceLayoutStatus<Renderer>(
  registry: ApplicationRegistry<Renderer>,
  state: ApplicationState,
): ApplicationWorkspaceLayoutStatus {
  const defaults = createPanelLayouts(registry, [], []);
  const custom =
    state.hiddenPanelIds.length > 0 ||
    defaults.some((defaultLayout) => {
      const current = state.panelLayouts.find(
        (layout) => layout.routeId === defaultLayout.routeId,
      );
      return (
        current === undefined ||
        current.docks.length !== defaultLayout.docks.length ||
        defaultLayout.docks.some((defaultDock, index) => {
          const currentDock = current.docks[index];
          return (
            currentDock === undefined ||
            currentDock.dockId !== defaultDock.dockId ||
            currentDock.sizeBasisPoints !== defaultDock.sizeBasisPoints ||
            !sameStringArray(currentDock.panelIds, defaultDock.panelIds)
          );
        })
      );
    });
  return Object.freeze({
    condition: custom ? "custom" : "default",
    canReset: custom,
    canUndoReset: state.workspaceLayoutResetUndo !== null,
  });
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
    registry: options.registry,
    state: options.state(),
    api: options.api,
    dispatch: options.dispatch,
  };
  if (!commandAvailability(command, context).enabled) {
    return { status: "disabled" };
  }
  await command.execute(context);
  return { status: "completed" };
}

export interface ApplicationCommandAvailability {
  readonly enabled: boolean;
  readonly reason: string | null;
}

export function applicationCommandAvailability<Renderer>(options: {
  readonly registry: ApplicationRegistry<Renderer>;
  readonly state: () => ApplicationState;
  readonly api: SuperiApiBindings | null;
  readonly dispatch: (action: ApplicationAction) => void;
  readonly commandId: string;
}): ApplicationCommandAvailability {
  const command = options.registry.command(options.commandId);
  return commandAvailability(command, {
    registry: options.registry,
    state: options.state(),
    api: options.api,
    dispatch: options.dispatch,
  });
}

function commandAvailability(
  command: ApplicationCommandDefinition,
  context: ApplicationCommandContext,
): ApplicationCommandAvailability {
  if (command.isEnabled?.(context) !== false) {
    return Object.freeze({ enabled: true, reason: null });
  }
  return Object.freeze({
    enabled: false,
    reason:
      command.disabledReason ??
      `${command.title} is unavailable in the current application state.`,
  });
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

function createPanelLayouts<Renderer>(
  registry: ApplicationRegistry<Renderer>,
  presentations: readonly ApplicationRoutePanelLayoutPresentation[],
  hiddenPanelIds: readonly string[],
): readonly ApplicationRoutePanelLayout[] {
  const requestedByRoute = new Map<
    string,
    ApplicationRoutePanelLayoutPresentation
  >();
  for (const presentation of presentations) {
    if (
      typeof presentation?.route_id === "string" &&
      !requestedByRoute.has(presentation.route_id)
    ) {
      requestedByRoute.set(presentation.route_id, presentation);
    }
  }
  const hidden = new Set(hiddenPanelIds);
  return registry.routeDefinitions.map((route) => {
    const requested = requestedByRoute.get(route.id);
    const requestedDocks = Array.isArray(requested?.docks)
      ? requested.docks
      : [];
    const usedPanels = new Set<string>();
    const panelsByDock = new Map<
      ApplicationPanelDockId,
      string[]
    >(APPLICATION_PANEL_DOCKS.map((dockId) => [dockId, []]));

    for (const dockId of APPLICATION_PANEL_DOCKS) {
      const requestedDock = requestedDocks.find(
        (dock) => dock?.dock_id === dockId,
      );
      const panelIds = Array.isArray(requestedDock?.panel_ids)
        ? requestedDock.panel_ids
        : [];
      for (const panelId of panelIds) {
        if (
          typeof panelId === "string" &&
          route.panelIds.includes(panelId) &&
          !usedPanels.has(panelId)
        ) {
          panelsByDock.get(dockId)!.push(panelId);
          usedPanels.add(panelId);
        }
      }
    }

    for (const panelId of route.panelIds) {
      if (!usedPanels.has(panelId)) {
        panelsByDock
          .get(defaultDockForRegion(registry.panel(panelId).region))!
          .push(panelId);
      }
    }

    return {
      routeId: route.id,
      docks: APPLICATION_PANEL_DOCKS.map((dockId) => {
        const panelIds = panelsByDock.get(dockId)!;
        const requestedDock = requestedDocks.find(
          (dock) => dock?.dock_id === dockId,
        );
        const requestedActive = requestedDock?.active_panel_id;
        return {
          dockId,
          panelIds,
          activePanelId:
            typeof requestedActive === "string" &&
            panelIds.includes(requestedActive) &&
            !hidden.has(requestedActive)
              ? requestedActive
              : panelIds.find((panelId) => !hidden.has(panelId)) ?? null,
          sizeBasisPoints: restoredDockSize(
            dockId,
            requestedDock?.size_basis_points,
          ),
        };
      }),
    };
  });
}

function defaultDockForRegion(
  region: ApplicationPanelRegion,
): ApplicationPanelDockId {
  switch (region) {
    case "primary":
      return "center";
    case "secondary":
      return "right";
    case "utility":
      return "bottom";
  }
}

function restoredDockSize(
  dockId: ApplicationPanelDockId,
  requested: unknown,
): number {
  const bounds = APPLICATION_PANEL_DOCK_SIZE_BOUNDS[dockId];
  return typeof requested === "number" &&
    Number.isSafeInteger(requested) &&
    requested >= bounds.minimum &&
    requested <= bounds.maximum
    ? requested
    : bounds.defaultValue;
}

function clampDockSize(
  dockId: ApplicationPanelDockId,
  requested: number,
): number {
  const bounds = APPLICATION_PANEL_DOCK_SIZE_BOUNDS[dockId];
  return Math.min(bounds.maximum, Math.max(bounds.minimum, requested));
}

function requireDockId(dockId: ApplicationPanelDockId): void {
  if (!(APPLICATION_PANEL_DOCKS as readonly string[]).includes(dockId)) {
    throw new Error(`unknown panel dock: ${dockId}`);
  }
}

function reconcileLayoutActivity(
  layouts: readonly ApplicationRoutePanelLayout[],
  hiddenPanelIds: readonly string[],
): readonly ApplicationRoutePanelLayout[] {
  const hidden = new Set(hiddenPanelIds);
  return layouts.map((layout) => ({
    ...layout,
    docks: layout.docks.map((dock) => ({
      ...dock,
      activePanelId:
        dock.activePanelId !== null &&
        dock.panelIds.includes(dock.activePanelId) &&
        !hidden.has(dock.activePanelId)
          ? dock.activePanelId
          : dock.panelIds.find((panelId) => !hidden.has(panelId)) ?? null,
    })),
  }));
}

function activatePanel(
  layout: ApplicationRoutePanelLayout,
  panelId: string,
): ApplicationRoutePanelLayout {
  const target = layout.docks.find((dock) => dock.panelIds.includes(panelId));
  if (target === undefined) {
    throw new Error(`panel ${panelId} is not present in route layout ${layout.routeId}`);
  }
  if (target.activePanelId === panelId) {
    return layout;
  }
  return {
    ...layout,
    docks: layout.docks.map((dock) =>
      dock.dockId === target.dockId
        ? { ...dock, activePanelId: panelId }
        : dock,
    ),
  };
}

function dockPanel(
  layout: ApplicationRoutePanelLayout,
  panelId: string,
  targetDockId: ApplicationPanelDockId,
  requestedIndex: number | undefined,
  hiddenPanelIds: readonly string[],
): ApplicationRoutePanelLayout {
  const hidden = new Set(hiddenPanelIds);
  const panelsByDock = new Map<ApplicationPanelDockId, string[]>(
    layout.docks.map((dock) => [
      dock.dockId,
      dock.panelIds.filter((candidate) => candidate !== panelId),
    ]),
  );
  const target = panelsByDock.get(targetDockId)!;
  const index = Math.min(requestedIndex ?? target.length, target.length);
  target.splice(index, 0, panelId);
  return {
    ...layout,
    docks: layout.docks.map((dock) => {
      const panelIds = panelsByDock.get(dock.dockId)!;
      return {
        ...dock,
        panelIds,
        activePanelId:
          dock.dockId === targetDockId
            ? panelId
            : dock.activePanelId !== null &&
                panelIds.includes(dock.activePanelId) &&
                !hidden.has(dock.activePanelId)
              ? dock.activePanelId
              : panelIds.find((candidate) => !hidden.has(candidate)) ?? null,
      };
    }),
  };
}

function replaceRouteLayout(
  layouts: readonly ApplicationRoutePanelLayout[],
  replacement: ApplicationRoutePanelLayout,
): readonly ApplicationRoutePanelLayout[] {
  let replaced = false;
  const next = layouts.map((layout) => {
    if (layout.routeId !== replacement.routeId) {
      return layout;
    }
    replaced = true;
    return replacement;
  });
  if (!replaced) {
    throw new Error(`unknown route layout: ${replacement.routeId}`);
  }
  return next;
}

function applicationRoutePanelLayoutFrom(
  layouts: readonly ApplicationRoutePanelLayout[],
  routeId: string,
): ApplicationRoutePanelLayout {
  const layout = layouts.find((candidate) => candidate.routeId === routeId);
  if (layout === undefined) {
    throw new Error(`unknown route layout: ${routeId}`);
  }
  return layout;
}

function visiblePanels(
  route: ApplicationRouteDefinition,
  hiddenPanelIds: readonly string[],
): readonly string[] {
  const hidden = new Set(hiddenPanelIds);
  return route.panelIds.filter((panelId) => !hidden.has(panelId));
}

function preferredPanelFromLayout(
  route: ApplicationRouteDefinition,
  layout: ApplicationRoutePanelLayout,
  visiblePanelIds: readonly string[],
): string | null {
  for (const dockId of ["center", "left", "right", "bottom"] as const) {
    const activePanelId = layout.docks.find(
      (dock) => dock.dockId === dockId,
    )?.activePanelId;
    if (
      activePanelId !== undefined &&
      activePanelId !== null &&
      visiblePanelIds.includes(activePanelId)
    ) {
      return activePanelId;
    }
  }
  return preferredPanel(route, visiblePanelIds);
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

function clearWorkspaceLayoutResetUndo(
  state: ApplicationState,
): ApplicationState {
  return state.workspaceLayoutResetUndo === null
    ? state
    : nextState(state, { workspaceLayoutResetUndo: null });
}

function freezeWorkspacePresentation(
  workspace: ApplicationWorkspacePresentation,
): ApplicationWorkspacePresentation {
  return Object.freeze({
    active_route_id: workspace.active_route_id,
    hidden_panel_ids: Object.freeze([...workspace.hidden_panel_ids]),
    focused_panel_id: workspace.focused_panel_id,
    panel_layouts: Object.freeze(
      workspace.panel_layouts.map((layout) =>
        Object.freeze({
          route_id: layout.route_id,
          docks: Object.freeze(
            layout.docks.map((dock) =>
              Object.freeze({
                dock_id: dock.dock_id,
                panel_ids: Object.freeze([...dock.panel_ids]),
                active_panel_id: dock.active_panel_id,
                size_basis_points: dock.size_basis_points,
              }),
            ),
          ),
        }),
      ),
    ),
  });
}

function sameStringArray(
  left: readonly string[],
  right: readonly string[],
): boolean {
  return (
    left.length === right.length &&
    left.every((value, index) => value === right[index])
  );
}

function freezeState(state: ApplicationState): ApplicationState {
  return Object.freeze({
    ...state,
    hiddenPanelIds: Object.freeze([...state.hiddenPanelIds]),
    visiblePanelIds: Object.freeze([...state.visiblePanelIds]),
    panelLayouts: Object.freeze(
      state.panelLayouts.map((layout) =>
        Object.freeze({
          ...layout,
          docks: Object.freeze(
            layout.docks.map((dock) =>
              Object.freeze({
                ...dock,
                panelIds: Object.freeze([...dock.panelIds]),
              }),
            ),
          ),
        }),
      ),
    ),
    workspaceLayoutResetUndo:
      state.workspaceLayoutResetUndo === null
        ? null
        : freezeWorkspacePresentation(state.workspaceLayoutResetUndo),
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
