// Browser-path smoke test for an Osprey-compiled WebAssembly module. [WASM-TARGET]
//
// Usage: node scripts/wasm-browser-smoke.mjs <module.wasm> [expected-stdout-file]
//
// Complements scripts/wasm-smoke.mjs (which runs under Node's WASI host) by
// exercising the EXACT inline WASI shim the browser uses — examples/wasm/
// wasi-shim.mjs — so a regression in the browser loader is caught in CI without
// launching a browser. Exits non-zero on trap or stdout mismatch.

import { runModule } from "../examples/wasm/wasi-shim.mjs";
import { loadModuleFromArgv, assertMatchesGolden } from "./wasm-smoke-support.mjs";

const { wasmPath, expectedPath, bytes } = await loadModuleFromArgv(
  "usage: node wasm-browser-smoke.mjs <module.wasm> [expected-stdout-file]",
);

let captured = "";
try {
  await runModule(bytes, (text) => {
    captured += text;
  });
} catch (err) {
  console.error(`FAIL: module trapped under the browser shim: ${err?.message ?? err}`);
  process.exit(1);
}
process.stdout.write(captured);

await assertMatchesGolden(expectedPath, captured, "browser-shim ");

console.error(`OK: ${wasmPath} ran cleanly under the browser WASI shim`);
