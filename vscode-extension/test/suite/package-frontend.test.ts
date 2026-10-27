// The frontend wire contract ([PACKAGE-API], packages/manager/README.md): one
// command line on stdin because the Osprey CLI has no argv builtin; the
// answer is one canonical `{"lines":[…],"status":N}` JSON line followed by
// the process's final `status: N` line. The editor requires the JSON's
// semantic status and IGNORES the process return code.

import * as assert from "assert";
import {
  buildStdinCommand,
  parseFrontendReport,
  semanticSuccess,
  runFrontendCommand,
} from "../../client/src/package-frontend";

suite("package frontend wire contract", () => {
  test("a command is one whitespace-joined line with --project . for spaces", () => {
    assert.strictEqual(
      buildStdinCommand(["--json", "add", "@demo/one", "--registry", "http://127.0.0.1:8787", "--project", "."]),
      "--json add @demo/one --registry http://127.0.0.1:8787 --project .",
    );
    assert.strictEqual(buildStdinCommand(["remove", "@a/b", "--project", "."]), "remove @a/b --project .");
  });

  test("a part containing whitespace is a bug, not a shell injection waiting to happen", () => {
    assert.throws(() => buildStdinCommand(["add", "/my spaced/checkout", "--project", "."]));
    assert.throws(() => buildStdinCommand(["add", "@a/b\nc"]));
  });

  test("the JSON line is the report; the trailing process status line is not it", () => {
    const stdout = '{"lines":["added @demo/one e1.r2"],"status":0}\nstatus: 0\n';
    assert.deepStrictEqual(parseFrontendReport(stdout), { status: 0, lines: ["added @demo/one e1.r2"] });
    // A nonzero process exit (the CLI cannot set one) must not turn a
    // semantically failed report into anything else.
    const failed = '{"lines":["add.exists @demo/one"],"status":1}\nstatus: 0\n';
    assert.strictEqual(parseFrontendReport(failed)?.status, 1);
  });

  test("non-JSON and wrong-shape stdout is not a report", () => {
    for (const stdout of ["", "status: 0", "not json", "[]", '{"lines":[]}', '{"status":0}', '{"lines":"x","status":0}', '{"lines":[1],"status":0}', '{"lines":[],"status":"0"}']) {
      assert.strictEqual(parseFrontendReport(stdout), undefined, stdout);
    }
  });

  test("only a semantic zero status is success", () => {
    assert.strictEqual(semanticSuccess({ status: 0, lines: [] }), true);
    for (const status of [1, 2, 3, -1]) {
      assert.strictEqual(semanticSuccess({ status, lines: [] }), false, String(status));
    }
  });

  test("failure lines carry the CLI's stable reason codes through", () => {
    const report = parseFrontendReport('{"lines":["remove.missing @a/b"],"status":1}\nstatus: 0\n');
    assert.deepStrictEqual(report?.lines, ["remove.missing @a/b"]);
  });
});

suite("package subprocess failures", () => {
  test("a crashed frontend cannot report successful mutation", async () => {
    const spawn = { execFile: ((_exe: unknown, _args: unknown, _options: unknown, callback: Function) => {
      callback(new Error("terminated by signal"), '{"lines":["added"],"status":0}', "");
      return { stdin: { end() {} } };
    }) as any };
    await assert.rejects(runFrontendCommand("frontend", "--json add @a/b", "/tmp", spawn), /signal/);
  });
});
