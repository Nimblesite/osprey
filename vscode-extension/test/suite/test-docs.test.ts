// Integration tests for test-case documentation in the Test Explorer
// ([TESTING-DOC]) and for the Profile run profile ([TESTING-PROFILE]). These
// drive a REAL vscode.TestController against the freshly built osprey compiler
// and real fixture files on disk: discovery really shells out to
// `osprey <file> --list-tests`, so the compiler's wire format, the extension's
// parsing, the TestItem properties VS Code renders, and the profiler artifacts
// are all proven end to end.

import * as assert from "assert";
import * as fs from "fs";
import * as os from "os";
import * as path from "path";
import * as vscode from "vscode";
import {
  documentedTestIds,
  executeRunRequest,
  refreshTestFile,
  registerOspreyTestExplorer,
  testDocFor,
  type TestRunSink,
} from "../../client/src/test-explorer";
import { leafTestId } from "../../client/src/test-explorer-parse";
import { testDocMarkdown } from "../../client/src/test-explorer-docs";
import {
  NO_TEST_MESSAGE,
  resolveTargetId,
  showTestDocumentation,
} from "../../client/src/test-docs-panel";
import {
  makeProfileRoot,
  presentProfile,
  profileMode,
  profileSink,
  registerTestProfileProfile,
} from "../../client/src/test-profile";
import { resolveBuiltOsprey } from "./osprey-test-env";
import {
  DOC_FAIL_FIXTURE,
  DOC_FIXTURE,
  ML_DOC_FIXTURE,
  PROFILE_FIXTURE,
  RecordingSink,
} from "./test-explorer-harness";

/** A RecordingSink that also captures the profile directories reported. */
class ProfileRecordingSink extends RecordingSink {
  public readonly profiles: { path: string; dir: string }[] = [];
  public addProfile(uri: vscode.Uri, dir: string): void {
    this.profiles.push({ path: uri.fsPath, dir });
  }
}

suite("Osprey test documentation and profiling", () => {
  const compiler = resolveBuiltOsprey();
  let fixtureDir: string;
  let docUri: vscode.Uri;
  let docFailUri: vscode.Uri;
  let mlDocUri: vscode.Uri;
  let profileUri: vscode.Uri;
  const disposables: vscode.Disposable[] = [];
  let controllerSequence = 0;

  function newController(): vscode.TestController {
    controllerSequence += 1;
    const context = {
      subscriptions: disposables,
    } as unknown as vscode.ExtensionContext;
    return registerOspreyTestExplorer(
      context,
      () => compiler ?? "osprey",
      `ospreyTests-docs-${controllerSequence}`,
    );
  }

  function writeFixture(name: string, content: string): vscode.Uri {
    const filePath = path.join(fixtureDir, name);
    fs.writeFileSync(filePath, content);
    return vscode.Uri.file(filePath);
  }

  function token(): vscode.CancellationToken {
    return new vscode.CancellationTokenSource().token;
  }

  async function discovered(uri: vscode.Uri): Promise<{
    controller: vscode.TestController;
    file: vscode.TestItem;
  }> {
    const controller = newController();
    const file = await refreshTestFile(controller, uri, compiler ?? "osprey");
    assert.strictEqual(file.error, undefined, String(file.error));
    return { controller, file };
  }

  function leaf(file: vscode.TestItem, uri: vscode.Uri, name: string) {
    const item = file.children.get(leafTestId(uri.toString(), name));
    assert.ok(item, `case "${name}" discovered`);
    return item;
  }

  suiteSetup(() => {
    fixtureDir = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-test-docs-"));
    docUri = writeFixture("docs.test.osp", DOC_FIXTURE);
    docFailUri = writeFixture("docfail.test.osp", DOC_FAIL_FIXTURE);
    mlDocUri = writeFixture("docs.test.ospml", ML_DOC_FIXTURE);
    profileUri = writeFixture("profiled.test.osp", PROFILE_FIXTURE);
  });

  suiteTeardown(() => {
    fs.rmSync(fixtureDir, { recursive: true, force: true });
  });

  teardown(() => {
    for (const disposable of disposables.splice(0)) {
      disposable.dispose();
    }
  });

  // ---------------------------------------------------------------- discovery

  suite("discovery carries documentation onto TestItems", () => {
    /**
     * A leaf's description must carry the WHOLE doc, not just its first
     * paragraph. `vscode.TestItem` has no tooltip of its own, so the row's
     * description is the only documentation its hover can show — pinning it to
     * the summary showed one line of a multi-paragraph block
     * ([TESTING-DOC-VSCODE]).
     */
    function assertWholeDoc(
      description: string | undefined,
      ...fragments: string[]
    ): void {
      assert.ok(description !== undefined, "documented case is described");
      for (const fragment of fragments) {
        assert.ok(description.includes(fragment), `${fragment}: ${description}`);
      }
    }

    test("a documented case's whole doc becomes the TestItem description", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const { file } = await discovered(docUri);
      assert.strictEqual(file.children.size, 3, "every case discovered");
      const rich = leaf(file, docUri, "documented case");
      assert.strictEqual(rich.label, "documented case");
      assertWholeDoc(
        rich.description,
        "Addition is commutative.",
        "Swapping the operands cannot change the sum",
        "**Parameters**",
        "**Returns**",
        "**Raises**",
        "**Since**",
      );
    });

    test("a summary-only case describes with its one line", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const { file } = await discovered(docUri);
      assert.strictEqual(
        leaf(file, docUri, "summary only").description,
        "Zero is the additive identity.",
      );
    });

    test("an undocumented case has no description", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const { file } = await discovered(docUri);
      assert.strictEqual(
        leaf(file, docUri, "undocumented case").description,
        undefined,
      );
    });

    test("a function's own doc does not leak onto the cases", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const { file } = await discovered(docUri);
      for (const [, item] of file.children) {
        assert.notStrictEqual(
          item.description,
          "Adds two integers.",
          `"${item.label}" must not inherit fn add's doc`,
        );
      }
    });

    test("the range still marks the test( call, not the doc block", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const { file } = await discovered(docUri);
      // DOC_FIXTURE declares the cases on 1-based lines 23, 26 and 28; the
      // `///` block above the first starts on line 4. VS Code ranges are
      // 0-based, and the gutter marker belongs on the `test(` call.
      assert.strictEqual(
        leaf(file, docUri, "documented case").range?.start.line,
        22,
      );
      assert.strictEqual(
        leaf(file, docUri, "summary only").range?.start.line,
        25,
      );
      assert.strictEqual(
        leaf(file, docUri, "undocumented case").range?.start.line,
        27,
      );
    });

    test("the full doc is stashed for the detail panel, section by section", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const { file } = await discovered(docUri);
      const id = leaf(file, docUri, "documented case").id;
      const doc = testDocFor(id);
      assert.ok(doc, "documentation recorded for the leaf");
      assert.strictEqual(doc.name, "documented case");
      assert.strictEqual(doc.summary, "Addition is commutative.");
      assert.strictEqual(doc.line, 23);
      for (const needle of [
        "Addition is commutative.",
        "Swapping the operands cannot change the sum, so both orders agree.",
        "**Parameters**",
        "- `left` — the first addend",
        "- `right` — the second addend",
        "**Returns**",
        "Unit, reported through `expect`.",
        "**Raises**",
        "- `Overflow` — when the sum leaves int range",
        "**See also**",
        "[add]",
        "**Since**",
        "0.3",
      ]) {
        assert.ok(
          doc.markdown.includes(needle),
          `missing ${needle} in:\n${doc.markdown}`,
        );
      }
    });

    test("an undocumented case records an empty doc, not a missing one", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const { file } = await discovered(docUri);
      const doc = testDocFor(leaf(file, docUri, "undocumented case").id);
      assert.ok(doc, "a record exists");
      assert.strictEqual(doc.summary, "");
      assert.strictEqual(doc.markdown, "");
      assert.strictEqual(doc.line, 28);
    });

    test("the ML flavor's (** … *) blocks reach the same TestItems", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const { file } = await discovered(mlDocUri);
      assert.strictEqual(file.children.size, 2);
      const documented = leaf(file, mlDocUri, "ml documented");
      assertWholeDoc(
        documented.description,
        "Addition is commutative.",
        "Swapping the operands cannot change the sum.",
      );
      assert.ok(
        testDocFor(documented.id)?.markdown.includes(
          "Swapping the operands cannot change the sum.",
        ),
        "the ML body renders",
      );
      assert.strictEqual(
        leaf(file, mlDocUri, "ml bare").description,
        undefined,
      );
    });

    test("re-resolving a file after the doc is deleted drops the description", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const uri = writeFixture("edited-docs.test.osp", DOC_FIXTURE);
      const controller = newController();
      const before = await refreshTestFile(controller, uri, compiler);
      assertWholeDoc(
        leaf(before, uri, "documented case").description,
        "Addition is commutative.",
        "Swapping the operands cannot change the sum",
      );
      fs.writeFileSync(
        uri.fsPath,
        'fn add(a, b) = a + b\ntest("documented case", fn() => expect(add(1, 1), 2))\n',
      );
      const after = await refreshTestFile(controller, uri, compiler);
      const stale = leaf(after, uri, "documented case");
      assert.strictEqual(stale.description, undefined, "description cleared");
      assert.strictEqual(testDocFor(stale.id)?.markdown, "", "doc cleared");
      assert.strictEqual(testDocFor(stale.id)?.line, 2, "line refreshed");
    });

    test("every discovered leaf is registered in the docs index", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const { file } = await discovered(docUri);
      const ids = documentedTestIds();
      for (const [, item] of file.children) {
        assert.ok(ids.includes(item.id), `${item.label} indexed`);
      }
    });
  });

  // ------------------------------------------------------------------ running

  suite("documentation reaches run results", () => {
    test("a documented case's failure message leads with its documentation", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const { controller, file } = await discovered(docFailUri);
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([file]),
        sink,
        token(),
        () => compiler,
      );
      const failed = sink.ofKind("failed");
      assert.strictEqual(failed.length, 1, "the one case failed");
      const message = failed[0].message ?? "";
      assert.ok(
        message.includes("Proves the broken invariant."),
        `documentation present:\n${message}`,
      );
      assert.ok(message.includes("**Since**"), message);
      assert.ok(message.includes("0.9"), message);
      assert.ok(message.includes("## Context For AI"), message);
      assert.ok(message.includes("- Test: documented failure"), message);
      assert.ok(message.includes("- Status: failed"), message);
      assert.ok(
        message.includes("- Documentation: Proves the broken invariant."),
        message,
      );
      assert.ok(
        message.indexOf("Proves the broken invariant.") <
          message.indexOf("## Context For AI"),
        "docs come first",
      );
    });

    test("running a documented suite still reports every verdict", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const { controller, file } = await discovered(docUri);
      const sink = new RecordingSink();
      await executeRunRequest(
        controller,
        new vscode.TestRunRequest([file]),
        sink,
        token(),
        () => compiler,
      );
      assert.strictEqual(sink.ofKind("passed").length, 3, "all three passed");
      assert.strictEqual(sink.ofKind("failed").length, 0);
      assert.strictEqual(sink.ofKind("end").length, 1);
    });
  });

  // -------------------------------------------------------- documentation UI

  suite("Show Test Documentation command", () => {
    test("a TestItem argument resolves to that case's documentation", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const { file } = await discovered(docUri);
      const item = leaf(file, docUri, "documented case");
      const shown: string[] = [];
      const markdown = showTestDocumentation(
        item,
        (md) => shown.push(md),
        undefined,
        () => assert.fail("no notification expected"),
      );
      assert.ok(markdown, "markdown returned");
      assert.strictEqual(shown.length, 1);
      assert.ok(shown[0].startsWith("### documented case\n"), shown[0]);
      assert.ok(shown[0].includes("**Parameters**"), shown[0]);
      assert.ok(
        shown[0].includes(`Declared at \`${docUri.fsPath}:23\``),
        shown[0],
      );
    });

    test("a bare id string resolves the same way", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const { file } = await discovered(docUri);
      const id = leaf(file, docUri, "summary only").id;
      const markdown = showTestDocumentation(
        id,
        () => undefined,
        undefined,
        () => assert.fail("no notification expected"),
      );
      assert.ok(markdown?.startsWith("### summary only\n"), markdown);
      assert.ok(markdown?.includes("Zero is the additive identity."), markdown);
    });

    test("an undocumented case still opens, explaining how to document it", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const { file } = await discovered(docUri);
      const item = leaf(file, docUri, "undocumented case");
      const markdown = showTestDocumentation(
        item,
        () => undefined,
        undefined,
        () => assert.fail("no notification expected"),
      );
      assert.ok(markdown?.includes("No documentation"), markdown);
      assert.ok(markdown?.includes("`///`"), markdown);
    });

    test("an unresolvable invocation notifies instead of throwing", () => {
      const notices: string[] = [];
      const markdown = showTestDocumentation(
        { id: "not-a-test-id" },
        () => assert.fail("nothing to present"),
        undefined,
        (message) => notices.push(message),
      );
      assert.strictEqual(markdown, undefined);
      assert.deepStrictEqual(notices, [NO_TEST_MESSAGE]);
    });

    test("resolveTargetId picks the nearest case at or above the cursor", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const { file } = await discovered(docUri);
      const rich = leaf(file, docUri, "documented case").id;
      const summary = leaf(file, docUri, "summary only").id;
      const bare = leaf(file, docUri, "undocumented case").id;
      // 0-based cursor lines map to 1-based declaration lines 23, 26, 28.
      assert.strictEqual(resolveTargetId(undefined, docUri, 22), rich);
      assert.strictEqual(resolveTargetId(undefined, docUri, 24), rich);
      assert.strictEqual(resolveTargetId(undefined, docUri, 25), summary);
      assert.strictEqual(resolveTargetId(undefined, docUri, 27), bare);
      assert.strictEqual(resolveTargetId(undefined, docUri, 99), bare);
      // Above every case there is nothing to resolve.
      assert.strictEqual(resolveTargetId(undefined, docUri, 0), undefined);
      // Another file's cursor never resolves into this file's cases.
      assert.strictEqual(
        resolveTargetId(undefined, vscode.Uri.file("/nope.test.osp"), 30),
        undefined,
      );
      // No editor at all.
      assert.strictEqual(
        resolveTargetId(undefined, undefined, undefined),
        undefined,
      );
    });

    test("a TestItem argument wins over the cursor position", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const { file } = await discovered(docUri);
      const bare = leaf(file, docUri, "undocumented case");
      // The cursor sits on the documented case, but the explicit item wins.
      assert.strictEqual(resolveTargetId(bare, docUri, 22), bare.id);
    });

    test("the rendered panel matches testDocMarkdown for the same record", async function () {
      if (!compiler) {
        this.skip();
      }
      this.timeout(30000);
      const { file } = await discovered(docUri);
      const item = leaf(file, docUri, "documented case");
      const record = testDocFor(item.id);
      assert.ok(record);
      assert.strictEqual(
        showTestDocumentation(
          item,
          () => undefined,
          undefined,
          () => undefined,
        ),
        testDocMarkdown(record, docUri.fsPath),
      );
    });
  });

  // ---------------------------------------------------------------- profiling

  suite("Profile run profile ([TESTING-PROFILE])", () => {
    test("registerTestProfileProfile adds a non-default Run profile named Profile", () => {
      const controller = newController();
      const profile = registerTestProfileProfile(
        controller,
        () => compiler ?? "osprey",
        () => undefined,
      );
      assert.strictEqual(profile.label, "Profile");
      assert.strictEqual(profile.kind, vscode.TestRunProfileKind.Run);
      assert.strictEqual(
        profile.isDefault,
        false,
        "the play button keeps launching the plain Run profile",
      );
      profile.dispose();
    });

    test("profileMode carries the artifact root", async () => {
      const root = await makeProfileRoot();
      try {
        const mode = profileMode(root);
        assert.strictEqual(mode.kind, "profile");
        assert.strictEqual(
          mode.kind === "profile" ? mode.dir : undefined,
          root,
        );
        assert.ok(fs.existsSync(root), "the root really exists on disk");
      } finally {
        fs.rmSync(root, { recursive: true, force: true });
      }
    });

    test("makeProfileRoot produces a fresh directory per request", async () => {
      const [a, b] = await Promise.all([makeProfileRoot(), makeProfileRoot()]);
      try {
        assert.notStrictEqual(a, b, "no collisions between concurrent runs");
        assert.ok(path.basename(a).startsWith("osprey-test-profile-"));
      } finally {
        fs.rmSync(a, { recursive: true, force: true });
        fs.rmSync(b, { recursive: true, force: true });
      }
    });

    test("a profiling run reports TAP verdicts AND writes profiler artifacts", async function () {
      if (!compiler || process.platform === "win32") {
        this.skip(); // the sampling profiler is POSIX-only ([PROF-CLI-RUN])
      }
      this.timeout(120000);
      const { controller, file } = await discovered(profileUri);
      const sink = new ProfileRecordingSink();
      const root = await makeProfileRoot();
      try {
        await executeRunRequest(
          controller,
          new vscode.TestRunRequest([file]),
          sink,
          token(),
          () => compiler,
          profileMode(root),
        );
        // The verdicts are unchanged by profiling.
        assert.strictEqual(sink.ofKind("passed").length, 1, sink.output);
        assert.strictEqual(sink.ofKind("failed").length, 0, sink.output);
        assert.ok(sink.output.includes("ok 1 - profiled work"), sink.output);
        // And the artifacts landed in this run's directory.
        assert.strictEqual(sink.profiles.length, 1, "one suite profiled");
        assert.strictEqual(sink.profiles[0].path, profileUri.fsPath);
        const dir = sink.profiles[0].dir;
        assert.ok(dir.startsWith(root), `${dir} is under ${root}`);
        for (const ext of [
          "profile.json",
          "speedscope.json",
          "cpuprofile",
          "folded",
        ]) {
          assert.ok(
            fs.existsSync(path.join(dir, `profiled.test.${ext}`)),
            `${ext} export written into ${dir}: ${fs.readdirSync(dir).join(", ")}`,
          );
        }
      } finally {
        fs.rmSync(root, { recursive: true, force: true });
      }
    });

    test("presentProfile loads the artifacts, opens the flame panel, and applies heat", async function () {
      if (!compiler || process.platform === "win32") {
        this.skip();
      }
      this.timeout(120000);
      const { controller, file } = await discovered(profileUri);
      const sink = new ProfileRecordingSink();
      const root = await makeProfileRoot();
      try {
        await executeRunRequest(
          controller,
          new vscode.TestRunRequest([file]),
          sink,
          token(),
          () => compiler,
          profileMode(root),
        );
        const dir = sink.profiles[0]?.dir;
        assert.ok(dir, "a profile directory was reported");
        const shown: string[] = [];
        const applied: number[] = [];
        const heat = {
          apply: (summary: { sampleCount: number }) =>
            applied.push(summary.sampleCount),
        } as unknown as Parameters<typeof presentProfile>[2];
        const outcome = presentProfile(
          profileUri,
          dir,
          heat,
          (_model, _summary, sourcePath) => shown.push(sourcePath),
        );
        assert.strictEqual(outcome.loaded, true, outcome.detail);
        assert.strictEqual(outcome.uri.fsPath, profileUri.fsPath);
        assert.deepStrictEqual(
          shown,
          [profileUri.fsPath],
          "flame panel opened",
        );
        assert.strictEqual(applied.length, 1, "heat decorations applied");
        assert.ok(applied[0] > 0, "the profile really collected samples");
        assert.ok(outcome.detail.length > 0, "a summary line for the output");
      } finally {
        fs.rmSync(root, { recursive: true, force: true });
      }
    });

    test("a directory with no artifacts reports a failure instead of throwing", () => {
      const empty = fs.mkdtempSync(path.join(os.tmpdir(), "osprey-noprof-"));
      try {
        const outcome = presentProfile(
          vscode.Uri.file(path.join(empty, "x.test.osp")),
          empty,
          undefined,
          () => assert.fail("nothing to show"),
        );
        assert.strictEqual(outcome.loaded, false);
        assert.ok(
          outcome.detail.includes("profiler output missing"),
          outcome.detail,
        );
      } finally {
        fs.rmSync(empty, { recursive: true, force: true });
      }
    });

    test("profileSink forwards verdicts to the run and narrates the profile", () => {
      const events: string[] = [];
      const output: string[] = [];
      const run = {
        enqueued: () => events.push("enqueued"),
        started: () => events.push("started"),
        passed: () => events.push("passed"),
        failed: () => events.push("failed"),
        errored: () => events.push("errored"),
        skipped: () => events.push("skipped"),
        appendOutput: (text: string) => output.push(text),
        end: () => events.push("end"),
      } as unknown as vscode.TestRun;
      const sink: TestRunSink = profileSink(run, undefined, (uri, dir) => ({
        uri,
        dir,
        loaded: true,
        detail: "12 samples over 0.4s",
      }));
      const item = { id: "x", label: "x" } as unknown as vscode.TestItem;
      sink.enqueued(item);
      sink.started(item);
      sink.passed(item);
      sink.skipped(item);
      sink.appendOutput("ok 1 - x\r\n");
      sink.addProfile?.(vscode.Uri.file("/w/s.test.osp"), "/tmp/p/s");
      sink.end();
      assert.deepStrictEqual(events, [
        "enqueued",
        "started",
        "passed",
        "skipped",
        "end",
      ]);
      const narration = output.join("");
      assert.ok(narration.includes("ok 1 - x"), narration);
      assert.ok(narration.includes("# profiling /w/s.test.osp"), narration);
      assert.ok(narration.includes("# profile artifacts: /tmp/p/s"), narration);
      assert.ok(narration.includes("12 samples over 0.4s"), narration);
      assert.ok(
        !/(?<!\r)\n/.test(narration),
        `pseudoterminal output is CRLF: ${JSON.stringify(narration)}`,
      );
    });

    test("profiling one selected case filters through OSPREY_TEST_FILTER", async function () {
      if (!compiler || process.platform === "win32") {
        this.skip();
      }
      this.timeout(120000);
      const uri = writeFixture(
        "two-profiled.test.osp",
        `fn spin(n, acc) = match n <= 0 {
    true => acc
    false => spin((n - 1) ?: 0, (acc + n) ?: 0)
}

test("first profiled", fn() => expect(spin(500000, 0) > 0, true))

test("second profiled", fn() => expect(spin(500000, 0) > 0, true))
`,
      );
      const controller = newController();
      const file = await refreshTestFile(controller, uri, compiler);
      const only = file.children.get(
        leafTestId(uri.toString(), "second profiled"),
      );
      assert.ok(only);
      const sink = new ProfileRecordingSink();
      const root = await makeProfileRoot();
      try {
        await executeRunRequest(
          controller,
          new vscode.TestRunRequest([only]),
          sink,
          token(),
          () => compiler,
          profileMode(root),
        );
        assert.strictEqual(sink.ofKind("passed").length, 1);
        assert.ok(sink.output.includes("second profiled"), sink.output);
        assert.ok(!sink.output.includes("ok 1 - first profiled"), sink.output);
        assert.strictEqual(sink.profiles.length, 1);
      } finally {
        fs.rmSync(root, { recursive: true, force: true });
      }
    });

    test("a suite that fails to compile profiles nothing but still errors the item", async function () {
      if (!compiler || process.platform === "win32") {
        this.skip();
      }
      this.timeout(60000);
      const broken = writeFixture(
        "brokenprof.test.osp",
        'test("ok", fn() => expect(1, 1))\n',
      );
      const controller = newController();
      const file = await refreshTestFile(controller, broken, compiler);
      fs.writeFileSync(broken.fsPath, "fn broken( = nonsense !!\n");
      const sink = new ProfileRecordingSink();
      const root = await makeProfileRoot();
      try {
        await executeRunRequest(
          controller,
          new vscode.TestRunRequest([file]),
          sink,
          token(),
          () => compiler,
          profileMode(root),
        );
        assert.ok(
          sink.ofKind("errored").length > 0,
          "compile failure surfaced",
        );
        const dir = sink.profiles[0]?.dir;
        assert.ok(dir, "the directory is still reported");
        const outcome = presentProfile(broken, dir, undefined, () =>
          assert.fail("no panel for a failed compile"),
        );
        assert.strictEqual(outcome.loaded, false);
      } finally {
        fs.rmSync(root, { recursive: true, force: true });
      }
    });
  });
});
