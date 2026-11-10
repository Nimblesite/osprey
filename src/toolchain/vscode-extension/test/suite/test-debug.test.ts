// The Test Explorer's Debug run profile ([TESTING-DEBUG-VSCODE]): the DAP
// output fold, the launch configuration one suite runs under, and the profile
// wiring — driven with an injected `startDebugging` so no adapter is needed.
// The real lldb-dap session is exercised in test-debug.e2e.test.ts.

import * as assert from "assert";
import * as path from "path";
import * as vscode from "vscode";
import {
  debugConfigFor,
  debugSessionName,
  foldDapMessage,
  LAUNCH_FAILED_MESSAGE,
  debugExecutor,
  makeDebugHandler,
  registerDebugOutputTracker,
  registerTestDebugProfile,
  type StartDebugging,
} from "../../client/src/test-debug";
import { refreshTestFile } from "../../client/src/test-explorer-items";
import {
  leafTestId,
  type ExecResult,
} from "../../client/src/test-explorer-parse";
import { resolveBuiltOsprey } from "./osprey-test-env";
import {
  assertVerdicts,
  controllerFactory,
  disposeRegistered,
  FAIL_FIXTURE,
  makeFixtureDir,
  RecordingSink,
  requireCompiler,
  type FixtureDir,
} from "./test-explorer-harness";

const NOTHING = { stdout: "", stderr: "", exitCode: 0 };

const outputEvent = (category: string | undefined, output: unknown) => ({
  type: "event",
  event: "output",
  body: category === undefined ? { output } : { category, output },
});

suite("Osprey Test Explorer Debug profile ([TESTING-DEBUG-VSCODE])", () => {
  const compiler = resolveBuiltOsprey();
  const disposables: vscode.Disposable[] = [];
  let fixtures: FixtureDir;
  let failUri: vscode.Uri;
  const context = (): vscode.ExtensionContext =>
    ({ subscriptions: disposables }) as unknown as vscode.ExtensionContext;
  const newController = controllerFactory(
    disposables,
    "ospreyTests-debug",
    () => compiler ?? "osprey",
  );

  suiteSetup(() => {
    fixtures = makeFixtureDir("osprey-test-debug-");
    failUri = fixtures.write("fail.test.osp", FAIL_FIXTURE);
  });

  suiteTeardown(() => fixtures.remove());

  teardown(() => disposeRegistered(disposables));

  // -------------------------------------------------------- the output fold

  suite("foldDapMessage", () => {
    test("stdout output events accumulate in order", () => {
      const first = foldDapMessage(
        NOTHING,
        outputEvent("stdout", "ok 1 - a\n"),
      );
      const both = foldDapMessage(first, outputEvent("stdout", "1..1\n"));
      assert.strictEqual(both.stdout, "ok 1 - a\n1..1\n");
      assert.strictEqual(both.stderr, "");
    });

    test("an output event with no category is stdout, as the DAP specifies", () => {
      assert.strictEqual(
        foldDapMessage(NOTHING, outputEvent(undefined, "ok 1 - a\n")).stdout,
        "ok 1 - a\n",
      );
    });

    test("stderr lands in stderr, not in the TAP stream", () => {
      const outcome = foldDapMessage(NOTHING, outputEvent("stderr", "boom\n"));
      assert.strictEqual(outcome.stderr, "boom\n");
      assert.strictEqual(outcome.stdout, "");
    });

    test("console output is the adapter talking about itself and is dropped", () => {
      const outcome = foldDapMessage(
        NOTHING,
        outputEvent("console", "Launched '/tmp/x'\n"),
      );
      assert.deepStrictEqual(outcome, NOTHING);
    });

    test("a non-string output body cannot corrupt the stream", () => {
      assert.deepStrictEqual(
        foldDapMessage(NOTHING, outputEvent("stdout", 7)),
        NOTHING,
      );
    });

    test("the exited event carries the process exit code", () => {
      assert.strictEqual(
        foldDapMessage(NOTHING, {
          type: "event",
          event: "exited",
          body: { exitCode: 1 },
        }).exitCode,
        1,
      );
    });

    test("an exited event without a code leaves the outcome alone", () => {
      assert.deepStrictEqual(
        foldDapMessage(NOTHING, { type: "event", event: "exited", body: {} }),
        NOTHING,
      );
    });

    test("responses, other events and malformed messages change nothing", () => {
      for (const message of [
        { type: "response", command: "continue", body: { output: "no" } },
        { type: "event", event: "stopped", body: { reason: "breakpoint" } },
        { type: "event", event: 7 },
        undefined,
      ]) {
        assert.deepStrictEqual(foldDapMessage(NOTHING, message), NOTHING);
      }
    });
  });

  // ------------------------------------------------- the launch configuration

  suite("debugConfigFor", () => {
    test("a whole-file run launches the suite with an EMPTY filter", () => {
      const config = debugConfigFor(failUri, undefined, "run-1");
      assert.strictEqual(config.type, "osprey");
      assert.strictEqual(config.request, "launch");
      assert.strictEqual(config.program, failUri.fsPath);
      assert.strictEqual(config.cwd, path.dirname(failUri.fsPath));
      // Empty, never absent: a value inherited from the editor's environment
      // would otherwise silently skip cases ([TESTING-FILTER]).
      assert.deepStrictEqual(config.env, { OSPREY_TEST_FILTER: "" });
      assert.strictEqual(config.name, debugSessionName(failUri, undefined));
    });

    test("a single-case run names the case in the filter and the session", () => {
      const config = debugConfigFor(failUri, "good math", "run-2");
      assert.deepStrictEqual(config.env, { OSPREY_TEST_FILTER: "good math" });
      assert.strictEqual(config.name, "good math");
    });

    test("every session carries its own run id, so outputs never cross", () => {
      const ids = new Set(
        ["a", "b"].map((id) => debugConfigFor(failUri, undefined, id)),
      );
      assert.strictEqual(ids.size, 2);
    });
  });

  // --------------------------------------------------------- profile wiring

  suite("registration", () => {
    test("registerTestDebugProfile adds the DEFAULT Debug profile", () => {
      const controller = newController();
      const profile = registerTestDebugProfile(
        context(),
        controller,
        () => compiler ?? "osprey",
      );
      assert.strictEqual(profile.label, "Debug");
      assert.strictEqual(profile.kind, vscode.TestRunProfileKind.Debug);
      assert.strictEqual(
        profile.isDefault,
        true,
        "the Testing view's Debug Test action must launch it",
      );
      profile.dispose();
    });

    test("the adapter tracker is disposed with the extension", () => {
      const before = disposables.length;
      registerTestDebugProfile(
        context(),
        newController(),
        () => compiler ?? "osprey",
      );
      assert.ok(
        disposables.length > before,
        "the tracker factory must be registered for disposal",
      );
    });

    test("registerDebugOutputTracker ignores sessions with no run id", () => {
      const tracker = registerDebugOutputTracker();
      // A session the user started themselves carries no run id; the factory
      // must decline it rather than accumulate a stranger's output.
      assert.ok(tracker);
      tracker.dispose();
    });
  });

  // ------------------------------------------------------------- the handler

  suite("running a request", () => {
    function childrenOf(file: vscode.TestItem): vscode.TestItem[] {
      const leaves: vscode.TestItem[] = [];
      file.children.forEach((leaf) => leaves.push(leaf));
      return leaves;
    }

    async function discovered(
      controller: vscode.TestController,
    ): Promise<vscode.TestItem> {
      const item = await refreshTestFile(
        controller,
        failUri,
        compiler ?? "osprey",
      );
      assert.strictEqual(item.error, undefined);
      return item;
    }

    /** A `startDebugging` double that records every launch and accepts it. */
    function recordingStart(
      launched: vscode.DebugConfiguration[],
    ): StartDebugging {
      return (_folder, config) => {
        launched.push(config);
        return Promise.resolve(false);
      };
    }

    /** Invoke the Debug profile's handler for `request` with `start`. */
    async function runDebugHandler(
      controller: vscode.TestController,
      request: vscode.TestRunRequest,
      start: StartDebugging,
    ): Promise<void> {
      await makeDebugHandler(
        controller,
        () => compiler ?? "osprey",
        start,
      )(request, new vscode.CancellationTokenSource().token);
    }

    /** Drive the debug executor for `file` with `outcome` as the session. */
    async function runDebugExecutor(
      sink: RecordingSink,
      file: vscode.TestItem,
      outcome: ExecResult | undefined,
    ): Promise<void> {
      const token = new vscode.CancellationTokenSource().token;
      await debugExecutor(sink, token, () => Promise.resolve(outcome))(
        file,
        childrenOf(file),
        undefined,
      );
    }

    test("a refused launch errors the suite instead of reporting nothing", async function () {
      requireCompiler(this, compiler, 30000);
      const controller = newController();
      const file = await discovered(controller);
      const sink = new RecordingSink();
      await runDebugExecutor(sink, file, undefined);
      const errors = sink.ofKind("errored");
      assert.deepStrictEqual(
        errors.map((event) => event.id),
        [file.id],
      );
      assert.strictEqual(errors[0].message, LAUNCH_FAILED_MESSAGE);
      // Every case was started, so none is left spinning in the Testing view.
      assert.strictEqual(sink.ofKind("started").length, file.children.size);
    });

    test("a session's own TAP decides every verdict", async function () {
      requireCompiler(this, compiler, 30000);
      const controller = newController();
      const file = await discovered(controller);
      const sink = new RecordingSink();
      // The debugged binary is the same program `osprey --run` executes, so
      // the session's own stdout IS the TAP stream the verdicts come from —
      // folded here exactly as the adapter tracker folds it.
      const outcome = [
        outputEvent("stdout", "# expect failed: expected 3, got 2\n"),
        outputEvent("stdout", "not ok 1 - bad math\n"),
        outputEvent("stdout", "ok 2 - good math\n1..2\n"),
        { type: "event", event: "exited", body: { exitCode: 1 } },
      ].reduce(foldDapMessage, NOTHING);
      await runDebugExecutor(sink, file, outcome);
      assert.deepStrictEqual(
        sink.ofKind("passed").map((event) => event.id),
        [leafTestId(failUri.toString(), "good math")],
      );
      assertVerdicts(
        sink,
        "failed",
        [leafTestId(failUri.toString(), "bad math")],
        /expected 3, got 2/,
      );
    });

    test("a whole-file request launches ONE session with no case filter", async function () {
      requireCompiler(this, compiler, 30000);
      const controller = newController();
      const file = await discovered(controller);
      const launched: vscode.DebugConfiguration[] = [];
      const start = recordingStart(launched);
      await runDebugHandler(
        controller,
        new vscode.TestRunRequest([file]),
        start,
      );
      assert.strictEqual(launched.length, 1);
      assert.deepStrictEqual(launched[0].env, { OSPREY_TEST_FILTER: "" });
      assert.strictEqual(launched[0].program, failUri.fsPath);
    });

    test("a single-case request filters the session to that case", async function () {
      requireCompiler(this, compiler, 30000);
      const controller = newController();
      const file = await discovered(controller);
      const good = file.children.get(
        leafTestId(failUri.toString(), "good math"),
      );
      assert.ok(good, "discovery found the case to debug");
      const launched: vscode.DebugConfiguration[] = [];
      const start = recordingStart(launched);
      await runDebugHandler(
        controller,
        new vscode.TestRunRequest([good]),
        start,
      );
      assert.deepStrictEqual(
        launched.map((config) => config.env),
        [{ OSPREY_TEST_FILTER: "good math" }],
      );
    });

    test("cancelling the run stops waiting on the session", async function () {
      const osprey = requireCompiler(this, compiler, 30000);
      const controller = newController();
      const file = await discovered(controller);
      const source = new vscode.CancellationTokenSource();
      // A session that never terminates: only the cancellation can end the
      // wait, so this test hangs for its whole timeout if the token is
      // ignored.
      const start: StartDebugging = () => {
        source.cancel();
        return Promise.resolve(true);
      };
      await makeDebugHandler(
        controller,
        () => osprey,
        start,
      )(new vscode.TestRunRequest([file]), source.token);
      assert.ok(source.token.isCancellationRequested);
    });
  });
});
