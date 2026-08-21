// Pure logic for the Osprey Test Explorer ([TESTING-VSCODE]): parsing the
// compiler's `--list-tests` JSON and TAP run output, planning which files and
// leaf tests a run request targets, and mapping TAP results back onto test
// items. No `vscode` import — everything here unit-tests directly; the wiring
// lives in test-explorer.ts (mirroring debug-panel.ts's pure/wiring split).

/** One statically discovered test case from `osprey <file> --list-tests`. */
export interface DiscoveredTest {
  readonly name: string;
  /** 1-based source line of the `test(...)` call. */
  readonly line: number;
  /** 1-based source column of the `test(...)` call. */
  readonly column: number;
  /**
   * First paragraph of the `///` / `(** … *)` block written above the case —
   * the inline description shown beside its name. Absent when undocumented
   * ([TESTING-DOC]).
   */
  readonly summary?: string;
  /**
   * The whole doc comment rendered as Markdown (summary, body, and every
   * populated `# Parameters` / `# Returns` / `# Raises` / `# Examples` /
   * `# See also` / `# Since` section). Absent when undocumented
   * ([TESTING-DOC], [DOC-EXPORT]).
   */
  readonly doc?: string;
}

/** The outcome of parsing a `--list-tests` invocation. */
export type TestListParse =
  | { readonly ok: true; readonly tests: DiscoveredTest[] }
  | { readonly ok: false; readonly error: string };

/** One TAP result line with the `#` diagnostics that preceded it. */
export interface TapResult {
  /** The case name: the description with any trailing `# SKIP` directive
   *  removed. Equal to `description` when the case was not skipped. */
  readonly name: string;
  /**
   * The line's WHOLE description, directive text included. A test name may
   * itself contain `# SKIP`, which makes the raw line ambiguous
   * ([TESTING-TAP-AMBIGUITY]); keeping both readings lets `outcomeForLeaf`
   * break the tie against the names discovery actually found.
   */
  readonly description: string;
  readonly ok: boolean;
  readonly comments: string[];
  /** A `# SKIP` directive marked the case skipped ([TESTING-VERDICT]); its
   *  reason, if any. `undefined` when the case was not skipped. */
  readonly skipReason?: string;
}

/** What a run should report for one leaf test ([TESTING-TAP]). A skipped
 *  outcome carries the TAP `# SKIP` reason (possibly empty); `reason` is
 *  absent entirely when the case never appeared in the run's TAP at all. */
export type LeafOutcome =
  | { readonly status: "passed" }
  | { readonly status: "failed"; readonly message: string }
  | { readonly status: "skipped"; readonly reason?: string };

/** What one finished compiler process looked like. */
export interface ExecResult {
  readonly stdout: string;
  readonly stderr: string;
  readonly exitCode: number;
}

/** The shape of vscode.TestItem that run planning needs (two-level tree). */
export interface TestItemLike {
  readonly id: string;
  readonly parent?: TestItemLike | undefined;
}

/** One file's share of a run request. */
export interface FilePlan<T extends TestItemLike> {
  readonly file: T;
  /** Requested leaves; ignored when `wholeFile` is set. */
  readonly leaves: T[];
  wholeFile: boolean;
}

/** An optional wire field is valid when absent or a string ([TESTING-DOC]). */
function isOptionalString(value: unknown): boolean {
  return value === undefined || typeof value === "string";
}

function isDiscoveredTest(value: unknown): value is DiscoveredTest {
  const record = value as {
    name?: unknown;
    line?: unknown;
    column?: unknown;
    summary?: unknown;
    doc?: unknown;
  };
  return (
    typeof value === "object" &&
    value !== null &&
    typeof record.name === "string" &&
    typeof record.line === "number" &&
    typeof record.column === "number" &&
    isOptionalString(record.summary) &&
    isOptionalString(record.doc)
  );
}

/** Parse the JSON array printed by `--list-tests` ([TESTING-LIST]). */
export function parseTestList(json: string): TestListParse {
  let parsed: unknown;
  try {
    parsed = JSON.parse(json);
  } catch (error) {
    return { ok: false, error: `--list-tests printed invalid JSON: ${error}` };
  }
  if (!Array.isArray(parsed)) {
    return { ok: false, error: "--list-tests did not print a JSON array" };
  }
  const bad = parsed.find((entry) => !isDiscoveredTest(entry));
  if (bad !== undefined) {
    return {
      ok: false,
      error: `--list-tests entry is malformed: ${JSON.stringify(bad)}`,
    };
  }
  return { ok: true, tests: parsed as DiscoveredTest[] };
}

/** Fold a whole `--list-tests` process result into a parse outcome. */
export function discoveryOutcome(result: ExecResult): TestListParse {
  if (result.exitCode !== 0) {
    const detail = result.stderr.trim();
    return {
      ok: false,
      error: detail || `--list-tests exited with code ${result.exitCode}`,
    };
  }
  return parseTestList(result.stdout);
}

/** Everything a TAP stream carries beyond the per-case results. */
export interface TapStream {
  readonly results: TapResult[];
  /** `#` lines not attached to any following result (e.g. stray-assert diagnostics). */
  readonly strayComments: string[];
  /** Whether a `1..N` plan line was seen — proof the test runtime epilogue ran. */
  readonly sawPlan: boolean;
}

// The runtime prints results as exactly `ok N - name` / `not ok N - name`,
// with an optional ` # SKIP reason` directive on a skipped case ([TESTING-TAP],
// [TESTING-VERDICT]). The description is captured byte-exact (leading/trailing
// whitespace preserved) so it matches `--list-tests` names precisely; the
// directive is split off separately by SKIP_DIRECTIVE below.
const TAP_RESULT = /^(not )?ok \d+ - (.*)$/;
// The directive is always LAST on the line, so it is split from the RIGHT: a
// name that itself contains `# SKIP` keeps all of it ([TESTING-TAP-AMBIGUITY]).
const SKIP_DIRECTIVE = /^(.*) # SKIP ?(.*)$/;
const TAP_COMMENT = /^#\s?(.*)$/;
const TAP_PLAN = /^\d+\.\.\d+$/;
const TAP_SUMMARY = /^tests=\d+ passed=\d+ failed=\d+/;
// `osprey test` runner chrome ([TESTING-CLI-RUN], [TESTING-COVERAGE-CLI]):
// per-file headers, coverage rates, and the suite tally are progress lines,
// not failure diagnostics.
const TEST_RUNNER_CHROME = /^(file: |coverage(?: total)?: |suites: \d+ passed)/;

/**
 * Parse a TAP stream ([TESTING-TAP]): one entry per `ok`/`not ok` line, each
 * carrying the `#` diagnostic lines seen since the previous result line.
 * Ordinary program output is ignored; the plan line (`1..N`, always printed,
 * `1..0` included) sets `sawPlan`; `#` lines after the last result (stray
 * out-of-case assertion diagnostics, the trailing summary) become
 * `strayComments`.
 */
export function parseTapStream(stdout: string): TapStream {
  const results: TapResult[] = [];
  let comments: string[] = [];
  let sawPlan = false;
  for (const line of stdout.split(/\r?\n/)) {
    const result = TAP_RESULT.exec(line);
    if (result) {
      const description = result[2];
      const directive = SKIP_DIRECTIVE.exec(description);
      results.push({
        name: directive ? directive[1] : description,
        description,
        ok: result[1] === undefined,
        comments,
        ...(directive ? { skipReason: directive[2] } : {}),
      });
      comments = [];
    } else if (TAP_PLAN.test(line)) {
      sawPlan = true;
    } else {
      const comment = TAP_COMMENT.exec(line);
      if (comment) {
        comments.push(comment[1]);
      }
    }
  }
  return { results, strayComments: comments, sawPlan };
}

/** Just the per-case results of a TAP stream. */
export function parseTapOutput(stdout: string): TapResult[] {
  return parseTapStream(stdout).results;
}

// A leaf id embeds the parent uri and the exact test name; the separator can
// never occur in a `file:` uri string, so ids stay collision-free.
const LEAF_ID_SEPARATOR = " ";

/** The TestItem id for a test file (its uri string). */
export function fileTestId(uriString: string): string {
  return uriString;
}

/** The TestItem id for one named test inside a file. */
export function leafTestId(uriString: string, name: string): string {
  return `${uriString}${LEAF_ID_SEPARATOR}${name}`;
}

/** A zero-based document position (what vscode.Position takes). */
export interface ZeroBasedPosition {
  readonly line: number;
  readonly character: number;
}

/** Convert a discovered test's 1-based line/column to a 0-based position. */
export function testRangeStart(test: DiscoveredTest): ZeroBasedPosition {
  return {
    line: Math.max(0, test.line - 1),
    character: Math.max(0, test.column - 1),
  };
}

/** The ids a run request excludes (both file and leaf items). */
export function excludedIdSet(
  exclude: readonly TestItemLike[] | undefined,
): ReadonlySet<string> {
  return new Set((exclude ?? []).map((item) => item.id));
}

function isExcluded(
  item: TestItemLike,
  excluded: ReadonlySet<string>,
): boolean {
  for (
    let candidate: TestItemLike | undefined = item;
    candidate !== undefined;
    candidate = candidate.parent
  ) {
    if (excluded.has(candidate.id)) {
      return true;
    }
  }
  return false;
}

/**
 * Group a run request's items into per-file plans: a requested file item means
 * "run the whole file" (one unfiltered process); requested leaves of a file
 * not itself requested each get their own OSPREY_TEST_FILTER run
 * ([TESTING-FILTER]). Excluded items (or leaves of excluded files) drop out.
 */
export function planRun<T extends TestItemLike>(
  requested: readonly T[],
  excluded: ReadonlySet<string> = new Set(),
  isFile: (item: T) => boolean = (item) => item.parent === undefined,
): FilePlan<T>[] {
  const plans = new Map<string, FilePlan<T>>();
  for (const item of requested) {
    if (isExcluded(item, excluded)) {
      continue;
    }
    const file = (isFile(item) ? item : item.parent) as T;
    if (file === undefined) {
      continue;
    }
    const plan = plans.get(file.id) ?? { file, leaves: [], wholeFile: false };
    plans.set(file.id, plan);
    if (isFile(item)) {
      plan.wholeFile = true;
    } else {
      plan.leaves.push(item);
    }
  }
  return [...plans.values()];
}

/**
 * Map one requested leaf onto the TAP results. Absent from the output means
 * skipped (e.g. filtered out, or removed from the file since discovery); a
 * duplicate name resolves to the last matching result.
 */
export function outcomeForLeaf(
  name: string,
  results: readonly TapResult[],
): LeafOutcome {
  // A line whose WHOLE description is this case's name is that case's own
  // result, even when the name contains `# SKIP` — the case ran, and the
  // directive-looking text is just part of its name ([TESTING-TAP-AMBIGUITY]).
  // Only when no line matches verbatim is the directive reading believed.
  const verbatim = results.filter((result) => result.description === name);
  const split = results.filter((result) => result.name === name);
  const matches = verbatim.length > 0 ? verbatim : split;
  const result = matches[matches.length - 1];
  if (result === undefined) {
    return { status: "skipped" };
  }
  if (verbatim.length === 0 && result.skipReason !== undefined) {
    return { status: "skipped", reason: result.skipReason };
  }
  if (result.ok) {
    return { status: "passed" };
  }
  return {
    status: "failed",
    message: result.comments.join("\n") || `Test failed: ${name}`,
  };
}

/**
 * A published skip diagnostic: its message, and whether the skip named no
 * reason and is therefore an error ([TESTING-SKIP-REASON]).
 */
export interface SkipReport {
  readonly message: string;
  readonly unexplained: boolean;
}

/**
 * The Problems-panel diagnostic for a leaf that did not actually run
 * ([TESTING-SKIP-WARNING]): a case the TAP flagged `# SKIP` (reason included
 * when it gave one), or a case that silently never appeared in its run's TAP
 * (filtered out, or removed since discovery — an ignored test either way).
 * `undefined` for a case that ran: no test skips without a diagnostic, and no
 * test that ran carries one.
 *
 * A case that skipped and named NO reason is `unexplained`, reported at Error
 * severity ([TESTING-SKIP-REASON]). A case merely absent from the TAP is not:
 * a filtered run legitimately produces no line for it, so no reason could
 * exist to demand.
 */
export function skipReportFor(
  name: string,
  outcome: LeafOutcome,
): SkipReport | undefined {
  if (outcome.status !== "skipped") {
    return undefined;
  }
  if (outcome.reason === undefined) {
    return {
      message: `Test '${name}' did not run (skipped/ignored)`,
      unexplained: false,
    };
  }
  return outcome.reason === ""
    ? {
        message: `Test '${name}' was skipped with no reason; every skip must name one`,
        unexplained: true,
      }
    : {
        message: `Test '${name}' was skipped: ${outcome.reason}`,
        unexplained: false,
      };
}

/**
 * A non-zero exit with no TAP at all (no results, no plan line) means the file
 * never ran — a compile/type error. A run whose plan printed but produced no
 * results (e.g. a filter matching nothing) is NOT a compile failure.
 */
export function isCompileFailure(exitCode: number, stream: TapStream): boolean {
  return exitCode !== 0 && !stream.sawPlan && stream.results.length === 0;
}

/**
 * The failure message for a run that exited non-zero although no test case
 * reported `not ok` — an assertion OUTSIDE any test failed ([TESTING-TAP]).
 * Returns the collected `#` diagnostics (summary line excluded), else stderr,
 * else a generic message; undefined when the exit code or a `not ok` result
 * already explains the failure. Call after ruling out a compile failure.
 */
export function strayFailureMessage(
  stream: TapStream,
  exitCode: number,
  stderr: string,
): string | undefined {
  if (exitCode === 0 || stream.results.some((result) => !result.ok)) {
    return undefined;
  }
  const diagnostics = [
    ...stream.results.flatMap((result) => result.comments),
    ...stream.strayComments,
  ].filter(
    (comment) =>
      !TAP_SUMMARY.test(comment) && !TEST_RUNNER_CHROME.test(comment),
  );
  return (
    diagnostics.join("\n") ||
    stderr.trim() ||
    `osprey --run exited with code ${exitCode} although every test case passed`
  );
}

/**
 * The child environment for one `--run` invocation ([TESTING-FILTER]): a
 * filtered run sets OSPREY_TEST_FILTER explicitly; an unfiltered run DELETES
 * it so a stray value inherited from the editor's environment cannot silently
 * skip test cases. Never mutates `base`.
 */
export function testRunEnv(
  base: NodeJS.ProcessEnv,
  filter: string | undefined,
): NodeJS.ProcessEnv {
  const env = { ...base };
  if (filter === undefined) {
    delete env.OSPREY_TEST_FILTER;
  } else {
    env.OSPREY_TEST_FILTER = filter;
  }
  return env;
}

/** The message for a run that produced no TAP ([TESTING-EXIT] compile path). */
export function compileFailureMessage(
  stderr: string,
  exitCode: number,
): string {
  const detail = stderr.trim();
  return (
    detail ||
    `osprey --run exited with code ${exitCode} and produced no TAP output`
  );
}

/** Test Explorer output is a pseudoterminal: lines must end in CRLF. */
export function toTerminalOutput(text: string): string {
  return text.replace(/\r?\n/g, "\r\n");
}

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
