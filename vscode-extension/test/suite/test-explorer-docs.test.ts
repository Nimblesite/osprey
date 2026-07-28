// Unit tests for the pure test-documentation layer ([TESTING-DOC]) and the
// run-mode selection the Profile run profile adds ([TESTING-PROFILE]). No
// vscode APIs are touched — the wiring is covered in test-explorer.test.ts and
// test-profile.test.ts.

import * as assert from "assert";
import {
  failureMarkdown,
  isDocumented,
  profileRunHeader,
  testDescription,
  testDocMarkdown,
  testDocOf,
  type TestDoc,
} from "../../client/src/test-explorer-docs";
import {
  COVERAGE_RUN,
  PLAIN_RUN,
  coverageRunArgs,
  discoveryOutcome,
  envFilterFor,
  parseTestList,
  runArgsFor,
  type DiscoveredTest,
  type RunMode,
} from "../../client/src/test-explorer-parse";

const PROFILE_RUN: RunMode = { kind: "profile", dir: "/tmp/profile-root" };

const RICH_DOC = [
  "Addition is commutative.",
  "",
  "Swapping the operands cannot change the sum.",
  "",
  "**Parameters**",
  "",
  "- `left` — the first addend",
  "",
  "**Since**",
  "",
  "0.3",
].join("\n");

const RICH: DiscoveredTest = {
  name: "documented case",
  line: 22,
  column: 1,
  summary: "Addition is commutative.",
  doc: RICH_DOC,
};

const BARE: DiscoveredTest = { name: "bare case", line: 27, column: 1 };

suite("Test documentation (pure)", () => {
  suite("parsing the --list-tests wire form", () => {
    test("summary and doc survive JSON.parse onto DiscoveredTest", () => {
      const json = JSON.stringify([RICH, BARE]);
      const parsed = parseTestList(json);
      assert.strictEqual(parsed.ok, true);
      if (!parsed.ok) {
        return;
      }
      assert.strictEqual(parsed.tests.length, 2);
      assert.strictEqual(parsed.tests[0].summary, "Addition is commutative.");
      assert.strictEqual(parsed.tests[0].doc, RICH_DOC);
      assert.strictEqual(parsed.tests[0].line, 22);
      assert.strictEqual(parsed.tests[1].summary, undefined);
      assert.strictEqual(parsed.tests[1].doc, undefined);
    });

    test("the exact JSON the compiler emits parses", () => {
      // Byte-for-byte the shape testing.rs writes: docs are optional keys.
      const wire =
        '[{"name":"documented case","line":22,"column":1,"summary":"Adds.","doc":"Adds.\\n\\n**Since**\\n\\n0.3"},' +
        '{"name":"bare case","line":27,"column":1}]';
      const parsed = parseTestList(wire);
      assert.strictEqual(parsed.ok, true);
      if (!parsed.ok) {
        return;
      }
      assert.strictEqual(parsed.tests[0].doc, "Adds.\n\n**Since**\n\n0.3");
      assert.strictEqual(parsed.tests[1].doc, undefined);
    });

    test("a non-string summary or doc is rejected as malformed", () => {
      for (const bad of [
        '[{"name":"a","line":1,"column":1,"summary":7}]',
        '[{"name":"a","line":1,"column":1,"doc":{"x":1}}]',
        '[{"name":"a","line":1,"column":1,"doc":null}]',
      ]) {
        const parsed = parseTestList(bad);
        assert.strictEqual(parsed.ok, false, bad);
        if (!parsed.ok) {
          assert.ok(parsed.error.includes("malformed"), parsed.error);
        }
      }
    });

    test("docs ride through discoveryOutcome unchanged", () => {
      const outcome = discoveryOutcome({
        stdout: JSON.stringify([RICH]),
        stderr: "",
        exitCode: 0,
      });
      assert.strictEqual(outcome.ok, true);
      if (outcome.ok) {
        assert.strictEqual(outcome.tests[0].doc, RICH_DOC);
      }
    });
  });

  suite("testDescription — the greyed text beside the case name", () => {
    test("a documented case describes with its WHOLE doc, not just the summary", () => {
      // [TESTING-DOC] `vscode.TestItem` has no tooltip API — the only text the
      // Test Explorer can show when a row is hovered is `label` +
      // `description`. Describing with the summary alone therefore threw away
      // every paragraph and section after the first, and the hover bubble
      // showed one line of a doc block that had four.
      const description = testDescription(RICH);
      assert.ok(description !== undefined, "documented case has a description");
      for (const fragment of [
        "Addition is commutative.",
        "Swapping the operands cannot change the sum.",
        "**Parameters**",
        "- `left` — the first addend",
        "**Since**",
        "0.3",
      ]) {
        assert.ok(description.includes(fragment), `${fragment}: ${description}`);
      }
    });

    test("the whole doc collapses to a single tree row", () => {
      assert.strictEqual(
        testDescription(RICH),
        "Addition is commutative. Swapping the operands cannot change the sum. " +
          "**Parameters** - `left` — the first addend **Since** 0.3",
      );
    });

    test("a summary-only case describes with that summary", () => {
      assert.strictEqual(
        testDescription({ ...BARE, summary: "Adds.", doc: "Adds." }),
        "Adds.",
      );
    });

    test("a summary with no rendered doc still describes", () => {
      // The wire omits `doc` only when undocumented, but a description must
      // never depend on that: summary alone is enough to describe.
      assert.strictEqual(
        testDescription({ ...BARE, summary: "Adds." }),
        "Adds.",
      );
    });

    test("an undocumented case has no description at all", () => {
      assert.strictEqual(testDescription(BARE), undefined);
    });

    test("a blank or whitespace summary is treated as absent", () => {
      assert.strictEqual(testDescription({ ...BARE, summary: "" }), undefined);
      assert.strictEqual(
        testDescription({ ...BARE, summary: "   \n\t " }),
        undefined,
      );
    });

    test("a multi-line summary collapses to a single tree row", () => {
      assert.strictEqual(
        testDescription({ ...BARE, summary: "First line.\n  Second line." }),
        "First line. Second line.",
      );
    });
  });

  suite("testDocOf / isDocumented", () => {
    test("absent fields become empty strings, not undefined", () => {
      const doc = testDocOf(BARE);
      assert.deepStrictEqual(doc, {
        name: "bare case",
        summary: "",
        markdown: "",
        line: 27,
      });
      assert.strictEqual(isDocumented(doc), false);
    });

    test("a documented case keeps its summary, markdown, and line", () => {
      const doc = testDocOf(RICH);
      assert.strictEqual(doc.name, "documented case");
      assert.strictEqual(doc.summary, "Addition is commutative.");
      assert.strictEqual(doc.markdown, RICH_DOC);
      assert.strictEqual(doc.line, 22);
      assert.strictEqual(isDocumented(doc), true);
    });

    test("summary-only and markdown-only both count as documented", () => {
      assert.strictEqual(
        isDocumented(testDocOf({ ...BARE, summary: "s" })),
        true,
      );
      assert.strictEqual(isDocumented(testDocOf({ ...BARE, doc: "m" })), true);
      assert.strictEqual(isDocumented(undefined), false);
    });
  });

  suite("testDocMarkdown — the detail panel", () => {
    test("renders the name heading, the doc, and the declaration footer", () => {
      const md = testDocMarkdown(testDocOf(RICH), "/w/suite.test.osp");
      assert.ok(md.startsWith("### documented case\n"), md);
      assert.ok(md.includes("Addition is commutative."), md);
      assert.ok(md.includes("**Parameters**"), md);
      assert.ok(md.includes("- `left` — the first addend"), md);
      assert.ok(md.includes("**Since**"), md);
      assert.ok(md.includes("0.3"), md);
      assert.ok(md.includes("Declared at `/w/suite.test.osp:22`"), md);
    });

    test("an undocumented case still renders, saying how to document it", () => {
      const md = testDocMarkdown(testDocOf(BARE), "/w/suite.test.osp");
      assert.ok(md.startsWith("### bare case\n"), md);
      assert.ok(md.includes("No documentation"), md);
      assert.ok(md.includes("`///`"), md);
      assert.ok(md.includes("Declared at `/w/suite.test.osp:27`"), md);
    });
  });

  suite("failureMarkdown — the peek message", () => {
    const doc: TestDoc = testDocOf(RICH);
    const FAILURE = "expected 3, got 2";

    test("documentation precedes the failure, separated by a rule", () => {
      const md = failureMarkdown(
        FAILURE,
        doc,
        "/w/suite.test.osp",
        "line 22, column 1",
      );
      const docAt = md.indexOf("Addition is commutative.");
      const ruleAt = md.indexOf("---");
      const failAt = md.indexOf(FAILURE);
      assert.ok(docAt >= 0 && ruleAt > docAt && failAt > ruleAt, md);
    });

    test("the AI context block names file, test, status, location, failure and docs", () => {
      const md = failureMarkdown(
        FAILURE,
        doc,
        "/w/suite.test.osp",
        "line 22, column 1",
      );
      assert.ok(md.includes("## Context For AI"), md);
      assert.ok(md.includes("- File: /w/suite.test.osp"), md);
      assert.ok(md.includes("- Test: documented case"), md);
      assert.ok(md.includes("- Status: failed"), md);
      assert.ok(md.includes("- Location: line 22, column 1"), md);
      assert.ok(md.includes(`- Failure: ${FAILURE}`), md);
      assert.ok(md.includes("- Documentation: Addition is commutative."), md);
    });

    test("an undocumented case reports no documentation and skips the rule", () => {
      const md = failureMarkdown(
        FAILURE,
        testDocOf(BARE),
        "/w/suite.test.osp",
        "unknown",
      );
      assert.ok(md.startsWith(FAILURE), md);
      assert.ok(md.includes("- Documentation: none"), md);
      assert.ok(md.includes("- Location: unknown"), md);
    });

    test("a missing doc record still produces a well-formed block", () => {
      const md = failureMarkdown(FAILURE, undefined, "/w/x.osp", "unknown");
      assert.ok(md.includes("- Test: unknown"), md);
      assert.ok(md.includes("- Documentation: none"), md);
    });
  });

  suite("run modes ([TESTING-PROFILE])", () => {
    test("the plain mode runs `<file> --run`", () => {
      assert.deepStrictEqual(
        runArgsFor(PLAIN_RUN, "/w/s.test.osp", undefined, undefined),
        ["/w/s.test.osp", "--run"],
      );
    });

    test("the plain mode ignores a coverage json path", () => {
      assert.deepStrictEqual(
        runArgsFor(PLAIN_RUN, "/w/s.test.osp", "/tmp/c.json", "one case"),
        ["/w/s.test.osp", "--run"],
      );
    });

    test("the coverage mode delegates to coverageRunArgs", () => {
      assert.deepStrictEqual(
        runArgsFor(COVERAGE_RUN, "/w/s.test.osp", "/tmp/c.json", "one case"),
        coverageRunArgs("/w/s.test.osp", "/tmp/c.json", "one case"),
      );
      assert.deepStrictEqual(
        runArgsFor(COVERAGE_RUN, "/w/s.test.osp", "/tmp/c.json", undefined),
        ["test", "/w/s.test.osp", "--coverage-json", "/tmp/c.json", "--quiet"],
      );
    });

    test("a coverage mode without a json path degrades to a plain run", () => {
      assert.deepStrictEqual(
        runArgsFor(COVERAGE_RUN, "/w/s.test.osp", undefined, undefined),
        ["/w/s.test.osp", "--run"],
      );
    });

    test("the profile mode adds --profile and nothing else", () => {
      assert.deepStrictEqual(
        runArgsFor(PROFILE_RUN, "/w/s.test.osp", undefined, "one case"),
        ["/w/s.test.osp", "--run", "--profile"],
      );
    });

    test("only the coverage mode drops the env filter", () => {
      assert.strictEqual(envFilterFor(PLAIN_RUN, "one case"), "one case");
      assert.strictEqual(envFilterFor(PROFILE_RUN, "one case"), "one case");
      assert.strictEqual(envFilterFor(COVERAGE_RUN, "one case"), undefined);
      assert.strictEqual(envFilterFor(PLAIN_RUN, undefined), undefined);
      assert.strictEqual(envFilterFor(PROFILE_RUN, undefined), undefined);
    });
  });

  suite("profileRunHeader", () => {
    test("names the suite and where its artifacts landed", () => {
      const header = profileRunHeader("/w/s.test.osp", "/tmp/p/s.test.osp");
      assert.ok(header.includes("# profiling /w/s.test.osp"), header);
      assert.ok(
        header.includes("# profile artifacts: /tmp/p/s.test.osp"),
        header,
      );
      assert.ok(header.endsWith("\n"), header);
    });
  });
});
