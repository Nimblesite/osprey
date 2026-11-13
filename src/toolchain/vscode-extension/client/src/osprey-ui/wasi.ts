/// <reference lib="dom" />
// A long-lived WebAssembly module's WASI host. The application's top level runs once through
// `_start`; the instance is then kept, and every later call is one of the module's own exports
// ([NATIVE-SHM-WASM]). No standard input, output or filesystem is granted.

import { WASI } from "node:wasi";

export interface WasiOptions {
  readonly args?: readonly string[];
  readonly env?: Readonly<Record<string, string>>;
}

/** Compile an Osprey workspace module; a malformed module fails here, before any panel opens. */
export function compileWasi(bytes: Uint8Array): Promise<WebAssembly.Module> {
  const copy = new ArrayBuffer(bytes.byteLength);
  new Uint8Array(copy).set(bytes);
  return WebAssembly.compile(copy);
}

/** The preview1 host and its import object; the caller instantiates against it once. */
export function createWasi(options: WasiOptions = {}): WASI {
  return new WASI({ version: "preview1", args: [...(options.args ?? [])], env: { ...(options.env ?? {}) }, returnOnExit: true });
}

/** The single import object an Osprey workspace module may receive. */
export function wasiImports(wasi: WASI): WebAssembly.Imports {
  return { wasi_snapshot_preview1: wasi.wasiImport };
}
