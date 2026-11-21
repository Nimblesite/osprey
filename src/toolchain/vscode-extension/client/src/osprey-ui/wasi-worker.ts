// The one application instance a panel owns, on a Node worker thread: Node's WASI host and its
// random bytes run `instance.ts`, and each message is one boundary call whose four runs are
// transferred back. The extension host owns the serialization boundary ([NATIVE-SHM-WASM]).

import { randomFillSync } from "node:crypto";
import { parentPort, workerData, type MessagePort } from "node:worker_threads";
import { startInstance } from "./instance";
import { createWasi, wasiImports } from "./wasi";
import type { WorkerRequest } from "./wasi-runner";

function record(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function start(module: WebAssembly.Module, port: MessagePort): void {
  const wasi = createWasi();
  const call = startInstance(module, {
    imports: (memory) => wasiImports(wasi, memory),
    start: (instance) => wasi.start(instance),
    random: (bytes) => { randomFillSync(bytes); },
  });
  port.on("message", (raw: unknown) => {
    if (!record(raw) || typeof raw.id !== "number" || (raw.call !== "open" && raw.call !== "event")) return;
    const request = raw as unknown as WorkerRequest;
    try {
      const result = call(request);
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
