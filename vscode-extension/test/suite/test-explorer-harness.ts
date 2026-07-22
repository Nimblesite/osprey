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
