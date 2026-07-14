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
  executeRunRequest, makeRunHandler, makeWatcherHandlers, refreshTestFile,
  registerOspreyTestExplorer, removeTestFile, requestedItems,
  scanWorkspaceTestFiles, testFileLabel,
} from "../../client/src/test-explorer";
import { resolveBuiltOsprey } from "./osprey-test-env";
import {
  BROKEN_FIXTURE, FAIL_FIXTURE, ML_FIXTURE, PASS_FIXTURE, RecordingSink,
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
    const context = { subscriptions: disposables } as unknown as vscode.ExtensionContext;
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
    fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-test-explorer-"));
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
      const first = item.children.get(leafTestId(passUri.toString(), "addition works"));
      assert.ok(first);
      assert.strictEqual(first.label, "addition works");
      assert.strictEqual(first.range?.start.line, 2);
      assert.strictEqual(first.range?.start.character, 0);
      const second = item.children.get(leafTestId(passUri.toString(), "zero identity"));
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
      const item = await refreshTestFile(controller, passUri, "/nonexistent/osprey-xyz");
      assert.match(String(item.error), /ENOENT|nonexistent/);
    });

    test("removeTestFile and the watcher handlers add and drop items", async () => {
      const controller = newController();
      const handlers = makeWatcherHandlers(controller, () => "/nonexistent/osprey-xyz");
      await handlers.refresh(passUri);
      assert.ok(controller.items.get(fileTestId(passUri.toString())));
      handlers.remove(passUri);
      assert.strictEqual(controller.items.get(fileTestId(passUri.toString())), undefined);
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
      await scanWorkspaceTestFiles(controller, () => compiler, () =>
        Promise.resolve([passUri]),
      );
      assert.strictEqual(controller.items.get(fileTestId(passUri.toString()))?.children.size, 2);
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
      await executeRunRequest(controller, request, sink, token(), () => compiler);
      const goodId = leafTestId(failUri.toString(), "good math");
      const badId = leafTestId(failUri.toString(), "bad math");
      assert.deepStrictEqual(sink.ofKind("passed").map((e) => e.id), [goodId]);
      const failures = sink.ofKind("failed");
      assert.deepStrictEqual(failures.map((e) => e.id), [badId]);
      assert.match(String(failures[0].message), /expect failed: expected 3, got 2/);
      assert.strictEqual(sink.ofKind("enqueued").length, 2);
      assert.ok(sink.output.includes("\r\n"));
      assert.ok(sink.output.includes("not ok 1 - bad math"));
      assert.deepStrictEqual(sink.events[sink.events.length - 1], { kind: "end" });
    });

    test("a single requested leaf runs with OSPREY_TEST_FILTER", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const file = await discoveredFile(controller, failUri);
      const good = file.children.get(leafTestId(failUri.toString(), "good math"));
      assert.ok(good);
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([good]),
        sink,
        token(),
        () => compiler,
      );
      assert.deepStrictEqual(sink.ofKind("passed").map((e) => e.id), [good.id]);
      assert.strictEqual(sink.ofKind("failed").length, 0);
      // The filter skipped "bad math": "good math" is the only executed case.
      assert.ok(sink.output.includes("ok 1 - good math"));
      assert.ok(!sink.output.includes("bad math"));
    });

    test("several leaves of one file run as sequential filtered invocations", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const file = await discoveredFile(controller, failUri);
      const bad = file.children.get(leafTestId(failUri.toString(), "bad math"));
      const good = file.children.get(leafTestId(failUri.toString(), "good math"));
      assert.ok(bad && good);
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([bad, good]),
        sink,
        token(),
        () => compiler,
      );
      assert.deepStrictEqual(sink.ofKind("failed").map((e) => e.id), [bad.id]);
      assert.deepStrictEqual(sink.ofKind("passed").map((e) => e.id), [good.id]);
      const kinds = sink.events.map((e) => `${e.kind}:${e.id ?? ""}`);
      assert.ok(kinds.indexOf(`failed:${bad.id}`) < kinds.indexOf(`enqueued:${good.id}`));
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
      assert.deepStrictEqual(errors.map((e) => e.id), [file.id]);
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
      assert.deepStrictEqual(sink.ofKind("skipped").map((e) => e.id), [ghost.id]);
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
      await executeRunRequest(controller, request, sink, token(), () => compiler);
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
      assert.deepStrictEqual(failures.map((e) => e.id), [file.id]);
      assert.match(String(failures[0].message), /expect failed: expected 5, got 2/);
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
      const sink = new RecordingSink();
      const running = executeRunRequest(
        controller,
        new vscode.TestRunRequest([file]),
        sink,
        source.token,
        () => compiler,
      );
      setTimeout(() => source.cancel(), 50);
      await running;
      const resultKinds = ["passed", "failed", "errored", "skipped"];
      assert.ok(sink.events.every((event) => !resultKinds.includes(event.kind)));
      assert.deepStrictEqual(sink.events[sink.events.length - 1], { kind: "end" });
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
      const good = file.children.get(leafTestId(passUri.toString(), "addition works"));
      assert.ok(good);
      const handler = makeRunHandler(controller, () => compiler);
      await handler(new vscode.TestRunRequest([good]), token());
      assert.ok(true, "run handler completed against a real TestRun");
    });
  });
});
