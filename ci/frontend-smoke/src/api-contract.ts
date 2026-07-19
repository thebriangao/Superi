import {
  SuperiClient,
  type EditorAiState,
  type ExecuteProjectCommand,
  type GetExtensions,
  type GetProjectCommandLog,
  type NegotiateApiVersion,
  type SuperiEventMap,
  type SuperiMethodMap,
  type SuperiResourceMap,
  type SuperiTransport,
} from "../../../open/bindings/typescript/superi-api";

export const inspectProjectCommand = {
  transaction_id: "frontend-smoke.inspect-project",
  expected_project_revision: 0,
  command: { command: "inspect" },
} satisfies ExecuteProjectCommand;

export const unavailableAiState = {
  runtime_availability: "unavailable",
  graph_resources: [],
  artifact_records: [],
} satisfies EditorAiState;

export const versionNegotiationRequest = {
  api_schema_versions: [
    "1.0.0",
    "1.1.0",
    "1.2.0",
    "1.3.0",
    "1.4.0",
    "1.5.0",
    "1.6.0",
  ],
  primitive_schema_revisions: [1],
  project: null,
} satisfies NegotiateApiVersion;

export const extensionDiscoveryRequest = null satisfies GetExtensions;

export const inspectCommandLog = {
  after_sequence: 0,
  requested_limit: 64,
  detail: "metadata",
} satisfies GetProjectCommandLog;

export type InspectProjectResult =
  SuperiMethodMap["superi.project.command.execute"]["response"];
export type ProjectStateEvent =
  SuperiEventMap["superi.project.state.changed"];
export type ProjectCommandLogResult =
  SuperiMethodMap["superi.project.command_log.get"]["response"];
export type ProjectCommandLogResource =
  SuperiResourceMap["superi.project.command_log"];
export type EditorStateResource = SuperiResourceMap["superi.editor.state"];
export type VersionNegotiationResult =
  SuperiMethodMap["superi.api.version.negotiate"]["response"];
export type ExtensionDiscoveryResult =
  SuperiMethodMap["superi.extensions.get"]["response"];
export type ExtensionChangedEvent = SuperiEventMap["superi.extensions.changed"];
export type ExtensionRegistryResource = SuperiResourceMap["superi.extensions"];

export function createSuperiClient(transport: SuperiTransport): SuperiClient {
  return new SuperiClient(transport);
}
