// Native debugger launch, source mapping, and primitive locals:
// [DEBUGGER-BUILD] [DEBUGGER-SOURCE-MAP] [DEBUGGER-EDITOR-LAUNCH]
// [DEBUGGER-DBG-DECLARE].
import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";
import * as vscode from "vscode";
import { lexicalFixtures, mutableFixtures, sourceFixtures } from "./debug-fixtures";
import { defaultDebugOutputPath } from "../../client/src/extension";
import { resolveBuiltOsprey, resolveRequiredLldbDap } from "./osprey-test-env";
import {
  assertCurrentLine,
  assertFrameLocals,
  assertLocalVariable,
  assertWatch,
  clearDebugBreakpoints,
  continueExecution,
  getScopes,
  readFrameVariables,
  setSourceBreakpoints,
  stepIn,
  stepOut,
  stepOver,
  waitForDebugSessionEnd,
  waitForDebugSessionStart,
  waitForStop,
  type DapStop,
} from "./dap-harness";

const extensionId = "nimblesite.osprey";

// One multi-construct fixture exercises every debugger workflow: two pure
// functions (so we have real call frames to step in/out of), each called more
// than once (so a conditional breakpoint can pick a single call), and a tail of
// top-level lets (so the final frame has live locals to inspect). Line numbers
// are load-bearing — the assertions below reference them. Verified against the
// real lldb on this exact program before it was written.
const FIXTURE = [
  "fn square(n) = wrapMul(n, n)", // 1
  "fn addThree(n) = wrapAdd(n, 3)", // 2
  "let first = square(5)", // 3
  "let second = square(7)", // 4
  "let third = addThree(second)", // 5
  'print("first=${first} second=${second} third=${third}")', // 6
  "", // 7
].join("\n");

// Long enough for compile + lldb-dap launch + the stepping each test drives.
const LAUNCH_TIMEOUT_MS = 45_000;
const TEST_TIMEOUT_MS = 90_000;

suite("Osprey Debugger E2E Workflows", function () {
  // lldb-dap under CI load can hit a breakpoint before its expression
  // evaluator is ready; a failed condition evaluation makes lldb stop anyway
  // (its documented default), so a conditional breakpoint intermittently
  // stops on the wrong call. Retry shields that infrastructure flake — a
  // real regression still fails every attempt.
  this.retries(2);
  let tempDir: string;
  let source: string;
  let debugOutput: string;
  let lldbDapPath: string;
  let priorCompilerPath: string | undefined;
  let priorLldbDapPath: string | undefined;

  suiteSetup(async function () {
    this.timeout(60_000);
    tempDir = fs.mkdtempSync(
      path.join(require("os").tmpdir(), "osprey-debug-e2e-"),
    );
    source = path.join(tempDir, "workflows.osp");
    debugOutput = defaultDebugOutputPath(source);
    fs.writeFileSync(source, FIXTURE);

    const ospreyPath = resolveBuiltOsprey();
    assert.ok(
      ospreyPath && fs.existsSync(ospreyPath),
      "a freshly built osprey binary (target/release) is required for the debugger E2E",
    );
    lldbDapPath = resolveRequiredLldbDap();

    const config = vscode.workspace.getConfiguration("osprey");
    priorCompilerPath = config.get<string>("server.compilerPath");
    priorLldbDapPath = config.get<string>("debug.lldbDapPath");
    await config.update(
      "server.compilerPath",
      ospreyPath,
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

    const document = await vscode.workspace.openTextDocument(source);
    await vscode.window.showTextDocument(document);
    await document.save();
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
    if (tempDir && fs.existsSync(tempDir)) {
      fs.rmSync(tempDir, { recursive: true, force: true });
    }
  });

  teardown(async () => {
    clearDebugBreakpoints();
    if (vscode.debug.activeDebugSession) {
      try {
        await vscode.debug.stopDebugging();
      } catch {
        // A session may terminate naturally while cleanup races it.
      }
    }
  });

  // Set the given breakpoints, launch the real adapter, and wait for the first
  // stop. Every test starts here.
  async function launchToFirstStop(
    specs: Parameters<typeof setSourceBreakpoints>[1],
    overrides: Record<string, unknown> = {},
  ): Promise<{ session: vscode.DebugSession; stop: DapStop }> {
    const program = typeof overrides.program === "string" ? overrides.program : source;
    setSourceBreakpoints(program, specs);
    const sessionPromise = waitForDebugSessionStart(LAUNCH_TIMEOUT_MS);
    const started = await vscode.debug.startDebugging(undefined, {
      type: "osprey",
      request: "launch",
      name: "Osprey Debugger E2E",
      program: source,
      cwd: tempDir,
      debugOutput,
      lldbDapPath,
      stopOnEntry: false,
      ...overrides,
    });
    assert.ok(started, "VS Code accepted the Osprey debug launch");
    const session = await sessionPromise;
    assert.strictEqual(session.type, "osprey");
    const stop = await waitForStop(session, LAUNCH_TIMEOUT_MS);
    return { session, stop };
  }

  for (const extension of ["osp", "ospml"]) {
    test(`debug launch preserves requested build artifacts (${extension})`, async function () {
      this.timeout(TEST_TIMEOUT_MS);
      const program = path.join(tempDir, `artifacts.${extension}`);
      const output = defaultDebugOutputPath(program);
      fs.writeFileSync(program, extension === "osp"
        ? "fn main() = {\n    let value = 41\n    print(wrapAdd(value, 1))\n}\n"
        : "main () =\n    value = 41\n    print (wrapAdd value 1)\n");
      const { session, stop } = await launchToFirstStop([{ line: 3 }], {
        program, debugOutput: output, compilerPath: resolveBuiltOsprey(), preserveArtifacts: true,
      });
      await assertFrameLocals(session, stop, program, 3, "main", { value: 41 });
      const ir = fs.readFileSync(`${output}.ll`, "utf8");
      assert.ok(ir.includes("!DICompileUnit"), "the exact debug build retains its IR");
      assert.ok(ir.includes(`artifacts.${extension}`), "the retained IR belongs to the current source");
      if (process.platform === "darwin") assert.ok(fs.existsSync(`${output}.dSYM`));
    });
  }

  test("debug launch honors an explicit compiler override without fallback", async function () {
    this.timeout(TEST_TIMEOUT_MS);
    const started = await vscode.debug.startDebugging(undefined, {
      type: "osprey", request: "launch", name: "Compiler override",
      program: source, compilerPath: path.join(tempDir, "missing-compiler"), lldbDapPath,
    });
    assert.strictEqual(started, false, "a missing override must not silently select another compiler");
  });

  const topName = (stop: DapStop): string => stop.stack.stackFrames[0].name;
  const hasFrame = (stop: DapStop, name: string): boolean =>
    stop.stack.stackFrames.some((frame) => frame.name.includes(name));

  for (const [extension, text] of [
    ["osp", 'namespace "billing/api";\nmodule Tax {\n export fn add(n) = satAdd(n, 1)\n}\nfn main() = print(Tax::add(41))\n'],
    ["ospml", 'namespace "billing/api"\nmodule Tax\n    export add n = satAdd n 1\nmain () = print (Tax::add 41)\n'],
  ]) {
    test(`module stack frames retain their source names (${extension})`, async function () {
      this.timeout(TEST_TIMEOUT_MS);
      const program = path.join(tempDir, `module.${extension}`);
      fs.writeFileSync(program, text);
      const { session, stop } = await launchToFirstStop([3], {
        program, debugOutput: defaultDebugOutputPath(program),
      });
      const frame = assertCurrentLine(stop.stack, 3, program);
      assert.ok(frame.name.includes("billing/api::Tax::add"), frame.name);
      assert.ok(!frame.name.includes("__osp_"), "no encoded names in user frames");
      await assertLocalVariable(session, frame.id, "n", /\b41\b/);
      await continueExecution(session, stop.threadId);
      await waitForDebugSessionEnd(LAUNCH_TIMEOUT_MS, session.id);
    });
  }

  for (const fixture of sourceFixtures) {
    for (const [extension, text] of Object.entries(fixture.sources)) {
      test(`${fixture.label} (${extension})`, async function () {
        this.timeout(TEST_TIMEOUT_MS);
        const program = path.join(tempDir, `${fixture.prefix}.${extension}`);
        fs.writeFileSync(program, text);
        const { session, stop } = await launchToFirstStop([fixture.line], {
          program, debugOutput: defaultDebugOutputPath(program),
        });
        await assertFrameLocals(session, stop, program, fixture.line, fixture.prefix, fixture.locals);
        await continueExecution(session, stop.threadId);
        await waitForDebugSessionEnd(LAUNCH_TIMEOUT_MS, session.id);
      });
    }
  }

  for (const fixture of mutableFixtures) {
    for (const [extension, source] of Object.entries(fixture.sources)) {
      test(`${fixture.label} (${extension})`, async function () {
        this.timeout(TEST_TIMEOUT_MS);
        const program = path.join(tempDir, `cell.${extension}`);
        fs.writeFileSync(program, source.text);
        const { session, stop } = await launchToFirstStop([source.line], {
          program, debugOutput: defaultDebugOutputPath(program),
        });
        await assertCellState(session, stop, program, source.line, fixture.prefix, fixture.arguments[0], 42, fixture.record);
        await continueExecution(session, stop.threadId);
        const second = await waitForStop(session, LAUNCH_TIMEOUT_MS);
        await assertCellState(session, second, program, source.line, fixture.prefix, fixture.arguments[1], 43, fixture.record);
        await continueExecution(session, second.threadId);
        await waitForDebugSessionEnd(LAUNCH_TIMEOUT_MS, session.id);
      });
    }
  }

  async function assertCellState(session: vscode.DebugSession, stop: DapStop, program: string, line: number, prefix: string, x: number, n: number, record: boolean) {
    await assertFrameLocals(session, stop, program, line, prefix, {
      x, n: record ? { count: n } : n, ...(prefix === "__closure_fn_" ? { sum: n } : {}),
    });
  }


  for (const fixture of lexicalFixtures) {
    test(`${fixture.label} (${fixture.extension})`, async function () {
      this.timeout(TEST_TIMEOUT_MS);
      const program = path.join(tempDir, `pattern.${fixture.extension}`);
      fs.writeFileSync(program, fixture.text);
      const { session, stop } = await launchToFirstStop([fixture.innerLine, fixture.outerLine], {
        program, debugOutput: defaultDebugOutputPath(program),
      });
      const inside = await assertFrameLocals(session, stop, program, fixture.innerLine, fixture.prefix, fixture.innerLocals);
      await assertWatch(session, inside, "value", new RegExp(`^${fixture.innerValue}$`));
      const bindings = await readFrameVariables(session, inside);
      assert.ok(!bindings.some(variable => ["selected", "outside"].includes(variable.name)), "bindings whose initializers have not completed must stay out of scope");
      assert.ok(bindings.some(variable => new RegExp(`^value(?: @ .*:${fixture.bindingLine})?$`).test(variable.name) && variable.value === String(fixture.innerValue)), "the visible scope exposes its initialized value binding");
      if (fixture.uniqueValue) assert.strictEqual(bindings.filter(variable => /^value(?: @ .*)?$/.test(variable.name)).length, 1, "an incomplete initializer cannot shadow the enclosing debugger binding");
      await continueExecution(session, stop.threadId);
      const outside = await waitForStop(session, LAUNCH_TIMEOUT_MS);
      const frame = await assertFrameLocals(session, outside, program, fixture.outerLine, fixture.prefix, { value: 100, selected: fixture.selected, outside: 100 + fixture.selected });
      assert.ok(!(await readFrameVariables(session, frame)).some(variable => variable.name === "observed"), "inner locals leave scope after the expression");
      await continueExecution(session, outside.threadId);
      await waitForDebugSessionEnd(LAUNCH_TIMEOUT_MS, session.id);
    });
  }

  test("conditional breakpoint stops only on the matching call, with detailed watch", async function () {
    this.timeout(TEST_TIMEOUT_MS);

    // square is called with 5 then 7; the condition must skip the n==5 call and
    // stop on n==7 only.
    const { session, stop } = await launchToFirstStop([
      { line: 1, condition: "n == 7" },
    ]);
    const top = assertCurrentLine(stop.stack, 1, source);
    assert.ok(top.column >= 1, "DAP reports a 1-based source column");
    assert.ok(hasFrame(stop, "square"), "stopped inside the square frame");
    assert.ok(hasFrame(stop, "main"), "the caller main is on the stack");

    // The parameter proves the condition was honoured (n is 7, never 5).
    await assertLocalVariable(session, top.id, "n", /\b7\b/);

    // DETAILED WATCH — arithmetic, multiplication, comparison, equality — each
    // evaluated in the stopped frame through the real adapter.
    await assertWatch(session, top.id, "n", /\b7\b/);
    await assertWatch(session, top.id, "n * n", /\b49\b/);
    await assertWatch(session, top.id, "n + 3", /\b10\b/);
    await assertWatch(session, top.id, "n - 1", /\b6\b/);
    await assertWatch(session, top.id, "n * 2", /\b14\b/);
    await assertWatch(session, top.id, "n > 5", /true|1/);
    await assertWatch(session, top.id, "n == 7", /true|1/);
    await assertWatch(session, top.id, "n == 5", /false|0/);

    await continueExecution(session, stop.threadId);
    await waitForDebugSessionEnd(LAUNCH_TIMEOUT_MS, session.id);
  });

  test("step in descends into the callee; step out returns to the caller", async function () {
    this.timeout(TEST_TIMEOUT_MS);

    // Stop at the addThree(second) call site in main.
    const { session, stop } = await launchToFirstStop([5]);
    assertCurrentLine(stop.stack, 5, source);
    assert.ok(hasFrame(stop, "main"), "first stop is in main");

    // Step IN: we land inside addThree.
    await stepIn(session, stop.threadId);
    const inside = await waitForStop(session, LAUNCH_TIMEOUT_MS);
    assertCurrentLine(inside.stack, 2, source);
    assert.ok(
      hasFrame(inside, "addThree"),
      `stepped into addThree (was ${topName(inside)})`,
    );
    // second = square(7) = 49, so addThree sees n == 49.
    await assertLocalVariable(
      session,
      inside.stack.stackFrames[0].id,
      "n",
      /\b49\b/,
    );
    await assertWatch(
      session,
      inside.stack.stackFrames[0].id,
      "n + 3",
      /\b52\b/,
    );

    // Step OUT: we return to the caller, main.
    await stepOut(session, inside.threadId);
    const back = await waitForStop(session, LAUNCH_TIMEOUT_MS);
    assert.ok(
      hasFrame(back, "main"),
      `stepped back out to main (was ${topName(back)})`,
    );

    await continueExecution(session, back.threadId);
    await waitForDebugSessionEnd(LAUNCH_TIMEOUT_MS, session.id);
  });

  test("step over executes a call without descending into it", async function () {
    this.timeout(TEST_TIMEOUT_MS);

    // Stop at the first square(5) call site in main.
    const { session, stop } = await launchToFirstStop([3]);
    assertCurrentLine(stop.stack, 3, source);

    // Step OVER the call: we stay in main and advance to line 4 (not into square).
    await stepOver(session, stop.threadId);
    const afterFirst = await waitForStop(session, LAUNCH_TIMEOUT_MS);
    assert.ok(hasFrame(afterFirst, "main"), "step over stayed in main");
    assertCurrentLine(afterFirst.stack, 4, source);

    // Step OVER again to line 5.
    await stepOver(session, afterFirst.threadId);
    const afterSecond = await waitForStop(session, LAUNCH_TIMEOUT_MS);
    assertCurrentLine(afterSecond.stack, 5, source);

    await continueExecution(session, afterSecond.threadId);
    await waitForDebugSessionEnd(LAUNCH_TIMEOUT_MS, session.id);
  });

  test("multiple breakpoints across functions are each hit in order", async function () {
    this.timeout(TEST_TIMEOUT_MS);

    // Breakpoints in BOTH functions; continue walks through every hit.
    const { session, stop } = await launchToFirstStop([1, 2]);

    // First hit: square(5).
    assertCurrentLine(stop.stack, 1, source);
    await assertLocalVariable(
      session,
      stop.stack.stackFrames[0].id,
      "n",
      /\b5\b/,
    );

    // Second hit: square(7).
    await continueExecution(session, stop.threadId);
    const secondHit = await waitForStop(session, LAUNCH_TIMEOUT_MS);
    assertCurrentLine(secondHit.stack, 1, source);
    await assertLocalVariable(
      session,
      secondHit.stack.stackFrames[0].id,
      "n",
      /\b7\b/,
    );

    // Third hit: addThree(second) where second == 49.
    await continueExecution(session, secondHit.threadId);
    const thirdHit = await waitForStop(session, LAUNCH_TIMEOUT_MS);
    assertCurrentLine(thirdHit.stack, 2, source);
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

  test("scopes and locals expose the program's computed values", async function () {
    this.timeout(TEST_TIMEOUT_MS);

    // Stop on the final print, by which point every let is assigned and live.
    const { session, stop } = await launchToFirstStop([6]);
    assertCurrentLine(stop.stack, 6, source);
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
      fs.existsSync(debugOutput),
      "the debug launch compiled a native binary for the adapter",
    );
  });
});
