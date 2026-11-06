// Which file each `--coverage-json` entry covers ([TESTING-COVERAGE-VSCODE]).
// Pure, so the mapping is tested without a controller or a compiler.

import * as path from "path";

/**
 * Key each `[TESTING-COVERAGE-JSON]` entry by the absolute path of the file
 * it covers. A file suite reports only itself, so its one entry belongs to the
 * file that ran (`ranPath`). A project suite reports every source it measured
 * under that source's own path ([TESTING-PROJECT-SUITE]); a relative one is
 * resolved against the directory the compiler ran in.
 */
export function coverageFiles<T>(
  ranPath: string,
  cwd: string,
  report: ReadonlyMap<string, T>,
): Map<string, T> {
  const [only] = report.values();
  if (report.size === 1 && only !== undefined) {
    return new Map([[ranPath, only]]);
  }
  return new Map(
    [...report].map(([file, hits]) => [path.resolve(cwd, file), hits]),
  );
}
