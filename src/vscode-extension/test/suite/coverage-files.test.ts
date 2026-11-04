// Tests for mapping a `--coverage-json` report onto the files it covers
// ([TESTING-COVERAGE-VSCODE], [TESTING-PROJECT-SUITE]).

import * as assert from "assert";
import * as path from "path";
import { coverageFiles } from "../../client/src/coverage-files";

suite("coverageFiles", () => {
  const ran = path.resolve("/work/app/tests/money.test.osp");
  const cwd = path.resolve("/work/app/tests");

  test("a file suite's one entry belongs to the file that ran", () => {
    const files = coverageFiles(ran, cwd, new Map([["money.test.osp", 1]]));
    assert.deepStrictEqual([...files], [[ran, 1]]);
  });

  test("a project suite keys every source by its own absolute path", () => {
    const framework = path.resolve("/work/framework/src/calc.ospml");
    const report = new Map([
      [framework, 2],
      ["main.ospml", 3],
    ]);
    const files = coverageFiles(ran, cwd, report);
    assert.deepStrictEqual(
      [...files],
      [
        [framework, 2],
        [path.join(cwd, "main.ospml"), 3],
      ],
    );
  });

  test("an empty report covers nothing", () => {
    assert.strictEqual(coverageFiles(ran, cwd, new Map()).size, 0);
  });
});
