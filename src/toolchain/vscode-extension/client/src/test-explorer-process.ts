// Running the compiler for Test Explorer ([TESTING-VSCODE]): spawn it, collect
// its output, and kill its whole process tree when a run is cancelled.

import { spawn, type ChildProcess } from "child_process";
import type { Readable } from "stream";
import * as vscode from "vscode";
import { type ExecResult } from "./test-explorer-parse";

// On POSIX the compiler child is spawned detached (its own process group) so
// cancellation can kill the WHOLE tree: `osprey --run` execs the compiled test
// binary as a grandchild, and killing only osprey would leave a hanging test
// running forever.
const POSIX = process.platform !== "win32";

// killProcessTree hard-kills a compiler invocation and everything it spawned:
// the process group on POSIX (guarded — the group may already be gone), a
// direct kill otherwise (win32, or an undefined pid after a failed spawn).
function killProcessTree(child: ChildProcess): void {
  if (POSIX && child.pid !== undefined) {
    try {
      process.kill(-child.pid, "SIGKILL");
      return;
    } catch {
      // Group already exited or child is not a leader; fall back to direct kill.
    }
  }
  child.kill("SIGKILL");
}

function collectOutput(stream: Readable): () => string {
  let data = "";
  stream.setEncoding("utf8");
  stream.on("data", (chunk: string) => (data += chunk));
  return () => data;
}

// runCompiler spawns the osprey compiler and resolves (never rejects) with the
// process outcome. spawn — not execFile — is deliberate: no maxBuffer cap to
// truncate verbose test output, and `detached` enables the group kill above. A
// spawn failure (e.g. ENOENT) maps to exit -1 with the message in stderr.
// Cancellation kills the process tree AND settles the promise immediately, so
// a run always reaches its end even if the child were to linger. The child's
// stdin is closed at once, so a program that reads input sees EOF rather than
// blocking on a pipe no one writes to. Exported: the profiler command
// ([PROF-VSCODE-FLAME]) launches the CLI through it too.
export function runCompiler(
  command: string,
  args: readonly string[],
  cwd: string,
  env: NodeJS.ProcessEnv,
  token?: vscode.CancellationToken,
): Promise<ExecResult> {
  return new Promise((resolve) => {
    const child = spawn(command, [...args], { cwd, env, detached: POSIX });
    // Close the child's stdin immediately: nothing is ever written to it, and a
    // compiler that reads stdin (`input()`) would otherwise block forever on a
    // pipe that never reaches EOF, hanging the run until it is cancelled.
    child.stdin?.end();
    const stdout = collectOutput(child.stdout);
    const stderr = collectOutput(child.stderr);
    let spawnFailure = "";
    let settled = false;
    const settle = (exitCode: number): void => {
      if (!settled) {
        settled = true;
        resolve({
          stdout: stdout(),
          stderr: `${spawnFailure}${stderr()}`,
          exitCode,
        });
      }
    };
    child.on("error", (error) => {
      spawnFailure = `${error.message}\n`;
      settle(-1);
    });
    child.on("close", (code) => settle(code ?? -1));
    token?.onCancellationRequested(() => {
      killProcessTree(child);
      settle(-1);
    });
  });
}
