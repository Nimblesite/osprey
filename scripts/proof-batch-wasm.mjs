// File-capability host for the shared batch checker [PROOF-RUNNABLE-LIBRARY].
import { readFile } from "node:fs/promises";
import { resolve } from "node:path";
import { WASI } from "node:wasi";
import { ensureWasiCapableNode } from "./wasm-smoke-support.mjs";

ensureWasiCapableNode(import.meta.url);
const [modulePath, fixtureDirectory] = process.argv.slice(2);
if (!modulePath || !fixtureDirectory) {
  throw new Error("usage: proof-batch-wasm.mjs <checker.wasm> <fixture-directory>");
}
const bytes = await readFile(modulePath);
if (!WebAssembly.validate(bytes)) throw new Error("invalid checker WebAssembly");
const wasi = new WASI({
  version: "preview1",
  args: [modulePath],
  env: {},
  preopens: { "/": resolve(fixtureDirectory), ".": resolve(fixtureDirectory) },
  returnOnExit: true,
});
const { instance } = await WebAssembly.instantiate(bytes, {
  wasi_snapshot_preview1: wasi.wasiImport,
});
process.exitCode = wasi.start(instance);
