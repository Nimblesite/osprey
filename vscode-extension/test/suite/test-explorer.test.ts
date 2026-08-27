// Integration tests for the Osprey Test Explorer ([TESTING-VSCODE]): discovery
// and run flows against a real TestController, temp-dir fixture files, and the
// freshly built osprey compiler. The pure parsing/planning helpers are covered
// in test-explorer-parse.test.ts.

import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import * as sinon from "sinon";
import * as vscode from "vscode";
import {
  COVERAGE_RUN,
  fileTestId,
  leafTestId,
  PLAIN_RUN,
} from "../../client/src/test-explorer-parse";
import {
  coverageSink,
  detailedCoverageFor,
  executeRunRequest,
  makeRunHandler,
  makeWatcherHandlers,
  refreshTestFile,
  registerOspreyTestExplorer,
  removeTestFile,
  runCompiler,
  requestedItems,
  scanWorkspaceTestFiles,
  testFileLabel,
} from "../../client/src/test-explorer";
import { resolveBuiltOsprey } from "./osprey-test-env";
import {
  BROKEN_FIXTURE,
  COVERAGE_FIXTURE,
  EXPLAINED_SKIP_FIXTURE,
  FAIL_FIXTURE,
  ML_FIXTURE,
  AMBIGUOUS_NAME_FIXTURE,
  MIXED_FIXTURE,
  ML_SKIP_FIXTURE,
  PASS_FIXTURE,
  RecordingSink,
  SKIP_FIXED_FIXTURE,
  SKIP_FIXTURE,
  SKIP_REPARKED_FIXTURE,
  STRAY_FIXTURE,
  UNEXPLAINED_SKIP_FIXTURE,
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
      assert.strictEqual(disposables.length, 3);
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

    test("refreshTestFile groups workspace test files by folder", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const workspaceUri = vscode.Uri.file(fixtureDir);
      const testDir = path.join(fixtureDir, "tests", "core", "arithmetic");
      fs.mkdirSync(testDir, { recursive: true });
      const uri = vscode.Uri.file(path.join(testDir, "calculator.test.osp"));
      fs.writeFileSync(uri.fsPath, PASS_FIXTURE);
      const workspaceFolder = {
        uri: workspaceUri,
        name: "fixture",
        index: 0,
      } satisfies vscode.WorkspaceFolder;
      const getWorkspaceFolder = sinon
        .stub(vscode.workspace, "getWorkspaceFolder")
        .returns(workspaceFolder);
      const asRelativePath = sinon
        .stub(vscode.workspace, "asRelativePath")
        .callsFake((resource: vscode.Uri | string) =>
          path.relative(
            fixtureDir,
            typeof resource === "string" ? resource : resource.fsPath,
          ),
        );
      try {
        const file = await refreshTestFile(controller, uri, compiler);
        const tests = [...controller.items].map(([, item]) => item);
        assert.strictEqual(tests.length, 1);
        assert.strictEqual(tests[0].label, "tests");
        const core = [...tests[0].children].map(([, item]) => item);
        assert.strictEqual(core.length, 1);
        assert.strictEqual(core[0].label, "core");
        const arithmetic = [...core[0].children].map(([, item]) => item);
        assert.strictEqual(arithmetic.length, 1);
        assert.strictEqual(arithmetic[0].label, "arithmetic");
        assert.strictEqual(arithmetic[0].children.get(file.id), file);
        assert.strictEqual(file.label, "calculator.test.osp");
      } finally {
        getWorkspaceFolder.restore();
        asRelativePath.restore();
      }
    });

    // removeTestFile has to find a file item that is NOT a direct child of the
    // controller — inside a workspace every test file hangs off a chain of
    // directory items — and then leave no empty directories behind. Both arms
    // of the prune matter: an inner directory is dropped from its parent, the
    // outermost from the controller itself. A stale empty folder in the
    // Explorer is how a deleted test file appears to still exist.
    test("removeTestFile prunes the directory chain it emptied", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const controller = newController();
      const workspaceUri = vscode.Uri.file(fixtureDir);
      const nestedDir = path.join(fixtureDir, "suites", "unit");
      fs.mkdirSync(nestedDir, { recursive: true });
      const uri = vscode.Uri.file(path.join(nestedDir, "nested.test.osp"));
      fs.writeFileSync(uri.fsPath, PASS_FIXTURE);
      const workspaceFolder = {
        uri: workspaceUri,
        name: "fixture",
        index: 0,
      } satisfies vscode.WorkspaceFolder;
      const getWorkspaceFolder = sinon
        .stub(vscode.workspace, "getWorkspaceFolder")
        .returns(workspaceFolder);
      const asRelativePath = sinon
        .stub(vscode.workspace, "asRelativePath")
        .callsFake((resource: vscode.Uri | string) =>
          path.relative(
            fixtureDir,
            typeof resource === "string" ? resource : resource.fsPath,
          ),
        );
      try {
        // The workspace-relative path is the label, not the basename.
        assert.strictEqual(
          testFileLabel(uri),
          path.join("suites", "unit", "nested.test.osp"),
        );
        const file = await refreshTestFile(controller, uri, compiler);
        // Discovering the same file twice must reuse the chain, not clone it.
        await refreshTestFile(controller, uri, compiler);
        const suites = [...controller.items].map(([, item]) => item);
        assert.strictEqual(suites.length, 1);
        const unit = [...suites[0].children].map(([, item]) => item);
        assert.strictEqual(unit.length, 1);
        assert.strictEqual(unit[0].children.get(file.id), file);

        // A file the tree has never seen is a no-op, not a throw.
        removeTestFile(
          controller,
          vscode.Uri.file(path.join(nestedDir, "ghost.test.osp")),
        );
        assert.strictEqual(controller.items.size, 1);

        removeTestFile(controller, uri);
        assert.strictEqual(controller.items.size, 0);
      } finally {
        getWorkspaceFolder.restore();
        asRelativePath.restore();
        fs.rmSync(path.join(fixtureDir, "suites"), {
          recursive: true,
          force: true,
        });
      }
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
    test("runCompiler closes child stdin so input reaches EOF", async function () {
      this.timeout(3000);
      const source = new vscode.CancellationTokenSource();
      const cancel = setTimeout(() => source.cancel(), 500);
      try {
        const result = await runCompiler(
          process.execPath,
          [
            "-e",
            "process.stdin.resume(); process.stdin.on('end', () => process.exit(0));",
          ],
          fixtureDir,
          process.env,
          source.token,
        );
        assert.strictEqual(
          result.exitCode,
          0,
          "child waited for stdin EOF until the test cancelled it",
        );
      } finally {
        clearTimeout(cancel);
        source.dispose();
      }
    });

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

    test("a failed test peek includes complete Context For AI details", async function () {
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
      const expectedContext = [
        "## Context For AI",
        "",
        `- File: ${failUri.fsPath}`,
        "- Test: bad math",
        "- Status: failed",
        "- Location: line 3, column 1",
        "- Failure: expect failed: expected 3, got 2",
      ].join("\n");
      assert.ok(
        String(failures[0].message).includes(expectedContext),
        `missing complete AI context in:\n${String(failures[0].message)}`,
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

    // [TESTING-SKIP-WARNING]: NO test skips silently. A case the TAP flags
    // `# SKIP` must land a Warning diagnostic on its `test` line — visible in
    // the Problems panel via vscode.languages.getDiagnostics — and a case that
    // runs again must clear its own warning.
    test("a skipped case raises a Warning diagnostic and a revived one clears it", async function () {
      if (!compiler) {
        this.skip();
      }
      // Two full compile+run cycles (skip, then revive) — twice the budget of
      // the single-run tests around it.
      this.timeout(120000);
      const skipUri = writeFixture("skips.test.osp", SKIP_FIXTURE);
      const controller = newController();
      const file = await discoveredFile(controller, skipUri);
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([file]),
        sink,
        token(),
        () => compiler,
      );
      const parkedId = leafTestId(skipUri.toString(), "parked case");
      assert.deepStrictEqual(
        sink.ofKind("skipped").map((e) => e.id),
        [parkedId],
      );
      // The warning is published through a REAL DiagnosticCollection, so the
      // editor's own diagnostics channel (Problems panel, squiggles) sees it.
      const published = vscode.languages
        .getDiagnostics(skipUri)
        .filter((d) => d.code === "test-skipped");
      assert.strictEqual(published.length, 1, JSON.stringify(published));
      assert.strictEqual(
        published[0].severity,
        vscode.DiagnosticSeverity.Warning,
      );
      assert.strictEqual(published[0].source, "osprey tests");
      assert.strictEqual(
        published[0].message,
        "Test 'parked case' was skipped: blocked on #123",
      );
      assert.strictEqual(
        published[0].range.start.line,
        2,
        "warning sits on the test call's line",
      );

      // Revive the case and re-run: the warning must clear.
      fs.writeFileSync(skipUri.fsPath, SKIP_FIXED_FIXTURE);
      const revived = await discoveredFile(controller, skipUri);
      const secondSink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([revived]),
        secondSink,
        token(),
        () => compiler,
      );
      assert.strictEqual(secondSink.ofKind("skipped").length, 0);
      assert.deepStrictEqual(
        vscode.languages
          .getDiagnostics(skipUri)
          .filter((d) => d.code === "test-skipped"),
        [],
        "revived case cleared its skip warning",
      );
    });

    // [TESTING-SKIP-REASON] a skip that names no reason is an ERROR, not a
    // warning: the case still reports skipped in the TAP, but the diagnostic
    // it publishes is the strongest one the editor has.
    test("a skip with no reason raises an Error diagnostic", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(120000);
      const uri = writeFixture(
        "unexplained.test.osp",
        UNEXPLAINED_SKIP_FIXTURE,
      );
      const controller = newController();
      const file = await discoveredFile(controller, uri);
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([file]),
        sink,
        token(),
        () => compiler,
      );
      const published = vscode.languages
        .getDiagnostics(uri)
        .filter((d) => d.code === "test-skipped");
      assert.strictEqual(published.length, 1, JSON.stringify(published));
      assert.strictEqual(
        published[0].severity,
        vscode.DiagnosticSeverity.Error,
        "an unexplained skip is an error, not a warning",
      );
      assert.strictEqual(
        published[0].message,
        "Test 'unexplained case' was skipped with no reason; every skip must name one",
      );
      // Giving the SAME case a reason downgrades it to a warning — the reason
      // is what decides the strength, nothing else.
      fs.writeFileSync(uri.fsPath, EXPLAINED_SKIP_FIXTURE);
      const revived = await discoveredFile(controller, uri);
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([revived]),
        new RecordingSink(),
        token(),
        () => compiler,
      );
      const reasoned = vscode.languages
        .getDiagnostics(uri)
        .filter((d) => d.code === "test-skipped");
      assert.strictEqual(reasoned.length, 1, JSON.stringify(reasoned));
      assert.strictEqual(
        reasoned[0].severity,
        vscode.DiagnosticSeverity.Warning,
      );
      assert.strictEqual(
        reasoned[0].message,
        "Test 'unexplained case' was skipped: blocked on #456",
      );
    });

    // [TESTING-SKIP-WARNING] skipped/ignored are the same: a discovered case
    // that never appears in its run's TAP at all (ignored) warns too, and
    // deleting the file drops its warnings.
    test("a case missing from the TAP warns as ignored; removal clears warnings", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(120000);
      const ghostUri = writeFixture("ghost.test.osp", PASS_FIXTURE);
      const controller = newController();
      const file = await discoveredFile(controller, ghostUri);
      assert.strictEqual(file.children.size, 2);
      // Drop one discovered case from the file AFTER discovery: the run's TAP
      // never mentions it, which is an ignored test.
      fs.writeFileSync(
        ghostUri.fsPath,
        'fn add(a, b) = a + b\n\ntest("addition works", fn() => expect(add(2, 3), 5))\n',
      );
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([file]),
        sink,
        token(),
        () => compiler,
      );
      const ghosts = vscode.languages
        .getDiagnostics(ghostUri)
        .filter((d) => d.code === "test-skipped");
      assert.strictEqual(ghosts.length, 1, JSON.stringify(ghosts));
      assert.strictEqual(
        ghosts[0].message,
        "Test 'zero identity' did not run (skipped/ignored)",
      );
      removeTestFile(controller, ghostUri);
      assert.deepStrictEqual(
        vscode.languages
          .getDiagnostics(ghostUri)
          .filter((d) => d.code === "test-skipped"),
        [],
        "removing the file drops its skip warnings",
      );
    });

    /** Every skip warning published for `uri`, sorted by line. */
    const skipWarnings = (uri: vscode.Uri): vscode.Diagnostic[] =>
      vscode.languages
        .getDiagnostics(uri)
        .filter((d) => d.code === "test-skipped")
        .sort((a, b) => a.range.start.line - b.range.start.line);

    /** Run every case of `file` through a fresh sink and return it. */
    async function runWholeFile(
      controller: vscode.TestController,
      file: vscode.TestItem,
      mode = PLAIN_RUN,
    ): Promise<RecordingSink> {
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([file]),
        sink,
        token(),
        () => compiler ?? "osprey",
        mode,
      );
      return sink;
    }

    // [TESTING-SKIP-WARNING] one suite, every outcome: a pass, a failure, a
    // STATIC skip and a DYNAMIC skip. Exactly the two skips warn — a failure
    // is a failure, not a skip, and a pass warns about nothing.
    test("a mixed suite warns for every skip and for nothing else", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(120000);
      const mixedUri = writeFixture("mixed.test.osp", MIXED_FIXTURE);
      const controller = newController();
      const file = await discoveredFile(controller, mixedUri);
      assert.strictEqual(file.children.size, 4, "all four cases discovered");
      const sink = await runWholeFile(controller, file);

      const id = (name: string) => leafTestId(mixedUri.toString(), name);
      // --- Verdicts reach the Test Explorer intact.
      assert.deepStrictEqual(
        sink.ofKind("passed").map((e) => e.id),
        [id("passes cleanly")],
      );
      assert.deepStrictEqual(
        sink.ofKind("failed").map((e) => e.id),
        [id("fails loudly")],
      );
      assert.deepStrictEqual(
        sink
          .ofKind("skipped")
          .map((e) => e.id)
          .sort(),
        [id("dynamically parked"), id("statically parked")].sort(),
        "BOTH the static and the dynamic skip report as skipped",
      );
      assert.strictEqual(sink.ofKind("errored").length, 0);
      assert.strictEqual(sink.ofKind("enqueued").length, 4);
      assert.strictEqual(sink.ofKind("started").length, 4);

      // --- The failure keeps its own diagnostic, unpolluted by skip handling.
      assert.match(
        String(sink.ofKind("failed")[0].message),
        /expect failed: expected 3, got 1/,
      );

      // --- Exactly two warnings, one per skip, each on its own `test` line.
      const warnings = skipWarnings(mixedUri);
      assert.strictEqual(
        warnings.length,
        2,
        `only the skips warn: ${JSON.stringify(warnings.map((w) => w.message))}`,
      );
      assert.deepStrictEqual(
        warnings.map((w) => w.range.start.line),
        [11, 13],
        "warnings land on the two `test(` lines",
      );
      assert.deepStrictEqual(
        warnings.map((w) => w.message),
        [
          "Test 'statically parked' was skipped: static reason",
          "Test 'dynamically parked' was skipped: runtime precondition unmet",
        ],
      );
      for (const warning of warnings) {
        assert.strictEqual(warning.severity, vscode.DiagnosticSeverity.Warning);
        assert.strictEqual(warning.source, "osprey tests");
        assert.ok(
          warning.range.end.character > warning.range.start.character,
          "the span is non-empty so it renders as a squiggle",
        );
      }
      // The pass and the failure are named in NO warning.
      const text = warnings.map((w) => w.message).join("\n");
      assert.ok(!text.includes("passes cleanly"), text);
      assert.ok(!text.includes("fails loudly"), text);

      // --- The raw TAP is echoed to the run's terminal, CRLF-terminated.
      assert.ok(sink.output.includes("# SKIP static reason"));
      assert.ok(sink.output.includes("# SKIP runtime precondition unmet"));
      assert.ok(sink.output.includes("\r\n"));
    });

    // [TESTING-TAP-AMBIGUITY] a PASSING case whose NAME contains `# SKIP` must
    // never be reported as skipped. This is the exact line a naive TAP split
    // mangles, and it would have marked a green test as an ignored one.
    test("a passing case whose name contains the SKIP directive is not warned", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(120000);
      const uri = writeFixture("ambiguous.test.osp", AMBIGUOUS_NAME_FIXTURE);
      const controller = newController();
      const file = await discoveredFile(controller, uri);
      const names = [...file.children].map(([, item]) => item.label).sort();
      assert.deepStrictEqual(
        names,
        ["name with # SKIP inside it", "really parked"],
        "discovery keeps the directive-shaped name whole",
      );
      const sink = await runWholeFile(controller, file);

      const ambiguousId = leafTestId(
        uri.toString(),
        "name with # SKIP inside it",
      );
      const parkedId = leafTestId(uri.toString(), "really parked");
      assert.deepStrictEqual(
        sink.ofKind("passed").map((e) => e.id),
        [ambiguousId],
        "the ambiguous case PASSED and must be reported as passed",
      );
      assert.deepStrictEqual(
        sink.ofKind("skipped").map((e) => e.id),
        [parkedId],
        "only the genuinely skipped case is skipped",
      );

      const warnings = skipWarnings(uri);
      assert.strictEqual(warnings.length, 1, JSON.stringify(warnings));
      assert.strictEqual(
        warnings[0].message,
        "Test 'really parked' was skipped: genuinely skipped",
      );
      assert.ok(
        !warnings[0].message.includes("inside it"),
        "the passing case's name never leaks into a warning",
      );
      assert.strictEqual(
        warnings[0].range.start.line,
        4,
        "the warning belongs to the parked case's line, not the ambiguous one",
      );
    });

    // [TESTING-SKIP-WARNING] the full edit lifecycle through the Explorer:
    // park → re-park under a new reason → revive → delete. Every step is a
    // real compile+run, and every step re-checks the published diagnostics.
    test("skip warnings track park, re-park, revive, and delete across runs", async function () {
      if (!compiler) {
        this.skip();
      }
      // Four full compile+run cycles, each on an edited file (so the build
      // cache misses every time): budgeted at ~75s per cycle.
      this.timeout(300000);
      const uri = writeFixture("lifecycle.test.osp", SKIP_FIXTURE);
      const controller = newController();

      // (1) Parked: one warning naming the original reason.
      const parked = await discoveredFile(controller, uri);
      await runWholeFile(controller, parked);
      const first = skipWarnings(uri);
      assert.strictEqual(first.length, 1, JSON.stringify(first));
      assert.strictEqual(
        first[0].message,
        "Test 'parked case' was skipped: blocked on #123",
      );

      // (2) Re-parked under a DIFFERENT reason: the message must update, not
      // duplicate — a stale warning would misreport why a test is off.
      fs.writeFileSync(uri.fsPath, SKIP_REPARKED_FIXTURE);
      const reparked = await discoveredFile(controller, uri);
      await runWholeFile(controller, reparked);
      const second = skipWarnings(uri);
      assert.strictEqual(
        second.length,
        1,
        `no duplicate: ${JSON.stringify(second)}`,
      );
      assert.strictEqual(
        second[0].message,
        "Test 'parked case' was skipped: now blocked on #456",
      );

      // (3) Revived: the warning clears and the case reports passed.
      fs.writeFileSync(uri.fsPath, SKIP_FIXED_FIXTURE);
      const revived = await discoveredFile(controller, uri);
      const revivedSink = await runWholeFile(controller, revived);
      assert.strictEqual(revivedSink.ofKind("skipped").length, 0);
      assert.strictEqual(revivedSink.ofKind("passed").length, 2);
      assert.deepStrictEqual(skipWarnings(uri), [], "revived case is clean");

      // (4) Parked AGAIN: the warning comes back rather than staying cleared.
      fs.writeFileSync(uri.fsPath, SKIP_FIXTURE);
      const reparkedAgain = await discoveredFile(controller, uri);
      await runWholeFile(controller, reparkedAgain);
      assert.strictEqual(
        skipWarnings(uri).length,
        1,
        "re-parking re-raises the warning",
      );

      // (5) Deleting the file drops its warnings entirely.
      removeTestFile(controller, uri);
      assert.deepStrictEqual(skipWarnings(uri), []);
    });

    // [TESTING-SKIP-WARNING] + [TESTING-FILTER]: running ONE case leaves the
    // others unrun — which is exactly an ignored test — so they warn; running
    // the whole file afterwards clears the ones that then execute.
    test("a filtered single-case run warns about the cases it left unrun", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(180000);
      const uri = writeFixture("filtered.test.osp", PASS_FIXTURE);
      const controller = newController();
      const file = await discoveredFile(controller, uri);
      const first = file.children.get(
        leafTestId(uri.toString(), "addition works"),
      );
      const second = file.children.get(
        leafTestId(uri.toString(), "zero identity"),
      );
      assert.ok(first && second);

      // Running ONLY the first case: the second never executes.
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([first]),
        sink,
        token(),
        () => compiler,
      );
      assert.deepStrictEqual(
        sink.ofKind("passed").map((e) => e.id),
        [first.id],
      );
      assert.ok(
        !sink.output.includes("zero identity"),
        "the filter really excluded the other case",
      );
      // The executed case has no warning; the unrun one is not touched at all
      // (a run reports only what it ran), so no stale state is invented.
      assert.deepStrictEqual(
        skipWarnings(uri).map((w) => w.message),
        [],
        "an excluded case is not a skip of THIS run",
      );

      // Now run the whole file WITH one case excluded: the excluded case is
      // deliberately not run, so it is an ignored test and must warn.
      const exclusionSink = new RecordingSink();
      const request = new vscode.TestRunRequest([file], [second]);
      await executeRunRequest(
        controller,
        request,
        exclusionSink,
        token(),
        () => compiler,
      );
      assert.deepStrictEqual(
        exclusionSink.ofKind("passed").map((e) => e.id),
        [first.id],
        "only the included case ran",
      );
      assert.ok(
        !exclusionSink.output.includes("zero identity"),
        "the excluded case never executed",
      );

      // Finally, running everything clears the slate: both pass, no warnings.
      const fullSink = await runWholeFile(controller, file);
      assert.strictEqual(fullSink.ofKind("passed").length, 2);
      assert.strictEqual(fullSink.ofKind("skipped").length, 0);
      assert.deepStrictEqual(
        skipWarnings(uri),
        [],
        "a full green run is silent",
      );
    });

    // [TESTING-SKIP-WARNING] the ML flavor reaches the same contract through
    // its own frontend, and a COVERAGE run publishes the same warnings as a
    // plain run — the profile must not change what is reported.
    test("ML suites and coverage runs publish the same skip warnings", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(180000);
      const mlUri = writeFixture("mlskip.test.ospml", ML_SKIP_FIXTURE);
      const controller = newController();
      const file = await discoveredFile(controller, mlUri);
      assert.strictEqual(file.children.size, 2, "ML cases discovered");

      const plain = await runWholeFile(controller, file);
      assert.deepStrictEqual(
        plain.ofKind("skipped").map((e) => e.id),
        [leafTestId(mlUri.toString(), "ml parked case")],
      );
      const afterPlain = skipWarnings(mlUri);
      assert.strictEqual(afterPlain.length, 1, JSON.stringify(afterPlain));
      assert.strictEqual(
        afterPlain[0].message,
        "Test 'ml parked case' was skipped: ml blocked",
        "the ML flavor produces the same message shape as Default",
      );
      assert.strictEqual(afterPlain[0].range.start.line, 2);
      assert.strictEqual(afterPlain[0].source, "osprey tests");

      // The same suite under the COVERAGE profile: same verdicts, same
      // warning, plus the coverage report.
      const coverage = await runWholeFile(controller, file, COVERAGE_RUN);
      assert.deepStrictEqual(
        coverage.ofKind("skipped").map((e) => e.id),
        [leafTestId(mlUri.toString(), "ml parked case")],
        "a coverage run reports the skip identically",
      );
      const afterCoverage = skipWarnings(mlUri);
      assert.strictEqual(
        afterCoverage.length,
        1,
        `coverage neither duplicates nor drops the warning: ${JSON.stringify(afterCoverage)}`,
      );
      assert.strictEqual(afterCoverage[0].message, afterPlain[0].message);
      assert.ok(
        coverage.coverage.get(mlUri.fsPath),
        "the coverage report still reached the sink",
      );
    });

    // [TESTING-SKIP-WARNING] damage case: a suite that does not COMPILE has no
    // verdicts at all. It must error, not manufacture skip warnings.
    test("a compile failure errors without inventing skip warnings", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(120000);
      const uri = writeFixture("breaks.test.osp", SKIP_FIXTURE);
      const controller = newController();
      const file = await discoveredFile(controller, uri);
      // Park it first so a real warning exists...
      await runWholeFile(controller, file);
      assert.strictEqual(skipWarnings(uri).length, 1, "warning is present");

      // ...then break the file so the next run cannot compile.
      fs.writeFileSync(uri.fsPath, `${SKIP_FIXTURE}\nfn broken( = 42\n`);
      const sink = await runWholeFile(controller, file);
      assert.ok(
        sink.ofKind("errored").length > 0,
        "a compile failure errors the run",
      );
      assert.strictEqual(
        sink.ofKind("skipped").length,
        0,
        "a compile failure is NOT a skip",
      );
      assert.match(
        String(sink.ofKind("errored")[0].message),
        /error|syntax/i,
        "the compiler's own message is surfaced",
      );
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
        COVERAGE_RUN,
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
        COVERAGE_RUN,
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
