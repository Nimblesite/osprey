import { parentPort, workerData } from "node:worker_threads";
import { runWasiInline } from "./wasi";
import { messageOf } from "./wire";

const port = parentPort;
if (!port) throw new Error("The WASI command entry requires a worker port");

void runWasiInline(workerData.module, workerData.input, workerData.options).then(
  result => port.postMessage({ result }),
  error => port.postMessage({ error: messageOf(error) }),
).finally(() => port.close());
