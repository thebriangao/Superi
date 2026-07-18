import {
  SuperiTransportError,
  type ErrorCategory,
  type PublicApiError,
  type PublicErrorContext,
  type PublicResourceReference,
  type Recoverability,
  type SuperiEventMap,
  type SuperiMethodMap,
  type SuperiTransport,
} from "./api.ts";

const DESKTOP_API_COMMAND = "desktop_api_dispatch";
const DESKTOP_API_EVENT = "superi://api-event";

interface DesktopEventEnvelope {
  readonly generation: number;
  readonly stream_id: string;
  readonly sequence: number;
  readonly event: keyof SuperiEventMap;
  readonly payload: SuperiEventMap[keyof SuperiEventMap];
}

interface ConnectedReply {
  readonly kind: "connected";
  readonly generation: number;
  readonly stream_id: string;
  readonly replay: readonly DesktopEventEnvelope[];
  readonly resync_required: boolean;
}

interface ResponseReply {
  readonly kind: "response";
  readonly generation: number;
  readonly request_id: string;
  readonly response: unknown;
}

type DesktopTransportReply =
  | ConnectedReply
  | ResponseReply
  | {
      readonly kind: "cancelled";
      readonly generation: number;
      readonly request_id: string;
      readonly cancelled: boolean;
    }
  | { readonly kind: "disconnected"; readonly generation: number };

interface PendingRequest {
  readonly generation: number;
  readonly reject: (reason: unknown) => void;
}

export interface DesktopTransportHost {
  invoke<T>(command: string, args: Record<string, unknown>): Promise<T>;
  listen<T>(
    event: string,
    listener: (event: { payload: T }) => void,
  ): Promise<() => void>;
}

export interface DesktopTransportFailure {
  readonly condition: Recoverability;
  readonly category: ErrorCategory;
  readonly code: string;
  readonly title: string;
  readonly action: string;
  readonly contexts: readonly PublicErrorContext[];
  readonly lastValidResource: PublicResourceReference | null;
}

const TAURI_HOST: DesktopTransportHost = {
  async invoke<T>(command: string, args: Record<string, unknown>): Promise<T> {
    const { invoke } = await import("@tauri-apps/api/core");
    return invoke<T>(command, args);
  },
  async listen<T>(
    event: string,
    listener: (event: { payload: T }) => void,
  ): Promise<() => void> {
    const { listen } = await import("@tauri-apps/api/event");
    return listen<T>(event, listener);
  },
};

export class DesktopSuperiTransport implements SuperiTransport {
  private generation = 0;
  private streamId: string | null = null;
  private lastSequence = 0;
  private nextRequestId = 1;
  private connected = false;
  private disposed = false;
  private connection: Promise<void> | null = null;
  private unlisten: (() => void) | null = null;
  private readonly pending = new Map<string, PendingRequest>();
  private readonly listeners = new Map<
    keyof SuperiEventMap,
    Set<(payload: never) => void>
  >();

  public constructor(private readonly host: DesktopTransportHost = TAURI_HOST) {}

  public async connect(): Promise<void> {
    if (this.disposed) {
      throw new SuperiTransportError(
        localPublicError("unavailable", "terminal", "connect"),
      );
    }
    if (this.connected) {
      return;
    }
    if (this.connection !== null) {
      return this.connection;
    }
    this.connection = this.openConnection().finally(() => {
      this.connection = null;
    });
    return this.connection;
  }

  public async reconnect(): Promise<void> {
    this.connected = false;
    this.cancelPending("The desktop connection was replaced.");
    await this.connect();
  }

  public request<M extends keyof SuperiMethodMap>(
    method: M,
    request: SuperiMethodMap[M]["request"],
  ): Promise<SuperiMethodMap[M]["response"]> {
    return this.requestWithSignal(method, request);
  }

  public async requestWithSignal<M extends keyof SuperiMethodMap>(
    method: M,
    request: SuperiMethodMap[M]["request"],
    signal?: AbortSignal,
  ): Promise<SuperiMethodMap[M]["response"]> {
    const requestId = `desktop-${this.nextRequestId}`;
    this.nextRequestId += 1;
    await this.connect();
    const generation = this.generation;

    if (signal?.aborted) {
      void this.cancelRequest(generation, requestId);
      throw abortError();
    }

    let rejectPending: (reason: unknown) => void = () => {};
    const cancellation = new Promise<never>((_resolve, reject) => {
      rejectPending = reject;
    });
    this.pending.set(requestId, { generation, reject: rejectPending });
    const onAbort = () => {
      const pending = this.pending.get(requestId);
      if (pending !== undefined) {
        pending.reject(abortError());
        void this.cancelRequest(pending.generation, requestId);
      }
    };
    signal?.addEventListener("abort", onAbort, { once: true });

    const invocation = this.host
      .invoke<DesktopTransportReply>(DESKTOP_API_COMMAND, {
        command: {
          operation: "request",
          generation,
          request_id: requestId,
          method,
          request,
        },
      })
      .then((reply) => {
        if (
          reply.kind !== "response" ||
          reply.generation !== generation ||
          reply.request_id !== requestId
        ) {
          throw new SuperiTransportError(
            localPublicError("corrupt_data", "terminal", "receive_response"),
          );
        }
        return reply.response as SuperiMethodMap[M]["response"];
      })
      .catch((error: unknown) => {
        const transportError = asTransportError(error, "request");
        if (transportError.publicError.data.recoverability === "retryable") {
          this.connected = false;
        }
        throw transportError;
      });

    try {
      return await Promise.race([invocation, cancellation]);
    } finally {
      this.pending.delete(requestId);
      signal?.removeEventListener("abort", onAbort);
    }
  }

  public subscribe<E extends keyof SuperiEventMap>(
    event: E,
    listener: (payload: SuperiEventMap[E]) => void,
  ): () => void {
    let listeners = this.listeners.get(event);
    if (listeners === undefined) {
      listeners = new Set();
      this.listeners.set(event, listeners);
    }
    listeners.add(listener as (payload: never) => void);
    void this.connect().catch(() => {});
    return () => {
      const current = this.listeners.get(event);
      current?.delete(listener as (payload: never) => void);
      if (current?.size === 0) {
        this.listeners.delete(event);
      }
    };
  }

  public async dispose(): Promise<void> {
    if (this.disposed) {
      return;
    }
    this.disposed = true;
    this.connected = false;
    const generation = this.generation;
    const cancellations = [...this.pending.entries()].map(([requestId, pending]) => {
      pending.reject(abortError());
      return this.cancelRequest(pending.generation, requestId);
    });
    this.pending.clear();
    this.listeners.clear();
    this.unlisten?.();
    this.unlisten = null;
    if (generation > 0) {
      cancellations.push(
        this.host
          .invoke(DESKTOP_API_COMMAND, {
            command: { operation: "disconnect", generation },
          })
          .then(() => undefined)
          .catch(() => undefined),
      );
    }
    await Promise.all(cancellations);
  }

  private async openConnection(): Promise<void> {
    if (this.unlisten === null) {
      this.unlisten = await this.host.listen<DesktopEventEnvelope>(
        DESKTOP_API_EVENT,
        ({ payload }) => this.acceptEvent(payload),
      );
    }
    const reply = await this.host
      .invoke<DesktopTransportReply>(DESKTOP_API_COMMAND, {
        command: { operation: "connect", after_sequence: this.lastSequence },
      })
      .catch((error: unknown) => {
        throw asTransportError(error, "connect");
      });
    if (reply.kind !== "connected") {
      throw new SuperiTransportError(
        localPublicError("corrupt_data", "terminal", "connect"),
      );
    }
    this.generation = reply.generation;
    this.streamId = reply.stream_id;
    this.connected = true;
    if (reply.resync_required) {
      this.lastSequence = 0;
    }
    for (const event of [...reply.replay].sort(
      (left, right) => left.sequence - right.sequence,
    )) {
      this.acceptEvent(event);
    }
  }

  private acceptEvent(envelope: DesktopEventEnvelope): void {
    if (
      !this.connected ||
      envelope.generation !== this.generation ||
      envelope.stream_id !== this.streamId ||
      envelope.sequence <= this.lastSequence
    ) {
      return;
    }
    if (this.lastSequence > 0 && envelope.sequence !== this.lastSequence + 1) {
      void this.reconnect().catch(() => {});
      return;
    }
    this.lastSequence = envelope.sequence;
    const listeners = this.listeners.get(envelope.event);
    for (const listener of listeners ?? []) {
      listener(envelope.payload as never);
    }
  }

  private cancelPending(message: string): void {
    for (const [requestId, pending] of this.pending) {
      pending.reject(abortError(message));
      void this.cancelRequest(pending.generation, requestId);
    }
    this.pending.clear();
  }

  private cancelRequest(generation: number, requestId: string): Promise<void> {
    return this.host
      .invoke(DESKTOP_API_COMMAND, {
        command: {
          operation: "cancel",
          generation,
          request_id: requestId,
        },
      })
      .then(() => undefined)
      .catch(() => undefined);
  }
}

export function classifyDesktopTransportError(
  error: unknown,
): DesktopTransportFailure {
  const publicError = asTransportError(error, "classify").publicError;
  return {
    condition: publicError.data.recoverability,
    category: publicError.data.category,
    code: publicError.data.code,
    title: publicError.data.title,
    action: publicError.data.action,
    contexts: publicError.data.contexts,
    lastValidResource: publicError.data.last_valid_resource,
  };
}

function asTransportError(
  error: unknown,
  operation: string,
): SuperiTransportError {
  if (error instanceof SuperiTransportError) {
    return error;
  }
  if (isPublicApiError(error)) {
    return new SuperiTransportError(error);
  }
  return new SuperiTransportError(
    localPublicError("unavailable", "retryable", operation),
  );
}

function isPublicApiError(value: unknown): value is PublicApiError {
  if (typeof value !== "object" || value === null) {
    return false;
  }
  const candidate = value as Partial<PublicApiError>;
  return (
    typeof candidate.code === "number" &&
    typeof candidate.message === "string" &&
    typeof candidate.data === "object" &&
    candidate.data !== null &&
    typeof candidate.data.code === "string" &&
    typeof candidate.data.recoverability === "string" &&
    Array.isArray(candidate.data.contexts)
  );
}

function localPublicError(
  category: ErrorCategory,
  recoverability: Recoverability,
  operation: string,
): PublicApiError {
  const title =
    category === "corrupt_data"
      ? "Some data could not be read safely."
      : "A required resource is temporarily unavailable.";
  const action =
    recoverability === "terminal"
      ? "Save any available work and restart Superi. If the problem returns, share the diagnostic report."
      : "Try the operation again. If it keeps failing, review the diagnostic report.";
  return {
    code: -32000,
    message: title,
    data: {
      schema_version: "1.0.0",
      primitive_schema_revision: 1,
      category,
      recoverability,
      code: `error.${category}.${recoverability}`,
      title,
      action,
      contexts: [
        {
          component: "superi-desktop.transport",
          operation,
          fields: {},
        },
      ],
      last_valid_resource: null,
    },
  };
}

function abortError(message = "The desktop request was cancelled."): DOMException {
  return new DOMException(message, "AbortError");
}
