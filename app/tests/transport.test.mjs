import assert from "node:assert/strict";
import { dirname, resolve } from "node:path";
import { pathToFileURL, fileURLToPath } from "node:url";
import test from "node:test";

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");

test("desktop transport connects through the single native dispatcher", async () => {
  const transportUrl = pathToFileURL(resolve(appRoot, "src/transport.ts")).href;
  const { DesktopSuperiTransport } = await import(transportUrl);
  const calls = [];
  const host = {
    async invoke(command, args) {
      calls.push({ command, args });
      return {
        kind: "connected",
        generation: 1,
        stream_id: "superi.desktop.events.v1",
        replay: [],
        resync_required: false,
      };
    },
    async listen() {
      return () => {};
    },
  };
  const transport = new DesktopSuperiTransport(host);

  await transport.connect();

  assert.equal(calls.length, 1);
  assert.equal(calls[0].command, "desktop_api_dispatch");
  assert.deepEqual(calls[0].args, {
    command: { operation: "connect", after_sequence: 0 },
  });
  await transport.dispose();
});

test("generated requests retain identity and classified public failures", async () => {
  const transportUrl = pathToFileURL(resolve(appRoot, "src/transport.ts")).href;
  const {
    DesktopSuperiTransport,
    classifyDesktopTransportError,
  } = await import(transportUrl);
  const calls = [];
  const terminal = {
    code: -32000,
    message: "Superi could not complete the operation.",
    data: {
      schema_version: { major: 1, minor: 0, patch: 0 },
      primitive_schema_revision: 1,
      category: "internal",
      recoverability: "terminal",
      code: "error.internal.terminal",
      title: "Superi could not complete the operation.",
      action: "Save any available work and restart Superi.",
      contexts: [{
        component: "superi-desktop.transport",
        operation: "request",
        fields: { request_id: "desktop-2" },
      }],
      last_valid_resource: {
        resource: "superi.engine.introspection",
        schema_version: { major: 1, minor: 0, patch: 0 },
        identity: "engine",
        revision: 9,
      },
    },
  };
  let shouldFail = false;
  const host = {
    async invoke(command, args) {
      calls.push({ command, args });
      if (args.command.operation === "connect") {
        return {
          kind: "connected",
          generation: 4,
          stream_id: "superi.desktop.events.v1",
          replay: [],
          resync_required: false,
        };
      }
      if (args.command.operation === "request" && shouldFail) {
        throw terminal;
      }
      if (args.command.operation === "request") {
        return {
          kind: "response",
          generation: 4,
          request_id: args.command.request_id,
          response: { snapshot: { coherent: true } },
        };
      }
      return { kind: "disconnected", generation: 4 };
    },
    async listen() {
      return () => {};
    },
  };
  const transport = new DesktopSuperiTransport(host);

  const response = await transport.request(
    "superi.engine.integration.validation.get",
    null,
  );
  assert.deepEqual(response, { snapshot: { coherent: true } });
  assert.deepEqual(calls[1], {
    command: "desktop_api_dispatch",
    args: {
      command: {
        operation: "request",
        generation: 4,
        request_id: "desktop-1",
        method: "superi.engine.integration.validation.get",
        request: null,
      },
    },
  });

  shouldFail = true;
  await assert.rejects(
    transport.request("superi.engine.integration.validation.get", null),
    (error) => {
      assert.equal(error.name, "SuperiTransportError");
      assert.strictEqual(error.publicError, terminal);
      const failure = classifyDesktopTransportError(error);
      assert.equal(failure.condition, "terminal");
      assert.equal(failure.code, "error.internal.terminal");
      assert.equal(failure.contexts[0].fields.request_id, "desktop-2");
      assert.equal(failure.lastValidResource.revision, 9);
      return true;
    },
  );
  await transport.dispose();
});

test("ordered events reject stale delivery and replay exactly once after reconnect", async () => {
  const transportUrl = pathToFileURL(resolve(appRoot, "src/transport.ts")).href;
  const { DesktopSuperiTransport } = await import(transportUrl);
  const connectCalls = [];
  let eventListener = null;
  let connectCount = 0;
  const envelope = (generation, sequence, revision) => ({
    generation,
    stream_id: "superi.desktop.events.v1",
    sequence,
    event: "superi.engine.introspection.changed",
    payload: { snapshot: { revision } },
  });
  const host = {
    async invoke(_command, args) {
      if (args.command.operation === "connect") {
        connectCalls.push(args.command);
        connectCount += 1;
        return {
          kind: "connected",
          generation: connectCount,
          stream_id: "superi.desktop.events.v1",
          replay: connectCount === 2 ? [envelope(2, 2, 2)] : [],
          resync_required: false,
        };
      }
      return { kind: "disconnected", generation: args.command.generation };
    },
    async listen(_event, listener) {
      eventListener = listener;
      return () => {
        eventListener = null;
      };
    },
  };
  const transport = new DesktopSuperiTransport(host);
  const revisions = [];
  const unsubscribe = transport.subscribe(
    "superi.engine.introspection.changed",
    (payload) => revisions.push(payload.snapshot.revision),
  );
  await transport.connect();

  eventListener({ payload: envelope(1, 1, 1) });
  eventListener({ payload: envelope(1, 1, 100) });
  await transport.reconnect();
  eventListener({ payload: envelope(1, 3, 100) });
  eventListener({ payload: envelope(2, 3, 3) });

  assert.deepEqual(revisions, [1, 2, 3]);
  assert.equal(connectCalls[1].after_sequence, 1);
  unsubscribe();
  await transport.dispose();
});

test("aborting a pending request sends one generation-scoped cancellation", async () => {
  const transportUrl = pathToFileURL(resolve(appRoot, "src/transport.ts")).href;
  const { DesktopSuperiTransport } = await import(transportUrl);
  const calls = [];
  const host = {
    async invoke(_command, args) {
      calls.push(args.command);
      if (args.command.operation === "connect") {
        return {
          kind: "connected",
          generation: 6,
          stream_id: "superi.desktop.events.v1",
          replay: [],
          resync_required: false,
        };
      }
      if (args.command.operation === "request") {
        return new Promise(() => {});
      }
      return {
        kind: "cancelled",
        generation: 6,
        request_id: args.command.request_id,
        cancelled: true,
      };
    },
    async listen() {
      return () => {};
    },
  };
  const transport = new DesktopSuperiTransport(host);
  const abort = new AbortController();
  const pending = transport.requestWithSignal(
    "superi.engine.integration.validation.get",
    null,
    abort.signal,
  );
  await Promise.resolve();
  await Promise.resolve();
  abort.abort();

  await assert.rejects(pending, (error) => error.name === "AbortError");
  await new Promise((resolvePromise) => setTimeout(resolvePromise, 0));
  assert.equal(
    calls.filter((command) => command.operation === "cancel").length,
    1,
  );
  assert.deepEqual(
    calls.find((command) => command.operation === "cancel"),
    { operation: "cancel", generation: 6, request_id: "desktop-1" },
  );
  await transport.dispose();
});
