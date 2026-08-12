// Smoke test for an Osprey-compiled WebAssembly module. [WASM-TARGET]
//
// Usage: node scripts/wasm-smoke.mjs <module.wasm> [expected-stdout-file]
//
// Validates the module is well-formed (`WebAssembly.validate`), runs it as a
// WASI command under Node's built-in `node:wasi` (no external runtime needed —
// the same preview1 ABI a browser WASI shim emulates), and, when an expected-
// output file is given, asserts captured stdout matches it. Exits non-zero on
// any failure so `make wasm` / CI can gate on it.

import { openSync, closeSync, readFileSync, mkdirSync, rmSync } from "node:fs";
import { WASI } from "node:wasi";
import { loadModuleFromArgv, assertMatchesGolden } from "./wasm-smoke-support.mjs";

// `node:wasi` caches the module's memory backing store when the instance starts
// and never refreshes it after `memory.grow`, so every WASI call a growing
// module makes afterwards reads or writes freed memory. On x86_64 that is a
// SIGSEGV inside node — exit 139, no stderr, no wasm trap — and where the stale
// page happens to still be mapped it silently drops the module's output
// instead, which is worse. A 12-line hand-written module that writes, grows and
// writes again reproduces it with no Osprey involved, and Node 24 is the first
// release that runs both it and the assertion corpus clean. Refuse older hosts
// rather than let a defect in the runner report itself as a compiler one; the
// module still has a second oracle in wasm-browser-smoke.mjs, whose shim reads
// the memory afresh per call and is unaffected. [WASM-TARGET]
const MIN_WASI_NODE_MAJOR = 24;
const nodeMajor = Number(process.versions.node.split(".")[0]);
if (nodeMajor < MIN_WASI_NODE_MAJOR) {
  console.error(
    `FAIL: node:wasi in Node ${process.versions.node} corrupts a module's memory after ` +
      `memory.grow (use-after-free on its cached backing store). Run under Node ` +
      `${MIN_WASI_NODE_MAJOR}+, or use scripts/wasm-browser-smoke.mjs, which drives the ` +
      `same module through the browser WASI shim.`,
  );
  process.exit(1);
}

const { wasmPath, expectedPath, bytes } = await loadModuleFromArgv(
  "usage: node wasm-smoke.mjs <module.wasm> [expected-stdout-file]",
);

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

await assertMatchesGolden(expectedPath, captured, "");

console.error(`OK: ${wasmPath} validated and ran cleanly`);
