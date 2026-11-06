// Existing debugger workflows, mounted in the same suite and sharing its real launch setup.
import * as assert from 'assert';
import * as fs from 'fs';
import * as vscode from 'vscode';
import { selectedProfileVariable } from '@nimblesite/lsp-profiler-ui/out/src/debug-selection';
import { assertCurrentLine, assertLocalVariable, continueExecution, getScopes, readFrameVariables, stepOver, waitForDebugSessionEnd, waitForStop, setSourceBreakpoints, type DapStop } from './dap-harness';

type Launch = (specs: Parameters<typeof setSourceBreakpoints>[1], overrides?: Record<string, unknown>) => Promise<{ session: vscode.DebugSession; stop: DapStop }>;

export function registerDebugTailTests(launchToFirstStop: Launch, getSource: () => string, getDebugOutput: () => string, hasFrame: (stop: DapStop, name: string) => boolean, TEST_TIMEOUT_MS: number, LAUNCH_TIMEOUT_MS: number): void {


  test("step over executes a call without descending into it", async function () {
    this.timeout(TEST_TIMEOUT_MS);

    // Stop at the first square(5) call site in main.
    const { session, stop } = await launchToFirstStop([3]);
    assertCurrentLine(stop.stack, 3, getSource());

    // Step OVER the call: we stay in main and advance to line 4 (not into square).
    await stepOver(session, stop.threadId);
    const afterFirst = await waitForStop(session, LAUNCH_TIMEOUT_MS);
    assert.ok(hasFrame(afterFirst, "main"), "step over stayed in main");
    assertCurrentLine(afterFirst.stack, 4, getSource());

    // Step OVER again to line 5.
    await stepOver(session, afterFirst.threadId);
    const afterSecond = await waitForStop(session, LAUNCH_TIMEOUT_MS);
    assertCurrentLine(afterSecond.stack, 5, getSource());

    await continueExecution(session, afterSecond.threadId);
    await waitForDebugSessionEnd(LAUNCH_TIMEOUT_MS, session.id);
  });


  test("multiple breakpoints across functions are each hit in order", async function () {
    this.timeout(TEST_TIMEOUT_MS);

    // Breakpoints in BOTH functions; continue walks through every hit.
    const { session, stop } = await launchToFirstStop([1, 2]);

    // First hit: square(5).
    assertCurrentLine(stop.stack, 1, getSource());
    await assertLocalVariable(
      session,
      stop.stack.stackFrames[0].id,
      "n",
      /\b5\b/,
    );

    // Second hit: square(7).
    await continueExecution(session, stop.threadId);
    const secondHit = await waitForStop(session, LAUNCH_TIMEOUT_MS);
    assertCurrentLine(secondHit.stack, 1, getSource());
    await assertLocalVariable(
      session,
      secondHit.stack.stackFrames[0].id,
      "n",
      /\b7\b/,
    );

    // Third hit: addThree(second) where second == 49.
    await continueExecution(session, secondHit.threadId);
    const thirdHit = await waitForStop(session, LAUNCH_TIMEOUT_MS);
    assertCurrentLine(thirdHit.stack, 2, getSource());
    assert.ok(hasFrame(thirdHit, "addThree"), "third stop is in addThree");
    await assertLocalVariable(
      session,
      thirdHit.stack.stackFrames[0].id,
      "n",
      /\b49\b/,
    );

    await continueExecution(session, thirdHit.threadId);
    await waitForDebugSessionEnd(LAUNCH_TIMEOUT_MS, session.id);
  });


  test("profiler links a real debugger variable and clears it on continue", async function () {
    this.timeout(TEST_TIMEOUT_MS);
    const { session, stop } = await launchToFirstStop([6]);
    const variables = await readFrameVariables(session, stop.stack.stackFrames[0].id);
    const variable = variables.find(item => item.name === "first");
    assert.ok(variable);
    await vscode.commands.executeCommand("osprey.profiler.inspectVariable", { sessionId: session.id, variable });
    assert.strictEqual(selectedProfileVariable.value?.sessionId, session.id);
    assert.strictEqual(selectedProfileVariable.value?.expression, "first");
    assert.match(selectedProfileVariable.value?.value ?? "", /25/);
    await vscode.commands.executeCommand("osprey.profiler.watchVariable");
    await continueExecution(session, stop.threadId);
    await waitForDebugSessionEnd(LAUNCH_TIMEOUT_MS, session.id);
    assert.strictEqual(selectedProfileVariable.value, undefined);
  });


  test("scopes and locals expose the program's computed values", async function () {
    this.timeout(TEST_TIMEOUT_MS);

    // Stop on the final print, by which point every let is assigned and live.
    const { session, stop } = await launchToFirstStop([6]);
    assertCurrentLine(stop.stack, 6, getSource());
    const frameId = stop.stack.stackFrames[0].id;

    const { scopes } = await getScopes(session, frameId);
    assert.ok(
      scopes.some((scope) => scope.variablesReference > 0),
      "the frame exposes at least one inspectable scope",
    );

    const names = (await readFrameVariables(session, frameId)).map(
      (v) => v.name,
    );
    assert.ok(
      names.includes("first") &&
        names.includes("second") &&
        names.includes("third"),
      `locals expose the top-level lets; saw ${names.join(", ")}`,
    );

    // square(5)=25, square(7)=49, addThree(49)=52.
    await assertLocalVariable(session, frameId, "first", /\b25\b/);
    await assertLocalVariable(session, frameId, "second", /\b49\b/);
    await assertLocalVariable(session, frameId, "third", /\b52\b/);

    await continueExecution(session, stop.threadId);
    await waitForDebugSessionEnd(LAUNCH_TIMEOUT_MS, session.id);
  });


  test("stopOnEntry pauses before user code, then resumes to completion", async function () {
    this.timeout(TEST_TIMEOUT_MS);

    // No breakpoints: the only reason we stop is stopOnEntry.
    const { session, stop } = await launchToFirstStop([], {
      stopOnEntry: true,
    });
    assert.ok(
      stop.stack.stackFrames.length > 0,
      "stopOnEntry paused with a live stack",
    );

    await continueExecution(session, stop.threadId);
    await waitForDebugSessionEnd(LAUNCH_TIMEOUT_MS, session.id);
    assert.ok(
      fs.existsSync(getDebugOutput()),
      "the debug launch compiled a native binary for the adapter",
    );
  });
}
