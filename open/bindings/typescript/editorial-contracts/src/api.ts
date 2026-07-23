import {
  SuperiClient,
  type SuperiEventMap,
  type SuperiMethodMap,
  type SuperiResourceMap,
  type SuperiTransport,
} from "../../superi-api.ts";

export * from "../../superi-api.ts";

export type SuperiApiMethod = keyof SuperiMethodMap;
export type SuperiApiEvent = keyof SuperiEventMap;
export type SuperiApiResource = keyof SuperiResourceMap;

export type SuperiApiRequest = <Method extends SuperiApiMethod>(
  method: Method,
  request: SuperiMethodMap[Method]["request"],
) => Promise<SuperiMethodMap[Method]["response"]>;

export type SuperiApiSubscribe = <Event extends SuperiApiEvent>(
  event: Event,
  listener: (payload: SuperiEventMap[Event]) => void,
) => () => void;

export interface SuperiApiBindings {
  readonly client: SuperiClient;
  readonly request: SuperiApiRequest;
  readonly automation: SuperiApiRequest;
  readonly subscribe: SuperiApiSubscribe;
}

export function createSuperiApiBindings(
  transport: SuperiTransport,
): SuperiApiBindings {
  const client = new SuperiClient(transport);
  const request: SuperiApiRequest = (method, payload) =>
    client.request(method, payload);
  const subscribe: SuperiApiSubscribe = (event, listener) =>
    client.subscribe(event, listener);

  return Object.freeze({
    client,
    request,
    automation: request,
    subscribe,
  });
}
