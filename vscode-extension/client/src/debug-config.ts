import * as path from "path";
import * as fs from "fs";
import { execFile, execFileSync } from "child_process";
import { workspace } from "vscode";
import { looksLikePath } from "./client-config";

export interface LldbDapResolutionHost {
  env?: NodeJS.ProcessEnv;
  existsSync?: (filePath: string) => boolean;
  readDir?: (directory: string) => string[];
  execFileSync?: (
    command: string,
    args: readonly string[],
    options: { encoding: BufferEncoding },
  ) => string | Buffer;
  getSetting?: () => string | undefined;
  platform?: NodeJS.Platform;
}

function findExecutableOnPath(
  command: string,
  env: NodeJS.ProcessEnv,
  existsSync: (filePath: string) => boolean,
): string | undefined {
  for (const dir of (env.PATH ?? "").split(path.delimiter)) {
    if (!dir) {
      continue;
    }
    const candidate = path.join(dir, command);
    if (existsSync(candidate)) {
      return candidate;
    }
  }
  return undefined;
}

function versionedDap(
  dir: string,
  name: string,
  existsSync: (filePath: string) => boolean,
): [number, string] | undefined {
  const version = /^lldb-dap-(\d+)$/.exec(name)?.[1];
  const candidate = path.join(dir, name);
  return version && existsSync(candidate) ? [Number(version), candidate] : undefined;
}

function findVersionedLldbDap(
  env: NodeJS.ProcessEnv,
  existsSync: (filePath: string) => boolean,
  readDir: (directory: string) => string[],
): string | undefined {
  const candidates = (env.PATH ?? "").split(path.delimiter).filter(Boolean).flatMap((dir) => {
    try {
      return readDir(dir)
        .map((name) => versionedDap(dir, name, existsSync))
        .filter((item): item is [number, string] => item !== undefined);
    } catch {
      return [];
    }
  });
  return candidates.sort((left, right) => right[0] - left[0])[0]?.[1];
}

export function resolveLldbDapCommand(
  config: any = {},
  host: LldbDapResolutionHost = {},
): string {
  const platform = host.platform ?? process.platform;
  const lldbDapName = platform === "win32" ? "lldb-dap.exe" : "lldb-dap";
  return resolveLldbDapExecutable(config, host) ?? lldbDapName;
}

export function resolveLldbDapExecutable(
  config: any = {},
  host: LldbDapResolutionHost = {},
): string | undefined {
  const setting = host.getSetting
    ? host.getSetting()
    : workspace.getConfiguration("osprey").get<string>("debug.lldbDapPath");
  const configured = config.lldbDapPath || setting;
  const platform = host.platform ?? process.platform;
  const existsSync = host.existsSync ?? fs.existsSync;
  const env = host.env ?? process.env;
  const lldbDapName = platform === "win32" ? "lldb-dap.exe" : "lldb-dap";
  const legacyName = platform === "win32" ? "lldb-vscode.exe" : "lldb-vscode";
  if (configured) {
    if (looksLikePath(configured)) {
      return existsSync(configured) ? configured : undefined;
    }
    return findExecutableOnPath(configured, env, existsSync);
  }

  const onPath =
    findExecutableOnPath(lldbDapName, env, existsSync) ??
    findExecutableOnPath(legacyName, env, existsSync);
  if (onPath) {
    return onPath;
  }
  if (platform === "linux") {
    const versioned = findVersionedLldbDap(env, existsSync, host.readDir ?? fs.readdirSync);
    if (versioned) {
      return versioned;
    }
  }

  if (platform === "darwin") {
    try {
      const xcrun = host.execFileSync ?? execFileSync;
      const resolved = String(
        xcrun("xcrun", ["-f", "lldb-dap"], { encoding: "utf8" }),
      ).trim();
      if (resolved && existsSync(resolved)) {
        return resolved;
      }
    } catch {
      // Fall through to common install locations.
    }
  }

  const commonCandidates =
    platform === "win32"
      ? [
          "C:\\Program Files\\LLVM\\bin\\lldb-dap.exe",
          "C:\\Program Files\\LLVM\\bin\\lldb-vscode.exe",
        ]
      : [
          "/opt/homebrew/opt/llvm/bin/lldb-dap",
          "/usr/local/opt/llvm/bin/lldb-dap",
          "/usr/bin/lldb-dap",
          "/opt/homebrew/opt/llvm/bin/lldb-vscode",
          "/usr/local/opt/llvm/bin/lldb-vscode",
          "/usr/bin/lldb-vscode",
        ];
  return commonCandidates.find(existsSync);
}

export function missingLldbDapMessage(config: any = {}): string {
  const configured = config.lldbDapPath
    ? ` Configured lldbDapPath: ${config.lldbDapPath}.`
    : "";
  return (
    "lldb-dap was not found. Install LLVM/LLDB or set osprey.debug.lldbDapPath " +
    "to an existing lldb-dap executable. Checked launch config, VS Code setting, PATH, " +
    `xcrun, and common LLVM install paths.${configured}`
  );
}

/** Compile the exact debug artifact selected by this launch. [DEBUGGER-EDITOR-LAUNCH] */
export function compileDebugProgram(
  compilerCommand: string,
  sourceProgram: string,
  debugOutput: string,
  cwd: string,
  log: (message: string) => void,
  preserveArtifacts = false,
): Promise<void> {
  fs.mkdirSync(path.dirname(debugOutput), { recursive: true });
  return new Promise((resolve, reject) => {
    const args = debugBuildArguments(sourceProgram, debugOutput, preserveArtifacts);
    const build = execFile(compilerCommand, args, { cwd }, (error, stdout, stderr) => {
      [stdout, stderr].filter(Boolean).forEach(log);
      if (error) {
        reject(new Error(`Osprey debug build failed: ${error.message}`));
      } else {
        resolve();
      }
    });
    // Nothing writes to a debug build's stdin. Closing it prevents a child
    // from waiting forever on an open, empty pipe.
    build.stdin?.end();
  });
}

function debugBuildArguments(source: string, output: string, preserve: boolean): string[] {
  return [source, "--debug", "--debug-opt=none", "--compile", "-o", output,
    ...(preserve ? ["--debug-preserve-ir", "--debug-preserve-symbols"] : [])];
}
