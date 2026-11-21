// One live application instance on any JavaScript host ([NATIVE-SHM-WASM], [NATIVE-SHM-PUBLISH]):
// `_start` runs the application's top level once, then every request is one boundary export,
// sealed and leased from the shared bridge, its four runs copied out of linear memory. The
// Node worker behind the VS Code panel and the browser worker behind the web host both run this;
// only their WASI host and their source of randomness differ.

import { SHARED_FINGERPRINT } from "./generated/native-abi.js";
import type { FrameResult, NativeRuns, WorkerRequest } from "./wasi-runner";

/** What a host gives the instance: its WASI imports and start, and random bytes. */
export interface GuestHost {
  imports(memory: () => ArrayBuffer | undefined): WebAssembly.Imports;
  start(instance: WebAssembly.Instance): number;
  random(bytes: Uint8Array): void;
}

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

const REQUIRED: readonly (keyof BoundaryExports)[] = ["memory", "_start", "malloc", "free", "osprey_app_argument",
  "osprey_app_open", "osprey_app_event", "osp_native_open", "osp_native_frame_commit", "osp_native_acquire",
  "osp_native_child", "osp_native_length", "osp_native_view", "osp_native_release"];

const LEASE_BYTES = 64;
/** Everything the bridge may hold at once, and the chunk and owner budgets. */
const BYTES = 64n << 20n;
const CHUNKS = 256n;
const OWNERS = 256n;

function boundary(instance: WebAssembly.Instance): BoundaryExports {
  const exports = instance.exports as unknown as BoundaryExports;
  for (const name of REQUIRED) {
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

/** Linear-memory helpers over one instance: allocation and the call's borrowed strings. */
class Memory {
  private readonly strings: number[] = [];
  constructor(readonly api: BoundaryExports) {}

  view(): DataView { return new DataView(this.api.memory.buffer); }

  allocate(size: number): number {
    const pointer = this.api.malloc(size);
    if (pointer === 0) throw new Error("The application could not allocate host text");
    return pointer;
  }

  /** `value` as a NUL-terminated string the application borrows until `release`. */
  text(value: string): number {
    const bytes = new TextEncoder().encode(`${value}\0`);
    const pointer = this.allocate(bytes.length);
    new Uint8Array(this.api.memory.buffer, pointer, bytes.length).set(bytes);
    this.strings.push(pointer);
    return pointer;
  }

  release(): void {
    for (const pointer of this.strings.splice(0)) this.api.free(pointer);
  }
}

/** [NATIVE-SHM-WASM] Host-held slices, answered synchronously during a call; the C ABI copies the
 * text before `native_slice` returns, so each answer's buffer is freed at the next one. */
class HeldSlices {
  readonly held = new Map<string, string>();
  private answered = 0;
  live: BoundaryExports | undefined;

  answer(key: number): number {
    const live = this.live;
    if (live === undefined) return 0;
    if (this.answered !== 0) live.free(this.answered);
    const memoryBytes = new Uint8Array(live.memory.buffer);
    let end = key;
    while (end < memoryBytes.length && memoryBytes[end] !== 0) end += 1;
    const name = new TextDecoder("utf-8", { fatal: true }).decode(memoryBytes.subarray(key, end));
    const bytes = new TextEncoder().encode(`${this.held.get(name) ?? ""}\0`);
    this.answered = live.malloc(bytes.length);
    if (this.answered === 0) throw new Error("The application could not allocate a host slice");
    new Uint8Array(live.memory.buffer, this.answered, bytes.length).set(bytes);
    return this.answered;
  }
}

function openBridge(memory: Memory, host: GuestHost): number {
  const instanceId = memory.allocate(16);
  host.random(new Uint8Array(memory.api.memory.buffer, instanceId, 16));
  const fingerprint = memory.text(SHARED_FINGERPRINT);
  const bridgeOut = memory.allocate(8);
  const opened = memory.api.osp_native_open(instanceId, fingerprint, BYTES, CHUNKS, OWNERS, bridgeOut);
  memory.release();
  memory.api.free(instanceId);
  const bridge = memory.view().getUint32(bridgeOut, true);
  memory.api.free(bridgeOut);
  checked(opened);
  if (bridge === 0) throw new Error("The shared bridge published no bridge");
  return bridge;
}

/** Run `index` of the leased frame `root`, copied out of linear memory. */
function copyRun(memory: Memory, bridge: number, index: number, root: number): ArrayBuffer {
  const api = memory.api;
  const lease = memory.allocate(LEASE_BYTES);
  const length = memory.allocate(8);
  const view = memory.allocate(4);
  let owned = false;
  try {
    checked(api.osp_native_child(bridge, root, BigInt(index), lease));
    owned = true;
    checked(api.osp_native_length(bridge, lease, length));
    const bytes = Number(memory.view().getBigInt64(length, true));
    if (!Number.isSafeInteger(bytes) || bytes < 0) throw new Error("The bridge reported an unreadable run length");
    if (bytes === 0) return new ArrayBuffer(0);
    checked(api.osp_native_view(bridge, lease, BigInt(index + 1), 0n, BigInt(bytes), view));
    const at = memory.view().getUint32(view, true);
    if (at === 0 || at + bytes > api.memory.buffer.byteLength) throw new Error("The bridge reported a run outside linear memory");
    return new Uint8Array(api.memory.buffer, at, bytes).slice().buffer;
  } finally {
    if (owned) checked(api.osp_native_release(bridge, lease));
    for (const pointer of [lease, length, view]) api.free(pointer);
  }
}

/** The call's frame, committed, leased and copied. */
function sealed(memory: Memory, bridge: number, status: number): FrameResult {
  const api = memory.api;
  const revision = memory.allocate(8);
  const root = memory.allocate(LEASE_BYTES);
  const acquired = memory.allocate(8);
  let owned = false;
  try {
    checked(api.osp_native_frame_commit(bridge, revision));
    checked(api.osp_native_acquire(bridge, root, acquired));
    owned = true;
    const run = (index: number): ArrayBuffer => copyRun(memory, bridge, index, root);
    const runs: NativeRuns = { shapes: run(0), views: run(1), text: run(2), asks: run(3) };
    return { status, revision: Number(memory.view().getBigInt64(acquired, true)), runs };
  } finally {
    if (owned) checked(api.osp_native_release(bridge, root));
    for (const pointer of [revision, root, acquired]) api.free(pointer);
  }
}

/** A borrowed-string call whose strings are freed once it returns. */
function borrowing<T>(memory: Memory, call: () => T): T {
  try { return call(); } finally { memory.release(); }
}

function opened(memory: Memory, bridge: number, request: Extract<WorkerRequest, { call: "open" }>): FrameResult {
  const api = memory.api;
  for (const argument of request.arguments) {
    borrowing(memory, () => sealed(memory, bridge, Number(api.osprey_app_argument(memory.text(argument)))));
  }
  return borrowing(memory, () => sealed(memory, bridge, Number(api.osprey_app_open(BigInt(request.now),
    BigInt(request.offset), memory.text(request.platform), memory.text(request.home), request.dark ? 1 : 0))));
}

function evented(memory: Memory, bridge: number, request: Extract<WorkerRequest, { call: "event" }>): FrameResult {
  return borrowing(memory, () => sealed(memory, bridge, Number(memory.api.osprey_app_event(BigInt(request.now),
    BigInt(request.kind), BigInt(request.ordinal), memory.text(request.words), memory.text(request.detail),
    request.x, request.y, request.down ? 1 : 0, request.shift ? 1 : 0))));
}

/** Instantiate `module`, run its top level once and answer each request with its frame. */
export function startInstance(module: WebAssembly.Module, host: GuestHost): (request: WorkerRequest) => FrameResult {
  const slices = new HeldSlices();
  const instance = new WebAssembly.Instance(module, { ...host.imports(() => slices.live?.memory.buffer),
    env: { native_slice: (key: number) => slices.answer(key) } });
  const api = boundary(instance);
  slices.live = api;
  const started = host.start(instance);
  if (started !== 0) throw new Error(`The application's _start returned ${started}`);
  const memory = new Memory(api);
  const bridge = openBridge(memory, host);
  return (request) => {
    for (const [key, value] of request.slices) slices.held.set(key, value);
    return request.call === "open" ? opened(memory, bridge, request) : evented(memory, bridge, request);
  };
}
