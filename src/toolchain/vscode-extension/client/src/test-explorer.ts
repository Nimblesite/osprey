// Native VS Code Test Explorer integration for Osprey ([TESTING-VSCODE]):
// discovers `*.test.{osp,ospml}` files, resolves their cases via
// `osprey <file> --list-tests`, and runs them via `osprey <file> --run`,
// mapping the TAP stream back onto test items. All decision logic is pure and
// lives in test-explorer-parse.ts; this file is the thin run wiring, structured
// like debug-panel.ts, over the item tree (test-explorer-items.ts), reporting
// (test-explorer-report.ts), the compiler process (test-explorer-process.ts)
// and skip warnings (test-explorer-skips.ts).

import * as path from "path";
import * as vscode from "vscode";
import { failureMarkdown } from "./test-explorer-docs";
import {
  compileFailureMessage,
  COVERAGE_RUN,
  envFilterFor,
  excludedIdSet,
  isCompileFailure,
  outcomeForLeaf,
  parseTapStream,
  PLAIN_RUN,
  planRun,
  runArgsFor,
  strayFailureMessage,
  testRunEnv,
  toTerminalOutput,
  type ExecResult,
  type FilePlan,
  type LeafOutcome,
  type RunMode,
} from "./test-explorer-parse";
import {
  coverageJsonPath,
  makeProfileDir,
  reportCoverage,
  detailedCoverage,
  coverageSink,
  type TestRunSink,
} from "./test-explorer-report";
import {
  testDocs,
  refreshTestFile,
  requestedItems,
  makeDiscovery,
  watchCompiler,
} from "./test-explorer-items";
import { runCompiler } from "./test-explorer-process";
import { publishSkipState, disposeSkipWarnings } from "./test-explorer-skips";

/** Every Osprey source: a module change can change the suites importing it. */
const SOURCE_GLOB = "**/*.{osp,ospml}";

const CONTROLLER_ID = "ospreyTests";
const CONTROLLER_LABEL = "Osprey Tests";

/**
 * A failed case's peek message: the case's own documentation (when it has any)
 * above the failure, then the machine-readable context block
 * ([TESTING-DOC]).
 */
function failedTestMessage(
  leaf: vscode.TestItem,
  failure: string,
): vscode.TestMessage {
  const start = leaf.range?.start;
  const location =
    start === undefined
      ? "unknown"
      : `line ${start.line + 1}, column ${start.character + 1}`;
  const doc = testDocs.get(leaf.id) ?? {
    name: leaf.label,
    summary: "",
    markdown: "",
    line: (start?.line ?? 0) + 1,
  };
  const context = failureMarkdown(
    failure,
    doc,
    leaf.uri?.fsPath ?? "unknown",
    location,
  );
  return new vscode.TestMessage(new vscode.MarkdownString(context));
}

function markLeaf(
  leaf: vscode.TestItem,
  outcome: LeafOutcome,
  sink: TestRunSink,
): void {
  if (outcome.status === "passed") {
    sink.passed(leaf);
  } else if (outcome.status === "failed") {
    sink.failed(leaf, failedTestMessage(leaf, outcome.message));
  } else {
    sink.skipped(leaf);
  }
  publishSkipState(leaf, outcome);
}

// ---------- [TESTING-SKIP-WARNING-RUN] ----------
// A skipped test is never silent: every case a run reports skipped (or fails
// to report at all) raises a Warning diagnostic on its `test` line, visible in
// the Problems panel and as an editor squiggle. A case that runs again clears
// its own warning; a deleted file drops all of its warnings.

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
// fed back through the sink ([TESTING-COVERAGE-VSCODE]). A profiling run adds
// `--profile` and executes in a per-file artifact directory, which the sink
// receives once the TAP has been reported ([TESTING-PROFILE]).
async function runLeaves(
  errorTarget: vscode.TestItem,
  leaves: vscode.TestItem[],
  filter: string | undefined,
  sink: TestRunSink,
  token: vscode.CancellationToken,
  compiler: string,
  mode: RunMode,
): Promise<void> {
  const uri = errorTarget.uri;
  if (uri === undefined) {
    return;
  }
  for (const leaf of leaves) {
    sink.enqueued(leaf);
    sink.started(leaf);
  }
  const env = testRunEnv(process.env, envFilterFor(mode, filter));
  const jsonPath = mode.kind === "coverage" ? coverageJsonPath(uri) : undefined;
  // Profiler exports land in the process's working directory ([PROF-CLI-RUN]),
  // so a profiling run executes in its own artifact directory instead of the
  // suite's own directory.
  const cwd =
    mode.kind === "profile"
      ? await makeProfileDir(mode.dir, uri)
      : path.dirname(uri.fsPath);
  const result = await runCompiler(
    compiler,
    runArgsFor(mode, uri.fsPath, jsonPath, filter),
    cwd,
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
  if (mode.kind === "profile" && sink.addProfile !== undefined) {
    sink.addProfile(uri, cwd);
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

/**
 * Map one finished execution onto its leaves: the process output into the
 * run's terminal, each case's TAP line into its verdict, and a failure that
 * belongs to no single case onto `errorTarget`. Shared by every profile —
 * the debug session's own stdout is the same TAP stream a compiler child
 * writes ([TESTING-DEBUG-VSCODE]).
 */
export function reportLeaves(
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

/**
 * How one suite's share of a run is executed: `leaves` under `filter` (a case
 * name, or undefined for the whole file), with a failure belonging to no
 * single case reported against `errorTarget`. Run, Coverage and Profile spawn
 * the compiler; Debug launches a debug session ([TESTING-DEBUG-VSCODE]).
 * Everything above this — planning, exclusions, cancellation — is shared.
 */
export type LeafExecutor = (
  errorTarget: vscode.TestItem,
  leaves: vscode.TestItem[],
  filter: string | undefined,
) => Promise<void>;

/**
 * Split one file's plan into executor calls. One unfiltered call only when the
 * whole file truly runs: a whole-file request with exclusions falls through to
 * per-leaf filtered calls so the excluded cases never execute — not merely go
 * unreported.
 */
export async function runPlanWith(
  plan: FilePlan<vscode.TestItem>,
  excluded: ReadonlySet<string>,
  token: vscode.CancellationToken,
  execute: LeafExecutor,
): Promise<void> {
  const leaves = plan.wholeFile
    ? includedChildren(plan.file, excluded)
    : plan.leaves;
  if (plan.wholeFile && leaves.length === plan.file.children.size) {
    await execute(plan.file, leaves, undefined);
    return;
  }
  for (const leaf of leaves) {
    if (token.isCancellationRequested) {
      return;
    }
    await execute(leaf, [leaf], leaf.label);
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

/**
 * Execute a run request through `executor`, honoring cancellation throughout.
 * The profile supplies only how one suite runs; discovery, planning and the
 * run's lifetime are identical whichever profile launched it.
 */
export async function executeRequestWith(
  controller: vscode.TestController,
  request: vscode.TestRunRequest,
  sink: TestRunSink,
  token: vscode.CancellationToken,
  resolveCompiler: () => string,
  executor: (compiler: string) => LeafExecutor,
): Promise<void> {
  const excluded = excludedIdSet(request.exclude);
  // end() MUST fire on every exit path — a thrown discovery/report error or a
  // reporting call rejected by an already-cancelled TestRun would otherwise
  // leave the run spinning forever in the Testing view, uncancellable (Stop
  // only signals the token; VS Code retires a run only when end() is called).
  try {
    for (const plan of planRun(
      requestedItems(controller, request),
      excluded,
      (item) =>
        item.uri !== undefined &&
        (item.parent === undefined || item.parent.uri === undefined),
    )) {
      if (token.isCancellationRequested) {
        break;
      }
      const compiler = resolveCompiler();
      await ensureFileResolved(controller, plan, compiler, token);
      await runPlanWith(plan, excluded, token, executor(compiler));
    }
  } finally {
    sink.end();
  }
}

/** Execute a run request as a compiler child process ([TESTING-VSCODE]). */
export async function executeRunRequest(
  controller: vscode.TestController,
  request: vscode.TestRunRequest,
  sink: TestRunSink,
  token: vscode.CancellationToken,
  resolveCompiler: () => string,
  mode: RunMode = PLAIN_RUN,
): Promise<void> {
  await executeRequestWith(
    controller,
    request,
    sink,
    token,
    resolveCompiler,
    (compiler) => (errorTarget, leaves, filter) =>
      runLeaves(errorTarget, leaves, filter, sink, token, compiler, mode),
  );
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
      COVERAGE_RUN,
    );
}

/**
 * Register the Osprey Test Explorer: controller + default Run profile, initial
 * workspace scan (also the Testing view's Refresh), a source watcher that
 * re-resolves the suites a change can affect, and a compiler watcher that
 * rescans after a rebuild. `resolveCompiler` is the same resolution chain the
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
  const discovery = makeDiscovery(controller, resolveCompiler);
  controller.refreshHandler = () => discovery.rescan();
  const watcher = vscode.workspace.createFileSystemWatcher(SOURCE_GLOB);
  watcher.onDidCreate((uri) => void discovery.changed(uri));
  watcher.onDidChange((uri) => void discovery.changed(uri));
  watcher.onDidDelete((uri) => void discovery.deleted(uri));
  void discovery.rescan();
  context.subscriptions.push(
    controller,
    watcher,
    watchCompiler(resolveCompiler(), discovery.rescan),
    new vscode.Disposable(disposeSkipWarnings),
  );
  return controller;
}
