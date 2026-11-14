// [NATIVE-SHM-WASM]: the WASI host and the worker that hold an application. Main's in-memory WASI
// shim became node's preview1 host inside one worker per panel; these cases guard the same promises
// there with real guests: no filesystem or environment is granted, every bound the worker reads
// from the guest is checked, and a guest that fails, loops or exits never yields a stale result.
import * as assert from "assert";
import { spawnSync } from "node:child_process";
import { EventEmitter } from "node:events";
import * as fs from "node:fs";
import * as os from "node:os";
import threads = require("node:worker_threads");
import * as sinon from "sinon";
import { compileWasi } from "../../client/src/osprey-ui/wasi";
import { startNativeRuntime, type NativeRuntime } from "../../client/src/osprey-ui/wasi-runner";
import { readFrame } from "../../client/src/osprey-ui/webview/rows";
import { frame } from "./native-rows-kit";
import {
  ERRNO, I32, expect, forever, guestModule, integer, iovec, load32, load8, ok, probe, runProbe, spin, store32, store8, stubApplication, wasi, wide, type StubBodies,
} from "./wasm-guest-kit";

const open = { now: 1, offset: 0, platform: "vscode", home: "/home", dark: false, arguments: [] };
const touch = { kind: 0, ordinal: 0, words: "", detail: "", x: 0, y: 0, down: false, shift: false };

async function stub(bodies: StubBodies = {}, signal?: AbortSignal): Promise<NativeRuntime> {
  return startNativeRuntime(await compileWasi(stubApplication(bodies)), signal);
}

/** Runs `use` against a fresh stub runtime and always disposes it. */
async function withStub<T>(bodies: StubBodies, use: (runtime: NativeRuntime) => Promise<T>): Promise<T> {
  const runtime = await stub(bodies);
  try { return await use(runtime); } finally { runtime.dispose(); }
}

/** Every ungranted-descriptor call `operation` makes on `fd`, expecting `errno`. */
function descriptorCall(operation: string, fd: number): number[] {
  switch (operation) {
    case "fd_seek": return wasi("fd_seek", integer(fd), wide(0), integer(0), integer(64));
    case "fd_close": return wasi("fd_close", integer(fd));
    default: return wasi(operation, integer(fd), integer(64));
  }
}

/** The runner fed by a scripted worker: `script` drives its events after the first call is sent. */
async function scriptedWorker(script: (worker: EventEmitter) => void): Promise<unknown> {
  const worker = Object.assign(new EventEmitter(), { postMessage: () => undefined, terminate: async () => 0 });
  const constructor = sinon.stub(threads, "Worker").returns(worker as unknown as threads.Worker);
  try {
    const runtime = startNativeRuntime(await compileWasi(stubApplication()));
    const pending = runtime.open(open);
    script(worker);
    try { return await pending; } finally { runtime.dispose(); }
  } finally {
    constructor.restore();
  }
}

const reply = (result: unknown) => (worker: EventEmitter) => { worker.emit("message", { id: 1, result }); };
const empty = new ArrayBuffer(0);

suite("WASI worker failure boundary", () => {
  test("the worker entry refuses execution without its private message port", () => {
    const entry = require.resolve("../../client/src/osprey-ui/wasi-worker");
    const result = spawnSync(process.execPath, [entry], { encoding: "utf8", env: { ...process.env, ELECTRON_RUN_AS_NODE: "1" } });
    assert.strictEqual(result.status, 1);
    assert.ok(result.stderr.includes("requires a worker port"), result.stderr);
  });

  test("the worker entry refuses execution without its private message port (extra: no stdout is written before the refusal)", () => {
    const entry = require.resolve("../../client/src/osprey-ui/wasi-worker");
    const result = spawnSync(process.execPath, [entry, "--unused"], { encoding: "utf8", env: { ...process.env, ELECTRON_RUN_AS_NODE: "1" } });
    assert.deepStrictEqual([result.status, result.stdout], [1, ""]);
  });

  test("worker startup failures and unexpected exits reject without a stale result", async function () {
    this.timeout(8000);
    const runtime = await stub({ start: [0x00] });
    try {
      await assert.rejects(runtime.open(open), /unreachable/);
      // Long after the worker is gone, a call must still be answered at once.
      await new Promise((resolve) => setTimeout(resolve, 500));
      const started = Date.now();
      await assert.rejects(runtime.event(touch), /unreachable|exited/);
      assert.ok(Date.now() - started < 2000, "a call after the worker died is refused at once, not after the call timeout");
    } finally { runtime.dispose(); }
    await assert.rejects(scriptedWorker((worker) => worker.emit("error", new Error("worker startup failed"))), /worker startup failed/);
    await assert.rejects(scriptedWorker((worker) => worker.emit("exit", 2)), /exited with 2/);
  });

  test("worker startup failures and unexpected exits reject without a stale result (extra: a module without the bridge is named)", async () => {
    const runtime = startNativeRuntime(await compileWasi(guestModule([], [{ name: "_start", params: [], body: [] }])));
    try { await assert.rejects(runtime.open(open), /does not export malloc/); } finally { runtime.dispose(); }
  });

  test("malformed worker replies never become successful WASI output", async () => {
    for (const result of [undefined, null, "invalid", {}, { status: 0 }, { status: 0, revision: 1 }, { status: "0", revision: 1, runs: {} },
      { status: 0, revision: 1, runs: { shapes: empty, views: empty, text: empty } }, { status: 0, revision: 1, runs: { shapes: empty, views: empty, text: empty, asks: "rows" } }]) {
      await assert.rejects(scriptedWorker(reply(result)), /no result|malformed/, JSON.stringify(result));
    }
    await assert.rejects(scriptedWorker((worker) => worker.emit("message", { id: 1, error: "guest failed" })), /guest failed/);
  });

  test("malformed worker replies never become successful WASI output (extra: replies for other calls are ignored)", async () => {
    const good = { status: 0, revision: 2, runs: { shapes: empty, views: empty, text: empty, asks: empty } };
    const answered = await scriptedWorker((worker) => {
      for (const forged of [null, { id: "1", result: good }, { id: 7, result: good }, { result: good }]) worker.emit("message", forged);
      worker.emit("message", { id: 1, result: { ...good, revision: 3 } });
    });
    assert.strictEqual((answered as { revision: number }).revision, 3);
  });
});

suite("package workspace in-memory WASI", () => {
  test("an aborted command never starts and an infinite guest can be cancelled", async () => {
    const cancelled = new AbortController();
    cancelled.abort();
    await assert.rejects(stub({}, cancelled.signal), /cancelled before it started/);
    const active = new AbortController();
    const runtime = await stub({ event: [...forever, ...wide(0)] }, active.signal);
    await runtime.open(open);
    const pending = runtime.event(touch);
    const timer = setTimeout(() => active.abort(), 100);
    try { await assert.rejects(pending, /cancelled/); } finally { clearTimeout(timer); runtime.dispose(); }
  });

  test("an aborted command never starts and an infinite guest can be cancelled (extra: disposal ends an infinite call)", async () => {
    const runtime = await stub({ open: [...forever, ...wide(0)] });
    const pending = runtime.open(open);
    setTimeout(() => runtime.dispose(), 50);
    await assert.rejects(pending, /disposed/);
    await assert.rejects(runtime.event(touch), /disposed/);
  });

  test("a CPU-bound command keeps the host event loop responsive", async () => {
    // The event takes nine parameters, so its counter is local 9.
    await withStub({ event: [...spin(60_000_000, 9), ...wide(0)], locals: { event: [0x7e] } }, async (runtime) => {
      await runtime.open(open);
      let ticks = 0;
      const timer = setInterval(() => { ticks += 1; }, 5);
      try { assert.strictEqual((await runtime.event(touch)).status, 0); } finally { clearInterval(timer); }
      assert.ok(ticks >= 3, `host timers must run while the application computes; received ${ticks} ticks`);
    });
  });

  test("a CPU-bound command keeps the host event loop responsive (extra: a busy start leaves timers running)", async () => {
    let ticks = 0;
    const timer = setInterval(() => { ticks += 1; }, 5);
    try {
      await withStub({ start: spin(60_000_000), locals: { start: [0x7e] } }, async (runtime) => { assert.strictEqual((await runtime.open(open)).status, 0); });
    } finally { clearInterval(timer); }
    assert.ok(ticks >= 3, `received ${ticks} ticks`);
  });

  test("a real guest consumes UTF-8 args, environment, clocks, entropy and vectored streams", async () => {
    const positive64 = (at: number) => expect([...integer(at), 0x29, 3, 0, ...wide(0), 0x56], 1);
    const checks = [
      ...ok("args_sizes_get", integer(64), integer(68)), ...expect(load32(64), 2), ...expect(load32(68), 7),
      ...ok("args_get", integer(256), integer(512)), ...expect(load8(512 + 4), 0xc3),
      ...ok("environ_sizes_get", integer(72), integer(76)), ...expect(load32(72), 1), ...expect(load32(76), 8),
      ...ok("environ_get", integer(280), integer(768)), ...expect(load8(768), 77),
      ...ok("clock_res_get", integer(0), integer(160)), ...positive64(160),
      ...ok("clock_time_get", integer(0), wide(0), integer(168)), ...positive64(168),
      ...store8(4096, 126), ...store8(4113, 126), ...ok("random_get", integer(4097), integer(16)),
      ...expect(load8(4096), 126), ...expect(load8(4113), 126),
      ...iovec(1024, 1200, 1), ...iovec(1032, 1208, 3),
      ...expect(wasi("fd_write", integer(3), integer(1024), integer(2), integer(1120)), ERRNO.badf),
    ];
    assert.strictEqual(await runProbe(probe(checks), { args: ["cli", "é"], env: { MODE: "π" } }), 0);
  });

  test("a real guest consumes UTF-8 args, environment, clocks, entropy and vectored streams (extra: the host grants no arguments or environment of its own)", async () => {
    const checks = [...ok("args_sizes_get", integer(64), integer(68)), ...expect(load32(64), 0), ...expect(load32(68), 0),
      ...ok("environ_sizes_get", integer(72), integer(76)), ...expect(load32(72), 0), ...expect(load32(76), 0)];
    assert.strictEqual(await runProbe(probe(checks)), 0, "the extension host's environment never reaches an application");
  });

  test("stdin is the host frame and stdout is captured without temporary files", async () => {
    const before = fs.readdirSync(os.tmpdir()).length;
    await withStub({}, async (runtime) => {
      const frame = await runtime.open(open);
      assert.deepStrictEqual([frame.status, frame.runs.views.byteLength, frame.runs.asks.byteLength], [0, 0, 0]);
      assert.ok(frame.runs.text instanceof ArrayBuffer, "the frame is copied out of linear memory");
    });
    assert.strictEqual(fs.readdirSync(os.tmpdir()).length, before);
  });

  test("stdin is the host frame and stdout is captured without temporary files (extra: an application's status rides its frame)", async () => {
    await withStub({ open: wide(5), event: wide(1) }, async (runtime) => {
      assert.strictEqual((await runtime.open(open)).status, 5);
      assert.strictEqual((await runtime.event(touch)).status, 1);
    });
  });

  test("malformed UTF-8 output is quarantined instead of changing serialized text", () => {
    assert.throws(() => readFrame(malformedLabel([0xff])), /not UTF-8/);
    assert.throws(() => readFrame(malformedLabel([0xfe, 0xff])), /not UTF-8/);
    assert.strictEqual(readFrame(malformedLabel([0xc3, 0xa9])).controls[0]?.words, "oké", "well-formed text is kept exactly");
  });

  test("malformed UTF-8 output is quarantined instead of changing serialized text (extra: truncated sequences are refused too)", () => {
    for (const bytes of [[0xc3], [0xe2, 0x86], [0xed, 0xa0, 0x80], [0xf8, 0x88, 0x80, 0x80, 0x80]]) {
      assert.throws(() => readFrame(malformedLabel(bytes)), /not UTF-8/, JSON.stringify(bytes));
    }
  });

  test("a second run gets a fresh stdin cursor, so dispatch is not stateful in the host", async () => {
    // The application counts its opens in its own memory: each runtime is a fresh instance.
    const counted = { open: [...integer(8), ...load32(8), ...integer(1), 0x6a, 0x36, 2, 0, ...load32(8), 0xad] };
    for (let run = 0; run < 2; run += 1) {
      await withStub(counted, async (runtime) => { assert.strictEqual((await runtime.open(open)).status, 1, `run ${run} starts fresh`); });
    }
  });

  test("a second run gets a fresh stdin cursor, so dispatch is not stateful in the host (extra: one instance keeps its own state across calls)", async () => {
    const counted = { event: [...integer(8), ...load32(8), ...integer(1), 0x6a, 0x36, 2, 0, ...load32(8), 0xad] };
    await withStub(counted, async (runtime) => {
      await runtime.open(open);
      const statuses = [];
      for (let call = 0; call < 3; call += 1) statuses.push((await runtime.event(touch)).status);
      assert.deepStrictEqual(statuses, [1, 2, 3]);
    });
  });

  test("an unimplemented WASI import is named, never answered with ENOSYS", async () => {
    const runtime = await stub({ imports: [{ name: "fd_advise_v2", params: [] }] });
    try { await assert.rejects(runtime.open(open), /fd_advise_v2/); } finally { runtime.dispose(); }
  });

  test("an unimplemented WASI import is named, never answered with ENOSYS (extra: a call spelt in another case is named too)", async () => {
    const runtime = await stub({ imports: [{ name: "FD_WRITE", params: [I32, I32, I32, I32], result: I32 }] });
    try { await assert.rejects(runtime.open(open), /FD_WRITE/); } finally { runtime.dispose(); }
  });

  test("an unknown import module is named rather than surfacing a bare LinkError", async () => {
    const runtime = await stub({ imports: [{ module: "env", name: "osprey_slice", params: [] }] });
    try { await assert.rejects(runtime.open(open), /osprey_slice/); } finally { runtime.dispose(); }
  });

  test("an unknown import module is named rather than surfacing a bare LinkError (extra: a foreign module name is named)", async () => {
    const runtime = await stub({ imports: [{ module: "wasi_unstable", name: "fd_write", params: [] }] });
    try { await assert.rejects(runtime.open(open), /wasi_unstable/); } finally { runtime.dispose(); }
  });

  test("arguments and environment are bounded before the module runs", async () => {
    const checks = [...ok("environ_sizes_get", integer(72), integer(76)), ...expect(load32(72), 0)];
    process.env.OSPREY_SECRET_PROBE = "token";
    try { assert.strictEqual(await runProbe(probe(checks)), 0, "a secret in the extension host's environment stays there"); }
    finally { delete process.env.OSPREY_SECRET_PROBE; }
  });

  test("arguments and environment are bounded before the module runs (extra: open arguments reach the application, not WASI)", async () => {
    const counter = [...integer(8), ...load32(8), ...integer(1), 0x6a, 0x36, 2, 0];
    await withStub({ argument: [...counter, ...wide(0)], open: [...load32(8), 0xad] }, async (runtime) => {
      assert.strictEqual((await runtime.open({ ...open, arguments: ["a", "é", ""] })).status, 3, "each argument is one call before open");
    });
    const checks = [...ok("args_sizes_get", integer(64), integer(68)), ...expect(load32(64), 0)];
    assert.strictEqual(await runProbe(probe(checks)), 0, "WASI itself carries no arguments");
  });

  test("libc can enumerate preopens but receives EBADF because no filesystem is granted", async () => {
    for (const call of [[...wasi("fd_prestat_get", integer(3), integer(0))], [...wasi("fd_prestat_dir_name", integer(3), integer(0), integer(0))]]) {
      assert.strictEqual(await runProbe(probe(expect(call, ERRNO.badf))), 0);
    }
  });

  test("libc can enumerate preopens but receives EBADF because no filesystem is granted (extra: no descriptor up to 64 is a preopen)", async () => {
    const checks = Array.from({ length: 62 }, (_, at) => expect(wasi("fd_prestat_get", integer(at + 3), integer(0)), ERRNO.badf)).flat();
    assert.strictEqual(await runProbe(probe(checks)), 0);
  });

  test("real guests cannot cross memory or vector-count host bounds", async () => {
    const writeLength = (value: number[]) => [0x20, 2, ...value, 0x37, 3, 0, ...integer(0)];
    const outside = { length: writeLength(wide(16)), view: [0x20, 5, ...integer(0x7fffff00), 0x36, 2, 0, ...integer(0)] };
    await withStub(outside, async (runtime) => { await assert.rejects(runtime.open(open), /outside linear memory/); });
    await withStub({ length: writeLength([0x42, 0x7f]) }, async (runtime) => { await assert.rejects(runtime.open(open), /unreadable run length/); });
  });

  test("real guests cannot cross memory or vector-count host bounds (extra: a run that ends one byte past memory is refused)", async () => {
    const writeLength = [0x20, 2, ...wide(2), 0x37, 3, 0, ...integer(0)];
    const view = [0x20, 5, ...integer(2 * 65536 - 1), 0x36, 2, 0, ...integer(0)];
    await withStub({ length: writeLength, view }, async (runtime) => { await assert.rejects(runtime.open(open), /outside linear memory/); });
  });

  test("a real guest cannot exceed the combined stdout and stderr output bound", async () => {
    await withStub({ commit: integer(12) }, async (runtime) => { await assert.rejects(runtime.open(open), /more rows than one frame may hold/); });
  });

  test("a real guest cannot exceed the combined stdout and stderr output bound (extra: every bridge refusal is named)", async () => {
    for (const [status, reason] of [[2, /memory budget is spent/], [8, /layout this host was not built for/], [99, /unknown failure/]] as const) {
      await withStub({ commit: integer(status) }, async (runtime) => { await assert.rejects(runtime.open(open), reason); });
    }
  });

  test("fdstat on an ungranted descriptor returns EBADF instead of success", async () => {
    assert.strictEqual(await runProbe(probe(expect(descriptorCall("fd_fdstat_get", 3), ERRNO.badf))), 0);
  });

  test("fdstat on an ungranted descriptor returns EBADF instead of success (extra: high descriptors too)", async () => {
    assert.strictEqual(await runProbe(probe([4, 64, 255, 4096].flatMap((fd) => expect(descriptorCall("fd_fdstat_get", fd), ERRNO.badf)))), 0);
  });

  for (const operation of ["fd_seek", "fd_close", "fd_fdstat_set_flags"]) {
    test(`${operation} on an ungranted descriptor returns EBADF instead of success`, async () => {
      assert.strictEqual(await runProbe(probe(expect(descriptorCall(operation, 3), ERRNO.badf))), 0);
    });

    test(`${operation} on an ungranted descriptor returns EBADF instead of success (extra: repeated calls on descriptor 255)`, async () => {
      assert.strictEqual(await runProbe(probe([0, 1, 2].flatMap(() => expect(descriptorCall(operation, 255), ERRNO.badf)))), 0);
    });
  }

  test("invalid and wrong-direction writes return EBADF without emitting bytes", async () => {
    for (const fd of [3, 255]) {
      const checks = [...store8(64, 65), ...iovec(0, 64, 1), ...store32(16, integer(99)),
        ...expect(wasi("fd_write", integer(fd), integer(0), integer(1), integer(16)), ERRNO.badf), ...expect(load32(16), 99)];
      assert.strictEqual(await runProbe(probe(checks)), 0, `fd ${fd}`);
    }
  });

  test("invalid and wrong-direction writes return EBADF without emitting bytes (extra: reads from ungranted descriptors fill nothing)", async () => {
    const checks = [...store8(64, 65), ...iovec(0, 64, 1), ...expect(wasi("fd_read", integer(3), integer(0), integer(1), integer(16)), ERRNO.badf), ...expect(load8(64), 65)];
    assert.strictEqual(await runProbe(probe(checks)), 0);
  });

  test("closed descriptors stay closed for metadata, flags, streams and a second close", async () => {
    for (const fd of [3, 4, 255]) {
      const checks = [
        ...expect(descriptorCall("fd_close", fd), ERRNO.badf), ...expect(descriptorCall("fd_close", fd), ERRNO.badf),
        ...expect(descriptorCall("fd_fdstat_get", fd), ERRNO.badf), ...expect(descriptorCall("fd_fdstat_set_flags", fd), ERRNO.badf),
        ...expect(wasi("fd_read", integer(fd), integer(0), integer(0), integer(64)), ERRNO.badf),
      ];
      assert.strictEqual(await runProbe(probe(checks)), 0, `fd ${fd}`);
    }
  });

  test("closed descriptors stay closed for metadata, flags, streams and a second close (extra: each run owns fresh descriptors)", async () => {
    const bytes = probe([...expect(descriptorCall("fd_close", 3), ERRNO.badf), ...expect(descriptorCall("fd_fdstat_get", 3), ERRNO.badf)]);
    for (let run = 0; run < 3; run += 1) assert.strictEqual(await runProbe(bytes), 0, `run ${run}`);
  });

  test("seeking a stream returns ESPIPE and leaves the result pointer unchanged", async () => {
    const checks = [3, 255].flatMap((fd) => [...store32(64, integer(123)), ...expect(descriptorCall("fd_seek", fd), ERRNO.badf), ...expect(load32(64), 123)]);
    assert.strictEqual(await runProbe(probe(checks)), 0);
  });

  test("seeking a stream returns ESPIPE and leaves the result pointer unchanged (extra: every whence is refused alike)", async () => {
    const checks = [0, 1, 2].flatMap((whence) => [...store32(64, integer(7)),
      ...expect(wasi("fd_seek", integer(3), wide(5), integer(whence), integer(64)), ERRNO.badf), ...expect(load32(64), 7)]);
    assert.strictEqual(await runProbe(probe(checks)), 0);
  });

  test("descriptor metadata reports actual stream rights and preserves only valid flags", async () => {
    const rights = (bit: number) => expect([...integer(64 + 8), 0x29, 3, 0, ...wide(bit), 0x83, ...wide(0), 0x52], 1);
    const checks = [...ok("fd_fdstat_get", integer(0), integer(64)), ...rights(2), ...ok("fd_fdstat_get", integer(1), integer(64)), ...rights(64)];
    assert.strictEqual(await runProbe(probe(checks)), 0, "standard input may be read and standard output written");
  });

  test("descriptor metadata reports actual stream rights and preserves only valid flags (extra: standard error is writable)", async () => {
    const checks = [...ok("fd_fdstat_get", integer(2), integer(64)), ...expect([...integer(72), 0x29, 3, 0, ...wide(64), 0x83, ...wide(0), 0x52], 1)];
    assert.strictEqual(await runProbe(probe(checks)), 0);
  });
});

/** A frame of one label whose words are "ok" followed by `bytes`. */
function malformedLabel(bytes: readonly number[]) {
  const built = frame({ views: [{ kind: 2, words: "ok" + "x".repeat(bytes.length) }] });
  const text = new Uint8Array(built.text.slice(0));
  text.set(bytes, 2);
  return { shapes: built.shapes, views: built.views, text: text.buffer };
}
