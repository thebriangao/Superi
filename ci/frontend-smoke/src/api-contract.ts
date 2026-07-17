import {
  SuperiClient,
  type EditorAiState,
  type ExecuteProjectCommand,
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
  api_schema_versions: ["1.0.0", "1.1.0", "1.2.0", "1.3.0"],
  primitive_schema_revisions: [1],
  project: null,
} satisfies NegotiateApiVersion;

export type InspectProjectResult =
  SuperiMethodMap["superi.project.command.execute"]["response"];
export type ProjectStateEvent =
  SuperiEventMap["superi.project.state.changed"];
export type EditorStateResource = SuperiResourceMap["superi.editor.state"];
export type VersionNegotiationResult =
  SuperiMethodMap["superi.api.version.negotiate"]["response"];

export function createSuperiClient(transport: SuperiTransport): SuperiClient {
  return new SuperiClient(transport);
}
