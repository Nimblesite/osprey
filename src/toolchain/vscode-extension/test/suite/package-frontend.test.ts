// The frontend wire contract ([PACKAGE-API], src/osprey/packages/manager/README.md): one
// command line on stdin because the Osprey CLI has no argv builtin; the
// answer is one canonical `{"lines":[…],"status":N}` JSON line followed by
// the process's final `status: N` line. The editor requires the JSON's
// semantic status and IGNORES the process return code.

import * as assert from "assert";
import {
  parseFrontendReport,
  runFrontendCommand,
} from "../../client/src/package-frontend";

suite("package frontend wire contract", () => {
  test("the JSON line is the report; the trailing process status line is not it", () => {
    const stdout = '{"lines":["added @demo/one e1.r2"],"status":0}\nstatus: 0\n';
    assert.deepStrictEqual(parseFrontendReport(stdout), { status: 0, lines: ["added @demo/one e1.r2"] });
    // A nonzero process exit (the CLI cannot set one) must not turn a
    // semantically failed report into anything else.
    const failed = '{"lines":["add.exists @demo/one"],"status":1}\nstatus: 0\n';
    assert.strictEqual(parseFrontendReport(failed)?.status, 1);
  });

  test("non-JSON and wrong-shape stdout is not a report", () => {
    for (const stdout of ["", "status: 0", "not json", "[]", '{"lines":[]}', '{"status":0}', '{"lines":"x","status":0}', '{"lines":[1],"status":0}', '{"lines":[],"status":"0"}', "{ not json", '{"lines":["cut']) {
      assert.strictEqual(parseFrontendReport(stdout), undefined, stdout);
    }
    assert.deepStrictEqual(parseFrontendReport('{"lines":["added @a/b"],"status":0}\n{"lines":["cut\nstatus: 0\n'),
      { lines: ["added @a/b"], status: 0 }, "a truncated JSON line after the report is skipped, not fatal");
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

  test("a frontend that answers without a report rejects with what it printed", async () => {
    const answering = (stdout: string, stderr: string) => ({ execFile: ((_exe: unknown, _args: unknown, _options: unknown, callback: Function) => {
      callback(null, stdout, stderr);
      return { stdin: { end() {} } };
    }) as any });
    await assert.rejects(runFrontendCommand("frontend", "--json add @a/b", "/tmp", answering("usage: osprey-pkg\n", "")),
      /did not answer with a JSON report: usage: osprey-pkg$/, "stdout explains a silent stderr");
    await assert.rejects(runFrontendCommand("frontend", "--json add @a/b", "/tmp", answering("partial", "lock busy\n")),
      /did not answer with a JSON report: lock busy$/, "stderr explains the failure first");
    const report = await runFrontendCommand("frontend", "--json add @a/b", "/tmp", answering('{"lines":["added @a/b"],"status":0}\n', ""));
    assert.deepStrictEqual(report, { lines: ["added @a/b"], status: 0 });
  });
});
