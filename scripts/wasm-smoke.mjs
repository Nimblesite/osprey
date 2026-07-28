// Smoke test for an Osprey-compiled WebAssembly module. [WASM-TARGET]
//
// Usage: node scripts/wasm-smoke.mjs <module.wasm> [expected-stdout-file]
//
// Validates the module is well-formed (`WebAssembly.validate`), runs it as a
// WASI command under Node's built-in `node:wasi` (no external runtime needed —
// the same preview1 ABI a browser WASI shim emulates), and, when an expected-
// output file is given, asserts captured stdout matches it. Exits non-zero on
// any failure so `make wasm` / CI can gate on it.

import { readFile } from "node:fs/promises";
import { openSync, closeSync, readFileSync, mkdirSync, rmSync } from "node:fs";
import { WASI } from "node:wasi";

const [, , wasmPath, expectedPath] = process.argv;
if (!wasmPath) {
  console.error("usage: node wasm-smoke.mjs <module.wasm> [expected-stdout-file]");
  process.exit(2);
}

const bytes = await readFile(wasmPath);
if (!WebAssembly.validate(bytes)) {
  console.error(`FAIL: ${wasmPath} is not a valid WebAssembly module`);
  process.exit(1);
}

// Node's WASI writes to the real stdout fd, so capture it by pointing the
// instance's fd 1 at a temp file and reading it back after the run.
const capturePath = `${wasmPath}.stdout.txt`;
const fd = openSync(capturePath, "w");

// A WASI module has NO filesystem until the host preopens one — capabilities
// are granted, not ambient. Without this, `writeFile("out.txt", …)` fails on
// wasm for a reason that has nothing to do with the compiler. Grant a private
// scratch directory per module, as both the root and the working directory, so
// a relative path resolves; it is fresh each run so file programs stay
// deterministic and cannot see another module's leftovers. [WASM-TARGET]
const sandbox = `${wasmPath}.fs`;
rmSync(sandbox, { recursive: true, force: true });
mkdirSync(sandbox, { recursive: true });

const wasi = new WASI({
  version: "preview1",
  args: [wasmPath],
  env: {},
  preopens: { "/": sandbox, ".": sandbox },
  stdout: fd,
  returnOnExit: true,
});

let exitCode = 0;
try {
  const { instance } = await WebAssembly.instantiate(bytes, {
    wasi_snapshot_preview1: wasi.wasiImport,
  });
  exitCode = wasi.start(instance);
} catch (err) {
  closeSync(fd);
  console.error(`FAIL: module trapped: ${err?.message ?? err}`);
  process.exit(1);
}
closeSync(fd);

const captured = readFileSync(capturePath, "utf8");
process.stdout.write(captured);

if (exitCode) {
  console.error(`FAIL: module exited with code ${exitCode}`);
  process.exit(1);
}

if (expectedPath) {
  const expected = (await readFile(expectedPath, "utf8")).trim();
  if (captured.trim() !== expected) {
    console.error("FAIL: stdout mismatch");
    console.error(`  expected: ${JSON.stringify(expected)}`);
    console.error(`  actual:   ${JSON.stringify(captured.trim())}`);
    process.exit(1);
  }
}

console.error(`OK: ${wasmPath} validated and ran cleanly`);
