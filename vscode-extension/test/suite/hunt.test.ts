// Tests for turning an `osprey hunt --json` report into editor diagnostics
// ([SYMEX-CLI], [SYMEX-FINDINGS]). The engine is KLEE; the editor only reads
// the report.

import * as assert from "assert";
import * as path from "path";
import { DiagnosticSeverity } from "vscode";
import {
  HUNT_SOURCE,
  HuntReport,
  findingMessage,
  findingPath,
  findingSeverity,
  huntDiagnostics,
  huntSummary,
  parseHuntReport,
} from "../../client/src/hunt";

const program = path.resolve("/work/app/main.osp");

function report(overrides: Partial<HuntReport> = {}): HuntReport {
  return {
    program,
    engine: "KLEE",
    budgetSeconds: 60,
    explored: { paths: 12, completed: 10, instructions: 5000, elapsed: "00:00:03" },
    findings: [
      {
        class: "memory",
        message: "nullptr-with-offset",
        file: "main.osp",
        line: 7,
        runtimeFrame: "runtime/json_runtime.c:237 in close_container",
        stdin: "9\\n",
        replay: "reproduced",
        replayDetail: "SIGSEGV",
      },
      {
        class: "unmodelled",
        message: "unmodelled: pipe",
        file: null,
        line: null,
        runtimeFrame: null,
        stdin: "",
        replay: "skipped",
        replayDetail: "an unmodelled call stops the path under KLEE only",
      },
    ],
    incomplete: [],
    ...overrides,
  };
}

suite("hunt report", () => {
  test("parses the JSON report and rejects anything else", () => {
    assert.strictEqual(parseHuntReport(JSON.stringify(report()))?.engine, "KLEE");
    assert.strictEqual(parseHuntReport("not json"), undefined);
    assert.strictEqual(parseHuntReport("{}"), undefined);
  });

  test("summarises findings, replay and exploration in one line", () => {
    assert.strictEqual(
      huntSummary(report()),
      "Hunt: 2 findings (1 reproduced natively), 12 paths explored, 10 normal exits.",
    );
    assert.strictEqual(
      huntSummary(report({ findings: [] })),
      "Hunt reported no findings; analysis unfinished (12 paths explored, 10 normal exits).",
    );
  });

  test("unfinished and unexplored runs are never described as clean", () => {
    const bodies = [
      report({ findings: [] }),
      report({ findings: [], incomplete: [{ test: "cut", reason: "time budget" }] }),
      report({ findings: [], explored: { paths: 0, completed: 0, instructions: 0, elapsed: "" } }),
    ];
    for (const body of bodies) {
      assert.ok(huntSummary(body).includes("unfinished"), huntSummary(body));
      assert.ok(!huntSummary(body).includes("found no bugs"), huntSummary(body));
    }
  });

  test("a fully completed empty report makes only a bounded no-findings claim", () => {
    const explored = { paths: 1, completed: 1, instructions: 42, elapsed: "00:00:01" };
    assert.strictEqual(huntSummary(report({ findings: [], explored })),
      "Hunt reported no findings: 1 paths explored, 1 normal exits.");
    const extra = { ...report(), context: { engineArguments: ["--max-time=8s"] } };
    assert.strictEqual(parseHuntReport(JSON.stringify(extra))?.engine, "KLEE");
  });

  test("a reproduced finding is an error, klee-only a warning, unmodelled information", () => {
    const [memory, unmodelled] = report().findings;
    assert.strictEqual(findingSeverity(memory), DiagnosticSeverity.Error);
    assert.strictEqual(findingSeverity({ ...memory, replay: "klee-only" }), DiagnosticSeverity.Warning);
    assert.strictEqual(findingSeverity(unmodelled), DiagnosticSeverity.Information);
  });

  test("a relative file resolves beside the program, an absolute one stands", () => {
    const [memory] = report().findings;
    assert.strictEqual(findingPath(memory, program), path.resolve("/work/app/main.osp"));
    assert.strictEqual(findingPath({ ...memory, file: "/abs/other.osp" }, program), "/abs/other.osp");
    assert.strictEqual(findingPath({ ...memory, file: null }, program), program);
  });

  test("diagnostics sit on the finding's line and name the engine", () => {
    const byFile = huntDiagnostics(report(), program);
    const diagnostics = byFile.get(program) ?? [];
    assert.strictEqual(diagnostics.length, 2);
    assert.strictEqual(diagnostics[0].range.start.line, 6);
    assert.strictEqual(diagnostics[0].source, HUNT_SOURCE);
    assert.strictEqual(diagnostics[0].code, "memory");
    assert.strictEqual(diagnostics[1].range.start.line, 0, "a finding without a line lands on line 1");
    assert.ok(findingMessage(report().findings[0]).includes("replay reproduced (SIGSEGV)"));
    assert.ok(findingMessage(report().findings[0]).includes('stdin "9\\n"'));
  });
});

// Permutations over the editor contract in SymbolicExecution.md's Editor
// section: "a `reproduced` finding is an error, a `klee-only` or unreplayed
// one a warning, an `unmodelled` one information" [SYMEX-FINDINGS].
suite("hunt report permutations", () => {
  const base = report().findings[0];

  test("every defect class at every replay verdict gets the spec severity", () => {
    const verdicts: [string, DiagnosticSeverity][] = [
      ["reproduced", DiagnosticSeverity.Error],
      ["klee-only", DiagnosticSeverity.Warning],
      ["not-run", DiagnosticSeverity.Warning],
      ["skipped", DiagnosticSeverity.Warning],
    ];
    for (const findingClass of ["memory", "abort", "arith", "ub"]) {
      for (const [replay, severity] of verdicts) {
        assert.strictEqual(findingSeverity({ ...base, class: findingClass, replay }), severity, `${findingClass}/${replay}`);
      }
    }
    for (const replay of ["not-run", "skipped", "klee-only"]) {
      assert.strictEqual(
        findingSeverity({ ...base, class: "unmodelled", replay }),
        DiagnosticSeverity.Information,
        `unmodelled/${replay}`,
      );
    }
  });

  test("only a JSON object with a findings array is a report", () => {
    const rejected = ["", " ", "null", "42", '"text"', "[]", "{}", '{"findings": {}}', '{"findings": null}', '{"findings": "x"}', '{"findings": [', "osprey hunt: klee was not found"];
    for (const stdout of rejected) {
      assert.strictEqual(parseHuntReport(stdout), undefined, JSON.stringify(stdout));
    }
    const pretty = `${JSON.stringify(report(), null, 2)}\n`;
    for (const stdout of [JSON.stringify(report()), pretty, `\n  ${pretty}`, '{"findings": []}']) {
      assert.ok(Array.isArray(parseHuntReport(stdout)?.findings), JSON.stringify(stdout).slice(0, 40));
    }
  });

  test("the CLI's full JSON shape, extra fields and all, becomes diagnostics", () => {
    const cli = {
      program,
      engine: "KLEE",
      budgetSeconds: 9,
      inputBytes: 0,
      explored: { paths: 3, completed: 2, instructions: 99, elapsed: "" },
      findings: [
        {
          test: "test000001",
          class: "arith",
          suffix: "div",
          message: "divide by zero",
          file: "main.osp",
          line: 3,
          runtimeFrame: null,
          stdin: "\x00\xff",
          files: [{ path: "in.txt", bytes: "abc" }],
          replay: "klee-only",
          replayDetail: "",
        },
      ],
      incomplete: [{ test: "test000002", reason: "Execution halting." }],
    };
    const parsed = parseHuntReport(JSON.stringify(cli));
    assert.ok(parsed);
    const diagnostics = huntDiagnostics(parsed, program).get(program) ?? [];
    assert.strictEqual(diagnostics.length, 1);
    assert.strictEqual(diagnostics[0].severity, DiagnosticSeverity.Warning);
    assert.strictEqual(diagnostics[0].range.start.line, 2);
    assert.strictEqual(diagnostics[0].code, "arith");
    assert.strictEqual(
      diagnostics[0].message,
      'arith: divide by zero\nstdin "\x00\xff"\nreplay klee-only',
    );
  });

  test("the summary counts findings and reproductions in every mix", () => {
    const at = (replay: string) => ({ ...base, replay });
    const cases: [HuntReport["findings"], string][] = [
      [[at("klee-only")], "Hunt: 1 finding (0 reproduced natively), 12 paths explored, 10 normal exits."],
      [[at("reproduced")], "Hunt: 1 finding (1 reproduced natively), 12 paths explored, 10 normal exits."],
      [[at("reproduced"), at("reproduced"), at("skipped")], "Hunt: 3 findings (2 reproduced natively), 12 paths explored, 10 normal exits."],
      [[at("not-run"), at("not-run")], "Hunt: 2 findings (0 reproduced natively), 12 paths explored, 10 normal exits."],
    ];
    for (const [findings, summary] of cases) {
      assert.strictEqual(huntSummary(report({ findings })), summary);
    }
    const incomplete = [{ test: "t", reason: "Execution halting." }];
    assert.strictEqual(
      huntSummary(report({ findings: [], incomplete })),
      "Hunt reported no findings; analysis unfinished (12 paths explored, 10 normal exits).",
      "incomplete paths are not findings",
    );
  });

  test("the message carries exactly the parts a finding has", () => {
    const bare = { ...base, runtimeFrame: null, stdin: "", replay: "not-run", replayDetail: "" };
    const cases: [typeof base, string][] = [
      [bare, "memory: nullptr-with-offset\nreplay not-run"],
      [{ ...bare, runtimeFrame: "r.c:1 in f" }, "memory: nullptr-with-offset\nruntime r.c:1 in f\nreplay not-run"],
      [{ ...bare, stdin: "42\n" }, 'memory: nullptr-with-offset\nstdin "42\n"\nreplay not-run'],
      [{ ...bare, replay: "skipped", replayDetail: "why" }, "memory: nullptr-with-offset\nreplay skipped (why)"],
      [
        { ...bare, runtimeFrame: "r.c:1 in f", stdin: "x", replay: "reproduced", replayDetail: "SIGABRT" },
        'memory: nullptr-with-offset\nruntime r.c:1 in f\nstdin "x"\nreplay reproduced (SIGABRT)',
      ],
    ];
    for (const [finding, message] of cases) {
      assert.strictEqual(findingMessage(finding), message);
    }
  });

  test("files resolve beside the program in every relative shape", () => {
    const dir = path.dirname(program);
    const cases: [string | null, string][] = [
      [null, program],
      ["main.osp", path.join(dir, "main.osp")],
      ["sub/lib.osp", path.join(dir, "sub/lib.osp")],
      ["../shared/lib.ospml", path.join(dir, "../shared/lib.ospml")],
      ["./here.osp", path.join(dir, "here.osp")],
      [path.resolve("/elsewhere/x.osp"), path.resolve("/elsewhere/x.osp")],
    ];
    for (const [file, expected] of cases) {
      assert.strictEqual(findingPath({ ...base, file }, program), expected, String(file));
    }
  });

  test("diagnostics group by file, keep order and clamp lines", () => {
    const findings = [
      { ...base, file: "a.osp", line: 1, message: "first" },
      { ...base, file: "b.osp", line: 1000, message: "second" },
      { ...base, file: "a.osp", line: 0, message: "third" },
      { ...base, file: "a.osp", line: null, message: "fourth" },
    ];
    const byFile = huntDiagnostics(report({ findings }), program);
    const dir = path.dirname(program);
    const a = byFile.get(path.join(dir, "a.osp")) ?? [];
    const b = byFile.get(path.join(dir, "b.osp")) ?? [];
    assert.strictEqual(byFile.size, 2);
    assert.deepStrictEqual(a.map((d) => d.range.start.line), [0, 0, 0]);
    assert.deepStrictEqual(a.map((d) => d.message.split(": ")[1].split("\n")[0]), ["first", "third", "fourth"]);
    assert.strictEqual(b[0].range.start.line, 999);
    assert.ok(a.every((d) => d.source === HUNT_SOURCE && d.range.start.character === 0));
    assert.ok(b[0].range.end.character > 1000, "the squiggle spans the whole line");
    assert.strictEqual(huntDiagnostics(report({ findings: [] }), program).size, 0);
  });
});
