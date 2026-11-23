// The browser host's two boundaries, driven with no browser: the WASI Preview 1 imports an Osprey
// Native application calls, and the `http` capability ([UI-EFFECTS], [NATIVE-SHM-WASM]).

import test from "node:test";
import assert from "node:assert/strict";
import { browserGuest } from "../src/wasi.ts";
import { fetched } from "../src/http.ts";

const ENOSYS = 52;
const PAGE = "http://127.0.0.1:18791/";

type Call = (...args: number[]) => number;

function wasiCalls(memory: WebAssembly.Memory): Record<string, Call> {
  const imports = browserGuest.imports(() => memory.buffer) as { wasi_snapshot_preview1: Record<string, Call> };
  return imports.wasi_snapshot_preview1;
}

function captured(action: () => void): string[] {
  const lines: string[] = [];
  const native = console.debug;
  console.debug = (text: string) => { lines.push(text); };
  try { action(); } finally { console.debug = native; }
  return lines;
}

test("fd_write decodes every iovec from wasm memory and reports the bytes written", () => {
  const memory = new WebAssembly.Memory({ initial: 1 });
  const wasi = wasiCalls(memory);
  const bytes = new Uint8Array(memory.buffer);
  const view = new DataView(memory.buffer);
  bytes.set(new TextEncoder().encode("hello from "), 64);
  bytes.set(new TextEncoder().encode("osprey ✓\n"), 128);
  view.setUint32(16, 64, true);
  view.setUint32(20, 11, true);
  view.setUint32(24, 128, true);
  view.setUint32(28, 11, true);
  for (const fd of [1, 2]) {
    view.setUint32(40, 0, true);
    assert.deepEqual(captured(() => assert.equal(wasi.fd_write(fd, 16, 2, 40), 0)), ["hello from osprey ✓"]);
    assert.equal(view.getUint32(40, true), 22);
  }
  assert.deepEqual(captured(() => assert.equal(wasi.fd_write(3, 16, 1, 40), 0)), []);
  assert.equal(view.getUint32(40, true), 11);
  assert.deepEqual(captured(() => assert.equal(wasi.fd_write(1, 16, 0, 40), 0)), [""]);
  assert.equal(view.getUint32(40, true), 0);
});

test("unknown WASI calls degrade to ENOSYS, known ones answer without files or arguments", () => {
  const memory = new WebAssembly.Memory({ initial: 1 });
  const wasi = wasiCalls(memory);
  for (const name of ["path_open", "fd_read", "fd_prestat_get", "sock_accept", "poll_oneoff"]) assert.equal(wasi[name](), ENOSYS, name);
  const view = new DataView(memory.buffer);
  for (const name of ["args_sizes_get", "environ_sizes_get"]) {
    view.setUint32(8, 7, true);
    view.setUint32(12, 9, true);
    assert.equal(wasi[name](8, 12), 0);
    assert.deepEqual([view.getUint32(8, true), view.getUint32(12, true)], [0, 0], name);
  }
  for (const name of ["args_get", "environ_get", "proc_exit", "sched_yield", "fd_close", "fd_seek", "fd_fdstat_get"]) assert.equal(wasi[name](), 0, name);
});

test("the clock reads nanoseconds now and random bytes fill the named range only", () => {
  const memory = new WebAssembly.Memory({ initial: 1 });
  const wasi = wasiCalls(memory);
  const view = new DataView(memory.buffer);
  const before = BigInt(Date.now()) * 1_000_000n;
  assert.equal(wasi.clock_time_get(0, 1, 256), 0);
  const read = view.getBigUint64(256, true);
  assert.ok(read >= before && read <= BigInt(Date.now()) * 1_000_000n);
  const bytes = new Uint8Array(memory.buffer);
  assert.equal(wasi.random_get(512, 64), 0);
  assert.ok(bytes.subarray(512, 576).some((byte) => byte !== 0));
  assert.ok(bytes.subarray(576, 640).every((byte) => byte === 0));
  const own = new Uint8Array(32);
  browserGuest.random(own);
  assert.ok(own.some((byte) => byte !== 0));
});

test("start runs the application's _start once and answers success", () => {
  let runs = 0;
  const instance = { exports: { _start: () => { runs += 1; } } } as unknown as WebAssembly.Instance;
  assert.equal(browserGuest.start(instance), 0);
  assert.equal(runs, 1);
});

interface Request { url: string; init: RequestInit }

function ask(fields: { kind?: string; method?: string; url: string; headers?: [string, string][]; body?: string; limit?: number; timeout?: number; key?: string }) {
  const texts: Record<number, string> = { 1: fields.method ?? "", 2: fields.url, 4: fields.body ?? "", 7: fields.key ?? "reply" };
  const numbers: Record<number, number> = { 5: fields.limit ?? 1 << 20, 6: fields.timeout ?? 5000 };
  return {
    kind: fields.kind ?? "http",
    text: (field: number) => texts[field] ?? "",
    number: (field: number) => numbers[field] ?? 0,
    pairs: () => (fields.headers ?? []).map(([name, value]) => ({ name, value })),
  } as unknown as Parameters<typeof fetched>[0];
}

async function withFetch<T>(reply: (request: Request) => Response | Promise<Response>, action: (seen: Request[]) => Promise<T>): Promise<T> {
  const seen: Request[] = [];
  const native = globalThis.fetch;
  const nativeLocation = (globalThis as { location?: unknown }).location;
  (globalThis as { location?: unknown }).location = new URL(PAGE);
  globalThis.fetch = (async (url: URL, init: RequestInit) => {
    const request = { url: String(url), init };
    seen.push(request);
    return reply(request);
  }) as typeof fetch;
  try { return await action(seen); } finally {
    globalThis.fetch = native;
    (globalThis as { location?: unknown }).location = nativeLocation;
  }
}

test("an http ask fetches with no credentials, cache or redirects and lands its body in the slice", async () => {
  await withFetch(() => new Response('{"error":"insufficient funds"}', { status: 422 }), async (seen) => {
    const done = await fetched(ask({ method: "POST", url: "/api/withdraw", headers: [["Content-Type", "application/json"]], body: '{"cents":5}', key: "withdraw" }));
    assert.deepEqual(JSON.parse(done.detail), { status: 422, headers: [] });
    assert.deepEqual(done.slices, [{ key: "withdraw", text: '{"error":"insufficient funds"}' }]);
    assert.equal(seen[0].url, PAGE + "api/withdraw");
    assert.equal(seen[0].init.method, "POST");
    assert.equal(seen[0].init.body, '{"cents":5}');
    assert.deepEqual(seen[0].init.headers, { "Content-Type": "application/json" });
    assert.equal(seen[0].init.credentials, "omit");
    assert.equal(seen[0].init.cache, "no-store");
    assert.equal(seen[0].init.redirect, "manual");
    assert.ok(seen[0].init.signal instanceof AbortSignal);
    const listed = await fetched(ask({ url: "http://127.0.0.1:18790/api/accounts" }));
    assert.equal(seen[1].init.method, "GET");
    assert.equal(seen[1].init.body, undefined);
    assert.deepEqual(JSON.parse(listed.detail), { status: 422, headers: [] });
  });
});

test("only https, this page's origin or this machine's http is reachable", async () => {
  await withFetch(() => new Response("ok"), async (seen) => {
    for (const url of ["https://example.com/a", "http://localhost:9/b", "http://[::1]:9/c", "http://127.0.0.1:18790/d", "/relative"]) {
      assert.equal((await fetched(ask({ url }))).slices?.[0].text, "ok", url);
    }
    for (const url of ["http://example.com/", "http://127.0.0.2/", "ftp://127.0.0.1/", "file:///etc/passwd", "data:text/plain,hi", "javascript:alert(1)"]) {
      await assert.rejects(fetched(ask({ url })), /Only https URLs/, url);
    }
    assert.equal(seen.length, 5);
    await assert.rejects(fetched(ask({ kind: "clipboard", url: "/" })), /answers no clipboard requests/);
    assert.equal(seen.length, 5);
  });
});

test("a reply over its limit is refused, and a failed fetch keeps its reason", async () => {
  await withFetch(() => new Response("x".repeat(65)), async () => {
    await assert.rejects(fetched(ask({ url: "/big", limit: 64 })), /exceeds the 64 byte limit/);
    assert.equal((await fetched(ask({ url: "/exact", limit: 65 }))).slices?.[0].text.length, 65);
    await assert.rejects(fetched(ask({ url: "/none", limit: -1 })), /exceeds the 0 byte limit/);
  });
  await withFetch(() => { throw new TypeError("Load failed"); }, async () => {
    await assert.rejects(fetched(ask({ url: "/api/accounts" })), /Load failed/);
  });
});

test("an endless reply stops at its limit instead of buffering without bound", { timeout: 5000 }, async () => {
  let pulled = 0;
  let cancelled = false;
  const endless = () => new Response(new ReadableStream<Uint8Array>({
    pull: (controller) => { pulled += 1; controller.enqueue(new Uint8Array(1024)); },
    cancel: () => { cancelled = true; },
  }));
  await withFetch(endless, async () => {
    await assert.rejects(fetched(ask({ url: "/endless", limit: 4096 })), /exceeds the 4096 byte limit/);
  });
  assert.ok(pulled <= 8, `read ${pulled} chunks past a 4 KiB limit`);
  assert.equal(cancelled, true);
});
