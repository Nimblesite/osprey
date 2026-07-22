// Tests for the Osprey Test Explorer's pure logic ([TESTING-VSCODE]): TAP
// stream parsing, `--list-tests` JSON parsing, id/range helpers, outcome
// mapping, and run planning. No fixtures, compiler, or controller needed —
// the discovery/run flows are covered in test-explorer.test.ts.

import * as assert from "assert";
import {
  compileFailureMessage,
  coverageCounts,
  coverageRunArgs,
  discoveryOutcome,
  excludedIdSet,
  fileTestId,
  isCompileFailure,
  leafTestId,
  outcomeForLeaf,
  parseCoverageJson,
  parseTapOutput,
  parseTapStream,
  parseTestList,
  planRun,
  strayFailureMessage,
  testRangeStart,
  testRunEnv,
  toTerminalOutput,
  type TestItemLike,
} from "../../client/src/test-explorer-parse";

suite("Test Explorer parsing", () => {
  suite("parseTapOutput", () => {
    test("maps result lines and attaches preceding diagnostics", () => {
      const stream = [
        "# expect failed: expected 3, got 2",
        "not ok 1 - bad math",
        "ok 2 - good math",
        "1..2",
        "# tests=2 passed=1 failed=1",
      ].join("\n");
      assert.deepStrictEqual(parseTapOutput(stream), [
        {
          name: "bad math",
          ok: false,
          comments: ["expect failed: expected 3, got 2"],
        },
        { name: "good math", ok: true, comments: [] },
      ]);
    });

    test("joins multiple diagnostics, resets between results, ignores noise and CRLF", () => {
      const stream =
        "program output\r\n# first\r\n# second\r\nnot ok 1 - x\r\nok 2 - y\r\n1..2\r\n";
      assert.deepStrictEqual(parseTapOutput(stream), [
        { name: "x", ok: false, comments: ["first", "second"] },
        { name: "y", ok: true, comments: [] },
      ]);
    });

    test("returns no results for empty output or compile-error text", () => {
      assert.deepStrictEqual(parseTapOutput(""), []);
      assert.deepStrictEqual(
        parseTapOutput('broken.test.osp:1:0: syntax error near "fn"'),
        [],
      );
    });

    test("captures names byte-exact: leading/trailing spaces and dashes survive", () => {
      const results = parseTapOutput(
        "ok 1 -  padded\nok 2 - trailing  \nnot ok 3 - a - b - c\n1..3\n",
      );
      assert.deepStrictEqual(
        results.map((result) => result.name),
        [" padded", "trailing  ", "a - b - c"],
      );
    });

    test("a SKIP directive splits the name from the reason and marks the case skipped", () => {
      const results = parseTapOutput(
        "ok 1 - runs\nok 2 - precondition unmet # SKIP not on a full moon\nok 3 - bare skip # SKIP\n1..3\n",
      );
      assert.deepStrictEqual(results, [
        { name: "runs", ok: true, comments: [] },
        {
          name: "precondition unmet",
          ok: true,
          comments: [],
          skipReason: "not on a full moon",
        },
        { name: "bare skip", ok: true, comments: [], skipReason: "" },
      ]);
    });

    test("parseTapStream reports the plan line and stray trailing diagnostics", () => {
      const stream = parseTapStream(
        "ok 1 - fine\n# expect failed: expected 5, got 2\n1..1\n# tests=1 passed=1 failed=0\n",
      );
      assert.strictEqual(stream.sawPlan, true);
      assert.deepStrictEqual(
        stream.results.map((result) => result.name),
        ["fine"],
      );
      assert.deepStrictEqual(stream.strayComments, [
        "expect failed: expected 5, got 2",
        "tests=1 passed=1 failed=0",
      ]);
      const empty = parseTapStream("1..0\n# tests=0 passed=0 failed=0\n");
      assert.strictEqual(empty.sawPlan, true);
      assert.deepStrictEqual(empty.results, []);
      assert.strictEqual(parseTapStream("").sawPlan, false);
    });
  });

  suite("parseTestList and discoveryOutcome", () => {
    const valid =
      '[{"name":"a","line":3,"column":1},{"name":"b","line":8,"column":2}]';

    test("parses a valid list and an empty list", () => {
      const parsed = parseTestList(valid);
      assert.ok(parsed.ok);
      assert.deepStrictEqual(
        parsed.tests.map((t) => t.name),
        ["a", "b"],
      );
      const empty = parseTestList("[]");
      assert.ok(empty.ok);
      assert.deepStrictEqual(empty.tests, []);
    });

    test("rejects malformed JSON, non-arrays, and malformed entries", () => {
      const invalid = parseTestList("not json {");
      assert.ok(!invalid.ok);
      assert.match(invalid.error, /invalid JSON/);
      const nonArray = parseTestList('{"name":"a"}');
      assert.ok(!nonArray.ok);
      assert.match(nonArray.error, /JSON array/);
      const badEntry = parseTestList('[{"name":"a"}]');
      assert.ok(!badEntry.ok);
      assert.match(badEntry.error, /malformed/);
      const nonObject = parseTestList("[5]");
      assert.ok(!nonObject.ok);
    });

    test("discoveryOutcome folds exit codes and stderr", () => {
      assert.ok(
        discoveryOutcome({ stdout: valid, stderr: "", exitCode: 0 }).ok,
      );
      const failed = discoveryOutcome({
        stdout: "",
        stderr: "x.osp:1:0: bad\n",
        exitCode: 1,
      });
      assert.ok(!failed.ok);
      assert.strictEqual(failed.error, "x.osp:1:0: bad");
      const silent = discoveryOutcome({ stdout: "", stderr: "", exitCode: 3 });
      assert.ok(!silent.ok);
      assert.match(silent.error, /exited with code 3/);
      assert.ok(
        !discoveryOutcome({ stdout: "garbage", stderr: "", exitCode: 0 }).ok,
      );
    });
  });

  suite("ids, ranges, and outcomes", () => {
    test("file and leaf ids are stable and distinct per test name", () => {
      const uri = "file:///work/a.test.osp";
      assert.strictEqual(fileTestId(uri), uri);
      assert.ok(leafTestId(uri, "adds").startsWith(uri));
      assert.notStrictEqual(
        leafTestId(uri, "adds"),
        leafTestId(uri, "subtracts"),
      );
      assert.ok(leafTestId(uri, "adds").includes("adds"));
    });

    test("testRangeStart converts 1-based positions and clamps at zero", () => {
      assert.deepStrictEqual(
        testRangeStart({ name: "t", line: 3, column: 5 }),
        { line: 2, character: 4 },
      );
      assert.deepStrictEqual(
        testRangeStart({ name: "t", line: 0, column: 0 }),
        { line: 0, character: 0 },
      );
    });

    test("outcomeForLeaf maps pass, fail, fallback message, absent, and duplicates", () => {
      const results = [
        { name: "p", ok: true, comments: [] },
        { name: "f", ok: false, comments: ["expected 3, got 2", "second"] },
        { name: "bare", ok: false, comments: [] },
        { name: "dup", ok: true, comments: [] },
        { name: "dup", ok: false, comments: ["later loss"] },
      ];
      assert.deepStrictEqual(outcomeForLeaf("p", results), {
        status: "passed",
      });
      assert.deepStrictEqual(outcomeForLeaf("f", results), {
        status: "failed",
        message: "expected 3, got 2\nsecond",
      });
      assert.deepStrictEqual(outcomeForLeaf("bare", results), {
        status: "failed",
        message: "Test failed: bare",
      });
      assert.deepStrictEqual(outcomeForLeaf("missing", results), {
        status: "skipped",
      });
      assert.strictEqual(outcomeForLeaf("dup", results).status, "failed");
    });

    test("outcomeForLeaf reports a SKIP-directive case as skipped", () => {
      const results = [
        {
          name: "s",
          ok: true,
          comments: [],
          skipReason: "precondition not met",
        },
        { name: "t", ok: true, comments: [], skipReason: "" },
      ];
      assert.deepStrictEqual(outcomeForLeaf("s", results), {
        status: "skipped",
      });
      assert.deepStrictEqual(outcomeForLeaf("t", results), {
        status: "skipped",
      });
    });

    test("isCompileFailure and compileFailureMessage", () => {
      const noTap = parseTapStream("x.osp:1:0: syntax error");
      assert.strictEqual(isCompileFailure(1, noTap), true);
      assert.strictEqual(isCompileFailure(0, noTap), false);
      assert.strictEqual(
        isCompileFailure(1, parseTapStream("not ok 1 - t\n1..1")),
        false,
      );
      // A plan line proves the tests ran, even with zero results (1..0).
      assert.strictEqual(isCompileFailure(1, parseTapStream("1..0")), false);
      assert.strictEqual(
        compileFailureMessage(" x.osp:1:1: bad \n", 1),
        "x.osp:1:1: bad",
      );
      assert.match(compileFailureMessage("", 2), /exited with code 2/);
    });

    test("strayFailureMessage flags all-ok runs that exit non-zero", () => {
      const stray = parseTapStream(
        "ok 1 - fine\n# expect failed: expected 5, got 2\n1..1\n# tests=1 passed=1 failed=0\n",
      );
      assert.strictEqual(
        strayFailureMessage(stray, 1, ""),
        "expect failed: expected 5, got 2",
      );
      // Diagnostics attached to passing results are collected too.
      const attached = parseTapStream("# early stray\nok 1 - fine\n1..1\n");
      assert.strictEqual(strayFailureMessage(attached, 1, ""), "early stray");
      // No diagnostics: stderr, then a generic message.
      const bare = parseTapStream("ok 1 - fine\n1..1\n");
      assert.strictEqual(strayFailureMessage(bare, 1, " boom \n"), "boom");
      assert.match(
        String(strayFailureMessage(bare, 1, "")),
        /exited with code 1/,
      );
      // Explained failures produce nothing.
      assert.strictEqual(strayFailureMessage(bare, 0, ""), undefined);
      assert.strictEqual(
        strayFailureMessage(parseTapStream("not ok 1 - t\n1..1"), 1, ""),
        undefined,
      );
    });

    test("testRunEnv sets the filter or scrubs an inherited one, without mutating", () => {
      const base: NodeJS.ProcessEnv = {
        PATH: "/bin",
        OSPREY_TEST_FILTER: "stale",
      };
      const unfiltered = testRunEnv(base, undefined);
      assert.strictEqual(unfiltered.OSPREY_TEST_FILTER, undefined);
      assert.ok(!("OSPREY_TEST_FILTER" in unfiltered));
      assert.strictEqual(unfiltered.PATH, "/bin");
      const filtered = testRunEnv(base, "one test");
      assert.strictEqual(filtered.OSPREY_TEST_FILTER, "one test");
      assert.strictEqual(base.OSPREY_TEST_FILTER, "stale");
    });

    test("toTerminalOutput normalizes line endings to CRLF", () => {
      assert.strictEqual(toTerminalOutput("a\nb\n"), "a\r\nb\r\n");
      assert.strictEqual(toTerminalOutput("a\r\nb"), "a\r\nb");
    });

    // [TESTING-COVERAGE-CLI] chrome from `osprey test` never masquerades as a
    // stray-failure diagnostic.
    test("strayFailureMessage drops osprey test runner chrome", () => {
      const stream = parseTapStream(
        [
          "# file: money.test.ospml",
          "ok 1 - fine",
          "1..1",
          "# coverage: 83.3% (5/6 lines) money.test.ospml",
          "# coverage total: 83.3% (5/6 lines)",
          "# suites: 1 passed, 0 failed",
        ].join("\n"),
      );
      assert.strictEqual(strayFailureMessage(stream, 1, "boom"), "boom");
    });
  });

  suite("coverage report", () => {
    // [TESTING-COVERAGE-CLI]: the arguments for one instrumented run.
    test("coverageRunArgs builds the osprey test invocation", () => {
      assert.deepStrictEqual(
        coverageRunArgs("m.test.osp", "cov.json", undefined),
        ["test", "m.test.osp", "--coverage-json", "cov.json", "--quiet"],
      );
      assert.deepStrictEqual(
        coverageRunArgs("m.test.osp", "cov.json", "one case").slice(-2),
        ["--filter", "one case"],
      );
    });

    // [TESTING-COVERAGE-JSON] parsing and the covered/total badge numbers.
    test("parseCoverageJson maps files to line hits; malformed input degrades to absent", () => {
      const report = parseCoverageJson(
        '{"files":{"m.test.osp":{"lines":{"3":2,"7":0,"12":1}}}}',
      );
      assert.ok(report);
      const hits = report.get("m.test.osp");
      assert.ok(hits);
      assert.strictEqual(hits.get(7), 0);
      assert.deepStrictEqual(coverageCounts(hits), { covered: 2, total: 3 });

      assert.strictEqual(parseCoverageJson("not json"), undefined);
      assert.strictEqual(parseCoverageJson('{"nope":true}'), undefined);
      assert.strictEqual(
        parseCoverageJson('{"files":{"f":{"lines":{"x":1}}}}'),
        undefined,
      );
      assert.strictEqual(parseCoverageJson('{"files":{"f":{}}}'), undefined);
    });
  });

  suite("planRun", () => {
    const fileA: TestItemLike = { id: "A" };
    const leafA1: TestItemLike = { id: "A#1", parent: fileA };
    const leafA2: TestItemLike = { id: "A#2", parent: fileA };
    const fileB: TestItemLike = { id: "B" };
    const leafB1: TestItemLike = { id: "B#1", parent: fileB };

    test("a requested file becomes a whole-file plan", () => {
      const plans = planRun([fileA]);
      assert.strictEqual(plans.length, 1);
      assert.strictEqual(plans[0].file.id, "A");
      assert.strictEqual(plans[0].wholeFile, true);
    });

    test("leaves group by file; a file request subsumes its leaves", () => {
      const leafPlans = planRun([leafA1, leafA2, leafB1]);
      assert.strictEqual(leafPlans.length, 2);
      assert.deepStrictEqual(
        leafPlans[0].leaves.map((l) => l.id),
        ["A#1", "A#2"],
      );
      assert.strictEqual(leafPlans[0].wholeFile, false);
      const merged = planRun([leafA1, fileA]);
      assert.strictEqual(merged.length, 1);
      assert.strictEqual(merged[0].wholeFile, true);
    });

    test("excluded files and leaves drop out", () => {
      assert.deepStrictEqual(planRun([fileA], excludedIdSet([fileA])), []);
      assert.deepStrictEqual(planRun([leafA1], excludedIdSet([fileA])), []);
      const plans = planRun([leafA1, leafA2], excludedIdSet([leafA2]));
      assert.deepStrictEqual(
        plans[0].leaves.map((l) => l.id),
        ["A#1"],
      );
      assert.strictEqual(excludedIdSet(undefined).size, 0);
    });
  });
});
