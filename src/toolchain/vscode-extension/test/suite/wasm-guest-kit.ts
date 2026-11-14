// Real WebAssembly guests, assembled byte by byte: WASI probes that run under the host's own
// `createWasi`, and a stub application that exports the shared-bridge ABI the worker drives
// ([NATIVE-SHM-WASM]). Every guest checks its own results and traps (`unreachable`) on a mismatch.
import * as assert from "assert";
import { compileWasi, createWasi, wasiImports, type WasiOptions } from "../../client/src/osprey-ui/wasi";

export const I32 = 0x7f;
export const I64 = 0x7e;
export const F64 = 0x7c;
/** WASI errno values ([wasi_snapshot_preview1]). */
export const ERRNO = { success: 0, badf: 8, inval: 28, spipe: 70 } as const;

export function uleb(value: number): number[] {
  const out: number[] = [];
  let rest = value;
  do {
    const byte = rest & 0x7f;
    rest >>>= 7;
    out.push(rest === 0 ? byte : byte | 0x80);
  } while (rest !== 0);
  return out;
}

const name = (value: string): number[] => {
  const encoded = [...Buffer.from(value, "utf8")];
  return [...uleb(encoded.length), ...encoded];
};

const section = (id: number, payload: number[]): number[] => [id, ...uleb(payload.length), ...payload];

/** `i32.const value` for a nonnegative value. */
export function integer(value: number): number[] {
  assert.ok(value >= 0 && Number.isSafeInteger(value), "guest constants are nonnegative");
  const out: number[] = [];
  let rest = BigInt(value);
  for (;;) {
    const byte = Number(rest & 0x7fn);
    rest >>= 7n;
    if (rest === 0n && (byte & 0x40) === 0) { out.push(byte); break; }
    out.push(byte | 0x80);
  }
  return [0x41, ...out];
}

/** `i64.const value` for a nonnegative value. */
export const wide = (value: number): number[] => [0x42, ...integer(value).slice(1)];

export interface Import { readonly module?: string; readonly name: string; readonly params: readonly number[]; readonly result?: number }
export interface Export { readonly name: string; readonly params: readonly number[]; readonly result?: number; readonly locals?: readonly number[]; readonly body: readonly number[] }

const signature = (params: readonly number[], result?: number): number[] =>
  [0x60, ...uleb(params.length), ...params, ...(result === undefined ? [0] : [1, result])];

/** A module with `imports`, `exports` (each a function) and an exported memory of `pages`. */
export function guestModule(imports: readonly Import[], exports: readonly Export[], pages = 2): Uint8Array {
  const body = (item: Export): number[] => {
    const locals = item.locals ?? [];
    const code = [...uleb(locals.length), ...locals.flatMap((type) => [1, type]), ...item.body, 0x0b];
    return [...uleb(code.length), ...code];
  };
  return Uint8Array.from([
    0, 0x61, 0x73, 0x6d, 1, 0, 0, 0,
    ...section(1, [...uleb(imports.length + exports.length), ...imports.flatMap((item) => signature(item.params, item.result)),
      ...exports.flatMap((item) => signature(item.params, item.result))]),
    ...section(2, [...uleb(imports.length), ...imports.flatMap((item, index) => [...name(item.module ?? "wasi_snapshot_preview1"), ...name(item.name), 0, ...uleb(index)])]),
    ...section(3, [...uleb(exports.length), ...exports.flatMap((_, index) => uleb(imports.length + index))]),
    ...section(5, [1, 0, ...uleb(pages)]),
    ...section(7, [...uleb(exports.length + 1), ...exports.flatMap((item, index) => [...name(item.name), 0, ...uleb(imports.length + index)]), ...name("memory"), 2, 0]),
    ...section(10, [...uleb(exports.length), ...exports.flatMap(body)]),
  ]);
}

/** The preview1 calls the probes use, in import order. */
export const WASI_CALLS: readonly Import[] = [
  ...["args_sizes_get", "args_get", "environ_sizes_get", "environ_get", "random_get", "fd_fdstat_get",
    "fd_fdstat_set_flags", "clock_res_get", "fd_prestat_get"].map((call) => ({ name: call, params: [I32, I32], result: I32 })),
  ...["fd_read", "fd_write"].map((call) => ({ name: call, params: [I32, I32, I32, I32], result: I32 })),
  { name: "clock_time_get", params: [I32, I64, I32], result: I32 },
  { name: "fd_close", params: [I32], result: I32 },
  { name: "fd_seek", params: [I32, I64, I32, I32], result: I32 },
  { name: "fd_prestat_dir_name", params: [I32, I32, I32], result: I32 },
];

/** Calls the WASI import `call` with `args`, leaving its errno on the stack. */
export function wasi(call: string, ...args: number[][]): number[] {
  const index = WASI_CALLS.findIndex((item) => item.name === call);
  assert.ok(index >= 0, `declared guest import ${call}`);
  return [...args.flat(), 0x10, ...uleb(index)];
}

/** Traps unless the i32 `expression` equals `expected`. */
export const expect = (expression: number[], expected: number): number[] => [...expression, ...integer(expected), 0x47, 0x04, 0x40, 0x00, 0x0b];
export const ok = (call: string, ...args: number[][]): number[] => expect(wasi(call, ...args), ERRNO.success);
export const load32 = (at: number): number[] => [...integer(at), 0x28, 2, 0];
export const load8 = (at: number): number[] => [...integer(at), 0x2d, 0, 0];
export const store32 = (at: number, value: number[]): number[] => [...integer(at), ...value, 0x36, 2, 0];
export const store8 = (at: number, value: number): number[] => [...integer(at), ...integer(value), 0x3a, 0, 0];
/** One iovec at `at`: `length` bytes at `pointer`. */
export const iovec = (at: number, pointer: number, length: number): number[] => [...store32(at, integer(pointer)), ...store32(at + 4, integer(length))];

/** A WASI command whose `_start` runs `checks`. */
export const probe = (checks: number[]): Uint8Array => guestModule(WASI_CALLS, [{ name: "_start", params: [], body: checks }]);

/** Runs a command under the host's own WASI configuration and returns its exit code. */
export async function runProbe(bytes: Uint8Array, options: WasiOptions = {}): Promise<number> {
  const host = createWasi(options);
  const instance = await WebAssembly.instantiate(await compileWasi(bytes), wasiImports(host));
  return host.start(instance);
}

// MARK: a stub application

const STUB_ARGS: Record<string, { params: number[]; result?: number }> = {
  malloc: { params: [I32], result: I32 },
  free: { params: [I32] },
  osprey_app_argument: { params: [I32], result: I64 },
  osprey_app_open: { params: [I64, I64, I32, I32, I32], result: I64 },
  osprey_app_event: { params: [I64, I64, I64, I32, I32, F64, F64, I32, I32], result: I64 },
  osp_native_open: { params: [I32, I32, I64, I64, I64, I32], result: I32 },
  osp_native_frame_commit: { params: [I32, I32], result: I32 },
  osp_native_acquire: { params: [I32, I32, I32], result: I32 },
  osp_native_child: { params: [I32, I32, I64, I32], result: I32 },
  osp_native_length: { params: [I32, I32, I32], result: I32 },
  osp_native_view: { params: [I32, I32, I64, I64, I64, I32], result: I32 },
  osp_native_release: { params: [I32, I32], result: I32 },
};

/** A bump allocator over the word at address 0, handing out memory from 4096. */
const MALLOC = [...integer(0), ...load32(0), 0x20, 0, 0x6a, ...integer(16), 0x6a, 0x36, 2, 0,
  ...load32(0), 0x20, 0, 0x6b, ...integer(16), 0x6b, ...integer(4096), 0x6a];

export interface StubBodies {
  readonly start?: readonly number[];
  readonly argument?: readonly number[];
  readonly open?: readonly number[];
  readonly event?: readonly number[];
  readonly commit?: readonly number[];
  readonly length?: readonly number[];
  readonly view?: readonly number[];
  readonly locals?: Partial<Record<"start" | "open" | "event", readonly number[]>>;
  readonly imports?: readonly Import[];
}

/** Spins `count` times on the i64 local numbered `local` (declare it in `locals`). */
export const spin = (count: number, local = 0): number[] =>
  [0x03, 0x40, 0x20, local, ...wide(1), 0x7c, 0x22, local, ...wide(count), 0x54, 0x0d, 0, 0x0b];
export const forever: readonly number[] = [0x03, 0x40, 0x0c, 0, 0x0b];

/** A stub application: every bridge call succeeds with empty runs unless `bodies` says otherwise. */
export function stubApplication(bodies: StubBodies = {}): Uint8Array {
  const exported = (export_: string, body: readonly number[], locals?: readonly number[]): Export =>
    ({ name: export_, ...STUB_ARGS[export_]!, body, locals });
  const ok32 = integer(0);
  return guestModule(bodies.imports ?? [], [
    { name: "_start", params: [], body: bodies.start ?? [], locals: bodies.locals?.start },
    exported("malloc", MALLOC),
    exported("free", []),
    exported("osprey_app_argument", bodies.argument ?? wide(0)),
    exported("osprey_app_open", bodies.open ?? wide(0), bodies.locals?.open),
    exported("osprey_app_event", bodies.event ?? wide(0), bodies.locals?.event),
    exported("osp_native_open", [0x20, 5, ...integer(1), 0x36, 2, 0, ...ok32]),
    exported("osp_native_frame_commit", bodies.commit ?? ok32),
    exported("osp_native_acquire", ok32),
    exported("osp_native_child", ok32),
    exported("osp_native_length", bodies.length ?? ok32),
    exported("osp_native_view", bodies.view ?? ok32),
    exported("osp_native_release", ok32),
  ]);
}
