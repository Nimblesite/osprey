// Native VS Code Test Explorer integration for Osprey ([TESTING-VSCODE]):
// discovers `*.test.{osp,ospml}` files, resolves their cases via
// `osprey <file> --list-tests`, and runs them via `osprey <file> --run`,
// mapping the TAP stream back onto test items. All decision logic is pure and
// lives in test-explorer-parse.ts; this file is the thin vscode/child_process
// wiring, structured like debug-panel.ts.

import { spawn, type ChildProcess } from "child_process";
import { promises as fs } from "fs";
import * as os from "os";
import * as path from "path";
import type { Readable } from "stream";
import * as vscode from "vscode";
import {
  compileFailureMessage,
  coverageCounts,
  coverageRunArgs,
  discoveryOutcome,
  excludedIdSet,
  fileTestId,
  isCompileFailure,
  leafTestId,
  outcomeForLeaf,
  parseCoverageJson,
  parseTapStream,
  planRun,
  strayFailureMessage,
  testRangeStart,
  testRunEnv,
  toTerminalOutput,
  type DiscoveredTest,
  type ExecResult,
  type FilePlan,
  type LeafOutcome,
  type LineHits,
  type TestListParse,
} from "./test-explorer-parse";

/** The discovery glob — both flavors ([TESTING-FILE-CONVENTION]). */
export const TEST_FILE_GLOB = "**/*.test.{osp,ospml}";

const CONTROLLER_ID = "ospreyTests";
const CONTROLLER_LABEL = "Osprey Tests";

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
}

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
// a run always reaches its end even if the child were to linger. Exported: the
// profiler command ([PROF-VSCODE-FLAME]) launches the CLI through it too.
export function runCompiler(
  command: string,
  args: readonly string[],
  cwd: string,
  env: NodeJS.ProcessEnv,
  token?: vscode.CancellationToken,
): Promise<ExecResult> {
  return new Promise((resolve) => {
    const child = spawn(command, [...args], { cwd, env, detached: POSIX });
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

/** A test file's Explorer label: workspace-relative path, or basename outside one. */
export function testFileLabel(uri: vscode.Uri): string {
  return vscode.workspace.getWorkspaceFolder(uri) !== undefined
    ? vscode.workspace.asRelativePath(uri, false)
    : path.basename(uri.fsPath);
}

function getOrCreateFileItem(
  controller: vscode.TestController,
  uri: vscode.Uri,
): vscode.TestItem {
  const id = fileTestId(uri.toString());
  const existing = controller.items.get(id);
  if (existing !== undefined) {
    return existing;
  }
  const item = controller.createTestItem(id, testFileLabel(uri), uri);
  controller.items.add(item);
  return item;
}

function makeLeafItem(
  controller: vscode.TestController,
  uri: vscode.Uri,
  test: DiscoveredTest,
): vscode.TestItem {
  const leaf = controller.createTestItem(
    leafTestId(uri.toString(), test.name),
    test.name,
    uri,
  );
  const start = testRangeStart(test);
  leaf.range = new vscode.Range(
    start.line,
    start.character,
    start.line,
    start.character,
  );
  return leaf;
}

function applyDiscovery(
  controller: vscode.TestController,
  item: vscode.TestItem,
  uri: vscode.Uri,
  outcome: TestListParse,
): void {
  if (!outcome.ok) {
    item.error = outcome.error;
    item.children.replace([]);
    return;
  }
  item.error = undefined;
  item.children.replace(
    outcome.tests.map((test) => makeLeafItem(controller, uri, test)),
  );
}

/**
 * (Re-)discover one test file: create/refresh its file-level item and replace
 * its children from `--list-tests` ([TESTING-LIST]). A parse failure surfaces
 * as the file item's error message (discovery skips the type gate, so this
 * only happens on syntax errors).
 */
export async function refreshTestFile(
  controller: vscode.TestController,
  uri: vscode.Uri,
  compiler: string,
  token?: vscode.CancellationToken,
): Promise<vscode.TestItem> {
  const item = getOrCreateFileItem(controller, uri);
  const result = await runCompiler(
    compiler,
    [uri.fsPath, "--list-tests"],
    path.dirname(uri.fsPath),
    process.env,
    token,
  );
  applyDiscovery(controller, item, uri, discoveryOutcome(result));
  return item;
}

/** Drop a deleted test file's item from the tree. */
export function removeTestFile(
  controller: vscode.TestController,
  uri: vscode.Uri,
): void {
  controller.items.delete(fileTestId(uri.toString()));
}

/** Initial workspace scan. `findFiles` is injectable so tests can seed uris. */
export async function scanWorkspaceTestFiles(
  controller: vscode.TestController,
  resolveCompiler: () => string,
  findFiles: (glob: string) => Thenable<vscode.Uri[]> = (glob) =>
    vscode.workspace.findFiles(glob),
): Promise<void> {
  for (const uri of await findFiles(TEST_FILE_GLOB)) {
    await refreshTestFile(controller, uri, resolveCompiler());
  }
}

/** The items a request targets: its `include`, or every root when absent. */
export function requestedItems(
  controller: vscode.TestController,
  request: Pick<vscode.TestRunRequest, "include">,
): vscode.TestItem[] {
  if (request.include !== undefined) {
    return [...request.include];
  }
  const roots: vscode.TestItem[] = [];
  controller.items.forEach((item) => roots.push(item));
  return roots;
}

function markLeaf(
  leaf: vscode.TestItem,
  outcome: LeafOutcome,
  sink: TestRunSink,
): void {
  if (outcome.status === "passed") {
    sink.passed(leaf);
  } else if (outcome.status === "failed") {
    sink.failed(leaf, new vscode.TestMessage(outcome.message));
  } else {
    sink.skipped(leaf);
  }
}

function includedChildren(
  file: vscode.TestItem,
  excluded: ReadonlySet<string>,
): vscode.TestItem[] {
  const leaves: vscode.TestItem[] = [];
  file.children.forEach((leaf) => {
    if (!excluded.has(leaf.id)) {
      leaves.push(leaf);
    }
  });
  return leaves;
}

// runLeaves executes one `osprey <file> --run` process (filtered to a single
// case when `filter` is set — [TESTING-FILTER]) and reports every leaf's
// outcome. A compile error (non-zero exit, no TAP) marks `errorTarget` (the
// file item for whole-file runs, the leaf for filtered runs) and any other
// started leaves errored with the compiler's stderr. A coverage run goes
// through `osprey test --coverage-json` instead — same TAP, plus the report
// fed back through the sink ([TESTING-COVERAGE-VSCODE]).
async function runLeaves(
  errorTarget: vscode.TestItem,
  leaves: vscode.TestItem[],
  filter: string | undefined,
  sink: TestRunSink,
  token: vscode.CancellationToken,
  compiler: string,
  coverage: boolean,
): Promise<void> {
  const uri = errorTarget.uri;
  if (uri === undefined) {
    return;
  }
  for (const leaf of leaves) {
    sink.enqueued(leaf);
    sink.started(leaf);
  }
  const env = testRunEnv(process.env, coverage ? undefined : filter);
  const jsonPath = coverage ? coverageJsonPath(uri) : undefined;
  const args =
    jsonPath === undefined
      ? [uri.fsPath, "--run"]
      : coverageRunArgs(uri.fsPath, jsonPath, filter);
  const result = await runCompiler(
    compiler,
    args,
    path.dirname(uri.fsPath),
    env,
    token,
  );
  if (token.isCancellationRequested) {
    return;
  }
  reportLeaves(errorTarget, leaves, result, sink);
  if (jsonPath !== undefined) {
    await reportCoverage(uri, jsonPath, sink);
  }
}

/** Where one file's coverage report lands (unique per file, per session). */
function coverageJsonPath(uri: vscode.Uri): string {
  const stem = path.basename(uri.fsPath).replace(/[^A-Za-z0-9_.-]/g, "_");
  return path.join(os.tmpdir(), `osprey-cov-${process.pid}-${stem}.json`);
}

/** Read, parse, and forward one run's coverage report, then drop the file. */
async function reportCoverage(
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
  for (const hits of report.values()) {
    // `osprey test <file>` reports exactly the one suite it ran.
    sink.addLineCoverage(uri, hits);
  }
}

function reportCompileFailure(
  errorTarget: vscode.TestItem,
  leaves: vscode.TestItem[],
  result: ExecResult,
  sink: TestRunSink,
): void {
  const message = new vscode.TestMessage(
    compileFailureMessage(result.stderr, result.exitCode),
  );
  sink.errored(errorTarget, message);
  for (const leaf of leaves.filter((item) => item !== errorTarget)) {
    sink.errored(leaf, message);
  }
}

function reportLeaves(
  errorTarget: vscode.TestItem,
  leaves: vscode.TestItem[],
  result: ExecResult,
  sink: TestRunSink,
): void {
  sink.appendOutput(toTerminalOutput(result.stdout + result.stderr));
  const stream = parseTapStream(result.stdout);
  if (isCompileFailure(result.exitCode, stream)) {
    reportCompileFailure(errorTarget, leaves, result, sink);
    return;
  }
  for (const leaf of leaves) {
    markLeaf(leaf, outcomeForLeaf(leaf.label, stream.results), sink);
  }
  // Exit 1 with every case ok: an assertion OUTSIDE any test failed. Surface
  // it on the file item (or the requested leaf) or the run looks green.
  const stray = strayFailureMessage(stream, result.exitCode, result.stderr);
  if (stray !== undefined) {
    sink.failed(errorTarget, new vscode.TestMessage(stray));
  }
}

async function runPlan(
  plan: FilePlan<vscode.TestItem>,
  excluded: ReadonlySet<string>,
  sink: TestRunSink,
  token: vscode.CancellationToken,
  compiler: string,
  coverage: boolean,
): Promise<void> {
  const leaves = plan.wholeFile
    ? includedChildren(plan.file, excluded)
    : plan.leaves;
  // One unfiltered process only when the whole file truly runs. A whole-file
  // request with exclusions falls through to per-leaf filtered runs so the
  // excluded cases never execute — not merely go unreported.
  if (plan.wholeFile && leaves.length === plan.file.children.size) {
    await runLeaves(
      plan.file,
      leaves,
      undefined,
      sink,
      token,
      compiler,
      coverage,
    );
    return;
  }
  for (const leaf of leaves) {
    if (token.isCancellationRequested) {
      return;
    }
    await runLeaves(leaf, [leaf], leaf.label, sink, token, compiler, coverage);
  }
}

// A whole-file run needs the file's cases; if a run lands before discovery has
// (e.g. "Run All" during activation), resolve the file first.
async function ensureFileResolved(
  controller: vscode.TestController,
  plan: FilePlan<vscode.TestItem>,
  compiler: string,
  token: vscode.CancellationToken,
): Promise<void> {
  const uri = plan.file.uri;
  if (plan.wholeFile && uri !== undefined && plan.file.children.size === 0) {
    await refreshTestFile(controller, uri, compiler, token);
  }
}

/** Execute a run request against the sink, honoring cancellation throughout. */
export async function executeRunRequest(
  controller: vscode.TestController,
  request: vscode.TestRunRequest,
  sink: TestRunSink,
  token: vscode.CancellationToken,
  resolveCompiler: () => string,
  coverage = false,
): Promise<void> {
  const excluded = excludedIdSet(request.exclude);
  // end() MUST fire on every exit path — a thrown discovery/report error or a
  // reporting call rejected by an already-cancelled TestRun would otherwise
  // leave the run spinning forever in the Testing view, uncancellable (Stop
  // only signals the token; VS Code retires a run only when end() is called).
  try {
    for (const plan of planRun(requestedItems(controller, request), excluded)) {
      if (token.isCancellationRequested) {
        break;
      }
      const compiler = resolveCompiler();
      await ensureFileResolved(controller, plan, compiler, token);
      await runPlan(plan, excluded, sink, token, compiler, coverage);
    }
  } finally {
    sink.end();
  }
}

/** The handler behind the default Run profile: one real TestRun per request. */
export function makeRunHandler(
  controller: vscode.TestController,
  resolveCompiler: () => string,
): (
  request: vscode.TestRunRequest,
  token: vscode.CancellationToken,
) => Promise<void> {
  return (request, token) =>
    executeRunRequest(
      controller,
      request,
      controller.createTestRun(request),
      token,
      resolveCompiler,
    );
}

/**
 * Detailed line coverage stashed per FileCoverage instance, served back
 * through the profile's loadDetailedCoverage hook — VS Code renders the
 * percentage in the Test Coverage view and the hits in the editor gutter
 * ([TESTING-COVERAGE-VSCODE]).
 */
const detailedCoverage = new WeakMap<
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
    enqueued: (test) => run.enqueued(test),
    started: (test) => run.started(test),
    passed: (test, duration) => run.passed(test, duration),
    failed: (test, message, duration) => run.failed(test, message, duration),
    errored: (test, message, duration) => run.errored(test, message, duration),
    skipped: (test) => run.skipped(test),
    appendOutput: (output) => run.appendOutput(output),
    end: () => run.end(),
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

/** The handler + detail loader behind the Coverage profile. */
export function makeCoverageHandler(
  controller: vscode.TestController,
  resolveCompiler: () => string,
): (
  request: vscode.TestRunRequest,
  token: vscode.CancellationToken,
) => Promise<void> {
  return (request, token) =>
    executeRunRequest(
      controller,
      request,
      coverageSink(controller.createTestRun(request)),
      token,
      resolveCompiler,
      true,
    );
}

/** The file-watcher callbacks (exported so they are directly testable). */
export function makeWatcherHandlers(
  controller: vscode.TestController,
  resolveCompiler: () => string,
): {
  refresh: (uri: vscode.Uri) => Promise<vscode.TestItem>;
  remove: (uri: vscode.Uri) => void;
} {
  return {
    refresh: (uri) => refreshTestFile(controller, uri, resolveCompiler()),
    remove: (uri) => removeTestFile(controller, uri),
  };
}

/**
 * Register the Osprey Test Explorer: controller + default Run profile, initial
 * workspace scan, and a watcher that re-resolves changed/created test files
 * and drops deleted ones. `resolveCompiler` is the same resolution chain the
 * LSP uses (setting → bundled → PATH). `controllerId` is overridable only so
 * tests can register disposable controllers beside the real one.
 */
export function registerOspreyTestExplorer(
  context: vscode.ExtensionContext,
  resolveCompiler: () => string,
  controllerId: string = CONTROLLER_ID,
): vscode.TestController {
  const controller = vscode.tests.createTestController(
    controllerId,
    CONTROLLER_LABEL,
  );
  controller.createRunProfile(
    "Run",
    vscode.TestRunProfileKind.Run,
    makeRunHandler(controller, resolveCompiler),
    true,
  );
  // Coverage profile ([TESTING-COVERAGE-VSCODE]): same discovery and TAP
  // mapping, but each file runs through `osprey test --coverage-json`; VS Code
  // shows the percentage in the Test Coverage view and hit counts in the
  // gutter via the detail loader.
  const coverageProfile = controller.createRunProfile(
    "Coverage",
    vscode.TestRunProfileKind.Coverage,
    makeCoverageHandler(controller, resolveCompiler),
    true,
  );
  coverageProfile.loadDetailedCoverage = (_run, fileCoverage) =>
    Promise.resolve(detailedCoverage.get(fileCoverage) ?? []);
  const handlers = makeWatcherHandlers(controller, resolveCompiler);
  const watcher = vscode.workspace.createFileSystemWatcher(TEST_FILE_GLOB);
  watcher.onDidCreate((uri) => void handlers.refresh(uri));
  watcher.onDidChange((uri) => void handlers.refresh(uri));
  watcher.onDidDelete(handlers.remove);
  void scanWorkspaceTestFiles(controller, resolveCompiler);
  context.subscriptions.push(controller, watcher);
  return controller;
}
