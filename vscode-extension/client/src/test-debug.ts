// The Test Explorer's **Debug** run profile ([TESTING-DEBUG-VSCODE]): the same
// discovery, filtering, and TAP mapping the Run profile uses, but each suite
// runs under the Osprey debug adapter instead of a compiler child process, so
// a breakpoint inside a `test(...)` body is live.
//
// The debugged binary IS the suite: `test(...)` compiles inline into the
// program and `osp_test_begin` applies OSPREY_TEST_FILTER at run time
// ([TESTING-FILTER]), so `osprey <file> --debug --compile` produces a program
// whose stdout is the very TAP stream `osprey <file> --run` writes. That is
// why the verdicts come back through the Run profile's own `reportLeaves`
// instead of a second reporting path.

import * as path from "path";
import * as vscode from "vscode";
import {
  executeRequestWith,
  reportLeaves,
  testFileLabel,
  verdictSink,
  type LeafExecutor,
  type TestRunSink,
} from "./test-explorer";
import type { ExecResult } from "./test-explorer-parse";

/**
 * The launch-config key carrying this run's id. A debug session is not
 * addressable until VS Code has created it, so the id travels IN the
 * configuration: the adapter tracker reads it back off `session.configuration`
 * to know whose output it is watching.
 */
const RUN_ID_KEY = "ospreyTestRunId";

/** What the run reports when VS Code refused to start the session at all. */
export const LAUNCH_FAILED_MESSAGE =
  "The Osprey debug session did not start. A failed debug build is reported " +
  "in the Osprey output channel; a missing adapter is settable as " +
  "osprey.debug.lldbDapPath.";

/** Nothing observed yet: no output, and an exit code only an `exited` event sets. */
const NO_OUTPUT: ExecResult = { stdout: "", stderr: "", exitCode: 0 };

/** How a suite is launched. Injected so tests drive the profile without a debugger. */
export type StartDebugging = (
  folder: vscode.WorkspaceFolder | undefined,
  config: vscode.DebugConfiguration,
) => Thenable<boolean>;

// ---------------------------------------------------------------- DAP output

/** One adapter→client DAP event, as far as this file cares. */
interface DapEvent {
  readonly event: string;
  readonly body: unknown;
}

function dapEvent(message: unknown): DapEvent | undefined {
  const record = message as { type?: unknown; event?: unknown; body?: unknown };
  return record?.type === "event" && typeof record.event === "string"
    ? { event: record.event, body: record.body }
    : undefined;
}

/**
 * Append one output event to the stream it names. `console` and `important`
 * are the ADAPTER talking about itself — never the debuggee — so they stay out
 * of the TAP stdout a verdict is read from. A body with no category is stdout,
 * as the DAP specifies.
 */
function withOutput(outcome: ExecResult, body: unknown): ExecResult {
  const { category, output } = body as { category?: unknown; output?: unknown };
  if (typeof output !== "string") {
    return outcome;
  }
  if (category === "stderr") {
    return { ...outcome, stderr: outcome.stderr + output };
  }
  if (category === undefined || category === "stdout") {
    return { ...outcome, stdout: outcome.stdout + output };
  }
  return outcome;
}

function withExitCode(outcome: ExecResult, body: unknown): ExecResult {
  const { exitCode } = body as { exitCode?: unknown };
  return typeof exitCode === "number" ? { ...outcome, exitCode } : outcome;
}

/**
 * Fold one DAP message into the outcome its session is accumulating: `output`
 * events carry the debuggee's streams, `exited` carries its exit code, and
 * every other message leaves the outcome alone.
 */
export function foldDapMessage(
  outcome: ExecResult,
  message: unknown,
): ExecResult {
  const event = dapEvent(message);
  if (event === undefined) {
    return outcome;
  }
  if (event.event === "output") {
    return withOutput(outcome, event.body);
  }
  return event.event === "exited" ? withExitCode(outcome, event.body) : outcome;
}

/** Live accumulations, keyed by run id and read once the session terminates. */
const outcomes = new Map<string, ExecResult>();

function runIdOf(session: vscode.DebugSession): string | undefined {
  const id: unknown = session.configuration[RUN_ID_KEY];
  return typeof id === "string" ? id : undefined;
}

function trackerFor(id: string | undefined): vscode.DebugAdapterTracker | undefined {
  return id === undefined
    ? undefined
    : {
        onDidSendMessage: (message) =>
          outcomes.set(
            id,
            foldDapMessage(outcomes.get(id) ?? NO_OUTPUT, message),
          ),
      };
}

/**
 * Watch every Osprey debug session this profile launches, accumulating the
 * debuggee's output by run id. Sessions the user starts themselves carry no
 * run id and are not tracked.
 */
export function registerDebugOutputTracker(): vscode.Disposable {
  return vscode.debug.registerDebugAdapterTrackerFactory("osprey", {
    createDebugAdapterTracker: (session) => trackerFor(runIdOf(session)),
  });
}

// --------------------------------------------------------------- one session

let sessionSequence = 0;

function freshRunId(): string {
  sessionSequence += 1;
  return `osprey-test-debug-${process.pid}-${sessionSequence}`;
}

/** What the Debug toolbar calls the session: the case, or the whole suite. */
export function debugSessionName(
  uri: vscode.Uri,
  filter: string | undefined,
): string {
  return filter ?? testFileLabel(uri);
}

/**
 * The launch configuration one debugged suite runs under. A single-case run
 * names the case in OSPREY_TEST_FILTER; a whole-file run sets it EMPTY rather
 * than omitting it, so a stray value inherited from the editor's environment
 * cannot silently skip cases ([TESTING-FILTER]).
 */
export function debugConfigFor(
  uri: vscode.Uri,
  filter: string | undefined,
  runId: string,
): vscode.DebugConfiguration {
  return {
    type: "osprey",
    request: "launch",
    name: debugSessionName(uri, filter),
    program: uri.fsPath,
    cwd: path.dirname(uri.fsPath),
    env: { OSPREY_TEST_FILTER: filter ?? "" },
    [RUN_ID_KEY]: runId,
  };
}

/** A pending session end: resolves on terminate, or at once on cancellation. */
interface SessionEnd {
  readonly done: Promise<void>;
  dispose(): void;
}

/**
 * Subscribe BEFORE the session starts — a suite that finishes instantly would
 * otherwise terminate between `startDebugging` resolving and a listener
 * attaching, and the run would wait forever on an event already past.
 * Cancelling the run stops the session it was waiting on.
 */
function sessionEnd(
  runId: string,
  token: vscode.CancellationToken,
): SessionEnd {
  const subscriptions: vscode.Disposable[] = [];
  let live: vscode.DebugSession | undefined;
  const done = new Promise<void>((resolve) => {
    subscriptions.push(
      vscode.debug.onDidStartDebugSession((session) => {
        live = runIdOf(session) === runId ? session : live;
      }),
      vscode.debug.onDidTerminateDebugSession((session) => {
        if (runIdOf(session) === runId) {
          resolve();
        }
      }),
      token.onCancellationRequested(() => {
        void vscode.debug.stopDebugging(live);
        resolve();
      }),
    );
  });
  return { done, dispose: () => subscriptions.forEach((s) => s.dispose()) };
}

/**
 * Debug one suite and resolve with what the debuggee wrote, or undefined when
 * VS Code refused the launch (a failed debug build, or no adapter).
 */
export async function debugSuite(
  uri: vscode.Uri,
  filter: string | undefined,
  token: vscode.CancellationToken,
  start: StartDebugging,
): Promise<ExecResult | undefined> {
  const runId = freshRunId();
  const ended = sessionEnd(runId, token);
  try {
    const started = await start(
      vscode.workspace.getWorkspaceFolder(uri),
      debugConfigFor(uri, filter, runId),
    );
    if (!started) {
      return undefined;
    }
    await ended.done;
    return outcomes.get(runId) ?? NO_OUTPUT;
  } finally {
    ended.dispose();
    outcomes.delete(runId);
  }
}

// ----------------------------------------------------------------- the profile

/**
 * Debug one suite and answer with what the debuggee wrote, or undefined when
 * the session never started. [`debugSuite`] is the real one; a caller supplies
 * another to drive the profile's reporting without an adapter.
 */
export type DebugSuiteRunner = (
  uri: vscode.Uri,
  filter: string | undefined,
  token: vscode.CancellationToken,
) => Promise<ExecResult | undefined>;

/** Announce every case the session is about to run ([TESTING-VSCODE]). */
function startAll(leaves: readonly vscode.TestItem[], sink: TestRunSink): void {
  for (const leaf of leaves) {
    sink.enqueued(leaf);
    sink.started(leaf);
  }
}

/** The LeafExecutor behind the Debug profile: one debug session per call. */
export function debugExecutor(
  sink: TestRunSink,
  token: vscode.CancellationToken,
  run: DebugSuiteRunner,
): LeafExecutor {
  return async (errorTarget, leaves, filter) => {
    const uri = errorTarget.uri;
    if (uri === undefined) {
      return;
    }
    startAll(leaves, sink);
    const outcome = await run(uri, filter, token);
    if (token.isCancellationRequested) {
      return;
    }
    if (outcome === undefined) {
      sink.errored(errorTarget, new vscode.TestMessage(LAUNCH_FAILED_MESSAGE));
      return;
    }
    reportLeaves(errorTarget, leaves, outcome, sink);
  };
}

/** The handler behind the Debug profile. */
export function makeDebugHandler(
  controller: vscode.TestController,
  resolveCompiler: () => string,
  start: StartDebugging = (folder, config) =>
    vscode.debug.startDebugging(folder, config),
): (
  request: vscode.TestRunRequest,
  token: vscode.CancellationToken,
) => Promise<void> {
  const run: DebugSuiteRunner = (uri, filter, token) =>
    debugSuite(uri, filter, token, start);
  return (request, token) => {
    const sink = verdictSink(controller.createTestRun(request));
    return executeRequestWith(
      controller,
      request,
      sink,
      token,
      resolveCompiler,
      () => debugExecutor(sink, token, run),
    );
  };
}

/**
 * Register the Debug run profile and the adapter tracker that reads its
 * sessions' output. It is the default Debug profile, so the Testing view's
 * debug button and the **Debug Test** context-menu entry both launch it
 * ([TESTING-DEBUG-VSCODE]).
 */
export function registerTestDebugProfile(
  context: vscode.ExtensionContext,
  controller: vscode.TestController,
  resolveCompiler: () => string,
  start?: StartDebugging,
): vscode.TestRunProfile {
  context.subscriptions.push(registerDebugOutputTracker());
  return controller.createRunProfile(
    "Debug",
    vscode.TestRunProfileKind.Debug,
    makeDebugHandler(controller, resolveCompiler, start),
    true,
  );
}
