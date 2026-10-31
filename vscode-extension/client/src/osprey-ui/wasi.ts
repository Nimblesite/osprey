/// <reference lib="dom" />
// A bounded, in-memory WASI preview1 command host for the Osprey workspace
// module. The extension never writes a temporary stdin/stdout file and never
// grants a filesystem: stdin is the frame string and stdout is one envelope.

import { randomFillSync } from "crypto";
import { runInWorker } from "./wasi-runner";

const ESUCCESS = 0;
const EBADF = 8;
const EINVAL = 28;
const ESPIPE = 70;
const STREAM_FLAGS = 31;
const RIGHT_FD_READ = 1n << 1n;
const RIGHT_FD_SET_FLAGS = 1n << 3n;
const RIGHT_FD_WRITE = 1n << 6n;
const MAX_OUTPUT = 8 * 1024 * 1024;
const MAX_IOVECS = 1024;
const MAX_ARGUMENTS = 64;
const MAX_ARGUMENT = 32 * 1024;

export interface WasiResult {
  readonly stdout: string;
  readonly stderr: string;
  readonly exitCode: number;
}

export interface WasiOptions {
  readonly args?: readonly string[];
  readonly env?: Readonly<Record<string, string>>;
  readonly signal?: AbortSignal;
}

/** The WASI functions this host implements; a module importing any other is rejected. */
const IMPLEMENTED = new Set([
  "args_get",
  "args_sizes_get",
  "clock_res_get",
  "clock_time_get",
  "environ_get",
  "environ_sizes_get",
  "fd_close",
  "fd_prestat_get",
  "fd_prestat_dir_name",
  "fd_fdstat_get",
  "fd_fdstat_set_flags",
  "fd_read",
  "fd_seek",
  "fd_write",
  "proc_exit",
  "random_get",
  "sched_yield",
]);

class WasiExit extends Error {
  constructor(readonly code: number) {
    super(`WASI command exited with ${code}`);
  }
}

interface WasiState {
  memory?: WebAssembly.Memory;
  readonly descriptors: Map<number, number>;
  cursor: number;
  readonly stdin: Uint8Array;
  readonly stdout: Buffer[];
  readonly stderr: Buffer[];
  readonly args: readonly string[];
  readonly environ: readonly string[];
  bytes: number;
}

function view(state: WasiState): DataView {
  if (!state.memory) throw new Error("WASI module has no exported memory");
  return new DataView(state.memory.buffer);
}

function bytes(state: WasiState): Uint8Array {
  if (!state.memory) throw new Error("WASI module has no exported memory");
  return new Uint8Array(state.memory.buffer);
}

function span(state: WasiState, ptr: number, length: number, what: string): Uint8Array {
  const memory = bytes(state);
  if (!Number.isInteger(ptr) || !Number.isInteger(length) || ptr < 0 || length < 0 || ptr + length > memory.length) {
    throw new Error(`WASI ${what} exceeds the module's memory`);
  }
  return memory.subarray(ptr, ptr + length);
}

function write(state: WasiState, ptr: number, length: number, what: string): Uint8Array {
  const target = span(state, ptr, length, what);
  target.fill(0);
  return target;
}

function append(state: WasiState, fd: number, chunk: Buffer): void {
  state.bytes += chunk.length;
  if (state.bytes > MAX_OUTPUT) throw new Error("WASI command output exceeds its host bound");
  if (fd === 2) state.stderr.push(chunk);
  else state.stdout.push(chunk);
}

function fdWrite(state: WasiState, fd: number, iovs: number, count: number, writtenPtr: number): number {
  if (!state.descriptors.has(fd) || (fd !== 1 && fd !== 2)) return EBADF;
  if (count > MAX_IOVECS) throw new Error("WASI fd_write received too many buffers");
  span(state, iovs, count * 8, "fd_write iovecs");
  const data = view(state);
  let written = 0;
  for (let index = 0; index < count; index += 1) {
    const ptr = data.getUint32(iovs + index * 8, true);
    const length = data.getUint32(iovs + index * 8 + 4, true);
    append(state, fd, Buffer.from(span(state, ptr, length, "fd_write buffer")));
    written += length;
  }
  write(state, writtenPtr, 4, "fd_write result");
  data.setUint32(writtenPtr, written, true);
  return ESUCCESS;
}

function fdRead(state: WasiState, fd: number, iovs: number, count: number, readPtr: number): number {
  if (!state.descriptors.has(fd) || fd !== 0) return EBADF;
  if (count > MAX_IOVECS) throw new Error("WASI fd_read received too many buffers");
  span(state, iovs, count * 8, "fd_read iovecs");
  write(state, readPtr, 4, "fd_read result");
  const data = view(state);
  let read = 0;
  for (let index = 0; index < count; index += 1) {
    const ptr = data.getUint32(iovs + index * 8, true);
    const length = data.getUint32(iovs + index * 8 + 4, true);
    const take = Math.max(0, Math.min(length, state.stdin.length - state.cursor));
    write(state, ptr, take, "fd_read buffer").set(state.stdin.subarray(state.cursor, state.cursor + take));
    state.cursor += take;
    read += take;
  }
  data.setUint32(readPtr, read, true);
  return ESUCCESS;
}

function descriptorStatus(state: WasiState, fd: number): number {
  return state.descriptors.has(fd) ? ESUCCESS : EBADF;
}

function setDescriptorFlags(state: WasiState, fd: number, flags: number): number {
  if (descriptorStatus(state, fd) !== ESUCCESS) return EBADF;
  if ((flags & ~STREAM_FLAGS) !== 0) return EINVAL;
  state.descriptors.set(fd, flags);
  return ESUCCESS;
}

function fdStat(state: WasiState, fd: number, ptr: number): number {
  if (descriptorStatus(state, fd) !== ESUCCESS) return EBADF;
  const data = view(state);
  write(state, ptr, 24, "fd_fdstat_get result");
  data.setUint8(ptr, 2);
  data.setUint8(ptr + 1, 0);
  data.setUint16(ptr + 2, state.descriptors.get(fd) ?? 0, true);
  data.setBigUint64(ptr + 8, RIGHT_FD_SET_FLAGS | (fd === 0 ? RIGHT_FD_READ : RIGHT_FD_WRITE), true);
  data.setBigUint64(ptr + 16, 0n, true);
  return ESUCCESS;
}

function clockTime(state: WasiState, _id: number, _precision: bigint, ptr: number): number {
  write(state, ptr, 8, "clock_time_get result");
  view(state).setBigUint64(ptr, BigInt(Date.now()) * 1000000n, true);
  return ESUCCESS;
}

function randomBytes(state: WasiState, ptr: number, length: number): number {
  randomFillSync(write(state, ptr, length, "random_get buffer"));
  return ESUCCESS;
}

function strings(state: WasiState, values: readonly string[], countPtr: number, sizePtr: number): number {
  const encoded = values.map((value) => Buffer.from(value, "utf8"));
  const total = encoded.reduce((sum, value) => sum + value.length + 1, 0);
  write(state, countPtr, 4, "string count");
  write(state, sizePtr, 4, "string size");
  const data = view(state);
  data.setUint32(countPtr, encoded.length, true);
  data.setUint32(sizePtr, total, true);
  return ESUCCESS;
}

function writeStrings(state: WasiState, values: readonly string[], arrayPtr: number, bufferPtr: number): number {
  const total = values.reduce((sum, value) => sum + Buffer.byteLength(value, "utf8") + 1, 0);
  write(state, arrayPtr, values.length * 4, "string pointer table");
  write(state, bufferPtr, total, "string table");
  const data = view(state);
  let cursor = bufferPtr;
  for (let index = 0; index < values.length; index += 1) {
    const encoded = Buffer.from(values[index] ?? "", "utf8");
    data.setUint32(arrayPtr + index * 4, cursor, true);
    bytes(state).set(encoded, cursor);
    cursor += encoded.length + 1;
  }
  return ESUCCESS;
}

function wasiImports(state: WasiState): WebAssembly.Imports["wasi_snapshot_preview1"] {
  const base: Record<string, (...args: never[]) => number> = {
    fd_close: (fd: never) => state.descriptors.delete(fd as number) ? ESUCCESS : EBADF,
    // No directory handles are granted; libc probes must receive EBADF.
    fd_prestat_get: () => EBADF,
    fd_prestat_dir_name: () => EBADF,
    fd_fdstat_get: (fd: never, ptr: never) => fdStat(state, fd as number, ptr as number),
    fd_fdstat_set_flags: (fd: never, flags: never) => setDescriptorFlags(state, fd as number, flags as number),
    fd_read: (fd: never, iovs: never, count: never, readPtr: never) => fdRead(state, fd as number, iovs as number, count as number, readPtr as number),
    fd_seek: (fd: never) => descriptorStatus(state, fd as number) === ESUCCESS ? ESPIPE : EBADF,
    fd_write: (fd: never, iovs: never, count: never, writtenPtr: never) => fdWrite(state, fd as number, iovs as number, count as number, writtenPtr as number),
    environ_sizes_get: (count: never, size: never) => strings(state, state.environ, count as number, size as number),
    environ_get: (array: never, buffer: never) => writeStrings(state, state.environ, array as number, buffer as number),
    args_sizes_get: (count: never, size: never) => strings(state, state.args, count as number, size as number),
    args_get: (array: never, buffer: never) => writeStrings(state, state.args, array as number, buffer as number),
    clock_res_get: (_id: never, ptr: never) => {
      write(state, ptr as number, 8, "clock_res_get result");
      view(state).setBigUint64(ptr as number, 1000000n, true);
      return ESUCCESS;
    },
    clock_time_get: (id: never, precision: never, ptr: never) => clockTime(state, id as number, precision as bigint, ptr as number),
    random_get: (ptr: never, length: never) => randomBytes(state, ptr as number, length as number),
    sched_yield: () => ESUCCESS,
    proc_exit: (code: never) => { throw new WasiExit(code as number); },
  };
  return base as unknown as WebAssembly.Imports["wasi_snapshot_preview1"];
}

function boundedArguments(values: readonly string[] | undefined, name: string): readonly string[] {
  if (values === undefined) return [];
  if (values.length > MAX_ARGUMENTS) throw new Error(`WASI ${name} exceeds its host bound`);
  if (!values.every((value) => typeof value === "string" && value.length <= MAX_ARGUMENT)) throw new Error(`WASI ${name} must be short strings`);
  return values;
}

function environment(values: Readonly<Record<string, string>> | undefined): readonly string[] {
  if (values === undefined) return [];
  return boundedArguments(Object.entries(values).map(([key, value]) => `${key}=${value}`), "environment");
}

/** Reject a module whose imports this host cannot answer, naming the exact import. */
export function validateImports(module: WebAssembly.Module): void {
  for (const item of WebAssembly.Module.imports(module)) {
    if (item.module !== "wasi_snapshot_preview1") throw new Error(`Unsupported WebAssembly import module: ${item.module}.${item.name}`);
    if (item.kind !== "function") throw new Error(`Unsupported WebAssembly import kind: ${item.module}.${item.name}`);
    if (!IMPLEMENTED.has(item.name)) throw new Error(`Unsupported WASI import: ${item.module}.${item.name}`);
  }
}

/** Compile the bundled module once; each frame instantiates the same trusted module. */
export async function compileWasi(bytesValue: Uint8Array): Promise<WebAssembly.Module> {
  const module = await WebAssembly.compile(bytesValue as unknown as BufferSource);
  validateImports(module);
  return module;
}

function decodedOutput(chunks: Buffer[], stream: string): string {
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(Buffer.concat(chunks));
  } catch {
    // Malformed protocol bytes must fail explicitly, never change package text.
    throw new Error(`Osprey workspace emitted malformed UTF-8 on ${stream}`);
  }
}

function commandState(input: string, options: WasiOptions): WasiState {
  return {
    descriptors: new Map([[0, 0], [1, 0], [2, 0]]),
    cursor: 0,
    stdin: Buffer.from(input, "utf8"),
    stdout: [],
    stderr: [],
    args: boundedArguments(options.args, "arguments"),
    environ: environment(options.env),
    bytes: 0,
  };
}

async function executeCommand(module: WebAssembly.Module, state: WasiState): Promise<number> {
  try {
    const instance = await WebAssembly.instantiate(module, { wasi_snapshot_preview1: wasiImports(state) });
    state.memory = instance.exports.memory as WebAssembly.Memory | undefined;
    const start = instance.exports._start;
    if (typeof start !== "function") throw new Error("Osprey workspace module does not export _start");
    start();
    return 0;
  } catch (error) {
    if (error instanceof WasiExit) return error.code;
    else throw error;
  }
}

/** The same bounded runtime, invoked only inside its command worker. */
export async function runWasiInline(module: WebAssembly.Module, input: string, options: WasiOptions = {}): Promise<WasiResult> {
  validateImports(module);
  const state = commandState(input, options);
  const exitCode = await executeCommand(module, state);
  return {
    stdout: decodedOutput(state.stdout, "stdout"),
    stderr: decodedOutput(state.stderr, "stderr"),
    exitCode,
  };
}

/** Run one isolated command without blocking the extension's event loop. */
export async function runWasi(module: WebAssembly.Module, input: string, options: WasiOptions = {}): Promise<WasiResult> {
  return runInWorker(module, input, options);
}
