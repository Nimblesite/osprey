// The one application instance a panel owns: `_start` runs the application's top level once,
// then every message is one export call, sealed and leased from the shared bridge. The worker
// copies the four runs out of linear memory and transfers them; the extension host owns the
// serialization boundary ([NATIVE-SHM-WASM], [NATIVE-SHM-PUBLISH]).

import { randomFillSync } from "node:crypto";
import { parentPort, workerData, type MessagePort } from "node:worker_threads";
import { SHARED_FINGERPRINT } from "./generated/native-abi";
import { createWasi, wasiImports } from "./wasi";
import type { FrameResult, NativeRuns, WorkerRequest } from "./wasi-runner";

interface BoundaryExports {
  readonly memory: WebAssembly.Memory;
  readonly _start: WebAssembly.ExportValue;
  readonly malloc: (size: number) => number;
  readonly free: (pointer: number) => void;
  readonly osprey_app_argument: (text: number) => bigint;
  readonly osprey_app_open: (now: bigint, offset: bigint, platform: number, home: number, dark: number) => bigint;
  readonly osprey_app_event: (now: bigint, kind: bigint, ordinal: bigint, words: number, detail: number,
    x: number, y: number, down: number, shift: number) => bigint;
  readonly osp_native_open: (instance: number, fingerprint: number, bytes: bigint, chunks: bigint,
    owners: bigint, out: number) => number;
  readonly osp_native_frame_commit: (bridge: number, revision: number) => number;
  readonly osp_native_acquire: (bridge: number, out: number, revision: number) => number;
  readonly osp_native_child: (bridge: number, lease: number, index: bigint, out: number) => number;
  readonly osp_native_length: (bridge: number, lease: number, length: number) => number;
  readonly osp_native_view: (bridge: number, lease: number, kind: bigint, offset: bigint, length: bigint, out: number) => number;
  readonly osp_native_release: (bridge: number, lease: number) => number;
}

const REFUSALS: readonly string[] = [
  "an invalid limit", "its memory budget is spent", "no free slot", "a lease from another application instance",
  "a stale allocation", "an invalid owner", "an invalid range", "a layout this host was not built for",
  "an exhausted counter", "the bridge is closing", "the bridge is closed",
  "the application sent more rows than one frame may hold", "a failed lock",
];

const LEASE_BYTES = 64;
/** Everything the bridge may hold at once, and the chunk and owner budgets. */
const BYTES = 64n << 20n;
const CHUNKS = 256n;
const OWNERS = 256n;

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function boundary(instance: WebAssembly.Instance): BoundaryExports {
  const exports = instance.exports as unknown as BoundaryExports;
  const required: readonly (keyof BoundaryExports)[] = ["memory", "_start", "malloc", "free", "osprey_app_argument",
    "osprey_app_open", "osprey_app_event", "osp_native_open", "osp_native_frame_commit", "osp_native_acquire",
    "osp_native_child", "osp_native_length", "osp_native_view", "osp_native_release"];
  for (const name of required) {
    const value: unknown = exports[name];
    if (!(name === "memory" ? value instanceof WebAssembly.Memory : typeof value === "function")) {
      throw new Error(`The application module does not export ${name}`);
    }
  }
  return exports;
}

function checked(status: number): void {
  if (status === 0) return;
  const reason = REFUSALS[status - 1] ?? "an unknown failure";
  throw new Error(`The shared bridge refused the frame: ${reason}`);
}

function start(module: WebAssembly.Module, port: MessagePort): void {
  const wasi = createWasi();
  // [NATIVE-SHM-WASM] Host-held slices, answered synchronously during a call; the C ABI copies the
  // text before `native_slice` returns, so each answer's buffer is freed at the next one.
  const held = new Map<string, string>();
  let answered = 0;
  let live: BoundaryExports | undefined;
  const nativeSlice = (key: number): number => {
    if (live === undefined) return 0;
    if (answered !== 0) live.free(answered);
    const memoryBytes = new Uint8Array(live.memory.buffer);
    let end = key;
    while (end < memoryBytes.length && memoryBytes[end] !== 0) end += 1;
    const name = new TextDecoder("utf-8", { fatal: true }).decode(memoryBytes.subarray(key, end));
    const bytes = new TextEncoder().encode(`${held.get(name) ?? ""}\0`);
    answered = live.malloc(bytes.length);
    if (answered === 0) throw new Error("The application could not allocate a host slice");
    new Uint8Array(live.memory.buffer, answered, bytes.length).set(bytes);
    return answered;
  };
  const instance = new WebAssembly.Instance(module, { ...wasiImports(wasi), env: { native_slice: nativeSlice } });
  const api = boundary(instance);
  live = api;
  const started = wasi.start(instance);
  if (started !== 0) throw new Error(`The application's _start returned ${started}`);

  const memory = (): DataView => new DataView(api.memory.buffer);
  const allocate = (size: number): number => {
    const pointer = api.malloc(size);
    if (pointer === 0) throw new Error("The application could not allocate host text");
    return pointer;
  };
  const strings: number[] = [];
  const text = (value: string): number => {
    const bytes = new TextEncoder().encode(`${value}\0`);
    const pointer = allocate(bytes.length);
    new Uint8Array(api.memory.buffer, pointer, bytes.length).set(bytes);
    strings.push(pointer);
    return pointer;
  };
  const freeText = (): void => {
    for (const pointer of strings.splice(0)) api.free(pointer);
  };

  const instanceId = allocate(16);
  randomFillSync(new Uint8Array(api.memory.buffer, instanceId, 16));
  const fingerprint = text(SHARED_FINGERPRINT);
  const bridgeOut = allocate(8);
  const opened = api.osp_native_open(instanceId, fingerprint, BYTES, CHUNKS, OWNERS, bridgeOut);
  freeText();
  api.free(instanceId);
  const bridge = memory().getUint32(bridgeOut, true);
  api.free(bridgeOut);
  checked(opened);
  if (bridge === 0) throw new Error("The shared bridge published no bridge");

  const run = (index: number, root: number): ArrayBuffer => {
    const lease = allocate(LEASE_BYTES);
    const length = allocate(8);
    const view = allocate(4);
    let owned = false;
    try {
      checked(api.osp_native_child(bridge, root, BigInt(index), lease));
      owned = true;
      checked(api.osp_native_length(bridge, lease, length));
      const bytes = Number(memory().getBigInt64(length, true));
      if (!Number.isSafeInteger(bytes) || bytes < 0) throw new Error("The bridge reported an unreadable run length");
      if (bytes === 0) return new ArrayBuffer(0);
      checked(api.osp_native_view(bridge, lease, BigInt(index + 1), 0n, BigInt(bytes), view));
      const at = memory().getUint32(view, true);
      const size = api.memory.buffer.byteLength;
      if (at === 0 || at + bytes > size) throw new Error("The bridge reported a run outside linear memory");
      return new Uint8Array(api.memory.buffer, at, bytes).slice().buffer;
    } finally {
      if (owned) checked(api.osp_native_release(bridge, lease));
      api.free(lease);
      api.free(length);
      api.free(view);
    }
  };

  const sealed = (status: number): FrameResult => {
    const revision = allocate(8);
    try {
      checked(api.osp_native_frame_commit(bridge, revision));
      const root = allocate(LEASE_BYTES);
      const acquired = allocate(8);
      let owned = false;
      try {
        checked(api.osp_native_acquire(bridge, root, acquired));
        owned = true;
        const runs: NativeRuns = { shapes: run(0, root), views: run(1, root), text: run(2, root), asks: run(3, root) };
        return { status, revision: Number(memory().getBigInt64(acquired, true)), runs };
      } finally {
        if (owned) checked(api.osp_native_release(bridge, root));
        api.free(root);
        api.free(acquired);
      }
    } finally {
      api.free(revision);
    }
  };

  const open = (request: Extract<WorkerRequest, { call: "open" }>): FrameResult => {
    for (const argument of request.arguments) {
      try {
        const status = Number(api.osprey_app_argument(text(argument)));
        sealed(status);
      } finally {
        freeText();
      }
    }
    const platform = text(request.platform);
    const home = text(request.home);
    try {
      const status = Number(api.osprey_app_open(BigInt(request.now), BigInt(request.offset), platform, home, request.dark ? 1 : 0));
      return sealed(status);
    } finally {
      freeText();
    }
  };

  const event = (request: Extract<WorkerRequest, { call: "event" }>): FrameResult => {
    const words = text(request.words);
    const detail = text(request.detail);
    try {
      const status = Number(api.osprey_app_event(BigInt(request.now), BigInt(request.kind), BigInt(request.ordinal),
        words, detail, request.x, request.y, request.down ? 1 : 0, request.shift ? 1 : 0));
      return sealed(status);
    } finally {
      freeText();
    }
  };

  port.on("message", (raw: unknown) => {
    if (!record(raw) || typeof raw.id !== "number" || (raw.call !== "open" && raw.call !== "event")) return;
    const request = raw as unknown as WorkerRequest;
    try {
      for (const [key, value] of request.slices) held.set(key, value);
      const result = request.call === "open" ? open(request) : event(request);
      port.postMessage({ id: request.id, result }, [result.runs.shapes, result.runs.views, result.runs.text, result.runs.asks]);
    } catch (error) {
      port.postMessage({ id: request.id, error: error instanceof Error ? error.message : String(error) });
    }
  });
}

// Nothing runs without the private port the extension host opened: no module, no `_start`.
if (!parentPort) throw new Error("The application entry requires a worker port");
const data: unknown = workerData;
if (!record(data) || !(data.module instanceof WebAssembly.Module)) throw new Error("The application entry requires a module");
start(data.module, parentPort);
