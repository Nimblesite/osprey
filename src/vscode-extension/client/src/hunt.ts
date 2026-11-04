// "Osprey: Hunt for Bugs" — symbolic bug hunting for the active file, powered
// by KLEE through `osprey hunt --json` [SYMEX-CLI] [SYMEX-FINDINGS]. The
// report lands in the "Osprey Hunt" output channel and every finding becomes a
// diagnostic at its Osprey line, so the squiggle sits where the bug is.
import * as path from "path";
import { execFile } from "child_process";
import {
  Diagnostic,
  DiagnosticCollection,
  DiagnosticSeverity,
  OutputChannel,
  Position,
  ProgressLocation,
  Range,
  Uri,
  window,
  workspace,
} from "vscode";
import { withSavedOspreyDocument } from "./compile-commands";

export const HUNT_SOURCE = "Osprey Hunt (KLEE)";

export interface HuntFinding {
  class: string;
  message: string;
  file: string | null;
  line: number | null;
  runtimeFrame: string | null;
  stdin: string;
  replay: string;
  replayDetail: string;
}

export interface HuntReport {
  program: string;
  engine: string;
  budgetSeconds: number;
  explored: { paths: number; completed: number; instructions: number; elapsed: string };
  findings: HuntFinding[];
  incomplete: { test: string; reason: string }[];
}

/** The JSON report `osprey hunt --json` prints, or undefined when stdout is not one. */
export function parseHuntReport(stdout: string): HuntReport | undefined {
  try {
    const parsed = JSON.parse(stdout);
    return Array.isArray(parsed?.findings) ? (parsed as HuntReport) : undefined;
  } catch {
    return undefined;
  }
}

/** One line for the notification and the channel. */
export function huntSummary(report: HuntReport): string {
  const paths = `${report.explored.paths} paths explored, ${report.explored.completed} normal exits`;
  if (report.findings.length === 0) {
    const unfinished = report.incomplete.length > 0 || report.explored.paths === 0
      || report.explored.completed < report.explored.paths;
    return unfinished
      ? `Hunt reported no findings; analysis unfinished (${paths}).`
      : `Hunt reported no findings: ${paths}.`;
  }
  const reproduced = report.findings.filter((f) => f.replay === "reproduced").length;
  const noun = report.findings.length === 1 ? "finding" : "findings";
  return `Hunt: ${report.findings.length} ${noun} (${reproduced} reproduced natively), ${paths}.`;
}

/** A finding that reproduced is an error; one the native run did not confirm is a warning. */
export function findingSeverity(finding: HuntFinding): DiagnosticSeverity {
  if (finding.replay === "reproduced") {
    return DiagnosticSeverity.Error;
  }
  if (finding.class === "unmodelled") {
    return DiagnosticSeverity.Information;
  }
  return DiagnosticSeverity.Warning;
}

/** The absolute path a finding's `file` (as the debug metadata named it) refers to. */
export function findingPath(finding: HuntFinding, programPath: string): string {
  const file = finding.file ?? programPath;
  if (path.isAbsolute(file)) {
    return file;
  }
  return path.join(path.dirname(programPath), file);
}

export function findingMessage(finding: HuntFinding): string {
  const parts = [`${finding.class}: ${finding.message}`];
  if (finding.runtimeFrame) {
    parts.push(`runtime ${finding.runtimeFrame}`);
  }
  if (finding.stdin) {
    parts.push(`stdin "${finding.stdin}"`);
  }
  const replay = finding.replayDetail ? `${finding.replay} (${finding.replayDetail})` : finding.replay;
  parts.push(`replay ${replay}`);
  return parts.join("\n");
}

/** Diagnostics grouped by file: a finding without a line lands on the program's first line. */
export function huntDiagnostics(report: HuntReport, programPath: string): Map<string, Diagnostic[]> {
  const byFile = new Map<string, Diagnostic[]>();
  for (const finding of report.findings) {
    const file = findingPath(finding, programPath);
    const line = Math.max((finding.line ?? 1) - 1, 0);
    const range = new Range(new Position(line, 0), new Position(line, Number.MAX_SAFE_INTEGER));
    const diagnostic = new Diagnostic(range, findingMessage(finding), findingSeverity(finding));
    diagnostic.source = HUNT_SOURCE;
    diagnostic.code = finding.class;
    byFile.set(file, [...(byFile.get(file) ?? []), diagnostic]);
  }
  return byFile;
}

function publishDiagnostics(collection: DiagnosticCollection, byFile: Map<string, Diagnostic[]>) {
  collection.clear();
  for (const [file, diagnostics] of byFile) {
    collection.set(Uri.file(file), diagnostics);
  }
}

function reportFailure(channel: OutputChannel, code: number | string, stderr: string) {
  channel.appendLine(`=== HUNT FAILED (exit ${code}) ===`);
  channel.appendLine(stderr);
  const needsKlee = /klee was not found|brew install klee|libosprey_runtime_klee/.test(stderr);
  window.showErrorMessage(
    needsKlee
      ? "Hunt needs KLEE: `brew install klee`, then `make build`. See the Osprey Hunt output."
      : "Hunt failed. See the Osprey Hunt output for details.",
  );
}

function reportSuccess(
  channel: OutputChannel,
  collection: DiagnosticCollection,
  report: HuntReport,
  programPath: string,
) {
  publishDiagnostics(collection, huntDiagnostics(report, programPath));
  const summary = huntSummary(report);
  channel.appendLine(summary);
  for (const finding of report.findings) {
    channel.appendLine(`  ${finding.class.padEnd(10)} ${finding.file ?? report.program}:${finding.line ?? "?"}`);
    for (const line of findingMessage(finding).split("\n").slice(1)) {
      channel.appendLine(`             ${line}`);
    }
  }
  if (report.incomplete.length > 0) {
    channel.appendLine(`  ${report.incomplete.length} path(s) hit the time budget (raise osprey.hunt.maxTime to finish them)`);
  }
  if (report.findings.length === 0) {
    window.showInformationMessage(summary);
  } else {
    window.showWarningMessage(summary);
  }
}

/** Runs `osprey hunt` on the active file and shows what KLEE found. */
export function huntCurrentFile(compilerCommand: string, collection: DiagnosticCollection) {
  withSavedOspreyDocument("hunt", (document) => {
    const channel = window.createOutputChannel("Osprey Hunt");
    channel.show(true);
    const maxTime = workspace.getConfiguration("osprey").get<number>("hunt.maxTime") ?? 60;
    channel.appendLine(`Hunting ${document.fileName} with KLEE for up to ${maxTime}s...`);
    const args = ["hunt", document.fileName, "--json", "--quiet", "--max-time", String(maxTime)];
    window.withProgress(
      { location: ProgressLocation.Notification, title: `Osprey Hunt: exploring ${path.basename(document.fileName)} with KLEE` },
      () =>
        new Promise<void>((resolve) => {
          const child = execFile(
            compilerCommand,
            args,
            { cwd: path.dirname(document.fileName), maxBuffer: 64 * 1024 * 1024 },
            (error: any, stdout: string, stderr: string) => {
              const report = parseHuntReport(stdout);
              if (stderr) {
                channel.appendLine(stderr.trim());
              }
              if (report) {
                reportSuccess(channel, collection, report, document.fileName);
              } else {
                reportFailure(channel, error?.code ?? "unknown", stderr || String(error?.message ?? ""));
              }
              resolve();
            },
          );
          child.stdin?.end();
        }),
    );
  });
}
