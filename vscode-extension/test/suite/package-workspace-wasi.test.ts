import * as assert from "assert";
import { compileWasi, runWasi } from "../../client/src/osprey-ui/wasi";

// A four-import WASI command that reads fd 0 into memory and writes the same
// bytes to fd 1. It exercises the in-memory stdin/stdout boundary without a
// temp file, child process or filesystem preopen.
const ECHO_WASM = "AGFzbQEAAAABEANgBH9/f38Bf2ABfwBgAAACZwMWd2FzaV9zbmFwc2hvdF9wcmV2aWV3MQdmZF9yZWFkAAAWd2FzaV9zbmFwc2hvdF9wcmV2aWV3MQhmZF93cml0ZQAAFndhc2lfc25hcHNob3RfcHJldmlldzEJcHJvY19leGl0AAEDAgECBQMBAAEHEwIGbWVtb3J5AgAGX3N0YXJ0AAMKPAE6AEEAQYAINgIAQQRBwAA2AgBBAEEAQQFBCBAAGkEQQYAINgIAQRRBCCgCADYCAEEBQRBBAUEYEAEaCw==";

function uleb(value: number): number[] {
  const out: number[] = [];
  let rest = value;
  do {
    const byte = rest & 0x7f;
    rest >>>= 7;
    out.push(rest === 0 ? byte : byte | 0x80);
  } while (rest !== 0);
  return out;
}

function text(value: string): number[] {
  const encoded = [...Buffer.from(value, "utf8")];
  return [...uleb(encoded.length), ...encoded];
}

function section(id: number, payload: number[]): number[] {
  return [id, ...uleb(payload.length), ...payload];
}

interface GuestImport { module?: string; name: string; params: number[]; result?: number; }
const I32 = 0x7f;
const I64 = 0x7e;

function functionType(imported: GuestImport): number[] {
  const result = imported.result === undefined ? [] : [imported.result];
  return [0x60, ...uleb(imported.params.length), ...imported.params, result.length, ...result];
}

function commandModule(imports: GuestImport[], body: number[]): Uint8Array {
  return Uint8Array.from([
    0, 0x61, 0x73, 0x6d, 1, 0, 0, 0,
    ...section(1, [...uleb(imports.length + 1), ...imports.flatMap(functionType), 0x60, 0, 0]),
    ...section(2, [...uleb(imports.length), ...imports.flatMap((item, index) => [
      ...text(item.module ?? "wasi_snapshot_preview1"), ...text(item.name), 0, ...uleb(index),
    ])]),
    ...section(3, [1, ...uleb(imports.length)]), ...section(5, [1, 0, 1]),
    ...section(7, [2, ...text("_start"), 0, ...uleb(imports.length), ...text("memory"), 2, 0]),
    ...section(10, [1, ...uleb(body.length), ...body]),
  ]);
}

// A real WebAssembly command whose unused import proves the host rejects a
// module it cannot answer instead of silently returning ENOSYS.
function moduleWithImport(module: string, name: string): Uint8Array {
  return commandModule([{ module, name, params: [] }], [0, 0x0b]);
}

function preopenProbe(operation = "fd_prestat_get"): Uint8Array {
  const extra = operation === "fd_prestat_dir_name" ? integer(0) : [];
  const body = [0, ...integer(3), ...integer(0), ...extra, 0x10, 0, 0x41, 8, 0x47, 0x04, 0x40, 0, 0x0b, 0x0b];
  return commandModule([{ name: operation, params: extra.length ? [I32, I32, I32] : [I32, I32], result: I32 }], body);
}

const ABI_IMPORTS: GuestImport[] = [
  ...["args_sizes_get", "args_get", "environ_sizes_get", "environ_get", "random_get",
    "fd_fdstat_get", "fd_fdstat_set_flags", "clock_res_get"].map((name) => ({ name, params: [I32, I32], result: I32 })),
  ...["fd_read", "fd_write"].map((name) => ({ name, params: [I32, I32, I32, I32], result: I32 })),
  { name: "clock_time_get", params: [I32, I64, I32], result: I32 },
  { name: "fd_close", params: [I32], result: I32 },
  { name: "fd_seek", params: [I32, I64, I32, I32], result: I32 },
  { name: "sched_yield", params: [], result: I32 },
  { name: "proc_exit", params: [I32] },
];

function integer(value: number): number[] {
  assert.ok(value >= 0, "the guest fixture uses nonnegative constants");
  const encoded = uleb(value);
  const last = encoded.length - 1;
  if ((encoded[last] ?? 0) & 0x40) { encoded[last] = (encoded[last] ?? 0) | 0x80; encoded.push(0); }
  return [0x41, ...encoded];
}

function guestCall(name: string, ...args: number[][]): number[] {
  const index = ABI_IMPORTS.findIndex((item) => item.name === name);
  assert.ok(index >= 0, `declared guest import ${name}`);
  return [...args.flat(), 0x10, ...uleb(index)];
}

function expectGuest(expression: number[], expected: number): number[] {
  return [...expression, ...integer(expected), 0x47, 0x04, 0x40, 0x00, 0x0b];
}

const load32 = (ptr: number) => [...integer(ptr), 0x28, 2, 0];
const load8 = (ptr: number) => [...integer(ptr), 0x2d, 0, 0];
const store32 = (ptr: number, value: number[]) => [...integer(ptr), ...value, 0x36, 2, 0];
const store8 = (ptr: number, value: number) => [...integer(ptr), ...integer(value), 0x3a, 0, 0];
const success = (name: string, ...args: number[][]) => expectGuest(guestCall(name, ...args), 0);

function ungrantedDescriptorProbe(operation: string): Uint8Array {
  const params = operation === "fd_seek" ? [I32, I64, I32, I32] : operation === "fd_close" ? [I32] : [I32, I32];
  const args = operation === "fd_seek" ? [integer(3), [0x42, 0], integer(0), integer(8)]
    : operation === "fd_close" ? [integer(3)] : [integer(3), integer(0)];
  return commandModule([{ name: operation, params, result: I32 }, { name: "proc_exit", params: [I32] }],
    [0, ...args.flat(), 0x10, 0, 0x10, 1, 0x0b]);
}
const vector = (at: number, ptr: number[], length: number) => [...store32(at, ptr), ...store32(at + 4, integer(length))];

function stringTableChecks(): number[] {
  return [
    ...success("args_sizes_get", integer(64), integer(68)), ...expectGuest(load32(64), 2), ...expectGuest(load32(68), 7),
    ...success("args_get", integer(256), integer(512)),
    ...success("environ_sizes_get", integer(72), integer(76)), ...expectGuest(load32(72), 1), ...expectGuest(load32(76), 8),
    ...success("environ_get", integer(280), integer(768)),
  ];
}

function clockAndRandomChecks(): number[] {
  const positive64 = (ptr: number) => expectGuest([...integer(ptr), 0x29, 3, 0, 0x42, 0, 0x56], 1);
  return [
    ...success("clock_res_get", integer(0), integer(160)), ...positive64(160),
    ...success("clock_time_get", integer(0), [0x42, 0], integer(168)), ...positive64(168),
    ...store8(4096, 126), ...store8(4113, 126), ...success("random_get", integer(4097), integer(16)),
    ...expectGuest(load8(4096), 126), ...expectGuest(load8(4113), 126),
    ...success("fd_fdstat_get", integer(1), integer(128)), ...expectGuest(load8(128), 2),
    ...success("fd_fdstat_set_flags", integer(1), integer(0)), ...success("sched_yield"),
  ];
}

function inputChecks(): number[] {
  return [
    ...vector(1024, integer(1200), 1), ...vector(1032, integer(1208), 3),
    ...expectGuest(guestCall("fd_read", integer(3), integer(1024), integer(2), integer(1120)), 8),
    ...expectGuest(guestCall("fd_read", integer(1), integer(1024), integer(2), integer(1120)), 8),
    ...expectGuest(guestCall("fd_read", integer(2), integer(1024), integer(2), integer(1120)), 8),
    ...success("fd_read", integer(0), integer(1024), integer(2), integer(1120)), ...expectGuest(load32(1120), 2),
    ...expectGuest(load8(1200), 97), ...expectGuest(load8(1208), 98),
    ...success("fd_read", integer(0), integer(1024), integer(2), integer(1120)), ...expectGuest(load32(1120), 0),
  ];
}

function outputChecks(): number[] {
  return [
    ...vector(1024, load32(260), 2), ...vector(1032, load32(280), 7),
    ...vector(1040, integer(1200), 1), ...vector(1048, integer(1208), 1),
    ...success("fd_write", integer(1), integer(1024), integer(4), integer(1120)), ...expectGuest(load32(1120), 11),
    ...store8(1320, 33), ...vector(1024, integer(1320), 1),
    ...success("fd_write", integer(2), integer(1024), integer(1), integer(1120)),
    ...success("fd_close", integer(2)), ...guestCall("proc_exit", integer(7)),
  ];
}

function busyCommand(durationMs: number): Uint8Array {
  const clock = success("clock_time_get", integer(0), [0x42, 0], integer(0));
  const time = [...integer(0), 0x29, 3, 0];
  return commandModule(ABI_IMPORTS, [1, 1, I64, ...clock, ...time, 0x21, 0,
    0x03, 0x40, ...clock, ...time, 0x20, 0, 0x7d, 0x42, ...integer(durationMs * 1000000).slice(1),
    0x54, 0x0d, 0, 0x0b, 0x0b]);
}

suite("package workspace in-memory WASI", () => {
  test("a CPU-bound command keeps the host event loop responsive", async () => {
    const module = await compileWasi(busyCommand(100));
    let ticks = 0;
    const timer = setInterval(() => { ticks += 1; }, 5);
    try { assert.strictEqual((await runWasi(module, "")).exitCode, 0); }
    finally { clearInterval(timer); }
    assert.ok(ticks >= 3, `host timers must run while real WASM computes; received ${ticks} ticks`);
  });
  test("a real guest consumes UTF-8 args, environment, clocks, entropy and vectored streams", async () => {
    const module = await compileWasi(commandModule(ABI_IMPORTS,
      [0, ...stringTableChecks(), ...clockAndRandomChecks(), ...inputChecks(), ...outputChecks(), 0x0b]));
    const result = await runWasi(module, "ab", { args: ["cli", "é"], env: { MODE: "π" } });
    assert.deepStrictEqual(result, { stdout: "éMODE=πab", stderr: "!", exitCode: 7 });
  });
  test("stdin is the host frame and stdout is captured without temporary files", async () => {
    const module = await compileWasi(Buffer.from(ECHO_WASM, "base64"));
    const result = await runWasi(module, '{"operation":"start"}');
    assert.strictEqual(result.exitCode, 0);
    assert.strictEqual(result.stdout, '{"operation":"start"}');
    assert.strictEqual(result.stderr, "");
  });

  test("malformed UTF-8 output is quarantined instead of changing serialized text", async () => {
    const module = await compileWasi(Buffer.from(ECHO_WASM, "base64"));
    const bytes = Uint8Array.from([0xff]) as unknown as string;
    await assert.rejects(runWasi(module, bytes), /malformed UTF-8.*stdout/);
  });

  test("a second run gets a fresh stdin cursor, so dispatch is not stateful in the host", async () => {
    const module = await compileWasi(Buffer.from(ECHO_WASM, "base64"));
    assert.strictEqual((await runWasi(module, "first")).stdout, "first");
    assert.strictEqual((await runWasi(module, "second")).stdout, "second");
  });

  test("an unimplemented WASI import is named, never answered with ENOSYS", async () => {
    await assert.rejects(compileWasi(moduleWithImport("wasi_snapshot_preview1", "fd_advise")), /Unsupported WASI import: wasi_snapshot_preview1\.fd_advise/);
  });

  test("an unknown import module is named rather than surfacing a bare LinkError", async () => {
    await assert.rejects(compileWasi(moduleWithImport("env", "osprey_slice")), /Unsupported WebAssembly import module: env\.osprey_slice/);
  });

  test("libc can enumerate preopens but receives EBADF because no filesystem is granted", async () => {
    for (const operation of ["fd_prestat_get", "fd_prestat_dir_name"]) {
      const module = await compileWasi(preopenProbe(operation));
      assert.strictEqual((await runWasi(module, "")).exitCode, 0, operation);
    }
  });

  test("real guests cannot cross memory or vector-count host bounds", async () => {
    for (const [name, args, error] of [
      ["random_get", [65535, 2], /random_get buffer exceeds/],
      ["fd_read", [0, 0, 1025, 0], /fd_read received too many/],
      ["fd_write", [1, 0, 1025, 0], /fd_write received too many/],
      ["fd_write", [1, 65535, 1, 0], /fd_write iovecs exceeds/],
      ["fd_read", [0, 65535, 1, 0], /fd_read iovecs exceeds/],
    ] as const) {
      const module = await compileWasi(commandModule(ABI_IMPORTS,
        [0, ...guestCall(name, ...args.map(integer)), 0x1a, 0x0b]));
      await assert.rejects(runWasi(module, "input"), error, name);
    }
  });

  test("a real guest cannot exceed the combined stdout and stderr output bound", async () => {
    const writes = Array.from({ length: 129 }, (_unused, index) =>
      success("fd_write", integer(index % 2 ? 2 : 1), integer(0), integer(1), integer(8))).flat();
    const module = await compileWasi(commandModule(ABI_IMPORTS,
      [0, ...vector(0, integer(0), 65536), ...writes, 0x0b]));
    await assert.rejects(runWasi(module, ""), /output exceeds its host bound/);
  });

  test("fdstat on an ungranted descriptor returns EBADF instead of success", async () => {
    const module = await compileWasi(preopenProbe("fd_fdstat_get"));
    assert.strictEqual((await runWasi(module, "")).exitCode, 0);
  });

  for (const operation of ["fd_seek", "fd_close", "fd_fdstat_set_flags"]) {
    test(`${operation} on an ungranted descriptor returns EBADF instead of success`, async () => {
      const module = await compileWasi(ungrantedDescriptorProbe(operation));
      assert.strictEqual((await runWasi(module, "")).exitCode, 8);
    });
  }

  test("invalid and wrong-direction writes return EBADF without emitting bytes", async () => {
    for (const fd of [0, 3, 255]) {
      const module = await compileWasi(commandModule(ABI_IMPORTS, [0,
        ...store8(64, 65), ...vector(0, integer(64), 1),
        ...expectGuest(guestCall("fd_write", integer(fd), integer(0), integer(1), integer(16)), 8), 0x0b]));
      assert.deepStrictEqual(await runWasi(module, ""), { stdout: "", stderr: "", exitCode: 0 });
    }
  });

  test("closed descriptors stay closed for metadata, flags, streams and a second close", async () => {
    for (const fd of [0, 1, 2]) {
      const checks = [
        ...success("fd_close", integer(fd)),
        ...expectGuest(guestCall("fd_close", integer(fd)), 8),
        ...expectGuest(guestCall("fd_fdstat_get", integer(fd), integer(64)), 8),
        ...expectGuest(guestCall("fd_fdstat_set_flags", integer(fd), integer(0)), 8),
        ...store32(64, integer(123)),
        ...expectGuest(guestCall("fd_seek", integer(fd), [0x42, 0], integer(0), integer(64)), 8),
        ...expectGuest(load32(64), 123),
        ...expectGuest(guestCall(fd === 0 ? "fd_read" : "fd_write", integer(fd), integer(0), integer(0), integer(64)), 8),
      ];
      const module = await compileWasi(commandModule(ABI_IMPORTS, [0, ...checks, 0x0b]));
      assert.strictEqual((await runWasi(module, "input")).exitCode, 0, `fd ${fd}`);
      assert.strictEqual((await runWasi(module, "input")).exitCode, 0, "each run owns fresh descriptors");
    }
  });

  test("seeking a stream returns ESPIPE and leaves the result pointer unchanged", async () => {
    for (const fd of [0, 1, 2, 3]) {
      const checks = [...store32(64, integer(123)),
        ...expectGuest(guestCall("fd_seek", integer(fd), [0x42, 0], integer(0), integer(64)), fd === 3 ? 8 : 70),
        ...expectGuest(load32(64), 123)];
      const module = await compileWasi(commandModule(ABI_IMPORTS, [0, ...checks, 0x0b]));
      assert.strictEqual((await runWasi(module, "")).exitCode, 0, `fd ${fd}`);
    }
  });

  test("descriptor metadata reports actual stream rights and preserves only valid flags", async () => {
    const equal64 = (ptr: number, expected: number) => [...integer(ptr), 0x29, 3, 0, 0x42, ...integer(expected).slice(1), 0x51];
    for (const fd of [0, 1, 2]) {
      const checks = [
        ...success("fd_fdstat_set_flags", integer(fd), integer(31)),
        ...success("fd_fdstat_get", integer(fd), integer(64)), ...expectGuest(load8(66), 31),
        ...expectGuest(equal64(72, fd === 0 ? 10 : 72), 1), ...expectGuest(equal64(80, 0), 1),
        ...expectGuest(guestCall("fd_fdstat_set_flags", integer(fd), integer(32)), 28),
        ...success("fd_fdstat_get", integer(fd), integer(64)), ...expectGuest(load8(66), 31),
        ...success("fd_fdstat_set_flags", integer(fd), integer(0)),
        ...success("fd_fdstat_get", integer(fd), integer(64)), ...expectGuest(load8(66), 0),
      ];
      const module = await compileWasi(commandModule(ABI_IMPORTS, [0, ...checks, 0x0b]));
      assert.strictEqual((await runWasi(module, "")).exitCode, 0, `fd ${fd}`);
    }
  });

  test("arguments and environment are bounded before the module runs", async () => {
    const module = await compileWasi(Buffer.from(ECHO_WASM, "base64"));
    const result = await runWasi(module, "frame", { args: ["a"], env: { OSPREY: "1" } });
    assert.strictEqual(result.exitCode, 0);
    await assert.rejects(runWasi(module, "frame", { args: new Array(65).fill("x") }), /arguments exceeds its host bound/);
    await assert.rejects(runWasi(module, "frame", { env: { LONG: "x".repeat(40000) } }), /short strings/);
  });
});
