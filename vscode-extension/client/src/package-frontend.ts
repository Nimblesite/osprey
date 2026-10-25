// The Osprey package frontend wire contract ([PACKAGE-API],
// packages/manager/README.md). The frontend binary (`osprey-pkg`, built by
// `osprey build packages/manager/cli`) has no argv builtin, so it reads ONE
// command line from stdin; because that line splits on whitespace, paths with
// spaces can never appear in it — the extension instead spawns with
// `cwd` at the project root and passes `--project .`. The answer is one
// canonical JSON line `{"lines":[…],"status":N}` ([PACKAGE-API] Reports::json)
// followed by the process's final `status: N` line: the editor requires the
// JSON's SEMANTIC status and also rejects process failures.

import { execFile } from "child_process";

/** The `--json` report the frontend prints: reason lines and a status. */
export interface PkgReport {
  readonly status: number;
  readonly lines: readonly string[];
}

/** A report is successful only when its semantic status is zero. */
export function semanticSuccess(report: PkgReport): boolean {
  return report.status === 0;
}

/**
 * One frontend command line from its whitespace-free parts. Any part that
 * contains whitespace is a programming error — the stdin entry splits on
 * whitespace, so a spaced part would silently become flag soup. Throwing
 * here is the broken-code quarantine: an injected path must fail loudly.
 */
export function buildStdinCommand(parts: readonly string[]): string {
  for (const part of parts) {
    if (/\s/.test(part)) {
      throw new Error(`frontend command part contains whitespace: ${JSON.stringify(part)}`);
    }
  }
  return parts.join(" ");
}

function isReportShape(value: any): value is PkgReport {
  return (
    Number.isSafeInteger(value?.status) && value.status >= 0 &&
    Array.isArray(value?.lines) &&
    value.lines.every((line: any) => typeof line === "string")
  );
}

/**
 * The JSON report line in frontend stdout, or undefined when stdout is not
 * one. The trailing `status: N` process line is deliberately NOT the answer:
 * the CLI cannot set exit statuses, so only the embedded JSON carries
 * semantics.
 */
export function parseFrontendReport(stdout: string): PkgReport | undefined {
  for (const line of stdout.split("\n").reverse()) {
    const trimmed = line.trim();
    if (!trimmed.startsWith("{")) {
      continue;
    }
    try {
      const parsed = JSON.parse(trimmed);
      if (isReportShape(parsed)) {
        return parsed;
      }
    } catch {
      // Not JSON — keep scanning.
    }
  }
  return undefined;
}

/** How the extension launches the frontend: an executable with an argument array, never a shell. */
export interface FrontendSpawnDeps {
  execFile: typeof execFile;
}

/**
 * Run one frontend command by spawning `executable` with NO arguments,
 * writing `command` to its stdin, and resolving with the parsed `--json`
 * report. A process failure always rejects; a successful process must also return a
 * successful semantic report. Rejects only when the frontend could not
 * be asked at all (spawn failure) or answered nothing parseable.
 */
export function runFrontendCommand(
  executable: string,
  command: string,
  cwd: string,
  spawn: FrontendSpawnDeps = { execFile },
): Promise<PkgReport> {
  return new Promise((resolve, reject) => {
    const child = spawn.execFile(
      executable,
      [],
      { cwd, timeout: 120_000, maxBuffer: 16 * 1024 * 1024, windowsHide: true },
      (error, stdout, stderr) => {
        const report = parseFrontendReport(String(stdout ?? ""));
        if (error) {
          reject(new Error(`Osprey package frontend failed: ${error.message}`));
        } else if (report) {
          resolve(report);
        } else {
          const why = String(stderr || stdout).trim();
          reject(new Error(`the Osprey package frontend did not answer with a JSON report: ${why}`));
        }
      },
    );
    child?.stdin?.once?.("error", reject);
    child?.stdin?.end(command + "\n", () => undefined);
  });
}
