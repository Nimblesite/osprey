import * as path from "node:path";
import { Worker } from "node:worker_threads";
import type { WasiOptions, WasiResult } from "./wasi";

function resultOf(code: number, reply: unknown, failure: unknown): WasiResult {
  if (failure) throw failure;
  if (code !== 0) throw new Error(`WASI command worker exited with ${code}`);
  if (!reply || typeof reply !== "object") throw new Error("WASI command worker exited without a result");
  if ("error" in reply && typeof reply.error === "string") throw new Error(reply.error);
  const result = "result" in reply ? reply.result : undefined;
  if (!result || typeof result !== "object" || !("stdout" in result) || typeof result.stdout !== "string"
    || !("stderr" in result) || typeof result.stderr !== "string" || !("exitCode" in result) || !Number.isInteger(result.exitCode)) {
    throw new Error("WASI command worker returned an invalid result");
  }
  return result as WasiResult;
}

function completion(worker: Worker, signal: AbortSignal | undefined): Promise<WasiResult> {
  return new Promise((resolve, reject) => {
    let reply: unknown, failure: unknown;
    const cancel = (): void => { failure = new Error("WASI command cancelled"); void worker.terminate().catch(reject); };
    worker.once("message", message => { reply = message; });
    worker.once("error", error => { failure = error; });
    worker.once("exit", code => {
      signal?.removeEventListener("abort", cancel);
      try { resolve(resultOf(code, reply, failure)); } catch (error) { reject(error); }
    });
    signal?.addEventListener("abort", cancel, { once: true });
    if (signal?.aborted) cancel();
  });
}

export function runInWorker(module: WebAssembly.Module, input: string, options: WasiOptions): Promise<WasiResult> {
  if (options.signal?.aborted) return Promise.reject(new Error("WASI command cancelled"));
  const worker = new Worker(path.join(__dirname, "wasi-worker.js"), {
    execArgv: [], workerData: { module, input, options: { args: options.args, env: options.env } },
  });
  return completion(worker, options.signal);
}
