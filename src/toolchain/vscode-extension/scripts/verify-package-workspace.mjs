import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import { WASI } from "node:wasi";

const extension = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const require = createRequire(import.meta.url);
const { compileWasi } = require(path.join(extension, "out/client/src/osprey-ui/wasi.js"));
const { SHARED_FINGERPRINT } = require(path.join(extension, "out/client/src/osprey-ui/generated/native-abi.js"));

const artifact = process.argv[2] ?? path.join(extension, "bin", `${process.platform}-${process.arch}`, "package-workspace.wasm");
const workbench = path.basename(artifact) === "workbench.wasm";
const name = workbench ? "Workbench" : "Package workspace";
const wasi = new WASI({ version: "preview1", args: [], env: {}, returnOnExit: true });
const module = await compileWasi(fs.readFileSync(artifact));
const instance = new WebAssembly.Instance(module, { wasi_snapshot_preview1: wasi.wasiImport });
const api = instance.exports;
assert.equal(wasi.start(instance), 0, `${name} ran its top level`);
for (const entry of ["osprey_app_argument", "osprey_app_open", "osprey_app_event", "osp_native_open",
  "osp_native_frame_commit", "osp_native_acquire", "osp_native_child", "osp_native_length",
  "osp_native_view", "osp_native_release", "malloc", "free"]) {
  assert.equal(typeof api[entry], "function", `${name} exports ${entry}`);
}
const memory = () => new DataView(api.memory.buffer);
const text = (value) => {
  const bytes = new TextEncoder().encode(`${value}\0`);
  const pointer = api.malloc(bytes.length);
  new Uint8Array(api.memory.buffer, pointer, bytes.length).set(bytes);
  return pointer;
};
const instanceId = api.malloc(16);
for (let at = 0; at < 16; at += 1) memory().setUint8(instanceId + at, (at * 37 + 11) & 255);
const out = api.malloc(8);
assert.equal(api.osp_native_open(instanceId, text(SHARED_FINGERPRINT), 64n << 20n, 256n, 256n, out), 0, `${name} opened its bridge`);
const bridge = memory().getUint32(out, true);
assert.notEqual(bridge, 0);
assert.equal(Number(api.osprey_app_open(1n, 0n, text("vscode"), text("/tmp"), 0)), 0, `${name} drew its first frame`);

const revision = api.malloc(8);
const root = api.malloc(64), child = api.malloc(64), length = api.malloc(8), acquired = api.malloc(8);
assert.equal(api.osp_native_frame_commit(bridge, revision), 0);
assert.equal(api.osp_native_acquire(bridge, root, acquired), 0);
const runs = [];
for (let run = 0; run < 4; run += 1) {
  assert.equal(api.osp_native_child(bridge, root, BigInt(run), child), 0);
  assert.equal(api.osp_native_length(bridge, child, length), 0);
  const bytes = Number(memory().getBigInt64(length, true));
  runs.push(bytes);
  assert.equal(api.osp_native_release(bridge, child), 0);
}
api.osp_native_release(bridge, root);
assert.equal(runs[1] % 152, 0, `${name} published whole view rows`);
assert.ok(runs[1] / 152 > 0, `${name} published at least one control`);
assert.ok(runs[2] > 0, `${name} published its text run`);
assert.equal(runs[3] % 32, 0, `${name} published whole ask rows`);
console.log(`${name} live wasm frame: PASS (${runs[1] / 152} controls, ${runs[3] / 32} ask fields)`);
