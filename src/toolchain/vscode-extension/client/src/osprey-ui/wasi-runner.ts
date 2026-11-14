// One worker thread per panel holds the live application instance ([NATIVE-SHM-WASM]): its
// `_start` ran once, and every later call is an export of the same module. The worker copies each
// call's four runs out of linear memory; the extension host transfers those copies to the webview.

import * as path from "node:path";
import { Worker } from "node:worker_threads";

/** One call's runs, copied out of the application's linear memory. */
export interface NativeRuns {
  readonly shapes: ArrayBuffer;
  readonly views: ArrayBuffer;
  readonly text: ArrayBuffer;
  readonly asks: ArrayBuffer;
}

/** One call's answer and the whole frame it drew. */
export interface FrameResult {
  readonly status: number;
  readonly revision: number;
  readonly runs: NativeRuns;
}

/** Host-held slices written since the last call, which the worker keeps for `native_slice`. */
export type SliceDelta = readonly (readonly [string, string])[];

export type WorkerRequest =
  | { readonly id: number; readonly call: "open"; readonly now: number; readonly offset: number;
      readonly platform: string; readonly home: string; readonly dark: boolean; readonly arguments: readonly string[];
      readonly slices: SliceDelta }
  | { readonly id: number; readonly call: "event"; readonly now: number; readonly kind: number;
      readonly ordinal: number; readonly words: string; readonly detail: string; readonly x: number;
      readonly y: number; readonly down: boolean; readonly shift: boolean; readonly slices: SliceDelta };

export interface WorkerReply {
  readonly id: number;
  readonly result?: FrameResult;
  readonly error?: string;
}

export interface OpenOptions {
  readonly now: number;
  readonly offset: number;
  readonly platform: string;
  readonly home: string;
  readonly dark: boolean;
  readonly arguments: readonly string[];
}

/** The typed host event a call delivers ([NATIVE-SHM-EVENT]). */
export interface NativeTouch {
  readonly kind: number;
  readonly ordinal: number;
  readonly words: string;
  readonly detail: string;
  readonly x: number;
  readonly y: number;
  readonly down: boolean;
  readonly shift: boolean;
}

/** A live application, called one event at a time. */
export interface NativeRuntime {
  open(options: OpenOptions): Promise<FrameResult>;
  event(touch: NativeTouch): Promise<FrameResult>;
  /** Holds `text` under the host-held slice `key`; the application reads it from its next call on. */
  hold(key: string, text: string): void;
  dispose(): void;
}

const CALL_TIMEOUT = 60000;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isBuffer(value: unknown): value is ArrayBuffer {
  return value instanceof ArrayBuffer;
}

function resultOf(reply: unknown): FrameResult {
  if (!isRecord(reply)) throw new Error("The application worker answered with no result");
  if (typeof reply.error === "string") throw new Error(reply.error);
  const result = reply.result;
  if (!isRecord(result) || typeof result.status !== "number" || typeof result.revision !== "number"
    || !isRecord(result.runs)) {
    throw new Error("The application worker answered with a malformed result");
  }
  const runs = result.runs;
  if (!isBuffer(runs.shapes) || !isBuffer(runs.views) || !isBuffer(runs.text) || !isBuffer(runs.asks)) {
    throw new Error("The application worker answered with malformed runs");
  }
  return { status: result.status, revision: result.revision,
    runs: { shapes: runs.shapes, views: runs.views, text: runs.text, asks: runs.asks } };
}

/** Start the one application instance this runtime owns. */
export function startNativeRuntime(module: WebAssembly.Module, signal?: AbortSignal): NativeRuntime {
  if (signal?.aborted) throw new Error("The application was cancelled before it started");
  const worker = new Worker(path.join(__dirname, "wasi-worker.js"), { execArgv: [], workerData: { module } });
  const pending = new Map<number, { resolve(result: FrameResult): void; reject(error: Error): void; timer: NodeJS.Timeout }>();
  let nextId = 1;
  let disposed = false;
  /** Why the worker stopped, once it has: every later call is refused with it at once. */
  let dead: Error | undefined;

  const fail = (error: Error): void => {
    for (const entry of pending.values()) {
      clearTimeout(entry.timer);
      entry.reject(error);
    }
    pending.clear();
  };
  const cancel = (): void => {
    fail(new Error("The application was cancelled"));
    void worker.terminate();
  };
  worker.on("message", (reply: unknown) => {
    if (!isRecord(reply) || typeof reply.id !== "number") return;
    const entry = pending.get(reply.id);
    if (entry === undefined) return;
    pending.delete(reply.id);
    clearTimeout(entry.timer);
    try {
      entry.resolve(resultOf(reply));
    } catch (error) {
      entry.reject(error instanceof Error ? error : new Error(String(error)));
    }
  });
  worker.on("error", (error: Error) => {
    dead ??= error;
    fail(error);
  });
  worker.on("exit", (code: number) => {
    dead ??= new Error(`The application worker exited with ${code}`);
    if (!disposed) fail(dead);
  });
  signal?.addEventListener("abort", cancel, { once: true });
  if (signal?.aborted) cancel();

  const send = (request: WorkerRequest): Promise<FrameResult> => new Promise((resolve, reject) => {
    if (disposed || dead !== undefined) {
      reject(disposed ? new Error("The application is disposed") : dead);
      return;
    }
    const timer = setTimeout(() => {
      pending.delete(request.id);
      reject(new Error("The application did not answer its call in time"));
      void worker.terminate();
    }, CALL_TIMEOUT);
    pending.set(request.id, { resolve, reject, timer });
    worker.postMessage(request);
  });

  let held: (readonly [string, string])[] = [];
  const delta = (): SliceDelta => held.splice(0);

  return {
    open(options) {
      const id = nextId;
      nextId += 1;
      return send({ id, call: "open", ...options, slices: delta() });
    },
    event(touch) {
      const id = nextId;
      nextId += 1;
      return send({ id, call: "event", now: Math.floor(Date.now() / 1000), ...touch, slices: delta() });
    },
    hold(key, text) {
      held = [...held, [key, text] as const];
    },
    dispose() {
      if (disposed) return;
      disposed = true;
      signal?.removeEventListener("abort", cancel);
      fail(new Error("The application is disposed"));
      void worker.terminate();
    },
  };
}
