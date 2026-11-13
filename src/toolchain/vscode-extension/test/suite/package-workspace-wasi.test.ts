// [NATIVE-SHM-WASM]: one worker thread holds a real Osprey application instance. `_start` runs
// once, every call is an export of the same module, and each call's four runs are copied out of
// linear memory. The fixture is the Tally reference application, built from its own sources.
import * as assert from "assert";
import { execFileSync } from "node:child_process";
import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { readAsks } from "../../client/src/osprey-ui/asks";
import { VIEW_ROW_BYTES } from "../../client/src/osprey-ui/generated/native-abi";
import { compileWasi } from "../../client/src/osprey-ui/wasi";
import { startNativeRuntime, type FrameResult } from "../../client/src/osprey-ui/wasi-runner";
import { readFrame } from "../../client/src/osprey-ui/webview/rows";

const extension = path.resolve(__dirname, "..", "..", "..");

function compiler(): string {
  const executable = process.platform === "win32" ? "osprey.exe" : "osprey";
  const built = path.resolve(extension, "..", "..", "..", "target", "release", executable);
  assert.ok(fs.existsSync(built), `build the compiler first: ${built}`);
  return built;
}

async function tallyModule(): Promise<WebAssembly.Module> {
  const project = path.resolve(extension, "..", "..", "osprey", "examples", "tally");
  assert.ok(fs.existsSync(path.join(project, "osprey.toml")), `the tally fixture exists: ${project}`);
  const directory = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-native-wasm-"));
  const wasm = path.join(directory, "tally.wasm");
  try {
    execFileSync(compiler(), [project, "--target=wasm32", "--memory=arc", "--compile", "-o", wasm],
      { timeout: 110000, stdio: "pipe" });
    return await compileWasi(fs.readFileSync(wasm));
  } finally {
    fs.rmSync(directory, { recursive: true, force: true });
  }
}

function words(frame: FrameResult): string {
  return new TextDecoder().decode(new Uint8Array(frame.runs.text));
}

const open = { now: 1, offset: 0, platform: "vscode", home: "/home", dark: true, arguments: ["--unused"] };

suite("native live instance", () => {
  let module: WebAssembly.Module;

  suiteSetup(async function () { this.timeout(120000); module = await tallyModule(); });

  test("one instance runs _start once and answers calls with whole frames", async () => {
    const runtime = startNativeRuntime(module);
    try {
      const first = await runtime.open(open);
      assert.strictEqual(first.status, 0);
      assert.strictEqual(first.runs.views.byteLength / VIEW_ROW_BYTES, 10, "Tally publishes ten controls");
      const shown = readFrame({ shapes: first.runs.shapes, views: first.runs.views, text: first.runs.text });
      assert.strictEqual(shown.roots.length, 1);
      assert.deepStrictEqual(shown.controls.filter((control) => control.words === "Tally").length, 1);
      assert.deepStrictEqual(shown.controls.map((control) => control.kind),
        ["column", "label", "label", "line", "press", "press", "press", "entry", "gap", "press"]);
      assert.deepStrictEqual(readAsks(first.runs.asks, first.runs.text), []);
      assert.ok(words(first).includes("Use night colours"));

      const add = shown.controls[5];
      assert.ok(add && add.action !== null);
      const next = await runtime.event({ kind: 0, ordinal: add.action, words: "", detail: "", x: 0, y: 0, down: false, shift: false });
      assert.strictEqual(next.status, 0);
      assert.ok(next.revision > first.revision, "each call seals a new revision");
      assert.ok(words(next).includes("1"), words(next));

      const refused = await runtime.event({ kind: 2, ordinal: 0, words: "", detail: "", x: 0, y: 0, down: false, shift: false });
      assert.strictEqual(refused.status, 1, "a refused event still draws a whole frame");
      assert.strictEqual(refused.runs.views.byteLength / VIEW_ROW_BYTES, 10);
    } finally {
      runtime.dispose();
    }
  });

  test("an application instance answers one call at a time and stops when disposed", async () => {
    const runtime = startNativeRuntime(module);
    const [first, second] = await Promise.all([runtime.open(open), runtime.open(open)]);
    assert.strictEqual(first.runs.views.byteLength, second.runs.views.byteLength);
    runtime.dispose();
    await assert.rejects(runtime.event({ kind: 0, ordinal: 0, words: "", detail: "", x: 0, y: 0, down: false, shift: false }), /disposed/);
  });
});
