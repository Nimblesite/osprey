// The Debug run profile against the REAL adapter ([TESTING-DEBUG-VSCODE]):
// lldb-dap stops inside a `test(...)` body, the case's locals are readable
// there, and the session's own stdout is the TAP the verdicts are read from.
// The vscode-free decisions are covered in test-debug.test.ts.

import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import * as vscode from "vscode";
import {
  debugExecutor,
  debugSuite,
  type DebugSuiteRunner,
} from "../../client/src/test-debug";
import {
  refreshTestFile,
  registerOspreyTestExplorer,
} from "../../client/src/test-explorer";
import { leafTestId } from "../../client/src/test-explorer-parse";
import {
  assertCurrentLine,
  assertLocalVariable,
  clearDebugBreakpoints,
  continueExecution,
  setSourceBreakpoints,
  waitForDebugSessionStart,
  waitForStop,
} from "./dap-harness";
import { resolveBuiltOsprey, resolveRequiredLldbDap } from "./osprey-test-env";
import { RecordingSink } from "./test-explorer-harness";

// Line numbers are load-bearing — the assertions below reference them. A `let`
// inside the case body is what a breakpoint lands on; verified against the
// real lldb on this exact program before it was written.
const FIXTURE = [
  "fn add(a, b) = a + b ?: 0", // 1
  "", // 2
  'test("adds in steps", fn() => {', // 3
  "    let x = add(2, 3)", // 4
  "    let y = add(x, 4)", // 5
  "    expect(y, 9)", // 6
  "})", // 7
  "", // 8
  'test("needs no debugger", fn() => expect(add(1, 1), 2))', // 9
  "", // 10
].join("\n");

/** The `let y = ...` line: inside the case body, and its own line entry. */
const BREAK_LINE = 5;

const extensionId = "nimblesite.osprey";
const LAUNCH_TIMEOUT_MS = 45_000;
const TEST_TIMEOUT_MS = 120_000;

suite("Osprey Test Explorer Debug profile E2E", function () {
  // Same infrastructure flake the debugger E2E shields: under CI load
  // lldb-dap can answer a stack request before it is ready. A real
  // regression still fails every attempt.
  this.retries(2);
  const compiler = resolveBuiltOsprey();
  const disposables: vscode.Disposable[] = [];
  let fixtureDir: string;
  let uri: vscode.Uri;
  let priorCompilerPath: string | undefined;
  let priorLldbDapPath: string | undefined;
  let controllerSequence = 0;

  const realRun: DebugSuiteRunner = (target, filter, token) =>
    debugSuite(target, filter, token, (folder, config) =>
      vscode.debug.startDebugging(folder, config),
    );

  suiteSetup(async function () {
    this.timeout(60_000);
    assert.ok(compiler, "a built osprey compiler is required for this E2E");
    fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-debug-tests-"));
    uri = vscode.Uri.file(path.join(fixtureDir, "steps.test.osp"));
    fs.writeFileSync(uri.fsPath, FIXTURE);

    const lldbDapPath = resolveRequiredLldbDap();
    const config = vscode.workspace.getConfiguration("osprey");
    priorCompilerPath = config.get<string>("server.compilerPath");
    priorLldbDapPath = config.get<string>("debug.lldbDapPath");
    await config.update(
      "server.compilerPath",
      compiler,
      vscode.ConfigurationTarget.Global,
    );
    await config.update(
      "debug.lldbDapPath",
      lldbDapPath,
      vscode.ConfigurationTarget.Global,
    );
    const extension = vscode.extensions.getExtension(extensionId);
    assert.ok(extension, "Osprey extension must be installed in the test host");
    await extension.activate();
  });

  suiteTeardown(async () => {
    const config = vscode.workspace.getConfiguration("osprey");
    await config.update(
      "server.compilerPath",
      priorCompilerPath,
      vscode.ConfigurationTarget.Global,
    );
    await config.update(
      "debug.lldbDapPath",
      priorLldbDapPath,
      vscode.ConfigurationTarget.Global,
    );
    fs.rmSync(fixtureDir, { recursive: true, force: true });
  });

  teardown(async () => {
    clearDebugBreakpoints();
    for (const disposable of disposables.splice(0)) {
      disposable.dispose();
    }
    if (vscode.debug.activeDebugSession) {
      try {
        await vscode.debug.stopDebugging();
      } catch {
        // A session may terminate naturally while cleanup races it.
      }
    }
  });

  async function discoveredFile(): Promise<{
    controller: vscode.TestController;
    file: vscode.TestItem;
  }> {
    controllerSequence += 1;
    const controller = registerOspreyTestExplorer(
      { subscriptions: disposables } as unknown as vscode.ExtensionContext,
      () => compiler ?? "osprey",
      `ospreyTests-debug-e2e-${controllerSequence}`,
    );
    const file = await refreshTestFile(controller, uri, compiler ?? "osprey");
    assert.strictEqual(file.error, undefined);
    return { controller, file };
  }

  function leaves(file: vscode.TestItem): vscode.TestItem[] {
    const items: vscode.TestItem[] = [];
    file.children.forEach((leaf) => items.push(leaf));
    return items;
  }

  test("a breakpoint inside a case body stops there with the case's locals", async function () {
    this.timeout(TEST_TIMEOUT_MS);
    const { file } = await discoveredFile();
    const sink = new RecordingSink();
    const token = new vscode.CancellationTokenSource().token;
    setSourceBreakpoints(uri.fsPath, [BREAK_LINE]);

    const sessionStarted = waitForDebugSessionStart(LAUNCH_TIMEOUT_MS);
    const run = debugExecutor(sink, token, realRun)(file, leaves(file), undefined);
    const session = await sessionStarted;
    assert.strictEqual(session.type, "osprey");
    const stop = await waitForStop(session, LAUNCH_TIMEOUT_MS);
    const top = assertCurrentLine(stop.stack, BREAK_LINE, uri.fsPath);
    // `let x = add(2, 3)` ran before the breakpoint: the case's own binding is
    // live, which is the whole point of debugging a test.
    await assertLocalVariable(session, top.id, "x", /\b5\b/);
    await continueExecution(session, stop.threadId);
    await run;

    assert.deepStrictEqual(
      sink.ofKind("passed").map((event) => event.id),
      [
        leafTestId(uri.toString(), "adds in steps"),
        leafTestId(uri.toString(), "needs no debugger"),
      ],
    );
    assert.strictEqual(sink.ofKind("failed").length, 0);
    assert.ok(
      sink.output.includes("ok 1 - adds in steps"),
      "the session's own stdout is the TAP the verdicts came from",
    );
  });

  test("a single-case debug run filters the debuggee to that case", async function () {
    this.timeout(TEST_TIMEOUT_MS);
    const { file } = await discoveredFile();
    const solo = file.children.get(
      leafTestId(uri.toString(), "needs no debugger"),
    );
    assert.ok(solo, "discovery found the case to debug");
    const sink = new RecordingSink();
    const token = new vscode.CancellationTokenSource().token;

    await debugExecutor(sink, token, realRun)(solo, [solo], solo.label);

    assert.deepStrictEqual(
      sink.ofKind("passed").map((event) => event.id),
      [solo.id],
    );
    // OSPREY_TEST_FILTER reached the debuggee through the launch
    // configuration: the other case never ran ([TESTING-FILTER]).
    assert.ok(sink.output.includes("ok 1 - needs no debugger"));
    assert.ok(!sink.output.includes("adds in steps"));
  });
});
