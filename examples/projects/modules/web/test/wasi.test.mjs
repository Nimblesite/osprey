import test from "node:test";
import assert from "node:assert/strict";
import { createWasi } from "../src/wasi.js";

test("WASI fd_write decodes stdout from wasm memory", () => {
  let output = "";
  const wasi = createWasi((text) => {
    output += text;
  });
  const memory = new WebAssembly.Memory({ initial: 1 });
  wasi.setMemory(memory);
  const bytes = new Uint8Array(memory.buffer);
  bytes.set(new TextEncoder().encode("hello from osprey\n"), 64);
  const view = new DataView(memory.buffer);
  view.setUint32(16, 64, true);
  view.setUint32(20, 18, true);
  assert.equal(wasi.imports.fd_write(1, 16, 1, 32), 0);
  assert.equal(view.getUint32(32, true), 18);
  assert.equal(output, "hello from osprey\n");
});

test("unknown WASI calls degrade to ENOSYS", () => {
  const wasi = createWasi(() => {});
  assert.equal(wasi.imports.path_open(), 52);
});
