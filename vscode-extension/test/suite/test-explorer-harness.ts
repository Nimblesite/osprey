// Shared harness for the Test Explorer integration suite: the Osprey fixture
// sources and a recording TestRunSink that captures run events for assertion.

import * as vscode from "vscode";
import type { TestRunSink } from "../../client/src/test-explorer";

/** Two passing cases ("addition works" line 3, "zero identity" line 7). */
export const PASS_FIXTURE = `fn add(a, b) = a + b

test("addition works", fn() => {
    expect(add(2, 3), 5)
})

test("zero identity", fn() => {
    expect(add(0, 0), 0)
})
`;

/** "bad math" fails (expected 3, got 2), "good math" passes. */
export const FAIL_FIXTURE = `fn add(a, b) = a + b

test("bad math", fn() => {
    expect(add(1, 1), 3)
})

test("good math", fn() => {
    expect(add(1, 1), 2)
})
`;

/** One passing ML-flavor case, "ml addition". */
export const ML_FIXTURE = `add (a, b) = a + b

test "ml addition" (\\() =>
    check "sum" 5 (add (2, 3)))
`;

/**
 * "parked case" reports the `Skip` verdict, "live case" passes
 * ([TESTING-SKIP-WARNING]): a run must raise a Warning diagnostic for the
 * skipped case and none for the live one.
 */
export const SKIP_FIXTURE = `type Verdict = Pass | Fail(string) | Skip(string)

test("parked case", fn() => Skip("blocked on #123"))

test("live case", fn() => expect(1, 1))
`;

/**
 * A skip that names NO reason ([TESTING-SKIP-REASON]): the run must raise an
 * ERROR diagnostic rather than a warning, because a hole in coverage whose
 * cause was never written down cannot be weighed by anyone reading it.
 */
export const UNEXPLAINED_SKIP_FIXTURE = `type Verdict = Pass | Fail(string) | Skip(string)

test("unexplained case", fn() => Skip(""))

test("live case", fn() => expect(1, 1))
`;

/**
 * UNEXPLAINED_SKIP_FIXTURE with the SAME case given a reason: its Error must
 * become a Warning in place ([TESTING-SKIP-REASON]).
 */
export const EXPLAINED_SKIP_FIXTURE = `type Verdict = Pass | Fail(string) | Skip(string)

test("unexplained case", fn() => Skip("blocked on #456"))

test("live case", fn() => expect(1, 1))
`;

/** SKIP_FIXTURE with "parked case" revived — its warning must clear. */
export const SKIP_FIXED_FIXTURE = `type Verdict = Pass | Fail(string) | Skip(string)

test("parked case", fn() => Pass)

test("live case", fn() => expect(1, 1))
`;

/** SKIP_FIXTURE re-parked under a DIFFERENT reason — the message must update. */
export const SKIP_REPARKED_FIXTURE = `type Verdict = Pass | Fail(string) | Skip(string)

test("parked case", fn() => Skip("now blocked on #456"))

test("live case", fn() => expect(1, 1))
`;

/**
 * Every outcome in one suite ([TESTING-TAP], [TESTING-VERDICT]): a pass, a
 * failure, a STATIC skip, and a DYNAMIC skip that only the run can discover
 * (line 3 helper). Exactly the two skips may warn; the pass and the failure
 * must not.
 */
export const MIXED_FIXTURE = `type Verdict = Pass | Fail(string) | Skip(string)

fn guard(n) = match n > 100 {
    true => Pass
    false => Skip("runtime precondition unmet")
}

test("passes cleanly", fn() => expect(2 + 2, 4))

test("fails loudly", fn() => expect(1, 3))

test("statically parked", fn() => Skip("static reason"))

test("dynamically parked", fn() => guard(1))
`;

/**
 * [TESTING-TAP-AMBIGUITY] a PASSING case whose NAME contains `# SKIP`, beside
 * a genuinely skipped one. A naive TAP split reports the passing case as
 * skipped; the discovered names must break the tie.
 */
export const AMBIGUOUS_NAME_FIXTURE = `type Verdict = Pass | Fail(string) | Skip(string)

test("name with # SKIP inside it", fn() => expect(1, 1))

test("really parked", fn() => Skip("genuinely skipped"))
`;

/** The ML twin of SKIP_FIXTURE — layout syntax, same warnings. */
export const ML_SKIP_FIXTURE = `type Verdict = Pass | Fail string | Skip string

test "ml parked case" (\\() => Skip "ml blocked")

test "ml live case" (\\() => Pass)
`;

/** Does not parse — `--list-tests` and `--run` both fail with syntax errors. */
export const BROKEN_FIXTURE = "fn broken( = nonsense !!\n";

/**
 * One passing case plus a failing assertion OUTSIDE any test: TAP is all-ok
 * but the process exits 1 ([TESTING-EXIT]).
 */
export const STRAY_FIXTURE = `fn add(a, b) = a + b

test("fine", fn() => {
    expect(add(1, 1), 2)
})

expect(add(1, 1), 5)
`;

/**
 * A covered `double` (line 1) and a never-called `unused` (line 3): a coverage
 * run must report line 3 with 0 hits ([TESTING-COVERAGE-VSCODE]).
 */
export const COVERAGE_FIXTURE = `fn double(x) = x * 2

fn unused(x) = x * 99

test("doubles", fn() => {
    expect(double(5), 10)
})
`;

/**
 * A documented suite ([TESTING-DOC]). "documented case" (line 22) carries every
 * recognised doc section; "summary only" (line 25) carries a bare summary;
 * "undocumented case" (line 27) carries none. `fn add` is documented too — a
 * declaration's doc must NOT leak onto the cases.
 */
export const DOC_FIXTURE = `/// Adds two integers.
fn add(a, b) = a + b

/// Addition is commutative.
///
/// Swapping the operands cannot change the sum, so both orders agree.
///
/// # Parameters
/// - left: the first addend
/// - right: the second addend
///
/// # Returns
/// Unit, reported through \`expect\`.
///
/// # Raises
/// - Overflow: when the sum leaves int range
///
/// # See also
/// [add]
///
/// # Since
/// 0.3
test("documented case", fn() => expect(add(1, 2), add(2, 1)))

/// Zero is the additive identity.
test("summary only", fn() => expect(add(5, 0), 5))

test("undocumented case", fn() => expect(add(1, 1), 2))
`;

/** A documented case that FAILS — proves docs reach the failure message. */
export const DOC_FAIL_FIXTURE = `fn add(a, b) = a + b

/// Proves the broken invariant.
///
/// # Since
/// 0.9
test("documented failure", fn() => {
    expect(add(1, 1), 3)
})
`;

/** The ML twin of DOC_FIXTURE's first case, using \`(** … *)\` blocks. */
export const ML_DOC_FIXTURE = `add a b = a + b

(** Addition is commutative.

    Swapping the operands cannot change the sum. *)
test "ml documented" (\\() => check "sum" (add 1 2) (add 2 1))

test "ml bare" (\\() => check "bare" 1 1)
`;

/**
 * A busy suite the sampling profiler can collect frames from ([TESTING-PROFILE]).
 *
 * The iteration count is a sampling floor, not an arbitrary number. At 2,000,000
 * this yielded 19 on-CPU samples on a fast dev machine — few enough that the
 * profiler's own report appends "run longer for confidence" — and on a Linux CI
 * runner, whose sampler is a separate SIGPROF path, it yielded none at all, so
 * `presentProfile` refused the empty export with "speedscope file has no
 * profiles". Measured yield scales cleanly: 2M -> 19 samples, 20M -> 210,
 * 60M -> 623. 60,000,000 buys a ~30x margin over the failure point for about
 * half a second of CPU, so a slower or busier runner still lands far from zero.
 */
export const PROFILE_FIXTURE = `fn spin(n, acc) = match n <= 0 {
    true => acc
    false => spin((n - 1) ?: 0, (acc + n) ?: 0)
}

/// Burns enough CPU for the sampling profiler to collect frames.
test("profiled work", fn() => {
    expect(spin(60000000, 0) > 0, true)
})
`;

export interface SinkEvent {
  kind:
    | "enqueued"
    | "started"
    | "passed"
    | "failed"
    | "errored"
    | "skipped"
    | "end";
  id?: string;
  message?: string;
}

type SinkMessage = vscode.TestMessage | readonly vscode.TestMessage[];

function messageText(message: SinkMessage): string {
  const first = (
    Array.isArray(message) ? message[0] : message
  ) as vscode.TestMessage;
  return typeof first.message === "string"
    ? first.message
    : first.message.value;
}

/** A TestRunSink that records every reported event and all appended output. */
export class RecordingSink implements TestRunSink {
  public readonly events: SinkEvent[] = [];
  public output = "";
  /** Coverage reports ([TESTING-COVERAGE-VSCODE]): fsPath → line → hits. */
  public readonly coverage = new Map<string, ReadonlyMap<number, number>>();

  private record(
    kind: SinkEvent["kind"],
    test?: vscode.TestItem,
    message?: SinkMessage,
  ): void {
    this.events.push({
      kind,
      ...(test ? { id: test.id } : {}),
      ...(message ? { message: messageText(message) } : {}),
    });
  }
  public enqueued(test: vscode.TestItem): void {
    this.record("enqueued", test);
  }
  public started(test: vscode.TestItem): void {
    this.record("started", test);
  }
  public passed(test: vscode.TestItem): void {
    this.record("passed", test);
  }
  public failed(test: vscode.TestItem, message: SinkMessage): void {
    this.record("failed", test, message);
  }
  public errored(test: vscode.TestItem, message: SinkMessage): void {
    this.record("errored", test, message);
  }
  public skipped(test: vscode.TestItem): void {
    this.record("skipped", test);
  }
  public appendOutput(output: string): void {
    this.output += output;
  }
  public end(): void {
    this.record("end");
  }
  public addLineCoverage(
    uri: vscode.Uri,
    hits: ReadonlyMap<number, number>,
  ): void {
    this.coverage.set(uri.fsPath, hits);
  }
  public ofKind(kind: SinkEvent["kind"]): SinkEvent[] {
    return this.events.filter((event) => event.kind === kind);
  }
}
