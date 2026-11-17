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

type WasiCall = (...args: unknown[]) => unknown;

/** Node's preview1 host hands a guest the embedder's own descriptors 0-2; none is granted. */
const STANDARD_STREAMS = 3;
const EBADF = 8;
/** preview1 subscription layout: 48 bytes each, tag at 8, the descriptor at 16 for tags 1 and 2. */
const SUBSCRIPTION = { size: 48, tag: 8, fd: 16 } as const;

const standard = (fd: unknown): boolean => typeof fd === "number" && fd >>> 0 < STANDARD_STREAMS;

/** Does `poll_oneoff` subscribe to a standard stream's readiness? */
function pollsStandard(memory: ArrayBuffer | undefined, subscriptions: unknown, count: unknown): boolean {
  if (memory === undefined || typeof subscriptions !== "number" || typeof count !== "number") return true;
  const view = new DataView(memory);
  const end = (subscriptions >>> 0) + (count >>> 0) * SUBSCRIPTION.size;
  if (end > view.byteLength) return false; // out of bounds: the host itself refuses it
  for (let at = subscriptions >>> 0; at < end; at += SUBSCRIPTION.size) {
    if (view.getUint8(at + SUBSCRIPTION.tag) !== 0 && standard(view.getUint32(at + SUBSCRIPTION.fd, true))) return true;
  }
  return false;
}

/** Whether `name` called with `args` names a standard stream. */
function refused(name: string, args: unknown[], memory: () => ArrayBuffer | undefined): boolean {
  // Windows uvwasi returns ENOSYS before checking the descriptor. Workspace guests have no descriptors.
  if (name === "fd_fdstat_set_flags") return true;
  if (name === "poll_oneoff") return pollsStandard(memory(), args[0], args[2]);
  if (name === "fd_renumber") return standard(args[0]) || standard(args[1]);
  return /^(fd|path|sock)_/.test(name) && standard(args[0]);
}

/**
 * The single import object an Osprey workspace module may receive ([NATIVE-SHM-WASM]): every call
 * that names a standard stream is refused, so a guest never reads the extension host's input or
 * writes into its output. `memory` is the instance's linear memory once it exists.
 */
export function wasiImports(wasi: WASI, memory: () => ArrayBuffer | undefined): WebAssembly.Imports {
  const calls = Object.entries(wasi.wasiImport as Record<string, WasiCall>).map(([name, call]): [string, WasiCall] =>
    [name, (...args) => refused(name, args, memory) ? EBADF : call(...args)]);
  return { wasi_snapshot_preview1: Object.fromEntries(calls) };
}
