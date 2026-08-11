// Shared entry/exit checks for the two wasm smoke tests. [WASM-TARGET]
//
// wasm-smoke.mjs (Node's WASI host) and wasm-browser-smoke.mjs (the inline
// browser shim) differ only in HOW they run a module — the argument handling,
// module validation and golden comparison around that run are one contract, so
// they live here rather than in both scripts.

import { readFile } from "node:fs/promises";

// Parse argv, load the module and reject anything malformed. Exits 2 without a
// path (usage error) and 1 when the bytes are not a valid module.
export async function loadModuleFromArgv(usage) {
  const [, , wasmPath, expectedPath] = process.argv;
  if (!wasmPath) {
    console.error(usage);
    process.exit(2);
  }
  const bytes = await readFile(wasmPath);
  if (!WebAssembly.validate(bytes)) {
    console.error(`FAIL: ${wasmPath} is not a valid WebAssembly module`);
    process.exit(1);
  }
  return { wasmPath, expectedPath, bytes };
}

// Compare captured stdout against the golden, trimmed at both ends. No expected
// path means the run itself was the assertion. `prefix` names the host in the
// failure line (e.g. "browser-shim "); it is empty for the plain WASI host.
export async function assertMatchesGolden(expectedPath, captured, prefix) {
  if (!expectedPath) {
    return;
  }
  const expected = (await readFile(expectedPath, "utf8")).trim();
  if (captured.trim() !== expected) {
    console.error(`FAIL: ${prefix}stdout mismatch`);
    console.error(`  expected: ${JSON.stringify(expected)}`);
    console.error(`  actual:   ${JSON.stringify(captured.trim())}`);
    process.exit(1);
  }
}
