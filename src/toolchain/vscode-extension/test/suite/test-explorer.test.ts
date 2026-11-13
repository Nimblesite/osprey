// Integration tests for the Osprey Test Explorer ([TESTING-VSCODE]): discovery
// and run flows against a real TestController, temp-dir fixture files, and the
// freshly built osprey compiler. The pure parsing/planning helpers are covered
// in test-explorer-parse.test.ts.

import * as assert from "assert";
import * as fs from "fs";
import * as path from "path";
import * as sinon from "sinon";
import * as vscode from "vscode";
import { fileTestId, leafTestId } from "../../client/src/test-explorer-parse";
import { COVERAGE_RUN, PLAIN_RUN } from "../../client/src/test-explorer-runs";
import { executeRunRequest, makeRunHandler } from "../../client/src/test-explorer";
import { coverageSink, detailedCoverageFor } from "../../client/src/test-explorer-report";
import {
  makeDiscovery,
  makeDiscoveryQueue,
  fileItems,
  isBuildOutput,
  isTestFile,
  projectRootOf,
  refreshTestFile,
  removeTestFile,
  requestedItems,
  testFileLabel,
} from "../../client/src/test-explorer-items";
import { runCompiler } from "../../client/src/test-explorer-process";
import {
  oneSkipWarning,
  resolveBuiltOsprey,
  skipWarningsOf,
} from "./osprey-test-env";
import {
  AMBIGUOUS_NAME_FIXTURE,
  assertVerdicts,
  BROKEN_FIXTURE,
  controllerFactory,
  COVERAGE_FIXTURE,
  disposeRegistered,
  EXPLAINED_SKIP_FIXTURE,
  FAIL_FIXTURE,
  makeFixtureDir,
  MIXED_FIXTURE,
  ML_FIXTURE,
  ML_SKIP_FIXTURE,
  PASS_FIXTURE,
  RecordingSink,
  requireCompiler,
  type FixtureDir,
  SinkEvent,
  SKIP_FIXED_FIXTURE,
  SKIP_FIXTURE,
  SKIP_REPARKED_FIXTURE,
  STRAY_FIXTURE,
  UNEXPLAINED_SKIP_FIXTURE,
} from "./test-explorer-harness";

suite("Osprey Test Explorer", () => {
  const compiler = resolveBuiltOsprey();
  let fixtures: FixtureDir;
  let passUri: vscode.Uri;
  let failUri: vscode.Uri;
  let mlUri: vscode.Uri;
  let brokenUri: vscode.Uri;
  let strayUri: vscode.Uri;
  const disposables: vscode.Disposable[] = [];
  const newController = controllerFactory(
    disposables,
    "ospreyTests-spec",
    () => compiler ?? "osprey",
  );

  /** The leaf `uri` declares as `name`, looked up by its stable id. */
  function leafOf(
    parent: vscode.TestItem,
    uri: vscode.Uri,
    name: string,
  ): vscode.TestItem | undefined {
    return parent.children.get(leafTestId(uri.toString(), name));
  }

  /** The leaf `uri` declares as `name`, asserted to have been discovered. */
  function requiredLeaf(
    parent: vscode.TestItem,
    uri: vscode.Uri,
    name: string,
  ): vscode.TestItem {
    const leaf = leafOf(parent, uri, name);
    assert.ok(leaf, `discovery lists ${name}`);
    return leaf;
  }

  /** Create `dir` if needed and write `name` = `content` into it. */
  function writeInto(dir: string, name: string, content: string): vscode.Uri {
    fs.mkdirSync(dir, { recursive: true });
    const uri = vscode.Uri.file(path.join(dir, name));
    fs.writeFileSync(uri.fsPath, content);
    return uri;
  }

  /**
   * Pretend the temp fixture dir is the open workspace folder: discovery asks
   * `getWorkspaceFolder` for it and labels files with `asRelativePath`.
   */
  function stubFixtureWorkspace() {
    const workspaceFolder = {
      uri: vscode.Uri.file(fixtures.path),
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
          fixtures.path,
          typeof resource === "string" ? resource : resource.fsPath,
        ),
      );
    return {
      restore(): void {
        getWorkspaceFolder.restore();
        asRelativePath.restore();
      },
    };
  }

  suiteSetup(() => {
    fixtures = makeFixtureDir("osprey-test-explorer-");
    passUri = fixtures.write("pass.test.osp", PASS_FIXTURE);
    failUri = fixtures.write("fail.test.osp", FAIL_FIXTURE);
    mlUri = fixtures.write("ml.test.ospml", ML_FIXTURE);
    brokenUri = fixtures.write("broken.test.osp", BROKEN_FIXTURE);
    strayUri = fixtures.write("stray.test.osp", STRAY_FIXTURE);
  });

  suiteTeardown(() => fixtures.remove());

  teardown(() => disposeRegistered(disposables));

  suite("discovery", () => {
    /** A fresh controller with `uri` discovered into it, plus its compiler. */
    async function discovering(
      testContext: { timeout(ms: number): unknown },
      uri: vscode.Uri,
      budgetMs = 30000,
    ): Promise<{
      controller: vscode.TestController;
      osprey: string;
      item: vscode.TestItem;
    }> {
      const osprey = requireCompiler(testContext, compiler, budgetMs);
      const controller = newController();
      return {
        controller,
        osprey,
        item: await refreshTestFile(controller, uri, osprey),
      };
    }

    /** The only item in `items`, asserted to be the sole one. */
    function onlyItem(items: vscode.TestItemCollection): vscode.TestItem {
      const all = [...items].map(([, item]) => item);
      assert.strictEqual(all.length, 1, "exactly one item hangs at this level");
      return all[0];
    }

    /** Remove `uri` from `controller` and assert the tree emptied. */
    function removeAndDrain(
      controller: vscode.TestController,
      uri: vscode.Uri,
    ): void {
      removeTestFile(controller, uri);
      assert.strictEqual(controller.items.size, 0);
    }

    /** Assert `predicate` accepts every `yes` uri and rejects every `no` one. */
    function assertClassified(
      predicate: (uri: vscode.Uri) => boolean,
      yes: readonly vscode.Uri[],
      no: readonly vscode.Uri[],
    ): void {
      for (const uri of yes) {
        assert.ok(predicate(uri));
      }
      for (const uri of no) {
        assert.ok(!predicate(uri));
      }
    }

    test("registerOspreyTestExplorer wires controller, refresh, and both watchers", () => {
      const controller = newController(() => "/nonexistent/bin/osprey");
      assert.strictEqual(controller.label, "Osprey Tests");
      assert.ok(controller.id.startsWith("ospreyTests-spec-"));
      assert.strictEqual(typeof controller.refreshHandler, "function");
      // controller, source watcher, compiler watcher, skip-warning cleanup
      assert.strictEqual(disposables.length, 4);
    });

    test("testFileLabel outside a workspace is the basename", () => {
      assert.strictEqual(testFileLabel(passUri), "pass.test.osp");
    });

    test("refreshTestFile discovers leaves with names, ids, and ranges", async function () {
      const { item } = await discovering(this, passUri);
      assert.strictEqual(item.error, undefined);
      assert.strictEqual(item.label, "pass.test.osp");
      assert.strictEqual(item.children.size, 2);
      const first = requiredLeaf(item, passUri, "addition works");
      assert.strictEqual(first.label, "addition works");
      assert.strictEqual(first.range?.start.line, 2);
      assert.strictEqual(first.range?.start.character, 0);
      const second = requiredLeaf(item, passUri, "zero identity");
      assert.strictEqual(second.range?.start.line, 6);
    });

    test("refreshTestFile re-resolves children after edits", async function () {
      const uri = fixtures.write("edited.test.osp", PASS_FIXTURE);
      const { controller, osprey, item: before } = await discovering(this, uri);
      assert.strictEqual(before.children.size, 2);
      fs.writeFileSync(uri.fsPath, 'test("only one", fn() => expect(1, 1))\n');
      const after = await refreshTestFile(controller, uri, osprey);
      assert.strictEqual(after.children.size, 1);
      assert.strictEqual(after, before);
    });

    test("refreshTestFile discovers the ML flavor", async function () {
      const { item } = await discovering(this, mlUri);
      assert.strictEqual(item.children.size, 1);
      requiredLeaf(item, mlUri, "ml addition");
    });

    test("refreshTestFile groups workspace test files by folder", async function () {
      const testDir = path.join(fixtures.path, "tests", "core", "arithmetic");
      const uri = writeInto(testDir, "calculator.test.osp", PASS_FIXTURE);
      const workspace = stubFixtureWorkspace();
      try {
        const { controller, item: file } = await discovering(this, uri);
        const tests = onlyItem(controller.items);
        assert.strictEqual(tests.label, "tests");
        const core = onlyItem(tests.children);
        assert.strictEqual(core.label, "core");
        const arithmetic = onlyItem(core.children);
        assert.strictEqual(arithmetic.label, "arithmetic");
        assert.strictEqual(arithmetic.children.get(file.id), file);
        assert.strictEqual(file.label, "calculator.test.osp");
      } finally {
        workspace.restore();
      }
    });

    // removeTestFile has to find a file item that is NOT a direct child of the
    // controller — inside a workspace every test file hangs off a chain of
    // directory items — and then leave no empty directories behind. Both arms
    // of the prune matter: an inner directory is dropped from its parent, the
    // outermost from the controller itself. A stale empty folder in the
    // Explorer is how a deleted test file appears to still exist.
    test("removeTestFile prunes the directory chain it emptied", async function () {
      const nestedDir = path.join(fixtures.path, "suites", "unit");
      const uri = writeInto(nestedDir, "nested.test.osp", PASS_FIXTURE);
      const workspace = stubFixtureWorkspace();
      try {
        // The workspace-relative path is the label, not the basename.
        assert.strictEqual(
          testFileLabel(uri),
          path.join("suites", "unit", "nested.test.osp"),
        );
        const { controller, osprey, item: file } = await discovering(this, uri);
        // Discovering the same file twice must reuse the chain, not clone it.
        await refreshTestFile(controller, uri, osprey);
        const suites = onlyItem(controller.items);
        const unit = onlyItem(suites.children);
        assert.strictEqual(unit.children.get(file.id), file);

        // A file the tree has never seen is a no-op, not a throw.
        removeTestFile(
          controller,
          vscode.Uri.file(path.join(nestedDir, "ghost.test.osp")),
        );
        assert.strictEqual(controller.items.size, 1);

        removeAndDrain(controller, uri);
      } finally {
        workspace.restore();
        fs.rmSync(path.join(fixtures.path, "suites"), {
          recursive: true,
          force: true,
        });
      }
    });

    test("a syntax error surfaces on the file item with no children", async function () {
      const { item } = await discovering(this, brokenUri);
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

    test("discovery reacts to test files, modules, and build output", async function () {
      const osprey = requireCompiler(this, compiler, 60000);
      const projectDir = fs.mkdtempSync(path.join(fixtures.path, "project-"));
      fs.writeFileSync(path.join(projectDir, "osprey.toml"), "");
      const nested = path.join(projectDir, "test");
      fs.mkdirSync(nested);
      const suitePath = path.join(nested, "suite.test.osp");
      fs.writeFileSync(suitePath, PASS_FIXTURE);
      const suiteUri = vscode.Uri.file(suitePath);
      const moduleUri = vscode.Uri.file(path.join(projectDir, "src.osp"));
      const outsideUri = vscode.Uri.file(path.join(fixtures.path, "lone.osp"));
      const controller = newController();
      const discovery = makeDiscovery(controller, () => osprey);
      const suiteItem = (): vscode.TestItem | undefined =>
        controller.items.get(fileTestId(suiteUri.toString()));

      assert.strictEqual(projectRootOf(moduleUri), projectDir);
      assert.strictEqual(projectRootOf(suiteUri), projectDir);
      assert.strictEqual(projectRootOf(outsideUri), fixtures.path);
      assertClassified(isTestFile, [suiteUri, mlUri], [moduleUri]);

      await discovery.changed(suiteUri);
      assert.strictEqual(suiteItem()?.children.size, 2);

      // A module edit re-resolves its project's suites (here: a case added).
      fs.writeFileSync(
        suitePath,
        `${PASS_FIXTURE}\ntest("third", fn() => {\n    expect(1, 1)\n})\n`,
      );
      await discovery.changed(moduleUri);
      assert.strictEqual(suiteItem()?.children.size, 3);
      assert.ok(
        suiteItem() &&
          leafOf(suiteItem() as vscode.TestItem, suiteUri, "third"),
      );

      // A source outside the project leaves it alone.
      fs.writeFileSync(suitePath, PASS_FIXTURE);
      await discovery.changed(outsideUri);
      assert.strictEqual(suiteItem()?.children.size, 3);

      // A deleted module re-resolves the project too.
      await discovery.deleted(moduleUri);
      assert.strictEqual(suiteItem()?.children.size, 2);

      // A deleted test file leaves the tree.
      await discovery.deleted(suiteUri);
      assert.strictEqual(suiteItem(), undefined);
      assert.strictEqual(controller.items.size, 0);

      // A discovery queued before the file went away does not bring it back.
      fs.rmSync(suitePath);
      await discovery.changed(suiteUri);
      assert.strictEqual(suiteItem(), undefined);
      assert.strictEqual(controller.items.size, 0);
      fs.rmSync(projectDir, { recursive: true, force: true });
    });

    test("removeTestFile drops items and empty ancestors", async () => {
      const controller = newController();
      await refreshTestFile(controller, passUri, "/nonexistent/osprey-xyz");
      assert.strictEqual(fileItems(controller.items).length, 1);
      removeAndDrain(controller, passUri);
      removeAndDrain(controller, passUri);
    });

    test("the discovery queue coalesces, bounds, and drains", async () => {
      const started: string[] = [];
      const releases: (() => void)[] = [];
      let peak = 0;
      let live = 0;
      const schedule = makeDiscoveryQueue((uri) => {
        started.push(path.basename(uri.fsPath));
        live += 1;
        peak = Math.max(peak, live);
        return new Promise<void>((release) =>
          releases.push(() => {
            live -= 1;
            release();
          }),
        );
      }, 2);
      const settle = async (): Promise<void> => {
        while (releases.length > 0) {
          releases.shift()?.();
          await new Promise((tick) => setImmediate(tick));
        }
      };
      const drained = schedule([passUri, failUri, mlUri, passUri]);
      // pass is already running; asking again queues exactly one more pass.
      const again = schedule([passUri, passUri]);
      assert.deepStrictEqual(started, ["pass.test.osp", "fail.test.osp"]);
      await settle();
      await Promise.all([drained, again]);
      assert.deepStrictEqual(started, [
        "pass.test.osp",
        "fail.test.osp",
        "ml.test.ospml",
        "pass.test.osp",
      ]);
      assert.strictEqual(peak, 2);
      assert.strictEqual(live, 0);
      await schedule([]);

      // A failing resolve neither stalls the queue nor rejects the drain.
      const failing = makeDiscoveryQueue(() => Promise.reject(new Error("x")));
      await failing([passUri, failUri]);
    });

    test("rescan resolves the workspace, skips build output, drops stale files", async function () {
      const osprey = requireCompiler(this, compiler, 30000);
      const controller = newController();
      let found: vscode.Uri[] = [passUri, mlUri];
      const discovery = makeDiscovery(
        controller,
        () => osprey,
        () => Promise.resolve(found),
      );
      await discovery.rescan();
      assert.strictEqual(
        controller.items.get(fileTestId(passUri.toString()))?.children.size,
        2,
      );
      assert.ok(controller.items.get(fileTestId(mlUri.toString())));
      const gone = fixtures.write("gone.test.osp", PASS_FIXTURE);
      found = [passUri, gone];
      await discovery.rescan();
      assert.strictEqual(fileItems(controller.items).length, 3);
      fs.rmSync(gone.fsPath);
      found = [passUri];
      await discovery.rescan();
      assert.deepStrictEqual(
        fileItems(controller.items).map((item) => item.label),
        ["pass.test.osp", "ml.test.ospml"],
      );

      // The refresh button runs the same rescan.
      const viaButton = newController();
      await viaButton.refreshHandler?.(
        new vscode.CancellationTokenSource().token,
      );
      assert.strictEqual(viaButton.items.size, 0);
    });

    test("build output never reaches the tree", async () => {
      const root = vscode.workspace.workspaceFolders?.[0]?.uri;
      if (root !== undefined) {
        const copy = vscode.Uri.joinPath(root, "target", "x", "a.test.osp");
        const dep = vscode.Uri.joinPath(root, "node_modules", "a.test.osp");
        const real = vscode.Uri.joinPath(root, "tests", "a.test.osp");
        assertClassified(isBuildOutput, [copy, dep], [real]);
      }
      assert.ok(!isBuildOutput(passUri));
      const controller = newController();
      const discovery = makeDiscovery(controller, () => "/nonexistent/x");
      const copyUri = vscode.Uri.file(
        path.join(fixtures.path, "target", "copy.test.osp"),
      );
      await discovery.changed(copyUri);
      assert.strictEqual(controller.items.size, 0);
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
          fixtures.path,
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

    /** A fresh controller with `uri` discovered into it. */
    async function discovered(
      uri: vscode.Uri,
    ): Promise<{ controller: vscode.TestController; file: vscode.TestItem }> {
      const controller = newController();
      return { controller, file: await discoveredFile(controller, uri) };
    }

    async function discoveredFile(
      controller: vscode.TestController,
      uri: vscode.Uri,
    ): Promise<vscode.TestItem> {
      const item = await refreshTestFile(controller, uri, compiler ?? "osprey");
      assert.strictEqual(item.error, undefined);
      return item;
    }

    /** Budget `budgetMs` of compiler time, then discover `uri` fresh. */
    async function budgeted(
      testContext: { timeout(ms: number): unknown },
      uri: vscode.Uri,
      budgetMs: number,
    ): Promise<{
      osprey: string;
      controller: vscode.TestController;
      file: vscode.TestItem;
    }> {
      const osprey = requireCompiler(testContext, compiler, budgetMs);
      const { controller, file } = await discovered(uri);
      return { osprey, controller, file };
    }

    /** Budget, stage `content` as fixture `name`, and discover it fresh. */
    async function staged(
      testContext: { timeout(ms: number): unknown },
      name: string,
      content: string,
      budgetMs: number,
    ): Promise<{
      osprey: string;
      controller: vscode.TestController;
      file: vscode.TestItem;
      uri: vscode.Uri;
    }> {
      const uri = fixtures.write(name, content);
      return { ...(await budgeted(testContext, uri, budgetMs)), uri };
    }

    test("a failed test peek includes complete Context For AI details", async function () {
      requireCompiler(this, compiler, 30000);
      const { sink } = await runFile(failUri);
      const goodId = leafTestId(failUri.toString(), "good math");
      const badId = leafTestId(failUri.toString(), "bad math");
      assert.deepStrictEqual(idsOf(sink, "passed"), [goodId]);
      const failures = assertVerdicts(
        sink,
        "failed",
        [badId],
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
      assertRunEnded(sink);
    });

    test("a single requested leaf runs with OSPREY_TEST_FILTER", async function () {
      const { controller, file } = await budgeted(this, failUri, 30000);
      const good = requiredLeaf(file, failUri, "good math");
      const sink = await runItems(controller, [good]);
      assert.deepStrictEqual(idsOf(sink, "passed"), [good.id]);
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
      // Two full compile+run cycles (skip, then revive) — twice the budget of
      // the single-run tests around it.
      requireCompiler(this, compiler, 120000);
      const skipUri = fixtures.write("skips.test.osp", SKIP_FIXTURE);
      const { controller, sink } = await runFile(skipUri);
      const parkedId = leafTestId(skipUri.toString(), "parked case");
      assert.deepStrictEqual(idsOf(sink, "skipped"), [parkedId]);
      // The warning is published through a REAL DiagnosticCollection, so the
      // editor's own diagnostics channel (Problems panel, squiggles) sees it.
      const published = assertWarningMessage(
        skipUri,
        "Test 'parked case' was skipped: blocked on #123",
      );
      assert.strictEqual(published.source, "osprey tests");
      assert.strictEqual(
        published.range.start.line,
        2,
        "warning sits on the test call's line",
      );

      // Revive the case and re-run: the warning must clear.
      const secondSink = await rerunFile(
        controller,
        skipUri,
        SKIP_FIXED_FIXTURE,
      );
      assert.strictEqual(secondSink.ofKind("skipped").length, 0);
      assert.deepStrictEqual(
        skipWarningsOf(skipUri),
        [],
        "revived case cleared its skip warning",
      );
    });

    // [TESTING-SKIP-REASON] a skip that names no reason is an ERROR, not a
    // warning: the case still reports skipped in the TAP, but the diagnostic
    // it publishes is the strongest one the editor has.
    test("a skip with no reason raises an Error diagnostic", async function () {
      const { controller, file, uri } = await staged(
        this,
        "unexplained.test.osp",
        UNEXPLAINED_SKIP_FIXTURE,
        120000,
      );
      await runWholeFile(controller, file);
      const published = oneSkipWarning(uri);
      assert.strictEqual(
        published.severity,
        vscode.DiagnosticSeverity.Error,
        "an unexplained skip is an error, not a warning",
      );
      assert.strictEqual(
        published.message,
        "Test 'unexplained case' was skipped with no reason; every skip must name one",
      );
      // Giving the SAME case a reason downgrades it to a warning — the reason
      // is what decides the strength, nothing else.
      fs.writeFileSync(uri.fsPath, EXPLAINED_SKIP_FIXTURE);
      const revived = await discoveredFile(controller, uri);
      await runWholeFile(controller, revived);
      assertWarningMessage(
        uri,
        "Test 'unexplained case' was skipped: blocked on #456",
      );
    });

    // [TESTING-SKIP-WARNING] skipped/ignored are the same: a discovered case
    // that never appears in its run's TAP at all (ignored) warns too, and
    // deleting the file drops its warnings.
    test("a case missing from the TAP warns as ignored; removal clears warnings", async function () {
      const { controller, file, uri: ghostUri } = await staged(
        this,
        "ghost.test.osp",
        PASS_FIXTURE,
        120000,
      );
      assert.strictEqual(file.children.size, 2);
      // Drop one discovered case from the file AFTER discovery: the run's TAP
      // never mentions it, which is an ignored test.
      fs.writeFileSync(
        ghostUri.fsPath,
        'fn add(a, b) = wrapAdd(a, b)\n\ntest("addition works", fn() => expect(add(2, 3), 5))\n',
      );
      await runWholeFile(controller, file);
      const ghosts = oneSkipWarning(ghostUri);
      assert.strictEqual(
        ghosts.message,
        "Test 'zero identity' did not run (skipped/ignored)",
      );
      removeTestFile(controller, ghostUri);
      assert.deepStrictEqual(
        skipWarningsOf(ghostUri),
        [],
        "removing the file drops its skip warnings",
      );
    });

    /** Run `request` into a fresh sink and return that sink. */
    async function runTo(
      controller: vscode.TestController,
      request: vscode.TestRunRequest,
      mode = PLAIN_RUN,
    ): Promise<RecordingSink> {
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        request,
        sink,
        token(),
        () => compiler ?? "osprey",
        mode,
      );
      return sink;
    }

    /** Run exactly `items` through a fresh sink and return it. */
    function runItems(
      controller: vscode.TestController,
      items: vscode.TestItem[],
      mode = PLAIN_RUN,
    ): Promise<RecordingSink> {
      return runTo(controller, new vscode.TestRunRequest(items), mode);
    }

    /** Run every case of `file` through a fresh sink and return it. */
    function runWholeFile(
      controller: vscode.TestController,
      file: vscode.TestItem,
      mode = PLAIN_RUN,
    ): Promise<RecordingSink> {
      return runItems(controller, [file], mode);
    }

    /** The one skip warning on `uri`, asserted to warn with `message`. */
    function assertWarningMessage(
      uri: vscode.Uri,
      message: string,
    ): vscode.Diagnostic {
      const warning = oneSkipWarning(uri);
      assert.strictEqual(warning.severity, vscode.DiagnosticSeverity.Warning);
      assert.strictEqual(warning.message, message);
      return warning;
    }

    /** `sink` recorded two passes and no skips. */
    function assertTwoPassed(sink: RecordingSink): void {
      assert.strictEqual(sink.ofKind("passed").length, 2);
      assert.strictEqual(sink.ofKind("skipped").length, 0);
    }

    /** `sink` ran two passes, no skips, and left no warnings on `uri`. */
    function assertGreenRun(
      sink: RecordingSink,
      uri: vscode.Uri,
      note: string,
    ): void {
      assertTwoPassed(sink);
      assert.deepStrictEqual(skipWarningsOf(uri), [], note);
    }

    /** `fc` counts `covered` of `total` coverable lines for `uri`. */
    function assertCoverageCounts(
      uri: vscode.Uri,
      fc: vscode.FileCoverage,
      covered: number,
      total: number,
      coveredNote: string,
    ): vscode.FileCoverage {
      assert.strictEqual(fc.uri.fsPath, uri.fsPath);
      assert.strictEqual(fc.statementCoverage.covered, covered, coveredNote);
      assert.strictEqual(fc.statementCoverage.total, total, "coverable lines");
      return fc;
    }

    /** Discover `uri` into a fresh controller and run its whole file. */
    async function runFile(
      uri: vscode.Uri,
      mode = PLAIN_RUN,
    ): Promise<{
      controller: vscode.TestController;
      file: vscode.TestItem;
      sink: RecordingSink;
    }> {
      const { controller, file } = await discovered(uri);
      return {
        controller,
        file,
        sink: await runWholeFile(controller, file, mode),
      };
    }

    /** Rewrite `uri` with `content`, re-discover it, and run the whole file. */
    async function rerunFile(
      controller: vscode.TestController,
      uri: vscode.Uri,
      content: string,
      mode = PLAIN_RUN,
    ): Promise<RecordingSink> {
      fs.writeFileSync(uri.fsPath, content);
      return runWholeFile(
        controller,
        await discoveredFile(controller, uri),
        mode,
      );
    }

    /** The test ids `sink` recorded for events of `kind`. */
    const idsOf = (sink: RecordingSink, kind: SinkEvent["kind"]) =>
      sink.ofKind(kind).map((event) => event.id);

    /** The run's last recorded event is its `end`. */
    function assertRunEnded(sink: RecordingSink): void {
      assert.deepStrictEqual(sink.events[sink.events.length - 1], {
        kind: "end",
      });
    }

    /** `sink` with its one reporting call `method` replaced by `override`. */
    function overriding(
      sink: RecordingSink,
      method: "enqueued" | "started",
      override: (target: RecordingSink) => (test: vscode.TestItem) => void,
    ): RecordingSink {
      return new Proxy(sink, {
        get(target, prop, receiver) {
          return prop === method
            ? override(target)
            : Reflect.get(target, prop, receiver);
        },
      });
    }

    // [TESTING-SKIP-WARNING] one suite, every outcome: a pass, a failure, a
    // STATIC skip and a DYNAMIC skip. Exactly the two skips warn — a failure
    // is a failure, not a skip, and a pass warns about nothing.
    test("a mixed suite warns for every skip and for nothing else", async function () {
      const { controller, file, uri: mixedUri } = await staged(
        this,
        "mixed.test.osp",
        MIXED_FIXTURE,
        120000,
      );
      assert.strictEqual(file.children.size, 4, "all four cases discovered");
      const sink = await runWholeFile(controller, file);

      const id = (name: string) => leafTestId(mixedUri.toString(), name);
      // --- Verdicts reach the Test Explorer intact.
      assert.deepStrictEqual(idsOf(sink, "passed"), [id("passes cleanly")]);
      assert.deepStrictEqual(idsOf(sink, "failed"), [id("fails loudly")]);
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
      const warnings = skipWarningsOf(mixedUri);
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
      const { controller, file, uri } = await staged(
        this,
        "ambiguous.test.osp",
        AMBIGUOUS_NAME_FIXTURE,
        120000,
      );
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
        idsOf(sink, "passed"),
        [ambiguousId],
        "the ambiguous case PASSED and must be reported as passed",
      );
      assert.deepStrictEqual(
        idsOf(sink, "skipped"),
        [parkedId],
        "only the genuinely skipped case is skipped",
      );

      const warning = oneSkipWarning(uri);
      assert.strictEqual(
        warning.message,
        "Test 'really parked' was skipped: genuinely skipped",
      );
      assert.ok(
        !warning.message.includes("inside it"),
        "the passing case's name never leaks into a warning",
      );
      assert.strictEqual(
        warning.range.start.line,
        4,
        "the warning belongs to the parked case's line, not the ambiguous one",
      );
    });

    // [TESTING-SKIP-WARNING] the full edit lifecycle through the Explorer:
    // park → re-park under a new reason → revive → delete. Every step is a
    // real compile+run, and every step re-checks the published diagnostics.
    test("skip warnings track park, re-park, revive, and delete across runs", async function () {
      // Four full compile+run cycles, each on an edited file (so the build
      // cache misses every time): budgeted at ~75s per cycle.
      requireCompiler(this, compiler, 300000);
      const uri = fixtures.write("lifecycle.test.osp", SKIP_FIXTURE);
      const controller = newController();

      // (1) Parked: one warning naming the original reason.
      const parked = await discoveredFile(controller, uri);
      await runWholeFile(controller, parked);
      const first = oneSkipWarning(uri);
      assert.strictEqual(
        first.message,
        "Test 'parked case' was skipped: blocked on #123",
      );

      // (2) Re-parked under a DIFFERENT reason: the message must update, not
      // duplicate — a stale warning would misreport why a test is off.
      await rerunFile(controller, uri, SKIP_REPARKED_FIXTURE);
      const second = oneSkipWarning(uri);
      assert.strictEqual(
        second.message,
        "Test 'parked case' was skipped: now blocked on #456",
      );

      // (3) Revived: the warning clears and the case reports passed.
      const revivedSink = await rerunFile(controller, uri, SKIP_FIXED_FIXTURE);
      assertGreenRun(revivedSink, uri, "revived case is clean");

      // (4) Parked AGAIN: the warning comes back rather than staying cleared.
      await rerunFile(controller, uri, SKIP_FIXTURE);
      assert.strictEqual(
        skipWarningsOf(uri).length,
        1,
        "re-parking re-raises the warning",
      );

      // (5) Deleting the file drops its warnings entirely.
      removeTestFile(controller, uri);
      assert.deepStrictEqual(skipWarningsOf(uri), []);
    });

    // [TESTING-SKIP-WARNING] + [TESTING-FILTER]: running ONE case leaves the
    // others unrun — which is exactly an ignored test — so they warn; running
    // the whole file afterwards clears the ones that then execute.
    test("a filtered single-case run warns about the cases it left unrun", async function () {
      const { osprey, controller, file, uri } = await staged(
        this,
        "filtered.test.osp",
        PASS_FIXTURE,
        180000,
      );
      const first = requiredLeaf(file, uri, "addition works");
      const second = requiredLeaf(file, uri, "zero identity");

      // Running ONLY the first case: the second never executes.
      const sink = await runItems(controller, [first]);
      assert.deepStrictEqual(idsOf(sink, "passed"), [first.id]);
      assert.ok(
        !sink.output.includes("zero identity"),
        "the filter really excluded the other case",
      );
      // The executed case has no warning; the unrun one is not touched at all
      // (a run reports only what it ran), so no stale state is invented.
      assert.deepStrictEqual(
        skipWarningsOf(uri).map((w) => w.message),
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
        () => osprey,
      );
      assert.deepStrictEqual(
        idsOf(exclusionSink, "passed"),
        [first.id],
        "only the included case ran",
      );
      assert.ok(
        !exclusionSink.output.includes("zero identity"),
        "the excluded case never executed",
      );

      // Finally, running everything clears the slate: both pass, no warnings.
      const fullSink = await runWholeFile(controller, file);
      assertGreenRun(fullSink, uri, "a full green run is silent");
    });

    // [TESTING-SKIP-WARNING] the ML flavor reaches the same contract through
    // its own frontend, and a COVERAGE run publishes the same warnings as a
    // plain run — the profile must not change what is reported.
    test("ML suites and coverage runs publish the same skip warnings", async function () {
      const { controller, file, uri: mlUri } = await staged(
        this,
        "mlskip.test.ospml",
        ML_SKIP_FIXTURE,
        180000,
      );
      assert.strictEqual(file.children.size, 2, "ML cases discovered");

      const plain = await runWholeFile(controller, file);
      assert.deepStrictEqual(idsOf(plain, "skipped"), [
        leafTestId(mlUri.toString(), "ml parked case"),
      ]);
      const afterPlain = skipWarningsOf(mlUri);
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
        idsOf(coverage, "skipped"),
        [leafTestId(mlUri.toString(), "ml parked case")],
        "a coverage run reports the skip identically",
      );
      const afterCoverage = skipWarningsOf(mlUri);
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
      const { controller, file, uri } = await staged(
        this,
        "breaks.test.osp",
        SKIP_FIXTURE,
        120000,
      );
      // Park it first so a real warning exists...
      await runWholeFile(controller, file);
      assert.strictEqual(skipWarningsOf(uri).length, 1, "warning is present");

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
      const { controller, file, uri: coverageUri } = await staged(
        this,
        "covered.test.osp",
        COVERAGE_FIXTURE,
        30000,
      );
      const sink = await runItems(controller, [file], COVERAGE_RUN);
      assert.strictEqual(sink.ofKind("passed").length, 1);
      const hits = sink.coverage.get(coverageUri.fsPath);
      assert.ok(hits, "coverage report reached the sink");
      assert.ok((hits.get(1) ?? 0) > 0, "double's definition line is covered");
      assert.strictEqual(hits.get(3), 0, "unused's definition line has 0 hits");
    });

    /** `[0-based line, executed]` per line of `fc`'s gutter detail. */
    function lineHits(fc: vscode.FileCoverage): [number, number | boolean][] {
      return detailedCoverageFor(fc).map((statement) => [
        (statement.location as vscode.Position).line,
        statement.executed,
      ]);
    }

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
      const fc = assertCoverageCounts(uri, received[0], 2, 3, "covered lines");
      assert.deepStrictEqual(
        lineHits(fc),
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
    // --coverage-json → parsed hits → FileCoverage. The fixture covers the
    // function, test registration and assertion (1, 5, 6); unused (3) stays
    // uncovered. Every source line and its hit count must reach the gutter.
    test("the Coverage profile path yields covered=3/total=4 for the fixture", async function () {
      const { osprey, controller, file, uri: coverageUri } = await staged(
        this,
        "calc-proof.test.osp",
        COVERAGE_FIXTURE,
        30000,
      );
      const received: vscode.FileCoverage[] = [];
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([file]),
        coverageSink(recordingRun(received)),
        token(),
        () => osprey,
        COVERAGE_RUN,
      );
      assert.strictEqual(received.length, 1, "one FileCoverage per suite file");
      const fc = assertCoverageCounts(
        coverageUri,
        received[0],
        3,
        4,
        "executed lines",
      );
      assert.deepStrictEqual(
        lineHits(fc),
        [
          [0, 1],
          [2, 0],
          [4, 1],
          [5, 1],
        ],
        "every executable source line keeps its exact hit count",
      );
      const zeroHit = detailedCoverageFor(fc).find(
        (s) => (s.location as vscode.Position).line === 2,
      );
      assert.strictEqual(zeroHit?.executed, 0, "dead fn renders as uncovered");
    });

    test("several leaves of one file run as sequential filtered invocations", async function () {
      const { controller, file } = await budgeted(this, failUri, 30000);
      const bad = requiredLeaf(file, failUri, "bad math");
      const good = requiredLeaf(file, failUri, "good math");
      const sink = await runItems(controller, [bad, good]);
      assert.deepStrictEqual(idsOf(sink, "failed"), [bad.id]);
      assert.deepStrictEqual(idsOf(sink, "passed"), [good.id]);
      const kinds = sink.events.map((e) => `${e.kind}:${e.id ?? ""}`);
      assert.ok(
        kinds.indexOf(`failed:${bad.id}`) <
          kinds.indexOf(`enqueued:${good.id}`),
      );
    });

    test("a compile error marks the file item errored with stderr", async function () {
      const osprey = requireCompiler(this, compiler, 30000);
      const controller = newController();
      const file = await refreshTestFile(controller, brokenUri, osprey);
      const sink = await runWholeFile(controller, file);
      assertVerdicts(sink, "errored", [file.id], /syntax error/);
      assert.strictEqual(sink.ofKind("passed").length, 0);
    });

    test("a requested leaf absent from the output is skipped", async function () {
      requireCompiler(this, compiler, 30000);
      const { controller, file } = await discovered(passUri);
      const ghost = controller.createTestItem(
        leafTestId(passUri.toString(), "ghost test"),
        "ghost test",
        passUri,
      );
      file.children.add(ghost);
      const sink = await runItems(controller, [ghost]);
      assert.deepStrictEqual(idsOf(sink, "skipped"), [ghost.id]);
    });

    test("a request without include runs every root", async function () {
      requireCompiler(this, compiler, 30000);
      const controller = newController();
      await discoveredFile(controller, passUri);
      const request = new vscode.TestRunRequest();
      assert.strictEqual(requestedItems(controller, request).length, 1);
      const sink = await runTo(controller, request);
      assert.strictEqual(sink.ofKind("passed").length, 2);
    });

    test("a whole-file run resolves children on demand", async function () {
      requireCompiler(this, compiler, 30000);
      const controller = newController();
      const bare = controller.createTestItem(
        fileTestId(passUri.toString()),
        "pass.test.osp",
        passUri,
      );
      controller.items.add(bare);
      const sink = await runItems(controller, [bare]);
      assert.strictEqual(bare.children.size, 2);
      assert.strictEqual(sink.ofKind("passed").length, 2);
    });

    test("excluded leaves never execute during a whole-file run", async function () {
      const { controller, file } = await budgeted(this, failUri, 30000);
      const bad = requiredLeaf(file, failUri, "bad math");
      const sink = await runTo(
        controller,
        new vscode.TestRunRequest([file], [bad]),
      );
      assert.strictEqual(sink.ofKind("failed").length, 0);
      assert.deepStrictEqual(idsOf(sink, "passed"), [
        leafTestId(failUri.toString(), "good math"),
      ]);
      // The excluded case must not run at all — the run degrades to filtered
      // per-leaf invocations, so its TAP line never appears in the output.
      assert.ok(!sink.output.includes("bad math"));
      assert.strictEqual(sink.ofKind("started").length, 1);
    });

    test("a stray assertion failure outside any test marks the file failed", async function () {
      requireCompiler(this, compiler, 30000);
      const { file, sink } = await runFile(strayUri);
      // The one real case passed, but the run must not look green: the file
      // item fails with the stray diagnostic.
      assert.deepStrictEqual(idsOf(sink, "passed"), [
        leafTestId(strayUri.toString(), "fine"),
      ]);
      assertVerdicts(
        sink,
        "failed",
        [file.id],
        /expect failed: expected 5, got 2/,
      );
      assert.strictEqual(sink.ofKind("errored").length, 0);
    });

    test("an unfiltered run scrubs an inherited OSPREY_TEST_FILTER", async function () {
      const { controller, file } = await budgeted(this, passUri, 30000);
      process.env.OSPREY_TEST_FILTER = "zero identity";
      let sink: RecordingSink;
      try {
        sink = await runWholeFile(controller, file);
      } finally {
        delete process.env.OSPREY_TEST_FILTER;
      }
      // Both cases ran despite the stray filter in the editor's environment.
      assertTwoPassed(sink);
    });

    test("a pre-cancelled token produces no results", async function () {
      const { osprey, controller, file } = await budgeted(this, passUri, 30000);
      const source = new vscode.CancellationTokenSource();
      source.cancel();
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([file]),
        sink,
        source.token,
        () => osprey,
      );
      assert.deepStrictEqual(sink.events, [{ kind: "end" }]);
    });

    test("cancellation mid-run kills the compiler and reports nothing", async function () {
      const { osprey, controller, file } = await budgeted(this, passUri, 30000);
      const source = new vscode.CancellationTokenSource();
      // Cancel deterministically the instant the run reports `started` — that
      // fires before the compiler is awaited, so the post-run cancellation
      // guard always wins the race (a wall-clock timer here is flaky: a fast or
      // cached compile can finish and report a result before the timer lands).
      const sink = new RecordingSink();
      const cancellingSink = overriding(sink, "started", (target) => (test) => {
        target.started(test);
        source.cancel();
      });
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([file]),
        cancellingSink,
        source.token,
        () => osprey,
      );
      const resultKinds = ["passed", "failed", "errored", "skipped"];
      assert.ok(
        sink.events.every((event) => !resultKinds.includes(event.kind)),
      );
      assertRunEnded(sink);
    });

    // The unstoppable-run regression: a throw anywhere in the run loop (a
    // rejected reporting call on a cancelled TestRun, a discovery error) must
    // STILL end the run — an un-ended run spins forever in the Testing view
    // and its Stop button is dead.
    test("a run whose reporting throws still ends (and rethrows)", async function () {
      const { osprey, controller, file } = await budgeted(this, passUri, 30000);
      const sink = new RecordingSink();
      const boom = new Error("TestRun already ended");
      const throwingSink = overriding(sink, "enqueued", () => () => {
        throw boom;
      });
      await assert.rejects(
        executeRunRequest(
          controller,
          new vscode.TestRunRequest([file]),
          throwingSink,
          token(),
          () => osprey,
        ),
        boom,
      );
      assertRunEnded(sink);
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
      const { osprey, controller, file } = await budgeted(this, passUri, 30000);
      const good = requiredLeaf(file, passUri, "addition works");
      const handler = makeRunHandler(controller, () => osprey);
      await handler(new vscode.TestRunRequest([good]), token());
      assert.ok(true, "run handler completed against a real TestRun");
    });
  });
});
