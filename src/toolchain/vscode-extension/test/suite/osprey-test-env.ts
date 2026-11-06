// Shared test-environment resolution for the Osprey VSIX suites: locate the
// freshly-built osprey compiler and the lldb-dap adapter. Defined once and
// imported by every suite so no binary-resolution logic is duplicated.

import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import * as vscode from "vscode";
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
  const built = path.resolve(extensionRoot, "..", "..", "..", "target", "release", exe);
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

/** User settings pinned at real binaries, with a restore for suite teardown. */
export interface PinnedOspreySettings {
  restore(): Promise<void>;
}

/**
 * Pin `values` as GLOBAL Osprey settings for a suite, returning a handle whose
 * restore() puts back exactly what was configured before (an unset key stays
 * unset, which is how VS Code itself represents "no explicit value").
 */
export async function pinOspreySettings(
  values: Record<string, string>,
): Promise<PinnedOspreySettings> {
  const config = vscode.workspace.getConfiguration("osprey");
  const prior = new Map<string, string | undefined>();
  for (const key of Object.keys(values)) {
    prior.set(key, config.get<string>(key));
  }
  for (const [key, value] of Object.entries(values)) {
    await config.update(key, value, vscode.ConfigurationTarget.Global);
  }
  return {
    async restore(): Promise<void> {
      for (const [key, value] of prior) {
        await config.update(key, value, vscode.ConfigurationTarget.Global);
      }
    },
  };
}

/**
 * Every `test-skipped` diagnostic currently published for `uri`, in source
 * order — the editor's own view of which cases are skipped or ignored
 * ([TESTING-SKIP-WARNING]).
 */
export function skipWarningsOf(uri: vscode.Uri): vscode.Diagnostic[] {
  return vscode.languages
    .getDiagnostics(uri)
    .filter((diagnostic) => diagnostic.code === "test-skipped")
    .sort((a, b) => a.range.start.line - b.range.start.line);
}

export async function compileSdkWasm(source: string, timeout: number): Promise<WebAssembly.Module> {
  const compiler = resolveBuiltOsprey();
  assert.ok(compiler, "the SDK E2E requires the built compiler");
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "osp-sdk-probe-"));
  try {
    const roots = JSON.stringify([".", ...["native", "graphics", "json", "vscode"].map((name) => path.resolve(extensionRoot, `../../osprey/packages/${name}/src`))]);
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
