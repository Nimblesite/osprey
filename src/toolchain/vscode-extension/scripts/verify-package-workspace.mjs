import assert from "node:assert/strict";
import fs from "node:fs";
import path from "node:path";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const extension = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const require = createRequire(import.meta.url);
const runtime = path.join(extension, "out/client/src/osprey-ui");
const { compileWasi } = require(path.join(runtime, "wasi.js"));
const { startNativeRuntime } = require(path.join(runtime, "wasi-runner.js"));
const { VIEW_ROW_BYTES, ASK_ROW_BYTES } = require(path.join(runtime, "generated/native-abi.js"));
const bundle = path.join(extension, "bin", `${process.platform}-${process.arch}`);

async function verify(artifact) {
  const name = path.basename(artifact) === "workbench.wasm" ? "Workbench" : "Package workspace";
  const app = startNativeRuntime(await compileWasi(fs.readFileSync(artifact)));
  try {
    const { status, runs } = await app.open({ now: 1, offset: 0, platform: "vscode", home: "/tmp", dark: false, arguments: [] });
    assert.equal(status, 0, `${name} drew its first frame`);
    assert.equal(runs.views.byteLength % VIEW_ROW_BYTES, 0, `${name} published whole view rows`);
    assert.ok(runs.views.byteLength > 0, `${name} published at least one control`);
    assert.ok(runs.text.byteLength > 0, `${name} published its text run`);
    assert.equal(runs.asks.byteLength % ASK_ROW_BYTES, 0, `${name} published whole ask rows`);
    console.log(`${name} live wasm frame: PASS (${runs.views.byteLength / VIEW_ROW_BYTES} controls, ${runs.asks.byteLength / ASK_ROW_BYTES} ask fields)`);
  } finally {
    app.dispose();
  }
}

const supplied = process.argv.slice(2);
for (const artifact of supplied.length ? supplied : ["package-workspace.wasm", "workbench.wasm"].map(file => path.join(bundle, file))) {
  await verify(artifact);
}
