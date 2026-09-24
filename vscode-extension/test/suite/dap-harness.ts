// Osprey VSIX DAP integration-test harness.
//
// This is the *vendored mirror* of `@nimblesite/lspkit-debug/testing`
// (lsp_toolkit/packages/lspkit-debug) — the language-neutral DAP test harness
// shared with the SharpLsp and Basilisk extensions. It is kept here, importing
// `vscode` directly, only because that package is not yet published to npm; once
// it is, this file is replaced by `import { ... } from
// "@nimblesite/lspkit-debug/testing"`. Until then it is the SINGLE copy of the
// DAP helpers in this extension — every debugger test imports it, none re-derive
// it. [DEBUGGER-REUSE]

import * as assert from "assert";
import * as vscode from "vscode";

/** A source breakpoint: a bare 1-based line, or a line with a hit condition. */
export type BreakpointSpec = number | { line: number; condition?: string };

/** A DAP stack frame (the fields the tests assert on). */
export interface DapStackFrame {
  id: number;
  name: string;
  source?: { path?: string; name?: string };
  line: number;
  column: number;
}

/** A DAP variable/scope entry. */
export interface DapVariable {
  name: string;
  value: string;
  type?: string;
  variablesReference: number;
}

/** A DAP scope. */
export interface DapScope {
  name: string;
  variablesReference: number;
  expensive?: boolean;
}

/** A stopped debuggee: the thread and its current stack. */
export interface DapStop {
  threadId: number;
  stack: { stackFrames: DapStackFrame[] };
}

const DEFAULT_STACK_LEVELS = 20;
const POLL_INTERVAL_MS = 100;

/** Remove every breakpoint VS Code currently holds. */
export function clearDebugBreakpoints(): void {
  vscode.debug.removeBreakpoints(vscode.debug.breakpoints);
}

/** Replace all breakpoints with one per spec on `filePath` (1-based lines). */
export function setSourceBreakpoints(
  filePath: string,
  specs: BreakpointSpec[],
): void {
  clearDebugBreakpoints();
  vscode.debug.addBreakpoints(
    specs.map((spec) => {
      const line = typeof spec === "number" ? spec : spec.line;
      const condition = typeof spec === "number" ? undefined : spec.condition;
      const location = new vscode.Location(
        vscode.Uri.file(filePath),
        new vscode.Position(line - 1, 0),
      );
      return new vscode.SourceBreakpoint(location, true, condition);
    }),
  );
}

/** Resolve with the next started debug session, or reject on timeout. */
export async function waitForDebugSessionStart(
  timeoutMs = 30_000,
): Promise<vscode.DebugSession> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      disposable.dispose();
      reject(new Error(`Debug session did not start within ${timeoutMs}ms`));
    }, timeoutMs);
    const disposable = vscode.debug.onDidStartDebugSession((session) => {
      clearTimeout(timer);
      disposable.dispose();
      resolve(session);
    });
  });
}

/** Resolve once no (matching) session is active, or reject on timeout. */
export async function waitForDebugSessionEnd(
  timeoutMs = 30_000,
  sessionId?: string,
): Promise<void> {
  const started = Date.now();
  while (Date.now() - started < timeoutMs) {
    const active = vscode.debug.activeDebugSession;
    if (!active || (sessionId && active.id !== sessionId)) {
      return;
    }
    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
  }
  throw new Error(`Debug session did not end within ${timeoutMs}ms`);
}

/** The stack trace for a thread. */
export async function getStackTrace(
  session: vscode.DebugSession,
  threadId: number,
): Promise<{ stackFrames: DapStackFrame[] }> {
  return session.customRequest("stackTrace", {
    threadId,
    startFrame: 0,
    levels: DEFAULT_STACK_LEVELS,
  }) as Promise<{ stackFrames: DapStackFrame[] }>;
}

/** The scopes of a stack frame. */
export async function getScopes(
  session: vscode.DebugSession,
  frameId: number,
): Promise<{ scopes: DapScope[] }> {
  return session.customRequest("scopes", { frameId }) as Promise<{
    scopes: DapScope[];
  }>;
}

/** The variables under a variablesReference. */
export async function getVariables(
  session: vscode.DebugSession,
  variablesReference: number,
): Promise<{ variables: DapVariable[] }> {
  return session.customRequest("variables", {
    variablesReference,
  }) as Promise<{ variables: DapVariable[] }>;
}

/** Evaluate an expression in a frame (watch/repl/hover context). */
export async function evaluate(
  session: vscode.DebugSession,
  expression: string,
  frameId: number,
  context: "watch" | "repl" | "hover" = "watch",
): Promise<{ result: string; type?: string; variablesReference: number }> {
  return session.customRequest("evaluate", {
    expression,
    frameId,
    context,
  }) as Promise<{ result: string; type?: string; variablesReference: number }>;
}

/** Step over (DAP `next`). */
export function stepOver(
  session: vscode.DebugSession,
  threadId: number,
): Thenable<unknown> {
  return session.customRequest("next", { threadId });
}

/** Step into (DAP `stepIn`). */
export function stepIn(
  session: vscode.DebugSession,
  threadId: number,
): Thenable<unknown> {
  return session.customRequest("stepIn", { threadId });
}

/** Step out (DAP `stepOut`). */
export function stepOut(
  session: vscode.DebugSession,
  threadId: number,
): Thenable<unknown> {
  return session.customRequest("stepOut", { threadId });
}

/** Resume (DAP `continue`). */
export function continueExecution(
  session: vscode.DebugSession,
  threadId: number,
): Thenable<unknown> {
  return session.customRequest("continue", { threadId });
}

/** Flatten every referenceable scope's variables for a frame. */
export async function readFrameVariables(
  session: vscode.DebugSession,
  frameId: number,
): Promise<DapVariable[]> {
  const { scopes } = await getScopes(session, frameId);
  const groups = await Promise.all(
    scopes
      .filter((scope) => scope.variablesReference > 0)
      .map((scope) => getVariables(session, scope.variablesReference)),
  );
  return groups.flatMap((group) => group.variables);
}

/** Find a named local/argument in a frame, or undefined. */
export async function findVariable(
  session: vscode.DebugSession,
  frameId: number,
  name: string,
): Promise<DapVariable | undefined> {
  const all = await readFrameVariables(session, frameId);
  return all.find((variable) => variable.name === name);
}

/** DAP requests that resume the debuggee. The adapter answers them before the
 * debuggee stops again and sends no `continued` event for them, so the request
 * itself is what marks the debuggee running. */
const RESUME_REQUESTS = new Set([
  "continue",
  "next",
  "stepIn",
  "stepOut",
  "stepBack",
  "reverseContinue",
  "goto",
  "restartFrame",
]);

/** DAP events after which the debuggee is no longer stopped. */
const RUNNING_EVENTS = new Set(["continued", "exited", "terminated"]);

/** Per session id, the thread of the last `stopped` event, held until a resume
 * request or a running event. A session absent from the map is not stopped. */
const stops = new Map<string, { threadId?: number }>();

function trackStops(session: vscode.DebugSession): vscode.DebugAdapterTracker {
  return {
    onWillReceiveMessage: (message: { command?: string }) => {
      if (RESUME_REQUESTS.has(message.command ?? "")) {
        stops.delete(session.id);
      }
    },
    onDidSendMessage: (message: {
      type?: string;
      event?: string;
      body?: { threadId?: number };
    }) => {
      if (message.type !== "event") {
        return;
      }
      if (message.event === "stopped") {
        stops.set(session.id, { threadId: message.body?.threadId });
      } else if (RUNNING_EVENTS.has(message.event ?? "")) {
        stops.delete(session.id);
      }
    },
    onWillStopSession: () => stops.delete(session.id),
  };
}

// Registered when the harness loads, before any test starts a session, so no
// session's `stopped` event is missed.
vscode.debug.registerDebugAdapterTrackerFactory("*", {
  createDebugAdapterTracker: trackStops,
});

/** How long to keep polling for a *source-resolved* top frame before accepting
 * an unresolved one. lldb-dap can briefly report the top frame's source as `.`
 * and its line as 0 on the first `stackTrace` of a fresh session, before the
 * module's line table finishes loading, then correct both. Re-polling within
 * this grace makes source/line assertions robust to that cold start without
 * masking a genuinely unresolved frame. */
const SOURCE_RESOLVE_GRACE_MS = 2_000;

/** A top frame lldb-dap has fully resolved: a real source path (not the cold
 * "." placeholder) on a real 1-based line (not the cold 0). A frame that simply
 * has no source — e.g. the C entry stub a `stopOnEntry` launch pauses in — is
 * not "resolvable", so callers fall back to it once the grace elapses. */
function topFrameResolved(stop: DapStop): boolean {
  const top = stop.stack.stackFrames[0];
  const path = top?.source?.path;
  return path !== undefined && path !== "" && path !== "." && top.line >= 1;
}

async function stoppedThreadIds(
  session: vscode.DebugSession,
  threadId: number | undefined,
): Promise<number[]> {
  if (threadId !== undefined) {
    return [threadId];
  }
  const response = (await session.customRequest("threads")) as {
    threads?: { id: number }[];
  };
  return (response.threads ?? []).map((thread) => thread.id);
}

/** The stack of the thread the adapter last reported `stopped`, or undefined
 * while the debuggee runs. Thread state is never read without that event:
 * between `launch` and resuming, lldb-dap answers `stackTrace` with the loader
 * entry (`ld-linux`'s `_start`, `_dyld_start`) and reports no stop, and right
 * after a step request it still answers with the pre-step frame. */
async function readStop(
  session: vscode.DebugSession,
): Promise<DapStop | undefined> {
  const stopped = stops.get(session.id);
  if (!stopped) {
    return undefined;
  }
  for (const threadId of await stoppedThreadIds(session, stopped.threadId)) {
    const stack = await getStackTrace(session, threadId);
    if (stack.stackFrames.length > 0) {
      return { threadId, stack };
    }
  }
  return undefined;
}

/** Poll until the adapter has reported the debuggee stopped and a stack frame
 * is readable; throws on timeout. Prefers a source-resolved top frame (see
 * {@link SOURCE_RESOLVE_GRACE_MS}) so the cold-start `.`/line-0 transient does
 * not race callers that assert on the frame's source path or line. */
export async function waitForStop(
  session: vscode.DebugSession,
  timeoutMs = 30_000,
): Promise<DapStop> {
  const started = Date.now();
  let lastError = "";
  let unresolved: { stop: DapStop; since: number } | undefined;
  while (Date.now() - started < timeoutMs) {
    try {
      const stop = await readStop(session);
      if (stop && topFrameResolved(stop)) {
        return stop;
      }
      unresolved = stop && { stop, since: unresolved?.since ?? Date.now() };
      if (
        unresolved &&
        Date.now() - unresolved.since >= SOURCE_RESOLVE_GRACE_MS
      ) {
        return unresolved.stop;
      }
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error);
    }
    await new Promise((resolve) => setTimeout(resolve, POLL_INTERVAL_MS));
  }
  if (unresolved) {
    return unresolved.stop;
  }
  throw new Error(
    `Timed out waiting for the debuggee to stop after ${timeoutMs}ms (${lastError})`,
  );
}

/** Accepts a substring, a RegExp, or a predicate to match a rendered value. */
export type ValueMatcher = string | RegExp | ((value: string) => boolean);

function matchValue(value: string, matcher: ValueMatcher): boolean {
  if (typeof matcher === "string") {
    return value.includes(matcher);
  }
  if (matcher instanceof RegExp) {
    return matcher.test(value);
  }
  return matcher(value);
}

/** Assert a named local/argument exists in `frameId` and matches. Returns it. */
export async function assertLocalVariable(
  session: vscode.DebugSession,
  frameId: number,
  name: string,
  matcher: ValueMatcher,
): Promise<DapVariable> {
  const variable = await findVariable(session, frameId, name);
  assert.ok(
    variable,
    `expected a local named "${name}" in frame ${frameId}; saw ${(
      await readFrameVariables(session, frameId)
    )
      .map((v) => v.name)
      .join(", ")}`,
  );
  assert.ok(
    matchValue(variable.value, matcher),
    `local "${name}" = ${JSON.stringify(variable.value)} did not match ${String(matcher)}`,
  );
  return variable;
}

/** Assert an evaluated watch expression matches. Returns the result string. */
export async function assertWatch(
  session: vscode.DebugSession,
  frameId: number,
  expression: string,
  matcher: ValueMatcher,
): Promise<string> {
  const result = await evaluate(session, expression, frameId, "watch");
  assert.ok(
    matchValue(result.result, matcher),
    `watch "${expression}" = ${JSON.stringify(result.result)} did not match ${String(matcher)}`,
  );
  return result.result;
}

/** Assert the top frame is on `expectedLine` (and optionally in `sourcePath`). */
export function assertCurrentLine(
  stack: { stackFrames: DapStackFrame[] },
  expectedLine: number,
  sourcePath?: string,
): DapStackFrame {
  const top = stack.stackFrames[0];
  assert.ok(top, "expected at least one stack frame");
  assert.strictEqual(
    top.line,
    expectedLine,
    `expected to stop on line ${expectedLine}`,
  );
  if (sourcePath !== undefined) {
    const norm = (p: string): string => p.replace(/\\/g, "/").toLowerCase();
    assert.ok(
      top.source?.path && norm(top.source.path) === norm(sourcePath),
      `expected top frame in ${sourcePath}, was ${top.source?.path}`,
    );
  }
  return top;
}
