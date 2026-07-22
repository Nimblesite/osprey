// Integration tests for the Osprey Test Explorer ([TESTING-VSCODE]): discovery
// and run flows against a real TestController, temp-dir fixture files, and the
// freshly built osprey compiler. The pure parsing/planning helpers are covered
// in test-explorer-parse.test.ts.

import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import * as vscode from "vscode";
import { fileTestId, leafTestId } from "../../client/src/test-explorer-parse";
import {
  coverageSink,
  detailedCoverageFor,
  executeRunRequest,
  makeRunHandler,
  makeWatcherHandlers,
  refreshTestFile,
  registerOspreyTestExplorer,
  removeTestFile,
  requestedItems,
  scanWorkspaceTestFiles,
  testFileLabel,
} from "../../client/src/test-explorer";
import { resolveBuiltOsprey } from "./osprey-test-env";
import {
  BROKEN_FIXTURE,
  COVERAGE_FIXTURE,
  FAIL_FIXTURE,
  ML_FIXTURE,
  PASS_FIXTURE,
  RecordingSink,
  STRAY_FIXTURE,
} from "./test-explorer-harness";

suite("Osprey Test Explorer", () => {
  const compiler = resolveBuiltOsprey();
  let fixtureDir: string;
  let passUri: vscode.Uri;
  let failUri: vscode.Uri;
  let mlUri: vscode.Uri;
  let brokenUri: vscode.Uri;
  let strayUri: vscode.Uri;
  const disposables: vscode.Disposable[] = [];
  let controllerSequence = 0;

  function newController(
    resolveCompiler: () => string = () => compiler ?? "osprey",
  ): vscode.TestController {
    controllerSequence += 1;
    const context = {
      subscriptions: disposables,
    } as unknown as vscode.ExtensionContext;
    return registerOspreyTestExplorer(
      context,
      resolveCompiler,
      `ospreyTests-spec-${controllerSequence}`,
    );
  }

  function writeFixture(name: string, content: string): vscode.Uri {
    const filePath = path.join(fixtureDir, name);
    fs.writeFileSync(filePath, content);
    return vscode.Uri.file(filePath);
  }

  suiteSetup(() => {
    fixtureDir = fs.mkdtempSync(
      path.join(os.tmpdir(), "osprey-test-explorer-"),
    );
    passUri = writeFixture("pass.test.osp", PASS_FIXTURE);
    failUri = writeFixture("fail.test.osp", FAIL_FIXTURE);
    mlUri = writeFixture("ml.test.ospml", ML_FIXTURE);
    brokenUri = writeFixture("broken.test.osp", BROKEN_FIXTURE);
    strayUri = writeFixture("stray.test.osp", STRAY_FIXTURE);
  });

  suiteTeardown(() => {
    fs.rmSync(fixtureDir, { recursive: true, force: true });
  });

  teardown(() => {
    for (const disposable of disposables.splice(0)) {
      disposable.dispose();
    }
  });

  suite("discovery", () => {
    test("registerOspreyTestExplorer wires controller, run profile, and watcher", () => {
      const controller = newController();
      assert.strictEqual(controller.label, "Osprey Tests");
      assert.ok(controller.id.startsWith("ospreyTests-spec-"));
      assert.strictEqual(disposables.length, 2);
    });

    test("testFileLabel outside a workspace is the basename", () => {
      assert.strictEqual(testFileLabel(passUri), "pass.test.osp");
    });

    test("refreshTestFile discovers leaves with names, ids, and ranges", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const item = await refreshTestFile(controller, passUri, compiler);
      assert.strictEqual(item.error, undefined);
      assert.strictEqual(item.label, "pass.test.osp");
      assert.strictEqual(item.children.size, 2);
      const first = item.children.get(
        leafTestId(passUri.toString(), "addition works"),
      );
      assert.ok(first);
      assert.strictEqual(first.label, "addition works");
      assert.strictEqual(first.range?.start.line, 2);
      assert.strictEqual(first.range?.start.character, 0);
      const second = item.children.get(
        leafTestId(passUri.toString(), "zero identity"),
      );
      assert.strictEqual(second?.range?.start.line, 6);
    });

    test("refreshTestFile re-resolves children after edits", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const uri = writeFixture("edited.test.osp", PASS_FIXTURE);
      const before = await refreshTestFile(controller, uri, compiler);
      assert.strictEqual(before.children.size, 2);
      fs.writeFileSync(uri.fsPath, 'test("only one", fn() => expect(1, 1))\n');
      const after = await refreshTestFile(controller, uri, compiler);
      assert.strictEqual(after.children.size, 1);
      assert.strictEqual(after, before);
    });

    test("refreshTestFile discovers the ML flavor", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const item = await refreshTestFile(controller, mlUri, compiler);
      assert.strictEqual(item.children.size, 1);
      assert.ok(item.children.get(leafTestId(mlUri.toString(), "ml addition")));
    });

    test("a syntax error surfaces on the file item with no children", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const item = await refreshTestFile(controller, brokenUri, compiler);
      assert.match(String(item.error), /syntax error/);
      assert.strictEqual(item.children.size, 0);
    });

    test("a missing compiler surfaces as the file item's error", async () => {
      const controller = newController();
      const item = await refreshTestFile(
        controller,
        passUri,
        "/nonexistent/osprey-xyz",
      );
      assert.match(String(item.error), /ENOENT|nonexistent/);
    });

    test("removeTestFile and the watcher handlers add and drop items", async () => {
      const controller = newController();
      const handlers = makeWatcherHandlers(
        controller,
        () => "/nonexistent/osprey-xyz",
      );
      await handlers.refresh(passUri);
      assert.ok(controller.items.get(fileTestId(passUri.toString())));
      handlers.remove(passUri);
      assert.strictEqual(
        controller.items.get(fileTestId(passUri.toString())),
        undefined,
      );
      await refreshTestFile(controller, passUri, "/nonexistent/osprey-xyz");
      removeTestFile(controller, passUri);
      assert.strictEqual(controller.items.size, 0);
    });

    test("scanWorkspaceTestFiles seeds items from the finder", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      await scanWorkspaceTestFiles(
        controller,
        () => compiler,
        () => Promise.resolve([passUri]),
      );
      assert.strictEqual(
        controller.items.get(fileTestId(passUri.toString()))?.children.size,
        2,
      );
      const emptyHost = newController();
      await scanWorkspaceTestFiles(emptyHost, () => compiler ?? "osprey");
      assert.strictEqual(emptyHost.items.size, 0);
    });
  });

  suite("run", () => {
    function token(): vscode.CancellationToken {
      return new vscode.CancellationTokenSource().token;
    }

    async function discoveredFile(
      controller: vscode.TestController,
      uri: vscode.Uri,
    ): Promise<vscode.TestItem> {
      const item = await refreshTestFile(controller, uri, compiler ?? "osprey");
      assert.strictEqual(item.error, undefined);
      return item;
    }

    test("a whole-file run maps TAP results onto leaves with diagnostics", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const file = await discoveredFile(controller, failUri);
      const sink = new RecordingSink();
      const request = new vscode.TestRunRequest([file]);
      await executeRunRequest(
        controller,
        request,
        sink,
        token(),
        () => compiler,
      );
      const goodId = leafTestId(failUri.toString(), "good math");
      const badId = leafTestId(failUri.toString(), "bad math");
      assert.deepStrictEqual(
        sink.ofKind("passed").map((e) => e.id),
        [goodId],
      );
      const failures = sink.ofKind("failed");
      assert.deepStrictEqual(
        failures.map((e) => e.id),
        [badId],
      );
      assert.match(
        String(failures[0].message),
        /expect failed: expected 3, got 2/,
      );
      assert.strictEqual(sink.ofKind("enqueued").length, 2);
      assert.ok(sink.output.includes("\r\n"));
      assert.ok(sink.output.includes("not ok 1 - bad math"));
      assert.deepStrictEqual(sink.events[sink.events.length - 1], {
        kind: "end",
      });
    });

    test("a single requested leaf runs with OSPREY_TEST_FILTER", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const file = await discoveredFile(controller, failUri);
      const good = file.children.get(
        leafTestId(failUri.toString(), "good math"),
      );
      assert.ok(good);
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([good]),
        sink,
        token(),
        () => compiler,
      );
      assert.deepStrictEqual(
        sink.ofKind("passed").map((e) => e.id),
        [good.id],
      );
      assert.strictEqual(sink.ofKind("failed").length, 0);
      // The filter skipped "bad math": "good math" is the only executed case.
      assert.ok(sink.output.includes("ok 1 - good math"));
      assert.ok(!sink.output.includes("bad math"));
    });

    // [TESTING-COVERAGE-VSCODE]: a coverage run maps TAP as usual AND reports
    // per-line hits — the executed `double` covered, the dead `unused` at 0.
    test("a coverage run reports line hits including uncovered lines", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const coverageUri = writeFixture("covered.test.osp", COVERAGE_FIXTURE);
      const controller = newController();
      const file = await discoveredFile(controller, coverageUri);
      const sink = new RecordingSink();
      const request = new vscode.TestRunRequest([file]);
      await executeRunRequest(
        controller,
        request,
        sink,
        token(),
        () => compiler,
        true,
      );
      assert.strictEqual(sink.ofKind("passed").length, 1);
      const hits = sink.coverage.get(coverageUri.fsPath);
      assert.ok(hits, "coverage report reached the sink");
      assert.ok((hits.get(1) ?? 0) > 0, "double's definition line is covered");
      assert.strictEqual(hits.get(3), 0, "unused's definition line has 0 hits");
    });

    // A TestRun double recording exactly what coverageSink hands VS Code —
    // the FileCoverage whose TestCoverageCount becomes the displayed
    // percentage, plus the run lifecycle calls it delegates.
    function recordingRun(received: vscode.FileCoverage[]): vscode.TestRun {
      const noop = (): void => undefined;
      return {
        enqueued: noop,
        started: noop,
        passed: noop,
        failed: noop,
        errored: noop,
        skipped: noop,
        appendOutput: noop,
        end: noop,
        addCoverage: (fc: vscode.FileCoverage) => received.push(fc),
      } as unknown as vscode.TestRun;
    }

    // [TESTING-COVERAGE-VSCODE] calc proof, pure layer: the numbers VS Code
    // renders. hits {1:1, 3:0, 5:1} MUST become TestCoverageCount(2, 3) —
    // the 66.7% badge — and three gutter StatementCoverages at 0-based lines.
    test("coverageSink computes the exact FileCoverage counts and gutter detail", () => {
      const received: vscode.FileCoverage[] = [];
      const sink = coverageSink(recordingRun(received));
      assert.ok(sink.addLineCoverage, "coverage sink accepts line coverage");
      const uri = vscode.Uri.file("/tmp/calc.test.osp");
      sink.addLineCoverage(
        uri,
        new Map([
          [1, 1],
          [3, 0],
          [5, 1],
        ]),
      );
      assert.strictEqual(received.length, 1);
      const fc = received[0];
      assert.strictEqual(fc.uri.fsPath, uri.fsPath);
      assert.strictEqual(fc.statementCoverage.covered, 2, "covered lines");
      assert.strictEqual(fc.statementCoverage.total, 3, "coverable lines");
      const detail = detailedCoverageFor(fc);
      assert.deepStrictEqual(
        detail.map((s) => [(s.location as vscode.Position).line, s.executed]),
        [
          [0, 1],
          [2, 0],
          [4, 1],
        ],
        "gutter detail: 0-based lines with per-line hit counts",
      );
      assert.deepStrictEqual(
        detailedCoverageFor(
          new vscode.FileCoverage(uri, new vscode.TestCoverageCount(0, 0)),
        ),
        [],
        "unknown FileCoverage yields no detail",
      );
    });

    // [TESTING-COVERAGE-VSCODE] calc proof, end to end: the Coverage button's
    // exact path — coverageSink → executeRunRequest → real compiler →
    // --coverage-json → parsed hits → FileCoverage. The fixture has exactly 3
    // coverable lines (double:1, unused:3, test:5) and executes 2 of them, so
    // the run MUST surface covered=2/total=3 — the 66.7% VS Code displays.
    test("the Coverage profile path yields covered=2/total=3 for the fixture", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const coverageUri = writeFixture("calc-proof.test.osp", COVERAGE_FIXTURE);
      const controller = newController();
      const file = await discoveredFile(controller, coverageUri);
      const received: vscode.FileCoverage[] = [];
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([file]),
        coverageSink(recordingRun(received)),
        token(),
        () => compiler,
        true,
      );
      assert.strictEqual(received.length, 1, "one FileCoverage per suite file");
      const fc = received[0];
      assert.strictEqual(fc.uri.fsPath, coverageUri.fsPath);
      assert.strictEqual(fc.statementCoverage.covered, 2, "executed lines");
      assert.strictEqual(fc.statementCoverage.total, 3, "coverable lines");
      const zeroHit = detailedCoverageFor(fc).find(
        (s) => (s.location as vscode.Position).line === 2,
      );
      assert.strictEqual(zeroHit?.executed, 0, "dead fn renders as uncovered");
    });

    test("several leaves of one file run as sequential filtered invocations", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const file = await discoveredFile(controller, failUri);
      const bad = file.children.get(leafTestId(failUri.toString(), "bad math"));
      const good = file.children.get(
        leafTestId(failUri.toString(), "good math"),
      );
      assert.ok(bad && good);
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([bad, good]),
        sink,
        token(),
        () => compiler,
      );
      assert.deepStrictEqual(
        sink.ofKind("failed").map((e) => e.id),
        [bad.id],
      );
      assert.deepStrictEqual(
        sink.ofKind("passed").map((e) => e.id),
        [good.id],
      );
      const kinds = sink.events.map((e) => `${e.kind}:${e.id ?? ""}`);
      assert.ok(
        kinds.indexOf(`failed:${bad.id}`) <
          kinds.indexOf(`enqueued:${good.id}`),
      );
    });

    test("a compile error marks the file item errored with stderr", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const file = await refreshTestFile(controller, brokenUri, compiler);
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([file]),
        sink,
        token(),
        () => compiler,
      );
      const errors = sink.ofKind("errored");
      assert.deepStrictEqual(
        errors.map((e) => e.id),
        [file.id],
      );
      assert.match(String(errors[0].message), /syntax error/);
      assert.strictEqual(sink.ofKind("passed").length, 0);
    });

    test("a requested leaf absent from the output is skipped", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const file = await discoveredFile(controller, passUri);
      const ghost = controller.createTestItem(
        leafTestId(passUri.toString(), "ghost test"),
        "ghost test",
        passUri,
      );
      file.children.add(ghost);
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([ghost]),
        sink,
        token(),
        () => compiler,
      );
      assert.deepStrictEqual(
        sink.ofKind("skipped").map((e) => e.id),
        [ghost.id],
      );
    });

    test("a request without include runs every root", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      await discoveredFile(controller, passUri);
      const sink = new RecordingSink();
      const request = new vscode.TestRunRequest();
      assert.strictEqual(requestedItems(controller, request).length, 1);
      await executeRunRequest(
        controller,
        request,
        sink,
        token(),
        () => compiler,
      );
      assert.strictEqual(sink.ofKind("passed").length, 2);
    });

    test("a whole-file run resolves children on demand", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const bare = controller.createTestItem(
        fileTestId(passUri.toString()),
        "pass.test.osp",
        passUri,
      );
      controller.items.add(bare);
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([bare]),
        sink,
        token(),
        () => compiler,
      );
      assert.strictEqual(bare.children.size, 2);
      assert.strictEqual(sink.ofKind("passed").length, 2);
    });

    test("excluded leaves never execute during a whole-file run", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const file = await discoveredFile(controller, failUri);
      const bad = file.children.get(leafTestId(failUri.toString(), "bad math"));
      assert.ok(bad);
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([file], [bad]),
        sink,
        token(),
        () => compiler,
      );
      assert.strictEqual(sink.ofKind("failed").length, 0);
      assert.deepStrictEqual(
        sink.ofKind("passed").map((e) => e.id),
        [leafTestId(failUri.toString(), "good math")],
      );
      // The excluded case must not run at all — the run degrades to filtered
      // per-leaf invocations, so its TAP line never appears in the output.
      assert.ok(!sink.output.includes("bad math"));
      assert.strictEqual(sink.ofKind("started").length, 1);
    });

    test("a stray assertion failure outside any test marks the file failed", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const file = await discoveredFile(controller, strayUri);
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([file]),
        sink,
        token(),
        () => compiler,
      );
      // The one real case passed, but the run must not look green: the file
      // item fails with the stray diagnostic.
      assert.deepStrictEqual(
        sink.ofKind("passed").map((e) => e.id),
        [leafTestId(strayUri.toString(), "fine")],
      );
      const failures = sink.ofKind("failed");
      assert.deepStrictEqual(
        failures.map((e) => e.id),
        [file.id],
      );
      assert.match(
        String(failures[0].message),
        /expect failed: expected 5, got 2/,
      );
      assert.strictEqual(sink.ofKind("errored").length, 0);
    });

    test("an unfiltered run scrubs an inherited OSPREY_TEST_FILTER", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const file = await discoveredFile(controller, passUri);
      const sink = new RecordingSink();
      process.env.OSPREY_TEST_FILTER = "zero identity";
      try {
        await executeRunRequest(
          controller,
          new vscode.TestRunRequest([file]),
          sink,
          token(),
          () => compiler,
        );
      } finally {
        delete process.env.OSPREY_TEST_FILTER;
      }
      // Both cases ran despite the stray filter in the editor's environment.
      assert.strictEqual(sink.ofKind("passed").length, 2);
      assert.strictEqual(sink.ofKind("skipped").length, 0);
    });

    test("a pre-cancelled token produces no results", async function () {
      if (!compiler) {
        this.skip();
      }
      const controller = newController();
      const file = await discoveredFile(controller, passUri);
      const source = new vscode.CancellationTokenSource();
      source.cancel();
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([file]),
        sink,
        source.token,
        () => compiler,
      );
      assert.deepStrictEqual(sink.events, [{ kind: "end" }]);
    });

    test("cancellation mid-run kills the compiler and reports nothing", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const file = await discoveredFile(controller, passUri);
      const source = new vscode.CancellationTokenSource();
      // Cancel deterministically the instant the run reports `started` — that
      // fires before the compiler is awaited, so the post-run cancellation
      // guard always wins the race (a wall-clock timer here is flaky: a fast or
      // cached compile can finish and report a result before the timer lands).
      const sink = new RecordingSink();
      const cancellingSink = new Proxy(sink, {
        get(target, prop, receiver) {
          if (prop === "started") {
            return (test: vscode.TestItem) => {
              target.started(test);
              source.cancel();
            };
          }
          return Reflect.get(target, prop, receiver);
        },
      });
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([file]),
        cancellingSink,
        source.token,
        () => compiler,
      );
      const resultKinds = ["passed", "failed", "errored", "skipped"];
      assert.ok(
        sink.events.every((event) => !resultKinds.includes(event.kind)),
      );
      assert.deepStrictEqual(sink.events[sink.events.length - 1], {
        kind: "end",
      });
    });

    // The unstoppable-run regression: a throw anywhere in the run loop (a
    // rejected reporting call on a cancelled TestRun, a discovery error) must
    // STILL end the run — an un-ended run spins forever in the Testing view
    // and its Stop button is dead.
    test("a run whose reporting throws still ends (and rethrows)", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const file = await discoveredFile(controller, passUri);
      const sink = new RecordingSink();
      const boom = new Error("TestRun already ended");
      const throwingSink = new Proxy(sink, {
        get(target, prop, receiver) {
          if (prop === "enqueued") {
            return () => {
              throw boom;
            };
          }
          return Reflect.get(target, prop, receiver);
        },
      });
      await assert.rejects(
        executeRunRequest(
          controller,
          new vscode.TestRunRequest([file]),
          throwingSink,
          token(),
          () => compiler,
        ),
        boom,
      );
      assert.deepStrictEqual(sink.events[sink.events.length - 1], {
        kind: "end",
      });
    });

    test("an item without a uri is ignored gracefully", async () => {
      const controller = newController();
      const bare = controller.createTestItem("no-uri", "no uri");
      controller.items.add(bare);
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([bare]),
        sink,
        token(),
        () => "/nonexistent/osprey-xyz",
      );
      assert.deepStrictEqual(sink.events, [{ kind: "end" }]);
    });

    test("makeRunHandler drives a real TestRun end to end", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const file = await discoveredFile(controller, passUri);
      const good = file.children.get(
        leafTestId(passUri.toString(), "addition works"),
      );
      assert.ok(good);
      const handler = makeRunHandler(controller, () => compiler);
      await handler(new vscode.TestRunRequest([good]), token());
      assert.ok(true, "run handler completed against a real TestRun");
    });
  });
});
