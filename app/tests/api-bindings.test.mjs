import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { pathToFileURL, fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import test from "node:test";

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repositoryRoot = resolve(appRoot, "..");

function read(path) {
  return readFileSync(path, "utf8");
}

test("production app exposes the canonical generated client through one injected provider", () => {
  const api = read(resolve(appRoot, "src/api.ts"));
  const context = read(resolve(appRoot, "src/api-context.tsx"));
  const lifecycle = read(resolve(appRoot, "src/lifecycle.ts"));
  const main = read(resolve(appRoot, "src/main.tsx"));

  assert.match(api, /export \* from "\.\.\/\.\.\/open\/bindings\/typescript\/superi-api\.ts"/);
  assert.match(api, /SuperiMethodMap/);
  assert.match(api, /SuperiEventMap/);
  assert.match(api, /SuperiResourceMap/);
  assert.match(api, /export interface SuperiApiBindings/);
  assert.match(api, /export function createSuperiApiBindings/);
  assert.match(context, /export interface SuperiApiProviderProps/);
  assert.match(context, /export function SuperiApiProvider/);
  assert.match(context, /export function useSuperiApi/);
  assert.match(context, /transport: SuperiTransport \| null/);
  assert.match(main, /new DesktopSuperiTransport\(\)/);
  assert.match(main, /<SuperiApiProvider transport=\{transport\}>/);
  assert.doesNotMatch(api, /@tauri-apps\/api|\binvoke\b|\blisten\b|reconnect|cancellation/);
  assert.doesNotMatch(context, /@tauri-apps\/api|\binvoke\b|\blisten\b|reconnect|cancellation/);
  assert.doesNotMatch(lifecycle, /open\/bindings\/typescript\/superi-api/);
});

test("canonical artifact retains typed methods, events, resources, scripts, and client transport", () => {
  const generated = read(
    resolve(repositoryRoot, "open/bindings/typescript/superi-api.ts"),
  );

  assert.match(generated, /export interface SuperiMethodMap/);
  assert.match(generated, /"superi\.project\.script\.run"/);
  assert.match(generated, /export interface SuperiEventMap/);
  assert.match(generated, /"superi\.project\.state\.changed"/);
  assert.match(generated, /export interface SuperiResourceMap/);
  assert.match(generated, /"superi\.editor\.state"/);
  assert.match(generated, /export interface SuperiTransport/);
  assert.match(generated, /export class SuperiClient/);
  assert.match(generated, /public subscribe<E extends keyof SuperiEventMap>/);
  assert.match(generated, /export type EditorNestedSequenceRequest/);
  assert.match(generated, /export type EditorCompoundClipRequest/);
  assert.match(generated, /action: "place_nested_sequence"/);
  assert.match(generated, /action: "create_compound_clip"/);
  assert.match(generated, /result: "nested_sequence_placed"/);
  assert.match(generated, /result: "compound_clip_created"/);
});

test("binding factory forwards request and subscription behavior without transport policy", () => {
  const apiUrl = pathToFileURL(resolve(appRoot, "src/api.ts")).href;
  const probe = `
    import assert from "node:assert/strict";
    import { createSuperiApiBindings, SuperiClient } from ${JSON.stringify(apiUrl)};

    const pending = Promise.resolve({ snapshot: { condition: "starting" } });
    const unsubscribe = () => {};
    const listener = () => {};
    const calls = [];
    const transport = {
      request(method, request) {
        calls.push({ kind: "request", method, request });
        return pending;
      },
      subscribe(event, receivedListener) {
        calls.push({ kind: "subscribe", event, listener: receivedListener });
        return unsubscribe;
      },
    };

    const bindings = createSuperiApiBindings(transport);
    assert.ok(Object.isFrozen(bindings));
    assert.ok(bindings.client instanceof SuperiClient);
    assert.strictEqual(bindings.request, bindings.automation);
    assert.strictEqual(
      bindings.request("superi.engine.integration.validation.get", {}),
      pending,
    );
    assert.deepEqual(calls[0], {
      kind: "request",
      method: "superi.engine.integration.validation.get",
      request: {},
    });
    assert.strictEqual(
      bindings.subscribe("superi.project.state.changed", listener),
      unsubscribe,
    );
    assert.deepEqual(calls[1], {
      kind: "subscribe",
      event: "superi.project.state.changed",
      listener,
    });
  `;
  const result = spawnSync(
    process.execPath,
    ["--no-warnings", "--experimental-transform-types", "--input-type=module", "--eval", probe],
    { encoding: "utf8" },
  );

  assert.equal(result.status, 0, result.stderr || result.stdout);
});
