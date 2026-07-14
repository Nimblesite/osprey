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
}

/** The outcome of parsing a `--list-tests` invocation. */
export type TestListParse =
  | { readonly ok: true; readonly tests: DiscoveredTest[] }
  | { readonly ok: false; readonly error: string };

/** One TAP result line with the `#` diagnostics that preceded it. */
export interface TapResult {
  readonly name: string;
  readonly ok: boolean;
  readonly comments: string[];
}

/** What a run should report for one leaf test ([TESTING-TAP]). */
export type LeafOutcome =
  | { readonly status: "passed" }
  | { readonly status: "failed"; readonly message: string }
  | { readonly status: "skipped" };

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

function isDiscoveredTest(value: unknown): value is DiscoveredTest {
  const record = value as { name?: unknown; line?: unknown; column?: unknown };
  return (
    typeof value === "object" &&
    value !== null &&
    typeof record.name === "string" &&
    typeof record.line === "number" &&
    typeof record.column === "number"
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
    return { ok: false, error: `--list-tests entry is malformed: ${JSON.stringify(bad)}` };
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

// The runtime prints results as exactly `ok N - name` / `not ok N - name`
// ([TESTING-TAP]); the name is everything after "- " to end of line, captured
// byte-exact (leading/trailing whitespace preserved) so it matches
// `--list-tests` names precisely.
const TAP_RESULT = /^(not )?ok \d+ - (.*)$/;
const TAP_COMMENT = /^#\s?(.*)$/;
const TAP_PLAN = /^\d+\.\.\d+$/;
const TAP_SUMMARY = /^tests=\d+ passed=\d+ failed=\d+/;

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
      results.push({ name: result[2], ok: result[1] === undefined, comments });
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

function isExcluded(item: TestItemLike, excluded: ReadonlySet<string>): boolean {
  return excluded.has(item.id) || (item.parent !== undefined && excluded.has(item.parent.id));
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
): FilePlan<T>[] {
  const plans = new Map<string, FilePlan<T>>();
  for (const item of requested) {
    if (isExcluded(item, excluded)) {
      continue;
    }
    const file = (item.parent ?? item) as T;
    const plan = plans.get(file.id) ?? { file, leaves: [], wholeFile: false };
    plans.set(file.id, plan);
    if (item.parent === undefined) {
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
  const matches = results.filter((result) => result.name === name);
  const result = matches[matches.length - 1];
  if (result === undefined) {
    return { status: "skipped" };
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
  ].filter((comment) => !TAP_SUMMARY.test(comment));
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
export function compileFailureMessage(stderr: string, exitCode: number): string {
  const detail = stderr.trim();
  return detail || `osprey --run exited with code ${exitCode} and produced no TAP output`;
}

/** Test Explorer output is a pseudoterminal: lines must end in CRLF. */
export function toTerminalOutput(text: string): string {
  return text.replace(/\r?\n/g, "\r\n");
}
