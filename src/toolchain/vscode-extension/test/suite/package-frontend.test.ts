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
    for (const stdout of ["", "status: 0", "not json", "[]", '{"lines":[]}', '{"status":0}', '{"lines":"x","status":0}', '{"lines":[1],"status":0}', '{"lines":[],"status":"0"}']) {
      assert.strictEqual(parseFrontendReport(stdout), undefined, stdout);
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
