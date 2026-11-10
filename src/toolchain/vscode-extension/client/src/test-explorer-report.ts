// How a Test Explorer run reports ([TESTING-VSCODE]): the verdict sink a run
// writes outcomes through, the coverage sink layered on it, where a run writes
// its coverage JSON, and the detailed per-line view.

import { promises as fs } from "fs";
import * as os from "os";
import * as path from "path";
import * as vscode from "vscode";
import { coverageFiles } from "./coverage-files";
import { coverageCounts, parseCoverageJson, type LineHits } from "./test-explorer-runs";

/** Where one file's coverage report lands (unique per file, per session). */
export function coverageJsonPath(uri: vscode.Uri): string {
  const stem = path.basename(uri.fsPath).replace(/[^A-Za-z0-9_.-]/g, "_");
  return path.join(os.tmpdir(), `osprey-cov-${process.pid}-${stem}.json`);
}

/**
 * A per-file directory under the run's profile root, so several suites in one
 * request cannot overwrite each other's `<stem>.profile.json`
 * ([TESTING-PROFILE]).
 */
export async function makeProfileDir(root: string, uri: vscode.Uri): Promise<string> {
  const stem = path.basename(uri.fsPath).replace(/[^A-Za-z0-9_.-]/g, "_");
  const dir = path.join(root, stem);
  await fs.mkdir(dir, { recursive: true });
  return dir;
}

/** Read, parse, and forward one run's coverage report, then drop the file. */
export async function reportCoverage(
  uri: vscode.Uri,
  jsonPath: string,
  sink: TestRunSink,
): Promise<void> {
  if (sink.addLineCoverage === undefined) {
    return;
  }
  let text: string;
  try {
    text = await fs.readFile(jsonPath, "utf8");
  } catch {
    return; // compile failure — no report was written
  }
  await fs.rm(jsonPath, { force: true });
  const report = parseCoverageJson(text);
  if (report === undefined) {
    return;
  }
  const ran = uri.fsPath;
  for (const [file, hits] of coverageFiles(ran, path.dirname(ran), report)) {
    sink.addLineCoverage(file === ran ? uri : vscode.Uri.file(file), hits);
  }
}

/**
 * Detailed line coverage stashed per FileCoverage instance, served back
 * through the profile's loadDetailedCoverage hook — VS Code renders the
 * percentage in the Test Coverage view and the hits in the editor gutter
 * ([TESTING-COVERAGE-VSCODE]).
 */
export const detailedCoverage = new WeakMap<
  vscode.FileCoverage,
  vscode.StatementCoverage[]
>();

/** The gutter detail stashed for one FileCoverage (what the Coverage profile's
 *  loadDetailedCoverage serves). Exported so tests can prove the per-line
 *  StatementCoverage values without reaching into VS Code internals. */
export function detailedCoverageFor(
  fileCoverage: vscode.FileCoverage,
): vscode.StatementCoverage[] {
  return detailedCoverage.get(fileCoverage) ?? [];
}

/**
 * A TestRun-backed sink whose coverage lands on the run as FileCoverage.
 * Exported so tests can assert the exact TestCoverageCount (the numbers VS
 * Code renders as the coverage percentage) against a recording run.
 */
export function coverageSink(run: vscode.TestRun): TestRunSink {
  return {
    ...verdictSink(run),
    addLineCoverage: (uri, hits) => {
      const counts = coverageCounts(hits);
      const file = new vscode.FileCoverage(
        uri,
        new vscode.TestCoverageCount(counts.covered, counts.total),
      );
      detailedCoverage.set(
        file,
        [...hits].map(
          ([line, count]) =>
            // Coverage lines are 1-based; vscode positions are 0-based.
            new vscode.StatementCoverage(
              count,
              new vscode.Position(Math.max(line - 1, 0), 0),
            ),
        ),
      );
      run.addCoverage(file);
    },
  };
}

/**
 * The slice of vscode.TestRun the executor reports through. A real TestRun
 * satisfies it structurally; tests substitute a recording sink to observe the
 * run without touching Test Explorer internals.
 */
export interface TestRunSink {
  enqueued(test: vscode.TestItem): void;
  started(test: vscode.TestItem): void;
  passed(test: vscode.TestItem, duration?: number): void;
  failed(
    test: vscode.TestItem,
    message: vscode.TestMessage | readonly vscode.TestMessage[],
    duration?: number,
  ): void;
  errored(
    test: vscode.TestItem,
    message: vscode.TestMessage | readonly vscode.TestMessage[],
    duration?: number,
  ): void;
  skipped(test: vscode.TestItem): void;
  appendOutput(output: string): void;
  end(): void;
  /** Coverage runs report each file's line hits here ([TESTING-COVERAGE-VSCODE]). */
  addLineCoverage?(uri: vscode.Uri, hits: LineHits): void;
  /**
   * Profiling runs report where one file's profiler artifacts landed
   * ([TESTING-PROFILE]); the Profile profile turns that directory into the
   * flame panel and the inline heat decorations.
   */
  addProfile?(uri: vscode.Uri, dir: string): void;
}

/**
 * The verdict half of a TestRunSink: every result forwarded straight to `run`.
 * Each profile's sink spreads this and adds only its own extra channel
 * (coverage, profiler artifacts), so a verdict is reported identically no
 * matter which profile launched the run.
 */
export function verdictSink(run: vscode.TestRun): TestRunSink {
  return {
    enqueued: (test) => run.enqueued(test),
    started: (test) => run.started(test),
    passed: (test, duration) => run.passed(test, duration),
    failed: (test, message, duration) => run.failed(test, message, duration),
    errored: (test, message, duration) => run.errored(test, message, duration),
    skipped: (test) => run.skipped(test),
    appendOutput: (output) => run.appendOutput(output),
    end: () => run.end(),
  };
}
