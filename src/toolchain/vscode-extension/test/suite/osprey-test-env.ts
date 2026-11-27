// Shared test-environment resolution for the Osprey VSIX suites: locate the
// freshly-built osprey compiler and the lldb-dap adapter. Defined once and
// imported by every suite so no binary-resolution logic is duplicated.

import * as assert from "assert";
import { spawnSync } from "child_process";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import * as vscode from "vscode";
import { resolveLldbDapExecutable } from "../../client/src/debug-config";

// The compiled test lives at <ext>/out/test/suite, so the extension root is
// three levels up.
export const extensionRoot = path.resolve(__dirname, "..", "..", "..");

/** `file` as VS Code spells a file URI's fsPath: on Windows the drive letter is lower case. */
export const uriPath = (file: string): string => vscode.Uri.file(file).fsPath;

import { present } from "../present";

export { present };

// Windows spawns only PE executables, so a stub there is `<stub>.js` beside a
// copy of this trampoline, which hands its own command line, quoting intact,
// to `node <stub>.js` and returns the script's exit code.
const TRAMPOLINE = `#include <windows.h>
#include <wchar.h>
int main(void) {
  static wchar_t self[MAX_PATH], line[32768];
  GetModuleFileNameW(NULL, self, MAX_PATH);
  wcscpy(wcsrchr(self, L'.'), L".js");
  const wchar_t *rest = GetCommandLineW();
  if (*rest == L'"') { rest = wcschr(rest + 1, L'"'); rest = rest ? rest + 1 : L""; }
  else { while (*rest && *rest != L' ') rest++; }
  swprintf(line, 32768, L"node \\"%ls\\"%ls", self, rest);
  STARTUPINFOW si = { sizeof si };
  PROCESS_INFORMATION pi;
  if (!CreateProcessW(NULL, line, NULL, NULL, TRUE, 0, NULL, NULL, &si, &pi)) return 127;
  DWORD code = 1;
  WaitForSingleObject(pi.hProcess, INFINITE);
  GetExitCodeProcess(pi.hProcess, &code);
  return (int)code;
}
`;

let trampoline: string | undefined;
function windowsTrampoline(): string {
  if (trampoline) return trampoline;
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-stub-"));
  const exe = path.join(dir, "trampoline.exe");
  fs.writeFileSync(path.join(dir, "trampoline.c"), TRAMPOLINE);
  const built = spawnSync(process.env.OSPREY_CC || "clang", ["-O2", "-o", exe, path.join(dir, "trampoline.c")], { encoding: "utf8" });
  assert.strictEqual(built.status, 0, `compiling the stub trampoline: ${built.error?.message ?? ""}${built.stderr}`);
  return (trampoline = exe);
}

/** A runnable stand-in for a CLI, written in JavaScript; returns the path to spawn. */
export function writeStubExecutable(file: string, script: string): string {
  if (process.platform !== "win32") {
    fs.writeFileSync(file, `#!/usr/bin/env node\n${script}`, { mode: 0o755 });
    return file;
  }
  fs.writeFileSync(`${file}.js`, script);
  fs.copyFileSync(windowsTrampoline(), `${file}.exe`);
  return `${file}.exe`;
}

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
  const built = path.resolve(
    extensionRoot,
    "..",
    "..",
    "..",
    "target",
    "release",
    exe,
  );
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

/**
 * The one skip diagnostic `uri` carries, proving it is exactly one — the
 * published set is JSON-dumped on failure so a wrong set is visible.
 */
export function oneSkipWarning(uri: vscode.Uri): vscode.Diagnostic {
  const warnings = skipWarningsOf(uri);
  assert.strictEqual(warnings.length, 1, JSON.stringify(warnings));
  return present(warnings[0], "the one skip warning");
}

/**
 * The skip warnings' full layout: one per expected line, in source order,
 * carrying exactly the expected messages.
 */
export function assertSkipWarningLayout(
  warnings: vscode.Diagnostic[],
  expected: { line: number; message: string }[],
  note: string,
): void {
  assert.deepStrictEqual(
    warnings.map((warning) => warning.range.start.line),
    expected.map((expectation) => expectation.line),
    note,
  );
  assert.deepStrictEqual(
    warnings.map((warning) => warning.message),
    expected.map((expectation) => expectation.message),
  );
}

/**
 * Every test deadline is multiplied by `OSPREY_TEST_DEADLINE_SCALE` (default 1)
 * so a cold or loaded runner can be given more time. The deadlines stay finite,
 * so a hang still fails. A scale below 1 or one that isn't a number is refused
 * when the suite loads, never silently ignored.
 */
const DEADLINE_SCALE = ((raw: string | undefined): number => {
  const scale = raw === undefined || raw === "" ? 1 : Number(raw);
  if (!Number.isFinite(scale) || scale < 1) {
    throw new Error(`OSPREY_TEST_DEADLINE_SCALE must be a number >= 1, got ${JSON.stringify(raw)}`);
  }
  return scale;
})(process.env.OSPREY_TEST_DEADLINE_SCALE);

/** `ms` scaled for this runner. */
export const testDeadline = (ms: number): number => ms * DEADLINE_SCALE;
