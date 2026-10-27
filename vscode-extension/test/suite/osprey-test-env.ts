// Shared test-environment resolution for the Osprey VSIX suites: locate the
// freshly-built osprey compiler and the lldb-dap adapter. Defined once and
// imported by every suite so no binary-resolution logic is duplicated.

import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import { execFileSync } from "child_process";
import { resolveLldbDapExecutable } from "../../client/src/extension";
import { compileWasi, runWasi } from "../../client/src/osprey-ui/wasi";

// The compiled test lives at <ext>/out/test/suite, so the extension root is
// three levels up.
export const extensionRoot = path.resolve(__dirname, "..", "..", "..");

/** Absolute path of the `osprey` binary staged on PATH, or undefined. */
export function resolveOspreyOnPath(): string | undefined {
  const exe = process.platform === "win32" ? "osprey.exe" : "osprey";
  for (const dir of (process.env.PATH ?? "").split(path.delimiter)) {
    if (!dir) {
      continue;
    }
    const candidate = path.join(dir, exe);
    if (fs.existsSync(candidate)) {
      return candidate;
    }
  }
  return undefined;
}

/**
 * The osprey binary tests should launch: PREFER the freshly-built dev compiler
 * under the repo (target/release/osprey — the `make build` output that speaks
 * the current protocol) over any older `osprey` on PATH. Falls back to the PATH
 * binary, then undefined.
 */
export function resolveBuiltOsprey(): string | undefined {
  const exe = process.platform === "win32" ? "osprey.exe" : "osprey";
  const built = path.resolve(extensionRoot, "..", "target", "release", exe);
  return fs.existsSync(built) ? built : resolveOspreyOnPath();
}

/** Resolve lldb-dap for the debugger E2E, failing the test if it is absent. */
export function resolveRequiredLldbDap(): string {
  const command = resolveLldbDapExecutable({});
  if (command && fs.existsSync(command)) {
    return command;
  }
  assert.fail(
    `lldb-dap is required for the Osprey VSIX debugger E2E; resolved "${command}" but it was not executable`,
  );
}

export async function compileSdkWasm(source: string, timeout: number): Promise<WebAssembly.Module> {
  const compiler = resolveBuiltOsprey();
  assert.ok(compiler, "the SDK E2E requires the built compiler");
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "osp-sdk-probe-"));
  try {
    const roots = JSON.stringify([".", ...["native", "vscode"].map((name) => path.resolve(extensionRoot, `../packages/${name}/src`))]);
    fs.writeFileSync(path.join(dir, "osprey.toml"), `[project]\nname = "sdk-probe"\nsource_roots = ${roots}\nentry = "main.ospml"\n`);
    fs.writeFileSync(path.join(dir, "main.ospml"), source);
    const wasm = path.join(dir, "probe.wasm");
    execFileSync(compiler, [dir, "--target=wasm32", "--memory=arc", "--compile", "-o", wasm], { timeout });
    return await compileWasi(fs.readFileSync(wasm));
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
}

export async function runSdkWasmJson(module: WebAssembly.Module, input: string): Promise<unknown> {
  const result = await runWasi(module, input);
  assert.strictEqual(result.exitCode, 0, result.stderr);
  assert.strictEqual(result.stderr, "");
  return JSON.parse(result.stdout);
}

export async function compileSdkWasmJson(source: string, input: string, timeout: number): Promise<unknown> {
  return runSdkWasmJson(await compileSdkWasm(source, timeout), input);
}
