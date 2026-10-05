// Explicit program I/O selection. [DEBUGGER-CONSOLE]
import { execFile } from "child_process";

export type DebugConsole = "internalConsole" | "integratedTerminal" | "externalTerminal";
interface ConsoleConfiguration {
  console?: unknown;
  runInTerminal?: unknown;
  launchCommands?: unknown;
}

function selectedConsole(config: ConsoleConfiguration): DebugConsole {
  if (config.runInTerminal !== undefined) {
    throw new Error("Use console: integratedTerminal instead of runInTerminal.");
  }
  const choice = config.console === undefined ? "internalConsole" : config.console;
  if (choice !== "internalConsole" && choice !== "integratedTerminal" && choice !== "externalTerminal") {
    throw new Error("console must be internalConsole, integratedTerminal or externalTerminal.");
  }
  if (choice !== "internalConsole" && Array.isArray(config.launchCommands) && config.launchCommands.length) {
    throw new Error("Terminal console modes cannot be combined with launchCommands.");
  }
  return choice;
}

/** Never let an older adapter silently ignore a requested terminal. */
export async function resolveDebugConsole(
  config: ConsoleConfiguration, adapter: string, version = adapterVersion,
): Promise<DebugConsole> {
  const choice = selectedConsole(config);
  if (choice === "internalConsole") return choice;
  const text = await version(adapter);
  const major = /\b(?:LLVM|lldb) version (\d+)\./i.exec(text)?.[1];
  if (major === undefined || Number(major) < 21) {
    throw new Error(`${choice} requires LLDB-DAP 21 or newer. Update ${adapter} or choose internalConsole.`);
  }
  return choice;
}

function adapterVersion(adapter: string): Promise<string> {
  return new Promise((resolve, reject) => {
    const child = execFile(adapter, ["--version"], { timeout: 3000, maxBuffer: 65536 }, (error, stdout, stderr) => {
      if (error) reject(new Error(`Cannot determine LLDB-DAP terminal support: ${error.message}`));
      else resolve(stdout + stderr);
    });
    child.stdin?.end();
  });
}
