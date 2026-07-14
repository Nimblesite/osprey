import test from "node:test";
import assert from "node:assert/strict";
import { OspreyReactHost } from "../src/host.js";

function hostHarness() {
  const events = [];
  const logs = [];
  const host = Object.create(OspreyReactHost.prototype);
  host.bridge = { trace: [] };
  host.httpSequence = 0;
  host.logger = {
    info: (...parts) => logs.push(["info", ...parts]),
    warn: (...parts) => logs.push(["warn", ...parts]),
  };
  host.dispatch = (event) => events.push(event);
  return { events, host, logs };
}

async function withFetch(replacement, action) {
  const nativeFetch = globalThis.fetch;
  globalThis.fetch = replacement;
  try {
    await action();
  } finally {
    globalThis.fetch = nativeFetch;
  }
}

function traceMetadata(entry) {
  const keys = ["commandId", "method", "url", "requestBytes"];
  return Object.fromEntries(keys.map((key) => [key, entry[key]]));
}

function assertBodyPrivacy(logs, trace) {
  const diagnostics = JSON.stringify({ logs, trace });
  assert.equal(diagnostics.includes("private note"), false);
  assert.equal(diagnostics.includes("insufficient funds"), false);
}

test("traces HTTP metadata without retaining request or response bodies", async () => {
  const { events, host, logs } = hostHarness();
  const body = '{"account":1,"cents":1234,"note":"private note"}';
  await withFetch(
    async () => ({ status: 422, text: async () => '{"error":"insufficient funds"}' }),
    () => host.runHttp({ id: "mutate-withdraw", method: "POST", url: "/api/withdraw", body }),
  );

  assert.equal(events[0].status, 422);
  assert.equal(events[0].data, '{"error":"insufficient funds"}');
  assert.equal(host.bridge.trace.length, 2);
  assert.equal(host.bridge.trace[0].correlationId, host.bridge.trace[1].correlationId);
  assert.deepEqual(traceMetadata(host.bridge.trace[0]), {
    commandId: "mutate-withdraw", method: "POST", url: "/api/withdraw", requestBytes: 48,
  });
  assert.equal(host.bridge.trace[1].status, 422);
  assert.equal(host.bridge.trace[1].responseBytes, 30);
  assert.ok(host.bridge.trace[1].durationMs >= 0);
  assertBodyPrivacy(logs, host.bridge.trace);
});

test("turns fetch failures into useful JSON errors and bounded diagnostics", async () => {
  const { events, host } = hostHarness();
  await withFetch(async () => { throw new TypeError("Load failed"); }, async () => {
    for (let index = 0; index < 40; index += 1) {
      await host.runHttp({ id: "accounts", method: "GET", url: "/api/accounts" });
    }
  });

  const failure = events.at(-1);
  assert.equal(failure.status, 0);
  assert.deepEqual(JSON.parse(failure.data), {
    error: "GET /api/accounts could not reach the ledger: Load failed",
  });
  assert.equal(host.bridge.trace.length, 64);
  assert.equal(host.bridge.trace.at(-1).status, 0);
  assert.equal(host.bridge.trace.at(-1).responseBytes, 0);
  assert.match(host.bridge.trace.at(-1).correlationId, /^accounts-\d+$/);
});
