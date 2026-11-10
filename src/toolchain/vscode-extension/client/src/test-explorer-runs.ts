// Pure run modes for Test Explorer ([TESTING-VSCODE]): the arguments and
// environment a plain or coverage run passes the compiler, and the per-line
// coverage JSON it reports back.

/** One file's coverage: 1-based source line → hit count ([TESTING-COVERAGE-JSON]). */
export type LineHits = ReadonlyMap<number, number>;

/**
 * The CLI arguments for one coverage run of a test file
 * ([TESTING-COVERAGE-CLI]): `osprey test` instruments the build, writes the
 * machine-readable report, and streams the same TAP the plain run produces.
 * `--quiet` drops the per-file chrome; `--filter` scopes to one case.
 */
export function coverageRunArgs(
  filePath: string,
  jsonPath: string,
  filter: string | undefined,
): string[] {
  return [
    "test",
    filePath,
    "--coverage-json",
    jsonPath,
    "--quiet",
    ...(filter === undefined ? [] : ["--filter", filter]),
  ];
}

/**
 * How one run profile executes a test file. `plain` is the default Run
 * profile; `coverage` instruments through `osprey test`
 * ([TESTING-COVERAGE-CLI]); `profile` runs the suite under the sampling CPU
 * profiler and drops its export artifacts in `dir` ([TESTING-PROFILE],
 * [PROF-CLI-RUN]).
 */
export type RunMode =
  | { readonly kind: "plain" }
  | { readonly kind: "coverage" }
  | { readonly kind: "profile"; readonly dir: string };

export const PLAIN_RUN: RunMode = { kind: "plain" };

export const COVERAGE_RUN: RunMode = { kind: "coverage" };

/**
 * The CLI arguments for one run of `filePath` under `mode`. Only the coverage
 * mode consumes `jsonPath`; the profiler needs no argument beyond `--profile`
 * because its artifacts land in the process's working directory
 * ([PROF-CLI-RUN]).
 */
export function runArgsFor(
  mode: RunMode,
  filePath: string,
  jsonPath: string | undefined,
  filter: string | undefined,
): string[] {
  if (mode.kind === "coverage" && jsonPath !== undefined) {
    return coverageRunArgs(filePath, jsonPath, filter);
  }
  if (mode.kind === "profile") {
    return [filePath, "--run", "--profile"];
  }
  return [filePath, "--run"];
}

/**
 * The filter a run of `mode` passes through OSPREY_TEST_FILTER. The coverage
 * mode carries its filter as a `--filter` ARGUMENT instead, so it must never
 * also inherit the environment variable ([TESTING-FILTER]); the plain and
 * profile modes have no such flag and rely on the variable.
 */
export function envFilterFor(
  mode: RunMode,
  filter: string | undefined,
): string | undefined {
  return mode.kind === "coverage" ? undefined : filter;
}

function parsedLineHits(value: unknown): Map<number, number> | undefined {
  const lines = (value as { lines?: unknown }).lines;
  if (typeof lines !== "object" || lines === null) {
    return undefined;
  }
  const hits = new Map<number, number>();
  for (const [line, count] of Object.entries(lines)) {
    const lineNumber = Number(line);
    if (!Number.isInteger(lineNumber) || typeof count !== "number") {
      return undefined;
    }
    hits.set(lineNumber, count);
  }
  return hits;
}

/**
 * Parse the `--coverage-json` report ([TESTING-COVERAGE-JSON]):
 * `{"files":{"<path>":{"lines":{"<line>":hits}}}}` → path → line hits.
 * `undefined` on any malformation — coverage then degrades to absent, never
 * to wrong numbers.
 */
export function parseCoverageJson(
  text: string,
): Map<string, LineHits> | undefined {
  let parsed: unknown;
  try {
    parsed = JSON.parse(text);
  } catch {
    return undefined;
  }
  const files = (parsed as { files?: unknown }).files;
  if (typeof files !== "object" || files === null) {
    return undefined;
  }
  const report = new Map<string, LineHits>();
  for (const [file, value] of Object.entries(files)) {
    const hits = parsedLineHits(value);
    if (hits === undefined) {
      return undefined;
    }
    report.set(file, hits);
  }
  return report;
}

/** Covered/total line counts for one file's hits (the summary badge numbers). */
export function coverageCounts(hits: LineHits): {
  covered: number;
  total: number;
} {
  let covered = 0;
  for (const count of hits.values()) {
    if (count > 0) {
      covered += 1;
    }
  }
  return { covered, total: hits.size };
}
